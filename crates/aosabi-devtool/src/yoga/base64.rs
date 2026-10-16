// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool yoga: Base64 with the standard alphabet and padding.
 *
 * No analogue in the reference. The alphabet and the padding are those of RFC
 * 4648 section 4, which Python's base64.b64encode writes and reads on the
 * notebook side of the seam.
 */

//! `encode` and `decode`.
//!
//! The request and the response lines of the primitives `yoga` and `http`
//! carry bytes as Base64, because a body or a write of the wire is not always
//! UTF-8 and a JSON string carries text alone.

/// The 64 characters of the standard alphabet, RFC 4648 section 4.
const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Returns `bytes` as Base64 with padding.
pub fn encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        let sextets = [
            b0 >> 2,
            (b0 & 0x03) << 4 | b1 >> 4,
            (b1 & 0x0f) << 2 | b2 >> 6,
            b2 & 0x3f,
        ];
        for (index, sextet) in sextets.into_iter().enumerate() {
            if index <= chunk.len() {
                out.push(char::from(ALPHABET[usize::from(sextet)]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Returns the bytes of the Base64 text `text`, or `None` when it is not
/// Base64 with padding.
pub fn decode(text: &str) -> Option<Vec<u8>> {
    let bytes = text.as_bytes();
    if !bytes.len().is_multiple_of(4) {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for (index, quad) in bytes.chunks(4).enumerate() {
        let last = index + 1 == bytes.len() / 4;
        let pad = quad.iter().rev().take_while(|&&byte| byte == b'=').count();
        if pad > 2 || (pad > 0 && !last) {
            return None;
        }
        let mut value = 0_u32;
        for &byte in &quad[..4 - pad] {
            value = value << 6 | u32::from(sextet(byte)?);
        }
        value <<= 6 * pad;
        let [_, high, middle, low] = value.to_be_bytes();
        out.extend_from_slice(&[high, middle, low][..3 - pad]);
    }
    Some(out)
}

/// Returns the value of one character of the alphabet.
fn sextet(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}
