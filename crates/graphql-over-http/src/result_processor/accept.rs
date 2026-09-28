// SPDX-License-Identifier: MIT
/*
 * graphql-over-http: the negotiation of the media type of an answer.
 *
 * Derived from graphql-yoga 5.18.0, cjs/plugins/result-processor/accept.js, MIT.
 */

//! The negotiation of the media type of an answer.
//!
//! The `Accept` header, `*/*` when absent, loses every whitespace character
//! and is lowercased and split on commas, an entry whose `charset` parameter
//! is not `charset=utf-8` is dropped, and the list is reversed, so no `q`
//! value is read. A request type matches a processor type when each half is
//! `*` or equal.
//!
//! The media types are read where the header stands, the entries of the split
//! from the last to the first, which is the order of the reversed list, so a
//! header of printable ASCII characters with no whitespace and no upper case
//! letter is read with no string made. A header that holds whitespace, an
//! upper case letter or a byte outside printable ASCII takes a copy without
//! its whitespace and in lower case.

use std::borrow::Cow;

use aosabi_http::Request;
use aosabi_json::case::lower_ascii_exact;

use crate::utils::header::with_header;

/// Returns whether `c` is matched by `\s` of a JavaScript regular expression.
fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{0B}' | '\u{0C}' | '\r' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

/// Runs `read` over the media types the request accepts, the last listed first, `getMediaTypesForRequestInOrder(request)`.
///
/// The reader may clone the media types to walk them again.
pub fn with_media_types_in_order<R>(
    request: &Request,
    read: impl FnOnce(MediaTypes<'_>) -> R,
) -> R {
    with_header(request, "accept", |accept| {
        let accept = accept.filter(|accept| !accept.is_empty()).unwrap_or("*/*");
        let accepts = normalized(accept);
        read(MediaTypes {
            entries: accepts.rsplit(','),
        })
    })
}

/// Returns `accept` without its whitespace and in lower case, `accept.replace(/\s/g, '').toLowerCase()`.
///
/// A header of printable ASCII characters with no upper case letter is its
/// own result. Every read of the lower case, here and in
/// [`is_matching_media_type`], splits it at an ASCII separator, looks for
/// ASCII text in it or compares a part with ASCII, so the lower case keeps the
/// mappings that hold ASCII alone.
fn normalized(accept: &str) -> Cow<'_, str> {
    if accept
        .bytes()
        .all(|byte| byte.is_ascii_graphic() && !byte.is_ascii_uppercase())
    {
        return Cow::Borrowed(accept);
    }
    let accepts: String = accept.chars().filter(|c| !is_js_space(*c)).collect();
    Cow::Owned(lower_ascii_exact(&accepts))
}

/// The media types of an `Accept` header, the last listed first.
#[derive(Clone)]
pub struct MediaTypes<'a> {
    /// The entries of the header not read yet, from the last.
    entries: core::str::RSplit<'a, char>,
}

impl<'a> Iterator for MediaTypes<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        for accept in self.entries.by_ref() {
            let mut parts = accept.split(';');
            let media_type = parts.next().unwrap_or_default();
            let charset = parts
                .find(|param| param.contains("charset="))
                .unwrap_or("charset=utf-8");
            // Only utf-8 is supported.
            if charset == "charset=utf-8" {
                return Some(media_type);
            }
        }
        None
    }
}

/// Returns whether the processor type `asked` matches the request type `processor`, `isMatchingMediaType(askedMediaType, processorMediaType)`.
///
/// The caller passes the processor's type first and the request's second, so
/// the wildcards are read on the request's side.
#[must_use]
pub fn is_matching_media_type(asked: &str, processor: &str) -> bool {
    let mut asked_parts = asked.split('/');
    let asked_pre = asked_parts.next();
    let asked_suf = asked_parts.next();
    let mut parts = processor.split('/');
    let pre = parts.next();
    let suf = parts.next();
    (pre == Some("*") || pre == asked_pre) && (suf == Some("*") || suf == asked_suf)
}
