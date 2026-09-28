// SPDX-License-Identifier: MIT
/*
 * graphql-over-http: JSON.parse as the V8 of Node 24.13.0 answers it, with the
 * messages of its SyntaxError.
 *
 * Derived from node v24.13.0, deps/v8/src/json/json-parser.cc, MIT.
 */

//! `json_parse`, `JSON.parse(text)` with V8's refusals.
//!
//! Yoga reads a JSON body with `request.json()`, which is
//! `JSON.parse(utf8DecodeBytes(bytes))`, and a query parameter with
//! `JSON.parse`, and the message of the `SyntaxError` V8 throws reaches the
//! wire as the `originalError` extension of the refusal of a JSON body. The
//! messages and the position they name are V8's.
//!
//! - A value that starts with a character no value starts with names that
//!   character and the source, the whole source when it is shorter than 21
//!   code units and a window of 10 code units on each side of the position
//!   otherwise, with `...` where the window cuts. The four sources
//!   `undefined`, `NaN`, `Infinity` and `[object Object]` are named whole.
//! - The end of the input where a value is due is `Unexpected end of JSON
//!   input`.
//! - Every other refusal names what was expected at the position, a UTF-16
//!   code unit offset, with its line and column, where `\r\n`, `\r` and `\n`
//!   each end a line.
//!
//! V8 runs over UTF-16 code units, and the parser runs over the bytes of the
//! text, which its caller checked as UTF-8 once. Every byte the grammar reads
//! outside a string is ASCII, and a string copies each run of its bytes up to a
//! quotation mark, a backslash or a control byte as it stands, found a word at
//! a time by [`aosabi_json::word::string_stops`]. An escape is decoded in
//! place, and a `\u` escape of a surrogate joins the escape of its partner
//! after it. A string that holds a lone surrogate takes the WTF-8 path: it is
//! read again from its start as code units, which [`JsString::from_utf16`]
//! keeps. A number is read from its own bytes in the pass that checks its
//! grammar. A refusal stands on the first byte of a character, so its position
//! counts the code units of the bytes before it, one for each character and two
//! for a character outside the basic plane, which is the offset V8 prints, and
//! a character outside the basic plane is named by its high surrogate. A text
//! that parses counts no code unit.
//!
//! The parser holds its open containers on a stack of its own, as V8 does,
//! and refuses a document nested deeper than [`aosabi_json::reader::MAX_DEPTH`]
//! with the `RangeError` V8 throws when its stack runs out, a limit of Aosabi,
//! because a value nested deeper is released one level per frame of the Rust
//! stack.

use aosabi_json::{
    JsObject,
    JsString,
    JsValue,
    reader::MAX_DEPTH,
    word, //
};

/// A throw of `JSON.parse`.
#[derive(Clone, Debug, PartialEq)]
pub struct JsonParseError {
    /// The class of the error, `SyntaxError` or `RangeError`.
    pub name: &'static str,
    /// The message.
    pub message: JsString,
}

/// The token a code unit starts, `JsonToken` of V8.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Token {
    /// The end of the input.
    Eos,
    /// `"`.
    String,
    /// A digit or `-`.
    Number,
    /// `t`.
    True,
    /// `f`.
    False,
    /// `n`.
    Null,
    /// A space, a tab, a carriage return or a line feed.
    Whitespace,
    /// `:`.
    Colon,
    /// `,`.
    Comma,
    /// `[`.
    LBrack,
    /// `]`.
    RBrack,
    /// `{`.
    LBrace,
    /// `}`.
    RBrace,
    /// Any other code unit.
    Illegal,
}

/// The token each byte starts, `one_char_json_tokens`, read by [`token_of`].
const TOKENS: [Token; 256] = tokens();

