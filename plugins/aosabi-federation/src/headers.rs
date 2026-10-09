// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-federation: the headers a configured federation sends to its remote.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/core/gateway.ts,
 * Apache-2.0. Changed by the Aosabi authors.
 */

//! The headers of a configured federation, [`Headers`], the function that
//! answers them, [`HeadersFn`], and their literal, [`headers!`](crate::headers!).
//!
//! The executor a federation builds passes
//! `headers: r => ({...(config.headers ? config.headers(r?.context) : {})})`,
//! so the headers function of a configuration answers an object literal of
//! names and values, which the executor copies with a spread. [`Headers`] is
//! that object. A name keeps its case and the place it was first set at, and
//! a later value for it replaces the earlier, as a key of an object literal
//! does. A name that is an array index stands before the others in ascending
//! order, which is the order `OrdinaryOwnPropertyKeys` gives the keys of an
//! object, so the headers iterate in the order the spread of the reference
//! copies them.
//!
//! The function takes the context of the delegating request, and none for
//! the introspection of the remote schema, whose request carries no context,
//! so a function that forwards a header of the caller sends that header
//! empty to the introspection. `aosabi-compat` registers that as
//! `Q77_FEDERATION_INTROSPECTION_HEADERS`.
//!
//! Under the feature `host-headers`, which only the builds of the
//! TypeScript option take, `HostHeaders` is the headers function of the
//! host, a function of the app that no function pointer holds.

use aosabi_http::Context;
use aosabi_json::value::array_index;

/// Header names and values in insertion order, a later value for a name replacing the earlier in place, as an object literal does.
///
/// The twin of the `Record<string, string>` a headers function answers.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Headers(Vec<(String, String)>);

impl Headers {
    /// Returns headers that hold no name, the object literal `{}`.
    #[must_use]
    pub const fn new() -> Self {
        Self(Vec::new())
    }

    /// Sets the header `name` to `value`, `headers[name] = value`.
    ///
    /// A name that is set already keeps its place and takes the new value.
    pub fn set(&mut self, name: &str, value: impl Into<String>) {
        let value = value.into();
        match self.0.iter_mut().find(|(known, _)| known == name) {
            Some(entry) => entry.1 = value,
            None => self.0.push((String::from(name), value)),
        }
    }

    /// Returns the value of the header `name`, `headers[name]`.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(known, _)| known == name)
            .map(|(_, value)| value.as_str())
    }

    /// Returns the number of names.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns whether no name is set.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Iterates over the names and their values in the order of the keys of an object.
    ///
    /// The names that are array indices come first in ascending numeric
    /// order, and the other names follow in the order they were first set.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        let mut indexed: Vec<(u32, usize)> = self
            .0
            .iter()
            .enumerate()
            .filter_map(|(position, (name, _))| {
                array_index(name.as_bytes()).map(|index| (index, position))
            })
            .collect();
        indexed.sort_unstable();
        let named = self
            .0
            .iter()
            .filter(|(name, _)| array_index(name.as_bytes()).is_none());
        indexed
            .into_iter()
            .filter_map(|(_, position)| self.0.get(position))
            .chain(named)
            .map(|(name, value)| (name.as_str(), value.as_str()))
    }
}

/// The headers function of a federation, `(ctx: Context) => Record<string, string>`.
///
/// The context is the one of the delegating request, and `None` for the
/// introspection of the remote schema.
pub type HeadersFn = fn(Option<&Context>) -> Headers;

/// A headers function of the host, the function of an app of the TypeScript
/// option that the host calls, under the feature `host-headers`.
///
/// A [`HeadersFn`] is a function pointer, which holds no function of the
/// host, so the engine of the TypeScript option hands a federation this
/// value: the call of the function of the app, with the context of the
/// delegating request and `None` for the introspection, and the identity the
/// host gives that function, equal for one function of the app and apart for
/// two, which keys the slot of the cache of remote schemas as the address of
/// a [`HeadersFn`] keys it.
#[cfg(feature = "host-headers")]
#[derive(Clone)]
pub struct HostHeaders {
    /// The identity of the function of the host.
    pub identity: u64,
    /// The call of the function of the host.
    pub call: HostHeadersCall,
}

/// The call of a headers function of the host, with the context of the
/// delegating request and `None` for the introspection.
#[cfg(feature = "host-headers")]
pub type HostHeadersCall = std::rc::Rc<dyn Fn(Option<&Context>) -> Headers>;

#[cfg(feature = "host-headers")]
impl core::fmt::Debug for HostHeaders {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("HostHeaders")
            .field("identity", &self.identity)
            .finish_non_exhaustive()
    }
}

/// Builds [`Headers`] from a literal of names and values, the object literal a headers function answers.
///
/// A name is an identifier or a string literal, the second for a name an
/// identifier cannot spell, and a value is an expression that converts into
/// a `String`. A raw identifier names the header without its prefix, so
/// `r#type` sets the header `type`. The names are set in source order, so a
/// name written twice keeps its first place and its last value, as in an
/// object literal.
///
/// # Examples
///
/// ```ignore
/// fn fwd(ctx: Option<&Context>) -> Headers {
///     headers! {
///         authorization: ctx
///             .and_then(|c| c.req().header("authorization"))
///             .unwrap_or_default(),
///         "x-api-key": "k1",
///     }
/// }
/// ```
#[macro_export]
macro_rules! headers {
    ($($name:tt : $value:expr),* $(,)?) => {{
        #[allow(unused_mut, reason = "an empty literal sets no header")]
        let mut headers = $crate::headers::Headers::new();
        $(headers.set($crate::__federation_key!($name), $value);)*
        headers
    }};
}

/// Answers the key of a member of a literal, the text of an identifier or a string literal.
///
/// The text of an identifier is the name it spells, so a raw identifier
/// loses its prefix when the literal is compiled, as the derive reads the
/// member of a field named with a Rust keyword. The key of the reference is
/// the plain property name, `{type: true}`.
#[doc(hidden)]
#[macro_export]
macro_rules! __federation_key {
    ($key:ident) => {
        const { $crate::headers::__unraw(::core::stringify!($key)) }
    };
    ($key:literal) => {
        $key
    };
}

/// Returns the name a stringified identifier spells, the text after the prefix `r#` of a raw identifier.
///
/// No GraphQL name holds `#`, so the prefix never belongs to a name of the
/// remote, and the text of an identifier with no prefix is returned as it
/// is.
#[doc(hidden)]
#[must_use]
pub const fn __unraw(name: &'static str) -> &'static str {
    match name.as_bytes() {
        [b'r', b'#', ..] => name.split_at(2).1,
        _ => name,
    }
}
