// SPDX-License-Identifier: MIT
/*
 * aosabi-core: the scalar markers, the Rust types a service writes where a
 * TypeScript service writes ID, Int, Float, Date, File, JSON or any.
 *
 * No file of the reference is translated here.
 */

//! The scalar markers.
//!
//! A TypeScript service names a built in scalar by a TypeScript type: `ID`,
//! `Int` and `Float` of the framework, `Date`, `JSON` and `File` of the
//! TypeScript library, and `any`. The scalar table maps each to the scalar of
//! its name. A marker is the Rust type that stands in the same place.
//!
//! | Marker | TypeScript | Scalar | Documentation of the type |
//! | --- | --- | --- | --- |
//! | [`ID`] | `ID` | `ID` | none |
//! | [`Int`] | `Int` | `Int` | none |
//! | [`Float`] | `Float` | `Float` | none |
//! | [`Number`] | `number` | `Number` | none |
//! | [`Date`] | `Date` | `Date` | the documentation comment of `interface Date` in `lib.es5.d.ts` |
//! | [`File`] | `File` | `File` | the documentation comment of `interface File` in `lib.dom.d.ts` |
//! | [`JsValue`] | `JSON` | `JSON` | the documentation comment of `declare var JSON` in `lib.es5.d.ts` |
//! | [`Any`] | `any` | `Any` | no symbol, so none |
//! | [`JsObject`] | `Record<string, any>` | `JSONObject` | none |
//!
//! `JsObject` is no scalar on either side. `Record<string, any>` is an object
//! type with no property and no call signature, which the type mapping calls
//! `JSONObject` before it looks at a name, and a schema type of that name is
//! never made, because `JSONObject` is a scalar of the schema. The JSON
//! object type of `aosabi-json` is described as the same object type, and the
//! glue reads and writes it as the object it is.
//!
//! The marker of `JSON` is the JSON value type of `aosabi-json`, which the
//! facade exports as `JSON` as well, so one scalar keeps one Rust type.
//!
//! `ID`, `Int` and `Float` are aliases of an intersection, which carries no
//! symbol, so the checker gives them no documentation. `Date`, `JSON` and
//! `File` are library symbols with a documentation comment, and the reference
//! prints that text as the description of a non null input field of such a
//! type, because the description of an input field is the documentation of
//! its type. A marker answers the same text as the documentation of its type.
//! The texts are literals of the provenance lock, never typed by hand, and a
//! bump of TypeScript changes them.
//!
//! The scalar line of a schema is printed without a description, whatever the
//! documentation of the type says.
//!
//! At run time a marker is the value the bound scalar of its name reads and
//! writes, so a resolver returns it and takes it as a parameter. `File`
//! holds its argument as the executor hands it over, and the module `file`
//! of this crate reads the upload behind it.

use alloc::string::String;

use aosabi_execute::value::Value;
use aosabi_json::{
    JsObject,
    JsValue, //
};
use aosabi_scalars::JsDate;

use super::{
    described::Described,
    info::{
        Docs,
        Site,
        SourceKey,
        TypeInfo,
        TypeKind, //
    },
    link::{
        Primitive,
        TypeExpr, //
    },
};
use crate::glue::{
    ArgumentMismatch,
    FromArgument,
    IntoValue, //
};

/// The name the facade exports the marker of the scalar `JSON` under.
pub type JSON = JsValue;

/// Returns the descriptor of the scalar `name` with the documentation comment
/// `docs`, which is a library text and carries no tag.
///
/// A marker belongs to no application, so it emits no declaration. Its
/// source key names this file relative to the crate directory, which reads
/// the same on every machine, where `file!()` gives the absolute path of a
/// path dependency.
const fn scalar(name: &'static str, docs: &'static str, line: u32) -> TypeInfo {
    TypeInfo {
        kind: TypeKind::Scalar,
        name,
        krate: env!("CARGO_PKG_NAME"),
        source: SourceKey {
            file: "src/descriptor/markers.rs",
            line,
            column: 1,
        },
        docs: Docs {
            comment: docs,
            tags: &[],
        },
        site: Site::bare("src/descriptor/markers.rs", line),
        typename: None,
        anonymous: false,
        fields: &[],
        methods: &[],
        variants: &[],
        members: &[],
        type_arguments: &[],
        heritage: &[],
    }
}

/// The scalar `ID`, the twin of the TypeScript `ID` type.
///
/// The value is the text of the id. The bound scalar decodes a global id on
/// input and passes any other value through, so a number arrives as its
/// `ToString` text, which is what a TypeScript resolver reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ID(pub String);

