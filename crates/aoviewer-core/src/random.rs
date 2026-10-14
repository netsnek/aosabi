// SPDX-License-Identifier: MIT
/*
 * aoviewer-core random: the seeded random graphs of the termination case.
 *
 * The generator lives in the core, so the same graphs run natively and inside
 * the module.
 */

//! The random graphs.
//!
//! The termination case lays out 10,000 seeded random graphs and counts the
//! iterations of every loop against its cap, never time. The graphs have 2 to
//! 200 nodes and take the shape of trees, dense cycles, complete tournaments
//! of 80 nodes and wide bipartite graphs. A tournament of 80 nodes has the
//! size of the graph on which a published ranking phase does not terminate. A
//! fifth shape scatters edges between random ends, so a graph can fall apart
//! into parts no root reaches.
//!
//! A graph is a schema of object types joined by fields, and the first type is
//! the query root, so the case can ask for the whole graph from the query
//! root, the whole graph of every root and the ego view of the first node
//! alike. A field names its target through one of four wrappers, a type may
//! reference itself, and two fields of one type may name one target. Every
//! name is a prefix letter, the index and a tail of lowercase letters, and one
//! name in 16 is longer than a box shows, so boxes of many widths and the cut
//! with an ellipsis are drawn. A type name starts with `T` and a digit and a
//! field name with `f` and a digit, so no name is one the Relay rule compares.
//! The graph holds object types alone, with no interface, no description, no
//! deprecation and no name that starts with `__`, so under the default options
//! no other transform marks, folds or hides a part of it either. A glob of
//! `hide=` the user sets may still hide its types.
//!
//! The generator uses integers alone, so a seed draws the same graphs on
//! every target. Every loop runs over the nodes of one graph, which the cap
//! `MAX_NODES` bounds, over the pairs of a tournament, which
//! `TOURNAMENT_NODES` bounds, over the edges of one graph, which `MAX_EDGES`
//! bounds, or over the letters of one name, which `MAX_TAIL` bounds.

use alloc::{
    boxed::Box,
    format,
    string::String,
    vec,
    vec::Vec, //
};

use crate::{
    limits::ReadLimits,
    model::{
        Builder,
        Kind,
        Model,
        ModelError,
        TypeRefSpec, //
    },
    text::MAX_NAME_CELLS,
};

/// The fewest nodes of a random graph.
const MIN_NODES: u32 = 2;

/// The most nodes of a random graph.
const MAX_NODES: u32 = 200;

/// The nodes of a complete tournament.
const TOURNAMENT_NODES: u32 = 80;

/// The fewest nodes of a wide bipartite graph, so its wide side has at least
/// 32 nodes.
const MIN_WIDE_NODES: u32 = 40;

/// The most nodes on the narrow side of a wide bipartite graph.
const MAX_NARROW: u32 = 8;

/// The most chords a node of a dense cycle adds to its ring edge.
const MAX_CHORDS: u32 = 4;

/// The most edges a scattered graph draws per node.
const SCATTER_PER_NODE: u32 = 2;

/// The most edges of a random graph, which the complete tournament reaches,
/// one edge per pair of its nodes.
const MAX_EDGES: u32 = TOURNAMENT_NODES * (TOURNAMENT_NODES - 1) / 2;

// Every shape stays under the edge cap: a dense cycle draws its ring edge and
// its chords per node, a wide bipartite graph one edge per pair across its
// sides at most, and a scattered graph its edges per node.
const _: () = assert!(MAX_NODES * (1 + MAX_CHORDS) <= MAX_EDGES);
const _: () = assert!(MAX_NARROW * (MAX_NODES - 1) <= MAX_EDGES);
const _: () = assert!(SCATTER_PER_NODE * MAX_NODES <= MAX_EDGES);

/// The chance of a long name, one in this many.
const LONG_ONE_IN: u32 = 16;

/// The most letters of the tail of a short name.
const MAX_SHORT_TAIL: u32 = 7;

/// The spread of the tail of a long name above the cut, in letters.
const LONG_SPREAD: u32 = 16;

/// The most letters of the tail of a name.
///
/// A long tail is one letter short of [`MAX_NAME_CELLS`] plus up to
/// [`LONG_SPREAD`] letters, and the prefix letter and at least one digit
/// before it make every long name longer than a box shows.
const MAX_TAIL: u32 = MAX_NAME_CELLS - 1 + LONG_SPREAD;

