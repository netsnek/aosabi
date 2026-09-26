// SPDX-License-Identifier: MIT
/*
 * aosabi-validate: the rule `OverlappingFieldsCanBeMergedRule`, with the
 * value sorter its argument comparison needs.
 *
 * Derived from graphql-js 16.12.0, src/validation/rules/OverlappingFieldsCanBeMergedRule.ts, MIT.
 * Derived from graphql-js 16.12.0, src/utilities/sortValueNode.ts, MIT.
 */

//! The `OverlappingFieldsCanBeMergedRule` of the validation layer.
//!
//! # Algorithm
//!
//! Conflicts occur when two fields exist in a query which will produce the same
//! response name, but represent differing values, thus creating a conflict.
//! The algorithm below finds all conflicts via making a series of comparisons
//! between fields. In order to compare as few fields as possible, this makes
//! a series of comparisons "within" sets of fields and "between" sets of fields.
//!
//! Given any selection set, a collection produces both a set of fields by
//! also including all inline fragments, as well as a list of fragments
//! referenced by fragment spreads.
//!
//! A) Each selection set represented in the document first compares "within" its
//! collected set of fields, finding any conflicts between every pair of
//! overlapping fields.
//! Note: This is the *only time* that a the fields "within" a set are compared
//! to each other. After this only fields "between" sets are compared.
//!
//! B) Also, if any fragment is referenced in a selection set, then a
//! comparison is made "between" the original set of fields and the
//! referenced fragment.
//!
//! C) Also, if multiple fragments are referenced, then comparisons
//! are made "between" each referenced fragment.
//!
//! D) When comparing "between" a set of fields and a referenced fragment, first
//! a comparison is made between each field in the original set of fields and
//! each field in the the referenced set of fields.
//!
//! E) Also, if any fragment is referenced in the referenced selection set,
//! then a comparison is made "between" the original set of fields and the
//! referenced fragment (recursively referring to step D).
//!
//! F) When comparing "between" two fragments, first a comparison is made between
//! each field in the first referenced set of fields and each field in the the
//! second referenced set of fields.
//!
//! G) Also, any fragments referenced by the first must be compared to the
//! second, and any fragments referenced by the second must be compared to the
//! first (recursively referring to step F).
//!
//! H) When comparing two fields, if both have selection sets, then a comparison
//! is made "between" both selection sets, first comparing the set of fields in
//! the first selection set with the set of fields in the second.
//!
//! I) Also, if any fragment is referenced in either selection set, then a
//! comparison is made "between" the other set of fields and the
//! referenced fragment.
//!
//! J) Also, if two fragments are referenced in both selection sets, then a
//! comparison is made "between" the two fragments.
//!
//! # The shape of the port
//!
//! The reference hands the context, the cache of collected selection sets
//! and the two sets of compared pairs to every function as its leading
//! parameters. They are the fields of [`OverlappingFieldsCanBeMergedRule`]
//! here, and each such function is a method of the rule that keeps the name
//! and the remaining parameters of the reference.
//!
//! A thunk of the schema throws in the reference where a parent type lists
//! its fields. The same read returns a [`DefinitionError`] here, every
//! function on the way passes it up, and the hook stops the validation with
//! it.
//!
//! The reference keys a `Map` by a selection set and by a field map, which finds a key
//! by object identity. A selection set never moves while the document is borrowed, and
//! a field map is shared from the cache, which drops no entry while the rule lives, so
//! the address of each is its identity here.
//!
//! A response name and a fragment name start with a letter or an underscore and are
//! never an array index, so `Object.entries` and `Object.keys` of the null prototype
//! objects of the reference list them in insertion order, which is the order of the
//! maps here.

use alloc::{
    borrow::ToOwned,
    boxed::Box,
    format,
    rc::Rc,
    string::String,
    sync::Arc,
    vec,
    vec::Vec, //
};
use core::hash::Hash;

use aosabi_json::{
    map::IndexMap,
    value::KeyHasherBuilder, //
};
use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions,
    ast::{
        AstNode,
        FieldNode,
        FragmentDefinitionNode,
        ListValueNode,
        NamedTypeNode,
        ObjectFieldNode,
        ObjectValueNode,
        SelectionNode,
        SelectionSetNode,
        ValueNode, //
    },
    print,
    visitor::{
        EnterLeave,
        VisitContext,
        VisitResult,
        Visitor, //
    }, //
};
use aosabi_type::{
    DefinitionError,
    FieldDef,
    NamedType,
    Schema,
    Type,
    get_named_type,
    is_leaf_type,
    is_list_type,
    is_non_null_type,
    natural_compare, //
};

use crate::validation_context::{
    Rule,
    ValidationContext,
    address_of, //
};

/// A map in insertion order with the hasher of the workspace, the carrier of
/// every `Map` and null prototype object of the rule.
type OrderedMap<K, V> = IndexMap<K, V, KeyHasherBuilder>;

