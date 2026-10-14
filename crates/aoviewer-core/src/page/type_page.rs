// SPDX-License-Identifier: MIT
/*
 * aoviewer-core page type_page: the page of one named type.
 *
 * A type page holds the description, the diagram and the tables of its
 * members.
 */

//! The type page.
//!
//! A type page carries, in this order, the description, the neighbourhood
//! diagram, the fields table, the group that fits the kind, the cross
//! references, the pagination note and the viewer link. The deprecated group
//! sits after the pagination note, since the section lists it last among the
//! blocks and places it nowhere in its order.
//!
//! - An object and an interface have the fields table with the columns of the
//!   plan's example, `Field`, `Type`, `Arguments`, `Description` and
//!   `Deprecated`, and a column `Inherited from` when a field comes from an
//!   interface, which is how an implementation lists its inherited fields with
//!   the interface they come from. The arguments of a field are a count in the
//!   table that links to one section per field with its arguments table under
//!   it.
//! - The group of a union is the name grid of its possible types, of an
//!   interface the name grid of its implementers, of an enum its values table
//!   and of an input its input fields table. A custom scalar names the
//!   specification URL it declares.
//! - The cross references are the interfaces the type implements, the fields
//!   that return or take it and the arguments that take it. A field the Relay
//!   collapse turns into a list of the type uses it as well, since that is the
//!   type the page shows for the field, and a field of a wrapper the collapse
//!   finds is left out, because the collapsed field stands for it and the
//!   Relay page documents the wrapper as one row.
//! - The pagination note is a table with one row per collapsed field, so it
//!   links the connection, the edge and the page info type.
//! - The deprecated group lists the deprecated members the graph hides with
//!   their reasons, so under `deprecated=1` it is left out.

use alloc::{
    format,
    string::String,
    vec,
    vec::Vec, //
};

use super::{
    Block,
    Cell,
    PageModel,
    count32,
    deprecation_cell,
    description_cell,
    empty,
    field_ref,
    link,
    overview_crumb,
    page_of_type,
    push_description,
    push_details,
    push_diagrams,
    shown_type,
    sort_names,
    title,
    type_link,
    type_ref_cell,
    type_route, //
};
use crate::{
    cluster::Clustering,
    id::ElementId,
    model::{
        FieldId,
        Kind,
        RefKind,
        TypeId, //
    },
    route::{
        Options,
        Route, //
    },
    schema::Schema,
    transform::Member,
    view::ViewError,
};

/// Returns the page of the type `ty`, which is no root, for `route`.
///
/// # Errors
///
/// Returns the errors of [`View::for_page`](crate::view::View::for_page) for
/// the route of the type.
pub(super) fn type_page(
    schema: &Schema,
    clustering: &Clustering,
    route: &Route,
    ty: TypeId,
    options: &Options,
) -> Result<PageModel, ViewError> {
    let model = &schema.model;
    let t = model.ty(ty);
    let name = model.name(t.name);
    let mut blocks = Vec::new();
    push_description(t.description.as_deref(), &mut blocks);
    push_diagrams(schema, clustering, &type_route(name), options, &mut blocks)?;
    if matches!(t.kind, Kind::Object | Kind::Interface) {
        push_fields(schema, ty, &mut blocks);
    }
    push_kind_group(schema, ty, &mut blocks);
    push_cross_references(schema, ty, &mut blocks);
    let fields = schema.transformed.fields(model, ty);
    push_pagination(schema, &fields, &mut blocks);
    push_deprecated_group(schema, ty, &mut blocks);
    blocks.push(Block::ViewerLink {
        route: route.clone(),
    });
    Ok(PageModel {
        route: route.clone(),
        title: String::from(name),
        kind: Some(t.kind),
        breadcrumb: breadcrumb(clustering, ty),
        blocks,
    })
}

/// Returns the breadcrumb of the type `ty`, the home page and the cluster
/// that holds the type, if one does.
fn breadcrumb(clustering: &Clustering, ty: TypeId) -> Vec<(String, Route)> {
    let mut crumbs = overview_crumb();
    let cluster = clustering
        .of(ty)
        .and_then(|i| clustering.clusters.get(i as usize));
    if let Some(cluster) = cluster {
        crumbs.push((
            cluster.name.clone(),
            Route::Cluster {
                slug: cluster.slug.clone(),
            },
        ));
    }
    crumbs
}

