// SPDX-License-Identifier: MIT
/*
 * aoviewer-core tree options: the options of a documentation tree.
 *
 * The options are the same in the command, in aosabi.docs.toml and in the
 * request record of the wasm module.
 */

//! The tree options.
//!
//! [`DocsOptions`] carries every option of a tree, and its query form is the
//! one text the command, the configuration file and the request record of the
//! wasm module share.
//!
//! | Key | Value | Default |
//! | --- | --- | --- |
//! | `schema` | the name of the schema | empty |
//! | `generator` | the generator string, such as `aoviewer-docs 0.1.0` | empty |
//! | `formats` | `md`, `typ` and `svg`, separated by commas | all three |
//! | `themes` | `light` and `dark`, separated by commas | both |
//! | `layout` | `auto`, `single` or `split` | `auto` |
//! | `view` | the option string of the routes, encoded | empty |
//! | `cluster.<name>` | the globs of a declared cluster, separated by commas | none |
//! | `viewer_url` | the URL of the live viewer | none |
//! | `mermaid` | `0` or `1` | `0` |
//! | `poster` | `0` or `1` | `0` |
//! | `typst_diagrams` | `native` or `svg` | `native` |
//! | `typst_template` | `0` or `1` | `0` |
//! | `svg_text` | `text` or `outlines` | `text` |
//! | `document_date` | Unix seconds, an optional `-` and decimal digits | none |
//!
//! The keys are the pairs the plan names, `schema`, `generator`, `formats`,
//! `themes`, `layout`, `view` and `cluster.<name>`, and the keys of
//! `aosabi.docs.toml` for the rest, so one word names one option in the file
//! and in the query. The defaults are the ones of the design: Markdown, Typst
//! and SVG in both themes, the layout chosen by the size of the schema, native
//! diagrams in Typst and text that stays text. The words of `formats`,
//! `themes`, `layout` and `typst_diagrams` are public as [`Formats::WORDS`],
//! [`Themes::WORDS`], [`LayoutChoice::WORDS`] and [`TypstDiagrams::WORDS`]
//! with the lookups beside them, so the configuration file and the flags read
//! them here and nowhere else.
//!
//! The pairs are split and decoded as the option string of a route splits and
//! decodes them, `&` between pairs, the first `=` between a key and its value,
//! `+` read as a space and percent escapes decoded. A key the table does not
//! name is refused, so is a value outside the grammar of its key and a key
//! given twice, so one query has one meaning. [`DocsOptions::to_query`] writes
//! the keys in the order of the table, the declared clusters in their order,
//! and leaves out every key that holds its default, so two equal sets of
//! options write one query.
//!
//! Every loop runs over the bytes of the query, which [`DocsOptions::parse`]
//! caps at [`MAX_QUERY_BYTES`] before it reads a pair, over its pairs, of
//! which there is at most one more than it has bytes, over at most
//! [`MAX_DECLARED`] declared clusters of at most [`MAX_DECLARED_GLOBS`] globs,
//! or over the items of one list of at most three words.
//!
//! [URL standard]: https://url.spec.whatwg.org/#urlencoded-parsing

use alloc::{
    string::String,
    vec::Vec, //
};
use core::fmt::Write;

use crate::{
    cluster::{
        Declared,
        MAX_DECLARED,
        MAX_DECLARED_GLOBS, //
    },
    render_svg::SvgText,
    route::{
        OptionError,
        Options, //
    },
};

/// The longest query [`DocsOptions::parse`] reads, in bytes, 1 MiB.
///
/// The longest option string of a route is 131,200 bytes, which its percent
/// escapes may triple, and the declared clusters of a configuration file of a
/// tool are a few hundred bytes.
pub const MAX_QUERY_BYTES: usize = 1 << 20;

/// The keys of the query besides `cluster.<name>`, in the order
/// [`DocsOptions::to_query`] writes them, the declared clusters between
/// `view` and `viewer_url`.
const KEYS: [&str; 13] = [
    "schema",
    "generator",
    "formats",
    "themes",
    "layout",
    "view",
    "viewer_url",
    "mermaid",
    "poster",
    "typst_diagrams",
    "typst_template",
    "svg_text",
    "document_date",
];

/// The prefix of the key of a declared cluster, `cluster.<name>`.
const CLUSTER_PREFIX: &str = "cluster.";

