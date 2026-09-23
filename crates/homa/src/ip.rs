// SPDX-License-Identifier: MIT
/*
 * homa ip: the IP driver of Network in user space over raw IP 146, the
 * IPPROTO_HOMA of HomaModule.
 *
 * The constants of Linux below are read at
 * https://github.com/torvalds/linux/tree/fe2ec83746e501645709761605c2464a44fd2929.
 * Every system call goes through socket2 or the standard library, so the file
 * holds no unsafe.
 */

//! The IP driver of Network.
//!
//! [`RawSocket`] is a raw socket of IP protocol 146 and a [`Carrier`] whose
//! peer is a `SocketAddr`, the source address of the IP header with the
//! source port of the Homa common header, so two processes of one host are
//! two peers to a server, each with its own RPCs, as the socket of a kernel
//! module names them. [`RawSocket::open`] claims the Homa ports of the
//! process, opens the socket, attaches the filter that admits only packets
//! for those ports and sizes the buffers.
//!
//! **The claims.** A raw socket of protocol 146 receives every packet of that
//! protocol in its network namespace, so every process of a host sees the
//! packets of every other, and nothing in the kernel keeps two processes off
//! one Homa port. Each process therefore claims each of its ports by binding
//! a Unix socket to the abstract name `homa-port-<n>`, which a second bind of
//! the same name refuses with `AddrInUse` and which the kernel frees when the
//! process ends, however it ends. Abstract names live in the network
//! namespace, as Homa ports do.
//!
//! **The filter.** A classic BPF program on the socket admits a packet only
//! when the destination port of its Homa header lies in the ports the process
//! owns, and the kernel runs it before it queues the packet, so a packet of
//! another process costs no copy into this one. [`filter`] returns the
//! program: on IPv4 the socket sees the IP header first, so the program reads
//! its length from the low nibble of the first byte, and on IPv6 it sees the
//! Homa header first, five instructions and four for one port, one comparison
//! more for a range. A packet too short to hold the port fails the load, and
//! the kernel drops it.
//!
//! **The priority.** A packet's level travels as one control message of the
//! send, `IP_TOS` on IPv4 and `IPV6_TCLASS` on IPv6, the level shifted left
//! by five bits, which the kernel writes into that packet's header alone.
//!
//! **The reads.** A receive on IPv4 reads the IP header with the packet
//! through `Read` for a `socket2` socket, and a receive on IPv6 reads the
//! packet and its source through `recv_from` of the `UdpSocket` the socket
//! turns into, because every receive of `socket2` that returns an address
//! writes into memory safe code cannot read. Both read into an initialised
//! buffer, and the driver calls no `recvmsg`.
//!
//! Every socket is non blocking, and every call answers `None` or `false`
//! where it would block. [`RawSocket::wait`] waits for a packet for at most a
//! given time, the one wait of a program that runs no reactor, the helper
//! `homa-raw` among them.

use core::{
    ffi::c_int,
    fmt,
    net::{
        IpAddr,
        Ipv4Addr,
        SocketAddr,
        SocketAddrV6, //
    },
    time::Duration,
};
use std::{
    format,
    io::{
        self,
        IoSlice,
        Read as _, //
    },
    net::UdpSocket,
    os::{
        fd::{
            AsFd,
            BorrowedFd, //
        },
        linux::net::SocketAddrExt as _,
        unix::net::{
            SocketAddr as UnixAddr,
            UnixListener, //
        },
    },
    vec,
    vec::Vec, //
};

use socket2::{
    Domain,
    MsgHdr,
    Protocol,
    SockAddr,
    SockFilter,
    SockRef,
    Socket,
    Type, //
};

use crate::{
    Carrier,
    Encap,
    Priority, //
};

// The constants below are those of Linux, so the module refuses to build
// anywhere else rather than pass a value it does not mean.
#[cfg(not(target_os = "linux"))]
compile_error!("homa::ip holds the IP driver of Linux user space and builds on Linux alone");

