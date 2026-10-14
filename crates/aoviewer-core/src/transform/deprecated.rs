// SPDX-License-Identifier: MIT
/*
 * aoviewer-core transform deprecated: the deprecation rule by directive and by
 * description tag.
 *
 * The rule that a deprecation filter never deletes a type it empties is a
 * translation of graphql-voyager src/introspection/introspection.ts:208-250 at
 * commit 656b8ddb4e7bec958d7300e2d737f632c7b5ba30, MIT, copyright
 * IvanGoncharov, whose notice travels in NOTICE section 4. A description line
 * that starts with the tag marks its member.
 */

//! The deprecation marks.
//!
//! A member the schema deprecates by directive carries [`Marked::Directive`]
//! with the reason of the directive. A build emits no deprecation directive and
//! keeps the tag as description text, so under `descdep=1` a member whose
//! description has a line starting with `@deprecated` carries [`Marked::Tag`],
//! with the rest of that line as its reason. The directive wins over the tag,
//! because the schema states it.
//!
//! Spaces and tabs before the tag are skipped. The reading of a block string
//! keeps the indent of its first line when a later line has none, so a tag on
//! the first line of a longer description reaches a client behind two spaces.
//!
//! The marks are the whole rule, and no type is ever deleted by it, not even
//! one whose every field the graph hides, because a schema may carry the
//! directive inconsistently and a live field may still point at a type whose
//! fields are all deprecated. [󰀀](super::Transformed::hides) applies the
//! filter to the members in the graph alone, and the documents list every
//! member with its mark.
//!
//! The marks cover the fields and input fields, the arguments of fields and
//! the enum values. An argument of a directive has no element id, so it
//! carries no mark.
//!
//! [introspection.ts]: https://github.com/APIs-guru/graphql-voyager/blob/656b8ddb4e7bec958d7300e2d737f632c7b5ba30/src/introspection/introspection.ts

use alloc::{
    string::String,
    vec::Vec, //
};

use super::{
    Marked,
    Member, //
};
use crate::model::{
    Deprecation,
    Model, //
};

/// The tag that marks a member when a description line starts with it.
const TAG: &str = "@deprecated";

/// The blanks skipped before the tag, the white space an indent is made of,
/// a space or a tab.
///
/// A block string keeps the indent of its first line, and graphql-js counts an
/// indent in these two characters alone.
const INDENT: [char; 2] = [' ', '\t'];

/// The blanks that may end the tag and that are trimmed around the reason.
const BLANKS: [char; 3] = [' ', '\t', '\r'];

/// Returns the mark of every deprecated member of `model`, sorted by member.
///
/// A description tag counts only when `descdep` holds.
pub(super) fn marks(model: &Model, descdep: bool) -> Vec<(Member, Marked)> {
    let mut marks = Vec::new();
    let mut push = |member, deprecation: Option<&Deprecation>, description: Option<&str>| {
        if let Some(marked) = mark(deprecation, description, descdep) {
            marks.push((member, marked));
        }
    };
    for &t in model.types() {
        let ty = model.ty(t);
        for &f in &ty.fields {
            let field = model.field(f);
            push(
                Member::Field(f),
                field.deprecation.as_ref(),
                field.description.as_deref(),
            );
            for &a in &field.args {
                let arg = model.arg(a);
                push(
                    Member::Arg(a),
                    arg.deprecation.as_ref(),
                    arg.description.as_deref(),
                );
            }
        }
        for &v in &ty.values {
            let value = model.value(v);
            push(
                Member::Value(v),
                value.deprecation.as_ref(),
                value.description.as_deref(),
            );
        }
    }
    marks.sort_unstable_by_key(|&(member, _)| member);
    marks
}

/// Returns the mark of one member from its directive and its description.
fn mark(
    deprecation: Option<&Deprecation>,
    description: Option<&str>,
    descdep: bool,
) -> Option<Marked> {
    if let Some(d) = deprecation {
        return Some(Marked::Directive {
            reason: d.reason.clone(),
        });
    }
    if !descdep {
        return None;
    }
    tag(description?)
}

/// Returns the tag mark of the first line of `description` that starts with
/// the tag, or `None` when no line does.
///
/// The tag has to end at the end of its line or at a blank, so
/// `@deprecatedSince` is no tag. The rest of the line without its blanks is
/// the reason, and an empty rest gives none.
fn tag(description: &str) -> Option<Marked> {
    description.split('\n').find_map(|line| {
        let rest = line.trim_start_matches(INDENT).strip_prefix(TAG)?;
        if !rest.is_empty() && !rest.starts_with(BLANKS) {
            return None;
        }
        let reason = rest.trim_matches(BLANKS);
        Some(Marked::Tag {
            reason: (!reason.is_empty()).then(|| String::from(reason)),
        })
    })
}
