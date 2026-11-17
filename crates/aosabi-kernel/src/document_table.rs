// SPDX-License-Identifier: MIT
/*
 * aosabi-kernel document_table: the persisted document table, withdrawn.
 *
 * No analogue in the reference. The table was to map the id of a persisted
 * document to the generated resolver plan a closed build carries in place of
 * a parser, and it was withdrawn with the closed executor and the CBOR
 * codec.
 */

//! The persisted document table, withdrawn.
//!
//! The module holds no item. A carrier sends the text of a document, which
//! the engine parses and validates at run time, and the general executor and
//! the service answer an operation that names a persisted document with one
//! error, since the engine accepts no persisted document.
