// SPDX-License-Identifier: MIT
/*
 * aosabi-validate pass: the single pass, which checks an executable document
 * against the field table of its schema in one walk over its tokens.
 *
 * No analogue in the reference.
 */

//! The single pass of the validation layer.
//!
//! [`check`] reads the tokens of one executable document from a [`Feed`] once,
//! in the grammar order of an executable document, and checks every rule of
//! the reference but `SingleFieldSubscriptionsRule` against a [`Table`] of the
//! schema at the token that decides it. The checks that span the document run
//! after its last token over the side structures of [`Bufs`]. The answer is a
//! [`Verdict`]:
//!
//! - [`Verdict::Accept`] where every rule holds, so [`crate::validate()`]
//!   answers no error for the document.
//! - [`Verdict::Refuse`] with the first [`Rule`] the pass finds broken, which
//!   names a rule and writes no error. The rules write the errors.
//! - [`Verdict::Defer`] with the [`Reason`] the pass leaves the document to
//!   the rules for.
//!
//! A refusal and a defer are both safe, because a caller then runs the rules,
//! so the contract of the pass is that it accepts no document the rules
//! refuse. It writes no message, no location and no order of errors.
//!
//! What is checked where:
//!
//! - In the walk, at the token that decides it: the field on its parent type,
//!   the arguments of a field or a directive (known, unique, required, each
//!   literal of the right type), leaf and composite selections, directives
//!   (known, at an allowed location, unique), type conditions (known,
//!   composite, possible on the parent), variable definitions (unique, of a
//!   known input type, with a default of the right type), the introspection
//!   depth inside one definition and the bound on nesting.
//! - After the last token: the fragment names (unique, known, used, without a
//!   cycle), the spreads possible on their parent, the operation names (unique,
//!   a lone anonymous operation), the field depth and the introspection depth
//!   through fragments, the variables of each operation through its fragments
//!   (defined, used, allowed in their position) and the rule for overlapping
//!   fields with its record of compared pairs.
//!
//! The walk also writes the plan of the document into [`Bufs`]: the
//! selections in document order, each followed by its subtree, the arguments,
//! the values, the operations and the fragments.
//!
//! # The two halves
//!
//! The files `mod.rs`, `table.rs`, `feed.rs`, `scalars.rs`, `walk.rs`,
//! `after.rs` and `overlap.rs` are the core. They name `core` and `alloc`
//! alone, hold no `Arc`, no `Rc`, no atomic type and no `static`, and read
//! the document through the trait [`Feed`] and the custom scalars through the
//! trait [`Scalars`]. The file `from_schema.rs` builds the table from a schema
//! of the type layer and asks its custom scalars through their own literal
//! parse, and `lang_feed.rs` feeds the walk from the lexer of the language
//! layer. Those two are the server half.
//!
//! # No panic on a served path
//!
//! The release profile of the workspace aborts on a panic, where a defer would
//! have answered. The module reads every buffer, every row of the table and
//! every token through a checked access, forms each sum the size of a document
//! enters by a checked operation, and answers [`Reason::Internal`] for a state
//! it holds impossible. The lints below keep it so.
//!
//! # The build setting
//!
//! A build carries the single pass or the conventional validation when it
//! compiles, by the build setting `AOSABI_VALIDATION`, whose value [`CARRIED`]
//! reads. The four names a caller holds a value of the module by,
//! [`OwnedTable`], [`TableError`], [`LangFeed`] and [`Bufs`], stand for the
//! types that [`Carry`] chooses for the setting, and [`check`] runs the check
//! it chooses. A build of the single pass holds the rows of a table, the error
//! of its build, the lexer of a feed and the buffers of the walk in them and
//! walks the document. A build of the conventional validation holds stubs in
//! their place: [`from_schema::NoTable`] has no value, the error of a table is
//! the sentence [`from_schema::NO_TABLE`], [`lang_feed::NoFeed`] reads no text,
//! [`NoBufs`] keep no buffer, and the check of `Setting<false>` defers every
//! document with [`Reason::Internal`]. Each stub has the functions a caller
//! names, so the same callers compile in both builds, and no stub names the
//! walk, the checks after it, the builder of a table or the feed of the lexer,
//! so no build of the conventional validation compiles them into a binary at
//! any level of optimization.
//!
//! An optimizing build inlines the stubs, so the served twin of
//! `in-context-app`, the Worker module of `tests/budget/worker`, the RV32 probe
//! of `tests/notebooks/09_single_pass_budget.ipynb` and the host probe
//! `tests/budget/pass`, built at the conventional validation in their profiles
//! `release`, `worker`, `z` and `release`, hold no symbol of the module. The
//! host probe built so in the profile `dev` keeps out of line the stubs it
//! calls and the functions of the other types of the module it names, the
//! verdict, the options and the kind of a token among them, and no function of
//! the walk, of the checks after it, of the builder of a table, of the feed of
//! the lexer or of the buffers of the walk.
//!
//! There is no test in this module, which is the rule of CONTRIBUTING.md
//! section 3. The judging notebook is `tests/notebooks/09_single_pass.ipynb`.

