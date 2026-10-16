// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool viewer random: the operation random, the termination case
 * over the seeded random graphs of the core.
 *
 * The case lays out 10,000 seeded graphs and counts the iterations of every
 * loop against its cap, never time.
 */

//! The `random` operation.
//!
//! The request `{seed, count, options}` draws `count` graphs from the
//! generator of the core seeded with `seed` and lays each out three times,
//! as the whole graph from the query root, as the whole graph of every root
//! and as the ego view of its first node, under the options of the option
//! string `options`. The views take a budget that admits every view the core
//! builds, because the budgets of the whole graph and of a page answer many
//! of these graphs as empty views over budget, which pass with no layout
//! work.
//!
//! The request names both `seed` and `count`, neither of them null, so a
//! seed is never taken from a default. A seed is a whole number up to
//! 2^53 less 1, the largest a double of the request carries with every
//! integer below it.
//!
//! It answers `graphs`, one entry per graph with its `nodes`, its `fields`
//! and the outcome of each layout under `query`, `all` and `ego`: the
//! iterations of the scene as a number, or a word, `empty` for a view that
//! draws nothing, `cap <loop>` for a layout that crosses its cap and
//! `refused <reason>` for a view the core refuses. A graph whose model the
//! core refuses answers only `model`, the word `refused <reason>`, and no
//! counts or layouts. `inside` counts the graphs whose three layouts stay
//! inside the caps, `most` is the largest count of iterations, and `caps`
//! names the caps of the layout, of which `steps` bounds the iterations and
//! the others bound the loops whose count the layout fixes.

use aosabi_json::{
    JsObject,
    JsValue, //
};
use aoviewer_core::{
    cluster::Clustering,
    layout::{
        LayoutError,
        MAX_STEPS,
        OVERLAP_PASSES,
        ROUTE_ITERATIONS,
        SWEEPS,
        WRAP_PASSES, //
    },
    limits::ReadLimits,
    random::{
        Rng,
        random_graph, //
    },
    route::{
        Options,
        Route,
        WholeRoot, //
    },
    schema::Schema,
    view::View, //
};

use super::{
    MAX_ITEMS,
    MAX_SAFE_INTEGER,
    OPEN,
    Stop,
    fatal,
    number,
    present,
    scene,
    type_name,
    view_options, //
};

/// The outcome of one layout of a graph.
enum Outcome {
    /// The scene, with its iterations.
    Laid(u32),
    /// The view draws nothing.
    Empty,
    /// The layout crosses the cap of the named loop.
    Cap(&'static str),
    /// The core refuses the view, for the reason given.
    Refused(String),
}

impl Outcome {
    /// Returns whether the outcome stays inside the caps.
    fn inside(&self) -> bool {
        matches!(self, Outcome::Laid(_) | Outcome::Empty)
    }

    /// Returns the outcome as JSON.
    fn json(&self) -> JsValue {
        match self {
            Outcome::Laid(n) => JsValue::from(*n),
            Outcome::Empty => JsValue::from("empty"),
            Outcome::Cap(name) => JsValue::from(format!("cap {name}")),
            Outcome::Refused(reason) => JsValue::from(format!("refused {reason}")),
        }
    }
}

/// Answers the operation `random`.
pub(super) fn run(request: &JsObject) -> Result<JsObject, Stop> {
    for name in ["seed", "count"] {
        if !present(request, name) {
            return Err(fatal(request, &format!("the request carries no {name}")));
        }
    }
    let seed = number(request, "seed", 0, MAX_SAFE_INTEGER)?;
    let count = number(request, "count", 1, MAX_ITEMS as u64)?;
    let options = view_options(request)?;
    let limits = ReadLimits::DEFAULT;
    let mut rng = Rng::new(seed);
    let mut graphs = Vec::new();
    let (mut inside, mut most) = (0u64, 0u32);
    for _ in 0..count {
        let entry = match random_graph(&mut rng, &limits) {
            Ok(model) => {
                let schema = Schema::build(model, options.transforms.clone());
                let outcomes = lay_out(&schema, &options);
                if outcomes.iter().all(Outcome::inside) {
                    inside += 1;
                }
                for o in &outcomes {
                    if let Outcome::Laid(n) = o {
                        most = most.max(*n);
                    }
                }
                graph_json(&schema, &outcomes)
            }
            Err(e) => {
                let mut entry = JsObject::with_capacity(1);
                entry.insert("model", format!("refused {e:?}"));
                JsValue::Object(entry)
            }
        };
        graphs.push(entry);
    }
    let mut payload = JsObject::with_capacity(4);
    payload.insert("graphs", JsValue::Array(graphs));
    payload.insert("inside", inside);
    payload.insert("most", most);
    payload.insert("caps", caps());
    Ok(payload)
}

/// Returns the outcomes of the three layouts of the graph of `schema`.
fn lay_out(schema: &Schema, options: &Options) -> [Outcome; 3] {
    let clustering = Clustering::pick(schema, &[]);
    let model = &schema.model;
    let first = model.types().first().map_or(Route::Home, |&t| Route::Type {
        name: String::from(type_name(model, t)),
        field: None,
        arg: None,
    });
    [
        Route::Whole {
            root: WholeRoot::Query,
        },
        Route::Whole {
            root: WholeRoot::All,
        },
        first,
    ]
    .map(|route| outcome(schema, &clustering, &route, options))
}

/// Returns the outcome of the layout of the view of `route`.
fn outcome(schema: &Schema, clustering: &Clustering, route: &Route, options: &Options) -> Outcome {
    let view = match View::of(schema, clustering, route, options, &OPEN) {
        Ok(view) => view,
        Err(e) => return Outcome::Refused(format!("{e:?}")),
    };
    if view.empty.is_some() {
        return Outcome::Empty;
    }
    match scene(&view, schema, route) {
        Ok(scene) => Outcome::Laid(scene.iterations),
        Err(LayoutError::Empty) => Outcome::Empty,
        Err(LayoutError::CapExceeded { loop_name }) => Outcome::Cap(loop_name),
    }
}

/// Returns one graph of `schema` with the outcomes of its three layouts.
fn graph_json(schema: &Schema, outcomes: &[Outcome; 3]) -> JsValue {
    let model = &schema.model;
    let fields: usize = model
        .types()
        .iter()
        .map(|&t| model.ty(t).fields.len())
        .sum();
    let mut out = JsObject::with_capacity(5);
    out.insert("nodes", model.types().len() as u64);
    out.insert("fields", fields as u64);
    for (key, o) in ["query", "all", "ego"].into_iter().zip(outcomes) {
        out.insert(key, o.json());
    }
    JsValue::Object(out)
}

/// Returns the caps of the layout by name.
fn caps() -> JsValue {
    let mut out = JsObject::with_capacity(5);
    out.insert("steps", MAX_STEPS);
    out.insert("sweeps", SWEEPS);
    out.insert("overlap_passes", OVERLAP_PASSES);
    out.insert("wrap_passes", WRAP_PASSES);
    out.insert("route_iterations", ROUTE_ITERATIONS);
    JsValue::Object(out)
}
