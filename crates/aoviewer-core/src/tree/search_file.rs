// SPDX-License-Identifier: MIT
/*
 * aoviewer-core tree search_file: the search file of a tree.
 *
 * The search file lists every name with its kind, its route and its link,
 * sorted by name bytes.
 */

//! The search file.
//!
//! `search-index.json` is
//! `{"version":1,"entries":[[name,kind,route,href], ...]}`, every name of the
//! schema with its kind, the route of the viewer that shows it and the link
//! of the Markdown page that documents it, sorted by the bytes of the name,
//! then of the kind, then of the route. It has no cap, since it lists every
//! name.
//!
//! - The names are every type a page documents, every field, input field,
//!   argument and enum value of those types, and every cluster. The
//!   introspection types and their members, which no page documents, and the
//!   directives, which no page lists, are left out.
//! - The kind is the kind of a type as a page writes it, such as `object`,
//!   or `field`, `input field`, `argument`, `enum value` or `cluster`.
//! - The route is the hash of the viewer under its default options, the route
//!   of the operation for a root field and the route of a type with the
//!   member selected for every other member.
//! - The link is the path of the Markdown file inside the tree with the
//!   anchor of the name, from [󰀀](crate::render_markdown::page_path) and
//!   [󰀁](crate::id::Anchors), and `null` when the tree writes no Markdown. A
//!   member of a Relay wrapper links the row of its wrapper, the one anchor
//!   the Relay page writes for it.
//!
//! Every entry is one line, so a difference of two files reads line by line,
//! and the JSON is the same document with or without the line feeds. Every
//! string is written as `JSON.stringify` writes it.
//!
//! Above [󰀀](super::LETTER_TYPES) types the entries move to
//! `search-index/<letter>.json`, one file per first letter of a name in the
//! same shape, and `search-index.json` lists the parts as
//! `{"version":1,"parts":[[letter,path,entries],...]}`.
//!
//! Every loop runs over the types, members and clusters of one schema, whose
//! counts [`ReadLimits`](crate::limits::ReadLimits) caps, over the entries
//! they give, or over the letters of the parts, of which there are at most
//! 37.

use alloc::{
    string::String,
    vec::Vec, //
};
use core::fmt::Write;

use super::plan::{
    Site,
    documented_types,
    first_letter,
    type_route, //
};
use crate::{
    id::ElementId,
    json::write_str,
    model::{
        Kind,
        TypeId, //
    },
    page::{
        kind_word,
        page_of_type, //
    },
    route::{
        Options,
        Route, //
    },
};

/// The version of the search file.
const VERSION: u32 = 1;

/// One name of the search file.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Entry {
    /// The name.
    name: String,
    /// The kind.
    kind: &'static str,
    /// The route of the viewer.
    route: String,
    /// The link of the Markdown page, or `None` when the tree writes no
    /// Markdown.
    href: Option<String>,
}

/// Returns every entry of the search file of `site`, in the byte order of
/// the name, the kind and the route.
pub(super) fn entries(site: &Site<'_>) -> Vec<Entry> {
    let mut out = Vec::new();
    for t in documented_types(site.schema) {
        type_entries(site, t, &mut out);
    }
    for cluster in &site.clustering.clusters {
        let route = Route::Cluster {
            slug: cluster.slug.clone(),
        };
        let id = ElementId::Cluster(cluster.slug.clone());
        out.push(Entry {
            name: cluster.name.clone(),
            kind: "cluster",
            route: hash(&route),
            href: href(site, &id, &route),
        });
    }
    out.sort_unstable_by(|a, b| {
        (a.name.as_str(), a.kind, a.route.as_str()).cmp(&(
            b.name.as_str(),
            b.kind,
            b.route.as_str(),
        ))
    });
    out.dedup();
    out
}

/// Appends the entries of the type `t` and of its members.
fn type_entries(site: &Site<'_>, t: TypeId, out: &mut Vec<Entry>) {
    let model = &site.schema.model;
    let ty = model.ty(t);
    let name = model.name(ty.name);
    let page = page_of_type(site.schema, t);
    let own = matches!(page, Some(Route::Type { .. }));
    let type_id = ElementId::of_type(model, t);
    let type_hash = hash(&type_route(name));
    let type_href = page.and_then(|p| href(site, &type_id, &p));
    out.push(entry(
        name,
        kind_word(ty.kind),
        type_hash,
        type_href.clone(),
    ));
    let root = model.roots().kind_of(t);
    let member_href = |id: &ElementId| {
        if own {
            href(site, id, &type_route(name))
        } else {
            type_href.clone()
        }
    };
    for &f in &ty.fields {
        let field = model.field(f);
        let field_name = model.name(field.name);
        let route = match root {
            Some(root) => Route::Op {
                root,
                field: String::from(field_name),
            },
            None => member_route(name, field_name, None),
        };
        let id = ElementId::of_field(model, f);
        let kind = if ty.kind == Kind::Input {
            "input field"
        } else {
            "field"
        };
        out.push(entry(field_name, kind, hash(&route), member_href(&id)));
        for &a in &field.args {
            let Some(id) = ElementId::of_arg(model, a) else {
                continue;
            };
            let arg_name = model.name(model.arg(a).name);
            let route = member_route(name, field_name, Some(arg_name));
            out.push(entry(arg_name, "argument", hash(&route), member_href(&id)));
        }
    }
    for &v in &ty.values {
        let value_name = model.name(model.value(v).name);
        let route = member_route(name, value_name, None);
        let id = ElementId::of_value(model, v);
        out.push(entry(
            value_name,
            "enum value",
            hash(&route),
            member_href(&id),
        ));
    }
}

