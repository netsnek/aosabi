// SPDX-License-Identifier: MIT AND Apache-2.0
// Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/auth/plugin.ts,
// Apache-2.0. Changed by the Aosabi authors.

//! `use_identity` and `IdentityPlugin` under the subpath `auth::plugin`.
//!
//! The identity plugin is the configuration plugin of the auth layer, so it
//! is exported from its own subpath, the convention `<feature>/plugin` of the
//! reference, while the root of the auth layer exports the authoring calls
//! alone. The plugin lives in `authz`, beside the calls whose key it shares.

pub use crate::authz::{
    IdentityPlugin,
    use_identity, //
};
