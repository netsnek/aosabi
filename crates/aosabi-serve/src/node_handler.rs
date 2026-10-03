// SPDX-License-Identifier: MIT
/*
 * aosabi-serve: the service hyper calls for a request, over the booted app.
 *
 * Derived from hono 1.19.9, dist/listener.js, MIT.
 *
 * It carries the halves of the serve layer of the quirks
 * Q51_NODE_ADAPTER_NULL_BODY and Q60_NODE_REASON_TABLE of docs/byte-equality.md
 * section 13.1.
 */

//! `NodeHandler`.
//!
//! Node hands the listener an `IncomingMessage` and a `ServerResponse`, and
//! the adapter builds a Fetch request from the first and writes its answer
//! into the second. Here hyper hands the service an `http::Request` with the
//! target as it arrived and the facts of Node's own checks among its
//! extensions, and the service answers an `http::Response` the Node
//! connection of the fork writes as Node's `ServerResponse` writes it.
//!
//! The request becomes a [`NodeIncoming`]: its method, its target as it
//! arrived, its version, its fields, the first `Host` Node reads and its body,
//! with the scheme of its connection, `https` inside TLS and `http` outside it,
//! as `incoming.socket.encrypted` decides for `@hono/node-server`. A request of
//! the HTTP/2 server carries its `:path`, its `:scheme` from the URI when `h2`
//! kept it and the scheme of the connection otherwise, its `:authority` or else
//! its first `host` field, and the mark of HTTP/2, which the adapter reads
//! where it tests `instanceof Http2ServerRequest`. The body of a stream of
//! HTTP/2 that the application leaves unread is read to its end by a local task
//! after the answer, so the stream stays open as Node's `http2` server keeps
//! it.
//!
//! The answer of the adapter becomes the response: the status with the reason
//! of Node's table, quirk `Q60_NODE_REASON_TABLE`, the lines of the record in
//! its order with its spellings, each value in the bytes the first write gives
//! it, quirk `Q58_NODE_ENCODING_FIRST_WRITE`, what the answer asks of the
//! connection, and the body as a body of unknown size, so a null body is
//! framed chunked where Node frames it, quirk `Q51_NODE_ADAPTER_NULL_BODY`.
//!
//! The driver of a stream body, which runs the agent of a subscription while
//! its chunks are written, is spawned as a local task before the answer goes to
//! the connection, so before its first chunk is polled. `spawn_local` answers
//! no handle and none is kept. When a write fails or the client closes, the
//! Node connection runs every service it called to its answer and drops the
//! answers (quirk Q45), and the drop of a [`ServeBody`] drops its stream, which
//! cancels it, so the driver runs the cancel to its end on its own task. The
//! drain of the agents of a request after its answer belongs to the core, over
//! the spawner the boot hands in, step 3 of [`crate::serve`].
//!
//! A panic of the application is caught at the handler in a build with debug
//! assertions, the development profile, and answered as a fetch that rejected,
//! a 500 with a null body, `handleFetchError` of the adapter. A build without
//! them, the release profile and the size profile among them, catches nothing,
//! so the panic ends the process under either panic strategy: `abort` ends it
//! at the panic, and `unwind` ends it when the panic leaves `block_on` on the
//! thread of `main`, as it does in a release build of a workspace that sets no
//! panic strategy. The Aosabi workspace and its twins set `abort`. The core
//! refuses and never panics on input.

#[cfg(debug_assertions)]
use core::future::Future;
use core::{
    convert::Infallible,
    future::poll_fn,
    pin::Pin,
    task::{
        Context,
        Poll, //
    }, //
};
#[cfg(debug_assertions)]
use std::panic::{
    AssertUnwindSafe,
    catch_unwind, //
};
use std::rc::Rc;

use aosabi_core::BootedApp;
use aosabi_http::{
    message::{
        BodyError,
        FirstWrite,
        IncomingBody,
        NodeBody,
        NodeIncoming,
        NodeOutgoing, //
    },
    node::{
        status_text,
        value_bytes, //
    },
};
use bytes::Bytes;
use http::{
    HeaderName,
    HeaderValue,
    Request,
    Response,
    StatusCode, //
};
use hyper::{
    body::{
        Body,
        Incoming, //
    },
    ext::{
        HeaderCaseMap,
        NODE_METHODS,
        NodeAnswer,
        NodeHead,
        NodeVersion,
        RawTarget,
        ReasonPhrase, //
    },
};

use crate::{
    runtime::{
        LocalBoxFuture,
        spawn_local, //
    },
    serve_body::ServeBody,
};

/// The service of one connection over the booted app.
#[derive(Clone)]
pub struct NodeHandler {
    /// The app every listener shares.
    app: Rc<BootedApp>,
    /// Whether the connection runs inside TLS.
    secure: bool,
}

impl NodeHandler {
    /// Returns the service of a connection to `app`, inside TLS when `secure`.
    #[must_use]
    pub fn new(app: Rc<BootedApp>, secure: bool) -> Self {
        Self { app, secure }
    }

