// SPDX-License-Identifier: MIT
/*
 * aosabi-http: the neutral messages the Node adapter of aosabi-serve and the
 * serve layer exchange.
 *
 * No analogue on the reference side. The messages are the request and the
 * answer of the Node adapter with the parts of the connection taken out, so
 * every serve path passes them whatever adapter it runs.
 */

//! The neutral messages of a served request and its answer.
//!
//! A serve path turns what its connection read into a [`NodeIncoming`], hands
//! it to the Node adapter of `aosabi-serve` and writes the [`NodeOutgoing`] the
//! adapter answers. The Node connection of the fork of hyper carries the parts
//! of a connection, the framing, the timers and the queues of the answers, so
//! the messages hold what the adapter reads and decides and nothing a
//! connection owns. The same messages serve HTTP/1.1, HTTP/2 and HTTP(Unix
//! Socket), which is why a request names its scheme and whether it came over
//! HTTP/2.
//!
//! A header value is one byte per character, the Latin-1 form Node reads a
//! head in, and [`FirstWrite`] names the path of the answer that fixes how a
//! value is written back (N13 of the plan's version 2 draft, quirk
//! `Q58_NODE_ENCODING_FIRST_WRITE` of `docs/byte-equality.md` section 13.1).

use alloc::{
    borrow::Cow,
    boxed::Box,
    vec::Vec, //
};
use core::{
    fmt,
    future::Future,
    pin::Pin,
    task::{
        Context,
        Poll, //
    }, //
};

/// The first write Node's response sees after `writeHead`, which fixes the encoding of the head.
///
/// Node holds the head as a string and sends it with the first write. A string
/// write joins the head to it in the string's encoding, UTF-8 when none is
/// named, and a write of bytes queues the head alone as Latin-1. The place of
/// the head beside a queued `100 Continue` follows the same write: in front of
/// it for [`FirstWrite::Bytes`] on an answer with a body and no chunked
/// framing, behind it for every other write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FirstWrite {
    /// A call of `end(bytes)` or `write(bytes)`, empty bytes among them, or a
    /// chunk of a body read on the object path: Latin-1.
    Bytes,
    /// A call of `end()` with no data, or of `end('')`, which Node's `end`
    /// treats as no data: Latin-1.
    End,
    /// A call of `end(string)` with a string that is not empty: UTF-8, and
    /// Latin-1 when the answer has no body.
    Text,
    /// A call of `flushHeaders()`, which sends an empty string with no
    /// encoding: UTF-8.
    Flush,
}

/// One line of the record `writeHead` receives, its name as the record spells it.
///
/// The value is one byte per character, the Latin-1 form Node reads a value
/// in, and it is written back as [`FirstWrite`] says and checked as Node's
/// `writeHead` checks it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    /// The name, spelled as the record spells it.
    pub name: Cow<'static, str>,
    /// The value, one byte per character.
    pub value: Vec<u8>,
}

impl Line {
    /// Creates the line `name` with the value `value`.
    pub fn new(name: impl Into<Cow<'static, str>>, value: impl Into<Vec<u8>>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }
}

/// A request as a serve path hands it to the adapter.
///
/// The fields are the parts of an `IncomingMessage` of Node or an
/// `Http2ServerRequest` the adapter of `@hono/node-server` 1.19.9 reads: the
/// method, the target, the version, the fields, the first `Host` and the body,
/// with the scheme of the connection and the mark of a request of the HTTP/2
/// server, which the URL of the request needs.
pub struct NodeIncoming {
    /// The method, one of the 35 names Node accepts.
    pub method: &'static str,
    /// The target as it arrived, or the `:path` of an HTTP/2 request.
    pub raw_target: Vec<u8>,
    /// The version, `(0, 9)`, `(1, 0)`, `(1, 1)` or `(2, 0)`, and `(2, 0)` for
    /// a request over HTTP/2.
    pub version: (u8, u8),
    /// Whether the request came from the HTTP/2 server.
    ///
    /// The field is never read from `version`, because an HTTP/1.1 request
    /// line may name `HTTP/2.0` and the adapter tests the class of the request
    /// and not its version.
    pub http2: bool,
    /// The scheme, `https` inside TLS and `http` outside it on HTTP/1.x, and
    /// the `:scheme` as it arrived on HTTP/2.
    pub scheme: Vec<u8>,
    /// The kept fields, names in lower case, in arrival order.
    pub fields: Vec<(Vec<u8>, Vec<u8>)>,
    /// The first `Host` of the head, or the `:authority` of an HTTP/2 request
    /// and else its first `host` field.
    pub host: Option<Vec<u8>>,
    /// The body of the request.
    pub body: Box<dyn IncomingBody>,
}

impl fmt::Debug for NodeIncoming {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NodeIncoming")
            .field("method", &self.method)
            .field("raw_target", &self.raw_target)
            .field("version", &self.version)
            .field("http2", &self.http2)
            .field("scheme", &self.scheme)
            .field("fields", &self.fields)
            .field("host", &self.host)
            .finish_non_exhaustive()
    }
}

