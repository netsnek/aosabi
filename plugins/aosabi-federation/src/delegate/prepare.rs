// SPDX-License-Identifier: MIT
/*
 * aosabi-federation: the first pass over the document a delegation sends.
 *
 * Derived from @graphql-tools/delegate 12.0.9, dist/index.cjs, MIT.
 */

//! The preparation of the outbound document, `prepare_gateway_document`.
//!
//! Before the transforms run, the delegation expands its document against the
//! schema of the field that delegates and the schema of the target.
//! `wrap_concrete_types` wraps the selection of a root field whose type is
//! abstract and named otherwise than the return type in inline fragments over
//! the possible types. Then every selection set of the operations and of the
//! fragments, with the fragments on an abstract type expanded per possible
//! type, is rewritten by `visit_selection_set`: an inline fragment on a type
//! the target lacks has its selections lifted to the parent, an inline
//! fragment on an abstract type is spread per possible type the target has, a
//! fragment spread names the expanded fragments, a field an interface of the
//! target lacks moves into inline fragments over the implementations, and
//! `__typename` is added where the reversed map of possible types names the
//! parent.
//!
//! The reference shares objects between the places of the tree it builds:
//! an inline fragment over a possible type holds the selection set of the
//! fragment it replaces, and an expanded fragment holds the selection set of
//! the fragment it expands. The walk then reaches a shared selection set
//! once per place, and the set `visitedSelections` lets the first visit of
//! an inline fragment alone add its extra possible types and lift its
//! fields. The walk here therefore runs on a tree of its own whose nodes sit
//! behind an `Rc`, built from the document once and converted back at the
//! end, so a shared node is shared and its identity is the identity the
//! reference tests. The visitor key maps of `:1209-1216` and `:1653-1659`
//! are the order of the two walks below.
//!
//! The stitching information the meta data reads, `fieldNodesByType`,
//! `fieldNodesByField` and `dynamicSelectionSetsByField`, is absent from
//! every schema of the federation, so the branches that read it add
//! nothing and are left out, `addDependenciesNestedly` with them. The meta
//! data is kept per pair of schemas, as the `memoize2` of `getSchemaMetaData`
//! keeps it, in a table of the runtime thread keyed by the addresses of the
//! two schemas, whose weak handles keep the allocations from every other
//! schema, so every delegation over one pair after the first reads it with
//! one lookup. The extra possible types of a type name are kept in the same
//! entry, as the `memoize2` of `getExtraPossibleTypesFn` keeps a `Map` per
//! pair from the type name to its answer. The type information is made per
//! walk where the reference keeps one per schema, which every balanced walk
//! leaves as it found it.
//!
//! The sets the walk keeps answer each question in one lookup: the names of
//! the fragments, the inline fragments visited and, once a selection set of
//! the walk holds more than `SMALL_SELECTIONS` selections, the selections
//! it added by identity and its fields by name and alias. A field that comes
//! again with a selection set is tested against the selection set of the
//! field before it, whose fields by name and alias are kept once that set
//! holds more than `SMALL_SELECTIONS` selections. The reference scans its
//! `Set` of selections for each field it adds and the selection set of the
//! field before it for each field of a repeated field, which makes a
//! selection set of `D` fields cost `D` squared comparisons there, and the
//! first field of a name and an alias decides in both, so the documents
//! stay the same.

use std::{
    cell::{
        Cell,
        RefCell, //
    },
    hash::{
        Hash,
        Hasher, //
    },
    rc::Rc,
    sync::{
        Arc,
        Weak, //
    }, //
};

use aosabi_execute::{
    errors::thrown_definition_error,
    value::Value, //
};
use aosabi_json::map::{
    IndexMap,
    IndexSet, //
};
use aosabi_lang::{
    SharedSlice,
    ast::{
        DefinitionNode,
        DocumentNode,
        FieldNode,
        FragmentDefinitionNode,
        FragmentSpreadNode,
        InlineFragmentNode,
        Location,
        NameNode,
        NamedTypeNode,
        OperationDefinitionNode,
        SelectionNode,
        SelectionSetNode, //
    },
    visitor::NodeRef, //
};
use aosabi_type::{
    DefinitionError,
    Field,
    NamedType,
    ObjMap,
    Schema,
    Type,
    get_named_type,
    type_info::TypeInfo, //
};

use crate::utils::{
    AddressState,
    get_root_type_names,
    implements_abstract_type,
    is_abstract, //
};

/// The possible type names of an abstract type the target has, by the name of the type.
pub type PossibleTypesMap = IndexMap<String, Vec<String>>;

/// The schema meta data the preparation reads, `getSchemaMetaData` without the stitching maps.
pub struct SchemaMetaData {
    /// The possible types the target has, `possibleTypesMap`.
    pub possible_types_map: PossibleTypesMap,
    /// The abstract types each possible type stands for, `reversePossibleTypesMap`.
    pub reverse_possible_types_map: PossibleTypesMap,
    /// The fields an interface of the source has beyond the target's, `interfaceExtensionsMap`.
    pub interface_extensions_map: IndexMap<String, Vec<String>>,
}

/// The number of selections up to which a selection set of the walk tests a selection by a scan.
///
/// A scan of a few selections is taken to cost less than the hash of a name
/// and an alias with `SipHash-1-3`, and the sets of the walk take over beyond
/// this count.
const SMALL_SELECTIONS: usize = 8;

/// The number of entries the table of the meta data holds before its first sweep.
const FIRST_SWEEP: usize = 16;

/// What the preparation keeps of one pair of schemas, with weak handles of the two.
struct MetaDataEntry {
    /// The schema of the field that delegates.
    source: Weak<Schema>,
    /// The schema the document is prepared against.
    target: Weak<Schema>,
    /// The meta data and the extra possible types of the pair.
    memo: Rc<PairMemo>,
}

/// The memos of one pair of schemas, those of `getSchemaMetaData` and of `getExtraPossibleTypesFn`.
struct PairMemo {
    /// The meta data of the pair.
    meta_data: SchemaMetaData,
    /// The extra possible types by type name, `extraPossiblesTypesMap` of `index.cjs:1242`.
    extra_possible_types: RefCell<IndexMap<String, Rc<[String]>>>,
}

