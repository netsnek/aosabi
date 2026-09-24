// SPDX-License-Identifier: MIT
/*
 * aosabi-lang: the lexer that turns a source into the token stream the
 * parser walks.
 *
 * Derived from graphql-js 16.12.0, src/language/lexer.ts, MIT.
 * Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
 */

//! The lexer of the language layer.
//!
//! A [`Lexer`] is a stateful stream generator over one [`Source`]. Every
//! [`Lexer::advance`] returns the next token that is not a comment, and once the
//! source lexes to its end the lexer returns the same end of file token for
//! every further call. The token readers below follow the reference function by
//! function and keep its names, and every syntax error message is the vendored
//! text, because the exit number of the milestone is byte equality of error text
//! and locations.
//!
//! Three shapes of the reference read differently in Rust.
//!
//! - The reference links each token to its neighbours through object references.
//! - A `throw syntaxError(...)` is an `Err` here, so every reader returns a
//!   `Result` and the parser propagates it. The error is a boxed
//!   [`GraphQLError`], so the error variant of a `Result` is one pointer
//!   wide, and the box is allocated once, on the path that aborts the parse.
//! - The reference indexes the body as a JavaScript string, whose index is a
//!   UTF-16 code unit index. The readers walk the WTF-8 bytes of
//!   [`Source::bytes`] instead, so a position inside the lexer is a byte offset,
//!   a character outside ASCII is stepped over by its width in bytes, and a read
//!   past the end yields the NaN reading of [`crate::char_utils`].
//!   [`create_token`] and every syntax error convert the byte offset into the
//!   code unit index the reference reports, so the start, the end and the column
//!   of a token and the position of an error are code unit indices. The index is
//!   the byte offset less the bytes the body holds beyond its code units before
//!   it, a count the lexer keeps as it reads (see [`Lexer`]), so an all ASCII
//!   body counts none and converts as the identity.
//!
//! The arena of a lexer reserves room for the tokens of its body from the length
//! of the body, before the first token. A token keeps its positions, its line
//! and its column as `u32`, and before it reads the first token the lexer
//! refuses a body too long for them to fit (see [`crate::source`]). The
//! reference has no such limit, and the refusal takes the form of the error of
//! the token budget of the parser.
//!
//! The `LexerInterface` of the reference exists so that a second lexer can
//! share the exported readers. The readers take the concrete [`Lexer`] here,
//! and a second lexer that shares them introduces the trait.

use alloc::{
    borrow::Cow,
    boxed::Box,
    format,
    string::String,
    sync::Arc,
    vec::Vec, //
};

use aosabi_json::JsString;

use crate::{
    ast::{
        Token,
        TokenArena,
        TokenId,
        TokenValue, //
    },
    block_string::dedent_block_string_lines,
    char_utils::{
        SURROGATE_PAIR_WIDTH,
        ascii_code_at,
        bytes_beyond_units,
        char_code_at,
        code_point_at,
        from_code_point,
        is_digit,
        is_leading_surrogate,
        is_name_continue,
        is_name_start,
        is_trailing_surrogate,
        is_unicode_scalar_value,
        slice_code_units,
        utf8_width,
        utf16_surrogate_pair_to_code_point, //
    },
    error::{
        GraphQLError,
        syntax_error, //
    },
    js_string::JsStringBuilder,
    kinds::TokenKind,
    source::{
        MAX_BODY_LENGTH,
        Source,
        narrow,
        widen, //
    }, //
};

/// The bytes of a body the arena of the lexer reserves one token for.
///
/// The divisor is that ratio rounded down, so the reserve errs toward a token
/// more, and the notebook asserts it. A document denser than the ratio grows the
/// arena once it is full, and a sparser one keeps room it never fills.
///
/// The reserve is made before the first token, whatever the body holds, so it
/// costs the room of a token for every 7 bytes of the body, about 9.14 bytes per
/// byte on a 64-bit host and 6.86 on `wasm32` and `riscv32`, where a token takes
/// 64 and 48 bytes. A sparse body, one long string, block string or comment, or
/// a body that fails at its first byte, pays all of it. A token budget of the
/// parser bounds it.
const BYTES_PER_TOKEN: usize = 7;

/// Returns the count of tokens the arena of a body of `length` bytes reserves.
///
/// That is a token for every [`BYTES_PER_TOKEN`] bytes, with `<SOF>` and
/// `<EOF>` beside them. A body longer than [`MAX_BODY_LENGTH`] reserves
/// `<SOF>` alone, since the lexer refuses it before the first token.
pub(crate) const fn token_capacity(length: usize) -> usize {
    if length > MAX_BODY_LENGTH {
        1
    } else {
        length / BYTES_PER_TOKEN + 2
    }
}

/// Given a Source object, creates a Lexer for that source.
///
/// A Lexer is a stateful stream generator in that every time it is advanced, it
/// returns the next token in the Source. Assuming the source lexes, the final
/// Token emitted by the lexer will be of kind EOF, after which the lexer will
/// repeatedly return the same EOF token whenever called. The
/// `Symbol.toStringTag` getter names the class for `Object.prototype.toString`
/// and has no reading here.
///
/// Derived from graphql-js 16.12.0, src/language/lexer.ts, MIT.
#[derive(Debug, Clone)]
pub struct Lexer {
    /// The source being lexed.
    pub source: Arc<Source>,
    /// Every token read so far, including comments, in reading order.
    ///
    /// This is the double linked list of the reference held in one place,
    /// and `<SOF>` is always its first token.
    pub tokens: TokenArena,
    /// The previously focused non-ignored token.
    pub last_token: TokenId,
    /// The currently focused non-ignored token.
    pub token: TokenId,
    /// The (1-indexed) line containing the current token.
    pub line: u32,
    /// The character offset at which the current line begins.
    pub line_start: u32,
    /// The byte offset the count `excess` stands at.
    ///
    /// Between two reads it is the end of the last token read, where the
    /// next read starts. A read moves it over the ignored characters to the
    /// start of the token it reads, and every conversion of that read counts
    /// from there. A read that fails puts it back, with the count.
    byte_offset: usize,
    /// The bytes the body holds beyond its UTF-16 code units before
    /// `byte_offset`.
    ///
    /// A byte offset at `byte_offset` converts into a code unit index by
    /// subtracting this count, and a later offset by subtracting as well
    /// what the bytes between hold (see [`bytes_beyond_units`]). A read adds two
    /// for every byte order mark among the ignored characters, and a token
    /// adds what its characters outside ASCII hold, which only a comment, a
    /// string and a block string carry.
    excess: usize,
}

