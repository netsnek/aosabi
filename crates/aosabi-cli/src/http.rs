// SPDX-License-Identifier: MIT
/*
 * aosabi-cli: the http:// only client that pull and the readiness probe of dev
 * use.
 *
 * No analogue in the reference. No outbound client of the project exists, so
 * this client speaks HTTP/1.1 over std::net::TcpStream, blocks, needs no
 * runtime and refuses https://.
 */

//! The `http://` only client.
//!
//! `pull` posts the introspection query to a remote and the supervisor of
//! `dev` posts `{__typename}` to its child, and both go through [`post`]. It
//! blocks and starts no thread, so the command line needs no runtime for it.
//!
//! The request carries `Accept: */*`, the content type and the length that
//! `fetch` of the reference sends for a JSON post, `Connection: close` in
//! place of its `keep-alive`, and none of its other headers,
//! `Accept-Language`, `Sec-Fetch-Mode`, `User-Agent` and `Accept-Encoding`, so
//! a server answers identity. An answer is read whole: the status line and the
//! header lines up to the empty line, any number of interim `1xx` heads
//! skipped, then a body framed by `Transfer-Encoding: chunked`, by
//! `Content-Length`, or by the end of the connection. Every answer that breaks
//! one of these rules or a bound is an [`HttpError`], and none panics. A
//! redirect is returned as the answer it is, with its status and its body,
//! because the client follows none.

use core::fmt::{
    self,
    Write as _, //
};
use std::{
    io::{
        self,
        Read,
        Write, //
    },
    net::{
        SocketAddr,
        TcpStream,
        ToSocketAddrs, //
    },
    time::{
        Duration,
        Instant, //
    }, //
};

use crate::{
    codes,
    report::CliError, //
};

/// The time a connection may take to open.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// The time the whole answer may take, from the open connection on.
pub const READ_TIMEOUT: Duration = Duration::from_secs(30);

/// The largest head of an answer the client reads.
pub const HEAD_LIMIT: usize = 64 * 1024;

/// The largest body of an answer the client reads.
pub const BODY_LIMIT: usize = 16 * 1024 * 1024;

/// The bytes the client asks the socket for at once.
const READ_CHUNK: usize = 16 * 1024;

/// The bounds of one exchange.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// The time a connection may take to open.
    pub connect: Duration,
    /// The time the whole answer may take.
    pub read: Duration,
    /// The largest head, and the largest trailer of a chunked body.
    pub head: usize,
    /// The largest body.
    pub body: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            connect: CONNECT_TIMEOUT,
            read: READ_TIMEOUT,
            head: HEAD_LIMIT,
            body: BODY_LIMIT,
        }
    }
}

/// An `http://` URL split into what the client needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Url {
    /// The host, without the brackets of an IPv6 address.
    pub host: String,
    /// The port, 80 when the URL names none.
    pub port: u16,
    /// The path and the query, percent encoded, `/` at least.
    pub target: String,
    /// The value of the `Host` header.
    pub authority: String,
}

/// One answer, read whole.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Response {
    /// The status code.
    pub status: u16,
    /// The header lines in the order of the answer, the names as written.
    pub headers: Vec<(String, String)>,
    /// The body, unframed.
    pub body: Vec<u8>,
}

