// SPDX-License-Identifier: MIT
/*
 * aosabi-carrier-lora frame: the LoRa frame that seals one compact Homa
 * packet, and the window that refuses a replayed frame.
 *
 * The frame is that of the Homa design note, second version, homa/design.md
 * section 6.5, lines 1928 to 1979: a header of the link and the counter, the
 * packet sealed with ChaCha20-Poly1305 of RFC 8439, and the tag of 16 bytes
 * over the header and the sealed bytes, the nonce the link, the direction,
 * six zero bytes and the counter, and a receiver that keeps the highest
 * counter per link and direction with a window of 64 frames. The AEAD is the
 * construction of RFC 8439 section 2.8 through the crate chacha20poly1305.
 */

//! The `LoRa` frame and the replay window.
//!
//! ```text
//! LoRa frame, one LoRa payload of at most 255 bytes
//!
//!  offset  size  field     meaning
//!  0       1     link      the link, which the pairing assigns, 1 to 255
//!  1       4     counter   the sender's frame counter for this link and
//!                          direction, big endian
//!  5       n     sealed    one Homa packet in the compact form, encrypted
//!  5+n     16    tag       the Poly1305 tag over link, counter and sealed
//! ```
//!
//! The nonce of a frame is the link, the byte of the direction, 0 toward
//! the device and 1 toward the client, six zero bytes and the counter big
//! endian, 12 bytes, so it never repeats while the counter of a link and a
//! direction does not. The header of 5 bytes is the associated data of the
//! AEAD. The counter `u32::MAX` marks the counters as spent, and neither
//! [`seal`] nor [`open`] takes it.
//!
//! [`open`] checks a frame in this order and answers the first rule it
//! breaks, before a byte of the packet leaves: [`FrameError::Short`] for a
//! frame shorter than the header and the tag, [`FrameError::TooLong`] for
//! one longer than a `LoRa` payload or whose packet the output cannot hold,
//! [`FrameError::Link`] for another link, [`FrameError::Counter`] for the
//! spent counter, and [`FrameError::Tag`] for a tag that does not verify.
//! The carrier then asks [`Replay::accept`], which refuses a counter it saw
//! or one below its window with [`FrameError::Replay`], so every altered or
//! replayed frame is refused before Homa sees it.

use chacha20poly1305::{
    ChaCha20Poly1305,
    Key,
    Nonce,
    Tag,
    aead::{
        AeadInOut,
        KeyInit, //
    }, //
};

use crate::{
    MAX_PAYLOAD,
    Pairing, //
};

/// The bytes of the header, the link and the counter.
pub const HEADER_LEN: usize = 5;

/// The bytes of the tag.
pub const TAG_LEN: usize = 16;

/// The bytes a frame adds to its packet, 21.
pub const OVERHEAD: usize = HEADER_LEN + TAG_LEN;

/// The longest packet a frame seals, 234 bytes.
pub const MAX_SEALED: usize = MAX_PAYLOAD - OVERHEAD;

/// The counters a receiver's window holds below the highest it saw.
pub const WINDOW: u32 = 64;

/// The direction a frame travels in, which the nonce carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// From the client to the device.
    ToDevice,
    /// From the device to the client.
    ToClient,
}

impl Direction {
    /// Returns the byte of the direction in the nonce, 0 toward the device
    /// and 1 toward the client.
    #[must_use]
    pub const fn byte(self) -> u8 {
        match self {
            Self::ToDevice => 0,
            Self::ToClient => 1,
        }
    }

    /// Returns the other direction.
    #[must_use]
    pub const fn other(self) -> Self {
        match self {
            Self::ToDevice => Self::ToClient,
            Self::ToClient => Self::ToDevice,
        }
    }
}

