// SPDX-License-Identifier: MIT
/*
 * aosabi-core: the value a delegation hands back, read from the remote
 * object with the object kept.
 *
 * No reference file.
 */

//! The delegated value contract.
//!
//! The federation of the reference hands a resolver the object the remote
//! answered, after its patch ran, and TypeScript assigns it to the declared
//! return of the resolver by structure, with no conversion at run time. The
//! executor then reads every member of that object, the aliased ones of the
//! request among them, and a patched object keeps the members its patch did
//! not name, as `{...processedData, ...patchedData}` keeps them.
//!
//! [`FromDelegated`] reads a Rust value from that object and keeps the object
//! where the value is a record. A pulled type keeps it in its
//! [`TypeName`](super::TypeName) slot, a struct with a spread field keeps it
//! through the spread, and [`Unchecked`] keeps the value whole, so a plain
//! struct, which would keep nothing, implements the trait nowhere.
//!
//! A record answers the kept object for every member, so a member no field
//! names, an aliased one among them, reads as the remote carries it. A field
//! a patch or a resolver assigns is answered from the field instead, since
//! the reference's patch changes the copy of the object it is handed and the
//! merge answers the change. [`SameMember`] tells the two apart: a field
//! whose value is still what [`read_member`] made of its member is unchanged,
//! and [`unchanged`] reads a field that fell back to the value of an absent
//! member as unchanged too.
//!
//! | Rust | Reads |
//! | --- | --- |
//! | `String`, `bool`, `f64`, `ID`, `Int`, `Float`, `Date`, `File`, `JsValue`, `Any`, `JsObject` | the leaf through its argument reader |
//! | `()` | the unit, whatever the value is |
//! | `Option<X>` | `None` for `null` and `undefined`, `X` otherwise |
//! | `Vec<X>` | each element as `X` |
//! | `Box<X>` | `X` |
//! | [`Callable`] | the signature, which holds no value |
//! | [`Unchecked<T>`](Unchecked) | the value as it is, under any remote type |
//! | `Lazy<T>` | the function the patch made, called when the member is selected |
//! | a pulled type, a spread struct, a unit enum | what `#[derive(Aosabi)]` writes for each |
//!
//! A member the object does not carry reads as `undefined`, which a leaf with
//! a `Default` reads as that default, `Option` as `None` and `Vec` as the
//! empty list, where TypeScript reads `undefined`. [`read_member`] reads a
//! member of a pulled record leniently: a member that holds an error of the
//! remote, a value the field type cannot read and an enum value or a union
//! member the pulled registry does not know read as an absent member does, so
//! the record stands and the executor answers the member from the kept
//! object, as the reference hands the object on unread. A type with no value
//! for an absent member, an enum, a record, a union and `Date`, keeps the
//! refusal, which is why `aosabi pull` writes a non null member of such a
//! type as an `Option`, and only a value that is no object at all refuses a
//! record.

use alloc::{
    boxed::Box,
    format,
    rc::Rc,
    string::String,
    vec::Vec, //
};
use core::marker::PhantomData;

use aosabi_execute::value::{
    Value,
    to_js_string, //
};
use aosabi_json::{
    JsObject,
    JsValue, //
};
use aosabi_lang::js_string::to_string_lossy;

use super::Callable;
use crate::{
    descriptor::{
        Any,
        Date,
        Described,
        File,
        Float,
        ID,
        Int,
        TypeExpr, //
    },
    glue::{
        FromArgument,
        Thrown,
        Unchecked, //
    }, //
};

