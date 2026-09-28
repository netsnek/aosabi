// SPDX-License-Identifier: MIT
/*
 * aosabi-http: the Fetch Headers of a request and an answer.
 */

//! The Fetch `Headers`.
//!
//! A header list holds its entries in insertion order with the names in lower
//! case, which is what every reading of a `Headers` gives, and its iteration
//! sorts the names, combines the values of one name and keeps every
//! `set-cookie` value apart. The values of `cookie` combine with `; ` and those
//! of every other name with `, `, as `HeadersList.append` of the undici of Node
//! 24.13.0 combines them, where the Standard writes `, ` for every name. So the
//! cookies of one request that arrive in two fields, as a client may send them
//! over HTTP/2, read as one list, which is the join RFC 9113 asks of a server.
//! A name that is no token and a value that holds a NUL, a CR, an LF or a
//! character above U+00FF are refused with a `TypeError`, and a value loses its
//! leading and trailing HTTP whitespace. A value with another control byte
//! passes, which is why Node's own check of a record can still refuse it, quirk
//! `Q59_NODE_RECORD_CHECK_500` of `docs/byte-equality.md` section 13.1.

use core::fmt;
use std::rc::Rc;

/// A refusal of a `Headers`, the `TypeError` the platform throws.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeadersError {
    /// The message of the `TypeError`.
    pub message: String,
}

impl HeadersError {
    /// Creates the refusal of a character above U+00FF at `index` of an argument.
    fn byte_string(index: usize, code: u32) -> Self {
        Self {
            message: format!(
                "Cannot convert argument to a ByteString because the character at index {index} has a value of {code} which is greater than 255."
            ),
        }
    }

    /// Creates the refusal of an invalid name by the method `method`.
    fn name(method: &str, name: &str) -> Self {
        Self {
            message: format!("Headers.{method}: \"{name}\" is an invalid header name."),
        }
    }

    /// Creates the refusal of an invalid value by the method `method`.
    fn value(method: &str, value: &str) -> Self {
        Self {
            message: format!("Headers.{method}: \"{value}\" is an invalid header value."),
        }
    }
}

impl fmt::Display for HeadersError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl core::error::Error for HeadersError {}

/// The Fetch `Headers`: a header list in insertion order, the names in lower case.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Headers {
    /// The entries in insertion order.
    list: Vec<(String, String)>,
}

impl Headers {
    /// Creates an empty `Headers`, `new Headers()`.
    #[must_use]
    pub fn new() -> Self {
        Self { list: Vec::new() }
    }

    /// Creates a `Headers` from the entries `init`, `new Headers([[name, value], ...])`.
    ///
    /// The entries are appended in order, so a repeated name keeps every
    /// value.
    ///
    /// # Errors
    ///
    /// Returns [`HeadersError`] for the first entry whose name or value the
    /// platform refuses.
    pub fn from_entries<N: AsRef<str>, V: AsRef<str>>(
        init: &[(N, V)],
    ) -> Result<Self, HeadersError> {
        let mut headers = Self::new();
        for (name, value) in init {
            headers.append(name.as_ref(), value.as_ref())?;
        }
        Ok(headers)
    }