impl Lexer {
    /// Creates a lexer focused on the start of file token of `source`.
    ///
    /// The arena reserves room for the tokens of the body from its length, one
    /// for every `BYTES_PER_TOKEN` bytes with `<SOF>` and `<EOF>` beside them.
    ///
    /// Derived from graphql-js 16.12.0, src/language/lexer.ts, MIT.
    #[must_use]
    pub fn new(source: Arc<Source>) -> Self {
        let capacity = token_capacity(source.bytes().len());
        Self::with_capacity(source, capacity)
    }

    /// Creates a lexer focused on the start of file token of `source`, with
    /// room for `capacity` tokens in its arena.
    ///
    /// The parser passes the capacity of [`token_capacity`] bounded by its
    /// token budget.
    pub(crate) fn with_capacity(source: Arc<Source>, capacity: usize) -> Self {
        let mut tokens = TokenArena::with_capacity(capacity);
        let start_of_file_token =
            tokens.push(Token::new(TokenKind::Sof, 0, 0, 0, 0, TokenValue::None));

        Self {
            source,
            tokens,
            last_token: start_of_file_token,
            token: start_of_file_token,
            line: 1,
            line_start: 0,
            byte_offset: 0,
            excess: 0,
        }
    }

    /// Returns the UTF-16 code unit index of the byte `offset` of the body.
    ///
    /// An offset at or after the offset the count of the lexer stands at,
    /// which every offset of the current read is, converts from the count
    /// and the bytes between. An earlier offset, which only a caller of the
    /// public [`create_token`] or [`read_name`] outside a read passes,
    /// converts from the start of the body. An offset past the end converts
    /// as if the body went on in ASCII.
    // The walk serves the end of a comment, a string and a block string and the
    // position of an error. Out of line it leaves every other token the one
    // subtraction of `create_token`, and inlined there it costs that path
    // instructions.
    #[inline(never)]
    fn utf16_offset(&self, offset: usize) -> usize {
        let body = self.source.bytes();
        let end = offset.min(body.len());
        let (from, excess) = if self.byte_offset <= end {
            (self.byte_offset, self.excess)
        } else {
            (0, 0)
        };
        let between = body.get(from..end).unwrap_or_default();
        offset - excess - bytes_beyond_units(between)
    }

    /// Advances the token stream to the next non-ignored token.
    ///
    /// Derived from graphql-js 16.12.0, src/language/lexer.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the first token that does not lex, or the
    /// refusal of a body longer than `MAX_BODY_LENGTH` bytes at position 0.
    pub fn advance(&mut self) -> Result<TokenId, Box<GraphQLError>> {
        self.last_token = self.token;
        let token = self.lookahead()?;
        self.token = token;
        Ok(token)
    }

    /// Looks ahead and returns the next non-ignored token, but does not change
    /// the focused token of the lexer.
    ///
    /// Reading ahead appends tokens to the arena and moves the line counters,
    /// which is why the method takes the lexer mutably.
    ///
    /// Derived from graphql-js 16.12.0, src/language/lexer.ts, MIT.
    ///
    /// The read after `<SOF>` first checks the length of the body. Every
    /// entry point of the parser reads that token first, and so does any
    /// other caller of a lexer.
    ///
    /// A read starts at the end of the last token the lexer read, the byte
    /// offset its count stands at, where the reference starts at the end of
    /// the token it links to. Every token of the chain the lexer builds links
    /// to the next one but the last it read, so the two are the same token
    /// for every walk of that chain. A read that fails leaves the count where
    /// it stood, so a read tried again passes the same ignored characters
    /// and moves the line counters again, as the reference does.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the first token that does not lex, or the
    /// refusal of a body longer than `MAX_BODY_LENGTH` bytes at position 0.
    pub fn lookahead(&mut self) -> Result<TokenId, Box<GraphQLError>> {
        let mut token = self.token;
        if self.tokens[token].kind != TokenKind::Eof {
            loop {
                if let Some(next) = self.tokens[token].next {
                    token = next;
                } else {
                    if self.tokens[token].kind == TokenKind::Sof {
                        refuse_long_body(self)?;
                    }
                    // Read the next token and form a link in the token linked-list.
                    let (start, excess) = (self.byte_offset, self.excess);
                    let (next_token, end) = match read_next_token(self, start) {
                        Ok(lexed) => lexed,
                        Err(error) => {
                            // The read moved the count to the start of the
                            // token that failed. Putting it back makes a read
                            // tried again pass the ignored characters again,
                            // as the reference does.
                            self.byte_offset = start;
                            self.excess = excess;
                            return Err(error);
                        }
                    };
                    self.excess = end - widen(next_token.end);
                    self.byte_offset = end;
                    let next = self.tokens.push(next_token);
                    self.tokens[token].next = Some(next);
                    self.tokens[next].prev = Some(token);
                    token = next;
                }
                if self.tokens[token].kind != TokenKind::Comment {
                    break;
                }
            }
        }
        Ok(token)
    }
}

/// Returns the refusal of a body longer than [`MAX_BODY_LENGTH`] bytes.
///
/// A token keeps its positions, its line and its column as `u32`, and a longer
/// body could lex a token whose numbers do not fit. The reference has no such
/// limit, so the message is Aosabi's alone. It is a syntax error, as the error
/// of the token budget is, so a caller that handles that error handles this one.
/// It stands at position 0, before any token.
fn refuse_long_body(lexer: &Lexer) -> Result<(), Box<GraphQLError>> {
    if lexer.source.bytes().len() > MAX_BODY_LENGTH {
        return Err(Box::new(syntax_error(
            &lexer.source,
            0,
            format!("Document contains more than {MAX_BODY_LENGTH} bytes. Parsing aborted."),
        )));
    }
    Ok(())
}

