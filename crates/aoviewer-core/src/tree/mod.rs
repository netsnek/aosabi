// SPDX-License-Identifier: MIT
/*
 * aoviewer-core tree: the plan of a documentation tree and the rendering of its
 * jobs.
 *
 * A tree is the pages, the chapters, the drawings, the index, the search file
 * and the manifest.
 */

//! The documentation tree.
//!
//! [`render_tree`] writes the whole reference of a schema into memory: the
//! Markdown pages, the Typst document with its chapters and its theme, the SVG
//! drawings in each theme, the search file and `manifest.json`. [`plan_tree`]
//! names the files and [`render_job`] writes one of them, so the native build
//! and the wasm module write the same bytes for one path.
//!
//! The bytes of a tree follow the rules of the plan. Every text is UTF-8 with
//! line feeds and holds no timestamp, no absolute path, no host name and no
//! environment value, and the generator string and [󰀀](crate::LAYOUT_VERSION)
//! appear in `manifest.json` alone.
//!
//! - A Markdown page is the page model of its route written by
//!   [󰀀](crate::render_markdown::render_markdown), its file and every link it
//!   makes named by [󰀁](crate::render_markdown::page_path). A diagram is a
//!   `picture` element over the files of both themes, or a plain image over the
//!   one theme written, and under `mermaid` a neighbourhood diagram of at most
//!   12 boxes gains its Mermaid form beside it.
//! - A Typst chapter opens with `#import "/sabi-theme.typ": *`, or with
//!   `#import "/template.typ": *` under `typst_template`, since an included
//!   file sees no binding of the file that includes it, and holds its pages
//!   written by [󰀃](crate::render_typst::render_typst_page_with_diagrams), each
//!   diagram the `#sv-diagram(..)` call of its scene, or under
//!   [`TypstDiagrams::Svg`] the image of its SVG file in the theme of the
//!   document. A chapter is never split by its size.
//! - `api.typ` imports the theme, sets the document with `date: none`, or with
//!   the date of `document_date` computed by integer arithmetic, applies the
//!   page setup and includes every chapter in the order of the paths.
//! - An SVG file is the scene of the view of its route and part, laid out by
//!   the layout of its kind and written by [`render_svg`] with the schema in
//!   second place.
//!
//! A diagram wider than the 1400 px of a page and a cluster map of more than
//! 120 clusters are drawn as they are, counted and named in a note with the
//! width or the clusters, never hidden. A job of a writer the build does not
//! carry writes nothing.
//!
//! Every loop runs over the jobs, pages, diagrams, themes, files or notes of
//! one tree, whose counts the model of the schema caps, or over the
//! differences of a change log, which [`diff`](crate::diff::diff) writes at
//! most once per element and aspect.

mod index;
mod manifest;
mod options;
mod plan;
mod search_file;

pub use options::{
    DocsOptions,
    Formats,
    LayoutChoice,
    MAX_QUERY_BYTES,
    Themes,
    TypstDiagrams, //
};
pub use plan::{
    Chapter,
    Job,
    JobKind,
    LETTER_TYPES,
    SINGLE_BOXES,
    TreePlan,
    plan_tree, //
};

use alloc::{
    format,
    string::String,
    vec,
    vec::Vec, //
};
use core::fmt::Write;

use plan::{
    Site,
    SiteDiagram,
    diagram_path,
    layout_mode, //
};

use crate::{
    cluster::{
        ClusterSource,
        Clustering, //
    },
    diff::Difference,
    layout::{
        LayoutError,
        layout_of, //
    },
    page::{
        Block,
        LayoutMode,
        PageModel, //
    },
    render_svg::{
        SvgError,
        Theme,
        render_svg, //
    },
    route::{
        OptionError,
        Route,
        WholeRoot, //
    },
    scene::Scene,
    schema::Schema,
    view::{
        Budget,
        View,
        ViewError, //
    },
};

/// The media type of a Markdown file.
const MARKDOWN_TYPE: &str = "text/markdown; charset=utf-8";

/// The media type of a Typst file, plain text, since no media type of Typst is
/// registered.
const TYPST_TYPE: &str = "text/plain; charset=utf-8";

/// The media type of an SVG file.
const SVG_TYPE: &str = "image/svg+xml";

/// The media type of a JSON file.
const JSON_TYPE: &str = "application/json";

/// The path of the Typst main file.
const TYPST_MAIN: &str = "api.typ";

/// The path of the manifest.
const MANIFEST: &str = "manifest.json";

/// The path of the change log in Markdown, which the fixture table of the plan
/// names for every layout.
const CHANGES_MD: &str = "md/changes.md";

/// The path of the change log in Typst.
const CHANGES_TYP: &str = "typ/90-changes.typ";

