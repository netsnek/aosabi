// SPDX-License-Identifier: MIT
/*
 * homa stream cobs: consistent overhead byte stuffing of the delimited form.
 *
 * The delimited form of AoFrame takes it.
 */

//! COBS, which removes every zero byte from a frame.
//!
//! The input is cut into blocks of at most 254 bytes that end at a zero byte
//! or at the end of the input, and each block becomes a code byte, one more
//! than its count of bytes that are not zero, followed by those bytes. A code
//! of 255 means 254 bytes with no zero after them. A block of 254 bytes that
//! ends the input is written without the code 1 of an empty block after it,
//! the shorter of the two endings the scheme allows, and the decoder reads
//! both endings the same.

/// Returns the most bytes the encoding of `len` bytes takes.
pub(crate) const fn max_len(len: usize) -> usize {
    len.saturating_add(len / 254).saturating_add(1)
}

/// A COBS encoder that writes into an output slice as the input arrives.
#[derive(Debug)]
pub(crate) struct Encoder<'o> {
    /// The output.
    out: &'o mut [u8],
    /// The position of the code byte of the open block, `None` right after a
    /// block of 254 bytes closed.
    code_at: Option<usize>,
    /// One more than the bytes of the open block.
    code: u8,
    /// The position the next byte goes to.
    at: usize,
}

impl<'o> Encoder<'o> {
    /// Returns an encoder that writes into `out` from `start` on, or `None`
    /// when `out` has no room for the first code byte.
    pub(crate) fn new(out: &'o mut [u8], start: usize) -> Option<Self> {
        if start >= out.len() {
            return None;
        }
        Some(Self {
            out,
            code_at: Some(start),
            code: 1,
            at: start + 1,
        })
    }

    /// Encodes every byte of `bytes`, or returns `None` when the output runs
    /// out of room.
    pub(crate) fn push_all(&mut self, bytes: &[u8]) -> Option<()> {
        for &byte in bytes {
            self.push(byte)?;
        }
        Some(())
    }

    /// Encodes one byte, or returns `None` when the output runs out of room.
    fn push(&mut self, byte: u8) -> Option<()> {
        if self.code_at.is_none() {
            self.code_at = Some(self.at);
            self.reserve()?;
        }
        let code_at = self.code_at?;
        if byte == 0 {
            *self.out.get_mut(code_at)? = self.code;
            self.code = 1;
            self.code_at = Some(self.at);
            return self.reserve();
        }
        *self.out.get_mut(self.at)? = byte;
        self.at += 1;
        self.code += 1;
        if self.code == 0xFF {
            *self.out.get_mut(code_at)? = self.code;
            self.code = 1;
            self.code_at = None;
        }
        Some(())
    }

    /// Takes one byte of the output for a code byte written later.
    fn reserve(&mut self) -> Option<()> {
        if self.at >= self.out.len() {
            return None;
        }
        self.at += 1;
        Some(())
    }

    /// Closes the last block and returns the position after the encoding.
    pub(crate) fn finish(self) -> Option<usize> {
        if let Some(code_at) = self.code_at {
            *self.out.get_mut(code_at)? = self.code;
        }
        Some(self.at)
    }
}

/// Decodes the first `len` bytes of `buf` in place and returns the length of
/// the decoded bytes, or `None` when a code byte is zero or runs past the end.
///
/// A decoded block is never longer than its encoding, so every byte is
/// written at or before the byte it is read from.
pub(crate) fn decode_in_place(buf: &mut [u8], len: usize) -> Option<usize> {
    let len = len.min(buf.len());
    let mut read = 0;
    let mut write = 0;
    while read < len {
        let code = usize::from(buf[read]);
        if code == 0 {
            return None;
        }
        read += 1;
        let end = read + code - 1;
        if end > len {
            return None;
        }
        buf.copy_within(read..end, write);
        write += end - read;
        read = end;
        if code != 0xFF && read < len {
            buf[write] = 0;
            write += 1;
        }
    }
    Some(write)
}
