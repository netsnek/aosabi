// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-core resolvers get_selected_fields: the selection walk below one
 * field and the discriminator fields it injects for an abstract type.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/core/define-pylon.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 */

//! The selected fields of a resolver call.
//!
//! The projection of [`wrap_resolver`](crate::resolvers::wrap_resolver())
//! copies the members a document selects and drops every other member, so it
//! needs the selection below a field as a list of names. For an abstract
//! parent type the list also names `__typename` and one field per possible
//! type that no other possible type declares, because the type resolution of
//! the executor reads the projected object and not the one the resolver
//! returned.
//!
//! Both caches of the reference are here. `selectionSetCache` is the
//! per request map of [`RequestState`], keyed by the address of the node
//! list and the address of the parent type, as the reference keys a
//! `WeakMap` and a `Map` by the identity of each. `uniqueFieldsCache` is the
//! map [`CoreState`] keeps for as long as the service lives. A selected field
//! keeps the selection below it once it is read, so the members of every
//! object a list of one selection answers find it with no lookup in the map.

use core::cell::{
    Cell,
    OnceCell,
    RefCell, //
};
use std::{
    rc::Rc,
    sync::Arc, //
};

use aosabi_execute::{
    execute::{
        FieldNodes,
        ResolveInfo, //
    },
    value::{
        MemberKind,
        Shape, //
    }, //
};
use aosabi_json::{
    map::IndexMap,
    value::KeyHasherBuilder, //
};
use aosabi_lang::ast::{
    FieldNode,
    SelectionNode, //
};
use aosabi_type::{
    Field,
    NamedType,
    ObjMap,
    get_named_type,
    is_non_null_type, //
};

use crate::{
    context::RequestState,
    service::CoreState, //
};

/// The name of the meta field that carries the concrete type of a value.
const TYPENAME: &str = "__typename";

/// One field selected below a resolver, under its name and never its alias.
pub struct SelectedField {
    /// The name of the field, which is the member a projection reads.
    pub name: String,
    /// Every node that selects the field, in document order. The list is
    /// empty for an injected discriminator.
    pub field_nodes: FieldNodes,
    /// The named type of the field, when the parent type declares the field.
    pub return_type: Option<NamedType>,
    /// Whether a node of [`SelectedField::field_nodes`] carries an alias,
    /// the test of source line 294.
    pub(crate) aliased: bool,
    /// The selection below [`SelectedField::field_nodes`] against
    /// [`SelectedField::return_type`], which the wrapping of a member reads
    /// once per request for every object of the selection.
    pub(crate) below: OnceCell<Selection>,
    /// The position of the earlier field of the selection under the same
    /// name, whose property the member of this field replaces.
    ///
    /// The walk names a field twice in one case alone, the `__typename` it
    /// injects for an abstract type behind a `__typename` the document
    /// selects under an alias alone, as the reference pushes it.
    pub(crate) repeat_of: Option<usize>,
}

impl SelectedField {
    /// Returns a selected field of `name` over `field_nodes`.
    fn new(name: String, field_nodes: FieldNodes, return_type: Option<NamedType>) -> Self {
        let aliased = field_nodes.iter().any(|node| node.alias.is_some());
        Self {
            name,
            field_nodes,
            return_type,
            aliased,
            below: OnceCell::new(),
            repeat_of: None,
        }
    }
}

/// The fields a projection reads below the nodes of one field against one
/// type, with the one name the walk repeats.
pub(crate) struct Selection {
    /// The selected fields in the order of the walk.
    pub(crate) fields: Rc<Vec<SelectedField>>,
    /// The positions of the earlier and of the later field of the name the
    /// walk repeats, the later naming the earlier in
    /// [`SelectedField::repeat_of`].
    pub(crate) repeat: Option<(usize, usize)>,
    /// Whether the projection onto the selection of an object of each shape
    /// it met is the object itself, by the address of the shape.
    sources: RefCell<Vec<(usize, bool)>>,
    /// The address of the shape the selection answered last and its answer,
    /// which the objects of a list of one shape read with no borrow.
    last: Cell<Option<(usize, bool)>>,
}