thread_local! {
    /// The memos by the addresses of the source schema and the target schema.
    static META_DATA: RefCell<IndexMap<(usize, usize), MetaDataEntry, AddressState>> =
        RefCell::new(IndexMap::default());
    /// The size of [`META_DATA`] at which its next sweep runs.
    static NEXT_SWEEP: Cell<usize> = const { Cell::new(FIRST_SWEEP) };
}

/// One replacement of a spread of a fragment on an abstract type.
#[derive(Clone)]
pub struct FragmentReplacement {
    /// The name of the expanded fragment, `fragmentName`.
    pub fragment_name: String,
    /// The possible type it is on, `typeName`.
    pub type_name: String,
}

/// Returns the document a delegation prepares from `original_document` and the fragments that follow it.
///
/// The document of the reference ends with the fragments of the client, which
/// `client_fragments` holds here as the request shares them with the client's
/// request. Without `info_schema` the document is the one
/// `wrap_concrete_types` answers.
///
/// # Errors
///
/// Returns what the type information and the field and member thunks of the
/// two schemas throw.
pub fn prepare_gateway_document(
    original_document: &DocumentNode,
    client_fragments: &[Rc<FragmentDefinitionNode>],
    transformed_schema: &Arc<Schema>,
    return_type: &Type,
    info_schema: Option<&Arc<Schema>>,
) -> Result<DocumentNode, Value> {
    let mirrored = PDocument::from_ast(original_document, client_fragments);
    let wrapped_concrete_types_document =
        wrap_concrete_types(return_type, transformed_schema, &mirrored)?;
    let Some(info_schema) = info_schema else {
        return Ok(wrapped_concrete_types_document.to_ast());
    };
    let memo = pair_memo(info_schema, transformed_schema)?;
    let meta_data = &memo.meta_data;
    let mut operations = Vec::new();
    let mut fragments = Vec::new();
    let mut fragment_names: IndexSet<String> = IndexSet::default();
    for definition in &wrapped_concrete_types_document.definitions {
        match definition {
            PDefinition::Operation(..) => operations.push(definition.clone()),
            PDefinition::Fragment(node, _) => {
                fragments.push(definition.clone());
                fragment_names.insert(String::from(node.name.value()));
            }
            PDefinition::Other(_) => {}
        }
    }
    let (expanded_fragments, fragment_replacements) = get_expanded_fragments(
        &fragments,
        &mut fragment_names,
        &meta_data.possible_types_map,
    );
    let mut walk = PrepareWalk {
        transformed_schema,
        info_schema,
        type_info: TypeInfo::new(transformed_schema, None, None),
        meta_data,
        extra_possible_types: &memo.extra_possible_types,
        fragment_replacements: &fragment_replacements,
        visited_selections: IndexMap::default(),
        empty_set: empty_selection_set(),
    };
    let mut definitions = Vec::new();
    for definition in operations
        .iter()
        .chain(&fragments)
        .chain(&expanded_fragments)
    {
        definitions.push(walk.definition(definition)?);
    }
    Ok(PDocument {
        loc: None,
        definitions,
    }
    .to_ast())
}

/// Returns the memos of the pair of schemas, made once per pair, `memoize2(getSchemaMetaData)`.
///
/// The table holds a weak handle of each schema, so an entry is the pair
/// itself while it stands, and an entry whose schemas are gone is dropped
/// when the table grows. A pair whose meta data throws keeps no entry, as
/// the memo of the reference keeps nothing for a call that throws. The
/// extra possible types of the pair start empty and fill one type name at a
/// time, `memoize2(getExtraPossibleTypes)` with its `Map`.
///
/// # Errors
///
/// Returns what [`get_schema_meta_data`] throws.
fn pair_memo(
    source_schema: &Arc<Schema>,
    target_schema: &Arc<Schema>,
) -> Result<Rc<PairMemo>, Value> {
    let key = (
        Arc::as_ptr(source_schema).addr(),
        Arc::as_ptr(target_schema).addr(),
    );
    if let Some(memo) =
        META_DATA.with(|table| table.borrow().get(&key).map(|entry| Rc::clone(&entry.memo)))
    {
        return Ok(memo);
    }
    let memo = Rc::new(PairMemo {
        meta_data: get_schema_meta_data(source_schema, target_schema)?,
        extra_possible_types: RefCell::new(IndexMap::default()),
    });
    META_DATA.with(|table| {
        let mut table = table.borrow_mut();
        let due = NEXT_SWEEP.with(Cell::get);
        if table.len() >= due {
            let old = core::mem::take(&mut *table);
            for (pair, entry) in old {
                if entry.source.strong_count() > 0 && entry.target.strong_count() > 0 {
                    table.insert(pair, entry);
                }
            }
            NEXT_SWEEP.with(|next| next.set(FIRST_SWEEP.max(table.len() * 2)));
        }
        table.insert(
            key,
            MetaDataEntry {
                source: Arc::downgrade(source_schema),
                target: Arc::downgrade(target_schema),
                memo: Rc::clone(&memo),
            },
        );
    });
    Ok(memo)
}

