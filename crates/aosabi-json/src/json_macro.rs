// SPDX-License-Identifier: MIT
/*
 * aosabi-json: the macro json!, exported at the root of the crate.
 *
 * No analogue on the reference side.
 */

//! The macro `json!`.
//!
//! `json!({ ... })` is an object literal and answers a [`JsObject`], which
//! `ServiceErrorExtensions::details` takes as it stands and a log record
//! takes as its fields. `json!([ ... ])` is an array literal and answers a
//! [`JsValue::Array`], and any other input is one value and answers a
//! [`JsValue`].
//!
//! A member of an object literal is `key: value`, where the key is an
//! identifier or a string literal, or a shorthand `key`, which is the key
//! `key` with the value of the binding `key`, the twin of `{ id }` of
//! JavaScript. A value is a nested object literal, a nested array literal or
//! an expression whose type has [`IntoJson`]. A shorthand clones its binding,
//! because a JavaScript shorthand copies a reference and leaves the binding
//! usable, and an expression is moved as Rust moves it. The keys are inserted
//! in source order, and the writer applies the V8 order of integer keys, so
//! `json!({ "2": a, b, "1": c })` writes `{"1":..,"2":..,"b":..}` as V8 does.
//!
//! [`JsObject`]: crate::JsObject
//! [`JsValue::Array`]: crate::JsValue::Array
//! [`JsValue`]: crate::JsValue
//! [`IntoJson`]: crate::into_json::IntoJson

/// Builds a JSON value from a JavaScript literal.
///
/// # Examples
///
/// ```ignore
/// use aosabi_json::json;
///
/// let id = String::from("o-1");
/// let details = json!({ id, field: ["sku"], nested: { "2": 2, "1": true } });
/// let list = json!([1, "two", { three: 3 }]);
/// let one = json!(4.5);
/// ```
#[macro_export]
macro_rules! json {
    ({ $($body:tt)* }) => {{
        #[allow(unused_mut, reason = "an empty literal inserts nothing")]
        let mut object = $crate::JsObject::new();
        $crate::__json_members!(object; $($body)*);
        object
    }};
    ([ $($body:tt)* ]) => {
        $crate::JsValue::Array($crate::__json_elements!([]; $($body)*))
    };
    ($value:expr) => {
        $crate::into_json::IntoJson::into_json($value)
    };
}

/// Inserts the members of an object literal into `$object`, one at a time.
#[doc(hidden)]
#[macro_export]
macro_rules! __json_members {
    ($object:ident;) => {};
    ($object:ident; $key:tt : { $($inner:tt)* } $(, $($rest:tt)*)?) => {
        $object.insert(
            $crate::__json_key!($key),
            $crate::JsValue::Object($crate::json!({ $($inner)* })),
        );
        $crate::__json_members!($object; $($($rest)*)?);
    };
    ($object:ident; $key:tt : [ $($inner:tt)* ] $(, $($rest:tt)*)?) => {
        $object.insert($crate::__json_key!($key), $crate::json!([ $($inner)* ]));
        $crate::__json_members!($object; $($($rest)*)?);
    };
    ($object:ident; $key:tt : $value:expr $(, $($rest:tt)*)?) => {
        $object.insert(
            $crate::__json_key!($key),
            $crate::into_json::IntoJson::into_json($value),
        );
        $crate::__json_members!($object; $($($rest)*)?);
    };
    ($object:ident; $key:ident $(, $($rest:tt)*)?) => {
        $object.insert(
            ::core::stringify!($key),
            $crate::into_json::IntoJson::into_json(::core::clone::Clone::clone(&$key)),
        );
        $crate::__json_members!($object; $($($rest)*)?);
    };
}

/// Answers the key of a member, the text of an identifier or a string literal.
#[doc(hidden)]
#[macro_export]
macro_rules! __json_key {
    ($key:ident) => {
        ::core::stringify!($key)
    };
    ($key:literal) => {
        $key
    };
}

/// Collects the elements of an array literal, one at a time.
#[doc(hidden)]
#[macro_export]
macro_rules! __json_elements {
    ([$($done:expr),*];) => {
        $crate::into_json::array([$($done),*])
    };
    ([$($done:expr),*]; { $($inner:tt)* } $(, $($rest:tt)*)?) => {
        $crate::__json_elements!(
            [$($done,)* $crate::JsValue::Object($crate::json!({ $($inner)* }))];
            $($($rest)*)?
        )
    };
    ([$($done:expr),*]; [ $($inner:tt)* ] $(, $($rest:tt)*)?) => {
        $crate::__json_elements!([$($done,)* $crate::json!([ $($inner)* ])]; $($($rest)*)?)
    };
    ([$($done:expr),*]; $value:expr $(, $($rest:tt)*)?) => {
        $crate::__json_elements!(
            [$($done,)* $crate::into_json::IntoJson::into_json($value)];
            $($($rest)*)?
        )
    };
}
