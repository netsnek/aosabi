// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool viewer schema: the operation schema, the model of an input as
 * canonical JSON with its introspection form.
 *
 * An input path is read through the loader of aoviewer-docs, so the seam and
 * the command share one reader.
 */

//! The `schema` operation.
//!
//! The request `{input, form}` answers `kind`, the word of the input kind,
//! `sha256`, the SHA-256 of the canonical introspection form, which is the
//! schema hash of `manifest.json`, `counts`, and the form itself as
//! `introspection` unless `form` is false. The form is the canonical JSON of
//! the model: one model writes one text and the reader reads it back to the
//! same model, so step 3 of `51_viewer_core` compares two models by their
//! forms and runs the round trip through a file that holds one. Leaving the
//! form out keeps the answer to a mutated document of the hostile corpus a few
//! bytes long whatever the size of the document.

use aosabi_json::JsObject;
use aoviewer_core::{
    model::{
        Kind,
        Model, //
    },
    read::to_introspection,
    sha256::sha256_hex, //
};

use super::{
    Stop,
    flag,
    load, //
};

/// Answers the operation `schema`.
pub(super) fn run(request: &JsObject) -> Result<JsObject, Stop> {
    let form = flag(request, "form", true)?;
    let loaded = load(request)?;
    let mut text = String::new();
    to_introspection(&loaded.model, &mut text);
    let mut payload = JsObject::with_capacity(4);
    payload.insert("kind", loaded.kind);
    payload.insert("sha256", sha256_hex(text.as_bytes()));
    payload.insert("counts", counts(&loaded.model));
    if form {
        payload.insert("introspection", text);
    }
    Ok(payload)
}

/// Returns the counts of the model: the types by kind, the fields, the input
/// fields, the arguments of fields, the enum values and the directives.
fn counts(model: &Model) -> JsObject {
    let mut kinds = [0u32; 6];
    let (mut fields, mut input_fields, mut args, mut values) = (0u32, 0u32, 0u32, 0u32);
    for &t in model.types() {
        let ty = model.ty(t);
        kinds[kind_index(ty.kind)] += 1;
        let n = count(ty.fields.len());
        if ty.kind == Kind::Input {
            input_fields += n;
        } else {
            fields += n;
        }
        for &f in &ty.fields {
            args += count(model.field(f).args.len());
        }
        values += count(ty.values.len());
    }
    let mut out = JsObject::with_capacity(11);
    out.insert("types", count(model.types().len()));
    for (word, n) in [
        "objects",
        "interfaces",
        "unions",
        "enums",
        "inputs",
        "scalars",
    ]
    .into_iter()
    .zip(kinds)
    {
        out.insert(word, n);
    }
    out.insert("fields", fields);
    out.insert("input_fields", input_fields);
    out.insert("arguments", args);
    out.insert("values", values);
    out.insert("directives", count(model.directives().len()));
    out
}

/// Returns the position of `kind` in the order of [`Kind`].
fn kind_index(kind: Kind) -> usize {
    match kind {
        Kind::Object => 0,
        Kind::Interface => 1,
        Kind::Union => 2,
        Kind::Enum => 3,
        Kind::Input => 4,
        Kind::Scalar => 5,
    }
}

/// Returns `n` as a count of the model.
///
/// The limits of the reader cap every list of a model far below `u32::MAX`,
/// 20,000 types and 400,000 members, so the saturation never happens.
fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}
