// SPDX-License-Identifier: MIT
/*
 * aosabi-http: the path and the query of a request URL as hono reads them.
 *
 * Derived from hono 4.11.9, dist/utils/url.js, MIT.
 */

//! The path, the query and the route patterns of hono.
//!
//! Hono reads the path of a request from its URL string without a URL parser:
//! the path starts at the first `/` after the scheme and ends before the query
//! or the fragment, and it is decoded with `decodeURI` only when it holds a
//! `%`, with `%25` kept so a decoded percent never forms a new escape. The path
//! is strict, so `/a/` and `/a` are two paths. A query parameter is read from
//! the URL string the same way, with `+` read as a space and
//! `decodeURIComponent` over a value that holds a `%`. The two encoders of
//! ECMA-262 stand beside the two decoders, for the location of a redirect and
//! the value of a cookie.

/// Splits `path` on `/`, dropping the empty first part of a path that starts with one.
#[must_use]
pub fn split_path(path: &str) -> Vec<String> {
    let mut paths: Vec<String> = path.split('/').map(String::from).collect();
    if paths.first().is_some_and(String::is_empty) {
        paths.remove(0);
    }
    paths
}

/// Splits a route path on `/`, keeping every group `{...}` whole.
///
/// A group is replaced by a mark before the split and put back after it, so a
/// `/` inside a group does not split.
#[must_use]
pub fn split_routing_path(route_path: &str) -> Vec<String> {
    let mut groups: Vec<(String, String)> = Vec::new();
    let mut path = String::with_capacity(route_path.len());
    let chars: Vec<char> = route_path.chars().collect();
    let mut i = 0;
    let mut offset = 0;
    while i < chars.len() {
        if chars[i] == '{'
            && let Some(len) = chars[i + 1..].iter().position(|&c| c == '}')
            && len > 0
        {
            let group: String = chars[i..=i + 1 + len].iter().collect();
            let mark = format!("@{offset}");
            path.push_str(&mark);
            groups.push((mark, group));
            offset += len + 2;
            i += len + 2;
            continue;
        }
        path.push(chars[i]);
        offset += 1;
        i += 1;
    }
    let mut paths = split_path(&path);
    for (mark, group) in groups.iter().rev() {
        if let Some(part) = paths
            .iter_mut()
            .rev()
            .find(|part| part.contains(mark.as_str()))
        {
            *part = part.replacen(mark.as_str(), group, 1);
        }
    }
    paths
}

/// The pattern of one label of a route path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pattern {
    /// The wildcard `*`.
    Wildcard,
    /// A parameter `:name`, with the key its node is stored under, the name,
    /// and the constraint `{...}` when the label carries one.
    Param {
        /// The key of the child node.
        key: String,
        /// The name of the parameter.
        name: String,
        /// The source of the constraint, which the trie router does not
        /// evaluate.
        constraint: Option<String>,
    },
}

/// Returns the pattern of the label `label` followed by `next`, or `None` for a static label.
///
/// A parameter with a constraint `{...}` is keyed by the label and the next
/// label when a static label follows it, as the reference keys its cache.
#[must_use]
pub fn get_pattern(label: &str, next: Option<&str>) -> Option<Pattern> {
    if label == "*" {
        return Some(Pattern::Wildcard);
    }
    let rest = label.strip_prefix(':')?;
    let (name, constraint) = match rest.find('{') {
        Some(open) if rest.ends_with('}') && open > 0 && rest.len() > open + 2 => {
            (&rest[..open], Some(&rest[open + 1..rest.len() - 1]))
        }
        _ => (rest, None),
    };
    if name.is_empty() || name.contains(['{', '}']) {
        return None;
    }
    let key = match (constraint, next) {
        (Some(_), Some(next)) if !next.starts_with([':', '*']) => format!("{label}#{next}"),
        _ => String::from(label),
    };
    Some(Pattern::Param {
        key,
        name: String::from(name),
        constraint: constraint.map(String::from),
    })
}

