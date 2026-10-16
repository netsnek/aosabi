// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool ast_json: renders an abstract syntax tree as the value
 * JSON.stringify sees when graphql-js hands it its own tree.
 */

//! The tree the `parse` primitive writes.
//!
//! [`to_json`] walks a node and builds the object `JSON.stringify` would
//! serialize. Three rules of the reference decide the shape.
//!
//! - A property whose value is `undefined` is left out by `JSON.stringify`, so
//!   an absent description, alias, name or default value writes no key. The
//!   tree of aosabi-lang holds such a property as `None`.
//! - `loc` is written as `{start, end}`, because `Location` carries its own
//!   `toJSON`, and it is absent under the `noLocation` option.
//! - `tokenCount` is defined as a non enumerable property of the document, so
//!   it is never written.

use aosabi_json::{
    JsObject,
    JsValue, //
};
use aosabi_lang::NodeRef;

/// Returns `node` as the value `JSON.stringify` serializes for it.
///
/// `node` is any node or any union of nodes. The object opens with `kind`,
/// carries the fields of the node in the order of the reference literal and
/// closes with `loc` when the node has one.
pub fn to_json<'a>(node: impl Into<NodeRef<'a>>) -> JsValue {
    let node = node.into();
    let mut object = JsObject::new();
    object.insert("kind", node.kind().as_str());
    insert_fields(&mut object, node);
    if let Some(loc) = node.loc() {
        object.insert("loc", loc.to_json());
    }
    JsValue::Object(object)
}

/// Inserts the fields of `node` between `kind` and `loc`.
///
/// The kinds are split by the part of the grammar they belong to, so each
/// match stays a page and reads beside its part of the reference parser.
fn insert_fields(object: &mut JsObject, node: NodeRef<'_>) {
    if insert_executable_fields(object, node) {
        return;
    }
    if insert_value_fields(object, node) {
        return;
    }
    if insert_type_system_fields(object, node) {
        return;
    }
    if insert_extension_fields(object, node) {
        return;
    }
    insert_coordinate_fields(object, node);
}

