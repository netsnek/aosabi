// SPDX-License-Identifier: MIT
/*
 * graphql-over-http: the cache of parsed and validated documents, the step of
 * the function that stands in the hook layer.
 *
 * Derived from graphql-yoga 5.18.0, cjs/plugins/use-parser-and-validation-cache.js, MIT.
 * Derived from graphql-yoga 5.18.0, cjs/utils/create-lru-cache.js, MIT.
 */

//! `use_parser_and_validation_cache`.
//!
//! The cache keeps the document of a text and the validation errors of a
//! document under a list of rules and a schema, so a repeated text skips the
//! parse and a repeated document skips the validation. It acts on the parse and
//! the validation the hook layer of `aosabi-hooks` runs, so it is a plugin of
//! that layer, which the function places after the plugins of the app as Yoga's
//! list places it, and which the probes of `60_request_cache` place in their own
//! rigs. Both live in one table of `aosabi-cache`, under the four keys of Yoga:
//!
//! | Part | How it enters |
//! | --- | --- |
//! | The text | Its keyed hash chooses where the table looks, and the body of the source the kept document holds decides every hit, so the table keeps no copy of a text |
//! | The schema | A weak handle, alive and at the address of the schema the validation names, as Yoga's `WeakMap` by schema |
//! | The rules | The names of the rules in order, as Yoga joins them into its key |
//! | The document | The entry of the text, through the record of the last parse and the address check below, as Yoga's `WeakMap` by document |
//!
//! A verdict therefore applies exactly where Yoga's applies, so every answer
//! stays the one Yoga gives. The table holds at most 1,024 documents, Yoga's
//! bound. A new document enters probation and a hit moves it to protected, and
//! at that bound the table drops the tail of probation, so a document hit while
//! a flood of new texts runs stays where Yoga's list of least recent use would
//! drop it. Where the window of its index that a new text hashes to is full, it
//! also drops the probation entry of that window used longest ago, or the
//! protected one where the window holds no probation entry, so it can hold fewer
//! documents. The budget of the card a build serves bounds the estimated bytes
//! of the table and of one entry as well, through the field
//! `parser_and_validation_cache_budget` of the options of Yoga, and with no
//! budget the table has no bound in bytes. A drop of any kind only makes a text
//! parse again, which answers the same document, and so does Yoga's lifetime of
//! an hour, which therefore has no reading.
//!
//! A kept value is a function of its key alone while two rules hold, which the
//! plugin relies on and the check of section 7 of
//! `tests/notebooks/60_request_cache.ipynb` holds over the workspace and the
//! fixtures of the matrix:
//!
//! 1. A validation rule reads only its context, the schema, the document and
//!    the rules, and no `thread_local!` or `static` with interior mutability.
//! 2. A parse or validate function that a plugin installs with `set_parse_fn`
//!    or `set_validation_fn` is built outside the hook and captures nothing
//!    of the payload, so it reads no state of a request.
//!
//! Yoga keys the same way and shares both cases, so a function that broke a
//! rule would answer the same on both sides, and the rules keep that answer
//! the one a request computes without the cache.
//!
//! The engine of the single pass, `Engine::graphql_single_pass` of
//! `aosabi-hooks`, holds the second rule in a narrower form. Its validate
//! function is built outside every hook and captures nothing of a payload, but
//! it reads a state of its engine that requests change: the weak handle of the
//! last document its parse function returned and the weak handle of the schema
//! its validation hook saw. It answers no error without the rules only where the
//! pass accepts and its gates of the rules, the document and the schema hold,
//! where `validate` answers no error and throws nothing, and every other
//! validation runs `validate` itself. So a kept verdict stays a function of its
//! key under that engine as well. A document the parse hook hands back without a
//! kept verdict passes its gate of the document only while it is the last one
//! the parse function returned, and meets the rules otherwise, which is slower
//! and never another answer.
//!
//! The parse hook records the address of the document it answers. The validation
//! hook reads a verdict, and its done hook keeps one, only where the address of
//! the validated document, taken in the validation hook, is the recorded one and
//! the recorded entry still holds it. A text whose document would pass the bound
//! of one entry even with no token never enters the table, so the parse hook
//! returns before it hashes the text, and the validation of its document meets
//! no record of it and runs uncached, as it runs after the table refuses a
//! document. A plugin that replaces the document between the parse and the
//! validation therefore meets no verdict of another document, and the validation
//! runs uncached. Every hook drops its borrow of the table before it calls into
//! the payload of the orchestrator, so a plugin that parses inside a hook never
//! meets a borrowed table.
//!
//! The entry of a text holds the document in a [`KeptDocument`] of the executor,
//! which keeps the fragment map of the document and its field plan under each
//! schema it ran against, so a request of a kept text collects no field and
//! builds no map. The executor finds the kept document by the address of the
//! document a request executes, so every request reads the plan of its own
//! document, however its context factory suspends between the parse and the
//! execution and whatever another request parsed meanwhile, and no record of the
//! last parse is read on the way to a plan. The map and the plan weigh on the
//! entry, which their executions add to through [`Table::add_weight`] and which
//! an eviction drops with its document, and a plan whose schema is gone leaves
//! at the next execution of its document and frees its bytes, as a verdict whose
//! schema is gone does. The weight of an entry also counts the table of line
//! starts its source builds once an error of the document is located.
//!
//! A hit sets the kept document through the mark of a document whose own source
//! holds the text, since the table compared the two, so the orchestrator keeps
//! the body as the text of the document and compares them no second time.
//!
//! The reference also keeps the error of a text that failed to parse and
//! throws the same object again, which the check of a mutation over GET
//! stamped in place after the first parse. An error of the model is no shared
//! object the stamp could change, so a failing text parses again here and the
//! stamp runs again, which answers the same error, because the stamp only
//! fills what is absent. The validation errors are kept as they leave the
//! validation, before the stamp of a validation error, and the stamp runs
//! again on a copy of them at each cached answer for the same reason, so a
//! hit of a document without errors allocates nothing and a hit with errors
//! allocates their copy.

