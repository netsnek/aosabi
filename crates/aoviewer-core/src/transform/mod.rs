// SPDX-License-Identifier: MIT
/*
 * aoviewer-core transform: the transforms between the model and the graph.
 *
 * The alphabetical sort, the Relay collapse, the deprecation rule, the
 * recognition of the twelve scalars of a build and the inherited interface fields
 * run here.
 */

//! The transforms.
//!
//! [`apply`] reads a finished [`Model`] under a set of [`Transforms`] and
//! answers a [`Transformed`], the tables every later stage reads beside the
//! model. The model itself never changes, so the documents list every member
//! the graph hides.
//!
//! Every list of a [`Transformed`] but [`Transformed::order`] is sorted by
//! id, which is schema order, so every lookup is a binary search.
//!
//! Every loop of the transforms runs over the arenas of one model, whose
//! counts [`ReadLimits`](crate::limits::ReadLimits) caps, over the lines of one
//! description, whose length it caps as well, over at most [`MAX_HIDE_GLOBS`]
//! globs, or under the step cap of [`Glob::matches`].

mod builtin;
mod deprecated;
mod glob;
mod inherited;
mod relay;

use alloc::{
    string::String,
    vec::Vec, //
};

pub use builtin::SCALARS;
pub use glob::{
    Glob,
    MAX_GLOB_BYTES, //
};

use crate::model::{
    ArgId,
    FieldId,
    Model,
    Name,
    TypeId,
    TypeRef,
    ValueId, //
};

/// The most globs of `hide=` the transforms read.
///
/// A longer list is cut after this many globs.
pub const MAX_HIDE_GLOBS: usize = 64;

/// The order of the types in the index and the rail, and of the members of a
/// type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortOrder {
    /// `sort=schema`, the order of the schema, which is the author's order.
    Schema,
    /// `sort=alpha`, the byte order of the names.
    Alpha,
}

/// The named types the graph makes nodes of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeSet {
    /// `nodes=all`, every named type but the leaf scalars.
    All,
    /// `nodes=composite`, the objects, interfaces and unions alone.
    Composite,
}

/// The six Voyager flags and the layers, the part of `Options` the transforms
/// read.
///
/// Each field is one key of the option grammar, and [`Transforms::default`]
/// carries the default of every key. [`apply`] reads `relay`, `sort`,
/// `descdep` and `hide`, and [`Transformed::hides`] reads `deprecated`. The
/// graph and the views read `leaf`, `hide_root` and `nodes`.
#[derive(Clone, Debug, PartialEq, Eq)]
// Each flag is one key of the option grammar, so an enum per flag would
// rename the grammar rather than simplify it.
#[expect(clippy::struct_excessive_bools)]
pub struct Transforms {
    /// `relay=1`, the default, collapses every Relay connection into a list
    /// of its node type.
    pub relay: bool,
    /// `deprecated=1` shows deprecated members in the graph. The default,
    /// `deprecated=0`, hides them there, and the documents always list them.
    pub deprecated: bool,
    /// `leaf=1`, the default, keeps the scalar and enum rows of a box.
    pub leaf: bool,
    /// `sort=schema`, the default, or `sort=alpha`.
    pub sort: SortOrder,
    /// `hideRoot=1` removes the root box after the walk. The default is
    /// `hideRoot=0`.
    pub hide_root: bool,
    /// `nodes=all`, the default, or `nodes=composite`.
    pub nodes: NodeSet,
    /// `descdep=1`, the default, lets a description line that starts with
    /// `@deprecated` mark its member.
    pub descdep: bool,
    /// `hide=glob,glob` hides every type one of the globs matches. At most
    /// [`MAX_HIDE_GLOBS`] of them are read.
    pub hide: Vec<Glob>,
}

