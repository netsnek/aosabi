// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool execute table: the behaviour table, which is a schema text
 * and what every resolver of it does, read out of its JSON form.
 *
 * No analogue in the reference. The grammar is the one of
 * tests/sidecar/execute.md, which the node side interprets with real promises
 * over the same JSON text.
 */

//! The behaviour table.
//!
//! A table is one JSON object with the keys `version`, `name`, `mode`, `sdl`
//! and `resolvers`. `resolvers` holds a [`Spec`] per type name and field name,
//! and a spec is an object whose key `do` names what it makes. [`Table::from_js`]
//! reads the whole grammar at once, so a table that loads is one the
//! interpreter answers without a refusal of its own.
//!
//! A table that breaks the grammar is a mistake of the harness and not an
//! answer of the crate under test, so every refusal here is a text that ends
//! up as a `fatal`. The text opens with the JSON pointer of the place that is
//! wrong.
//!
//! Every key of a production is known. A key the grammar does not name is
//! refused, because a misspelt `units` would otherwise read as the default
//! and hide the mistake behind a byte difference.

use std::sync::Arc;

use aosabi_json::{
    JsObject,
    JsString,
    JsValue, //
};
use aosabi_type::ObjMap;

/// The version of the grammar this reader reads.
const VERSION: f64 = 1.0;

/// The three root types a table of [`Mode::Service`] may name.
const ROOT_TYPES: [&str; 3] = ["Query", "Mutation", "Subscription"];

/// The kinds a table of [`Mode::Service`] holds only inside the `returns` of a `fn`.
///
/// A root object is made once when a table loads on the node side, and on
/// this side once when the table loads and once more by the first call of a
/// field of the root in the thread, and the two are the same value only while
/// making it starts no timer, makes no promise, makes no stream, publishes
/// nothing and reads no argument.
const CALL_ONLY_KINDS: [&str; 9] = [
    "after",
    "resolved",
    "rejected",
    "throw",
    "arg",
    "repeater",
    "not_iterable",
    "pubsub_topic",
    "pubsub_publish",
];

/// The kinds a table of [`Mode::Service`] writes as a member of `Subscription` itself.
///
/// aosabi-core binds a member of `Subscription` as the subscribe resolver, so such a
/// member reads as a `fn` whose `returns` is the spec, and a stream is made on
/// every call and never once when the table loads.
const SUBSCRIPTION_SOURCES: [&str; 3] = ["repeater", "not_iterable", "pubsub_topic"];

/// The largest integer a double carries exactly, `Number.MAX_SAFE_INTEGER`.
const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

/// How the resolvers of a table enter the schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// A spec is the body of a field resolver, and no scalar is bound.
    Plain,
    /// The mode `pylon` of the table format, which the compat parity corpora
    /// carry. A spec is a member of a root object of a service of
    /// aosabi-core, which goes through the resolver wrapping, and the seven
    /// scalars aosabi-core adds are bound.
    Service,
}

/// A behaviour table.
#[derive(Debug)]
pub struct Table {
    /// The name requests refer to.
    pub name: String,
    /// How the resolvers enter the schema.
    pub mode: Mode,
    /// The schema text.
    pub sdl: String,
    /// The specs, by type name and then by field name, in the order of the text.
    pub resolvers: ObjMap<ObjMap<Arc<Spec>>>,
}

/// One spec: what it makes and the label its trace events carry.
#[derive(Debug)]
pub struct Spec {
    /// The key `label`, or the JSON pointer of the spec inside the table.
    pub label: String,
    /// What the spec makes.
    pub kind: SpecKind,
}

