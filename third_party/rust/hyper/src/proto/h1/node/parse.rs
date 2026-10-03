// SPDX-License-Identifier: MIT
/*
 * hyper::proto::h1::node::parse: the head parser of llhttp 9.3.0 as Node
 * 24.13.0 drives it.
 *
 * Translation of the request grammar of llhttp 9.3.0, src/llhttp/http.ts,
 * src/llhttp/url.ts, src/llhttp/constants.ts and src/native/http.c, as Node
 * 24.13.0 counts and reads it in src/node_http_parser.cc:305-358 and
 * lib/_http_server.js:1161-1205. Each item names the quirk of
 * docs/byte-equality.md section 13.1 or the divergence of its section 3 it
 * implements.
 */
//! The incremental head parser of llhttp 9.3.0 as Node 24.13.0 applies it.
//!
//! The parser takes the bytes of each read from where the last call stopped,
//! so a head that arrives one byte at a time costs time linear in its length.
//! It keeps the target and the names and values of the fields it keeps, and it
//! drops every byte it skips as it reads it: the empty lines before a request
//! line, which it reads as hyper's own parser reads them (divergence D73 of
//! quirk Q21), and the whitespace before a value (quirk Q25).

use alloc::vec::Vec;
use core::fmt;
use core::ops::Range;

use bytes::{Buf, Bytes, BytesMut};
use http::header::{HeaderMap, HeaderName, HeaderValue};
use http::{Method, Uri, Version};

use super::methods;
use super::{NodeLimits, OverFields};
use crate::body::DecodedLength;
#[cfg(any(feature = "node-chunked", feature = "node-conn"))]
use crate::common::io;
use crate::error::Parse;
use crate::ext::{HeaderCaseMap, NodeExpect, NodeHead, NodeVersion, RawTarget};
use crate::proto::h1::{ParseContext, ParseResult, ParsedMessage};
use crate::proto::{MessageHead, RequestLine};

/// Why the parser refuses a request, which decides Node's fixed answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// A byte llhttp refuses in the part named, answered with 400.
    Invalid(Part),
    /// The counted bytes of a head or of a trailer section reached their
    /// bound, or a head holds a field past the bound of the server cards,
    /// answered with 431 (quirks Q25 and Q32, divergence D74 of quirk Q26).
    HeaderOverflow,
    /// A bound of chunk extensions or of the configuration, answered with 413.
    TooLarge,
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Refusal::Invalid(_) => REFUSED_INVALID,
            Refusal::HeaderOverflow => REFUSED_OVERFLOW,
            Refusal::TooLarge => REFUSED_TOO_LARGE,
        })
    }
}

impl crate::error::StdError for Refusal {}

/// The message of a body llhttp refuses, answered with 400.
const REFUSED_INVALID: &str = "a byte llhttp refuses";
/// The message of a trailer section past its count, answered with 431.
const REFUSED_OVERFLOW: &str = "a head past its count";
/// The message of chunk extensions past their bound, answered with 413.
const REFUSED_TOO_LARGE: &str = "a body past its bound";

/// Returns the refusal an error of the body decoder carries, a byte llhttp
/// refuses for every error but the two bounds of a chunked body.
#[cfg(feature = "node-conn")]
pub(crate) fn refusal_of(error: &io::Error) -> Refusal {
    use alloc::string::ToString;
    match error.to_string().as_str() {
        REFUSED_OVERFLOW => Refusal::HeaderOverflow,
        REFUSED_TOO_LARGE => Refusal::TooLarge,
        _ => Refusal::Invalid(Part::Field),
    }
}

/// The part of a request a refused byte belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Part {
    /// The method of the request line (quirk Q18).
    Method,
    /// The target of the request line (quirk Q19).
    Target,
    /// The version of the request line and its end (quirks Q20 and Q21).
    Version,
    /// A field line (quirks Q22 and Q23).
    Field,
    /// A `Content-Length` (quirk Q24).
    Length,
    /// A `Transfer-Encoding` (quirk Q24).
    Encoding,
    /// A byte after a request that is not kept alive (quirk Q41).
    Closed,
}

/// How the body of a request is framed, as llhttp decides it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Framing {
    /// No body.
    None,
    /// A body of this many bytes.
    Length(u64),
    /// A chunked body.
    Chunked,
    /// A `Transfer-Encoding` of a request whose last coding is not chunked,
    /// which llhttp refuses after the callback of the head, so Node's own
    /// checks and the request listener run before the refusal (quirk Q24).
    Refused,
}

/// The longest name the parser reads as a word, `transfer-encoding`.
const WORD: usize = 17;

// The flags llhttp sets as it reads each field, over every field of a head.
const KEEP_ALIVE: u16 = 1 << 0;
const CLOSE: u16 = 1 << 1;
const CONNECTION_UPGRADE: u16 = 1 << 2;
const CHUNKED: u16 = 1 << 3;
const UPGRADE: u16 = 1 << 4;
const CONTENT_LENGTH: u16 = 1 << 5;
const TRANSFER_ENCODING: u16 = 1 << 9;

/// Where the parser stands in a head.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Start,
    StartLf,
    Method,
    SpacesBeforeTarget,
    Target(Url),
    SpacesBeforeVersion,
    Protocol,
    Major,
    Dot,
    Minor,
    LineCr,
    LineLf,
    NoVersionLf,
    HeadersStart,
    FieldStart,
    Name,
    DiscardWs,
    DiscardWsLf,
    DiscardLws,
    Value(Value),
    ValueLf,
    ValueLws,
    HeadLf,
    Done,
    // Only the Node connection reads past a request that is not kept alive.
    #[cfg_attr(not(feature = "node-conn"), allow(dead_code))]
    Closed,
}

