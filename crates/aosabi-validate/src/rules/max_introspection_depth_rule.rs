// SPDX-License-Identifier: MIT
/*
 * aosabi-validate: the rule `MaxIntrospectionDepthRule`.
 *
 * Derived from graphql-js 16.12.0, src/validation/rules/MaxIntrospectionDepthRule.ts, MIT.
 */

//! The `MaxIntrospectionDepthRule` of the validation layer.
//!
//! The walk keeps, for each fragment and each field `__schema` or `__type`,
//! and for each depth below the limit at which the walk comes to it, the
//! outcome of its walk from there, so a fragment that two fields use is
//! walked at most once per starting depth. An outcome is kept only when it is
//! the same on every path that reaches the node, which holds for a walk that
//! did not reach the limit and met no fragment open above the node, and for
//! every walk of a node inside an operation or inside a fragment that lies on
//! no cycle of the graph of fragment uses. So the answer stays the
//! reference's on every document, documents with a fragment cycle included,
//! which `NoFragmentCyclesRule` reports beside this rule.

use alloc::{
    boxed::Box,
    vec,
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
        DefinitionNode,
        FieldNode,
        SelectionSetNode, //
    },
    visitor::{
        EnterLeave,
        NodeOrList,
        NodeRef,
        VisitContext,
        VisitResult,
        Visitor, //
    }, //
};

use crate::validation_context::{
    AstValidationContext,
    Rule,
    address_of, //
};

/// The depth of list fields at which an introspection query is refused.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/MaxIntrospectionDepthRule.ts, MIT.
const MAX_LISTS_DEPTH: usize = 3;

/// The position of no fragment: the entry of a fragment the walk left, and
/// the [`Walk::met`] of a walk that met no open fragment.
const NO_POSITION: usize = usize::MAX;

/// The fragments a depth count is inside of, by name, the `visitedFragments`
/// of the reference, each with its position on the path.
///
/// The reference writes `true` on the way into a fragment and `undefined` on
/// the way out, and reads the entry with `=== true` alone. Here the entry of
/// an open fragment holds its position, the count of fragments that were
/// open when the walk entered it, and [`NO_POSITION`] stands for `undefined`.
type VisitedFragments<'a> = IndexMap<&'a str, usize, KeyHasherBuilder>;

/// The result of the walk below one node: whether it reached the limit, and
/// the lowest position of a fragment it met open.
///
/// More open fragments only cut a walk short. A walk that met no fragment
/// open above the node, only fragments it opened itself and, below a
/// fragment definition, that definition's own fragment, walks as it would
/// with no fragment open above the node, so when it did not reach the limit
/// it answers `false` on every path that reaches the node. A walk that
/// reached the limit may answer `false` on another path, where a fragment it
/// entered here is open above the node and cuts the walk short there. A kept
/// result reads as a walk that met no open fragment, since it answers the
/// same on every path.
#[derive(Clone, Copy)]
struct Walk {
    /// Whether the walk reached the depth of list fields the limit names.
    reached: bool,
    /// The lowest position among the fragments the walk met open, or
    /// [`NO_POSITION`] when it met none.
    met: usize,
}

/// The kept result of the walk of each fragment and each field `__schema` or
/// `__type`, by the address of the node.
///
/// Such a node is entered below the limit at one of [`MAX_LISTS_DEPTH`]
/// depths, so the kept results of a node are an array of that length by its
/// entry depth, and the node is walked once per depth. A result is kept only when it is the
/// same on every path that reaches the node, so a read answers for the current
/// path as well. The map lives on the rule instance of one validation, so it
/// holds no result across documents.
type Kept = IndexMap<usize, [Option<bool>; MAX_LISTS_DEPTH], KeyHasherBuilder>;

/// The fragments that lie on a cycle of the spread graph.
///
/// The graph holds one vertex for each fragment name, the definition that
/// `get_fragment` answers, and one edge for each spread below that definition
/// that names a known fragment, as `get_fragment_spreads` finds them. A
/// fragment lies on a cycle when its strongly connected component holds two
/// fragments or more, or when it spreads itself. The components come from
/// Tarjan's algorithm with a stack of its own, so a long chain of fragments
/// needs no deep recursion.
struct Cycles<'a> {
    /// The vertex of each fragment, by name.
    vertex: IndexMap<&'a str, usize, KeyHasherBuilder>,
    /// Whether the fragment of each vertex lies on a cycle.
    on_cycle: Vec<bool>,
}