/// What a spec makes, by its key `do`.
#[derive(Debug)]
pub enum SpecKind {
    /// `value`: the JSON data, deep.
    Value(JsValue),
    /// `undefined`.
    Undefined,
    /// `number`: a number JSON cannot spell.
    Number(f64),
    /// `date`: `new Date(iso)`, and `new Date(NaN)` for `None`.
    Date(Option<String>),
    /// `object`: the members in the order of the text, `__typename` first
    /// when the spec carries a `typename`.
    Object(Vec<(String, Arc<Spec>)>),
    /// `list`: the items, which may be pending promises.
    List(Vec<Arc<Spec>>),
    /// `fn`: a function that makes its `returns` on every call.
    Fn(Arc<Spec>),
    /// `arg`: the argument of the running call.
    Arg {
        /// The name [`Mode::Plain`] reads the argument object by.
        name: String,
        /// The position [`Mode::Service`] reads the argument list at.
        index: usize,
    },
    /// `resolved`: `Promise.resolve(value)`.
    Resolved(Arc<Spec>),
    /// `rejected`: `Promise.reject(error)`.
    Rejected(Throw),
    /// `after`: a promise that settles when a timer of `units` fires.
    After {
        /// The delay of the timer in timer units.
        units: u64,
        /// What the promise settles with.
        outcome: Outcome,
    },
    /// `throw`: a synchronous throw.
    Throw(Throw),
    /// `error_value`: the error instance, returned and not thrown.
    ErrorValue(Throw),
    /// `repeater`: a `Repeater` whose executor runs the steps in order.
    Repeater(Arc<[RepeaterStep]>),
    /// `not_iterable`: the value itself, returned from a subscribe resolver.
    NotIterable(Arc<Spec>),
    /// `pubsub_topic`: `pubSub.subscribe(name)`, or `pubSub.subscribe(name,
    /// key)`, on the pub sub of the request.
    PubSubTopic {
        /// The routing key.
        name: String,
        /// The spec of the id, when the topic has one.
        key: Option<Arc<Spec>>,
    },
    /// `pubsub_publish`: `pubSub.publish(name, payload)`, or
    /// `pubSub.publish(name, key, payload)`, then the value of `then`.
    PubSubPublish {
        /// The routing key.
        name: String,
        /// The spec of the id, when the topic has one.
        key: Option<Arc<Spec>>,
        /// The spec of the payload.
        payload: Arc<Spec>,
        /// The spec of the value the spec makes after the publish.
        then: Arc<Spec>,
    },
}

/// One step of the executor of a `repeater` spec.
#[derive(Debug)]
pub enum RepeaterStep {
    /// `push(value)`, the value made when the step runs, awaited unless
    /// `awaited` is false.
    Push {
        /// The spec of the value.
        value: Arc<Spec>,
        /// Whether the executor awaits what `push` answers.
        awaited: bool,
    },
    /// Awaits a timer of the units on the wheel of the request.
    Sleep(u64),
    /// `stop()`, or `stop(error)` with the thrown value of the throw.
    Stop(Option<Throw>),
    /// Throws the value out of the executor.
    Throw(Throw),
}

/// What the promise of an `after` spec settles with.
#[derive(Debug)]
pub enum Outcome {
    /// The key `with`: the promise fulfils with what the spec makes.
    With(Arc<Spec>),
    /// The key `error`: the promise rejects with the thrown value.
    Error(Throw),
}

/// A thrown value, by its key `kind`.
#[derive(Debug)]
pub enum Throw {
    /// `plain`: `new Error(message)`.
    Plain {
        /// The message.
        message: JsString,
    },
    /// `service`: `new ServiceError(message, {code, statusCode, details})`.
    Service {
        /// The message.
        message: JsString,
        /// The member `code` of the extensions.
        code: String,
        /// The member `statusCode` of the extensions.
        status_code: f64,
        /// The member `details`, left out of the extensions when absent.
        details: Option<JsObject>,
    },
    /// `graphql`: `new GraphQLError(message, {extensions})`.
    GraphQL {
        /// The message.
        message: JsString,
        /// The extensions, when the throw carries any.
        extensions: Option<JsObject>,
    },
    /// `value`: a value that is no error, made by a spec of kind `value`,
    /// `undefined` or `number`.
    Value(Arc<Spec>),
    /// `aggregate`: `new AggregateError(errors, message)`.
    Aggregate {
        /// The message.
        message: JsString,
        /// The inner errors in order.
        errors: Vec<Self>,
    },
}

/// Where a spec sits, which decides the kinds it may be.
#[derive(Debug, Clone, Copy)]
struct Place {
    /// The mode of the table.
    mode: Mode,
    /// Whether the spec sits inside the `returns` of a `fn`.
    inside_call: bool,
    /// Whether the spec is a whole resolver body or the whole `returns` of a
    /// `fn`, which are the two places a `throw` may stand.
    whole_body: bool,
}

impl Place {
    /// Returns the place of a spec nested inside one at this place.
    const fn nested(self) -> Self {
        Self {
            whole_body: false,
            ..self
        }
    }

