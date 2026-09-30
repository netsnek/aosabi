// SPDX-License-Identifier: MIT
/*
 * aosabi-core: the environment of the process through the alias table.
 */

//! `var`, the environment through the alias table.
//!
//! Aosabi reads the conventional name of a variable, `PORT` and `NODE_ENV`,
//! and its own name, so one environment block drives both processes in the
//! harness. A variable of Aosabi's own is read under its Aosabi name alone,
//! `AOSABI_ROLE` among them, which the table
//! holds no row for, so [`var`] reads it as it stands. When both names of a
//! pair are set and disagree, the Aosabi name wins and a note goes to standard
//! error once per name, so a half migrated environment is visible rather than
//! silently resolved.

use core::cell::RefCell;

/// The conventional names and the Aosabi name beside each, the table of section 6.2.
const ALIASES: [(&str, &str); 2] = [("PORT", "AOSABI_PORT"), ("NODE_ENV", "AOSABI_ENV")];

thread_local! {
    /// The names a note was written for.
    static NOTED: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
}

/// Returns the Aosabi name of the conventional name `name`, `None` for a name the
/// table maps to itself alone.
#[must_use]
pub fn aosabi_name(name: &str) -> Option<&'static str> {
    ALIASES
        .iter()
        .find(|(conventional, _)| *conventional == name)
        .map(|(_, aosabi)| *aosabi)
}

/// Returns the variable `name`, the conventional name, through the alias table.
///
/// The Aosabi name wins when it is set. A variable whose value is no Unicode
/// reads as absent, as a JavaScript process reads no such variable either,
/// and a variable `getEnv()` wrote onto the process reads as written.
#[must_use]
pub fn var(name: &str) -> Option<String> {
    let conventional = crate::get_env::process_overlay(name).or_else(|| std::env::var(name).ok());
    let Some((conventional_name, alias)) = ALIASES.iter().find(|(name_of, _)| *name_of == name)
    else {
        return conventional;
    };
    let Ok(aosabi) = std::env::var(alias) else {
        return conventional;
    };
    if let Some(conventional) = &conventional
        && *conventional != aosabi
    {
        note(conventional_name, alias, &aosabi);
    }
    Some(aosabi)
}

/// Writes the note of two names that disagree, once per name.
fn note(conventional: &'static str, aosabi: &'static str, value: &str) {
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
        eprintln!(
            "note: {conventional} and {aosabi} are both set and disagree, and {aosabi}={value} wins"
        );
    }
}
