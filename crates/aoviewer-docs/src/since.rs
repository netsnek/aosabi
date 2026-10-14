// SPDX-License-Identifier: MIT
/*
 * aoviewer-docs since: the old schema of --since and the text change log.
 *
 * The old schema loads through the same loader and is compared through the
 * difference of the core.
 */

//! The change log input.
//!
//! The flag `--since` names an older schema in any of the four inputs.
//!
//! Both models are compared as the inputs describe them, before any
//! transform of the views, so the change log lists a member the options of
//! the views hide.

use aoviewer_core::{
    diff::{
        Difference,
        diff, //
    },
    limits::ReadLimits,
    model::Model, //
};

use crate::{
    error::DocsError,
    input::{
        Input,
        load, //
    },
};

/// Returns every difference from the schema of `since` to `new`, in the order
/// of the ids, with the old schema read under `limits`.
///
/// The old schema is the first model of [`diff`] and `new` the second, so a
/// type only `new` declares is [`Change::Added`].
///
/// # Errors
///
/// Returns [`DocsError::Since`] with the refusal of [`load`] inside it when
/// the schema of `since` does not load, whichever code that refusal has.
///
/// [`Change::Added`]: aoviewer_core::diff::Change::Added
pub fn changes(
    since: &Input,
    new: &Model,
    limits: &ReadLimits,
) -> Result<Vec<Difference>, DocsError> {
    let old = load(since, limits).map_err(|e| DocsError::Since(Box::new(e)))?;
    let mut out = Vec::new();
    diff(&old.model, new, &mut out);
    Ok(out)
}
