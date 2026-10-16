// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool viewer page-model: the operation page-model, the page model of
 * a route as JSON.
 *
 * A page is a list of blocks, and the page renderers read it and nothing else,
 * so the model is what the Markdown and the Typst of a page are judged against.
 */

//! The `page-model` operation.
//!
//! The request `{input, route, routes, declared}` answers `pages`, one entry
//! per route with its `route` and its `page`, or with `refused` and
//! `message` when the core refuses the route. The options of a route come
//! from its hash, and its schema is built under their transforms.
//!
//! A page carries `route`, `title`, `kind`, the kind word of the type or
//! null, `breadcrumb` as pairs of a title and a route, and `blocks`. A block
//! is an object whose `block` names its variant in lower case with an
//! underscore between words, beside the fields of the variant, and a cell
//! of a table is an object whose `cell` names its variant the same way. A
//! route is written as its canonical hash without options and an element as
//! the text of its id, such as `FIELD::Organization::projects`.

use aosabi_json::{
    JsObject,
    JsValue, //
};
use aoviewer_core::{
    model::Model,
    page::{
        Block,
        Cell,
        DescriptionSource,
        PageModel,
        TypeRefPart,
        kind_word, //
    },
    route::{
        Options,
        Route, //
    }, //
};

use super::{
    Stop,
    declared,
    each_route,
    empty_json,
    load,
    refused_by,
    routes,
    view_refused, //
};

/// Answers the operation `page-model`.
pub(super) fn run(request: &JsObject) -> Result<JsObject, Stop> {
    let asked = routes(request)?;
    let declared = declared(request)?;
    let model = load(request)?.model;
    let out = each_route(model, declared, &asked, |schema, clustering, a| {
        let mut entry = match PageModel::of(schema, clustering, &a.route, &a.options) {
            Ok(page) => {
                let mut entry = JsObject::with_capacity(2);
                entry.insert("page", page_json(&schema.model, &page));
                entry
            }
            Err(e) => refused_by(&view_refused(e)),
        };
        entry.insert("route", a.hash.as_str());
        JsValue::Object(entry)
    });
    let mut payload = JsObject::with_capacity(1);
    payload.insert("pages", JsValue::Array(out));
    Ok(payload)
}

/// Returns `page` of `model` as JSON.
fn page_json(model: &Model, page: &PageModel) -> JsValue {
    let breadcrumb = page
        .breadcrumb
        .iter()
        .map(|(title, route)| JsValue::Array(vec![JsValue::from(title.as_str()), hash(route)]))
        .collect();
    let mut out = JsObject::with_capacity(5);
    out.insert("route", hash(&page.route));
    out.insert("title", page.title.as_str());
    out.insert(
        "kind",
        page.kind
            .map_or(JsValue::Null, |k| JsValue::from(kind_word(k))),
    );
    out.insert("breadcrumb", JsValue::Array(breadcrumb));
    out.insert("blocks", blocks(model, &page.blocks));
    JsValue::Object(out)
}

/// Returns the canonical hash of `route` without options.
fn hash(route: &Route) -> JsValue {
    JsValue::from(route.to_hash(&Options::default()))
}

/// Returns `blocks` of a page of `model` as a JSON array.
///
/// A section holds blocks of its own, which this function writes again, so the
/// recursion is as deep as the page model nests its sections, one per field of
/// a page.
fn blocks(model: &Model, blocks: &[Block]) -> JsValue {
    JsValue::Array(blocks.iter().map(|b| block(model, b)).collect())
}

