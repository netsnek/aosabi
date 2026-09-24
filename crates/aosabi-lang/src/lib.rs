// SPDX-License-Identifier: MIT
/*
 * aosabi-lang: the GraphQL language layer of Aosabi.
 *
 * This file holds the data model the lexer, the parser and the printer share:
 * the source, the location arithmetic, the kind enums, the token, the abstract
 * syntax tree and the GraphQL error.
 *
 * Derived from graphql-js 16.12.0, src/language, MIT.
 * Derived from graphql-js 16.12.0, src/error, MIT.
 * Derived from graphql-js 16.12.0, src/utilities/stripIgnoredCharacters.ts, MIT.
 */

//! The GraphQL language layer of Aosabi.
//!
//! The crate is a literal translation of the language directory of graphql-js
//! 16.12.0, the version the wire is pinned to, and of
//! `utilities/stripIgnoredCharacters.ts` beside it. Every function keeps the
//! graphql-js name in snake case and follows the TypeScript line by line,
//! because the exit number of the milestone is byte equality of error text and
//! locations.
//!
//! | Module | Translates | Holds |
//! | --- | --- | --- |
//! | [`source`] | `source.ts` | [`Source`], the body as text with its name and its location offset, and [`SourceBody`], the text or the WTF-8 of a body with a lone surrogate |
//! | [`location`] | `location.ts` | [`SourceLocation`] and [`get_location()`] |
//! | [`kinds`] | `kinds.ts`, `tokenKind.ts` | [`Kind`] and [`TokenKind`] |
//! | [`ast`] | `ast.ts` | [`Token`], the token arena, [`Location`] and every node |
//! | [`print_location`](mod@print_location) | `printLocation.ts` | [`print_location()`] and [`print_source_location()`] |
//! | [`error`] | `GraphQLError.ts`, `syntaxError.ts`, `locatedError.ts` | [`GraphQLError`], [`syntax_error()`] and [`located_error()`] |
//! | [`char_utils`] | `characterClasses.ts` | the character classes and the readings of `charCodeAt`, `codePointAt` and `slice` over the WTF-8 body |
//! | [`js_string`] | no analogue | [`JsStringBuilder`], [`js_string::join`], [`js_string::to_string_lossy`] and [`js_string::number_to_string`], the string operations behind the message of an error |
//! | [`block_string`] | `blockString.ts` | [`dedent_block_string_lines()`](block_string::dedent_block_string_lines), [`is_printable_as_block_string()`](block_string::is_printable_as_block_string) and [`print_block_string()`](block_string::print_block_string) |
//! | [`lexer`] | `lexer.ts` | [`Lexer`](lexer::Lexer), the token readers and the token descriptions |
//! | [`parser`] | `parser.ts` | [`parse()`], [`parse_value()`], [`parse_const_value()`], [`parse_type()`], [`ParseOptions`] and [`Parser`], and [`parse_value_within()`] with [`Nesting`] and [`ParseValueError`], a value under a bound on its nesting |
//! | [`predicates`] | `predicates.ts` | the kind predicates over a [`NodeRef`] |
//! | [`print_string`](mod@print_string) | `printString.ts` | [`print_string()`] |
//! | [`printer`] | `printer.ts` | [`print()`] |
//! | [`visitor`] | `visitor.ts`, `QueryDocumentKeys` of `ast.ts` | [`visit()`], [`Visitor`], [`NodeRef`] and the keys table |
//! | [`visitor_edit`](mod@visitor_edit) | `visitor.ts:179-302`, the edit path | [`visit_edit()`](visitor_edit::visit_edit), [`visit_edit_with_keys()`](visitor_edit::visit_edit_with_keys), [`EditVisitor`](visitor_edit::EditVisitor) and [`Edit`](visitor_edit::Edit), the walk that edits |
//! | [`strip_ignored_characters`](mod@strip_ignored_characters) | `utilities/stripIgnoredCharacters.ts` | [`strip_ignored_characters()`](strip_ignored_characters::strip_ignored_characters) |
//!
//! # Positions are UTF-16 code unit offsets
//!
//! A JavaScript string is a sequence of UTF-16 code units, and every index into
//! one counts code units. graphql-js indexes the body that way, so a position,
//! a column and a token boundary the crate reports are code unit indices, never
//! byte offsets and never code point offsets. The lexer walks the WTF-8 bytes
//! of the body in byte offsets and converts one into the code unit index
//! wherever a token or an error takes one, as the offset less a running count
//! of the bytes the body holds beyond its code units (see [`lexer`]), and
//! [`get_location()`] reads the code unit offsets of the line starts from a
//! table that one scan of the body builds on the first location request. An all
//! ASCII body counts none. An astral character therefore advances a column by
//! two, and a surrogate without its partner, which the text holds as three
//! WTF-8 bytes, is refused by the lexer at the position it stands at.
//!
//! The same rule reaches the text of an error. Three lexer errors quote a fixed
//! count of code units of the body, and that window ends on a leading surrogate
//! when it cuts an astral character. The message of a [`GraphQLError`] is
//! therefore a JavaScript string, built through [`js_string`], and the JSON
//! writer puts the lone code unit on the wire as the `\u` escape the reference
//! writes.
//!
//! The crate builds with no standard library and needs an allocator, because
//! every node owns its children. The `std` feature is the configuration symbol
//! of a host build and gates nothing today. The dev feature
//! `exec-walk-location` keeps `location::get_location_by_walk`, the walk of
//! the body from its start, as the second judge of the table of line starts,
//! and only `aosabi-devtool` names it, so no card ships the walk. There is no test in this crate,
//! which is the rule of CONTRIBUTING.md section 3, and the judging notebook is
//! `tests/notebooks/02_language.ipynb`.

