// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-core glue resolve_type: the type resolution of a union or an
 * interface.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/cli/builder/schema/schema-parser.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 *
 * Evaluates as function resolveType(node) { if (!node || typeof node !==
 * 'object') return null.. }, and of the names the projection of the canary
 * 3.0.0-canary-20260206115650 holds, getAllPropertyNames and
 * wrapFunctionsRecursively of its with a downstream patch, which no tree
 * of Aosabi vendors.
 */

//! `ResolveTypeRule` and `resolve_type`.
//!
//! The build writes the type resolver of every union and interface as the text
//! of `resolvers.js` and, from the same model, as a table of
//! [`ResolveTypeRule`] that the crate of a server includes. [`resolve_type`]
//! evaluates a rule as the text evaluates: a value that is no object answers
//! `null`, a `__typename` among the member names answers itself, then each
//! check in order, the members with more fields first, and `null` last. A check
//! of fields no other member has asks `"field" in node`, which holds for a
//! field and for a method of a described struct, as `in` holds for a member of
//! the prototype, and a check of every field asks as well that each is not
//! `undefined`.
//!
//! The type resolver of a canary build has another form: no read of
//! `__typename`, and for each member in order the check that each of its fields
//! is `in` the value. The value it tests is the projection of the canary, which
//! holds every property name of the value of the app. With the feature
//! `canary-types`, a boot that takes such a resolver makes it a rule with no
//! member names, whose [`ResolveTypeCheck::AllFields`] checks name the fields
//! of each member, and notes its type through `hold_canary_types`, and
//! `hold_names` writes into the projection of a value at that type every
//! property name of the value the projection does not hold, as `null`. The
//! check of every field over that projection then holds where each field is
//! `in` the projection of the canary, a member the value answers as `undefined`
//! among them, and the projection keeps every member it read, so the executor
//! reads each selected field as before, `null` where it read `undefined`, which
//! it completes alike.

use std::{
    rc::Rc,
    sync::Arc, //
};

#[cfg(feature = "canary-types")]
use core::cell::RefCell;

#[cfg(feature = "canary-types")]
use aosabi_execute::value::PlainObject;
#[cfg(feature = "canary-types")]
use aosabi_type::NamedType;

use aosabi_execute::{
    execute::TypeResolverFn,
    value::{
        Completion,
        ObjectLike,
        Value, //
    }, //
};

/// One check of a type resolver.
#[derive(Clone, Copy, Debug)]
pub enum ResolveTypeCheck {
    /// A node that holds one of `fields` is the member `name`.
    AnyField {
        /// The required fields no other member has.
        fields: &'static [&'static str],
        /// The member.
        name: &'static str,
    },
    /// A node that holds each of `fields` with a value other than
    /// `undefined` is the member `name`.
    AllFields {
        /// The fields of the member.
        fields: &'static [&'static str],
        /// The member.
        name: &'static str,
    },
}

/// The type resolver of one union or interface.
#[derive(Clone, Copy, Debug)]
pub struct ResolveTypeRule {
    /// The union or the interface.
    pub type_name: &'static str,
    /// The names of the member types.
    pub valid_names: &'static [&'static str],
    /// The checks, in order.
    pub checks: &'static [ResolveTypeCheck],
}

/// Returns whether `node` has the member `key`, `key in node`.
///
/// The question is [`ObjectLike::has_own`], which answers what a scan of
/// [`ObjectLike::own_keys`] answers with no list of the keys built, so a check
/// of a type resolver allocates nothing for the projection of the resolver,
/// whose keys are the names of its selection.
fn has(node: &Rc<dyn ObjectLike>, key: &str) -> bool {
    node.has_own(key)
}

