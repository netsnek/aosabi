// SPDX-License-Identifier: MIT
/*
 * homa stream: AoFrame, the framing of one Homa packet on a byte stream.
 *
 * AoFrame is the framing of one Homa packet on a plain byte stream and nothing
 * else, absent wherever the carrier keeps packet boundaries, with a delimited
 * form for a stream that loses bytes and a prefixed form for one that never
 * does.
 */

//! `AoFrame`, one Homa packet per frame on a byte stream.
//!
//! ```text
//! Delimited, on a serial line and USB serial
//!
//!  0x00  COBS( packet ‖ CRC-32C(packet) )  0x00
//!
//! Prefixed, on a stream Unix Socket and the Bluetooth channel
//!
//!  length  4 bytes, big endian, the packet length
//!  packet  one Homa packet, header and payload
//! ```
//!
//! **The delimited form.** The CRC-32C of the packet follows it least
//! significant byte first, COBS removes every zero byte from the two, and a
//! zero byte stands before and after every frame, so line noise ahead of a
//! frame is discarded as a frame of its own and two frames in a row are
//! separated by an empty one, which a receiver ignores. A receiver collects
//! bytes into a buffer of fixed size until a delimiter, decodes COBS in place
//! and checks the CRC. A frame that overflows the buffer is discarded up to
//! the next delimiter as [`FrameError::TooLong`]. A lost byte, a merged frame
//! and a split frame that fit the buffer fail the COBS decode or the CRC, as
//! [`FrameError::Cobs`] or [`FrameError::Crc`], whichever check meets the
//! damage first. Every failure is counted and never answered, so Homa's
//! RESEND recovers the packet.
//!
//! **The prefixed form.** A stream that never loses, flips or reorders a byte
//! needs no check, so a length prefix finds every edge. The prefix is refused
//! above the buffer's size before a byte of the packet is stored, because a
//! length from a peer is an allocation request, and the bytes of a refused
//! packet are skipped, which keeps the stream in step.
//!
//! The module compiles with either feature, `stream-delimited` or
//! `stream-prefixed`, and every such build compiles both forms with `cobs`
//! and `crc32c`. Each feature enables the codec of its form through `cfg!`,
//! so the code of a form the build left out folds away. A mode the build left
//! out has no codec: [`encode`] answers [`NoRoom`] for it and a [`Decoder`]
//! of it has no room, so it refuses every frame as [`FrameError::TooLong`], a
//! prefixed frame of length 0 included.

mod cobs;
mod crc32c;

use core::fmt;

use crate::{
    Carrier,
    NoRoom,
    Priority, //
};

use self::crc32c::Crc32c;

/// The byte that ends a delimited frame, and starts one after silence.
pub const DELIMITER: u8 = 0x00;

/// The bytes of the CRC-32C that follows a packet in a delimited frame.
pub const CRC_LEN: usize = 4;

/// The bytes of the length that precedes a packet in a prefixed frame.
pub const PREFIX_LEN: usize = 4;

/// The two forms of `AoFrame`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// A zero byte, COBS of the packet and its CRC-32C, and a zero byte, for a
    /// stream that loses bytes.
    Delimited,
    /// A four byte big endian length and the packet, for a stream that never
    /// loses a byte.
    Prefixed,
}

impl Mode {
    /// Returns whether the build holds the codec of the mode.
    const fn built(self) -> bool {
        match self {
            Self::Delimited => cfg!(feature = "stream-delimited"),
            Self::Prefixed => cfg!(feature = "stream-prefixed"),
        }
    }
}

/// Returns the bytes one frame of a `packet_len` byte packet takes at most.
#[must_use]
pub const fn frame_len(mode: Mode, packet_len: usize) -> usize {
    match mode {
        Mode::Delimited => cobs::max_len(packet_len.saturating_add(CRC_LEN)).saturating_add(2),
        Mode::Prefixed => packet_len.saturating_add(PREFIX_LEN),
    }
}

