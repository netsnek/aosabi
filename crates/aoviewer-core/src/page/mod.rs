// SPDX-License-Identifier: MIT
/*
 * aoviewer-core page: the page model, a list of blocks the page writers read.
 *
 * A page is a title with a kind, a description, tables, name grids and notes,
 * and the page writers read it and nothing else.
 */

//! The page model.
//!
//! [`PageModel::of`] answers every [`Route`] with the page of that route, the
//! structure the viewer shows and the documents write, so one golden tests
//! both. The pages are the home page, the page of a type, the root page with
//! one section per root field, the page of one root field, the page of a
//! cluster and of the cluster map, the four report pages, and the short pages
//! of the whole graph, of a path and of a search.
//!
//! A page names every element by its [`ElementId`] and leaves the anchor and
//! the file of a link to its writer, which reads them from
//! [`Schema::anchors`] and from [`page_of`]. Four kinds of cell and block
//! write an element, and every other one links to it.
//!
//! - [`Cell::Name`] is the element its row documents, and its writer puts
//!   the anchor of the id before the name.
//! - [`Block::Section`] of a root field puts the anchor of the field before
//!   its title, and the section of the arguments of any other field puts the
//!   anchor of those arguments, the target of the field's [`Cell::Count`].
//! - [`Block::Details`] puts one anchor per entry, the target of the `more`
//!   link of a [`Cell::Description`].
//! - The title of a type page and of a root page is the type, whose anchor
//!   its writer puts before it.
//!
//! Every name of a [`TypeRefPart`] and of a [`Block::Grid`] is a link. A
//! reference to a field, an argument, an enum value or a cluster is a
//! [`Cell::TypeRef`] of one [`TypeRefPart::Name`], so the rule that a name
//! cell writes and every other name links holds without an exception.
//!
//! The order of a page is the one of `docs/documentation-generator.md` section 4.
//! Fields, arguments and enum values keep the order of the transforms, which
//! is schema order unless `sort=alpha`, and every other list sorts by the
//! bytes of its names with an explicit tie break, so the order of the input
//! never reaches a page. A page lists every member of the model, the ones the
//! graph hides included.
//!
//! A diagram of a page is the view of its route under the page budget, one
//! [`Block::Diagram`] per part of a split view or one that says why it is
//! skipped, which is what [`View::for_page`] answers.
//!
//! The model does not record which argument descriptions only `ir.json`
//! carries, so every description of a page is [`DescriptionSource::Served`]
//! until it does.
//!
//! Every loop runs over the types, members, clusters or graph nodes of one
//! schema, whose counts [`ReadLimits`](crate::limits::ReadLimits) caps, over
//! the lines of one description, whose length it caps as well, over at most
//! `MAX_WRAPPERS` wrappers of a type reference, or over the parts of one
//! view.

mod cluster_page;
mod home;
mod reports;
mod root_page;
mod type_page;

use alloc::{
    format,
    string::String,
    vec,
    vec::Vec, //
};

use crate::{
    cluster::{
        ClusterSource,
        Clustering, //
    },
    graph::NodeId,
    id::ElementId,
    model::{
        FieldId,
        Kind,
        Model,
        TypeId,
        TypeRef, //
    },
    route::{
        Options,
        Route,
        WholeRoot, //
    },
    schema::Schema,
    sort::name_order,
    transform::Marked,
    view::{
        EmptyReason,
        View,
        ViewError, //
    },
};

