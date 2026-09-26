// SPDX-License-Identifier: MIT
/*
 * aosabi-type: the writing of a value of the value model as a value literal
 * of the tree.
 *
 * Derived from graphql-js 16.12.0, src/utilities/astFromValue.ts, MIT.
 */

//! A value written as a literal against an input type.
//!
//! The reference turns a JavaScript value into a value node so that the SDL
//! printer can print a default value. The number it prints is
//! `String(serialized)`, which is ECMA-262 `Number::toString`, and the digits
//! come from the JSON writer through [`number_to_string`].
//!
//! A property read on the value model is an own property read. The reference
//! reads `value[field.name]` on any object, which also finds the members of
//! `Object.prototype` and of `Array.prototype` under a field named after one of
//! them, and no golden hands such a value to this function.

use alloc::{
    borrow::Cow,
    format,
    string::String,
    sync::Arc,
    vec::Vec, //
};

use aosabi_json::JsValue;
use aosabi_lang::ast::{
    BooleanValueNode,
    EnumValueNode,
    FloatValueNode,
    IntValueNode,
    ListValueNode,
    NameNode,
    NullValueNode,
    ObjectFieldNode,
    ObjectValueNode,
    StringValueNode,
    ValueNode, //
};

use crate::{
    definition::{
        DefinitionError,
        Type,
        is_enum_type,
        number_to_string, //
    },
    inspect::inspect,
    scalars::graphql_id, //
};

