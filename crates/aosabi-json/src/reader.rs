// SPDX-License-Identifier: MIT
/*
 * aosabi-json reader: the strict JSON reader over bytes, the inverse of the
 * writer.
 *
 * No analogue in the reference, which reads with JSON.parse. A served crate
 * reads the request lines of its stream with it, and the devtool reads its
 * --json-file input with it, so that a document reaches the writer as the
 * value node builds from the same text. It accepts exactly the JSON grammar
 * and nothing more.
 */

//! The JSON reader.
//!
//! [`parse`] turns UTF-8 bytes into a [`JsValue`]. Key order is kept and a
//! repeated key takes the last value at the position of the first, which is
//! what `JSON.parse` does, a number is the nearest double of its text, and a
//! string literal is read as UTF-16 code units so that a `\u` escape of a
//! lone surrogate survives. A failure names the first thing the reader cannot
//! read and its byte offset.
//!
//! The reader recurses once per container, so a document nested deeper than
//! [`MAX_DEPTH`] is refused as a failure rather than read, because the stack
//! would give out first and a process that overflows it aborts without an exit
//! code a caller can read.
//!
//! The comparison of this reader against `JSON.parse` is the work of a later
//! milestone, and no notebook judges it yet.

use alloc::vec::Vec;
use core::{
    fmt,
    str, //
};

use crate::value::{
    JsObject,
    JsString,
    JsValue, //
};

/// The deepest nesting of arrays and objects the reader follows.
///
/// The failure a deeper document earns names this number in its message.
pub const MAX_DEPTH: usize = 1000;

/// Where and why the text stopped being JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadError {
    /// The byte offset of the first thing the reader cannot read.
    offset: usize,
    /// What the reader expected there.
    what: &'static str,
}

impl ReadError {
    /// Returns the byte offset of the first thing the reader cannot read.
    #[must_use]
    pub const fn offset(&self) -> usize {
        self.offset
    }
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at byte offset {}", self.what, self.offset)
    }
}

impl core::error::Error for ReadError {}

/// Parses one JSON document.
///
/// # Errors
///
/// Returns [`ReadError`] when `bytes` is not UTF-8 or is not one value with
/// nothing but whitespace around it.
pub fn parse(bytes: &[u8]) -> Result<JsValue, ReadError> {
    let text = str::from_utf8(bytes).map_err(|error| ReadError {
        offset: error.valid_up_to(),
        what: "not UTF-8",
    })?;
    let mut reader = Reader {
        text,
        at: 0,
        depth: 0,
    };
    reader.skip_whitespace();
    let value = reader.value()?;
    reader.skip_whitespace();
    if reader.at == text.len() {
        Ok(value)
    } else {
        Err(reader.error("trailing bytes after the document"))
    }
}

/// The cursor over the text.
struct Reader<'a> {
    /// The whole document.
    text: &'a str,
    /// The byte offset of the next byte to read.
    at: usize,
    /// How many containers are open around the cursor.
    depth: usize,
}

