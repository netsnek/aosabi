// SPDX-License-Identifier: MIT
/*
 * aosabi-cli: the reader of the .env file of the working directory.
 *
 * Derived from dotenv 16.6.1, lib/main.js, BSD-2-Clause.
 */

//! The `.env` reader.
//!
//! The command line of the reference calls `dotenv.config()` at module scope,
//! so `.env` of the working directory is read before any command runs,
//! quietly, and a variable the environment already sets is never overridden.
//! The parse is one regular expression, `LINE`, run with the flags `m` and
//! `g` over the text with every `\r\n` and lone `\r` folded to `\n`, and each
//! match gives a key and a value that is trimmed, loses one pair of
//! surrounding quotes, and has `\n` and `\r` expanded when it was double
//! quoted.
//!
//! [`parse`] reproduces that expression by hand, with the order in which a
//! backtracking engine tries its alternatives, because the project takes no
//! regular expression crate. The pattern is
//!
//! ```text
//! (?:^|^)\s*(?:export\s+)?([\w.-]+)(?:\s*=\s*?|:\s+?)
//! (\s*'(?:\\'|[^'])*'|\s*"(?:\\"|[^"])*"|\s*`(?:\\`|[^`])*`|[^#\r\n]+)?
//! \s*(?:#.*)?(?:$|$)
//! ```
//!
//! where `\s` is the white space and the line terminators of ECMA-262, `\w`
//! is `[A-Za-z0-9_]`, `.` is any character but a line terminator, and `^` and
//! `$` match at the start and the end of every line. A value may span lines
//! inside quotes, and the white space before a key or a quoted value may
//! cross a line break, as the expression lets it. The pairs come out in the
//! order of the keys of the object the reference fills, every key that is an
//! array index first in ascending order and every other key in the order it
//! was first written, a later value of a key replacing the earlier one in
//! place. The key `__proto__` is left out, because the assignment of the
//! reference calls the setter of `Object.prototype` with a string, which does
//! nothing.
//!
//! The reading of a vault through `DOTENV_KEY` and the options of `config`
//! are not ported, since the reference calls `config()` with none.

use std::{
    ffi::OsString,
    fs,
    path::Path, //
};

/// The pairs of `.env` that the environment of the process does not set.
///
/// The command line adds them to the environment of every child process it
/// starts and never changes its own environment.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DotEnv {
    /// The pairs in the order [`parse`] returns them.
    pub pairs: Vec<(OsString, OsString)>,
}

/// The name of the file [`load`] reads in the working directory.
pub const FILE_NAME: &str = ".env";

/// Returns the pairs `dotenv.parse` returns for `src`, in the order of the
/// keys of its object.
#[must_use]
pub fn parse(src: &str) -> Vec<(String, String)> {
    let text = fold_line_breaks(src);
    let mut pairs: Vec<(String, String)> = Vec::new();
    let mut last = 0;
    while let Some(found) = exec(&text, last) {
        let key: String = text[found.key.0..found.key.1].iter().collect();
        let value = found
            .value
            .map_or_else(String::new, |(start, end)| clean(&text[start..end]));
        insert(&mut pairs, key, value);
        last = found.end;
    }
    order(pairs)
}

/// Reads `.env` of `cwd` and returns the pairs the environment does not set.
///
/// A missing or unreadable file gives no pair and prints nothing, as
/// `dotenv.config()` stays quiet by default. The bytes are read as UTF-8 with
/// U+FFFD for each ill formed sequence, as `readFileSync` with the encoding
/// `utf8` reads them.
#[must_use]
pub fn load(cwd: &Path) -> DotEnv {
    let Ok(bytes) = fs::read(cwd.join(FILE_NAME)) else {
        return DotEnv::default();
    };
    let pairs = parse(&String::from_utf8_lossy(&bytes))
        .into_iter()
        .filter(|(key, _)| std::env::var_os(key).is_none())
        .map(|(key, value)| (OsString::from(key), OsString::from(value)))
        .collect();
    DotEnv { pairs }
}

/// One match of `LINE`: the key, the value group and the end of the match,
/// each as indices into the folded text.
struct Found {
    key: (usize, usize),
    value: Option<(usize, usize)>,
    end: usize,
}

/// Returns the text with every `\r\n` and every lone `\r` replaced by `\n`.
fn fold_line_breaks(src: &str) -> Vec<char> {
    let mut out = Vec::with_capacity(src.len());
    let mut chars = src.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            out.push('\n');
        } else {
            out.push(c);
        }
    }
    out
}

