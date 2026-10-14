// SPDX-License-Identifier: MIT
/*
 * aoviewer-core transform relay: the structural detection of Relay connections
 * with its dry run.
 *
 * The collapse follows Voyager's structural test and its dry run.
 */

//! The Relay collapse.
//!
//! A field of an object or an interface names a connection when its named type
//! is an object whose name ends in `Connection` after at least one more
//! character, that has a field `edges` whose named type is an object and a
//! field `pageInfo`, and whose edge object has a field `node` and a field
//! `cursor`. The connection and its edge become Relay types beside an
//! interface called `Node` and an object called `PageInfo`.
//!
//! A field whose type without its outer non null wrapper is a connection
//! returns a list of the type of the edge's `node` field, and a field that
//! returns an edge that way returns the type of the edge's `node` field
//! without its outer non null wrapper. The outer non null wrapper of the field
//! stays in both cases, and the arguments `first`, `last`, `before` and
//! `after` are stripped. A type that is found both as a connection and as an
//! edge keeps the part it was found in last, because both parts are entries of
//! one map. The query root loses a field `relay` that returns the query root
//! and a field `node` or `nodes` whose named type is `Node`, and every type
//! loses `Node` from its interfaces.
//!
//! The dry run applies these changes to every type that is no Relay type and
//! collects the types each changed type names directly: the members of a
//! union, the interfaces of an object or an interface with the named types of
//! its fields and of their arguments, and the named types of the fields of an
//! input type. A Relay type that none of them names is removed, and a Relay
//! type still named stays.
//!
//! Every loop runs over the types or the fields of one model, whose counts
//! [`ReadLimits`](crate::limits::ReadLimits) caps, or over the arguments of
//! one field.
//!
//! [introspection.ts]: https://github.com/APIs-guru/graphql-voyager/blob/656b8ddb4e7bec958d7300e2d737f632c7b5ba30/src/introspection/introspection.ts
//! [collect-referenced-types.ts]: https://github.com/APIs-guru/graphql-voyager/blob/656b8ddb4e7bec958d7300e2d737f632c7b5ba30/src/utils/collect-referenced-types.ts

use alloc::{
    boxed::Box,
    vec,
    vec::Vec, //
};

use super::Collapsed;
use crate::model::{
    ArgId,
    FieldId,
    Kind,
    Model,
    Name,
    TypeId,
    TypeRef, //
};

/// The suffix of the name of a connection type.
const CONNECTION: &str = "Connection";

/// The names of the arguments a collapse strips, which page through a
/// connection.
const PAGING: [&str; 4] = ["first", "last", "before", "after"];

/// What the collapse finds in one model.
#[derive(Debug, Default)]
pub(super) struct Relay {
    /// One entry per collapsed field of a type that stays, sorted by field.
    pub(super) collapsed: Vec<Collapsed>,
    /// Every Relay type, removed or kept, sorted.
    pub(super) types: Vec<TypeId>,
    /// The Relay types the dry run removes, sorted.
    pub(super) removed: Vec<TypeId>,
    /// The fields of the query root the collapse deletes, sorted.
    pub(super) dropped: Vec<FieldId>,
    /// The interface called `Node`, which every type loses from its
    /// interfaces.
    pub(super) node: Option<TypeId>,
}

/// The fields of one type the structural test reads, by name.
#[derive(Clone, Copy, Debug, Default)]
struct Probe {
    edges: Option<FieldId>,
    page_info: Option<FieldId>,
    node: Option<FieldId>,
    cursor: Option<FieldId>,
}

/// The part a type plays after the structural test.
#[derive(Clone, Copy, Debug)]
enum Role {
    /// The type is neither a connection nor an edge.
    Plain,
    /// The type is a connection with its edge, the named type of its field
    /// `pageInfo`, and the field `node` of its edge.
    Connection {
        edge: TypeId,
        page_info: TypeId,
        node: FieldId,
    },
    /// The type is an edge with the first connection found with it and its
    /// field `node`.
    Edge { connection: TypeId, node: FieldId },
}

/// A model with the tables of its collapse, each indexed by type.
struct Collapse<'m> {
    /// The model the collapse reads.
    model: &'m Model,
    /// The fields the structural test reads, per type.
    probes: Vec<Probe>,
    /// The part the structural test gives each type.
    roles: Vec<Role>,
    /// Whether each type is a Relay type.
    relay: Vec<bool>,
    /// The interface called `Node`, if the schema declares one.
    node: Option<TypeId>,
    /// The fields of the query root the collapse deletes, sorted.
    dropped: Vec<FieldId>,
    /// The interned names of [`PAGING`], where the model carries them.
    paging: [Option<Name>; 4],
}

/// Runs the Relay collapse over `model`.
pub(super) fn collapse(model: &Model) -> Relay {
    let mut c = Collapse::new(model);
    c.detect();
    c.drop_root_fields();
    let referenced = c.dry_run();
    c.finish(&referenced)
}

