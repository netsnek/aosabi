// SPDX-License-Identifier: MIT
/*
 * aoviewer-core id: the stable element ids and the anchors of the documents.
 *
 * The id grammar of Voyager extended by argument, enum value and cluster ids is
 * what the routes, the SVG ids and the anchors carry. Input field ids belong to
 * the same grammar.
 */

//! The stable ids and the anchors.
//!
//! Voyager writes a type as `TYPE::<type>`, a field as
//! `FIELD::<type>::<field>`, a union member as
//! `POSSIBLE_TYPE::<union>::<member>`, an implementer of an interface as
//! `DERIVED_TYPE::<interface>::<type>`, an implemented interface as
//! `INTERFACE::<type>::<interface>`, and an edge as its source id, ` => ` and
//! the id of the target type. The extension adds `ARG::<type>::<field>::<arg>`,
//! `ENUMVALUE::<enum>::<value>`, `INPUTFIELD::<input>::<field>` and
//! `CLUSTER::<slug>`.
//!
//! Every loop here runs over the elements of one model, whose counts
//! [`ReadLimits`](crate::limits::ReadLimits) caps, over the at most three
//! names of one id, or over the bytes of one id text. Writing and reading an
//! id go one level into an edge and no further.
//!
//! [utils.ts]: https://github.com/APIs-guru/graphql-voyager/blob/656b8ddb4e7bec958d7300e2d737f632c7b5ba30/src/introspection/utils.ts
//! [dot.ts]: https://github.com/APIs-guru/graphql-voyager/blob/656b8ddb4e7bec958d7300e2d737f632c7b5ba30/src/graph/dot.ts

use alloc::{
    boxed::Box,
    string::String,
    vec::Vec, //
};

use core::fmt::Write;

use crate::model::{
    ArgId,
    FieldId,
    Kind,
    Model,
    Name,
    TypeId,
    ValueId, //
};

/// The separator between the source and the target of an edge id.
const EDGE: &str = " => ";

/// A stable id of an element of the schema, of a cluster or of an edge.
///
/// The grammar is the one of `docs/aoviewer-parity.md` row 38, extended by
/// argument, enum value, input field and cluster ids. The text form is what
/// the routes, the SVG ids and the anchors carry. The derived order compares
/// the variant first and then the names in byte order.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ElementId {
    /// A named type, `TYPE::Organization`.
    Type(Name),
    /// A field of an object or an interface,
    /// `FIELD::Organization::projects`.
    Field(Name, Name),
    /// An argument of a field, `ARG::Organization::projects::first`.
    Arg(Name, Name, Name),
    /// A value of an enum, `ENUMVALUE::OrgState::ACTIVE`.
    Value(Name, Name),
    /// A field of an input type, `INPUTFIELD::OrgFilter::state`.
    InputField(Name, Name),
    /// A cluster by its slug, `CLUSTER::organization`.
    Cluster(String),
    /// An edge from an element to the named type it points at.
    ///
    /// A field, an input field or an argument writes its own id as the
    /// source. A type as the source writes Voyager's `POSSIBLE_TYPE`,
    /// `INTERFACE` or `DERIVED_TYPE` form by the relation the model holds
    /// between the two types. An edge as the source is outside the grammar
    /// and writes no source.
    Edge(Box<ElementId>, Name),
}

impl ElementId {
    /// Returns the id of the type `id`.
    ///
    /// # Panics
    ///
    /// Panics when `id` comes from another model.
    #[must_use]
    pub fn of_type(model: &Model, id: TypeId) -> ElementId {
        ElementId::Type(model.ty(id).name)
    }

    /// Returns the id of the field `id`, an input field id when its owner is
    /// an input type.
    ///
    /// # Panics
    ///
    /// Panics when `id` comes from another model or is
    /// [`FieldId::DIRECTIVE`].
    #[must_use]
    pub fn of_field(model: &Model, id: FieldId) -> ElementId {
        let field = model.field(id);
        let owner = model.ty(field.owner);
        if owner.kind == Kind::Input {
            ElementId::InputField(owner.name, field.name)
        } else {
            ElementId::Field(owner.name, field.name)
        }
    }

