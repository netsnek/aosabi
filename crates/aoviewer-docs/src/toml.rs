// SPDX-License-Identifier: MIT
/*
 * aoviewer-docs toml: the TOML subset aosabi.docs.toml is read with.
 *
 * The subset is comments, bare keys, basic strings with their escapes,
 * booleans, arrays of strings and table headers, of which aosabi.docs.toml
 * takes the one [clusters], and anything else is refused with its line and
 * column.
 */

//! The TOML subset.
//!
//! [`parse`] reads a document of [TOML 1.0.0] that keeps to the forms below,
//! each as the specification defines it, so a document the subset takes reads
//! to the same keys and values under the full language.
//!
//! - A comment runs from `#` to the end of its line and holds no control
//!   character but tab.
//! - A pair is `key = value` with a bare key of `A-Za-z0-9_-`, and its
//!   value starts on the line of its key.
//! - A table header is `[name]` with one bare key, and the pairs after it
//!   up to the next header belong to that table. The pairs before the first
//!   header belong to the root.
//! - A value is a basic string in double quotes, `true`, `false` or an array
//!   of basic strings. An array may span lines and hold comments, and may end
//!   with a comma.
//! - A basic string holds no control character but tab, and takes the
//!   escapes `\b`, `\t`, `\n`, `\f`, `\r`, `\"`, `\\`, `\uXXXX` and
//!   `\UXXXXXXXX` of a Unicode scalar value.
//! - A line ends with a line feed or a carriage return and a line feed, and
//!   a blank is a space or a tab.
//!
//! Anything else is refused at its line and column, and a form TOML has and
//! the subset leaves out is named, such as an integer, a literal string, an
//! inline table or a dotted key. So is a key or a table defined twice, which
//! TOML refuses as well, and a byte order mark, which the grammar of TOML has
//! no place for.
//!
//! A line counts from 1 at each line feed, and a column counts the Unicode
//! scalar values of its line from 1, as the reader of Python's standard
//! library counts the characters of a `str`.
//!
//! [`parse`] refuses a document longer than [`MAX_BYTES`] before it parses a
//! byte of it. The refusal names the line and the column of the byte at the
//! cap, so it scans the first [`MAX_BYTES`] bytes for them and no byte past
//! them. Every loop of the reader moves forward by at least one byte, so
//! each runs at most once per byte. A key or a table defined twice is found
//! by a scan over the keys of its table or over the root, and a document
//! holds at most [`MAX_KEYS`] keys and headers, so no scan reads more than
//! that.
//!
//! [TOML 1.0.0]: https://toml.io/en/v1.0.0
//! [ABNF of TOML 1.0.0]: https://github.com/toml-lang/toml/blob/1.0.0/toml.abnf
//! [`Lib/tomllib/_parser.py`]: https://github.com/python/cpython/blob/v3.14.3/Lib/tomllib/_parser.py

use core::{
    fmt,
    str, //
};

use crate::error::Position;

/// The longest document [`parse`] reads, in bytes, 1 MiB.
///
/// `aosabi.docs.toml` holds twelve keys and the table of the declared
/// clusters, a few hundred bytes, as the doc of [`MAX_QUERY_BYTES`] assumes
/// for the declared clusters of a configuration file. The cap equals the
/// longest query form of the options of a tree, [`MAX_QUERY_BYTES`], which
/// carries the same options, and lies far above any such file.
///
/// [`MAX_QUERY_BYTES`]: aoviewer_core::tree::MAX_QUERY_BYTES
pub const MAX_BYTES: usize = 1 << 20;

/// The most keys and table headers one document holds.
///
/// `aosabi.docs.toml` takes twelve keys and one table of at most 256 declared
/// clusters, the most the core reads, so 1024 leaves room for both.
pub const MAX_KEYS: usize = 1024;

/// A document of the subset.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Document {
    /// The pairs before the first table header, in the order of the file.
    pub root: Vec<Entry>,
    /// The tables, in the order of their headers.
    pub tables: Vec<Table>,
}

