// SPDX-License-Identifier: MIT
/*
 * aosabi-ir: the JSON shape of the model, the bytes `JSON.stringify(ir)`
 * writes and the reader that takes them back.
 *
 * No file of the reference is translated here.
 */

//! The JSON shape of the model.
//!
//! The reference never serializes an IR with a schema of its own. It hands
//! the object to `JSON.stringify`, which writes the keys of each object in
//! the order the object literal created them and leaves out a key whose value
//! is `undefined`. The shape is therefore the order of the literals in
//! `toIR`, in `emptyIR` and in `entityFromDefinition` at `db/ir.ts:281-324`,
//! with `sharedTable` set on an entity at `db/ir.ts:407`. [`to_js_value`]
//! writes that order through the value model of `aosabi-json`, the only crate
//! that writes JSON bytes. A `None` is an absent key and never `null`, an
//! empty list is `[]` and an empty record is `{}`.
//!
//! | Type | Keys in the order written |
//! | --- | --- |
//! | [`SchemaIr`] | `version`, `entities`, `objects`, `interfaces`, `unions`, `inputs`, `enums`, `scalars`, `operations` |
//! | [`TypeRef::Scalar`], [`TypeRef::Ref`] | `kind`, `name`, `nullable` |
//! | [`TypeRef::List`] | `kind`, `of`, `nullable` |
//! | [`Field`] | `name`, `type`, `exposed`, `description`, `args`, `column`, `relation` |
//! | [`Operation`] | `root`, `name`, `args`, `returns`, `description` |
//! | [`ObjectType`], [`InterfaceType`] | `name`, `description`, `implements`, `fields` |
//! | [`InputType`] | `name`, `description`, `fields` |
//! | [`UnionType`] | `name`, `description`, `members` |
//! | [`EnumType`] | `name`, `values`, `description` |
//! | [`Entity`] | `name`, `table`, `abstract`, `primaryKey`, `implements`, `fields`, `indexes`, `sharedTable` |
//!
//! Byte identity of `to_json(from_js_value(x))` with `x` holds for every IR
//! that `toIR` projects. It is not claimed for an IR that a test writes by
//! hand in another key order, because the typed model keeps one order.
//!
//! [stringify]: https://tc39.es/ecma262/#sec-json.stringify

use alloc::{
    boxed::Box,
    format,
    string::String,
    vec::Vec, //
};
use core::fmt;

use aosabi_json::{
    JsObject,
    JsValue, //
};

use crate::{
    ir::{
        Entity,
        EnumType,
        Field,
        InputType,
        InterfaceType,
        ObjectType,
        Operation,
        RootName,
        SchemaIr,
        TypeRef,
        UnionType, //
    },
    record::Record, //
};

/// What [`IrError::expected`] reads where an object has to stand.
const OBJECT: &str = "an object";

/// What [`IrError::expected`] reads where an array has to stand.
const ARRAY: &str = "an array";

/// What [`IrError::expected`] reads where a string has to stand.
const STRING: &str = "a string";

/// What [`IrError::expected`] reads where a string holds a lone surrogate.
const WELL_FORMED_STRING: &str = "a well formed string";

/// What [`IrError::expected`] reads where a boolean has to stand.
const BOOLEAN: &str = "a boolean";

/// What [`IrError::expected`] reads for the `version` member.
const VERSION: &str = "the number 1";

/// What [`IrError::expected`] reads for the `kind` member of a type
/// reference.
const KIND: &str = "one of \"scalar\", \"ref\" and \"list\"";

/// What [`IrError::expected`] reads for the `root` member of an operation.
const ROOT: &str = "one of \"Query\", \"Mutation\" and \"Subscription\"";

/// The refusal of a value that is not an IR.
///
/// The error carries the reason and the message is built in one place, its
/// `Display`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrError {
    /// Where the value stands, such as `objects.User.fields[2].type.kind`.
    /// The path of the whole value is the empty string.
    pub path: String,
    /// What has to stand there, as a noun phrase such as `a boolean`.
    pub expected: &'static str,
}

impl IrError {
    /// Creates the refusal of the value at `path`.
    fn new(path: impl Into<String>, expected: &'static str) -> Self {
        Self {
            path: path.into(),
            expected,
        }
    }
}

impl fmt::Display for IrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.path.is_empty() {
            return write!(f, "expected {} at the root of the IR", self.expected);
        }
        write!(f, "expected {} at {} of the IR", self.expected, self.path)
    }
}

impl core::error::Error for IrError {}

