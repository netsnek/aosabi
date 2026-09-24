// SPDX-License-Identifier: MIT
/*
 * aosabi-json: the only crate that turns a number, a string or an object into
 * JSON bytes.
 *
 * Translation of ECMA-262 JSON.stringify as V8 implements it, which is the
 * shape of every body the reference writes. The clauses are read at
 * https://tc39.es/ecma262/ and each file names the ones it translates by their
 * anchor. The file number.rs is Number::toString and the null rule of
 * SerializeJSONProperty, over the digits of the port of Ryu under number/,
 * whose notice NOTICE carries. The file string.rs is QuoteJSONString and
 * UnicodeEscape. The file word.rs is the word test that finds the bytes a scan
 * of text stops at, a word at a time, which has no analogue in the reference.
 * The file value.rs is the value model and OrdinaryOwnPropertyKeys. The file
 * writer.rs is SerializeJSONProperty, SerializeJSONObject and
 * SerializeJSONArray. The file reader.rs is the strict reader of the JSON
 * grammar of RFC 8259 back into the value model, which has no analogue in the
 * reference. The file map.rs is the insertion ordered map and set every crate
 * of Aosabi carries, over an index table under map/ that translates the
 * generic group of hashbrown, whose notice NOTICE carries. The file case.rs is
 * the lower case and the upper case of String.prototype.toLowerCase and
 * toUpperCase for a caller that reads ASCII alone, with no Unicode table. The
 * file space.rs is the white space and the line terminators of ECMA-262, which
 * the class \s of a regular expression matches and String.prototype.trim
 * drops. The file sort.rs is the stable sort of the project, a merge sort over
 * indices that a binary holds once, which has no analogue in the reference.
 */

//! The JSON writer of Aosabi, and the reader that is its inverse.
//!
//! One crate is the only place a value becomes JSON bytes. It exposes a
//! JavaScript shaped value model with an explicit absent variant, [`JsValue`],
//! a number that is always an `f64`, a string that carries the UTF-16 shape of
//! a JavaScript string, [`JsString`], an insertion ordered object,
//! [`JsObject`], a host object carried through unread, [`Opaque`], one writer,
//! [`write()`], with its form for the gap of a space argument,
//! [`write_with_gap()`], and one reader, [`parse()`], which a served crate
//! reads its request lines with. The macro [`json!`] builds a value from a
//! JavaScript literal over the conversion [`into_json::IntoJson`]. [`Map`] and
//! [`Value`] are the object and the value under the names a Rust author knows
//! from `serde_json`, which an application builds with no macro, through
//! `Map::from([(key, value), ..])`, `FromIterator` and the `From` conversions
//! of a value, while the engine keeps the names `JsObject` and `JsValue`. The
//! object and every keyed collection of Aosabi are carried by the insertion
//! ordered [`map::IndexMap`] and [`map::IndexSet`] of this crate. The module
//! [`case`] maps text to the lower case or the upper case of JavaScript as far
//! as the mapping holds ASCII, for every crate that compares mapped text with
//! ASCII, and [`is_js_space()`] is the white space of a JavaScript text, the
//! class `\s` and what `trim` drops, for every crate that skips it.
//! [`JsValue::truthy`] is `ToBoolean` of a value.
//!
//! The crate builds with no standard library and needs an allocator, because
//! the kernel and the player link it. The feature `std` links the standard
//! library for the keys of the map: the keyed `RandomState` of
//! [`value::KeyHasherBuilder`] where the standard library reaches a source of
//! entropy, and elsewhere the thread local that keeps the key of
//! `value::seed`, which the platform calls with its randomness. A build
//! without the feature hashes its maps with the fixed key of
//! [`value::KeyState`]. The feature `narrow-group` probes the index table of
//! the map a group of four slots at a time on every target, which is how a
//! host runs the probe of the ESP32-C3.
//!
//! The judging notebook is `tests/notebooks/01_json_writer.ipynb`, which
//! drives `aosabi-devtool json` against node `JSON.stringify`. There is no
//! test in this crate, which is the rule of CONTRIBUTING.md section 3.

#![no_std]

extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

pub mod case;
pub mod into_json;
// The home of the macro json!, which #[macro_export] exports at the root of
// the crate, so the module itself stays private.
mod json_macro;
pub mod map;
pub mod number;
pub mod reader;
pub mod sort;
pub mod space;
pub mod string;
pub mod value;
pub mod word;
pub mod writer;

pub use crate::{
    number::number_to_string,
    reader::{
        ReadError,
        parse, //
    },
    space::is_js_space,
    value::{
        JsObject,
        JsString,
        JsValue,
        Opaque,
        Wtf8Error, //
    },
    writer::{
        to_bytes,
        to_string,
        to_string_with_gap,
        write,
        write_with_gap, //
    }, //
};

/// A JSON object under its Rust name, the name `serde_json` gives it.
///
/// It is [`JsObject`] itself, so a map an application builds is the object the
/// engine writes. It keeps its keys in insertion order, and the writer writes
/// them in the order of JavaScript, the keys that are array indices first in
/// ascending order and every other key in insertion order, so
/// `Map::from([("b", 1), ("1", 2)])` writes `{"1":2,"b":1}` as V8 writes the
/// object literal `{b: 1, "1": 2}`.
pub type Map = JsObject;

/// A JSON value under its Rust name, the name `serde_json` gives it.
///
/// It is [`JsValue`] itself, with the absent value `Undefined` beside `Null`,
/// since a JavaScript object leaves out a member whose value is `undefined`.
pub type Value = JsValue;
