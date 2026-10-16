// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool viewer: the seam of the shared core of aoviewer and of the
 * documentation generator.
 *
 * No analogue in the reference. The crates under test are aoviewer-core and
 * aoviewer-docs, and the judging notebooks are 51_viewer_core, 75_docgen and
 * 76_docs_pdf.
 */

//! The `viewer` primitive of the seam.
//!
//! The flag `--requests PATH` answers every request line of the file, one
//! response line each, in the batch form of [`crate::batch`], so a notebook
//! drives thousands of cases through one process. Each module below carries
//! one operation, but `text.rs` carries `md` and `typ` and `sha256.rs`
//! carries `json-str` beside `sha256`.
//!
//! - `schema {input, form}` answers the input kind, the SHA-256 of the
//!   canonical introspection form, the counts of the model and, unless `form`
//!   is false, the form itself.
//! - `graph {input, options, root}` answers the type graph in the Voyager
//!   form.
//! - `walks {input, options}` answers the counts and the orphans of the three
//!   walks and the types reached only as an implementation.
//! - `clusters {input, options, declared}` answers both automatic clusterings
//!   with their shares and the pick, and the clustering a declared map gives.
//! - `view {input, route, routes, budget, declared}` answers the nodes with
//!   their levels, the edges, the chips, the reductions and the parts of the
//!   view of each route.
//! - `svg {input, route, routes, themes, budget, declared, svg_text, title}`
//!   answers the SVG of each part of each route in each theme with the
//!   iterations of its layout.
//! - `page-model {input, route, routes, declared}` answers the page model of
//!   each route.
//! - `md {input, options, route, path}` and `typ {...}` answer the bytes the
//!   generator writes for a route or a path of the tree, and `md {device}`
//!   and `typ {device}` the two files of a device chapter.
//! - `tree {input, options, since, out}` answers the whole tree and writes it
//!   into `out`.
//! - `random {seed, count, options}` answers the iterations of the layouts of
//!   each random graph against the caps.
//! - `commonmark {texts}` answers `to_html` of each text.
//! - `sha256 {hex}` answers the core's digest of each byte string, and
//!   `json-str {texts}` the JSON string `json::write_str` writes for each.
//! - `bench {input, routes, repetitions, budget, declared}` answers the time
//!   of each call in nanoseconds and the peak resident set, the one clock of
//!   the primitive.
//!
//! An input path is read through [`aoviewer_docs::input::load_path`], which
//! names its kind as `input/detect.rs` does, so the seam and the command
//! share one reader. The loader refuses a file longer than the readers take
//! by its length before its bytes are held, so a hostile document of 64 MiB
//! costs no memory of its size.
//!
//! A field that is null is absent in every request, so a request that writes
//! out an optional field as null takes its default. An input the generator
//! refuses answers `refused`, its code of the family `AO-D`, and `message`,
//! the sentence of the generator, in place of the payload. A request the seam
//! cannot read, a missing field or a route outside the grammar among them,
//! answers `fatal`.

mod bench;
mod clusters;
mod commonmark;
mod graph;
mod page_model;
mod random;
mod schema;
mod sha256;
mod svg;
mod text;
mod tree;
mod view;
mod walks;
mod wasm;

use std::{
    path::PathBuf,
    process::ExitCode, //
};

use aosabi_json::{
    JsObject,
    JsString,
    JsValue, //
};
use aoviewer_core::{
    cluster::{
        ClusterSource,
        Clustering,
        Declared, //
    },
    graph::NodeId,
    layout::{
        EgoLayout,
        Layout as _,
        LayoutError,
        WholeLayout, //
    },
    limits::ReadLimits,
    model::{
        Model,
        TypeId, //
    },
    route::{
        MAX_RING,
        OptionError,
        Options,
        Route,
        RouteError, //
    },
    scene::Scene,
    schema::Schema,
    transform::Transforms,
    tree::TreeError,
    view::{
        Budget,
        EmptyReason,
        View,
        ViewError, //
    }, //
};
use aoviewer_docs::{
    error::DocsError,
    input::{
        Loaded,
        load_path, //
    }, //
};