#![deny(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]

mod after;
pub mod feed;
pub mod from_schema;
pub mod lang_feed;
mod overlap;
pub mod scalars;
pub mod table;
mod walk;

use alloc::vec::Vec;

pub use self::{
    feed::{
        Feed,
        Kind as TokenKind,
        Tok, //
    },
    from_schema::SchemaScalars,
    scalars::{
        Answer,
        Literal,
        NoScalars,
        Scalars, //
    },
    table::Table,
};

/// The most levels a document may nest when its caller sets no bound, 64.
///
/// A level is a selection set, a list or object value or a list type that
/// opens at one point of the text, or a field below a field counted through
/// the fragment spreads between them. The rules answer a document deeper than
/// the bound, so the bound decides the stack of the pass and never an answer.
pub const MAX_DEPTH: u32 = 64;

/// Whether the build carries the single pass, which the build setting
/// `AOSABI_VALIDATION` chooses when the build compiles.
///
/// The setting reads `single-pass`, which an unset setting means, or
/// `conventional`, and any other value fails the build with the message of the
/// assertion below. A build of the conventional validation holds the types and
/// the check of [󰀂](Setting) under the names of the module, and every
/// validation of it runs the rules.
pub const CARRIED: bool = matches!(SETTING, None | Some(b"single-pass"));

/// The build setting `AOSABI_VALIDATION` as its bytes, `None` where the build
/// sets none.
const SETTING: Option<&[u8]> = match option_env!("AOSABI_VALIDATION") {
    Some(setting) => Some(setting.as_bytes()),
    None => None,
};

// A build that sets AOSABI_VALIDATION to any other value fails here.
const _: () = assert!(
    matches!(SETTING, None | Some(b"single-pass" | b"conventional")),
    "the build setting AOSABI_VALIDATION takes single-pass or conventional"
);

/// The build setting as a type, `Setting<true>` for a build of the single
/// pass and `Setting<false>` for a build of the conventional validation,
/// which [`Carry`] reads.
pub struct Setting<const SINGLE_PASS: bool>;

/// What a build holds for its setting: the table of a schema and the error
/// of its build, the feed of a parsed document, the buffers of [`check`] and
/// the check itself.
///
/// A constant chooses no type, so the module names its types and its check
/// through this trait over [`Setting`], whose parameter is [`CARRIED`]. Both
/// choices compile in every build, as the kernel's `IS_ENABLED` keeps both
/// branches checked, and a build instantiates the generic functions of its own
/// choice alone, so only the check of `Setting<true>` names the walk.
pub trait Carry {
    /// The table a schema builds.
    type Table;
    /// Why a table does not build.
    type TableError;
    /// The feed of the text of a parsed document.
    type Feed;
    /// The buffers of a check.
    type Bufs;

    /// Checks the document `feed` reads against `table`, the work of
    /// [`check`].
    fn check<F: Feed, S: Scalars>(
        table: &Table<'_>,
        feed: &mut F,
        scalars: &S,
        options: Options,
        bufs: &mut Self::Bufs,
    ) -> Verdict;

    /// Checks the document and returns the lowest address of the stack the
    /// walk reached beside the verdict, the work of [`check_marked`].
    #[cfg(feature = "pass-stack-mark")]
    fn check_marked<F: Feed, S: Scalars>(
        table: &Table<'_>,
        feed: &mut F,
        scalars: &S,
        options: Options,
        bufs: &mut Self::Bufs,
    ) -> (Verdict, usize);
}

impl Carry for Setting<true> {
    type Table = from_schema::OwnedTable;
    type TableError = from_schema::TableError;
    type Feed = lang_feed::LangFeed;
    type Bufs = Buffers;

    fn check<F: Feed, S: Scalars>(
        table: &Table<'_>,
        feed: &mut F,
        scalars: &S,
        options: Options,
        bufs: &mut Buffers,
    ) -> Verdict {
        bufs.clear();
        let outcome = walk::run(table, feed, scalars, options, bufs);
        bufs.release_large();
        verdict_of(outcome.result)
    }