/// Returns the JavaScript value of `ir`, with every key in the order the
/// reference creates it.
#[must_use]
pub fn to_js_value(ir: &SchemaIr) -> JsValue {
    let mut object = JsObject::with_capacity(9);
    object.insert("version", ir.version);
    object.insert("entities", record_value(&ir.entities, entity_value));
    object.insert("objects", record_value(&ir.objects, object_type_value));
    object.insert(
        "interfaces",
        record_value(&ir.interfaces, interface_type_value),
    );
    object.insert("unions", record_value(&ir.unions, union_type_value));
    object.insert("inputs", record_value(&ir.inputs, input_type_value));
    object.insert("enums", record_value(&ir.enums, enum_type_value));
    object.insert("scalars", strings_value(&ir.scalars));
    object.insert("operations", list_value(&ir.operations, operation_value));
    JsValue::Object(object)
}

/// Returns the bytes of `JSON.stringify(ir)` as a string.
///
/// The text is compact and carries no trailing newline. The writer answers
/// `None` for an undefined value alone, and [`to_js_value`] always returns an
/// object, so the empty string is never the answer.
#[must_use]
pub fn to_json(ir: &SchemaIr) -> String {
    aosabi_json::to_string(&to_js_value(ir)).unwrap_or_default()
}

/// Reads an IR back from its JavaScript value.
///
/// The keys of an object are accepted in any order, and a key the model does
/// not name is ignored. A record is read through [`JsObject::own_keys`], so
/// its order is the order `Object.values` gives the printer. An absent key,
/// an `undefined` and a `null` are all `None` for an optional member, because
/// `??` and the truthiness test treat them alike. `column`, `relation` and
/// `indexes` are taken as they stand.
///
/// # Errors
///
/// Returns an [`IrError`] that names the path and what has to stand there,
/// for a value of the wrong type, for a required key that is missing, for a
/// `version` other than `1`, and for a string that holds a lone surrogate,
/// which is out of contract.
pub fn from_js_value(value: &JsValue) -> Result<SchemaIr, IrError> {
    let reader = Reader::new(value, "")?;
    Ok(SchemaIr {
        version: reader.required("version", VERSION, version_at)?,
        entities: reader.required("entities", OBJECT, |value, path| {
            record_at(value, path, entity_at)
        })?,
        objects: reader.required("objects", OBJECT, |value, path| {
            record_at(value, path, object_type_at)
        })?,
        interfaces: reader.required("interfaces", OBJECT, |value, path| {
            record_at(value, path, interface_type_at)
        })?,
        unions: reader.required("unions", OBJECT, |value, path| {
            record_at(value, path, union_type_at)
        })?,
        inputs: reader.required("inputs", OBJECT, |value, path| {
            record_at(value, path, input_type_at)
        })?,
        enums: reader.required("enums", OBJECT, |value, path| {
            record_at(value, path, enum_type_at)
        })?,
        scalars: reader.required("scalars", ARRAY, strings_at)?,
        operations: reader.required("operations", ARRAY, |value, path| {
            list_at(value, path, operation_at)
        })?,
    })
}

/// Sets `key` on `object` when `value` is present, and leaves the key out
/// otherwise, which is what `JSON.stringify` does with `undefined`.
fn insert_optional(object: &mut JsObject, key: &str, value: Option<JsValue>) {
    if let Some(value) = value {
        object.insert(key, value);
    }
}

/// Returns the array of `items`, each written by `write_item`.
fn list_value<T>(items: &[T], write_item: fn(&T) -> JsValue) -> JsValue {
    JsValue::Array(items.iter().map(write_item).collect())
}

/// Returns the array of strings `items`.
fn strings_value(items: &[String]) -> JsValue {
    JsValue::Array(items.iter().map(|item| item.as_str().into()).collect())
}

/// Returns the object of `record`, each value written by `write_item`.
fn record_value<T>(record: &Record<T>, write_item: fn(&T) -> JsValue) -> JsValue {
    let mut object = JsObject::with_capacity(record.len());
    for (key, item) in record.entries() {
        object.insert(key, write_item(item));
    }
    JsValue::Object(object)
}

/// Returns the object of a type reference.
fn type_ref_value(type_ref: &TypeRef) -> JsValue {
    let mut object = JsObject::with_capacity(3);
    match type_ref {
        TypeRef::Scalar { name, .. } => {
            object.insert("kind", "scalar");
            object.insert("name", name.as_str());
        }
        TypeRef::Ref { name, .. } => {
            object.insert("kind", "ref");
            object.insert("name", name.as_str());
        }
        TypeRef::List { of, .. } => {
            object.insert("kind", "list");
            object.insert("of", type_ref_value(of));
        }
    }
    object.insert("nullable", type_ref.nullable());
    JsValue::Object(object)
}

