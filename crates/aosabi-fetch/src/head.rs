// SPDX-License-Identifier: MIT
/*
 * aosabi-fetch: the request line, the fields and the framing of a response
 * of HTTP/1.1.
 *
 * No analogue file on the reference side. The format is that of RFC 9112
 * sections 2 to 7, https://www.rfc-editor.org/rfc/rfc9112.
 */

//! `check_request`, `write_request` and `read_response`.
//!
//! [`write_request`] writes the request line with the target in origin form,
//! `host` from the URL and `connection`, the fields the caller set in their
//! order, `content-length` for a body, and the body, so the request is the
//! caller's and the client adds what HTTP/1.1 needs alone. The order is the
//! one the fetch of Node writes, `host` and `connection` first and
//! `content-length` last. The option of `connection` is `keep-alive`, since
//! the client keeps a connection for the next request to its origin, and
//! `close` on a request of `HEAD` and on one whose own field `connection`
//! reads `close`, after which the client keeps no connection, as undici
//! writes the field, and as RFC 9112 section 9.6 asks a client that will
//! close to say. [`check_request`] normalizes the method as the Fetch
//! standard normalizes it, and refuses before it connects, with the texts of
//! undici, a method that is no token, `CONNECT`, `TRACE` and `TRACK`, a body
//! on `GET` or `HEAD`, the fields `transfer-encoding`, `keep-alive`,
//! `upgrade` and `expect`, and a field `connection` that reads neither
//! `close` nor `keep-alive`. The fields `host`, `connection` and
//! `content-length` of the caller give way to the client's own. A request
//! that carries field lines of its own, which the executor of Federation sets
//! as node:http writes them, has those lines written in place of its
//! `Headers`, each name as spelled and each value as it stands, and the three
//! fields of the client give way the same way, the name read in any case.
//!
//! [`read_response`] reads the status line and the fields of each answer,
//! skips an interim answer of status 1xx, and reads the body of the final one
//! framed as RFC 9112 section 6.3 orders it: no body for an answer to `HEAD`
//! and for the status 1xx, 204 and 304, `chunked` when it is the last
//! transfer coding, the end of the connection under any other transfer
//! coding, `Content-Length`, and the end of the connection when neither field
//! is present. A line ends with CRLF or a bare LF, which section 2.2 lets a
//! recipient read, and an obsolete line fold is replaced with a space, which
//! section 5.2 asks of a user agent. The signs of smuggling and the framings
//! sections 6.1, 6.3 and 5.1 call invalid end the call with `Protocol`: a
//! `Content-Length` beside a `Transfer-Encoding`, a `Content-Length` with two
//! different values or one that is no number, a `Transfer-Encoding` in an
//! answer of HTTP/1.0, whitespace between a field name and its colon, a bare
//! CR, and heads above [`HEAD_LIMIT`] bytes, the client's own bound. A body
//! framed by the end of a TLS connection that ends with no close
//! notification is cut, which section 9.8 counts as failed.
//!
//! Beside the answer it tells whether the connection can carry the next
//! request: an answer of HTTP/1.1 whose fields `connection` and
//! `proxy-connection` name no option `close` as llhttp reads them, whose body
//! the end of the connection does not frame and after which the connection
//! holds no byte, as undici keeps the socket of an answer that llhttp lets it
//! keep and destroys one that holds the bytes of no request. It reads the
//! timeout the field `Keep-Alive` names as undici reads it, the digits after
//! the first `timeout=` of the values of every such field joined with nothing
//! between them.

use core::{
    future::poll_fn,
    pin::Pin,
    task::Poll, //
};
use std::{
    borrow::Cow,
    io::{
        self,
        ErrorKind, //
    },
};

use aosabi_core::net::FetchError;
use aosabi_http::{
    Body,
    Headers,
    HeadersInit,
    Request,
    Response,
    ResponseInit,
    Url,
    parse_url, //
};
use aosabi_serve::runtime::LocalBoxFuture;
use hyper::rt::{
    Read,
    ReadBuf,
    Write, //
};

