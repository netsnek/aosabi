// SPDX-License-Identifier: MIT
/*
 * homa unix: the Unix Socket transports of Linux user space, a SOCK_SEQPACKET
 * socket whose every record is one Homa packet and a stream socket that
 * carries prefixed AoFrame.
 *
 * A record of a sequenced packet socket keeps its boundary, so it carries one
 * Homa packet of at most 16,384 bytes with no framing, and a record that
 * arrives longer than that is dropped and counted. A stream socket beside it
 * carries the same packets in prefixed AoFrame, the socket Node.js reaches
 * with node:net. Every system call goes through socket2 or the standard
 * library, so the file holds no unsafe.
 */

//! The Unix Socket transports.
//!
//! [`SeqPacket`] is one connected `SOCK_SEQPACKET` socket and a [`Carrier`]
//! with one peer, whose every record is one Homa packet. [`SeqListener`]
//! binds and accepts such sockets at a path, and [`pair`] connects two in one
//! process. [`StreamListener`] accepts stream sockets at a path, and the
//! standard library's `UnixStream` is a
//! [`ByteStream`](crate::stream::ByteStream), so a stream carries prefixed
//! `AoFrame` through [`Framed`](crate::stream::Framed). Every socket is non
//! blocking from the moment this module hands it out, set by a safe call of
//! `socket2` or of the standard library, and every call answers `None` or
//! `false` where the socket would block, so a driver waits on the socket's
//! readiness and calls again. Each socket implements `AsFd`, which a reactor
//! takes.
//!
//! **A record longer than a packet.** A receive asks the kernel for the
//! length of the next record first, with `MSG_PEEK` and `MSG_TRUNC` over no
//! buffer, which returns the length of the whole record, as `recv(2)`
//! documents for a sequenced packet socket since Linux 3.4 and
//! `unix_dgram_recvmsg` does. A record longer than the packets of the link or
//! the caller's buffer is taken off the socket unread and counted in
//! [`SeqPacket::truncated`], and Homa's RESEND recovers what it carried.
//!
//! **A closed peer.** A record of no bytes, which no Homa packet is, reads as
//! the end of the socket, as the kernel signals a peer that closed a
//! sequenced packet socket. Every send passes `MSG_NOSIGNAL`, so a peer that
//! closed costs the error `EPIPE` and never the signal `SIGPIPE`, which the
//! kernel raises on a stream socket without that flag. A send that fails
//! other than by blocking, the write of a packet that waited among them,
//! marks the link closed and returns its error, and every receive after it
//! answers `None`.
//!
//! **Stale files.** A listener binds a path after it removes a socket file
//! that no process listens on any more, which a connect to it tells by
//! `ECONNREFUSED`, and it leaves every other file alone, so a bind over a
//! live socket or over a file that is no socket fails with `AddrInUse`.
//!
//! The peer credential stays outside the crate: a driver reads `SO_PEERCRED`
//! of an accepted socket through its own runtime, over the socket's `AsFd`.

use core::ffi::c_int;
use std::{
    io::{
        self,
        IoSlice,
        Read as _, //
    },
    os::{
        fd::{
            AsFd,
            BorrowedFd, //
        },
        unix::{
            fs::FileTypeExt as _,
            net::{
                UnixListener,
                UnixStream, //
            }, //
        }, //
    },
    path::Path,
    vec::Vec, //
};

use socket2::{
    Domain,
    SockAddr,
    SockRef,
    Socket,
    Type, //
};

use crate::{
    Carrier,
    Priority, //
};

/// The Linux value of `MSG_PEEK`, a receive that leaves the record queued.
const MSG_PEEK: c_int = 2;

/// The Linux value of `MSG_TRUNC`, a receive that returns the length of the
/// whole record.
const MSG_TRUNC: c_int = 0x20;

/// The Linux value of `MSG_NOSIGNAL`, a send that raises no `SIGPIPE`.
const MSG_NOSIGNAL: c_int = 0x4000;

// The three values above are those of Linux, and another system spells the
// same flags otherwise, so the module refuses to build anywhere else rather
// than pass a flag it does not mean.
#[cfg(not(target_os = "linux"))]
compile_error!("homa::unix holds the transports of Linux user space and builds on Linux alone");

/// The connections a listener keeps waiting for `accept`.
const BACKLOG: c_int = 64;

/// Returns whether `error` says the socket would block.
fn would_block(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::WouldBlock
}

