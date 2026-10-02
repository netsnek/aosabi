// SPDX-License-Identifier: MIT
/*
 * aosabi-build: the JSDoc parser of the TypeScript checker, over the text of
 * one documentation block, for the source path.
 *
 * Every line below this block is the line of that file below its block,
 * because the source path reads a comment as the derive path reads it and a
 * procedural macro crate exports no function another crate calls, and check C6
 * of tests/notebooks/12_authoring_parity.ipynb holds the two equal.
 */

//! The `JSDoc` parser of the TypeScript checker.
//!
//! [`parse`] reads the text between `/**` and `*/` the way
//! `parseJSDocCommentWorker` reads it and returns the comment and the tags the
//! checker hands to the inference. Every position, length, indent and margin is a
//! count of UTF-16 code units, as in the reference.
//!
//! The functions keep the names of the reference in snake case, and the
//! control flow follows it line by line, because the margin, the blank lines
//! and the tag split decide bytes of the description. Four places of the
//! reference call the scanner of the TypeScript grammar and not the one of
//! `JSDoc`: after an identifier of a link name, of a `@see` name and of a
//! bracketed parameter name. [`Parser::next_token_regular`] stands in for it
//! by skipping the whitespace and the line breaks the grammar scanner treats
//! as trivia, which is what those places observe.
//!
//! # What is left out
//!
//! The reference parses a type expression with the TypeScript grammar, and
//! this port skips the braces of one with their nesting counted. Tags other
//! than `@param`, `@arg`, `@argument`, `@return`, `@returns`, `@see`,
//! `@throws` and `@exception` are read as the unknown tag, whose text is its
//! comment. A child `@param` of an `Object` parameter is a tag of its own. A
//! link renders as `buildLinkParts` renders one whose name resolves to a
//! declaration, because the macro cannot resolve a name.

/// The token kinds of the `JSDoc` scanner the parser tells apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Token {
    /// A character no other kind covers, `SyntaxKind.Unknown` and the
    /// punctuation the parser never tests.
    Unknown,
    /// The end of the text.
    EndOfFile,
    /// A run of single line whitespace.
    Whitespace,
    /// One line break.
    NewLine,
    /// `@`.
    At,
    /// `*`.
    Asterisk,
    /// `{`.
    OpenBrace,
    /// `}`.
    CloseBrace,
    /// `[`.
    OpenBracket,
    /// `]`.
    CloseBracket,
    /// `.`.
    Dot,
    /// `=`.
    Equals,
    /// `` ` ``.
    Backtick,
    /// An identifier, which in `JSDoc` may hold `-`.
    Identifier,
    /// A run of comment text, `SyntaxKind.JSDocCommentTextToken`.
    CommentText,
}

/// The comment state of the two loops of the parser.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    /// `BeginningOfLine`.
    BeginningOfLine,
    /// `SawAsterisk`.
    SawAsterisk,
    /// `SavingComments`.
    SavingComments,
    /// `SavingBackticks`.
    SavingBackticks,
}

/// The scanner state a look ahead saves and restores.
#[derive(Clone, Copy, Debug)]
struct Cursor {
    /// The position after the current token.
    pos: usize,
    /// The start of the current token without its trivia.
    token_start: usize,
    /// The start of the current token with its trivia, `getTokenFullStart`.
    full_start: usize,
    /// The current token.
    token: Token,
    /// Whether a line break precedes the current token,
    /// `hasPrecedingLineBreak`.
    preceding_line_break: bool,
}

/// What a tag is beyond its name, where the text of the tag depends on it.
#[derive(Clone, Debug)]
enum TagKind {
    /// A `JSDocParameterTag`, with the text of its name and whether the name
    /// is one identifier.
    Parameter {
        /// The source text of the name.
        name: String,
        /// Whether the name is an identifier and not a qualified name.
        is_identifier: bool,
    },
    /// A `JSDocSeeTag`, with the source text of its name reference.
    See(Option<String>),
    /// A `JSDocThrowsTag`, with the source text of its type expression.
    Throws(Option<String>),
    /// Every other tag, whose text is its comment.
    Plain,
}

/// One tag of a documentation block.
#[derive(Clone, Debug)]
pub(crate) struct Tag {
    /// The tag name without the `@`, `tag.tagName.text`.
    pub(crate) name: String,
    /// The comment of the tag, `undefined` in the reference when absent.
    pub(crate) comment: Option<String>,
    /// What the tag is beyond its name.
    kind: TagKind,
}

impl Tag {
    /// Returns the tag text, `displayPartsToString(getCommentDisplayParts(tag))`
    /// of typescript.js:175846-175906.
    pub(crate) fn text(&self) -> String {
        match &self.kind {
            TagKind::Parameter { name, .. }
            | TagKind::See(Some(name))
            | TagKind::Throws(Some(name)) => self.add_comment(name),
            TagKind::See(None) | TagKind::Throws(None) | TagKind::Plain => {
                self.comment.clone().unwrap_or_default()
            }
        }
    }