/// The IP protocol of Homa.
const IPPROTO_HOMA: c_int = 146;

/// The level of an IPv4 control message, `SOL_IP`.
const SOL_IP: c_int = 0;

/// The control message of the IPv4 TOS byte.
const IP_TOS: c_int = 1;

/// The level of an IPv6 control message, `SOL_IPV6`.
const SOL_IPV6: c_int = 41;

/// The control message of the IPv6 traffic class.
const IPV6_TCLASS: c_int = 67;

/// The prefix of the abstract name that claims a Homa port.
const CLAIM: &str = "homa-port-";

/// The longest IP datagram, whose length field is 16 bits, the whole IPv4
/// datagram or the IPv6 payload a raw socket of IPv6 returns.
const MAX_DATAGRAM: usize = 65_535;

/// The bytes of an IPv4 header without options, an IHL of 5 words of 32 bits.
const IPV4_MIN_HEADER: usize = 20;

/// The bytes of the Homa header that carry both ports.
const PORTS_LEN: usize = 4;

/// The instruction classes and modes of classic BPF.
mod bpf {
    /// Loads into the accumulator.
    pub(super) const LD: u16 = 0x00;
    /// Loads into the index register.
    pub(super) const LDX: u16 = 0x01;
    /// A conditional jump.
    pub(super) const JMP: u16 = 0x05;
    /// Returns the bytes of the packet to keep, 0 for none.
    pub(super) const RET: u16 = 0x06;
    /// A load of 16 bits.
    pub(super) const H: u16 = 0x08;
    /// A load of 8 bits.
    pub(super) const B: u16 = 0x10;
    /// A load at a fixed offset.
    pub(super) const ABS: u16 = 0x20;
    /// A load at the index register plus an offset.
    pub(super) const IND: u16 = 0x40;
    /// The index register as four times the low nibble of a byte, the length
    /// of an IPv4 header.
    pub(super) const MSH: u16 = 0xa0;
    /// Jumps when the accumulator equals the constant.
    pub(super) const JEQ: u16 = 0x10;
    /// Jumps when the accumulator is greater than the constant.
    pub(super) const JGT: u16 = 0x20;
    /// Jumps when the accumulator is at least the constant.
    pub(super) const JGE: u16 = 0x30;
    /// The operand is the constant of the instruction.
    pub(super) const K: u16 = 0x00;
}

/// Keeps the whole packet, the answer of a filter that admits it.
const KEEP: u32 = u32::MAX;

/// How the IP driver opens its raw socket.
#[derive(Clone, Debug)]
pub struct Options {
    /// IPv4 or IPv6.
    pub encap: Encap,
    /// The local address, or the unspecified address for every interface.
    pub bind: IpAddr,
    /// The first Homa port this process owns on the host, which the filter
    /// admits.
    pub port: u16,
    /// The count of further ports after `port` this process owns, 0 but for
    /// the helper.
    pub extra_ports: u16,
    /// Whether packets carry their priority in the TOS byte or the traffic
    /// class.
    pub priorities: bool,
    /// Bytes of the socket's send buffer, 0 for the kernel's default.
    pub send_buffer: usize,
    /// Bytes of the socket's receive buffer, 0 for the kernel's default.
    pub recv_buffer: usize,
}

impl Options {
    /// Returns the options of one port on every interface of `encap`, with
    /// priorities and the kernel's buffers.
    #[must_use]
    pub const fn new(encap: Encap, port: u16) -> Self {
        Self {
            encap,
            bind: match encap {
                Encap::Ip4 => IpAddr::V4(Ipv4Addr::UNSPECIFIED),
                Encap::Ip6 => IpAddr::V6(core::net::Ipv6Addr::UNSPECIFIED),
            },
            port,
            extra_ports: 0,
            priorities: true,
            send_buffer: 0,
            recv_buffer: 0,
        }
    }
}