/// One table of a document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Table {
    /// The name of its header.
    pub name: String,
    /// The place of the `[` of its header.
    pub at: Position,
    /// Its pairs, in the order of the file.
    pub entries: Vec<Entry>,
}

/// One pair of a key and a value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The key.
    pub key: String,
    /// The place of its first character.
    pub at: Position,
    /// The value.
    pub value: Value,
    /// The place of the first character of the value.
    pub value_at: Position,
}

/// One value of the subset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    /// A basic string, its escapes decoded.
    String(String),
    /// `true` or `false`.
    Boolean(bool),
    /// An array of basic strings.
    Array(Vec<Item>),
}

/// One basic string of an array.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    /// The string, its escapes decoded.
    pub text: String,
    /// The place of its opening quote.
    pub at: Position,
}

/// A document the subset refuses, with the place it stops at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    /// The place of the first character the reader refuses.
    pub at: Position,
    /// Why it refuses the document there.
    pub reason: Reason,
}

/// Why the subset refuses a document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reason {
    /// The document is longer than [`MAX_BYTES`].
    TooLong,
    /// The bytes are no UTF-8.
    Utf8,
    /// A comment or a string holds a control character other than tab.
    Control(char),
    /// A character, or the end of the document when `found` is `None`,
    /// stands where the grammar expects another.
    Unexpected {
        /// What stands there.
        found: Option<char>,
        /// What the grammar expects there.
        expected: Expected,
    },
    /// A backslash escape TOML does not define, or one of a code point that
    /// is no Unicode scalar value.
    Escape,
    /// A form of TOML the subset leaves out.
    Outside(Form),
    /// A key, or the name of a table, defined a second time.
    Duplicate {
        /// The key or the name.
        key: String,
        /// The place of its first definition.
        first: Position,
    },
    /// The document holds more than [`MAX_KEYS`] keys and headers.
    TooManyKeys,
}

/// What the grammar expects where a document is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Expected {
    /// A key, a table header, a comment or the end of the line.
    Statement,
    /// The `=` after a key.
    Equals,
    /// A value.
    Value,
    /// The end of the line, or a comment before it.
    LineEnd,
    /// The bare key of a table header.
    Key,
    /// The `]` of a table header.
    HeaderEnd,
    /// A basic string or the `]` of an array.
    Item,
    /// The `,` between two items or the `]` of an array.
    Separator,
    /// The closing quote of a basic string, on its line.
    Quote,
}

/// A form of TOML the subset leaves out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Form {
    /// An integer, a float or a date.
    NumberOrDate,
    /// A string in single quotes.
    LiteralString,
    /// A string in three quotes.
    MultiLineString,
    /// A table in braces.
    InlineTable,
    /// A key of parts joined by dots.
    DottedKey,
    /// A key in quotes.
    QuotedKey,
    /// A header in double brackets.
    ArrayOfTables,
    /// An array inside an array.
    NestedArray,
    /// A boolean inside an array.
    BooleanItem,
}

impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLong => write!(f, "the file is longer than {MAX_BYTES} bytes"),
            Self::Utf8 => f.write_str("the bytes are no UTF-8"),
            Self::Control(c) => write!(
                f,
                "the control character {c:?} stands in a comment or a string, where TOML allows none but tab"
            ),
            Self::Unexpected {
                found: Some(c),
                expected,
            } => write!(f, "{c:?} stands where {} belongs", expected.text()),
            Self::Unexpected {
                found: None,
                expected,
            } => write!(f, "the file ends where {} belongs", expected.text()),
            Self::Escape => f.write_str(
                "the escape is none of \\b, \\t, \\n, \\f, \\r, \\\", \\\\, \\uXXXX and \\UXXXXXXXX of a Unicode scalar value",
            ),
            Self::Outside(form) => write!(
                f,
                "{} lies outside the TOML subset, which takes comments, bare keys, basic strings, booleans, arrays of basic strings and table headers",
                form.text()
            ),
            Self::Duplicate { key, first } => write!(
                f,
                "{key} is defined a second time, first at {}:{}",
                first.line, first.column
            ),
            Self::TooManyKeys => write!(f, "the file defines more than {MAX_KEYS} keys and tables"),
        }
    }
}