/// Appends the fields table of the object or interface `ty`, the details of
/// the descriptions it cut, and one section per field with arguments.
fn push_fields(schema: &Schema, ty: TypeId, blocks: &mut Vec<Block>) {
    let (model, tr) = (&schema.model, &schema.transformed);
    let fields = tr.fields(model, ty);
    if fields.is_empty() {
        return;
    }
    let inherited = fields.iter().any(|&f| tr.inherited_from(f).is_some());
    let mut columns = vec!["Field", "Type", "Arguments", "Description", "Deprecated"];
    if inherited {
        columns.push("Inherited from");
    }
    let mut details = Vec::new();
    let mut rows = Vec::with_capacity(fields.len());
    for &f in &fields {
        let mut row = field_row(schema, f, &mut details);
        if inherited {
            row.push(
                tr.inherited_from(f)
                    .map_or_else(empty, |i| type_link(model, i)),
            );
        }
        rows.push(row);
    }
    blocks.push(Block::Table {
        title: title::FIELDS,
        columns,
        rows,
    });
    push_details(details, blocks);
    for &f in &fields {
        let field = model.field(f);
        if !field.args.is_empty() {
            blocks.push(Block::Section {
                title: String::from(model.name(field.name)),
                id: ElementId::of_field(model, f),
                blocks: arguments(schema, f),
            });
        }
    }
}

/// Returns the row of the field `f` in a fields table, the name, the type it
/// shows, the argument count, the description and the deprecation.
fn field_row(schema: &Schema, f: FieldId, details: &mut Vec<(ElementId, String)>) -> Vec<Cell> {
    let model = &schema.model;
    let field = model.field(f);
    let id = ElementId::of_field(model, f);
    let args = if field.args.is_empty() {
        empty()
    } else {
        Cell::Count {
            n: count32(field.args.len()),
            id: id.clone(),
        }
    };
    vec![
        Cell::Name {
            text: String::from(model.name(field.name)),
            id: id.clone(),
        },
        type_ref_cell(model, shown_type(schema, f)),
        args,
        description_cell(field.description.as_deref(), &id, details),
        deprecation_cell(schema.transformed.mark(Member::Field(f))),
    ]
}

/// Returns the arguments table of the field `f` in the order of the
/// transforms, with the details of the descriptions it cut.
///
/// The table lists every argument, the paging arguments the Relay collapse
/// strips included, which the pagination note names again.
pub(super) fn arguments(schema: &Schema, f: FieldId) -> Vec<Block> {
    let (model, tr) = (&schema.model, &schema.transformed);
    let mut details = Vec::new();
    let mut rows = Vec::new();
    for a in tr.args(model, f) {
        let Some(id) = ElementId::of_arg(model, a) else {
            continue;
        };
        let arg = model.arg(a);
        rows.push(vec![
            Cell::Name {
                text: String::from(model.name(arg.name)),
                id: id.clone(),
            },
            type_ref_cell(model, &arg.type_ref),
            default_cell(arg.default_value.as_deref()),
            description_cell(arg.description.as_deref(), &id, &mut details),
            deprecation_cell(tr.mark(Member::Arg(a))),
        ]);
    }
    let mut blocks = vec![Block::Table {
        title: title::ARGUMENTS,
        columns: vec!["Argument", "Type", "Default", "Description", "Deprecated"],
        rows,
    }];
    push_details(details, &mut blocks);
    blocks
}

/// Returns the cell of the default value `value`, or the empty cell.
fn default_cell(value: Option<&str>) -> Cell {
    value.map_or_else(empty, |v| Cell::Default(String::from(v)))
}

/// Appends the group that fits the kind of `ty`.
fn push_kind_group(schema: &Schema, ty: TypeId, blocks: &mut Vec<Block>) {
    let (model, tr) = (&schema.model, &schema.transformed);
    let t = model.ty(ty);
    match t.kind {
        Kind::Object => {}
        Kind::Interface => {
            let mut names: Vec<(String, ElementId)> = model
                .implementers(ty)
                .iter()
                .map(|&i| type_name(schema, i))
                .collect();
            sort_names(&mut names);
            push_grid(title::IMPLEMENTED_BY, names, blocks);
        }
        Kind::Union => {
            let mut names: Vec<(String, ElementId)> = tr
                .members(model, ty)
                .into_iter()
                .map(|m| type_name(schema, m))
                .collect();
            sort_names(&mut names);
            push_grid(title::POSSIBLE_TYPES, names, blocks);
        }
        Kind::Enum => push_values(schema, ty, blocks),
        Kind::Input => push_input_fields(schema, ty, blocks),
        Kind::Scalar => {
            if let Some(url) = &t.specified_by_url {
                blocks.push(Block::Note {
                    text: format!("Specified by {url}"),
                });
            }
        }
    }
}

