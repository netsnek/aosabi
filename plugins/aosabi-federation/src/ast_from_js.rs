// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-federation: the literal of an argument value and the uploads it
 * lifts.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/core/gateway.ts,
 * Apache-2.0. Changed by the Aosabi authors.
 *
 * The builders of the abstract syntax tree from a JavaScript value.
 */

//! The literal of an argument value, [`ast_from_js_value`], with the files
//! it lifts into upload variables.
//!
//! A value a federation writes into an outbound document becomes the value
//! node of its JavaScript kind: null and `undefined` a null value, a string a
//! string value, an integral number an int value and any other number a float
//! value, each written as `String(value)` writes the number, a boolean a
//! boolean value, an array a list of its items, and an object an input object
//! of its members that are not `undefined`, in the order of its keys. A file
//! has no member of its own, so the object branch writes it as an input
//! object with no field. `InlineArgsTransform` hands an [`UploadCollector`],
//! and then a file becomes the variable `_aosabiUpload_<n>`, numbered by the
//! files collected before it, which the request sends as a variable for the
//! multipart body of the executor. Its name is the named divergence D55 of
//! `docs/byte-equality.md` section 3.
//!
//! A value is one of the JSON model of `aosabi-json`. A host object it
//! carries through unread is a file when the marker [`File`] of the core
//! reads an upload in it, which is how a front holds the file of a multipart
//! request. Any other host object, a `Date` among them, has no member that
//! `Object.entries` reads either, so it is an input object with no field, as
//! the reference writes a `Date`. The last branch of the reference,
//! `String(value)` for a symbol, a big integer and a function, takes no value
//! of the model. A string value of the syntax tree holds text, so a lone
//! surrogate of a string prints as the replacement character.

use aosabi_core::{
    descriptor::File,
    glue::from_argument::FromArgument, //
};
use aosabi_execute::value::Value;
use aosabi_json::{
    JsObject,
    JsValue,
    map::IndexMap,
    number::number_to_string, //
};
use aosabi_lang::{
    ast::{
        BooleanValueNode,
        FloatValueNode,
        IntValueNode,
        ListValueNode,
        NameNode,
        NullValueNode,
        ObjectFieldNode,
        ObjectValueNode,
        StringValueNode,
        ValueNode,
        VariableNode, //
    },
    js_string::to_string_lossy, //
};

/// The files a transform lifts out of the values it writes into a document, keyed by the variable that stands for each.
///
/// A `Map` from the generated variable name to the value, in the order the
/// values were set. The map answers `has` and `set` in one lookup, as the
/// `Map` does.
#[derive(Clone, Debug, Default)]
pub struct UploadCollector {
    /// The variable names with their files, in insertion order.
    entries: IndexMap<String, JsValue>,
}

impl UploadCollector {
    /// Returns a collector that holds no file.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the number of files, `uploads.size`.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns whether the collector holds no file.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns whether a file stands for the variable `name`, `uploads.has(name)`.
    #[must_use]
    pub fn has(&self, name: &str) -> bool {
        self.entries.contains_key(name)
    }

    /// Sets the file of the variable `name`, `uploads.set(name, value)`.
    ///
    /// A name that is set already keeps its place and takes the new value.
    pub fn set(&mut self, name: String, value: JsValue) {
        self.entries.insert(name, value);
    }

    /// Iterates over the variable names and their files in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &JsValue)> {
        self.entries
            .iter()
            .map(|(name, value)| (name.as_str(), value))
    }
}

/// Returns whether a value is a file, a `File` or a `Blob` of a request.
///
/// The reference duck types an object with `arrayBuffer`, `stream` and a
/// numeric `size`, which every file of a multipart request has, and the port
/// asks the marker [`File`] whether the host object carries the upload a
/// front received.
#[must_use]
pub fn is_binary(value: &JsValue) -> bool {
    matches!(value, JsValue::Opaque(_))
        && File::from_argument(&Value::from_js(value)).is_ok_and(|file| file.is_upload())
}

/// Returns the value node of a JavaScript value, lifting each file into a variable of `uploads` when a collector is given.
///
/// A file meets the collector before any other branch, so it becomes the
/// variable `_aosabiUpload_<n>`, where `n` is the number of files collected
/// before it, and the collector keeps the file under that name. The items of
/// an array are written whole, so an item that is `undefined` writes `null`,
/// and the members of an object that are `undefined` are left out, in the
/// order of its keys.
#[must_use]
pub fn ast_from_js_value(value: &JsValue, mut uploads: Option<&mut UploadCollector>) -> ValueNode {
    if let Some(collector) = uploads.as_deref_mut()
        && is_binary(value)
    {
        let name = format!("_aosabiUpload_{}", collector.len());
        collector.set(name.clone(), value.clone());
        return ValueNode::Variable(VariableNode {
            loc: None,
            name: NameNode::new(None, name),
        });
    }
    match value {
        JsValue::Undefined | JsValue::Null => ValueNode::NullValue(NullValueNode { loc: None }),
        JsValue::String(string) => ValueNode::StringValue(StringValueNode {
            loc: None,
            value: to_string_lossy(string).into_owned(),
            block: None,
        }),
        JsValue::Number(number) => number_value(*number),
        JsValue::Bool(boolean) => ValueNode::BooleanValue(BooleanValueNode {
            loc: None,
            value: *boolean,
        }),
        JsValue::Array(items) => ValueNode::ListValue(ListValueNode {
            loc: None,
            values: items
                .iter()
                .map(|item| ast_from_js_value(item, uploads.as_deref_mut()))
                .collect(),
        }),
        JsValue::Object(object) => ValueNode::ObjectValue(ObjectValueNode {
            loc: None,
            fields: object_fields(object, uploads),
        }),
        JsValue::Opaque(_) => ValueNode::ObjectValue(ObjectValueNode {
            loc: None,
            fields: Vec::new(),
        }),
    }
}

/// Returns the int or the float value of a number, `Number.isInteger(value)` deciding.
///
/// A number that is not finite is no integer, so `NaN` and the infinities
/// are floats with the text `String` gives them, and `-0` is the int `0`.
fn number_value(number: f64) -> ValueNode {
    let value = number_to_string(number);
    if number.is_finite() && number.fract() == 0.0 {
        ValueNode::IntValue(IntValueNode { loc: None, value })
    } else {
        ValueNode::FloatValue(FloatValueNode { loc: None, value })
    }
}

/// Returns the fields of an input object, each member that is not `undefined` in the order of the keys.
fn object_fields(
    object: &JsObject,
    mut uploads: Option<&mut UploadCollector>,
) -> Vec<ObjectFieldNode> {
    object
        .own_keys()
        .filter(|(_, member)| !member.is_undefined())
        .map(|(key, member)| ObjectFieldNode {
            loc: None,
            name: NameNode::new(None, to_string_lossy(key)),
            value: ast_from_js_value(member, uploads.as_deref_mut()),
        })
        .collect()
}
