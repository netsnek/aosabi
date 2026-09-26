// SPDX-License-Identifier: MIT
/*
 * aosabi-validate: the rule `PossibleFragmentSpreadsRule`.
 *
 * Derived from graphql-js 16.12.0, src/validation/rules/PossibleFragmentSpreadsRule.ts, MIT.
 */

//! The `PossibleFragmentSpreadsRule` of the validation layer.

use alloc::{
    boxed::Box,
    format,
    vec, //
};

use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions,
    ast::{
        FragmentSpreadNode,
        InlineFragmentNode,
        TypeNode, //
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
    NamedType,
    Type,
    do_types_overlap,
    is_composite_type,
    type_from_ast, //
};

use crate::validation_context::{
    Rule,
    ValidationContext, //
};

/// Possible fragment spread
///
/// A fragment spread is only valid if the type condition could ever possibly
/// be true: if there is a non-empty intersection of the possible parent types,
/// and possible types which pass the type condition.
///
/// The visitor holds the `InlineFragment` hook and the `FragmentSpread` hook,
/// and each reports a fragment whose type shares no possible type with the
/// parent type.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/PossibleFragmentSpreadsRule.ts, MIT.
///
/// The reference prints both types with `inspect`, which answers the `toJSON`
/// of an object that has one, and the `toJSON` of a named type is its name.
pub struct PossibleFragmentSpreadsRule<'c, 'a> {
    /// The context the rule reads and reports to.
    context: &'c ValidationContext<'a>,
}

impl<'c, 'a> Rule<'c, 'a> for PossibleFragmentSpreadsRule<'c, 'a> {
    type Context = &'c ValidationContext<'a>;

    fn new(context: Self::Context) -> Self {
        Self { context }
    }
}

impl<'a> PossibleFragmentSpreadsRule<'_, 'a> {
    /// Checks the inline fragment `node`, the body of the `InlineFragment`
    /// hook at lines 27-44.
    ///
    /// # Errors
    ///
    /// Returns what [`do_types_overlap()`] returns.
    fn check_inline_fragment(&self, node: &'a InlineFragmentNode) -> Result<(), DefinitionError> {
        let frag_type = self.context.get_type();
        let parent_type = self.context.get_parent_type();
        let (Some(frag_type), Some(parent_type)) = (
            frag_type.as_ref().and_then(as_composite_type),
            parent_type
                .map(Type::from)
                .as_ref()
                .and_then(as_composite_type),
        ) else {
            return Ok(());
        };
        if !do_types_overlap(self.context.get_schema(), &frag_type, &parent_type)? {
            let parent_type_str = parent_type.name();
            let frag_type_str = frag_type.name();
            self.context.report_error(GraphQLError::new(
                format!("Fragment cannot be spread here as objects of type \"{parent_type_str}\" can never be of type \"{frag_type_str}\"."),
                GraphQLErrorOptions {
                    nodes: Some(vec![node.clone().into()]),
                    ..GraphQLErrorOptions::default()
                },
            ));
        }
        Ok(())
    }

    /// Checks the fragment spread `node`, the body of the `FragmentSpread`
    /// hook at lines 45-63.
    ///
    /// # Errors
    ///
    /// Returns what [`do_types_overlap()`] returns.
    fn check_fragment_spread(&self, node: &'a FragmentSpreadNode) -> Result<(), DefinitionError> {
        let frag_name = node.name.value();
        let frag_type = get_fragment_type(self.context, frag_name);
        let parent_type = self.context.get_parent_type();
        let (Some(frag_type), Some(parent_type)) = (frag_type, parent_type) else {
            return Ok(());
        };
        if !do_types_overlap(self.context.get_schema(), &frag_type, &parent_type)? {
            let parent_type_str = parent_type.name();
            let frag_type_str = frag_type.name();
            self.context.report_error(GraphQLError::new(
                format!("Fragment \"{frag_name}\" cannot be spread here as objects of type \"{parent_type_str}\" can never be of type \"{frag_type_str}\"."),
                GraphQLErrorOptions {
                    nodes: Some(vec![node.clone().into()]),
                    ..GraphQLErrorOptions::default()
                },
            ));
        }
        Ok(())
    }
}

impl<'a> Visitor<'a> for PossibleFragmentSpreadsRule<'_, 'a> {
    fn enter_inline_fragment(
        &mut self,
        node: &'a InlineFragmentNode,
        _context: &VisitContext<'a, '_>,
    ) -> VisitResult {
        match self.check_inline_fragment(node) {
            Ok(()) => VisitResult::Continue,
            Err(error) => self.context.throw_error(error),
        }
    }

    fn enter_fragment_spread(
        &mut self,
        node: &'a FragmentSpreadNode,
        _context: &VisitContext<'a, '_>,
    ) -> VisitResult {
        match self.check_fragment_spread(node) {
            Ok(()) => VisitResult::Continue,
            Err(error) => self.context.throw_error(error),
        }
    }
}

/// Returns `ty` as a named type when it is composite, the type guard
/// `isCompositeType` of the reference read as a conversion.
fn as_composite_type(ty: &Type) -> Option<NamedType> {
    if is_composite_type(ty) {
        return ty.as_named();
    }
    None
}

/// Returns the composite type the fragment named `name` conditions on.
///
/// An unknown fragment, an unknown type and a type that is not composite
/// answer `None`, the `undefined` the reference falls through to.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/PossibleFragmentSpreadsRule.ts, MIT.
fn get_fragment_type(context: &ValidationContext<'_>, name: &str) -> Option<NamedType> {
    let frag = context.get_fragment(name)?;
    let ty = type_from_ast(
        context.get_schema(),
        &TypeNode::NamedType(frag.type_condition.clone()),
    )?;
    as_composite_type(&ty)
}

/// Creates the visitor of `PossibleFragmentSpreadsRule` over `context`.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/PossibleFragmentSpreadsRule.ts, MIT.
#[must_use]
pub fn possible_fragment_spreads_rule<'c, 'a>(
    context: &'c ValidationContext<'a>,
) -> Box<dyn EnterLeave<'a> + 'c> {
    PossibleFragmentSpreadsRule::boxed(context)
}
