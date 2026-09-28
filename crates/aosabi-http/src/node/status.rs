// SPDX-License-Identifier: MIT
/*
 * aosabi-http: the 63 reasons of Node and the reason of a status.
 *
 * It carries the quirk Q60_NODE_REASON_TABLE of docs/byte-equality.md section
 * 13.1.
 */

//! `STATUS_CODES` and `status_text`.
//!
//! Node writes the status line `HTTP/1.1 <status> <reason>` with the reason of
//! its own table, or `unknown` for a status the table lacks, because the
//! adapter of `@hono/node-server` never passes a reason phrase (N1 of the
//! plan's version 2 draft). The table has 63 entries, in the order of the
//! internal module, which is the order of the status. Hyper reads its own
//! table, which differs in three rows and answers no reason for an unknown
//! status, so the serve layer hands this reason to the fork through
//! `ext::ReasonPhrase`, quirk `Q60_NODE_REASON_TABLE`. The judging notebook
//! compares the table with the pinned Node,
//! `tests/notebooks/30_http_tier1.ipynb`.

/// The 63 reasons of Node, each beside its status, in the order of the status.
///
/// The reason of 418 is `I'm a Teapot` and the one of 413 is
/// `Payload Too Large`, as Node spells them.
pub const STATUS_CODES: [(u16, &str); 63] = [
    (100, "Continue"),
    (101, "Switching Protocols"),
    (102, "Processing"),
    (103, "Early Hints"),
    (200, "OK"),
    (201, "Created"),
    (202, "Accepted"),
    (203, "Non-Authoritative Information"),
    (204, "No Content"),
    (205, "Reset Content"),
    (206, "Partial Content"),
    (207, "Multi-Status"),
    (208, "Already Reported"),
    (226, "IM Used"),
    (300, "Multiple Choices"),
    (301, "Moved Permanently"),
    (302, "Found"),
    (303, "See Other"),
    (304, "Not Modified"),
    (305, "Use Proxy"),
    (307, "Temporary Redirect"),
    (308, "Permanent Redirect"),
    (400, "Bad Request"),
    (401, "Unauthorized"),
    (402, "Payment Required"),
    (403, "Forbidden"),
    (404, "Not Found"),
    (405, "Method Not Allowed"),
    (406, "Not Acceptable"),
    (407, "Proxy Authentication Required"),
    (408, "Request Timeout"),
    (409, "Conflict"),
    (410, "Gone"),
    (411, "Length Required"),
    (412, "Precondition Failed"),
    (413, "Payload Too Large"),
    (414, "URI Too Long"),
    (415, "Unsupported Media Type"),
    (416, "Range Not Satisfiable"),
    (417, "Expectation Failed"),
    (418, "I'm a Teapot"),
    (421, "Misdirected Request"),
    (422, "Unprocessable Entity"),
    (423, "Locked"),
    (424, "Failed Dependency"),
    (425, "Too Early"),
    (426, "Upgrade Required"),
    (428, "Precondition Required"),
    (429, "Too Many Requests"),
    (431, "Request Header Fields Too Large"),
    (451, "Unavailable For Legal Reasons"),
    (500, "Internal Server Error"),
    (501, "Not Implemented"),
    (502, "Bad Gateway"),
    (503, "Service Unavailable"),
    (504, "Gateway Timeout"),
    (505, "HTTP Version Not Supported"),
    (506, "Variant Also Negotiates"),
    (507, "Insufficient Storage"),
    (508, "Loop Detected"),
    (509, "Bandwidth Limit Exceeded"),
    (510, "Not Extended"),
    (511, "Network Authentication Required"),
];

/// Returns the reason Node writes for `status`, `unknown` for a status its table lacks.
#[must_use]
pub fn status_text(status: u16) -> &'static str {
    match STATUS_CODES.binary_search_by_key(&status, |&(code, _)| code) {
        Ok(index) => STATUS_CODES[index].1,
        Err(_) => "unknown",
    }
}
