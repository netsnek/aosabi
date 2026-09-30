// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-core boot ir: the part of the build model a boot reads.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/ir/ir.ts,
 * Apache-2.0. Changed by the Aosabi authors.
 */

//! [`Ir`], the part of the build model a boot reads.
//!
//! The build infers its model from the TypeScript source, prints the SDL
//! from it and writes the type resolver of every union and interface from it. A
//! boot reads four parts of the JSON text of that model: the object types with
//! their fields and the interfaces they implement, the interfaces with their
//! fields, the unions with their members, and the root operations with the
//! names of their arguments in order. A field is required where the type at the
//! end of its chain of lists is not nullable, which is the `isRequired` of the
//! parser the type resolvers read, since the model writes that flag on the leaf
//! of a list.

use aosabi_json::{
    JsObject,
    JsValue, //
};

/// The version of the model a boot reads, `version: 1`.
const VERSION: f64 = 1.0;

/// A field of an object type or an interface.
#[derive(Debug, Clone)]
pub struct Field {
    /// The name of the field.
    pub name: String,
    /// Whether the type at the end of its chain of lists is not nullable.
    pub required: bool,
}

/// An object type or an interface of the model.
#[derive(Debug, Clone)]
pub struct Type {
    /// The name of the type.
    pub name: String,
    /// The fields, in the order of the model.
    pub fields: Vec<Field>,
    /// The interfaces the type implements, in the order of the model.
    pub implements: Vec<String>,
}

/// A union of the model.
#[derive(Debug, Clone)]
pub struct Union {
    /// The name of the union.
    pub name: String,
    /// The names of its members, in the order of the model.
    pub members: Vec<String>,
}

/// A root operation of the model, one field of `Query`, `Mutation` or
/// `Subscription`.
#[derive(Debug, Clone)]
pub struct Operation {
    /// The root, `Query`, `Mutation` or `Subscription`.
    pub root: String,
    /// The name of the field.
    pub name: String,
    /// The names of its arguments, in the order the field takes them.
    pub args: Vec<String>,
}

/// The part of the build model a boot reads.
#[derive(Debug, Clone, Default)]
pub struct Ir {
    /// The object types, in the order of the model.
    pub objects: Vec<Type>,
    /// The interfaces, in the order of the model.
    pub interfaces: Vec<Type>,
    /// The unions, in the order of the model.
    pub unions: Vec<Union>,
    /// The root operations, in the order of the model.
    pub operations: Vec<Operation>,
}

impl Ir {
    /// Reads the model from the JSON text `text`.
    ///
    /// # Errors
    ///
    /// Returns a message that names what the text lacks: JSON, the version
    /// 1, or a member of the shape of `ir.ts`.
    pub fn read(text: &str) -> Result<Self, String> {
        let value = aosabi_json::parse(text.as_bytes())
            .map_err(|error| format!("the IR is no JSON: {error}"))?;
        let JsValue::Object(model) = value else {
            return Err(String::from("the IR is no object"));
        };
        if !matches!(model.get("version"), Some(JsValue::Number(version)) if *version == VERSION) {
            return Err(String::from("the IR is no model of version 1"));
        }
        Ok(Self {
            objects: types(&model, "objects")?,
            interfaces: types(&model, "interfaces")?,
            unions: unions(&model)?,
            operations: operations(&model)?,
        })
    }
}

/// Returns the members of the object `key` of `model`, a record by name.
fn record<'a>(model: &'a JsObject, key: &str) -> Result<Vec<&'a JsObject>, String> {
    let Some(JsValue::Object(record)) = model.get(key) else {
        return Err(format!("the IR holds no record {key}"));
    };
    record
        .iter()
        .map(|(_, entry)| match entry {
            JsValue::Object(entry) => Ok(entry),
            _ => Err(format!("an entry of {key} of the IR is no object")),
        })
        .collect()
}

