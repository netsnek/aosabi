// SPDX-License-Identifier: MIT
/*
 * aosabi-federation: the resolution of a delegated value into the external
 * objects the outer executor reads.
 *
 * Derived from @graphql-tools/delegate 12.0.9, dist/index.cjs, MIT.
 */

//! The resolution of a delegated value, `resolve_external_value` and the
//! object and list forms below it.
//!
//! The value of a delegated field becomes what the executor of the field that
//! delegated completes: an error stays the error it is, a null value reports
//! the errors of the result no path placed, a leaf is parsed by its type with
//! `null` for a value the type refuses, an object is annotated as an external
//! object of its subschema, a list resolves each item, and an object of an
//! abstract type whose `__typename` the schema of the field lacks is `null`.
//!
//! The `handleMaybePromise` around the object of the reference continues at
//! once, because with no stitching information `resolveExternalObject`
//! answers the object itself and never a promise, so the resolution is
//! synchronous here. Its stitching half at `:360-403`, which merges the
//! fields of several subschemas through `mergeFields`, is left out, since
//! the federation builds no stitched schema and its schemas carry no
//! `stitchingInfo` extension.
//!
//! The errors one null reports are reported once per thread, as the
//! module wide `WeakSet` of the reference remembers them, keyed by the
//! address of the error and dropped with it, so a null asks one lookup per
//! error. A list of unpathed errors every error of which a null reported is
//! remembered as well, keyed by its address, so a later null that carries
//! the same list answers null without asking for its errors again, which
//! the reference asks one by one for every null.

use std::{
    cell::{
        Cell,
        RefCell, //
    },
    rc::{
        Rc,
        Weak, //
    }, //
};

use aosabi_execute::{
    errors::ThrownError,
    execute::ResolveInfo,
    value::{
        ContextValue,
        ErrorObject,
        ObjectLike,
        Value,
        to_js_string, //
    }, //
};
use aosabi_json::map::IndexMap;
use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions,
    OriginalError,
    PathSegment,
    js_string::join, //
};
use aosabi_type::{
    NamedType,
    Type,
    get_nullable_type,
    is_abstract_type,
    is_composite_type,
    is_leaf_type, //
};

use crate::{
    delegate::{
        external::{
            annotate_external_object,
            is_external_object, //
        },
        subschema::SchemaOrSubschemaConfig, //
    },
    utils::{
        AddressState,
        error_message,
        error_path, //
    }, //
};

/// The number of entries a table holds before its first sweep of the entries whose value is gone.
const FIRST_SWEEP: usize = 64;

thread_local! {
    /// The errors a null reported already, `reportedErrors`, keyed by the address of the error.
    static REPORTED_ERRORS: RefCell<IndexMap<usize, ReportedError, AddressState>> =
        RefCell::new(IndexMap::default());
    /// The size of [`REPORTED_ERRORS`] at which its next sweep runs.
    static NEXT_ERRORS_SWEEP: Cell<usize> = const { Cell::new(FIRST_SWEEP) };
    /// The lists of unpathed errors every error of which a null reported, keyed by the address of the list.
    static REPORTED_LISTS: RefCell<IndexMap<usize, Weak<[Value]>, AddressState>> =
        RefCell::new(IndexMap::default());
    /// The size of [`REPORTED_LISTS`] at which its next sweep runs.
    static NEXT_LISTS_SWEEP: Cell<usize> = const { Cell::new(FIRST_SWEEP) };
}

/// A weak handle of a reported error, an error or an error like object.
///
/// The handle keeps the allocation of the error, so no other value takes its
/// address while the entry stands, and an entry whose error is gone is
/// dropped when the table grows.
enum ReportedError {
    /// An error.
    Error(Weak<ErrorObject>),
    /// An object, an error a copy of a result made plain.
    Object(Weak<dyn ObjectLike>),
}

impl ReportedError {
    /// Returns whether the error is alive.
    fn is_alive(&self) -> bool {
        match self {
            Self::Error(weak) => weak.strong_count() > 0,
            Self::Object(weak) => weak.strong_count() > 0,
        }
    }
}