    #[cfg(feature = "pass-stack-mark")]
    fn check_marked<F: Feed, S: Scalars>(
        table: &Table<'_>,
        feed: &mut F,
        scalars: &S,
        options: Options,
        bufs: &mut Buffers,
    ) -> (Verdict, usize) {
        bufs.clear();
        let outcome = walk::run(table, feed, scalars, options, bufs);
        bufs.release_large();
        (verdict_of(outcome.result), outcome.low)
    }
}

impl Carry for Setting<false> {
    type Table = from_schema::NoTable;
    type TableError = &'static str;
    type Feed = lang_feed::NoFeed;
    type Bufs = NoBufs;

    /// Defers every document with [`Reason::Internal`] and names no walk.
    #[inline]
    fn check<F: Feed, S: Scalars>(
        _table: &Table<'_>,
        _feed: &mut F,
        _scalars: &S,
        _options: Options,
        _bufs: &mut NoBufs,
    ) -> Verdict {
        Verdict::Defer(Reason::Internal)
    }

    /// Defers every document with [`Reason::Internal`] at the address 0 and
    /// names no walk.
    #[cfg(feature = "pass-stack-mark")]
    #[inline]
    fn check_marked<F: Feed, S: Scalars>(
        _table: &Table<'_>,
        _feed: &mut F,
        _scalars: &S,
        _options: Options,
        _bufs: &mut NoBufs,
    ) -> (Verdict, usize) {
        (Verdict::Defer(Reason::Internal), 0)
    }
}

/// The table of a schema in this build: the rows of
/// [`from_schema::OwnedTable`] in a build of the single pass, and
/// [`from_schema::NoTable`], of which no value exists, in a build of the
/// conventional validation ([`Carry`]).
pub type OwnedTable = <Setting<{ CARRIED }> as Carry>::Table;

/// Why a table does not build in this build: [`from_schema::TableError`] in a
/// build of the single pass, and the sentence [`from_schema::NO_TABLE`] in a
/// build of the conventional validation ([`Carry`]), which the `Display` of
/// `str` in `core` prints, so a caller that prints it links no function of
/// the module.
pub type TableError = <Setting<{ CARRIED }> as Carry>::TableError;

/// The feed of the text of a parsed document in this build: the lexer of
/// [`lang_feed::LangFeed`] in a build of the single pass, and
/// [`lang_feed::NoFeed`], which reads no text, in a build of the
/// conventional validation ([`Carry`]).
pub type LangFeed = <Setting<{ CARRIED }> as Carry>::Feed;

/// The buffers of [`check`] in this build: [`Buffers`] in a build of the
/// single pass, and [`NoBufs`], which keep no buffer, in a build of the
/// conventional validation ([`Carry`]).
pub type Bufs = <Setting<{ CARRIED }> as Carry>::Bufs;

/// The bytes of capacity a buffer of [`Bufs`] keeps after its request, 256 KiB.
///
/// A buffer whose capacity passes this many bytes is dropped after the
/// request that grew it, so one large document pins no memory between
/// requests.
pub const KEEP_BYTES: usize = 256 * 1024;

/// The verdict of the pass on one document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Every rule the pass checks holds.
    Accept,
    /// The document breaks a rule, the first the pass finds broken.
    Refuse(Rule),
    /// The pass does not decide the document, for the reason it names.
    Defer(Reason),
}

impl Verdict {
    /// Returns the word a primitive of the seam answers for the verdict.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Accept => "accept",
            Self::Refuse(_) => "refuse",
            Self::Defer(_) => "defer",
        }
    }
}

