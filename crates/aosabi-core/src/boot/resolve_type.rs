// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-core boot resolve_type: the type resolver of every union and
 * interface of a model.
 *
 * Derived from Pylon 2.0.0-canary-20260206115650 and 2.9.5 by Nico Schett,
 * packages/pylon/src/cli/builder/schema/schema-parser.ts and
 * @getcronit/pylon-dev 2.0.0-canary-20260206115650, Apache-2.0.
 * Changed by the Aosabi authors.
 */

//! The type resolvers of a model.
//!
//! The build writes the type resolver of each union and of each interface
//! with fields as the text of a function into `resolvers.js`, the unions
//! first, and an interface with no field takes none, since the SDL leaves it
//! out. [`rules`] makes the model of each function, the [`ResolveTypeRule`]
//! that [`crate::glue::resolve_type`] evaluates as the text evaluates: the
//! members with more fields first, then for each member the check of its
//! required fields no other member has, any of which names it, or else the
//! check of all its fields, and no check for a member with no field.
//!
//! The rules of a boot are made once per instance and live as long as it, so
//! their texts are leaked into the `'static` form the twins build at compile
//! time, which [`crate::HandlerOptions`] takes.
//!
//! A canary build writes its type resolvers in another form, each
//! member in the order of the model sorted by its count of fields, named by
//! the check that every one of its fields is `in` the value, with no read of
//! `__typename`. With the feature `canary-types`, `canary_rules` puts the
//! rule of each such resolver the host read in place of the rule the model
//! gives its type.

use super::ir::{
    Ir,
    Type, //
};
use crate::glue::resolve_type::{
    ResolveTypeCheck,
    ResolveTypeRule, //
};

/// Returns the type resolver of every union and of every interface with
/// fields of `ir`, the unions first, as `getResolvers` writes them.
#[must_use]
pub fn rules(ir: &Ir) -> Vec<ResolveTypeRule> {
    let unions = ir.unions.iter().map(|union| {
        let members = ir
            .objects
            .iter()
            .filter(|object| union.members.contains(&object.name))
            .collect();
        rule(&union.name, members)
    });
    let interfaces = ir
        .interfaces
        .iter()
        .filter(|interface| !interface.fields.is_empty())
        .map(|interface| {
            let members = ir
                .objects
                .iter()
                .filter(|object| object.implements.contains(&interface.name))
                .collect();
            rule(&interface.name, members)
        });
    unions.chain(interfaces).collect()
}

/// Returns the type resolver of the abstract type `name` over its members
/// `members`, in the order of the model.
///
/// The sort is stable, as `Array.prototype.sort` is, so two members with as
/// many fields keep the order of the model.
fn rule(name: &str, mut members: Vec<&Type>) -> ResolveTypeRule {
    members.sort_by_key(|member| core::cmp::Reverse(member.fields.len()));
    let checks: Vec<ResolveTypeCheck> = members
        .iter()
        .filter_map(|member| check(member, &members))
        .collect();
    let valid_names: Vec<&'static str> = members.iter().map(|member| leak(&member.name)).collect();
    ResolveTypeRule {
        type_name: leak(name),
        valid_names: valid_names.leak(),
        checks: checks.leak(),
    }
}

/// Returns the check of the member `member` among `members`, or `None` for
/// a member with no field.
fn check(member: &Type, members: &[&Type]) -> Option<ResolveTypeCheck> {
    let others = |field: &str| {
        members
            .iter()
            .filter(|other| other.name != member.name)
            .any(|other| other.fields.iter().any(|own| own.name == field))
    };
    let unique: Vec<&'static str> = member
        .fields
        .iter()
        .filter(|field| field.required && !others(&field.name))
        .map(|field| leak(&field.name))
        .collect();
    let name = leak(&member.name);
    if !unique.is_empty() {
        return Some(ResolveTypeCheck::AnyField {
            fields: unique.leak(),
            name,
        });
    }
    if member.fields.is_empty() {
        return None;
    }
    let fields: Vec<&'static str> = member
        .fields
        .iter()
        .map(|field| leak(&field.name))
        .collect();
    Some(ResolveTypeCheck::AllFields {
        fields: fields.leak(),
        name,
    })
}

/// Returns `text` as a string that lives as long as the module.
fn leak(text: &str) -> &'static str {
    Box::leak(Box::<str>::from(text))
}

/// The type resolver of one union or interface of a canary build,
/// as the host read it off the function the glue hands `handler()`: for each
/// member in order its name and the fields its check tests with `in`.
#[cfg(feature = "canary-types")]
#[derive(Clone, Debug)]
pub struct CanaryResolver {
    /// The union or the interface.
    pub type_name: String,
    /// The members in the order of the checks, each with its fields.
    pub members: Vec<(String, Vec<String>)>,
}

/// Puts the rule of each resolver of `canary` in place of the rule of its
/// type among `rules`, or after them for a type `rules` holds no rule of.
///
/// The rule names no member, so no `__typename` decides, and checks for each
/// member in order that each of its fields is held and is not `undefined`,
/// which over the projection [`crate::glue::resolve_type::hold_names`] fills
/// is the check of `in` alone of the canary.
///
/// # Errors
///
/// Returns a message that names a type of `canary` that is no union and no
/// interface of `ir`, or a member that is no member of its type there.
#[cfg(feature = "canary-types")]
pub fn canary_rules(
    ir: &Ir,
    canary: &[CanaryResolver],
    rules: &mut Vec<ResolveTypeRule>,
) -> Result<(), String> {
    for resolver in canary {
        let name = resolver.type_name.as_str();
        let members = members_of(ir, name).ok_or_else(|| {
            format!(
                "the type resolver of the build resolves the type {name}, which is no union and no interface of the IR"
            )
        })?;
        if let Some((member, _)) = resolver
            .members
            .iter()
            .find(|(member, _)| !members.contains(&member.as_str()))
        {
            return Err(format!(
                "the type resolver of the build resolves {name} to {member}, which is no member of it in the IR"
            ));
        }
        let checks: Vec<ResolveTypeCheck> = resolver
            .members
            .iter()
            .map(|(member, fields)| {
                let fields: Vec<&'static str> = fields.iter().map(|field| leak(field)).collect();
                ResolveTypeCheck::AllFields {
                    fields: fields.leak(),
                    name: leak(member),
                }
            })
            .collect();
        let rule = ResolveTypeRule {
            type_name: leak(name),
            valid_names: &[],
            checks: checks.leak(),
        };
        match rules.iter_mut().find(|held| held.type_name == name) {
            Some(held) => *held = rule,
            None => rules.push(rule),
        }
    }
    Ok(())
}

/// Returns the names of the members of the union or the interface `name` of
/// `ir`, or `None` where `ir` holds neither of that name.
#[cfg(feature = "canary-types")]
fn members_of<'a>(ir: &'a Ir, name: &str) -> Option<Vec<&'a str>> {
    if let Some(union) = ir.unions.iter().find(|union| union.name == name) {
        return Some(union.members.iter().map(String::as_str).collect());
    }
    if !ir.interfaces.iter().any(|interface| interface.name == name) {
        return None;
    }
    let implements = |object: &&Type| object.implements.iter().any(|own| own == name);
    Some(
        ir.objects
            .iter()
            .filter(implements)
            .map(|object| object.name.as_str())
            .collect(),
    )
}
