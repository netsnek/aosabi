# Embedding the engine in a program

A card builds a program that serves sockets: it boots the app, listens on its carriers and answers
every request with the routes, the middleware and the plugins of the app around the schema. A host
that already is a program of its own, and has no socket to read a request from, links the engine
and the roots of the app into itself instead. Such a host is a module of WebAssembly that a Web
Worker of a page runs, a desktop program, or a tool that answers requests on its standard input. It
boots the roots once and hands each GraphQL request to the engine as the JSON body a GraphQL request
over HTTP carries, and the engine answers the response body. This guide shows when to embed, the two
calls, the build of a host, what a request sees, what takes no part, the bodies that are no request
and a complete host.

## 1. When to embed and when to serve

| The host needs | It takes |
| --- | --- |
| A socket, HTTP, routes, middleware or plugins | A card, as in section 13 of [rust.md](rust.md) |
| Subscriptions over server sent events or GraphTP | A card |
| GraphQL inside a program with a loop of its own | The embedding of this guide |
| GraphQL inside a module of WebAssembly for a page | The embedding, with the card `worker` |

An embedded engine needs no socket, no HTTP, no clock and no thread, so a host builds for every
target the library of the app builds for, `wasm32-unknown-unknown` among them. There it needs one
thing of the host, a source of random bytes, which section 3 installs. The app itself stays as
[rust.md](rust.md) writes it, and the same source serves through a card and answers inside a host.

## 2. The calls of a host

The module `aosabi::embed` of the facade holds the embedding. Its type `Embedded` is the engine with
the roots of one app.

| Call | What it does |
| --- | --- |
| `Embedded::boot(entry, output)` | Boots the roots of the app of `entry` over the schema `output` |
| `engine.answer(body)` | Answers the JSON body of one request with the JSON text of the response |
| `engine.schema()` | Returns the schema text the engine answers with |
| `install_randomness(provider)` | Installs the source of random bytes, see section 3 |

The entry is the function `__aosabi::entry` of the generated copy of the app, which the build writes
around the index of the app. The output is an `EmbedOutput`, the schema text and the type resolution
of every union and interface, and the macro `aosabi::embed_output!()` includes both from the
`OUT_DIR` of the host. A boot fails with the text of the error the entry answered, of a schema
that does not build over the roots of the app, or of a key `maskedErrors` that names a mask function
of its own, `mask_error`, which the embedding does not run, and a host prints it and stops.

```rust
let engine = aosabi::embed::Embedded::boot(my_app::__aosabi::entry, aosabi::embed_output!())?;
let body = engine.answer(r#"{"query":"{ hello }"}"#);
```

A host boots the engine once and keeps it for its whole run, so the state of the app, such as a
`static` that a mutation changes, lives as long as the program.

## 3. The build of a host

The host builds in two steps, the app first and the host after it:

1. The command `cargo aosabi build --device worker` reads the source of the app and writes the
   generated copy of the crate under `target/aosabi/app` of the app, beside the schema
   `schema.graphql`, the type resolution `aosabi_resolve_types.rs` and the package name
   `aosabi_name.txt`. The card `worker` builds the library of the copy for `wasm32-unknown-unknown`
   with no server around it. The card `server` writes the same copy beside its server, so either
   build serves a host on the build machine.
2. The host depends on the copy by its path and on the facade, and its build script takes the three
   artifacts into its `OUT_DIR` with `aosabi_build::take_artifacts`, as the build script of the
   server package of a card does. The script reruns when one of them changes, and a missing artifact
   fails it with a message that names the command which writes it.

A module of WebAssembly for a page builds the host as a `cdylib` for `wasm32-unknown-unknown` and
exports one function that hands a string to `answer` and returns the answer, through the binding
the host chooses. The engine adds no JavaScript to the module.