use crate::{
    body::BodyBuffer,
    error::{
        self,
        protocol, //
    },
};

/// The bytes the heads of one answer hold at most, the interim heads with the
/// final one, and the bytes of one line of the chunked coding or of its
/// trailer section.
pub const HEAD_LIMIT: usize = 65_536;

/// The bytes one read of the connection takes at most.
const READ_SIZE: usize = 16_384;

/// The fields the client writes itself, which a field of the caller of the
/// same name does not replace.
const OWNED: [&str; 3] = ["host", "connection", "content-length"];

/// A request the client can write, with its normalized method and whether it closes its connection.
#[derive(Debug)]
pub struct Checked<'a> {
    /// The method, normalized as the Fetch standard normalizes it.
    pub method: Cow<'a, str>,
    /// Whether the request writes `connection: close`, a request of `HEAD`
    /// and one whose own field `connection` reads `close`, after which the
    /// client keeps no connection.
    pub close: bool,
}

/// Returns the normalized method of `request` and whether it closes its connection, once the client can write it.
///
/// The method is normalized as the Fetch standard normalizes it: `DELETE`,
/// `GET`, `HEAD`, `OPTIONS`, `POST` and `PUT` in upper case whatever their
/// case, and any other method as it is. The client checks a request before it
/// connects, as the constructor of the `Request` of undici checks it before
/// the fetch starts.
///
/// # Errors
///
/// Returns `Protocol` with the text of the error of undici for a method that
/// is no token, for `CONNECT`, `TRACE` and `TRACK`, which the Fetch standard
/// forbids, for a body on `GET` or `HEAD`, for a field `transfer-encoding`,
/// `keep-alive`, `upgrade` or `expect`, which undici refuses since the client
/// frames the request and its connection itself, and for a field
/// `connection` that reads neither `close` nor `keep-alive`.
pub fn check_request(request: &Request) -> Result<Checked<'_>, FetchError> {
    let method: &str = &request.method;
    if method.is_empty() || !method.bytes().all(is_tchar) {
        return Err(protocol(&format!("'{method}' is not a valid HTTP method.")));
    }
    let upper = method.to_ascii_uppercase();
    if matches!(upper.as_str(), "CONNECT" | "TRACE" | "TRACK") {
        return Err(protocol(&format!("'{method}' HTTP method is unsupported.")));
    }
    if !request.body().is_empty() && matches!(upper.as_str(), "GET" | "HEAD") {
        return Err(protocol("Request with GET/HEAD method cannot have body."));
    }
    let mut close = upper == "HEAD";
    let headers = request.headers.borrow();
    for (name, _) in headers.list() {
        match name.as_str() {
            "transfer-encoding" | "keep-alive" | "upgrade" => {
                return Err(protocol(&format!("invalid {name} header")));
            }
            "connection" => close |= connection_close(&headers)?,
            "expect" => return Err(protocol("expect header not supported")),
            _ => {}
        }
    }
    let known = matches!(
        upper.as_str(),
        "DELETE" | "GET" | "HEAD" | "OPTIONS" | "POST" | "PUT"
    );
    let method = if known && upper != method {
        Cow::Owned(upper)
    } else {
        Cow::Borrowed(method)
    };
    Ok(Checked { method, close })
}

/// Returns whether the field `connection` of `headers` reads `close`, its
/// values joined as a `Headers` joins them.
///
/// The value is compared in lower case, as undici compares it after
/// `toLowerCase`, which maps no code point a `Headers` holds onto a letter of
/// ASCII but that letter in upper case, so the lower case of ASCII alone
/// gives the same answer.
///
/// # Errors
///
/// Returns `Protocol` with the text of undici for a value that reads neither
/// `close` nor `keep-alive`.
fn connection_close(headers: &Headers) -> Result<bool, FetchError> {
    let value = headers
        .get("connection")
        .unwrap_or_default()
        .to_ascii_lowercase();
    match value.as_str() {
        "close" => Ok(true),
        "keep-alive" => Ok(false),
        _ => Err(protocol("invalid connection header")),
    }
}