/// The titles of the tables and name grids of the pages.
///
/// A writer that treats one block apart, such as the deprecated group a
/// type page folds, matches its title against these.
pub mod title {
    /// The fields table of an object or an interface.
    pub const FIELDS: &str = "Fields";
    /// The arguments table of a field.
    pub const ARGUMENTS: &str = "Arguments";
    /// The input fields table of an input type.
    pub const INPUT_FIELDS: &str = "Input fields";
    /// The values table of an enum.
    pub const VALUES: &str = "Values";
    /// The name grid of the members of a union.
    pub const POSSIBLE_TYPES: &str = "Possible types";
    /// The name grid of the implementers of an interface.
    pub const IMPLEMENTED_BY: &str = "Implemented by";
    /// The name grid of the interfaces a type implements.
    pub const IMPLEMENTS: &str = "Implements";
    /// The name grid of the fields that return or take a type.
    pub const USED_BY: &str = "Used by";
    /// The name grid of the arguments that take a type.
    pub const USED_AS_ARGUMENT_BY: &str = "Used as argument by";
    /// The pagination note of the collapsed Relay fields of a type.
    pub const PAGINATION: &str = "Pagination";
    /// The deprecated group of a type, which a writer folds.
    pub const DEPRECATED: &str = "Deprecated";
    /// The whole descriptions a table cell cut.
    pub const DETAILS: &str = "Details";
    /// The return type of a root field.
    pub const RETURNS: &str = "Returns";
    /// The counts of the home page.
    pub const COUNTS: &str = "Counts";
    /// The table of the clusters.
    pub const CLUSTERS: &str = "Clusters";
    /// The table of the types of the home page and of a cluster.
    pub const TYPES: &str = "Types";
    /// The root table of the query root on the home page.
    pub const QUERY: &str = "Query";
    /// The root table of the mutation root on the home page.
    pub const MUTATION: &str = "Mutation";
    /// The root table of the subscription root on the home page.
    pub const SUBSCRIPTION: &str = "Subscription";
    /// The built in scalars of the scalars page.
    pub const BUILTIN_SCALARS: &str = "Built in scalars";
    /// The custom scalars of the scalars page.
    pub const CUSTOM_SCALARS: &str = "Custom scalars";
    /// The collapsed wrapper types of the Relay page.
    pub const RELAY_WRAPPERS: &str = "Relay wrappers";
    /// The types no root reaches under the walk of aoviewer.
    pub const UNREACHABLE: &str = "Unreachable";
    /// The lint of the types reached only as an implementation.
    pub const IMPLEMENTATION_ONLY: &str = "Reached only as an implementation";
    /// The lint of the types a parity run with Voyager's walk drops.
    pub const NOT_VOYAGER: &str = "Not reachable by Voyager";
    /// The table of the deprecated report page.
    pub const DEPRECATED_MEMBERS: &str = "Deprecated members";
}

/// The deepest type reference a page writes, the depth the limits of the
/// reader allow, which is a `u16`.
const MAX_WRAPPERS: usize = 1 << 16;

/// The title of the home page and the first entry of every breadcrumb.
const OVERVIEW: &str = "Overview";

/// The prefix the specification reserves for the introspection types, which no
/// page documents.
///
/// [specification]: https://spec.graphql.org/October2021/
const INTROSPECTION: &str = "__";

/// The sentence that states the walk every count of reachability comes from,
/// and that it deviates from Voyager's on purpose in all three ways the walks
/// differ, the seeds, the argument edges and the object to interface edges.
const WALK_NOTE: &str = "Reachable means reached by the walk of aoviewer, which starts from every root and also follows arguments into input types and objects into the interfaces they implement. Voyager starts from one root and follows neither, so this is a deliberate deviation from Voyager.";

/// A page is a list of blocks, and the page renderers read it and nothing
/// else.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageModel {
    /// The route the page answers.
    pub route: Route,
    /// The title, the name of the type on a type page.
    pub title: String,
    /// The kind of the type on a type page and a root page.
    pub kind: Option<Kind>,
    /// The one breadcrumb line, from the home page to the page above this
    /// one, empty on the home page.
    pub breadcrumb: Vec<(String, Route)>,
    /// The blocks in the order a page writer writes them.
    pub blocks: Vec<Block>,
}

/// A block of a page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Block {
    /// The description as the model carries it, which a writer inserts
    /// verbatim with its headings demoted.
    Description {
        /// The text.
        text: String,
    },
    /// The diagram of `route`, one part of it, or the reason it is skipped,
    /// which a writer says in one sentence rather than embedding an empty
    /// frame.
    Diagram {
        /// The route whose view the diagram draws.
        route: Route,
        /// The number of the part, from 1, when the view splits.
        part: Option<u16>,
        /// Why the page shows no diagram.
        skipped: Option<EmptyReason>,
    },
    /// A table with its header and its rows, one cell per column.
    Table {
        /// The title, one of [`title`].
        title: &'static str,
        /// The header cells.
        columns: Vec<&'static str>,
        /// The rows.
        rows: Vec<Vec<Cell>>,
    },
    /// The whole descriptions a table cell cut, each under its element,
    /// placed under that table.
    Details {
        /// The title, [`title::DETAILS`].
        title: &'static str,
        /// The element and its whole description.
        entries: Vec<(ElementId, String)>,
    },
    /// A name grid, every name a link.
    Grid {
        /// The title, one of [`title`].
        title: &'static str,
        /// The text of each name and the element it links to.
        names: Vec<(String, ElementId)>,
    },
    /// A sentence of the tool.
    Note {
        /// The text, which a writer escapes as it escapes a name.
        text: String,
    },
    /// The signature of a root field as GraphQL writes it, such as
    /// `createOrganization(input: CreateOrganizationInput!): Organization!`.
    Signature {
        /// The text.
        text: String,
    },
    /// A titled group of blocks about the field `id`.
    ///
    /// The section of a root field carries the anchor of the field. The
    /// section a type page holds under its fields table carries the arguments
    /// of the field `id`, whose anchor the fields table carries, and a writer
    /// puts the anchor of those arguments before it, the target of the field's
    /// [`Cell::Count`].
    Section {
        /// The title, the name of the field.
        title: String,
        /// The field.
        id: ElementId,
        /// The blocks of the section.
        blocks: Vec<Block>,
    },
    /// A link into the live viewer at `route`, which a writer writes only when
    /// a viewer URL is set.
    ViewerLink {
        /// The route.
        route: Route,
    },
}

