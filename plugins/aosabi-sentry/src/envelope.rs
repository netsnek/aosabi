// SPDX-License-Identifier: MIT
/*
 * aosabi-sentry: the event a capture sends and the envelope that carries it.
 *
 * No analogue file on the reference side.
 */

//! `Event`, `Envelope`, `event_id` and the send of an envelope.
//!
//! An event carries the fields the plugin and the middleware decide, the
//! exception or the message, the fingerprint, the tags, the extras, the
//! contexts, the breadcrumbs, the transaction and the request, and the fields
//! of the SDK, which are Aosabi's own and never compared with the reference.
//! An envelope holds one event, its header line, the header line of the item
//! and the event, joined by a line feed with none after the last, as
//! `serializeEnvelope` writes them.
//!
//! [`event_id`] is the id `crypto.randomUUID()` gives with its dashes taken
//! out, 32 lower case hexadecimal digits over 16 bytes of the randomness
//! capability with the version and the variant of a version 4 UUID set.
//!
//! [`Envelope::send`] posts the envelope through the network capability of
//! `aosabi-core` and ignores the answer, as the transport of the SDK ignores
//! all but its rate limits. The future of the call is polled once where the
//! capture happens. A card that installs no driver of the network answers
//! `NoNetwork` on that poll, so nothing leaves the process.

use core::{
    future::Future,
    pin::pin,
    task::{
        Context as TaskContext,
        Waker, //
    },
};

use aosabi_core::{
    net,
    random::{
        self,
        RandomError, //
    },
};
use aosabi_http::fetch::{
    body::Body,
    headers::Headers,
    request::Request, //
};
use aosabi_json::{
    JsObject,
    JsValue, //
};

use crate::dsn::Dsn;

/// The name the SDK writes as its own, in the `sdk` of an event and in the
/// query of the endpoint.
pub const SDK_NAME: &str = "aosabi-sentry";

/// The version the SDK writes as its own.
pub const SDK_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The `platform` of an event, which names the language of the SDK.
pub const PLATFORM: &str = "rust";

/// The media type of an envelope.
const ENVELOPE_TYPE: &str = "application/x-sentry-envelope";

/// Returns a new event id, 32 lower case hexadecimal digits.
///
/// # Errors
///
/// Returns [`RandomError`] when the process has no source of random bytes.
/// The reference never meets that case, since Node always has `crypto`.
pub fn event_id() -> Result<String, RandomError> {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut bytes = [0_u8; 16];
    random::fill(&mut bytes)?;
    // The version 4 in the high nibble of byte 6 and the variant 10 in the
    // two high bits of byte 8, as randomUUID() writes them.
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let mut id = String::with_capacity(32);
    for byte in bytes {
        id.push(char::from(HEX[usize::from(byte >> 4)]));
        id.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    Ok(id)
}

/// The exception of an event, `exception.values[0]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Exception {
    /// The class of the error, `type`.
    pub ty: String,
    /// The message of the error, `value`.
    pub value: String,
}

/// One breadcrumb of a scope.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Breadcrumb {
    /// `category`.
    pub category: Option<String>,
    /// `message`.
    pub message: Option<String>,
    /// `level`.
    pub level: Option<String>,
    /// `timestamp` in seconds, which a scope writes when it adds the crumb.
    pub timestamp: Option<f64>,
}

impl Breadcrumb {
    /// Returns the crumb as the event writes it.
    fn to_json(&self) -> JsValue {
        let mut crumb = JsObject::new();
        if let Some(timestamp) = self.timestamp {
            crumb.insert("timestamp", timestamp);
        }
        put(&mut crumb, "category", self.category.as_deref());
        put(&mut crumb, "message", self.message.as_deref());
        put(&mut crumb, "level", self.level.as_deref());
        JsValue::Object(crumb)
    }
}

/// An event, the payload of an envelope.
#[derive(Clone, Debug, Default)]
pub struct Event {
    /// `event_id`.
    pub event_id: String,
    /// `timestamp` in seconds, absent where the target has no clock of the
    /// wall.
    pub timestamp: Option<f64>,
    /// `platform`.
    pub platform: &'static str,
    /// `level`.
    pub level: Option<String>,
    /// The thrown error.
    pub exception: Option<Exception>,
    /// `message`, the message a capture of a message reports.
    pub message: Option<String>,
    /// `transaction`, the name of the operation.
    pub transaction: Option<String>,
    /// `fingerprint`, left out when empty.
    pub fingerprint: Vec<String>,
    /// `tags`, left out when empty.
    pub tags: JsObject,
    /// `extra`, left out when empty.
    pub extra: JsObject,
    /// `contexts`, left out when empty.
    pub contexts: JsObject,
    /// `breadcrumbs`, left out when empty.
    pub breadcrumbs: Vec<Breadcrumb>,
    /// `request`, the request data of the middleware.
    pub request: Option<JsObject>,
    /// `user`.
    pub user: Option<JsObject>,
    /// `environment`.
    pub environment: Option<String>,
    /// `release`.
    pub release: Option<String>,
}

