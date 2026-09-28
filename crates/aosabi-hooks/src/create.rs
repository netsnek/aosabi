// SPDX-License-Identifier: MIT
/*
 * aosabi-hooks: the entry of the hook layer, which builds the orchestrator of
 * a list of plugins and the functions of each request.
 */

//! [`hooks`], the entry of the crate.
//!
//! [`hooks`] builds the orchestrator of a list of plugins, and [`Hooks::hooked`] runs
//! `on_enveloped` over the context of a request and answers the parse, validate, context
//! factory, execute and subscribe of that request with the current schema, [`Hooked`], as
//! the function `getEnveloped` of the reference answers them. The list of the plugins after
//! every splice is [`Hooks::plugins`], the `_plugins` the HTTP server reads its own hooks
//! from.

use std::{
    rc::Rc,
    sync::Arc, //
};

use aosabi_execute::{
    Agent,
    ExecutionArgs,
    Value, //
};
use aosabi_lang::{
    GraphQLError,
    ast::DocumentNode, //
};
use aosabi_type::Schema;

use crate::{
    context::HookContext,
    orchestrator::{
        Orchestrator,
        create_hook_orchestrator, //
    },
    plugin::{
        ExecutionOutcome,
        HookPlugin,
        LocalBoxFuture, //
    }, //
};

/// The hook layer of a list of plugins, `envelop({ plugins })`.
pub struct Hooks {
    /// The orchestrator of the plugins.
    orchestrator: Rc<Orchestrator>,
}

/// Returns the hook layer of `plugins`, `envelop({ plugins })`.
///
/// The reference drops the null entries of the array first, which a vector of plugins has
/// none of.
#[must_use]
pub fn hooks(plugins: Vec<Rc<dyn HookPlugin>>) -> Hooks {
    Hooks {
        orchestrator: create_hook_orchestrator(plugins),
    }
}

impl Hooks {
    /// Returns the plugins after every splice, `getEnveloped._plugins`.
    #[must_use]
    pub fn plugins(&self) -> &[Rc<dyn HookPlugin>] {
        self.orchestrator.plugins()
    }

    /// Returns the functions of a request over `initial_context`, `getEnveloped(context)`.
    ///
    /// `onEnveloped` of every plugin runs over the context first.
    ///
    /// # Errors
    ///
    /// Returns the value an `onEnveloped` hook throws.
    pub fn hooked(&self, initial_context: HookContext) -> Result<Hooked, Value> {
        self.orchestrator.init(&initial_context)?;
        Ok(Hooked {
            orchestrator: Rc::clone(&self.orchestrator),
            schema: self.orchestrator.current_schema(),
            context: initial_context,
        })
    }
}

/// The functions of one request, the parse, validate, context factory, execute and subscribe it runs.
pub struct Hooked {
    /// The orchestrator of the plugins.
    orchestrator: Rc<Orchestrator>,
    /// The schema when the functions were made.
    schema: Option<Arc<Schema>>,
    /// The initial context of the request.
    context: HookContext,
}

impl Hooked {
    /// Returns the schema, `enveloped.schema`.
    #[must_use]
    pub fn schema(&self) -> Option<Arc<Schema>> {
        self.schema.clone()
    }

    /// Returns the initial context of the request.
    #[must_use]
    pub fn context(&self) -> &HookContext {
        &self.context
    }

    /// Parses `source`, `enveloped.parse(source)`.
    ///
    /// # Errors
    ///
    /// Returns the parse error or the value a hook throws.
    pub fn parse(&self, source: &str) -> Result<Arc<DocumentNode>, Value> {
        self.orchestrator.parse(&self.context, source)
    }

    /// Validates `document` against `schema` with the rules of the validate function, `enveloped.validate(schema, document)`.
    ///
    /// # Errors
    ///
    /// Returns the value the validate function throws.
    pub fn validate(
        &self,
        schema: &Arc<Schema>,
        document: &DocumentNode,
    ) -> Result<Vec<GraphQLError>, Value> {
        self.orchestrator
            .validate(&self.context, schema, document, None)
    }

    /// Builds the context of the request, `enveloped.contextFactory()`.
    #[must_use]
    pub fn context_factory(&self) -> LocalBoxFuture<'static, Result<HookContext, Value>> {
        self.orchestrator.context_factory(self.context.clone())
    }

    /// Runs the executor on `agent` and drives the agent until the result settles, `enveloped.execute(args)`.
    #[must_use]
    pub fn execute(
        &self,
        agent: &Agent,
        args: ExecutionArgs,
    ) -> LocalBoxFuture<'static, Result<ExecutionOutcome, Value>> {
        self.orchestrator.execute(agent, args)
    }

    /// Runs the subscription entry on `agent`, `enveloped.subscribe(args)`, which answers a
    /// stream for a subscription.
    #[must_use]
    pub fn subscribe(
        &self,
        agent: &Agent,
        args: ExecutionArgs,
    ) -> LocalBoxFuture<'static, Result<ExecutionOutcome, Value>> {
        self.orchestrator.subscribe(agent, args)
    }
}