/// Returns the possible types, their reverse and the interface extensions of two schemas.
///
/// An abstract type of the source whose name the target holds as an interface
/// records the fields it has beyond it, and an abstract type the target
/// extends or does not hold as an abstract type records its implementations
/// the target has.
///
/// # Errors
///
/// Returns what the field and member thunks of the two schemas throw.
pub fn get_schema_meta_data(
    source_schema: &Schema,
    target_schema: &Schema,
) -> Result<SchemaMetaData, Value> {
    let mut possible_types_map: PossibleTypesMap = IndexMap::default();
    let mut interface_extensions_map: IndexMap<String, Vec<String>> = IndexMap::default();
    for (type_name, r#type) in source_schema.get_type_map() {
        if !is_abstract(r#type) {
            continue;
        }
        let target_type = target_schema.get_type(type_name);
        if let (NamedType::Interface(source), Some(NamedType::Interface(target))) =
            (r#type, target_type)
        {
            let target_type_fields = target.get_fields().map_err(thrown_definition_error)?;
            let extension_fields: Vec<String> = source
                .get_fields()
                .map_err(thrown_definition_error)?
                .keys()
                .filter(|field_name| !target_type_fields.contains_key(field_name.as_str()))
                .cloned()
                .collect();
            if !extension_fields.is_empty() {
                interface_extensions_map.insert(type_name.clone(), extension_fields);
            }
        }
        let extended = interface_extensions_map.contains_key(type_name.as_str());
        if extended || !target_type.is_some_and(is_abstract) {
            let implementations = source_schema
                .get_possible_types(r#type)
                .map_err(thrown_definition_error)?;
            let names = implementations
                .iter()
                .filter(|implementation| target_schema.get_type(implementation.name()).is_some())
                .map(|implementation| String::from(implementation.name()))
                .collect();
            possible_types_map.insert(type_name.clone(), names);
        }
    }
    let mut reverse_possible_types_map: PossibleTypesMap = IndexMap::default();
    for (type_name, to_type_names) in &possible_types_map {
        for to_type_name in to_type_names {
            reverse_possible_types_map
                .entry(to_type_name.clone())
                .or_default()
                .push(type_name.clone());
        }
    }
    Ok(SchemaMetaData {
        possible_types_map,
        reverse_possible_types_map,
        interface_extensions_map,
    })
}

/// Returns the fragments a fragment on an abstract type expands to and the replacements of its spreads.
///
/// Each expanded fragment is named `_<Type>_Fragment<n>` with a counter that
/// skips a name the document holds, and shares the selection set of the
/// fragment it expands.
fn get_expanded_fragments(
    fragments: &[PDefinition],
    fragment_names: &mut IndexSet<String>,
    possible_types_map: &PossibleTypesMap,
) -> (Vec<PDefinition>, IndexMap<String, Vec<FragmentReplacement>>) {
    let mut fragment_counter = 0_usize;
    let mut expanded_fragments = Vec::new();
    let mut fragment_replacements: IndexMap<String, Vec<FragmentReplacement>> = IndexMap::default();
    for fragment in fragments {
        let PDefinition::Fragment(node, selection_set) = fragment else {
            continue;
        };
        let Some(possible_types) = possible_types_map.get(node.type_condition.name.value()) else {
            continue;
        };
        let fragment_name = String::from(node.name.value());
        let mut replacements = Vec::new();
        for possible_type_name in possible_types {
            let name = loop {
                let candidate = format!("_{possible_type_name}_Fragment{fragment_counter}");
                fragment_counter += 1;
                if !fragment_names.contains(&candidate) {
                    break candidate;
                }
            };
            fragment_names.insert(name.clone());
            expanded_fragments.push(PDefinition::Fragment(
                Rc::new(FragmentDefinitionNode {
                    description: None,
                    loc: None,
                    name: NameNode::new(None, name.as_str()),
                    variable_definitions: None,
                    type_condition: named_type_node(possible_type_name),
                    directives: None,
                    selection_set: empty_selection_set(),
                }),
                Rc::clone(selection_set),
            ));
            replacements.push(FragmentReplacement {
                fragment_name: name,
                type_name: possible_type_name.clone(),
            });
        }
        fragment_replacements.insert(fragment_name, replacements);
    }
    (expanded_fragments, fragment_replacements)
}

/// Returns the document with the selection of each root field of an abstract type wrapped per possible type.
///
/// A leaf return type leaves the document as it is. The walk visits the
/// operations and the fragments on a root type, their selection sets and
/// inline fragments, and the fields of those sets without descending into
/// them, which is the key map of `:1653-1659`.
///
/// # Errors
///
/// Returns what the type information and the member thunk of a union throw.
fn wrap_concrete_types(
    return_type: &Type,
    target_schema: &Schema,
    document: &PDocument,
) -> Result<PDocument, Value> {
    let named_type = get_named_type(return_type);
    if matches!(named_type, NamedType::Scalar(_) | NamedType::Enum(_)) {
        return Ok(document.clone());
    }
    let mut possible_types: Vec<String> = if is_abstract(&named_type) {
        target_schema
            .get_possible_types(&named_type)
            .map_err(thrown_definition_error)?
            .iter()
            .map(|possible_type| String::from(possible_type.name()))
            .collect()
    } else {
        vec![String::from(named_type.name())]
    };
    if possible_types.is_empty() {
        possible_types = vec![String::from(named_type.name())];
    }
    let mut walk = WrapWalk {
        type_info: TypeInfo::new(target_schema, None, None),
        root_type_names: get_root_type_names(target_schema),
        named_type_name: String::from(named_type.name()),
        possible_types,
        empty_set: empty_selection_set(),
    };
    let mut definitions = Vec::with_capacity(document.definitions.len());
    for definition in &document.definitions {
        definitions.push(walk.definition(definition)?);
    }
    Ok(PDocument {
        loc: document.loc.clone(),
        definitions,
    })
}

/// The walk of [`wrap_concrete_types`].
struct WrapWalk<'s> {
    /// The type information of the target schema.
    type_info: TypeInfo<'s>,
    /// The names of the root types of the target.
    root_type_names: Vec<String>,
    /// The name of the named return type.
    named_type_name: String,
    /// The names of the possible types of the return type.
    possible_types: Vec<String>,
    /// The empty selection set the type information enters for a set.
    empty_set: SelectionSetNode,
}

impl WrapWalk<'_> {
    /// Walks a definition, skipping a fragment on a type that roots no operation.
    fn definition(&mut self, definition: &PDefinition) -> Result<PDefinition, Value> {
        match definition {
            PDefinition::Operation(node, selection_set) => {
                enter(&mut self.type_info, NodeRef::from(node.as_ref()))?;
                let walked = self.selection_set(selection_set)?;
                self.type_info.leave(NodeRef::from(node.as_ref()));
                Ok(PDefinition::Operation(Rc::clone(node), walked))
            }
            PDefinition::Fragment(node, selection_set) => {
                enter(&mut self.type_info, NodeRef::from(node.as_ref()))?;
                let on_root = self
                    .root_type_names
                    .iter()
                    .any(|name| name == node.type_condition.name.value());
                if !on_root {
                    self.type_info.leave(NodeRef::from(node.as_ref()));
                    return Ok(definition.clone());
                }
                let walked = self.selection_set(selection_set)?;
                self.type_info.leave(NodeRef::from(node.as_ref()));
                Ok(PDefinition::Fragment(Rc::clone(node), walked))
            }
            PDefinition::Other(_) => Ok(definition.clone()),
        }
    }

    /// Walks a selection set, answering the same set when nothing below it changed.
    fn selection_set(&mut self, set: &Rc<PSet>) -> Result<Rc<PSet>, Value> {
        enter(&mut self.type_info, NodeRef::from(&self.empty_set))?;
        let mut changed = false;
        let mut selections = Vec::with_capacity(set.selections.len());
        for selection in &set.selections {
            let walked = match selection {
                PSelection::Field(field) => self.field(field)?,
                PSelection::InlineFragment(fragment) => {
                    enter(&mut self.type_info, NodeRef::from(&fragment.node))?;
                    let walked = self.selection_set(&fragment.selection_set)?;
                    self.type_info.leave(NodeRef::from(&fragment.node));
                    if Rc::ptr_eq(&walked, &fragment.selection_set) {
                        selection.clone()
                    } else {
                        PSelection::InlineFragment(Rc::new(PInlineFragment {
                            node: fragment.node.clone(),
                            selection_set: walked,
                        }))
                    }
                }
                PSelection::FragmentSpread(_) => selection.clone(),
            };
            changed |= !walked.same(selection);
            selections.push(walked);
        }
        self.type_info.leave(NodeRef::from(&self.empty_set));
        if !changed {
            return Ok(Rc::clone(set));
        }
        Ok(Rc::new(PSet {
            loc: set.loc.clone(),
            selections,
        }))
    }

    /// The hook of a field, which wraps the selection of a field of another abstract type.
    fn field(&mut self, field: &Rc<PField>) -> Result<PSelection, Value> {
        enter(&mut self.type_info, NodeRef::from(&field.node))?;
        let field_named_type = self.type_info.get_type().map(get_named_type);
        let wrapped = match field_named_type {
            Some(field_named_type)
                if is_abstract(&field_named_type)
                    && field_named_type.name() != self.named_type_name =>
            {
                let selections = self
                    .possible_types
                    .iter()
                    .map(|possible_type| {
                        PSelection::InlineFragment(Rc::new(PInlineFragment {
                            node: inline_fragment_shell(possible_type),
                            selection_set: field.selection_set.clone().unwrap_or_else(empty_p_set),
                        }))
                    })
                    .collect();
                Some(PSelection::Field(Rc::new(PField {
                    node: field.node.clone(),
                    selection_set: Some(Rc::new(PSet {
                        loc: None,
                        selections,
                    })),
                })))
            }
            _ => None,
        };
        self.type_info.leave(NodeRef::from(&field.node));
        Ok(wrapped.unwrap_or_else(|| PSelection::Field(Rc::clone(field))))
    }
}

