// SPDX-License-Identifier: MIT
/*
 * aosabi-sentry: the DSN of a Sentry project and the endpoint it names.
 *
 * No analogue file on the reference side.
 */

//! `Dsn` and the envelope endpoint with its authentication in the query.
//!
//! A DSN reads
//! `{protocol}://{public_key}[:{pass}]@{host}[:{port}]/[{path}/]{project_id}`.
//! [`Dsn::parse`] is the regular expression of the SDK,
//! `^(?:(\w+):)\/\/(?:(\w+)(?::(\w+)?)?@)([\w.-]+)(?::(\d+))?\/(.+)`, written
//! as a scan, followed by the split of the path from the project and the checks
//! of `validateDsn`. The SDK sends its key in the query of the endpoint and not
//! in an `X-Sentry-Auth` header, which spares a browser the preflight of a
//! header of its own, so [`Dsn::envelope_url`] writes that query.

use core::fmt;

/// The version of the ingest protocol, `SENTRY_API_VERSION`.
const SENTRY_API_VERSION: &str = "7";

/// The parts of a DSN, `DsnComponents`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dsn {
    /// The scheme, `http` or `https`.
    pub protocol: String,
    /// The public key, the user of the DSN.
    pub public_key: String,
    /// The secret key, empty when the DSN names none.
    pub pass: String,
    /// The host.
    pub host: String,
    /// The port, empty when the DSN names none.
    pub port: String,
    /// The path before the project, empty when the DSN names none.
    pub path: String,
    /// The id of the project, its leading digits.
    pub project_id: String,
}

/// Why a DSN was refused, the two refusals of `makeDsn`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DsnError {
    /// The text does not match the expression of the SDK, which the SDK
    /// prints as `Invalid Sentry Dsn: <text>`.
    Syntax,
    /// A part is missing or holds a value `validateDsn` refuses.
    Invalid,
}

impl fmt::Display for DsnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Syntax => f.write_str("the DSN does not match the DSN form"),
            Self::Invalid => f.write_str("the DSN names no valid protocol, key, host or project"),
        }
    }
}

impl core::error::Error for DsnError {}

impl Dsn {
    /// Parses `text` as `makeDsn(text)` does.
    ///
    /// # Errors
    ///
    /// Returns [`DsnError::Syntax`] for a text the expression does not match
    /// and [`DsnError::Invalid`] for a DSN whose project is no number or whose
    /// protocol is neither `http` nor `https`.
    pub fn parse(text: &str) -> Result<Self, DsnError> {
        let dsn = from_string(text).ok_or(DsnError::Syntax)?;
        if dsn.project_id.is_empty()
            || !dsn.project_id.bytes().all(|b| b.is_ascii_digit())
            || !matches!(dsn.protocol.as_str(), "http" | "https")
        {
            return Err(DsnError::Invalid);
        }
        Ok(dsn)
    }

    /// Returns the envelope endpoint with the key of the project in its query.
    ///
    /// The base endpoint, the project, `/envelope/`, and the query
    /// `sentry_key`, `sentry_version` and `sentry_client` with each value
    /// percent encoded as `encodeURIComponent` encodes it.
    #[must_use]
    pub fn envelope_url(&self, client: &str) -> String {
        let mut url = String::with_capacity(64 + self.host.len() + self.public_key.len());
        url.push_str(&self.protocol);
        url.push_str("://");
        url.push_str(&self.host);
        if !self.port.is_empty() {
            url.push(':');
            url.push_str(&self.port);
        }
        if !self.path.is_empty() {
            url.push('/');
            url.push_str(&self.path);
        }
        url.push_str("/api/");
        url.push_str(&self.project_id);
        url.push_str("/envelope/?sentry_key=");
        encode_uri_component(&mut url, &self.public_key);
        url.push_str("&sentry_version=");
        url.push_str(SENTRY_API_VERSION);
        url.push_str("&sentry_client=");
        encode_uri_component(&mut url, client);
        url
    }
}

/// Returns the parts of `text`, `dsnFromString(text)` without its print.
///
/// The scan reads the groups of the expression in turn. Each group is a
/// maximal run of the characters it takes, and the character after it is the
/// one the expression names next, so the expression never backtracks into a
/// shorter run and the scan answers what the expression matches.
fn from_string(text: &str) -> Option<Dsn> {
    let (protocol, rest) = take_word(text);
    let rest = rest.strip_prefix(':')?.strip_prefix("//")?;
    let (public_key, mut rest) = take_word(rest);
    let mut pass = "";
    if let Some(after) = rest.strip_prefix(':') {
        (pass, rest) = take_while(after, is_word);
    }
    let rest = rest.strip_prefix('@')?;
    let (host, mut rest) = take_while(rest, |b| is_word(b) || b == b'.' || b == b'-');
    if protocol.is_empty() || public_key.is_empty() || host.is_empty() {
        return None;
    }
    let mut port = "";
    if let Some(after) = rest.strip_prefix(':') {
        let (digits, after) = take_while(after, |b| b.is_ascii_digit());
        if digits.is_empty() {
            return None;
        }
        (port, rest) = (digits, after);
    }
    let rest = rest.strip_prefix('/')?;
    // `.+` stops at a line terminator of ECMA-262, and the match needs one
    // character at least.
    let last_path = rest
        .split(['\n', '\r', '\u{2028}', '\u{2029}'])
        .next()
        .unwrap_or_default();
    if last_path.is_empty() {
        return None;
    }
    let (path, project) = match last_path.rsplit_once('/') {
        Some((path, project)) => (path, project),
        None => ("", last_path),
    };
    let digits = project.bytes().take_while(u8::is_ascii_digit).count();
    let project_id = if digits > 0 {
        &project[..digits]
    } else {
        project
    };
    Some(Dsn {
        protocol: String::from(protocol),
        public_key: String::from(public_key),
        pass: String::from(pass),
        host: String::from(host),
        port: String::from(port),
        path: String::from(path),
        project_id: String::from(project_id),
    })
}

/// Returns whether `byte` is a character of `\w` without the flag `u`.
const fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Splits `text` after its leading run of `\w`.
fn take_word(text: &str) -> (&str, &str) {
    take_while(text, is_word)
}

/// Splits `text` after its leading run of ASCII bytes that `keep` accepts.
fn take_while(text: &str, keep: impl Fn(u8) -> bool) -> (&str, &str) {
    let end = text.bytes().position(|b| !keep(b)).unwrap_or(text.len());
    text.split_at(end)
}

/// Appends `text` to `out` as `encodeURIComponent(text)` writes it.
///
/// The characters `A-Z a-z 0-9 - _ . ! ~ * ' ( )` stay, and every other byte of
/// the UTF-8 form becomes `%XX` with upper case digits.
fn encode_uri_component(out: &mut String, text: &str) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric()
            || matches!(
                byte,
                b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')'
            )
        {
            out.push(char::from(byte));
        } else {
            out.push('%');
            out.push(char::from(HEX[usize::from(byte >> 4)]));
            out.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
    }
}
