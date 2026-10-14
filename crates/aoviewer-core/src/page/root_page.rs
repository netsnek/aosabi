// SPDX-License-Identifier: MIT
/*
 * aoviewer-core page root_page: the page of one root type and the page of one
 * root field.
 *
 * A root page carries one section per root field with its signature, its
 * arguments table, its return type and its operation diagram.
 */

//! The root page and the page of a root field.
//!
//! A root page opens with the description of the root and holds one
//! [`Block::Section`] per root field in the order of the transforms, then the
//! cross references of the root type and the viewer link. The page of one
//! root field, the route `#/op/<root>/<field>`, holds the blocks of that
//! field's section, since the viewer shows that page beside the operation
//! graph.
//!
//! The blocks of a root field are its signature, its description, the
//! sentence of its deprecation, its arguments table, its return type, its
//! pagination note when the Relay collapse collapsed it, and its operation
//! diagram. The signature is the field as the schema declares it, its
//! arguments in schema order with their defaults, and the return type the
//! page shows is the one after the Relay collapse, as in a fields table.

use alloc::{
    format,
    string::String,
    vec,
    vec::Vec, //
};

use super::{
    Block,
    PageModel,
    overview_crumb,
    push_description,
    push_diagrams,
    shown_type,
    summary_cell,
    title,
    type_page::{
        arguments,
        push_cross_references,
        push_pagination, //
    },
    type_ref_cell,
    type_route, //
};
use crate::{
    cluster::Clustering,
    id::ElementId,
    model::{
        FieldId,
        Model,
        RootKind, //
    },
    route::{
        Options,
        Route, //
    },
    schema::Schema,
    transform::{
        Marked,
        Member, //
    },
    view::ViewError,
};

/// Returns the root page of the root `root` for `route`.
///
/// # Errors
///
/// Returns [`ViewError::NoRoot`] when the schema declares no root `root`, and
/// the errors of [`View::for_page`](crate::view::View::for_page) for the
/// operation route of a root field.
pub(super) fn root_page(
    schema: &Schema,
    clustering: &Clustering,
    route: &Route,
    root: RootKind,
    options: &Options,
) -> Result<PageModel, ViewError> {
    let model = &schema.model;
    let ty = model.roots().get(root).ok_or(ViewError::NoRoot(root))?;
    let t = model.ty(ty);
    let mut blocks = Vec::new();
    push_description(t.description.as_deref(), &mut blocks);
    for f in schema.transformed.fields(model, ty) {
        blocks.push(Block::Section {
            title: String::from(model.name(model.field(f).name)),
            id: ElementId::of_field(model, f),
            blocks: field_blocks(schema, clustering, root, f, options)?,
        });
    }
    push_cross_references(schema, ty, &mut blocks);
    blocks.push(Block::ViewerLink {
        route: route.clone(),
    });
    Ok(PageModel {
        route: route.clone(),
        title: String::from(model.name(t.name)),
        kind: Some(t.kind),
        breadcrumb: overview_crumb(),
        blocks,
    })
}

/// Returns the page of the field `field` of the root `root` for `route`.
///
/// # Errors
///
/// Returns [`ViewError::NoRoot`] when the schema declares no root `root`,
/// [`ViewError::UnknownField`] when the root declares no field `field`, and
/// the errors of [`View::for_page`](crate::view::View::for_page) for the
/// route.
pub(super) fn op_page(
    schema: &Schema,
    clustering: &Clustering,
    route: &Route,
    root: RootKind,
    field: &str,
    options: &Options,
) -> Result<PageModel, ViewError> {
    let model = &schema.model;
    let ty = model.roots().get(root).ok_or(ViewError::NoRoot(root))?;
    let root_name = model.name(model.ty(ty).name);
    let found = model
        .ty(ty)
        .fields
        .iter()
        .copied()
        .find(|&f| model.name(model.field(f).name) == field);
    let Some(f) = found else {
        return Err(ViewError::UnknownField {
            ty: String::from(root_name),
            field: String::from(field),
        });
    };
    let mut blocks = field_blocks(schema, clustering, root, f, options)?;
    blocks.push(Block::ViewerLink {
        route: route.clone(),
    });
    let mut breadcrumb = overview_crumb();
    breadcrumb.push((String::from(root_name), type_route(root_name)));
    Ok(PageModel {
        route: route.clone(),
        title: String::from(field),
        kind: None,
        breadcrumb,
        blocks,
    })
}

/// Returns the blocks of the root field `f` of the root `root`.
fn field_blocks(
    schema: &Schema,
    clustering: &Clustering,
    root: RootKind,
    f: FieldId,
    options: &Options,
) -> Result<Vec<Block>, ViewError> {
    let (model, tr) = (&schema.model, &schema.transformed);
    let field = model.field(f);
    let mut blocks = vec![Block::Signature {
        text: signature(model, f),
    }];
    push_description(field.description.as_deref(), &mut blocks);
    if let Some(mark) = tr.mark(Member::Field(f)) {
        blocks.push(Block::Note {
            text: deprecation_sentence(mark),
        });
    }
    if !field.args.is_empty() {
        blocks.extend(arguments(schema, f));
    }
    let returns = shown_type(schema, f);
    let named = model.ty(returns.named());
    blocks.push(Block::Table {
        title: title::RETURNS,
        columns: vec!["Type", "Description"],
        rows: vec![vec![
            type_ref_cell(model, returns),
            summary_cell(named.description.as_deref()),
        ]],
    });
    push_pagination(schema, &[f], &mut blocks);
    let op = Route::Op {
        root,
        field: String::from(model.name(field.name)),
    };
    push_diagrams(schema, clustering, &op, options, &mut blocks)?;
    Ok(blocks)
}

/// Returns the signature of the field `f` as the schema declares it, such as
/// `organization(id: ID!, first: Int = 10): Organization`.
fn signature(model: &Model, f: FieldId) -> String {
    let field = model.field(f);
    let mut out = String::from(model.name(field.name));
    if !field.args.is_empty() {
        let args: Vec<String> = field
            .args
            .iter()
            .map(|&a| {
                let arg = model.arg(a);
                let name = model.name(arg.name);
                let ty = arg.type_ref.parts(model).text();
                match &arg.default_value {
                    Some(value) => format!("{name}: {ty} = {value}"),
                    None => format!("{name}: {ty}"),
                }
            })
            .collect();
        out.push('(');
        out.push_str(&args.join(", "));
        out.push(')');
    }
    out.push_str(": ");
    out.push_str(&field.type_ref.parts(model).text());
    out
}

/// Returns the sentence that says a root field is deprecated, with the reason
/// and where the mark comes from.
fn deprecation_sentence(mark: &Marked) -> String {
    match mark {
        Marked::Directive { reason: Some(r) } => format!("Deprecated: {r}"),
        Marked::Directive { reason: None } => String::from("Deprecated."),
        Marked::Tag { reason: Some(r) } => format!("Deprecated: {r} (from the description)"),
        Marked::Tag { reason: None } => String::from("Deprecated (from the description)."),
    }
}