/// Returns whether the type `t` has fields that can name a connection, which
/// objects and interfaces do.
fn has_output_fields(model: &Model, t: TypeId) -> bool {
    matches!(model.ty(t).kind, Kind::Object | Kind::Interface)
}

/// Returns `type_ref` without its outer non null wrapper.
fn nullable(type_ref: &TypeRef) -> &TypeRef {
    match type_ref {
        TypeRef::NonNull(inner) => inner,
        other => other,
    }
}

/// Returns the fields `edges`, `pageInfo`, `node` and `cursor` of every type
/// of `model`, indexed by type.
fn probes(model: &Model) -> Vec<Probe> {
    let [edges, page_info, node, cursor] =
        ["edges", "pageInfo", "node", "cursor"].map(|name| model.find_name(name));
    let mut probes = vec![Probe::default(); model.types().len()];
    for &t in model.types() {
        let probe = &mut probes[t.index()];
        for &f in &model.ty(t).fields {
            let name = Some(model.field(f).name);
            if name == edges {
                probe.edges = Some(f);
            } else if name == page_info {
                probe.page_info = Some(f);
            } else if name == node {
                probe.node = Some(f);
            } else if name == cursor {
                probe.cursor = Some(f);
            }
        }
    }
    probes
}

impl<'m> Collapse<'m> {
    /// Builds the empty tables of `model` and marks `Node` and `PageInfo` as
    /// Relay types where the schema declares them with the right kind.
    fn new(model: &'m Model) -> Self {
        let count = model.types().len();
        let node = model
            .type_by_name("Node")
            .filter(|&t| model.ty(t).kind == Kind::Interface);
        let page_info = model
            .type_by_name("PageInfo")
            .filter(|&t| model.ty(t).kind == Kind::Object);
        let mut relay = vec![false; count];
        for t in node.into_iter().chain(page_info) {
            relay[t.index()] = true;
        }
        Collapse {
            model,
            probes: probes(model),
            roles: vec![Role::Plain; count],
            relay,
            node,
            dropped: Vec::new(),
            paging: PAGING.map(|name| model.find_name(name)),
        }
    }

    /// Runs the structural test on the named type of every field of every
    /// object and interface, in schema order.
    fn detect(&mut self) {
        let model = self.model;
        for &t in model.types() {
            if !has_output_fields(model, t) {
                continue;
            }
            for &f in &model.ty(t).fields {
                let connection = model.field(f).type_ref.named();
                if let Some(role) = self.test(connection) {
                    self.found(connection, role);
                }
            }
        }
    }

    /// Returns the part of a connection the type `connection` passes the
    /// structural test with, or `None` when it fails.
    fn test(&self, connection: TypeId) -> Option<Role> {
        let model = self.model;
        let ty = model.ty(connection);
        let name = model.name(ty.name);
        if ty.kind != Kind::Object || name.len() <= CONNECTION.len() || !name.ends_with(CONNECTION)
        {
            return None;
        }
        let probe = self.probes[connection.index()];
        let edge = model.field(probe.edges?).type_ref.named();
        let page_info = model.field(probe.page_info?).type_ref.named();
        if model.ty(edge).kind != Kind::Object {
            return None;
        }
        let edge_probe = self.probes[edge.index()];
        edge_probe.cursor?;
        Some(Role::Connection {
            edge,
            page_info,
            node: edge_probe.node?,
        })
    }

    /// Records a connection and its edge as Relay types with their parts.
    fn found(&mut self, connection: TypeId, role: Role) {
        let Role::Connection { edge, node, .. } = role else {
            return;
        };
        self.relay[connection.index()] = true;
        self.relay[edge.index()] = true;
        self.roles[connection.index()] = role;
        let first = if let Role::Edge { connection: c, .. } = self.roles[edge.index()] {
            c
        } else {
            connection
        };
        self.roles[edge.index()] = Role::Edge {
            connection: first,
            node,
        };
    }

    /// Records the fields `relay`, `node` and `nodes` of the query root that
    /// the collapse deletes.
    fn drop_root_fields(&mut self) {
        let model = self.model;
        let Some(query) = model.roots().query else {
            return;
        };
        for &f in &model.ty(query).fields {
            let field = model.field(f);
            let named = field.type_ref.named();
            let deleted = match model.name(field.name) {
                "relay" => named == query,
                "node" | "nodes" => Some(named) == self.node,
                _ => false,
            };
            if deleted {
                self.dropped.push(f);
            }
        }
        self.dropped.sort_unstable();
    }

    /// Returns whether the collapse deletes the field `f`.
    fn is_dropped(&self, f: FieldId) -> bool {
        self.dropped.binary_search(&f).is_ok()
    }

