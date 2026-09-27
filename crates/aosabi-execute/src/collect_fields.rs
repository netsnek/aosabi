// SPDX-License-Identifier: MIT
/*
 * aosabi-execute collect_fields: the fields a selection set selects on a runtime type.
 *
 * Derived from @graphql-tools/utils 11.0.0, cjs/collectFields.js, MIT.
 * Derived from graphql-js 16.12.0, src/execution/collectFields.ts, MIT.
 */

//! The field collection of the executor.
//!
//! The reference collects the field nodes of a selection set into an
//! `AccumulatorMap`, a `Map` from the response key to the list of nodes that
//! answer it. A `Map` iterates in insertion order, so the carrier here is an
//! insertion ordered map, [`CollectedFields`].
//!
//! The `@defer` branches of the reference are left out, which is
//! `getDeferValues` at `collectFields.js:123-133`, the `patches` list and the
//! branches at `:27-34`, `:45-48` and `:56-63`. No directive named `defer`
//! reaches a schema of Aosabi, so `getDeferValues` answers `undefined` for every
//! node and the branches that are left run as the reference runs them.
//!
//! The reference names a node by its object identity. A node of Aosabi is an
//! owned value inside the document, so [`FieldNodeCache`] clones a node out
//! of the document once and hands the same `Rc` to every later caller, which
//! gives a node list an identity that lasts for the execution, and for every
//! execution that reads the plan of the document (see [`crate::plan`]).
//!
//! The plan builds its nodes through `collect_root_fields` and
//! `collect_sub_fields_of`, which collect as [`collect_fields`] and
//! [`collect_sub_fields`] do and tell whether an `@skip` or an `@include` the
//! collection read takes its condition from a variable, in which case the
//! collection answers the variables of its request alone.

use core::{
    cell::{
        Cell,
        RefCell, //
    },
    mem::size_of, //
};
use std::{
    rc::Rc,
    sync::Arc, //
};

use aosabi_json::{
    JsObject,
    JsValue,
    map::{
        IndexMap,
        IndexSet, //
    },
    value::KeyHasherBuilder, //
};
use aosabi_lang::ast::{
    DirectiveNode,
    FieldNode,
    FragmentDefinitionNode,
    FragmentSpreadNode,
    InlineFragmentNode,
    NamedTypeNode,
    SelectionNode,
    SelectionSetNode, //
};
use aosabi_type::{
    DefinitionError,
    NamedType,
    ObjMap,
    ObjectType,
    Schema,
    SharedType,
    Type,
    directives::{
        graphql_include_directive,
        graphql_skip_directive, //
    },
    get_directive_values,
    is_abstract_type, //
};

use crate::{
    execute::FieldNodes,
    plan::{
        Collected,
        condition_reads_variable,
        head_bytes, //
    }, //
};

/// The field nodes of a selection set by response key, in the order the keys first appear.
pub type CollectedFields = ObjMap<FieldNodes>;

/// The fields while they are collected, before a node list is shared.
type Accumulator = ObjMap<Vec<Rc<FieldNode>>>;

/// The shared clone of every field node the collections of a plan have collected, by the address of the node.
///
/// A clone of a field node shares the selections of its selection set with
/// the node it is cloned from, so a field node below a clone has the address
/// it has in the document, and the cache finds it by that address whether
/// the walk reaches it through the document or through a clone.
///
/// The clones a plan counts come first, and the clones made since the plan
/// last counted or dropped them come after, so a plan whose keeper refuses
/// their bytes drops them again and the cache holds no clone the plan does
/// not count.
#[derive(Default)]
pub struct FieldNodeCache {
    /// The clones by the address of the node they copy.
    nodes: RefCell<IndexMap<usize, Rc<FieldNode>, KeyHasherBuilder>>,
    /// The count of the first clones, those the plan counts.
    counted: Cell<usize>,
    /// The estimated bytes of the clones after them.
    uncounted: Cell<usize>,
    /// The estimated bytes of every clone the cache holds.
    held: Cell<usize>,
}

impl FieldNodeCache {
    /// Returns an empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the shared clone of `node`, made on the first call for that node.
    ///
    /// `node` lives in the operation, in a fragment definition or in a clone
    /// this cache holds, and each of them outlives the cache entry.
    pub fn share(&self, node: &FieldNode) -> Rc<FieldNode> {
        let address = core::ptr::from_ref(node).addr();
        let mut nodes = self.nodes.borrow_mut();
        let shared = nodes.entry(address).or_insert_with(|| {
            let bytes = clone_bytes(node);
            self.uncounted.set(self.uncounted.get() + bytes);
            self.held.set(self.held.get() + bytes);
            Rc::new(node.clone())
        });
        Rc::clone(shared)
    }

