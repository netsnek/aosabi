// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool viewer clusters: the operation clusters, both automatic
 * clusterings of an input with their shares and the pick.
 *
 * The rule that picks among the declared map, the name stems and the root
 * field territories is docs/aoviewer.md section 5.5, and 51_viewer_core
 * prints the table for 75_docgen to judge.
 */

//! The `clusters` operation.
//!
//! The request `{input, options, declared}` builds the schema under the
//! transforms of the option string `options` and answers `table`, the text
//! of [`Clustering::table`], both automatic clusterings judged before their
//! split with their shares and the pick, and `picked`, the clustering
//! [`Clustering::pick`] answers with the declared map `declared`, a list of
//! `{name, globs}`, or without one. The picked clustering carries its
//! `source`, its `clusters` with the `name`, the `slug`, the `types` and the
//! edges `inside` and `across` of each, the `other` bucket and the types
//! `outside`.

use aosabi_json::{
    JsObject,
    JsValue, //
};
use aoviewer_core::{
    cluster::Clustering,
    model::Model,
    schema::Schema, //
};

use super::{
    Stop,
    declared,
    load,
    names,
    source_word,
    view_options, //
};

/// Answers the operation `clusters`.
pub(super) fn run(request: &JsObject) -> Result<JsObject, Stop> {
    let options = view_options(request)?;
    let declared = declared(request)?;
    let loaded = load(request)?;
    let schema = Schema::build(loaded.model, options.transforms);
    let mut table = String::new();
    Clustering::table(&schema, &mut table);
    let picked = Clustering::pick(&schema, &declared);
    let mut payload = JsObject::with_capacity(2);
    payload.insert("table", table);
    payload.insert("picked", clustering(&schema.model, &picked));
    Ok(payload)
}

/// Returns `clustering` of `model` as JSON.
fn clustering(model: &Model, clustering: &Clustering) -> JsValue {
    let clusters = clustering
        .clusters
        .iter()
        .map(|c| {
            let mut entry = JsObject::with_capacity(5);
            entry.insert("name", c.name.as_str());
            entry.insert("slug", c.slug.as_str());
            entry.insert("types", names(model, &c.types));
            entry.insert("inside", c.inside);
            entry.insert("across", c.across);
            JsValue::Object(entry)
        })
        .collect();
    let mut out = JsObject::with_capacity(4);
    out.insert("source", source_word(clustering.source));
    out.insert("clusters", JsValue::Array(clusters));
    out.insert("other", names(model, &clustering.other));
    out.insert("outside", names(model, &clustering.outside));
    JsValue::Object(out)
}
