// SPDX-License-Identifier: MIT
/*
 * aosabi-federation: the helpers of @graphql-tools/utils 11.0.0 the
 * delegation calls.
 *
 * Derived from @graphql-tools/utils 11.0.0, cjs, MIT.
 */

//! The helpers of `@graphql-tools/utils`, `create_graphql_error`,
//! `relocated_error` and the five the delegation calls.
//!
//! [`create_graphql_error`] is the constructor of `errors.js:27-44` that the
//! executor already ports, so this module names it again and writes it once.
//! [`relocated_error`] copies an error to another response path, which is how
//! a remote error reaches the path of the front. The helpers of
//! `rootTypes.js`, [`get_root_type_map`], [`get_root_type_names`] and
//! [`get_defined_root_type`], answer the root types of a schema in the order
//! query, mutation, subscription, which is the insertion order of the `Map`
//! of the reference. [`get_response_key_from_info`],
//! [`get_operation_ast_from_request`], [`ast_from_value_untyped`] and
//! [`implements_abstract_type`] are the other helpers the call graph of the
//! delegation reaches, and [`create_graphql_error_from_object`] rebuilds an
//! error a remote wrote as JSON. Beside them stand the reads the delegation
//! makes of a thrown value or of a test of JavaScript, [`error_path`],
//! [`error_message`] and [`is_truthy`], and the two tests of a named type,
//! [`is_composite`] and [`is_abstract`], which every module of the delegation
//! shares. Three helpers of the crate stand here once for every module that
//! reads them: the member of a value, the test `== null` of a JSON value and
//! the byte comparison a constant can evaluate.
//!
//! An error here is a [`Value`], the value the reference throws or carries
//! in a result, so its identity is the `Rc` behind it. The functions
//! `isGraphQLErrorLike`, `getSchemaCoordinate` and `locatedError` of
//! `errors.js` and the memo of `memoize1` around the root type helpers are
//! left out: no caller of the delegation reads a coordinate, the executor
//! ports `locatedError`, and the memo keeps a pure answer per schema.

use std::{
    hash::{
        BuildHasher,
        Hasher, //
    },
    rc::Rc, //
};

pub use aosabi_execute::errors::create_graphql_error;
use aosabi_execute::{
    errors::ThrownError,
    execute::ResolveInfo,
    value::{
        ErrorObject,
        Value,
        to_js_string, //
    }, //
};
use aosabi_json::{
    JsObject,
    JsString,
    JsValue,
    number::number_to_string, //
};
use aosabi_lang::{
    GraphQLErrorOptions,
    OriginalError,
    PathSegment,
    ast::{
        AstNode,
        BooleanValueNode,
        FloatValueNode,
        IntValueNode,
        ListValueNode,
        NameNode,
        NullValueNode,
        ObjectFieldNode,
        ObjectValueNode,
        OperationDefinitionNode,
        OperationTypeNode,
        StringValueNode,
        ValueNode, //
    }, //
};
use aosabi_type::{
    NamedType,
    Schema,
    validate::do_types_overlap, //
};

use crate::transforms::ExecutionRequest;

