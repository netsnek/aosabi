// SPDX-License-Identifier: MIT
/*
 * aosabi-http: the body of a request or an answer, and the stream of its
 * pieces.
 *
 * No analogue on the reference side. A body there is a ReadableStream of the
 * platform, and this file holds the shapes a body takes: none, a
 * string, bytes, the string of the html template and a stream.
 */

//! The body of a request or an answer.
//!
//! The adapter of `@hono/node-server` tells a body by its JavaScript type: a
//! primitive string, a `Uint8Array` and a stream are kept by its lightweight
//! `Response`, while a `String` object, which the `html` template makes, is not
//! (A4 and A5 of the plan's version 2 draft). [`Body`] carries that type, and
//! every other reading takes the bytes of the body whatever its variant.
//!
//! A stream is shared by the objects that hold it, as a `ReadableStream` is
//! shared by two responses one of which was made from the other, so a clone of
//! a [`BodyStream`] reads the same stream.

use core::{
    cell::RefCell,
    fmt,
    future::Future,
    pin::Pin,
    task::{
        Context,
        Poll, //
    }, //
};
use std::rc::Rc;

/// A future that is neither `Send` nor `'static` bound by force, boxed and pinned.
///
/// Every handler and middleware future of the crate is `'static` and not
/// `Send`, because a served binary runs one thread.
pub type LocalBoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

/// A source of items polled one at a time, the one stream trait of the crate.
pub trait Stream {
    /// The item the stream yields.
    type Item;

    /// Polls the next item, `None` at the end of the stream.
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>>;

    /// Returns the driver of the stream, handed out once, which the serve layer spawns beside the write.
    ///
    /// A stream of a single result has none, which the default answers.
    #[must_use]
    fn take_driver(self: Pin<&mut Self>) -> Option<LocalBoxFuture<'static, ()>> {
        None
    }
}

/// Why a body could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BodyError {
    /// The parser of the connection refused a byte of the body.
    Refused,
    /// The connection closed before the body ended.
    Closed,
    /// The body passed the ceiling of its card.
    TooLarge,
    /// The body was read already, the `TypeError` `Body is unusable`.
    Used,
    /// The stream of the body failed with the message.
    Failed(String),
}

impl fmt::Display for BodyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused => f.write_str("the body was refused"),
            Self::Closed => f.write_str("the connection closed before the body ended"),
            Self::TooLarge => f.write_str("the body passed its ceiling"),
            Self::Used => f.write_str("Body is unusable: Body has already been read"),
            Self::Failed(message) => f.write_str(message),
        }
    }
}

impl core::error::Error for BodyError {}

/// The item of a body stream.
pub type Chunk = Result<Vec<u8>, BodyError>;

/// A body produced piece by piece, shared by every object that holds it.
#[derive(Clone)]
pub struct BodyStream(Rc<RefCell<Pin<Box<dyn Stream<Item = Chunk>>>>>);

impl BodyStream {
    /// Wraps `stream`.
    pub fn new(stream: impl Stream<Item = Chunk> + 'static) -> Self {
        Self(Rc::new(RefCell::new(Box::pin(stream))))
    }

    /// Polls the next chunk of the stream.
    pub fn poll_chunk(&self, cx: &mut Context<'_>) -> Poll<Option<Chunk>> {
        self.0.borrow_mut().as_mut().poll_next(cx)
    }

    /// Returns the driver of the stream, handed out once.
    #[must_use]
    pub fn take_driver(&self) -> Option<LocalBoxFuture<'static, ()>> {
        self.0.borrow_mut().as_mut().take_driver()
    }

    /// Returns the next chunk, `None` at the end of the stream.
    pub async fn next(&self) -> Option<Chunk> {
        core::future::poll_fn(|cx| self.poll_chunk(cx)).await
    }

    /// Returns whether `self` and `other` are the same stream.
    #[must_use]
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl fmt::Debug for BodyStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("BodyStream")
    }
}

