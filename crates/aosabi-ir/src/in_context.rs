// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-ir: the SDL of the `@inContext` directive, the tail every emitted
 * schema carries.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/core/in-context.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 *
 * The text itself is the extracted literal
 * crates/aosabi-core/src/in_context.graphql, which tools/provenance writes and
 * nothing else does.
 */

//! The directive tail of an emitted schema.
//!
//! The reference assembles `schema.graphql` as the printed IR, a blank line,
//! `IN_CONTEXT_SDL` and one newline. The constant carries U+2014 inside a
//! description, quirk `Q11_IN_CONTEXT_EM_DASH`, so it is never typed by hand.
//! The extractor evaluates the escapes of the template literal and writes the
//! evaluated string, and this module includes that file.

/// The SDL of the `@inContext` directive.
///
/// The text ends without a newline, because the literal file carries none, and
/// the build appends the newline that `schema.graphql` ends in. The literal
/// stays in the crate directory that `NOTICE` declares for it, which is why the
/// path leaves this crate.
pub const IN_CONTEXT_SDL: &str = include_str!("../../aosabi-core/src/in_context.graphql");
