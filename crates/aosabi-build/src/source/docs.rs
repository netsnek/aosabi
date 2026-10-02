// SPDX-License-Identifier: MIT
/*
 * aosabi-build: the documentation of an item of a plain crate, read from its
 * doc attributes by the rule of docs/authoring.md section 29.5.
 */

//! The documentation of an item of a plain crate.
//!
//! The source path reads a comment as the derive path reads it, so a doc
//! comment gives a plain crate the description its twin of the derive path
//! prints. The reading goes in the three steps of
//! `crates/aosabi-macros/src/docs.rs`.
//!
//! 1. The attributes form blocks. A run of `///` lines on consecutive source
//!    lines is one block, and each `/** */` comment is one. Only the last
//!    block counts, as only the last `JSDoc` does.
//! 2. The last block becomes the text a `JSDoc` comment would hold. A `/** */`
//!    block is that text already, with the column of its `/**`, and a run of
//!    `///` lines becomes a `JSDoc` whose lines each open with ` *`.
//! 3. The parser of `jsdoc` reads that text into a comment and tags, and the
//!    composition below gives the comment and the tags of the item, or of one
//!    of its parameters.
//!
//! The reader keeps the form and the place of every doc attribute, which a
//! macro reads from the spans rustc hands it, so the blocks form alike. A
//! `#[doc = "..."]` written out is read as `///` lines. A parameter carries no
//! doc comment in plain Rust, since rustc refuses one outside an attribute
//! macro, so the comment of an argument comes from the `@param` tags of its
//! method alone.

use super::{
    jsdoc::{
        self,
        Tag, //
    },
    model::{
        DocSpan,
        Docs, //
    },
};

/// The comment and the tags of an item, the two halves the checker answers
/// apart, with the lines of its last block as written.
#[derive(Clone, Debug, Default)]
pub(crate) struct Documentation {
    /// The comment, `displayPartsToString(symbol.getDocumentationComment())`.
    pub(crate) comment: String,
    /// The tags, `symbol.getJsDocTags()`.
    pub(crate) tags: Vec<Tag>,
    /// The lines of the last block as written, the `docs` of `authoring.json`.
    pub(crate) lines: Vec<String>,
    /// The number of blocks the item carries.
    pub(crate) blocks: usize,
}

/// One documentation block before it is parsed.
enum RawBlock {
    /// A run of `///` lines, with the source line of the last one.
    Lines {
        /// The text of each line after its slashes.
        lines: Vec<String>,
        /// The source line of the last line of the run.
        last_line: usize,
    },
    /// One `/** */` comment.
    Comment {
        /// The text between `/**` and `*/`.
        body: String,
        /// The zero based column of `/**`.
        column: usize,
    },
}

impl RawBlock {
    /// Returns the lines of the block as written, the way the sidecar reads
    /// the lines of a `JSDoc` block for check C2.
    ///
    /// A `///` line loses one space after its slashes, and a `/** */` line
    /// its gutter against the margin of its first text. Every line loses its
    /// trailing spaces and tabs, and the trailing blank lines go.
    fn written_lines(&self) -> Vec<String> {
        let mut lines = match self {
            Self::Lines { lines, .. } => lines
                .iter()
                .map(|line| line.strip_prefix(' ').unwrap_or(line).to_owned())
                .collect(),
            Self::Comment { body, column } => comment_lines(body, *column),
        };
        for line in &mut lines {
            line.truncate(line.trim_end_matches([' ', '\t']).len());
        }
        while lines.last().is_some_and(String::is_empty) {
            lines.pop();
        }
        lines
    }

    /// Parses the block into the comment and the tags of the reference.
    fn parse(&self) -> jsdoc::Block {
        match self {
            Self::Lines { lines, .. } => {
                let mut body = String::from("\n");
                for line in lines {
                    body.push_str(" *");
                    body.push_str(line);
                    body.push('\n');
                }
                body.push(' ');
                jsdoc::parse(&body, 0)
            }
            Self::Comment { body, column } => jsdoc::parse(body, *column),
        }
    }
}

/// Returns the documentation of the item whose doc attributes are `docs`.
pub(crate) fn item(docs: &Docs) -> Documentation {
    let blocks = blocks(docs);
    let Some(last) = blocks.last() else {
        return Documentation::default();
    };
    let block = last.parse();
    Documentation {
        comment: own_comment(&block).unwrap_or_default(),
        tags: block.tags,
        lines: last.written_lines(),
        blocks: blocks.len(),
    }
}