/// The files of a tree in path order, what it counted and the notes it
/// prints.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tree {
    /// The files in the byte order of their paths, `manifest.json` among
    /// them.
    pub files: Vec<TreeFile>,
    /// The counts of the tree.
    pub counts: Counts,
    /// The sentences a command prints beside the tree, such as a poster that
    /// is not written and why.
    pub notes: Vec<String>,
}

/// One file of a tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeFile {
    /// The path inside the tree, such as `svg/type/Product.light.svg`.
    pub path: String,
    /// The media type.
    pub media_type: &'static str,
    /// The bytes.
    pub bytes: Vec<u8>,
}

/// The counts of a tree, which `manifest.json` records and the command
/// prints.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Counts {
    /// The types a page documents, the introspection types left out.
    pub types: u32,
    /// The boxes of the graph.
    pub boxes: u32,
    /// The drawn diagrams of the pages, each part one, each counted once
    /// whatever its themes.
    pub diagrams: u32,
    /// The pages of the schema, the home page, the root, cluster, type and
    /// report pages and the change log of `--since`, each counted once
    /// whatever its formats and whether or not a format of the tree writes
    /// it, the parts of the index left out.
    pub pages: u32,
    /// The diagrams the pages skip and say why in a sentence.
    pub skipped: u32,
    /// The drawn diagrams a reduction of the page budget shaped.
    pub reduced: u32,
    /// The drawn diagrams wider than the 1400 px of a page.
    pub over_budget: u32,
    /// The clusters.
    pub clusters: u32,
    /// The source of the clusters.
    pub cluster_source: ClusterSource,
}

/// A reason a tree or one of its jobs is not written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TreeError {
    /// The view of a page or of a diagram refuses its route.
    View(ViewError),
    /// The layout of a diagram answers no scene.
    Layout(LayoutError),
    /// The SVG writer writes no drawing, under `svg_text=outlines` before
    /// the face exists.
    Svg(SvgError),
    /// The options of the tree are refused, the error of
    /// [`DocsOptions::parse`], which a caller that reads the options from
    /// their query reports as a refusal of the tree. The functions of this
    /// module take parsed options and never return it.
    Option(OptionError),
}

/// The facts of one drawn diagram that a page writes beside it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Drawn {
    /// The stem of its files.
    stem: String,
    /// The width in pixels, rounded up.
    width_px: u32,
    /// The height in pixels, rounded up.
    height_px: u32,
    /// Whether a reduction of the page budget shaped it.
    reduced: bool,
    /// The width in centipixels, which the note of a diagram wider than a
    /// page prints.
    width_cp: i32,
    /// Whether it is wider than the 1400 px of a page.
    over: bool,
    /// Its Mermaid form, for a neighbourhood diagram of at most 12 boxes
    /// under `mermaid`.
    mermaid: Option<String>,
}

/// Writes the file of `job` of the tree of `schema` under `options` and
/// appends its bytes to `out`.
///
/// `clustering` is the clustering picked from `schema`. A page draws the
/// diagrams the plan names and lays each one out anew, and `manifest.json`
/// hashes the whole tree [`render_tree`] writes without a change log, so a
/// job writes the bytes the same file holds in that tree. The job of the
/// change log knows no older schema and writes the page of no difference.
/// A job of a writer the build does not carry writes nothing.
///
/// # Errors
///
/// Returns [`TreeError::View`] when the view of a page or a diagram refuses
/// its route, [`TreeError::Layout`] when the layout of a diagram answers no
/// scene and [`TreeError::Svg`] when the SVG writer refuses its text mode.
///
/// # Panics
///
/// Panics when `clustering` was not picked from `schema`.
pub fn render_job(
    schema: &Schema,
    clustering: &Clustering,
    options: &DocsOptions,
    job: &Job,
    out: &mut Vec<u8>,
) -> Result<(), TreeError> {
    let site = Site::build(schema, clustering, options);
    if job.kind == JobKind::Manifest {
        let tree = render_tree(schema, clustering, options, None)?;
        if let Some(file) = tree.files.iter().find(|f| f.path == MANIFEST) {
            out.extend_from_slice(&file.bytes);
        }
        return Ok(());
    }
    let mut text = String::new();
    match &job.kind {
        JobKind::Diagram { route, part, theme } => {
            if let Some(d) = site.diagram(route, *part) {
                let (scene, _) = draw(&site, d)?;
                svg(&site, &scene, &d.route, d.part, *theme, &mut text)?;
            }
        }
        JobKind::Poster { root, theme } => poster(&site, root, *theme, &mut text)?,
        JobKind::SearchFile => {
            search_file::write(&site, &search_file::entries(&site), None, &mut text);
        }
        JobKind::SearchLetter(letter) => {
            let entries = search_file::entries(&site);
            search_file::write(&site, &entries, Some(*letter), &mut text);
        }
        JobKind::Index | JobKind::IndexLetter(_) | JobKind::Page { .. } => {
            let drawn = drawn_of(&site, &job.kind)?;
            markdown_job(&site, &job.kind, &drawn, &mut text)?;
        }
        JobKind::Theme | JobKind::TypstMain | JobKind::TypstChapter(_) => {
            typst_job(&site, &job.kind, &mut text)?;
        }
        JobKind::Changes { typst } => change_log(&site, &[], *typst, &mut text),
        JobKind::Manifest => {}
    }
    out.extend_from_slice(text.as_bytes());
    Ok(())
}

