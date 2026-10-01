// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-infer: the scalar table, the twelve scalars every schema declares.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/cli/builder/schema/scalars.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 */

//! The scalar table.
//!
//! The order of the table is the order of the `scalar` blocks in every
//! schema, because `Object.keys` lists the keys of an object literal in
//! insertion order when none of them is an array index, and none of the
//! twelve is.
//!
//! [own-keys]: https://tc39.es/ecma262/#sec-ordinaryownpropertykeys

/// Maps each scalar name to the TypeScript type the reference notes beside
/// it, in the order of the object literal.
pub const SCALARS: [(&str, &str); 12] = [
    ("String", "string"),
    ("ID", "string"),
    ("Int", "number"),
    ("Float", "number"),
    ("Boolean", "boolean"),
    ("Number", "number"),
    ("Any", "any"),
    ("Void", "void"),
    ("JSONObject", "object"),
    ("File", "File"),
    ("Date", "Date"),
    ("JSON", "object"),
];

/// Lists the scalar names, which is `Object.keys(SCALARS)`.
pub const SCALAR_NAMES: [&str; 12] = [
    SCALARS[0].0,
    SCALARS[1].0,
    SCALARS[2].0,
    SCALARS[3].0,
    SCALARS[4].0,
    SCALARS[5].0,
    SCALARS[6].0,
    SCALARS[7].0,
    SCALARS[8].0,
    SCALARS[9].0,
    SCALARS[10].0,
    SCALARS[11].0,
];
