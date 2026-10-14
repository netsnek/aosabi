// SPDX-License-Identifier: MIT
/*
 * aoviewer-core tree plan: the jobs of a tree in path order.
 *
 * The single layout writes API.md and the split layout a directory of pages.
 */

//! The tree plan.
//!
//! [`plan_tree`] names every file a tree holds, one [`Job`] per file in the
//! byte order of the paths.
//!
//! - **The layout.** [`LayoutChoice::Auto`] selects the single layout up to
//!   [`SINGLE_BOXES`] boxes of the graph and the split layout above. The
//!   single layout writes every page into `API.md`, and the split layout
//!   writes the home page as `README.md` and every other page under `md/`,
//!   each file named by [󰀅](crate::render_markdown::page_path). The single
//!   layout gives a type no neighbourhood diagram, since the overview shows
//!   every type at that size, so it leaves the diagram blocks of a type page
//!   out in every format.
//! - **The pages.** The home page, the root pages in the order Query,
//!   Mutation, Subscription, then per cluster in the order of the clustering
//!   its page and the pages of its types, then the types no cluster holds in
//!   name order, then the four report pages, Relay, scalars, unreachable and
//!   deprecated. The Typst document takes the pages in this order, one chapter
//!   per root, per cluster and per report, so the PDF reads front to back as a
//!   manual, and `API.md` takes the same order.
//! - **The chapters.** `typ/00-overview.typ`, `typ/10-query.typ`,
//!   `typ/11-mutation.typ`, `typ/12-subscription.typ`, one
//!   `typ/20-cluster-<nn>-<slug>.typ` per cluster with the types no cluster
//!   holds last as `typ/20-cluster-<nn>-other.typ`, `typ/80-relay.typ`,
//!   `typ/81-scalars.typ`, `typ/82-unreachable.typ`, `typ/83-deprecated.typ`
//!   and `typ/99-index.typ`, so the byte order of the paths is the order
//!   `api.typ` includes them in. The cluster number counts from 1 with two
//!   digits, or with as many as the count of the cluster chapters needs.
//! - **The drawings.** One file per diagram a page draws and per theme,
//!   `svg/overview.<theme>.svg`, `svg/type/<Name>.<theme>.svg`,
//!   `svg/op/<root>.<field>.<theme>.svg` and `svg/cluster/<slug>.<theme>.svg`,
//!   a part `.<n>` before the theme. A name that only case separates from an
//!   earlier one in name order takes the infix `.2`, then `.3`: a type the
//!   number of its anchor, as its page does, and a root field the number among
//!   the fields of its root. The poster of a root is
//!   `svg/poster.<root>.<theme>.svg`, written when the whole graph from that
//!   root fits [`Budget::WHOLE`], and a note of the tree says why it is not
//!   otherwise.
//! - **The index.** Above [`LETTER_TYPES`] types the index of the split
//!   layout, the Typst index and the search file split by the first letter of
//!   a name, as `md/index/<letter>.md`, `typ/99-index/<letter>.typ` and
//!   `search-index/<letter>.json`.
//!
//! A build without the Markdown writer plans no Markdown file and one without
//! the Typst writer no Typst file, and the tree says so in a note. Every SVG
//! file and every JSON file is planned in every build.
//!
//! Every loop runs over the types, the fields of one root, the clusters or
//! the pages of one schema, whose counts
//! [`ReadLimits`](crate::limits::ReadLimits) caps, over the blocks of one
//! page and of its sections, which the page model nests one level deep, over
//! the three roots or over the two themes.

use alloc::{
    format,
    string::String,
    vec::Vec, //
};
use core::fmt::Write;

use super::options::{
    DocsOptions,
    LayoutChoice, //
};
use crate::{
    cluster::Clustering,
    id::ElementId,
    model::{
        RootKind,
        TypeId, //
    },
    page::{
        Block,
        LayoutMode,
        PageModel,
        page_of_type, //
    },
    render_svg::Theme,
    route::{
        Route,
        WholeRoot, //
    },
    schema::Schema,
    sort::name_order,
    view::{
        Budget,
        EmptyReason,
        View,
        ViewError, //
    },
};

