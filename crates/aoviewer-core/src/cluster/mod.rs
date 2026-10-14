// SPDX-License-Identifier: MIT
/*
 * aoviewer-core cluster: the clustering and its picking rule.
 *
 * Clusters come from declared domains, name stems and root field territories,
 * and one rule picks among them.
 */

//! The clustering.
//!
//! [`Clustering::pick`] places the nodes of a [`Schema`]'s graph into
//! clusters. The nodes it places are every node of the graph but the roots,
//! which are pages of their own. The clusters, the other bucket and the types
//! outside of `zitadel-gql` add up to 75 of its 77 composite types. The two
//! types the counts leave out are its query and its mutation root, so every
//! node but the roots is placed.
//!
//! Three sources place the nodes.
//!
//! - A declared map, read in `declared.rs`, places a node in the first
//!   declared cluster one of whose globs matches its name, and in the other
//!   bucket when none does.
//! - The name stems, read in `stems.rs`, group the nodes by the first word of
//!   their names once the role and verb words are stripped.
//! - The root field territories, read in `territory.rs`, give each node the
//!   query root field whose breadth first walk reaches it first, and leave
//!   outside the nodes neither the territory walks nor the interface vote
//!   place.
//!
//! Both automatic sources then merge every group under three members into the
//! group it shares the most edges with. A small group that shares no edge with
//! another group moves into the other bucket, and the first group in name
//! order wins a tie.
//!
//! **The rule that picks.** A declared map wins whenever the caller passes
//! one. Otherwise both automatic clusterings are judged by three checks, which
//! are that no cluster holds more than half of the placed types, that the
//! other bucket holds at most a quarter of them, and that a graph of more than
//! 40 boxes has at least three clusters. Among those that pass, the one with
//! the larger share of edges inside a cluster is kept, and territories are
//! used anyway when neither passes. The share counts the edges of the graph
//! between two nodes a cluster or the other bucket holds, and an edge is
//! inside when one cluster holds both of its ends. Two equal shares keep the
//! territories.
//!
//! **The split.** A cluster above the size cap, the larger of 24 and the
//! placed type count divided by 120, splits along the breadth first order of
//! the territory walks into parts of the cap, each named after its first type.
//! The division rounds up. The section derives from the cap that a cluster map
//! never exceeds 120 boxes, but the split bounds the size of a part and not
//! the count of the clusters, so the clusters of a large schema can outnumber 120.
//! The three checks judge the clusters before the split, because a split
//! cluster never holds more than half of a large schema and the first check
//! would judge nothing. The split applies to the clusters of every source.
//!
//! **The order.** The clusters follow the byte order of their names, the parts
//! of a split cluster taking its place in breadth first order, and the types
//! of a cluster, of the other bucket and of the outside list follow
//! [`name_order`], so the order of the types in the input never reaches the
//! clustering. Every tie among the groups breaks by name for the same reason.
//!
//! Every loop runs over the nodes and edges of one graph, over the types of
//! one model, over the bytes or words of one name, or over at most
//! [`MAX_DECLARED`] declared clusters of at most [`MAX_DECLARED_GLOBS`] globs
//! each. The loop that merges the small groups pops a heap that receives at
//! most two entries per group, and the loop that numbers a slug tries at most
//! one more candidate than there are slugs taken.

mod declared;
mod stems;
mod territory;

use alloc::{
    collections::BinaryHeap,
    string::String,
    vec,
    vec::Vec, //
};
use core::{
    cmp::{
        Ordering,
        Reverse, //
    },
    fmt::Write as _,
};

use crate::{
    graph::{
        Graph,
        NodeId, //
    },
    model::{
        Model,
        TypeId, //
    },
    schema::Schema,
    sort::name_order,
};

/// The most declared clusters [`Clustering::pick`] reads.
///
/// A longer list is cut after this many clusters.
pub const MAX_DECLARED: usize = 256;

/// The most globs of one declared cluster [`Clustering::pick`] reads.
///
/// A longer list is cut after this many globs, the cap of `hide=` as well,
/// [󰀁](crate::transform::MAX_HIDE_GLOBS).
pub const MAX_DECLARED_GLOBS: usize = 64;

/// The size under which an automatic group merges into its strongest
/// neighbour, clusters under three members.
const SMALL: usize = 3;

/// The smallest size cap of a cluster.
const MIN_CAP: usize = 24;

