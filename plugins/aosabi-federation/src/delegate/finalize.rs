// SPDX-License-Identifier: MIT
/*
 * aosabi-federation: the last pass over the document a delegation sends.
 *
 * Derived from @graphql-tools/delegate 12.0.9, dist/index.cjs, MIT.
 */

//! The finalization of the outbound request, `finalize_gateway_request`.
//!
//! After the transforms ran, the delegation keeps of its document what the
//! target schema can answer. Only the fragments whose type condition the
//! target knows stay, and each selection set is filtered with the type
//! information of the target: a field the parent type lacks, an argument the
//! field does not declare, a composite field whose selection set ends empty
//! and a fragment on a type the parent does not overlap are dropped, the
//! `__typename` fields of a set are removed and one is put first when one was
//! there or the parent type is abstract, and an inline fragment that ends
//! empty is dropped. The variable definitions shrink to the variables still
//! used, in the order of their first use, and the variables to their values.
//! A document with no operation left throws the refusal with the extension
//! `CRITICAL_ERROR`.
//!
//! The walk of the reference is `visit` with `visitWithTypeInfo` over the
//! key map `filteredSelectionSetVisitorKeys`, and a second `visit` over
//! `variablesVisitorKeys` collects the variables. Both maps are reproduced
//! as the order of the walks below. The walk is written out per kind, since
//! a field of a union parent answers a list of inline fragments, which the
//! reference stores in place of the field and a selection list here cannot
//! hold. The port puts the fragments into the list in place of the field.
//!
//! The two documents print alike once the strip of the wire removes the comma
//! with which the printer of the reference joins the stored list, but they do
//! not run alike. The executor of the reference collects the fields of a
//! selection set by their `kind` and skips the stored list, and a delegation
//! from a field that holds it reads `name.value` of every selection that is
//! no fragment in `visitSelectionSet` and throws
//! `TypeError: Cannot read properties of undefined (reading 'value')` at the
//! list. The federation finalizes its outer request against the wrapping
//! schema and runs it with the local executor, whose proxying root resolver
//! delegates again, so a need on a field whose remote type is a union answers
//! that `TypeError` as the error of the field and sends no data request. No
//! target schema of the three pairs declares a union, so only that case
//! reaches the branch.
//!
//! The type information of a selection set is made fresh per call, where the
//! reference keeps one per schema and type in `getTypeInfoWithType`. A visit
//! leaves its type information as it found it, so the two agree unless a
//! field of a union parent is filtered with the very type information the
//! walk around it holds, which needs a member field of the type that roots
//! the walk and is left out. The branch that asks a merged type
//! configuration for an override and the branch that adds the provided
//! selections of the stitching information read members the federation
//! never sets, `merge` of a subschema configuration and the `stitchingInfo`
//! extension, and are left out with `handleOverrideByDelegation` and
//! `getTypeInfo`, which only they call.

use std::rc::Rc;

use aosabi_execute::{
    CRITICAL_ERROR,
    errors::thrown_definition_error,
    value::{
        ErrorObject,
        Value, //
    }, //
};
use aosabi_json::{
    JsObject,
    JsValue,
    map::{
        IndexMap,
        IndexSet, //
    }, //
};
use aosabi_lang::{
    GraphQLErrorOptions,
    SharedSlice,
    ast::{
        ArgumentNode,
        DefinitionNode,
        DirectiveNode,
        DocumentNode,
        FieldNode,
        FragmentDefinitionNode,
        FragmentSpreadNode,
        InlineFragmentNode,
        NameNode,
        NamedTypeNode,
        OperationDefinitionNode,
        OperationTypeNode,
        SelectionNode,
        SelectionSetNode,
        ValueNode,
        VariableDefinitionNode, //
    },
    visitor::NodeRef, //
};
use aosabi_type::{
    DefinitionError,
    Field,
    InputField,
    NamedType,
    ObjMap,
    Schema,
    Type,
    get_named_type,
    is_leaf_type,
    is_nullable_type,
    type_info::TypeInfo, //
};

use crate::{
    transforms::{
        DelegationContext,
        ExecutionRequest,
        Variables, //
    },
    utils::{
        create_graphql_error,
        get_defined_root_type,
        implements_abstract_type,
        is_abstract,
        is_composite,
        is_nullish, //
    }, //
};

/// The operations and the fragments of a document and the names of the fragments.
pub struct DocumentMetadata<'d> {
    /// The operation definitions, in document order.
    pub operations: Vec<&'d OperationDefinitionNode>,
    /// The fragment definitions, in document order.
    pub fragments: Vec<&'d FragmentDefinitionNode>,
    /// The names of the fragments, each once.
    pub fragment_names: Vec<String>,
}

/// A selection set the finalization filtered, with the fragments and the variables it uses.
pub struct FinalizedSelectionSet {
    /// The filtered selection set.
    pub selection_set: SelectionSetNode,
    /// The names of the fragments its spreads use, in the order of the walk.
    pub used_fragments: Vec<String>,
    /// The names of the variables it uses, in the order of the walk.
    pub used_variables: Vec<String>,
}

/// The type of a valid fragment, keyed by the fragment's name, `validFragmentsWithType`.
///
/// A later fragment of a name replaces the type of an earlier one in its
/// place, as the assignment to the object of the reference does.
pub type ValidFragmentsWithType = IndexMap<String, NamedType>;