/// A rule of the reference the pass finds broken.
///
/// Every rule of the specified list but `SingleFieldSubscriptionsRule`, which
/// the pass leaves to the rules through [`Reason::Subscription`], and the rule
/// that refuses introspection, which [`Options::no_schema_introspection`]
/// turns on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rule {
    /// `ExecutableDefinitionsRule`.
    ExecutableDefinitions,
    /// `UniqueOperationNamesRule`.
    UniqueOperationNames,
    /// `LoneAnonymousOperationRule`.
    LoneAnonymousOperation,
    /// `KnownTypeNamesRule`.
    KnownTypeNames,
    /// `FragmentsOnCompositeTypesRule`.
    FragmentsOnCompositeTypes,
    /// `VariablesAreInputTypesRule`.
    VariablesAreInputTypes,
    /// `ScalarLeafsRule`.
    ScalarLeafs,
    /// `FieldsOnCorrectTypeRule`.
    FieldsOnCorrectType,
    /// `UniqueFragmentNamesRule`.
    UniqueFragmentNames,
    /// `KnownFragmentNamesRule`.
    KnownFragmentNames,
    /// `NoUnusedFragmentsRule`.
    NoUnusedFragments,
    /// `PossibleFragmentSpreadsRule`.
    PossibleFragmentSpreads,
    /// `NoFragmentCyclesRule`.
    NoFragmentCycles,
    /// `UniqueVariableNamesRule`.
    UniqueVariableNames,
    /// `NoUndefinedVariablesRule`.
    NoUndefinedVariables,
    /// `NoUnusedVariablesRule`.
    NoUnusedVariables,
    /// `KnownDirectivesRule`.
    KnownDirectives,
    /// `UniqueDirectivesPerLocationRule`.
    UniqueDirectivesPerLocation,
    /// `KnownArgumentNamesRule`.
    KnownArgumentNames,
    /// `UniqueArgumentNamesRule`.
    UniqueArgumentNames,
    /// `ValuesOfCorrectTypeRule`.
    ValuesOfCorrectType,
    /// `ProvidedRequiredArgumentsRule`.
    ProvidedRequiredArguments,
    /// `VariablesInAllowedPositionRule`.
    VariablesInAllowedPosition,
    /// `OverlappingFieldsCanBeMergedRule`.
    OverlappingFieldsCanBeMerged,
    /// `UniqueInputFieldNamesRule`.
    UniqueInputFieldNames,
    /// `MaxIntrospectionDepthRule`.
    MaxIntrospectionDepth,
    /// `NoSchemaIntrospectionCustomRule`.
    NoSchemaIntrospectionCustom,
}

impl Rule {
    /// Returns the name of the rule function of the reference.
    ///
    /// The names are those a [`crate::ValidationRule`] carries, so a refusal
    /// and the rule list name a rule alike.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::ExecutableDefinitions => "ExecutableDefinitionsRule",
            Self::UniqueOperationNames => "UniqueOperationNamesRule",
            Self::LoneAnonymousOperation => "LoneAnonymousOperationRule",
            Self::KnownTypeNames => "KnownTypeNamesRule",
            Self::FragmentsOnCompositeTypes => "FragmentsOnCompositeTypesRule",
            Self::VariablesAreInputTypes => "VariablesAreInputTypesRule",
            Self::ScalarLeafs => "ScalarLeafsRule",
            Self::FieldsOnCorrectType => "FieldsOnCorrectTypeRule",
            Self::UniqueFragmentNames => "UniqueFragmentNamesRule",
            Self::KnownFragmentNames => "KnownFragmentNamesRule",
            Self::NoUnusedFragments => "NoUnusedFragmentsRule",
            Self::PossibleFragmentSpreads => "PossibleFragmentSpreadsRule",
            Self::NoFragmentCycles => "NoFragmentCyclesRule",
            Self::UniqueVariableNames => "UniqueVariableNamesRule",
            Self::NoUndefinedVariables => "NoUndefinedVariablesRule",
            Self::NoUnusedVariables => "NoUnusedVariablesRule",
            Self::KnownDirectives => "KnownDirectivesRule",
            Self::UniqueDirectivesPerLocation => "UniqueDirectivesPerLocationRule",
            Self::KnownArgumentNames => "KnownArgumentNamesRule",
            Self::UniqueArgumentNames => "UniqueArgumentNamesRule",
            Self::ValuesOfCorrectType => "ValuesOfCorrectTypeRule",
            Self::ProvidedRequiredArguments => "ProvidedRequiredArgumentsRule",
            Self::VariablesInAllowedPosition => "VariablesInAllowedPositionRule",
            Self::OverlappingFieldsCanBeMerged => "OverlappingFieldsCanBeMergedRule",
            Self::UniqueInputFieldNames => "UniqueInputFieldNamesRule",
            Self::MaxIntrospectionDepth => "MaxIntrospectionDepthRule",
            Self::NoSchemaIntrospectionCustom => "NoSchemaIntrospectionCustomRule",
        }
    }
}

