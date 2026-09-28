// SPDX-License-Identifier: MIT
/*
 * aosabi-http: the html template of hono and its escape.
 *
 * Derived from hono 4.11.9, dist/helper/html/index.js, MIT.
 * Derived from hono 4.11.9, dist/utils/html.js, MIT.
 */

//! The `html` template, `raw` and the escape of a string.
//!
//! The template joins its strings with its values, escaping a string value and
//! writing an escaped value as it is, and answers an escaped `String` object,
//! which `c.html` hands to the response as a body the adapter keeps in no
//! cache (A5 of the plan's version 2 draft). A number is written as JavaScript
//! writes it, and a boolean, `null` and `undefined` are left out. The
//! callbacks and the promises of the reference serve its JSX and a streamed
//! render, which the pages of an app do not use, so the template here is
//! synchronous.

use aosabi_json::number_to_string;

/// A string the template writes as it is, the escaped `String` object of the reference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HtmlEscaped(String);

impl HtmlEscaped {
    /// Returns the string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns the string, consuming the value.
    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }
}

/// One value of the template.
#[derive(Clone, Debug)]
pub enum HtmlValue {
    /// A string, escaped.
    Str(String),
    /// A number, written as JavaScript writes it.
    Number(f64),
    /// A boolean, `null` or `undefined`, left out.
    Skip,
    /// An escaped string, written as it is.
    Escaped(HtmlEscaped),
    /// An array, flattened.
    List(Vec<HtmlValue>),
}

impl From<&str> for HtmlValue {
    fn from(value: &str) -> Self {
        Self::Str(String::from(value))
    }
}

impl From<String> for HtmlValue {
    fn from(value: String) -> Self {
        Self::Str(value)
    }
}

impl From<f64> for HtmlValue {
    fn from(value: f64) -> Self {
        Self::Number(value)
    }
}

impl From<HtmlEscaped> for HtmlValue {
    fn from(value: HtmlEscaped) -> Self {
        Self::Escaped(value)
    }
}

/// Returns `value` as a string the template writes as it is, `raw(value)`.
#[must_use]
pub fn raw(value: impl Into<String>) -> HtmlEscaped {
    HtmlEscaped(value.into())
}

/// Appends `value` to `buffer` with `&`, `<`, `>`, `'` and `"` escaped.
pub fn escape_to_buffer(value: &str, buffer: &mut String) {
    for c in value.chars() {
        match c {
            '"' => buffer.push_str("&quot;"),
            '\'' => buffer.push_str("&#39;"),
            '&' => buffer.push_str("&amp;"),
            '<' => buffer.push_str("&lt;"),
            '>' => buffer.push_str("&gt;"),
            c => buffer.push(c),
        }
    }
}

/// Returns the template of `strings` with `values` between them, `` html`...` ``.
///
/// `strings` holds one string more than `values`, as a tagged template gives
/// them.
#[must_use]
pub fn html(strings: &[&str], values: &[HtmlValue]) -> HtmlEscaped {
    let mut buffer = String::new();
    for (i, string) in strings.iter().enumerate() {
        buffer.push_str(string);
        if let Some(value) = values.get(i)
            && i + 1 < strings.len()
        {
            write_value(value, &mut buffer);
        }
    }
    HtmlEscaped(buffer)
}

/// Writes one value of the template into `buffer`.
fn write_value(value: &HtmlValue, buffer: &mut String) {
    match value {
        HtmlValue::Str(string) => escape_to_buffer(string, buffer),
        HtmlValue::Number(number) => buffer.push_str(&number_to_string(*number)),
        HtmlValue::Skip => {}
        HtmlValue::Escaped(escaped) => buffer.push_str(escaped.as_str()),
        HtmlValue::List(items) => {
            for item in items {
                write_value(item, buffer);
            }
        }
    }
}
