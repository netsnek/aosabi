// SPDX-License-Identifier: MIT
/*
 * aosabi-execute execute: the executor of one operation.
 *
 * Derived from @graphql-tools/executor 1.5.1, cjs/execution/execute.js, MIT.
 * Derived from @graphql-tools/utils 11.0.0, cjs/rootTypes.js, MIT.
 * Derived from graphql-js 16.12.0, src/execution/execute.ts, MIT.
 */

//! The executor of one operation.
//!
//! Every function keeps the name it has in `execute.js`, in snake case, and
//! the order of the file is the order of the reference. A value the
//! reference types as `PromiseOrValue` is a [`Value`], a `throw` is the `Err`
//! of a `Result` or a [`Completion::Throw`], and a `try` block is a match on
//! that result at the same place.
//!
//! # What is left out
//!
//! | Reference | Why |
//! | --- | --- |
//! | the abort signal, `signalPromise` and `throwIfAborted`, `execute.js:65`, `:153`, `:208-229` and every `exeContext.signal` read | the server hands the executor no signal |
//! | `@defer`, `@stream`, the incremental results, `subsequentPayloads`, `filterSubsequentPayloads` and the async iterable list value, `execute.js:79-87`, `:276-279`, `:516-581`, `:586-589`, `:593`, `:597`, `:604-608`, `:751-754` and `:1022-1385` | no directive named `defer` or `stream` reaches a schema of Aosabi, and a resolver value of Aosabi is no async iterable |
//! | `schemaCoordinateInErrors`, the fourth argument of every `locatedError` | Yoga never asks for it |
//! | `executeSync` and `assertValidExecutionArguments`, `execute.js:105-132` | no caller, the entry is [`execute`] |
//!
//! # Where a promise is continued
//!
//! Every `.then` of the reference is one [`Promise::then`] or one
//! `Promise::then_with`, the latter with the handlers of the executor in
//! place, and nothing else in the crate calls them.
//!
//! | Reference | Call |
//! | --- | --- |
//! | `normalizedExecutor.js:17-22` | `handle_maybe_promise(execute, identity)`, in [`crate::normalized_executor()`] |
//! | `execute.js:77-99` | `handle_maybe_promise(execute_operation, build, on_error)`, in `execute_impl` |
//! | `execute.js:286-298` | [`promise_reduce`] with one `handle_maybe_promise` for the accumulator and one for the field |
//! | `execute.js:319-329` | `handle_maybe_promise(promise_for_object, rethrow, rethrow)` when a sibling is a promise |
//! | `promiseForObject.js:16-31` | one `then_with` per promised key, the single promise or `Promise::all_of`, and one `then_with`, each with the handlers `Handlers::Object`, in [`crate::promise_for_object`] |
//! | `execute.js:368` | `result.then_with(..)` with `Continuation::Complete`, which runs `complete_value` |
//! | `execute.js:376-391` | `completed.then_with(..)` with `Continuation::FieldError`, the second tick of a rejection |
//! | `execute.js:625` and `:633-639` | the same pair for a list item, with `Continuation::Complete` and `Continuation::ItemError` |
//! | `execute.js:614` | `Promise::all(completed_results)` when one item is a promise |
//! | `execute.js:687` | `runtime_type.then(..)` |
//! | `execute.js:729-734` | `is_type_of.then(..)` |
//! | `execute.js:788-794` | `Promise::all(promised_is_type_of_results).then(..)` |
//!
//! # The agent
//!
//! Every function takes the [`Agent`] of the request first. The context does
//! not hold it, because a job of the agent holds the context, and the two
//! would keep each other alive after the request.

use core::cell::{
    OnceCell,
    RefCell, //
};
use std::{
    borrow::Cow,
    rc::Rc,
    sync::{
        Arc,
        LazyLock,
        OnceLock, //
    }, //
};

use aosabi_json::{
    JsObject,
    JsValue,
    map::IndexMap,
    value::KeyHasherBuilder, //
};
use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions,
    ast::{
        AstNode,
        DefinitionNode,
        DocumentNode,
        FieldNode,
        FragmentDefinitionNode,
        NameNode,
        OperationDefinitionNode,
        OperationTypeNode, //
    },
    js_string::to_string_lossy, //
};
use aosabi_scalars::{
    JsDate,
    ScalarError,
    ScalarResult,
    ScalarValue, //
};
use aosabi_type::{
    DefinitionError,
    Field,
    NamedType,
    ObjMap,
    ObjectType,
    Schema,
    SharedType,
    Type,
    assert_valid_schema,
    is_abstract_type,
    is_leaf_type,
    is_list_type,
    is_non_null_type,
    is_object_type,
    is_specified_scalar_name,
    schema_meta_field_def,
    specified_scalar_types,
    type_meta_field_def,
    type_name_meta_field_def, //
};

use crate::{
    arena::{
        Arena,
        Kept,
        Key,
        NodeRef, //
    },
    coerce_error::coerce_error,
    errors::{
        create_graphql_error,
        into_graphql_error,
        located_error,
        thrown_definition_error, //
    },
    handle_maybe_promise::{
        MaybePromise,
        handle_maybe_promise,
        is_promise, //
    },
    introspection::resolve_introspection_field,
    path::{
        Path,
        PathFrame, //
    },
    plan::{
        ArgsKind,
        KeptArgs,
        LeafKind,
        Plan,
        PlanField,
        PlanNode,
        ResolverKind,
        plan_for, //
    },
    promise::{
        Agent,
        Promise,
        promise::Handlers, //
    },
    promise_for_object::promise_for_object,
    promise_reduce::{
        ReduceCallback,
        promise_reduce, //
    },
    result::{
        ResultObject,
        errors_value, //
    },
    value::{
        Completion,
        ContextValue,
        ErrorObject,
        Function,
        HostValue,
        MemberSlot,
        ObjectLike,
        PlainObject,
        Value,
        cross_inspect,
        inspect,
        to_js_string, //
    },
    values::{
        CoercedVariableValues,
        get_argument_values,
        get_variable_values, //
    }, //
};

/// The field nodes of one response key, in document order.
///
/// A list is built once per selection set and shared from then on, so its
/// address names it, which is what the caches keyed by a node list rely on.
pub type FieldNodes = Rc<[Rc<FieldNode>]>;

/// A `resolve` or a `subscribe` function, called with `(source, args, contextValue, info)`.
///
/// This is the shape graphql-js calls a resolver with. The function is
/// shared between requests and threads, and the values it is called with
/// belong to one request.
pub type FieldResolverFn = Arc<
    dyn Fn(&Agent, &Value, &Rc<PlainObject>, &ContextValue, &Rc<ResolveInfo>) -> Completion
        + Send
        + Sync,
>;

/// A `resolveType` function, called with `(value, contextValue, info, abstractType)`.
///
/// It answers the name of an object type as a string, or a promise of it.
pub type TypeResolverFn = Arc<
    dyn Fn(&Agent, &Value, &ContextValue, &Rc<ResolveInfo>, &NamedType) -> Completion + Send + Sync,
>;

/// An `isTypeOf` function, called with `(value, contextValue, info)`.
///
/// It answers a value that is read as a boolean, or a promise of it.
pub type IsTypeOfFn =
    Arc<dyn Fn(&Agent, &Value, &ContextValue, &Rc<ResolveInfo>) -> Completion + Send + Sync>;

/// What the `resolve` and the `subscribe` slot of a field hold for the executor.
///
/// A slot of the type layer is an [`aosabi_type::ResolverHandle`] without a
/// signature. [`execute_field`] reads the handle as a `FieldResolve` first and
/// as an [`IntrospectionResolver`](aosabi_type::IntrospectionResolver) second.
pub struct FieldResolve(pub FieldResolverFn);

/// What the `resolveType` slot of an interface or a union holds for the executor.
pub struct ResolveType(pub TypeResolverFn);

/// What the `isTypeOf` slot of an object type holds for the executor.
pub struct IsTypeOf(pub IsTypeOfFn);

/// A serializer of a scalar that reads the executor value.
///
/// The `serialize` hook of the type layer reads a [`JsValue`], which cannot
/// carry a `Date` or a function. A scalar whose serializer tells those apart
/// registers one of these by its name, and [`complete_leaf_value`] asks it
/// before it asks the hook. The `Err` is the thrown value.
pub type LeafSerializer = Arc<dyn Fn(&Value) -> Result<JsValue, Value> + Send + Sync>;

/// Returns the leaf serializer `value == null ? value : String(value)`.
///
/// This is the `serialize` Aosabi binds over `ID` and `GID`: a value that is not
/// nullish is printed through `ToString`, which refuses no value of the model
/// (see [`to_js_string`]). Every call answers the one serializer the process
/// holds, so an execution knows it among the serializers of its request and
/// completes a string of its scalar as the string itself, which is what the
/// serializer answers for it, with no call and no copy.
#[must_use]
pub fn string_of_value() -> LeafSerializer {
    Arc::clone(&STRING_OF_VALUE)
}

/// The one serializer [`string_of_value`] answers.
static STRING_OF_VALUE: LazyLock<LeafSerializer> = LazyLock::new(|| {
    Arc::new(|value: &Value| match value {
        Value::Undefined => Ok(JsValue::Undefined),
        Value::Null => Ok(JsValue::Null),
        other => Ok(JsValue::String(to_js_string(other))),
    })
});

/// Returns the leaf serializer that answers a number as itself and throws
/// what `refuse` makes of any other value.
///
/// This is the `serialize` Aosabi binds over `Number`, whose message the binder
/// makes in `refuse`: a value of the type `number` is returned as it is, `NaN`
/// and both infinities among them, which the writer prints as `null` as
/// `JSON.stringify` does, and every other value is refused. The first call makes
/// the one serializer the process holds, and every later call with the same
/// function answers it, so an execution knows it among the serializers of its
/// request and completes a number of its scalar as the number itself, with no
/// call. A call with another function answers a serializer of its own, which
/// completes every value through a call. Rust promises no single address per
/// function, so one function at two addresses costs that call, while two
/// functions merged to one address have one body and refuse alike.
#[must_use]
pub fn number_as_itself(refuse: fn(&Value) -> Value) -> LeafSerializer {
    let (held, serializer) = NUMBER_AS_ITSELF.get_or_init(|| (refuse, numbers_or(refuse)));
    if core::ptr::fn_addr_eq(*held, refuse) {
        Arc::clone(serializer)
    } else {
        numbers_or(refuse)
    }
}

/// The function a serializer of [`number_as_itself`] makes the error of a refused value with.
type Refusal = fn(&Value) -> Value;

/// The serializer [`number_as_itself`] made on its first call, with the function it refuses with.
static NUMBER_AS_ITSELF: OnceLock<(Refusal, LeafSerializer)> = OnceLock::new();

/// Returns a serializer that answers a number as itself and throws what
/// `refuse` makes of any other value.
fn numbers_or(refuse: Refusal) -> LeafSerializer {
    Arc::new(move |value: &Value| match value {
        Value::Number(number) => Ok(JsValue::Number(*number)),
        other => Err(refuse(other)),
    })
}

/// Answers whether `serializer` is the one serializer [`number_as_itself`] holds.
fn is_number_as_itself(serializer: &LeafSerializer) -> bool {
    NUMBER_AS_ITSELF
        .get()
        .is_some_and(|(_, held)| Arc::ptr_eq(serializer, held))
}

/// Returns the leaf serializer that answers an object as the value of the
/// JSON model it is and throws what `refuse` makes of any other value.
///
/// This is the `serialize` Aosabi binds over `JSONObject`, `ensureObject` of
/// graphql-scalars 1.25.0, whose message the binder makes in `refuse` over the
/// value of the executor: a value whose `typeof` is `object` and that is neither
/// `null` nor an array is returned as it is, a date among them, and every other
/// value is refused. An object is answered as the value of the JSON model
/// [`Value::to_js`] makes of it, and a date as the text of its `toJSON`, which a
/// valid date alone answers as a string. The first call makes the one serializer
/// the process holds, and every later call with the same function answers it, so
/// an execution knows it among the serializers of its request and completes an
/// object and a date of its scalar with no call, a date as itself, which the
/// answer writes through its `toJSON` (see `LeafRules`). A call with another
/// function answers a serializer of its own, which completes every value through
/// a call, as [`number_as_itself`] does, and so refuses an invalid date, whose
/// `toJSON` it answers as `null`, where the one serializer the process holds
/// passes the date, whose `toJSON` the answer writes as `null`, as the reference
/// does.
#[must_use]
pub fn object_as_itself(refuse: fn(&Value) -> Value) -> LeafSerializer {
    let (held, serializer) = OBJECT_AS_ITSELF.get_or_init(|| (refuse, objects_or(refuse)));
    if core::ptr::fn_addr_eq(*held, refuse) {
        Arc::clone(serializer)
    } else {
        objects_or(refuse)
    }
}

/// The serializer [`object_as_itself`] made on its first call, with the function it refuses with.
static OBJECT_AS_ITSELF: OnceLock<(Refusal, LeafSerializer)> = OnceLock::new();

/// Returns a serializer that answers an object and a date as the values of
/// the JSON model they are and throws what `refuse` makes of any other value.
fn objects_or(refuse: Refusal) -> LeafSerializer {
    Arc::new(move |value: &Value| match value {
        Value::Date(date) => Ok(date.to_iso_string().map_or(JsValue::Null, JsValue::from)),
        other => match other.to_js() {
            object @ JsValue::Object(_) => Ok(object),
            _ => Err(refuse(other)),
        },
    })
}

/// Answers whether `serializer` is the one serializer [`object_as_itself`] holds.
fn is_object_as_itself(serializer: &LeafSerializer) -> bool {
    OBJECT_AS_ITSELF
        .get()
        .is_some_and(|(_, held)| Arc::ptr_eq(serializer, held))
}

/// A `serialize` hook of `aosabi-scalars`, which answers the scalar value a
/// value serializes to or the error the hook throws.
pub type ScalarHook = fn(&ScalarValue) -> ScalarResult;

/// Returns the leaf serializer over the `serialize` hook `hook` of
/// `aosabi-scalars`.
///
/// A date reaches the hook as the date it is and every other value as the value
/// of the JSON model [`Value::to_js`] makes of it. A date the hook answers is
/// the text of its `toISOString`, or `null` for an invalid date, which is also
/// what `JSON.stringify` writes for it, and an error the hook throws keeps its
/// kind, a `GraphQLError` or the `RangeError` of `toISOString`.
///
/// Aosabi binds two hooks the executor knows, the `serialize` of `DateTimeISO`
/// over `Date` and of `JSON`, and binds `JSONObject` through
/// [`object_as_itself`]. A call with one of the two answers the one serializer
/// of it the process holds, so an execution knows it among the serializers of
/// its request and completes a valid date as the digits of its `toISOString` and
/// a value of the JSON model as itself, with no call of the hook and no copy
/// (see `LeafRules`). Rust promises no single address per function, so one hook
/// at two addresses answers a serializer of its own at the second, which costs
/// that call and changes one answer: it answers an invalid date under `JSON` as
/// `null`, which the completion of a leaf refuses, where the serializer the
/// process holds passes the date, whose `toJSON` the answer writes as `null`, as
/// the reference does. Two hooks merged to one address have one body and answer
/// alike.
#[must_use]
pub fn scalar_hook(hook: ScalarHook) -> LeafSerializer {
    for (known, held) in known_hooks() {
        if core::ptr::fn_addr_eq(hook, known) {
            return Arc::clone(held.get_or_init(|| hook_serializer(known)));
        }
    }
    hook_serializer(hook)
}

/// The serializer of the hook of `DateTimeISO` that [`scalar_hook`] answers.
static DATE_TIME_ISO: OnceLock<LeafSerializer> = OnceLock::new();

/// The serializer of the hook of `JSON` that [`scalar_hook`] answers.
static JSON_AS_ITSELF: OnceLock<LeafSerializer> = OnceLock::new();