/// Returns whether `kind` is one of the punctuator token kinds.
///
/// Derived from graphql-js 16.12.0, src/language/lexer.ts, MIT.
#[must_use]
pub const fn is_punctuator_token_kind(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Bang
            | TokenKind::Dollar
            | TokenKind::Amp
            | TokenKind::ParenL
            | TokenKind::ParenR
            | TokenKind::Dot
            | TokenKind::Spread
            | TokenKind::Colon
            | TokenKind::Equals
            | TokenKind::At
            | TokenKind::BracketL
            | TokenKind::BracketR
            | TokenKind::BraceL
            | TokenKind::Pipe
            | TokenKind::BraceR
    )
}

/// Returns whether a surrogate pair starts at the byte `location` in `body`.
///
/// The GraphQL specification defines source text as a sequence of unicode scalar
/// values (which Unicode defines to exclude surrogate code points). However
/// JavaScript defines strings as a sequence of UTF-16 code units which may
/// include surrogates. A surrogate pair is a valid source character as it
/// encodes a supplementary code point (above U+FFFF), but unpaired surrogate
/// code points are not valid source characters.
///
/// Derived from graphql-js 16.12.0, src/language/lexer.ts, MIT.
///
/// The body writes a pair as the four bytes of the code point it encodes and
/// a lone surrogate as three bytes of its own, so a pair starts at
/// `location` exactly when the character there is above U+FFFF.
fn is_supplementary_code_point(body: &[u8], location: usize) -> bool {
    code_point_at(body, location).is_some_and(|code| code > 0xffff)
}

/// Prints the code point (or end of file reference) at a given location in a
/// source for use in error messages.
///
/// Printable ASCII is printed quoted, while other points are printed in Unicode
/// code point form (ie. U+1234). `lexer` holds the source and `location` is the
/// byte offset of a character of its body.
///
/// Derived from graphql-js 16.12.0, src/language/lexer.ts, MIT.
#[must_use]
pub fn print_code_point_at(lexer: &Lexer, location: usize) -> String {
    let Some(code) = code_point_at(lexer.source.bytes(), location) else {
        return String::from(TokenKind::Eof.as_str());
    };

    if (0x0020..=0x007e).contains(&code) {
        // Printable ASCII
        let char = from_code_point(code);
        return if char == '"' {
            String::from("'\"'")
        } else {
            format!("\"{char}\"")
        };
    }

    // Unicode code point. `toString(16)` writes the integer in hexadecimal
    // digits without a leading zero, `toUpperCase` raises them and
    // `padStart(4, '0')` fills to four digits, which is the width of the format
    // below.
    format!("U+{code:04X}")
}

/// Create a token with line and column location information.
///
/// `lexer` supplies the line, the line start and the count of the bytes its body
/// holds beyond its code units, `kind` and `value` are the fields of the token,
/// and `start` and `end` are the byte offsets of its ends in the body. The token
/// holds them as UTF-16 code unit indices and the column as one more than the
/// distance of the start from the line start, in code units. Each is a `u32`,
/// which holds it for every body the lexer reads, and an offset past `u32::MAX`
/// holds `u32::MAX`. The token is not yet in the arena, [`Lexer::lookahead`]
/// appends and links it.
///
/// Derived from graphql-js 16.12.0, src/language/lexer.ts, MIT.
///
/// Every reader creates its token at the offset the count of `lexer` stands
/// at, the start of the token being read, whose index is the offset less the
/// count. Only a comment, a string and a block string hold characters outside
/// ASCII, so the end of those three converts by a walk over the bytes of the
/// token, and the end of any other kind lies as many code units after its
/// start as it takes bytes, which the grammar makes ASCII, for any caller. A
/// caller outside the readers may pass any start, which converts from the
/// count and the bytes between at or after the offset the count stands at,
/// the end of the last token read between two reads, and by a walk from the
/// start of the body before it.
///
/// The reference passes the value as a string. Here `value` is a
/// [`TokenValue`], a span with byte offsets that stay byte offsets for a
/// name, a number, a comment and a string without an escape, and an owned
/// value for a string with an escape and for every block string.
///
/// # Panics
///
/// In a build with overflow checks, panics when the column underflows, for
/// a `start` more than one code unit before the line start of `lexer`, and,
/// for a kind other than a comment, a string and a block string, when `end`
/// is smaller than the count of bytes the body holds beyond its code units
/// before `start`. The readers pass neither. Without overflow checks the
/// subtraction wraps and the token holds `u32::MAX` in that position.
#[must_use]
pub fn create_token(
    lexer: &Lexer,
    kind: TokenKind,
    start: usize,
    end: usize,
    value: TokenValue,
) -> Token {
    let unit_start = if start == lexer.byte_offset {
        start - lexer.excess
    } else {
        lexer.utf16_offset(start)
    };
    let unit_end = match kind {
        TokenKind::Comment | TokenKind::String | TokenKind::BlockString => lexer.utf16_offset(end),
        _ => end - (start - unit_start),
    };
    let line = lexer.line;
    let col = 1 + unit_start - widen(lexer.line_start);
    Token::new(
        kind,
        narrow(unit_start),
        narrow(unit_end),
        line,
        narrow(col),
        value,
    )
}

/// Returns the value of a token that is the text of the body between the
/// byte offsets `start` and `end`.
///
/// The offsets narrow to `u32` as the positions of the token do.
fn span(start: usize, end: usize) -> TokenValue {
    TokenValue::Span {
        start: narrow(start),
        end: narrow(end),
    }
}

