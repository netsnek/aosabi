// SPDX-License-Identifier: MIT
/*
 * aosabi-http: the zlib wrapper of a DEFLATE stream.
 *
 * Translation of RFC 1950 section 2.2, https://www.rfc-editor.org/rfc/rfc1950,
 * the stream with its header and its Adler-32, around the DEFLATE encoder of
 * graphtp.
 */

//! The zlib wrapper, the content coding `deflate` of HTTP.
//!
//! A zlib stream is a header of 2 bytes, the DEFLATE data and the Adler-32 of
//! the plain bytes, big endian. The header names the window of the scratch,
//! CINFO its base 2 logarithm less 8, and the level 2 of the default, so a
//! stream at the 32 KiB of the `server` card opens with `0x78 0x9C` as C zlib
//! writes it at its default level. The header and the Adler-32 come from
//! [`graphtp::zlib`], whose profile of `GraphTP` holds the window at 4 KiB,
//! while this wrapper takes the window of the card up to 32 KiB.

use graphtp::{
    deflate::Scratch,
    sink::{
        Full,
        Sink, //
    },
    zlib::{
        self,
        Adler32, //
    },
};

use super::Framed;

/// The encoder of one zlib stream, over a scratch the caller owns.
///
/// The stream opens with the header of the window of the scratch, its DEFLATE
/// data comes from [`graphtp::deflate::Encoder`], and [`Encoder::finish`]
/// closes it with the Adler-32 of the plain bytes. A write that meets a sink
/// with no room returns [`Full`] and leaves the stream incomplete.
pub struct Encoder<'a> {
    /// The DEFLATE encoder behind the header.
    framed: Framed<'a>,
    /// The header of the window of the scratch.
    header: [u8; 2],
    /// The Adler-32 of the plain bytes written.
    sum: Adler32,
}

impl<'a> Encoder<'a> {
    /// Returns the encoder of a new stream over the memory `scratch`.
    #[must_use]
    pub fn new(scratch: &'a mut Scratch) -> Encoder<'a> {
        let header = zlib::header(scratch.params().window_bits);
        Encoder {
            framed: Framed::new(scratch),
            header,
            sum: Adler32::new(),
        }
    }

    /// Codes the plain bytes `plain` and puts the header and the coded bytes of
    /// every block that closes into `sink`.
    ///
    /// # Errors
    ///
    /// Returns [`Full`] when `sink` has no room, and on every call after that.
    pub fn write(&mut self, plain: &[u8], sink: &mut dyn Sink) -> Result<(), Full> {
        self.framed.write(&self.header, plain, sink)?;
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
        self.framed.finish(&self.header, sink)?;
        sink.put(&self.sum.value().to_be_bytes())
    }
}