/// The boxes of the graph up to which [`LayoutChoice::Auto`] selects the
/// single layout.
pub const SINGLE_BOXES: usize = 40;

/// The types above which the index and the search file split by first letter.
pub const LETTER_TYPES: u32 = 2000;

/// The clusters a cluster map holds at most, as `docs/aoviewer.md` section 5.5
/// derives it from the size cap.
const MAP_BOXES: usize = 120;

/// The fewest digits of the number of a cluster chapter.
const CLUSTER_DIGITS: usize = 2;

/// The slug of the chapter of the types no cluster holds.
const OTHER_SLUG: &str = "other";

/// The note of a build without the Markdown writer.
const NO_MARKDOWN: &str =
    "This build carries no Markdown writer, so the tree holds no Markdown file.";

/// The note of a build without the Typst writer.
const NO_TYPST: &str = "This build carries no Typst writer, so the tree holds no Typst file.";

/// The note of a poster asked for while the tree writes no SVG file.
const NO_POSTER_SVG: &str =
    "The poster is an SVG file, and the tree writes no SVG file, so it writes no poster.";

/// The note of Typst diagrams asked for as images of SVG files the tree does
/// not write.
const NATIVE_FALLBACK: &str = "The Typst chapters draw their diagrams natively, because the tree writes no SVG file in the theme of the document.";

/// The jobs of a tree, one per file, in the byte order of the paths.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreePlan {
    /// The jobs.
    pub jobs: Vec<Job>,
}

/// One file of a tree and what writes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Job {
    /// The path of the file inside the tree, such as `md/types/Product.md`.
    pub path: String,
    /// What the file holds.
    pub kind: JobKind,
}

/// What a file of a tree holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JobKind {
    /// `README.md` of the split layout or `API.md` of the single layout.
    Index,
    /// `md/index/<letter>.md`, the part of the index of one first letter.
    IndexLetter(u8),
    /// `search-index.json`.
    SearchFile,
    /// `search-index/<letter>.json`, the part of the search file of one
    /// first letter.
    SearchLetter(u8),
    /// `sabi-theme.typ`.
    Theme,
    /// `api.typ`, the Typst main file.
    TypstMain,
    /// A chapter under `typ/`.
    TypstChapter(Chapter),
    /// A page of the split layout under `md/`.
    Page {
        /// The route the page answers.
        route: Route,
    },
    /// One theme of a diagram a page draws, or of one part of it.
    Diagram {
        /// The route whose view the diagram draws.
        route: Route,
        /// The number of the part, from 1, when the view splits.
        part: Option<u16>,
        /// The theme.
        theme: Theme,
    },
    /// One theme of the poster of the whole graph from a root.
    Poster {
        /// The root.
        root: WholeRoot,
        /// The theme.
        theme: Theme,
    },
    /// The change log of `--since`, `md/changes.md` or, with `typst`,
    /// `typ/90-changes.typ`.
    Changes {
        /// Whether the job writes the Typst chapter.
        typst: bool,
    },
    /// `manifest.json`, which hashes every other file.
    Manifest,
}

/// A chapter of the Typst document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Chapter {
    /// `typ/00-overview.typ`, the home page.
    Overview,
    /// `typ/10-query.typ`, `typ/11-mutation.typ` or `typ/12-subscription.typ`,
    /// the page of a root.
    Root(RootKind),
    /// `typ/20-cluster-<nn>-<slug>.typ`, the page of the cluster at this
    /// position of the clustering and the pages of its types.
    Cluster(u32),
    /// `typ/20-cluster-<nn>-other.typ`, the pages of the types no cluster
    /// holds.
    Other,
    /// `typ/80-relay.typ`.
    Relay,
    /// `typ/81-scalars.typ`.
    Scalars,
    /// `typ/82-unreachable.typ`.
    Unreachable,
    /// `typ/83-deprecated.typ`.
    Deprecated,
    /// `typ/99-index.typ`, the index of the types with their pages.
    Index,
    /// `typ/99-index/<letter>.typ`, the part of the index of one first
    /// letter.
    IndexLetter(u8),
}

