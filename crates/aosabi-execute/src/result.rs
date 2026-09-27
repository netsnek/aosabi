// SPDX-License-Identifier: MIT
/*
 * aosabi-execute result: the execution result and the key order it is written in.
 *
 * Derived from @graphql-tools/executor 1.5.1, cjs/execution/execute.js, MIT.
 * Derived from graphql-js 16.12.0, src/error/GraphQLError.ts, MIT.
 */

//! The execution result of one request.
//!
//! The executor settles to a JavaScript object, `{ errors?, data? }`, whose
//! values belong to the agent of the request. [`ExecutionResult`] is that
//! object moved out of the agent: the errors as owned `GraphQLError`s and
//! the data in the value model of the writer. It is what a carrier above the
//! executor holds, and [`ExecutionResult::to_js_value`] is the object
//! `JSON.stringify` sees.
//!
//! The data sits behind accessors, so how a result holds it stays inside this
//! module. A result of the executor holds the settled [`Answer`] of the arena
//! of the execution, which [`ExecutionResult::write_json`] writes, and
//! [`ExecutionResult::data`] builds the tree of the data from it once, on its
//! first read, by the rules of [`Value::to_js`]. [`ExecutionResult::data_mut`] and
//! [`ExecutionResult::set_data`] change the data, which then is that tree,
//! [`ExecutionResult::has_data`] answers whether the `data` key is present
//! and [`ExecutionResult::data_is_null`] whether the data is `null`, both
//! without reading the data.
//!
//! With the feature `host-results` a result keeps the members a host set in
//! the order of its object, each a `HostMember`, which a done hook of a plugin
//! of an app of the TypeScript option sets, so the result writes them as Yoga
//! writes that object.
//!
//! The object the executor settles to is a `ResultObject`, which holds the
//! arena of the execution until [`ExecutionResult::from_value`] takes it,
//! and which answers any other reader of its data with the tree of executor
//! values the arena holds.

use core::cell::RefCell;
use std::{
    rc::Rc,
    sync::OnceLock, //
};

#[cfg(feature = "host-results")]
use aosabi_json::JsString;
use aosabi_json::{
    JsObject,
    JsValue,
    writer, //
};
use aosabi_lang::GraphQLError;

use crate::{
    arena::{
        Answer,
        Arena,
        NodeRef, //
    },
    errors::copy_graphql_error,
    value::{
        ErrorObject,
        ObjectLike,
        PlainObject,
        Value, //
    }, //
};

/// The result of one execution, the `ExecutionResult` of the reference.
///
/// Derived from graphql-js 16.12.0, src/execution/execute.ts, MIT.
#[derive(Debug, Default)]
pub struct ExecutionResult {
    /// The errors, in the order the executor handled them.
    pub errors: Vec<GraphQLError>,
    /// The data, which is `None` when the result has no `data` key, as the
    /// result of an early error has none.
    data: Option<ResultData>,
    /// The extensions of the result, which the executor never sets.
    pub extensions: Option<JsObject>,
    /// The members of a result a host set, in the order of the own keys of
    /// its object, and `None` for a result whose members are those
    /// [`ExecutionResult::to_js_value`] writes in its own order.
    #[cfg(feature = "host-results")]
    pub host_members: Option<Vec<HostMember>>,
}

/// A member of a result a host set, which [`ExecutionResult::to_js_value`]
/// writes in the order of the members.
///
/// A done hook of a plugin of an app of the TypeScript option may set a result
/// whose errors are an empty array or null, whose extensions are null, which
/// holds a member of another name or whose members stand in another order than
/// `errors`, `data` and `extensions`, and Yoga writes the object as
/// `JSON.stringify` writes it.
#[cfg(feature = "host-results")]
#[derive(Debug, Clone, PartialEq)]
pub enum HostMember {
    /// The member `errors`, the errors of the result, written where it holds
    /// none as well.
    Errors,
    /// The member `data`, the data of the result.
    Data,
    /// The member `extensions`, the extensions of the result.
    Extensions,
    /// A member of another name, or a member `errors` or `extensions` that
    /// holds null, with the copy JSON reads of its value.
    Other(JsString, JsValue),
}