/// Returns the string member `key` of `object`, which `what` names.
fn text(object: &JsObject, key: &str, what: &str) -> Result<String, String> {
    match object.get(key) {
        Some(JsValue::String(text)) => text
            .as_str()
            .map(String::from)
            .ok_or_else(|| format!("the {key} of {what} of the IR is no well formed text")),
        _ => Err(format!("{what} of the IR holds no string {key}")),
    }
}

/// Returns the strings of the array member `key` of `object`, an empty list
/// where the member is absent.
fn texts(object: &JsObject, key: &str, what: &str) -> Result<Vec<String>, String> {
    match object.get(key) {
        None | Some(JsValue::Undefined) => Ok(Vec::new()),
        Some(JsValue::Array(items)) => items
            .iter()
            .map(|item| match item {
                JsValue::String(text) => text.as_str().map(String::from).ok_or_else(|| {
                    format!("an item of {key} of {what} of the IR is no well formed text")
                }),
                _ => Err(format!("an item of {key} of {what} of the IR is no string")),
            })
            .collect(),
        Some(_) => Err(format!("the {key} of {what} of the IR is no array")),
    }
}

/// Returns the fields or the arguments of the array member `key` of
/// `object`, an empty list where the member is absent.
fn fields(object: &JsObject, key: &str, what: &str) -> Result<Vec<Field>, String> {
    let items = match object.get(key) {
        None | Some(JsValue::Undefined) => return Ok(Vec::new()),
        Some(JsValue::Array(items)) => items,
        Some(_) => return Err(format!("the {key} of {what} of the IR is no array")),
    };
    items
        .iter()
        .map(|item| {
            let JsValue::Object(field) = item else {
                return Err(format!("an item of {key} of {what} of the IR is no object"));
            };
            Ok(Field {
                name: text(field, "name", what)?,
                required: !leaf_nullable(field.get("type"), what)?,
            })
        })
        .collect()
}

/// Returns whether the type at the end of the chain of lists of the type
/// reference `reference` is nullable.
fn leaf_nullable(reference: Option<&JsValue>, what: &str) -> Result<bool, String> {
    let mut current = reference;
    loop {
        let Some(JsValue::Object(node)) = current else {
            return Err(format!(
                "a field of {what} of the IR holds no type reference"
            ));
        };
        match node.get("kind") {
            Some(JsValue::String(kind)) if kind.as_str() == Some("list") => {
                current = node.get("of");
            }
            _ => {
                return match node.get("nullable") {
                    Some(JsValue::Bool(nullable)) => Ok(*nullable),
                    _ => Err(format!(
                        "a type reference of {what} of the IR holds no nullable flag"
                    )),
                };
            }
        }
    }
}

/// Returns the object types or the interfaces of the record `key`.
fn types(model: &JsObject, key: &str) -> Result<Vec<Type>, String> {
    record(model, key)?
        .into_iter()
        .map(|entry| {
            let name = text(entry, "name", key)?;
            Ok(Type {
                fields: fields(entry, "fields", &name)?,
                implements: texts(entry, "implements", &name)?,
                name,
            })
        })
        .collect()
}

/// Returns the unions of the model.
fn unions(model: &JsObject) -> Result<Vec<Union>, String> {
    record(model, "unions")?
        .into_iter()
        .map(|entry| {
            let name = text(entry, "name", "unions")?;
            Ok(Union {
                members: texts(entry, "members", &name)?,
                name,
            })
        })
        .collect()
}

/// Returns the root operations of the model.
fn operations(model: &JsObject) -> Result<Vec<Operation>, String> {
    let Some(JsValue::Array(items)) = model.get("operations") else {
        return Err(String::from("the IR holds no array operations"));
    };
    items
        .iter()
        .map(|item| {
            let JsValue::Object(operation) = item else {
                return Err(String::from("an operation of the IR is no object"));
            };
            let name = text(operation, "name", "an operation")?;
            Ok(Operation {
                root: text(operation, "root", &name)?,
                args: fields(operation, "args", &name)?
                    .into_iter()
                    .map(|field| field.name)
                    .collect(),
                name,
            })
        })
        .collect()
}