/// Writes one frame of the packet made of `header` and `payload` into `out`
/// and returns its length.
///
/// # Errors
///
/// Returns [`NoRoom`] when the frame does not fit `out`, when the packet is
/// longer than a prefix states, and for a mode the build left out.
pub fn encode(mode: Mode, header: &[u8], payload: &[u8], out: &mut [u8]) -> Result<usize, NoRoom> {
    if !mode.built() {
        return Err(NoRoom);
    }
    match mode {
        Mode::Delimited => encode_delimited(header, payload, out),
        Mode::Prefixed => encode_prefixed(header, payload, out),
    }
}

/// Writes one delimited frame.
fn encode_delimited(header: &[u8], payload: &[u8], out: &mut [u8]) -> Result<usize, NoRoom> {
    let mut crc = Crc32c::new();
    crc.update(header);
    crc.update(payload);
    *out.first_mut().ok_or(NoRoom)? = DELIMITER;
    let mut encoder = cobs::Encoder::new(out, 1).ok_or(NoRoom)?;
    encoder.push_all(header).ok_or(NoRoom)?;
    encoder.push_all(payload).ok_or(NoRoom)?;
    encoder
        .push_all(&crc.finish().to_le_bytes())
        .ok_or(NoRoom)?;
    let end = encoder.finish().ok_or(NoRoom)?;
    *out.get_mut(end).ok_or(NoRoom)? = DELIMITER;
    Ok(end + 1)
}

/// Writes one prefixed frame.
fn encode_prefixed(header: &[u8], payload: &[u8], out: &mut [u8]) -> Result<usize, NoRoom> {
    let len = header.len().checked_add(payload.len()).ok_or(NoRoom)?;
    let prefix = u32::try_from(len).map_err(|_| NoRoom)?.to_be_bytes();
    let end = len.checked_add(PREFIX_LEN).ok_or(NoRoom)?;
    let frame = out.get_mut(..end).ok_or(NoRoom)?;
    let (head, body) = frame.split_at_mut(PREFIX_LEN);
    head.copy_from_slice(&prefix);
    let (first, second) = body.split_at_mut(header.len());
    first.copy_from_slice(header);
    second.copy_from_slice(payload);
    Ok(end)
}

/// Why a frame was dropped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameError {
    /// The frame is longer than the decoder's buffer, or its prefix states
    /// more than the buffer holds.
    TooLong,
    /// The COBS of a delimited frame does not decode.
    Cobs,
    /// The CRC-32C of a delimited frame does not match its packet.
    Crc,
}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::TooLong => "the frame is longer than the buffer",
            Self::Cobs => "the COBS of the frame does not decode",
            Self::Crc => "the CRC-32C of the frame does not match",
        })
    }
}

impl core::error::Error for FrameError {}

/// Where a decoder stands in the stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    /// A delimited frame is collected into the buffer.
    Collect,
    /// A delimited frame overflowed the buffer and is discarded up to the
    /// next delimiter.
    Overflow,
    /// The prefix of a frame arrives, `got` bytes of it so far.
    Prefix {
        /// The bytes of the prefix received.
        got: u8,
        /// The value of those bytes.
        value: u32,
    },
    /// The packet of a prefixed frame arrives, `need` bytes in all.
    Body {
        /// The bytes of the packet.
        need: usize,
    },
    /// The packet of a refused prefixed frame is skipped, `left` bytes more.
    Skip {
        /// The bytes still to skip.
        left: u32,
    },
}

/// Collects frames from a byte stream into a buffer of fixed size.
#[derive(Debug)]
pub struct Decoder<'b> {
    /// The form of the frames.
    mode: Mode,
    /// The buffer a frame is collected in.
    buf: &'b mut [u8],
    /// The bytes of the frame collected.
    len: usize,
    /// Where the decoder stands.
    state: State,
}

/// Where a decoder stands in its stream, apart from the buffer it collects
/// into, which [`Decoder::place`] returns and [`Decoder::resume`] takes up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Place {
    /// The form of the frames.
    mode: Mode,
    /// The bytes of the frame collected.
    len: usize,
    /// Where the decoder stands.
    state: State,
}

impl Place {
    /// Returns the place of a decoder of the form `mode` before the first byte
    /// of its stream, where [`Decoder::new`] stands.
    #[must_use]
    pub fn start(mode: Mode) -> Self {
        Decoder::new(mode, &mut []).place()
    }
}