/// Produces a GraphQL Value AST given a JavaScript object.
///
/// Function matches JavaScript/JSON values to GraphQL AST schema format by using
/// suggested `GraphQLInputType`. A GraphQL type must be provided, which is used
/// to interpret different JavaScript values. `value` is the internal value and
/// `ty` the input type it is written against, and `None` is the `null` the
/// reference returns for a value that has no literal.
///
/// Derived from graphql-js 16.12.0, src/utilities/astFromValue.ts, MIT.
///
/// | JSON Value | GraphQL Value |
/// | --- | --- |
/// | Object | Input Object |
/// | Array | List |
/// | Boolean | Boolean |
/// | String | String / Enum Value |
/// | Number | Int / Float |
/// | Unknown | Enum Value |
/// | null | `NullValue` |
///
/// # Errors
///
/// Returns what `serialize` of a leaf type throws and what the field thunk
/// of an input object type throws, the reference's `TypeError` whose message
/// is `Cannot convert value to AST: ` followed by the inspected value and a
/// period, when a leaf serializes to neither a boolean, a finite number nor
/// a string, and the reference's invariant, a plain `Error` whose message is
/// `Unexpected input type: ` followed by the type, when `ty` is no input
/// type.
pub fn ast_from_value(value: &JsValue, ty: &Type) -> Result<Option<ValueNode>, DefinitionError> {
    if let Some(non_null) = ty.as_non_null() {
        let ast_value = ast_from_value(value, non_null.of_type())?;
        if matches!(ast_value, Some(ValueNode::NullValue(_))) {
            return Ok(None);
        }
        return Ok(ast_value);
    }

    // only explicit null, not undefined, NaN
    if matches!(value, JsValue::Null) {
        return Ok(Some(ValueNode::NullValue(NullValueNode { loc: None })));
    }

    // undefined
    if value.is_undefined() {
        return Ok(None);
    }

    // Convert JavaScript array to GraphQL list. If the GraphQLType is a list, but
    // the value is not an array, convert the value using the list's item type.
    if let Some(list) = ty.as_list() {
        let item_type = &list.of_type;
        if let JsValue::Array(items) = value {
            let mut values_nodes = Vec::new();
            for item in items {
                if let Some(item_node) = ast_from_value(item, item_type)? {
                    values_nodes.push(item_node);
                }
            }
            return Ok(Some(ValueNode::ListValue(ListValueNode {
                loc: None,
                values: values_nodes,
            })));
        }
        return ast_from_value(value, item_type);
    }

    // Populate the fields of the input object by creating ASTs from each value
    // in the JavaScript object according to the fields in the input type.
    if let Type::InputObject(input_object) = ty {
        if !is_object_like(value) {
            return Ok(None);
        }
        let mut field_nodes: Vec<ObjectFieldNode> = Vec::new();
        for field in input_object.get_fields()?.values() {
            let field_value = ast_from_value(&own_property(value, &field.name), &field.r#type)?;
            if let Some(field_value) = field_value {
                field_nodes.push(ObjectFieldNode {
                    loc: None,
                    name: NameNode::new(None, field.name.clone()),
                    value: field_value,
                });
            }
        }
        return Ok(Some(ValueNode::ObjectValue(ObjectValueNode {
            loc: None,
            fields: field_nodes,
        })));
    }

    // Since value is an internally represented value, it must be serialized
    // to an externally represented value before converting into an AST.
    let serialized = match ty {
        Type::Scalar(scalar) => scalar.serialize(value)?,
        Type::Enum(enum_type) => enum_type.serialize(value)?,
        // Not reachable, all possible types have been considered.
        _ => {
            return Err(DefinitionError::plain(format!(
                "Unexpected input type: {ty}"
            )));
        }
    };
    leaf_from_serialized(&serialized, ty)
}

/// Writes the serialized value of a leaf as its literal.
fn leaf_from_serialized(
    serialized: &JsValue,
    ty: &Type,
) -> Result<Option<ValueNode>, DefinitionError> {
    if matches!(serialized, JsValue::Null | JsValue::Undefined) {
        return Ok(None);
    }

    // Others serialize based on their corresponding JavaScript scalar types.
    if let JsValue::Bool(value) = serialized {
        return Ok(Some(ValueNode::BooleanValue(BooleanValueNode {
            loc: None,
            value: *value,
        })));
    }

    // JavaScript numbers can be Int or Float values.
    if let JsValue::Number(number) = serialized
        && number.is_finite()
    {
        let string_num = number_to_string(*number);
        return Ok(Some(if is_integer_string(&string_num) {
            ValueNode::IntValue(IntValueNode {
                loc: None,
                value: string_num,
            })
        } else {
            ValueNode::FloatValue(FloatValueNode {
                loc: None,
                value: string_num,
            })
        }));
    }

    if let JsValue::String(text) = serialized {
        // A string of the value model that holds a lone surrogate has no `str`
        // form, and the replacement character stands in for it.
        let value = String::from_utf8_lossy(text.as_wtf8()).into_owned();

        // Enum types use Enum literals.
        if is_enum_type(ty) {
            return Ok(Some(ValueNode::EnumValue(EnumValueNode {
                loc: None,
                value,
            })));
        }

        // ID types can use Int literals.
        if is_graphql_id(ty) && is_integer_string(&value) {
            return Ok(Some(ValueNode::IntValue(IntValueNode { loc: None, value })));
        }

        return Ok(Some(ValueNode::StringValue(StringValueNode {
            loc: None,
            value,
            block: None,
        })));
    }

    Err(DefinitionError::plain(format!(
        "Cannot convert value to AST: {}.",
        inspect(serialized)
    )))
}

/// Returns whether `ty` is the one `ID` scalar.
///
/// The reference compares `type === GraphQLID`, an identity comparison, and the
/// `ID` scalar of this crate is one static allocation.
fn is_graphql_id(ty: &Type) -> bool {
    ty.as_scalar()
        .is_some_and(|scalar| Arc::ptr_eq(scalar, &graphql_id()))
}

/// Returns whether `value` is an object under `isObjectLike`.
///
/// It is `typeof value == 'object' && value !== null`. An array is an object
/// under `typeof`.
///
/// Derived from graphql-js 16.12.0, src/jsutils/isObjectLike.ts, MIT.
fn is_object_like(value: &JsValue) -> bool {
    matches!(value, JsValue::Object(_) | JsValue::Array(_))
}

/// Returns `value[name]` for an object like value.
///
/// An object yields its property or undefined. An array yields its `length` under
/// that name, its one own data property a field name can spell, and undefined
/// otherwise, because a field name is never an array index.
fn own_property<'a>(value: &'a JsValue, name: &str) -> Cow<'a, JsValue> {
    match value {
        JsValue::Object(object) => object
            .get(name)
            .map_or(Cow::Owned(JsValue::Undefined), Cow::Borrowed),
        JsValue::Array(items) if name == "length" => {
            // The length of an array is far below 2^53, so the conversion is
            // exact.
            #[expect(
                clippy::cast_precision_loss,
                reason = "the length of an array is a double in the reference"
            )]
            let length = items.len() as f64;
            Cow::Owned(JsValue::Number(length))
        }
        _ => Cow::Owned(JsValue::Undefined),
    }
}

/// Returns whether `text` matches `/^-?(?:0|[1-9][0-9]*)$/`.
///
/// The grammar of an `IntValue`.
///
/// Derived from graphql-js 16.12.0, src/utilities/astFromValue.ts, MIT.
fn is_integer_string(text: &str) -> bool {
    let digits = text.strip_prefix('-').unwrap_or(text);
    match digits.as_bytes() {
        [b'0'] => true,
        [b'1'..=b'9', rest @ ..] => rest.iter().all(u8::is_ascii_digit),
        _ => false,
    }
}