impl Selection {
    /// Returns the selection of `fields`, one walk of [`get_selected_fields`].
    pub(crate) fn of(fields: Rc<Vec<SelectedField>>) -> Self {
        let repeat = fields
            .iter()
            .enumerate()
            .find_map(|(later, field)| Some((field.repeat_of?, later)));
        Self {
            fields,
            repeat,
            sources: RefCell::new(Vec::new()),
            last: Cell::new(None),
        }
    }

    /// Answers whether the projection onto the selection, of an object type,
    /// of an object of the static shape `shape` hands the executor what the
    /// object hands it, so the object is its own projection.
    ///
    /// The executor reads a field of an object type through the default
    /// resolver, the member of the field's name, which the projection holds
    /// as the loop of the reference wrapped it. The object answers the same
    /// member where every field of the selection holds:
    ///
    /// - It carries no alias, whose member the projection routes through a
    ///   function that reads the alias first (source lines 320-345).
    /// - Its name is no method of the shape, whose member the projection
    ///   wraps into a function over positional arguments (`:248-277`).
    /// - Its name is a data member that holds a leaf or a list of leaves,
    ///   which the wrap answers as itself or as a list of the same items
    ///   (`:205-243`), or a value of the JSON model at a leaf type, which
    ///   has no selection below it, or an object of a shape at an object
    ///   type whose selection below holds by the same rule, so its own
    ///   projection is itself, or a name the object does not hold, whose
    ///   member both read as `undefined`, `__typename` among them, which the
    ///   executor answers from the type.
    ///
    /// A member of an interface or a union type is projected with the
    /// fields its type resolver reads (`:115-183`), a member of an object
    /// type with an `isTypeOf` is handed to it as the projection, and a
    /// member that can hold a function or a promise is wrapped, so none of
    /// them holds. The answer is a function of the selection and of the
    /// shape, so the selection keeps it by the address of the shape, and the
    /// selections below it are those the projection reads, which their
    /// fields keep.
    #[inline]
    pub(crate) fn projects_to_source(
        &self,
        state: &CallState,
        info: &ResolveInfo,
        shape: &'static Shape,
    ) -> bool {
        let address = core::ptr::from_ref(shape).addr();
        match self.last.get() {
            Some((at, known)) if at == address => known,
            _ => self.answer_shape(state, info, shape, address),
        }
    }

    /// Answers [`Selection::projects_to_source`] for the shape `shape` at
    /// `address`, which the selection did not answer last, and keeps the
    /// answer.
    fn answer_shape(
        &self,
        state: &CallState,
        info: &ResolveInfo,
        shape: &'static Shape,
        address: usize,
    ) -> bool {
        let kept = self
            .sources
            .borrow()
            .iter()
            .find(|(at, _)| *at == address)
            .map(|(_, known)| *known);
        let source = kept.unwrap_or_else(|| {
            let source = self
                .fields
                .iter()
                .all(|field| reads_member(state, info, field, shape));
            self.sources.borrow_mut().push((address, source));
            source
        });
        self.last.set(Some((address, source)));
        source
    }
}

/// Answers whether the projection of an object of the shape `shape` holds
/// for `field` the member the object holds, as
/// [`Selection::projects_to_source`] says.
fn reads_member(
    state: &CallState,
    info: &ResolveInfo,
    field: &SelectedField,
    shape: &'static Shape,
) -> bool {
    if field.aliased {
        return false;
    }
    let Some(member) = shape.member(&field.name) else {
        return !shape.has_method(&field.name);
    };
    match member.kind() {
        MemberKind::Leaf => true,
        MemberKind::Dynamic => false,
        MemberKind::Json => !matches!(
            field.return_type,
            Some(NamedType::Object(_) | NamedType::Interface(_) | NamedType::Union(_))
        ),
        MemberKind::Object(shape_of) => match &field.return_type {
            Some(NamedType::Object(object_type)) if object_type.is_type_of.is_some() => false,
            Some(NamedType::Object(_)) => shape_of().is_some_and(|nested| {
                field
                    .below
                    .get_or_init(|| {
                        Selection::of(get_selected_fields(
                            state,
                            info,
                            &field.field_nodes,
                            field.return_type.as_ref(),
                        ))
                    })
                    .projects_to_source(state, info, nested)
            }),
            Some(NamedType::Interface(_) | NamedType::Union(_)) => false,
            _ => true,
        },
    }
}

