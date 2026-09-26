// SPDX-License-Identifier: MIT
/*
 * aosabi-validate pass scalars: the question the single pass asks a custom
 * scalar about a literal.
 *
 * No analogue in the reference.
 */

//! The custom scalars of the single pass.
//!
//! The rule for values of the correct type asks a scalar about a literal
//! through the literal parse of that scalar, and reads a result of
//! `undefined` or a thrown error as a refusal. The five specified scalars
//! answer by rules the pass knows, and a custom scalar answers through
//! [`Scalars`], which a table built from a schema of the type layer fills
//! with the literal parse of each custom scalar ([󰀂](super::SchemaScalars)).
//! A table with no scalar handles, such as the static form, answers through
//! [`NoScalars`], which leaves every literal at a custom scalar to the rules.
//!
//! The pass asks only about a literal of a scalar kind. A `null` keeps the
//! check of a non null position that the reference makes apart from the
//! scalar, and a list or an object literal at a custom scalar defers.

/// A literal of a scalar kind at a custom scalar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Literal<'a> {
    /// An integer literal, its digits as written.
    Int(&'a [u8]),
    /// A float literal, its text as written.
    Float(&'a [u8]),
    /// A string or a block string literal with its value.
    String {
        /// The value, its escapes resolved and a block string dedented.
        value: &'a [u8],
        /// Whether the document wrote a block string.
        block: bool,
    },
    /// `true` or `false`.
    Boolean(bool),
    /// An enum value, its name.
    Enum(&'a [u8]),
}

/// What a custom scalar answers about a literal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    /// The literal parse takes the literal.
    Accept,
    /// The literal parse throws or answers `undefined`.
    Refuse,
    /// The scalar cannot be asked, and the rules decide.
    Defer,
}

/// The custom scalars of a table, which answer for a literal.
pub trait Scalars {
    /// Returns what the custom scalar `scalar` answers about `literal`.
    ///
    /// `scalar` is the index of [`Scalar::Custom`](super::table::Scalar::Custom).
    fn literal(&self, scalar: u16, literal: Literal<'_>) -> Answer;
}

/// The scalars of a table that holds no handle, which defer every literal.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoScalars;

impl Scalars for NoScalars {
    fn literal(&self, _scalar: u16, _literal: Literal<'_>) -> Answer {
        Answer::Defer
    }
}