/// Returns the table of [`TOKENS`].
///
/// Every token but `Illegal` is an ASCII character, so a byte above ASCII,
/// which starts or continues a character above ASCII, is `Illegal`, as the
/// code unit of that character is in V8.
const fn tokens() -> [Token; 256] {
    let mut table = [Token::Illegal; 256];
    let mut digit = 0x30;
    while digit <= 0x39 {
        table[digit] = Token::Number;
        digit += 1;
    }
    table[0x22] = Token::String;
    table[0x2D] = Token::Number;
    table[0x74] = Token::True;
    table[0x66] = Token::False;
    table[0x6E] = Token::Null;
    table[0x20] = Token::Whitespace;
    table[0x09] = Token::Whitespace;
    table[0x0D] = Token::Whitespace;
    table[0x0A] = Token::Whitespace;
    table[0x3A] = Token::Colon;
    table[0x2C] = Token::Comma;
    table[0x5B] = Token::LBrack;
    table[0x5D] = Token::RBrack;
    table[0x7B] = Token::LBrace;
    table[0x7D] = Token::RBrace;
    table
}

/// Returns the token the byte `byte` starts, `one_char_json_tokens`.
#[inline]
fn token_of(byte: u8) -> Token {
    TOKENS[usize::from(byte)]
}

/// The context width of a message, `kMaxContextCharacters`.
const MAX_CONTEXT_CHARACTERS: usize = 10;

/// The shortest source a message cuts a window from, `kMinOriginalSourceLengthForContext`.
const MIN_ORIGINAL_SOURCE_LENGTH_FOR_CONTEXT: usize = MAX_CONTEXT_CHARACTERS * 2 + 1;

/// The quotation mark.
const QUOTE: u8 = 0x22;

/// The backslash.
const BACKSLASH: u8 = 0x5C;

/// The bytes a string with an escape reserves beyond its first run.
const ESCAPED_RESERVE: usize = 16;

/// An open container.
enum Frame {
    /// An array and its elements so far.
    Array(Vec<JsValue>),
    /// An object, its properties so far and the key of the value being parsed.
    Object(JsObject, JsString),
}

/// The cursor over the bytes of the source.
struct Parser<'a> {
    /// The source.
    text: &'a str,
    /// The byte offset of the next byte.
    cursor: usize,
}

/// Parses `text` as `JSON.parse(text)` does.
///
/// # Errors
///
/// Returns the `SyntaxError` V8 throws, and the `RangeError` of a document
/// nested deeper than [`MAX_DEPTH`].
pub fn json_parse(text: &str) -> Result<JsValue, JsonParseError> {
    let mut parser = Parser { text, cursor: 0 };
    let value = parser.parse_value()?;
    if parser.peek() != Token::Eos {
        return Err(parser.expected("Unexpected non-whitespace character after JSON"));
    }
    Ok(value)
}

