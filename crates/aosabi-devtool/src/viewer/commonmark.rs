// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool viewer commonmark: the operation commonmark, the HTML the
 * CommonMark subset of the core writes for Markdown texts.
 */

//! The `commonmark` operation.
//!
//! The request `{texts}` answers `html`, the text [`to_html`] writes for
//! each Markdown text after [`parse`] read it under
//! [`ReadLimits::DEFAULT`], in the order of the texts. A text holding a lone
//! surrogate is refused, since the subset reads a Rust string.

use aosabi_json::{
    JsObject,
    JsValue, //
};
use aoviewer_core::{
    commonmark::{
        parse,
        to_html, //
    },
    limits::ReadLimits, //
};

use super::{
    Stop,
    strings, //
};

/// Answers the operation `commonmark`.
pub(super) fn run(request: &JsObject) -> Result<JsObject, Stop> {
    let texts = strings(request, "texts")?;
    let limits = ReadLimits::DEFAULT;
    let mut out = Vec::with_capacity(texts.len());
    for text in texts {
        let doc = parse(text, &limits);
        let mut html = String::new();
        to_html(&doc, text, &mut html);
        out.push(JsValue::from(html));
    }
    let mut payload = JsObject::with_capacity(1);
    payload.insert("html", JsValue::Array(out));
    Ok(payload)
}
