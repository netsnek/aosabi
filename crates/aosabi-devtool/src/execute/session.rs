// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool execute session: the behaviour tables of one run, each built
 * into the schema and the executor its requests run through.
 *
 * No analogue in the reference.
 */

//! The tables of one run.
//!
//! A [`Session`] holds every table a run loads by its name. A table of
//! [`Mode::Plain`] becomes a [`GeneralExecutor`] over a schema whose field
//! resolvers are the specs of the table, with no scalar bound. A table of
//! [`Mode::Service`] becomes a [`Service`], so its root objects go through the
//! resolver wrapping of aosabi-core and the seven scalars it adds are bound,
//! and the factory of its root `Query` stays beside the service for the
//! operation `project`. Both run a request through `run_request` of aosabi-execute, which is the
//! one pipeline a served application runs through too, and a subscription
//! through `run_subscription` of the same crate, which the service reaches
//! through [`Service::subscribe`] with the policy of Network.
//!
//! A member of `Subscription` in a [`Mode::Plain`] table is the subscribe
//! resolver of its field, and the field keeps the default resolver, which
//! reads the member of the field's name off each event, the reading
//! graphql-js gives a subscription field with no resolver.
//!
//! A session also holds root factories outside every schema, each made from
//! one root of a [`Mode::Service`] table as its service would make it, so a
//! run can drop a factory the table of root members of aosabi-core holds
//! entries of.

use std::{
    rc::Rc,
    sync::Arc, //
};

use aosabi_core::{
    CreateSchemaError,
    ResolverMap,
    Resolvers,
    Service,
    carrier::CarrierPolicy,
    make_executable_schema,
    resolvers::resolvers_to_graphql_resolvers::read_root_member, //
};
use aosabi_execute::{
    execute::{
        FieldResolve,
        FieldResolverFn,
        SharedSchema, //
    },
    kernel_entry::{
        Executed,
        GeneralExecutor,
        Request,
        RootFactory,
        RunFailure,
        Subscribed,
        run_request,
        run_subscription, //
    },
    promise::{
        Agent,
        AgentOptions, //
    },
    value::{
        Completion,
        Value, //
    }, //
};
use aosabi_type::{
    ObjMap,
    ResolverHandle,
    Schema,
    schema_bindings::FieldBinding, //
};

use crate::execute::{
    interpret::{
        CALLED,
        Frame,
        materialize,
        note_agent, //
    },
    table::{
        Mode,
        Spec,
        SpecKind,
        Table, //
    }, //
};

/// Why a table did not load.
#[derive(Debug)]
pub enum LoadError {
    /// The name is taken by a table the run loaded before, which is a mistake
    /// of the harness.
    NameTaken(String),
    /// The schema does not build or the resolvers name a type or a field it
    /// lacks, which is an answer of the crate under test.
    Schema(CreateSchemaError),
}

/// Why a root was not held, each a mistake of the harness.
#[derive(Debug)]
pub enum HoldError {
    /// The name is taken by a table or a root the run holds already.
    NameTaken(String),
    /// The table is of [`Mode::Plain`], whose resolvers are no root objects.
    NotService(String),
    /// The table names no root of that type.
    NoRoot(String),
}

/// What runs the requests of one table.
enum Runner {
    /// The executor of a table of [`Mode::Plain`].
    Plain(GeneralExecutor),
    /// The service of a table of [`Mode::Service`].
    Service {
        /// The service.
        service: Service,
        /// The factory of the root `Query` the service holds, which
        /// [`Session::query_root`] answers.
        query: Option<RootFactory>,
    },
}

/// The tables of one run, by name, and the root factories it holds outside every schema.
#[derive(Default)]
pub struct Session {
    /// The runner of every loaded table.
    tables: ObjMap<Runner>,
    /// The root factories held by [`Session::hold`], by the name of their table.
    roots: ObjMap<RootFactory>,
}

impl Session {
    /// Builds `table` and keeps it under its name.
    ///
    /// # Errors
    ///
    /// Returns [`LoadError::NameTaken`] for a name the session holds already,
    /// and [`LoadError::Schema`] with what the schema build throws.
    pub fn load(&mut self, table: Table) -> Result<(), LoadError> {
        if self.holds(&table.name) {
            return Err(LoadError::NameTaken(table.name));
        }
        let name = table.name.clone();
        let runner = match table.mode {
            Mode::Plain => plain_runner(&table),
            Mode::Service => service_runner(table),
        };
        self.tables.insert(name, runner.map_err(LoadError::Schema)?);
        Ok(())
    }