/// Writes the request line in origin form, `host`, `connection`, the caller's fields in order and `content-length`.
///
/// The option of `connection` is `close` where [`check_request`] answers that
/// the request closes its connection, and `keep-alive` otherwise. The body of
/// the request is read whole and written after the head, and a `POST` or a
/// `PUT` with no body writes `content-length: 0`, as the Fetch standard asks.
///
/// # Errors
///
/// The future answers `InvalidUrl` for a URL that does not parse, what
/// [`check_request`] answers, `Protocol` for a body that fails, and the
/// failure of the write.
pub fn write_request<'a, I>(
    io: &'a mut I,
    request: &'a Request,
) -> LocalBoxFuture<'a, Result<(), FetchError>>
where
    I: Write + Unpin,
{
    Box::pin(async move {
        let url = parse_url(&request.url, None)
            .map_err(|_| FetchError::InvalidUrl(request.url.clone()))?;
        let checked = check_request(request)?;
        let body = match request.body() {
            Body::Empty => None,
            body => Some(body.bytes().await.map_err(|failure| {
                protocol(&format!("the body of the request failed: {failure}"))
            })?),
        };
        let length = match &body {
            Some(bytes) => Some(bytes.len()),
            None if matches!(&*checked.method, "POST" | "PUT") => Some(0),
            None => None,
        };
        let mut out = Vec::with_capacity(512 + length.unwrap_or(0));
        match request.wire_fields() {
            Some(lines) => write_head(&mut out, &checked, &url, lines, length)?,
            None => write_head(
                &mut out,
                &checked,
                &url,
                request.headers.borrow().list(),
                length,
            )?,
        }
        if let Some(bytes) = &body {
            out.extend_from_slice(bytes);
        }
        write_all(io, &out)
            .await
            .map_err(|failure| error::io(&failure))
    })
}

/// Writes the head of the request `checked` with the field lines `fields` into `out`.
fn write_head(
    out: &mut Vec<u8>,
    checked: &Checked<'_>,
    url: &Url,
    fields: &[(String, String)],
    length: Option<usize>,
) -> Result<(), FetchError> {
    out.extend_from_slice(checked.method.as_bytes());
    out.push(b' ');
    out.extend_from_slice(origin_form(url).as_bytes());
    out.extend_from_slice(b" HTTP/1.1\r\n");
    push_field(out, "host", &url.host)?;
    let option = if checked.close { "close" } else { "keep-alive" };
    push_field(out, "connection", option)?;
    for (name, value) in fields {
        if !OWNED.iter().any(|owned| owned.eq_ignore_ascii_case(name)) {
            push_field(out, name, value)?;
        }
    }
    if let Some(length) = length {
        push_field(out, "content-length", &length.to_string())?;
    }
    out.extend_from_slice(b"\r\n");
    Ok(())
}

/// Returns the target of a request in origin form, the path and the query of `url`.
///
/// The query keeps its `?` when it is empty, as `href` keeps it, and the
/// fragment is never sent.
fn origin_form(url: &Url) -> String {
    let before_hash = url.href.split('#').next().unwrap_or_default();
    let query = before_hash.find('?').map_or("", |at| &before_hash[at..]);
    format!("{}{query}", url.pathname)
}

/// Appends the field line of `name` and `value`, the value as Latin-1.
///
/// A value of a `Headers` holds no code point above U+00FF and no CR, LF or
/// NUL, and a line of a request's own field lines passed the stricter check
/// of node:http before it was set, so each code point is one byte on the
/// wire.
fn push_field(out: &mut Vec<u8>, name: &str, value: &str) -> Result<(), FetchError> {
    out.extend_from_slice(name.as_bytes());
    out.extend_from_slice(b": ");
    for c in value.chars() {
        let byte =
            u8::try_from(u32::from(c)).map_err(|_| protocol(&format!("invalid {name} header")))?;
        out.push(byte);
    }
    out.extend_from_slice(b"\r\n");
    Ok(())
}