/// The divisor of the size cap, the count of boxes docs/aoviewer.md gives as
/// the bound of a cluster map.
///
/// The size cap it enters as the divisor bounds the size of a part and not the
/// count of the clusters, so the map of a large schema can hold more boxes
/// than this.
const MAP_BOXES: usize = 120;

/// The boxes above which a clustering needs [`MIN_CLUSTERS`] clusters.
const CHECK_BOXES: usize = 40;

/// The fewest clusters a graph above [`CHECK_BOXES`] boxes needs.
const MIN_CLUSTERS: usize = 3;

/// The slug of a name that holds no letter and no digit.
const EMPTY_SLUG: &str = "cluster";

/// The mark of a node in the other bucket.
const OTHER: u32 = u32::MAX - 1;

/// The mark of a node outside every cluster and the other bucket, and of a
/// root.
const OUTSIDE: u32 = u32::MAX;

/// The source that produced a clustering.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClusterSource {
    /// A declared map of names to type name globs.
    Declared,
    /// The name stems, picked by the rule.
    Stems,
    /// The root field territories, picked by the rule.
    Territories,
    /// The root field territories, used because neither automatic clustering
    /// passes the three checks.
    TerritoriesFallback,
}

/// A declared cluster, a name and the type name globs it holds.
///
/// The file `aosabi.docs.toml` maps each name to its globs. A glob holds `*`,
/// `?` and the bytes of a name, and one that holds anything else matches no
/// type.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Declared {
    /// The name of the cluster.
    pub name: String,
    /// The type name globs, of which at most [`MAX_DECLARED_GLOBS`] are read.
    pub globs: Vec<String>,
}

/// One cluster of a clustering.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cluster {
    /// The name, which a declared map, a stem, a root field or the first type
    /// of a part gives.
    pub name: String,
    /// The slug of the name, numbered when an earlier cluster holds the same
    /// one, which the routes and the cluster ids carry.
    pub slug: String,
    /// The types in [`name_order`].
    pub types: Vec<TypeId>,
    /// The edges of the graph whose two ends the cluster holds.
    pub inside: u32,
    /// The edges of the graph with one end in the cluster and the other in
    /// another cluster or in the other bucket.
    pub across: u32,
}

/// The picked clustering: a declared map wins, otherwise the automatic source
/// with the larger share of inside edges among those that pass the three
/// checks, and territories anyway when neither does.
///
/// Beside its public fields a clustering holds a private index of the cluster
/// of every type, which [`Clustering::pick`] builds from the clusters it
/// places and [`Clustering::of`] reads, so a lookup costs no scan. Code
/// outside this module therefore gets a clustering from [`Clustering::pick`]
/// and never from a struct literal. The index reflects the clusters as built,
/// so a caller that reorders, filters or edits [`Clustering::clusters`] no
/// longer reads them through [`Clustering::of`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Clustering {
    /// The source that produced the clusters.
    pub source: ClusterSource,
    /// The clusters in the order of their names, the parts of a split cluster
    /// in its place.
    pub clusters: Vec<Cluster>,
    /// The types the source places in no cluster, in [`name_order`].
    pub other: Vec<TypeId>,
    /// The types neither the territory walks nor the interface vote places,
    /// in [`name_order`], which only the territories leave.
    pub outside: Vec<TypeId>,
    /// The cluster of every type as built, indexed by type, or [`OUTSIDE`].
    index: Vec<u32>,
}

/// The groups one source places the nodes in, before they become clusters.
#[derive(Clone, Debug)]
struct Placement {
    /// The name of every group.
    names: Vec<String>,
    /// The group of every node, indexed by node, [`OTHER`] or [`OUTSIDE`].
    at: Vec<u32>,
}

/// The edges a clustering counts inside a cluster and across.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Share {
    /// The edges one cluster holds both ends of.
    inside: u32,
    /// The other edges between two nodes a cluster or the other bucket holds.
    across: u32,
}

/// The three checks of the picking rule, each true when it passes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Checks {
    /// No cluster holds more than half of the placed types.
    half: bool,
    /// The other bucket holds at most a quarter of them.
    quarter: bool,
    /// A graph above [`CHECK_BOXES`] boxes has [`MIN_CLUSTERS`] clusters.
    three: bool,
}

impl Checks {
    /// Returns whether all three checks pass.
    fn pass(self) -> bool {
        self.half && self.quarter && self.three
    }
}

