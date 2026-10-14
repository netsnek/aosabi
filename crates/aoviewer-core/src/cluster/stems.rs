// SPDX-License-Identifier: MIT
/*
 * aoviewer-core cluster stems: the clusters of the name stems.
 *
 * Types that share the first word of their names, once the role and verb words
 * are stripped, form a cluster.
 */

//! The stem clusters.
//!
//! The tokenizer splits a name at underscores, at case changes and at digit
//! borders, so upper camel case, snake case and screaming case all work. A
//! case change is a lower case letter followed by an upper case one, or the
//! last upper case letter of a run that a lower case letter follows, so
//! `HTTPServer` reads as `http` and `server`. Every word is lower cased.
//!
//! The stem strips a leading `I` of an interface, trailing role words and
//! leading verb words, then groups by the first remaining word. It keeps at
//! least one word, so the `I` goes only when a word follows it and the last
//! word left is never stripped. The role words are the nine the section names
//! in [`ROLE_WORDS`]. The roles go before the verbs, so `DeleteResult` keeps
//! `delete` rather than `result`. The verbs are the words of [`VERB_WORDS`],
//! the imperatives that open the names of mutation payloads and inputs. A name
//! with no word at all goes to the other bucket.
//!
//! The groups under three members then merge as [`super`] describes. The
//! loops run over the nodes of one graph, over the bytes of one name and over
//! the words of one name, from which the loops of the stem strip one word per
//! iteration.

use alloc::{
    string::String,
    vec::Vec, //
};

use super::{
    OTHER,
    Placement,
    id32,
    type_of, //
};
use crate::{graph::NodeId, model::Kind, schema::Schema, sort::name_order};

/// The trailing role words a stem strips.
const ROLE_WORDS: [&str; 9] = [
    "connection",
    "edge",
    "input",
    "args",
    "result",
    "payload",
    "state",
    "filter",
    "aggregate",
];

/// The leading verb words a stem strips.
///
/// They are imperatives that open the name of a mutation's payload or input
/// and are no domain of their own, which leaves out a word such as `order`,
/// `link` or `import` that also names a thing.
const VERB_WORDS: [&str; 34] = [
    "activate",
    "add",
    "approve",
    "archive",
    "assign",
    "cancel",
    "create",
    "deactivate",
    "delete",
    "disable",
    "enable",
    "generate",
    "grant",
    "invite",
    "reactivate",
    "regenerate",
    "register",
    "reject",
    "remove",
    "rename",
    "reset",
    "resend",
    "restore",
    "revoke",
    "send",
    "submit",
    "unassign",
    "unlink",
    "unlock",
    "unregister",
    "update",
    "upsert",
    "validate",
    "verify",
];

/// Returns the stem groups of `schema`, in the byte order of the stems, the
/// small ones merged into their neighbours.
pub(super) fn place(schema: &Schema) -> Placement {
    let (model, graph) = (&schema.model, &schema.graph);
    let mut keyed: Vec<(String, NodeId)> = graph
        .node_ids()
        .filter(|n| !graph.nodes[n.index()].is_root)
        .map(|n| {
            let ty = model.ty(type_of(graph, n));
            (stem(model.name(ty.name), ty.kind), n)
        })
        .collect();
    keyed.sort_by(|(a, m), (b, n)| {
        a.cmp(b)
            .then_with(|| name_order(model, type_of(graph, *m), type_of(graph, *n)))
    });
    let mut names: Vec<String> = Vec::new();
    let mut at = Vec::with_capacity(keyed.len());
    for (key, n) in keyed {
        if key.is_empty() {
            at.push((n, OTHER));
            continue;
        }
        if names.last() != Some(&key) {
            names.push(key);
        }
        at.push((n, id32(names.len() - 1)));
    }
    let mut placement = Placement::new(names, graph.nodes.len());
    for (n, g) in at {
        placement.at[n.index()] = g;
    }
    placement.merge_small(graph);
    placement
}

/// Returns the stem of the name `name` of a type of kind `kind`, or the
/// empty string for a name with no word.
fn stem(name: &str, kind: Kind) -> String {
    let words = words(name);
    let mut first = 0;
    let mut end = words.len();
    if kind == Kind::Interface && end > 1 && words[0] == "i" {
        first = 1;
    }
    // Each iteration of the two loops strips one word and one word always
    // stays, so each loop runs at most `words.len()` times.
    while end - first > 1 && ROLE_WORDS.contains(&words[end - 1].as_str()) {
        end -= 1;
    }
    while end - first > 1 && VERB_WORDS.contains(&words[first].as_str()) {
        first += 1;
    }
    words.into_iter().nth(first).unwrap_or_default()
}

/// Returns the words of `name`, lower cased.
///
/// Every byte outside `[0-9A-Za-z]` separates two words, and so does a case
/// change or a digit border.
fn words(name: &str) -> Vec<String> {
    let bytes = name.as_bytes();
    let mut words = Vec::new();
    let mut start: Option<usize> = None;
    for i in 0..bytes.len() {
        if !bytes[i].is_ascii_alphanumeric() {
            if let Some(s) = start.take() {
                words.push(lower(&bytes[s..i]));
            }
            continue;
        }
        match start {
            Some(s) if border(bytes, i) => {
                words.push(lower(&bytes[s..i]));
                start = Some(i);
            }
            Some(_) => {}
            None => start = Some(i),
        }
    }
    if let Some(s) = start {
        words.push(lower(&bytes[s..]));
    }
    words
}

/// Returns whether a word ends before the byte at `i` of `bytes`, whose byte
/// before is alphanumeric as well.
fn border(bytes: &[u8], i: usize) -> bool {
    let (prev, cur) = (bytes[i - 1], bytes[i]);
    let next = bytes.get(i + 1).copied();
    let case_change = prev.is_ascii_lowercase() && cur.is_ascii_uppercase();
    let acronym_end = prev.is_ascii_uppercase()
        && cur.is_ascii_uppercase()
        && next.is_some_and(|b| b.is_ascii_lowercase());
    let digit_border = prev.is_ascii_digit() != cur.is_ascii_digit();
    case_change || acronym_end || digit_border
}

/// Returns the ASCII bytes `word` as a lower case string.
fn lower(word: &[u8]) -> String {
    word.iter()
        .map(|&b| char::from(b.to_ascii_lowercase()))
        .collect()
}