    /// Returns the value of `name`, the values of the name joined, or `None`.
    ///
    /// The name is read case insensitively. The values of `cookie` are joined
    /// with `; ` and those of every other name with `, `, as undici joins
    /// them, which the documentation of the module cites.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<String> {
        let name = name.to_ascii_lowercase();
        let delimiter = if name == "cookie" { "; " } else { ", " };
        let mut values = self
            .list
            .iter()
            .filter(|(key, _)| *key == name)
            .map(|(_, value)| value.as_str());
        let first = values.next()?;
        let mut joined = String::from(first);
        for value in values {
            joined.push_str(delimiter);
            joined.push_str(value);
        }
        Some(joined)
    }

    /// Returns every `set-cookie` value in order, `getSetCookie()`.
    #[must_use]
    pub fn get_set_cookie(&self) -> Vec<String> {
        self.list
            .iter()
            .filter(|(key, _)| key == "set-cookie")
            .map(|(_, value)| value.clone())
            .collect()
    }

    /// Returns whether the list holds `name`.
    #[must_use]
    pub fn has(&self, name: &str) -> bool {
        let name = name.to_ascii_lowercase();
        self.list.iter().any(|(key, _)| *key == name)
    }

    /// Sets `name` to `value`, replacing its first entry and removing the others.
    ///
    /// # Errors
    ///
    /// Returns [`HeadersError`] when the name or the value is refused, and then
    /// the list is unchanged.
    pub fn set(&mut self, name: &str, value: &str) -> Result<(), HeadersError> {
        let (name, value) = checked("set", name, value)?;
        match self.list.iter().position(|(key, _)| *key == name) {
            Some(first) => {
                self.list[first].1 = value;
                let mut index = 0;
                self.list.retain(|(key, _)| {
                    let keep = index <= first || *key != name;
                    index += 1;
                    keep
                });
            }
            None => self.list.push((name, value)),
        }
        Ok(())
    }

    /// Appends `value` to `name`.
    ///
    /// # Errors
    ///
    /// Returns [`HeadersError`] when the name or the value is refused, and then
    /// the list is unchanged.
    pub fn append(&mut self, name: &str, value: &str) -> Result<(), HeadersError> {
        let (name, value) = checked("append", name, value)?;
        self.list.push((name, value));
        Ok(())
    }

    /// Removes every entry of `name`.
    pub fn delete(&mut self, name: &str) {
        let name = name.to_ascii_lowercase();
        self.list.retain(|(key, _)| *key != name);
    }

    /// Returns the Fetch iteration: the names sorted, the values of one name combined, `set-cookie` kept apart.
    ///
    /// The names sort by their bytes, and each value of `set-cookie` is an
    /// entry of its own in its place among the sorted names.
    #[must_use]
    pub fn entries(&self) -> Vec<(String, String)> {
        let mut names: Vec<&str> = self.list.iter().map(|(key, _)| key.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        let mut entries = Vec::with_capacity(names.len());
        for name in names {
            if name == "set-cookie" {
                for cookie in self.get_set_cookie() {
                    entries.push((String::from(name), cookie));
                }
            } else if let Some(value) = self.get(name) {
                entries.push((String::from(name), value));
            }
        }
        entries
    }

    /// Returns the entries in insertion order, the header list as it is held.
    #[must_use]
    pub fn list(&self) -> &[(String, String)] {
        &self.list
    }

    /// Returns whether the list is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }
}

/// Returns the lower case name and the normalized value, or the refusal of either.
///
/// The order is the platform's: each argument is converted to a byte string,
/// which refuses a character above U+00FF, then the name is checked as a token,
/// then the value loses its leading and trailing HTTP whitespace and is checked
/// for a NUL, a CR and an LF. The messages are the ones the undici of Node
/// 24.13.0 throws.
fn checked(method: &str, name: &str, value: &str) -> Result<(String, String), HeadersError> {
    for argument in [name, value] {
        if let Some((index, c)) = argument
            .chars()
            .enumerate()
            .find(|&(_, c)| u32::from(c) > 0xFF)
        {
            return Err(HeadersError::byte_string(index, u32::from(c)));
        }
    }
    if name.is_empty() || !name.bytes().all(is_token_byte) {
        return Err(HeadersError::name(method, name));
    }
    let value = value.trim_matches(|c| matches!(c, '\t' | '\n' | '\r' | ' '));
    if value.chars().any(|c| matches!(c, '\0' | '\n' | '\r')) {
        return Err(HeadersError::value(method, value));
    }
    Ok((name.to_ascii_lowercase(), String::from(value)))
}

/// Returns whether `byte` is a character of a token of RFC 9110.
fn is_token_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte)
}

/// A `Headers` shared by the objects that hold it, as a JavaScript object is.
pub type SharedHeaders = Rc<core::cell::RefCell<Headers>>;
