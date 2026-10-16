// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool viewer sha256: the operation sha256, the digest of the core
 * over byte strings, and the operation json-str, the JSON string writer of
 * the core over texts.
 *
 * The module hashlib of Python judges the digests and the json primitive of
 * the seam the strings, the two small primitives of step 9 of
 * 51_viewer_core.
 */

//! The `sha256` and `json-str` operations.
//!
//! The request `{hex}` holds byte strings in base 16, upper or lower case,
//! and answers `sha256`, the digest of each in lower case base 16 as
//! [`sha256_hex`] of the core writes it. The request `{texts}` answers
//! `json`, the JSON string [`write_str`] of the core writes for each text,
//! which step 9 compares with the `json` primitive of the seam. A text
//! holding a lone surrogate is refused, since the writer takes a Rust
//! string, and the two primitives sit together in this module because the
//! plan names no operation of its own for the writer.

use aosabi_json::{
    JsObject,
    JsValue, //
};
use aoviewer_core::{
    json::write_str,
    sha256::sha256_hex, //
};

use super::{
    Stop,
    fatal,
    strings, //
};
use crate::homa_packet::unhex;

/// Answers the operation `sha256`.
pub(super) fn run(request: &JsObject) -> Result<JsObject, Stop> {
    let inputs = strings(request, "hex")?;
    let mut digests = Vec::with_capacity(inputs.len());
    for (i, text) in inputs.iter().enumerate() {
        let Some(bytes) = unhex(text) else {
            return Err(fatal(
                request,
                &format!("hex[{i}] is no string of base 16 digits"),
            ));
        };
        digests.push(JsValue::from(sha256_hex(&bytes)));
    }
    let mut payload = JsObject::with_capacity(1);
    payload.insert("sha256", JsValue::Array(digests));
    Ok(payload)
}

/// Answers the operation `json-str`.
pub(super) fn run_json_str(request: &JsObject) -> Result<JsObject, Stop> {
    let texts = strings(request, "texts")?;
    let mut out = Vec::with_capacity(texts.len());
    for text in texts {
        let mut json = String::with_capacity(text.len() + 2);
        write_str(text, &mut json);
        out.push(JsValue::from(json));
    }
    let mut payload = JsObject::with_capacity(1);
    payload.insert("json", JsValue::Array(out));
    Ok(payload)
}