/// Returns the object of a field.
fn field_value(field: &Field) -> JsValue {
    let mut object = JsObject::with_capacity(7);
    object.insert("name", field.name.as_str());
    object.insert("type", type_ref_value(&field.r#type));
    object.insert("exposed", field.exposed);
    insert_optional(
        &mut object,
        "description",
        text_value(field.description.as_deref()),
    );
    let args = field.args.as_ref();
    insert_optional(
        &mut object,
        "args",
        args.map(|args| list_value(args, field_value)),
    );
    insert_optional(&mut object, "column", field.column.clone());
    insert_optional(&mut object, "relation", field.relation.clone());
    JsValue::Object(object)
}

/// Returns the string of an optional text member.
fn text_value(text: Option<&str>) -> Option<JsValue> {
    text.map(JsValue::from)
}

/// Returns the object of an operation.
fn operation_value(operation: &Operation) -> JsValue {
    let mut object = JsObject::with_capacity(5);
    object.insert("root", operation.root.as_str());
    object.insert("name", operation.name.as_str());
    object.insert("args", list_value(&operation.args, field_value));
    object.insert("returns", type_ref_value(&operation.returns));
    insert_optional(
        &mut object,
        "description",
        text_value(operation.description.as_deref()),
    );
    JsValue::Object(object)
}

/// Returns the object of an object type.
fn object_type_value(object_type: &ObjectType) -> JsValue {
    composite_value(
        &object_type.name,
        object_type.description.as_deref(),
        object_type.implements.as_deref(),
        &object_type.fields,
    )
}

/// Returns the object of an interface.
fn interface_type_value(interface_type: &InterfaceType) -> JsValue {
    composite_value(
        &interface_type.name,
        interface_type.description.as_deref(),
        interface_type.implements.as_deref(),
        &interface_type.fields,
    )
}

/// Returns the object an object type and an interface share, whose literals
/// carry the same keys in the same order.
fn composite_value(
    name: &str,
    description: Option<&str>,
    implements: Option<&[String]>,
    fields: &[Field],
) -> JsValue {
    let mut object = JsObject::with_capacity(4);
    object.insert("name", name);
    insert_optional(&mut object, "description", text_value(description));
    insert_optional(&mut object, "implements", implements.map(strings_value));
    object.insert("fields", list_value(fields, field_value));
    JsValue::Object(object)
}

/// Returns the object of an input.
fn input_type_value(input_type: &InputType) -> JsValue {
    let mut object = JsObject::with_capacity(3);
    object.insert("name", input_type.name.as_str());
    insert_optional(
        &mut object,
        "description",
        text_value(input_type.description.as_deref()),
    );
    object.insert("fields", list_value(&input_type.fields, field_value));
    JsValue::Object(object)
}

/// Returns the object of a union.
fn union_type_value(union_type: &UnionType) -> JsValue {
    let mut object = JsObject::with_capacity(3);
    object.insert("name", union_type.name.as_str());
    insert_optional(
        &mut object,
        "description",
        text_value(union_type.description.as_deref()),
    );
    object.insert("members", strings_value(&union_type.members));
    JsValue::Object(object)
}

/// Returns the object of an enum.
fn enum_type_value(enum_type: &EnumType) -> JsValue {
    let mut object = JsObject::with_capacity(3);
    object.insert("name", enum_type.name.as_str());
    object.insert("values", strings_value(&enum_type.values));
    insert_optional(
        &mut object,
        "description",
        text_value(enum_type.description.as_deref()),
    );
    JsValue::Object(object)
}

/// Returns the object of an entity.
fn entity_value(entity: &Entity) -> JsValue {
    let mut object = JsObject::with_capacity(8);
    object.insert("name", entity.name.as_str());
    object.insert("table", entity.table.as_str());
    object.insert("abstract", entity.r#abstract);
    insert_optional(
        &mut object,
        "primaryKey",
        text_value(entity.primary_key.as_deref()),
    );
    object.insert("implements", strings_value(&entity.implements));
    object.insert("fields", list_value(&entity.fields, field_value));
    insert_optional(&mut object, "indexes", entity.indexes.clone());
    insert_optional(
        &mut object,
        "sharedTable",
        entity.shared_table.map(JsValue::from),
    );
    JsValue::Object(object)
}

/// The reader of one object of an IR, which knows where the object stands.
struct Reader<'a> {
    /// The object being read.
    object: &'a JsObject,
    /// The path of the object, the empty string for the whole IR.
    path: &'a str,
}

impl<'a> Reader<'a> {
    /// Creates the reader of `value`, which stands at `path`.
    fn new(value: &'a JsValue, path: &'a str) -> Result<Self, IrError> {
        match value {
            JsValue::Object(object) => Ok(Self { object, path }),
            _ => Err(IrError::new(path, OBJECT)),
        }
    }

    /// Returns the path of the member named `key`.
    fn path_of(&self, key: &str) -> String {
        if self.path.is_empty() {
            return String::from(key);
        }
        format!("{}.{key}", self.path)
    }

    /// Reads the member named `key` with `read`, or returns `None` when the
    /// key is absent or its value is `undefined` or `null`.
    fn optional<T>(
        &self,
        key: &str,
        read: impl FnOnce(&JsValue, &str) -> Result<T, IrError>,
    ) -> Result<Option<T>, IrError> {
        match self.object.get(key) {
            None | Some(JsValue::Undefined | JsValue::Null) => Ok(None),
            Some(value) => read(value, &self.path_of(key)).map(Some),
        }
    }

    /// Reads the member named `key` with `read`, and refuses with `expected`
    /// when the member is missing.
    fn required<T>(
        &self,
        key: &str,
        expected: &'static str,
        read: impl FnOnce(&JsValue, &str) -> Result<T, IrError>,
    ) -> Result<T, IrError> {
        self.optional(key, read)?
            .ok_or_else(|| IrError::new(self.path_of(key), expected))
    }
}

/// Reads the `version` member, which is the number `1` and nothing else.
fn version_at(value: &JsValue, path: &str) -> Result<u32, IrError> {
    match value {
        JsValue::Number(number) if number.to_bits() == 1.0_f64.to_bits() => Ok(1),
        _ => Err(IrError::new(path, VERSION)),
    }
}

/// Reads a string, which has to be well formed to become a Rust `String`.
fn string_at(value: &JsValue, path: &str) -> Result<String, IrError> {
    let JsValue::String(text) = value else {
        return Err(IrError::new(path, STRING));
    };
    text.as_str()
        .map(String::from)
        .ok_or_else(|| IrError::new(path, WELL_FORMED_STRING))
}

/// Reads a boolean.
fn boolean_at(value: &JsValue, path: &str) -> Result<bool, IrError> {
    match value {
        JsValue::Bool(flag) => Ok(*flag),
        _ => Err(IrError::new(path, BOOLEAN)),
    }
}

/// Takes a persistence position as it stands.
// The function cannot fail and still returns a `Result`, because
// `Reader::optional` takes every reader of a member in one signature.
#[expect(
    clippy::unnecessary_wraps,
    reason = "the signature is the one every reader of a member shares"
)]
fn opaque_at(value: &JsValue, _path: &str) -> Result<JsValue, IrError> {
    Ok(value.clone())
}

