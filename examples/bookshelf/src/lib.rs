// SPDX-License-Identifier: MIT

//! A small bookshelf, a service of the native Rust layer of Aosabi.
//!
//! The shelf lives in memory. Two queries read it, a mutation adds a book, a
//! route beside `/graphql` answers the health of the service, and a middleware
//! stamps the size of the shelf on every answer.

use std::sync::{LazyLock, Mutex, MutexGuard, PoisonError};

use aosabi::{prelude::*, time::sleep};

/// The genre of a book.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Genre {
    /// Novels and stories.
    Fiction,
    /// Books about the laws of nature.
    Science,
    /// Poems and verse.
    Poetry,
}

/// A book on the shelf.
#[derive(Clone)]
pub struct Book {
    /// The id of the book on this shelf.
    pub id: ID,
    /// The title of the book.
    pub title: String,
    /// The author of the book.
    pub author: String,
    /// The year the book first appeared.
    pub year: i32,
    /// The genre of the book.
    pub genre: Genre,
}

impl Book {
    /// The title and the author in one line, as a catalogue prints them.
    pub fn label(&self) -> String {
        format!("{} by {}", self.title, self.author)
    }
}

/// A book a client adds to the shelf.
pub struct NewBook {
    /// The title of the book.
    pub title: String,
    /// The author of the book.
    pub author: String,
    /// The year the book first appeared.
    pub year: i32,
    /// The genre of the book.
    pub genre: Genre,
}

/// The books on the shelf, three of them when the service starts.
static SHELF: LazyLock<Mutex<Vec<Book>>> = LazyLock::new(|| {
    Mutex::new(vec![
        book("1", "Moby-Dick", "Herman Melville", 1851, Genre::Fiction),
        book("2", "Opticks", "Isaac Newton", 1704, Genre::Science),
        book("3", "Leaves of Grass", "Walt Whitman", 1855, Genre::Poetry),
    ])
});

/// Returns a book of the first shelf.
fn book(id: &str, title: &str, author: &str, year: i32, genre: Genre) -> Book {
    Book {
        id: ID::from(id),
        title: String::from(title),
        author: String::from(author),
        year,
        genre,
    }
}

/// Returns the shelf, locked for the caller.
fn shelf() -> MutexGuard<'static, Vec<Book>> {
    SHELF.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The queries of the bookshelf.
pub struct Query;

impl Query {
    /// Returns one book, or null when no book on the shelf has the id.
    /// @param id The id of the book.
    pub fn book(&self, id: ID) -> Option<Book> {
        shelf().iter().find(|book| book.id == id).cloned()
    }

    /// Lists the books on the shelf, of one genre when a genre is given.
    pub async fn books(&self, genre: Option<Genre>) -> Vec<Book> {
        // One timer of the request stands for a store that answers later.
        let _ = sleep(1).await;
        shelf()
            .iter()
            .filter(|book| genre.is_none_or(|genre| book.genre == genre))
            .cloned()
            .collect()
    }
}

/// The changes to the bookshelf.
pub struct Mutation;

impl Mutation {
    /// Adds a book to the shelf and returns it with its new id.
    pub fn add_book(&self, input: NewBook) -> Result<Book> {
        if input.title.trim().is_empty() {
            return Err(ServiceError::msg("A book needs a title")
                .code("BAD_USER_INPUT")
                .status(400)
                .details(json::Map::from([("field", "title")]))
                .into());
        }
        let mut books = shelf();
        let book = Book {
            id: ID::from((books.len() + 1).to_string()),
            title: input.title,
            author: input.author,
            year: input.year,
            genre: input.genre,
        };
        books.push(book.clone());
        Ok(book)
    }
}

/// Returns the app, with its roots, middleware, route and plugin on one chain.
pub fn app() -> impl IntoApp {
    let app = App::new()
        .query(Query)
        .mutation(Mutation)
        // Stamps the size of the shelf on every answer, after the handler.
        .middleware("*", |c, next| async move {
            next.run(c.clone()).await?;
            c.header("x-shelf-size", &shelf().len().to_string());
            Ok(c.res())
        })
        // A route beside `/graphql`, which answers the health of the service.
        .get("/health", |c| async move {
            c.json(&json::Value::from(json::Map::from([
                ("status", json::Value::from("ok")),
                ("books", json::Value::from(shelf().len())),
            ])))
        });
    // The serving plugin binds a socket, which only the cards that serve
    // sockets link, so the build for the Worker leaves it out.
    #[cfg(unix)]
    let app = app.plugin(use_node_server());
    app
}
