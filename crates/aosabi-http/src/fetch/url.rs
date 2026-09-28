// SPDX-License-Identifier: MIT
/*
 * aosabi-http: the URL of a request and the parser of a form body.
 *
 * Behind the feature node-server it parses the host a request head carries, with domain to ASCII
 * of https://url.spec.whatwg.org/#concept-domain-to-ascii over UTS 46,
 * https://www.unicode.org/reports/tr46/, and Punycode, RFC 3492, as ada 3.3.0, the URL parser of
 * Node 24.13.0, runs it.
 *
 * It carries the half of fetch/url.rs of the quirk Q54_NODE_ADAPTER_URL of
 * docs/byte-equality.md section 13.1.
 */

//! The URL of a request and the parser of `application/x-www-form-urlencoded`.
//!
//! Without the feature `node-server`, which is the Worker, a URL is the one the platform hands
//! over, already serialized, and [`parse_url`] splits it into its parts, resolving a target that
//! is no URL against the base the caller names, as Yoga reads
//! `new URL(request.url, 'http://localhost')`.
//!
//! Behind the feature `node-server` the adapter builds the URL of a request from the bytes of
//! its head, read as Latin-1 as Node reads them, so [`parse_url`] is the basic URL parser of the
//! WHATWG URL Standard for the special schemes: the scheme, the userinfo, the host parser with
//! its IPv4 number forms, IPv6 and forbidden code points, the port with the scheme's default
//! dropped, the dot segments, the percent encode sets of path, query and special query, and
//! domain to ASCII with the mapping and the checks of UTS 46 and Punycode as ada 3.3.0 runs them
//! for `new URL` of Node 24.13.0, over ada's tables. A URL of any other scheme is refused,
//! because no request carries one. Row C50 pins the parser with its URL cases.
//!
//! The absolute URL an outbound call reaches is the same [`Url`] with its
//! [`Url::href`] and [`Url::origin`], [`Url::set_pathname`] replaces its path
//! as the setter of `url.pathname` does, and [`UrlSearchParams`] holds the
//! pairs of a query in order and writes them back through the urlencoded
//! serializer, the `toString()` of `URLSearchParams`.

/// A parsed URL, the parts `new URL(...)` answers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Url {
    /// The serialization, `url.href`.
    pub href: String,
    /// The scheme with its colon, `url.protocol`.
    pub protocol: String,
    /// The user name, percent encoded, `url.username`.
    pub username: String,
    /// The password, percent encoded, `url.password`.
    pub password: String,
    /// The host with its port, `url.host`.
    pub host: String,
    /// The host without its port, `url.hostname`.
    pub hostname: String,
    /// The port, empty for the default of the scheme, `url.port`.
    pub port: String,
    /// The path, `url.pathname`.
    pub pathname: String,
    /// The query with its `?`, empty when it has none or an empty one, `url.search`.
    pub search: String,
    /// The fragment with its `#`, empty when it has none or an empty one, `url.hash`.
    pub hash: String,
}

impl Url {
    /// Returns the origin, `url.origin`, the scheme, the host and the port.
    #[must_use]
    pub fn origin(&self) -> String {
        format!("{}//{}", self.protocol, self.host)
    }

    /// Returns the pairs of the query, `url.searchParams`.
    #[must_use]
    pub fn search_params(&self) -> Vec<(String, String)> {
        url_search_params(self.search.strip_prefix('?').unwrap_or(&self.search))
    }

    /// Replaces the path with `value`, `url.pathname = value`.
    ///
    /// So one leading `/` or `\` is read, `\` separates segments as `/` does, the segments `.`
    /// and `..` resolve, and every code point of the path percent encode set is escaped, `?` and
    /// `#` among them, since the override ends no segment at either. The query and the fragment
    /// of `href` stay as they are.
    pub fn set_pathname(&mut self, value: &str) {
        let pathname = path_of(value);
        // The scheme, the userinfo and the host hold no `?` and no `#`, the
        // userinfo escaping both, so the first of them after the scheme
        // starts the query or the fragment.
        let tail = self.href.find("://").map_or(self.href.len(), |at| {
            let authority = at + 3;
            self.href[authority..]
                .find(['?', '#'])
                .map_or(self.href.len(), |end| authority + end)
        });
        let (head, rest) = self.href.split_at(tail);
        let head = head.strip_suffix(self.pathname.as_str()).unwrap_or(head);
        self.href = format!("{head}{pathname}{rest}");
        self.pathname = pathname;
    }
}

/// The pairs of a query in order, `URLSearchParams`.
///
/// The pairs come from the urlencoded parser of [`url_search_params`], and
/// the `Display` of the list is the urlencoded serializer, so `to_string`
/// answers what `params.toString()` answers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UrlSearchParams {
    /// The pairs of a name and a value, in order.
    list: Vec<(String, String)>,
}

impl UrlSearchParams {
    /// Returns a list with no pair, `new URLSearchParams()`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the pairs of `query`, `new URLSearchParams(query)`.
    ///
    /// A leading `?` is dropped, as the constructor drops it from a string.
    #[must_use]
    pub fn from_query(query: &str) -> Self {
        Self {
            list: url_search_params(query.strip_prefix('?').unwrap_or(query)),
        }
    }

    /// Appends the pair of `name` and `value`, `params.append(name, value)`.
    pub fn append(&mut self, name: &str, value: &str) {
        self.list.push((String::from(name), String::from(value)));
    }

    /// Sets `name` to `value`, `params.set(name, value)`.
    ///
    /// The first pair of the name takes `value` and every later pair of the name is removed, and
    /// a name with no pair is appended.
    pub fn set(&mut self, name: &str, value: &str) {
        let mut seen = false;
        self.list.retain_mut(|(key, slot)| {
            if key != name {
                return true;
            }
            if seen {
                return false;
            }
            seen = true;
            value.clone_into(slot);
            true
        });
        if !seen {
            self.append(name, value);
        }
    }

    /// Returns whether a pair of `name` exists, `params.has(name)`.
    #[must_use]
    pub fn has(&self, name: &str) -> bool {
        self.list.iter().any(|(key, _)| key == name)
    }

    /// Returns the value of the first pair of `name`, `params.get(name)`.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        self.list
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    /// Returns the pairs in order, what iterating `params` gives.
    #[must_use]
    pub fn pairs(&self) -> &[(String, String)] {
        &self.list
    }
}

impl core::fmt::Display for UrlSearchParams {
    /// Writes the pairs as the urlencoded serializer writes them.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        for (i, (name, value)) in self.list.iter().enumerate() {
            if i > 0 {
                f.write_str("&")?;
            }
            form_encode(f, name)?;
            f.write_str("=")?;
            form_encode(f, value)?;
        }
        Ok(())
    }
}

/// The failure of the parser, the `TypeError` `Invalid URL`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UrlError;

impl core::fmt::Display for UrlError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("Invalid URL")
    }
}

impl core::error::Error for UrlError {}

/// Returns the pairs of `query`, the parser of `application/x-www-form-urlencoded`.
#[must_use]
pub fn url_search_params(query: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    for sequence in query.split('&') {
        if sequence.is_empty() {
            continue;
        }
        let (name, value) = sequence.split_once('=').unwrap_or((sequence, ""));
        pairs.push((form_decode(name), form_decode(value)));
    }
    pairs
}

/// Returns a part of a form body with `+` read as a space and its escapes decoded.
fn form_decode(part: &str) -> String {
    let bytes: Vec<u8> = part
        .bytes()
        .map(|byte| if byte == b'+' { b' ' } else { byte })
        .collect();
    String::from_utf8_lossy(&percent_decode(&bytes)).into_owned()
}

