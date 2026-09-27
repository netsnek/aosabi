// SPDX-License-Identifier: MIT
/*
 * aosabi-execute value: the JavaScript value a resolver returns or throws.
 *
 * No analogue in the reference, where the value is whatever the engine holds.
 * The variants are the ECMA-262 language types the executor tells apart,
 * https://tc39.es/ecma262/#sec-ecmascript-language-types, and the completion
 * is the completion record of https://tc39.es/ecma262/#sec-completion-record-specification-type.
 */

//! The dynamic value of the executor.
//!
//! The reference executor is written over `unknown`. It asks a value whether
//! it is nullish, a promise, an error, iterable or an object, and it hands
//! the value on to user code untouched. [`Value`] is that value: one enum
//! with a variant for every answer the executor acts on, which a resolver
//! returns, a promise settles with and a throw carries.
//!
//! | Module | Holds |
//! | --- | --- |
//! | [`object`] | [`ObjectLike`], [`PlainObject`], [`Read`], [`Selected`], [`Thrown`], [`read_member`], [`select_members`] and [`HOST_GETTERS`] |
//! | [`function`] | [`Function`] |
//! | [`error`] | [`ErrorObject`], [`ErrorName`] and [`to_error`] |
//! | [`convert`] | the conversions between [`Value`] and the JSON value model |
//! | [`inspect`](mod@inspect) | [`inspect()`] and [`cross_inspect`], the two value printers of a message |
//! | [`to_string`] | [`to_js_string`], ECMA-262 `ToString` |
//! | [`iter`] | [`IteratorObject`], [`is_async_iterable`], [`get_async_iterator`], [`get_method`], [`iter_result`] and [`iter_result_parts`], the async iterator protocol of ECMA-262 |
//!
//! A value is `Rc` based and belongs to one request on one thread. It is
//! neither `Send` nor `Sync`, and it never enters a schema, a resolver or a
//! service, which are shared between requests.

pub mod convert;
pub mod error;
pub mod function;
pub mod inspect;
pub mod iter;
pub mod object;
pub mod to_string;

use alloc::{
    rc::Rc,
    string::String,
    sync::Arc,
    vec::Vec, //
};
use core::{
    any::Any,
    convert::Infallible,
    fmt, //
};

use aosabi_json::JsString;
use aosabi_scalars::JsDate;
use aosabi_type::{
    Directive,
    Schema,
    Type, //
};

pub use self::{
    error::{
        ErrorName,
        ErrorObject,
        to_error, //
    },
    function::Function,
    inspect::{
        cross_inspect,
        inspect, //
    },
    iter::{
        IteratorObject,
        get_async_iterator,
        get_method,
        is_async_iterable,
        iter_result,
        iter_result_parts, //
    },
    object::{
        HOST_GETTERS,
        MemberKind,
        MemberShape,
        MemberSlot,
        NO_MEMBER,
        ObjectLike,
        PlainObject,
        Read,
        Selected,
        Shape,
        Thrown,
        read_member,
        select_members, //
    },
    to_string::to_js_string, //
};
use crate::{
    arena::NodeRef,
    execute::ResolveInfo,
    introspection::Definition,
    promise::Promise, //
};

/// Any JavaScript value a resolver, a completion or a thrown position carries.
///
/// A clone is the copy JavaScript makes when it passes a value on: a
/// primitive is copied, and a list, an object, a function, a promise and an
/// error keep their identity, because the clone shares the `Rc`.
#[derive(Clone)]
pub enum Value {
    /// The value `undefined`.
    Undefined,
    /// The value `null`.
    Null,
    /// A boolean.
    Bool(bool),
    /// A number, which is always a double.
    Number(f64),
    /// A string.
    String(JsString),
    /// A `Date` instance.
    Date(JsDate),
    /// An array. The executor reads a list and never writes one, so the
    /// items sit behind the `Rc` without a cell.
    List(Rc<Vec<Value>>),
    /// An object with properties, a user object or a [`PlainObject`].
    Object(Rc<dyn ObjectLike>),
    /// A function.
    Function(Function),
    /// A promise, the one thenable of the model.
    Promise(Promise),
    /// An instance of `Error` or of a subclass of it.
    Error(Rc<ErrorObject>),
    /// A value of the executor itself.
    Host(HostValue),
}

/// The outcome of a call, a normal completion or a throw completion.
///
/// Translation of the completion record of ECMA-262 section 6.2.4, of which a
/// call produces these two kinds.
#[derive(Debug)]
pub enum Completion {
    /// The call returned the value.
    Normal(Value),
    /// The call threw the value.
    Throw(Value),
}

