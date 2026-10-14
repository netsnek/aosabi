// SPDX-License-Identifier: MIT
/*
 * aoviewer-core render_markdown: the Markdown writer of a page.
 *
 * The Markdown is GitHub flavoured with relative links, one anchor per element
 * and one breadcrumb line per page.
 */

//! The Markdown writer.
//!
//! [`render_markdown`] writes one [`PageModel`] as GitHub flavoured Markdown,
//! the page of the tree the generator writes and the page the viewer copies as
//! Markdown. The page opens with its breadcrumb line, then its title with the
//! kind of a type page beside it, then its blocks in the order of the model.
//!
//! - A title is a heading of level 1 in the split layout. In the single layout
//!   every page is a section of `API.md`, so the home page keeps level 1 and
//!   every other page takes level 2. A table, a name grid and a section take
//!   the level under the heading that holds them, and every heading stops at
//!   level 6, the deepest one the [CommonMark specification] knows.
//! - A description that ends inside a fenced code block outside every list item
//!   gets its closing fence, so the page after it stays Markdown. A fence
//!   indented by two or three columns while a list may be open may sit in one
//!   of its items, which the next line at the margin closes together with its
//!   fence, and a fence after a line that may open an HTML block may be text of
//!   that block, so both are left as written. A line indented by four columns
//!   or more, as every line led by a tab is, may continue an item and so never
//!   ends a list.
//! - A table is a GitHub table under a heading with its title, which a table
//!   titled as its page leaves out, as a name grid does. A cell of a name
//!   writes the anchor of its element before the name, and every name of a
//!   cell, a link or a grid is a code span, as in the shape of the page of
//!   `Product` in the plan, where rule 4 of subsection C4 sends names through
//!   [`md_text`]. A type reference writes every name as a link and every run of
//!   wrappers as code, a description writes its first paragraph through
//!   [`first_paragraph_cell`], a link `more` to its entry in the details when
//!   more follows and the words `(from ir.json, not served)` when only the
//!   intermediate representation carries it, and a deprecation writes its
//!   reason with `(from the description)` after the reason of a tag. The
//!   deprecated group of a type is folded into a `details` element.
//! - A name grid is one paragraph of links separated by commas.
//! - A diagram is a `picture` element with a source for the dark colour scheme
//!   when both themes are written, and a plain image link when one is. Its
//!   Mermaid form, when the caller has one, follows it as a fenced block. A
//!   skipped diagram is one sentence.
//! - A signature is a fenced code block of GraphQL, a note a paragraph, and the
//!   viewer link a paragraph written only when a viewer URL is set.
//!
//! A link inside the tree is relative to the file being written, and its
//! fragment is the anchor of its target. [`page_path`] names the file of every
//! page of the tree. An element links to the page [`page_of`] answers under its
//! anchor from [󰀂](crate::id::Anchors), which writes its anchor exactly once.
//! The section of the arguments of a field carries the anchor of the field with
//! `args-` in place of `field-`, and the entry of a description in the details
//! the anchor of its element behind `details-`. Neither prefix starts the
//! anchor of an element, which is the text of a stable id, so no two anchors
//! meet. In the single layout a page that documents no element carries an
//! anchor of its own, `home`, `clusters`, `scalars`, `relay`, `unreachable`,
//! `deprecated` or `cluster-<slug>`, since every page shares one file there.
//! None of them starts as the anchor of an element does, with `type`, `field`,
//! `arg`, `enumvalue` or `inputfield` and a hyphen. A route whose page the tree
//! does not write, a whole graph, a path or a search, is no link, and the
//! cluster map links to the home page, which holds the table of the clusters.
//!
//! Every link the writer makes is relative, or the viewer URL when its scheme
//! is `http` or `https`, so no link carries another scheme.
//!
//! Every loop runs over the blocks, rows, cells, parts or names of one page,
//! whose counts the model of the schema caps, over the crumbs of one
//! breadcrumb, over the components of one path, over the lines or bytes of one
//! text, whose length [`ReadLimits`] caps, or up to the heading level
//! `MAX_LEVEL`. The page model nests a section one level deep, so the writer of
//! a section calls the writer of its blocks once.
//!
//! [CommonMark specification]: https://spec.commonmark.org/0.31.2/

mod mermaid;

pub use mermaid::{
    MermaidRefused,
    render_mermaid, //
};

use alloc::{
    format,
    string::String,
    vec::Vec, //
};
use core::fmt::Write;