/// Writes `text` with the `application/x-www-form-urlencoded` percent encode set and a space as `+`.
///
/// The set leaves the ASCII alphanumerics, `*`, `-`, `.` and `_` as they are and escapes every
/// other byte of the UTF-8 form as `%XX` with upper case digits, the space aside.
fn form_encode(out: &mut impl core::fmt::Write, text: &str) -> core::fmt::Result {
    for byte in text.bytes() {
        match byte {
            b' ' => out.write_char('+')?,
            b'*' | b'-' | b'.' | b'_' | b'0'..=b'9' | b'A'..=b'Z' | b'a'..=b'z' => {
                out.write_char(char::from(byte))?;
            }
            _ => write!(out, "%{byte:02X}")?,
        }
    }
    Ok(())
}

/// Returns `bytes` with every escape `%XX` decoded, a `%` that starts no escape kept.
#[must_use]
pub fn percent_decode(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let (Some(high), Some(low)) =
                (hex_value(bytes.get(i + 1)), hex_value(bytes.get(i + 2)))
        {
            out.push(high * 16 + low);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

/// Returns the value of a hexadecimal digit.
fn hex_value(byte: Option<&u8>) -> Option<u8> {
    let digit = char::from(*byte?).to_digit(16)?;
    u8::try_from(digit).ok()
}

/// Returns the default port of a special scheme, `None` for another scheme.
fn default_port(scheme: &str) -> Option<u16> {
    match scheme {
        "http" | "ws" => Some(80),
        "https" | "wss" => Some(443),
        "ftp" => Some(21),
        _ => None,
    }
}

/// Returns whether `c` is in the path percent encode set.
///
/// The set is the C0 controls, every code point above `~`, the space, `"`, `#`, `<`, `>`, `?`,
/// `^`, `` ` ``, `{` and `}`.
fn in_path_set(c: char) -> bool {
    !(' '..='~').contains(&c)
        || matches!(c, ' ' | '"' | '#' | '<' | '>' | '?' | '^' | '`' | '{' | '}')
}

/// Returns whether `segment` is a single dot segment, `.` or `%2e`.
fn is_single_dot(segment: &str) -> bool {
    segment == "." || segment.eq_ignore_ascii_case("%2e")
}

/// Returns whether `segment` is a double dot segment, `..` and its escaped forms.
fn is_double_dot(segment: &str) -> bool {
    matches!(
        segment.to_ascii_lowercase().as_str(),
        ".." | ".%2e" | "%2e." | "%2e%2e"
    )
}

/// Returns the pathname `value` gives in the path start state with that state as its override.
fn path_of(value: &str) -> String {
    let mut chars = value
        .chars()
        .filter(|&c| !matches!(c, '\t' | '\n' | '\r'))
        .peekable();
    if matches!(chars.peek(), Some('/' | '\\')) {
        chars.next();
    }
    let mut path: Vec<String> = Vec::new();
    let mut buffer = String::new();
    loop {
        let c = chars.next();
        if let Some(c) = c.filter(|c| !matches!(c, '/' | '\\')) {
            push_path_encoded(&mut buffer, c);
            continue;
        }
        let slash = c.is_some();
        if is_double_dot(&buffer) {
            path.pop();
            if !slash {
                path.push(String::new());
            }
        } else if is_single_dot(&buffer) {
            if !slash {
                path.push(String::new());
            }
        } else {
            path.push(core::mem::take(&mut buffer));
        }
        buffer.clear();
        if !slash {
            break;
        }
    }
    format!("/{}", path.join("/"))
}

/// Appends `c` to `out`, its UTF-8 bytes percent encoded when it is in the path percent encode set.
fn push_path_encoded(out: &mut String, c: char) {
    if !in_path_set(c) {
        out.push(c);
        return;
    }
    let mut buffer = [0_u8; 4];
    for byte in c.encode_utf8(&mut buffer).bytes() {
        // A write into a string does not fail.
        let _ = core::fmt::Write::write_fmt(out, format_args!("%{byte:02X}"));
    }
}

/// Parses `input` against `base`, `new URL(input, base)`, as the platform serialized it.
///
/// A URL the platform hands over is serialized already, so its parts are
/// read by splitting it, and a target that names no scheme is resolved
/// against `base` by the same split.
///
/// # Errors
///
/// Returns [`UrlError`] for an input that names no special scheme with no
/// base to resolve it against.
#[cfg(not(feature = "node-server"))]
pub fn parse_url(input: &str, base: Option<&str>) -> Result<Url, UrlError> {
    let input = input.trim_matches(|c: char| c <= ' ');
    if names_special_scheme(input) {
        return split_serialized(input);
    }
    let base = split_serialized(base.ok_or(UrlError)?)?;
    let joined = if input.starts_with("//") {
        format!("{}{}", base.protocol, input)
    } else if input.starts_with('/') {
        format!("{}{}", base.origin(), input)
    } else if input.starts_with('?') {
        format!("{}{}{}", base.origin(), base.pathname, input)
    } else if input.starts_with('#') {
        format!("{}{}{}{}", base.origin(), base.pathname, base.search, input)
    } else {
        let dir = base.pathname.rsplit_once('/').map_or("", |(dir, _)| dir);
        format!("{}{}/{}", base.origin(), dir, input)
    };
    split_serialized(&joined)
}

/// Returns whether `input` starts with a special scheme and its colon, which makes it absolute.
#[cfg(not(feature = "node-server"))]
fn names_special_scheme(input: &str) -> bool {
    input
        .split_once(':')
        .is_some_and(|(scheme, _)| default_port(&scheme.to_ascii_lowercase()).is_some())
}

/// Splits a serialized special URL into its parts.
#[cfg(not(feature = "node-server"))]
fn split_serialized(href: &str) -> Result<Url, UrlError> {
    let (scheme, rest) = href.split_once(':').ok_or(UrlError)?;
    let scheme = scheme.to_ascii_lowercase();
    let rest = rest.strip_prefix("//").ok_or(UrlError)?;
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(authority_end);
    let (userinfo, host) = authority.rsplit_once('@').unwrap_or(("", authority));
    let (username, password) = userinfo.split_once(':').unwrap_or((userinfo, ""));
    let (hostname, port) = match host.rsplit_once(':') {
        Some((name, port)) if !name.ends_with(']') || host.ends_with(']') => {
            if host.starts_with('[') && !name.ends_with(']') {
                (host, "")
            } else {
                (name, port)
            }
        }
        _ => (host, ""),
    };
    if hostname.is_empty() {
        return Err(UrlError);
    }
    let port = match port.parse::<u16>() {
        Ok(port) if Some(port) == default_port(&scheme) => String::new(),
        Ok(port) => port.to_string(),
        Err(_) => String::new(),
    };
    let (before_hash, hash) = match tail.split_once('#') {
        Some((before, hash)) if !hash.is_empty() => (before, format!("#{hash}")),
        Some((before, _)) => (before, String::new()),
        None => (tail, String::new()),
    };
    let (path, search) = match before_hash.split_once('?') {
        Some((path, query)) if !query.is_empty() => (path, format!("?{query}")),
        Some((path, _)) => (path, String::new()),
        None => (before_hash, String::new()),
    };
    let pathname = if path.is_empty() {
        String::from("/")
    } else {
        String::from(path)
    };
    let hostname = hostname.to_ascii_lowercase();
    let host = if port.is_empty() {
        hostname.clone()
    } else {
        format!("{hostname}:{port}")
    };
    let userinfo = match (username.is_empty(), password.is_empty()) {
        (true, true) => String::new(),
        (false, true) => format!("{username}@"),
        _ => format!("{username}:{password}@"),
    };
    let href = format!(
        "{scheme}://{userinfo}{host}{pathname}{}{}",
        tail_query(before_hash),
        tail_hash(tail)
    );
    Ok(Url {
        href,
        protocol: format!("{scheme}:"),
        username: String::from(username),
        password: String::from(password),
        host,
        hostname,
        port,
        pathname,
        search,
        hash,
    })
}

/// Returns the query of a serialized URL with its `?`, an empty query kept.
#[cfg(not(feature = "node-server"))]
fn tail_query(before_hash: &str) -> String {
    before_hash
        .find('?')
        .map_or_else(String::new, |at| String::from(&before_hash[at..]))
}

/// Returns the fragment of a serialized URL with its `#`, an empty fragment kept.
#[cfg(not(feature = "node-server"))]
fn tail_hash(tail: &str) -> String {
    tail.find('#')
        .map_or_else(String::new, |at| String::from(&tail[at..]))
}

/// Parses `input` against `base`, `new URL(input, base)`, with the basic URL parser.
///
/// Translation of the basic URL parser of the WHATWG URL Standard with no state override and the
/// encoding UTF-8, for the special schemes but `file`.
///
/// # Errors
///
/// Returns [`UrlError`] where the parser returns failure, for an input of a
/// scheme that is not special, and for a base that does not parse.
#[cfg(feature = "node-server")]
pub fn parse_url(input: &str, base: Option<&str>) -> Result<Url, UrlError> {
    let base = match base {
        Some(base) => Some(basic::parse(base, None)?),
        None => None,
    };
    let record = basic::parse(input, base.as_ref())?;
    Ok(record.to_url())
}

/// Returns the origin of `input`, `new URL(input).origin`, read as [`parse_url`] reads `input`
/// with no base.
///
/// Without the feature `node-server` the split of a serialized URL reads it, so the module of a
/// Worker that checks the origin of a request holds no symbol of [`parse_url`], the name that
/// `tests/notebooks/08_card_builds.ipynb` refuses in a Worker, since behind `node-server` it is
/// the parser with domain to ASCII.
///
/// # Errors
///
/// Returns [`UrlError`] where [`parse_url`] with no base does.
pub fn origin_of(input: &str) -> Result<String, UrlError> {
    #[cfg(not(feature = "node-server"))]
    let url = {
        let input = input.trim_matches(|c: char| c <= ' ');
        if !names_special_scheme(input) {
            return Err(UrlError);
        }
        split_serialized(input)?
    };
    #[cfg(feature = "node-server")]
    let url = parse_url(input, None)?;
    Ok(url.origin())
}

/// The basic URL parser of the feature `node-server`.
#[cfg(feature = "node-server")]
mod basic {
    use core::fmt::Write;

    use super::{
        UrlError,
        default_port,
        host::parse_host,
        in_path_set,
        is_double_dot,
        is_single_dot, //
    };

    /// The URL record the parser fills.
    #[derive(Clone, Debug, Default)]
    pub(super) struct Record {
        /// The scheme, lower case.
        scheme: String,
        /// The user name, percent encoded.
        username: String,
        /// The password, percent encoded.
        password: String,
        /// The serialized host.
        host: String,
        /// The port, `None` for none or the default.
        port: Option<u16>,
        /// The segments of the path.
        path: Vec<String>,
        /// The query, `None` for none.
        query: Option<String>,
        /// The fragment, `None` for none.
        fragment: Option<String>,
    }

    impl Record {
        /// Returns the parts of the record, `new URL(...)` of the platform.
        pub(super) fn to_url(&self) -> super::Url {
            let port = self.port.map(|port| port.to_string()).unwrap_or_default();
            let host = if port.is_empty() {
                self.host.clone()
            } else {
                format!("{}:{port}", self.host)
            };
            let mut userinfo = String::new();
            if !self.username.is_empty() || !self.password.is_empty() {
                userinfo.push_str(&self.username);
                if !self.password.is_empty() {
                    userinfo.push(':');
                    userinfo.push_str(&self.password);
                }
                userinfo.push('@');
            }
            let pathname = format!("/{}", self.path.join("/"));
            let mut href = format!("{}://{userinfo}{host}{pathname}", self.scheme);
            if let Some(query) = &self.query {
                href.push('?');
                href.push_str(query);
            }
            if let Some(fragment) = &self.fragment {
                href.push('#');
                href.push_str(fragment);
            }
            let search = match &self.query {
                Some(query) if !query.is_empty() => format!("?{query}"),
                _ => String::new(),
            };
            let hash = match &self.fragment {
                Some(fragment) if !fragment.is_empty() => format!("#{fragment}"),
                _ => String::new(),
            };
            super::Url {
                href,
                protocol: format!("{}:", self.scheme),
                username: self.username.clone(),
                password: self.password.clone(),
                host,
                hostname: self.host.clone(),
                port,
                pathname,
                search,
                hash,
            }
        }
    }

    /// The states of the parser this port reaches.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum State {
        SchemeStart,
        Scheme,
        NoScheme,
        SpecialRelativeOrAuthority,
        Relative,
        RelativeSlash,
        SpecialAuthoritySlashes,
        SpecialAuthorityIgnoreSlashes,
        Authority,
        Host,
        Port,
        PathStart,
        Path,
        Query,
        Fragment,
    }

    /// The percent encode sets of the parser.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum EncodeSet {
        Fragment,
        SpecialQuery,
        Path,
        Userinfo,
    }

    /// Returns whether `c` is in the percent encode set `set`.
    fn in_set(c: char, set: EncodeSet) -> bool {
        let control = !(' '..='~').contains(&c);
        match set {
            EncodeSet::Fragment => control || matches!(c, ' ' | '"' | '<' | '>' | '`'),
            EncodeSet::SpecialQuery => control || matches!(c, ' ' | '"' | '#' | '<' | '>' | '\''),
            EncodeSet::Path => in_path_set(c),
            EncodeSet::Userinfo => {
                control
                    || matches!(
                        c,
                        ' ' | '"'
                            | '#'
                            | '<'
                            | '>'
                            | '?'
                            | '^'
                            | '`'
                            | '{'
                            | '}'
                            | '/'
                            | ':'
                            | ';'
                            | '='
                            | '@'
                            | '['
                            | '\\'
                            | ']'
                            | '|'
                    )
            }
        }
    }

    /// Appends `c` to `out`, its UTF-8 bytes percent encoded when it is in `set`.
    fn push_encoded(out: &mut String, c: char, set: EncodeSet) {
        if !in_set(c, set) {
            out.push(c);
            return;
        }
        let mut buffer = [0_u8; 4];
        for byte in c.encode_utf8(&mut buffer).bytes() {
            // A write into a string does not fail.
            let _ = write!(out, "%{byte:02X}");
        }
    }

    /// Returns the special scheme `scheme` names, `None` for any other.
    fn special(scheme: &str) -> bool {
        default_port(scheme).is_some()
    }

    /// Runs the basic URL parser over `input` with `base`.
    ///
    /// The input loses its leading and trailing C0 controls and spaces and
    /// every tab and newline before the states run over its code points.
    // The standard writes the parser as one state machine over one pointer,
    // and the function keeps its states side by side so that each reads
    // against its clause.
    #[expect(clippy::too_many_lines)]
    pub(super) fn parse(input: &str, base: Option<&Record>) -> Result<Record, UrlError> {
        let input: Vec<char> = input
            .trim_matches(|c: char| c <= ' ')
            .chars()
            .filter(|&c| !matches!(c, '\t' | '\n' | '\r'))
            .collect();
        let mut url = Record::default();
        let mut state = State::SchemeStart;
        let mut buffer = String::new();
        let mut at_sign_seen = false;
        let mut inside_brackets = false;
        let mut password_token_seen = false;
        let mut pointer: isize = 0;
        let len = isize::try_from(input.len()).map_err(|_| UrlError)?;
        loop {
            let c = usize::try_from(pointer)
                .ok()
                .and_then(|at| input.get(at))
                .copied();
            let remaining = |ahead: usize| {
                usize::try_from(pointer)
                    .ok()
                    .and_then(|at| input.get(at + ahead))
                    .copied()
            };
            match state {
                State::SchemeStart => match c {
                    Some(c) if c.is_ascii_alphabetic() => {
                        buffer.push(c.to_ascii_lowercase());
                        state = State::Scheme;
                    }
                    _ => {
                        state = State::NoScheme;
                        pointer -= 1;
                    }
                },
                State::Scheme => match c {
                    Some(c) if c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.') => {
                        buffer.push(c.to_ascii_lowercase());
                    }
                    Some(':') => {
                        if !special(&buffer) {
                            return Err(UrlError);
                        }
                        url.scheme = core::mem::take(&mut buffer);
                        state = match base {
                            Some(base) if base.scheme == url.scheme => {
                                State::SpecialRelativeOrAuthority
                            }
                            _ => State::SpecialAuthoritySlashes,
                        };
                    }
                    _ => {
                        buffer.clear();
                        state = State::NoScheme;
                        pointer = -1;
                    }
                },
                State::NoScheme => {
                    let Some(base) = base else {
                        return Err(UrlError);
                    };
                    if !special(&base.scheme) {
                        return Err(UrlError);
                    }
                    state = State::Relative;
                    pointer -= 1;
                }
                State::SpecialRelativeOrAuthority => {
                    if c == Some('/') && remaining(1) == Some('/') {
                        state = State::SpecialAuthorityIgnoreSlashes;
                        pointer += 1;
                    } else {
                        state = State::Relative;
                        pointer -= 1;
                    }
                }
                State::Relative => {
                    let base = base.ok_or(UrlError)?;
                    url.scheme.clone_from(&base.scheme);
                    if matches!(c, Some('/' | '\\')) {
                        state = State::RelativeSlash;
                    } else {
                        url.username.clone_from(&base.username);
                        url.password.clone_from(&base.password);
                        url.host.clone_from(&base.host);
                        url.port = base.port;
                        url.path.clone_from(&base.path);
                        url.query.clone_from(&base.query);
                        match c {
                            Some('?') => {
                                url.query = Some(String::new());
                                state = State::Query;
                            }
                            Some('#') => {
                                url.fragment = Some(String::new());
                                state = State::Fragment;
                            }
                            Some(_) => {
                                url.query = None;
                                url.path.pop();
                                state = State::Path;
                                pointer -= 1;
                            }
                            None => {}
                        }
                    }
                }
                State::RelativeSlash => {
                    if matches!(c, Some('/' | '\\')) {
                        state = State::SpecialAuthorityIgnoreSlashes;
                    } else {
                        let base = base.ok_or(UrlError)?;
                        url.username.clone_from(&base.username);
                        url.password.clone_from(&base.password);
                        url.host.clone_from(&base.host);
                        url.port = base.port;
                        state = State::Path;
                        pointer -= 1;
                    }
                }
                State::SpecialAuthoritySlashes => {
                    state = State::SpecialAuthorityIgnoreSlashes;
                    if c == Some('/') && remaining(1) == Some('/') {
                        pointer += 1;
                    } else {
                        pointer -= 1;
                    }
                }
                State::SpecialAuthorityIgnoreSlashes => {
                    if !matches!(c, Some('/' | '\\')) {
                        state = State::Authority;
                        pointer -= 1;
                    }
                }
                State::Authority => match c {
                    Some('@') => {
                        if at_sign_seen {
                            buffer.insert_str(0, "%40");
                        }
                        at_sign_seen = true;
                        for code_point in buffer.chars() {
                            if code_point == ':' && !password_token_seen {
                                password_token_seen = true;
                                continue;
                            }
                            let target = if password_token_seen {
                                &mut url.password
                            } else {
                                &mut url.username
                            };
                            push_encoded(target, code_point, EncodeSet::Userinfo);
                        }
                        buffer.clear();
                    }
                    None | Some('/' | '?' | '#' | '\\') => {
                        if at_sign_seen && buffer.is_empty() {
                            return Err(UrlError);
                        }
                        let back = isize::try_from(buffer.chars().count()).map_err(|_| UrlError)?;
                        pointer -= back + 1;
                        buffer.clear();
                        state = State::Host;
                    }
                    Some(c) => buffer.push(c),
                },
                State::Host => match c {
                    Some(':') if !inside_brackets => {
                        if buffer.is_empty() {
                            return Err(UrlError);
                        }
                        url.host = parse_host(&buffer)?;
                        buffer.clear();
                        state = State::Port;
                    }
                    None | Some('/' | '?' | '#' | '\\') => {
                        pointer -= 1;
                        if buffer.is_empty() {
                            return Err(UrlError);
                        }
                        url.host = parse_host(&buffer)?;
                        buffer.clear();
                        state = State::PathStart;
                    }
                    Some(c) => {
                        if c == '[' {
                            inside_brackets = true;
                        }
                        if c == ']' {
                            inside_brackets = false;
                        }
                        buffer.push(c);
                    }
                },
                State::Port => match c {
                    Some(c) if c.is_ascii_digit() => buffer.push(c),
                    None | Some('/' | '?' | '#' | '\\') => {
                        if !buffer.is_empty() {
                            let digits = buffer.trim_start_matches('0');
                            if digits.len() > 5 {
                                return Err(UrlError);
                            }
                            let port: u32 = if digits.is_empty() {
                                0
                            } else {
                                digits.parse().map_err(|_| UrlError)?
                            };
                            let port = u16::try_from(port).map_err(|_| UrlError)?;
                            url.port = (default_port(&url.scheme) != Some(port)).then_some(port);
                            buffer.clear();
                        }
                        state = State::PathStart;
                        pointer -= 1;
                    }
                    Some(_) => return Err(UrlError),
                },
                State::PathStart => {
                    state = State::Path;
                    if !matches!(c, Some('/' | '\\')) {
                        pointer -= 1;
                    }
                }
                State::Path => match c {
                    None | Some('/' | '\\' | '?' | '#') => {
                        let slash = matches!(c, Some('/' | '\\'));
                        if is_double_dot(&buffer) {
                            url.path.pop();
                            if !slash {
                                url.path.push(String::new());
                            }
                        } else if is_single_dot(&buffer) {
                            if !slash {
                                url.path.push(String::new());
                            }
                        } else {
                            url.path.push(core::mem::take(&mut buffer));
                        }
                        buffer.clear();
                        match c {
                            Some('?') => {
                                url.query = Some(String::new());
                                state = State::Query;
                            }
                            Some('#') => {
                                url.fragment = Some(String::new());
                                state = State::Fragment;
                            }
                            _ => {}
                        }
                    }
                    Some(c) => push_encoded(&mut buffer, c, EncodeSet::Path),
                },
                State::Query => match c {
                    None | Some('#') => {
                        let query = url.query.get_or_insert_with(String::new);
                        for code_point in buffer.chars() {
                            push_encoded(query, code_point, EncodeSet::SpecialQuery);
                        }
                        buffer.clear();
                        if c == Some('#') {
                            url.fragment = Some(String::new());
                            state = State::Fragment;
                        }
                    }
                    Some(c) => buffer.push(c),
                },
                State::Fragment => {
                    if let Some(c) = c {
                        push_encoded(
                            url.fragment.get_or_insert_with(String::new),
                            c,
                            EncodeSet::Fragment,
                        );
                    }
                }
            }
            if pointer >= len {
                break;
            }
            pointer += 1;
        }
        Ok(url)
    }
}