impl Response {
    /// Returns the value of the first header named `name`, compared without
    /// case.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(known, _)| known.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

/// Why an exchange failed.
#[derive(Debug)]
pub enum HttpError {
    /// The URL is `https://`, which the client does not speak, with the text
    /// behind the colon of its scheme, trimmed and with its leading slashes
    /// and back slashes stripped.
    Https(String, String),
    /// The URL names neither `http://` nor `https://`.
    Scheme(String),
    /// The URL is no valid `http://` URL, with the reason.
    Url(String, &'static str),
    /// The host does not resolve.
    Resolve(String, io::Error),
    /// No connection opened, with the last error.
    Connect(String, io::Error),
    /// No connection opened within the bound.
    ConnectTimeout(String, Duration),
    /// No whole answer within the bound.
    Timeout(Duration),
    /// The connection failed while the request or the answer travelled.
    Io(io::Error),
    /// The connection closed before any byte of an answer.
    ClosedBeforeHead,
    /// The connection closed inside the head.
    ClosedInHead,
    /// The head is larger than the bound.
    HeadTooLarge(usize),
    /// The first line of the head is no status line.
    StatusLine,
    /// A line of the head is no header line.
    HeaderLine,
    /// The `Content-Length` is no length, or two of them disagree.
    ContentLength,
    /// A chunk size is no size.
    ChunkSize,
    /// The body is larger than the bound.
    BodyTooLarge(usize),
    /// The connection closed before the end of the body.
    BodyShort,
    /// The answer carries a coding the client does not decode, with its name.
    Coding(String),
}

impl fmt::Display for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Https(url, _) => write!(
                f,
                "the URL {url} is https://, and this build of aosabi speaks http:// alone"
            ),
            Self::Scheme(url) => write!(f, "the URL {url} names no http:// or https:// scheme"),
            Self::Url(url, reason) => write!(f, "the URL {url} is no valid http:// URL: {reason}"),
            Self::Resolve(host, error) => write!(f, "the host {host} does not resolve: {error}"),
            Self::Connect(authority, error) => {
                write!(f, "the connection to {authority} failed: {error}")
            }
            Self::ConnectTimeout(authority, limit) => write!(
                f,
                "no connection to {authority} opened within {} seconds",
                limit.as_secs()
            ),
            Self::Timeout(limit) => {
                write!(
                    f,
                    "no whole answer arrived within {} seconds",
                    limit.as_secs()
                )
            }
            Self::Io(error) => write!(f, "the connection failed: {error}"),
            Self::ClosedBeforeHead => {
                write!(f, "the server closed the connection before it answered")
            }
            Self::ClosedInHead => write!(
                f,
                "the server closed the connection inside the head of its answer"
            ),
            Self::HeadTooLarge(limit) => {
                write!(f, "the head of the answer is larger than {limit} bytes")
            }
            Self::StatusLine => write!(f, "the answer starts with no valid status line"),
            Self::HeaderLine => write!(f, "the head of the answer holds a line that is no header"),
            Self::ContentLength => write!(f, "the answer carries an invalid Content-Length"),
            Self::ChunkSize => write!(f, "the answer carries an invalid chunk size"),
            Self::BodyTooLarge(limit) => {
                write!(f, "the body of the answer is larger than {limit} bytes")
            }
            Self::BodyShort => write!(
                f,
                "the server closed the connection before the end of the body"
            ),
            Self::Coding(coding) => write!(
                f,
                "the answer is coded {coding}, which this client does not decode"
            ),
        }
    }
}

impl From<HttpError> for CliError {
    fn from(error: HttpError) -> Self {
        match &error {
            HttpError::Https(url, rest) => codes::https_unsupported(url, &format!("http://{rest}")),
            HttpError::Scheme(url) => codes::url_scheme(url),
            _ => CliError::new(error.to_string()),
        }
    }
}

/// Posts `body` with the content type `content_type` to `url` under the
/// bounds of the module and returns the answer, whatever its status.
///
/// # Errors
///
/// Returns an [`HttpError`] for a URL the client does not speak, a
/// connection that fails or times out, and an answer that breaks the framing
/// of HTTP/1.1 or a bound.
pub fn post(url: &str, content_type: &str, body: &[u8]) -> Result<Response, HttpError> {
    post_with(url, content_type, body, Limits::default())
}