/// Writes every job of the plan of the tree of `schema` under `options`, and
/// `manifest.json` last, because it hashes the others.
///
/// `clustering` is the clustering picked from `schema`. `changes` adds the
/// text change log of `--since`, `md/changes.md` with the Markdown and
/// `typ/90-changes.typ` with the Typst, which `api.typ` includes. Every
/// diagram is laid out once for its SVG files and its sizes, and once more
/// for the chapter that draws it natively. The notes say what the tree
/// leaves out and why, which diagrams are wider than a page and how wide,
/// and how many paths only case separates and the first of them.
///
/// # Errors
///
/// Returns the errors of [`render_job`].
///
/// # Panics
///
/// Panics when `clustering` was not picked from `schema`.
pub fn render_tree(
    schema: &Schema,
    clustering: &Clustering,
    options: &DocsOptions,
    changes: Option<&[Difference]>,
) -> Result<Tree, TreeError> {
    let site = Site::build(schema, clustering, options);
    let plan = site.plan();
    let mut files = Vec::with_capacity(plan.jobs.len() + 2);
    let drawn = draw_all(&site, &mut files)?;
    let entries = search_file::entries(&site);
    for job in &plan.jobs {
        let mut text = String::new();
        match &job.kind {
            JobKind::Diagram { .. } | JobKind::TypstMain | JobKind::Manifest => continue,
            JobKind::Poster { root, theme } => poster(&site, root, *theme, &mut text)?,
            JobKind::SearchFile => search_file::write(&site, &entries, None, &mut text),
            JobKind::SearchLetter(letter) => {
                search_file::write(&site, &entries, Some(*letter), &mut text);
            }
            JobKind::Index | JobKind::IndexLetter(_) | JobKind::Page { .. } => {
                markdown_job(&site, &job.kind, &drawn, &mut text)?;
            }
            JobKind::Theme | JobKind::TypstChapter(_) => typst_job(&site, &job.kind, &mut text)?,
            JobKind::Changes { typst } => change_log(&site, &[], *typst, &mut text),
        }
        files.push(file(job.path.clone(), text));
    }
    if let Some(diffs) = changes {
        push_changes(&site, diffs, &mut files);
    }
    let counts = count(&site, &drawn, changes.is_some());
    let mut notes = site.notes.clone();
    if let Some(note) = over_note(&drawn) {
        notes.push(note);
    }
    let mut tree = Tree {
        files,
        counts,
        notes,
    };
    seal_tree(&mut tree, schema, options);
    if let Some(note) = case_note(&tree.files) {
        tree.notes.push(note);
    }
    Ok(tree)
}

/// Writes `api.typ` over every chapter `tree` holds and `manifest.json` over
/// every other file, and puts the files in the byte order of their paths.
///
/// [`render_tree`] seals every tree it writes. `api.typ` is written when the
/// tree holds a file under `typ/` and the build carries the Typst writer, and
/// an `api.typ` or a `manifest.json` the tree held before is replaced.
pub fn seal_tree(tree: &mut Tree, schema: &Schema, options: &DocsOptions) {
    tree.files
        .retain(|f| f.path != MANIFEST && f.path != TYPST_MAIN);
    sort_files(&mut tree.files);
    let chapters: Vec<&str> = tree
        .files
        .iter()
        .map(|f| f.path.as_str())
        .filter(|p| p.starts_with("typ/"))
        .collect();
    if !chapters.is_empty() {
        let mut text = String::new();
        typst_main(options, &chapters, &mut text);
        if !text.is_empty() {
            tree.files.push(file(String::from(TYPST_MAIN), text));
            sort_files(&mut tree.files);
        }
    }
    let layout = layout_mode(schema, options.layout);
    let mut bytes = Vec::new();
    manifest::write(
        schema,
        options,
        layout,
        &tree.counts,
        &tree.files,
        &mut bytes,
    );
    tree.files.push(TreeFile {
        path: String::from(MANIFEST),
        media_type: JSON_TYPE,
        bytes,
    });
    sort_files(&mut tree.files);
}