/// The host parser of the feature `node-server`.
#[cfg(feature = "node-server")]
mod host {
    use core::fmt::Write;

    use super::{
        UrlError,
        idna::domain_to_ascii,
        percent_decode, //
    };

    /// Returns the serialized host of `input`, the host parser of a special URL.
    pub(super) fn parse_host(input: &str) -> Result<String, UrlError> {
        if let Some(inner) = input.strip_prefix('[') {
            let inner = inner.strip_suffix(']').ok_or(UrlError)?;
            let address = parse_ipv6(inner)?;
            return Ok(format!("[{}]", serialize_ipv6(&address)));
        }
        let decoded = percent_decode(input.as_bytes());
        let domain = String::from_utf8_lossy(&decoded);
        let ascii = domain_to_ascii(&domain)?;
        if ends_in_a_number(&ascii) {
            let address = parse_ipv4(&ascii)?;
            return Ok(serialize_ipv4(address));
        }
        Ok(ascii)
    }

    /// Returns whether `input` ends in a number, the checker of the standard.
    fn ends_in_a_number(input: &str) -> bool {
        let mut parts: Vec<&str> = input.split('.').collect();
        if parts.last() == Some(&"") {
            if parts.len() == 1 {
                return false;
            }
            parts.pop();
        }
        let last = parts.last().copied().unwrap_or("");
        if !last.is_empty() && last.bytes().all(|byte| byte.is_ascii_digit()) {
            return true;
        }
        parse_ipv4_number(last).is_some()
    }

