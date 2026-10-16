// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool viewer text: the operations md and typ, the Markdown and the
 * Typst the generator writes for a route, a path of the tree or a device
 * chapter.
 *
 * Every file comes from render_job of the core, so the operation answers the
 * bytes the tree holds at that path. The device chapter is the hand written
 * fixture of 75_docgen step 15, read in the field names of DeviceChapter.
 */

//! The `md` and `typ` operations.
//!
//! The request `{input, options, route}` or `{input, options, path}` answers
//! `path`, the file of the tree, and `text`, its bytes as [`render_job`]
//! writes them for the tree of the input under the tree options `options`,
//! the query form of [`DocsOptions`]. A path names a job of the plan of the
//! tree. A route names the file that holds its page: for `md` the page under
//! `md/`, or the index file for the home page and for every page when the
//! layout writes all of them into one, and for `typ` the chapter that holds
//! the page, by the rule the plan of the tree places the pages by. The page
//! of a type route is the page that documents the type, so a built in scalar
//! answers the scalars page and a Relay wrapper the Relay page. A route
//! carries no options of its own, since the tree takes them from `options`.
//!
//! The request `{device}` answers `text`, the device chapter of the JSON file
//! `device` through `render_device_markdown` or `render_device_typst` of the
//! generator, `md/device.md` and `typ/05-device.typ`. The mapping from the
//! JSON to a [`DeviceChapter`] is the seam's, since the fixture is the shape
//! of the chapter and not of a card. An enum is an object whose `kind` holds
//! the variant in lower case beside its fields, every member is required, a
//! key the chapter does not name is refused, and the thumbnail is null or its
//! bytes in base 16.
//!
//! A request names exactly one of `route`, `path` and `device`, and one that
//! names none or more than one is fatal.

use std::{
    fs,
    path::Path, //
};

use aosabi_json::{
    JsObject,
    JsValue, //
};
use aoviewer_core::{
    cluster::Clustering,
    limits::ReadLimits,
    model::Model,
    page::page_of_type,
    route::{
        Options,
        Route, //
    },
    schema::Schema,
    tree::{
        Chapter,
        DocsOptions,
        Job,
        JobKind,
        plan_tree,
        render_job, //
    }, //
};
use aoviewer_docs::{
    device::{
        BlueprintTable,
        DeviceChapter,
        Fact,
        KnowledgePage,
        LinkedRow,
        Mark,
        PayloadRow,
        RanRow,
        SignatureStatus,
        render_device_markdown,
        render_device_typst, //
    },
    error::DocsError, //
};

use super::{
    MAX_ITEMS,
    Stop,
    fatal,
    load,
    option_text,
    optional,
    route, //
};
use crate::{
    homa_packet::{
        unhex,
        whole, //
    },
    json::reader, //
};

/// The format an operation answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Format {
    /// The Markdown of `md`.
    Markdown,
    /// The Typst of `typ`.
    Typst,
}

/// Answers the operation `md`.
pub(super) fn run_md(request: &JsObject) -> Result<JsObject, Stop> {
    run(request, Format::Markdown)
}

/// Answers the operation `typ`.
pub(super) fn run_typ(request: &JsObject) -> Result<JsObject, Stop> {
    run(request, Format::Typst)
}

/// The fatal of a request that names none or more than one of `route`,
/// `path` and `device`.
const NOT_ONE: &str = "the request names no route, path or device, or more than one";

