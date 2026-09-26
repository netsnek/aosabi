// SPDX-License-Identifier: MIT
/*
 * aosabi-type: the type definitions of a schema, from the scalar to the input
 * object, the two wrapping types, and the predicates over all of them.
 *
 * The helpers that file imports, inspect, didYouMean, suggestionList and
 * valueFromASTUntyped, are the sibling modules of the same names, and this
 * file calls them.
 *
 * Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
 */

//! The type definitions of the type layer.
//!
//! Each class of the reference is a struct here, and each `getX` method keeps
//! its name in snake case. The static unions of the reference,
//! `GraphQLInputType`, `GraphQLOutputType`, `GraphQLLeafType` and the others,
//! have no runtime existence in graphql-js, and a field built from SDL can
//! carry an input object in an output position, which `validateSchema` then
//! reports. So there are two enums, [`Type`] for every type and [`NamedType`]
//! for the six named ones, and every other union of the reference is a
//! predicate over a [`Type`], as `isInputType` is over an `unknown`.
//!
//! # Ownership and identity
//!
//! A named type is shared through an [`Arc`], because a schema is a graph with
//! cycles and every field, argument and union member points into it. Two named
//! types are the same type when they are the same allocation, which is the
//! `===` of the reference, and [`NamedType`] compares and hashes that way. A
//! wrapping type holds the type it wraps through a [`SharedType`] as well, so
//! a clone of a [`Type`] costs one count whatever its wrappers, while every
//! new wrapper is an allocation of its own, as every `new GraphQLList(type)`
//! of the reference is a fresh object. [`Type`] compares structurally, which
//! is what `isEqualType` computes and the only comparison the reference ever
//! makes on a wrapping type.
//!
//! # Thunks
//!
//! The fields, the interfaces, the union members and the enum values of a type
//! are thunks, so that a type can refer to itself and to types defined after
//! it. The reference resolves a thunk on first read and assigns the result
//! over it. Here the result lands in a race cell, [`OnceBox`], and a thunk
//! that fails is run again on the next read, exactly as the reference's is,
//! because the assignment there happens only after the call returns. A race
//! cell needs no lock and is `Sync`, so every type is `Send` and `Sync`, and
//! the built in types live in statics, which gives them the one identity the
//! module level constants of the reference have.
//!
//! The crate builds with no standard library, because the bare metal build of
//! a microcontroller card links it. A cell that is `Sync` there has to turn a
//! stored raw pointer back into a reference, which safe code cannot write, so
//! the race cell of `once_cell` carries every value that is built lazily. What
//! the built in types know when the crate is compiled, their names,
//! descriptions, locations, coercion functions and resolver tags, is read only
//! data, and a [`ResolverHandle`] or a coercion function of that data borrows
//! it rather than counting a copy on the heap.
//!
//! # The value model
//!
//! An internal or external value of a scalar or an enum is a [`JsValue`].
//! The `typeof` branches of the reference read as follows: `string` is
//! [`JsValue::String`], `number` is [`JsValue::Number`], which carries `NaN`
//! and the infinities that JSON cannot, `boolean` is [`JsValue::Bool`],
//! `undefined` is [`JsValue::Undefined`], and `object` is [`JsValue::Null`],
//! [`JsValue::Array`] or [`JsValue::Object`]. A function, a symbol and a
//! bigint have no representation, because no Rust resolver produces one, and
//! the branches of the reference that read them have no item here.
//!
//! # Errors
//!
//! The reference throws two kinds of error from this layer, a `GraphQLError`
//! from `assertName` and from the coercion of a scalar or an enum, and a plain
//! `Error` from `devAssert`, from an assertion predicate and from the schema
//! constructor. [`DefinitionError`] carries both, and it converts into the
//! [`OriginalError`] of the language layer, which is what `locatedError`
//! takes.
//!
//! # Resolver slots
//!
//! `resolve`, `subscribe`, `isTypeOf` and `resolveType` are functions of the
//! executor, and their signature is the executor's decision. Each slot holds
//! a [`ResolverHandle`], a shared value the executor downcasts, so this crate
//! carries the slots without deciding their shape.

use alloc::{
    borrow::Cow,
    boxed::Box,
    format,
    string::{
        String,
        ToString, //
    },
    sync::Arc,
    vec::Vec, //
};
use core::{
    any::Any,
    fmt,
    hash::{
        Hash,
        Hasher, //
    },
    ops::Deref, //
};

use aosabi_json::{
    JsObject,
    JsString,
    JsValue,
    map::IndexMap,
    value::{
        KeyHasherBuilder,
        array_index, //
    }, //
};
use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions,
    OriginalError,
    ast::{
        AstNode,
        EnumTypeDefinitionNode,
        EnumTypeExtensionNode,
        EnumValueDefinitionNode,
        FieldDefinitionNode,
        InputObjectTypeDefinitionNode,
        InputObjectTypeExtensionNode,
        InputValueDefinitionNode,
        InterfaceTypeDefinitionNode,
        InterfaceTypeExtensionNode,
        ObjectTypeDefinitionNode,
        ObjectTypeExtensionNode,
        ScalarTypeDefinitionNode,
        ScalarTypeExtensionNode,
        UnionTypeDefinitionNode,
        UnionTypeExtensionNode,
        ValueNode, //
    },
    js_string::to_string_lossy,
    print, //
};
use once_cell::race::OnceBox;

use crate::{
    assert_name::{
        assert_enum_value_name,
        assert_name, //
    },
    did_you_mean::did_you_mean,
    inspect::inspect,
    suggestion_list::suggestion_list,
    value_from_ast_untyped::value_from_ast_untyped, //
};

/// A map keyed by name in insertion order.
///
/// The null prototype object the reference keeps fields, arguments, enum
/// values and types in. An [`IndexMap`] is the only accepted carrier of an
/// ordered map in this workspace, and like every map of the workspace it
/// carries the [`KeyHasherBuilder`] of `aosabi-json`.
///
/// Derived from graphql-js 16.12.0, src/jsutils/ObjMap.ts, MIT.
pub type ObjMap<T> = IndexMap<String, T, KeyHasherBuilder>;

/// The pointer the type layer shares a type and the scope of a builder through.
///
/// It is `alloc::sync::Arc` while a schema is shared across threads.
pub type SharedType<T> = Arc<T>;

/// The error of the type layer.
///
/// The reference throws a `GraphQLError` from `assertName` and from the
/// coercion functions, and a plain `Error` from `devAssert`, from an assertion
/// predicate and from the schema constructor. Both reach a caller through this
/// enum, and `message()` reads the one property every consumer of the
/// reference reads. A message is a JavaScript string on both arms, because a
/// syntax error may quote the body up to a leading surrogate, and `Display`
/// prints the text of `to_string_lossy`. `Debug` shows the variant and a
/// message as the JSON string the writer of `aosabi-json` makes of it, as the
/// `Debug` of [`GraphQLError`] does.
pub enum DefinitionError {
    /// A `GraphQLError`, boxed because it is wide and the error path runs
    /// at most once per schema.
    GraphQL(Box<GraphQLError>),
    /// A plain `Error`, carried as its message.
    Plain(JsString),
}

impl DefinitionError {
    /// Returns a plain `Error` with `message`.
    ///
    /// This is `new Error(message)` and the failing branch of `devAssert`.
    pub fn plain(message: impl Into<JsString>) -> Self {
        Self::Plain(message.into())
    }

    /// Returns a `GraphQLError` with `message` and no options.
    ///
    /// This is `new GraphQLError(message)`.
    pub fn graphql(message: impl Into<JsString>) -> Self {
        Self::from(GraphQLError::new(message, GraphQLErrorOptions::default()))
    }

    /// Returns the message of the error.
    #[must_use]
    pub const fn message(&self) -> &JsString {
        match self {
            Self::GraphQL(error) => &error.message,
            Self::Plain(message) => message,
        }
    }
}

impl fmt::Display for DefinitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&to_string_lossy(self.message()))
    }
}

impl fmt::Debug for DefinitionError {
    /// Writes the variant around the `Debug` of the error or the JSON string
    /// of the message.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GraphQL(error) => f.debug_tuple("GraphQL").field(error).finish(),
            Self::Plain(message) => f
                .debug_tuple("Plain")
                .field(&JsValue::from(message.clone()))
                .finish(),
        }
    }
}

impl core::error::Error for DefinitionError {}

impl From<GraphQLError> for DefinitionError {
    fn from(error: GraphQLError) -> Self {
        Self::GraphQL(Box::new(error))
    }
}

impl From<DefinitionError> for OriginalError {
    fn from(error: DefinitionError) -> Self {
        match error {
            DefinitionError::GraphQL(error) => Self::GraphQL(error),
            DefinitionError::Plain(message) => {
                Self::Other(Box::new(DefinitionError::Plain(message)))
            }
        }
    }
}

/// A shared value, borrowed from read only data or counted on the heap.
///
/// A handle that the executor or a schema builder fills at runtime counts its
/// value through an [`Arc`]. A handle of a built in type borrows a value the
/// image carries, which costs no allocation and no initialisation at runtime.
enum Shared<T: ?Sized + 'static> {
    /// A value of the image.
    Static(&'static T),
    /// A value on the heap.
    Counted(Arc<T>),
}

impl<T: ?Sized> Clone for Shared<T> {
    fn clone(&self) -> Self {
        match self {
            Self::Static(value) => Self::Static(value),
            Self::Counted(value) => Self::Counted(Arc::clone(value)),
        }
    }
}

impl<T: ?Sized> Deref for Shared<T> {
    type Target = T;

    fn deref(&self) -> &T {
        match self {
            Self::Static(value) => value,
            Self::Counted(value) => value,
        }
    }
}

/// A function of the executor, held by a field or a type without a shape.
///
/// The reference stores `resolve`, `subscribe`, `isTypeOf` and `resolveType`
/// on the definition and calls them from `execute.ts`, and the signature of
/// those calls belongs to the executor of Aosabi. The handle keeps the slot
/// without deciding that signature. The executor puts its function in with
/// [`ResolverHandle::new`] and reads it back with
/// [`ResolverHandle::downcast_ref`].
#[derive(Clone)]
pub struct ResolverHandle(Shared<dyn Any + Send + Sync>);

impl ResolverHandle {
    /// Wraps `value`, any shareable value of the executor.
    pub fn new<T: Any + Send + Sync>(value: T) -> Self {
        Self(Shared::Counted(Arc::new(value)))
    }

    /// Borrows `value`, a value of read only data, without an allocation.
    ///
    /// The introspection fields store their resolver tags this way, so the
    /// tag lives in the table that describes the field.
    pub(crate) const fn from_static(value: &'static (dyn Any + Send + Sync)) -> Self {
        Self(Shared::Static(value))
    }

    /// Returns the value as `T`, or `None` when the handle holds another type.
    #[must_use]
    pub fn downcast_ref<T: Any>(&self) -> Option<&T> {
        self.0.downcast_ref::<T>()
    }
}

impl fmt::Debug for ResolverHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ResolverHandle")
    }
}

/// The `resolve` or `subscribe` function of a field.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
pub type FieldResolver = ResolverHandle;

/// The `resolveType` function of an interface or a union.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
pub type TypeResolver = ResolverHandle;

/// The `isTypeOf` function of an object type.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
pub type IsTypeOfFn = ResolverHandle;