/// Returns the first match of `LINE` at or after `from`, as `exec` finds it
/// with `lastIndex` at `from`.
fn exec(text: &[char], from: usize) -> Option<Found> {
    (from..=text.len())
        .filter(|&start| is_line_start(text, start))
        .find_map(|start| match_at(text, start))
}

/// Returns whether `at` is where `^` matches under the flag `m`.
fn is_line_start(text: &[char], at: usize) -> bool {
    at == 0 || is_line_terminator(text[at - 1])
}

/// Returns the match of `LINE` that starts at the line start `start`.
///
/// The white space before the key is taken whole, because the key that must
/// follow is no white space. The prefix `export` is tried first and left out
/// when the rest fails behind it, as an optional group is.
fn match_at(text: &[char], start: usize) -> Option<Found> {
    let at = skip_space(text, start);
    if has_prefix(text, at, "export") {
        let after = skip_space(text, at + 6);
        if after > at + 6
            && let Some(found) = after_prefix(text, after)
        {
            return Some(found);
        }
    }
    after_prefix(text, at)
}

/// Returns the match from the key at `at` on.
///
/// The key is the longest run of key characters, since the separator that
/// follows starts with none of them. The separator is `=` behind optional
/// white space, with the white space behind it taken lazily, or `:` with at
/// least one white space character behind it, also taken lazily.
fn after_prefix(text: &[char], at: usize) -> Option<Found> {
    let key_end = at + text[at..].iter().take_while(|&&c| is_key_char(c)).count();
    if key_end == at {
        return None;
    }
    let key = (at, key_end);
    let before_equals = skip_space(text, key_end);
    if text.get(before_equals) == Some(&'=') {
        let first = before_equals + 1;
        let found = (first..=skip_space(text, first))
            .find_map(|value_start| value_and_tail(text, value_start, key));
        if found.is_some() {
            return found;
        }
    }
    if text.get(key_end) == Some(&':') {
        let first = key_end + 1;
        return (first + 1..=skip_space(text, first))
            .find_map(|value_start| value_and_tail(text, value_start, key));
    }
    None
}

/// Returns the match from the value group at `at` on.
///
/// The group is optional and greedy, so each of its four alternatives is
/// tried in order with every end it can take, and the group is left out
/// last.
fn value_and_tail(text: &[char], at: usize, key: (usize, usize)) -> Option<Found> {
    let quoted = ['\'', '"', '`']
        .into_iter()
        .flat_map(|quote| quoted_ends(text, at, quote));
    for end in quoted.chain(bare_ends(text, at)) {
        if let Some(match_end) = tail(text, end) {
            return Some(Found {
                key,
                value: Some((at, end)),
                end: match_end,
            });
        }
    }
    tail(text, at).map(|match_end| Found {
        key,
        value: None,
        end: match_end,
    })
}

/// Returns the ends a quoted alternative can take at `at`, in the order the
/// engine tries them.
///
/// The alternative is white space, the quote, a run of `\` with the quote or
/// any character but the quote, and the closing quote. Taken greedily, the
/// run passes every escaped quote and stops at the first quote it cannot
/// pass, which closes it first. Backtracking then gives each escaped quote up
/// as the closing one, the last one first. Each end lies behind its closing
/// quote.
fn quoted_ends(text: &[char], at: usize, quote: char) -> Vec<usize> {
    let open = skip_space(text, at);
    if text.get(open) != Some(&quote) {
        return Vec::new();
    }
    let mut escaped = Vec::new();
    let mut i = open + 1;
    let mut closing = None;
    while i < text.len() {
        if text[i] == '\\' && text.get(i + 1) == Some(&quote) {
            escaped.push(i + 1);
            i += 2;
        } else if text[i] == quote {
            closing = Some(i);
            break;
        } else {
            i += 1;
        }
    }
    closing
        .into_iter()
        .chain(escaped.into_iter().rev())
        .map(|quote_at| quote_at + 1)
        .collect()
}

/// Returns the ends the bare alternative `[^#\r\n]+` can take at `at`, the
/// longest first.
fn bare_ends(text: &[char], at: usize) -> impl Iterator<Item = usize> {
    let run = text[at..]
        .iter()
        .take_while(|&&c| c != '#' && c != '\r' && c != '\n')
        .count();
    (at + 1..=at + run).rev()
}

