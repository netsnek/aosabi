// SPDX-License-Identifier: MIT
/*
 * hyper::proto::h1::node::head: the head of an answer of the Node connection.
 *
 * Translation of _storeHeader of Node 24.13.0, lib/_http_outgoing.js:397-517
 * and :574-602, and of the constructor of ServerResponse,
 * lib/_http_server.js:195-213 and :488-494. It implements quirk Q44 of
 * docs/byte-equality.md section 13.1.
 */
//! The head of an answer with Node's date and connection lines, quirk Q44.

use alloc::vec::Vec;

use http::header::CONNECTION;
#[cfg(feature = "node-lines")]
use http::header::{HeaderName, CONTENT_LENGTH, DATE, TRANSFER_ENCODING};
use http::response::Parts;
use http::{HeaderValue, Method, StatusCode, Version};

use super::NodeConfig;
use crate::ext::{NodeHead, NodeVersion};
use crate::proto::h1::{role, Encode, Encoder, ServerTransaction};
use crate::proto::{BodyLength, MessageHead};

/// Writes the head of an answer into `dst` and returns the encoder of its body.
///
/// The answer has no body for a `HEAD` request and for 204, 304 and a 1xx
/// status, as Node's `_hasBody` gives none (rule N4), and a body of unknown
/// size otherwise, so hyper writes the lines of the record as they stand and
/// frames the body chunked where Node does. The date `date` is the wall clock
/// of the timer, `None` on a card without one.
///
/// # Errors
///
/// Returns hyper's error for a record hyper cannot write.
pub(crate) fn encode(
    mut parts: Parts,
    method: &Method,
    request: &NodeHead,
    config: &NodeConfig,
    date: Option<&[u8]>,
    dst: &mut Vec<u8>,
) -> crate::Result<Encoder> {
    let status = parts.status;
    let has_body = *method != Method::HEAD
        && !status.is_informational()
        && !matches!(status, StatusCode::NO_CONTENT | StatusCode::NOT_MODIFIED);
    let keep_alive = lines(&mut parts, request, config, date, has_body);
    let mut head = MessageHead {
        version: if request.version == NodeVersion::Http11 {
            Version::HTTP_11
        } else {
            Version::HTTP_10
        },
        subject: status,
        headers: parts.headers,
        extensions: parts.extensions,
    };
    if cfg!(feature = "node-framing") {
        head.extensions.insert(request.clone());
    }
    let mut req_method = Some(method.clone());
    role::encode_headers::<ServerTransaction>(
        Encode {
            head: &mut head,
            body: has_body.then_some(BodyLength::Unknown),
            keep_alive,
            req_method: &mut req_method,
            // Node's own lines carry capitals, quirk Q44.
            title_case_headers: cfg!(feature = "node-lines"),
            date_header: !cfg!(feature = "node-lines") && date.is_some(),
            #[cfg(not(feature = "std"))]
            date: date.unwrap_or_default(),
        },
        dst,
    )
}

/// Adds hyper's own connection line, `connection: close` for an answer whose
/// connection ends, and returns whether the connection stays open.
#[cfg(not(feature = "node-lines"))]
fn lines(parts: &mut Parts, request: &NodeHead, _: &NodeConfig, _: Option<&[u8]>, _: bool) -> bool {
    if !request.keep_alive && !parts.headers.contains_key(CONNECTION) {
        parts
            .headers
            .insert(CONNECTION, HeaderValue::from_static("close"));
    }
    request.keep_alive
}

/// Appends Node's own lines after the record and returns whether the
/// connection stays open after the answer (quirk Q44).
///
/// The lines are `Date` unless the record names `date`, then, unless the
/// record names `connection`, `Connection: keep-alive` with
/// `Keep-Alive: timeout=5` or `Connection: close`, as `_storeHeader` writes
/// them. A record that names `connection` decides the close by the
/// expression `(?:^|\W)close(?:$|\W)` and keeps the connection otherwise, and
/// one that names `keep-alive` turns Node's own `Keep-Alive` off. A version
/// other than HTTP/1.1 frames a body of unknown size chunked only when its
/// `TE` names `chunked` and ends the connection after the body otherwise.
#[cfg(feature = "node-lines")]
fn lines(
    parts: &mut Parts,
    request: &NodeHead,
    config: &NodeConfig,
    date: Option<&[u8]>,
    has_body: bool,
) -> bool {
    use super::parse::has_word;

    let headers = &parts.headers;
    let mut keep = request.keep_alive;
    let mut last = false;
    for value in headers.get_all(CONNECTION) {
        if has_word(value.as_bytes(), b"close") {
            last = true;
        } else {
            keep = true;
        }
    }
    let length = headers.contains_key(CONTENT_LENGTH);
    let encoding = headers.contains_key(TRANSFER_ENCODING);
    // A chunked record on a 204 or a 304 is not kept alive, quirk Q30.
    if cfg!(feature = "node-framing")
        && matches!(
            parts.status,
            StatusCode::NO_CONTENT | StatusCode::NOT_MODIFIED
        )
        && headers
            .get_all(TRANSFER_ENCODING)
            .iter()
            .any(|value| has_word(value.as_bytes(), b"chunked"))
    {
        keep = false;
    }
    let chunked = request.version == NodeVersion::Http11 || request.te_chunked;
    let keep_alive = HeaderName::from_static("keep-alive");
    let mut added = Vec::new();
    if !headers.contains_key(DATE) {
        if let Some(value) = date.and_then(|date| HeaderValue::from_bytes(date).ok()) {
            added.push((DATE, "Date", value));
        }
    }
    if !headers.contains_key(CONNECTION) {
        if keep && (length || chunked) {
            added.push((
                CONNECTION,
                "Connection",
                HeaderValue::from_static("keep-alive"),
            ));
            if !headers.contains_key(&keep_alive) {
                let timeout = alloc::format!("timeout={}", config.keep_alive_timeout_ms / 1000);
                if let Ok(value) = HeaderValue::from_str(&timeout) {
                    added.push((keep_alive, "Keep-Alive", value));
                }
            }
        } else {
            last = true;
            added.push((CONNECTION, "Connection", HeaderValue::from_static("close")));
        }
    }
    if !length && !encoding && has_body && !chunked {
        last = true;
    }
    append(parts, added);
    !last
}

/// Appends `lines` after the record, with their spellings in its case map
/// when the answer carries one (quirk Q28).
#[cfg(feature = "node-lines")]
fn append(parts: &mut Parts, lines: Vec<(HeaderName, &'static str, HeaderValue)>) {
    #[cfg(feature = "node-record")]
    let spell = {
        use crate::ext::HeaderCaseMap;
        if parts.extensions.get::<HeaderCaseMap>().is_none() {
            let mut record = HeaderCaseMap::new();
            for name in parts.headers.keys() {
                for _ in parts.headers.get_all(name) {
                    record.append(
                        name,
                        bytes::Bytes::copy_from_slice(name.as_str().as_bytes()),
                    );
                }
            }
            parts.extensions.insert(record);
        }
        parts.extensions.get_mut::<HeaderCaseMap>()
    };
    #[cfg(feature = "node-record")]
    let mut spell = spell;
    for (name, spelling, value) in lines {
        #[cfg(not(feature = "node-record"))]
        let _ = spelling;
        #[cfg(feature = "node-record")]
        if let Some(record) = spell.as_deref_mut() {
            record.append(&name, bytes::Bytes::from_static(spelling.as_bytes()));
        }
        parts.headers.append(name, value);
    }
}