    /// Returns the number of one part of an IPv4 address, in decimal, octal or hexadecimal.
    fn parse_ipv4_number(input: &str) -> Option<u64> {
        if input.is_empty() {
            return None;
        }
        let (digits, radix) = if let Some(hex) = input
            .strip_prefix("0x")
            .or_else(|| input.strip_prefix("0X"))
        {
            (hex, 16)
        } else if input.len() > 1 && input.starts_with('0') {
            (&input[1..], 8)
        } else {
            (input, 10)
        };
        if digits.is_empty() {
            return Some(0);
        }
        let mut value: u64 = 0;
        for c in digits.chars() {
            let digit = c.to_digit(radix)?;
            value = value
                .saturating_mul(u64::from(radix))
                .saturating_add(u64::from(digit));
        }
        Some(value)
    }

    /// Returns the IPv4 address of `input`.
    fn parse_ipv4(input: &str) -> Result<u32, UrlError> {
        let mut parts: Vec<&str> = input.split('.').collect();
        if parts.last() == Some(&"") && parts.len() > 1 {
            parts.pop();
        }
        if parts.len() > 4 {
            return Err(UrlError);
        }
        let numbers: Vec<u64> = parts
            .iter()
            .map(|part| parse_ipv4_number(part).ok_or(UrlError))
            .collect::<Result<_, _>>()?;
        let (last, init) = numbers.split_last().ok_or(UrlError)?;
        if init.iter().any(|&number| number > 255) {
            return Err(UrlError);
        }
        let limit = 256_u64.pow(u32::try_from(5 - numbers.len()).map_err(|_| UrlError)?);
        if *last >= limit {
            return Err(UrlError);
        }
        let mut address = *last;
        for (i, &number) in init.iter().enumerate() {
            address += number * 256_u64.pow(u32::try_from(3 - i).map_err(|_| UrlError)?);
        }
        u32::try_from(address).map_err(|_| UrlError)
    }