/// Why the pass leaves a document to the rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    /// A subscription operation on a schema with a subscription root type,
    /// whose rule `SingleFieldSubscriptionsRule` the pass does not check.
    Subscription,
    /// A list or object literal at a custom scalar, or a literal a custom
    /// scalar leaves undecided, as every literal on a table with no scalar
    /// handles.
    Scalar,
    /// A literal of an input object type with `@oneOf`.
    OneOf,
    /// A `__schema` or `__type` field on a schema whose query root type is a
    /// possible type of an interface or a union, or the named return type of
    /// a field.
    MetaCorner,
    /// Two object values that the rule for overlapping fields compares and
    /// finds alike field by field, with a field name that holds a run of 16 or
    /// more digits. The rules order the fields of an object value by name and
    /// read a run of digits there as a double. A double holds every integer up
    /// to 2^53, and every sum of a run of up to 15 digits lies below 10^15 +
    /// 57, while 2^53 + 1 is a run of 16 digits that no double holds.
    Digits,
    /// Nesting beyond the bound of [`Options::max_depth`].
    Depth,
    /// A field or a directive with more than 64 argument definitions where the
    /// document names it, an input object type with more than 64 fields at an
    /// object literal, or a directive that may not repeat whose index among the
    /// directives of the schema passes 63, which the bit sets of the walk do
    /// not hold.
    Wide,
    /// A directive the schema allows on a variable definition.
    VariableDirective,
    /// A token the feed refuses, or a token the grammar of an executable
    /// document does not take.
    Syntax,
    /// A state the pass holds impossible, an index outside its buffer or a
    /// sum past its type, or a call of [`check`] in a build of the
    /// conventional validation ([`CARRIED`]).
    Internal,
}

impl Reason {
    /// Returns the name a primitive of the seam answers for the reason.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Subscription => "Subscription",
            Self::Scalar => "Scalar",
            Self::OneOf => "OneOf",
            Self::MetaCorner => "MetaCorner",
            Self::Digits => "Digits",
            Self::Depth => "Depth",
            Self::Wide => "Wide",
            Self::VariableDirective => "VariableDirective",
            Self::Syntax => "Syntax",
            Self::Internal => "Internal",
        }
    }
}

/// The bounds and the switches of one call of [`check`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    /// The most levels a document may nest, beyond which the pass defers.
    ///
    /// A server card sets the bound its build serves with, and a device sets
    /// it from the stack its firmware gives the pass.
    pub max_depth: u32,
    /// Whether the pass refuses a field whose named type is an introspection
    /// type, the test of `NoSchemaIntrospectionCustomRule`.
    pub no_schema_introspection: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            max_depth: MAX_DEPTH,
            no_schema_introspection: false,
        }
    }
}

/// Why the walk stopped before the end of the document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stop {
    /// A rule is broken.
    Refuse(Rule),
    /// The pass does not decide the document.
    Defer(Reason),
}

/// The outcome of one step of the walk.
type R<T> = Result<T, Stop>;

/// The stop of a state the pass holds impossible.
const INTERNAL: Stop = Stop::Defer(Reason::Internal);

/// The stop of a token the grammar does not take.
const SYNTAX: Stop = Stop::Defer(Reason::Syntax);

/// Returns `n` as a `u32`, or the internal stop when it does not fit.
#[inline]
fn narrow(n: usize) -> R<u32> {
    u32::try_from(n).map_err(|_| INTERNAL)
}

/// Returns `n` as a `usize`, which every `u32` fits on the targets of the
/// workspace.
#[inline]
fn widen(n: u32) -> R<usize> {
    usize::try_from(n).map_err(|_| INTERNAL)
}

/// A selection of the plan is a field.
pub const FIELD: u8 = 0;
/// A selection of the plan is an inline fragment.
pub const INLINE: u8 = 1;
/// A selection of the plan is a fragment spread.
pub const SPREAD: u8 = 2;

/// The field of a selection below a root type the schema lacks.
pub const UNTYPED: u32 = u32::MAX;

/// The index of no value in [`Bufs`].
pub const NO_VALUE: u32 = u32::MAX;

/// An `@include` or `@skip` condition of a selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cond {
    /// No such directive.
    None,
    /// A literal `if`.
    Lit(bool),
    /// A variable `if`, by the token of its name.
    Var(Tok),
}