impl Event {
    /// Returns the event as its JSON object.
    #[must_use]
    pub fn to_json(&self) -> JsObject {
        let mut event = JsObject::new();
        event.insert("event_id", self.event_id.as_str());
        if let Some(timestamp) = self.timestamp {
            event.insert("timestamp", timestamp);
        }
        event.insert("platform", self.platform);
        put(&mut event, "level", self.level.as_deref());
        put(&mut event, "message", self.message.as_deref());
        if let Some(exception) = &self.exception {
            let mut mechanism = JsObject::new();
            mechanism.insert("type", "generic");
            mechanism.insert("handled", true);
            let mut value = JsObject::new();
            value.insert("type", exception.ty.as_str());
            value.insert("value", exception.value.as_str());
            value.insert("mechanism", JsValue::Object(mechanism));
            let mut values = JsObject::new();
            values.insert("values", JsValue::Array(vec![JsValue::Object(value)]));
            event.insert("exception", JsValue::Object(values));
        }
        put(&mut event, "transaction", self.transaction.as_deref());
        if !self.fingerprint.is_empty() {
            let parts = self
                .fingerprint
                .iter()
                .map(|part| JsValue::from(part.as_str()));
            event.insert("fingerprint", JsValue::Array(parts.collect()));
        }
        put_object(&mut event, "tags", &self.tags);
        put_object(&mut event, "extra", &self.extra);
        put_object(&mut event, "contexts", &self.contexts);
        if !self.breadcrumbs.is_empty() {
            let crumbs = self.breadcrumbs.iter().map(Breadcrumb::to_json);
            event.insert("breadcrumbs", JsValue::Array(crumbs.collect()));
        }
        if let Some(request) = &self.request {
            event.insert("request", JsValue::Object(request.clone()));
        }
        if let Some(user) = &self.user {
            event.insert("user", JsValue::Object(user.clone()));
        }
        put(&mut event, "environment", self.environment.as_deref());
        put(&mut event, "release", self.release.as_deref());
        event.insert("sdk", JsValue::Object(sdk()));
        event
    }
}

/// An envelope of one event.
#[derive(Clone, Debug)]
pub struct Envelope {
    /// The event.
    pub event: Event,
}

impl Envelope {
    /// Returns the envelope of `event`.
    #[must_use]
    pub fn new(event: Event) -> Self {
        Self { event }
    }

    /// Returns the bytes of the envelope.
    ///
    /// The header line carries `event_id` and `sdk`, the item header line
    /// `{"type":"event"}`, and the last line the event, with no line feed
    /// after it.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut header = JsObject::new();
        header.insert("event_id", self.event.event_id.as_str());
        header.insert("sdk", JsValue::Object(sdk()));
        let mut item = JsObject::new();
        item.insert("type", "event");
        let mut out = Vec::with_capacity(512);
        aosabi_json::write(&mut out, &JsValue::Object(header));
        out.push(b'\n');
        aosabi_json::write(&mut out, &JsValue::Object(item));
        out.push(b'\n');
        aosabi_json::write(&mut out, &JsValue::Object(self.event.to_json()));
        out
    }

    /// Posts the envelope to the ingest of `dsn` and ignores the answer.
    ///
    /// The call goes through the network capability, so a process with no
    /// driver sends nothing. The future of the call is polled once here, and
    /// one that is still pending after that poll is dropped, since this crate
    /// holds no task that outlives the request of the capture.
    pub fn send(&self, dsn: &Dsn) {
        let client = format!("{SDK_NAME}/{SDK_VERSION}");
        let mut headers = Headers::new();
        // A literal name and a literal value are always valid.
        let _ = headers.set("content-type", ENVELOPE_TYPE);
        let body = Body::Full(self.to_bytes().into());
        let request = Request::new("POST", dsn.envelope_url(&client), headers, body);
        let call = net::fetch(request);
        let mut call = pin!(call);
        let mut cx = TaskContext::from_waker(Waker::noop());
        // The answer is ignored, as the transport of the SDK ignores it.
        let _ = call.as_mut().poll(&mut cx);
    }
}

/// Returns the `sdk` of an event and of an envelope header.
fn sdk() -> JsObject {
    let mut sdk = JsObject::new();
    sdk.insert("name", SDK_NAME);
    sdk.insert("version", SDK_VERSION);
    sdk
}

/// Inserts `value` under `key` when it is present.
fn put(object: &mut JsObject, key: &str, value: Option<&str>) {
    if let Some(value) = value {
        object.insert(key, value);
    }
}

/// Inserts `value` under `key` when it holds a property.
fn put_object(object: &mut JsObject, key: &str, value: &JsObject) {
    if !value.is_empty() {
        object.insert(key, JsValue::Object(value.clone()));
    }
}

/// Returns the seconds since the epoch, `dateTimestampInSeconds()`.
///
/// A Worker build has no clock of the wall the standard library reads, so
/// an event there carries no timestamp and the ingest writes the time it
/// received the event.
#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
#[must_use]
pub fn now_seconds() -> Option<f64> {
    let since = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?;
    Some(since.as_secs_f64())
}

/// Returns no time, since a Worker build has no clock of the wall the
/// standard library reads.
#[cfg(all(target_family = "wasm", target_os = "unknown"))]
#[must_use]
pub fn now_seconds() -> Option<f64> {
    None
}