/// Returns one block of a page of `model` as JSON.
fn block(model: &Model, block: &Block) -> JsValue {
    let id = |id: &aoviewer_core::id::ElementId| JsValue::from(id.text(model));
    let mut out = JsObject::with_capacity(4);
    match block {
        Block::Description { text } => {
            out.insert("block", "description");
            out.insert("text", text.as_str());
        }
        Block::Diagram {
            route,
            part,
            skipped,
        } => {
            out.insert("block", "diagram");
            out.insert("route", hash(route));
            out.insert(
                "part",
                part.map_or(JsValue::Null, |n| JsValue::from(u32::from(n))),
            );
            out.insert("skipped", skipped.map_or(JsValue::Null, empty_json));
        }
        Block::Table {
            title,
            columns,
            rows,
        } => {
            out.insert("block", "table");
            out.insert("title", *title);
            let columns = columns.iter().map(|&c| JsValue::from(c)).collect();
            out.insert("columns", JsValue::Array(columns));
            let rows = rows
                .iter()
                .map(|row| JsValue::Array(row.iter().map(|c| cell(model, c)).collect()))
                .collect();
            out.insert("rows", JsValue::Array(rows));
        }
        Block::Details { title, entries } => {
            out.insert("block", "details");
            out.insert("title", *title);
            let entries = entries
                .iter()
                .map(|(e, text)| JsValue::Array(vec![id(e), JsValue::from(text.as_str())]))
                .collect();
            out.insert("entries", JsValue::Array(entries));
        }
        Block::Grid { title, names } => {
            out.insert("block", "grid");
            out.insert("title", *title);
            let names = names
                .iter()
                .map(|(text, e)| JsValue::Array(vec![JsValue::from(text.as_str()), id(e)]))
                .collect();
            out.insert("names", JsValue::Array(names));
        }
        Block::Note { text } => {
            out.insert("block", "note");
            out.insert("text", text.as_str());
        }
        Block::Signature { text } => {
            out.insert("block", "signature");
            out.insert("text", text.as_str());
        }
        Block::Section {
            title,
            id: section,
            blocks: inner,
        } => {
            out.insert("block", "section");
            out.insert("title", title.as_str());
            out.insert("id", id(section));
            out.insert("blocks", blocks(model, inner));
        }
        Block::ViewerLink { route } => {
            out.insert("block", "viewer_link");
            out.insert("route", hash(route));
        }
    }
    JsValue::Object(out)
}

/// Returns one cell of a table of `model` as JSON.
fn cell(model: &Model, cell: &Cell) -> JsValue {
    let mut out = JsObject::with_capacity(4);
    match cell {
        Cell::Text(text) => {
            out.insert("cell", "text");
            out.insert("text", text.as_str());
        }
        Cell::Name { text, id } => {
            out.insert("cell", "name");
            out.insert("text", text.as_str());
            out.insert("id", id.text(model));
        }
        Cell::TypeRef { parts } => {
            out.insert("cell", "type_ref");
            let parts = parts.iter().map(|p| part(model, p)).collect();
            out.insert("parts", JsValue::Array(parts));
        }
        Cell::Description {
            first,
            more,
            source,
        } => {
            out.insert("cell", "description");
            out.insert("first", first.as_str());
            out.insert(
                "more",
                more.as_ref()
                    .map_or(JsValue::Null, |id| JsValue::from(id.text(model))),
            );
            out.insert(
                "source",
                match source {
                    DescriptionSource::Served => "served",
                    DescriptionSource::IrOnly => "ir_only",
                },
            );
        }
        Cell::Deprecation { reason, from_tag } => {
            out.insert("cell", "deprecation");
            out.insert(
                "reason",
                reason.as_deref().map_or(JsValue::Null, JsValue::from),
            );
            out.insert("from_tag", *from_tag);
        }
        Cell::Count { n, id } => {
            out.insert("cell", "count");
            out.insert("n", *n);
            out.insert("id", id.text(model));
        }
        Cell::Default(text) => {
            out.insert("cell", "default");
            out.insert("text", text.as_str());
        }
    }
    JsValue::Object(out)
}

/// Returns one part of a type reference as `[kind, text]` or, for a name,
/// `["name", text, id]`.
fn part(model: &Model, part: &TypeRefPart) -> JsValue {
    match part {
        TypeRefPart::Open(text) => {
            JsValue::Array(vec![JsValue::from("open"), JsValue::from(*text)])
        }
        TypeRefPart::Name { text, id } => JsValue::Array(vec![
            JsValue::from("name"),
            JsValue::from(text.as_str()),
            JsValue::from(id.text(model)),
        ]),
        TypeRefPart::Close(text) => {
            JsValue::Array(vec![JsValue::from("close"), JsValue::from(*text)])
        }
    }
}