/// Two sets of fields that conflict under one reason, the `Conflict` of the
/// reference.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/OverlappingFieldsCanBeMergedRule.ts, MIT.
type Conflict<'a> = (ConflictReason, Vec<&'a FieldNode>, Vec<&'a FieldNode>);

/// Field name and reason.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/OverlappingFieldsCanBeMergedRule.ts, MIT.
type ConflictReason = (String, ConflictReasonMessage);

/// Reason is a string, or a nested list of conflicts.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/OverlappingFieldsCanBeMergedRule.ts, MIT.
#[derive(Debug, Clone)]
enum ConflictReasonMessage {
    /// The reason in words.
    Message(String),
    /// The reasons of the sub fields that conflict.
    Reasons(Vec<ConflictReason>),
}

/// Tuple defining a field node in a context.
///
/// The three slots are the parent type, the field node and the definition of the field
/// on that parent type.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/OverlappingFieldsCanBeMergedRule.ts, MIT.
type NodeAndDef<'a> = (Option<NamedType>, &'a FieldNode, Option<FieldDef>);

/// Map of array of those.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/OverlappingFieldsCanBeMergedRule.ts, MIT.
type NodeAndDefCollection<'a> = OrderedMap<&'a str, Vec<NodeAndDef<'a>>>;

/// The names of the fragments a selection set spreads, the `FragmentNames`
/// of the reference at line 112 of the rule file.
type FragmentNames<'a> = Rc<[&'a str]>;

/// The fields and the fragment names of one selection set, as the cache
/// shares them.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/OverlappingFieldsCanBeMergedRule.ts, MIT.
type FieldsAndFragmentNames<'a> = (Rc<NodeAndDefCollection<'a>>, FragmentNames<'a>);

/// Renders a conflict reason as the words of a message.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/OverlappingFieldsCanBeMergedRule.ts, MIT.
fn reason_message(reason: &ConflictReasonMessage) -> String {
    match reason {
        ConflictReasonMessage::Reasons(reason) => reason
            .iter()
            .map(|(response_name, sub_reason)| {
                format!("subfields \"{response_name}\" conflict because ")
                    + &reason_message(sub_reason)
            })
            .collect::<Vec<_>>()
            .join(" and "),
        ConflictReasonMessage::Message(reason) => reason.clone(),
    }
}

/// Overlapping fields can be merged
///
/// A selection set is only valid if all fields (including spreading any
/// fragments) either correspond to distinct response names or can be merged
/// without ambiguity.
///
/// See <https://spec.graphql.org/draft/#sec-Field-Selection-Merging>
///
/// Derived from graphql-js 16.12.0, src/validation/rules/OverlappingFieldsCanBeMergedRule.ts, MIT.
pub struct OverlappingFieldsCanBeMergedRule<'c, 'a> {
    /// The context the rule reads and reports to.
    context: &'c ValidationContext<'a>,
    /// A memoization for when fields and a fragment are compared "between"
    /// each other for conflicts, keyed by the address of the field map.
    ///
    /// Comparisons made be made many times, so memoizing this can
    /// dramatically improve the performance of this validator.
    compared_fields_and_fragment_pairs: OrderedPairSet<usize, &'a str>,
    /// The same memoization for two fragments.
    compared_fragment_pairs: PairSet<&'a str>,
    /// A cache for the "field map" and list of fragment names found in any
    /// given selection set, keyed by the address of the selection set.
    ///
    /// Selection sets may be asked for this information multiple times, so
    /// this improves the performance of this validator.
    cached_fields_and_fragment_names: OrderedMap<usize, FieldsAndFragmentNames<'a>>,
}

impl<'c, 'a> Rule<'c, 'a> for OverlappingFieldsCanBeMergedRule<'c, 'a> {
    type Context = &'c ValidationContext<'a>;

    fn new(context: Self::Context) -> Self {
        Self {
            context,
            compared_fields_and_fragment_pairs: OrderedPairSet::new(),
            compared_fragment_pairs: PairSet::new(),
            cached_fields_and_fragment_names: OrderedMap::default(),
        }
    }
}

impl<'a> Visitor<'a> for OverlappingFieldsCanBeMergedRule<'_, 'a> {
    fn enter_selection_set(
        &mut self,
        selection_set: &'a SelectionSetNode,
        _: &VisitContext<'a, '_>,
    ) -> VisitResult {
        match self.report_conflicts(selection_set) {
            Ok(()) => VisitResult::Continue,
            Err(error) => self.context.throw_error(error),
        }
    }
}