use crate::{
    batch::{
        self,
        Operations, //
    },
    cli::{
        self,
        Grammar,
        Matches, //
    },
    homa_packet::whole,
    seam::{
        self,
        Failure, //
    }, //
};

/// The name of the primitive.
const NAME: &str = "viewer";

/// The milestone the operations of the primitive land at.
const LANDS_AT: &str = "M14";

/// The largest integer n such that n and n + 1 are both exact doubles, which
/// is 2^53 less 1, the largest whole number [`crate::homa_packet::whole`]
/// reads.
pub(super) const MAX_SAFE_INTEGER: u64 = (1 << 53) - 1;

/// The most routes, texts, byte strings or repetitions one request names.
///
/// The cap keeps one request line from asking for unbounded work, and it is
/// ten times the 10,000 strings of step 9 of `51_viewer_core`.
pub(super) const MAX_ITEMS: usize = 100_000;

/// The command line of the `viewer` primitive.
pub static GRAMMAR: Grammar = Grammar {
    options: &[cli::SMOKE, cli::REQUESTS],
    positional: None,
    conflicts: &[],
};

/// The operations of the batch form, by the `op` that names each.
static OPERATIONS: &Operations = &[
    ("schema", |request| answer(schema::run, request)),
    ("graph", |request| answer(graph::run, request)),
    ("walks", |request| answer(walks::run, request)),
    ("clusters", |request| answer(clusters::run, request)),
    ("view", |request| answer(view::run, request)),
    ("svg", |request| answer(svg::run, request)),
    ("page-model", |request| answer(page_model::run, request)),
    ("md", |request| answer(text::run_md, request)),
    ("typ", |request| answer(text::run_typ, request)),
    ("tree", |request| answer(tree::run, request)),
    ("random", |request| answer(random::run, request)),
    ("commonmark", |request| answer(commonmark::run, request)),
    ("sha256", |request| answer(sha256::run, request)),
    ("json-str", |request| answer(sha256::run_json_str, request)),
    ("bench", |request| answer(bench::run, request)),
];

/// What the `viewer` primitive accepts.
#[derive(Debug)]
pub struct ViewerArgs {
    /// Whether the call asks for the interface line.
    smoke: bool,
    /// The file of request lines of the batch form.
    requests: Option<PathBuf>,
}

impl ViewerArgs {
    /// Reads the arguments `matches` holds for [`GRAMMAR`].
    pub fn read(matches: &Matches) -> Self {
        Self {
            smoke: matches.flag("smoke"),
            requests: matches.path("requests"),
        }
    }
}

/// Runs the `viewer` primitive.
///
/// Under `--smoke` it prints the interface line and exits 0. With
/// `--requests PATH` it answers every request line of the file. Any other
/// call exits 64, the usage code of `sysexits.h`.
pub fn run(args: &ViewerArgs) -> ExitCode {
    if args.smoke {
        return seam::smoke_at(NAME, LANDS_AT);
    }
    let outcome = match &args.requests {
        Some(path) => batch::run(OPERATIONS, path),
        None => Err(Failure::Usage(String::from("the call names no --requests"))),
    };
    seam::finish(NAME, outcome)
}

/// Why an operation answers no payload of its own.
#[derive(Debug)]
pub(super) enum Stop {
    /// The request cannot be read, which the batch form answers as `fatal`.
    Fatal(String),
    /// The generator refuses the input or a value, with its code and its
    /// sentence.
    Refused {
        /// The code, such as `AO-D0002`.
        code: &'static str,
        /// The sentence the generator prints, its code first.
        message: String,
    },
}

impl From<DocsError> for Stop {
    fn from(e: DocsError) -> Self {
        Stop::Refused {
            code: e.code(),
            message: e.to_string(),
        }
    }
}

/// The function of one operation.
type Run = fn(&JsObject) -> Result<JsObject, Stop>;