/// Returns `decoder(value)`, or `value` with every run of valid escapes decoded where the whole fails.
pub fn try_decode(value: &str, decoder: fn(&str) -> Option<String>) -> String {
    if let Some(decoded) = decoder(value) {
        return decoded;
    }
    let bytes = value.as_bytes();
    let mut out = String::with_capacity(value.len());
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        while i + 2 < bytes.len()
            && bytes[i] == b'%'
            && is_hex(bytes[i + 1])
            && is_hex(bytes[i + 2])
        {
            i += 3;
        }
        if i > start {
            let run = &value[start..i];
            out.push_str(&decoder(run).unwrap_or_else(|| String::from(run)));
            continue;
        }
        let next = value[i..].chars().next().map_or(1, char::len_utf8);
        out.push_str(&value[i..i + next]);
        i += next;
    }
    out
}

/// Returns whether `byte` is a hexadecimal digit.
fn is_hex(byte: u8) -> bool {
    byte.is_ascii_hexdigit()
}

/// Returns `decodeURI(value)`, or `None` where it throws a `URIError`.
///
/// Translation of the global `decodeURI` of ECMA-262, which keeps the escape of
/// a character of the reserved set `;/?:@&=+$,#`.
#[must_use]
pub fn decode_uri(value: &str) -> Option<String> {
    decode(value, ";/?:@&=+$,#")
}

/// Returns `decodeURIComponent(value)`, or `None` where it throws a `URIError`.
///
/// Translation of the global `decodeURIComponent` of ECMA-262.
#[must_use]
pub fn decode_uri_component(value: &str) -> Option<String> {
    decode(value, "")
}

/// Returns the ECMA-262 `Decode` of `value` with the reserved set `reserved`.
///
/// An escape sequence must form one UTF-8 character, whose escapes are kept
/// as they are when it is one character of `reserved`, and a malformed
/// escape or sequence is `None`, the `URIError` of the reference.
fn decode(value: &str, reserved: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut out = String::with_capacity(value.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'%' {
            let next = value[i..].chars().next().map_or(1, char::len_utf8);
            out.push_str(&value[i..i + next]);
            i += next;
            continue;
        }
        let first = hex_byte(bytes, i)?;
        let len = match first {
            0x00..=0x7F => 1,
            0xC0..=0xDF => 2,
            0xE0..=0xEF => 3,
            0xF0..=0xF7 => 4,
            _ => return None,
        };
        let mut sequence = vec![first];
        for k in 1..len {
            let byte = hex_byte(bytes, i + 3 * k)?;
            if byte & 0xC0 != 0x80 {
                return None;
            }
            sequence.push(byte);
        }
        let decoded = core::str::from_utf8(&sequence).ok()?;
        if len == 1 && reserved.contains(decoded) {
            out.push_str(&value[i..i + 3]);
        } else {
            out.push_str(decoded);
        }
        i += 3 * len;
    }
    Some(out)
}

/// Returns the byte of the escape `%XX` at `at`, or `None`.
fn hex_byte(bytes: &[u8], at: usize) -> Option<u8> {
    if bytes.get(at) != Some(&b'%') {
        return None;
    }
    let high = char::from(*bytes.get(at + 1)?).to_digit(16)?;
    let low = char::from(*bytes.get(at + 2)?).to_digit(16)?;
    u8::try_from(high * 16 + low).ok()
}

/// Returns `encodeURI(value)`.
///
/// Translation of the global `encodeURI` of ECMA-262, which keeps the
/// characters of the reserved set `;/?:@&=+$,` and `#` beside those of
/// `encodeURIComponent`. A Rust string holds no lone surrogate, so the
/// `URIError` of the reference has no case here.
#[must_use]
pub fn encode_uri(value: &str) -> String {
    encode(value, b";/?:@&=+$,#")
}

/// Returns `encodeURIComponent(value)`.
///
/// Translation of the global `encodeURIComponent` of ECMA-262, which keeps the
/// ASCII letters and digits and `-_.!~*'()` and writes every other byte of the
/// UTF-8 form as `%XX` with upper case digits.
#[must_use]
pub fn encode_uri_component(value: &str) -> String {
    encode(value, b"")
}

