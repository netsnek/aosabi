// SPDX-License-Identifier: MIT
/*
 * aoviewer-core tree manifest: the manifest of a tree.
 *
 * The manifest hashes every other file and carries the generator and the layout
 * version alone.
 */

//! The manifest.
//!
//! `manifest.json` is
//! `{"version":1,"generator":..,"layout_version":.., "schema":{"name":..,"sha256":..},"options":..,"layout":..,"counts":{..}, "theme_sha256":..,"files":[[path,sha256,bytes],...]}`
//! with the files in the byte order of their paths.
//!
//! - The schema hash is the SHA-256 of the canonical introspection form
//!   [`to_introspection`] writes, so every input of one schema, the native
//!   build and the module give one hash.
//! - The options are the query of [`DocsOptions::to_query`] without the
//!   schema and the generator, which the manifest records under keys of
//!   their own.
//! - The layout is the one the options select for the schema, `single` or
//!   `split`.
//! - The counts are the ones of [`Counts`], the source of the clusters as
//!   `declared`, `stems`, `territories` or `territories-fallback`.
//! - The theme is recorded by the hash of [`SABI_THEME`] and not of the file on
//!   disk, which a reader may have edited, and is `null` when the tree holds no
//!   theme file.
//! - The files are every file of the tree but the manifest, each with the
//!   SHA-256 of its bytes and its length.
//!
//! The generator string and [`LAYOUT_VERSION`] appear here and nowhere else in
//! a tree. Every file is one line, so a difference of two manifests reads line
//! by line, and the JSON is the same document with or without the line feeds.
//!
//! The loop runs over the files of one tree.
//!
//! [`to_introspection`]: crate::read::to_introspection
//! [`DocsOptions::to_query`]: super::DocsOptions::to_query
//! [`Counts`]: super::Counts
//! [`SABI_THEME`]: crate::render_typst::SABI_THEME
//! [`LAYOUT_VERSION`]: crate::LAYOUT_VERSION

use alloc::{
    string::String,
    vec::Vec, //
};
use core::fmt::Write;

use super::{
    Counts,
    TreeFile,
    options::DocsOptions, //
};
use crate::{
    LAYOUT_VERSION,
    cluster::ClusterSource,
    json::write_str,
    page::LayoutMode,
    read::to_introspection,
    schema::Schema,
    sha256::sha256_hex, //
};

/// The version of the manifest.
const VERSION: u32 = 1;

/// The path of the theme file in a tree.
#[cfg(feature = "typst")]
const THEME_FILE: &str = "sabi-theme.typ";

/// Writes the manifest of the tree of `schema` under `options` in `layout`,
/// with `counts`, over `files`, and appends its bytes to `out`.
///
/// `files` is every file of the tree in the byte order of the paths, and a
/// `manifest.json` among them is not listed.
pub(super) fn write(
    schema: &Schema,
    options: &DocsOptions,
    layout: LayoutMode,
    counts: &Counts,
    files: &[TreeFile],
    out: &mut Vec<u8>,
) {
    let mut text = String::new();
    // Writing into a `String` cannot fail.
    let _ = write!(text, "{{\"version\":{VERSION},\"generator\":");
    write_str(&options.generator, &mut text);
    let _ = write!(
        text,
        ",\"layout_version\":{LAYOUT_VERSION},\"schema\":{{\"name\":"
    );
    write_str(&options.schema_name, &mut text);
    text.push_str(",\"sha256\":\"");
    text.push_str(&schema_hash(schema));
    text.push_str("\"},\"options\":");
    let mut query = String::new();
    options.write_query(false, &mut query);
    write_str(&query, &mut text);
    text.push_str(",\"layout\":\"");
    text.push_str(match layout {
        LayoutMode::Single => "single",
        LayoutMode::Split => "split",
    });
    text.push_str("\",\"counts\":");
    write_counts(counts, &mut text);
    text.push_str(",\"theme_sha256\":");
    match theme_hash(files) {
        Some(hash) => {
            text.push('"');
            text.push_str(&hash);
            text.push('"');
        }
        None => text.push_str("null"),
    }
    text.push_str(",\"files\":[");
    let listed = files.iter().filter(|f| f.path != "manifest.json");
    for (i, f) in listed.enumerate() {
        if i > 0 {
            text.push(',');
        }
        text.push_str("\n[");
        write_str(&f.path, &mut text);
        let _ = write!(text, ",\"{}\",{}]", sha256_hex(&f.bytes), f.bytes.len());
    }
    text.push_str("\n]}\n");
    out.extend_from_slice(text.as_bytes());
}

/// Returns the SHA-256 of the canonical introspection form of the model of
/// `schema`, in lower case hex.
fn schema_hash(schema: &Schema) -> String {
    let mut text = String::new();
    to_introspection(&schema.model, &mut text);
    sha256_hex(text.as_bytes())
}

/// Writes the counts as a JSON object.
fn write_counts(counts: &Counts, out: &mut String) {
    let source = match counts.cluster_source {
        ClusterSource::Declared => "declared",
        ClusterSource::Stems => "stems",
        ClusterSource::Territories => "territories",
        ClusterSource::TerritoriesFallback => "territories-fallback",
    };
    // Writing into a `String` cannot fail.
    let _ = write!(
        out,
        "{{\"types\":{},\"boxes\":{},\"diagrams\":{},\"pages\":{},\"skipped\":{},\"reduced\":{},\"over_budget\":{},\"clusters\":{},\"cluster_source\":\"{source}\"}}",
        counts.types,
        counts.boxes,
        counts.diagrams,
        counts.pages,
        counts.skipped,
        counts.reduced,
        counts.over_budget,
        counts.clusters
    );
}

/// Returns the hash of [`SABI_THEME`](crate::render_typst::SABI_THEME) when
/// `files` holds the theme file.
#[cfg(feature = "typst")]
fn theme_hash(files: &[TreeFile]) -> Option<String> {
    files
        .iter()
        .any(|f| f.path == THEME_FILE)
        .then(|| sha256_hex(crate::render_typst::SABI_THEME.as_bytes()))
}

/// Returns no hash, since the build carries no theme.
#[cfg(not(feature = "typst"))]
fn theme_hash(_files: &[TreeFile]) -> Option<String> {
    None
}