/// Returns the operations, the fragments and the fragment names of `document`.
#[must_use]
pub fn get_document_metadata(document: &DocumentNode) -> DocumentMetadata<'_> {
    let mut operations = Vec::new();
    let mut fragments = Vec::new();
    let mut fragment_names: IndexSet<&str> = IndexSet::default();
    for def in &document.definitions {
        match def {
            DefinitionNode::FragmentDefinition(def) => {
                fragments.push(def);
                fragment_names.insert(def.name.value());
            }
            DefinitionNode::OperationDefinition(def) => operations.push(def),
            _ => {}
        }
    }
    DocumentMetadata {
        operations,
        fragments,
        fragment_names: fragment_names.into_iter().map(String::from).collect(),
    }
}

/// Returns the request with its document reduced to what the target schema answers.
///
/// A variable the document still uses takes the value of the request, or else
/// the value of the client's variables when the request has none, and null
/// when either is null. `on_overlapping_aliases` is called when two inline
/// fragments of one selection set select a field of one name with a nullable
/// and a non null type, which the finalization aliases apart.
///
/// # Errors
///
/// Returns what [`finalize_gateway_document`] throws.
pub fn finalize_gateway_request(
    original_request: ExecutionRequest,
    delegation_context: &DelegationContext,
    on_overlapping_aliases: &mut dyn FnMut(),
) -> Result<ExecutionRequest, Value> {
    let metadata = get_document_metadata(&original_request.document);
    let (used_variables, new_document) = finalize_gateway_document(
        &delegation_context.target_schema,
        &metadata.fragments,
        &metadata.operations,
        on_overlapping_aliases,
    )?;
    let outer_variables = delegation_context
        .info
        .as_ref()
        .map(|info| Rc::clone(&info.variable_values));
    let mut new_variables = JsObject::new();
    for var_name in &used_variables {
        let existing_var = original_request
            .variables
            .as_ref()
            .and_then(|variables| variables.get(var_name));
        let outer_var = outer_variables
            .as_ref()
            .and_then(|variables| variables.get(var_name));
        if let Some(existing_var) = existing_var.filter(|value| !is_nullish(value)) {
            new_variables.insert(var_name.as_str(), existing_var.clone());
        } else if let Some(outer_var) = outer_var.filter(|value| !is_nullish(value)) {
            new_variables.insert(var_name.as_str(), outer_var.clone());
        }
        if matches!(existing_var, Some(JsValue::Null)) || matches!(outer_var, Some(JsValue::Null)) {
            new_variables.insert(var_name.as_str(), JsValue::Null);
        }
    }
    Ok(ExecutionRequest {
        document: Rc::new(new_document),
        variables: Some(Variables::from(new_variables)),
        ..original_request
    })
}