/// Returns the syntax error `description` at the byte `position` of the body.
///
/// This is `syntaxError(lexer.source, position, description)` of every
/// reader, with the byte offset converted into the code unit index the
/// reference reports. The error is boxed here, where it is made, so the
/// readers and the parser pass it on as one pointer.
// The lint depends on the pointer width: `GraphQLError` takes at most the 128
// bytes of its threshold on every 32-bit target, exactly 128 on wasm32 with the
// feature std of aosabi-json, as in a Worker, and more on a 64-bit target, so
// the lint fires on the 32-bit targets alone. The box stays on a 32-bit target
// as well, since it keeps the error side of every `Result` of the lexer and the
// parser one pointer wide, and the `wasm32` build of the release seam is smaller
// with it.
#[cfg_attr(
    not(target_pointer_width = "64"),
    expect(clippy::unnecessary_box_returns)
)]
fn lexer_syntax_error(
    lexer: &Lexer,
    position: usize,
    description: impl Into<JsString>,
) -> Box<GraphQLError> {
    Box::new(syntax_error(
        &lexer.source,
        lexer.utf16_offset(position),
        description,
    ))
}

/// A token a reader lexed and the byte offset of the body after it, where
/// the next read starts.
type Lexed = (Token, usize);

/// Returns the one code unit punctuator token of `kind` at `position` and
/// the byte offset after it.
///
/// This is the `createToken(lexer, kind, position, position + 1)` line the
/// reference writes once per punctuator, so that every arm of the switch in
/// [`read_next_token`] stays one line beside its reference line. Every
/// punctuator is one ASCII byte.
fn punctuator_token(lexer: &Lexer, kind: TokenKind, position: usize) -> Lexed {
    let end = position + 1;
    (
        create_token(lexer, kind, position, end, TokenValue::None),
        end,
    )
}

/// The bytes of a word of the target, eight on a 64-bit host and four on
/// `wasm32` and `riscv32`.
const WORD: usize = size_of::<usize>();

/// A word whose every byte is a space.
const SPACES: usize = usize::from_le_bytes([0x20; WORD]);

/// Returns the byte offset of the first byte from `position` on that is not a
/// space, or the length of the body where the run reaches its end.
///
/// The run is read a word at a time, [`WORD`] bytes as one `usize`. The
/// difference of a word from [`SPACES`] is zero in every byte that is a
/// space, and `usize::from_le_bytes` puts the first byte lowest on every
/// target, so its trailing zero bits divided by eight count the spaces
/// before the first byte that is not one. The bytes after the last whole
/// word of the body are read one at a time.
// The arm of `read_next_token` calls this where a space follows a blank. Inlined
// into that loop the word costs the path of every token instructions, and out of
// line it costs a call per run.
#[inline(never)]
fn skip_spaces(body: &[u8], mut position: usize) -> usize {
    while let Some(word) = body.get(position..).and_then(<[u8]>::first_chunk::<WORD>) {
        let differ = usize::from_le_bytes(*word) ^ SPACES;
        if differ != 0 {
            return position + widen(differ.trailing_zeros() / 8);
        }
        position += WORD;
    }
    while body.get(position) == Some(&0x20) {
        position += 1;
    }
    position
}

/// Gets the next token from the source starting at the given position.
///
/// This skips over whitespace until it finds the next lexable token, then lexes
/// punctuators immediately or calls the appropriate helper function for more
/// complicated tokens.
///
/// Derived from graphql-js 16.12.0, src/language/lexer.ts, MIT.
///
/// The token comes back with the byte offset after it. The count of the
/// lexer stands at `start` on entry, and the walk moves it on over the
/// ignored characters to the start of the token, which every conversion of
/// the token and of an error of its read counts from.
// The reference reads every token start in one switch, and the port keeps
// its arms whole and in their order, split once where the ignored characters
// end, so that a reader can lay the two side by side, which is the rule of
// the literal port.
#[allow(clippy::too_many_lines)]
fn read_next_token(lexer: &mut Lexer, start: usize) -> Result<Lexed, Box<GraphQLError>> {
    // The walk borrows the field `source` alone, so the line counters and the
    // count, other fields of the same lexer, move while it holds the body.
    let body = lexer.source.bytes();
    let body_length = body.len();
    let mut position = start;
    let mut excess = lexer.excess;

    while position < body_length {
        let code = char_code_at(body, position);

        // SourceCharacter
        match code {
            // Ignored ::
            //   - UnicodeBOM
            //   - WhiteSpace
            //   - LineTerminator
            //   - Comment
            //   - Comma
            //
            // UnicodeBOM :: "Byte Order Mark (U+FEFF)"
            0xfeff => {
                // Three bytes and one code unit.
                position += 3;
                excess += 2;
                continue;
            }
            // WhiteSpace ::
            //   - "Horizontal Tab (U+0009)"
            //   - "Space (U+0020)"
            //
            // Comma :: ,
            0x0009 | 0x0020 | 0x002c => {
                position += 1;
                // A space that follows starts a run, such as an indentation,
                // which goes a word at a time.
                if ascii_code_at(body, position) == 0x0020 {
                    position = skip_spaces(body, position + 1);
                }
                continue;
            }
            // LineTerminator ::
            //   - "New Line (U+000A)"
            //   - "Carriage Return (U+000D)" [lookahead != "New Line (U+000A)"]
            //   - "Carriage Return (U+000D)" "New Line (U+000A)"
            0x000a => {
                position += 1;
                lexer.line += 1;
                lexer.line_start = narrow(position - excess);
                continue;
            }
            0x000d => {
                if ascii_code_at(body, position + 1) == 0x000a {
                    position += 2;
                } else {
                    position += 1;
                }
                lexer.line += 1;
                lexer.line_start = narrow(position - excess);
                continue;
            }
            _ => {}
        }

        // A token or an error starts here, and every conversion of the rest
        // of the read counts from this start.
        lexer.byte_offset = position;
        lexer.excess = excess;

        match code {
            // Comment
            0x0023 => return Ok(read_comment(lexer, position)),
            // Token ::
            //   - Punctuator
            //   - Name
            //   - IntValue
            //   - FloatValue
            //   - StringValue
            //
            // Punctuator :: one of ! $ & ( ) ... : = @ [ ] { | }
            0x0021 => return Ok(punctuator_token(lexer, TokenKind::Bang, position)),
            0x0024 => return Ok(punctuator_token(lexer, TokenKind::Dollar, position)),
            0x0026 => return Ok(punctuator_token(lexer, TokenKind::Amp, position)),
            0x0028 => return Ok(punctuator_token(lexer, TokenKind::ParenL, position)),
            0x0029 => return Ok(punctuator_token(lexer, TokenKind::ParenR, position)),
            0x002e
                if ascii_code_at(body, position + 1) == 0x002e
                    && ascii_code_at(body, position + 2) == 0x002e =>
            {
                let end = position + 3;
                return Ok((
                    create_token(lexer, TokenKind::Spread, position, end, TokenValue::None),
                    end,
                ));
            }
            0x003a => return Ok(punctuator_token(lexer, TokenKind::Colon, position)),
            0x003d => return Ok(punctuator_token(lexer, TokenKind::Equals, position)),
            0x0040 => return Ok(punctuator_token(lexer, TokenKind::At, position)),
            0x005b => return Ok(punctuator_token(lexer, TokenKind::BracketL, position)),
            0x005d => return Ok(punctuator_token(lexer, TokenKind::BracketR, position)),
            0x007b => return Ok(punctuator_token(lexer, TokenKind::BraceL, position)),
            0x007c => return Ok(punctuator_token(lexer, TokenKind::Pipe, position)),
            0x007d => return Ok(punctuator_token(lexer, TokenKind::BraceR, position)),
            // StringValue
            0x0022 => {
                if ascii_code_at(body, position + 1) == 0x0022
                    && ascii_code_at(body, position + 2) == 0x0022
                {
                    return read_block_string(lexer, position);
                }
                return read_string(lexer, position);
            }
            _ => {}
        }

        // IntValue | FloatValue (Digit | -)
        if is_digit(code) || code == 0x002d {
            return read_number(lexer, position, code);
        }

        // Name
        if is_name_start(code) {
            return Ok(read_name_to(lexer, position));
        }

        let description = if code == 0x0027 {
            String::from(
                "Unexpected single quote character ('), did you mean to use a double quote (\")?",
            )
        } else if is_unicode_scalar_value(code) || is_supplementary_code_point(body, position) {
            format!(
                "Unexpected character: {}.",
                print_code_point_at(lexer, position)
            )
        } else {
            format!(
                "Invalid character: {}.",
                print_code_point_at(lexer, position)
            )
        };
        return Err(lexer_syntax_error(lexer, position, description));
    }

    lexer.byte_offset = body_length;
    lexer.excess = excess;
    Ok((
        create_token(
            lexer,
            TokenKind::Eof,
            body_length,
            body_length,
            TokenValue::None,
        ),
        body_length,
    ))
}