/// Returns the jobs of the tree of `schema` under `options`, in the byte
/// order of their paths.
///
/// `clustering` is the clustering picked from `schema`, with the declared
/// clusters of `options` when it has any. The plan builds the page of every
/// route it writes, so the parts of every diagram are the ones the pages
/// draw, and the jobs of a page whose route a view refuses are planned all
/// the same, so [`render_job`](super::render_job) reports the refusal.
///
/// # Panics
///
/// Panics when `clustering` was not picked from `schema`.
#[must_use]
pub fn plan_tree(schema: &Schema, clustering: &Clustering, options: &DocsOptions) -> TreePlan {
    Site::build(schema, clustering, options).plan()
}

/// Returns the layout the tree of `schema` writes under `choice`.
pub(super) fn layout_mode(schema: &Schema, choice: LayoutChoice) -> LayoutMode {
    match choice {
        LayoutChoice::Single => LayoutMode::Single,
        LayoutChoice::Split => LayoutMode::Split,
        LayoutChoice::Auto => {
            if schema.graph.nodes.len() <= SINGLE_BOXES {
                LayoutMode::Single
            } else {
                LayoutMode::Split
            }
        }
    }
}

/// Returns the word a file name and the manifest write for `theme`.
pub(super) fn theme_word(theme: Theme) -> &'static str {
    match theme {
        Theme::Light => "light",
        Theme::Dark => "dark",
    }
}

/// Returns the route of the page of the type called `name`.
pub(super) fn type_route(name: &str) -> Route {
    Route::Type {
        name: String::from(name),
        field: None,
        arg: None,
    }
}

/// Returns the first letter of `name` as the index and the search file
/// split by it: a letter or a digit of ASCII in lower case, and `_` for
/// every other start.
pub(super) fn first_letter(name: &str) -> u8 {
    match name.bytes().next() {
        Some(b) if b.is_ascii_alphanumeric() => b.to_ascii_lowercase(),
        _ => b'_',
    }
}

/// Returns the types of `schema` that have a page of their own and are no
/// root, in name order.
pub(super) fn own_types(schema: &Schema) -> Vec<TypeId> {
    let model = &schema.model;
    let mut own: Vec<TypeId> = model
        .types()
        .iter()
        .copied()
        .filter(|&t| {
            model.roots().kind_of(t).is_none()
                && matches!(page_of_type(schema, t), Some(Route::Type { .. }))
        })
        .collect();
    own.sort_by(|&a, &b| name_order(model, a, b));
    own
}

/// Returns the types of `schema` that a page documents, the introspection
/// types left out, in name order.
pub(super) fn documented_types(schema: &Schema) -> Vec<TypeId> {
    let model = &schema.model;
    let mut types: Vec<TypeId> = model
        .types()
        .iter()
        .copied()
        .filter(|&t| page_of_type(schema, t).is_some())
        .collect();
    types.sort_by(|&a, &b| name_order(model, a, b));
    types
}