/// Returns whether `error` says a signal interrupted the call.
fn interrupted(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::Interrupted
}

/// Removes the socket file at `path` where no process listens on it any
/// more, which a connect of `ty` tells by `ECONNREFUSED`.
///
/// A path that does not exist needs nothing, and a path that holds a live
/// socket or a file that is no socket stays, so the bind that follows fails
/// with `AddrInUse`.
fn clear_stale(path: &Path, ty: Type) -> io::Result<()> {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return Ok(());
    };
    if !meta.file_type().is_socket() {
        return Ok(());
    }
    let probe = Socket::new(Domain::UNIX, ty, None)?;
    probe.set_nonblocking(true)?;
    match probe.connect(&SockAddr::unix(path)?) {
        Err(error) if error.kind() == io::ErrorKind::ConnectionRefused => {
            std::fs::remove_file(path)
        }
        _ => Ok(()),
    }
}

/// A listening `SOCK_SEQPACKET` socket at a path, non blocking.
#[derive(Debug)]
pub struct SeqListener {
    /// The listening socket.
    socket: Socket,
}

impl SeqListener {
    /// Binds `path`, removing a stale socket file of a dead process first,
    /// and listens.
    ///
    /// The caller checks the directory, its owner and its mode before the
    /// bind, as the driver of a card does.
    ///
    /// # Errors
    ///
    /// Returns the error of the system call that failed, `AddrInUse` where a
    /// live socket or another file holds the path.
    pub fn bind(path: &Path) -> io::Result<Self> {
        clear_stale(path, Type::SEQPACKET)?;
        let socket = Socket::new(Domain::UNIX, Type::SEQPACKET, None)?;
        socket.bind(&SockAddr::unix(path)?)?;
        socket.listen(BACKLOG)?;
        socket.set_nonblocking(true)?;
        Ok(Self { socket })
    }

    /// Accepts one connection whose records carry packets of at most
    /// `max_packet` bytes, or answers `None` when none is waiting.
    ///
    /// # Errors
    ///
    /// Returns the error of `accept` or of making the socket non blocking.
    pub fn accept(&self, max_packet: u16) -> io::Result<Option<SeqPacket>> {
        loop {
            match self.socket.accept() {
                Ok((socket, _)) => {
                    socket.set_nonblocking(true)?;
                    return Ok(Some(SeqPacket::new(socket, max_packet)));
                }
                Err(error) if would_block(&error) => return Ok(None),
                Err(error) if interrupted(&error) => {}
                Err(error) => return Err(error),
            }
        }
    }
}

impl AsFd for SeqListener {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.socket.as_fd()
    }
}

/// One connected `SOCK_SEQPACKET` socket, non blocking, whose every record
/// is one Homa packet of at most `max_packet` bytes.
///
/// A packet the socket does not take at once waits in the socket's own
/// buffer of one packet, and [`Carrier::ready`] answers false until
/// [`SeqPacket::flush`] or the next receive has written it. A record that
/// arrives longer than `max_packet` is dropped and counted in
/// [`SeqPacket::truncated`].
#[derive(Debug)]
pub struct SeqPacket {
    /// The socket.
    socket: Socket,
    /// Bytes of the longest record taken as a packet.
    max_packet: u16,
    /// Records dropped because they were longer than a packet or the buffer.
    truncated: u64,
    /// Packets dropped because one waited already.
    dropped: u64,
    /// The packet the socket did not take yet, empty for none.
    pending: Vec<u8>,
    /// Whether the peer closed its end.
    closed: bool,
}

impl SeqPacket {
    /// Wraps a connected, non blocking socket.
    const fn new(socket: Socket, max_packet: u16) -> Self {
        Self {
            socket,
            max_packet,
            truncated: 0,
            dropped: 0,
            pending: Vec::new(),
            closed: false,
        }
    }

    /// Connects to the listener at `path`, whose records carry packets of at
    /// most `max_packet` bytes.
    ///
    /// # Errors
    ///
    /// Returns the error of the connect, `ConnectionRefused` where no process
    /// listens at `path`.
    pub fn connect(path: &Path, max_packet: u16) -> io::Result<Self> {
        let socket = Socket::new(Domain::UNIX, Type::SEQPACKET, None)?;
        socket.connect(&SockAddr::unix(path)?)?;
        socket.set_nonblocking(true)?;
        Ok(Self::new(socket, max_packet))
    }

    /// Returns the count of records dropped as truncated, each longer than a
    /// packet of the link or than the buffer of the receive.
    #[must_use]
    pub const fn truncated(&self) -> u64 {
        self.truncated
    }

