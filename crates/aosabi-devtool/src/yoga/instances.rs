// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool yoga: the Yoga instances of the in process capture.
 */

//! The instances by name.
//!
//! | Name | Schema | Options beside the base options |
//! | --- | --- | --- |
//! | `prod` | first | none, masking on |
//! | `dev` | first | `maskedErrors: false` |
//! | `graphiqlFalse` | first | `graphiql: false` |
//! | `batching` | first | `batching: true` |
//! | `noData` | first | a plugin that throws in `onParams` for `{__boom__}` |
//! | `second` | second | none |
//! | `secondBatchingLimit2` | second | `batching: {limit: 2}` |
//! | `secondBatchingTrue` | second | `batching: true` |
//! | `secondNoIntrospection` | second | `useDisableIntrospection()` |
//!
//! The [`Switches`] of a run lay two choices over every instance. The switch
//! of the single pass sets the field `single_pass` of the options, on or off,
//! hands each instance counters of its verdicts, and adds the two probe
//! instances of [`super::pass_probe`], `passReplace` and `passInvalidSchema`,
//! which exist only under it. The switch of the request cache sets
//! `parserAndValidationCache: false` where it is off, on the nine instances
//! above. Without a switch every instance is the one above, whose engine
//! follows the default of `graphql-over-http`. Every instance takes the bound on
//! nesting of the card this build serves, `aosabi_core::single_pass::TARGET`,
//! as a served build does.

use std::{
    rc::Rc,
    sync::Arc, //
};

use aosabi_core::single_pass;
use aosabi_execute::{
    ErrorObject,
    Value, //
};
use aosabi_hooks::{
    HookContext,
    HookPlugin,
    plugins::use_engine::PassCounts, //
};
use aosabi_http::Request;
use aosabi_type::Schema;
use graphql_over_http::{
    AgentSetup,
    Graphiql,
    GraphiqlOptions,
    HttpOptions,
    HttpPlugin,
    HttpServer,
    MaskedErrors,
    ServerAdapterPlugin,
    plugin::OnParams,
    use_disable_introspection, //
};

use super::{
    capture_schema::{
        first_agent,
        first_schema,
        second_agent,
        second_schema, //
    },
    pass_probe, //
};

/// The names of the instances, first half then second half.
pub const NAMES: [&str; 9] = [
    "prod",
    "dev",
    "graphiqlFalse",
    "batching",
    "noData",
    "second",
    "secondBatchingLimit2",
    "secondBatchingTrue",
    "secondNoIntrospection",
];

/// The choices a run lays over every instance.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Switches {
    /// Whether the engine validates through the single pass, `None` for the
    /// default of `graphql-over-http` with no counters and no probe instance.
    pub single_pass: Option<bool>,
    /// Whether the request cache of Yoga is installed, `None` for the
    /// default, which installs it.
    pub cache: Option<bool>,
}

/// One instance and the counters of its engine.
struct Instance {
    /// The name.
    name: &'static str,
    /// The server.
    server: Rc<HttpServer>,
    /// The counters of the single pass, where the switch of the single pass
    /// is given.
    counts: Option<Rc<PassCounts>>,
}

/// The instances of one run, built once each.
pub struct Instances {
    /// The instances in the order of [`NAMES`], then the probe instances.
    instances: Vec<Instance>,
}

impl Instances {
    /// Builds every instance with no switch.
    ///
    /// # Errors
    ///
    /// Returns the message of a schema that does not build.
    pub fn new() -> Result<Self, String> {
        Self::with(Switches::default())
    }

    /// Builds every instance under `switches`.
    ///
    /// # Errors
    ///
    /// Returns the message of a schema that does not build.
    pub fn with(switches: Switches) -> Result<Self, String> {
        let first = first_schema()?;
        let second = second_schema()?;
        let mut instances = Vec::with_capacity(NAMES.len() + pass_probe::NAMES.len());
        for name in NAMES {
            let (schema, agent) = if name.starts_with("second") {
                (Arc::clone(&second), second_agent())
            } else {
                (Arc::clone(&first), first_agent())
            };
            let mut options = options(name, schema, agent);
            if switches.cache == Some(false) {
                options.parser_and_validation_cache = Some(false);
            }
            instances.push(Instance::create(name, options, switches));
        }
        if switches.single_pass.is_some() {
            for name in pass_probe::NAMES {
                let options = pass_probe::options(name)?
                    .ok_or_else(|| format!("no probe instance is named {name}"))?;
                instances.push(Instance::create(name, options, switches));
            }
        }
        Ok(Self { instances })
    }

    /// Returns the instance named `name`.
    pub fn get(&self, name: &str) -> Option<&Rc<HttpServer>> {
        self.find(name).map(|instance| &instance.server)
    }

    /// Returns the counters of the single pass of the instance named
    /// `name`, which exist where the switch of the single pass is given.
    pub fn counts(&self, name: &str) -> Option<&Rc<PassCounts>> {
        self.find(name)?.counts.as_ref()
    }

    /// Returns the instance named `name`.
    fn find(&self, name: &str) -> Option<&Instance> {
        self.instances.iter().find(|instance| instance.name == name)
    }
}

impl Instance {
    /// Returns the instance `name` of `options` with the switch of the single
    /// pass of `switches` and its counters, and with the bound on nesting of
    /// the card this build serves, as `build_http_options` of `aosabi-core` sets it.
    fn create(name: &'static str, options: HttpOptions, switches: Switches) -> Self {
        let mut options = options;
        let counts = switches.single_pass.map(|_| Rc::new(PassCounts::default()));
        options.single_pass = switches.single_pass;
        options.single_pass_counts.clone_from(&counts);
        options.single_pass_max_depth = Some(single_pass::TARGET);
        Self {
            name,
            server: HttpServer::new(options),
            counts,
        }
    }
}

/// Returns the options of the instance `name`, the base options with the
/// variation the capture gives it.
fn options(name: &str, schema: Arc<Schema>, agent: AgentSetup) -> HttpOptions {
    let graphiql = Graphiql::Factory(Rc::new(|_: &Request, _: &HookContext| {
        Some(GraphiqlOptions {
            should_persist_headers: Some(true),
            title: Some(String::from("Aosabi Playground")),
            default_query: Some(String::from("# Welcome to the Aosabi Playground!")),
        })
    }));
    let mut options = HttpOptions {
        graphql_endpoint: Some(String::from("/graphql")),
        graphiql,
        schema: Some(schema),
        agent,
        ..HttpOptions::default()
    };
    match name {
        "dev" => options.masked_errors = MaskedErrors::Off,
        "graphiqlFalse" => options.graphiql = Graphiql::Off,
        "noData" => options.plugins = vec![Rc::new(Boom)],
        "secondNoIntrospection" => options.plugins = vec![use_disable_introspection()],
        _ => {}
    }
    options
}

/// The plugin of the instance `noData`, which throws a plain error in
/// `onParams` for the document `{__boom__}`, so the result carries no data key.
struct Boom;

impl HookPlugin for Boom {}

impl ServerAdapterPlugin for Boom {}

impl HttpPlugin for Boom {
    fn on_params(&self, params: &mut OnParams<'_>) -> Result<(), Value> {
        if params.params.query().and_then(|query| query.as_str()) == Some("{__boom__}") {
            return Err(Value::from(ErrorObject::error(
                "plugin blew up before execution",
            )));
        }
        Ok(())
    }
}