impl<'a> OverlappingFieldsCanBeMergedRule<'_, 'a> {
    /// Reports every conflict within `selection_set`, the body of the
    /// `SelectionSet` hook.
    ///
    /// Derived from graphql-js 16.12.0, src/validation/rules/OverlappingFieldsCanBeMergedRule.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns what a thunk of the schema throws while a parent type lists
    /// its fields.
    fn report_conflicts(
        &mut self,
        selection_set: &'a SelectionSetNode,
    ) -> Result<(), DefinitionError> {
        let parent_type = self.context.get_parent_type();
        let conflicts =
            self.find_conflicts_within_selection_set(parent_type.as_ref(), selection_set)?;
        for ((response_name, reason), fields1, fields2) in conflicts {
            let reason_msg = reason_message(&reason);
            let nodes = fields1
                .into_iter()
                .chain(fields2)
                .map(|field| AstNode::from(field.clone()))
                .collect();
            self.context.report_error(GraphQLError::new(
                format!(
                    "Fields \"{response_name}\" conflict because {reason_msg}. Use different aliases on the fields to fetch both if this was intentional."
                ),
                GraphQLErrorOptions {
                    nodes: Some(nodes),
                    ..GraphQLErrorOptions::default()
                },
            ));
        }
        Ok(())
    }

    /// Finds all conflicts found "within" a selection set, including those
    /// found via spreading in fragments.
    ///
    /// Called when visiting each `SelectionSet` in the GraphQL Document.
    ///
    /// Derived from graphql-js 16.12.0, src/validation/rules/OverlappingFieldsCanBeMergedRule.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns what a thunk of the schema throws while a parent type lists
    /// its fields.
    fn find_conflicts_within_selection_set(
        &mut self,
        parent_type: Option<&NamedType>,
        selection_set: &'a SelectionSetNode,
    ) -> Result<Vec<Conflict<'a>>, DefinitionError> {
        let mut conflicts = Vec::new();

        let (field_map, fragment_names) =
            self.get_fields_and_fragment_names(parent_type, selection_set)?;

        // (A) Find find all conflicts "within" the fields of this selection set.
        // Note: this is the *only place* `collect_conflicts_within` is called.
        self.collect_conflicts_within(&mut conflicts, &field_map)?;

        if !fragment_names.is_empty() {
            // (B) Then collect conflicts between these fields and those represented by
            // each spread fragment name found.
            for (i, fragment_name) in fragment_names.iter().enumerate() {
                self.collect_conflicts_between_fields_and_fragment(
                    &mut conflicts,
                    false,
                    &field_map,
                    fragment_name,
                )?;
                // (C) Then compare this fragment with all other fragments found in this
                // selection set to collect conflicts between fragments spread together.
                // This compares each item in the list of fragment names to every other
                // item in that same list (except for itself).
                for other_fragment_name in &fragment_names[i + 1..] {
                    self.collect_conflicts_between_fragments(
                        &mut conflicts,
                        false,
                        fragment_name,
                        other_fragment_name,
                    )?;
                }
            }
        }
        Ok(conflicts)
    }

    /// Collects all conflicts found between a set of fields and a fragment
    /// reference including via spreading in any nested fragments.
    ///
    /// Derived from graphql-js 16.12.0, src/validation/rules/OverlappingFieldsCanBeMergedRule.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns what a thunk of the schema throws while a parent type lists
    /// its fields.
    fn collect_conflicts_between_fields_and_fragment(
        &mut self,
        conflicts: &mut Vec<Conflict<'a>>,
        are_mutually_exclusive: bool,
        field_map: &Rc<NodeAndDefCollection<'a>>,
        fragment_name: &'a str,
    ) -> Result<(), DefinitionError> {
        // Memoize so the fields and fragments are not compared for conflicts more
        // than once.
        let field_map_key = address_of(field_map.as_ref());
        if self.compared_fields_and_fragment_pairs.has(
            field_map_key,
            fragment_name,
            are_mutually_exclusive,
        ) {
            return Ok(());
        }
        self.compared_fields_and_fragment_pairs.add(
            field_map_key,
            fragment_name,
            are_mutually_exclusive,
        );

        let Some(fragment) = self.context.get_fragment(fragment_name) else {
            return Ok(());
        };

        let (field_map2, referenced_fragment_names) =
            self.get_referenced_fields_and_fragment_names(fragment)?;

        // Do not compare a fragment's fieldMap to itself.
        if Rc::ptr_eq(field_map, &field_map2) {
            return Ok(());
        }

        // (D) First collect any conflicts between the provided collection of fields
        // and the collection of fields represented by the given fragment.
        self.collect_conflicts_between(conflicts, are_mutually_exclusive, field_map, &field_map2)?;

        // (E) Then collect any conflicts between the provided collection of fields
        // and any fragment names found in the given fragment.
        for referenced_fragment_name in referenced_fragment_names.iter() {
            self.collect_conflicts_between_fields_and_fragment(
                conflicts,
                are_mutually_exclusive,
                field_map,
                referenced_fragment_name,
            )?;
        }
        Ok(())
    }

    /// Collects all conflicts found between two fragments, including via
    /// spreading in any nested fragments.
    ///
    /// Derived from graphql-js 16.12.0, src/validation/rules/OverlappingFieldsCanBeMergedRule.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns what a thunk of the schema throws while a parent type lists
    /// its fields.
    fn collect_conflicts_between_fragments(
        &mut self,
        conflicts: &mut Vec<Conflict<'a>>,
        are_mutually_exclusive: bool,
        fragment_name1: &'a str,
        fragment_name2: &'a str,
    ) -> Result<(), DefinitionError> {
        // No need to compare a fragment to itself.
        if fragment_name1 == fragment_name2 {
            return Ok(());
        }

        // Memoize so two fragments are not compared for conflicts more than once.
        if self
            .compared_fragment_pairs
            .has(fragment_name1, fragment_name2, are_mutually_exclusive)
        {
            return Ok(());
        }
        self.compared_fragment_pairs
            .add(fragment_name1, fragment_name2, are_mutually_exclusive);

        let fragment1 = self.context.get_fragment(fragment_name1);
        let fragment2 = self.context.get_fragment(fragment_name2);
        let (Some(fragment1), Some(fragment2)) = (fragment1, fragment2) else {
            return Ok(());
        };

        let (field_map1, referenced_fragment_names1) =
            self.get_referenced_fields_and_fragment_names(fragment1)?;
        let (field_map2, referenced_fragment_names2) =
            self.get_referenced_fields_and_fragment_names(fragment2)?;

        // (F) First, collect all conflicts between these two collections of fields
        // (not including any nested fragments).
        self.collect_conflicts_between(
            conflicts,
            are_mutually_exclusive,
            &field_map1,
            &field_map2,
        )?;

        // (G) Then collect conflicts between the first fragment and any nested
        // fragments spread in the second fragment.
        for referenced_fragment_name2 in referenced_fragment_names2.iter() {
            self.collect_conflicts_between_fragments(
                conflicts,
                are_mutually_exclusive,
                fragment_name1,
                referenced_fragment_name2,
            )?;
        }

        // (G) Then collect conflicts between the second fragment and any nested
        // fragments spread in the first fragment.
        for referenced_fragment_name1 in referenced_fragment_names1.iter() {
            self.collect_conflicts_between_fragments(
                conflicts,
                are_mutually_exclusive,
                referenced_fragment_name1,
                fragment_name2,
            )?;
        }
        Ok(())
    }

    /// Finds all conflicts found between two selection sets, including those
    /// found via spreading in fragments.
    ///
    /// Called when determining if conflicts exist between the sub-fields of two
    /// overlapping fields.
    ///
    /// Derived from graphql-js 16.12.0, src/validation/rules/OverlappingFieldsCanBeMergedRule.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns what a thunk of the schema throws while a parent type lists
    /// its fields.
    fn find_conflicts_between_sub_selection_sets(
        &mut self,
        are_mutually_exclusive: bool,
        parent_type1: Option<&NamedType>,
        selection_set1: &'a SelectionSetNode,
        parent_type2: Option<&NamedType>,
        selection_set2: &'a SelectionSetNode,
    ) -> Result<Vec<Conflict<'a>>, DefinitionError> {
        let mut conflicts = Vec::new();

        let (field_map1, fragment_names1) =
            self.get_fields_and_fragment_names(parent_type1, selection_set1)?;
        let (field_map2, fragment_names2) =
            self.get_fields_and_fragment_names(parent_type2, selection_set2)?;

        // (H) First, collect all conflicts between these two collections of field.
        self.collect_conflicts_between(
            &mut conflicts,
            are_mutually_exclusive,
            &field_map1,
            &field_map2,
        )?;

        // (I) Then collect conflicts between the first collection of fields and
        // those referenced by each fragment name associated with the second.
        for fragment_name2 in fragment_names2.iter() {
            self.collect_conflicts_between_fields_and_fragment(
                &mut conflicts,
                are_mutually_exclusive,
                &field_map1,
                fragment_name2,
            )?;
        }

        // (I) Then collect conflicts between the second collection of fields and
        // those referenced by each fragment name associated with the first.
        for fragment_name1 in fragment_names1.iter() {
            self.collect_conflicts_between_fields_and_fragment(
                &mut conflicts,
                are_mutually_exclusive,
                &field_map2,
                fragment_name1,
            )?;
        }

        // (J) Also collect conflicts between any fragment names by the first and
        // fragment names by the second. This compares each item in the first set of
        // names to each item in the second set of names.
        for fragment_name1 in fragment_names1.iter() {
            for fragment_name2 in fragment_names2.iter() {
                self.collect_conflicts_between_fragments(
                    &mut conflicts,
                    are_mutually_exclusive,
                    fragment_name1,
                    fragment_name2,
                )?;
            }
        }
        Ok(conflicts)
    }

    /// Collects all Conflicts "within" one collection of fields.
    ///
    /// Derived from graphql-js 16.12.0, src/validation/rules/OverlappingFieldsCanBeMergedRule.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns what a thunk of the schema throws while a parent type lists
    /// its fields.
    fn collect_conflicts_within(
        &mut self,
        conflicts: &mut Vec<Conflict<'a>>,
        field_map: &NodeAndDefCollection<'a>,
    ) -> Result<(), DefinitionError> {
        // A field map is a keyed collection, where each key represents a response
        // name and the value at that key is a list of all fields which provide that
        // response name. For every response name, if there are multiple fields, they
        // must be compared to find a potential conflict.
        for (response_name, fields) in field_map {
            // This compares every field in the list to every other field in this list
            // (except to itself). If the list only has one item, nothing needs to
            // be compared.
            if fields.len() <= 1 {
                continue;
            }
            for (i, field1) in fields.iter().enumerate() {
                for field2 in &fields[i + 1..] {
                    // Within one collection is never mutually exclusive.
                    let conflict = self.find_conflict(false, response_name, field1, field2)?;
                    conflicts.extend(conflict);
                }
            }
        }
        Ok(())
    }

    /// Collects all Conflicts between two collections of fields.
    ///
    /// This is similar to, but different from the `collect_conflicts_within` function
    /// above. This check assumes that `collect_conflicts_within` has already been
    /// called on each provided collection of fields. This is true because this
    /// validator traverses each individual selection set.
    ///
    /// Derived from graphql-js 16.12.0, src/validation/rules/OverlappingFieldsCanBeMergedRule.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns what a thunk of the schema throws while a parent type lists
    /// its fields.
    fn collect_conflicts_between(
        &mut self,
        conflicts: &mut Vec<Conflict<'a>>,
        parent_fields_are_mutually_exclusive: bool,
        field_map1: &NodeAndDefCollection<'a>,
        field_map2: &NodeAndDefCollection<'a>,
    ) -> Result<(), DefinitionError> {
        // A field map is a keyed collection, where each key represents a response
        // name and the value at that key is a list of all fields which provide that
        // response name. For any response name which appears in both provided field
        // maps, each field from the first field map must be compared to every field
        // in the second field map to find potential conflicts.
        for (response_name, fields1) in field_map1 {
            let Some(fields2) = field_map2.get(response_name) else {
                continue;
            };
            for field1 in fields1 {
                for field2 in fields2 {
                    let conflict = self.find_conflict(
                        parent_fields_are_mutually_exclusive,
                        response_name,
                        field1,
                        field2,
                    )?;
                    conflicts.extend(conflict);
                }
            }
        }
        Ok(())
    }

    /// Determines if there is a conflict between two particular fields,
    /// including comparing their sub-fields.
    ///
    /// The block under `!areMutuallyExclusive` is [`find_field_conflict()`].
    ///
    /// Derived from graphql-js 16.12.0, src/validation/rules/OverlappingFieldsCanBeMergedRule.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns what a thunk of the schema throws while a parent type lists
    /// its fields.
    fn find_conflict(
        &mut self,
        parent_fields_are_mutually_exclusive: bool,
        response_name: &str,
        field1: &NodeAndDef<'a>,
        field2: &NodeAndDef<'a>,
    ) -> Result<Option<Conflict<'a>>, DefinitionError> {
        let (parent_type1, node1, def1) = field1;
        let (parent_type2, node2, def2) = field2;

        // If it is known that two fields could not possibly apply at the same
        // time, due to the parent types, then it is safe to permit them to diverge
        // in aliased field or arguments used as they will not present any ambiguity
        // by differing.
        // It is known that two parent types could never overlap if they are
        // different Object types. Interface or Union types might overlap, if not
        // in the current state of the schema, then perhaps in some future version,
        // thus may not safely diverge.
        let are_mutually_exclusive = parent_fields_are_mutually_exclusive
            || (parent_type1 != parent_type2
                && is_object_type(parent_type1.as_ref())
                && is_object_type(parent_type2.as_ref()));

        if !are_mutually_exclusive && let Some(reason) = find_field_conflict(node1, node2) {
            return Ok(Some((
                (response_name.to_owned(), reason),
                vec![node1],
                vec![node2],
            )));
        }

        // The return type for each field.
        let type1 = def1.as_ref().and_then(FieldDef::get).map(|def| &def.r#type);
        let type2 = def2.as_ref().and_then(FieldDef::get).map(|def| &def.r#type);

        if let (Some(type1), Some(type2)) = (type1, type2)
            && do_types_conflict(type1, type2)
        {
            let reason = ConflictReasonMessage::Message(format!(
                "they return conflicting types \"{type1}\" and \"{type2}\""
            ));
            return Ok(Some((
                (response_name.to_owned(), reason),
                vec![node1],
                vec![node2],
            )));
        }

        // Collect and compare sub-fields. Use the same "visited fragment names" list
        // for both collections so fields in a fragment reference are never
        // compared to themselves.
        let selection_set1 = node1.selection_set.as_ref();
        let selection_set2 = node2.selection_set.as_ref();
        if let (Some(selection_set1), Some(selection_set2)) = (selection_set1, selection_set2) {
            let conflicts = self.find_conflicts_between_sub_selection_sets(
                are_mutually_exclusive,
                type1.map(get_named_type).as_ref(),
                selection_set1,
                type2.map(get_named_type).as_ref(),
                selection_set2,
            )?;
            return Ok(subfield_conflicts(&conflicts, response_name, node1, node2));
        }
        Ok(None)
    }

    /// Given a selection set, returns the collection of fields (a mapping of
    /// response name to field nodes and definitions) as well as a list of
    /// fragment names referenced via fragment spreads.
    ///
    /// The `fragmentNames` of the reference is an object whose keys are the result,
    /// which keeps each name once at the position of its first spread, and the map
    /// here does the same.
    ///
    /// Derived from graphql-js 16.12.0, src/validation/rules/OverlappingFieldsCanBeMergedRule.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns what a thunk of the schema throws while a parent type lists
    /// its fields.
    fn get_fields_and_fragment_names(
        &mut self,
        parent_type: Option<&NamedType>,
        selection_set: &'a SelectionSetNode,
    ) -> Result<FieldsAndFragmentNames<'a>, DefinitionError> {
        let key = address_of(selection_set);
        if let Some(cached) = self.cached_fields_and_fragment_names.get(&key) {
            return Ok(cached.clone());
        }
        let mut node_and_defs = NodeAndDefCollection::default();
        let mut fragment_names: OrderedMap<&'a str, bool> = OrderedMap::default();
        self.collect_fields_and_fragment_names(
            parent_type,
            selection_set,
            &mut node_and_defs,
            &mut fragment_names,
        )?;
        let result = (
            Rc::new(node_and_defs),
            fragment_names.into_keys().collect::<FragmentNames<'a>>(),
        );
        self.cached_fields_and_fragment_names
            .insert(key, result.clone());
        Ok(result)
    }

    /// Given a reference to a fragment, returns the represented collection of
    /// fields as well as a list of nested fragment names referenced via
    /// fragment spreads.
    ///
    /// Derived from graphql-js 16.12.0, src/validation/rules/OverlappingFieldsCanBeMergedRule.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns what a thunk of the schema throws while a parent type lists
    /// its fields.
    fn get_referenced_fields_and_fragment_names(
        &mut self,
        fragment: &'a FragmentDefinitionNode,
    ) -> Result<FieldsAndFragmentNames<'a>, DefinitionError> {
        // Short-circuit building a type from the node if possible.
        let key = address_of(&fragment.selection_set);
        if let Some(cached) = self.cached_fields_and_fragment_names.get(&key) {
            return Ok(cached.clone());
        }

        let fragment_type =
            named_type_from_ast(self.context.get_schema(), &fragment.type_condition);
        self.get_fields_and_fragment_names(fragment_type.as_ref(), &fragment.selection_set)
    }

    /// Collects the fields and the fragment names of `selection_set` into
    /// `node_and_defs` and `fragment_names`, inline fragments included.
    ///
    /// `parent_type` is the type the fields of the selection set belong to.
    ///
    /// Derived from graphql-js 16.12.0, src/validation/rules/OverlappingFieldsCanBeMergedRule.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns what a thunk of the schema throws while a parent type lists
    /// its fields.
    fn collect_fields_and_fragment_names(
        &self,
        parent_type: Option<&NamedType>,
        selection_set: &'a SelectionSetNode,
        node_and_defs: &mut NodeAndDefCollection<'a>,
        fragment_names: &mut OrderedMap<&'a str, bool>,
    ) -> Result<(), DefinitionError> {
        for selection in selection_set.selections.iter() {
            match selection {
                SelectionNode::Field(selection) => {
                    let field_name = selection.name.value();
                    let field_def = field_def_of(parent_type, field_name)?;
                    let response_name = selection
                        .alias
                        .as_ref()
                        .map_or(field_name, |alias| alias.value());
                    node_and_defs.entry(response_name).or_default().push((
                        parent_type.cloned(),
                        selection,
                        field_def,
                    ));
                }
                SelectionNode::FragmentSpread(selection) => {
                    fragment_names.insert(selection.name.value(), true);
                }
                SelectionNode::InlineFragment(selection) => {
                    let type_condition = selection.type_condition.as_ref();
                    let inline_fragment_type = match type_condition {
                        Some(type_condition) => {
                            named_type_from_ast(self.context.get_schema(), type_condition)
                        }
                        None => parent_type.cloned(),
                    };
                    self.collect_fields_and_fragment_names(
                        inline_fragment_type.as_ref(),
                        &selection.selection_set,
                        node_and_defs,
                        fragment_names,
                    )?;
                }
            }
        }
        Ok(())
    }
}