/// Answers `request` with `op`, a refusal as its payload and a request that
/// cannot be read as the `fatal` of the batch form.
fn answer(op: Run, request: &JsObject) -> Result<JsObject, JsString> {
    match op(request) {
        Ok(payload) => Ok(payload),
        Err(Stop::Fatal(text)) => Err(JsString::from(text)),
        Err(Stop::Refused { code, message }) => Ok(refused(code, message)),
    }
}

/// Returns the payload of a refusal, `refused` with the code and `message`.
pub(super) fn refused(code: &str, message: String) -> JsObject {
    let mut payload = JsObject::with_capacity(2);
    payload.insert("refused", code);
    payload.insert("message", message);
    payload
}

/// Returns the payload of a refusal of the core, as the generator words it.
pub(super) fn refused_by(e: &DocsError) -> JsObject {
    refused(e.code(), e.to_string())
}

/// Returns the `fatal` of `request` with `text`, after the name of its
/// operation.
pub(super) fn fatal(request: &JsObject, text: &str) -> Stop {
    let op = match request.get("op") {
        Some(JsValue::String(op)) => op.as_str().unwrap_or("?"),
        _ => "?",
    };
    Stop::Fatal(format!("{op}: {text}"))
}

/// Returns the string field `name` of `request`.
///
/// # Errors
///
/// Returns [`Stop::Fatal`] when the field is missing, is no string or holds
/// a lone surrogate.
pub(super) fn string<'r>(request: &'r JsObject, name: &str) -> Result<&'r str, Stop> {
    batch::require_string(request, name).map_err(Stop::Fatal)
}

/// Returns whether `request` carries the field `name` with a value other
/// than null.
///
/// A field that is null is absent, the rule every reader of a field below
/// follows, so a request that writes out an optional field as null asks for
/// its default.
pub(super) fn present(request: &JsObject, name: &str) -> bool {
    !matches!(
        request.get(name),
        None | Some(JsValue::Undefined | JsValue::Null)
    )
}

/// Returns the string field `name` of `request`, or `None` when it is
/// absent or null.
///
/// # Errors
///
/// Returns [`Stop::Fatal`] when the field is no string.
pub(super) fn optional<'r>(request: &'r JsObject, name: &str) -> Result<Option<&'r str>, Stop> {
    if present(request, name) {
        string(request, name).map(Some)
    } else {
        Ok(None)
    }
}

/// Returns the boolean field `name` of `request`, or `default` when it is
/// absent.
///
/// # Errors
///
/// Returns [`Stop::Fatal`] when the field is no boolean.
pub(super) fn flag(request: &JsObject, name: &str, default: bool) -> Result<bool, Stop> {
    match request.get(name) {
        None | Some(JsValue::Undefined | JsValue::Null) => Ok(default),
        Some(JsValue::Bool(value)) => Ok(*value),
        Some(_) => Err(fatal(request, &format!("{name} is no boolean"))),
    }
}

/// Returns the field `name` of `request` as a whole number of at most
/// `most`, or `default` when it is absent.
///
/// A number above [`MAX_SAFE_INTEGER`] has no exact double, so `most` is
/// read as that bound at most.
///
/// # Errors
///
/// Returns [`Stop::Fatal`] when the field is no whole number from 0 to
/// `most`.
pub(super) fn number(request: &JsObject, name: &str, default: u64, most: u64) -> Result<u64, Stop> {
    let most = most.min(MAX_SAFE_INTEGER);
    match request.get(name) {
        None | Some(JsValue::Undefined | JsValue::Null) => Ok(default),
        Some(value @ JsValue::Number(_)) => whole(value).filter(|&n| n <= most).ok_or_else(|| {
            fatal(
                request,
                &format!("{name} is no whole number from 0 to {most}"),
            )
        }),
        Some(_) => Err(fatal(request, &format!("{name} is no number"))),
    }
}