/// Why a frame was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameError {
    /// The frame is shorter than its header and its tag.
    Short,
    /// The frame names another link.
    Link,
    /// The tag does not verify, so a byte of the frame changed.
    Tag,
    /// The counter was seen already or lies below the window.
    Replay,
    /// The frame, or its packet, is longer than a `LoRa` payload or the
    /// output holds.
    TooLong,
    /// The counters are spent, which the counter `u32::MAX` marks.
    Counter,
}

impl core::fmt::Display for FrameError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::Short => "the frame is shorter than its header and its tag",
            Self::Link => "the frame names another link",
            Self::Tag => "the tag of the frame does not verify",
            Self::Replay => "the counter of the frame was seen or lies below the window",
            Self::TooLong => "the frame is longer than a LoRa payload or its output",
            Self::Counter => "the counters of the link are spent",
        })
    }
}

impl core::error::Error for FrameError {}

/// Returns the nonce of a frame: the link, the byte of the direction, six
/// zero bytes and the counter big endian.
#[must_use]
pub fn nonce(link: u8, direction: Direction, counter: u32) -> [u8; 12] {
    let mut nonce = [0u8; 12];
    nonce[0] = link;
    nonce[1] = direction.byte();
    nonce[8..].copy_from_slice(&counter.to_be_bytes());
    nonce
}

/// Encrypts `buffer` in place under `key` and `nonce` with the associated
/// data `aad`, `AEAD_CHACHA20_POLY1305` of RFC 8439 section 2.8, and returns
/// the tag.
///
/// # Errors
///
/// Returns [`FrameError::TooLong`] where the AEAD refuses the length, which
/// no `LoRa` payload reaches.
pub fn seal_detached(
    key: &[u8; 32],
    nonce: &[u8; 12],
    aad: &[u8],
    buffer: &mut [u8],
) -> Result<[u8; TAG_LEN], FrameError> {
    let cipher = ChaCha20Poly1305::new(&Key::from(*key));
    let tag = cipher
        .encrypt_inout_detached(&Nonce::from(*nonce), aad, buffer.into())
        .map_err(|_| FrameError::TooLong)?;
    Ok(tag.into())
}

/// Verifies `tag` over `aad` and the ciphertext in `buffer` under `key` and
/// `nonce` and decrypts `buffer` in place, `AEAD_CHACHA20_POLY1305` of RFC
/// 8439 section 2.8.
///
/// # Errors
///
/// Returns [`FrameError::Tag`] where the tag does not verify, and leaves
/// `buffer` as it was.
pub fn open_detached(
    key: &[u8; 32],
    nonce: &[u8; 12],
    aad: &[u8],
    buffer: &mut [u8],
    tag: &[u8; TAG_LEN],
) -> Result<(), FrameError> {
    let cipher = ChaCha20Poly1305::new(&Key::from(*key));
    cipher
        .decrypt_inout_detached(&Nonce::from(*nonce), aad, buffer.into(), &Tag::from(*tag))
        .map_err(|_| FrameError::Tag)
}

/// Seals one compact Homa packet into a `LoRa` frame of the design note's
/// section 6.5: a 5 byte header of link and counter, the sealed packet and
/// a 16 byte tag, the nonce the link, the direction, six zero bytes and the
/// counter, and returns the bytes of the frame written to `out`.
///
/// # Errors
///
/// Returns [`FrameError::Counter`] for the counter `u32::MAX`, and
/// [`FrameError::TooLong`] for a packet longer than [`MAX_SEALED`] or a
/// frame `out` does not hold, and writes nothing.
pub fn seal(
    pairing: &Pairing,
    direction: Direction,
    counter: u32,
    packet: &[u8],
    out: &mut [u8],
) -> Result<usize, FrameError> {
    if counter == u32::MAX {
        return Err(FrameError::Counter);
    }
    let len = packet.len() + OVERHEAD;
    if packet.len() > MAX_SEALED || out.len() < len {
        return Err(FrameError::TooLong);
    }
    let (header, rest) = out.split_at_mut(HEADER_LEN);
    header[0] = pairing.link;
    header[1..].copy_from_slice(&counter.to_be_bytes());
    let (body, rest) = rest.split_at_mut(packet.len());
    body.copy_from_slice(packet);
    let tag = seal_detached(
        &pairing.key,
        &nonce(pairing.link, direction, counter),
        header,
        body,
    )?;
    rest[..TAG_LEN].copy_from_slice(&tag);
    Ok(len)
}

