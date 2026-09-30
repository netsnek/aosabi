// SPDX-License-Identifier: MIT
/*
 * aosabi-core: the network capability, the outbound HTTP call a service asks
 * its context for.
 *
 * No analogue on the reference side, where the global fetch of the runtime is
 * the network. The capability is one of the five of Documentation/architecture.md section
 * 14.2.
 */

//! `Fetch`, `FetchError`, `fetch` and `install_fetch`.
//!
//! A service asks its context for a capability rather than assuming one, so an outbound
//! call goes through the driver the serving layer installed before the first pass of the
//! boot. The driver lives in a thread local of the one runtime thread, since a request
//! and its futures never leave that thread. With no driver a call answers
//! [`FetchError::NoNetwork`], which is every card that links no client and a Worker build
//! before its glue hands the host's `fetch`, and a route that needs the network answers
//! the error a failed call throws.
//!
//! A failed call throws `new TypeError('fetch failed')`, the text the fetch of Node
//! throws for a failure below HTTP with the failure as its `cause`. The reason rides in
//! the host slot of the error as an `Opaque` handle, where a layer that logs it reads it
//! back, and `cause` is no enumerable property, so the error has no own property a layer
//! above reads.

use core::{
    cell::RefCell,
    fmt, //
};
use std::{
    rc::Rc,
    sync::Arc, //
};

use aosabi_execute::value::{
    ErrorName,
    ErrorObject,
    Value, //
};
use aosabi_hooks::LocalBoxFuture;
use aosabi_http::{
    Request,
    Response, //
};
use aosabi_json::{
    JsObject,
    Opaque, //
};

use crate::glue::Thrown;

/// An outbound HTTP call, the network capability a service asks its context for.
pub trait Fetch: 'static {
    /// Sends `request` and answers the response, `fetch(request)`.
    fn fetch(&self, request: Request) -> LocalBoxFuture<'static, Result<Response, FetchError>>;
}

/// Why an outbound call gave no response.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum FetchError {
    /// No driver is installed.
    NoNetwork,
    /// The URL of the request is no absolute URL the driver reaches.
    InvalidUrl(String),
    /// The host of the URL does not resolve.
    Resolve(String),
    /// The connection was refused or broke before the response.
    Connect(String),
    /// The TLS handshake failed.
    Tls(String),
    /// The peer answered no HTTP the driver reads.
    Protocol(String),
    /// The call ran past the bound of its driver.
    Timeout,
    /// The body of the response ran past the bound of its driver.
    BodyTooLarge,
}

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoNetwork => f.write_str("no network capability is installed"),
            Self::InvalidUrl(url) => write!(f, "invalid url: {url}"),
            Self::Resolve(reason) => write!(f, "resolve: {reason}"),
            Self::Connect(reason) => write!(f, "connect: {reason}"),
            Self::Tls(reason) => write!(f, "tls: {reason}"),
            Self::Protocol(reason) => write!(f, "protocol: {reason}"),
            Self::Timeout => f.write_str("the call timed out"),
            Self::BodyTooLarge => f.write_str("the body of the response is too large"),
        }
    }
}

impl core::error::Error for FetchError {}

impl From<FetchError> for Thrown {
    fn from(error: FetchError) -> Self {
        let cause: Rc<dyn core::any::Any> = Rc::new(Opaque(Arc::new(error)));
        Self(Value::from(ErrorObject::custom(
            ErrorName::TypeError,
            "fetch failed",
            JsObject::new(),
            Some(cause),
        )))
    }
}

thread_local! {
    /// The driver of the runtime thread.
    static DRIVER: RefCell<Option<Rc<dyn Fetch>>> = const { RefCell::new(None) };
}

/// Sends `request` through the driver the boot installed, or answers [`FetchError::NoNetwork`].
pub fn fetch(request: Request) -> LocalBoxFuture<'static, Result<Response, FetchError>> {
    let driver = DRIVER
        .try_with(|driver| driver.borrow().clone())
        .ok()
        .flatten();
    match driver {
        Some(driver) => driver.fetch(request),
        None => Box::pin(async { Err(FetchError::NoNetwork) }),
    }
}

/// Installs `fetch` as the driver of this runtime thread, once, before the first pass of the boot.
///
/// The first call wins and a later call is ignored, as the randomness of the
/// process keeps its first provider.
pub fn install_fetch(fetch: Rc<dyn Fetch>) {
    // A thread that is being torn down has no driver to keep.
    let _ = DRIVER.try_with(|driver| {
        driver.borrow_mut().get_or_insert(fetch);
    });
}
