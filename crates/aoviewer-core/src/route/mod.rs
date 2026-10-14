// SPDX-License-Identifier: MIT
/*
 * aoviewer-core route: the routes of the viewer and of the documents.
 *
 * One route grammar holds in the URL, in the documents and in the view flag of
 * the command. The hash is read as a browser hands it over, which the fragment
 * state of the URL standard has percent encoded.
 */

//! The routes.
//!
//! [`Route::parse`] reads a hash such as `#/type/Organization/projects?relay=0`
//! into a [`Route`] and its [`Options`], and [`Route::to_hash`] writes the
//! canonical hash of a route under a set of options. Any hash the parser takes
//! writes back as the canonical hash of what it read, and a canonical hash of
//! at most [`MAX_ROUTE_BYTES`] reads and writes back byte for byte. A
//! canonical hash can be longer than the hash it came from and longer than
//! the cap, which the parser then refuses, because [`Route::to_hash`] writes a
//! byte outside `pchar` as three bytes, so a `%FF` the decoder reads as
//! U+FFFD writes back as `%EF%BF%BD`.
//!
//! | Hash | Route |
//! | --- | --- |
//! | `#/` | [`Route::Home`] |
//! | `#/type/<type>`, `#/type/<type>/<field>`, `#/type/<type>/<field>/<arg>` | [`Route::Type`] |
//! | `#/op/query/<field>`, `#/op/mutation/<field>`, `#/op/subscription/<field>` | [`Route::Op`] |
//! | `#/cluster/<slug>` | [`Route::Cluster`] |
//! | `#/clusters` | [`Route::Clusters`] |
//! | `#/whole/Query`, `#/whole/Mutation`, `#/whole/Subscription`, `#/whole/All` | [`Route::Whole`] |
//! | `#/path/<type>/<type>` | [`Route::Path`] |
//! | `#/search/<text>` | [`Route::Search`] |
//! | `#/unreachable`, `#/deprecated`, `#/scalars`, `#/relay` | the four report pages |
//!
//! Every form but `#/scalars` and `#/relay` is a route of `docs/aoviewer.md`
//! sections 3.8 and 3.11. The report pages of the scalars and of the Relay
//! wrappers are pages of the documents, and their hashes follow the two report
//! pages of the viewer.
//!
//! A part of a path is percent decoded as the URL standard decodes, a `%`
//! without two hex digits after it kept as it is, and read as UTF-8 with a
//! malformed sequence replaced by U+FFFD. A name, a root and a slug are then
//! checked against their grammar. The text of a search is everything after
//! `#/search/` up to the option string, slashes included, and may be empty.
//! [`Route::to_hash`] writes every part as a segment of RFC 3986, every byte
//! outside `pchar` percent encoded with upper case hex digits. No byte it keeps
//! is in the fragment percent-encode set, the set the fragment state encodes,
//! so a browser keeps the hash as it is written.
//!
//! Every loop runs over the bytes of one hash or of one option string, which
//! [`Route::parse`] and [`Options::parse`] cap at [`MAX_ROUTE_BYTES`] before
//! they read them, over the bytes of one part the writer encodes, or over at
//! most [`MAX_PARTS`] parts of a path.
//!
//! [URL standard]: https://github.com/whatwg/url/blob/8e14777cfa145b08a9fb735fe580ec0c366564c3/url.bs
//! [RFC 3986]: https://www.rfc-editor.org/rfc/rfc3986

mod options;

use alloc::{
    string::String,
    vec::Vec, //
};

pub use options::{
    DEFAULT_RING,
    MAX_EXPAND,
    MAX_RING,
    OptionError,
    Options,
    ThemeChoice, //
};

pub use crate::transform::{
    NodeSet,
    SortOrder, //
};

use crate::{
    model::RootKind,
    transform::{
        MAX_GLOB_BYTES,
        MAX_HIDE_GLOBS, //
    },
};

/// The longest hash [`Route::parse`] reads and the longest option string
/// [`Options::parse`] reads, in bytes.
///
/// The cap is twice the longest value of `hide=` the transforms read, 64 globs
/// of at most 1024 bytes and a comma after each.
pub const MAX_ROUTE_BYTES: usize = 2 * MAX_HIDE_GLOBS * (MAX_GLOB_BYTES + 1);

/// The most parts a path holds after its route word, the type, the field and
/// the argument of `#/type/<type>/<field>/<arg>`.
pub const MAX_PARTS: usize = 3;