use crate::{
    commonmark::{
        RawHtml,
        first_paragraph_cell,
        parse,
        to_markdown, //
    },
    escape::{
        md_cell,
        md_text,
        xml_attr,
        xml_text, //
    },
    id::ElementId,
    limits::ReadLimits,
    model::{
        Model,
        RootKind,
        TypeId, //
    },
    page::{
        Block,
        Cell,
        DescriptionSource,
        LayoutMode,
        Links,
        PageModel,
        TypeRefPart,
        kind_word,
        page_of,
        page_of_type,
        title, //
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

/// The deepest heading level.
///
/// [CommonMark specification]: https://spec.commonmark.org/0.31.2/
const MAX_LEVEL: u8 = 6;

/// The shortest run of a code fence.
///
/// [CommonMark specification]: https://spec.commonmark.org/0.31.2/
const MIN_FENCE: usize = 3;

/// The deepest indentation, in columns, of a line that stands outside every
/// list item, unless it continues a paragraph lazily.
///
/// A list item indents its content by at least two columns, so a line indented
/// by one column or none belongs to no item. A fence there belongs to no item,
/// since a fence never continues a paragraph, and so does any line there after
/// a blank line, which ends every paragraph.
///
/// [CommonMark specification]: https://spec.commonmark.org/0.31.2/
const MAX_MARGIN_INDENT: usize = 1;

/// The tab stop of an indentation: a tab advances it to the next multiple of
/// four columns.
///
/// [CommonMark specification]: https://spec.commonmark.org/0.31.2/
const TAB_STOP: usize = 4;

/// The longest run of digits of an ordered list marker.
///
/// [CommonMark specification]: https://spec.commonmark.org/0.31.2/
const MAX_MARKER_DIGITS: usize = 9;

/// The deepest indentation, in columns, of a fence, of a list marker and of the
/// start of an HTML block. An indentation that holds a tab always reaches past
/// it.
///
/// [CommonMark specification]: https://spec.commonmark.org/0.31.2/
const MAX_FENCE_INDENT: usize = 3;

/// The one file of the single layout.
const SINGLE_FILE: &str = "API.md";

/// The file of the home page in the split layout.
const INDEX_FILE: &str = "README.md";

/// The bytes besides letters and digits a destination keeps as they are, the
/// unreserved and reserved characters of a URL without the parentheses and the
/// `&`, and the `%` of a byte already encoded.
const URL_BYTES: &[u8] = b"-._~:/?#[]@!$'*+,;=%";

/// The separator of the crumbs of a breadcrumb line.
const CRUMB_SEPARATOR: &str = " / ";

/// The prefix of the anchor of a field, which the anchor of its arguments
/// section replaces.
const FIELD_PREFIX: &str = "field-";

/// The prefix of the anchor of the arguments section of a field.
const ARGS_PREFIX: &str = "args-";

/// The prefix of the anchor of an entry of the details.
const DETAILS_PREFIX: &str = "details-";

/// The prefix of the anchor of a type.
const TYPE_PREFIX: &str = "type-";

/// The sentence of a type with fewer than two neighbours.
const FEW_NEIGHBOURS: &str =
    "This type has fewer than two neighbours, so it has no neighbourhood diagram.";

/// The sentence of a view the graph holds no box for.
const NO_BOX: &str = "This page has no diagram, because the graph holds no box for it.";

/// The sentence of a name grid without a name, such as the unreachable types
/// of a schema whose every type a root reaches.
const EMPTY_GRID: &str = "None.";

/// The label of the link into the live viewer.
const VIEWER_LABEL: &str = "Open this page in the viewer";

/// The words after the description only the intermediate representation
/// carries.
const IR_ONLY: &str = " (from ir.json, not served)";

/// The words after the reason of a deprecation a description tag marks.
const FROM_TAG: &str = " (from the description)";

/// A diagram of a page as the tree writes it, the files of its themes, its
/// size and its Mermaid form.
///
/// The paths are paths inside the tree, such as `svg/type/Product.light.svg`,
/// which the writer makes relative to [`Links::from`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagramRef {
    /// The SVG of the light theme, when the tree writes it.
    pub light: Option<String>,
    /// The SVG of the dark theme, when the tree writes it.
    pub dark: Option<String>,
    /// The width of the diagram in pixels.
    pub width_px: u32,
    /// The height of the diagram in pixels.
    pub height_px: u32,
    /// The body of a Mermaid class diagram of the same view, which
    /// [`render_mermaid`] writes, when the caller adds one.
    pub mermaid: Option<String>,
}

/// Writes `page` as GitHub flavoured Markdown into `out`.
///
/// `schema` is the bundle the page was built from, which names the anchors
/// and the files of the links, and `links` says which file is written, in
/// which layout and with which viewer URL. `diagram` answers the files of
/// the diagram of a route and a part, or `None` when the tree writes none,
/// and a skipped diagram is written as its sentence without asking it. When
/// `out` already holds text, such as the pages before it in the one file of
/// the single layout, the page starts after a blank line. The page ends with
/// one line feed.
///
/// # Panics
///
/// Panics when a name of `page` comes from another model than the one of
/// `schema`.
pub fn render_markdown(
    page: &PageModel,
    schema: &Schema,
    links: &Links,
    diagram: &dyn Fn(&Route, Option<u16>) -> Option<DiagramRef>,
    out: &mut String,
) {
    if !out.is_empty() {
        // A blank line ends the paragraph or the table the text before ends
        // with, so the breadcrumb line is neither a row nor a line of it.
        if !out.ends_with('\n') {
            out.push('\n');
        }
        if !out.ends_with("\n\n") {
            out.push('\n');
        }
    }
    let start = out.len();
    let mut writer = Writer {
        schema,
        links,
        diagram,
        out,
        start,
        title: &page.title,
    };
    writer.page(page);
}

/// Returns the path inside the tree of the file that holds the page of
/// `route` in `layout`, or `None` for a route whose page the tree does not
/// write.
///
/// In the split layout the home page is `README.md`, a root page `md/query.md`,
/// `md/mutation.md` or `md/subscription.md`, a type page `md/types/<Name>.md`,
/// a cluster page `md/clusters/<slug>.md` and a report page `md/relay.md`,
/// `md/scalars.md`, `md/unreachable.md` or `md/deprecated.md`. A type name that
/// only case separates from an earlier one in name order takes the number of
/// its anchor as an infix, `md/types/Foo.2.md`. A type documented on the
/// scalars page or the Relay page answers that page, the page of one root field
/// answers its root page, and the cluster map answers the home page. In the
/// single layout every page the split layout writes is in `API.md`.
///
/// The route of a type answers `None` when the schema declares no such type
/// and for an introspection type, whose page no tree writes, and so do the
/// routes of a whole graph, a path and a search.
#[must_use]
pub fn page_path(schema: &Schema, route: &Route, layout: LayoutMode) -> Option<String> {
    let path = split_path(schema, route)?;
    match layout {
        LayoutMode::Split => Some(path),
        LayoutMode::Single => Some(String::from(SINGLE_FILE)),
    }
}

/// Returns the path of the page of `route` in the split layout.
fn split_path(schema: &Schema, route: &Route) -> Option<String> {
    let model = &schema.model;
    let path = match route {
        Route::Home | Route::Clusters => String::from(INDEX_FILE),
        Route::Type { name, .. } => {
            let ty = model.type_by_name(name)?;
            if let Some(root) = model.roots().kind_of(ty) {
                return Some(root_file(root));
            }
            match page_of_type(schema, ty)? {
                Route::Type { .. } => type_file(schema, ty),
                Route::Scalars => String::from("md/scalars.md"),
                Route::Relay => String::from("md/relay.md"),
                _ => return None,
            }
        }
        Route::Op { root, .. } => {
            model.roots().get(*root)?;
            root_file(*root)
        }
        Route::Cluster { slug } => format!("md/clusters/{slug}.md"),
        Route::Unreachable => String::from("md/unreachable.md"),
        Route::Deprecated => String::from("md/deprecated.md"),
        Route::Scalars => String::from("md/scalars.md"),
        Route::Relay => String::from("md/relay.md"),
        Route::Whole { .. } | Route::Path { .. } | Route::Search { .. } => return None,
    };
    Some(path)
}

/// Returns the file of the root page of `root`.
fn root_file(root: RootKind) -> String {
    format!("md/{}.md", root.word())
}

/// Returns the file of the type page of `ty`, with the number of its anchor
/// as an infix when a case collision numbers it.
///
/// The anchor of a type is `type-` and its name in lower case, and `-2` and up
/// follow it for a name that only case separates from an earlier one in name
/// order, while a name carries no hyphen of its own, so the numbering of the
/// files is the numbering of the anchors.
fn type_file(schema: &Schema, ty: TypeId) -> String {
    let model = &schema.model;
    let name = model.name(model.ty(ty).name);
    let anchor = schema.anchors.of(&ElementId::of_type(model, ty));
    let number = anchor
        .strip_prefix(TYPE_PREFIX)
        .and_then(|rest| rest.rsplit_once('-'))
        .map(|(_, n)| n);
    match number {
        Some(n) => format!("md/types/{name}.{n}.md"),
        None => format!("md/types/{name}.md"),
    }
}

/// Returns the anchor a page that documents no element carries in the single
/// layout, where every page shares one file.
fn single_anchor(route: &Route) -> Option<String> {
    let word = match route {
        Route::Home => "home",
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

/// Returns the heading level of the title of the page of `route`.
fn page_level(route: &Route, layout: LayoutMode) -> u8 {
    match (layout, route) {
        (LayoutMode::Single, Route::Home) | (LayoutMode::Split, _) => 1,
        (LayoutMode::Single, _) => 2,
    }
}

/// Returns the path of `to` relative to the directory of `from`, both paths
/// inside the tree, with `fragment` after it.
///
/// A link into the file being written is the fragment alone.
fn relative(from: &str, to: &str, fragment: Option<&str>) -> String {
    let mut out = String::new();
    if let (true, Some(f)) = (from == to, fragment) {
        out.push('#');
        out.push_str(f);
        return out;
    }
    let from_dirs: Vec<&str> = from.split('/').collect();
    let from_dirs = &from_dirs[..from_dirs.len() - 1];
    let to_parts: Vec<&str> = to.split('/').collect();
    let (file, to_dirs) = to_parts.split_last().unwrap_or((&"", &[]));
    let common = from_dirs
        .iter()
        .zip(to_dirs)
        .take_while(|(a, b)| a == b)
        .count();
    for _ in common..from_dirs.len() {
        out.push_str("../");
    }
    for dir in &to_dirs[common..] {
        out.push_str(dir);
        out.push('/');
    }
    out.push_str(file);
    if let Some(f) = fragment {
        out.push('#');
        out.push_str(f);
    }
    out
}

/// Writes `url` as the destination of a Markdown link.
///
/// A byte a URL carries unencoded is kept and every other one is percent
/// encoded, the parentheses and the backslash too, so the destination needs no
/// angle brackets and holds no space. An `&` is escaped with a backslash, since
/// a reader decodes an entity reference inside a destination.
///
/// [CommonMark specification]: https://spec.commonmark.org/0.31.2/
fn push_url(url: &str, out: &mut String) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for b in url.bytes() {
        if b == b'&' {
            out.push_str("\\&");
        } else if b.is_ascii_alphanumeric() || URL_BYTES.contains(&b) {
            out.push(char::from(b));
        } else {
            out.push('%');
            out.push(char::from(HEX[usize::from(b >> 4)]));
            out.push(char::from(HEX[usize::from(b & 0x0f)]));
        }
    }
}

/// Returns whether the viewer URL `url` may be linked, which it may when it
/// names the scheme `http` or `https` or no scheme at all.
///
/// A scheme is a letter followed by letters, digits, `+`, `-` and `.` up to the
/// first colon. A URL that holds a space or a control character is refused
/// whole, since a browser strips leading controls and spaces and removes every
/// tab and newline before it reads the scheme, so a tab inside `javascript:`
/// would hide from this rule a scheme the browser still reads.
fn linkable(url: &str) -> bool {
    if url.bytes().any(|b| b <= b' ' || b == 0x7f) {
        return false;
    }
    let Some((scheme, _)) = url.split_once(':') else {
        return true;
    };
    let is_scheme = scheme
        .bytes()
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic())
        && scheme
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.'));
    !is_scheme || scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https")
}

/// Writes `text` as one code span, whose fence is one backtick longer than
/// the longest run of backticks inside.
///
/// A space pads both sides when the text starts or ends with a backtick or a
/// space, which the reader takes off again, and every line ending becomes a
/// space, as the reader reads it. Inside a cell every `|` is escaped, which the
/// table extension reads before the code span. The empty text writes nothing.
///
/// [CommonMark specification]: https://spec.commonmark.org/0.31.2/
/// [GitHub Flavored Markdown specification]: https://github.github.com/gfm/
fn code_span(text: &str, cell: bool, out: &mut String) {
    if text.is_empty() {
        return;
    }
    let fence = longest_run(text, b'`') + 1;
    let pad = text.starts_with(['`', ' ', '\r', '\n']) || text.ends_with(['`', ' ', '\r', '\n']);
    push_run('`', fence, out);
    if pad {
        out.push(' ');
    }
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' if chars.peek() == Some(&'\n') => {}
            '\r' | '\n' => out.push(' '),
            '|' if cell => out.push_str("\\|"),
            '\0' => out.push('\u{FFFD}'),
            c => out.push(c),
        }
    }
    if pad {
        out.push(' ');
    }
    push_run('`', fence, out);
}