/// Returns the variables the finalized document uses and the document.
///
/// Each operation is filtered against its root type, the fragments it reaches
/// are finalized after it, and an operation left with `__typename` alone is
/// dropped. The fragments of the document are the new ones of the last
/// operation, as the reference assigns them per operation.
///
/// # Errors
///
/// Returns the `GraphQLError` `Failed to create a gateway request. The
/// request must contain at least one operation.` with the extension
/// `CRITICAL_ERROR` when no operation with a selection is left, the refusal
/// of [`get_defined_root_type`] for an operation the target cannot run, and
/// what the type information throws.
pub fn finalize_gateway_document(
    target_schema: &Schema,
    fragments: &[&FragmentDefinitionNode],
    operations: &[&OperationDefinitionNode],
    on_overlapping_aliases: &mut dyn FnMut(),
) -> Result<(Vec<String>, DocumentNode), Value> {
    let mut used_variables: IndexSet<String> = IndexSet::default();
    let mut used_fragments: IndexSet<String> = IndexSet::default();
    let mut new_operations = Vec::new();
    let mut new_fragments = Vec::new();
    let mut valid_fragments = Vec::new();
    let mut valid_fragments_with_type: ValidFragmentsWithType = IndexMap::default();
    for fragment in fragments {
        let type_name = fragment.type_condition.name.value();
        if let Some(r#type) = target_schema.get_type(type_name) {
            valid_fragments.push(*fragment);
            valid_fragments_with_type.insert(String::from(fragment.name.value()), r#type.clone());
        }
    }
    let mut fragment_set: IndexSet<String> = IndexSet::default();
    let mut selection_cnt = 0;
    for operation in operations {
        let r#type = get_defined_root_type(target_schema, operation.operation, None)?;
        let finalized = finalize_selection_set(
            target_schema,
            &r#type,
            &valid_fragments_with_type,
            &operation.selection_set,
            on_overlapping_aliases,
        )?;
        extend_once(&mut used_fragments, finalized.used_fragments);
        let used_fragment_list: Vec<String> = used_fragments.iter().cloned().collect();
        let collected = collect_fragment_variables(
            target_schema,
            fragment_set,
            &valid_fragments,
            &valid_fragments_with_type,
            &used_fragment_list,
            on_overlapping_aliases,
        )?;
        let mut operation_or_fragment_variables: IndexSet<String> = IndexSet::default();
        extend_once(
            &mut operation_or_fragment_variables,
            finalized.used_variables,
        );
        extend_once(
            &mut operation_or_fragment_variables,
            collected.used_variables,
        );
        extend_once(
            &mut used_variables,
            operation_or_fragment_variables.iter().cloned(),
        );
        new_fragments = collected.new_fragments;
        fragment_set = collected.fragment_set;
        let variable_definitions =
            variable_definitions_of(operation, &operation_or_fragment_variables);
        let mut selection_set = finalized.selection_set;
        if operation.operation == OperationTypeNode::Subscription {
            retain_selections(&mut selection_set.selections, |selection| {
                !is_field_named(selection, "__typename")
            });
        }
        if let [only] = &*selection_set.selections
            && is_field_named(only, "__typename")
        {
            continue;
        }
        selection_cnt += selection_set.selections.len();
        new_operations.push(DefinitionNode::OperationDefinition(
            OperationDefinitionNode {
                description: None,
                loc: None,
                operation: operation.operation,
                name: operation.name.clone(),
                variable_definitions: Some(variable_definitions),
                directives: operation.directives.clone(),
                selection_set,
            },
        ));
    }
    if new_operations.is_empty() || selection_cnt == 0 {
        let mut extensions = JsObject::new();
        extensions.insert(CRITICAL_ERROR, JsValue::Bool(true));
        return Err(Value::from(ErrorObject::GraphQL(create_graphql_error(
            "Failed to create a gateway request. The request must contain at least one operation.",
            GraphQLErrorOptions {
                extensions: Some(extensions),
                ..GraphQLErrorOptions::default()
            },
        ))));
    }
    new_operations.extend(
        new_fragments
            .into_iter()
            .map(DefinitionNode::FragmentDefinition),
    );
    let new_document = DocumentNode {
        loc: None,
        definitions: new_operations,
        token_count: None,
    };
    Ok((used_variables.into_iter().collect(), new_document))
}

/// Returns the definitions of `operation` that declare `names`, in the order of `names`.
///
/// A name takes the first definition that declares it, the `find` of the
/// reference, read through a map of the definitions by name, so the
/// definitions are linear in the variables.
fn variable_definitions_of(
    operation: &OperationDefinitionNode,
    names: &IndexSet<String>,
) -> Vec<VariableDefinitionNode> {
    let mut definitions_by_name: IndexMap<&str, &VariableDefinitionNode> = IndexMap::default();
    for var_def in operation.variable_definitions.iter().flatten() {
        definitions_by_name
            .entry(var_def.variable.name.value())
            .or_insert(var_def);
    }
    names
        .iter()
        .filter_map(|variable_name| definitions_by_name.get(variable_name.as_str()))
        .map(|var_def| (*var_def).clone())
        .collect()
}

/// The fragments a finalization reached and the variables their selections use.
pub struct CollectedFragmentVariables {
    /// The variables the finalized fragments use, each once.
    pub used_variables: Vec<String>,
    /// The fragments finalized for the first time, in the order they were finalized.
    pub new_fragments: Vec<FragmentDefinitionNode>,
    /// The names of every fragment finalized so far.
    pub fragment_set: IndexSet<String>,
}

/// Finalizes every fragment `used_fragments` reaches, and the fragments those reach.
///
/// The names are taken from the end of the list, a fragment's own fragments
/// join the list, and a finalized fragment keeps its name and its type
/// condition and drops its directives.
///
/// The reference unites the list with the names of each fragment it
/// finalizes, a pass over the whole list per fragment, and finalizes a
/// fragment again when it reaches it again. Here the list keeps a mark of
/// the names it holds, so a name joins it in one lookup once the first union
/// removed the repeated names of the list it starts from, and a fragment this
/// call finalized already is not finalized again. Its finalization answers
/// what it answered before, and every fragment it reaches was finalized or
/// is on the list since its first finalization, so the second one adds no
/// fragment, no variable and no order the reference's list does not have.
/// The work is linear in the fragments and their selections.
///
/// # Errors
///
/// Returns the plain error `Fragment reference type "<T>", but the type is
/// not contained within the target schema.`, which the list of valid
/// fragments rules out, and what [`finalize_selection_set`] throws.
pub fn collect_fragment_variables(
    target_schema: &Schema,
    mut fragment_set: IndexSet<String>,
    valid_fragments: &[&FragmentDefinitionNode],
    valid_fragments_with_type: &ValidFragmentsWithType,
    used_fragments: &[String],
    on_overlapping_aliases: &mut dyn FnMut(),
) -> Result<CollectedFragmentVariables, Value> {
    let mut first_of_name: IndexMap<&str, &FragmentDefinitionNode> = IndexMap::default();
    for fragment in valid_fragments {
        first_of_name
            .entry(fragment.name.value())
            .or_insert(fragment);
    }
    let mut remaining_fragments = used_fragments.to_vec();
    let mut on_list: Option<IndexMap<String, bool>> = None;
    let mut finalized_here: IndexSet<String> = IndexSet::default();
    let mut used_variables: IndexSet<String> = IndexSet::default();
    let mut new_fragments = Vec::new();
    while let Some(next_fragment_name) = remaining_fragments.pop() {
        if let Some(mark) = on_list
            .as_mut()
            .and_then(|marks| marks.get_mut(next_fragment_name.as_str()))
        {
            *mark = false;
        }
        let Some(fragment) = first_of_name.get(next_fragment_name.as_str()) else {
            continue;
        };
        let marks = on_list.get_or_insert_with(|| without_repeats(&mut remaining_fragments));
        if !finalized_here.insert(next_fragment_name.clone()) {
            continue;
        }
        let type_name = fragment.type_condition.name.value();
        let Some(r#type) = target_schema.get_type(type_name) else {
            return Err(Value::from(ErrorObject::error(format!(
                "Fragment reference type \"{type_name}\", but the type is not contained within the target schema."
            ))));
        };
        let finalized = finalize_selection_set(
            target_schema,
            r#type,
            valid_fragments_with_type,
            &fragment.selection_set,
            on_overlapping_aliases,
        )?;
        for name in finalized.used_fragments {
            let mark = marks.entry(name.clone()).or_insert(false);
            if !*mark {
                *mark = true;
                remaining_fragments.push(name);
            }
        }
        extend_once(&mut used_variables, finalized.used_variables);
        if !next_fragment_name.is_empty() && !fragment_set.contains(next_fragment_name.as_str()) {
            fragment_set.insert(next_fragment_name.clone());
            new_fragments.push(FragmentDefinitionNode {
                description: None,
                loc: None,
                name: NameNode::new(None, next_fragment_name),
                variable_definitions: None,
                type_condition: fragment.type_condition.clone(),
                directives: None,
                selection_set: finalized.selection_set,
            });
        }
    }
    Ok(CollectedFragmentVariables {
        used_variables: used_variables.into_iter().collect(),
        new_fragments,
        fragment_set,
    })
}

/// Removes every name of `names` after its first, and answers the mark of each name the list holds.
///
/// The first union of the reference rewrites the list it pops from without
/// its repeated names, which only a list a caller passes in can hold.
fn without_repeats(names: &mut Vec<String>) -> IndexMap<String, bool> {
    let mut marks: IndexMap<String, bool> = IndexMap::default();
    names.retain(|name| marks.insert(name.clone(), true).is_none());
    marks
}

/// Adds every name of `names` that `set` lacks, in their order.
fn extend_once(set: &mut IndexSet<String>, names: impl IntoIterator<Item = String>) {
    for name in names {
        set.insert(name);
    }
}

/// Filters one selection set against `type` and collects the fragments and the variables it uses.
///
/// The variables are collected by a second walk over the filtered set with
/// the key map `variablesVisitorKeys` of `:833-844`.
///
/// # Errors
///
/// Returns what the type information and the overlap of two types throw.
pub fn finalize_selection_set(
    schema: &Schema,
    r#type: &NamedType,
    valid_fragments: &ValidFragmentsWithType,
    selection_set: &SelectionSetNode,
    on_overlapping_aliases: &mut dyn FnMut(),
) -> Result<FinalizedSelectionSet, Value> {
    let mut used_fragments = Vec::new();
    let filtered_selection_set = filter_selection_set(
        schema,
        &Type::from(r#type),
        valid_fragments,
        selection_set,
        on_overlapping_aliases,
        &mut used_fragments,
    )?;
    let mut used_variables = Vec::new();
    variables_of_selection_set(&filtered_selection_set, &mut used_variables);
    Ok(FinalizedSelectionSet {
        selection_set: filtered_selection_set,
        used_fragments,
        used_variables,
    })
}

/// Filters a selection set against the type information that starts at `initial_type`.
///
/// The walk over the key map `filteredSelectionSetVisitorKeys`: a selection
/// set, its selections, the selection set of a field and of an inline
/// fragment, and the selection set of a fragment definition, which a walk
/// that starts at a selection set never meets. The names of the fragments the
/// spreads use are pushed to `used_fragments` in the order of the walk.
///
/// # Errors
///
/// Returns what the type information and the overlap of two types throw.
pub fn filter_selection_set(
    schema: &Schema,
    initial_type: &Type,
    valid_fragments: &ValidFragmentsWithType,
    selection_set: &SelectionSetNode,
    on_overlapping_aliases: &mut dyn FnMut(),
    used_fragments: &mut Vec<String>,
) -> Result<SelectionSetNode, Value> {
    let mut filter = Filter {
        schema,
        type_info: TypeInfo::new(schema, Some(initial_type), None),
        valid_fragments,
        on_overlapping_aliases,
        used_fragments,
    };
    filter.selection_set(selection_set)
}

/// Returns the names in the order they first appear in `arrays`, each once.
#[must_use]
pub fn union(arrays: &[&[String]]) -> Vec<String> {
    let mut result: IndexSet<String> = IndexSet::default();
    for array in arrays {
        extend_once(&mut result, array.iter().cloned());
    }
    result.into_iter().collect()
}

/// What a hook of the walk does with a selection.
// A step moves into the list of its level at once, so its width is paid on
// the stack of one level and a box would cost an allocation per selection.
// On a target of 32 bit pointers, the module of the TypeScript option among
// them, the variants weigh less and the lint does not fire.
#[cfg_attr(
    target_pointer_width = "64",
    expect(
        clippy::large_enum_variant,
        reason = "a step moves into the list of its level at once"
    )
)]
enum Step {
    /// The selection stays, as the edits of its children left it.
    Keep(SelectionNode),
    /// The selection is removed from its list.
    Remove,
    /// The selection is replaced by the inline fragments of a union field.
    Spread(Vec<SelectionNode>),
}

