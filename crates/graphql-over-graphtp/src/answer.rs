// SPDX-License-Identifier: MIT
/*
 * graphql-over-graphtp answer: what the listener answers for what the app
 * answered, step 4 of the listener.
 *
 * No analogue in the reference. The cases and their order are those of
 * docs/graphtp.md section 7, step 4, and docs/graphtp-in-aosabi.md section 3,
 * step 4.
 */

//! What an answer of the app becomes.
//!
//! An answer under `text/event-stream` opens a stream. Every other answer
//! becomes its status, its substatus and its body, verbatim, but in four
//! cases, taken in this order, and the first that fits decides: a status of
//! 294 becomes 200.1 or 200.2 by its `data`, a status with no byte 500.2, a
//! status of class 4 or 5 under another media type than the answer type
//! `status.0` with an empty body, and a status of class 2 or 3 under such a
//! type 500.3 with an empty body.

use graphtp::status::{
    Code,
    Status,
    Substatus, //
};

/// The status the GraphQL over HTTP draft asks for a result with `data` and
/// `errors`, which has no status byte.
const PARTIAL_SUCCESS: u16 = 294;

/// What the listener answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Decision {
    /// The answer opens a stream.
    Stream,
    /// The answer carries the code and the body the app wrote.
    Answer(Code),
    /// The answer carries the code and an empty body.
    Refuse(Code),
}

/// Returns what the listener answers for the status `status`, the substatus
/// `substatus` and the media type `content_type` of an answer of the app with
/// the body `body`.
///
/// A substatus the registry does not hold under the status gives the status
/// alone, so a phase never yields a code the registry lacks.
pub(crate) fn decide(
    status: u16,
    substatus: u8,
    content_type: Option<&[u8]>,
    body: &[u8],
) -> Decision {
    if content_type.is_some_and(is_event_stream) {
        return Decision::Stream;
    }
    if status == PARTIAL_SUCCESS {
        let code = if data_holds_data(body) {
            Code::PARTIAL
        } else {
            Code::NO_DATA
        };
        return Decision::Answer(code);
    }
    let Some(status) = Status::from_u16(status) else {
        return Decision::Refuse(Code::STATUS_NOT_REPRESENTABLE);
    };
    if !content_type.is_some_and(is_answer_type) {
        return match status.class() {
            4 | 5 => Decision::Refuse(Code::new(status, Substatus::NONE)),
            _ => Decision::Refuse(Code::UNSUPPORTED_ANSWER_TYPE),
        };
    }
    let code = Code::new(status, Substatus::new(substatus));
    if substatus != 0 && code.name().is_none() {
        return Decision::Answer(Code::new(status, Substatus::NONE));
    }
    Decision::Answer(code)
}

/// Tells whether `value` names the media type `text/event-stream`.
pub(crate) fn is_event_stream(value: &[u8]) -> bool {
    media_type(value).eq_ignore_ascii_case(b"text/event-stream")
}

/// Tells whether `value` names the answer type of `GraphTP`,
/// `application/graphql-response+json` with the parameter `charset=utf-8`.
pub(crate) fn is_answer_type(value: &[u8]) -> bool {
    if !media_type(value).eq_ignore_ascii_case(b"application/graphql-response+json") {
        return false;
    }
    value.split(|&byte| byte == b';').skip(1).any(|parameter| {
        let mut parts = parameter.splitn(2, |&byte| byte == b'=');
        let name = parts.next().unwrap_or_default().trim_ascii();
        let charset = parts.next().unwrap_or_default().trim_ascii();
        let charset = charset
            .strip_prefix(b"\"")
            .and_then(|inner| inner.strip_suffix(b"\""))
            .unwrap_or(charset);
        name.eq_ignore_ascii_case(b"charset") && charset.eq_ignore_ascii_case(b"utf-8")
    })
}

/// Returns the media type of the value of `content-type`, the part before its
/// first parameter.
fn media_type(value: &[u8]) -> &[u8] {
    value
        .split(|&byte| byte == b';')
        .next()
        .unwrap_or_default()
        .trim_ascii()
}

/// Tells whether the member `data` of the JSON object `body` holds a value
/// other than `null`.
///
/// A body that is no object, or an object with no member `data` at its top
/// level, holds no data.
fn data_holds_data(body: &[u8]) -> bool {
    let mut at = skip_space(body, 0);
    if body.get(at) != Some(&b'{') {
        return false;
    }
    at += 1;
    loop {
        at = skip_space(body, at);
        let Some(key) = string_end(body, at) else {
            return false;
        };
        let name = body.get(at + 1..key - 1).unwrap_or_default();
        at = skip_space(body, key);
        if body.get(at) != Some(&b':') {
            return false;
        }
        at = skip_space(body, at + 1);
        if name == b"data" {
            return !body.get(at..).unwrap_or_default().starts_with(b"null");
        }
        let Some(end) = value_end(body, at) else {
            return false;
        };
        at = skip_space(body, end);
        if body.get(at) != Some(&b',') {
            return false;
        }
        at += 1;
    }
}

/// Returns the first index at or after `at` that holds no JSON whitespace.
fn skip_space(body: &[u8], mut at: usize) -> usize {
    while matches!(body.get(at), Some(b' ' | b'\t' | b'\n' | b'\r')) {
        at += 1;
    }
    at
}

/// Returns the index after the JSON string that starts at `at`, or `None`
/// when no string starts there or it does not end.
fn string_end(body: &[u8], at: usize) -> Option<usize> {
    if body.get(at) != Some(&b'"') {
        return None;
    }
    let mut index = at + 1;
    loop {
        match body.get(index)? {
            b'"' => return Some(index + 1),
            b'\\' => index += 2,
            _ => index += 1,
        }
    }
}

/// Returns the index after the JSON value that starts at `at`, or `None` when
/// it does not end.
fn value_end(body: &[u8], at: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut index = at;
    loop {
        match body.get(index) {
            Some(b'"') => index = string_end(body, index)?,
            Some(b'{' | b'[') => {
                depth += 1;
                index += 1;
            }
            Some(b'}' | b']') if depth > 0 => {
                depth -= 1;
                index += 1;
            }
            Some(b',' | b'}' | b']' | b' ' | b'\t' | b'\n' | b'\r') | None if depth == 0 => {
                return (index > at).then_some(index);
            }
            Some(_) => index += 1,
            None => return None,
        }
        if depth == 0 && index > at && matches!(body.get(index - 1), Some(b'"' | b'}' | b']')) {
            return Some(index);
        }
    }
}
