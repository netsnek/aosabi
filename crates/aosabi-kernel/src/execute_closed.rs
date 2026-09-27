// SPDX-License-Identifier: MIT
/*
 * aosabi-kernel execute_closed: the executor entry below HTTP and the closed
 * executor.
 *
 * No analogue in the reference.
 */

//! The module holds the entry traits a carrier calls an executor through,
//! `execute(operation, variables, context)`, returning a result stream, with the HTTP
//! handler as one caller and a pluggable encoder of a result. Every item is `core` only
//! and names no type of a layer above, so an executor of any layer implements the
//! traits over its own variables, context and result. The first implementation is the
//! general executor of `aosabi-execute`.
//!
//! The document table behind [`Operation::Persisted`] lands with it.

use core::{
    pin::Pin,
    task::{
        Context,
        Poll, //
    }, //
};

/// What a carrier hands the executor, a document text or the id of a persisted document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation<'a> {
    /// A document the executor parses and validates.
    Document {
        /// The text of the document.
        text: &'a str,
        /// The name of the operation to run, when the document holds several.
        operation_name: Option<&'a str>,
    },
    /// A document the device holds in its table, named by its id.
    Persisted {
        /// The id of the document in the table.
        id: u32,
    },
}

/// A stream of results.
///
/// The shape is the one of a stream in the `futures` sense, written here because the
/// kernel links nothing above `core`.
pub trait ResultStream {
    /// The result the stream yields.
    type Item;

    /// Attempts to pull the next result out of the stream.
    ///
    /// Returns `Poll::Pending` when no result is ready and the waker of `cx`
    /// is kept, `Poll::Ready(Some(item))` for a result, and
    /// `Poll::Ready(None)` when the stream has ended.
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>>;
}

/// The entry below HTTP, `execute(operation, variables, context)`.
///
/// An executor implements the trait over its own types, and a carrier, the
/// HTTP handler being one, calls it and encodes what the stream yields.
pub trait Execute {
    /// The variable values of a request.
    type Variables: ?Sized;
    /// The context value of a request.
    type Context: ?Sized;
    /// The result of one execution.
    type Output;
    /// The stream [`Execute::execute`] returns.
    type Stream: ResultStream<Item = Self::Output>;

    /// Runs `operation` with `variables` and `context` and returns the stream of its results.
    fn execute(
        &self,
        operation: Operation<'_>,
        variables: &Self::Variables,
        context: &Self::Context,
    ) -> Self::Stream;
}

/// The one failure of a [`Sink`], which has no room left.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SinkFull;

/// Where an encoder writes.
pub trait Sink {
    /// Appends `bytes` to the sink.
    ///
    /// # Errors
    ///
    /// Returns [`SinkFull`] when the sink has no room left for `bytes`.
    fn write(&mut self, bytes: &[u8]) -> Result<(), SinkFull>;
}

/// The pluggable encoder of a result, JSON today and CBOR beside it later.
pub trait Encode<R: ?Sized> {
    /// Writes `result` into `sink` in the encoding of the implementor.
    ///
    /// # Errors
    ///
    /// Returns [`SinkFull`] when `sink` has no room left.
    fn encode(&self, result: &R, sink: &mut dyn Sink) -> Result<(), SinkFull>;
}
