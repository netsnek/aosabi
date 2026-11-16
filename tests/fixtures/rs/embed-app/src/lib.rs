// SPDX-License-Identifier: MIT
/*
 * embed-app: the app the embedding host of tests/notebooks/80_embed.ipynb boots.
 */

//! The app the embedding host of `80_embed` boots.
//!
//! A drawer of shapes lives in memory. The query answers a greeting, the
//! shapes of the drawer after a timer, one shape by its place and the label
//! of the drawer, whose store is out of reach, and the mutation adds a
//! square. A shape is a union of a circle and a square, so the build writes
//! the type resolution of the union, which an embedding host takes into its
//! own program beside the schema. The query also answers the locale the
//! `@inContext` of its operation names and the count of a list it is handed,
//! so a long document that parses and validates reads its locale. The variant
//! of the harness that `AOSABI_CONFIG` names sets the cap on the tokens of a
//! request document, `tokens_low` or `tokens_high`, or decides whether the app
//! masks an unexpected error, `masked_errors_true`, `masked_errors_function`
//! sets a mask function of the app's own, which the embed road refuses at the
//! boot, and `introspection_false` turns introspection off.

use std::sync::{LazyLock, Mutex, MutexGuard, PoisonError};

use aosabi::{get_locale, prelude::*, time::sleep};

/// A circle of the drawer.
#[derive(Clone)]
pub struct Circle {
    /// The radius of the circle.
    pub radius: f64,
}

/// A square of the drawer.
#[derive(Clone)]
pub struct Square {
    /// The length of a side of the square.
    pub side: f64,
}

/// A shape of the drawer, a circle or a square.
#[derive(Clone)]
pub enum Shape {
    /// A circle.
    Circle(Circle),
    /// A square.
    Square(Square),
}

/// The shapes of the drawer, a circle and a square when the app boots.
static DRAWER: LazyLock<Mutex<Vec<Shape>>> = LazyLock::new(|| {
    Mutex::new(vec![
        Shape::Circle(Circle { radius: 1.5 }),
        Shape::Square(Square { side: 2.0 }),
    ])
});

/// Returns the drawer, locked for the caller.
fn drawer() -> MutexGuard<'static, Vec<Shape>> {
    DRAWER.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The queries of the drawer.
pub struct Query;

impl Query {
    /// Greets the caller by name, or the world when no name is given.
    /// @param name The name to greet.
    pub fn hello(&self, name: Option<String>) -> String {
        format!("Hello, {}", name.unwrap_or_else(|| String::from("world")))
    }

    /// Lists the shapes of the drawer.
    pub async fn shapes(&self) -> Vec<Shape> {
        // One timer of the request stands for a store that answers later.
        let _ = sleep(1).await;
        drawer().clone()
    }

    /// Returns the shape at a place of the drawer, counted from 0.
    /// @param index The place of the shape.
    pub fn shape(&self, index: i32) -> Result<Shape> {
        let found = usize::try_from(index)
            .ok()
            .and_then(|index| drawer().get(index).cloned());
        let Some(shape) = found else {
            return Err(ServiceError::msg("The drawer holds no shape at this place")
                .code("BAD_USER_INPUT")
                .details(json::Map::from([("index", json::Value::from(index))]))
                .into());
        };
        Ok(shape)
    }

    /// Answers the locale the `@inContext` of the operation names, or
    /// `(none)` where it names none.
    pub fn locale(&self) -> String {
        get_locale().unwrap_or_else(|| String::from("(none)"))
    }

    /// Counts the items of a list.
    /// @param items The items to count.
    pub fn count(&self, items: Vec<i32>) -> i32 {
        i32::try_from(items.len()).unwrap_or(i32::MAX)
    }

    /// Returns the label of the drawer, whose store is out of reach, so the
    /// resolver fails with an error that is no GraphQL error.
    pub fn label(&self) -> Result<String> {
        Err(Error::msg("The store of labels is out of reach"))
    }
}

/// The changes to the drawer.
pub struct Mutation;

impl Mutation {
    /// Adds a square to the drawer and returns the number of its shapes.
    /// @param side The length of a side of the square.
    pub fn add_square(&self, side: f64) -> i32 {
        let mut shapes = drawer();
        shapes.push(Shape::Square(Square { side }));
        i32::try_from(shapes.len()).unwrap_or(i32::MAX)
    }
}

/// Returns the app with its two roots under the variant `AOSABI_CONFIG`
/// names. `default` sets no key, so every request document parses under the
/// cap of 120,008 tokens of an app that sets none, an unexpected error
/// answers its own message and introspection answers, `tokens_low` and
/// `tokens_high` set the key `maxTokens` of Aosabi to 1,000 and to 240,016,
/// below and above that cap, `masked_errors_true` turns the mask of an
/// unexpected error on, `masked_errors_function` sets a mask function of the
/// app's own, and `introspection_false` turns introspection off. The cap,
/// the mask and the refusal of introspection are named divergences from the
/// reference. A name of no variant fails the boot.
pub fn app() -> Result<impl IntoApp, String> {
    let variant = std::env::var("AOSABI_CONFIG").unwrap_or_else(|_| String::from("default"));
    let app = App::new().query(Query).mutation(Mutation);
    let app = match variant.as_str() {
        "default" => app,
        "tokens_low" => app.max_tokens(1000),
        "tokens_high" => app.max_tokens(240_016),
        // The mask an app that hardens turns on, a named divergence.
        "masked_errors_true" => app.masked_errors(aosabi::MaskedErrors::On),
        // A mask function of the app's own, which the embed road does not take.
        "masked_errors_function" => {
            app.masked_errors(aosabi::MaskedErrors::Options(aosabi::MaskedErrorsConfig {
                mask_error: Some(std::rc::Rc::new(|error, _, _| error)),
                ..aosabi::MaskedErrorsConfig::default()
            }))
        }
        // The refusal of introspection, a named divergence.
        "introspection_false" => app.introspection(false),
        name => {
            return Err(format!(
                "AOSABI_CONFIG names no variant of this app: {name}. The six names are \
                 default, tokens_low, tokens_high, masked_errors_true, masked_errors_function, \
                 introspection_false."
            ));
        }
    };
    // The serving plugin binds a socket, which only the cards that serve
    // sockets link, so the build for the Worker leaves it out.
    #[cfg(unix)]
    let app = app.plugin(use_node_server());
    Ok(app)
}