/// Returns whether `type` is an object type, the `isObjectType` of the
/// reference over a type that may be absent.
fn is_object_type(r#type: Option<&NamedType>) -> bool {
    matches!(r#type, Some(NamedType::Object(_)))
}

/// Returns the reason two fields that may apply at the same time differ in
/// what they call, the block under `!areMutuallyExclusive` of `findConflict`.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/OverlappingFieldsCanBeMergedRule.ts, MIT.
fn find_field_conflict(node1: &FieldNode, node2: &FieldNode) -> Option<ConflictReasonMessage> {
    // Two aliases must refer to the same field.
    let name1 = node1.name.value();
    let name2 = node2.name.value();
    if name1 != name2 {
        return Some(ConflictReasonMessage::Message(format!(
            "\"{name1}\" and \"{name2}\" are different fields"
        )));
    }

    // Two field calls must have the same arguments.
    if !same_arguments(node1, node2) {
        return Some(ConflictReasonMessage::Message(String::from(
            "they have differing arguments",
        )));
    }
    None
}

/// Returns whether two fields carry the same arguments, in any order.
///
/// The reference also types a directive node here and calls the function with field
/// nodes alone. `new Map` of a list of entries keeps the last value of a repeated key,
/// and so does the collected map.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/OverlappingFieldsCanBeMergedRule.ts, MIT.
fn same_arguments(node1: &FieldNode, node2: &FieldNode) -> bool {
    let args1 = node1.arguments.as_deref();
    let args2 = node2.arguments.as_deref();

    let Some(args1) = args1.filter(|args| !args.is_empty()) else {
        return args2.is_none_or(<[_]>::is_empty);
    };
    let Some(args2) = args2.filter(|args| !args.is_empty()) else {
        return false;
    };

    if args1.len() != args2.len() {
        return false;
    }

    let args2_values: OrderedMap<&str, &ValueNode> = args2
        .iter()
        .map(|arg| (arg.name.value(), &arg.value))
        .collect();
    args1.iter().all(|arg1| {
        let value1 = &arg1.value;
        let Some(value2) = args2_values.get(arg1.name.value()) else {
            return false;
        };

        stringify_value(value1) == stringify_value(value2)
    })
}

/// Prints a value with the fields of every object in natural order.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/OverlappingFieldsCanBeMergedRule.ts, MIT.
fn stringify_value(value: &ValueNode) -> String {
    print(&sort_value_node(value))
}

/// Returns whether two types conflict, which they do if both types could
/// not apply to a value simultaneously.
///
/// Composite types are ignored as their individual field types will be compared later
/// recursively. However List and Non-Null types must match. The `!==` of two leaf
/// types is the comparison of [`Type`], which tells two named types apart by identity.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/OverlappingFieldsCanBeMergedRule.ts, MIT.
fn do_types_conflict(type1: &Type, type2: &Type) -> bool {
    if let Some(list1) = type1.as_list() {
        return match type2.as_list() {
            Some(list2) => do_types_conflict(&list1.of_type, &list2.of_type),
            None => true,
        };
    }
    if is_list_type(type2) {
        return true;
    }
    if let Some(non_null1) = type1.as_non_null() {
        return match type2.as_non_null() {
            Some(non_null2) => do_types_conflict(non_null1.of_type(), non_null2.of_type()),
            None => true,
        };
    }
    if is_non_null_type(type2) {
        return true;
    }
    if is_leaf_type(type1) || is_leaf_type(type2) {
        return type1 != type2;
    }
    false
}

/// Returns the definition of the field `field_name` on `parent_type`, the
/// `fieldDef` of `_collectFieldsAndFragmentNames`.
///
/// The field map of the reference is a null prototype object, so a name reads a field
/// of the type or nothing.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/OverlappingFieldsCanBeMergedRule.ts, MIT.
///
/// # Errors
///
/// Returns what the field thunk of `parent_type` throws.
fn field_def_of(
    parent_type: Option<&NamedType>,
    field_name: &str,
) -> Result<Option<FieldDef>, DefinitionError> {
    match parent_type {
        Some(NamedType::Object(object)) => Ok(object
            .get_fields()?
            .get_index_of(field_name)
            .map(|index| FieldDef::Object(Arc::clone(object), index))),
        Some(NamedType::Interface(interface)) => Ok(interface
            .get_fields()?
            .get_index_of(field_name)
            .map(|index| FieldDef::Interface(Arc::clone(interface), index))),
        _ => Ok(None),
    }
}

/// Returns the type of `schema` that a type condition names.
fn named_type_from_ast(schema: &Schema, node: &NamedTypeNode) -> Option<NamedType> {
    schema.get_type(node.name.value()).cloned()
}

/// Given a series of Conflicts which occurred between two sub-fields,
/// generates a single Conflict.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/OverlappingFieldsCanBeMergedRule.ts, MIT.
fn subfield_conflicts<'a>(
    conflicts: &[Conflict<'a>],
    response_name: &str,
    node1: &'a FieldNode,
    node2: &'a FieldNode,
) -> Option<Conflict<'a>> {
    if !conflicts.is_empty() {
        let reasons = conflicts.iter().map(|(reason, _, _)| reason.clone());
        let fields1 = conflicts.iter().flat_map(|(_, fields1, _)| fields1);
        let fields2 = conflicts.iter().flat_map(|(_, _, fields2)| fields2);
        return Some((
            (
                response_name.to_owned(),
                ConflictReasonMessage::Reasons(reasons.collect()),
            ),
            core::iter::once(node1).chain(fields1.copied()).collect(),
            core::iter::once(node2).chain(fields2.copied()).collect(),
        ));
    }
    None
}

