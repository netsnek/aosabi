// SPDX-License-Identifier: MIT
/*
 * aosabi-lang: the JavaScript string operations the text of an error is
 * built with.
 *
 * No analogue in graphql-js, which gets each of them from the language it is
 * written in.
 */

//! The string operations behind the message of an error.
//!
//! A message of the reference is a JavaScript string, which is a sequence of
//! UTF-16 code units and may hold a surrogate without its partner. Three
//! lexer errors quote a fixed count of code units of the body, and that
//! window cuts an astral character in two whenever its leading surrogate is
//! the last unit inside it. `JSON.stringify` then writes the lone unit as a
//! `\u` escape, so the unit is part of the bytes an error has on the wire. A
//! Rust `String` cannot hold it, so a message is a [`JsString`], and this
//! module is how one is put together.
//!
//! | Item | JavaScript |
//! | --- | --- |
//! | [`JsStringBuilder`] | a template literal, or a chain of the `+` operator |
//! | [`join`] | `Array.prototype.join` over strings |
//! | [`to_string_lossy`] | no analogue, the text a Rust formatter can carry |
//! | [`number_to_string`] | `Number::toString` with radix 10 |

use alloc::{
    borrow::Cow,
    string::String,
    vec::Vec, //
};

use aosabi_json::JsString;

/// Builds one JavaScript string out of text and of other JavaScript strings.
///
/// This is string concatenation, the reading of a template literal and of the
/// `+` operator over strings: the code units of each part follow those of the
/// part before it. A leading surrogate that ends one part and a trailing
/// surrogate that starts the next therefore read as one code point in the
/// result, which [`JsStringBuilder::finish`] reproduces.
#[derive(Debug, Default)]
pub struct JsStringBuilder {
    /// The WTF-8 bytes of the parts pushed so far, not yet merged at the seams.
    wtf8: Vec<u8>,
}

impl JsStringBuilder {
    /// Creates a builder that holds the empty string.
    #[must_use]
    pub const fn new() -> Self {
        Self { wtf8: Vec::new() }
    }

    /// Appends `text`.
    pub fn push_str(&mut self, text: &str) -> &mut Self {
        self.wtf8.extend_from_slice(text.as_bytes());
        self
    }

    /// Appends the code units of `string`.
    pub fn push(&mut self, string: &JsString) -> &mut Self {
        self.wtf8.extend_from_slice(string.as_wtf8());
        self
    }

    /// Returns the string the parts form.
    ///
    /// [`JsString::from_wtf8`] merges a surrogate pair whose halves two parts
    /// contributed. Every part is WTF-8 already, so the bytes always decode
    /// and the default is never taken.
    #[must_use]
    pub fn finish(&self) -> JsString {
        JsString::from_wtf8(&self.wtf8).unwrap_or_default()
    }
}

/// Returns `strings` joined by `separator`.
///
/// This is `Array.prototype.join` over an array of strings: the separator
/// stands between two neighbours and nowhere else, and an empty array joins
/// to the empty string.
#[must_use]
pub fn join<'a>(strings: impl IntoIterator<Item = &'a JsString>, separator: &str) -> JsString {
    let mut joined = JsStringBuilder::new();
    for (index, string) in strings.into_iter().enumerate() {
        if index > 0 {
            joined.push_str(separator);
        }
        joined.push(string);
    }
    joined.finish()
}

/// Returns `string` as text, with U+FFFD in place of each lone surrogate.
///
/// A Rust formatter writes `str` and nothing else, so this is what `Display`
/// of an error prints. It is never the form that reaches the wire, which is
/// the [`JsString`] itself through the JSON writer. A string without a lone
/// surrogate, which is every message but the three of the module
/// documentation, is borrowed unchanged.
// A `from_utf8_unchecked` would skip the recheck.
#[must_use]
pub fn to_string_lossy(string: &JsString) -> Cow<'_, str> {
    if let Some(text) = string.as_str() {
        return Cow::Borrowed(text);
    }
    let mut rest = string.as_wtf8();
    let mut lossy = String::with_capacity(rest.len());
    while let Err(error) = core::str::from_utf8(rest) {
        let (text, tail) = rest.split_at(error.valid_up_to());
        // The stretch before the error is UTF-8 by the contract of
        // `valid_up_to`, so the default is never taken.
        lossy.push_str(core::str::from_utf8(text).unwrap_or_default());
        lossy.push(char::REPLACEMENT_CHARACTER);
        // A `JsString` is well formed WTF-8, so what UTF-8 refuses in it is
        // the three bytes of a lone surrogate and nothing else.
        rest = tail.get(3..).unwrap_or_default();
    }
    lossy.push_str(core::str::from_utf8(rest).unwrap_or_default());
    Cow::Owned(lossy)
}

pub use aosabi_json::number::number_to_string;
