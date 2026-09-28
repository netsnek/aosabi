// SPDX-License-Identifier: MIT
/*
 * aosabi-http: the parser of a Cookie header and the serializer of a
 * Set-Cookie value as hono writes them.
 *
 * Derived from hono 4.11.9, dist/utils/cookie.js, MIT.
 */

//! `parse`, `serialize`, `CookieOptions`, `SameSite`, `Priority`,
//! `CookiePrefix` and `CookieError`.
//!
//! A `Cookie` header is split on `;`, each pair is trimmed and split at its
//! first `=`, a pair whose name or value fails the grammar of hono is
//! skipped, a value in double quotes loses them, and a value that holds a `%`
//! is decoded with `decodeURIComponent` where it can be. The answer is the
//! object the reference builds, a later pair of a name taking the place of
//! the earlier one and the names that are array indices first in ascending
//! order, which is the key order of an object of JavaScript.
//!
//! A `Set-Cookie` value is the name, `=`, the value encoded with
//! `encodeURIComponent`, and the attributes in the order `Max-Age`, `Domain`,
//! `Path`, `Expires`, `HttpOnly`, `Secure`, `SameSite`, `Priority` and
//! `Partitioned`. The throws of the reference are the values of
//! [`CookieError`] with its messages, in the order the reference meets them.

use core::fmt::Write as _;
use std::time::SystemTime;

use aosabi_json::sort::sort_by_key_stable;

use crate::{
    date::{
        self,
        time_value, //
    },
    utils::url::{
        decode_uri_component,
        encode_uri_component,
        try_decode, //
    }, //
};

/// The longest `Max-Age` hono writes, 400 days in seconds.
const MAX_AGE_LIMIT: f64 = 34_560_000.0;

/// The furthest `Expires` hono writes, 400 days in milliseconds.
const EXPIRES_LIMIT_MS: i64 = 34_560_000_000;

/// The attribute `SameSite`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SameSite {
    /// `Strict`.
    Strict,
    /// `Lax`.
    Lax,
    /// `None`.
    None,
}

impl SameSite {
    /// Returns the value as the attribute writes it, the first letter upper case.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Strict => "Strict",
            Self::Lax => "Lax",
            Self::None => "None",
        }
    }
}

/// The attribute `Priority`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Priority {
    /// `Low`.
    Low,
    /// `Medium`.
    Medium,
    /// `High`.
    High,
}

impl Priority {
    /// Returns the value as the attribute writes it, the first letter upper case.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
        }
    }
}

/// The prefix of a cookie name, `'secure'` or `'host'`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CookiePrefix {
    /// `'secure'`, the name prefix `__Secure-`.
    Secure,
    /// `'host'`, the name prefix `__Host-`.
    Host,
}

impl CookiePrefix {
    /// Returns the text the prefix puts before a name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Secure => "__Secure-",
            Self::Host => "__Host-",
        }
    }
}

/// The options of a cookie, `CookieOptions`.
///
/// A member the reference leaves `undefined` is `None` or `false` here, and a
/// member whose text is empty is false where the reference tests it for
/// truth, as `Domain` and `Path` are.
#[derive(Clone, Debug, Default)]
pub struct CookieOptions {
    /// `domain`.
    pub domain: Option<String>,
    /// `expires`, the `Date` of the attribute `Expires`.
    pub expires: Option<SystemTime>,
    /// `httpOnly`.
    pub http_only: bool,
    /// `maxAge`, in seconds.
    pub max_age: Option<f64>,
    /// `path`.
    pub path: Option<String>,
    /// `secure`.
    pub secure: bool,
    /// `sameSite`.
    pub same_site: Option<SameSite>,
    /// `priority`.
    pub priority: Option<Priority>,
    /// `partitioned`.
    pub partitioned: bool,
    /// `prefix`.
    pub prefix: Option<CookiePrefix>,
}

/// A cookie hono refuses to serialize, the `Error` it throws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CookieError {
    /// A `__Secure-` name without `secure`.
    SecurePrefixNotSecure,
    /// A `__Host-` name without `secure`.
    HostPrefixNotSecure,
    /// A `__Host-` name whose path is not `/`.
    HostPrefixPath,
    /// A `__Host-` name with a domain.
    HostPrefixDomain,
    /// A `maxAge` above 400 days.
    MaxAgeTooLong,
    /// An `expires` more than 400 days ahead.
    ExpiresTooFar,
    /// `partitioned` without `secure`.
    PartitionedNotSecure,
}

