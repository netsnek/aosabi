// SPDX-License-Identifier: MIT
/*
 * aosabi-core: the contract of the registry aosabi pull writes for a remote
 * schema.
 *
 * No reference file.
 */

//! The contract of a pulled registry.
//!
//! The command `aosabi pull` writes the types of a remote schema and an
//! interface `RemoteRegistry` with two members: `delegate`, the arguments and
//! the return of every field of `Query` and then of `Mutation` under the key
//! `<root>.<field>`, and `types`, every object, interface and union of the type
//! map but the three roots. The command `aosabi pull` writes the same registry
//! as Rust, and the traits here are what it implements.
//!
//! | Reference | Rust |
//! | --- | --- |
//! | the keys of `delegate` and of `types` | [`Registry`] with [`Registry::DELEGATE`] and [`Registry::TYPES`] |
//! | one entry of `delegate`, `{ args; return }` | a struct of the arguments that implements [`Delegate`] |
//! | the value of an argument as the federation sends it | [`IntoArgument`] |
//! | `readonly __typename: '<Name>'` of an object | [`TypeName`], which keeps the remote object a record was read from |
//! | a key of `types` as a type | [`RemoteType`], the registry of a type and its name on the remote |
//! | a method of an object, a field with arguments | [`Callable`], described through the [`Params`] of its arguments |
//! | the structural assignment of a delegate result | [`FromDelegated`] |
//!
//! An argument is the value the federation of the reference hands its
//! transforms: an absent optional argument is undefined and is left out of its
//! object, a `None` inside a list is `null`, a file is the host object it
//! holds, and a date is a host object too, the `Date` a TypeScript registry
//! types the argument as, which the federation writes as an input object with
//! no field and sends as `{}`, quirk
//! `Q83_FEDERATION_DATE_ARGUMENT_EMPTY_OBJECT`.
//!
//! A record a delegation hands back keeps the remote object it was read from,
//! so the executor reads every member of that object and not a view over the
//! fields of the record, the aliased members of a request among them. The key
//! check of a table of patches reads [`Registry::TYPES`], the list of every
//! object, interface and union of the remote but the roots.

pub mod delegated;
pub mod params;

use alloc::{
    rc::Rc,
    string::String,
    vec::Vec, //
};
use core::{
    fmt,
    marker::PhantomData, //
};

use aosabi_execute::value::Value;
use aosabi_json::{
    JsObject,
    JsValue,
    Opaque,
    value::OpaqueHandle, //
};
use aosabi_lang::js_string::to_string_lossy;
use aosabi_scalars::js_date::JsDate;

pub use self::{
    delegated::FromDelegated,
    params::Params, //
};
/// The value a resolver returns where its declared type holds a delegated
/// value only by the structural assignment of TypeScript, which the note of
/// [`FromDelegated`] names.
pub use crate::glue::Unchecked;
use crate::{
    descriptor::{
        Any,
        Date,
        Described,
        File,
        Float,
        ID,
        Int,
        TypeExpr,
        TypeLink,
        callable::CallableInfo, //
    },
    glue::{
        FieldValue,
        IntoValue,
        Shared, //
    }, //
};

/// A pulled registry, the `delegate` keys and the `types` keys of the
/// reference registry.
pub trait Registry: 'static {
    /// `Query.<field>` for every field of `Query`, then `Mutation.<field>`, in
    /// schema order.
    const DELEGATE: &'static [&'static str];
    /// Every object, interface and union but the three roots, in type map
    /// order, which the key check of a table of patches reads.
    const TYPES: &'static [&'static str];
}

/// A type of a pulled registry, with the registry it belongs to and its name
/// on the remote.
///
/// `aosabi pull` lists every object, interface and union of the remote under the
/// key `types` of its registry, and a patch of the federation is keyed by one
/// of those names. The trait ties the Rust type to that name and to its
/// registry, so a patch keyed by a type of another registry fails to compile.
pub trait RemoteType: 'static {
    /// The registry the type belongs to.
    type Registry: Registry;
    /// The name of the type on the remote, which the patch that takes it is
    /// keyed by.
    const TYPENAME: &'static str;
}

