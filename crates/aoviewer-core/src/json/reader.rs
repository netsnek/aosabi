// SPDX-License-Identifier: MIT
/*
 * aoviewer-core json reader: the pull parser over the bytes of an RFC 8259
 * document.
 *
 * It checks UTF-8, decodes escapes and surrogate pairs, keeps numbers as their
 * text and caps the depth, over the grammar of RFC 8259.
 */

//! The JSON pull parser.
//!
//! Every loop of the parser either consumes at least one byte per iteration
//! and stops at the end of the input, whose length
//! [`ReadLimits::max_input_bytes`] caps, or reads the four digits of a `\u`
//! escape in place. The container stack is capped by
//! [`ReadLimits::max_depth`].

use alloc::{
    borrow::Cow,
    string::String,
    vec::Vec, //
};

use crate::limits::ReadLimits;

/// One token of a JSON document, in document order.
///
/// A string borrows the input when it carries no escape and owns its decoded
/// text otherwise. A number is its text as the document writes it, so no float
/// is ever formed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Token<'a> {
    /// The `{` that opens an object.
    BeginObject,
    /// The `}` that closes an object.
    EndObject,
    /// The `[` that opens an array.
    BeginArray,
    /// The `]` that closes an array.
    EndArray,
    /// A member name of an object, with its colon consumed.
    Key(Cow<'a, str>),
    /// A string value.
    Str(Cow<'a, str>),
    /// A number value as its text.
    Number(&'a str),
    /// The literal `true`.
    True,
    /// The literal `false`.
    False,
    /// The literal `null`.
    Null,
}

/// A reason the parser refuses a document.
///
/// [`Parser::offset`] names the byte the error was found at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JsonError {
    /// The document ends inside a value.
    Eof,
    /// A byte stands where the grammar allows none of its kind.
    Byte(u8),
    /// A string is not valid UTF-8.
    Utf8,
    /// A backslash escape is not one RFC 8259 defines.
    Escape,
    /// The containers nest deeper than [`ReadLimits::max_depth`].
    Depth,
    /// A number does not follow the number grammar.
    Number,
    /// Bytes other than whitespace follow the top level value.
    TrailingData,
    /// The document is longer than [`ReadLimits::max_input_bytes`].
    TooLarge,
}

/// The kind of an open container.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Frame {
    Object,
    Array,
}

/// What the parser reads next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    /// A value, at the top level, after a key or after a comma in an array.
    Value,
    /// A key or the `}` right after `{`.
    ObjectFirst,
    /// A value or the `]` right after `[`.
    ArrayFirst,
    /// A comma or a closing bracket, or the end once no container is open.
    AfterValue,
    /// Nothing but whitespace.
    Done,
    /// Nothing, because the document was refused.
    Failed(JsonError),
}

/// A pull parser over bytes.
///
/// It reads RFC 8259 tokens, checks UTF-8, decodes `\u` escapes and surrogate
/// pairs, replaces a lone surrogate by U+FFFD, keeps numbers as their text and
/// caps the depth. A duplicate key is passed on as it stands, so the reader
/// above decides what it means.
#[derive(Debug)]
pub struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
    max_depth: usize,
    stack: Vec<Frame>,
    state: State,
}

impl<'a> Parser<'a> {
    /// Returns a parser over `bytes` that holds `limits`.
    ///
    /// A document longer than [`ReadLimits::max_input_bytes`] is refused by
    /// the first call of [`Parser::next`] with [`JsonError::TooLarge`].
    #[must_use]
    pub fn new(bytes: &'a [u8], limits: &ReadLimits) -> Self {
        let too_large = u32::try_from(bytes.len()).map_or(true, |n| n > limits.max_input_bytes);
        Self {
            bytes,
            pos: 0,
            max_depth: usize::from(limits.max_depth),
            stack: Vec::new(),
            state: if too_large {
                State::Failed(JsonError::TooLarge)
            } else {
                State::Value
            },
        }
    }

