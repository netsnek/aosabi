// SPDX-License-Identifier: MIT
/*
 * aosabi-execute value convert: the conversions between the executor value
 * and the JSON value model.
 *
 * Derived from graphql-js 16.12.0, src/type/scalars.ts, MIT.
 */

//! The conversions between [`Value`] and [`JsValue`].
//!
//! The type layer coerces and serializes over the JSON value model, and the
//! executor holds a [`Value`]. A coerced argument enters through
//! [`Value::from_js_owned`], which consumes it, a value the caller keeps
//! through [`Value::from_js`] or [`Value::from_js_object`], and a completed
//! leaf leaves through [`Value::to_js`] or [`Value::to_specified_input`]. A
//! value of the JSON model a serializer answers or a member holds enters the
//! arena of an execution as it is, with no value of the executor made of it
//! (see [`crate::arena`]).
//!
//! A key of a [`PlainObject`] is a string of Rust, which holds no lone
//! surrogate. With the feature `host-reads`, the builds of the TypeScript
//! option, an object of the JSON model with such a key becomes a `WideKeyed`
//! object in place of a [`PlainObject`], which keeps its keys as JavaScript
//! strings, so the key of an object of a host that a leaf of `JSONObject`,
//! `JSON` or `Any` answers, or that a variable brings in, keeps its UTF-16
//! units through the executor.

use std::{
    rc::Rc,
    sync::Arc, //
};

#[cfg(feature = "host-reads")]
use aosabi_json::JsString;
use aosabi_json::{
    JsObject,
    JsValue,
    Opaque, //
};

use crate::value::{
    ErrorObject,
    HostValue,
    ObjectLike,
    PlainObject,
    Value, //
};

/// What a `Date` becomes in a converted value.
#[derive(Clone, Copy)]
enum DateView {
    /// The text of `toJSON`, or null for an invalid date.
    Json,
    /// The time value, which `valueOf` answers.
    TimeValue,
}

impl Value {
    /// Returns the executor value of a value of the JSON model.
    ///
    /// An object becomes a [`PlainObject`] with the properties in their
    /// insertion order, an array becomes a list, and a host object carried
    /// through unread becomes [`HostValue::Opaque`] over the same object, so a
    /// `File` of a variable reaches the resolver as the parser made it.
    #[must_use]
    pub fn from_js(value: &JsValue) -> Self {
        match value {
            JsValue::Undefined => Self::Undefined,
            JsValue::Null => Self::Null,
            JsValue::Bool(boolean) => Self::Bool(*boolean),
            JsValue::Number(number) => Self::Number(*number),
            JsValue::String(string) => Self::String(string.clone()),
            JsValue::Array(items) => Self::List(Rc::new(items.iter().map(Self::from_js).collect())),
            JsValue::Object(object) => object_from_js(object),
            JsValue::Opaque(opaque) => Self::Host(HostValue::Opaque(Arc::clone(&opaque.0))),
        }
    }

    /// Returns the executor value of an object of the JSON model, the value
    /// [`Value::from_js`] makes of the object as a value.
    #[must_use]
    pub fn from_js_object(object: &JsObject) -> Self {
        object_from_js(object)
    }

    /// Returns the executor value of a value of the JSON model, consuming
    /// it.
    ///
    /// This is [`Value::from_js`] over a value the caller gives up, such as
    /// a coerced argument: a string, an item and a key move into the
    /// executor value and none is copied, a list keeps the vector of the
    /// array, and a host object keeps the handle of the value with no count.
    #[must_use]
    pub fn from_js_owned(value: JsValue) -> Self {
        match value {
            JsValue::Undefined => Self::Undefined,
            JsValue::Null => Self::Null,
            JsValue::Bool(boolean) => Self::Bool(boolean),
            JsValue::Number(number) => Self::Number(number),
            JsValue::String(string) => Self::String(string),
            JsValue::Array(items) => Self::List(Rc::new(
                items.into_iter().map(Self::from_js_owned).collect(),
            )),
            JsValue::Object(object) => object_from_js_owned(object),
            JsValue::Opaque(opaque) => Self::Host(HostValue::Opaque(opaque.0)),
        }
    }

