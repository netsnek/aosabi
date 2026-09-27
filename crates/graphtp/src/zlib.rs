// SPDX-License-Identifier: MIT
/*
 * graphtp zlib: the zlib profile of GraphTP/1 over the DEFLATE core, with its
 * header check, its encoder, its decoder and Adler-32.
 *
 * No analogue in the reference. The profile, its window and the memory it holds
 * are those of the specification, over RFC 1950, and the codes of its refusals
 * are those of rules 9 to 11 of section 5.3.
 */

//! The zlib profile of GraphTP/1: the header check, the encoder at a window of
//! 4 KiB with the scratch the caller owns, the decoder with its bound, and
//! Adler-32.
//!
//! A coded content or body is one zlib stream of RFC 1950 whose data is DEFLATE
//! of RFC 1951: a header of 2 bytes with the method 8, a window of at most
//! 4,096 bytes and no preset dictionary, the DEFLATE data, and the Adler-32 of
//! the plain bytes, with nothing after it. The encoder writes the header
//! `0x48 0x89`, a window of 4 KiB and the level byte of the default, and the
//! decoder refuses a stream by the rules 9 to 11 of section 5.3, each with its
//! [`Code`].

use core::{
    error,
    fmt, //
};

use crate::{
    deflate::{
        decode,
        encode::{
            self,
            Params,
            ScratchError, //
        },
    },
    sink::{
        Full,
        Sink, //
    },
    status::Code,
};

/// The base 2 logarithm of the window of the profile, 12.
pub const WINDOW_BITS: u8 = 12;

/// The window of the profile, 4,096 bytes, which a streaming decoder holds and
/// which no distance of a stream of the profile passes.
pub const WINDOW: usize = 1 << WINDOW_BITS;

/// The sizes of the scratch of a sender at the window of the profile: 4,096
/// hash heads, blocks of 8,192 tokens and a buffer of 8,192 plain bytes, twice
/// the window.
pub const PARAMS: Params = Params::new(WINDOW_BITS);

/// The method of every zlib stream, 8 for DEFLATE.
const DEFLATE: u8 = 8;

/// The bit of FLG that asks for a preset dictionary.
const FDICT: u8 = 0x20;

/// The level FLEVEL the encoder writes, 2 for the default algorithm, which is
/// informational and never read.
const FLEVEL: u8 = 2;

/// The modulus of Adler-32, the largest prime below 65,536.
const BASE: u32 = 65_521;

/// The most bytes Adler-32 sums before it reduces its two sums, so that neither
/// passes 32 bits.
const NMAX: usize = 5552;

/// Why a zlib stream of the profile was refused, each reason with the code of
/// the rule of `docs/graphtp.md` section 5.3 it breaks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Error {
    /// The plain bytes pass the decoded bound, or the room the caller gave
    /// them, which is rule 10, answered 413.0.
    Bound,
    /// The header is a zlib header outside the profile: a method other than
    /// DEFLATE, a window above 4 KiB or a preset dictionary, which is rule 9,
    /// answered 415.1.
    Unsupported,
    /// The stream is shorter than a header, its FCHECK fails, or its DEFLATE
    /// data does not decode, which is rules 9 and 11, answered 400.8.
    Corrupt,
    /// The Adler-32 after the data differs from the one of the plain bytes,
    /// which is rule 11, answered 400.8.
    Checksum,
    /// Bytes follow the Adler-32, which is rule 11, answered 400.8.
    Trailing,
}

impl Error {
    /// Returns the code a server answers the refusal with: 413.0 for
    /// [`Error::Bound`], 415.1 for [`Error::Unsupported`] and 400.8 for every
    /// other reason.
    #[must_use]
    pub const fn code(self) -> Code {
        match self {
            Error::Bound => Code::CONTENT_TOO_LARGE,
            Error::Unsupported => Code::UNSUPPORTED_CODING,
            Error::Corrupt | Error::Checksum | Error::Trailing => Code::BAD_CODING,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Error::Bound => "the plain bytes pass the bound",
            Error::Unsupported => "the zlib header is outside the profile",
            Error::Corrupt => "the stream is no valid zlib stream",
            Error::Checksum => "the Adler-32 differs from the one of the plain bytes",
            Error::Trailing => "bytes follow the Adler-32",
        })
    }
}

impl error::Error for Error {}

impl From<decode::Error> for Error {
    fn from(error: decode::Error) -> Error {
        match error {
            decode::Error::Bound => Error::Bound,
            decode::Error::Corrupt => Error::Corrupt,
        }
    }
}

/// Checks the zlib header `cmf` and `flg` against the profile.
///
/// FCHECK is read first, since 2 bytes whose FCHECK fails are no zlib header
/// and their fields mean nothing, and then the fields: the method 8, CINFO of
/// at most 4 and FDICT clear. FLEVEL is never read.
///
/// # Errors
///
/// Returns [`Code::BAD_CODING`], 400.8, when `cmf * 256 + flg` is no multiple
/// of 31, and [`Code::UNSUPPORTED_CODING`], 415.1, for a method other than 8,
/// a window above 4 KiB or FDICT set.
pub fn check_header(cmf: u8, flg: u8) -> Result<(), Code> {
    profile(cmf, flg).map_err(Error::code)
}