/// Returns `i` as a `u32`, saturated.
///
/// Every count the clustering numbers is capped far below `u32::MAX` by
/// [`ReadLimits`](crate::limits::ReadLimits), so the saturation never
/// happens.
fn id32(i: usize) -> u32 {
    u32::try_from(i).unwrap_or(u32::MAX)
}

impl Clustering {
    /// Places the types of `schema` into clusters, from `declared` when it
    /// holds a cluster and by the picking rule otherwise.
    ///
    /// The declared clusters are read in their order, at most
    /// [`MAX_DECLARED`] of them, and a type joins the first whose globs
    /// match its name. The clusters come out in the order of their names,
    /// with every cluster above the size cap split into parts.
    ///
    /// # Panics
    ///
    /// Panics when `schema.graph` was not built from `schema.model`.
    #[must_use]
    pub fn pick(schema: &Schema, declared: &[Declared]) -> Clustering {
        let (territories, rank) = territory::place(schema);
        let (source, placement) = if declared.is_empty() {
            let stems = stems::place(schema);
            choose(schema, stems, territories)
        } else {
            let declared = &declared[..declared.len().min(MAX_DECLARED)];
            (ClusterSource::Declared, declared::place(schema, declared))
        };
        assemble(schema, source, &placement, Some(&rank)).0
    }

    /// Returns the position in [`Clustering::clusters`] of the cluster that
    /// holds `ty`.
    ///
    /// A type in the other bucket or outside, a root, and a type that is no
    /// node of the graph answer `None`. The answer comes from the index
    /// [`Clustering::pick`] built, so it holds for the clusters as built and
    /// not after a caller edits [`Clustering::clusters`].
    #[must_use]
    pub fn of(&self, ty: TypeId) -> Option<u32> {
        self.index
            .get(ty.index())
            .copied()
            .filter(|&c| c != OUTSIDE)
    }

    /// Writes both automatic clusterings with their shares, for the notebook.
    ///
    /// Each clustering is judged before any split, as the picking rule judges
    /// it. It opens with the line
    /// `clustering <source> types <placed> clusters <count> largest <size>
    /// other <size> outside <size> inside <edges> across <edges> checks
    /// <result>`, where `<result>` is `pass` or the failed checks among
    /// `half`, `quarter` and `three` joined by commas. A line
    /// `cluster <slug> <size> <inside> <across> <types>` follows per cluster,
    /// then `other <types>` and `outside <types>`, the types separated by
    /// spaces. The stems come first and the territories second, and the last
    /// line is `pick <source>`, the source the rule picks without a declared
    /// map. Every line ends with a line feed.
    ///
    /// # Panics
    ///
    /// Panics when `schema.graph` was not built from `schema.model`.
    pub fn table(schema: &Schema, out: &mut String) {
        let (territories, _) = territory::place(schema);
        let stems = stems::place(schema);
        for (source, placement) in [
            (ClusterSource::Stems, &stems),
            (ClusterSource::Territories, &territories),
        ] {
            let (clustering, share) = assemble(schema, source, placement, None);
            let judged = checks(schema, &clustering);
            write_table(schema, &clustering, share, judged, out);
        }
        let (source, _) = choose(schema, stems, territories);
        out.push_str("pick ");
        out.push_str(source_label(source));
        out.push('\n');
    }
}

/// Returns the cluster name lower cased, every run outside `[a-z0-9]` one
/// hyphen.
///
/// A run at either end is dropped rather than written as a hyphen, and a name
/// with no letter and no digit reads `cluster`, so a slug is always a nonempty
/// run of lower case letters, digits and inner hyphens. A byte outside ASCII
/// belongs to a run. [`Clustering::pick`] numbers a slug an earlier cluster
/// holds with `-2`, then `-3`, which is the duplicate rule of the anchors.
#[must_use]
pub fn slug(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut gap = false;
    for b in name.bytes().map(|b| b.to_ascii_lowercase()) {
        if b.is_ascii_lowercase() || b.is_ascii_digit() {
            if gap && !out.is_empty() {
                out.push('-');
            }
            gap = false;
            out.push(char::from(b));
        } else {
            gap = true;
        }
    }
    if out.is_empty() {
        out.push_str(EMPTY_SLUG);
    }
    out
}