/// Answers `md` or `typ` in `format`.
fn run(request: &JsObject, format: Format) -> Result<JsObject, Stop> {
    let (route_hash, path) = (optional(request, "route")?, optional(request, "path")?);
    if let Some(device) = optional(request, "device")? {
        if route_hash.is_some() || path.is_some() {
            return Err(fatal(request, NOT_ONE));
        }
        return chapter(request, Path::new(device), format);
    }
    let options = docs_options(request)?;
    let target = match (route_hash, path) {
        (Some(hash), None) => {
            let asked = route(request, hash)?;
            if asked.options != Options::default() {
                let text = "the route carries options, which the tree takes from options alone";
                return Err(fatal(request, text));
            }
            Target::Route(asked.route)
        }
        (None, Some(path)) => Target::Path(String::from(path)),
        _ => return Err(fatal(request, NOT_ONE)),
    };
    let loaded = load(request)?;
    let schema = Schema::build(loaded.model, options.view.transforms.clone());
    let clustering = Clustering::pick(&schema, &options.declared);
    let plan = plan_tree(&schema, &clustering, &options);
    let job = match &target {
        Target::Path(path) => plan.jobs.iter().find(|j| j.path == *path),
        Target::Route(route) => {
            let kind = job_of(&schema, &clustering, route, format);
            plan.jobs
                .iter()
                .find(|j| Some(&j.kind) == kind.as_ref())
                .or_else(|| index_of(&plan.jobs, &schema, &clustering, route, format))
        }
    };
    let Some(job) = job.filter(|j| writes(&j.kind, format)) else {
        return Err(fatal(
            request,
            "the tree writes no file of that route or path in this format",
        ));
    };
    let mut bytes = Vec::new();
    render_job(&schema, &clustering, &options, job, &mut bytes)
        .map_err(|e| Stop::from(DocsError::Tree(e)))?;
    let mut payload = JsObject::with_capacity(2);
    payload.insert("path", job.path.as_str());
    payload.insert("text", String::from_utf8_lossy(&bytes).into_owned());
    Ok(payload)
}

/// What a request of `md` or `typ` names.
enum Target {
    /// The file that holds the page of a route.
    Route(Route),
    /// A path of the tree.
    Path(String),
}

/// Returns the tree options of the field `options` of `request`, the
/// defaults when it is absent.
///
/// # Errors
///
/// Returns [`Stop::Fatal`] when the query is refused.
fn docs_options(request: &JsObject) -> Result<DocsOptions, Stop> {
    match optional(request, "options")? {
        Some(query) => DocsOptions::parse(query)
            .map_err(|e| fatal(request, &format!("options is refused, {}", option_text(&e)))),
        None => Ok(DocsOptions::default()),
    }
}

/// Returns the job that writes the page of `route` in `format`, or `None`
/// for a route no page answers.
///
/// A type route answers the page that documents its type. The Markdown page
/// is the job of that route. The Typst chapter follows the order of the pages
/// of the plan: the home page in the overview, a root in its chapter, a type
/// in the chapter of its cluster or of the types no cluster holds, and each
/// report page in its own.
fn job_of(
    schema: &Schema,
    clustering: &Clustering,
    route: &Route,
    format: Format,
) -> Option<JobKind> {
    let model = &schema.model;
    let page = match route {
        Route::Type { name, .. } => page_of_type(schema, model.type_by_name(name)?)?,
        other => other.clone(),
    };
    if format == Format::Markdown {
        return Some(JobKind::Page { route: page });
    }
    let chapter = match &page {
        Route::Home => Chapter::Overview,
        Route::Type { name, .. } => type_chapter(model, clustering, name)?,
        Route::Cluster { slug } => {
            let i = clustering.clusters.iter().position(|c| c.slug == *slug)?;
            Chapter::Cluster(u32::try_from(i).ok()?)
        }
        Route::Relay => Chapter::Relay,
        Route::Scalars => Chapter::Scalars,
        Route::Unreachable => Chapter::Unreachable,
        Route::Deprecated => Chapter::Deprecated,
        _ => return None,
    };
    Some(JobKind::TypstChapter(chapter))
}

/// Returns the chapter of the page of the type called `name`: its root's,
/// its cluster's, or the one of the types no cluster holds.
fn type_chapter(model: &Model, clustering: &Clustering, name: &str) -> Option<Chapter> {
    let ty = model.type_by_name(name)?;
    if let Some(root) = model.roots().kind_of(ty) {
        return Some(Chapter::Root(root));
    }
    Some(match clustering.of(ty) {
        Some(i) => Chapter::Cluster(i),
        None => Chapter::Other,
    })
}

