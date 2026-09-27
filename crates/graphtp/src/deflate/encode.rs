// SPDX-License-Identifier: MIT
/*
 * graphtp deflate encode: the DEFLATE encoder, LZ77 over hash chains with one
 * step of lazy matching, in memory the caller owns.
 *
 * No analogue in the reference. The encoder follows RFC 1951, its hash chains
 * and its one step of lazy matching are those of the plan, and the memory it
 * holds is that of the specification. The buffer of plain bytes with its
 * lookahead of 262 bytes and the rule that a block is stored only while its
 * plain bytes stand in that buffer are those of zlib-ng.
 */

//! The DEFLATE encoder, which writes each block stored, fixed or dynamic,
//! whichever is shortest.
//!
//! The encoder streams: [`Encoder::write`] takes the plain bytes in pieces of
//! any size and puts the coded bytes into a [`Sink`] as its blocks close, and
//! [`Encoder::finish`] writes the last block. Its memory is a [`Scratch`] the
//! caller owns and lends to one stream at a time, so an encoder allocates
//! nothing while it codes. The scratch holds four parts, each sized by
//! [`Params`]:
//!
//! - The hash heads, 2 bytes each, the newest position of each hash of 4 bytes,
//! - The chains, 2 bytes per byte of the window, the position before each
//!   position with the same hash,
//! - The tokens of the open block, 3 bytes each, a literal or a match,
//! - The buffer of plain bytes, the window behind the next byte to code and
//!   the lookahead in front of it.
//!
//! A match is the longest found along at most 128 links of the chain, each
//! candidate compared a word of 8 bytes at a time, or of 4 on a core of 32
//! bits, and a match found one byte later wins when it is longer, which is the
//! one step of lazy matching. A block closes when it holds [`Params::tokens`]
//! tokens, and it is written stored, with the fixed code or with a dynamic
//! code, whichever takes the fewest bits. A block is stored only while its
//! plain bytes stand in the buffer, so when the buffer slides past the start of
//! a block whose bytes would be shortest stored, the block closes there.

use alloc::vec::Vec;
use core::{
    error,
    fmt, //
};

use crate::{
    deflate::huffman::{
        self,
        DISTANCES,
        END_OF_BLOCK,
        FIXED_LITERALS,
        LITERALS,
        MAX_MATCH,
        MIN_MATCH, //
    },
    sink::{
        Full,
        Sink, //
    },
};

/// The smallest window, 512 bytes, as the base 2 logarithm.
pub const MIN_WINDOW_BITS: u8 = 9;

/// The largest window, 32,768 bytes, as the base 2 logarithm.
pub const MAX_WINDOW_BITS: u8 = 15;

/// The plain bytes the encoder keeps in front of the byte it codes, the longest
/// match and the 4 bytes zlib-ng keeps beyond it, 262 bytes.
pub const LOOKAHEAD: usize = MAX_MATCH + MIN_MATCH + 1;

/// The count of hash heads of [`Params::new`], 4,096.
pub const DEFAULT_HEADS: usize = 4096;

/// The count of tokens per block of [`Params::new`], 8,192.
pub const DEFAULT_TOKENS: usize = 8192;

/// The fewest hash heads, 256.
const MIN_HEADS: usize = 256;

/// The most hash heads, 65,536, since a chain holds a position in 2 bytes.
const MAX_HEADS: usize = 65_536;

/// The most tokens per block, 65,535.
const MAX_TOKENS: usize = 65_535;

/// The largest buffer of plain bytes, 1 MiB.
const MAX_BUFFER: usize = 1 << 20;

/// The bytes of one token in the scratch.
const TOKEN: usize = 3;

/// The links of a chain the search follows at the most.
const CHAIN: usize = 128;

/// The length of a match that ends the search and the lazy step, 128 bytes.
const NICE: usize = 128;

/// The plain bytes the hash of a position reads, 4.
const HASHED: usize = 4;

/// The multiplier of the hash of 4 bytes, the golden ratio in 32 bits, whose
/// high bits spread the bytes over the heads.
const HASH: u32 = 0x9E37_79B1;

/// The longest stored block, 65,535 bytes.
const MAX_STORED: usize = 65_535;

/// The bytes the encoder gathers before it puts them into the sink.
const STAGE: usize = 64;

/// The integer the longest match compares at a time, 64 bits where the target
/// computes in 64 bits, a host of 64 bits and `wasm32`.
#[cfg(any(target_pointer_width = "64", target_family = "wasm"))]
type Word = u64;

/// The integer the longest match compares at a time, 32 bits on a core of 32
/// bits, the ESP32-C3 and the player.
#[cfg(not(any(target_pointer_width = "64", target_family = "wasm")))]
type Word = u32;

/// The bytes of a [`Word`].
const WORD: usize = core::mem::size_of::<Word>();