/// Returns the length of the longest run of `byte` in `text`.
fn longest_run(text: &str, byte: u8) -> usize {
    let mut longest = 0;
    let mut run = 0;
    for b in text.bytes() {
        run = if b == byte { run + 1 } else { 0 };
        longest = longest.max(run);
    }
    longest
}

/// Writes `c` `n` times.
fn push_run(c: char, n: usize, out: &mut String) {
    for _ in 0..n {
        out.push(c);
    }
}

/// Writes `body` as a fenced code block with the info string `info`, under a
/// fence of backticks longer than every run of backticks in `body`.
fn fenced(info: &str, body: &str, out: &mut String) {
    let fence = (longest_run(body, b'`') + 1).max(MIN_FENCE);
    push_run('`', fence, out);
    out.push_str(info);
    out.push('\n');
    out.push_str(body);
    if !body.is_empty() && !body.ends_with('\n') {
        out.push('\n');
    }
    push_run('`', fence, out);
    out.push('\n');
}

/// Returns the byte and the length of the fence of a fenced code block that
/// `text` opens outside every list item and leaves open, or `None` when it
/// leaves none open or its blocks are not known well enough to tell.
///
/// A fence is a run of at least three backticks or tildes indented by at most
/// three columns, and a backtick fence has no backtick in its info string. A
/// run of the same byte at least as long, indented by at most three columns and
/// followed by white space alone closes it. A line ends at a line feed, a
/// carriage return or both, and its indentation counts a space as one column
/// and a tab as the columns up to the next tab stop, so a line led by a tab is
/// indented by four columns or more.
///
/// A list may be open from a line that opens a list item until a fence or,
/// after a blank line, any other line at the margin, and a fence indented by
/// two or three columns while a list may be open may belong to one of its
/// items. A line indented by four columns or more is indented code or paragraph
/// text outside every list item and may continue an item inside one, so it
/// neither opens a fence nor ends a list. A line that may open an HTML block
/// may hold a fence line as text of that block. The answer is `None` as soon as
/// such a fence or such a line comes, so the writer never adds a fence the text
/// does not need. A fence line inside what would be the title of a link
/// reference definition opens a fence, since the blocks are read before the
/// definitions.
///
/// The writer of a description closes the fence this answers with a line of
/// the same byte and length, and so does a writer outside the core that
/// places a text before more Markdown, such as the knowledge pages of the
/// device chapter of the generator, which follow each other in one file. A
/// writer that changes the text, as one that escapes its raw HTML does,
/// asks this of the text it writes.
///
/// [CommonMark specification]: https://spec.commonmark.org/0.31.2/
#[must_use]
pub fn open_fence(text: &str) -> Option<(u8, usize)> {
    let mut open: Option<(u8, usize)> = None;
    let mut list = false;
    let mut blank = false;
    let lines = text
        .split('\n')
        .flat_map(|l| l.strip_suffix('\r').unwrap_or(l).split('\r'));
    for line in lines {
        let (indent, rest) = indentation(line);
        let after_blank = blank;
        blank = rest.is_empty();
        if let Some((byte, len)) = open {
            let run = rest.bytes().take_while(|&b| b == byte).count();
            let closes = run >= len
                && indent <= MAX_FENCE_INDENT
                && rest[run..].trim_matches([' ', '\t']).is_empty();
            if closes {
                open = None;
            }
            continue;
        }
        if blank || indent > MAX_FENCE_INDENT {
            continue;
        }
        if opens_html(rest) {
            return None;
        }
        match opening_fence(rest) {
            Some(_) if list && indent > MAX_MARGIN_INDENT => return None,
            Some(fence) => {
                list = false;
                open = Some(fence);
            }
            None if opens_item(rest) => list = true,
            None if after_blank && indent <= MAX_MARGIN_INDENT => list = false,
            None => {}
        }
    }
    open
}