/// Returns the job of the index file when it holds the Markdown page of
/// `route`: the home page under both layouts, and every page of the tree
/// under the single layout, whose plan has no page job.
///
/// A route has a page of the tree when a chapter of the Typst document holds
/// it, which is the rule of [`job_of`].
fn index_of<'j>(
    jobs: &'j [Job],
    schema: &Schema,
    clustering: &Clustering,
    route: &Route,
    format: Format,
) -> Option<&'j Job> {
    if format != Format::Markdown {
        return None;
    }
    let single = !jobs.iter().any(|j| matches!(j.kind, JobKind::Page { .. }));
    let paged = job_of(schema, clustering, route, Format::Typst).is_some();
    if !(*route == Route::Home || single && paged) {
        return None;
    }
    jobs.iter().find(|j| j.kind == JobKind::Index)
}

/// Returns whether a job of `kind` writes a file of `format`.
fn writes(kind: &JobKind, format: Format) -> bool {
    match format {
        Format::Markdown => matches!(
            kind,
            JobKind::Index
                | JobKind::IndexLetter(_)
                | JobKind::Page { .. }
                | JobKind::Changes { typst: false }
        ),
        Format::Typst => matches!(
            kind,
            JobKind::Theme
                | JobKind::TypstMain
                | JobKind::TypstChapter(_)
                | JobKind::Changes { typst: true }
        ),
    }
}

/// Answers the device chapter of the JSON file at `path` in `format`.
///
/// # Errors
///
/// Returns [`Stop::Fatal`] when the file cannot be read, is longer than the
/// readers take, is no JSON or holds no device chapter.
fn chapter(request: &JsObject, path: &Path, format: Format) -> Result<JsObject, Stop> {
    let most = u64::from(ReadLimits::DEFAULT.max_input_bytes);
    if fs::metadata(path).is_ok_and(|m| m.len() > most) {
        let text = format!("device {} is longer than {most} bytes", path.display());
        return Err(fatal(request, &text));
    }
    let bytes = fs::read(path).map_err(|e| {
        let text = format!("cannot read device {}: {e}", path.display());
        fatal(request, &text)
    })?;
    let value =
        reader::parse(&bytes).map_err(|e| fatal(request, &format!("device is no JSON: {e}")))?;
    let device = Field {
        value: &value,
        at: String::from("device"),
    }
    .device()
    .map_err(|at| fatal(request, &at))?;
    let mut text = String::new();
    match format {
        Format::Markdown => render_device_markdown(&device, &mut text),
        Format::Typst => render_device_typst(&device, &mut text),
    }
    let mut payload = JsObject::with_capacity(1);
    payload.insert("text", text);
    Ok(payload)
}

/// One value of the device file with the path that names it, which an error
/// carries.
struct Field<'v> {
    /// The value.
    value: &'v JsValue,
    /// Its path from the root, such as `device.blueprint[2].rows[0]`.
    at: String,
}

/// The reason a value of the device file is refused, the path and what was
/// expected there.
type Refusal = String;

impl<'v> Field<'v> {
    /// Returns the refusal of this value, which is no `what`.
    fn refuse(&self, what: &str) -> Refusal {
        format!("{} is no {what}", self.at)
    }

