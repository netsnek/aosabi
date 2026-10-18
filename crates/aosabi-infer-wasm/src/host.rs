// SPDX-License-Identifier: MIT
/*
 * aosabi-infer-wasm host: the buffers the module shares with the host and the
 * reader and writer of the records that cross between them.
 *
 * No analogue in the reference.
 */

//! The buffers the host writes and reads, and the records that cross the
//! boundary.
//!
//! One `thread_local` cell holds the input buffer, which the host writes, and
//! the output buffer, which the host reads, a plain state in a module without
//! threads. The export `alloc` sizes the input buffer and answers its
//! address, taken through `as_mut_ptr`, so its exposed provenance permits the
//! write of the host. The host writes the input buffer inside an import,
//! between its call of `alloc` and its return, while no reference of the
//! module to the buffer is alive, and keeps no address and no view of the
//! memory across calls, since a later call may move the buffer or grow the
//! memory. A record is copied out of the input buffer before the module asks
//! the host anything else, since the next import writes the buffer again.
//!
//! A count, a length and a handle are four little endian bytes, a truth is
//! one byte, and a text is a length and its UTF-8 bytes.

#![forbid(unsafe_code)]

use core::cell::RefCell;

/// The buffers of the module.
struct Buffers {
    /// The buffer the host writes a record into.
    input: Vec<u8>,
    /// The buffer the host reads an answer from.
    output: Vec<u8>,
}

// An `unsafe` cell would skip each borrow flag.
thread_local! {
    /// The buffers of the module.
    static BUFFERS: RefCell<Buffers> = const {
        RefCell::new(Buffers {
            input: Vec::new(),
            output: Vec::new(),
        })
    };
}

/// Returns the address of `bytes` in the memory of the module.
///
/// The address is taken through `as_mut_ptr`, so its exposed provenance is
/// that of a mutable borrow and permits the host to write the range, which
/// an address of a shared slice would not.
pub fn address(bytes: &mut [u8]) -> usize {
    bytes.as_mut_ptr().expose_provenance()
}

/// Returns `len` as the four bytes a length takes at the boundary.
#[must_use]
pub fn length(len: usize) -> u32 {
    u32::try_from(len).unwrap_or(u32::MAX)
}

/// Sizes the input buffer to `len` zero bytes and returns its address, or 0
/// when the allocator gives no buffer of that length.
// A buffer left unwritten for the host would skip this zeroing.
#[must_use]
pub fn alloc(len: u32) -> usize {
    let Ok(len) = usize::try_from(len) else {
        return 0;
    };
    BUFFERS.with_borrow_mut(|buffers| {
        let input = &mut buffers.input;
        input.clear();
        if input.try_reserve_exact(len).is_err() {
            return 0;
        }
        input.resize(len, 0);
        address(input)
    })
}

/// Releases both buffers.
pub fn dealloc() {
    BUFFERS.with_borrow_mut(|buffers| {
        buffers.input = Vec::new();
        buffers.output = Vec::new();
    });
}

/// Returns a copy of the first `len` bytes of the input buffer, or of the
/// whole buffer where it is shorter.
#[must_use]
pub fn input(len: u32) -> Vec<u8> {
    BUFFERS.with_borrow(|buffers| {
        let input = &buffers.input;
        let end = usize::try_from(len).map_or(input.len(), |len| len.min(input.len()));
        input.get(..end).map_or_else(Vec::new, <[u8]>::to_vec)
    })
}

/// Writes `record` with its length into the output buffer and returns the
/// address of the length, which the host reads before its next call.
#[must_use]
pub fn output(record: &[u8]) -> usize {
    BUFFERS.with_borrow_mut(|buffers| {
        let output = &mut buffers.output;
        output.clear();
        output.extend_from_slice(&length(record.len()).to_le_bytes());
        output.extend_from_slice(record);
        address(output)
    })
}

/// A reader of the fields of a record, which answers `None` past its end.
pub struct Reader<'a> {
    /// The bytes not read yet.
    rest: &'a [u8],
}

impl<'a> Reader<'a> {
    /// Returns the reader of `bytes`.
    #[must_use]
    pub const fn new(bytes: &'a [u8]) -> Self {
        Self { rest: bytes }
    }

    /// Reads one byte.
    pub fn byte(&mut self) -> Option<u8> {
        let (head, rest) = self.rest.split_first()?;
        self.rest = rest;
        Some(*head)
    }

    /// Reads a count, a length or a handle of four little endian bytes.
    pub fn word(&mut self) -> Option<u32> {
        let (head, rest) = self.rest.split_first_chunk::<4>()?;
        self.rest = rest;
        Some(u32::from_le_bytes(*head))
    }

    /// Reads a count of four little endian bytes as a size.
    pub fn count(&mut self) -> Option<usize> {
        usize::try_from(self.word()?).ok()
    }

    /// Reads a length and that many bytes as UTF-8.
    // An unchecked read would skip this check of UTF-8. The host writes each
    // text through `TextEncoder`, which writes no lone surrogate, but this
    // check is what holds the module to UTF-8 whatever host calls it.
    pub fn text(&mut self) -> Option<String> {
        let len = self.count()?;
        let (bytes, rest) = self.rest.split_at_checked(len)?;
        self.rest = rest;
        String::from_utf8(bytes.to_vec()).ok()
    }

    /// Returns whether every byte is read.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rest.is_empty()
    }
}

/// Appends a count or a length of four little endian bytes.
pub fn push_count(out: &mut Vec<u8>, count: usize) {
    out.extend_from_slice(&length(count).to_le_bytes());
}

/// Appends a length and the UTF-8 bytes of `text`.
pub fn push_text(out: &mut Vec<u8>, text: &str) {
    push_count(out, text.len());
    out.extend_from_slice(text.as_bytes());
}
