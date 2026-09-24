// SPDX-License-Identifier: MIT
/*
 * aosabi-lang: the source of a document with every ignored token removed.
 *
 * Derived from graphql-js 16.12.0, src/utilities/stripIgnoredCharacters.ts, MIT.
 */

//! The removal of the ignored tokens of a source, the text a printed document
//! is folded to on the wire.
//!
//! The HTTP executor of `@graphql-tools/executor-http` sends the query of a
//! delegation as `stripIgnoredCharacters(print(document))`, so the bytes of
//! every outbound request pass through this function.
//!
//! The function walks the tokens of the lexer and copies the text of each from
//! the body. A token of the lexer keeps its start and its end as UTF-16 code
//! unit indices, which is how the reference slices the body, while the body
//! here is UTF-8 or WTF-8 (see [`crate::source`]). The copy therefore converts
//! both indices into byte offsets, reading the body forward once across the
//! whole walk, since the tokens come in source order. A body of ASCII converts
//! as the identity. Every token whose text the function copies is a name, a
//! number, a punctuator or a string, and the lexer refuses a string that holds
//! a surrogate without its partner, so the text of every copied token is UTF-8
//! and the result is a [`String`].

use alloc::string::String;

use crate::{
    block_string::print_block_string,
    char_utils::code_units_started_by,
    error::GraphQLError,
    kinds::TokenKind,
    lexer::{
        Lexer,
        is_punctuator_token_kind, //
    },
    source::{
        Source,
        widen, //
    }, //
};

/// Strips characters that are not significant to the validity or execution
/// of a GraphQL document.
///
/// The characters it strips are the Unicode byte order mark, white space,
/// line terminators, comments, commas and the indentation of a block string.
/// It is required to have a delimiter character between neighboring
/// non-punctuator tokens and this function always uses single space as
/// delimiter. It is guaranteed that both input and output documents if
/// parsed would result in the exact same AST except for nodes location.
/// It is guaranteed that this function will always produce stable results.
/// However, it's not guaranteed that it will stay the same between different
/// releases due to bugfixes or changes in the GraphQL specification.
///
/// `source` is the document text or a [`Source`], which is the `isSource`
/// branch of the reference.
///
/// Derived from graphql-js 16.12.0, src/utilities/stripIgnoredCharacters.ts, MIT.
///
/// # Examples
///
/// ```text
/// query SomeQuery($foo: String!, $bar: String) {
///   someField(foo: $foo, bar: $bar) {
///     a
///     b {
///       c
///       d
///     }
///   }
/// }
/// ```
///
/// becomes
///
/// ```text
/// query SomeQuery($foo:String!$bar:String){someField(foo:$foo bar:$bar){a b{c d}}}
/// ```
///
/// # Errors
///
/// Returns the syntax error of the first token that does not lex, as the
/// reference throws it, and the refusal of a body too long for the lexer.
// The error leaves the crate here as the type every caller outside it
// matches on, as the errors of `parse` do, so it is unboxed. The lint depends
// on the target and on the features, as it does on `parse`.
#[allow(clippy::result_large_err)]
pub fn strip_ignored_characters(source: impl Into<Source>) -> Result<String, GraphQLError> {
    let source: Source = source.into();
    let mut lexer = Lexer::new(source.into());
    let source_obj = lexer.source.clone();
    let body = source_obj.bytes();
    let mut units = CodeUnits::new(body);
    let mut stripped_body = String::new();

    let mut was_last_added_token_non_punctuator = false;
    loop {
        let current = lexer.advance().map_err(|error| *error)?;
        let current_token = &lexer.tokens[current];
        let token_kind = current_token.kind;
        if token_kind == TokenKind::Eof {
            break;
        }

        // Every two non-punctuator tokens should have space between them.
        // Also prevent case of non-punctuator token following by spread
        // resulting in invalid token (e.g. `1...` is invalid Float token).
        let is_non_punctuator = !is_punctuator_token_kind(token_kind);
        if was_last_added_token_non_punctuator
            && (is_non_punctuator || token_kind == TokenKind::Spread)
        {
            stripped_body.push(' ');
        }

        if token_kind == TokenKind::BlockString {
            let value = current_token.value_in(&source_obj).unwrap_or_default();
            stripped_body.push_str(&print_block_string(value, true));
        } else {
            let start = units.byte_offset(widen(current_token.start));
            let end = units.byte_offset(widen(current_token.end));
            stripped_body.push_str(source_obj.slice(start, end));
        }

        was_last_added_token_non_punctuator = is_non_punctuator;
    }

    Ok(stripped_body)
}

/// The conversion of the code unit indices of a body into byte offsets, read
/// forward.
///
/// Each conversion starts where the one before stopped, so the indices of a
/// walk in source order cost one pass over the body together.
struct CodeUnits<'b> {
    /// The UTF-8 or WTF-8 bytes of the body.
    body: &'b [u8],
    /// Whether the body is ASCII, where an index is its own byte offset.
    ascii: bool,
    /// The byte offset the reading stands at, on a character boundary.
    byte: usize,
    /// The code unit index of `byte`.
    unit: usize,
}

impl<'b> CodeUnits<'b> {
    /// Returns the reading of `body` from its start.
    fn new(body: &'b [u8]) -> Self {
        Self {
            body,
            ascii: body.is_ascii(),
            byte: 0,
            unit: 0,
        }
    }

    /// Returns the byte offset of the code unit index `unit`.
    ///
    /// `unit` is the start or the end of a token, which stands on the boundary
    /// of a character, and is not below the index of the conversion before.
    /// An index past the end of the body converts to its length.
    fn byte_offset(&mut self, unit: usize) -> usize {
        if self.ascii {
            return unit;
        }
        while self.unit < unit {
            let Some(&lead) = self.body.get(self.byte) else {
                break;
            };
            self.unit += code_units_started_by(lead);
            self.byte += 1;
            while self
                .body
                .get(self.byte)
                .is_some_and(|&byte| byte & 0xc0 == 0x80)
            {
                self.byte += 1;
            }
        }
        self.byte
    }
}