/// Writes `bytes` whole and flushes them.
async fn write_all<I: Write + Unpin>(io: &mut I, mut bytes: &[u8]) -> io::Result<()> {
    while !bytes.is_empty() {
        let n = poll_fn(|cx| Pin::new(&mut *io).poll_write(cx, bytes)).await?;
        if n == 0 {
            return Err(ErrorKind::WriteZero.into());
        }
        bytes = &bytes[n..];
    }
    poll_fn(|cx| Pin::new(&mut *io).poll_flush(cx)).await
}

/// One answer and what it tells of the connection it came on.
#[derive(Debug)]
pub struct Answered {
    /// The answer.
    pub response: Response,
    /// Whether the connection can carry the next request: the answer is of
    /// HTTP/1.1, its `connection` and `proxy-connection` name no option
    /// `close` as llhttp reads them, the end of the connection does not frame
    /// its body, and the connection holds no byte after it.
    pub reusable: bool,
    /// The timeout in seconds the fields `Keep-Alive` of the answer name, as
    /// undici reads it, or `None` where they name none.
    pub timeout_s: Option<u64>,
}

/// Reads one answer, its body framed as RFC 9112 section 6.3 orders it, up to `ceiling` bytes.
///
/// `method` is the normalized method of the request, which decides whether
/// the answer has a body. The future answers what the answer tells of its
/// connection beside it, which the module documentation describes.
///
/// # Errors
///
/// The future answers `Protocol` for an answer the client does not read,
/// `BodyTooLarge` for a body above `ceiling`, and the failure of the read.
pub fn read_response<'a, I>(
    io: &'a mut I,
    method: &'a str,
    ceiling: usize,
) -> LocalBoxFuture<'a, Result<Answered, FetchError>>
where
    I: Read + Unpin,
{
    Box::pin(async move {
        let mut reader = Reader {
            io,
            buf: Vec::new(),
            pos: 0,
        };
        let mut budget = HEAD_LIMIT;
        let head = loop {
            let head = read_head(&mut reader, &mut budget).await?;
            match head.status {
                101 => {
                    return Err(protocol(
                        "a switch of protocols the request did not ask for",
                    ));
                }
                100..=199 => {}
                _ => break head,
            }
        };
        let framing = framing(&head, method)?;
        let body = read_body(&mut reader, framing, ceiling).await?;
        let reusable =
            head.minor > 0 && framing != Framing::Close && !head.close && reader.held().is_empty();
        let timeout_s = keep_alive_timeout(&head);
        let mut headers = Headers::new();
        for (name, value) in &head.fields {
            headers
                .append(name, value)
                .map_err(|refusal| protocol(&refusal.message))?;
        }
        let init = ResponseInit {
            status: Some(head.status),
            status_text: Some(head.reason),
            headers: Some(HeadersInit::Fetch(headers)),
        };
        let response =
            Response::new(body, Some(init)).map_err(|refusal| protocol(&refusal.to_string()))?;
        Ok(Answered {
            response,
            reusable,
            timeout_s,
        })
    })
}

/// Returns whether the field line of `name`, with the bytes `value` after
/// its colon, names the option `close` as llhttp reads it.
///
/// The parser llhttp reads the options of a field `connection` and of a field
/// `proxy-connection` alike, whatever the case of the name. It splits the
/// value at each comma, and an option counts as `close`, in any case, after
/// spaces and tabs and before spaces alone, while any other byte after it, a
/// tab among them, makes it an option of another name.
fn closes(name: &str, value: &[u8]) -> bool {
    if !name.eq_ignore_ascii_case("connection") && !name.eq_ignore_ascii_case("proxy-connection") {
        return false;
    }
    value.split(|&byte| byte == b',').any(|option| {
        let start = option
            .iter()
            .position(|&byte| !matches!(byte, b' ' | b'\t'))
            .unwrap_or(option.len());
        option[start..]
            .split_at_checked(5)
            .is_some_and(|(word, rest)| {
                word.eq_ignore_ascii_case(b"close") && rest.iter().all(|&byte| byte == b' ')
            })
    })
}