/// The form the data of a result is held in.
#[derive(Debug)]
enum ResultData {
    /// The data as a tree of the value model of the writer.
    Value(JsValue),
    /// The answer of the arena of the execution with the tree of its data,
    /// boxed so a result stays the size of a result that holds a tree.
    Arena(Box<Settled>),
}

/// The answer of an execution, with the tree of its data once a reader built it.
#[derive(Debug)]
struct Settled {
    /// The answer, which the writer writes.
    answer: Answer,
    /// The tree of the data, built on the first read of [`ExecutionResult::data`].
    ///
    /// Two threads may read the cell at once, so a result is `Sync`.
    tree: OnceLock<JsValue>,
}

impl ExecutionResult {
    /// Returns the result with `errors` and no `data` key, `{ errors }`.
    ///
    /// The result of an early error has that form, and so has the result a
    /// carrier makes of a thrown value or a stall. It has no extensions.
    #[must_use]
    pub fn from_errors(errors: Vec<GraphQLError>) -> Self {
        Self {
            errors,
            data: None,
            extensions: None,
            #[cfg(feature = "host-results")]
            host_members: None,
        }
    }

    /// Returns the result with `errors`, `data` and `extensions`.
    ///
    /// A `data` of `None` is a result with no `data` key.
    #[must_use]
    pub fn new(
        errors: Vec<GraphQLError>,
        data: Option<JsValue>,
        extensions: Option<JsObject>,
    ) -> Self {
        Self {
            errors,
            data: data.map(ResultData::Value),
            extensions,
            #[cfg(feature = "host-results")]
            host_members: None,
        }
    }

    /// Reads `{ errors?, data? }` out of a settled result value.
    ///
    /// `value` is what [`crate::normalized_executor::normalized_executor`]
    /// settles to. An entry of `errors` that is no `GraphQLError` and a value
    /// that is no object have no reading and are left out. The result object
    /// of an execution hands over the answer of its arena, so the data is
    /// written from the arena and no tree of it is built, and the data of any
    /// other object is the value of the JSON model of its member `data`.
    #[must_use]
    pub fn from_value(value: &Value) -> Self {
        let Value::Object(object) = value else {
            return Self::default();
        };
        let errors = match Rc::clone(object).get("errors") {
            Value::List(errors) => errors.iter().filter_map(graphql_error_of).collect(),
            _ => Vec::new(),
        };
        if let Some(answer) = object.take_answer() {
            return Self {
                errors,
                data: Some(ResultData::Arena(Box::new(Settled {
                    answer,
                    tree: OnceLock::new(),
                }))),
                extensions: None,
                #[cfg(feature = "host-results")]
                host_members: None,
            };
        }
        let has_data = object.has_own("data");
        let data = has_data.then(|| Rc::clone(object).get("data").to_js());
        Self::new(errors, data, None)
    }

    /// Returns the data, and `None` when the result has no `data` key.
    ///
    /// The data of an answer is built as the tree of the value model of the
    /// writer on the first read, by the rules of [`Value::to_js`], and kept.
    #[must_use]
    pub fn data(&self) -> Option<&JsValue> {
        match &self.data {
            Some(ResultData::Value(data)) => Some(data),
            Some(ResultData::Arena(settled)) => {
                Some(settled.tree.get_or_init(|| settled.answer.to_js()))
            }
            None => None,
        }
    }