    /// Returns the estimated bytes of the clones made since the plan last counted or dropped them.
    pub(crate) fn uncounted(&self) -> usize {
        self.uncounted.get()
    }

    /// Counts the clones made since the plan last counted or dropped them, whose bytes its keeper admitted.
    pub(crate) fn count_uncounted(&self) {
        self.counted.set(self.nodes.borrow().len());
        self.uncounted.set(0);
    }

    /// Drops the clones made since the plan last counted or dropped them, whose bytes it does not count.
    ///
    /// The clones leave the cache from the last, so no clone before them
    /// moves, and a node built from them holds them while it stands.
    pub(crate) fn drop_uncounted(&self) {
        let mut nodes = self.nodes.borrow_mut();
        while nodes.len() > self.counted.get() {
            let Some((&address, _)) = nodes.get_index(nodes.len() - 1) else {
                break;
            };
            let Some(dropped) = nodes.shift_remove(&address) else {
                break;
            };
            self.held
                .set(self.held.get().saturating_sub(clone_bytes(&dropped)));
        }
        self.uncounted.set(0);
    }

    /// Returns the count of clones the cache holds and their estimated bytes.
    pub(crate) fn holds(&self) -> (usize, usize) {
        (self.nodes.borrow().len(), self.held.get())
    }
}

/// Returns the estimated bytes the cache holds for a clone of `node`.
///
/// The clone takes its entry in the map, the two counts of its `Rc`, the
/// node and the heap of its head.
fn clone_bytes(node: &FieldNode) -> usize {
    2 * size_of::<(usize, Rc<FieldNode>)>()
        + 2 * size_of::<usize>()
        + size_of::<FieldNode>()
        + head_bytes(node)
}

/// What every step of one collection reads, the first four parameters of `collectFieldsImpl`.
struct Collector<'a> {
    /// The schema the operation runs against.
    schema: &'a Schema,
    /// The fragment definitions of the document by name.
    fragments: &'a ObjMap<Rc<FragmentDefinitionNode>>,
    /// The coerced variable values.
    variable_values: &'a JsObject,
    /// The object type the fields are selected on.
    runtime_type: &'a Arc<ObjectType>,
    /// The shared field nodes of the execution.
    cache: &'a FieldNodeCache,
    /// Whether an `@skip` or an `@include` the collection read takes its condition from a variable.
    reads_variable: Cell<bool>,
}

impl Collector<'_> {
    /// Answers whether a node with `directives` is included, [`should_include_node`].
    ///
    /// The collection notes a condition that reads a variable, which makes
    /// its answer one of the variables of its request.
    fn includes(&self, directives: Option<&[DirectiveNode]>) -> Result<bool, DefinitionError> {
        if condition_reads_variable(directives) {
            self.reads_variable.set(true);
        }
        should_include_node(self.variable_values, directives)
    }

    /// Adds the fields `selection_set` selects to `fields`.
    ///
    /// A fragment is marked as visited after its type condition has matched,
    /// where graphql-js marks it before. Inside one collection the runtime type
    /// is the same for every visit, so no result shows the difference.
    fn collect_fields_impl(
        &self,
        selection_set: &SelectionSetNode,
        fields: &mut Accumulator,
        visited_fragment_names: &mut IndexSet<String, KeyHasherBuilder>,
    ) -> Result<(), DefinitionError> {
        for selection in selection_set.selections.iter() {
            match selection {
                SelectionNode::Field(selection) => {
                    if !self.includes(selection.directives.as_deref())? {
                        continue;
                    }
                    // The key is looked up as it stands in the node, and only
                    // a key the map lacks is copied into a string of its own.
                    let key = get_field_entry_key(selection);
                    let node = self.cache.share(selection);
                    if let Some(nodes) = fields.get_mut(key) {
                        nodes.push(node);
                    } else {
                        fields.insert(String::from(key), vec![node]);
                    }
                }
                SelectionNode::InlineFragment(selection) => {
                    self.collect_inline_fragment(selection, fields, visited_fragment_names)?;
                }
                SelectionNode::FragmentSpread(selection) => {
                    self.collect_fragment_spread(selection, fields, visited_fragment_names)?;
                }
            }
        }
        Ok(())
    }

    /// Collects the fields of the inline fragment `selection`, the branch
    /// `INLINE_FRAGMENT` of `collectFieldsImpl` at `collectFields.js:28-47`.
    // Out of line, so a selection with no fragment compiles none of it.
    #[inline(never)]
    fn collect_inline_fragment(
        &self,
        selection: &InlineFragmentNode,
        fields: &mut Accumulator,
        visited_fragment_names: &mut IndexSet<String, KeyHasherBuilder>,
    ) -> Result<(), DefinitionError> {
        if !self.includes(selection.directives.as_deref())?
            || !does_fragment_condition_match(
                self.schema,
                selection.type_condition.as_ref(),
                self.runtime_type,
            )?
        {
            return Ok(());
        }
        self.collect_fields_impl(&selection.selection_set, fields, visited_fragment_names)
    }

    /// Collects the fields of the fragment the spread `selection` names, the
    /// branch `FRAGMENT_SPREAD` of `collectFieldsImpl` at
    /// `collectFields.js:48-71`.
    // Out of line, so a selection with no fragment compiles none of it.
    #[inline(never)]
    fn collect_fragment_spread(
        &self,
        selection: &FragmentSpreadNode,
        fields: &mut Accumulator,
        visited_fragment_names: &mut IndexSet<String, KeyHasherBuilder>,
    ) -> Result<(), DefinitionError> {
        let frag_name = selection.name.value();
        if !self.includes(selection.directives.as_deref())? {
            return Ok(());
        }
        if visited_fragment_names.contains(frag_name) {
            return Ok(());
        }
        let Some(fragment) = self.fragments.get(frag_name) else {
            return Ok(());
        };
        if !does_fragment_condition_match(
            self.schema,
            Some(&fragment.type_condition),
            self.runtime_type,
        )? {
            return Ok(());
        }
        visited_fragment_names.insert(String::from(frag_name));
        self.collect_fields_impl(&fragment.selection_set, fields, visited_fragment_names)
    }
}

