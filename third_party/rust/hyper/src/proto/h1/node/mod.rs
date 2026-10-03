// SPDX-License-Identifier: MIT
/*
 * hyper::proto::h1::node: the configuration of the Node connection and the
 * modules of the Node parity layer.
 *
 * Translation of the limits of Node, whose sources are Node 24.13.0
 * lib/_http_common.js:53 and :62-63, http.maxHeaderSize,
 * lib/_http_server.js:468-510, :1094-1104 and :1110-1114,
 * lib/_http_outgoing.js:388-392 and src/node_http_parser.cc:84-86 and
 * :751-764, and ADR-0026 of Aosabi, sections on JSON content coding and on
 * the limits outside the API, for max_request and max_response. The
 * bounds of ESP32C3 are the bounds of a device card and no quirk.
 */
//! The configuration of the Node connection and the modules of the Node parity layer.

#[cfg(feature = "node-conn")]
pub(crate) mod answers;
#[cfg(feature = "node-conn")]
pub(crate) mod conn;
#[cfg(feature = "node-conn")]
pub(crate) mod head;
pub(crate) mod methods;
pub(crate) mod parse;
#[cfg(feature = "node-timers")]
pub(crate) mod timers;

/// How the parser answers a head with a field past [`NodeLimits::max_fields`].
// Without `node-conn` the parser reads the limits of Node alone, so the
// bounds of a device card have no reader there.
#[cfg_attr(not(feature = "node-conn"), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverFields {
    /// Refuses the head with 431, as hyper refuses a head past its
    /// `max_headers`, where Node drops every field past the first 1,023
    /// (divergence D74 of quirk Q26).
    Overflow,
    /// Refuses the request with Node's fixed 413, the bound of a device card.
    Refuse,
}

/// The bounds of a request head and body the Node parser holds.
// Without `node-conn` no path reads these bounds but the parser's own.
#[cfg_attr(not(feature = "node-conn"), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NodeLimits {
    /// Counted bytes of a head, the target, the names and the values, answered with 431.
    pub max_head: usize,
    /// Bytes of a head the parser keeps, the bytes llhttp skips not among them.
    ///
    /// `None` leaves the bound to the count of `max_head`, as Node does, and a
    /// device card sets it to the bytes it can hold.
    pub head_buffer: Option<usize>,
    /// Fields of a head that reach the service, past which the head is refused.
    pub max_fields: usize,
    /// How a head with a field past `max_fields` is refused.
    pub over_fields: OverFields,
    /// Bytes of the extensions of one chunk, counted from zero at each chunk, answered with 413.
    pub max_chunk_extensions: usize,
    /// Decoded body bytes, answered with Node's fixed 413.
    pub max_body: Option<u64>,
    /// Kept head bytes and decoded body bytes together, answered with Node's fixed 413.
    pub max_request: Option<u64>,
    /// Unread request body bytes held before the connection stops reading.
    pub body_buffer: usize,
}

// Without `node-conn` no path reads these bounds but the parser's own.
#[cfg_attr(not(feature = "node-conn"), allow(dead_code))]
impl NodeLimits {
    /// The bounds of Node 24.13.0, which the server cards take.
    pub const NODE: NodeLimits = NodeLimits {
        max_head: 16_384,
        head_buffer: None,
        max_fields: 1_023,
        over_fields: OverFields::Overflow,
        max_chunk_extensions: 16_384,
        max_body: Some(64 * 1024 * 1024),
        max_request: None,
        body_buffer: 65_536,
    };

    /// The bounds of the card `esp32c3-devkitm1`, whose request fits in 4 KiB.
    #[cfg(feature = "node-conn")]
    pub const ESP32C3: NodeLimits = NodeLimits {
        max_head: 4_096,
        head_buffer: Some(4_096),
        max_fields: 64,
        over_fields: OverFields::Refuse,
        max_chunk_extensions: 16_384,
        max_body: None,
        max_request: Some(4_096),
        body_buffer: 4_096,
    };
}

/// The configuration of the Node connection, its limits, its timers and its queues.
#[cfg(feature = "node-conn")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NodeConfig {
    /// The bounds of a request head and body.
    pub limits: NodeLimits,
    /// Milliseconds an idle connection stays open, advertised as `timeout=5`.
    pub keep_alive_timeout_ms: u32,
    /// Milliseconds the idle close waits beyond `keep_alive_timeout_ms`.
    pub keep_alive_buffer_ms: u32,
    /// Milliseconds a head may take from the first byte llhttp does not skip.
    pub headers_timeout_ms: u32,
    /// Milliseconds a whole request may take.
    pub request_timeout_ms: u32,
    /// Milliseconds between two checks of the head and request timers.
    pub check_interval_ms: u32,
    /// Requests whose answers have not ended, `None` for no bound.
    ///
    /// A device card bounds them, and one more head answers Node's fixed 413.
    pub max_in_flight: Option<usize>,
    /// Bytes one read takes.
    pub read_size: usize,
    /// Queued answer bytes of the connection at which it stops reading.
    pub write_high_water: usize,
    /// Bytes queued for one answer that waits behind the oldest, at which its body waits.
    pub answer_high_water: usize,
    /// The largest whole body and the largest chunk the connection writes, `None` for no bound.
    pub max_response: Option<usize>,
    /// Milliseconds after an answer at which an unread request body closes the connection.
    pub cleanup_after_ms: u32,
}

#[cfg(feature = "node-conn")]
impl NodeConfig {
    /// Node's connection, which the server cards take.
    pub const NODE: NodeConfig = NodeConfig {
        limits: NodeLimits::NODE,
        keep_alive_timeout_ms: 5_000,
        keep_alive_buffer_ms: 1_000,
        headers_timeout_ms: 60_000,
        request_timeout_ms: 300_000,
        check_interval_ms: 30_000,
        max_in_flight: None,
        read_size: 65_536,
        write_high_water: 65_536,
        answer_high_water: 65_536,
        max_response: None,
        cleanup_after_ms: 2,
    };

    /// Node's timers over the bounds of `esp32c3-devkitm1`, two requests in
    /// flight and an answer bounded by the card's `max_response`.
    pub const ESP32C3: NodeConfig = NodeConfig {
        limits: NodeLimits::ESP32C3,
        keep_alive_timeout_ms: 5_000,
        keep_alive_buffer_ms: 1_000,
        headers_timeout_ms: 60_000,
        request_timeout_ms: 300_000,
        check_interval_ms: 30_000,
        max_in_flight: Some(2),
        read_size: 4_096,
        write_high_water: 8_192,
        answer_high_water: 8_192,
        max_response: Some(8_192),
        cleanup_after_ms: 2,
    };
}