/// The walk of the expanded document with the hook of a selection set.
struct PrepareWalk<'p, 's> {
    /// The schema the document is prepared against.
    transformed_schema: &'s Schema,
    /// The schema of the field that delegates.
    info_schema: &'p Schema,
    /// The type information of the transformed schema.
    type_info: TypeInfo<'s>,
    /// The meta data of the two schemas.
    meta_data: &'p SchemaMetaData,
    /// The extra possible types of the two schemas by type name, kept with the meta data.
    extra_possible_types: &'p RefCell<IndexMap<String, Rc<[String]>>>,
    /// The replacements of the spreads of an expanded fragment.
    fragment_replacements: &'p IndexMap<String, Vec<FragmentReplacement>>,
    /// The inline fragments whose extra types and fields were added, `visitedSelections`, by address.
    visited_selections: IndexMap<usize, Rc<PInlineFragment>, AddressState>,
    /// The empty selection set the type information enters for a set.
    empty_set: SelectionSetNode,
}

impl PrepareWalk<'_, '_> {
    /// Walks a definition of the expanded document.
    fn definition(&mut self, definition: &PDefinition) -> Result<PDefinition, Value> {
        match definition {
            PDefinition::Operation(node, selection_set) => {
                enter(&mut self.type_info, NodeRef::from(node.as_ref()))?;
                let walked = self.selection_set(selection_set)?;
                self.type_info.leave(NodeRef::from(node.as_ref()));
                Ok(PDefinition::Operation(Rc::clone(node), walked))
            }
            PDefinition::Fragment(node, selection_set) => {
                enter(&mut self.type_info, NodeRef::from(node.as_ref()))?;
                let walked = self.selection_set(selection_set)?;
                self.type_info.leave(NodeRef::from(node.as_ref()));
                Ok(PDefinition::Fragment(Rc::clone(node), walked))
            }
            PDefinition::Other(_) => Ok(definition.clone()),
        }
    }

    /// Walks a selection set: its hook, then the selections of the set the hook answers.
    fn selection_set(&mut self, set: &Rc<PSet>) -> Result<Rc<PSet>, Value> {
        enter(&mut self.type_info, NodeRef::from(&self.empty_set))?;
        let visited = self
            .visit_selection_set(set)?
            .unwrap_or_else(|| set.as_ref().clone());
        let mut selections = Vec::with_capacity(visited.selections.len());
        for selection in &visited.selections {
            selections.push(match selection {
                PSelection::Field(field) => {
                    enter(&mut self.type_info, NodeRef::from(&field.node))?;
                    let selection_set = match &field.selection_set {
                        Some(set) => Some(self.selection_set(set)?),
                        None => None,
                    };
                    self.type_info.leave(NodeRef::from(&field.node));
                    PSelection::Field(Rc::new(PField {
                        node: field.node.clone(),
                        selection_set,
                    }))
                }
                PSelection::InlineFragment(fragment) => {
                    enter(&mut self.type_info, NodeRef::from(&fragment.node))?;
                    let selection_set = self.selection_set(&fragment.selection_set)?;
                    self.type_info.leave(NodeRef::from(&fragment.node));
                    PSelection::InlineFragment(Rc::new(PInlineFragment {
                        node: fragment.node.clone(),
                        selection_set,
                    }))
                }
                PSelection::FragmentSpread(_) => selection.clone(),
            });
        }
        self.type_info.leave(NodeRef::from(&self.empty_set));
        Ok(Rc::new(PSet {
            loc: visited.loc,
            selections,
        }))
    }