/// The sizes of the memory of an encoder.
///
/// The window decides how far back a match reaches and the zlib header that
/// announces it, and the three other sizes trade memory against the ratio.
/// A set of sizes is valid when the window bits are 9 to 15, the heads a power
/// of two from 256 to 65,536, the tokens 1 to 65,535, and the buffer at least
/// the window and the lookahead of 262 bytes and at most 1 MiB.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Params {
    /// The base 2 logarithm of the window, 9 to 15.
    pub window_bits: u8,
    /// The count of hash heads.
    pub heads: usize,
    /// The count of tokens after which a block closes.
    pub tokens: usize,
    /// The count of plain bytes the buffer holds.
    pub buffer: usize,
}

impl Params {
    /// Returns the sizes of a window of `window_bits` with 4,096 hash heads,
    /// blocks of 8,192 tokens and a buffer of twice the window.
    ///
    /// The heads and the tokens are those of the prototype of the encoder, and
    /// the buffer of twice the window is the one zlib-ng slides over.
    #[must_use]
    pub const fn new(window_bits: u8) -> Params {
        let window = match 1usize.checked_shl(window_bits as u32) {
            Some(window) => window,
            None => 0,
        };
        Params {
            window_bits,
            heads: DEFAULT_HEADS,
            tokens: DEFAULT_TOKENS,
            buffer: window.saturating_mul(2),
        }
    }

    /// Returns the size of the window in bytes, or 0 for window bits outside 9
    /// to 15.
    #[must_use]
    pub fn window(&self) -> usize {
        if (MIN_WINDOW_BITS..=MAX_WINDOW_BITS).contains(&self.window_bits) {
            1 << self.window_bits
        } else {
            0
        }
    }

    /// Tells whether the sizes are valid.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        let window = self.window();
        window != 0
            && self.heads.is_power_of_two()
            && (MIN_HEADS..=MAX_HEADS).contains(&self.heads)
            && (1..=MAX_TOKENS).contains(&self.tokens)
            && (window + LOOKAHEAD..=MAX_BUFFER).contains(&self.buffer)
    }

    /// Returns the bytes a scratch of these sizes holds: 2 per hash head, 2
    /// per byte of the window, 3 per token and 1 per byte of the buffer.
    #[must_use]
    pub fn bytes(&self) -> usize {
        2 * self.heads + 2 * self.window() + TOKEN * self.tokens + self.buffer
    }
}

/// Why a scratch was not made.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ScratchError {
    /// The sizes are not valid.
    Params,
    /// The allocator refused the memory.
    Memory,
}

impl fmt::Display for ScratchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ScratchError::Params => "the sizes of the scratch are not valid",
            ScratchError::Memory => "the allocator refused the memory of the scratch",
        })
    }
}

impl error::Error for ScratchError {}

/// The memory of an encoder: the hash heads, the chains, the tokens of a block
/// and the buffer of plain bytes.
///
/// A scratch is made once, and each [`Encoder`] borrows it for one stream, so
/// the memory of a sender is the scratch whatever the count of its messages.
pub struct Scratch {
    /// The sizes the memory was made with.
    params: Params,
    /// The newest position of each hash, modulo 65,536.
    head: Vec<u16>,
    /// The position before each position of the window with the same hash.
    chain: Vec<u16>,
    /// The tokens of the open block.
    tokens: Vec<u8>,
    /// The plain bytes of the window and the lookahead.
    buffer: Vec<u8>,
}

impl Scratch {
    /// Returns the scratch of the sizes `params`.
    ///
    /// # Errors
    ///
    /// Returns [`ScratchError::Params`] when the sizes are not valid, and
    /// [`ScratchError::Memory`] when the allocator refuses the memory.
    pub fn new(params: Params) -> Result<Scratch, ScratchError> {
        if !params.is_valid() {
            return Err(ScratchError::Params);
        }
        Ok(Scratch {
            params,
            head: zeroed(params.heads)?,
            chain: zeroed(params.window())?,
            tokens: zeroed(TOKEN * params.tokens)?,
            buffer: zeroed(params.buffer)?,
        })
    }

    /// Returns the sizes of the scratch.
    #[must_use]
    pub fn params(&self) -> Params {
        self.params
    }

    /// Returns the bytes the scratch holds.
    #[must_use]
    pub fn bytes(&self) -> usize {
        2 * self.head.len() + 2 * self.chain.len() + self.tokens.len() + self.buffer.len()
    }
}

/// Returns a vector of `len` zero values, or [`ScratchError::Memory`] when the
/// allocator refuses the room.
// Storage left uninitialized would skip this fill.
fn zeroed<T: Copy + Default>(len: usize) -> Result<Vec<T>, ScratchError> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(len)
        .map_err(|_| ScratchError::Memory)?;
    values.resize(len, T::default());
    Ok(values)
}