use core::{
    cell::RefCell,
    mem::size_of, //
};
use std::{
    rc::{
        self,
        Rc, //
    },
    sync::{
        Arc,
        Weak, //
    }, //
};

use aosabi_cache::{
    CacheBudget,
    DocumentHandle,
    ESTIMATE,
    Kept,
    RuleList,
    SchemaHandle,
    Table,
    TextHash, //
};
use aosabi_execute::{
    Value,
    errors::copy_graphql_error,
    execute::SharedDocument,
    plan::{
        Keeper,
        KeptDocument, //
    }, //
};
use aosabi_hooks::{
    HookPlugin,
    plugin::{
        OnParse,
        OnParseDoneHook,
        OnValidate,
        OnValidateDoneHook, //
    }, //
};
use aosabi_json::value::KeyState;
use aosabi_lang::{
    GraphQLError,
    ast::AstNode,
    location::SourceLocation, //
};
use aosabi_type::Schema;

/// The bound of the document cache, `DEFAULT_MAX`.
pub const DEFAULT_MAX: usize = 1024;

/// The budget of the table: Yoga's bound of entries and no bound in bytes.
const BUDGET: CacheBudget = CacheBudget {
    entries: DEFAULT_MAX,
    bytes: usize::MAX,
    entry_bytes: usize::MAX,
};

/// A parsed document as the table keeps it, with the fragment map and the field plans of its executions.
struct Document(Rc<KeptDocument>);

impl DocumentHandle for Document {
    fn address(&self) -> usize {
        SharedDocument::as_ptr(self.0.document()).addr()
    }

    fn holds_text(&self, text: &str) -> bool {
        self.0
            .document()
            .loc
            .as_ref()
            .is_some_and(|loc| loc.source.body_eq(text))
    }
}

/// The keeper of a kept document, the entry of its text in the table.
///
/// The table holds the entry, the entry the kept document and the kept
/// document its keeper, so the keeper holds the table by a weak handle.
struct EntryKeeper {
    /// The table.
    table: rc::Weak<RefCell<Documents>>,
    /// The hash of the text of the document, which names the window of its entry.
    hash: u64,
    /// The address of the document.
    address: usize,
}

