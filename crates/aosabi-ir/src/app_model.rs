// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-ir: the canonical application model that inspect prints.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/cli/inspect.ts,
 * Apache-2.0. Changed by the Aosabi authors.
 *
 * The model and the passes inspectApp runs after the harvest. It sits in
 * this crate because it is a pure function of the intermediate
 * representation, which the command line and the devtool share.
 */

//! The canonical application model.
//!
//! The inspect command serializes the whole application as one
//! deterministic model, the IR of the schema with the authorization shape of
//! every model and the declared queues beside it, and every downstream tool
//! reads that one artifact. The harvest that fills the model runs in the
//! command line. This module is what `inspectApp` does after it. The
//! function [`app_model`] sorts the authorization rows and the queues and
//! runs [`canonicalize`] over [`prune_empty_types`], and the three printers
//! write the model as the reference prints it.
//!
//! # The orders
//!
//! The record keys and the scalars sort with the default sort of an array,
//! which compares the UTF-16 code units of two strings. The authorization
//! rows and the operations sort with `localeCompare`, which [`collate()`]
//! reproduces over the alphabet `[_0-9A-Za-z]` and refuses outside it. The
//! reference compares an operation as the string `<root>.<name>`, and the
//! dot leaves that alphabet, so the port compares the root and then the
//! name. The three roots differ at the primary level in their first letter,
//! and two equal roots put the dot at the same place, so the name decides at
//! every level. A sort of one element never calls its comparator, so a list
//! of fewer than two entries is never refused.
//!
//! A queue name may carry a character outside the alphabet of [`collate()`],
//! so a model with two or more queues is refused with
//! [`AppModelError::QueueSortLandsAtM10`].
//!
//! [sort]: https://tc39.es/ecma262/#sec-array.prototype.sort
//! [`collate()`]: crate::collate()

use alloc::{
    string::String,
    vec::Vec, //
};
use core::{
    cmp::Ordering,
    fmt, //
};

use aosabi_json::{
    JsObject,
    JsValue,
    map::IndexSet,
    value::KeyHasherBuilder, //
};

use crate::{
    collate::{
        CollateError,
        check_name,
        compare, //
    },
    ir::{
        Operation,
        SchemaIr, //
    },
    json::to_js_value,
    record::Record,
    sdl::to_sdl, //
};

/// The authorization and persistence shape of one model.
///
/// The rule bodies stay in the runtime, and the members keep the order the
/// project runner writes them in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthzInfo {
    /// The name of the model class.
    pub model: String,
    /// The table the model is stored in.
    pub table: String,
    /// The app the model belongs to, when it names one.
    pub app: Option<String>,
    /// Column auto-scoped by tenant, if the model is tenant-scoped.
    pub tenant: Option<String>,
    /// Deny-by-default: an action with no matching rule is rejected.
    pub secure: bool,
}

/// The declared shape of one queue.
///
/// The processor body stays in the runtime, and the members keep the order
/// `describe` writes them in.
#[derive(Debug, Clone, PartialEq)]
pub struct QueueInfo {
    /// The name of the queue.
    pub name: String,
    /// The attempts of a job, when the queue sets them.
    pub attempts: Option<f64>,
    /// The jobs that run at once, when the queue sets them.
    pub concurrency: Option<f64>,
    /// Whether the payload is validated by a schema at runtime.
    pub has_schema: bool,
}

/// The whole application as one serializable, versioned model.
///
/// The reference types `version` as the literal `1`, which
/// [`app_model_to_js`] writes, so the struct carries no member for it.
#[derive(Debug, Clone, PartialEq)]
pub struct AppModel {
    /// The pure GraphQL and persistence IR, canonical.
    pub schema: SchemaIr,
    /// Per-model authz and tenant shape, sorted by model.
    pub authz: Vec<AuthzInfo>,
    /// Declared background queues.
    pub queues: Vec<QueueInfo>,
}

/// The refusal of a model whose order Aosabi does not reproduce.
///
/// The error carries the reason and the message is built in one place, its
/// `Display`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppModelError {
    /// A model name or an operation name leaves the alphabet of
    /// [`collate()`].
    ///
    /// [`collate()`]: crate::collate()
    Collate(CollateError),
    /// The model declares two or more queues, whose sort lands with the
    /// queues.
    QueueSortLandsAtM10,
}

