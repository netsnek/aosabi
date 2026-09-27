// SPDX-License-Identifier: MIT
/*
 * graphtp deflate: the DEFLATE core of RFC 1951, its encoder, its decoder and
 * the Huffman codes they share.
 *
 * No analogue in the reference. The core follows RFC 1951, and the module is
 * public, so a crate that wraps DEFLATE for HTTP takes the one encoder.
 */

//! The DEFLATE core of RFC 1951: the encoder and the decoder, each in memory
//! the caller owns.
//!
//! The core writes and reads raw DEFLATE data and knows no wrapper, so the
//! zlib profile of `GraphTP` in [`crate::zlib`] and a wrapper of HTTP take the
//! one encoder and the one decoder. The [`Encoder`] takes a
//! window of 512 bytes to 32 KiB in a [`Scratch`] the caller owns, and the
//! [`Decoder`] decodes whole into the caller's buffer or streaming with the
//! caller's window, each under a bound.

pub mod decode;
pub mod encode;
pub mod huffman;

pub use decode::{
    Decoded,
    Decoder,
    Error, //
};
pub use encode::{
    Encoder,
    Params,
    Scratch,
    ScratchError, //
};