    /// The hook of a selection set, `None` when the walk knows no parent type.
    fn visit_selection_set(&mut self, node: &PSet) -> Result<Option<PSet>, Value> {
        let Some(parent_type) = self.type_info.get_parent_type().cloned() else {
            return Ok(None);
        };
        let parent_type_name = String::from(parent_type.name());
        let mut new_selections = Selections::default();
        let interface_extensions = self
            .meta_data
            .interface_extensions_map
            .get(parent_type_name.as_str());
        let mut interface_extension_fields = Vec::new();
        for selection in &node.selections {
            match selection {
                PSelection::InlineFragment(fragment) => {
                    self.inline_fragment(fragment, selection, &parent_type, &mut new_selections)?;
                }
                PSelection::FragmentSpread(spread) => {
                    self.fragment_spread(spread, selection, &parent_type, &mut new_selections)?;
                }
                PSelection::Field(field) => {
                    let field_name = field.node.name.value();
                    if interface_extensions
                        .is_some_and(|fields| fields.iter().any(|name| name == field_name))
                    {
                        interface_extension_fields.push(selection.clone());
                    } else {
                        new_selections.add(selection.clone());
                    }
                }
            }
        }
        if self
            .meta_data
            .reverse_possible_types_map
            .contains_key(parent_type_name.as_str())
        {
            new_selections.add(PSelection::Field(Rc::new(PField {
                node: field_shell("__typename"),
                selection_set: None,
            })));
        }
        if !interface_extension_fields.is_empty()
            && let Some(possible_types) = self
                .meta_data
                .possible_types_map
                .get(parent_type_name.as_str())
        {
            for possible_type in possible_types {
                new_selections.add(generate_inline_fragment(
                    possible_type,
                    Rc::new(PSet {
                        loc: None,
                        selections: interface_extension_fields.clone(),
                    }),
                ));
            }
        }
        Ok(Some(PSet {
            loc: node.loc.clone(),
            selections: new_selections.into_vec(),
        }))
    }

    /// The inline fragment branch of [`Self::visit_selection_set`], `index.cjs:1285-1357`.
    fn inline_fragment(
        &mut self,
        fragment: &Rc<PInlineFragment>,
        selection: &PSelection,
        parent_type: &NamedType,
        new_selections: &mut Selections,
    ) -> Result<(), Value> {
        let Some(type_condition) = &fragment.node.type_condition else {
            new_selections.add(selection.clone());
            return Ok(());
        };
        let type_name = type_condition.name.value();
        let address = Rc::as_ptr(fragment).addr();
        if !self.visited_selections.contains_key(&address) {
            self.visited_selections.insert(address, Rc::clone(fragment));
            let extra_possible_types = self.get_extra_possible_types(type_name)?;
            for extra_possible_type_name in extra_possible_types.iter() {
                let mut node = fragment.node.clone();
                node.type_condition = Some(named_type_node(extra_possible_type_name));
                new_selections.add(PSelection::InlineFragment(Rc::new(PInlineFragment {
                    node,
                    selection_set: Rc::clone(&fragment.selection_set),
                })));
            }
            match self.transformed_schema.get_type(type_name) {
                Some(NamedType::Object(object)) => {
                    add_fields_outside(object.get_fields(), fragment, new_selections)?;
                }
                Some(NamedType::Interface(interface)) => {
                    add_fields_outside(interface.get_fields(), fragment, new_selections)?;
                }
                Some(_) => {}
                None => {
                    for sub_selection in &fragment.selection_set.selections {
                        new_selections.add(sub_selection.clone());
                    }
                }
            }
        }
        let Some(possible_types) = self.meta_data.possible_types_map.get(type_name) else {
            new_selections.add(selection.clone());
            return Ok(());
        };
        for possible_type_name in possible_types {
            let maybe_possible_type = self.transformed_schema.get_type(possible_type_name);
            if maybe_possible_type.is_some()
                && implements_abstract_type(
                    self.transformed_schema,
                    Some(parent_type),
                    maybe_possible_type,
                )?
            {
                new_selections.add(generate_inline_fragment(
                    possible_type_name,
                    Rc::clone(&fragment.selection_set),
                ));
            }
        }
        if possible_types.is_empty() {
            new_selections.add(selection.clone());
        }
        Ok(())
    }

    /// The fragment spread branch of [`Self::visit_selection_set`], `index.cjs:1358-1376`.
    ///
    /// The reference tests the parent type against the parent type itself,
    /// so a replacement whose type the target has is always taken.
    fn fragment_spread(
        &mut self,
        spread: &FragmentSpreadNode,
        selection: &PSelection,
        parent_type: &NamedType,
        new_selections: &mut Selections,
    ) -> Result<(), Value> {
        let fragment_name = spread.name.value();
        let Some(replacements) = self.fragment_replacements.get(fragment_name) else {
            new_selections.add(selection.clone());
            return Ok(());
        };
        let maybe_type = self.type_info.get_parent_type().cloned();
        for replacement in replacements {
            let maybe_replacement_type = self.transformed_schema.get_type(&replacement.type_name);
            if maybe_replacement_type.is_some()
                && implements_abstract_type(
                    self.transformed_schema,
                    Some(parent_type),
                    maybe_type.as_ref(),
                )?
            {
                new_selections.add(PSelection::FragmentSpread(Rc::new(FragmentSpreadNode {
                    loc: None,
                    name: NameNode::new(None, replacement.fragment_name.as_str()),
                    directives: None,
                })));
            }
        }
        Ok(())
    }

    /// Returns the possible types of an abstract type the source has and the target's type lacks.
    ///
    /// The answer of a type name is kept with the meta data of the pair, the
    /// `Map` the memo of the reference keeps per pair, so each type name is
    /// answered once per pair of schemas, and a type name whose possible types
    /// throw keeps nothing, as the reference sets its `Map` after the possible
    /// types are read.
    ///
    /// # Errors
    ///
    /// Returns what the member thunk of a union of either schema throws.
    fn get_extra_possible_types(&self, type_name: &str) -> Result<Rc<[String]>, Value> {
        if let Some(known) = self.extra_possible_types.borrow().get(type_name) {
            return Ok(Rc::clone(known));
        }
        let extra_types_for_subschema =
            extra_possible_types(self.info_schema, self.transformed_schema, type_name)?;
        self.extra_possible_types.borrow_mut().insert(
            String::from(type_name),
            Rc::clone(&extra_types_for_subschema),
        );
        Ok(extra_types_for_subschema)
    }
}

