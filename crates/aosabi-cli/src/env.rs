// SPDX-License-Identifier: MIT
/*
 * aosabi-cli: the environment names the command line reads and the table of
 * their conventional aliases.
 *
 * The names and the aliases are those of docs/cli-and-targets.md section 6.2.
 */

//! The environment and its aliases.
//!
//! Aosabi reads `PORT` and `NODE_ENV` under their conventional names and under
//! its own names, `AOSABI_PORT` and `AOSABI_ENV`, so one environment block
//! drives both processes in the harness, and reads `AOSABI_ROLE`,
//! `AOSABI_LOG_FORMAT`, `AOSABI_DEBUG` and `AOSABI_DEV` under the Aosabi name
//! alone. A variable is read from the environment of the process and
//! then from the pairs of `.env` that the environment does not set, which is
//! the environment every child of the command line sees. When both names of a
//! pair are set and disagree, the Aosabi name wins and
//! `note: <Aosabi name> overrides <conventional name>` goes to standard error
//! once per pair, so a half migrated environment is visible rather than silently
//! resolved. When the command line sets one name of a pair for a child, it sets
//! both, so the child reads the same value under either name.
//!
//! The table is the one of section 6.2, as the table of `aosabi-core`
//! carries it. The command line adds two variables of its own,
//! [`CONTAINER_ENGINE`] and the switch `AOSABI_STD` of [`crate::std_source`],
//! and nothing else.

use std::{
    cell::RefCell,
    ffi::{
        OsStr,
        OsString, //
    }, //
};

use crate::dotenv::{
    self,
    DotEnv, //
};

/// The conventional names and the Aosabi name beside each.
pub const ALIASES: [(&str, &str); 2] = [("PORT", "AOSABI_PORT"), ("NODE_ENV", "AOSABI_ENV")];

/// The port when no variable names one, the fallback of
/// `Number(process.env.PORT) || 3000`.
pub const DEFAULT_PORT: u16 = 3000;

/// The role when no variable names one.
pub const DEFAULT_ROLE: &str = "web";

/// The variable that names the container engine of the `docker` card, or
/// `none` for no engine.
pub const CONTAINER_ENGINE: &str = "AOSABI_CONTAINER_ENGINE";

thread_local! {
    /// The conventional names a note was written for.
    static NOTED: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
}

/// Returns the variable `name` as a child of the command line sees it, from
/// the environment of the process and then from `.env`.
///
/// A value of the environment that is no Unicode reads as absent, as the
/// served binary reads it.
#[must_use]
pub fn var(env: &DotEnv, name: &str) -> Option<String> {
    if let Ok(value) = std::env::var(name) {
        return Some(value);
    }
    if std::env::var_os(name).is_some() {
        return None;
    }
    env.pairs
        .iter()
        .find(|(key, _)| key == name)
        .and_then(|(_, value)| value.to_str())
        .map(str::to_owned)
}

/// Returns the Aosabi name of the conventional name `conventional`, or `None`
/// for a name the table does not hold.
#[must_use]
pub fn aosabi_name(conventional: &str) -> Option<&'static str> {
    ALIASES
        .iter()
        .find(|(name, _)| *name == conventional)
        .map(|(_, aosabi)| *aosabi)
}

/// Returns the variable of the conventional name `conventional` through the
/// alias table.
///
/// The Aosabi name wins when it is set, and the note of the module goes out
/// when the conventional name is set to another value.
#[must_use]
pub fn aliased(env: &DotEnv, conventional: &str) -> Option<String> {
    let conventional_value = var(env, conventional);
    let Some((conventional_name, aosabi)) = ALIASES.iter().find(|(name, _)| *name == conventional)
    else {
        return conventional_value;
    };
    let Some(aosabi_value) = var(env, aosabi) else {
        return conventional_value;
    };
    if conventional_value.is_some_and(|value| value != aosabi_value) {
        note(conventional_name, aosabi);
    }
    Some(aosabi_value)
}

/// Returns the pairs that set the conventional name `conventional` and its
/// Aosabi name to `value` for a child, the conventional name alone when the
/// table holds no alias.
#[must_use]
pub fn both_names(conventional: &str, value: &OsStr) -> Vec<(OsString, OsString)> {
    let mut pairs = vec![(OsString::from(conventional), value.to_os_string())];
    if let Some(aosabi) = aosabi_name(conventional) {
        pairs.push((OsString::from(aosabi), value.to_os_string()));
    }
    pairs
}