    /// Returns the scheme of the connection, `https` inside TLS.
    fn scheme(&self) -> &'static [u8] {
        if self.secure { b"https" } else { b"http" }
    }

    /// Returns the request the adapter reads for `request`, or `None` for a
    /// method outside Node's table, which the HTTP/2 server alone hands over.
    #[must_use]
    pub fn incoming(&self, request: Request<Incoming>) -> Option<NodeIncoming> {
        let method = NODE_METHODS
            .iter()
            .copied()
            .find(|name| *name == request.method().as_str())?;
        let (parts, body) = request.into_parts();
        let fields = parts
            .headers
            .iter()
            .map(|(name, value)| (name.as_str().as_bytes().to_vec(), value.as_bytes().to_vec()))
            .collect();
        let Some(head) = parts.extensions.get::<NodeHead>() else {
            let body: Box<dyn IncomingBody> = Box::new(HyperBody {
                body: Some(body),
                drain: true,
            });
            return Some(self.incoming_h2(method, &parts, fields, body));
        };
        let body: Box<dyn IncomingBody> = Box::new(HyperBody {
            body: Some(body),
            drain: false,
        });
        let raw_target = parts.extensions.get::<RawTarget>().map_or_else(
            || parts.uri.to_string().into_bytes(),
            |target| target.0.to_vec(),
        );
        let version = match head.version {
            NodeVersion::Http09 => (0, 9),
            NodeVersion::Http10 => (1, 0),
            NodeVersion::Http11 => (1, 1),
            NodeVersion::Http20 => (2, 0),
        };
        Some(NodeIncoming {
            method,
            raw_target,
            version,
            http2: false,
            scheme: self.scheme().to_vec(),
            fields,
            host: head.host.as_ref().map(|host| host.to_vec()),
            body,
        })
    }

    /// Returns the request of the HTTP/2 server the adapter reads.
    ///
    /// The target is the `:path`, the scheme the `:scheme` that `h2` keeps in
    /// the URI whenever the request carries `:authority`, and the connection's
    /// otherwise, and the host the `:authority` and else the first `host`
    /// field, as Node's `getAuthority` reads it.
    fn incoming_h2(
        &self,
        method: &'static str,
        parts: &http::request::Parts,
        fields: Vec<(Vec<u8>, Vec<u8>)>,
        body: Box<dyn IncomingBody>,
    ) -> NodeIncoming {
        let raw_target = parts
            .uri
            .path_and_query()
            .map_or_else(Vec::new, |path| path.as_str().as_bytes().to_vec());
        let scheme = parts.uri.scheme_str().map_or_else(
            || self.scheme().to_vec(),
            |scheme| scheme.as_bytes().to_vec(),
        );
        let host = parts
            .uri
            .authority()
            .map(|authority| authority.as_str().as_bytes().to_vec())
            .or_else(|| {
                parts
                    .headers
                    .get(http::header::HOST)
                    .map(|host| host.as_bytes().to_vec())
            });
        NodeIncoming {
            method,
            raw_target,
            version: (2, 0),
            http2: true,
            scheme,
            fields,
            host,
            body,
        }
    }
}

impl hyper::service::Service<Request<Incoming>> for NodeHandler {
    type Response = Response<ServeBody>;
    type Error = Infallible;
    type Future = LocalBoxFuture<'static, Result<Response<ServeBody>, Infallible>>;

    fn call(&self, request: Request<Incoming>) -> Self::Future {
        let method = request.method().clone();
        let Some(incoming) = self.incoming(request) else {
            return Box::pin(async { Ok(refused()) });
        };
        let app = Rc::clone(&self.app);
        let answer =
            crate::node_adapter::serve(incoming, move |request, env| app.fetch(request, env, None));
        Box::pin(async move {
            #[cfg(debug_assertions)]
            let answer = if let Some(answer) = CatchPanic(Some(answer)).await {
                answer
            } else {
                let rejected = crate::node_adapter::handle_fetch_error("Error");
                crate::node_adapter::outgoing(rejected).await
            };
            #[cfg(not(debug_assertions))]
            let answer = answer.await;
            Ok(response_of(answer, &method))
        })
    }
}

/// A future whose panic ends it with `None` in place of unwinding further,
/// in a build with debug assertions alone.
///
/// The future is dropped at its panic and never polled again, so no broken
/// state is read after it.
#[cfg(debug_assertions)]
struct CatchPanic<F>(Option<F>);

#[cfg(debug_assertions)]
impl<F: Future + Unpin> Future for CatchPanic<F> {
    type Output = Option<F::Output>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let Some(future) = self.0.as_mut() else {
            return Poll::Ready(None);
        };
        match catch_unwind(AssertUnwindSafe(|| Pin::new(future).poll(cx))) {
            Ok(Poll::Pending) => Poll::Pending,
            Ok(Poll::Ready(output)) => Poll::Ready(Some(output)),
            Err(_) => {
                self.0 = None;
                Poll::Ready(None)
            }
        }
    }
}