/// The fields of one selection list that the inline fragments in it selected, by nullability.
///
/// The `seenNonNullableMap` and `seenNullableMap` of the reference key
/// these sets by the list that holds the inline fragments, which is the list
/// the walk of one selection set visits, so one value per walked list holds
/// them.
#[derive(Default)]
struct SeenFields {
    /// The names selected with a non null type, `seenNonNullable`.
    non_nullable: IndexSet<String>,
    /// The names selected with a nullable type, `seenNullable`.
    nullable: IndexSet<String>,
}

/// The state of one walk of `filterSelectionSet`.
struct Filter<'w, 's> {
    /// The target schema.
    schema: &'s Schema,
    /// The type information of the walk.
    type_info: TypeInfo<'s>,
    /// The valid fragments and their types.
    valid_fragments: &'w ValidFragmentsWithType,
    /// The callback of an overlapping alias.
    on_overlapping_aliases: &'w mut dyn FnMut(),
    /// The fragment names the spreads use.
    used_fragments: &'w mut Vec<String>,
}

impl Filter<'_, '_> {
    /// Walks a selection set, whose enter hook always answers a copy of it.
    ///
    /// The copy the enter hook answers holds the selections without the
    /// `__typename` fields, with one first where the hook puts it, and the
    /// walk visits those selections in place, since the type information of
    /// a selection set reads none of them, so no selection is copied before
    /// the walk of its own node copies it.
    fn selection_set(&mut self, node: &SelectionSetNode) -> Result<SelectionSetNode, Value> {
        self.enter(NodeRef::from(node))?;
        let parent_type = self.type_info.get_parent_type().cloned();
        let has_type_name_field = node.selections.iter().any(is_type_name_field);
        let typename = (has_type_name_field || parent_type.as_ref().is_some_and(is_abstract))
            .then(typename_field);
        self.type_info.leave(NodeRef::from(node));
        self.enter(NodeRef::from(node))?;
        let mut seen = SeenFields::default();
        let mut out = Vec::with_capacity(node.selections.len() + 1);
        let replaced = typename.iter().chain(
            node.selections
                .iter()
                .filter(|selection| !is_type_name_field(selection)),
        );
        for selection in replaced {
            match self.selection(selection, &mut seen)? {
                Step::Keep(selection) => out.push(selection),
                Step::Remove => {}
                Step::Spread(fragments) => out.extend(fragments),
            }
        }
        self.type_info.leave(NodeRef::from(node));
        Ok(SelectionSetNode {
            loc: node.loc.clone(),
            selections: SharedSlice::from(out),
        })
    }

