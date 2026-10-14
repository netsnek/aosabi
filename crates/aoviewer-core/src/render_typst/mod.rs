// SPDX-License-Identifier: MIT
/*
 * aoviewer-core render_typst: the Typst writers of a page and of a scene.
 *
 * Every string of the schema reaches Typst as a string literal in code mode and
 * never as markup.
 */

//! The Typst writers.
//!
//! [`render_typst_page`] writes one [`PageModel`] as Typst, the page of a
//! chapter of the document the generator writes, and [`render_typst_scene`]
//! writes one [󰀃](crate::scene::Scene) as the `#sv-diagram(..)` call of a
//! diagram. The generated files hold data only, and every function they call
//! lives in the theme, [`SABI_THEME`], so the whole visual language is restyled
//! by editing that one file.
//!
//! A page is markup that holds nothing but one call of the theme per line,
//! `#sv-title(..)`, `#sv-desc(..)`, `#sv-table(..)` and their kin, and every
//! text inside a call is a string literal written by [`typst_str`]. A
//! description enters as the calls [`to_typst`] writes, emphasis, links and
//! tables as functions of the theme with string arguments, so markup of a
//! description never reaches the compiler as syntax. A chapter opens with the
//! import of the theme, which the writer of the chapter adds, since an included
//! file sees no binding of the file that includes it.
//!
//! The page follows the order of the model, the same structure the Markdown
//! writer writes.
//!
//! - Every chapter joins one document, so a link is a label and never a file,
//!   and every page carries the labels the Markdown writer puts in the single
//!   layout, where every page shares one file: the anchor of every element it
//!   documents, `args-` and `details-` before the anchor of a field and of an
//!   element, and `home`, `clusters`, `scalars`, `relay`, `unreachable`,
//!   `deprecated` and `cluster-<slug>` for a page that documents no element. A
//!   label is `label("...")` of a string, never the `<..>` syntax, and an
//!   anchor is an empty metadata element that carries it,
//!   `#sv-anchor(label("..."))`, before the heading or inside the cell of its
//!   element. A link to an element whose page the tree does not write is no
//!   link, so every label a page links to is one the document holds, and a dead
//!   label is a compile error that `76_docs_pdf` counts.
//! - The title of a page is a heading of the outline, level 1 for a page that
//!   opens a chapter and level 2 for a type page and the pages that follow a
//!   chapter opener, since the type pages of a cluster follow the page of the
//!   cluster in its chapter. The heading of a table, a name grid, the details
//!   or the section of the arguments of a field sits one level deeper and stays
//!   out of the outline, while the section of a root field is a heading of the
//!   outline, the navigation of an operation chapter.
//! - A description is `#sv-desc(blocks, level: n)`, which moves its headings
//!   below the heading `n` that holds it, and a table cell holds the first
//!   paragraph of a description as `sv-desc(blocks, cell: true)`, with no level
//!   and its headings shown as strong text, followed by the link `more` to its
//!   entry in the details.
//! - A deprecation cell carries the filled mark of the directive or the hollow
//!   mark of a description tag before its reason.
//! - A diagram of a page is written by the caller of
//!   [`render_typst_page_with_diagrams`], the scene through
//!   [`render_typst_scene`] or the light SVG file through `sv-image` of the
//!   theme, and a skipped diagram is its sentence.
//! - The link into the live viewer is written only when the viewer URL names
//!   the scheme `http` or `https`, since a printed document has no base a
//!   relative link could start from.
//!
//! Every loop runs over the blocks, rows, cells, parts or names of one page,
//! whose counts the model of the schema caps, over the crumbs of one
//! breadcrumb, or over the characters of one string. The page model nests a
//! section one level deep, so the writer of a section calls the writer of its
//! blocks once.
//!
//! [Typst scripting]: https://typst.app/docs/reference/scripting/

mod scene;
mod string;

pub use scene::render_typst_scene;
pub use string::typst_str;

use alloc::{
    format,
    string::String,
    vec::Vec, //
};
use core::fmt::Write;

use crate::{
    commonmark::{
        parse,
        to_typst, //
    },
    id::ElementId,
    limits::ReadLimits,
    page::{
        Block,
        Cell,
        DescriptionSource,
        Links,
        PageModel,
        TypeRefPart,
        kind_word,
        page_of,
        page_of_type, //
    },
    route::{
        Options,
        Route, //
    },
    schema::Schema,
    view::{
        Budget,
        EmptyReason, //
    },
};