    /// Returns the place of the `returns` of a `fn` at this place.
    const fn returns(self) -> Self {
        Self {
            inside_call: true,
            whole_body: true,
            ..self
        }
    }

    /// Checks that a spec of the kind `kind` may stand here.
    fn admit(self, kind: &str) -> Result<(), &'static str> {
        if kind == "throw" && !self.whole_body {
            return Err(
                "a throw is valid as a whole resolver body and as the whole returns of a fn",
            );
        }
        let call_only = CALL_ONLY_KINDS.contains(&kind);
        if self.mode == Mode::Service && !self.inside_call && call_only {
            return Err("a service table holds this kind only inside the returns of a fn");
        }
        Ok(())
    }
}

/// One object of the grammar and the JSON pointer it sits at.
struct Node<'t> {
    /// The object.
    object: &'t JsObject,
    /// The JSON pointer of the object inside the table.
    pointer: &'t str,
}

impl<'t> Node<'t> {
    /// Returns `value` as a node.
    fn new(value: &'t JsValue, pointer: &'t str) -> Result<Self, String> {
        match value {
            JsValue::Object(object) => Ok(Self { object, pointer }),
            _ => Err(Self::not_an_object(pointer)),
        }
    }

    /// Returns the refusal of a value at `pointer` that is no object.
    fn not_an_object(pointer: &str) -> String {
        if pointer.is_empty() {
            return String::from("the table itself: not an object");
        }
        format!("{pointer}: not an object")
    }

    /// Returns the refusal of this node for `reason`.
    ///
    /// The pointer of the whole table is the empty string, which a reader of a
    /// refusal would take for a missing word, so the table itself is named in
    /// words.
    fn refuse(&self, reason: &str) -> String {
        if self.pointer.is_empty() {
            return format!("the table itself: {reason}");
        }
        format!("{}: {reason}", self.pointer)
    }

    /// Returns the pointer of the child `key`.
    ///
    /// No key of the grammar holds `~` or `/`, so no key needs the escapes of
    /// RFC 6901 section 3.
    fn child(&self, key: &str) -> String {
        format!("{}/{key}", self.pointer)
    }

    /// Checks that the node holds no key beside `known`.
    fn only(&self, known: &[&str]) -> Result<(), String> {
        let unknown = self
            .object
            .iter()
            .find(|(key, _)| !known.iter().any(|name| **key == **name));
        match unknown {
            Some((key, _)) => Err(self.refuse(&format!("unknown key {key:?}"))),
            None => Ok(()),
        }
    }

    /// Returns the value under `key`.
    fn required(&self, key: &str) -> Result<&'t JsValue, String> {
        self.object
            .get(key)
            .ok_or_else(|| self.refuse(&format!("no key {key}")))
    }

    /// Returns the string under `key` with every code unit it carries.
    fn js_string(&self, key: &str) -> Result<&'t JsString, String> {
        match self.required(key)? {
            JsValue::String(string) => Ok(string),
            _ => Err(self.refuse(&format!("{key} is no string"))),
        }
    }

    /// Returns the string under `key` as text a Rust string holds.
    fn text(&self, key: &str) -> Result<String, String> {
        text_of(self.js_string(key)?)
            .ok_or_else(|| self.refuse(&format!("{key} carries a lone surrogate")))
    }

    /// Returns the object under `key`, `None` when the key is absent.
    fn optional_object(&self, key: &str) -> Result<Option<&'t JsObject>, String> {
        match self.object.get(key) {
            None => Ok(None),
            Some(JsValue::Object(object)) => Ok(Some(object)),
            Some(_) => Err(self.refuse(&format!("{key} is no object"))),
        }
    }

    /// Returns the object under `key`.
    fn object(&self, key: &str) -> Result<&'t JsObject, String> {
        self.optional_object(key)?
            .ok_or_else(|| self.refuse(&format!("no key {key}")))
    }

    /// Returns the array under `key`.
    fn array(&self, key: &str) -> Result<&'t [JsValue], String> {
        match self.required(key)? {
            JsValue::Array(items) => Ok(items),
            _ => Err(self.refuse(&format!("{key} is no array"))),
        }
    }

    /// Returns the number under `key`.
    fn number(&self, key: &str) -> Result<f64, String> {
        match self.required(key)? {
            JsValue::Number(number) => Ok(*number),
            _ => Err(self.refuse(&format!("{key} is no number"))),
        }
    }

    /// Returns the integer from 0 under `key`.
    fn count(&self, key: &str) -> Result<u64, String> {
        let number = self.number(key)?;
        let whole = number.fract() == 0.0 && (0.0..=MAX_SAFE_INTEGER).contains(&number);
        if !whole {
            return Err(self.refuse(&format!("{key} is no integer from 0")));
        }
        // The cast is exact, because the value is a whole number inside the
        // range a double carries without a gap.
        #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Ok(number as u64)
    }
}