/// A match, a length of 3 to 258 bytes at a distance, or none with the length
/// 0.
#[derive(Clone, Copy)]
struct Match {
    /// The count of bytes that match, 0 for none.
    len: usize,
    /// How far back the match starts.
    dist: usize,
}

/// No match.
const NONE: Match = Match { len: 0, dist: 0 };

/// The streaming DEFLATE encoder over a scratch the caller owns.
///
/// The output is raw DEFLATE data of RFC 1951. A write that meets a sink with
/// no room returns [`Full`] and leaves the stream incomplete, so the encoder
/// answers every later call with [`Full`] as well.
pub struct Encoder<'a> {
    /// The memory of the encoder.
    scratch: &'a mut Scratch,
    /// The bits and whole bytes not yet in the sink.
    out: Writer,
    /// The count of plain bytes in the buffer.
    end: usize,
    /// The index in the buffer of the next plain byte to code.
    pos: usize,
    /// The position in the stream of the first byte of the buffer, modulo
    /// 65,536, which is how the chains name a position.
    base: u16,
    /// The index in the buffer of the first plain byte of the open block, or
    /// none once the buffer has slid past it.
    block: Option<usize>,
    /// The count of tokens in the open block.
    count: usize,
    /// The match found one byte later by the lazy step, not yet a token.
    pending: Option<Match>,
    /// Whether a sink had no room, which leaves the stream incomplete.
    failed: bool,
}