/// Returns the entry of `name`.
fn entry(name: &str, kind: &'static str, route: String, href: Option<String>) -> Entry {
    Entry {
        name: String::from(name),
        kind,
        route,
        href,
    }
}

/// Returns the route of the type `ty` with the member `member` and the
/// argument `arg` selected.
fn member_route(ty: &str, member: &str, arg: Option<&str>) -> Route {
    Route::Type {
        name: String::from(ty),
        field: Some(String::from(member)),
        arg: arg.map(String::from),
    }
}

/// Returns the hash of `route` under the default options.
fn hash(route: &Route) -> String {
    route.to_hash(&Options::default())
}

/// Returns the link of the element `id` on the page of `route`, the path of
/// the Markdown file with the anchor of the element, or `None` when the tree
/// writes no Markdown.
///
/// A cluster is its page in the split layout and the anchor `cluster-<slug>`
/// of `API.md` in the single layout.
#[cfg(feature = "markdown")]
fn href(site: &Site<'_>, id: &ElementId, route: &Route) -> Option<String> {
    use crate::{
        page::LayoutMode,
        render_markdown::page_path, //
    };

    if !site.md {
        return None;
    }
    let mut out = page_path(site.schema, route, site.layout)?;
    let anchor = match id {
        ElementId::Cluster(slug) if site.layout == LayoutMode::Single => {
            alloc::format!("cluster-{slug}")
        }
        ElementId::Cluster(_) => return Some(out),
        _ => String::from(site.schema.anchors.of(id)),
    };
    if !anchor.is_empty() {
        out.push('#');
        out.push_str(&anchor);
    }
    Some(out)
}

/// Returns no link, since the build carries no Markdown writer.
#[cfg(not(feature = "markdown"))]
fn href(_site: &Site<'_>, _id: &ElementId, _route: &Route) -> Option<String> {
    None
}

/// Returns the first letters of the parts of the search file, which it
/// splits into above [`LETTER_TYPES`](super::LETTER_TYPES) types, in byte
/// order, or none.
pub(super) fn letters(site: &Site<'_>) -> Vec<u8> {
    if !site.splits() {
        return Vec::new();
    }
    letters_of(&entries(site))
}

/// Returns the first letters of the names of `entries`, sorted and without
/// repeats.
fn letters_of(entries: &[Entry]) -> Vec<u8> {
    let mut out: Vec<u8> = entries.iter().map(|e| first_letter(&e.name)).collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// Writes `search-index.json` of `site` from `entries`, or with `letter` its
/// part of that first letter.
pub(super) fn write(site: &Site<'_>, entries: &[Entry], letter: Option<u8>, out: &mut String) {
    // Writing into a `String` cannot fail.
    let _ = write!(out, "{{\"version\":{VERSION},");
    match letter {
        None if site.splits() => write_parts(entries, out),
        None => write_entries(entries.iter(), out),
        Some(letter) => {
            let part = entries.iter().filter(|e| first_letter(&e.name) == letter);
            write_entries(part, out);
        }
    }
    out.push_str("}\n");
}

/// Writes the list of the parts, one per first letter of `entries`.
fn write_parts(entries: &[Entry], out: &mut String) {
    out.push_str("\"parts\":[");
    for (i, letter) in letters_of(entries).into_iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let count = entries
            .iter()
            .filter(|e| first_letter(&e.name) == letter)
            .count();
        let letter = char::from(letter);
        // Writing into a `String` cannot fail.
        let _ = write!(
            out,
            "\n[\"{letter}\",\"search-index/{letter}.json\",{count}]"
        );
    }
    out.push_str("\n]");
}

/// Writes the list of `entries`, one per line.
fn write_entries<'a>(entries: impl Iterator<Item = &'a Entry>, out: &mut String) {
    out.push_str("\"entries\":[");
    let mut any = false;
    for (i, e) in entries.enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("\n[");
        write_str(&e.name, out);
        out.push(',');
        write_str(e.kind, out);
        out.push(',');
        write_str(&e.route, out);
        out.push(',');
        match &e.href {
            Some(href) => write_str(href, out),
            None => out.push_str("null"),
        }
        out.push(']');
        any = true;
    }
    if any {
        out.push('\n');
    }
    out.push(']');
}
