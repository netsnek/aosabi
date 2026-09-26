// SPDX-License-Identifier: MIT
/*
 * aosabi-validate pass lang_feed: the feed of the single pass over the lexer
 * of the language layer.
 *
 * No analogue in the reference.
 */

//! The feed of the single pass over the lexer of the language layer.
//!
//! A server parses a request with the parser of the language layer, which
//! keeps the text it parsed in the location of the document, an `Arc<Source>`.
//! [`LangFeed::of`] lexes that text once more, sharing the source through the
//! `Arc` and copying no byte of it, so the pass reads the very text the tree
//! was parsed from. The lexer keeps every token in its arena, and a [`Tok`] is
//! the place of a token there.
//!
//! A build of the conventional validation lexes nothing for the pass. There
//! the name [`LangFeed`](super::LangFeed) of the module stands for [`NoFeed`]
//! ([`Carry`](super::Carry)), which holds no lexer and hands out no token.

use alloc::sync::Arc;

use aosabi_lang::{
    DocumentNode,
    Source,
    Token,
    TokenValue,
    kinds::TokenKind,
    lexer::Lexer, //
};

use super::feed::{
    Feed,
    Kind,
    Tok, //
};

/// The tokens of one text read by the lexer of the language layer.
#[derive(Debug, Clone)]
pub struct LangFeed {
    /// The lexer, whose arena holds every token read so far.
    lexer: Lexer,
}

impl LangFeed {
    /// Returns the feed over the text `source`.
    #[must_use]
    pub fn new(source: Arc<Source>) -> Self {
        Self {
            lexer: Lexer::new(source),
        }
    }

    /// Returns the feed over the text `document` was parsed from, or `None`
    /// for a document parsed without its location.
    #[must_use]
    pub fn of(document: &DocumentNode) -> Option<Self> {
        let location = document.loc.as_ref()?;
        Some(Self::new(Arc::clone(&location.source)))
    }

    /// Returns the token `tok` of the arena.
    #[inline]
    fn token(&self, tok: Tok) -> Option<&Token> {
        let index = usize::try_from(tok.0).ok()?;
        (&self.lexer.tokens).into_iter().as_slice().get(index)
    }
}

/// Returns the handle of the token with the place `index` in the arena.
#[inline]
fn handle(index: usize) -> Option<Tok> {
    u32::try_from(index).ok().map(Tok)
}

impl Feed for LangFeed {
    fn start(&self) -> Tok {
        handle(self.lexer.token.index()).unwrap_or(Tok(u32::MAX))
    }

    fn advance(&mut self) -> Option<Tok> {
        let token = self.lexer.advance().ok()?;
        handle(token.index())
    }

    #[inline]
    fn kind(&self, tok: Tok) -> Option<Kind> {
        Some(match self.token(tok)?.kind {
            TokenKind::Sof => Kind::Sof,
            TokenKind::Eof => Kind::Eof,
            TokenKind::Bang => Kind::Bang,
            TokenKind::Dollar => Kind::Dollar,
            TokenKind::ParenL => Kind::ParenL,
            TokenKind::ParenR => Kind::ParenR,
            TokenKind::Spread => Kind::Spread,
            TokenKind::Colon => Kind::Colon,
            TokenKind::Equals => Kind::Equals,
            TokenKind::At => Kind::At,
            TokenKind::BracketL => Kind::BracketL,
            TokenKind::BracketR => Kind::BracketR,
            TokenKind::BraceL => Kind::BraceL,
            TokenKind::BraceR => Kind::BraceR,
            TokenKind::Name => Kind::Name,
            TokenKind::Int => Kind::Int,
            TokenKind::Float => Kind::Float,
            TokenKind::String => Kind::String,
            TokenKind::BlockString => Kind::BlockString,
            TokenKind::Amp | TokenKind::Dot | TokenKind::Pipe | TokenKind::Comment => Kind::Other,
        })
    }

    #[inline]
    fn text(&self, tok: Tok) -> Option<&[u8]> {
        match &self.token(tok)?.value {
            TokenValue::None => Some(&[]),
            TokenValue::Span { start, end } => {
                let start = usize::try_from(*start).ok()?;
                let end = usize::try_from(*end).ok()?;
                self.lexer.source.bytes().get(start..end)
            }
            TokenValue::Owned(value) => Some(value.as_bytes()),
        }
    }
}

/// The feed of a build of the conventional validation, which reads no text
/// and hands out no token.
///
/// A build whose setting `AOSABI_VALIDATION` is `conventional` carries no
/// single pass ([`CARRIED`](super::CARRIED)), and the name
/// [`LangFeed`](super::LangFeed) of the module stands for this type there
/// ([`Carry`](super::Carry)). Its functions are those of [`LangFeed`] that a
/// caller names, so every caller compiles in both builds, while it holds no
/// lexer, so nothing of the lexer is linked for the pass.
#[derive(Debug, Clone)]
pub struct NoFeed;

impl NoFeed {
    /// Returns the feed over the text `source`, which it drops unread.
    // Always inlined, so a caller that builds a feed itself, as the RV32 probe
    // of tests/notebooks/09_single_pass_budget.ipynb does, keeps the drop of
    // the source in its own code and links no function of this module, where
    // a build at opt-level z leaves a function of a few instructions out of
    // line.
    #[expect(
        clippy::inline_always,
        reason = "a conventional build at opt-level z keeps no function of the module for a feed"
    )]
    #[inline(always)]
    #[must_use]
    pub fn new(source: Arc<Source>) -> Self {
        drop(source);
        Self
    }

    /// Returns `None` for every document, as [`LangFeed::of`] answers for a
    /// document parsed without its location.
    #[must_use]
    pub fn of(_document: &DocumentNode) -> Option<Self> {
        None
    }
}

impl Feed for NoFeed {
    /// Returns a handle the feed never gave, since it reads no text.
    fn start(&self) -> Tok {
        Tok(u32::MAX)
    }

    fn advance(&mut self) -> Option<Tok> {
        None
    }

    fn kind(&self, _tok: Tok) -> Option<Kind> {
        None
    }

    fn text(&self, _tok: Tok) -> Option<&[u8]> {
        None
    }
}