/// Reads an array, each element with `read_item`.
fn list_at<T>(
    value: &JsValue,
    path: &str,
    read_item: fn(&JsValue, &str) -> Result<T, IrError>,
) -> Result<Vec<T>, IrError> {
    let JsValue::Array(items) = value else {
        return Err(IrError::new(path, ARRAY));
    };
    items
        .iter()
        .enumerate()
        .map(|(index, item)| read_item(item, &format!("{path}[{index}]")))
        .collect()
}

/// Reads an array of strings.
fn strings_at(value: &JsValue, path: &str) -> Result<Vec<String>, IrError> {
    list_at(value, path, string_at)
}

/// Reads an array of fields.
fn fields_at(value: &JsValue, path: &str) -> Result<Vec<Field>, IrError> {
    list_at(value, path, field_at)
}

/// Reads a record in `OrdinaryOwnPropertyKeys` order, each value with
/// `read_item`.
fn record_at<T>(
    value: &JsValue,
    path: &str,
    read_item: fn(&JsValue, &str) -> Result<T, IrError>,
) -> Result<Record<T>, IrError> {
    let reader = Reader::new(value, path)?;
    let mut record = Record::new();
    for (key, item) in reader.object.own_keys() {
        let Some(key) = key.as_str() else {
            let lossy = String::from_utf8_lossy(key.as_wtf8());
            return Err(IrError::new(reader.path_of(&lossy), WELL_FORMED_STRING));
        };
        record.insert(String::from(key), read_item(item, &reader.path_of(key))?);
    }
    Ok(record)
}