/// Returns the ECMA-262 `Encode` of `value` with the extra unescaped set `extra`.
fn encode(value: &str, extra: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(value.len());
    for &byte in value.as_bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) || extra.contains(&byte) {
            out.push(char::from(byte));
        } else {
            out.push('%');
            out.push(char::from(DIGITS[usize::from(byte >> 4)]));
            out.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
        }
    }
    out
}

/// Returns the strict path of `url`, the path hono routes a request by.
///
/// The path starts at the first `/` after the scheme's `:` plus 3 and ends
/// before `?` or `#`. When it holds a `%` it is decoded with `decodeURI` after
/// every `%25` is written `%2525`, so a decoded percent never forms a new
/// escape.
#[must_use]
pub fn get_path(url: &str) -> String {
    let after_scheme = url.find(':').map_or(3, |colon| colon + 4);
    let Some(start) = url
        .get(after_scheme..)
        .and_then(|rest| rest.find('/'))
        .map(|at| at + after_scheme)
    else {
        return String::new();
    };
    let bytes = url.as_bytes();
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                let end = url[i..].find(['?', '#']).map_or(url.len(), |at| at + i);
                let path = &url[start..end];
                let path = if path.contains("%25") {
                    path.replace("%25", "%2525")
                } else {
                    String::from(path)
                };
                return try_decode(&path, decode_uri);
            }
            b'?' | b'#' => break,
            _ => i += 1,
        }
    }
    String::from(&url[start..i])
}

/// Returns the query of `url` from its first `?` after the scheme, with the `?`, or the empty string.
#[must_use]
pub fn get_query_strings(url: &str) -> String {
    match url.get(8..).and_then(|rest| rest.find('?')) {
        Some(at) => format!("?{}", &url[at + 8 + 1..]),
        None => String::new(),
    }
}

/// Returns the path of `url` without a trailing `/`, the path of a router that is not strict.
#[must_use]
pub fn get_path_no_strict(url: &str) -> String {
    let mut result = get_path(url);
    if result.len() > 1 && result.ends_with('/') {
        result.pop();
    }
    result
}

/// Returns `base` and `sub` joined with one `/` between them.
#[must_use]
pub fn merge_path(base: &str, sub: &str) -> String {
    let lead = if base.starts_with('/') { "" } else { "/" };
    if sub == "/" {
        return format!("{lead}{base}");
    }
    let join = if base.ends_with('/') { "" } else { "/" };
    let sub = sub.strip_prefix('/').unwrap_or(sub);
    format!("{lead}{base}{join}{sub}")
}

/// Returns the paths an optional parameter stands for, or `None` for a path without one.
///
/// A path whose last character is `?` and that holds a `:` gives the path up to
/// each optional parameter and the path with it, each once.
#[must_use]
pub fn check_optional_parameter(path: &str) -> Option<Vec<String>> {
    if !path.ends_with('?') || !path.contains(':') {
        return None;
    }
    let mut results: Vec<String> = Vec::new();
    let mut base_path = String::new();
    for segment in path.split('/') {
        if !segment.is_empty() && !segment.contains(':') {
            base_path.push('/');
            base_path.push_str(segment);
        } else if segment.contains(':') {
            if segment.contains('?') {
                if results.is_empty() && base_path.is_empty() {
                    results.push(String::from("/"));
                } else {
                    results.push(base_path.clone());
                }
                let optional_segment = segment.replacen('?', "", 1);
                base_path.push('/');
                base_path.push_str(&optional_segment);
                results.push(base_path.clone());
            } else {
                base_path.push('/');
                base_path.push_str(segment);
            }
        }
    }
    let mut unique: Vec<String> = Vec::with_capacity(results.len());
    for result in results {
        if !unique.contains(&result) {
            unique.push(result);
        }
    }
    Some(unique)
}

/// Returns a query value decoded as hono decodes it, `+` as a space and escapes through `decodeURIComponent`.
fn decode_query(value: &str) -> String {
    if !value.contains(['%', '+']) {
        return String::from(value);
    }
    let value = value.replace('+', " ");
    if value.contains('%') {
        try_decode(&value, decode_uri_component)
    } else {
        value
    }
}