/// Returns the column the indentation of `line` reaches and the line without
/// it, where a space advances one column and a tab to the next multiple of
/// `TAB_STOP`.
///
/// [CommonMark specification]: https://spec.commonmark.org/0.31.2/
fn indentation(line: &str) -> (usize, &str) {
    let rest = line.trim_start_matches([' ', '\t']);
    let lead = &line.as_bytes()[..line.len() - rest.len()];
    let column = lead.iter().fold(0, |column: usize, &byte| match byte {
        b'\t' => column + (TAB_STOP - column % TAB_STOP),
        _ => column + 1,
    });
    (column, rest)
}

/// Returns whether `rest`, a line without its indentation, may open an HTML
/// block, which every one of the seven kinds starts with `<` and a letter, `/`,
/// `!` or `?`.
///
/// [CommonMark specification]: https://spec.commonmark.org/0.31.2/
fn opens_html(rest: &str) -> bool {
    let bytes = rest.as_bytes();
    bytes.first() == Some(&b'<')
        && bytes
            .get(1)
            .is_some_and(|b| b.is_ascii_alphabetic() || matches!(b, b'/' | b'!' | b'?'))
}

/// Returns the byte and the length of the fence `rest` opens, the line
/// without its indentation.
fn opening_fence(rest: &str) -> Option<(u8, usize)> {
    let byte = rest.bytes().next().filter(|b| matches!(b, b'`' | b'~'))?;
    let run = rest.bytes().take_while(|&b| b == byte).count();
    let opens = run >= MIN_FENCE && (byte == b'~' || !rest[run..].contains('`'));
    opens.then_some((byte, run))
}

