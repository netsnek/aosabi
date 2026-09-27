// SPDX-License-Identifier: MIT
/*
 * graphtp credential: the value of `Authorization` a request carries between
 * its head and its content, never compressed.
 *
 * No analogue in the reference. The place of the credential and its checks are
 * those of the specification.
 */

//! The credential of a request, checked for its bytes and for its length.
//!
//! The credential is the value an HTTP client puts in `Authorization`,
//! verbatim, such as `Bearer eyJ...`. It stands between the head and the
//! content, outside the zlib stream of the content, so no secret shares a
//! compression context with bytes a caller chooses. A credential is a JWT or
//! another token in the form HTTP carries, so every byte is visible ASCII or
//! the space, and the crate reads it as bytes and never as text.

use crate::status::Code;

/// The value of `Authorization` a request carries, checked for its bytes and
/// for the length a request head can carry.
///
/// The check refuses every byte outside `0x20` to `0x7E`, the visible ASCII
/// and the space that separates the scheme from the token, since a credential
/// is a JWT or another token in the form HTTP carries, and a length above
/// 65,535 bytes, so every value of this type can be written into a request. An
/// empty credential is none, which a request writes with the length 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Credential<'a> {
    /// The bytes of the value.
    bytes: &'a [u8],
    /// The length of the value, which the head of a request carries.
    len: u16,
}

impl<'a> Credential<'a> {
    /// The longest credential a request head can carry, 65,535 bytes.
    pub const MAX_LEN: u16 = u16::MAX;

    /// Returns the credential of the bytes `bytes`.
    ///
    /// The checks are those of rule 6 of `docs/graphtp.md` section 5.3 for the
    /// bytes, then the length of the head for the size, in that order, as a
    /// server answers them.
    ///
    /// # Errors
    ///
    /// Returns [`Code::MALFORMED_MESSAGE`] when a byte is outside `0x20` to
    /// `0x7E`, and [`Code::REQUEST_HEADER_FIELDS_TOO_LARGE`] when the value is
    /// longer than [`Credential::MAX_LEN`].
    pub fn new(bytes: &'a [u8]) -> Result<Credential<'a>, Code> {
        if !bytes.iter().all(|&byte| matches!(byte, 0x20..=0x7E)) {
            return Err(Code::MALFORMED_MESSAGE);
        }
        let Ok(len) = u16::try_from(bytes.len()) else {
            return Err(Code::REQUEST_HEADER_FIELDS_TOO_LARGE);
        };
        Ok(Credential { bytes, len })
    }

    /// Returns the credential when it is within the server's bound `max`.
    ///
    /// This is rule 7 of `docs/graphtp.md` section 5.3, which a server checks
    /// after rule 6.
    ///
    /// # Errors
    ///
    /// Returns [`Code::REQUEST_HEADER_FIELDS_TOO_LARGE`] when the credential is
    /// longer than `max` bytes.
    pub fn within(self, max: u16) -> Result<Credential<'a>, Code> {
        if self.len > max {
            return Err(Code::REQUEST_HEADER_FIELDS_TOO_LARGE);
        }
        Ok(self)
    }

    /// Returns the bytes of the credential.
    #[must_use]
    pub const fn as_bytes(self) -> &'a [u8] {
        self.bytes
    }

    /// Returns the length of the credential, which the head of a request
    /// carries.
    #[must_use]
    pub const fn len(self) -> u16 {
        self.len
    }

    /// Tells whether the credential is empty, which is none.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.len == 0
    }
}