impl<'b> Decoder<'b> {
    /// Returns the bytes of buffer a decoder of packets up to `max_packet`
    /// needs, 0 for a mode the build left out.
    #[must_use]
    pub const fn buffer_len(mode: Mode, max_packet: usize) -> usize {
        if !mode.built() {
            return 0;
        }
        match mode {
            Mode::Delimited => cobs::max_len(max_packet.saturating_add(CRC_LEN)),
            Mode::Prefixed => max_packet,
        }
    }

    /// Builds a decoder over `buf`, which refuses a frame above its size
    /// before storing it.
    ///
    /// A prefixed decoder refuses a prefix above the length of `buf`, and a
    /// delimited one a frame whose encoding outgrows it. A decoder of a mode
    /// the build left out uses none of `buf`.
    pub fn new(mode: Mode, buf: &'b mut [u8]) -> Self {
        let room = if mode.built() { buf.len() } else { 0 };
        let state = match mode {
            Mode::Delimited => State::Collect,
            Mode::Prefixed => State::Prefix { got: 0, value: 0 },
        };
        Self {
            mode,
            buf: &mut buf[..room],
            len: 0,
            state,
        }
    }

    /// Builds a decoder over `buf` that stands at `place`, where an earlier
    /// decoder over the same buffer stood.
    ///
    /// A frame split between the two decoders is read whole, since `buf` holds
    /// the bytes the earlier one collected. An engine that a host drives
    /// through calls, which lends its buffer to a decoder for one call at a
    /// time, keeps its place this way, so each byte passes a decoder once.
    pub fn resume(buf: &'b mut [u8], place: Place) -> Self {
        Self {
            len: place.len,
            state: place.state,
            ..Self::new(place.mode, buf)
        }
    }

    /// Returns where the decoder stands, for a decoder [`Decoder::resume`]
    /// builds over the same buffer.
    #[must_use]
    pub const fn place(&self) -> Place {
        Place {
            mode: self.mode,
            len: self.len,
            state: self.state,
        }
    }

    /// Takes one byte and returns the checked packet or the reason a frame was
    /// dropped, once a frame ends.
    pub fn push(&mut self, byte: u8) -> Option<Result<&[u8], FrameError>> {
        match self.mode {
            Mode::Delimited => self.push_delimited(byte),
            Mode::Prefixed => self.push_prefixed(byte),
        }
    }

    /// Takes one byte of a delimited stream.
    fn push_delimited(&mut self, byte: u8) -> Option<Result<&[u8], FrameError>> {
        if byte != DELIMITER {
            if self.state == State::Collect {
                if let Some(slot) = self.buf.get_mut(self.len) {
                    *slot = byte;
                    self.len += 1;
                } else {
                    self.state = State::Overflow;
                }
            }
            return None;
        }
        let len = core::mem::replace(&mut self.len, 0);
        if core::mem::replace(&mut self.state, State::Collect) == State::Overflow {
            return Some(Err(FrameError::TooLong));
        }
        if len == 0 {
            return None;
        }
        Some(unframe(self.buf, len))
    }

    /// Takes one byte of a prefixed stream.
    fn push_prefixed(&mut self, byte: u8) -> Option<Result<&[u8], FrameError>> {
        match self.state {
            State::Prefix { got, value } => {
                let value = (value << 8) | u32::from(byte);
                if usize::from(got) + 1 < PREFIX_LEN {
                    self.state = State::Prefix {
                        got: got + 1,
                        value,
                    };
                    return None;
                }
                self.state = State::Prefix { got: 0, value: 0 };
                let need = usize::try_from(value).unwrap_or(usize::MAX);
                if !Mode::Prefixed.built() || need > self.buf.len() {
                    if value > 0 {
                        self.state = State::Skip { left: value };
                    }
                    return Some(Err(FrameError::TooLong));
                }
                if need == 0 {
                    return Some(Ok(&[]));
                }
                self.len = 0;
                self.state = State::Body { need };
                None
            }
            State::Body { need } => {
                if let Some(slot) = self.buf.get_mut(self.len) {
                    *slot = byte;
                }
                self.len += 1;
                if self.len < need {
                    return None;
                }
                self.state = State::Prefix { got: 0, value: 0 };
                self.len = 0;
                Some(Ok(self.buf.get(..need).unwrap_or(&[])))
            }
            State::Skip { left } => {
                self.state = if left > 1 {
                    State::Skip { left: left - 1 }
                } else {
                    State::Prefix { got: 0, value: 0 }
                };
                None
            }
            State::Collect | State::Overflow => {
                self.state = State::Prefix { got: 0, value: 0 };
                self.push_prefixed(byte)
            }
        }
    }
}