impl Keeper for EntryKeeper {
    fn admit(&self, bytes: usize) -> bool {
        let Some(table) = self.table.upgrade() else {
            return false;
        };
        // A table that is borrowed admits nothing, so no plan is kept where
        // the weight of its entry cannot follow it.
        let Ok(mut table) = table.try_borrow_mut() else {
            return false;
        };
        table.add_weight(self.hash, self.address, bytes)
    }

    fn free(&self, bytes: usize) {
        if let Some(table) = self.table.upgrade()
            && let Ok(mut table) = table.try_borrow_mut()
        {
            table.free_weight(self.hash, self.address, bytes);
        }
    }
}

/// The bytes one line of the table of line starts of a source holds, the offset
/// of its start in code units and in bytes, four bytes each.
const LINE_BYTES: usize = 8;

/// The bytes of the table of line starts of a source beside its lines, its box,
/// two boxed slices and a count.
const LINE_TABLE_BYTES: usize = 6 * size_of::<usize>();

/// Returns the bytes the table of line starts of `text` holds at most.
///
/// Each line feed and each carriage return starts a line at most once, and the
/// table is built once an error of the document is located.
fn line_table_bytes(text: &[u8]) -> usize {
    let breaks = text
        .iter()
        .filter(|&&byte| byte == b'\n' || byte == b'\r')
        .count();
    LINE_TABLE_BYTES.saturating_add(LINE_BYTES.saturating_mul(breaks))
}

/// The schema of a verdict, which keeps no schema alive, as Yoga's `WeakMap` by schema.
struct SchemaRef(Weak<Schema>);

impl SchemaHandle for SchemaRef {
    fn address(&self) -> usize {
        self.0.as_ptr().addr()
    }

    fn is_alive(&self) -> bool {
        self.0.strong_count() > 0
    }
}

/// The table of the plugin, whose verdicts are the errors of a validation.
type Documents = Table<Document, SchemaRef, Vec<GraphQLError>>;

/// The keyed hash of the text of a request, the one call `SipHash-1-3` of
/// [`KeyState::hash_bytes`] on every target.
///
/// The state takes the key `aosabi_json::value::seed` stored for its thread when
/// the plugin is made, and the fixed key where none was stored. The boot of a
/// Linux card keys its thread from the randomness it installs before its first
/// pass, which makes the plugin, so a client that does not know the key cannot
/// choose texts that share a window of the table, and a Worker keeps the fixed
/// key until its boot calls `seed`, as before. The one call keeps at opt-level z
/// the speed of opt-level 3, where the streaming `SipHash-1-3` of the standard
/// library keeps its rounds out of line and its state in memory. The hash steers
/// the window of the table alone, and the whole text decides every hit, so the
/// function the hash computes changes no answer.
struct TextKeys(KeyState);

impl TextHash for TextKeys {
    #[inline]
    fn hash_text(&self, text: &[u8]) -> u64 {
        self.0.hash_text(text)
    }
}

/// Returns the tokens and the UTF-16 code units `node` spans.
///
/// A node that carries no location spans one token and no unit.
fn span(node: &AstNode) -> (usize, usize) {
    node.loc().map_or((1, 0), |loc| {
        (
            loc.end_token
                .index()
                .saturating_sub(loc.start_token.index())
                + 1,
            loc.end.saturating_sub(loc.start),
        )
    })
}

/// Returns the estimated bytes of the errors of a verdict.
///
/// Each error weighs [`aosabi_cache::Estimate::error`] of its size, its message
/// and its locations, and each node it blames [`aosabi_cache::Estimate::node`]
/// of its size, the tokens it spans and the UTF-16 code units of its text. The
/// table adds its own record of the verdict.
fn errors_weight(errors: &[GraphQLError]) -> usize {
    errors
        .iter()
        .map(|error| {
            let locations = error
                .locations
                .as_ref()
                .map_or(0, |locations| locations.len() * size_of::<SourceLocation>());
            let nodes: usize = error.nodes.as_ref().map_or(0, |nodes| {
                nodes
                    .iter()
                    .map(|node| {
                        let (tokens, units) = span(node);
                        ESTIMATE.node(size_of::<AstNode>(), tokens, units)
                    })
                    .sum()
            });
            ESTIMATE.error(size_of::<GraphQLError>(), error.message.len(), locations) + nodes
        })
        .sum()
}

