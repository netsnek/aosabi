// SPDX-License-Identifier: MIT
/*
 * hyper::ext::node: what the Node parser hands a service beside a request and
 * what a service hands the Node connection beside an answer.
 *
 * Translation of Node 24.13.0 lib/_http_common.js:53,
 * lib/_http_server.js:1161-1205, lib/_http_outgoing.js:341-360 and :610-625,
 * and @hono/node-server 1.19.9 dist/listener.js:375-387 and :509-548. It holds
 * the types of the quirks Q19, Q26, Q38, Q47 and Q59 of docs/byte-equality.md
 * section 13 of Aosabi, and no logic.
 */
//! The extensions of the Node parity layer.

use bytes::Bytes;

pub use crate::proto::h1::node::methods::NODE_METHODS;

/// The target of a request as it arrived.
///
/// llhttp accepts bytes in a target that `http::Uri` refuses, and the `Uri` of a
/// request the Node parser reads is built with those bytes percent encoded and
/// without the fragment. An adapter that needs the target as Node saw it reads it
/// here (quirk Q19).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawTarget(pub Bytes);

/// What Node's own checks read of a request head.
///
/// Node checks `Host` and `Expect` of an HTTP/1.1 request and reads `TE`, and
/// the parser reads all three over every field of a head, as llhttp sets its
/// flags (quirk Q20 and divergence D74 of quirk Q26).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeHead {
    /// The version of the request line, one of the four llhttp accepts.
    pub version: NodeVersion,
    /// The value of the first `Host` of the head, when one exists.
    pub host: Option<Bytes>,
    /// The `Expect` of the head.
    pub expect: NodeExpect,
    /// Whether a `TE` of the head names `chunked`.
    pub te_chunked: bool,
    /// Whether the connection stays open after the answer, as llhttp decides it.
    pub keep_alive: bool,
    /// Whether the request carries `Upgrade` with `upgrade` in `Connection`.
    pub upgrade: bool,
    /// Whether the method is `CONNECT`.
    pub connect: bool,
}

/// A version of the request line llhttp accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeVersion {
    /// `HTTP/0.9`, read under the rules of HTTP/1.0.
    Http09,
    /// `HTTP/1.0`.
    Http10,
    /// `HTTP/1.1`.
    Http11,
    /// `HTTP/2.0` on a plain HTTP/1 connection, read under the rules of HTTP/1.0.
    Http20,
}

/// The `Expect` of a request as Node reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeExpect {
    /// No `Expect` in the head.
    None,
    /// An `Expect` whose value holds `100-continue` as a word.
    Continue,
    /// Any other `Expect`, answered with 417 on HTTP/1.1.
    Other,
}

/// What a service asks of the Node connection beside an answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NodeAnswer {
    /// The first write of the answer, which decides where its head goes
    /// against a queued `100 Continue` (quirk Q38).
    pub first_write: FirstWrite,
    /// Whether the connection cleans up an unread request body after the
    /// answer as the adapter does (quirk Q47).
    pub cleanup_incoming: bool,
    /// Whether the connection ends with no further byte after the answer, as
    /// Node's check of a record destroys it (quirk Q59).
    pub destroy: bool,
}

/// The first write of an answer, as Node's outgoing message sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FirstWrite {
    /// A write of bytes.
    Bytes,
    /// The end of an answer with no write before it.
    End,
    /// A write of a string, whose characters from U+0080 up the first write encodes.
    Text,
    /// A flush of the head with no body byte.
    Flush,
}