/// Decodes the delimited frame of `len` bytes at the front of `buf` and
/// returns its checked packet.
fn unframe(buf: &mut [u8], len: usize) -> Result<&[u8], FrameError> {
    if !Mode::Delimited.built() {
        return Err(FrameError::TooLong);
    }
    let decoded = cobs::decode_in_place(buf, len).ok_or(FrameError::Cobs)?;
    let packet_len = decoded.checked_sub(CRC_LEN).ok_or(FrameError::Crc)?;
    let (packet, rest) = buf.split_at(packet_len);
    let mut sent = [0u8; CRC_LEN];
    sent.copy_from_slice(rest.get(..CRC_LEN).ok_or(FrameError::Crc)?);
    let mut crc = Crc32c::new();
    crc.update(packet);
    if crc.finish() != u32::from_le_bytes(sent) {
        return Err(FrameError::Crc);
    }
    Ok(packet)
}

/// A byte stream: a UART, a tty of USB serial, a stream socket.
///
/// Both calls are non blocking and answer `None` where the stream would
/// block, so a driver on Linux retries after its reactor wakes it and a bare
/// metal driver after its executor does.
pub trait ByteStream {
    /// A failure of the stream.
    type Error;

    /// Reads what the stream holds into `buf` and returns how many bytes it
    /// read, `Some(0)` at the end of the stream, or `None` where it would
    /// block.
    ///
    /// # Errors
    ///
    /// Returns the stream's error when the stream itself fails.
    fn read(&mut self, buf: &mut [u8]) -> Result<Option<usize>, Self::Error>;

    /// Writes a prefix of `bytes` and returns how many bytes it wrote, or
    /// `None` where it would block.
    ///
    /// # Errors
    ///
    /// Returns the stream's error when the stream itself fails.
    fn write(&mut self, bytes: &[u8]) -> Result<Option<usize>, Self::Error>;
}

/// The bytes a [`Framed`] reads from its stream at once.
const CHUNK: usize = 64;

/// A packet carrier over one byte stream, with one peer.
///
/// A sent packet is framed into the transmit buffer and written as far as the
/// stream takes it, the rest when the stream takes more, and a received
/// packet is read from the stream in chunks and unframed in the receive
/// buffer. A frame dropped in either direction is counted in
/// [`Framed::dropped`], and Homa's RESEND recovers its packet.
#[derive(Debug)]
pub struct Framed<'b, S> {
    /// The stream.
    stream: S,
    /// The form of the frames.
    mode: Mode,
    /// The decoder of received frames.
    decoder: Decoder<'b>,
    /// The frames queued for the stream.
    tx: &'b mut [u8],
    /// The first queued byte the stream has not taken.
    tx_start: usize,
    /// The end of the queued bytes.
    tx_end: usize,
    /// The bytes read from the stream and not yet decoded.
    chunk: [u8; CHUNK],
    /// The first byte of `chunk` not yet decoded.
    chunk_start: usize,
    /// The end of the bytes read into `chunk`.
    chunk_end: usize,
    /// The frames dropped in either direction.
    dropped: u64,
    /// Whether the stream would not take the queued bytes.
    blocked: bool,
    /// Whether the stream reported its end.
    closed: bool,
}

impl<'b, S: ByteStream> Framed<'b, S> {
    /// Builds a carrier over `stream` with the given form and buffers.
    ///
    /// `rx` holds one received frame, sized by [`Decoder::buffer_len`], and
    /// `tx` the frames not yet written, at least one [`frame_len`] of the
    /// longest packet.
    pub fn new(stream: S, mode: Mode, rx: &'b mut [u8], tx: &'b mut [u8]) -> Self {
        Self {
            stream,
            mode,
            decoder: Decoder::new(mode, rx),
            tx,
            tx_start: 0,
            tx_end: 0,
            chunk: [0; CHUNK],
            chunk_start: 0,
            chunk_end: 0,
            dropped: 0,
            blocked: false,
            closed: false,
        }
    }