    /// Returns the collapse of the field `f`, or `None` when its type without
    /// the outer non null wrapper is neither a connection nor an edge.
    fn collapsed(&self, f: FieldId) -> Option<Collapsed> {
        let model = self.model;
        let field = model.field(f);
        let &TypeRef::Named(target) = nullable(&field.type_ref) else {
            return None;
        };
        let (connection, edge, page_info, node) = match self.roles[target.index()] {
            Role::Plain => return None,
            Role::Connection {
                edge,
                page_info,
                node,
            } => (target, edge, Some(page_info), node),
            Role::Edge { connection, node } => (connection, target, None, node),
        };
        let node_ref = &model.field(node).type_ref;
        let mut collapsed = if page_info.is_some() {
            TypeRef::List(Box::new(node_ref.clone()))
        } else {
            nullable(node_ref).clone()
        };
        if matches!(field.type_ref, TypeRef::NonNull(_)) {
            collapsed = TypeRef::NonNull(Box::new(collapsed));
        }
        Some(Collapsed {
            field: f,
            connection,
            edge,
            page_info,
            node: collapsed,
            stripped_args: self.stripped(&field.args),
            extra_fields: self.extras(page_info.map(|_| connection), edge),
        })
    }

    /// Returns the arguments among `args` a collapse strips, in schema order.
    fn stripped(&self, args: &[ArgId]) -> Vec<ArgId> {
        let model = self.model;
        args.iter()
            .copied()
            .filter(|&a| self.paging.contains(&Some(model.arg(a).name)))
            .collect()
    }

    /// Returns the fields a collapse hides beyond the Relay structure, those
    /// of `connection`, when given, but `edges` and `pageInfo`, then those of
    /// `edge` but `node` and `cursor`, each in schema order.
    fn extras(&self, connection: Option<TypeId>, edge: TypeId) -> Vec<FieldId> {
        let mut out = Vec::new();
        if let Some(c) = connection {
            let p = self.probes[c.index()];
            out.extend(self.fields_but(c, [p.edges, p.page_info]));
        }
        let p = self.probes[edge.index()];
        out.extend(self.fields_but(edge, [p.node, p.cursor]));
        out
    }

    /// Returns the fields of `t` in schema order but the two of `structure`.
    fn fields_but(
        &self,
        t: TypeId,
        structure: [Option<FieldId>; 2],
    ) -> impl Iterator<Item = FieldId> + use<'_, 'm> {
        self.model
            .ty(t)
            .fields
            .iter()
            .copied()
            .filter(move |&f| !structure.contains(&Some(f)))
    }

    /// Returns, indexed by type, whether a type that is no Relay type names
    /// it directly once the collapse has changed it.
    fn dry_run(&self) -> Vec<bool> {
        let model = self.model;
        let mut referenced = vec![false; model.types().len()];
        for &t in model.types() {
            if !self.relay[t.index()] {
                self.refer(t, &mut referenced);
            }
        }
        referenced
    }

    /// Marks in `referenced` every type the changed type `t` names directly.
    fn refer(&self, t: TypeId, referenced: &mut [bool]) {
        let model = self.model;
        let ty = model.ty(t);
        match ty.kind {
            Kind::Union => {
                for &m in &ty.members {
                    referenced[m.index()] = true;
                }
            }
            Kind::Input => {
                for &f in &ty.fields {
                    referenced[model.field(f).type_ref.named().index()] = true;
                }
            }
            Kind::Object | Kind::Interface => {
                for &i in ty.implements.iter().filter(|&&i| Some(i) != self.node) {
                    referenced[i.index()] = true;
                }
                for &f in ty.fields.iter().filter(|&&f| !self.is_dropped(f)) {
                    self.refer_field(f, referenced);
                }
            }
            Kind::Enum | Kind::Scalar => {}
        }
    }

    /// Marks in `referenced` the named type of the changed field `f` and of
    /// every argument it keeps.
    fn refer_field(&self, f: FieldId, referenced: &mut [bool]) {
        let model = self.model;
        let field = model.field(f);
        let collapsed = self.collapsed(f);
        let (named, stripped) = match &collapsed {
            Some(c) => (c.node.named(), c.stripped_args.as_slice()),
            None => (field.type_ref.named(), &[][..]),
        };
        referenced[named.index()] = true;
        for &a in field.args.iter().filter(|&&a| !stripped.contains(&a)) {
            referenced[model.arg(a).type_ref.named().index()] = true;
        }
    }

    /// Splits the Relay types into kept and removed ones by `referenced`, and
    /// collapses the fields of every object and interface that stays.
    fn finish(self, referenced: &[bool]) -> Relay {
        let model = self.model;
        let mut types: Vec<TypeId> = model
            .types()
            .iter()
            .copied()
            .filter(|t| self.relay[t.index()])
            .collect();
        types.sort_unstable();
        let removed: Vec<TypeId> = types
            .iter()
            .copied()
            .filter(|t| !referenced[t.index()])
            .collect();
        let mut collapsed = Vec::new();
        for &t in model.types() {
            if !has_output_fields(model, t) || removed.binary_search(&t).is_ok() {
                continue;
            }
            collapsed.extend(
                model
                    .ty(t)
                    .fields
                    .iter()
                    .filter(|&&f| !self.is_dropped(f))
                    .filter_map(|&f| self.collapsed(f)),
            );
        }
        collapsed.sort_unstable_by_key(|c| c.field);
        Relay {
            collapsed,
            types,
            removed,
            dropped: self.dropped,
            node: self.node,
        }
    }
}
