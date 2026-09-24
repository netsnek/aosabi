// SPDX-License-Identifier: MIT
/*
 * aosabi-json: the conversion of a Rust value into a JSON value.
 *
 * No analogue on the reference side. The conversion serves the macro json! and
 * a field of a log record, where the reference writes a JavaScript value as it
 * stands.
 */

//! `IntoJson`.
//!
//! [`IntoJson`] is the conversion the macro `json!` applies to every value it
//! is given. A number of any width becomes a double, because the reference has
//! the one type `number`, `None` becomes `null`, `()` becomes `undefined`,
//! which a member of an object leaves out, and a list becomes an array.

use alloc::{
    borrow::Cow,
    boxed::Box,
    string::String,
    vec::Vec, //
};

use crate::value::{
    JsObject,
    JsString,
    JsValue,
    Opaque, //
};

/// A Rust value that converts into a JSON value.
#[diagnostic::on_unimplemented(
    message = "`{Self}` has no JSON value",
    note = "a JSON value is a string, a number, a boolean, `()`, an `Option` or a list of one, a `JsValue` or a `JsObject`"
)]
pub trait IntoJson {
    /// Returns the value.
    fn into_json(self) -> JsValue;
}

/// Returns the elements of an array literal as the vector of an array.
///
/// The macro `json!` calls it, so the crate that expands the macro needs no
/// allocator of its own in scope.
#[doc(hidden)]
#[must_use]
pub fn array<const N: usize>(elements: [JsValue; N]) -> Vec<JsValue> {
    Vec::from(elements)
}

impl IntoJson for JsValue {
    fn into_json(self) -> JsValue {
        self
    }
}

impl IntoJson for JsObject {
    fn into_json(self) -> JsValue {
        JsValue::Object(self)
    }
}

impl IntoJson for JsString {
    fn into_json(self) -> JsValue {
        JsValue::String(self)
    }
}

impl IntoJson for Opaque {
    fn into_json(self) -> JsValue {
        JsValue::Opaque(self)
    }
}

impl IntoJson for String {
    fn into_json(self) -> JsValue {
        JsValue::from(self)
    }
}

impl IntoJson for &str {
    fn into_json(self) -> JsValue {
        JsValue::from(self)
    }
}

impl IntoJson for &String {
    fn into_json(self) -> JsValue {
        JsValue::from(self.as_str())
    }
}

impl IntoJson for Box<str> {
    fn into_json(self) -> JsValue {
        JsValue::from(String::from(self))
    }
}

impl IntoJson for Cow<'_, str> {
    fn into_json(self) -> JsValue {
        JsValue::from(self.into_owned())
    }
}

impl IntoJson for char {
    fn into_json(self) -> JsValue {
        JsValue::from(JsString::from(self))
    }
}

impl IntoJson for bool {
    fn into_json(self) -> JsValue {
        JsValue::Bool(self)
    }
}

impl IntoJson for () {
    fn into_json(self) -> JsValue {
        JsValue::Undefined
    }
}

/// Implements [`IntoJson`] for a number type every value of which is a double.
macro_rules! exact_number_into_json {
    ($($type:ty),+ $(,)?) => {
        $(
            impl IntoJson for $type {
                fn into_json(self) -> JsValue {
                    JsValue::Number(f64::from(self))
                }
            }
        )+
    };
}

/// Implements [`IntoJson`] for a number type wider than the 53 bits of a double.
macro_rules! wide_number_into_json {
    ($($type:ty),+ $(,)?) => {
        $(
            impl IntoJson for $type {
                // The nearest double is the value a JavaScript number holds
                // for an integer of this size, so the rounding is the twin.
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "a JavaScript number holds the nearest double"
                )]
                fn into_json(self) -> JsValue {
                    JsValue::Number(self as f64)
                }
            }
        )+
    };
}

exact_number_into_json!(f64, f32, i8, i16, i32, u8, u16, u32);
wide_number_into_json!(i64, i128, isize, u64, u128, usize);

impl<T: IntoJson> IntoJson for Option<T> {
    fn into_json(self) -> JsValue {
        self.map_or(JsValue::Null, IntoJson::into_json)
    }
}

impl<T: IntoJson> IntoJson for Vec<T> {
    fn into_json(self) -> JsValue {
        JsValue::Array(self.into_iter().map(IntoJson::into_json).collect())
    }
}

impl<T: IntoJson, const N: usize> IntoJson for [T; N] {
    fn into_json(self) -> JsValue {
        JsValue::Array(self.into_iter().map(IntoJson::into_json).collect())
    }
}

impl<T: IntoJson + Clone> IntoJson for &[T] {
    fn into_json(self) -> JsValue {
        JsValue::Array(self.iter().cloned().map(IntoJson::into_json).collect())
    }
}

impl<T: IntoJson> IntoJson for Box<T> {
    fn into_json(self) -> JsValue {
        (*self).into_json()
    }
}
