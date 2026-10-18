// SPDX-License-Identifier: MIT
/*
 * aosabi-infer-wasm infer: the run of the inference over the type source of
 * the host, the writers of the three files and the answer record.
 *
 * No file of the reference is translated here.
 */

//! The run of the inference and its answer.
//!
//! [`infer`] runs [`SchemaBuilder::build`] over the type source of the host,
//! which is `build` of the schema builder with no contribution of the database
//! layer, the path every app without the object relational mapper takes. The
//! answer carries the schema text, the JSON form of the IR as `JSON.stringify`
//! writes it, the text of `resolvers.js`, the warnings of the inference in
//! their order, and the verdict of the validation of the schema text with the
//! message the reference throws, or the refusal of the inference with its
//! message, and then the first record of the host the module could not read.
//! The command writes the files and fails where the answer says so, before it
//! writes a byte of an invalid schema, as `emitServerGlue` fails.
//!
//! The answer record opens with a byte, 0 where the inference built the
//! schema and 1 where it refused. A built schema follows with the three texts,
//! a count and the text of each warning, and a byte 0 for a valid schema or a
//! byte 1 and the message of an invalid one. A refusal follows with its
//! message. Both close with a byte 0, or a byte 1 and the failure to read a
//! record of the host.
//!
//! [`SchemaBuilder::build`]: aosabi_infer::SchemaBuilder::build

#![forbid(unsafe_code)]

use aosabi_build::{
    assert_schema_is_valid,
    resolvers_js, //
};
use aosabi_infer::SchemaBuilder;

use crate::{
    host::{
        self,
        push_count,
        push_text, //
    },
    source::HostSource, //
};

/// Runs the inference over the type source of the host and returns the
/// address of the answer record in the output buffer.
pub fn infer() -> usize {
    let source = HostSource::default();
    let mut answer = Vec::new();

    match SchemaBuilder::new(&source).build() {
        Ok(built) => {
            answer.push(0);
            push_text(&mut answer, &built.type_defs);
            push_text(&mut answer, &aosabi_ir::to_json(&built.ir));
            push_text(&mut answer, &resolvers_js(&built));
            push_count(&mut answer, built.warnings.len());
            for warning in &built.warnings {
                push_text(&mut answer, warning);
            }
            match assert_schema_is_valid(&built.type_defs) {
                Ok(()) => answer.push(0),
                Err(invalid) => {
                    answer.push(1);
                    push_text(&mut answer, &invalid.to_string());
                }
            }
        }
        Err(refused) => {
            answer.push(1);
            push_text(&mut answer, &refused.to_string());
        }
    }

    match source.first_failure() {
        None => answer.push(0),
        Some(failure) => {
            answer.push(1);
            push_text(&mut answer, &failure);
        }
    }

    host::output(&answer)
}