/// Returns the hooks the executor knows, each with the cell of the one
/// serializer the process holds of it.
fn known_hooks() -> [(ScalarHook, &'static OnceLock<LeafSerializer>); 2] {
    [
        (aosabi_scalars::date_time_iso::serialize, &DATE_TIME_ISO),
        (aosabi_scalars::json::serialize, &JSON_AS_ITSELF),
    ]
}

/// Returns a new serializer over the hook `hook`, as [`scalar_hook`] reads it.
fn hook_serializer(hook: ScalarHook) -> LeafSerializer {
    Arc::new(move |value: &Value| {
        let input = match value {
            Value::Date(date) => ScalarValue::Date(*date),
            other => ScalarValue::Value(other.to_js()),
        };
        match hook(&input) {
            Ok(ScalarValue::Value(value)) => Ok(value),
            Ok(ScalarValue::Date(date)) => {
                Ok(date.to_iso_string().map_or(JsValue::Null, JsValue::from))
            }
            Err(ScalarError::GraphQL(error)) => Err(Value::from(ErrorObject::GraphQL(error))),
            Err(ScalarError::Range(error)) => {
                Err(Value::from(ErrorObject::range_error(error.message())))
            }
        }
    })
}

/// Answers whether `serializer` is the serializer the cell `held` holds.
fn is_held(held: &OnceLock<LeafSerializer>, serializer: &LeafSerializer) -> bool {
    held.get()
        .is_some_and(|known| Arc::ptr_eq(serializer, known))
}

/// Which serializer of the request binds a scalar of a known kind in one execution.
///
/// A scalar a serializer of the request binds completes through it, a
/// specified scalar through the type layer and any other scalar as itself
/// (see [`serialize_leaf_value`]). The serializers are no part of the key of
/// a plan, so the plan names the kind of a leaf by its named type alone (see
/// [`LeafKind`]), and an execution reads once which serializer of its
/// request binds each of the nine known kinds of a scalar.
#[derive(Clone, Copy)]
enum LeafRule {
    /// No serializer of the request binds the scalar, so the type layer or the
    /// identity completes it.
    Unbound,
    /// The serializer [`string_of_value`] binds the scalar.
    StringOfValue,
    /// The serializer [`number_as_itself`] holds binds the scalar.
    NumberAsItself,
    /// The serializer of the hook of `DateTimeISO` [`scalar_hook`] holds binds the scalar.
    DateTimeIso,
    /// The serializer of the hook of `JSON` [`scalar_hook`] holds binds the scalar.
    Json,
    /// The serializer [`object_as_itself`] holds binds the scalar.
    JsonObject,
    /// The serializer at this position of the map of the request binds the scalar.
    Bound(usize),
}

impl LeafRule {
    /// Returns the serializer of the request the rule names, none for a scalar no serializer binds.
    fn serializer(self, exe_context: &ExecutionContext) -> Option<&LeafSerializer> {
        match self {
            Self::Unbound => None,
            Self::StringOfValue => Some(&*STRING_OF_VALUE),
            Self::NumberAsItself => NUMBER_AS_ITSELF.get().map(|(_, serializer)| serializer),
            Self::DateTimeIso => DATE_TIME_ISO.get(),
            Self::Json => JSON_AS_ITSELF.get(),
            Self::JsonObject => OBJECT_AS_ITSELF.get().map(|(_, serializer)| serializer),
            Self::Bound(position) => exe_context
                .leaf_serializers
                .as_ref()
                .and_then(|serializers| serializers.get_index(position))
                .map(|(_, serializer)| serializer),
        }
    }

    /// Answers how the leaf `result` of a bound scalar, `Date`, `JSON` or
    /// `JSONObject`, completes where the rule binds it.
    ///
    /// The hook of `DateTimeISO` answers a valid date as the text of its
    /// `toISOString`. The hook of `JSON` answers every value as itself,
    /// `identity` at `utils.js:3-5`, and the serializer of `JSONObject` an
    /// object and a date, `ensureObject` at `utils.js:6-15`, so a date stays a
    /// date there, no null, which `JSON.stringify` writes through its `toJSON`,
    /// the text of its `toISOString` or `null` for an invalid date. A value so
    /// answered goes into the arena with no call: a string, a boolean and a
    /// number as themselves, a date as the value of its `toJSON`, and a list and
    /// an object as the value of the JSON model [`Value::to_js`] makes of them.
    /// Every other value, an invalid date of `Date` among them, goes through its
    /// serializer.
    fn pass_bound<'a>(self, exe_context: &'a ExecutionContext, result: LeafRef<'_>) -> Pass<'a> {
        match (self, result) {
            (Self::DateTimeIso, LeafRef::Date(date)) if date.get_time().is_finite() => {
                Pass::Date(date)
            }
            (Self::Json | Self::JsonObject, LeafRef::Date(date)) => Pass::Date(date),
            (Self::Json, LeafRef::Text(_) | LeafRef::Bool | LeafRef::Number(_)) => Pass::Itself,
            (Self::Json, LeafRef::Object | LeafRef::List) | (Self::JsonObject, LeafRef::Object) => {
                Pass::Json
            }
            _ => Pass::Serialize(Bound::Found(self.serializer(exe_context))),
        }
    }

    /// Answers whether the value of the JSON model `value` that a member
    /// holds passes as itself where the rule binds its scalar: every value
    /// under the hook of `JSON` and an object under the serializer of
    /// `JSONObject`.
    fn keeps(self, value: Kept<'_>) -> bool {
        match self {
            Self::Json => true,
            Self::JsonObject => value.is_object(),
            _ => false,
        }
    }
}

/// The values of a known kind whose serializer answers them as themselves.
#[derive(Clone, Copy)]
enum Passes {
    /// No value.
    Nothing,
    /// A string.
    Strings,
    /// A boolean.
    Booleans,
    /// A number that is an integer of 32 bits.
    Int32,
    /// A finite number.
    Finite,
    /// Every number.
    Numbers,
}

/// How the leaves of one known kind complete in one execution.
#[derive(Clone, Copy)]
struct KindRule {
    /// The serializer of the request that binds the scalar.
    rule: LeafRule,
    /// The values its serializer answers as themselves.
    passes: Passes,
    /// The address of the scalar type a leaf has to be of for a value to pass,
    /// the specified scalar of graphql-js, or none where every type of the
    /// kind passes.
    type_at: Option<usize>,
}

/// The rules of the leaves of the nine known kinds of a scalar in one
/// execution, read from the serializers of its request when its context is
/// built.
#[derive(Clone, Copy)]
pub(crate) struct LeafRules {
    /// The rule of `String`.
    string: KindRule,
    /// The rule of `ID`.
    id: KindRule,
    /// The rule of `Boolean`.
    boolean: KindRule,
    /// The rule of `Int`.
    int: KindRule,
    /// The rule of `Float`.
    float: KindRule,
    /// The rule of the scalar `Number`.
    number: KindRule,
    /// The serializer of the request that binds the scalar `Date`.
    date: LeafRule,
    /// The serializer of the request that binds the scalar `JSON`.
    json: LeafRule,
    /// The serializer of the request that binds the scalar `JSONObject`.
    json_object: LeafRule,
    /// Whether the name of a type passes as itself as the value of
    /// `__typename` (see [`LeafRules::passes_type_name`]).
    type_name: bool,
}

/// What [`LeafRules::pass`] reads of a leaf: the WTF-8 bytes of a string, a
/// boolean, a number, a date, or the class of any other value.
#[derive(Clone, Copy)]
enum LeafRef<'v> {
    /// A string by its WTF-8 bytes, which a `&str` holds as its UTF-8.
    Text(&'v [u8]),
    /// A boolean.
    Bool,
    /// A number.
    Number(f64),
    /// A date.
    Date(JsDate),
    /// An object.
    Object,
    /// A list.
    List,
    /// Any other value.
    Other,
}

impl<'v> LeafRef<'v> {
    /// Returns what the rules read of `value`.
    fn of(value: &'v Value) -> Self {
        match value {
            Value::String(text) => Self::Text(text.as_wtf8()),
            Value::Bool(_) => Self::Bool,
            Value::Number(number) => Self::Number(*number),
            Value::Date(date) => Self::Date(*date),
            Value::Object(_) => Self::Object,
            Value::List(_) => Self::List,
            _ => Self::Other,
        }
    }
}

/// How a leaf of a known kind completes.
#[derive(Clone, Copy)]
enum Pass<'a> {
    /// As itself, which its serializer answers for it.
    Itself,
    /// As the name of the enum value whose internal value it is.
    Name(&'a str),
    /// As the value of `toJSON` of the date, the digits of its `toISOString`
    /// or `null` for an invalid date, which the reference writes of the value
    /// its serializer answers.
    Date(JsDate),
    /// As the value of the JSON model [`Value::to_js`] makes of it, which its serializer answers for it.
    Json,
    /// Through its serializer, with the serializer of the request that binds the scalar.
    Serialize(Bound<'a>),
}

/// A leaf as its serializer answered it.
enum Serialized {
    /// A value of the executor, which the identity of a scalar with no
    /// serializer answers as the value itself.
    Value(Value),
    /// A value of the JSON model, which a serializer answered.
    Json(JsValue),
}

impl Serialized {
    /// Returns whether the leaf is `null` or `undefined`.
    fn is_nullish(&self) -> bool {
        match self {
            Self::Value(value) => value.is_nullish(),
            Self::Json(value) => matches!(value, JsValue::Null | JsValue::Undefined),
        }
    }

    /// Returns the leaf as a value of the executor.
    fn into_value(self) -> Value {
        match self {
            Self::Value(value) => value,
            Self::Json(value) => Value::from_js_owned(value),
        }
    }
}

/// The serializer of the request a leaf of a scalar is asked through.
#[derive(Clone, Copy)]
enum Bound<'a> {
    /// The map of the request is read by the name of the scalar.
    ByName,
    /// The serializer the rules of the execution found, or none.
    Found(Option<&'a LeafSerializer>),
}

impl LeafRules {
    /// Reads the rules of the nine known kinds of a scalar from `serializers`, the serializers of a request.
    ///
    /// The type layer serializes `String`, `ID`, `Boolean`, `Int` and `Float`
    /// where no serializer binds them, so a value passes where the type of its
    /// leaf is the specified scalar that [`specified_scalar_types`] holds, whose
    /// serializer is the one of graphql-js. A schema holds those types where no
    /// binding replaced them. `Number` is no specified scalar, and the identity
    /// completes it where no serializer binds it, while the serializer
    /// [`number_as_itself`] holds, which Aosabi binds over it, answers a number
    /// as itself as well. `Date`, `JSON` and `JSONObject` complete through the
    /// serializer that binds them, which the rule names, so no leaf of theirs
    /// reads the map of the request by name.
    fn new(serializers: Option<&ObjMap<LeafSerializer>>) -> Self {
        let specified = specified_scalar_types();
        let type_at = |at: usize| {
            specified
                .get(at)
                .map(|scalar| SharedType::as_ptr(scalar).addr())
        };
        let rule = |name: &str| {
            let Some(serializers) = serializers else {
                return LeafRule::Unbound;
            };
            match serializers.get_index_of(name) {
                None => LeafRule::Unbound,
                Some(position) => match serializers.get_index(position) {
                    Some((_, serializer)) if Arc::ptr_eq(serializer, &STRING_OF_VALUE) => {
                        LeafRule::StringOfValue
                    }
                    Some((_, serializer)) if is_number_as_itself(serializer) => {
                        LeafRule::NumberAsItself
                    }
                    Some((_, serializer)) if is_held(&DATE_TIME_ISO, serializer) => {
                        LeafRule::DateTimeIso
                    }
                    Some((_, serializer)) if is_held(&JSON_AS_ITSELF, serializer) => LeafRule::Json,
                    Some((_, serializer)) if is_object_as_itself(serializer) => {
                        LeafRule::JsonObject
                    }
                    _ => LeafRule::Bound(position),
                },
            }
        };
        // A scalar of graphql-js passes its values where no serializer binds
        // it and its type is the specified one.
        let specified_rule = |name: &str, passes: Passes, at: usize| {
            let rule = rule(name);
            match rule {
                LeafRule::Unbound => KindRule {
                    rule,
                    passes,
                    type_at: type_at(at),
                },
                _ => KindRule {
                    rule,
                    passes: Passes::Nothing,
                    type_at: None,
                },
            }
        };
        // `String(value)` answers a string as itself whatever its type.
        let text_rule = |name: &str, at: usize| match rule(name) {
            LeafRule::StringOfValue => KindRule {
                rule: LeafRule::StringOfValue,
                passes: Passes::Strings,
                type_at: None,
            },
            _ => specified_rule(name, Passes::Strings, at),
        };
        let number = rule("Number");
        let string = text_rule("String", SPECIFIED_STRING);
        // The test of `pass` for a string of the type of `__typename`.
        let type_name = matches!(string.passes, Passes::Strings)
            && string.type_at.is_none_or(|at| {
                matches!(
                    leaf_of(&type_name_meta_field_def().r#type),
                    Some(Type::Scalar(scalar)) if SharedType::as_ptr(scalar).addr() == at
                )
            });
        Self {
            string,
            id: text_rule("ID", SPECIFIED_ID),
            boolean: specified_rule("Boolean", Passes::Booleans, SPECIFIED_BOOLEAN),
            int: specified_rule("Int", Passes::Int32, SPECIFIED_INT),
            float: specified_rule("Float", Passes::Finite, SPECIFIED_FLOAT),
            number: KindRule {
                rule: number,
                passes: match number {
                    LeafRule::Unbound | LeafRule::NumberAsItself => Passes::Numbers,
                    LeafRule::StringOfValue
                    | LeafRule::DateTimeIso
                    | LeafRule::Json
                    | LeafRule::JsonObject
                    | LeafRule::Bound(_) => Passes::Nothing,
                },
                type_at: None,
            },
            date: rule("Date"),
            json: rule("JSON"),
            json_object: rule("JSONObject"),
            type_name,
        }
    }

    /// Answers whether the name of a type passes as itself as the value of
    /// `__typename`, as [`LeafRules::pass`] answers for a string of the type
    /// of the meta field, the specified `String`.
    ///
    /// The name passes where no serializer of the request binds `String` or
    /// where the serializer [`string_of_value`] binds it, which answers a string
    /// as itself.
    fn passes_type_name(&self) -> bool {
        self.type_name
    }

    /// Answers whether the value of the JSON model `value` that a member
    /// holds passes as itself as a leaf of the kind `kind`, so the arena
    /// writes it from where it stands (see [`LeafRule::keeps`]).
    fn keeps(&self, kind: Option<LeafKind>, value: Kept<'_>) -> bool {
        match kind {
            Some(LeafKind::Date) => self.date.keeps(value),
            Some(LeafKind::Json) => self.json.keeps(value),
            Some(LeafKind::JsonObject) => self.json_object.keeps(value),
            _ => false,
        }
    }

    /// Answers how the leaf `result` of `return_type`, a leaf of the kind
    /// `kind`, completes: as itself, as the name of an enum value, or through
    /// its serializer.
    ///
    /// A value passes where its serializer answers it as itself, by the
    /// reference's rules: a string under `String` and `ID`, a boolean under
    /// `Boolean` (:189-191), a number under `Int` that is an integer of 32 bits
    /// (:42-44), a finite number under `Float` (:105-107), a number under
    /// `Number` where no serializer of the request binds it, whose identity
    /// answers every value as itself, or where the serializer
    /// [`number_as_itself`] holds binds it, which answers every number as
    /// itself, and a string that is the internal value of a value of an enum,
    /// which answers the name of that value. A leaf of `Date`, `JSON` or
    /// `JSONObject` passes as its serializer answers it where the serializer is
    /// one Aosabi binds over them (see [`LeafRule::pass_bound`]). Every other
    /// value, and a value of a scalar a serializer binds that the execution does
    /// not know, goes through its serializer, the one the rules found.
    // Always inlined, so the test of a leaf lands in its two callers, the
    // completion of a leaf and the append of the leaf of a data member, where a
    // call keeps the test a call of its own on every leaf.
    #[expect(clippy::inline_always)]
    #[inline(always)]
    fn pass<'a>(
        &self,
        exe_context: &'a ExecutionContext,
        kind: Option<LeafKind>,
        return_type: &'a Type,
        result: LeafRef<'_>,
    ) -> Pass<'a> {
        let rule = match kind {
            Some(LeafKind::String) => &self.string,
            Some(LeafKind::Id) => &self.id,
            Some(LeafKind::Boolean) => &self.boolean,
            Some(LeafKind::Int) => &self.int,
            Some(LeafKind::Float) => &self.float,
            Some(LeafKind::Number) => &self.number,
            Some(LeafKind::Enum) => {
                let name = match result {
                    LeafRef::Text(text) => enum_name(return_type, text),
                    _ => None,
                };
                return name.map_or(Pass::Serialize(Bound::Found(None)), Pass::Name);
            }
            Some(LeafKind::Date) => return self.date.pass_bound(exe_context, result),
            Some(LeafKind::Json) => return self.json.pass_bound(exe_context, result),
            Some(LeafKind::JsonObject) => return self.json_object.pass_bound(exe_context, result),
            _ => return Pass::Serialize(Bound::ByName),
        };
        let passes = match (rule.passes, result) {
            (Passes::Strings, LeafRef::Text(_))
            | (Passes::Booleans, LeafRef::Bool)
            | (Passes::Numbers, LeafRef::Number(_)) => true,
            (Passes::Int32, LeafRef::Number(number)) => is_int32(number),
            (Passes::Finite, LeafRef::Number(number)) => number.is_finite(),
            _ => false,
        };
        if passes
            && rule.type_at.is_none_or(|at| {
                matches!(return_type, Type::Scalar(scalar)
                    if SharedType::as_ptr(scalar).addr() == at)
            })
        {
            return Pass::Itself;
        }
        Pass::Serialize(Bound::Found(rule.rule.serializer(exe_context)))
    }
}

/// The positions of the five specified scalars in [`specified_scalar_types`],
/// the order of `specifiedScalarTypes`, `String` first.
const SPECIFIED_STRING: usize = 0;
/// The position of `Int` in [`specified_scalar_types`].
const SPECIFIED_INT: usize = 1;
/// The position of `Float` in [`specified_scalar_types`].
const SPECIFIED_FLOAT: usize = 2;
/// The position of `Boolean` in [`specified_scalar_types`].
const SPECIFIED_BOOLEAN: usize = 3;
/// The position of `ID` in [`specified_scalar_types`].
const SPECIFIED_ID: usize = 4;

/// Answers whether `number` is an integer of 32 bits, `Number.isInteger(num)`
/// with `num` from -2^31 to 2^31 - 1, the two tests of the `serialize` of `Int`.
///
/// The cast saturates at the bounds of `i32` and answers 0 for `NaN`, so the
/// number comes back unchanged exactly when it is an integer in the range,
/// `-0` among them, which `Number.isInteger` accepts.
fn is_int32(number: f64) -> bool {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::float_cmp,
        reason = "the saturating cast and the exact comparison are the two tests of the clause"
    )]
    let is_int32 = f64::from(number as i32) == number;
    is_int32
}

/// Returns the name of the value of the enum `return_type` whose internal
/// value is the string of the WTF-8 bytes `text`.
///
/// This is the lookup of `serialize` of an enum for a string, whose
/// `SameValueZero` with another string is the equality of their code units,
/// which is the equality of their WTF-8 bytes, run from the back as
/// [`aosabi_type::EnumType::serialize`] runs it, and none where no value has
/// that internal value or the values cannot be read, which the serializer then
/// answers with its error.
fn enum_name<'a>(return_type: &'a Type, text: &[u8]) -> Option<&'a str> {
    let Type::Enum(enumeration) = return_type else {
        return None;
    };
    enumeration
        .get_values()
        .ok()?
        .iter()
        .rev()
        .find(
            |value| matches!(&value.value, JsValue::String(internal) if internal.as_wtf8() == text),
        )
        .map(|value| value.name.as_str())
}

