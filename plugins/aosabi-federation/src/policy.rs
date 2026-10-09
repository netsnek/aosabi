// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-federation: the argument policy of a patched type, its allowed and
 * its forced arguments.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/core/gateway.ts,
 * Apache-2.0. Changed by the Aosabi authors.
 */

//! The argument policies of the patched types, [`FieldPolicy`] and
//! [`PatchPolicy`], the twin of `pass(patch, policy)`, [`pass`], and the
//! collection of the policies of a federation, [`collect_policies`].
//!
//! A patch transforms the result of a delegated field, so on its own it
//! cannot constrain what a client asks for, and `pass` attaches an argument
//! policy to it: `args`, the allowlist of the arguments a caller may set,
//! and `force`, the arguments the federation sets itself, each a constant or
//! a function of the context of the request. The reference hides the policy
//! in a non enumerable property of the patch function and answers that
//! function, and the port answers a [`Pass`] that holds the patch with its
//! policy, which the patch table of a federation takes apart again.
//! [`collect_policies`] reads the policies of a patch table once per
//! federation into [`Policies`], keyed `<Type>.<field>`, which
//! `ForceArgsTransform` applies to every request.
//!
//! The macro [`policy!`](crate::policy!) writes a [`PatchPolicy`] as the
//! object literal of the reference writes it, with `args` and `force` each
//! optional, and a forced value is an expression of a JSON kind or a closure
//! of the context. A name is a `Cow` of a static string, so a literal costs
//! no allocation and a policy read at run time, as the devtool reads one
//! from a request line, owns its names.

use std::borrow::Cow;

use aosabi_http::Context;
use aosabi_json::{
    JsValue,
    into_json::IntoJson, //
};

/// The argument policy for one field of a patched type.
///
/// `args` is an allowlist: an argument a caller sets outside it is refused,
/// so an argument the remote adds later is denied by default, and an absent
/// list allows every argument. `force` sets arguments on the outgoing
/// request, and a caller may not supply a forced argument, so `force` and
/// `args` are disjoint.
#[derive(Clone, Debug, Default)]
pub struct FieldPolicy {
    /// The arguments a caller may set, `args`, every argument when absent.
    pub args: Option<Vec<Cow<'static, str>>>,
    /// The arguments the federation sets, `force`, in the order of the keys.
    pub force: Option<Vec<(Cow<'static, str>, Force)>>,
}

/// A forced value, a constant or `(ctx) => value` for a per request one.
#[derive(Clone, Debug)]
pub enum Force {
    /// A constant.
    Value(JsValue),
    /// A function of the context of the request, called once per request,
    /// whose `None` forces nothing.
    Context(ForceFn),
}

impl Force {
    /// Returns the constant `value`, a value of any JSON kind.
    #[must_use]
    pub fn value(value: impl IntoJson) -> Self {
        Self::Value(value.into_json())
    }
}

/// The function of a forced value, `(ctx) => value`, which answers `None` for `undefined`.
///
/// A function of the reference may throw, and the port's function answers a value or
/// none, so a policy has no refusal of its own beside the two of
/// `ForceArgsTransform`.
pub type ForceFn = fn(&Context) -> Option<JsValue>;

/// Per field policies for one patched type, keyed by field name.
///
/// The keys in the order of the object literal.
pub type PatchPolicy = Vec<(Cow<'static, str>, FieldPolicy)>;

/// A patch with an argument policy attached, what `pass(patch, policy)` returns.
#[derive(Clone, Debug)]
pub struct Pass<P> {
    /// The patch, unchanged in its effect.
    patch: P,
    /// The policy of the fields of the patched type.
    policy: PatchPolicy,
}

impl<P> Pass<P> {
    /// Returns the patch.
    #[must_use]
    pub const fn patch(&self) -> &P {
        &self.patch
    }

    /// Returns the policy attached to the patch.
    #[must_use]
    pub const fn policy(&self) -> &PatchPolicy {
        &self.policy
    }

    /// Returns the patch and its policy, which a patch table keeps apart.
    #[must_use]
    pub fn into_parts(self) -> (P, PatchPolicy) {
        (self.patch, self.policy)
    }
}

/// Attaches an argument policy to a patch and returns the patch unchanged in its effect.
///
/// The type name of the policy is the key the patch stands under in the
/// patch map, so it is never repeated, and a root field needs no policy,
/// since the resolver that delegates it decides its arguments.
pub const fn pass<P>(patch: P, policy: PatchPolicy) -> Pass<P> {
    Pass { patch, policy }
}

/// The argument policies of a federation keyed `<Type>.<field>`, in the order they were collected.
///
/// The twin of the `Map<string, FieldPolicy>` that `collectPolicies` answers.
#[derive(Clone, Debug, Default)]
pub struct Policies(Vec<(String, FieldPolicy)>);

impl Policies {
    /// Returns a map that holds no policy.
    #[must_use]
    pub const fn new() -> Self {
        Self(Vec::new())
    }

    /// Sets the policy of `key`, `map.set(key, policy)`.
    ///
    /// A key that is set already keeps its place and takes the new policy.
    pub fn set(&mut self, key: String, policy: FieldPolicy) {
        match self.0.iter_mut().find(|(known, _)| *known == key) {
            Some(entry) => entry.1 = policy,
            None => self.0.push((key, policy)),
        }
    }

    /// Returns the policy of `key`, `map.get(key)`.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&FieldPolicy> {
        self.0
            .iter()
            .find(|(known, _)| known == key)
            .map(|(_, policy)| policy)
    }

    /// Returns the number of policies, `map.size`.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns whether the map holds no policy.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Iterates over the keys and their policies in the order they were set.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &FieldPolicy)> {
        self.0.iter().map(|(key, policy)| (key.as_str(), policy))
    }
}

