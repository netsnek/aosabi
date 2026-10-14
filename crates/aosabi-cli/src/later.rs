// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-cli: the commands that land with a later milestone or track, each
 * answering the row that lands it.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/cli/index.ts,
 * Apache-2.0. Changed by the Aosabi authors.
 */

//! The commands of later milestones.
//!
//! The command line knows every command it does not carry yet and answers
//! each with the row that lands it and exit code 1, so a script learns when a
//! command arrives and never meets an unknown command for a name the
//! documents promise. Three kinds of row stand in [`ROWS`].
//!
//! A word with sub commands answers its own row when no sub command follows,
//! and a sub command it does not know is the usage error
//! `error: unknown command '<name>'` of commander.

use crate::{
    codes,
    report::CliError, //
};

/// The sixteen sub commands of `db` in the order of the help.
pub const DB_SUB_COMMANDS: [&str; 16] = [
    "status",
    "diff",
    "plan",
    "check",
    "fix-deps",
    "migrate",
    "rollback",
    "resolve",
    "rename-app",
    "seed",
    "baseline",
    "merge",
    "squash",
    "push",
    "create",
    "reset",
];

/// The flag of `card attach` that attaches a reproduction, which binds ahead
/// of the review and hardware attachments.
pub const REPRODUCTION_FLAG: &str = "--reproduction";

/// One command word of a later row, the row that lands it, and the row of
/// each of its sub commands.
#[derive(Clone, Copy, Debug)]
pub struct Row {
    /// The word as it is typed.
    pub word: &'static str,
    /// The milestone or the track row that lands the word alone.
    pub lands: &'static str,
    /// Each sub command and the row that lands it.
    pub subs: &'static [(&'static str, &'static str)],
}

/// The sub commands of `db`, all.
const DB_ROWS: [(&str, &str); 16] = [
    (DB_SUB_COMMANDS[0], "M9"),
    (DB_SUB_COMMANDS[1], "M9"),
    (DB_SUB_COMMANDS[2], "M9"),
    (DB_SUB_COMMANDS[3], "M9"),
    (DB_SUB_COMMANDS[4], "M9"),
    (DB_SUB_COMMANDS[5], "M9"),
    (DB_SUB_COMMANDS[6], "M9"),
    (DB_SUB_COMMANDS[7], "M9"),
    (DB_SUB_COMMANDS[8], "M9"),
    (DB_SUB_COMMANDS[9], "M9"),
    (DB_SUB_COMMANDS[10], "M9"),
    (DB_SUB_COMMANDS[11], "M9"),
    (DB_SUB_COMMANDS[12], "M9"),
    (DB_SUB_COMMANDS[13], "M9"),
    (DB_SUB_COMMANDS[14], "M9"),
    (DB_SUB_COMMANDS[15], "M9"),
];

/// The verbs of `card` and the row that binds each.
const CARD_ROWS: [(&str, &str); 15] = [
    ("inspect", "DC0"),
    ("ls", "DC0"),
    ("verify", "DC0"),
    ("extract", "DC0"),
    ("pack", "DC0"),
    ("restore", "DC0"),
    ("keygen", "DC1"),
    ("sign", "DC1"),
    ("fetch", "DC3"),
    ("new", "DC4"),
    ("knowledge", "DC4"),
    ("lint", "DC4"),
    ("attach", "DC7"),
    ("diff", "DC7"),
    ("attest", "DC11"),
];

/// Every command word of a later row.
pub const ROWS: [Row; 20] = [
    Row {
        word: "db",
        lands: "M9",
        subs: &DB_ROWS,
    },
    Row {
        word: "docs",
        lands: "M14",
        subs: &[],
    },
    Row {
        word: "bridge",
        lands: "M12",
        subs: &[],
    },
    Row {
        word: "card",
        lands: "DC0",
        subs: &CARD_ROWS,
    },
    Row {
        word: "trust",
        lands: "DC1",
        subs: &[
            ("list", "DC1"),
            ("add", "DC1"),
            ("remove", "DC1"),
            ("update", "DC1"),
        ],
    },
    Row {
        word: "revocations",
        lands: "DC1",
        subs: &[("sign", "DC1"), ("check", "DC1")],
    },
    Row {
        word: "lock",
        lands: "DC1",
        subs: &[("pin", "DC1")],
    },
    Row {
        word: "explain",
        lands: "DC1",
        subs: &[],
    },
    Row {
        word: "add",
        lands: "DC3",
        subs: &[],
    },
    Row {
        word: "publish",
        lands: "DC3",
        subs: &[],
    },
    Row {
        word: "registry",
        lands: "DC3",
        subs: &[("mirror", "DC3"), ("status", "DC7")],
    },
    Row {
        word: "install",
        lands: "DC5",
        subs: &[],
    },
    Row {
        word: "resolve",
        lands: "DC5",
        subs: &[],
    },
    Row {
        word: "attest-build",
        lands: "DC6",
        subs: &[],
    },
    Row {
        word: "search",
        lands: "DC7",
        subs: &[],
    },
    Row {
        word: "yank",
        lands: "DC7",
        subs: &[],
    },
    Row {
        word: "report",
        lands: "DC7",
        subs: &[],
    },
    Row {
        word: "blob",
        lands: "DC10",
        subs: &[("fetch", "DC10")],
    },
    Row {
        word: "flash",
        lands: "DC11",
        subs: &[],
    },
    Row {
        word: "device",
        lands: "DC11",
        subs: &[("gate", "DC11"), ("receipts", "DC11")],
    },
];

/// What a command word of a later row resolves to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolved {
    /// The command, its words joined by one space, and the row that lands it.
    Later {
        /// The command as typed, such as `db status`.
        command: String,
        /// The milestone or the track row that lands it.
        lands: &'static str,
    },
    /// A sub command the word does not know, by its name.
    UnknownSub(String),
}

/// Returns the row of `word`, `None` for a word no later row holds.
#[must_use]
pub fn row(word: &str) -> Option<&'static Row> {
    ROWS.iter().find(|row| row.word == word)
}

/// Resolves the later command `word` with the operands `operands` that
/// follow it and every argument `args` behind it, `None` for a word no
/// later row holds.
///
/// The first operand names the sub command of a word that has sub commands.
#[must_use]
pub fn resolve(word: &str, operands: &[String], args: &[String]) -> Option<Resolved> {
    let row = row(word)?;
    let Some(sub) = operands.first().filter(|_| !row.subs.is_empty()) else {
        return Some(Resolved::Later {
            command: word.to_owned(),
            lands: row.lands,
        });
    };
    let Some((name, lands)) = row.subs.iter().find(|(name, _)| name == sub) else {
        return Some(Resolved::UnknownSub(sub.clone()));
    };
    let lands =
        if word == "card" && *name == "attach" && args.iter().any(|a| a == REPRODUCTION_FLAG) {
            "DC6"
        } else {
            lands
        };
    Some(Resolved::Later {
        command: format!("{word} {name}"),
        lands,
    })
}

/// Returns the refusal of the later command `command`, which the row `lands`
/// lands: `AO-C0201` for a row of the device card track and `AO-C0601` for a
/// milestone.
#[must_use]
pub fn refusal(command: &str, lands: &str) -> CliError {
    if lands.starts_with("DC") {
        codes::card_track_verb(command, lands)
    } else {
        codes::later_milestone(command, lands)
    }
}