    /// Walks one selection of a list.
    fn selection(&mut self, node: &SelectionNode, seen: &mut SeenFields) -> Result<Step, Value> {
        match node {
            SelectionNode::Field(field) => self.field(field),
            SelectionNode::FragmentSpread(spread) => self.fragment_spread(spread),
            SelectionNode::InlineFragment(fragment) => self.inline_fragment(fragment, seen),
        }
    }

    /// Walks a field: its enter hook, its selection set and its leave hook.
    ///
    /// The copy of the field is made without its selection set, which the
    /// walk of the selection set answers, so a field is copied once however
    /// deep it stands.
    fn field(&mut self, node: &FieldNode) -> Result<Step, Value> {
        self.enter(NodeRef::from(node))?;
        let mut edited = match self.field_enter(node)? {
            FieldEnter::Keep => field_without_selections(node),
            FieldEnter::Remove => {
                self.type_info.leave(NodeRef::from(node));
                return Ok(Step::Remove);
            }
            FieldEnter::Spread(fragments) => {
                self.type_info.leave(NodeRef::from(node));
                return Ok(Step::Spread(fragments));
            }
            FieldEnter::Replace(replacement) => {
                self.type_info.leave(NodeRef::from(node));
                self.enter(NodeRef::from(&replacement))?;
                replacement
            }
        };
        if let Some(selection_set) = &node.selection_set {
            edited.selection_set = Some(self.selection_set(selection_set)?);
        }
        let keep = self.field_leave(&edited);
        self.type_info.leave(NodeRef::from(&edited));
        Ok(if keep {
            Step::Keep(SelectionNode::Field(edited))
        } else {
            Step::Remove
        })
    }

    /// The enter hook of a field, `[Kind.FIELD].enter` at `index.cjs:885-1010`.
    fn field_enter(&mut self, node: &FieldNode) -> Result<FieldEnter, Value> {
        let parent_type = self.type_info.get_parent_type().cloned();
        let field = self.type_info.get_field_def().cloned();
        if matches!(
            parent_type,
            Some(NamedType::Object(_) | NamedType::Interface(_))
        ) {
            let Some(field) = &field else {
                return Ok(FieldEnter::Remove);
            };
            if let Some(arguments) = &node.arguments {
                let new_args: Vec<ArgumentNode> = arguments
                    .iter()
                    .filter(|arg| {
                        field
                            .args
                            .iter()
                            .any(|known| known.name == arg.name.value())
                    })
                    .cloned()
                    .collect();
                if new_args.len() != arguments.len() {
                    return Ok(FieldEnter::Replace(FieldNode {
                        arguments: Some(new_args),
                        ..field_without_selections(node)
                    }));
                }
            }
        }
        if let Some(NamedType::Union(union)) = &parent_type
            && self.type_info.get_type().is_none()
        {
            let member_types = union.get_types().map_err(thrown_definition_error)?.to_vec();
            return self.union_field(node, &member_types);
        }
        Ok(FieldEnter::Keep)
    }

