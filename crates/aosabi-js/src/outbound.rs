// SPDX-License-Identifier: MIT
/*
 * aosabi-js outbound: the outbound fetch and the crypto of the host.
 *
 * No analogue in the reference, whose openid-client calls the global fetch and
 * the WebCrypto of its runtime.
 */

//! The outbound fetch and the crypto of the host.
//!
//! [`HostFetch`] is the network capability of `aosabi-core` over the import
//! `host_fetch`: the record of an outbound request crosses once, the host
//! answers with a promise of its own `fetch` with `redirect: 'manual'`, which
//! settles with the status, the status text, the header fields and the whole
//! body of the response, or with the message of the failure, and the call waits
//! for that promise on an agent of its own, as the body of a route waits for a
//! read of the host. A failure becomes [`FetchError::Connect`], which throws
//! the `TypeError` `fetch failed` of the fetch of Node.
//!
//! [`HostCrypto`] is the backend `host-crypto` of `aosabi-auth-zitadel` over
//! the import `host_crypto`, a call that answers at once: the SHA-256 digest of
//! the PKCE challenge, the RSASSA-PKCS1-v1_5 signature with SHA-256 of a
//! client assertion under the `RSAPrivateKey` DER of the key of the app, the
//! read of a key file below the working directory of the host, and its
//! clock, `Date.now()`, the time of a client assertion and of the checks of a
//! token.

#![forbid(unsafe_code)]

use std::{
    rc::Rc,
    sync::Arc, //
};

use crate::{
    abi::import,
    host::{
        self,
        Reader, //
    },
    request,
    value, //
};
use aosabi_auth_zitadel::Host;
use aosabi_core::net::{
    Fetch,
    FetchError, //
};
use aosabi_execute::{
    promise::{
        Agent,
        AgentOptions,
        PromiseState, //
    },
    value::{
        Completion,
        HostValue,
        Value, //
    },
};
use aosabi_http::{
    Body,
    Headers,
    HeadersInit,
    LocalBoxFuture,
    Request,
    Response,
    ResponseInit, //
};

/// The op of a call of `host_crypto` that digests bytes with SHA-256.
const SHA256: u8 = 0;

/// The op of a call of `host_crypto` that signs bytes with a key.
const SIGN: u8 = 1;

/// The op of a call of `host_crypto` that reads a file.
const READ_FILE: u8 = 2;

/// The op of a call of `host_crypto` that reads the clock of the host,
/// `Date.now()`, as eight little endian bytes of a double.
const NOW: u8 = 3;

/// The op of a call of `host_crypto` that imports a key, which answers no
/// bytes where the host imports it.
const IMPORT: u8 = 4;

/// The answer of the `fetch` of the host, as the settlement of its promise
/// carries it.
pub enum Fetched {
    /// A response, its status, its status text, its header fields and its
    /// whole body.
    Response {
        /// The status.
        status: u16,
        /// The status text.
        status_text: String,
        /// The header fields, in the order of the iteration of its `Headers`.
        headers: Vec<(String, String)>,
        /// The body.
        body: Vec<u8>,
    },
    /// The message of the failure the `fetch` of the host rejected with.
    Failed(String),
}

/// Reads the record of a [`Fetched`] at the cursor of `reader`, after its
/// tag, as the value of the settlement it is.
pub fn read_fetched(reader: &mut Reader<'_>) -> Option<Value> {
    let fetched = match reader.byte()? {
        1 => {
            let status = u16::try_from(reader.word()?).ok()?;
            let status_text = String::from(reader.text()?);
            let count = reader.count()?;
            let mut headers = Vec::with_capacity(count.min(64));
            for _ in 0..count {
                let name = String::from(reader.text()?);
                headers.push((name, String::from(reader.text()?)));
            }
            let body = reader.bytes()?.to_vec();
            Fetched::Response {
                status,
                status_text,
                headers,
                body,
            }
        }
        0 => Fetched::Failed(String::from(reader.text()?)),
        _ => return None,
    };
    // An `unsafe` `Rc` would skip the atomic count each copy takes.
    Some(Value::Host(HostValue::Opaque(Arc::new(fetched))))
}

/// Skips the record of a [`Fetched`] at the cursor of `reader`, after its tag.
pub fn skip_fetched(reader: &mut Reader<'_>) -> Option<()> {
    read_fetched(reader).map(|_| ())
}

/// The network capability over the `fetch` of the host.
pub struct HostFetch;

