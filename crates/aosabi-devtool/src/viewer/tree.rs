// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool viewer tree: the operation tree, the whole tree of an input
 * written into a directory.
 *
 * The tree is the one `aosabi docs` writes, rendered by generate and written
 * by write of aoviewer-docs, so the seam and the command share one path.
 */

//! The `tree` operation.
//!
//! The request `{input, options, since, out}` renders the tree of the input
//! under the tree options `options`, the query form of [`DocsOptions`], with
//! the change log of the older schema `since` when the request names one,
//! through [`generate`], and writes it into the directory `out` through
//! [`write()`] when the request names one. It answers `files`, one
//! `[path, sha256, bytes]` per file in the order of the paths, `counts` and
//! `notes` of the tree, and for a written tree `written`, `unchanged`,
//! `removed` and `theme_kept`. A refusal of the generator answers `refused`
//! and `message`, an older schema that does not load `AO-D0014` around its
//! own refusal.

use std::path::PathBuf;

use aosabi_json::{
    JsObject,
    JsValue, //
};
use aoviewer_core::{
    limits::ReadLimits,
    sha256::sha256_hex,
    tree::{
        Counts,
        DocsOptions,
        Tree, //
    }, //
};
use aoviewer_docs::{
    error::DocsError,
    generate::generate,
    input::detect,
    request::Request,
    write::write, //
};

use super::{
    Stop,
    fatal,
    option_text,
    optional,
    path,
    source_word, //
};

/// Answers the operation `tree`.
pub(super) fn run(request: &JsObject) -> Result<JsObject, Stop> {
    let options = match optional(request, "options")? {
        Some(query) => DocsOptions::parse(query)
            .map_err(|e| fatal(request, &format!("options is refused, {}", option_text(&e))))?,
        None => DocsOptions::default(),
    };
    let limits = ReadLimits::DEFAULT;
    let input = detect(&path(request, "input")?, &limits)?;
    let since = match optional(request, "since")? {
        Some(old) => {
            let old = detect(&PathBuf::from(old), &limits);
            Some(old.map_err(|e| DocsError::Since(Box::new(e)))?)
        }
        None => None,
    };
    let out = optional(request, "out")?.map(PathBuf::from);
    let mut asked = Request::new(input);
    asked.options = options;
    asked.since = since;
    let tree = generate(&asked)?;
    let mut payload = JsObject::with_capacity(8);
    payload.insert("files", files(&tree));
    payload.insert("counts", counts(&tree.counts));
    let notes = tree
        .notes
        .iter()
        .map(|n| JsValue::from(n.as_str()))
        .collect();
    payload.insert("notes", JsValue::Array(notes));
    if let Some(out) = out {
        let written = write(&tree, &out)?;
        payload.insert("written", written.written);
        payload.insert("unchanged", written.unchanged);
        payload.insert("removed", written.removed);
        payload.insert("theme_kept", written.theme_kept);
    }
    Ok(payload)
}

/// Returns the files of `tree` as `[path, sha256, bytes]`.
fn files(tree: &Tree) -> JsValue {
    JsValue::Array(
        tree.files
            .iter()
            .map(|f| {
                JsValue::Array(vec![
                    JsValue::from(f.path.as_str()),
                    JsValue::from(sha256_hex(&f.bytes)),
                    JsValue::from(f.bytes.len() as u64),
                ])
            })
            .collect(),
    )
}

/// Returns the counts of a tree as JSON.
fn counts(counts: &Counts) -> JsValue {
    let mut out = JsObject::with_capacity(9);
    out.insert("types", counts.types);
    out.insert("boxes", counts.boxes);
    out.insert("diagrams", counts.diagrams);
    out.insert("pages", counts.pages);
    out.insert("skipped", counts.skipped);
    out.insert("reduced", counts.reduced);
    out.insert("over_budget", counts.over_budget);
    out.insert("clusters", counts.clusters);
    out.insert("cluster_source", source_word(counts.cluster_source));
    JsValue::Object(out)
}
