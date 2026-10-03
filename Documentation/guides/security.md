# Security defaults

Aosabi puts security before features: a feature is off until a service names it, and the defaults
are the safe ones. This page lists the defaults with the code that sets them. A security issue goes
to the private vulnerability reporting of the repository, as [SECURITY.md](../../SECURITY.md) says.

## 1. TLS

A native build serves TLS through `rustls` with the provider `ring` once the service turns on the
feature `tls` of the facade, and the server and the outbound client speak TLS 1.3 alone.

- The server reads a certificate chain of any CA and its private key from the two PEM files that
  `AOSABI_TLS_CERT` and `AOSABI_TLS_KEY` name, the key as PKCS#1, PKCS#8 or SEC1, RSA or ECDSA. The
  boot fails when one of the two is set alone or a file does not read.
- It offers HTTP/2 and HTTP/1.1 through ALPN, asks for no client certificate, and refuses a client
  held at TLS 1.2 in the handshake with the alert `protocol_version`.
- A handshake that has not ended two minutes after the accept closes the socket with no further
  byte.
- The outbound client of `aosabi::fetch` checks the server against the system's bundle of CA
  certificates, and against one more file that `AOSABI_EXTRA_CA_CERTS` names, and ends every call
  after 30 seconds.

The settings stand in [crates/aosabi-serve/src/tls.rs](../../crates/aosabi-serve/src/tls.rs) and in
the client of [crates/aosabi-fetch](../../crates/aosabi-fetch/). A TypeScript app takes TLS, the
outbound `fetch` and the crypto of its host, Bun or Cloudflare Workers, and neither `core.wasm` nor
the native library links a TLS or crypto library of its own, so one artifact never carries two
crypto stacks.

## 2. CORS and CSRF

The engine adds no CORS header of its own and guesses no origin. A service names the sites a browser
may call it from, once for every route, with the middleware `cors()` and `csrf()` and their option
`origin`:

```ts
import {cors} from 'hono/cors'
import {csrf} from 'hono/csrf'

const SITES = ['https://app.example']

app.use('*', cors({origin: SITES, credentials: true}))
app.use('*', csrf({origin: SITES}))
```

- Without `cors()` a browser on another site gets no CORS header, so it cannot read an answer.
- Without `csrf()` the app accepts a form that another site posts, so a service that a browser
  reaches with cookies writes `csrf()` beside `cors()`.
- A TypeScript app takes both from `hono`, which runs its routes. A Rust service takes the `cors`
  and `csrf` of the facade, which carry the same names and the same options, so one rule holds on
  both layers and no second check runs beside it.

## 3. What a default build leaves out

Every optional feature of the facade is off until a service or its card names it:

| Feature | What it adds |
| --- | --- |
| `serve`, `unix-socket`, `linux` | The native server, HTTP on a Unix socket, the `epoll` reactor |
| `tls`, `http2`, `h2c` | TLS 1.3, HTTP/2 through ALPN, HTTP/2 by prior knowledge |
| `fetch` | The outbound HTTP client |
| `auth-zitadel` | The OpenID Connect login and provider through Zitadel |
| `sentry` | Reports to Sentry |
| `federation` | Delegation to other GraphQL services |
| `serial`, `usb`, `bluetooth`, `homa-network`, `lora` | The carriers of GraphTP |

The pages of development stay off as well. The landing page, GraphiQL and the route `/viewer` answer
only where the app turns them on, and uploads only where it sets `multipart: true`. The helper
`requireAuth` refuses a request with no user with 401.

Aosabi sends no telemetry of any kind, in the server, the command line and the npm packages.

## 4. Unsafe code

The workspace sets `unsafe_code = "forbid"`, and the project's own crates hold no unsafe block,
function, impl or trait. Three crates that build a WebAssembly module set `deny` in its place, for
the named forms of this table alone:

| Crate | What it allows |
| --- | --- |
| `graphtp-wasm` | The export attribute `#[unsafe(no_mangle)]` of each exported function |
| `aosabi-js` | The same, and one `unsafe extern "C"` block of imports that are all `safe fn` |
| `aosabi-infer-wasm` | The same as `aosabi-js` |

Each such line stands under its own `#[expect(unsafe_code)]` with its reason, and none of them is an
unsafe block. The native library that `aosabi-js` builds for Bun imports no function by name and
takes the functions of the host as typed pointers when it boots.

The unsafe code a build runs is that of the forks under [third_party/](../../third_party/), of the
outside crates and of the standard library. The project keeps a register of every such place, and
its own pipeline holds the register to the tree, so a change that adds one changes the register as
well. The probes that a notebook writes and builds to measure a device stand outside the workspace
and ship in no artifact.

## 5. A small supply chain

A crate from outside the project enters only where the standard library and code written in the
project fall short: when it is more secure than own code, clearly faster, carried by the Linux
kernel or impossible to write within the rules of the workspace, and then only when it is well
known, well maintained and really needed. The four forks of HTTP, `hyper` 1.11.1, `h2` 0.4.19,
`http` 1.5.0 and `http-body` 1.1.0, are vendored under `third_party/rust/` and built from the tree.

The outside crates each deployed build links, counted on 2026-10-26 with
`cargo tree -p <package> -e normal,no-proc-macro --locked --offline --target <target>` and the
features of the build, as the distinct packages that come from a registry:

| Build | Target | Outside crates |
| --- | --- | ---: |
| The facade with its defaults | Linux and `riscv32imc-unknown-none-elf` | 1 |
| A Worker of the Rust layer | `wasm32-unknown-unknown` | 1 |
| `core.wasm` and the native library for Bun | `wasm32-unknown-unknown` and Linux | 1 |
| The server card | Linux, `aarch64` and `x86_64` | 5 |
| The server card with TLS and HTTP/2 | Linux, `aarch64` and `x86_64` | 15 |
| Every feature on Linux | Linux, `aarch64` and `x86_64` | 16 |
| The server for a small Linux device | `mipsel-unknown-linux-musl` | 7 |

The one crate of the default build is `once_cell`. The server card adds `bytes`, `rustix`,
`bitflags` and `linux-raw-sys`, and TLS adds `rustls`, `rustls-pki-types`, `rustls-webpki`, `ring`,
`getrandom`, `libc`, `cfg-if`, `untrusted`, `subtle` and `zeroize`. A crate that runs on the build
host alone, such as the parser `syn` of the builder or `typescript` of the dev package, ships in no
artifact and counts in no row.

## 6. Local sockets and links

- HTTP over a Unix socket listens on `<name>.http` in the directory `XDG_RUNTIME_DIR` names, which
  must belong to the user of the process with the mode 0700, and the socket takes the mode 0600. On
  Linux, where the server runs in the initial namespaces of the host, it also reads the user of each
  peer through `SO_PEERCRED` and closes a peer of another user at once, with no byte. In a container
  the modes of the directory and the socket are the boundary.
- GraphTP bounds the credential, the decoded content and the window of its coding, and keeps the
  credential out of the compressed bytes. [graphtp.md](graphtp.md) section 10 says more.

## 7. Failing safe

The release profile aborts on a panic, so a panic of a release build never unwinds through foreign
frames. The single pass denies indexing, `unwrap`, `expect`, `panic!` and `unreachable!` in its
whole module, reads every buffer through a checked access, and hands a document to the validation
rules on any state it holds impossible.