/// The shape of a function over one value, which a serializer and a value
/// parser share.
pub type ValueFn = dyn Fn(&JsValue) -> Result<JsValue, DefinitionError> + Send + Sync;

/// The shape of a value parser that tells whether its internal value is its
/// input.
pub type ParseFn = dyn Fn(&JsValue) -> Result<Parsed, DefinitionError> + Send + Sync;

/// What a value parser answers for an input it accepts.
///
/// The `parseValue` of the specified scalars returns `inputValue` for every
/// value it accepts but the integer an `ID` turns into its text, and so does
/// the `Number` scalar, so a parser answers [`Parsed::Input`] for
/// such an input and the caller keeps the value it holds, with no copy.
#[derive(Debug)]
pub enum Parsed {
    /// The internal value is the input itself.
    Input,
    /// The internal value, another value than the input.
    Value(JsValue),
}

impl Parsed {
    /// Returns the internal value, a copy of `input` where it is the input
    /// itself.
    ///
    /// A number, a boolean and a string are copied as the leaves they are,
    /// and an object, a list or a host object a custom scalar keeps is
    /// copied whole.
    #[must_use]
    pub fn into_value(self, input: &JsValue) -> JsValue {
        match (self, input) {
            (Self::Value(value), _) => value,
            (Self::Input, JsValue::Number(number)) => JsValue::Number(*number),
            (Self::Input, JsValue::Bool(flag)) => JsValue::Bool(*flag),
            (Self::Input, JsValue::String(text)) => JsValue::String(text.clone()),
            (Self::Input, other) => other.clone(),
        }
    }
}

/// The shape of a function over a literal and the variables.
pub type LiteralFn =
    dyn Fn(&ValueNode, Option<&JsObject>) -> Result<JsValue, DefinitionError> + Send + Sync;

/// The function that turns an internal value into a response value.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
#[derive(Clone)]
pub struct ScalarSerializer(Shared<ValueFn>);

impl ScalarSerializer {
    /// Wraps `function`.
    pub fn new(
        function: impl Fn(&JsValue) -> Result<JsValue, DefinitionError> + Send + Sync + 'static,
    ) -> Self {
        Self(Shared::Counted(Arc::new(function)))
    }

    /// Borrows `function`, a function of read only data, without an
    /// allocation.
    pub(crate) const fn from_static(function: &'static ValueFn) -> Self {
        Self(Shared::Static(function))
    }

    /// Calls the function on `output_value`.
    ///
    /// # Errors
    ///
    /// Returns what the function throws.
    pub fn call(&self, output_value: &JsValue) -> Result<JsValue, DefinitionError> {
        (self.0)(output_value)
    }
}

impl fmt::Debug for ScalarSerializer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ScalarSerializer")
    }
}

/// The function that turns an external value into an internal value.
///
/// A parser answers the internal value, as [`ScalarValueParser::new`] wraps
/// it, or tells whether the internal value is its input, as
/// [`ScalarValueParser::keeping`] wraps it, so a caller that holds the input
/// keeps it with no copy.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
#[derive(Clone)]
pub struct ScalarValueParser(ValueParser);

/// The two forms of a value parser.
#[derive(Clone)]
enum ValueParser {
    /// A function that answers the internal value.
    Answering(Shared<ValueFn>),
    /// A function that tells whether the internal value is its input.
    Keeping(Shared<ParseFn>),
}

impl ScalarValueParser {
    /// Wraps `function`, which answers the internal value.
    pub fn new(
        function: impl Fn(&JsValue) -> Result<JsValue, DefinitionError> + Send + Sync + 'static,
    ) -> Self {
        Self(ValueParser::Answering(Shared::Counted(Arc::new(function))))
    }

    /// Wraps `function`, which answers [`Parsed::Input`] where the internal
    /// value is its input.
    pub fn keeping(
        function: impl Fn(&JsValue) -> Result<Parsed, DefinitionError> + Send + Sync + 'static,
    ) -> Self {
        Self(ValueParser::Keeping(Shared::Counted(Arc::new(function))))
    }

    /// Borrows `function`, a function of read only data that tells whether
    /// the internal value is its input, without an allocation.
    pub(crate) const fn from_static(function: &'static ParseFn) -> Self {
        Self(ValueParser::Keeping(Shared::Static(function)))
    }

    /// Calls the function on `input_value` and returns the internal value.
    ///
    /// An internal value that is the input itself is a copy of it.
    ///
    /// # Errors
    ///
    /// Returns what the function throws.
    pub fn call(&self, input_value: &JsValue) -> Result<JsValue, DefinitionError> {
        Ok(self.parse(input_value)?.into_value(input_value))
    }

    /// Calls the function on `input_value` and tells whether the internal
    /// value is the input itself.
    ///
    /// A function that answers the internal value answers
    /// [`Parsed::Value`].
    ///
    /// # Errors
    ///
    /// Returns what the function throws.
    pub fn parse(&self, input_value: &JsValue) -> Result<Parsed, DefinitionError> {
        match &self.0 {
            ValueParser::Answering(function) => function(input_value).map(Parsed::Value),
            ValueParser::Keeping(function) => function(input_value),
        }
    }
}

impl fmt::Debug for ScalarValueParser {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ScalarValueParser")
    }
}

/// The function that turns a value literal into an internal value.
///
/// `variables` is the variable map the executor passes, and it is absent when
/// there is none.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
#[derive(Clone)]
pub struct ScalarLiteralParser(Shared<LiteralFn>);

impl ScalarLiteralParser {
    /// Wraps `function`.
    pub fn new(
        function: impl Fn(&ValueNode, Option<&JsObject>) -> Result<JsValue, DefinitionError>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self(Shared::Counted(Arc::new(function)))
    }

    /// Borrows `function`, a function of read only data, without an
    /// allocation.
    pub(crate) const fn from_static(function: &'static LiteralFn) -> Self {
        Self(Shared::Static(function))
    }

    /// Calls the function on `value_node` with `variables`.
    ///
    /// # Errors
    ///
    /// Returns what the function throws.
    pub fn call(
        &self,
        value_node: &ValueNode,
        variables: Option<&JsObject>,
    ) -> Result<JsValue, DefinitionError> {
        (self.0)(value_node, variables)
    }
}

impl fmt::Debug for ScalarLiteralParser {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ScalarLiteralParser")
    }
}

/// A thunk, a function a type calls when it first needs the value.
///
/// It is `Fn` and not `FnOnce`, because the reference calls a thunk that
/// throws again on the next read, and it may throw, because a thunk built from
/// SDL resolves type names and throws on an unknown one.
pub type ThunkFn<T> = Box<dyn Fn() -> Result<T, DefinitionError> + Send + Sync>;

/// A list, or a thunk that returns one.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
pub enum ThunkReadonlyArray<T> {
    /// The list itself.
    Array(Vec<T>),
    /// A function that returns the list.
    Thunk(ThunkFn<Vec<T>>),
}

impl<T> ThunkReadonlyArray<T> {
    /// Wraps `function` as the thunk form.
    pub fn thunk(
        function: impl Fn() -> Result<Vec<T>, DefinitionError> + Send + Sync + 'static,
    ) -> Self {
        Self::Thunk(Box::new(function))
    }
}

impl<T> Default for ThunkReadonlyArray<T> {
    fn default() -> Self {
        Self::Array(Vec::new())
    }
}

impl<T> From<Vec<T>> for ThunkReadonlyArray<T> {
    fn from(items: Vec<T>) -> Self {
        Self::Array(items)
    }
}

impl<T: fmt::Debug> fmt::Debug for ThunkReadonlyArray<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Array(items) => f.debug_tuple("Array").field(items).finish(),
            Self::Thunk(_) => f.write_str("Thunk"),
        }
    }
}

/// A map keyed by name, or a thunk that returns one.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
pub enum ThunkObjMap<T> {
    /// The map itself.
    Map(ObjMap<T>),
    /// A function that returns the map.
    Thunk(ThunkFn<ObjMap<T>>),
}

impl<T> ThunkObjMap<T> {
    /// Wraps `function` as the thunk form.
    pub fn thunk(
        function: impl Fn() -> Result<ObjMap<T>, DefinitionError> + Send + Sync + 'static,
    ) -> Self {
        Self::Thunk(Box::new(function))
    }
}

impl<T> Default for ThunkObjMap<T> {
    fn default() -> Self {
        Self::Map(ObjMap::default())
    }
}

impl<T> From<ObjMap<T>> for ThunkObjMap<T> {
    fn from(map: ObjMap<T>) -> Self {
        Self::Map(map)
    }
}

impl<T: fmt::Debug> fmt::Debug for ThunkObjMap<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Map(map) => f.debug_tuple("Map").field(map).finish(),
            Self::Thunk(_) => f.write_str("Thunk"),
        }
    }
}

/// Returns the list a thunk stands for, calling it when it is a function.
///
/// The list form is borrowed and the function form is owned, which is what the
/// reference's `typeof thunk === 'function' ? thunk() : thunk` returns.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
///
/// # Errors
///
/// Returns what the thunk throws.
pub fn resolve_readonly_array_thunk<T: Clone>(
    thunk: &ThunkReadonlyArray<T>,
) -> Result<Cow<'_, [T]>, DefinitionError> {
    match thunk {
        ThunkReadonlyArray::Array(items) => Ok(Cow::Borrowed(items)),
        ThunkReadonlyArray::Thunk(function) => function().map(Cow::Owned),
    }
}

/// Returns the map a thunk stands for, calling it when it is a function.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
///
/// # Errors
///
/// Returns what the thunk throws.
pub fn resolve_obj_map_thunk<T: Clone>(
    thunk: &ThunkObjMap<T>,
) -> Result<Cow<'_, ObjMap<T>>, DefinitionError> {
    match thunk {
        ThunkObjMap::Map(map) => Ok(Cow::Borrowed(map)),
        ThunkObjMap::Thunk(function) => function().map(Cow::Owned),
    }
}

/// Returns the entries of a map in the order `Object.entries` walks them.
///
/// `Object.entries` lists the own keys of an object through
/// `OrdinaryOwnPropertyKeys`, which puts every key that is an array index
/// first in ascending numeric order and the rest in insertion order. A key
/// that passes `assertName` is never an array index, because a name starts
/// with a letter or an underscore, so the order only shows on a config map
/// that has not been asserted yet, and that is where the reference walks it.
///
/// [keys]: https://tc39.es/ecma262/#sec-ordinaryownpropertykeys
pub(crate) fn own_entries<T>(map: &ObjMap<T>) -> Vec<(&String, &T)> {
    let mut indexed: Vec<(u32, (&String, &T))> = Vec::new();
    let mut named: Vec<(&String, &T)> = Vec::new();
    for entry in map {
        match array_index(entry.0.as_bytes()) {
            Some(index) => indexed.push((index, entry)),
            None => named.push(entry),
        }
    }
    indexed.sort_by_key(|(index, _)| *index);
    indexed
        .into_iter()
        .map(|(_, entry)| entry)
        .chain(named)
        .collect()
}

pub use aosabi_json::number::number_to_string;

