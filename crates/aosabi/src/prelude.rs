// SPDX-License-Identifier: MIT
/*
 * aosabi prelude: the names an application of the native layer imports at
 * once.
 *
 * The module is the prelude of a Rust crate, the form PyO3,
 * Bevy and Diesel open a file with.
 */

//! The names an application imports at once, `use aosabi::prelude::*;`.
//!
//! The prelude holds what the index and the types of a plain crate name of
//! the facade, so a crate of the native layer opens each file with one line,
//! and an explicit import of any name stays as good. Clippy's lint
//! `wildcard_imports`, which the group `pedantic` holds, exempts a glob of a
//! module named `prelude`, which the compile cell `c6-prelude-pedantic` of
//! `tests/fixtures/rs/c6/cells.toml` records.
//!
//! | Name | Holds |
//! | --- | --- |
//! | [`App`], [`IntoApp`] | the builder of an app and the trait the entry answers |
//! | [`Result`], [`Error`], [`ServiceError`] | the answer of a resolver, its error and the error a resolver throws on purpose |
//! | [`MutationPayload`] | the answer of a mutation method, the payload of `mutation(...)` with nullable fields and `userErrors` |
//! | [`ID`], [`Int`], [`Float`], [`Number`], [`Date`], [`File`], [`JSON`], [`Any`] | the markers of the scalars of the schema |
//! | [`json`] | `json::Map` and `json::Value`, the JSON object and value |
//! | [`get_context`], [`get_env`](fn@get_env), [`Key`] | the context of a request, its environment and the typed key of a variable of it |
//! | [`logger`](fn@logger), [`get_logger`] | the logger |
//! | [`PubSub`], [`Topic`], [`Stream`], [`stream`] | the pub sub a service keeps in a `static`, its topics, the stream a subscription answers and `stream::task` |
//! | `use_node_server`, `use_node_server_with` | the serving plugin of a card that serves sockets, under the feature `serve` on a native target |
//! | [`use_identity`] | the identity plugin of `aosabi::auth::plugin` |
//! | `use_sentry` | the Sentry plugin, under the feature `sentry` |
//! | `zitadel_login` | the login routes of Zitadel, under the feature `auth-zitadel` |
//!
//! The prelude exports no macro, since an application of the native layer
//! calls none.

/// The answer of a resolver, a route, a gate or an entry of the native
/// layer, whose error is [`Error`] unless it names another.
///
/// It is the `Result` of the standard library with a default for its error,
/// the form of `anyhow::Result`, so `?` throws every error that converts into
/// [`Error`].
pub type Result<T, E = Error> = core::result::Result<T, E>;

/// The JSON object and value under their Rust names, the names of
/// `serde_json`.
///
/// `Map::from([("id", id)])` builds an object, `Value::from(..)` a value and
/// `collect()` either, and the writer keeps the key order of JavaScript.
pub mod json {
    pub use aosabi_json::{
        Map,
        Value, //
    };
}

#[cfg(feature = "auth-zitadel")]
pub use crate::auth::zitadel::zitadel_login;
#[cfg(feature = "sentry")]
pub use crate::use_sentry;
pub use crate::{
    Any,
    Date,
    Error,
    File,
    Float,
    ID,
    Int,
    IntoApp,
    JSON,
    Key,
    MutationPayload,
    Number,
    PubSub,
    ServiceError,
    Stream,
    Topic,
    app::App,
    auth::plugin::use_identity,
    get_context,
    get_env,
    get_logger,
    logger,
    stream, //
};
#[cfg(all(feature = "serve", unix))]
pub use crate::{
    use_node_server,
    use_node_server_with, //
};