/// The pages, the drawings and the chapters of one tree, from which the plan
/// and every job are read.
pub(super) struct Site<'a> {
    /// The schema.
    pub(super) schema: &'a Schema,
    /// The clustering picked from the schema.
    pub(super) clustering: &'a Clustering,
    /// The options.
    pub(super) options: &'a DocsOptions,
    /// The layout the options select for the schema.
    pub(super) layout: LayoutMode,
    /// Whether the tree writes Markdown, which the build has to carry.
    pub(super) md: bool,
    /// Whether the tree writes Typst, which the build has to carry.
    pub(super) typ: bool,
    /// The themes of the SVG files, empty when the tree writes none.
    pub(super) svg_themes: Vec<Theme>,
    /// The theme of the Typst document, dark when the options ask for the dark
    /// theme alone, since paper is light.
    pub(super) doc_theme: Theme,
    /// The pages in the order of the document.
    pub(super) pages: Vec<SitePage>,
    /// The drawn diagrams of the pages, sorted by their stems.
    pub(super) diagrams: Vec<SiteDiagram>,
    /// The roots whose poster the tree writes.
    pub(super) posters: Vec<WholeRoot>,
    /// The chapters of the Typst document in the order of their paths.
    pub(super) chapters: Vec<(Chapter, String)>,
    /// The types a page documents, the count the index splits at.
    pub(super) types: u32,
    /// The notes of the plan.
    pub(super) notes: Vec<String>,
}

/// One page of a tree.
pub(super) struct SitePage {
    /// The route the page answers.
    pub(super) route: Route,
    /// The chapter of the Typst document that holds it.
    pub(super) chapter: Chapter,
    /// The page, without the diagrams of a type page in the single layout,
    /// or the refusal of a view of its route.
    pub(super) model: Result<PageModel, ViewError>,
}

/// One drawn diagram of a page, a view or one part of it.
pub(super) struct SiteDiagram {
    /// The path of its files under `svg/` without the theme and the
    /// extension, such as `type/Product` or `op/query.user.2`.
    pub(super) stem: String,
    /// The route whose view the diagram draws.
    pub(super) route: Route,
    /// The number of the part, from 1, when the view splits.
    pub(super) part: Option<u16>,
}

