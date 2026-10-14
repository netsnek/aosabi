// SPDX-License-Identifier: MIT
/*
 * aoviewer-docs config: the configuration file aosabi.docs.toml beside
 * Cargo.toml.
 *
 * The file is configuration of a tool, as Cargo.toml is, and a flag of the
 * command line wins over a key.
 */

//! The configuration file.
//!
//! `aosabi.docs.toml` sits beside `Cargo.toml` and holds the keys below and
//! the table `[clusters]`, read through the TOML subset of [`crate::toml`].
//! It is no construct of the service source and lies outside the byte
//! contract, and the declared clusters live there and nowhere else.
//!
//! | Key | Value | Sets |
//! | --- | --- | --- |
//! | `docs` | A boolean, `true` by default | [`Config::docs`], which the build step reads |
//! | `out` | A path, relative to the directory of the file | [`Request::out`] |
//! | `formats` | An array of `md`, `typ` and `svg` | [`DocsOptions::formats`] |
//! | `themes` | An array of `light` and `dark` | [`DocsOptions::themes`] |
//! | `layout` | `auto`, `single` or `split` | [`DocsOptions::layout`] |
//! | `view` | An option string of the routes, such as `relay=0` | [`DocsOptions::view`] |
//! | `viewer_url` | The URL of the live viewer | [`DocsOptions::viewer_url`] |
//! | `mermaid` | A boolean | [`DocsOptions::mermaid`] |
//! | `poster` | A boolean | [`DocsOptions::poster`] |
//! | `typst_diagrams` | `native` or `svg` | [`DocsOptions::typst_diagrams`] |
//! | `typst_template` | A boolean | [`DocsOptions::typst_template`] |
//! | `pdf` | A boolean, `false` by default | [`Request::pdf`] |
//! | `[clusters]` | One key per cluster, an array of type name globs | [`DocsOptions::declared`] |
//!
//! The words of `formats`, `themes`, `layout` and `typst_diagrams` are read
//! through the lists and lookups the query form of [`DocsOptions`] reads them
//! with, such as [`Formats::WORDS`] and [`LayoutChoice::from_word`], so a
//! word means one option in the file and in the query, and the flags of the
//! command take the same words. The key `pdf` stands apart from `formats`, as
//! the PDF is no format of the tree, while the flag `--format` also takes the
//! word `pdf`.
//!
//! A path in `out` is read relative to the directory of the file, as Cargo
//! reads a path of `Cargo.toml` relative to the directory of the manifest.
//!
//! [`Config::read`] refuses with `AO-D0006` at the line and column of the
//! first defect: what the TOML subset refuses, a key or a table the file does
//! not take, a value of the wrong type, an `out` that is empty or holds a
//! NUL, a `viewer_url` with a control character, a word given twice, a
//! `view` the grammar of the routes refuses, `pdf = true` beside a `formats`
//! without `typ`, a glob [`Glob::parse`] refuses and more clusters or globs
//! than the core reads. It refuses with `AO-D0007` a word outside the list
//! of its key, naming the key with its place, the word and the list. The
//! subset reads the whole file before a key is looked at, so a defect of its
//! grammar is refused before a defect of a key earlier in the file.
//!
//! [`Config::read`] takes a missing file for an empty one, which is the rule
//! of the file the build step and the command look for beside `Cargo.toml`.
//! [`Config::read_required`] reads a path the caller names, as the flag
//! `--clusters` names one, and refuses it when it is missing.
//!
//! [`Config::apply`] sets every option the file holds into a [`Request`] and
//! leaves the others as they are. The caller applies the file to a new
//! request first and its flags after, so a flag wins over a key.
//!
//! [the Cargo book]: https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html

use std::{
    fs::File,
    io::{
        self,
        Read as _, //
    },
    path::{
        Path,
        PathBuf, //
    },
};

use aoviewer_core::{
    cluster::{
        Declared,
        MAX_DECLARED,
        MAX_DECLARED_GLOBS, //
    },
    route::{
        OptionError,
        Options, //
    },
    transform::{
        Glob,
        MAX_GLOB_BYTES, //
    },
    tree::{
        DocsOptions,
        Formats,
        LayoutChoice,
        Themes,
        TypstDiagrams, //
    },
};