/// Where the parser stands in a target, the states of llhttp's `url.ts`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Url {
    Start,
    Schema,
    SchemaSlash(bool),
    Server,
    ServerAt,
    Path,
    Query,
    Fragment,
}

/// What ends a target.
enum UrlEnd {
    Space,
    Cr,
    Lf,
}

/// Where the parser stands in a value, whose field name decides its grammar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Value {
    General,
    Length,
    LengthWs,
    TeWord(u8),
    TeLast,
    TeToken,
    TeOws,
    ConnectionStart,
    ConnectionWord(Token, u8),
    ConnectionWs(Token),
    ConnectionToken,
}

/// A token of `Connection` llhttp reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Token {
    Close,
    Upgrade,
    KeepAlive,
}

impl Token {
    fn word(self) -> &'static [u8] {
        match self {
            Token::Close => b"close",
            Token::Upgrade => b"upgrade",
            Token::KeepAlive => b"keep-alive",
        }
    }

    fn flag(self) -> u16 {
        match self {
            Token::Close => CLOSE,
            Token::Upgrade => CONNECTION_UPGRADE,
            Token::KeepAlive => KEEP_ALIVE,
        }
    }
}

/// The names llhttp reads with a grammar of their own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Special {
    General,
    Connection,
    Length,
    Encoding,
    Upgrade,
}

/// A kept field: its name in `start..colon` and its value in `colon..end`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Field {
    start: usize,
    colon: usize,
    end: usize,
}

/// The parser of the request heads of one connection.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NodeParser {
    limits: NodeLimits,
    state: State,
    begun: bool,
    trailers: bool,
    kept: BytesMut,
    word: [u8; WORD],
    word_len: usize,
    method: &'static str,
    target: Range<usize>,
    version: (u8, u8),
    fields: Vec<Field>,
    field_count: usize,
    field: Field,
    special: Special,
    mark: u16,
    flags: u16,
    count: usize,
    content_length: u64,
}

/// What a call of [`NodeParser::parse`] found.
pub(crate) enum Parsed {
    /// Every byte was read and the head is not complete.
    Partial,
    /// The head is complete after this many bytes.
    Head(usize, Head),
}

/// A request head the parser has completed.
#[derive(Debug)]
pub(crate) struct Head {
    /// The method, one of [`methods::NODE_METHODS`] (quirk Q18).
    pub(crate) method: Method,
    /// What Node's own checks read of the head (quirks Q20 and Q26).
    pub(crate) node: NodeHead,
    /// The framing of the body (quirk Q24).
    pub(crate) framing: Framing,
    /// Whether a field named `Upgrade` has a value, which hyper's own path reads.
    pub(crate) upgrade_field: bool,
    kept: Bytes,
    target: Range<usize>,
    fields: Vec<Field>,
}

impl NodeParser {
    /// Returns a parser of request heads under `limits`.
    pub(crate) fn new(limits: NodeLimits) -> Self {
        NodeParser {
            limits,
            state: State::Start,
            begun: false,
            trailers: false,
            kept: BytesMut::new(),
            word: [0; WORD],
            word_len: 0,
            method: "",
            target: 0..0,
            version: (0, 0),
            fields: Vec::new(),
            field_count: 0,
            field: Field {
                start: 0,
                colon: 0,
                end: 0,
            },
            special: Special::General,
            mark: 0,
            flags: 0,
            count: 0,
            content_length: 0,
        }
    }

    /// Returns a parser of the trailer section of a chunked body.
    ///
    /// The trailers take the field grammar of a head, strict CRLF and names of
    /// token bytes, their names and values count from zero toward `max_head`,
    /// no field is kept, and the flags of a chunked body stand, so a
    /// `Content-Length` or a `Transfer-Encoding` among them is refused as
    /// llhttp refuses it (quirk Q32).
    #[cfg(feature = "node-chunked")]
    pub(crate) fn trailers(limits: NodeLimits) -> Self {
        let mut parser = NodeParser::new(limits);
        parser.state = State::FieldStart;
        parser.begun = true;
        parser.trailers = true;
        parser.flags = TRANSFER_ENCODING | CHUNKED;
        parser
    }

    /// Returns whether a byte llhttp does not skip has begun a message.
    #[cfg(feature = "node-conn")]
    pub(crate) fn begun(&self) -> bool {
        self.begun
    }

    /// Prepares the parser for the next message, a request when the one before
    /// it is kept alive and a refusal of every byte but CR and LF otherwise
    /// (quirk Q41).
    #[cfg(feature = "node-conn")]
    pub(crate) fn next_message(&mut self, keep_alive: bool) {
        *self = NodeParser::new(self.limits);
        if !keep_alive {
            self.state = State::Closed;
        }
    }

    /// Reads the bytes of one read from where the last call stopped.
    ///
    /// # Errors
    ///
    /// Returns the [`Refusal`] of the first byte llhttp refuses.
    pub(crate) fn parse(&mut self, bytes: &[u8]) -> Result<Parsed, Refusal> {
        for (i, &b) in bytes.iter().enumerate() {
            if self.byte(b)? {
                self.state = State::Done;
                return self.complete().map(|head| Parsed::Head(i + 1, head));
            }
        }
        Ok(Parsed::Partial)
    }

    /// Reads one byte of a trailer section and returns whether it ends it.
    ///
    /// # Errors
    ///
    /// Returns the [`Refusal`] of a byte llhttp refuses there.
    #[cfg(feature = "node-chunked")]
    pub(crate) fn trailer(&mut self, b: u8) -> Result<bool, Refusal> {
        self.byte(b)
    }