/// The theme file, `include_str!("sabi-theme.typ")`.
///
/// It is the visual language as Typst functions: the page setup, the two font
/// faces by name, the accents and tints of the six kinds in both themes,
/// `sv-diagram` drawing boxes, rows, curves, chips, stubs and trunks from data,
/// `sv-box-link` over a label, `sv-stub` printing a page number through
/// `context` and `query`, and the text functions a page and a description call.
/// It carries no schema text.
pub const SABI_THEME: &str = include_str!("sabi-theme.typ");

/// The prefix of the anchor of a field, which the anchor of its arguments
/// section replaces.
const FIELD_PREFIX: &str = "field-";

/// The prefix of the anchor of the arguments section of a field, the one of the
/// Markdown writer.
const ARGS_PREFIX: &str = "args-";

/// The prefix of the anchor of an entry of the details, the one of the Markdown
/// writer.
const DETAILS_PREFIX: &str = "details-";

/// The label of the home page, which a link to the cluster map jumps to as
/// well, since the home page holds the table of the clusters.
const HOME_LABEL: &str = "home";

/// The heading level of a page that opens a chapter.
const CHAPTER_LEVEL: u8 = 1;

/// The heading level of a page inside a chapter.
const PAGE_LEVEL: u8 = 2;

/// The sentence of a type with fewer than two neighbours, the sentence of the
/// Markdown writer.
const FEW_NEIGHBOURS: &str =
    "This type has fewer than two neighbours, so it has no neighbourhood diagram.";

/// The sentence of a view the graph holds no box for, the sentence of the
/// Markdown writer.
const NO_BOX: &str = "This page has no diagram, because the graph holds no box for it.";

/// The sentence of a name grid without a name.
const EMPTY_GRID: &str = "None.";

/// The label of the link into the live viewer.
const VIEWER_LABEL: &str = "Open this page in the viewer";

/// The label of the link from a table cell to the whole description in the
/// details.
const MORE_LABEL: &str = "more";

/// The words after the description only the intermediate representation
/// carries.
const IR_ONLY: &str = " (from ir.json, not served)";

/// The words after the reason of a deprecation a description tag marks.
const FROM_TAG: &str = " (from the description)";

/// The reason a deprecation without one shows.
const NO_REASON: &str = "Deprecated";

/// The language of the code block of a signature.
const SIGNATURE_LANG: &str = "graphql";

/// Writes `page` as Typst into `out`, without the drawings of its diagrams.
///
/// It is [`render_typst_page_with_diagrams`] with a diagram writer that
/// writes nothing, so a skipped diagram is still its sentence and a drawn
/// one leaves no trace.
///
/// # Panics
///
/// Panics when a name of `page` comes from another model than the one of
/// `schema`.
pub fn render_typst_page(page: &PageModel, schema: &Schema, links: &Links, out: &mut String) {
    render_typst_page_with_diagrams(page, schema, links, &|_, _, _| {}, out);
}

/// Writes `page` as Typst into `out`, with every diagram written by
/// `diagram`.
///
/// `schema` is the bundle the page was built from, which names the labels,
/// and `links` carries the viewer URL. The layout and the file of `links`
/// change nothing, since every chapter joins one document and a link is a
/// label. `diagram` writes the diagram of a route and a part as markup at
/// the start of a line, the `#sv-diagram(..)` call of
/// [`render_typst_scene`] or a `#sv-image(..)` of the light SVG file, or
/// nothing when the tree draws none, and a skipped diagram is written as its
/// sentence without asking it. When `out` already holds text, such as the
/// pages before it in one chapter, the page starts on a line of its own. The
/// page ends with a line feed.
///
/// # Panics
///
/// Panics when a name of `page` comes from another model than the one of
/// `schema`.
pub fn render_typst_page_with_diagrams(
    page: &PageModel,
    schema: &Schema,
    links: &Links,
    diagram: &dyn Fn(&Route, Option<u16>, &mut String),
    out: &mut String,
) {
    end_line(out);
    let mut writer = Writer {
        schema,
        links,
        diagram,
        out,
        title: &page.title,
    };
    writer.page(page);
}

/// An array literal being written, which counts its elements.
///
/// An array of one element keeps a trailing comma, since `(x)` is the
/// parenthesized `x` and `(x,)` the array.
///
/// [Typst array]: https://typst.app/docs/reference/foundations/array/
struct Seq {
    /// The elements written so far.
    count: u32,
}