/// Why the raw socket did not open.
#[derive(Debug)]
pub enum OpenError {
    /// The process lacks `CAP_NET_RAW` in its network namespace.
    NotPermitted,
    /// Another process of the host holds one of the Homa ports, the first
    /// such port.
    PortTaken(u16),
    /// Any other failure of a system call, and options that name no port, a
    /// range past port 65,535 or an address of the other family, which read
    /// `InvalidInput`.
    Io(io::Error),
}

impl fmt::Display for OpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotPermitted => {
                f.write_str("the process lacks CAP_NET_RAW in its network namespace")
            }
            Self::PortTaken(port) => write!(f, "another process holds the Homa port {port}"),
            Self::Io(error) => write!(f, "{error}"),
        }
    }
}

impl core::error::Error for OpenError {}

/// One instruction of a classic BPF program, the fields of
/// `struct sock_filter`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Instruction {
    /// The operation.
    pub code: u16,
    /// The instructions to skip where a jump holds.
    pub jt: u8,
    /// The instructions to skip where a jump does not hold.
    pub jf: u8,
    /// The constant.
    pub k: u32,
}

impl Instruction {
    /// Returns the instruction of `code` with the constant `k` and no jump.
    const fn op(code: u16, k: u32) -> Self {
        Self {
            code,
            jt: 0,
            jf: 0,
            k,
        }
    }

    /// Returns the jump of `code` over `k`, skipping `jt` instructions where
    /// it holds and `jf` where it does not.
    const fn jump(code: u16, k: u32, jt: u8, jf: u8) -> Self {
        Self { code, jt, jf, k }
    }
}

/// Returns the program that admits a packet of `encap` whose Homa
/// destination port lies in the `count` ports from `first`.
///
/// On IPv4 it reads the length of the IP header into the index register and
/// the port behind it, and on IPv6 the port at offset 2, then compares it
/// with the one port, or with both ends of the range, keeps the whole packet
/// where it lies inside and drops it otherwise. A `count` of 0 reads as 1.
#[must_use]
pub fn filter(encap: Encap, first: u16, count: u16) -> Vec<Instruction> {
    let mut program = match encap {
        Encap::Ip4 => vec![
            Instruction::op(bpf::LDX | bpf::B | bpf::MSH, 0),
            Instruction::op(bpf::LD | bpf::H | bpf::IND, 2),
        ],
        Encap::Ip6 => vec![Instruction::op(bpf::LD | bpf::H | bpf::ABS, 2)],
    };
    let last = u32::from(first).saturating_add(u32::from(count.max(1)) - 1);
    if last == u32::from(first) {
        program.push(Instruction::jump(bpf::JMP | bpf::JEQ | bpf::K, last, 0, 1));
    } else {
        program.push(Instruction::jump(
            bpf::JMP | bpf::JGE | bpf::K,
            u32::from(first),
            0,
            2,
        ));
        program.push(Instruction::jump(bpf::JMP | bpf::JGT | bpf::K, last, 1, 0));
    }
    program.push(Instruction::op(bpf::RET | bpf::K, KEEP));
    program.push(Instruction::op(bpf::RET | bpf::K, 0));
    program
}

/// What the driver counted since it opened.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    /// Homa packets handed to the engine.
    pub received: u64,
    /// Homa packets the kernel took for sending.
    pub sent: u64,
    /// Datagrams too short for an IP header or for the ports of a Homa
    /// header, dropped.
    pub malformed: u64,
    /// Packets longer than the engine's buffer, dropped.
    pub truncated: u64,
    /// Packets for a peer of the other address family, dropped.
    pub other_family: u64,
    /// Sends the kernel refused for their destination, dropped as a lost
    /// packet is, which a RESEND recovers.
    pub send_failed: u64,
    /// Packets dropped because one waited for the socket already, which an
    /// engine that asks [`Carrier::ready`] first never meets.
    pub dropped: u64,
}

/// The socket of either family.
#[derive(Debug)]
enum Raw {
    /// An IPv4 socket, read through `Read`, which hands the IP header with
    /// the packet.
    V4(Socket),
    /// An IPv6 socket as the standard library's `UdpSocket`, whose
    /// `recv_from` hands the source address.
    V6(UdpSocket),
}