impl Default for Transforms {
    /// Returns the defaults of the option grammar.
    ///
    /// They are Voyager's display defaults, `skipRelay` and `skipDeprecated`
    /// on, `showLeafFields` on, `sortByAlphabet` and `hideRoot` off, with
    /// every named type but the leaf scalars as a node and the description
    /// tag counted.
    fn default() -> Self {
        Transforms {
            relay: true,
            deprecated: false,
            leaf: true,
            sort: SortOrder::Schema,
            hide_root: false,
            nodes: NodeSet::All,
            descdep: true,
            hide: Vec::new(),
        }
    }
}

/// The model after the transforms, owned so a `Schema` holds it beside the
/// model.
#[derive(Clone, Debug)]
pub struct Transformed {
    /// One entry per collapsed Relay field, sorted by field.
    pub collapsed: Vec<Collapsed>,
    /// The types the graph leaves out, the Relay types the dry run removes
    /// and the types a glob of `hide=` matches, sorted.
    pub hidden: Vec<TypeId>,
    /// Every deprecated member and where its mark comes from, sorted by
    /// member, so the fields come first, then the arguments, then the enum
    /// values, each in schema order.
    pub marks: Vec<(Member, Marked)>,
    /// A field of an object or an interface and the interface it comes from,
    /// sorted by field.
    pub inherited: Vec<(FieldId, TypeId)>,
    /// The built in types, sorted: the five scalars of the specification,
    /// the seven a build adds when the schema declares all twelve
    /// [`SCALARS`], and the introspection types.
    pub builtin: Vec<TypeId>,
    /// Every type in the order the index and the rail use.
    pub order: Vec<TypeId>,
    /// Every Relay type, removed or kept, sorted.
    relay_types: Vec<TypeId>,
    /// The fields of the query root the collapse deletes, sorted.
    dropped: Vec<FieldId>,
    /// The interface the collapse strips from every type.
    node_interface: Option<TypeId>,
    /// The order the member lists answer in.
    sort: SortOrder,
    /// Whether the graph hides the deprecated members.
    hide_deprecated: bool,
}

/// One collapsed Relay field with the wrappers it hides.
///
/// A field that returns a connection names the connection, its edge and the
/// named type of its field `pageInfo`. A field that returns an edge directly,
/// as a large public schema does in its mutations, names the edge, the first
/// connection found with that edge, and no page info type.
///
/// [introspection.ts]: https://github.com/APIs-guru/graphql-voyager/blob/656b8ddb4e7bec958d7300e2d737f632c7b5ba30/src/introspection/introspection.ts
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Collapsed {
    /// The collapsed field.
    pub field: FieldId,
    /// The connection type.
    pub connection: TypeId,
    /// The edge type.
    pub edge: TypeId,
    /// The page info type, when the field returns the connection.
    pub page_info: Option<TypeId>,
    /// The type the field returns after the collapse, such as `[Project!]`
    /// for a connection over `Project!`.
    pub node: TypeRef,
    /// The paging arguments the collapse strips, in schema order.
    pub stripped_args: Vec<ArgId>,
    /// For a field that returns the connection, every field of the
    /// connection but `edges` and `pageInfo`, then every field of the edge
    /// but `node` and `cursor`. For a field that returns an edge directly,
    /// the fields of the edge but `node` and `cursor` alone.
    pub extra_fields: Vec<FieldId>,
}

/// A member of a type that can carry a deprecation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Member {
    /// A field of an object or an interface, or an input field.
    Field(FieldId),
    /// An argument of a field.
    Arg(ArgId),
    /// A value of an enum.
    Value(ValueId),
}

/// Where the deprecation mark of a member comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Marked {
    /// The schema states the deprecation by directive.
    Directive {
        /// The reason the directive gives.
        reason: Option<String>,
    },
    /// A line of the description starts with `@deprecated`, which is the
    /// hollow mark.
    Tag {
        /// The rest of that line, if any.
        reason: Option<String>,
    },
}