impl Seq {
    /// Opens an array literal in `out`.
    fn open(out: &mut String) -> Seq {
        out.push('(');
        Seq { count: 0 }
    }

    /// Starts the next element in `out`.
    fn next(&mut self, out: &mut String) {
        if self.count > 0 {
            out.push_str(", ");
        }
        self.count = self.count.saturating_add(1);
    }

    /// Closes the array literal in `out`.
    fn close(self, out: &mut String) {
        if self.count == 1 {
            out.push(',');
        }
        out.push(')');
    }
}

/// Ends the line `out` stands on, when it holds text.
fn end_line(out: &mut String) {
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
}

/// Writes `n` in decimal.
fn push_number(n: impl core::fmt::Display, out: &mut String) {
    // Writing into a `String` cannot fail.
    let _ = write!(out, "{n}");
}

/// Writes the label `name` as `label("name")`.
fn push_label(name: &str, out: &mut String) {
    out.push_str("label(");
    typst_str(name, out);
    out.push(')');
}

/// Writes `text` as a code span, linked to `target` when it names a label.
fn push_code_link(text: &str, target: Option<&str>, out: &mut String) {
    if let Some(target) = target {
        out.push_str("sv-link(");
        push_label(target, out);
        out.push_str(", (");
    }
    out.push_str("sv-code(");
    typst_str(text, out);
    out.push(')');
    if target.is_some() {
        out.push_str(",))");
    }
}

/// Writes `text` as text, linked to `target` when it names a label.
fn push_text_link(text: &str, target: Option<&str>, out: &mut String) {
    let Some(target) = target else {
        typst_str(text, out);
        return;
    };
    out.push_str("sv-link(");
    push_label(target, out);
    out.push_str(", (");
    typst_str(text, out);
    out.push_str(",))");
}

/// Returns whether the tree writes the page of `route`.
///
/// It writes the home page, the pages of the clusters and the four report
/// pages, the root page of a declared root with the pages of its fields, and
/// the page that documents a declared type, which an introspection type has
/// none of. It writes no page of a whole graph, a path or a search, and the
/// cluster map shares the home page.
fn documented(schema: &Schema, route: &Route) -> bool {
    let model = &schema.model;
    match route {
        Route::Home
        | Route::Clusters
        | Route::Cluster { .. }
        | Route::Unreachable
        | Route::Deprecated
        | Route::Scalars
        | Route::Relay => true,
        Route::Type { name, .. } => model
            .type_by_name(name)
            .and_then(|ty| page_of_type(schema, ty))
            .is_some(),
        Route::Op { root, .. } => model.roots().get(*root).is_some(),
        Route::Whole { .. } | Route::Path { .. } | Route::Search { .. } => false,
    }
}

/// Returns the label a page that documents no element carries, since every
/// page shares one document.
fn page_label(route: &Route) -> Option<String> {
    let word = match route {
        Route::Home => HOME_LABEL,
        Route::Clusters => "clusters",
        Route::Cluster { slug } => return Some(format!("cluster-{slug}")),
        Route::Unreachable => "unreachable",
        Route::Deprecated => "deprecated",
        Route::Scalars => "scalars",
        Route::Relay => "relay",
        _ => return None,
    };
    Some(String::from(word))
}

/// Returns the anchor of the type called `name`, or `None` when the schema
/// declares no such type.
fn type_anchor(schema: &Schema, name: &str) -> Option<String> {
    let model = &schema.model;
    let ty = model.type_by_name(name)?;
    let anchor = schema.anchors.of(&ElementId::of_type(model, ty));
    (!anchor.is_empty()).then(|| String::from(anchor))
}

/// Returns the label a link to the page of `route` jumps to, or `None` when
/// the tree writes no such page.
///
/// A type page and a root page are the anchor of their type, the page of a
/// root field the anchor of the field on its root page, the cluster map the
/// home page, and every other page its own label.
fn route_label(schema: &Schema, route: &Route) -> Option<String> {
    if !documented(schema, route) {
        return None;
    }
    let model = &schema.model;
    match route {
        Route::Type { name, .. } => type_anchor(schema, name),
        Route::Op { root, field } => {
            let ty = model.roots().get(*root)?;
            let f = model
                .ty(ty)
                .fields
                .iter()
                .copied()
                .find(|&f| model.name(model.field(f).name) == field)?;
            let anchor = schema.anchors.of(&ElementId::of_field(model, f));
            (!anchor.is_empty()).then(|| String::from(anchor))
        }
        Route::Clusters => Some(String::from(HOME_LABEL)),
        _ => page_label(route),
    }
}

