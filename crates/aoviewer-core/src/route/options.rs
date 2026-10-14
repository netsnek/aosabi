// SPDX-License-Identifier: MIT
/*
 * aoviewer-core route options: the option grammar after the question mark of a
 * route.
 *
 * The keys relay, deprecated, leaf, sort, hideRoot, nodes, descdep, x, theme,
 * z, hide, lens and ring are read, and an unknown key or value is refused. The
 * pairs are split and decoded as the parser of
 * application/x-www-form-urlencoded splits and decodes them.
 */

//! The route options.
//!
//! | Key | Value | Default |
//! | --- | --- | --- |
//! | `relay` | `0` or `1` | `1` |
//! | `deprecated` | `0` or `1` | `0` |
//! | `leaf` | `0` or `1` | `1` |
//! | `sort` | `schema` or `alpha` | `schema` |
//! | `hideRoot` | `0` or `1` | `0` |
//! | `nodes` | `all` or `composite` | `all` |
//! | `descdep` | `0` or `1` | `1` |
//! | `x` | type names, separated by commas, at most [`MAX_EXPAND`] | none |
//! | `theme` | `auto`, `light` or `dark` | `auto` |
//! | `z` | the zoom factor and the two offsets in pixels, separated by commas | none |
//! | `hide` | globs, separated by commas, at most [`MAX_HIDE_GLOBS`] | none |
//! | `lens` | `Query`, `Mutation` or `Subscription` | none |
//! | `ring` | `0` to [`MAX_RING`] | [`DEFAULT_RING`] |
//!
//! The keys the transforms carry take the defaults of [`Transforms::default`].
//! Auto is the default theme, and 24 boxes per side is the default ring. The
//! zoom is kept in per mille and the offsets in centipixels, so the zoom takes
//! at most three decimals and an offset at most two, and a finer value is
//! refused rather than rounded. A zoom of 0 is refused, because it scales the
//! graph to nothing.
//!
//! [`Options::to_query`] writes the keys in the order of the table and leaves
//! out every key that holds its default, so two equal sets of options write
//! one option string.
//!
//! Every loop runs over the bytes of the option string, which
//! [`Options::parse`] caps at [`MAX_ROUTE_BYTES`] before it reads a pair, over
//! its pairs, of which there is at most one more than it has bytes, over at
//! most [`MAX_EXPAND`] names, [`MAX_HIDE_GLOBS`] globs or the three parts of
//! `z=` in one value, or, in the writer, over the bytes of one item it
//! encodes.

use alloc::{
    format,
    string::String,
    vec::Vec, //
};

use core::fmt::Write;

use super::{
    MAX_ROUTE_BYTES,
    decode,
    is_name,
    push_encoded, //
};

use crate::{
    model::RootKind,
    transform::{
        Glob,
        MAX_HIDE_GLOBS,
        NodeSet,
        SortOrder,
        Transforms, //
    },
};

/// The ring budget of an ego view unless `ring=` sets another, in boxes per
/// side.
pub const DEFAULT_RING: u16 = 24;

/// The largest ring budget `ring=` takes, in boxes per side.
pub const MAX_RING: u16 = 64;

/// The most type names `x=` takes.
///
/// An expanded type is a neighbour box of its view, and the largest budget of
/// a view draws 150 boxes. The cap takes that count.
pub const MAX_EXPAND: usize = 150;

/// The keys of the option grammar in the order [`Options::to_query`] writes
/// them.
const KEYS: [&str; 13] = [
    "relay",
    "deprecated",
    "leaf",
    "sort",
    "hideRoot",
    "nodes",
    "descdep",
    "x",
    "theme",
    "z",
    "hide",
    "lens",
    "ring",
];

/// The option string after the question mark.
///
/// Every key of `docs/aoviewer.md` section 3.8 is read. The generator uses
/// all but `theme`, `z` and `lens`, which it carries and writes back
/// unchanged. [`Options::default`] carries the default of every key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    /// The six Voyager flags and the layers, `relay`, `deprecated`, `leaf`,
    /// `sort`, `hideRoot`, `nodes`, `descdep` and `hide`.
    pub transforms: Transforms,
    /// `x=Project,Domain`, the types whose own ring an ego view adds, in the
    /// order the option string lists them, at most [`MAX_EXPAND`] of them in
    /// an option string.
    pub expand: Vec<String>,
    /// `theme=auto`, `theme=light` or `theme=dark`.
    pub theme: ThemeChoice,
    /// `z=1.2,340,120`, the zoom in per mille and the two offsets in
    /// centipixels.
    pub zoom: Option<(u32, i32, i32)>,
    /// `lens=Mutation`, the root whose reachable types stay at full strength.
    pub lens: Option<RootKind>,
    /// `ring=24`, the ring budget in boxes per side, at most [`MAX_RING`].
    pub ring: u16,
}