impl Parser<'_> {
    /// Returns the bytes of the source.
    fn bytes(&self) -> &[u8] {
        self.text.as_bytes()
    }

    /// Returns the byte at the cursor.
    // Always inlined, since at opt-level z a hint leaves it a call in the loops
    // of `Parser::scan_string`, `Parser::scan_escape` and
    // `Parser::scan_literal`.
    #[expect(
        clippy::inline_always,
        reason = "a hint inlines nothing into a caller built at opt-level z"
    )]
    #[inline(always)]
    fn current(&self) -> Option<u8> {
        self.bytes().get(self.cursor).copied()
    }

    /// Returns the text of the bytes from `start` up to `end`.
    ///
    /// Each end stands on an ASCII byte, right after one or at the end of the
    /// source, so each is the boundary of a character and the text holds whole
    /// characters.
    // A `from_utf8_unchecked` over the span would skip its two boundary checks.
    // Always inlined, since at opt-level z a hint leaves it a call in the loops
    // of `Parser::scan_string` and `json_parse`.
    #[expect(
        clippy::inline_always,
        reason = "a hint inlines nothing into a caller built at opt-level z"
    )]
    #[inline(always)]
    fn slice(&self, start: usize, end: usize) -> &str {
        self.text.get(start..end).unwrap_or_default()
    }

    /// Skips whitespace and returns the token at the cursor, `SkipWhitespace` and `peek`.
    fn peek(&mut self) -> Token {
        while let Some(byte) = self.current() {
            let token = token_of(byte);
            if token != Token::Whitespace {
                return token;
            }
            self.cursor += 1;
        }
        Token::Eos
    }

    /// Returns the code unit offset, the line and the column of the cursor, `CalculateFileLocation`.
    ///
    /// The cursor stands on the first byte of a character, and each byte
    /// before it adds the code units it starts: one for an ASCII byte and for
    /// the lead of a character of two or three bytes, none for a continuation
    /// byte and two for the lead of a character of four bytes. So the offset
    /// and the column are the ones V8 counts over code units, and `\r\n`
    /// counts as a single newline.
    fn location(&self) -> (usize, usize, usize) {
        let bytes = self.bytes().get(..self.cursor).unwrap_or_default();
        let mut line = 1;
        let mut units = 0;
        let mut line_start = 0;
        let mut i = 0;
        while let Some(&byte) = bytes.get(i) {
            if byte == b'\r' && bytes.get(i + 1) == Some(&b'\n') {
                i += 1;
                units += 1;
            }
            let byte = bytes.get(i).copied().unwrap_or_default();
            units += units_started_by(byte);
            if byte == b'\r' || byte == b'\n' {
                line += 1;
                line_start = units;
            }
            i += 1;
        }
        (units, line, 1 + units - line_start)
    }

    /// Returns the refusal whose template names the position, `ReportUnexpectedToken(token, message)`.
    fn expected(&self, what: &str) -> JsonParseError {
        let (position, line, column) = self.location();
        syntax_error(JsString::from(format!(
            "{what} at position {position} (line {line} column {column})"
        )))
    }

    /// Returns the refusal of the unexpected byte `byte`, `ReportUnexpectedCharacter(c)`.
    fn unexpected_character(&self, byte: Option<u8>) -> JsonParseError {
        match byte {
            None => self.unexpected_token(Token::Eos),
            Some(byte) => self.unexpected_token(token_of(byte)),
        }
    }

    /// Returns the refusal of `token` with the message its kind looks up, `LookUpErrorMessageForJsonToken`.
    fn unexpected_token(&self, token: Token) -> JsonParseError {
        match token {
            Token::Eos => syntax_error(JsString::from("Unexpected end of JSON input")),
            Token::Number => self.expected("Unexpected number in JSON"),
            Token::String => self.expected("Unexpected string in JSON"),
            _ => self.unexpected_with_context(),
        }
    }

    /// Returns the refusal that names the code unit at the cursor and the source around it.
    ///
    /// V8 names one code unit and cuts its window in code units, which may
    /// part a surrogate pair, so the message is built from the code units of
    /// the source, on the path of this refusal alone.
    fn unexpected_with_context(&self) -> JsonParseError {
        if is_special_string(self.text) {
            return syntax_error(JsString::from(format!(
                "\"{}\" is not valid JSON",
                self.text
            )));
        }
        let source: Vec<u16> = self.text.encode_utf16().collect();
        let (pos, _, _) = self.location();
        let token = source.get(pos).copied().unwrap_or_default();
        let mut message: Vec<u16> = "Unexpected token '".encode_utf16().collect();
        message.push(token);
        message.extend("', ".encode_utf16());
        let length = source.len();
        if length < MIN_ORIGINAL_SOURCE_LENGTH_FOR_CONTEXT {
            message.push(0x22);
            message.extend_from_slice(&source);
            message.extend("\" is not valid JSON".encode_utf16());
        } else if pos < MAX_CONTEXT_CHARACTERS {
            message.push(0x22);
            message.extend_from_slice(&source[..pos + MAX_CONTEXT_CHARACTERS]);
            message.extend("\"... is not valid JSON".encode_utf16());
        } else if pos < length - MAX_CONTEXT_CHARACTERS {
            message.extend("...\"".encode_utf16());
            message.extend_from_slice(
                &source[pos - MAX_CONTEXT_CHARACTERS..pos + MAX_CONTEXT_CHARACTERS],
            );
            message.extend("\"... is not valid JSON".encode_utf16());
        } else {
            message.extend("...\"".encode_utf16());
            message.extend_from_slice(&source[pos - MAX_CONTEXT_CHARACTERS..]);
            message.extend("\" is not valid JSON".encode_utf16());
        }
        syntax_error(JsString::from_utf16(&message))
    }

    /// Parses one value with its containers, `ParseJsonValue`.
    fn parse_value(&mut self) -> Result<JsValue, JsonParseError> {
        let mut stack: Vec<Frame> = Vec::new();
        loop {
            // Produce a JSON value.
            let mut value = loop {
                match self.peek() {
                    Token::String => {
                        self.cursor += 1;
                        break JsValue::String(self.scan_string()?);
                    }
                    Token::Number => break self.parse_number()?,
                    Token::LBrace => {
                        self.cursor += 1;
                        check_depth(&stack)?;
                        if self.peek() == Token::RBrace {
                            self.cursor += 1;
                            break JsValue::Object(JsObject::new());
                        }
                        let key =
                            self.scan_property_key("Expected property name or '}' in JSON")?;
                        stack.push(Frame::Object(JsObject::new(), key));
                    }
                    Token::LBrack => {
                        self.cursor += 1;
                        check_depth(&stack)?;
                        if self.peek() == Token::RBrack {
                            self.cursor += 1;
                            break JsValue::Array(Vec::new());
                        }
                        stack.push(Frame::Array(Vec::new()));
                    }
                    Token::True => {
                        self.scan_literal("true")?;
                        break JsValue::Bool(true);
                    }
                    Token::False => {
                        self.scan_literal("false")?;
                        break JsValue::Bool(false);
                    }
                    Token::Null => {
                        self.scan_literal("null")?;
                        break JsValue::Null;
                    }
                    _ => return Err(self.unexpected_character(self.current())),
                }
            };
            // Consume a produced JSON value. The open container stays on the
            // stack while it takes a value and leaves it when it closes.
            loop {
                match stack.last_mut() {
                    None => return Ok(value),
                    Some(Frame::Object(object, key)) => {
                        object.insert(core::mem::take(key), value);
                        if self.peek() == Token::Comma {
                            self.cursor += 1;
                            *key = self.scan_property_key(
                                "Expected double-quoted property name in JSON",
                            )?;
                            break;
                        }
                        if self.peek() != Token::RBrace {
                            return Err(
                                self.expected("Expected ',' or '}' after property value in JSON")
                            );
                        }
                    }
                    Some(Frame::Array(items)) => {
                        items.push(value);
                        if self.peek() == Token::Comma {
                            self.cursor += 1;
                            break;
                        }
                        if self.peek() != Token::RBrack {
                            return Err(
                                self.expected("Expected ',' or ']' after array element in JSON")
                            );
                        }
                    }
                }
                self.cursor += 1;
                value = match stack.pop() {
                    Some(Frame::Object(object, _)) => JsValue::Object(object),
                    Some(Frame::Array(items)) => JsValue::Array(items),
                    None => JsValue::Undefined,
                };
            }
        }
    }

    /// Reads a property key and the colon after it, the `ExpectNext` pair of V8.
    fn scan_property_key(&mut self, missing: &str) -> Result<JsString, JsonParseError> {
        if self.peek() != Token::String {
            return Err(self.expected(missing));
        }
        self.cursor += 1;
        let key = self.scan_string()?;
        if self.peek() != Token::Colon {
            return Err(self.expected("Expected ':' after property name in JSON"));
        }
        self.cursor += 1;
        Ok(key)
    }

    /// Reads a string after its opening quote, `ScanJsonString`.
    ///
    /// A string with no escape is its first run, copied in one allocation of
    /// its length, and any other string goes on in [`Parser::scan_escaped`].
    fn scan_string(&mut self) -> Result<JsString, JsonParseError> {
        let start = self.cursor;
        self.cursor = run_end(self.bytes(), start);
        if self.current() == Some(QUOTE) {
            let text = JsString::from(self.slice(start, self.cursor));
            self.cursor += 1;
            return Ok(text);
        }
        self.scan_escaped(start)
    }

    /// Reads on a string that starts at `start` and whose run at the cursor ends at a byte other than its quote.
    ///
    /// Each run is appended as it stands and each escape decoded in place, and
    /// a string that holds a lone surrogate goes to [`Parser::scan_units`].
    fn scan_escaped(&mut self, start: usize) -> Result<JsString, JsonParseError> {
        let mut text = String::with_capacity(self.cursor - start + ESCAPED_RESERVE);
        let mut run = start;
        loop {
            match self.current() {
                None => return Err(self.expected("Unterminated string in JSON")),
                Some(QUOTE) => {
                    text.push_str(self.slice(run, self.cursor));
                    self.cursor += 1;
                    return Ok(JsString::from(text));
                }
                Some(BACKSLASH) => {
                    text.push_str(self.slice(run, self.cursor));
                    self.cursor += 1;
                    let unit = self.scan_escape()?;
                    if !self.push_unit(&mut text, unit) {
                        return self.scan_units(start);
                    }
                    self.cursor += 1;
                }
                Some(_) => {
                    return Err(self.expected("Bad control character in string literal in JSON"));
                }
            }
            run = self.cursor;
            self.cursor = run_end(self.bytes(), run);
        }
    }

    /// Reads a string that holds a lone surrogate again from its start `start`, as code units, the WTF-8 path.
    ///
    /// [`JsString::from_utf16`] keeps a lone surrogate, which a `String`
    /// cannot hold, and joins the escapes of a pair.
    fn scan_units(&mut self, start: usize) -> Result<JsString, JsonParseError> {
        let mut units: Vec<u16> = Vec::new();
        self.cursor = start;
        loop {
            let end = run_end(self.bytes(), self.cursor);
            units.extend(self.slice(self.cursor, end).encode_utf16());
            self.cursor = end;
            match self.current() {
                None => return Err(self.expected("Unterminated string in JSON")),
                Some(QUOTE) => {
                    self.cursor += 1;
                    return Ok(JsString::from_utf16(&units));
                }
                Some(BACKSLASH) => {
                    self.cursor += 1;
                    units.push(self.scan_escape()?);
                    self.cursor += 1;
                }
                Some(_) => {
                    return Err(self.expected("Bad control character in string literal in JSON"));
                }
            }
        }
    }

    /// Reads the escape whose backslash stands before the cursor and returns its code unit, leaving the cursor on its last byte.
    ///
    /// A character above U+00FF after the backslash is an unexpected token,
    /// and any other character no escape names is a bad escape, as V8 looks
    /// the code unit up among the escapes when it is at most 0xFF. Such a
    /// character of two bytes starts with 0xC2 or 0xC3, and every other
    /// character above ASCII with a byte from 0xC4.
    fn scan_escape(&mut self) -> Result<u16, JsonParseError> {
        let Some(escape) = self.current() else {
            return Err(self.unexpected_character(None));
        };
        match escape {
            QUOTE | BACKSLASH | b'/' => Ok(u16::from(escape)),
            b'b' => Ok(0x08),
            b'f' => Ok(0x0C),
            b'n' => Ok(0x0A),
            b'r' => Ok(0x0D),
            b't' => Ok(0x09),
            b'u' => self.scan_unicode_character(),
            0xC4.. => Err(self.unexpected_character(Some(escape))),
            _ => Err(self.expected("Bad escaped character in JSON")),
        }
    }

    /// Appends the character of the escaped code unit `unit` to `text` and returns whether it is one.
    ///
    /// A high surrogate whose escape the escape of a low surrogate follows is
    /// the code point of the pair, and the cursor moves to the last byte of
    /// the second escape. A lone surrogate is no character.
    fn push_unit(&mut self, text: &mut String, unit: u16) -> bool {
        if let Some(c) = char::from_u32(u32::from(unit)) {
            text.push(c);
            return true;
        }
        let Some(low) = self.escaped_low_after() else {
            return false;
        };
        let Some(Ok(c)) = char::decode_utf16([unit, low]).next() else {
            return false;
        };
        text.push(c);
        self.cursor += 6;
        true
    }

    /// Returns the low surrogate the `\u` escape after the cursor names, if one stands there.
    ///
    /// The cursor stands on the last digit of an escape, and nothing is read
    /// as an error here, so a malformed escape after it is refused where the
    /// scan reaches it.
    fn escaped_low_after(&self) -> Option<u16> {
        let next = self.bytes().get(self.cursor + 1..self.cursor + 7)?;
        let [BACKSLASH, b'u', digits @ ..] = next else {
            return None;
        };
        let unit = digits
            .iter()
            .try_fold(0u16, |value, &byte| Some(value * 16 + hex_digit(byte)?))?;
        (0xDC00..=0xDFFF).contains(&unit).then_some(unit)
    }

    /// Reads the four hex digits of a `\u` escape, `ScanUnicodeCharacter`, and leaves the cursor on the last one.
    fn scan_unicode_character(&mut self) -> Result<u16, JsonParseError> {
        let mut value: u16 = 0;
        for _ in 0..4 {
            self.cursor += 1;
            let Some(digit) = self.current().and_then(hex_digit) else {
                return Err(self.expected("Bad Unicode escape in JSON"));
            };
            value = value * 16 + digit;
        }
        Ok(value)
    }

    /// Reads the literal `word` whose first character the token matched, `ScanLiteral`.
    fn scan_literal(&mut self, word: &str) -> Result<(), JsonParseError> {
        let word = word.as_bytes();
        let rest = self.bytes().get(self.cursor..).unwrap_or_default();
        if rest.starts_with(word) {
            self.cursor += word.len();
            return Ok(());
        }
        let remaining = rest.len();
        self.cursor += 1;
        for &expected in word.iter().skip(1).take(remaining - 1) {
            if Some(expected) != self.current() {
                return Err(self.unexpected_character(self.current()));
            }
            self.cursor += 1;
        }
        Err(self.unexpected_token(Token::Eos))
    }

    /// Reads a number, `ParseJsonNumber`, from its own bytes.
    ///
    /// The scan that checks the grammar of the number reads its digits into a
    /// [`Decimal`] in the same pass, over an offset of its own, and the double
    /// is the one [`Decimal::exact`] reads, or the one `f64::from_str` reads
    /// of the text of the number, which allocates nothing either.
    // Out of line, so a body with no number compiles no read of one.
    #[inline(never)]
    fn parse_number(&mut self) -> Result<JsValue, JsonParseError> {
        let bytes = self.text.as_bytes();
        let start = self.cursor;
        let negative = bytes.get(start) == Some(&b'-');
        let mut at = start + usize::from(negative);
        let mut number = Decimal::default();
        if bytes.get(at) == Some(&b'0') {
            // Prefix zero is only allowed if it's the only digit before a
            // decimal point or exponent.
            at += 1;
            match bytes.get(at) {
                Some(b'0'..=b'9') => {
                    self.cursor = at;
                    return Err(self.unexpected_token(Token::Number));
                }
                Some(b'.' | b'E' | b'e' | b'+' | b'-') => {}
                _ if !negative => {
                    self.cursor = at;
                    return Ok(JsValue::Number(0.0));
                }
                _ => {}
            }
        } else {
            let digits = at;
            at = number.read_digits(bytes, at);
            if at == digits {
                self.cursor = at;
                return Err(self.expected("No number after minus sign in JSON"));
            }
        }
        if bytes.get(at) == Some(&b'.') {
            at += 1;
            if !bytes.get(at).is_some_and(u8::is_ascii_digit) {
                self.cursor = at;
                return Err(self.expected("Unterminated fractional number in JSON"));
            }
            at = number.read_fraction(bytes, at);
        }
        if matches!(bytes.get(at), Some(b'E' | b'e')) {
            at += 1;
            let negative_exponent = bytes.get(at) == Some(&b'-');
            if matches!(bytes.get(at), Some(b'+' | b'-')) {
                at += 1;
            }
            if !bytes.get(at).is_some_and(u8::is_ascii_digit) {
                self.cursor = at;
                return Err(self.expected("Exponent part is missing a number in JSON"));
            }
            at = number.read_exponent(bytes, at, negative_exponent);
        }
        self.cursor = at;
        let value = number
            .exact()
            .map(|magnitude| if negative { -magnitude } else { magnitude });
        Ok(JsValue::Number(value.unwrap_or_else(|| {
            self.slice(start, at).parse::<f64>().unwrap_or(f64::NAN)
        })))
    }
}

