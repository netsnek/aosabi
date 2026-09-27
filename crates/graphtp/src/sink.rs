// SPDX-License-Identifier: MIT
/*
 * graphtp sink: where the writers of the crate put bytes, a vector, a slice or
 * pages the caller owns.
 *
 * No analogue in the reference. The sink is the one the specification names.
 */

//! The sink the writers of the crate put their bytes into.
//!
//! A put appends all of its bytes or none of them, so a writer that meets a
//! full sink leaves no half written field behind. A vector grows, a slice and
//! a list of pages hold what the caller gave them, and a microcontroller that
//! allocates nothing writes into the pages of its message.

use alloc::vec::Vec;
use core::{
    error,
    fmt, //
};

/// The refusal of a sink that has no room for all the bytes of one put.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Full;

impl fmt::Display for Full {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the sink has no room for the bytes")
    }
}

impl error::Error for Full {}

/// Where the writers of the crate put their bytes.
pub trait Sink {
    /// Appends all of `bytes` to the sink, or none of them.
    ///
    /// # Errors
    ///
    /// Returns [`Full`] when the sink has no room for all of `bytes`, and then
    /// the sink holds what it held before the call.
    fn put(&mut self, bytes: &[u8]) -> Result<(), Full>;
}

impl<S: Sink + ?Sized> Sink for &mut S {
    fn put(&mut self, bytes: &[u8]) -> Result<(), Full> {
        (**self).put(bytes)
    }
}

/// A vector grows by the bytes of each put, and is full only when the
/// allocator refuses the room.
impl Sink for Vec<u8> {
    fn put(&mut self, bytes: &[u8]) -> Result<(), Full> {
        self.try_reserve(bytes.len()).map_err(|_| Full)?;
        self.extend_from_slice(bytes);
        Ok(())
    }
}

/// A sink over one slice the caller owns, filled from its start.
#[derive(Debug)]
pub struct Slice<'a> {
    /// The slice the bytes go into.
    buf: &'a mut [u8],
    /// The count of bytes written, the start of the free part of `buf`.
    len: usize,
}

impl<'a> Slice<'a> {
    /// Returns the empty sink over the slice `buf`.
    #[must_use]
    pub fn new(buf: &'a mut [u8]) -> Slice<'a> {
        Slice { buf, len: 0 }
    }

    /// Returns the count of bytes written.
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Tells whether no byte is written.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns the count of bytes the slice still has room for.
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.buf.len() - self.len
    }

    /// Returns the bytes written, from the start of the slice.
    #[must_use]
    pub fn written(&self) -> &[u8] {
        self.buf.get(..self.len).unwrap_or_default()
    }
}

impl Sink for Slice<'_> {
    fn put(&mut self, bytes: &[u8]) -> Result<(), Full> {
        let end = self.len.checked_add(bytes.len()).ok_or(Full)?;
        let free = self.buf.get_mut(self.len..end).ok_or(Full)?;
        free.copy_from_slice(bytes);
        self.len = end;
        Ok(())
    }
}

/// A sink over pages the caller owns, filled page after page.
///
/// A page is anything that lends its bytes as a mutable slice, such as an
/// array, a vector or a borrowed slice, and the pages may differ in size. A
/// put that crosses the end of a page goes on at the start of the next one.
#[derive(Debug)]
pub struct Pages<'a, P> {
    /// The pages the bytes go into, in order.
    list: &'a mut [P],
    /// The index of the page the next byte goes into.
    page: usize,
    /// The offset in that page of the next byte.
    offset: usize,
    /// The count of bytes written over all pages.
    len: usize,
    /// The count of bytes all pages hold together.
    capacity: usize,
}

impl<'a, P: AsMut<[u8]>> Pages<'a, P> {
    /// Returns the empty sink over the pages `pages`.
    #[must_use]
    pub fn new(pages: &'a mut [P]) -> Pages<'a, P> {
        let capacity = pages.iter_mut().map(|page| page.as_mut().len()).sum();
        Pages {
            list: pages,
            page: 0,
            offset: 0,
            len: 0,
            capacity,
        }
    }

    /// Returns the count of bytes written over all pages.
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Tells whether no byte is written.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns the count of bytes the pages still have room for.
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.capacity - self.len
    }

    /// Returns the index of the page where the next put starts and the offset
    /// in that page.
    ///
    /// A page that a put fills to its end is left behind, so the position is
    /// then the start of the next page.
    #[must_use]
    pub fn position(&self) -> (usize, usize) {
        (self.page, self.offset)
    }
}

impl<P: AsMut<[u8]>> Sink for Pages<'_, P> {
    fn put(&mut self, bytes: &[u8]) -> Result<(), Full> {
        if bytes.len() > self.remaining() {
            return Err(Full);
        }
        let mut rest = bytes;
        while !rest.is_empty() {
            let page = self.list.get_mut(self.page).ok_or(Full)?;
            let free = page.as_mut().get_mut(self.offset..).unwrap_or_default();
            let count = free.len().min(rest.len());
            let (now, later) = rest.split_at_checked(count).ok_or(Full)?;
            free.get_mut(..count).ok_or(Full)?.copy_from_slice(now);
            self.len += count;
            rest = later;
            if count == free.len() {
                self.page += 1;
                self.offset = 0;
            } else {
                self.offset += count;
            }
        }
        Ok(())
    }
}