/// The service state and the request state one resolver call works with.
///
/// The reference keeps both caches in module variables. A service of Aosabi
/// is shared between threads and a request is not, so the two lifetimes are
/// two values, and every function of this directory takes them together.
#[derive(Clone)]
pub struct CallState {
    /// What lives as long as the service.
    pub core: Arc<CoreState>,
    /// What lives as long as the request.
    pub request: Rc<RequestState>,
}

/// The nodes and the return type gathered for one field name.
struct FieldData {
    /// The nodes that select the name, in document order.
    nodes: Vec<Rc<FieldNode>>,
    /// The named type of the field, fixed by the first node.
    return_type: Option<NamedType>,
}

/// The `fieldsMap` of the reference, keyed by field name in first seen order.
type FieldsMap = IndexMap<String, FieldData, KeyHasherBuilder>;

/// Returns the fields selected below `field_nodes`, read against `parent_type`.
///
/// `info` is the `info` of the running field, `field_nodes` the nodes whose
/// selection sets are walked, and `parent_type` the named type those
/// selection sets select on. The reference defaults `fieldNodes` to
/// `info.fieldNodes`, and a caller of this function writes that default out.
///
/// The walk honours no `@skip` and no `@include` and matches no type
/// condition, as the reference does not. A field a document excludes is
/// projected and never read by the executor.
#[must_use]
pub fn get_selected_fields(
    state: &CallState,
    info: &ResolveInfo,
    field_nodes: &FieldNodes,
    parent_type: Option<&NamedType>,
) -> Rc<Vec<SelectedField>> {
    // 1. Check cache for this selection set and parent type
    if let Some(cached) = state.request.selected_fields(field_nodes, parent_type) {
        return cached;
    }

    let mut fields_map = FieldsMap::default();
    for field_node in field_nodes.iter() {
        if let Some(selection_set) = &field_node.selection_set {
            extract(
                info,
                &mut fields_map,
                &selection_set.selections,
                parent_type,
            );
        }
    }

    let mut result: Vec<SelectedField> = fields_map
        .into_iter()
        .map(|(name, data)| SelectedField::new(name, Rc::from(data.nodes), data.return_type))
        .collect();

    // Auto-inject Discriminator Fields for Abstract Types
    if let Some(parent_type @ (NamedType::Interface(_) | NamedType::Union(_))) = parent_type {
        inject_discriminators(state, info, parent_type, &mut result);
    }

    // 2. Cache the result before returning
    let result = Rc::new(result);
    state
        .request
        .set_selected_fields(field_nodes, parent_type, &result);
    result
}

/// Gathers the fields of `selections` into `fields_map`.
///
/// `current_type` is the type the selections select on, `None` where the
/// reference holds `undefined`, which is the case for a type condition that
/// names no type of the schema.
fn extract(
    info: &ResolveInfo,
    fields_map: &mut FieldsMap,
    selections: &[SelectionNode],
    current_type: Option<&NamedType>,
) {
    for selection in selections {
        match selection {
            SelectionNode::Field(field) => extract_field(fields_map, field, current_type),
            SelectionNode::InlineFragment(fragment) => {
                let inline_type = match &fragment.type_condition {
                    Some(condition) => info.schema.get_type(condition.name.value()),
                    None => current_type,
                };
                extract(
                    info,
                    fields_map,
                    &fragment.selection_set.selections,
                    inline_type,
                );
            }
            SelectionNode::FragmentSpread(spread) => {
                let Some(fragment) = info.fragments.get(spread.name.value()) else {
                    continue;
                };
                let fragment_type = info.schema.get_type(fragment.type_condition.name.value());
                extract(
                    info,
                    fields_map,
                    &fragment.selection_set.selections,
                    fragment_type,
                );
            }
        }
    }
}

/// Adds one field node to `fields_map` under the name of its field.
///
/// The return type is the one the first node of a name finds, because the
/// entry is created once and later nodes are pushed onto it.
fn extract_field(fields_map: &mut FieldsMap, field: &FieldNode, current_type: Option<&NamedType>) {
    let name = field.name.value();
    if !fields_map.contains_key(name) {
        let data = FieldData {
            nodes: Vec::new(),
            return_type: child_return_type(current_type, name),
        };
        fields_map.insert(String::from(name), data);
    }
    if let Some(data) = fields_map.get_mut(name) {
        // The node is copied out of its parent once, inside the cached
        // result, so the list that holds it has one address for the request.
        // The copy shares the selections below the node with its parent.
        data.nodes.push(Rc::new(field.clone()));
    }
}