/// Reads a comment token from the source file.
///
/// ```text
/// Comment :: # CommentChar* [lookahead != CommentChar]
///
/// CommentChar :: SourceCharacter but not LineTerminator
/// ```
///
/// Derived from graphql-js 16.12.0, src/language/lexer.ts, MIT.
// Out of line, so a document with no comment compiles no reader of one.
#[inline(never)]
fn read_comment(lexer: &Lexer, start: usize) -> Lexed {
    let body = lexer.source.bytes();
    let body_length = body.len();
    let mut position = start + 1;

    while position < body_length {
        let code = char_code_at(body, position);

        // LineTerminator (\n | \r)
        if code == 0x000a || code == 0x000d {
            break;
        }

        // SourceCharacter
        if is_unicode_scalar_value(code) {
            position += utf8_width(code);
        } else if is_supplementary_code_point(body, position) {
            position += SURROGATE_PAIR_WIDTH;
        } else {
            break;
        }
    }

    (
        create_token(
            lexer,
            TokenKind::Comment,
            start,
            position,
            span(start + 1, position),
        ),
        position,
    )
}

/// Reads a number token from the source file, either a `FloatValue` or an
/// `IntValue` depending on whether a `FractionalPart` or `ExponentPart` is
/// encountered.
///
/// ```text
/// IntValue :: IntegerPart [lookahead != {Digit, `.`, NameStart}]
///
/// IntegerPart ::
///   - NegativeSign? 0
///   - NegativeSign? NonZeroDigit Digit*
///
/// NegativeSign :: -
///
/// NonZeroDigit :: Digit but not `0`
///
/// FloatValue ::
///   - IntegerPart FractionalPart ExponentPart [lookahead != {Digit, `.`, NameStart}]
///   - IntegerPart FractionalPart [lookahead != {Digit, `.`, NameStart}]
///   - IntegerPart ExponentPart [lookahead != {Digit, `.`, NameStart}]
///
/// FractionalPart :: . Digit+
///
/// ExponentPart :: ExponentIndicator Sign? Digit+
///
/// ExponentIndicator :: one of `e` `E`
///
/// Sign :: one of + -
/// ```
///
/// Derived from graphql-js 16.12.0, src/language/lexer.ts, MIT.
// Out of line, so a document with no number compiles no reader of one.
#[inline(never)]
fn read_number(lexer: &Lexer, start: usize, first_code: i32) -> Result<Lexed, Box<GraphQLError>> {
    let body = lexer.source.bytes();
    let mut position = start;
    let mut code = first_code;
    let mut is_float = false;

    // NegativeSign (-)
    if code == 0x002d {
        position += 1;
        code = ascii_code_at(body, position);
    }

    // Zero (0)
    if code == 0x0030 {
        position += 1;
        code = ascii_code_at(body, position);
        if is_digit(code) {
            return Err(lexer_syntax_error(
                lexer,
                position,
                format!(
                    "Invalid number, unexpected digit after 0: {}.",
                    print_code_point_at(lexer, position)
                ),
            ));
        }
    } else {
        position = read_digits(lexer, position, code)?;
        code = ascii_code_at(body, position);
    }

    // Full stop (.)
    if code == 0x002e {
        is_float = true;

        position += 1;
        code = ascii_code_at(body, position);
        position = read_digits(lexer, position, code)?;
        code = ascii_code_at(body, position);
    }

    // E e
    if code == 0x0045 || code == 0x0065 {
        is_float = true;

        position += 1;
        code = ascii_code_at(body, position);
        // + -
        if code == 0x002b || code == 0x002d {
            position += 1;
            code = ascii_code_at(body, position);
        }
        position = read_digits(lexer, position, code)?;
        code = ascii_code_at(body, position);
    }

    // Numbers cannot be followed by . or NameStart
    if code == 0x002e || is_name_start(code) {
        return Err(lexer_syntax_error(
            lexer,
            position,
            format!(
                "Invalid number, expected digit but got: {}.",
                print_code_point_at(lexer, position)
            ),
        ));
    }

    let kind = if is_float {
        TokenKind::Float
    } else {
        TokenKind::Int
    };
    Ok((
        create_token(lexer, kind, start, position, span(start, position)),
        position,
    ))
}