/// A schema as the requests that run against it share it, the type of
/// [`ExecutionArgs::schema`].
///
/// A caller that keeps a schema for the requests it runs later names its
/// handle through this alias, so a change of the shared pointer changes the
/// alias alone.
pub type SharedSchema = Arc<Schema>;

/// The handle of a [`SharedSchema`] that keeps no schema alive, as a plan names its schema.
///
/// A change of the shared pointer of [`SharedSchema`] changes this alias
/// with it.
pub type WeakSchema = std::sync::Weak<Schema>;

/// A parsed document as the requests of its text share it, the type of
/// [`ExecutionArgs::document`].
///
/// A caller that keeps a document for the requests of its text names its
/// handle through this alias, so a change of the shared pointer changes the
/// alias alone.
pub type SharedDocument = Arc<DocumentNode>;

/// What holds the raw variable values of an execution, the parameters of the
/// request they came in or the values alone.
///
/// The reference hands the executor the object `params.variables` of the
/// request, which the context of the request and every hook read too. The
/// arguments share the holder by `Rc`, so a copy of the arguments copies no
/// value.
pub trait VariableValues {
    /// Returns the raw variable values, `undefined` where the request holds none.
    fn values(&self) -> &JsValue;

    /// Returns the values by value where `self` is their only handle.
    ///
    /// # Errors
    ///
    /// Returns `self` where another handle shares the values, or where the
    /// holder keeps them for its other readers.
    fn take(self: Rc<Self>) -> Result<JsValue, SharedVariables>;
}

/// The raw variable values of an execution as its arguments share them, the
/// type of [`ExecutionArgs::variable_values`].
pub type SharedVariables = Rc<dyn VariableValues>;

/// A value of the variables that its holder keeps for no other reader.
impl VariableValues for JsValue {
    fn values(&self) -> &JsValue {
        self
    }

    fn take(self: Rc<Self>) -> Result<JsValue, SharedVariables> {
        Rc::try_unwrap(self).map_err(|shared| -> SharedVariables { shared })
    }
}

/// The arguments of [`execute`], the `ExecutionArgs` of the reference.
///
/// The `signal` and the `schemaCoordinateInErrors` of the reference are left
/// out with the branches that read them.
pub struct ExecutionArgs {
    /// The schema the operation runs against.
    pub schema: SharedSchema,
    /// The parsed document.
    pub document: SharedDocument,
    /// The value the root fields are resolved on.
    pub root_value: Value,
    /// The context value every resolver receives.
    pub context_value: ContextValue,
    /// The raw variable values of the request, shared with what holds them.
    pub variable_values: Option<SharedVariables>,
    /// The name of the operation to run.
    pub operation_name: Option<String>,
    /// The resolver of a field without one, [`default_field_resolver`] when `None`.
    pub field_resolver: Option<FieldResolverFn>,
    /// The resolver of an abstract type without one, [`default_type_resolver`] when `None`.
    pub type_resolver: Option<TypeResolverFn>,
    /// The subscribe resolver of a field without one, [`default_field_resolver`] when `None`.
    pub subscribe_field_resolver: Option<FieldResolverFn>,
    /// The serializers that see the executor value, by scalar name.
    pub leaf_serializers: Option<Arc<ObjMap<LeafSerializer>>>,
}

impl ExecutionArgs {
    /// Returns the raw variable values, `args.variableValues`.
    #[must_use]
    pub fn variables(&self) -> Option<&JsValue> {
        self.variable_values.as_deref().map(VariableValues::values)
    }
}

/// The nodes an execution keeps for itself, by the address of the operation
/// or the field that leads to each and of its runtime type.
///
/// The plan holds every operation and every field such an entry names, and
/// the schema of the context every runtime type, so both addresses stay
/// taken for as long as the execution reads the entry.
type LocalNodes = IndexMap<(usize, usize), Rc<PlanNode>, KeyHasherBuilder>;

/// Data that must be available at all points during query execution.
///
/// Namely, schema of the type system that is currently executing, and the
/// fragments defined in the query document.
pub struct ExecutionContext {
    /// The schema the operation runs against.
    pub schema: Arc<Schema>,
    /// The fragment definitions of the document by name.
    pub fragments: Rc<ObjMap<Rc<FragmentDefinitionNode>>>,
    /// The value the root fields are resolved on.
    pub root_value: Value,
    /// The context value every resolver receives.
    pub context_value: ContextValue,
    /// The operation that runs.
    pub operation: Rc<OperationDefinitionNode>,
    /// The coerced variable values.
    pub variable_values: Rc<JsObject>,
    /// The resolver of a field without one.
    pub field_resolver: FieldResolverFn,
    /// Whether a field without a resolver of its own reads the member of its
    /// name, which it does where the resolver of the context is
    /// [`default_field_resolver`] (see [`resolve_member`]).
    pub(crate) reads_members: bool,
    /// The resolver of an abstract type without one.
    pub type_resolver: TypeResolverFn,
    /// The subscribe resolver of a field without one.
    pub subscribe_field_resolver: FieldResolverFn,
    /// The serializers that see the executor value, by scalar name.
    pub leaf_serializers: Option<Arc<ObjMap<LeafSerializer>>>,
    /// The rules of the leaves of the six known kinds, read from `leaf_serializers`.
    pub(crate) leaf_rules: LeafRules,
    /// The errors of the fields handled so far, in the order they were handled.
    pub errors: RefCell<Vec<GraphQLError>>,
    /// The field plan of the document under the schema, kept with the document or made for the request.
    pub(crate) plan: Rc<Plan>,
    /// The nodes of the selections the plan does not keep, which the
    /// execution collects for its request and reads again from here.
    pub(crate) local: RefCell<LocalNodes>,
    /// The address of the operation in its document, which keys its root node in the plan.
    pub(crate) operation_key: usize,
    /// The arena the execution writes its answer into.
    pub(crate) arena: RefCell<Arena>,
    /// The fields of the objects of the execution whose result is a promise,
    /// with their keys, a stack that every object shares.
    ///
    /// An object notes the height of the stack when its fields start, adds
    /// each field whose result is a promise, and takes the fields above that
    /// height when its fields end, so a nested object completed in between
    /// has taken its own already. So the promised fields of an object wait
    /// in no list of their own, and the stack grows by doubling, as the
    /// vectors of the arena do (see [`promise_for_object`]).
    pub(crate) pending: RefCell<Vec<(Key, Promise)>>,
}

impl ExecutionContext {
    /// Returns the node the execution keeps for itself under `key`.
    pub(crate) fn local_node(&self, key: (usize, usize)) -> Option<Rc<PlanNode>> {
        self.local.borrow().get(&key).map(Rc::clone)
    }

    /// Keeps `node` for the execution under `key`.
    pub(crate) fn keep_local_node(&self, key: (usize, usize), node: Rc<PlanNode>) {
        self.local.borrow_mut().insert(key, node);
    }

    /// Returns a new empty object of the answer.
    fn new_object(&self) -> NodeRef {
        self.arena.borrow_mut().object()
    }

    /// Returns the height of the stack of the promised fields, where the fields of an object start.
    fn pending_mark(&self) -> usize {
        self.pending.borrow().len()
    }

    /// Adds the field of the key `key` whose result is `promise` to the object whose fields run.
    fn push_pending(&self, key: Key, promise: Promise) {
        self.pending.borrow_mut().push((key, promise));
    }

    /// Returns whether a field of the object whose fields start at `mark` is promised.
    fn has_pending(&self, mark: usize) -> bool {
        self.pending.borrow().len() > mark
    }

    /// Returns the encoded key `encoded`, `"key":`, copied into the arena.
    pub(crate) fn key(&self, encoded: &str) -> Key {
        self.arena.borrow_mut().key(encoded)
    }

    /// Appends the member `key` with the completed value `value` to `object`.
    pub(crate) fn append(&self, object: NodeRef, key: Key, value: Value) {
        self.arena.borrow_mut().append(object, key, value);
    }

    /// Appends the member `encoded`, `"key":`, with the completed value `value` to `object`.
    // Always inlined, as `append_leaf` is, into the loop of `execute_fields` and
    // the sink of a shape source.
    #[expect(clippy::inline_always)]
    #[inline(always)]
    fn put(&self, object: NodeRef, encoded: &str, value: Value) {
        self.arena.borrow_mut().put(object, encoded, value);
    }

    /// Answers whether `field` is a `__typename` whose value the execution
    /// writes from the constant of its plan (see
    /// [`ExecutionContext::put_type_name`]).
    fn writes_type_name(&self, field: &PlanField) -> bool {
        field.is_type_name() && self.leaf_rules.passes_type_name()
    }

    /// Appends the member `encoded`, `"key":`, with the value of
    /// `__typename` on an object of the node `node` to `object`.
    ///
    /// The reference resolves the meta field to the name of the parent type and
    /// completes it as the specified `String`, whose serializer answers the name
    /// as itself where [`ExecutionContext::writes_type_name`] holds. The parent
    /// type is the type of the node, so the value is the literal the plan holds
    /// for it (see [`PlanNode::typename`]), written with no call of the
    /// resolver, no string, no info and no path. The resolver reads nothing else
    /// and throws nothing, and the field settles at once, so its member takes
    /// the place in the collected order the reference gives it.
    fn put_type_name(&self, (object, encoded): (NodeRef, &str), node: &PlanNode) {
        self.arena
            .borrow_mut()
            .put_literal(object, encoded, node.typename());
    }

    /// Returns the completed leaf `value` as a node of the answer.
    fn leaf(&self, value: Value) -> Value {
        Value::from(self.arena.borrow_mut().leaf(value))
    }

    /// Returns the string `text` as a node of the answer, its escaped literal among the bytes.
    fn text(&self, text: &str) -> Value {
        Value::from(self.arena.borrow_mut().text(text))
    }

    /// Returns the date `date` as a node of the answer, the text of its `toJSON`.
    fn date(&self, date: JsDate) -> Value {
        Value::from(self.arena.borrow_mut().date(date))
    }

    /// Returns the leaf `leaf` its serializer answered as a node of the
    /// answer, a value of the JSON model moved in as it is.
    fn serialized(&self, leaf: Serialized) -> Value {
        let mut arena = self.arena.borrow_mut();
        Value::from(match leaf {
            Serialized::Value(value) => arena.leaf(value),
            Serialized::Json(value) => arena.json(value),
        })
    }

    /// Appends the member `encoded` with the leaf `leaf` its serializer answered to `object`.
    fn put_serialized(&self, (object, encoded): (NodeRef, &str), leaf: Serialized) {
        let mut arena = self.arena.borrow_mut();
        match leaf {
            Serialized::Value(value) => arena.put_leaf(object, encoded, value),
            Serialized::Json(value) => arena.put_json(object, encoded, value),
        }
    }

    /// Completes `member`, a leaf of the leaf type `named` of the kind
    /// `kind`, and appends it to `object` under the encoded key `encoded`,
    /// `"key":`.
    ///
    /// This is [`complete_leaf`] and the append of its node, for a member
    /// that is no error, no null and no `undefined`, whose completion at a
    /// type `T!` is the completion at `T`, which is no null (see
    /// [`complete_value`]). A leaf that passes as itself, as the name of an
    /// enum value, as a date or as a value of the JSON model pushes its node
    /// and its entry in one borrow of the arena.
    ///
    /// # Errors
    ///
    /// Returns what the serializer of the leaf throws, as [`complete_leaf`] does.
    // Always inlined into the two copies of `execute_member`, the loop of
    // `execute_fields` and the sink of a shape source, where a call costs every
    // leaf of a data member its arguments.
    #[expect(clippy::inline_always)]
    #[inline(always)]
    fn append_leaf(
        &self,
        (object, encoded): (NodeRef, &str),
        kind: Option<LeafKind>,
        named: &Type,
        member: Value,
    ) -> Result<(), Value> {
        let leaf = match self
            .leaf_rules
            .pass(self, kind, named, LeafRef::of(&member))
        {
            Pass::Itself => member,
            Pass::Name(name) => {
                self.arena.borrow_mut().put_text(object, encoded, name);
                return Ok(());
            }
            Pass::Date(date) => {
                self.arena.borrow_mut().put_date(object, encoded, date);
                return Ok(());
            }
            Pass::Json => {
                let json = member.to_js();
                self.arena.borrow_mut().put_json(object, encoded, json);
                return Ok(());
            }
            Pass::Serialize(bound) => {
                let leaf = complete_leaf_through(self, named, bound, &member)?;
                self.put_serialized((object, encoded), leaf);
                return Ok(());
            }
        };
        self.arena.borrow_mut().put_leaf(object, encoded, leaf);
        Ok(())
    }

    /// Completes `text`, a string member of the leaf type `named` of the
    /// kind `kind`, and appends it to `object` under the encoded key
    /// `encoded`, `"key":`.
    ///
    /// This is [`ExecutionContext::append_leaf`] of the string of `text`,
    /// which a `&str` holds with no copy: a string that passes as itself is
    /// escaped from `text` into the bytes of the arena, which writes the
    /// bytes the escape of the same string writes (see [`Arena::text`]), and
    /// a string that is the internal value of an enum value appends its
    /// name. Every other string completes through
    /// [`ExecutionContext::append_text_through`], and only a string its
    /// serializer completes is copied into a string of the executor, the
    /// value the serializer reads.
    ///
    /// # Errors
    ///
    /// Returns what the serializer of the leaf throws, as [`complete_leaf`] does.
    // Always inlined into the sink of a shape source, where a call costs every
    // string member of a derived struct about 40 instructions.
    #[expect(clippy::inline_always)]
    #[inline(always)]
    fn append_text(
        &self,
        (object, encoded): (NodeRef, &str),
        kind: Option<LeafKind>,
        named: &Type,
        text: &str,
    ) -> Result<(), Value> {
        let text = match self
            .leaf_rules
            .pass(self, kind, named, LeafRef::Text(text.as_bytes()))
        {
            // A string is the value of the JSON model of itself.
            Pass::Itself | Pass::Json => text,
            Pass::Name(name) => name,
            pass @ (Pass::Date(_) | Pass::Serialize(_)) => {
                return self.append_text_through((object, encoded), named, pass, text);
            }
        };
        self.arena.borrow_mut().put_text(object, encoded, text);
        Ok(())
    }

    /// Completes `text`, a string member of the leaf type `named`, for
    /// [`ExecutionContext::append_text`] through its serializer, which reads
    /// a copy of `text` as a string of the executor, while a string that
    /// passes as itself or as a name costs `append_text` no call. The other
    /// arms of `pass` only keep the match whole: the rules of a leaf answer
    /// a date for a date alone and never for a string, and `append_text`
    /// appends a string that passes as itself or as a name before it calls.
    ///
    /// # Errors
    ///
    /// Returns what the serializer of the leaf throws, as [`complete_leaf`] does.
    // Kept out of line, so the copy of `append_text` in the sink of a shape
    // source holds the string that passes as itself or as a name alone.
    #[cold]
    #[inline(never)]
    fn append_text_through(
        &self,
        (object, encoded): (NodeRef, &str),
        named: &Type,
        pass: Pass<'_>,
        text: &str,
    ) -> Result<(), Value> {
        match pass {
            Pass::Serialize(bound) => {
                let leaf = complete_leaf_through(self, named, bound, &Value::from(text))?;
                self.put_serialized((object, encoded), leaf);
            }
            Pass::Date(date) => self.arena.borrow_mut().put_date(object, encoded, date),
            // A string is the value of the JSON model of itself.
            Pass::Itself | Pass::Json => self.arena.borrow_mut().put_text(object, encoded, text),
            Pass::Name(name) => self.arena.borrow_mut().put_text(object, encoded, name),
        }
        Ok(())
    }
}

/// Returns the leaf type of a field of the type `return_type`, `T` of `T!` or
/// of `T`, where `T` is a scalar or an enum, and none for a list, an object,
/// an interface or a union.
fn leaf_of(return_type: &Type) -> Option<&Type> {
    let named = match return_type {
        Type::NonNull(non_null) => non_null.of_type(),
        other => other,
    };
    matches!(named, Type::Scalar(_) | Type::Enum(_)).then_some(named)
}

/// The fourth argument of a resolver, the state of the execution at a field.
///
/// The `signal` member has no reading, because the server hands the executor no abort
/// signal.
///
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
pub struct ResolveInfo {
    /// The name of the field definition.
    pub field_name: String,
    /// The field nodes of the response key.
    pub field_nodes: FieldNodes,
    /// The type of the field definition.
    pub return_type: Type,
    /// The object type the field is selected on.
    pub parent_type: Arc<ObjectType>,
    /// The path that leads to the field.
    pub path: Path,
    /// The schema the operation runs against.
    pub schema: Arc<Schema>,
    /// The fragment definitions of the document by name.
    pub fragments: Rc<ObjMap<Rc<FragmentDefinitionNode>>>,
    /// The root value of the operation.
    pub root_value: Value,
    /// The operation that runs.
    pub operation: Rc<OperationDefinitionNode>,
    /// The coerced variable values.
    pub variable_values: Rc<JsObject>,
}

