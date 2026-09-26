// SPDX-License-Identifier: MIT
/*
 * aosabi-type: the five specified scalars and their coercion functions.
 *
 * Derived from graphql-js 16.12.0, src/type/scalars.ts, MIT.
 */

//! The specified scalar types of the type layer.
//!
//! Each scalar is a module level constant in the reference. Here its name,
//! its description and its three coercion functions are a row of read only
//! data, and one static holds the five types built from the rows on first
//! use. A function per scalar returns the one shared allocation, so that the
//! `Boolean` a directive argument refers to and the `Boolean` a schema lists
//! are the same type, as they are in the reference.
//!
//! The coercion functions read a [`JsValue`], and each `typeof` branch of the
//! reference is one variant of it. `serializeObject` unwraps an object with a
//! custom `valueOf` or `toJSON`, and a plain object of the value model has
//! neither, so it returns its argument. Every message is the reference's byte
//! for byte, with `inspect` printing the value and `print` printing the node.
//! A `parseValue` answers [`Parsed::Input`] where the reference returns
//! `inputValue`, every accepted value but the integer an `ID` turns into its
//! text, so the coercion keeps an accepted string with no copy.
//!
//! The constructor of the reference can fail only in `assertName`, and the
//! five names are literals that pass it, so each type is written directly
//! from its row with the defaults the constructor would fill in.

use alloc::{
    boxed::Box,
    format,
    string::String,
    sync::Arc,
    vec::Vec, //
};

use aosabi_json::{
    JsObject,
    JsValue, //
};
use aosabi_lang::{
    GraphQLError,
    ast::ValueNode,
    print, //
};
use once_cell::race::OnceBox;

use crate::{
    definition::{
        DefinitionError,
        LiteralFn,
        NamedType,
        ParseFn,
        Parsed,
        ScalarLiteralParser,
        ScalarSerializer,
        ScalarType,
        ScalarValueParser,
        ValueFn,
        blame,
        number_to_string,
        parse_number_token, //
    },
    inspect::inspect, //
};

/// Maximum possible Int value as per GraphQL Spec (32-bit signed integer).
///
/// The reference keeps it as a number, and the comparisons against it are
/// double comparisons.
pub const GRAPHQL_MAX_INT: f64 = 2_147_483_647.0;

/// Minimum possible Int value as per GraphQL Spec (32-bit signed integer).
///
/// The reference keeps it as a number.
pub const GRAPHQL_MIN_INT: f64 = -2_147_483_648.0;

/// The magnitude from which every finite double is an integer, 2^52.
const INTEGER_MAGNITUDE: f64 = 4_503_599_627_370_496.0;

/// Returns `Number.isInteger(value)`.
///
/// A finite double whose truncation is itself is an integer. Every finite
/// double of magnitude 2^52 or more is one, because the significand has no
/// fraction bits left there, and below that magnitude the round trip through
/// an `i64` is exact and truncates toward zero.
///
/// [isinteger]: https://tc39.es/ecma262/#sec-number.isinteger
fn is_integer(value: f64) -> bool {
    if !value.is_finite() {
        return false;
    }
    if value.abs() >= INTEGER_MAGNITUDE {
        return true;
    }
    // The magnitude is below 2^52, so the cast neither saturates nor loses
    // an integer part, and its truncation is the comparison the clause
    // makes.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::float_cmp,
        reason = "the truncation and the exact comparison are the integer test of the clause"
    )]
    let is_integer = value as i64 as f64 == value;
    is_integer
}

/// Returns whether `num` lies outside the `Int` range.
///
/// This is the comparison the three `Int` coercions make, kept in its shape
/// because it answers false for `NaN` where a range test answers true.
#[expect(
    clippy::manual_range_contains,
    reason = "the two comparisons are the reference's and differ from a range test on NaN"
)]
fn is_outside_int_range(num: f64) -> bool {
    num > GRAPHQL_MAX_INT || num < GRAPHQL_MIN_INT
}

/// Returns `Number.isFinite(value)` for a value of any type.
///
/// The clause answers `false` for anything but a finite number, without
/// converting its argument.
///
/// [isfinite]: https://tc39.es/ecma262/#sec-number.isfinite
fn is_finite_number(value: &JsValue) -> Option<f64> {
    match value {
        JsValue::Number(number) if number.is_finite() => Some(*number),
        _ => None,
    }
}