impl Raw {
    /// Returns a `socket2` view of the socket.
    fn sock(&self) -> SockRef<'_> {
        match self {
            Self::V4(socket) => SockRef::from(socket),
            Self::V6(socket) => SockRef::from(socket),
        }
    }
}

/// A packet the socket did not take yet.
#[derive(Debug)]
struct Pending {
    /// Where it goes.
    to: SocketAddr,
    /// Its level.
    priority: Priority,
    /// The header and the payload.
    bytes: Vec<u8>,
}

/// A raw socket of protocol 146 from `socket2` with its feature `all`, its
/// filter and its claims.
#[derive(Debug)]
pub struct RawSocket {
    /// The socket.
    raw: Raw,
    /// The Unix sockets bound to the abstract names that claim the ports,
    /// held for as long as the socket lives.
    claims: Vec<UnixListener>,
    /// The first Homa port of the process.
    port: u16,
    /// The count of ports from `port`.
    count: u16,
    /// Whether a send carries its level.
    priorities: bool,
    /// The buffer every datagram is read into.
    rx: Vec<u8>,
    /// The packet the socket did not take yet.
    pending: Option<Pending>,
    /// The counters.
    counts: Counts,
}

/// Returns whether `error` says the socket would block.
fn would_block(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::WouldBlock
}

/// Returns whether `error` says a signal interrupted the call.
fn interrupted(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::Interrupted
}

/// Returns an `InvalidInput` error with `why`.
fn invalid(why: &str) -> OpenError {
    OpenError::Io(io::Error::new(io::ErrorKind::InvalidInput, why))
}

/// Claims `port` by binding a Unix socket to its abstract name.
fn claim(port: u16) -> Result<UnixListener, OpenError> {
    let name = format!("{CLAIM}{port}");
    let addr = UnixAddr::from_abstract_name(name.as_bytes()).map_err(OpenError::Io)?;
    UnixListener::bind_addr(&addr).map_err(|error| {
        if error.kind() == io::ErrorKind::AddrInUse {
            OpenError::PortTaken(port)
        } else {
            OpenError::Io(error)
        }
    })
}

/// Writes into `out` the control message that sets the level of one send,
/// and returns its length.
///
/// A control message is a `struct cmsghdr` of a length of the width of a
/// pointer, the level and the type, followed by its data, here one `int`,
/// padded to the width of a pointer, in the byte order of the machine.
fn control(encap: Encap, priority: Priority, out: &mut [u8; 32]) -> usize {
    let (level, kind) = match encap {
        Encap::Ip4 => (SOL_IP, IP_TOS),
        Encap::Ip6 => (SOL_IPV6, IPV6_TCLASS),
    };
    let word = core::mem::size_of::<usize>();
    let header = word + 2 * core::mem::size_of::<c_int>();
    let data = core::mem::size_of::<c_int>();
    let len = header + data;
    let space = header + data.div_ceil(word) * word;
    out.fill(0);
    let fields = [
        (0, &len.to_ne_bytes()[..]),
        (word, &level.to_ne_bytes()[..]),
        (word + 4, &kind.to_ne_bytes()[..]),
        (header, &c_int::from(priority.tos()).to_ne_bytes()[..]),
    ];
    for (at, bytes) in fields {
        if let Some(slot) = out.get_mut(at..at + bytes.len()) {
            slot.copy_from_slice(bytes);
        }
    }
    space.min(out.len())
}

/// Returns the source address of the IPv4 datagram `bytes` and where its
/// payload starts and ends, or `None` for bytes that hold no IPv4 header.
fn ipv4_payload(bytes: &[u8]) -> Option<(Ipv4Addr, usize, usize)> {
    let first = *bytes.first()?;
    let header = usize::from(first & 0x0f) * 4;
    if first >> 4 != 4 || header < IPV4_MIN_HEADER || bytes.len() < header {
        return None;
    }
    let total = usize::from(u16::from_be_bytes([*bytes.get(2)?, *bytes.get(3)?]));
    let end = total.min(bytes.len()).max(header);
    let source = bytes.get(12..16)?;
    let source = Ipv4Addr::new(source[0], source[1], source[2], source[3]);
    Some((source, header, end))
}