    /// Returns the value as `JSON.stringify` sees it.
    ///
    /// A function and `Value::Undefined` are absent, a `Date` is the text of
    /// its `toJSON` or null when it is invalid, a `GraphQLError` is the object
    /// of its `toJSON`, a host object carried through unread is the same
    /// object again, and a promise, any other host value and any other error
    /// are an object with no enumerable own property. The reference throws a
    /// `TypeError` for a value that contains itself. This function has no
    /// throw, so the inner occurrence reads as null.
    #[must_use]
    pub fn to_js(&self) -> JsValue {
        convert(self, DateView::Json, &mut Vec::new())
    }

    /// Returns the value as a serialize hook of the type layer reads it.
    ///
    /// This is [`Value::to_js`] with one difference. A `Date` is its time
    /// value, because `serializeObject` asks `valueOf` before it asks
    /// `toJSON`, and `valueOf` of a `Date` answers a number.
    #[must_use]
    pub fn to_specified_input(&self) -> JsValue {
        convert(self, DateView::TimeValue, &mut Vec::new())
    }
}

/// Returns the value of an object of the JSON model, a [`PlainObject`], or
/// with the feature `host-reads` a [`WideKeyed`] object where a key holds a
/// lone surrogate.
fn object_from_js(object: &JsObject) -> Value {
    #[cfg(feature = "host-reads")]
    if object.iter().any(|(key, _)| key.as_str().is_none()) {
        return Value::Object(Rc::new(WideKeyed(object.clone())));
    }
    Value::Object(plain_object_from_js(object))
}

/// Returns the value of an object of the JSON model it consumes, as
/// [`object_from_js`] makes it of the object as a value: a [`PlainObject`],
/// or with the feature `host-reads` a [`WideKeyed`] object that keeps the
/// object itself where a key holds a lone surrogate.
fn object_from_js_owned(object: JsObject) -> Value {
    #[cfg(feature = "host-reads")]
    if object.iter().any(|(key, _)| key.as_str().is_none()) {
        return Value::Object(Rc::new(WideKeyed(object)));
    }
    Value::Object(plain_object_from_js_owned(object))
}

/// An object of the JSON model with a key that holds a lone surrogate, kept
/// as it is.
///
/// Its keys and its members read as those of the [`PlainObject`] the object
/// would make, a key of a string of Rust with each byte UTF-8 refuses
/// replaced, and the provided methods `own_keys_js` and `get_js` of
/// [`ObjectLike`] answer the keys as JavaScript strings, through which the
/// conversion to the JSON model and a host read them. An object of a build
/// of the Rust option has no such key, and only the feature `host-reads`
/// makes one.
#[cfg(feature = "host-reads")]
struct WideKeyed(JsObject);

#[cfg(feature = "host-reads")]
impl ObjectLike for WideKeyed {
    fn get(self: Rc<Self>, key: &str) -> Value {
        self.0.get(key).map_or(Value::Undefined, Value::from_js)
    }

    /// Returns the keys in the order of `OrdinaryOwnPropertyKeys`, each as
    /// the key of the [`PlainObject`] the object would make.
    fn own_keys(&self) -> Vec<String> {
        self.0
            .own_keys()
            .map(|(key, _)| String::from_utf8_lossy(key.as_wtf8()).into_owned())
            .collect()
    }

    /// Returns the keys in the order of `OrdinaryOwnPropertyKeys`, as they are.
    fn own_keys_js(&self) -> Vec<JsString> {
        self.0.own_keys().map(|(key, _)| key.clone()).collect()
    }

    fn get_js(self: Rc<Self>, key: &JsString) -> Value {
        self.0
            .iter()
            .find(|(own, _)| *own == key)
            .map_or(Value::Undefined, |(_, value)| Value::from_js(value))
    }
}