/// Every option of a tree, the same in the command, in `aosabi.docs.toml` and
/// in the request record of the wasm module.
///
/// [`DocsOptions::default`] carries the default of every option, the query
/// that is empty.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocsOptions {
    /// The name of the schema, which titles the Typst document and
    /// `manifest.json` records.
    pub schema_name: String,
    /// The generator string, such as `aoviewer-docs 0.1.0`, written by the
    /// caller and only into `manifest.json`.
    pub generator: String,
    /// The formats the tree writes.
    pub formats: Formats,
    /// The layout of the Markdown pages.
    pub layout: LayoutChoice,
    /// The themes every drawing is written in.
    pub themes: Themes,
    /// The options of the views and the transforms, in the grammar of the
    /// routes.
    pub view: Options,
    /// The declared clusters, in the order they were declared, which a type
    /// joins the first matching one of.
    pub declared: Vec<Declared>,
    /// The URL of the live viewer every page links to.
    pub viewer_url: Option<String>,
    /// Whether a neighbourhood diagram of at most 12 boxes gains a fenced
    /// Mermaid diagram beside it.
    pub mermaid: bool,
    /// Whether the tree writes the poster of the whole graph of every root.
    pub poster: bool,
    /// How the Typst chapters draw a diagram.
    pub typst_diagrams: TypstDiagrams,
    /// Whether every Typst file imports `template.typ` beside `api.typ` in
    /// place of `sabi-theme.typ`.
    pub typst_template: bool,
    /// How the text of a drawing is written.
    pub svg_text: SvgText,
    /// The date of the Typst document in Unix seconds, which passes as the
    /// `source_date_epoch` of a card.
    pub document_date: Option<i64>,
}

/// The formats a tree writes.
///
/// The PDF is no format of the tree, because the core never compiles one, so
/// the caller asks for it apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Formats {
    /// The Markdown pages.
    pub md: bool,
    /// The Typst document, its chapters and the theme.
    pub typ: bool,
    /// The SVG drawings.
    pub svg: bool,
}

/// The layout of the Markdown pages of a tree.
///
/// `Auto` selects `Single` up to 40 boxes and `Split` above.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutChoice {
    /// The layout follows the size of the schema.
    Auto,
    /// Every page in one `API.md`.
    Single,
    /// `README.md` and one file per page under `md/`.
    Split,
}

/// The themes a tree writes its drawings in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Themes {
    /// The light theme.
    pub light: bool,
    /// The dark theme.
    pub dark: bool,
}

/// How a Typst chapter draws a diagram.
///
/// Native primitives are the default, and the image of an SVG file is the
/// cheaper route a template with its own drawing style takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypstDiagrams {
    /// The `#sv-diagram(..)` call of the scene.
    Native,
    /// An image over the SVG file of the theme of the document.
    Svg,
}

impl Default for DocsOptions {
    /// Returns the default of every option, the query that is empty.
    fn default() -> Self {
        DocsOptions {
            schema_name: String::new(),
            generator: String::new(),
            formats: Formats {
                md: true,
                typ: true,
                svg: true,
            },
            layout: LayoutChoice::Auto,
            themes: Themes {
                light: true,
                dark: true,
            },
            view: Options::default(),
            declared: Vec::new(),
            viewer_url: None,
            mermaid: false,
            poster: false,
            typst_diagrams: TypstDiagrams::Native,
            typst_template: false,
            svg_text: SvgText::Text,
            document_date: None,
        }
    }
}

impl Formats {
    /// The words of the formats, which the query form and `aosabi.docs.toml`
    /// take.
    pub const WORDS: &'static [&'static str] = &["md", "typ", "svg"];

    /// No format at all, which an empty list names.
    pub const NONE: Formats = Formats {
        md: false,
        typ: false,
        svg: false,
    };

    /// Returns the switch of the format `word` names, one of
    /// [`Formats::WORDS`], or `None` for any other word.
    pub fn switch(&mut self, word: &str) -> Option<&mut bool> {
        match word {
            "md" => Some(&mut self.md),
            "typ" => Some(&mut self.typ),
            "svg" => Some(&mut self.svg),
            _ => None,
        }
    }
}

impl Themes {
    /// The words of the themes, which the query form and `aosabi.docs.toml`
    /// take.
    pub const WORDS: &'static [&'static str] = &["light", "dark"];

    /// No theme at all, which an empty list names.
    pub const NONE: Themes = Themes {
        light: false,
        dark: false,
    };

    /// Returns the switch of the theme `word` names, one of
    /// [`Themes::WORDS`], or `None` for any other word.
    pub fn switch(&mut self, word: &str) -> Option<&mut bool> {
        match word {
            "light" => Some(&mut self.light),
            "dark" => Some(&mut self.dark),
            _ => None,
        }
    }
}