/// Returns the strings of the array field `name` of `request`, at most
/// [`MAX_ITEMS`] of them.
///
/// # Errors
///
/// Returns [`Stop::Fatal`] when the field is missing, is no array, holds
/// more than [`MAX_ITEMS`] items or holds an item that is no string or that
/// carries a lone surrogate.
pub(super) fn strings<'r>(request: &'r JsObject, name: &str) -> Result<Vec<&'r str>, Stop> {
    let Some(JsValue::Array(items)) = request.get(name) else {
        return Err(fatal(request, &format!("{name} is no array")));
    };
    if items.len() > MAX_ITEMS {
        return Err(fatal(
            request,
            &format!("{name} holds more than {MAX_ITEMS} items"),
        ));
    }
    let mut out = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        let text = match item {
            JsValue::String(text) => text.as_str(),
            _ => None,
        };
        let text = text.ok_or_else(|| {
            fatal(
                request,
                &format!("{name}[{i}] is no string a Rust string can hold"),
            )
        })?;
        out.push(text);
    }
    Ok(out)
}

/// Returns the path the string field `name` of `request` names.
///
/// # Errors
///
/// Returns [`Stop::Fatal`] when the field is missing or is no string.
pub(super) fn path(request: &JsObject, name: &str) -> Result<PathBuf, Stop> {
    string(request, name).map(PathBuf::from)
}

/// Reads the input the field `input` of `request` names under
/// [`ReadLimits::DEFAULT`].
///
/// # Errors
///
/// Returns [`Stop::Fatal`] for a request without `input` and
/// [`Stop::Refused`] for an input the loader refuses.
pub(super) fn load(request: &JsObject) -> Result<Loaded, Stop> {
    let path = path(request, "input")?;
    Ok(load_path(&path, &ReadLimits::DEFAULT)?)
}

/// Returns the options of the option string `options` of `request`, the
/// defaults when it is absent.
///
/// # Errors
///
/// Returns [`Stop::Fatal`] when the field is no string or the option string
/// is refused.
pub(super) fn view_options(request: &JsObject) -> Result<Options, Stop> {
    match optional(request, "options")? {
        Some(query) => Options::parse(query)
            .map_err(|e| fatal(request, &format!("options is refused, {}", option_text(&e)))),
        None => Ok(Options::default()),
    }
}

/// Returns why an option string is refused, in one clause.
pub(super) fn option_text(e: &OptionError) -> String {
    match e {
        OptionError::UnknownKey(key) => format!("the key {key:?} is unknown"),
        OptionError::BadValue { key, value } => {
            format!("the key {key} takes no value {value:?} or appears twice")
        }
        OptionError::TooLong => String::from("the string is longer than the grammar reads"),
    }
}

/// One route of a request, its text, the route and its options.
pub(super) struct Asked {
    /// The hash as the request gives it.
    pub(super) hash: String,
    /// The route.
    pub(super) route: Route,
    /// The options of the hash.
    pub(super) options: Options,
}

/// Returns the routes of `request`, the hash of `route` alone or every hash
/// of `routes`, a field that is null being absent.
///
/// # Errors
///
/// Returns [`Stop::Fatal`] when neither field is given, both are, or a hash
/// is refused.
pub(super) fn routes(request: &JsObject) -> Result<Vec<Asked>, Stop> {
    let hashes = match (optional(request, "route")?, present(request, "routes")) {
        (Some(hash), false) => vec![hash],
        (None, true) => strings(request, "routes")?,
        _ => {
            return Err(fatal(
                request,
                "the request names no route or both route and routes",
            ));
        }
    };
    hashes
        .into_iter()
        .map(|hash| route(request, hash))
        .collect()
}

/// Reads the hash `hash` of `request` into its route and options.
///
/// # Errors
///
/// Returns [`Stop::Fatal`] when the hash is refused.
pub(super) fn route(request: &JsObject, hash: &str) -> Result<Asked, Stop> {
    let (route, options) = Route::parse(hash).map_err(|e| {
        let reason = match e {
            RouteError::TooLong => String::from("is longer than the grammar reads"),
            RouteError::Unknown(path) => format!("names no route, {path:?}"),
            RouteError::BadPart { route, part } => {
                format!("holds {part:?}, which the route {route} does not take")
            }
            RouteError::Options(e) => {
                format!("carries options that are refused, {}", option_text(&e))
            }
        };
        fatal(request, &format!("the route {hash:?} {reason}"))
    })?;
    Ok(Asked {
        hash: String::from(hash),
        route,
        options,
    })
}

