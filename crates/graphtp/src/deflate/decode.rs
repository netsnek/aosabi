// SPDX-License-Identifier: MIT
/*
 * graphtp deflate decode: the DEFLATE decoder, whole into the caller's buffer
 * or streaming with the caller's window, each with a bound.
 *
 * No analogue in the reference. The window and the tables it holds are those of
 * the specification.
 */

//! The DEFLATE decoder, whole into the caller's buffer or streaming with the
//! caller's window, each with a bound.
//!
//! The input is one whole slice, as Homa delivers a whole message. The plain
//! bytes go into memory the caller owns, and the two ways differ in what that
//! memory holds.
//!
//! - [`Decoder::whole`] writes every plain byte into one buffer and copies a
//!   match from anywhere before it, so it decodes a stream whatever the window
//!   its header names and needs no window of its own.
//! - [`Decoder::streaming`] keeps the last plain bytes in a window, a ring the
//!   caller gives, and hands the plain bytes to a [`Sink`] each time the ring
//!   fills, so its memory is the window whatever the length of the message. A
//!   match that reaches further back than the window is corrupt there.
//!
//! Both stop at the bound, as soon as the plain bytes would pass it, so a
//! stream that expands a thousand fold costs the receiver its bound and no
//! more. The decoder allocates nothing: its tables stand on the stack, two
//! tables of 608 bytes and 320 bytes of code lengths.

use core::{
    error,
    fmt, //
};

use crate::{
    deflate::huffman::{
        self,
        Table, //
    },
    sink::Sink, //
};

/// Why a DEFLATE stream was not decoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Error {
    /// The plain bytes pass the bound, or the room the caller gave them.
    Bound,
    /// The stream is no DEFLATE data: it ends early, holds a block of type 3,
    /// a stored length whose complement fails, a code that is over subscribed
    /// or incomplete, a symbol no code has, or a distance before the start of
    /// the plain bytes or beyond the window.
    Corrupt,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Error::Bound => "the plain bytes pass the bound",
            Error::Corrupt => "the stream is no valid DEFLATE data",
        })
    }
}

impl error::Error for Error {}

/// What one decoding read and wrote.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Decoded {
    /// The count of input bytes the DEFLATE data takes, up to and with the
    /// byte that holds the last bit of its last block.
    pub consumed: usize,
    /// The count of plain bytes the data decodes to.
    pub produced: usize,
}

/// The DEFLATE decoder with its bound on the plain bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Decoder {
    /// The largest count of plain bytes a decoding may produce.
    bound: usize,
}

impl Decoder {
    /// Returns the decoder that produces at most `bound` plain bytes.
    #[must_use]
    pub const fn new(bound: usize) -> Decoder {
        Decoder { bound }
    }

    /// Returns the bound on the plain bytes.
    #[must_use]
    pub const fn bound(&self) -> usize {
        self.bound
    }

    /// Decodes the DEFLATE data at the start of `input` into `buffer`, from its
    /// start.
    ///
    /// The plain bytes are `buffer[..produced]` of the result. A match copies
    /// from anywhere before it in `buffer`, so the window of the stream plays
    /// no part. The bytes of `input` after the data are left to the caller,
    /// such as the Adler-32 of a zlib stream.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Bound`] as soon as the plain bytes would pass the bound
    /// or the length of `buffer`, and [`Error::Corrupt`] when the data is no
    /// DEFLATE data.
    pub fn whole(&self, input: &[u8], buffer: &mut [u8]) -> Result<Decoded, Error> {
        let mut out = Out {
            window: buffer,
            at: 0,
            flushed: 0,
            produced: 0,
            bound: self.bound,
            sink: None,
        };
        run(input, &mut out)
    }

    /// Decodes the DEFLATE data at the start of `input` through the ring
    /// `window` into `sink`.
    ///
    /// Each time the ring fills, its bytes go into `sink`, and the rest goes at
    /// the end. A window of 4,096 bytes decodes every stream of the zlib
    /// profile of `docs/graphtp.md` section 3.2, whose distances reach back
    /// 4,096 bytes at the most. The bytes of `input` after the data are left to
    /// the caller.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Bound`] as soon as the plain bytes would pass the bound
    /// or when `sink` has no room for them, and [`Error::Corrupt`] when the
    /// data is no DEFLATE data or a match reaches further back than `window`.
    pub fn streaming(
        &self,
        input: &[u8],
        window: &mut [u8],
        sink: &mut dyn Sink,
    ) -> Result<Decoded, Error> {
        let mut out = Out {
            window,
            at: 0,
            flushed: 0,
            produced: 0,
            bound: self.bound,
            sink: Some(sink),
        };
        run(input, &mut out)
    }
}

/// Decodes every block of the data at the start of `input` into `out`.
fn run(input: &[u8], out: &mut Out<'_>) -> Result<Decoded, Error> {
    let mut bits = Bits {
        input,
        at: 0,
        buffer: 0,
        count: 0,
    };
    loop {
        let last = bits.take(1)?;
        match bits.take(2)? {
            0 => stored(&mut bits, out)?,
            1 => fixed(&mut bits, out)?,
            2 => dynamic(&mut bits, out)?,
            _ => return Err(Error::Corrupt),
        }
        if last == 1 {
            break;
        }
    }
    out.flush()?;
    Ok(Decoded {
        consumed: bits.at,
        produced: out.produced,
    })
}

