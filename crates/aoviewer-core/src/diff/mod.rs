// SPDX-License-Identifier: MIT
/*
 * aoviewer-core diff: the differences between two models, keyed by the stable
 * id.
 *
 * A rename is two entries, and the change log of --since reads the list in the
 * names the viewer extends.
 */

//! The schema difference.
//!
//! [`diff`] compares an older model with a newer one element by element. An
//! element is a named type, a field, an input field, an argument of a field,
//! an enum value, a directive or an argument of a directive, and each is keyed
//! by the text of its stable id, such as `FIELD::Item::count`. An id only the
//! newer model holds is [`Change::Added`], an id only the older one holds is
//! [`Change::Removed`], and an id both hold is [`Change::Changed`] once per
//! [`Aspect`] that differs. A renamed element is therefore removed under its
//! old id and added under its new one, because the comparison guesses nothing.
//! An added or removed type lists its fields, arguments and values as added or
//! removed too, since each has an id of its own.
//!
//! The grammar of `crate::id` names no directive, so a directive is keyed as
//! `DIRECTIVE::<directive>` and an argument of a directive as
//! `DIRECTIVEARG::<directive>::<argument>`, in the form of the prefixes the
//! grammar adds to Voyager's.
//!
//! The comparison covers the elements a schema defines, the ones the SDL
//! printer of graphql-js prints. It leaves out the introspection types, every
//! type whose name starts with `__`, the five scalars of the specification and
//! the five directives graphql-js specifies. Every schema holds them, while
//! one reader keeps them and another does not, as the reader of `ir.json`
//! keeps no introspection type, so they would add differences of the reader
//! and none of the schema. The aspects follow the comparison of graphql-js by
//! name, so the interfaces of a type, the members of a union and the locations
//! of a directive differ as sets and not by their order. The roots of the
//! schema, its description and the `specifiedByURL` of a scalar carry no
//! aspect, so they are not compared.
//!
//! Every loop runs over the elements of one model, whose counts
//! [`ReadLimits`](crate::limits::ReadLimits) caps, over the two lists of
//! elements together, each step taking at least one element off them, over
//! the interfaces, members or locations of one element, over the nine
//! aspects of [`Aspect`] of one element, or over the wrappers of two type
//! references, which the limits cap as well.

mod changes;

pub use changes::changes_page;

use alloc::{
    format,
    string::String,
    vec::Vec, //
};
use core::cmp::Ordering;

use crate::{
    id::ElementId,
    model::{
        ArgId,
        Deprecation,
        DirectiveId,
        FieldId,
        Kind,
        Model,
        TypeId,
        TypeRef,
        ValueId, //
    },
};

/// How an element differs between the two models.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Change {
    /// Only the newer model holds the element.
    Added,
    /// Only the older model holds the element.
    Removed,
    /// Both models hold the element, and one of its aspects differs.
    Changed,
}

/// What changed on an element that both schemas carry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Aspect {
    /// The kind of a named type, such as an object that became an interface.
    Kind,
    /// The type of a field, an input field or an argument, its wrappers
    /// included, so a change of nullability is one.
    Type,
    /// The description.
    Description,
    /// The deprecation the schema states by directive, or its reason.
    Deprecation,
    /// The default value of an input field or an argument.
    DefaultValue,
    /// The interfaces an object or an interface implements.
    Implements,
    /// The members of a union.
    Members,
    /// Whether a directive may repeat at one location.
    Repeatable,
    /// The locations of a directive.
    Locations,
}

/// One difference between two models, keyed by the stable id text.
///
/// A rename is two entries. The derived order compares the id text by its
/// bytes, then the change, then the aspect, which is the order [`diff`]
/// writes.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Difference {
    /// The text of the stable id of the element, such as
    /// `FIELD::Item::count`.
    pub id: String,
    /// How the element differs.
    pub change: Change,
    /// The aspect that differs, set for [`Change::Changed`] alone.
    pub aspect: Option<Aspect>,
}

/// The prefix the specification reserves for the introspection types.
///
/// [specification]: https://spec.graphql.org/October2021/
const INTROSPECTION: &str = "__";

