// SPDX-License-Identifier: MIT
/*
 * aoviewer-core read write: the writer of the introspection result a model
 * answers.
 *
 * The form it writes is the one the introspection reader reads, so the two
 * round trip, and the schema hash of a manifest is taken over it.
 */

//! The introspection writer.
//!
//! Every loop of the writer runs over an arena or a list of the model, whose
//! length the builder capped by [`ReadLimits`](crate::limits::ReadLimits), or
//! over the wrappers of a type reference, which
//! [`ReadLimits::max_depth`](crate::limits::ReadLimits::max_depth) caps.

use alloc::string::String;

use crate::{
    json::write_str,
    model::{
        Arg,
        Deprecation,
        Field,
        Kind,
        Model,
        TypeId,
        TypeRef, //
    },
};

/// Writes the introspection result the model answers, in the shape
/// [`from_introspection`](super::from_introspection) reads, with argument
/// descriptions where the model carries them, so the two round trip.
///
/// The result is `{"data":{"__schema":..}}`, the answer to
/// [󰀁](super::INTROSPECTION_QUERY) with every key in the order of the
/// selections of the query, no whitespace, and every string in the escapes of
/// `JSON.stringify`, which is how graphql-js answers the same query. A named
/// type nine wrappers deep stands without `ofType`, as the query selects it
/// there. A deeper reference, which graphql-js cuts off, is written in full,
/// so it round trips. The types and the directives stand in schema order. A
/// key a kind does not carry is `null`, the possible types of an interface
/// are the objects that implement it, and every directive argument is written
/// as not deprecated, because the model holds no deprecation for one. The
/// output is appended to `out`.
///
/// For a schema graphql-js builds from SDL, with nine wrappers or fewer on
/// every type reference and no deprecated directive argument, the model that
/// [󰀀](super::from_introspection) reads from graphql-js's answer is written
/// byte equal to that answer. A deprecated directive argument is outside it,
/// because graphql-js answers it as deprecated. A schema built otherwise is
/// outside it, because its text may hold a lone surrogate, which the JSON
/// reader replaces by U+FFFD and which SDL cannot carry.
///
/// [specification]: https://spec.graphql.org/October2021/
pub fn to_introspection(model: &Model, out: &mut String) {
    out.push_str("{\"data\":{\"__schema\":{\"description\":");
    opt_str(model.schema_description(), out);
    let roots = model.roots();
    for (key, root) in [
        ("queryType", roots.query),
        ("mutationType", roots.mutation),
        ("subscriptionType", roots.subscription),
    ] {
        out.push_str(",\"");
        out.push_str(key);
        out.push_str("\":");
        match root {
            Some(id) => {
                out.push_str("{\"name\":");
                write_str(model.name(model.ty(id).name), out);
                out.push_str(",\"kind\":\"");
                out.push_str(model.ty(id).kind.introspection_word());
                out.push_str("\"}");
            }
            None => out.push_str("null"),
        }
    }
    out.push_str(",\"types\":[");
    for (i, &id) in model.types().iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        ty(model, id, out);
    }
    out.push_str("],\"directives\":[");
    for (i, &id) in model.directives().iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let d = model.directive(id);
        out.push_str("{\"name\":");
        write_str(model.name(d.name), out);
        out.push_str(",\"description\":");
        opt_str(d.description.as_deref(), out);
        out.push_str(",\"isRepeatable\":");
        out.push_str(if d.repeatable { "true" } else { "false" });
        out.push_str(",\"locations\":[");
        for (j, location) in d.locations.iter().enumerate() {
            if j > 0 {
                out.push(',');
            }
            write_str(location, out);
        }
        out.push_str("],\"args\":[");
        for (j, &a) in d.args.iter().enumerate() {
            if j > 0 {
                out.push(',');
            }
            arg(model, model.arg(a), out);
        }
        out.push_str("]}");
    }
    out.push_str("]}}}");
}

/// Writes one named type as the fragment `FullType` answers it.
fn ty(model: &Model, id: TypeId, out: &mut String) {
    let t = model.ty(id);
    out.push_str("{\"kind\":\"");
    out.push_str(t.kind.introspection_word());
    out.push_str("\",\"name\":");
    write_str(model.name(t.name), out);
    out.push_str(",\"description\":");
    opt_str(t.description.as_deref(), out);
    out.push_str(",\"specifiedByURL\":");
    opt_str(t.specified_by_url.as_deref(), out);
    let composite = matches!(t.kind, Kind::Object | Kind::Interface);
    out.push_str(",\"fields\":");
    if composite {
        out.push('[');
        for (i, &f) in t.fields.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            field(model, model.field(f), out);
        }
        out.push(']');
    } else {
        out.push_str("null");
    }
    out.push_str(",\"inputFields\":");
    if t.kind == Kind::Input {
        out.push('[');
        for (i, &f) in t.fields.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            input_field(model, model.field(f), out);
        }
        out.push(']');
    } else {
        out.push_str("null");
    }
    out.push_str(",\"interfaces\":");
    if composite {
        named_list(model, t.implements.iter().copied(), out);
    } else {
        out.push_str("null");
    }
    out.push_str(",\"enumValues\":");
    if t.kind == Kind::Enum {
        out.push('[');
        for (i, &v) in t.values.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            let value = model.value(v);
            out.push_str("{\"name\":");
            write_str(model.name(value.name), out);
            out.push_str(",\"description\":");
            opt_str(value.description.as_deref(), out);
            deprecation(value.deprecation.as_ref(), out);
            out.push('}');
        }
        out.push(']');
    } else {
        out.push_str("null");
    }
    out.push_str(",\"possibleTypes\":");
    match t.kind {
        Kind::Union => named_list(model, t.members.iter().copied(), out),
        Kind::Interface => {
            // The possible types of an interface are the objects that
            // implement it, in schema order, as `getPossibleTypes` of
            // graphql-js answers them.
            let objects = model
                .implementers(id)
                .iter()
                .copied()
                .filter(|&o| model.ty(o).kind == Kind::Object);
            named_list(model, objects, out);
        }
        _ => out.push_str("null"),
    }
    out.push('}');
}

