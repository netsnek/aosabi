// SPDX-License-Identifier: MIT
/*
 * aosabi-serve: HTTP/2 over the server of hyper and the fork of h2.
 *
 * No analogue on the reference side.
 */

//! `H2Config` and `serve_h2`.
//!
//! HTTP/2 serves the same [`NodeHandler`] over the same adapter, so the
//! record, the status and the body are the ones HTTP/1.1 writes. The answer is
//! mapped for HTTP/2: the connection specific lines `connection`,
//! `keep-alive`, `proxy-connection`, `transfer-encoding` and `upgrade` are
//! dropped, as RFC 9113 section 8.2.2 prohibits them, every name is lower case
//! already, no reason phrase goes out, which HTTP/2 does not carry, `date` is
//! added by hyper's `auto_date_header`, and the body keeps its inexact size
//! hint, so hyper adds no `content-length` of its own and the line of the
//! record stands. A `CONNECT` reaches the service as an ordinary request after
//! hyper's C11, and it is answered 405 with no body and the end of the stream
//! before the adapter, as Node's compatibility layer does under
//! `@hono/node-server`. HTTP/2 carries no quirk, so what differs from Node's
//! `http2` server is a finding of `33_http2_tls`.

use core::convert::Infallible;
use std::rc::Rc;

use aosabi_core::BootedApp;
use aosabi_http::message::NodeBody;
use http::{
    Method,
    Request,
    Response,
    StatusCode, //
    header::{
        CONNECTION,
        HeaderName,
        TRANSFER_ENCODING,
        UPGRADE, //
    },
};
use hyper::{
    body::Incoming,
    ext::{
        HeaderCaseMap,
        ReasonPhrase, //
    },
    server::conn::http2,
    service::Service,
};

use crate::{
    node_handler::NodeHandler,
    runtime::{
        Handle,
        LocalBoxFuture, //
        hyper_rt::{
            LocalExec,
            RuntimeTimer, //
        },
    },
    serve_body::ServeBody,
};

/// The settings of hyper's HTTP/2 server on a Linux card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct H2Config {
    /// The streams a client holds open at once, `SETTINGS_MAX_CONCURRENT_STREAMS`.
    pub max_concurrent_streams: u32,
    /// The bytes of a header list, `SETTINGS_MAX_HEADER_LIST_SIZE`.
    pub max_header_list_size: u32,
    /// The initial window of a stream.
    pub stream_window: u32,
    /// The initial window of the connection.
    pub connection_window: u32,
    /// The bytes a stream buffers to send.
    pub max_send_buf_size: usize,
    /// The streams reset before the server accepted them, hyper's bound.
    pub max_pending_accept_reset_streams: usize,
    /// The streams the server resets for a local error, hyper's bound.
    pub max_local_error_reset_streams: usize,
}

impl H2Config {
    /// The settings of the Linux cards.
    ///
    /// At most 100 concurrent streams, which Cloudflare's edge respects, a
    /// header list of at most 65,535 bytes, the `maxHeaderListSize` of Node's
    /// HTTP/2 server, a stream window of 65,535 bytes and a connection window
    /// of 1 MiB, a send buffer of 64 KiB per stream, and hyper's reset bounds,
    /// 20 and 1,024.
    pub const LINUX: Self = Self {
        max_concurrent_streams: 100,
        max_header_list_size: 65_535,
        stream_window: 65_535,
        connection_window: 1_048_576,
        max_send_buf_size: 65_536,
        max_pending_accept_reset_streams: 20,
        max_local_error_reset_streams: 1_024,
    };
}

/// Serves HTTP/2 over `io` for `app` with the settings `config`, inside TLS
/// when `secure`.
///
/// No keep alive ping is sent, and no idle connection is closed by a timer. A
/// stream is a local task of the runtime through [`LocalExec`].
pub fn serve_h2<I>(
    io: I,
    app: Rc<BootedApp>,
    config: &H2Config,
    secure: bool,
) -> LocalBoxFuture<'static, ()>
where
    I: hyper::rt::Read + hyper::rt::Write + Unpin + 'static,
{
    let mut builder = http2::Builder::new(LocalExec);
    builder
        .max_concurrent_streams(config.max_concurrent_streams)
        .max_header_list_size(config.max_header_list_size)
        .initial_stream_window_size(config.stream_window)
        .initial_connection_window_size(config.connection_window)
        .max_send_buf_size(config.max_send_buf_size)
        .max_pending_accept_reset_streams(config.max_pending_accept_reset_streams)
        .max_local_error_reset_streams(config.max_local_error_reset_streams)
        .auto_date_header(true);
    if let Ok(handle) = Handle::current() {
        builder.timer(RuntimeTimer::of(&handle));
    }
    let service = H2Handler {
        inner: NodeHandler::new(app, secure),
    };
    let connection = builder.serve_connection(io, service);
    Box::pin(async move {
        // The end of a connection answers nothing further, so its error is
        // the end.
        let _ = connection.await;
    })
}

/// Returns the answer `answer` of the adapter mapped for HTTP/2.
///
/// The names are lower case already, the connection specific lines are
/// dropped, the reason phrase and the spellings of the record go, which
/// HTTP/2 carries neither of, and an empty body ends the stream with the
/// head.
#[must_use]
pub fn h2_response(answer: Response<ServeBody>) -> Response<ServeBody> {
    let (mut parts, body) = answer.into_parts();
    for name in [
        CONNECTION,
        HeaderName::from_static("keep-alive"),
        HeaderName::from_static("proxy-connection"),
        TRANSFER_ENCODING,
        UPGRADE,
    ] {
        parts.headers.remove(name);
    }
    parts.extensions.remove::<ReasonPhrase>();
    parts.extensions.remove::<HeaderCaseMap>();
    Response::from_parts(parts, body.for_http2())
}

/// The service of an HTTP/2 connection: `CONNECT` answered 405, and every
/// other request through the node handler with its answer mapped.
#[derive(Clone)]
struct H2Handler {
    /// The handler both protocols share.
    inner: NodeHandler,
}

impl Service<Request<Incoming>> for H2Handler {
    type Response = Response<ServeBody>;
    type Error = Infallible;
    type Future = LocalBoxFuture<'static, Result<Response<ServeBody>, Infallible>>;

    fn call(&self, request: Request<Incoming>) -> Self::Future {
        if request.method() == Method::CONNECT {
            let mut answer = Response::new(ServeBody::new(NodeBody::Empty).for_http2());
            *answer.status_mut() = StatusCode::METHOD_NOT_ALLOWED;
            return Box::pin(async move { Ok(answer) });
        }
        let answer = self.inner.call(request);
        Box::pin(async move { answer.await.map(h2_response) })
    }
}