impl RawSocket {
    /// Claims every port, opens the socket, attaches the filter and sizes
    /// the buffers.
    ///
    /// # Errors
    ///
    /// Returns [`OpenError::PortTaken`] where another process of the network
    /// namespace claimed one of the ports, [`OpenError::NotPermitted`] where
    /// the process lacks `CAP_NET_RAW`, and [`OpenError::Io`] for any other
    /// failure, `InvalidInput` for a port of 0, a range past port 65,535 or a
    /// local address of the other family.
    pub fn open(options: &Options) -> Result<Self, OpenError> {
        if options.port == 0 {
            return Err(invalid("a Homa port is never 0"));
        }
        if u32::from(options.port) + u32::from(options.extra_ports) > u32::from(u16::MAX) {
            return Err(invalid("the ports run past 65,535"));
        }
        let domain = match (options.encap, options.bind) {
            (Encap::Ip4, IpAddr::V4(_)) => Domain::IPV4,
            (Encap::Ip6, IpAddr::V6(_)) => Domain::IPV6,
            _ => return Err(invalid("the local address is of the other family")),
        };
        let count = options.extra_ports.saturating_add(1);
        let mut claims = Vec::with_capacity(usize::from(count));
        for n in 0..count {
            claims.push(claim(options.port + n)?);
        }
        let socket = Socket::new(domain, Type::RAW, Some(Protocol::from(IPPROTO_HOMA))).map_err(
            |error| {
                if error.kind() == io::ErrorKind::PermissionDenied {
                    OpenError::NotPermitted
                } else {
                    OpenError::Io(error)
                }
            },
        )?;
        if !options.bind.is_unspecified() {
            socket
                .bind(&SockAddr::from(SocketAddr::new(options.bind, 0)))
                .map_err(OpenError::Io)?;
        }
        let program: Vec<SockFilter> = filter(options.encap, options.port, count)
            .iter()
            .map(|op| SockFilter::new(op.code, op.jt, op.jf, op.k))
            .collect();
        socket.attach_filter(&program).map_err(OpenError::Io)?;
        if options.recv_buffer > 0 {
            socket
                .set_recv_buffer_size(options.recv_buffer)
                .map_err(OpenError::Io)?;
        }
        if options.send_buffer > 0 {
            socket
                .set_send_buffer_size(options.send_buffer)
                .map_err(OpenError::Io)?;
        }
        socket.set_nonblocking(true).map_err(OpenError::Io)?;
        let raw = match options.encap {
            Encap::Ip4 => Raw::V4(socket),
            Encap::Ip6 => Raw::V6(UdpSocket::from(socket)),
        };
        Ok(Self {
            raw,
            claims,
            port: options.port,
            count,
            priorities: options.priorities,
            rx: vec![0; MAX_DATAGRAM],
            pending: None,
            counts: Counts::default(),
        })
    }

    /// Returns the first Homa port of the process and the count of ports
    /// from it.
    #[must_use]
    pub const fn ports(&self) -> (u16, u16) {
        (self.port, self.count)
    }

    /// Returns what the driver counted since it opened.
    #[must_use]
    pub const fn counts(&self) -> Counts {
        self.counts
    }

    /// Returns how many abstract names the socket holds, one for each port.
    #[must_use]
    pub fn claims(&self) -> usize {
        self.claims.len()
    }

    /// Returns the family of the socket.
    const fn encap(&self) -> Encap {
        match self.raw {
            Raw::V4(_) => Encap::Ip4,
            Raw::V6(_) => Encap::Ip6,
        }
    }

