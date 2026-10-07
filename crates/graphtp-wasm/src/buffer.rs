// SPDX-License-Identifier: MIT
/*
 * graphtp-wasm buffer: the memory the host shares with the module, buffers
 * the module owns and names by their address.
 *
 * No analogue in the reference.
 */

//! The memory the host shares with the module.
//!
//! The host asks for a buffer of a length and writes its bytes at the address
//! it receives, and a later call names the buffer by that address and a length
//! within it. The module finds the buffer among those it holds and reads it
//! through the vector that owns it, so no address is ever turned back into a
//! pointer. Every call that answers bytes puts them into the answer buffer,
//! whose address the host reads after the call, since a call may move it.
//!
//! An address is the one of the module's linear memory, which fits a `u32` on
//! `wasm32-unknown-unknown`. On a host whose addresses pass `u32` no buffer is
//! made and the answer buffer reads at address 0.

#![forbid(unsafe_code)]

/// The longest buffer [`Buffers::alloc`] fills through `resize`, where the
/// stores cost less than the second allocation of a longer buffer.
const FILLED: usize = 64;

/// The buffers the module holds for the host.
#[derive(Debug, Default)]
pub struct Buffers {
    /// The buffers the host writes into, each with its address.
    held: Vec<(u32, Vec<u8>)>,
    /// The bytes the last call answered.
    answer: Vec<u8>,
}

impl Buffers {
    /// Returns an empty set of buffers.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            held: Vec::new(),
            answer: Vec::new(),
        }
    }

    /// Makes a buffer of `len` zero bytes and returns its address, 0 when the
    /// allocator refuses it or the address does not fit a `u32`.
    ///
    /// A buffer of no byte still owns one byte, so its address is unique. The
    /// buffer is reserved through `try_reserve_exact` first, so a length the
    /// allocator refuses answers 0, which the glue reports as no memory, where
    /// `vec!` alone would abort the module. A buffer of at most `FILLED` bytes
    /// is then filled through `resize`, one store per byte under the size
    /// profile of the module, and a longer one is dropped and made again
    /// through `vec!`, which takes it zeroed from the allocator in one
    /// `memory.fill`. The second allocation takes the chunk the first freed,
    /// since dlmalloc, the allocator of the standard library on wasm32, gives
    /// no memory back and serves a request from a freed chunk before it grows
    /// the memory.
    ///
    /// [the source of dlmalloc 0.2.11]: https://docs.rs/crate/dlmalloc/0.2.11/source/
    // A buffer the host fills could skip its zero fill, about 0.75 of the 2.1 microseconds of a put of 64 KiB.
    pub fn alloc(&mut self, len: u32) -> u32 {
        let Ok(len) = usize::try_from(len) else {
            return 0;
        };
        let len = len.max(1);
        let mut bytes = Vec::new();
        if bytes.try_reserve_exact(len).is_err() || self.held.try_reserve(1).is_err() {
            return 0;
        }
        if len <= FILLED {
            bytes.resize(len, 0);
        } else {
            drop(bytes);
            bytes = vec![0; len];
        }
        let addr = address(&mut bytes);
        if addr == 0 {
            return 0;
        }
        self.held.push((addr, bytes));
        addr
    }

    /// Drops the buffer at `addr`, and tells whether the module held one.
    pub fn free(&mut self, addr: u32) -> bool {
        match self.held.iter().position(|(held, _)| *held == addr) {
            Some(index) => {
                self.held.swap_remove(index);
                true
            }
            None => false,
        }
    }

    /// Returns the first `len` bytes of the buffer at `addr`, the empty slice
    /// for a `len` of 0, or `None` where the module holds no such buffer or it
    /// is shorter.
    #[must_use]
    pub fn get(&self, addr: u32, len: u32) -> Option<&[u8]> {
        if len == 0 {
            return Some(&[]);
        }
        let len = usize::try_from(len).ok()?;
        let (_, bytes) = self.held.iter().find(|(held, _)| *held == addr)?;
        bytes.get(..len)
    }

    /// Returns the answer buffer, emptied, for a call to write its answer
    /// into.
    pub fn answer_mut(&mut self) -> &mut Vec<u8> {
        self.answer.clear();
        &mut self.answer
    }

    /// Returns the address of the answer buffer.
    #[must_use]
    pub fn answer_addr(&mut self) -> u32 {
        address(&mut self.answer)
    }

    /// Returns the length of the answer buffer as the exports answer it.
    #[must_use]
    pub fn answer_len(&self) -> i32 {
        i32::try_from(self.answer.len()).unwrap_or(i32::MAX)
    }
}

/// Returns the address of the bytes of `bytes`, with a provenance that
/// permits writes exposed to the host that writes there, or 0 where it does
/// not fit a `u32`.
fn address(bytes: &mut Vec<u8>) -> u32 {
    u32::try_from(bytes.as_mut_ptr().expose_provenance()).unwrap_or(0)
}
