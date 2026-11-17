# Hardening a service for production

Some defaults of Aosabi serve the developer, such as the message of an unexpected error in an answer
and the schema through introspection, and some settings only the service can choose, such as the
sites a browser may call it from. This page lists each step by which a service hardens itself before
it goes to production, with what the step does, its default and its line of code on the Rust layer
and in the TypeScript layer. Each step is a line the service writes on purpose, and no variable of
the environment, `NODE_ENV` among them, takes one for it. [security.md](security.md) lists the
defaults that hold with no line at all.

A Rust service writes its steps on the chain of its entry, as [rust.md](rust.md) shows. A TypeScript
app writes its keys into `aosabi.config.ts` and its middleware with `app.use` in its entry, as a
Pylon v3 app does, which [typescript.md](typescript.md) and [pylon.md](pylon.md) describe. Its
middleware `cors()`, `csrf()` and `bodyLimit()` come from the package `hono`, which the package
`aosabi` takes as a peer dependency in the range `^4.13.16`.

| Step | Default | Rust layer | TypeScript layer |
| --- | --- | --- | --- |
| The message of an unexpected error | The error's own message | `.masked_errors(MaskedErrors::On)` | `maskedErrors: true` |
| Introspection | On | `.introspection(false)` | `introspection: false` |
| Forms that other sites post | Refused on Rust where a cookie comes along, taken on TypeScript | `csrf(..)` in a plugin that runs first | `csrf()` |
| Answers that other sites read | No CORS header, but for the explorer lines of a new project | `.middleware(path, cors(..))` | `cors()` with the sites |
| Coded answers | Plain on Rust, coded on TypeScript | `.middleware(path, compress())` | No line |
| The size of a body | 1 MiB on Rust, no bound on TypeScript | `body_limit(..)` | `bodyLimit(..)` |
| The tokens of a document | 120,008 | `.max_tokens(..)` | `maxTokens` |
| The session cookie | `aosabi-auth`, with `Secure` over HTTPS | `cookie_name` and a line of the proxy | `cookieName` and a line of the proxy |
| Sentry | No variable, no credential and no address of a client in an event | `include_execute_variables` left unset | `includeExecuteVariables` left unset, `enabled: false` |
| The redirects of the federation | Followed with the headers it forwards | An `https` URL of an own service | The same |
| A target without random bytes | 503 until a provider is installed | `install_randomness(..)` | Installed at the boot |
| TLS | Plain HTTP until it is set, then TLS 1.3 alone | The feature `tls` and two variables | TLS at a proxy |

## 1. The message of an unexpected error

An error that a resolver throws and that is no GraphQL error the service made itself, such as a
failed read of a database, is unexpected. By default its answer carries the error's own message, in
every build and whatever `NODE_ENV` says, so a developer sees what failed. A service in production
masks it: the answer then carries the message `Unexpected error.` in its place, and a client learns
nothing of the inside of the service. A GraphQL error that the service made itself keeps its message
and its extensions in both cases.

The key decides what a client reads, the message of the error or `Unexpected error.` with the code
`INTERNAL_SERVER_ERROR`, and never whether the log holds the error. With the mask on or off, the
server writes the record `graphql error` at the level `error` for every unexpected error, with the
error, its path and its code, and where the mask is on it also writes the original error on standard
error. The log holds the message of every unexpected error in both cases, so the log is what an
operator keeps private. A gateway of the federation that faces clients turns the mask on as well,
since an unmasked error of the gateway carries the answer of a remote service that is no GraphQL
result and the operation the gateway sent to it.

```rust
use aosabi::{MaskedErrors, prelude::*};

pub fn app() -> impl IntoApp {
    App::new().query(Query).masked_errors(MaskedErrors::On)
}
```

```ts
import type {AppConfig} from 'aosabi/pylon'

export default {
  maskedErrors: true
} satisfies AppConfig
```

The embedding of [embedding.md](embedding.md) reads the same key, so a host masks as a served app
masks. Its boot refuses a key that names a mask function of its own, `mask_error`, with an error
that says so, and a card serves such an app over HTTP.

## 2. Introspection

The fields `__schema` and `__type` answer the whole schema to any client, which the tools of
development read. Introspection is on by default. A service whose clients need no schema at run
time turns it off. A request that selects either field then fails its validation with the error
`GraphQL introspection has been disabled, but the requested query contained the field "__schema".`,
which names the field it found, and runs nothing. `__typename` still answers. The embedding reads
the key as well and refuses such a document with the same errors.

```rust
App::new().query(Query).introspection(false)
```

