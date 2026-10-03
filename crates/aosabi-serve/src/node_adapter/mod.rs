// SPDX-License-Identifier: MIT
/*
 * aosabi-serve: the port of the Node adapter of hono, @hono/node-server 1.19.9.
 *
 * The directory uses core and alloc alone.
 */

//! The Node adapter of hono.

pub mod handle_error;
pub mod outgoing;
pub mod request;

pub use self::{
    handle_error::{
        handle_fetch_error,
        handle_request_error,
        handle_response_error, //
    },
    outgoing::{
        NodeBindings,
        outgoing,
        serve, //
    },
    request::{
        RequestError,
        new_request, //
    }, //
};