impl Expected {
    /// Returns the words of what the grammar expects.
    fn text(self) -> &'static str {
        match self {
            Self::Statement => "a key, a table header, a comment or the end of the line",
            Self::Equals => "the = after a key",
            Self::Value => "a value, a basic string, true, false or an array of basic strings",
            Self::LineEnd => "the end of the line or a comment",
            Self::Key => "the bare key of a table header",
            Self::HeaderEnd => "the ] of a table header",
            Self::Item => "a basic string or the ] of the array",
            Self::Separator => "a comma or the ] of the array",
            Self::Quote => "the closing quote of the string on its line",
        }
    }
}

impl Form {
    /// Returns the name of the form.
    fn text(self) -> &'static str {
        match self {
            Self::NumberOrDate => "a number or a date",
            Self::LiteralString => "a literal string in single quotes",
            Self::MultiLineString => "a multi line string",
            Self::InlineTable => "an inline table",
            Self::DottedKey => "a dotted key",
            Self::QuotedKey => "a quoted key",
            Self::ArrayOfTables => "an array of tables",
            Self::NestedArray => "an array inside an array",
            Self::BooleanItem => "a boolean inside an array",
        }
    }
}

/// Reads `bytes` as a document of the subset.
///
/// # Errors
///
/// Returns an [`Error`] with the place of the first character the subset
/// refuses and its [`Reason`].
pub fn parse(bytes: &[u8]) -> Result<Document, Error> {
    if bytes.len() > MAX_BYTES {
        return Err(Error {
            at: position_of(bytes, MAX_BYTES),
            reason: Reason::TooLong,
        });
    }
    let text = str::from_utf8(bytes).map_err(|e| Error {
        at: position_of(bytes, e.valid_up_to()),
        reason: Reason::Utf8,
    })?;
    let mut reader = Reader {
        text,
        i: 0,
        line: 1,
        column: 1,
        keys: 0,
        doc: Document::default(),
    };
    reader.document()?;
    Ok(reader.doc)
}

/// Returns the place of the byte at `offset` of `bytes`.
///
/// The column counts the bytes that start a character, which is the count
/// of Unicode scalar values over UTF-8.
fn position_of(bytes: &[u8], offset: usize) -> Position {
    let before = &bytes[..offset.min(bytes.len())];
    let start = before
        .iter()
        .rposition(|&b| b == b'\n')
        .map_or(0, |i| i + 1);
    let line = before.split(|&b| b == b'\n').count();
    let chars = before[start..]
        .iter()
        .filter(|&&b| b & 0xc0 != 0x80)
        .count();
    Position {
        line: u32::try_from(line).unwrap_or(u32::MAX),
        column: u32::try_from(chars + 1).unwrap_or(u32::MAX),
    }
}

/// Returns the place of the first of `earlier` that defines `key`.
fn first_of<'k>(
    key: &str,
    mut earlier: impl Iterator<Item = (&'k str, Position)>,
) -> Option<Position> {
    earlier.find(|&(k, _)| k == key).map(|(_, at)| at)
}

/// Returns whether `b` may stand in a bare key.
fn is_bare(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}

/// Returns whether TOML refuses `c` in a comment and in a basic string.
fn is_control(c: char) -> bool {
    c != '\t' && (c <= '\u{1f}' || c == '\u{7f}')
}

/// The reader of one document, its place and what it has read.
struct Reader<'a> {
    /// The document.
    text: &'a str,
    /// The byte offset of the next character.
    i: usize,
    /// The line of the next character.
    line: u32,
    /// The column of the next character.
    column: u32,
    /// The keys and headers read so far.
    keys: usize,
    /// What the reader has read.
    doc: Document,
}