    /// Returns the dotted decimal form of `address`.
    fn serialize_ipv4(address: u32) -> String {
        let [a, b, c, d] = address.to_be_bytes();
        format!("{a}.{b}.{c}.{d}")
    }

    /// Returns the IPv6 address of `input`, the eight pieces.
    fn parse_ipv6(input: &str) -> Result<[u16; 8], UrlError> {
        let chars: Vec<char> = input.chars().collect();
        let mut address = [0_u16; 8];
        let mut piece_index = 0_usize;
        let mut compress: Option<usize> = None;
        let mut pointer = 0_usize;
        let at = |i: usize| chars.get(i).copied();
        if at(pointer) == Some(':') {
            if at(pointer + 1) != Some(':') {
                return Err(UrlError);
            }
            pointer += 2;
            piece_index += 1;
            compress = Some(piece_index);
        }
        while at(pointer).is_some() {
            if piece_index == 8 {
                return Err(UrlError);
            }
            if at(pointer) == Some(':') {
                if compress.is_some() {
                    return Err(UrlError);
                }
                pointer += 1;
                piece_index += 1;
                compress = Some(piece_index);
                continue;
            }
            let mut value: u16 = 0;
            let mut length = 0;
            while length < 4
                && let Some(digit) = at(pointer).and_then(|c| c.to_digit(16))
            {
                value = value * 0x10 + u16::try_from(digit).map_err(|_| UrlError)?;
                pointer += 1;
                length += 1;
            }
            if at(pointer) == Some('.') {
                if length == 0 {
                    return Err(UrlError);
                }
                pointer -= length;
                if piece_index > 6 {
                    return Err(UrlError);
                }
                piece_index = parse_embedded_ipv4(&chars[pointer..], &mut address, piece_index)?;
                break;
            } else if at(pointer) == Some(':') {
                pointer += 1;
                if at(pointer).is_none() {
                    return Err(UrlError);
                }
            } else if at(pointer).is_some() {
                return Err(UrlError);
            }
            address[piece_index] = value;
            piece_index += 1;
        }
        if let Some(compress) = compress {
            let mut swaps = piece_index - compress;
            piece_index = 7;
            while piece_index != 0 && swaps > 0 {
                address.swap(piece_index, compress + swaps - 1);
                piece_index -= 1;
                swaps -= 1;
            }
        } else if piece_index != 8 {
            return Err(UrlError);
        }
        Ok(address)
    }

    /// Reads the IPv4 address at the end of an IPv6 address into the pieces from `piece_index` on.
    ///
    /// The part of the IPv6 parser of the standard that reads four decimal
    /// numbers, each without a leading zero and at most 255, into two pieces,
    /// and answers the index of the piece after them, which the compression
    /// of the parser reads.
    fn parse_embedded_ipv4(
        chars: &[char],
        address: &mut [u16; 8],
        mut piece_index: usize,
    ) -> Result<usize, UrlError> {
        let mut pointer = 0;
        let mut numbers_seen = 0;
        while pointer < chars.len() {
            let mut ipv4_piece: Option<u16> = None;
            if numbers_seen > 0 {
                if chars[pointer] == '.' && numbers_seen < 4 {
                    pointer += 1;
                } else {
                    return Err(UrlError);
                }
            }
            if !chars.get(pointer).is_some_and(char::is_ascii_digit) {
                return Err(UrlError);
            }
            while let Some(digit) = chars.get(pointer).and_then(|c| c.to_digit(10)) {
                let digit = u16::try_from(digit).map_err(|_| UrlError)?;
                ipv4_piece = match ipv4_piece {
                    None => Some(digit),
                    Some(0) => return Err(UrlError),
                    Some(piece) => Some(piece * 10 + digit),
                };
                if ipv4_piece.is_some_and(|piece| piece > 255) {
                    return Err(UrlError);
                }
                pointer += 1;
            }
            let piece = ipv4_piece.unwrap_or(0);
            address[piece_index] = address[piece_index] * 0x100 + piece;
            numbers_seen += 1;
            if numbers_seen == 2 || numbers_seen == 4 {
                piece_index += 1;
            }
        }
        if numbers_seen != 4 {
            return Err(UrlError);
        }
        Ok(piece_index)
    }