    /// Returns the data to change in place, and `None` when the result has no `data` key.
    ///
    /// The data of an answer becomes its tree, which the result writes from
    /// then on.
    #[must_use]
    pub fn data_mut(&mut self) -> Option<&mut JsValue> {
        if let Some(ResultData::Arena(settled)) = &mut self.data {
            let data = settled
                .tree
                .take()
                .unwrap_or_else(|| settled.answer.to_js());
            self.data = Some(ResultData::Value(data));
        }
        match &mut self.data {
            Some(ResultData::Value(data)) => Some(data),
            _ => None,
        }
    }

    /// Replaces the data with `data`, and `None` removes the `data` key.
    pub fn set_data(&mut self, data: Option<JsValue>) {
        self.data = data.map(ResultData::Value);
    }

    /// Returns whether the result has a `data` key.
    ///
    /// It reads the key alone and never the data.
    #[must_use]
    pub fn has_data(&self) -> bool {
        self.data.is_some()
    }

    /// Returns whether the result has a `data` key whose value is `null`.
    ///
    /// The data of an answer is read by the kind of its node, and no tree of
    /// it is built.
    #[must_use]
    pub fn data_is_null(&self) -> bool {
        match &self.data {
            Some(ResultData::Value(data)) => matches!(data, JsValue::Null),
            Some(ResultData::Arena(settled)) => settled.answer.is_null(),
            None => false,
        }
    }

    /// Returns the object `JSON.stringify` sees.
    ///
    /// The key order is `errors` when there is an error, `data` when the key
    /// is present, then `extensions`, which is the order `buildResponse`
    /// writes the first two in (docs/byte-equality.md row D14). Each error is
    /// the object of `GraphQLError.toJSON`, whose key order is `message`,
    /// `locations`, `path`, `extensions` (row D15). The data of an answer is
    /// a tree built for the object, and any other data is a copy.
    ///
    /// With the feature `host-results` a result a host set with members of
    /// its own writes them in their order instead, its errors where they are
    /// empty as well.
    #[must_use]
    pub fn to_js_value(&self) -> JsValue {
        #[cfg(feature = "host-results")]
        if let Some(members) = &self.host_members {
            return self.host_js_value(members, &self.errors, self.extensions.as_ref());
        }
        let mut result = JsObject::new();
        if !self.errors.is_empty() {
            let errors: Vec<JsValue> = self
                .errors
                .iter()
                .map(|error| JsValue::from(error.to_json()))
                .collect();
            result.insert("errors", JsValue::from(errors));
        }
        match &self.data {
            Some(ResultData::Value(data)) => {
                result.insert("data", data.clone());
            }
            Some(ResultData::Arena(settled)) => {
                result.insert("data", settled.answer.to_js());
            }
            None => {}
        }
        if let Some(extensions) = &self.extensions {
            result.insert("extensions", JsValue::from(extensions.clone()));
        }
        JsValue::from(result)
    }

    /// Returns the object `JSON.stringify` sees of a result a host set with
    /// the members `members`, each in its order, with `errors` and
    /// `extensions` as its errors and its extensions, the data and the
    /// extensions where the result holds them.
    #[cfg(feature = "host-results")]
    fn host_js_value(
        &self,
        members: &[HostMember],
        errors: &[GraphQLError],
        extensions: Option<&JsObject>,
    ) -> JsValue {
        let mut result = JsObject::new();
        for member in members {
            match member {
                HostMember::Errors => {
                    let errors: Vec<JsValue> = errors
                        .iter()
                        .map(|error| JsValue::from(error.to_json()))
                        .collect();
                    result.insert("errors", JsValue::from(errors));
                }
                HostMember::Data => {
                    if let Some(data) = self.data() {
                        result.insert("data", data.clone());
                    }
                }
                HostMember::Extensions => {
                    if let Some(extensions) = extensions {
                        result.insert("extensions", JsValue::from(extensions.clone()));
                    }
                }
                HostMember::Other(key, value) => {
                    result.insert(key.clone(), value.clone());
                }
            }
        }
        JsValue::from(result)
    }