The engine keys the hashes of the maps of a request with random bytes, and the standard library of
`wasm32-unknown-unknown` has no source of them. So such a host installs one before its first
answer: a type that implements `aosabi::embed::Randomness`, whose `fill` fills a buffer from
`crypto.getRandomValues` of the page or of the Worker through the binding of the host, handed to
`aosabi::embed::install_randomness` once. The first provider a process installs stays. Until a
provider is installed and its `fill` succeeds, the engine reads no body and answers each with one
error whose message is `no source of random bytes: the engine answers no request on this target
until the host installs a provider with install_randomness`, and it tries again at the next answer,
so a host installs the provider before its first answer. A host on a target whose standard library
draws random bytes from the operating system, such as Linux, macOS or Windows, needs no such step,
and each answer keys the thread that calls it before it reads the body. Only the thread that booted
an engine holds the roots of its app, so a host calls the engine on that thread.

## 4. What a request sees

A body is a JSON object with the string `query` and optionally `variables` and `operationName`, and
a `variables` or an `operationName` of `null` reads as an absent one, as a GraphQL server over HTTP
reads it. Every document is validated, as a server validates it.

Every request runs on a fresh agent with virtual timers, which the host never drives. A timer a
resolver sets, such as `aosabi::time::sleep`, fires at once in the order of the deadlines, and the
agent runs until it is idle after its answer, so a timer that a resolver left behind fires before
the next body is read. A resolver that waits on what no job and no timer of its agent settles stalls
the request, which then answers one error.

A resolver that reads the context with `get_context()` sees a synthetic `POST /graphql` with no
header, and `get_env()` reads no variable. A host that needs a value in its resolvers hands it over
through the state of the app.

The engine reads two keys of the configuration of the app, as a served app reads them.
`.masked_errors(MaskedErrors::On)` answers an unexpected error with `Unexpected error.` and the code
`INTERNAL_SERVER_ERROR`, and `.introspection(false)` refuses a document that selects a field of an
introspection type with the errors of its validation and the code `GRAPHQL_VALIDATION_FAILED`. An
app that sets neither key answers with the messages of its errors and with introspection on.

The answer is the JSON text of the execution result, `data` and `errors` as the executor writes
them. It is byte for byte the result that the mode `--requests` of a served binary of the same app
writes for the same document.

## 5. What takes no part

No HTTP request reaches the app, so everything of the chain that acts on one takes no part:

- The routes beside `/graphql`, such as `.get("/status", ..)`, are never called.
- The middleware never runs, so a header it stamps or a check it makes does not happen. A check
  that must hold in a host belongs in the body of the resolver, as [rust.md](rust.md) section 8
  places permissions.
- The plugins of the app never run, the serving plugin and the identity plugin among them.
- A subscription operation answers one error, `A subscription answers a stream, which
  run_subscription returns.`, and never runs, since the answer of a subscription is a stream and the
  facade answers each body with one body.

An app that needs any of them is served through a card. The same app may do both, since the
embedding reads the roots alone.

## 6. The bodies that are no request

A body that the engine cannot read as a request answers a response with one error and no data, as a
GraphQL server over HTTP answers a request it cannot read:

- Text that is no JSON, such as `{ broken`, answers `the request body is not JSON: ` followed by
  the reason of the reader.
- JSON that is no object, such as `[1]`, answers `the request body is not a JSON object`.
- An object with no string `query`, such as `{}`, answers `the request carries no string field
  query`.
- An `operationName` that is no string answers `the field operationName is no string`.
- A request whose resolver throws a value that is no GraphQL error answers the message of the
  thrown error, or `the request threw a value that is no error` for a value that is no error, and
  the mask where the key `maskedErrors` turns it on.
- A request that stalls answers `the request stalled: no job, no timer and no waker is left and the
  result is pending`.

For `{ broken` the answer is
`{"errors":[{"message":"the request body is not JSON: expected a string at byte offset 2"}]}`. The
engine answers the next body as before, so one bad body never ends a host.

## 7. A complete host

