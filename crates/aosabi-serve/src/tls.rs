// SPDX-License-Identifier: MIT
/*
 * aosabi-serve: TLS over rustls, offering h2 and http/1.1 through ALPN.
 *
 * No analogue on the reference side.
 */

//! `TlsConfig` and `TlsStream`.
//!
//! The configuration is a certificate chain, the leaf first and its
//! intermediates after it, and a private key as PKCS#1, PKCS#8 or SEC1, RSA or
//! ECDSA, both PEM files of any CA: a Cloudflare Origin CA certificate behind
//! Cloudflare, or a public one such as the `fullchain.pem` and `privkey.pem`
//! certbot writes for browsers that reach the card directly. The settings are
//! TLS 1.3 alone with the cipher suites of the provider `ring`, no client
//! certificates, one chain whatever SNI a client sends, and ALPN `h2` and
//! `http/1.1`, so a client that offers ALPN with neither protocol is refused in
//! the handshake with the alert `no_application_protocol` and one that offers
//! none is served without it.
//!
//! The stream runs a `rustls::ServerConnection` over a socket of the runtime,
//! sans IO, through `read_tls`, `process_new_packets`, `reader`, `writer` and
//! `write_tls`, with no `unsafe` and no `tokio-rustls`. A read of the socket
//! lands in a buffer on the stack, and the TLS bytes rustls hands out wait in
//! the stream until the socket takes them.
//!
//! A handshake that has not ended [`HANDSHAKE_TIMEOUT`] after the accept ends
//! with `TimedOut`, and the socket closes with no further byte, as Node
//! destroys a socket whose handshake passed its `handshakeTimeout`.

use core::{
    fmt,
    future::poll_fn,
    pin::Pin,
    task::{
        Context,
        Poll, //
    },
    time::Duration,
};
use std::{
    env,
    ffi::OsString,
    fs,
    io::{
        self,
        ErrorKind,
        Read as _,
        Write as _, //
    },
    path::Path,
    sync::Arc,
};

use hyper::rt::ReadBufCursor;
use rustls::{
    ServerConfig,
    ServerConnection, //
    pki_types::{
        CertificateDer,
        PrivateKeyDer,
        pem::PemObject, //
    },
};

use crate::runtime::{
    LocalBoxFuture,
    hyper_rt::RuntimeTimer,
    net::Socket, //
};

/// The variable that names the certificate chain.
pub const CERT_ENV: &str = "AOSABI_TLS_CERT";

/// The variable that names the private key.
pub const KEY_ENV: &str = "AOSABI_TLS_KEY";

/// The protocols a TLS listener offers through ALPN, in its order of
/// preference.
pub const ALPN: [&[u8]; 2] = [b"h2", b"http/1.1"];

/// The time a handshake may take from the accept, the default `handshakeTimeout`
/// of Node's TLS server, 120,000 ms.
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_mins(2);

/// The plaintext bytes one read takes at most.
const PLAIN_READ: usize = 16_384;

/// The TLS bytes one read of the socket takes at most. A record that is
/// longer arrives over two reads, and rustls keeps the part it holds until
/// the rest comes.
const TLS_READ: usize = 16_384;

/// Why the TLS configuration could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TlsConfigError {
    /// One of the two variables is set and the other is not.
    OneOfTwo,
    /// A file could not be read.
    Read(String),
    /// A file holds no chain or no key in PEM.
    Parse(String),
    /// The key does not match the leaf, or rustls refuses the pair.
    Key(String),
}

impl fmt::Display for TlsConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OneOfTwo => write!(
                f,
                "{CERT_ENV} and {KEY_ENV} name the certificate chain and its key together, and one of them is not set"
            ),
            Self::Read(message) => write!(f, "the TLS configuration could not be read: {message}"),
            Self::Parse(message) => write!(f, "the TLS configuration does not parse: {message}"),
            Self::Key(message) => write!(f, "the TLS key does not fit the certificate: {message}"),
        }
    }
}

impl core::error::Error for TlsConfigError {}

/// The TLS configuration of a listener.
#[derive(Clone)]
pub struct TlsConfig {
    /// The configuration of rustls.
    config: Arc<ServerConfig>,
}

impl fmt::Debug for TlsConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TlsConfig").finish_non_exhaustive()
    }
}

impl TlsConfig {
    /// Returns the configuration the environment names, `None` when neither
    /// `AOSABI_TLS_CERT` nor `AOSABI_TLS_KEY` is set.
    ///
    /// # Errors
    ///
    /// Returns [`TlsConfigError::OneOfTwo`] when one variable is set alone,
    /// and what [`TlsConfig::from_files`] returns.
    pub fn from_env() -> Result<Option<Self>, TlsConfigError> {
        let set = |name| env::var_os(name).filter(|value: &OsString| !value.is_empty());
        match (set(CERT_ENV), set(KEY_ENV)) {
            (None, None) => Ok(None),
            (Some(chain), Some(key)) => {
                Self::from_files(Path::new(&chain), Path::new(&key)).map(Some)
            }
            _ => Err(TlsConfigError::OneOfTwo),
        }
    }