/// Returns the new position in the source after reading one or more digits.
///
/// Derived from graphql-js 16.12.0, src/language/lexer.ts, MIT.
fn read_digits(lexer: &Lexer, start: usize, first_code: i32) -> Result<usize, Box<GraphQLError>> {
    if !is_digit(first_code) {
        return Err(lexer_syntax_error(
            lexer,
            start,
            format!(
                "Invalid number, expected digit but got: {}.",
                print_code_point_at(lexer, start)
            ),
        ));
    }

    let body = lexer.source.bytes();
    let mut position = start + 1; // +1 to skip first firstCode

    while is_digit(ascii_code_at(body, position)) {
        position += 1;
    }

    Ok(position)
}

/// Reads a single-quote string token from the source file.
///
/// ```text
/// StringValue ::
///   - `""` [lookahead != `"`]
///   - `"` StringCharacter+ `"`
///
/// StringCharacter ::
///   - SourceCharacter but not `"` or `\` or LineTerminator
///   - `\u` EscapedUnicode
///   - `\` EscapedCharacter
///
/// EscapedUnicode ::
///   - `{` HexDigit+ `}`
///   - HexDigit HexDigit HexDigit HexDigit
///
/// EscapedCharacter :: one of `"` `\` `/` `b` `f` `n` `r` `t`
/// ```
///
/// Derived from graphql-js 16.12.0, src/language/lexer.ts, MIT.
///
/// The value of a string without an escape is the text between its quotes,
/// and the token names it by its span. The value is built only from the
/// first escape on, and the token owns it.
// Out of line, so a document with no string compiles no reader of one.
#[inline(never)]
fn read_string(lexer: &Lexer, start: usize) -> Result<Lexed, Box<GraphQLError>> {
    let body = lexer.source.bytes();
    let body_length = body.len();
    let mut position = start + 1;
    let mut chunk_start = position;
    let mut value = String::new();

    while position < body_length {
        let code = char_code_at(body, position);

        // Closing Quote (")
        if code == 0x0022 {
            // The first chunk still starts after the open quote exactly when
            // no escape was read.
            let value = if chunk_start == start + 1 {
                span(chunk_start, position)
            } else {
                value.push_str(lexer.source.slice(chunk_start, position));
                TokenValue::Owned(value)
            };
            let end = position + 1;
            return Ok((
                create_token(lexer, TokenKind::String, start, end, value),
                end,
            ));
        }

        // Escape Sequence (\)
        if code == 0x005c {
            value.push_str(lexer.source.slice(chunk_start, position));
            let escape = if ascii_code_at(body, position + 1) == 0x0075 {
                if ascii_code_at(body, position + 2) == 0x007b {
                    read_escaped_unicode_variable_width(lexer, position)?
                } else {
                    read_escaped_unicode_fixed_width(lexer, position)?
                }
            } else {
                read_escaped_character(lexer, position)?
            };
            value.push(escape.value);
            position += escape.size;
            chunk_start = position;
            continue;
        }

        // LineTerminator (\n | \r)
        if code == 0x000a || code == 0x000d {
            break;
        }

        // SourceCharacter
        if is_unicode_scalar_value(code) {
            position += utf8_width(code);
        } else if is_supplementary_code_point(body, position) {
            position += SURROGATE_PAIR_WIDTH;
        } else {
            return Err(lexer_syntax_error(
                lexer,
                position,
                format!(
                    "Invalid character within String: {}.",
                    print_code_point_at(lexer, position)
                ),
            ));
        }
    }

    Err(lexer_syntax_error(lexer, position, "Unterminated string."))
}

/// The string value and lexed size of an escape sequence.
///
/// The value of every escape is one code point, and that of a surrogate pair
/// escape is the one the pair encodes, so it is a `char` here.
///
/// Derived from graphql-js 16.12.0, src/language/lexer.ts, MIT.
struct EscapeSequence {
    /// The character the escape stands for.
    value: char,
    /// The number of bytes the escape occupies in the body, each of them
    /// ASCII and so one code unit as well.
    size: usize,
}

/// Returns `head`, the code units of `quoted` and `tail` as one description.
///
/// This is the template literal of the three escape errors, which quote a fixed
/// count of code units of the body. The window may end on the leading half of an
/// astral character, and the description keeps that code unit the way the
/// JavaScript string of the reference does. The window starts at the byte offset
/// of the backslash and counts its code units from there character by character,
/// so it cuts an astral character where the reference cuts it.
fn quote_code_units(head: &str, quoted: &JsString, tail: &str) -> JsString {
    let mut description = JsStringBuilder::new();
    description.push_str(head).push(quoted).push_str(tail);
    description.finish()
}

/// Reads a `\u{...}` escape starting at `position`.
///
/// Derived from graphql-js 16.12.0, src/language/lexer.ts, MIT.
fn read_escaped_unicode_variable_width(
    lexer: &Lexer,
    position: usize,
) -> Result<EscapeSequence, Box<GraphQLError>> {
    let body = lexer.source.bytes();
    let mut point: i32 = 0;
    let mut size = 3;
    // Cannot be larger than 12 chars (\u{00000000}).
    while size < 12 {
        let code = ascii_code_at(body, position + size);
        size += 1;
        // Closing Brace (})
        if code == 0x007d {
            // Must be at least 5 chars (\u{0}) and encode a Unicode scalar value.
            if size < 5 || !is_unicode_scalar_value(point) {
                break;
            }
            return Ok(EscapeSequence {
                value: from_code_point(point),
                size,
            });
        }
        // Append this hex digit to the code point. The shift is the 32 bit
        // signed shift of JavaScript, which drops the bits above the sign, and
        // an `i32` shift drops them the same way.
        point = (point << 4) | read_hex_digit(code);
        if point < 0 {
            break;
        }
    }

    Err(lexer_syntax_error(
        lexer,
        position,
        quote_code_units(
            "Invalid Unicode escape sequence: \"",
            &slice_code_units(body, position, size),
            "\".",
        ),
    ))
}