/// Returns the label a link to the element `id` jumps to, or `None` when no
/// page of the tree documents it.
///
/// A cluster is the label of its page and every other element its anchor.
fn element_label(schema: &Schema, id: &ElementId) -> Option<String> {
    let route = page_of(schema, id)?;
    if let ElementId::Cluster(_) = id {
        return route_label(schema, &route);
    }
    if !documented(schema, &route) {
        return None;
    }
    let anchor = schema.anchors.of(id);
    (!anchor.is_empty()).then(|| String::from(anchor))
}

/// Returns the heading level of the title of the page of `route`.
///
/// The home page, a root page, a cluster page and a report page open a
/// chapter, and a type page follows the page of its cluster.
fn page_level(schema: &Schema, route: &Route) -> u8 {
    let model = &schema.model;
    match route {
        Route::Type { name, .. } => {
            let root = model
                .type_by_name(name)
                .is_some_and(|ty| model.roots().kind_of(ty).is_some());
            if root { CHAPTER_LEVEL } else { PAGE_LEVEL }
        }
        Route::Op { .. } | Route::Whole { .. } | Route::Path { .. } | Route::Search { .. } => {
            PAGE_LEVEL
        }
        _ => CHAPTER_LEVEL,
    }
}

/// Returns whether the viewer URL `url` may be linked from a printed
/// document, which it may when it names the scheme `http` or `https` and
/// holds no space and no ASCII control character.
///
/// A scheme is a letter followed by letters, digits, `+`, `-` and `.` up to the
/// first colon. A URL that holds a space or an ASCII control character, a byte
/// up to 0x1f or 0x7f, is refused whole. A reader strips a C0 control or a
/// space at either end and a tab, a line feed or a carriage return anywhere
/// before it reads the scheme, and the delete byte is refused with them as the
/// last ASCII control.
fn web_url(url: &str) -> bool {
    if url.bytes().any(|b| b <= b' ' || b == 0x7f) {
        return false;
    }
    url.split_once(':').is_some_and(|(scheme, _)| {
        scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https")
    })
}

/// Returns the sentence of a diagram skipped for `reason`, the sentence of the
/// Markdown writer.
fn skipped_sentence(reason: EmptyReason) -> String {
    match reason {
        EmptyReason::FewerThanTwoNeighbours => String::from(FEW_NEIGHBOURS),
        EmptyReason::Unreachable => String::from(NO_BOX),
        EmptyReason::OverBudget { boxes, edges } => format!(
            "This page has no diagram, because its view holds {boxes} boxes and {edges} edges, more than the {} boxes and {} edges a page holds.",
            Budget::PAGE.boxes,
            Budget::PAGE.edges
        ),
    }
}

/// Returns the name of the element `id` as the heading of its entry in the
/// details, the last name of its id, which for an edge is the one of its
/// source.
fn element_name(schema: &Schema, id: &ElementId) -> String {
    let element = match id {
        ElementId::Edge(source, _) => source,
        _ => id,
    };
    let name = match element {
        ElementId::Type(t) => *t,
        ElementId::Field(_, f) | ElementId::InputField(_, f) | ElementId::Value(_, f) => *f,
        ElementId::Arg(_, _, a) => *a,
        ElementId::Cluster(slug) => return slug.clone(),
        ElementId::Edge(..) => return String::new(),
    };
    String::from(schema.model.name(name))
}

/// The state of one call of [`render_typst_page_with_diagrams`].
struct Writer<'a> {
    /// The schema the page was built from.
    schema: &'a Schema,
    /// The viewer URL.
    links: &'a Links,
    /// The writer of the diagram of a route and a part.
    diagram: &'a dyn Fn(&Route, Option<u16>, &mut String),
    /// The text written.
    out: &'a mut String,
    /// The title of the page.
    title: &'a str,
}

