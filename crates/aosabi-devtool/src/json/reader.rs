// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool json reader: the JSON reader behind --json-file, which is
 * the reader of aosabi-json.
 *
 * No analogue in the reference. The reader lives in the served crate, so that
 * a request line of a service and the input of this tool are read by one
 * grammar, and this module keeps the path the subcommands import.
 */

//! The reader of the `--json-file` input, which is `aosabi_json::reader`.

pub use aosabi_json::reader::parse;
