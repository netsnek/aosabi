// SPDX-License-Identifier: MIT
/*
 * aosabi-core glue from_argument: a positional argument of a resolver as a
 * Rust value.
 *
 * No analogue in the reference, where a resolver takes whatever value the
 * argument coercion made.
 */

//! The conversion of a positional argument into a Rust value.
//!
//! The executor coerces the arguments of a field against the schema before a
//! resolver runs, so a value that reaches a method has the type its
//! parameter describes. The conversion still checks, because a Rust
//! parameter is narrower than a JavaScript one: a `u8` does not hold every
//! `Number`. A value that does not fit throws a `TypeError`, which has no
//! twin on the TypeScript side, where no such check runs.

use aosabi_execute::value::{
    ErrorObject,
    Value,
    inspect,
    to_js_string, //
};
use aosabi_json::{
    JsObject,
    JsValue, //
};
use aosabi_lang::js_string::to_string_lossy;
use aosabi_scalars::JsDate;

/// The first integer a double no longer holds exactly, two to the power of 53.
const TWO_POW_53: f64 = 9_007_199_254_740_992.0;

/// The refusal of a value that does not fit a parameter type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArgumentMismatch {
    /// What the parameter type holds, as the words of a message.
    pub expected: &'static str,
}

/// A parameter type of a resolver method.
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be a parameter of a resolver",
    note = "a parameter is a string, a boolean, a number, a scalar marker, a list or an `Option` of one, a unit variant enum that derives `aosabi::Aosabi`, or a struct with named fields that derives `aosabi::Aosabi` and whose public fields are all parameter types"
)]
pub trait FromArgument: Sized {
    /// Returns `value` as the parameter type.
    ///
    /// # Errors
    ///
    /// Returns [`ArgumentMismatch`] for a value the type does not hold.
    fn from_argument(value: &Value) -> Result<Self, ArgumentMismatch>;
}

/// Returns the positional argument `index` of a call as the parameter type `T`.
///
/// `name` is the name of the parameter, for the message. An argument the
/// call does not carry reads as `undefined`.
///
/// # Errors
///
/// Returns the `TypeError` a resolver throws for a value that does not fit,
/// as a thrown value.
pub fn argument<T: FromArgument>(args: &[Value], index: usize, name: &str) -> Result<T, Value> {
    let undefined = Value::Undefined;
    let value = args.get(index).unwrap_or(&undefined);
    T::from_argument(value).map_err(|mismatch| {
        let message = format!(
            "Argument \"{name}\" expects {}, found: {}",
            mismatch.expected,
            inspect(value)
        );
        Value::from(ErrorObject::type_error(message))
    })
}

impl FromArgument for Value {
    fn from_argument(value: &Value) -> Result<Self, ArgumentMismatch> {
        Ok(value.clone())
    }
}

impl FromArgument for bool {
    fn from_argument(value: &Value) -> Result<Self, ArgumentMismatch> {
        match value {
            Value::Bool(value) => Ok(*value),
            _ => Err(ArgumentMismatch {
                expected: "a boolean",
            }),
        }
    }
}

impl FromArgument for String {
    /// A number and a boolean are taken as their `ToString` text.
    ///
    /// The bound `ID` scalar passes a value that is no global id through
    /// untouched, so a TypeScript resolver holds a number where its type says
    /// `string`, and every use of it as a string reads that text.
    fn from_argument(value: &Value) -> Result<Self, ArgumentMismatch> {
        match value {
            Value::String(text) => Ok(to_string_lossy(text).into_owned()),
            Value::Number(_) | Value::Bool(_) => {
                Ok(to_string_lossy(&to_js_string(value)).into_owned())
            }
            _ => Err(ArgumentMismatch {
                expected: "a string",
            }),
        }
    }
}

impl FromArgument for f64 {
    fn from_argument(value: &Value) -> Result<Self, ArgumentMismatch> {
        match value {
            Value::Number(number) => Ok(*number),
            _ => Err(ArgumentMismatch {
                expected: "a number",
            }),
        }
    }
}

impl FromArgument for f32 {
    // A `f32` parameter asks for the nearest single, which is what the cast gives.
    #[expect(clippy::cast_possible_truncation)]
    fn from_argument(value: &Value) -> Result<Self, ArgumentMismatch> {
        f64::from_argument(value).map(|number| number as f32)
    }
}

/// Implements [`FromArgument`] for an integer type.
macro_rules! integer_from_argument {
    ($($type:ty),+ $(,)?) => {
        $(
            impl FromArgument for $type {
                // The cast saturates and drops a fraction, and the comparison
                // of the way back refuses every double the cast changed. The
                // lints differ between the expansions, so they are allowed
                // and not expected.
                #[allow(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    clippy::cast_precision_loss,
                    clippy::cast_lossless,
                    clippy::float_cmp
                )]
                fn from_argument(value: &Value) -> Result<Self, ArgumentMismatch> {
                    let mismatch = ArgumentMismatch {
                        expected: concat!("an integer of the type ", stringify!($type)),
                    };
                    let Value::Number(number) = value else {
                        return Err(mismatch);
                    };
                    let integer = *number as $type;
                    // The greatest value of a wide type is no double, so a
                    // double that saturates to it is out of range.
                    let saturated = integer == <$type>::MAX && *number >= TWO_POW_53;
                    if integer as f64 != *number || saturated {
                        return Err(mismatch);
                    }
                    Ok(integer)
                }
            }
        )+
    };
}

integer_from_argument!(
    i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize
);

impl FromArgument for JsValue {
    fn from_argument(value: &Value) -> Result<Self, ArgumentMismatch> {
        Ok(value.to_js())
    }
}

impl FromArgument for JsObject {
    fn from_argument(value: &Value) -> Result<Self, ArgumentMismatch> {
        match value.to_js() {
            JsValue::Object(object) => Ok(object),
            _ => Err(ArgumentMismatch {
                expected: "an object",
            }),
        }
    }
}

impl FromArgument for JsDate {
    /// A parsed `Date` argument travels through the type layer as its
    /// `toISOString` text, so the text is read back into the date here.
    fn from_argument(value: &Value) -> Result<Self, ArgumentMismatch> {
        match value {
            Value::Date(date) => Ok(*date),
            Value::String(text) => Ok(Self::parse(&to_string_lossy(text))),
            _ => Err(ArgumentMismatch { expected: "a date" }),
        }
    }
}

impl<T: FromArgument> FromArgument for Option<T> {
    fn from_argument(value: &Value) -> Result<Self, ArgumentMismatch> {
        if value.is_nullish() {
            return Ok(None);
        }
        T::from_argument(value).map(Some)
    }
}

impl<T: FromArgument> FromArgument for Vec<T> {
    fn from_argument(value: &Value) -> Result<Self, ArgumentMismatch> {
        match value {
            Value::List(items) => items.iter().map(T::from_argument).collect(),
            _ => Err(ArgumentMismatch { expected: "a list" }),
        }
    }
}

impl<T: FromArgument> FromArgument for Box<T> {
    fn from_argument(value: &Value) -> Result<Self, ArgumentMismatch> {
        T::from_argument(value).map(Box::new)
    }
}