/// Returns the [`PlainObject`] of an object of the JSON model.
fn plain_object_from_js(object: &JsObject) -> Rc<PlainObject> {
    let entries = object
        .iter()
        .map(|(key, value)| {
            let key = String::from_utf8_lossy(key.as_wtf8()).into_owned();
            (key, Value::from_js(value))
        })
        .collect();
    PlainObject::from_entries(entries)
}

/// Returns the [`PlainObject`] of an object of the JSON model it consumes.
///
/// A key keeps its bytes where they are UTF-8, and a key with a lone
/// surrogate reads as [`plain_object_from_js`] reads it. The entries take
/// the room of the properties of the object and no more.
fn plain_object_from_js_owned(object: JsObject) -> Rc<PlainObject> {
    let mut entries = Vec::with_capacity(object.len());
    for (key, value) in object {
        let key = key
            .into_string()
            .unwrap_or_else(|key| String::from_utf8_lossy(key.as_wtf8()).into_owned());
        entries.push((key, Value::from_js_owned(value)));
    }
    PlainObject::from_entries(entries)
}

/// Converts `value`, with `seen` holding the objects that enclose it.
fn convert(value: &Value, dates: DateView, seen: &mut Vec<*const ()>) -> JsValue {
    match value {
        Value::Undefined | Value::Function(_) => JsValue::Undefined,
        Value::Null => JsValue::Null,
        Value::Bool(boolean) => JsValue::Bool(*boolean),
        Value::Number(number) => JsValue::Number(*number),
        Value::String(string) => JsValue::String(string.clone()),
        Value::Date(date) => match dates {
            DateView::Json => date.to_iso_string().map_or(JsValue::Null, JsValue::from),
            DateView::TimeValue => JsValue::Number(date.get_time()),
        },
        Value::List(items) => JsValue::Array(
            items
                .iter()
                .map(|item| convert(item, dates, seen))
                .collect(),
        ),
        Value::Object(object) => convert_object(object, dates, seen),
        Value::Error(error) => match error.as_ref() {
            ErrorObject::GraphQL(error) => JsValue::from(error.to_json()),
            ErrorObject::Custom { props, .. } => JsValue::Object((**props).clone()),
            _ => JsValue::Object(JsObject::new()),
        },
        Value::Host(HostValue::Opaque(handle)) => JsValue::Opaque(Opaque(Arc::clone(handle))),
        Value::Promise(_) | Value::Host(_) => JsValue::Object(JsObject::new()),
    }
}

/// Converts the enumerable own properties of `object` in key order.
///
/// The keys are those of `EnumerableOwnProperties` when the conversion starts,
/// the list [`ObjectLike::own_keys`] answers, and each value is read through
/// [`ObjectLike::get`] after the list is made, as `SerializeJSONObject` reads
/// each key of its list. The converted object keeps the keys of the list with
/// no copy and takes its capacity at once.
///
/// With the feature `host-reads` the keys are read as JavaScript strings, so
/// a key of an object of a host that holds a lone surrogate is written as
/// `JSON.stringify` writes it.
fn convert_object(
    object: &Rc<dyn ObjectLike>,
    dates: DateView,
    seen: &mut Vec<*const ()>,
) -> JsValue {
    let address = Rc::as_ptr(object).cast::<()>();
    if seen.contains(&address) {
        return JsValue::Null;
    }
    seen.push(address);
    #[cfg(feature = "host-reads")]
    let keys = object.own_keys_js();
    #[cfg(not(feature = "host-reads"))]
    let keys = object.own_keys();
    let mut converted = JsObject::with_capacity(keys.len());
    #[cfg(feature = "host-reads")]
    for key in keys {
        let property = convert(&Rc::clone(object).get_js(&key), dates, seen);
        if !property.is_undefined() {
            converted.insert(key, property);
        }
    }
    #[cfg(not(feature = "host-reads"))]
    for key in keys {
        let property = convert(&Rc::clone(object).get(&key), dates, seen);
        if !property.is_undefined() {
            converted.insert(key, property);
        }
    }
    seen.pop();
    JsValue::Object(converted)
}