/// A cell of a table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cell {
    /// A sentence or a word of the tool, or the empty cell.
    Text(String),
    /// The element the row documents, whose anchor a writer puts before the
    /// name.
    Name {
        /// The name.
        text: String,
        /// The element.
        id: ElementId,
    },
    /// A type reference through every wrapper, or a link to one element as
    /// a single [`TypeRefPart::Name`].
    TypeRef {
        /// The wrappers and the name, in reading order.
        parts: Vec<TypeRefPart>,
    },
    /// The first paragraph of a description, with a link to its whole text in
    /// the [`Block::Details`] under the table when more follows.
    Description {
        /// The source text of the first paragraph.
        first: String,
        /// The element whose entry in the details holds the whole text.
        more: Option<ElementId>,
        /// Where the description comes from.
        source: DescriptionSource,
    },
    /// The deprecation of a member with its reason.
    Deprecation {
        /// The reason, if any.
        reason: Option<String>,
        /// Whether a description line starting with `@deprecated` marks the
        /// member, the hollow mark, rather than the directive.
        from_tag: bool,
    },
    /// The count of the arguments of the field `id`, a link to the section
    /// that lists them.
    Count {
        /// The count.
        n: u32,
        /// The field.
        id: ElementId,
    },
    /// A default value as the schema prints it.
    Default(String),
}

/// Where a description of the page comes from, so an argument description that
/// only the intermediate representation carries says so beside its text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DescriptionSource {
    /// The served schema carries the description.
    Served,
    /// Only `ir.json` carries it, which a writer marks with the words
    /// `(from ir.json, not served)`.
    IrOnly,
}

/// A part of a type reference in reading order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeRefPart {
    /// An opening wrapper, `[`.
    Open(&'static str),
    /// A name, which is a link.
    Name {
        /// The text of the link.
        text: String,
        /// The element it links to.
        id: ElementId,
    },
    /// A closing wrapper, `]` or `!`.
    Close(&'static str),
}

/// How a renderer turns an id into a target: a relative link inside the tree,
/// an anchor, or the viewer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Links {
    /// The layout of the tree, which decides the file of every page.
    pub layout: LayoutMode,
    /// The URL of the live viewer, which a [`Block::ViewerLink`] joins with
    /// the hash of its route.
    pub viewer_url: Option<String>,
    /// The path of the file being written, inside the tree, which every
    /// relative link starts from.
    pub from: String,
}

/// The layout of a tree.
///
/// `Single` writes one `API.md` and `Split` one file per page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutMode {
    /// Every page in one file.
    Single,
    /// One file per page.
    Split,
}