/// Returns `string` as text, `None` when it carries a lone surrogate.
fn text_of(string: &JsString) -> Option<String> {
    string.as_str().map(str::to_owned)
}

impl Table {
    /// Reads a table out of its JSON form.
    ///
    /// # Errors
    ///
    /// Returns the text of the refusal for a value that breaks the grammar,
    /// opened by the JSON pointer of the place that is wrong.
    pub fn from_js(value: &JsValue) -> Result<Self, String> {
        let node = Node::new(value, "")?;
        node.only(&["version", "name", "mode", "sdl", "resolvers"])?;
        if node.number("version")? != VERSION {
            return Err(node.refuse("version is not 1"));
        }
        let mode = match node.text("mode")?.as_str() {
            "plain" => Mode::Plain,
            "pylon" => Mode::Service,
            _ => return Err(node.refuse("mode is neither plain nor pylon")),
        };
        Ok(Self {
            name: node.text("name")?,
            mode,
            sdl: node.text("sdl")?,
            resolvers: resolvers(&node, mode)?,
        })
    }
}

/// Reads the key `resolvers` of the table `node`.
fn resolvers(node: &Node<'_>, mode: Mode) -> Result<ObjMap<ObjMap<Arc<Spec>>>, String> {
    let pointer = node.child("resolvers");
    let mut types = ObjMap::default();
    for (type_name, fields) in node.object("resolvers")? {
        let type_name = text_of(type_name)
            .ok_or_else(|| format!("{pointer}: a type name carries a lone surrogate"))?;
        let pointer = format!("{pointer}/{type_name}");
        if mode == Mode::Service && !ROOT_TYPES.contains(&type_name.as_str()) {
            return Err(format!("{pointer}: a service table names root types only"));
        }
        let subscription = type_name == "Subscription";
        let fields = fields_of(&Node::new(fields, &pointer)?, mode, subscription)?;
        types.insert(type_name, fields);
    }
    Ok(types)
}

/// Reads the specs of one type, by field name.
///
/// A key that opens with two underscores is refused. The resolver map of the
/// reference reads such a key as `__resolveType` or `__isTypeOf` and never as
/// a field, and an abstract value of a table names its type through the
/// member `__typename` alone. `subscription` says that the type is
/// `Subscription`, whose members of a [`Mode::Service`] table read as
/// [`member`] says.
fn fields_of(node: &Node<'_>, mode: Mode, subscription: bool) -> Result<ObjMap<Arc<Spec>>, String> {
    let place = Place {
        mode,
        inside_call: false,
        whole_body: mode == Mode::Plain,
    };
    let mut fields = ObjMap::default();
    for (field_name, value) in node.object {
        let field_name = text_of(field_name)
            .ok_or_else(|| node.refuse("a field name carries a lone surrogate"))?;
        if field_name.starts_with("__") {
            return Err(node.refuse("a field name opens with two underscores"));
        }
        let pointer = node.child(&field_name);
        let spec = if subscription {
            member(value, &pointer, place)?
        } else {
            spec(value, &pointer, place)?
        };
        fields.insert(field_name, Arc::new(spec));
    }
    Ok(fields)
}

/// Reads one member of `Subscription`.
///
/// In a [`Mode::Service`] table a member whose kind makes a stream reads as a
/// `fn` that returns it, the member aosabi-core wraps as the subscribe resolver,
/// and the `fn` and the spec it returns carry one label. Any other member
/// reads as a spec.
fn member(value: &JsValue, pointer: &str, place: Place) -> Result<Spec, String> {
    let node = Node::new(value, pointer)?;
    let kind = node.text("do")?;
    if place.mode != Mode::Service || !SUBSCRIPTION_SOURCES.contains(&kind.as_str()) {
        return spec(value, pointer, place);
    }
    let returns = spec(value, pointer, place.returns())?;
    Ok(Spec {
        label: returns.label.clone(),
        kind: SpecKind::Fn(Arc::new(returns)),
    })
}