/// Puts `files` in the byte order of their paths.
fn sort_files(files: &mut [TreeFile]) {
    files.sort_by(|a, b| a.path.as_bytes().cmp(b.path.as_bytes()));
}

/// Returns the file of `path` holding `text`, with the media type of its
/// extension.
fn file(path: String, text: String) -> TreeFile {
    let media_type = match path.rsplit_once('.').map(|(_, extension)| extension) {
        Some("md") => MARKDOWN_TYPE,
        Some("typ") => TYPST_TYPE,
        Some("svg") => SVG_TYPE,
        _ => JSON_TYPE,
    };
    TreeFile {
        path,
        media_type,
        bytes: text.into_bytes(),
    }
}

/// Returns the view and the scene of the drawn diagram `d`.
///
/// The view is the part `d` names of the view of its route under the page
/// budget, laid out by the layout of its kind with no seed, the canonical
/// scene.
fn draw(site: &Site<'_>, d: &SiteDiagram) -> Result<(Scene, View), TreeError> {
    let views = View::for_page(site.schema, site.clustering, &d.route, &site.options.view)
        .map_err(TreeError::View)?;
    let index = d.part.map_or(0, |n| usize::from(n).saturating_sub(1));
    let view = views
        .into_iter()
        .nth(index)
        .ok_or(TreeError::Layout(LayoutError::Empty))?;
    let scene = layout_of(&view, site.schema, None).map_err(TreeError::Layout)?;
    Ok((scene, view))
}

/// Returns the facts of the drawn diagram `d` from its scene and its view.
fn facts(site: &Site<'_>, d: &SiteDiagram, scene: &Scene, view: &View) -> Drawn {
    let mermaid = if site.options.mermaid && is_type(&d.route) {
        mermaid_of(site, view)
    } else {
        None
    };
    Drawn {
        stem: d.stem.clone(),
        width_px: px(scene.width),
        height_px: px(scene.height),
        reduced: !view.reductions.is_empty(),
        width_cp: scene.width,
        over: scene.width > Budget::PAGE.width_cp,
        mermaid,
    }
}

/// Lays out every drawn diagram of the tree once, appends its SVG files to
/// `files`, and returns its facts in the order of the stems.
fn draw_all(site: &Site<'_>, files: &mut Vec<TreeFile>) -> Result<Vec<Drawn>, TreeError> {
    let mut drawn = Vec::with_capacity(site.diagrams.len());
    for d in &site.diagrams {
        let (scene, view) = draw(site, d)?;
        for &theme in &site.svg_themes {
            let mut text = String::new();
            svg(site, &scene, &d.route, d.part, theme, &mut text)?;
            files.push(file(diagram_path(&d.stem, theme), text));
        }
        drawn.push(facts(site, d, &scene, &view));
    }
    Ok(drawn)
}

/// Returns the facts of the drawn diagrams the Markdown file of `kind`
/// writes, in the order of the stems.
fn drawn_of(site: &Site<'_>, kind: &JobKind) -> Result<Vec<Drawn>, TreeError> {
    let mut drawn = Vec::new();
    if site.svg_themes.is_empty() && !site.options.mermaid {
        return Ok(drawn);
    }
    let wanted: Vec<&Route> = match kind {
        JobKind::Page { route } => vec![route],
        JobKind::Index if site.layout == LayoutMode::Single => {
            site.pages.iter().map(|p| &p.route).collect()
        }
        JobKind::Index => vec![&Route::Home],
        _ => Vec::new(),
    };
    for page in site.pages.iter().filter(|p| wanted.contains(&&p.route)) {
        let Ok(model) = &page.model else {
            continue;
        };
        for (route, part) in diagrams_of(model) {
            if let Some(d) = site.diagram(route, part) {
                let (scene, view) = draw(site, d)?;
                drawn.push(facts(site, d, &scene, &view));
            }
        }
    }
    drawn.sort_by(|a, b| a.stem.as_bytes().cmp(b.stem.as_bytes()));
    drawn.dedup_by(|a, b| a.stem == b.stem);
    Ok(drawn)
}

/// Returns the route and the part of every drawn diagram of `model`, its
/// sections included.
fn diagrams_of(model: &PageModel) -> Vec<(&Route, Option<u16>)> {
    let mut out = Vec::new();
    let sections = model.blocks.iter().filter_map(|b| match b {
        Block::Section { blocks, .. } => Some(blocks.as_slice()),
        _ => None,
    });
    for blocks in core::iter::once(model.blocks.as_slice()).chain(sections) {
        for block in blocks {
            if let Block::Diagram {
                route,
                part,
                skipped: None,
            } = block
            {
                out.push((route, *part));
            }
        }
    }
    out
}