/// Applies `transforms` to `model`.
///
/// The Relay collapse runs under `relay`, the description tag counts under
/// `descdep`, the globs of `hide` hide the types they match, and `sort`
/// orders the index. The built in types and the inherited fields are found
/// under every set of transforms.
#[must_use]
pub fn apply(model: &Model, transforms: &Transforms) -> Transformed {
    let relay = if transforms.relay {
        relay::collapse(model)
    } else {
        relay::Relay::default()
    };
    let mut hidden = relay.removed;
    let globs = &transforms.hide[..transforms.hide.len().min(MAX_HIDE_GLOBS)];
    hidden.extend(model.types().iter().copied().filter(|&t| {
        let name = model.name(model.ty(t).name);
        globs.iter().any(|g| g.matches(name))
    }));
    hidden.sort_unstable();
    hidden.dedup();
    let mut order = model.types().to_vec();
    if transforms.sort == SortOrder::Alpha {
        order.sort_by(|&a, &b| crate::sort::name_order(model, a, b));
    }
    Transformed {
        collapsed: relay.collapsed,
        hidden,
        marks: deprecated::marks(model, transforms.descdep),
        inherited: inherited::inherited(model, relay.node),
        builtin: builtin::builtin(model),
        order,
        relay_types: relay.types,
        dropped: relay.dropped,
        node_interface: relay.node,
        sort: transforms.sort,
        hide_deprecated: !transforms.deprecated,
    }
}

impl Transformed {
    /// Returns whether the graph leaves the type `ty` out, because the Relay
    /// collapse removes it or a glob of `hide=` matches it.
    #[must_use]
    pub fn is_hidden(&self, ty: TypeId) -> bool {
        self.hidden.binary_search(&ty).is_ok()
    }

    /// Returns whether `ty` is a built in scalar or an introspection type.
    ///
    /// The five scalars of the GraphQL specification are built in in every
    /// schema, and the seven a build adds when the schema declares all twelve
    /// [`SCALARS`].
    #[must_use]
    pub fn is_builtin(&self, ty: TypeId) -> bool {
        self.builtin.binary_search(&ty).is_ok()
    }

    /// Returns whether the Relay collapse found `ty` as a Relay type, removed
    /// or kept.
    ///
    /// Voyager draws no Relay type as a node, not even one the dry run keeps.
    ///
    /// [type-graph.ts]: https://github.com/APIs-guru/graphql-voyager/blob/656b8ddb4e7bec958d7300e2d737f632c7b5ba30/src/graph/type-graph.ts
    #[must_use]
    pub fn is_relay_type(&self, ty: TypeId) -> bool {
        self.relay_types.binary_search(&ty).is_ok()
    }

    /// Returns whether the field `field` carries the Relay mark, which a
    /// collapsed field does and so does one whose named type is a Relay type.
    ///
    /// # Panics
    ///
    /// Panics when `field` comes from another model.
    ///
    /// [introspection.ts]: https://github.com/APIs-guru/graphql-voyager/blob/656b8ddb4e7bec958d7300e2d737f632c7b5ba30/src/introspection/introspection.ts
    #[must_use]
    pub fn is_relay_field(&self, model: &Model, field: FieldId) -> bool {
        self.collapse_of(field).is_some() || self.is_relay_type(model.field(field).type_ref.named())
    }

    /// Returns the collapse of the field `field`, if the Relay collapse
    /// collapsed it.
    #[must_use]
    pub fn collapse_of(&self, field: FieldId) -> Option<&Collapsed> {
        self.collapsed
            .binary_search_by_key(&field, |c| c.field)
            .ok()
            .map(|i| &self.collapsed[i])
    }

    /// Returns the deprecation mark of `member`, if it carries one.
    #[must_use]
    pub fn mark(&self, member: Member) -> Option<&Marked> {
        self.marks
            .binary_search_by_key(&member, |&(m, _)| m)
            .ok()
            .map(|i| &self.marks[i].1)
    }

    /// Returns the interface the field `field` comes from, if it is
    /// inherited.
    #[must_use]
    pub fn inherited_from(&self, field: FieldId) -> Option<TypeId> {
        self.inherited
            .binary_search_by_key(&field, |&(f, _)| f)
            .ok()
            .map(|i| self.inherited[i].1)
    }