/// A way to keep track of pairs of things where the ordering of the pair
/// matters.
///
/// Provides a third argument for has/set to allow flagging the pair as weakly or
/// strongly present within the collection.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/OverlappingFieldsCanBeMergedRule.ts, MIT.
struct OrderedPairSet<T, U> {
    /// The flag of every pair, by the first thing and then by the second,
    /// the `_data` of the reference.
    data: OrderedMap<T, OrderedMap<U, bool>>,
}

impl<T: Copy + Hash + Eq, U: Copy + Hash + Eq> OrderedPairSet<T, U> {
    /// Creates an empty set, the constructor of the reference.
    fn new() -> Self {
        Self {
            data: OrderedMap::default(),
        }
    }

    /// Returns whether the pair of `a` and `b` is present as `weakly_present`
    /// asks.
    fn has(&self, a: T, b: U, weakly_present: bool) -> bool {
        let Some(result) = self.data.get(&a).and_then(|map| map.get(&b)) else {
            return false;
        };

        if weakly_present {
            true
        } else {
            weakly_present == *result
        }
    }

    /// Adds the pair of `a` and `b` with the flag `weakly_present`, which
    /// replaces the flag of a pair that is present.
    fn add(&mut self, a: T, b: U, weakly_present: bool) {
        match self.data.get_mut(&a) {
            None => {
                let mut map = OrderedMap::default();
                map.insert(b, weakly_present);
                self.data.insert(a, map);
            }
            Some(map) => {
                map.insert(b, weakly_present);
            }
        }
    }
}