/// Appends the values table of the enum `ty` and the details it cut.
fn push_values(schema: &Schema, ty: TypeId, blocks: &mut Vec<Block>) {
    let (model, tr) = (&schema.model, &schema.transformed);
    let mut details = Vec::new();
    let rows: Vec<Vec<Cell>> = tr
        .values(model, ty)
        .into_iter()
        .map(|v| {
            let value = model.value(v);
            let id = ElementId::of_value(model, v);
            vec![
                Cell::Name {
                    text: String::from(model.name(value.name)),
                    id: id.clone(),
                },
                description_cell(value.description.as_deref(), &id, &mut details),
                deprecation_cell(tr.mark(Member::Value(v))),
            ]
        })
        .collect();
    blocks.push(Block::Table {
        title: title::VALUES,
        columns: vec!["Value", "Description", "Deprecated"],
        rows,
    });
    push_details(details, blocks);
}

/// Appends the input fields table of the input `ty` and the details it cut.
///
/// The table has the columns of `docs/documentation-generator.md` section 4,
/// with no deprecation, and the deprecated group lists a deprecated input
/// field.
fn push_input_fields(schema: &Schema, ty: TypeId, blocks: &mut Vec<Block>) {
    let (model, tr) = (&schema.model, &schema.transformed);
    let mut details = Vec::new();
    let rows: Vec<Vec<Cell>> = tr
        .fields(model, ty)
        .into_iter()
        .map(|f| {
            let field = model.field(f);
            let id = ElementId::of_field(model, f);
            vec![
                Cell::Name {
                    text: String::from(model.name(field.name)),
                    id: id.clone(),
                },
                type_ref_cell(model, &field.type_ref),
                default_cell(field.default_value.as_deref()),
                description_cell(field.description.as_deref(), &id, &mut details),
            ]
        })
        .collect();
    blocks.push(Block::Table {
        title: title::INPUT_FIELDS,
        columns: vec!["Field", "Type", "Default", "Description"],
        rows,
    });
    push_details(details, blocks);
}

/// Appends the cross references of `ty`: the interfaces it implements, the
/// fields that use it and the arguments that take it, each list in name
/// order.
///
/// A field of an introspection type, which no page documents, is left out
/// with its arguments, so every name of the lists links to an anchor a page
/// writes. A field of a Relay wrapper is left out as well, since the
/// collapsed field stands for it.
pub(super) fn push_cross_references(schema: &Schema, ty: TypeId, blocks: &mut Vec<Block>) {
    let (model, tr) = (&schema.model, &schema.transformed);
    let mut implements: Vec<(String, ElementId)> = tr
        .implements(model, ty)
        .into_iter()
        .map(|i| type_name(schema, i))
        .collect();
    sort_names(&mut implements);
    push_grid(title::IMPLEMENTS, implements, blocks);
    let listed = |f: FieldId| {
        let owner = model.field(f).owner;
        !tr.is_relay_type(owner) && page_of_type(schema, owner).is_some()
    };
    let mut used_by = Vec::new();
    let mut as_argument = Vec::new();
    for &(f, kind) in model.referrers(ty) {
        if !listed(f) {
            continue;
        }
        match kind {
            RefKind::Field | RefKind::InputField => used_by.push(field_ref(model, f)),
            RefKind::Argument => as_argument.extend(argument_refs(schema, f, ty)),
        }
    }
    used_by.extend(
        tr.collapsed
            .iter()
            .filter(|c| c.node.named() == ty && listed(c.field))
            .map(|c| field_ref(model, c.field)),
    );
    sort_names(&mut used_by);
    sort_names(&mut as_argument);
    push_grid(title::USED_BY, used_by, blocks);
    push_grid(title::USED_AS_ARGUMENT_BY, as_argument, blocks);
}

/// Returns a reference to every argument of the field `f` that takes the
/// type `ty`, such as `Organization.projects(filter)`.
fn argument_refs(schema: &Schema, f: FieldId, ty: TypeId) -> Vec<(String, ElementId)> {
    let model = &schema.model;
    let (owner, _) = field_ref(model, f);
    model
        .field(f)
        .args
        .iter()
        .filter(|&&a| model.arg(a).type_ref.named() == ty)
        .filter_map(|&a| {
            let id = ElementId::of_arg(model, a)?;
            Some((format!("{owner}({})", model.name(model.arg(a).name)), id))
        })
        .collect()
}