/// The digits of a number read so far, a mantissa and the power of ten that scales it.
///
/// The mantissa takes every digit it reads, and a number of more than
/// [`MAX_DIGITS`] digits, whose mantissa may have wrapped, is left to
/// `f64::from_str`, as is a mantissa or a power of ten outside the fast path
/// of [`Decimal::exact`].
#[derive(Default)]
struct Decimal {
    /// The digits read so far, modulo 2^64.
    mantissa: u64,
    /// How many digits the mantissa took.
    digits: usize,
    /// The power of ten that scales the mantissa.
    exponent: i32,
}

impl Decimal {
    /// Reads the digits of `bytes` from `at` into the mantissa and returns the offset after them, `AdvanceToNonDecimal`.
    fn read_digits(&mut self, bytes: &[u8], at: usize) -> usize {
        let mut end = at;
        while let Some(digit) = bytes.get(end).and_then(|byte| digit_of(*byte)) {
            self.mantissa = self
                .mantissa
                .wrapping_mul(10)
                .wrapping_add(u64::from(digit));
            end += 1;
        }
        self.digits += end - at;
        end
    }

    /// Reads the digits of a fraction from `at`, each scaling the mantissa down by ten, and returns the offset after them.
    fn read_fraction(&mut self, bytes: &[u8], at: usize) -> usize {
        let end = self.read_digits(bytes, at);
        let scale = i32::try_from(end - at).unwrap_or(i32::MAX);
        self.exponent = self.exponent.saturating_sub(scale);
        end
    }

