// SPDX-License-Identifier: MIT
/*
 * aoviewer-core transform inherited: the detection of fields an object inherits
 * from an interface.
 *
 * The inherited interface fields are one of the transforms before the graph.
 */

//! The inherited fields.
//!
//! An implementation lists the fields it inherits greyed with the interface
//! they come from, and the graph folds them. A field of an object or of an
//! interface is inherited when an interface the type implements declares a
//! field of the same name. An interface can implement an interface, so both
//! kinds of holder are read.
//!
//! When several interfaces declare the name, the field comes from the first
//! of them in the order the type lists its interfaces. The interface `Node`
//! that the Relay collapse strips is taken only when no other interface
//! declares the name, so a field the graph folds names an interface the graph
//! still draws where the schema offers one.
//!
//! [specification]: https://spec.graphql.org/October2021/

use alloc::{
    vec,
    vec::Vec, //
};

use crate::model::{
    FieldId,
    Kind,
    Model,
    Name,
    TypeId, //
};

/// Returns every inherited field of `model` with the interface it comes
/// from, sorted by field.
///
/// `stripped` is the interface the Relay collapse strips from every type, if
/// any.
pub(super) fn inherited(model: &Model, stripped: Option<TypeId>) -> Vec<(FieldId, TypeId)> {
    let names = interface_fields(model);
    let mut out = Vec::new();
    for &t in model.types() {
        let ty = model.ty(t);
        if !matches!(ty.kind, Kind::Object | Kind::Interface) || ty.implements.is_empty() {
            continue;
        }
        out.extend(ty.fields.iter().filter_map(|&f| {
            let from = source(&names, &ty.implements, model.field(f).name, stripped)?;
            Some((f, from))
        }));
    }
    out.sort_unstable();
    out
}

/// Returns the field names of every interface of `model`, sorted, indexed by
/// type, and no names for a type of another kind.
fn interface_fields(model: &Model) -> Vec<Vec<Name>> {
    let mut names = vec![Vec::new(); model.types().len()];
    for &t in model.types() {
        let ty = model.ty(t);
        if ty.kind != Kind::Interface {
            continue;
        }
        let list = &mut names[t.index()];
        list.extend(ty.fields.iter().map(|&f| model.field(f).name));
        list.sort_unstable();
    }
    names
}

/// Returns the interface among `implements` that the field `name` comes
/// from, or `None` when none declares it.
fn source(
    names: &[Vec<Name>],
    implements: &[TypeId],
    name: Name,
    stripped: Option<TypeId>,
) -> Option<TypeId> {
    let declares = |i: &&TypeId| names[i.index()].binary_search(&name).is_ok();
    implements
        .iter()
        .filter(declares)
        .find(|&&i| Some(i) != stripped)
        .or_else(|| implements.iter().find(declares))
        .copied()
}
