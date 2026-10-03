// SPDX-License-Identifier: MIT
/*
 * aosabi-fetch: the body of a response.
 *
 * No analogue on the reference side, where the body of a response streams.
 */

//! The response body read whole up to its ceiling.
//!
//! An outbound call answers its body whole, so a body is bounded where the
//! reference streams it without a bound: a body above [`CEILING`] ends the
//! call with `BodyTooLarge`, which no case of the reference reaches.

use std::rc::Rc;

use aosabi_core::net::FetchError;
use aosabi_http::Body;

/// The bytes of a response body one call reads at most, 16 MiB.
pub const CEILING: usize = 16 * 1024 * 1024;

/// The bytes a body reserves before it reads, whatever length its head
/// claims, so a peer that claims a large body and sends none costs no more.
const RESERVE: usize = 65_536;

/// A response body read whole, refused once it passes its ceiling.
#[derive(Debug)]
pub struct BodyBuffer {
    /// The bytes read so far.
    bytes: Vec<u8>,
    /// The bytes the body may hold.
    ceiling: usize,
}

impl BodyBuffer {
    /// Returns an empty body bounded by `ceiling`.
    #[must_use]
    pub fn new(ceiling: usize) -> Self {
        Self {
            bytes: Vec::new(),
            ceiling,
        }
    }

    /// Returns an empty body that expects `length` bytes, bounded by `ceiling`.
    ///
    /// # Errors
    ///
    /// Returns [`FetchError::BodyTooLarge`] when `length` passes `ceiling`,
    /// before any byte of the body is read.
    pub fn with_length(length: u64, ceiling: usize) -> Result<Self, FetchError> {
        let length = usize::try_from(length).map_err(|_| FetchError::BodyTooLarge)?;
        if length > ceiling {
            return Err(FetchError::BodyTooLarge);
        }
        Ok(Self {
            bytes: Vec::with_capacity(length.min(RESERVE)),
            ceiling,
        })
    }

    /// Returns the bytes read so far.
    #[must_use]
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Returns whether no byte was read.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Appends `chunk`.
    ///
    /// # Errors
    ///
    /// Returns [`FetchError::BodyTooLarge`] when the body would pass its
    /// ceiling.
    pub fn push(&mut self, chunk: &[u8]) -> Result<(), FetchError> {
        if chunk.len() > self.ceiling - self.bytes.len() {
            return Err(FetchError::BodyTooLarge);
        }
        self.bytes.extend_from_slice(chunk);
        Ok(())
    }

    /// Returns the body of the response, bytes, empty ones included.
    #[must_use]
    pub fn into_body(self) -> Body {
        Body::Full(Rc::from(self.bytes))
    }
}
