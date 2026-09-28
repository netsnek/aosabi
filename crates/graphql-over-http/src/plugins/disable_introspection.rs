// SPDX-License-Identifier: MIT
/*
 * graphql-over-http: the refusal of every introspection field but __typename.
 *
 * Derived from graphql-yoga 2.19.0, cjs/index.js, MIT.
 * Derived from graphql-js 16.12.0, src/validation/rules/custom/NoSchemaIntrospectionCustomRule.ts, MIT.
 */

//! `use_disable_introspection`.
//!
//! The plugin marks every request in `onRequestParse` and adds the rule
//! `NoSchemaIntrospectionCustomRule` to the validation of a marked request,
//! which the app installs when its config sets `graphiql: false`. The rule
//! reports every field whose type is an introspection type, so `__schema`
//! and `__type` are refused once per field and `__typename`, a `String!`,
//! stays allowed. The refusal is a validation error, so the HTTP validation
//! error plugin stamps it. The object of the reference carries the two hooks
//! and no other key, so the plugin answers `onValidate` and `onRequestParse`
//! as its hooks, the members the boot report lists for it.

use core::cell::RefCell;
use std::rc::{
    Rc,
    Weak, //
};

use aosabi_execute::Value;
use aosabi_hooks::{
    HookPlugin,
    plugin::{
        OnValidate,
        OnValidateDoneHook, //
    }, //
};
use aosabi_http::Request;
use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions,
    ast::FieldNode,
    visitor::{
        VisitContext,
        VisitResult,
        Visitor, //
    }, //
};
use aosabi_type::{
    get_named_type,
    introspection::is_introspection_type, //
};
use aosabi_validate::{
    Rule,
    ValidationContext,
    ValidationRule, //
};

use crate::{
    plugin::{
        HttpPlugin,
        OnRequestParse,
        OnRequestParseDoneHook, //
    },
    server_adapter::ServerAdapterPlugin, //
};

/// Prohibit introspection queries.
///
/// A GraphQL document is only valid if all fields selected are not fields
/// that return an introspection type.
pub struct NoSchemaIntrospectionCustomRule<'c, 'a> {
    /// The context the rule reads and reports to.
    context: &'c ValidationContext<'a>,
}

impl<'c, 'a> Rule<'c, 'a> for NoSchemaIntrospectionCustomRule<'c, 'a> {
    type Context = &'c ValidationContext<'a>;

    fn new(context: Self::Context) -> Self {
        Self { context }
    }
}

impl<'a> Visitor<'a> for NoSchemaIntrospectionCustomRule<'_, 'a> {
    fn enter_field(&mut self, node: &'a FieldNode, _context: &VisitContext<'a, '_>) -> VisitResult {
        if let Some(ty) = self.context.get_type()
            && is_introspection_type(&get_named_type(&ty))
        {
            self.context.report_error(GraphQLError::new(
                format!(
                    "GraphQL introspection has been disabled, but the requested query contained the field \"{}\".",
                    node.name.value()
                ),
                GraphQLErrorOptions {
                    nodes: Some(vec![node.clone().into()]),
                    ..GraphQLErrorOptions::default()
                },
            ));
        }
        VisitResult::Continue
    }
}

/// The rule value, `NoSchemaIntrospectionCustomRule`.
pub const NO_SCHEMA_INTROSPECTION_CUSTOM_RULE: ValidationRule = ValidationRule {
    name: "NoSchemaIntrospectionCustomRule",
    create: |context| NoSchemaIntrospectionCustomRule::boxed(context),
};

/// The introspection refusal plugin.
struct UseDisableIntrospection {
    /// The requests marked for the refusal, `disabledIntrospection`, held weakly as a `WeakSet` holds them.
    disabled: RefCell<Vec<Weak<Request>>>,
}

impl HookPlugin for UseDisableIntrospection {
    fn on_validate(&self, p: &mut OnValidate<'_>) -> Option<OnValidateDoneHook> {
        let request = p.context.get_as::<Request>("request");
        let is_disabled = request.is_some_and(|request| {
            self.disabled
                .borrow()
                .iter()
                .any(|known| known.as_ptr() == Rc::as_ptr(&request))
        });
        if is_disabled {
            p.add_validation_rule(NO_SCHEMA_INTROSPECTION_CUSTOM_RULE);
        }
        None
    }
}

impl ServerAdapterPlugin for UseDisableIntrospection {}

impl HttpPlugin for UseDisableIntrospection {
    fn on_request_parse(
        &self,
        p: &mut OnRequestParse<'_>,
    ) -> Result<Option<OnRequestParseDoneHook>, Value> {
        let mut disabled = self.disabled.borrow_mut();
        disabled.retain(|known| known.strong_count() > 0);
        disabled.push(Rc::downgrade(p.request));
        Ok(None)
    }

    /// Returns `onValidate` and `onRequestParse`, the two keys of the object
    /// `useDisableIntrospection` returns.
    fn hooks(&self) -> Vec<&'static str> {
        vec!["onValidate", "onRequestParse"]
    }
}

/// Returns the introspection refusal plugin for every request, `useDisableIntrospection()`.
#[must_use]
pub fn use_disable_introspection() -> Rc<dyn HttpPlugin> {
    Rc::new(UseDisableIntrospection {
        disabled: RefCell::new(Vec::new()),
    })
}