    /// Returns the object of this value whose keys are all among `keys`.
    fn object(&self, keys: &[&str]) -> Result<&'v JsObject, Refusal> {
        let JsValue::Object(object) = self.value else {
            return Err(self.refuse("object"));
        };
        for (key, _) in object {
            let known = key.as_str().is_some_and(|k| keys.contains(&k));
            if !known {
                return Err(format!(
                    "{} holds a key the device chapter does not name",
                    self.at
                ));
            }
        }
        Ok(object)
    }

    /// Returns the member `key` of the object `object` of this value.
    fn member(&self, object: &'v JsObject, key: &str) -> Result<Field<'v>, Refusal> {
        let at = format!("{}.{key}", self.at);
        match object.get(key) {
            Some(value) => Ok(Field { value, at }),
            None => Err(format!("{at} is missing")),
        }
    }

    /// Returns this value as a string.
    fn string(&self) -> Result<String, Refusal> {
        match self.value {
            JsValue::String(text) => text
                .as_str()
                .map(String::from)
                .ok_or_else(|| self.refuse("string a Rust string can hold")),
            _ => Err(self.refuse("string")),
        }
    }

    /// Returns this value as a count of at most `most`.
    fn count(&self, most: u64) -> Result<u64, Refusal> {
        match self.value {
            JsValue::Number(_) => whole(self.value)
                .filter(|&n| n <= most)
                .ok_or_else(|| self.refuse("whole number in its range")),
            _ => Err(self.refuse("number")),
        }
    }

    /// Returns this value as a signed whole number a double holds without a
    /// gap.
    fn signed(&self) -> Result<i64, Refusal> {
        let JsValue::Number(n) = self.value else {
            return Err(self.refuse("number"));
        };
        let magnitude =
            whole(&JsValue::Number(n.abs())).ok_or_else(|| self.refuse("whole number"))?;
        let magnitude = i64::try_from(magnitude).map_err(|_| self.refuse("whole number"))?;
        Ok(if *n < 0.0 { -magnitude } else { magnitude })
    }

    /// Returns this value as a boolean.
    fn boolean(&self) -> Result<bool, Refusal> {
        match self.value {
            JsValue::Bool(b) => Ok(*b),
            _ => Err(self.refuse("boolean")),
        }
    }

    /// Returns every item of this array read by `read`.
    fn list<T>(&self, read: impl Fn(&Field<'v>) -> Result<T, Refusal>) -> Result<Vec<T>, Refusal> {
        let JsValue::Array(items) = self.value else {
            return Err(self.refuse("array"));
        };
        if items.len() > MAX_ITEMS {
            return Err(self.refuse(&format!("array of at most {MAX_ITEMS} items")));
        }
        items
            .iter()
            .enumerate()
            .map(|(i, value)| {
                read(&Field {
                    value,
                    at: format!("{}[{i}]", self.at),
                })
            })
            .collect()
    }

    /// Returns this value as the strings of an array.
    fn strings(&self) -> Result<Vec<String>, Refusal> {
        self.list(Field::string)
    }

    /// Returns this value as a device chapter.
    fn device(&self) -> Result<DeviceChapter, Refusal> {
        let o = self.object(&[
            "card_file",
            "card_sha256",
            "id",
            "revision",
            "title",
            "signature",
            "source_date_epoch",
            "blueprint",
            "payload",
            "ran",
            "linked",
            "knowledge",
            "thumbnail",
        ])?;
        let m = |key| self.member(o, key);
        Ok(DeviceChapter {
            card_file: m("card_file")?.string()?,
            card_sha256: m("card_sha256")?.string()?,
            id: m("id")?.string()?,
            revision: u32::try_from(m("revision")?.count(u64::from(u32::MAX))?).unwrap_or(u32::MAX),
            title: m("title")?.string()?,
            signature: m("signature")?.signature()?,
            source_date_epoch: m("source_date_epoch")?.signed()?,
            blueprint: m("blueprint")?.list(Field::table)?,
            payload: m("payload")?.list(Field::payload)?,
            ran: m("ran")?.list(Field::ran)?,
            linked: m("linked")?.list(Field::linked)?,
            knowledge: m("knowledge")?.list(Field::knowledge)?,
            thumbnail: m("thumbnail")?.thumbnail()?,
        })
    }

    /// Returns this value as a signature status.
    fn signature(&self) -> Result<SignatureStatus, Refusal> {
        let o = self.object(&["kind", "signers", "tier", "serial"])?;
        let m = |key| self.member(o, key);
        match m("kind")?.string()?.as_str() {
            "unsigned" => Ok(SignatureStatus::Unsigned),
            "signed" => Ok(SignatureStatus::Signed {
                signers: m("signers")?.strings()?,
                tier: m("tier")?.string()?,
            }),
            "revoked" => Ok(SignatureStatus::Revoked {
                serial: m("serial")?.count(u64::MAX)?,
            }),
            _ => Err(format!("{}.kind is no signature status", self.at)),
        }
    }

    /// Returns this value as a table of the blueprint.
    fn table(&self) -> Result<BlueprintTable, Refusal> {
        let o = self.object(&["title", "rows"])?;
        Ok(BlueprintTable {
            title: self.member(o, "title")?.string()?,
            rows: self.member(o, "rows")?.list(Field::fact)?,
        })
    }

    /// Returns this value as a fact of the blueprint.
    fn fact(&self) -> Result<Fact, Refusal> {
        let o = self.object(&["name", "value", "mark", "sources"])?;
        let m = |key| self.member(o, key);
        Ok(Fact {
            name: m("name")?.string()?,
            value: m("value")?.string()?,
            mark: m("mark")?.mark()?,
            sources: m("sources")?.strings()?,
        })
    }

    /// Returns this value as a provenance mark.
    fn mark(&self) -> Result<Mark, Refusal> {
        let o = self.object(&["kind", "candidates", "closes"])?;
        let m = |key| self.member(o, key);
        match m("kind")?.string()?.as_str() {
            "proven" => Ok(Mark::Proven),
            "documented" => Ok(Mark::Documented),
            "measured" => Ok(Mark::Measured),
            "inventoried" => Ok(Mark::Inventoried),
            "assumed" => Ok(Mark::Assumed),
            "unknown" => Ok(Mark::Unknown {
                candidates: m("candidates")?.strings()?,
                closes: m("closes")?.string()?,
            }),
            _ => Err(format!("{}.kind is no mark", self.at)),
        }
    }

    /// Returns this value as a part of the payload.
    fn payload(&self) -> Result<PayloadRow, Refusal> {
        let o = self.object(&["part", "files", "raw", "packed", "digest"])?;
        let m = |key| self.member(o, key);
        Ok(PayloadRow {
            part: m("part")?.string()?,
            files: u32::try_from(m("files")?.count(u64::from(u32::MAX))?).unwrap_or(u32::MAX),
            raw: m("raw")?.count(u64::MAX)?,
            packed: m("packed")?.count(u64::MAX)?,
            digest: m("digest")?.string()?,
        })
    }

    /// Returns this value as a build script that ran.
    fn ran(&self) -> Result<RanRow, Refusal> {
        let o = self.object(&["path", "digest", "purpose"])?;
        let m = |key| self.member(o, key);
        Ok(RanRow {
            path: m("path")?.string()?,
            digest: m("digest")?.string()?,
            purpose: m("purpose")?.string()?,
        })
    }

    /// Returns this value as a crate that linked.
    fn linked(&self) -> Result<LinkedRow, Refusal> {
        let o = self.object(&["name", "version", "digest", "reason"])?;
        let m = |key| self.member(o, key);
        Ok(LinkedRow {
            name: m("name")?.string()?,
            version: m("version")?.string()?,
            digest: m("digest")?.string()?,
            reason: m("reason")?.string()?,
        })
    }

    /// Returns this value as a page of the knowledge annex.
    fn knowledge(&self) -> Result<KnowledgePage, Refusal> {
        let o = self.object(&["name", "origin", "pin", "text", "diverged"])?;
        let m = |key| self.member(o, key);
        Ok(KnowledgePage {
            name: m("name")?.string()?,
            origin: m("origin")?.string()?,
            pin: m("pin")?.string()?,
            text: m("text")?.string()?,
            diverged: m("diverged")?.boolean()?,
        })
    }

    /// Returns this value as a thumbnail, null or its bytes in base 16.
    fn thumbnail(&self) -> Result<Option<Vec<u8>>, Refusal> {
        if matches!(self.value, JsValue::Null) {
            return Ok(None);
        }
        let text = self.string()?;
        unhex(&text)
            .map(Some)
            .ok_or_else(|| self.refuse("string of base 16 digits"))
    }
}