#![no_std]

extern crate alloc;

use alloc::sync::Arc;

pub mod ast;
pub mod block_string;
pub mod char_utils;
pub mod error;
pub mod js_string;
pub mod kinds;
pub mod lexer;
pub mod location;
pub mod parser;
pub mod predicates;
pub mod print_location;
pub mod print_string;
pub mod printer;
pub mod source;
pub mod strip_ignored_characters;
pub mod visitor;
pub mod visitor_edit;

/// The path and the locations of an error of a host as graphql-js writes
/// them, a type of the builds of the TypeScript option alone.
#[cfg(feature = "host-errors")]
pub use crate::error::HostPlaces;

/// A slice the nodes of a tree share, so a clone of a node shares it by one increment.
///
/// The selections of a [`SelectionSetNode`](ast::SelectionSetNode) are such a
/// slice, so a clone of a field node copies its name, its alias, its
/// arguments, its directives and its location and shares the selections
/// below it, which the nodes of the reference share as objects. A parsed
/// document is handed out as an `Arc` and may cross threads, so the slice is
/// an `Arc` too. Every use names the alias and none names the `Arc` below
/// it, so a target without atomics can take another pointer here alone.
pub type SharedSlice<T> = Arc<[T]>;

/// Builds for a type every thread may send and share, and fails the build for any other.
#[cfg(target_has_atomic = "ptr")]
const fn assert_send_sync<T: Send + Sync>() {}

// A parsed document is handed out as an `Arc` that may cross threads, so no
// node of it may hold an `Rc` or a cell. The assertion fails the build of
// any change that puts one into the tree, a `SharedSlice` among them.
#[cfg(target_has_atomic = "ptr")]
const _: () = assert_send_sync::<DocumentNode>();

pub use crate::{
    ast::{
        AstNode,
        DocumentNode,
        Location,
        Token,
        TokenArena,
        TokenId,
        TokenValue, //
    },
    error::{
        GraphQLError,
        GraphQLErrorExtensions,
        GraphQLErrorOptions,
        GraphQLFormattedError,
        OriginalError,
        PathSegment,
        located_error,
        syntax_error, //
    },
    js_string::{
        JsStringBuilder, //
    },
    kinds::{
        Kind,
        TokenKind, //
    },
    location::{
        SourceLocation,
        get_location, //
    },
    parser::{
        Nesting,
        ParseOptions,
        ParseValueError,
        Parser,
        parse,
        parse_const_value,
        parse_type,
        parse_value,
        parse_value_within, //
    },
    print_location::{
        print_location,
        print_source_location, //
    },
    print_string::{
        print_string, //
    },
    printer::{
        print, //
    },
    source::{
        Source,
        SourceBody,
        SourceError, //
    },
    visitor::{
        NodeRef,
        VisitResult,
        Visitor,
        visit, //
    }, //
};