/// Posts `body` as [`post`] does, under the bounds `limits`.
///
/// # Errors
///
/// Returns an [`HttpError`] as [`post`] does.
pub fn post_with(
    url: &str,
    content_type: &str,
    body: &[u8],
    limits: Limits,
) -> Result<Response, HttpError> {
    let parsed = parse_url(url)?;
    let stream = connect(&parsed, limits.connect)?;
    let mut conn = Conn::new(stream, limits.read);
    let request = format!(
        "POST {} HTTP/1.1\r\nHost: {}\r\nAccept: */*\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        parsed.target,
        parsed.authority,
        body.len()
    );
    conn.write_all(request.as_bytes())?;
    conn.write_all(body)?;
    read_response(&mut conn, limits)
}

/// Returns `url` split into host, port and target.
///
/// The scheme is read without case, and the slashes and back slashes behind
/// it are skipped, as a URL parser does for a special scheme. A URL with
/// credentials is refused, as `fetch` refuses one, and the fragment is
/// dropped. The target is the path and the query with every byte outside
/// printable ASCII, the space, `"`, `<`, `>` and the back quote percent
/// encoded.
///
/// # Errors
///
/// Returns [`HttpError::Https`] for `https://` with the text behind its
/// slashes, [`HttpError::Scheme`] for any other scheme or none, and
/// [`HttpError::Url`] for an authority that is no host and port.
pub fn parse_url(url: &str) -> Result<Url, HttpError> {
    let text = url.trim_matches(|c: char| c.is_ascii_whitespace() || c.is_ascii_control());
    let Some((scheme, rest)) = text.split_once(':') else {
        return Err(HttpError::Scheme(url.to_owned()));
    };
    let rest = rest.trim_start_matches(['/', '\\']);
    if scheme.eq_ignore_ascii_case("https") {
        return Err(HttpError::Https(url.to_owned(), rest.to_owned()));
    }
    if !scheme.eq_ignore_ascii_case("http") {
        return Err(HttpError::Scheme(url.to_owned()));
    }
    let end = rest.find(['/', '\\', '?', '#']).unwrap_or(rest.len());
    let (authority, path) = rest.split_at(end);
    if authority.contains('@') {
        return Err(HttpError::Url(url.to_owned(), "it carries credentials"));
    }
    let (host, port) =
        split_host_port(authority).map_err(|reason| HttpError::Url(url.to_owned(), reason))?;
    let path = path.split('#').next().unwrap_or_default();
    let target = if path.starts_with('?') || path.is_empty() {
        format!("/{path}")
    } else {
        path.replace('\\', "/")
    };
    let shown_host = if host.contains(':') {
        format!("[{host}]")
    } else {
        host.clone()
    };
    let authority = if port == 80 {
        shown_host
    } else {
        format!("{shown_host}:{port}")
    };
    Ok(Url {
        host,
        port,
        target: percent_encode(&target),
        authority,
    })
}

/// Returns the host and the port of `authority`, 80 when no port is named.
fn split_host_port(authority: &str) -> Result<(String, u16), &'static str> {
    let (host, port) = if let Some(bracketed) = authority.strip_prefix('[') {
        let Some((host, after)) = bracketed.split_once(']') else {
            return Err("its IPv6 host has no closing bracket");
        };
        match after.strip_prefix(':') {
            Some(port) => (host, port),
            None if after.is_empty() => (host, ""),
            None => return Err("its host is followed by something other than a port"),
        }
    } else {
        authority.rsplit_once(':').unwrap_or((authority, ""))
    };
    if host.is_empty() {
        return Err("it names no host");
    }
    let port = if port.is_empty() {
        80
    } else if port.bytes().all(|byte| byte.is_ascii_digit()) {
        port.parse::<u16>()
            .map_err(|_| "its port is larger than 65535")?
    } else {
        return Err("its port is no number");
    };
    Ok((host.to_ascii_lowercase(), port))
}

/// Returns `target` with the bytes a request line cannot carry percent
/// encoded.
fn percent_encode(target: &str) -> String {
    let mut out = String::with_capacity(target.len());
    for byte in target.bytes() {
        if byte <= b' ' || byte >= 0x7f || matches!(byte, b'"' | b'<' | b'>' | b'`') {
            let _ = write!(out, "%{byte:02X}");
        } else {
            out.push(char::from(byte));
        }
    }
    out
}