/// Returns the declared clusters of the array field `declared` of
/// `request`, each `{name, globs}`, none when it is absent.
///
/// # Errors
///
/// Returns [`Stop::Fatal`] when the field is no array of such objects,
/// when it holds more than [`MAX_ITEMS`] of them or when the globs of one
/// hold more than [`MAX_ITEMS`] items.
pub(super) fn declared(request: &JsObject) -> Result<Vec<Declared>, Stop> {
    let items = match request.get("declared") {
        None | Some(JsValue::Undefined | JsValue::Null) => return Ok(Vec::new()),
        Some(JsValue::Array(items)) if items.len() > MAX_ITEMS => {
            let text = format!("declared holds more than {MAX_ITEMS} items");
            return Err(fatal(request, &text));
        }
        Some(JsValue::Array(items)) => items,
        Some(_) => return Err(fatal(request, "declared is no array")),
    };
    let mut out = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        let JsValue::Object(item) = item else {
            return Err(fatal(request, &format!("declared[{i}] is no object")));
        };
        let name = string(item, "name")
            .map_err(|_| fatal(request, &format!("declared[{i}] has no name")))?;
        if let Some(JsValue::Array(globs)) = item.get("globs")
            && globs.len() > MAX_ITEMS
        {
            let text = format!("declared[{i}].globs holds more than {MAX_ITEMS} items");
            return Err(fatal(request, &text));
        }
        let globs = strings(item, "globs")
            .map_err(|_| fatal(request, &format!("declared[{i}] has no array of globs")))?;
        out.push(Declared {
            name: String::from(name),
            globs: globs.into_iter().map(String::from).collect(),
        });
    }
    Ok(out)
}

/// An item of a request that names one route.
pub(super) trait Routed {
    /// Returns the route of the item.
    fn asked(&self) -> &Asked;
}

impl Routed for Asked {
    fn asked(&self) -> &Asked {
        self
    }
}

impl Routed for (Asked, Plan) {
    fn asked(&self) -> &Asked {
        &self.0
    }
}

/// The schema of one model under one set of transforms with its picked
/// clustering, which the routes of a request with those transforms share.
pub(super) struct Schemas {
    /// The schema, which holds the model.
    schema: Schema,
    /// The clustering [`Clustering::pick`] answers for the schema.
    clustering: Clustering,
    /// The declared clusters every clustering takes.
    declared: Vec<Declared>,
}

impl Schemas {
    /// Builds the schema of `model` under `transforms` and its clustering,
    /// which takes `declared`.
    ///
    /// The model moves into the schema, as the generator builds its one
    /// schema from the model it loads, so a build copies no model.
    pub(super) fn new(model: Model, transforms: &Transforms, declared: Vec<Declared>) -> Schemas {
        let schema = Schema::build(model, transforms.clone());
        let clustering = Clustering::pick(&schema, &declared);
        Schemas {
            schema,
            clustering,
            declared,
        }
    }

    /// Returns the schema.
    pub(super) fn schema(&self) -> &Schema {
        &self.schema
    }

    /// Returns these schemas under `transforms`, the same when they were
    /// built under them, and otherwise built anew from the model the old
    /// schema hands on, which the transforms never change.
    fn under(self, transforms: &Transforms) -> Schemas {
        if self.schema.transforms == *transforms {
            return self;
        }
        Schemas::new(self.schema.model, transforms, self.declared)
    }