/// The cache plugin.
struct UseParserAndValidationCache {
    /// The table, shared with the done hooks.
    table: Rc<RefCell<Documents>>,
    /// The key of the hash of a text.
    keys: TextKeys,
    /// The estimated bytes of one entry at most, the bound the table holds.
    entry_bytes: usize,
}

impl HookPlugin for UseParserAndValidationCache {
    fn on_parse(&self, p: &mut OnParse<'_>) -> Result<Option<OnParseDoneHook>, Value> {
        if ESTIMATE.document(0, p.source.len()) > self.entry_bytes {
            return Ok(None);
        }
        let hash = self.keys.hash_text(p.source.as_bytes());
        let hit = self
            .table
            .borrow_mut()
            .lookup(hash, p.source)
            .map(|document| SharedDocument::clone(document.0.document()));
        if let Some(document) = hit {
            // The lookup compared the body of the document's source with the
            // text, so the document carries the mark that skips the second
            // compare.
            p.set_parsed_document_of_source(document);
            return Ok(None);
        }
        let table = Rc::clone(&self.table);
        let len = p.source.len();
        Ok(Some(Box::new(move |done| {
            let Ok(document) = done.result else {
                return;
            };
            // A document that holds no text of this length never answers a
            // lookup of this text, so it takes no entry.
            let Some(loc) = document
                .loc
                .as_ref()
                .filter(|loc| loc.source.bytes().len() == len)
            else {
                return;
            };
            let tokens = document.token_count.unwrap_or(len);
            let weight = ESTIMATE
                .document(tokens, len)
                .saturating_add(line_table_bytes(loc.source.bytes()))
                .saturating_add(KeptDocument::bytes())
                .saturating_add(size_of::<EntryKeeper>());
            let keeper = EntryKeeper {
                table: Rc::downgrade(&table),
                hash,
                address: SharedDocument::as_ptr(document).addr(),
            };
            let kept = KeptDocument::keep(document, Box::new(keeper));
            table.borrow_mut().insert(hash, Document(kept), weight);
        })))
    }

    fn on_validate(&self, p: &mut OnValidate<'_>) -> Option<OnValidateDoneHook> {
        let address = core::ptr::from_ref(p.document).addr();
        let names = p.rules().unwrap_or_default().iter().map(|rule| rule.name);
        let schema = Arc::as_ptr(p.schema).addr();
        let kept: Result<Vec<GraphQLError>, RuleList> =
            match self.table.borrow_mut().verdict(address, names, schema) {
                Kept::Hit(errors) => Ok(errors.iter().map(copy_graphql_error).collect()),
                Kept::Miss(rules) => Err(rules),
                Kept::Uncached => return None,
            };
        let rules = match kept {
            Ok(errors) => {
                p.set_result(errors);
                return None;
            }
            Err(rules) => rules,
        };
        let table = Rc::clone(&self.table);
        let schema = SchemaRef(Arc::downgrade(p.schema));
        Some(Box::new(move |done| {
            let errors: Vec<GraphQLError> = done.result.iter().map(copy_graphql_error).collect();
            let weight = errors_weight(&errors);
            table
                .borrow_mut()
                .add_verdict(address, rules, schema, errors, weight);
        }))
    }
}

/// Returns the cache plugin with an empty table, `useParserAndValidationCache({})`.
#[must_use]
pub fn use_parser_and_validation_cache() -> Rc<dyn HookPlugin> {
    use_parser_and_validation_cache_with(None)
}

/// Returns the cache plugin with an empty table under `budget`.
///
/// With no budget the table keeps Yoga's bound and no bound in bytes. The server
/// passes the budget of its options, which the card a build serves sets.
pub(crate) fn use_parser_and_validation_cache_with(
    budget: Option<CacheBudget>,
) -> Rc<dyn HookPlugin> {
    let table = Table::new(budget.unwrap_or(BUDGET));
    Rc::new(UseParserAndValidationCache {
        entry_bytes: table.budget().entry_bytes,
        table: Rc::new(RefCell::new(table)),
        keys: TextKeys(KeyState::default()),
    })
}
