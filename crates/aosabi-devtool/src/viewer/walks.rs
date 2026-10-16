// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool viewer walks: the operation walks, the counts and the orphans
 * of the three walks of an input.
 */

//! The `walks` operation.
//!
//! The request `{input, options}` builds the schema under the transforms of
//! the option string `options` and answers `nodes`, the nodes of its graph,
//! `composite`, the objects, interfaces and unions among them, and under
//! `walks` one entry per walk, `aoviewer`, `voyager` and `strict`, each with
//! `count`, the nodes it reaches, and `orphans`, the names of those it does
//! not. `implementation_only` names the nodes the walk of aoviewer reaches and
//! the strict walk does not, the types reached only as an implementation.
//! Every list follows the name order of the core, so the order of the types in
//! the input reaches no answer.

use aosabi_json::{
    JsObject,
    JsValue, //
};
use aoviewer_core::{
    graph::{
        NodeId,
        ReachSet, //
    },
    model::Kind,
    schema::Schema,
    sort::name_order, //
};

use super::{
    Stop,
    load,
    names,
    view_options, //
};

/// Answers the operation `walks`.
pub(super) fn run(request: &JsObject) -> Result<JsObject, Stop> {
    let options = view_options(request)?;
    let loaded = load(request)?;
    let schema = Schema::build(loaded.model, options.transforms);
    let (graph, model) = (&schema.graph, &schema.model);
    let composite = graph
        .nodes
        .iter()
        .filter(|n| {
            matches!(
                model.ty(n.ty).kind,
                Kind::Object | Kind::Interface | Kind::Union
            )
        })
        .count();
    let walks = graph.walks();
    let mut each = JsObject::with_capacity(3);
    for (word, set) in [
        ("aoviewer", &walks.aoviewer),
        ("voyager", &walks.voyager),
        ("strict", &walks.strict),
    ] {
        each.insert(word, walk(&schema, set));
    }
    let only: Vec<NodeId> = graph
        .node_ids()
        .filter(|&n| walks.aoviewer.contains(n) && !walks.strict.contains(n))
        .collect();
    let mut payload = JsObject::with_capacity(4);
    payload.insert("nodes", graph.nodes.len() as u64);
    payload.insert("composite", composite as u64);
    payload.insert("walks", each);
    payload.insert("implementation_only", sorted(&schema, &only));
    Ok(payload)
}

/// Returns the count and the orphans of the walk `set`.
fn walk(schema: &Schema, set: &ReachSet) -> JsValue {
    let mut out = JsObject::with_capacity(2);
    out.insert("count", set.count());
    out.insert("orphans", sorted(schema, &set.orphans(&schema.graph)));
    JsValue::Object(out)
}

/// Returns the names of the types of `nodes` in the name order of the core.
fn sorted(schema: &Schema, nodes: &[NodeId]) -> JsValue {
    let model = &schema.model;
    let mut types: Vec<_> = nodes
        .iter()
        .map(|&n| schema.graph.nodes[n.index()].ty)
        .collect();
    types.sort_by(|&a, &b| name_order(model, a, b));
    names(model, &types)
}