/// Reads a `\uXXXX` escape starting at `position`, with its surrogate pair form.
///
/// Derived from graphql-js 16.12.0, src/language/lexer.ts, MIT.
fn read_escaped_unicode_fixed_width(
    lexer: &Lexer,
    position: usize,
) -> Result<EscapeSequence, Box<GraphQLError>> {
    let body = lexer.source.bytes();
    let code = read_16_bit_hex_code(body, position + 2);

    if is_unicode_scalar_value(code) {
        return Ok(EscapeSequence {
            value: from_code_point(code),
            size: 6,
        });
    }

    // GraphQL allows JSON-style surrogate pair escape sequences, but only when
    // a valid pair is formed.
    if is_leading_surrogate(code) {
        // \u
        if ascii_code_at(body, position + 6) == 0x005c
            && ascii_code_at(body, position + 7) == 0x0075
        {
            let trailing_code = read_16_bit_hex_code(body, position + 8);
            if is_trailing_surrogate(trailing_code) {
                // JavaScript defines strings as a sequence of UTF-16 code units and
                // encodes Unicode code points above U+FFFF using a surrogate pair of
                // code units. Since this is a surrogate pair escape sequence, the
                // reference includes both codes into the JavaScript string value,
                // where they read as the supplementary code point a Rust string
                // holds directly.
                return Ok(EscapeSequence {
                    value: from_code_point(utf16_surrogate_pair_to_code_point(code, trailing_code)),
                    size: 12,
                });
            }
        }
    }

    Err(lexer_syntax_error(
        lexer,
        position,
        quote_code_units(
            "Invalid Unicode escape sequence: \"",
            &slice_code_units(body, position, 6),
            "\".",
        ),
    ))
}

/// Reads four hexadecimal characters and returns the positive integer that
/// 16bit hexadecimal string represents.
///
/// For example, "000f" will return 15, and "dead" will return 57005. Returns a
/// negative number if any char was not a valid hexadecimal digit.
///
/// Derived from graphql-js 16.12.0, src/language/lexer.ts, MIT.
///
/// The four reads are four bytes. A hex digit is ASCII, so the bytes are the
/// four characters of the reference whenever all of them are digits, and
/// otherwise the first character that is no digit starts at the same byte
/// in both readings and makes the result negative in both.
fn read_16_bit_hex_code(body: &[u8], position: usize) -> i32 {
    // readHexDigit() returns -1 on error. ORing a negative value with any other
    // value always produces a negative value.
    (read_hex_digit(ascii_code_at(body, position)) << 12)
        | (read_hex_digit(ascii_code_at(body, position + 1)) << 8)
        | (read_hex_digit(ascii_code_at(body, position + 2)) << 4)
        | read_hex_digit(ascii_code_at(body, position + 3))
}

/// Reads a hexadecimal character and returns its positive integer value (0-15).
///
/// '0' becomes 0, '9' becomes 9, 'A' becomes 10, 'F' becomes 15, 'a' becomes
/// 10 and 'f' becomes 15. Returns -1 if the provided character code was not
/// a valid hexadecimal digit.
///
/// ```text
/// HexDigit :: one of
///   - `0` `1` `2` `3` `4` `5` `6` `7` `8` `9`
///   - `A` `B` `C` `D` `E` `F`
///   - `a` `b` `c` `d` `e` `f`
/// ```
///
/// Derived from graphql-js 16.12.0, src/language/lexer.ts, MIT.
const fn read_hex_digit(code: i32) -> i32 {
    if code >= 0x0030 && code <= 0x0039 {
        // 0-9
        code - 0x0030
    } else if code >= 0x0041 && code <= 0x0046 {
        // A-F
        code - 0x0037
    } else if code >= 0x0061 && code <= 0x0066 {
        // a-f
        code - 0x0057
    } else {
        -1
    }
}

/// Reads a single character escape starting at `position`.
///
/// | Escaped Character | Code Point | Character Name               |
/// | ----------------- | ---------- | ---------------------------- |
/// | `"`               | U+0022     | double quote                 |
/// | `\`               | U+005C     | reverse solidus (back slash) |
/// | `/`               | U+002F     | solidus (forward slash)      |
/// | `b`               | U+0008     | backspace                    |
/// | `f`               | U+000C     | form feed                    |
/// | `n`               | U+000A     | line feed (new line)         |
/// | `r`               | U+000D     | carriage return              |
/// | `t`               | U+0009     | horizontal tab               |
///
/// Derived from graphql-js 16.12.0, src/language/lexer.ts, MIT.
fn read_escaped_character(
    lexer: &Lexer,
    position: usize,
) -> Result<EscapeSequence, Box<GraphQLError>> {
    let body = lexer.source.bytes();
    let code = ascii_code_at(body, position + 1);
    let value = match code {
        0x0022 => '\u{0022}', // "
        0x005c => '\u{005c}', // \
        0x002f => '\u{002f}', // /
        0x0062 => '\u{0008}', // b
        0x0066 => '\u{000c}', // f
        0x006e => '\u{000a}', // n
        0x0072 => '\u{000d}', // r
        0x0074 => '\u{0009}', // t
        _ => {
            return Err(lexer_syntax_error(
                lexer,
                position,
                quote_code_units(
                    "Invalid character escape sequence: \"",
                    &slice_code_units(body, position, 2),
                    "\".",
                ),
            ));
        }
    };
    Ok(EscapeSequence { value, size: 2 })
}