/// Implements the "Executing requests" section of the GraphQL specification.
///
/// Returns the result object, `{ errors?, data? }`, or a promise of it. If the
/// arguments to this function do not result in a legal execution context, a
/// result with only errors is returned. Each early error gains
/// `extensions.http.status` 400 after the extensions it has, inside an `http`
/// member that keeps its other keys.
///
/// # Errors
///
/// Returns the thrown value when the schema is invalid, which
/// `assertValidSchema` throws.
pub fn execute(agent: &Agent, args: ExecutionArgs) -> Result<MaybePromise, Value> {
    // If a valid execution context cannot be created due to incorrect arguments,
    // a "Response" with only errors is returned.
    let exe_context = match build_execution_context(args)? {
        Ok(exe_context) => Rc::new(exe_context),
        // Return early errors if execution context failed.
        Err(errors) => {
            let errors: Vec<Value> = errors.into_iter().map(with_bad_request_status).collect();
            let entries = vec![(String::from("errors"), Value::from(errors))];
            return Ok(Value::from(PlainObject::from_entries(entries)));
        }
    };
    execute_impl(agent, &exe_context)
}

/// Returns `error` as a value, with `http.status` 400 set in its extensions.
///
/// A spread keeps the position of a key that exists and appends one that does
/// not, which is what an insert into a [`JsObject`] does. An `http` member that
/// is no object spreads no key.
fn with_bad_request_status(mut error: GraphQLError) -> Value {
    let mut http = match error.extensions.get("http") {
        Some(JsValue::Object(http)) => http.clone(),
        _ => JsObject::new(),
    };
    http.insert("status", JsValue::Number(400.0));
    error.extensions.insert("http", JsValue::Object(http));
    Value::from(ErrorObject::GraphQL(error))
}

/// Runs the operation of `exe_context` and builds the response of what it settles to.
///
/// If errors are encountered while executing a GraphQL field, only that field
/// and its descendants will be omitted, and sibling fields will still be
/// executed. An execution which encounters errors will still result in a
/// resolved promise. Errors from sub-fields of a `NonNull` type may propagate to
/// the top level, at which point we still log the error and null the parent
/// field, which in this case is the entire response.
pub(crate) fn execute_impl(
    agent: &Agent,
    exe_context: &Rc<ExecutionContext>,
) -> Result<MaybePromise, Value> {
    let success_context = Rc::clone(exe_context);
    let error_context = Rc::clone(exe_context);
    handle_maybe_promise(
        agent,
        |agent| execute_operation(agent, exe_context).into(),
        Box::new(move |_, data| Completion::Normal(build_response(data, &success_context))),
        Some(Box::new(move |_, error| {
            let mut errors = error_context.errors.borrow_mut();
            match thrown_errors(&error) {
                Some(thrown) => errors.extend(thrown.into_iter().map(into_graphql_error)),
                None => errors.push(into_graphql_error(error)),
            }
            drop(errors);
            Completion::Normal(build_response(Value::Null, &error_context))
        })),
    )
    .into_result()
}

/// Returns the `errors` member of a thrown value when it is truthy, which an `AggregateError` has.
fn thrown_errors(error: &Value) -> Option<Vec<Value>> {
    match error {
        Value::Error(error) => match error.as_ref() {
            ErrorObject::Aggregate { errors, .. } => Some(errors.clone()),
            _ => None,
        },
        Value::Object(object) => match Rc::clone(object).get("errors") {
            Value::List(errors) => Some(errors.to_vec()),
            _ => None,
        },
        _ => None,
    }
}

/// Given a completed execution context and data, build the `{ errors, data }`
/// response defined by the "Response" section of the GraphQL specification.
///
/// The errors move out of the list of the context. The reference shares the
/// array, so an error a field pushes after the response exists still shows in
/// it. Here such an error stays on the context, and no fixture orders an error
/// that late. The arena moves out of the context with the node of `data`, the
/// completed root object or `null`, into the result object, which holds the data
/// in the arena until a reader takes it (see [`crate::arena`]).
#[must_use]
pub fn build_response(data: Value, exe_context: &ExecutionContext) -> Value {
    let errors = errors_value(exe_context.errors.take());
    let mut arena = exe_context.arena.take();
    let root = arena.place(data);
    Value::Object(Rc::new(ResultObject::of_execution(errors, arena, root)))
}

/// Returns the early error of an operation that cannot be resolved.
fn operation_resolution_failure(message: String) -> Vec<GraphQLError> {
    let mut extensions = JsObject::new();
    extensions.insert("code", JsValue::from("OPERATION_RESOLUTION_FAILURE"));
    vec![create_graphql_error(
        message,
        GraphQLErrorOptions {
            extensions: Some(extensions),
            ..GraphQLErrorOptions::default()
        },
    )]
}

/// Returns the operation of `document` that `operation_name` names, or the one it holds.
///
/// The `Err` is the early error of an operation that cannot be resolved. With an
/// operation name the last operation of that name is the one that runs, because
/// the loop of the reference has no `break`.
fn find_operation<'a>(
    document: &'a DocumentNode,
    operation_name: Option<&str>,
) -> Result<&'a OperationDefinitionNode, Vec<GraphQLError>> {
    let mut operation: Option<&OperationDefinitionNode> = None;
    for definition in &document.definitions {
        // A definition of any other kind is no executable definition.
        let DefinitionNode::OperationDefinition(definition) = definition else {
            continue;
        };
        let Some(operation_name) = operation_name else {
            if operation.is_some() {
                return Err(operation_resolution_failure(String::from(
                    "Must provide operation name if query contains multiple operations.",
                )));
            }
            operation = Some(definition);
            continue;
        };
        let name = definition.name.as_ref().map(NameNode::value);
        if name == Some(operation_name) {
            operation = Some(definition);
        }
    }
    if let Some(operation) = operation {
        return Ok(operation);
    }
    if let Some(operation_name) = operation_name {
        return Err(operation_resolution_failure(format!(
            "Unknown operation named \"{operation_name}\"."
        )));
    }
    Err(operation_resolution_failure(String::from(
        "Must provide an operation.",
    )))
}

/// Constructs a `ExecutionContext` object from the arguments passed to
/// execute, which we will pass throughout the other execution methods.
///
/// The inner `Err` is the array of errors the reference returns in place of a
/// context. The fragment map and the field plan come from the kept document
/// where a keeper keeps the document, and are made for the request otherwise
/// (see [`crate::plan`]).
///
/// # Errors
///
/// Returns the thrown value when the schema is invalid, which
/// `assertValidSchema` throws.
pub fn build_execution_context(
    args: ExecutionArgs,
) -> Result<Result<ExecutionContext, Vec<GraphQLError>>, Value> {
    let schema = args.schema;
    let document = args.document;
    let operation_name = args.operation_name;
    // If the schema used for execution is invalid, throw an error.
    assert_valid_schema(&schema).map_err(thrown_definition_error)?;
    let (fragments, plan) = plan_for(&document, &schema);
    let operation = match find_operation(&document, operation_name.as_deref()) {
        Ok(operation) => operation,
        Err(errors) => return Ok(Err(errors)),
    };
    let operation_key = core::ptr::from_ref(operation).addr();
    let variable_definitions = operation.variable_definitions.as_deref().unwrap_or(&[]);
    // `rawVariableValues ?? {}`. A value that is no object has no own
    // property a variable name reads, so it coerces like an empty object.
    // Values the arguments hold alone are coerced where they stand, and
    // values another reader shares are read where they stand.
    let (owned, shared) = match args.variable_values.map(VariableValues::take) {
        Some(Ok(JsValue::Object(inputs))) => (Some(inputs), None),
        Some(Err(shared)) => (None, Some(shared)),
        _ => (None, None),
    };
    let inputs = match (owned, shared.as_deref().map(VariableValues::values)) {
        (Some(inputs), _) => Cow::Owned(inputs),
        (None, Some(JsValue::Object(inputs))) => Cow::Borrowed(inputs),
        _ => Cow::Owned(JsObject::new()),
    };
    let coerced = match get_variable_values(&schema, variable_definitions, inputs, Some(50)) {
        CoercedVariableValues::Errors(errors) => return Ok(Err(errors)),
        CoercedVariableValues::Coerced(coerced) => coerced,
    };
    let default_field: FieldResolverFn = Arc::new(default_field_resolver);
    let default_type: TypeResolverFn = Arc::new(default_type_resolver);
    Ok(Ok(ExecutionContext {
        fragments,
        root_value: args.root_value,
        context_value: args.context_value,
        operation: Rc::new(operation.clone()),
        variable_values: Rc::new(coerced),
        reads_members: args.field_resolver.is_none(),
        field_resolver: args
            .field_resolver
            .unwrap_or_else(|| Arc::clone(&default_field)),
        type_resolver: args.type_resolver.unwrap_or(default_type),
        subscribe_field_resolver: args.subscribe_field_resolver.unwrap_or(default_field),
        leaf_rules: LeafRules::new(args.leaf_serializers.as_deref()),
        leaf_serializers: args.leaf_serializers,
        errors: RefCell::new(Vec::new()),
        plan,
        local: RefCell::default(),
        operation_key,
        arena: RefCell::new(Arena::new()),
        pending: RefCell::default(),
        schema,
    }))
}

/// Returns the root type of `operation`, which the schema has to define.
///
/// A root type is an object type in a valid schema, and the context is built
/// over a valid one, so a root type of another kind reads as none.
fn get_defined_root_type(
    schema: &Schema,
    operation: &OperationDefinitionNode,
) -> Result<Arc<ObjectType>, Value> {
    let root_type = schema
        .get_root_type(operation.operation)
        .and_then(NamedType::as_object);
    let Some(root_type) = root_type else {
        let error = create_graphql_error(
            format!(
                "Schema is not configured to execute {} operation.",
                operation.operation.as_str()
            ),
            GraphQLErrorOptions {
                nodes: Some(vec![AstNode::from(operation.clone())]),
                ..GraphQLErrorOptions::default()
            },
        );
        return Err(Value::from(ErrorObject::GraphQL(error)));
    };
    Ok(Arc::clone(root_type))
}

/// Implements the "Executing operations" section of the spec.
///
/// The check at `execute.js:262-266` builds an error and drops it, after
/// `getDefinedRootType` has thrown the same one, so it has no reading. The root
/// fields are the root node of the plan, collected on the first execution of the
/// operation and read by every later one.
fn execute_operation(agent: &Agent, exe_context: &Rc<ExecutionContext>) -> Result<Value, Value> {
    let operation = &exe_context.operation;
    let root_type = get_defined_root_type(&exe_context.schema, operation)?;
    exe_context.plan.enter();
    let root_fields = exe_context
        .plan
        .root(exe_context, &root_type)
        .map_err(thrown_definition_error)?;
    let root_value = &exe_context.root_value;
    if operation.operation == OperationTypeNode::Mutation {
        execute_fields_serially(agent, exe_context, &root_type, root_value, &root_fields)
    } else {
        execute_fields(
            agent,
            exe_context,
            &root_type,
            root_value,
            None,
            &root_fields,
        )
    }
}

/// Implements the "Executing selection sets" section of the spec
/// for fields that must be executed serially.
///
/// The fields are those of `fields`, a node of the plan, and they are the root
/// fields of a mutation, so the path of each starts at its own key.
fn execute_fields_serially(
    agent: &Agent,
    exe_context: &Rc<ExecutionContext>,
    parent_type: &Arc<ObjectType>,
    source_value: &Value,
    fields: &PlanNode,
) -> Result<Value, Value> {
    let results = exe_context.new_object();
    let fields: Vec<(String, Key, Rc<PlanField>)> = fields
        .fields()
        .iter()
        .map(|field| {
            let encoded_key = fields.encoded_key(field);
            let key = exe_context.key(encoded_key);
            (String::from(encoded_key), key, Rc::clone(field))
        })
        .collect();
    let exe_context = Rc::clone(exe_context);
    let parent_type = Arc::clone(parent_type);
    let source_value = source_value.clone();
    let callback_fn: ReduceCallback<(String, Key, Rc<PlanField>)> =
        Rc::new(move |agent, accumulated, (encoded_key, key, field)| {
            let field_path = PathFrame::key(None, &encoded_key, &parent_type.name);
            let context = Rc::clone(&exe_context);
            handle_maybe_promise(
                agent,
                |agent| {
                    execute_field(
                        agent,
                        &exe_context,
                        &parent_type,
                        &source_value,
                        &field,
                        &field_path,
                    )
                    .into()
                },
                Box::new(move |_, result| {
                    if !matches!(result, Value::Undefined) {
                        context.append(results, key, result);
                    }
                    Completion::Normal(accumulated)
                }),
                None,
            )
        });
    promise_reduce(agent, fields, &callback_fn, Value::from(results))
}

/// Implements the "Executing selection sets" section of the spec
/// for fields that may be executed in parallel.
///
/// A field that throws while a sibling is a promise waits for the siblings
/// through [`promise_for_object`] and throws its own error whatever they settle
/// to, where graphql-js uses `finally`. The fields are those of `fields`, a node
/// of the plan, in collected order.
///
/// The results object is an object of the arena. A field that completes at
/// once is appended to it in collected order, and a promised field waits on
/// the stack of the promised fields of the context with its key until the
/// loop ends, as the results object of the reference holds it until
/// `promiseForObject` registers its reaction, which appends it when it
/// settles.
///
/// A field of the default resolver with no argument on an object source, a
/// data member, runs in the loop itself through [`execute_member`], which
/// reads the member and appends a leaf of a known kind straight to the
/// results object, so a member that completes as a leaf enters no resolver
/// and no test of its result for a promise. A `__typename` whose value the
/// plan holds is appended from that constant through
/// [`ExecutionContext::put_type_name`]. A member that is a function or a
/// promise, and every other field, completes as [`execute_field`] completes
/// it.
///
/// An object with a static shape writes the data members of the selection by
/// their slots, which the node of the plan holds for the shape, through one call
/// of [`ObjectLike::write_members`], into a [`MemberSink`] that completes each
/// field in collected order with the member the object wrote for it, a string
/// with no copy before its escape into the arena (see `execute_shape`). Every
/// other object runs the loop of this function, so the order of the fields, of
/// the resolver calls and of the jobs is the collected order either way.
fn execute_fields(
    agent: &Agent,
    exe_context: &Rc<ExecutionContext>,
    parent_type: &Arc<ObjectType>,
    source_value: &Value,
    path: Option<&PathFrame<'_>>,
    fields: &PlanNode,
) -> Result<Value, Value> {
    // The default resolver reads a member of an object alone.
    let members = match source_value {
        Value::Object(object) if exe_context.reads_members => Some(object),
        _ => None,
    };
    if let Some(object) = members
        && let Some(shape) = object.shape()
        && let Some(slots) = fields.slots(&exe_context.plan, parent_type, shape)
    {
        return execute_shape(
            agent,
            exe_context,
            parent_type,
            source_value,
            object,
            path,
            fields,
            &slots,
        );
    }
    let results = exe_context.new_object();
    let mark = exe_context.pending_mark();
    for field in fields.fields() {
        let encoded_key = fields.encoded_key(field);
        let field_path = PathFrame::key(path, encoded_key, &parent_type.name);
        let result = match members {
            Some(object) if is_data_member(field) => {
                let member = execute_member(
                    agent,
                    exe_context,
                    parent_type,
                    Read::Get(object),
                    field,
                    &field_path,
                    (results, encoded_key),
                );
                match member {
                    Ok(Member::Appended) => continue,
                    Ok(Member::Completed(result)) => Ok(result),
                    Err(error) => Err(error),
                }
            }
            _ if exe_context.writes_type_name(field) => {
                exe_context.put_type_name((results, encoded_key), fields);
                continue;
            }
            _ => execute_field(
                agent,
                exe_context,
                parent_type,
                source_value,
                field,
                &field_path,
            ),
        };
        let result = match result {
            Ok(result) => result,
            Err(error) if exe_context.has_pending(mark) => {
                return rethrow_after_siblings(agent, exe_context, results, mark, error);
            }
            Err(error) => return Err(error),
        };
        match result {
            Value::Undefined => {}
            Value::Promise(promise) => {
                exe_context.push_pending(exe_context.key(encoded_key), promise);
            }
            result => exe_context.put(results, encoded_key, result),
        }
    }
    settle_object(agent, exe_context, results, mark)
}

/// Returns the results object of a selection whose fields all completed,
/// or the promise of it where the stack of the promised fields holds fields
/// of it above `mark`.
///
/// The end of `executeFields` at `execute.js:330-337`.
fn settle_object(
    agent: &Agent,
    exe_context: &Rc<ExecutionContext>,
    results: NodeRef,
    mark: usize,
) -> Result<Value, Value> {
    // If there are no promises, we can just return the object
    if !exe_context.has_pending(mark) {
        return Ok(Value::from(results));
    }

    // Otherwise, results is a map from field name to the result of resolving that
    // field, which is possibly a promise. Return a promise that will return this
    // same map, but with any promises replaced with the values they resolved to.
    promise_for_object(agent, exe_context, results, mark).into_result()
}

/// Executes the fields of `fields` on `object`, a shape source whose data
/// members `slots` names, the slot of each field in collected order.
///
/// The object writes its members into a [`MemberSink`] through one call of
/// [`ObjectLike::write_members`], and the sink completes each field with the
/// member written for it and every field the object leaves.
#[expect(
    clippy::too_many_arguments,
    reason = "the arguments of execute_fields with the object and its slots"
)]
fn execute_shape(
    agent: &Agent,
    exe_context: &Rc<ExecutionContext>,
    parent_type: &SharedType<ObjectType>,
    source_value: &Value,
    object: &Rc<dyn ObjectLike>,
    path: Option<&PathFrame<'_>>,
    fields: &PlanNode,
    slots: &[MemberSlot],
) -> Result<Value, Value> {
    let mut sink = MemberSink {
        agent,
        exe_context,
        parent_type,
        source_value,
        object,
        path,
        fields,
        results: exe_context.new_object(),
        mark: exe_context.pending_mark(),
        next: 0,
        failed: None,
    };
    Rc::clone(object).write_members(slots, &mut sink);
    sink.finish()
}