/// Opens a connection to the first address of `url` that answers within
/// `limit`, counted over every address.
fn connect(url: &Url, limit: Duration) -> Result<TcpStream, HttpError> {
    let addresses: Vec<SocketAddr> = (url.host.as_str(), url.port)
        .to_socket_addrs()
        .map_err(|error| HttpError::Resolve(url.host.clone(), error))?
        .collect();
    let deadline = Instant::now() + limit;
    let mut last = io::Error::new(io::ErrorKind::NotFound, "no address");
    for address in addresses {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(HttpError::ConnectTimeout(url.authority.clone(), limit));
        }
        match TcpStream::connect_timeout(&address, remaining) {
            Ok(stream) => return Ok(stream),
            Err(error) if error.kind() == io::ErrorKind::TimedOut => {
                return Err(HttpError::ConnectTimeout(url.authority.clone(), limit));
            }
            Err(error) => last = error,
        }
    }
    Err(HttpError::Connect(url.authority.clone(), last))
}

/// A connection with the bytes read and not yet taken, and the deadline of
/// the answer.
struct Conn {
    stream: TcpStream,
    buf: Vec<u8>,
    at: usize,
    deadline: Instant,
    limit: Duration,
}

impl Conn {
    /// Returns the connection with the deadline `limit` from now.
    fn new(stream: TcpStream, limit: Duration) -> Self {
        Self {
            stream,
            buf: Vec::new(),
            at: 0,
            deadline: Instant::now() + limit,
            limit,
        }
    }

    /// Returns the time left before the deadline.
    fn remaining(&self) -> Result<Duration, HttpError> {
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(HttpError::Timeout(self.limit));
        }
        Ok(remaining)
    }

    /// Writes all of `bytes` before the deadline.
    fn write_all(&mut self, bytes: &[u8]) -> Result<(), HttpError> {
        let remaining = self.remaining()?;
        self.stream
            .set_write_timeout(Some(remaining))
            .map_err(HttpError::Io)?;
        self.stream
            .write_all(bytes)
            .map_err(|error| self.timeout_or(error))
    }

    /// Reads more bytes into the buffer and returns their count, 0 at the end
    /// of the connection.
    // A read into a buffer left unwritten would skip the zeroing.
    fn fill(&mut self) -> Result<usize, HttpError> {
        if self.at > 0 && self.at == self.buf.len() {
            self.buf.clear();
            self.at = 0;
        }
        let remaining = self.remaining()?;
        self.stream
            .set_read_timeout(Some(remaining))
            .map_err(HttpError::Io)?;
        let mut chunk = [0u8; READ_CHUNK];
        loop {
            match self.stream.read(&mut chunk) {
                Ok(count) => {
                    self.buf.extend_from_slice(&chunk[..count]);
                    return Ok(count);
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => return Err(self.timeout_or(error)),
            }
        }
    }

    /// Returns the bytes read and not yet taken.
    fn pending(&self) -> &[u8] {
        &self.buf[self.at..]
    }

    /// Takes `count` bytes of the pending ones.
    fn take(&mut self, count: usize) -> Vec<u8> {
        let taken = self.buf[self.at..self.at + count].to_vec();
        self.at += count;
        taken
    }

    /// Returns `error` as a timeout when the socket timed out.
    fn timeout_or(&self, error: io::Error) -> HttpError {
        if matches!(
            error.kind(),
            io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
        ) {
            HttpError::Timeout(self.limit)
        } else {
            HttpError::Io(error)
        }
    }

    /// Returns the next line without its line end, at most `limit` bytes
    /// long, or `None` at the end of the connection.
    // A SIMD scan for the line ends would read a head faster.
    fn line(&mut self, limit: usize, too_long: HttpError) -> Result<Option<Vec<u8>>, HttpError> {
        loop {
            if let Some(end) = self.pending().iter().position(|&byte| byte == b'\n') {
                if end > limit {
                    return Err(too_long);
                }
                let mut line = self.take(end + 1);
                line.pop();
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                return Ok(Some(line));
            }
            if self.pending().len() > limit {
                return Err(too_long);
            }
            if self.fill()? == 0 {
                return Ok(None);
            }
        }
    }
}