/// Returns the answer to a method outside Node's table, a 400 with no body.
fn refused() -> Response<ServeBody> {
    let mut response = Response::new(ServeBody::new(NodeBody::Empty));
    *response.status_mut() = StatusCode::BAD_REQUEST;
    response
}

/// Returns the response hyper writes for the answer `answer` of the adapter
/// to a request of `method`.
///
/// A value goes out in the bytes the first write gives it, whether the answer
/// has a body in Node's sense, which is not the case for a `HEAD` request and
/// for a status of 1xx, 204 or 304. The driver of a stream body is spawned
/// here, ahead of the first poll of the body.
#[must_use]
pub fn response_of(answer: NodeOutgoing, method: &http::Method) -> Response<ServeBody> {
    let NodeOutgoing {
        status,
        reason,
        record,
        mut body,
        first_write,
        cleanup_incoming,
        destroy,
    } = answer;
    let code = StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let has_body = *method != http::Method::HEAD
        && !code.is_informational()
        && !matches!(code, StatusCode::NO_CONTENT | StatusCode::NOT_MODIFIED);
    if let NodeBody::Stream(source) = &mut body
        && let Some(driver) = source.take_driver()
    {
        spawn_local(driver);
    }
    let mut response = Response::new(ServeBody::new(body));
    *response.status_mut() = code;
    let reason = reason.unwrap_or_else(|| status_text(code.as_u16()));
    if let Ok(reason) = ReasonPhrase::try_from(reason.as_bytes()) {
        response.extensions_mut().insert(reason);
    }
    let mut spellings = HeaderCaseMap::new();
    for line in &record {
        let Ok(name) = HeaderName::from_bytes(line.name.as_bytes()) else {
            continue;
        };
        let bytes = value_bytes(&line.value, first_write, has_body);
        let Ok(value) = HeaderValue::from_bytes(&bytes) else {
            continue;
        };
        spellings.append(&name, Bytes::copy_from_slice(line.name.as_bytes()));
        response.headers_mut().append(name, value);
    }
    response.extensions_mut().insert(spellings);
    response.extensions_mut().insert(NodeAnswer {
        first_write: match first_write {
            FirstWrite::Bytes => hyper::ext::FirstWrite::Bytes,
            FirstWrite::End => hyper::ext::FirstWrite::End,
            FirstWrite::Text => hyper::ext::FirstWrite::Text,
            FirstWrite::Flush => hyper::ext::FirstWrite::Flush,
        },
        cleanup_incoming,
        destroy,
    });
    response
}

/// The body of a request as hyper reads it.
///
/// The body of a stream of the HTTP/2 server that the application drops
/// before its end is read to its end by a local task, which hands hyper back
/// the window of every frame, so the stream stays open on the side of the
/// client after the answer, half closed as RFC 9113 section 5.1 names it, as
/// it does for Node's `http2` server, and a frame the client sends on it
/// later meets the rules of its state. Dropped unread, the stream would end
/// with a reset of `h2`'s own, after which `h2` ignores what the client sends
/// on it.
struct HyperBody {
    /// The body, which the drop hands to the task that drains it.
    body: Option<Incoming>,
    /// Whether a drop before the end drains the rest, for HTTP/2 alone.
    drain: bool,
}

impl Drop for HyperBody {
    fn drop(&mut self) {
        if let Some(body) = self.body.take()
            && self.drain
            && !body.is_end_stream()
        {
            spawn_local(Box::pin(drain(body)));
        }
    }
}

/// Reads `body` to its end or its error and drops every frame.
async fn drain(mut body: Incoming) {
    while let Some(Ok(_)) = poll_fn(|cx| Pin::new(&mut body).poll_frame(cx)).await {}
}

impl IncomingBody for HyperBody {
    fn poll_chunk(&mut self, cx: &mut Context<'_>) -> Poll<Option<Result<Vec<u8>, BodyError>>> {
        let Some(body) = self.body.as_mut() else {
            return Poll::Ready(None);
        };
        loop {
            match Pin::new(&mut *body).poll_frame(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(None) => return Poll::Ready(None),
                Poll::Ready(Some(Err(error))) => return Poll::Ready(Some(Err(body_error(&error)))),
                Poll::Ready(Some(Ok(frame))) => {
                    // A trailer section carries no byte of the body, and the
                    // Node parser drops it before it gets here (quirk Q32).
                    if let Ok(data) = frame.into_data() {
                        return Poll::Ready(Some(Ok(data.to_vec())));
                    }
                }
            }
        }
    }
}

/// Returns why the body of a request ended, as hyper reports it.
fn body_error(error: &hyper::Error) -> BodyError {
    if error.is_parse_too_large() {
        BodyError::TooLarge
    } else if error.is_parse() {
        BodyError::Refused
    } else {
        BodyError::Closed
    }
}