impl<'a> Cycles<'a> {
    /// Finds the fragments of the document of `context` that lie on a cycle.
    fn of(context: &AstValidationContext<'a>) -> Self {
        let mut vertex: IndexMap<&'a str, usize, KeyHasherBuilder> = IndexMap::default();
        let mut fragments = Vec::new();
        for definition in &context.get_document().definitions {
            if let DefinitionNode::FragmentDefinition(definition) = definition {
                let name = definition.name.value();
                if vertex.get(name).is_none()
                    && let Some(fragment) = context.get_fragment(name)
                {
                    vertex.insert(name, fragments.len());
                    fragments.push(fragment);
                }
            }
        }

        // The edges of the vertex `v` are `targets[starts[v]..starts[v + 1]]`.
        let mut starts = Vec::with_capacity(fragments.len() + 1);
        let mut targets = Vec::new();
        for fragment in &fragments {
            starts.push(targets.len());
            for spread in context.get_fragment_spreads(&fragment.selection_set).iter() {
                if let Some(&target) = vertex.get(spread.name.value()) {
                    targets.push(target);
                }
            }
        }
        starts.push(targets.len());

        let count = fragments.len();
        let mut order = vec![usize::MAX; count];
        let mut low = vec![0; count];
        let mut on_stack = vec![false; count];
        let mut on_cycle = vec![false; count];
        let mut stack: Vec<usize> = Vec::new();
        // Each frame holds a vertex and the position of its next edge.
        let mut frames: Vec<(usize, usize)> = Vec::new();
        let mut next = 0;
        for root in 0..count {
            if order[root] != usize::MAX {
                continue;
            }
            order[root] = next;
            low[root] = next;
            next += 1;
            on_stack[root] = true;
            stack.push(root);
            frames.push((root, starts[root]));
            while let Some(frame) = frames.last_mut() {
                let v = frame.0;
                if frame.1 < starts[v + 1] {
                    let w = targets[frame.1];
                    frame.1 += 1;
                    if w == v {
                        on_cycle[v] = true;
                    }
                    if order[w] == usize::MAX {
                        order[w] = next;
                        low[w] = next;
                        next += 1;
                        on_stack[w] = true;
                        stack.push(w);
                        frames.push((w, starts[w]));
                    } else if on_stack[w] {
                        low[v] = low[v].min(order[w]);
                    }
                    continue;
                }
                frames.pop();
                if let Some(&(parent, _)) = frames.last() {
                    low[parent] = low[parent].min(low[v]);
                }
                // A vertex whose low link is its own order roots a component,
                // which is the stack from that vertex up.
                if low[v] == order[v]
                    && let Some(at) = stack.iter().rposition(|&w| w == v)
                {
                    let cyclic = stack.len() - at > 1;
                    for &w in &stack[at..] {
                        on_stack[w] = false;
                        on_cycle[w] |= cyclic;
                    }
                    stack.truncate(at);
                }
            }
        }

        Self { vertex, on_cycle }
    }

    /// Returns whether the fragment `name` lies on a cycle.
    fn contains(&self, name: &str) -> bool {
        self.vertex.get(name).is_some_and(|&v| self.on_cycle[v])
    }
}

/// The visitor of `MaxIntrospectionDepthRule`.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/MaxIntrospectionDepthRule.ts, MIT.
pub struct MaxIntrospectionDepthRule<'c, 'a> {
    /// The context the rule reads and reports to.
    context: &'c AstValidationContext<'a>,
    /// The kept outcome of the walk of each fragment and each meta field from
    /// each starting depth.
    kept: Kept,
    /// Each fragment on a cycle of the graph of fragment uses, found on first
    /// use.
    cycles: Option<Cycles<'a>>,
}

impl<'c, 'a> Rule<'c, 'a> for MaxIntrospectionDepthRule<'c, 'a> {
    type Context = &'c AstValidationContext<'a>;

    fn new(context: Self::Context) -> Self {
        Self {
            context,
            kept: Kept::default(),
            cycles: None,
        }
    }
}