/// The scalars every schema has by the specification.
const SPECIFIED_SCALARS: [&str; 5] = ["Int", "Float", "String", "Boolean", "ID"];

/// The directives graphql-js gives every schema.
const SPECIFIED_DIRECTIVES: [&str; 5] = ["include", "skip", "deprecated", "specifiedBy", "oneOf"];

/// An element of one model that the comparison keys by the text of its id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Element {
    /// A named type.
    Type(TypeId),
    /// A field of an object or an interface, or an input field.
    Field(FieldId),
    /// An argument of a field.
    Arg(ArgId),
    /// A value of an enum.
    Value(ValueId),
    /// A directive definition.
    Directive(DirectiveId),
    /// An argument of a directive.
    DirectiveArg(ArgId),
}

/// What the comparison reads of one element, empty where its kind has no
/// such aspect.
#[derive(Debug, Default)]
struct Facts<'m> {
    /// The kind of a named type.
    kind: Option<Kind>,
    /// The type of a field, an input field or an argument.
    type_ref: Option<&'m TypeRef>,
    /// The description.
    description: Option<&'m str>,
    /// The deprecation by directive.
    deprecation: Option<&'m Deprecation>,
    /// The default value.
    default_value: Option<&'m str>,
    /// The names of the implemented interfaces, sorted and without repeats.
    implements: Vec<&'m str>,
    /// The names of the members of a union, sorted and without repeats.
    members: Vec<&'m str>,
    /// Whether a directive is repeatable.
    repeatable: bool,
    /// The locations of a directive, sorted and without repeats.
    locations: Vec<&'m str>,
}

/// Appends every difference between `old` and `new` to `out`, in the order of
/// the ids.
///
/// The elements of both models are sorted by the text of their ids and walked
/// together, so the differences come out in the byte order of the id text, and
/// the changed aspects of one element in the order of [`Aspect`]. The builder
/// refuses two elements of one scope with one name, so no id text stands twice
/// in one model.
pub fn diff(old: &Model, new: &Model, out: &mut Vec<Difference>) {
    let olds = elements(old);
    let news = elements(new);
    let steps = olds.len() + news.len();
    let mut olds = olds.into_iter().peekable();
    let mut news = news.into_iter().peekable();
    for _ in 0..steps {
        let order = match (olds.peek(), news.peek()) {
            (Some((a, _)), Some((b, _))) => a.cmp(b),
            (Some(_), None) => Ordering::Less,
            (None, _) => Ordering::Greater,
        };
        let pair = match order {
            Ordering::Less => (olds.next(), None),
            Ordering::Greater => (None, news.next()),
            Ordering::Equal => (olds.next(), news.next()),
        };
        match pair {
            (Some((id, _)), None) => push(out, id, Change::Removed, None),
            (None, Some((id, _))) => push(out, id, Change::Added, None),
            (Some((id, a)), Some((_, b))) => push_changed(old, a, new, b, &id, out),
            (None, None) => break,
        }
    }
}

/// Appends one difference.
fn push(out: &mut Vec<Difference>, id: String, change: Change, aspect: Option<Aspect>) {
    out.push(Difference { id, change, aspect });
}

/// Appends one [`Change::Changed`] for each aspect in which the element `a`
/// of `old` and the element `b` of `new`, both keyed by `id`, differ.
fn push_changed(
    old: &Model,
    a: Element,
    new: &Model,
    b: Element,
    id: &str,
    out: &mut Vec<Difference>,
) {
    let (fa, fb) = (facts(old, a), facts(new, b));
    let same_type = match (fa.type_ref, fb.type_ref) {
        (Some(x), Some(y)) => x.parts(old).eq(y.parts(new)),
        (x, y) => x.is_none() && y.is_none(),
    };
    let differs = [
        (Aspect::Kind, fa.kind != fb.kind),
        (Aspect::Type, !same_type),
        (Aspect::Description, fa.description != fb.description),
        (Aspect::Deprecation, fa.deprecation != fb.deprecation),
        (Aspect::DefaultValue, fa.default_value != fb.default_value),
        (Aspect::Implements, fa.implements != fb.implements),
        (Aspect::Members, fa.members != fb.members),
        (Aspect::Repeatable, fa.repeatable != fb.repeatable),
        (Aspect::Locations, fa.locations != fb.locations),
    ];
    for (aspect, _) in differs.iter().filter(|(_, d)| *d) {
        push(out, String::from(id), Change::Changed, Some(*aspect));
    }
}