/// Reads one spec.
fn spec(value: &JsValue, pointer: &str, place: Place) -> Result<Spec, String> {
    let node = Node::new(value, pointer)?;
    let kind = node.text("do")?;
    place.admit(&kind).map_err(|reason| node.refuse(reason))?;
    let label = match node.object.get("label") {
        None => pointer.to_owned(),
        Some(_) => node.text("label")?,
    };
    let kind = spec_kind(&node, &kind, place)?;
    Ok(Spec { label, kind })
}

/// Reads what the spec `node` of the kind `kind` makes.
fn spec_kind(node: &Node<'_>, kind: &str, place: Place) -> Result<SpecKind, String> {
    match kind {
        "value" => {
            node.only(&["do", "label", "value"])?;
            Ok(SpecKind::Value(node.required("value")?.clone()))
        }
        "undefined" => {
            node.only(&["do", "label"])?;
            Ok(SpecKind::Undefined)
        }
        "number" => number(node),
        "date" => date(node),
        "object" => object(node, place.nested()),
        "list" => list(node, place.nested()),
        "fn" => {
            node.only(&["do", "label", "returns"])?;
            nested(node, "returns", place.returns()).map(SpecKind::Fn)
        }
        "arg" => arg(node),
        "resolved" => {
            node.only(&["do", "label", "with"])?;
            nested(node, "with", place.nested()).map(SpecKind::Resolved)
        }
        "rejected" => error_of(node, place).map(SpecKind::Rejected),
        "after" => after(node, place.nested()),
        "throw" => error_of(node, place).map(SpecKind::Throw),
        "error_value" => error_value(node, place),
        "repeater" => repeater(node, place),
        "not_iterable" => {
            node.only(&["do", "label", "value"])?;
            nested(node, "value", place.nested()).map(SpecKind::NotIterable)
        }
        "pubsub_topic" => {
            node.only(&["do", "label", "name", "key"])?;
            Ok(SpecKind::PubSubTopic {
                name: node.text("name")?,
                key: optional(node, "key", place.nested())?,
            })
        }
        "pubsub_publish" => {
            node.only(&["do", "label", "name", "key", "payload", "then"])?;
            Ok(SpecKind::PubSubPublish {
                name: node.text("name")?,
                key: optional(node, "key", place.nested())?,
                payload: nested(node, "payload", place.nested())?,
                then: nested(node, "then", place.nested())?,
            })
        }
        _ => Err(node.refuse(&format!("unknown do {kind:?}"))),
    }
}

/// Reads the spec under `key` of `node` when the key is there.
fn optional(node: &Node<'_>, key: &str, place: Place) -> Result<Option<Arc<Spec>>, String> {
    if !node.object.contains_key(key) {
        return Ok(None);
    }
    nested(node, key, place).map(Some)
}

/// Reads a `repeater` spec, whose steps run in the executor of a `Repeater`.
///
/// A step carries exactly one of `push`, `sleep`, `stop` and `throw`, and a
/// push may carry `await`, true when it is absent. The value of a push sits
/// inside the call of the executor, so it may be any kind a `fn` returns but
/// a `throw`, and `stop` is `null` or a throw.
fn repeater(node: &Node<'_>, place: Place) -> Result<SpecKind, String> {
    node.only(&["do", "label", "steps"])?;
    let pointer = node.child("steps");
    let inside = Place {
        inside_call: true,
        whole_body: false,
        ..place
    };
    let mut steps = Vec::new();
    for (index, value) in node.array("steps")?.iter().enumerate() {
        let pointer = format!("{pointer}/{index}");
        steps.push(repeater_step(&Node::new(value, &pointer)?, inside)?);
    }
    Ok(SpecKind::Repeater(steps.into()))
}