impl<'a> Encoder<'a> {
    /// Returns the encoder of a new stream over the memory `scratch`.
    ///
    /// The hash heads and the chains start empty, so a stream codes to the
    /// same bytes whatever streams the scratch served before.
    #[must_use]
    pub fn new(scratch: &'a mut Scratch) -> Encoder<'a> {
        Encoder::with_prefix(scratch, &[])
    }

    /// Returns the encoder of a new stream whose output opens with the bytes
    /// `prefix`, at most 64 of them, such as a zlib header.
    pub(crate) fn with_prefix(scratch: &'a mut Scratch, prefix: &[u8]) -> Encoder<'a> {
        scratch.head.fill(0);
        scratch.chain.fill(0);
        let mut out = Writer {
            acc: 0,
            bits: 0,
            stage: [0; STAGE],
            len: 0,
        };
        for (slot, &byte) in out.stage.iter_mut().zip(prefix) {
            *slot = byte;
            out.len += 1;
        }
        Encoder {
            scratch,
            out,
            end: 0,
            pos: 0,
            base: 0,
            block: Some(0),
            count: 0,
            pending: None,
            failed: false,
        }
    }

    /// Codes the plain bytes `plain` and puts the coded bytes of every block
    /// that closes into `sink`.
    ///
    /// # Errors
    ///
    /// Returns [`Full`] when `sink` has no room, and on every call after that.
    pub fn write(&mut self, plain: &[u8], sink: &mut dyn Sink) -> Result<(), Full> {
        if self.failed {
            return Err(Full);
        }
        let written = self.take(plain, sink);
        self.failed = written.is_err();
        written
    }

    /// Codes the rest of the plain bytes, writes the last block and puts every
    /// coded byte into `sink`, the stream ending on a byte boundary.
    ///
    /// # Errors
    ///
    /// Returns [`Full`] when `sink` has no room or an earlier call met a sink
    /// with no room.
    pub fn finish(mut self, sink: &mut dyn Sink) -> Result<(), Full> {
        if self.failed {
            return Err(Full);
        }
        self.code(true, sink)?;
        self.close_block(true, false, sink)?;
        self.out.align(sink)?;
        self.out.drain(sink)
    }

    /// Moves the plain bytes `plain` into the buffer and codes them as far as
    /// the lookahead allows.
    fn take(&mut self, plain: &[u8], sink: &mut dyn Sink) -> Result<(), Full> {
        let mut rest = plain;
        while !rest.is_empty() {
            if self.end == self.scratch.buffer.len() {
                self.slide(sink)?;
            }
            let free = self.scratch.buffer.get_mut(self.end..).unwrap_or_default();
            if free.is_empty() {
                return Err(Full);
            }
            let (now, later) = rest.split_at(free.len().min(rest.len()));
            if let Some(slots) = free.get_mut(..now.len()) {
                slots.copy_from_slice(now);
            }
            self.end += now.len();
            rest = later;
            self.code(false, sink)?;
        }
        Ok(())
    }

    /// Drops the plain bytes more than a window behind the next byte to code
    /// from the buffer.
    ///
    /// The code stops while fewer bytes than the lookahead stand in front of
    /// the next byte, and the buffer holds the window and the lookahead at the
    /// least, so the next byte stands more than a window into the buffer and
    /// the slide frees room. A block whose first bytes the slide drops closes
    /// first when it is shortest stored, since it can no longer be stored
    /// afterwards.
    fn slide(&mut self, sink: &mut dyn Sink) -> Result<(), Full> {
        let shift = self.pos.saturating_sub(self.scratch.params.window());
        if self.count > 0 && self.block.is_some_and(|start| start < shift) {
            self.close_block(false, true, sink)?;
        }
        self.scratch.buffer.copy_within(shift..self.end, 0);
        self.end -= shift;
        self.pos -= shift;
        self.base = self.base.wrapping_add(low16(shift));
        self.block = self.block.and_then(|start| start.checked_sub(shift));
        Ok(())
    }

    /// Turns the plain bytes of the buffer into tokens, up to the lookahead
    /// or, when `last` holds, up to the end, and closes each full block.
    ///
    /// A block that fills with the last plain byte stays open, so the final
    /// block of [`Encoder::finish`] holds its tokens and no empty block follows
    /// it.
    fn code(&mut self, last: bool, sink: &mut dyn Sink) -> Result<(), Full> {
        loop {
            let avail = self.end - self.pos;
            if avail == 0 || (!last && avail < LOOKAHEAD) {
                return Ok(());
            }
            let pos = self.pos;
            let here = match self.pending.take() {
                Some(found) => found,
                None => self.find(pos),
            };
            self.insert(pos);
            if here.len < MIN_MATCH {
                self.literal(pos);
                self.pos += 1;
            } else {
                let next = if here.len < NICE && pos + 1 < self.end {
                    self.find(pos + 1)
                } else {
                    NONE
                };
                if next.len > here.len {
                    self.literal(pos);
                    self.pending = Some(next);
                    self.pos += 1;
                } else {
                    self.token(here);
                    for later in pos + 1..pos + here.len {
                        self.insert(later);
                    }
                    self.pos += here.len;
                }
            }
            if self.count >= self.scratch.params.tokens && !(last && self.pos == self.end) {
                self.close_block(false, false, sink)?;
            }
        }
    }

    /// Returns the hash of the 4 plain bytes at the index `at`, an index of the
    /// hash heads.
    ///
    /// The bytes are read as one word, the first in the lowest lane, so two
    /// positions share a chain when their first 4 bytes are alike, or by a
    /// collision of the hash, and a match of 3 bytes alone is found only where
    /// the hash of its 4 bytes collides.
    fn hash(&self, at: usize) -> usize {
        let Some(&bytes) = self
            .scratch
            .buffer
            .get(at..)
            .and_then(<[u8]>::first_chunk::<HASHED>)
        else {
            return 0;
        };
        let value = u32::from_le_bytes(bytes);
        let bits = self.scratch.params.heads.trailing_zeros();
        usize::try_from(value.wrapping_mul(HASH) >> (32 - bits)).unwrap_or(0)
    }

    /// Enters the position at the index `at` into the chains.
    fn insert(&mut self, at: usize) {
        if at + HASHED > self.end {
            return;
        }
        let hash = self.hash(at);
        let position = self.base.wrapping_add(low16(at));
        let mask = self.scratch.params.window() - 1;
        let Some(head) = self.scratch.head.get_mut(hash) else {
            return;
        };
        let previous = core::mem::replace(head, position);
        if let Some(link) = self.scratch.chain.get_mut(usize::from(position) & mask) {
            *link = previous;
        }
    }

    /// Returns the longest match for the plain bytes at the index `at`, or
    /// [`NONE`] when no match of 3 bytes is found or fewer than the 4 bytes
    /// the hash reads stand at `at`.
    ///
    /// The search follows the chain of the hash from the newest position and
    /// stops at a position no nearer than the one before, beyond the window,
    /// before the start of the buffer, after 128 links, or at a match of 128
    /// bytes. Each candidate whose byte at the length of the best match so far
    /// equals the one at `at` is measured by [`common`].
    fn find(&self, at: usize) -> Match {
        let buffer = self.scratch.buffer.get(..self.end).unwrap_or_default();
        if at + HASHED > buffer.len() {
            return NONE;
        }
        let max = (buffer.len() - at).min(MAX_MATCH);
        let newer = buffer.get(at..at + max).unwrap_or_default();
        let window = self.scratch.params.window();
        let here = self.base.wrapping_add(low16(at));
        let mut candidate = self.scratch.head.get(self.hash(at)).copied().unwrap_or(0);
        let mut best = NONE;
        let mut last = 0;
        for _ in 0..CHAIN {
            let dist = usize::from(here.wrapping_sub(candidate));
            if dist == 0 || dist > window || dist <= last || dist > at {
                break;
            }
            let from = at - dist;
            let probe = best.len.min(max - 1);
            if buffer.get(from + probe) == buffer.get(at + probe) {
                let len = common(buffer.get(from..from + max).unwrap_or_default(), newer);
                if len > best.len {
                    best = Match { len, dist };
                    if len >= NICE.min(max) {
                        break;
                    }
                }
            }
            last = dist;
            candidate = self
                .scratch
                .chain
                .get(usize::from(candidate) & (window - 1))
                .copied()
                .unwrap_or(0);
        }
        if best.len >= MIN_MATCH { best } else { NONE }
    }

    /// Appends the plain byte at the index `at` as a literal token.
    fn literal(&mut self, at: usize) {
        let byte = self.scratch.buffer.get(at).copied().unwrap_or(0);
        self.push([0, 0, byte]);
    }

    /// Appends the match `found` as a token.
    fn token(&mut self, found: Match) {
        let [low, high] = low16(found.dist).to_le_bytes();
        let len = u8::try_from(found.len - MIN_MATCH).unwrap_or(u8::MAX);
        self.push([low, high, len]);
    }

    /// Appends the token `token` to the open block.
    fn push(&mut self, token: [u8; TOKEN]) {
        let at = TOKEN * self.count;
        if let Some(slot) = self.scratch.tokens.get_mut(at..at + TOKEN) {
            slot.copy_from_slice(&token);
            self.count += 1;
        }
    }

    /// Writes the open block as the shortest of stored, fixed and dynamic,
    /// the final block when `last` holds, and opens the next one.
    ///
    /// When `only_stored` holds, the block is written only when stored is the
    /// shortest, and stays open otherwise.
    fn close_block(
        &mut self,
        last: bool,
        only_stored: bool,
        sink: &mut dyn Sink,
    ) -> Result<(), Full> {
        let tokens = self
            .scratch
            .tokens
            .get(..TOKEN * self.count)
            .unwrap_or_default();
        let plain = self
            .block
            .and_then(|start| self.scratch.buffer.get(start..self.pos));
        let block = Block::new(tokens);
        let stored = plain.map(|plain| stored_bits(plain.len(), self.out.bits));
        let stored_wins = stored.is_some_and(|bits| bits < block.dynamic && bits < block.fixed);
        if only_stored && !stored_wins {
            return Ok(());
        }
        let last = u32::from(last);
        match plain {
            Some(plain) if stored_wins => write_stored(&mut self.out, plain, last, sink)?,
            _ if block.fixed <= block.dynamic => {
                self.out.put(last | 2, 3, sink)?;
                write_tokens(&mut self.out, tokens, &Code::fixed(), sink)?;
            }
            _ => {
                self.out.put(last | 4, 3, sink)?;
                block.write_dynamic(&mut self.out, tokens, sink)?;
            }
        }
        self.block = Some(self.pos);
        self.count = 0;
        Ok(())
    }
}

/// Returns the low 16 bits of `value`, a position modulo 65,536.
fn low16(value: usize) -> u16 {
    let [low, high, ..] = value.to_le_bytes();
    u16::from_le_bytes([low, high])
}

/// Returns the count of the first bytes `older` and `newer` hold alike, for
/// two slices of one length.
///
/// The bytes are compared a [`Word`] at a time while whole words remain. The
/// exclusive or of two words read little endian is zero while they are equal,
/// and at the first word that differs its lowest set bit stands in the first
/// lane that differs, so `trailing_zeros` reads the lane once at the
/// mismatch. The bytes after the last whole word are compared one at a time.
// Unaligned `unsafe` loads would be faster. An unaligned word read through a
// raw pointer would read the last bytes as a word too.
#[inline]
fn common(older: &[u8], newer: &[u8]) -> usize {
    let (old_words, _) = older.as_chunks::<WORD>();
    let (new_words, _) = newer.as_chunks::<WORD>();
    for (at, (old, new)) in old_words.iter().zip(new_words).enumerate() {
        let differ = Word::from_le_bytes(*old) ^ Word::from_le_bytes(*new);
        if differ != 0 {
            return at * WORD + (differ.trailing_zeros() / 8) as usize;
        }
    }
    let mut len = old_words.len().min(new_words.len()) * WORD;
    while let (Some(old), Some(new)) = (older.get(len), newer.get(len)) {
        if old != new {
            break;
        }
        len += 1;
    }
    len
}

/// Returns the bits of a stored block of `len` plain bytes written after `bits`
/// bits of a partial byte: the header of 3 bits, the padding to the byte
/// boundary, the length and its complement, and the bytes, with a block of its
/// own for every 65,535 bytes.
fn stored_bits(len: usize, bits: u32) -> usize {
    let pad = (8 - (usize::try_from(bits).unwrap_or(0) + 3) % 8) % 8;
    let blocks = len.div_ceil(MAX_STORED).max(1);
    8 * len + 3 + pad + 32 + (blocks - 1) * 40
}

/// Writes the plain bytes `plain` as stored blocks of at most 65,535 bytes,
/// the last of them final when `last` is 1.
fn write_stored(
    out: &mut Writer,
    plain: &[u8],
    last: u32,
    sink: &mut dyn Sink,
) -> Result<(), Full> {
    let mut rest = plain;
    loop {
        let (now, later) = rest.split_at(rest.len().min(MAX_STORED));
        let final_block = later.is_empty();
        out.put(if final_block { last } else { 0 }, 3, sink)?;
        out.align(sink)?;
        let len = low16(now.len());
        out.put(u32::from(len), 16, sink)?;
        out.put(u32::from(!len), 16, sink)?;
        out.drain(sink)?;
        sink.put(now)?;
        rest = later;
        if final_block {
            return Ok(());
        }
    }
}

/// One token of a block, read from its 3 bytes.
enum Token {
    /// A plain byte.
    Literal(u8),
    /// A match of a length at a distance.
    Copy {
        /// The index of the length symbol, 0 for the symbol 257.
        length: usize,
        /// The extra bits of the length.
        length_extra: u32,
        /// The distance symbol.
        distance: usize,
        /// The extra bits of the distance.
        distance_extra: u32,
    },
}

impl Token {
    /// Reads the token of the 3 bytes `bytes`.
    fn of(bytes: &[u8]) -> Token {
        let [low, high, value] = <[u8; TOKEN]>::try_from(bytes).unwrap_or_default();
        let dist = usize::from(u16::from_le_bytes([low, high]));
        if dist == 0 {
            return Token::Literal(value);
        }
        let len = usize::from(value) + MIN_MATCH;
        let length = huffman::length_symbol(len);
        let distance = huffman::distance_symbol(dist);
        let length_base = huffman::LENGTH_BASE.get(length).copied().unwrap_or(0);
        let distance_base = huffman::DISTANCE_BASE.get(distance).copied().unwrap_or(0);
        Token::Copy {
            length,
            length_extra: u32::from(low16(len - usize::from(length_base))),
            distance,
            distance_extra: u32::from(low16(dist - usize::from(distance_base))),
        }
    }
}

/// The lengths and the bit reversed codes of the literal and length code and
/// of the distance code of one block.
struct Code {
    /// The length of the code of each literal and length symbol.
    literal_lengths: [u8; FIXED_LITERALS],
    /// The code of each literal and length symbol.
    literal_codes: [u16; FIXED_LITERALS],
    /// The length of the code of each distance symbol.
    distance_lengths: [u8; DISTANCES],
    /// The code of each distance symbol.
    distance_codes: [u16; DISTANCES],
}

impl Code {
    /// Returns the code of the lengths `literal_lengths` and
    /// `distance_lengths`.
    fn new(literal_lengths: &[u8; FIXED_LITERALS], distance_lengths: &[u8; DISTANCES]) -> Code {
        let mut code = Code {
            literal_lengths: *literal_lengths,
            literal_codes: [0; FIXED_LITERALS],
            distance_lengths: *distance_lengths,
            distance_codes: [0; DISTANCES],
        };
        huffman::codes_of(&code.literal_lengths, &mut code.literal_codes);
        huffman::codes_of(&code.distance_lengths, &mut code.distance_codes);
        code
    }

    /// Returns the fixed code of RFC 1951 section 3.2.6.
    fn fixed() -> Code {
        let mut literal = [0u8; FIXED_LITERALS];
        for (symbol, len) in literal.iter_mut().enumerate() {
            *len = huffman::fixed_literal_length(symbol);
        }
        Code::new(&literal, &[huffman::FIXED_DISTANCE_LENGTH; DISTANCES])
    }

    /// Returns the bits the tokens `tokens` and the end of block take in this
    /// code, the extra bits included.
    fn cost(&self, tokens: &[u8]) -> usize {
        let mut bits = usize::from(self.literal_lengths[END_OF_BLOCK]);
        for bytes in tokens.chunks_exact(TOKEN) {
            bits += match Token::of(bytes) {
                Token::Literal(byte) => usize::from(self.literal_lengths[usize::from(byte)]),
                Token::Copy {
                    length, distance, ..
                } => {
                    let literal = self.literal_lengths.get(257 + length).copied();
                    let distance_len = self.distance_lengths.get(distance).copied();
                    let length_extra = huffman::LENGTH_EXTRA.get(length).copied();
                    let distance_extra = huffman::DISTANCE_EXTRA.get(distance).copied();
                    [literal, distance_len, length_extra, distance_extra]
                        .into_iter()
                        .map(|bits| usize::from(bits.unwrap_or(0)))
                        .sum()
                }
            };
        }
        bits
    }
}

/// The frequencies and the code of one block and what each way of writing it
/// costs.
struct Block {
    /// The dynamic code the frequencies of the block give.
    code: Code,
    /// The count of literal and length codes the header sends, 257 to 286.
    literals: usize,
    /// The count of distance codes the header sends, 1 to 30.
    distances: usize,
    /// The lengths of the code length code.
    code_lengths: [u8; 19],
    /// The count of code length codes the header sends, 4 to 19.
    code_length_count: usize,
    /// The bits of the block with its dynamic code and its header.
    dynamic: usize,
    /// The bits of the block with the fixed code.
    fixed: usize,
}

impl Block {
    /// Returns the code of the tokens `tokens` and the cost of each way.
    fn new(tokens: &[u8]) -> Block {
        let mut literal_freq = [0usize; LITERALS];
        let mut distance_freq = [0usize; DISTANCES];
        literal_freq[END_OF_BLOCK] = 1;
        for bytes in tokens.chunks_exact(TOKEN) {
            match Token::of(bytes) {
                Token::Literal(byte) => literal_freq[usize::from(byte)] += 1,
                Token::Copy {
                    length, distance, ..
                } => {
                    if let Some(freq) = literal_freq.get_mut(257 + length) {
                        *freq += 1;
                    }
                    if let Some(freq) = distance_freq.get_mut(distance) {
                        *freq += 1;
                    }
                }
            }
        }
        let mut literal_lengths = [0u8; FIXED_LITERALS];
        let mut distance_lengths = [0u8; DISTANCES];
        huffman::lengths_of(
            &literal_freq,
            huffman::MAX_BITS,
            &mut literal_lengths[..LITERALS],
        );
        huffman::lengths_of(&distance_freq, huffman::MAX_BITS, &mut distance_lengths);
        let literals = sent(&literal_lengths[..LITERALS], 257);
        let distances = sent(&distance_lengths, 1);
        let all = joined(&literal_lengths, literals, &distance_lengths, distances);
        let all = all.get(..literals + distances).unwrap_or_default();
        let mut code_freq = [0usize; 19];
        for (symbol, _) in Runs::new(all) {
            code_freq[usize::from(symbol)] += 1;
        }
        let mut code_lengths = [0u8; 19];
        huffman::lengths_of(&code_freq, huffman::MAX_CODE_LENGTH_BITS, &mut code_lengths);
        let code_length_count = huffman::ORDER
            .iter()
            .rposition(|&symbol| code_lengths[usize::from(symbol)] != 0)
            .map_or(4, |at| (at + 1).max(4));
        let runs: usize = Runs::new(all)
            .map(|(symbol, _)| {
                usize::from(code_lengths[usize::from(symbol)]) + usize::from(extra_bits(symbol))
            })
            .sum();
        let code = Code::new(&literal_lengths, &distance_lengths);
        let dynamic = 3 + 14 + 3 * code_length_count + runs + code.cost(tokens);
        let fixed = 3 + Code::fixed().cost(tokens);
        Block {
            code,
            literals,
            distances,
            code_lengths,
            code_length_count,
            dynamic,
            fixed,
        }
    }

    /// Writes the header of the dynamic code and the tokens `tokens` in it,
    /// after the 3 bits of the block type.
    fn write_dynamic(
        &self,
        out: &mut Writer,
        tokens: &[u8],
        sink: &mut dyn Sink,
    ) -> Result<(), Full> {
        out.put(low16(self.literals - 257).into(), 5, sink)?;
        out.put(low16(self.distances - 1).into(), 5, sink)?;
        out.put(low16(self.code_length_count - 4).into(), 4, sink)?;
        for &symbol in huffman::ORDER.iter().take(self.code_length_count) {
            out.put(self.code_lengths[usize::from(symbol)].into(), 3, sink)?;
        }
        let mut codes = [0u16; 19];
        huffman::codes_of(&self.code_lengths, &mut codes);
        let all = joined(
            &self.code.literal_lengths,
            self.literals,
            &self.code.distance_lengths,
            self.distances,
        );
        let all = all
            .get(..self.literals + self.distances)
            .unwrap_or_default();
        for (symbol, extra) in Runs::new(all) {
            let at = usize::from(symbol);
            out.put(codes[at].into(), self.code_lengths[at].into(), sink)?;
            out.put(extra.into(), extra_bits(symbol).into(), sink)?;
        }
        write_tokens(out, tokens, &self.code, sink)
    }
}

/// Returns the `literals` first lengths of `literal` followed by the
/// `distances` first lengths of `distance`, the one sequence a dynamic header
/// codes.
fn joined(
    literal: &[u8],
    literals: usize,
    distance: &[u8],
    distances: usize,
) -> [u8; LITERALS + DISTANCES] {
    let mut all = [0u8; LITERALS + DISTANCES];
    let sources = literal
        .iter()
        .take(literals)
        .chain(distance.iter().take(distances));
    for (slot, &len) in all.iter_mut().zip(sources) {
        *slot = len;
    }
    all
}

/// Returns the count of lengths a header sends of `lengths`, up to the last
/// that is not 0 and at least `least`.
fn sent(lengths: &[u8], least: usize) -> usize {
    lengths
        .iter()
        .rposition(|&len| len != 0)
        .map_or(least, |at| (at + 1).max(least))
}

/// Returns the count of extra bits of the code length symbol `symbol`: 2 for
/// the repeat 16, 3 for the zeros 17, 7 for the zeros 18, and none for a
/// length.
fn extra_bits(symbol: u8) -> u8 {
    match symbol {
        16 => 2,
        17 => 3,
        18 => 7,
        _ => 0,
    }
}

/// Writes the tokens `tokens` and the end of block in the code `code`.
fn write_tokens(
    out: &mut Writer,
    tokens: &[u8],
    code: &Code,
    sink: &mut dyn Sink,
) -> Result<(), Full> {
    let literal = |out: &mut Writer, symbol: usize, sink: &mut dyn Sink| {
        let bits = code.literal_codes.get(symbol).copied().unwrap_or(0);
        let len = code.literal_lengths.get(symbol).copied().unwrap_or(0);
        out.put(bits.into(), len.into(), sink)
    };
    for bytes in tokens.chunks_exact(TOKEN) {
        match Token::of(bytes) {
            Token::Literal(byte) => literal(out, usize::from(byte), sink)?,
            Token::Copy {
                length,
                length_extra,
                distance,
                distance_extra,
            } => {
                literal(out, 257 + length, sink)?;
                let bits = huffman::LENGTH_EXTRA.get(length).copied().unwrap_or(0);
                out.put(length_extra, bits.into(), sink)?;
                let code_bits = code.distance_codes.get(distance).copied().unwrap_or(0);
                let len = code.distance_lengths.get(distance).copied().unwrap_or(0);
                out.put(code_bits.into(), len.into(), sink)?;
                let bits = huffman::DISTANCE_EXTRA.get(distance).copied().unwrap_or(0);
                out.put(distance_extra, bits.into(), sink)?;
            }
        }
    }
    literal(out, END_OF_BLOCK, sink)
}

/// The code lengths of a dynamic header as the symbols of the code length
/// alphabet with their extra bits, runs of zeros as 17 and 18 and runs of a
/// length as the length and 16.
struct Runs<'a> {
    /// The code lengths.
    lengths: &'a [u8],
    /// The index of the next length to read.
    at: usize,
    /// The repeat that follows a length, sent next.
    repeat: Option<(u8, u8)>,
}

impl<'a> Runs<'a> {
    /// Returns the runs of the code lengths `lengths`.
    fn new(lengths: &'a [u8]) -> Runs<'a> {
        Runs {
            lengths,
            at: 0,
            repeat: None,
        }
    }
}

impl Iterator for Runs<'_> {
    type Item = (u8, u8);