/// Returns the timeout in seconds the fields `Keep-Alive` of `head` name, or
/// `None`.
///
/// The values of every such field are joined with nothing between them, and
/// the timeout is the run of digits after the first `timeout=` a digit
/// follows, as the expression `timeout=(\d+)` of undici matches it, held at
/// `u64::MAX` where the run is longer.
fn keep_alive_timeout(head: &Head) -> Option<u64> {
    const KEY: &str = "timeout=";
    let joined: String = head
        .fields
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("keep-alive"))
        .map(|(_, value)| value.as_str())
        .collect();
    let mut rest = joined.as_str();
    while let Some(at) = rest.find(KEY) {
        rest = &rest[at + KEY.len()..];
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if digits > 0 {
            let seconds = rest.as_bytes()[..digits]
                .iter()
                .fold(0_u64, |value, digit| {
                    value
                        .saturating_mul(10)
                        .saturating_add(u64::from(digit - b'0'))
                });
            return Some(seconds);
        }
    }
    None
}

/// The status line and the fields of one answer.
struct Head {
    /// The minor version of HTTP/1.
    minor: u8,
    /// The status code.
    status: u16,
    /// The reason phrase as Latin-1.
    reason: String,
    /// The fields in order, the names as they came and the values as Latin-1.
    fields: Vec<(String, String)>,
    /// Whether a field `connection` or `proxy-connection` names the option
    /// `close` as llhttp reads it, which [`closes`] tells of each field line.
    close: bool,
}

/// How the body of an answer ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Framing {
    /// The answer has no body.
    Empty,
    /// The body holds the bytes `Content-Length` names.
    Length(u64),
    /// The body is in the chunked coding.
    Chunked,
    /// The body ends with the connection.
    Close,
}

/// A reader of the bytes of a connection with the bytes it holds ahead.
struct Reader<'a, I> {
    /// The connection.
    io: &'a mut I,
    /// The bytes read and the room of the next read.
    buf: Vec<u8>,
    /// The first byte of `buf` that is not consumed.
    pos: usize,
}

impl<I: Read + Unpin> Reader<'_, I> {
    /// Returns the bytes held and not consumed.
    fn held(&self) -> &[u8] {
        &self.buf[self.pos..]
    }

    /// Consumes `n` bytes of those held.
    fn consume(&mut self, n: usize) {
        self.pos += n;
    }

    /// Reads more bytes after those held, and answers how many, 0 at the end
    /// of the stream.
    async fn fill(&mut self) -> io::Result<usize> {
        if self.pos == self.buf.len() {
            self.buf.clear();
            self.pos = 0;
        }
        let held = self.buf.len();
        self.buf.resize(held + READ_SIZE, 0);
        let read = poll_fn(|cx| {
            let mut room = ReadBuf::new(&mut self.buf[held..]);
            match Pin::new(&mut *self.io).poll_read(cx, room.unfilled()) {
                Poll::Ready(Ok(())) => Poll::Ready(Ok(room.filled().len())),
                Poll::Ready(Err(failure)) => Poll::Ready(Err(failure)),
                Poll::Pending => Poll::Pending,
            }
        })
        .await;
        self.buf.truncate(held + read.as_ref().map_or(0, |n| *n));
        read
    }

    /// Reads more bytes, and answers `Protocol` at the end of the stream.
    async fn more(&mut self) -> Result<(), FetchError> {
        match self.fill().await {
            Ok(0) => Err(protocol("the connection closed before the answer ended")),
            Ok(_) => Ok(()),
            Err(failure) => Err(error::io(&failure)),
        }
    }

    /// Reads one line of at most the `budget` bytes left with its end, and
    /// answers it without its CRLF or bare LF.
    ///
    /// `what` names the part of the answer the bound is for.
    async fn line(&mut self, budget: &mut usize, what: &str) -> Result<Vec<u8>, FetchError> {
        loop {
            let held = self.held();
            if let Some(end) = held.iter().position(|&byte| byte == b'\n') {
                let used = end + 1;
                if used > *budget {
                    return Err(too_long(what));
                }
                *budget -= used;
                let mut line = held[..end].to_vec();
                self.consume(used);
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                if line.contains(&b'\r') {
                    return Err(protocol("a bare CR in the answer"));
                }
                return Ok(line);
            }
            if held.len() >= *budget {
                return Err(too_long(what));
            }
            self.more().await?;
        }
    }
}

