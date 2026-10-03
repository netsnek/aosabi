// SPDX-License-Identifier: MIT
/*
 * aosabi-serve: the stream type of every socket and the selection of the
 * protocol of an accepted socket.
 *
 * No analogue on the reference side.
 */

//! `Stream` and `serve_accepted`.
//!
//! | Features of the build | The plain listener | With a certificate configured |
//! | --- | --- | --- |
//! | None | HTTP/1.1 through `node1`, the byte contract | Not available |
//! | `tls` | HTTP/1.1 through `node1` | TLS with ALPN `h2` to HTTP/2 and `http/1.1` or none to `node1` |
//! | `h2c` | HTTP/1.1, and a connection whose first 24 bytes are the preface to HTTP/2 | Not available |
//! | `tls` and `h2c` | As the row of `h2c` | As the row of `tls`, no preface detection inside TLS |
//!
//! Under `h2c` the detection hands a connection to `node1` at the first byte
//! that differs from the preface, so a request that does not start with `P`
//! meets `node1` at its first read, and only one that starts like the preface
//! waits for the bytes that decide it. The wait ends at the head deadline
//! `node1` keeps for a connection it takes at the accept, and a socket that
//! reaches it undecided, a silent one among them, gets Node's 408 and is
//! closed, as `node1` answers a head past its timer. A `PRI * HTTP/2.0`
//! request, which Node refuses with 400 (quirk Q18), becomes an HTTP/2
//! connection under `h2c`, the one named divergence of the opt in, which is why
//! the twins of the byte contract build without it. The `Upgrade: h2c` of
//! HTTP/1.1 is not served, because RFC 9113 deprecates it, and a request that
//! carries it is an ordinary request of `node1` (quirk Q40).
//!
//! Every socket a card serves goes through one stream type, [`Stream`], a
//! plain TCP socket, a Unix socket of HTTP(Unix Socket) or, in a build with
//! `tls`, a TCP socket inside TLS, so the Node connection and the HTTP/2
//! server of the forks are built once for all of them, and the opt in adds
//! rustls with its provider and not a second copy of the two protocols.

#[cfg(feature = "h2c")]
use core::{
    future::poll_fn,
    time::Duration, //
};
use core::{
    pin::Pin,
    task::{
        Context,
        Poll, //
    },
};
use std::{
    io,
    rc::Rc, //
};

use aosabi_core::BootedApp;
#[cfg(feature = "h2c")]
use hyper::server::conn::node1::NodeConfig;

#[cfg(feature = "h2c")]
use crate::runtime::{
    hyper_rt::{
        RuntimeSleep,
        RuntimeTimer, //
    },
    net::Socket, //
};
use crate::{
    runtime::{
        LocalBoxFuture,
        hyper_rt::HyperIo,
        net::{
            TcpStream,
            UnixStream, //
        },
    },
    serve::serve_stream,
};

/// The TLS configuration of a listener under the feature `tls`.
#[cfg(feature = "tls")]
pub use crate::tls::TlsConfig;

/// The TLS configuration of a listener in a build without the feature `tls`,
/// a type with no value, so no listener of such a build serves TLS.
#[cfg(not(feature = "tls"))]
#[derive(Clone, Copy, Debug)]
pub enum TlsConfig {}

/// The preface every HTTP/2 connection by prior knowledge starts with (RFC
/// 9113 section 3.4).
#[cfg(feature = "h2c")]
pub const PREFACE: &[u8; 24] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";

/// Node's answer to a head past its timer, which `node1` writes to a connection
/// with no answer in flight.
#[cfg(feature = "h2c")]
const REQUEST_TIMEOUT: &[u8] = b"HTTP/1.1 408 Request Timeout\r\nConnection: close\r\n\r\n";

/// The stream every listener of a card serves over.
///
/// The Node connection of the fork and the HTTP/2 server take this one type,
/// so their code is built once for every socket, and each read and write of a
/// connection matches the variant. The bytes on the wire never pass through
/// the enum, which only picks the socket a poll reads or writes.
///
/// Every variant but the TCP socket is boxed, so the type has the size of a
/// TCP socket alone, and a connection of the TCP listener, the one a card
/// serves most, holds the stream it would hold without the other sockets.
pub enum Stream {
    /// A socket of the plain TCP listener.
    Tcp(HyperIo<TcpStream>),
    /// A socket of HTTP(Unix Socket), which `aosabi-carrier-unix` accepts.
    Unix(Box<HyperIo<UnixStream>>),
    /// A socket of the TLS listener after its handshake, boxed, since its
    /// session holds kilobytes a plain socket does not.
    #[cfg(feature = "tls")]
    Tls(Box<crate::tls::TlsStream<TcpStream>>),
}

impl hyper::rt::Read for Stream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: hyper::rt::ReadBufCursor<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Tcp(io) => Pin::new(io).poll_read(cx, buf),
            Self::Unix(io) => Pin::new(&mut **io).poll_read(cx, buf),
            #[cfg(feature = "tls")]
            Self::Tls(io) => Pin::new(&mut **io).poll_read(cx, buf),
        }
    }
}