/// Returns the number `parseInt(text, 10)` or `parseFloat(text)` reads from a
/// numeric token.
///
/// An `IntValue` token is a decimal integer and a `FloatValue` token is a
/// decimal with a fraction or an exponent, and both clauses read the
/// mathematical value of such a text rounded to the nearest double. The Rust
/// parser rounds a decimal the same way. `parseInt` may replace every digit
/// after the twentieth by a zero at the option of the implementation, and V8
/// does not.
///
/// [parseint]: https://tc39.es/ecma262/#sec-parseint-string-radix
/// [parsefloat]: https://tc39.es/ecma262/#sec-parsefloat-string
#[must_use]
pub fn parse_number_token(text: &str) -> f64 {
    text.parse::<f64>().unwrap_or(f64::NAN)
}

/// Returns the text of a JavaScript string for a message.
///
/// A string that holds a lone surrogate has no `str` form, and the replacement
/// character stands in for it.
fn js_string_text(text: &JsString) -> String {
    String::from_utf8_lossy(text.as_wtf8()).into_owned()
}

/// Returns whether two values are the same under `SameValueZero`.
///
/// A `Map` keys its entries by `SameValueZero`, under which `NaN` is itself
/// and the two zeros are one. An object or an array is a reference in the
/// reference and a value here, and two values have no identity to share, so
/// they are never the same key.
///
/// [samevaluezero]: https://tc39.es/ecma262/#sec-samevaluezero
#[expect(
    clippy::float_cmp,
    reason = "the exact comparison of two doubles is the clause"
)]
fn same_value_zero(a: &JsValue, b: &JsValue) -> bool {
    match (a, b) {
        (JsValue::Undefined, JsValue::Undefined) | (JsValue::Null, JsValue::Null) => true,
        (JsValue::Bool(a), JsValue::Bool(b)) => a == b,
        (JsValue::Number(a), JsValue::Number(b)) => (a.is_nan() && b.is_nan()) || a == b,
        (JsValue::String(a), JsValue::String(b)) => a == b,
        _ => false,
    }
}

/// Returns `GraphQLError` options that blame one node.
///
/// This is `{ nodes: valueNode }`, and the deprecated positional form the
/// `Float` scalar uses reads the same, because a node has a `kind` and
/// `toNormalizedOptions` takes that as the nodes argument.
pub(crate) fn blame(value_node: &ValueNode) -> GraphQLErrorOptions {
    GraphQLErrorOptions {
        nodes: Some(alloc::vec![AstNode::from(value_node.clone())]),
        ..GraphQLErrorOptions::default()
    }
}

// Predicates & Assertions

/// Any GraphQL type.
///
/// A named type is shared, and a wrapping type is shared with the type it
/// wraps, so a clone of any variant counts one reference and copies no
/// wrapper.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
#[derive(Clone)]
pub enum Type {
    /// A scalar type.
    Scalar(Arc<ScalarType>),
    /// An object type.
    Object(Arc<ObjectType>),
    /// An interface type.
    Interface(Arc<InterfaceType>),
    /// A union type.
    Union(Arc<UnionType>),
    /// An enum type.
    Enum(Arc<EnumType>),
    /// An input object type.
    InputObject(Arc<InputObjectType>),
    /// A list type.
    List(SharedType<ListType>),
    /// A non null type.
    NonNull(SharedType<NonNullType>),
}

impl Type {
    /// Returns the scalar type this is, if it is one.
    #[must_use]
    pub fn as_scalar(&self) -> Option<&Arc<ScalarType>> {
        match self {
            Self::Scalar(scalar) => Some(scalar),
            _ => None,
        }
    }

    /// Returns the object type this is, if it is one.
    #[must_use]
    pub fn as_object(&self) -> Option<&Arc<ObjectType>> {
        match self {
            Self::Object(object) => Some(object),
            _ => None,
        }
    }

    /// Returns the interface type this is, if it is one.
    #[must_use]
    pub fn as_interface(&self) -> Option<&Arc<InterfaceType>> {
        match self {
            Self::Interface(interface) => Some(interface),
            _ => None,
        }
    }

    /// Returns the union type this is, if it is one.
    #[must_use]
    pub fn as_union(&self) -> Option<&Arc<UnionType>> {
        match self {
            Self::Union(union) => Some(union),
            _ => None,
        }
    }

    /// Returns the enum type this is, if it is one.
    #[must_use]
    pub fn as_enum(&self) -> Option<&Arc<EnumType>> {
        match self {
            Self::Enum(enum_type) => Some(enum_type),
            _ => None,
        }
    }

    /// Returns the input object type this is, if it is one.
    #[must_use]
    pub fn as_input_object(&self) -> Option<&Arc<InputObjectType>> {
        match self {
            Self::InputObject(input_object) => Some(input_object),
            _ => None,
        }
    }

    /// Returns the list type this is, if it is one.
    #[must_use]
    pub fn as_list(&self) -> Option<&ListType> {
        match self {
            Self::List(list) => Some(list),
            _ => None,
        }
    }

    /// Returns the non null type this is, if it is one.
    #[must_use]
    pub fn as_non_null(&self) -> Option<&NonNullType> {
        match self {
            Self::NonNull(non_null) => Some(non_null),
            _ => None,
        }
    }

    /// Returns the named type this is, if it is not a wrapping type.
    #[must_use]
    pub fn as_named(&self) -> Option<NamedType> {
        match self {
            Self::Scalar(scalar) => Some(NamedType::Scalar(Arc::clone(scalar))),
            Self::Object(object) => Some(NamedType::Object(Arc::clone(object))),
            Self::Interface(interface) => Some(NamedType::Interface(Arc::clone(interface))),
            Self::Union(union) => Some(NamedType::Union(Arc::clone(union))),
            Self::Enum(enum_type) => Some(NamedType::Enum(Arc::clone(enum_type))),
            Self::InputObject(input_object) => {
                Some(NamedType::InputObject(Arc::clone(input_object)))
            }
            Self::List(_) | Self::NonNull(_) => None,
        }
    }

    /// Returns the type a wrapping type wraps, if this is one.
    #[must_use]
    pub fn of_type(&self) -> Option<&Type> {
        match self {
            Self::List(list) => Some(&list.of_type),
            Self::NonNull(non_null) => Some(&non_null.of_type),
            _ => None,
        }
    }
}

impl PartialEq for Type {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::List(a), Self::List(b)) => a.of_type == b.of_type,
            (Self::NonNull(a), Self::NonNull(b)) => a.of_type == b.of_type,
            _ => match (self.as_named(), other.as_named()) {
                (Some(a), Some(b)) => a == b,
                _ => false,
            },
        }
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Scalar(scalar) => f.write_str(&scalar.name),
            Self::Object(object) => f.write_str(&object.name),
            Self::Interface(interface) => f.write_str(&interface.name),
            Self::Union(union) => f.write_str(&union.name),
            Self::Enum(enum_type) => f.write_str(&enum_type.name),
            Self::InputObject(input_object) => f.write_str(&input_object.name),
            Self::List(list) => fmt::Display::fmt(list, f),
            Self::NonNull(non_null) => fmt::Display::fmt(non_null, f),
        }
    }
}

impl fmt::Debug for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl From<NamedType> for Type {
    fn from(named: NamedType) -> Self {
        match named {
            NamedType::Scalar(scalar) => Self::Scalar(scalar),
            NamedType::Object(object) => Self::Object(object),
            NamedType::Interface(interface) => Self::Interface(interface),
            NamedType::Union(union) => Self::Union(union),
            NamedType::Enum(enum_type) => Self::Enum(enum_type),
            NamedType::InputObject(input_object) => Self::InputObject(input_object),
        }
    }
}

impl From<&NamedType> for Type {
    fn from(named: &NamedType) -> Self {
        Self::from(named.clone())
    }
}

impl From<Arc<ScalarType>> for Type {
    fn from(scalar: Arc<ScalarType>) -> Self {
        Self::Scalar(scalar)
    }
}

impl From<Arc<ObjectType>> for Type {
    fn from(object: Arc<ObjectType>) -> Self {
        Self::Object(object)
    }
}

impl From<Arc<InterfaceType>> for Type {
    fn from(interface: Arc<InterfaceType>) -> Self {
        Self::Interface(interface)
    }
}

impl From<Arc<UnionType>> for Type {
    fn from(union: Arc<UnionType>) -> Self {
        Self::Union(union)
    }
}

impl From<Arc<EnumType>> for Type {
    fn from(enum_type: Arc<EnumType>) -> Self {
        Self::Enum(enum_type)
    }
}

impl From<Arc<InputObjectType>> for Type {
    fn from(input_object: Arc<InputObjectType>) -> Self {
        Self::InputObject(input_object)
    }
}

impl From<ListType> for Type {
    fn from(list: ListType) -> Self {
        Self::List(SharedType::new(list))
    }
}

impl From<NonNullType> for Type {
    fn from(non_null: NonNullType) -> Self {
        Self::NonNull(SharedType::new(non_null))
    }
}

/// Returns the text `inspect` prints for a type.
///
/// A type has a `toJSON` that returns its `toString`, and `inspect` prints the
/// string a `toJSON` returns as it is, so the text is the `Display` of the
/// type.
fn inspect_type(ty: &Type) -> String {
    ty.to_string()
}

/// Returns whether the type is a scalar type.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
#[must_use]
pub fn is_scalar_type(ty: &Type) -> bool {
    ty.as_scalar().is_some()
}

/// Returns the type as a scalar type.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
///
/// # Errors
///
/// Returns the reference's plain `Error` when the type is not a scalar type.
pub fn assert_scalar_type(ty: &Type) -> Result<&Arc<ScalarType>, DefinitionError> {
    ty.as_scalar().ok_or_else(|| {
        DefinitionError::plain(format!(
            "Expected {} to be a GraphQL Scalar type.",
            inspect_type(ty)
        ))
    })
}

/// Returns whether the type is an object type.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
#[must_use]
pub fn is_object_type(ty: &Type) -> bool {
    ty.as_object().is_some()
}

/// Returns the type as an object type.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
///
/// # Errors
///
/// Returns the reference's plain `Error` when the type is not an object type.
pub fn assert_object_type(ty: &Type) -> Result<&Arc<ObjectType>, DefinitionError> {
    ty.as_object().ok_or_else(|| {
        DefinitionError::plain(format!(
            "Expected {} to be a GraphQL Object type.",
            inspect_type(ty)
        ))
    })
}

/// Returns whether the type is an interface type.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
#[must_use]
pub fn is_interface_type(ty: &Type) -> bool {
    ty.as_interface().is_some()
}

/// Returns the type as an interface type.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
///
/// # Errors
///
/// Returns the reference's plain `Error` when the type is not an interface
/// type.
pub fn assert_interface_type(ty: &Type) -> Result<&Arc<InterfaceType>, DefinitionError> {
    ty.as_interface().ok_or_else(|| {
        DefinitionError::plain(format!(
            "Expected {} to be a GraphQL Interface type.",
            inspect_type(ty)
        ))
    })
}

/// Returns whether the type is a union type.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
#[must_use]
pub fn is_union_type(ty: &Type) -> bool {
    ty.as_union().is_some()
}

/// Returns the type as a union type.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
///
/// # Errors
///
/// Returns the reference's plain `Error` when the type is not a union type.
pub fn assert_union_type(ty: &Type) -> Result<&Arc<UnionType>, DefinitionError> {
    ty.as_union().ok_or_else(|| {
        DefinitionError::plain(format!(
            "Expected {} to be a GraphQL Union type.",
            inspect_type(ty)
        ))
    })
}

/// Returns whether the type is an enum type.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
#[must_use]
pub fn is_enum_type(ty: &Type) -> bool {
    ty.as_enum().is_some()
}