/// One selection of the plan.
///
/// The plan lists the selections of every definition in document order, each
/// followed by its subtree, and `end` is the index past the subtree, so the
/// children of a selection are found by jumping from `end` to `end`.
#[derive(Clone, Copy, Debug)]
pub struct Sel {
    /// [`FIELD`], [`INLINE`] or [`SPREAD`].
    pub kind: u8,
    /// The parent type, [`table::NONE`] where there is no type.
    pub parent: u16,
    /// The type condition of an inline fragment, [`table::NONE`] without one.
    pub cond: u16,
    /// The index past the subtree.
    pub end: u32,
    /// A field: its reference (see [`table::fref`]) or [`UNTYPED`]. A spread:
    /// the index of its fragment once the checks after the walk resolve it.
    pub field: u32,
    /// A field: the token of the response key, the alias or the name. A
    /// spread: the token of the fragment name.
    pub key: Tok,
    /// A field: the token of the field name.
    pub name: Tok,
    /// A field: its first argument in the arguments of [`Bufs`].
    pub args: u32,
    /// A field: its count of arguments.
    pub nargs: u16,
    /// `@skip`.
    pub skip: Cond,
    /// `@include`.
    pub include: Cond,
}

/// An argument as the document gives it.
#[derive(Clone, Copy, Debug)]
pub struct ArgUse {
    /// The token of the name.
    pub name: Tok,
    /// The index of the definition among the arguments of the field or the
    /// directive, [`table::NONE`] where there is no type.
    pub arg: u16,
    /// The value in the values of [`Bufs`].
    pub val: u32,
}

/// The kinds of a value of the plan.
pub mod v {
    /// An integer literal.
    pub const INT: u8 = 0;
    /// A float literal.
    pub const FLOAT: u8 = 1;
    /// A string literal.
    pub const STRING: u8 = 2;
    /// A block string literal.
    pub const BLOCK: u8 = 3;
    /// `true`.
    pub const TRUE: u8 = 4;
    /// `false`.
    pub const FALSE: u8 = 5;
    /// `null`.
    pub const NULL: u8 = 6;
    /// An enum value.
    pub const ENUM: u8 = 7;
    /// A variable.
    pub const VAR: u8 = 8;
    /// A list, whose items follow up to its `end`.
    pub const LIST: u8 = 9;
    /// An input object, whose fields follow up to its `end`.
    pub const OBJECT: u8 = 10;
    /// A field of an input object, whose token is its name and whose value
    /// follows it.
    pub const FIELD: u8 = 11;
}

/// One value of the plan, kept in pre order with the index past its subtree.
#[derive(Clone, Copy, Debug)]
pub struct Val {
    /// One of [`v`].
    pub kind: u8,
    /// The token of the literal, of the variable name or of the field name.
    pub tok: Tok,
    /// The index past the subtree.
    pub end: u32,
}

/// A variable definition.
#[derive(Clone, Copy, Debug)]
pub struct VarDef {
    /// The token of the name.
    pub name: Tok,
    /// The named type.
    pub named: u16,
    /// The first of its wrappers in the wrappers of [`Bufs`].
    pub mods: u32,
    /// The count of its wrappers.
    pub nmods: u16,
    /// Its default value in the values of [`Bufs`], or [`NO_VALUE`].
    pub default: u32,
}

/// A use of a variable with the type of its position.
#[derive(Clone, Copy, Debug)]
pub struct Use {
    /// The token of the name.
    pub name: Tok,
    /// The named type of the position, [`table::NONE`] where there is no type.
    pub named: u16,
    /// The first of the wrappers of the position in the wrappers of uses.
    pub mods: u32,
    /// The count of those wrappers.
    pub nmods: u16,
    /// Whether the position has a default value.
    pub has_default: bool,
}

/// A fragment spread with what the checks after the walk need.
#[derive(Clone, Copy, Debug)]
pub struct SpreadRec {
    /// The token of the fragment name.
    pub name: Tok,
    /// The selection of the spread.
    pub sel: u32,
    /// The fragment, once resolved.
    pub frag: u32,
    /// The parent type where it is spread.
    pub parent: u16,
    /// The introspection list fields from the root of its definition to it.
    pub lc: u32,
    /// The introspection list fields below the nearest `__schema` or
    /// `__type` field, or `u32::MAX` outside one.
    pub intro: u32,
    /// The depth of the fields of the selection set that holds it, 1 at the
    /// root of its definition.
    pub fd: u32,
}

/// The kinds of an operation.
pub mod op {
    /// `query` or a shorthand query.
    pub const QUERY: u8 = 0;
    /// `mutation`.
    pub const MUTATION: u8 = 1;
    /// `subscription`.
    pub const SUBSCRIPTION: u8 = 2;
}

/// An operation of the document.
#[derive(Clone, Copy, Debug)]
pub struct Op {
    /// One of [`op`].
    pub kind: u8,
    /// The token of the name, the start of file token when anonymous.
    pub name: Tok,
    /// Whether the operation has a name.
    pub named: bool,
    /// The root type, [`table::NONE`] when the schema has none for the kind.
    pub root: u16,
    /// The selections of the root, from the first to the one past the last.
    pub sels: (u32, u32),
    /// The variable definitions.
    pub vars: (u32, u32),
    /// The uses of variables.
    pub uses: (u32, u32),
    /// The spreads.
    pub spreads: (u32, u32),
}

