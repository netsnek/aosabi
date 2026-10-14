// SPDX-License-Identifier: MIT
/*
 * aoviewer-docs input build: the build directory, the served schema.graphql
 * with ir.json beside it.
 *
 * The served schema is read through the SDL path, ir.json is checked against it
 * and gives the argument descriptions the SDL leaves out.
 */

//! The build input.
//!
//! [`load`] reads the served `schema.graphql` of a build directory through the
//! SDL path, so every type and field description reaches the model in the
//! reading a client gets through introspection, and reads `ir.json` beside it
//! through `from_ir_json` of the core.
//!
//! The two files agree when they hold the same types that are no `__` type,
//! each of the same kind, with the same fields and input fields, each of the
//! same type expression, and the same arguments by name, each of the same type
//! expression, which are the four things the plan names for the check. Every
//! comparison goes by name, so order is not compared. Neither are descriptions,
//! which the reading of a block string changes, nor default values,
//! deprecations, enum values, union members, interfaces or directives, among
//! which the served schema adds the `@inContext` directive that `ir.json` never
//! holds. A scalar of the specification that the served schema holds and
//! `ir.json` does not list is no difference either, because graphql-js adds
//! `String` and `Boolean` to every schema for its introspection types and takes
//! each of the five from the specification where a field names it undeclared.
//! The first path that differs is refused with `AO-D0005`: the served types in
//! schema order, each with its fields in schema order and each field with its
//! type expression and its arguments, and then the types `ir.json` alone holds,
//! in its order.
//!
//! Then every argument description `ir.json` carries is set into the model with
//! [`Builder::set_arg_description`], because the printer of the reference
//! writes none of them into the SDL. The check pairs each served argument with
//! the argument of `ir.json` of its name, so each description is set by the id
//! of its served argument and costs no search among the arguments of its field.
//!
//! Every loop runs over a list of one of the two models or of the outline of
//! the served schema, whose counts the builder caps, or over the wrappers of
//! one type reference, which [`ReadLimits::max_depth`] caps.
//!
//! [`Builder::set_arg_description`]: aoviewer_core::model::Builder::set_arg_description

use std::path::Path;

use aoviewer_core::{
    limits::ReadLimits,
    model::{
        ArgId,
        FieldId,
        Model,
        TypeId, //
    },
};

use super::{
    IR_FILE, SCHEMA_FILE, read_input, read_ir,
    sdl::{
        self,
        OutlineField,
        OutlineType, //
    },
};
use crate::error::DocsError;

/// The word for what one of the two files lacks at a path.
const ABSENT: &str = "nothing";

/// The first path at which the two files differ.
#[derive(Debug)]
struct Difference {
    /// The path, `Type`, `Type.field` or `Type.field(argument:)`.
    path: String,
    /// What `schema.graphql` holds there.
    served: String,
    /// What `ir.json` holds there.
    ir: String,
}

/// Reads the build directory `dir` into a model under `limits`.
///
/// # Errors
///
/// Returns [`DocsError::Unreadable`] when one of the two files cannot be
/// read, [`DocsError::Sdl`] or [`DocsError::Ir`] when its reader refuses it,
/// [`DocsError::Disagree`] at the first path at which they differ, and
/// [`DocsError::Model`] for a schema the model builder refuses.
pub(super) fn load(dir: &Path, limits: &ReadLimits) -> Result<Model, DocsError> {
    let schema_path = dir.join(SCHEMA_FILE);
    let sdl_read = read_input(&schema_path, true, limits)?;
    let mut served = sdl::read(&schema_path, &sdl_read, limits)?;
    let ir = read_ir(&dir.join(IR_FILE), limits)?;
    let pairs = agree(&served.outline, &ir).map_err(|d| DocsError::Disagree {
        dir: dir.to_path_buf(),
        path: d.path,
        served: d.served,
        ir: d.ir,
    })?;
    for (arg, ir_arg) in pairs {
        if let Some(text) = &ir.arg(ir_arg).description {
            served.builder.set_arg_description(arg, text)?;
        }
    }
    Ok(served.builder.finish()?)
}

/// Checks that the served outline and the model of `ir.json` agree, and
/// returns every served argument with the argument of `ir.json` it agrees
/// with.
fn agree(outline: &[OutlineType], ir: &Model) -> Result<Vec<(ArgId, ArgId)>, Difference> {
    let mut pairs = Vec::new();
    let mut found = 0;
    for t in outline {
        let Some(ir_type) = ir.type_by_name(&t.name) else {
            if t.specified {
                continue;
            }
            return Err(diff(&t.name, t.kind.introspection_word(), ABSENT));
        };
        let ir_kind = ir.ty(ir_type).kind;
        if ir_kind != t.kind {
            return Err(diff(
                &t.name,
                t.kind.introspection_word(),
                ir_kind.introspection_word(),
            ));
        }
        agree_fields(t, ir, ir_type, &mut pairs)?;
        found += 1;
    }
    // The names of either side are unique, so `ir.json` holds a type of its
    // own exactly when it holds more than the served ones it matched.
    if found == ir.types().len() {
        return Ok(pairs);
    }
    let names = sorted(outline.iter().map(|t| (t.name.as_str(), ())));
    for &id in ir.types() {
        let ty = ir.ty(id);
        let name = ir.name(ty.name);
        if find(&names, name).is_none() {
            return Err(diff(name, ABSENT, ty.kind.introspection_word()));
        }
    }
    Ok(pairs)
}