/// Returns the possible types of `type_name` in `info_schema` that `transformed_schema` has and not as its own.
///
/// The body of `getExtraPossibleTypes2` at `index.cjs:1246-1264`, each name
/// once in the order of the source. The possible types of the target are a
/// set of their names, so each possible type of the source takes one lookup
/// where the `some` of the reference scans them.
///
/// # Errors
///
/// Returns what the member thunk of a union of either schema throws.
fn extra_possible_types(
    info_schema: &Schema,
    transformed_schema: &Schema,
    type_name: &str,
) -> Result<Rc<[String]>, Value> {
    let gateway_type = info_schema.get_type(type_name);
    let subschema_type = transformed_schema.get_type(type_name);
    let (Some(gateway_type), Some(subschema_type)) = (gateway_type, subschema_type) else {
        return Ok(Rc::from([]));
    };
    if !is_abstract(gateway_type) || !is_abstract(subschema_type) {
        return Ok(Rc::from([]));
    }
    let possible_types = info_schema
        .get_possible_types(gateway_type)
        .map_err(thrown_definition_error)?;
    let possible_types_in_subschema: IndexSet<&str> = transformed_schema
        .get_possible_types(subschema_type)
        .map_err(thrown_definition_error)?
        .iter()
        .map(NamedType::name)
        .collect();
    let mut extra_types_for_subschema: IndexSet<&str> = IndexSet::default();
    for possible_type in possible_types {
        let name = possible_type.name();
        if transformed_schema.get_type(name).is_none() || possible_types_in_subschema.contains(name)
        {
            continue;
        }
        extra_types_for_subschema.insert(name);
    }
    Ok(extra_types_for_subschema
        .into_iter()
        .map(String::from)
        .collect())
}

/// The selections a hook adds, a set by identity in which a field another field covers is not added.
///
/// The `Set` of `visitSelectionSet` with `addSelectionNodeToSelections` at
/// `index.cjs:1494-1499`. Up to [`SMALL_SELECTIONS`] selections a selection
/// is tested by a scan, and beyond that through the index, which keeps the
/// address of every selection added and the first field of each name and
/// alias, the field the scan of the reference stops at. A field that comes
/// again with a selection set is tested against the selection set of that
/// first field through `sets`, by the same rule.
#[derive(Default)]
struct Selections {
    /// The selections in the order they were added.
    list: Vec<PSelection>,
    /// The index of `list`, made once it holds more than [`SMALL_SELECTIONS`] selections.
    index: Option<SelectionIndex>,
    /// The first fields of the selection sets a repeated field is tested against.
    sets: SetIndexes,
}

/// The index of the selections of a [`Selections`].
#[derive(Default)]
struct SelectionIndex {
    /// The address of every selection added.
    added: IndexSet<usize, AddressState>,
    /// The first field added for each name and alias.
    fields: IndexMap<FieldKey, Rc<PField>>,
}

impl SelectionIndex {
    /// Records `selection`, which was added last.
    fn record(&mut self, selection: &PSelection) {
        self.added.insert(selection.address());
        if let PSelection::Field(field) = selection {
            self.fields
                .entry(FieldKey(Rc::clone(field)))
                .or_insert_with(|| Rc::clone(field));
        }
    }
}

impl Selections {
    /// Adds `selection` unless it is there or it is a field a field there satisfies.
    fn add(&mut self, selection: PSelection) {
        let present = match &self.index {
            None => {
                if let PSelection::Field(field) = &selection
                    && is_field_node_satisfied_by_selections(field, &self.list, &mut self.sets)
                {
                    return;
                }
                self.list.iter().any(|known| known.same(&selection))
            }
            Some(index) => {
                if let PSelection::Field(field) = &selection
                    && index
                        .fields
                        .get(&FieldKey(Rc::clone(field)))
                        .is_some_and(|existing| {
                            is_field_satisfied_by(field, existing, &mut self.sets)
                        })
                {
                    return;
                }
                index.added.contains(&selection.address())
            }
        };
        if present {
            return;
        }
        self.list.push(selection);
        match &mut self.index {
            Some(index) => {
                if let Some(added) = self.list.last() {
                    index.record(added);
                }
            }
            None if self.list.len() > SMALL_SELECTIONS => {
                let mut index = SelectionIndex::default();
                for added in &self.list {
                    index.record(added);
                }
                self.index = Some(index);
            }
            None => {}
        }
    }

    /// Returns the selections in the order they were added, `Array.from(newSelections)`.
    fn into_vec(self) -> Vec<PSelection> {
        self.list
    }
}

/// The first field of each name and alias of the selection sets the tests of a hook read, by the set.
///
/// A set of more than [`SMALL_SELECTIONS`] selections is indexed at the first
/// test that reads it and kept for the hook, so `K` fields that come again
/// against one set of `D` fields cost one index of the `D` fields and `K`
/// lookups, where the `find` of the reference scans up to `D` fields for
/// each of them. A smaller set is scanned.
#[derive(Default)]
struct SetIndexes(IndexMap<SetKey, IndexMap<FieldKey, Rc<PField>>, AddressState>);

impl SetIndexes {
    /// Returns the first field of `set` with the name and the alias of `field`, the `existing.find` of `index.cjs:1470-1472`.
    fn first_field(&mut self, set: &Rc<PSet>, field: &Rc<PField>) -> Option<Rc<PField>> {
        if set.selections.len() <= SMALL_SELECTIONS {
            return first_field_by_scan(&set.selections, field).cloned();
        }
        self.0
            .entry(SetKey(Rc::clone(set)))
            .or_insert_with(|| {
                let mut fields = IndexMap::default();
                for selection in &set.selections {
                    if let PSelection::Field(known) = selection {
                        fields
                            .entry(FieldKey(Rc::clone(known)))
                            .or_insert_with(|| Rc::clone(known));
                    }
                }
                fields
            })
            .get(&FieldKey(Rc::clone(field)))
            .cloned()
    }
}

/// A selection set as a key by its identity, the address of its node, which the key holds.
struct SetKey(Rc<PSet>);

impl PartialEq for SetKey {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for SetKey {}

impl Hash for SetKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Rc::as_ptr(&self.0).addr().hash(state);
    }
}