impl From<String> for ID {
    fn from(text: String) -> Self {
        Self(text)
    }
}

impl From<&str> for ID {
    fn from(text: &str) -> Self {
        Self(String::from(text))
    }
}

impl Described for ID {
    const TYPE: TypeExpr = TypeExpr::Named(&scalar("ID", "", line!()));
}

impl IntoValue for ID {
    fn into_value(self) -> Value {
        Value::from(self.0)
    }
}

impl FromArgument for ID {
    fn from_argument(value: &Value) -> Result<Self, ArgumentMismatch> {
        String::from_argument(value).map(Self)
    }
}

/// The scalar `Int`, the twin of the TypeScript `Int` type.
///
/// The value is a double, because a TypeScript `Int` is a `number`, and
/// `Int::from(3.0)` is the twin of `3 as Int`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Int(pub f64);

impl From<f64> for Int {
    fn from(number: f64) -> Self {
        Self(number)
    }
}

impl Described for Int {
    const TYPE: TypeExpr = TypeExpr::Named(&scalar("Int", "", line!()));
}

impl IntoValue for Int {
    fn into_value(self) -> Value {
        Value::Number(self.0)
    }
}

impl FromArgument for Int {
    fn from_argument(value: &Value) -> Result<Self, ArgumentMismatch> {
        f64::from_argument(value).map(Self)
    }
}

/// The scalar `Float`, the twin of the TypeScript `Float` type.
///
/// `Float::from(1.5)` is the twin of `1.5 as Float`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Float(pub f64);

impl From<f64> for Float {
    fn from(number: f64) -> Self {
        Self(number)
    }
}

impl Described for Float {
    const TYPE: TypeExpr = TypeExpr::Named(&scalar("Float", "", line!()));
}

impl IntoValue for Float {
    fn into_value(self) -> Value {
        Value::Number(self.0)
    }
}

impl FromArgument for Float {
    fn from_argument(value: &Value) -> Result<Self, ArgumentMismatch> {
        f64::from_argument(value).map(Self)
    }
}

/// The TypeScript `number`, which maps to the scalar `Number`.
///
/// A port writes `Number` where its fixture writes `number`, so the field
/// prints `Number` whatever the width a Rust number would print by on the
/// source path, where `i32` prints `Int` and `f64` prints `Float`. The value
/// is a double, as a TypeScript `number` is, and `Number::from(1.5)` is the
/// twin of the literal `1.5`. The marker describes the same node as `f64` on
/// the derive path, so a twin that writes it prints the bytes it prints with
/// `f64` today.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct Number(pub f64);

impl From<f64> for Number {
    fn from(number: f64) -> Self {
        Self(number)
    }
}

impl From<i32> for Number {
    fn from(number: i32) -> Self {
        Self(f64::from(number))
    }
}

impl From<u32> for Number {
    fn from(number: u32) -> Self {
        Self(f64::from(number))
    }
}

impl From<Number> for f64 {
    fn from(number: Number) -> Self {
        number.0
    }
}

impl From<Number> for JsValue {
    fn from(number: Number) -> Self {
        Self::Number(number.0)
    }
}

impl Described for Number {
    const TYPE: TypeExpr = TypeExpr::Primitive(Primitive::Number);
}

impl IntoValue for Number {
    fn into_value(self) -> Value {
        Value::Number(self.0)
    }
}

impl FromArgument for Number {
    fn from_argument(value: &Value) -> Result<Self, ArgumentMismatch> {
        f64::from_argument(value).map(Self)
    }
}

/// The scalar `Date`, the twin of a TypeScript `Date`.
///
/// The value is a `Date` instance, whose time value the bound scalar writes
/// through `toISOString`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Date(JsDate);

impl Date {
    /// Returns the date `new Date()` makes, which is the current time.
    ///
    /// The time value is the whole milliseconds since the epoch the system
    /// clock reads.
    #[must_use]
    pub fn now() -> Self {
        Self(JsDate::from_time_value(epoch_milliseconds()))
    }

    /// Returns the date `new Date(value)` makes from a number.
    #[must_use]
    pub fn from_time_value(value: f64) -> Self {
        Self(JsDate::from_time_value(value))
    }

    /// Returns the date `new Date(text)` makes from a string.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        Self(JsDate::parse(text))
    }

    /// Returns `getTime()`, the time value, which is NaN for an invalid date.
    #[must_use]
    pub const fn get_time(self) -> f64 {
        self.0.get_time()
    }
}

