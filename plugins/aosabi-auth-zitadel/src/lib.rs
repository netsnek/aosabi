// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-auth-zitadel: the Zitadel integration of Aosabi.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/auth/zitadel.ts,
 * Apache-2.0. Changed by the Aosabi authors.
 *
 * This file holds the module list.
 */

//! The Zitadel integration of Aosabi.
//!
//! The crate has two halves that share one bootstrap per issuer. The login half, `zitadel_login`,
//! is a plugin that adds the browser routes `<endpoint>/login`, `<endpoint>/logout` and
//! `<endpoint>/callback`, where the callback sets the session cookie. The provider half,
//! `zitadel_auth`, reads the token of a request from its header, its query or that cookie,
//! introspects it and fetches the user info, and answers the principal of the request. Every
//! outbound request goes through the network capability of `aosabi-core`. The feature `ring`, a
//! default, signs and digests over `ring` on a target of `cfg(unix)`, and the feature
//! `host-crypto` hands the signature, the digest and the read of a key file to the functions of
//! the host the crate runs in, `Host`, which `install_host` keeps, the build of the module
//! `core.wasm` and of the native library of the TypeScript option. There the bootstrap, which
//! imports the key of the signer, never settles with a configuration, so every path after it is
//! dead, and the bootstrap, the login routes and the provider return that error where those paths
//! start, which keeps them out of the module of a Worker.
//!
//! The modules, each with what it translates and what it holds:
//!
//! - `zitadel` translates `auth/zitadel.ts:26-105` and `:151-202` and holds the login half, the
//!   auth key, the private key and the bootstrap cache.
//! - `zitadel_auth` translates `auth/zitadel.ts:26-45` and `:107-149` and holds the provider half,
//!   the token reader, the roles and the principal.
//! - `crypto` has no analogue and holds the signer and the digest over `ring` or the host.
//! - `oidc` translates `openid-client` and `oauth4webapi` and holds the OIDC client subset the two
//!   halves reach.
//!
//! The root re-exports the items of the two halves, which the facade names under
//! `aosabi::auth::zitadel` behind its feature `auth-zitadel`: the login half `zitadel_login`,
//! `ZitadelLoginOptions` and `ZitadelLoginPlugin`, and the provider half `zitadel_auth`,
//! `ZitadelAuthOptions`, `ZitadelAuth`, `ZitadelProvider`, `zitadel_principal` and `OidcUser`. The
//! hidden `verify_rs256` serves the devtool alone, and `Host` with `install_host` the host of the
//! TypeScript option, which `ZitadelAuth::claims` answers the claims of a request to.
//!
//! There is no test in this crate, which is the rule of CONTRIBUTING.md
//! section 3. The judging notebooks are `tests/notebooks/61_oidc_login.ipynb`
//! and `tests/notebooks/63_zitadel_auth.ipynb`.

#![forbid(unsafe_code)]

mod crypto;
pub mod oidc;
pub mod zitadel;
pub mod zitadel_auth;

#[doc(hidden)]
pub use self::crypto::verify_rs256;
#[cfg(feature = "host-crypto")]
pub use self::crypto::{
    Host,
    install_host, //
};
pub use self::{
    zitadel::{
        ZitadelLoginOptions,
        ZitadelLoginPlugin,
        zitadel_login, //
    },
    zitadel_auth::{
        OidcUser,
        ZitadelAuth,
        ZitadelAuthOptions,
        ZitadelProvider,
        zitadel_auth,
        zitadel_principal, //
    },
};