/// Returns the type as an enum type.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
///
/// # Errors
///
/// Returns the reference's plain `Error` when the type is not an enum type.
pub fn assert_enum_type(ty: &Type) -> Result<&Arc<EnumType>, DefinitionError> {
    ty.as_enum().ok_or_else(|| {
        DefinitionError::plain(format!(
            "Expected {} to be a GraphQL Enum type.",
            inspect_type(ty)
        ))
    })
}

/// Returns whether the type is an input object type.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
#[must_use]
pub fn is_input_object_type(ty: &Type) -> bool {
    ty.as_input_object().is_some()
}

/// Returns the type as an input object type.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
///
/// # Errors
///
/// Returns the reference's plain `Error` when the type is not an input
/// object type.
pub fn assert_input_object_type(ty: &Type) -> Result<&Arc<InputObjectType>, DefinitionError> {
    ty.as_input_object().ok_or_else(|| {
        DefinitionError::plain(format!(
            "Expected {} to be a GraphQL Input Object type.",
            inspect_type(ty)
        ))
    })
}

/// Returns whether the type is a list type.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
#[must_use]
pub fn is_list_type(ty: &Type) -> bool {
    ty.as_list().is_some()
}

/// Returns the type as a list type.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
///
/// # Errors
///
/// Returns the reference's plain `Error` when the type is not a list type.
pub fn assert_list_type(ty: &Type) -> Result<&ListType, DefinitionError> {
    ty.as_list().ok_or_else(|| {
        DefinitionError::plain(format!(
            "Expected {} to be a GraphQL List type.",
            inspect_type(ty)
        ))
    })
}

/// Returns whether the type is a non null type.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
#[must_use]
pub fn is_non_null_type(ty: &Type) -> bool {
    ty.as_non_null().is_some()
}

/// Returns the type as a non null type.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
///
/// # Errors
///
/// Returns the reference's plain `Error` when the type is not a non null
/// type.
pub fn assert_non_null_type(ty: &Type) -> Result<&NonNullType, DefinitionError> {
    ty.as_non_null().ok_or_else(|| {
        DefinitionError::plain(format!(
            "Expected {} to be a GraphQL Non-Null type.",
            inspect_type(ty)
        ))
    })
}

/// Returns whether the type may be used as an input type for arguments and
/// directives.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
#[must_use]
pub fn is_input_type(ty: &Type) -> bool {
    is_scalar_type(ty)
        || is_enum_type(ty)
        || is_input_object_type(ty)
        || (is_wrapping_type(ty) && ty.of_type().is_some_and(is_input_type))
}

/// Returns the type as an input type.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
///
/// # Errors
///
/// Returns the reference's plain `Error` when the type is not an input type.
pub fn assert_input_type(ty: &Type) -> Result<&Type, DefinitionError> {
    if !is_input_type(ty) {
        return Err(DefinitionError::plain(format!(
            "Expected {} to be a GraphQL input type.",
            inspect_type(ty)
        )));
    }
    Ok(ty)
}

/// Returns whether the type may be used as an output type as the result of
/// fields.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
#[must_use]
pub fn is_output_type(ty: &Type) -> bool {
    is_scalar_type(ty)
        || is_object_type(ty)
        || is_interface_type(ty)
        || is_union_type(ty)
        || is_enum_type(ty)
        || (is_wrapping_type(ty) && ty.of_type().is_some_and(is_output_type))
}

/// Returns the type as an output type.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
///
/// # Errors
///
/// Returns the reference's plain `Error` when the type is not an output
/// type.
pub fn assert_output_type(ty: &Type) -> Result<&Type, DefinitionError> {
    if !is_output_type(ty) {
        return Err(DefinitionError::plain(format!(
            "Expected {} to be a GraphQL output type.",
            inspect_type(ty)
        )));
    }
    Ok(ty)
}

/// Returns whether the type may describe leaf values.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
#[must_use]
pub fn is_leaf_type(ty: &Type) -> bool {
    is_scalar_type(ty) || is_enum_type(ty)
}

/// Returns the type as a leaf type.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
///
/// # Errors
///
/// Returns the reference's plain `Error` when the type is not a leaf type.
pub fn assert_leaf_type(ty: &Type) -> Result<&Type, DefinitionError> {
    if !is_leaf_type(ty) {
        return Err(DefinitionError::plain(format!(
            "Expected {} to be a GraphQL leaf type.",
            inspect_type(ty)
        )));
    }
    Ok(ty)
}

/// Returns whether the type may describe the parent context of a selection
/// set.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
#[must_use]
pub fn is_composite_type(ty: &Type) -> bool {
    is_object_type(ty) || is_interface_type(ty) || is_union_type(ty)
}

/// Returns the type as a composite type.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
///
/// # Errors
///
/// Returns the reference's plain `Error` when the type is not a composite
/// type.
pub fn assert_composite_type(ty: &Type) -> Result<&Type, DefinitionError> {
    if !is_composite_type(ty) {
        return Err(DefinitionError::plain(format!(
            "Expected {} to be a GraphQL composite type.",
            inspect_type(ty)
        )));
    }
    Ok(ty)
}

/// Returns whether the type is an interface or a union type.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
#[must_use]
pub fn is_abstract_type(ty: &Type) -> bool {
    is_interface_type(ty) || is_union_type(ty)
}

/// Returns the type as an abstract type.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
///
/// # Errors
///
/// Returns the reference's plain `Error` when the type is not an abstract
/// type.
pub fn assert_abstract_type(ty: &Type) -> Result<&Type, DefinitionError> {
    if !is_abstract_type(ty) {
        return Err(DefinitionError::plain(format!(
            "Expected {} to be a GraphQL abstract type.",
            inspect_type(ty)
        )));
    }
    Ok(ty)
}

/// List Type Wrapper.
///
/// A list is a wrapping type which points to another type. Lists are often
/// created within the context of defining the fields of an object type. The
/// `devAssert` of the constructor, that the wrapped value is a type, has no
/// reading because the argument type carries it.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
#[derive(Clone, PartialEq)]
pub struct ListType {
    /// The type of every item.
    pub of_type: Type,
}

impl ListType {
    /// Wraps `of_type` in a list.
    #[must_use]
    pub fn new(of_type: Type) -> Self {
        Self { of_type }
    }
}

impl fmt::Display for ListType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}]", self.of_type)
    }
}

impl fmt::Debug for ListType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

/// Non-Null Type Wrapper.
///
/// A non-null is a wrapping type which points to another type. Non-null types
/// enforce that their values are never null and can ensure an error is raised
/// if this ever occurs during a request. The wrapped type is private so that
/// it is nullable by construction, which is what the constructor asserts.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
#[derive(Clone, PartialEq)]
pub struct NonNullType {
    of_type: Type,
}

impl NonNullType {
    /// Wraps `of_type` in a non null type.
    ///
    /// # Errors
    ///
    /// Returns the reference's plain `Error` when `of_type` is itself a non
    /// null type.
    pub fn new(of_type: Type) -> Result<Self, DefinitionError> {
        if !is_nullable_type(&of_type) {
            return Err(DefinitionError::plain(format!(
                "Expected {} to be a GraphQL nullable type.",
                inspect_type(&of_type)
            )));
        }
        Ok(Self { of_type })
    }

    /// Returns the wrapped type.
    #[must_use]
    pub fn of_type(&self) -> &Type {
        &self.of_type
    }
}

impl From<NamedType> for NonNullType {
    // A named type is nullable, so the assertion of the constructor holds.
    fn from(named: NamedType) -> Self {
        Self {
            of_type: Type::from(named),
        }
    }
}

impl fmt::Display for NonNullType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}!", self.of_type)
    }
}

impl fmt::Debug for NonNullType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

/// Returns whether the type wraps another type.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
#[must_use]
pub fn is_wrapping_type(ty: &Type) -> bool {
    is_list_type(ty) || is_non_null_type(ty)
}

/// Returns the type as a wrapping type.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
///
/// # Errors
///
/// Returns the reference's plain `Error` when the type wraps nothing.
pub fn assert_wrapping_type(ty: &Type) -> Result<&Type, DefinitionError> {
    if !is_wrapping_type(ty) {
        return Err(DefinitionError::plain(format!(
            "Expected {} to be a GraphQL wrapping type.",
            inspect_type(ty)
        )));
    }
    Ok(ty)
}

/// Returns whether the type can accept null as a value.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
#[must_use]
pub fn is_nullable_type(ty: &Type) -> bool {
    !is_non_null_type(ty)
}

/// Returns the type as a nullable type.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
///
/// # Errors
///
/// Returns the reference's plain `Error` when the type is a non null type.
pub fn assert_nullable_type(ty: &Type) -> Result<&Type, DefinitionError> {
    if !is_nullable_type(ty) {
        return Err(DefinitionError::plain(format!(
            "Expected {} to be a GraphQL nullable type.",
            inspect_type(ty)
        )));
    }
    Ok(ty)
}

/// Returns the type without a non null wrapper.
///
/// The overload over an absent type has no reading, because a caller with an
/// `Option` maps over it.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
#[must_use]
pub fn get_nullable_type(ty: &Type) -> &Type {
    match ty {
        Type::NonNull(non_null) => &non_null.of_type,
        _ => ty,
    }
}

/// A named type, which carries no wrapper.
///
/// Two named types are equal when they are the same allocation, which is the
/// `===` of the reference, and the hash follows the same identity, so a set of
/// named types is a set of identities as the `Set` of `collectReferencedTypes`
/// is.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
#[derive(Clone)]
pub enum NamedType {
    /// A scalar type.
    Scalar(Arc<ScalarType>),
    /// An object type.
    Object(Arc<ObjectType>),
    /// An interface type.
    Interface(Arc<InterfaceType>),
    /// A union type.
    Union(Arc<UnionType>),
    /// An enum type.
    Enum(Arc<EnumType>),
    /// An input object type.
    InputObject(Arc<InputObjectType>),
}

impl NamedType {
    /// Returns the name of the type.
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::Scalar(scalar) => &scalar.name,
            Self::Object(object) => &object.name,
            Self::Interface(interface) => &interface.name,
            Self::Union(union) => &union.name,
            Self::Enum(enum_type) => &enum_type.name,
            Self::InputObject(input_object) => &input_object.name,
        }
    }

    /// Returns the description of the type.
    #[must_use]
    pub fn description(&self) -> Option<&str> {
        match self {
            Self::Scalar(scalar) => scalar.description.as_deref(),
            Self::Object(object) => object.description.as_deref(),
            Self::Interface(interface) => interface.description.as_deref(),
            Self::Union(union) => union.description.as_deref(),
            Self::Enum(enum_type) => enum_type.description.as_deref(),
            Self::InputObject(input_object) => input_object.description.as_deref(),
        }
    }

    /// Returns the scalar type this is, if it is one.
    #[must_use]
    pub fn as_scalar(&self) -> Option<&Arc<ScalarType>> {
        match self {
            Self::Scalar(scalar) => Some(scalar),
            _ => None,
        }
    }

    /// Returns the object type this is, if it is one.
    #[must_use]
    pub fn as_object(&self) -> Option<&Arc<ObjectType>> {
        match self {
            Self::Object(object) => Some(object),
            _ => None,
        }
    }

    /// Returns the interface type this is, if it is one.
    #[must_use]
    pub fn as_interface(&self) -> Option<&Arc<InterfaceType>> {
        match self {
            Self::Interface(interface) => Some(interface),
            _ => None,
        }
    }

    /// Returns the union type this is, if it is one.
    #[must_use]
    pub fn as_union(&self) -> Option<&Arc<UnionType>> {
        match self {
            Self::Union(union) => Some(union),
            _ => None,
        }
    }

    /// Returns the enum type this is, if it is one.
    #[must_use]
    pub fn as_enum(&self) -> Option<&Arc<EnumType>> {
        match self {
            Self::Enum(enum_type) => Some(enum_type),
            _ => None,
        }
    }

    /// Returns the input object type this is, if it is one.
    #[must_use]
    pub fn as_input_object(&self) -> Option<&Arc<InputObjectType>> {
        match self {
            Self::InputObject(input_object) => Some(input_object),
            _ => None,
        }
    }

    /// Returns the address of the allocation, the identity of the type.
    fn identity(&self) -> *const () {
        match self {
            Self::Scalar(scalar) => Arc::as_ptr(scalar).cast(),
            Self::Object(object) => Arc::as_ptr(object).cast(),
            Self::Interface(interface) => Arc::as_ptr(interface).cast(),
            Self::Union(union) => Arc::as_ptr(union).cast(),
            Self::Enum(enum_type) => Arc::as_ptr(enum_type).cast(),
            Self::InputObject(input_object) => Arc::as_ptr(input_object).cast(),
        }
    }
}