    /// Reads one byte and returns whether it ends the head.
    fn byte(&mut self, b: u8) -> Result<bool, Refusal> {
        loop {
            match self.state {
                // The empty lines before a request line, each a CRLF or a lone
                // LF, are skipped and not kept, and a CR before any other byte
                // is refused, as hyper's own parser reads them (divergence D73
                // of quirk Q21).
                State::Start => match b {
                    b'\n' => return Ok(false),
                    b'\r' => {
                        self.state = State::StartLf;
                        return Ok(false);
                    }
                    _ => {
                        self.begun = true;
                        self.state = State::Method;
                    }
                },
                State::StartLf => return self.expect(b, b'\n', State::Start, Part::Method),
                State::Method => return self.method_byte(b).map(|()| false),
                State::SpacesBeforeTarget => match b {
                    // One space parts the method from the target, so a second
                    // space or a tab is refused (divergence D73 of quirk Q21).
                    b' ' | b'\t' | 0x0c => return Err(Refusal::Invalid(Part::Target)),
                    _ => {
                        self.target = self.kept.len()..self.kept.len();
                        let entry = if self.method == "CONNECT" {
                            Url::Server
                        } else {
                            Url::Start
                        };
                        self.state = State::Target(entry);
                    }
                },
                State::Target(url) => return self.target_byte(url, b).map(|()| false),
                // One space parts the target from the version, so a second
                // space is refused (divergence D73 of quirk Q21).
                State::SpacesBeforeVersion => {
                    if b == b' ' {
                        return Err(Refusal::Invalid(Part::Version));
                    }
                    self.word_len = 0;
                    self.state = State::Protocol;
                }
                State::Protocol => return self.protocol_byte(b).map(|()| false),
                State::Major | State::Minor => return self.version_byte(b).map(|()| false),
                State::Dot => return self.expect(b, b'.', State::Minor, Part::Version),
                State::LineCr => return self.expect(b, b'\r', State::LineLf, Part::Version),
                State::LineLf => return self.expect(b, b'\n', State::HeadersStart, Part::Version),
                State::NoVersionLf => {
                    self.version = (0, 9);
                    return self.expect(b, b'\n', State::HeadersStart, Part::Target);
                }
                // A field line never starts with a space (quirk Q23).
                State::HeadersStart => {
                    if b == b' ' {
                        return Err(Refusal::Invalid(Part::Field));
                    }
                    self.state = State::FieldStart;
                }
                State::FieldStart => match b {
                    b'\r' => {
                        self.state = State::HeadLf;
                        return Ok(false);
                    }
                    // Every line ends in CRLF (quirk Q22).
                    b'\n' | b':' => return Err(Refusal::Invalid(Part::Field)),
                    _ => self.begin_field()?,
                },
                State::Name => return self.name_byte(b).map(|()| false),
                State::DiscardWs => match b {
                    b' ' | b'\t' => return Ok(false),
                    b'\r' => {
                        self.state = State::DiscardWsLf;
                        return Ok(false);
                    }
                    b'\n' => return Err(Refusal::Invalid(Part::Field)),
                    _ => self.begin_value()?,
                },
                State::DiscardWsLf => return self.expect(b, b'\n', State::DiscardLws, Part::Field),
                // An empty value followed by a space or a tab is an obsolete
                // line folding, which llhttp refuses (quirk Q23).
                State::DiscardLws => {
                    if b == b' ' || b == b'\t' {
                        return Err(Refusal::Invalid(Part::Field));
                    }
                    if self.special == Special::Length {
                        return Err(Refusal::Invalid(Part::Length));
                    }
                    self.field.end = self.kept.len();
                    self.end_field()?;
                }
                State::Value(value) => {
                    if let Some(next) = self.value_byte(value, b)? {
                        self.state = next;
                        return Ok(false);
                    }
                }
                State::ValueLf => return self.expect(b, b'\n', State::ValueLws, Part::Field),
                State::ValueLws => {
                    if b == b' ' || b == b'\t' {
                        return Err(Refusal::Invalid(Part::Field));
                    }
                    self.flags |= self.mark;
                    self.end_field()?;
                }
                State::HeadLf => {
                    if b != b'\n' {
                        return Err(Refusal::Invalid(Part::Field));
                    }
                    return Ok(true);
                }
                State::Done => return Ok(false),
                // A byte other than CR and LF after a request that is not kept
                // alive is refused (quirk Q41).
                State::Closed => {
                    if b == b'\r' || b == b'\n' {
                        return Ok(false);
                    }
                    return Err(Refusal::Invalid(Part::Closed));
                }
            }
        }
    }

    /// Moves to `next` when `b` is `wanted`, and refuses it otherwise.
    fn expect(&mut self, b: u8, wanted: u8, next: State, part: Part) -> Result<bool, Refusal> {
        if b != wanted {
            return Err(Refusal::Invalid(part));
        }
        self.state = next;
        Ok(false)
    }

    /// Reads a byte of the method, which ends at its one space and is one of
    /// the 35 methods of Node (quirk Q18).
    fn method_byte(&mut self, b: u8) -> Result<(), Refusal> {
        if b == b' ' {
            self.method =
                methods::find(&self.word[..self.word_len]).ok_or(Refusal::Invalid(Part::Method))?;
            self.state = State::SpacesBeforeTarget;
            return Ok(());
        }
        if self.word_len == methods::LONGEST || !(b.is_ascii_uppercase() || b == b'-') {
            return Err(Refusal::Invalid(Part::Method));
        }
        self.word[self.word_len] = b;
        self.word_len += 1;
        Ok(())
    }

