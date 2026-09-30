// SPDX-License-Identifier: MIT
/*
 * aosabi-core glue into_value: a Rust value as a value of the executor.
 *
 * No analogue in the reference, where a resolver returns a JavaScript value.
 */

//! The conversion of a Rust value into a value of the executor.
//!
//! Every type that has a GraphQL description has this conversion, so
//! whatever a field or a method may return by its description can also be
//! handed to the executor. A number of any width becomes a double, because
//! the TypeScript side has the one type `number`. `None` is `null`, and `()`
//! is `undefined`, the value of a function that returns nothing. A shared
//! pointer converts through the shared reference it holds, so the value
//! behind it needs no `Clone`.

use core::convert::Infallible;
use std::{
    borrow::Cow,
    collections::VecDeque,
    rc::Rc,
    sync::Arc, //
};

use aosabi_execute::value::Value;
use aosabi_json::{
    JsObject,
    JsValue, //
};
use aosabi_scalars::JsDate;

use crate::glue::shared::{
    FieldValue,
    Shared, //
};

/// A Rust value that converts into a value of the executor.
#[diagnostic::on_unimplemented(
    message = "`{Self}` has no value of the executor",
    note = "derive `aosabi::Aosabi` on the type, or mark a newtype with `#[aosabi::scalar]`"
)]
pub trait IntoValue {
    /// Returns the value.
    fn into_value(self) -> Value;
}

impl IntoValue for Value {
    fn into_value(self) -> Value {
        self
    }
}

impl IntoValue for () {
    fn into_value(self) -> Value {
        Value::Undefined
    }
}

impl IntoValue for Infallible {
    fn into_value(self) -> Value {
        match self {}
    }
}

impl IntoValue for bool {
    fn into_value(self) -> Value {
        Value::Bool(self)
    }
}

impl IntoValue for String {
    fn into_value(self) -> Value {
        Value::from(self)
    }
}

impl IntoValue for &str {
    fn into_value(self) -> Value {
        Value::from(self)
    }
}

impl IntoValue for Box<str> {
    fn into_value(self) -> Value {
        Value::from(String::from(self))
    }
}

impl IntoValue for Cow<'_, str> {
    fn into_value(self) -> Value {
        Value::from(self.into_owned())
    }
}

impl IntoValue for char {
    fn into_value(self) -> Value {
        Value::from(String::from(self))
    }
}

/// Implements [`IntoValue`] for a number type every value of which is a double.
macro_rules! exact_number_into_value {
    ($($type:ty),+ $(,)?) => {
        $(
            impl IntoValue for $type {
                fn into_value(self) -> Value {
                    Value::Number(f64::from(self))
                }
            }
        )+
    };
}

/// Implements [`IntoValue`] for a number type wider than the 53 bits of a double.
macro_rules! wide_number_into_value {
    ($($type:ty),+ $(,)?) => {
        $(
            impl IntoValue for $type {
                // The nearest double is the value a JavaScript number holds
                // for an integer of this size, so the rounding is the twin.
                #[allow(clippy::cast_precision_loss)]
                fn into_value(self) -> Value {
                    Value::Number(self as f64)
                }
            }
        )+
    };
}

exact_number_into_value!(f64, f32, i8, i16, i32, u8, u16, u32);
wide_number_into_value!(i64, i128, isize, u64, u128, usize);

/// A value of the JSON model moves into the value of the executor, so a key,
/// a string and an item move with no copy.
impl IntoValue for JsValue {
    fn into_value(self) -> Value {
        Value::from_js_owned(self)
    }
}

impl IntoValue for JsObject {
    fn into_value(self) -> Value {
        Value::from_js_owned(JsValue::Object(self))
    }
}

impl IntoValue for JsDate {
    fn into_value(self) -> Value {
        Value::Date(self)
    }
}

impl<T: IntoValue> IntoValue for Option<T> {
    fn into_value(self) -> Value {
        self.map_or(Value::Null, IntoValue::into_value)
    }
}

/// Returns the list of the converted items.
fn list_into_value<T: IntoValue>(items: impl IntoIterator<Item = T>) -> Value {
    Value::from(
        items
            .into_iter()
            .map(IntoValue::into_value)
            .collect::<Vec<_>>(),
    )
}

impl<T: IntoValue> IntoValue for Vec<T> {
    fn into_value(self) -> Value {
        list_into_value(self)
    }
}

impl<T: IntoValue> IntoValue for Box<[T]> {
    fn into_value(self) -> Value {
        list_into_value(self)
    }
}

impl<T: IntoValue> IntoValue for VecDeque<T> {
    fn into_value(self) -> Value {
        list_into_value(self)
    }
}

impl<T: IntoValue, const N: usize> IntoValue for [T; N] {
    fn into_value(self) -> Value {
        list_into_value(self)
    }
}

impl<T: IntoValue> IntoValue for Box<T> {
    fn into_value(self) -> Value {
        (*self).into_value()
    }
}

/// A struct behind the pointer is the object of the pointer, so it needs no
/// `Clone`, and plain data converts as the field of its type does.
impl<T: FieldValue> IntoValue for Rc<T> {
    fn into_value(self) -> Value {
        (*self).field_value(&|| Shared::from(Rc::clone(&self)))
    }
}

/// A struct behind the pointer is the object of a part of the pointer, so it
/// needs no `Clone`.
impl<T: FieldValue> IntoValue for Arc<T> {
    fn into_value(self) -> Value {
        Rc::new(self).into_value()
    }
}