/// Returns a `GraphQLError` that blames `value_node`.
fn error_at(message: String, value_node: &ValueNode) -> DefinitionError {
    DefinitionError::from(GraphQLError::new(message, blame(value_node)))
}

/// Returns whether `c` is a `StrWhiteSpaceChar`.
///
/// The set is `WhiteSpace` and `LineTerminator` together, which is the
/// Unicode `White_Space` property with the zero width no break space added
/// and the next line character left out. `char::is_whitespace` is the
/// `White_Space` property.
///
/// [stn]: https://tc39.es/ecma262/#sec-stringtonumber
fn is_str_white_space(c: char) -> bool {
    c == '\u{FEFF}' || (c.is_whitespace() && c != '\u{0085}')
}

/// Returns whether every byte of `text` is an ASCII digit.
///
/// An empty text passes, and the caller decides whether that is allowed.
fn all_decimal_digits(text: &str) -> bool {
    text.bytes().all(|byte| byte.is_ascii_digit())
}

/// Returns whether `text` is a `StrDecimalLiteral`.
///
/// The grammar is an optional sign, then `Infinity`, or decimal digits with
/// an optional fraction, or a fraction alone, each with an optional exponent.
/// A text that passes is also a text the Rust parser reads with the same
/// rounding.
///
/// [stn]: https://tc39.es/ecma262/#sec-stringtonumber
fn is_str_decimal_literal(text: &str) -> bool {
    let unsigned = text.strip_prefix(['+', '-']).unwrap_or(text);
    if unsigned == "Infinity" {
        return true;
    }

    let (mantissa, exponent) = match unsigned.find(['e', 'E']) {
        Some(at) => (&unsigned[..at], Some(&unsigned[at + 1..])),
        None => (unsigned, None),
    };
    if let Some(exponent) = exponent {
        let digits = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
        if digits.is_empty() || !all_decimal_digits(digits) {
            return false;
        }
    }

    match mantissa.split_once('.') {
        None => !mantissa.is_empty() && all_decimal_digits(mantissa),
        Some((integer, fraction)) => {
            !(integer.is_empty() && fraction.is_empty())
                && all_decimal_digits(integer)
                && all_decimal_digits(fraction)
        }
    }
}

/// Returns the value of a `NonDecimalIntegerLiteral` after its prefix.
///
/// The value is the mathematical value of the digits rounded once to a
/// double. The digits accumulate exactly while they fit a `u128`, and a
/// longer literal accumulates in doubles, which may round differently after
/// the point where it no longer fits.
///
/// [stn]: https://tc39.es/ecma262/#sec-stringtonumber
fn non_decimal_integer(digits: &str, radix: u32) -> f64 {
    if digits.is_empty() {
        return f64::NAN;
    }
    let mut exact: Option<u128> = Some(0);
    let mut approximate = 0.0;
    for c in digits.chars() {
        let Some(digit) = c.to_digit(radix) else {
            return f64::NAN;
        };
        exact = exact.and_then(|value| {
            value
                .checked_mul(u128::from(radix))
                .and_then(|value| value.checked_add(u128::from(digit)))
        });
        approximate = approximate * f64::from(radix) + f64::from(digit);
    }
    // The conversion rounds to the nearest double, which is the one rounding
    // the clause performs.
    #[expect(
        clippy::cast_precision_loss,
        reason = "the rounding to the nearest double is the clause"
    )]
    exact.map_or(approximate, |value| value as f64)
}

/// Returns `Number(text)`, ECMA-262 `StringToNumber`.
///
/// White space around the literal is trimmed, an empty remainder is zero, a
/// `0x`, `0o` or `0b` prefix introduces an unsigned non decimal integer, and
/// anything else is a decimal literal or `NaN`.
///
/// [stn]: https://tc39.es/ecma262/#sec-stringtonumber
#[must_use]
pub fn string_to_number(text: &str) -> f64 {
    let trimmed = text.trim_matches(is_str_white_space);
    if trimmed.is_empty() {
        return 0.0;
    }
    let non_decimal = trimmed.strip_prefix('0').and_then(|rest| {
        let digits = rest.get(1..).unwrap_or_default();
        match rest.as_bytes().first() {
            Some(b'x' | b'X') => Some((digits, 16)),
            Some(b'o' | b'O') => Some((digits, 8)),
            Some(b'b' | b'B') => Some((digits, 2)),
            _ => None,
        }
    });
    if let Some((digits, radix)) = non_decimal {
        return non_decimal_integer(digits, radix);
    }
    if !is_str_decimal_literal(trimmed) {
        return f64::NAN;
    }
    let (negative, unsigned) = match trimmed.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, trimmed.strip_prefix('+').unwrap_or(trimmed)),
    };
    if unsigned == "Infinity" {
        return if negative {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
    }
    trimmed.parse::<f64>().unwrap_or(f64::NAN)
}