/// Returns the end of `\s*(?:#.*)?(?:$|$)` from `at`, or `None` when it
/// cannot match there.
///
/// The white space is greedy and gives back one character at a time. A
/// comment runs to the end of its line, where `$` always matches.
fn tail(text: &[char], at: usize) -> Option<usize> {
    let space_end = skip_space(text, at);
    for end in (at..=space_end).rev() {
        if text.get(end) == Some(&'#') {
            let rest = text[end..]
                .iter()
                .take_while(|&&c| !is_line_terminator(c))
                .count();
            return Some(end + rest);
        }
        if end == text.len() || is_line_terminator(text[end]) {
            return Some(end);
        }
    }
    None
}

/// Returns the value as the reference stores it: trimmed, one pair of
/// surrounding quotes removed, and `\n` and `\r` expanded when it was double
/// quoted.
fn clean(raw: &[char]) -> String {
    let trimmed = trim(raw);
    let double = trimmed.first() == Some(&'"');
    let unquoted = strip_quotes(trimmed);
    let text: String = unquoted.into_iter().collect();
    if double {
        text.replace("\\n", "\n").replace("\\r", "\r")
    } else {
        text
    }
}

/// Returns `chars` with the white space of `String.prototype.trim` removed
/// from both ends.
fn trim(chars: &[char]) -> &[char] {
    let start = chars.iter().take_while(|&&c| is_space(c)).count();
    let end = chars.len()
        - chars[start..]
            .iter()
            .rev()
            .take_while(|&&c| is_space(c))
            .count();
    &chars[start..end]
}

/// Returns `value` after ``replace(/^(['"`])([\s\S]*)\1$/mg, '$2')``.
///
/// A match starts at a line start with a quote and ends at the last same
/// quote that ends a line, and every match is replaced by what lies between
/// its quotes, scanning on behind it.
fn strip_quotes(value: &[char]) -> Vec<char> {
    let mut out = Vec::with_capacity(value.len());
    let mut copied = 0;
    let mut at = 0;
    while at < value.len() {
        if let Some(close) = quoted_line_at(value, at) {
            out.extend_from_slice(&value[copied..at]);
            out.extend_from_slice(&value[at + 1..close]);
            copied = close + 1;
            at = close + 1;
        } else {
            at += 1;
        }
    }
    out.extend_from_slice(&value[copied..]);
    out
}

/// Returns the closing quote of a match of ``^(['"`])([\s\S]*)\1$`` at `at`.
fn quoted_line_at(value: &[char], at: usize) -> Option<usize> {
    if !is_line_start(value, at) {
        return None;
    }
    let quote = value[at];
    if !matches!(quote, '\'' | '"' | '`') {
        return None;
    }
    (at + 1..value.len()).rev().find(|&close| {
        value[close] == quote
            && value
                .get(close + 1)
                .is_none_or(|&next| is_line_terminator(next))
    })
}

/// Writes `value` under `key` the way an assignment to a plain object does.
fn insert(pairs: &mut Vec<(String, String)>, key: String, value: String) {
    if key == "__proto__" {
        return;
    }
    if let Some(pair) = pairs.iter_mut().find(|(known, _)| *known == key) {
        pair.1 = value;
    } else {
        pairs.push((key, value));
    }
}

/// Returns the pairs in the order of `Object.keys`, the array indices first
/// in ascending order.
fn order(pairs: Vec<(String, String)>) -> Vec<(String, String)> {
    let (mut indices, names): (Vec<_>, Vec<_>) = pairs
        .into_iter()
        .partition(|(key, _)| array_index(key).is_some());
    indices.sort_by_key(|(key, _)| array_index(key));
    indices.extend(names);
    indices
}

/// Returns the value of `key` when it is an array index, a canonical decimal
/// below 2^32 - 1.
fn array_index(key: &str) -> Option<u32> {
    if key.is_empty() || (key.len() > 1 && key.starts_with('0')) {
        return None;
    }
    if !key.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    key.parse::<u32>().ok().filter(|&index| index != u32::MAX)
}

/// Returns the index behind the white space that starts at `at`.
fn skip_space(text: &[char], at: usize) -> usize {
    at + text
        .get(at..)
        .map_or(0, |rest| rest.iter().take_while(|&&c| is_space(c)).count())
}

/// Returns whether `text` holds `prefix` at `at`.
fn has_prefix(text: &[char], at: usize, prefix: &str) -> bool {
    let length = prefix.chars().count();
    text.get(at..at + length)
        .is_some_and(|slice| slice.iter().copied().eq(prefix.chars()))
}

/// Returns whether `c` matches `[\w.-]`.
fn is_key_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-')
}

/// Returns whether `c` is a line terminator of ECMA-262.
pub(crate) fn is_line_terminator(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// Returns whether `c` matches `\s`, the white space and the line
/// terminators of ECMA-262, which is also what `trim` removes.
pub(crate) fn is_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}