/// Reads one step of a `repeater` spec.
fn repeater_step(step: &Node<'_>, place: Place) -> Result<RepeaterStep, String> {
    if step.object.contains_key("push") {
        step.only(&["push", "await"])?;
        let awaited = match step.object.get("await") {
            None => true,
            Some(JsValue::Bool(awaited)) => *awaited,
            Some(_) => return Err(step.refuse("await is no boolean")),
        };
        let value = nested(step, "push", place)?;
        return Ok(RepeaterStep::Push { value, awaited });
    }
    if step.object.len() != 1 {
        return Err(step.refuse("a step carries exactly one of push, sleep, stop and throw"));
    }
    if step.object.contains_key("sleep") {
        return step.count("sleep").map(RepeaterStep::Sleep);
    }
    if let Some(stop) = step.object.get("stop") {
        if matches!(stop, JsValue::Null) {
            return Ok(RepeaterStep::Stop(None));
        }
        let error = throw(stop, &step.child("stop"), place)?;
        return Ok(RepeaterStep::Stop(Some(error)));
    }
    if let Some(error) = step.object.get("throw") {
        return throw(error, &step.child("throw"), place).map(RepeaterStep::Throw);
    }
    Err(step.refuse("a step carries exactly one of push, sleep, stop and throw"))
}

/// Reads the spec under `key` of `node`, which sits at `place`.
fn nested(node: &Node<'_>, key: &str, place: Place) -> Result<Arc<Spec>, String> {
    spec(node.required(key)?, &node.child(key), place).map(Arc::new)
}

/// Reads a `number` spec, whose `text` names a number JSON cannot spell.
fn number(node: &Node<'_>) -> Result<SpecKind, String> {
    node.only(&["do", "label", "text"])?;
    let number = match node.text("text")?.as_str() {
        "NaN" => f64::NAN,
        "Infinity" => f64::INFINITY,
        "-Infinity" => f64::NEG_INFINITY,
        "-0" => -0.0,
        _ => return Err(node.refuse("text is none of NaN, Infinity, -Infinity and -0")),
    };
    Ok(SpecKind::Number(number))
}

/// Reads a `date` spec, whose `iso` is a string or null.
fn date(node: &Node<'_>) -> Result<SpecKind, String> {
    node.only(&["do", "label", "iso"])?;
    match node.required("iso")? {
        JsValue::Null => Ok(SpecKind::Date(None)),
        JsValue::String(_) => node.text("iso").map(|iso| SpecKind::Date(Some(iso))),
        _ => Err(node.refuse("iso is neither a string nor null")),
    }
}

/// Reads an `object` spec.
///
/// A member key is a GraphQL name. A key that is an array index is no name,
/// and a JavaScript object would move it in front of the other members. A
/// `typename` beside a member `__typename` is refused, because the two would
/// name one property twice.
fn object(node: &Node<'_>, place: Place) -> Result<SpecKind, String> {
    node.only(&["do", "label", "typename", "members"])?;
    let mut members = Vec::new();
    if node.object.contains_key("typename") {
        let typename = Spec {
            label: node.child("typename"),
            kind: SpecKind::Value(JsValue::String(node.js_string("typename")?.clone())),
        };
        members.push((String::from("__typename"), Arc::new(typename)));
    }
    let pointer = node.child("members");
    for (key, value) in node.object("members")? {
        let key = text_of(key).filter(|key| is_name(key));
        let key = key.ok_or_else(|| format!("{pointer}: a member key is no GraphQL name"))?;
        if members.first().is_some_and(|(first, _)| *first == key) {
            return Err(format!("{pointer}: __typename stands beside a typename"));
        }
        let spec = spec(value, &format!("{pointer}/{key}"), place)?;
        members.push((key, Arc::new(spec)));
    }
    Ok(SpecKind::Object(members))
}

/// Returns whether `text` is a GraphQL name.
///
/// The grammar is `Name` of the specification, a letter or an underscore and
/// then letters, digits and underscores.
fn is_name(text: &str) -> bool {
    let mut bytes = text.bytes();
    let opens = bytes
        .next()
        .is_some_and(|byte| byte == b'_' || byte.is_ascii_alphabetic());
    opens && bytes.all(|byte| byte == b'_' || byte.is_ascii_alphanumeric())
}

/// Reads a `list` spec.
fn list(node: &Node<'_>, place: Place) -> Result<SpecKind, String> {
    node.only(&["do", "label", "items"])?;
    let pointer = node.child("items");
    let items = node.array("items")?.iter().enumerate();
    let items = items.map(|(index, item)| spec(item, &format!("{pointer}/{index}"), place));
    let items: Result<Vec<Spec>, String> = items.collect();
    Ok(SpecKind::List(items?.into_iter().map(Arc::new).collect()))
}