/// The start of the state of Marsaglia's 64 bit generator, which the seed is
/// mixed into.
///
/// [Xorshift RNGs]: https://doi.org/10.18637/jss.v008.i14
const SEED_MIX: u64 = 88_172_645_463_325_252;

/// The number of shapes a graph draws its shape from.
const SHAPES: u32 = 5;

/// The number of wrappers a field draws the reference to its target from.
const WRAPPERS: u32 = 4;

/// A seeded xorshift generator over `u64`.
///
/// The seed is mixed into the start of the paper's state, and a seed that
/// would leave the state at zero, where the generator stays, starts from the
/// paper's state itself.
///
/// [Xorshift RNGs]: https://doi.org/10.18637/jss.v008.i14
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rng {
    state: u64,
}

impl Rng {
    /// Returns a generator seeded with `seed`.
    ///
    /// Two generators of one seed draw the same numbers on every target.
    #[must_use]
    pub fn new(seed: u64) -> Rng {
        let state = seed ^ SEED_MIX;
        Rng {
            state: if state == 0 { SEED_MIX } else { state },
        }
    }

    /// Returns the next 32 bits of the generator, the upper half of its next
    /// state.
    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        u32::try_from(x >> 32).unwrap_or(0)
    }

    /// Returns a number below `n`, or 0 when `n` is 0.
    ///
    /// The draw times `n` is shifted right by 32 bits, which is Lemire's
    /// multiply shift map without its rejection step. That step keeps exactly
    /// the floor of 2^32 over `n` draws for every number below `n` and drops,
    /// for each number, the products of a range shorter than `n`, where one
    /// multiple of `n` fits at most. So every number below `n` answers the
    /// floor or the ceiling of 2^32 over `n` of the 2^32 draws. The rejection
    /// step loops, and a termination case needs graphs of every shape more
    /// than a perfectly even draw.
    ///
    /// [Fast Random Integer Generation in an Interval]: https://arxiv.org/abs/1805.10941
    pub fn below(&mut self, n: u32) -> u32 {
        let product = u64::from(self.next_u32()) * u64::from(n);
        u32::try_from(product >> 32).unwrap_or(0)
    }

    /// Returns a number from `lo` to `hi`, both included, for `lo <= hi`.
    fn between(&mut self, lo: u32, hi: u32) -> u32 {
        lo + self.below(hi - lo + 1)
    }
}

/// The directed edges of one graph, each from the index of a source node to
/// the index of a target node.
type Edges = Vec<(u32, u32)>;

/// Returns the next random graph of `rng` as a model under `limits`.
///
/// The shape is drawn first, then the node count, the edges, the names and
/// the wrapper of every field, so a seed and the number of graphs drawn
/// before fix the graph. A graph has at most 200 object types and 3160
/// fields.
///
/// # Errors
///
/// Returns the [`ModelError`] of the [`Builder`] when `limits` refuse a type,
/// a field or a name of the graph. [`ReadLimits::DEFAULT`] never does: a graph
/// needs at most 200 types, 3160 fields, names of 67 bytes and three wrappers
/// per reference, and it admits 20,000 types, 400,000 members, names of 1024
/// bytes and a depth of 128.
pub fn random_graph(rng: &mut Rng, limits: &ReadLimits) -> Result<Model, ModelError> {
    let (nodes, edges) = match rng.below(SHAPES) {
        0 => tree(rng),
        1 => dense_cycle(rng),
        2 => tournament(rng),
        3 => bipartite(rng),
        _ => scattered(rng),
    };
    build(rng, limits, nodes, &edges)
}

/// Returns a tree whose edges run from each parent to its children.
///
/// The parent of a node is one of the `reach` nodes before it, and `reach` is
/// drawn once per tree, so a reach of 1 draws a chain and a reach of every
/// node a random recursive tree.
fn tree(rng: &mut Rng) -> (u32, Edges) {
    let nodes = rng.between(MIN_NODES, MAX_NODES);
    let reach = rng.between(1, nodes);
    let mut edges = Vec::with_capacity(nodes as usize);
    for child in 1..nodes {
        let parent = child - 1 - rng.below(reach.min(child));
        edges.push((parent, child));
    }
    (nodes, edges)
}

/// Returns a ring through every node with chords to random nodes, the node
/// itself and the targets of its other edges among them.
fn dense_cycle(rng: &mut Rng) -> (u32, Edges) {
    let nodes = rng.between(MIN_NODES, MAX_NODES);
    let mut edges = Vec::new();
    for from in 0..nodes {
        edges.push((from, (from + 1) % nodes));
        for _ in 0..rng.below(MAX_CHORDS + 1) {
            edges.push((from, rng.below(nodes)));
        }
    }
    (nodes, edges)
}

