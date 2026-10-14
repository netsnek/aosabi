// SPDX-License-Identifier: MIT
/*
 * aoviewer-core limits: the bounds every reader of the core holds whatever it
 * is fed.
 *
 * The core parses untrusted bytes in one place, the reader, and every nesting,
 * count and length it accepts is capped here.
 */

//! The read limits of the core.

/// The bounds every reader holds whatever it is fed.
///
/// The JSON pull parser, the model builder and the `commonmark` subset read
/// these bounds, so a hostile document ends in an error variant and never in
/// unbounded memory or time. [`ReadLimits::DEFAULT`] carries the values of the
/// plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadLimits {
    /// The length of an input document in bytes.
    pub max_input_bytes: u32,
    /// The nesting of JSON containers, of `commonmark` blocks and of the
    /// wrappers of a type reference.
    pub max_depth: u16,
    /// The number of named types, and separately the number of directives.
    pub max_types: u32,
    /// The number of fields, arguments, enum values and input fields
    /// together.
    pub max_members: u32,
    /// The length of a name in bytes.
    pub max_name_bytes: u16,
    /// The length of a description, a deprecation reason, a default value or
    /// a specification URL in bytes.
    pub max_description_bytes: u32,
}

impl ReadLimits {
    /// The limits a reader holds unless its caller passes others.
    ///
    /// The input is capped at 64 MiB, nesting at 128, types at 20,000, members
    /// at 400,000, a name at 1024 bytes and a description at 1 MiB.
    pub const DEFAULT: ReadLimits = ReadLimits {
        max_input_bytes: 64 << 20,
        max_depth: 128,
        max_types: 20_000,
        max_members: 400_000,
        max_name_bytes: 1024,
        max_description_bytes: 1 << 20,
    };
}

impl Default for ReadLimits {
    fn default() -> Self {
        Self::DEFAULT
    }
}
