// SPDX-License-Identifier: MIT
/*
 * aosabi-http: the bytes of a header value under the first write.
 *
 * It carries the quirk Q58_NODE_ENCODING_FIRST_WRITE of docs/byte-equality.md
 * section 13.1.
 */

//! `value_bytes`.
//!
//! Node holds the head of an answer as a string and sends it with the first
//! write after `writeHead`. A string write whose encoding is `utf8`, `latin1`
//! or none joins the head to it and sends both in that encoding, none being
//! UTF-8, and a write of bytes queues the head alone as `latin1` (N13 of the
//! plan's version 2 draft). A value character from U+0080 to U+00FF, which is
//! how a request byte from 0x80 up reaches the application, therefore goes out
//! as one byte under Latin-1 and as two under UTF-8, quirk
//! `Q58_NODE_ENCODING_FIRST_WRITE`. Names, the status line and every other line
//! are ASCII and the same under both.

use alloc::{
    borrow::Cow,
    vec::Vec, //
};

use crate::message::FirstWrite;

/// Returns the bytes of the value `value` in the head that `first_write` sends.
///
/// `value` is one byte per character, the Latin-1 form of a record line, and
/// `has_body` tells whether the answer has a body. A value goes out one byte
/// per character when the first write is [`FirstWrite::Bytes`] or
/// [`FirstWrite::End`], and when it is [`FirstWrite::Text`] on an answer with
/// no body, whose string Node drops before it sends `''` as `latin1`. It goes
/// out as UTF-8, two bytes for a character from U+0080 up, when the first write
/// is [`FirstWrite::Text`] with a body or [`FirstWrite::Flush`].
#[must_use]
pub fn value_bytes(value: &[u8], first_write: FirstWrite, has_body: bool) -> Cow<'_, [u8]> {
    let utf8 = match first_write {
        FirstWrite::Bytes | FirstWrite::End => false,
        FirstWrite::Text => has_body,
        FirstWrite::Flush => true,
    };
    if !utf8 || value.is_ascii() {
        return Cow::Borrowed(value);
    }
    let mut bytes = Vec::with_capacity(value.len() * 2);
    for &byte in value {
        if byte < 0x80 {
            bytes.push(byte);
        } else {
            bytes.push(0xC0 | (byte >> 6));
            bytes.push(0x80 | (byte & 0x3F));
        }
    }
    Cow::Owned(bytes)
}