/// The theme an option string asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeChoice {
    /// `theme=auto`, which follows `prefers-color-scheme`.
    Auto,
    /// `theme=light`.
    Light,
    /// `theme=dark`.
    Dark,
}

/// A reason [`Options::parse`] refuses an option string.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OptionError {
    /// A key the grammar does not name, decoded.
    UnknownKey(String),
    /// A value outside the grammar of its key, or a key that appears twice.
    BadValue {
        /// The key, decoded.
        key: String,
        /// The value, decoded.
        value: String,
    },
    /// The option string is longer than [`MAX_ROUTE_BYTES`].
    TooLong,
}

impl Default for Options {
    /// Returns the default of every key, the option string that is empty.
    fn default() -> Self {
        Options {
            transforms: Transforms::default(),
            expand: Vec::new(),
            theme: ThemeChoice::Auto,
            zoom: None,
            lens: None,
            ring: DEFAULT_RING,
        }
    }
}

impl Options {
    /// Reads an option string such as `relay=0&x=Project,Domain&ring=12`.
    ///
    /// A leading `?` is dropped, as the constructor of `URLSearchParams` drops
    /// it. The string splits at `&` into pairs and a pair at its first `=`
    /// into a key and a value, an empty pair is skipped, a pair without `=`
    /// has the empty value, a `+` reads as a space, and the key and the value
    /// are percent decoded. No value of the grammar holds a space, so a `+` is
    /// refused wherever it stands.
    ///
    /// An empty `x=` or `hide=` is the empty list. A key that appears twice
    /// is refused, so one option string has one meaning. A string longer
    /// than [`MAX_ROUTE_BYTES`], its leading `?` not counted, is refused
    /// before a pair is read, since it is longer than any hash
    /// [`super::Route::parse`] reads.
    ///
    /// # Errors
    ///
    /// Returns [`OptionError::UnknownKey`] for a key outside the grammar,
    /// [`OptionError::BadValue`] for a value outside the grammar of its key
    /// or for the second pair of one key, and [`OptionError::TooLong`] for a
    /// string longer than [`MAX_ROUTE_BYTES`].
    ///
    /// [URL standard]: https://github.com/whatwg/url/blob/8e14777cfa145b08a9fb735fe580ec0c366564c3/url.bs
    pub fn parse(query: &str) -> Result<Options, OptionError> {
        Options::from_pairs(query.strip_prefix('?').unwrap_or(query))
    }

    /// Returns the canonical option string, the keys in the order of the
    /// grammar and every key that holds its default left out.
    ///
    /// The options that are all defaults write the empty string. At most the
    /// first [`MAX_EXPAND`] names of `x=` and the first [`MAX_HIDE_GLOBS`]
    /// globs of `hide=` are written, the most [`Options::parse`] reads. The
    /// globs left out are the ones [`crate::transform::apply`] never reads.
    ///
    /// An item of `x=` is percent encoded outside `[A-Za-z0-9_*?]`, so no
    /// item splits its pair or starts another. [`Options::parse`] decodes a
    /// value before it splits the list at its commas, so an item holding a
    /// comma reads back as the pieces between its commas when each of them
    /// is a name, and a lone empty item reads back as the empty list. Any
    /// other item that is no GraphQL name makes [`Options::parse`] refuse
    /// the string, as a zoom of 0, a ring past [`MAX_RING`] and a string
    /// longer than [`MAX_ROUTE_BYTES`] do.
    #[must_use]
    pub fn to_query(&self) -> String {
        let mut out = String::new();
        self.write_query(&mut out);
        out
    }

    /// Reads the pairs of an option string whose leading `?` is gone.
    pub(super) fn from_pairs(query: &str) -> Result<Options, OptionError> {
        if query.len() > MAX_ROUTE_BYTES {
            return Err(OptionError::TooLong);
        }
        let mut options = Options::default();
        let mut seen = [false; KEYS.len()];
        for pair in query.split('&').filter(|pair| !pair.is_empty()) {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            let (key, value) = (decode_form(key), decode_form(value));
            let Some(k) = KEYS.iter().position(|&known| known == key) else {
                return Err(OptionError::UnknownKey(key));
            };
            if seen[k] || options.read(k, &value).is_none() {
                return Err(OptionError::BadValue { key, value });
            }
            seen[k] = true;
        }
        Ok(options)
    }