use crate::{
    error::{
        DocsError,
        Position, //
    },
    request::Request,
    toml::{
        self,
        Entry,
        Item,
        Table,
        Value, //
    },
};

/// The name of the file beside `Cargo.toml`.
pub const FILE: &str = "aosabi.docs.toml";

/// The keys of the root of the file, in the order of the plan.
const KEYS: [&str; 12] = [
    "docs",
    "out",
    "formats",
    "themes",
    "layout",
    "view",
    "viewer_url",
    "mermaid",
    "poster",
    "typst_diagrams",
    "typst_template",
    "pdf",
];

/// The one table of the file.
const CLUSTERS: &str = "clusters";

/// The keys of `aosabi.docs.toml`.
///
/// Every key is `None` when the file does not hold it, but `docs`, which
/// holds its default then. [`Config::default`] is the configuration of an
/// empty file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    /// Whether the build step writes the reference, `true` unless the file
    /// says `docs = false`.
    pub docs: bool,
    /// The output directory, joined to the directory of the file.
    pub out: Option<PathBuf>,
    /// The formats of the tree.
    pub formats: Option<Formats>,
    /// The themes of the drawings.
    pub themes: Option<Themes>,
    /// The layout of the Markdown pages.
    pub layout: Option<LayoutChoice>,
    /// The options of the views and the transforms.
    pub view: Option<Options>,
    /// The URL of the live viewer.
    pub viewer_url: Option<String>,
    /// Whether small neighbourhood diagrams gain a Mermaid form.
    pub mermaid: Option<bool>,
    /// Whether the tree writes the posters.
    pub poster: Option<bool>,
    /// How the Typst chapters draw a diagram.
    pub typst_diagrams: Option<TypstDiagrams>,
    /// Whether the Typst files import `template.typ`.
    pub typst_template: Option<bool>,
    /// Whether the run compiles the PDF, `false` by default.
    pub pdf: Option<bool>,
    /// The declared clusters, in the order of the file.
    pub clusters: Option<Vec<Declared>>,
}

impl Default for Config {
    /// Returns the configuration of an empty file.
    fn default() -> Self {
        Config {
            docs: true,
            out: None,
            formats: None,
            themes: None,
            layout: None,
            view: None,
            viewer_url: None,
            mermaid: None,
            poster: None,
            typst_diagrams: None,
            typst_template: None,
            pdf: None,
            clusters: None,
        }
    }
}

impl Config {
    /// Reads the configuration file at `path`, or returns `None` when no
    /// file is there.
    ///
    /// # Errors
    ///
    /// Returns [`DocsError::Config`] with the line and column of the first
    /// defect, at line 1 and column 1 for a file that exists and cannot be
    /// read, and [`DocsError::Value`] for a word outside the list of its key.
    pub fn read(path: &Path) -> Result<Option<Config>, DocsError> {
        match read_file(path) {
            Ok(bytes) => Config::from_bytes(path, &bytes).map(Some),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(unreadable(path, &e)),
        }
    }

    /// Reads the configuration file at `path`, which must exist.
    ///
    /// [`Config::read`] takes a missing file for an empty one, which is the
    /// rule of the file beside `Cargo.toml` the build step and the command
    /// look for. A path the caller names, as the flag `--clusters` names one,
    /// is read here, so a mistyped path is refused rather than read as no
    /// configuration.
    ///
    /// # Errors
    ///
    /// Returns [`DocsError::Config`] at line 1 and column 1 for a file that
    /// is missing or cannot be read, and what [`Config::read`] returns for a
    /// file it reads.
    pub fn read_required(path: &Path) -> Result<Config, DocsError> {
        let bytes = read_file(path).map_err(|e| unreadable(path, &e))?;
        Config::from_bytes(path, &bytes)
    }