impl PageModel {
    /// The page of a route, the structure of `<gen>` section 4, which is the
    /// structure the viewer shows.
    ///
    /// The page of a type route is the root page when the type is a root and
    /// the type page otherwise, whichever field and argument the route
    /// selects, and the diagram of a type page is the view of the type
    /// without a selection. The views and the reductions of the diagrams run
    /// under `options`, while the tables read the transforms `schema` was
    /// built under.
    ///
    /// # Errors
    ///
    /// Returns [`ViewError::UnknownType`], [`ViewError::UnknownField`],
    /// [`ViewError::UnknownArg`], [`ViewError::NoRoot`] or
    /// [`ViewError::UnknownCluster`] when `route` names what `schema` or
    /// `clustering` does not hold, the errors [`View::for_page`] answers for
    /// the same route.
    ///
    /// # Panics
    ///
    /// Panics when `clustering` was not picked from `schema`.
    pub fn of(
        schema: &Schema,
        clustering: &Clustering,
        route: &Route,
        options: &Options,
    ) -> Result<PageModel, ViewError> {
        match route {
            Route::Home => home::home(schema, clustering, options),
            Route::Type { name, field, arg } => {
                let model = &schema.model;
                let ty = model
                    .type_by_name(name)
                    .ok_or_else(|| ViewError::UnknownType(name.clone()))?;
                check_member(model, ty, field.as_deref(), arg.as_deref())?;
                match model.roots().kind_of(ty) {
                    Some(root) => root_page::root_page(schema, clustering, route, root, options),
                    None => type_page::type_page(schema, clustering, route, ty, options),
                }
            }
            Route::Op { root, field } => {
                root_page::op_page(schema, clustering, route, *root, field, options)
            }
            Route::Cluster { slug } => {
                cluster_page::cluster_page(schema, clustering, route, slug, options)
            }
            Route::Clusters => cluster_page::map_page(schema, clustering, options),
            Route::Whole { root } => whole_page(schema, clustering, route, root, options),
            Route::Path { from, to } => {
                let title = format!("Path from {from} to {to}");
                graph_page(schema, clustering, route, title, options)
            }
            Route::Search { text } => Ok(search_page(route, text)),
            Route::Unreachable => Ok(reports::unreachable(schema)),
            Route::Deprecated => Ok(reports::deprecated(schema)),
            Route::Scalars => Ok(reports::scalars(schema)),
            Route::Relay => Ok(reports::relay(schema)),
        }
    }
}

/// Returns the route of the page that documents the type `ty`, or `None` for
/// an introspection type, which no page documents.
///
/// A built in scalar is documented on the scalars page and a Relay type the
/// collapse finds on the Relay page, which hold them one table row each. A
/// root is documented on its root page and every other type on its own page.
///
/// # Panics
///
/// Panics when `ty` comes from another model.
#[must_use]
pub fn page_of_type(schema: &Schema, ty: TypeId) -> Option<Route> {
    let model = &schema.model;
    let name = model.name(model.ty(ty).name);
    if name.starts_with(INTROSPECTION) {
        return None;
    }
    if schema.transformed.is_builtin(ty) {
        return Some(Route::Scalars);
    }
    if schema.transformed.is_relay_type(ty) {
        return Some(Route::Relay);
    }
    Some(type_route(name))
}

/// Returns the route of the page that documents the element `id`.
///
/// A member is documented on the page of its type, a cluster on its cluster
/// page and an edge where its source is. An introspection type, a member of
/// one and an edge whose source is an edge answer `None`, and so does a type
/// the model does not declare.
///
/// A member of a Relay wrapper or of a built in scalar answers the Relay
/// page or the scalars page, whose one row per type shows it through the
/// row of its type and writes no anchor for the member, so a page links such
/// a member through its type, as the deprecated page does.
///
/// # Panics
///
/// Panics when a name of `id` comes from another model.
#[must_use]
pub fn page_of(schema: &Schema, id: &ElementId) -> Option<Route> {
    let model = &schema.model;
    let owner = match id {
        ElementId::Type(t)
        | ElementId::Field(t, _)
        | ElementId::Arg(t, _, _)
        | ElementId::Value(t, _)
        | ElementId::InputField(t, _) => *t,
        ElementId::Cluster(slug) => return Some(Route::Cluster { slug: slug.clone() }),
        ElementId::Edge(source, _) => {
            return match &**source {
                ElementId::Edge(..) => None,
                other => page_of(schema, other),
            };
        }
    };
    page_of_type(schema, model.type_by_name(model.name(owner))?)
}

/// Returns the word a page writes for `kind`, such as `object`.
#[must_use]
pub fn kind_word(kind: Kind) -> &'static str {
    match kind {
        Kind::Object => "object",
        Kind::Interface => "interface",
        Kind::Union => "union",
        Kind::Enum => "enum",
        Kind::Input => "input",
        Kind::Scalar => "scalar",
    }
}

/// Returns the route of the type page of the type called `name`.
fn type_route(name: &str) -> Route {
    Route::Type {
        name: String::from(name),
        field: None,
        arg: None,
    }
}