/// Writes a field of an object or an interface.
fn field(model: &Model, f: &Field, out: &mut String) {
    out.push_str("{\"name\":");
    write_str(model.name(f.name), out);
    out.push_str(",\"description\":");
    opt_str(f.description.as_deref(), out);
    out.push_str(",\"args\":[");
    for (i, &a) in f.args.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        arg(model, model.arg(a), out);
    }
    out.push_str("],\"type\":");
    type_ref(model, &f.type_ref, out);
    deprecation(f.deprecation.as_ref(), out);
    out.push('}');
}

/// Writes an input field as the fragment `InputValue` answers it.
fn input_field(model: &Model, f: &Field, out: &mut String) {
    input_value(
        model,
        InputValue {
            name: model.name(f.name),
            description: f.description.as_deref(),
            type_ref: &f.type_ref,
            default_value: f.default_value.as_deref(),
            deprecation: f.deprecation.as_ref(),
        },
        out,
    );
}

/// Writes an argument as the fragment `InputValue` answers it.
fn arg(model: &Model, a: &Arg, out: &mut String) {
    input_value(
        model,
        InputValue {
            name: model.name(a.name),
            description: a.description.as_deref(),
            type_ref: &a.type_ref,
            default_value: a.default_value.as_deref(),
            deprecation: a.deprecation.as_ref(),
        },
        out,
    );
}

/// The parts of an argument or an input field the fragment `InputValue`
/// answers.
#[derive(Clone, Copy)]
struct InputValue<'m> {
    name: &'m str,
    description: Option<&'m str>,
    type_ref: &'m TypeRef,
    default_value: Option<&'m str>,
    deprecation: Option<&'m Deprecation>,
}

/// Writes an input value.
fn input_value(model: &Model, v: InputValue<'_>, out: &mut String) {
    out.push_str("{\"name\":");
    write_str(v.name, out);
    out.push_str(",\"description\":");
    opt_str(v.description, out);
    out.push_str(",\"type\":");
    type_ref(model, v.type_ref, out);
    out.push_str(",\"defaultValue\":");
    opt_str(v.default_value, out);
    deprecation(v.deprecation, out);
    out.push('}');
}

/// Writes `isDeprecated` and `deprecationReason`, each after a comma.
fn deprecation(d: Option<&Deprecation>, out: &mut String) {
    out.push_str(",\"isDeprecated\":");
    out.push_str(if d.is_some() { "true" } else { "false" });
    out.push_str(",\"deprecationReason\":");
    opt_str(d.and_then(|d| d.reason.as_deref()), out);
}

/// The level of a type reference at which the fragment `TypeRef` of
/// [󰀁](super::INTROSPECTION_QUERY) selects `kind name` without `ofType`,
/// counted from 0 at the reference itself, which is the tenth level.
const QUERY_LAST_LEVEL: usize = 9;

/// Writes a list of references to named types.
fn named_list(model: &Model, ids: impl Iterator<Item = TypeId>, out: &mut String) {
    out.push('[');
    for (i, id) in ids.enumerate() {
        if i > 0 {
            out.push(',');
        }
        named(model, id, true, out);
    }
    out.push(']');
}

/// Writes a reference to the named type `id` as the fragment `TypeRef`
/// answers it, with `"ofType":null` when `of_type` is set.
fn named(model: &Model, id: TypeId, of_type: bool, out: &mut String) {
    let t = model.ty(id);
    out.push_str("{\"kind\":\"");
    out.push_str(t.kind.introspection_word());
    out.push_str("\",\"name\":");
    write_str(model.name(t.name), out);
    if of_type {
        out.push_str(",\"ofType\":null");
    }
    out.push('}');
}

/// Writes a type reference with its wrappers, one object per wrapper.
///
/// The fragment `TypeRef` selects `ofType` at the levels 0 to 8 and not at
/// [`QUERY_LAST_LEVEL`], so a named type that stands there is written with
/// its kind and name alone, and every other named type carries
/// `"ofType":null`. Every reference graphql-js answers completely, nine
/// wrappers and fewer, therefore comes out byte equal to its answer. A
/// wrapper at level 9 or deeper, which that answer cuts off and
/// `buildClientSchema` refuses as deeper than the query, carries its `ofType`
/// all the same, and so does the named type it ends in, so a model of deeper
/// wrappers round trips.
fn type_ref(model: &Model, r: &TypeRef, out: &mut String) {
    let mut cur = r;
    let mut depth: usize = 0;
    loop {
        match cur {
            TypeRef::Named(id) => {
                named(model, *id, depth != QUERY_LAST_LEVEL, out);
                break;
            }
            TypeRef::List(inner) => {
                out.push_str("{\"kind\":\"LIST\",\"name\":null,\"ofType\":");
                cur = inner;
            }
            TypeRef::NonNull(inner) => {
                out.push_str("{\"kind\":\"NON_NULL\",\"name\":null,\"ofType\":");
                cur = inner;
            }
        }
        depth += 1;
    }
    for _ in 0..depth {
        out.push('}');
    }
}

/// Writes a string or `null`.
fn opt_str(text: Option<&str>, out: &mut String) {
    match text {
        Some(text) => write_str(text, out),
        None => out.push_str("null"),
    }
}