/// Given a selection set, collects all of the fields and returns them.
///
/// `collect_fields` requires the "runtime type" of an object. For a field that
/// returns an Interface or Union type, the "runtime type" will be the actual
/// object type returned by that field. `cache` holds the shared field nodes of
/// the execution.
///
/// # Errors
///
/// Returns what `getDirectiveValues` throws for an `@skip` or an `@include`
/// whose argument does not coerce, and what a thunk of the schema throws.
pub fn collect_fields(
    schema: &Schema,
    fragments: &ObjMap<Rc<FragmentDefinitionNode>>,
    variable_values: &JsObject,
    runtime_type: &Arc<ObjectType>,
    selection_set: &SelectionSetNode,
    cache: &FieldNodeCache,
) -> Result<CollectedFields, DefinitionError> {
    collect_root_fields(
        schema,
        fragments,
        variable_values,
        runtime_type,
        selection_set,
        cache,
    )
    .map(|(collected, _)| collected.fields)
}

/// Collects the fields of `selection_set` on `runtime_type` as [`collect_fields`] does, for a node of a plan.
///
/// The answer holds the estimated bytes of the clones `cache` holds that
/// its plan does not count yet, which are the clones the collection added
/// once the plan counted or dropped those of every collection before it,
/// and whether an `@skip` or an `@include` it read takes its condition from
/// a variable.
///
/// # Errors
///
/// Returns what [`collect_fields`] returns.
pub(crate) fn collect_root_fields(
    schema: &Schema,
    fragments: &ObjMap<Rc<FragmentDefinitionNode>>,
    variable_values: &JsObject,
    runtime_type: &SharedType<ObjectType>,
    selection_set: &SelectionSetNode,
    cache: &FieldNodeCache,
) -> Result<(Collected, bool), DefinitionError> {
    let collector = Collector {
        schema,
        fragments,
        variable_values,
        runtime_type,
        cache,
        reads_variable: Cell::new(false),
    };
    let mut fields = Accumulator::default();
    let mut visited_fragment_names = IndexSet::default();
    collector.collect_fields_impl(selection_set, &mut fields, &mut visited_fragment_names)?;
    Ok(collector.finish(fields))
}

/// Determines if a field should be included based on the `@include` and `@skip`
/// directives, where `@skip` has higher precedence than `@include`.
///
/// `directives` is the `directives` list of the node. The comparisons are
/// `=== true` and `=== false`, so a value of any other kind includes the node.
///
/// # Errors
///
/// Returns what `getDirectiveValues` throws.
pub fn should_include_node(
    variable_values: &JsObject,
    directives: Option<&[DirectiveNode]>,
) -> Result<bool, DefinitionError> {
    let skip = get_directive_values(&graphql_skip_directive(), directives, Some(variable_values))?;
    if matches!(
        skip.as_ref().and_then(|skip| skip.get("if")),
        Some(JsValue::Bool(true))
    ) {
        return Ok(false);
    }

    let include = get_directive_values(
        &graphql_include_directive(),
        directives,
        Some(variable_values),
    )?;
    if matches!(
        include.as_ref().and_then(|include| include.get("if")),
        Some(JsValue::Bool(false))
    ) {
        return Ok(false);
    }
    Ok(true)
}