/// Returns the source and the placement the picking rule keeps among the
/// `stems` and the `territories` of `schema`.
fn choose(schema: &Schema, stems: Placement, territories: Placement) -> (ClusterSource, Placement) {
    let (by_stems, stems_share) = assemble(schema, ClusterSource::Stems, &stems, None);
    let (by_territories, territories_share) =
        assemble(schema, ClusterSource::Territories, &territories, None);
    let stems_pass = checks(schema, &by_stems).pass();
    let territories_pass = checks(schema, &by_territories).pass();
    let stems_win = match (stems_pass, territories_pass) {
        (true, true) => compare(stems_share, territories_share) == Ordering::Greater,
        (true, false) => true,
        (false, _) => false,
    };
    if stems_win {
        (ClusterSource::Stems, stems)
    } else if territories_pass {
        (ClusterSource::Territories, territories)
    } else {
        (ClusterSource::TerritoriesFallback, territories)
    }
}

/// Compares the share of the inside edges of `a` with that of `b`.
///
/// A clustering that counts no edge has a share of zero.
fn compare(a: Share, b: Share) -> Ordering {
    let total = |s: Share| u64::from(s.inside) + u64::from(s.across);
    let left = u64::from(a.inside) * total(b).max(1);
    let right = u64::from(b.inside) * total(a).max(1);
    left.cmp(&right)
}

/// Returns the three checks of `clustering` over the graph of `schema`.
fn checks(schema: &Schema, clustering: &Clustering) -> Checks {
    let placed = placed_count(&schema.graph);
    let largest = clustering
        .clusters
        .iter()
        .map(|c| c.types.len())
        .max()
        .unwrap_or(0);
    Checks {
        half: largest * 2 <= placed,
        quarter: clustering.other.len() * 4 <= placed,
        three: schema.graph.nodes.len() <= CHECK_BOXES || clustering.clusters.len() >= MIN_CLUSTERS,
    }
}

/// Returns the count of the nodes of `graph` a clustering places, every node
/// but the roots.
fn placed_count(graph: &Graph) -> usize {
    graph.nodes.iter().filter(|n| !n.is_root).count()
}

/// Returns the size cap of a cluster over `placed` placed types.
fn size_cap(placed: usize) -> usize {
    MIN_CAP.max(placed.div_ceil(MAP_BOXES))
}

/// Returns the node of the id `i`.
fn node(i: usize) -> NodeId {
    NodeId(id32(i))
}

/// Returns the type of the node `n` of `graph`.
fn type_of(graph: &Graph, n: NodeId) -> TypeId {
    graph.nodes[n.index()].ty
}

/// Returns the name of the type `ty`.
fn type_name(model: &Model, ty: TypeId) -> &str {
    model.name(model.ty(ty).name)
}

/// Sorts `nodes` of the graph of `schema` by the [`name_order`] of their
/// types.
fn sort_by_name(schema: &Schema, nodes: &mut [NodeId]) {
    let (model, graph) = (&schema.model, &schema.graph);
    nodes.sort_by(|&a, &b| name_order(model, type_of(graph, a), type_of(graph, b)));
}

impl Placement {
    /// Returns a placement of `nodes` nodes, every node outside, with the
    /// groups `names`.
    fn new(names: Vec<String>, nodes: usize) -> Placement {
        Placement {
            names,
            at: vec![OUTSIDE; nodes],
        }
    }

    /// Returns the nodes of every group, indexed by group, each in node
    /// order.
    fn members(&self) -> Vec<Vec<NodeId>> {
        let mut members = vec![Vec::new(); self.names.len()];
        for (i, &g) in self.at.iter().enumerate() {
            if let Some(list) = members.get_mut(g as usize) {
                list.push(node(i));
            }
        }
        members
    }

    /// Merges every group under [`SMALL`] members into the group it shares
    /// the most edges of `graph` with, and moves a small group that shares
    /// none into the other bucket.
    ///
    /// The smallest group goes first, the first in the order of the groups
    /// on a tie, and the partner with the most shared edges wins, the first
    /// in the order of the groups on a tie. A small group that grows by a
    /// merge and stays small is judged again. The groups are in the byte
    /// order of their names, so every tie breaks by name.
    fn merge_small(&mut self, graph: &Graph) {
        let groups = self.names.len();
        let mut members = self.members();
        let mut heap: BinaryHeap<Reverse<(usize, usize)>> = members
            .iter()
            .enumerate()
            .filter(|(_, m)| !m.is_empty() && m.len() < SMALL)
            .map(|(g, m)| Reverse((m.len(), g)))
            .collect();
        let mut tally = vec![0u32; groups];
        let mut touched = Vec::new();
        // A pop either finds a stale entry or empties a group, and a merge
        // pushes at most one entry, so the heap receives at most two entries
        // per group.
        for _ in 0..=2 * groups {
            let Some(Reverse((len, g))) = heap.pop() else {
                break;
            };
            if members[g].len() != len {
                continue;
            }
            self.tally(graph, &members[g], g, &mut tally, &mut touched);
            let partner = touched
                .iter()
                .copied()
                .max_by(|&a, &b| tally[a].cmp(&tally[b]).then(b.cmp(&a)));
            for &t in &touched {
                tally[t] = 0;
            }
            touched.clear();
            let moved = core::mem::take(&mut members[g]);
            let mark = partner.map_or(OTHER, id32);
            for n in &moved {
                self.at[n.index()] = mark;
            }
            if let Some(p) = partner {
                members[p].extend(moved);
                if members[p].len() < SMALL {
                    heap.push(Reverse((members[p].len(), p)));
                }
            }
        }
    }