    /// Builds the factory of the root `type_name` of `table` outside every schema and holds it.
    ///
    /// The factory is the one the service of the table would hold. Each
    /// member of the root is read once through the table of root members of
    /// the thread, as a root field reads it, and the factory is held under the
    /// name of the table until [`Session::remove`] drops it, which drops the
    /// factory, since no schema holds it. Returns the count of the members
    /// read.
    ///
    /// # Errors
    ///
    /// Returns [`HoldError::NameTaken`] for a name the session holds already,
    /// [`HoldError::NotService`] for a table of [`Mode::Plain`] and
    /// [`HoldError::NoRoot`] for a root the table does not name.
    pub fn hold(&mut self, mut table: Table, type_name: &str) -> Result<usize, HoldError> {
        if self.holds(&table.name) {
            return Err(HoldError::NameTaken(table.name));
        }
        if table.mode != Mode::Service {
            return Err(HoldError::NotService(table.name));
        }
        let Some(factory) = root_factory(&mut table, type_name) else {
            return Err(HoldError::NoRoot(table.name));
        };
        // A service reads the names of the members of a root from one call of
        // its factory when it is built.
        let keys = match factory() {
            Value::Object(object) => object.own_keys(),
            _ => Vec::new(),
        };
        for key in &keys {
            drop(read_root_member(&factory, key));
        }
        self.roots.insert(table.name, factory);
        Ok(keys.len())
    }

    /// Drops the table `name` with its executor or its service, or the root held under `name`.
    ///
    /// Returns false when the session holds no table and no root of that name.
    /// The members a thread reads from a factory that is gone leave the table
    /// of root members of aosabi-core at the next fill of the thread. A
    /// dropped root of [`Session::hold`] is such a factory. A schema built
    /// from SDL holds its types in a reference cycle with the scope of its
    /// build, which holds the bindings, so the factories of a dropped table of
    /// [`Mode::Service`] outlive its service.
    pub fn remove(&mut self, name: &str) -> bool {
        self.tables.shift_remove(name).is_some() || self.roots.shift_remove(name).is_some()
    }

    /// Returns whether the session holds a table or a root named `name`.
    fn holds(&self, name: &str) -> bool {
        self.tables.contains_key(name) || self.roots.contains_key(name)
    }

    /// Runs `request` against the table `name` on a fresh agent.
    ///
    /// Returns `None` when the session holds no table of that name.
    pub fn execute(
        &self,
        name: &str,
        request: &Request<'_>,
        options: AgentOptions,
    ) -> Option<Executed> {
        let executed = match self.tables.get(name)? {
            Runner::Plain(executor) => {
                // No spec reads the context value, so a plain table runs with
                // one that holds nothing.
                run_request(executor, request, Rc::new(()), options, &|_| {})
            }
            Runner::Service { service, .. } => service.execute(request, options),
        };
        Some(executed)
    }

    /// Subscribes `request` against the table `name` on a fresh agent.
    ///
    /// A table of [`Mode::Service`] subscribes through its service with the
    /// policy of Network, which allows every field, as a request of HTTP(TCP)
    /// carries it. Returns `None` when the session holds no table of that
    /// name.
    pub fn subscribe(
        &self,
        name: &str,
        request: &Request<'_>,
        options: AgentOptions,
    ) -> Option<Result<Subscribed, RunFailure>> {
        let subscribed = match self.tables.get(name)? {
            Runner::Plain(executor) => {
                run_subscription(executor, request, Rc::new(()), options, &|_| {})
            }
            Runner::Service { service, .. } => {
                service.subscribe(request, options, CarrierPolicy::NETWORK)
            }
        };
        Some(subscribed)
    }

    /// Returns the schema of the table `name`, `None` when the session holds no such table.
    pub fn schema(&self, name: &str) -> Option<&Arc<Schema>> {
        match self.tables.get(name)? {
            Runner::Plain(executor) => Some(&executor.schema),
            Runner::Service { service, .. } => Some(service.schema()),
        }
    }

