// SPDX-License-Identifier: MIT
/*
 * aosabi-http: the cookie helpers of hono an application imports.
 *
 * Derived from hono 4.11.9, dist/helper/cookie/index.js, MIT.
 */

//! `get_cookie`, `get_cookie_with`, `get_cookies`, `set_cookie`,
//! `delete_cookie` and `generate_cookie`.
//!
//! The helpers read the `Cookie` header of the request of a context and
//! append a `Set-Cookie` header to its answer. A cookie set with a prefix
//! carries the name `__Secure-` or `__Host-` and the attributes that prefix
//! asks for, and every other cookie defaults its path to `/`. The package
//! root re-exports the helpers of `hono/cookie`, and the signed
//! forms are left out, because no fixture and no port asks for them.

use crate::{
    context::Context,
    utils::cookie::{
        CookieError,
        CookieOptions,
        CookiePrefix,
        parse,
        serialize, //
    },
};

/// Returns the cookie `name` of the request of `c`, `getCookie(c, name)`.
#[must_use]
pub fn get_cookie(c: &Context, name: &str) -> Option<String> {
    get_cookie_with(c, name, None)
}

/// Returns the cookie `name` with the name prefix `prefix`, `getCookie(c, name, prefix)`.
///
/// A request with no `Cookie` header, or an empty one, has no cookie, and
/// otherwise the header is parsed for the prefixed name alone. A name the
/// parsed object holds no own member for answers `None`, where the reference
/// answers the member of that name the object inherits from
/// `Object.prototype`, if there is one: `__proto__` answers
/// `Object.prototype` itself, and `constructor`, `toString` and
/// `hasOwnProperty`, among the others, answer a function.
#[must_use]
pub fn get_cookie_with(c: &Context, name: &str, prefix: Option<CookiePrefix>) -> Option<String> {
    let cookie = c
        .req()
        .header("Cookie")
        .filter(|cookie| !cookie.is_empty())?;
    let key = match prefix {
        Some(prefix) => format!("{}{name}", prefix.as_str()),
        None => String::from(name),
    };
    parse(&cookie, Some(&key))
        .into_iter()
        .find(|(found, _)| *found == key)
        .map(|(_, value)| value)
}

/// Returns every cookie of the request of `c`, `getCookie(c)`.
///
/// An object of the cookies in the key order of an object of JavaScript, and
/// none for a request with no `Cookie` header or an empty one.
#[must_use]
pub fn get_cookies(c: &Context) -> Vec<(String, String)> {
    match c.req().header("Cookie").filter(|cookie| !cookie.is_empty()) {
        Some(cookie) => parse(&cookie, None),
        None => Vec::new(),
    }
}

/// Returns the `Set-Cookie` value of the cookie `name` with `value`, `generateCookie(name, value, opt)`.
///
/// The prefix `secure` names the cookie `__Secure-<name>` with the path `/`
/// unless `opt` names one and `secure` on, the prefix `host` names it
/// `__Host-<name>` with the path `/`, `secure` on and no domain whatever
/// `opt` says, and no prefix defaults the path to `/`.
///
/// # Errors
///
/// Returns the [`CookieError`] of the rule of hono the cookie breaks.
pub fn generate_cookie(name: &str, value: &str, opt: CookieOptions) -> Result<String, CookieError> {
    match opt.prefix {
        Some(CookiePrefix::Secure) => {
            let opt = CookieOptions {
                path: opt.path.or_else(|| Some(String::from("/"))),
                secure: true,
                ..opt
            };
            serialize(&format!("__Secure-{name}"), value, &opt)
        }
        Some(CookiePrefix::Host) => {
            let opt = CookieOptions {
                path: Some(String::from("/")),
                secure: true,
                domain: None,
                ..opt
            };
            serialize(&format!("__Host-{name}"), value, &opt)
        }
        None => {
            let opt = CookieOptions {
                path: opt.path.or_else(|| Some(String::from("/"))),
                ..opt
            };
            serialize(name, value, &opt)
        }
    }
}

/// Appends the cookie `name` with `value` to the answer of `c`, `setCookie(c, name, value, opt)`.
///
/// The value of [`generate_cookie`] is appended as a `set-cookie` header, the
/// name in lower case as a Fetch `Headers` keeps it.
///
/// # Errors
///
/// Returns the [`CookieError`] the reference throws, before any header is set.
pub fn set_cookie(
    c: &Context,
    name: &str,
    value: &str,
    opt: CookieOptions,
) -> Result<(), CookieError> {
    let cookie = generate_cookie(name, value, opt)?;
    c.header_append("Set-Cookie", &cookie);
    Ok(())
}

/// Expires the cookie `name` and returns its value before, `deleteCookie(c, name, opt)`.
///
/// The cookie of the request is read with the prefix of `opt` and set again
/// to the empty value with `maxAge` 0.
///
/// # Errors
///
/// Returns the [`CookieError`] of [`set_cookie`].
pub fn delete_cookie(
    c: &Context,
    name: &str,
    opt: CookieOptions,
) -> Result<Option<String>, CookieError> {
    let deleted = get_cookie_with(c, name, opt.prefix);
    set_cookie(
        c,
        name,
        "",
        CookieOptions {
            max_age: Some(0.0),
            ..opt
        },
    )?;
    Ok(deleted)
}