/// Determines if a fragment is applicable to the given type.
///
/// `type_condition_node` is the `typeCondition` of the fragment. A type
/// condition is a named type node, so `typeFromAST` is its `NAMED_TYPE` case, a
/// read of the type map. The comparison with `ty` and the `includes` over the
/// possible types are by identity, which is how a [`NamedType`] compares.
///
/// # Errors
///
/// Returns what the thunk of an abstract type throws.
pub fn does_fragment_condition_match(
    schema: &Schema,
    type_condition_node: Option<&NamedTypeNode>,
    ty: &Arc<ObjectType>,
) -> Result<bool, DefinitionError> {
    let Some(type_condition_node) = type_condition_node else {
        return Ok(true);
    };
    let Some(conditional_type) = schema.get_type(type_condition_node.name.value()) else {
        return Ok(false);
    };
    let ty = NamedType::from(Arc::clone(ty));
    if *conditional_type == ty {
        return Ok(true);
    }
    if is_abstract_type(&Type::from(conditional_type)) {
        let possible_types = schema.get_possible_types(conditional_type)?;
        return Ok(possible_types.contains(&ty));
    }
    Ok(false)
}

/// Implements the logic to compute the key of a given field's entry.
#[must_use]
pub fn get_field_entry_key(node: &FieldNode) -> &str {
    node.alias
        .as_ref()
        .map_or(node.name.value(), |alias| alias.value())
}

/// Given an array of field nodes, collects all of the subfields of the passed
/// in fields, and returns them at the end.
///
/// `collect_sub_fields` requires the "return type" of an object. For a field
/// that returns an Interface or Union type, the "return type" will be the
/// actual object type returned by that field. The `memoize5` around it keys on
/// the identity of its five arguments, of which three are fixed for an
/// execution, so the executor keeps the memo by the other two.
///
/// # Errors
///
/// Returns what [`collect_fields`] returns.
pub fn collect_sub_fields(
    schema: &Schema,
    fragments: &ObjMap<Rc<FragmentDefinitionNode>>,
    variable_values: &JsObject,
    return_type: &Arc<ObjectType>,
    field_nodes: &FieldNodes,
    cache: &FieldNodeCache,
) -> Result<CollectedFields, DefinitionError> {
    collect_sub_fields_of(
        schema,
        fragments,
        variable_values,
        return_type,
        field_nodes,
        cache,
    )
    .map(|(collected, _)| collected.fields)
}

/// Collects the subfields of `field_nodes` on `return_type` as [`collect_sub_fields`] does, for a node of a plan.
///
/// The answer holds the estimated bytes of the clones `cache` holds that
/// its plan does not count yet, which are the clones the collection added
/// once the plan counted or dropped those of every collection before it,
/// and whether an `@skip` or an `@include` it read takes its condition from
/// a variable.
///
/// # Errors
///
/// Returns what [`collect_fields`] returns.
pub(crate) fn collect_sub_fields_of(
    schema: &Schema,
    fragments: &ObjMap<Rc<FragmentDefinitionNode>>,
    variable_values: &JsObject,
    return_type: &SharedType<ObjectType>,
    field_nodes: &FieldNodes,
    cache: &FieldNodeCache,
) -> Result<(Collected, bool), DefinitionError> {
    let collector = Collector {
        schema,
        fragments,
        variable_values,
        runtime_type: return_type,
        cache,
        reads_variable: Cell::new(false),
    };
    let mut sub_field_nodes = Accumulator::default();
    let mut visited_fragment_names = IndexSet::default();
    for node in field_nodes.iter() {
        if let Some(selection_set) = &node.selection_set {
            collector.collect_fields_impl(
                selection_set,
                &mut sub_field_nodes,
                &mut visited_fragment_names,
            )?;
        }
    }
    Ok(collector.finish(sub_field_nodes))
}

impl Collector<'_> {
    /// Returns what the collection of `fields` answers a plan.
    ///
    /// Every node list goes behind its `Rc`, which gives the list its
    /// identity.
    fn finish(&self, fields: Accumulator) -> (Collected, bool) {
        let fields = fields
            .into_iter()
            .map(|(response_name, field_nodes)| (response_name, FieldNodes::from(field_nodes)))
            .collect();
        let collected = Collected {
            fields,
            shared: self.cache.uncounted(),
        };
        (collected, self.reads_variable.get())
    }
}
