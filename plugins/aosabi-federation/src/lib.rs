// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-federation: Federation, the port of the reference's federation and of
 * the delegation it runs on.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/core/index.ts,
 * Apache-2.0. Changed by the Aosabi authors.
 *
 * The root exports of the federation. This file holds the module list.
 */

//! Federation, the port of the reference's federation, `createGateway`.
//!
//! A service delegates a field to another service through a configured
//! federation. It is built over two layers of graphql-tools, the
//! delegation of `@graphql-tools/delegate` 12.0.9 with the wrapping schema of
//! `@graphql-tools/wrap` 11.1.9, and the HTTP executor of
//! `@graphql-tools/executor-http` 3.1.0. The crate holds the gateway half
//! and the graphql-tools half, and every function keeps the name of its source
//! in snake case.
//!
//! | Module | Translates | Holds |
//! | --- | --- | --- |
//! | [`gateway`] | `core/gateway.ts:126-168` and `:787-1018` | `create_gateway`, `GatewayFactory`, `Gateway`, `GatewayOptions`, `DelegateCall`, `reset_schema_cache`, `passthrough` and the cache of remote schemas of `getRemoteSchema` |
//! | [`headers`](mod@headers) | the header object of `core/gateway.ts:907-914` | `Headers`, `HeadersFn` and `headers!` |
//! | [`needs`](mod@needs) | `core/gateway.ts:25-81` and `:262-301` | `Needs`, `NeedsNode`, `NeedsBuilder`, `NeedsTarget`, `needs!` and `build_selections_from_needs` |
//! | [`ast_from_js`] | `core/gateway.ts:184-260` | `UploadCollector`, `is_binary` and `ast_from_js_value` |
//! | [`policy`](mod@policy) | `core/gateway.ts:496-571` | `FieldPolicy`, `Force`, `ForceFn`, `PatchPolicy`, `policy!`, `Pass`, `pass`, `Policies` and `collect_policies` |
//! | [`patches`](mod@patches) | the patch map of `core/gateway.ts:995-1000` | `Patches`, `IntoPatch` with the markers `OneArg`, `TwoArgs` and `Fallible`, `PatchKey`, `Passthrough` and `patches!` |
//! | [`transforms`] | the `Transform` interface of `@graphql-tools/delegate`, `core/gateway.ts:305-785` | `Transform`, `ExecutionRequest`, `DelegationContext`, the four transforms of the reference and the patch table `PatchTable` they run over |
//! | [`delegate`] | `@graphql-tools/delegate` `dist/index.cjs:10-2651` | `delegate_to_schema` and the delegation below it |
//! | [`wrap`] | `@graphql-tools/wrap` `dist/index.cjs:8-87` and `:2318-2363` | the proxying resolvers, `wrap_schema` and `schema_from_executor` |
//! | [`executor_http`] | `@graphql-tools/executor-http` `dist/index.cjs` and `@graphql-tools/executor-common` | `build_http_executor`, the multipart body, the errors of a fetch and the serialization of a request |
//! | [`utils`] | `@graphql-tools/utils` `cjs/errors.js` and five helpers | `create_graphql_error`, `relocated_error` and the helpers |
//!
//! The graphql-tools half holds its items: the delegation in [`delegate`], the
//! interface of a transform in [`transforms`], the helpers in [`utils`], and
//! the serialization, the multipart body and the errors of a request in
//! [`executor_http`]. The gateway half holds [`headers`](mod@headers), [`ast_from_js`],
//! [`policy`](mod@policy), the selection of [`needs`](mod@needs) and the four transforms
//! with the patch table they run over. [`gateway`], [`patches`](mod@patches) and the
//! typed builder of [`needs`](mod@needs) read the registry of a pulled remote. The
//! literals [`needs!`] and [`patches!`] are a closure over the typed builder
//! and a block per entry with a key type of its own, the forms the probe cells
//! of W00 decide. The judging notebooks are
//! `tests/notebooks/90_federation.ipynb`,
//! `tests/notebooks/90_federation_transforms.ipynb` and
//! `tests/notebooks/90_federation_budget.ipynb`. There is no test in this
//! crate, which is the rule of CONTRIBUTING.md section 3.

#![forbid(unsafe_code)]

pub mod ast_from_js;
pub mod delegate;
pub mod executor_http;
pub mod gateway;
pub mod headers;
pub mod needs;
pub mod patches;
pub mod policy;
pub mod transforms;
pub mod utils;
pub mod wrap;