/// Returns the value a custom `valueOf` or `toJSON` would unwrap.
///
/// A plain object's `valueOf` returns the object itself, which is object like
/// and therefore not returned, and a plain object and an array carry no
/// `toJSON`, so the value model has nothing this function unwraps and it
/// returns its argument.
///
/// Derived from graphql-js 16.12.0, src/type/scalars.ts, MIT.
fn serialize_object(output_value: &JsValue) -> &JsValue {
    output_value
}

/// Returns the number a serializer reads from a coerced value.
///
/// This is the `let num = coercedValue` of the two numeric serializers, where
/// a non empty string is converted with `Number`.
fn serializer_number(coerced_value: &JsValue) -> Option<f64> {
    match coerced_value {
        // A string with a lone surrogate is no numeric literal, so it is
        // `NaN` as `Number` makes it.
        JsValue::String(text) if !text.is_empty() => {
            Some(text.as_str().map_or(f64::NAN, string_to_number))
        }
        JsValue::Number(number) => Some(*number),
        _ => None,
    }
}

/// Serializes a value as an `Int`.
///
/// Derived from graphql-js 16.12.0, src/type/scalars.ts, MIT.
fn int_serialize(output_value: &JsValue) -> Result<JsValue, DefinitionError> {
    let coerced_value = serialize_object(output_value);

    if let JsValue::Bool(boolean) = coerced_value {
        return Ok(JsValue::Number(if *boolean { 1.0 } else { 0.0 }));
    }

    let num = serializer_number(coerced_value).filter(|num| is_integer(*num));
    let Some(num) = num else {
        return Err(DefinitionError::graphql(format!(
            "Int cannot represent non-integer value: {}",
            inspect(coerced_value)
        )));
    };
    if is_outside_int_range(num) {
        return Err(DefinitionError::graphql(format!(
            "Int cannot represent non 32-bit signed integer value: {}",
            inspect(coerced_value)
        )));
    }
    Ok(JsValue::Number(num))
}

/// Parses an external value as an `Int`.
///
/// Derived from graphql-js 16.12.0, src/type/scalars.ts, MIT.
fn int_parse_value(input_value: &JsValue) -> Result<Parsed, DefinitionError> {
    let num = match input_value {
        JsValue::Number(number) if is_integer(*number) => *number,
        _ => {
            return Err(DefinitionError::graphql(format!(
                "Int cannot represent non-integer value: {}",
                inspect(input_value)
            )));
        }
    };
    if is_outside_int_range(num) {
        return Err(DefinitionError::graphql(format!(
            "Int cannot represent non 32-bit signed integer value: {}",
            number_to_string(num)
        )));
    }
    Ok(Parsed::Input)
}

/// Parses a literal as an `Int`.
///
/// Derived from graphql-js 16.12.0, src/type/scalars.ts, MIT.
fn int_parse_literal(
    value_node: &ValueNode,
    _variables: Option<&JsObject>,
) -> Result<JsValue, DefinitionError> {
    let ValueNode::IntValue(node) = value_node else {
        return Err(error_at(
            format!(
                "Int cannot represent non-integer value: {}",
                print(value_node)
            ),
            value_node,
        ));
    };
    let num = parse_number_token(&node.value);
    if is_outside_int_range(num) {
        return Err(error_at(
            format!(
                "Int cannot represent non 32-bit signed integer value: {}",
                node.value
            ),
            value_node,
        ));
    }
    Ok(JsValue::Number(num))
}

/// Serializes a value as a `Float`.
///
/// Derived from graphql-js 16.12.0, src/type/scalars.ts, MIT.
fn float_serialize(output_value: &JsValue) -> Result<JsValue, DefinitionError> {
    let coerced_value = serialize_object(output_value);

    if let JsValue::Bool(boolean) = coerced_value {
        return Ok(JsValue::Number(if *boolean { 1.0 } else { 0.0 }));
    }

    let num = serializer_number(coerced_value).filter(|num| num.is_finite());
    let Some(num) = num else {
        return Err(DefinitionError::graphql(format!(
            "Float cannot represent non numeric value: {}",
            inspect(coerced_value)
        )));
    };
    Ok(JsValue::Number(num))
}

