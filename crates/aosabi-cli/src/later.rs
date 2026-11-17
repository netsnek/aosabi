// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-cli: the commands that land with a later release or arrive with
 * device cards, each answering when it lands.
 *
 * Derived from Pylon 3.0.0-canary by Nico Schett,
 * packages/pylon/src/cli/index.ts, Apache-2.0. Changed by the Aosabi authors.
 *
 * The commands keep the names of the reference, the sixteen sub commands of
 * db among them in their order.
 */

//! The commands of later releases.
//!
//! The command line knows every command it does not carry yet and answers
//! each with when it lands and exit code 1, so a script learns when a command
//! arrives and never meets an unknown command for a name the documents
//! promise. Three kinds of row stand in [`ROWS`].
//!
//! - The sixteen sub commands of `db`, the names of the reference, which land
//!   with a later release and answer `AO-C0601`.
//! - `docs` and `bridge`, the gateway from HTTP to `GraphTP`, which land with
//!   a later release and answer `AO-C0601`.
//! - The card and registry verbs, which arrive with device cards and answer
//!   `AO-C0201`: the six reading and packing verbs of `card`, the keys, the
//!   signatures, the trust store, the revocations, the lock pin and
//!   `explain`, the registry verbs that fetch and publish, the authoring
//!   verbs, the install and the resolver, the build attestation and the
//!   reproduction attached to a card, search, yank, report, the review and
//!   hardware attachments, `card diff` and `registry status`, the blob fetch,
//!   and the flash, the device gate and receipts and the hardware
//!   attestation.
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

/// A command that lands with a later release of Aosabi.
pub const LATER_RELEASE: &str = "a later release";

/// A verb that arrives with device cards.
pub const DEVICE_CARDS: &str = "device cards";

/// One command word of a later row, when it lands, and when each of its sub
/// commands lands.
#[derive(Clone, Copy, Debug)]
pub struct Row {
    /// The word as it is typed.
    pub word: &'static str,
    /// When the word alone lands, [`LATER_RELEASE`] or [`DEVICE_CARDS`].
    pub lands: &'static str,
    /// Each sub command and when it lands.
    pub subs: &'static [(&'static str, &'static str)],
}

/// The sub commands of `db`, all of a later release.
const DB_ROWS: [(&str, &str); 16] = [
    (DB_SUB_COMMANDS[0], LATER_RELEASE),
    (DB_SUB_COMMANDS[1], LATER_RELEASE),
    (DB_SUB_COMMANDS[2], LATER_RELEASE),
    (DB_SUB_COMMANDS[3], LATER_RELEASE),
    (DB_SUB_COMMANDS[4], LATER_RELEASE),
    (DB_SUB_COMMANDS[5], LATER_RELEASE),
    (DB_SUB_COMMANDS[6], LATER_RELEASE),
    (DB_SUB_COMMANDS[7], LATER_RELEASE),
    (DB_SUB_COMMANDS[8], LATER_RELEASE),
    (DB_SUB_COMMANDS[9], LATER_RELEASE),
    (DB_SUB_COMMANDS[10], LATER_RELEASE),
    (DB_SUB_COMMANDS[11], LATER_RELEASE),
    (DB_SUB_COMMANDS[12], LATER_RELEASE),
    (DB_SUB_COMMANDS[13], LATER_RELEASE),
    (DB_SUB_COMMANDS[14], LATER_RELEASE),
    (DB_SUB_COMMANDS[15], LATER_RELEASE),
];

/// The verbs of `card`, each of which arrives with device cards.
const CARD_ROWS: [(&str, &str); 15] = [
    ("inspect", DEVICE_CARDS),
    ("ls", DEVICE_CARDS),
    ("verify", DEVICE_CARDS),
    ("extract", DEVICE_CARDS),
    ("pack", DEVICE_CARDS),
    ("restore", DEVICE_CARDS),
    ("keygen", DEVICE_CARDS),
    ("sign", DEVICE_CARDS),
    ("fetch", DEVICE_CARDS),
    ("new", DEVICE_CARDS),
    ("knowledge", DEVICE_CARDS),
    ("lint", DEVICE_CARDS),
    ("attach", DEVICE_CARDS),
    ("diff", DEVICE_CARDS),
    ("attest", DEVICE_CARDS),
];

