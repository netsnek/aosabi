// SPDX-License-Identifier: MIT
/*
 * embed-app: the app the embedding host of tests/notebooks/07_embed.ipynb boots.
 */

//! The app the embedding host of `07_embed` boots.
//!
//! A drawer of shapes lives in memory. The query answers a greeting, the
//! shapes of the drawer after a timer and one shape by its place, and the
//! mutation adds a square. A shape is a union of a circle and a square, so
//! the build writes the type resolution of the union, which an embedding
//! host takes into its own program beside the schema.

use std::sync::{LazyLock, Mutex, MutexGuard, PoisonError};

use aosabi::{prelude::*, time::sleep};

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

/// Returns the app with its two roots.
pub fn app() -> impl IntoApp {
    let app = App::new().query(Query).mutation(Mutation);
    // The serving plugin binds a socket, which only the cards that serve
    // sockets link, so the build for the Worker leaves it out.
    #[cfg(unix)]
    let app = app.plugin(use_node_server());
    app
}