/// Parses an external value as a `Float`.
///
/// Derived from graphql-js 16.12.0, src/type/scalars.ts, MIT.
fn float_parse_value(input_value: &JsValue) -> Result<Parsed, DefinitionError> {
    if is_finite_number(input_value).is_none() {
        return Err(DefinitionError::graphql(format!(
            "Float cannot represent non numeric value: {}",
            inspect(input_value)
        )));
    }
    Ok(Parsed::Input)
}

/// Parses a literal as a `Float`.
///
/// Derived from graphql-js 16.12.0, src/type/scalars.ts, MIT.
fn float_parse_literal(
    value_node: &ValueNode,
    _variables: Option<&JsObject>,
) -> Result<JsValue, DefinitionError> {
    let value = match value_node {
        ValueNode::FloatValue(node) => &node.value,
        ValueNode::IntValue(node) => &node.value,
        _ => {
            return Err(error_at(
                format!(
                    "Float cannot represent non numeric value: {}",
                    print(value_node)
                ),
                value_node,
            ));
        }
    };
    Ok(JsValue::Number(parse_number_token(value)))
}

/// Serializes a value as a `String`.
///
/// A string, a boolean and a finite number become a string, and no other type
/// is coerced.
///
/// Derived from graphql-js 16.12.0, src/type/scalars.ts, MIT.
fn string_serialize(output_value: &JsValue) -> Result<JsValue, DefinitionError> {
    let coerced_value = serialize_object(output_value);

    match coerced_value {
        JsValue::String(_) => Ok(coerced_value.clone()),
        JsValue::Bool(boolean) => Ok(JsValue::from(if *boolean { "true" } else { "false" })),
        JsValue::Number(number) if number.is_finite() => {
            Ok(JsValue::from(number_to_string(*number)))
        }
        _ => Err(DefinitionError::graphql(format!(
            "String cannot represent value: {}",
            inspect(output_value)
        ))),
    }
}

/// Parses an external value as a `String`.
///
/// Derived from graphql-js 16.12.0, src/type/scalars.ts, MIT.
fn string_parse_value(input_value: &JsValue) -> Result<Parsed, DefinitionError> {
    if !matches!(input_value, JsValue::String(_)) {
        return Err(DefinitionError::graphql(format!(
            "String cannot represent a non string value: {}",
            inspect(input_value)
        )));
    }
    Ok(Parsed::Input)
}

/// Parses a literal as a `String`.
///
/// Derived from graphql-js 16.12.0, src/type/scalars.ts, MIT.
fn string_parse_literal(
    value_node: &ValueNode,
    _variables: Option<&JsObject>,
) -> Result<JsValue, DefinitionError> {
    let ValueNode::StringValue(node) = value_node else {
        return Err(error_at(
            format!(
                "String cannot represent a non string value: {}",
                print(value_node)
            ),
            value_node,
        ));
    };
    Ok(JsValue::from(node.value.as_str()))
}

/// Serializes a value as a `Boolean`.
///
/// A finite number is true when it is not zero, and `-0 !== 0` is false, so a
/// negative zero is false as well.
///
/// Derived from graphql-js 16.12.0, src/type/scalars.ts, MIT.
fn boolean_serialize(output_value: &JsValue) -> Result<JsValue, DefinitionError> {
    let coerced_value = serialize_object(output_value);

    if let JsValue::Bool(_) = coerced_value {
        return Ok(coerced_value.clone());
    }
    if let Some(number) = is_finite_number(coerced_value) {
        return Ok(JsValue::Bool(number != 0.0));
    }
    Err(DefinitionError::graphql(format!(
        "Boolean cannot represent a non boolean value: {}",
        inspect(coerced_value)
    )))
}

/// Parses an external value as a `Boolean`.
///
/// Derived from graphql-js 16.12.0, src/type/scalars.ts, MIT.
fn boolean_parse_value(input_value: &JsValue) -> Result<Parsed, DefinitionError> {
    if !matches!(input_value, JsValue::Bool(_)) {
        return Err(DefinitionError::graphql(format!(
            "Boolean cannot represent a non boolean value: {}",
            inspect(input_value)
        )));
    }
    Ok(Parsed::Input)
}