/// Reads an `arg` spec, which carries the name and the position of one argument.
fn arg(node: &Node<'_>) -> Result<SpecKind, String> {
    node.only(&["do", "label", "name", "index"])?;
    let index = usize::try_from(node.count("index")?);
    Ok(SpecKind::Arg {
        name: node.text("name")?,
        index: index.map_err(|_| node.refuse("index is out of range"))?,
    })
}

/// Reads an `after` spec, which carries exactly one of `with` and `error`.
fn after(node: &Node<'_>, place: Place) -> Result<SpecKind, String> {
    node.only(&["do", "label", "units", "with", "error"])?;
    let units = node.count("units")?;
    let settles = (
        node.object.contains_key("with"),
        node.object.contains_key("error"),
    );
    let outcome = match settles {
        (true, false) => Outcome::With(nested(node, "with", place)?),
        (false, true) => {
            Outcome::Error(throw(node.required("error")?, &node.child("error"), place)?)
        }
        _ => return Err(node.refuse("after carries exactly one of with and error")),
    };
    Ok(SpecKind::After { units, outcome })
}

/// Reads the `error` of a `rejected` or a `throw` spec.
fn error_of(node: &Node<'_>, place: Place) -> Result<Throw, String> {
    node.only(&["do", "label", "error"])?;
    throw(
        node.required("error")?,
        &node.child("error"),
        place.nested(),
    )
}

/// Reads an `error_value` spec, whose error is an error instance.
fn error_value(node: &Node<'_>, place: Place) -> Result<SpecKind, String> {
    let error = error_of(node, place)?;
    match error {
        Throw::Plain { .. } | Throw::Service { .. } | Throw::GraphQL { .. } => {
            Ok(SpecKind::ErrorValue(error))
        }
        Throw::Value(_) | Throw::Aggregate { .. } => {
            Err(node.refuse("the error is none of plain, service and graphql"))
        }
    }
}

/// Reads one thrown value.
fn throw(value: &JsValue, pointer: &str, place: Place) -> Result<Throw, String> {
    let node = Node::new(value, pointer)?;
    match node.text("kind")?.as_str() {
        "plain" => {
            node.only(&["kind", "message"])?;
            let message = node.js_string("message")?.clone();
            Ok(Throw::Plain { message })
        }
        "service" => service(&node),
        "graphql" => {
            node.only(&["kind", "message", "extensions"])?;
            Ok(Throw::GraphQL {
                message: node.js_string("message")?.clone(),
                extensions: node.optional_object("extensions")?.cloned(),
            })
        }
        "value" => thrown_value(&node, place),
        "aggregate" => aggregate(&node, place),
        kind => Err(node.refuse(&format!("unknown kind {kind:?}"))),
    }
}

/// Reads a throw of the kind `service`.
///
/// `details` is an object or absent, which is all the extensions of a
/// `ServiceError` of Aosabi hold.
fn service(node: &Node<'_>) -> Result<Throw, String> {
    node.only(&["kind", "message", "code", "statusCode", "details"])?;
    Ok(Throw::Service {
        message: node.js_string("message")?.clone(),
        code: node.text("code")?,
        status_code: node.number("statusCode")?,
        details: node.optional_object("details")?.cloned(),
    })
}

/// Reads a throw of the kind `value`, whose spec makes a value that is no error.
fn thrown_value(node: &Node<'_>, place: Place) -> Result<Throw, String> {
    node.only(&["kind", "value"])?;
    let spec = nested(node, "value", place.nested())?;
    match spec.kind {
        SpecKind::Value(_) | SpecKind::Undefined | SpecKind::Number(_) => Ok(Throw::Value(spec)),
        _ => Err(node.refuse("the value is none of value, undefined and number")),
    }
}

/// Reads a throw of the kind `aggregate`.
fn aggregate(node: &Node<'_>, place: Place) -> Result<Throw, String> {
    node.only(&["kind", "message", "errors"])?;
    let pointer = node.child("errors");
    let errors = node.array("errors")?.iter().enumerate();
    let errors = errors.map(|(index, error)| throw(error, &format!("{pointer}/{index}"), place));
    Ok(Throw::Aggregate {
        message: node.js_string("message")?.clone(),
        errors: errors.collect::<Result<_, _>>()?,
    })
}
