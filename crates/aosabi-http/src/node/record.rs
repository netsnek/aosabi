// SPDX-License-Identifier: MIT
/*
 * aosabi-http: the record of the Node adapter over neutral lines.
 *
 * Derived from hono 1.19.9, dist/utils.js, MIT.
 * Derived from hono 1.19.9, dist/listener.js, MIT.
 *
 * It carries the half of aosabi-http of the quirk Q49_NODE_ADAPTER_RECORD of
 * docs/byte-equality.md section 13.1.
 */

//! The record of the Node adapter.
//!
//! The adapter hands `writeHead` a plain object, its record, and Node writes
//! one line per key of that object in the order `Object.keys` gives, one line
//! per element of an array value (N2 of the plan's version 2 draft). The
//! adapter builds the record from the Fetch `Headers` of the answer in their
//! iteration order, the names in lower case, sorted and combined, collects
//! every `set-cookie` value apart and appends them after every other name,
//! then appends `content-type: text/plain; charset=UTF-8` when no
//! `content-type` was set (A1, quirk `Q49_NODE_ADAPTER_RECORD`). The cached
//! path appends `Content-Length` with capitals and the object path
//! `content-length` in lower case, each after the record (A2 and A4).
//!
//! The keys of a JavaScript object are ordered by the rule of
//! `OrdinaryOwnPropertyKeys`: the keys that are array indices first, in
//! ascending numeric order, then every other key in insertion order. A header
//! name that is an array index, `42` among them, is a token, so the record
//! keeps that rule.

use alloc::{
    borrow::Cow,
    format,
    string::String,
    vec::Vec, //
};

pub use crate::message::Line;

/// The default `content-type` the adapter appends to an answer that sets none.
pub const DEFAULT_CONTENT_TYPE: &str = "text/plain; charset=UTF-8";

/// Returns the record of the Node adapter over entries in Fetch iteration order.
///
/// `entries` are the entries of the `Headers` of the answer as their iteration
/// gives them, the names in lower case, sorted and combined, `set-cookie` left
/// out, and `set_cookie` are the values of `getSetCookie()`. Every value is
/// written one byte per character, the Latin-1 form a Fetch `Headers` holds,
/// since no value of a `Headers` has a character above U+00FF.
#[must_use]
pub fn build_outgoing_http_headers(
    entries: &[(String, String)],
    set_cookie: &[String],
) -> Vec<Line> {
    let mut record: Vec<Line> = Vec::with_capacity(entries.len() + set_cookie.len() + 1);
    for (name, value) in entries {
        if name == "set-cookie" {
            continue;
        }
        assign(&mut record, name, latin1(value));
    }
    for cookie in set_cookie {
        record.push(Line::new(String::from("set-cookie"), latin1(cookie)));
    }
    if !record.iter().any(|line| line.name == "content-type") {
        record.push(Line::new("content-type", DEFAULT_CONTENT_TYPE.as_bytes()));
    }
    order_array_indices(&mut record);
    record
}

/// Appends `Content-Length` with capitals, the length of a cached body, after the record.
///
/// The assignment replaces the value of a key spelled exactly `Content-Length`
/// in place and appends the key otherwise, so a lower case `content-length` of
/// the record stays beside it.
pub fn append_cached_length(record: &mut Vec<Line>, len: u64) {
    assign(record, "Content-Length", decimal(len));
}

/// Appends `content-length` in lower case, the length of a body read whole, when the record has none.
pub fn append_read_length(record: &mut Vec<Line>, len: u64) {
    if record.iter().any(|line| line.name == "content-length") {
        return;
    }
    record.push(Line::new("content-length", decimal(len)));
}

/// Assigns `value` to the key `name` of the record, in place when it has the key.
fn assign(record: &mut Vec<Line>, name: &str, value: Vec<u8>) {
    if let Some(line) = record.iter_mut().find(|line| line.name == name) {
        line.value = value;
        return;
    }
    record.push(Line::new(Cow::Owned(String::from(name)), value));
}

/// Moves the keys that are array indices to the front, in ascending numeric order.
///
/// The rule of `OrdinaryOwnPropertyKeys`. The sort is stable, so every other
/// key keeps its insertion order. The lines of one key, the values of
/// `set-cookie`, never form an index.
fn order_array_indices(record: &mut [Line]) {
    if !record.iter().any(|line| array_index(&line.name).is_some()) {
        return;
    }
    record.sort_by_key(|line| match array_index(&line.name) {
        Some(index) => (0, index),
        None => (1, 0),
    });
}

/// Returns the array index `name` is, when it is one.
///
/// An array index is the canonical decimal form of an integer from 0 to
/// 4,294,967,294.
fn array_index(name: &str) -> Option<u32> {
    let bytes = name.as_bytes();
    if bytes.is_empty() || !bytes.iter().all(u8::is_ascii_digit) {
        return None;
    }
    if bytes.len() > 1 && bytes[0] == b'0' {
        return None;
    }
    name.parse::<u32>().ok().filter(|&index| index != u32::MAX)
}

/// Returns `value` one byte per character.
///
/// A character above U+00FF, which no value of a Fetch `Headers` holds, is
/// written as the low byte of its code point, as `Buffer.from(s, 'latin1')`
/// writes it.
fn latin1(value: &str) -> Vec<u8> {
    // Latin-1 keeps the low byte of a code point, as the latin1 encoding of
    // Node does, so the truncation is the encoding.
    #[expect(clippy::cast_possible_truncation)]
    value
        .chars()
        .map(|character| character as u32 as u8)
        .collect()
}

/// Returns the decimal digits of `value`, as `String(value)` writes an integer.
fn decimal(value: u64) -> Vec<u8> {
    format!("{value}").into_bytes()
}