/// Returns the failure of a part of the answer above its bound.
fn too_long(what: &str) -> FetchError {
    protocol(&format!("{what} passed {HEAD_LIMIT} bytes"))
}

/// Reads the status line and the fields of one answer.
// The SIMD path of `httparse` in `unsafe` code would read this head faster.
async fn read_head<I: Read + Unpin>(
    reader: &mut Reader<'_, I>,
    budget: &mut usize,
) -> Result<Head, FetchError> {
    const WHAT: &str = "the head of the answer";
    let line = reader.line(budget, WHAT).await?;
    let (minor, status, reason) = status_line(&line)?;
    let mut fields: Vec<(String, String)> = Vec::new();
    let mut close = false;
    loop {
        let line = reader.line(budget, WHAT).await?;
        let Some(&first) = line.first() else {
            break;
        };
        if matches!(first, b' ' | b'\t') {
            let Some((_, value)) = fields.last_mut() else {
                return Err(protocol("a folded line before the first field"));
            };
            let more = field_value(&line)?;
            if !more.is_empty() {
                value.push(' ');
                value.push_str(&more);
            }
            continue;
        }
        let (name, value) = field_line(&line)?;
        // A name is token bytes, so its colon follows its bytes in the line.
        close |= closes(&name, &line[name.len() + 1..]);
        fields.push((name, value));
    }
    Ok(Head {
        minor,
        status,
        reason,
        fields,
        close,
    })
}

/// Returns the minor version, the status and the reason of a status line.
///
/// The line reads `HTTP/1.` and a digit, a space, three digits of at least
/// 100, and a space with the reason when the reason is not left out (RFC 9112
/// section 4).
fn status_line(line: &[u8]) -> Result<(u8, u16, String), FetchError> {
    let refuse = || protocol("the answer starts with no status line of HTTP/1.1");
    let rest = line.strip_prefix(b"HTTP/1.").ok_or_else(refuse)?;
    let (&minor, rest) = rest.split_first().ok_or_else(refuse)?;
    if !minor.is_ascii_digit() {
        return Err(refuse());
    }
    let rest = rest.strip_prefix(b" ").ok_or_else(refuse)?;
    let (code, rest) = rest.split_at_checked(3).ok_or_else(refuse)?;
    if !code.iter().all(u8::is_ascii_digit) {
        return Err(refuse());
    }
    let status = code
        .iter()
        .fold(0_u16, |status, digit| status * 10 + u16::from(digit - b'0'));
    if status < 100 {
        return Err(refuse());
    }
    let reason = match rest {
        [] => &[][..],
        [b' ', reason @ ..] => reason,
        _ => return Err(refuse()),
    };
    if reason.iter().any(|&byte| is_control(byte)) {
        return Err(protocol("a control character in the reason phrase"));
    }
    Ok((minor - b'0', status, latin1(reason)))
}

