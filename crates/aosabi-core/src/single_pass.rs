// SPDX-License-Identifier: MIT
/*
 * aosabi-core single_pass: the build settings of the single pass, the
 * validation a build serves with and the bound on nesting of each card.
 *
 * No analogue in the reference, which validates every document with its rules
 * and bounds no nesting.
 */

//! The build settings of the single pass and the bound on nesting of each
//! card.
//!
//! Two variables of the environment of the build set them, and a crate reads
//! each with `option_env!` when it compiles, so no setting reaches a running
//! process, and cargo compiles that crate and the crates after it again when
//! the value changes:
//!
//! | Setting | Values | Unset | Read by |
//! | --- | --- | --- | --- |
//! | `AOSABI_VALIDATION` | `single-pass` or `conventional` | `single-pass` | `aosabi_validate::pass::CARRIED`, which [`SINGLE_PASS`] takes |
//! | `AOSABI_SINGLE_PASS_DEPTH` | a whole number of levels from 1 to [`MOST_LEVELS`] | the bound of the card, [`CARD`] | [`TARGET`] |
//!
//! Any other value fails the build with the message of its setting. The module
//! `aosabi_validate::pass` reads `AOSABI_VALIDATION` itself, since it chooses
//! its own types and its check by it: a build of the conventional validation
//! holds stubs in place of the table, the feed of the lexer and the buffers of
//! the pass, and its check defers every document with no walk, so no build of
//! it compiles the walk into a binary, a probe that links no `aosabi-core`
//! among them. An optimizing build inlines the stubs, so the served twin, the
//! Worker module and the RV32 probe built that way hold no symbol of the
//! module, while an unoptimized build keeps the stubs a caller calls out of
//! line. That crate compiles before this one, so the module and the server
//! read one value. [󰀄](crate::app::handler::build_http_options) sets the
//! option `single_pass` of Yoga to [`SINGLE_PASS`] and its bound on nesting to
//! [`TARGET`], so no field of `HandlerOptions` names either. Neither setting
//! changes an answer: a build of the conventional validation validates every
//! document with the rules, and the rules answer every document deeper than
//! the bound.
//!
//! The engine of the single pass of `aosabi-hooks` leaves a document that
//! nests deeper than its bound to the rules of the validation, which answer it
//! the same way, so the bound decides the stack and the speed of the pass and
//! never an answer. A level is a selection set, a list or object value or a
//! list type that opens at one point of the text, or a field below a field
//! counted through the fragment spreads between them.
//!
//! | Card | Levels |
//! | --- | --- |
//! | `worker` | 64 |
//! | `server`, `server-legacy`, `docker` | 64 |
//! | `player` | 64 |
//! | `test-graphtp` | the bound of `server`, the card it stands in for |
//! | `esp32c3-devkitm1` | none, the card links no facade |
//!
//! The stack the pass takes grows with the bound, and a card that raises it is
//! judged at its new bound.
//!
//! The served path learns its card by its target, as the budget of the
//! request cache does ([`crate::cache_budget`]): [`CARD`] is the bound of
//! the player on `target_arch = "mips"`, of the Worker on `target_family =
//! "wasm"` and of the server on every other target, until the generated
//! package of a card passes its own. The judging notebook is
//! `tests/notebooks/09_single_pass.ipynb`, which holds every bound of
//! [`CARDS`] at the bound and one level past it, and builds a twin with
//! each setting.

/// The card `worker`: 64 levels.
pub const WORKER: u32 = 64;

/// The cards `server`, `server-legacy` and `docker`: 64 levels.
pub const SERVER: u32 = 64;

/// The player: 64 levels.
pub const PLAYER: u32 = 64;

/// Every card that serves the single pass with its bound, by name.
///
/// The harness card `test-graphtp` takes the bound of `server`, which it
/// stands in for. The card `esp32c3-devkitm1` links no facade, so it has no
/// row.
pub const CARDS: [(&str, u32); 6] = [
    ("worker", WORKER),
    ("server", SERVER),
    ("server-legacy", SERVER),
    ("docker", SERVER),
    ("player", PLAYER),
    ("test-graphtp", SERVER),
];

/// The bound of the card the target of this build serves.
pub const CARD: u32 = if cfg!(target_arch = "mips") {
    PLAYER
} else if cfg!(target_family = "wasm") {
    WORKER
} else {
    SERVER
};

/// The most levels the build setting `AOSABI_SINGLE_PASS_DEPTH` may name,
/// 1,024.
///
/// The stack the pass takes at its bound grows with the bound, 27,628 bytes at
/// 64 levels on ARM64 at opt-level z. At 1,024 levels that is about 442 KB on
/// ARM64. Raising the bound changes no answer, as the module documentation
/// says.
pub const MOST_LEVELS: u32 = 1024;

/// The bound on nesting of this build: the build setting
/// `AOSABI_SINGLE_PASS_DEPTH`, or [`CARD`] where the build sets none.
pub const TARGET: u32 = match option_env!("AOSABI_SINGLE_PASS_DEPTH") {
    Some(setting) => levels(setting.as_bytes()),
    None => CARD,
};

// A build that sets AOSABI_SINGLE_PASS_DEPTH to anything but a whole number
// of levels from 1 to MOST_LEVELS fails here.
const _: () = assert!(
    TARGET >= 1 && TARGET <= MOST_LEVELS,
    "the build setting AOSABI_SINGLE_PASS_DEPTH takes a whole number of levels from 1 to 1024"
);

/// Whether this build validates through the single pass: the build setting
/// `AOSABI_VALIDATION` as `aosabi_validate::pass::CARRIED` reads it, `true`
/// for `single-pass` and where the build sets none, and `false` for
/// `conventional`.
pub const SINGLE_PASS: bool = aosabi_validate::pass::CARRIED;

/// Returns the whole number the decimal digits `digits` write, or 0 where
/// they write no number or one that a `u32` does not hold, which the
/// assertion on [`TARGET`] refuses.
const fn levels(mut digits: &[u8]) -> u32 {
    if digits.is_empty() {
        return 0;
    }
    let mut value: u32 = 0;
    while let [digit, rest @ ..] = digits {
        if !digit.is_ascii_digit() {
            return 0;
        }
        let Some(tens) = value.checked_mul(10) else {
            return 0;
        };
        let Some(next) = tens.checked_add((*digit - b'0') as u32) else {
            return 0;
        };
        value = next;
        digits = rest;
    }
    value
}