    /// Returns the id of the argument `id`, or `None` for an argument of a
    /// directive, which the grammar does not name.
    ///
    /// # Panics
    ///
    /// Panics when `id` comes from another model.
    #[must_use]
    pub fn of_arg(model: &Model, id: ArgId) -> Option<ElementId> {
        let arg = model.arg(id);
        if arg.owner == FieldId::DIRECTIVE {
            return None;
        }
        let field = model.field(arg.owner);
        Some(ElementId::Arg(
            model.ty(field.owner).name,
            field.name,
            arg.name,
        ))
    }

    /// Returns the id of the enum value `id`.
    ///
    /// # Panics
    ///
    /// Panics when `id` comes from another model.
    #[must_use]
    pub fn of_value(model: &Model, id: ValueId) -> ElementId {
        let value = model.value(id);
        ElementId::Value(model.ty(value.owner).name, value.name)
    }

    /// Returns the text form of the id, such as `TYPE::Organization`,
    /// `FIELD::Organization::projects`, `ARG::Organization::projects::first`,
    /// `ENUMVALUE::OrgState::ACTIVE` or `CLUSTER::organization`.
    ///
    /// # Panics
    ///
    /// Panics when a name of the id comes from another model.
    #[must_use]
    pub fn text(&self, model: &Model) -> String {
        let mut out = String::new();
        self.write(model, &mut out);
        out
    }

    /// Reads the text form of an id of an element `model` holds.
    ///
    /// Returns `None` for a text outside the grammar, for a name the model
    /// does not carry, and for an element or an edge the model does not
    /// hold. A cluster slug is checked against its grammar alone, because
    /// the clustering is built after the model.
    #[must_use]
    pub fn parse(text: &str, model: &Model) -> Option<ElementId> {
        if let Some((source, target)) = text.split_once(EDGE) {
            return parse_edge(source, target, model);
        }
        let (prefix, rest) = text.split_once("::")?;
        if prefix == "CLUSTER" {
            return is_slug(rest).then(|| ElementId::Cluster(String::from(rest)));
        }
        let mut names: [&str; 3] = [""; 3];
        let count = split_names(rest, &mut names)?;
        match (prefix, count) {
            ("TYPE", 1) => type_named(model, names[0]).map(|(_, t)| ElementId::Type(t)),
            ("FIELD", 2) => {
                let (ty, f) = field_named(model, names[0], names[1], false)?;
                Some(ElementId::Field(ty, model.field(f).name))
            }
            ("INPUTFIELD", 2) => {
                let (ty, f) = field_named(model, names[0], names[1], true)?;
                Some(ElementId::InputField(ty, model.field(f).name))
            }
            ("ARG", 3) => {
                let (ty, f) = field_named(model, names[0], names[1], false)?;
                let field = model.field(f);
                let a = field
                    .args
                    .iter()
                    .find(|&&a| model.name(model.arg(a).name) == names[2])?;
                Some(ElementId::Arg(ty, field.name, model.arg(*a).name))
            }
            ("ENUMVALUE", 2) => {
                let (id, ty) = type_named(model, names[0])?;
                let v = model
                    .ty(id)
                    .values
                    .iter()
                    .find(|&&v| model.name(model.value(v).name) == names[1])?;
                Some(ElementId::Value(ty, model.value(*v).name))
            }
            _ => None,
        }
    }

    /// Appends the text form of the id to `out`.
    ///
    /// The walk goes one level into an edge and no further, so it never
    /// recurses.
    fn write(&self, model: &Model, out: &mut String) {
        let ElementId::Edge(source, target) = self else {
            self.write_element(model, out);
            return;
        };
        match **source {
            ElementId::Type(s) => write_type_edge(model, s, *target, out),
            ref element => element.write_element(model, out),
        }
        out.push_str(EDGE);
        write_names(model, "TYPE", &[*target], out);
    }