    fn next(&mut self) -> Option<(u8, u8)> {
        if let Some(repeat) = self.repeat.take() {
            return Some(repeat);
        }
        let rest = self.lengths.get(self.at..)?;
        let &value = rest.first()?;
        let run = rest.iter().take_while(|&&len| len == value).count();
        if value == 0 && run >= 3 {
            let run = run.min(138);
            self.at += run;
            let extra = low16(run).to_le_bytes()[0];
            return Some(if run >= 11 {
                (18, extra - 11)
            } else {
                (17, extra - 3)
            });
        }
        if value != 0 && run >= 4 {
            let repeat = (run - 1).min(6);
            self.at += 1 + repeat;
            self.repeat = Some((16, low16(repeat - 3).to_le_bytes()[0]));
            return Some((value, 0));
        }
        self.at += 1;
        Some((value, 0))
    }
}

/// The bits of the coded stream not yet whole bytes, and the whole bytes not
/// yet in the sink.
struct Writer {
    /// The bits not yet a whole byte, the first lowest.
    acc: u32,
    /// The count of bits in `acc`, below 8 between two puts.
    bits: u32,
    /// The whole bytes not yet in the sink.
    stage: [u8; STAGE],
    /// The count of bytes in `stage`.
    len: usize,
}

impl Writer {
    /// Appends the `n` low bits of `value`, for `n` of 0 to 16, the lowest
    /// first, which is how DEFLATE packs its fields and its bit reversed codes.
    fn put(&mut self, value: u32, n: u32, sink: &mut dyn Sink) -> Result<(), Full> {
        self.acc |= (value & ((1 << n) - 1)) << self.bits;
        self.bits += n;
        while self.bits >= 8 {
            let [byte, ..] = self.acc.to_le_bytes();
            if self.len == STAGE {
                self.drain(sink)?;
            }
            if let Some(slot) = self.stage.get_mut(self.len) {
                *slot = byte;
                self.len += 1;
            }
            self.acc >>= 8;
            self.bits -= 8;
        }
        Ok(())
    }

    /// Pads the stream with zero bits to the next byte boundary.
    fn align(&mut self, sink: &mut dyn Sink) -> Result<(), Full> {
        if self.bits > 0 {
            self.put(0, 8 - self.bits, sink)?;
        }
        Ok(())
    }

    /// Puts the whole bytes gathered into the sink.
    fn drain(&mut self, sink: &mut dyn Sink) -> Result<(), Full> {
        sink.put(self.stage.get(..self.len).unwrap_or_default())?;
        self.len = 0;
        Ok(())
    }
}
