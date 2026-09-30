// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-core resolvers: the resolver wrapping.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/core/define-pylon.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 *
 * One file per part. Each file names its own line range.
 */

//! The resolver wrapping.
//!
//! A TypeScript service lets an application write a resolver as a plain function over
//! positional arguments that returns plain values, objects with methods and
//! promises of either. The source file named above turns that into what a GraphQL
//! executor calls, and this directory is that file.
//!
//! | Module | Translates | Holds |
//! | --- | --- | --- |
//! | [`get_selected_fields`](mod@get_selected_fields) | source lines 22-27 and 44-189 | the selection walk, the discriminator injection and the two caches |
//! | [`wrap_resolver`](mod@wrap_resolver) | source lines 191-360 | the wrapping of a value a resolver returns |
//! | [`resolvers_to_graphql_resolvers`](mod@resolvers_to_graphql_resolvers) | source lines 29-33 and 362-486 | the root resolvers of a service |
//! | [`service_error`] | source lines 488-506 | the error an application throws on purpose |
//! | `view` | the object source lines 280-359 build | the projection as a view of the selection |

pub mod get_selected_fields;
pub mod resolvers_to_graphql_resolvers;
pub mod service_error;
mod view;
pub mod wrap_resolver;

pub use self::{
    get_selected_fields::{
        CallState,
        SelectedField,
        get_selected_fields, //
    },
    resolvers_to_graphql_resolvers::{
        Resolvers,
        resolvers_to_graphql_resolvers,
        root_graphql_resolver,
        with_scope, //
    },
    service_error::{
        ServiceError,
        ServiceErrorExtensions, //
    },
    wrap_resolver::{
        bind_if_method,
        wrap_resolver, //
    }, //
};