/// Returns whether `rest`, a line without its indentation, starts with the
/// marker of a list item, a bullet or at most nine digits with a `.` or a `)`,
/// followed by a space, a tab or the end of the line.
///
/// [CommonMark specification]: https://spec.commonmark.org/0.31.2/
fn opens_item(rest: &str) -> bool {
    let bytes = rest.as_bytes();
    let marker = match bytes.first() {
        Some(b'-' | b'+' | b'*') => 1,
        Some(b'0'..=b'9') => {
            let digits = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
            match bytes.get(digits) {
                Some(b'.' | b')') if digits <= MAX_MARKER_DIGITS => digits + 1,
                _ => return false,
            }
        }
        _ => return false,
    };
    matches!(bytes.get(marker), None | Some(b' ' | b'\t'))
}

/// Returns the name of the element `id` as the heading of its entry in the
/// details, the last name of its id, which for an edge is the one of its
/// source.
fn element_label(model: &Model, id: &ElementId) -> String {
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
    String::from(model.name(name))
}

/// Returns the alternative text of the diagram of `route`, part `part`.
fn alt_text(route: &Route, part: Option<u16>) -> String {
    let mut alt = match route {
        Route::Home => String::from("Overview diagram"),
        Route::Type { name, .. } => format!("Neighbourhood diagram of {name}"),
        Route::Op { field, .. } => format!("Operation diagram of {field}"),
        Route::Cluster { slug } => format!("Cluster diagram of {slug}"),
        Route::Clusters => String::from("Cluster map"),
        Route::Whole { .. } => String::from("Whole graph diagram"),
        Route::Path { from, to } => format!("Path diagram from {from} to {to}"),
        _ => String::from("Diagram"),
    };
    if let Some(n) = part {
        // Writing into a `String` cannot fail.
        let _ = write!(alt, ", part {n}");
    }
    alt
}

/// Returns the sentence of a diagram skipped for `reason`.
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

/// The state of one call of [`render_markdown`].
struct Writer<'a> {
    /// The schema the page was built from.
    schema: &'a Schema,
    /// The file being written, its layout and the viewer URL.
    links: &'a Links,
    /// The files of the diagram of a route and a part.
    diagram: &'a dyn Fn(&Route, Option<u16>) -> Option<DiagramRef>,
    /// The text written.
    out: &'a mut String,
    /// The length of `out` before the page.
    start: usize,
    /// The title of the page.
    title: &'a str,
}