impl LayoutChoice {
    /// The words of the layouts, in the order of the variants.
    pub const WORDS: &'static [&'static str] = &[
        LayoutChoice::Auto.word(),
        LayoutChoice::Single.word(),
        LayoutChoice::Split.word(),
    ];

    /// Returns the word of the layout, such as `auto`.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            LayoutChoice::Auto => "auto",
            LayoutChoice::Single => "single",
            LayoutChoice::Split => "split",
        }
    }

    /// Returns the layout the word `word` names, or `None` for a word
    /// outside [`LayoutChoice::WORDS`].
    #[must_use]
    pub fn from_word(word: &str) -> Option<LayoutChoice> {
        match word {
            "auto" => Some(LayoutChoice::Auto),
            "single" => Some(LayoutChoice::Single),
            "split" => Some(LayoutChoice::Split),
            _ => None,
        }
    }
}

impl TypstDiagrams {
    /// The words of the ways to draw a diagram in Typst, in the order of the
    /// variants.
    pub const WORDS: &'static [&'static str] =
        &[TypstDiagrams::Native.word(), TypstDiagrams::Svg.word()];

    /// Returns the word of the way to draw, such as `native`.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            TypstDiagrams::Native => "native",
            TypstDiagrams::Svg => "svg",
        }
    }

    /// Returns the way to draw the word `word` names, or `None` for a word
    /// outside [`TypstDiagrams::WORDS`].
    #[must_use]
    pub fn from_word(word: &str) -> Option<TypstDiagrams> {
        match word {
            "native" => Some(TypstDiagrams::Native),
            "svg" => Some(TypstDiagrams::Svg),
            _ => None,
        }
    }
}

impl DocsOptions {
    /// Reads the query form, such as
    /// `schema=Shop&formats=md,typ,svg&themes=light&layout=auto&view=relay%3D0`
    /// with the declared clusters as `cluster.<name>=<glob>,<glob>`.
    ///
    /// A leading `?` is dropped, an empty pair is skipped, a pair without `=`
    /// has the empty value, and a key absent from the query keeps its
    /// default. The value of `view` is read by [`Options::parse`] once it is
    /// decoded. A declared cluster needs a name, takes its globs in their
    /// order with every empty glob dropped, since an empty glob matches no
    /// name, and keeps the place its key holds among the declared clusters.
    /// An empty `formats=` or `themes=` is the empty list, and an empty
    /// `viewer_url=` the empty URL.
    ///
    /// # Errors
    ///
    /// Returns [`OptionError::TooLong`] for a query longer than
    /// [`MAX_QUERY_BYTES`], [`OptionError::UnknownKey`] for a key outside the
    /// table, and [`OptionError::BadValue`] for a value outside the grammar of
    /// its key, for a key given twice, for a cluster without a name, for more
    /// than [`MAX_DECLARED`] declared clusters or more than
    /// [`MAX_DECLARED_GLOBS`] globs in one, and for a `view` that
    /// [`Options::parse`] refuses.
    pub fn parse(query: &str) -> Result<DocsOptions, OptionError> {
        let query = query.strip_prefix('?').unwrap_or(query);
        if query.len() > MAX_QUERY_BYTES {
            return Err(OptionError::TooLong);
        }
        let mut options = DocsOptions::default();
        let mut seen = [false; KEYS.len()];
        for pair in query.split('&').filter(|pair| !pair.is_empty()) {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            let (key, value) = (decode(key), decode(value));
            let bad = || OptionError::BadValue {
                key: key.clone(),
                value: value.clone(),
            };
            if let Some(name) = key.strip_prefix(CLUSTER_PREFIX) {
                options.read_cluster(name, &value).ok_or_else(bad)?;
                continue;
            }
            let Some(k) = KEYS.iter().position(|&known| known == key) else {
                return Err(OptionError::UnknownKey(key));
            };
            if seen[k] || options.read(k, &value).is_none() {
                return Err(bad());
            }
            seen[k] = true;
        }
        Ok(options)
    }