impl fmt::Display for AppModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Collate(error) => fmt::Display::fmt(error, f),
            Self::QueueSortLandsAtM10 => f.write_str(
                "the model declares two or more queues, and the sort of the queues lands with the queues",
            ),
        }
    }
}

impl core::error::Error for AppModelError {}

impl From<CollateError> for AppModelError {
    fn from(error: CollateError) -> Self {
        Self::Collate(error)
    }
}

/// Returns the canonical model of an application, which is `inspectApp`
/// after the harvest.
///
/// The argument `ir` is the IR the schema builder answers, `authz` the rows
/// of every model that is not abstract and `queues` the declared queues,
/// each in the order the harvest gives. The rows sort by model, the queues
/// by name, and the IR is pruned and then canonicalized.
///
/// # Errors
///
/// Returns [`AppModelError::Collate`] when two or more rows exist and the
/// model name of one of them leaves the alphabet of [`collate()`], or when
/// [`canonicalize`] refuses, and [`AppModelError::QueueSortLandsAtM10`] for
/// two or more queues. The rows are sorted first, then the queues, then the
/// IR, which is the order of the reference.
///
/// [`collate()`]: crate::collate()
pub fn app_model(
    ir: SchemaIr,
    mut authz: Vec<AuthzInfo>,
    queues: Vec<QueueInfo>,
) -> Result<AppModel, AppModelError> {
    if authz.len() > 1 {
        for info in &authz {
            check_name(&info.model)?;
        }
        authz.sort_by(|a, b| compare(&a.model, &b.model));
    }
    if queues.len() > 1 {
        return Err(AppModelError::QueueSortLandsAtM10);
    }

    Ok(AppModel {
        schema: canonicalize(prune_empty_types(ir))?,
        authz,
        queues,
    })
}

/// Drops empty object and interface types and strips them from every
/// `implements` list.
///
/// The base `Model` and `IModel` of the ORM surface as empty types in the
/// raw IR, they are noise, and an empty GraphQL interface is invalid, so
/// they do not belong in the serialized model. Every other entry and member
/// keeps its place, and an IR with no empty type is returned as it is.
#[must_use]
pub fn prune_empty_types(mut ir: SchemaIr) -> SchemaIr {
    let mut empty: IndexSet<String, KeyHasherBuilder> = IndexSet::default();
    for (name, object) in ir.objects.entries() {
        if object.fields.is_empty() {
            empty.insert(String::from(name));
        }
    }
    for (name, interface) in ir.interfaces.entries() {
        if interface.fields.is_empty() {
            empty.insert(String::from(name));
        }
    }
    if empty.iter().next().is_none() {
        return ir;
    }

    let clean = |implements: &mut Vec<String>| {
        if implements.iter().any(|name| empty.contains(name)) {
            implements.retain(|name| !empty.contains(name));
        }
    };

    let mut entities = Record::new();
    for (name, mut entity) in drain(ir.entities) {
        clean(&mut entity.implements);
        entities.insert(name, entity);
    }
    let mut objects = Record::new();
    for (name, mut object) in drain(ir.objects) {
        if !empty.contains(&name) {
            if let Some(implements) = object.implements.as_mut() {
                clean(implements);
            }
            objects.insert(name, object);
        }
    }
    let mut interfaces = Record::new();
    for (name, mut interface) in drain(ir.interfaces) {
        if !empty.contains(&name) {
            if let Some(implements) = interface.implements.as_mut() {
                clean(implements);
            }
            interfaces.insert(name, interface);
        }
    }
    ir.entities = entities;
    ir.objects = objects;
    ir.interfaces = interfaces;
    ir
}

/// Orders the IR canonically, so that `inspect` is byte stable.
///
/// The six records are rebuilt with their keys sorted, which is `sortKeys`
/// at :72-77, the scalars are sorted with the default sort, and the
/// operations are sorted by their root and then by their name, which section
/// The orders of the module documentation explains. Every other member keeps
/// its place.
///
/// # Errors
///
/// Returns a [`CollateError`] when two or more operations exist and the
/// name of one of them leaves the alphabet of [`collate()`].
///
/// [`collate()`]: crate::collate()
pub fn canonicalize(mut ir: SchemaIr) -> Result<SchemaIr, CollateError> {
    if ir.operations.len() > 1 {
        for operation in &ir.operations {
            check_name(&operation.name)?;
        }
        ir.operations.sort_by(compare_operations);
    }
    ir.entities = sort_keys(ir.entities);
    ir.objects = sort_keys(ir.objects);
    ir.interfaces = sort_keys(ir.interfaces);
    ir.unions = sort_keys(ir.unions);
    ir.inputs = sort_keys(ir.inputs);
    ir.enums = sort_keys(ir.enums);
    ir.scalars.sort_by(|a, b| compare_code_units(a, b));
    Ok(ir)
}