    /// Returns the interface called `Node` that the Relay collapse strips
    /// from the interfaces of every type, if it runs and the schema declares
    /// one.
    #[must_use]
    pub fn node_interface(&self) -> Option<TypeId> {
        self.node_interface
    }

    /// Returns whether the Relay collapse deletes the field `field` of the
    /// query root, a field `relay` returning the root or a field `node` or
    /// `nodes` returning `Node`.
    #[must_use]
    pub fn is_dropped(&self, field: FieldId) -> bool {
        self.dropped.binary_search(&field).is_ok()
    }

    /// Returns whether the graph leaves `member` out.
    ///
    /// A deprecated member is left out under `deprecated=0`, whichever way
    /// it is marked, and its type stays whatever the filter empties. A field
    /// the Relay collapse deletes and an argument it strips are left out as
    /// well.
    ///
    /// # Panics
    ///
    /// Panics when `member` comes from another model.
    #[must_use]
    pub fn hides(&self, model: &Model, member: Member) -> bool {
        if self.hide_deprecated && self.mark(member).is_some() {
            return true;
        }
        match member {
            Member::Field(f) => self.is_dropped(f),
            Member::Arg(a) => {
                let owner = model.arg(a).owner;
                owner != FieldId::DIRECTIVE
                    && self
                        .collapse_of(owner)
                        .is_some_and(|c| c.stripped_args.contains(&a))
            }
            Member::Value(_) => false,
        }
    }

    /// Returns the fields of `ty` in the order of the transforms, schema
    /// order or, under `sort=alpha`, the byte order of their names.
    ///
    /// # Panics
    ///
    /// Panics when `ty` comes from another model.
    #[must_use]
    pub fn fields(&self, model: &Model, ty: TypeId) -> Vec<FieldId> {
        self.arrange(&model.ty(ty).fields, |f| model.field(f).name)
    }

    /// Returns the arguments of the field `field` in the order of the
    /// transforms.
    ///
    /// # Panics
    ///
    /// Panics when `field` comes from another model.
    #[must_use]
    pub fn args(&self, model: &Model, field: FieldId) -> Vec<ArgId> {
        self.arrange(&model.field(field).args, |a| model.arg(a).name)
    }

    /// Returns the values of the enum `ty` in the order of the transforms.
    ///
    /// # Panics
    ///
    /// Panics when `ty` comes from another model.
    #[must_use]
    pub fn values(&self, model: &Model, ty: TypeId) -> Vec<ValueId> {
        self.arrange(&model.ty(ty).values, |v| model.value(v).name)
    }

    /// Returns the members of the union `ty` in the order of the transforms.
    ///
    /// # Panics
    ///
    /// Panics when `ty` comes from another model.
    #[must_use]
    pub fn members(&self, model: &Model, ty: TypeId) -> Vec<TypeId> {
        self.arrange(&model.ty(ty).members, |t| model.ty(t).name)
    }

    /// Returns the interfaces `ty` implements in the order of the
    /// transforms, [`Transformed::node_interface`] included, which the graph
    /// leaves out.
    ///
    /// # Panics
    ///
    /// Panics when `ty` comes from another model.
    #[must_use]
    pub fn implements(&self, model: &Model, ty: TypeId) -> Vec<TypeId> {
        self.arrange(&model.ty(ty).implements, |t| model.ty(t).name)
    }

    /// Returns `items` in the order of the transforms, sorted by `name`
    /// under `sort=alpha`.
    ///
    /// Names are unique within one list, and the tie break by id keeps the
    /// order total all the same.
    fn arrange<T: Copy + Ord>(&self, items: &[T], name: impl Fn(T) -> Name) -> Vec<T> {
        let mut out = items.to_vec();
        if self.sort == SortOrder::Alpha {
            out.sort_by(|&a, &b| name(a).cmp(&name(b)).then(a.cmp(&b)));
        }
        out
    }
}
