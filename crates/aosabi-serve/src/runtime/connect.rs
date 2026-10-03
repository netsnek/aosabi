// SPDX-License-Identifier: MIT
/*
 * aosabi-serve: the outbound connect of a TCP stream on the runtime.
 *
 * No analogue on the reference side, where the event loop of Node opens a
 * connection without blocking.
 */

//! `TcpStream::connect`, a connect that yields the thread while it waits.
//!
//! Under the feature `linux` on a Linux target the socket opens non blocking
//! through `rustix::net::socket_with`, `rustix::net::connect` answers
//! `EINPROGRESS`, and the socket waits in the reactor of the runtime with the
//! write interest until the connection opens, fails or the deadline passes,
//! so the one thread runs every other task meanwhile. A wake asks the kernel
//! through a `poll` that does not wait whether the socket is writable, since
//! the deadline wakes the same task, and reads the outcome with `SO_ERROR`.
//! The open socket becomes a stream of the standard library through
//! `From<OwnedFd>`, with no `unsafe`.
//!
//! Every other build connects through `std::net::TcpStream::connect_timeout`,
//! which blocks the thread up to the deadline. Either way the stream is set
//! `TCP_NODELAY`, as undici sets it on the sockets of the fetch of Node.

use std::{
    io::{
        self,
        ErrorKind, //
    },
    net::{
        self,
        SocketAddr, //
    },
    time::Duration, //
};

use super::{
    Handle,
    LocalBoxFuture,
    net::TcpStream, //
};

impl TcpStream {
    /// Opens a connection to `addr`, yielding the thread while it waits, or
    /// ends at `deadline_ms` of the clock of the runtime.
    ///
    /// The deadline is a time of [`RuntimeClock`](super::RuntimeClock) in
    /// milliseconds since its start, so a caller that makes several attempts
    /// gives each the part of one bound it has left.
    ///
    /// # Errors
    ///
    /// The future answers the error of the socket or of the connect,
    /// `TimedOut` once the deadline passes, and an error outside a runtime.
    #[must_use]
    pub fn connect(
        addr: SocketAddr,
        deadline_ms: u64,
    ) -> LocalBoxFuture<'static, io::Result<Self>> {
        Box::pin(async move {
            let handle = Handle::current()?;
            let stream = open(&handle, addr, deadline_ms).await?;
            stream.set_nodelay(true)?;
            Self::from_std(stream)
        })
    }
}

/// Returns the time left until `deadline_ms`, or `TimedOut` when it passed.
fn remaining(handle: &Handle, deadline_ms: u64) -> io::Result<Duration> {
    let left = deadline_ms.saturating_sub(handle.clock().now_ms());
    if left == 0 {
        return Err(timed_out());
    }
    Ok(Duration::from_millis(left))
}

/// Returns the error of a connect that passed its deadline.
fn timed_out() -> io::Error {
    io::Error::new(ErrorKind::TimedOut, "the connect passed its deadline")
}

/// Opens the connection through the standard library, blocking the thread up
/// to the deadline.
#[cfg(not(all(feature = "linux", target_os = "linux")))]
// The connect of this build blocks, and its signature is the one of the
// reactor's connect, which awaits.
#[expect(clippy::unused_async)]
async fn open(handle: &Handle, addr: SocketAddr, deadline_ms: u64) -> io::Result<net::TcpStream> {
    let timeout = remaining(handle, deadline_ms)?;
    let stream = net::TcpStream::connect_timeout(&addr, timeout)?;
    stream.set_nonblocking(true)?;
    Ok(stream)
}

/// Opens the connection on the reactor of the runtime.
#[cfg(all(feature = "linux", target_os = "linux"))]
async fn open(handle: &Handle, addr: SocketAddr, deadline_ms: u64) -> io::Result<net::TcpStream> {
    linux::open(handle, addr, deadline_ms).await
}

#[cfg(all(feature = "linux", target_os = "linux"))]
mod linux {
    use core::{
        future::{
            Future,
            poll_fn, //
        },
        pin::Pin,
        task::{
            Context,
            Poll, //
        },
    };
    use std::{
        io,
        net::{
            self,
            SocketAddr, //
        },
        os::fd::{
            AsFd,
            OwnedFd, //
        },
    };