    /// Returns the next token, or `None` once the document ended well.
    ///
    /// # Errors
    ///
    /// Returns the [`JsonError`] of the first defect, and the same error on
    /// every later call.
    // The method is not `Iterator::next`: it returns a `Result` around the
    // option, so a refused document is told apart from a finished one.
    #[expect(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<Option<Token<'a>>, JsonError> {
        let step = self.step();
        if let Err(e) = step {
            self.state = State::Failed(e);
        }
        step
    }

    /// Returns the byte offset the parser stands at, which is the offset of
    /// the defect after an error.
    #[must_use]
    pub fn offset(&self) -> u64 {
        u64::try_from(self.pos).unwrap_or(u64::MAX)
    }

    /// Reads one token in the current state.
    fn step(&mut self) -> Result<Option<Token<'a>>, JsonError> {
        match self.state {
            State::Failed(e) => Err(e),
            State::Done => self.end(),
            State::Value => self.value().map(Some),
            State::ObjectFirst => {
                if self.peek_token()? == b'}' {
                    Ok(Some(self.close()))
                } else {
                    self.key().map(Some)
                }
            }
            State::ArrayFirst => {
                if self.peek_token()? == b']' {
                    Ok(Some(self.close()))
                } else {
                    self.value().map(Some)
                }
            }
            State::AfterValue => self.after_value(),
        }
    }

    /// Reads the comma or the closing bracket after a value.
    fn after_value(&mut self) -> Result<Option<Token<'a>>, JsonError> {
        let Some(&top) = self.stack.last() else {
            self.state = State::Done;
            return self.end();
        };
        let b = self.peek_token()?;
        match (b, top) {
            (b',', Frame::Object) => {
                self.pos += 1;
                self.key().map(Some)
            }
            (b',', Frame::Array) => {
                self.pos += 1;
                self.value().map(Some)
            }
            (b'}', Frame::Object) | (b']', Frame::Array) => Ok(Some(self.close())),
            _ => Err(JsonError::Byte(b)),
        }
    }

    /// Accepts the end of the document after the top level value.
    fn end(&mut self) -> Result<Option<Token<'a>>, JsonError> {
        self.skip_whitespace();
        if self.pos == self.bytes.len() {
            Ok(None)
        } else {
            Err(JsonError::TrailingData)
        }
    }

    /// Reads a value.
    fn value(&mut self) -> Result<Token<'a>, JsonError> {
        let b = self.peek_token()?;
        let token = match b {
            b'{' => return self.open(Frame::Object),
            b'[' => return self.open(Frame::Array),
            b'"' => {
                self.pos += 1;
                Token::Str(self.string()?)
            }
            b't' => self.literal(b"true", Token::True)?,
            b'f' => self.literal(b"false", Token::False)?,
            b'n' => self.literal(b"null", Token::Null)?,
            b'-' | b'0'..=b'9' => Token::Number(self.number()?),
            _ => return Err(JsonError::Byte(b)),
        };
        self.state = State::AfterValue;
        Ok(token)
    }

    /// Reads a key with its colon.
    fn key(&mut self) -> Result<Token<'a>, JsonError> {
        let b = self.peek_token()?;
        if b != b'"' {
            return Err(JsonError::Byte(b));
        }
        self.pos += 1;
        let key = self.string()?;
        let colon = self.peek_token()?;
        if colon != b':' {
            return Err(JsonError::Byte(colon));
        }
        self.pos += 1;
        self.state = State::Value;
        Ok(Token::Key(key))
    }

    /// Opens a container at the current byte.
    fn open(&mut self, frame: Frame) -> Result<Token<'a>, JsonError> {
        if self.stack.len() >= self.max_depth {
            return Err(JsonError::Depth);
        }
        self.stack.push(frame);
        self.pos += 1;
        Ok(match frame {
            Frame::Object => {
                self.state = State::ObjectFirst;
                Token::BeginObject
            }
            Frame::Array => {
                self.state = State::ArrayFirst;
                Token::BeginArray
            }
        })
    }

    /// Closes the innermost container at the current byte, which the caller
    /// has matched against it.
    fn close(&mut self) -> Token<'a> {
        self.pos += 1;
        self.state = State::AfterValue;
        match self.stack.pop() {
            Some(Frame::Array) => Token::EndArray,
            _ => Token::EndObject,
        }
    }

    /// Skips whitespace and returns the byte after it.
    fn peek_token(&mut self) -> Result<u8, JsonError> {
        self.skip_whitespace();
        self.bytes.get(self.pos).copied().ok_or(JsonError::Eof)
    }

    /// Skips the four whitespace bytes of RFC 8259 section 2.
    fn skip_whitespace(&mut self) {
        while let Some(&b) = self.bytes.get(self.pos) {
            if !matches!(b, b' ' | b'\t' | b'\n' | b'\r') {
                break;
            }
            self.pos += 1;
        }
    }

    /// Reads one of the three literals.
    fn literal(&mut self, word: &[u8], token: Token<'a>) -> Result<Token<'a>, JsonError> {
        for &w in word {
            match self.bytes.get(self.pos) {
                None => return Err(JsonError::Eof),
                Some(&b) if b != w => return Err(JsonError::Byte(b)),
                Some(_) => self.pos += 1,
            }
        }
        Ok(token)
    }

    /// Reads a number by the grammar of RFC 8259 section 6 and returns its
    /// text.
    fn number(&mut self) -> Result<&'a str, JsonError> {
        let bytes: &'a [u8] = self.bytes;
        let start = self.pos;
        if self.at(b'-') {
            self.pos += 1;
        }
        match bytes.get(self.pos) {
            Some(b'0') => self.pos += 1,
            Some(b'1'..=b'9') => self.digits(),
            _ => return Err(JsonError::Number),
        }
        if self.at(b'.') {
            self.pos += 1;
            self.required_digits()?;
        }
        if self.at(b'e') || self.at(b'E') {
            self.pos += 1;
            if self.at(b'+') || self.at(b'-') {
                self.pos += 1;
            }
            self.required_digits()?;
        }
        // The grammar admits ASCII bytes alone, so the check cannot fail.
        core::str::from_utf8(&bytes[start..self.pos]).map_err(|_| JsonError::Number)
    }

    /// Returns whether the current byte is `b`.
    fn at(&self, b: u8) -> bool {
        self.bytes.get(self.pos) == Some(&b)
    }

    /// Skips a run of decimal digits.
    fn digits(&mut self) {
        while self.bytes.get(self.pos).is_some_and(u8::is_ascii_digit) {
            self.pos += 1;
        }
    }

    /// Skips a run of at least one decimal digit.
    fn required_digits(&mut self) -> Result<(), JsonError> {
        let start = self.pos;
        self.digits();
        if self.pos == start {
            return Err(JsonError::Number);
        }
        Ok(())
    }

    /// Reads a string after its opening quote, through its closing quote.
    fn string(&mut self) -> Result<Cow<'a, str>, JsonError> {
        let run = self.run()?;
        if self.at(b'"') {
            self.pos += 1;
            return Ok(Cow::Borrowed(run));
        }
        let mut out = String::from(run);
        while self.at(b'\\') {
            self.escape(&mut out)?;
            out.push_str(self.run()?);
        }
        // `run` stops at a quote or a backslash alone, so the byte is the
        // closing quote.
        self.pos += 1;
        Ok(Cow::Owned(out))
    }

    /// Reads the bytes up to the next quote or backslash as checked UTF-8.
    fn run(&mut self) -> Result<&'a str, JsonError> {
        let bytes: &'a [u8] = self.bytes;
        let start = self.pos;
        loop {
            match bytes.get(self.pos) {
                None => return Err(JsonError::Eof),
                Some(b'"' | b'\\') => break,
                Some(&b) if b < 0x20 => return Err(JsonError::Byte(b)),
                Some(_) => self.pos += 1,
            }
        }
        core::str::from_utf8(&bytes[start..self.pos]).map_err(|e| {
            self.pos = start + e.valid_up_to();
            JsonError::Utf8
        })
    }

    /// Decodes the escape at the current backslash into `out`.
    fn escape(&mut self, out: &mut String) -> Result<(), JsonError> {
        let b = *self.bytes.get(self.pos + 1).ok_or(JsonError::Eof)?;
        let c = match b {
            b'"' => '"',
            b'\\' => '\\',
            b'/' => '/',
            b'b' => '\u{8}',
            b'f' => '\u{c}',
            b'n' => '\n',
            b'r' => '\r',
            b't' => '\t',
            b'u' => {
                self.pos += 2;
                return self.unicode(out);
            }
            _ => return Err(JsonError::Escape),
        };
        self.pos += 2;
        out.push(c);
        Ok(())
    }

    /// Decodes the four digits of a `\u` escape, and the low half that
    /// follows a high surrogate, into `out`.
    ///
    /// A surrogate without its partner becomes U+FFFD, because a Rust string
    /// cannot hold it, and the escape after an unpaired high surrogate is left
    /// for the next call.
    fn unicode(&mut self, out: &mut String) -> Result<(), JsonError> {
        let unit = self.hex4(self.pos)?;
        self.pos += 4;
        let c = match unit {
            0xd800..=0xdbff => {
                let pair = self.bytes.get(self.pos..self.pos + 2) == Some(b"\\u");
                match pair.then(|| self.hex4(self.pos + 2)).transpose()? {
                    Some(low @ 0xdc00..=0xdfff) => {
                        self.pos += 6;
                        char::from_u32(0x10000 + ((unit - 0xd800) << 10) + (low - 0xdc00))
                    }
                    _ => None,
                }
            }
            _ => char::from_u32(unit),
        };
        out.push(c.unwrap_or(char::REPLACEMENT_CHARACTER));
        Ok(())
    }

    /// Reads four hexadecimal digits at `at` without moving.
    fn hex4(&self, at: usize) -> Result<u32, JsonError> {
        let mut unit = 0;
        for i in at..at + 4 {
            let b = *self.bytes.get(i).ok_or(JsonError::Eof)?;
            let digit = char::from(b).to_digit(16).ok_or(JsonError::Escape)?;
            unit = unit * 16 + digit;
        }
        Ok(unit)
    }
}