/// A way to keep track of pairs of similar things when the ordering of the
/// pair does not matter.
///
/// The `<` of the reference orders two strings by their code units. The order decides
/// only which of the two things keys the outer map, and both methods decide alike, so
/// any total order gives the answers of the reference. The rule holds fragment names
/// in it, which are ASCII, and on ASCII the byte order of `str` is the code unit
/// order.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/OverlappingFieldsCanBeMergedRule.ts, MIT.
struct PairSet<T> {
    /// The pairs, each under its smaller thing first, the `_orderedPairSet`
    /// of the reference.
    ordered_pair_set: OrderedPairSet<T, T>,
}

impl<T: Copy + Hash + Eq + Ord> PairSet<T> {
    /// Creates an empty set, the constructor of the reference.
    fn new() -> Self {
        Self {
            ordered_pair_set: OrderedPairSet::new(),
        }
    }

    /// Returns whether the pair of `a` and `b` is present as `weakly_present`
    /// asks, in either order.
    fn has(&self, a: T, b: T, weakly_present: bool) -> bool {
        if a < b {
            self.ordered_pair_set.has(a, b, weakly_present)
        } else {
            self.ordered_pair_set.has(b, a, weakly_present)
        }
    }

    /// Adds the pair of `a` and `b` with the flag `weakly_present`.
    fn add(&mut self, a: T, b: T, weakly_present: bool) {
        if a < b {
            self.ordered_pair_set.add(a, b, weakly_present);
        } else {
            self.ordered_pair_set.add(b, a, weakly_present);
        }
    }
}