    use rustix::{
        event::{
            PollFd,
            PollFlags,
            Timespec, //
        },
        io::Errno,
        net::{
            AddressFamily,
            SocketFlags,
            SocketType,
            sockopt, //
        },
    };

    use super::{
        remaining,
        timed_out, //
    };
    use crate::runtime::{
        Handle,
        hyper_rt::RuntimeTimer,
        reactor::{
            Interest,
            SourceId, //
        },
    };

    /// A socket whose connect is in progress and its place in the reactor.
    struct Connecting {
        /// The socket, taken once the connect ends.
        fd: Option<OwnedFd>,
        /// The number of the socket in the reactor.
        id: SourceId,
        /// The runtime the socket was added to.
        handle: Handle,
    }

    impl Connecting {
        /// Polls the connect, registering the waker for the write interest
        /// while it is in progress.
        fn poll_open(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<OwnedFd>> {
            let Some(fd) = &self.fd else {
                return Poll::Ready(Err(io::Error::other("the connect ended already")));
            };
            match writable(fd) {
                Ok(false) => {
                    self.handle
                        .reactor()
                        .register(self.id, Interest::Write, cx.waker());
                    Poll::Pending
                }
                Ok(true) => Poll::Ready(self.finish()),
                Err(error) => Poll::Ready(Err(error)),
            }
        }

        /// Reads the outcome of the connect and takes the socket out of the
        /// reactor.
        fn finish(&mut self) -> io::Result<OwnedFd> {
            let fd = self
                .fd
                .take()
                .ok_or_else(|| io::Error::other("the connect ended already"))?;
            self.handle.reactor().remove(fd.as_fd(), self.id);
            sockopt::socket_error(&fd)??;
            Ok(fd)
        }
    }

    impl Drop for Connecting {
        fn drop(&mut self) {
            if let Some(fd) = &self.fd {
                self.handle.reactor().remove(fd.as_fd(), self.id);
            }
        }
    }

    /// Returns whether the connect of `fd` has ended, opened or failed,
    /// through a `poll` that does not wait.
    fn writable(fd: &OwnedFd) -> io::Result<bool> {
        let mut fds = [PollFd::new(fd, PollFlags::OUT)];
        let now = Timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        match rustix::event::poll(&mut fds, Some(&now)) {
            Ok(_) => Ok(!fds[0].revents().is_empty()),
            Err(Errno::INTR) => Ok(false),
            Err(error) => Err(error.into()),
        }
    }

    /// Opens a connection to `addr` on the reactor of the runtime of `handle`.
    pub(super) async fn open(
        handle: &Handle,
        addr: SocketAddr,
        deadline_ms: u64,
    ) -> io::Result<net::TcpStream> {
        let timeout = remaining(handle, deadline_ms)?;
        let family = if addr.is_ipv4() {
            AddressFamily::INET
        } else {
            AddressFamily::INET6
        };
        let fd = rustix::net::socket_with(
            family,
            SocketType::STREAM,
            SocketFlags::NONBLOCK | SocketFlags::CLOEXEC,
            None,
        )?;
        match rustix::net::connect(&fd, &addr) {
            Ok(()) => return Ok(net::TcpStream::from(fd)),
            Err(Errno::INPROGRESS | Errno::INTR) => {}
            Err(error) => return Err(error.into()),
        }
        let id = handle.reactor().add(fd.as_fd())?;
        let mut connecting = Connecting {
            fd: Some(fd),
            id,
            handle: handle.clone(),
        };
        let mut deadline = RuntimeTimer::of(handle).sleep_for(timeout);
        let fd = poll_fn(|cx| match connecting.poll_open(cx) {
            Poll::Pending => match Pin::new(&mut deadline).poll(cx) {
                Poll::Ready(()) => Poll::Ready(Err(timed_out())),
                Poll::Pending => Poll::Pending,
            },
            ready @ Poll::Ready(_) => ready,
        })
        .await?;
        Ok(net::TcpStream::from(fd))
    }
}