/// Returns a copy of `original_error` that carries `path`, or its own path when `path` is `None`.
///
/// The copy keeps the message, the nodes, the source, the positions and the
/// extensions, and the original error is the error itself. An error that is
/// no `GraphQLError` carries none of these but its message, and an object
/// that is no error, which an error becomes once
/// [󰀅](crate::delegate::transformer::remove_overlapping_aliases) copied it,
/// carries its own `message`, `path` and `extensions` members.
#[must_use]
pub fn relocated_error(original_error: &Value, path: Option<Vec<PathSegment>>) -> Value {
    let error = match original_error {
        Value::Error(error) => match error.as_ref() {
            ErrorObject::GraphQL(error) => create_graphql_error(
                error.message.clone(),
                GraphQLErrorOptions {
                    nodes: error.nodes.clone(),
                    source: error.source.clone(),
                    positions: error.positions.clone(),
                    path: path.or_else(|| error.path.clone()),
                    original_error: Some(OriginalError::from(
                        aosabi_execute::errors::copy_graphql_error(error),
                    )),
                    extensions: Some(error.extensions.clone()),
                },
            ),
            other => create_graphql_error(
                other.message(),
                GraphQLErrorOptions {
                    path,
                    original_error: Some(thrown_original(other)),
                    extensions: other.prop("extensions").and_then(object_of),
                    ..GraphQLErrorOptions::default()
                },
            ),
        },
        other => create_graphql_error(
            member_message(other),
            GraphQLErrorOptions {
                path: path.or_else(|| error_path(other)),
                extensions: member(other, "extensions")
                    .to_js()
                    .as_object_value()
                    .cloned(),
                ..GraphQLErrorOptions::default()
            },
        ),
    };
    Value::from(ErrorObject::GraphQL(error))
}

/// Returns `createGraphQLError(error.message, error)` of an error object that is no `GraphQLError`.
///
/// The message is `String(error.message)`, empty when absent, and the
/// constructor of graphql 16 takes the `path` and the `extensions` of the
/// object, which a remote writes, and computes `locations` from nodes and
/// positions, which a JSON error has none of, so the `locations` the remote
/// wrote is not kept. The members `nodes`, `source`, `positions` and
/// `originalError` of a JSON object hold no node, no source and no error, and
/// are not read. An `extensions` member that is no object is not kept.
#[must_use]
pub fn create_graphql_error_from_object(error: &Value) -> Value {
    Value::from(ErrorObject::GraphQL(create_graphql_error(
        member_message(error),
        GraphQLErrorOptions {
            path: error_path(error),
            extensions: member(error, "extensions")
                .to_js()
                .as_object_value()
                .cloned(),
            ..GraphQLErrorOptions::default()
        },
    )))
}

/// Returns the `path` member of an error or of an error like object, `error.path`.
///
/// A `GraphQLError` answers its path. An object answers its `path` member
/// when that is a list of keys and indices, and every other value answers
/// none.
#[must_use]
pub fn error_path(error: &Value) -> Option<Vec<PathSegment>> {
    match error {
        Value::Error(error) => match error.as_ref() {
            ErrorObject::GraphQL(error) => error.path.clone(),
            _ => None,
        },
        Value::Object(_) => match member(error, "path") {
            Value::List(segments) => segments.iter().map(path_segment).collect(),
            _ => None,
        },
        _ => None,
    }
}

/// Returns the `message` member of an error or of an error like object, `error.message`.
///
/// A member that is no string is converted with `String`, and an absent one
/// is the empty string, which is what the constructor of `Error` makes of
/// `undefined`.
#[must_use]
pub fn error_message(error: &Value) -> JsString {
    match error {
        Value::Error(error) => error.message(),
        other => member_message(other),
    }
}

/// Returns the segment `index` of the path of an error, `error.path?.[index]`.
///
/// A `GraphQLError` answers the one segment of the path it holds, read in
/// place. An object answers the segment of its `path` member as
/// [`error_path`] reads that member whole, so a member with one segment
/// that is no key and no index answers none at every index.
#[must_use]
pub fn error_path_segment(error: &Value, index: usize) -> Option<PathSegment> {
    match error {
        Value::Error(object) => match object.as_ref() {
            ErrorObject::GraphQL(error) => error.path.as_ref()?.get(index).cloned(),
            _ => None,
        },
        other => error_path(other).and_then(|path| path.into_iter().nth(index)),
    }
}

/// Returns the key under which the result of this resolver will be placed in the response JSON.
///
/// Basically, just resolves aliases. Answers the alias or the name itself, so
/// the key is borrowed from `info`.
#[must_use]
pub fn get_response_key_from_info(info: &ResolveInfo) -> &str {
    match info
        .field_nodes
        .first()
        .and_then(|node| node.alias.as_ref())
    {
        Some(alias) => alias.value(),
        None => &info.field_name,
    }
}

