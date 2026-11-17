// SPDX-License-Identifier: MIT
/*
 * hyper::proto::h1::node::methods: the methods Node 24.13.0 serves.
 *
 * Translation of the list require('http').METHODS of Node 24.13.0, the methods
 * of HTTP_METHOD_MAP of llhttp 9.3.0 that Node names. It holds the list of
 * the quirk LLHTTP_METHODS.
 */
//! The 35 methods of Node 24.13.0 the Node parser accepts, quirk
//! `LLHTTP_METHODS`.

/// The names of `require('http').METHODS` of Node 24.13.0, in its order.
///
/// A request whose method is not one of them, `PRI` among them, is refused
/// with Node's fixed 400 (quirk `LLHTTP_METHODS`).
pub const NODE_METHODS: &[&str] = &[
    "ACL",
    "BIND",
    "CHECKOUT",
    "CONNECT",
    "COPY",
    "DELETE",
    "GET",
    "HEAD",
    "LINK",
    "LOCK",
    "M-SEARCH",
    "MERGE",
    "MKACTIVITY",
    "MKCALENDAR",
    "MKCOL",
    "MOVE",
    "NOTIFY",
    "OPTIONS",
    "PATCH",
    "POST",
    "PROPFIND",
    "PROPPATCH",
    "PURGE",
    "PUT",
    "QUERY",
    "REBIND",
    "REPORT",
    "SEARCH",
    "SOURCE",
    "SUBSCRIBE",
    "TRACE",
    "UNBIND",
    "UNLINK",
    "UNLOCK",
    "UNSUBSCRIBE",
];

/// The length of the longest name of [`NODE_METHODS`].
pub(crate) const LONGEST: usize = 11;

/// Returns the name of [`NODE_METHODS`] that `name` spells, or `None` when
/// Node refuses the method.
pub(crate) fn find(name: &[u8]) -> Option<&'static str> {
    NODE_METHODS
        .iter()
        .copied()
        .find(|method| method.as_bytes() == name)
}