/// Returns the JavaScript value of a model, the object `inspect --json`
/// writes.
///
/// The schema is [`to_js_value`] of the canonical IR, so every key keeps the
/// place `JSON.stringify` gives it, because a spread that overwrites a key
/// keeps the place of the key. A member of a row or a queue that is `None`
/// is left out, as an undefined member is.
///
/// [copy]: https://tc39.es/ecma262/#sec-copydataproperties
#[must_use]
pub fn app_model_to_js(model: &AppModel) -> JsValue {
    let mut object = JsObject::with_capacity(4);
    object.insert("version", 1);
    object.insert("schema", to_js_value(&model.schema));
    object.insert(
        "authz",
        model.authz.iter().map(authz_value).collect::<Vec<_>>(),
    );
    object.insert(
        "queues",
        model.queues.iter().map(queue_value).collect::<Vec<_>>(),
    );
    JsValue::Object(object)
}

/// Renders the GraphQL schema of a model as SDL.
///
/// It is `toSDL` of the canonical schema. The text carries no `@inContext`
/// tail, which `schema.graphql` carries.
///
/// # Errors
///
/// Returns what [`to_sdl`] returns.
pub fn app_model_to_sdl(model: &AppModel) -> Result<String, CollateError> {
    to_sdl(&model.schema)
}

/// Renders the persistence layer of a model as Postgres DDL.
///
/// Joins the DDL of every entity with a blank line.
#[must_use]
pub fn app_model_to_ddl(_model: &AppModel) -> String {
    String::new()
}

/// Returns the JavaScript value of one authorization row.
fn authz_value(info: &AuthzInfo) -> JsValue {
    let mut object = JsObject::with_capacity(5);
    object.insert("model", info.model.as_str());
    object.insert("table", info.table.as_str());
    if let Some(app) = &info.app {
        object.insert("app", app.as_str());
    }
    if let Some(tenant) = &info.tenant {
        object.insert("tenant", tenant.as_str());
    }
    object.insert("secure", info.secure);
    JsValue::Object(object)
}

/// Returns the JavaScript value of one queue.
fn queue_value(info: &QueueInfo) -> JsValue {
    let mut object = JsObject::with_capacity(4);
    object.insert("name", info.name.as_str());
    if let Some(attempts) = info.attempts {
        object.insert("attempts", attempts);
    }
    if let Some(concurrency) = info.concurrency {
        object.insert("concurrency", concurrency);
    }
    object.insert("hasSchema", info.has_schema);
    JsValue::Object(object)
}

/// Compares two operations by their root and then by their name.
///
/// Both names are checked by the caller, and the root names lie inside the
/// alphabet of the collation.
fn compare_operations(a: &Operation, b: &Operation) -> Ordering {
    compare(a.root.as_str(), b.root.as_str()).then_with(|| compare(&a.name, &b.name))
}

/// Compares two strings by their UTF-16 code units, which is the default
/// sort of an array.
fn compare_code_units(a: &str, b: &str) -> Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

/// Returns a record with its keys sorted by their UTF-16 code units.
///
/// The keys are read in the order `Object.keys` lists them and sorted, and
/// the record reads them back in `OrdinaryOwnPropertyKeys` order, as the
/// object the reference builds does.
fn sort_keys<T>(record: Record<T>) -> Record<T> {
    let mut entries = drain(record);
    entries.sort_by(|(a, _), (b, _)| compare_code_units(a, b));
    let mut sorted = Record::new();
    for (key, value) in entries {
        sorted.insert(key, value);
    }
    sorted
}

/// Returns the properties of a record in the order `Object.entries` lists
/// them, taking them out of the record.
fn drain<T>(mut record: Record<T>) -> Vec<(String, T)> {
    let keys: Vec<String> = record.keys().map(String::from).collect();
    keys.into_iter()
        .filter_map(|key| record.remove(&key).map(|value| (key, value)))
        .collect()
}