/// Returns the delegated value `result` as the field that delegated completes it.
///
/// The return type is the one of `info` when none is given. `context` is the
/// context value the stitching branch hands on, which this branch never reads.
///
/// # Errors
///
/// Returns the plain error `Return type cannot be inferred without a source
/// schema.` with neither a return type nor a resolve information, the
/// `TypeError` of annotating a primitive where the type is composite, and the
/// `TypeError` a `WeakSet` throws for an unpathed error that is a primitive.
pub fn resolve_external_value(
    result: Value,
    unpathed_errors: Rc<[Value]>,
    subschema: &SchemaOrSubschemaConfig,
    context: Option<&ContextValue>,
    info: Option<&Rc<ResolveInfo>>,
    return_type: Option<&Type>,
    skip_type_merging: bool,
) -> Result<Value, Value> {
    let return_type = match return_type {
        Some(return_type) => return_type.clone(),
        None => get_return_type(info)?,
    };
    let r#type = get_nullable_type(&return_type);
    if result.is_error() {
        return Ok(result);
    }
    if result.is_nullish() {
        return report_list_via_null(&unpathed_errors);
    }
    if is_leaf_type(r#type) {
        return Ok(parse_leaf(r#type, &result));
    }
    if is_composite_type(r#type) {
        let object = resolve_external_object(
            r#type,
            result,
            unpathed_errors,
            subschema,
            context,
            info,
            skip_type_merging,
        )?;
        return Ok(check_abstract_typename(r#type, object, info));
    }
    if let Some(list) = r#type.as_list() {
        return match &result {
            Value::List(items) => resolve_external_list(
                &list.of_type,
                items,
                &unpathed_errors,
                subschema,
                context,
                info,
                skip_type_merging,
            ),
            _ => resolve_external_value(
                result,
                unpathed_errors,
                subschema,
                context,
                info,
                Some(&list.of_type),
                skip_type_merging,
            ),
        };
    }
    Ok(Value::Undefined)
}

/// Returns the object a delegation answered, annotated with its subschema and its unpathed errors.
///
/// An object a delegation annotated already keeps its annotation.
///
/// # Errors
///
/// Returns the `TypeError` of annotating a value that is no object.
pub fn resolve_external_object(
    r#type: &Type,
    object: Value,
    unpathed_errors: Rc<[Value]>,
    subschema: &SchemaOrSubschemaConfig,
    context: Option<&ContextValue>,
    info: Option<&Rc<ResolveInfo>>,
    skip_type_merging: bool,
) -> Result<Value, Value> {
    let _ = (r#type, context, info, skip_type_merging);
    if !is_external_object(&object) {
        annotate_external_object(&object, unpathed_errors, subschema.clone(), Vec::new())?;
    }
    Ok(object)
}

/// Returns a new list of every item of a delegated list, each resolved by the item type.
///
/// # Errors
///
/// Returns what [`resolve_external_value`] throws for an item.
pub fn resolve_external_list(
    item_type: &Type,
    list: &[Value],
    unpathed_errors: &Rc<[Value]>,
    subschema: &SchemaOrSubschemaConfig,
    context: Option<&ContextValue>,
    info: Option<&Rc<ResolveInfo>>,
    skip_type_merging: bool,
) -> Result<Value, Value> {
    let mut items = Vec::with_capacity(list.len());
    for list_member in list {
        items.push(resolve_external_value(
            list_member.clone(),
            Rc::clone(unpathed_errors),
            subschema,
            context,
            info,
            Some(item_type),
            skip_type_merging,
        )?);
    }
    Ok(Value::from(items))
}

/// Returns the unpathed errors no null reported yet as the error of this null, or null when there is none.
///
/// One error is located at its own path, and several become an
/// `AggregateError` of errors located at the path of the first, whose message
/// joins their messages with a comma, a space and a line feed.
///
/// # Errors
///
/// Returns the `TypeError` `Invalid value used in weak set` for an error
/// that is a primitive, which the `WeakSet` of the reference refuses.
pub fn report_unpathed_errors_via_null(unpathed_errors: &[Value]) -> Result<Value, Value> {
    let mut unreported_errors = Vec::new();
    for error in unpathed_errors {
        if mark_reported(error)? {
            unreported_errors.push(error.clone());
        }
    }
    let Some(unreported_error) = unreported_errors.first() else {
        return Ok(Value::Null);
    };
    let path = error_path(unreported_error);
    if unreported_errors.len() == 1 {
        return Ok(located_error(unreported_error, path));
    }
    let errors = unreported_errors
        .iter()
        .map(|error| located_error(error, path.clone()))
        .collect();
    let messages: Vec<_> = unreported_errors.iter().map(error_message).collect();
    Ok(Value::from(ErrorObject::Aggregate {
        message: join(&messages, ", \n"),
        errors,
    }))
}

/// Returns what [`report_unpathed_errors_via_null`] answers for the list `unpathed_errors`.
///
/// A list every error of which a null reported answers null at once, and a
/// list whose report completes is remembered as such. Its errors stay
/// reported while the list holds them, so the answer of a later null is the
/// null the report would answer after a lookup per error.
fn report_list_via_null(unpathed_errors: &Rc<[Value]>) -> Result<Value, Value> {
    if unpathed_errors.is_empty() {
        return Ok(Value::Null);
    }
    let address = Rc::as_ptr(unpathed_errors).cast::<()>().addr();
    if REPORTED_LISTS.with(|lists| lists.borrow().contains_key(&address)) {
        return Ok(Value::Null);
    }
    let answer = report_unpathed_errors_via_null(unpathed_errors)?;
    REPORTED_LISTS.with(|lists| {
        let mut lists = lists.borrow_mut();
        sweep(&mut lists, &NEXT_LISTS_SWEEP, |weak| {
            weak.strong_count() > 0
        });
        lists.insert(address, Rc::downgrade(unpathed_errors));
    });
    Ok(answer)
}

/// Drops the entries whose value is gone once `table` has grown to the size `next` holds.
fn sweep<T>(
    table: &mut IndexMap<usize, T, AddressState>,
    next: &'static std::thread::LocalKey<Cell<usize>>,
    is_alive: impl Fn(&T) -> bool,
) {
    let due = next.with(Cell::get);
    if table.len() < due {
        return;
    }
    let old = core::mem::take(table);
    for (address, entry) in old {
        if is_alive(&entry) {
            table.insert(address, entry);
        }
    }
    next.with(|next| next.set(FIRST_SWEEP.max(table.len() * 2)));
}

/// Returns the return type of the field that delegates, `getReturnType$1`.
///
/// # Errors
///
/// Returns the plain error `Return type cannot be inferred without a source
/// schema.` without a resolve information.
pub fn get_return_type(info: Option<&Rc<ResolveInfo>>) -> Result<Type, Value> {
    info.map(|info| info.return_type.clone()).ok_or_else(|| {
        Value::from(ErrorObject::error(
            "Return type cannot be inferred without a source schema.",
        ))
    })
}

/// Returns the leaf value `result` as its type parses it, and null where the type refuses it.
///
/// This is `type.parseValue(result)` inside the `try` of `:299-303`. A
/// scalar and an enum parse over the JSON value model of the type layer.
fn parse_leaf(r#type: &Type, result: &Value) -> Value {
    let parsed = match r#type {
        Type::Scalar(scalar) => scalar.parse_value(&result.to_js()),
        Type::Enum(enum_type) => enum_type.parse_value(&result.to_js()),
        _ => return Value::Null,
    };
    parsed.map_or(Value::Null, |value| Value::from_js(&value))
}

/// Returns null for an object of an abstract type whose `__typename` the schema of `info` lacks.
///
/// The continuation of `:315-326`. A `__typename` that is null or absent, a
/// type that is no abstract type and a call without `info` keep the object.
fn check_abstract_typename(r#type: &Type, object: Value, info: Option<&Rc<ResolveInfo>>) -> Value {
    let Some(info) = info else {
        return object;
    };
    if !is_abstract_type(r#type) {
        return object;
    }
    let typename = match &object {
        Value::Object(fields) => Rc::clone(fields).get("__typename"),
        _ => Value::Undefined,
    };
    if typename.is_nullish() {
        return object;
    }
    let name = to_js_string(&typename);
    let resolved_type: Option<&NamedType> = info
        .schema
        .get_type(&String::from_utf8_lossy(name.as_wtf8()));
    if resolved_type.is_none() {
        return Value::Null;
    }
    object
}

/// Records `error` as reported and returns whether it was not reported before.
///
/// An entry at the address of `error` is `error` itself, since the weak
/// handle of the entry keeps the allocation from every other value, so the
/// test is one lookup.
fn mark_reported(error: &Value) -> Result<bool, Value> {
    let (address, handle) = match error {
        Value::Error(error) => (
            Rc::as_ptr(error).cast::<()>().addr(),
            ReportedError::Error(Rc::downgrade(error)),
        ),
        Value::Object(object) => (
            Rc::as_ptr(object).cast::<()>().addr(),
            ReportedError::Object(Rc::downgrade(object)),
        ),
        _ => {
            return Err(Value::from(ErrorObject::type_error(
                "Invalid value used in weak set",
            )));
        }
    };
    REPORTED_ERRORS.with(|reported| {
        let mut reported = reported.borrow_mut();
        if reported.contains_key(&address) {
            return Ok(false);
        }
        sweep(&mut reported, &NEXT_ERRORS_SWEEP, ReportedError::is_alive);
        reported.insert(address, handle);
        Ok(true)
    })
}

/// Returns `locatedError(error, undefined, path)` of graphql-js, the same error when it carries a path.
///
/// A value that is no error is wrapped as `toError` wraps it, and an error
/// that already carries a path, which every error a result relocated does,
/// comes back as the same error.
///
/// Derived from graphql-js 16.12.0, src/error/locatedError.ts, MIT.
fn located_error(error: &Value, path: Option<Vec<PathSegment>>) -> Value {
    let error_object = aosabi_execute::value::to_error(error);
    if let ErrorObject::GraphQL(graphql_error) = error_object.as_ref() {
        if graphql_error.path.is_some() {
            return Value::Error(error_object);
        }
        let located = GraphQLError::new(
            graphql_error.message.clone(),
            GraphQLErrorOptions {
                nodes: graphql_error.nodes.clone(),
                source: graphql_error.source.clone(),
                positions: graphql_error.positions.clone(),
                path,
                original_error: Some(OriginalError::from(
                    aosabi_execute::errors::copy_graphql_error(graphql_error),
                )),
                extensions: None,
            },
        );
        return Value::from(ErrorObject::GraphQL(located));
    }
    let message = error_object.message();
    let located = GraphQLError::new(
        message.clone(),
        GraphQLErrorOptions {
            path,
            original_error: Some(OriginalError::Other(Box::new(ThrownError {
                name: error_object.name(),
                message,
                host: None,
            }))),
            ..GraphQLErrorOptions::default()
        },
    );
    Value::from(ErrorObject::GraphQL(located))
}
