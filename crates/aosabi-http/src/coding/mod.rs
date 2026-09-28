// SPDX-License-Identifier: MIT
/*
 * aosabi-http: the content coding deflate of HTTP around the DEFLATE encoder
 * of graphtp.
 *
 * Translation of RFC 1950, https://www.rfc-editor.org/rfc/rfc1950. The module
 * holds the zlib wrapper of HTTP at the window of the card and no DEFLATE code
 * of its own. The wrapper calls the DEFLATE encoder graphtp::deflate, takes
 * the zlib header of a window and the Adler-32 of the zlib trailer from
 * graphtp::zlib, and writes into the Sink of graphtp::sink.
 */

//! The content coding `deflate` of HTTP.
//!
//! Hono's `compress()` codes an answer as `gzip`, the format of RFC 1952, or as
//! `deflate`, which HTTP names for the zlib format of RFC 1950, since it pipes the
//! body through `CompressionStream` with that name. The wrapper frames the raw
//! DEFLATE data of RFC 1951 that [`graphtp::deflate::Encoder`] writes, the one
//! DEFLATE encoder of the workspace, so the module adds the header and the trailer
//! of zlib and nothing else.
//!
//! - [`zlib::Encoder`] writes one zlib stream at the window of its scratch.
//!
//! An encoder borrows a [`Scratch`] the caller owns for one answer and puts its
//! bytes into a [`Sink`], as the DEFLATE encoder does, so a card sizes the memory
//! of its coding once. The window is the card's: 32 KiB with the sizes of
//! [`SERVER`] on the `server` and `docker` cards, whose `compress()` in the web
//! layer widens the hash heads of `SERVER` to 32,768 on a Linux target of 64 bits,
//! while the zlib profile of `GraphTP` fixes 4 KiB for its own messages. A stream
//! at any window up to 32 KiB decodes in every decoder of RFC 1951, so the window
//! changes the ratio and never the validity.
//!
//! Byte equality is judged after decoding, so a coded answer is valid when it
//! decodes to the plain bytes. The judging notebook is
//! `tests/notebooks/37_content_coding.ipynb`, whose rows of the HTTP wrapper
//! decode every stream with Python's `zlib`.

use graphtp::{
    deflate::{
        self,
        Params,
        Scratch, //
    },
    sink::{
        Full,
        Sink, //
    },
};

pub mod zlib;

/// The base 2 logarithm of the window of the HTTP codings on the `server` and
/// `docker` cards, 15, a window of 32 KiB.
pub const SERVER_WINDOW_BITS: u8 = 15;

/// The sizes of the scratch of the HTTP codings at the window of the `server`
/// and `docker` cards: a window of 32 KiB, 4,096 hash heads, blocks of 8,192
/// tokens and a buffer of 65,536 plain bytes, the sizes of [`Params::new`].
///
/// The scratch of these sizes holds 163,840 bytes. The `compress()` of the web
/// layer takes them with 32,768 hash heads on a Linux target of 64 bits, the
/// `server`, `server-legacy` and `docker` cards, a scratch of 221,184 bytes, and
/// as they stand on the player.
pub const SERVER: Params = Params::new(SERVER_WINDOW_BITS);

/// Where the header of a wrapper stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Opening {
    /// No byte is in the sink yet.
    Pending,
    /// The header is in the sink.
    Written,
    /// A sink had no room for the header, which leaves the stream incomplete.
    Failed,
}

/// The DEFLATE encoder of one answer behind the header of its wrapper.
///
/// The header reaches the sink before the first coded byte, with the first
/// write or with the finish of an empty answer, so an encoder is made without
/// a sink as the DEFLATE encoder is. A sink with no room for the header fails
/// the stream as a sink with no room for a block does, and every later call
/// returns [`Full`].
struct Framed<'a> {
    /// The DEFLATE encoder over the caller's scratch.
    deflate: deflate::Encoder<'a>,
    /// Where the header stands.
    opening: Opening,
}

impl<'a> Framed<'a> {
    /// Returns the encoder of a new stream over the memory `scratch`.
    fn new(scratch: &'a mut Scratch) -> Framed<'a> {
        Framed {
            deflate: deflate::Encoder::new(scratch),
            opening: Opening::Pending,
        }
    }

    /// Puts `header` into `sink` unless it stands there already.
    fn open(&mut self, header: &[u8], sink: &mut dyn Sink) -> Result<(), Full> {
        match self.opening {
            Opening::Written => Ok(()),
            Opening::Failed => Err(Full),
            Opening::Pending => {
                let put = sink.put(header);
                self.opening = if put.is_ok() {
                    Opening::Written
                } else {
                    Opening::Failed
                };
                put
            }
        }
    }

    /// Codes the plain bytes `plain` behind `header`.
    fn write(&mut self, header: &[u8], plain: &[u8], sink: &mut dyn Sink) -> Result<(), Full> {
        self.open(header, sink)?;
        self.deflate.write(plain, sink)
    }

    /// Writes the last block behind `header`, which ends the DEFLATE data.
    fn finish(mut self, header: &[u8], sink: &mut dyn Sink) -> Result<(), Full> {
        self.open(header, sink)?;
        self.deflate.finish(sink)
    }
}