impl Fetch for HostFetch {
    fn fetch(&self, outbound: Request) -> LocalBoxFuture<'static, Result<Response, FetchError>> {
        Box::pin(async move {
            let body = outbound.body().bytes().await.unwrap_or_default();
            let mut record = Vec::with_capacity(body.len() + 256);
            host::push_bytes(&mut record, outbound.method.as_bytes());
            host::push_bytes(&mut record, outbound.url.as_bytes());
            {
                let headers = outbound.headers.borrow();
                let fields = headers.list();
                host::push_count(&mut record, fields.len());
                for (name, field) in fields {
                    host::push_bytes(&mut record, name.as_bytes());
                    host::push_bytes(&mut record, field.as_bytes());
                }
            }
            host::push_bytes(&mut record, &body);
            let request = request::current();
            let agent = Agent::new(AgentOptions::default());
            let completion = agent.enter(|| value::fetch(request, &mut record));
            let settled = match completion {
                Completion::Normal(Value::Promise(promise)) => match agent.drive(&promise).await {
                    Ok(PromiseState::Fulfilled(value)) => value,
                    _ => return Err(FetchError::Connect(String::from("the host fetch stalled"))),
                },
                Completion::Normal(other) => other,
                Completion::Throw(_) => {
                    return Err(FetchError::Connect(String::from("the host fetch threw")));
                }
            };
            response_of(&settled)
        })
    }
}

/// Returns the response the settlement `value` of the `fetch` of the host
/// carries, or the failure it names.
fn response_of(value: &Value) -> Result<Response, FetchError> {
    let Value::Host(HostValue::Opaque(opaque)) = value else {
        return Err(FetchError::Protocol(String::from(
            "the host fetch answered no response",
        )));
    };
    let Some(fetched) = opaque.downcast_ref::<Fetched>() else {
        return Err(FetchError::Protocol(String::from(
            "the host fetch answered no response",
        )));
    };
    match fetched {
        Fetched::Failed(message) => Err(FetchError::Connect(message.clone())),
        Fetched::Response {
            status,
            status_text,
            headers,
            body,
        } => {
            let mut fields = Headers::new();
            for (name, field) in headers {
                fields
                    .append(name, field)
                    .map_err(|refusal| FetchError::Protocol(refusal.to_string()))?;
            }
            let init = ResponseInit {
                status: Some(*status),
                status_text: Some(status_text.clone()),
                headers: Some(HeadersInit::Fetch(fields)),
            };
            Response::new(Body::Full(Rc::from(body.as_slice())), Some(init))
                .map_err(|refusal| FetchError::Protocol(refusal.to_string()))
        }
    }
}

/// The crypto and the key file reader of the host.
pub struct HostCrypto;

impl HostCrypto {
    /// Calls `host_crypto` with the op `op` and the fields `fields`, and
    /// answers the bytes of its answer, or `None` where the host answers none.
    fn call(op: u8, fields: &[&[u8]]) -> Option<Vec<u8>> {
        let mut record = vec![op];
        host::push_count(&mut record, fields.len());
        for field in fields {
            host::push_bytes(&mut record, field);
        }
        let len = import::host_crypto(host::address(&mut record), host::length(record.len()));
        let answer = host::input(len);
        let mut reader = Reader::new(&answer);
        match reader.byte()? {
            1 => Some(reader.bytes()?.to_vec()),
            _ => None,
        }
    }
}

impl Host for HostCrypto {
    fn import_rs256(&self, key: &[u8]) -> bool {
        Self::call(IMPORT, &[key]).is_some()
    }

    fn sign_rs256(&self, key: &[u8], data: &[u8]) -> Option<Vec<u8>> {
        Self::call(SIGN, &[key, data])
    }

    fn sha256(&self, data: &[u8]) -> Option<[u8; 32]> {
        Self::call(SHA256, &[data])?.try_into().ok()
    }

    fn read_file(&self, path: &str) -> Option<Vec<u8>> {
        Self::call(READ_FILE, &[path.as_bytes()])
    }

    fn now_ms(&self) -> Option<f64> {
        let bytes: [u8; 8] = Self::call(NOW, &[])?.try_into().ok()?;
        Some(f64::from_le_bytes(bytes))
    }
}

/// Installs the network capability and the crypto of the host for the
/// runtime thread, before the first pass of the boot.
pub fn install() {
    aosabi_core::net::install_fetch(Rc::new(HostFetch));
    aosabi_auth_zitadel::install_host(Rc::new(HostCrypto));
}
