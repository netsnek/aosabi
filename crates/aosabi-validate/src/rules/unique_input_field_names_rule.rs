// SPDX-License-Identifier: MIT
/*
 * aosabi-validate: the rule `UniqueInputFieldNamesRule`.
 *
 * Derived from graphql-js 16.12.0, src/validation/rules/UniqueInputFieldNamesRule.ts, MIT.
 */

//! The `UniqueInputFieldNamesRule` of the validation layer.

use alloc::{
    boxed::Box,
    format,
    string::String,
    vec,
    vec::Vec, //
};
use core::mem;

use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions,
    ast::{
        AstNode,
        NameNode,
        ObjectFieldNode,
        ObjectValueNode, //
    },
    visitor::{
        EnterLeave,
        VisitContext,
        VisitResult,
        Visitor, //
    }, //
};
use aosabi_type::{
    DefinitionError,
    ObjMap, //
};

use crate::validation_context::{
    AstValidationContext,
    Rule, //
};

/// Unique input field names
///
/// A GraphQL input object value is only valid if all supplied fields are
/// uniquely named.
///
/// See <https://spec.graphql.org/draft/#sec-Input-Object-Field-Uniqueness>
///
/// The name maps of the reference are null prototype objects, so every field
/// name is an ordinary key, `__proto__` included, because that accessor
/// belongs to `Object.prototype` and such an object does not inherit it.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/UniqueInputFieldNamesRule.ts, MIT.
pub struct UniqueInputFieldNamesRule<'c, 'a> {
    /// The context the rule reads and reports to.
    context: &'c AstValidationContext<'a>,
    /// The names of the enclosing object values, the `knownNameStack` of the
    /// reference.
    known_name_stack: Vec<ObjMap<&'a NameNode>>,
    /// The names of the object value the walk is in, the `knownNames` of the
    /// reference.
    known_names: ObjMap<&'a NameNode>,
}

impl<'c, 'a> Rule<'c, 'a> for UniqueInputFieldNamesRule<'c, 'a> {
    type Context = &'c AstValidationContext<'a>;

    fn new(context: Self::Context) -> Self {
        Self {
            context,
            known_name_stack: Vec::new(),
            known_names: ObjMap::default(),
        }
    }
}

impl<'a> Visitor<'a> for UniqueInputFieldNamesRule<'_, 'a> {
    fn enter_object_value(
        &mut self,
        _node: &'a ObjectValueNode,
        _: &VisitContext<'a, '_>,
    ) -> VisitResult {
        let known_names = mem::take(&mut self.known_names);
        self.known_name_stack.push(known_names);
        VisitResult::Continue
    }

    fn leave_object_value(
        &mut self,
        _node: &'a ObjectValueNode,
        _: &VisitContext<'a, '_>,
    ) -> VisitResult {
        let Some(prev_known_names) = self.known_name_stack.pop() else {
            // The `invariant` of the reference, which throws its default
            // message. The walk leaves an object value only after it entered
            // it, so no walk reaches this.
            return self
                .context
                .throw_error(DefinitionError::plain("Unexpected invariant triggered."));
        };
        self.known_names = prev_known_names;
        VisitResult::Continue
    }

    fn enter_object_field(
        &mut self,
        node: &'a ObjectFieldNode,
        _: &VisitContext<'a, '_>,
    ) -> VisitResult {
        let field_name = node.name.value();
        if let Some(known_name) = self.known_names.get(field_name) {
            self.context.report_error(GraphQLError::new(
                format!("There can be only one input field named \"{field_name}\"."),
                GraphQLErrorOptions {
                    nodes: Some(vec![
                        AstNode::from((*known_name).clone()),
                        AstNode::from(node.name.clone()),
                    ]),
                    ..GraphQLErrorOptions::default()
                },
            ));
        } else {
            self.known_names
                .insert(String::from(field_name), &node.name);
        }
        VisitResult::Continue
    }
}

/// Creates the visitor of `UniqueInputFieldNamesRule` over `context`.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/UniqueInputFieldNamesRule.ts, MIT.
#[must_use]
pub fn unique_input_field_names_rule<'c, 'a>(
    context: &'c AstValidationContext<'a>,
) -> Box<dyn EnterLeave<'a> + 'c> {
    UniqueInputFieldNamesRule::boxed(context)
}