impl<'a> MaxIntrospectionDepthRule<'_, 'a> {
    /// Counts the depth of list fields in "__Type" recursively and
    /// returns `true` if the limit has been reached.
    ///
    /// `node` is the node to count below, `visited_fragments` holds each
    /// fragment the count is inside of, and `depth` is the count above `node`.
    /// The two defaults of the reference, an empty object and zero, are what
    /// the hook passes. `open` is the number of open fragment walks on the path,
    /// the position of the next fragment the walk goes into, and `enclosing`
    /// names the fragment that holds `node`, absent for a node of an operation.
    /// A fragment and a field `__schema` or `__type` read their kept outcome
    /// before the walk and keep it after, so a fragment that two fields use is
    /// walked once from each depth.
    ///
    /// Derived from graphql-js 16.12.0, src/validation/rules/MaxIntrospectionDepthRule.ts, MIT.
    fn check_depth(
        &mut self,
        node: NodeRef<'a>,
        visited_fragments: &mut VisitedFragments<'a>,
        depth: usize,
        open: usize,
        enclosing: Option<&'a str>,
    ) -> Walk {
        if let NodeRef::FragmentSpread(node) = node {
            let fragment_name = node.name.value();
            if let Some(&position) = visited_fragments.get(fragment_name)
                && position != NO_POSITION
            {
                // Fragment cycles are handled by `NoFragmentCyclesRule`. The
                // walk met the fragment open at `position` on its path.
                return Walk {
                    reached: false,
                    met: position,
                };
            }
            let Some(fragment) = self.context.get_fragment(fragment_name) else {
                // Missing fragments checks are handled by `KnownFragmentNamesRule`.
                return Walk {
                    reached: false,
                    met: NO_POSITION,
                };
            };

            // Rather than following an immutable programming pattern which has
            // significant memory and garbage collection overhead, we've opted to
            // take a mutable approach for efficiency's sake. Importantly visiting a
            // fragment twice is fine, so long as you don't do one visit inside the
            // other. The entry holds the position of the fragment on the path in
            // place of `true`. The fragment carries the kept result, so a second
            // spread of it reads that result and walks the fragment no second time.
            visited_fragments.insert(fragment_name, open);
            let walk = self.check_depth(
                fragment.into(),
                visited_fragments,
                depth,
                open + 1,
                Some(fragment_name),
            );
            visited_fragments.insert(fragment_name, NO_POSITION);
            return walk;
        }

        if let Some(kept) = self.read_kept(node, depth) {
            return kept;
        }
        let walk = self.walk_below(node, visited_fragments, depth, open, enclosing);
        self.write_kept(node, depth, open, walk, enclosing);
        walk
    }

    /// Walks below `node` from `depth`, the body of `check_depth` for a node
    /// that is not a fragment spread.
    ///
    /// A list field raises the depth, and the limit ends the walk before its
    /// children. A node with a selection set walks its children and met the
    /// lowest position any child met.
    fn walk_below(
        &mut self,
        node: NodeRef<'a>,
        visited_fragments: &mut VisitedFragments<'a>,
        mut depth: usize,
        open: usize,
        enclosing: Option<&'a str>,
    ) -> Walk {
        // Check all introspection lists.
        if let NodeRef::Field(node) = node
            && matches!(
                node.name.value(),
                "fields" | "interfaces" | "possibleTypes" | "inputFields"
            )
        {
            depth += 1;
            if depth >= MAX_LISTS_DEPTH {
                return Walk {
                    reached: true,
                    met: NO_POSITION,
                };
            }
        }

        // Handles fields and inline fragments.
        let mut met = NO_POSITION;
        if let Some(selection_set) = selection_set_of(node) {
            for child in selection_set.selections.iter() {
                let child =
                    self.check_depth(child.into(), visited_fragments, depth, open, enclosing);
                met = met.min(child.met);
                if child.reached {
                    return Walk { reached: true, met };
                }
            }
        }

        Walk {
            reached: false,
            met,
        }
    }

    /// Returns the kept result of `node` from entry `depth`, when one is kept.
    ///
    /// A kept result is the same on every path that reaches the node, so the
    /// read answers for the current path and reads as a walk that met no open
    /// fragment.
    fn read_kept(&self, node: NodeRef<'a>, depth: usize) -> Option<Walk> {
        let key = memo_key(node)?;
        let reached = self.kept.get(&key)?.get(depth).copied().flatten()?;
        Some(Walk {
            reached,
            met: NO_POSITION,
        })
    }

    /// Keeps the result of `node` from entry `depth` when it is the same on
    /// every path that reaches the node.
    ///
    /// The fragments open above a field are the `open` fragments of the path
    /// that reaches it, and those above a fragment definition are the ones
    /// below the position of its own fragment, which its spread opened last.
    /// A walk that met none of them and did not reach the limit answers
    /// `false` with none open above, and more open fragments only cut a walk
    /// short, so it answers `false` on every path. A fragment open above a
    /// node reaches the node, so a walk below the node that met such a
    /// fragment closes a cycle through the fragment that holds the node, and
    /// its result holds for the current path alone and is not kept, with no
    /// need to find the cycles. Every walk of a node inside an operation, or
    /// inside a fragment on no cycle of the spread graph, meets no fragment
    /// open above the node on any path, so a walk that reached the limit
    /// there is kept as well. A walk that reached the limit inside a fragment
    /// on a cycle is not kept, so such a node is walked through the path set
    /// as the reference walks it.
    fn write_kept(
        &mut self,
        node: NodeRef<'a>,
        depth: usize,
        open: usize,
        walk: Walk,
        enclosing: Option<&'a str>,
    ) {
        if depth >= MAX_LISTS_DEPTH {
            return;
        }
        let Some(key) = memo_key(node) else {
            return;
        };
        // Only a spread enters a fragment definition, after it opens the
        // fragment at the position `open - 1`.
        let above = match node {
            NodeRef::FragmentDefinition(_) => open - 1,
            _ => open,
        };
        if walk.met < above || (walk.reached && self.on_cycle(enclosing)) {
            return;
        }
        let mut slots = self
            .kept
            .get(&key)
            .copied()
            .unwrap_or([None; MAX_LISTS_DEPTH]);
        slots[depth] = Some(walk.reached);
        self.kept.insert(key, slots);
    }

    /// Returns whether `enclosing`, the fragment that holds a node, lies on a
    /// cycle of the spread graph, which a node of an operation never does.
    ///
    /// The cycles are found on the first call, which only a walk that reached
    /// the limit inside a fragment makes, so a document with no such walk
    /// never builds the graph.
    fn on_cycle(&mut self, enclosing: Option<&'a str>) -> bool {
        let Some(name) = enclosing else {
            return false;
        };
        let context = self.context;
        self.cycles
            .get_or_insert_with(|| Cycles::of(context))
            .contains(name)
    }
}