    /// Returns the count of packets dropped because one waited for the
    /// socket already, which an engine that asks [`Carrier::ready`] first
    /// never meets.
    #[must_use]
    pub const fn dropped(&self) -> u64 {
        self.dropped
    }

    /// Returns whether the peer closed its end.
    #[must_use]
    pub const fn closed(&self) -> bool {
        self.closed
    }

    /// Writes the packet that waits for the socket, and returns whether none
    /// waits any more.
    ///
    /// A send that fails other than by blocking marks the link closed and
    /// drops the packet, which no later call can write, as [`Carrier::send`]
    /// does, so a receive after it reads the link as closed.
    ///
    /// # Errors
    ///
    /// Returns the error of the send, `BrokenPipe` where the peer closed, or
    /// `ConnectionReset` where it closed with records unread.
    pub fn flush(&mut self) -> io::Result<bool> {
        if self.pending.is_empty() {
            return Ok(true);
        }
        loop {
            match self.socket.send_with_flags(&self.pending, MSG_NOSIGNAL) {
                Ok(_) => {
                    self.pending.clear();
                    return Ok(true);
                }
                Err(error) if would_block(&error) => return Ok(false),
                Err(error) if interrupted(&error) => {}
                Err(error) => {
                    self.pending.clear();
                    self.closed = true;
                    return Err(error);
                }
            }
        }
    }

    /// Returns the length of the next record without taking it, or `None`
    /// where none is waiting.
    fn next_len(&self) -> io::Result<Option<usize>> {
        loop {
            match self.socket.recv_with_flags(&mut [], MSG_PEEK | MSG_TRUNC) {
                Ok(len) => return Ok(Some(len)),
                Err(error) if would_block(&error) => return Ok(None),
                Err(error) if interrupted(&error) => {}
                Err(error) => return Err(error),
            }
        }
    }

    /// Takes the next record off the socket unread.
    fn discard(&self) -> io::Result<()> {
        loop {
            match self.socket.recv_with_flags(&mut [], 0) {
                Ok(_) => return Ok(()),
                Err(error) if interrupted(&error) => {}
                Err(error) => return Err(error),
            }
        }
    }
}

impl Carrier for SeqPacket {
    type Peer = ();
    type Error = io::Error;

    /// Answers whether no packet waits for the socket and the peer is open.
    fn ready(&self, _peer: (), _len: usize) -> bool {
        self.pending.is_empty() && !self.closed
    }

    /// Sends the packet as one record, header and payload together. A socket
    /// has no priorities, so the level is ignored, and a packet the socket
    /// does not take waits for [`SeqPacket::flush`].
    fn send(
        &mut self,
        _peer: (),
        _priority: Priority,
        header: &[u8],
        payload: &[u8],
    ) -> io::Result<()> {
        if !self.flush()? {
            self.dropped = self.dropped.saturating_add(1);
            return Ok(());
        }
        let parts = [IoSlice::new(header), IoSlice::new(payload)];
        loop {
            match self.socket.send_vectored_with_flags(&parts, MSG_NOSIGNAL) {
                Ok(_) => return Ok(()),
                Err(error) if would_block(&error) => {
                    self.pending.extend_from_slice(header);
                    self.pending.extend_from_slice(payload);
                    return Ok(());
                }
                Err(error) if interrupted(&error) => {}
                Err(error) => {
                    self.closed = true;
                    return Err(error);
                }
            }
        }
    }

    /// Writes the packet that waits first, then takes the next record whose
    /// length fits both a packet and `buf`, dropping and counting every
    /// longer one on the way.
    fn recv(&mut self, buf: &mut [u8]) -> io::Result<Option<((), usize)>> {
        self.flush()?;
        loop {
            if self.closed {
                return Ok(None);
            }
            let Some(len) = self.next_len()? else {
                return Ok(None);
            };
            if len == 0 {
                self.discard()?;
                self.closed = true;
                return Ok(None);
            }
            let room = buf.len().min(usize::from(self.max_packet));
            let Some(target) = buf.get_mut(..len).filter(|_| len <= room) else {
                self.discard()?;
                self.truncated = self.truncated.saturating_add(1);
                continue;
            };
            loop {
                match (&self.socket).read(target) {
                    Ok(read) => return Ok(Some(((), read))),
                    Err(error) if interrupted(&error) => {}
                    Err(error) => return Err(error),
                }
            }
        }
    }
}