    /// Returns `s` followed by the comment, the `addComment` arrow.
    ///
    /// A name that is `http` or `https` joins its comment with no space,
    /// because the comment of `@see https://example.com` starts with `://`.
    fn add_comment(&self, s: &str) -> String {
        match &self.comment {
            Some(comment) if s == "http" || s == "https" => format!("{s}{comment}"),
            Some(comment) => format!("{s} {comment}"),
            None => s.to_owned(),
        }
    }

    /// Returns the parameter a `@param` tag names, when the name is one
    /// identifier, the test of `getJSDocParameterTagsWorker` at
    /// typescript.js:15126-15141.
    pub(crate) fn parameter_name(&self) -> Option<&str> {
        match &self.kind {
            TagKind::Parameter {
                name,
                is_identifier: true,
            } => Some(name),
            _ => None,
        }
    }

    /// Returns whether the tag is the generic tag `@inheritDoc`, which the
    /// reference tests at typescript.js:175781.
    pub(crate) fn is_inherit_doc(&self) -> bool {
        matches!(self.kind, TagKind::Plain)
            && (self.name == "inheritDoc" || self.name == "inheritdoc")
    }
}

/// One documentation block as the checker reads it.
#[derive(Clone, Debug, Default)]
pub(crate) struct Block {
    /// The comment, `undefined` in the reference when the block has no text
    /// before its first tag.
    pub(crate) comment: Option<String>,
    /// The tags in source order.
    pub(crate) tags: Vec<Tag>,
}

/// Returns the block the reference parses from `body`, the text between
/// `/**` and `*/`, whose `/**` stands at the zero based `column` of its line.
pub(crate) fn parse(body: &str, column: usize) -> Block {
    let mut parser = Parser {
        text: body.encode_utf16().collect(),
        cursor: Cursor {
            pos: 0,
            token_start: 0,
            full_start: 0,
            token: Token::Unknown,
            preceding_line_break: false,
        },
    };
    parser.do_jsdoc_scan(units(column))
}

/// Returns `count` as a signed count of code units, saturated.
fn units(count: usize) -> isize {
    isize::try_from(count).unwrap_or(isize::MAX)
}

/// Returns the length of `text` in UTF-16 code units, `text.length`.
fn length(text: &str) -> isize {
    units(text.encode_utf16().count())
}

/// Returns `text.slice(start)` as ECMA-262 section 22.1.3.25 defines it: a
/// negative start counts from the end and is clamped at zero, and a start
/// past the end gives the empty string.
fn js_slice(text: &str, start: isize) -> String {
    let units: Vec<u16> = text.encode_utf16().collect();
    let len = self::units(units.len());
    let from = if start < 0 {
        (len + start).max(0)
    } else {
        start.min(len)
    };
    let from = usize::try_from(from).unwrap_or(0);
    String::from_utf16_lossy(units.get(from..).unwrap_or_default())
}

/// Returns whether `c` is removed by `String.prototype.trimEnd`, which is
/// `WhiteSpace` or `LineTerminator` of ECMA-262 sections 12.2 and 12.3, and
/// not the `White_Space` property `str::trim_end` tests.
fn is_js_trimmed(c: char) -> bool {
    matches!(
        c,
        '\t' | '\u{b}' | '\u{c}' | '\u{feff}' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\n'
                | '\r'
                | '\u{2028}'
                | '\u{2029}'
    )
}

/// Returns `text.trimEnd()`.
fn js_trim_end(text: &str) -> String {
    text.trim_end_matches(is_js_trimmed).to_owned()
}

/// Returns whether the code point `ch` is single line whitespace,
/// `isWhiteSpaceSingleLine` of typescript.js:11737-11739.
fn is_white_space_single_line(ch: u32) -> bool {
    matches!(
        ch,
        0x20 | 0x09 | 0x0B | 0x0C | 0xA0 | 0x85 | 0x1680 | 0x2000
            ..=0x200B | 0x202F | 0x205F | 0x3000 | 0xFEFF
    )
}

/// Returns whether the code point `ch` breaks a line, `isLineBreak` of
/// typescript.js:11740-11742.
fn is_line_break(ch: u32) -> bool {
    matches!(ch, 0x0A | 0x0D | 0x2028 | 0x2029)
}

/// Returns whether `ch` may start an identifier.
///
/// The reference tests the Unicode property `ID_Start`, and this test takes
/// the alphabetic property for a character outside ASCII.
fn is_identifier_start(ch: u32) -> bool {
    char::from_u32(ch).is_some_and(|c| {
        c.is_ascii_alphabetic() || c == '$' || c == '_' || (!c.is_ascii() && c.is_alphabetic())
    })
}