impl CookieError {
    /// Returns the message of the error the reference throws.
    ///
    /// The texts are those of `cookie.js:77`, `:81`, `:84`, `:87`, `:93`,
    /// `:107` and `:126`.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::SecurePrefixNotSecure => "__Secure- Cookie must have Secure attributes",
            Self::HostPrefixNotSecure => "__Host- Cookie must have Secure attributes",
            Self::HostPrefixPath => "__Host- Cookie must have Path attributes with \"/\"",
            Self::HostPrefixDomain => "__Host- Cookie must not have Domain attributes",
            Self::MaxAgeTooLong => {
                "Cookies Max-Age SHOULD NOT be greater than 400 days (34560000 seconds) in duration."
            }
            Self::ExpiresTooFar => {
                "Cookies Expires SHOULD NOT be greater than 400 days (34560000 seconds) in the future."
            }
            Self::PartitionedNotSecure => "Partitioned Cookie must have Secure attributes",
        }
    }
}

impl core::fmt::Display for CookieError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.message())
    }
}

impl core::error::Error for CookieError {}

/// Returns the cookies of the header value `cookie`, or the one named `name`, `parse(cookie, name)`.
///
/// With a name the walk stops at the first pair of that name whose value
/// passes, and a header that does not hold the name at all answers nothing.
/// An empty name is no name, as the reference tests it for truth.
#[must_use]
pub fn parse(cookie: &str, name: Option<&str>) -> Vec<(String, String)> {
    let name = name.filter(|name| !name.is_empty());
    if let Some(name) = name
        && !cookie.contains(name)
    {
        return Vec::new();
    }
    let mut parsed: Vec<(String, String)> = Vec::new();
    for pair in js_trim(cookie).split(';') {
        let pair = js_trim(pair);
        let Some(value_start) = pair.find('=') else {
            continue;
        };
        let cookie_name = js_trim(&pair[..value_start]);
        if name.is_some_and(|name| name != cookie_name) || !is_cookie_name(cookie_name) {
            continue;
        }
        let mut value = js_trim(&pair[value_start + 1..]);
        if value.starts_with('"') && value.ends_with('"') {
            value = value.get(1..value.len().saturating_sub(1)).unwrap_or("");
        }
        if !is_cookie_value(value) {
            continue;
        }
        let value = if value.contains('%') {
            try_decode(value, decode_uri_component)
        } else {
            String::from(value)
        };
        set_property(&mut parsed, cookie_name, value);
        if name.is_some() {
            break;
        }
    }
    own_keys_order(parsed)
}

/// Returns the `Set-Cookie` value of `name` and `value`, `serialize(name, value, opt)`.
///
/// # Errors
///
/// Returns the [`CookieError`] of the first rule the cookie breaks, in the
/// order the reference checks them.
pub fn serialize(name: &str, value: &str, opt: &CookieOptions) -> Result<String, CookieError> {
    serialize_encoded(name, &encode_uri_component(value), opt)
}

/// Returns the `Set-Cookie` value of `name` and the encoded `value`, `_serialize(name, value, opt)`.
fn serialize_encoded(name: &str, value: &str, opt: &CookieOptions) -> Result<String, CookieError> {
    let mut cookie = format!("{name}={value}");
    check_prefix(name, opt)?;
    if let Some(max_age) = opt.max_age.filter(|max_age| *max_age >= 0.0) {
        if max_age > MAX_AGE_LIMIT {
            return Err(CookieError::MaxAgeTooLong);
        }
        // The value lies in 0 to 34560000, where `maxAge | 0` truncates.
        #[expect(
            clippy::cast_possible_truncation,
            reason = "the value lies in the range of the target"
        )]
        let seconds = max_age.trunc() as i64;
        // Writing into a `String` does not fail.
        let _ = write!(cookie, "; Max-Age={seconds}");
    }
    if let Some(domain) = opt.domain.as_deref().filter(|domain| !domain.is_empty())
        && opt.prefix != Some(CookiePrefix::Host)
    {
        cookie.push_str("; Domain=");
        cookie.push_str(domain);
    }
    if let Some(path) = opt.path.as_deref().filter(|path| !path.is_empty()) {
        cookie.push_str("; Path=");
        cookie.push_str(path);
    }
    if let Some(expires) = opt.expires {
        let time = time_value(expires);
        if time.is_some_and(|time| time - date::now() > EXPIRES_LIMIT_MS) {
            return Err(CookieError::ExpiresTooFar);
        }
        cookie.push_str("; Expires=");
        cookie.push_str(&utc_string(time));
    }
    if opt.http_only {
        cookie.push_str("; HttpOnly");
    }
    if opt.secure {
        cookie.push_str("; Secure");
    }
    if let Some(same_site) = opt.same_site {
        cookie.push_str("; SameSite=");
        cookie.push_str(same_site.as_str());
    }
    if let Some(priority) = opt.priority {
        cookie.push_str("; Priority=");
        cookie.push_str(priority.as_str());
    }
    if opt.partitioned {
        if !opt.secure {
            return Err(CookieError::PartitionedNotSecure);
        }
        cookie.push_str("; Partitioned");
    }
    Ok(cookie)
}