    /// Returns the canonical query, the keys in the order of the table of
    /// the module, the declared clusters in their order, and every key that
    /// holds its default left out.
    ///
    /// The options that are all defaults write the empty string. Every byte of
    /// a value outside the unreserved characters of a URL, `*` and `?` is
    /// percent encoded, so no value splits its pair or its query.
    /// [`DocsOptions::parse`] decodes a value before it splits the globs of a
    /// declared cluster at their commas, so a glob that holds a comma reads
    /// back as the pieces between its commas, as an item of `x=` of a route
    /// does, and a type name holds no comma for either to match. It reads the
    /// query back to equal options as long as it is no longer than
    /// [`MAX_QUERY_BYTES`], no declared cluster has the empty name, an empty
    /// glob or a glob with a comma, and the lists stay within the caps of the
    /// clusters.
    ///
    /// [specification]: https://spec.graphql.org/October2021/
    #[must_use]
    pub fn to_query(&self) -> String {
        let mut out = String::new();
        self.write_query(true, &mut out);
        out
    }

    /// Appends the canonical query to `out`, without `schema` and
    /// `generator` unless `identity` holds.
    ///
    /// `manifest.json` records the options without the two, since it records
    /// the schema and the generator under keys of their own.
    pub(super) fn write_query(&self, identity: bool, out: &mut String) {
        let start = out.len();
        if identity && !self.schema_name.is_empty() {
            push_pair(out, start, "schema", &self.schema_name);
        }
        if identity && !self.generator.is_empty() {
            push_pair(out, start, "generator", &self.generator);
        }
        let f = self.formats;
        if !(f.md && f.typ && f.svg) {
            let words = [(f.md, "md"), (f.typ, "typ"), (f.svg, "svg")];
            push_words(out, start, "formats", &words);
        }
        let t = self.themes;
        if !(t.light && t.dark) {
            push_words(
                out,
                start,
                "themes",
                &[(t.light, "light"), (t.dark, "dark")],
            );
        }
        if self.layout != LayoutChoice::Auto {
            push_pair(out, start, "layout", self.layout.word());
        }
        let view = self.view.to_query();
        if !view.is_empty() {
            push_pair(out, start, "view", &view);
        }
        for declared in &self.declared {
            separate(out, start);
            out.push_str(CLUSTER_PREFIX);
            push_encoded(&declared.name, out);
            out.push('=');
            for (i, glob) in declared.globs.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                push_encoded(glob, out);
            }
        }
        if let Some(url) = &self.viewer_url {
            push_pair(out, start, "viewer_url", url);
        }
        self.write_switches(out, start);
    }

    /// Appends the pairs of the keys after `viewer_url` that differ from
    /// their defaults.
    fn write_switches(&self, out: &mut String, start: usize) {
        let flags = [
            (self.mermaid, "mermaid=1"),
            (self.poster, "poster=1"),
            (
                self.typst_diagrams == TypstDiagrams::Svg,
                "typst_diagrams=svg",
            ),
            (self.typst_template, "typst_template=1"),
            (self.svg_text == SvgText::Outlines, "svg_text=outlines"),
        ];
        for (set, pair) in flags {
            if set {
                separate(out, start);
                out.push_str(pair);
            }
        }
        if let Some(date) = self.document_date {
            separate(out, start);
            // Writing into a `String` cannot fail.
            let _ = write!(out, "document_date={date}");
        }
    }

    /// Sets the option of the key `KEYS[k]` from `value`, or returns `None`
    /// when the value is outside the grammar of that key.
    fn read(&mut self, k: usize, value: &str) -> Option<()> {
        match KEYS[k] {
            "schema" => self.schema_name = String::from(value),
            "generator" => self.generator = String::from(value),
            "formats" => self.formats = formats(value)?,
            "themes" => self.themes = themes(value)?,
            "layout" => self.layout = LayoutChoice::from_word(value)?,
            "view" => self.view = Options::parse(value).ok()?,
            "viewer_url" => self.viewer_url = Some(String::from(value)),
            "mermaid" => self.mermaid = flag(value)?,
            "poster" => self.poster = flag(value)?,
            "typst_diagrams" => self.typst_diagrams = TypstDiagrams::from_word(value)?,
            "typst_template" => self.typst_template = flag(value)?,
            "svg_text" => self.svg_text = svg_text(value)?,
            "document_date" => self.document_date = Some(epoch(value)?),
            _ => return None,
        }
        Some(())
    }

    /// Appends the declared cluster `name` with the globs of `value`, or
    /// returns `None` when the name is empty or taken, or a cap is crossed.
    fn read_cluster(&mut self, name: &str, value: &str) -> Option<()> {
        let taken = self.declared.iter().any(|d| d.name == name);
        if name.is_empty() || taken || self.declared.len() == MAX_DECLARED {
            return None;
        }
        let mut globs = Vec::new();
        for glob in value.split(',').filter(|g| !g.is_empty()) {
            if globs.len() == MAX_DECLARED_GLOBS {
                return None;
            }
            globs.push(String::from(glob));
        }
        self.declared.push(Declared {
            name: String::from(name),
            globs,
        });
        Some(())
    }
}

