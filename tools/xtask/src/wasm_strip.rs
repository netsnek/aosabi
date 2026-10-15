// SPDX-License-Identifier: MIT
/*
 * xtask wasm_strip: the section walker that strips a wasm module.
 *
 * It drops every custom section and refuses a module with an import, over the
 * section list of the WebAssembly core specification.
 */

//! The section walker.
//!
//! A module is the preamble and a list of sections, each a byte of its id, its
//! length as an unsigned LEB128 `u32` and its contents. Custom sections may
//! stand anywhere, and every other section at most once and in the order of
//! [`ORDER`]. The walker keeps every section but the custom ones byte for
//! byte, reads no contents but the count of the import section, and refuses
//! what breaks that list, so the module it writes holds the code it was given.

use std::fmt;

/// The magic `\0asm` and the version 1 that open every module.
const PREAMBLE: [u8; 8] = [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

/// The id of a custom section.
const CUSTOM: u8 = 0;

/// The id of the import section.
const IMPORT: u8 = 2;

/// The ids of the other sections in the order a module holds them: type,
/// import, function, table, memory, tag, global, export, start, element, data
/// count, code and data.
const ORDER: [u8; 13] = [1, 2, 3, 4, 5, 13, 6, 7, 8, 9, 12, 10, 11];

/// The most bytes an unsigned LEB128 `u32` takes, `ceil(32 / 7)`.
const U32_BYTES: usize = 5;

/// Why the walker refuses a module, with the byte offset of the cause.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StripError {
    /// The module does not open with the magic and the version 1.
    Preamble,
    /// A length or a section runs past the end of what holds it.
    Truncated(usize),
    /// A `u32` takes more than five bytes or sets a bit above bit 31.
    Overlong(usize),
    /// A section id is unknown, repeated or out of [`ORDER`].
    Order {
        /// The id of the section.
        id: u8,
        /// The offset of its id byte.
        at: usize,
    },
    /// The import section holds this many imports.
    Import(u32),
    /// The import section holds bytes after a count of 0.
    Trailing(usize),
}

impl fmt::Display for StripError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Preamble => f.write_str("the module does not open with \\0asm and version 1"),
            Self::Truncated(at) => write!(f, "the module ends early at byte offset {at}"),
            Self::Overlong(at) => {
                write!(
                    f,
                    "the u32 at byte offset {at} is longer than five bytes or 32 bits"
                )
            }
            Self::Order { id, at } => {
                write!(
                    f,
                    "section id {id} at byte offset {at} is unknown or out of order"
                )
            }
            Self::Import(count) => {
                write!(
                    f,
                    "the import section counts {count} and the module may import nothing"
                )
            }
            Self::Trailing(at) => {
                write!(f, "the import section has bytes left at byte offset {at}")
            }
        }
    }
}

/// Returns `module` without its custom sections.
///
/// Every other section is copied with its id, its length as written and its
/// contents, in the order the module holds them.
///
/// # Errors
///
/// Returns [`StripError::Import`] when the module imports anything, and the
/// other variants of [`StripError`] when it breaks the section list.
pub fn strip(module: &[u8]) -> Result<Vec<u8>, StripError> {
    if !module.starts_with(&PREAMBLE) {
        return Err(StripError::Preamble);
    }
    let mut out = Vec::with_capacity(module.len());
    out.extend_from_slice(&PREAMBLE);
    let mut at = PREAMBLE.len();
    let mut next = 0;
    while let Some(&id) = module.get(at) {
        let (len, body) = read_u32(module, at + 1)?;
        let end = usize::try_from(len)
            .ok()
            .and_then(|len| body.checked_add(len))
            .filter(|&end| end <= module.len())
            .ok_or(StripError::Truncated(at))?;
        if id != CUSTOM {
            next = rank(id, next).ok_or(StripError::Order { id, at })?;
            if id == IMPORT {
                refuse_imports(&module[..end], body)?;
            }
            out.extend_from_slice(&module[at..end]);
        }
        at = end;
    }
    Ok(out)
}

/// Returns the place after `id` in [`ORDER`] when `id` may follow the
/// sections before `next`.
fn rank(id: u8, next: usize) -> Option<usize> {
    let place = ORDER.iter().position(|&known| known == id)?;
    (place >= next).then_some(place + 1)
}

/// Refuses an import section whose count, read at `at` of `bytes`, is not 0
/// or is followed by more bytes before the end of `bytes`.
fn refuse_imports(bytes: &[u8], at: usize) -> Result<(), StripError> {
    match read_u32(bytes, at)? {
        (0, end) if end == bytes.len() => Ok(()),
        (0, end) => Err(StripError::Trailing(end)),
        (count, _) => Err(StripError::Import(count)),
    }
}

/// Reads the unsigned LEB128 `u32` at `at` of `bytes` and returns it with the
/// offset after it.
///
/// A `u32` takes at most five bytes, and the fifth carries bits 28 to 31
/// alone. A fifth byte with a bit above them is refused at once, and one that
/// asks for a sixth byte ends the loop.
fn read_u32(bytes: &[u8], at: usize) -> Result<(u32, usize), StripError> {
    let mut value = 0u32;
    for i in 0..U32_BYTES {
        let &byte = bytes.get(at + i).ok_or(StripError::Truncated(at))?;
        if i == U32_BYTES - 1 && byte & 0x70 != 0 {
            return Err(StripError::Overlong(at));
        }
        value |= u32::from(byte & 0x7f) << (7 * i);
        if byte & 0x80 == 0 {
            return Ok((value, at + i + 1));
        }
    }
    Err(StripError::Overlong(at))
}
