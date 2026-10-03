// SPDX-License-Identifier: MIT
/*
 * aosabi-fetch: the TLS stream of a client over rustls.
 *
 * No analogue on the reference side. The adapter has the shape of the
 * TlsStream of crates/aosabi-serve/src/tls.rs over a client connection in
 * place of a server connection.
 */

//! `TlsStream` and its handshake.
//!
//! The stream runs a `rustls::ClientConnection` over a socket of the runtime,
//! sans IO, through `read_tls`, `process_new_packets`, `reader`, `writer` and
//! `write_tls`, with no `unsafe` and no `tokio-rustls`. A read of the socket
//! lands in a buffer on the stack, and the TLS bytes rustls hands out wait in
//! the stream until the socket takes them. The TLS bytes of the socket go to
//! rustls only while it holds no plaintext, since rustls refuses a read of TLS
//! once its plaintext passes 16,384 bytes. The rest of a read of the socket
//! waits in the stream, and the next fill hands it over before it reads the
//! socket again, as `tokio-rustls` reads only while rustls wants to read.
//!
//! A server that closes the connection with no close notification ends the
//! plaintext with `UnexpectedEof` and not with an end of the stream, so the
//! reader of the answer sees an incomplete close, which RFC 9112 section 9.8
//! asks a client to tell apart from a complete one.

use core::{
    future::poll_fn,
    pin::Pin,
    task::{
        Context,
        Poll, //
    },
};
use std::{
    io::{
        self,
        ErrorKind,
        Read as _,
        Write as _, //
    },
    sync::Arc, //
};

use aosabi_serve::runtime::{
    LocalBoxFuture,
    net::Socket, //
};
use hyper::rt::ReadBufCursor;
use rustls::{
    ClientConfig,
    ClientConnection, //
};
use rustls_pki_types::ServerName;

/// The plaintext bytes one read takes at most.
const PLAIN_READ: usize = 16_384;

/// The TLS bytes one read of the socket takes at most. A record that is
/// longer arrives over two reads, and rustls keeps the part it holds until
/// the rest comes.
const TLS_READ: usize = 16_384;

/// A TLS connection of a client over a socket of the runtime.
pub struct TlsStream<S> {
    /// The socket.
    stream: S,
    /// The connection of rustls.
    conn: ClientConnection,
    /// TLS bytes rustls handed out that the socket has not taken yet.
    out: Vec<u8>,
    /// TLS bytes the socket gave that rustls has not taken yet.
    input: Vec<u8>,
    /// Whether the socket reached its end.
    eof: bool,
    /// Whether the close notification is queued.
    closing: bool,
}

impl<S: Socket + Unpin + 'static> TlsStream<S> {
    /// Runs the handshake of a connection to the server `name` over `stream`
    /// under `config`.
    ///
    /// The bound of the call covers the handshake, so the handshake has no
    /// timeout of its own.
    ///
    /// # Errors
    ///
    /// The future answers the error of the handshake, a certificate the root
    /// store does not trust among them, as an error of the kind
    /// `InvalidData`, and the error of the socket.
    pub fn handshake(
        stream: S,
        config: Arc<ClientConfig>,
        name: ServerName<'static>,
    ) -> LocalBoxFuture<'static, io::Result<Self>> {
        let conn = ClientConnection::new(config, name);
        Box::pin(async move {
            let mut tls = Self {
                stream,
                conn: conn.map_err(invalid)?,
                out: Vec::new(),
                input: Vec::new(),
                eof: false,
                closing: false,
            };
            poll_fn(|cx| tls.poll_handshake(cx)).await?;
            Ok(tls)
        })
    }
}

impl<S: Socket> TlsStream<S> {
    /// Returns the protocol the server chose through ALPN.
    #[must_use]
    pub fn alpn(&self) -> Option<&[u8]> {
        self.conn.alpn_protocol()
    }

    /// Returns the socket.
    #[must_use]
    pub fn get_ref(&self) -> &S {
        &self.stream
    }

    /// Returns whether the stream holds nothing of either direction between
    /// two answers: no byte of TLS to send or to hand rustls, no plaintext,
    /// no handshake in flight, no end of the socket and no close notification
    /// of either side.
    ///
    /// The connection of rustls wants to read while it holds no plaintext and
    /// has no close notification of its peer, so a connection whose stream
    /// holds nothing and whose socket holds no byte can carry the next
    /// request.
    #[must_use]
    pub fn holds_nothing(&self) -> bool {
        self.out.is_empty()
            && self.input.is_empty()
            && !self.eof
            && !self.closing
            && !self.conn.is_handshaking()
            && self.conn.wants_read()
            && !self.conn.wants_write()
    }