/// Returns the root types of `schema` by operation, in the order query, mutation, subscription.
#[must_use]
pub fn get_root_type_map(schema: &Schema) -> Vec<(OperationTypeNode, NamedType)> {
    [
        OperationTypeNode::Query,
        OperationTypeNode::Mutation,
        OperationTypeNode::Subscription,
    ]
    .into_iter()
    .filter_map(|operation| {
        schema
            .get_root_type(operation)
            .map(|root_type| (operation, root_type.clone()))
    })
    .collect()
}

/// Returns the names of the root types of `schema`, without a repeated name.
#[must_use]
pub fn get_root_type_names(schema: &Schema) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for (_, root_type) in get_root_type_map(schema) {
        if !names.iter().any(|name| name == root_type.name()) {
            names.push(String::from(root_type.name()));
        }
    }
    names
}

/// Returns the root type of `operation` in `schema`.
///
/// # Errors
///
/// Returns the `GraphQLError` `Schema is not configured to execute
/// <operation> operation.` blaming `nodes` when the schema has no such root
/// type.
pub fn get_defined_root_type(
    schema: &Schema,
    operation: OperationTypeNode,
    nodes: Option<Vec<AstNode>>,
) -> Result<NamedType, Value> {
    schema.get_root_type(operation).cloned().ok_or_else(|| {
        Value::from(ErrorObject::GraphQL(create_graphql_error(
            format!(
                "Schema is not configured to execute {} operation.",
                operation.as_str()
            ),
            GraphQLErrorOptions {
                nodes,
                ..GraphQLErrorOptions::default()
            },
        )))
    })
}

/// Returns the operation of the request's document the request's operation name selects.
///
/// The memo of `memoize1` keeps the same answer per request object, so it has
/// no reading.
///
/// # Errors
///
/// Returns the plain error `Cannot infer operation <name>` when the document
/// holds no such operation, or several operations and no name.
pub fn get_operation_ast_from_request(
    request: &ExecutionRequest,
) -> Result<&OperationDefinitionNode, Value> {
    let operation_name = request.operation_name.as_deref();
    aosabi_execute::normalized_executor::get_operation_ast(&request.document, operation_name)
        .ok_or_else(|| {
            Value::from(ErrorObject::error(format!(
                "Cannot infer operation {}",
                operation_name.unwrap_or("")
            )))
        })
}

/// Produces a GraphQL Value AST given a JavaScript object.
///
/// Function will match JavaScript/JSON values to GraphQL AST schema format,
/// an object to an input object, an array to a list, a boolean, a string, a
/// number to an int or a float and `null` to a null value. `undefined`
/// answers `None`, and an item or a member whose node is `None` is left out.
/// A host object carried through unread, the file of an upload, answers an
/// input object with no field. The reference walks a `File` with `for...in`,
/// which yields `name`, `lastModified`, `type` and `size` and then the
/// methods of a `Blob`, so it throws `Cannot convert value to AST` at the
/// method `slice`. A file reaches this function only as an argument the
/// target field does not declare, which no root field of a federation pair
/// meets.
///
/// # Errors
///
/// Returns the `TypeError` `Cannot convert value to AST: <value>.` for a
/// number that is not finite.
pub fn ast_from_value_untyped(value: &JsValue) -> Result<Option<ValueNode>, Value> {
    let node = match value {
        // Only an explicit null, not undefined and not NaN.
        JsValue::Null => ValueNode::NullValue(NullValueNode { loc: None }),
        JsValue::Undefined => return Ok(None),
        JsValue::Array(items) => {
            let mut values = Vec::new();
            for item in items {
                if let Some(item_node) = ast_from_value_untyped(item)? {
                    values.push(item_node);
                }
            }
            ValueNode::ListValue(ListValueNode { loc: None, values })
        }
        JsValue::Object(object) => ValueNode::ObjectValue(ObjectValueNode {
            loc: None,
            fields: object_fields(object)?,
        }),
        JsValue::Opaque(_) => ValueNode::ObjectValue(ObjectValueNode {
            loc: None,
            fields: Vec::new(),
        }),
        JsValue::Bool(value) => ValueNode::BooleanValue(BooleanValueNode {
            loc: None,
            value: *value,
        }),
        // JavaScript numbers can be Int or Float values.
        JsValue::Number(number) if number.is_finite() => {
            let string_num = number_to_string(*number);
            if is_integer_string(&string_num) {
                ValueNode::IntValue(IntValueNode {
                    loc: None,
                    value: string_num,
                })
            } else {
                ValueNode::FloatValue(FloatValueNode {
                    loc: None,
                    value: string_num,
                })
            }
        }
        JsValue::String(string) => ValueNode::StringValue(StringValueNode {
            loc: None,
            value: String::from_utf8_lossy(string.as_wtf8()).into_owned(),
            block: None,
        }),
        JsValue::Number(number) => {
            return Err(Value::from(ErrorObject::type_error(format!(
                "Cannot convert value to AST: {}.",
                number_to_string(*number)
            ))));
        }
    };
    Ok(Some(node))
}