impl Reader<'_> {
    /// Reads every line of the document.
    fn document(&mut self) -> Result<(), Error> {
        loop {
            self.blanks();
            match self.peek() {
                None => return Ok(()),
                Some(b'#' | b'\n' | b'\r') => {}
                Some(b'[') => self.header()?,
                Some(b) if is_bare(b) => self.pair()?,
                Some(b'"' | b'\'') => return Err(self.outside(Form::QuotedKey)),
                Some(_) => return Err(self.unexpected(Expected::Statement)),
            }
            if !self.line_end()? {
                return Ok(());
            }
        }
    }

    /// Reads a table header and opens its table.
    fn header(&mut self) -> Result<(), Error> {
        let at = self.at();
        self.bump();
        if self.peek() == Some(b'[') {
            return Err(Error {
                at,
                reason: Reason::Outside(Form::ArrayOfTables),
            });
        }
        self.blanks();
        let name = self.key(Expected::Key)?;
        self.blanks();
        match self.peek() {
            Some(b'.') => return Err(self.outside(Form::DottedKey)),
            Some(b']') => self.bump(),
            _ => return Err(self.unexpected(Expected::HeaderEnd)),
        }
        let root = self.doc.root.iter().map(|e| (e.key.as_str(), e.at));
        let tables = self.doc.tables.iter().map(|t| (t.name.as_str(), t.at));
        let first = first_of(&name, root.chain(tables));
        self.define(&name, at, first)?;
        self.doc.tables.push(Table {
            name,
            at,
            entries: Vec::new(),
        });
        Ok(())
    }

    /// Reads a pair into the table that is open.
    fn pair(&mut self) -> Result<(), Error> {
        let at = self.at();
        let key = self.key(Expected::Statement)?;
        self.blanks();
        match self.peek() {
            Some(b'.') => return Err(self.outside(Form::DottedKey)),
            Some(b'=') => self.bump(),
            _ => return Err(self.unexpected(Expected::Equals)),
        }
        let entries = match self.doc.tables.last() {
            Some(table) => &table.entries,
            None => &self.doc.root,
        };
        let first = first_of(&key, entries.iter().map(|e| (e.key.as_str(), e.at)));
        self.define(&key, at, first)?;
        self.blanks();
        let value_at = self.at();
        let value = self.value()?;
        let entry = Entry {
            key,
            at,
            value,
            value_at,
        };
        match self.doc.tables.last_mut() {
            Some(table) => table.entries.push(entry),
            None => self.doc.root.push(entry),
        }
        Ok(())
    }

    /// Counts the key `key` at `at`, or refuses it when it was defined
    /// first at `first` or when the document holds too many keys.
    fn define(&mut self, key: &str, at: Position, first: Option<Position>) -> Result<(), Error> {
        if let Some(first) = first {
            return Err(Error {
                at,
                reason: Reason::Duplicate {
                    key: String::from(key),
                    first,
                },
            });
        }
        self.keys += 1;
        if self.keys > MAX_KEYS {
            return Err(Error {
                at,
                reason: Reason::TooManyKeys,
            });
        }
        Ok(())
    }

    /// Reads a bare key, or refuses what stands in its place with
    /// `expected`.
    fn key(&mut self, expected: Expected) -> Result<String, Error> {
        match self.peek() {
            Some(b'"' | b'\'') => return Err(self.outside(Form::QuotedKey)),
            Some(b) if is_bare(b) => {}
            _ => return Err(self.unexpected(expected)),
        }
        let start = self.i;
        while self.peek().is_some_and(is_bare) {
            self.bump();
        }
        Ok(String::from(
            self.text.get(start..self.i).unwrap_or_default(),
        ))
    }

    /// Reads the value of a pair.
    fn value(&mut self) -> Result<Value, Error> {
        if let Some(form) = self.form() {
            return Err(self.outside(form));
        }
        match self.peek() {
            Some(b'"') => Ok(Value::String(self.string()?)),
            Some(b'[') => Ok(Value::Array(self.array()?)),
            _ if self.word("true") => Ok(Value::Boolean(true)),
            _ if self.word("false") => Ok(Value::Boolean(false)),
            _ => Err(self.unexpected(Expected::Value)),
        }
    }

    /// Returns the form outside the subset that starts at the next
    /// character, if one does.
    fn form(&self) -> Option<Form> {
        let rest = self.rest();
        match rest.as_bytes().first()? {
            b'"' if rest.starts_with("\"\"\"") => Some(Form::MultiLineString),
            b'\'' => Some(Form::LiteralString),
            b'{' => Some(Form::InlineTable),
            b'0'..=b'9' | b'+' | b'-' => Some(Form::NumberOrDate),
            _ if self.starts_word("inf") || self.starts_word("nan") => Some(Form::NumberOrDate),
            _ => None,
        }
    }

    /// Reads an array of basic strings.
    fn array(&mut self) -> Result<Vec<Item>, Error> {
        self.bump();
        let mut items = Vec::new();
        loop {
            self.space()?;
            if self.peek() == Some(b']') {
                self.bump();
                return Ok(items);
            }
            items.push(self.item()?);
            self.space()?;
            match self.peek() {
                Some(b',') => self.bump(),
                Some(b']') => {
                    self.bump();
                    return Ok(items);
                }
                _ => return Err(self.unexpected(Expected::Separator)),
            }
        }
    }

    /// Reads one item of an array.
    fn item(&mut self) -> Result<Item, Error> {
        let form = match self.peek() {
            Some(b'[') => Some(Form::NestedArray),
            _ if self.starts_word("true") || self.starts_word("false") => Some(Form::BooleanItem),
            _ => self.form(),
        };
        if let Some(form) = form {
            return Err(self.outside(form));
        }
        if self.peek() != Some(b'"') {
            return Err(self.unexpected(Expected::Item));
        }
        let at = self.at();
        Ok(Item {
            text: self.string()?,
            at,
        })
    }

    /// Passes over the blanks, comments and line ends between the items of
    /// an array.
    fn space(&mut self) -> Result<(), Error> {
        loop {
            self.blanks();
            if self.peek() == Some(b'#') {
                self.comment()?;
            }
            if !self.newline() {
                return Ok(());
            }
        }
    }

    /// Reads a basic string from its opening quote and returns it with its
    /// escapes decoded.
    fn string(&mut self) -> Result<String, Error> {
        self.bump();
        let mut out = String::new();
        loop {
            match self.peek() {
                None | Some(b'\n') => return Err(self.unexpected(Expected::Quote)),
                Some(b'\r') if self.rest().starts_with("\r\n") => {
                    return Err(self.unexpected(Expected::Quote));
                }
                Some(b'"') => {
                    self.bump();
                    return Ok(out);
                }
                Some(b'\\') => out.push(self.escape()?),
                Some(_) => out.push(self.plain()?),
            }
        }
    }

    /// Reads one character of a string or a comment, refusing a control
    /// character.
    fn plain(&mut self) -> Result<char, Error> {
        let at = self.at();
        let Some(c) = self.rest().chars().next() else {
            return Err(self.unexpected(Expected::Quote));
        };
        if is_control(c) {
            return Err(Error {
                at,
                reason: Reason::Control(c),
            });
        }
        for _ in 0..c.len_utf8() {
            self.bump();
        }
        Ok(c)
    }

    /// Reads one escape of a basic string and returns the character it
    /// stands for.
    fn escape(&mut self) -> Result<char, Error> {
        let at = self.at();
        let refused = Error {
            at,
            reason: Reason::Escape,
        };
        self.bump();
        let digits = match self.peek() {
            Some(b'u') => 4,
            Some(b'U') => 8,
            Some(b) => {
                let c = match b {
                    b'b' => '\u{8}',
                    b't' => '\t',
                    b'n' => '\n',
                    b'f' => '\u{c}',
                    b'r' => '\r',
                    b'"' => '"',
                    b'\\' => '\\',
                    _ => return Err(refused),
                };
                self.bump();
                return Ok(c);
            }
            None => return Err(refused),
        };
        self.bump();
        let mut code = 0u32;
        for _ in 0..digits {
            let Some(digit) = self.peek().and_then(|b| char::from(b).to_digit(16)) else {
                return Err(refused);
            };
            code = (code << 4) | digit;
            self.bump();
        }
        char::from_u32(code).ok_or(refused)
    }

    /// Passes over a comment up to the end of its line.
    fn comment(&mut self) -> Result<(), Error> {
        self.bump();
        loop {
            match self.peek() {
                None | Some(b'\n') => return Ok(()),
                Some(b'\r') if self.rest().starts_with("\r\n") => return Ok(()),
                Some(_) => {
                    self.plain()?;
                }
            }
        }
    }

    /// Reads what may follow a statement on its line, and returns whether a
    /// line end follows rather than the end of the document.
    fn line_end(&mut self) -> Result<bool, Error> {
        self.blanks();
        if self.peek() == Some(b'#') {
            self.comment()?;
        }
        if self.newline() {
            return Ok(true);
        }
        match self.peek() {
            None => Ok(false),
            Some(_) => Err(self.unexpected(Expected::LineEnd)),
        }
    }

    /// Passes over a line end, and returns whether one stood there.
    fn newline(&mut self) -> bool {
        match self.peek() {
            Some(b'\n') => self.bump(),
            Some(b'\r') if self.rest().starts_with("\r\n") => {
                self.bump();
                self.bump();
            }
            _ => return false,
        }
        true
    }

    /// Returns whether `word` stands next with no character of a bare key
    /// after it.
    fn starts_word(&self, word: &str) -> bool {
        let rest = self.rest();
        rest.starts_with(word) && !rest.as_bytes().get(word.len()).is_some_and(|&b| is_bare(b))
    }

    /// Passes over `word` when it stands next with no character of a bare
    /// key after it, and returns whether it did.
    ///
    /// So `trueish` is no `true` followed by `ish` but a value the subset
    /// refuses where it starts.
    fn word(&mut self, word: &str) -> bool {
        if !self.starts_word(word) {
            return false;
        }
        for _ in 0..word.len() {
            self.bump();
        }
        true
    }

    /// Passes over spaces and tabs.
    fn blanks(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t')) {
            self.bump();
        }
    }

    /// Returns the text from the next character on.
    ///
    /// The reader moves past a character other than ASCII only as a whole,
    /// so its offset stands on a character boundary, and the text is empty
    /// at the end of the document alone.
    fn rest(&self) -> &str {
        self.text.get(self.i..).unwrap_or_default()
    }

    /// Returns the next byte.
    fn peek(&self) -> Option<u8> {
        self.text.as_bytes().get(self.i).copied()
    }

    /// Moves past the next byte, counting lines at a line feed and columns
    /// at each byte that starts a character.
    fn bump(&mut self) {
        let Some(b) = self.peek() else {
            return;
        };
        self.i += 1;
        if b == b'\n' {
            self.line = self.line.saturating_add(1);
            self.column = 1;
        } else if b & 0xc0 != 0x80 {
            self.column = self.column.saturating_add(1);
        }
    }

    /// Returns the place of the next character.
    fn at(&self) -> Position {
        Position {
            line: self.line,
            column: self.column,
        }
    }

    /// Returns the refusal of the next character, or of the end of the
    /// document, where the grammar expects `expected`.
    fn unexpected(&self, expected: Expected) -> Error {
        Error {
            at: self.at(),
            reason: Reason::Unexpected {
                found: self.rest().chars().next(),
                expected,
            },
        }
    }

    /// Returns the refusal of `form` at the next character.
    fn outside(&self, form: Form) -> Error {
        Error {
            at: self.at(),
            reason: Reason::Outside(form),
        }
    }
}