The host [tests/fixtures/rs/embed-host](../../tests/fixtures/rs/embed-host) reads one request body
per line of its standard input and writes one answer per line. It boots the app
[tests/fixtures/rs/embed-app](../../tests/fixtures/rs/embed-app), a drawer of shapes with a union,
and names the facade and the app by their paths in this repository, as a host names them from a
checkout of Aosabi, since Aosabi is released through its repository alone. Its manifest, shortened
to the parts that make it a host:

```toml
[workspace]
resolver = "3"

[package]
name = "embed-host"
version = "0.0.0"
edition = "2024"
publish = false
build = "build.rs"

[dependencies]
# The generated copy of the app, whose module __aosabi holds the entry.
embed-app = { path = "../embed-app/target/aosabi/app" }
aosabi = { path = "../../../../crates/aosabi" }

[build-dependencies]
aosabi-build = { path = "../../../../crates/aosabi-build" }
```

Its build script, `build.rs`:

```rust
fn main() -> std::process::ExitCode {
    aosabi_build::take_artifacts("../embed-app/target/aosabi")
}
```

Its program, `src/main.rs`, without its comments, which also prints the schema under `--schema`:

```rust
use std::{
    io::{
        self,
        BufRead as _,
        Write as _, //
    },
    process::ExitCode,
};

use aosabi::embed::Embedded;

const EX_SOFTWARE: u8 = 70;
const EX_IOERR: u8 = 74;

fn main() -> ExitCode {
    let engine = match Embedded::boot(embed_app::__aosabi::entry, aosabi::embed_output!()) {
        Ok(engine) => engine,
        Err(error) => {
            eprintln!("embed-host: {error}");
            return ExitCode::from(EX_SOFTWARE);
        }
    };
    let mut out = io::stdout().lock();
    if std::env::args().nth(1).as_deref() == Some("--schema") {
        return match out.write_all(engine.schema().as_bytes()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(_) => ExitCode::from(EX_IOERR),
        };
    }
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else {
            return ExitCode::from(EX_IOERR);
        };
        if line.trim().is_empty() {
            continue;
        }
        let answer = engine.answer(&line);
        if writeln!(out, "{answer}")
            .and_then(|()| out.flush())
            .is_err()
        {
            return ExitCode::from(EX_IOERR);
        }
    }
    ExitCode::SUCCESS
}
```

From the root of the repository, the app builds first and the host after it:

```sh
cargo build --locked -p aosabi-cli --bin cargo-aosabi
target/debug/cargo-aosabi build --locked --device worker \
  --manifest-path tests/fixtures/rs/embed-app/Cargo.toml
cargo build --locked --manifest-path tests/fixtures/rs/embed-host/Cargo.toml
echo '{"query":"{ hello }"}' | tests/fixtures/rs/embed-host/target/debug/embed-host
```

```json
{"data":{"hello":"Hello, world"}}
```

## 8. Where the embedding is judged

The notebook [80_embed](../../tests/notebooks/80_embed.ipynb) judges both layers. It holds
`aosabi_core::embed::answer`, the entry below the facade, byte for byte to the executor over the
cases of the capture corpus whose tables become a service and that run with validation, through the
operation `embed` of `aosabi-devtool execute`, and checks every body that is no request. Its section
The facade of a host builds the app with the cards `server` and `worker` and the host above, and
holds every answer of the host byte for byte to the binary of the card `server` in the mode
`--requests`, or to the operation `embed` of the devtool for a body that is no request. Variants of
the app hold the answers under the keys `maskedErrors` and `introspection` and the error of a boot
whose key names a mask function of its own. Its section The key of a fresh thread calls each entry
that answers a request on two fresh threads and checks that the entry gives each thread a key of its
own, and its section The nesting bound of a request holds the entry to the bound of 1,000 levels
under which the service parses a request document. The notebook belongs to the fast subset of
[.github/scripts/fast_tests.py](../../.github/scripts/fast_tests.py), which continuous integration
runs on every change, and [conformance.md](conformance.md) says how to run the suite.
