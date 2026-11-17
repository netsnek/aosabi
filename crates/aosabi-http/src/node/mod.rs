// SPDX-License-Identifier: MIT
/*
 * aosabi-http: the tables of Node and the helpers of the record of the Node
 * adapter.
 *
 * No file of its own on the reference side. Each file of the directory names
 * the source it translates and the quirks it carries.
 */

//! The tables of Node and the helpers of the Node adapter.
//!
//! | Module | Holds | Quirk |
//! | --- | --- | --- |
//! | [`status`] | [`status::STATUS_CODES`], the 63 reasons of Node, and [`status::status_text`] | `NODE_REASON_TABLE` |
//! | [`record`] | [`record::Line`] and the helpers of the adapter's record | `NODE_ADAPTER_RECORD` |
//! | [`value`] | [`value::value_bytes`], the bytes of a value under the first write | `NODE_ENCODING_FIRST_WRITE` |
//! | [`check`] | [`check::check_record`] and [`check::RecordError`], the check of `writeHead` | `NODE_RECORD_CHECK_500` |
//!
//! The tables are no vendored text, because Node's sources are not under
//! `vendor/`, so no provenance row writes them.

pub mod check;
pub mod record;
pub mod status;
pub mod value;

pub use self::{
    check::{
        RecordError,
        check_record, //
    },
    record::{
        DEFAULT_CONTENT_TYPE,
        append_cached_length,
        append_read_length,
        build_outgoing_http_headers, //
    },
    status::{
        STATUS_CODES,
        status_text, //
    },
    value::value_bytes, //
};