/// A fragment definition of the document.
#[derive(Clone, Copy, Debug)]
pub struct Frag {
    /// The token of the name.
    pub name: Tok,
    /// The type condition.
    pub cond: u16,
    /// The selections of the root.
    pub sels: (u32, u32),
    /// The uses of variables.
    pub uses: (u32, u32),
    /// The spreads.
    pub spreads: (u32, u32),
    /// The most introspection list fields on one path of the fragment
    /// without its spreads.
    pub local: u32,
    /// The same through its spreads, computed after the walk.
    pub max: u32,
    /// The depth of its deepest field without its spreads, 1 for a field at
    /// its root.
    pub fdl: u32,
    /// The same through its spreads, computed after the walk.
    pub fdm: u32,
}

/// The buffers of the plan and of the checks, which [`Bufs`] names in a build
/// of the single pass.
///
/// They keep their capacity from request to request, as a firmware keeps its
/// static buffers, and [`check`] drops a buffer whose capacity passes
/// [`KEEP_BYTES`] after its request.
#[derive(Default, Debug)]
pub struct Buffers {
    /// The selections.
    sels: Vec<Sel>,
    /// The arguments of fields and directives.
    args: Vec<ArgUse>,
    /// The values.
    vals: Vec<Val>,
    /// The operations.
    ops: Vec<Op>,
    /// The fragments.
    frags: Vec<Frag>,
    /// The variable definitions.
    vars: Vec<VarDef>,
    /// The wrappers of the types of variables.
    mods: Vec<u8>,
    /// The uses of variables.
    uses: Vec<Use>,
    /// The wrappers of the positions of uses.
    use_mods: Vec<u8>,
    /// The spreads.
    spreads: Vec<SpreadRec>,
    /// The fragments one operation reaches, in the order the walk of its
    /// spreads marks them.
    reached: Vec<u32>,
    /// Scratch for the orders of fragments and the stacks of the walks.
    scratch: Vec<u32>,
    /// Scratch for marks over fragments and variables.
    marks: Vec<u32>,
    /// Scratch for the names whose uniqueness is checked.
    names: Vec<Tok>,
    /// Scratch for sorted orders of arguments, input fields and variables.
    tmp: Vec<u32>,
    /// The fields of the selection sets the rule for overlapping fields
    /// compares.
    fl: Vec<u32>,
    /// The fragments those sets spread.
    fr: Vec<u32>,
    /// The pending fragments and pairs of fragments of that rule.
    work: Vec<(u32, u32)>,
    /// The pairs of a selection set and a fragment and of two fragments that
    /// rule has compared, an open addressing set of 64 bit entries.
    memo: Vec<u64>,
    /// The entries in `memo`.
    memo_len: usize,
}

impl Buffers {
    /// Returns empty buffers.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the selections of the plan of the last accepted document.
    #[must_use]
    pub fn selections(&self) -> &[Sel] {
        &self.sels
    }

    /// Returns the arguments of the plan.
    #[must_use]
    pub fn arguments(&self) -> &[ArgUse] {
        &self.args
    }

    /// Returns the values of the plan.
    #[must_use]
    pub fn values(&self) -> &[Val] {
        &self.vals
    }

    /// Returns the operations of the plan.
    #[must_use]
    pub fn operations(&self) -> &[Op] {
        &self.ops
    }

    /// Returns the fragments of the plan.
    #[must_use]
    pub fn fragments(&self) -> &[Frag] {
        &self.frags
    }

    /// Empties every buffer and keeps its capacity.
    fn clear(&mut self) {
        self.sels.clear();
        self.args.clear();
        self.vals.clear();
        self.ops.clear();
        self.frags.clear();
        self.vars.clear();
        self.mods.clear();
        self.uses.clear();
        self.use_mods.clear();
        self.spreads.clear();
        self.reached.clear();
        self.scratch.clear();
        self.marks.clear();
        self.names.clear();
        self.tmp.clear();
        self.fl.clear();
        self.fr.clear();
        self.work.clear();
    }