/// Returns the port, `AOSABI_PORT` and then `PORT` read as
/// `Number(value) || 3000`.
///
/// `0`, an empty value, a missing value and a value `Number` reads as `NaN`
/// give 3000, as `||` does. A number that is no whole number from 1 to 65535
/// gives 3000 as well, which is where the served binary falls back too, so
/// the port the supervisor of `dev` probes is the port its child binds.
#[must_use]
pub fn port(env: &DotEnv) -> u16 {
    aliased(env, "PORT")
        .and_then(|value| js_number_port(&value))
        .unwrap_or(DEFAULT_PORT)
}

/// Returns the environment of the node, `AOSABI_ENV` and then `NODE_ENV`.
#[must_use]
pub fn node_env(env: &DotEnv) -> Option<String> {
    aliased(env, "NODE_ENV")
}

/// Returns whether `AOSABI_DEBUG` reads `1`, which prints the detail of every
/// failure whole.
#[must_use]
pub fn debug(env: &DotEnv) -> bool {
    aliased(env, "AOSABI_DEBUG").is_some_and(|value| value == "1")
}

/// Returns the role, `AOSABI_ROLE`, `web` when it is not set.
#[must_use]
pub fn role(env: &DotEnv) -> String {
    aliased(env, "AOSABI_ROLE").unwrap_or_else(|| DEFAULT_ROLE.to_owned())
}

/// Writes the note of a pair whose names disagree, once per pair.
fn note(conventional: &'static str, aosabi: &'static str) {
    let first = NOTED
        .try_with(|noted| {
            let mut noted = noted.borrow_mut();
            if noted.contains(&conventional) {
                return false;
            }
            noted.push(conventional);
            true
        })
        .unwrap_or(false);
    if first {
        eprintln!("note: {aosabi} overrides {conventional}");
    }
}

/// Returns the port `Number(value)` gives, `None` for 0, `NaN` and a number
/// that is no port.
///
/// The function `Number` trims white space, reads the empty string as 0, a
/// decimal with a fraction or an exponent, `Infinity` with its sign, and the
/// prefixes `0x`, `0o` and `0b` with no sign.
fn js_number_port(value: &str) -> Option<u16> {
    let text = value.trim_matches(dotenv::is_space);
    let number = if let Some(digits) = strip_radix_prefix(text, 'x') {
        whole_in_radix(digits, 16)
    } else if let Some(digits) = strip_radix_prefix(text, 'o') {
        whole_in_radix(digits, 8)
    } else if let Some(digits) = strip_radix_prefix(text, 'b') {
        whole_in_radix(digits, 2)
    } else if is_decimal_literal(text) {
        text.parse::<f64>().ok()
    } else {
        None
    }?;
    if number.fract() != 0.0 || !(1.0..=65_535.0).contains(&number) {
        return None;
    }
    format!("{number}").parse().ok()
}

/// Returns the digits behind `0<letter>` or `0<LETTER>`.
fn strip_radix_prefix(text: &str, letter: char) -> Option<&str> {
    let rest = text.strip_prefix('0')?;
    rest.strip_prefix(letter)
        .or_else(|| rest.strip_prefix(letter.to_ascii_uppercase()))
}

/// Returns the value of `digits` in `radix`, `None` for no digit, a wrong
/// digit or a value past every port.
fn whole_in_radix(digits: &str, radix: u32) -> Option<f64> {
    if digits.is_empty() {
        return None;
    }
    u32::from_str_radix(digits, radix).ok().map(f64::from)
}

/// Returns whether `text` is a `StrDecimalLiteral` with its sign, the empty
/// string among them, which reads as 0.
fn is_decimal_literal(text: &str) -> bool {
    let unsigned = text
        .strip_prefix('+')
        .or_else(|| text.strip_prefix('-'))
        .unwrap_or(text);
    if unsigned == "Infinity" || text.is_empty() {
        return true;
    }
    let (mantissa, exponent) = match unsigned.find(['e', 'E']) {
        Some(at) => (&unsigned[..at], Some(&unsigned[at + 1..])),
        None => (unsigned, None),
    };
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let digits = |part: &str| part.bytes().all(|byte| byte.is_ascii_digit());
    if !digits(whole) || !digits(fraction) || (whole.is_empty() && fraction.is_empty()) {
        return false;
    }
    exponent.is_none_or(|exponent| {
        let exponent = exponent
            .strip_prefix('+')
            .or_else(|| exponent.strip_prefix('-'))
            .unwrap_or(exponent);
        !exponent.is_empty() && digits(exponent)
    })
}
