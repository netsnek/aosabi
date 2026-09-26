// SPDX-License-Identifier: MIT
/*
 * aosabi-scalars: the four graphql-scalars scalars Aosabi binds.
 *
 * This file holds the value model the hooks share and the module table.
 *
 * Derived from graphql-scalars 1.25.0, ., MIT.
 */

//! The scalar library of Aosabi.
//!
//! Aosabi binds four scalars from graphql-scalars: `DateTimeISO` under the name
//! `Date`, `JSON`, `JSONObject` and `Void`. This crate is a literal
//! translation of the files behind those four, with every message string
//! copied from the installed package, because the byte contract names their
//! messages. The three scalars with a resolver of their own, `ID`, `GID`
//! and `Number`, live in the handler and are not part of this crate.
//!
//! | Module | Translates | Holds |
//! | --- | --- | --- |
//! | [`error`] | `esm/error.js` | [`create_graphql_error()`], [`ScalarError`] and [`RangeError`] |
//! | [`js_string`] | ECMA-262 `ToString` | the string a template literal renders a value as |
//! | [`js_date`] | ECMA-262 Date objects and the V8 date reader | [`JsDate`], the time value and `toISOString` |
//! | [`validator`] | `esm/scalars/iso-date/validator.js` | the RFC 3339 checks |
//! | [`formatter`] | `esm/scalars/iso-date/formatter.js` | [`parse_date_time()`](formatter::parse_date_time) |
//! | [`date_time`] | `esm/scalars/iso-date/DateTime.js` | the `DateTime` configuration the ISO scalar spreads |
//! | [`date_time_iso`] | `esm/scalars/iso-date/DateTimeISO.js` | the `DateTimeISO` hooks |
//! | [`json_utils`] | `esm/scalars/json/utils.js` | `identity`, `ensureObject`, `parseObject` and `parseLiteral` |
//! | [`json`] | `esm/scalars/json/JSON.js` | the `JSON` hooks |
//! | [`json_object`] | `esm/scalars/json/JSONObject.js` | the `JSONObject` hooks |
//! | [`void`] | `esm/scalars/Void.js` | the `Void` hooks |
//!
//! # The value model
//!
//! A hook of graphql-js receives any JavaScript value. The four scalars tell
//! exactly one runtime object apart from JSON shaped data, the `Date`
//! instance, so a hook takes and returns a [`ScalarValue`], which is a
//! [`JsValue`] or a [`JsDate`]. Every `serialize` and `parseValue` hook has
//! the signature `fn(&ScalarValue) -> ScalarResult`, and every `parseLiteral`
//! hook has the signature `fn(&ValueNode, Option<&JsObject>) -> ScalarResult`,
//! which is the arity graphql-js calls `parseLiteral(valueNode, variables)`
//! with. A hook that ignores an argument ignores it the way the JavaScript
//! function does.
//!
//! A hook fails with a [`ScalarError`]. Every message the scalars throw is a
//! `GraphQLError` built by `createGraphQLError`, and one message is not: the
//! `RangeError` V8 throws from `Date.prototype.toISOString` on an invalid
//! date, which the execution layer wraps the same way it wraps any thrown
//! error.
//!
//! The crate builds with no standard library and needs an allocator, because
//! the kernel and the player link it. There is no test in this crate, which is
//! the rule of CONTRIBUTING.md section 3.

#![no_std]
// A hook returns the GraphQLError of aosabi-lang by value, the way the
// reference throws it and the way the parser of aosabi-lang returns it, so
// the size of the error variant is the size of that error.
#![allow(clippy::result_large_err)]

extern crate alloc;

pub mod date_time;
pub mod date_time_iso;
pub mod error;
pub mod formatter;
pub mod js_date;
pub mod js_string;
pub mod json;
pub mod json_object;
pub mod json_utils;
pub mod validator;
pub mod void;

pub use aosabi_json::{JsObject, JsValue};
pub use aosabi_lang::ast::ValueNode;

pub use crate::{
    error::{
        RangeError,
        ScalarError,
        create_graphql_error, //
    },
    js_date::JsDate, //
};

/// A runtime value as a scalar hook sees it.
///
/// graphql-js hands a hook whatever JavaScript value the resolver returned or
/// the request carried. The four scalars of this crate distinguish one object
/// that is not JSON shaped, the `Date` instance, so the model is a JSON value
/// beside a date. A `JsValue::Undefined` is the JavaScript `undefined`, which
/// `parseLiteral` of the `JSON` scalar returns for an enum literal and for a
/// variable that is not set.
#[derive(Debug, Clone, PartialEq)]
pub enum ScalarValue {
    /// Any JSON shaped value, including `undefined`.
    Value(JsValue),
    /// A `Date` instance, valid or invalid.
    Date(JsDate),
}

impl From<JsValue> for ScalarValue {
    fn from(value: JsValue) -> Self {
        Self::Value(value)
    }
}

impl From<JsDate> for ScalarValue {
    fn from(date: JsDate) -> Self {
        Self::Date(date)
    }
}

/// The result of a scalar hook.
pub type ScalarResult = Result<ScalarValue, ScalarError>;