/// Returns the name `rule` resolves `node` to, `None` for `null`.
#[must_use]
pub fn resolve_type(rule: &ResolveTypeRule, node: &Value) -> Option<String> {
    // `!node || typeof node !== 'object'`: a function is no object, and
    // every other value of the model that is truthy and no primitive is one.
    let object = match node {
        Value::Object(object) => Rc::clone(object),
        _ => return None,
    };
    if let Value::String(typename) = Rc::clone(&object).get("__typename")
        && !typename.is_empty()
        && let Some(name) = rule
            .valid_names
            .iter()
            .find(|name| typename.as_wtf8() == name.as_bytes())
    {
        return Some(String::from(*name));
    }
    for check in rule.checks {
        match check {
            ResolveTypeCheck::AnyField { fields, name } => {
                if fields.iter().any(|field| has(&object, field)) {
                    return Some(String::from(*name));
                }
            }
            ResolveTypeCheck::AllFields { fields, name } => {
                let all = fields.iter().all(|field| {
                    has(&object, field)
                        && !matches!(Rc::clone(&object).get(field), Value::Undefined)
                });
                if all {
                    return Some(String::from(*name));
                }
            }
        }
    }
    None
}

/// Returns the `__resolveType` of `rule` as a type resolver of the executor.
#[must_use]
pub fn type_resolver(rule: &'static ResolveTypeRule) -> TypeResolverFn {
    Arc::new(move |_agent, value, _context, _info, _abstract_type| {
        Completion::Normal(match resolve_type(rule, value) {
            Some(name) => Value::from(name),
            None => Value::Null,
        })
    })
}

/// The names `in` finds on every plain object of the projection of a canary
/// build through `Object.prototype`, whose prototype a literal `{}` takes.
#[cfg(feature = "canary-types")]
const OBJECT_PROTOTYPE: [&str; 12] = [
    "constructor",
    "hasOwnProperty",
    "isPrototypeOf",
    "propertyIsEnumerable",
    "toLocaleString",
    "toString",
    "valueOf",
    "__proto__",
    "__defineGetter__",
    "__defineSetter__",
    "__lookupGetter__",
    "__lookupSetter__",
];

// An `unsafe` cell would skip each borrow flag.
#[cfg(feature = "canary-types")]
thread_local! {
    /// The unions and the interfaces the boot resolves by a type resolver of
    /// a canary build, whose projection holds every property name of a value.
    static CANARY: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// Notes `types`, the unions and the interfaces the boot resolves by a type
/// resolver of a canary build, in place of those noted before.
#[cfg(feature = "canary-types")]
pub fn hold_canary_types(types: Vec<String>) {
    CANARY.with_borrow_mut(|held| *held = types);
}

/// Answers whether `parent_type` is a union or an interface the boot resolves
/// by a type resolver of a canary build, where [`hold_names`] fills the
/// projection of a value, so the projection there is a [`PlainObject`].
#[cfg(feature = "canary-types")]
#[must_use]
pub fn holds_names(parent_type: Option<&NamedType>) -> bool {
    let Some(parent @ (NamedType::Interface(_) | NamedType::Union(_))) = parent_type else {
        return false;
    };
    CANARY.with_borrow(|held| held.iter().any(|name| name == parent.name()))
}

/// Writes into `projection`, the projection of `source` at `parent_type`,
/// every property name of `source` and of `Object.prototype` it does not
/// hold, as `null`, where `parent_type` is a union or an interface the boot
/// resolves by a type resolver of a canary build.
///
/// The projection of the canary holds every property name of the value and
/// finds the names of `Object.prototype` through its prototype, so the checks
/// of `in` of its type resolver hold on this projection where they hold on the
/// canary's.
#[cfg(feature = "canary-types")]
pub fn hold_names(source: &Value, parent_type: Option<&NamedType>, projection: &PlainObject) {
    if !holds_names(parent_type) {
        return;
    }
    let Value::Object(object) = source else {
        return;
    };
    let names = object.property_names();
    let prototype = OBJECT_PROTOTYPE.iter().map(|name| String::from(*name));
    for name in names.into_iter().chain(prototype) {
        if !projection.contains_key(&name) {
            projection.set(&name, Value::Null);
        }
    }
}