/// Collects `Type.field` → policy from a patch map.
///
/// `patches` are the entries of a patch map in the order of its keys, each a
/// type name with the policy `pass` attached to its patch, and a patch with
/// no policy adds nothing. Every field of a policy is set under the key
/// `<Type>.<field>`.
#[must_use]
pub fn collect_policies<'a>(
    patches: impl IntoIterator<Item = (&'a str, Option<&'a PatchPolicy>)>,
) -> Policies {
    let mut out = Policies::new();
    for (type_name, policy) in patches {
        let Some(policy) = policy else {
            continue;
        };
        for (field, field_policy) in policy {
            out.set(format!("{type_name}.{field}"), field_policy.clone());
        }
    }
    out
}

/// Builds a [`PatchPolicy`] from a literal of field policies, the object literal `pass` takes.
///
/// A field is an identifier or a string literal, and its policy holds
/// `args`, a list of the names a caller may set, and `force`, the forced
/// arguments, each optional and in either order. A forced value is a closure
/// of the context, `|ctx| ...` or `|ctx: &Context| ...`, which captures
/// nothing and answers an `Option<JsValue>`, or else an expression of a JSON
/// kind, which is a constant. A field or a forced argument written as a raw
/// identifier names the GraphQL field or argument without its prefix, so
/// `r#type` keys the policy `<Type>.type` and `r#in` forces the argument
/// `in`, as the plain keys of the reference do.
///
/// # Examples
///
/// ```ignore
/// let policy = policy! {
///     products: { args: ["first"], force: { query: "status:ACTIVE" } },
///     orders: { args: [], force: { tenant: |ctx| tenant_of(ctx) } },
/// };
/// ```
#[macro_export]
macro_rules! policy {
    ($($field:tt : { $($body:tt)* }),* $(,)?) => {
        ::std::vec![$((
            ::std::borrow::Cow::Borrowed($crate::__federation_key!($field)),
            $crate::__federation_field_policy!($($body)*),
        )),*]
    };
}

/// Builds the [`FieldPolicy`] of one field of a [`policy!`](crate::policy!) literal.
#[doc(hidden)]
#[macro_export]
macro_rules! __federation_field_policy {
    () => {
        $crate::policy::FieldPolicy {
            args: ::core::option::Option::None,
            force: ::core::option::Option::None,
        }
    };
    (args: [$($arg:expr),* $(,)?] $(,)?) => {
        $crate::policy::FieldPolicy {
            args: ::core::option::Option::Some(::std::vec![$(::std::borrow::Cow::from($arg)),*]),
            force: ::core::option::Option::None,
        }
    };
    (force: { $($force:tt)* } $(,)?) => {
        $crate::policy::FieldPolicy {
            args: ::core::option::Option::None,
            force: ::core::option::Option::Some($crate::__federation_force!([] $($force)*)),
        }
    };
    (args: [$($arg:expr),* $(,)?], force: { $($force:tt)* } $(,)?) => {
        $crate::policy::FieldPolicy {
            args: ::core::option::Option::Some(::std::vec![$(::std::borrow::Cow::from($arg)),*]),
            force: ::core::option::Option::Some($crate::__federation_force!([] $($force)*)),
        }
    };
    (force: { $($force:tt)* }, args: [$($arg:expr),* $(,)?] $(,)?) => {
        $crate::policy::FieldPolicy {
            args: ::core::option::Option::Some(::std::vec![$(::std::borrow::Cow::from($arg)),*]),
            force: ::core::option::Option::Some($crate::__federation_force!([] $($force)*)),
        }
    };
}

/// Collects the forced arguments of one field of a [`policy!`](crate::policy!) literal, one at a time.
#[doc(hidden)]
#[macro_export]
macro_rules! __federation_force {
    ([$($done:expr),*]) => {
        ::std::vec![$($done),*]
    };
    ([$($done:expr),*] $name:tt : | $ctx:ident : $ty:ty | $body:expr $(, $($rest:tt)*)?) => {
        $crate::__federation_force!(
            [$($done,)* (
                ::std::borrow::Cow::Borrowed($crate::__federation_key!($name)),
                $crate::policy::Force::Context(|$ctx: $ty| $body),
            )]
            $($($rest)*)?
        )
    };
    ([$($done:expr),*] $name:tt : | $ctx:pat_param | $body:expr $(, $($rest:tt)*)?) => {
        $crate::__federation_force!(
            [$($done,)* (
                ::std::borrow::Cow::Borrowed($crate::__federation_key!($name)),
                $crate::policy::Force::Context(|$ctx| $body),
            )]
            $($($rest)*)?
        )
    };
    ([$($done:expr),*] $name:tt : $value:expr $(, $($rest:tt)*)?) => {
        $crate::__federation_force!(
            [$($done,)* (
                ::std::borrow::Cow::Borrowed($crate::__federation_key!($name)),
                $crate::policy::Force::value($value),
            )]
            $($($rest)*)?
        )
    };
}
