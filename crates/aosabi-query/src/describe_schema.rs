// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-query: the schema descriptor the generated client carries.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/query/build/describe-schema.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 */

//! The schema descriptor.
//!
//! The descriptor is the compact runtime description the result wrapper of
//! the client reads: for every object and interface type, each field's named
//! return type and four flags, each present only when it holds. It reaches
//! `index.ts` through `JSON.stringify`, so its keys keep the order the
//! reference inserts them in, which [`JsObject`] keeps.

use aosabi_json::{
    JsObject,
    JsValue, //
};
use aosabi_type::{
    DefinitionError,
    Field,
    NamedType,
    ObjMap,
    Schema,
    Type,
    get_named_type,
    is_non_null_type, //
};

/// The field name whose assignment to a plain object sets its prototype.
///
/// `out[field.name] = desc` on the object literal `{}` runs the setter of
/// `Object.prototype.__proto__` for this name, so the field becomes the
/// prototype of the object and no own property of it, and `JSON.stringify`
/// leaves it out. The field map of graphql-js is an object of no prototype and
/// keeps the name as a key.
///
/// [proto]: https://tc39.es/ecma262/#sec-object.prototype.__proto__
const PROTO: &str = "__proto__";

/// Builds the compact runtime descriptor the result wrapper needs.
///
/// For every object type and every interface type of the type map of
/// `schema`, in its order, the descriptor records each field's named return
/// type, whether it is a list, whether it is a leaf (a scalar or an enum),
/// and whether it is callable (declares arguments). Introspection types, the
/// names that start with `__`, are skipped. The answer is the object
/// `{query, types}`, `query` naming the query root or `Query` when the schema
/// has none.
///
/// The interface types are described as well, so the wrapper knows their
/// shared fields, and a concrete field resolves through the runtime
/// `__typename`.
///
/// # Errors
///
/// Returns what the field thunk of a type throws, which a schema built from
/// SDL has already run.
pub fn describe_schema(schema: &Schema) -> Result<JsValue, DefinitionError> {
    let mut types = JsObject::new();

    for ty in schema.get_type_map().values() {
        if ty.name().starts_with("__") {
            continue;
        }
        let fields = match ty {
            NamedType::Object(object) => object.get_fields()?,
            NamedType::Interface(interface) => interface.get_fields()?,
            _ => continue,
        };
        types.insert(ty.name(), describe_object(fields));
    }

    let mut descriptor = JsObject::with_capacity(2);
    descriptor.insert(
        "query",
        schema.get_query_type().map_or("Query", NamedType::name),
    );
    descriptor.insert("types", types);
    Ok(JsValue::Object(descriptor))
}

/// Returns the description of every field of `fields`, keyed by field name.
///
/// An argument is optional when it is nullable or carries a default, and a
/// field whose arguments are all optional is dual mode, a bare read or a
/// call, which `optionalArgs` says.
fn describe_object(fields: &ObjMap<Field>) -> JsObject {
    let mut out = JsObject::new();
    for field in fields.values() {
        let named = get_named_type(&field.r#type);
        let mut desc = JsObject::new();
        desc.insert("type", named.name());
        if is_list_anywhere(&field.r#type) {
            desc.insert("list", true);
        }
        if matches!(named, NamedType::Scalar(_) | NamedType::Enum(_)) {
            desc.insert("scalar", true);
        }
        if !field.args.is_empty() {
            desc.insert("callable", true);
            let all_optional = field
                .args
                .iter()
                .all(|a| !is_non_null_type(&a.r#type) || !a.default_value.is_undefined());
            if all_optional {
                desc.insert("optionalArgs", true);
            }
        }
        // The assignment of the reference sets the prototype of `out` here
        // and adds no property, as `PROTO` says.
        if field.name == PROTO {
            continue;
        }
        out.insert(field.name.as_str(), desc);
    }
    out
}

/// Returns whether `ty` is a list at any level of its wrapping.
///
/// The walk unwraps a non null type and stops at the named type, which wraps
/// nothing.
fn is_list_anywhere(ty: &Type) -> bool {
    let mut t = Some(ty);
    while let Some(current) = t {
        if matches!(current, Type::List(_)) {
            return true;
        }
        t = current.of_type();
    }
    false
}
