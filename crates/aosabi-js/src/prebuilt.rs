// SPDX-License-Identifier: MIT
/*
 * aosabi-js prebuilt: the schema record of a build, which the export
 * schema_record writes from the SDL and the IR of the build and the boot
 * reads where the host hands it in place of the IR.
 *
 * No analogue in the reference.
 */

//! The schema record of a build.
//!
//! A build hands the export `schema_record` the record of the step it asks
//! for: the byte 0, then the SDL of the build and the JSON text of its IR,
//! the two texts the boot record of the app carries, to write the schema
//! record of the two, or, in a test build with the feature `print`, the byte
//! 1, then the SDL and a schema record of it, to print the schema the record
//! builds and the schema the SDL builds, each with its type map and its
//! directives, and the schema the record builds with no bindings. The answer
//! is the byte 0 and what the step answers, the record or the three printed
//! schemas each with its length in front, or the byte 1 and the message of
//! what kept it from that answer. The record is UTF-8 text, which the host
//! hands the app as a string and the boot record carries as it carries the
//! IR.
//!
//! A boot reads the text in the place of the IR as a schema record where it
//! opens with the magic of the record, which no JSON text opens with, and as
//! the IR otherwise, so a host that hands the IR boots from the SDL as
//! before.

#![forbid(unsafe_code)]

use aosabi_core::boot::{
    CanaryResolver,
    SchemaRecord,
    canary_schema_record,
    prebuilt, //
};

use crate::host::{
    self,
    Reader, //
};

/// The step that writes the schema record of the SDL and the IR.
const WRITE: u8 = 0;

/// The step that prints the schema a record builds and the schema its SDL
/// builds.
const PRINT: u8 = 1;

/// The message of a record of a step that ends early or names no step.
const BROKEN: &str = "the record of the step of the schema record is broken";

/// Answers the step of the record in the first `len` bytes of the input
/// buffer and returns the address of the answer in the output buffer.
pub fn schema_record(len: u32) -> usize {
    let input = host::input(len);
    let mut reader = Reader::new(&input);
    let answer = match reader.byte() {
        Some(WRITE) => write(&mut reader),
        Some(PRINT) => print(&mut reader),
        _ => Err(String::from(BROKEN)),
    };
    let mut out = Vec::new();
    match answer {
        Ok(bytes) => {
            out.push(0);
            out.extend_from_slice(&bytes);
        }
        Err(message) => {
            out.push(1);
            out.extend_from_slice(message.as_bytes());
        }
    }
    host::output(&out)
}

/// Writes the schema record of the SDL and the IR at the cursor of `reader`.
fn write(reader: &mut Reader<'_>) -> Result<Vec<u8>, String> {
    let type_defs = reader.text().ok_or(BROKEN)?;
    let ir = reader.text().ok_or(BROKEN)?;
    prebuilt::write(type_defs, ir)
}

/// Prints the schema the record at the cursor of `reader` builds and the
/// schema its SDL builds, with the type map and the directives of each, and
/// the schema the record builds with no bindings, each with its length in
/// front.
#[cfg(feature = "print")]
fn print(reader: &mut Reader<'_>) -> Result<Vec<u8>, String> {
    let type_defs = reader.text().ok_or(BROKEN)?;
    let record = reader.text().ok_or(BROKEN)?;
    let mut out = Vec::new();
    for printed in prebuilt::print(type_defs, record)? {
        host::push_count(&mut out, printed.len());
        out.extend_from_slice(printed.as_bytes());
    }
    Ok(out)
}

/// Answers the step that prints in a module with no feature `print`, which
/// prints no schema.
#[cfg(not(feature = "print"))]
fn print(_reader: &mut Reader<'_>) -> Result<Vec<u8>, String> {
    Err(String::from(
        "the module prints no schema, which a test build with the feature print does",
    ))
}

/// Reads the schema record of a boot from the SDL `type_defs` and the text
/// `ir` the host hands in the place of the IR, a schema record where it
/// opens with its magic and the JSON text of the IR otherwise, with the type
/// resolvers `canary` of a build of the canary.
///
/// # Errors
///
/// Returns what the read of the record or of the IR returns.
pub fn schema_record_of(
    type_defs: &str,
    ir: &str,
    canary: &[CanaryResolver],
) -> Result<SchemaRecord, String> {
    if prebuilt::is_record(ir) {
        return prebuilt::schema_record(type_defs, ir, canary);
    }
    canary_schema_record(type_defs, ir, canary)
}
