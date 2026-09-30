// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-auth-contract: the error an application throws when a caller lacks a
 * permission.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/auth/errors.ts,
 * Apache-2.0. Changed by the Aosabi authors.
 */

//! `ForbiddenError`.
//!
//! The one authorization error of Aosabi, thrown by a capability gate and by
//! the row policies and feature gates of the ORM, so there is one class
//! everywhere. It carries the code `FORBIDDEN`, the status 403 and, for the
//! denial of a feature gate, the feature. The module names no crate but
//! `core`, `alloc` and the JSON values of `aosabi-json`, as the reference
//! module imports nothing, so a data layer names the error without the web
//! framework. `aosabi-core` converts it into the value a resolver throws.
//!
//! The message is a JavaScript string and the feature any value, as the
//! constructor of the reference takes them, so an error a host of the
//! TypeScript option made keeps both as the host holds them: a message with a
//! lone surrogate among its code units and a feature that is a number or an
//! object, which the map of the identity plugin writes into the error
//! `FORBIDDEN` as `authz.ts:57-70` writes them. An error a Rust app makes
//! takes a string for both. A host tests the truth of the feature on the
//! value it holds, which the copy may not keep, so under the feature
//! `host-errors` the error carries that truth beside the copy.

use core::fmt::{
    self,
    Write as _, //
};

use aosabi_json::{
    JsString,
    JsValue, //
};

/// The error of a denied permission, `ForbiddenError`.
#[derive(Clone, Debug, PartialEq)]
pub struct ForbiddenError {
    /// The message, `Not permitted.` by default.
    pub message: JsString,
    /// The feature whose gate denied the call, `feature`, as the copy JSON
    /// writes of the value, and `None` where the member holds `undefined`.
    pub feature: Option<JsValue>,
    /// Whether a host tested the feature true on the value it holds, so the
    /// map writes the copy whatever the truth of the copy, as the map of the
    /// reference tests the value and writes its copy.
    #[cfg(feature = "host-errors")]
    pub feature_true: bool,
}

impl ForbiddenError {
    /// The code, `readonly code = 'FORBIDDEN'`.
    pub const CODE: &'static str = "FORBIDDEN";

    /// The status, `readonly statusCode = 403`.
    pub const STATUS_CODE: f64 = 403.0;

    /// The name of the class, `this.name = 'ForbiddenError'`.
    pub const NAME: &'static str = "ForbiddenError";

    /// The message of an error made with none, `message = 'Not permitted.'`.
    pub const DEFAULT_MESSAGE: &'static str = "Not permitted.";

    /// Returns `new ForbiddenError()`.
    #[must_use]
    pub fn new() -> Self {
        Self::with_message(Self::DEFAULT_MESSAGE)
    }

    /// Returns `new ForbiddenError(message)`.
    #[must_use]
    pub fn with_message(message: impl Into<JsString>) -> Self {
        Self {
            message: message.into(),
            feature: None,
            #[cfg(feature = "host-errors")]
            feature_true: false,
        }
    }

    /// Returns the error with the feature whose gate denied the call,
    /// `new ForbiddenError(message, feature)`.
    #[must_use]
    pub fn with_feature(self, feature: impl Into<JsValue>) -> Self {
        Self {
            feature: Some(feature.into()),
            ..self
        }
    }

    /// Returns the feature the map of the identity plugin writes into the
    /// error `FORBIDDEN`, the feature where it is true, `ToBoolean` of
    /// ECMA-262, and `None` otherwise.
    ///
    /// A feature the copy of JSON leaves out, a function or a symbol of a
    /// host, answers `None` as well, which writes the error as the reference
    /// writes it, since its JSON leaves the member out. With the feature
    /// `host-errors` a feature a host tested true answers its copy whatever
    /// the truth of the copy, the `null` of an invalid `Date`, the `0` of a
    /// `toJSON` and the `false` and the empty string of a boxed primitive
    /// among them, and `None` where the copy is `undefined`.
    #[must_use]
    pub fn true_feature(&self) -> Option<&JsValue> {
        #[cfg(feature = "host-errors")]
        if self.feature_true {
            return self
                .feature
                .as_ref()
                .filter(|feature| !feature.is_undefined());
        }
        self.feature.as_ref().filter(|feature| feature.truthy())
    }
}

impl Default for ForbiddenError {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ForbiddenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: ", Self::NAME)?;
        write_lossy(f, self.message.as_wtf8())
    }
}

impl core::error::Error for ForbiddenError {}

/// Writes the WTF-8 bytes `bytes` as text, each lone surrogate, the three
/// bytes of its sequence, as U+FFFD.
// A `from_utf8_unchecked` would skip the recheck.
fn write_lossy(f: &mut fmt::Formatter<'_>, mut bytes: &[u8]) -> fmt::Result {
    loop {
        match core::str::from_utf8(bytes) {
            Ok(text) => return f.write_str(text),
            Err(error) => {
                let (valid, rest) = bytes.split_at(error.valid_up_to());
                f.write_str(core::str::from_utf8(valid).unwrap_or_default())?;
                f.write_char(char::REPLACEMENT_CHARACTER)?;
                bytes = rest.get(3..).unwrap_or_default();
            }
        }
    }
}
