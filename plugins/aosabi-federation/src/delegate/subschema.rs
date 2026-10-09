// SPDX-License-Identifier: MIT
/*
 * aosabi-federation: the subschema a delegation targets and the schema its
 * transforms produce.
 *
 * Derived from @graphql-tools/delegate 12.0.9, dist/index.cjs, MIT.
 */

//! The subschema of a delegation, `Subschema` and its configuration.
//!
//! A delegation targets a plain schema or a subschema configuration, the
//! `GraphQLSchema | SubschemaConfig` of the option `schema` of
//! `delegateToSchema`, which [`SchemaOrSubschemaConfig`] is. The
//! federation hands the outer delegation the wrapped schema of a remote, a
//! plain schema, and the proxying resolvers of that schema hand the inner
//! delegation the configuration `{schema, executor}` of the remote, so the
//! inner one sends through the HTTP executor.
//!
//! A schema transform runs only for a subschema configuration that lists
//! transforms with a schema hook, and the [`Transform`] of this crate has no
//! such hook, so [`apply_schema_transforms`] answers the schema it is given,
//! and its memo per schema and configuration has no reading. The members
//! `batch`, `batchingOptions`, `merge` and `createProxyingResolver` of a
//! configuration are left out, since the federation sets none of them, and so is
//! `cloneSubschemaConfig` at `:2394-2423`, which only the stitching of
//! several subschemas calls. The `Subschema` class keeps its lazy
//! `transformedSchema` without the warning it prints under `DEBUG`.

use std::{
    cell::RefCell,
    rc::Rc,
    sync::Arc, //
};

use aosabi_execute::{
    promise::Agent,
    value::{
        Completion,
        Value, //
    }, //
};
use aosabi_type::Schema;

use crate::transforms::{
    ExecutionRequest,
    Transform, //
};

/// An executor, a function from a request to its result or a promise of it, `Executor`.
///
/// The executor runs on the agent of the request whose resolver delegates.
/// It answers the result object `{data, errors}` or a promise of it, and a
/// throw is a throw of the executor, which the delegation passes on, as a
/// synchronous throw of the reference's executor is.
pub type Executor = Rc<dyn Fn(&Agent, ExecutionRequest) -> Completion>;

/// The configuration of a subschema, `SubschemaConfig`.
#[derive(Clone)]
pub struct SubschemaConfig {
    /// The name of the subschema, `name`.
    pub name: Option<String>,
    /// The schema of the subschema, `schema`.
    pub schema: Arc<Schema>,
    /// The value its root fields resolve on, `rootValue`.
    pub root_value: Value,
    /// The transforms every delegation to it runs first, `transforms`.
    pub transforms: Vec<Rc<dyn Transform>>,
    /// The executor a request to it goes through, `executor`.
    pub executor: Option<Executor>,
}

impl SubschemaConfig {
    /// Returns the configuration `{schema, executor}`, the form the federation builds.
    #[must_use]
    pub fn new(schema: Arc<Schema>, executor: Option<Executor>) -> Self {
        Self {
            name: None,
            schema,
            root_value: Value::Undefined,
            transforms: Vec::new(),
            executor,
        }
    }
}

/// A subschema, the class `Subschema` over one configuration.
pub struct Subschema {
    /// The configuration the subschema was made from.
    pub config: Rc<SubschemaConfig>,
    /// The schema its transforms produce, made when it is first read, `_transformedSchema`.
    transformed_schema: RefCell<Option<Arc<Schema>>>,
}

impl Subschema {
    /// Returns the subschema of `config`, `new Subschema(config)`.
    #[must_use]
    pub fn new(config: Rc<SubschemaConfig>) -> Self {
        Self {
            config,
            transformed_schema: RefCell::new(None),
        }
    }

    /// Returns the schema its transforms produce, the getter of `transformedSchema`.
    ///
    /// The first read with no schema set makes it with
    /// [`apply_schema_transforms`] and keeps it.
    #[must_use]
    pub fn transformed_schema(&self) -> Arc<Schema> {
        let mut transformed_schema = self.transformed_schema.borrow_mut();
        Arc::clone(
            transformed_schema
                .get_or_insert_with(|| apply_schema_transforms(&self.config.schema, &self.config)),
        )
    }

    /// Sets the schema its transforms produce, the setter of `transformedSchema`.
    ///
    /// The schema replaces the one it holds, so the next read answers it.
    pub fn set_transformed_schema(&self, schema: Arc<Schema>) {
        self.transformed_schema.replace(Some(schema));
    }
}

/// The target of a delegation, a plain schema or a subschema configuration.
///
/// The option `schema` of `delegateToSchema`, typed
/// `GraphQLSchema | SubschemaConfig`, with the class `Subschema` beside the
/// configuration. A clone shares the target, and two targets are the same
/// one when they share it.
#[derive(Clone)]
pub enum SchemaOrSubschemaConfig {
    /// A plain schema.
    Schema(Arc<Schema>),
    /// A subschema configuration.
    Config(Rc<SubschemaConfig>),
    /// A subschema.
    Subschema(Rc<Subschema>),
}

impl SchemaOrSubschemaConfig {
    /// Returns whether both name the same target object, the `===` of the reference.
    #[must_use]
    pub fn same(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Schema(a), Self::Schema(b)) => Arc::ptr_eq(a, b),
            (Self::Config(a), Self::Config(b)) => Rc::ptr_eq(a, b),
            (Self::Subschema(a), Self::Subschema(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }

    /// Returns the configuration of a configuration or a subschema, `undefined` for a schema.
    #[must_use]
    pub fn config(&self) -> Option<Rc<SubschemaConfig>> {
        match self {
            Self::Schema(_) => None,
            Self::Config(config) => Some(Rc::clone(config)),
            Self::Subschema(subschema) => Some(Rc::clone(&subschema.config)),
        }
    }

    /// Returns the schema of the target, the plain schema or the schema of the configuration.
    #[must_use]
    pub fn schema(&self) -> Arc<Schema> {
        match self {
            Self::Schema(schema) => Arc::clone(schema),
            Self::Config(config) => Arc::clone(&config.schema),
            Self::Subschema(subschema) => Arc::clone(&subschema.config.schema),
        }
    }

    /// Returns the name of the target, `schema.name`, which a plain schema lacks.
    #[must_use]
    pub fn name(&self) -> Option<String> {
        self.config().and_then(|config| config.name.clone())
    }

    /// Returns the root value of the target, `schema.rootValue`, which a plain schema lacks.
    #[must_use]
    pub fn root_value(&self) -> Value {
        self.config()
            .map_or(Value::Undefined, |config| config.root_value.clone())
    }
}

/// Returns whether a value is a subschema, which has a transformed schema.
#[must_use]
pub fn is_subschema(value: &SchemaOrSubschemaConfig) -> bool {
    matches!(value, SchemaOrSubschemaConfig::Subschema(_))
}

/// Returns whether a value is a subschema configuration, which has a schema.
///
/// It holds for a configuration and for a subschema and not for a plain
/// schema.
#[must_use]
pub fn is_subschema_config(value: &SchemaOrSubschemaConfig) -> bool {
    !matches!(value, SchemaOrSubschemaConfig::Schema(_))
}

/// Returns the schema the transforms of a subschema configuration make of a wrapping schema.
#[must_use]
pub fn apply_schema_transforms(
    original_wrapping_schema: &Arc<Schema>,
    subschema_config: &SubschemaConfig,
) -> Arc<Schema> {
    let _ = &subschema_config.transforms;
    Arc::clone(original_wrapping_schema)
}