/// Returns the name and the value of a field line.
fn field_line(line: &[u8]) -> Result<(String, String), FetchError> {
    let colon = line
        .iter()
        .position(|&byte| byte == b':')
        .ok_or_else(|| protocol("a field line with no colon"))?;
    let (name, value) = (&line[..colon], &line[colon + 1..]);
    if name
        .last()
        .is_some_and(|&byte| matches!(byte, b' ' | b'\t'))
    {
        return Err(protocol("whitespace between a field name and its colon"));
    }
    if name.is_empty() || !name.iter().copied().all(is_tchar) {
        return Err(protocol("a field name that is no token"));
    }
    Ok((latin1(name), field_value(value)?))
}

/// Returns a field value without the whitespace around it, as Latin-1.
fn field_value(value: &[u8]) -> Result<String, FetchError> {
    let start = value
        .iter()
        .position(|&byte| !matches!(byte, b' ' | b'\t'))
        .unwrap_or(value.len());
    let end = value
        .iter()
        .rposition(|&byte| !matches!(byte, b' ' | b'\t'))
        .map_or(start, |at| at + 1);
    let value = &value[start..end];
    if value.iter().any(|&byte| is_control(byte)) {
        return Err(protocol("a control character in a field value"));
    }
    Ok(latin1(value))
}

/// Returns how the body of the answer `head` to `method` ends.
fn framing(head: &Head, method: &str) -> Result<Framing, FetchError> {
    if method == "HEAD" || head.status < 200 || head.status == 204 || head.status == 304 {
        return Ok(Framing::Empty);
    }
    let codings = members(head, "transfer-encoding");
    let lengths = members(head, "content-length");
    if let Some(codings) = codings {
        if lengths.is_some() {
            return Err(protocol("a content-length beside a transfer-encoding"));
        }
        if head.minor == 0 {
            return Err(protocol("a transfer-encoding in an answer of HTTP/1.0"));
        }
        let last = codings
            .last()
            .ok_or_else(|| protocol("a transfer-encoding with no coding"))?;
        if last.eq_ignore_ascii_case("chunked") {
            return Ok(Framing::Chunked);
        }
        return Ok(Framing::Close);
    }
    let Some(lengths) = lengths else {
        return Ok(Framing::Close);
    };
    let mut length = None;
    for member in lengths {
        let invalid = || protocol("an invalid content-length");
        if !member.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(invalid());
        }
        let value: u64 = member.parse().map_err(|_| invalid())?;
        match length {
            Some(seen) if seen != value => {
                return Err(protocol("a content-length with two different values"));
            }
            _ => length = Some(value),
        }
    }
    length
        .map(Framing::Length)
        .ok_or_else(|| protocol("an invalid content-length"))
}

/// Returns the members of every list `name` holds in `head`, the empty
/// members skipped, or `None` when no field of the name is present.
fn members<'a>(head: &'a Head, name: &str) -> Option<Vec<&'a str>> {
    let mut found = false;
    let mut members = Vec::new();
    for (field, value) in &head.fields {
        if field.eq_ignore_ascii_case(name) {
            found = true;
            members.extend(
                value
                    .split(',')
                    .map(|member| member.trim_matches([' ', '\t']))
                    .filter(|member| !member.is_empty()),
            );
        }
    }
    found.then_some(members)
}

/// Reads the body `framing` names, up to `ceiling` bytes.
async fn read_body<I: Read + Unpin>(
    reader: &mut Reader<'_, I>,
    framing: Framing,
    ceiling: usize,
) -> Result<Body, FetchError> {
    match framing {
        Framing::Empty => Ok(Body::Empty),
        Framing::Length(length) => {
            let mut body = BodyBuffer::with_length(length, ceiling)?;
            let length = usize::try_from(length).map_err(|_| FetchError::BodyTooLarge)?;
            read_exact(reader, &mut body, length).await?;
            Ok(body.into_body())
        }
        Framing::Chunked => read_chunked(reader, ceiling).await,
        Framing::Close => {
            let mut body = BodyBuffer::new(ceiling);
            loop {
                let held = reader.held().len();
                body.push(reader.held())?;
                reader.consume(held);
                match reader.fill().await {
                    Ok(0) => return Ok(body.into_body()),
                    Ok(_) => {}
                    Err(failure) if failure.kind() == ErrorKind::UnexpectedEof => {
                        return Err(protocol(
                            "the body ended with the connection and no close notification of TLS",
                        ));
                    }
                    Err(failure) => return Err(error::io(&failure)),
                }
            }
        }
    }
}