    /// Returns the stream.
    pub fn stream(&self) -> &S {
        &self.stream
    }

    /// Returns the stream to change it in place.
    pub fn stream_mut(&mut self) -> &mut S {
        &mut self.stream
    }

    /// Returns the count of frames dropped in either direction.
    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    /// Returns whether the stream reported its end.
    pub fn closed(&self) -> bool {
        self.closed
    }

    /// Writes the queued frames as far as the stream takes them, and returns
    /// whether the queue is empty.
    ///
    /// # Errors
    ///
    /// Returns the stream's error when the stream itself fails.
    pub fn flush(&mut self) -> Result<bool, S::Error> {
        while self.tx_start < self.tx_end {
            let queued = self.tx.get(self.tx_start..self.tx_end).unwrap_or(&[]);
            match self.stream.write(queued)? {
                Some(written) if written > 0 => {
                    self.tx_start += written.min(queued.len());
                    self.blocked = false;
                }
                _ => {
                    self.blocked = true;
                    return Ok(false);
                }
            }
        }
        self.tx_start = 0;
        self.tx_end = 0;
        self.blocked = false;
        Ok(true)
    }

    /// Moves the queued bytes to the front of the transmit buffer.
    fn compact(&mut self) {
        if self.tx_start > 0 {
            self.tx.copy_within(self.tx_start..self.tx_end, 0);
            self.tx_end -= self.tx_start;
            self.tx_start = 0;
        }
    }
}

impl<S: ByteStream> Carrier for Framed<'_, S> {
    type Peer = ();
    type Error = S::Error;

    /// Answers whether the stream took the queued bytes so far and the
    /// transmit buffer has room for the frame of a `len` byte packet.
    fn ready(&self, _peer: (), len: usize) -> bool {
        let queued = self.tx_end - self.tx_start;
        !self.blocked && self.tx.len() - queued >= frame_len(self.mode, len)
    }

    /// Frames the packet into the transmit buffer and writes what the stream
    /// takes. The stream has no priorities, so the level is ignored, and a
    /// packet that finds no room is dropped and counted.
    fn send(
        &mut self,
        _peer: (),
        _priority: Priority,
        header: &[u8],
        payload: &[u8],
    ) -> Result<(), S::Error> {
        self.compact();
        let free = self.tx.get_mut(self.tx_end..).unwrap_or(&mut []);
        match encode(self.mode, header, payload, free) {
            Ok(written) => self.tx_end += written,
            Err(NoRoom) => self.dropped = self.dropped.saturating_add(1),
        }
        self.flush().map(|_| ())
    }

    /// Writes the queued frames first, then reads the stream until a frame
    /// ends with a checked packet that fits `buf`, and counts every frame it
    /// drops on the way.
    fn recv(&mut self, buf: &mut [u8]) -> Result<Option<((), usize)>, S::Error> {
        if self.tx_start < self.tx_end {
            self.flush()?;
        }
        loop {
            while self.chunk_start < self.chunk_end {
                let byte = self.chunk[self.chunk_start];
                self.chunk_start += 1;
                match self.decoder.push(byte) {
                    None => {}
                    Some(Ok(packet)) => {
                        if let Some(target) = buf.get_mut(..packet.len()) {
                            target.copy_from_slice(packet);
                            return Ok(Some(((), packet.len())));
                        }
                        self.dropped = self.dropped.saturating_add(1);
                    }
                    Some(Err(_)) => self.dropped = self.dropped.saturating_add(1),
                }
            }
            if self.closed {
                return Ok(None);
            }
            match self.stream.read(&mut self.chunk)? {
                None => return Ok(None),
                Some(0) => {
                    self.closed = true;
                    return Ok(None);
                }
                Some(read) => {
                    self.chunk_start = 0;
                    self.chunk_end = read.min(CHUNK);
                }
            }
        }
    }
}