    /// Answers `each` for every item of `items` with the schema and the
    /// clustering of the transforms of its route, in the order of `items`.
    ///
    /// The items run grouped by those transforms, the groups in the order
    /// their first item stands in `items`, so the schema of each set of
    /// transforms is built once, and the first group takes the schema these
    /// schemas hold when they were built under its transforms.
    pub(super) fn each<R: Routed, T>(
        self,
        items: &[R],
        mut each: impl FnMut(&Schema, &Clustering, &R) -> T,
    ) -> Vec<T> {
        let mut held = self;
        let mut out = Vec::with_capacity(items.len());
        for group in groups(items) {
            let Some(&(_, first)) = group.first() else {
                continue;
            };
            held = held.under(&first.asked().options.transforms);
            for (i, item) in group {
                out.push((i, each(&held.schema, &held.clustering, item)));
            }
        }
        out.sort_unstable_by_key(|&(i, _)| i);
        out.into_iter().map(|(_, answer)| answer).collect()
    }
}

/// Answers `each` for every item of `items` through [`Schemas::each`], the
/// schemas built from `model` with `declared`, and answers nothing for no
/// item.
pub(super) fn each_route<R: Routed, T>(
    model: Model,
    declared: Vec<Declared>,
    items: &[R],
    each: impl FnMut(&Schema, &Clustering, &R) -> T,
) -> Vec<T> {
    match items.first() {
        Some(first) => {
            Schemas::new(model, &first.asked().options.transforms, declared).each(items, each)
        }
        None => Vec::new(),
    }
}

/// Returns the items of `items` with their indices, grouped by the
/// transforms of their routes, the groups in the order their first item
/// stands in `items` and the items of a group in that order too.
///
/// The key of a group is the canonical option string of the transforms alone,
/// which equal transforms write alike. [`Schemas::under`] compares the
/// transforms themselves, so two sets that shared a key would still each be
/// drawn from a schema of their own.
fn groups<R: Routed>(items: &[R]) -> Vec<Vec<(usize, &R)>> {
    let mut keyed: Vec<(String, usize, &R)> = items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let options = Options {
                transforms: item.asked().options.transforms.clone(),
                ..Options::default()
            };
            (options.to_query(), i, item)
        })
        .collect();
    keyed.sort_unstable_by(|a, b| (&a.0, a.1).cmp(&(&b.0, b.1)));
    let mut groups: Vec<Vec<(usize, &R)>> = keyed
        .chunk_by(|a, b| a.0 == b.0)
        .map(|group| group.iter().map(|&(_, i, item)| (i, item)).collect())
        .collect();
    groups.sort_unstable_by_key(|group| group.first().map(|&(i, _)| i));
    groups
}

/// The budget a request lays its views out under.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Plan {
    /// The page budget of the generator, reduced and split into parts by
    /// [`View::for_page`].
    Page,
    /// [`Budget::WHOLE`], the budget of the whole graph and of the poster.
    Whole,
    /// A budget that admits every view the core builds, the one of the
    /// termination case.
    Open,
}

/// The budget of [`Plan::Open`]: every count the fields take, no width and
/// the largest ring, so the options decide the ring.
pub(super) const OPEN: Budget = Budget {
    boxes: u16::MAX,
    edges: u16::MAX,
    width_cp: i32::MAX,
    ring: MAX_RING,
};

/// Returns the plan the field `budget` of `request` names, or the one the
/// generator takes for `route`: the whole graph under [`Budget::WHOLE`], as
/// the poster, and every other route under the page budget.
///
/// # Errors
///
/// Returns [`Stop::Fatal`] for a word other than `page`, `whole` and `open`.
pub(super) fn plan(request: &JsObject, route: &Route) -> Result<Plan, Stop> {
    match optional(request, "budget")? {
        None if matches!(route, Route::Whole { .. }) => Ok(Plan::Whole),
        None | Some("page") => Ok(Plan::Page),
        Some("whole") => Ok(Plan::Whole),
        Some("open") => Ok(Plan::Open),
        Some(other) => Err(fatal(
            request,
            &format!("budget takes page, whole or open, not {other:?}"),
        )),
    }
}