/// Moves `length` bytes of the connection into `body`.
async fn read_exact<I: Read + Unpin>(
    reader: &mut Reader<'_, I>,
    body: &mut BodyBuffer,
    length: usize,
) -> Result<(), FetchError> {
    let mut left = length;
    while left > 0 {
        if reader.held().is_empty() {
            reader.more().await?;
        }
        let take = reader.held().len().min(left);
        body.push(&reader.held()[..take])?;
        reader.consume(take);
        left -= take;
    }
    Ok(())
}

/// Reads a body in the chunked coding and its trailer section, which is
/// read and dropped (RFC 9112 section 7.1).
async fn read_chunked<I: Read + Unpin>(
    reader: &mut Reader<'_, I>,
    ceiling: usize,
) -> Result<Body, FetchError> {
    const WHAT: &str = "a line of the chunked coding";
    let mut body = BodyBuffer::new(ceiling);
    loop {
        let mut budget = HEAD_LIMIT;
        let size = chunk_size(&reader.line(&mut budget, WHAT).await?)?;
        if size == 0 {
            break;
        }
        let size = usize::try_from(size).map_err(|_| FetchError::BodyTooLarge)?;
        if size > ceiling - body.len() {
            return Err(FetchError::BodyTooLarge);
        }
        read_exact(reader, &mut body, size).await?;
        if !reader.line(&mut budget, WHAT).await?.is_empty() {
            return Err(protocol("a chunk that does not end where its size says"));
        }
    }
    let mut budget = HEAD_LIMIT;
    while !reader
        .line(&mut budget, "the trailer section")
        .await?
        .is_empty()
    {}
    Ok(body.into_body())
}

/// Returns the size of a chunk from its line, its extensions ignored.
fn chunk_size(line: &[u8]) -> Result<u64, FetchError> {
    let digits = line
        .iter()
        .take_while(|byte| byte.is_ascii_hexdigit())
        .count();
    let rest = &line[digits..];
    let rest = rest
        .iter()
        .position(|&byte| !matches!(byte, b' ' | b'\t'))
        .map_or(&[][..], |at| &rest[at..]);
    if digits == 0 || !(rest.is_empty() || rest[0] == b';') {
        return Err(protocol("an invalid chunk size"));
    }
    line[..digits].iter().try_fold(0_u64, |size, &digit| {
        let value = u64::from(char::from(digit).to_digit(16).unwrap_or(0));
        size.checked_mul(16)
            .and_then(|size| size.checked_add(value))
            .ok_or(FetchError::BodyTooLarge)
    })
}

/// Returns whether `byte` is a `tchar` of a token (RFC 9110 section 5.6.2).
fn is_tchar(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'!' | b'#'
                | b'$'
                | b'%'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b'-'
                | b'.'
                | b'^'
                | b'_'
                | b'`'
                | b'|'
                | b'~'
        )
}

/// Returns whether `byte` is a control a field value and a reason phrase may
/// not hold, every C0 control but HTAB, and DEL (RFC 9110 section 5.5).
fn is_control(byte: u8) -> bool {
    (byte < 0x20 && byte != b'\t') || byte == 0x7f
}

/// Returns `bytes` as Latin-1, one code point per byte, as the fetch of Node
/// reads a field.
fn latin1(bytes: &[u8]) -> String {
    bytes.iter().copied().map(char::from).collect()
}