    /// Counts in `tally` the edges of `graph` between the nodes `nodes` of
    /// the group `g` and every other group, and lists in `touched` each group
    /// it counts once.
    fn tally(
        &self,
        graph: &Graph,
        nodes: &[NodeId],
        g: usize,
        tally: &mut [u32],
        touched: &mut Vec<usize>,
    ) {
        for &n in nodes {
            let outs = graph
                .outbound(n)
                .iter()
                .map(|&e| graph.edges[e as usize].to);
            let ins = graph
                .inbound(n)
                .iter()
                .map(|&e| graph.edges[e as usize].from);
            for m in outs.chain(ins) {
                let h = self.at[m.index()] as usize;
                if h < tally.len() && h != g {
                    if tally[h] == 0 {
                        touched.push(h);
                    }
                    tally[h] += 1;
                }
            }
        }
    }
}

/// Builds the clustering of `placement` from `source` over `schema`, with the
/// share of its edges.
///
/// The groups become clusters in the byte order of their names, an empty
/// group dropped. With the breadth first `rank` of every node, a cluster above
/// the size cap splits into parts, and without it no cluster splits.
fn assemble(
    schema: &Schema,
    source: ClusterSource,
    placement: &Placement,
    rank: Option<&[u32]>,
) -> (Clustering, Share) {
    let (model, graph) = (&schema.model, &schema.graph);
    let mut members = placement.members();
    let mut groups: Vec<usize> = (0..members.len())
        .filter(|&g| !members[g].is_empty())
        .collect();
    let names = &placement.names;
    groups.sort_by(|&a, &b| names[a].as_bytes().cmp(names[b].as_bytes()).then(a.cmp(&b)));
    let cap = size_cap(placed_count(graph));
    let mut parts: Vec<(String, Vec<NodeId>)> = Vec::with_capacity(groups.len());
    for g in groups {
        let mut nodes = core::mem::take(&mut members[g]);
        match rank {
            Some(rank) if nodes.len() > cap => {
                nodes.sort_by_key(|n| rank[n.index()]);
                for chunk in nodes.chunks(cap) {
                    let first = String::from(type_name(model, type_of(graph, chunk[0])));
                    parts.push((first, chunk.to_vec()));
                }
            }
            _ => parts.push((names[g].clone(), nodes)),
        }
    }
    let mut at = placement.at.clone();
    let mut clusters = Vec::with_capacity(parts.len());
    for (c, (name, mut nodes)) in parts.into_iter().enumerate() {
        for n in &nodes {
            at[n.index()] = id32(c);
        }
        sort_by_name(schema, &mut nodes);
        clusters.push(Cluster {
            name,
            slug: String::new(),
            types: nodes.iter().map(|&n| type_of(graph, n)).collect(),
            inside: 0,
            across: 0,
        });
    }
    number_slugs(&mut clusters);
    let share = measure(graph, &at, &mut clusters);
    let clustering = Clustering {
        source,
        other: listed(schema, &at, OTHER),
        outside: listed(schema, &at, OUTSIDE),
        index: index(model, &clusters),
        clusters,
    };
    (clustering, share)
}

/// Returns the types of the nodes but the roots that `at` marks with `mark`,
/// in [`name_order`].
fn listed(schema: &Schema, at: &[u32], mark: u32) -> Vec<TypeId> {
    let graph = &schema.graph;
    let mut nodes: Vec<NodeId> = (0..at.len())
        .filter(|&i| at[i] == mark && !graph.nodes[i].is_root)
        .map(node)
        .collect();
    sort_by_name(schema, &mut nodes);
    nodes.into_iter().map(|n| type_of(graph, n)).collect()
}