impl PartialEq for NamedType {
    fn eq(&self, other: &Self) -> bool {
        core::ptr::eq(self.identity(), other.identity())
    }
}

impl Eq for NamedType {}

impl Hash for NamedType {
    fn hash<H: Hasher>(&self, state: &mut H) {
        core::ptr::hash(self.identity(), state);
    }
}

impl fmt::Display for NamedType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl fmt::Debug for NamedType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl From<Arc<ScalarType>> for NamedType {
    fn from(scalar: Arc<ScalarType>) -> Self {
        Self::Scalar(scalar)
    }
}

impl From<Arc<ObjectType>> for NamedType {
    fn from(object: Arc<ObjectType>) -> Self {
        Self::Object(object)
    }
}

impl From<Arc<InterfaceType>> for NamedType {
    fn from(interface: Arc<InterfaceType>) -> Self {
        Self::Interface(interface)
    }
}

impl From<Arc<UnionType>> for NamedType {
    fn from(union: Arc<UnionType>) -> Self {
        Self::Union(union)
    }
}

impl From<Arc<EnumType>> for NamedType {
    fn from(enum_type: Arc<EnumType>) -> Self {
        Self::Enum(enum_type)
    }
}

impl From<Arc<InputObjectType>> for NamedType {
    fn from(input_object: Arc<InputObjectType>) -> Self {
        Self::InputObject(input_object)
    }
}

/// Returns whether the type carries no wrapper.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
#[must_use]
pub fn is_named_type(ty: &Type) -> bool {
    ty.as_named().is_some()
}

/// Returns the type as a named type.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
///
/// # Errors
///
/// Returns the reference's plain `Error` when the type is a wrapping type.
pub fn assert_named_type(ty: &Type) -> Result<NamedType, DefinitionError> {
    ty.as_named().ok_or_else(|| {
        DefinitionError::plain(format!(
            "Expected {} to be a GraphQL named type.",
            inspect_type(ty)
        ))
    })
}

/// Returns the named type under every wrapper.
///
/// The `while` of the reference is the recursion over the two wrapping
/// variants, so that the match is total and no branch stands for an impossible
/// state. The overload over an absent type has no reading, because a caller
/// with an `Option` maps over it.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
#[must_use]
pub fn get_named_type(ty: &Type) -> NamedType {
    match ty {
        Type::List(list) => get_named_type(&list.of_type),
        Type::NonNull(non_null) => get_named_type(&non_null.of_type),
        Type::Scalar(scalar) => NamedType::Scalar(Arc::clone(scalar)),
        Type::Object(object) => NamedType::Object(Arc::clone(object)),
        Type::Interface(interface) => NamedType::Interface(Arc::clone(interface)),
        Type::Union(union) => NamedType::Union(Arc::clone(union)),
        Type::Enum(enum_type) => NamedType::Enum(Arc::clone(enum_type)),
        Type::InputObject(input_object) => NamedType::InputObject(Arc::clone(input_object)),
    }
}

/// The configuration of a scalar type.
///
/// A field that is optional in the reference and defaults to an empty
/// collection is the collection here, which reads the same through `??`. This
/// struct is also what `toConfig` returns, so the normalized config interface
/// of the reference has no separate item.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
#[derive(Clone, Debug, Default)]
pub struct ScalarTypeConfig {
    /// The name.
    pub name: String,
    /// The description.
    pub description: Option<String>,
    /// The URL of the specification of a custom scalar.
    pub specified_by_url: Option<String>,
    /// Serializes an internal value to include in a response.
    pub serialize: Option<ScalarSerializer>,
    /// Parses an externally provided value to use as an input.
    pub parse_value: Option<ScalarValueParser>,
    /// Parses an externally provided literal value to use as an input.
    pub parse_literal: Option<ScalarLiteralParser>,
    /// The extensions.
    pub extensions: JsObject,
    /// The definition node the type was built from.
    pub ast_node: Option<ScalarTypeDefinitionNode>,
    /// The extension nodes the type was built from.
    pub extension_ast_nodes: Vec<ScalarTypeExtensionNode>,
}

/// Scalar Type Definition.
///
/// The leaf values of any request and input values to arguments are Scalars
/// (or Enums) and are defined with a name and a series of functions used to
/// parse input from ast or variables and to ensure validity.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
#[derive(Clone)]
pub struct ScalarType {
    /// The name.
    pub name: String,
    /// The description.
    pub description: Option<String>,
    /// The URL of the specification of a custom scalar.
    pub specified_by_url: Option<String>,
    /// Serializes an internal value to include in a response.
    pub serialize: ScalarSerializer,
    /// Parses an externally provided value to use as an input.
    pub parse_value: ScalarValueParser,
    /// Parses an externally provided literal value to use as an input.
    pub parse_literal: ScalarLiteralParser,
    /// The extensions.
    pub extensions: JsObject,
    /// The definition node the type was built from.
    pub ast_node: Option<ScalarTypeDefinitionNode>,
    /// The extension nodes the type was built from.
    pub extension_ast_nodes: Vec<ScalarTypeExtensionNode>,
}

impl ScalarType {
    /// Builds a scalar type from `config`.
    ///
    /// The `serialize` and `parseValue` defaults are the identity, and the
    /// `parseLiteral` default parses the untyped value of the literal with
    /// `parseValue`. The two `devAssert` calls over the types of
    /// `specifiedByURL` and `serialize` have no reading, because the config
    /// carries the types.
    ///
    /// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the error of [`assert_name()`], or the reference's plain
    /// `Error` when `parse_literal` is given without `parse_value`.
    pub fn new(config: ScalarTypeConfig) -> Result<Self, DefinitionError> {
        let has_parse_value = config.parse_value.is_some();
        let parse_value = config
            .parse_value
            .unwrap_or_else(|| ScalarValueParser::keeping(|_| Ok(Parsed::Input)));

        let name = assert_name(&config.name)?;
        let serialize = config
            .serialize
            .unwrap_or_else(|| ScalarSerializer::new(|value| Ok(value.clone())));
        let parse_literal = config.parse_literal.clone().unwrap_or_else(|| {
            let parse_value = parse_value.clone();
            ScalarLiteralParser::new(move |node, variables| {
                let value = value_from_ast_untyped(node, variables);
                match parse_value.parse(&value)? {
                    Parsed::Input => Ok(value),
                    Parsed::Value(parsed) => Ok(parsed),
                }
            })
        });

        if config.parse_literal.is_some() && !has_parse_value {
            return Err(DefinitionError::plain(format!(
                "{name} must provide both \"parseValue\" and \"parseLiteral\" functions."
            )));
        }

        Ok(Self {
            name,
            description: config.description,
            specified_by_url: config.specified_by_url,
            serialize,
            parse_value,
            parse_literal,
            extensions: config.extensions,
            ast_node: config.ast_node,
            extension_ast_nodes: config.extension_ast_nodes,
        })
    }

    /// Serializes `output_value` for a response.
    ///
    /// # Errors
    ///
    /// Returns what the serializer throws.
    pub fn serialize(&self, output_value: &JsValue) -> Result<JsValue, DefinitionError> {
        self.serialize.call(output_value)
    }

    /// Parses `input_value`, an external value, into an internal value.
    ///
    /// # Errors
    ///
    /// Returns what the value parser throws.
    pub fn parse_value(&self, input_value: &JsValue) -> Result<JsValue, DefinitionError> {
        self.parse_value.call(input_value)
    }

    /// Parses `input_value`, an external value, and tells whether its
    /// internal value is the input itself.
    ///
    /// The coercion keeps an input whose parser answers [`Parsed::Input`]
    /// where it stands, which [`ScalarValueParser::parse`] states.
    ///
    /// # Errors
    ///
    /// Returns what the value parser throws.
    pub fn parse_input(&self, input_value: &JsValue) -> Result<Parsed, DefinitionError> {
        self.parse_value.parse(input_value)
    }

    /// Parses `value_node`, a literal, into an internal value.
    ///
    /// # Errors
    ///
    /// Returns what the literal parser throws.
    pub fn parse_literal(
        &self,
        value_node: &ValueNode,
        variables: Option<&JsObject>,
    ) -> Result<JsValue, DefinitionError> {
        self.parse_literal.call(value_node, variables)
    }

    /// Returns the config the type was built from, normalized.
    ///
    /// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
    #[must_use]
    pub fn to_config(&self) -> ScalarTypeConfig {
        ScalarTypeConfig {
            name: self.name.clone(),
            description: self.description.clone(),
            specified_by_url: self.specified_by_url.clone(),
            serialize: Some(self.serialize.clone()),
            parse_value: Some(self.parse_value.clone()),
            parse_literal: Some(self.parse_literal.clone()),
            extensions: self.extensions.clone(),
            ast_node: self.ast_node.clone(),
            extension_ast_nodes: self.extension_ast_nodes.clone(),
        }
    }
}

impl fmt::Display for ScalarType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name)
    }
}

impl fmt::Debug for ScalarType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ScalarType")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

/// The configuration of an argument.
///
/// An absent default value is [`JsValue::Undefined`], which is what
/// `defaultValue === undefined` tests.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
#[derive(Clone, Debug)]
pub struct ArgumentConfig {
    /// The description.
    pub description: Option<String>,
    /// The type.
    pub r#type: Type,
    /// The default value, undefined when there is none.
    pub default_value: JsValue,
    /// The deprecation reason.
    pub deprecation_reason: Option<String>,
    /// The extensions.
    pub extensions: JsObject,
    /// The definition node the argument was built from.
    pub ast_node: Option<InputValueDefinitionNode>,
}

impl ArgumentConfig {
    /// Returns a config of `r#type` with every other field empty.
    #[must_use]
    pub fn of_type(r#type: Type) -> Self {
        Self {
            description: None,
            r#type,
            default_value: JsValue::Undefined,
            deprecation_reason: None,
            extensions: JsObject::new(),
            ast_node: None,
        }
    }
}