    /// Appends the JSON of the result to `out`, the bytes `JSON.stringify` writes of [`ExecutionResult::to_js_value`].
    pub fn write_json(&self, out: &mut Vec<u8>) {
        self.write_json_with(out, &self.errors, self.extensions.as_ref());
    }

    /// Appends the JSON of the result with `errors` and `extensions` in place of its own to `out`.
    ///
    /// The bytes are those `JSON.stringify` writes of the object
    /// [`ExecutionResult::to_js_value`] makes of such a result: `errors` when
    /// there is one, `data` unless the result has no `data` key or the data is
    /// `undefined`, and `extensions` when given, in that order. The data of
    /// an answer is written from its arena into room reserved for it at
    /// once, and the data of a tree by the writer of `aosabi-json`. With the
    /// feature `host-results` a result a host set with members of its own
    /// writes the object [`ExecutionResult::to_js_value`] makes of it, its
    /// members in their order.
    pub fn write_json_with(
        &self,
        out: &mut Vec<u8>,
        errors: &[GraphQLError],
        extensions: Option<&JsObject>,
    ) {
        #[cfg(feature = "host-results")]
        if let Some(members) = &self.host_members {
            aosabi_json::write(out, &self.host_js_value(members, errors, extensions));
            return;
        }
        if let Some(ResultData::Arena(settled)) = &self.data {
            out.reserve(settled.answer.size_hint());
        }
        out.push(b'{');
        let mut first = true;
        if !errors.is_empty() {
            out.extend_from_slice(b"\"errors\":[");
            for (position, error) in errors.iter().enumerate() {
                if position > 0 {
                    out.push(b',');
                }
                aosabi_json::write(out, &JsValue::from(error.to_json()));
            }
            out.push(b']');
            first = false;
        }
        if let Some(data) = &self.data {
            let mark = out.len();
            if !first {
                out.push(b',');
            }
            out.extend_from_slice(b"\"data\":");
            let written = match data {
                ResultData::Value(data) => aosabi_json::write(out, data).is_some(),
                ResultData::Arena(settled) => settled.answer.write(out),
            };
            if written {
                first = false;
            } else {
                out.truncate(mark);
            }
        }
        if let Some(extensions) = extensions {
            if !first {
                out.push(b',');
            }
            out.extend_from_slice(b"\"extensions\":");
            writer::write_object(out, extensions);
        }
        out.push(b'}');
    }

    /// Returns the result as the object of the agent that [`ExecutionResult::from_value`] reads back.
    ///
    /// The object is `{ errors?, data?, extensions? }`, with the keys in the
    /// order of `buildResponse` and the extensions last. The data of an
    /// answer stays the answer, so a result of a stream that passes a hook
    /// is written from its arena again, and any other data is the executor
    /// value of its tree.
    #[must_use]
    pub fn into_value(self) -> Value {
        let errors = errors_value(self.errors);
        let extensions = self
            .extensions
            .map(|extensions| Value::from_js(&JsValue::from(extensions)));
        let data = match self.data {
            Some(ResultData::Arena(settled)) => {
                let Settled { answer, .. } = *settled;
                return Value::Object(Rc::new(ResultObject {
                    errors,
                    extensions,
                    data: RefCell::new(Data::Settled(answer)),
                }));
            }
            Some(ResultData::Value(data)) => Some(Value::from_js(&data)),
            None => None,
        };
        let mut entries = Vec::new();
        if let Some(errors) = errors {
            entries.push((String::from("errors"), errors));
        }
        if let Some(data) = data {
            entries.push((String::from("data"), data));
        }
        if let Some(extensions) = extensions {
            entries.push((String::from("extensions"), extensions));
        }
        Value::from(PlainObject::from_entries(entries))
    }
}

/// Returns the list of `errors` as executor values, and `None` for no error.
pub(crate) fn errors_value(errors: Vec<GraphQLError>) -> Option<Value> {
    (!errors.is_empty()).then(|| {
        let errors: Vec<Value> = errors
            .into_iter()
            .map(|error| Value::from(ErrorObject::GraphQL(error)))
            .collect();
        Value::from(errors)
    })
}