/// Returns the cluster of every type of `model`, indexed by type, or
/// [`OUTSIDE`].
fn index(model: &Model, clusters: &[Cluster]) -> Vec<u32> {
    let mut index = vec![OUTSIDE; model.types().len()];
    for (c, cluster) in clusters.iter().enumerate() {
        for t in &cluster.types {
            index[t.index()] = id32(c);
        }
    }
    index
}

/// Counts the inside and across edges of every cluster of `clusters`, whose
/// nodes `at` marks, and returns the share over the whole graph.
fn measure(graph: &Graph, at: &[u32], clusters: &mut [Cluster]) -> Share {
    let mut share = Share::default();
    for edge in &graph.edges {
        let (a, b) = (at[edge.from.index()], at[edge.to.index()]);
        if a == OUTSIDE || b == OUTSIDE {
            continue;
        }
        if a == b && a != OTHER {
            clusters[a as usize].inside += 1;
            share.inside += 1;
            continue;
        }
        share.across += 1;
        for c in [a, b].into_iter().filter(|&c| c != OTHER) {
            clusters[c as usize].across += 1;
        }
    }
    share
}

/// Gives every cluster of `clusters` its slug, in order, numbering a slug an
/// earlier cluster holds with `-2`, then `-3`, up to the first free one.
fn number_slugs(clusters: &mut [Cluster]) {
    let mut taken: Vec<String> = Vec::with_capacity(clusters.len());
    for cluster in clusters.iter_mut() {
        let base = slug(&cluster.name);
        let candidate = free_slug(&taken, &base);
        if let Err(at) = taken.binary_search(&candidate) {
            taken.insert(at, candidate.clone());
        }
        cluster.slug = candidate;
    }
}

/// Returns `base`, or `base` with the first suffix `-2`, `-3` and on that
/// `taken` does not hold, `taken` being sorted.
///
/// The candidates are distinct, so one of the first `taken.len() + 1` is
/// free.
fn free_slug(taken: &[String], base: &str) -> String {
    let mut candidate = String::from(base);
    for run in 2..=taken.len() + 1 {
        if taken.binary_search(&candidate).is_err() {
            break;
        }
        candidate.clear();
        candidate.push_str(base);
        // Writing into a `String` cannot fail.
        let _ = write!(candidate, "-{run}");
    }
    candidate
}

/// Returns the word the table writes for `source`.
fn source_label(source: ClusterSource) -> &'static str {
    match source {
        ClusterSource::Declared => "declared",
        ClusterSource::Stems => "stems",
        ClusterSource::Territories => "territories",
        ClusterSource::TerritoriesFallback => "territories-fallback",
    }
}

/// Appends the table of one automatic clustering to `out`.
fn write_table(
    schema: &Schema,
    clustering: &Clustering,
    share: Share,
    checks: Checks,
    out: &mut String,
) {
    let model = &schema.model;
    let largest = clustering
        .clusters
        .iter()
        .map(|c| c.types.len())
        .max()
        .unwrap_or(0);
    // Writing into a `String` cannot fail.
    let _ = write!(
        out,
        "clustering {} types {} clusters {} largest {} other {} outside {} inside {} across {} checks ",
        source_label(clustering.source),
        placed_count(&schema.graph),
        clustering.clusters.len(),
        largest,
        clustering.other.len(),
        clustering.outside.len(),
        share.inside,
        share.across,
    );
    if checks.pass() {
        out.push_str("pass");
    } else {
        let failed = [
            (checks.half, "half"),
            (checks.quarter, "quarter"),
            (checks.three, "three"),
        ];
        let names: Vec<&str> = failed
            .iter()
            .filter(|(passed, _)| !passed)
            .map(|&(_, name)| name)
            .collect();
        out.push_str(&names.join(","));
    }
    out.push('\n');
    for c in &clustering.clusters {
        let _ = write!(
            out,
            "cluster {} {} {} {}",
            c.slug,
            c.types.len(),
            c.inside,
            c.across
        );
        write_types(model, &c.types, out);
    }
    out.push_str("other");
    write_types(model, &clustering.other, out);
    out.push_str("outside");
    write_types(model, &clustering.outside, out);
}

/// Appends a space and the name of every type of `types`, then a line feed,
/// to `out`.
fn write_types(model: &Model, types: &[TypeId], out: &mut String) {
    for &t in types {
        out.push(' ');
        out.push_str(type_name(model, t));
    }
    out.push('\n');
}