    /// Reads the digits of an exponent from `at`, negative when `negative` holds, and returns the offset after them.
    fn read_exponent(&mut self, bytes: &[u8], mut at: usize, negative: bool) -> usize {
        let mut value: i32 = 0;
        while let Some(digit) = bytes.get(at).and_then(|byte| digit_of(*byte)) {
            value = value.saturating_mul(10).saturating_add(i32::from(digit));
            at += 1;
        }
        let value = if negative { -value } else { value };
        self.exponent = self.exponent.saturating_add(value);
        at
    }

    /// Returns the magnitude of the number when one rounding reaches it, or `None`.
    ///
    /// A mantissa of at most 2^53 and a power of ten from 10^-22 to 10^22 are
    /// both exact in a double, so the product, or the quotient by the power for
    /// a negative exponent, is rounded once and is the double nearest to the
    /// number, the fast path of Clinger. V8 reads the double nearest to the
    /// number too, and every other number takes `f64::from_str`, which rounds
    /// to the nearest as well. A mantissa of zero is a zero, whatever its
    /// exponent.
    ///
    /// [round]: https://tc39.es/ecma262/#sec-roundmvresult
    fn exact(&self) -> Option<f64> {
        if self.digits > MAX_DIGITS {
            return None;
        }
        if self.mantissa == 0 {
            return Some(0.0);
        }
        if self.mantissa > EXACT_MANTISSA {
            return None;
        }
        let power = *POWERS_OF_TEN.get(usize::try_from(self.exponent.unsigned_abs()).ok()?)?;
        Some(if self.exponent < 0 {
            exact_double(self.mantissa) / power
        } else {
            exact_double(self.mantissa) * power
        })
    }
}