    /// Returns the configuration of the chain in the PEM file `chain` and the
    /// key in the PEM file `key`.
    ///
    /// # Errors
    ///
    /// Returns [`TlsConfigError::Read`] for a file that cannot be read,
    /// [`TlsConfigError::Parse`] for a file with no certificate or no key,
    /// and [`TlsConfigError::Key`] for a key rustls refuses beside the leaf.
    pub fn from_files(chain: &Path, key: &Path) -> Result<Self, TlsConfigError> {
        let read = |path: &Path| {
            fs::read(path)
                .map_err(|error| TlsConfigError::Read(format!("{}: {error}", path.display())))
        };
        let chain_pem = read(chain)?;
        let certs = CertificateDer::pem_slice_iter(&chain_pem)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| TlsConfigError::Parse(format!("{}: {error}", chain.display())))?;
        if certs.is_empty() {
            return Err(TlsConfigError::Parse(format!(
                "{}: no certificate",
                chain.display()
            )));
        }
        let key_pem = read(key)?;
        let key_der = PrivateKeyDer::from_pem_slice(&key_pem)
            .map_err(|error| TlsConfigError::Parse(format!("{}: {error}", key.display())))?;
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let mut config = ServerConfig::builder_with_provider(provider)
            .with_protocol_versions(&[&rustls::version::TLS13])
            .map_err(|error| TlsConfigError::Key(error.to_string()))?
            .with_no_client_auth()
            .with_single_cert(certs, key_der)
            .map_err(|error| TlsConfigError::Key(error.to_string()))?;
        config.alpn_protocols = ALPN.iter().map(|protocol| protocol.to_vec()).collect();
        Ok(Self {
            config: Arc::new(config),
        })
    }
}

/// A TLS connection over a socket of the runtime.
pub struct TlsStream<S> {
    /// The socket.
    stream: S,
    /// The connection of rustls.
    conn: ServerConnection,
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
    /// Runs the handshake of a connection over `stream` under `config`.
    ///
    /// # Errors
    ///
    /// The future answers the error of the handshake, a refused ALPN among
    /// them, the error of the socket, and `TimedOut` for a handshake that
    /// has not ended [`HANDSHAKE_TIMEOUT`] after the call.
    pub fn handshake(stream: S, config: &TlsConfig) -> LocalBoxFuture<'static, io::Result<Self>> {
        let conn = ServerConnection::new(Arc::clone(&config.config));
        let mut deadline = RuntimeTimer::current()
            .ok()
            .map(|timer| timer.sleep_for(HANDSHAKE_TIMEOUT));
        Box::pin(async move {
            let mut tls = Self {
                stream,
                conn: conn.map_err(invalid)?,
                out: Vec::new(),
                input: Vec::new(),
                eof: false,
                closing: false,
            };
            poll_fn(|cx| match tls.poll_handshake(cx) {
                Poll::Pending => match deadline.as_mut().map(|sleep| Pin::new(sleep).poll(cx)) {
                    Some(Poll::Ready(())) => Poll::Ready(Err(io::Error::new(
                        ErrorKind::TimedOut,
                        "the handshake passed the handshakeTimeout of Node",
                    ))),
                    _ => Poll::Pending,
                },
                ready @ Poll::Ready(_) => ready,
            })
            .await?;
            Ok(tls)
        })
    }
}

impl<S: Socket> TlsStream<S> {
    /// Returns the protocol the client chose through ALPN.
    #[must_use]
    pub fn alpn(&self) -> Option<&[u8]> {
        self.conn.alpn_protocol()
    }

    /// Returns the socket.
    pub fn get_ref(&self) -> &S {
        &self.stream
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
                Poll::Ready(()) if self.eof => {
                    return Poll::Ready(Err(io::Error::new(
                        ErrorKind::UnexpectedEof,
                        "the client closed the connection during the handshake",
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
    /// A record rustls refuses queues its alert, which goes out on a best
    /// effort, and answers the error.
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
        while !self.input.is_empty() {
            let mut slice: &[u8] = &self.input;
            let n = self.conn.read_tls(&mut slice)?;
            self.input.drain(..n);
            if let Err(error) = self.conn.process_new_packets() {
                let _ = self.conn.write_tls(&mut self.out);
                let _ = self.poll_flush_tls(cx);
                return Poll::Ready(Err(invalid(error)));
            }
            if n == 0 {
                break;
            }
        }
        if let Err(error) = self.conn.process_new_packets() {
            let _ = self.conn.write_tls(&mut self.out);
            let _ = self.poll_flush_tls(cx);
            return Poll::Ready(Err(invalid(error)));
        }
        Poll::Ready(Ok(()))
    }
}

impl<S: Socket + Unpin> hyper::rt::Read for TlsStream<S> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        mut buf: ReadBufCursor<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        loop {
            let mut stack = [0u8; PLAIN_READ];
            let want = buf.remaining().min(PLAIN_READ);
            match this.conn.reader().read(&mut stack[..want]) {
                Ok(n) => {
                    buf.put_slice(&stack[..n]);
                    return Poll::Ready(Ok(()));
                }
                // A peer that closed with no close notification ends the
                // stream as a plain socket that closed does.
                Err(error) if error.kind() == ErrorKind::UnexpectedEof => {
                    return Poll::Ready(Ok(()));
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => {}
                Err(error) => return Poll::Ready(Err(error)),
            }
            if let Poll::Ready(Err(error)) = this.poll_flush_tls(cx) {
                return Poll::Ready(Err(error));
            }
            if this.eof {
                return Poll::Ready(Ok(()));
            }
            match this.poll_fill(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                Poll::Ready(Ok(())) => {}
            }
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

/// Returns an IO error of the kind `InvalidData` for an error of rustls.
///
/// The error of rustls is dropped, since no caller reads more than the end
/// of the connection, and an IO error that carried it would link the
/// formatting of every error of rustls through its vtable.
fn invalid(_: rustls::Error) -> io::Error {
    io::Error::from(ErrorKind::InvalidData)
}