/// An argument of a field or a directive.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
#[derive(Clone, Debug)]
pub struct Argument {
    /// The name.
    pub name: String,
    /// The description.
    pub description: Option<String>,
    /// The type.
    pub r#type: Type,
    /// The default value, undefined when there is none.
    pub default_value: JsValue,
    /// The deprecation reason.
    pub deprecation_reason: Option<String>,
    /// The extensions.
    pub extensions: JsObject,
    /// The definition node the argument was built from.
    pub ast_node: Option<InputValueDefinitionNode>,
}

/// Returns whether the argument has to be given.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
#[must_use]
pub fn is_required_argument(arg: &Argument) -> bool {
    is_non_null_type(&arg.r#type) && arg.default_value.is_undefined()
}

/// The configuration of a field of an object or an interface.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
#[derive(Clone, Debug)]
pub struct FieldConfig {
    /// The description.
    pub description: Option<String>,
    /// The type.
    pub r#type: Type,
    /// The arguments, keyed by name.
    pub args: ObjMap<ArgumentConfig>,
    /// The resolver.
    pub resolve: Option<FieldResolver>,
    /// The subscription resolver.
    pub subscribe: Option<FieldResolver>,
    /// The deprecation reason.
    pub deprecation_reason: Option<String>,
    /// The extensions.
    pub extensions: JsObject,
    /// The definition node the field was built from.
    pub ast_node: Option<FieldDefinitionNode>,
}

impl FieldConfig {
    /// Returns a config of `r#type` with every other field empty.
    #[must_use]
    pub fn of_type(r#type: Type) -> Self {
        Self {
            description: None,
            r#type,
            args: ObjMap::default(),
            resolve: None,
            subscribe: None,
            deprecation_reason: None,
            extensions: JsObject::new(),
            ast_node: None,
        }
    }
}

/// A field of an object or an interface.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
#[derive(Clone, Debug)]
pub struct Field {
    /// The name.
    pub name: String,
    /// The description.
    pub description: Option<String>,
    /// The type.
    pub r#type: Type,
    /// The arguments, in definition order.
    pub args: Vec<Argument>,
    /// The resolver.
    pub resolve: Option<FieldResolver>,
    /// The subscription resolver.
    pub subscribe: Option<FieldResolver>,
    /// The deprecation reason.
    pub deprecation_reason: Option<String>,
    /// The extensions.
    pub extensions: JsObject,
    /// The definition node the field was built from.
    pub ast_node: Option<FieldDefinitionNode>,
}

/// Builds the interface list of an object or an interface type.
///
/// The `devAssert` that the result is an array has no reading, because the
/// thunk returns one.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
///
/// # Errors
///
/// Returns what the thunk throws.
fn define_interfaces(
    interfaces: &ThunkReadonlyArray<NamedType>,
) -> Result<Vec<NamedType>, DefinitionError> {
    Ok(resolve_readonly_array_thunk(interfaces)?.into_owned())
}

/// Builds the field map of an object or an interface type.
///
/// The three `devAssert` calls over the shapes of the map, the field config
/// and the args have no reading, because the config carries the shapes, and
/// the type name the reference reads for their messages has no reader here.
/// The walk follows `Object.entries` order.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
///
/// # Errors
///
/// Returns what the thunk throws, or the error of [`assert_name()`] for a
/// field or argument name.
fn define_field_map(fields: &ThunkObjMap<FieldConfig>) -> Result<ObjMap<Field>, DefinitionError> {
    let field_map = resolve_obj_map_thunk(fields)?;

    let mut result = ObjMap::default();
    for (field_name, field_config) in own_entries(&field_map) {
        let field = Field {
            name: assert_name(field_name)?,
            description: field_config.description.clone(),
            r#type: field_config.r#type.clone(),
            args: define_arguments(&field_config.args)?,
            resolve: field_config.resolve.clone(),
            subscribe: field_config.subscribe.clone(),
            deprecation_reason: field_config.deprecation_reason.clone(),
            extensions: field_config.extensions.clone(),
            ast_node: field_config.ast_node.clone(),
        };
        result.insert(field_name.clone(), field);
    }
    Ok(result)
}

/// Builds the argument list of a field or a directive from its config map.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
///
/// # Errors
///
/// Returns the error of [`assert_name()`] for an argument name.
pub fn define_arguments(config: &ObjMap<ArgumentConfig>) -> Result<Vec<Argument>, DefinitionError> {
    own_entries(config)
        .into_iter()
        .map(|(arg_name, arg_config)| {
            Ok(Argument {
                name: assert_name(arg_name)?,
                description: arg_config.description.clone(),
                r#type: arg_config.r#type.clone(),
                default_value: arg_config.default_value.clone(),
                deprecation_reason: arg_config.deprecation_reason.clone(),
                extensions: arg_config.extensions.clone(),
                ast_node: arg_config.ast_node.clone(),
            })
        })
        .collect()
}

/// Returns the config map of a field map.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
fn fields_to_fields_config(fields: &ObjMap<Field>) -> ObjMap<FieldConfig> {
    fields
        .iter()
        .map(|(name, field)| {
            let config = FieldConfig {
                description: field.description.clone(),
                r#type: field.r#type.clone(),
                args: args_to_args_config(&field.args),
                resolve: field.resolve.clone(),
                subscribe: field.subscribe.clone(),
                deprecation_reason: field.deprecation_reason.clone(),
                extensions: field.extensions.clone(),
                ast_node: field.ast_node.clone(),
            };
            (name.clone(), config)
        })
        .collect()
}

/// Returns the config map of an argument list.
///
/// `keyValMap` writes each key once, and a repeated name keeps the position of
/// its first appearance with the value of its last, which is what an insert
/// into an [`IndexMap`] does.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
#[must_use]
pub fn args_to_args_config(args: &[Argument]) -> ObjMap<ArgumentConfig> {
    args.iter()
        .map(|arg| {
            let config = ArgumentConfig {
                description: arg.description.clone(),
                r#type: arg.r#type.clone(),
                default_value: arg.default_value.clone(),
                deprecation_reason: arg.deprecation_reason.clone(),
                extensions: arg.extensions.clone(),
                ast_node: arg.ast_node.clone(),
            };
            (arg.name.clone(), config)
        })
        .collect()
}

/// The configuration of an object type.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
#[derive(Debug, Default)]
pub struct ObjectTypeConfig {
    /// The name.
    pub name: String,
    /// The description.
    pub description: Option<String>,
    /// The interfaces, or a thunk that returns them.
    ///
    /// The reference types the list as interface types and stores whatever
    /// named type the config holds, and `validateSchema` reports an entry that
    /// is not an interface type, so the list holds any [`NamedType`].
    pub interfaces: ThunkReadonlyArray<NamedType>,
    /// The fields, or a thunk that returns them.
    pub fields: ThunkObjMap<FieldConfig>,
    /// The `isTypeOf` function of the executor.
    pub is_type_of: Option<IsTypeOfFn>,
    /// The extensions.
    pub extensions: JsObject,
    /// The definition node the type was built from.
    pub ast_node: Option<ObjectTypeDefinitionNode>,
    /// The extension nodes the type was built from.
    pub extension_ast_nodes: Vec<ObjectTypeExtensionNode>,
}

/// Object Type Definition.
///
/// Almost all of the GraphQL types you define will be object types. Object
/// types have a name, but most importantly describe their fields.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
pub struct ObjectType {
    /// The name.
    pub name: String,
    /// The description.
    pub description: Option<String>,
    /// The `isTypeOf` function of the executor.
    pub is_type_of: Option<IsTypeOfFn>,
    /// The extensions.
    pub extensions: JsObject,
    /// The definition node the type was built from.
    pub ast_node: Option<ObjectTypeDefinitionNode>,
    /// The extension nodes the type was built from.
    pub extension_ast_nodes: Vec<ObjectTypeExtensionNode>,
    fields_thunk: ThunkObjMap<FieldConfig>,
    interfaces_thunk: ThunkReadonlyArray<NamedType>,
    fields: OnceBox<ObjMap<Field>>,
    interfaces: OnceBox<Vec<NamedType>>,
}

impl ObjectType {
    /// Builds an object type from `config`.
    ///
    /// The `devAssert` over the type of `isTypeOf` has no reading, because the
    /// config carries the type.
    ///
    /// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the error of [`assert_name()`].
    pub fn new(config: ObjectTypeConfig) -> Result<Self, DefinitionError> {
        Ok(Self {
            name: assert_name(&config.name)?,
            description: config.description,
            is_type_of: config.is_type_of,
            extensions: config.extensions,
            ast_node: config.ast_node,
            extension_ast_nodes: config.extension_ast_nodes,
            fields_thunk: config.fields,
            interfaces_thunk: config.interfaces,
            fields: OnceBox::new(),
            interfaces: OnceBox::new(),
        })
    }

    /// Returns the fields, resolving the thunk on first read.
    ///
    /// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns what the thunk or a name assertion throws.
    pub fn get_fields(&self) -> Result<&ObjMap<Field>, DefinitionError> {
        self.fields
            .get_or_try_init(|| define_field_map(&self.fields_thunk).map(Box::new))
    }

    /// Returns the interfaces, resolving the thunk on first read.
    ///
    /// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns what the thunk throws.
    pub fn get_interfaces(&self) -> Result<&[NamedType], DefinitionError> {
        self.interfaces
            .get_or_try_init(|| define_interfaces(&self.interfaces_thunk).map(Box::new))
            .map(Vec::as_slice)
    }

    /// Returns the config the type was built from, normalized.
    ///
    /// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns what the two thunks throw.
    pub fn to_config(&self) -> Result<ObjectTypeConfig, DefinitionError> {
        Ok(ObjectTypeConfig {
            name: self.name.clone(),
            description: self.description.clone(),
            interfaces: ThunkReadonlyArray::Array(self.get_interfaces()?.to_vec()),
            fields: ThunkObjMap::Map(fields_to_fields_config(self.get_fields()?)),
            is_type_of: self.is_type_of.clone(),
            extensions: self.extensions.clone(),
            ast_node: self.ast_node.clone(),
            extension_ast_nodes: self.extension_ast_nodes.clone(),
        })
    }
}

impl fmt::Display for ObjectType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name)
    }
}

impl fmt::Debug for ObjectType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ObjectType")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

/// The configuration of an interface type.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
#[derive(Debug, Default)]
pub struct InterfaceTypeConfig {
    /// The name.
    pub name: String,
    /// The description.
    pub description: Option<String>,
    /// The interfaces, or a thunk that returns them.
    ///
    /// The reference types the list as interface types and stores whatever
    /// named type the config holds, and `validateSchema` reports an entry that
    /// is not an interface type, so the list holds any [`NamedType`].
    pub interfaces: ThunkReadonlyArray<NamedType>,
    /// The fields, or a thunk that returns them.
    pub fields: ThunkObjMap<FieldConfig>,
    /// The `resolveType` function of the executor.
    pub resolve_type: Option<TypeResolver>,
    /// The extensions.
    pub extensions: JsObject,
    /// The definition node the type was built from.
    pub ast_node: Option<InterfaceTypeDefinitionNode>,
    /// The extension nodes the type was built from.
    pub extension_ast_nodes: Vec<InterfaceTypeExtensionNode>,
}

