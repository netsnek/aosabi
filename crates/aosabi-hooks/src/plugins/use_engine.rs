// SPDX-License-Identifier: MIT
/*
 * aosabi-hooks: the engine plugin, which binds the functions of an engine
 * into every request.
 *
 * The engine of the single pass, Engine::graphql_single_pass, has no analogue in
 * the reference.
 */

//! `use_engine`.
//!
//! The engine plugin binds the functions of an engine into every request:
//! `setExecuteFn`, `setParseFn`, `setValidationFn` and `setSubscribeFn`, and
//! adds every specified rule to the rules of the validation. Yoga's server
//! installs it first with `parse`, `validate` and the specified rules of
//! graphql-js and with `normalizedExecutor` as both execute and subscribe, which
//! [`Engine::graphql`] is: `aosabi_lang::parse`, `aosabi_validate::validate`,
//! its specified rules and `normalized_executor`, run on the agent the caller
//! passes.
//!
//! # The engine of the single pass
//!
//! [`Engine::graphql_single_pass`] is the same engine with the single pass of
//! `aosabi_validate::pass` in the seat of the validator, Aosabi only. Its parse
//! function, its validation hook and its validate function share one state in an
//! `Rc<RefCell<_>>` that the constructor builds, and no global. The parse
//! function parses as [`Engine::graphql`] does and records a weak handle of the
//! document it returns. The validation hook records a weak handle of the schema
//! of the validation. The validate function runs the pass only where five gates
//! hold, in their order:
//!
//! 1. The option is on.
//! 2. The rules are the specified rules of the engine in their order, each
//!    the same by its name and by the address of its function, alone or
//!    followed by the one extra rule the engine knows, which turns on the
//!    refusal of introspection in the pass.
//! 3. The document is the last one the parse function returned, at the
//!    recorded address with a live handle. While the handle lives no plugin
//!    changes the document in place, since `Arc::get_mut` fails on it, and a
//!    document a plugin replaced lies at another address. A validate function
//!    that runs while the state is borrowed, which no path of either road reaches,
//!    cannot read the record and counts this gate closed.
//! 4. The schema is the recorded one, at its address with a live handle,
//!    `assert_valid_schema` answers `Ok` for it, and the field table of the
//!    pass is built from it, or builds now over every thunk of the schema.
//! 5. The document holds its location, whose source is the text the pass
//!    reads.
//!
//! Where every gate holds and the pass accepts, the validate function answers
//! no error, which is what `validate` answers there, since the gates of the
//! rules, of the document and of the schema leave the rules no error and no
//! throw to give. Every other validation, a refusal and a defer of the pass
//! among them, runs `validate` as [`Engine::graphql`] runs it, so the rules
//! write every error. The engine counts its verdicts and the gates that
//! closed into the [`PassCounts`] of its settings, which only an instance
//! that judges the pass hands it.

use core::cell::{
    Cell,
    RefCell, //
};
use std::{
    rc::Rc,
    sync::{
        Arc,
        Weak, //
    }, //
};

use aosabi_execute::{
    ErrorObject,
    Value,
    errors::thrown_definition_error,
    normalized_executor, //
};
use aosabi_lang::{
    GraphQLError,
    ast::DocumentNode,
    parser::{
        ParseOptions,
        parse, //
    }, //
};
use aosabi_type::{
    Schema,
    assert_valid_schema, //
};
use aosabi_validate::{
    SPECIFIED_RULES,
    ValidateOptions,
    ValidationRule,
    pass::{
        self,
        Bufs,
        LangFeed,
        Options,
        OwnedTable,
        Reason,
        Verdict, //
    },
    validate, //
};

use crate::plugin::{
    ExecuteFn,
    HookPlugin,
    OnExecute,
    OnExecuteHooks,
    OnParse,
    OnParseDoneHook,
    OnSubscribe,
    OnSubscribeHooks,
    OnValidate,
    OnValidateDoneHook,
    ParseFn,
    ValidateFn, //
};

/// The function the engine plugin hands the schema of every validation.
pub type SchemaSeenFn = Rc<dyn Fn(&Arc<Schema>)>;