```ts
export default {
  introspection: false
} satisfies AppConfig
```

An app moved over from Pylon v3 that turned introspection off with its key `graphiql: false` sets
`introspection: false` in its place, since the layer reads no key `graphiql`.

## 3. Forms that other sites post

A browser sends the cookies of a service with a form that a page of another site posts to it. The
middleware `csrf()` refuses such a request with 403 `Forbidden`: one whose method is neither `GET`
nor `HEAD`, whose `Content-Type` is missing or names a type that a form or a link sends,
`application/x-www-form-urlencoded`, `multipart/form-data`, `text/plain` or `text/ping`, whose
`Sec-Fetch-Site` is not `same-origin` and whose `Origin` is not the origin of the service, or with
the option `origin` none of the sites it names.

The base pipeline of a Rust service runs this check with its defaults on every request that carries
a `Cookie` field, so only a page of the service itself posts such a form there. A service that takes
forms from sites of its own registers `csrf(options)` with those sites, and that check takes the
place of the default on a request where it stands ahead of the handlers of the routes the request
matches. The GraphQL endpoint counts as such a handler, so a check that stands behind it leaves the
default in place. The chain of the facade has no call for the check in this release, since the
closure that `.middleware(path, f)` takes is no check the dispatch tells apart from another
middleware, so a service registers it with `use_boxed` on the app of the engine, in a plugin that
runs first:

```rust
use std::rc::Rc;

use aosabi::{CsrfOptions, CsrfOrigin, PluginObject, Strategy, csrf, prelude::*};

pub fn app() -> impl IntoApp {
    let forms = PluginObject::new("forms").strategy(Strategy::First).setup(|app| {
        app.use_boxed(
            "*",
            Rc::new(csrf(CsrfOptions {
                origin: Some(CsrfOrigin::List(vec![String::from("https://app.example")])),
                ..CsrfOptions::default()
            })),
        );
    });
    App::new().query(Query).plugin(forms)
}
```

The TypeScript layer runs no check of its own, and an app that a browser reaches with cookies writes
`csrf()` before the routes it guards:

```ts
import {app} from 'aosabi/pylon'
import {csrf} from 'hono/csrf'

app.use('*', csrf({origin: ['https://app.example']}))
```

## 4. Answers that other sites read

A browser lets a page of another site read an answer only where the answer names that site in
`Access-Control-Allow-Origin`. Neither layer adds a CORS header of its own, so no page of another
site reads an answer until the service names its sites with the middleware `cors()`. `cors()` with
no option `origin` answers `*`, which every site reads, so a service in production always names its
sites, and it sets `credentials: true` only beside such a list:

```rust
use aosabi::{CorsOptions, CorsOrigin, cors, prelude::*};

pub fn app() -> impl IntoApp {
    App::new().query(Query).middleware(
        "*",
        cors(CorsOptions {
            origin: CorsOrigin::List(vec![String::from("https://app.example")]),
            credentials: true,
            ..CorsOptions::default()
        }),
    )
}
```

```ts
import {app} from 'aosabi/pylon'
import {cors} from 'hono/cors'

app.use('*', cors({origin: ['https://app.example'], credentials: true}))
```

A project that `aosabi new` writes is open to one site from its start. Its `src/lib.rs` registers
`cors()` for `https://aosabi.org` on `/graphql`, under a comment that points at this guide, so the
explorer that the site of Aosabi hosts reads the schema of the new service and runs its operations
from a browser:

```rust
    // These lines admit the hosted explorer. The hardening guide says what to do before production.
    let explorer = aosabi::cors(aosabi::CorsOptions {
        origin: aosabi::CorsOrigin::Exact(String::from("https://aosabi.org")),
        ..aosabi::CorsOptions::default()
    });
    App::new().query(Query).middleware("/graphql", explorer)
```

Before production a service removes these lines, or writes the sites of its own frontends in place
of `https://aosabi.org`. A TypeScript app that uses the explorer writes the same site itself, with
`app.use('/graphql', cors({origin: 'https://aosabi.org'}))`, and drops the line before production as
well.

## 5. Coded answers

The base pipeline of a Rust service codes no answer. A service codes the answers of the routes it
chooses with the middleware `compress()`, which codes them in deflate where the client accepts it
and adds `Accept-Encoding` to the field `Vary` of each answer it may code, whether the client
accepts a coding or not. It chooses routes whose answers hold no secret beside text that a client
sends, such as its public files:

```rust
use aosabi::{compress, prelude::*};

pub fn app() -> impl IntoApp {
    App::new().query(Query).middleware("/public/*", compress())
}
```