impl Writer<'_> {
    /// Writes the page.
    fn page(&mut self, page: &PageModel) {
        let level = page_level(&page.route, self.links.layout);
        self.breadcrumb(&page.breadcrumb);
        let anchor = self.page_anchor(page);
        self.open_heading(level, anchor.as_deref());
        md_text(&page.title, self.out);
        if let Some(kind) = page.kind {
            self.out.push(' ');
            code_span(kind_word(kind), false, self.out);
        }
        self.out.push('\n');
        self.blocks(&page.blocks, level);
        self.end();
    }

    /// Returns the anchor the page carries before its title, the anchor of
    /// the type of a type page or a root page, and in the single layout the
    /// anchor of a page that documents no element.
    fn page_anchor(&self, page: &PageModel) -> Option<String> {
        match (&page.route, page.kind) {
            (Route::Type { name, .. }, Some(_)) => self.type_anchor(name),
            (route, _) if self.links.layout == LayoutMode::Single => single_anchor(route),
            _ => None,
        }
    }

    /// Returns the anchor of the type called `name`, or `None` when the
    /// schema declares no such type.
    fn type_anchor(&self, name: &str) -> Option<String> {
        let model = &self.schema.model;
        let ty = model.type_by_name(name)?;
        let anchor = self.schema.anchors.of(&ElementId::of_type(model, ty));
        (!anchor.is_empty()).then(|| String::from(anchor))
    }

    /// Writes the breadcrumb line, every crumb a link to its page.
    fn breadcrumb(&mut self, crumbs: &[(String, Route)]) {
        if crumbs.is_empty() {
            return;
        }
        for (i, (text, route)) in crumbs.iter().enumerate() {
            if i > 0 {
                self.out.push_str(CRUMB_SEPARATOR);
            }
            match self.route_href(route) {
                Some(href) => {
                    self.out.push('[');
                    md_text(text, self.out);
                    self.out.push_str("](");
                    push_url(&href, self.out);
                    self.out.push(')');
                }
                None => md_text(text, self.out),
            }
        }
        self.out.push('\n');
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
            } if *title == title::DEPRECATED => self.folded_table(title, columns, rows),
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
            Block::Note { text } => {
                self.gap();
                md_text(text, self.out);
                self.out.push('\n');
            }
            Block::Signature { text } => {
                self.gap();
                fenced("graphql", text, self.out);
            }
            Block::Section { title, id, blocks } => self.section(title, id, blocks, level),
            Block::ViewerLink { route } => self.viewer_link(route),
        }
    }

    /// Starts a new block, one blank line after the text before it.
    fn gap(&mut self) {
        if self.out.len() == self.start {
            return;
        }
        if !self.out.ends_with('\n') {
            self.out.push('\n');
        }
        if !self.out[self.start..].ends_with("\n\n") {
            self.out.push('\n');
        }
    }

    /// Ends the page with one line feed.
    fn end(&mut self) {
        let body = self.out[self.start..].trim_end_matches('\n').len();
        self.out.truncate(self.start + body);
        self.out.push('\n');
    }

    /// Starts a heading of `level`, with the line of `anchor` before it when
    /// there is one, and leaves the text of the heading to the caller.
    fn open_heading(&mut self, level: u8, anchor: Option<&str>) {
        self.gap();
        if let Some(anchor) = anchor.filter(|a| !a.is_empty()) {
            self.anchor(anchor);
            self.out.push('\n');
        }
        for _ in 0..level.min(MAX_LEVEL) {
            self.out.push('#');
        }
        self.out.push(' ');
    }

    /// Writes a heading of `level` that reads `text`.
    fn plain_heading(&mut self, level: u8, text: &str) {
        self.open_heading(level, None);
        md_text(text, self.out);
        self.out.push('\n');
    }

    /// Writes the heading of a table or a name grid under a heading of
    /// `level`, which a block titled as its page leaves out, since the title
    /// of the page says it already.
    fn block_heading(&mut self, level: u8, text: &str) {
        if text != self.title {
            self.plain_heading(level + 1, text);
        }
    }

    /// Writes the empty link element that carries `anchor`.
    fn anchor(&mut self, anchor: &str) {
        self.out.push_str("<a id=\"");
        xml_attr(anchor, self.out);
        self.out.push_str("\"></a>");
    }

    /// Writes a description under a heading of `level`, verbatim with its
    /// headings demoted by `level`.
    fn description(&mut self, text: &str, level: u8) {
        self.gap();
        let mark = self.out.len();
        let doc = parse(text, &ReadLimits::DEFAULT);
        to_markdown(&doc, text, level, RawHtml::default(), self.out);
        let body = self.out[mark..].trim_end_matches(['\n', '\r']).len();
        self.out.truncate(mark + body);
        if let Some((byte, len)) = open_fence(text) {
            self.out.push('\n');
            push_run(char::from(byte), len, self.out);
        }
        self.out.push('\n');
    }

    /// Writes a diagram, or the sentence that says why it is skipped.
    fn diagram(&mut self, route: &Route, part: Option<u16>, skipped: Option<EmptyReason>) {
        if let Some(reason) = skipped {
            self.gap();
            md_text(&skipped_sentence(reason), self.out);
            self.out.push('\n');
            return;
        }
        let Some(files) = (self.diagram)(route, part) else {
            return;
        };
        let alt = alt_text(route, part);
        match (&files.light, &files.dark) {
            (Some(light), Some(dark)) => self.picture(&alt, light, dark, &files),
            (Some(one), None) | (None, Some(one)) => {
                self.gap();
                self.out.push_str("![");
                md_text(&alt, self.out);
                self.out.push_str("](");
                push_url(&relative(&self.links.from, one, None), self.out);
                self.out.push_str(")\n");
            }
            (None, None) => {}
        }
        if let Some(body) = &files.mermaid {
            self.gap();
            fenced("mermaid", body, self.out);
        }
    }

    /// Writes the `picture` element of a diagram written in both themes, the
    /// dark one behind a colour scheme query and the light one as the image.
    fn picture(&mut self, alt: &str, light: &str, dark: &str, files: &DiagramRef) {
        self.gap();
        let from = &self.links.from;
        let out = &mut *self.out;
        out.push_str("<picture>\n  <source media=\"(prefers-color-scheme: dark)\" srcset=\"");
        xml_attr(&relative(from, dark, None), out);
        out.push_str("\">\n  <img alt=\"");
        xml_attr(alt, out);
        out.push_str("\" src=\"");
        xml_attr(&relative(from, light, None), out);
        // Writing into a `String` cannot fail.
        let _ = write!(
            out,
            "\" width=\"{}\" height=\"{}\">\n</picture>\n",
            files.width_px, files.height_px
        );
    }

    /// Writes a table with its header row and its rows.
    ///
    /// An empty cell is written as one space between its pipes, as in the shape
    /// of the plan.
    fn table(&mut self, columns: &[&'static str], rows: &[Vec<Cell>]) {
        self.gap();
        self.out.push('|');
        for column in columns {
            self.out.push(' ');
            md_cell(column, self.out);
            self.out.push_str(" |");
        }
        self.out.push_str("\n|");
        for _ in columns {
            self.out.push_str(" --- |");
        }
        self.out.push('\n');
        for row in rows {
            self.out.push('|');
            for cell in row {
                let mark = self.out.len();
                self.out.push(' ');
                self.cell(cell);
                if self.out.len() == mark + 1 {
                    self.out.truncate(mark);
                }
                self.out.push_str(" |");
            }
            self.out.push('\n');
        }
    }

    /// Writes the deprecated group of a type folded into a `details`
    /// element, with its title as the summary.
    fn folded_table(&mut self, title: &str, columns: &[&'static str], rows: &[Vec<Cell>]) {
        self.gap();
        self.out.push_str("<details>\n<summary>");
        xml_text(title, self.out);
        self.out.push_str("</summary>\n");
        self.table(columns, rows);
        self.gap();
        self.out.push_str("</details>\n");
    }

    /// Writes one cell of a table.
    fn cell(&mut self, cell: &Cell) {
        match cell {
            Cell::Text(text) => md_cell(text, self.out),
            Cell::Name { text, id } => {
                let schema = self.schema;
                let anchor = schema.anchors.of(id);
                if !anchor.is_empty() {
                    self.anchor(anchor);
                }
                code_span(text, true, self.out);
            }
            Cell::TypeRef { parts } => self.type_ref(parts),
            Cell::Description {
                first,
                more,
                source,
            } => self.description_cell(first, more.as_ref(), *source),
            Cell::Deprecation { reason, from_tag } => {
                match reason.as_deref().filter(|r| !r.trim().is_empty()) {
                    Some(reason) => md_cell(reason, self.out),
                    None => self.out.push_str("Deprecated"),
                }
                if *from_tag {
                    self.out.push_str(FROM_TAG);
                }
            }
            Cell::Count { n, id } => match self.args_href(id) {
                Some(href) => {
                    // Writing into a `String` cannot fail.
                    let _ = write!(self.out, "[{n}](");
                    push_url(&href, self.out);
                    self.out.push(')');
                }
                None => {
                    let _ = write!(self.out, "{n}");
                }
            },
            Cell::Default(value) => code_span(value, true, self.out),
        }
    }

    /// Writes the cell of a description, its first paragraph, the link to
    /// its entry in the details and the mark of a description only the
    /// intermediate representation carries.
    fn description_cell(
        &mut self,
        first: &str,
        more: Option<&ElementId>,
        source: DescriptionSource,
    ) {
        let schema = self.schema;
        let doc = parse(first, &ReadLimits::DEFAULT);
        first_paragraph_cell(&doc, first, self.out);
        let anchor = more.map_or("", |id| schema.anchors.of(id));
        if !anchor.is_empty() {
            self.out.push_str(" [more](#");
            self.out.push_str(DETAILS_PREFIX);
            push_url(anchor, self.out);
            self.out.push(')');
        }
        if source == DescriptionSource::IrOnly {
            self.out.push_str(IR_ONLY);
        }
    }

    /// Writes a type reference in a cell, every run of wrappers as code and
    /// every name as a link.
    fn type_ref(&mut self, parts: &[TypeRefPart]) {
        let mut wrappers = String::new();
        for part in parts {
            match part {
                TypeRefPart::Open(s) | TypeRefPart::Close(s) => wrappers.push_str(s),
                TypeRefPart::Name { text, id } => {
                    code_span(&wrappers, true, self.out);
                    wrappers.clear();
                    self.element_link(text, id, true);
                }
            }
        }
        code_span(&wrappers, true, self.out);
    }

    /// Writes `text` as code linked to the element `id`, or as code alone
    /// when no page of the tree documents the element.
    fn element_link(&mut self, text: &str, id: &ElementId, cell: bool) {
        match self.element_href(id) {
            Some(href) => {
                self.out.push('[');
                code_span(text, cell, self.out);
                self.out.push_str("](");
                push_url(&href, self.out);
                self.out.push(')');
            }
            None => code_span(text, cell, self.out),
        }
    }

    /// Returns the link to the element `id` from the file being written.
    fn element_href(&self, id: &ElementId) -> Option<String> {
        let route = page_of(self.schema, id)?;
        let file = page_path(self.schema, &route, self.links.layout)?;
        let fragment = if let ElementId::Cluster(_) = id {
            self.route_fragment(&route)
        } else {
            let anchor = self.schema.anchors.of(id);
            (!anchor.is_empty()).then(|| String::from(anchor))
        };
        Some(relative(&self.links.from, &file, fragment.as_deref()))
    }

    /// Returns the link to the arguments section of the field `id`.
    fn args_href(&self, id: &ElementId) -> Option<String> {
        let rest = self.schema.anchors.of(id).strip_prefix(FIELD_PREFIX)?;
        let route = page_of(self.schema, id)?;
        let file = page_path(self.schema, &route, self.links.layout)?;
        let fragment = format!("{ARGS_PREFIX}{rest}");
        Some(relative(&self.links.from, &file, Some(&fragment)))
    }

    /// Returns the link to the page of `route` from the file being written.
    fn route_href(&self, route: &Route) -> Option<String> {
        let file = page_path(self.schema, route, self.links.layout)?;
        let fragment = self.route_fragment(route);
        Some(relative(&self.links.from, &file, fragment.as_deref()))
    }

    /// Returns the fragment of a link to the page of `route` inside its
    /// file.
    ///
    /// A type page and a root page are the anchor of their type, the page of
    /// a root field the anchor of the field on its root page, and in the
    /// single layout every other page its own anchor, the cluster map the
    /// one of the home page.
    fn route_fragment(&self, route: &Route) -> Option<String> {
        let model = &self.schema.model;
        match route {
            Route::Type { name, .. } => self.type_anchor(name),
            Route::Op { root, field } => {
                let ty = model.roots().get(*root)?;
                let f = model
                    .ty(ty)
                    .fields
                    .iter()
                    .copied()
                    .find(|&f| model.name(model.field(f).name) == field)?;
                let anchor = self.schema.anchors.of(&ElementId::of_field(model, f));
                (!anchor.is_empty()).then(|| String::from(anchor))
            }
            _ if self.links.layout == LayoutMode::Split => None,
            Route::Clusters => single_anchor(&Route::Home),
            _ => single_anchor(route),
        }
    }

    /// Writes the details of the descriptions a table cut under a heading of
    /// `level`, one entry per element with its anchor and its whole
    /// description.
    fn details(&mut self, title: &str, entries: &[(ElementId, String)], level: u8) {
        self.plain_heading(level + 1, title);
        for (id, text) in entries {
            let anchor = self.schema.anchors.of(id);
            let anchor = (!anchor.is_empty()).then(|| format!("{DETAILS_PREFIX}{anchor}"));
            self.open_heading(level + 2, anchor.as_deref());
            code_span(&element_label(&self.schema.model, id), false, self.out);
            self.out.push('\n');
            self.description(text, level + 2);
        }
    }

    /// Writes a name grid under a heading of `level`, one paragraph of links
    /// separated by commas, or the sentence of an empty grid.
    fn grid(&mut self, title: &str, names: &[(String, ElementId)], level: u8) {
        self.block_heading(level, title);
        self.gap();
        if names.is_empty() {
            md_text(EMPTY_GRID, self.out);
        }
        for (i, (text, id)) in names.iter().enumerate() {
            if i > 0 {
                self.out.push_str(", ");
            }
            self.element_link(text, id, false);
        }
        self.out.push('\n');
    }

    /// Writes a section under a heading of `level`, its title as a heading
    /// with its anchor and its blocks one level deeper.
    ///
    /// The section of a root field carries the anchor of the field, and the
    /// section of the arguments of any other field the anchor of those
    /// arguments (see [`Block::Section`]).
    fn section(&mut self, title: &str, id: &ElementId, blocks: &[Block], level: u8) {
        let anchor = self.schema.anchors.of(id);
        let anchor = if self.is_root_field(id) {
            String::from(anchor)
        } else {
            anchor
                .strip_prefix(FIELD_PREFIX)
                .map_or_else(String::new, |rest| format!("{ARGS_PREFIX}{rest}"))
        };
        self.open_heading(level + 1, Some(&anchor));
        code_span(title, false, self.out);
        self.out.push('\n');
        self.blocks(blocks, level + 1);
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
    /// the route under the default options.
    fn viewer_link(&mut self, route: &Route) {
        let Some(url) = self.links.viewer_url.as_deref().filter(|u| linkable(u)) else {
            return;
        };
        let base = url.split_once('#').map_or(url, |(base, _)| base);
        let mut href = String::from(base);
        href.push_str(&route.to_hash(&Options::default()));
        self.gap();
        self.out.push('[');
        md_text(VIEWER_LABEL, self.out);
        self.out.push_str("](");
        push_url(&href, self.out);
        self.out.push_str(")\n");
    }
}