/// Returns the value of the decimal digit `byte`, if it is one.
fn digit_of(byte: u8) -> Option<u8> {
    byte.checked_sub(b'0').filter(|digit| *digit < 10)
}

/// Returns the offset of the first byte of `bytes` at or after `from` that ends a run of a string, or the length of `bytes`.
///
/// A run ends at a quotation mark, a backslash or a control byte. The bytes
/// are read a word at a time while whole words remain, and the lowest lane
/// the word test marks is the first such byte, so a byte above ASCII passes
/// in the word as it stands. The bytes after the last whole word are read one
/// at a time.
// An `unsafe` vector scan would test 16 bytes a step. An unaligned word read
// through a raw pointer would read the last bytes as a word too.
fn run_end(bytes: &[u8], from: usize) -> usize {
    let rest = bytes.get(from..).unwrap_or_default();
    let (words, tail) = rest.as_chunks::<{ word::BYTES }>();
    for (at, chunk) in words.iter().enumerate() {
        let stops = word::string_stops(word::load(chunk));
        if stops != 0 {
            return from + at * word::BYTES + word::first_lane(stops);
        }
    }
    let start = from + words.len() * word::BYTES;
    tail.iter()
        .position(|&byte| byte == QUOTE || byte == BACKSLASH || byte < 0x20)
        .map_or(bytes.len(), |at| start + at)
}