/// The functions of an engine, `{ parse, validate, execute, subscribe, specifiedRules }`.
#[derive(Clone, Default)]
pub struct Engine {
    /// The parse function.
    pub parse: Option<ParseFn>,
    /// The validate function.
    pub validate: Option<ValidateFn>,
    /// The execute function.
    pub execute: Option<ExecuteFn>,
    /// The subscribe function.
    pub subscribe: Option<ExecuteFn>,
    /// The rules every validation adds.
    pub specified_rules: Option<Vec<ValidationRule>>,
    /// Aosabi only: the function the engine plugin hands the schema of every
    /// validation before the validate function runs, through which the
    /// engine of the single pass records its handle, and `None` for none.
    pub schema_seen: Option<SchemaSeenFn>,
}

/// Parses `source` as the parse function of the engine of Yoga parses it.
///
/// A parse error is the thrown `GraphQLError`.
fn parse_document(source: &str) -> Result<Arc<DocumentNode>, Value> {
    parse(source, ParseOptions::default())
        .map(Arc::new)
        .map_err(|error| Value::from(ErrorObject::GraphQL(error)))
}

/// Validates `document` against `schema` as the validate function of the
/// engine of Yoga validates it, with `rules` or the specified rules.
///
/// A schema the validation refuses is the throw of the type layer.
fn validate_document(
    schema: &Schema,
    document: &DocumentNode,
    rules: Option<&[ValidationRule]>,
) -> Result<Vec<GraphQLError>, Value> {
    validate(schema, document, rules, ValidateOptions::default(), None)
        .map_err(thrown_definition_error)
}

impl Engine {
    /// Returns the engine of `parse_fn`, `validate_fn`, the rules `specified`
    /// and `schema_seen`, with `normalizedExecutor` as both execute and
    /// subscribe.
    fn with_normalized_executor(
        parse_fn: ParseFn,
        validate_fn: ValidateFn,
        specified: Vec<ValidationRule>,
        schema_seen: Option<SchemaSeenFn>,
    ) -> Self {
        let execute_fn: ExecuteFn = Rc::new(normalized_executor);
        Self {
            parse: Some(parse_fn),
            validate: Some(validate_fn),
            execute: Some(Rc::clone(&execute_fn)),
            subscribe: Some(execute_fn),
            specified_rules: Some(specified),
            schema_seen,
        }
    }

    /// Returns the engine the HTTP server installs: graphql-js with `normalizedExecutor`.
    ///
    /// A parse error is the thrown `GraphQLError`, and a schema the validation
    /// refuses is the throw of the type layer.
    #[must_use]
    pub fn graphql() -> Self {
        Self::with_normalized_executor(
            Rc::new(parse_document),
            Rc::new(validate_document),
            SPECIFIED_RULES.to_vec(),
            None,
        )
    }

    /// Returns the engine of [`Engine::graphql`] with the single pass in the
    /// seat of the validator and the rules for every document it does not
    /// accept.
    ///
    /// The parse function, the validation hook and the validate function
    /// are built here, outside every hook, and share the state of the module
    /// documentation, so the engine holds no global. An acceptance answers
    /// no error, and every other validation runs the validate function of
    /// [`Engine::graphql`].
    #[must_use]
    pub fn graphql_single_pass(settings: SinglePass) -> Self {
        let specified = SPECIFIED_RULES.to_vec();
        let shared = Rc::new(RefCell::new(PassState::default()));
        let gates = Gates {
            on: settings.on,
            options: Options {
                max_depth: settings.max_depth,
                no_schema_introspection: false,
            },
            specified: specified.clone(),
            extra: settings.extra_rule,
            counts: settings.counts,
        };
        let parsed = Rc::clone(&shared);
        let parse_fn: ParseFn = Rc::new(move |source: &str| {
            let document = parse_document(source)?;
            if let Ok(mut state) = parsed.try_borrow_mut() {
                state.document = Arc::downgrade(&document);
            }
            Ok(document)
        });
        let seen = Rc::clone(&shared);
        let schema_seen: SchemaSeenFn = Rc::new(move |schema: &Arc<Schema>| {
            if let Ok(mut state) = seen.try_borrow_mut() {
                state.see(schema);
            }
        });
        let validate_fn: ValidateFn = Rc::new(
            move |schema: &Schema, document: &DocumentNode, rules: Option<&[ValidationRule]>| {
                let outcome = gates.decide(&shared, schema, document, rules);
                gates.count(outcome);
                if outcome == Outcome::Ran(Verdict::Accept) {
                    return Ok(Vec::new());
                }
                validate_document(schema, document, rules)
            },
        );
        Self::with_normalized_executor(parse_fn, validate_fn, specified, Some(schema_seen))
    }
}