The base pipeline of the TypeScript layer runs `compress()` ahead of the routes, so its answers are
coded where the client accepts a coding and an app there writes no line.

## 6. The size of a body

The base pipeline of a Rust service bounds every read of a request body at 1 MiB, 1,048,576 bytes,
by `Content-Length` before a byte of the body is read and by the bytes it reads of a chunked body,
and a body past the bound answers 413 with the text `Payload Too Large`. A `body_limit` that the
service registers on a path takes the place of that bound there, and where two of them match a
request, the smaller holds. It decides before the routes run: a body with no usable
`Content-Length`, such as a chunked one, is read through its bound first, so a body past the bound
answers before any route, also before a route that never reads its body, and a body within it
reaches the route as the bytes read. A service whose documents are small lowers the bound of its
GraphQL endpoint:

```rust
use aosabi::{BodyLimitOptions, body_limit, prelude::*};

pub fn app() -> impl IntoApp {
    App::new().query(Query).middleware(
        "/graphql",
        body_limit(BodyLimitOptions {
            max_size: 64 * 1024,
            ..BodyLimitOptions::default()
        }),
    )
}
```

The TypeScript layer bounds no body by default, so an app writes the bound itself, before the
routes and the GraphQL endpoint it guards:

```ts
import {app} from 'aosabi/pylon'
import {bodyLimit} from 'hono/body-limit'

app.use('*', bodyLimit({maxSize: 1024 * 1024}))
```

## 7. The tokens of a document

The parser counts the tokens of a request document and stops at the cap, before the validation
runs, with the syntax error of graphql-js, `Syntax Error: Document contains more that 120008 tokens.
Parsing aborted.`, which names the cap. The cap is 120,008 tokens by default on both layers and
holds on every road that parses a request, HTTP and GraphTP among them. A service whose documents
are short lowers it, and one that needs longer documents raises it. Every value keeps a cap, the
largest is 4,294,967,295, and the boot of a TypeScript app refuses a value that is no whole number
from 0 to 4,294,967,295 with a `TypeError`.

```rust
App::new().query(Query).max_tokens(5_000)
```

```ts
export default {
  maxTokens: 5000
} satisfies AppConfig
```

## 8. The session cookie

The login routes of the Zitadel plugin set two cookies: the session cookie, with `HttpOnly`,
`SameSite=Lax` and the lifetime the issuer gives the token, or an hour, and the cookie
`aosabi_code_verifier` for the 300 seconds from a login to its callback. The session cookie is named
`aosabi-auth` unless the service names another, with the option `cookie_name` of
`ZitadelLoginOptions` and `ZitadelAuthOptions` on the Rust layer, and with the key `cookieName` of
`zitadelLogin`, `zitadelAuth` and `useAuth` in the TypeScript layer. The routes that set the cookie
and the provider that reads it take the same name:

```rust
let login = ZitadelLoginOptions {
    issuer: String::from("https://issuer.example"),
    cookie_name: Some(String::from("__Host-session")),
    ..ZitadelLoginOptions::default()
};
let identity = ZitadelAuthOptions {
    issuer: String::from("https://issuer.example"),
    cookie_name: Some(String::from("__Host-session")),
    ..ZitadelAuthOptions::default()
};
```

```ts
import {zitadelAuth, zitadelLogin} from 'aosabi/auth/zitadel'

const options = {issuer: 'https://issuer.example', cookieName: '__Host-session'}
const login = zitadelLogin(options)
const identity = zitadelAuth(options)
```

A name that is no valid cookie name fails the setup of the login routes on the Rust layer and the
call of `zitadelLogin`, `zitadelAuth` or `useAuth` in the TypeScript layer, each with a `TypeError`
that names the option. A name with the prefix `__Host-` or `__Secure-` needs HTTPS at the callback,
and the logout deletes it with `Secure`.

Each cookie that the login or the callback writes carries `Secure` only where the request reached
the service over HTTPS: where its URL starts with `https:` or where the first value of the header
`X-Forwarded-Proto` is `https`. A proxy that ends TLS and hands the request on over plain HTTP
therefore sets that header. The first value counts, so the proxy replaces a value that a client
sends in place of adding its own after it. Neither layer has a line for it, and the line belongs to
the proxy, in nginx:

```nginx
proxy_set_header X-Forwarded-Proto https;
```

## 9. Sentry