    /// Returns the text of an IPv6 address with its longest run of zero pieces compressed.
    fn serialize_ipv6(address: &[u16; 8]) -> String {
        let mut compress: Option<(usize, usize)> = None;
        let mut i = 0;
        while i < 8 {
            if address[i] == 0 {
                let start = i;
                while i < 8 && address[i] == 0 {
                    i += 1;
                }
                let length = i - start;
                if length > 1 && compress.is_none_or(|(_, best)| length > best) {
                    compress = Some((start, length));
                }
            } else {
                i += 1;
            }
        }
        let mut output = String::new();
        let mut ignore0 = false;
        for (index, piece) in address.iter().enumerate() {
            if ignore0 && *piece == 0 {
                continue;
            }
            ignore0 = false;
            if compress.is_some_and(|(start, _)| start == index) {
                output.push_str(if index == 0 { "::" } else { ":" });
                ignore0 = true;
                continue;
            }
            // A write into a string does not fail.
            let _ = write!(output, "{piece:x}");
            if index != 7 {
                output.push(':');
            }
        }
        output
    }
}

/// Domain to ASCII of the feature `node-server`, as ada 3.3.0 runs it for `new URL` of Node.
#[cfg(feature = "node-server")]
mod idna {
    include!("url_idna_tables.rs");

    use aosabi_json::sort::sort_by_key_stable;

    use super::UrlError;

    /// The status of a code point in the mapping of UTS 46, with its mapping when it is mapped.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Status {
        Valid,
        /// Mapped to a text of the pool.
        Mapped(&'static str),
        /// Mapped to one code point.
        MappedTo(char),
        Ignored,
        Disallowed,
    }

    /// The ranges between two anchors of the mapping, the most distances a walk reads.
    const BLOCK: usize = 256;

    /// Returns the index of the range of the mapping that holds `cp` and the first code point of
    /// that range.
    ///
    /// The search finds the last anchor at or below `cp`, and the walk adds
    /// the distances after it while the next range still starts at or below
    /// `cp`, at most the ranges up to the next anchor.
    fn range_of(cp: u32) -> (usize, u32) {
        let block = IDNA_ANCHORS
            .partition_point(|&anchor| anchor <= cp)
            .saturating_sub(1);
        let mut index = block * BLOCK;
        let mut start = IDNA_ANCHORS[block];
        let end = IDNA_DELTAS.len().min(index + BLOCK);
        while index + 1 < end {
            let next = start + u32::from(IDNA_DELTAS[index + 1]);
            if next > cp {
                break;
            }
            start = next;
            index += 1;
        }
        (index, start)
    }

    /// Returns the status of `c` with its mapping when it is mapped.
    ///
    /// The rule of the range decides: a range of rule 2 maps each code point
    /// to the code point at the distance its first code point maps over, and
    /// a range of rule 5 maps a code point at an even distance from its first
    /// code point to the next code point and keeps one at an odd distance.
    fn status(c: char) -> Status {
        let cp = u32::from(c);
        let (index, start) = range_of(cp);
        let value = IDNA_VALUES[index];
        match value >> 28 {
            1 => {
                let offset = ((value >> 8) & 0xF_FFFF) as usize;
                let length = (value & 0xFF) as usize;
                Status::Mapped(IDNA_POOL.get(offset..offset + length).unwrap_or(""))
            }
            2 => char::from_u32((value & 0x1F_FFFF) + (cp - start))
                .map_or(Status::Disallowed, Status::MappedTo),
            3 => Status::Ignored,
            5 if (cp - start).is_multiple_of(2) => {
                char::from_u32(cp + 1).map_or(Status::Disallowed, Status::MappedTo)
            }
            0 | 5 => Status::Valid,
            _ => Status::Disallowed,
        }
    }

    /// Returns the low 16 bits of `cp`, the form the tables of a plane keep.
    fn low_bits(cp: u32) -> u16 {
        u16::try_from(cp & 0xFFFF).unwrap_or(0)
    }

    /// Returns the entries of the plane of `cp` in a table whose planes start at the indexes of
    /// `planes`.
    fn plane_range(planes: &[u16], cp: u32) -> Option<core::ops::Range<usize>> {
        let plane = (cp >> 16) as usize;
        let first = usize::from(*planes.get(plane)?);
        let after = usize::from(*planes.get(plane + 1)?);
        Some(first..after)
    }