/// Returns whether `ch` may continue an identifier, with the same
/// approximation as [`is_identifier_start`] for `ID_Continue`.
fn is_identifier_part(ch: u32) -> bool {
    is_identifier_start(ch)
        || ch == 0x200C
        || ch == 0x200D
        || char::from_u32(ch)
            .is_some_and(|c| c.is_ascii_digit() || (!c.is_ascii() && c.is_alphanumeric()))
}

/// Returns whether `kind` names a link, `isJSDocLinkTag`.
fn is_jsdoc_link_tag(kind: &str) -> bool {
    kind == "link" || kind == "linkcode" || kind == "linkplain"
}

/// The text pieces of a comment, the `comments` and `parts` arrays of the
/// reference, where a part is the text before a link or a rendered link.
#[derive(Default)]
struct Pieces {
    /// The text since the last link.
    comments: Vec<String>,
    /// The parts, each rendered to its text.
    parts: Vec<String>,
}

impl Pieces {
    /// Closes the text before a link and adds the rendered link.
    fn link(&mut self, rendered: String) {
        self.parts.push(self.comments.concat());
        self.parts.push(rendered);
        self.comments.clear();
    }

    /// Returns the comment the pieces make, which is the parts with the
    /// trimmed text after the last link, the text alone, or `undefined`.
    fn finish(&self) -> Option<String> {
        let trimmed = js_trim_end(&self.comments.concat());
        if !self.parts.is_empty() {
            let mut text = self.parts.concat();
            text.push_str(&trimmed);
            return Some(text);
        }
        (!trimmed.is_empty()).then_some(trimmed)
    }
}

/// Removes the leading line breaks of `comments`, `removeLeadingNewlines`.
fn remove_leading_newlines(comments: &mut Vec<String>) {
    let count = comments
        .iter()
        .take_while(|comment| *comment == "\n" || *comment == "\r")
        .count();
    comments.drain(..count);
}

/// Removes the trailing whitespace of `comments`, `removeTrailingWhitespace`.
fn remove_trailing_whitespace(comments: &mut Vec<String>) {
    while let Some(last) = comments.last_mut() {
        let trimmed = js_trim_end(last);
        if trimmed.is_empty() {
            comments.pop();
        } else {
            if trimmed.len() < last.len() {
                *last = trimmed;
            }
            break;
        }
    }
}

/// Adds `text` to `comments`, the `pushComment` arrow of both loops.
///
/// The reference tests `!margin`, which ECMA-262 `ToBoolean` makes true for a
/// margin of 0 as well as for an absent one.
fn push_comment(
    comments: &mut Vec<String>,
    margin: &mut Option<isize>,
    indent: &mut isize,
    text: String,
) {
    if margin.is_none_or(|margin| margin == 0) {
        *margin = Some(*indent);
    }
    *indent += length(&text);
    comments.push(text);
}

/// Returns the text of a link as `buildLinkParts` renders one whose name
/// resolves to a declaration, typescript.js:141207-141259.
fn render_link(kind: &str, name: Option<&str>, text: &str) -> String {
    let mut out = format!("{{@{kind} ");
    match name {
        None => out.push_str(text),
        Some(name) => {
            let suffix = find_link_name_end(text);
            let (head, tail) = text.split_at(suffix);
            out.push_str(name);
            out.push_str(head);
            out.push_str(skip_separator_from_link_text(tail));
        }
    }
    out.push('}');
    out
}

/// Returns the byte length of the part of a link text that belongs to the
/// name, `findLinkNameEnd`, which ends at an ASCII character, so the count
/// in bytes marks the same place as the count in code units of the
/// reference.
fn find_link_name_end(text: &str) -> usize {
    if text.starts_with("://") {
        return text.find('|').unwrap_or(text.len());
    }
    if text.starts_with("()") {
        return 2;
    }
    if text.starts_with('<') {
        let mut brackets = 0_i32;
        for (i, c) in text.char_indices() {
            if c == '<' {
                brackets += 1;
            }
            if c == '>' {
                brackets -= 1;
            }
            if brackets == 0 {
                return i + c.len_utf8();
            }
        }
    }
    0
}

/// Returns `text` without a leading `|` and the spaces after it,
/// `skipSeparatorFromLinkText`.
fn skip_separator_from_link_text(text: &str) -> &str {
    text.strip_prefix('|')
        .map_or(text, |rest| rest.trim_start_matches(' '))
}

/// The parser over one block.
struct Parser {
    /// The text between `/**` and `*/` as UTF-16 code units.
    text: Vec<u16>,
    /// The scanner state.
    cursor: Cursor,
}

impl Parser {
    /// Returns the code unit at `pos` widened.
    fn unit(&self, pos: usize) -> u32 {
        self.text.get(pos).copied().map_or(0, u32::from)
    }

    /// Returns the code point at `pos`, `codePointUnchecked`.
    fn code_point_at(&self, pos: usize) -> u32 {
        let high = self.unit(pos);
        let low = self.unit(pos + 1);
        if (0xD800..0xDC00).contains(&high) && (0xDC00..0xE000).contains(&low) {
            return 0x10000 + ((high - 0xD800) << 10) + (low - 0xDC00);
        }
        high
    }

