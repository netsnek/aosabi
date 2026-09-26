// SPDX-License-Identifier: MIT
/*
 * aosabi-validate pass feed: the stream of tokens the single pass pulls a
 * document from.
 *
 * No analogue in the reference.
 */

//! The feed of the single pass.
//!
//! The walk reads a document as a stream of tokens through [`Feed`], so the
//! one walk serves every source of tokens: the lexer of the language layer
//! over the text a parsed document keeps, which is
//! [`LangFeed`](super::LangFeed), and a lexer that reads tokens in place on a
//! device. A token is named by a [`Tok`], which the feed hands out and reads
//! back, and the walk keeps those handles in its plan, so a feed keeps every
//! token it handed out readable until the next document.

/// A token of a feed, by the handle the feed gave it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tok(pub u32);

/// The kind of a token, as far as an executable document holds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `<SOF>`, the start of the text.
    Sof,
    /// `<EOF>`, the end of the text.
    Eof,
    /// `!`.
    Bang,
    /// `$`.
    Dollar,
    /// `(`.
    ParenL,
    /// `)`.
    ParenR,
    /// `...`.
    Spread,
    /// `:`.
    Colon,
    /// `=`.
    Equals,
    /// `@`.
    At,
    /// `[`.
    BracketL,
    /// `]`.
    BracketR,
    /// `{`.
    BraceL,
    /// `}`.
    BraceR,
    /// A name.
    Name,
    /// An integer literal.
    Int,
    /// A float literal.
    Float,
    /// A string literal.
    String,
    /// A block string literal.
    BlockString,
    /// A token no executable document holds, such as `&` or `|`.
    Other,
}

/// A stream of the tokens of one document.
pub trait Feed {
    /// Returns the start of file token, which the feed stands at before its
    /// first [`Feed::advance`].
    fn start(&self) -> Tok;

    /// Moves to the next token that is not a comment and returns it, or
    /// `None` when the text does not lex there.
    fn advance(&mut self) -> Option<Tok>;

    /// Returns the kind of `tok`, or `None` for a handle the feed never gave.
    fn kind(&self, tok: Tok) -> Option<Kind>;

    /// Returns the value of `tok`: the text of a name or a number as written,
    /// the value of a string with its escapes resolved and of a block string
    /// with its lines dedented, and nothing for a punctuator, or `None` for a
    /// handle the feed never gave.
    fn text(&self, tok: Tok) -> Option<&[u8]>;
}
