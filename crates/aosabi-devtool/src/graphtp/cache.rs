// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool graphtp cache: what the table of aosabi-cache keeps of a
 * document under the budget of a card.
 *
 * No analogue in the reference, whose cache holds 1,024 documents with no
 * bound in bytes. The table and the estimate are those of aosabi-cache, the
 * parser that counts the tokens of a document is that of aosabi-lang, and
 * the budgets are those aosabi_core::cache_budget holds for each card.
 */

//! The operation `cache`.
//!
//! The operation `cache {card, budget, width, texts}` answers for each file
//! of the array `texts` what the request cache of a card keeps of the
//! document of its text: `tokens`, the tokens the parser of `aosabi-lang`
//! keeps on the document, `bytes`, the bytes of the text, `weight`, the
//! estimate of `aosabi-cache` on a target whose pointers have `width` bits,
//! 64 by default and 32 for a target of 32 bits, and `held`, the entries a
//! table of `aosabi-cache` under the budget holds once one copy more than
//! its bound of entries entered it, with `used`, the estimated bytes it then
//! holds, and its `refusals` and `evictions`. The budget is that of `card`
//! in `aosabi_core::cache_budget::CARDS`, or the object `budget` with its
//! `entries`, `bytes` and `entry_bytes` where the request names one. A text
//! that does not parse answers `error` with the message of the parse.
//!
//! Each copy enters under a hash of its own, `SLOTS_PER_ENTRY` positions of
//! the index after the hash of the copy before it, so no window of the index
//! fills, and the three bounds of the budget alone decide what the table
//! keeps.

use std::fs;

use aosabi_cache::{
    CacheBudget,
    DocumentHandle,
    ESTIMATE_32,
    ESTIMATE_64,
    Estimate,
    SLOTS_PER_ENTRY,
    SchemaHandle,
    Table, //
};
use aosabi_core::cache_budget;
use aosabi_json::{
    JsObject,
    JsString,
    JsValue, //
};
use aosabi_lang::ParseOptions;

use crate::homa_packet::{
    Fields,
    count, //
};

/// One copy of the document a fill enters, by its number, which is its address.
struct Held(usize);

impl DocumentHandle for Held {
    fn address(&self) -> usize {
        self.0
    }

    fn holds_text(&self, _text: &str) -> bool {
        false
    }
}

/// The schema of a fill, which validates nothing and keeps no verdict.
struct NoSchema;

impl SchemaHandle for NoSchema {
    fn address(&self) -> usize {
        0
    }

    fn is_alive(&self) -> bool {
        false
    }
}

/// Answers the operation `cache`.
pub(super) fn op_cache(request: &JsObject) -> Result<JsObject, JsString> {
    let fields = Fields::new(request, "cache");
    let budget = budget_of(&fields)?;
    let width = fields.int_or::<u64>("width", 64)?;
    let estimate = match width {
        64 => ESTIMATE_64,
        32 => ESTIMATE_32,
        _ => return Err(fields.refuse("width", "is neither 32 nor 64")),
    };
    let mut documents = Vec::new();
    for item in fields.array("texts")? {
        let JsValue::String(path) = item else {
            return Err(fields.refuse("texts", "holds a value that is no path"));
        };
        let path = path
            .as_str()
            .ok_or_else(|| fields.refuse("texts", "holds a path with a lone surrogate"))?;
        let text = fs::read_to_string(path).map_err(|error| {
            fields.refuse(
                "texts",
                &format!("names {path}, which reads as no text: {error}"),
            )
        })?;
        documents.push(JsValue::from(document_json(path, &text, budget, estimate)));
    }
    let mut payload = JsObject::with_capacity(3);
    payload.insert("budget", budget_json(&budget));
    payload.insert("width", width);
    payload.insert("documents", JsValue::Array(documents));
    Ok(payload)
}

/// Returns the budget the request names, its object `budget` or that of its card.
fn budget_of(fields: &Fields<'_>) -> Result<CacheBudget, JsString> {
    if fields.get("budget").is_some() {
        let budget = fields.nested("budget")?;
        return Ok(CacheBudget {
            entries: budget.int("entries")?,
            bytes: budget.int("bytes")?,
            entry_bytes: budget.int("entry_bytes")?,
        });
    }
    let name = fields.str("card")?;
    cache_budget::CARDS
        .iter()
        .find(|(card, _)| *card == name)
        .map(|(_, budget)| *budget)
        .ok_or_else(|| fields.refuse("card", &format!("names no card with a budget: {name}")))
}

/// Returns what a table under `budget` keeps of the document of `text`.
fn document_json(path: &str, text: &str, budget: CacheBudget, estimate: Estimate) -> JsObject {
    let mut object = JsObject::with_capacity(8);
    object.insert("path", path);
    object.insert("bytes", count(text.len()));
    let document = match aosabi_lang::parse(text, ParseOptions::default()) {
        Ok(document) => document,
        Err(error) => {
            object.insert("error", error.message);
            return object;
        }
    };
    let tokens = document.token_count.unwrap_or(text.len());
    let weight = estimate.document(tokens, text.len());
    let table = fill(budget, weight);
    let counters = table.counters();
    object.insert("tokens", count(tokens));
    object.insert("weight", count(weight));
    object.insert("held", count(table.len()));
    object.insert("used", count(table.used()));
    object.insert("refusals", counters.refusals);
    object.insert("evictions", counters.evictions);
    object
}

/// Returns a table under `budget` that one copy more than its bound of
/// entries of a document of `weight` estimated bytes entered.
fn fill(budget: CacheBudget, weight: usize) -> Table<Held, NoSchema, ()> {
    let mut table = Table::new(budget);
    let copies = table.budget().entries.saturating_add(1);
    for at in 0..copies {
        let hash = u64::try_from(at.saturating_mul(SLOTS_PER_ENTRY)).unwrap_or(u64::MAX);
        table.insert(hash, Held(at + 1), weight);
    }
    table
}

/// Returns the budget of a request cache as an object, as the operations
/// `cache` and `card` answer it.
pub(super) fn budget_json(budget: &CacheBudget) -> JsObject {
    let mut object = JsObject::with_capacity(3);
    object.insert("entries", count(budget.entries));
    object.insert("bytes", count(budget.bytes));
    object.insert("entry_bytes", count(budget.entry_bytes));
    object
}