/// One route of `docs/aoviewer.md` section 3.8, one grammar in the URL, the
/// documents and `--view`.
///
/// A name of a type, a field or an argument is the name as the schema writes
/// it, and the route holds it whether or not a schema declares it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Route {
    /// `#/`, the home page.
    Home,
    /// `#/type/Organization`, a type page and its ego graph, with the field
    /// and the argument that are selected.
    Type {
        /// The name of the type.
        name: String,
        /// The selected field, whose edge is highlighted.
        field: Option<String>,
        /// The selected argument of the selected field.
        arg: Option<String>,
    },
    /// `#/op/mutation/createOrganization`, a root field page and its
    /// operation graph.
    Op {
        /// The root the field belongs to.
        root: RootKind,
        /// The name of the root field.
        field: String,
    },
    /// `#/cluster/organization`, a cluster page and its cluster graph.
    Cluster {
        /// The slug of the cluster.
        slug: String,
    },
    /// `#/clusters`, the cluster map.
    Clusters,
    /// `#/whole/Query`, the whole graph anchored at a root, or everything
    /// under the budget with `#/whole/All`.
    Whole {
        /// The root of the whole graph.
        root: WholeRoot,
    },
    /// `#/path/User/Invoice`, the shortest path between two types.
    Path {
        /// The name of the type the path starts at.
        from: String,
        /// The name of the type the path ends at.
        to: String,
    },
    /// `#/search/orgst`, the results of a search.
    Search {
        /// The text of the search, decoded.
        text: String,
    },
    /// `#/unreachable`, the report of the types no walk reaches.
    Unreachable,
    /// `#/deprecated`, the report of the deprecated members.
    Deprecated,
    /// `#/scalars`, the page of the built in and the custom scalars.
    Scalars,
    /// `#/relay`, the page of the collapsed Relay wrapper types.
    Relay,
}

/// The root a whole graph is anchored at, one of the three operation roots
/// or all of them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WholeRoot {
    /// `#/whole/Query`.
    Query,
    /// `#/whole/Mutation`.
    Mutation,
    /// `#/whole/Subscription`.
    Subscription,
    /// `#/whole/All`, where the root tabs act as a lens.
    All,
}

/// A reason [`Route::parse`] refuses a hash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RouteError {
    /// The hash is longer than [`MAX_ROUTE_BYTES`].
    TooLong,
    /// The path names no route of the grammar, or a route with the wrong
    /// number of parts. It is written as read, without the `#` and the
    /// option string.
    Unknown(String),
    /// A part of a known route is outside its grammar, such as an empty
    /// part, a type name that is no GraphQL name or a root that is none of
    /// the three.
    BadPart {
        /// The route word, such as `type` or `op`.
        route: &'static str,
        /// The part, decoded.
        part: String,
    },
    /// The option string after the question mark is refused.
    Options(OptionError),
}

impl Route {
    /// Reads a hash into its route and its options.
    ///
    /// A leading `#` is optional, and the empty hash, `#` and `#/` are all the
    /// home page, since a browser answers the empty string for a URL whose
    /// fragment is empty or missing. The option string starts at the first `?`
    /// and is read by the pair grammar of [`Options::parse`] without its rule
    /// for a leading `?`, so a second `?` is a byte of a key or a value, which
    /// only a glob of `hide=` takes. `#/??relay=0` is refused, since its first
    /// key is `?relay`. A hash without a `?` takes the default of every option.
    ///
    /// # Errors
    ///
    /// Returns [`RouteError::TooLong`] for a hash longer than
    /// [`MAX_ROUTE_BYTES`], [`RouteError::Unknown`] for a path that is no
    /// route, [`RouteError::BadPart`] for a name, a root or a slug outside
    /// its grammar, and [`RouteError::Options`] for a refused option string.
    ///
    /// [URL standard]: https://github.com/whatwg/url/blob/8e14777cfa145b08a9fb735fe580ec0c366564c3/url.bs
    pub fn parse(hash: &str) -> Result<(Route, Options), RouteError> {
        if hash.len() > MAX_ROUTE_BYTES {
            return Err(RouteError::TooLong);
        }
        let hash = hash.strip_prefix('#').unwrap_or(hash);
        let (path, query) = match hash.split_once('?') {
            Some((path, query)) => (path, Some(query)),
            None => (hash, None),
        };
        let route = parse_path(path)?;
        let options = match query {
            Some(query) => Options::from_pairs(query).map_err(RouteError::Options)?,
            None => Options::default(),
        };
        Ok((route, options))
    }

