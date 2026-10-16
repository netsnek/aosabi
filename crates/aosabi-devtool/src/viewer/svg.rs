// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool viewer svg: the operation svg, the drawing of the view of a
 * route in one theme with the iterations of its layout.
 *
 * The drawing is the file the generator writes for a diagram, laid out by the
 * layout of its kind and written by render_svg.
 */

//! The `svg` operation.
//!
//! The request `{input, route, routes, themes, budget, declared, svg_text,
//! title}` answers `drawings`, one entry per part of the view of each route
//! in each theme of `themes`, `light`, `dark` or both, `light` by default,
//! in the order of the routes, then the parts, then the themes. An entry
//! carries `route`, `part`, the number of the part when the view splits or
//! null, `theme`, `svg`, `iterations`, the steps of the layout counted
//! against `MAX_STEPS`, and the `width` and the `height` of the scene in
//! centipixels with its counts of `boxes`, `edges` and `chips`. A view that
//! draws nothing answers `svg` null and `empty`, and a route the core
//! refuses answers `refused` and `message` in its entry.
//!
//! The budget of a route is the one of the generator unless `budget` names
//! another: the whole graph under the budget of the poster and every other
//! route under the page budget with its parts. The title is the one the
//! generator gives the file of the diagram unless `title` names another, so
//! the drawing of a diagram the tree writes is the bytes of its file.

use std::fmt::Write as _;

use aosabi_json::{
    JsObject,
    JsValue, //
};
use aoviewer_core::{
    layout::LayoutError,
    render_svg::{
        SvgText,
        Theme,
        render_svg, //
    },
    route::{
        Route,
        WholeRoot, //
    },
    scene::Scene,
    schema::Schema,
    view::{
        EmptyReason,
        View, //
    }, //
};
use aoviewer_docs::error::DocsError;

use super::{
    Asked,
    Stop,
    declared,
    each_route,
    empty_json,
    fatal,
    layout_refused,
    load,
    optional,
    planned,
    present,
    refused_by,
    routes,
    scene,
    strings,
    view_refused,
    views, //
};

/// What every drawing of one request shares.
struct Style {
    /// The themes, in the order of the request.
    themes: Vec<Theme>,
    /// How the text is written.
    text: SvgText,
    /// The title the request gives every drawing, if any.
    title: Option<String>,
}

/// What the layout of one part answers.
enum Laid {
    /// The view draws nothing, for this reason.
    Empty(EmptyReason),
    /// The scene of the view.
    Drawn(Scene),
    /// The layout answers no scene.
    Refused(LayoutError),
}

/// Answers the operation `svg`.
pub(super) fn run(request: &JsObject) -> Result<JsObject, Stop> {
    let asked = routes(request)?;
    let style = style(request)?;
    if style.text == SvgText::Outlines {
        return Err(Stop::from(DocsError::SvgOutlines));
    }
    let asked = planned(request, asked)?;
    let declared = declared(request)?;
    let model = load(request)?.model;
    let drawings = each_route(model, declared, &asked, |schema, clustering, (a, plan)| {
        let mut out = Vec::new();
        match views(schema, clustering, a, *plan) {
            Ok(parts) => {
                for view in &parts {
                    draw(schema, a, view, &style, &mut out);
                }
            }
            Err(e) => {
                let mut entry = refused_by(&view_refused(e));
                entry.insert("route", a.hash.as_str());
                out.push(JsValue::Object(entry));
            }
        }
        out
    });
    let mut payload = JsObject::with_capacity(1);
    let drawings = drawings.into_iter().flatten().collect();
    payload.insert("drawings", JsValue::Array(drawings));
    Ok(payload)
}

/// Returns the themes, the text mode and the title of `request`.
///
/// # Errors
///
/// Returns [`Stop::Fatal`] for a theme or a text mode outside its words.
fn style(request: &JsObject) -> Result<Style, Stop> {
    let words = if present(request, "themes") {
        strings(request, "themes")?
    } else {
        vec!["light"]
    };
    let mut themes = Vec::with_capacity(words.len());
    for word in words {
        themes.push(match word {
            "light" => Theme::Light,
            "dark" => Theme::Dark,
            other => {
                let text = format!("themes takes light and dark, not {other:?}");
                return Err(fatal(request, &text));
            }
        });
    }
    let text = match optional(request, "svg_text")? {
        None | Some("text") => SvgText::Text,
        Some("outlines") => SvgText::Outlines,
        Some(other) => {
            let text = format!("svg_text takes text or outlines, not {other:?}");
            return Err(fatal(request, &text));
        }
    };
    let title = optional(request, "title")?.map(String::from);
    Ok(Style {
        themes,
        text,
        title,
    })
}

/// Appends the drawings of the part `view` of the route `asked` in every
/// theme of `style` to `out`.
fn draw(schema: &Schema, asked: &Asked, view: &View, style: &Style, out: &mut Vec<JsValue>) {
    let part = view.part.map(|(n, _)| n);
    let laid = match view.empty {
        Some(reason) => Laid::Empty(reason),
        None => match scene(view, schema, &asked.route) {
            Ok(scene) => Laid::Drawn(scene),
            Err(e) => Laid::Refused(e),
        },
    };
    for &theme in &style.themes {
        let mut entry = JsObject::with_capacity(10);
        entry.insert("route", asked.hash.as_str());
        entry.insert(
            "part",
            part.map_or(JsValue::Null, |n| JsValue::from(u32::from(n))),
        );
        entry.insert("theme", theme_word(theme));
        match &laid {
            Laid::Empty(reason) => {
                entry.insert("svg", JsValue::Null);
                entry.insert("empty", empty_json(*reason));
            }
            Laid::Drawn(scene) => {
                let title = style
                    .title
                    .clone()
                    .unwrap_or_else(|| diagram_title(&asked.route, part));
                drawing(schema, scene, theme, style.text, &title, &mut entry);
            }
            Laid::Refused(e) => merge(&mut entry, &refused_by(&layout_refused(*e))),
        }
        out.push(JsValue::Object(entry));
    }
}

/// Writes the drawing of `scene` in `theme` under `title` and its counts
/// into `entry`, or the refusal of its text mode.
fn drawing(
    schema: &Schema,
    scene: &Scene,
    theme: Theme,
    text: SvgText,
    title: &str,
    entry: &mut JsObject,
) {
    let mut svg = String::new();
    if render_svg(scene, schema, theme, text, title, &mut svg).is_err() {
        merge(entry, &refused_by(&DocsError::SvgOutlines));
        return;
    }
    entry.insert("svg", svg);
    entry.insert("iterations", scene.iterations);
    entry.insert("width", scene.width);
    entry.insert("height", scene.height);
    entry.insert("boxes", scene.boxes.len() as u64);
    entry.insert("edges", scene.edges.len() as u64);
    entry.insert("chips", scene.chips.len() as u64);
}

/// Copies every field of `from` into `entry`.
fn merge(entry: &mut JsObject, from: &JsObject) {
    for (key, value) in from {
        entry.insert(key.clone(), value.clone());
    }
}

/// Returns the word of a theme, the one the file names of the tree carry.
fn theme_word(theme: Theme) -> &'static str {
    match theme {
        Theme::Light => "light",
        Theme::Dark => "dark",
    }
}

/// Returns the title the generator gives the file of the diagram of `route`
/// and `part`.
///
/// The rule is the one of `diagram_title` in
/// crates/aoviewer-core/src/tree/mod.rs, which the core keeps private, so the
/// seam writes it again until a seam fix makes it public.
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