/// Returns every route of `asked` with the plan [`plan`] gives it.
///
/// # Errors
///
/// Returns [`Stop::Fatal`] for a word of `budget` other than `page`,
/// `whole` and `open`.
pub(super) fn planned(request: &JsObject, asked: Vec<Asked>) -> Result<Vec<(Asked, Plan)>, Stop> {
    asked
        .into_iter()
        .map(|a| {
            let budget = plan(request, &a.route)?;
            Ok((a, budget))
        })
        .collect()
}

/// Returns the views of `route` under `options` and `plan`, the parts of a
/// page or one view.
///
/// # Errors
///
/// Returns the [`ViewError`] of the core.
pub(super) fn views(
    schema: &Schema,
    clustering: &Clustering,
    asked: &Asked,
    plan: Plan,
) -> Result<Vec<View>, ViewError> {
    let (route, options) = (&asked.route, &asked.options);
    match plan {
        Plan::Page => View::for_page(schema, clustering, route, options),
        Plan::Whole => {
            View::of(schema, clustering, route, options, &Budget::WHOLE).map(|v| vec![v])
        }
        Plan::Open => View::of(schema, clustering, route, options, &OPEN).map(|v| vec![v]),
    }
}

/// Returns the scene of `view` of the route `route`, drawn by the layout of
/// its kind with no seed.
///
/// The core draws a view of the whole graph with [`WholeLayout`] and every
/// other view with [`EgoLayout`], and both draw the cluster map alike. The
/// whole graph is the view of a route to the whole graph and the view of the
/// home page whenever it is no cluster map, so the route decides the layout
/// as the kind of its view does.
///
/// # Errors
///
/// Returns the [`LayoutError`] of the layout.
pub(super) fn scene(view: &View, schema: &Schema, route: &Route) -> Result<Scene, LayoutError> {
    if matches!(route, Route::Whole { .. } | Route::Home) {
        WholeLayout.layout(view, schema, None)
    } else {
        EgoLayout.layout(view, schema, None)
    }
}

/// Returns the refusal of the generator for a view the core refuses.
pub(super) fn view_refused(e: ViewError) -> DocsError {
    DocsError::Tree(TreeError::View(e))
}

/// Returns the refusal of the generator for a layout that answers no scene.
pub(super) fn layout_refused(e: LayoutError) -> DocsError {
    DocsError::Tree(TreeError::Layout(e))
}

/// Returns the name of the type of the node `node` of the graph of
/// `schema`.
pub(super) fn node_name(schema: &Schema, node: NodeId) -> &str {
    type_name(&schema.model, schema.graph.nodes[node.index()].ty)
}

/// Returns the name of the type `ty` of `model`.
pub(super) fn type_name(model: &Model, ty: TypeId) -> &str {
    model.name(model.ty(ty).name)
}

/// Returns why a view draws nothing as JSON: `reason` and, over a budget,
/// the boxes and edges it would draw.
pub(super) fn empty_json(reason: EmptyReason) -> JsValue {
    let mut out = JsObject::with_capacity(3);
    match reason {
        EmptyReason::Unreachable => {
            out.insert("reason", "unreachable");
        }
        EmptyReason::FewerThanTwoNeighbours => {
            out.insert("reason", "fewer_than_two_neighbours");
        }
        EmptyReason::OverBudget { boxes, edges } => {
            out.insert("reason", "over_budget");
            out.insert("boxes", boxes);
            out.insert("edges", edges);
        }
    }
    JsValue::Object(out)
}

/// Returns the word of a clustering source, the word of the table of
/// [`Clustering::table`].
pub(super) fn source_word(source: ClusterSource) -> &'static str {
    match source {
        ClusterSource::Declared => "declared",
        ClusterSource::Stems => "stems",
        ClusterSource::Territories => "territories",
        ClusterSource::TerritoriesFallback => "territories-fallback",
    }
}

/// Returns the names of `types` of `model` as a JSON array.
pub(super) fn names(model: &Model, types: &[TypeId]) -> JsValue {
    JsValue::Array(
        types
            .iter()
            .map(|&t| JsValue::from(type_name(model, t)))
            .collect(),
    )
}
