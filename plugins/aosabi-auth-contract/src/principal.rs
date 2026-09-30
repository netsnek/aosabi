// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-auth-contract: the canonical actor of a request and the checks of
 * its roles and permissions.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/auth/principal.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 */

//! `Principal`, its id, `PRINCIPAL_KEY`, `has_role` and `has_permission`.
//!
//! The principal is the actor every gate, row policy and resource check
//! authorizes against. An identity provider produces it and the identity
//! plugin binds it on the request under [`PRINCIPAL_KEY`]. It is plain data,
//! so its checks are free functions that answer `false` for no principal, as
//! the reference's are null safe.
//!
//! `Principal` is no described type, so a resolver never returns it and its
//! doc comments are no API text. The five member comments carry the `JSDoc` of
//! `principal.ts:8-17`, the one dash of line 10 written as a comma.

use alloc::{
    string::String,
    vec::Vec, //
};
use core::fmt;

use aosabi_json::JsObject;

/// The identifier of a principal, a string or a number as the reference types it.
///
/// The reference types `id` and `tenant` as `string | number`, and a check
/// compares them with `===`. So a numeric id never equals the string of its
/// digits, and the comparisons with a string or a number below answer as `===`
/// does. A number is a double, as every number of the reference is.
#[derive(Clone, Debug, PartialEq)]
pub enum PrincipalId {
    /// An id written as a string.
    String(String),
    /// An id written as a number.
    Number(f64),
}

impl Default for PrincipalId {
    /// Returns the empty string, which every literal of a principal
    /// overwrites, so `..Principal::default()` fills the optional members.
    fn default() -> Self {
        Self::String(String::new())
    }
}

impl From<String> for PrincipalId {
    fn from(id: String) -> Self {
        Self::String(id)
    }
}

impl From<&str> for PrincipalId {
    fn from(id: &str) -> Self {
        Self::String(String::from(id))
    }
}

impl From<f64> for PrincipalId {
    fn from(id: f64) -> Self {
        Self::Number(id)
    }
}

impl From<i32> for PrincipalId {
    fn from(id: i32) -> Self {
        Self::Number(f64::from(id))
    }
}

impl From<u32> for PrincipalId {
    fn from(id: u32) -> Self {
        Self::Number(f64::from(id))
    }
}

impl PartialEq<str> for PrincipalId {
    /// Answers `id === other` for a string, which is false for a number.
    fn eq(&self, other: &str) -> bool {
        matches!(self, Self::String(id) if id == other)
    }
}

impl PartialEq<&str> for PrincipalId {
    /// Answers `id === other` for a string, which is false for a number.
    fn eq(&self, other: &&str) -> bool {
        *self == **other
    }
}

impl PartialEq<String> for PrincipalId {
    /// Answers `id === other` for a string, which is false for a number.
    fn eq(&self, other: &String) -> bool {
        *self == *other.as_str()
    }
}

impl PartialEq<f64> for PrincipalId {
    /// Answers `id === other` for a number, which is false for a string.
    fn eq(&self, other: &f64) -> bool {
        matches!(self, Self::Number(id) if id == other)
    }
}

impl fmt::Display for PrincipalId {
    /// Writes `String(id)`, a number through `Number::toString`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::String(id) => f.write_str(id),
            Self::Number(id) => f.write_str(&aosabi_json::number_to_string(*id)),
        }
    }
}

/// The canonical actor of a request.
///
/// An identity provider produces it from a request, whatever the mechanism,
/// and the identity plugin binds it on the request. Every capability gate,
/// resource check and row policy downstream authorizes against it. A member
/// the reference marks optional is an `Option`, so an absent list and an
/// empty list stay apart as they do in the reference.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Principal {
    /// Stable unique id of the actor (user id, service-account id).
    pub id: PrincipalId,
    /// Tenant the actor belongs to (e.g. organizationId), also drives ORM tenant scoping.
    pub tenant: Option<PrincipalId>,
    /// Coarse roles (RBAC).
    pub roles: Option<Vec<String>>,
    /// Fine-grained permissions, e.g. `invoice:write` (PBAC).
    pub permissions: Option<Vec<String>>,
    /// Arbitrary claims for attribute-based rules (ABAC).
    pub attributes: Option<JsObject>,
}

/// The request context key the bound principal lives under.
///
/// A layer below the web runtime reads the principal of a request under this
/// key without naming the auth root, as the data layer binds its principal
/// and tenant from it.
pub const PRINCIPAL_KEY: &str = "principal";

/// A principal or its absence, as a check receives it.
///
/// The reference's checks take `Principal | undefined`. Each form an author
/// holds implements this trait, so `has_role(get_principal(), &["a"])` and
/// `has_role(p, &["a"])` inside an `authorize` check read like
/// `hasRole(getPrincipal(), 'a')` and `hasRole(p, 'a')`.
pub trait PrincipalRef {
    /// Returns the principal, or `None` for public access.
    fn principal(&self) -> Option<&Principal>;
}

impl PrincipalRef for Option<&Principal> {
    fn principal(&self) -> Option<&Principal> {
        *self
    }
}

impl PrincipalRef for Option<Principal> {
    fn principal(&self) -> Option<&Principal> {
        self.as_ref()
    }
}

impl PrincipalRef for &Option<Principal> {
    fn principal(&self) -> Option<&Principal> {
        self.as_ref()
    }
}

impl PrincipalRef for &Principal {
    fn principal(&self) -> Option<&Principal> {
        Some(*self)
    }
}

/// Answers whether the principal holds any of `roles`, false for no principal.
///
/// A principal without the member `roles` holds none, and an empty `roles`
/// asks for none, so both answer `false`, which is
/// `!!p?.roles && roles.some(r => p.roles!.includes(r))`.
#[must_use]
// The principal is taken by value, so `has_role(get_principal(), &["a"])`
// reads as `hasRole(getPrincipal(), 'a')` with no borrow at the call.
#[expect(
    clippy::needless_pass_by_value,
    reason = "the call shape of the reference takes the principal as it is"
)]
pub fn has_role(p: impl PrincipalRef, roles: &[&str]) -> bool {
    holds_any(p.principal().and_then(|p| p.roles.as_deref()), roles)
}

/// Answers whether the principal holds any of `permissions`, false for no principal.
///
/// A principal without the member `permissions` holds none, and an empty
/// `permissions` asks for none, so both answer `false`, which is
/// `!!p?.permissions && permissions.some(x => p.permissions!.includes(x))`.
#[must_use]
// The principal is taken by value, as for `has_role`.
#[expect(
    clippy::needless_pass_by_value,
    reason = "the call shape of the reference takes the principal as it is"
)]
pub fn has_permission(p: impl PrincipalRef, permissions: &[&str]) -> bool {
    holds_any(
        p.principal().and_then(|p| p.permissions.as_deref()),
        permissions,
    )
}

/// Answers whether `held` is present and holds any of `wanted`, in the order
/// of `wanted`, as `wanted.some(x => held.includes(x))` walks.
fn holds_any(held: Option<&[String]>, wanted: &[&str]) -> bool {
    held.is_some_and(|held| wanted.iter().any(|x| held.iter().any(|h| h == x)))
}