/// Returns a complete tournament, one edge in a drawn direction between every
/// two nodes.
fn tournament(rng: &mut Rng) -> (u32, Edges) {
    let mut edges = Vec::with_capacity(MAX_EDGES as usize);
    for a in 0..TOURNAMENT_NODES {
        for b in a + 1..TOURNAMENT_NODES {
            let edge = if rng.below(2) == 0 { (a, b) } else { (b, a) };
            edges.push(edge);
        }
    }
    (TOURNAMENT_NODES, edges)
}

/// Returns a bipartite graph between a narrow side, which holds the first
/// node, and a wide side.
///
/// A pair across the sides is joined from the narrow side in half of the
/// draws, from the wide side in a quarter and not at all in the last quarter.
fn bipartite(rng: &mut Rng) -> (u32, Edges) {
    let nodes = rng.between(MIN_WIDE_NODES, MAX_NODES);
    let narrow = rng.between(1, MAX_NARROW);
    let mut edges = Vec::new();
    for a in 0..narrow {
        for b in narrow..nodes {
            match rng.below(4) {
                0 | 1 => edges.push((a, b)),
                2 => edges.push((b, a)),
                _ => {}
            }
        }
    }
    (nodes, edges)
}

/// Returns edges between random ends, which may leave nodes no edge touches.
fn scattered(rng: &mut Rng) -> (u32, Edges) {
    let nodes = rng.between(MIN_NODES, MAX_NODES);
    let count = rng.below(SCATTER_PER_NODE * nodes + 1);
    let mut edges = Vec::with_capacity(count as usize);
    for _ in 0..count {
        edges.push((rng.below(nodes), rng.below(nodes)));
    }
    (nodes, edges)
}

/// Returns the model of `nodes` object types joined by `edges`.
///
/// The types are added in node order, the first is the query root, and a
/// field is added to its source in edge order.
///
/// # Errors
///
/// Returns the [`ModelError`] of the [`Builder`].
fn build(
    rng: &mut Rng,
    limits: &ReadLimits,
    nodes: u32,
    edges: &[(u32, u32)],
) -> Result<Model, ModelError> {
    let mut builder = Builder::new(limits);
    let mut names = Vec::with_capacity(nodes as usize);
    let mut ids = Vec::with_capacity(nodes as usize);
    for node in 0..nodes {
        let type_name = name(rng, 'T', node);
        ids.push(builder.add_type(Kind::Object, &type_name, None)?);
        names.push(type_name);
    }
    builder.roots(names.first().map(String::as_str), None, None);
    let mut counts = vec![0_u32; nodes as usize];
    for &(from, to) in edges {
        let (Some(&on), Some(target), Some(count)) = (
            ids.get(from as usize),
            names.get(to as usize),
            counts.get_mut(from as usize),
        ) else {
            continue;
        };
        let field_name = name(rng, 'f', *count);
        *count += 1;
        let type_ref = wrap(rng, target);
        builder.add_field(on, &field_name, &type_ref, None, None, None)?;
    }
    builder.finish()
}

/// Returns the name of the element `index` of a scope, `prefix` and the index
/// followed by a drawn tail of lowercase letters.
fn name(rng: &mut Rng, prefix: char, index: u32) -> String {
    let tail = if rng.below(LONG_ONE_IN) == 0 {
        MAX_NAME_CELLS - 1 + rng.below(LONG_SPREAD + 1)
    } else {
        rng.below(MAX_SHORT_TAIL + 1)
    };
    let mut name = format!("{prefix}{index}");
    for _ in 0..tail.min(MAX_TAIL) {
        name.push(char::from_u32(u32::from('a') + rng.below(26)).unwrap_or('a'));
    }
    name
}

/// Returns a reference to the type `target` under a drawn wrapper, bare, non
/// null, a list of non null or a non null list of non null.
fn wrap(rng: &mut Rng, target: &str) -> TypeRefSpec {
    let named = TypeRefSpec::Named(String::from(target));
    let non_null = |inner| TypeRefSpec::NonNull(Box::new(inner));
    let list = |inner| TypeRefSpec::List(Box::new(inner));
    match rng.below(WRAPPERS) {
        0 => named,
        1 => non_null(named),
        2 => list(non_null(named)),
        _ => non_null(list(non_null(named))),
    }
}