impl Reader<'_> {
    /// Returns the error for what the cursor is at.
    fn error(&self, what: &'static str) -> ReadError {
        ReadError {
            offset: self.at,
            what,
        }
    }

    /// Returns the byte at the cursor.
    fn peek(&self) -> Option<u8> {
        self.text.as_bytes().get(self.at).copied()
    }

    /// Moves the cursor past the whitespace the grammar allows.
    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    /// Moves the cursor past `byte`, or fails with `what`.
    fn expect(&mut self, byte: u8, what: &'static str) -> Result<(), ReadError> {
        if self.peek() == Some(byte) {
            self.at += 1;
            Ok(())
        } else {
            Err(self.error(what))
        }
    }

    /// Reads one value of any kind.
    fn value(&mut self) -> Result<JsValue, ReadError> {
        match self.peek() {
            Some(b'{') => self.nested(Self::object),
            Some(b'[') => self.nested(Self::array),
            Some(b'"') => self.string().map(JsValue::String),
            Some(b't') => self.literal("true", JsValue::Bool(true)),
            Some(b'f') => self.literal("false", JsValue::Bool(false)),
            Some(b'n') => self.literal("null", JsValue::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => Err(self.error("expected a value")),
        }
    }

    /// Reads a container one level down, or fails when the document is nested
    /// deeper than [`MAX_DEPTH`].
    fn nested(
        &mut self,
        read: fn(&mut Self) -> Result<JsValue, ReadError>,
    ) -> Result<JsValue, ReadError> {
        if self.depth == MAX_DEPTH {
            return Err(self.error("nesting deeper than 1000 levels"));
        }
        self.depth += 1;
        let value = read(self)?;
        self.depth -= 1;
        Ok(value)
    }

    /// Reads one of the three keyword literals.
    fn literal(&mut self, word: &str, value: JsValue) -> Result<JsValue, ReadError> {
        if self.text[self.at..].starts_with(word) {
            self.at += word.len();
            Ok(value)
        } else {
            Err(self.error("expected true, false or null"))
        }
    }

    /// Reads an object, keeping key order and the first position of a repeated
    /// key.
    fn object(&mut self) -> Result<JsValue, ReadError> {
        self.expect(b'{', "expected {")?;
        let mut object = JsObject::new();
        self.skip_whitespace();
        if self.peek() == Some(b'}') {
            self.at += 1;
            return Ok(JsValue::Object(object));
        }
        loop {
            let (key, value) = self.member()?;
            object.insert(key, value);
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b'}') => {
                    self.at += 1;
                    return Ok(JsValue::Object(object));
                }
                _ => return Err(self.error("expected , or } in an object")),
            }
        }
    }

    /// Reads one `key: value` member of an object.
    fn member(&mut self) -> Result<(JsString, JsValue), ReadError> {
        self.skip_whitespace();
        let key = self.string()?;
        self.skip_whitespace();
        self.expect(b':', "expected : after a key")?;
        self.skip_whitespace();
        let value = self.value()?;
        Ok((key, value))
    }

    /// Reads an array in element order.
    fn array(&mut self) -> Result<JsValue, ReadError> {
        self.expect(b'[', "expected [")?;
        let mut items = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b']') {
            self.at += 1;
            return Ok(JsValue::Array(items));
        }
        loop {
            self.skip_whitespace();
            items.push(self.value()?);
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b']') => {
                    self.at += 1;
                    return Ok(JsValue::Array(items));
                }
                _ => return Err(self.error("expected , or ] in an array")),
            }
        }
    }

    /// Reads a string literal as UTF-16 code units, so that a lone surrogate
    /// escape survives, and builds the string from them.
    fn string(&mut self) -> Result<JsString, ReadError> {
        self.expect(b'"', "expected a string")?;
        let mut units: Vec<u16> = Vec::new();
        loop {
            let Some(c) = self.text[self.at..].chars().next() else {
                return Err(self.error("unterminated string"));
            };
            match c {
                '"' => {
                    self.at += 1;
                    return Ok(JsString::from_utf16(&units));
                }
                '\\' => {
                    self.at += 1;
                    self.escape(&mut units)?;
                }
                c if u32::from(c) < 0x20 => {
                    return Err(self.error("control character in a string"));
                }
                c => {
                    let mut buffer = [0; 2];
                    units.extend_from_slice(c.encode_utf16(&mut buffer));
                    self.at += c.len_utf8();
                }
            }
        }
    }

    /// Reads the escape after a backslash and appends its code unit.
    fn escape(&mut self, units: &mut Vec<u16>) -> Result<(), ReadError> {
        if self.peek() == Some(b'u') {
            return self.unicode_escape(units);
        }
        let unit = match self.peek() {
            Some(b'"') => 0x22,
            Some(b'\\') => 0x5C,
            Some(b'/') => 0x2F,
            Some(b'b') => 0x08,
            Some(b'f') => 0x0C,
            Some(b'n') => 0x0A,
            Some(b'r') => 0x0D,
            Some(b't') => 0x09,
            _ => return Err(self.error("unknown escape")),
        };
        self.at += 1;
        units.push(unit);
        Ok(())
    }

    /// Reads the four hexadecimal digits after `\u` and appends the code unit.
    fn unicode_escape(&mut self, units: &mut Vec<u16>) -> Result<(), ReadError> {
        let digits = self
            .text
            .get(self.at + 1..self.at + 5)
            .filter(|digits| digits.bytes().all(|byte| byte.is_ascii_hexdigit()));
        let Some(unit) = digits.and_then(|digits| u16::from_str_radix(digits, 16).ok()) else {
            return Err(self.error("expected four hex digits after \\u"));
        };
        self.at += 5;
        units.push(unit);
        Ok(())
    }

    /// Reads a number by the grammar and converts it to the nearest double,
    /// which is what `JSON.parse` does.
    fn number(&mut self) -> Result<JsValue, ReadError> {
        let start = self.at;
        if self.peek() == Some(b'-') {
            self.at += 1;
        }
        self.integer()?;
        if self.peek() == Some(b'.') {
            self.at += 1;
            self.digits("expected a digit after the point")?;
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.at += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.at += 1;
            }
            self.digits("expected a digit in the exponent")?;
        }
        // Rust's parser is correctly rounded and returns an infinity for a
        // magnitude beyond the double range, which is what `JSON.parse` does
        // with the same text.
        self.text[start..self.at]
            .parse::<f64>()
            .map(JsValue::Number)
            .map_err(|_| self.error("not a number"))
    }

    /// Reads the integer part, which is `0` alone or a digit from 1 to 9
    /// followed by any digits.
    fn integer(&mut self) -> Result<(), ReadError> {
        match self.peek() {
            Some(b'0') => {
                self.at += 1;
                Ok(())
            }
            Some(b'1'..=b'9') => self.digits("expected a digit"),
            _ => Err(self.error("expected a digit")),
        }
    }

    /// Reads one or more digits, or fails with `what` when there is none.
    fn digits(&mut self, what: &'static str) -> Result<(), ReadError> {
        if !matches!(self.peek(), Some(b'0'..=b'9')) {
            return Err(self.error(what));
        }
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.at += 1;
        }
        Ok(())
    }
}
