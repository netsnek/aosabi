// SPDX-License-Identifier: MIT
/*
 * aosabi-serve: the body hyper sends, over the body of an answer of the
 * adapter.
 *
 * No analogue on the reference side. The Node adapter writes its answer to a
 * Node socket there, and this file hands the same bytes to hyper as a body of
 * unknown size.
 *
 * It carries the half of the serve layer of the quirk
 * Q51_NODE_ADAPTER_NULL_BODY of docs/byte-equality.md section 13.1.
 */

//! `ServeBody`.
//!
//! The size hint of the body is never exact and the body is never at its end
//! before its last frame, so hyper keeps every length line of the record as
//! the record wrote it, adds no `content-length` of its own, and frames a null
//! body chunked where Node frames it, with `0\r\n\r\n` at the end. A chunk
//! that fails ends the body with [`ChunkError`], on which the Node connection
//! writes no further byte (quirk `Q46`). An answer of HTTP/2 with no body ends
//! its stream with its `HEADERS` frame, which [`ServeBody::for_http2`] asks
//! for.
//!
//! A stream body keeps no handle of its driver, which runs as a task of its
//! own from the moment the handler spawned it. The drop of a [`ServeBody`]
//! that still holds a stream drops the stream, which is how the stream of a
//! subscription learns that its client went away or a write failed: it cancels
//! on that drop, and its driver runs the cancel to its end. A stream that
//! ended has left the body before, so its drop cancels nothing.

use core::{
    pin::Pin,
    task::{
        Context,
        Poll, //
    },
};

use aosabi_http::message::{
    ChunkError,
    NodeBody, //
};
use bytes::Bytes;
use http_body::{
    Frame,
    SizeHint, //
};

/// The body of an answer as hyper sends it.
pub struct ServeBody {
    /// What is left of the body, `None` once its last frame went out.
    body: Option<NodeBody>,
    /// Whether an empty body tells hyper it is at its end, which HTTP/2 reads
    /// to end the stream with the head.
    end_when_empty: bool,
}

impl ServeBody {
    /// Returns the body that sends `body`.
    #[must_use]
    pub fn new(body: NodeBody) -> Self {
        Self {
            body: Some(body),
            end_when_empty: false,
        }
    }

    /// Returns the body for an answer of HTTP/2, which reports an empty body
    /// at its end, so the stream ends with its head.
    #[must_use]
    pub fn for_http2(mut self) -> Self {
        self.end_when_empty = true;
        self
    }
}

impl http_body::Body for ServeBody {
    type Data = Bytes;
    type Error = ChunkError;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, ChunkError>>> {
        loop {
            match self.body.take() {
                None | Some(NodeBody::Empty) => return Poll::Ready(None),
                Some(NodeBody::Full(bytes)) => {
                    if bytes.is_empty() {
                        return Poll::Ready(None);
                    }
                    return Poll::Ready(Some(Ok(Frame::data(Bytes::from(bytes)))));
                }
                Some(NodeBody::Stream(mut source)) => match source.poll_chunk(cx) {
                    Poll::Pending => {
                        self.body = Some(NodeBody::Stream(source));
                        return Poll::Pending;
                    }
                    Poll::Ready(None) => return Poll::Ready(None),
                    Poll::Ready(Some(Err(error))) => return Poll::Ready(Some(Err(error))),
                    Poll::Ready(Some(Ok(chunk))) => {
                        self.body = Some(NodeBody::Stream(source));
                        if chunk.is_empty() {
                            continue;
                        }
                        return Poll::Ready(Some(Ok(Frame::data(Bytes::from(chunk)))));
                    }
                },
            }
        }
    }

    fn is_end_stream(&self) -> bool {
        self.end_when_empty && matches!(self.body, None | Some(NodeBody::Empty))
    }

    fn size_hint(&self) -> SizeHint {
        SizeHint::default()
    }
}