/// Opens a `LoRa` frame and returns its counter and the packet, before Homa
/// sees it.
///
/// # Errors
///
/// Returns the first rule of the module's order the frame breaks, and hands
/// out no byte of the packet then.
pub fn open<'a>(
    pairing: &Pairing,
    direction: Direction,
    frame: &[u8],
    out: &'a mut [u8],
) -> Result<(u32, &'a [u8]), FrameError> {
    if frame.len() < OVERHEAD {
        return Err(FrameError::Short);
    }
    let len = frame.len() - OVERHEAD;
    if frame.len() > MAX_PAYLOAD || out.len() < len {
        return Err(FrameError::TooLong);
    }
    let (header, rest) = frame.split_at(HEADER_LEN);
    if header[0] != pairing.link {
        return Err(FrameError::Link);
    }
    let counter = u32::from_be_bytes([header[1], header[2], header[3], header[4]]);
    if counter == u32::MAX {
        return Err(FrameError::Counter);
    }
    let (sealed, tag) = rest.split_at(len);
    let mut tag_bytes = [0u8; TAG_LEN];
    tag_bytes.copy_from_slice(tag);
    let body = &mut out[..len];
    body.copy_from_slice(sealed);
    if let Err(error) = open_detached(
        &pairing.key,
        &nonce(pairing.link, direction, counter),
        header,
        body,
        &tag_bytes,
    ) {
        body.fill(0);
        return Err(error);
    }
    Ok((counter, &out[..len]))
}

/// The highest counter a receiver saw on a link and direction, and a window
/// of the 64 counters up to it.
///
/// A counter above the highest moves the window up, a counter inside the
/// window is taken once, and a counter below it is refused, as the design
/// note's section 6.5 asks. The window lives in the memory of the
/// receiver, so a receiver that restarts starts a new window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Replay {
    /// The highest counter taken.
    highest: u32,
    /// Bit `i` set where the counter `highest - i` was taken.
    seen: u64,
    /// Whether any counter was taken.
    any: bool,
}

impl Replay {
    /// Returns a window that has taken no counter.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            highest: 0,
            seen: 0,
            any: false,
        }
    }

    /// Returns the highest counter taken, `None` before the first.
    #[must_use]
    pub const fn highest(&self) -> Option<u32> {
        if self.any { Some(self.highest) } else { None }
    }

    /// Returns whether `counter` would be taken, without taking it.
    ///
    /// # Errors
    ///
    /// Returns [`FrameError::Replay`] for a counter taken before or below
    /// the window.
    pub fn check(&self, counter: u32) -> Result<(), FrameError> {
        if !self.any || counter > self.highest {
            return Ok(());
        }
        let back = self.highest - counter;
        if back >= WINDOW || self.seen & (1u64 << back) != 0 {
            return Err(FrameError::Replay);
        }
        Ok(())
    }

    /// Takes `counter` into the window.
    ///
    /// # Errors
    ///
    /// Returns [`FrameError::Replay`] for a counter taken before or below
    /// the window, and changes nothing.
    pub fn accept(&mut self, counter: u32) -> Result<(), FrameError> {
        self.check(counter)?;
        if !self.any {
            self.any = true;
            self.highest = counter;
            self.seen = 1;
        } else if counter > self.highest {
            let ahead = counter - self.highest;
            self.seen = if ahead >= WINDOW {
                1
            } else {
                (self.seen << ahead) | 1
            };
            self.highest = counter;
        } else {
            self.seen |= 1u64 << (self.highest - counter);
        }
        Ok(())
    }
}