    /// Reads the configuration of the file `path`, whose bytes are `bytes`.
    fn from_bytes(path: &Path, bytes: &[u8]) -> Result<Config, DocsError> {
        let doc = toml::parse(bytes).map_err(|e| refused(path, e.at, e.reason.to_string()))?;
        let dir = path.parent().unwrap_or(Path::new(""));
        let mut config = Config::default();
        for entry in &doc.root {
            config.set(path, dir, entry)?;
        }
        config.check_pdf(path, &doc.root)?;
        for table in &doc.tables {
            if table.name != CLUSTERS {
                let reason = format!(
                    "[{}] is no table of {FILE}, which takes the one table [{CLUSTERS}]",
                    table.name
                );
                return Err(refused(path, table.at, reason));
            }
            config.clusters = Some(clusters(path, table)?);
        }
        Ok(config)
    }

    /// Sets every option the file holds into `request` and leaves the
    /// others as they are.
    ///
    /// The caller applies the file to a new request before it sets its
    /// flags, so a flag wins over a key. The key `docs` sets no field of a
    /// request, and the build step reads it from [`Config::docs`].
    pub fn apply(&self, request: &mut Request) {
        if let Some(out) = &self.out {
            request.out.clone_from(out);
        }
        if let Some(pdf) = self.pdf {
            request.pdf = pdf;
        }
        let options = &mut request.options;
        if let Some(formats) = self.formats {
            options.formats = formats;
        }
        if let Some(themes) = self.themes {
            options.themes = themes;
        }
        if let Some(layout) = self.layout {
            options.layout = layout;
        }
        if let Some(view) = &self.view {
            options.view.clone_from(view);
        }
        if let Some(url) = &self.viewer_url {
            options.viewer_url = Some(url.clone());
        }
        self.apply_switches(options);
        if let Some(clusters) = &self.clusters {
            options.declared.clone_from(clusters);
        }
    }

    /// Refuses `pdf = true` beside a `formats` without `typ`, whose entries
    /// are `root`.
    ///
    /// The PDF is compiled from the `api.typ` of the tree, which a tree
    /// without the format `typ` lacks. A file that leaves `formats` out keeps
    /// the formats of the request, which hold `typ` by default.
    fn check_pdf(&self, path: &Path, root: &[Entry]) -> Result<(), DocsError> {
        if self.pdf != Some(true) || self.formats.is_none_or(|f| f.typ) {
            return Ok(());
        }
        let at = root
            .iter()
            .find(|e| e.key == "pdf")
            .map_or(Position { line: 1, column: 1 }, |e| e.value_at);
        let reason = String::from(
            "pdf is compiled from api.typ, which formats leaves out, so formats takes typ beside pdf = true",
        );
        Err(refused(path, at, reason))
    }

    /// Sets the switches of the Mermaid form, the posters and the Typst
    /// diagrams and template that the file holds.
    fn apply_switches(&self, options: &mut DocsOptions) {
        if let Some(mermaid) = self.mermaid {
            options.mermaid = mermaid;
        }
        if let Some(poster) = self.poster {
            options.poster = poster;
        }
        if let Some(diagrams) = self.typst_diagrams {
            options.typst_diagrams = diagrams;
        }
        if let Some(template) = self.typst_template {
            options.typst_template = template;
        }
    }

    /// Sets the key of `entry` of the root of the file at `path`, whose
    /// paths are relative to `dir`.
    fn set(&mut self, path: &Path, dir: &Path, entry: &Entry) -> Result<(), DocsError> {
        match entry.key.as_str() {
            "docs" => self.docs = boolean(path, entry)?,
            "out" => self.out = Some(dir.join(out(path, entry)?)),
            "formats" => self.formats = Some(formats(path, entry)?),
            "themes" => self.themes = Some(themes(path, entry)?),
            "layout" => self.layout = Some(layout(path, entry)?),
            "view" => self.view = Some(view(path, entry)?),
            "viewer_url" => self.viewer_url = Some(String::from(viewer_url(path, entry)?)),
            "mermaid" => self.mermaid = Some(boolean(path, entry)?),
            "poster" => self.poster = Some(boolean(path, entry)?),
            "typst_diagrams" => self.typst_diagrams = Some(typst_diagrams(path, entry)?),
            "typst_template" => self.typst_template = Some(boolean(path, entry)?),
            "pdf" => self.pdf = Some(boolean(path, entry)?),
            key => {
                let reason = format!(
                    "{key} is no key of {FILE}, which takes {} and the table [{CLUSTERS}]",
                    KEYS.join(", ")
                );
                return Err(refused(path, entry.at, reason));
            }
        }
        Ok(())
    }
}