    /// Appends the text form of an id that is no edge to `out`.
    fn write_element(&self, model: &Model, out: &mut String) {
        match self {
            ElementId::Type(t) => write_names(model, "TYPE", &[*t], out),
            ElementId::Field(t, f) => write_names(model, "FIELD", &[*t, *f], out),
            ElementId::Arg(t, f, a) => write_names(model, "ARG", &[*t, *f, *a], out),
            ElementId::Value(t, v) => write_names(model, "ENUMVALUE", &[*t, *v], out),
            ElementId::InputField(t, f) => write_names(model, "INPUTFIELD", &[*t, *f], out),
            ElementId::Cluster(slug) => {
                out.push_str("CLUSTER::");
                out.push_str(slug);
            }
            // An edge is no source in the grammar. The source of an edge of
            // an edge writes nothing, so its text starts with ` => ` and
            // `parse` refuses it, as `Anchors::of` answers the empty string
            // for it.
            ElementId::Edge(..) => {}
        }
    }
}

/// Appends `prefix` and each of `names` after `::` to `out`.
fn write_names(model: &Model, prefix: &str, names: &[Name], out: &mut String) {
    out.push_str(prefix);
    for &name in names {
        out.push_str("::");
        out.push_str(model.name(name));
    }
}

/// Appends the source of an edge from the type `source` to the type
/// `target`, in the form of the relation the model holds between them.
fn write_type_edge(model: &Model, source: Name, target: Name, out: &mut String) {
    let prefix = type_relation(model, source, target).unwrap_or("TYPE");
    if prefix == "TYPE" {
        write_names(model, prefix, &[source], out);
    } else {
        write_names(model, prefix, &[source, target], out);
    }
}

/// Returns Voyager's prefix for the relation from the type `source` to the
/// type `target`, a union member, an implemented interface or an
/// implementer, or `None` when the model holds none of them.
fn type_relation(model: &Model, source: Name, target: Name) -> Option<&'static str> {
    let s = model.type_by_name(model.name(source))?;
    let t = model.type_by_name(model.name(target))?;
    let ty = model.ty(s);
    if ty.kind == Kind::Union && ty.members.contains(&t) {
        return Some("POSSIBLE_TYPE");
    }
    if ty.implements.contains(&t) {
        return Some("INTERFACE");
    }
    if model.implementers(s).contains(&t) {
        return Some("DERIVED_TYPE");
    }
    None
}

/// Reads an edge id from its source text and its target text.
fn parse_edge(source: &str, target: &str, model: &Model) -> Option<ElementId> {
    let target = target.strip_prefix("TYPE::")?;
    let (target_id, target_name) = type_named(model, target)?;
    let (prefix, rest) = source.split_once("::")?;
    if matches!(prefix, "POSSIBLE_TYPE" | "INTERFACE" | "DERIVED_TYPE") {
        let (s, t) = rest.split_once("::")?;
        let (_, source_name) = type_named(model, s)?;
        let holds = t == target && type_relation(model, source_name, target_name) == Some(prefix);
        return holds.then(|| ElementId::Edge(Box::new(ElementId::Type(source_name)), target_name));
    }
    let element = ElementId::parse(source, model)?;
    let points = match &element {
        ElementId::Field(t, f) | ElementId::InputField(t, f) => {
            let (_, id) = field_named(
                model,
                model.name(*t),
                model.name(*f),
                prefix == "INPUTFIELD",
            )?;
            model.field(id).type_ref.named() == target_id
        }
        ElementId::Arg(t, f, a) => {
            let (_, id) = field_named(model, model.name(*t), model.name(*f), false)?;
            model
                .field(id)
                .args
                .iter()
                .any(|&x| model.arg(x).name == *a && model.arg(x).type_ref.named() == target_id)
        }
        _ => false,
    };
    points.then(|| ElementId::Edge(Box::new(element), target_name))
}

/// Splits `rest` at `::` into at most three names and returns their count.
fn split_names<'t>(rest: &'t str, names: &mut [&'t str; 3]) -> Option<usize> {
    let mut count = 0;
    for part in rest.split("::") {
        *names.get_mut(count)? = part;
        count += 1;
    }
    Some(count)
}