    /// Writes the packet that waits for the socket, and returns whether none
    /// waits any more.
    ///
    /// A send the kernel refuses for its destination drops the packet and
    /// counts it in [`Counts::send_failed`], as a lost packet, which a
    /// RESEND recovers.
    pub fn flush(&mut self) -> bool {
        let Some(pending) = self.pending.take() else {
            return true;
        };
        if self.transmit(pending.to, pending.priority, &pending.bytes, &[]) {
            true
        } else {
            self.pending = Some(pending);
            false
        }
    }

    /// Waits until a packet is waiting or `timeout` has passed, whichever
    /// comes first, and returns at once for a `timeout` below a microsecond.
    ///
    /// The socket blocks for the wait alone, with the timeout as its receive
    /// timeout and a peek of no bytes, which leaves the packet queued, and
    /// is non blocking again before the call returns.
    ///
    /// # Errors
    ///
    /// Returns the error of a system call other than the end of the timeout.
    pub fn wait(&self, timeout: Duration) -> io::Result<()> {
        if timeout < Duration::from_micros(1) {
            return Ok(());
        }
        let sock = self.raw.sock();
        sock.set_read_timeout(Some(timeout))?;
        sock.set_nonblocking(false)?;
        let peeked = match &self.raw {
            Raw::V4(socket) => socket.peek(&mut []),
            Raw::V6(socket) => socket.peek(&mut []),
        };
        sock.set_nonblocking(true)?;
        match peeked {
            Ok(_) => Ok(()),
            Err(error) if would_block(&error) || interrupted(&error) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::TimedOut => Ok(()),
            Err(error) => Err(error),
        }
    }

    /// Sends one packet to `to` at `priority`, and returns whether the socket
    /// took it or dropped it, false where it would block.
    fn transmit(
        &mut self,
        to: SocketAddr,
        priority: Priority,
        header: &[u8],
        payload: &[u8],
    ) -> bool {
        let encap = self.encap();
        let target = match (encap, to) {
            (Encap::Ip4, SocketAddr::V4(addr)) => {
                SockAddr::from(SocketAddr::new(IpAddr::V4(*addr.ip()), 0))
            }
            // The port of an IPv6 raw send names the protocol and must be 0
            // or 146, so the Homa port stays in the Homa header alone.
            (Encap::Ip6, SocketAddr::V6(addr)) => {
                SockAddr::from(SocketAddrV6::new(*addr.ip(), 0, 0, addr.scope_id()))
            }
            _ => {
                self.counts.other_family = self.counts.other_family.saturating_add(1);
                return true;
            }
        };
        let parts = [IoSlice::new(header), IoSlice::new(payload)];
        let mut cmsg = [0u8; 32];
        let cmsg_len = if self.priorities {
            control(encap, priority, &mut cmsg)
        } else {
            0
        };
        let message = MsgHdr::new().with_addr(&target).with_buffers(&parts);
        let message = match cmsg.get(..cmsg_len) {
            Some(bytes) if !bytes.is_empty() => message.with_control(bytes),
            _ => message,
        };
        loop {
            match self.raw.sock().sendmsg(&message, 0) {
                Ok(_) => {
                    self.counts.sent = self.counts.sent.saturating_add(1);
                    return true;
                }
                Err(error) if would_block(&error) => return false,
                Err(error) if interrupted(&error) => {}
                Err(_) => {
                    self.counts.send_failed = self.counts.send_failed.saturating_add(1);
                    return true;
                }
            }
        }
    }

    /// Reads the next datagram into `rx` and returns what it holds, or
    /// `None` where none is waiting.
    // An `unsafe` read would skip zeroing `rx`.
    fn read_datagram(&mut self) -> io::Result<Option<Datagram>> {
        loop {
            let read = match &self.raw {
                Raw::V4(socket) => (&*socket).read(&mut self.rx).map(|n| {
                    let bytes = self.rx.get(..n).unwrap_or(&[]);
                    match ipv4_payload(bytes) {
                        Some((source, start, end)) => Datagram::Packet {
                            source: SocketAddr::new(IpAddr::V4(source), 0),
                            start,
                            end,
                        },
                        None => Datagram::Malformed,
                    }
                }),
                Raw::V6(socket) => {
                    socket
                        .recv_from(&mut self.rx)
                        .map(|(n, source)| Datagram::Packet {
                            source,
                            start: 0,
                            end: n,
                        })
                }
            };
            match read {
                Ok(datagram) => return Ok(Some(datagram)),
                Err(error) if would_block(&error) => return Ok(None),
                Err(error) if interrupted(&error) => {}
                Err(error) => return Err(error),
            }
        }
    }
}