    /// Drops every buffer whose capacity passes [`KEEP_BYTES`].
    fn release_large(&mut self) {
        release(&mut self.sels);
        release(&mut self.args);
        release(&mut self.vals);
        release(&mut self.ops);
        release(&mut self.frags);
        release(&mut self.vars);
        release(&mut self.mods);
        release(&mut self.uses);
        release(&mut self.use_mods);
        release(&mut self.spreads);
        release(&mut self.reached);
        release(&mut self.scratch);
        release(&mut self.marks);
        release(&mut self.names);
        release(&mut self.tmp);
        release(&mut self.fl);
        release(&mut self.fr);
        release(&mut self.work);
        if self.memo.capacity().saturating_mul(size_of::<u64>()) > KEEP_BYTES {
            self.memo = Vec::new();
            self.memo_len = 0;
        }
    }
}

/// The buffers of a build of the conventional validation, which keep no
/// buffer, since such a build carries no single pass ([`CARRIED`]).
///
/// [`Bufs`] names them in such a build ([`Carry`]). Their functions are those
/// of [`Buffers`] that a caller names, so every caller compiles in both
/// builds, and each answers the empty plan of a document no walk read.
#[derive(Default, Debug)]
pub struct NoBufs;

impl NoBufs {
    /// Returns the buffers.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Returns no selection.
    #[must_use]
    pub fn selections(&self) -> &[Sel] {
        &[]
    }

    /// Returns no argument.
    #[must_use]
    pub fn arguments(&self) -> &[ArgUse] {
        &[]
    }

    /// Returns no value.
    #[must_use]
    pub fn values(&self) -> &[Val] {
        &[]
    }

    /// Returns no operation.
    #[must_use]
    pub fn operations(&self) -> &[Op] {
        &[]
    }

    /// Returns no fragment.
    #[must_use]
    pub fn fragments(&self) -> &[Frag] {
        &[]
    }
}

/// Drops `buffer` when its capacity passes [`KEEP_BYTES`].
fn release<T>(buffer: &mut Vec<T>) {
    if buffer.capacity().saturating_mul(size_of::<T>()) > KEEP_BYTES {
        *buffer = Vec::new();
    }
}

/// Sorts `list` by the bytes `key` answers for each entry.
///
/// Every sort of the walk and of the checks after it goes through this function
/// with its key behind a `dyn Fn`, so a build holds one instance of the sort of
/// `core` for each type of entry, where a closure handed to `sort_unstable_by`
/// at each place would instantiate the sort once per place, about ten kilobytes
/// of code each in a served binary. The call through the reference costs a
/// comparison two indirect calls, and the pass sorts only above the crossings
/// of SP1.
fn sort_by_bytes<'k, T: Copy>(list: &mut [T], key: &dyn Fn(T) -> &'k [u8]) {
    list.sort_unstable_by(|&a, &b| key(a).cmp(key(b)));
}

/// Checks the document `feed` reads against `table` in one pass.
///
/// `scalars` answers for a literal at a custom scalar of the table, and
/// `options` carries the bound on nesting and the switch of the rule that
/// refuses introspection. The buffers of `bufs` are emptied first, hold the
/// plan of the document after the call, and keep their capacity for the next
/// one. The check is that of [`Carry`] for the setting of the build, so a
/// build of the conventional validation, whose buffers are [`NoBufs`],
/// defers every document with [`Reason::Internal`] and holds no walk.
pub fn check<F: Feed, S: Scalars>(
    table: &Table<'_>,
    feed: &mut F,
    scalars: &S,
    options: Options,
    bufs: &mut Bufs,
) -> Verdict {
    <Setting<{ CARRIED }> as Carry>::check(table, feed, scalars, options, bufs)
}

/// Checks the document as [`check`] does and returns the lowest address of
/// the stack the walk reached beside the verdict.
///
/// The address is the lowest a local of a recursive step of the walk took,
/// so the stack a call needs is the address of the caller less it. A build
/// of the conventional validation answers as [`check`] does with the address
/// 0.
#[cfg(feature = "pass-stack-mark")]
pub fn check_marked<F: Feed, S: Scalars>(
    table: &Table<'_>,
    feed: &mut F,
    scalars: &S,
    options: Options,
    bufs: &mut Bufs,
) -> (Verdict, usize) {
    <Setting<{ CARRIED }> as Carry>::check_marked(table, feed, scalars, options, bufs)
}

/// Returns the verdict of the result of a walk.
fn verdict_of(result: R<()>) -> Verdict {
    match result {
        Ok(()) => Verdict::Accept,
        Err(Stop::Refuse(rule)) => Verdict::Refuse(rule),
        Err(Stop::Defer(reason)) => Verdict::Defer(reason),
    }
}