    /// Returns the current token.
    fn token(&self) -> Token {
        self.cursor.token
    }

    /// Returns the text of the current token, `getTokenText`.
    fn token_text(&self) -> String {
        let range = self.cursor.token_start..self.cursor.pos;
        String::from_utf16_lossy(self.text.get(range).unwrap_or_default())
    }

    /// Returns the text between `start` and `end`.
    fn slice(&self, start: usize, end: usize) -> String {
        String::from_utf16_lossy(self.text.get(start..end).unwrap_or_default())
    }

    /// Scans the next `JSDoc` token, `scanJsDocToken` of
    /// typescript.js:14438-14523, and `nextTokenJSDoc`.
    fn next_token_jsdoc(&mut self) -> Token {
        let start = self.cursor.pos;
        self.cursor.full_start = start;
        self.cursor.token_start = start;
        self.cursor.preceding_line_break = false;
        if start >= self.text.len() {
            self.cursor.token = Token::EndOfFile;
            return Token::EndOfFile;
        }
        let ch = self.code_point_at(start);
        let mut pos = start + if ch >= 0x10000 { 2 } else { 1 };
        let token = match ch {
            0x09 | 0x0B | 0x0C | 0x20 => {
                while pos < self.text.len() && is_white_space_single_line(self.unit(pos)) {
                    pos += 1;
                }
                Token::Whitespace
            }
            0x40 => Token::At,
            0x0D | 0x0A => {
                if ch == 0x0D && self.unit(pos) == 0x0A {
                    pos += 1;
                }
                self.cursor.preceding_line_break = true;
                Token::NewLine
            }
            0x2A => Token::Asterisk,
            0x7B => Token::OpenBrace,
            0x7D => Token::CloseBrace,
            0x5B => Token::OpenBracket,
            0x5D => Token::CloseBracket,
            0x2E => Token::Dot,
            0x3D => Token::Equals,
            0x60 => Token::Backtick,
            _ if is_identifier_start(ch) => {
                pos = self.identifier_end(pos);
                Token::Identifier
            }
            _ => Token::Unknown,
        };
        self.cursor.pos = pos;
        self.cursor.token = token;
        token
    }

    /// Returns the end of an identifier whose parts start at `pos`.
    ///
    /// The reference also takes a `-` as a part. Its loop tests the last
    /// character again once the text ends, which never ends for a text that
    /// ends in `-`, and this loop stops at the end.
    fn identifier_end(&self, mut pos: usize) -> usize {
        while pos < self.text.len() {
            let ch = self.code_point_at(pos);
            if !is_identifier_part(ch) && ch != 0x2D {
                break;
            }
            pos += if ch >= 0x10000 { 2 } else { 1 };
        }
        pos
    }

    /// Scans a run of comment text, `scanJSDocCommentTextToken` of
    /// typescript.js:14417-14437, and `nextJSDocCommentTextToken`.
    ///
    /// The run ends at a line break and at a backtick, and outside backticks
    /// at `{` and at an `@` that follows single line whitespace and precedes
    /// no whitespace.
    fn next_comment_text_token(&mut self, in_backticks: bool) -> Token {
        let start = self.cursor.pos;
        self.cursor.full_start = start;
        self.cursor.token_start = start;
        self.cursor.preceding_line_break = false;
        if start >= self.text.len() {
            self.cursor.token = Token::EndOfFile;
            return Token::EndOfFile;
        }
        let mut pos = start;
        while pos < self.text.len() {
            let ch = self.unit(pos);
            if is_line_break(ch) || ch == 0x60 {
                break;
            }
            if !in_backticks && (ch == 0x7B || self.is_tag_at(pos)) {
                break;
            }
            pos += 1;
        }
        if pos == start {
            return self.next_token_jsdoc();
        }
        self.cursor.pos = pos;
        self.cursor.token = Token::CommentText;
        Token::CommentText
    }

    /// Returns whether the `@` at `pos` starts a tag inside a line.
    ///
    /// The character before the text is the last `*` of `/**`, which is no
    /// whitespace, so position 0 never starts one.
    fn is_tag_at(&self, pos: usize) -> bool {
        self.unit(pos) == 0x40
            && pos >= 1
            && is_white_space_single_line(self.unit(pos - 1))
            && !(pos + 1 < self.text.len()
                && (is_white_space_single_line(self.unit(pos + 1))
                    || is_line_break(self.unit(pos + 1))))
    }

    /// Scans the next token the way the grammar scanner does after an
    /// identifier, which skips whitespace and line breaks as trivia, so the
    /// full start of the token is the end of the identifier.
    fn next_token_regular(&mut self) -> Token {
        let full_start = self.cursor.pos;
        let mut pos = full_start;
        let mut crossed = false;
        while pos < self.text.len() {
            let ch = self.unit(pos);
            if is_line_break(ch) {
                crossed = true;
            } else if !is_white_space_single_line(ch) {
                break;
            }
            pos += 1;
        }
        self.cursor.pos = pos;
        let token = self.next_token_jsdoc();
        self.cursor.full_start = full_start;
        self.cursor.preceding_line_break |= crossed;
        token
    }