/// Checks that the type `ty` declares the field `field` and the field the
/// argument `arg`, where a field of an enum is one of its values, which is
/// the rule the views check a type route by.
fn check_member(
    model: &Model,
    ty: TypeId,
    field: Option<&str>,
    arg: Option<&str>,
) -> Result<(), ViewError> {
    let Some(field) = field else {
        return Ok(());
    };
    let t = model.ty(ty);
    let unknown = || ViewError::UnknownField {
        ty: String::from(model.name(t.name)),
        field: String::from(field),
    };
    let found = t
        .fields
        .iter()
        .copied()
        .find(|&f| model.name(model.field(f).name) == field);
    let Some(f) = found else {
        let value = t
            .values
            .iter()
            .any(|&v| model.name(model.value(v).name) == field);
        return if value && arg.is_none() {
            Ok(())
        } else {
            Err(unknown())
        };
    };
    match arg {
        Some(arg)
            if !model
                .field(f)
                .args
                .iter()
                .any(|&a| model.name(model.arg(a).name) == arg) =>
        {
            Err(ViewError::UnknownArg {
                ty: String::from(model.name(t.name)),
                field: String::from(field),
                arg: String::from(arg),
            })
        }
        _ => Ok(()),
    }
}

/// Returns the page of a whole graph route, its diagram and the viewer link.
fn whole_page(
    schema: &Schema,
    clustering: &Clustering,
    route: &Route,
    root: &WholeRoot,
    options: &Options,
) -> Result<PageModel, ViewError> {
    let title = match root {
        WholeRoot::Query => "Whole graph from Query",
        WholeRoot::Mutation => "Whole graph from Mutation",
        WholeRoot::Subscription => "Whole graph from Subscription",
        WholeRoot::All => "Whole graph of every root",
    };
    graph_page(schema, clustering, route, String::from(title), options)
}

/// Returns a page that holds the diagram of `route` and the viewer link, the
/// page of a whole graph and of a path.
fn graph_page(
    schema: &Schema,
    clustering: &Clustering,
    route: &Route,
    title: String,
    options: &Options,
) -> Result<PageModel, ViewError> {
    let mut blocks = Vec::new();
    push_diagrams(schema, clustering, route, options, &mut blocks)?;
    blocks.push(Block::ViewerLink {
        route: route.clone(),
    });
    Ok(PageModel {
        route: route.clone(),
        title,
        kind: None,
        breadcrumb: overview_crumb(),
        blocks,
    })
}

/// Returns the page of a search, which names the text searched for.
fn search_page(route: &Route, text: &str) -> PageModel {
    PageModel {
        route: route.clone(),
        title: String::from("Search"),
        kind: None,
        breadcrumb: overview_crumb(),
        blocks: vec![
            Block::Note {
                text: format!("Results for {text}"),
            },
            Block::ViewerLink {
                route: route.clone(),
            },
        ],
    }
}

/// Returns the breadcrumb of a page right under the home page.
fn overview_crumb() -> Vec<(String, Route)> {
    vec![(String::from(OVERVIEW), Route::Home)]
}

/// Appends one [`Block::Diagram`] per part of the view of `route` under the
/// page budget, or one that carries the reason the view is skipped.
fn push_diagrams(
    schema: &Schema,
    clustering: &Clustering,
    route: &Route,
    options: &Options,
    blocks: &mut Vec<Block>,
) -> Result<(), ViewError> {
    let views = View::for_page(schema, clustering, route, options)?;
    for view in &views {
        blocks.push(Block::Diagram {
            route: route.clone(),
            part: view.part.map(|(n, _)| n),
            skipped: view.empty,
        });
    }
    Ok(())
}

/// Appends the description `text` when it holds more than white space.
fn push_description(text: Option<&str>, blocks: &mut Vec<Block>) {
    if let Some(text) = text.filter(|t| !t.trim().is_empty()) {
        blocks.push(Block::Description {
            text: String::from(text),
        });
    }
}

/// Appends the details of the descriptions a table cut, when it cut one.
fn push_details(entries: Vec<(ElementId, String)>, blocks: &mut Vec<Block>) {
    if !entries.is_empty() {
        blocks.push(Block::Details {
            title: title::DETAILS,
            entries,
        });
    }
}

/// Returns the empty cell.
fn empty() -> Cell {
    Cell::Text(String::new())
}

