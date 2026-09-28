// SPDX-License-Identifier: MIT
/*
 * graphql-over-http: the read of a header of a request where its list holds it.
 *
 * No analogue in the reference. Each caller names the read of the Fetch API it
 * stands for, `request.headers.get(name)` or `headers.has(name)`.
 */

//! The read of a header where the list of a `Headers` holds it.
//!
//! [`value`] answers the value [`Headers::get`] answers for a lower case name,
//! the values of the name joined with `, `, or with `; ` for `cookie`, or
//! `None`, and [`with_header`] hands it to a reader while it borrows the
//! headers of a request. A `Headers` holds every name in lower case, since each
//! name it takes passes the check of `append` and `set`, which lowers it, so a
//! name of one entry is read where the list holds it and only the values of a
//! repeated name are joined into a new string. [`has`] answers what
//! [`Headers::has`] answers for a lower case name.

use std::borrow::Cow;

use aosabi_http::{
    Headers,
    Request, //
};

/// Runs `read` over the value of the header `name` of `request`, `request.headers.get(name)`.
///
/// `name` is lower case, as every name of the list of a `Headers` is.
///
/// # Panics
///
/// Panics when the headers of `request` are borrowed mutably, as the borrow
/// of their `RefCell` does, and in a debug build when `name` holds an upper
/// case letter, as [`value`] does.
pub fn with_header<R>(request: &Request, name: &str, read: impl FnOnce(Option<&str>) -> R) -> R {
    let headers = request.headers.borrow();
    read(value(&headers, name).as_deref())
}

/// Returns the value of the header `name` of `headers`, `headers.get(name)`, borrowed where one entry holds it.
///
/// `name` is lower case, as every name of the list of a `Headers` is.
///
/// # Panics
///
/// Panics in a debug build when `name` holds an upper case letter, which is a
/// fault of the caller, since no name of the list matches it.
#[must_use]
pub fn value<'a>(headers: &'a Headers, name: &str) -> Option<Cow<'a, str>> {
    debug_assert!(
        !name.bytes().any(|byte| byte.is_ascii_uppercase()),
        "a name of the list of a Headers is lower case"
    );
    let mut values = headers
        .list()
        .iter()
        .filter(|(key, _)| key == name)
        .map(|(_, value)| value.as_str());
    let first = values.next()?;
    let Some(second) = values.next() else {
        return Some(Cow::Borrowed(first));
    };
    let delimiter = if name == "cookie" { "; " } else { ", " };
    let mut joined = String::from(first);
    for value in core::iter::once(second).chain(values) {
        joined.push_str(delimiter);
        joined.push_str(value);
    }
    Some(Cow::Owned(joined))
}

/// Returns whether `headers` holds the header `name`, `headers.has(name)`.
///
/// `name` is lower case, as every name of the list of a `Headers` is.
///
/// # Panics
///
/// Panics in a debug build when `name` holds an upper case letter, as
/// [`value`] does.
#[must_use]
pub fn has(headers: &Headers, name: &str) -> bool {
    debug_assert!(
        !name.bytes().any(|byte| byte.is_ascii_uppercase()),
        "a name of the list of a Headers is lower case"
    );
    headers.list().iter().any(|(key, _)| key == name)
}