The Sentry plugin reports an error of an operation as an event. The request data of every event
leaves out the fields whose names mark a credential, such as `Authorization` and `Cookie`, every
field that carries the address of the client, such as `X-Forwarded-For` and `X-Real-IP`, and the
query parameters `token` and `variables`, whatever the options say. By default the event carries no
variable of the operation. The document of the operation travels in the extra `document`, so a value
that a client writes into the document itself reaches Sentry, and a client passes secrets and
personal data as variables. A service in production leaves the option that adds the variables,
`include_execute_variables` on the Rust layer and `includeExecuteVariables` in the TypeScript
layer, unset. The plugin of the Rust layer stands under the feature `sentry` of the facade:

```rust
use aosabi::{SentryPluginOptions, prelude::*, use_sentry};

pub fn app() -> impl IntoApp {
    App::new().query(Query).plugin(use_sentry(SentryPluginOptions::default()))
}
```

```ts
import {useSentry, type AppConfig} from 'aosabi/pylon'

export default {
  plugins: [useSentry()]
} satisfies AppConfig
```

A TypeScript app initializes the client with `init` of `aosabi/sentry/node`, and an environment that
sends no event, such as a run of its tests, passes `enabled: false`, for which the engine sets up no
client and no event leaves:

```ts
import * as Sentry from 'aosabi/sentry/node'

Sentry.init({enabled: false})
```

## 10. The redirects of the federation

The gateway that `createGateway` binds to another GraphQL service, and `create_gateway` of the Rust
layer under the feature `federation`, follows a redirect of that service and sends the request with
the headers it forwards to the location. A service names each remote service by an `https` URL of a
service its operator runs, forwards only the headers that service needs and keeps that service from
redirecting to hosts outside.

```ts
import {createGateway} from 'aosabi/pylon'

const users = createGateway<RemoteRegistry>().configure({
  url: 'https://users.internal.example/graphql',
  headers: ctx => ({authorization: ctx.req.header('authorization') ?? ''})
})
```

## 11. A target without random bytes

Each entry that answers a request keys the hash maps of its thread with random bytes before it
reads the request, and the id of a request and the event id of an error take their bytes from the
same provider. A native host reads the source of its operating system, and the module of the
TypeScript layer installs the source of its host, Bun or a Worker, at its boot, so neither needs a
line. The standard library of `wasm32-unknown-unknown`, the target of the WebAssembly of a page or
of a Worker, and of the systems `xous` and `vexos` has no such source. A host that links the Rust
engine into a program for such a target installs a provider with `install_randomness` before its
first request, in a page or a Worker one that reads `crypto.getRandomValues`. The first provider
installed stays.

```rust
use aosabi::embed::{RandomError, Randomness, install_randomness};

struct HostRandom;

impl Randomness for HostRandom {
    fn fill(&self, buf: &mut [u8]) -> Result<(), RandomError> {
        host_fill(buf).map_err(|_| RandomError)
    }
}

pub fn start() {
    install_randomness(Box::new(HostRandom));
}
```

`host_fill` stands for the function of the host that reads `crypto.getRandomValues`.

Until a provider is installed and fills, the engine reads no part of a request on such a target. An
app answers every request over HTTP, to its GraphQL endpoint and to its routes alike, with 503, no
header field and the text `no source of random bytes: the engine answers no request on this target
until the host installs a provider with install_randomness`, and over GraphTP with 503.0 and an
empty body, and the embedding answers every body with one error of that message. Each request tries
again, so the requests after the install are answered.

A build without the standard library, such as the core crates on a board with no operating system,
keeps a fixed key for the hashes of its maps and refuses no request for want of random bytes.

## 12. TLS

The native server serves TLS 1.3 alone, through `rustls` with the provider `ring`, once the facade
has its feature `tls`, which brings `serve` and `http2` with it, and the variables `AOSABI_TLS_CERT`
and `AOSABI_TLS_KEY` name the PEM files of the certificate chain and of its key. Without both it
serves plain HTTP, and one without the other ends the boot. A client held at TLS 1.2 fails the
handshake, and the outbound client of the service speaks TLS 1.3 alone as well. A project that
`aosabi new` writes adds `tls` to the features that its table for a Unix target names:

```toml
[target.'cfg(unix)'.dependencies]
aosabi = { path = "../aosabi/crates/aosabi", features = ["serve", "unix-socket", "deflate", "linux", "tls"] }
```

```sh
export AOSABI_TLS_CERT=/etc/aosabi/fullchain.pem
export AOSABI_TLS_KEY=/etc/aosabi/key.pem
```

On Bun the TypeScript layer serves plain HTTP, so TLS ends at a proxy in front of it, held to TLS
1.3 alone, in nginx with `ssl_protocols TLSv1.3;`. On Cloudflare Workers the platform ends TLS, and
the minimum version of TLS is a setting of the zone.