impl hyper::rt::Write for Stream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            Self::Tcp(io) => Pin::new(io).poll_write(cx, buf),
            Self::Unix(io) => Pin::new(&mut **io).poll_write(cx, buf),
            #[cfg(feature = "tls")]
            Self::Tls(io) => Pin::new(&mut **io).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Tcp(io) => Pin::new(io).poll_flush(cx),
            Self::Unix(io) => Pin::new(&mut **io).poll_flush(cx),
            #[cfg(feature = "tls")]
            Self::Tls(io) => Pin::new(&mut **io).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Tcp(io) => Pin::new(io).poll_shutdown(cx),
            Self::Unix(io) => Pin::new(&mut **io).poll_shutdown(cx),
            #[cfg(feature = "tls")]
            Self::Tls(io) => Pin::new(&mut **io).poll_shutdown(cx),
        }
    }

    fn is_write_vectored(&self) -> bool {
        false
    }
}

/// Serves the accepted socket `stream` for `app`, with TLS when `tls` names a
/// configuration, in the protocol section What a Linux card serves selects.
#[must_use]
pub fn serve_accepted(
    stream: TcpStream,
    app: Rc<BootedApp>,
    tls: Option<Rc<TlsConfig>>,
) -> LocalBoxFuture<'static, ()> {
    Box::pin(async move {
        if let Some(tls) = tls {
            #[cfg(feature = "tls")]
            {
                serve_tls(stream, app, &tls).await;
                return;
            }
            // A build without the feature has no TLS configuration to serve.
            #[cfg(not(feature = "tls"))]
            match *tls {}
        }
        serve_plain(stream, app).await;
    })
}

/// Serves a socket of a plain listener, HTTP/1.1 through `node1`.
#[cfg(not(feature = "h2c"))]
async fn serve_plain(stream: TcpStream, app: Rc<BootedApp>) {
    serve_stream(Stream::Tcp(HyperIo::new(stream)), app, false).await;
}

/// Serves a socket of a plain listener, HTTP/2 when its first bytes are the
/// preface and HTTP/1.1 through `node1` at the first byte that differs.
#[cfg(feature = "h2c")]
async fn serve_plain(stream: TcpStream, app: Rc<BootedApp>) {
    let deadline = RuntimeTimer::current()
        .ok()
        .map(|timer| timer.sleep_for(preface_deadline()));
    let (h2, ahead) = match read_preface(&stream, deadline).await {
        Ok(Some(decided)) => decided,
        Ok(None) => {
            // The 408 goes out on a best effort, and the drop closes the socket.
            let _ = poll_fn(|cx| stream.poll_write(cx, REQUEST_TIMEOUT)).await;
            return;
        }
        Err(_) => return,
    };
    let io = Stream::Tcp(HyperIo::with_ahead(stream, ahead));
    if h2 {
        crate::h2::serve_h2(io, app, &crate::h2::H2Config::LINUX, false).await;
    } else {
        serve_stream(io, app, false).await;
    }
}

/// Returns the head deadline `node1` keeps under `NodeConfig::NODE` for a
/// connection it takes at the accept, the first check of its interval strictly
/// after the head timeout, 90 seconds.
#[cfg(feature = "h2c")]
fn preface_deadline() -> Duration {
    let config = NodeConfig::NODE;
    let timeout = u64::from(config.headers_timeout_ms.min(config.request_timeout_ms));
    let interval = u64::from(config.check_interval_ms.max(1));
    Duration::from_millis((timeout / interval + 1) * interval)
}

/// Reads until the bytes decide whether they are the preface, and answers
/// whether they are with every byte read, or `None` when `deadline` passes
/// first.
#[cfg(feature = "h2c")]
async fn read_preface(
    stream: &TcpStream,
    mut deadline: Option<RuntimeSleep>,
) -> std::io::Result<Option<(bool, Vec<u8>)>> {
    let mut ahead = Vec::new();
    let mut read = vec![0u8; crate::runtime::hyper_rt::READ_SIZE];
    loop {
        let read_or_deadline = poll_fn(|cx| match stream.poll_read(cx, &mut read) {
            Poll::Pending => match deadline.as_mut().map(|sleep| Pin::new(sleep).poll(cx)) {
                Some(Poll::Ready(())) => Poll::Ready(Ok(None)),
                _ => Poll::Pending,
            },
            Poll::Ready(result) => Poll::Ready(result.map(Some)),
        });
        let Some(n) = read_or_deadline.await? else {
            return Ok(None);
        };
        ahead.extend_from_slice(&read[..n]);
        let seen = ahead.len().min(PREFACE.len());
        if n == 0 || ahead[..seen] != PREFACE[..seen] {
            return Ok(Some((false, ahead)));
        }
        if seen == PREFACE.len() {
            return Ok(Some((true, ahead)));
        }
    }
}

/// Serves a socket of a TLS listener: the handshake, then `h2` to HTTP/2 and
/// `http/1.1` or no ALPN to `node1`, with no preface detection inside TLS.
#[cfg(feature = "tls")]
async fn serve_tls(stream: TcpStream, app: Rc<BootedApp>, tls: &TlsConfig) {
    let Ok(stream) = crate::tls::TlsStream::handshake(stream, tls).await else {
        return;
    };
    let h2 = stream.alpn() == Some(b"h2");
    let io = Stream::Tls(Box::new(stream));
    if h2 {
        crate::h2::serve_h2(io, app, &crate::h2::H2Config::LINUX, true).await;
    } else {
        serve_stream(io, app, true).await;
    }
}
