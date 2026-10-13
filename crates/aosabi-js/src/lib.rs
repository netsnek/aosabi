// SPDX-License-Identifier: MIT
/*
 * aosabi-js: the module core.wasm of the npm package aosabi and its native
 * library for Bun on Linux, the engine of Aosabi with every default layer and
 * no app behind the host boundary of the TypeScript option.
 *
 * No analogue in the reference.
 */

//! The module `core.wasm` of the npm package `aosabi` and its native library
//! for Bun on Linux.
//!
//! The module holds the engine of Aosabi with every default layer an app
//! can reach on its GraphQL endpoint and no app. The wrapper of the package
//! runs the routes, the middleware and the context `c` of an app in the real
//! hono of its host, `aosabi/pylon`, with its base pipeline, and hands the
//! engine the requests of the GraphQL endpoint and of the routes of the
//! plugins of the engine, the record of a request and the values of the app as
//! handles. The engine reads the app's objects, calls its functions and
//! settles its promises through the one import block of the module `abi`, so
//! every byte of an answer of the endpoint is the engine's.
//!
//! | Module | Holds |
//! | --- | --- |
//! | `abi` | The export lines and the one import block, where `unsafe_code` is expected |
//! | `native` | The table of the functions of the host a native library takes at boot in place of the import block |
//! | `host` | The input and output buffers and the tagged record codec |
//! | `value` | `JsObject`, the functions and the promises of the host over the imports |
//! | `request` | The pending table, one entry per request in flight, the two job modes and the body pushed after its head |
//! | `endpoint` | The GraphQL endpoint, answered by the server of `graphql-over-http` with no router |
//! | `clock` | The clock of the host as the clock of the timers of the agents of the engine |
//! | `pubsub` | The pub subs of the app, whose topics live in the pub sub of the engine |
//! | `context` | The context the engine runs a request of the host in, and the members of it the host calls |
//! | `nodes` | The views of the resolve info, of the nodes of the document and of the arguments of an execution |
//! | `log` | The logger of the engine as the logger of the app |
//! | `plugin` | The plugins of the app as plugins of the engine, with the hook `onExecute` over `js_call` |
//! | `outbound` | The outbound fetch and the crypto of the host, the network capability and the backend `host-crypto` of `aosabi-auth-zitadel` |
//! | `zitadel` | The Zitadel layer of the app, its providers over `aosabi-auth-zitadel` and the plugin of its login routes |
//! | `federation` | The federations of the app over `aosabi-federation`, their delegations and the patches, guards and headers functions of the host they call |
//! | `sentry` | The Sentry of the app over `aosabi-sentry`, the plugin `useSentry`, `Sentry.init` and the client of a request |
//! | `boot` | The boot from the SDL and the IR, the handle of the graphql object, the plugins and the plugin markers, and the boot of the logger alone |
//! | `prebuilt` | The schema record of a build, which the export `schema_record` writes and the boot reads in the place of the IR |
//! | `execute` | The executor below HTTP of a test build, the feature `execute` |
//!
//! The same code builds two artifacts. For `wasm32` it is the module, whose
//! exports and imports are those of a wasm module. Under the feature `native`
//! it is the native library of the engine for Bun on Linux, a `cdylib` for
//! `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu` and their musl
//! twins, which exports the same export lines and takes the functions of the
//! host as typed `extern "C"` function pointers at boot, the table of the
//! module `native`, in place of the import block. So the crate root holds its
//! modules for `wasm32` and, with the feature, for those four targets, and it
//! is empty on any other target and without the feature. The feature is meant
//! for the four Linux targets alone, since its path packages compile for any
//! target it is turned on for. The crate denies `unsafe_code` in place of
//! forbidding it, every module but this root and `abi` forbids it, and no
//! unsafe block exists. The root holds no code but the declarations of its
//! modules, and a forbid there would bind `abi` as well. The judging notebooks
//! are `tests/notebooks/13_ts1_boundary.ipynb`, which serves apps on the
//! module through the wrapper `aosabi-ts/lib/core.js` and checks their
//! answers, `tests/notebooks/39_ts1_parity.ipynb`, which serves the build of
//! each fixture on the module and on the native library
//! through `aosabi-ts/lib/bun.js`, `tests/notebooks/36_ts1_gate.ipynb`, which
//! measures both and reads their trees and symbols, and
//! `tests/notebooks/08_card_builds.ipynb` and
//! `tests/notebooks/32_graphtp_fetch.ipynb`, which count the boundary, and
//! there is no test in this crate, the rule of `CONTRIBUTING.md` section 3.
#![cfg(any(
    target_arch = "wasm32",
    all(
        feature = "native",
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64"),
        any(target_env = "gnu", target_env = "musl"),
    ),
))]

mod abi;
mod boot;
mod clock;
mod context;
mod endpoint;
#[cfg(feature = "execute")]
mod execute;
mod federation;
pub mod host;
mod log;
#[cfg(not(target_arch = "wasm32"))]
mod native;
mod nodes;
mod outbound;
mod plugin;
mod prebuilt;
mod pubsub;
mod request;
mod sentry;
mod value;
mod zitadel;
