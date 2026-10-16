// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool viewer bench: the operation bench, the time of the calls of
 * the core in nanoseconds and the peak resident set of the process.
 *
 * It holds the one clock of the primitive, since the core reads none and every
 * time gate of the notebooks is a 95th percentile over runs of this operation.
 */

//! The `bench` operation.
//!
//! The request `{input, routes, options, repetitions, budget, declared}` loads
//! the input once and answers `load_ns`, the time of the read into the model,
//! `build_ns`, the time of the schema and its clustering under the transforms
//! of the first route, and `routes`, one entry per route with its `route`,
//! `ns`, the time of each of `repetitions` calls, 1 to [`MAX_ITEMS`] of them
//! and 1 by default, and the `parts` the last call drew, each with its
//! `iterations`, its `width` and `height` in centipixels and its counts of
//! `boxes`, `edges` and `chips`, or `empty`. A call is the view of the route
//! under its budget, the parts of the page budget or one view of the whole
//! graph, and the layout of every part, which is what the generator does for
//! the diagrams of a route and the viewer for one route. The parts of the last
//! call turn into JSON after its clock stops, so `ns` times no JSON. `routes`
//! is a list of hashes or the word `types`, the route of every node of the
//! graph in schema order under the option string `options`, which step 14 of
//! `51_viewer_core` times. The routes run grouped by their transforms, so the
//! schema of each set is built once, and answer in the order of the request. A
//! refused input answers its refusal beside `load_ns`.
//!
//! `peak_rss_kib` is the peak resident set of the process in KiB, the line
//! `VmHWM` of `/proc/self/status`, read after the last call, or null where
//! the file does not exist. The process reads the request file and holds the
//! answers of the earlier lines, so a notebook that judges the memory of one
//! input sends it alone.

use std::{
    fs,
    num::NonZeroU64,
    time::Instant, //
};

use aosabi_json::{
    JsObject,
    JsValue, //
};
use aoviewer_core::{
    cluster::Clustering,
    layout::LayoutError,
    route::{
        Options,
        Route, //
    },
    scene::Scene,
    schema::Schema,
    view::{
        EmptyReason,
        View,
        ViewError, //
    }, //
};

use super::{
    Asked,
    MAX_ITEMS,
    Plan,
    Schemas,
    Stop,
    declared,
    empty_json,
    fatal,
    layout_refused,
    load,
    node_name,
    number,
    plan,
    planned,
    refused_by,
    route,
    scene,
    strings,
    view_options,
    view_refused,
    views, //
};

/// Answers the operation `bench`.
pub(super) fn run(request: &JsObject) -> Result<JsObject, Stop> {
    let repetitions = number(request, "repetitions", 1, MAX_ITEMS as u64)?;
    let repetitions = NonZeroU64::new(repetitions).ok_or_else(|| {
        let text = format!("repetitions is no whole number from 1 to {MAX_ITEMS}");
        fatal(request, &text)
    })?;
    let declared = declared(request)?;
    let typed =
        matches!(request.get("routes"), Some(JsValue::String(w)) if w.as_str() == Some("types"));
    let (asked, typed) = if typed {
        plan(request, &Route::Home)?;
        (Vec::new(), Some(view_options(request)?))
    } else {
        let hashes = strings(request, "routes")?;
        let asked = hashes
            .into_iter()
            .map(|hash| route(request, hash))
            .collect::<Result<Vec<_>, _>>()?;
        (asked, None)
    };
    let asked = planned(request, asked)?;
    let mut payload = JsObject::with_capacity(5);
    let start = Instant::now();
    let loaded = load(request);
    payload.insert("load_ns", nanos(start));
    let loaded = match loaded {
        Ok(loaded) => loaded,
        Err(Stop::Refused { code, message }) => {
            payload.insert("refused", code);
            payload.insert("message", message);
            payload.insert("peak_rss_kib", peak_rss_kib());
            return Ok(payload);
        }
        Err(stop) => return Err(stop),
    };
    let first = match &typed {
        Some(options) => Some(&options.transforms),
        None => asked.first().map(|(a, _)| &a.options.transforms),
    };
    let start = Instant::now();
    let schemas = first.map(|transforms| Schemas::new(loaded.model, transforms, declared));
    payload.insert("build_ns", nanos(start));
    let out = match schemas {
        Some(schemas) => {
            let asked = match &typed {
                Some(options) => planned(request, every_type(schemas.schema(), options))?,
                None => asked,
            };
            schemas.each(&asked, |schema, clustering, (a, plan)| {
                time(schema, clustering, a, *plan, repetitions)
            })
        }
        None => Vec::new(),
    };
    payload.insert("routes", JsValue::Array(out));
    payload.insert("peak_rss_kib", peak_rss_kib());
    Ok(payload)
}

