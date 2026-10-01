// SPDX-License-Identifier: MIT
/*
 * aosabi-macros: the documentation of a described item, read from its doc
 * attributes by the rule of docs/authoring.md section 29.5.
 */

//! The documentation of a described item.
//!
//! A `///` line reaches a macro as `#[doc = "..."]` with the text after the
//! slashes, and a `/** */` comment as one such attribute with the text between
//! the delimiters, whose span starts at the `/**`. The reading goes in three
//! steps.
//!
//! 1. The attributes form blocks. A run of `///` lines on consecutive source
//!    lines is one block, and each `/** */` comment is one. Only the last block
//!    counts, as only the last `JSDoc` does.
//! 2. The last block becomes the text a `JSDoc` comment would hold. A `/** */`
//!    block is that text already, and its `/**` column is read from the span
//!    of its literal. A run of `///` lines becomes a `JSDoc` whose lines each
//!    open with ` *`, which puts every line behind the same gutter, so the
//!    margin is the column where the text of the first non blank line begins.
//! 3. The port of the `JSDoc` parser in `jsdoc` reads that text into a comment
//!    and tags, and the composition below gives the comment and the tags of
//!    the item, or of one of its parameters.
//!
//! A `#[doc = "..."]` written out is read as `///` lines. A `#[doc]` whose value
//! is no string literal and a `#[deprecated]` are refused on a described item.

use proc_macro2::{
    Span,
    TokenStream, //
};
use quote::quote;
use syn::{
    Attribute,
    Expr,
    ExprLit,
    Lit,
    LitStr,
    Meta,
    spanned::Spanned, //
};

use crate::{
    jsdoc::{
        self,
        Tag, //
    },
    refuse::{
        DEPRECATED_ON_DESCRIBED,
        DOC_NOT_LITERAL,
        refusal, //
    },
};

/// The comment and the tags of a described item, the two halves the checker
/// answers apart, with the lines of its last block as written.
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

impl Documentation {
    /// Returns the `Docs` literal of the descriptor.
    pub(crate) fn to_tokens(&self) -> TokenStream {
        let comment = &self.comment;
        let tags = self.tags.iter().map(|tag| {
            let name = &tag.name;
            let text = tag.text();
            quote! {
                ::aosabi::descriptor::DocTag {
                    name: #name,
                    text: #text,
                }
            }
        });
        quote! {
            ::aosabi::descriptor::Docs {
                comment: #comment,
                tags: &[#(#tags),*],
            }
        }
    }

    /// Returns the `Site` literal of the item whose name stands at `span`.
    pub(crate) fn site_tokens(&self, span: Span) -> TokenStream {
        let line = u32::try_from(span.unwrap().line()).unwrap_or(u32::MAX);
        let lines = &self.lines;
        let blocks = u32::try_from(self.blocks).unwrap_or(u32::MAX);
        quote! {
            ::aosabi::descriptor::Site {
                file: ::core::file!(),
                line: #line,
                docs: &[#(#lines),*],
                blocks: #blocks,
            }
        }
    }
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
    /// Returns the lines of the block as written, the way the sidecar reads the
    /// lines of a `JSDoc` block for check C2.
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

/// Removes every doc attribute from `attrs` and returns them in order.
///
/// The compiler refuses a doc comment on a function parameter, and a macro that
/// takes it off before it emits the function makes the form legal.
pub(crate) fn take(attrs: &mut Vec<Attribute>) -> Vec<Attribute> {
    let (taken, kept) = core::mem::take(attrs)
        .into_iter()
        .partition(|attr| attr.path().is_ident("doc"));
    *attrs = kept;
    taken
}

/// Returns the documentation of the described item that carries `attrs`.
///
/// # Errors
///
/// Returns the refusal of a `#[deprecated]` and of a `#[doc]` whose value is
/// no string literal.
pub(crate) fn item(attrs: &[Attribute]) -> syn::Result<Documentation> {
    if let Some(attr) = attrs.iter().find(|attr| attr.path().is_ident("deprecated")) {
        return Err(refusal(attr.span(), DEPRECATED_ON_DESCRIBED));
    }
    let blocks = blocks(attrs)?;
    let Some(last) = blocks.last() else {
        return Ok(Documentation::default());
    };
    let block = last.parse();
    Ok(Documentation {
        comment: own_comment(&block).unwrap_or_default(),
        tags: block.tags,
        lines: last.written_lines(),
        blocks: blocks.len(),
    })
}

/// Returns the documentation of the parameter `name` that carries `attrs`,
/// in the method documented by `method`.
///
/// The comment is the one written on the parameter, then the comment of each
/// `@param` tag of the method that names it, joined by line breaks, with a part
/// the list holds already left out. The tags are those of the comment on the
/// parameter, then those `@param` tags.
///
/// # Errors
///
/// Returns the refusal of a `#[doc]` whose value is no string literal.
pub(crate) fn parameter(
    attrs: &[Attribute],
    name: &str,
    method: &Documentation,
) -> syn::Result<Documentation> {
    let blocks = blocks(attrs)?;
    let own = blocks.last().map(RawBlock::parse);
    let named: Vec<&Tag> = method
        .tags
        .iter()
        .filter(|tag| tag.parameter_name() == Some(name))
        .collect();
    let mut parts: Vec<String> = Vec::new();
    let own_parts = own.as_ref().and_then(own_comment);
    let tag_parts = named.iter().filter_map(|tag| tag.comment.clone());
    for part in own_parts.into_iter().chain(tag_parts) {
        if !parts.contains(&part) {
            parts.push(part);
        }
    }
    let mut tags = own.map(|block| block.tags).unwrap_or_default();
    tags.extend(named.into_iter().cloned());
    Ok(Documentation {
        comment: parts.join("\n"),
        tags,
        lines: blocks
            .last()
            .map(RawBlock::written_lines)
            .unwrap_or_default(),
        blocks: blocks.len(),
    })
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

/// Returns the documentation blocks of `attrs` in source order.
///
/// # Errors
///
/// Returns the refusal of a `#[doc]` whose value is no string literal.
fn blocks(attrs: &[Attribute]) -> syn::Result<Vec<RawBlock>> {
    let mut blocks: Vec<RawBlock> = Vec::new();
    for attr in attrs.iter().filter(|attr| attr.path().is_ident("doc")) {
        // `#[doc(hidden)]` and its kin carry no text.
        let Meta::NameValue(meta) = &attr.meta else {
            continue;
        };
        let Expr::Lit(ExprLit {
            lit: Lit::Str(text),
            ..
        }) = &meta.value
        else {
            return Err(refusal(meta.value.span(), DOC_NOT_LITERAL));
        };
        add_to_blocks(&mut blocks, text);
    }
    Ok(blocks)
}

/// Adds the doc literal `text` to `blocks`, joining the run of `///` lines
/// the last block holds when the literal stands on the next source line.
fn add_to_blocks(blocks: &mut Vec<RawBlock>, text: &LitStr) {
    let span = text.span().unwrap();
    let value = text.value();
    if span
        .source_text()
        .is_some_and(|source| source.starts_with("/**"))
    {
        // The column of a span counts from 1.
        blocks.push(RawBlock::Comment {
            body: value,
            column: span.column().saturating_sub(1),
        });
        return;
    }
    let line = span.line();
    if let Some(RawBlock::Lines { lines, last_line }) = blocks.last_mut()
        && line <= *last_line + 1
    {
        lines.extend(value.split('\n').map(String::from));
        *last_line = line;
        return;
    }
    blocks.push(RawBlock::Lines {
        lines: value.split('\n').map(String::from).collect(),
        last_line: line,
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
