// SPDX-License-Identifier: MIT
/*
 * aoviewer-core graph voyager: the type graph in the canonical form of the
 * Voyager goldens.
 */

//! The Voyager form.
//!
//! The form is every node as `node <name>` in byte order, then every edge as
//! `edge <from>.<port> -> <to> <kind>` in byte order, each line ended by one
//! line feed and an empty graph an empty text. `<kind>` is `field` with ` relay`
//! appended on a field Voyager marks as a Relay field, `possible_type` for a
//! member of a union and `derived_type` for an implementation of an interface,
//! and `<port>` is the field name of a field edge and the name of the member or
//! the implementation otherwise.
//!
//! The nodes are those Voyager's walk reaches from one root, the query root
//! unless the caller names another, and the root leaves the nodes under
//! `hideRoot=1` once the walk is done. A field edge is written only when its
//! target is a node, while a possible type edge and a derived type edge are
//! written for every member and every implementation the transformed schema
//! holds, a node or not. A Relay type the dry run removes is gone from that
//! schema, and so is a type a glob of `hide=` hides, which Voyager has no option
//! for. `showLeafFields` and `sortByAlphabet` change no line of the form.
//!
//! The loops run over the nodes of one graph with their rows, and over the
//! members and implementations of one model.
//!
//! [type-graph.ts]: https://github.com/APIs-guru/graphql-voyager/blob/656b8ddb4e7bec958d7300e2d737f632c7b5ba30/src/graph/type-graph.ts
//! [dot.ts]: https://github.com/APIs-guru/graphql-voyager/blob/656b8ddb4e7bec958d7300e2d737f632c7b5ba30/src/graph/dot.ts

use alloc::{
    string::String,
    vec::Vec, //
};

use super::walk::{
    Rule,
    walk, //
};
use super::{
    Graph,
    NodeId, //
};
use crate::{
    model::{
        Model,
        RootKind,
        TypeId, //
    },
    schema::Schema,
};

impl Graph {
    /// Writes the type graph in the canonical form of the Voyager goldens.
    ///
    /// The graph is the one of `schema`, whose model, transforms and
    /// transformed tables the form reads beside it. `root` names the root
    /// Voyager's walk starts from, the query root when `None`, and
    /// `schema.transforms.hide_root` leaves that root out after the walk. A
    /// root the schema does not declare, or one that is no node, writes an
    /// empty form. The lines are appended to `out`.
    ///
    /// # Panics
    ///
    /// Panics when `schema` holds another graph than this one.
    pub fn voyager_form(&self, schema: &Schema, root: Option<RootKind>, out: &mut String) {
        let model = &schema.model;
        let Some(seed) = model
            .roots()
            .get(root.unwrap_or(RootKind::Query))
            .and_then(|t| self.node_of(t))
        else {
            return;
        };
        let reach = walk(self, model, &[seed], Rule::Voyager);
        let hidden = schema.transforms.hide_root.then_some(seed);
        let shown = |n: NodeId| reach.contains(n) && Some(n) != hidden;
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        for n in self.node_ids().filter(|&n| shown(n)) {
            nodes.push(["node ", type_name(model, self.nodes[n.index()].ty)].concat());
            self.form_edges(schema, n, &shown, &mut edges);
        }
        nodes.sort_unstable();
        edges.sort_unstable();
        for line in nodes.iter().chain(&edges) {
            out.push_str(line);
            out.push('\n');
        }
    }

    /// Appends the edge lines of the node `n` to `out`, `shown` telling the
    /// nodes of the form.
    fn form_edges(
        &self,
        schema: &Schema,
        n: NodeId,
        shown: &impl Fn(NodeId) -> bool,
        out: &mut Vec<String>,
    ) {
        let (model, t) = (&schema.model, &schema.transformed);
        let node = &self.nodes[n.index()];
        let from = type_name(model, node.ty);
        for row in &node.rows {
            let Some(to) = row.target.filter(|&to| self.node_of(to).is_some_and(shown)) else {
                continue;
            };
            let field = model.name(model.field(row.field).name);
            let relay = t.is_relay_field(model, row.field);
            out.push(edge_line(from, field, type_name(model, to), "field", relay));
        }
        let ty = model.ty(node.ty);
        for &m in ty.members.iter().filter(|&&m| !t.is_hidden(m)) {
            let member = type_name(model, m);
            out.push(edge_line(from, member, member, "possible_type", false));
        }
        for &i in model
            .implementers(node.ty)
            .iter()
            .filter(|&&i| !t.is_hidden(i))
        {
            let implementation = type_name(model, i);
            out.push(edge_line(
                from,
                implementation,
                implementation,
                "derived_type",
                false,
            ));
        }
    }
}

/// Returns the name of the type `ty`.
fn type_name(model: &Model, ty: TypeId) -> &str {
    model.name(model.ty(ty).name)
}

/// Returns the line `edge <from>.<port> -> <to> <kind>`, with ` relay`
/// appended when `relay` holds.
fn edge_line(from: &str, port: &str, to: &str, kind: &str, relay: bool) -> String {
    let flag = if relay { " relay" } else { "" };
    ["edge ", from, ".", port, " -> ", to, " ", kind, flag].concat()
}