/// Returns whether `type_b` can stand where `type_a` stands, the two being one type or overlapping.
///
/// A missing type answers `false`, and a type that is no composite type
/// overlaps nothing but itself.
///
/// # Errors
///
/// Returns what the member thunk of a union throws, as a thrown value.
pub fn implements_abstract_type(
    schema: &Schema,
    type_a: Option<&NamedType>,
    type_b: Option<&NamedType>,
) -> Result<bool, Value> {
    let (Some(type_a), Some(type_b)) = (type_a, type_b) else {
        return Ok(false);
    };
    if type_a == type_b {
        return Ok(true);
    }
    if is_composite(type_a) && is_composite(type_b) {
        return do_types_overlap(schema, type_a, type_b)
            .map_err(aosabi_execute::errors::thrown_definition_error);
    }
    Ok(false)
}

/// Returns whether a value is truthy, ECMA-262 `ToBoolean`, the test of `if (value)`.
#[must_use]
pub fn is_truthy(value: &Value) -> bool {
    match value {
        Value::Undefined | Value::Null => false,
        Value::Bool(boolean) => *boolean,
        Value::Number(number) => *number != 0.0 && !number.is_nan(),
        Value::String(string) => !string.is_empty(),
        _ => true,
    }
}

/// Returns whether a named type is an object, an interface or a union, `isCompositeType`.
#[must_use]
pub fn is_composite(named_type: &NamedType) -> bool {
    matches!(
        named_type,
        NamedType::Object(_) | NamedType::Interface(_) | NamedType::Union(_)
    )
}

/// Returns whether a named type is an interface or a union, `isAbstractType`.
#[must_use]
pub fn is_abstract(named_type: &NamedType) -> bool {
    matches!(named_type, NamedType::Interface(_) | NamedType::Union(_))
}

/// Returns the object fields of an input object, each member in key order with a node.
///
/// The `toJSON` branch of the reference reads a `Date` and other objects
/// with that method, and an object of the JSON model has none.
fn object_fields(object: &JsObject) -> Result<Vec<ObjectFieldNode>, Value> {
    let mut field_nodes = Vec::new();
    for (key, field_value) in object.own_keys() {
        if let Some(ast) = ast_from_value_untyped(field_value)? {
            field_nodes.push(ObjectFieldNode {
                loc: None,
                name: NameNode::new(None, String::from_utf8_lossy(key.as_wtf8())),
                value: ast,
            });
        }
    }
    Ok(field_nodes)
}

/// Returns whether `text` is an `IntValue`, `-?(?:0|[1-9][0-9]*)`.
fn is_integer_string(text: &str) -> bool {
    let digits = text.strip_prefix('-').unwrap_or(text);
    match digits.as_bytes() {
        [b'0'] => true,
        [first, rest @ ..] => (b'1'..=b'9').contains(first) && rest.iter().all(u8::is_ascii_digit),
        [] => false,
    }
}