/// Returns a copy of the `GraphQLError` `value` holds, and `None` for any other value.
fn graphql_error_of(value: &Value) -> Option<GraphQLError> {
    let Value::Error(error) = value else {
        return None;
    };
    match error.as_ref() {
        ErrorObject::GraphQL(error) => Some(copy_graphql_error(error)),
        _ => None,
    }
}

/// The object an execution settles to, `{ errors?, data }`, whose data stays in the arena of the execution.
///
/// [`ExecutionResult::from_value`] takes the arena as the settled answer, so
/// the data is written from it. Any other reader of the member `data` meets
/// the tree of executor values the arena holds, built on its first read and
/// kept, which the reader may change, and the object answers the tree from
/// then on and hands over no answer. A result made again by
/// [`ExecutionResult::into_value`] holds its answer the same way, with the
/// extensions of the result after the data.
pub(crate) struct ResultObject {
    /// The errors, a list of `GraphQLError`s, when there is one.
    errors: Option<Value>,
    /// The extensions of a result made again, none for the result of an execution.
    extensions: Option<Value>,
    /// The data.
    data: RefCell<Data>,
}

/// Where a [`ResultObject`] holds its data.
enum Data {
    /// The arena of the execution, with the node of the data.
    Live(Arena, NodeRef),
    /// The settled answer of a result made again.
    Settled(Answer),
    /// The tree a reader of the member `data` read.
    Tree(Value),
    /// Nothing, after a result took the answer, which leaves the member `undefined`.
    Taken,
}

impl ResultObject {
    /// Returns the result object of an execution with `errors` and the data of the node `root` of `arena`.
    pub(crate) fn of_execution(errors: Option<Value>, arena: Arena, root: NodeRef) -> Self {
        Self {
            errors,
            extensions: None,
            data: RefCell::new(Data::Live(arena, root)),
        }
    }

    /// Returns the member `data` as a tree of executor values, built on the first read.
    fn tree(&self) -> Value {
        let mut data = self.data.borrow_mut();
        let tree = match &*data {
            Data::Live(arena, root) => arena.to_value(*root),
            Data::Settled(answer) => answer.to_value(),
            Data::Tree(tree) => return tree.clone(),
            Data::Taken => return Value::Undefined,
        };
        *data = Data::Tree(tree.clone());
        tree
    }
}

impl ObjectLike for ResultObject {
    fn get(self: Rc<Self>, key: &str) -> Value {
        match key {
            "errors" => self.errors.clone().unwrap_or(Value::Undefined),
            "data" => self.tree(),
            "extensions" => self.extensions.clone().unwrap_or(Value::Undefined),
            _ => Value::Undefined,
        }
    }

    /// Returns `errors` when there is an error, `data`, and `extensions` when the result has them.
    fn own_keys(&self) -> Vec<String> {
        let mut keys = Vec::with_capacity(3);
        if self.errors.is_some() {
            keys.push(String::from("errors"));
        }
        keys.push(String::from("data"));
        if self.extensions.is_some() {
            keys.push(String::from("extensions"));
        }
        keys
    }

    fn has_own(&self, key: &str) -> bool {
        match key {
            "errors" => self.errors.is_some(),
            "data" => true,
            "extensions" => self.extensions.is_some(),
            _ => false,
        }
    }

    /// Hands over the answer once, and `None` once a reader read the data as a tree.
    fn take_answer(&self) -> Option<Answer> {
        let mut data = self.data.borrow_mut();
        match core::mem::replace(&mut *data, Data::Taken) {
            Data::Live(arena, root) => Some(arena.into_answer(root)),
            Data::Settled(answer) => Some(answer),
            kept => {
                *data = kept;
                None
            }
        }
    }
}