/// Appends the pagination note of the collapsed fields among `fields`, one row
/// per field naming the connection, the edge and the page info type, the
/// stripped arguments and every extra field the wrappers carry.
pub(super) fn push_pagination(schema: &Schema, fields: &[FieldId], blocks: &mut Vec<Block>) {
    let (model, tr) = (&schema.model, &schema.transformed);
    let rows: Vec<Vec<Cell>> = fields
        .iter()
        .filter_map(|&f| tr.collapse_of(f))
        .map(|c| {
            let stripped: Vec<&str> = c
                .stripped_args
                .iter()
                .map(|&a| model.name(model.arg(a).name))
                .collect();
            let extra: Vec<String> = c
                .extra_fields
                .iter()
                .map(|&e| field_ref(model, e).0)
                .collect();
            vec![
                link(
                    String::from(model.name(model.field(c.field).name)),
                    ElementId::of_field(model, c.field),
                ),
                type_link(model, c.connection),
                type_link(model, c.edge),
                c.page_info.map_or_else(empty, |p| type_link(model, p)),
                Cell::Text(stripped.join(", ")),
                Cell::Text(extra.join(", ")),
            ]
        })
        .collect();
    if !rows.is_empty() {
        blocks.push(Block::Table {
            title: title::PAGINATION,
            columns: vec![
                "Field",
                "Connection",
                "Edge",
                "Page info",
                "Stripped arguments",
                "Extra fields",
            ],
            rows,
        });
    }
}

/// Appends the deprecated group of `ty`, every deprecated field, argument,
/// input field and enum value the graph hides, with its reason.
fn push_deprecated_group(schema: &Schema, ty: TypeId, blocks: &mut Vec<Block>) {
    if schema.transforms.deprecated {
        return;
    }
    let rows = deprecated_rows(schema, ty, false);
    if !rows.is_empty() {
        blocks.push(Block::Table {
            title: title::DEPRECATED,
            columns: vec!["Member", "Reason"],
            rows,
        });
    }
}

/// Returns one row per deprecated member of `ty` in the order of the
/// transforms, each field before its arguments and the values last, with a
/// link to the member and its deprecation.
///
/// The link reads the member alone, `projects` or `projects(first)`, and
/// with `qualified` it reads the type first, `Organization.projects`.
///
/// A member of a type the Relay page or the scalars page documents links to
/// its type, because those pages hold one row per type and write no anchor
/// for a member (see [`page_of`](super::page_of)).
pub(super) fn deprecated_rows(schema: &Schema, ty: TypeId, qualified: bool) -> Vec<Vec<Cell>> {
    let (model, tr) = (&schema.model, &schema.transformed);
    let prefix = if qualified {
        format!("{}.", model.name(model.ty(ty).name))
    } else {
        String::new()
    };
    let anchored = matches!(page_of_type(schema, ty), Some(Route::Type { .. }));
    let target = |member: ElementId| {
        if anchored {
            member
        } else {
            ElementId::of_type(model, ty)
        }
    };
    let mut rows = Vec::new();
    for f in tr.fields(model, ty) {
        let name = model.name(model.field(f).name);
        if let Some(mark) = tr.mark(Member::Field(f)) {
            let id = ElementId::of_field(model, f);
            rows.push(vec![
                link(format!("{prefix}{name}"), target(id)),
                deprecation_cell(Some(mark)),
            ]);
        }
        for a in tr.args(model, f) {
            let (Some(mark), Some(id)) = (tr.mark(Member::Arg(a)), ElementId::of_arg(model, a))
            else {
                continue;
            };
            let arg = model.name(model.arg(a).name);
            rows.push(vec![
                link(format!("{prefix}{name}({arg})"), target(id)),
                deprecation_cell(Some(mark)),
            ]);
        }
    }
    for v in tr.values(model, ty) {
        if let Some(mark) = tr.mark(Member::Value(v)) {
            let name = model.name(model.value(v).name);
            rows.push(vec![
                link(
                    format!("{prefix}{name}"),
                    target(ElementId::of_value(model, v)),
                ),
                deprecation_cell(Some(mark)),
            ]);
        }
    }
    rows
}

/// Returns the name of the type `ty` with its id, for a name grid.
fn type_name(schema: &Schema, ty: TypeId) -> (String, ElementId) {
    let model = &schema.model;
    (
        String::from(model.name(model.ty(ty).name)),
        ElementId::of_type(model, ty),
    )
}

/// Appends the name grid `title` when it holds a name.
fn push_grid(title: &'static str, names: Vec<(String, ElementId)>, blocks: &mut Vec<Block>) {
    if !names.is_empty() {
        blocks.push(Block::Grid { title, names });
    }
}