/// Checks the rules of the name prefixes `__Secure-` and `__Host-`, `cookie.js:76-89`.
fn check_prefix(name: &str, opt: &CookieOptions) -> Result<(), CookieError> {
    if name.starts_with("__Secure-") && !opt.secure {
        return Err(CookieError::SecurePrefixNotSecure);
    }
    if name.starts_with("__Host-") {
        if !opt.secure {
            return Err(CookieError::HostPrefixNotSecure);
        }
        if opt.path.as_deref() != Some("/") {
            return Err(CookieError::HostPrefixPath);
        }
        if opt
            .domain
            .as_deref()
            .is_some_and(|domain| !domain.is_empty())
        {
            return Err(CookieError::HostPrefixDomain);
        }
    }
    Ok(())
}

/// Returns whether `name` passes ``/^[\w!#$%&'*.^`|~+-]+$/``.
fn is_cookie_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_!#$%&'*.^`|~+-".contains(&byte))
}

/// Returns whether `value` passes `/^[ !#-:<-[\]-~]*$/`.
fn is_cookie_value(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| matches!(byte, b' ' | b'!' | b'#'..=b':' | b'<'..=b'[' | b']'..=b'~'))
}

/// Sets the property `key` of the object `object` to `value`, `object[key] = value`.
///
/// An existing key keeps its place. The key `__proto__` reaches the setter of
/// `Object.prototype`, which ignores a string, so no property is made.
fn set_property(object: &mut Vec<(String, String)>, key: &str, value: String) {
    if key == "__proto__" {
        return;
    }
    match object.iter_mut().find(|(known, _)| known == key) {
        Some(entry) => entry.1 = value,
        None => object.push((String::from(key), value)),
    }
}

/// Returns `object` in the key order of an object of JavaScript.
///
/// The keys that are array indices come first in ascending order, then the
/// rest in insertion order.
fn own_keys_order(mut object: Vec<(String, String)>) -> Vec<(String, String)> {
    sort_by_key_stable(&mut object, |(key, _)| {
        aosabi_json::value::array_index(key.as_bytes()).map_or((1, 0), |index| (0, index))
    });
    object
}

/// Returns `text` without the white space `String.prototype.trim` removes.
///
/// That set is `WhiteSpace` and `LineTerminator` of ECMA-262, which is the
/// Unicode `White_Space` property with U+FEFF added and U+0085 left out.
fn js_trim(text: &str) -> &str {
    text.trim_matches(|c: char| c == '\u{FEFF}' || (c.is_whitespace() && c != '\u{0085}'))
}

/// Returns `new Date(time).toUTCString()`, `Invalid Date` for no time value.
///
/// The form is `Www, DD Mmm YYYY HH:MM:SS GMT` with the year padded to four
/// digits and a minus sign before a year before 1.
fn utc_string(time: Option<i64>) -> String {
    const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let Some(time) = time else {
        return String::from("Invalid Date");
    };
    let days = time.div_euclid(86_400_000);
    let in_day = time.rem_euclid(86_400_000) / 1000;
    let (year, month, day) = civil_from_days(days);
    let weekday = usize::try_from((days + 4).rem_euclid(7)).unwrap_or(0);
    let sign = if year < 0 { "-" } else { "" };
    format!(
        "{}, {day:02} {} {sign}{:04} {:02}:{:02}:{:02} GMT",
        WEEKDAYS[weekday],
        MONTHS[month - 1],
        year.unsigned_abs(),
        in_day / 3600,
        in_day / 60 % 60,
        in_day % 60,
    )
}

/// Returns the year, the month from 1 and the day of the month of `days` since the epoch.
///
/// The proleptic Gregorian calendar the time values of ECMA-262 count in,
/// over the days of eras of 400 years from 0000-03-01.
fn civil_from_days(days: i64) -> (i64, usize, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, usize::try_from(month).unwrap_or(1), day)
}
