// SPDX-License-Identifier: MIT
/*
 * aosabi-federation: the resolver of an object field of the wrapping schema.
 *
 * Derived from @graphql-tools/delegate 12.0.9, dist/index.cjs, MIT.
 */

//! The default merged resolver, `default_merged_resolver`.
//!
//! Every object field of a wrapped schema resolves through this resolver. A
//! parent that a delegation answered hands the member under the field's
//! response key to [`resolve_external_value`] with the subschema and the
//! unpathed errors the parent carries, so a nested object is annotated in
//! turn and a remote error placed in the data is thrown where the executor
//! completes it. A falsy parent answers null, a parent no delegation
//! answered goes to the default field resolver, and a key the parent lacks
//! answers `undefined`.
//!
//! The plan of a stitched delegation leaves members over for a later
//! subschema, which the branch of `getPlanLeftOverFromParent` at
//! `:1997-2033` and `handleLeftOver` at `:2062-2109` resolve. The
//! federation builds no plan, so a parent never carries one, the branch
//! answers `undefined` as it does with no plan, and `handleLeftOver` is left
//! out with the functions below it at `:2110-2389`,
//! `handleFlattenedParent`, `handleDeferredResolverResult`,
//! `handleDeferredResolverFailure`, `parentSatisfiedSelectionSet` and
//! `flattenPromise`, which only it calls.

use std::rc::Rc;

use aosabi_execute::{
    default_field_resolver,
    execute::ResolveInfo,
    promise::Agent,
    value::{
        Completion,
        ContextValue,
        PlainObject,
        Value, //
    }, //
};

use crate::{
    delegate::{
        external::{
            ExternalAnnotation,
            annotation_of, //
        },
        resolve_external::resolve_external_value, //
    },
    utils::{
        get_response_key_from_info,
        is_truthy, //
    }, //
};

/// Resolves a field of a parent a delegation answered, `defaultMergedResolver`.
///
/// The signature is the one of a field resolver of the executor, so the
/// wrapping schema binds the function as it is. The annotation of the parent
/// is read once, where the reference reads its three symbols one by one, and
/// the reads have no effect a caller can see, so the order of the calls
/// stays the reference's.
pub fn default_merged_resolver(
    agent: &Agent,
    parent: &Value,
    args: &Rc<PlainObject>,
    context: &ContextValue,
    info: &Rc<ResolveInfo>,
) -> Completion {
    if !is_truthy(parent) {
        return Completion::Normal(Value::Null);
    }
    let response_key = get_response_key_from_info(info);
    let Some(annotation) = annotation_of(parent) else {
        return default_field_resolver(agent, parent, args, context, info);
    };
    if !has_own_property(parent, response_key) {
        return Completion::Normal(Value::Undefined);
    }
    resolve_member(parent, response_key, &annotation, context, info)
}

/// Resolves the member `response_key` of a parent a delegation answered.
pub fn handle_result(
    parent: &Value,
    response_key: &str,
    context: &ContextValue,
    info: &Rc<ResolveInfo>,
) -> Completion {
    match annotation_of(parent) {
        Some(annotation) => resolve_member(parent, response_key, &annotation, context, info),
        None => Completion::Normal(Value::Undefined),
    }
}

/// Resolves the member `response_key` of a parent with the annotation `annotation`.
fn resolve_member(
    parent: &Value,
    response_key: &str,
    annotation: &ExternalAnnotation,
    context: &ContextValue,
    info: &Rc<ResolveInfo>,
) -> Completion {
    let data = match parent {
        Value::Object(object) => Rc::clone(object).get(response_key),
        _ => Value::Undefined,
    };
    Completion::from(resolve_external_value(
        data,
        Rc::clone(&annotation.unpathed_errors),
        &annotation.subschema_of(response_key),
        Some(context),
        Some(info),
        None,
        false,
    ))
}

/// Returns `Object.prototype.hasOwnProperty.call(parent, key)`.
///
/// An object answers through
/// [`ObjectLike::has_own`](aosabi_execute::value::ObjectLike::has_own), which a
/// [`PlainObject`] answers from its map, one lookup however wide the parent
/// is, as the property lookup of the reference is.
fn has_own_property(parent: &Value, key: &str) -> bool {
    match parent {
        Value::Object(object) => object.has_own(key),
        _ => false,
    }
}