/// Returns the first value of the query parameter `key` of `url`.
///
/// A key with no `%` and no `+` is looked for in the URL string first, as the
/// reference's fast path does, and the parameters are decoded one by one only
/// when that finds nothing and the URL holds a `%` or a `+`.
#[must_use]
pub fn get_query_param(url: &str, key: &str) -> Option<String> {
    if !key.is_empty() && !key.contains(['%', '+']) {
        let query = url
            .get(8..)
            .and_then(|rest| rest.find('?'))
            .map(|at| at + 8)?;
        let mut key_index = if url[query + 1..].starts_with(key) {
            Some(query)
        } else {
            find_from(url, &format!("&{key}"), query + 1)
        };
        while let Some(at) = key_index {
            match url.as_bytes().get(at + key.len() + 1) {
                Some(b'=') => {
                    let value_index = at + key.len() + 2;
                    let end = find_from(url, "&", value_index).unwrap_or(url.len());
                    return Some(decode_query(&url[value_index..end]));
                }
                Some(b'&') | None => return Some(String::new()),
                Some(_) => key_index = find_from(url, &format!("&{key}"), at + 1),
            }
        }
        if !url.contains(['%', '+']) {
            return None;
        }
    }
    query_values(url, Some(key), false)
        .into_iter()
        .next()
        .map(|(_, values)| values[0].clone())
}

/// Returns the byte index of the first `needle` in `haystack` at or after `from`.
fn find_from(haystack: &str, needle: &str, from: usize) -> Option<usize> {
    haystack.get(from..)?.find(needle).map(|at| at + from)
}

/// Returns every value of the query parameter `key` of `url`, `None` when it is missing.
#[must_use]
pub fn get_query_params(url: &str, key: &str) -> Option<Vec<String>> {
    query_values(url, Some(key), true)
        .into_iter()
        .next()
        .map(|(_, values)| values)
}

/// Returns every query parameter of `url` with its first value, in the order of their first key.
#[must_use]
pub fn get_query_param_all(url: &str) -> Vec<(String, String)> {
    query_values(url, None, false)
        .into_iter()
        .map(|(name, values)| (name, values[0].clone()))
        .collect()
}

/// Returns the query parameters of `url` in the order of their first key, each with its values.
///
/// The slow path of `_getQueryParam`: every name and value is decoded when
/// the URL holds a `%` or a `+`, a key already seen keeps its first value
/// unless `multiple` asks for all of them, and an empty name is skipped.
fn query_values(url: &str, key: Option<&str>, multiple: bool) -> Vec<(String, Vec<String>)> {
    let encoded = url.contains(['%', '+']);
    let mut results: Vec<(String, Vec<String>)> = Vec::new();
    let Some(mut key_index) = url
        .get(8..)
        .and_then(|rest| rest.find('?'))
        .map(|at| at + 8)
    else {
        return results;
    };
    loop {
        let next_key_index = url[key_index + 1..].find('&').map(|at| at + key_index + 1);
        let mut value_index = url[key_index..].find('=').map(|at| at + key_index);
        if let (Some(value), Some(next)) = (value_index, next_key_index)
            && value > next
        {
            value_index = None;
        }
        let name_end = value_index.or(next_key_index).unwrap_or(url.len());
        let mut name = String::from(&url[key_index + 1..name_end]);
        if encoded {
            name = decode_query(&name);
        }
        if !name.is_empty() {
            let value = match value_index {
                None => String::new(),
                Some(at) => {
                    let raw = &url[at + 1..next_key_index.unwrap_or(url.len())];
                    if encoded {
                        decode_query(raw)
                    } else {
                        String::from(raw)
                    }
                }
            };
            match results.iter_mut().find(|(known, _)| *known == name) {
                Some((_, values)) if multiple => values.push(value),
                Some(_) => {}
                None => results.push((name, vec![value])),
            }
        }
        let Some(next) = next_key_index else {
            break;
        };
        key_index = next;
    }
    match key {
        Some(key) => results
            .into_iter()
            .filter(|(name, _)| name == key)
            .collect(),
        None => results,
    }
}