/// Returns the cell of the description `text` of the element `id`, and
/// records the whole text in `details` when more than its first paragraph
/// follows.
fn description_cell(
    text: Option<&str>,
    id: &ElementId,
    details: &mut Vec<(ElementId, String)>,
) -> Cell {
    let Some(text) = text.filter(|t| !t.trim().is_empty()) else {
        return empty();
    };
    let (first, more) = first_paragraph(text);
    let more = more.then(|| {
        details.push((id.clone(), String::from(text)));
        id.clone()
    });
    Cell::Description {
        first: String::from(first),
        more,
        source: DescriptionSource::Served,
    }
}

/// Returns the cell of the first paragraph of `text` alone, for a table that
/// refers to an element documented elsewhere.
fn summary_cell(text: Option<&str>) -> Cell {
    match text.filter(|t| !t.trim().is_empty()) {
        Some(text) => Cell::Description {
            first: String::from(first_paragraph(text).0),
            more: None,
            source: DescriptionSource::Served,
        },
        None => empty(),
    }
}

/// A fence of a fenced code block, its character and the length of its run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Fence {
    /// The byte of the run, a backtick or a tilde.
    byte: u8,
    /// The length of the run.
    len: usize,
}

/// Returns the first paragraph of the description `text` and whether more
/// than white space follows it.
///
/// The paragraph starts at the first line that is not blank and ends before
/// the next blank line outside a fenced code block, and its trailing white
/// space is dropped. The table cell of the `commonmark` subset cuts with this
/// function too, so a cell and its `Details` entry agree.
///
/// [CommonMark specification]: https://spec.commonmark.org/0.31.2/
pub(crate) fn first_paragraph(text: &str) -> (&str, bool) {
    let mut span: Option<(usize, usize)> = None;
    let mut ended = false;
    let mut fence = None;
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        let from = at;
        at += line.len();
        let body = line.trim_end_matches(['\n', '\r']);
        let blank = body.trim_matches([' ', '\t']).is_empty();
        match span {
            None if blank => {}
            Some(_) if ended => {
                if !blank {
                    return (paragraph(text, span), true);
                }
            }
            Some(_) if blank && fence.is_none() => ended = true,
            _ => {
                let start = span.map_or(from, |(s, _)| s);
                span = Some((start, from + body.len()));
                fence = fence_after(body, fence);
            }
        }
    }
    (paragraph(text, span), false)
}

/// Returns the text of `span` inside `text` without its trailing white space.
fn paragraph(text: &str, span: Option<(usize, usize)>) -> &str {
    span.map_or("", |(s, e)| text[s..e].trim_end())
}

/// Returns the fence that is open after the line `body`, given the fence
/// `open` before it.
///
/// A fence is a run of at least three backticks or tildes behind at most three
/// spaces, and a run of the same byte at least as long with nothing but white
/// space after it closes it.
///
/// [CommonMark specification]: https://spec.commonmark.org/0.31.2/
fn fence_after(body: &str, open: Option<Fence>) -> Option<Fence> {
    let rest = body.trim_start_matches(' ');
    if body.len() - rest.len() > 3 {
        return open;
    }
    let Some(&byte) = rest.as_bytes().first().filter(|b| matches!(b, b'`' | b'~')) else {
        return open;
    };
    let len = rest.bytes().take_while(|&b| b == byte).count();
    if len < 3 {
        return open;
    }
    match open {
        None => Some(Fence { byte, len }),
        Some(f) if f.byte == byte && len >= f.len && rest[len..].trim().is_empty() => None,
        Some(f) => Some(f),
    }
}

/// Returns the cell of the deprecation `mark`, or the empty cell.
fn deprecation_cell(mark: Option<&Marked>) -> Cell {
    match mark {
        Some(Marked::Directive { reason }) => Cell::Deprecation {
            reason: reason.clone(),
            from_tag: false,
        },
        Some(Marked::Tag { reason }) => Cell::Deprecation {
            reason: reason.clone(),
            from_tag: true,
        },
        None => empty(),
    }
}