/// What one part of the view of a route draws.
enum Drawn {
    /// The view of the part is empty, for this reason.
    Empty(EmptyReason),
    /// The scene of the layout of the part, or the refusal of the layout.
    Laid(Result<Scene, LayoutError>),
}

/// What one call draws: every part of the view of a route with what it
/// draws. The views stay beside their scenes, so the clock of a call stops
/// before any of its answer is dropped.
type Parts = Vec<(View, Drawn)>;

/// Returns the entry of the route `asked` under `plan`, timed over
/// `repetitions` calls.
///
/// The clock of a call covers the view and the layouts of [`draw`] alone.
/// The answer of the last call turns into JSON after its clock stops.
fn time(
    schema: &Schema,
    clustering: &Clustering,
    asked: &Asked,
    plan: Plan,
    repetitions: NonZeroU64,
) -> JsValue {
    let (first, mut last) = once(schema, clustering, asked, plan);
    let mut ns = vec![JsValue::from(first)];
    for _ in 1..repetitions.get() {
        let (n, drawn) = once(schema, clustering, asked, plan);
        ns.push(JsValue::from(n));
        last = drawn;
    }
    let mut entry = match last {
        Err(e) => refused_by(&view_refused(e)),
        Ok(parts) => {
            let mut entry = JsObject::with_capacity(3);
            entry.insert("parts", JsValue::Array(parts_json(parts)));
            entry
        }
    };
    entry.insert("route", asked.hash.as_str());
    entry.insert("ns", JsValue::Array(ns));
    JsValue::Object(entry)
}

/// Returns the route of every node of the graph of `schema` in schema
/// order, under `options`.
fn every_type(schema: &Schema, options: &Options) -> Vec<Asked> {
    let mut out = Vec::with_capacity(schema.graph.nodes.len());
    for n in schema.graph.node_ids() {
        let route = Route::Type {
            name: String::from(node_name(schema, n)),
            field: None,
            arg: None,
        };
        out.push(Asked {
            hash: route.to_hash(options),
            route,
            options: options.clone(),
        });
    }
    out
}

/// Draws the route `asked` under `plan` once and returns the nanoseconds
/// of the call beside what it drew, which outlives the clock.
fn once(
    schema: &Schema,
    clustering: &Clustering,
    asked: &Asked,
    plan: Plan,
) -> (u64, Result<Parts, ViewError>) {
    let start = Instant::now();
    let drawn = draw(schema, clustering, asked, plan);
    (nanos(start), drawn)
}

/// Draws the route `asked` under `plan`: the view of the route under its
/// budget and the layout of every part that draws something.
fn draw(
    schema: &Schema,
    clustering: &Clustering,
    asked: &Asked,
    plan: Plan,
) -> Result<Parts, ViewError> {
    let parts = views(schema, clustering, asked, plan)?;
    Ok(parts
        .into_iter()
        .map(|view| {
            let drawn = match view.empty {
                Some(reason) => Drawn::Empty(reason),
                None => Drawn::Laid(scene(&view, schema, &asked.route)),
            };
            (view, drawn)
        })
        .collect())
}

/// Returns what each part of `parts` drew as JSON.
fn parts_json(parts: Parts) -> Vec<JsValue> {
    parts
        .into_iter()
        .map(|(_, drawn)| {
            let entry = match drawn {
                Drawn::Empty(reason) => {
                    let mut entry = JsObject::with_capacity(1);
                    entry.insert("empty", empty_json(reason));
                    entry
                }
                Drawn::Laid(Ok(scene)) => {
                    let mut entry = JsObject::with_capacity(6);
                    entry.insert("iterations", scene.iterations);
                    entry.insert("width", scene.width);
                    entry.insert("height", scene.height);
                    entry.insert("boxes", scene.boxes.len() as u64);
                    entry.insert("edges", scene.edges.len() as u64);
                    entry.insert("chips", scene.chips.len() as u64);
                    entry
                }
                Drawn::Laid(Err(e)) => refused_by(&layout_refused(e)),
            };
            JsValue::Object(entry)
        })
        .collect()
}

/// Returns the nanoseconds since `start`, saturated at `u64::MAX`.
fn nanos(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

/// Returns the peak resident set of the process in KiB, the line `VmHWM` of
/// `/proc/self/status`, or null where the file does not exist or holds no such
/// line.
fn peak_rss_kib() -> JsValue {
    let Ok(status) = fs::read_to_string("/proc/self/status") else {
        return JsValue::Null;
    };
    status
        .lines()
        .find_map(|line| line.strip_prefix("VmHWM:"))
        .and_then(|rest| rest.trim().strip_suffix("kB"))
        .and_then(|kib| kib.trim().parse::<u64>().ok())
        .map_or(JsValue::Null, JsValue::from)
}
