# The Rust layer

A Rust service is plain Rust. Its source holds no macro, no derive and no attribute of Aosabi, its
package holds no build script and no configuration file of Aosabi, and the build reads the schema
from the types, the `impl` blocks and the doc comments as they stand. This guide shows the files of
a service, the index that registers it, the forms each GraphQL construct takes, and how a service is
built and served. The complete services under [examples/](../../examples/) build as they stand.

## 1. The files of a service

| Path | What it holds |
| --- | --- |
| `Cargo.toml` | The facade, and a second table of it for a Unix target |
| `src/lib.rs` | The index, the crate root by cargo's default |
| `src/` | Any modules the index declares |
| `target/aosabi/` | What the build writes: `schema.graphql` and the server package |

The command `aosabi new <dir>` writes such a project. Its manifest names the facade `aosabi` once
under `[dependencies]`, and a second time under `[target.'cfg(unix)'.dependencies]` with the
features of the cards that serve sockets, so a build for a Worker links no server. A service names
no other crate of Aosabi.

## 2. The index and its entry

The index is the crate root, the one place where a service looks like a framework. It registers the
roots, the routes, the plugins and the options on one chain, and its entry is the one public
function of the crate root whose return type is an app.

```rust
use aosabi::prelude::*;

/// A post of the blog.
pub struct Post {
    pub id: ID,
    pub title: String,
}

pub struct Query;

impl Query {
    /// Returns the post with `id`, or null where there is none.
    pub fn post(&self, _id: ID) -> Option<Post> {
        None
    }
}

pub struct Mutation;

impl Mutation {
    pub fn create_post(&self, title: String) -> Post {
        Post { id: ID::from("1"), title }
    }
}

pub fn app() -> impl IntoApp {
    App::new().query(Query).mutation(Mutation)
}
```

The builder reads the entry and never calls it. It follows the `mod` declarations and the `use`
paths from the crate root, reads the types the calls `query`, `mutation`, `subscription` and
`compose` register, and walks their public fields and the public `&self` methods of their inherent
`impl` blocks to the types, fields and arguments of the schema. Every other call of the chain, a
route, a middleware, a plugin or an option, runs when the app boots, and the builder reads nothing
of it.

A root is a value of a struct of the crate. A second part of a root follows the first, as
`.query(Query).query(billing::Billing)`, and `.compose(admin())` mounts the app a function of the
crate answers, its roots after those of the parent. An entry may answer a `Result`, which ends the
boot with its error.

What the source does not state, the builder does not guess. It refuses at the file and the line,
with what to write in its place:

- A root chosen on a branch at run time, since the builder never runs the entry.
- A second public function of the crate root that answers an app.
- A type of a crate from a registry, a trait object, an associated type or a type a macro makes,
  since their fields are not in the source the builder reads.

## 3. Types

| Rust | GraphQL |
| --- | --- |
| A struct with public fields | An object type, or an input type where an argument takes it |
| `Option<T>` | A nullable `T`, and every other type is non null |
| `Vec<T>`, a slice or an array | A list |
| An enum of unit variants | An enum |
| An enum whose variants each hold one type | A union |
| `async fn` | A field that resolves later |
| `Result<T>` | A field that may answer an error |
| A method with arguments | A field with arguments, in the order of the parameters |
| A field without `pub` | No field |

A Rust number takes the GraphQL type of its range: `i8`, `i16`, `i32`, `u8` and `u16` are `Int`,
`f32` and `f64` are `Float`, and `i64`, `u32`, `u64`, `i128`, `u128`, `isize` and `usize` are
`Number`. The markers of the prelude name a scalar directly: `ID`, `Int`, `Float`, `Number`, `Date`,
`File`, `JSON` and `Any`. A JSON value is built with `json::Map` and `json::Value` of the prelude,
whose writer keeps the key order of JavaScript:

```rust
let payload = json::Map::from([
    ("id", json::Value::from("7")),
    ("open", json::Value::from(true)),
]);
```

Where the variants of a union enum share fields, the union prints as an interface of the shared
fields, which every variant's type implements. An optional argument is an `Option` with its default
taken in the body, as `limit.unwrap_or(10)`, since a default does not reach the schema.

## 4. Names

A field, a method and an argument written in snake case reach the schema in lower camel case, so
`create_post` is `createPost`. A variant written in upper camel case reaches it in screaming snake
case, so `Done` is `DONE`. A type keeps its name, and one leading underscore of a parameter is
dropped, so an unused parameter keeps the compiler quiet. Any other spelling stays as written, so a
field that has to read `altText` is written `altText` under the compiler's own lint allowance.

An input type is named after the field and the argument that take it, whatever the struct is called,
so `create_post(&self, input: NewPost)` prints `createPost(input: CreatePostInput!)`. The name of
the struct reaches no artifact, so a service names it as Rust names things.

## 5. Comments

A doc comment on a type, a field, a method or a variant is its description in the schema. A
parameter takes no doc comment in Rust, so the description of an argument is an `@param` line of its
method's doc comment:

```rust
impl Query {
    /// Returns the newest posts.
    ///
    /// @param limit The most posts to return.
    pub fn posts(&self, limit: Option<Int>) -> Vec<Post> {
        Vec::new()
    }
}
```

A `//` comment never reaches the schema, so a note for Rust readers alone stays a line comment.

## 6. Errors

A resolver answers `Result<T>` of the prelude and returns early with `?`. The error a resolver
throws on purpose is `ServiceError`, whose message, code, status and details reach the client:

```rust
impl Query {
    pub async fn author(&self, id: String) -> Result<Author> {
        let Some(author) = Author::find(&id).await? else {
            return Err(ServiceError::msg("Author not found")
                .code("AUTHOR_NOT_FOUND")
                .status(404)
                .details(json::Map::from([("id", id)]))
                .into());
        };
        Ok(author)
    }
}
```