/// The bits of the input, read from the least significant bit of each byte.
struct Bits<'a> {
    /// The whole input.
    input: &'a [u8],
    /// The index of the next byte to read.
    at: usize,
    /// The bits read and not yet taken, the next one lowest.
    buffer: u32,
    /// The count of bits in `buffer`, always below 8 between two takes.
    count: u32,
}

impl Bits<'_> {
    /// Takes the next `n` bits, the first of them lowest, for `n` of 0 to 16.
    fn take(&mut self, n: u32) -> Result<u32, Error> {
        let mut value = self.buffer;
        while self.count < n {
            let byte = *self.input.get(self.at).ok_or(Error::Corrupt)?;
            self.at += 1;
            value |= u32::from(byte) << self.count;
            self.count += 8;
        }
        self.buffer = value >> n;
        self.count -= n;
        Ok(value & ((1 << n) - 1))
    }

    /// Takes one symbol of the code of `table`.
    ///
    /// The code is read one bit at a time. At each length the codes of that
    /// length are the `count` values from `first` on, so a code below
    /// `first + count` is the symbol at its offset in the canonical order.
    fn symbol(&mut self, table: &Table) -> Result<usize, Error> {
        let mut code = 0usize;
        let mut first = 0usize;
        let mut index = 0usize;
        for len in 1..=usize::from(huffman::MAX_BITS) {
            code |= usize::from(self.take(1)? == 1);
            let count = usize::from(table.count(len));
            if code < first + count {
                return table
                    .symbol(index + code - first)
                    .map(usize::from)
                    .ok_or(Error::Corrupt);
            }
            index += count;
            first = (first + count) << 1;
            code <<= 1;
        }
        Err(Error::Corrupt)
    }
}

/// Where the plain bytes go: a buffer that holds them all, or a ring that
/// hands them to a sink as it fills.
struct Out<'a> {
    /// The buffer or the ring.
    window: &'a mut [u8],
    /// The index in `window` of the next plain byte.
    at: usize,
    /// The index in `window` up to which the bytes are in the sink.
    flushed: usize,
    /// The count of plain bytes written.
    produced: usize,
    /// The largest count of plain bytes.
    bound: usize,
    /// The sink of a ring, and none for a buffer that holds every byte.
    sink: Option<&'a mut dyn Sink>,
}

impl Out<'_> {
    /// Appends one plain byte.
    fn put(&mut self, byte: u8) -> Result<(), Error> {
        if self.produced >= self.bound {
            return Err(Error::Bound);
        }
        if self.at == self.window.len() {
            if self.sink.is_none() {
                return Err(Error::Bound);
            }
            self.flush()?;
            self.at = 0;
            self.flushed = 0;
        }
        *self.window.get_mut(self.at).ok_or(Error::Bound)? = byte;
        self.at += 1;
        self.produced += 1;
        Ok(())
    }

    /// Appends `len` bytes copied from `dist` bytes back.
    ///
    /// The copy runs byte by byte, so a match may overlap the bytes it writes,
    /// which repeats a short pattern, as RFC 1951 section 3.2.3 requires.
    // Chunked `unsafe` copies would be faster.
    fn copy(&mut self, dist: usize, len: usize) -> Result<(), Error> {
        let size = self.window.len();
        if dist == 0 || dist > self.produced || dist > size {
            return Err(Error::Corrupt);
        }
        let mut from = if self.at >= dist {
            self.at - dist
        } else {
            self.at + size - dist
        };
        for _ in 0..len {
            let byte = *self.window.get(from).ok_or(Error::Corrupt)?;
            self.put(byte)?;
            from += 1;
            if from == size {
                from = 0;
            }
        }
        Ok(())
    }

    /// Hands the bytes of the ring that the sink has not yet taken to the
    /// sink, and nothing for a buffer that holds every byte.
    fn flush(&mut self) -> Result<(), Error> {
        let Some(sink) = self.sink.as_deref_mut() else {
            return Ok(());
        };
        let bytes = self
            .window
            .get(self.flushed..self.at)
            .ok_or(Error::Corrupt)?;
        sink.put(bytes).map_err(|_| Error::Bound)?;
        self.flushed = self.at;
        Ok(())
    }
}