/// Interface Type Definition.
///
/// When a field can return one of a heterogeneous set of types, a Interface
/// type is used to describe what types are possible, what fields are in common
/// across all types, as well as a function to determine which type is actually
/// used when the field is resolved.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
pub struct InterfaceType {
    /// The name.
    pub name: String,
    /// The description.
    pub description: Option<String>,
    /// The `resolveType` function of the executor.
    pub resolve_type: Option<TypeResolver>,
    /// The extensions.
    pub extensions: JsObject,
    /// The definition node the type was built from.
    pub ast_node: Option<InterfaceTypeDefinitionNode>,
    /// The extension nodes the type was built from.
    pub extension_ast_nodes: Vec<InterfaceTypeExtensionNode>,
    fields_thunk: ThunkObjMap<FieldConfig>,
    interfaces_thunk: ThunkReadonlyArray<NamedType>,
    fields: OnceBox<ObjMap<Field>>,
    interfaces: OnceBox<Vec<NamedType>>,
}

impl InterfaceType {
    /// Builds an interface type from `config`.
    ///
    /// The `devAssert` over the type of `resolveType` has no reading, because
    /// the config carries the type.
    ///
    /// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the error of [`assert_name()`].
    pub fn new(config: InterfaceTypeConfig) -> Result<Self, DefinitionError> {
        Ok(Self {
            name: assert_name(&config.name)?,
            description: config.description,
            resolve_type: config.resolve_type,
            extensions: config.extensions,
            ast_node: config.ast_node,
            extension_ast_nodes: config.extension_ast_nodes,
            fields_thunk: config.fields,
            interfaces_thunk: config.interfaces,
            fields: OnceBox::new(),
            interfaces: OnceBox::new(),
        })
    }

    /// Returns the fields, resolving the thunk on first read.
    ///
    /// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns what the thunk or a name assertion throws.
    pub fn get_fields(&self) -> Result<&ObjMap<Field>, DefinitionError> {
        self.fields
            .get_or_try_init(|| define_field_map(&self.fields_thunk).map(Box::new))
    }

    /// Returns the interfaces, resolving the thunk on first read.
    ///
    /// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns what the thunk throws.
    pub fn get_interfaces(&self) -> Result<&[NamedType], DefinitionError> {
        self.interfaces
            .get_or_try_init(|| define_interfaces(&self.interfaces_thunk).map(Box::new))
            .map(Vec::as_slice)
    }

    /// Returns the config the type was built from, normalized.
    ///
    /// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns what the two thunks throw.
    pub fn to_config(&self) -> Result<InterfaceTypeConfig, DefinitionError> {
        Ok(InterfaceTypeConfig {
            name: self.name.clone(),
            description: self.description.clone(),
            interfaces: ThunkReadonlyArray::Array(self.get_interfaces()?.to_vec()),
            fields: ThunkObjMap::Map(fields_to_fields_config(self.get_fields()?)),
            resolve_type: self.resolve_type.clone(),
            extensions: self.extensions.clone(),
            ast_node: self.ast_node.clone(),
            extension_ast_nodes: self.extension_ast_nodes.clone(),
        })
    }
}

impl fmt::Display for InterfaceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name)
    }
}

impl fmt::Debug for InterfaceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InterfaceType")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

/// The configuration of a union type.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
#[derive(Debug, Default)]
pub struct UnionTypeConfig {
    /// The name.
    pub name: String,
    /// The description.
    pub description: Option<String>,
    /// The member types, or a thunk that returns them.
    ///
    /// The reference types the list as object types and stores whatever named
    /// type the config holds, and `validateSchema` reports a member that is
    /// not an object type, so the list holds any [`NamedType`].
    pub types: ThunkReadonlyArray<NamedType>,
    /// The `resolveType` function of the executor.
    pub resolve_type: Option<TypeResolver>,
    /// The extensions.
    pub extensions: JsObject,
    /// The definition node the type was built from.
    pub ast_node: Option<UnionTypeDefinitionNode>,
    /// The extension nodes the type was built from.
    pub extension_ast_nodes: Vec<UnionTypeExtensionNode>,
}

/// Union Type Definition.
///
/// When a field can return one of a heterogeneous set of types, a Union type
/// is used to describe what types are possible as well as providing a function
/// to determine which type is actually used when the field is resolved.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
pub struct UnionType {
    /// The name.
    pub name: String,
    /// The description.
    pub description: Option<String>,
    /// The `resolveType` function of the executor.
    pub resolve_type: Option<TypeResolver>,
    /// The extensions.
    pub extensions: JsObject,
    /// The definition node the type was built from.
    pub ast_node: Option<UnionTypeDefinitionNode>,
    /// The extension nodes the type was built from.
    pub extension_ast_nodes: Vec<UnionTypeExtensionNode>,
    types_thunk: ThunkReadonlyArray<NamedType>,
    types: OnceBox<Vec<NamedType>>,
}

impl UnionType {
    /// Builds a union type from `config`.
    ///
    /// The `devAssert` over the type of `resolveType` has no reading, because
    /// the config carries the type.
    ///
    /// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the error of [`assert_name()`].
    // The builders of `extendSchema` call the constructor at two sites.
    // Inlined there, each site holds the moves of the config and the call that
    // drops it on a refused name, so the constructor stays out of line and a
    // binary holds one body.
    #[inline(never)]
    pub fn new(config: UnionTypeConfig) -> Result<Self, DefinitionError> {
        Ok(Self {
            name: assert_name(&config.name)?,
            description: config.description,
            resolve_type: config.resolve_type,
            extensions: config.extensions,
            ast_node: config.ast_node,
            extension_ast_nodes: config.extension_ast_nodes,
            types_thunk: config.types,
            types: OnceBox::new(),
        })
    }

    /// Returns the member types, resolving the thunk on first read.
    ///
    /// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns what the thunk throws.
    pub fn get_types(&self) -> Result<&[NamedType], DefinitionError> {
        self.types
            .get_or_try_init(|| define_types(&self.types_thunk).map(Box::new))
            .map(Vec::as_slice)
    }

    /// Returns the config the type was built from, normalized.
    ///
    /// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns what the thunk throws.
    pub fn to_config(&self) -> Result<UnionTypeConfig, DefinitionError> {
        Ok(UnionTypeConfig {
            name: self.name.clone(),
            description: self.description.clone(),
            types: ThunkReadonlyArray::Array(self.get_types()?.to_vec()),
            resolve_type: self.resolve_type.clone(),
            extensions: self.extensions.clone(),
            ast_node: self.ast_node.clone(),
            extension_ast_nodes: self.extension_ast_nodes.clone(),
        })
    }
}

impl fmt::Display for UnionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name)
    }
}

impl fmt::Debug for UnionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UnionType")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

/// Builds the member list of a union type.
///
/// The `devAssert` that the result is an array has no reading, because the
/// thunk returns one.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
///
/// # Errors
///
/// Returns what the thunk throws.
fn define_types(types: &ThunkReadonlyArray<NamedType>) -> Result<Vec<NamedType>, DefinitionError> {
    Ok(resolve_readonly_array_thunk(types)?.into_owned())
}

/// The configuration of an enum value.
///
/// An absent internal value is [`JsValue::Undefined`], and the name then
/// stands in for it.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
#[derive(Clone, Debug, Default)]
pub struct EnumValueConfig {
    /// The description.
    pub description: Option<String>,
    /// The internal value, undefined when the name is the value.
    pub value: JsValue,
    /// The deprecation reason.
    pub deprecation_reason: Option<String>,
    /// The extensions.
    pub extensions: JsObject,
    /// The definition node the value was built from.
    pub ast_node: Option<EnumValueDefinitionNode>,
}

/// A value of an enum type.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
#[derive(Clone, Debug)]
pub struct EnumValue {
    /// The name.
    pub name: String,
    /// The description.
    pub description: Option<String>,
    /// The internal value.
    pub value: JsValue,
    /// The deprecation reason.
    pub deprecation_reason: Option<String>,
    /// The extensions.
    pub extensions: JsObject,
    /// The definition node the value was built from.
    pub ast_node: Option<EnumValueDefinitionNode>,
}

/// The configuration of an enum type.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
#[derive(Debug, Default)]
pub struct EnumTypeConfig {
    /// The name.
    pub name: String,
    /// The description.
    pub description: Option<String>,
    /// The values, or a thunk that returns them.
    pub values: ThunkObjMap<EnumValueConfig>,
    /// The extensions.
    pub extensions: JsObject,
    /// The definition node the type was built from.
    pub ast_node: Option<EnumTypeDefinitionNode>,
    /// The extension nodes the type was built from.
    pub extension_ast_nodes: Vec<EnumTypeExtensionNode>,
}

/// Enum Type Definition.
///
/// Some leaf values of requests and input values are Enums. GraphQL serializes
/// Enum values as strings, however internally Enums can be represented by any
/// kind of type, often integers.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
///
/// The reference keeps two lookup maps beside the values, one by internal
/// value and one by name, and both are searches over the value list here.
/// The map by internal value is a `Map`, whose keys compare under
/// `SameValueZero` and whose later entry wins on a repeated key, and the map
/// by name is `keyMap`, with the same last wins rule, so both searches run
/// from the back.
pub struct EnumType {
    /// The name.
    pub name: String,
    /// The description.
    pub description: Option<String>,
    /// The extensions.
    pub extensions: JsObject,
    /// The definition node the type was built from.
    pub ast_node: Option<EnumTypeDefinitionNode>,
    /// The extension nodes the type was built from.
    pub extension_ast_nodes: Vec<EnumTypeExtensionNode>,
    values_thunk: ThunkObjMap<EnumValueConfig>,
    values: OnceBox<Vec<EnumValue>>,
}

impl EnumType {
    /// Builds an enum type from `config`.
    ///
    /// A value map that is not a thunk is defined here, in the constructor,
    /// and a thunk is defined on first read, which is the reference's split.
    ///
    /// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the error of [`assert_name()`], or of
    /// [`assert_enum_value_name()`] for a value name when the values are
    /// not a thunk.
    pub fn new(config: EnumTypeConfig) -> Result<Self, DefinitionError> {
        let name = assert_name(&config.name)?;
        let values = OnceBox::new();
        if let ThunkObjMap::Map(value_map) = &config.values {
            let defined = define_enum_values(value_map)?;
            values.get_or_init(|| Box::new(defined));
        }
        Ok(Self {
            name,
            description: config.description,
            extensions: config.extensions,
            ast_node: config.ast_node,
            extension_ast_nodes: config.extension_ast_nodes,
            values_thunk: config.values,
            values,
        })
    }

    /// Returns the values, resolving the thunk on first read.
    ///
    /// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns what the thunk or a name assertion throws.
    pub fn get_values(&self) -> Result<&[EnumValue], DefinitionError> {
        self.values
            .get_or_try_init(|| {
                let value_map = resolve_obj_map_thunk(&self.values_thunk)?;
                define_enum_values(&value_map).map(Box::new)
            })
            .map(Vec::as_slice)
    }

    /// Returns the value named `name`.
    ///
    /// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns what [`EnumType::get_values`] throws.
    pub fn get_value(&self, name: &str) -> Result<Option<&EnumValue>, DefinitionError> {
        Ok(self
            .get_values()?
            .iter()
            .rev()
            .find(|value| value.name == name))
    }