    /// Returns the count of the entries of `table`, each `stride` words long, whose first word is at most `cp`.
    fn entries_up_to(table: &[u32], stride: usize, cp: u32) -> usize {
        let (mut low, mut high) = (0, table.len() / stride);
        while low < high {
            let middle = low + (high - low) / 2;
            if table[stride * middle] <= cp {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        low
    }

    /// Returns the value of `cp` in a table of ranges of three words, the first, the last and the value.
    fn range_value(table: &[u32], cp: u32) -> u32 {
        match entries_up_to(table, 3, cp).checked_sub(1) {
            Some(i) if cp <= table[3 * i + 1] => table[3 * i + 2],
            _ => 0,
        }
    }

    /// Returns the canonical combining class of `c`.
    fn combining_class(c: char) -> u32 {
        range_value(&CCC, u32::from(c))
    }

    /// Returns whether a label must not start with `c`, a mark of ada's list.
    fn is_mark(c: char) -> bool {
        let cp = u32::from(c);
        entries_up_to(&MARKS, 2, cp)
            .checked_sub(1)
            .is_some_and(|i| cp <= MARKS[2 * i + 1])
    }

    /// Returns whether `c` is a virama of ada's list.
    fn is_virama(c: char) -> bool {
        VIRAMAS.binary_search(&u32::from(c)).is_ok()
    }

    /// The bidirectional classes of the table `BIDI` that the rule reads, 0 for none of them.
    const L: u32 = 1;
    const R: u32 = 2;
    const AL: u32 = 3;
    const AN: u32 = 4;
    const EN: u32 = 5;
    const ES: u32 = 6;
    const CS: u32 = 7;
    const ET: u32 = 8;
    const ON: u32 = 9;
    const BN: u32 = 10;
    const NSM: u32 = 11;

    /// Returns the bidirectional class of `c`.
    fn bidi_class(c: char) -> u32 {
        range_value(&BIDI, u32::from(c))
    }

    /// Returns the joining type of `c`, 1 L, 2 D, 3 R and 0 for none.
    fn joining_type(c: char) -> u32 {
        range_value(&JOINING, u32::from(c))
    }

    /// The first Hangul syllable, the first leading, vowel and trailing jamo, and the counts.
    const S_BASE: u32 = 0xAC00;
    const L_BASE: u32 = 0x1100;
    const V_BASE: u32 = 0x1161;
    const T_BASE: u32 = 0x11A7;
    const L_COUNT: u32 = 19;
    const V_COUNT: u32 = 21;
    const T_COUNT: u32 = 28;
    const N_COUNT: u32 = V_COUNT * T_COUNT;
    const S_COUNT: u32 = L_COUNT * N_COUNT;

    /// Appends the full canonical decomposition of `c` to `out`.
    fn decompose(c: char, out: &mut Vec<char>) {
        let cp = u32::from(c);
        if (S_BASE..S_BASE + S_COUNT).contains(&cp) {
            let index = cp - S_BASE;
            let l = L_BASE + index / N_COUNT;
            let v = V_BASE + (index % N_COUNT) / T_COUNT;
            let t = T_BASE + index % T_COUNT;
            out.extend([l, v].into_iter().filter_map(char::from_u32));
            if t != T_BASE {
                out.extend(char::from_u32(t));
            }
            return;
        }
        let found = plane_range(&DECOMPOSITION_PLANES, cp).and_then(|keys| {
            let at = DECOMPOSITION_KEYS[keys.clone()]
                .binary_search(&low_bits(cp))
                .ok()?;
            Some(keys.start + at)
        });
        match found {
            Some(index) => {
                let offset = usize::from(DECOMPOSITION_OFFSETS[index]);
                let after = usize::from(DECOMPOSITION_OFFSETS[index + 1]);
                out.extend(DECOMPOSITION_POOL.get(offset..after).unwrap_or("").chars());
            }
            None => out.push(c),
        }
    }

    /// Returns the primary composite of `first` and `second`.
    fn compose_pair(first: char, second: char) -> Option<char> {
        let (a, b) = (u32::from(first), u32::from(second));
        if (L_BASE..L_BASE + L_COUNT).contains(&a) && (V_BASE..V_BASE + V_COUNT).contains(&b) {
            return char::from_u32(S_BASE + ((a - L_BASE) * V_COUNT + (b - V_BASE)) * T_COUNT);
        }
        if (S_BASE..S_BASE + S_COUNT).contains(&a)
            && (a - S_BASE).is_multiple_of(T_COUNT)
            && (T_BASE + 1..T_BASE + T_COUNT).contains(&b)
        {
            return char::from_u32(a + (b - T_BASE));
        }
        if a >> 16 != b >> 16 {
            return None;
        }
        let pair = (low_bits(a), low_bits(b));
        let range = plane_range(&COMPOSITION_PLANES, a)?;
        let (mut low, mut high) = (range.start, range.end);
        while low < high {
            let middle = low + (high - low) / 2;
            match (COMPOSITIONS[3 * middle], COMPOSITIONS[3 * middle + 1]).cmp(&pair) {
                core::cmp::Ordering::Less => low = middle + 1,
                core::cmp::Ordering::Greater => high = middle,
                core::cmp::Ordering::Equal => {
                    return char::from_u32((a & !0xFFFF) | u32::from(COMPOSITIONS[3 * middle + 2]));
                }
            }
        }
        None
    }

    /// Returns `text` in normalization form C.
    fn nfc(text: &[char]) -> Vec<char> {
        let mut decomposed: Vec<char> = Vec::with_capacity(text.len());
        for &c in text {
            decompose(c, &mut decomposed);
        }
        let mut i = 0;
        while i < decomposed.len() {
            let mut j = i;
            while j < decomposed.len() && combining_class(decomposed[j]) != 0 {
                j += 1;
            }
            sort_by_key_stable(&mut decomposed[i..j], |&c| combining_class(c));
            i = j + 1;
        }
        let mut out: Vec<char> = Vec::with_capacity(decomposed.len());
        let mut starter: Option<usize> = None;
        let mut last_class: Option<u32> = None;
        for c in decomposed {
            let class = combining_class(c);
            let blocked = last_class.is_some_and(|last| last == 0 || last >= class);
            if let Some(at) = starter
                && !blocked
                && let Some(composite) = compose_pair(out[at], c)
            {
                out[at] = composite;
                continue;
            }
            if class == 0 {
                starter = Some(out.len());
                last_class = None;
            } else {
                last_class = Some(class);
            }
            out.push(c);
        }
        out
    }

    /// The parameters of Punycode, RFC 3492 section 5.
    const BASE: u32 = 36;
    const T_MIN: u32 = 1;
    const T_MAX: u32 = 26;
    const INITIAL_BIAS: u32 = 72;
    const INITIAL_N: u32 = 128;

    /// The largest value ada's Punycode holds in a signed 32 bit integer.
    const MAX_INT: u32 = 0x7FFF_FFFF;

    /// Returns the threshold of the digit at `k` for `bias`, RFC 3492 section 6.
    fn threshold(k: u32, bias: u32) -> u32 {
        if k <= bias {
            T_MIN
        } else if k >= bias + T_MAX {
            T_MAX
        } else {
            k - bias
        }
    }

    /// Returns the Punycode encoding of `input`, RFC 3492 section 6.3.
    // The names of the bindings are the ones of RFC 3492 section 6.
    #[expect(clippy::many_single_char_names)]
    fn punycode_encode(input: &[char]) -> Option<String> {
        let mut output: String = input.iter().filter(|c| c.is_ascii()).collect();
        let basic = u32::try_from(output.len()).ok()?;
        let mut handled = basic;
        if basic > 0 {
            output.push('-');
        }
        let mut n = INITIAL_N;
        let mut delta: u32 = 0;
        let mut bias = INITIAL_BIAS;
        let total = u32::try_from(input.len()).ok()?;
        while handled < total {
            let m = input
                .iter()
                .map(|&c| u32::from(c))
                .filter(|&cp| cp >= n)
                .min()?;
            if m - n > (MAX_INT - delta) / (handled + 1) {
                return None;
            }
            delta += (m - n) * (handled + 1);
            n = m;
            for &c in input {
                let cp = u32::from(c);
                if cp < n {
                    if delta == MAX_INT {
                        return None;
                    }
                    delta += 1;
                }
                if cp == n {
                    let mut q = delta;
                    let mut k = BASE;
                    loop {
                        let t = threshold(k, bias);
                        if q < t {
                            break;
                        }
                        output.push(digit(t + (q - t) % (BASE - t)));
                        q = (q - t) / (BASE - t);
                        k += BASE;
                    }
                    output.push(digit(q));
                    bias = adapt(delta, handled + 1, handled == basic);
                    delta = 0;
                    handled += 1;
                }
            }
            delta += 1;
            n += 1;
        }
        Some(output)
    }

    /// Returns the Punycode decoding of `input`, RFC 3492 section 6.2.
    ///
    /// A code point that is no scalar value is refused here, where ada keeps it and its mapping
    /// refuses it as disallowed.
    // The names of the bindings are the ones of RFC 3492 section 6.
    #[expect(clippy::many_single_char_names)]
    fn punycode_decode(input: &str) -> Option<Vec<char>> {
        if input.starts_with("xn--") {
            return None;
        }
        let (basic, extended) = match input.rfind('-') {
            Some(at) => (&input[..at], &input[at + 1..]),
            None => ("", input),
        };
        if !basic.is_ascii() {
            return None;
        }
        let mut output: Vec<char> = basic.chars().collect();
        let mut n = INITIAL_N;
        let mut i: u32 = 0;
        let mut bias = INITIAL_BIAS;
        let mut chars = extended.chars().peekable();
        while chars.peek().is_some() {
            let old_i = i;
            let mut w: u32 = 1;
            let mut k = BASE;
            loop {
                let d = digit_value(chars.next()?)?;
                if d > (MAX_INT - i) / w {
                    return None;
                }
                i += d * w;
                let t = threshold(k, bias);
                if d < t {
                    break;
                }
                if w > MAX_INT / (BASE - t) {
                    return None;
                }
                w *= BASE - t;
                k += BASE;
            }
            let length = u32::try_from(output.len()).ok()? + 1;
            bias = adapt(i - old_i, length, old_i == 0);
            if i / length > MAX_INT - n {
                return None;
            }
            n += i / length;
            i %= length;
            output.insert(usize::try_from(i).ok()?, char::from_u32(n)?);
            i += 1;
        }
        Some(output)
    }

    /// Returns the Punycode digit of `value`, `a` to `z` for 0 to 25 and `0` to `9` for 26 to 35.
    fn digit(value: u32) -> char {
        let code = if value < 26 {
            u32::from('a') + value
        } else {
            u32::from('0') + value - 26
        };
        char::from_u32(code).unwrap_or('a')
    }

    /// Returns the value of the Punycode digit `c`, which ada reads in lower case alone.
    fn digit_value(c: char) -> Option<u32> {
        match c {
            'a'..='z' => Some(u32::from(c) - u32::from('a')),
            '0'..='9' => Some(u32::from(c) - u32::from('0') + 26),
            _ => None,
        }
    }

    /// Returns the bias after a delta, RFC 3492 section 6.1.
    fn adapt(delta: u32, points: u32, first: bool) -> u32 {
        let mut delta = if first { delta / 700 } else { delta / 2 };
        delta += delta / points;
        let mut k = 0;
        while delta > ((BASE - T_MIN) * T_MAX) / 2 {
            delta /= BASE - T_MIN;
            k += BASE;
        }
        k + (BASE * delta) / (delta + 38)
    }

    /// Returns whether a label passes the validity criteria as ada checks them.
    ///
    /// A label must not start with a mark. The first joiner decides the label: after a virama
    /// the label is valid, a zero width joiner elsewhere refuses it, and a zero width non-joiner
    /// inside the label passes when a code point of joining type L or D stands anywhere before
    /// it and one of type R or D anywhere after it. A label of nonspacing marks alone is
    /// refused. The rules of RFC 5893 section 2 run on a label that holds a code point of the
    /// classes R, AL or AN, and on no other label of the domain. A label that starts with L is
    /// read as a left to right label whose code points before the last one that is no NSM are of
    /// the classes L, EN, ES, CS, ET, ON, BN or NSM. Any other is read as a right to left label
    /// with the classes R, AL, AN, EN, ES, CS, ET, ON, BN or NSM up to that code point, which is
    /// R, AL, EN or AN, and not both EN and AN.
    fn valid_label(label: &[char]) -> bool {
        let Some(&first) = label.first() else {
            return true;
        };
        if is_mark(first) {
            return false;
        }
        for (i, &c) in label.iter().enumerate() {
            if c != '\u{200C}' && c != '\u{200D}' {
                continue;
            }
            if i > 0 && is_virama(label[i - 1]) {
                return true;
            }
            if c == '\u{200D}' || i + 1 >= label.len() {
                return false;
            }
            let left = label[..i].iter().any(|&x| matches!(joining_type(x), 1 | 2));
            let right = label[i + 1..]
                .iter()
                .any(|&x| matches!(joining_type(x), 2 | 3));
            return left && right;
        }
        let Some(last) = label.iter().rposition(|&c| bidi_class(c) != NSM) else {
            return false;
        };
        if !label.iter().any(|&c| matches!(bidi_class(c), R | AL | AN)) {
            return true;
        }
        if bidi_class(first) == L {
            return label[..last]
                .iter()
                .all(|&c| matches!(bidi_class(c), L | EN | ES | CS | ET | ON | BN | NSM));
        }
        let classes: Vec<u32> = label[..=last].iter().map(|&c| bidi_class(c)).collect();
        let allowed = classes
            .iter()
            .all(|&class| matches!(class, R | AL | AN | EN | ES | CS | ET | ON | BN | NSM));
        let end = matches!(classes[last], R | AL | EN | AN);
        allowed && end && !(classes.contains(&EN) && classes.contains(&AN))
    }

    /// Returns whether the decoding of a label of Punycode passes as ada checks it.
    ///
    /// The decoding holds a code point outside ASCII, each of its code points
    /// is valid, so the mapping leaves it as it is, it is in normalization form
    /// C, and it is a valid label.
    fn valid_decoding(decoded: &[char]) -> bool {
        !decoded.iter().all(char::is_ascii)
            && decoded.iter().all(|&c| status(c) == Status::Valid)
            && nfc(decoded) == decoded
            && valid_label(decoded)
    }

    /// Returns `domain` in ASCII, domain to ASCII of the URL Standard with `beStrict` false.
    ///
    /// The result is refused when it is empty or holds a forbidden domain code point.
    pub(super) fn domain_to_ascii(domain: &str) -> Result<String, UrlError> {
        let result = to_ascii(domain).ok_or(UrlError)?;
        if result.is_empty() || result.chars().any(is_forbidden_domain_code_point) {
            return Err(UrlError);
        }
        Ok(result)
    }

    /// Returns whether `c` is a forbidden domain code point of the URL Standard.
    fn is_forbidden_domain_code_point(c: char) -> bool {
        c <= '\u{1F}'
            || c == '\u{7F}'
            || matches!(
                c,
                ' ' | '#' | '%' | '/' | ':' | '<' | '>' | '?' | '@' | '[' | '\\' | ']' | '^' | '|'
            )
    }

    /// Returns `ToASCII` of UTS 46 of `domain` as ada runs it, `None` on an error.
    ///
    /// An ASCII domain is lower cased and a label that starts with `xn--` is checked by its
    /// decoding. Any other domain is mapped as a whole, where a disallowed code point refuses
    /// it, and normalized to form C, and then each label that starts with `xn--` is checked by
    /// its decoding, an ASCII label is kept, and any other is checked and encoded in Punycode.
    fn to_ascii(domain: &str) -> Option<String> {
        if domain.is_ascii() {
            let lower = domain.to_ascii_lowercase();
            for label in lower.split('.') {
                if let Some(rest) = label.strip_prefix("xn--")
                    && !valid_decoding(&punycode_decode(rest)?)
                {
                    return None;
                }
            }
            return Some(lower);
        }
        let mut mapped: Vec<char> = Vec::with_capacity(domain.len());
        for c in domain.chars() {
            match status(c) {
                Status::Valid => mapped.push(c),
                Status::Mapped(mapping) => mapped.extend(mapping.chars()),
                Status::MappedTo(to) => mapped.push(to),
                Status::Ignored => {}
                Status::Disallowed => return None,
            }
        }
        let normalized = nfc(&mapped);
        let mut ascii: Vec<String> = Vec::new();
        for label in normalized.split(|&c| c == '.') {
            if label.starts_with(&['x', 'n', '-', '-']) {
                let text: String = label.iter().collect();
                if !text.is_ascii() || !valid_decoding(&punycode_decode(&text[4..])?) {
                    return None;
                }
                ascii.push(text);
            } else if label.iter().all(char::is_ascii) {
                ascii.push(label.iter().collect());
            } else if valid_label(label) {
                ascii.push(format!("xn--{}", punycode_encode(label)?));
            } else {
                return None;
            }
        }
        Some(ascii.join("."))
    }
}