impl AsFd for SeqPacket {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.socket.as_fd()
    }
}

/// Sets the send buffer of the Unix socket `socket` to `bytes`, which the
/// kernel doubles and holds above its floor.
///
/// A Unix socket charges a record or the bytes of a stream to the sender
/// until the peer reads them, so a small send buffer keeps a message the peer
/// has not read yet in the engine, where SRPT lets a shorter one overtake it.
/// A sequenced packet socket refuses a record longer than its send buffer
/// less 32 bytes with `EMSGSIZE`, which [`SeqPacket`] returns from the send
/// and takes as the end of its link, so the buffer of such a socket holds at
/// least one packet of its link and 32 bytes once the kernel has doubled it.
///
/// # Errors
///
/// Returns the error of `setsockopt`.
pub fn set_send_buffer<S: AsFd>(socket: &S, bytes: usize) -> io::Result<()> {
    SockRef::from(socket).set_send_buffer_size(bytes)
}

/// Returns two connected `SOCK_SEQPACKET` sockets in one process, non
/// blocking, whose records carry packets of at most `max_packet` bytes.
///
/// # Errors
///
/// Returns the error of `socketpair` or of making a socket non blocking.
pub fn pair(max_packet: u16) -> io::Result<(SeqPacket, SeqPacket)> {
    let (a, b) = Socket::pair(Domain::UNIX, Type::SEQPACKET, None)?;
    a.set_nonblocking(true)?;
    b.set_nonblocking(true)?;
    Ok((SeqPacket::new(a, max_packet), SeqPacket::new(b, max_packet)))
}

/// A listening stream socket at a path, non blocking, whose connections
/// carry prefixed `AoFrame`, the socket Node.js reaches with `node:net`.
#[derive(Debug)]
pub struct StreamListener {
    /// The listening socket.
    listener: UnixListener,
}

impl StreamListener {
    /// Binds `path`, removing a stale socket file of a dead process first,
    /// and listens.
    ///
    /// # Errors
    ///
    /// Returns the error of the system call that failed, `AddrInUse` where a
    /// live socket or another file holds the path.
    pub fn bind(path: &Path) -> io::Result<Self> {
        clear_stale(path, Type::STREAM)?;
        let listener = UnixListener::bind(path)?;
        listener.set_nonblocking(true)?;
        Ok(Self { listener })
    }

    /// Accepts one connection, non blocking, or answers `None` when none is
    /// waiting.
    ///
    /// # Errors
    ///
    /// Returns the error of `accept` or of making the stream non blocking.
    pub fn accept(&self) -> io::Result<Option<UnixStream>> {
        loop {
            match self.listener.accept() {
                Ok((stream, _)) => {
                    stream.set_nonblocking(true)?;
                    return Ok(Some(stream));
                }
                Err(error) if would_block(&error) => return Ok(None),
                Err(error) if interrupted(&error) => {}
                Err(error) => return Err(error),
            }
        }
    }
}

impl AsFd for StreamListener {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.listener.as_fd()
    }
}

/// The stream form: a `UnixStream` read and written without blocking, whose
/// bytes a [`Framed`](crate::stream::Framed) frames in prefixed `AoFrame`.
///
/// The stream must be non blocking, as [`StreamListener::accept`] hands it
/// out and as `UnixStream::set_nonblocking` makes a connected one, and a
/// write passes `MSG_NOSIGNAL` through `socket2`, so a peer that closed costs
/// the error `EPIPE` and never the signal `SIGPIPE`.
#[cfg(any(feature = "stream-delimited", feature = "stream-prefixed"))]
impl crate::stream::ByteStream for UnixStream {
    type Error = io::Error;

    fn read(&mut self, buf: &mut [u8]) -> io::Result<Option<usize>> {
        loop {
            match io::Read::read(self, buf) {
                Ok(read) => return Ok(Some(read)),
                Err(error) if would_block(&error) => return Ok(None),
                Err(error) if interrupted(&error) => {}
                Err(error) => return Err(error),
            }
        }
    }

    fn write(&mut self, bytes: &[u8]) -> io::Result<Option<usize>> {
        loop {
            match SockRef::from(&*self).send_with_flags(bytes, MSG_NOSIGNAL) {
                Ok(written) => return Ok(Some(written)),
                Err(error) if would_block(&error) => return Ok(None),
                Err(error) if interrupted(&error) => {}
                Err(error) => return Err(error),
            }
        }
    }
}
