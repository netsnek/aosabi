// SPDX-License-Identifier: MIT
/*
 * graphtp-homa handler: the one trait a GraphQL executor serves GraphTP
 * through.
 *
 * No analogue in the reference. The trait is the one of the specification.
 */

//! The one trait a GraphQL executor serves `GraphTP` through.
//!
//! The server hands each OPERATION that passes the 11 rules to a [`Handler`]
//! with its credential and its decoded content. The handler completes the
//! exchange with [`Server::answer`], or opens a stream with
//! [`Server::open_stream`] and hands its entries to [`Server::next`],
//! [`Server::complete`] and [`Server::aborted`]. It may do so at once or later,
//! so a synchronous executor and an asynchronous one both fit.

use crate::server::{
    Exchange,
    Server, //
};

/// A GraphQL executor behind a `GraphTP` server over the peers of type `P`.
pub trait Handler<P> {
    /// Takes the OPERATION `exchange` with its credential `credential`, the
    /// value of `Authorization` or `None`, and its decoded content `content`.
    ///
    /// The handler answers the exchange through `server` now or keeps it and
    /// answers it later. An exchange never answered holds its Homa RPC until
    /// the peer goes down.
    fn operation(
        &mut self,
        exchange: Exchange<P>,
        credential: Option<&[u8]>,
        content: &[u8],
        server: &mut Server<P>,
    );
}

/// A closure with the arguments of [`Handler::operation`] is a handler, which
/// serves a harness and a synchronous executor with no type of its own.
impl<P, F> Handler<P> for F
where
    F: FnMut(Exchange<P>, Option<&[u8]>, &[u8], &mut Server<P>),
{
    fn operation(
        &mut self,
        exchange: Exchange<P>,
        credential: Option<&[u8]>,
        content: &[u8],
        server: &mut Server<P>,
    ) {
        self(exchange, credential, content, server);
    }
}