impl<'a> Site<'a> {
    /// Builds the pages, the drawings and the chapters of the tree of
    /// `schema` under `options`.
    pub(super) fn build(
        schema: &'a Schema,
        clustering: &'a Clustering,
        options: &'a DocsOptions,
    ) -> Site<'a> {
        let layout = layout_mode(schema, options.layout);
        let md = options.formats.md && cfg!(feature = "markdown");
        let typ = options.formats.typ && cfg!(feature = "typst");
        let mut svg_themes = Vec::new();
        if options.formats.svg && options.themes.light {
            svg_themes.push(Theme::Light);
        }
        if options.formats.svg && options.themes.dark {
            svg_themes.push(Theme::Dark);
        }
        let doc_theme = if options.themes.dark && !options.themes.light {
            Theme::Dark
        } else {
            Theme::Light
        };
        let types = u32::try_from(documented_types(schema).len()).unwrap_or(u32::MAX);
        let mut site = Site {
            schema,
            clustering,
            options,
            layout,
            md,
            typ,
            svg_themes,
            doc_theme,
            pages: Vec::new(),
            diagrams: Vec::new(),
            posters: Vec::new(),
            chapters: Vec::new(),
            types,
            notes: Vec::new(),
        };
        site.pages = site.build_pages();
        site.diagrams = site.build_diagrams();
        site.chapters = site.build_chapters();
        site.posters = site.build_posters();
        site.build_notes();
        site
    }

    /// Returns whether the index and the search file split by first letter.
    pub(super) fn splits(&self) -> bool {
        self.types > LETTER_TYPES
    }

    /// Returns the jobs of the tree in the byte order of their paths.
    pub(super) fn plan(&self) -> TreePlan {
        let mut jobs = Vec::new();
        if self.md {
            markdown_jobs(self, &mut jobs);
        }
        if self.typ {
            push(&mut jobs, String::from("api.typ"), JobKind::TypstMain);
            push(&mut jobs, String::from("sabi-theme.typ"), JobKind::Theme);
            for (chapter, path) in &self.chapters {
                push(&mut jobs, path.clone(), JobKind::TypstChapter(*chapter));
            }
        }
        for d in &self.diagrams {
            for &theme in &self.svg_themes {
                let kind = JobKind::Diagram {
                    route: d.route.clone(),
                    part: d.part,
                    theme,
                };
                push(&mut jobs, diagram_path(&d.stem, theme), kind);
            }
        }
        for root in &self.posters {
            for &theme in &self.svg_themes {
                let path = poster_path(root, theme);
                let root = root.clone();
                push(&mut jobs, path, JobKind::Poster { root, theme });
            }
        }
        push(
            &mut jobs,
            String::from("search-index.json"),
            JobKind::SearchFile,
        );
        for letter in super::search_file::letters(self) {
            let path = format!("search-index/{}.json", char::from(letter));
            push(&mut jobs, path, JobKind::SearchLetter(letter));
        }
        push(&mut jobs, String::from("manifest.json"), JobKind::Manifest);
        jobs.sort_by(|a, b| a.path.as_bytes().cmp(b.path.as_bytes()));
        TreePlan { jobs }
    }

    /// Returns the drawn diagram of `route` and `part`, if a page draws it.
    pub(super) fn diagram(&self, route: &Route, part: Option<u16>) -> Option<&SiteDiagram> {
        let stem = self.stem(route, part)?;
        self.diagrams
            .binary_search_by(|d| d.stem.as_bytes().cmp(stem.as_bytes()))
            .ok()
            .map(|i| &self.diagrams[i])
    }

    /// Returns the path of the chapter `chapter`.
    pub(super) fn chapter_path(&self, chapter: Chapter) -> String {
        let clusters = &self.clustering.clusters;
        let width = digits(clusters.len() + 1).max(CLUSTER_DIGITS);
        match chapter {
            Chapter::Overview => String::from("typ/00-overview.typ"),
            Chapter::Root(root) => {
                let n = match root {
                    RootKind::Query => 10,
                    RootKind::Mutation => 11,
                    RootKind::Subscription => 12,
                };
                format!("typ/{n}-{}.typ", root.word())
            }
            Chapter::Cluster(i) => {
                let slug = clusters.get(i as usize).map_or(OTHER_SLUG, |c| &c.slug);
                format!("typ/20-cluster-{:0width$}-{slug}.typ", i as usize + 1)
            }
            Chapter::Other => {
                let n = clusters.len() + 1;
                format!("typ/20-cluster-{n:0width$}-{OTHER_SLUG}.typ")
            }
            Chapter::Relay => String::from("typ/80-relay.typ"),
            Chapter::Scalars => String::from("typ/81-scalars.typ"),
            Chapter::Unreachable => String::from("typ/82-unreachable.typ"),
            Chapter::Deprecated => String::from("typ/83-deprecated.typ"),
            Chapter::Index => String::from("typ/99-index.typ"),
            Chapter::IndexLetter(letter) => format!("typ/99-index/{}.typ", char::from(letter)),
        }
    }

    /// Returns the stem of the files of the diagram of `route` and `part`, or
    /// `None` for a route no file draws.
    pub(super) fn stem(&self, route: &Route, part: Option<u16>) -> Option<String> {
        let model = &self.schema.model;
        let mut stem = match route {
            Route::Home => String::from("overview"),
            Route::Type { name, .. } => {
                let ty = model.type_by_name(name)?;
                format!("type/{}", numbered(name, self.type_number(ty)))
            }
            Route::Op { root, field } => {
                let n = self.op_number(*root, field);
                format!("op/{}.{}", root.word(), numbered(field, n))
            }
            Route::Cluster { slug } => format!("cluster/{slug}"),
            _ => return None,
        };
        if let Some(n) = part {
            // Writing into a `String` cannot fail.
            let _ = write!(stem, ".{n}");
        }
        Some(stem)
    }

    /// Returns the number of the case collision of the type `ty`, the number
    /// its anchor carries.
    ///
    /// The anchor of a type is `type-` and its name in lower case, with `-2`
    /// and up for a name that only case separates from an earlier one, and a
    /// name holds no hyphen, so the page and the drawings of a type take one
    /// number.
    fn type_number(&self, ty: TypeId) -> Option<String> {
        let model = &self.schema.model;
        let anchor = self.schema.anchors.of(&ElementId::of_type(model, ty));
        let (_, n) = anchor.strip_prefix("type-")?.rsplit_once('-')?;
        Some(String::from(n))
    }

    /// Returns the number of the case collision of the field `field` of the
    /// root `root`, one more than the fields of that root that only case
    /// separates from it and that come before it in name order, or `None`
    /// for the first.
    fn op_number(&self, root: RootKind, field: &str) -> Option<String> {
        let model = &self.schema.model;
        let ty = model.roots().get(root)?;
        let before = model
            .ty(ty)
            .fields
            .iter()
            .map(|&f| model.name(model.field(f).name))
            .filter(|&n| n != field && n.eq_ignore_ascii_case(field) && n < field)
            .count();
        (before > 0).then(|| format!("{}", before + 1))
    }

    /// Returns the pages of the tree in the order of the document.
    fn build_pages(&self) -> Vec<SitePage> {
        let (schema, model) = (self.schema, &self.schema.model);
        let mut pages = Vec::new();
        self.push_page(&mut pages, Route::Home, Chapter::Overview);
        for root in [RootKind::Query, RootKind::Mutation, RootKind::Subscription] {
            if let Some(ty) = model.roots().get(root) {
                let route = type_route(model.name(model.ty(ty).name));
                self.push_page(&mut pages, route, Chapter::Root(root));
            }
        }
        let own = own_types(schema);
        let mut is_own = Vec::new();
        is_own.resize(model.types().len(), false);
        for &t in &own {
            is_own[t.index()] = true;
        }
        for (i, cluster) in self.clustering.clusters.iter().enumerate() {
            let chapter = Chapter::Cluster(u32::try_from(i).unwrap_or(u32::MAX));
            let slug = cluster.slug.clone();
            self.push_page(&mut pages, Route::Cluster { slug }, chapter);
            for &t in &cluster.types {
                if is_own.get(t.index()).copied().unwrap_or(false) {
                    is_own[t.index()] = false;
                    let route = type_route(model.name(model.ty(t).name));
                    self.push_page(&mut pages, route, chapter);
                }
            }
        }
        for t in own.into_iter().filter(|t| is_own[t.index()]) {
            let route = type_route(model.name(model.ty(t).name));
            self.push_page(&mut pages, route, Chapter::Other);
        }
        let reports = [
            (Route::Relay, Chapter::Relay),
            (Route::Scalars, Chapter::Scalars),
            (Route::Unreachable, Chapter::Unreachable),
            (Route::Deprecated, Chapter::Deprecated),
        ];
        for (route, chapter) in reports {
            self.push_page(&mut pages, route, chapter);
        }
        pages
    }

    /// Appends the page of `route` in `chapter`.
    fn push_page(&self, pages: &mut Vec<SitePage>, route: Route, chapter: Chapter) {
        let mut model = PageModel::of(self.schema, self.clustering, &route, &self.options.view);
        if let Ok(page) = &mut model {
            let type_page = page.kind.is_some() && !matches!(chapter, Chapter::Root(_));
            if self.layout == LayoutMode::Single && type_page {
                page.blocks.retain(|b| !matches!(b, Block::Diagram { .. }));
            }
        }
        pages.push(SitePage {
            route,
            chapter,
            model,
        });
    }

    /// Returns the drawn diagrams of the pages, one per stem, sorted by
    /// stem.
    fn build_diagrams(&self) -> Vec<SiteDiagram> {
        let mut found = Vec::new();
        for page in &self.pages {
            if let Ok(model) = &page.model {
                self.collect(&model.blocks, &mut found);
                for block in &model.blocks {
                    if let Block::Section { blocks, .. } = block {
                        self.collect(blocks, &mut found);
                    }
                }
            }
        }
        found.sort_by(|a: &SiteDiagram, b| a.stem.as_bytes().cmp(b.stem.as_bytes()));
        found.dedup_by(|a, b| a.stem == b.stem);
        found
    }

    /// Appends the drawn diagrams of `blocks` that a file draws.
    fn collect(&self, blocks: &[Block], found: &mut Vec<SiteDiagram>) {
        for block in blocks {
            if let Block::Diagram {
                route,
                part,
                skipped: None,
            } = block
                && let Some(stem) = self.stem(route, *part)
            {
                found.push(SiteDiagram {
                    stem,
                    route: route.clone(),
                    part: *part,
                });
            }
        }
    }

    /// Returns the chapters of the Typst document in the order of their
    /// paths.
    fn build_chapters(&self) -> Vec<(Chapter, String)> {
        let mut chapters: Vec<Chapter> = Vec::new();
        for page in &self.pages {
            if chapters.last() != Some(&page.chapter) {
                chapters.push(page.chapter);
            }
        }
        chapters.push(Chapter::Index);
        if self.splits() {
            for letter in super::index::typ_letters(self) {
                chapters.push(Chapter::IndexLetter(letter));
            }
        }
        let mut out: Vec<(Chapter, String)> = chapters
            .into_iter()
            .map(|c| (c, self.chapter_path(c)))
            .collect();
        out.sort_by(|a, b| a.1.as_bytes().cmp(b.1.as_bytes()));
        out.dedup_by(|a, b| a.1 == b.1);
        out
    }

    /// Returns the roots whose whole graph fits [`Budget::WHOLE`], when the
    /// options ask for the poster and the tree writes SVG.
    fn build_posters(&self) -> Vec<WholeRoot> {
        let mut posters = Vec::new();
        if !self.options.poster || self.svg_themes.is_empty() {
            return posters;
        }
        for (kind, root) in [
            (RootKind::Query, WholeRoot::Query),
            (RootKind::Mutation, WholeRoot::Mutation),
            (RootKind::Subscription, WholeRoot::Subscription),
        ] {
            if self.schema.model.roots().get(kind).is_none() {
                continue;
            }
            if poster_reason(self, &root).is_none() {
                posters.push(root);
            }
        }
        posters
    }

    /// Records the notes of the plan: a writer the build lacks, a poster
    /// that is not written and why, Typst images of files the tree does not
    /// write, and a cluster map larger than its bound.
    fn build_notes(&mut self) {
        let options = self.options;
        if options.formats.md && !self.md {
            self.notes.push(String::from(NO_MARKDOWN));
        }
        if options.formats.typ && !self.typ {
            self.notes.push(String::from(NO_TYPST));
        }
        if options.poster && self.svg_themes.is_empty() {
            self.notes.push(String::from(NO_POSTER_SVG));
        }
        if options.poster && !self.svg_themes.is_empty() {
            self.poster_notes();
        }
        let svg_images = options.typst_diagrams == super::options::TypstDiagrams::Svg;
        if self.typ && svg_images && !self.svg_themes.contains(&self.doc_theme) {
            self.notes.push(String::from(NATIVE_FALLBACK));
        }
        let clusters = self.clustering.clusters.len();
        if clusters > MAP_BOXES {
            self.notes.push(format!(
                "The cluster map holds {clusters} clusters, more than the {MAP_BOXES} boxes docs/aoviewer.md section 5.5 bounds it to."
            ));
        }
    }

    /// Records the note of every declared root whose poster is not written.
    fn poster_notes(&mut self) {
        for (kind, root, word) in [
            (RootKind::Query, WholeRoot::Query, "Query"),
            (RootKind::Mutation, WholeRoot::Mutation, "Mutation"),
            (
                RootKind::Subscription,
                WholeRoot::Subscription,
                "Subscription",
            ),
        ] {
            if self.schema.model.roots().get(kind).is_none() {
                continue;
            }
            let note = match poster_reason(self, &root) {
                None => continue,
                Some(EmptyReason::OverBudget { boxes, edges }) => format!(
                    "The whole graph from {word} holds {boxes} boxes and {edges} edges, more than {} of a poster, so the tree writes no poster of it.",
                    crossed_bound(boxes, edges)
                ),
                Some(_) => format!(
                    "The whole graph from {word} draws no box, so the tree writes no poster of it."
                ),
            };
            self.notes.push(note);
        }
    }
}