    /// Reads a byte of the target in llhttp's grammar of a URL (quirk Q19).
    ///
    /// The target is one or more bytes of llhttp's URL class, `#` and `?`
    /// included, and a control byte, a tab or a byte from 0x80 up is refused.
    /// A target that ends at CR or LF has no version, which llhttp reads as
    /// HTTP/0.9 (quirk Q20).
    fn target_byte(&mut self, url: Url, b: u8) -> Result<(), Refusal> {
        let next = match url_next(url, b) {
            Ok(next) => next,
            Err(Some(end)) => {
                self.target.end = self.kept.len();
                self.state = match end {
                    UrlEnd::Space => State::SpacesBeforeVersion,
                    UrlEnd::Cr => State::NoVersionLf,
                    UrlEnd::Lf => {
                        self.version = (0, 9);
                        State::HeadersStart
                    }
                };
                return Ok(());
            }
            Err(None) => return Err(Refusal::Invalid(Part::Target)),
        };
        self.counted()?;
        self.keep(b, false)?;
        self.state = State::Target(next);
        Ok(())
    }

    /// Reads a byte of the protocol before its slash, `HTTP`, or `RTSP` and
    /// `ICE` for the methods llhttp allows with them.
    fn protocol_byte(&mut self, b: u8) -> Result<(), Refusal> {
        if b != b'/' {
            if self.word_len == 4 || !b.is_ascii_uppercase() {
                return Err(Refusal::Invalid(Part::Version));
            }
            self.word[self.word_len] = b;
            self.word_len += 1;
            return Ok(());
        }
        let allowed = match &self.word[..self.word_len] {
            b"HTTP" => true,
            b"RTSP" => matches!(self.method, "OPTIONS" | "GET" | "POST"),
            b"ICE" => self.method == "SOURCE",
            _ => false,
        };
        if !allowed {
            return Err(Refusal::Invalid(Part::Version));
        }
        self.state = State::Major;
        Ok(())
    }

    /// Reads a digit of the version, of which llhttp takes HTTP/0.9, 1.0, 1.1
    /// and 2.0 (quirk Q20).
    fn version_byte(&mut self, b: u8) -> Result<(), Refusal> {
        if !b.is_ascii_digit() {
            return Err(Refusal::Invalid(Part::Version));
        }
        if self.state == State::Major {
            self.version.0 = b - b'0';
            self.state = State::Dot;
            return Ok(());
        }
        self.version.1 = b - b'0';
        if !matches!(self.version, (0, 9) | (1, 0) | (1, 1) | (2, 0)) {
            return Err(Refusal::Invalid(Part::Version));
        }
        self.state = State::LineCr;
        Ok(())
    }

    /// Begins a field line, and refuses a head with a field past `max_fields`
    /// as hyper's own parser refuses one past its `max_headers`, where Node's
    /// window dropped the rest (divergence D74 of quirk Q26).
    fn begin_field(&mut self) -> Result<(), Refusal> {
        self.field_count += 1;
        if !self.trailers && self.field_count > self.limits.max_fields {
            // A server card answers 431 and a device card its own 413.
            return Err(match self.limits.over_fields {
                OverFields::Overflow => Refusal::HeaderOverflow,
                OverFields::Refuse => Refusal::TooLarge,
            });
        }
        self.field.start = self.kept.len();
        self.word_len = 0;
        self.state = State::Name;
        Ok(())
    }

    /// Reads a byte of a field name, token bytes directly before the colon
    /// (quirk Q23).
    fn name_byte(&mut self, b: u8) -> Result<(), Refusal> {
        if b == b':' {
            let name = if self.word_len <= WORD {
                &self.word[..self.word_len]
            } else {
                &[]
            };
            self.special = match name {
                b"connection" | b"proxy-connection" => Special::Connection,
                b"content-length" => Special::Length,
                b"transfer-encoding" => Special::Encoding,
                b"upgrade" => Special::Upgrade,
                _ => Special::General,
            };
            // A `Transfer-Encoding` beside a `Content-Length` is refused in
            // either order, as llhttp 9 refuses it (quirk Q24).
            match self.special {
                Special::Encoding if self.flags & CONTENT_LENGTH != 0 => {
                    return Err(Refusal::Invalid(Part::Encoding));
                }
                Special::Length if self.flags & TRANSFER_ENCODING != 0 => {
                    return Err(Refusal::Invalid(Part::Length));
                }
                _ => (),
            }
            self.mark = 0;
            self.field.colon = self.kept.len();
            self.state = State::DiscardWs;
            return Ok(());
        }
        if !is_token(b) {
            return Err(Refusal::Invalid(Part::Field));
        }
        if self.word_len < WORD {
            self.word[self.word_len] = b.to_ascii_lowercase();
        }
        self.word_len = self.word_len.saturating_add(1);
        self.counted()?;
        self.keep(b, true)
    }

    /// Begins the value of a field at its first byte after the whitespace
    /// llhttp skips, in the grammar its name selects (quirks Q23 and Q24).
    fn begin_value(&mut self) -> Result<(), Refusal> {
        let value = match self.special {
            Special::General => Value::General,
            Special::Upgrade => {
                self.flags |= UPGRADE;
                Value::General
            }
            // A `Transfer-Encoding` after one whose last coding is
            // `chunked` is refused, as a request names nothing after
            // `chunked` (quirk Q24).
            Special::Encoding => {
                if self.flags & CHUNKED != 0 {
                    return Err(Refusal::Invalid(Part::Encoding));
                }
                self.flags |= TRANSFER_ENCODING;
                self.flags &= !CHUNKED;
                Value::TeWord(0)
            }
            // A second `Content-Length` is refused, even with the same value
            // (quirk Q24).
            Special::Length => {
                if self.flags & CONTENT_LENGTH != 0 {
                    return Err(Refusal::Invalid(Part::Length));
                }
                self.content_length = 0;
                Value::Length
            }
            Special::Connection => Value::ConnectionStart,
        };
        self.state = State::Value(value);
        Ok(())
    }