/// Returns every element of `model` the comparison covers with the text of
/// its id, sorted by that text.
fn elements(model: &Model) -> Vec<(String, Element)> {
    let mut out = Vec::new();
    for &t in model.types() {
        if !defined(model, t) {
            continue;
        }
        let ty = model.ty(t);
        out.push((ElementId::of_type(model, t).text(model), Element::Type(t)));
        for &f in &ty.fields {
            out.push((ElementId::of_field(model, f).text(model), Element::Field(f)));
            out.extend(model.field(f).args.iter().filter_map(|&a| {
                let id = ElementId::of_arg(model, a)?;
                Some((id.text(model), Element::Arg(a)))
            }));
        }
        for &v in &ty.values {
            out.push((ElementId::of_value(model, v).text(model), Element::Value(v)));
        }
    }
    for &d in model.directives() {
        let directive = model.directive(d);
        let name = model.name(directive.name);
        if SPECIFIED_DIRECTIVES.contains(&name) {
            continue;
        }
        out.push((format!("DIRECTIVE::{name}"), Element::Directive(d)));
        for &a in &directive.args {
            let arg = model.name(model.arg(a).name);
            out.push((
                format!("DIRECTIVEARG::{name}::{arg}"),
                Element::DirectiveArg(a),
            ));
        }
    }
    out.sort_unstable();
    out
}

/// Returns whether the schema defines the type `t` itself, which holds for
/// every type but the introspection types and the scalars of the
/// specification.
fn defined(model: &Model, t: TypeId) -> bool {
    let ty = model.ty(t);
    let name = model.name(ty.name);
    let specified = ty.kind == Kind::Scalar && SPECIFIED_SCALARS.contains(&name);
    !specified && !name.starts_with(INTROSPECTION)
}

/// Returns what the comparison reads of the element `e` of `model`.
fn facts(model: &Model, e: Element) -> Facts<'_> {
    match e {
        Element::Type(t) => {
            let ty = model.ty(t);
            Facts {
                kind: Some(ty.kind),
                description: ty.description.as_deref(),
                implements: type_names(model, &ty.implements),
                members: type_names(model, &ty.members),
                ..Facts::default()
            }
        }
        Element::Field(f) => {
            let field = model.field(f);
            Facts {
                type_ref: Some(&field.type_ref),
                description: field.description.as_deref(),
                deprecation: field.deprecation.as_ref(),
                default_value: field.default_value.as_deref(),
                ..Facts::default()
            }
        }
        Element::Arg(a) | Element::DirectiveArg(a) => {
            let arg = model.arg(a);
            Facts {
                type_ref: Some(&arg.type_ref),
                description: arg.description.as_deref(),
                deprecation: arg.deprecation.as_ref(),
                default_value: arg.default_value.as_deref(),
                ..Facts::default()
            }
        }
        Element::Value(v) => {
            let value = model.value(v);
            Facts {
                description: value.description.as_deref(),
                deprecation: value.deprecation.as_ref(),
                ..Facts::default()
            }
        }
        Element::Directive(d) => {
            let directive = model.directive(d);
            let mut locations: Vec<&str> = directive.locations.iter().map(String::as_str).collect();
            locations.sort_unstable();
            locations.dedup();
            Facts {
                description: directive.description.as_deref(),
                repeatable: directive.repeatable,
                locations,
                ..Facts::default()
            }
        }
    }
}

/// Returns the names of the types `types`, sorted by their bytes and without
/// repeats.
fn type_names<'m>(model: &'m Model, types: &[TypeId]) -> Vec<&'m str> {
    let mut names: Vec<&str> = types
        .iter()
        .map(|&t| model.name(model.ty(t).name))
        .collect();
    names.sort_unstable();
    names.dedup();
    names
}