    /// Moves back to `position`, `resetTokenState` of typescript.js:14604-14612.
    fn reset_token_state(&mut self, position: usize) {
        self.cursor = Cursor {
            pos: position,
            token_start: position,
            full_start: position,
            token: Token::Unknown,
            preceding_line_break: false,
        };
    }

    /// Runs `callback` and restores the scanner, `lookAhead`.
    fn look_ahead<T>(&mut self, callback: impl FnOnce(&mut Self) -> T) -> T {
        let saved = self.cursor;
        let result = callback(self);
        self.cursor = saved;
        result
    }

    /// Runs `callback` and restores the scanner when it gives nothing,
    /// `tryParse`.
    fn try_parse<T>(&mut self, callback: impl FnOnce(&mut Self) -> Option<T>) -> Option<T> {
        let saved = self.cursor;
        let result = callback(self);
        if result.is_none() {
            self.cursor = saved;
        }
        result
    }

    /// Moves past the current token when it is `token`,
    /// `parseOptionalJsdoc`.
    fn parse_optional_jsdoc(&mut self, token: Token) -> bool {
        if self.token() == token {
            self.next_token_jsdoc();
            return true;
        }
        false
    }

    /// Reads the whole block, `doJSDocScan` of typescript.js:39067-39159.
    fn do_jsdoc_scan(&mut self, column: isize) -> Block {
        let mut state = State::SawAsterisk;
        let mut margin: Option<isize> = None;
        let mut indent = column + 4;
        let mut pieces = Pieces::default();
        let mut tags = Vec::new();
        let mut linked = false;
        self.next_token_jsdoc();
        while self.parse_optional_jsdoc(Token::Whitespace) {}
        if self.parse_optional_jsdoc(Token::NewLine) {
            state = State::BeginningOfLine;
            indent = 0;
        }
        loop {
            match self.token() {
                Token::At => {
                    remove_trailing_whitespace(&mut pieces.comments);
                    tags.push(self.parse_tag(indent));
                    state = State::BeginningOfLine;
                    margin = None;
                }
                Token::NewLine => {
                    pieces.comments.push(self.token_text());
                    state = State::BeginningOfLine;
                    indent = 0;
                }
                Token::Asterisk => {
                    if state == State::SawAsterisk {
                        state = State::SavingComments;
                        push_comment(
                            &mut pieces.comments,
                            &mut margin,
                            &mut indent,
                            self.token_text(),
                        );
                    } else {
                        state = State::SawAsterisk;
                        indent += 1;
                    }
                }
                Token::Whitespace => {
                    let whitespace = self.token_text();
                    if let Some(margin) = margin
                        && indent + length(&whitespace) > margin
                    {
                        pieces.comments.push(js_slice(&whitespace, margin - indent));
                    }
                    indent += length(&whitespace);
                }
                Token::EndOfFile => break,
                Token::OpenBrace => {
                    state = State::SavingComments;
                    if let Some(link) = self.parse_jsdoc_link() {
                        if !linked {
                            remove_leading_newlines(&mut pieces.comments);
                        }
                        pieces.link(link);
                        linked = true;
                    } else {
                        push_comment(
                            &mut pieces.comments,
                            &mut margin,
                            &mut indent,
                            self.token_text(),
                        );
                    }
                }
                _ => {
                    state = State::SavingComments;
                    push_comment(
                        &mut pieces.comments,
                        &mut margin,
                        &mut indent,
                        self.token_text(),
                    );
                }
            }
            if state == State::SavingComments {
                self.next_comment_text_token(false);
            } else {
                self.next_token_jsdoc();
            }
        }
        Block {
            comment: pieces.finish(),
            tags,
        }
    }

    /// Returns whether only whitespace and line breaks remain,
    /// `isNextNonwhitespaceTokenEndOfFile`.
    fn is_next_nonwhitespace_token_end_of_file(&mut self) -> bool {
        loop {
            match self.next_token_jsdoc() {
                Token::EndOfFile => return true,
                Token::Whitespace | Token::NewLine => {}
                _ => return false,
            }
        }
    }

    /// Returns whether the current token is whitespace or a line break.
    fn is_whitespace_or_newline(&self) -> bool {
        matches!(self.token(), Token::Whitespace | Token::NewLine)
    }

    /// Skips whitespace and line breaks unless only those remain,
    /// `skipWhitespace`.
    fn skip_whitespace(&mut self) {
        if self.is_whitespace_or_newline()
            && self.look_ahead(Self::is_next_nonwhitespace_token_end_of_file)
        {
            return;
        }
        while self.is_whitespace_or_newline() {
            self.next_token_jsdoc();
        }
    }