/// Appends the jobs of the Markdown files of the layout of `site`.
#[cfg(feature = "markdown")]
fn markdown_jobs(site: &Site<'_>, jobs: &mut Vec<Job>) {
    use crate::render_markdown::page_path;

    if let Some(path) = page_path(site.schema, &Route::Home, site.layout) {
        push(jobs, path, JobKind::Index);
    }
    if site.layout == LayoutMode::Single {
        return;
    }
    for page in site.pages.iter().filter(|p| p.route != Route::Home) {
        if let Some(path) = page_path(site.schema, &page.route, LayoutMode::Split) {
            let route = page.route.clone();
            push(jobs, path, JobKind::Page { route });
        }
    }
    for letter in super::index::md_letters(site) {
        let path = format!("md/index/{}.md", char::from(letter));
        push(jobs, path, JobKind::IndexLetter(letter));
    }
}

/// Appends nothing, since the build carries no Markdown writer.
#[cfg(not(feature = "markdown"))]
fn markdown_jobs(_site: &Site<'_>, _jobs: &mut Vec<Job>) {}

/// Returns the bound of [`Budget::WHOLE`] that `boxes` and `edges` cross,
/// both when both are crossed.
fn crossed_bound(boxes: u32, edges: u32) -> String {
    let whole = Budget::WHOLE;
    match (
        boxes > u32::from(whole.boxes),
        edges > u32::from(whole.edges),
    ) {
        (true, true) => format!("the {} boxes and {} edges", whole.boxes, whole.edges),
        (true, false) => format!("the {} boxes", whole.boxes),
        (false, _) => format!("the {} edges", whole.edges),
    }
}

/// Returns why the whole graph from `root` makes no poster, or `None` when
/// it fits [`Budget::WHOLE`].
///
/// A view the core refuses counts as one that draws no box.
fn poster_reason(site: &Site<'_>, root: &WholeRoot) -> Option<EmptyReason> {
    let route = Route::Whole { root: root.clone() };
    let budget = Budget::WHOLE;
    match View::of(
        site.schema,
        site.clustering,
        &route,
        &site.options.view,
        &budget,
    ) {
        Ok(view) => view.empty,
        Err(_) => Some(EmptyReason::Unreachable),
    }
}

/// Returns the path of the file of the diagram `stem` in `theme`.
pub(super) fn diagram_path(stem: &str, theme: Theme) -> String {
    format!("svg/{stem}.{}.svg", theme_word(theme))
}

/// Returns the path of the poster of `root` in `theme`.
pub(super) fn poster_path(root: &WholeRoot, theme: Theme) -> String {
    let word = match root {
        WholeRoot::Query => "query",
        WholeRoot::Mutation => "mutation",
        WholeRoot::Subscription => "subscription",
        WholeRoot::All => "all",
    };
    format!("svg/poster.{word}.{}.svg", theme_word(theme))
}

/// Returns `name` with the infix `.<n>` of a case collision when `n` is
/// given.
fn numbered(name: &str, n: Option<String>) -> String {
    match n {
        Some(n) => format!("{name}.{n}"),
        None => String::from(name),
    }
}

/// Returns the decimal digits of `n`.
fn digits(n: usize) -> usize {
    let mut count = 1;
    let mut rest = n / 10;
    // The loop runs once per decimal digit of `n`, at most 20 times.
    while rest > 0 {
        count += 1;
        rest /= 10;
    }
    count
}

/// Appends the job of `path` holding `kind`.
fn push(jobs: &mut Vec<Job>, path: String, kind: JobKind) {
    jobs.push(Job { path, kind });
}
