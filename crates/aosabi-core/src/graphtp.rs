// SPDX-License-Identifier: MIT
/*
 * aosabi-core graphtp: the GraphTP listener of the crate graphql-over-graphtp
 * under the paths a card and the devtool name, and the fetch of a served
 * build.
 *
 * No analogue in the reference. The listener is the one of
 * docs/graphtp-in-aosabi.md section 3, which the crate graphql-over-graphtp
 * holds.
 */

//! The `GraphTP` listener.
//!
//! The listener, its request and answer and the bounds of the cards are the
//! crate `graphql-over-graphtp`, which builds with no standard library, and
//! this module names its items under the paths of `aosabi-core` that a card
//! and the devtool name. The module [`served`] builds with the feature
//! `runtime` beside `homa` and holds the fetch a served build and a carrier
//! hand their listeners, straight through the hooks and the engine of the
//! booted app.

#[cfg(feature = "runtime")]
pub mod served;

pub use graphql_over_graphtp::{
    Answer,
    Body,
    Broken,
    CardLimits,
    Chunks,
    Fetch,
    Listener,
    Progress,
    Reply,
    Request,
    fetch,
    limits, //
};