    /// Skips whitespace, line breaks and the leading `*` of a line, and
    /// returns the gutter of the last line crossed, `skipWhitespaceOrAsterisk`
    /// of typescript.js:39200-39221.
    fn skip_whitespace_or_asterisk(&mut self) -> String {
        if self.is_whitespace_or_newline()
            && self.look_ahead(Self::is_next_nonwhitespace_token_end_of_file)
        {
            return String::new();
        }
        let mut preceding_line_break = self.cursor.preceding_line_break;
        let mut seen_line_break = false;
        let mut indent_text = String::new();
        while (preceding_line_break && self.token() == Token::Asterisk)
            || self.is_whitespace_or_newline()
        {
            indent_text.push_str(&self.token_text());
            if self.token() == Token::NewLine {
                preceding_line_break = true;
                seen_line_break = true;
                indent_text.clear();
            } else if self.token() == Token::Asterisk {
                preceding_line_break = false;
            }
            self.next_token_jsdoc();
        }
        if seen_line_break {
            indent_text
        } else {
            String::new()
        }
    }

    /// Reads one tag at its `@`, `parseTag` of typescript.js:39222-39313.
    ///
    /// `margin` is the indent of the `@`.
    fn parse_tag(&mut self, margin: isize) -> Tag {
        let start = self.cursor.token_start;
        self.next_token_jsdoc();
        let tag_name = self.parse_jsdoc_identifier_name();
        let indent_text = self.skip_whitespace_or_asterisk();
        match tag_name.as_str() {
            "arg" | "argument" | "param" => {
                self.parse_parameter_or_property_tag(start, tag_name, margin)
            }
            "return" | "returns" => {
                self.try_parse_type_expression();
                self.tag_with_comments(start, tag_name, margin, &indent_text, TagKind::Plain)
            }
            "see" => self.parse_see_tag(start, tag_name, margin, &indent_text),
            "exception" | "throws" => {
                let r#type = self.try_parse_type_expression();
                self.tag_with_comments(
                    start,
                    tag_name,
                    margin,
                    &indent_text,
                    TagKind::Throws(r#type),
                )
            }
            _ => self.tag_with_comments(start, tag_name, margin, &indent_text, TagKind::Plain),
        }
    }

    /// Returns the tag `tag_name` of `kind` with the comment that follows,
    /// the shape of `parseUnknownTag`, `parseSimpleTag`, `parseReturnTag`
    /// and `parseThrowsTag`.
    fn tag_with_comments(
        &mut self,
        start: usize,
        tag_name: String,
        margin: isize,
        indent_text: &str,
        kind: TagKind,
    ) -> Tag {
        let comment =
            self.parse_trailing_tag_comments(start, self.cursor.full_start, margin, indent_text);
        Tag {
            name: tag_name,
            comment,
            kind,
        }
    }

    /// Reads the comment of a tag, `parseTrailingTagComments` of
    /// typescript.js:39314-39319.
    ///
    /// A comment on the line of the tag keeps its margin at the column where
    /// it starts, and a comment on the next line keeps the part of its gutter
    /// beyond the indent of the `@`.
    fn parse_trailing_tag_comments(
        &mut self,
        pos: usize,
        end: usize,
        mut margin: isize,
        indent_text: &str,
    ) -> Option<String> {
        if indent_text.is_empty() {
            margin += units(end) - units(pos);
        }
        let initial_margin = js_slice(indent_text, margin);
        self.parse_tag_comments(margin, Some(initial_margin))
    }

    /// Reads the text of a tag up to the next tag, `parseTagComments` of
    /// typescript.js:39320-39422.
    fn parse_tag_comments(
        &mut self,
        mut indent: isize,
        initial_margin: Option<String>,
    ) -> Option<String> {
        let mut pieces = Pieces::default();
        let mut state = State::BeginningOfLine;
        let mut margin: Option<isize> = None;
        if let Some(initial_margin) = initial_margin {
            if !initial_margin.is_empty() {
                push_comment(
                    &mut pieces.comments,
                    &mut margin,
                    &mut indent,
                    initial_margin,
                );
            }
            state = State::SawAsterisk;
        }
        let mut token = self.token();
        loop {
            match token {
                Token::NewLine => {
                    state = State::BeginningOfLine;
                    pieces.comments.push(self.token_text());
                    indent = 0;
                }
                Token::At => {
                    self.reset_token_state(self.cursor.pos - 1);
                    break;
                }
                Token::EndOfFile => break,
                Token::Whitespace => {
                    let whitespace = self.token_text();
                    if let Some(margin) = margin
                        && indent + length(&whitespace) > margin
                    {
                        pieces.comments.push(js_slice(&whitespace, margin - indent));
                        state = State::SavingComments;
                    }
                    indent += length(&whitespace);
                }
                Token::OpenBrace => {
                    state = State::SavingComments;
                    if let Some(link) = self.parse_jsdoc_link() {
                        pieces.link(link);
                    } else {
                        push_comment(
                            &mut pieces.comments,
                            &mut margin,
                            &mut indent,
                            self.token_text(),
                        );
                    }
                }
                Token::Backtick => {
                    state = if state == State::SavingBackticks {
                        State::SavingComments
                    } else {
                        State::SavingBackticks
                    };
                    push_comment(
                        &mut pieces.comments,
                        &mut margin,
                        &mut indent,
                        self.token_text(),
                    );
                }
                Token::Asterisk if state == State::BeginningOfLine => {
                    state = State::SawAsterisk;
                    indent += 1;
                }
                _ => {
                    if state != State::SavingBackticks {
                        state = State::SavingComments;
                    }
                    push_comment(
                        &mut pieces.comments,
                        &mut margin,
                        &mut indent,
                        self.token_text(),
                    );
                }
            }
            token = match state {
                State::SavingComments => self.next_comment_text_token(false),
                State::SavingBackticks => self.next_comment_text_token(true),
                State::BeginningOfLine | State::SawAsterisk => self.next_token_jsdoc(),
            };
        }
        remove_leading_newlines(&mut pieces.comments);
        pieces.finish()
    }

    /// Reads a link at its `{` and returns its rendered text, or nothing
    /// when the brace starts no link, `parseJSDocLink` of
    /// typescript.js:39423-39437.
    fn parse_jsdoc_link(&mut self) -> Option<String> {
        let kind = self.try_parse(Self::parse_jsdoc_link_prefix)?;
        self.next_token_jsdoc();
        self.skip_whitespace();
        let name = self.parse_jsdoc_link_name();
        let mut text = String::new();
        while !matches!(
            self.token(),
            Token::CloseBrace | Token::NewLine | Token::EndOfFile
        ) {
            text.push_str(&self.token_text());
            self.next_token_jsdoc();
        }
        Some(render_link(&kind, name.as_deref(), &text))
    }

    /// Reads the name of a link, `parseJSDocLinkName`, whose identifiers the
    /// grammar scanner reads.
    fn parse_jsdoc_link_name(&mut self) -> Option<String> {
        if self.token() != Token::Identifier {
            return None;
        }
        let start = self.cursor.token_start;
        let mut end = self.cursor.pos;
        self.next_token_regular();
        while self.token() == Token::Dot {
            self.next_token_regular();
            if self.token() == Token::Identifier {
                end = self.cursor.pos;
                self.next_token_regular();
            }
        }
        Some(self.slice(start, end))
    }

    /// Reads `{@link`, `{@linkcode` or `{@linkplain` and returns the kind,
    /// `parseJSDocLinkPrefix`.
    fn parse_jsdoc_link_prefix(&mut self) -> Option<String> {
        self.skip_whitespace_or_asterisk();
        if self.token() == Token::OpenBrace
            && self.next_token_jsdoc() == Token::At
            && self.next_token_jsdoc() == Token::Identifier
        {
            let kind = self.token_text();
            if is_jsdoc_link_tag(&kind) {
                return Some(kind);
            }
        }
        None
    }

    /// Reads an identifier and returns its text, or the empty text of the
    /// missing identifier the reference makes, `parseJSDocIdentifierName`.
    fn parse_jsdoc_identifier_name(&mut self) -> String {
        if self.token() != Token::Identifier {
            return String::new();
        }
        let text = self.token_text();
        self.next_token_jsdoc();
        text
    }

    /// Reads a braced type expression when one follows, and returns its
    /// source text, `tryParseTypeExpression`.
    fn try_parse_type_expression(&mut self) -> Option<String> {
        self.skip_whitespace_or_asterisk();
        (self.token() == Token::OpenBrace).then(|| self.parse_jsdoc_type_expression())
    }

    /// Reads the braces of a type expression with their nesting and returns
    /// the source text, the braced form of `parseJSDocTypeExpression`, which
    /// moves on with the `JSDoc` scanner after the closing brace.
    fn parse_jsdoc_type_expression(&mut self) -> String {
        let start = self.cursor.token_start;
        let mut pos = self.cursor.pos;
        let mut depth = 1_u32;
        while pos < self.text.len() && depth > 0 {
            match self.unit(pos) {
                0x7B => depth += 1,
                0x7D => depth -= 1,
                _ => {}
            }
            pos += 1;
        }
        self.cursor.pos = pos;
        let text = self.slice(start, pos);
        self.next_token_jsdoc();
        text
    }

    /// Reads a parameter tag, `parseParameterOrPropertyTag` of
    /// typescript.js:39517-39540.
    fn parse_parameter_or_property_tag(
        &mut self,
        start: usize,
        tag_name: String,
        indent: isize,
    ) -> Tag {
        let is_name_first = self.try_parse_type_expression().is_none();
        self.skip_whitespace_or_asterisk();
        let (name, is_identifier) = self.parse_bracket_name_in_property_and_param_tag();
        let indent_text = self.skip_whitespace_or_asterisk();
        if is_name_first && self.look_ahead(Self::parse_jsdoc_link_prefix).is_none() {
            self.try_parse_type_expression();
        }
        let comment =
            self.parse_trailing_tag_comments(start, self.cursor.full_start, indent, &indent_text);
        Tag {
            name: tag_name,
            comment,
            kind: TagKind::Parameter {
                name,
                is_identifier,
            },
        }
    }

    /// Reads the name of a parameter tag, bracketed or not, and returns its
    /// source text and whether it is one identifier,
    /// `parseBracketNameInPropertyAndParamTag` of typescript.js:39486-39505.
    ///
    /// The default of a bracketed name is an expression of the grammar, which
    /// this port skips up to the closing bracket.
    fn parse_bracket_name_in_property_and_param_tag(&mut self) -> (String, bool) {
        let is_bracketed = self.parse_optional_jsdoc(Token::OpenBracket);
        if is_bracketed {
            self.skip_whitespace();
        }
        let is_backquoted = self.parse_optional_jsdoc(Token::Backtick);
        let name = self.parse_jsdoc_entity_name();
        if is_backquoted {
            self.parse_optional_jsdoc(Token::Backtick);
        }
        if is_bracketed {
            self.skip_whitespace();
            if self.token() == Token::Equals {
                self.skip_to_close_bracket();
            }
            if self.token() == Token::CloseBracket {
                self.next_token_regular();
            }
        }
        name
    }

    /// Moves to the `]` that closes a bracketed parameter name, over a
    /// default whose brackets, braces and parentheses nest.
    fn skip_to_close_bracket(&mut self) {
        let mut pos = self.cursor.pos;
        let mut depth = 0_u32;
        while pos < self.text.len() {
            match self.unit(pos) {
                0x28 | 0x5B | 0x7B => depth += 1,
                0x5D if depth == 0 => break,
                0x29 | 0x5D | 0x7D => depth = depth.saturating_sub(1),
                _ => {}
            }
            pos += 1;
        }
        self.cursor.pos = pos;
        self.next_token_jsdoc();
    }

    /// Reads a name with its dotted parts and returns its source text and
    /// whether it is one identifier, `parseJSDocEntityName` of
    /// typescript.js:39944-39957.
    fn parse_jsdoc_entity_name(&mut self) -> (String, bool) {
        let start = self.cursor.token_start;
        let first = self.parse_jsdoc_identifier_name();
        let mut end = start + first.encode_utf16().count();
        self.skip_empty_brackets();
        let mut is_identifier = true;
        while self.token() == Token::Dot {
            self.next_token_regular();
            let part_start = self.cursor.token_start;
            let part = self.parse_jsdoc_identifier_name();
            end = part_start + part.encode_utf16().count();
            self.skip_empty_brackets();
            is_identifier = false;
        }
        (self.slice(start, end), is_identifier)
    }

    /// Skips the `[]` an entity name may carry after a part.
    fn skip_empty_brackets(&mut self) {
        if self.token() == Token::OpenBracket {
            self.next_token_regular();
            if self.token() == Token::CloseBracket {
                self.next_token_regular();
            }
        }
    }

    /// Reads a `@see` tag, `parseSeeTag` of typescript.js:39571-39576.
    fn parse_see_tag(
        &mut self,
        start: usize,
        tag_name: String,
        indent: isize,
        indent_text: &str,
    ) -> Tag {
        let is_markdown_or_jsdoc_link = self.token() == Token::OpenBracket
            || self.look_ahead(|parser| {
                parser.next_token_jsdoc() == Token::At
                    && parser.next_token_jsdoc() == Token::Identifier
                    && is_jsdoc_link_tag(&parser.token_text())
            });
        let name = (!is_markdown_or_jsdoc_link).then(|| self.parse_jsdoc_name_reference());
        self.tag_with_comments(start, tag_name, indent, indent_text, TagKind::See(name))
    }

    /// Reads the name a `@see` tag refers to and returns its source text,
    /// braces included, `parseJSDocNameReference` of
    /// typescript.js:38975-38990, whose name the grammar scanner reads.
    fn parse_jsdoc_name_reference(&mut self) -> String {
        let start = self.cursor.token_start;
        let has_brace = self.token() == Token::OpenBrace;
        if has_brace {
            self.next_token_regular();
        }
        let mut end = self.cursor.token_start;
        if self.token() == Token::Identifier {
            end = self.cursor.pos;
            self.next_token_regular();
            while self.token() == Token::Dot {
                self.next_token_regular();
                if self.token() == Token::Identifier {
                    end = self.cursor.pos;
                    self.next_token_regular();
                }
            }
        }
        if has_brace && self.token() == Token::CloseBrace {
            end = self.cursor.pos;
            self.next_token_jsdoc();
        }
        self.slice(start, end)
    }
}