    /// Returns the canonical hash of the route under `options`.
    ///
    /// The hash starts with `#/`, and the option string of
    /// [`Options::to_query`] follows a `?` unless every option has its
    /// default. An argument without a field is left out, because the grammar
    /// selects an argument only inside its field.
    #[must_use]
    pub fn to_hash(&self, options: &Options) -> String {
        let mut out = String::from("#/");
        self.write_path(&mut out);
        let mark = out.len();
        out.push('?');
        options.write_query(&mut out);
        if out.len() == mark + 1 {
            out.truncate(mark);
        }
        out
    }

    /// Appends the path of the route after `#/` to `out`.
    fn write_path(&self, out: &mut String) {
        match self {
            Route::Home => {}
            Route::Type { name, field, arg } => {
                out.push_str("type/");
                push_part(name, out);
                if let Some(field) = field {
                    out.push('/');
                    push_part(field, out);
                    if let Some(arg) = arg {
                        out.push('/');
                        push_part(arg, out);
                    }
                }
            }
            Route::Op { root, field } => {
                out.push_str("op/");
                out.push_str(root.word());
                out.push('/');
                push_part(field, out);
            }
            Route::Cluster { slug } => {
                out.push_str("cluster/");
                push_part(slug, out);
            }
            Route::Clusters => out.push_str("clusters"),
            Route::Whole { root } => {
                out.push_str("whole/");
                out.push_str(whole_root_word(root));
            }
            Route::Path { from, to } => {
                out.push_str("path/");
                push_part(from, out);
                out.push('/');
                push_part(to, out);
            }
            Route::Search { text } => {
                out.push_str("search/");
                push_part(text, out);
            }
            Route::Unreachable => out.push_str("unreachable"),
            Route::Deprecated => out.push_str("deprecated"),
            Route::Scalars => out.push_str("scalars"),
            Route::Relay => out.push_str("relay"),
        }
    }
}

/// Reads the path of a hash, the part before the option string.
fn parse_path(path: &str) -> Result<Route, RouteError> {
    if path.is_empty() || path == "/" {
        return Ok(Route::Home);
    }
    let unknown = || RouteError::Unknown(String::from(path));
    let rest = path.strip_prefix('/').ok_or_else(unknown)?;
    let (word, tail) = match rest.split_once('/') {
        Some((word, tail)) => (word, Some(tail)),
        None => (rest, None),
    };
    let Some(tail) = tail else {
        return match word {
            "clusters" => Ok(Route::Clusters),
            "unreachable" => Ok(Route::Unreachable),
            "deprecated" => Ok(Route::Deprecated),
            "scalars" => Ok(Route::Scalars),
            "relay" => Ok(Route::Relay),
            _ => Err(unknown()),
        };
    };
    if word == "search" {
        return Ok(Route::Search { text: decode(tail) });
    }
    let mut parts = [""; MAX_PARTS];
    let count = split_parts(tail, &mut parts).ok_or_else(unknown)?;
    match (word, count) {
        ("type", 1..=3) => {
            let name = name_part("type", parts[0])?;
            let field = (count > 1)
                .then(|| name_part("type", parts[1]))
                .transpose()?;
            let arg = (count > 2)
                .then(|| name_part("type", parts[2]))
                .transpose()?;
            Ok(Route::Type { name, field, arg })
        }
        ("op", 2) => {
            let root = decode(parts[0]);
            let root = op_root(&root).ok_or(RouteError::BadPart {
                route: "op",
                part: root,
            })?;
            let field = name_part("op", parts[1])?;
            Ok(Route::Op { root, field })
        }
        ("cluster", 1) => {
            let slug = decode(parts[0]);
            if is_slug(&slug) {
                Ok(Route::Cluster { slug })
            } else {
                Err(RouteError::BadPart {
                    route: "cluster",
                    part: slug,
                })
            }
        }
        ("whole", 1) => {
            let root = decode(parts[0]);
            let root = whole_root(&root).ok_or(RouteError::BadPart {
                route: "whole",
                part: root,
            })?;
            Ok(Route::Whole { root })
        }
        ("path", 2) => {
            let from = name_part("path", parts[0])?;
            let to = name_part("path", parts[1])?;
            Ok(Route::Path { from, to })
        }
        _ => Err(unknown()),
    }
}