/// The arguments of one remote root field, in the order the remote declares
/// them.
pub trait Delegate {
    /// The registry the field belongs to.
    type Registry: Registry;
    /// What the field returns, `Option<T>` where the remote type is nullable.
    type Return;
    /// The key of the reference registry, such as `Query.user`.
    const PATH: &'static str;

    /// Returns the arguments as the object the reference sends, an absent
    /// optional argument left out.
    fn arguments(&self) -> JsObject;
}

/// A value that can be an argument of a remote field.
pub trait IntoArgument {
    /// Returns the value as `JSON.stringify` would see it.
    // The name is the contract a pulled registry calls, and a field of the
    // arguments struct is read, never moved, because one struct serves every
    // call of its field.
    #[expect(
        clippy::wrong_self_convention,
        reason = "a pulled registry calls this name on a borrowed field"
    )]
    fn into_argument(&self) -> JsValue;
}

impl IntoArgument for String {
    fn into_argument(&self) -> JsValue {
        JsValue::from(self.as_str())
    }
}

impl IntoArgument for str {
    fn into_argument(&self) -> JsValue {
        JsValue::from(self)
    }
}

impl IntoArgument for bool {
    fn into_argument(&self) -> JsValue {
        JsValue::Bool(*self)
    }
}

impl IntoArgument for f64 {
    fn into_argument(&self) -> JsValue {
        JsValue::Number(*self)
    }
}

impl IntoArgument for ID {
    fn into_argument(&self) -> JsValue {
        JsValue::from(self.0.as_str())
    }
}

impl IntoArgument for Int {
    fn into_argument(&self) -> JsValue {
        JsValue::Number(self.0)
    }
}

impl IntoArgument for Float {
    fn into_argument(&self) -> JsValue {
        JsValue::Number(self.0)
    }
}

// The marker `JSON` is this type, so one implementation serves both names.
impl IntoArgument for JsValue {
    fn into_argument(&self) -> JsValue {
        self.clone()
    }
}

impl IntoArgument for JsObject {
    fn into_argument(&self) -> JsValue {
        JsValue::Object(self.clone())
    }
}

impl IntoArgument for Any {
    fn into_argument(&self) -> JsValue {
        self.clone().into_value().to_js()
    }
}

impl IntoArgument for Date {
    /// Returns the date as a host object, the `Date` the federation of the reference hands its transforms.
    ///
    /// `aosabi pull` types a remote `Date` argument as the global `Date`, and
    /// `astFromJSValue` writes an object through `Object.entries`, which a
    /// `Date` answers with no entry, so the reference sends every date
    /// argument, valid or not, as the input object `{}`. A host object carried
    /// through unread is what `ast_from_js_value` of the federation and the
    /// writer of the JSON model write as `{}`, so the outbound variables equal
    /// the reference's, quirk `Q83_FEDERATION_DATE_ARGUMENT_EMPTY_OBJECT` of
    /// `aosabi-compat`.
    fn into_argument(&self) -> JsValue {
        JsValue::Opaque(Opaque(host_date(JsDate::from_time_value(self.get_time()))))
    }
}

/// Returns the shared handle of a host object that holds `date`.
#[cfg(target_has_atomic = "ptr")]
fn host_date(date: JsDate) -> OpaqueHandle {
    alloc::sync::Arc::new(date)
}

/// Returns the shared handle of a host object that holds `date`, on a target with no atomic pointer.
#[cfg(not(target_has_atomic = "ptr"))]
fn host_date(date: JsDate) -> OpaqueHandle {
    Rc::new(date)
}

impl IntoArgument for File {
    /// Returns the host object of the upload the marker holds, which
    /// `JSON.stringify` writes as `{}`.
    fn into_argument(&self) -> JsValue {
        self.held().to_js()
    }
}

impl<T: IntoArgument + ?Sized> IntoArgument for Box<T> {
    fn into_argument(&self) -> JsValue {
        (**self).into_argument()
    }
}