/// Returns `text` with every `+` read as a space and its percent escapes
/// decoded, as a key and a value of a form are decoded.
///
/// An escape that does not encode a byte stays as written, and bytes that
/// decode to no UTF-8 are replaced by U+FFFD.
///
/// [URL standard]: https://url.spec.whatwg.org/
fn decode(text: &str) -> String {
    if !text.contains(['+', '%']) {
        return String::from(text);
    }
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    // Each iteration moves `i` forward by one or three bytes, so the loop
    // runs at most once per byte of `text`.
    while i < bytes.len() {
        match (bytes[i], escaped(bytes, i)) {
            (_, Some(b)) => {
                out.push(b);
                i += 3;
            }
            (b'+', None) => {
                out.push(b' ');
                i += 1;
            }
            (b, None) => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Returns the byte a `%` and two hex digits at `bytes[i..]` encode.
fn escaped(bytes: &[u8], i: usize) -> Option<u8> {
    if bytes.get(i) != Some(&b'%') {
        return None;
    }
    let high = char::from(*bytes.get(i + 1)?).to_digit(16)?;
    let low = char::from(*bytes.get(i + 2)?).to_digit(16)?;
    u8::try_from((high << 4) | low).ok()
}

/// Appends `text` with every byte outside the unreserved characters of a URL,
/// `*` and `?` percent encoded in upper case hex.
///
/// The comma, the `&`, the `=` and the `+` are encoded with the rest, so a
/// value never splits its pair or its query and never reads back as a space.
fn push_encoded(text: &str, out: &mut String) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~' | b'*' | b'?') {
            out.push(char::from(b));
        } else {
            out.push('%');
            out.push(char::from(HEX[usize::from(b >> 4)]));
            out.push(char::from(HEX[usize::from(b & 0x0f)]));
        }
    }
}

/// Appends `&` to `out` when a pair is written after `start` already.
fn separate(out: &mut String, start: usize) {
    if out.len() > start {
        out.push('&');
    }
}

/// Appends the pair of `key` and the encoded `value`.
fn push_pair(out: &mut String, start: usize, key: &str, value: &str) {
    separate(out, start);
    out.push_str(key);
    out.push('=');
    push_encoded(value, out);
}

/// Appends the pair of `key` and the words of `words` that are set,
/// separated by commas.
fn push_words(out: &mut String, start: usize, key: &str, words: &[(bool, &str)]) {
    separate(out, start);
    out.push_str(key);
    out.push('=');
    let mut first = true;
    for &(set, word) in words {
        if set {
            if !first {
                out.push(',');
            }
            out.push_str(word);
            first = false;
        }
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

/// Reads a list of the words of [`Formats::WORDS`], each at most once, the
/// empty value being no format.
fn formats(value: &str) -> Option<Formats> {
    let mut f = Formats::NONE;
    switches(value, &mut f, Formats::switch)?;
    Some(f)
}

/// Reads a list of the words of [`Themes::WORDS`], each at most once, the
/// empty value being no theme.
fn themes(value: &str) -> Option<Themes> {
    let mut t = Themes::NONE;
    switches(value, &mut t, Themes::switch)?;
    Some(t)
}

/// Sets the switch of `set` that each word of the list `value` names through
/// `switch`, or returns `None` for a word it does not name or names twice.
fn switches<T>(
    value: &str,
    set: &mut T,
    switch: for<'t> fn(&'t mut T, &str) -> Option<&'t mut bool>,
) -> Option<()> {
    if value.is_empty() {
        return Some(());
    }
    for word in value.split(',') {
        let on = switch(set, word)?;
        if *on {
            return None;
        }
        *on = true;
    }
    Some(())
}

/// Reads `text` or `outlines`.
fn svg_text(value: &str) -> Option<SvgText> {
    match value {
        "text" => Some(SvgText::Text),
        "outlines" => Some(SvgText::Outlines),
        _ => None,
    }
}

/// Reads Unix seconds, an optional `-` and at least one decimal digit, into
/// an `i64`, refusing a value past its range.
fn epoch(value: &str) -> Option<i64> {
    let digits = value.strip_prefix('-').unwrap_or(value);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    value.parse::<i64>().ok()
}