/// Returns the facts of the diagram of `stem` among `drawn`.
#[cfg(feature = "markdown")]
fn find<'d>(drawn: &'d [Drawn], stem: &str) -> Option<&'d Drawn> {
    drawn
        .binary_search_by(|d| d.stem.as_bytes().cmp(stem.as_bytes()))
        .ok()
        .map(|i| &drawn[i])
}

/// Returns whether `route` is the route of a type, whose diagram is a
/// neighbourhood diagram.
fn is_type(route: &Route) -> bool {
    matches!(route, Route::Type { .. })
}

/// Returns the Mermaid form of `view`, when it holds at most 12 boxes and
/// the build carries the Markdown writer.
#[cfg(feature = "markdown")]
fn mermaid_of(site: &Site<'_>, view: &View) -> Option<String> {
    let mut body = String::new();
    crate::render_markdown::render_mermaid(view, site.schema, &mut body).ok()?;
    Some(body)
}

/// Returns no Mermaid form, since the build carries no Markdown writer.
#[cfg(not(feature = "markdown"))]
fn mermaid_of(_site: &Site<'_>, _view: &View) -> Option<String> {
    None
}

/// Returns `cp` centipixels as whole pixels, rounded up, and 0 for a
/// negative length.
fn px(cp: i32) -> u32 {
    u32::try_from(cp.max(0)).map_or(0, |cp| cp.div_ceil(100))
}

/// Writes the SVG of `scene` in `theme`, titled after `route` and `part`.
fn svg(
    site: &Site<'_>,
    scene: &Scene,
    route: &Route,
    part: Option<u16>,
    theme: Theme,
    out: &mut String,
) -> Result<(), TreeError> {
    let title = diagram_title(route, part);
    render_svg(
        scene,
        site.schema,
        theme,
        site.options.svg_text,
        &title,
        out,
    )
    .map_err(TreeError::Svg)
}

/// Writes the poster of `root` in `theme`, or nothing when the whole graph
/// from `root` does not fit [`Budget::WHOLE`].
fn poster(
    site: &Site<'_>,
    root: &WholeRoot,
    theme: Theme,
    out: &mut String,
) -> Result<(), TreeError> {
    let route = Route::Whole { root: root.clone() };
    let view = View::of(
        site.schema,
        site.clustering,
        &route,
        &site.options.view,
        &Budget::WHOLE,
    )
    .map_err(TreeError::View)?;
    if view.empty.is_some() {
        return Ok(());
    }
    let scene = layout_of(&view, site.schema, None).map_err(TreeError::Layout)?;
    svg(site, &scene, &route, None, theme, out)
}

/// Returns the title of the drawing of `route` and `part`, the alternative text
/// the Markdown writer gives the same diagram.
fn diagram_title(route: &Route, part: Option<u16>) -> String {
    let mut title = match route {
        Route::Home => String::from("Overview diagram"),
        Route::Type { name, .. } => format!("Neighbourhood diagram of {name}"),
        Route::Op { field, .. } => format!("Operation diagram of {field}"),
        Route::Cluster { slug } => format!("Cluster diagram of {slug}"),
        Route::Whole { root } => String::from(match root {
            WholeRoot::Query => "Whole graph from Query",
            WholeRoot::Mutation => "Whole graph from Mutation",
            WholeRoot::Subscription => "Whole graph from Subscription",
            WholeRoot::All => "Whole graph of every root",
        }),
        _ => String::from("Diagram"),
    };
    if let Some(n) = part {
        // Writing into a `String` cannot fail.
        let _ = write!(title, ", part {n}");
    }
    title
}