    /// The branch of the enter hook of a field whose parent is a union, `index.cjs:926-1008`.
    fn union_field(
        &mut self,
        node: &FieldNode,
        member_types: &[NamedType],
    ) -> Result<FieldEnter, Value> {
        let has_selections = node
            .selection_set
            .as_ref()
            .is_some_and(|set| !set.selections.is_empty());
        let field_name = node.name.value();
        let mut possible_type_names = Vec::new();
        for member_type in member_types {
            let Some(possible_field) = object_field(member_type, field_name)? else {
                continue;
            };
            let named_type = Type::from(get_named_type(&possible_field.r#type));
            if has_selections && is_leaf_type(&named_type) {
                continue;
            }
            if !has_selections && is_composite(&get_named_type(&possible_field.r#type)) {
                continue;
            }
            possible_type_names.push(String::from(member_type.name()));
        }
        if possible_type_names.is_empty() {
            return Ok(FieldEnter::Keep);
        }
        let mut non_empty_spreads = Vec::new();
        for possible_type_name in possible_type_names {
            let Some(selection_set) = &node.selection_set else {
                non_empty_spreads.push(inline_fragment_on(
                    &possible_type_name,
                    vec![SelectionNode::Field(node.clone())],
                ));
                continue;
            };
            let Some(possible_type) = self.schema.get_type(&possible_type_name) else {
                continue;
            };
            let Some(possible_field) = object_field(possible_type, field_name)? else {
                continue;
            };
            let field_filtered_selection_set = filter_selection_set(
                self.schema,
                &possible_field.r#type,
                self.valid_fragments,
                selection_set,
                self.on_overlapping_aliases,
                self.used_fragments,
            )?;
            if field_filtered_selection_set.selections.is_empty() {
                continue;
            }
            non_empty_spreads.push(inline_fragment_on(
                &possible_type_name,
                vec![SelectionNode::Field(FieldNode {
                    selection_set: Some(field_filtered_selection_set),
                    ..field_without_selections(node)
                })],
            ));
        }
        if non_empty_spreads.is_empty() {
            return Ok(FieldEnter::Keep);
        }
        Ok(FieldEnter::Spread(non_empty_spreads))
    }

    /// The leave hook of a field, `[Kind.FIELD].leave` at `index.cjs:1011-1027`, `true` to keep it.
    fn field_leave(&self, node: &FieldNode) -> bool {
        let Some(r#type) = self.type_info.get_type() else {
            return false;
        };
        let named_type = get_named_type(r#type);
        if self.schema.get_type(named_type.name()).is_none() {
            return false;
        }
        if matches!(named_type, NamedType::Object(_) | NamedType::Interface(_)) {
            let empty = node
                .selection_set
                .as_ref()
                .is_none_or(|set| set.selections.is_empty());
            if empty {
                return false;
            }
        }
        true
    }

    /// Walks a fragment spread, whose enter hook keeps a spread of a valid overlapping fragment.
    ///
    /// The hook of `[Kind.FRAGMENT_SPREAD].enter` at `index.cjs:1030-1041`. A
    /// spread has no key of the walk, so nothing below it is visited.
    fn fragment_spread(&mut self, node: &FragmentSpreadNode) -> Result<Step, Value> {
        let name = node.name.value();
        let Some(inner_type) = self.valid_fragments.get(name) else {
            return Ok(Step::Remove);
        };
        let parent_type = self.type_info.get_parent_type().cloned();
        if !implements_abstract_type(self.schema, parent_type.as_ref(), Some(inner_type))? {
            return Ok(Step::Remove);
        }
        self.used_fragments.push(String::from(name));
        Ok(Step::Keep(SelectionNode::FragmentSpread(node.clone())))
    }

    /// Walks an inline fragment: its enter hook, its selection set and its leave hook.
    fn inline_fragment(
        &mut self,
        node: &InlineFragmentNode,
        seen: &mut SeenFields,
    ) -> Result<Step, Value> {
        self.enter(NodeRef::from(node))?;
        if let Some(type_condition) = &node.type_condition {
            let parent_type = self.type_info.get_parent_type().cloned();
            let inner_type = self.schema.get_type(type_condition.name.value()).cloned();
            let member_of_union = match &parent_type {
                Some(NamedType::Union(union)) => {
                    let inner_name = inner_type.as_ref().map(NamedType::name);
                    union
                        .get_types()
                        .map_err(thrown_definition_error)?
                        .iter()
                        .any(|t| Some(t.name()) == inner_name)
                }
                _ => false,
            };
            if !member_of_union
                && !implements_abstract_type(
                    self.schema,
                    parent_type.as_ref(),
                    inner_type.as_ref(),
                )?
            {
                self.type_info.leave(NodeRef::from(node));
                return Ok(Step::Remove);
            }
        }
        let edited = InlineFragmentNode {
            loc: node.loc.clone(),
            type_condition: node.type_condition.clone(),
            directives: node.directives.clone(),
            selection_set: self.selection_set(&node.selection_set)?,
        };
        let Some(fragment) = self.inline_fragment_leave(edited, seen)? else {
            self.type_info.leave(NodeRef::from(node));
            return Ok(Step::Remove);
        };
        self.type_info.leave(NodeRef::from(&fragment));
        Ok(Step::Keep(SelectionNode::InlineFragment(fragment)))
    }

    /// The leave hook of an inline fragment, `[Kind.INLINE_FRAGMENT].leave` at `index.cjs:1078-1155`.
    ///
    /// A field of the fragment that another inline fragment of the same list
    /// selected with the other nullability gets the alias `_nullable_<name>`
    /// or `_nonNullable_<name>` and calls the callback of an overlapping
    /// alias, then the `__typename` fields are removed and the directive
    /// `@defer` dropped. `None` removes the fragment.
    fn inline_fragment_leave(
        &mut self,
        selection: InlineFragmentNode,
        seen: &mut SeenFields,
    ) -> Result<Option<InlineFragmentNode>, Value> {
        if selection.selection_set.selections.is_empty() {
            return Ok(None);
        }
        let mut selection = selection;
        if let Some(type_condition) = &selection.type_condition
            && let Some(selection_type) = self.schema.get_type(type_condition.name.value())
            && let Some(fields) = FieldTypes::of(selection_type)?
        {
            // The walk reads every field before an alias is written, since no
            // field reads the alias of another, and `make_mut` copies the
            // selections only where another node shares them.
            let mut aliases = Vec::new();
            for (index, sub_selection) in selection.selection_set.selections.iter().enumerate() {
                let SelectionNode::Field(field_node) = sub_selection else {
                    continue;
                };
                if field_node.alias.is_some() {
                    continue;
                }
                let field_name = String::from(field_node.name.value());
                let Some(field_type) = fields.get(&field_name) else {
                    continue;
                };
                let current_nullable = is_nullable_type(field_type);
                if current_nullable {
                    seen.nullable.insert(field_name.clone());
                } else {
                    seen.non_nullable.insert(field_name.clone());
                }
                if seen.nullable.contains(&field_name) && seen.non_nullable.contains(&field_name) {
                    (self.on_overlapping_aliases)();
                    let alias = if current_nullable {
                        format!("_nullable_{field_name}")
                    } else {
                        format!("_nonNullable_{field_name}")
                    };
                    aliases.push((index, NameNode::new(None, alias)));
                }
            }
            if !aliases.is_empty() {
                let selections = SharedSlice::make_mut(&mut selection.selection_set.selections);
                for (index, alias) in aliases {
                    if let SelectionNode::Field(field_node) = &mut selections[index] {
                        field_node.alias = Some(alias);
                    }
                }
            }
        }
        let (_, selections) = filter_typename_fields(&selection.selection_set.selections);
        if selections.is_empty() {
            return Ok(None);
        }
        selection.selection_set.selections = SharedSlice::from(selections);
        selection.directives = selection.directives.map(|directives| {
            directives
                .into_iter()
                .filter(|directive| directive.name.value() != "defer")
                .collect()
        });
        Ok(Some(selection))
    }

    /// Enters a node in the type information.
    fn enter(&mut self, node: NodeRef<'_>) -> Result<(), Value> {
        self.type_info.enter(node).map_err(thrown_definition_error)
    }
}

/// What the enter hook of a field answers.
// The answer is matched at once by the walk of the field, so its width is
// paid on the stack of one call. On a target of 32 bit pointers the
// variants weigh less and the lint does not fire.
#[cfg_attr(
    target_pointer_width = "64",
    expect(
        clippy::large_enum_variant,
        reason = "the answer is matched at once by the walk of the field"
    )
)]
enum FieldEnter {
    /// Visit the field as it is.
    Keep,
    /// Remove the field.
    Remove,
    /// Visit the copy with its unknown arguments dropped.
    Replace(FieldNode),
    /// Replace the field by the inline fragments of a union.
    Spread(Vec<SelectionNode>),
}

/// Returns whether a selection is a field without an alias named `__typename`.
fn is_type_name_field(selection: &SelectionNode) -> bool {
    matches!(selection, SelectionNode::Field(field) if field.alias.is_none() && field.name.value() == "__typename")
}

/// Returns whether a selection is a field named `name`, with or without an alias.
fn is_field_named(selection: &SelectionNode, name: &str) -> bool {
    matches!(selection, SelectionNode::Field(field) if field.name.value() == name)
}

/// Keeps the selections `keep` answers true for, in their order, as `Vec::retain` keeps them.
///
/// The selections stay as they are when `keep` keeps each, and a new slice
/// holds the kept ones otherwise, so a node that shares the old slice keeps
/// it whole.
fn retain_selections(
    selections: &mut SharedSlice<SelectionNode>,
    keep: impl Fn(&SelectionNode) -> bool,
) {
    if selections.iter().all(&keep) {
        return;
    }
    *selections = selections
        .iter()
        .filter(|selection| keep(selection))
        .cloned()
        .collect();
}

/// Returns whether the selections hold a `__typename` field and the selections without it.
fn filter_typename_fields(selections: &[SelectionNode]) -> (bool, Vec<SelectionNode>) {
    let mut has_type_name_field = false;
    let filtered_selections = selections
        .iter()
        .filter(|selection| {
            if is_type_name_field(selection) {
                has_type_name_field = true;
                return false;
            }
            true
        })
        .cloned()
        .collect();
    (has_type_name_field, filtered_selections)
}

/// Returns a copy of a field without its selection set.
fn field_without_selections(node: &FieldNode) -> FieldNode {
    FieldNode {
        loc: node.loc.clone(),
        alias: node.alias.clone(),
        name: node.name.clone(),
        arguments: node.arguments.clone(),
        directives: node.directives.clone(),
        selection_set: None,
    }
}

/// Returns the field `{ __typename }` the finalization puts first.
fn typename_field() -> SelectionNode {
    SelectionNode::Field(FieldNode {
        loc: None,
        alias: None,
        name: NameNode::new(None, "__typename"),
        arguments: None,
        directives: None,
        selection_set: None,
    })
}

/// Returns an inline fragment on the type named `type_name` holding `selections`.
fn inline_fragment_on(type_name: &str, selections: Vec<SelectionNode>) -> SelectionNode {
    SelectionNode::InlineFragment(InlineFragmentNode {
        loc: None,
        type_condition: Some(NamedTypeNode {
            loc: None,
            name: NameNode::new(None, type_name),
        }),
        directives: None,
        selection_set: SelectionSetNode {
            loc: None,
            selections: SharedSlice::from(selections),
        },
    })
}

/// Returns the field `name` of an object or an interface type, `type.getFields()[name]`.
fn object_field(named_type: &NamedType, name: &str) -> Result<Option<aosabi_type::Field>, Value> {
    let fields = match named_type {
        NamedType::Object(object) => object.get_fields(),
        NamedType::Interface(interface) => interface.get_fields(),
        _ => return Ok(None),
    };
    Ok(fields.map_err(thrown_definition_error)?.get(name).cloned())
}

/// The fields of a type that has `getFields`, an object, an interface or an input object.
enum FieldTypes<'t> {
    /// The fields of an object or an interface.
    Output(&'t ObjMap<Field>),
    /// The fields of an input object.
    Input(&'t ObjMap<InputField>),
}

impl<'t> FieldTypes<'t> {
    /// Returns the fields of `named_type`, `None` for a type without `getFields`.
    fn of(named_type: &'t NamedType) -> Result<Option<Self>, Value> {
        let map_err = |error: DefinitionError| thrown_definition_error(error);
        Ok(match named_type {
            NamedType::Object(object) => Some(Self::Output(object.get_fields().map_err(map_err)?)),
            NamedType::Interface(interface) => {
                Some(Self::Output(interface.get_fields().map_err(map_err)?))
            }
            NamedType::InputObject(input) => {
                Some(Self::Input(input.get_fields().map_err(map_err)?))
            }
            _ => None,
        })
    }

    /// Returns the type of the field `name`, `selectionTypeFields[fieldName].type`.
    fn get(&self, name: &str) -> Option<&'t Type> {
        match self {
            Self::Output(fields) => fields.get(name).map(|field| &field.r#type),
            Self::Input(fields) => fields.get(name).map(|field| &field.r#type),
        }
    }
}

/// Collects the variables of a selection set in the order of `variablesVisitorKeys`.
///
/// The key map of `index.cjs:833-844`: a selection set, the arguments, the
/// directives and the selection set of a field, the directives and the
/// selection set of an inline fragment, the directives of a spread, and the
/// values of an argument, of an object field and of a list.
fn variables_of_selection_set(node: &SelectionSetNode, used_variables: &mut Vec<String>) {
    for selection in node.selections.iter() {
        match selection {
            SelectionNode::Field(field) => {
                for argument in field.arguments.iter().flatten() {
                    variables_of_value(&argument.value, used_variables);
                }
                variables_of_directives(field.directives.as_deref(), used_variables);
                if let Some(selection_set) = &field.selection_set {
                    variables_of_selection_set(selection_set, used_variables);
                }
            }
            SelectionNode::InlineFragment(fragment) => {
                variables_of_directives(fragment.directives.as_deref(), used_variables);
                variables_of_selection_set(&fragment.selection_set, used_variables);
            }
            SelectionNode::FragmentSpread(spread) => {
                variables_of_directives(spread.directives.as_deref(), used_variables);
            }
        }
    }
}

/// Collects the variables of the arguments of directives.
fn variables_of_directives(directives: Option<&[DirectiveNode]>, used_variables: &mut Vec<String>) {
    for directive in directives.into_iter().flatten() {
        for argument in directive.arguments.iter().flatten() {
            variables_of_value(&argument.value, used_variables);
        }
    }
}

/// Collects the variables of a value, every occurrence in the order of the walk.
fn variables_of_value(value: &ValueNode, used_variables: &mut Vec<String>) {
    match value {
        ValueNode::Variable(variable) => used_variables.push(String::from(variable.name.value())),
        ValueNode::ObjectValue(object) => {
            for field in &object.fields {
                variables_of_value(&field.value, used_variables);
            }
        }
        ValueNode::ListValue(list) => {
            for item in &list.values {
                variables_of_value(item, used_variables);
            }
        }
        _ => {}
    }
}