A mutation that answers a payload with user errors returns `MutationPayload<T>`, which makes every
field of `T` nullable in the schema and adds `userErrors` after them.

## 7. The context and the environment

The context of a request is ambient, as `get_context()?` reads it in any resolver. It holds the
request with its headers and the values a middleware set under a typed `Key`:

```rust
const REQUEST_ID: Key<String> = Key::new("requestId");

impl Query {
    pub fn who_am_i(&self) -> Result<String> {
        let c = get_context()?;
        Ok(c.req().header("authorization").unwrap_or_else(|| String::from("anonymous")))
    }

    pub fn request_id(&self) -> Result<Option<String>> {
        Ok(get_context()?.get(REQUEST_ID))
    }

    pub fn region(&self) -> String {
        get_env().var("REGION").unwrap_or_else(|| String::from("local"))
    }
}
```

An outbound HTTP call is `aosabi::fetch` under the feature `fetch` of the facade. On a native card
it goes through the client of the project over TLS 1.3, and on a target with no network it answers
an error, so a service never opens a socket of its own.

## 8. Permissions

A resolver checks permissions in its body, where the check is seen, with the calls of
`aosabi::auth`:

```rust
use aosabi::auth::{authorize, has_permission, require_role};

impl Mutation {
    pub fn publish_post(&self, id: String) -> Result<Post> {
        require_role(&["editor", "admin"])?;
        publish(id)
    }

    pub fn delete_account(&self, id: String) -> Result<bool> {
        authorize(|p| p.is_some_and(|p| p.id == id) || has_permission(p, &["accounts:delete"]))?;
        remove(id)
    }
}
```

The principal comes from the identity plugin `use_identity`, which takes a provider function of the
service. A check that several resolvers share is a function they call first in their body. A whole
app takes a gate, as `.gate(|| authorize(|p| has_role(p, &["billing"])))`, which guards the fields
of its own roots.

## 9. Routes and middleware

An app answers routes beside its schema, and a middleware runs around them:

```rust
use aosabi::{auth::ForbiddenError, prelude::*};

fn vault() -> App {
    App::new()
        .base_path("/vault")
        .middleware("/files/*", |c, next| async move {
            if c.req().header("x-key").as_deref() != Some("secret") {
                Err(ForbiddenError::new())?;
            }
            next.run(c).await
        })
        .get("/ping", |c| async move { c.text("vault-pong") })
}
```

The methods are `get`, `post`, `put`, `delete`, `patch`, `options`, `all`, `on`, `route`,
`not_found` and `on_error`, and `.middleware(path, mw)` registers a middleware, since `use` is a
keyword of Rust. A middleware answers a response of its own, which ends the request, or what
`next.run(c)` answers. `ForbiddenError` is the error of `aosabi::auth` that answers 403.

## 10. Plugins and options

A plugin is one call of the chain, in the order a service lists them:

```rust
pub fn app() -> impl IntoApp {
    let app = App::new().query(Query).plugin(use_identity(header_auth));
    // The serving plugin binds a socket, which only the cards that serve sockets link.
    #[cfg(unix)]
    let app = app.plugin(use_node_server());
    app
}
```

A plugin of the service's own is built with `PluginObject::new(name)` and one call per member, as
`.setup(..)` and `.middleware(..)`, and a hook of an operation such as `.on_execute(..)`. Every
option of the app is one call of the chain named as the option in snake case, so a service holds no
configuration file.

## 11. Subscriptions

A field of `Subscription` that answers a stream is a subscription. A service keeps its pub sub in a
`static` and publishes from any code, a resolver, a timer or a driver:

```rust
static PUB_SUB: PubSub = PubSub::new();

const CHANGED: Topic<(), json::Map> = Topic::new("changed");

impl Mutation {
    pub fn touch(&self) -> bool {
        PUB_SUB.publish(CHANGED, (), json::Map::from([("by", "touch")]));
        true
    }
}

impl Subscription {
    pub fn changes(&self) -> impl Stream<Item = json::Map> {
        PUB_SUB.subscribe(CHANGED, ())
    }
}
```

`stream::task(|push, stop| async move { .. })` writes a stream by hand, pushing each value and
stopping it at its end. Over HTTP a subscription answers as server sent events, and over GraphTP as
batches of events that the client pulls.

## 12. Logging and uploads

The line `static LOG: Logger = logger("billing");` gives a module its logger, and
`LOG.info(message, fields)` writes one line. A field whose argument is a `File` takes an upload of
the GraphQL multipart request form once the app turns uploads on with `.multipart(true)`, and the
file gives its name, its type and its bytes.

## 13. Building and serving

| Command | What it does |
| --- | --- |
| `cargo aosabi build` | Reads the source, writes the schema and builds the server |
| `cargo aosabi build --release` | The same in the release profile, with a stripped copy |
| `cargo aosabi build --device docker` | Builds for another card, here a container image |
| `cargo aosabi dev` | Builds, serves on port 3000 or `PORT`, and builds again on every change |
| `aosabi inspect` | Prints the model of the app as JSON, or its schema with `--sdl` |

The build writes the glue between the types and the executor into a generated copy of the crate
under `target/aosabi/`, which links every module file of the copy to the author's own file, so the
author's files stay as they are and a compiler error names the author's file and line. The lint
tables of the package hold in the copy, so a package that forbids `unsafe_code` keeps that rule in
the glue as well.

The types of a remote schema, which a delegation to another GraphQL service reads, are code a
command writes: `aosabi pull <url>` writes them as a registry of the remote API.