/// A field as the key of its name and its alias, the pair the scans of the preparation compare.
struct FieldKey(Rc<PField>);

impl PartialEq for FieldKey {
    fn eq(&self, other: &Self) -> bool {
        same_name_and_alias(&self.0, &other.0)
    }
}

impl Eq for FieldKey {}

impl Hash for FieldKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.node.name.value().hash(state);
        self.0.node.alias.as_ref().map(NameNode::value).hash(state);
    }
}

/// Returns the first field among `selections` with the name and the alias of `field`.
fn first_field_by_scan<'s>(selections: &'s [PSelection], field: &PField) -> Option<&'s Rc<PField>> {
    selections.iter().find_map(|selection| match selection {
        PSelection::Field(known) if same_name_and_alias(known, field) => Some(known),
        _ => None,
    })
}

/// Returns whether a selection among `existing` covers `field_node`.
///
/// The first field of the same name and alias decides, and it covers the field
/// when the field has no selection set or when its own covers that set.
fn is_field_node_satisfied_by_selections(
    field_node: &PField,
    existing: &[PSelection],
    sets: &mut SetIndexes,
) -> bool {
    first_field_by_scan(existing, field_node)
        .is_some_and(|existing_field| is_field_satisfied_by(field_node, existing_field, sets))
}

/// Returns whether `existing_field`, of the name and the alias of `field_node`, covers it.
fn is_field_satisfied_by(
    field_node: &PField,
    existing_field: &PField,
    sets: &mut SetIndexes,
) -> bool {
    match (&field_node.selection_set, &existing_field.selection_set) {
        (Some(_), None) => false,
        (Some(incoming), Some(existing_set)) => {
            is_selection_set_satisfied(incoming, existing_set, sets)
        }
        (None, _) => true,
    }
}

/// Returns whether `existing` covers every selection of `incoming`, which holds fields alone.
///
/// The first field of `existing` with the name and the alias of an incoming
/// field decides, which `sets` answers in one lookup for a set of more than
/// [`SMALL_SELECTIONS`] selections.
fn is_selection_set_satisfied(incoming: &PSet, existing: &Rc<PSet>, sets: &mut SetIndexes) -> bool {
    for incoming_selection in &incoming.selections {
        let PSelection::Field(incoming_field) = incoming_selection else {
            return false;
        };
        let Some(existing_field) = sets.first_field(existing, incoming_field) else {
            return false;
        };
        if !is_field_satisfied_by(incoming_field, &existing_field, sets) {
            return false;
        }
    }
    true
}

/// Returns whether two fields share their name and their alias, an absent alias matching only another.
fn same_name_and_alias(a: &PField, b: &PField) -> bool {
    a.node.name.value() == b.node.name.value()
        && a.node.alias.as_ref().map(NameNode::value) == b.node.alias.as_ref().map(NameNode::value)
}

/// Returns an inline fragment on `type_name` that holds `selection_set`.
fn generate_inline_fragment(type_name: &str, selection_set: Rc<PSet>) -> PSelection {
    PSelection::InlineFragment(Rc::new(PInlineFragment {
        node: inline_fragment_shell(type_name),
        selection_set,
    }))
}

/// Adds every field of an inline fragment that the fields of its type in the target lack.
///
/// The lift of `index.cjs:1316-1330`, which tests each name against
/// `typeInSubschema.getFields()`.
fn add_fields_outside(
    fields: Result<&ObjMap<Field>, DefinitionError>,
    fragment: &PInlineFragment,
    new_selections: &mut Selections,
) -> Result<(), Value> {
    let fields = fields.map_err(thrown_definition_error)?;
    for sub_selection in &fragment.selection_set.selections {
        if let PSelection::Field(field) = sub_selection
            && !fields.contains_key(field.node.name.value())
        {
            new_selections.add(sub_selection.clone());
        }
    }
    Ok(())
}

/// Enters a node in the type information.
fn enter(type_info: &mut TypeInfo<'_>, node: NodeRef<'_>) -> Result<(), Value> {
    type_info.enter(node).map_err(thrown_definition_error)
}

/// Returns the named type node of `name`.
fn named_type_node(name: &str) -> NamedTypeNode {
    NamedTypeNode {
        loc: None,
        name: NameNode::new(None, name),
    }
}

/// Returns an empty selection set.
fn empty_selection_set() -> SelectionSetNode {
    SelectionSetNode {
        loc: None,
        selections: SharedSlice::default(),
    }
}

/// Returns an empty selection set of the walk's tree.
fn empty_p_set() -> Rc<PSet> {
    Rc::new(PSet {
        loc: None,
        selections: Vec::new(),
    })
}

/// Returns the shell of a field named `name` with no selection set.
fn field_shell(name: &str) -> FieldNode {
    FieldNode {
        loc: None,
        alias: None,
        name: NameNode::new(None, name),
        arguments: None,
        directives: None,
        selection_set: None,
    }
}

/// Returns the shell of an inline fragment on `type_name`, its selection set held apart.
fn inline_fragment_shell(type_name: &str) -> InlineFragmentNode {
    InlineFragmentNode {
        loc: None,
        type_condition: Some(named_type_node(type_name)),
        directives: None,
        selection_set: empty_selection_set(),
    }
}

/// A document of the walk's tree.
#[derive(Clone)]
struct PDocument {
    /// The location of the document.
    loc: Option<Location>,
    /// The definitions.
    definitions: Vec<PDefinition>,
}

/// A definition of the walk's tree, its node with the selection set held apart.
#[derive(Clone)]
enum PDefinition {
    /// An operation and its selection set.
    Operation(Rc<OperationDefinitionNode>, Rc<PSet>),
    /// A fragment definition and its selection set.
    Fragment(Rc<FragmentDefinitionNode>, Rc<PSet>),
    /// Any other definition, which the walks leave as it is.
    Other(Rc<DefinitionNode>),
}

/// A selection set of the walk's tree.
#[derive(Clone)]
struct PSet {
    /// The location of the set.
    loc: Option<Location>,
    /// The selections.
    selections: Vec<PSelection>,
}