/// A value a delegation hands back, read from the remote object with the
/// object kept.
///
/// `Remote` is the type the remote field returns in the pulled registry, so
/// the bound `T: FromDelegated<D::Return>` of a delegation states which
/// Rust types may stand for the answer of the field.
#[diagnostic::on_unimplemented(
    message = "`{Self}` does not keep the remote object it would be read from",
    note = "a delegated value keeps the remote object, so a resolver returns a pulled type, a struct with an `#[aosabi(spread)]` field, or `Unchecked<{Self}>`, the twin of the structural assignment TypeScript applies to a delegate result"
)]
pub trait FromDelegated<Remote>: Sized {
    /// Reads the value from what the delegation answered.
    ///
    /// # Errors
    ///
    /// Returns the thrown `TypeError` of a member that the value does not
    /// carry where the type requires it, and of a value of another kind than
    /// the type reads.
    fn from_delegated(value: Value) -> Result<Self, Thrown>;
}

/// Returns the leaf `T` its argument reader reads from `value`, or the
/// thrown `TypeError` that names what the reader expected.
fn leaf<T: FromArgument>(value: &Value) -> Result<T, Thrown> {
    T::from_argument(value).map_err(|mismatch| {
        Thrown::type_error(&format!("the delegated value is not {}", mismatch.expected))
    })
}

/// Implements [`FromDelegated`] of a leaf for itself, with the value an
/// absent member reads as.
macro_rules! leaf_with_default {
    ($($type:ty => $absent:expr),+ $(,)?) => {
        $(
            impl FromDelegated<$type> for $type {
                fn from_delegated(value: Value) -> Result<Self, Thrown> {
                    if matches!(value, Value::Undefined) {
                        return Ok($absent);
                    }
                    leaf(&value)
                }
            }
        )+
    };
}

leaf_with_default!(
    String => String::new(),
    bool => false,
    f64 => 0.0,
    ID => ID(String::new()),
    Int => Int(0.0),
    Float => Float(0.0),
    JsObject => JsObject::new(),
);

/// Implements [`FromDelegated`] of a leaf for itself, whose argument reader
/// decides what an absent member reads as.
macro_rules! leaf_as_read {
    ($($type:ty),+ $(,)?) => {
        $(
            impl FromDelegated<$type> for $type {
                fn from_delegated(value: Value) -> Result<Self, Thrown> {
                    leaf(&value)
                }
            }
        )+
    };
}

// A `Date` has no default, so an absent member throws, `File` holds any
// value and `JsValue` and `Any` read `undefined` as the value they hold.
leaf_as_read!(Date, File, JsValue, Any);

impl FromDelegated<Self> for () {
    /// Returns the unit, the twin of `Void`, whatever the value is.
    fn from_delegated(_: Value) -> Result<Self, Thrown> {
        Ok(())
    }
}

impl<X: FromDelegated<Y>, Y> FromDelegated<Option<Y>> for Option<X> {
    /// Returns `None` for `null` and `undefined`, the absent member among
    /// them.
    fn from_delegated(value: Value) -> Result<Self, Thrown> {
        if value.is_nullish() {
            return Ok(None);
        }
        X::from_delegated(value).map(Some)
    }
}

impl<X: FromDelegated<Y>, Y> FromDelegated<Vec<Y>> for Vec<X> {
    /// Returns each element in order, and the empty list for an absent
    /// member.
    fn from_delegated(value: Value) -> Result<Self, Thrown> {
        match value {
            Value::Undefined => Ok(Vec::new()),
            Value::List(items) => items.iter().cloned().map(X::from_delegated).collect(),
            _ => Err(Thrown::type_error("the delegated value is not a list")),
        }
    }
}

impl<X: FromDelegated<Y>, Y> FromDelegated<Box<Y>> for Box<X> {
    fn from_delegated(value: Value) -> Result<Self, Thrown> {
        X::from_delegated(value).map(Box::new)
    }
}

impl<A, R> FromDelegated<Self> for Callable<A, R> {
    /// Returns the signature, since a field with arguments holds no value
    /// in the record and the executor reads it from the kept object.
    fn from_delegated(_: Value) -> Result<Self, Thrown> {
        Ok(Self::new())
    }
}