/// Every command word of a later row.
pub const ROWS: [Row; 20] = [
    Row {
        word: "db",
        lands: LATER_RELEASE,
        subs: &DB_ROWS,
    },
    Row {
        word: "docs",
        lands: LATER_RELEASE,
        subs: &[],
    },
    Row {
        word: "bridge",
        lands: LATER_RELEASE,
        subs: &[],
    },
    Row {
        word: "card",
        lands: DEVICE_CARDS,
        subs: &CARD_ROWS,
    },
    Row {
        word: "trust",
        lands: DEVICE_CARDS,
        subs: &[
            ("list", DEVICE_CARDS),
            ("add", DEVICE_CARDS),
            ("remove", DEVICE_CARDS),
            ("update", DEVICE_CARDS),
        ],
    },
    Row {
        word: "revocations",
        lands: DEVICE_CARDS,
        subs: &[("sign", DEVICE_CARDS), ("check", DEVICE_CARDS)],
    },
    Row {
        word: "lock",
        lands: DEVICE_CARDS,
        subs: &[("pin", DEVICE_CARDS)],
    },
    Row {
        word: "explain",
        lands: DEVICE_CARDS,
        subs: &[],
    },
    Row {
        word: "add",
        lands: DEVICE_CARDS,
        subs: &[],
    },
    Row {
        word: "publish",
        lands: DEVICE_CARDS,
        subs: &[],
    },
    Row {
        word: "registry",
        lands: DEVICE_CARDS,
        subs: &[("mirror", DEVICE_CARDS), ("status", DEVICE_CARDS)],
    },
    Row {
        word: "install",
        lands: DEVICE_CARDS,
        subs: &[],
    },
    Row {
        word: "resolve",
        lands: DEVICE_CARDS,
        subs: &[],
    },
    Row {
        word: "attest-build",
        lands: DEVICE_CARDS,
        subs: &[],
    },
    Row {
        word: "search",
        lands: DEVICE_CARDS,
        subs: &[],
    },
    Row {
        word: "yank",
        lands: DEVICE_CARDS,
        subs: &[],
    },
    Row {
        word: "report",
        lands: DEVICE_CARDS,
        subs: &[],
    },
    Row {
        word: "blob",
        lands: DEVICE_CARDS,
        subs: &[("fetch", DEVICE_CARDS)],
    },
    Row {
        word: "flash",
        lands: DEVICE_CARDS,
        subs: &[],
    },
    Row {
        word: "device",
        lands: DEVICE_CARDS,
        subs: &[("gate", DEVICE_CARDS), ("receipts", DEVICE_CARDS)],
    },
];

/// What a command word of a later row resolves to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolved {
    /// The command, its words joined by one space, and the row that lands it.
    Later {
        /// The command as typed, such as `db status`.
        command: String,
        /// When it lands, [`LATER_RELEASE`] or [`DEVICE_CARDS`].
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
/// follow it, `None` for a word no later row holds.
///
/// The first operand names the sub command of a word that has sub commands.
#[must_use]
pub fn resolve(word: &str, operands: &[String]) -> Option<Resolved> {
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
    Some(Resolved::Later {
        command: format!("{word} {name}"),
        lands,
    })
}

/// Returns the refusal of the later command `command`, which lands with
/// `lands`: `AO-C0201` for a verb of device cards and `AO-C0601` for a
/// command of a later release.
#[must_use]
pub fn refusal(command: &str, lands: &str) -> CliError {
    if lands == DEVICE_CARDS {
        codes::card_track_verb(command)
    } else {
        codes::later_release(command, lands)
    }
}