    /// Returns the schema and the factory of the root `Query` of the table
    /// `name`, `None` when the session holds no such table of
    /// [`Mode::Service`].
    pub fn query_root(&self, name: &str) -> Option<(&SharedSchema, &RootFactory)> {
        match self.tables.get(name)? {
            Runner::Plain(_) => None,
            Runner::Service { service, query } => Some((service.schema(), query.as_ref()?)),
        }
    }
}

/// Builds the executor of a table of [`Mode::Plain`].
///
/// The schema is built the way `makeExecutableSchema` builds it, with no
/// scalar bound, so a custom scalar of the SDL keeps its identity hooks. No
/// root value enters, as on the node side.
fn plain_runner(table: &Table) -> Result<Runner, CreateSchemaError> {
    let mut resolvers = ResolverMap::default();
    for (type_name, fields) in &table.resolvers {
        let subscription = type_name == "Subscription";
        let bindings = fields
            .iter()
            .map(|(field_name, spec)| (field_name.clone(), plain_binding(spec, subscription)));
        resolvers
            .fields
            .insert(type_name.clone(), bindings.collect());
    }
    let schema = make_executable_schema(&table.sdl, &resolvers)?;
    Ok(Runner::Plain(GeneralExecutor {
        schema,
        root_value: Arc::new(|| Value::Undefined),
        leaf_serializers: None,
    }))
}

/// Returns the binding of a field whose resolver body is `spec`.
///
/// The body is made on every call, with the argument object of the field as
/// the one argument an `arg` spec reads. A field of `Subscription` takes the
/// body as its subscribe resolver and keeps the default resolver.
fn plain_binding(spec: &Arc<Spec>, subscription: bool) -> FieldBinding {
    let spec = Arc::clone(spec);
    let resolver: FieldResolverFn = Arc::new(move |agent, _source, args, _context, _info| {
        note_agent(agent);
        agent.record(&spec.label, CALLED);
        let args = [Value::from(Rc::clone(args))];
        let frame = Frame {
            agent,
            mode: Mode::Plain,
            args: &args,
        };
        materialize(&spec, frame)
    });
    let handle = Some(ResolverHandle::new(FieldResolve(resolver)));
    if subscription {
        return FieldBinding {
            resolve: None,
            subscribe: handle,
        };
    }
    FieldBinding {
        resolve: handle,
        subscribe: None,
    }
}

/// Builds the service of a table of [`Mode::Service`].
fn service_runner(mut table: Table) -> Result<Runner, CreateSchemaError> {
    let query = root_factory(&mut table, "Query");
    let roots = Resolvers {
        query: query.clone(),
        mutation: root_factory(&mut table, "Mutation"),
        subscription: root_factory(&mut table, "Subscription"),
    };
    // A service keeps its SDL text for the life of the process, so the text
    // of a table outlives the table, also one the run drops.
    let type_defs: &'static str = Box::leak(table.sdl.into_boxed_str());
    let service = Service::builder(type_defs).roots(roots).build()?;
    Ok(Runner::Service { service, query })
}

/// Returns the factory of the root object `type_name`, `None` when the table
/// names no such root.
///
/// The root is one `object` spec over the members of the type. A factory is
/// called once when the service is built, outside every request, and once by
/// the first call of a field of the root in the thread, whose members the
/// thread keeps for every later call. The grammar keeps every timer, promise
/// and trace event of a service table inside the `returns` of a `fn`, so the
/// agent a root is made with is never used and the root made for the build is
/// the value a request sees.
fn root_factory(table: &mut Table, type_name: &str) -> Option<RootFactory> {
    let members = table.resolvers.shift_remove(type_name)?;
    let root = Spec {
        label: format!("/resolvers/{type_name}"),
        kind: SpecKind::Object(members.into_iter().collect()),
    };
    Some(Arc::new(move || {
        let agent = Agent::current().unwrap_or_else(|| Agent::new(AgentOptions::default()));
        let frame = Frame {
            agent: &agent,
            mode: Mode::Service,
            args: &[],
        };
        match materialize(&root, frame) {
            Completion::Normal(root) => root,
            // The grammar admits no throw outside a `fn` of a service table.
            Completion::Throw(_) => Value::Undefined,
        }
    }))
}