/// Checks the fields of the served type `t` against those of the type
/// `ir_type` of `ir`, and appends the pairs of their arguments to `pairs`.
fn agree_fields(
    t: &OutlineType,
    ir: &Model,
    ir_type: TypeId,
    pairs: &mut Vec<(ArgId, ArgId)>,
) -> Result<(), Difference> {
    let ir_fields = &ir.ty(ir_type).fields;
    let by_name = sorted(ir_fields.iter().map(|&id| (ir.name(ir.field(id).name), id)));
    for f in &t.fields {
        let Some(ir_field) = find(&by_name, &f.name) else {
            let path = format!("{}.{}", t.name, f.name);
            return Err(diff(&path, &f.type_ref.parts().text(), ABSENT));
        };
        agree_field(&t.name, f, ir, ir_field, pairs)?;
    }
    if t.fields.len() == ir_fields.len() {
        return Ok(());
    }
    let served = sorted(t.fields.iter().map(|f| (f.name.as_str(), ())));
    for &id in ir_fields {
        let field = ir.field(id);
        let name = ir.name(field.name);
        if find(&served, name).is_none() {
            let path = format!("{}.{name}", t.name);
            return Err(diff(&path, ABSENT, &field.type_ref.parts(ir).text()));
        }
    }
    Ok(())
}

/// Checks the served field `f` of the type `owner` against the field
/// `ir_field` of `ir`, its type expression first and then its arguments, and
/// appends the pairs of its arguments to `pairs`.
///
/// The path of a difference is written only when there is one.
fn agree_field(
    owner: &str,
    f: &OutlineField,
    ir: &Model,
    ir_field: FieldId,
    pairs: &mut Vec<(ArgId, ArgId)>,
) -> Result<(), Difference> {
    let path = |arg: Option<&str>| match arg {
        Some(arg) => format!("{owner}.{}({arg}:)", f.name),
        None => format!("{owner}.{}", f.name),
    };
    let field = ir.field(ir_field);
    if !f.type_ref.parts().eq(field.type_ref.parts(ir)) {
        let (served, theirs) = (f.type_ref.parts().text(), field.type_ref.parts(ir).text());
        return Err(diff(&path(None), &served, &theirs));
    }
    let by_name = sorted(field.args.iter().map(|&id| (ir.name(ir.arg(id).name), id)));
    for arg in &f.args {
        let Some(ir_arg) = find(&by_name, &arg.name) else {
            return Err(diff(
                &path(Some(&arg.name)),
                &arg.type_ref.parts().text(),
                ABSENT,
            ));
        };
        let theirs = &ir.arg(ir_arg).type_ref;
        if !arg.type_ref.parts().eq(theirs.parts(ir)) {
            let (served, theirs) = (arg.type_ref.parts().text(), theirs.parts(ir).text());
            return Err(diff(&path(Some(&arg.name)), &served, &theirs));
        }
        pairs.push((arg.id, ir_arg));
    }
    if f.args.len() == field.args.len() {
        return Ok(());
    }
    let served = sorted(f.args.iter().map(|arg| (arg.name.as_str(), ())));
    for &id in &field.args {
        let arg = ir.arg(id);
        let name = ir.name(arg.name);
        if find(&served, name).is_none() {
            return Err(diff(
                &path(Some(name)),
                ABSENT,
                &arg.type_ref.parts(ir).text(),
            ));
        }
    }
    Ok(())
}

/// Returns the difference at `path`.
fn diff(path: &str, served: &str, ir: &str) -> Difference {
    Difference {
        path: String::from(path),
        served: String::from(served),
        ir: String::from(ir),
    }
}

/// Returns the pairs of `items` sorted by the bytes of their names, for
/// [`find`].
fn sorted<'a, T>(items: impl Iterator<Item = (&'a str, T)>) -> Vec<(&'a str, T)> {
    let mut items: Vec<_> = items.collect();
    items.sort_unstable_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    items
}

/// Returns the value of the name `name` in the sorted pairs `items`.
fn find<T: Copy>(items: &[(&str, T)], name: &str) -> Option<T> {
    items
        .binary_search_by(|item| item.0.as_bytes().cmp(name.as_bytes()))
        .ok()
        .map(|i| items[i].1)
}