    /// Drives the handshake until it ended and its last flight went out.
    fn poll_handshake(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        loop {
            let flushed = self.poll_flush_tls(cx)?;
            if !self.conn.is_handshaking() {
                return flushed.map(Ok);
            }
            if flushed.is_pending() {
                return Poll::Pending;
            }
            match self.poll_fill(cx)? {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(()) if self.eof && self.conn.is_handshaking() => {
                    return Poll::Ready(Err(io::Error::new(
                        ErrorKind::UnexpectedEof,
                        "the server closed the connection during the handshake",
                    )));
                }
                Poll::Ready(()) => {}
            }
        }
    }

    /// Writes the TLS bytes rustls holds until the socket would block.
    fn poll_flush_tls(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        loop {
            if !self.out.is_empty() {
                match self.stream.poll_write(cx, &self.out) {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                    Poll::Ready(Ok(0)) => return Poll::Ready(Err(ErrorKind::WriteZero.into())),
                    Poll::Ready(Ok(n)) => {
                        self.out.drain(..n);
                    }
                }
                continue;
            }
            if !self.conn.wants_write() {
                return Poll::Ready(Ok(()));
            }
            self.conn.write_tls(&mut self.out)?;
        }
    }

    /// Reads TLS bytes from the socket into rustls and processes them.
    ///
    /// The bytes go to rustls until it wants no more, which it says while it
    /// holds plaintext, and those it does not take wait in `input` for the
    /// next fill. A record rustls refuses queues its alert, which goes out on
    /// a best effort, and answers the error.
    fn poll_fill(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        if self.input.is_empty() && !self.eof {
            let mut stack = [0u8; TLS_READ];
            match self.stream.poll_read(cx, &mut stack) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                Poll::Ready(Ok(0)) => self.eof = true,
                Poll::Ready(Ok(n)) => self.input.extend_from_slice(&stack[..n]),
            }
        }
        if self.eof && self.input.is_empty() {
            // An empty read tells rustls that the peer closed.
            let mut end: &[u8] = &[];
            self.conn.read_tls(&mut end)?;
        }
        while !self.input.is_empty() && self.conn.wants_read() {
            let mut slice: &[u8] = &self.input;
            let n = self.conn.read_tls(&mut slice)?;
            self.input.drain(..n);
            self.process(cx)?;
            if n == 0 {
                break;
            }
        }
        self.process(cx)?;
        Poll::Ready(Ok(()))
    }

    /// Processes the records rustls holds, and on a refusal sends its alert
    /// on a best effort and answers the error.
    fn process(&mut self, cx: &mut Context<'_>) -> io::Result<()> {
        if let Err(error) = self.conn.process_new_packets() {
            let _ = self.conn.write_tls(&mut self.out);
            let _ = self.poll_flush_tls(cx);
            return Err(invalid(error));
        }
        Ok(())
    }
}

impl<S: Socket + Unpin> hyper::rt::Read for TlsStream<S> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        mut buf: ReadBufCursor<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let mut told_end = false;
        loop {
            let mut stack = [0u8; PLAIN_READ];
            let want = buf.remaining().min(PLAIN_READ);
            match this.conn.reader().read(&mut stack[..want]) {
                Ok(n) => {
                    buf.put_slice(&stack[..n]);
                    return Poll::Ready(Ok(()));
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => {}
                Err(error) => return Poll::Ready(Err(error)),
            }
            if told_end {
                return Poll::Ready(Err(io::Error::new(
                    ErrorKind::UnexpectedEof,
                    "the server closed the connection with no close notification",
                )));
            }
            if let Poll::Ready(Err(error)) = this.poll_flush_tls(cx) {
                return Poll::Ready(Err(error));
            }
            match this.poll_fill(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                Poll::Ready(Ok(())) => {}
            }
            told_end = this.eof && this.input.is_empty();
        }
    }
}

impl<S: Socket + Unpin> hyper::rt::Write for TlsStream<S> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        if this.poll_flush_tls(cx)?.is_pending() {
            return Poll::Pending;
        }
        let n = this.conn.writer().write(buf)?;
        if let Poll::Ready(Err(error)) = this.poll_flush_tls(cx) {
            return Poll::Ready(Err(error));
        }
        Poll::Ready(Ok(n))
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        this.conn.writer().flush()?;
        this.poll_flush_tls(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if !this.closing {
            this.conn.send_close_notify();
            this.closing = true;
        }
        if this.poll_flush_tls(cx)?.is_pending() {
            return Poll::Pending;
        }
        Poll::Ready(this.stream.shutdown_write())
    }

    fn is_write_vectored(&self) -> bool {
        false
    }
}

/// Returns `error` of rustls as an IO error of the kind `InvalidData`.
fn invalid(error: rustls::Error) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, error)
}