    /// Reads a byte of a value and returns the next state, or `None` when the
    /// byte is read again in another state.
    fn value_byte(&mut self, value: Value, b: u8) -> Result<Option<State>, Refusal> {
        let next = match value {
            // A value is a tab, a space, a visible byte or a byte from 0x80
            // up, NUL and DEL refused (quirk Q23).
            Value::General => {
                if b == b'\r' {
                    return Ok(Some(self.end_value()));
                }
                if !is_value(b) {
                    return Err(Refusal::Invalid(Part::Field));
                }
                Value::General
            }
            // A length is digits alone, with spaces after them (quirk Q24).
            Value::Length => {
                if !b.is_ascii_digit() {
                    return Ok(self.again(Value::LengthWs));
                }
                self.content_length = self
                    .content_length
                    .checked_mul(10)
                    .and_then(|n| n.checked_add(u64::from(b - b'0')))
                    .ok_or(Refusal::Invalid(Part::Length))?;
                Value::Length
            }
            Value::LengthWs => match b {
                b' ' => Value::LengthWs,
                b'\r' => {
                    self.flags |= CONTENT_LENGTH;
                    return Ok(Some(self.end_value()));
                }
                _ => return Err(Refusal::Invalid(Part::Length)),
            },
            // A body is chunked when the last coding is `chunked` in any case
            // (quirk Q24).
            Value::TeWord(n) => {
                if b.to_ascii_lowercase() != b"chunked"[usize::from(n)] {
                    return Ok(self.again(Value::TeToken));
                }
                if n == 6 {
                    Value::TeLast
                } else {
                    Value::TeWord(n + 1)
                }
            }
            Value::TeLast => match b {
                b' ' => Value::TeLast,
                b'\r' => {
                    self.mark = CHUNKED;
                    return Ok(Some(self.end_value()));
                }
                b'\n' | b',' => return Err(Refusal::Invalid(Part::Encoding)),
                _ => return Ok(self.again(Value::TeToken)),
            },
            Value::TeToken => match b {
                b',' => Value::TeOws,
                _ if is_value(b) => Value::TeToken,
                _ => return Ok(self.again(Value::General)),
            },
            Value::TeOws => match b {
                b' ' | b'\t' => Value::TeOws,
                _ => return Ok(self.again(Value::TeWord(0))),
            },
            // The tokens `close`, `keep-alive` and `upgrade` of a connection
            // decide the keep alive and the mark of an upgrade.
            Value::ConnectionStart => match b.to_ascii_lowercase() {
                b' ' | b'\t' => Value::ConnectionStart,
                b'c' => Value::ConnectionWord(Token::Close, 1),
                b'u' => Value::ConnectionWord(Token::Upgrade, 1),
                b'k' => Value::ConnectionWord(Token::KeepAlive, 1),
                _ => return Ok(self.again(Value::ConnectionToken)),
            },
            Value::ConnectionWord(token, n) => {
                let word = token.word();
                if b.to_ascii_lowercase() != word[usize::from(n)] {
                    return Ok(self.again(Value::ConnectionToken));
                }
                if usize::from(n) + 1 == word.len() {
                    Value::ConnectionWs(token)
                } else {
                    Value::ConnectionWord(token, n + 1)
                }
            }
            Value::ConnectionWs(token) => match b {
                b',' => {
                    self.flags |= token.flag();
                    Value::ConnectionStart
                }
                b' ' => Value::ConnectionWs(token),
                b'\r' => {
                    self.mark = token.flag();
                    return Ok(Some(self.end_value()));
                }
                b'\n' => return Err(Refusal::Invalid(Part::Field)),
                _ => return Ok(self.again(Value::ConnectionToken)),
            },
            Value::ConnectionToken => match b {
                b',' => Value::ConnectionStart,
                b'\r' => return Ok(Some(self.end_value())),
                _ if is_value(b) => Value::ConnectionToken,
                _ => return Err(Refusal::Invalid(Part::Field)),
            },
        };
        self.counted()?;
        self.keep(b, true)?;
        Ok(Some(State::Value(next)))
    }

    /// Moves to `value` and has the byte read again there.
    fn again(&mut self, value: Value) -> Option<State> {
        self.state = State::Value(value);
        None
    }

    /// Ends a value at its CR.
    fn end_value(&mut self) -> State {
        self.field.end = self.kept.len();
        State::ValueLf
    }

    /// Ends a field line, keeping it unless it is a trailer.
    fn end_field(&mut self) -> Result<(), Refusal> {
        if !self.trailers {
            self.fields.push(self.field);
        }
        self.state = State::FieldStart;
        Ok(())
    }

    /// Counts a byte of the target, of a name or of a value, which Node
    /// refuses with 431 once the count reaches `max_head`, and a byte llhttp
    /// skips counts nothing (quirk Q25).
    fn counted(&mut self) -> Result<(), Refusal> {
        self.count += 1;
        if self.count >= self.limits.max_head {
            return Err(Refusal::HeaderOverflow);
        }
        Ok(())
    }

    /// Refuses with 413 a body of `length` bytes past `max_body`, or past
    /// `max_request` with the kept head, the bounds of a device card and the
    /// body ceiling of a server card.
    fn bound(&self, length: u64) -> Result<(), Refusal> {
        let head = self.kept.len() as u64;
        if self.limits.max_body.is_some_and(|max| length > max)
            || self
                .limits
                .max_request
                .is_some_and(|max| head.saturating_add(length) > max)
        {
            return Err(Refusal::TooLarge);
        }
        Ok(())
    }