    /// Returns the name of the value whose internal value is `output_value`.
    ///
    /// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns a `GraphQLError` when no value has that internal value, or
    /// what [`EnumType::get_values`] throws.
    pub fn serialize(&self, output_value: &JsValue) -> Result<JsValue, DefinitionError> {
        let enum_value = self
            .get_values()?
            .iter()
            .rev()
            .find(|enum_value| same_value_zero(&enum_value.value, output_value));
        match enum_value {
            None => Err(DefinitionError::graphql(format!(
                "Enum \"{}\" cannot represent value: {}",
                self.name,
                inspect(output_value)
            ))),
            Some(enum_value) => Ok(JsValue::from(enum_value.name.as_str())),
        }
    }

    /// Returns the internal value of the value named `input_value`.
    ///
    /// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns a `GraphQLError` when the input is not a string or names no
    /// value, or what [`EnumType::get_values`] throws.
    pub fn parse_value(&self, input_value: &JsValue) -> Result<JsValue, DefinitionError> {
        let JsValue::String(text) = input_value else {
            let value_str = inspect(input_value);
            return Err(DefinitionError::graphql(format!(
                "Enum \"{}\" cannot represent non-string value: {value_str}.{}",
                self.name,
                did_you_mean_enum_value(self, &value_str)?
            )));
        };

        let input_value = js_string_text(text);
        let Some(enum_value) = self.get_value(&input_value)? else {
            return Err(DefinitionError::graphql(format!(
                "Value \"{input_value}\" does not exist in \"{}\" enum.{}",
                self.name,
                did_you_mean_enum_value(self, &input_value)?
            )));
        };
        Ok(enum_value.value.clone())
    }

    /// Returns the internal value of the value the literal names.
    ///
    /// Variables are resolved to a value before this function is called, so
    /// `variables` is unread, as it is in the reference.
    ///
    /// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns a `GraphQLError` that blames the node when the literal is not
    /// an enum value or names no value, or what [`EnumType::get_values`]
    /// throws.
    pub fn parse_literal(
        &self,
        value_node: &ValueNode,
        variables: Option<&JsObject>,
    ) -> Result<JsValue, DefinitionError> {
        let _ = variables;
        let ValueNode::EnumValue(node) = value_node else {
            let value_str = print(value_node);
            return Err(DefinitionError::from(GraphQLError::new(
                format!(
                    "Enum \"{}\" cannot represent non-enum value: {value_str}.{}",
                    self.name,
                    did_you_mean_enum_value(self, &value_str)?
                ),
                blame(value_node),
            )));
        };

        let Some(enum_value) = self.get_value(&node.value)? else {
            let value_str = print(value_node);
            return Err(DefinitionError::from(GraphQLError::new(
                format!(
                    "Value \"{value_str}\" does not exist in \"{}\" enum.{}",
                    self.name,
                    did_you_mean_enum_value(self, &value_str)?
                ),
                blame(value_node),
            )));
        };
        Ok(enum_value.value.clone())
    }

    /// Returns the config the type was built from, normalized.
    ///
    /// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns what [`EnumType::get_values`] throws.
    pub fn to_config(&self) -> Result<EnumTypeConfig, DefinitionError> {
        let values: ObjMap<EnumValueConfig> = self
            .get_values()?
            .iter()
            .map(|value| {
                let config = EnumValueConfig {
                    description: value.description.clone(),
                    value: value.value.clone(),
                    deprecation_reason: value.deprecation_reason.clone(),
                    extensions: value.extensions.clone(),
                    ast_node: value.ast_node.clone(),
                };
                (value.name.clone(), config)
            })
            .collect();

        Ok(EnumTypeConfig {
            name: self.name.clone(),
            description: self.description.clone(),
            values: ThunkObjMap::Map(values),
            extensions: self.extensions.clone(),
            ast_node: self.ast_node.clone(),
            extension_ast_nodes: self.extension_ast_nodes.clone(),
        })
    }
}

impl fmt::Display for EnumType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name)
    }
}

impl fmt::Debug for EnumType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EnumType")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

/// Returns the suggestion suffix for an unknown enum value.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
///
/// # Errors
///
/// Returns what [`EnumType::get_values`] throws.
fn did_you_mean_enum_value(
    enum_type: &EnumType,
    unknown_value_str: &str,
) -> Result<String, DefinitionError> {
    let all_names: Vec<String> = enum_type
        .get_values()?
        .iter()
        .map(|value| value.name.clone())
        .collect();
    let suggested_values = suggestion_list(unknown_value_str, &all_names);

    Ok(did_you_mean(Some("the enum value"), &suggested_values))
}

/// Builds the value list of an enum type.
///
/// The two `devAssert` calls over the shapes of the map and of a value config
/// have no reading, because the config carries the shapes, and the type name
/// the reference reads for their messages has no reader here.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
///
/// # Errors
///
/// Returns the error of [`assert_enum_value_name()`] for a value name.
fn define_enum_values(
    value_map: &ObjMap<EnumValueConfig>,
) -> Result<Vec<EnumValue>, DefinitionError> {
    own_entries(value_map)
        .into_iter()
        .map(|(value_name, value_config)| {
            Ok(EnumValue {
                name: assert_enum_value_name(value_name)?,
                description: value_config.description.clone(),
                value: if value_config.value.is_undefined() {
                    JsValue::from(value_name.as_str())
                } else {
                    value_config.value.clone()
                },
                deprecation_reason: value_config.deprecation_reason.clone(),
                extensions: value_config.extensions.clone(),
                ast_node: value_config.ast_node.clone(),
            })
        })
        .collect()
}

/// The configuration of an input field.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
#[derive(Clone, Debug)]
pub struct InputFieldConfig {
    /// The description.
    pub description: Option<String>,
    /// The type.
    pub r#type: Type,
    /// The default value, undefined when there is none.
    pub default_value: JsValue,
    /// The deprecation reason.
    pub deprecation_reason: Option<String>,
    /// The extensions.
    pub extensions: JsObject,
    /// The definition node the field was built from.
    pub ast_node: Option<InputValueDefinitionNode>,
}

impl InputFieldConfig {
    /// Returns a config of `r#type` with every other field empty.
    #[must_use]
    pub fn of_type(r#type: Type) -> Self {
        Self {
            description: None,
            r#type,
            default_value: JsValue::Undefined,
            deprecation_reason: None,
            extensions: JsObject::new(),
            ast_node: None,
        }
    }
}

/// A field of an input object type.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
#[derive(Clone, Debug)]
pub struct InputField {
    /// The name.
    pub name: String,
    /// The description.
    pub description: Option<String>,
    /// The type.
    pub r#type: Type,
    /// The default value, undefined when there is none.
    pub default_value: JsValue,
    /// The deprecation reason.
    pub deprecation_reason: Option<String>,
    /// The extensions.
    pub extensions: JsObject,
    /// The definition node the field was built from.
    pub ast_node: Option<InputValueDefinitionNode>,
}

/// Returns whether the input field has to be given.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
#[must_use]
pub fn is_required_input_field(field: &InputField) -> bool {
    is_non_null_type(&field.r#type) && field.default_value.is_undefined()
}

/// The configuration of an input object type.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
#[derive(Debug, Default)]
pub struct InputObjectTypeConfig {
    /// The name.
    pub name: String,
    /// The description.
    pub description: Option<String>,
    /// The fields, or a thunk that returns them.
    pub fields: ThunkObjMap<InputFieldConfig>,
    /// The extensions.
    pub extensions: JsObject,
    /// The definition node the type was built from.
    pub ast_node: Option<InputObjectTypeDefinitionNode>,
    /// The extension nodes the type was built from.
    pub extension_ast_nodes: Vec<InputObjectTypeExtensionNode>,
    /// Whether exactly one field has to be given.
    pub is_one_of: bool,
}

/// Input Object Type Definition.
///
/// An input object defines a structured collection of fields which may be
/// supplied to a field argument.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
pub struct InputObjectType {
    /// The name.
    pub name: String,
    /// The description.
    pub description: Option<String>,
    /// The extensions.
    pub extensions: JsObject,
    /// The definition node the type was built from.
    pub ast_node: Option<InputObjectTypeDefinitionNode>,
    /// The extension nodes the type was built from.
    pub extension_ast_nodes: Vec<InputObjectTypeExtensionNode>,
    /// Whether exactly one field has to be given.
    pub is_one_of: bool,
    fields_thunk: ThunkObjMap<InputFieldConfig>,
    fields: OnceBox<ObjMap<InputField>>,
}

impl InputObjectType {
    /// Builds an input object type from `config`.
    ///
    /// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the error of [`assert_name()`].
    pub fn new(config: InputObjectTypeConfig) -> Result<Self, DefinitionError> {
        Ok(Self {
            name: assert_name(&config.name)?,
            description: config.description,
            extensions: config.extensions,
            ast_node: config.ast_node,
            extension_ast_nodes: config.extension_ast_nodes,
            is_one_of: config.is_one_of,
            fields_thunk: config.fields,
            fields: OnceBox::new(),
        })
    }

    /// Returns the fields, resolving the thunk on first read.
    ///
    /// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns what the thunk or a name assertion throws.
    pub fn get_fields(&self) -> Result<&ObjMap<InputField>, DefinitionError> {
        self.fields
            .get_or_try_init(|| define_input_field_map(&self.fields_thunk).map(Box::new))
    }

    /// Returns the config the type was built from, normalized.
    ///
    /// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns what the thunk throws.
    pub fn to_config(&self) -> Result<InputObjectTypeConfig, DefinitionError> {
        let fields: ObjMap<InputFieldConfig> = self
            .get_fields()?
            .iter()
            .map(|(name, field)| {
                let config = InputFieldConfig {
                    description: field.description.clone(),
                    r#type: field.r#type.clone(),
                    default_value: field.default_value.clone(),
                    deprecation_reason: field.deprecation_reason.clone(),
                    extensions: field.extensions.clone(),
                    ast_node: field.ast_node.clone(),
                };
                (name.clone(), config)
            })
            .collect();

        Ok(InputObjectTypeConfig {
            name: self.name.clone(),
            description: self.description.clone(),
            fields: ThunkObjMap::Map(fields),
            extensions: self.extensions.clone(),
            ast_node: self.ast_node.clone(),
            extension_ast_nodes: self.extension_ast_nodes.clone(),
            is_one_of: self.is_one_of,
        })
    }
}

impl fmt::Display for InputObjectType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name)
    }
}

impl fmt::Debug for InputObjectType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InputObjectType")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

/// Builds the field map of an input object type.
///
/// The two `devAssert` calls, over the shape of the map and over a `resolve`
/// property that an input field must not carry, have no reading, because the
/// config carries neither, and the type name the reference reads for their
/// messages has no reader here.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
///
/// # Errors
///
/// Returns what the thunk throws, or the error of [`assert_name()`] for a
/// field name.
fn define_input_field_map(
    fields: &ThunkObjMap<InputFieldConfig>,
) -> Result<ObjMap<InputField>, DefinitionError> {
    let field_map = resolve_obj_map_thunk(fields)?;

    let mut result = ObjMap::default();
    for (field_name, field_config) in own_entries(&field_map) {
        let field = InputField {
            name: assert_name(field_name)?,
            description: field_config.description.clone(),
            r#type: field_config.r#type.clone(),
            default_value: field_config.default_value.clone(),
            deprecation_reason: field_config.deprecation_reason.clone(),
            extensions: field_config.extensions.clone(),
            ast_node: field_config.ast_node.clone(),
        };
        result.insert(field_name.clone(), field);
    }
    Ok(result)
}
