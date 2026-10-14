// SPDX-License-Identifier: MIT
/*
 * aoviewer-core sort: the one order of names the core sorts by.
 *
 * Schema order is kept where order has meaning, and everything else sorts by
 * the bytes of the name with an explicit tie break.
 */

//! The name order.

use core::cmp::Ordering;

use crate::model::{
    Model,
    TypeId, //
};

/// Compares two types by the byte order of their UTF-8 names, then by kind,
/// then by id.
///
/// The model numbers its names in byte order, so the comparison of two
/// [`Name`](crate::model::Name) values is the comparison of their bytes. Type
/// names are unique in a finished model, and the two tie breaks keep the
/// order total all the same.
///
/// # Panics
///
/// Panics when `a` or `b` comes from another model.
#[must_use]
pub fn name_order(model: &Model, a: TypeId, b: TypeId) -> Ordering {
    let (ta, tb) = (model.ty(a), model.ty(b));
    ta.name
        .cmp(&tb.name)
        .then(ta.kind.cmp(&tb.kind))
        .then(a.cmp(&b))
}