/// Checks a zlib header as [`check_header`] does, with the reason in place of
/// the code.
fn profile(cmf: u8, flg: u8) -> Result<(), Error> {
    if !u16::from_be_bytes([cmf, flg]).is_multiple_of(31) {
        return Err(Error::Corrupt);
    }
    if cmf & 0x0F != DEFLATE || cmf >> 4 > WINDOW_BITS - 8 || flg & FDICT != 0 {
        return Err(Error::Unsupported);
    }
    Ok(())
}

/// Returns the zlib header of a window of `window_bits`, with FLEVEL 2 and
/// FCHECK set so the two bytes are a multiple of 31.
///
/// The profile writes a window of 4 KiB, whose header is `0x48 0x89`, and the
/// windows of 512 bytes to 2 KiB write CINFO 1 to 3.
#[must_use]
pub fn header(window_bits: u8) -> [u8; 2] {
    let cmf = (window_bits.saturating_sub(8) << 4) | DEFLATE;
    let flg = FLEVEL << 6;
    let rest = u16::from_be_bytes([cmf, flg]) % 31;
    let check = u8::try_from((31 - rest) % 31).unwrap_or(0);
    [cmf, flg | check]
}

/// The Adler-32 of RFC 1950 section 8.2, over bytes that arrive in pieces.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Adler32 {
    /// The sum of the bytes and 1, modulo 65,521.
    a: u32,
    /// The sum of the values of `a` after each byte, modulo 65,521.
    b: u32,
}

impl Adler32 {
    /// Returns the Adler-32 of no bytes, 1.
    #[must_use]
    pub const fn new() -> Adler32 {
        Adler32 { a: 1, b: 0 }
    }

    /// Adds the bytes `bytes` to the sum.
    // Vector sums in `unsafe` would be faster.
    pub fn update(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(NMAX) {
            for &byte in chunk {
                self.a += u32::from(byte);
                self.b += self.a;
            }
            self.a %= BASE;
            self.b %= BASE;
        }
    }

    /// Returns the Adler-32 of the bytes added so far.
    #[must_use]
    pub const fn value(&self) -> u32 {
        (self.b << 16) | self.a
    }
}

impl Default for Adler32 {
    fn default() -> Adler32 {
        Adler32::new()
    }
}

/// Returns the Adler-32 of `bytes`.
#[must_use]
pub fn adler32(bytes: &[u8]) -> u32 {
    let mut sum = Adler32::new();
    sum.update(bytes);
    sum.value()
}

/// The memory of a sender of the profile: the hash heads, the chains, the
/// tokens and the buffer of plain bytes of `docs/graphtp.md` section 3.6, at a
/// window of at most 4 KiB.
///
/// A scratch is made once and lent to one [`Encoder`] at a time, so a sender
/// holds it whatever the count of its messages.
pub struct Scratch(encode::Scratch);

impl Scratch {
    /// Returns the scratch of the sizes `params`, whose window is 512 bytes to
    /// 4 KiB, such as [`PARAMS`].
    ///
    /// # Errors
    ///
    /// Returns [`ScratchError::Params`] when the sizes are not valid or the
    /// window passes 4 KiB, and [`ScratchError::Memory`] when the allocator
    /// refuses the memory.
    pub fn new(params: Params) -> Result<Scratch, ScratchError> {
        if params.window_bits > WINDOW_BITS {
            return Err(ScratchError::Params);
        }
        encode::Scratch::new(params).map(Scratch)
    }

    /// Returns the sizes of the scratch.
    #[must_use]
    pub fn params(&self) -> Params {
        self.0.params()
    }

    /// Returns the bytes the scratch holds, its buffer of plain bytes
    /// included.
    #[must_use]
    pub fn bytes(&self) -> usize {
        self.0.bytes()
    }
}

/// The encoder of one zlib stream of the profile, over a scratch the caller
/// owns.
///
/// The stream opens with the header of the window of the scratch, `0x48 0x89`
/// at 4 KiB, and [`Encoder::finish`] closes it with the Adler-32 of the plain
/// bytes. A write that meets a sink with no room returns [`Full`] and leaves
/// the stream incomplete.
pub struct Encoder<'a> {
    /// The DEFLATE encoder, whose output opens with the zlib header.
    deflate: encode::Encoder<'a>,
    /// The Adler-32 of the plain bytes written.
    sum: Adler32,
}