/// Returns a sorted copy of the given `ValueNode`.
///
/// Derived from graphql-js 16.12.0, src/utilities/sortValueNode.ts, MIT.
fn sort_value_node(value_node: &ValueNode) -> ValueNode {
    match value_node {
        ValueNode::ObjectValue(value_node) => ValueNode::ObjectValue(ObjectValueNode {
            loc: value_node.loc.clone(),
            fields: sort_fields(&value_node.fields),
        }),
        ValueNode::ListValue(value_node) => ValueNode::ListValue(ListValueNode {
            loc: value_node.loc.clone(),
            values: value_node.values.iter().map(sort_value_node).collect(),
        }),
        ValueNode::IntValue(_)
        | ValueNode::FloatValue(_)
        | ValueNode::StringValue(_)
        | ValueNode::BooleanValue(_)
        | ValueNode::NullValue(_)
        | ValueNode::EnumValue(_)
        | ValueNode::Variable(_) => value_node.clone(),
    }
}

/// Returns a copy of `fields` in the natural order of their names, each
/// with its value sorted.
///
/// The sort of the reference is stable, and so is the sort of a slice.
///
/// Derived from graphql-js 16.12.0, src/utilities/sortValueNode.ts, MIT.
fn sort_fields(fields: &[ObjectFieldNode]) -> Vec<ObjectFieldNode> {
    let mut sorted: Vec<ObjectFieldNode> = fields
        .iter()
        .map(|field_node| ObjectFieldNode {
            loc: field_node.loc.clone(),
            name: field_node.name.clone(),
            value: sort_value_node(&field_node.value),
        })
        .collect();
    sorted.sort_by(|field_a, field_b| natural_compare(field_a.name.value(), field_b.name.value()));
    sorted
}

/// Creates the visitor of `OverlappingFieldsCanBeMergedRule` over `context`.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/OverlappingFieldsCanBeMergedRule.ts, MIT.
#[must_use]
pub fn overlapping_fields_can_be_merged_rule<'c, 'a>(
    context: &'c ValidationContext<'a>,
) -> Box<dyn EnterLeave<'a> + 'c> {
    OverlappingFieldsCanBeMergedRule::boxed(context)
}