/// A selection of the walk's tree, whose identity is the `Rc` behind it.
#[derive(Clone)]
enum PSelection {
    /// A field.
    Field(Rc<PField>),
    /// A fragment spread.
    FragmentSpread(Rc<FragmentSpreadNode>),
    /// An inline fragment.
    InlineFragment(Rc<PInlineFragment>),
}

impl PSelection {
    /// Returns the address of the node, its identity.
    fn address(&self) -> usize {
        match self {
            Self::Field(field) => Rc::as_ptr(field).addr(),
            Self::FragmentSpread(spread) => Rc::as_ptr(spread).addr(),
            Self::InlineFragment(fragment) => Rc::as_ptr(fragment).addr(),
        }
    }

    /// Returns whether both are the same node, the `===` of the reference.
    fn same(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Field(a), Self::Field(b)) => Rc::ptr_eq(a, b),
            (Self::FragmentSpread(a), Self::FragmentSpread(b)) => Rc::ptr_eq(a, b),
            (Self::InlineFragment(a), Self::InlineFragment(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
}

/// A field of the walk's tree, its node with the selection set held apart.
struct PField {
    /// The field with no selection set.
    node: FieldNode,
    /// The selection set.
    selection_set: Option<Rc<PSet>>,
}

/// An inline fragment of the walk's tree, its node with the selection set held apart.
struct PInlineFragment {
    /// The inline fragment with an empty selection set.
    node: InlineFragmentNode,
    /// The selection set.
    selection_set: Rc<PSet>,
}

impl PDocument {
    /// Returns the walk's tree of a document followed by `fragments`, every node a node of its own.
    ///
    /// Each node is copied without its selection set, which the tree holds
    /// apart, so a selection set is copied once whatever its depth.
    fn from_ast(document: &DocumentNode, fragments: &[Rc<FragmentDefinitionNode>]) -> Self {
        let definitions = document
            .definitions
            .iter()
            .map(|definition| match definition {
                DefinitionNode::OperationDefinition(operation) => PDefinition::Operation(
                    Rc::new(OperationDefinitionNode {
                        description: operation.description.clone(),
                        loc: operation.loc.clone(),
                        operation: operation.operation,
                        name: operation.name.clone(),
                        variable_definitions: operation.variable_definitions.clone(),
                        directives: operation.directives.clone(),
                        selection_set: empty_selection_set(),
                    }),
                    PSet::from_ast(&operation.selection_set),
                ),
                DefinitionNode::FragmentDefinition(fragment) => PDefinition::fragment(fragment),
                other => PDefinition::Other(Rc::new(other.clone())),
            })
            .chain(
                fragments
                    .iter()
                    .map(|fragment| PDefinition::fragment(fragment)),
            )
            .collect();
        Self {
            loc: document.loc.clone(),
            definitions,
        }
    }

    /// Returns the document the tree stands for, a shared node copied at each place.
    fn to_ast(&self) -> DocumentNode {
        DocumentNode {
            loc: self.loc.clone(),
            definitions: self
                .definitions
                .iter()
                .map(|definition| match definition {
                    PDefinition::Operation(node, set) => {
                        DefinitionNode::OperationDefinition(OperationDefinitionNode {
                            selection_set: set.to_ast(),
                            ..node.as_ref().clone()
                        })
                    }
                    PDefinition::Fragment(node, set) => {
                        DefinitionNode::FragmentDefinition(FragmentDefinitionNode {
                            selection_set: set.to_ast(),
                            ..node.as_ref().clone()
                        })
                    }
                    PDefinition::Other(node) => node.as_ref().clone(),
                })
                .collect(),
            token_count: None,
        }
    }
}

impl PDefinition {
    /// Returns the definition of the walk's tree of a fragment definition.
    fn fragment(fragment: &FragmentDefinitionNode) -> Self {
        Self::Fragment(
            Rc::new(FragmentDefinitionNode {
                description: fragment.description.clone(),
                loc: fragment.loc.clone(),
                name: fragment.name.clone(),
                variable_definitions: fragment.variable_definitions.clone(),
                type_condition: fragment.type_condition.clone(),
                directives: fragment.directives.clone(),
                selection_set: empty_selection_set(),
            }),
            PSet::from_ast(&fragment.selection_set),
        )
    }
}

impl PSet {
    /// Returns the walk's tree of a selection set.
    fn from_ast(set: &SelectionSetNode) -> Rc<Self> {
        let selections = set
            .selections
            .iter()
            .map(|selection| match selection {
                SelectionNode::Field(field) => PSelection::Field(Rc::new(PField {
                    node: FieldNode {
                        loc: field.loc.clone(),
                        alias: field.alias.clone(),
                        name: field.name.clone(),
                        arguments: field.arguments.clone(),
                        directives: field.directives.clone(),
                        selection_set: None,
                    },
                    selection_set: field.selection_set.as_ref().map(Self::from_ast),
                })),
                SelectionNode::FragmentSpread(spread) => {
                    PSelection::FragmentSpread(Rc::new(spread.clone()))
                }
                SelectionNode::InlineFragment(fragment) => {
                    PSelection::InlineFragment(Rc::new(PInlineFragment {
                        node: InlineFragmentNode {
                            loc: fragment.loc.clone(),
                            type_condition: fragment.type_condition.clone(),
                            directives: fragment.directives.clone(),
                            selection_set: empty_selection_set(),
                        },
                        selection_set: Self::from_ast(&fragment.selection_set),
                    }))
                }
            })
            .collect();
        Rc::new(Self {
            loc: set.loc.clone(),
            selections,
        })
    }

    /// Returns the selection set the tree stands for.
    fn to_ast(&self) -> SelectionSetNode {
        SelectionSetNode {
            loc: self.loc.clone(),
            selections: self
                .selections
                .iter()
                .map(|selection| match selection {
                    PSelection::Field(field) => SelectionNode::Field(FieldNode {
                        selection_set: field.selection_set.as_ref().map(|set| set.to_ast()),
                        ..field.node.clone()
                    }),
                    PSelection::FragmentSpread(spread) => {
                        SelectionNode::FragmentSpread(spread.as_ref().clone())
                    }
                    PSelection::InlineFragment(fragment) => {
                        SelectionNode::InlineFragment(InlineFragmentNode {
                            selection_set: fragment.selection_set.to_ast(),
                            ..fragment.node.clone()
                        })
                    }
                })
                .collect(),
        }
    }
}