impl<T: Described, R> FromDelegated<R> for Unchecked<T> {
    /// Returns the value as it is under the declared type `T`, the twin of
    /// the structural assignment TypeScript applies to a delegate result.
    fn from_delegated(value: Value) -> Result<Self, Thrown> {
        Ok(Self::value(value))
    }
}

/// Reads the member `key` of the remote object `object` as `T`, leniently.
///
/// An absent member reads as `undefined`, which `T` reads as its default
/// where it has one. A member `T` cannot read, a member that holds an error
/// of the remote or a value of another kind among them, reads as an absent
/// member does, so the record stands and the executor answers the member
/// from the kept object. Where `T` has no value for an absent member, the
/// read throws the `TypeError` `the delegated value carries no member
/// "<key>"` for an absent member and the thrown value of the read for a
/// member it cannot read.
///
/// # Errors
///
/// Returns the thrown value of the read of the member, for a type with no
/// value for an absent member.
pub fn read_member<T: FromDelegated<T>>(object: &Value, key: &str) -> Result<T, Thrown> {
    let member = member_of(object, key);
    if matches!(member, Value::Undefined) {
        return T::from_delegated(member).map_err(|_| no_member(key));
    }
    T::from_delegated(member)
        .or_else(|thrown| T::from_delegated(Value::Undefined).map_err(|_| thrown))
}

/// Reads the member `key` of the remote object `object` as `T`, and throws
/// for an absent member whatever `T` is.
///
/// This is the strict read of an own field of a struct with a spread, whose
/// patch always carries its own members, so an absent one is a patch
/// registered for the wrong type.
///
/// # Errors
///
/// Returns the thrown `TypeError` `the delegated value carries no member
/// "<key>"` for an absent member, and the thrown value of the read of the
/// member otherwise.
pub fn read_own_member<T: FromDelegated<T>>(object: &Value, key: &str) -> Result<T, Thrown> {
    let member = member_of(object, key);
    if matches!(member, Value::Undefined) {
        return Err(no_member(key));
    }
    T::from_delegated(member)
}

/// Returns the thrown `TypeError` of the field `member` of the struct
/// `owner`, whose type a delegated value cannot be read into.
///
/// A struct with a spread reads each field through the type of the field,
/// and a type that keeps no remote object, a plain struct among them, or a
/// private field whose type has no `Default`, has no reader, which the read
/// throws when a delegated value reaches it.
#[must_use]
pub fn unreadable_member(member: &str, owner: &str) -> Thrown {
    Thrown::type_error(&format!(
        "the field \"{member}\" of \"{owner}\" cannot be read from a delegated value, since its type keeps no remote object"
    ))
}

/// Returns `Ok` when `value` is an object, and otherwise the thrown
/// `TypeError` that names the record `name` the value was to be read as.
///
/// # Errors
///
/// Returns the thrown `TypeError` `the delegated value is not an object of
/// the type "<name>"` for a value that is no object.
pub fn require_object(value: &Value, name: &str) -> Result<(), Thrown> {
    match value {
        Value::Object(_) => Ok(()),
        _ => Err(Thrown::type_error(&format!(
            "the delegated value is not an object of the type \"{name}\""
        ))),
    }
}

/// Returns `object[key]`, `undefined` for a value that is no object.
fn member_of(object: &Value, key: &str) -> Value {
    match object {
        Value::Object(object) => alloc::rc::Rc::clone(object).get(key),
        _ => Value::Undefined,
    }
}

/// Returns the thrown `TypeError` of an absent member `key`.
fn no_member(key: &str) -> Thrown {
    Thrown::type_error(&format!("the delegated value carries no member \"{key}\""))
}

/// Returns the `__typename` of the delegated value `value` as text, none for
/// a value that is no object or whose typename is no string.
#[must_use]
pub fn typename_of(value: &Value) -> Option<String> {
    match member_of(value, "__typename") {
        Value::String(text) => Some(to_string_lossy(&text).into_owned()),
        _ => None,
    }
}

