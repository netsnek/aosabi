// SPDX-License-Identifier: MIT
/*
 * aosabi-fetch: the address of a host.
 *
 * No analogue on the reference side, where Node resolves a name through its
 * own resolver.
 */

//! `resolve`, an IP literal as it is and a name through the resolver of the C
//! library, and `AddressCache`, the last good answer of each name.
//!
//! A name resolves through `std::net::ToSocketAddrs`, which calls the resolver
//! of the C library on the one runtime thread and blocks it for as long as the
//! resolver takes, because the standard library resolves a name only by
//! blocking. An IP literal never resolves. The addresses come back with their
//! families alternating from the family of the first, as section 4 of RFC 8305
//! orders them, which the `autoSelectFamily` of Node implements loosely and
//! turns on by default.
//!
//! [`AddressCache`] keeps the last good answer of a name for [`KEEP_MS`], so a
//! client that opens connections to one host blocks its thread in the resolver
//! once a minute and not once a connection. It never keeps a failure, and once
//! the bound passed it asks the resolver again, so a lookup that then fails
//! fails the call, as the fetch of Node fails a call whose lookup fails. The
//! fetch of Node resolves the name for every connection it opens and keeps no
//! answer of its own, so the twin takes a changed address up to [`KEEP_MS`]
//! after the reference takes it, the entry of `NOT_PORTED` on that lookup.

use std::net::{
    IpAddr,
    SocketAddr,
    ToSocketAddrs, //
};

use aosabi_core::net::FetchError;

use crate::error;

/// The time an answer of the resolver is kept, in milliseconds, 60 seconds.
pub const KEEP_MS: u64 = 60_000;

/// Returns the addresses of `hostname` at `port`, the families alternating.
///
/// `hostname` is the host of a URL without its port, so an IPv6 literal
/// stands in brackets.
///
/// # Errors
///
/// Returns [`FetchError::Resolve`] when the resolver fails or answers no
/// address.
pub fn resolve(hostname: &str, port: u16) -> Result<Vec<SocketAddr>, FetchError> {
    if let Some(ip) = literal(hostname) {
        return Ok(vec![SocketAddr::new(ip, port)]);
    }
    let addrs: Vec<SocketAddr> = (hostname, port)
        .to_socket_addrs()
        .map_err(|error| error::resolve(hostname, &error))?
        .collect();
    if addrs.is_empty() {
        return Err(FetchError::Resolve(format!("{hostname}: no address")));
    }
    Ok(interleave(addrs))
}

/// Returns the IP address `hostname` writes, without the brackets of an IPv6
/// literal, or `None` for a name.
fn literal(hostname: &str) -> Option<IpAddr> {
    hostname
        .strip_prefix('[')
        .and_then(|inner| inner.strip_suffix(']'))
        .unwrap_or(hostname)
        .parse()
        .ok()
}

/// Returns `addrs` with the two families alternating, the family of the first
/// address first, each family in the order the resolver gave.
#[must_use]
pub fn interleave(addrs: Vec<SocketAddr>) -> Vec<SocketAddr> {
    let Some(first) = addrs.first().map(SocketAddr::is_ipv4) else {
        return addrs;
    };
    let (mut lead, mut other): (Vec<_>, Vec<_>) =
        addrs.into_iter().partition(|addr| addr.is_ipv4() == first);
    let mut out = Vec::with_capacity(lead.len() + other.len());
    lead.reverse();
    other.reverse();
    loop {
        match (lead.pop(), other.pop()) {
            (None, None) => return out,
            (a, b) => out.extend(a.into_iter().chain(b)),
        }
    }
}

/// The last good answer of the resolver for each name a client resolved, each
/// kept for [`KEEP_MS`] from the lookup that gave it.
///
/// A client holds one cache on its runtime thread. The answers sit in a list,
/// since a client reaches a few hosts, and a lookup drops every answer whose
/// bound passed, so the list holds the names resolved within the last
/// [`KEEP_MS`] alone.
#[derive(Debug, Default)]
pub struct AddressCache {
    /// The kept answers, one per name.
    kept: Vec<Kept>,
}

/// One kept answer of the resolver.
#[derive(Debug)]
struct Kept {
    /// The name, the host of a URL without its port.
    hostname: String,
    /// The addresses in the order of [`interleave`], at the port of the
    /// lookup that gave them.
    addrs: Vec<SocketAddr>,
    /// The time of that lookup in milliseconds of the clock of the runtime.
    at_ms: u64,
}

impl AddressCache {
    /// Returns a cache that keeps no answer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the addresses of `hostname` at `port` at the time `now_ms`, the
    /// answer kept for the name while it is younger than [`KEEP_MS`], or else
    /// a lookup of [`resolve`], whose answer the cache keeps when it is good.
    ///
    /// An IP literal never reaches the cache. `now_ms` is a time of the clock
    /// of the runtime in milliseconds.
    ///
    /// # Errors
    ///
    /// Returns what [`resolve`] returns, and then the cache keeps no answer
    /// for the name.
    pub fn resolve(
        &mut self,
        hostname: &str,
        port: u16,
        now_ms: u64,
    ) -> Result<Vec<SocketAddr>, FetchError> {
        if let Some(ip) = literal(hostname) {
            return Ok(vec![SocketAddr::new(ip, port)]);
        }
        self.kept
            .retain(|kept| now_ms.saturating_sub(kept.at_ms) < KEEP_MS);
        if let Some(kept) = self.kept.iter().find(|kept| kept.hostname == hostname) {
            return Ok(at_port(&kept.addrs, port));
        }
        let addrs = resolve(hostname, port)?;
        self.kept.push(Kept {
            hostname: String::from(hostname),
            addrs: addrs.clone(),
            at_ms: now_ms,
        });
        Ok(addrs)
    }
}

/// Returns `addrs` at `port`, each with its flow and its scope as the
/// resolver gave them.
fn at_port(addrs: &[SocketAddr], port: u16) -> Vec<SocketAddr> {
    addrs
        .iter()
        .map(|addr| {
            let mut addr = *addr;
            addr.set_port(port);
            addr
        })
        .collect()
}