/// Reads one whole answer from `conn`.
fn read_response(conn: &mut Conn, limits: Limits) -> Result<Response, HttpError> {
    let (status, headers) = loop {
        let (status, headers) = read_head(conn, limits.head)?;
        if !(100..200).contains(&status) {
            break (status, headers);
        }
    };
    let response = Response {
        status,
        headers,
        body: Vec::new(),
    };
    if let Some(coding) = response.header("content-encoding").map(str::trim)
        && !coding.is_empty()
        && !coding.eq_ignore_ascii_case("identity")
    {
        return Err(HttpError::Coding(coding.to_owned()));
    }
    let body = if status == 204 || status == 304 {
        Vec::new()
    } else if let Some(encoding) = response.header("transfer-encoding") {
        let last = encoding.rsplit(',').next().unwrap_or_default().trim();
        if !last.eq_ignore_ascii_case("chunked") || encoding.contains(',') {
            return Err(HttpError::Coding(encoding.trim().to_owned()));
        }
        read_chunked(conn, limits)?
    } else if let Some(length) = content_length(&response)? {
        read_length(conn, length, limits.body)?
    } else {
        read_to_end(conn, limits.body)?
    };
    Ok(Response { body, ..response })
}

/// Reads the status line and the header lines of one head.
fn read_head(conn: &mut Conn, limit: usize) -> Result<(u16, Vec<(String, String)>), HttpError> {
    let mut size = 0;
    let mut first = true;
    let mut status = 0;
    let mut headers = Vec::new();
    loop {
        let left = limit.saturating_sub(size);
        let Some(line) = conn.line(left, HttpError::HeadTooLarge(limit))? else {
            if first && conn.pending().is_empty() && size == 0 {
                return Err(HttpError::ClosedBeforeHead);
            }
            return Err(HttpError::ClosedInHead);
        };
        size += line.len() + 2;
        if size > limit {
            return Err(HttpError::HeadTooLarge(limit));
        }
        if first {
            status = status_of(&line)?;
            first = false;
            continue;
        }
        if line.is_empty() {
            return Ok((status, headers));
        }
        headers.push(header_of(&line)?);
    }
}

/// Returns the status code of the status line `line`.
fn status_of(line: &[u8]) -> Result<u16, HttpError> {
    let rest = line.strip_prefix(b"HTTP/1.").ok_or(HttpError::StatusLine)?;
    let (minor, rest) = rest.split_first().ok_or(HttpError::StatusLine)?;
    if !minor.is_ascii_digit() || rest.first() != Some(&b' ') || rest.len() < 4 {
        return Err(HttpError::StatusLine);
    }
    let code = &rest[1..4];
    if !code.iter().all(u8::is_ascii_digit) || rest.get(4).is_some_and(|&byte| byte != b' ') {
        return Err(HttpError::StatusLine);
    }
    Ok(code
        .iter()
        .fold(0u16, |value, digit| value * 10 + u16::from(digit - b'0')))
}

/// Returns the name and the value of the header line `line`.
fn header_of(line: &[u8]) -> Result<(String, String), HttpError> {
    let colon = line
        .iter()
        .position(|&byte| byte == b':')
        .ok_or(HttpError::HeaderLine)?;
    let name = &line[..colon];
    let is_token = |byte: &u8| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(byte);
    if name.is_empty() || !name.iter().all(is_token) {
        return Err(HttpError::HeaderLine);
    }
    let value = String::from_utf8_lossy(&line[colon + 1..])
        .trim_matches([' ', '\t'])
        .to_owned();
    Ok((String::from_utf8_lossy(name).into_owned(), value))
}