    /// Sets the option of the key `KEYS[k]` from `value`, or returns `None`
    /// when the value is outside the grammar of that key.
    fn read(&mut self, k: usize, value: &str) -> Option<()> {
        let t = &mut self.transforms;
        match KEYS[k] {
            "relay" => t.relay = flag(value)?,
            "deprecated" => t.deprecated = flag(value)?,
            "leaf" => t.leaf = flag(value)?,
            "sort" => t.sort = sort(value)?,
            "hideRoot" => t.hide_root = flag(value)?,
            "nodes" => t.nodes = nodes(value)?,
            "descdep" => t.descdep = flag(value)?,
            "x" => self.expand = names(value)?,
            "theme" => self.theme = theme(value)?,
            "z" => self.zoom = Some(zoom(value)?),
            "hide" => t.hide = globs(value)?,
            "lens" => self.lens = Some(root(value)?),
            "ring" => self.ring = ring(value)?,
            _ => return None,
        }
        Some(())
    }

    /// Appends the canonical option string to `out`.
    pub(super) fn write_query(&self, out: &mut String) {
        let start = out.len();
        let t = &self.transforms;
        let flags = [
            (!t.relay, "relay=0"),
            (t.deprecated, "deprecated=1"),
            (!t.leaf, "leaf=0"),
            (t.sort == SortOrder::Alpha, "sort=alpha"),
            (t.hide_root, "hideRoot=1"),
            (t.nodes == NodeSet::Composite, "nodes=composite"),
            (!t.descdep, "descdep=0"),
        ];
        for (set, pair) in flags {
            if set {
                separate(out, start);
                out.push_str(pair);
            }
        }
        if !self.expand.is_empty() {
            push_key(out, start, "x");
            let items = self.expand.iter().take(MAX_EXPAND);
            push_list(items.map(String::as_str), out);
        }
        if self.theme != ThemeChoice::Auto {
            push_key(out, start, "theme");
            out.push_str(theme_word(self.theme));
        }
        if let Some((scale, x, y)) = self.zoom {
            push_key(out, start, "z");
            push_fixed(u64::from(scale), 3, out);
            out.push(',');
            push_signed(x, out);
            out.push(',');
            push_signed(y, out);
        }
        if !t.hide.is_empty() {
            push_key(out, start, "hide");
            let items = t.hide.iter().take(MAX_HIDE_GLOBS);
            push_list(items.map(Glob::as_str), out);
        }
        if let Some(lens) = self.lens {
            push_key(out, start, "lens");
            out.push_str(root_word(lens));
        }
        if self.ring != DEFAULT_RING {
            push_key(out, start, "ring");
            // Writing into a `String` cannot fail.
            let _ = write!(out, "{}", self.ring);
        }
    }
}

/// Returns `text` with every `+` read as a space and then percent decoded,
/// the decoding of a key and of a value of the URL standard.
fn decode_form(text: &str) -> String {
    if text.contains('+') {
        decode(&text.replace('+', " "))
    } else {
        decode(text)
    }
}

/// Reads `0` or `1`.
fn flag(value: &str) -> Option<bool> {
    match value {
        "0" => Some(false),
        "1" => Some(true),
        _ => None,
    }
}

/// Reads `schema` or `alpha`.
fn sort(value: &str) -> Option<SortOrder> {
    match value {
        "schema" => Some(SortOrder::Schema),
        "alpha" => Some(SortOrder::Alpha),
        _ => None,
    }
}

/// Reads `all` or `composite`.
fn nodes(value: &str) -> Option<NodeSet> {
    match value {
        "all" => Some(NodeSet::All),
        "composite" => Some(NodeSet::Composite),
        _ => None,
    }
}

/// Reads `auto`, `light` or `dark`.
fn theme(value: &str) -> Option<ThemeChoice> {
    match value {
        "auto" => Some(ThemeChoice::Auto),
        "light" => Some(ThemeChoice::Light),
        "dark" => Some(ThemeChoice::Dark),
        _ => None,
    }
}

/// Returns the word `theme=` writes for `theme`.
fn theme_word(theme: ThemeChoice) -> &'static str {
    match theme {
        ThemeChoice::Auto => "auto",
        ThemeChoice::Light => "light",
        ThemeChoice::Dark => "dark",
    }
}

/// Reads the name of a root, `Query`, `Mutation` or `Subscription`.
fn root(value: &str) -> Option<RootKind> {
    match value {
        "Query" => Some(RootKind::Query),
        "Mutation" => Some(RootKind::Mutation),
        "Subscription" => Some(RootKind::Subscription),
        _ => None,
    }
}

/// Returns the name `lens=` writes for `root`.
fn root_word(root: RootKind) -> &'static str {
    match root {
        RootKind::Query => "Query",
        RootKind::Mutation => "Mutation",
        RootKind::Subscription => "Subscription",
    }
}