/// The settings of [`Engine::graphql_single_pass`].
#[derive(Clone)]
pub struct SinglePass {
    /// Whether the pass runs, gate 1. An engine with the option off counts
    /// every validation at gate 1 and runs the rules for it.
    pub on: bool,
    /// The most levels a document may nest before the pass leaves it to the
    /// rules, the bound of the card a build serves.
    pub max_depth: u32,
    /// The one rule that may follow the specified rules, the refusal of
    /// introspection that turns on its check in the pass, and `None` for
    /// none.
    pub extra_rule: Option<ValidationRule>,
    /// The counters of the verdicts and of the gates that closed, and `None`
    /// to count nothing.
    pub counts: Option<Rc<PassCounts>>,
}

impl Default for SinglePass {
    /// Returns the pass on with the bound of `pass::MAX_DEPTH`, no extra rule
    /// and no counters.
    ///
    /// A served build takes two settings beyond these, which the server of
    /// `graphql-over-http` hands every engine it builds: the refusal of
    /// introspection as the extra rule, and the bound of its options, which
    /// `build_http_options` of `aosabi-core` sets to the bound of the card the build
    /// serves.
    fn default() -> Self {
        Self {
            on: true,
            max_depth: pass::MAX_DEPTH,
            extra_rule: None,
            counts: None,
        }
    }
}

/// A gate of the engine of the single pass, which sends a validation to the
/// rules when it closes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gate {
    /// Gate 1, the option is off.
    Option,
    /// Gate 2, the rules are not the specified rules of the engine, alone or
    /// followed by its extra rule.
    Rules,
    /// Gate 3, the document is not the last one the parse function returned,
    /// alive at the recorded address, or the record is out of reach, since
    /// the validate function runs while the state of the engine is borrowed.
    Document,
    /// Gate 4, the schema is not the recorded one alive at its address, is
    /// invalid, or yields no field table.
    Schema,
    /// Gate 5, the document holds no location.
    Location,
}

impl Gate {
    /// Every gate in its order, the order of [`PassTally::closed`].
    pub const ALL: [Self; GATES] = [
        Self::Option,
        Self::Rules,
        Self::Document,
        Self::Schema,
        Self::Location,
    ];

    /// Returns the number of the gate, from 1 to 5, its place in [`Gate::ALL`] plus one.
    #[must_use]
    pub const fn number(self) -> usize {
        self as usize + 1
    }
}

/// The count of gates, the length of [`PassTally::closed`].
pub const GATES: usize = 5;

/// The count of the reasons of a defer, the length of [`PassTally::deferred`].
pub const REASONS: usize = 10;

/// Every reason of a defer, in the order of [`PassTally::deferred`].
pub const REASON_ORDER: [Reason; REASONS] = [
    Reason::Subscription,
    Reason::Scalar,
    Reason::OneOf,
    Reason::MetaCorner,
    Reason::Digits,
    Reason::Depth,
    Reason::Wide,
    Reason::VariableDirective,
    Reason::Syntax,
    Reason::Internal,
];

/// Returns the place of `reason` in [`REASON_ORDER`].
const fn reason_slot(reason: Reason) -> usize {
    match reason {
        Reason::Subscription => 0,
        Reason::Scalar => 1,
        Reason::OneOf => 2,
        Reason::MetaCorner => 3,
        Reason::Digits => 4,
        Reason::Depth => 5,
        Reason::Wide => 6,
        Reason::VariableDirective => 7,
        Reason::Syntax => 8,
        Reason::Internal => 9,
    }
}

/// What an engine of the single pass counted: each verdict of the pass, each
/// reason of a defer and each gate that closed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PassTally {
    /// The validations the pass accepted, which answered no error without
    /// the rules.
    pub accepted: u64,
    /// The validations the pass refused, which the rules answered.
    pub refused: u64,
    /// The validations the pass deferred, by the reason of [`REASON_ORDER`],
    /// which the rules answered.
    pub deferred: [u64; REASONS],
    /// The validations a gate sent to the rules before the pass ran, by the
    /// gate of [`Gate::ALL`].
    pub closed: [u64; GATES],
}

impl PassTally {
    /// Returns the validations the pass deferred for `reason`.
    #[must_use]
    pub fn deferred_for(&self, reason: Reason) -> u64 {
        self.deferred
            .get(reason_slot(reason))
            .copied()
            .unwrap_or_default()
    }

