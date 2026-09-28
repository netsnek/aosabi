// SPDX-License-Identifier: MIT
/*
 * graphql-over-graphtp fetch: the request the GraphTP listener hands a fetch
 * function and the answer it reads back.
 *
 * No analogue in the reference. The request is the POST of
 * docs/graphtp-in-aosabi.md section 2.1 and docs/graphtp.md section 7, step 2,
 * and the answer carries what step 4 of that section reads: the status, the
 * substatus the HTTP layer decides beside it, the media type and the body.
 */

//! The request of the listener and the answer of a fetch function.
//!
//! The listener hands a fetch function each OPERATION as a [`Request`], the
//! fixed fields and the content, and takes back what the app answered as an
//! [`Answer`]. A served build hands the listener the fetch of its booted app,
//! which runs a call straight through the hooks and the engine, and the
//! harness the fetch of a bare server of `graphql-over-http` or of its own
//! service. The fetch is a shared function returning a future, so one app
//! serves every listener of a card and an answer may come later than the call.

use alloc::{
    boxed::Box,
    rc::Rc,
    vec::Vec, //
};
use core::{
    future::Future,
    pin::Pin,
    task::{
        Context,
        Poll, //
    }, //
};

/// The method of every request the listener builds.
pub const METHOD: &str = "POST";

/// The URL of every request the listener builds, the GraphQL endpoint of
/// an app on a host the request never names otherwise.
pub const URL: &str = "http://localhost/graphql";

/// The value of `content-type` of every request, the fixed request type of
/// `GraphTP`.
pub const CONTENT_TYPE: &str = "application/json";

/// The value of `accept` of every request, under which the HTTP layer answers
/// every result but a stream with the answer type and a subscription as an
/// event stream.
pub const ACCEPT: &str = "application/graphql-response+json, text/event-stream";

/// The request an OPERATION becomes, the POST of `docs/graphtp-in-aosabi.md`
/// section 2.1.
///
/// The request carries exactly the fields [`Request::fields`] names and no
/// `accept-encoding`, so the compression middleware of the app leaves the
/// answer alone.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Request {
    /// The value of `authorization`, visible ASCII, or `None` when no
    /// credential travels.
    pub credential: Option<Vec<u8>>,
    /// The body, the content of the OPERATION decoded.
    pub content: Vec<u8>,
}

impl Request {
    /// Returns the fields of the request in the order the listener names
    /// them: `content-type`, `accept`, and `authorization` when a credential
    /// travels.
    pub fn fields(&self) -> impl Iterator<Item = (&'static str, &[u8])> {
        [
            ("content-type", Some(CONTENT_TYPE.as_bytes())),
            ("accept", Some(ACCEPT.as_bytes())),
            ("authorization", self.credential.as_deref()),
        ]
        .into_iter()
        .filter_map(|(name, value)| value.map(|value| (name, value)))
    }
}

/// What the app answered a [`Request`].
pub struct Answer {
    /// The status the app wrote.
    pub status: u16,
    /// The substatus the HTTP layer decides where it decides the status, 0 for
    /// none, which the HTTP drivers never write.
    pub substatus: u8,
    /// The value of `content-type`, or `None` when the answer has none.
    pub content_type: Option<Vec<u8>>,
    /// The body.
    pub body: Body,
}

/// The body of an [`Answer`].
pub enum Body {
    /// A body the app wrote whole.
    Full(Vec<u8>),
    /// A body the app writes piece by piece, an event stream among them.
    Stream(Box<dyn Chunks>),
}

/// The failure of a body's source, which ends a stream with `aborted`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Broken;

/// The source of a body the app writes piece by piece.
///
/// The listener reads a stream's source only while the stream's queue has
/// room, so back pressure reaches the source, and drops the source when the
/// client cancels the stream or its link goes down, which returns it.
pub trait Chunks {
    /// Returns the next piece of the body, `None` once the body has ended, or
    /// [`Broken`] when its source failed.
    fn poll_chunk(&mut self, cx: &mut Context<'_>) -> Poll<Option<Result<Vec<u8>, Broken>>>;
}

/// The answer of a fetch function, which may come later than the call.
pub type Reply = Pin<Box<dyn Future<Output = Answer>>>;

/// A fetch function, shared by every listener of an app.
pub type Fetch = Rc<dyn Fn(Request) -> Reply>;
