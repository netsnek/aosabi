// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-build: the source text of an object literal that a generated module
 * exports.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/cli/builder/bundler/emit-server-glue.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 *
 * The branch of a string value at :64 and the branch of any other value at
 * :68 are not translated, because the object the build hands over holds
 * objects, functions and undefined alone.
 */

//! The source text of an object literal that a generated module exports.
//!
//! `resolvers.js` of the reference is `export const resolvers = ` and the
//! text [`prepare_object_injection`] makes of the resolvers of the build,
//! which is an object of objects whose one key `__resolveType` holds a
//! function. A Rust build holds no function, so a function is carried as the
//! text its `toString()` gives, which is the text the parser evaluated with
//! `new Function`.

/// Holds one value of the object that [`prepare_object_injection`] writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Injected<'a> {
    /// `undefined`, which the text leaves out together with its key.
    Undefined,
    /// A function, carried as the text its `toString()` gives.
    Function(&'a str),
    /// An object that is no array, carried as its entries in the order
    /// `Object.entries` gives them.
    Object(Vec<(&'a str, Injected<'a>)>),
}

/// Returns the source text of the object whose entries are `obj`.
///
/// `obj` lists the entries in the order `Object.entries` gives them, which
/// for an object keyed by names that are no array index is insertion order. A
/// key is written as `JSON.stringify` writes a string, a function as its text
/// unchanged, an object by the same function, and an entry whose value is
/// undefined is left out, so an object of undefined values alone is `{}`.
///
/// [OrdinaryOwnPropertyKeys]: https://tc39.es/ecma262/#sec-ordinaryownpropertykeys
#[must_use]
pub fn prepare_object_injection(obj: &[(&str, Injected<'_>)]) -> String {
    let entries: Vec<Option<String>> = obj
        .iter()
        .map(|(key, value)| match value {
            Injected::Undefined => None,
            Injected::Function(source) => Some([&json_stringify(key), ":", source].concat()),
            Injected::Object(entries) => Some(
                [
                    &json_stringify(key),
                    ":",
                    &prepare_object_injection(entries),
                ]
                .concat(),
            ),
        })
        .collect();

    // filter(Boolean) drops what ToBoolean makes false. An entry that is no
    // undefined starts with a quoted key, so it is never the empty string and
    // only the undefined entries go.
    let entries: Vec<String> = entries.into_iter().flatten().collect();
    ["{", &entries.join(","), "}"].concat()
}

/// Returns `key` as `JSON.stringify` writes a string, quotation marks
/// included.
fn json_stringify(key: &str) -> String {
    let mut out = Vec::new();
    aosabi_json::string::escape_str(&mut out, key);
    // The writer escapes within UTF-8 and adds ASCII alone, so the bytes of a
    // Rust string stay well formed.
    String::from_utf8(out).unwrap_or_default()
}
