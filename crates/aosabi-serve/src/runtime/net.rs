// SPDX-License-Identifier: MIT
/*
 * aosabi-serve: the non blocking sockets of the runtime.
 *
 * No analogue on the reference side.
 */

//! `TcpListener`, `TcpStream`, `UnixListener` and `UnixStream`.
//!
//! Each socket is a socket of the standard library set non blocking and added
//! to the reactor of the runtime. A read, a write or an accept that answers
//! `WouldBlock` registers the waker of its task for its interest and answers
//! `Pending`, and every byte that moves is reported to the reactor, which the
//! poll reactor reads as heat. A socket leaves the reactor when it drops.
//! Every accepted TCP socket is set `TCP_NODELAY`, which Node's server sets
//! by default.

use core::{
    future::poll_fn,
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
    net::{
        self,
        Shutdown,
        SocketAddr,
        ToSocketAddrs, //
    },
    os::{
        fd::{
            AsFd,
            BorrowedFd, //
        },
        unix::net as unix,
    },
    path::Path,
};

use super::{
    Handle,
    reactor::{
        Interest,
        SourceId, //
    },
};

/// A socket of the runtime and its place in the reactor.
struct Source<T: AsFd> {
    /// The socket of the standard library, non blocking.
    socket: T,
    /// The number of the socket in the reactor.
    id: SourceId,
    /// The runtime the socket was added to.
    handle: Handle,
}

impl<T: AsFd> Source<T> {
    /// Adds `socket` to the reactor of the current runtime.
    fn new(socket: T) -> io::Result<Self> {
        let handle = Handle::current()?;
        let id = handle.reactor().add(socket.as_fd())?;
        Ok(Self { socket, id, handle })
    }

    /// Runs `op` on the socket, and registers `interest` and answers
    /// `Pending` when it would block.
    fn poll_io<R>(
        &self,
        cx: &mut Context<'_>,
        interest: Interest,
        mut op: impl FnMut(&T) -> io::Result<R>,
    ) -> Poll<io::Result<R>> {
        loop {
            match op(&self.socket) {
                Err(error) if error.kind() == ErrorKind::WouldBlock => {
                    self.handle
                        .reactor()
                        .register(self.id, interest, cx.waker());
                    return Poll::Pending;
                }
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                result => return Poll::Ready(result),
            }
        }
    }

    /// Reports a byte that moved.
    fn progressed(&self) {
        self.handle.reactor().progressed(self.id);
    }
}

impl<T: AsFd> Drop for Source<T> {
    fn drop(&mut self) {
        self.handle.reactor().remove(self.socket.as_fd(), self.id);
    }
}

/// A stream socket of the runtime, the IO a protocol reads and writes.
pub trait Socket {
    /// Reads into `buf`, `Ok(0)` at the end of the stream.
    ///
    /// # Errors
    ///
    /// Returns the error of the read.
    fn poll_read(&self, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<io::Result<usize>>;

    /// Writes from `buf` and answers how many bytes went out.
    ///
    /// # Errors
    ///
    /// Returns the error of the write.
    fn poll_write(&self, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>>;

    /// Shuts the writing half of the stream down.
    ///
    /// # Errors
    ///
    /// Returns the error of the shutdown.
    fn shutdown_write(&self) -> io::Result<()>;
}

/// A non blocking TCP listener.
pub struct TcpListener {
    /// The listener and its place in the reactor.
    source: Source<net::TcpListener>,
}

impl TcpListener {
    /// Binds a listener to `addr` on the current runtime.
    ///
    /// # Errors
    ///
    /// Returns the error of the bind, and an error outside a runtime.
    pub fn bind(addr: impl ToSocketAddrs) -> io::Result<Self> {
        let listener = net::TcpListener::bind(addr)?;
        listener.set_nonblocking(true)?;
        Ok(Self {
            source: Source::new(listener)?,
        })
    }

    /// Returns the address the listener is bound to.
    ///
    /// # Errors
    ///
    /// Returns the error of `getsockname`.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.source.socket.local_addr()
    }

    /// Accepts one connection, set non blocking and `TCP_NODELAY`.
    ///
    /// # Errors
    ///
    /// Returns the error of the accept.
    pub async fn accept(&self) -> io::Result<(TcpStream, SocketAddr)> {
        let (stream, addr) = poll_fn(|cx| {
            self.source
                .poll_io(cx, Interest::Read, net::TcpListener::accept)
        })
        .await?;
        stream.set_nonblocking(true)?;
        stream.set_nodelay(true)?;
        Ok((TcpStream::from_std(stream)?, addr))
    }
}

/// A non blocking TCP stream.
pub struct TcpStream {
    /// The stream and its place in the reactor.
    source: Source<net::TcpStream>,
}

impl TcpStream {
    /// Adds a stream of the standard library, set non blocking already, to
    /// the current runtime.
    ///
    /// # Errors
    ///
    /// Returns an error outside a runtime.
    pub fn from_std(stream: net::TcpStream) -> io::Result<Self> {
        Ok(Self {
            source: Source::new(stream)?,
        })
    }