/// The writer of the members of one shape source of a selection, which
/// completes the fields of the selection in collected order.
///
/// `execute_fields` makes one per object with a static shape. The object
/// hands it the data members of the selection through
/// [`ObjectLike::write_members`], one call per field in collected order:
/// [`MemberSink::str`] for a string the object holds, [`MemberSink::value`]
/// for any other member, and [`MemberSink::field`] for a field that reads no
/// data member of the shape, which the sink completes as the loop of
/// `execute_fields` completes a field of any object. Each call completes the
/// next field of the selection, and a call after a field threw, or after the
/// last field, does nothing. The executor then completes every field the
/// object did not reach, so a selection is complete whatever the object
/// writes.
pub struct MemberSink<'a> {
    /// The agent of the request.
    agent: &'a Agent,
    /// The context of the execution.
    exe_context: &'a Rc<ExecutionContext>,
    /// The object type the selection is collected on.
    parent_type: &'a SharedType<ObjectType>,
    /// The value the fields are resolved on, the object as a value.
    source_value: &'a Value,
    /// The object whose members the default resolver reads.
    object: &'a Rc<dyn ObjectLike>,
    /// The frame of the field whose value the object is, `None` at the root.
    path: Option<&'a PathFrame<'a>>,
    /// The node of the selection.
    fields: &'a PlanNode,
    /// The results object of the selection in the arena.
    results: NodeRef,
    /// The height of the stack of the promised fields where the fields of the object start.
    mark: usize,
    /// The position of the next field the sink completes.
    next: usize,
    /// The error a field threw, after which no field completes.
    failed: Option<Value>,
}

impl<'a> MemberSink<'a> {
    /// Returns the next field the sink completes, and moves past it, or
    /// `None` after the last field and after a field threw.
    fn take(&mut self) -> Option<&'a Rc<PlanField>> {
        if self.failed.is_some() {
            return None;
        }
        let field = self.fields.fields().get(self.next)?;
        self.next += 1;
        Some(field)
    }

    /// Completes the next field with the string member `text`.
    ///
    /// The field reads a data member of the shape of the object, which holds
    /// `text`. A string of a leaf type that passes as itself is escaped from
    /// `text` into the arena with no copy before, as
    /// `ExecutionContext::append_text` writes it, and every other
    /// completion reads the string as the value the object answers for it.
    #[inline]
    pub fn str(&mut self, text: &str) {
        self.member(Read::Text(text));
    }

    /// Completes the next field with the member `value`, the value the
    /// object answers for it through [`ObjectLike::get`].
    #[inline]
    pub fn value(&mut self, value: Value) {
        self.member(Read::Value(value));
    }

    /// Completes the next field with the value of the JSON model `value`, a
    /// member the object holds.
    ///
    /// A value a leaf of `JSON` or `JSONObject` takes as itself, every value
    /// under the hook of `JSON` and an object under the serializer of
    /// `JSONObject` that Aosabi binds, is written into the arena from where it
    /// stands (see [`Arena::kept`]), with no value of the executor made of it
    /// and no copy of it, and every other completion reads the value as the
    /// value [`Value::from_js`] makes of it, the value the object answers for
    /// it through [`ObjectLike::get`].
    #[inline]
    pub fn json(&mut self, value: &JsValue) {
        self.member(Read::Kept(Kept::Value(value)));
    }

    /// Completes the next field with the object of the JSON model `object`,
    /// a member the object holds, as [`MemberSink::json`] completes a value.
    #[inline]
    pub fn json_object(&mut self, object: &JsObject) {
        self.member(Read::Kept(Kept::Object(object)));
    }

    /// Completes the next field as the loop of `execute_fields` completes a
    /// field of an object with no shape, for a field that reads no data
    /// member of the shape.
    pub fn field(&mut self) {
        if self
            .fields
            .fields()
            .get(self.next)
            .is_some_and(|field| is_data_member(field))
        {
            self.member(Read::Get(self.object));
            return;
        }
        let Some(field) = self.take() else {
            return;
        };
        let (fields, parent_type) = (self.fields, self.parent_type);
        let encoded_key = fields.encoded_key(field);
        if self.exe_context.writes_type_name(field) {
            self.exe_context
                .put_type_name((self.results, encoded_key), fields);
            return;
        }
        let field_path = PathFrame::key(self.path, encoded_key, &parent_type.name);
        let result = execute_field(
            self.agent,
            self.exe_context,
            parent_type,
            self.source_value,
            field,
            &field_path,
        );
        self.settle(encoded_key, result);
    }

    /// Completes the next field, a data member of the object, with the member `read`.
    fn member(&mut self, read: Read<'_>) {
        let Some(field) = self.take() else {
            return;
        };
        let (fields, parent_type) = (self.fields, self.parent_type);
        let encoded_key = fields.encoded_key(field);
        let field_path = PathFrame::key(self.path, encoded_key, &parent_type.name);
        let member = execute_member(
            self.agent,
            self.exe_context,
            parent_type,
            read,
            field,
            &field_path,
            (self.results, encoded_key),
        );
        let result = match member {
            Ok(Member::Appended) => return,
            Ok(Member::Completed(result)) => Ok(result),
            Err(error) => Err(error),
        };
        self.settle(encoded_key, result);
    }

    /// Takes the result of the field of the encoded key `encoded_key` into
    /// the results object, or into the promised fields, or keeps the error
    /// it threw.
    fn settle(&mut self, encoded_key: &str, result: Result<Value, Value>) {
        match result {
            Ok(Value::Undefined) => {}
            Ok(Value::Promise(promise)) => {
                let key = self.exe_context.key(encoded_key);
                self.exe_context.push_pending(key, promise);
            }
            Ok(result) => self.exe_context.put(self.results, encoded_key, result),
            Err(error) => self.failed = Some(error),
        }
    }

    /// Completes the fields the object did not reach and returns the
    /// results object, or the promise of it.
    fn finish(mut self) -> Result<Value, Value> {
        while self.failed.is_none() && self.next < self.fields.fields().len() {
            self.field();
        }
        let (agent, exe_context) = (self.agent, self.exe_context);
        match self.failed {
            Some(error) if !exe_context.has_pending(self.mark) => Err(error),
            Some(error) => {
                rethrow_after_siblings(agent, exe_context, self.results, self.mark, error)
            }
            None => settle_object(agent, exe_context, self.results, self.mark),
        }
    }
}

/// Returns a promise that rejects with `error` once every promised field above `mark` is handled.
///
/// It ensures that any promises returned by other fields are handled, as they
/// may also reject. The members of `results` settle into an object no reader
/// reaches.
fn rethrow_after_siblings(
    agent: &Agent,
    exe_context: &Rc<ExecutionContext>,
    results: NodeRef,
    mark: usize,
    error: Value,
) -> Result<Value, Value> {
    let rejected = error.clone();
    handle_maybe_promise(
        agent,
        |agent| promise_for_object(agent, exe_context, results, mark),
        Box::new(move |_, _| Completion::Throw(error)),
        Some(Box::new(move |_, _| Completion::Throw(rejected))),
    )
    .into_result()
}

/// Implements the "Executing fields" section of the spec.
///
/// In particular, this function figures out the value that the field returns by
/// calling its resolve function, then calls `complete_value` to complete
/// promises, serialize scalars, or execute the sub-selection-set for objects.
/// `field` is the field of the plan, which holds the field nodes and where the
/// definition of the field stands, so the lookup of `getFieldDef` ran once, when
/// the plan was built. A field the parent type does not define answers
/// `undefined`, which leaves its key out of the result.
///
/// `path` is the frame of the field on the stack. The `info` the reference
/// builds for every field at `execute.js:353` is a [`FieldInfo`], which
/// builds it on its first read, so a field of the default resolver whose
/// member is no function builds no info, no arguments and no path (see
/// `resolve_member`).
///
/// # Errors
///
/// Returns the located error of a field of a non null type, and what
/// `getFields` of the parent type throws.
pub fn execute_field(
    agent: &Agent,
    exe_context: &Rc<ExecutionContext>,
    parent_type: &Arc<ObjectType>,
    source: &Value,
    field: &Rc<PlanField>,
    path: &PathFrame<'_>,
) -> Result<Value, Value> {
    let field_nodes = field.field_nodes();
    if field_nodes.is_empty() {
        return Ok(Value::Undefined);
    }
    let field_def = field
        .field_def(parent_type)
        .map_err(thrown_definition_error)?;
    let Some(field_def) = field_def else {
        return Ok(Value::Undefined);
    };

    let return_type = &field_def.r#type;
    let info = FieldInfo::new(field_def, parent_type, field_nodes, path);

    // Get the resolve function, regardless of if its result is normal or abrupt (error).
    let completed = resolve_and_complete(agent, exe_context, field, field_def, source, &info, path);
    completed.or_else(|raw_error| {
        handle_raw_error(exe_context, raw_error, return_type, field_nodes, path)
    })
}

/// The `info` of the field a value completes for, built on its first read.
///
/// The reference builds the info of every field before it calls the resolver and
/// hands that one object to the resolver, to `resolveType` and to `isTypeOf` of
/// the field's value. A resolver function reads its info, so its field builds it
/// before the call. An introspection resolver reads the schema, the parent type
/// and the coerced arguments where they stand, so its field builds no info for
/// the call. A field of the default resolver reads the name of the field alone
/// unless its member is a function. A field of either of the two builds its info
/// on the first read, by the call of such a member, by `resolveType`, by
/// `isTypeOf` or by a promise, and every later read takes the same info. A
/// reaction of a promise holds the info it was made with.
pub struct FieldInfo<'a>(InfoSite<'a>);

/// Where the info of a [`FieldInfo`] comes from.
enum InfoSite<'a> {
    /// The info a resolver received or a reaction holds.
    Built(&'a Rc<ResolveInfo>),
    /// The field the info is built of, and the info once a reader built it.
    Unread {
        /// The field definition.
        field_def: &'a Field,
        /// The object type the field is selected on.
        parent_type: &'a SharedType<ObjectType>,
        /// The field nodes of the response key.
        field_nodes: &'a FieldNodes,
        /// The frame of the field.
        path: &'a PathFrame<'a>,
        /// The info, once built.
        info: OnceCell<Rc<ResolveInfo>>,
    },
}

impl<'a> FieldInfo<'a> {
    /// Returns the field whose info `info` is.
    #[must_use]
    pub const fn built(info: &'a Rc<ResolveInfo>) -> Self {
        Self(InfoSite::Built(info))
    }

    /// Returns the field of `field_def` selected on `parent_type` at `path`,
    /// whose info is built on its first read.
    #[must_use]
    pub const fn new(
        field_def: &'a Field,
        parent_type: &'a SharedType<ObjectType>,
        field_nodes: &'a FieldNodes,
        path: &'a PathFrame<'a>,
    ) -> Self {
        Self(InfoSite::Unread {
            field_def,
            parent_type,
            field_nodes,
            path,
            info: OnceCell::new(),
        })
    }

    /// Returns the name of the field definition, `info.fieldName`.
    #[must_use]
    pub fn field_name(&self) -> &str {
        match &self.0 {
            InfoSite::Built(info) => &info.field_name,
            InfoSite::Unread { field_def, .. } => &field_def.name,
        }
    }

    /// Returns the name of the object type the field is selected on, `info.parentType.name`.
    #[must_use]
    pub fn parent_type_name(&self) -> &str {
        &self.parent_type().name
    }

    /// Returns the object type the field is selected on, `info.parentType`.
    #[must_use]
    pub fn parent_type(&self) -> &SharedType<ObjectType> {
        match &self.0 {
            InfoSite::Built(info) => &info.parent_type,
            InfoSite::Unread { parent_type, .. } => parent_type,
        }
    }

    /// Returns the info of the field, built by [`build_resolve_info`] on the first read.
    #[must_use]
    pub fn info(&self, exe_context: &ExecutionContext) -> &Rc<ResolveInfo> {
        match &self.0 {
            InfoSite::Built(info) => info,
            InfoSite::Unread {
                field_def,
                parent_type,
                field_nodes,
                path,
                info,
            } => info.get_or_init(|| {
                Rc::new(build_resolve_info(
                    exe_context,
                    field_def,
                    field_nodes,
                    parent_type,
                    &path.path(),
                ))
            }),
        }
    }
}

/// Resolves the field of `info` on `source` and completes what the resolver returns.
///
/// A promise the resolver returns is completed in one reaction, and a promise
/// the completion returns gets its rejection handler in a second one, so the
/// error of a rejecting resolver is handled two jobs after the rejection. The
/// resolver is the one the plan read from the definition,
/// `fieldDef.resolve ?? exeContext.fieldResolver` at `execute.js:352`, and it is
/// borrowed, so a field calls it with no count of a reference. A field with no
/// resolver of its own under the default resolver of the context reads its
/// member through [`resolve_member`], which builds the info and the arguments
/// for a member that is a function alone. An introspection resolver reads the
/// schema of the execution, the parent type and the coerced arguments where they
/// stand alone, so its field builds no info, no path and no object of its
/// arguments, and a field whose definition holds no argument coerces none, which
/// coerce to an empty object that throws nothing (see
/// [`resolve_introspection_field`]). Every other field builds both before the
/// call. A promise takes its reaction with the path of `path` made and the info
/// built, so the jobs and their order stay those of the reference.
fn resolve_and_complete(
    agent: &Agent,
    exe_context: &Rc<ExecutionContext>,
    field: &Rc<PlanField>,
    field_def: &Field,
    source: &Value,
    info: &FieldInfo<'_>,
    path: &PathFrame<'_>,
) -> Result<Value, Value> {
    let Some(first_node) = field.field_nodes().first() else {
        return Ok(Value::Undefined);
    };
    let result = match field.resolver() {
        ResolverKind::Default if exe_context.reads_members => resolve_member(
            agent,
            exe_context,
            field,
            field_def,
            first_node,
            source,
            info,
        )?,
        ResolverKind::Introspection(resolver) => {
            let args = coerced_arguments(exe_context, field, field_def, first_node)?;
            resolve_introspection_field(
                *resolver,
                source,
                args.as_deref(),
                &exe_context.schema,
                info.parent_type(),
            )
            .into_result()?
        }
        ResolverKind::Default => {
            let resolve = &exe_context.field_resolver;
            call_resolver(
                agent,
                exe_context,
                field,
                field_def,
                first_node,
                source,
                info,
                resolve,
            )?
        }
        ResolverKind::Function(resolve) => call_resolver(
            agent,
            exe_context,
            field,
            field_def,
            first_node,
            source,
            info,
            resolve,
        )?,
    };
    complete_resolved(
        agent,
        exe_context,
        field,
        &field_def.r#type,
        info,
        path,
        result,
    )
}

/// Calls `resolve`, the resolver function of `field` or the resolver of the
/// context, with the arguments, the context value and the info of the field,
/// as `executeField` calls it at `execute.js:355-362`.
///
/// # Errors
///
/// Returns the error the coercion of the arguments throws, and what the
/// resolver throws.
#[expect(
    clippy::too_many_arguments,
    reason = "the call of the reference names the field, its definition, its node, the source, its info and the resolver"
)]
fn call_resolver(
    agent: &Agent,
    exe_context: &ExecutionContext,
    field: &PlanField,
    field_def: &Field,
    first_node: &FieldNode,
    source: &Value,
    info: &FieldInfo<'_>,
    resolve: &FieldResolverFn,
) -> Result<Value, Value> {
    // Build a JS object of arguments from the field.arguments AST, using the
    // variables scope to fulfill any variable references.
    let args = arguments_of(exe_context, field, field_def, first_node)?;

    // The resolve function's optional third argument is a context value that
    // is provided to every resolve function within an execution. It is commonly
    // used to represent an authenticated user, or request-specific caches.
    let context_value = &exe_context.context_value;
    let info = info.info(exe_context);
    resolve(agent, source, &args, context_value, info).into_result()
}

/// Completes `result`, what the resolver of `field` returned, for `return_type` at `path`.
///
/// A promise is completed in one reaction with the path of `path` made and the
/// info built, and a promise the completion returns gets its rejection handler
/// in a second one.
///
/// # Errors
///
/// Returns what [`complete_value`] throws.
fn complete_resolved(
    agent: &Agent,
    exe_context: &Rc<ExecutionContext>,
    field: &Rc<PlanField>,
    return_type: &Type,
    info: &FieldInfo<'_>,
    path: &PathFrame<'_>,
    result: Value,
) -> Result<Value, Value> {
    let completed = if let Value::Promise(result) = &result {
        then_complete_value(
            agent,
            result,
            exe_context,
            return_type,
            field,
            info.info(exe_context),
            path.path(),
        )
    } else {
        complete_value(agent, exe_context, return_type, field, info, path, result)?
    };

    let Value::Promise(completed) = &completed else {
        return Ok(completed);
    };
    // Note: we don't rely on a `catch` method, but we do expect "thenable"
    // to take a second callback for the error case.
    let handler = Continuation::FieldError {
        exe_context: Rc::clone(exe_context),
        return_type: return_type.clone(),
        field: Rc::clone(field),
        path: path.path(),
    };
    Ok(Value::Promise(
        completed.then_with(agent, Handlers::Execute(handler)),
    ))
}

