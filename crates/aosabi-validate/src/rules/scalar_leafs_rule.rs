// SPDX-License-Identifier: MIT
/*
 * aosabi-validate: the rule `ScalarLeafsRule`.
 *
 * Derived from graphql-js 16.12.0, src/validation/rules/ScalarLeafsRule.ts, MIT.
 */

//! The `ScalarLeafsRule` of the validation layer.

use alloc::{
    boxed::Box,
    format,
    string::ToString,
    vec, //
};

use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions,
    ast::FieldNode,
    visitor::{
        EnterLeave,
        VisitContext,
        VisitResult,
        Visitor, //
    }, //
};
use aosabi_type::{
    Type,
    get_named_type,
    is_leaf_type, //
};

use crate::validation_context::{
    Rule,
    ValidationContext, //
};

/// Scalar leafs
///
/// A GraphQL document is valid only if all leaf fields (fields without
/// sub selections) are of scalar or enum types.
///
/// The visitor holds the `Field` hook, which reports a selection on a leaf
/// type and a missing or empty selection on any other type.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/ScalarLeafsRule.ts, MIT.
///
/// The reference prints the type with `inspect`, which answers the `toJSON`
/// of an object that has one, and the `toJSON` of a type is its `toString`.
/// The `Display` of a [`Type`] is that text.
pub struct ScalarLeafsRule<'c, 'a> {
    /// The context the rule reads and reports to.
    context: &'c ValidationContext<'a>,
}

impl<'c, 'a> Rule<'c, 'a> for ScalarLeafsRule<'c, 'a> {
    type Context = &'c ValidationContext<'a>;

    fn new(context: Self::Context) -> Self {
        Self { context }
    }
}

impl<'a> Visitor<'a> for ScalarLeafsRule<'_, 'a> {
    fn enter_field(&mut self, node: &'a FieldNode, _context: &VisitContext<'a, '_>) -> VisitResult {
        let Some(ty) = self.context.get_type() else {
            return VisitResult::Continue;
        };
        let selection_set = node.selection_set.as_ref();
        let field_name = node.name.value();
        let type_str = ty.to_string();
        if is_leaf_type(&Type::from(get_named_type(&ty))) {
            if let Some(selection_set) = selection_set {
                self.context.report_error(GraphQLError::new(
                    format!("Field \"{field_name}\" must not have a selection since type \"{type_str}\" has no subfields."),
                    GraphQLErrorOptions {
                        nodes: Some(vec![selection_set.clone().into()]),
                        ..GraphQLErrorOptions::default()
                    },
                ));
            }
        } else if selection_set.is_none() {
            self.context.report_error(GraphQLError::new(
                format!("Field \"{field_name}\" of type \"{type_str}\" must have a selection of subfields. Did you mean \"{field_name} {{ ... }}\"?"),
                GraphQLErrorOptions {
                    nodes: Some(vec![node.clone().into()]),
                    ..GraphQLErrorOptions::default()
                },
            ));
        } else if selection_set.is_some_and(|selection_set| selection_set.selections.is_empty()) {
            self.context.report_error(GraphQLError::new(
                format!("Field \"{field_name}\" of type \"{type_str}\" must have at least one field selected."),
                GraphQLErrorOptions {
                    nodes: Some(vec![node.clone().into()]),
                    ..GraphQLErrorOptions::default()
                },
            ));
        }
        VisitResult::Continue
    }
}

/// Creates the visitor of `ScalarLeafsRule` over `context`.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/ScalarLeafsRule.ts, MIT.
#[must_use]
pub fn scalar_leafs_rule<'c, 'a>(
    context: &'c ValidationContext<'a>,
) -> Box<dyn EnterLeave<'a> + 'c> {
    ScalarLeafsRule::boxed(context)
}
