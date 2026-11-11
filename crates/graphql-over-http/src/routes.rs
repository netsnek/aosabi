// SPDX-License-Identifier: MIT
/*
 * graphql-over-http: the path the function answers, its endpoint.
 *
 * The function matches_endpoint holds the test of the path of the endpoint.
 *
 * Derived from graphql-yoga 5.18.0, cjs/plugins/use-unhandled-route.js, MIT.
 * Derived from graphql-yoga 5.18.0, cjs/plugins/use-health-check.js, MIT.
 */

//! The paths of the function.
//!
//! A request passes as one for the endpoint when its path is the endpoint,
//! where Yoga's five endpoint tests pass a request whose whole URL ends in the
//! endpoint or in the endpoint and a slash as well, so a request to
//! `/a/b/graphql` or one whose query ends in `/graphql` is no request for the
//! endpoint, a named divergence, and neither is a request to the endpoint and a
//! slash, a named divergence. The function answers any other request with an
//! empty 404, the unhandled route, and the app passes `landingPage: false`, so
//! the landing page of Yoga never renders and the 404 falls through to the not
//! found handler of the app.
//!
//! The URL pattern of an endpoint that holds no pattern syntax matches the path
//! equal to it, which the test of the path covers already, and the endpoint of
//! an app is `/graphql`, so [`matches_endpoint`] reads the pattern as that
//! text.
//!
//! A plain URL, the form of the URL of a request the server built for a host
//! in lower case, is tested by its text through [`is_plain_endpoint`], which
//! reads its path as the URL parser does, so the endpoint costs no parse of
//! the URL, and any other URL goes to the parser.

/// Returns whether `pathname` is `endpoint`.
#[must_use]
pub fn matches_endpoint(pathname: &str, endpoint: &str) -> bool {
    pathname == endpoint
}

/// Returns whether `href` is a plain URL whose path is `endpoint`, a test that needs no parse.
///
/// A plain URL starts with `http://` or `https://` and holds a host, a domain
/// of lower case ASCII letters, digits, dots and hyphens whose last label
/// starts with a letter and none of whose labels starts with `xn--`, or four
/// decimal parts of at most 255 with no leading zero, then an optional port of
/// one to five digits of at most 65535 and then the path, which runs from the
/// first `/` to the first `?` or `#`. The URL parser accepts every such URL and
/// reads its path as the text: domain to ASCII lower cases an ASCII domain and
/// checks only a label that starts with `xn--`, a domain whose last label
/// starts with a letter ends in no number, the IPv4 parser reads four such
/// parts, and a path of ASCII letters, digits, `/`, `-`, `_` and `~` holds no
/// dot segment, no backslash and no code point the parser encodes.
#[must_use]
pub(crate) fn is_plain_endpoint(href: &str, endpoint: &str) -> bool {
    let Some(rest) = href
        .strip_prefix("http://")
        .or_else(|| href.strip_prefix("https://"))
    else {
        return false;
    };
    let bytes = rest.as_bytes();
    let Some(slash) = bytes.iter().position(|&byte| byte == b'/') else {
        return false;
    };
    let end = bytes[slash..]
        .iter()
        .position(|&byte| byte == b'?' || byte == b'#')
        .map_or(bytes.len(), |at| slash + at);
    if &rest[slash..end] != endpoint || !is_plain_path(endpoint.as_bytes()) {
        return false;
    }
    let authority = &bytes[..slash];
    let (host, port) = match authority.iter().position(|&byte| byte == b':') {
        Some(colon) => (&authority[..colon], Some(&authority[colon + 1..])),
        None => (authority, None),
    };
    (is_plain_domain(host) || is_plain_ipv4(host)) && port.is_none_or(is_plain_port)
}

/// Returns whether `path` starts with `/` and holds only ASCII letters, digits, `/`, `-`, `_` and `~`.
fn is_plain_path(path: &[u8]) -> bool {
    path.first() == Some(&b'/')
        && path
            .iter()
            .all(|&byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'_' | b'~'))
}

/// Returns whether `host` is a domain of lower case ASCII letters, digits, dots and hyphens whose last
/// label starts with a letter and none of whose labels starts with `xn--`.
fn is_plain_domain(host: &[u8]) -> bool {
    let last = host.rsplit(|&byte| byte == b'.').next().unwrap_or_default();
    host.iter().all(|&byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-')
    }) && last.first().is_some_and(u8::is_ascii_lowercase)
        && !host
            .split(|&byte| byte == b'.')
            .any(|label| label.starts_with(b"xn--"))
}

/// Returns whether `host` is four decimal parts of at most 255 with no leading zero, joined by dots.
fn is_plain_ipv4(host: &[u8]) -> bool {
    let mut parts = 0;
    for part in host.split(|&byte| byte == b'.') {
        parts += 1;
        let decimal = (1..=3).contains(&part.len())
            && part.iter().all(u8::is_ascii_digit)
            && (part.len() == 1 || part[0] != b'0')
            && number(part) <= 255;
        if !decimal {
            return false;
        }
    }
    parts == 4
}

/// Returns whether `port` is one to five ASCII digits of at most 65535.
fn is_plain_port(port: &[u8]) -> bool {
    (1..=5).contains(&port.len()) && port.iter().all(u8::is_ascii_digit) && number(port) <= 65_535
}

/// Returns the number of at most five ASCII digits.
fn number(digits: &[u8]) -> u32 {
    digits
        .iter()
        .fold(0, |number, &digit| number * 10 + u32::from(digit - b'0'))
}