/// Decodes a stored block, whose bytes stand in the input from the next byte
/// boundary behind a length and its complement.
///
/// The bytes go out in the order they stand, and a block the input cuts short
/// is corrupt where its bytes run out, so a block that passes the bound first
/// answers the bound, as a decoder does that meets the bytes as they come.
fn stored(bits: &mut Bits<'_>, out: &mut Out<'_>) -> Result<(), Error> {
    // The bits left in the buffer are the rest of the current byte, since a
    // take never reads a byte more than it needs.
    bits.buffer = 0;
    bits.count = 0;
    let at = bits.at;
    let Some(&[low, high, not_low, not_high]) = bits.input.get(at..at + 4) else {
        return Err(Error::Corrupt);
    };
    let len = u16::from_le_bytes([low, high]);
    if len != !u16::from_le_bytes([not_low, not_high]) {
        return Err(Error::Corrupt);
    }
    let len = usize::from(len);
    let data = bits.input.get(at + 4..).unwrap_or_default();
    for &byte in data.iter().take(len) {
        out.put(byte)?;
    }
    if data.len() < len {
        return Err(Error::Corrupt);
    }
    bits.at = at + 4 + len;
    Ok(())
}

/// Decodes a block of the fixed code of RFC 1951 section 3.2.6.
fn fixed(bits: &mut Bits<'_>, out: &mut Out<'_>) -> Result<(), Error> {
    let mut lengths = [0u8; huffman::FIXED_LITERALS + huffman::DISTANCES];
    let (literal, distance) = lengths.split_at_mut(huffman::FIXED_LITERALS);
    for (symbol, len) in literal.iter_mut().enumerate() {
        *len = huffman::fixed_literal_length(symbol);
    }
    distance.fill(huffman::FIXED_DISTANCE_LENGTH);
    let (literal, _) = Table::new(literal);
    let (distance, _) = Table::new(distance);
    codes(bits, out, &literal, &distance)
}

/// Decodes a block of a dynamic code, whose lengths travel coded by the code
/// length code in front of the data.
fn dynamic(bits: &mut Bits<'_>, out: &mut Out<'_>) -> Result<(), Error> {
    let literals = bits.take(5)? as usize + 257;
    let distances = bits.take(5)? as usize + 1;
    let code_lengths = bits.take(4)? as usize + 4;
    if literals > huffman::LITERALS || distances > huffman::DISTANCES {
        return Err(Error::Corrupt);
    }
    let mut lengths = [0u8; huffman::LITERALS + huffman::DISTANCES + 4];
    for &symbol in &huffman::ORDER[..code_lengths] {
        lengths[usize::from(symbol)] = u8::try_from(bits.take(3)?).unwrap_or(0);
    }
    let (code, left) = Table::new(&lengths[..huffman::ORDER.len()]);
    if left != 0 {
        return Err(Error::Corrupt);
    }
    let total = literals + distances;
    let mut index = 0;
    while index < total {
        let symbol = bits.symbol(&code)?;
        if symbol < 16 {
            lengths[index] = u8::try_from(symbol).unwrap_or(0);
            index += 1;
            continue;
        }
        let (value, repeat) = match symbol {
            16 => {
                let previous = *lengths
                    .get(index.wrapping_sub(1))
                    .filter(|_| index > 0)
                    .ok_or(Error::Corrupt)?;
                (previous, 3 + bits.take(2)? as usize)
            }
            17 => (0, 3 + bits.take(3)? as usize),
            _ => (0, 11 + bits.take(7)? as usize),
        };
        let run = lengths
            .get_mut(index..index + repeat)
            .filter(|_| index + repeat <= total)
            .ok_or(Error::Corrupt)?;
        run.fill(value);
        index += repeat;
    }
    if lengths[huffman::END_OF_BLOCK] == 0 {
        return Err(Error::Corrupt);
    }
    let literal_lengths = &lengths[..literals];
    let (literal, left) = Table::new(literal_lengths);
    if left < 0 || (left > 0 && !literal.is_single(literal_lengths)) {
        return Err(Error::Corrupt);
    }
    let distance_lengths = &lengths[literals..total];
    let (distance, left) = Table::new(distance_lengths);
    if left < 0 || (left > 0 && !distance.is_single(distance_lengths)) {
        return Err(Error::Corrupt);
    }
    codes(bits, out, &literal, &distance)
}

/// Decodes the literals and matches of one block up to its end of block symbol.
fn codes(
    bits: &mut Bits<'_>,
    out: &mut Out<'_>,
    literal: &Table,
    distance: &Table,
) -> Result<(), Error> {
    loop {
        let symbol = bits.symbol(literal)?;
        if symbol < huffman::END_OF_BLOCK {
            out.put(u8::try_from(symbol).unwrap_or(0))?;
            continue;
        }
        if symbol == huffman::END_OF_BLOCK {
            return Ok(());
        }
        let index = symbol - 257;
        let (Some(&base), Some(&extra)) = (
            huffman::LENGTH_BASE.get(index),
            huffman::LENGTH_EXTRA.get(index),
        ) else {
            return Err(Error::Corrupt);
        };
        let len = usize::from(base) + bits.take(u32::from(extra))? as usize;
        let index = bits.symbol(distance)?;
        let (Some(&base), Some(&extra)) = (
            huffman::DISTANCE_BASE.get(index),
            huffman::DISTANCE_EXTRA.get(index),
        ) else {
            return Err(Error::Corrupt);
        };
        let dist = usize::from(base) + bits.take(u32::from(extra))? as usize;
        out.copy(dist, len)?;
    }
}