/// Parses a literal as a `Boolean`.
///
/// Derived from graphql-js 16.12.0, src/type/scalars.ts, MIT.
fn boolean_parse_literal(
    value_node: &ValueNode,
    _variables: Option<&JsObject>,
) -> Result<JsValue, DefinitionError> {
    let ValueNode::BooleanValue(node) = value_node else {
        return Err(error_at(
            format!(
                "Boolean cannot represent a non boolean value: {}",
                print(value_node)
            ),
            value_node,
        ));
    };
    Ok(JsValue::Bool(node.value))
}

/// Serializes a value as an `ID`.
///
/// A string is itself and an integer is its `String` form.
///
/// Derived from graphql-js 16.12.0, src/type/scalars.ts, MIT.
fn id_serialize(output_value: &JsValue) -> Result<JsValue, DefinitionError> {
    let coerced_value = serialize_object(output_value);

    match coerced_value {
        JsValue::String(_) => Ok(coerced_value.clone()),
        JsValue::Number(number) if is_integer(*number) => {
            Ok(JsValue::from(number_to_string(*number)))
        }
        _ => Err(DefinitionError::graphql(format!(
            "ID cannot represent value: {}",
            inspect(output_value)
        ))),
    }
}

/// Parses an external value as an `ID`.
///
/// Derived from graphql-js 16.12.0, src/type/scalars.ts, MIT.
fn id_parse_value(input_value: &JsValue) -> Result<Parsed, DefinitionError> {
    match input_value {
        JsValue::String(_) => Ok(Parsed::Input),
        JsValue::Number(number) if is_integer(*number) => {
            Ok(Parsed::Value(JsValue::from(number_to_string(*number))))
        }
        _ => Err(DefinitionError::graphql(format!(
            "ID cannot represent value: {}",
            inspect(input_value)
        ))),
    }
}

/// Parses a literal as an `ID`.
///
/// Derived from graphql-js 16.12.0, src/type/scalars.ts, MIT.
fn id_parse_literal(
    value_node: &ValueNode,
    _variables: Option<&JsObject>,
) -> Result<JsValue, DefinitionError> {
    let value = match value_node {
        ValueNode::StringValue(node) => &node.value,
        ValueNode::IntValue(node) => &node.value,
        _ => {
            return Err(error_at(
                format!(
                    "ID cannot represent a non-string and non-integer value: {}",
                    print(value_node)
                ),
                value_node,
            ));
        }
    };
    Ok(JsValue::from(value.as_str()))
}

/// A specified scalar as the image carries it.
///
/// Everything the reference writes into the config of a specified scalar is
/// known when the crate is compiled, so a row holds it as read only data. The
/// remaining fields of the type are what the constructor of the reference
/// fills in for a config that leaves them out.
struct ScalarRow {
    /// The name.
    name: &'static str,
    /// The description.
    description: &'static str,
    /// Serializes an internal value to include in a response.
    serialize: &'static ValueFn,
    /// Parses an externally provided value to use as an input, and tells
    /// whether the internal value is the input itself.
    parse_value: &'static ParseFn,
    /// Parses an externally provided literal value to use as an input.
    parse_literal: &'static LiteralFn,
}

/// The five specified scalars, in the order of `specifiedScalarTypes`, which
/// lists `String`, `Int`, `Float`, `Boolean` and `ID`.
static SPECIFIED: [ScalarRow; 5] = [
    ScalarRow {
        name: "String",
        description: "The `String` scalar type represents textual data, represented as UTF-8 character sequences. The String type is most often used by GraphQL to represent free-form human-readable text.",
        serialize: &string_serialize,
        parse_value: &string_parse_value,
        parse_literal: &string_parse_literal,
    },
    ScalarRow {
        name: "Int",
        description: "The `Int` scalar type represents non-fractional signed whole numeric values. Int can represent values between -(2^31) and 2^31 - 1.",
        serialize: &int_serialize,
        parse_value: &int_parse_value,
        parse_literal: &int_parse_literal,
    },
    ScalarRow {
        name: "Float",
        description: "The `Float` scalar type represents signed double-precision fractional values as specified by [IEEE 754](https://en.wikipedia.org/wiki/IEEE_floating_point).",
        serialize: &float_serialize,
        parse_value: &float_parse_value,
        parse_literal: &float_parse_literal,
    },
    ScalarRow {
        name: "Boolean",
        description: "The `Boolean` scalar type represents `true` or `false`.",
        serialize: &boolean_serialize,
        parse_value: &boolean_parse_value,
        parse_literal: &boolean_parse_literal,
    },
    ScalarRow {
        name: "ID",
        description: "The `ID` scalar type represents a unique identifier, often used to refetch an object or as key for a cache. The ID type appears in a JSON response as a String; however, it is not intended to be human-readable. When expected as an input type, any string (such as `\"4\"`) or integer (such as `4`) input value will be accepted as an ID.",
        serialize: &id_serialize,
        parse_value: &id_parse_value,
        parse_literal: &id_parse_literal,
    },
];