/// The body of a request, read one chunk at a time.
pub trait IncomingBody {
    /// Polls the next chunk of the body, `None` at its end.
    fn poll_chunk(&mut self, cx: &mut Context<'_>) -> Poll<Option<Result<Vec<u8>, BodyError>>>;
}

/// The body of a request with no bytes, which a `GET` and a `HEAD` read.
#[derive(Clone, Copy, Debug, Default)]
pub struct EmptyBody;

impl IncomingBody for EmptyBody {
    fn poll_chunk(&mut self, _cx: &mut Context<'_>) -> Poll<Option<Result<Vec<u8>, BodyError>>> {
        Poll::Ready(None)
    }
}

/// Why the body of a request ended before its last byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyError {
    /// The parser refused a byte of the body.
    Refused,
    /// The peer closed the connection.
    Closed,
    /// The body passed the ceiling of its card.
    TooLarge,
}

impl fmt::Display for BodyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Refused => "the request body was refused",
            Self::Closed => "the connection closed before the request body ended",
            Self::TooLarge => "the request body passed its ceiling",
        })
    }
}

impl core::error::Error for BodyError {}

/// An answer as the adapter hands it to a serve path.
pub struct NodeOutgoing {
    /// The status.
    pub status: u16,
    /// The reason a failed `writeHead` left, and `None` otherwise, which reads
    /// the reason of the status.
    pub reason: Option<&'static str>,
    /// The record `writeHead` receives, in its order.
    pub record: Vec<Line>,
    /// The body.
    pub body: NodeBody,
    /// The first write of the path the adapter took.
    pub first_write: FirstWrite,
    /// Whether the connection ends unless the body of the request ends soon
    /// after the answer, true for a method other than `GET` and `HEAD`.
    pub cleanup_incoming: bool,
    /// Whether the connection ends after the answer, as
    /// `handleResponseError` destroys the socket.
    pub destroy: bool,
}

impl fmt::Debug for NodeOutgoing {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NodeOutgoing")
            .field("status", &self.status)
            .field("reason", &self.reason)
            .field("record", &self.record)
            .field("first_write", &self.first_write)
            .field("cleanup_incoming", &self.cleanup_incoming)
            .field("destroy", &self.destroy)
            .finish_non_exhaustive()
    }
}

/// The body of an answer.
pub enum NodeBody {
    /// No body.
    Empty,
    /// The whole body, written in one write.
    Full(Vec<u8>),
    /// A body written chunk by chunk as it comes.
    Stream(Box<dyn ChunkSource>),
}

/// A body written chunk by chunk as it comes.
pub trait ChunkSource {
    /// Polls the next chunk, `None` at the end of the body.
    ///
    /// An error ends the connection with no further byte, as the adapter's
    /// `handleStreamError` destroys the socket.
    fn poll_chunk(&mut self, cx: &mut Context<'_>) -> Poll<Option<Result<Vec<u8>, ChunkError>>>;

    /// Returns the driver of the stream, handed out once, which the serve layer spawns beside the write.
    ///
    /// A stream of a subscription has a driver that runs its agent while the
    /// chunks are written, and every stream of a single result has none, so
    /// the default answers `None`.
    fn take_driver(&mut self) -> Option<Pin<Box<dyn Future<Output = ()>>>> {
        None
    }
}

/// The failure of a body stream, which ends the connection with no further byte.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChunkError;

impl fmt::Display for ChunkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the body stream of the answer failed")
    }
}

impl core::error::Error for ChunkError {}