/// Resolves a field with no resolver of its own under the default resolver of the context.
///
/// The resolver of the context where [`ExecutionContext::field_resolver`] is
/// [`default_field_resolver`]. It reads the member of the source the field names
/// and hands `args`, the context value and `info` to a member that is a
/// function, and it reads `info.fieldName` alone otherwise, which is the name of
/// `field_def`. So a member of any other kind is read with no info and no
/// arguments built, and the info and the arguments are built for a function
/// alone, with the values the reference hands it. The arguments of a field whose
/// coercion can throw are coerced before the member is read, as the reference
/// coerces them before it calls the resolver, and a field with no argument or
/// whose kept arguments cannot throw makes its copy of them for a function
/// alone.
///
/// # Errors
///
/// Returns the error the coercion of the arguments throws, and what a
/// member that is a function throws.
fn resolve_member(
    agent: &Agent,
    exe_context: &ExecutionContext,
    field: &PlanField,
    field_def: &Field,
    first_node: &FieldNode,
    source: &Value,
    info: &FieldInfo<'_>,
) -> Result<Value, Value> {
    let coerced = if coerces_arguments(field) {
        Some(arguments_of(exe_context, field, field_def, first_node)?)
    } else {
        None
    };
    // ensure source is a value for which property access is acceptable.
    let Value::Object(source) = source else {
        return Ok(Value::Undefined);
    };
    let property = Rc::clone(source).get(&field_def.name);
    let Value::Function(property) = &property else {
        return Ok(property);
    };
    let args = match coerced {
        Some(args) => args,
        None => arguments_of(exe_context, field, field_def, first_node)?,
    };
    call_member(agent, exe_context, property, args, info)
}

/// Calls `function`, the member a field of the default resolver read, with
/// `args`, the context value and the info of the field, as
/// `defaultFieldResolver` calls it at `execute.js:810`.
///
/// # Errors
///
/// Returns what the function throws.
fn call_member(
    agent: &Agent,
    exe_context: &ExecutionContext,
    function: &Function,
    args: Rc<PlainObject>,
    info: &FieldInfo<'_>,
) -> Result<Value, Value> {
    let arguments = [
        Value::from(args),
        Value::Host(HostValue::Context(Rc::clone(&exe_context.context_value))),
        Value::Host(HostValue::Info(Rc::clone(info.info(exe_context)))),
    ];
    function.call(agent, &arguments).into_result()
}

/// Answers whether `field` is a data member, a field of the default resolver
/// with no argument that its parent type defines, which [`execute_member`]
/// runs where the resolver of the context reads members.
fn is_data_member(field: &PlanField) -> bool {
    field.is_data_member()
}

/// What a data member answers to the loop of its selection.
enum Member {
    /// Its leaf, appended to the results object already.
    Appended,
    /// Its completed value, which the loop appends or awaits as any field's.
    Completed(Value),
}

/// Where [`execute_member`] reads the member of a data member from.
enum Read<'r> {
    /// The member of the field's name, read from the object through [`ObjectLike::get`].
    Get(&'r Rc<dyn ObjectLike>),
    /// A string the object holds, which the member writer of its shape hands over.
    Text(&'r str),
    /// A value of the JSON model the object holds, which the member writer of its shape hands over.
    Kept(Kept<'r>),
    /// The value the member writer of its shape read, the value [`ObjectLike::get`] answers.
    Value(Value),
}

/// Executes the data member `field` of an object in the loop of its selection, with the
/// member `read` names.
///
/// This is [`execute_field`] with the resolver of the context, which reads
/// the member of the field's name (see [`resolve_member`]), for a field with
/// no argument, so no arguments are coerced and none can throw. A member that
/// is a function is called with the arguments, the context value and the
/// info, and a promise takes the reaction of [`complete_resolved`], as the
/// reference runs them. A leaf of a known kind whose serializer answers it as
/// itself is appended to the results object `appended.0` under its encoded
/// key `appended.1` with its node in one step (see [`LeafRules`]), and every
/// other value completes through [`complete_resolved`]. An error is handled
/// at the field, as `executeField` handles it.
///
/// The member is read through [`ObjectLike::get`] after the definition of
/// the field is found, or it is the member the writer of the shape of the
/// object read by its slot, a read with no side effect that answers the
/// value the read through `get` answers. A string the object holds is no
/// function, no promise, no nullish value and no error, so it completes as
/// a leaf from the `&str` itself (see [`ExecutionContext::append_text`]) or
/// as the string of the executor it is.
///
/// # Errors
///
/// Returns the located error of a field of a non null type, and what
/// `getFields` of the parent type throws.
// Always inlined, into the loop of `execute_fields` and into the sink of a shape
// source, where a call costs every data member the passing of its eight
// arguments.
#[expect(clippy::inline_always)]
#[inline(always)]
fn execute_member(
    agent: &Agent,
    exe_context: &Rc<ExecutionContext>,
    parent_type: &SharedType<ObjectType>,
    read: Read<'_>,
    field: &Rc<PlanField>,
    path: &PathFrame<'_>,
    appended: (NodeRef, &str),
) -> Result<Member, Value> {
    let field_nodes = field.field_nodes();
    let field_def = field
        .field_def(parent_type)
        .map_err(thrown_definition_error)?;
    let Some(field_def) = field_def else {
        return Ok(Member::Completed(Value::Undefined));
    };
    let return_type = &field_def.r#type;
    let info = FieldInfo::new(field_def, parent_type, field_nodes, path);
    let read = match read {
        Read::Get(source) => Rc::clone(source).get(&field_def.name),
        Read::Value(value) => value,
        Read::Text(text) => {
            let completed = match leaf_of(return_type) {
                Some(named) => match exe_context.append_text(appended, field.leaf(), named, text) {
                    Ok(()) => return Ok(Member::Appended),
                    Err(raw_error) => Err(raw_error),
                },
                None => complete_resolved(
                    agent,
                    exe_context,
                    field,
                    return_type,
                    &info,
                    path,
                    Value::from(text),
                ),
            };
            return completed
                .or_else(|raw_error| {
                    handle_raw_error(exe_context, raw_error, return_type, field_nodes, path)
                })
                .map(Member::Completed);
        }
        Read::Kept(kept) => {
            // A value a `JSON` or a `JSONObject` leaf takes as itself is
            // written from where it stands (see `LeafRules::keeps`), and every
            // other value completes as the value of the executor it is.
            if !kept.is_nullish()
                && leaf_of(return_type).is_some()
                && exe_context.leaf_rules.keeps(field.leaf(), kept)
            {
                let (object, encoded) = appended;
                exe_context
                    .arena
                    .borrow_mut()
                    .put_kept(object, encoded, kept);
                return Ok(Member::Appended);
            }
            kept.to_value()
        }
    };
    let member = match read {
        Value::Function(function) => {
            let args = PlainObject::new();
            call_member(agent, exe_context, &function, args, &info)
        }
        Value::Promise(promise) => Ok(Value::Promise(promise)),
        member if member.is_nullish() || member.is_error() => Ok(member),
        member => match leaf_of(return_type) {
            Some(named) => {
                return match exe_context.append_leaf(appended, field.leaf(), named, member) {
                    Ok(()) => Ok(Member::Appended),
                    Err(raw_error) => {
                        handle_raw_error(exe_context, raw_error, return_type, field_nodes, path)
                            .map(Member::Completed)
                    }
                };
            }
            None => Ok(member),
        },
    };
    member
        .and_then(|member| {
            complete_resolved(agent, exe_context, field, return_type, &info, path, member)
        })
        .or_else(|raw_error| {
            handle_raw_error(exe_context, raw_error, return_type, field_nodes, path)
        })
        .map(Member::Completed)
}

/// Answers whether a call of `field` coerces its arguments, which can throw.
///
/// A definition with no argument takes an empty object, and the kept
/// arguments of a field whose first coercion succeeded take a copy, so
/// neither throws. The first call of a field whose arguments are literals,
/// a field whose first coercion failed and a field whose arguments read a
/// variable coerce, which can throw (see [`arguments_of`]).
fn coerces_arguments(field: &PlanField) -> bool {
    match field.args() {
        ArgsKind::None => false,
        ArgsKind::Literal(kept) => !matches!(kept.get(), Some(Some(_))),
        ArgsKind::Variables => true,
    }
}

/// Returns the promise of what `promise` resolves to, completed for `return_type` at `path`.
///
/// The field nodes are those of `field`, the field of the plan, which `info`
/// shares. The reaction holds the info and the path, which outlive the stack
/// frame of the field.
fn then_complete_value(
    agent: &Agent,
    promise: &Promise,
    exe_context: &Rc<ExecutionContext>,
    return_type: &Type,
    field: &Rc<PlanField>,
    info: &Rc<ResolveInfo>,
    path: Path,
) -> Value {
    let handler = Continuation::Complete {
        exe_context: Rc::clone(exe_context),
        return_type: return_type.clone(),
        field: Rc::clone(field),
        info: Rc::clone(info),
        path,
    };
    Value::Promise(promise.then_with(agent, Handlers::Execute(handler)))
}

/// A reaction of the executor on the promise of a field or of a list item, by its kind.
///
/// Each kind holds in place the state the closure of the reference captures, so
/// a reaction of the executor and the job that runs it allocate nothing. The
/// completion of a value handles the fulfilment and passes a rejection through,
/// and the two error handlers handle the rejection and pass a fulfilment
/// through, as the `then` of the reference with one of its two handlers does.
pub(crate) enum Continuation {
    /// `resolved => completeValue(..)` of a field at `execute.js:368` and of
    /// a list item at `:625`, see [`then_complete_value`].
    Complete {
        /// The context of the execution.
        exe_context: Rc<ExecutionContext>,
        /// The type the value completes for, of the field or of the item.
        return_type: Type,
        /// The field of the plan, whose nodes `info` shares.
        field: Rc<PlanField>,
        /// The info of the field.
        info: Rc<ResolveInfo>,
        /// The path of the field or of the item.
        path: Path,
    },
    /// The rejection handler of a field at `execute.js:376-391`, see [`complete_resolved`].
    FieldError {
        /// The context of the execution.
        exe_context: Rc<ExecutionContext>,
        /// The return type of the field.
        return_type: Type,
        /// The field of the plan, whose nodes locate the error.
        field: Rc<PlanField>,
        /// The path of the field.
        path: Path,
    },
    /// The rejection handler of a list item at `execute.js:633-639`, see
    /// [`complete_list_item_value`].
    ItemError {
        /// The context of the execution.
        exe_context: Rc<ExecutionContext>,
        /// The type of the item.
        item_type: Type,
        /// The field of the plan of the list, whose nodes locate the error.
        field: Rc<PlanField>,
        /// The path of the item.
        path: Path,
    },
}

impl Continuation {
    /// Returns whether the reaction handles a rejection, and a fulfilment otherwise.
    pub(crate) fn handles_rejection(&self) -> bool {
        !matches!(self, Self::Complete { .. })
    }

    /// Runs the reaction with the value or the reason `argument` it handles.
    pub(crate) fn run(self, agent: &Agent, argument: Value) -> Completion {
        match self {
            Self::Complete {
                exe_context,
                return_type,
                field,
                info,
                path,
            } => complete_value(
                agent,
                &exe_context,
                &return_type,
                &field,
                &FieldInfo::built(&info),
                &PathFrame::held(path),
                argument,
            )
            .into(),
            Self::FieldError {
                exe_context,
                return_type,
                field,
                path,
            } => {
                let path = PathFrame::held(path);
                handle_raw_error(
                    &exe_context,
                    argument,
                    &return_type,
                    field.field_nodes(),
                    &path,
                )
                .into()
            }
            Self::ItemError {
                exe_context,
                item_type,
                field,
                path,
            } => handle_raw_item_error(
                &exe_context,
                argument,
                &item_type,
                field.field_nodes(),
                &PathFrame::held(path),
            )
            .into(),
        }
    }
}

/// Returns the `args` object of a call of the resolver of `field`.
///
/// A definition with no argument answers an empty object at once, and the
/// arguments of a field whose argument values read no variable are coerced once
/// and kept by the plan, so a later call takes a fresh object of their copy, as
/// every call of the reference takes a fresh object. A field whose argument
/// values read a variable, and a field whose first coercion failed, coerce the
/// arguments at each call.
///
/// # Errors
///
/// Returns the error `getArgumentValues` throws, as a thrown value.
fn arguments_of(
    exe_context: &ExecutionContext,
    field: &PlanField,
    field_def: &Field,
    first_node: &FieldNode,
) -> Result<Rc<PlainObject>, Value> {
    Ok(
        coerced_arguments(exe_context, field, field_def, first_node)?
            .map_or_else(PlainObject::new, |args| arguments_object(args.into_owned())),
    )
}

/// Returns the coerced arguments of a call of `field`, none for a definition with no argument.
///
/// The coercion is the one of [`arguments_of`], and the arguments the plan
/// keeps are borrowed from it, so a reader that reads them where they stand,
/// an introspection resolver, copies none, and [`arguments_of`] copies them
/// into the fresh object a resolver receives.
///
/// # Errors
///
/// Returns the error `getArgumentValues` throws, as a thrown value.
fn coerced_arguments<'a>(
    exe_context: &ExecutionContext,
    field: &'a PlanField,
    field_def: &Field,
    first_node: &FieldNode,
) -> Result<Option<Cow<'a, JsObject>>, Value> {
    let args = match field.kept_args() {
        KeptArgs::None => return Ok(None),
        KeptArgs::Kept(args) => Cow::Borrowed(args),
        KeptArgs::First => {
            let coerced = get_argument_values(field_def, first_node, &exe_context.variable_values);
            field.keep_args(&exe_context.plan, coerced.as_ref().ok());
            Cow::Owned(coerced.map_err(|error| Value::from(ErrorObject::GraphQL(error)))?)
        }
        KeptArgs::Coerce => Cow::Owned(
            get_argument_values(field_def, first_node, &exe_context.variable_values)
                .map_err(|error| Value::from(ErrorObject::GraphQL(error)))?,
        ),
    };
    Ok(Some(args))
}

/// Returns the `args` object a resolver receives, with the keys in definition order.
///
/// The coerced arguments are consumed, so a name and a value move into the
/// object and no string of them is copied. A name keeps its text, with the
/// replacement character for a lone surrogate. A field with no argument,
/// most fields of an answer, takes an empty object at once, and the entries
/// of any other take the room of its arguments and no more.
pub(crate) fn arguments_object(args: JsObject) -> Rc<PlainObject> {
    if args.is_empty() {
        return PlainObject::new();
    }
    let mut entries = Vec::with_capacity(args.len());
    for (name, value) in args {
        let name = name
            .into_string()
            .unwrap_or_else(|name| to_string_lossy(&name).into_owned());
        entries.push((name, Value::from_js_owned(value)));
    }
    PlainObject::from_entries(entries)
}

/// Handles what a field threw or rejected with, which is null for a nullable field.
///
/// Each error of an `AggregateError` is located and handled on its own, in
/// order, and the result is that of the last one, which is `undefined` for an
/// aggregate without an error. [`coerce_error`] runs before the error is
/// located, so a thrown value that is no error never reaches `toError`. The path
/// of the error is the keys of the frame `path`, `pathToArray(path)`.
// Out of line, so an execution compiles the handling of a raw error where a field throws.
#[inline(never)]
fn handle_raw_error(
    exe_context: &ExecutionContext,
    raw_error: Value,
    return_type: &Type,
    field_nodes: &FieldNodes,
    path: &PathFrame<'_>,
) -> Result<Value, Value> {
    let errors = &exe_context.errors;
    if let Value::Error(aggregate) = &raw_error
        && let ErrorObject::Aggregate {
            errors: raw_errors, ..
        } = aggregate.as_ref()
    {
        let mut result = Value::Undefined;
        for raw_error_item in raw_errors {
            let raw_error_item = coerce_error(raw_error_item.clone());
            let error = located_error(raw_error_item, field_nodes, path.to_array());
            result = handle_field_error(error, return_type, errors)?;
        }
        return Ok(result);
    }
    let coerced_error = coerce_error(raw_error);
    let error = located_error(coerced_error, field_nodes, path.to_array());
    handle_field_error(error, return_type, errors)
}

/// Returns the `info` argument of the resolver of `field_def`.
// Pointers valid for the request would skip the three counts.
#[must_use]
pub fn build_resolve_info(
    exe_context: &ExecutionContext,
    field_def: &Field,
    field_nodes: &FieldNodes,
    parent_type: &Arc<ObjectType>,
    path: &Path,
) -> ResolveInfo {
    // The resolve function's optional fourth argument is a collection of
    // information about the current execution state.
    ResolveInfo {
        field_name: field_def.name.clone(),
        field_nodes: Rc::clone(field_nodes),
        return_type: field_def.r#type.clone(),
        parent_type: Arc::clone(parent_type),
        path: path.clone(),
        schema: Arc::clone(&exe_context.schema),
        fragments: Rc::clone(&exe_context.fragments),
        root_value: exe_context.root_value.clone(),
        operation: Rc::clone(&exe_context.operation),
        variable_values: Rc::clone(&exe_context.variable_values),
    }
}

/// The key of the extension that makes an error propagate from a nullable position.
pub const CRITICAL_ERROR: &str = "CRITICAL_ERROR";

/// Adds `error` to `errors` and answers null, or throws it where null is no answer.
///
/// An error whose extensions carry a truthy [`CRITICAL_ERROR`] is thrown at a
/// nullable position too, which graphql-js has no rule for. Every handled error
/// is pushed, and an error below a path that already reported one is no
/// exception, neither here nor in graphql-js 16.12.0.
///
/// # Errors
///
/// Returns `error` as the thrown value for a non null `return_type` and for
/// a critical error.
pub fn handle_field_error(
    error: GraphQLError,
    return_type: &Type,
    errors: &RefCell<Vec<GraphQLError>>,
) -> Result<Value, Value> {
    // If the field type is non-nullable, then it is resolved without any
    // protection from errors, however it still properly locates the error.
    if is_non_null_type(return_type) {
        return Err(Value::from(ErrorObject::GraphQL(error)));
    }
    let critical = error.extensions.get(CRITICAL_ERROR);
    if critical.is_some_and(|critical| to_boolean(&Value::from_js(critical))) {
        return Err(Value::from(ErrorObject::GraphQL(error)));
    }
    // Otherwise, error protection is applied, logging the error and resolving
    // a null value for this field if one is encountered.
    errors.borrow_mut().push(error);
    Ok(Value::Null)
}