/// Returns the parts of `type_ref` in reading order, such as `[`, `Project`,
/// `!`, `]` for `[Project!]`.
fn type_ref_parts(model: &Model, type_ref: &TypeRef) -> Vec<TypeRefPart> {
    let mut parts = Vec::new();
    let mut close = Vec::new();
    let mut cur = type_ref;
    for _ in 0..MAX_WRAPPERS {
        match cur {
            TypeRef::Named(id) => {
                parts.push(TypeRefPart::Name {
                    text: String::from(model.name(model.ty(*id).name)),
                    id: ElementId::of_type(model, *id),
                });
                break;
            }
            TypeRef::List(inner) => {
                parts.push(TypeRefPart::Open("["));
                close.push("]");
                cur = inner;
            }
            TypeRef::NonNull(inner) => {
                close.push("!");
                cur = inner;
            }
        }
    }
    parts.extend(close.into_iter().rev().map(TypeRefPart::Close));
    parts
}

/// Returns the cell of the type reference `type_ref`.
fn type_ref_cell(model: &Model, type_ref: &TypeRef) -> Cell {
    Cell::TypeRef {
        parts: type_ref_parts(model, type_ref),
    }
}

/// Returns a cell that links to the element `id` under the text `text`.
fn link(text: String, id: ElementId) -> Cell {
    Cell::TypeRef {
        parts: vec![TypeRefPart::Name { text, id }],
    }
}

/// Returns a cell that links to the type `ty` under its name.
fn type_link(model: &Model, ty: TypeId) -> Cell {
    link(
        String::from(model.name(model.ty(ty).name)),
        ElementId::of_type(model, ty),
    )
}

/// Returns the type the field `f` shows on a page, the node type of its
/// collapse when the Relay collapse collapsed it, such as `[Project!]` for a
/// connection over `Project!`.
fn shown_type(schema: &Schema, f: FieldId) -> &TypeRef {
    schema
        .transformed
        .collapse_of(f)
        .map_or(&schema.model.field(f).type_ref, |c| &c.node)
}

/// Returns `text` and `id` of a reference to the field `f`, such as
/// `Organization.projects`.
fn field_ref(model: &Model, f: FieldId) -> (String, ElementId) {
    let field = model.field(f);
    let owner = model.name(model.ty(field.owner).name);
    (
        format!("{owner}.{}", model.name(field.name)),
        ElementId::of_field(model, f),
    )
}

/// Returns the sentence of the source that produced `clustering`.
fn source_note(clustering: &Clustering) -> Block {
    let text = match clustering.source {
        ClusterSource::Declared => "The clusters come from the declared map.",
        ClusterSource::Stems => "The clusters come from the name stems.",
        ClusterSource::Territories => "The clusters come from the root field territories.",
        ClusterSource::TerritoriesFallback => {
            "The clusters come from the root field territories, because neither grouping passes the three checks."
        }
    };
    Block::Note {
        text: String::from(text),
    }
}

/// Returns `n` as a `u32`, saturated.
///
/// Every count a page numbers is capped far below `u32::MAX` by
/// [`ReadLimits`](crate::limits::ReadLimits), so the saturation never
/// happens.
fn count32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// The two lints of reachability of `docs/aoviewer.md` section 2.3.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Lint {
    /// Reached only as an implementation.
    ImplementationOnly,
    /// Not reachable by Voyager.
    NotVoyager,
}

/// Returns whether the node `n` carries `lint`.
///
/// Reached only as an implementation is the walk of aoviewer minus the strict
/// walk, and not reachable by Voyager is the walk of aoviewer minus the walk
/// of Voyager. Both lints judge the objects, interfaces and unions alone, the
/// types the section counts its figures over, since the strict walk follows no
/// argument and Voyager's walk enters no other kind, so an input or an enum
/// would carry a lint about the walk and not about the type.
fn has_lint(schema: &Schema, n: NodeId, lint: Lint) -> bool {
    let graph = &schema.graph;
    let walks = graph.walks();
    let kind = schema.model.ty(graph.nodes[n.index()].ty).kind;
    let composite = matches!(kind, Kind::Object | Kind::Interface | Kind::Union);
    let other = match lint {
        Lint::ImplementationOnly => &walks.strict,
        Lint::NotVoyager => &walks.voyager,
    };
    composite && walks.aoviewer.contains(n) && !other.contains(n)
}

/// Returns the types of `types` sorted by [`name_order`].
fn by_name(schema: &Schema, types: impl Iterator<Item = TypeId>) -> Vec<TypeId> {
    let mut out: Vec<TypeId> = types.collect();
    out.sort_by(|&a, &b| name_order(&schema.model, a, b));
    out
}

/// Sorts `names` by the bytes of their text, then by element, and drops the
/// repeats.
fn sort_names(names: &mut Vec<(String, ElementId)>) {
    names.sort_unstable();
    names.dedup();
}