/// Returns the documentation of the parameter whose GraphQL name is `name`,
/// in the method documented by `method`.
///
/// The comment is the comment of each `@param` tag of the method that names
/// the parameter, joined by line breaks, with a part the list holds already
/// left out, and the tags are those `@param` tags. A tag names the parameter
/// as the schema spells it, `@param maxDigits` for the parameter `max_digits`,
/// since the line stands in the description of the method as it is written and
/// the inference matches the name of the tag with the name of the argument.
pub(crate) fn parameter(name: &str, method: &Documentation) -> Documentation {
    let named: Vec<&Tag> = method
        .tags
        .iter()
        .filter(|tag| tag.parameter_name() == Some(name))
        .collect();
    let mut parts: Vec<String> = Vec::new();
    for part in named.iter().filter_map(|tag| tag.comment.clone()) {
        if !parts.contains(&part) {
            parts.push(part);
        }
    }
    Documentation {
        comment: parts.join("\n"),
        tags: named.into_iter().cloned().collect(),
        lines: Vec::new(),
        blocks: 0,
    }
}

/// Returns the display text a `JSDoc` block contributes to the comment, or
/// nothing when it contributes none, the loop body of
/// `getJsDocCommentsFromDeclarations` at typescript.js:175781-175792.
///
/// A block with no comment contributes only through `@inheritDoc`, whose own
/// comment follows the comment of the block with no separator.
fn own_comment(block: &jsdoc::Block) -> Option<String> {
    let inherit_doc = block.tags.iter().find(|tag| tag.is_inherit_doc());
    if block.comment.is_none() && inherit_doc.is_none() {
        return None;
    }
    let mut text = block.comment.clone().unwrap_or_default();
    if let Some(comment) = inherit_doc.and_then(|tag| tag.comment.as_deref()) {
        text.push_str(comment);
    }
    Some(text)
}

/// Returns the documentation blocks of `docs` in source order.
fn blocks(docs: &Docs) -> Vec<RawBlock> {
    let mut blocks: Vec<RawBlock> = Vec::new();
    for (text, span) in docs.lines.iter().zip(&docs.spans) {
        add_to_blocks(&mut blocks, text, *span);
    }
    blocks
}

/// Adds the doc text `value` at `span` to `blocks`, joining the run of `///`
/// lines the last block holds when the text stands on the next source line.
fn add_to_blocks(blocks: &mut Vec<RawBlock>, value: &str, span: DocSpan) {
    if span.block {
        blocks.push(RawBlock::Comment {
            body: String::from(value),
            column: span.column,
        });
        return;
    }
    if let Some(RawBlock::Lines { lines, last_line }) = blocks.last_mut()
        && span.line <= *last_line + 1
    {
        lines.extend(value.split('\n').map(String::from));
        *last_line = span.line;
        return;
    }
    blocks.push(RawBlock::Lines {
        lines: value.split('\n').map(String::from).collect(),
        last_line: span.line,
    });
}

/// Returns the lines of a `/** */` block whose `/**` stands at the zero based
/// `column`, each without its gutter, `jsDocLines` of the sidecar.
///
/// The first line starts at the column after `/** `, and its leading
/// whitespace goes. A later line loses its leading whitespace, one `*` and
/// the whitespace after it, and keeps whatever of both lies beyond the margin,
/// which is the indent of the first text. The opening line goes when it
/// carries nothing.
fn comment_lines(body: &str, column: usize) -> Vec<String> {
    let mut margin: Option<usize> = None;
    let mut lines: Vec<String> = body
        .split('\n')
        .enumerate()
        .map(|(index, raw)| {
            let raw = raw.strip_suffix('\r').unwrap_or(raw);
            let (text, indent, kept) = if index == 0 {
                (
                    raw.trim_start_matches([' ', '\t']),
                    column + 4,
                    String::new(),
                )
            } else {
                gutter(raw, margin)
            };
            if !text.is_empty() && margin.is_none() {
                margin = Some(indent);
            }
            format!("{kept}{text}")
        })
        .collect();
    if lines.first().is_some_and(String::is_empty) {
        lines.remove(0);
    }
    lines
}

/// Splits a later line of a `/** */` block into its text, the indent of that
/// text and the whitespace beyond `margin` it keeps.
fn gutter(raw: &str, margin: Option<usize>) -> (&str, usize, String) {
    let rest = raw.trim_start_matches([' ', '\t']);
    let lead = raw.len() - rest.len();
    let mut kept = String::new();
    if let Some(margin) = margin
        && lead > margin
    {
        kept.push_str(&raw[margin..lead]);
    }
    let Some(after) = rest.strip_prefix('*') else {
        return (rest, lead, kept);
    };
    let indent = lead + 1;
    let text = after.trim_start_matches([' ', '\t']);
    let gap = after.len() - text.len();
    if let Some(margin) = margin
        && indent + gap > margin
    {
        // `gap.slice(margin - indent)` of the sidecar, where a negative start
        // counts from the end, ECMA-262 section 22.1.3.25.
        let from = if margin >= indent {
            margin - indent
        } else {
            gap.saturating_sub(indent - margin)
        };
        kept.push_str(&after[from.min(gap)..gap]);
    }
    (text, indent + gap, kept)
}