/// Splits `tail` at `/` into at most [`MAX_PARTS`] parts and returns their
/// count, or `None` when it holds more.
fn split_parts<'t>(tail: &'t str, parts: &mut [&'t str; MAX_PARTS]) -> Option<usize> {
    let mut count = 0;
    for part in tail.split('/').take(MAX_PARTS + 1) {
        *parts.get_mut(count)? = part;
        count += 1;
    }
    Some(count)
}

/// Returns the part `part` of the route `route` decoded, if it is a name.
fn name_part(route: &'static str, part: &str) -> Result<String, RouteError> {
    let part = decode(part);
    if is_name(&part) {
        Ok(part)
    } else {
        Err(RouteError::BadPart { route, part })
    }
}

/// Returns whether `text` matches `[_A-Za-z][_0-9A-Za-z]*`, the grammar of a
/// name.
///
/// [specification]: https://spec.graphql.org/October2021/
fn is_name(text: &str) -> bool {
    let mut bytes = text.bytes();
    bytes
        .next()
        .is_some_and(|b| b == b'_' || b.is_ascii_alphabetic())
        && bytes.all(|b| b == b'_' || b.is_ascii_alphanumeric())
}

/// Returns whether `text` can be a cluster slug, a nonempty run of `[a-z0-9]`
/// and hyphens, which every slug of a cluster is.
fn is_slug(text: &str) -> bool {
    !text.is_empty()
        && text
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Returns the root an operation route names in lower case.
fn op_root(word: &str) -> Option<RootKind> {
    match word {
        "query" => Some(RootKind::Query),
        "mutation" => Some(RootKind::Mutation),
        "subscription" => Some(RootKind::Subscription),
        _ => None,
    }
}

/// Returns the root a whole graph route names.
fn whole_root(word: &str) -> Option<WholeRoot> {
    match word {
        "Query" => Some(WholeRoot::Query),
        "Mutation" => Some(WholeRoot::Mutation),
        "Subscription" => Some(WholeRoot::Subscription),
        "All" => Some(WholeRoot::All),
        _ => None,
    }
}

/// Returns the word a whole graph route writes for `root`.
fn whole_root_word(root: &WholeRoot) -> &'static str {
    match root {
        WholeRoot::Query => "Query",
        WholeRoot::Mutation => "Mutation",
        WholeRoot::Subscription => "Subscription",
        WholeRoot::All => "All",
    }
}

/// Returns whether `b` is a `pchar` of RFC 3986 other than a percent
/// encoding, an unreserved byte, a sub delimiter, `:` or `@`.
fn is_pchar(b: u8) -> bool {
    b.is_ascii_alphanumeric()
        || matches!(
            b,
            b'-' | b'.'
                | b'_'
                | b'~'
                | b'!'
                | b'$'
                | b'&'
                | b'\''
                | b'('
                | b')'
                | b'*'
                | b'+'
                | b','
                | b';'
                | b'='
                | b':'
                | b'@'
        )
}

/// Appends `text` to `out` as one segment of a path, every byte that is no
/// [`is_pchar`] byte percent encoded.
fn push_part(text: &str, out: &mut String) {
    push_encoded(text, is_pchar, out);
}

/// Appends `text` to `out` with every byte `keep` refuses percent encoded
/// with upper case hex digits.
fn push_encoded(text: &str, keep: fn(u8) -> bool, out: &mut String) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for b in text.bytes() {
        if keep(b) {
            out.push(char::from(b));
        } else {
            out.push('%');
            out.push(char::from(HEX[usize::from(b >> 4)]));
            out.push(char::from(HEX[usize::from(b & 0x0f)]));
        }
    }
}

/// Returns `text` percent decoded and read as UTF-8.
///
/// A `%` that two hex digits do not follow is kept as it is, and a malformed
/// UTF-8 sequence becomes U+FFFD, as the URL standard decodes.
fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    if !bytes.contains(&b'%') {
        return String::from(text);
    }
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    // Each iteration moves `i` forward by one or three bytes, so the loop
    // runs at most once per byte of `text`.
    while i < bytes.len() {
        if let Some(b) = escaped(bytes, i) {
            out.push(b);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Returns the byte a `%` and two hex digits at `bytes[i..]` encode.
fn escaped(bytes: &[u8], i: usize) -> Option<u8> {
    if bytes.get(i) != Some(&b'%') {
        return None;
    }
    let high = hex(*bytes.get(i + 1)?)?;
    let low = hex(*bytes.get(i + 2)?)?;
    Some((high << 4) | low)
}

/// Returns the value of the hex digit `b`.
fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        b'a'..=b'f' => Some(b - b'a' + 10),
        _ => None,
    }
}