/// Returns the `Content-Length` of `response`, `None` when it names none.
fn content_length(response: &Response) -> Result<Option<usize>, HttpError> {
    let mut length = None;
    for (name, value) in &response.headers {
        if !name.eq_ignore_ascii_case("content-length") {
            continue;
        }
        for part in value.split(',') {
            let part = part.trim();
            if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(HttpError::ContentLength);
            }
            let parsed = part.parse::<usize>().unwrap_or(usize::MAX);
            if length.is_some_and(|known| known != parsed) {
                return Err(HttpError::ContentLength);
            }
            length = Some(parsed);
        }
    }
    Ok(length)
}

/// Reads a body of `length` bytes.
fn read_length(conn: &mut Conn, length: usize, limit: usize) -> Result<Vec<u8>, HttpError> {
    if length > limit {
        return Err(HttpError::BodyTooLarge(limit));
    }
    while conn.pending().len() < length {
        if conn.fill()? == 0 {
            return Err(HttpError::BodyShort);
        }
    }
    Ok(conn.take(length))
}

/// Reads a body to the end of the connection.
fn read_to_end(conn: &mut Conn, limit: usize) -> Result<Vec<u8>, HttpError> {
    loop {
        if conn.pending().len() > limit {
            return Err(HttpError::BodyTooLarge(limit));
        }
        if conn.fill()? == 0 {
            let length = conn.pending().len();
            return Ok(conn.take(length));
        }
    }
}

/// Reads a chunked body and its trailer.
fn read_chunked(conn: &mut Conn, limits: Limits) -> Result<Vec<u8>, HttpError> {
    let mut body = Vec::new();
    loop {
        let line = conn
            .line(limits.head, HttpError::ChunkSize)?
            .ok_or(HttpError::BodyShort)?;
        let size = chunk_size(&line)?;
        if size == 0 {
            return read_trailer(conn, limits.head).map(|()| body);
        }
        if size > limits.body.saturating_sub(body.len()) {
            return Err(HttpError::BodyTooLarge(limits.body));
        }
        body.extend(read_length(conn, size, usize::MAX)?);
        let end = conn
            .line(2, HttpError::ChunkSize)?
            .ok_or(HttpError::BodyShort)?;
        if !end.is_empty() {
            return Err(HttpError::ChunkSize);
        }
    }
}

/// Returns the size of the chunk size line `line`, its extensions dropped.
fn chunk_size(line: &[u8]) -> Result<usize, HttpError> {
    let digits = line.split(|&byte| byte == b';').next().unwrap_or_default();
    let digits = digits.trim_ascii();
    if digits.is_empty() || !digits.iter().all(u8::is_ascii_hexdigit) {
        return Err(HttpError::ChunkSize);
    }
    digits.iter().try_fold(0usize, |size, &digit| {
        size.checked_mul(16)
            .and_then(|size| size.checked_add(usize::from(hex_value(digit))))
            .ok_or(HttpError::ChunkSize)
    })
}

/// Returns the value of the hexadecimal digit `digit`, which the caller has
/// checked, and 0 for any other byte.
fn hex_value(digit: u8) -> u8 {
    match digit {
        b'0'..=b'9' => digit - b'0',
        b'a'..=b'f' => digit - b'a' + 10,
        b'A'..=b'F' => digit - b'A' + 10,
        _ => 0,
    }
}

/// Reads the trailer lines of a chunked body up to the empty line.
fn read_trailer(conn: &mut Conn, limit: usize) -> Result<(), HttpError> {
    let mut size = 0;
    loop {
        let line = conn
            .line(limit.saturating_sub(size), HttpError::HeadTooLarge(limit))?
            .ok_or(HttpError::BodyShort)?;
        if line.is_empty() {
            return Ok(());
        }
        size += line.len() + 2;
    }
}
