// SPDX-License-Identifier: MIT
/*
 * aosabi-core cache_budget: the budget of the request cache of each card.
 *
 * No analogue in the reference, whose cache holds 1,024 documents with no bound
 * in bytes.
 */

//! The budget of the request cache of each card.
//!
//! The plugin `use_parser_and_validation_cache` of `graphql-over-http` keeps the
//! parsed documents and the verdicts of their validations in one table, and
//! the card a build serves bounds that table by the three numbers of a
//! [`CacheBudget`]: the entries, the estimated bytes of all entries and the
//! estimated bytes of one entry, so the text of a larger entry parses every time,
//! as with no cache. A budget changes what is kept and never what is answered.
//!
//! An entry weighs its parsed document, the verdicts of its validations and the
//! kept document of the executor beside it, with the fragment map and the field
//! plans the executions of the document build, which the entry admits byte for
//! byte before the plan keeps them. So a plan stays inside the budget of its
//! card, and a document whose plan would take its entry past the bound of one
//! entry collects the rest of its plan per request.
//!
//! | Card | Entries | Bytes | One entry |
//! | --- | --- | --- | --- |
//! | `worker` | 1,024 | 16 MiB | 2 MiB |
//! | `server`, `server-legacy`, `docker` | 1,024 | 64 MiB per process | 8 MiB |
//! | `player` | 1,024 | 512 KiB | 128 KiB |
//! | `test-graphtp` | the budget of `server`, the card it stands in for | | |
//! | `esp32c3-devkitm1` | none, the card links no facade | | |
//!
//! A server runs one runtime on one thread, so the runtime keeps the budget of
//! the process, and a build with more runtimes splits it evenly over them, since
//! each keeps its own cache.
//!
//! The served path learns its card by its target until the generated package of a
//! card passes the budget: [`TARGET`] is the budget of the player on
//! `target_arch = "mips"`, of the Worker on `target_family = "wasm"` and of the
//! server on every other target. The cards `server`, `server-legacy`, `docker`
//! and the player name the same features of the facade, so no feature tells them
//! apart. `tests/notebooks/38_card_limits.ipynb` prints every budget of [`CARDS`]
//! with the entries it admits for each document of the corpus, and
//! `tests/notebooks/08_card_builds.ipynb` builds the player with the plugin and
//! its budget.

use graphql_over_http::{
    CacheBudget,
    plugins::use_parser_and_validation_cache::DEFAULT_MAX, //
};

/// One kibibyte.
const KIB: usize = 1 << 10;

/// One mebibyte.
const MIB: usize = 1 << 20;

/// The card `worker`: 16 MiB, 13.1 percent of the 128 MB of an isolate, with 2
/// MiB for one entry.
pub const WORKER: CacheBudget = CacheBudget {
    entries: DEFAULT_MAX,
    bytes: 16 * MIB,
    entry_bytes: 2 * MIB,
};

/// The cards `server`, `server-legacy` and `docker`: 64 MiB per process with 8
/// MiB for one entry.
pub const SERVER: CacheBudget = CacheBudget {
    entries: DEFAULT_MAX,
    bytes: 64 * MIB,
    entry_bytes: 8 * MIB,
};

/// The player: 512 KiB, 4.2 percent of a resident set of 12 MiB, with 128 KiB for
/// one entry.
pub const PLAYER: CacheBudget = CacheBudget {
    entries: DEFAULT_MAX,
    bytes: 512 * KIB,
    entry_bytes: 128 * KIB,
};

/// Every card that serves the request cache with its budget, by name.
///
/// The harness card `test-graphtp` takes the budget of `server`, which it
/// stands in for. The card `esp32c3-devkitm1` links no facade, so it has no
/// row.
pub const CARDS: [(&str, CacheBudget); 6] = [
    ("worker", WORKER),
    ("server", SERVER),
    ("server-legacy", SERVER),
    ("docker", SERVER),
    ("player", PLAYER),
    ("test-graphtp", SERVER),
];

/// The budget of the card the target of this build serves.
pub const TARGET: CacheBudget = if cfg!(target_arch = "mips") {
    PLAYER
} else if cfg!(target_family = "wasm") {
    WORKER
} else {
    SERVER
};
