// SPDX-License-Identifier: MIT
/*
 * graphtp body: the threshold, >128 B, and the first byte rule that tells a
 * plain body from a coded one.
 *
 * No analogue in the reference. The threshold and the rule are those of the
 * specification, and the zlib header they read is that of RFC 1950 section 2.2
 * under the profile of section 3.2.
 */

//! The threshold, >128 B, and the coding of a body by its first byte.

use crate::wire::WireError;

/// The longest content or body a sender sends plain, 128 bytes.
///
/// A sender codes exactly the contents and bodies of more than 128 bytes, the
/// rule written >128 B, and sends every one of at most 128 bytes plain, and a
/// receiver decodes by the compressed bit of a request or the first byte of an
/// answer and never refuses either form at any length.
pub const THRESHOLD: usize = 128;

/// The coding of a content or a body.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Coding {
    /// The bytes as the service or the client wrote them.
    Plain,
    /// One zlib stream of the profile of `docs/graphtp.md` section 3.2.
    Zlib,
}

/// Returns the coding of the answer body `body` by its first byte, or `None`
/// for an empty body.
///
/// The brace and the bracket open plain JSON, and `0x08`, `0x18`, `0x28`,
/// `0x38` and `0x48` open one zlib stream when the second byte completes a
/// header of the profile: FCHECK makes `CMF * 256 + FLG` a multiple of 31 and
/// FDICT is clear. No zlib header opens with the brace or the bracket, since
/// CM 8 puts 8 in the low four bits of the first byte and those two bytes hold
/// 11 there.
///
/// # Errors
///
/// Returns [`WireError::Coding`] for a body that opens with any other byte, or
/// with a zlib opener whose second byte completes no header of the profile or
/// is missing.
pub fn coding_of(body: &[u8]) -> Result<Option<Coding>, WireError> {
    match *body {
        [] => Ok(None),
        [b'{' | b'[', ..] => Ok(Some(Coding::Plain)),
        [cmf, flg, ..] if opens_zlib(cmf, flg) => Ok(Some(Coding::Zlib)),
        _ => Err(WireError::Coding),
    }
}

/// Tells whether `cmf` and `flg` are a zlib header of the profile of
/// `docs/graphtp.md` section 3.2.
///
/// CM is the low four bits of `cmf` and must be 8, CINFO its high four bits
/// and at most 4, FCHECK makes `cmf * 256 + flg` a multiple of 31, and FDICT,
/// bit 5 of `flg`, is clear. FLEVEL is never read.
fn opens_zlib(cmf: u8, flg: u8) -> bool {
    let method = cmf & 0x0F == 8;
    let window = cmf >> 4 <= 4;
    let check = (u16::from(cmf) * 256 + u16::from(flg)) % 31 == 0;
    let dictionary = flg & 0x20 != 0;
    method && window && check && !dictionary
}