/// Returns the `__typename` literal the described record `T` carries, through
/// a box, none for a type that carries none.
///
/// A payload enum of a pulled registry reads the variant of a delegated value
/// by this literal, which the key `remote` sets to the name of the type.
#[must_use]
pub fn described_typename<T: Described>() -> Option<&'static str> {
    let mut expr = T::TYPE;
    loop {
        match expr {
            TypeExpr::Boxed(link) => expr = link.resolve(),
            TypeExpr::Named(info) => return info.typename,
            _ => return None,
        }
    }
}

/// Returns the thrown `TypeError` of a delegated value whose typename
/// `typename` names no member of the union `name`.
#[must_use]
pub fn unknown_typename(typename: Option<&str>, name: &str) -> Thrown {
    let typename = typename.unwrap_or("undefined");
    Thrown::type_error(&format!(
        "the delegated value carries the typename \"{typename}\", which names no member of \"{name}\""
    ))
}

/// A value read from a member of a delegated object, which tells whether it is still what the read made of that member.
///
/// A record of a pulled registry answers the kept object for a field whose
/// value is unchanged and the field for one a patch or a resolver assigned,
/// so the derive asks each field this question.
pub trait SameMember {
    /// Returns whether the value is what [`FromDelegated`] reads from `member`, and `false` where it reads nothing.
    ///
    /// The member `undefined` reads as the value of an absent member, so a
    /// value that is the default of its type is the same as `undefined`.
    fn same_member(&self, member: &Value) -> bool;
}

/// Returns whether the field `field` still holds what [`read_member`] made of `member`.
///
/// The read is lenient, so a member the type cannot read gave the value of
/// an absent member, and a field that still holds that value is unchanged
/// as well.
pub fn unchanged<T: SameMember + FromDelegated<T>>(field: &T, member: &Value) -> bool {
    field.same_member(member)
        || (field.same_member(&Value::Undefined)
            && <T as FromDelegated<T>>::from_delegated(member.clone()).is_err())
}

/// The question [`unchanged`] of a field whose type implements [`SameMember`], which the derive asks through a reference.
#[doc(hidden)]
pub trait SameMemberRead<T> {
    /// Returns whether `field` still holds what the read made of `member`.
    fn aosabi_same_member(&self, field: &T, member: &Value) -> bool;
}

impl<T: SameMember + FromDelegated<T>> SameMemberRead<T> for &PhantomData<T> {
    fn aosabi_same_member(&self, field: &T, member: &Value) -> bool {
        unchanged(field, member)
    }
}

/// The fallback of [`SameMemberRead`] for a field of a type with no [`SameMember`], which reads as unchanged.
#[doc(hidden)]
pub trait SameMemberFallback<T> {
    /// Returns `true`, so the record answers the kept member.
    fn aosabi_same_member(&self, field: &T, member: &Value) -> bool;
}

impl<T> SameMemberFallback<T> for PhantomData<T> {
    fn aosabi_same_member(&self, _: &T, _: &Value) -> bool {
        true
    }
}

/// Returns whether two numbers are the same value, ECMA-262 `SameValue`.
fn same_number(left: f64, right: f64) -> bool {
    left.to_bits() == right.to_bits() || (left.is_nan() && right.is_nan())
}

/// Returns whether two values are one value, the same primitive or the same object.
fn same_value(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Undefined, Value::Undefined) | (Value::Null, Value::Null) => true,
        (Value::Bool(left), Value::Bool(right)) => left == right,
        (Value::Number(left), Value::Number(right)) => same_number(*left, *right),
        (Value::String(left), Value::String(right)) => left == right,
        (Value::Date(left), Value::Date(right)) => same_number(left.get_time(), right.get_time()),
        (Value::List(left), Value::List(right)) => Rc::ptr_eq(left, right),
        (Value::Object(left), Value::Object(right)) => Rc::ptr_eq(left, right),
        _ => false,
    }
}