/// Returns the named type of the field `name` of `current_type`.
///
/// Only an object type and an interface type answer fields, and a name the
/// type does not declare, such as `__typename`, has no return type.
fn child_return_type(current_type: Option<&NamedType>, name: &str) -> Option<NamedType> {
    let field_def = fields_of(current_type?)?.get(name)?;
    Some(get_named_type(&field_def.r#type))
}

/// Returns `type.getFields()` of an object type or an interface type.
///
/// A built schema has resolved every field thunk, so the error a thunk may
/// return is not reachable here and reads as a type without fields.
fn fields_of(named_type: &NamedType) -> Option<&ObjMap<Field>> {
    match named_type {
        NamedType::Object(object) => object.get_fields().ok(),
        NamedType::Interface(interface) => interface.get_fields().ok(),
        _ => None,
    }
}

/// Pushes `__typename` and the unique field of every possible type onto `result`.
///
/// `parent_type` is an interface type or a union type.
fn inject_discriminators(
    state: &CallState,
    info: &ResolveInfo,
    parent_type: &NamedType,
    result: &mut Vec<SelectedField>,
) {
    // An aliased `__typename` does not count. The projection writes a member
    // under the field name and the executor reads `__typename` from it, so
    // only a node without an alias proves that the member is carried.
    let carries_typename = result.iter().any(|field| {
        field.name == TYPENAME && field.field_nodes.iter().any(|node| node.alias.is_none())
    });
    if !carries_typename {
        let mut typename = injected_field(TYPENAME, None);
        typename.repeat_of = result.iter().position(|field| field.name == TYPENAME);
        result.push(typename);
    }

    // The reference reads the type back from the schema by name and throws a
    // `TypeError` for a name the schema lacks. A parent type comes out of the
    // schema of `info`, so the name is always found, and the fallback keeps
    // the function total.
    let abstract_type = info
        .schema
        .get_type(parent_type.name())
        .unwrap_or(parent_type);
    let possible_types = info
        .schema
        .get_possible_types(abstract_type)
        .unwrap_or_default();

    for possible_type in possible_types {
        let Some(type_fields_map) = fields_of(possible_type) else {
            continue;
        };
        let unique_field = state
            .core
            .unique_field(possible_type.name())
            .or_else(|| find_unique_field(state, possible_type, type_fields_map, possible_types));
        let Some(unique_field) = unique_field else {
            continue;
        };
        if result.iter().any(|field| field.name == unique_field) {
            continue;
        }
        let return_type = type_fields_map
            .get(&unique_field)
            .map(|field| get_named_type(&field.r#type));
        result.push(injected_field(&unique_field, return_type));
    }
}

/// Finds the first non null field of `possible_type` no other possible type
/// declares, and caches the answer.
///
/// The reference caches an empty list for a type without such a field and
/// reads the cache through `?.[0]`, which is `undefined` for an empty list,
/// so the search runs again on every call for such a type. The cache is keyed
/// by the possible type alone, so a type that belongs to two abstract types
/// keeps the field the first of them found.
fn find_unique_field(
    state: &CallState,
    possible_type: &NamedType,
    type_fields_map: &ObjMap<Field>,
    possible_types: &[NamedType],
) -> Option<String> {
    let declared_elsewhere = |name: &str| {
        possible_types
            .iter()
            .filter(|other| other.name() != possible_type.name())
            .filter_map(fields_of)
            .any(|fields| fields.contains_key(name))
    };
    let found_unique_field = type_fields_map
        .iter()
        .filter(|(_, field)| is_non_null_type(&field.r#type))
        .map(|(name, _)| name)
        .find(|name| !declared_elsewhere(name))
        .cloned();

    let cached = found_unique_field.iter().cloned().collect();
    state.core.set_unique_fields(possible_type.name(), cached);
    found_unique_field
}

/// Returns a selected field no node of the document selects.
fn injected_field(name: &str, return_type: Option<NamedType>) -> SelectedField {
    SelectedField::new(name.to_owned(), Rc::from(Vec::new()), return_type)
}