    /// Returns the validations `gate` sent to the rules.
    #[must_use]
    pub fn closed_at(&self, gate: Gate) -> u64 {
        self.closed
            .get(gate.number() - 1)
            .copied()
            .unwrap_or_default()
    }

    /// Returns the counts since `earlier`, each count less its count there.
    #[must_use]
    pub fn since(&self, earlier: &Self) -> Self {
        let mut deferred = self.deferred;
        for (count, before) in deferred.iter_mut().zip(earlier.deferred) {
            *count = count.saturating_sub(before);
        }
        let mut closed = self.closed;
        for (count, before) in closed.iter_mut().zip(earlier.closed) {
            *count = count.saturating_sub(before);
        }
        Self {
            accepted: self.accepted.saturating_sub(earlier.accepted),
            refused: self.refused.saturating_sub(earlier.refused),
            deferred,
            closed,
        }
    }
}

/// The counters an engine of the single pass counts into, which only an
/// instance that judges the pass reads.
#[derive(Debug, Default)]
pub struct PassCounts(Cell<PassTally>);

impl PassCounts {
    /// Returns the counts so far.
    #[must_use]
    pub fn get(&self) -> PassTally {
        self.0.get()
    }

    /// Counts `outcome` once.
    fn add(&self, outcome: Outcome) {
        let mut tally = self.0.get();
        let count = match outcome {
            Outcome::Closed(gate) => tally.closed.get_mut(gate.number() - 1),
            Outcome::Ran(Verdict::Accept) => Some(&mut tally.accepted),
            Outcome::Ran(Verdict::Refuse(_)) => Some(&mut tally.refused),
            Outcome::Ran(Verdict::Defer(reason)) => tally.deferred.get_mut(reason_slot(reason)),
        };
        if let Some(count) = count {
            *count = count.saturating_add(1);
        }
        self.0.set(tally);
    }
}

/// What the engine of the single pass did with one validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Outcome {
    /// The gate closed, so the rules answer.
    Closed(Gate),
    /// The pass ran and answered the verdict, and the rules answer every
    /// verdict but an acceptance.
    Ran(Verdict),
}

/// The state the parse function, the validation hook and the validate
/// function of one engine of the single pass share.
// An `unsafe` cell would skip the borrow flag.
#[derive(Default)]
struct PassState {
    /// The document the parse function returned last.
    document: Weak<DocumentNode>,
    /// The schema the validation hook saw last.
    schema: Weak<Schema>,
    /// The field table of the recorded schema, `None` until a validation
    /// reaches its build and where that build failed.
    table: Option<OwnedTable>,
    /// Whether the table was built for the recorded schema, whether or not
    /// its build succeeded.
    built: bool,
    /// The buffers of the pass, which keep their capacity from request to
    /// request.
    bufs: Bufs,
}

impl PassState {
    /// Records the handle of `schema` where it is not the recorded schema,
    /// and drops the table of the schema before it.
    fn see(&mut self, schema: &Arc<Schema>) {
        if core::ptr::eq(self.schema.as_ptr(), Arc::as_ptr(schema)) {
            return;
        }
        self.schema = Arc::downgrade(schema);
        self.table = None;
        self.built = false;
    }
}

/// The settings the validate function of an engine of the single pass reads.
struct Gates {
    /// Whether the pass runs, gate 1.
    on: bool,
    /// The options of every check of the pass, without the refusal of
    /// introspection, which gate 2 sets.
    options: Options,
    /// The specified rules the engine adds to every validation, the very
    /// values the engine plugin copies into the rules, so gate 2 compares
    /// every function address with its own copy.
    specified: Vec<ValidationRule>,
    /// The one rule that may follow the specified rules.
    extra: Option<ValidationRule>,
    /// The counters of the verdicts and of the gates that closed.
    counts: Option<Rc<PassCounts>>,
}

/// Returns whether `a` and `b` are the same rule, by the address of the
/// function that creates it and by name.
///
/// A rule the engine plugin copied from the engine's own list holds the very
/// name of that list, so the name compares by its address before its bytes.
fn same_rule(a: &ValidationRule, b: &ValidationRule) -> bool {
    core::ptr::fn_addr_eq(a.create, b.create) && (core::ptr::eq(a.name, b.name) || a.name == b.name)
}