/// Inserts the fields of a node of the executable grammar and the type
/// references, and returns whether `node` was one.
fn insert_executable_fields(object: &mut JsObject, node: NodeRef<'_>) -> bool {
    match node {
        NodeRef::Name(node) => {
            object.insert("value", node.value());
        }
        NodeRef::Document(node) => put_list(object, "definitions", &node.definitions),
        NodeRef::OperationDefinition(node) => {
            object.insert("operation", node.operation.as_str());
            put_optional(object, "description", node.description.as_ref());
            put_optional(object, "name", node.name.as_ref());
            put_optional_list(
                object,
                "variableDefinitions",
                node.variable_definitions.as_ref(),
            );
            put_optional_list(object, "directives", node.directives.as_ref());
            put_node(object, "selectionSet", &node.selection_set);
        }
        NodeRef::VariableDefinition(node) => {
            put_optional(object, "description", node.description.as_ref());
            put_node(object, "variable", &node.variable);
            put_node(object, "type", &node.r#type);
            put_optional(object, "defaultValue", node.default_value.as_ref());
            put_optional_list(object, "directives", node.directives.as_ref());
        }
        NodeRef::Variable(node) => put_node(object, "name", &node.name),
        NodeRef::SelectionSet(node) => put_list(object, "selections", &node.selections),
        NodeRef::Field(node) => {
            put_optional(object, "alias", node.alias.as_ref());
            put_node(object, "name", &node.name);
            put_optional_list(object, "arguments", node.arguments.as_ref());
            put_optional_list(object, "directives", node.directives.as_ref());
            put_optional(object, "selectionSet", node.selection_set.as_ref());
        }
        NodeRef::Argument(node) => {
            put_node(object, "name", &node.name);
            put_node(object, "value", &node.value);
        }
        NodeRef::FragmentSpread(node) => {
            put_node(object, "name", &node.name);
            put_optional_list(object, "directives", node.directives.as_ref());
        }
        NodeRef::InlineFragment(node) => {
            put_optional(object, "typeCondition", node.type_condition.as_ref());
            put_optional_list(object, "directives", node.directives.as_ref());
            put_node(object, "selectionSet", &node.selection_set);
        }
        NodeRef::FragmentDefinition(node) => {
            put_optional(object, "description", node.description.as_ref());
            put_node(object, "name", &node.name);
            put_optional_list(
                object,
                "variableDefinitions",
                node.variable_definitions.as_ref(),
            );
            put_node(object, "typeCondition", &node.type_condition);
            put_optional_list(object, "directives", node.directives.as_ref());
            put_node(object, "selectionSet", &node.selection_set);
        }
        NodeRef::Directive(node) => {
            put_node(object, "name", &node.name);
            put_optional_list(object, "arguments", node.arguments.as_ref());
        }
        NodeRef::NamedType(node) => put_node(object, "name", &node.name),
        NodeRef::ListType(node) => put_node(object, "type", &*node.r#type),
        NodeRef::NonNullType(node) => put_node(object, "type", &node.r#type),
        _ => return false,
    }
    true
}

/// Inserts the fields of a value literal, and returns whether `node` was one.
fn insert_value_fields(object: &mut JsObject, node: NodeRef<'_>) -> bool {
    match node {
        NodeRef::IntValue(node) => {
            object.insert("value", node.value.as_str());
        }
        NodeRef::FloatValue(node) => {
            object.insert("value", node.value.as_str());
        }
        NodeRef::StringValue(node) => {
            object.insert("value", node.value.as_str());
            if let Some(block) = node.block {
                object.insert("block", block);
            }
        }
        NodeRef::BooleanValue(node) => {
            object.insert("value", node.value);
        }
        NodeRef::NullValue(_) => {}
        NodeRef::EnumValue(node) => {
            object.insert("value", node.value.as_str());
        }
        NodeRef::ListValue(node) => put_list(object, "values", &node.values),
        NodeRef::ObjectValue(node) => put_list(object, "fields", &node.fields),
        NodeRef::ObjectField(node) => {
            put_node(object, "name", &node.name);
            put_node(object, "value", &node.value);
        }
        _ => return false,
    }
    true
}

/// Inserts the fields of a type system definition, and returns whether `node`
/// was one.
fn insert_type_system_fields(object: &mut JsObject, node: NodeRef<'_>) -> bool {
    match node {
        NodeRef::SchemaDefinition(node) => {
            put_optional(object, "description", node.description.as_ref());
            put_optional_list(object, "directives", node.directives.as_ref());
            put_list(object, "operationTypes", &node.operation_types);
        }
        NodeRef::OperationTypeDefinition(node) => {
            object.insert("operation", node.operation.as_str());
            put_node(object, "type", &node.r#type);
        }
        NodeRef::ScalarTypeDefinition(node) => {
            put_optional(object, "description", node.description.as_ref());
            put_node(object, "name", &node.name);
            put_optional_list(object, "directives", node.directives.as_ref());
        }
        NodeRef::ObjectTypeDefinition(node) => {
            put_optional(object, "description", node.description.as_ref());
            put_node(object, "name", &node.name);
            put_optional_list(object, "interfaces", node.interfaces.as_ref());
            put_optional_list(object, "directives", node.directives.as_ref());
            put_optional_list(object, "fields", node.fields.as_ref());
        }
        NodeRef::FieldDefinition(node) => {
            put_optional(object, "description", node.description.as_ref());
            put_node(object, "name", &node.name);
            put_optional_list(object, "arguments", node.arguments.as_ref());
            put_node(object, "type", &node.r#type);
            put_optional_list(object, "directives", node.directives.as_ref());
        }
        NodeRef::InputValueDefinition(node) => {
            put_optional(object, "description", node.description.as_ref());
            put_node(object, "name", &node.name);
            put_node(object, "type", &node.r#type);
            put_optional(object, "defaultValue", node.default_value.as_ref());
            put_optional_list(object, "directives", node.directives.as_ref());
        }
        NodeRef::InterfaceTypeDefinition(node) => {
            put_optional(object, "description", node.description.as_ref());
            put_node(object, "name", &node.name);
            put_optional_list(object, "interfaces", node.interfaces.as_ref());
            put_optional_list(object, "directives", node.directives.as_ref());
            put_optional_list(object, "fields", node.fields.as_ref());
        }
        NodeRef::UnionTypeDefinition(node) => {
            put_optional(object, "description", node.description.as_ref());
            put_node(object, "name", &node.name);
            put_optional_list(object, "directives", node.directives.as_ref());
            put_optional_list(object, "types", node.types.as_ref());
        }
        NodeRef::EnumTypeDefinition(node) => {
            put_optional(object, "description", node.description.as_ref());
            put_node(object, "name", &node.name);
            put_optional_list(object, "directives", node.directives.as_ref());
            put_optional_list(object, "values", node.values.as_ref());
        }
        NodeRef::EnumValueDefinition(node) => {
            put_optional(object, "description", node.description.as_ref());
            put_node(object, "name", &node.name);
            put_optional_list(object, "directives", node.directives.as_ref());
        }
        NodeRef::InputObjectTypeDefinition(node) => {
            put_optional(object, "description", node.description.as_ref());
            put_node(object, "name", &node.name);
            put_optional_list(object, "directives", node.directives.as_ref());
            put_optional_list(object, "fields", node.fields.as_ref());
        }
        NodeRef::DirectiveDefinition(node) => {
            put_optional(object, "description", node.description.as_ref());
            put_node(object, "name", &node.name);
            put_optional_list(object, "arguments", node.arguments.as_ref());
            object.insert("repeatable", node.repeatable);
            put_list(object, "locations", &node.locations);
        }
        _ => return false,
    }
    true
}

/// Inserts the fields of a type system extension, and returns whether `node`
/// was one.
fn insert_extension_fields(object: &mut JsObject, node: NodeRef<'_>) -> bool {
    match node {
        NodeRef::SchemaExtension(node) => {
            put_optional_list(object, "directives", node.directives.as_ref());
            put_optional_list(object, "operationTypes", node.operation_types.as_ref());
        }
        NodeRef::ScalarTypeExtension(node) => {
            put_node(object, "name", &node.name);
            put_optional_list(object, "directives", node.directives.as_ref());
        }
        NodeRef::ObjectTypeExtension(node) => {
            put_node(object, "name", &node.name);
            put_optional_list(object, "interfaces", node.interfaces.as_ref());
            put_optional_list(object, "directives", node.directives.as_ref());
            put_optional_list(object, "fields", node.fields.as_ref());
        }
        NodeRef::InterfaceTypeExtension(node) => {
            put_node(object, "name", &node.name);
            put_optional_list(object, "interfaces", node.interfaces.as_ref());
            put_optional_list(object, "directives", node.directives.as_ref());
            put_optional_list(object, "fields", node.fields.as_ref());
        }
        NodeRef::UnionTypeExtension(node) => {
            put_node(object, "name", &node.name);
            put_optional_list(object, "directives", node.directives.as_ref());
            put_optional_list(object, "types", node.types.as_ref());
        }
        NodeRef::EnumTypeExtension(node) => {
            put_node(object, "name", &node.name);
            put_optional_list(object, "directives", node.directives.as_ref());
            put_optional_list(object, "values", node.values.as_ref());
        }
        NodeRef::InputObjectTypeExtension(node) => {
            put_node(object, "name", &node.name);
            put_optional_list(object, "directives", node.directives.as_ref());
            put_optional_list(object, "fields", node.fields.as_ref());
        }
        _ => return false,
    }
    true
}

/// Inserts the fields of a schema coordinate.
///
/// A kind of another part of the grammar reaches this function only after
/// every other part declined it, so the last arm is never taken by a tree the
/// parser built.
fn insert_coordinate_fields(object: &mut JsObject, node: NodeRef<'_>) {
    match node {
        NodeRef::TypeCoordinate(node) => put_node(object, "name", &node.name),
        NodeRef::MemberCoordinate(node) => {
            put_node(object, "name", &node.name);
            put_node(object, "memberName", &node.member_name);
        }
        NodeRef::ArgumentCoordinate(node) => {
            put_node(object, "name", &node.name);
            put_node(object, "fieldName", &node.field_name);
            put_node(object, "argumentName", &node.argument_name);
        }
        NodeRef::DirectiveCoordinate(node) => put_node(object, "name", &node.name),
        NodeRef::DirectiveArgumentCoordinate(node) => {
            put_node(object, "name", &node.name);
            put_node(object, "argumentName", &node.argument_name);
        }
        _ => {}
    }
}

/// Inserts the child `node` under `key`.
fn put_node<'a, T>(object: &mut JsObject, key: &str, node: &'a T)
where
    &'a T: Into<NodeRef<'a>>,
{
    object.insert(key, to_json(node));
}

/// Inserts the child `node` under `key` when the tree carries it.
fn put_optional<'a, T>(object: &mut JsObject, key: &str, node: Option<&'a T>)
where
    &'a T: Into<NodeRef<'a>>,
{
    if let Some(node) = node {
        put_node(object, key, node);
    }
}

/// Inserts the children `list` under `key` as an array.
fn put_list<'a, T>(object: &mut JsObject, key: &str, list: &'a [T])
where
    &'a T: Into<NodeRef<'a>>,
{
    let values: Vec<JsValue> = list.iter().map(to_json).collect();
    object.insert(key, values);
}

/// Inserts the children `list` under `key` when the tree carries the list.
fn put_optional_list<'a, T>(object: &mut JsObject, key: &str, list: Option<&'a Vec<T>>)
where
    &'a T: Into<NodeRef<'a>>,
{
    if let Some(list) = list {
        put_list(object, key, list);
    }
}