/// Reads a type reference.
fn type_ref_at(value: &JsValue, path: &str) -> Result<TypeRef, IrError> {
    let reader = Reader::new(value, path)?;
    let kind = reader.required("kind", KIND, string_at)?;
    match kind.as_str() {
        "scalar" => Ok(TypeRef::Scalar {
            name: reader.required("name", STRING, string_at)?,
            nullable: reader.required("nullable", BOOLEAN, boolean_at)?,
        }),
        "ref" => Ok(TypeRef::Ref {
            name: reader.required("name", STRING, string_at)?,
            nullable: reader.required("nullable", BOOLEAN, boolean_at)?,
        }),
        "list" => Ok(TypeRef::List {
            of: Box::new(reader.required("of", OBJECT, type_ref_at)?),
            nullable: reader.required("nullable", BOOLEAN, boolean_at)?,
        }),
        _ => Err(IrError::new(reader.path_of("kind"), KIND)),
    }
}

/// Reads the `root` member of an operation.
fn root_at(value: &JsValue, path: &str) -> Result<RootName, IrError> {
    let JsValue::String(text) = value else {
        return Err(IrError::new(path, ROOT));
    };
    RootName::ALL
        .into_iter()
        .find(|root| text == root.as_str())
        .ok_or_else(|| IrError::new(path, ROOT))
}

/// Reads a field.
fn field_at(value: &JsValue, path: &str) -> Result<Field, IrError> {
    let reader = Reader::new(value, path)?;
    Ok(Field {
        name: reader.required("name", STRING, string_at)?,
        r#type: reader.required("type", OBJECT, type_ref_at)?,
        exposed: reader.required("exposed", BOOLEAN, boolean_at)?,
        description: reader.optional("description", string_at)?,
        args: reader.optional("args", fields_at)?,
        column: reader.optional("column", opaque_at)?,
        relation: reader.optional("relation", opaque_at)?,
    })
}

/// Reads an operation.
fn operation_at(value: &JsValue, path: &str) -> Result<Operation, IrError> {
    let reader = Reader::new(value, path)?;
    Ok(Operation {
        root: reader.required("root", ROOT, root_at)?,
        name: reader.required("name", STRING, string_at)?,
        args: reader.required("args", ARRAY, fields_at)?,
        returns: reader.required("returns", OBJECT, type_ref_at)?,
        description: reader.optional("description", string_at)?,
    })
}

/// Reads an object type.
fn object_type_at(value: &JsValue, path: &str) -> Result<ObjectType, IrError> {
    let reader = Reader::new(value, path)?;
    Ok(ObjectType {
        name: reader.required("name", STRING, string_at)?,
        fields: reader.required("fields", ARRAY, fields_at)?,
        implements: reader.optional("implements", strings_at)?,
        description: reader.optional("description", string_at)?,
    })
}

/// Reads an interface.
fn interface_type_at(value: &JsValue, path: &str) -> Result<InterfaceType, IrError> {
    let reader = Reader::new(value, path)?;
    Ok(InterfaceType {
        name: reader.required("name", STRING, string_at)?,
        fields: reader.required("fields", ARRAY, fields_at)?,
        implements: reader.optional("implements", strings_at)?,
        description: reader.optional("description", string_at)?,
    })
}

/// Reads an input.
fn input_type_at(value: &JsValue, path: &str) -> Result<InputType, IrError> {
    let reader = Reader::new(value, path)?;
    Ok(InputType {
        name: reader.required("name", STRING, string_at)?,
        fields: reader.required("fields", ARRAY, fields_at)?,
        description: reader.optional("description", string_at)?,
    })
}

/// Reads a union.
fn union_type_at(value: &JsValue, path: &str) -> Result<UnionType, IrError> {
    let reader = Reader::new(value, path)?;
    Ok(UnionType {
        name: reader.required("name", STRING, string_at)?,
        members: reader.required("members", ARRAY, strings_at)?,
        description: reader.optional("description", string_at)?,
    })
}

/// Reads an enum.
fn enum_type_at(value: &JsValue, path: &str) -> Result<EnumType, IrError> {
    let reader = Reader::new(value, path)?;
    Ok(EnumType {
        name: reader.required("name", STRING, string_at)?,
        values: reader.required("values", ARRAY, strings_at)?,
        description: reader.optional("description", string_at)?,
    })
}

/// Reads an entity.
fn entity_at(value: &JsValue, path: &str) -> Result<Entity, IrError> {
    let reader = Reader::new(value, path)?;
    Ok(Entity {
        name: reader.required("name", STRING, string_at)?,
        table: reader.required("table", STRING, string_at)?,
        r#abstract: reader.required("abstract", BOOLEAN, boolean_at)?,
        fields: reader.required("fields", ARRAY, fields_at)?,
        primary_key: reader.optional("primaryKey", string_at)?,
        implements: reader.required("implements", ARRAY, strings_at)?,
        indexes: reader.optional("indexes", opaque_at)?,
        shared_table: reader.optional("sharedTable", boolean_at)?,
    })
}
