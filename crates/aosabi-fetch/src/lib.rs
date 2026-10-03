// SPDX-License-Identifier: MIT
/*
 * aosabi-fetch: the outbound HTTP client of the Linux cards.
 *
 * No file of its own on the reference side, where the global fetch of Node
 * is the network. Each module names the specification it follows in its own
 * header, or says that it has no analogue. This file holds the module list.
 */

//! The outbound HTTP client of the Linux cards.
//!
//! The crate is the driver of the network capability of `aosabi-core` on the
//! server and docker cards. A call takes an idle connection to its origin or
//! opens one, which resolves the host, connects on the runtime of
//! `aosabi-serve` and runs the TLS 1.3 handshake for `https:`, writes the
//! request, reads the answer and keeps the connection for the next call to
//! the origin where the request and the answer allow it, as the fetch of Node
//! keeps it, while after an answer of HTTP/1.0 that names `keep-alive` the
//! fetch of Node keeps the connection and the client does not. It builds for
//! the native targets of `cfg(unix)`.
//!
//! The facade hands [`driver`] to the boot under its feature `fetch`.
//!
//! | Module | Translates | Holds |
//! | --- | --- | --- |
//! | `client` | the Fetch standard as a server's outbound call uses it, over the idle connections of undici | `NativeFetch` |
//! | `head` | RFC 9112 sections 2 to 7 | the request writer and the response reader of HTTP/1.1 |
//! | `tls` | no analogue | the TLS stream of a client over rustls |
//! | `roots` | no analogue | the root store and the extra CA file |
//! | `resolve` | no analogue | the address of a host and the last good answer of each name |
//! | `body` | no analogue | the response body up to its ceiling |
//! | `error` | no analogue | the mapping of every failure onto `FetchError` |
//!
//! There is no test in this crate, which is the rule of CONTRIBUTING.md
//! section 3. The judging notebooks are `tests/notebooks/62_sentry.ipynb`,
//! and `61_oidc_login.ipynb`, `63_zitadel_auth.ipynb` and
//! `61_oidc_budget.ipynb` for the calls of the issuer.

pub mod body;
pub mod client;
pub mod error;
pub mod head;
pub mod resolve;
pub mod roots;
pub mod tls;

use std::{
    io,
    rc::Rc, //
};

use aosabi_core::net::Fetch;

pub use crate::client::NativeFetch;

/// Returns the fetch driver of the server and docker cards, which the facade hands `serve_with`.
///
/// The boot installs the driver it answers on the runtime thread before the
/// first pass, so an outbound call of a setup or a request reaches the
/// network through [`NativeFetch`].
///
/// # Errors
///
/// Returns what [`NativeFetch::from_env`] returns, which the boot prints
/// before it exits.
pub fn driver() -> io::Result<Rc<dyn Fetch>> {
    Ok(Rc::new(NativeFetch::from_env()?))
}