/// A stream that yields the items of a list in order.
pub struct Chunks(std::collections::VecDeque<Chunk>);

impl Chunks {
    /// Creates the stream of `items`.
    pub fn new(items: impl IntoIterator<Item = Chunk>) -> Self {
        Self(items.into_iter().collect())
    }
}

impl Stream for Chunks {
    type Item = Chunk;

    fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Chunk>> {
        Poll::Ready(self.get_mut().0.pop_front())
    }
}

/// The body of a request or an answer.
#[derive(Clone, Debug)]
pub enum Body {
    /// No body, `null`.
    Empty,
    /// A primitive string.
    Text(Rc<str>),
    /// Bytes, a `Uint8Array`.
    Full(Rc<[u8]>),
    /// The string of the `html` template, a `String` object of the reference,
    /// which the adapter keeps in no cache and every other reading takes as
    /// its bytes.
    Html(Rc<str>),
    /// A stream.
    Stream(BodyStream),
}

impl Body {
    /// Returns whether the body is `null`.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }

    /// Returns the bytes of a body that is whole, and `None` for a stream.
    ///
    /// A string gives its UTF-8 bytes, as the Fetch extraction of a string
    /// body encodes it, and no body gives no bytes.
    #[must_use]
    pub fn whole_bytes(&self) -> Option<Rc<[u8]>> {
        match self {
            Self::Empty => Some(Rc::from(&[][..])),
            Self::Text(text) | Self::Html(text) => Some(Rc::from(text.as_bytes())),
            Self::Full(bytes) => Some(Rc::clone(bytes)),
            Self::Stream(_) => None,
        }
    }

    /// Reads the whole body into bytes.
    ///
    /// # Errors
    ///
    /// Returns the [`BodyError`] a chunk of a stream failed with.
    pub async fn bytes(&self) -> Result<Rc<[u8]>, BodyError> {
        if let Some(bytes) = self.whole_bytes() {
            return Ok(bytes);
        }
        let Self::Stream(stream) = self else {
            return Ok(Rc::from(&[][..]));
        };
        let mut bytes = Vec::new();
        while let Some(chunk) = stream.next().await {
            bytes.extend_from_slice(&chunk?);
        }
        Ok(Rc::from(bytes))
    }

    /// Returns the body as a stream, wrapping a whole body in a stream of one chunk.
    ///
    /// No body gives `None`, and a stream comes back as the same stream.
    #[must_use]
    pub fn to_stream(&self) -> Option<BodyStream> {
        match self {
            Self::Empty => None,
            Self::Stream(stream) => Some(stream.clone()),
            whole => {
                let bytes = whole.whole_bytes().unwrap_or_else(|| Rc::from(&[][..]));
                Some(BodyStream::new(Chunks::new([Ok(bytes.to_vec())])))
            }
        }
    }
}

/// Returns `bytes` decoded as UTF-8 with the replacement character, as Fetch decodes a body.
///
/// A leading byte order mark is dropped, which is UTF-8 decode of the Encoding
/// Standard. Under the feature `node-server` a second one is dropped too,
/// because the undici of Node 24.13.0 cuts the first itself and then decodes
/// with a `TextDecoder` that drops a leading mark again.
///
/// The bytes are checked by `str::from_utf8`, which reads a run of ASCII two
/// words a step once the run reaches a byte aligned to a word, and
/// `String::from_utf8_lossy` replaces the bytes that are no UTF-8 on the path
/// of a body that holds some alone.
#[must_use]
pub fn utf8_decode(bytes: &[u8]) -> String {
    const BOM: &[u8] = b"\xEF\xBB\xBF";
    let mut bytes = bytes.strip_prefix(BOM).unwrap_or(bytes);
    if cfg!(feature = "node-server") {
        bytes = bytes.strip_prefix(BOM).unwrap_or(bytes);
    }
    match core::str::from_utf8(bytes) {
        Ok(text) => String::from(text),
        Err(_) => String::from_utf8_lossy(bytes).into_owned(),
    }
}
