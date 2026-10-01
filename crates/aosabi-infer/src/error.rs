// SPDX-License-Identifier: MIT
/*
 * aosabi-infer: the error of the inference, which stands in for the
 * `throw new Error` sites of the five translated files and for the one read
 * of `undefined` that the engine of the reference refuses.
 *
 * No file of the reference is translated here. The read of `undefined` is the
 * `schemaType` of schema-parser.ts:1116-1118 and its `args` at :1143 and
 * :1176. The two refusals of a spread are the inference's own, because the
 * TypeScript checker presents every object spread it accepts.
 */

//! The error of the inference.

use core::fmt;

use aosabi_ir::CollateError;

/// Says why an inference stopped.
///
/// The inference keeps the behaviour of its TypeScript sources and adds three
/// refusals of its own,
/// [`InferError::Sdl`], [`InferError::SpreadNotDescribed`] and
/// [`InferError::SpreadCycle`]. Every other refusal of Aosabi is a compile
/// error before the inference runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InferError {
    /// The reference throws at this point, and the text is the message of its
    /// `Error`, copied from the vendored file.
    Thrown(String),
    /// The reference reads a member of `undefined` at this point, so it fails
    /// with a runtime `TypeError` of its engine and with no `Error` of its
    /// own.
    ///
    /// The text of that `TypeError` is the engine's. No vendored file holds
    /// it and it is outside the byte contract, so the variant carries the
    /// reason and no message.
    ///
    /// [to-object]: https://tc39.es/ecma262/#sec-toobject
    ReadOfUndefined {
        /// The expression of the vendored file whose value is `undefined`.
        construct: &'static str,
        /// The member the reference reads from it.
        member: String,
    },
    /// A type name leaves the alphabet of the collation that orders an
    /// `implements` list, so the printer refuses the schema.
    Sdl(CollateError),
    /// A field under `#[aosabi(spread)]` has a type that describes no object,
    /// so there are no members to present in place of the field.
    SpreadNotDescribed {
        /// The struct that declares the field.
        owner: String,
        /// The field under `#[aosabi(spread)]`.
        field: String,
    },
    /// A field under `#[aosabi(spread)]` has a type whose spreads reach back
    /// to the struct that declares the field, a spread type the checker has
    /// no finite answer for.
    SpreadCycle {
        /// The struct that declares the field.
        owner: String,
        /// The field under `#[aosabi(spread)]`.
        field: String,
    },
}

impl fmt::Display for InferError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Thrown(message) => f.write_str(message),
            Self::ReadOfUndefined { construct, member } => write!(
                f,
                "the reference reads the member \"{member}\" of `{construct}`, which is undefined there, and its engine refuses that read with a TypeError"
            ),
            Self::Sdl(error) => fmt::Display::fmt(error, f),
            Self::SpreadNotDescribed { owner, field } => write!(
                f,
                "the field \"{field}\" of \"{owner}\" stands under #[aosabi(spread)], and its type describes no object whose members could stand in place of the field"
            ),
            Self::SpreadCycle { owner, field } => write!(
                f,
                "the field \"{field}\" of \"{owner}\" stands under #[aosabi(spread)], and the spreads of its type reach back to \"{owner}\", so its members have no finite answer"
            ),
        }
    }
}

impl core::error::Error for InferError {}

impl From<CollateError> for InferError {
    fn from(error: CollateError) -> Self {
        Self::Sdl(error)
    }
}