    /// Keeps a byte of the target, or of a field when `field` is set and the
    /// field is no trailer.
    fn keep(&mut self, b: u8, field: bool) -> Result<(), Refusal> {
        if field && self.trailers {
            return Ok(());
        }
        self.kept.extend_from_slice(&[b]);
        // A device card bounds the bytes it keeps with the answer of the
        // head's own bound, 431.
        if self
            .limits
            .head_buffer
            .is_some_and(|max| self.kept.len() > max)
        {
            return Err(Refusal::HeaderOverflow);
        }
        Ok(())
    }

    /// Completes a head after its last CRLF and decides its framing, its keep
    /// alive and the facts Node's own checks read.
    fn complete(&mut self) -> Result<Head, Refusal> {
        let connect = self.method == "CONNECT";
        let upgrade =
            (self.flags & UPGRADE != 0 && self.flags & CONNECTION_UPGRADE != 0) || connect;
        let chunked = self.flags & CHUNKED != 0;
        let length = self.flags & CONTENT_LENGTH != 0 && self.content_length > 0;
        // The order of llhttp's `after_headers_complete`, so an upgrade with no
        // body and a `CONNECT` end at their head (quirk Q24).
        let framing = if upgrade && (connect || !(chunked || length)) {
            Framing::None
        } else if chunked {
            Framing::Chunked
        } else if self.flags & TRANSFER_ENCODING != 0 {
            Framing::Refused
        } else if length {
            self.bound(self.content_length)?;
            Framing::Length(self.content_length)
        } else {
            Framing::None
        };
        // Every version but HTTP/1.1 takes the rules of HTTP/1.0 (quirk Q20).
        let keep_alive = if self.version == (1, 1) {
            self.flags & CLOSE == 0
        } else {
            self.flags & KEEP_ALIVE != 0
        };
        let kept = core::mem::take(&mut self.kept).freeze();
        let fields = core::mem::take(&mut self.fields);
        // `Host`, `Expect` and `TE` are read over every field, as the flags
        // are (divergence D74 of quirk Q26).
        let every = fields.as_slice();
        let bytes = &kept;
        let named = move |name: &'static [u8]| {
            every
                .iter()
                .filter(move |f| bytes[f.start..f.colon].eq_ignore_ascii_case(name))
                .map(move |f| trim(&bytes[f.colon..f.end]))
        };
        let host = named(b"host").next().map(|v| kept.slice_ref(v));
        let expect = if named(b"expect").any(|v| has_word(v, b"100-continue")) {
            NodeExpect::Continue
        } else if named(b"expect").next().is_some() {
            NodeExpect::Other
        } else {
            NodeExpect::None
        };
        let te_chunked = named(b"te").any(|v| has_word(v, b"chunked"));
        let version = match self.version {
            (0, 9) => NodeVersion::Http09,
            (1, 0) => NodeVersion::Http10,
            (1, 1) => NodeVersion::Http11,
            _ => NodeVersion::Http20,
        };
        let method = Method::from_bytes(self.method.as_bytes())
            .map_err(|_| Refusal::Invalid(Part::Method))?;
        Ok(Head {
            method,
            node: NodeHead {
                version,
                host,
                expect,
                te_chunked,
                keep_alive,
                upgrade: upgrade && !connect,
                connect,
            },
            framing,
            upgrade_field: self.flags & UPGRADE != 0,
            kept,
            target: self.target.clone(),
            fields,
        })
    }
}

/// Returns the next state of a target after `b`, or what ends it, or `None`
/// for a byte llhttp refuses there.
fn url_next(url: Url, b: u8) -> Result<Url, Option<UrlEnd>> {
    let end = match b {
        b' ' => Some(UrlEnd::Space),
        b'\r' => Some(UrlEnd::Cr),
        b'\n' => Some(UrlEnd::Lf),
        b'\t' | 0x0c => return Err(None),
        _ => None,
    };
    let url_char = (0x21..=0x7e).contains(&b) && b != b'#' && b != b'?';
    match url {
        Url::Start if b == b'/' || b == b'*' => Ok(Url::Path),
        Url::Start | Url::Schema if b.is_ascii_alphabetic() => Ok(Url::Schema),
        Url::Schema if b == b':' => Ok(Url::SchemaSlash(false)),
        Url::SchemaSlash(false) if b == b'/' => Ok(Url::SchemaSlash(true)),
        Url::SchemaSlash(true) if b == b'/' => Ok(Url::Server),
        Url::Start | Url::Schema | Url::SchemaSlash(_) => Err(None),
        Url::Server | Url::ServerAt => match b {
            b'/' => Ok(Url::Path),
            b'?' => Ok(Url::Query),
            b'@' if url == Url::Server => Ok(Url::ServerAt),
            _ if is_userinfo(b) || b == b'[' || b == b']' => Ok(Url::Server),
            _ => Err(end),
        },
        Url::Path if url_char => Ok(Url::Path),
        Url::Path | Url::Query if b == b'?' => Ok(Url::Query),
        Url::Query if url_char => Ok(Url::Query),
        Url::Path | Url::Query if b == b'#' => Ok(Url::Fragment),
        Url::Fragment if url_char || b == b'?' || b == b'#' => Ok(Url::Fragment),
        Url::Path | Url::Query | Url::Fragment => Err(end),
    }
}

/// Returns whether `b` is a byte of llhttp's `USERINFO_CHARS`.
fn is_userinfo(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"-_.!~*'()%;:&=+$,".contains(&b)
}