/// Reads a list of at most [`MAX_EXPAND`] GraphQL names separated by commas,
/// the empty value being the empty list.
fn names(value: &str) -> Option<Vec<String>> {
    if value.is_empty() {
        return Some(Vec::new());
    }
    let mut out = Vec::new();
    for name in value.split(',').take(MAX_EXPAND + 1) {
        if out.len() == MAX_EXPAND || !is_name(name) {
            return None;
        }
        out.push(String::from(name));
    }
    Some(out)
}

/// Reads a list of at most [`MAX_HIDE_GLOBS`] globs separated by commas,
/// the empty value being the empty list.
fn globs(value: &str) -> Option<Vec<Glob>> {
    if value.is_empty() {
        return Some(Vec::new());
    }
    let mut out = Vec::new();
    for pattern in value.split(',').take(MAX_HIDE_GLOBS + 1) {
        if out.len() == MAX_HIDE_GLOBS {
            return None;
        }
        out.push(Glob::parse(pattern)?);
    }
    Some(out)
}

/// Reads a ring budget, decimal digits of a number up to [`MAX_RING`].
fn ring(value: &str) -> Option<u16> {
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    value.parse::<u16>().ok().filter(|&ring| ring <= MAX_RING)
}

/// Reads `z=`, a zoom factor above 0 with at most three decimals and two
/// offsets in pixels with at most two, into per mille and centipixels.
fn zoom(value: &str) -> Option<(u32, i32, i32)> {
    let mut parts = value.split(',');
    let (scale, x, y) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    let scale = u32::try_from(fixed(scale, 3)?)
        .ok()
        .filter(|&scale| scale > 0)?;
    Some((scale, signed(x)?, signed(y)?))
}

/// Reads an offset, an optional `-` and a decimal with at most two
/// decimals, into centipixels.
fn signed(text: &str) -> Option<i32> {
    let (negative, digits) = match text.strip_prefix('-') {
        Some(digits) => (true, digits),
        None => (false, text),
    };
    let magnitude = i64::try_from(fixed(digits, 2)?).ok()?;
    i32::try_from(if negative { -magnitude } else { magnitude }).ok()
}

/// Reads a decimal of digits, optionally a point and at most `places` more
/// digits, as an integer in units of ten to the minus `places`.
///
/// Returns `None` for any other text and for a value past `u64`.
fn fixed(text: &str, places: u32) -> Option<u64> {
    let (whole, fraction) = match text.split_once('.') {
        Some((whole, fraction)) => (whole, fraction),
        None => (text, ""),
    };
    let is_digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    let point = whole.len() < text.len();
    if whole.is_empty()
        || (point && fraction.is_empty())
        || fraction.len() > places as usize
        || !is_digits(whole)
        || !is_digits(fraction)
    {
        return None;
    }
    let mut value: u64 = 0;
    for b in whole.bytes().chain(fraction.bytes()) {
        value = value.checked_mul(10)?.checked_add(u64::from(b - b'0'))?;
    }
    let short = places - u32::try_from(fraction.len()).ok()?;
    value.checked_mul(10u64.checked_pow(short)?)
}

/// Appends `value`, an integer in units of ten to the minus `places`, as a
/// decimal with its trailing zeros dropped.
fn push_fixed(value: u64, places: u32, out: &mut String) {
    let unit = 10u64.pow(places);
    let (whole, fraction) = (value / unit, value % unit);
    // Writing into a `String` cannot fail.
    let _ = write!(out, "{whole}");
    if fraction != 0 {
        let digits = format!("{fraction:0width$}", width = places as usize);
        out.push('.');
        out.push_str(digits.trim_end_matches('0'));
    }
}

/// Appends an offset in centipixels as pixels, with a `-` when it is below 0.
fn push_signed(value: i32, out: &mut String) {
    if value < 0 {
        out.push('-');
    }
    push_fixed(u64::from(value.unsigned_abs()), 2, out);
}

/// Appends `&` to `out` when a pair is written after `start` already.
fn separate(out: &mut String, start: usize) {
    if out.len() > start {
        out.push('&');
    }
}

/// Appends the separator, `key` and `=` to `out`.
fn push_key(out: &mut String, start: usize, key: &str) {
    separate(out, start);
    out.push_str(key);
    out.push('=');
}

/// Appends `items` separated by commas, every byte that is no letter, digit,
/// `_`, `*` or `?` percent encoded, so an item never splits its pair or starts
/// another. A comma inside an item is encoded as well, but the reader decodes
/// the value before it splits the list, so the item reads back split there.
fn push_list<'a>(items: impl Iterator<Item = &'a str>, out: &mut String) {
    for (i, item) in items.enumerate() {
        if i > 0 {
            out.push(',');
        }
        push_encoded(item, is_list_byte, out);
    }
}

/// Returns whether `b` stands in an item of a list as it is, which every
/// byte of a GraphQL name and of a glob does.
fn is_list_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'*' | b'?')
}