/// The powers of ten a double holds exactly, 10^0 to 10^22.
const POWERS_OF_TEN: [f64; 23] = [
    1e0, 1e1, 1e2, 1e3, 1e4, 1e5, 1e6, 1e7, 1e8, 1e9, 1e10, 1e11, 1e12, 1e13, 1e14, 1e15, 1e16,
    1e17, 1e18, 1e19, 1e20, 1e21, 1e22,
];

/// The bound up to which a double holds every integer exactly, 2^53.
const EXACT_MANTISSA: u64 = 1 << 53;

/// The most digits a `u64` holds whatever they are, 19.
const MAX_DIGITS: usize = 19;

/// Returns the double of the mantissa `mantissa`, which is at most [`EXACT_MANTISSA`].
#[expect(
    clippy::cast_precision_loss,
    reason = "a mantissa of at most 2^53 is exact in a double"
)]
fn exact_double(mantissa: u64) -> f64 {
    mantissa as f64
}

/// Returns the UTF-16 code units the byte `byte` of UTF-8 starts.
fn units_started_by(byte: u8) -> usize {
    match byte {
        0x80..=0xBF => 0,
        0xF0.. => 2,
        _ => 1,
    }
}

/// Returns the value of the hex digit `byte`, if it is one.
///
/// A byte above ASCII is no digit, as V8 reads no digit outside ASCII. The
/// three ranges are the digits `char::to_digit` of radix 16 reads.
// Always inlined, since at opt-level z a hint leaves it a call in the loops of
// `Parser::scan_string` and `Parser::scan_escape`, and a match of the byte,
// since the call of `char::to_digit` stays a call there as well.
#[expect(
    clippy::inline_always,
    reason = "a hint inlines nothing into a caller built at opt-level z"
)]
#[inline(always)]
fn hex_digit(byte: u8) -> Option<u16> {
    let digit = match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        b'A'..=b'F' => byte - b'A' + 10,
        _ => return None,
    };
    Some(u16::from(digit))
}

/// Returns whether the source is one of the four a message names whole, `IsSpecialString`.
fn is_special_string(source: &str) -> bool {
    ["NaN", "Infinity", "undefined", "[object Object]"].contains(&source)
}

/// Returns the `SyntaxError` of the message `message`.
fn syntax_error(message: JsString) -> JsonParseError {
    JsonParseError {
        name: "SyntaxError",
        message,
    }
}

/// Refuses a container, empty or not, that would open deeper than [`MAX_DEPTH`].
///
/// The stack holds the containers open around it, so the container opens at
/// the level one past the length of the stack.
fn check_depth(stack: &[Frame]) -> Result<(), JsonParseError> {
    if stack.len() >= MAX_DEPTH {
        return Err(JsonParseError {
            name: "RangeError",
            message: JsString::from("Maximum call stack size exceeded"),
        });
    }
    Ok(())
}