/// Returns the type called `name` with its interned name.
fn type_named(model: &Model, name: &str) -> Option<(TypeId, Name)> {
    let id = model.type_by_name(name)?;
    Some((id, model.ty(id).name))
}

/// Returns the field `field` of the type `ty`, which is an input type when
/// `input` holds and an object or an interface otherwise.
fn field_named(model: &Model, ty: &str, field: &str, input: bool) -> Option<(Name, FieldId)> {
    let (id, name) = type_named(model, ty)?;
    let owner = model.ty(id);
    if (owner.kind == Kind::Input) != input {
        return None;
    }
    let f = owner
        .fields
        .iter()
        .copied()
        .find(|&f| model.name(model.field(f).name) == field)?;
    Some((name, f))
}

/// Returns whether `text` can be a cluster slug, a nonempty run of
/// `[a-z0-9]` and hyphens.
fn is_slug(text: &str) -> bool {
    !text.is_empty()
        && text
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// The anchors of the documents, one per type, field, input field, argument
/// and enum value.
///
/// An anchor is the text form of the id in lower case with every `::` a hyphen,
/// so `FIELD::Organization::projects` is `field-organization-projects`. A name
/// that only case separates from an earlier one in name order takes `-2`, then
/// `-3`. Names carry no hyphen, so a numbered anchor never meets another
/// element's anchor.
#[derive(Clone, Debug)]
pub struct Anchors {
    /// Every id with its anchor, sorted by the id.
    entries: Vec<(ElementId, String)>,
}

impl Anchors {
    /// Builds the anchor of every type, field, input field, argument and enum
    /// value of `model`.
    #[must_use]
    pub fn build(model: &Model) -> Anchors {
        let mut based: Vec<(String, ElementId)> = element_ids(model)
            .into_iter()
            .map(|id| (base_anchor(model, &id), id))
            .collect();
        based.sort_unstable();
        let mut entries = Vec::with_capacity(based.len());
        let mut run = 1;
        for i in 0..based.len() {
            run = if i > 0 && based[i - 1].0 == based[i].0 {
                run + 1
            } else {
                1
            };
            let (anchor, id) = &based[i];
            let mut anchor = anchor.clone();
            if run > 1 {
                // Writing into a `String` cannot fail.
                let _ = write!(anchor, "-{run}");
            }
            entries.push((id.clone(), anchor));
        }
        entries.sort_unstable_by(|a, b| a.0.cmp(&b.0));
        Anchors { entries }
    }

    /// Returns the anchor of `id`.
    ///
    /// An edge answers the anchor of its source. A cluster, an edge whose
    /// source is an edge, and an id the model of [`Anchors::build`] does not
    /// hold answer the empty string, because a cluster is a page of its own
    /// and has no anchor inside one.
    #[must_use]
    pub fn of(&self, id: &ElementId) -> &str {
        let element = match id {
            ElementId::Edge(source, _) => &**source,
            _ => id,
        };
        self.entries
            .binary_search_by(|(e, _)| e.cmp(element))
            .map_or("", |i| self.entries[i].1.as_str())
    }
}

/// Returns the id of every element that carries an anchor, in schema order.
fn element_ids(model: &Model) -> Vec<ElementId> {
    let mut ids = Vec::new();
    for &t in model.types() {
        let ty = model.ty(t);
        ids.push(ElementId::of_type(model, t));
        for &f in &ty.fields {
            ids.push(ElementId::of_field(model, f));
            ids.extend(
                model
                    .field(f)
                    .args
                    .iter()
                    .filter_map(|&a| ElementId::of_arg(model, a)),
            );
        }
        ids.extend(ty.values.iter().map(|&v| ElementId::of_value(model, v)));
    }
    ids
}

/// Returns the anchor of `id` before a case collision is numbered.
fn base_anchor(model: &Model, id: &ElementId) -> String {
    let mut anchor = id.text(model);
    anchor.make_ascii_lowercase();
    anchor.replace("::", "-")
}