/// Reads the file at `path`, at most one byte more than the subset reads.
fn read_file(path: &Path) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(toml::MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// Returns the refusal `AO-D0006` of the file `path`, which cannot be read
/// for `e`, at line 1 and column 1.
fn unreadable(path: &Path, e: &io::Error) -> DocsError {
    let reason = format!("the file cannot be read ({e})");
    refused(path, Position { line: 1, column: 1 }, reason)
}

/// Returns the refusal `AO-D0006` of the file `path` at `at`.
fn refused(path: &Path, at: Position, reason: String) -> DocsError {
    DocsError::Config {
        path: path.to_path_buf(),
        position: at,
        reason,
    }
}

/// Returns the refusal of the value of `entry`, which is no `what`.
fn mistyped(path: &Path, entry: &Entry, what: &str) -> DocsError {
    let reason = format!("{} takes {what}", entry.key);
    refused(path, entry.value_at, reason)
}

/// Returns the refusal `AO-D0007` of `word` at `at`, outside the list
/// `allowed` of the key `key`.
fn unlisted(
    path: &Path,
    key: &str,
    at: Position,
    word: &str,
    allowed: &'static [&'static str],
) -> DocsError {
    DocsError::Value {
        name: format!("{key} in {}:{}:{}", path.display(), at.line, at.column),
        value: String::from(word),
        allowed,
    }
}

/// Reads the boolean of `entry`.
fn boolean(path: &Path, entry: &Entry) -> Result<bool, DocsError> {
    match entry.value {
        Value::Boolean(b) => Ok(b),
        _ => Err(mistyped(path, entry, "a boolean, true or false")),
    }
}

/// Reads the basic string of `entry`.
fn string<'e>(path: &Path, entry: &'e Entry) -> Result<&'e str, DocsError> {
    match &entry.value {
        Value::String(s) => Ok(s),
        _ => Err(mistyped(path, entry, "a basic string")),
    }
}

/// Reads the array of basic strings of `entry`.
fn array<'e>(path: &Path, entry: &'e Entry) -> Result<&'e [Item], DocsError> {
    match &entry.value {
        Value::Array(items) => Ok(items),
        _ => Err(mistyped(path, entry, "an array of basic strings")),
    }
}

/// Reads the output directory of `entry`, which names one.
///
/// A NUL ends a path of the operating system, so a path that holds one names
/// no directory the file means, and the escape `\u0000` of a basic string
/// writes one.
///
/// [POSIX.1-2024]: https://pubs.opengroup.org/onlinepubs/9799919799/basedefs/V1_chap03.html
fn out<'e>(path: &Path, entry: &'e Entry) -> Result<&'e str, DocsError> {
    let dir = string(path, entry)?;
    if dir.is_empty() {
        let reason = String::from("out names no directory");
        return Err(refused(path, entry.value_at, reason));
    }
    if dir.contains('\0') {
        let reason = String::from("out holds a NUL character, which ends a path");
        return Err(refused(path, entry.value_at, reason));
    }
    Ok(dir)
}

/// Reads the URL of the live viewer of `entry`.
///
/// A URL holds digits, letters and a few graphic symbols and no control
/// character, while an escape of a basic string writes any, so the URL is
/// refused for one.
///
/// [RFC 3986]: https://www.rfc-editor.org/rfc/rfc3986
fn viewer_url<'e>(path: &Path, entry: &'e Entry) -> Result<&'e str, DocsError> {
    let url = string(path, entry)?;
    if let Some(c) = url.chars().find(char::is_ascii_control) {
        let reason = format!("viewer_url holds the control character {c:?}, which no URL holds");
        return Err(refused(path, entry.value_at, reason));
    }
    Ok(url)
}

