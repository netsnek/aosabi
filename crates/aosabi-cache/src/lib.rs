// SPDX-License-Identifier: MIT
/*
 * aosabi-cache: the request side cache of Aosabi, the table of parsed
 * documents and of the verdicts of their validations.
 *
 * This file holds the module list, the re-exports and every constant of the
 * crate in one place.
 */

//! The request side cache of Aosabi.
//!
//! A [`Table`] keeps the parsed document of the text of a request and the
//! verdicts of its validations, each the payload of a validation under one
//! schema and one list of rules, and never an answer, so a value it hands
//! back is the value the same request computes without it. The table is
//! generic over the handle of a document, [`DocumentHandle`], the handle of a
//! schema, [`SchemaHandle`], and the payload of a verdict, so it names no
//! type of the GraphQL crates and no type of `alloc::sync`.
//!
//! | Module | Holds |
//! | --- | --- |
//! | `budget` | [`CacheBudget`], the bounds of a table, and [`Estimate`], the weight of an entry |
//! | `hash` | [`TextHash`], the keyed hash a caller hashes the text of a request with |
//! | `rules` | [`RuleList`], an interned list of the names of the rules of a validation |
//! | `table` | [`Table`], its slab, its index, its lists and its counters |
//!
//! The constants below are the whole tuning of the table and of its estimate,
//! so a change of the window, of the rule lists or of a constant of the
//! estimate changes this file and no other. The three bounds of a budget are
//! the caller's, since the card a build serves sets them.
//!
//! The crate builds with no standard library and needs an allocator, and it
//! holds no atomic type, so a build for `riscv32imc-unknown-none-elf`, whose
//! core has no atomic instruction, links it. There is no test in this crate,
//! which is the rule of CONTRIBUTING.md section 3. The judging notebooks are
//! `tests/notebooks/60_request_cache.ipynb`, whose section 4 holds the table
//! against a model of it in Python and whose sections 3 and 5 hold the
//! estimate and the bounds, and `tests/notebooks/08_card_builds.ipynb`, which
//! builds the crate for both targets of the ESP32-C3.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

mod budget;
mod hash;
mod rules;
mod table;

pub use crate::{
    budget::{
        CacheBudget,
        Estimate, //
    },
    hash::TextHash,
    rules::RuleList,
    table::{
        Counters,
        DocumentHandle,
        Kept,
        SchemaHandle,
        Table, //
    }, //
};

/// The slots of the index for each entry of the bound, rounded up to a power of two.
///
/// Four slots per entry hold the index at most a quarter full, 16 KiB for the
/// 1,024 entries of Yoga.
pub const SLOTS_PER_ENTRY: usize = 4;

/// The slots of the index one lookup reads at most, and the window an insert places its entry in.
///
/// An insert into a full window evicts from that window, so every entry
/// stands within this many slots of the slot its hash names, and a lookup
/// compares at most this many hashes and texts.
pub const WINDOW: usize = 8;

/// The lists of rule names a table keeps verdicts for at most.
///
/// A served Yoga validates under two lists, the specified rules with or
/// without the refusal of introspection, so eight are four times that, and a
/// validation under a ninth list runs uncached.
pub const RULE_LISTS: usize = 8;

/// The lists of recency of a table, probation and protected, which the segment of an entry names.
///
/// A new entry enters probation, a hit in probation moves its entry to the
/// head of protected and a hit in protected moves it to the head, and an
/// eviction for a bound takes the tail of probation first, so a document hit
/// while a flood of new texts runs stays in protected however many texts
/// arrive between two of its requests.
pub const SEGMENTS: usize = 2;

/// The share of the bound of bytes that protected holds at most, in percent.
///
/// Protected moves its tail to the head of probation while its entries weigh
/// more.
pub(crate) const PROTECTED_PERCENT: usize = 80;

/// The most entries a budget admits.
///
/// It keeps every slot of the slab within a `u32` and the index at 4 Mi
/// slots, 16 MiB (arithmetic, 2^20 entries of 4 slots of 4 bytes), where a
/// card asks for 1,024 entries.
pub const MAX_ENTRIES: usize = 1 << 20;

/// The estimate of a target with pointers of 64 bits.
///
/// Over 14 hostile shapes and the seven documents of the corpus the heap of a
/// parsed document less twice its text is at most 330.7 bytes per token on
/// aarch64 for every shape past 100 tokens, and a document of 3 tokens takes
/// 2,556 bytes, so the base and the charge per token lie above both.
/// `tests/notebooks/60_request_cache.ipynb` holds the estimate against the
/// bytes DHAT counts for every document and shape of its section 3. A blamed
/// node holds the string values of its subtree, which no charge per token
/// bounds, so it takes 3 bytes for each UTF-16 code unit it spans, the most
/// bytes of UTF-8 one code unit stands for, and its copy stays within its
/// weight.
pub const ESTIMATE_64: Estimate = Estimate {
    base: 2_560,
    per_token: 352,
    per_error: 64,
    per_unit: 3,
};

/// The estimate of a target with pointers of 32 bits, `wasm32` and `riscv32`.
///
/// On wasm32 the heap of a parsed document less twice its text is at most
/// 186.3 bytes per token for every shape past 100 tokens, and a document of 3
/// tokens takes 1,448 bytes. `tests/notebooks/60_request_cache.ipynb` holds
/// the estimate against the growth of linear memory per copy of every
/// document and shape of its section 3, which carries the overhead of the
/// allocator.
pub const ESTIMATE_32: Estimate = Estimate {
    base: 1_536,
    per_token: 208,
    per_error: 64,
    per_unit: 3,
};

/// The estimate of the target the crate is built for.
pub const ESTIMATE: Estimate = if cfg!(target_pointer_width = "64") {
    ESTIMATE_64
} else {
    ESTIMATE_32
};