/// Returns whether `b` is a token byte of RFC 9110, llhttp's `TOKEN`.
fn is_token(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b)
}

/// Returns whether `b` is a byte of llhttp's `HEADER_CHARS`, a tab, a visible
/// byte, a space or a byte from 0x80 up.
fn is_value(b: u8) -> bool {
    b == b'\t' || (b >= 0x20 && b != 0x7f)
}

/// Returns `value` without the spaces and tabs at its end, which Node trims.
fn trim(value: &[u8]) -> &[u8] {
    let end = value
        .iter()
        .rposition(|&b| b != b' ' && b != b'\t')
        .map_or(0, |i| i + 1);
    &value[..end]
}

/// Returns whether `value` names `word` as Node's expressions
/// `(?:^|\W)word(?:$|\W)` find it, ignoring case.
pub(crate) fn has_word(value: &[u8], word: &[u8]) -> bool {
    let is_word = |b: &u8| b.is_ascii_alphanumeric() || *b == b'_';
    (0..value.len().saturating_sub(word.len() - 1)).any(|i| {
        value[i..i + word.len()].eq_ignore_ascii_case(word)
            && (i == 0 || !is_word(&value[i - 1]))
            && value.get(i + word.len()).map_or(true, |b| !is_word(b))
    })
}

impl Head {
    /// Returns the number of bytes of the head the parser kept, which
    /// `max_request` counts with the body.
    #[cfg(feature = "node-conn")]
    pub(crate) fn kept_len(&self) -> u64 {
        self.kept.len() as u64
    }

    /// Returns the target as it arrived (quirk Q19).
    pub(crate) fn raw_target(&self) -> Bytes {
        self.kept.slice(self.target.clone())
    }

    /// Returns the target as an `http::Uri`, without its fragment and with every
    /// byte `http::Uri` refuses percent encoded (quirk Q19).
    pub(crate) fn uri(&self) -> Uri {
        let target = &self.kept[self.target.clone()];
        let target = target
            .iter()
            .position(|&b| b == b'#')
            .map_or(target, |i| &target[..i]);
        if let Ok(uri) = Uri::try_from(target) {
            return uri;
        }
        const HEX: &[u8; 16] = b"0123456789ABCDEF";
        let mut escaped = Vec::with_capacity(target.len() * 3);
        for &b in target {
            if b.is_ascii_alphanumeric() || b"-._~!$&'()*+,;=:@/?%".contains(&b) {
                escaped.push(b);
            } else {
                escaped.extend_from_slice(&[
                    b'%',
                    HEX[usize::from(b >> 4)],
                    HEX[usize::from(b & 15)],
                ]);
            }
        }
        Uri::try_from(escaped).unwrap_or_else(|_| Uri::from_static("/"))
    }

    /// Appends the kept fields to `headers` in arrival order, each value
    /// trimmed, and hands the spelling of each name to `spelling`.
    ///
    /// # Errors
    ///
    /// Returns a [`Refusal`] for a name or value `http` refuses, which the
    /// grammar of the parser never lets through.
    pub(crate) fn headers(
        &self,
        headers: &mut HeaderMap,
        mut spelling: impl FnMut(&HeaderName, Bytes),
    ) -> Result<(), Refusal> {
        headers.reserve(self.fields.len());
        for field in &self.fields {
            let name = HeaderName::from_bytes(&self.kept[field.start..field.colon])
                .map_err(|_| Refusal::Invalid(Part::Field))?;
            let value = trim(&self.kept[field.colon..field.end]);
            let value = HeaderValue::from_maybe_shared(self.kept.slice_ref(value))
                .map_err(|_| Refusal::Invalid(Part::Field))?;
            spelling(&name, self.kept.slice(field.start..field.colon));
            headers.append(name, value);
        }
        Ok(())
    }
}

/// Reads a request head on hyper's own path, from the start of the buffer at
/// every read as hyper's own parse does (quirks Q18 to Q26).
///
/// A version other than HTTP/1.1 takes the rules of HTTP/1.0, as Node applies
/// them (quirk Q20), and the head carries its [`RawTarget`] and its
/// [`NodeHead`] among its extensions.
///
/// # Errors
///
/// Returns hyper's parse error for the part llhttp refuses.
pub(crate) fn server(buf: &mut BytesMut, ctx: ParseContext<'_>) -> ParseResult<RequestLine> {
    // Hyper answers a bound of the parser with 431 on its own path and has no
    // answer of 413, so its own path reads a body without the ceiling of
    // `max_body`, which the Node connection holds.
    let limits = NodeLimits {
        max_body: None,
        ..NodeLimits::NODE
    };
    let mut parser = NodeParser::new(limits);
    let (len, head) = match parser.parse(buf).map_err(parse_error)? {
        Parsed::Partial => return Ok(None),
        Parsed::Head(len, head) => (len, head),
    };
    buf.advance(len);
    let mut headers = ctx.cached_headers.take().unwrap_or_default();
    let mut case = ctx.preserve_header_case.then(HeaderCaseMap::default);
    head.headers(&mut headers, |name, spelling| {
        if let Some(case) = case.as_mut() {
            case.append(name, spelling);
        }
    })
    .map_err(parse_error)?;
    let mut extensions = http::Extensions::default();
    if let Some(case) = case {
        extensions.insert(case);
    }
    extensions.insert(RawTarget(head.raw_target()));
    let version = match head.node.version {
        NodeVersion::Http11 => Version::HTTP_11,
        _ => Version::HTTP_10,
    };
    let decode = match head.framing {
        Framing::None => DecodedLength::ZERO,
        Framing::Length(len) => DecodedLength::checked_new(len)?,
        Framing::Chunked => DecodedLength::CHUNKED,
        // Hyper's own path has no callback of the head before the refusal.
        Framing::Refused => return Err(parse_error(Refusal::Invalid(Part::Encoding))),
    };
    let http11 = version == Version::HTTP_11;
    // Node's match of `100-continue` in any word of the field goes with the
    // eager `100 Continue` of the Node connection (quirk Q38), so hyper's own
    // path keeps hyper's exact match of the last `Expect`.
    let expect_continue = http11
        && headers
            .get_all(http::header::EXPECT)
            .iter()
            .last()
            .is_some_and(|value| value.as_bytes().eq_ignore_ascii_case(b"100-continue"));
    let wants_upgrade = head.node.connect || (http11 && head.upgrade_field);
    let keep_alive = head.node.keep_alive;
    let uri = head.uri();
    extensions.insert(head.node);
    *ctx.req_method = Some(head.method.clone());
    Ok(Some(ParsedMessage {
        head: MessageHead {
            version,
            subject: RequestLine(head.method, uri),
            headers,
            extensions,
        },
        decode,
        expect_continue,
        keep_alive,
        wants_upgrade,
    }))
}

