// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool viewer graph: the operation graph, the type graph of an input
 * in the canonical form of the Voyager goldens.
 */

//! The `graph` operation.
//!
//! The request `{input, options, root}` answers `form`, the text
//! [󰀂](aoviewer_core::graph::Graph::voyager_form) writes for the schema built
//! under the transforms of the option string `options`, from the root `root`,
//! `query`, `mutation` or `subscription`, the query root when the request
//! names none. The four option sets of SV0 are option strings of the routes,
//! such as `relay=0&nodes=composite&descdep=0`.
//!
//! Beside the form it answers `relay`, the Relay detection that the same
//! capture judges and step 5 of `51_viewer_core` counts: `connections` and
//! `edges`, the connection and edge types of the collapsed fields, `types`,
//! every type the collapse found, removed or kept, and `candidates`, every
//! other type whose name ends in `Connection` or `Edge` after at least one
//! more character, each with `reason`. The structural test keeps its reasons
//! private, so every reason reads `waiting` until a seam of the core names
//! them. Every list follows the name order of the core.

use aosabi_json::{
    JsObject,
    JsValue, //
};
use aoviewer_core::{
    model::{
        RootKind,
        TypeId, //
    },
    schema::Schema,
    sort::name_order, //
};

use super::{
    Stop,
    fatal,
    load,
    names,
    optional,
    type_name,
    view_options, //
};

/// The suffix of the name of a connection type, which the structural test
/// reads.
const CONNECTION: &str = "Connection";

/// The suffix of the name of an edge type, which the structural test does not
/// read, since it takes the named type of the field `edges`, and which the
/// candidates read beside `Connection`.
const EDGE: &str = "Edge";

/// The reason of a candidate while the structural test keeps its reasons
/// private.
const WAITING: &str = "waiting";

/// Answers the operation `graph`.
pub(super) fn run(request: &JsObject) -> Result<JsObject, Stop> {
    let options = view_options(request)?;
    let root = match optional(request, "root")? {
        None => None,
        Some("query") => Some(RootKind::Query),
        Some("mutation") => Some(RootKind::Mutation),
        Some("subscription") => Some(RootKind::Subscription),
        Some(other) => {
            let text = format!("root takes query, mutation or subscription, not {other:?}");
            return Err(fatal(request, &text));
        }
    };
    let loaded = load(request)?;
    let schema = Schema::build(loaded.model, options.transforms);
    let mut form = String::new();
    schema.graph.voyager_form(&schema, root, &mut form);
    let mut payload = JsObject::with_capacity(2);
    payload.insert("form", form);
    payload.insert("relay", relay(&schema));
    Ok(payload)
}

/// Returns the Relay detection of `schema`.
fn relay(schema: &Schema) -> JsValue {
    let (model, t) = (&schema.model, &schema.transformed);
    let mut connections: Vec<TypeId> = t.collapsed.iter().map(|c| c.connection).collect();
    let mut edges: Vec<TypeId> = t.collapsed.iter().map(|c| c.edge).collect();
    let mut types: Vec<TypeId> = model
        .types()
        .iter()
        .copied()
        .filter(|&ty| t.is_relay_type(ty))
        .collect();
    let mut candidates: Vec<TypeId> = model
        .types()
        .iter()
        .copied()
        .filter(|&ty| !t.is_relay_type(ty) && suffixed(type_name(model, ty)))
        .collect();
    for list in [&mut connections, &mut edges, &mut types, &mut candidates] {
        list.sort_by(|&a, &b| name_order(model, a, b));
        list.dedup();
    }
    let candidates = candidates
        .iter()
        .map(|&ty| {
            let mut entry = JsObject::with_capacity(2);
            entry.insert("name", type_name(model, ty));
            entry.insert("reason", WAITING);
            JsValue::Object(entry)
        })
        .collect();
    let mut out = JsObject::with_capacity(4);
    out.insert("connections", names(model, &connections));
    out.insert("edges", names(model, &edges));
    out.insert("types", names(model, &types));
    out.insert("candidates", JsValue::Array(candidates));
    JsValue::Object(out)
}

/// Returns whether `name` ends in `Connection` or `Edge` after at least one
/// more character.
fn suffixed(name: &str) -> bool {
    [CONNECTION, EDGE]
        .iter()
        .any(|suffix| name.len() > suffix.len() && name.ends_with(suffix))
}