/// Reads the word of `entry` through `from_word`, which names a value for
/// each of `allowed`.
fn word<T>(
    path: &Path,
    entry: &Entry,
    from_word: fn(&str) -> Option<T>,
    allowed: &'static [&'static str],
) -> Result<T, DocsError> {
    let text = string(path, entry)?;
    from_word(text).ok_or_else(|| unlisted(path, &entry.key, entry.value_at, text, allowed))
}

/// Reads the array of words of `entry` into `set`, each a word whose switch
/// `switch` returns, one of `allowed`, and each at most once.
fn switches<T>(
    path: &Path,
    entry: &Entry,
    set: &mut T,
    switch: for<'t> fn(&'t mut T, &str) -> Option<&'t mut bool>,
    allowed: &'static [&'static str],
) -> Result<(), DocsError> {
    for item in array(path, entry)? {
        let Some(on) = switch(set, &item.text) else {
            return Err(unlisted(path, &entry.key, item.at, &item.text, allowed));
        };
        if *on {
            let reason = format!("{} names {} twice", entry.key, item.text);
            return Err(refused(path, item.at, reason));
        }
        *on = true;
    }
    Ok(())
}

/// Reads `formats`.
fn formats(path: &Path, entry: &Entry) -> Result<Formats, DocsError> {
    let mut formats = Formats::NONE;
    switches(path, entry, &mut formats, Formats::switch, Formats::WORDS)?;
    Ok(formats)
}

/// Reads `themes`.
fn themes(path: &Path, entry: &Entry) -> Result<Themes, DocsError> {
    let mut themes = Themes::NONE;
    switches(path, entry, &mut themes, Themes::switch, Themes::WORDS)?;
    Ok(themes)
}

/// Reads `layout`.
fn layout(path: &Path, entry: &Entry) -> Result<LayoutChoice, DocsError> {
    word(path, entry, LayoutChoice::from_word, LayoutChoice::WORDS)
}

/// Reads `typst_diagrams`.
fn typst_diagrams(path: &Path, entry: &Entry) -> Result<TypstDiagrams, DocsError> {
    word(path, entry, TypstDiagrams::from_word, TypstDiagrams::WORDS)
}

/// Reads `view` through the grammar of the option string of the routes.
fn view(path: &Path, entry: &Entry) -> Result<Options, DocsError> {
    Options::parse(string(path, entry)?).map_err(|e| {
        let reason = match e {
            OptionError::UnknownKey(key) => {
                format!("view holds the key {key:?}, which the option string of the routes does not name")
            }
            OptionError::BadValue { key, value } => format!(
                "view gives {key} the value {value:?}, which its grammar refuses or which a second {key} repeats"
            ),
            OptionError::TooLong => {
                String::from("view is longer than the option string of the routes reads")
            }
        };
        refused(path, entry.value_at, reason)
    })
}

/// Reads the table `[clusters]` into the declared clusters, in the order of
/// the file.
fn clusters(path: &Path, table: &Table) -> Result<Vec<Declared>, DocsError> {
    let mut declared = Vec::new();
    for entry in &table.entries {
        if declared.len() == MAX_DECLARED {
            let reason = format!("[{CLUSTERS}] declares more than {MAX_DECLARED} clusters");
            return Err(refused(path, entry.at, reason));
        }
        let items = array(path, entry)?;
        if let Some(item) = items.get(MAX_DECLARED_GLOBS) {
            let reason = format!(
                "the cluster {} holds more than {MAX_DECLARED_GLOBS} globs",
                entry.key
            );
            return Err(refused(path, item.at, reason));
        }
        if let Some(item) = items.iter().find(|i| Glob::parse(&i.text).is_none()) {
            let reason = format!(
                "{:?} is no glob of a type name, which holds letters, digits, _, * and ? and at most {MAX_GLOB_BYTES} bytes",
                item.text
            );
            return Err(refused(path, item.at, reason));
        }
        declared.push(Declared {
            name: entry.key.clone(),
            globs: items.iter().map(|i| i.text.clone()).collect(),
        });
    }
    Ok(declared)
}