/// Returns whether `text` is the text the reader of a string makes of `member`.
fn same_text(text: &str, member: &Value) -> bool {
    match member {
        Value::String(member) => {
            member.as_wtf8() == text.as_bytes() || to_string_lossy(member) == text
        }
        Value::Number(_) | Value::Bool(_) => to_string_lossy(&to_js_string(member)) == text,
        Value::Undefined => text.is_empty(),
        _ => false,
    }
}

impl SameMember for String {
    fn same_member(&self, member: &Value) -> bool {
        same_text(self, member)
    }
}

impl SameMember for ID {
    fn same_member(&self, member: &Value) -> bool {
        same_text(&self.0, member)
    }
}

impl SameMember for bool {
    fn same_member(&self, member: &Value) -> bool {
        match member {
            Value::Bool(value) => value == self,
            Value::Undefined => !*self,
            _ => false,
        }
    }
}

/// Returns whether `number` is the number the reader of a number makes of `member`.
fn same_read_number(number: f64, member: &Value) -> bool {
    match member {
        Value::Number(value) => same_number(*value, number),
        Value::Undefined => same_number(0.0, number),
        _ => false,
    }
}

impl SameMember for f64 {
    fn same_member(&self, member: &Value) -> bool {
        same_read_number(*self, member)
    }
}

impl SameMember for Int {
    fn same_member(&self, member: &Value) -> bool {
        same_read_number(self.0, member)
    }
}

impl SameMember for Float {
    fn same_member(&self, member: &Value) -> bool {
        same_read_number(self.0, member)
    }
}

impl SameMember for Date {
    fn same_member(&self, member: &Value) -> bool {
        <Self as FromDelegated<Self>>::from_delegated(member.clone())
            .is_ok_and(|read| same_number(read.get_time(), self.get_time()))
    }
}

impl SameMember for File {
    fn same_member(&self, member: &Value) -> bool {
        same_value(self.held(), member)
    }
}

impl SameMember for Any {
    fn same_member(&self, member: &Value) -> bool {
        same_value(self.held(), member)
    }
}

impl SameMember for JsValue {
    fn same_member(&self, member: &Value) -> bool {
        *self == member.to_js()
    }
}

impl SameMember for JsObject {
    fn same_member(&self, member: &Value) -> bool {
        <Self as FromDelegated<Self>>::from_delegated(member.clone())
            .is_ok_and(|read| read == *self)
    }
}

impl SameMember for () {
    fn same_member(&self, _: &Value) -> bool {
        true
    }
}

impl<X: SameMember> SameMember for Option<X> {
    fn same_member(&self, member: &Value) -> bool {
        match self {
            None => member.is_nullish(),
            Some(value) => !member.is_nullish() && value.same_member(member),
        }
    }
}

impl<X: SameMember> SameMember for Vec<X> {
    fn same_member(&self, member: &Value) -> bool {
        match member {
            Value::List(items) => {
                items.len() == self.len()
                    && self
                        .iter()
                        .zip(items.iter())
                        .all(|(value, item)| value.same_member(item))
            }
            Value::Undefined => self.is_empty(),
            _ => false,
        }
    }
}

impl<X: SameMember> SameMember for Box<X> {
    fn same_member(&self, member: &Value) -> bool {
        (**self).same_member(member)
    }
}

impl<A, R> SameMember for Callable<A, R> {
    /// Returns `true`, since a field with arguments holds no value and the executor reads it from the kept object.
    fn same_member(&self, _: &Value) -> bool {
        true
    }
}

/// Returns whether the record that keeps `kept` was read from `member`, the same remote object.
///
/// The derive answers [`SameMember`] of a pulled record through it, beside
/// its own question whether a field of the record changed.
#[must_use]
pub fn kept_is(kept: Option<&Value>, member: &Value) -> bool {
    matches!(
        (kept, member),
        (Some(Value::Object(kept)), Value::Object(member)) if Rc::ptr_eq(kept, member)
    )
}