/// Returns hyper's parse error for a refusal, which hyper answers with 400 or
/// 431 on its own path.
fn parse_error(refusal: Refusal) -> Parse {
    match refusal {
        Refusal::Invalid(Part::Method | Part::Closed) => Parse::Method,
        Refusal::Invalid(Part::Target) => Parse::Uri,
        Refusal::Invalid(Part::Version) => Parse::Version,
        Refusal::Invalid(Part::Field) => Parse::Header(crate::error::Header::Token),
        Refusal::Invalid(Part::Length) => Parse::content_length_invalid(),
        Refusal::Invalid(Part::Encoding) => Parse::transfer_encoding_invalid(),
        Refusal::HeaderOverflow | Refusal::TooLarge => Parse::TooLarge,
    }
}

/// The chunk extensions and the trailer section of a chunked body as llhttp
/// reads them (quirks Q32 and Q33).
#[cfg(feature = "node-chunked")]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Chunked {
    limits: NodeLimits,
    extension: Extension,
    trailers: Option<NodeParser>,
}

/// Where the reader stands in the extensions of a chunk size line.
#[cfg(feature = "node-chunked")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Extension {
    Done,
    AfterSemicolon,
    Name,
    Value,
    Quoted,
    Pair,
    QuotedDone,
}

#[cfg(feature = "node-chunked")]
impl Chunked {
    /// Returns the reader of a chunked body whose trailer section counts to
    /// `max_head`, Node's 16,384 by default.
    pub(crate) fn new(max_head: Option<usize>) -> Self {
        let limits = NodeLimits {
            max_head: max_head.unwrap_or(NodeLimits::NODE.max_head),
            ..NodeLimits::NODE
        };
        Chunked {
            limits,
            extension: Extension::Done,
            trailers: None,
        }
    }

    /// Reads a byte of the extensions of a chunk size line after its first
    /// semicolon and returns whether the byte ends them, their CR.
    ///
    /// The names and values count from zero at each chunk in `count`, and
    /// more than `max_chunk_extensions` of them are refused with 413 (quirk
    /// Q33).
    ///
    /// # Errors
    ///
    /// Returns an error for a byte llhttp refuses there or for the bound.
    pub(crate) fn extension(&mut self, b: u8, count: &mut u64) -> io::Result<bool> {
        use Extension::*;
        if self.extension == Done {
            *count = 0;
            self.extension = AfterSemicolon;
        }
        let quoted = b == b'\t' || b == b' ' || b >= 0x21;
        let (next, counted) = match (self.extension, b) {
            (AfterSemicolon, b' ' | b'\r') => return self.refuse(Refusal::Invalid(Part::Encoding)),
            (AfterSemicolon | Name, b'=') => (Value, false),
            (AfterSemicolon | Name, _) if is_token(b) => (Name, true),
            (AfterSemicolon | Name | Value | QuotedDone, b';') => (AfterSemicolon, false),
            (Name | Value | QuotedDone, b'\r') => (Done, false),
            (Value, b'"') => (Quoted, true),
            (Value, _) if is_token(b) => (Value, true),
            (Quoted, b'"') => (QuotedDone, true),
            (Quoted, b'\\') => (Pair, true),
            (Quoted, _) if quoted => (Quoted, true),
            (Pair, _) if quoted && b != 0x7f => (Quoted, true),
            _ => return self.refuse(Refusal::Invalid(Part::Encoding)),
        };
        if counted {
            *count += 1;
            if *count > self.limits.max_chunk_extensions as u64 {
                return self.refuse(Refusal::TooLarge);
            }
        }
        self.extension = next;
        Ok(next == Done)
    }

    /// Reads a byte of the trailer section and returns whether it ends it,
    /// the trailers dropped (quirk Q32).
    ///
    /// # Errors
    ///
    /// Returns an error for a byte llhttp refuses there or for the count of
    /// its names and values.
    pub(crate) fn trailer(&mut self, b: u8) -> io::Result<bool> {
        let limits = self.limits;
        let parser = self
            .trailers
            .get_or_insert_with(|| NodeParser::trailers(limits));
        match parser.trailer(b) {
            Ok(end) => Ok(end),
            Err(refusal) => self.refuse(refusal),
        }
    }

    fn refuse(&mut self, refusal: Refusal) -> io::Result<bool> {
        Err(io::Error::new(io::ErrorKind::InvalidData, refusal))
    }
}