/// Returns the milliseconds the system clock is past the epoch, negative
/// before it.
///
/// A Worker reads no clock of the system from `std`, whose `SystemTime::now`
/// panics on `wasm32-unknown-unknown`, so a Worker build reads 0 until the
/// glue of the Worker hands the host's time, as the file of
/// `graphql-over-http` does.
// A time value is a double and the clock holds far fewer than 2 to the 53
// milliseconds, so the conversion keeps every digit.
#[allow(clippy::cast_precision_loss)]
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
fn epoch_milliseconds() -> f64 {
    let now = std::time::SystemTime::now();
    match now.duration_since(std::time::UNIX_EPOCH) {
        Ok(after) => after.as_millis() as f64,
        Err(before) => -(before.duration().as_millis() as f64),
    }
}

/// Returns 0, the time of a Worker build, see the other variant.
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
fn epoch_milliseconds() -> f64 {
    0.0
}

impl Described for Date {
    const TYPE: TypeExpr =
        TypeExpr::Named(&scalar("Date", include_str!("library/date.txt"), line!()));
}

impl IntoValue for Date {
    fn into_value(self) -> Value {
        self.0.into_value()
    }
}

impl FromArgument for Date {
    fn from_argument(value: &Value) -> Result<Self, ArgumentMismatch> {
        JsDate::from_argument(value).map(Self)
    }
}

// The scalar `JSON`, the twin of a TypeScript `JSON`. The value is any JSON
// value, which the bound scalar writes as it is, and the two conversions of
// the run time are those of `JsValue` in the glue.
impl Described for JsValue {
    const TYPE: TypeExpr =
        TypeExpr::Named(&scalar("JSON", include_str!("library/json.txt"), line!()));
}

/// The scalar `File`, the twin of a TypeScript `File`.
///
/// The value is the argument as the executor hands it over. A file of a
/// multipart request arrives as a host object the resolver reads through the
/// methods of the module `file` of this crate, which read the value through
/// `File::held`. The one way to make a `File` is to take it as a
/// parameter.
#[derive(Clone)]
pub struct File(Value);

impl File {
    /// Returns the value the marker holds, the host object of an upload.
    #[must_use]
    pub(crate) const fn held(&self) -> &Value {
        &self.0
    }
}

impl Described for File {
    const TYPE: TypeExpr =
        TypeExpr::Named(&scalar("File", include_str!("library/file.txt"), line!()));
}

impl IntoValue for File {
    fn into_value(self) -> Value {
        self.0
    }
}

impl FromArgument for File {
    fn from_argument(value: &Value) -> Result<Self, ArgumentMismatch> {
        Ok(Self(value.clone()))
    }
}

// The twin of `Record<string, any>`. The mapped type the alias stands for
// carries the symbol `__type` of a type literal and no declaration name,
// which is the presentation of an anonymous object, and it has no property.
impl Described for JsObject {
    const TYPE: TypeExpr = TypeExpr::Named(&TypeInfo {
        kind: TypeKind::Object,
        name: "Record",
        krate: env!("CARGO_PKG_NAME"),
        source: SourceKey {
            file: "src/descriptor/markers.rs",
            line: line!(),
            column: 1,
        },
        docs: Docs::NONE,
        site: Site::bare("src/descriptor/markers.rs", line!()),
        typename: None,
        anonymous: true,
        fields: &[],
        methods: &[],
        variants: &[],
        members: &[],
        type_arguments: &[],
        heritage: &[],
    });
}

/// The scalar `Any`, the twin of the TypeScript `any`.
///
/// The type is the checker's `any` and not a named type, because `any` has no
/// symbol, and the type mapping gives it the scalar `Any`. The value is any
/// value of the executor, as the value of a TypeScript expression cast to
/// `any` is.
#[derive(Clone)]
pub struct Any(Value);

impl Any {
    /// Returns `value` as `any`, the twin of `value as any`.
    pub fn value(value: impl IntoValue) -> Self {
        Self(value.into_value())
    }

    /// Returns the value the marker holds.
    #[must_use]
    pub(crate) const fn held(&self) -> &Value {
        &self.0
    }
}

impl Described for Any {
    const TYPE: TypeExpr = TypeExpr::Any;
}

impl IntoValue for Any {
    fn into_value(self) -> Value {
        self.0
    }
}

impl FromArgument for Any {
    fn from_argument(value: &Value) -> Result<Self, ArgumentMismatch> {
        Ok(Self(value.clone()))
    }
}