/// Returns whether `value` is truthy.
///
/// Translation of ECMA-262 section 7.1.2, `ToBoolean`: `undefined`, `null`,
/// `false`, `+0`, `-0`, `NaN` and the empty string are false, and every other
/// value is true.
pub(crate) fn to_boolean(value: &Value) -> bool {
    match value {
        Value::Undefined | Value::Null => false,
        Value::Bool(boolean) => *boolean,
        Value::Number(number) => *number != 0.0 && !number.is_nan(),
        Value::String(string) => !string.is_empty(),
        _ => true,
    }
}

/// Implements the instructions for completeValue as defined in the
/// "Value Completion" section of the spec.
///
/// If the field type is Non-Null, then this recursively completes the value
/// for the inner type. It throws a field error if that completion returns
/// null, as per the "Nullability" section of the spec.
///
/// If the field type is a List, then this recursively completes the value
/// for the inner type on each item in the list.
///
/// If the field type is a Scalar or Enum, ensures the completed value is a
/// legal value of the type by calling the `serialize` method of GraphQL type
/// definition.
///
/// If the field is an abstract type, determine the runtime type of the value
/// and then complete based on that type.
///
/// Otherwise, the field type expects a sub-selection set, and will complete the
/// value by executing all sub-selections. `field` is the field of the plan,
/// whose field nodes the reference passes as `fieldNodes`, `info` the info of
/// the field, which a reader builds on its first read, and `path` the frame of
/// the position the value completes at.
///
/// # Errors
///
/// Returns `result` when it is an error, the error of a null at a non null
/// type, and what the completion of the inner type throws.
pub fn complete_value(
    agent: &Agent,
    exe_context: &Rc<ExecutionContext>,
    return_type: &Type,
    field: &Rc<PlanField>,
    info: &FieldInfo<'_>,
    path: &PathFrame<'_>,
    result: Value,
) -> Result<Value, Value> {
    // If result is an Error, throw a located error.
    if result.is_error() {
        return Err(result);
    }

    // If field type is NonNull, complete for inner type, and throw field error
    // if result is null.
    if let Type::NonNull(return_type) = return_type {
        let completed = complete_value(
            agent,
            exe_context,
            return_type.of_type(),
            field,
            info,
            path,
            result,
        )?;
        if matches!(completed, Value::Null) {
            return Err(Value::from(ErrorObject::error(format!(
                "Cannot return null for non-nullable field {}.{}.",
                info.parent_type_name(),
                info.field_name()
            ))));
        }
        return Ok(completed);
    }

    // If result value is null or undefined then return null.
    if result.is_nullish() {
        return Ok(Value::Null);
    }

    // If field type is List, complete each item in the list with the inner type
    if is_list_type(return_type) {
        return complete_list_value(agent, exe_context, return_type, field, info, path, &result);
    }

    // If field type is a leaf type, Scalar or Enum, serialize to a valid value,
    // returning null if serialization is not possible.
    if is_leaf_type(return_type) {
        return complete_leaf(exe_context, return_type, field.leaf(), result);
    }

    // If field type is an abstract type, Interface or Union, determine the
    // runtime Object type and complete for that type.
    if is_abstract_type(return_type) {
        return complete_abstract_value(agent, exe_context, return_type, field, info, path, result);
    }

    // If field type is Object, execute and complete all sub-selections.
    if let Type::Object(return_type) = return_type {
        return complete_object_value(agent, exe_context, return_type, field, info, path, result);
    }
    // Not reachable, all possible output types have been considered. The
    // reference asserts on the console and answers `undefined`.
    Ok(Value::Undefined)
}

/// Complete a list value by completing each item in the list with the
/// inner type.
///
/// An array is the one iterable of the value model, so every other value is
/// refused.
///
/// # Errors
///
/// Returns the error of a value that is no list, and the located error of an
/// item of a non null type.
// Out of line, so an execution compiles the completion of a list where a field returns one.
#[inline(never)]
pub fn complete_list_value(
    agent: &Agent,
    exe_context: &Rc<ExecutionContext>,
    return_type: &Type,
    field: &Rc<PlanField>,
    info: &FieldInfo<'_>,
    path: &PathFrame<'_>,
    result: &Value,
) -> Result<Value, Value> {
    let Some(item_type) = return_type.of_type() else {
        return Ok(Value::Undefined);
    };

    let Value::List(result) = result else {
        let error = create_graphql_error(
            format!(
                "Expected Iterable, but did not find one for field \"{}.{}\".",
                info.parent_type_name(),
                info.field_name()
            ),
            GraphQLErrorOptions::default(),
        );
        return Err(Value::from(ErrorObject::GraphQL(error)));
    };

    // This is specified as a simple map, however we're optimizing the path
    // where the list contains no Promises by avoiding creating another Promise.
    let mark = exe_context.arena.borrow().items_mark();
    let mut completed_results: Option<Vec<Value>> = None;
    for (index, item) in result.iter().enumerate() {
        // No need to modify the info object containing the path,
        // since from here on it is not ever accessed by resolver functions.
        let item_path = PathFrame::index(path, index);
        let completed_item = complete_list_item_value(
            agent,
            item.clone(),
            exe_context,
            item_type,
            field,
            info,
            &item_path,
        );
        let completed_item = match completed_item {
            Ok(completed_item) => completed_item,
            Err(error) => {
                exe_context.arena.borrow_mut().drop_items(mark);
                return Err(error);
            }
        };
        match &mut completed_results {
            Some(completed_results) => completed_results.push(completed_item),
            None if is_promise(&completed_item) => {
                let mut values = Vec::with_capacity(result.len());
                exe_context.arena.borrow_mut().take_items(mark, &mut values);
                values.push(completed_item);
                completed_results = Some(values);
            }
            None => exe_context.arena.borrow_mut().push_item(completed_item),
        }
    }

    if let Some(completed_results) = completed_results {
        return Ok(Value::Promise(Promise::all(agent, completed_results)));
    }
    Ok(Value::from(exe_context.arena.borrow_mut().list(mark)))
}

/// Complete a list item value, which the list adds to its completed results.
///
/// Returns the completed item, a promise of it with its rejection handled, or
/// the `null` of a handled error. The `errors` parameter of the reference is the
/// list of the context, because the record of a deferred payload, the one other
/// list, is left out.
///
/// # Errors
///
/// Returns the located error of an item of a non null type.
pub fn complete_list_item_value(
    agent: &Agent,
    item: Value,
    exe_context: &Rc<ExecutionContext>,
    item_type: &Type,
    field: &Rc<PlanField>,
    info: &FieldInfo<'_>,
    item_path: &PathFrame<'_>,
) -> Result<Value, Value> {
    let field_nodes = field.field_nodes();
    let completed_item = if let Value::Promise(item) = &item {
        Ok(then_complete_value(
            agent,
            item,
            exe_context,
            item_type,
            field,
            info.info(exe_context),
            item_path.path(),
        ))
    } else {
        complete_value(agent, exe_context, item_type, field, info, item_path, item)
    };

    match completed_item {
        Ok(Value::Promise(completed_item)) => {
            // Note: we don't rely on a `catch` method, but we do expect "thenable"
            // to take a second callback for the error case.
            let handler = Continuation::ItemError {
                exe_context: Rc::clone(exe_context),
                item_type: item_type.clone(),
                field: Rc::clone(field),
                path: item_path.path(),
            };
            Ok(Value::Promise(
                completed_item.then_with(agent, Handlers::Execute(handler)),
            ))
        }
        Ok(completed_item) => Ok(completed_item),
        Err(raw_error) => {
            handle_raw_item_error(exe_context, raw_error, item_type, field_nodes, item_path)
        }
    }
}

/// Handles what a list item threw or rejected with, which is null for a nullable item.
///
/// An `AggregateError` gets no reading of its own here, where a field has one.
fn handle_raw_item_error(
    exe_context: &ExecutionContext,
    raw_error: Value,
    item_type: &Type,
    field_nodes: &FieldNodes,
    item_path: &PathFrame<'_>,
) -> Result<Value, Value> {
    let coerced_error = coerce_error(raw_error);
    let error = located_error(coerced_error, field_nodes, item_path.to_array());
    handle_field_error(error, item_type, &exe_context.errors)
}

/// Calls the serializer of the leaf type `return_type` on `result`.
///
/// This is the `returnType.serialize(result)` of `execute.js:658`. A scalar that
/// registered a [`LeafSerializer`] by its name is asked first. A specified
/// scalar and an enum serialize in the type layer, which reads the value the way
/// `serializeObject` does, through [`Value::to_specified_input`], except for a
/// function, which the value model of the type layer has no room for and which
/// [`serialize_function_leaf`] answers. Every other scalar keeps the
/// `identityFunc` a `GraphQLScalarType` takes for a missing `serialize`, so its
/// result is the value itself, a date included, and the response writer prints
/// it the way `JSON.stringify` does. The scalars the type layer serializes are
/// the ones `isSpecifiedScalarType` names, because a binding over a specified
/// scalar carries a leaf serializer beside the hook. `bound` is the serializer
/// of the request the rules of the execution found for a leaf of a known kind,
/// and any other leaf reads the map of the request by the name of its scalar.
/// The value of the JSON model a serializer answers is the leaf as it is, so a
/// string, a list or an object it answers moves into the arena with no copy and
/// no value of the executor made of it.
fn serialize_leaf_value(
    exe_context: &ExecutionContext,
    return_type: &Type,
    bound: Bound<'_>,
    result: &Value,
) -> Result<Serialized, Value> {
    match return_type {
        Type::Scalar(scalar) => {
            let leaf_serializer = match bound {
                Bound::Found(found) => found,
                Bound::ByName => exe_context
                    .leaf_serializers
                    .as_ref()
                    .and_then(|leaf_serializers| leaf_serializers.get(&scalar.name)),
            };
            if let Some(leaf_serializer) = leaf_serializer {
                return leaf_serializer(result).map(Serialized::Json);
            }
            if !is_specified_scalar_name(&scalar.name) {
                return Ok(Serialized::Value(result.clone()));
            }
            if let Value::Function(_) = result {
                return Err(serialize_function_leaf(&scalar.name, result));
            }
            if refuses_invalid_date(&scalar.name, result) {
                return Err(serialize_function_leaf(&scalar.name, result));
            }
            scalar
                .serialize(&result.to_specified_input())
                .map(Serialized::Json)
                .map_err(thrown_definition_error)
        }
        Type::Enum(enumeration) => {
            if let Value::Function(_) | Value::Date(_) = result {
                return Err(serialize_function_leaf(&enumeration.name, result));
            }
            enumeration
                .serialize(&result.to_specified_input())
                .map(Serialized::Json)
                .map_err(thrown_definition_error)
        }
        _ => Ok(Serialized::Value(Value::Undefined)),
    }
}

/// Returns the error a specified scalar or an enum throws for a function or for a date it refuses.
///
/// `serializeObject` returns a function as it is, because a function is not
/// object like, and no branch of the five serializers accepts one, so each
/// throws its last message over `inspect`, which prints `[function]` or
/// `[function name]`. An enum finds no value for a function in its lookup and
/// throws the same way, and so it does for a date, which is no internal value of
/// an enum, and so do `String` and `ID` for a date [`refuses_invalid_date`]
/// names. `inspect` prints a date through its `toJSON`, its ISO text or `null`
/// for an invalid date. `type_name` names the scalar or the enum.
fn serialize_function_leaf(type_name: &str, function: &Value) -> Value {
    let printed = inspect(function);
    let message = match type_name {
        "Int" => format!("Int cannot represent non-integer value: {printed}"),
        "Float" => format!("Float cannot represent non numeric value: {printed}"),
        "String" => format!("String cannot represent value: {printed}"),
        "Boolean" => format!("Boolean cannot represent a non boolean value: {printed}"),
        "ID" => format!("ID cannot represent value: {printed}"),
        name => format!("Enum \"{name}\" cannot represent value: {printed}"),
    };
    thrown_definition_error(DefinitionError::graphql(message))
}

/// Answers whether the specified scalar `type_name` refuses `result` as a date with no time value.
///
/// `serializeObject` reads a date through `valueOf`, its time value, which is
/// `NaN` for an invalid date. `String` and `ID` accept no `NaN` and print the
/// value they were handed in their message, the date, where the type layer,
/// which reads the time value alone, would print `NaN`. `Int`, `Float` and
/// `Boolean` print the coerced value, `NaN`, as the type layer does.
fn refuses_invalid_date(type_name: &str, result: &Value) -> bool {
    matches!(result, Value::Date(date) if !date.get_time().is_finite())
        && matches!(type_name, "String" | "ID")
}

/// Complete a Scalar or Enum by serializing to a valid value, returning
/// null if serialization is not possible.
///
/// A `GraphQLError` of the serializer is thrown again as a plain `Error` with
/// the same message, in order to be consistent with how non-null checks work
/// later on, so its extensions never reach the response. The values of the last
/// message print with `cross-inspect`.
///
/// # Errors
///
/// Returns what the serializer throws, and the error of a serializer that
/// answers null or undefined.
pub fn complete_leaf_value(
    exe_context: &ExecutionContext,
    return_type: &Type,
    result: &Value,
) -> Result<Value, Value> {
    complete_leaf_through(exe_context, return_type, Bound::ByName, result)
        .map(Serialized::into_value)
}

/// Completes the leaf `result` of the leaf type `return_type`, of the kind
/// `kind`, as a node of the answer.
///
/// A leaf of a known kind whose serializer answers it as itself goes straight
/// into the arena, a string escaped from the value the resolver returned, and so
/// does the name of an enum value, so neither calls a serializer nor copies the
/// leaf. A valid date whose serializer answers its `toISOString` goes into the
/// arena as its digits, and a list or an object whose serializer answers it as
/// itself as the value of the JSON model [`Value::to_js`] makes of it. Every
/// other leaf takes [`complete_leaf_value`] through the serializer the rules of
/// the execution found for its kind, with no search of the map of the request,
/// and the value of the JSON model the serializer answers moves into the arena
/// as it is.
///
/// # Errors
///
/// Returns what [`complete_leaf_value`] returns.
fn complete_leaf(
    exe_context: &ExecutionContext,
    return_type: &Type,
    kind: Option<LeafKind>,
    result: Value,
) -> Result<Value, Value> {
    match exe_context
        .leaf_rules
        .pass(exe_context, kind, return_type, LeafRef::of(&result))
    {
        Pass::Itself => Ok(exe_context.leaf(result)),
        Pass::Name(name) => Ok(exe_context.text(name)),
        Pass::Date(date) => Ok(exe_context.date(date)),
        Pass::Json => Ok(exe_context.serialized(Serialized::Json(result.to_js()))),
        Pass::Serialize(bound) => {
            let leaf = complete_leaf_through(exe_context, return_type, bound, &result)?;
            Ok(exe_context.serialized(leaf))
        }
    }
}

/// Completes the leaf `result` through `bound`, the body of [`complete_leaf_value`].
///
/// # Errors
///
/// Returns what [`complete_leaf_value`] returns.
fn complete_leaf_through(
    exe_context: &ExecutionContext,
    return_type: &Type,
    bound: Bound<'_>,
    result: &Value,
) -> Result<Serialized, Value> {
    let serialized_result = serialize_leaf_value(exe_context, return_type, bound, result).map_err(
        |err| match &err {
            Value::Error(error) => match error.as_ref() {
                ErrorObject::GraphQL(error) => {
                    Value::from(ErrorObject::error(error.message.clone()))
                }
                _ => err,
            },
            _ => err,
        },
    )?;
    if serialized_result.is_nullish() {
        return Err(Value::from(ErrorObject::error(format!(
            "Expected `{return_type}.serialize({})` to return non-nullable value, returned: {}",
            cross_inspect(result),
            cross_inspect(&serialized_result.into_value())
        ))));
    }
    Ok(serialized_result)
}

/// Returns the function the runtime type of `return_type` is resolved with.
///
/// This is `returnType.resolveType ?? exeContext.typeResolver` at
/// `execute.js:683`. A handle that holds another kind reads as no resolver.
/// The function is borrowed, as a field borrows its resolver from the plan.
fn resolve_type_fn_of<'a>(
    exe_context: &'a ExecutionContext,
    return_type: &'a NamedType,
) -> &'a TypeResolverFn {
    let handle = match return_type {
        NamedType::Interface(interface) => interface.resolve_type.as_ref(),
        NamedType::Union(union) => union.resolve_type.as_ref(),
        _ => None,
    };
    handle
        .and_then(|handle| handle.downcast_ref::<ResolveType>())
        .map_or(&exe_context.type_resolver, |resolve_type| &resolve_type.0)
}

/// Complete a value of an abstract type by determining the runtime object type
/// of that value, then complete the value for that type.
///
/// # Errors
///
/// Returns what the type resolver throws, the error of
/// [`ensure_valid_runtime_type`] and what [`complete_object_value`] throws.
pub fn complete_abstract_value(
    agent: &Agent,
    exe_context: &Rc<ExecutionContext>,
    return_type: &Type,
    field: &Rc<PlanField>,
    info: &FieldInfo<'_>,
    path: &PathFrame<'_>,
    result: Value,
) -> Result<Value, Value> {
    let Some(return_type) = return_type.as_named() else {
        return Ok(Value::Undefined);
    };
    let resolve_type_fn = resolve_type_fn_of(exe_context, &return_type);
    let context_value = &exe_context.context_value;
    let built = info.info(exe_context);
    let runtime_type =
        resolve_type_fn(agent, &result, context_value, built, &return_type).into_result()?;

    let Value::Promise(runtime_type) = &runtime_type else {
        return complete_runtime_type(
            agent,
            exe_context,
            &runtime_type,
            &return_type,
            field,
            info,
            path,
            result,
        );
    };
    let exe_context = Rc::clone(exe_context);
    let field = Rc::clone(field);
    let info = Rc::clone(built);
    let path = path.path();
    Ok(Value::Promise(runtime_type.then(
        agent,
        Some(Box::new(move |agent, resolved_runtime_type| {
            complete_runtime_type(
                agent,
                &exe_context,
                &resolved_runtime_type,
                &return_type,
                &field,
                &FieldInfo::built(&info),
                &PathFrame::held(path),
                result,
            )
            .into()
        })),
        None,
    )))
}