/// Writes the Markdown file of `kind`, with the facts of its diagrams in
/// `drawn`.
#[cfg(feature = "markdown")]
fn markdown_job(
    site: &Site<'_>,
    kind: &JobKind,
    drawn: &[Drawn],
    out: &mut String,
) -> Result<(), TreeError> {
    if !site.md {
        return Ok(());
    }
    match kind {
        JobKind::Index => index::markdown_index(site, drawn, out),
        JobKind::IndexLetter(letter) => index::markdown_letter(site, *letter, out),
        JobKind::Page { route } => {
            let Some(page) = site.pages.iter().find(|p| &p.route == route) else {
                return Ok(());
            };
            let model = page
                .model
                .as_ref()
                .map_err(|e| TreeError::View(e.clone()))?;
            let path = crate::render_markdown::page_path(site.schema, route, LayoutMode::Split)
                .unwrap_or_default();
            markdown_page(site, model, &path, drawn, out);
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Writes nothing, since the build carries no Markdown writer.
// The stub keeps the signature of the writer it stands for, whose refusal
// the callers propagate, and the lint fires in this feature set alone.
#[cfg(not(feature = "markdown"))]
#[allow(clippy::unnecessary_wraps)]
fn markdown_job(
    _site: &Site<'_>,
    _kind: &JobKind,
    _drawn: &[Drawn],
    _out: &mut String,
) -> Result<(), TreeError> {
    Ok(())
}

/// Writes `model` as the Markdown page of the file `from`, every drawn
/// diagram over its SVG files with the facts of `drawn`.
#[cfg(feature = "markdown")]
fn markdown_page(
    site: &Site<'_>,
    model: &PageModel,
    from: &str,
    drawn: &[Drawn],
    out: &mut String,
) {
    use crate::{
        page::Links,
        render_markdown::{
            DiagramRef,
            render_markdown, //
        },
    };

    let links = Links {
        layout: site.layout,
        viewer_url: site.options.viewer_url.clone(),
        from: String::from(from),
    };
    let diagram = |route: &Route, part: Option<u16>| -> Option<DiagramRef> {
        let stem = site.stem(route, part)?;
        let d = find(drawn, &stem)?;
        let path = |theme| {
            site.svg_themes
                .contains(&theme)
                .then(|| diagram_path(&stem, theme))
        };
        Some(DiagramRef {
            light: path(Theme::Light),
            dark: path(Theme::Dark),
            width_px: d.width_px,
            height_px: d.height_px,
            mermaid: d.mermaid.clone(),
        })
    };
    render_markdown(model, site.schema, &links, &diagram, out);
}

/// Writes the Typst file of `kind`.
#[cfg(feature = "typst")]
fn typst_job(site: &Site<'_>, kind: &JobKind, out: &mut String) -> Result<(), TreeError> {
    if !site.typ {
        return Ok(());
    }
    match kind {
        JobKind::Theme => out.push_str(crate::render_typst::SABI_THEME),
        JobKind::TypstMain => {
            let paths: Vec<&str> = site.chapters.iter().map(|(_, p)| p.as_str()).collect();
            typst_main(site.options, &paths, out);
        }
        JobKind::TypstChapter(chapter) => typst_chapter(site, *chapter, out)?,
        _ => {}
    }
    Ok(())
}

/// Writes nothing, since the build carries no Typst writer.
// The stub keeps the signature of the writer it stands for, whose refusal
// the callers propagate, and the lint fires in this feature set alone.
#[cfg(not(feature = "typst"))]
#[allow(clippy::unnecessary_wraps)]
fn typst_job(_site: &Site<'_>, _kind: &JobKind, _out: &mut String) -> Result<(), TreeError> {
    Ok(())
}

/// Writes the import line every Typst file of the tree opens with.
#[cfg(feature = "typst")]
fn typst_import(options: &DocsOptions, out: &mut String) {
    out.push_str(if options.typst_template {
        "#import \"/template.typ\": *\n"
    } else {
        "#import \"/sabi-theme.typ\": *\n"
    });
}

/// Writes `api.typ`: the import, the document with its title and date, the
/// page setup and the include of every chapter of `paths` in their order.
#[cfg(feature = "typst")]
fn typst_main(options: &DocsOptions, paths: &[&str], out: &mut String) {
    use crate::render_typst::typst_str;

    typst_import(options, out);
    let title = if options.schema_name.is_empty() {
        String::from("API reference")
    } else {
        format!("{} API reference", options.schema_name)
    };
    out.push_str("#set document(title: ");
    typst_str(&title, out);
    out.push_str(", author: (), date: ");
    match options.document_date.and_then(civil) {
        Some(date) => date.write(out),
        None => out.push_str("none"),
    }
    out.push_str(")\n");
    let dark = options.themes.dark && !options.themes.light;
    out.push_str(if dark {
        "#show: sv-setup.with(dark: true)\n"
    } else {
        "#show: sv-setup\n"
    });
    for path in paths {
        out.push_str("#include ");
        typst_str(&format!("/{path}"), out);
        out.push('\n');
    }
}

/// Writes nothing, since the build carries no Typst writer.
#[cfg(not(feature = "typst"))]
fn typst_main(_options: &DocsOptions, _paths: &[&str], _out: &mut String) {}

/// Writes the chapter `chapter`: the import, an opening title for the types
/// no cluster holds, and its pages, or the index.
#[cfg(feature = "typst")]
fn typst_chapter(site: &Site<'_>, chapter: Chapter, out: &mut String) -> Result<(), TreeError> {
    use crate::render_typst::typst_str;

    typst_import(site.options, out);
    match chapter {
        Chapter::Index | Chapter::IndexLetter(_) => {
            index::typst_index(site, chapter, out);
            return Ok(());
        }
        Chapter::Other => {
            let title = if site.clustering.clusters.is_empty() {
                "Types"
            } else {
                "Other types"
            };
            out.push_str("#sv-title(1, (");
            typst_str(title, out);
            out.push_str(",))\n");
        }
        _ => {}
    }
    let path = site.chapter_path(chapter);
    for page in site.pages.iter().filter(|p| p.chapter == chapter) {
        let model = page
            .model
            .as_ref()
            .map_err(|e| TreeError::View(e.clone()))?;
        typst_page(site, model, &path, out)?;
    }
    Ok(())
}

/// Writes `model` as a page of the chapter `from`, every drawn diagram as
/// the call of its scene or the image of its SVG file.
#[cfg(feature = "typst")]
fn typst_page(
    site: &Site<'_>,
    model: &PageModel,
    from: &str,
    out: &mut String,
) -> Result<(), TreeError> {
    use crate::{
        page::Links,
        render_typst::{
            render_typst_page_with_diagrams,
            render_typst_scene,
            typst_str, //
        },
    };

    let images = site.options.typst_diagrams == TypstDiagrams::Svg
        && site.svg_themes.contains(&site.doc_theme);
    let mut scenes: Vec<(String, Scene)> = Vec::new();
    if !images {
        for (route, part) in diagrams_of(model) {
            if let Some(d) = site.diagram(route, part) {
                let (scene, _) = draw(site, d)?;
                scenes.push((d.stem.clone(), scene));
            }
        }
    }
    let links = Links {
        layout: site.layout,
        viewer_url: site.options.viewer_url.clone(),
        from: String::from(from),
    };
    let diagram = |route: &Route, part: Option<u16>, out: &mut String| {
        let Some(stem) = site.stem(route, part) else {
            return;
        };
        if images {
            out.push_str("#sv-image(");
            typst_str(&format!("/{}", diagram_path(&stem, site.doc_theme)), out);
            out.push_str(")\n");
        } else if let Some((_, scene)) = scenes.iter().find(|(s, _)| *s == stem) {
            render_typst_scene(scene, site.schema, out);
        }
    };
    render_typst_page_with_diagrams(model, site.schema, &links, &diagram, out);
    Ok(())
}

/// Writes the change log of `diffs` as the Markdown page, or with `typst` as
/// the Typst chapter, when the tree writes that format.
fn change_log(site: &Site<'_>, diffs: &[Difference], typst: bool, out: &mut String) {
    let page = crate::diff::changes_page(diffs);
    if typst {
        changes_typst(site, &page, out);
    } else {
        changes_markdown(site, &page, out);
    }
}

/// Appends the files of the change log of `diffs` in the formats the tree
/// writes.
fn push_changes(site: &Site<'_>, diffs: &[Difference], files: &mut Vec<TreeFile>) {
    for (typst, path, written) in [(false, CHANGES_MD, site.md), (true, CHANGES_TYP, site.typ)] {
        if written {
            let mut text = String::new();
            change_log(site, diffs, typst, &mut text);
            files.push(file(String::from(path), text));
        }
    }
}

/// Writes the change log `page` as `md/changes.md`, linked in the layout of
/// the tree.
#[cfg(feature = "markdown")]
fn changes_markdown(site: &Site<'_>, page: &PageModel, out: &mut String) {
    if site.md {
        markdown_page(site, page, CHANGES_MD, &[], out);
    }
}

/// Writes nothing, since the build carries no Markdown writer.
#[cfg(not(feature = "markdown"))]
fn changes_markdown(_site: &Site<'_>, _page: &PageModel, _out: &mut String) {}

/// Writes the change log `page` as `typ/90-changes.typ`.
#[cfg(feature = "typst")]
fn changes_typst(site: &Site<'_>, page: &PageModel, out: &mut String) {
    if site.typ {
        typst_import(site.options, out);
        let links = crate::page::Links {
            layout: site.layout,
            viewer_url: site.options.viewer_url.clone(),
            from: String::from(CHANGES_TYP),
        };
        crate::render_typst::render_typst_page(page, site.schema, &links, out);
    }
}

/// Writes nothing, since the build carries no Typst writer.
#[cfg(not(feature = "typst"))]
fn changes_typst(_site: &Site<'_>, _page: &PageModel, _out: &mut String) {}

/// Returns the counts of the tree of `site` with the facts of its diagrams.
fn count(site: &Site<'_>, drawn: &[Drawn], changes: bool) -> Counts {
    let n = |count: usize| u32::try_from(count).unwrap_or(u32::MAX);
    let skipped = site
        .pages
        .iter()
        .filter_map(|p| p.model.as_ref().ok())
        .map(skipped_of)
        .sum::<usize>();
    Counts {
        types: site.types,
        boxes: n(site.schema.graph.nodes.len()),
        diagrams: n(site.diagrams.len()),
        pages: n(site.pages.len() + usize::from(changes)),
        skipped: n(skipped),
        reduced: n(drawn.iter().filter(|d| d.reduced).count()),
        over_budget: n(drawn.iter().filter(|d| d.over).count()),
        clusters: n(site.clustering.clusters.len()),
        cluster_source: site.clustering.source,
    }
}

/// Returns the diagrams `model` skips, its sections included.
fn skipped_of(model: &PageModel) -> usize {
    let skipped = |blocks: &[Block]| {
        blocks
            .iter()
            .filter(|b| {
                matches!(
                    b,
                    Block::Diagram {
                        skipped: Some(_),
                        ..
                    }
                )
            })
            .count()
    };
    let nested: usize = model
        .blocks
        .iter()
        .map(|b| match b {
            Block::Section { blocks, .. } => skipped(blocks),
            _ => 0,
        })
        .sum();
    skipped(&model.blocks) + nested
}

/// Returns the note that counts the paths of `files` that only case
/// separates from an earlier one and names the first of them, which a file
/// system that folds case cannot hold apart, or `None` when there are none.
fn case_note(files: &[TreeFile]) -> Option<String> {
    let mut folded: Vec<(String, &str)> = files
        .iter()
        .map(|f| (f.path.to_ascii_lowercase(), f.path.as_str()))
        .collect();
    folded.sort_unstable();
    let clashes: Vec<&str> = folded
        .windows(2)
        .filter(|w| w[0].0 == w[1].0)
        .map(|w| w[1].1)
        .collect();
    let first = clashes.first()?;
    let n = u32::try_from(clashes.len()).unwrap_or(u32::MAX);
    Some(format!(
        "The tree holds {} from an earlier one only by case, the first {first}, so it does not check out on a file system that folds case.",
        counted(n, "path that differs", "paths that differ")
    ))
}

/// Returns the note that names every diagram of `drawn` wider than the
/// 1400 px of a page with its width, in the order of the stems, or `None`
/// when there is none.
fn over_note(drawn: &[Drawn]) -> Option<String> {
    let over: Vec<String> = drawn
        .iter()
        .filter(|d| d.over)
        .map(|d| {
            let cp = d.width_cp.max(0);
            format!("{} at {}.{:02} px", d.stem, cp / 100, cp % 100)
        })
        .collect();
    let n = u32::try_from(over.len()).unwrap_or(u32::MAX);
    let (last, rest) = over.split_last()?;
    let names = if rest.is_empty() {
        last.clone()
    } else {
        format!("{} and {last}", rest.join(", "))
    };
    Some(format!(
        "The tree draws {} wider than the {} px of a page, {names}.",
        counted(n, "diagram", "diagrams"),
        Budget::PAGE.width_cp / 100
    ))
}

/// Returns `n` followed by `one` when it is 1 and by `many` otherwise.
fn counted(n: u32, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// A date of the proleptic Gregorian calendar with its time of day.
#[cfg(feature = "typst")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Civil {
    /// The year.
    year: i64,
    /// The month, from 1.
    month: i64,
    /// The day of the month, from 1.
    day: i64,
    /// The seconds since midnight.
    seconds: i64,
}

#[cfg(feature = "typst")]
impl Civil {
    /// Writes the date as the Typst call `datetime(..)`.
    fn write(self, out: &mut String) {
        // Writing into a `String` cannot fail.
        let _ = write!(
            out,
            "datetime(year: {}, month: {}, day: {}, hour: {}, minute: {}, second: {})",
            self.year,
            self.month,
            self.day,
            self.seconds / 3600,
            self.seconds / 60 % 60,
            self.seconds % 60
        );
    }
}

/// The earliest year a document date carries, the first of four digits.
#[cfg(feature = "typst")]
const FIRST_YEAR: i64 = 1;

/// The latest year a document date carries, the last of four digits.
#[cfg(feature = "typst")]
const LAST_YEAR: i64 = 9999;

/// Returns the date of `epoch` Unix seconds in UTC, or `None` for a date
/// outside the years [`FIRST_YEAR`] to [`LAST_YEAR`].
///
/// The day number is turned into a year, a month and a day by integer
/// arithmetic over eras of 400 years, and a time before 1970 is handled by
/// Euclidean division. A PDF date carries a year of four digits, so a date
/// outside them leaves the document date unset.
///
/// [chrono-Compatible Low-Level Date Algorithms]: https://howardhinnant.github.io/date_algorithms.html
#[cfg(feature = "typst")]
fn civil(epoch: i64) -> Option<Civil> {
    let days = epoch.div_euclid(86_400);
    let seconds = epoch.rem_euclid(86_400);
    let z = days.checked_add(719_468)?;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = era.checked_mul(400)?.checked_add(yoe)? + i64::from(month <= 2);
    (FIRST_YEAR..=LAST_YEAR).contains(&year).then_some(Civil {
        year,
        month,
        day,
        seconds,
    })
}
