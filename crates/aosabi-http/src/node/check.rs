// SPDX-License-Identifier: MIT
/*
 * aosabi-http: the check Node runs over a record, and the two messages of its
 * validators.
 *
 * It carries the quirk Q59_NODE_RECORD_CHECK_500 of docs/byte-equality.md
 * section 13.1.
 */

//! `check_record` and `RecordError`.
//!
//! The call `writeHead` checks every name of its record against the token of
//! RFC 9110 and every value against tab, 0x20 to 0x7E and 0x80 to 0xFF, and
//! throws `ERR_INVALID_HTTP_TOKEN` or `ERR_INVALID_CHAR` before the head is
//! stored and after the reason is set (N14 of the plan's version 2 draft).
//! In a TypeScript app a record has passed a Fetch `Headers` first, which refuses a
//! name that is no token and a value with a NUL, a CR, an LF or a character
//! above U+00FF, so the check refuses a value with another control byte, 0x01
//! to 0x08, 0x0B, 0x0C, 0x0E to 0x1F or 0x7F. The adapter then answers what
//! `handleResponseError` answers, quirk `Q59_NODE_RECORD_CHECK_500`.

use alloc::{
    format,
    string::String, //
};
use core::fmt;

use crate::message::Line;

/// The first line of a record that `writeHead` refuses, by its index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordError {
    /// The name of the line is no token, `ERR_INVALID_HTTP_TOKEN`.
    Name(usize),
    /// The value of the line holds a byte Node refuses, `ERR_INVALID_CHAR`.
    Value(usize),
}

impl RecordError {
    /// Returns the index of the line the error names.
    #[must_use]
    pub fn index(&self) -> usize {
        match *self {
            Self::Name(index) | Self::Value(index) => index,
        }
    }

    /// Returns the message of the error Node throws for the line of `record` it names.
    ///
    /// The message of `ERR_INVALID_HTTP_TOKEN` is
    /// `Header name must be a valid HTTP token ["<name>"]` and the one of
    /// `ERR_INVALID_CHAR` is `Invalid character in header content ["<name>"]`,
    /// each with the name as the record spells it and no escape.
    #[must_use]
    pub fn message(&self, record: &[Line]) -> String {
        let name = record.get(self.index()).map_or("", |line| &*line.name);
        match self {
            Self::Name(_) => format!("Header name must be a valid HTTP token [\"{name}\"]"),
            Self::Value(_) => format!("Invalid character in header content [\"{name}\"]"),
        }
    }
}

impl fmt::Display for RecordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Name(index) => write!(f, "the name of line {index} of the record is no token"),
            Self::Value(index) => write!(
                f,
                "the value of line {index} of the record holds a byte Node refuses"
            ),
        }
    }
}

impl core::error::Error for RecordError {}

/// Checks every line of `record` in order as Node's `writeHead` does.
///
/// A name is a token when it is not empty and every byte is one of
/// ``^_`a-zA-Z-0-9!#$%&'*+.|~``, the `tokenRegExp` of `_http_common.js:214`,
/// and a value is refused when it holds a byte other than tab, 0x20 to 0x7E
/// and 0x80 to 0xFF, the `headerCharRegex` of `_http_common.js:257`.
///
/// # Errors
///
/// Returns [`RecordError`] for the first line whose name or value Node refuses.
pub fn check_record(record: &[Line]) -> Result<(), RecordError> {
    for (index, line) in record.iter().enumerate() {
        if !is_token(line.name.as_bytes()) {
            return Err(RecordError::Name(index));
        }
        if line.value.iter().any(|&byte| !is_header_char(byte)) {
            return Err(RecordError::Value(index));
        }
    }
    Ok(())
}

/// Returns whether `name` is a token of RFC 9110 as Node's `tokenRegExp` reads it.
fn is_token(name: &[u8]) -> bool {
    !name.is_empty() && name.iter().all(|&byte| is_token_byte(byte))
}

/// Returns whether `byte` is a character of a token.
fn is_token_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"^_`-!#$%&'*+.|~".contains(&byte)
}

/// Returns whether `byte` may stand in a header value.
fn is_header_char(byte: u8) -> bool {
    byte == b'\t' || (0x20..=0x7E).contains(&byte) || byte >= 0x80
}