/// What one datagram held.
enum Datagram {
    /// A packet from `source`, whose port is not yet read, in the bytes
    /// `start..end` of the buffer.
    Packet {
        /// The source address of the IP header, with the scope of a link
        /// local IPv6 source as the kernel reported it.
        source: SocketAddr,
        /// Where the Homa packet starts.
        start: usize,
        /// Where it ends.
        end: usize,
    },
    /// Bytes that hold no IPv4 header.
    Malformed,
}

/// Returns the peer of the Homa packet `packet` from `source`, the source
/// port of its common header set on the address, or `None` for a packet too
/// short for its ports.
fn peer_of(mut source: SocketAddr, packet: &[u8]) -> Option<SocketAddr> {
    let ports = packet.get(..PORTS_LEN)?;
    source.set_port(u16::from_be_bytes([ports[0], ports[1]]));
    if let SocketAddr::V6(addr) = &mut source {
        addr.set_flowinfo(0);
    }
    Some(source)
}

impl Carrier for RawSocket {
    /// The source address of the IP header with the source port of the Homa
    /// common header.
    type Peer = SocketAddr;
    type Error = io::Error;

    /// Answers whether no packet waits for the socket.
    fn ready(&self, _peer: SocketAddr, _len: usize) -> bool {
        self.pending.is_none()
    }

    /// Sends the packet as one datagram, header and payload together, with
    /// its level where the socket carries priorities, and keeps a packet the
    /// socket does not take for [`RawSocket::flush`].
    fn send(
        &mut self,
        peer: SocketAddr,
        priority: Priority,
        header: &[u8],
        payload: &[u8],
    ) -> io::Result<()> {
        if !self.flush() {
            self.counts.dropped = self.counts.dropped.saturating_add(1);
            return Ok(());
        }
        if !self.transmit(peer, priority, header, payload) {
            let mut bytes = Vec::with_capacity(header.len() + payload.len());
            bytes.extend_from_slice(header);
            bytes.extend_from_slice(payload);
            self.pending = Some(Pending {
                to: peer,
                priority,
                bytes,
            });
        }
        Ok(())
    }

    /// Writes the packet that waits first, then takes the next datagram
    /// that holds a Homa packet and fits `buf`, dropping and counting every
    /// other one on the way.
    fn recv(&mut self, buf: &mut [u8]) -> io::Result<Option<(SocketAddr, usize)>> {
        self.flush();
        loop {
            let Some(datagram) = self.read_datagram()? else {
                return Ok(None);
            };
            let Datagram::Packet { source, start, end } = datagram else {
                self.counts.malformed = self.counts.malformed.saturating_add(1);
                continue;
            };
            let packet = self.rx.get(start..end).unwrap_or(&[]);
            let Some(peer) = peer_of(source, packet) else {
                self.counts.malformed = self.counts.malformed.saturating_add(1);
                continue;
            };
            let Some(target) = buf.get_mut(..packet.len()) else {
                self.counts.truncated = self.counts.truncated.saturating_add(1);
                continue;
            };
            target.copy_from_slice(packet);
            self.counts.received = self.counts.received.saturating_add(1);
            return Ok(Some((peer, packet.len())));
        }
    }
}

impl AsFd for RawSocket {
    fn as_fd(&self) -> BorrowedFd<'_> {
        match &self.raw {
            Raw::V4(socket) => socket.as_fd(),
            Raw::V6(socket) => socket.as_fd(),
        }
    }
}
