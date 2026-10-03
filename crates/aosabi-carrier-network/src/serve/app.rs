// SPDX-License-Identifier: MIT
/*
 * aosabi-carrier-network serve app: the fetch of the booted app behind the
 * GraphTP listener of Network.
 *
 * No analogue on the reference side. A call runs straight through the hooks
 * and the engine of the booted app with the fixed fields of
 * docs/graphtp-in-aosabi.md section 2.1 as its request, so one GraphQL server
 * answers both protocols.
 */

//! The fetch of the booted app.
//!
//! [`app_fetch`] turns the booted app into the fetch function of the
//! listener, the fetch of a carrier of `aosabi_core::graphtp::served` with
//! the driver of a stream body spawned on the runtime of the thread: each
//! OPERATION runs straight through the hooks and the engine of the app with
//! `content-type`, `accept` and the credential as `authorization` as its
//! request, an app whose router holds more than the core, a plugin
//! middleware of its config among it, answers it as
//! `POST http://localhost/graphql` through its whole pipeline, and the
//! status, the substatus the HTTP layer decides, the media type and the body
//! go back to the listener, a stream body piece by piece. A value the road
//! throws is answered 500 with an empty body, as the Node adapter answers it.

use alloc::rc::Rc;

use aosabi_core::{
    BootedApp,
    graphtp::served, //
};
use aosabi_serve::runtime::spawn_local;
use graphql_over_graphtp::Fetch;

/// Returns the fetch function of the listener over the booted app `app`.
#[must_use]
pub fn app_fetch(app: Rc<BootedApp>) -> Fetch {
    served::carrier_fetch(app, spawn_local)
}