impl Gates {
    /// Returns what the engine does with the validation of `document`
    /// against `schema` under `rules`: the gate that closed first, or the
    /// verdict of the pass.
    fn decide(
        &self,
        shared: &RefCell<PassState>,
        schema: &Schema,
        document: &DocumentNode,
        rules: Option<&[ValidationRule]>,
    ) -> Outcome {
        if !self.on {
            return Outcome::Closed(Gate::Option);
        }
        let Some(no_schema_introspection) = self.rules(rules) else {
            return Outcome::Closed(Gate::Rules);
        };
        // A validate function that runs while the state is borrowed is one
        // that a parse or a check of this engine reached again, which no
        // path of either road does. It cannot read the record of the last parsed
        // document, so it counts gate 3 closed and the rules answer.
        let Ok(mut state) = shared.try_borrow_mut() else {
            return Outcome::Closed(Gate::Document);
        };
        let parsed = core::ptr::eq(state.document.as_ptr(), document);
        if !parsed || state.document.strong_count() == 0 {
            return Outcome::Closed(Gate::Document);
        }
        let seen = core::ptr::eq(state.schema.as_ptr(), schema);
        if !seen || state.schema.strong_count() == 0 || assert_valid_schema(schema).is_err() {
            return Outcome::Closed(Gate::Schema);
        }
        if !state.built {
            state.table = OwnedTable::from_schema(schema).ok();
            state.built = true;
        }
        let PassState { table, bufs, .. } = &mut *state;
        let Some(table) = table.as_ref() else {
            return Outcome::Closed(Gate::Schema);
        };
        let Some(mut feed) = LangFeed::of(document) else {
            return Outcome::Closed(Gate::Location);
        };
        let options = Options {
            no_schema_introspection,
            ..self.options
        };
        Outcome::Ran(pass::check(
            &table.view(),
            &mut feed,
            &table.scalars(),
            options,
            bufs,
        ))
    }

    /// Returns whether `rules` hold the extra rule after the specified rules,
    /// or `None` where they are no such list.
    ///
    /// No rules at all are the specified rules, which `validate` runs in
    /// their place.
    fn rules(&self, rules: Option<&[ValidationRule]>) -> Option<bool> {
        let Some(rules) = rules else {
            return Some(false);
        };
        let (head, tail) = rules.split_at_checked(self.specified.len())?;
        if !head
            .iter()
            .zip(&self.specified)
            .all(|(rule, specified)| same_rule(rule, specified))
        {
            return None;
        }
        match (tail, &self.extra) {
            ([], _) => Some(false),
            ([rule], Some(extra)) if same_rule(rule, extra) => Some(true),
            _ => None,
        }
    }

    /// Counts `outcome` where the engine has counters.
    fn count(&self, outcome: Outcome) {
        if let Some(counts) = &self.counts {
            counts.add(outcome);
        }
    }
}

/// The engine plugin.
struct UseEngine(Engine);

impl HookPlugin for UseEngine {
    fn on_execute(&self, p: &mut OnExecute<'_>) -> Result<Option<OnExecuteHooks>, Value> {
        if let Some(execute) = &self.0.execute {
            p.set_execute_fn(Rc::clone(execute));
        }
        Ok(None)
    }

    fn on_parse(&self, p: &mut OnParse<'_>) -> Result<Option<OnParseDoneHook>, Value> {
        if let Some(parse) = &self.0.parse {
            p.set_parse_fn(Rc::clone(parse));
        }
        Ok(None)
    }

    fn on_validate(&self, p: &mut OnValidate<'_>) -> Option<OnValidateDoneHook> {
        if let Some(seen) = &self.0.schema_seen {
            seen(p.schema);
        }
        if let Some(validate) = &self.0.validate {
            p.set_validation_fn(Rc::clone(validate));
        }
        for rule in self.0.specified_rules.iter().flatten() {
            p.add_validation_rule(*rule);
        }
        None
    }

    fn on_subscribe(&self, p: &mut OnSubscribe<'_>) -> Result<Option<OnSubscribeHooks>, Value> {
        if let Some(subscribe) = &self.0.subscribe {
            p.set_subscribe_fn(Rc::clone(subscribe));
        }
        Ok(None)
    }
}

/// Returns the plugin that binds `engine`, `useEngine(engine)`.
///
/// Each function the engine holds replaces the one of its step, and the
/// specified rules join the rules of every validation.
#[must_use]
pub fn use_engine(engine: Engine) -> Rc<dyn HookPlugin> {
    Rc::new(UseEngine(engine))
}