/// The position of `String` in [`SPECIFIED`].
const STRING: usize = 0;

/// The position of `Int` in [`SPECIFIED`].
const INT: usize = 1;

/// The position of `Float` in [`SPECIFIED`].
const FLOAT: usize = 2;

/// The position of `Boolean` in [`SPECIFIED`].
const BOOLEAN: usize = 3;

/// The position of `ID` in [`SPECIFIED`].
const ID: usize = 4;

/// Builds the type of one row.
///
/// The name, the description and the three functions stay in the row, and
/// the type copies the two strings, because its fields are owned strings.
fn specified_scalar(row: &ScalarRow) -> Arc<ScalarType> {
    Arc::new(ScalarType {
        name: String::from(row.name),
        description: Some(String::from(row.description)),
        specified_by_url: None,
        serialize: ScalarSerializer::from_static(row.serialize),
        parse_value: ScalarValueParser::from_static(row.parse_value),
        parse_literal: ScalarLiteralParser::from_static(row.parse_literal),
        extensions: JsObject::new(),
        ast_node: None,
        extension_ast_nodes: Vec::new(),
    })
}

/// The five types built from [`SPECIFIED`], in its order.
static BUILT: OnceBox<[Arc<ScalarType>; 5]> = OnceBox::new();

/// Returns the five types, building them on first use.
fn built() -> &'static [Arc<ScalarType>; 5] {
    BUILT.get_or_init(|| Box::new(SPECIFIED.each_ref().map(specified_scalar)))
}

/// Returns the `Int` scalar type.
///
/// Every call returns the same allocation.
///
/// Derived from graphql-js 16.12.0, src/type/scalars.ts, MIT.
#[must_use]
pub fn graphql_int() -> Arc<ScalarType> {
    Arc::clone(&built()[INT])
}

/// Returns the `Float` scalar type.
///
/// Every call returns the same allocation.
///
/// Derived from graphql-js 16.12.0, src/type/scalars.ts, MIT.
#[must_use]
pub fn graphql_float() -> Arc<ScalarType> {
    Arc::clone(&built()[FLOAT])
}

/// Returns the `String` scalar type.
///
/// Every call returns the same allocation.
///
/// Derived from graphql-js 16.12.0, src/type/scalars.ts, MIT.
#[must_use]
pub fn graphql_string() -> Arc<ScalarType> {
    Arc::clone(&built()[STRING])
}

/// Returns the `Boolean` scalar type.
///
/// Every call returns the same allocation.
///
/// Derived from graphql-js 16.12.0, src/type/scalars.ts, MIT.
#[must_use]
pub fn graphql_boolean() -> Arc<ScalarType> {
    Arc::clone(&built()[BOOLEAN])
}

/// Returns the `ID` scalar type.
///
/// Every call returns the same allocation.
///
/// Derived from graphql-js 16.12.0, src/type/scalars.ts, MIT.
#[must_use]
pub fn graphql_id() -> Arc<ScalarType> {
    Arc::clone(&built()[ID])
}

/// Returns the five specified scalars in the reference's order.
///
/// Lists `String`, `Int`, `Float`, `Boolean` and `ID` in that order.
///
/// Derived from graphql-js 16.12.0, src/type/scalars.ts, MIT.
#[must_use]
pub fn specified_scalar_types() -> &'static [Arc<ScalarType>] {
    built()
}

/// Returns whether the named type carries the name of a specified scalar.
///
/// Compares names and not identities. The names are read from the rows of
/// `SPECIFIED`, so the predicate builds no type.
///
/// Derived from graphql-js 16.12.0, src/type/scalars.ts, MIT.
#[must_use]
pub fn is_specified_scalar_type(ty: &NamedType) -> bool {
    is_specified_scalar_name(ty.name())
}

/// Returns whether `name` is the name of a specified scalar.
///
/// This is the comparison of [`is_specified_scalar_type`], for a caller that
/// borrows a scalar type and holds no [`NamedType`] of it, so the test
/// counts no reference.
#[must_use]
pub fn is_specified_scalar_name(name: &str) -> bool {
    SPECIFIED.iter().any(|scalar| name == scalar.name)
}