    /// Returns the stream of the standard library.
    #[must_use]
    pub fn as_std(&self) -> &net::TcpStream {
        &self.source.socket
    }
}

impl Socket for TcpStream {
    fn poll_read(&self, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<io::Result<usize>> {
        let read = self
            .source
            .poll_io(cx, Interest::Read, |mut socket| socket.read(buf));
        if matches!(read, Poll::Ready(Ok(n)) if n > 0) {
            self.source.progressed();
        }
        read
    }

    fn poll_write(&self, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        let written = self
            .source
            .poll_io(cx, Interest::Write, |mut socket| socket.write(buf));
        if matches!(written, Poll::Ready(Ok(n)) if n > 0) {
            self.source.progressed();
        }
        written
    }

    fn shutdown_write(&self) -> io::Result<()> {
        match self.source.socket.shutdown(Shutdown::Write) {
            Err(error) if error.kind() == ErrorKind::NotConnected => Ok(()),
            result => result,
        }
    }
}

impl AsFd for TcpStream {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.source.socket.as_fd()
    }
}

/// A non blocking listener of a Unix stream socket.
pub struct UnixListener {
    /// The listener and its place in the reactor.
    source: Source<unix::UnixListener>,
}

impl UnixListener {
    /// Binds a listener to the socket file `path` on the current runtime.
    ///
    /// # Errors
    ///
    /// Returns the error of the bind, and an error outside a runtime.
    pub fn bind(path: impl AsRef<Path>) -> io::Result<Self> {
        let listener = unix::UnixListener::bind(path)?;
        listener.set_nonblocking(true)?;
        Ok(Self {
            source: Source::new(listener)?,
        })
    }

    /// Accepts one connection, set non blocking.
    ///
    /// # Errors
    ///
    /// Returns the error of the accept.
    pub async fn accept(&self) -> io::Result<UnixStream> {
        let (stream, _) = poll_fn(|cx| {
            self.source
                .poll_io(cx, Interest::Read, unix::UnixListener::accept)
        })
        .await?;
        stream.set_nonblocking(true)?;
        UnixStream::from_std(stream)
    }
}

/// A non blocking Unix stream socket.
pub struct UnixStream {
    /// The stream and its place in the reactor.
    source: Source<unix::UnixStream>,
}

impl UnixStream {
    /// Adds a stream of the standard library, set non blocking already, to
    /// the current runtime.
    ///
    /// # Errors
    ///
    /// Returns an error outside a runtime.
    pub fn from_std(stream: unix::UnixStream) -> io::Result<Self> {
        Ok(Self {
            source: Source::new(stream)?,
        })
    }

    /// Returns the stream of the standard library.
    #[must_use]
    pub fn as_std(&self) -> &unix::UnixStream {
        &self.source.socket
    }
}

impl Socket for UnixStream {
    fn poll_read(&self, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<io::Result<usize>> {
        let read = self
            .source
            .poll_io(cx, Interest::Read, |mut socket| socket.read(buf));
        if matches!(read, Poll::Ready(Ok(n)) if n > 0) {
            self.source.progressed();
        }
        read
    }

    fn poll_write(&self, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        let written = self
            .source
            .poll_io(cx, Interest::Write, |mut socket| socket.write(buf));
        if matches!(written, Poll::Ready(Ok(n)) if n > 0) {
            self.source.progressed();
        }
        written
    }

    fn shutdown_write(&self) -> io::Result<()> {
        match self.source.socket.shutdown(Shutdown::Write) {
            Err(error) if error.kind() == ErrorKind::NotConnected => Ok(()),
            result => result,
        }
    }
}

impl AsFd for UnixStream {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.source.socket.as_fd()
    }
}