/// Reads a block string token from the source file.
///
/// ```text
/// StringValue ::
///   - `"""` BlockStringCharacter* `"""`
///
/// BlockStringCharacter ::
///   - SourceCharacter but not `"""` or `\"""`
///   - `\"""`
/// ```
///
/// Derived from graphql-js 16.12.0, src/language/lexer.ts, MIT.
///
/// A raw line is text of the body unless an escaped triple quote splits it,
/// so a line borrows the body and owns a string only when it joins the
/// chunks around such an escape. The value, the dedented lines joined, is
/// built apart from those joined lines, and the token owns it. A block
/// string without an escaped triple quote builds the value alone.
// Out of line, so a document with no block string compiles no reader of one.
#[inline(never)]
fn read_block_string(lexer: &mut Lexer, start: usize) -> Result<Lexed, Box<GraphQLError>> {
    // The walk borrows the field `source` alone, so the line counters, other
    // fields of the same lexer, move while it holds the body.
    let body = lexer.source.bytes();
    let body_length = body.len();
    let mut line_start = widen(lexer.line_start);
    // The bytes before `position` beyond their code units, so that the
    // start of a line inside the block string is its byte offset less this.
    let mut excess = lexer.excess;

    let mut position = start + 3;
    let mut chunk_start = position;
    let mut current_line = Cow::Borrowed("");

    let mut block_lines: Vec<Cow<'_, str>> = Vec::new();
    while position < body_length {
        let code = char_code_at(body, position);

        // Closing Triple-Quote (""")
        if code == 0x0022
            && ascii_code_at(body, position + 1) == 0x0022
            && ascii_code_at(body, position + 2) == 0x0022
        {
            push_chunk(&mut current_line, lexer.source.slice(chunk_start, position));
            block_lines.push(current_line);

            let end = position + 3;
            let token = create_token(
                lexer,
                TokenKind::BlockString,
                start,
                end,
                // Return a string of the lines joined with U+000A.
                TokenValue::Owned(dedent_block_string_lines(&block_lines).join("\n")),
            );

            lexer.line += narrow(block_lines.len() - 1);
            lexer.line_start = narrow(line_start);
            return Ok((token, end));
        }

        // Escaped Triple-Quote (\""")
        if code == 0x005c
            && ascii_code_at(body, position + 1) == 0x0022
            && ascii_code_at(body, position + 2) == 0x0022
            && ascii_code_at(body, position + 3) == 0x0022
        {
            push_chunk(&mut current_line, lexer.source.slice(chunk_start, position));
            chunk_start = position + 1; // skip only slash
            position += 4;
            continue;
        }

        // LineTerminator
        if code == 0x000a || code == 0x000d {
            push_chunk(&mut current_line, lexer.source.slice(chunk_start, position));
            block_lines.push(current_line);

            if code == 0x000d && ascii_code_at(body, position + 1) == 0x000a {
                position += 2;
            } else {
                position += 1;
            }

            current_line = Cow::Borrowed("");
            chunk_start = position;
            line_start = position - excess;
            continue;
        }

        // SourceCharacter, whose continuation bytes are bytes beyond its
        // code units, less one for the second unit of a surrogate pair.
        if is_unicode_scalar_value(code) {
            let width = utf8_width(code);
            position += width;
            excess += width - 1;
        } else if is_supplementary_code_point(body, position) {
            position += SURROGATE_PAIR_WIDTH;
            excess += SURROGATE_PAIR_WIDTH - 2;
        } else {
            return Err(lexer_syntax_error(
                lexer,
                position,
                format!(
                    "Invalid character within String: {}.",
                    print_code_point_at(lexer, position)
                ),
            ));
        }
    }

    Err(lexer_syntax_error(lexer, position, "Unterminated string."))
}

/// Appends `chunk` to the raw block string line `line`.
///
/// This is the `currentLine += body.slice(chunkStart, position)` of the
/// reference. An empty line borrows the chunk, and a line that holds text
/// already becomes an owned string, which happens only across an escaped
/// triple quote.
fn push_chunk<'a>(line: &mut Cow<'a, str>, chunk: &'a str) {
    if line.is_empty() {
        *line = Cow::Borrowed(chunk);
    } else if !chunk.is_empty() {
        line.to_mut().push_str(chunk);
    }
}

/// Reads an alphanumeric + underscore name from the source.
///
/// ```text
/// Name ::
///   - NameStart NameContinue* [lookahead != NameContinue]
/// ```
///
/// `lexer` holds the source and `start` is the byte offset of the name start.
///
/// Derived from graphql-js 16.12.0, src/language/lexer.ts, MIT.
///
/// # Panics
///
/// In a build with overflow checks, panics when the column underflows, for
/// a `start` more than one code unit before the line start of `lexer`, as
/// [`create_token`] does.
#[must_use]
pub fn read_name(lexer: &Lexer, start: usize) -> Token {
    read_name_to(lexer, start).0
}

/// Reads a name as [`read_name`] does and returns the byte offset after it
/// beside the token.
fn read_name_to(lexer: &Lexer, start: usize) -> Lexed {
    let body = lexer.source.bytes();
    let body_length = body.len();
    let mut position = start + 1;

    while position < body_length {
        let code = ascii_code_at(body, position);
        if is_name_continue(code) {
            position += 1;
        } else {
            break;
        }
    }

    (
        create_token(
            lexer,
            TokenKind::Name,
            start,
            position,
            span(start, position),
        ),
        position,
    )
}

/// A helper function to describe a token as a string for debugging.
///
/// The description is the kind description followed by the value in double
/// quotes when the token carries one, and `source` is the source the lexer read
/// `token` from, which holds the text of its value.
///
/// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
#[must_use]
pub fn get_token_desc(token: &Token, source: &Source) -> String {
    let value = token.value_in(source);
    get_token_kind_desc(token.kind)
        + &value.map_or_else(String::new, |value| format!(" \"{value}\""))
}

/// A helper function to describe a token kind as a string for debugging.
///
/// A punctuator is quoted and every other kind is its bare name.
///
/// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
#[must_use]
pub fn get_token_kind_desc(kind: TokenKind) -> String {
    if is_punctuator_token_kind(kind) {
        format!("\"{}\"", kind.as_str())
    } else {
        String::from(kind.as_str())
    }
}