/// Returns `value?.[key]`, the member `key` of an object value and `undefined` for any other value.
pub(crate) fn member(value: &Value, key: &str) -> Value {
    match value {
        Value::Object(object) => Rc::clone(object).get(key),
        _ => Value::Undefined,
    }
}

/// Returns whether a JSON value is `null` or `undefined`, the test `value == null`.
pub(crate) fn is_nullish(value: &JsValue) -> bool {
    matches!(value, JsValue::Undefined | JsValue::Null)
}

/// Returns whether two byte strings are equal, which a constant can evaluate.
pub(crate) const fn same_bytes(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut index = 0;
    while index < left.len() {
        if left[index] != right[index] {
            return false;
        }
        index += 1;
    }
    true
}

/// The builder of [`AddressHasher`], for a table keyed by the address of an allocation.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct AddressState;

impl BuildHasher for AddressState {
    type Hasher = AddressHasher;

    fn build_hasher(&self) -> AddressHasher {
        AddressHasher(0)
    }
}

/// The hasher of an address, which spreads every bit of the address over the hash.
///
/// A table keyed by the address of an object needs no keyed hash, since no
/// client picks an address, and the `SipHash-1-3` of a lookup takes 7.180 of
/// the 8.324 ns of a miss in a map of 64 keys with the fixed hasher. It needs
/// the bits spread: the index table takes its tag from the top seven bits of
/// a hash and its first slot from the low bits, and an allocation is taken to
/// be aligned to 8 or 16 bytes at an address below 2^48, which sets neither,
/// so the address itself would give every key one tag and a part of the
/// slots. The mix is the finalizer of `SplitMix64`, a bijection of 64 bits in
/// which every bit of the input moves every bit of the output, so the low
/// half a 32 bit target keeps is spread as well. A key of two addresses, a
/// pair, folds the first into the state before it adds the second.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct AddressHasher(u64);

impl Hasher for AddressHasher {
    fn finish(&self) -> u64 {
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 = self.0.rotate_left(8) ^ u64::from(*byte);
        }
    }

    fn write_usize(&mut self, address: usize) {
        self.0 = self
            .0
            .wrapping_mul(0x9e37_79b9_7f4a_7c15)
            .wrapping_add(address as u64);
    }
}

/// Returns `String(object.message)`, the empty string for an absent member.
fn member_message(value: &Value) -> JsString {
    match member(value, "message") {
        Value::Undefined => JsString::from(""),
        message => to_js_string(&message),
    }
}

/// Returns the path segment a value of a path list is.
fn path_segment(value: &Value) -> Option<PathSegment> {
    match value {
        Value::String(key) => Some(PathSegment::Key(
            String::from_utf8_lossy(key.as_wtf8()).into_owned(),
        )),
        // A list index of a path is a non negative integer below 2^53.
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "the guard admits a non negative integer below 2^53 alone"
        )]
        Value::Number(index) if index.fract() == 0.0 && *index >= 0.0 && *index < 9e15 => {
            Some(PathSegment::Index(*index as usize))
        }
        _ => None,
    }
}

/// Returns the object a member holds, when it holds one.
fn object_of(value: &JsValue) -> Option<JsObject> {
    value.as_object_value().cloned()
}

/// Returns the original error a relocated copy of an error that is no `GraphQLError` keeps.
fn thrown_original(error: &ErrorObject) -> OriginalError {
    OriginalError::Other(Box::new(ThrownError {
        name: error.name(),
        message: error.message(),
        host: None,
    }))
}

/// The object a value of the JSON model holds.
trait AsObjectValue {
    /// Returns the object, when the value is one.
    fn as_object_value(&self) -> Option<&JsObject>;
}

impl AsObjectValue for JsValue {
    fn as_object_value(&self) -> Option<&JsObject> {
        match self {
            JsValue::Object(object) => Some(object),
            _ => None,
        }
    }
}
