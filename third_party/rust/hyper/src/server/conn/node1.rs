// SPDX-License-Identifier: MIT
/*
 * hyper::server::conn::node1: the public face of the Node connection.
 *
 * Translation of the connection of Node 24.13.0, whose connection machine
 * lives in src/proto/h1/node/conn.rs.
 */
//! The public face of the Node connection, which serves HTTP/1.1 as Node 24.13.0 does.
//!
//! A [`Connection`] parses every byte of a read as llhttp does before the
//! services move, but where Node runs code inside a read: it polls a service
//! once when it hands it its head, as Node runs the request listener inside
//! the callback of the head, and moves the services and the bodies after each
//! piece of a body, as Node runs its ticks after the callback of a piece of a
//! body. It hands each request to the service as soon as its head is
//! complete, writes the answers in the order of their requests and writes
//! Node's own answers where Node writes them, with the bounds of a
//! [`NodeConfig`]. With the feature `node-conn` off this module does not exist
//! and a server serves through `http1`.

use alloc::boxed::Box;
use core::fmt;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

use crate::body::{Body, Incoming};
use crate::common::time::Time;
use crate::error::StdError;
use crate::proto::h1::node::conn::NodeConn;
use crate::rt::bare::{Read, Write};
use crate::service::HttpService;

pub use crate::proto::h1::node::{NodeConfig, NodeLimits, OverFields};

/// Why a Node connection ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionEnd {
    /// The client or an answer closed the connection, or a `CONNECT` ended it.
    Closed,
    /// A request was refused with one of Node's fixed answers or with none.
    Refused,
    /// A head or a request passed its timer.
    TimedOut,
    /// The body of an answer failed, and no further byte was written.
    StreamError,
    /// An answer asked the connection to end after its bytes.
    Destroyed,
}

/// A builder of Node connections.
#[derive(Clone, Debug)]
pub struct Builder {
    config: NodeConfig,
    timer: Time,
}

impl Builder {
    /// Returns a builder of connections under `config`.
    pub fn new(config: NodeConfig) -> Self {
        Builder {
            config,
            timer: Time::empty(),
        }
    }

    /// Sets the timer of the connections, which runs Node's timers.
    #[cfg(feature = "std")]
    pub fn timer<M>(&mut self, timer: M) -> &mut Self
    where
        M: crate::rt::Timer + Send + Sync + 'static,
    {
        self.timer = Time::rt(timer);
        self
    }

    /// Sets the timer of the connections, one that measures time from its own
    /// epoch and needs no standard library.
    pub fn bare_timer<M>(&mut self, timer: M) -> &mut Self
    where
        M: crate::rt::bare::Timer + Send + Sync + 'static,
    {
        self.timer = Time::bare(timer);
        self
    }

    /// Binds a connection over `io` to `service`.
    ///
    /// The returned future serves the connection until it ends and must be
    /// polled for the connection to make progress.
    pub fn serve_connection<I, S, B>(&self, io: I, service: S) -> Connection<I, S>
    where
        I: Read + Write + Unpin,
        S: HttpService<Incoming, ResBody = B>,
        B: Body + 'static,
        B::Error: Into<Box<dyn StdError + Send + Sync>>,
    {
        Connection {
            inner: NodeConn::new(io, service, self.config, self.timer.clone()),
        }
    }
}

/// A Node connection bound to a service.
#[must_use = "futures do nothing unless polled"]
pub struct Connection<I, S>
where
    S: HttpService<Incoming>,
{
    inner: NodeConn<I, S>,
}

impl<I, S> fmt::Debug for Connection<I, S>
where
    S: HttpService<Incoming>,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Connection").finish()
    }
}

impl<I, S, B> Future for Connection<I, S>
where
    I: Read + Write + Unpin,
    S: HttpService<Incoming, ResBody = B>,
    B: Body + 'static,
    B::Error: Into<Box<dyn StdError + Send + Sync>>,
{
    type Output = crate::Result<ConnectionEnd>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.inner.poll_connection(cx)
    }
}