impl<T: IntoArgument> IntoArgument for Vec<T> {
    /// Returns the elements in order, an absent element as `null`, which is
    /// where `JSON.stringify` writes an undefined element of an array.
    fn into_argument(&self) -> JsValue {
        let items = self
            .iter()
            .map(|item| match item.into_argument() {
                JsValue::Undefined => JsValue::Null,
                value => value,
            })
            .collect();
        JsValue::Array(items)
    }
}

impl<T: IntoArgument> IntoArgument for Option<T> {
    /// Returns undefined for `None`, which leaves the argument out of its
    /// object and is `null` inside a list.
    fn into_argument(&self) -> JsValue {
        self.as_ref()
            .map_or(JsValue::Undefined, IntoArgument::into_argument)
    }
}

/// The literal `__typename` of a remote object, and the remote object a
/// record was read from.
///
/// A record built in Rust holds none, and its object is a view over its
/// fields. A record read by [`FromDelegated`] holds the value it was read
/// from, which its object answers in place of its fields, so a member its
/// fields do not name, an aliased one among them, reads as the remote object
/// carries it.
#[derive(Clone, Default)]
pub struct TypeName(Option<Value>);

impl TypeName {
    /// Returns the slot that keeps `value`, the remote object a record is
    /// read from.
    #[must_use]
    pub const fn keep(value: Value) -> Self {
        Self(Some(value))
    }

    /// Returns the remote object the record was read from, none for a
    /// record built in Rust.
    #[must_use]
    pub const fn data(&self) -> Option<&Value> {
        self.0.as_ref()
    }

    /// Returns the `__typename` the remote object carries, none for a
    /// record built in Rust and for an object whose typename is no string.
    #[must_use]
    pub fn name(&self) -> Option<String> {
        match self.member("__typename")? {
            Value::String(text) => Some(to_string_lossy(&text).into_owned()),
            _ => None,
        }
    }

    /// Returns `object[key]` of the kept remote object, none for a record
    /// built in Rust and for a kept value that is no object.
    #[must_use]
    pub fn member(&self, key: &str) -> Option<Value> {
        match &self.0 {
            Some(Value::Object(object)) => Some(Rc::clone(object).get(key)),
            _ => None,
        }
    }

    /// Returns `Object.keys` of the kept remote object, none for a record
    /// built in Rust and for a kept value that is no object.
    #[must_use]
    pub fn keys(&self) -> Option<Vec<String>> {
        match &self.0 {
            Some(Value::Object(object)) => Some(object.own_keys()),
            _ => None,
        }
    }
}

impl fmt::Debug for TypeName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            Some(value) => write!(f, "TypeName({value:?})"),
            None => f.write_str("TypeName"),
        }
    }
}

/// A field with arguments on a remote object, the method signature of the
/// reference registry.
///
/// `A` is the arguments struct of the field and `R` what it returns. It is
/// described as a callable with the parameters of `A` returning `R`, the method
/// `orders(status?, limit?)` the reference writes, so a front that hands the
/// member on presents it as the reference presents it. Its value is
/// `undefined`, since the executor reads the member from the remote object a
/// record keeps.
pub struct Callable<A, R>(PhantomData<(A, R)>);

impl<A, R> Callable<A, R> {
    /// Returns the signature.
    #[must_use]
    pub const fn new() -> Self {
        Self(PhantomData)
    }
}

impl<A, R> Default for Callable<A, R> {
    fn default() -> Self {
        Self::new()
    }
}

impl<A, R> Clone for Callable<A, R> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<A, R> Copy for Callable<A, R> {}

impl<A, R> fmt::Debug for Callable<A, R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Callable")
    }
}

impl<A: Params + 'static, R: Described> Described for Callable<A, R> {
    const TYPE: TypeExpr = TypeExpr::Callable(&CallableInfo {
        params: A::PARAMS,
        returns: TypeLink::of::<R>(),
    });
}

impl<A, R> IntoValue for Callable<A, R> {
    fn into_value(self) -> Value {
        Value::Undefined
    }
}

impl<A: 'static, R: 'static> FieldValue for Callable<A, R> {
    fn field_value(&self, _: &dyn Fn() -> Shared<Self>) -> Value {
        Value::Undefined
    }
}