/// A value of the executor itself that user code passes through and never inspects.
///
/// A schema, a type and a directive share the definition of the type layer,
/// and a field, an argument, an input field and an enum value are a
/// [`Definition`], their position in the schema of the execution, so a
/// [`Value`], which is moved through every reaction and cloned wherever
/// JavaScript copies a reference, holds no copy of a definition.
#[derive(Clone)]
pub enum HostValue {
    /// The context value of the request.
    Context(ContextValue),
    /// The `info` argument of a resolver.
    Info(Rc<ResolveInfo>),
    /// A schema, which the introspection field `__schema` resolves to.
    Schema(Arc<Schema>),
    /// A type, which `__type` and the type fields of introspection resolve to.
    Type(Type),
    /// A field, an argument, an input field or an enum value of the schema
    /// of the execution, which the fields of introspection resolve to.
    Definition(Definition),
    /// A directive definition.
    Directive(Arc<Directive>),
    /// A host object that user code passes through and never inspects.
    ///
    /// A `File` of a multipart request is one: the `File` scalar has no bound
    /// resolver, so the file passes the coercion of its variable untouched
    /// and reaches the resolver as the object the form parser made. The
    /// object is `Send` and `Sync`, because it rides in the JSON value of a
    /// variable, which the executor shares with the request that holds it.
    Opaque(Arc<dyn Any + Send + Sync>),
    /// A node of an arena, a completed value on its way to the node that
    /// holds it.
    ///
    /// The executor reads it in the arena of its own execution, for the
    /// completed values it places, and every other reader answers it as it
    /// answers an opaque host value, `undefined` in introspection and `{}` in
    /// JSON (see [`crate::arena`]).
    Node(NodeRef),
}

/// The context value of a request, which the executor hands to every resolver untouched.
pub type ContextValue = Rc<dyn Any>;

impl Value {
    /// Returns whether the value is `undefined` or `null`, which is `value == null`.
    ///
    /// The loose comparison with `null` holds for these two values and for no
    /// other.
    #[must_use]
    pub fn is_nullish(&self) -> bool {
        matches!(self, Self::Undefined | Self::Null)
    }

    /// Returns whether the value is a thenable, which is `value?.then != null`.
    ///
    /// A promise is the one thenable of the model. A user object that carries
    /// a `then` member is none, because no Rust type produces one.
    #[must_use]
    pub fn is_thenable(&self) -> bool {
        matches!(self, Self::Promise(_))
    }

    /// Returns whether the value is an instance of `Error`.
    #[must_use]
    pub fn is_error(&self) -> bool {
        matches!(self, Self::Error(_))
    }
}

impl fmt::Debug for Value {
    /// Writes an error as its `toString` and any other value the way
    /// [`inspect()`] prints it, which shows an error as an empty object.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Error(error) => f.write_str(&error.to_js_string()),
            value => f.write_str(&inspect(value)),
        }
    }
}

impl Completion {
    /// Returns the completion as a `Result`, a throw being the `Err`.
    ///
    /// # Errors
    ///
    /// Returns the thrown value of a throw completion.
    pub fn into_result(self) -> Result<Value, Value> {
        match self {
            Self::Normal(value) => Ok(value),
            Self::Throw(value) => Err(value),
        }
    }
}

impl From<Result<Value, Value>> for Completion {
    fn from(result: Result<Value, Value>) -> Self {
        match result {
            Ok(value) => Self::Normal(value),
            Err(value) => Self::Throw(value),
        }
    }
}

/// A result that cannot fail is a normal completion, which is what a read of
/// an object answers without the feature `host-reads`, where a [`Thrown`] has
/// no value.
impl From<Result<Value, Infallible>> for Completion {
    fn from(result: Result<Value, Infallible>) -> Self {
        let Ok(value) = result;
        Self::Normal(value)
    }
}

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<f64> for Value {
    fn from(value: f64) -> Self {
        Self::Number(value)
    }
}

impl From<&str> for Value {
    fn from(value: &str) -> Self {
        Self::String(JsString::from(value))
    }
}

impl From<String> for Value {
    fn from(value: String) -> Self {
        Self::String(JsString::from(value))
    }
}

impl From<JsString> for Value {
    fn from(value: JsString) -> Self {
        Self::String(value)
    }
}

impl From<Vec<Value>> for Value {
    fn from(items: Vec<Value>) -> Self {
        Self::List(Rc::new(items))
    }
}

impl From<Rc<PlainObject>> for Value {
    fn from(object: Rc<PlainObject>) -> Self {
        Self::Object(object)
    }
}

impl From<Function> for Value {
    fn from(function: Function) -> Self {
        Self::Function(function)
    }
}

impl From<Promise> for Value {
    fn from(promise: Promise) -> Self {
        Self::Promise(promise)
    }
}

impl From<ErrorObject> for Value {
    fn from(error: ErrorObject) -> Self {
        Self::Error(Rc::new(error))
    }
}