impl Writer<'_> {
    /// Writes the page.
    fn page(&mut self, page: &PageModel) {
        let level = page_level(self.schema, &page.route);
        self.breadcrumb(&page.breadcrumb);
        let anchor = match (&page.route, page.kind) {
            (Route::Type { name, .. }, Some(_)) => type_anchor(self.schema, name),
            (route, _) => page_label(route),
        };
        if let Some(anchor) = anchor {
            self.anchor(&anchor);
        }
        self.out.push_str("#sv-title(");
        push_number(level, self.out);
        self.out.push_str(", (");
        typst_str(&page.title, self.out);
        self.out.push_str(",)");
        if let Some(kind) = page.kind {
            self.out.push_str(", kind: ");
            typst_str(kind_word(kind), self.out);
        }
        self.out.push_str(")\n");
        self.blocks(&page.blocks, level);
    }

    /// Writes the breadcrumb line, every crumb a link to its page.
    fn breadcrumb(&mut self, crumbs: &[(String, Route)]) {
        if crumbs.is_empty() {
            return;
        }
        self.out.push_str("#sv-crumbs(");
        let mut seq = Seq::open(self.out);
        for (text, route) in crumbs {
            seq.next(self.out);
            let target = route_label(self.schema, route);
            push_text_link(text, target.as_deref(), self.out);
        }
        seq.close(self.out);
        self.out.push_str(")\n");
    }

    /// Writes the anchor that carries the label `name`.
    fn anchor(&mut self, name: &str) {
        self.out.push_str("#sv-anchor(");
        push_label(name, self.out);
        self.out.push_str(")\n");
    }

    /// Writes a heading of `level` outside the outline that reads `text`.
    fn heading(&mut self, level: u8, text: &str) {
        self.out.push_str("#sv-h(");
        push_number(level, self.out);
        self.out.push_str(", (");
        typst_str(text, self.out);
        self.out.push_str(",))\n");
    }

    /// Writes a heading of `level` that reads `text` as a code span, in the
    /// outline when `outlined` holds.
    fn code_heading(&mut self, level: u8, text: &str, outlined: bool) {
        self.out
            .push_str(if outlined { "#sv-title(" } else { "#sv-h(" });
        push_number(level, self.out);
        self.out.push_str(", (sv-code(");
        typst_str(text, self.out);
        self.out.push_str("),))\n");
    }

    /// Writes the heading of a table or a name grid under a heading of
    /// `level`, which a block titled as its page leaves out, since the title
    /// of the page says it already.
    fn block_heading(&mut self, level: u8, text: &str) {
        if text != self.title {
            self.heading(level.saturating_add(1), text);
        }
    }

    /// Writes a paragraph of the one text `text`.
    fn note(&mut self, text: &str) {
        self.out.push_str("#sv-p((");
        typst_str(text, self.out);
        self.out.push_str(",))\n");
    }

    /// Writes the blocks of a page or of a section under a heading of
    /// `level`.
    fn blocks(&mut self, blocks: &[Block], level: u8) {
        for block in blocks {
            self.block(block, level);
        }
    }

    /// Writes one block under a heading of `level`.
    fn block(&mut self, block: &Block, level: u8) {
        match block {
            Block::Description { text } => self.description(text, level),
            Block::Diagram {
                route,
                part,
                skipped,
            } => self.diagram(route, *part, *skipped),
            Block::Table {
                title,
                columns,
                rows,
            } => {
                self.block_heading(level, title);
                self.table(columns, rows);
            }
            Block::Details { title, entries } => self.details(title, entries, level),
            Block::Grid { title, names } => self.grid(title, names, level),
            Block::Note { text } => self.note(text),
            Block::Signature { text } => {
                self.out.push_str("#sv-codeblock(");
                typst_str(SIGNATURE_LANG, self.out);
                self.out.push_str(", ");
                typst_str(text, self.out);
                self.out.push_str(")\n");
            }
            Block::Section { title, id, blocks } => self.section(title, id, blocks, level),
            Block::ViewerLink { route } => self.viewer_link(route),
        }
    }

    /// Writes a description under a heading of `level`, its blocks as the
    /// calls [`to_typst`] writes with their headings below `level`.
    fn description(&mut self, text: &str, level: u8) {
        let doc = parse(text, &ReadLimits::DEFAULT);
        self.out.push_str("#sv-desc(");
        to_typst(&doc, text, self.out);
        self.out.push_str(", level: ");
        push_number(level, self.out);
        self.out.push_str(")\n");
    }

    /// Writes a diagram through the writer of the caller, or the sentence
    /// that says why it is skipped.
    fn diagram(&mut self, route: &Route, part: Option<u16>, skipped: Option<EmptyReason>) {
        if let Some(reason) = skipped {
            self.note(&skipped_sentence(reason));
            return;
        }
        (self.diagram)(route, part, self.out);
        end_line(self.out);
    }

    /// Writes a table with its header row and its rows, every cell an array
    /// of inlines.
    fn table(&mut self, columns: &[&'static str], rows: &[Vec<Cell>]) {
        self.out.push_str("#sv-table((), ");
        let mut header = Seq::open(self.out);
        for column in columns {
            header.next(self.out);
            self.out.push('(');
            typst_str(column, self.out);
            self.out.push_str(",)");
        }
        header.close(self.out);
        self.out.push_str(", ");
        let mut body = Seq::open(self.out);
        for row in rows {
            body.next(self.out);
            let mut cells = Seq::open(self.out);
            for cell in row {
                cells.next(self.out);
                self.cell(cell);
            }
            cells.close(self.out);
        }
        body.close(self.out);
        self.out.push_str(")\n");
    }

    /// Writes one cell of a table as an array of inlines.
    fn cell(&mut self, cell: &Cell) {
        let schema = self.schema;
        let mut seq = Seq::open(self.out);
        match cell {
            Cell::Text(text) => {
                if !text.is_empty() {
                    seq.next(self.out);
                    typst_str(text, self.out);
                }
            }
            Cell::Name { text, id } => {
                let anchor = schema.anchors.of(id);
                if !anchor.is_empty() {
                    seq.next(self.out);
                    self.out.push_str("sv-anchor(");
                    push_label(anchor, self.out);
                    self.out.push(')');
                }
                seq.next(self.out);
                push_code_link(text, None, self.out);
            }
            Cell::TypeRef { parts } => {
                seq.next(self.out);
                self.type_ref(parts);
            }
            Cell::Description {
                first,
                more,
                source,
            } => self.description_cell(&mut seq, first, more.as_ref(), *source),
            Cell::Deprecation { reason, from_tag } => {
                seq.next(self.out);
                self.out.push_str("sv-mark(");
                typst_str(if *from_tag { "tag" } else { "deprecated" }, self.out);
                self.out.push(')');
                seq.next(self.out);
                let reason = reason.as_deref().filter(|r| !r.trim().is_empty());
                typst_str(&format!(" {}", reason.unwrap_or(NO_REASON)), self.out);
                if *from_tag {
                    seq.next(self.out);
                    typst_str(FROM_TAG, self.out);
                }
            }
            Cell::Count { n, id } => {
                seq.next(self.out);
                let target = self.args_label(id);
                push_text_link(&format!("{n}"), target.as_deref(), self.out);
            }
            Cell::Default(value) => {
                seq.next(self.out);
                push_code_link(value, None, self.out);
            }
        }
        seq.close(self.out);
    }

    /// Writes the inlines of the cell of a description into the array
    /// `seq`: its first paragraph, the link to its entry in the details and
    /// the mark of a description only the intermediate representation
    /// carries.
    fn description_cell(
        &mut self,
        seq: &mut Seq,
        first: &str,
        more: Option<&ElementId>,
        source: DescriptionSource,
    ) {
        let schema = self.schema;
        let doc = parse(first, &ReadLimits::DEFAULT);
        seq.next(self.out);
        self.out.push_str("sv-desc(");
        to_typst(&doc, first, self.out);
        self.out.push_str(", cell: true)");
        let anchor = more.map_or("", |id| schema.anchors.of(id));
        if !anchor.is_empty() {
            seq.next(self.out);
            typst_str(" ", self.out);
            seq.next(self.out);
            let target = format!("{DETAILS_PREFIX}{anchor}");
            push_text_link(MORE_LABEL, Some(&target), self.out);
        }
        if source == DescriptionSource::IrOnly {
            seq.next(self.out);
            typst_str(IR_ONLY, self.out);
        }
    }

    /// Writes a type reference, every run of wrappers a string and every
    /// name an array of its text and the label of its page.
    fn type_ref(&mut self, parts: &[TypeRefPart]) {
        self.out.push_str("sv-type(");
        let mut seq = Seq::open(self.out);
        let mut wrappers = String::new();
        for part in parts {
            match part {
                TypeRefPart::Open(s) | TypeRefPart::Close(s) => wrappers.push_str(s),
                TypeRefPart::Name { text, id } => {
                    if !wrappers.is_empty() {
                        seq.next(self.out);
                        typst_str(&wrappers, self.out);
                        wrappers.clear();
                    }
                    seq.next(self.out);
                    self.out.push('(');
                    typst_str(text, self.out);
                    self.out.push_str(", ");
                    match element_label(self.schema, id) {
                        Some(target) => push_label(&target, self.out),
                        None => self.out.push_str("none"),
                    }
                    self.out.push(')');
                }
            }
        }
        if !wrappers.is_empty() {
            seq.next(self.out);
            typst_str(&wrappers, self.out);
        }
        seq.close(self.out);
        self.out.push(')');
    }

    /// Returns the label of the arguments section of the field `id`.
    fn args_label(&self, id: &ElementId) -> Option<String> {
        let rest = self.schema.anchors.of(id).strip_prefix(FIELD_PREFIX)?;
        let route = page_of(self.schema, id)?;
        documented(self.schema, &route).then(|| format!("{ARGS_PREFIX}{rest}"))
    }

    /// Writes the details of the descriptions a table cut under a heading of
    /// `level`, one entry per element with its anchor and its whole
    /// description.
    fn details(&mut self, title: &str, entries: &[(ElementId, String)], level: u8) {
        let entry = level.saturating_add(2);
        self.heading(level.saturating_add(1), title);
        for (id, text) in entries {
            let anchor = self.schema.anchors.of(id);
            if !anchor.is_empty() {
                self.anchor(&format!("{DETAILS_PREFIX}{anchor}"));
            }
            self.code_heading(entry, &element_name(self.schema, id), false);
            self.description(text, entry);
        }
    }

    /// Writes a name grid under a heading of `level`, one paragraph of links
    /// separated by commas, or the sentence of an empty grid.
    fn grid(&mut self, title: &str, names: &[(String, ElementId)], level: u8) {
        self.block_heading(level, title);
        if names.is_empty() {
            self.note(EMPTY_GRID);
            return;
        }
        self.out.push_str("#sv-p(");
        let mut seq = Seq::open(self.out);
        for (i, (text, id)) in names.iter().enumerate() {
            if i > 0 {
                seq.next(self.out);
                typst_str(", ", self.out);
            }
            seq.next(self.out);
            let target = element_label(self.schema, id);
            push_code_link(text, target.as_deref(), self.out);
        }
        seq.close(self.out);
        self.out.push_str(")\n");
    }

    /// Writes a section under a heading of `level`, its title as a heading
    /// with its anchor and its blocks one level deeper.
    ///
    /// The section of a root field carries the anchor of the field and is a
    /// heading of the outline, and the section of the arguments of any other
    /// field carries the anchor of those arguments (see [`Block::Section`]).
    fn section(&mut self, title: &str, id: &ElementId, blocks: &[Block], level: u8) {
        let anchor = self.schema.anchors.of(id);
        let root = self.is_root_field(id);
        let anchor = if root {
            String::from(anchor)
        } else {
            anchor
                .strip_prefix(FIELD_PREFIX)
                .map_or_else(String::new, |rest| format!("{ARGS_PREFIX}{rest}"))
        };
        if !anchor.is_empty() {
            self.anchor(&anchor);
        }
        let inner = level.saturating_add(1);
        self.code_heading(inner, title, root);
        self.blocks(blocks, inner);
    }

    /// Returns whether `id` is a field of a root type.
    fn is_root_field(&self, id: &ElementId) -> bool {
        let model = &self.schema.model;
        let ElementId::Field(owner, _) = id else {
            return false;
        };
        model
            .type_by_name(model.name(*owner))
            .is_some_and(|ty| model.roots().kind_of(ty).is_some())
    }

    /// Writes the link into the live viewer at `route`, when a viewer URL is
    /// set and may be linked.
    ///
    /// The link is the viewer URL without its fragment, followed by the hash of
    /// the route under the default options, as the Markdown writer writes it.
    fn viewer_link(&mut self, route: &Route) {
        let Some(url) = self.links.viewer_url.as_deref().filter(|u| web_url(u)) else {
            return;
        };
        let base = url.split_once('#').map_or(url, |(base, _)| base);
        let mut href = String::from(base);
        href.push_str(&route.to_hash(&Options::default()));
        self.out.push_str("#sv-p((sv-link(");
        typst_str(&href, self.out);
        self.out.push_str(", (");
        typst_str(VIEWER_LABEL, self.out);
        self.out.push_str(",)),))\n");
    }
}
