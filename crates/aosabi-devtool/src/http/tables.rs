// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool http: the tables of Node the HTTP layer carries.
 *
 * No analogue in the reference. The keys STATUS_CODES, METHODS and
 * maxHeaderSize are the names of require('http') of Node 24.13.0, which the
 * table cell of tests/notebooks/30_http_tier1.ipynb reads beside them.
 */

//! `tables`.
//!
//! The object answers `STATUS_CODES` as Node's object of reasons keyed by the
//! status as a decimal string, `METHODS` as Node's list, `maxHeaderSize` as
//! the counted bytes of a head, and `NodeLimits` with every bound of the
//! connection by the names of the fork.

use aosabi_http::node::STATUS_CODES;
use aosabi_json::{
    JsObject,
    JsValue, //
};
use hyper::{
    ext::NODE_METHODS,
    server::conn::node1::{
        NodeLimits,
        OverFields, //
    },
};

/// Returns the tables under the bounds `limits`.
pub fn tables(limits: NodeLimits) -> JsValue {
    let mut codes = JsObject::with_capacity(STATUS_CODES.len());
    for (status, reason) in STATUS_CODES {
        codes.insert(status.to_string(), reason);
    }
    let methods = NODE_METHODS.iter().copied().map(JsValue::from).collect();
    let mut tables = JsObject::with_capacity(4);
    tables.insert("STATUS_CODES", JsValue::Object(codes));
    tables.insert("METHODS", JsValue::Array(methods));
    tables.insert("maxHeaderSize", number(limits.max_head));
    tables.insert("NodeLimits", JsValue::Object(bounds(limits)));
    JsValue::Object(tables)
}

/// Returns the bounds of `limits` by the names of the fork.
fn bounds(limits: NodeLimits) -> JsObject {
    let mut bounds = JsObject::with_capacity(8);
    bounds.insert("max_head", number(limits.max_head));
    bounds.insert(
        "head_buffer",
        limits.head_buffer.map_or(JsValue::Null, number),
    );
    bounds.insert("max_fields", number(limits.max_fields));
    bounds.insert(
        "over_fields",
        match limits.over_fields {
            OverFields::Overflow => "overflow",
            OverFields::Refuse => "refuse",
        },
    );
    bounds.insert("max_chunk_extensions", number(limits.max_chunk_extensions));
    bounds.insert(
        "max_body",
        limits.max_body.map_or(JsValue::Null, number_u64),
    );
    bounds.insert(
        "max_request",
        limits.max_request.map_or(JsValue::Null, number_u64),
    );
    bounds.insert("body_buffer", number(limits.body_buffer));
    bounds
}

/// Returns a count as a JSON number.
fn number(count: usize) -> JsValue {
    number_u64(u64::try_from(count).unwrap_or(u64::MAX))
}

/// Returns a count as a JSON number, exact below 2^53, which every bound is.
fn number_u64(count: u64) -> JsValue {
    #[expect(
        clippy::cast_precision_loss,
        reason = "every bound of the connection is below 2^53"
    )]
    JsValue::Number(count as f64)
}
