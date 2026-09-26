// SPDX-License-Identifier: MIT
/*
 * aosabi-type: the typed reading of a value literal into a value of the
 * value model.
 *
 * Derived from graphql-js 16.12.0, src/utilities/valueFromAST.ts, MIT.
 */

//! A value literal read against an input type.
//!
//! The reference returns `undefined` for every literal it cannot coerce and
//! never throws for one, and the port returns [`JsValue::Undefined`] in the
//! same places. What the reference does let escape is a throw from a field
//! thunk and the invariant on a type that is no input type, and those are the
//! error of the port.

use alloc::{
    format,
    string::String,
    vec::Vec, //
};

use aosabi_json::{
    JsObject,
    JsValue, //
};
use aosabi_lang::ast::{
    ObjectFieldNode,
    ValueNode, //
};

use crate::definition::{
    DefinitionError,
    ObjMap,
    Type,
    is_non_null_type, //
};

/// Produces a JavaScript value given a GraphQL Value AST.
///
/// A GraphQL type must be provided, which is used to interpret different
/// GraphQL Value literals. Returns [`JsValue::Undefined`] when the value
/// could not be validly coerced according to the provided type.
///
/// Derived from graphql-js 16.12.0, src/utilities/valueFromAST.ts, MIT.
///
/// `value_node` is the literal, or `None` for no literal at all, which reads
/// as no value and not as null. `ty` is the input type the literal is read
/// against and `variables` the map a variable reads from. A variable whose
/// name `variables` does not hold, or holds as undefined, is a missing
/// variable, which is what `variables[name] === undefined` tests.
///
/// | GraphQL Value | JSON Value |
/// | --- | --- |
/// | Input Object | Object |
/// | List | Array |
/// | Boolean | Boolean |
/// | String | String |
/// | Int / Float | Number |
/// | Enum Value | Unknown |
/// | `NullValue` | null |
///
/// # Errors
///
/// Returns what the field thunk of an input object type throws, and the
/// reference's invariant, a plain `Error` whose message is
/// `Unexpected input type: ` followed by the type, when `ty` is no input
/// type. The reference calls `inspect` on the type there, which reads the
/// `toJSON` of a type and prints it as `String(type)`.
pub fn value_from_ast(
    value_node: Option<&ValueNode>,
    ty: &Type,
    variables: Option<&JsObject>,
) -> Result<JsValue, DefinitionError> {
    let Some(value_node) = value_node else {
        // When there is no node, then there is also no value.
        // Importantly, this is different from returning the value null.
        return Ok(JsValue::Undefined);
    };

    if let ValueNode::Variable(node) = value_node {
        let variable_name = node.name.value();
        let variable_value = variables
            .and_then(|variables| variables.get(variable_name))
            .filter(|value| !value.is_undefined());
        let Some(variable_value) = variable_value else {
            // No valid return value.
            return Ok(JsValue::Undefined);
        };
        if matches!(variable_value, JsValue::Null) && is_non_null_type(ty) {
            return Ok(JsValue::Undefined); // Invalid: intentionally return no value.
        }
        // Note: This does no further checking that this variable is correct.
        // This assumes that this query has been validated and the variable
        // usage here is of the correct type.
        return Ok(variable_value.clone());
    }

    if let Some(non_null) = ty.as_non_null() {
        if matches!(value_node, ValueNode::NullValue(_)) {
            return Ok(JsValue::Undefined); // Invalid: intentionally return no value.
        }
        return value_from_ast(Some(value_node), non_null.of_type(), variables);
    }

    if matches!(value_node, ValueNode::NullValue(_)) {
        // This is explicitly returning the value null.
        return Ok(JsValue::Null);
    }

    if let Some(list) = ty.as_list() {
        return list_from_ast(value_node, &list.of_type, variables);
    }

    if let Type::InputObject(input_object) = ty {
        let ValueNode::ObjectValue(node) = value_node else {
            return Ok(JsValue::Undefined); // Invalid: intentionally return no value.
        };
        let mut coerced_obj = JsObject::new();
        let field_nodes = key_map(&node.fields);
        for field in input_object.get_fields()?.values() {
            let field_node = field_nodes.get(&field.name);
            let present =
                field_node.filter(|field_node| !is_missing_variable(&field_node.value, variables));
            let Some(field_node) = present else {
                if !field.default_value.is_undefined() {
                    coerced_obj.insert(field.name.as_str(), field.default_value.clone());
                } else if is_non_null_type(&field.r#type) {
                    return Ok(JsValue::Undefined); // Invalid: intentionally return no value.
                }
                continue;
            };
            let field_value = value_from_ast(Some(&field_node.value), &field.r#type, variables)?;
            if field_value.is_undefined() {
                return Ok(JsValue::Undefined); // Invalid: intentionally return no value.
            }
            coerced_obj.insert(field.name.as_str(), field_value);
        }

        if input_object.is_one_of && !is_exactly_one_non_null_key(&coerced_obj) {
            return Ok(JsValue::Undefined);
        }

        return Ok(JsValue::Object(coerced_obj));
    }

    // Scalars and Enums fulfill parsing a literal value via parseLiteral().
    // Invalid values represent a failure to parse correctly, in which case
    // no value is returned.
    let result = match ty {
        Type::Scalar(scalar) => scalar.parse_literal(value_node, variables),
        Type::Enum(enum_type) => enum_type.parse_literal(value_node, variables),
        // Not reachable, all possible input types have been considered.
        _ => {
            return Err(DefinitionError::plain(format!(
                "Unexpected input type: {ty}"
            )));
        }
    };
    Ok(result.unwrap_or(JsValue::Undefined))
}

/// Reads a literal against the item type of a list.
///
/// A literal that is no list is read as a list of one.
fn list_from_ast(
    value_node: &ValueNode,
    item_type: &Type,
    variables: Option<&JsObject>,
) -> Result<JsValue, DefinitionError> {
    if let ValueNode::ListValue(node) = value_node {
        let mut coerced_values = Vec::new();
        for item_node in &node.values {
            if is_missing_variable(item_node, variables) {
                // If an array contains a missing variable, it is either coerced to
                // null or if the item type is non-null, it considered invalid.
                if is_non_null_type(item_type) {
                    return Ok(JsValue::Undefined); // Invalid: intentionally return no value.
                }
                coerced_values.push(JsValue::Null);
            } else {
                let item_value = value_from_ast(Some(item_node), item_type, variables)?;
                if item_value.is_undefined() {
                    return Ok(JsValue::Undefined); // Invalid: intentionally return no value.
                }
                coerced_values.push(item_value);
            }
        }
        return Ok(JsValue::Array(coerced_values));
    }
    let coerced_value = value_from_ast(Some(value_node), item_type, variables)?;
    if coerced_value.is_undefined() {
        return Ok(JsValue::Undefined); // Invalid: intentionally return no value.
    }
    Ok(JsValue::Array(alloc::vec![coerced_value]))
}

/// Returns whether a coerced one of object holds exactly one key and that key
/// is not null.
///
/// `Object.keys` lists the keys in the order the fields were inserted,
/// because a field name is never an array index.
fn is_exactly_one_non_null_key(coerced_obj: &JsObject) -> bool {
    let mut keys = coerced_obj.own_keys();
    let (Some((first, _)), None) = (keys.next(), keys.next()) else {
        return false; // Invalid: not exactly one key, intentionally return no value.
    };
    // Invalid: value not non-null, intentionally return no value.
    !matches!(
        coerced_obj.get(&String::from_utf8_lossy(first.as_wtf8())),
        Some(JsValue::Null)
    )
}

/// Maps the fields of an object literal by name.
///
/// Derived from graphql-js 16.12.0, src/utilities/valueFromAST.ts, MIT.
fn key_map(fields: &[ObjectFieldNode]) -> ObjMap<&ObjectFieldNode> {
    let mut result = ObjMap::default();
    for field in fields {
        result.insert(String::from(field.name.value()), field);
    }
    result
}

/// Returns true if the provided valueNode is a variable which is not defined
/// in the set of variables.
///
/// Derived from graphql-js 16.12.0, src/utilities/valueFromAST.ts, MIT.
fn is_missing_variable(value_node: &ValueNode, variables: Option<&JsObject>) -> bool {
    match value_node {
        ValueNode::Variable(node) => variables
            .and_then(|variables| variables.get(node.name.value()))
            .is_none_or(JsValue::is_undefined),
        _ => false,
    }
}