impl<'a> Encoder<'a> {
    /// Returns the encoder of a new stream over the memory `scratch`.
    #[must_use]
    pub fn new(scratch: &'a mut Scratch) -> Encoder<'a> {
        let header = header(scratch.params().window_bits);
        Encoder {
            deflate: encode::Encoder::with_prefix(&mut scratch.0, &header),
            sum: Adler32::new(),
        }
    }

    /// Codes the plain bytes `plain` and puts the coded bytes of every block
    /// that closes into `sink`.
    ///
    /// # Errors
    ///
    /// Returns [`Full`] when `sink` has no room, and on every call after that.
    pub fn write(&mut self, plain: &[u8], sink: &mut dyn Sink) -> Result<(), Full> {
        self.deflate.write(plain, sink)?;
        self.sum.update(plain);
        Ok(())
    }

    /// Writes the last block and the Adler-32, which ends the stream.
    ///
    /// # Errors
    ///
    /// Returns [`Full`] when `sink` has no room or an earlier call met a sink
    /// with no room.
    pub fn finish(self, sink: &mut dyn Sink) -> Result<(), Full> {
        self.deflate.finish(sink)?;
        sink.put(&self.sum.value().to_be_bytes())
    }
}

/// The decoder of a zlib stream of the profile, with the decoded bound of
/// rule 10 of `docs/graphtp.md` section 5.3.
///
/// A receiver of a whole message decodes into one buffer and needs no window,
/// and a receiver that streams holds a window of 4,096 bytes and hands the
/// plain bytes to a sink.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Decoder {
    /// The DEFLATE decoder with the bound.
    deflate: decode::Decoder,
}

impl Decoder {
    /// Returns the decoder that refuses a stream as soon as its plain bytes
    /// pass `bound`.
    #[must_use]
    pub const fn new(bound: usize) -> Decoder {
        Decoder {
            deflate: decode::Decoder::new(bound),
        }
    }

    /// Returns the decoded bound.
    #[must_use]
    pub const fn bound(&self) -> usize {
        self.deflate.bound()
    }

    /// Decodes the zlib stream `stream`, which fills the slice, into
    /// `buffer`, from its start, and returns the count of plain bytes.
    ///
    /// A match copies from anywhere before it in `buffer`, so a stream whose
    /// distances pass its window decodes as well, as C zlib decodes it into one
    /// output buffer.
    ///
    /// # Errors
    ///
    /// Returns the [`Error`] of the first rule the stream breaks: the header,
    /// then the bound as soon as the plain bytes pass it or the length of
    /// `buffer`, then the data, the Adler-32 and the bytes after it.
    pub fn whole(&self, stream: &[u8], buffer: &mut [u8]) -> Result<usize, Error> {
        let data = open(stream)?;
        let decoded = self.deflate.whole(data, buffer)?;
        let plain = buffer.get(..decoded.produced).unwrap_or_default();
        close(data, decoded, adler32(plain))
    }

    /// Decodes the zlib stream `stream`, which fills the slice, through the
    /// ring `window` into `sink`, and returns the count of plain bytes.
    ///
    /// A window of [`WINDOW`] bytes decodes every stream of the profile.
    ///
    /// # Errors
    ///
    /// Returns the [`Error`] of the first rule the stream breaks, as
    /// [`Decoder::whole`] does, with [`Error::Bound`] as well when `sink` has
    /// no room and [`Error::Corrupt`] for a match that reaches further back
    /// than `window`. The plain bytes put into `sink` before the refusal are
    /// no answer.
    pub fn streaming(
        &self,
        stream: &[u8],
        window: &mut [u8],
        sink: &mut dyn Sink,
    ) -> Result<usize, Error> {
        let data = open(stream)?;
        let mut summed = Summed {
            sink,
            sum: Adler32::new(),
        };
        let decoded = self.deflate.streaming(data, window, &mut summed)?;
        close(data, decoded, summed.sum.value())
    }
}

/// Checks the header of `stream` and returns the bytes after it.
fn open(stream: &[u8]) -> Result<&[u8], Error> {
    let [cmf, flg, ref data @ ..] = *stream else {
        return Err(Error::Corrupt);
    };
    profile(cmf, flg)?;
    Ok(data)
}

/// Checks the Adler-32 after the DEFLATE data of `data` against `sum` and that
/// nothing follows it, and returns the count of plain bytes.
fn close(data: &[u8], decoded: decode::Decoded, sum: u32) -> Result<usize, Error> {
    let at = decoded.consumed;
    let Some(&[a, b, c, d]) = data.get(at..at + 4) else {
        return Err(Error::Corrupt);
    };
    if u32::from_be_bytes([a, b, c, d]) != sum {
        return Err(Error::Checksum);
    }
    if data.len() != at + 4 {
        return Err(Error::Trailing);
    }
    Ok(decoded.produced)
}

/// A sink that sums the Adler-32 of the bytes it passes on.
struct Summed<'a> {
    /// The sink the bytes go on to.
    sink: &'a mut dyn Sink,
    /// The Adler-32 of the bytes passed on.
    sum: Adler32,
}

impl Sink for Summed<'_> {
    fn put(&mut self, bytes: &[u8]) -> Result<(), Full> {
        self.sink.put(bytes)?;
        self.sum.update(bytes);
        Ok(())
    }
}