/// Completes `result` as the object type `runtime_type_name` names.
///
/// This is `completeObjectValue(exeContext, ensureValidRuntimeType(..), ..)`,
/// which the reference writes out at `execute.js:687` for a promised name and
/// at `:689` for any other.
// The parameter list is that of `completeObjectValue`, with the agent, the
// name and the abstract type on top of it.
#[expect(clippy::too_many_arguments)]
fn complete_runtime_type(
    agent: &Agent,
    exe_context: &Rc<ExecutionContext>,
    runtime_type_name: &Value,
    return_type: &NamedType,
    field: &Rc<PlanField>,
    info: &FieldInfo<'_>,
    path: &PathFrame<'_>,
    result: Value,
) -> Result<Value, Value> {
    let runtime_type = ensure_valid_runtime_type(
        runtime_type_name,
        exe_context,
        return_type,
        info.info(exe_context),
        &result,
    )?;
    complete_object_value(agent, exe_context, runtime_type, field, info, path, result)
}

/// Returns a `GraphQLError` with `message` as a thrown value, which blames `nodes` when given.
fn thrown_graphql_error(message: String, nodes: Option<&FieldNodes>) -> Value {
    let nodes = nodes.map(|nodes| {
        nodes
            .iter()
            .map(|node| AstNode::from(FieldNode::clone(node)))
            .collect()
    });
    let error = create_graphql_error(
        message,
        GraphQLErrorOptions {
            nodes,
            ..GraphQLErrorOptions::default()
        },
    );
    Value::from(ErrorObject::GraphQL(error))
}

/// Returns the object type a type resolver named, which has to be a possible type of `return_type`.
///
/// The field nodes are those of `info`. The pinned graphql is 16.12.0, so an
/// object type in place of a name is refused, the branch of
/// `versionInfo.major >= 16`. The values of the third message print with
/// `cross-inspect`. The object type is borrowed from the schema of
/// `exe_context`, which holds it for the request.
///
/// # Errors
///
/// Returns the six `GraphQLError`s of the reference, and what a thunk of the
/// schema throws.
pub fn ensure_valid_runtime_type<'a>(
    runtime_type_name: &Value,
    exe_context: &'a ExecutionContext,
    return_type: &NamedType,
    info: &ResolveInfo,
    result: &Value,
) -> Result<&'a SharedType<ObjectType>, Value> {
    let field_nodes = Some(&info.field_nodes);
    let abstract_name = return_type.name();
    if runtime_type_name.is_nullish() {
        return Err(thrown_graphql_error(
            format!(
                "Abstract type \"{abstract_name}\" must resolve to an Object type at runtime for field \"{}.{}\". Either the \"{abstract_name}\" type should provide a \"resolveType\" function or each possible type should provide an \"isTypeOf\" function.",
                info.parent_type.name, info.field_name
            ),
            field_nodes,
        ));
    }

    // releases before 16.0.0 supported returning `GraphQLObjectType` from `resolveType`
    if let Value::Host(HostValue::Type(runtime_type)) = runtime_type_name
        && is_object_type(runtime_type)
    {
        return Err(thrown_graphql_error(
            String::from(
                "Support for returning GraphQLObjectType from resolveType was removed in graphql-js@16.0.0 please return type name instead.",
            ),
            None,
        ));
    }

    let Value::String(runtime_type_name) = runtime_type_name else {
        return Err(thrown_graphql_error(
            format!(
                "Abstract type \"{abstract_name}\" must resolve to an Object type at runtime for field \"{}.{}\" with value {}, received \"{}\".",
                info.parent_type.name,
                info.field_name,
                cross_inspect(result),
                cross_inspect(runtime_type_name)
            ),
            None,
        ));
    };
    let runtime_type_name = to_string_lossy(runtime_type_name);

    let Some(runtime_type) = exe_context.schema.get_type(&runtime_type_name) else {
        return Err(thrown_graphql_error(
            format!(
                "Abstract type \"{abstract_name}\" was resolved to a type \"{runtime_type_name}\" that does not exist inside the schema."
            ),
            field_nodes,
        ));
    };

    let NamedType::Object(object_type) = runtime_type else {
        return Err(thrown_graphql_error(
            format!(
                "Abstract type \"{abstract_name}\" was resolved to a non-object type \"{runtime_type_name}\"."
            ),
            field_nodes,
        ));
    };

    let is_sub_type = exe_context
        .schema
        .is_sub_type(return_type, runtime_type)
        .map_err(thrown_definition_error)?;
    if !is_sub_type {
        return Err(thrown_graphql_error(
            format!(
                "Runtime Object type \"{}\" is not a possible type for \"{abstract_name}\".",
                object_type.name
            ),
            field_nodes,
        ));
    }
    Ok(object_type)
}

/// Complete an Object value by executing all sub-selections.
///
/// The `isTypeOf` of the type runs before the sub fields are collected, where
/// graphql-js collects them first, and no result shows the order.
///
/// # Errors
///
/// Returns what `isTypeOf` throws, the error of [`invalid_return_type_error`]
/// and what [`collect_and_execute_subfields`] throws.
pub fn complete_object_value(
    agent: &Agent,
    exe_context: &Rc<ExecutionContext>,
    return_type: &Arc<ObjectType>,
    field: &Rc<PlanField>,
    info: &FieldInfo<'_>,
    path: &PathFrame<'_>,
    result: Value,
) -> Result<Value, Value> {
    // If there is an isTypeOf predicate function, call it with the
    // current result. If isTypeOf returns false, then raise an error rather
    // than continuing execution.
    let is_type_of = return_type
        .is_type_of
        .as_ref()
        .and_then(|handle| handle.downcast_ref::<IsTypeOf>());
    if let Some(is_type_of) = is_type_of {
        let is_type_of = (is_type_of.0)(
            agent,
            &result,
            &exe_context.context_value,
            info.info(exe_context),
        )
        .into_result()?;

        let Value::Promise(is_type_of) = &is_type_of else {
            return execute_subfields_of_type(
                agent,
                exe_context,
                return_type,
                field,
                path,
                &result,
                &is_type_of,
            );
        };
        let exe_context = Rc::clone(exe_context);
        let return_type = Arc::clone(return_type);
        let field = Rc::clone(field);
        let path = path.path();
        return Ok(Value::Promise(is_type_of.then(
            agent,
            Some(Box::new(move |agent, resolved_is_type_of| {
                execute_subfields_of_type(
                    agent,
                    &exe_context,
                    &return_type,
                    &field,
                    &PathFrame::held(path),
                    &result,
                    &resolved_is_type_of,
                )
                .into()
            })),
            None,
        )));
    }

    collect_and_execute_subfields(agent, exe_context, return_type, field, path, &result)
}

/// Executes the sub fields of `result` when `is_type_of` is truthy, and throws otherwise.
///
/// This is the check the reference writes out at `execute.js:730-733` for a
/// promised answer of `isTypeOf` and at `:736-738` for any other.
fn execute_subfields_of_type(
    agent: &Agent,
    exe_context: &Rc<ExecutionContext>,
    return_type: &Arc<ObjectType>,
    field: &Rc<PlanField>,
    path: &PathFrame<'_>,
    result: &Value,
    is_type_of: &Value,
) -> Result<Value, Value> {
    if !to_boolean(is_type_of) {
        return Err(invalid_return_type_error(
            return_type,
            result,
            field.field_nodes(),
        ));
    }
    collect_and_execute_subfields(agent, exe_context, return_type, field, path, result)
}

/// Returns the error of a value `isTypeOf` of `return_type` refuses, as a thrown value.
///
/// The value prints with `cross-inspect`.
#[must_use]
pub fn invalid_return_type_error(
    return_type: &ObjectType,
    result: &Value,
    field_nodes: &FieldNodes,
) -> Value {
    thrown_graphql_error(
        format!(
            "Expected value of type \"{}\" but got: {}.",
            return_type.name,
            cross_inspect(result)
        ),
        Some(field_nodes),
    )
}

/// Collects the sub fields of `field` on `return_type` and executes them on `result`.
///
/// The sub fields are the node of the plan for `field` on `return_type`, which
/// the plan collects once, as `collectSubfields` does with its memo at
/// `execute.js:28` (see [󰀆](crate::plan::Plan)).
///
/// # Errors
///
/// Returns what the collection throws and what `execute_fields` throws.
pub fn collect_and_execute_subfields(
    agent: &Agent,
    exe_context: &Rc<ExecutionContext>,
    return_type: &Arc<ObjectType>,
    field: &Rc<PlanField>,
    path: &PathFrame<'_>,
    result: &Value,
) -> Result<Value, Value> {
    // Collect sub-fields to execute to complete this value.
    let sub_field_nodes = exe_context
        .plan
        .sub(exe_context, field, return_type)
        .map_err(thrown_definition_error)?;
    execute_fields(
        agent,
        exe_context,
        return_type,
        result,
        Some(path),
        &sub_field_nodes,
    )
}

/// Returns the `isTypeOf` function of `ty`, when the executor put one there.
fn is_type_of_fn(ty: &NamedType) -> Option<&IsTypeOfFn> {
    let handle = ty.as_object()?.is_type_of.as_ref()?;
    handle
        .downcast_ref::<IsTypeOf>()
        .map(|is_type_of| &is_type_of.0)
}

/// If a resolveType function is not given, then a default resolve behavior is
/// used which attempts two strategies.
///
/// First, See if the provided value has a `__typename` field defined, if so,
/// use that value as name of the resolved type.
///
/// Otherwise, test each possible type for the abstract type by calling isTypeOf
/// for the object being coerced, returning the first type that matches. The
/// promised results go into a sparse array at the index of their type, and
/// `Promise.all` reads a hole as `undefined`, so every hole is
/// [`Value::Undefined`]. A type that matches while an earlier answer is still a
/// promise returns at once, without the `Promise.allSettled` that graphql-js
/// puts on the pending answers. A promise of Aosabi reports no unhandled
/// rejection, so no result shows the difference.
pub fn default_type_resolver(
    agent: &Agent,
    value: &Value,
    context_value: &ContextValue,
    info: &Rc<ResolveInfo>,
    abstract_type: &NamedType,
) -> Completion {
    // First, look for `__typename`.
    if let Value::Object(object) = value
        && let Value::String(typename) = Rc::clone(object).get("__typename")
    {
        return Completion::Normal(Value::String(typename));
    }

    // Otherwise, test each possible type.
    let possible_types = match info.schema.get_possible_types(abstract_type) {
        Ok(possible_types) => possible_types,
        Err(error) => return Completion::Throw(thrown_definition_error(error)),
    };
    let mut promised_is_type_of_results: Vec<Value> = Vec::new();
    for (i, ty) in possible_types.iter().enumerate() {
        let Some(is_type_of) = is_type_of_fn(ty) else {
            continue;
        };
        let is_type_of_result = match is_type_of(agent, value, context_value, info) {
            Completion::Normal(is_type_of_result) => is_type_of_result,
            Completion::Throw(error) => return Completion::Throw(error),
        };
        if is_promise(&is_type_of_result) {
            promised_is_type_of_results.resize(i, Value::Undefined);
            promised_is_type_of_results.push(is_type_of_result);
        } else if to_boolean(&is_type_of_result) {
            return Completion::Normal(Value::from(ty.name()));
        }
    }

    if promised_is_type_of_results.is_empty() {
        return Completion::Normal(Value::Undefined);
    }
    let resolved = first_promised_match(agent, promised_is_type_of_results, possible_types);
    Completion::Normal(Value::Promise(resolved))
}

/// Returns a promise of the name of the first type whose promised `isTypeOf` answer is truthy.
///
/// `promised_is_type_of_results` holds the answer of a type at the index of the
/// type in `possible_types`.
fn first_promised_match(
    agent: &Agent,
    promised_is_type_of_results: Vec<Value>,
    possible_types: &[NamedType],
) -> Promise {
    let names: Vec<String> = possible_types
        .iter()
        .map(|ty| String::from(ty.name()))
        .collect();
    Promise::all(agent, promised_is_type_of_results).then(
        agent,
        Some(Box::new(move |_, is_type_of_results| {
            let Value::List(is_type_of_results) = &is_type_of_results else {
                return Completion::Normal(Value::Undefined);
            };
            let matched = is_type_of_results
                .iter()
                .zip(names)
                .find(|(is_type_of_result, _)| to_boolean(is_type_of_result));
            Completion::Normal(matched.map_or(Value::Undefined, |(_, name)| Value::from(name)))
        })),
        None,
    )
}

/// If a resolve function is not given, then a default resolve behavior is used
/// which takes the property of the source object of the same name as the field
/// and returns it as the result, or if it's a function, returns the result
/// of calling that function while passing along args and context value.
///
/// An object is the one source of the value model with a property a field names,
/// so a source of any other kind answers `undefined`. The reference reads the
/// property a second time for the call, and a property of an [`ObjectLike`]
/// reads the same twice.
pub fn default_field_resolver(
    agent: &Agent,
    source: &Value,
    args: &Rc<PlainObject>,
    context_value: &ContextValue,
    info: &Rc<ResolveInfo>,
) -> Completion {
    // ensure source is a value for which property access is acceptable.
    let Value::Object(source) = source else {
        return Completion::Normal(Value::Undefined);
    };
    let property = Rc::clone(source).get(&info.field_name);
    if let Value::Function(property) = &property {
        let arguments = [
            Value::from(Rc::clone(args)),
            Value::Host(HostValue::Context(Rc::clone(context_value))),
            Value::Host(HostValue::Info(Rc::clone(info))),
        ];
        return property.call(agent, &arguments);
    }
    Completion::Normal(property)
}

/// This method looks up the field on the given type definition.
///
/// It has special casing for the three introspection fields, `__schema`,
/// `__type` and `__typename`. `__typename` is special because it can always be
/// queried as a field, even in situations where no other fields are allowed,
/// like on a Union. `__schema` and `__type` could get automatically added to the
/// query type, but that would require mutating type definitions, which would
/// cause issues. The three names start with `__`, so a name that does not is
/// looked up in the fields of `parent_type` alone, and only a name that does
/// reads the meta fields, on a path the compiler keeps out of line. The field is
/// borrowed, from the field map of `parent_type` or from the static that holds a
/// meta field, so a lookup counts no reference.
///
/// # Errors
///
/// Returns what `getFields` of `parent_type` throws, and the error of a meta
/// field definition, which is never produced.
pub fn get_field_def<'a>(
    schema: &Schema,
    parent_type: &'a ObjectType,
    field_node: &FieldNode,
) -> Result<Option<&'a Field>, DefinitionError> {
    Ok(match find_field_def(schema, parent_type, field_node)? {
        FieldSite::Own(_, field) | FieldSite::Meta(field) => Some(field),
        FieldSite::Missing => None,
    })
}

/// Where [`get_field_def`] finds the definition of a field.
pub(crate) enum FieldSite<'a> {
    /// A field of the parent type, with its position among the fields of the type.
    Own(usize, &'a Field),
    /// A meta field, borrowed from the static that holds it.
    Meta(&'static Field),
    /// No field of the name.
    Missing,
}

/// Returns where the definition of the field of `field_node` on `parent_type` stands.
///
/// The lookup of [`get_field_def`], which answers the position of a field of
/// the parent type as well, so the plan reads the field by its position.
///
/// # Errors
///
/// Returns what [`get_field_def`] returns.
pub(crate) fn find_field_def<'a>(
    schema: &Schema,
    parent_type: &'a ObjectType,
    field_node: &FieldNode,
) -> Result<FieldSite<'a>, DefinitionError> {
    let field_name = field_node.name.value();
    if field_name.starts_with("__")
        && let Some(meta_field_def) = get_meta_field_def(schema, parent_type, field_name)?
    {
        return Ok(FieldSite::Meta(meta_field_def));
    }
    let fields = parent_type.get_fields()?;
    Ok(fields
        .get_index_of(field_name)
        .and_then(|index| Some(FieldSite::Own(index, fields.get_index(index)?.1)))
        .unwrap_or(FieldSite::Missing))
}

/// Returns the meta field `field_name` names on `parent_type`, if any.
///
/// The three branches of `getFieldDef` at `execute.js:1399-1407`, in their
/// order: `__schema` and `__type` on the query type, `__typename` on any
/// type. The comparison with the query type is by identity, which is how a
/// [`NamedType`] compares. The field is borrowed from the static that holds
/// it. A name that names none of them falls through to the field map of
/// `parent_type`, as the reference does.
///
/// # Errors
///
/// Returns the error of a meta field definition, which is never produced.
#[cold]
#[inline(never)]
fn get_meta_field_def(
    schema: &Schema,
    parent_type: &ObjectType,
    field_name: &str,
) -> Result<Option<&'static Field>, DefinitionError> {
    let is_query_type = || {
        schema
            .get_query_type()
            .and_then(NamedType::as_object)
            .is_some_and(|query_type| core::ptr::eq(SharedType::as_ptr(query_type), parent_type))
    };
    let schema_meta_field_def = schema_meta_field_def()?;
    if field_name == schema_meta_field_def.name && is_query_type() {
        return Ok(Some(schema_meta_field_def));
    }
    let type_meta_field_def = type_meta_field_def()?;
    if field_name == type_meta_field_def.name && is_query_type() {
        return Ok(Some(type_meta_field_def));
    }
    let type_name_meta_field_def = type_name_meta_field_def();
    if field_name == type_name_meta_field_def.name {
        return Ok(Some(type_name_meta_field_def));
    }
    Ok(None)
}