/// Returns the selection set of `node`, the `'selectionSet' in node &&
/// node.selectionSet` of the reference.
///
/// Four node kinds declare the property, and a field holds it optionally.
const fn selection_set_of(node: NodeRef<'_>) -> Option<&SelectionSetNode> {
    match node {
        NodeRef::OperationDefinition(node) => Some(&node.selection_set),
        NodeRef::Field(node) => node.selection_set.as_ref(),
        NodeRef::InlineFragment(node) => Some(&node.selection_set),
        NodeRef::FragmentDefinition(node) => Some(&node.selection_set),
        _ => None,
    }
}

/// Returns the address of `node` when the walk keeps its result, the key its
/// kept result is held under.
///
/// The walk reaches a node on more than one path only where paths join, at a
/// fragment definition, which every spread of the fragment enters, and at a
/// field `__schema` or `__type`, where the hook starts a walk of its own.
/// Every other node has one parent and is walked once for each walk of the
/// fragment or the field above it, so results are kept at those two kinds of
/// node alone, and a fragment is still walked at most once per entry depth.
fn memo_key(node: NodeRef<'_>) -> Option<usize> {
    match node {
        NodeRef::FragmentDefinition(node) => Some(address_of(node)),
        NodeRef::Field(node) if matches!(node.name.value(), "__schema" | "__type") => {
            Some(address_of(node))
        }
        _ => None,
    }
}

/// Returns the name of the fragment definition that holds the visited node,
/// absent for a node of an operation.
///
/// The definition is the third ancestor of a walk from the document, after
/// the document and its list of definitions, and the first of a walk from
/// the definition itself.
fn enclosing_fragment<'a>(at: &VisitContext<'a, '_>) -> Option<&'a str> {
    at.ancestors
        .iter()
        .take(3)
        .find_map(|&ancestor| match ancestor {
            NodeOrList::Node(NodeRef::FragmentDefinition(fragment)) => Some(fragment.name.value()),
            _ => None,
        })
}

impl<'a> Visitor<'a> for MaxIntrospectionDepthRule<'_, 'a> {
    fn enter_field(&mut self, node: &'a FieldNode, at: &VisitContext<'a, '_>) -> VisitResult {
        if (node.name.value() == "__schema" || node.name.value() == "__type")
            && self
                .check_depth(
                    node.into(),
                    &mut VisitedFragments::default(),
                    0,
                    0,
                    enclosing_fragment(at),
                )
                .reached
        {
            self.context.report_error(GraphQLError::new(
                "Maximum introspection depth exceeded",
                GraphQLErrorOptions {
                    nodes: Some(vec![AstNode::from(node.clone())]),
                    ..GraphQLErrorOptions::default()
                },
            ));
            return VisitResult::Skip;
        }
        VisitResult::Continue
    }
}

/// Creates the visitor of `MaxIntrospectionDepthRule` over `context`.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/MaxIntrospectionDepthRule.ts, MIT.
#[must_use]
pub fn max_introspection_depth_rule<'c, 'a>(
    context: &'c AstValidationContext<'a>,
) -> Box<dyn EnterLeave<'a> + 'c> {
    MaxIntrospectionDepthRule::boxed(context)
}
