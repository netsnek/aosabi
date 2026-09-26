// SPDX-License-Identifier: MIT
/*
 * aosabi-validate: the rule `NoFragmentCyclesRule`.
 *
 * Derived from graphql-js 16.12.0, src/validation/rules/NoFragmentCyclesRule.ts, MIT.
 */

//! The `NoFragmentCyclesRule` of the validation layer.

use alloc::{
    boxed::Box,
    format,
    string::String,
    vec::Vec, //
};

use aosabi_json::{
    map::IndexMap,
    value::KeyHasherBuilder, //
};
use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions,
    ast::{
        AstNode,
        FragmentDefinitionNode,
        FragmentSpreadNode,
        OperationDefinitionNode, //
    },
    visitor::{
        EnterLeave,
        VisitContext,
        VisitResult,
        Visitor, //
    }, //
};

use crate::validation_context::{
    AstValidationContext,
    Rule, //
};

/// No fragment cycles
///
/// The graph of fragment spreads must not form any cycles including spreading
/// itself. Otherwise an operation could infinitely spread or infinitely execute
/// on cycles in the underlying data.
///
/// See <https://spec.graphql.org/draft/#sec-Fragment-spreads-must-not-form-cycles>
///
/// The visitor holds the `OperationDefinition` hook, which skips the operation,
/// and the `FragmentDefinition` hook, which searches the spreads below the
/// fragment for a cycle and skips its children.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/NoFragmentCyclesRule.ts, MIT.
pub struct NoFragmentCyclesRule<'c, 'a> {
    /// The context the rule reads and reports to.
    context: &'c AstValidationContext<'a>,
    /// Tracks already visited fragments to maintain O(N) and to ensure that
    /// cycles are not redundantly reported.
    visited_frags: IndexMap<&'a str, bool, KeyHasherBuilder>,
    /// Array of AST nodes used to produce meaningful errors
    spread_path: Vec<&'a FragmentSpreadNode>,
    /// Position in the spread path
    spread_path_index_by_name: IndexMap<&'a str, Option<usize>, KeyHasherBuilder>,
}

impl<'c, 'a> Rule<'c, 'a> for NoFragmentCyclesRule<'c, 'a> {
    type Context = &'c AstValidationContext<'a>;

    fn new(context: Self::Context) -> Self {
        Self {
            context,
            visited_frags: IndexMap::default(),
            spread_path: Vec::new(),
            spread_path_index_by_name: IndexMap::default(),
        }
    }
}

impl<'a> NoFragmentCyclesRule<'_, 'a> {
    /// Searches the spreads below `fragment` for a cycle.
    ///
    /// This does a straight-forward DFS to find cycles.
    /// It does not terminate when a cycle was found but continues to explore
    /// the graph to find all possible cycles.
    ///
    /// Derived from graphql-js 16.12.0, src/validation/rules/NoFragmentCyclesRule.ts, MIT.
    fn detect_cycle_recursive(&mut self, fragment: &'a FragmentDefinitionNode) {
        let fragment_name = fragment.name.value();
        if self.visited_frags.get(fragment_name) == Some(&true) {
            return;
        }
        self.visited_frags.insert(fragment_name, true);

        let spread_nodes = self.context.get_fragment_spreads(&fragment.selection_set);
        if spread_nodes.is_empty() {
            return;
        }

        self.spread_path_index_by_name
            .insert(fragment_name, Some(self.spread_path.len()));

        for spread_node in spread_nodes.iter().copied() {
            let spread_name = spread_node.name.value();
            let cycle_index = self
                .spread_path_index_by_name
                .get(spread_name)
                .copied()
                .flatten();

            self.spread_path.push(spread_node);
            match cycle_index {
                None => {
                    let spread_fragment = self.context.get_fragment(spread_name);
                    if let Some(spread_fragment) = spread_fragment {
                        self.detect_cycle_recursive(spread_fragment);
                    }
                }
                Some(cycle_index) => self.report_cycle(spread_name, cycle_index),
            }
            self.spread_path.pop();
        }

        self.spread_path_index_by_name.insert(fragment_name, None);
    }

    /// Reports the cycle that closes at the end of the spread path, lines
    /// 71-83 of the reference.
    ///
    /// `spread_name` is the fragment that is spread within itself and
    /// `cycle_index` is the position in the spread path where the cycle
    /// starts. The message names every spread of the cycle but the last,
    /// and the error blames all of them.
    fn report_cycle(&self, spread_name: &str, cycle_index: usize) {
        let cycle_path = self.spread_path.get(cycle_index..).unwrap_or_default();
        let via_path = cycle_path
            .split_last()
            .map_or(&[][..], |(_, rest)| rest)
            .iter()
            .map(|s| format!("\"{}\"", s.name.value()))
            .collect::<Vec<String>>()
            .join(", ");

        let ending = if via_path.is_empty() {
            String::from(".")
        } else {
            format!(" via {via_path}.")
        };
        self.context.report_error(GraphQLError::new(
            format!("Cannot spread fragment \"{spread_name}\" within itself") + &ending,
            GraphQLErrorOptions {
                nodes: Some(
                    cycle_path
                        .iter()
                        .map(|spread| AstNode::from((*spread).clone()))
                        .collect(),
                ),
                ..GraphQLErrorOptions::default()
            },
        ));
    }
}

impl<'a> Visitor<'a> for NoFragmentCyclesRule<'_, 'a> {
    fn enter_operation_definition(
        &mut self,
        _node: &'a OperationDefinitionNode,
        _context: &VisitContext<'a, '_>,
    ) -> VisitResult {
        VisitResult::Skip
    }

    fn enter_fragment_definition(
        &mut self,
        node: &'a FragmentDefinitionNode,
        _context: &VisitContext<'a, '_>,
    ) -> VisitResult {
        self.detect_cycle_recursive(node);
        VisitResult::Skip
    }
}

/// Creates the visitor of `NoFragmentCyclesRule` over `context`.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/NoFragmentCyclesRule.ts, MIT.
#[must_use]
pub fn no_fragment_cycles_rule<'c, 'a>(
    context: &'c AstValidationContext<'a>,
) -> Box<dyn EnterLeave<'a> + 'c> {
    NoFragmentCyclesRule::boxed(context)
}
