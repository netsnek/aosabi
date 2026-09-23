// SPDX-License-Identifier: MIT
/*
 * homa-raw: the helper that runs the client role of the Network profile over
 * raw IP 146 once per host for any local program, Node.js, Python, Go or a
 * shell, which reach Network with file permissions alone and hold no
 * capability.
 *
 * It names nothing of Aosabi and reads no byte of GraphTP.
 */

//! The helper `homa-raw`.
//!
//! One engine of the crate in its client role runs over one raw socket of
//! [`homa::ip`] and owns a range of Homa ports, `--ports <first>:<count>`,
//! which its socket claims and its filter admits. A local program connects
//! to the Unix stream socket `--socket <path>`, and each connection takes
//! the lowest free port of the range as the source port of every request it
//! sends, so each connection is a peer of its own to every server, with its
//! own RPCs, as a socket of a kernel module is. A connection past the range
//! is answered `FAILED` with id 0 and reason 255 and closed.
//!
//! **The local protocol**, big endian, in frames of a `u32` length that
//! counts the op and the body, then a `u8` op and the body:
//!
//! | Op | Name | Direction | Body |
//! | ---: | --- | --- | --- |
//! | 1 | `REQUEST` | To the helper | `family: u8`, 4 or 6, the address in 4 or 16 bytes, `port: u16`, `patient: u8`, `report_unknown: u8`, then the message to the end of the frame |
//! | 2 | `ID` | From the helper, once per `REQUEST`, in its order | `id: u64`, the engine's identifier of the RPC |
//! | 3 | `RESPONSE` | From the helper | `id: u64`, then the message to the end of the frame |
//! | 4 | `FAILED` | From the helper | `id: u64`, `why: u8`: 1 `LinkDown`, 2 `PeerTimeout`, 3 `TooLong`, 4 `Aborted`, 5 `RpcUnknown`, 255 a failure of the helper itself |
//! | 5 | `ABORT` | To the helper | `id: u64` |
//!
//! Every `REQUEST` is answered by exactly one `ID`, and every RPC whose `ID`
//! is not 0 ends with exactly one `RESPONSE` or `FAILED`. A request the
//! engine refuses, or one of a family the helper does not carry, is answered
//! `ID` with id 0 and then `FAILED` with id 0 and the reason, 3 for a
//! message longer than a Homa message and 255 otherwise, and an `ABORT` is
//! answered `FAILED` with reason 4, since the engine ends an aborted RPC
//! without an event. A frame longer than a Homa message plus 32 bytes, an op
//! a program never sends or a truncated body closes the connection and
//! aborts its RPCs, and a message is never read past its frame.
//!
//! **The loop.** One thread of the standard library runs everything: it
//! accepts and reads the local connections without blocking, runs the
//! engine over the raw socket, hands each event to the connection whose RPC
//! it ends, writes what the connections take, and then waits on the raw
//! socket for at most the engine's next deadline and never more than 1 ms,
//! so a local request waits at most that long before it leaves.
//!
//! **The exits.** 64 for a command line the helper does not take, among
//! them a start without `--protection`, which has no default because Homa
//! carries no security of its own, 77 where the process lacks `CAP_NET_RAW`,
//! and 69 where the service cannot run: another process holds one of the
//! ports or the socket path, or a system call of the raw socket or the local
//! socket fails.

use std::{
    ffi::OsString,
    fs,
    io::{
        self,
        Read as _,
        Write as _, //
    },
    net::{
        IpAddr,
        Ipv4Addr,
        Ipv6Addr,
        SocketAddr, //
    },
    os::unix::{
        fs::{
            FileTypeExt as _,
            PermissionsExt as _, //
        },
        net::{
            UnixListener,
            UnixStream, //
        },
    },
    path::PathBuf,
    process::ExitCode,
    time::{
        Duration,
        Instant, //
    },
};

use homa::{
    ip::{
        OpenError,
        Options,
        RawSocket, //
    },
    Config,
    Encap,
    Endpoint,
    Event,
    Failure,
    Owned,
    Profile,
    Read as _,
    RequestOptions,
    RpcId,
    SendError, //
};

/// The exit of a command line the helper does not take (`EX_USAGE` of
/// `sysexits.h`).
const EX_USAGE: u8 = 64;

/// The exit where the service cannot run (`EX_UNAVAILABLE`).
const EX_UNAVAILABLE: u8 = 69;

/// The exit where the process lacks a permission (`EX_NOPERM`).
const EX_NOPERM: u8 = 77;

/// The longest Homa message.
const MAX_MESSAGE: usize = 1_000_000;

/// The bytes of one page of the engine, so that the 16 pages of a slot hold
/// a message of [`MAX_MESSAGE`] bytes and no byte more.
///
/// A longer request is refused as `TooLong` before a byte leaves.
const PAGE_LEN: u32 = 62_500;

/// The bytes of the page pool, 16 MiB, which every connection of the port
/// range draws from, so the pool is shared by the ports and never grows with
/// `--ports`.
const POOL: u32 = 16 << 20;

/// The pages of the pool, 268 of [`PAGE_LEN`] bytes, 16,750,000 bytes, which
/// with the two bytes a page of the page table stay inside [`POOL`].
///
/// They hold 16 messages of [`MAX_MESSAGE`] bytes at once. A request past
/// the free pages is refused as a request past the slots is, `ID` 0 and
/// `FAILED` with reason 255, and a response past them waits, since the
/// engine refuses its first packet until its sender sends it again.
const PAGES: u32 = POOL / (PAGE_LEN + 2);

/// The bytes a frame may hold beyond a message: the op and the head of a
/// request of IPv6, 1 + 1 + 16 + 2 + 1 + 1, rounded up.
const FRAME_SLACK: usize = 32;

/// The bytes of the length prefix of a frame.
const PREFIX: usize = 4;

/// The ops of the local protocol.
const OP_REQUEST: u8 = 1;
const OP_ID: u8 = 2;
const OP_RESPONSE: u8 = 3;
const OP_FAILED: u8 = 4;
const OP_ABORT: u8 = 5;

/// The reason of a failure of the helper itself.
const WHY_HELPER: u8 = 255;

/// The reason of an aborted RPC.
const WHY_ABORTED: u8 = 4;

/// The reason of a request longer than the peer takes.
const WHY_TOO_LONG: u8 = 3;

/// The bytes one connection may have waiting to be written before the
/// helper takes it as a program that no longer reads, four whole messages.
const MAX_BACKLOG: usize = 4 * (MAX_MESSAGE + FRAME_SLACK + PREFIX);

/// The bytes read from one connection in one sweep, so one busy program
/// cannot hold the loop.
const READ_CHUNK: usize = 1 << 16;

/// The longest wait on the raw socket.
const MAX_WAIT: Duration = Duration::from_millis(1);

/// The MTU of the Network path the profile assumes.
const MTU: u16 = 1_500;

/// The rate of the host's link in bits a second, `HomaModule`'s `link_mbps`
/// of 25,000.
const LINK_BITS_PER_S: u64 = 25_000_000_000;

/// The backlog the rate pacer lets wait in the link, and the shortest
/// message it holds back, `HomaModule`'s defaults.
const PACER_BACKLOG_NS: u64 = 5_000;
const PACER_DEFER_MIN: u32 = 1_000;

/// How the path under Homa is protected, which the administrator states,
/// since Homa carries no security of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Protection {
    /// `IPsec`, which keeps the DS field, so the priorities reach the switch.
    Ipsec,
    /// A fabric the administrator trusts.
    TrustedFabric,
    /// `WireGuard`, which writes 0 into the outer TOS, so the packets carry
    /// no priority.
    WireguardWithoutPriorities,
}

impl Protection {
    /// Returns the protection a command line names.
    fn parse(text: &str) -> Option<Self> {
        match text {
            "ipsec" => Some(Self::Ipsec),
            "trusted-fabric" => Some(Self::TrustedFabric),
            "wireguard-without-priorities" => Some(Self::WireguardWithoutPriorities),
            _ => None,
        }
    }

    /// Returns the name the command line takes.
    const fn name(self) -> &'static str {
        match self {
            Self::Ipsec => "ipsec",
            Self::TrustedFabric => "trusted-fabric",
            Self::WireguardWithoutPriorities => "wireguard-without-priorities",
        }
    }
}

/// The options of the command line.
#[derive(Debug)]
struct Args {
    /// The path of the local socket.
    socket: PathBuf,
    /// The group the local socket is given, and whether the command line
    /// named it.
    group: (String, bool),
    /// The first client port and the count of ports.
    ports: (u16, u16),
    /// The local address, or the unspecified address.
    bind: Option<IpAddr>,
    /// IPv4 or IPv6.
    encap: Encap,
    /// The Homa processes of the host, which share `max_incoming` and the
    /// link.
    processes: u32,
    /// How the path is protected.
    protection: Protection,
}

/// The text `--help` prints.
const USAGE: &str = "usage: homa-raw --protection ipsec|trusted-fabric|wireguard-without-priorities
                [--socket PATH] [--group NAME] [--ports FIRST:COUNT]
                [--bind ADDRESS] [--encap ip4|ip6] [--processes N]

  --protection  how the path under Homa is protected, no default
  --socket      the local stream socket, /run/homa-raw.sock by default, mode 0660
  --group       the group of the socket, homa by default
  --ports       the client ports the helper claims, 40000:64 by default
  --bind        the local address, every address by default
  --encap       ip4 or ip6, ip4 by default
  --processes   the Homa processes of the host, 1 by default";

/// Why the helper stops, with its exit.
#[derive(Debug)]
struct Stop {
    /// The exit.
    code: u8,
    /// What the helper prints.
    why: String,
}

impl Stop {
    /// Returns a stop with `code` and `why`.
    fn new(code: u8, why: impl Into<String>) -> Self {
        Self {
            code,
            why: why.into(),
        }
    }
}

/// Returns the value after a flag, or why there is none.
fn value(flag: &str, next: Option<OsString>) -> Result<String, Stop> {
    next.and_then(|value| value.into_string().ok())
        .ok_or_else(|| Stop::new(EX_USAGE, format!("{flag} takes a value")))
}

/// Returns the options of the command line, or why it is not taken.
fn parse(mut argv: impl Iterator<Item = OsString>) -> Result<Args, Stop> {
    let mut socket = PathBuf::from("/run/homa-raw.sock");
    let mut group = (String::from("homa"), false);
    let mut ports = (40_000u16, 64u16);
    let mut bind = None;
    let mut encap = Encap::Ip4;
    let mut processes = 1u32;
    let mut protection = None;
    while let Some(arg) = argv.next() {
        let arg = arg
            .into_string()
            .map_err(|_| Stop::new(EX_USAGE, "an argument is not UTF-8"))?;
        match arg.as_str() {
            "--help" | "-h" => return Err(Stop::new(0, USAGE)),
            "--socket" => socket = PathBuf::from(value(&arg, argv.next())?),
            "--group" => group = (value(&arg, argv.next())?, true),
            "--ports" => {
                let text = value(&arg, argv.next())?;
                ports = text
                    .split_once(':')
                    .and_then(|(first, count)| Some((first.parse().ok()?, count.parse().ok()?)))
                    .filter(|&(first, count): &(u16, u16)| {
                        first > 0 && count > 0 && u32::from(first) + u32::from(count) - 1 <= 65_535
                    })
                    .ok_or_else(|| {
                        Stop::new(
                            EX_USAGE,
                            format!("--ports takes FIRST:COUNT inside 1 to 65535, not {text}"),
                        )
                    })?;
            }
            "--bind" => {
                let text = value(&arg, argv.next())?;
                bind = Some(text.parse().map_err(|_| {
                    Stop::new(EX_USAGE, format!("--bind takes an address, not {text}"))
                })?);
            }
            "--encap" => {
                encap = match value(&arg, argv.next())?.as_str() {
                    "ip4" => Encap::Ip4,
                    "ip6" => Encap::Ip6,
                    other => {
                        return Err(Stop::new(
                            EX_USAGE,
                            format!("--encap takes ip4 or ip6, not {other}"),
                        ))
                    }
                };
            }
            "--processes" => {
                let text = value(&arg, argv.next())?;
                processes = text.parse().ok().filter(|&n: &u32| n > 0).ok_or_else(|| {
                    Stop::new(
                        EX_USAGE,
                        format!("--processes takes a count above 0, not {text}"),
                    )
                })?;
            }
            "--protection" => {
                let text = value(&arg, argv.next())?;
                protection = Some(Protection::parse(&text).ok_or_else(|| {
                    Stop::new(
                        EX_USAGE,
                        format!("--protection takes ipsec, trusted-fabric or wireguard-without-priorities, not {text}"),
                    )
                })?);
            }
            other => return Err(Stop::new(EX_USAGE, format!("unknown argument {other}"))),
        }
    }
    let protection = protection.ok_or_else(|| {
        Stop::new(
            EX_USAGE,
            "--protection is required and has no default: Homa carries no security of its own, so the administrator states how the path under it is protected, ipsec, trusted-fabric or wireguard-without-priorities",
        )
    })?;
    match (encap, bind) {
        (Encap::Ip4, Some(IpAddr::V6(_))) | (Encap::Ip6, Some(IpAddr::V4(_))) => {
            return Err(Stop::new(
                EX_USAGE,
                "--bind names an address of the other family than --encap",
            ));
        }
        _ => {}
    }
    Ok(Args {
        socket,
        group,
        ports,
        bind,
        encap,
        processes,
        protection,
    })
}

/// Returns `N` bytes of the system's randomness, which seeds the first
/// identifier and the key of the engine's table.
fn random_bytes<const N: usize>() -> io::Result<[u8; N]> {
    let mut bytes = [0u8; N];
    fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes)
}

/// Returns the identifier of the group `name` from `/etc/group`, or `None`.
fn group_id(name: &str) -> Option<u32> {
    let text = fs::read_to_string("/etc/group").ok()?;
    text.lines().find_map(|line| {
        let mut fields = line.split(':');
        if fields.next()? != name {
            return None;
        }
        fields.nth(1)?.parse().ok()
    })
}

/// Binds the local socket at the path of `args`, removing a socket file
/// no process listens on any more, with mode 0660 and the group of `args`.
fn listen(args: &Args) -> Result<UnixListener, Stop> {
    let path = &args.socket;
    if let Ok(meta) = fs::symlink_metadata(path) {
        if !meta.file_type().is_socket() {
            return Err(Stop::new(
                EX_UNAVAILABLE,
                format!("{} exists and is no socket", path.display()),
            ));
        }
        match UnixStream::connect(path) {
            Err(error) if error.kind() == io::ErrorKind::ConnectionRefused => {
                fs::remove_file(path).map_err(|error| {
                    Stop::new(EX_UNAVAILABLE, format!("{}: {error}", path.display()))
                })?;
            }
            _ => {
                return Err(Stop::new(
                    EX_UNAVAILABLE,
                    format!("another process listens on {}", path.display()),
                ));
            }
        }
    }
    let unavailable =
        |error: io::Error| Stop::new(EX_UNAVAILABLE, format!("{}: {error}", path.display()));
    let listener = UnixListener::bind(path).map_err(unavailable)?;
    listener.set_nonblocking(true).map_err(unavailable)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o660)).map_err(unavailable)?;
    let (name, named) = &args.group;
    match group_id(name) {
        Some(gid) => std::os::unix::fs::chown(path, None, Some(gid)).map_err(unavailable)?,
        None if *named => {
            return Err(Stop::new(
                EX_USAGE,
                format!("--group names {name}, which /etc/group does not hold"),
            ));
        }
        None => eprintln!(
            "homa-raw: no group {name} on this host, so {} keeps the group of the process",
            path.display()
        ),
    }
    Ok(listener)
}

/// One local connection.
#[derive(Debug)]
struct Conn {
    /// The stream, non blocking.
    stream: UnixStream,
    /// The bytes read and not yet taken as frames.
    inbuf: Vec<u8>,
    /// The bytes waiting to be written.
    outbuf: Vec<u8>,
    /// The engine's identifiers of this connection's open RPCs.
    rpcs: Vec<RpcId>,
}

impl Conn {
    /// Queues the frame of `op` with the parts of `body`.
    fn frame(&mut self, op: u8, body: &[&[u8]]) {
        let len = 1 + body.iter().map(|part| part.len()).sum::<usize>();
        let len = u32::try_from(len).unwrap_or(u32::MAX);
        self.outbuf.extend_from_slice(&len.to_be_bytes());
        self.outbuf.push(op);
        for part in body {
            self.outbuf.extend_from_slice(part);
        }
    }

    /// Queues `ID`.
    fn id(&mut self, id: u64) {
        self.frame(OP_ID, &[&id.to_be_bytes()]);
    }

    /// Queues `FAILED`.
    fn failed(&mut self, id: u64, why: u8) {
        self.frame(OP_FAILED, &[&id.to_be_bytes(), &[why]]);
    }

    /// Writes what the stream takes, and returns whether it is still open.
    fn flush(&mut self) -> bool {
        while !self.outbuf.is_empty() {
            match self.stream.write(&self.outbuf) {
                Ok(0) => return false,
                Ok(written) => {
                    self.outbuf.drain(..written);
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return true,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(_) => return false,
            }
        }
        true
    }
}

/// What one frame of a program asks.
enum Frame {
    /// A request to `peer` with its options and message.
    Request {
        /// The server, `None` for a family the helper does not carry.
        peer: Option<SocketAddr>,
        /// Whether the answer may take long.
        patient: bool,
        /// Whether a restart reports `RpcUnknown`.
        report_unknown: bool,
        /// The message.
        message: Vec<u8>,
    },
    /// An abort of an RPC.
    Abort(u64),
}

/// Takes the next whole frame off `inbuf`, `Ok(None)` where it is not whole
/// yet, and `Err` for a frame that closes the connection.
fn next_frame(inbuf: &mut Vec<u8>, encap: Encap) -> Result<Option<Frame>, ()> {
    let Some(prefix) = inbuf.get(..PREFIX) else {
        return Ok(None);
    };
    let len = usize::try_from(u32::from_be_bytes([
        prefix[0], prefix[1], prefix[2], prefix[3],
    ]))
    .map_err(|_| ())?;
    if len == 0 || len > MAX_MESSAGE + FRAME_SLACK {
        return Err(());
    }
    let Some(frame) = inbuf.get(PREFIX..PREFIX + len) else {
        return Ok(None);
    };
    let (op, body) = frame.split_first().ok_or(())?;
    let parsed = match *op {
        OP_REQUEST => {
            let (&family, rest) = body.split_first().ok_or(())?;
            let (ip, rest) = match family {
                4 => {
                    let (address, rest) = rest.split_at_checked(4).ok_or(())?;
                    let octets: [u8; 4] = address.try_into().map_err(|_| ())?;
                    (IpAddr::V4(Ipv4Addr::from(octets)), rest)
                }
                6 => {
                    let (address, rest) = rest.split_at_checked(16).ok_or(())?;
                    let octets: [u8; 16] = address.try_into().map_err(|_| ())?;
                    (IpAddr::V6(Ipv6Addr::from(octets)), rest)
                }
                _ => return Err(()),
            };
            let (head, message) = rest.split_at_checked(4).ok_or(())?;
            let port = u16::from_be_bytes([head[0], head[1]]);
            let carried = matches!(
                (encap, ip),
                (Encap::Ip4, IpAddr::V4(_)) | (Encap::Ip6, IpAddr::V6(_))
            );
            Frame::Request {
                peer: carried.then(|| SocketAddr::new(ip, port)),
                patient: head[2] != 0,
                report_unknown: head[3] != 0,
                message: message.to_vec(),
            }
        }
        OP_ABORT => {
            let id: [u8; 8] = body.try_into().map_err(|_| ())?;
            Frame::Abort(u64::from_be_bytes(id))
        }
        _ => return Err(()),
    };
    inbuf.drain(..PREFIX + len);
    Ok(Some(parsed))
}

/// The reason `FAILED` carries for `failure`.
const fn why(failure: Failure) -> u8 {
    match failure {
        Failure::LinkDown => 1,
        Failure::PeerTimeout => 2,
        Failure::TooLong => WHY_TOO_LONG,
        Failure::Aborted => WHY_ABORTED,
        Failure::RpcUnknown => 5,
    }
}

/// The engine of the helper.
type Engine = Endpoint<SocketAddr, Owned<SocketAddr>, homa::wire::Full>;

/// The helper's state: the engine, its socket and the connections, one
/// place per client port.
struct Helper {
    /// The engine.
    engine: Engine,
    /// The raw socket.
    raw: RawSocket,
    /// The local socket.
    listener: UnixListener,
    /// The first client port.
    first: u16,
    /// One place per client port, `None` where the port is free.
    conns: Vec<Option<Conn>>,
    /// The buffer the engine receives into.
    rx: Vec<u8>,
    /// The buffer a connection is read into.
    chunk: Vec<u8>,
    /// When the helper started, the zero of the engine's clock.
    start: Instant,
    /// The family the raw socket carries.
    encap: Encap,
}

impl Helper {
    /// Returns the engine's clock now.
    fn now(&self) -> u64 {
        u64::try_from(self.start.elapsed().as_nanos()).unwrap_or(u64::MAX)
    }

    /// Accepts every waiting connection.
    fn accept(&mut self) -> Result<(), Stop> {
        loop {
            let stream = match self.listener.accept() {
                Ok((stream, _)) => stream,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(()),
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(Stop::new(EX_UNAVAILABLE, format!("accept: {error}"))),
            };
            if stream.set_nonblocking(true).is_err() {
                continue;
            }
            let mut conn = Conn {
                stream,
                inbuf: Vec::new(),
                outbuf: Vec::new(),
                rpcs: Vec::new(),
            };
            if let Some(place) = self.conns.iter().position(Option::is_none) {
                self.conns[place] = Some(conn);
            } else {
                // A connection past the range learns why before it closes.
                conn.failed(0, WHY_HELPER);
                conn.flush();
            }
        }
    }

    /// Reads every connection and hands its frames to the engine.
    fn sweep(&mut self) {
        let mut chunk = core::mem::take(&mut self.chunk);
        for place in 0..self.conns.len() {
            let Some(conn) = self.conns[place].as_mut() else {
                continue;
            };
            let open = loop {
                match conn.stream.read(&mut chunk) {
                    Ok(0) => break false,
                    Ok(read) => {
                        conn.inbuf.extend_from_slice(&chunk[..read]);
                        break true;
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => break true,
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                    Err(_) => break false,
                }
            };
            let ok = open && self.take_frames(place);
            if !ok {
                self.close(place);
            }
        }
        self.chunk = chunk;
    }

    /// Hands the whole frames of the connection at `place` to the engine,
    /// and returns false for a frame that closes it.
    fn take_frames(&mut self, place: usize) -> bool {
        let port = self.first + u16::try_from(place).unwrap_or(0);
        let encap = self.encap;
        loop {
            let Some(conn) = self.conns[place].as_mut() else {
                return false;
            };
            let frame = match next_frame(&mut conn.inbuf, encap) {
                Ok(Some(frame)) => frame,
                Ok(None) => return true,
                Err(()) => return false,
            };
            match frame {
                Frame::Request {
                    peer,
                    patient,
                    report_unknown,
                    message,
                } => {
                    let Some(peer) = peer else {
                        conn.id(0);
                        conn.failed(0, WHY_HELPER);
                        continue;
                    };
                    let options = RequestOptions {
                        patient,
                        report_unknown,
                        source_port: port,
                    };
                    match self.engine.request(peer, &message, options) {
                        Ok(id) => {
                            conn.id(id.get());
                            conn.rpcs.push(id);
                        }
                        Err(error) => {
                            conn.id(0);
                            let reason = if error == SendError::TooLong {
                                WHY_TOO_LONG
                            } else {
                                WHY_HELPER
                            };
                            conn.failed(0, reason);
                        }
                    }
                }
                Frame::Abort(id) => {
                    if let Some(at) = conn.rpcs.iter().position(|rpc| rpc.get() == id) {
                        let rpc = conn.rpcs.swap_remove(at);
                        // An RPC that ended in this sweep is gone already,
                        // and the abort needs nothing more.
                        let _ = self.engine.abort(rpc);
                        conn.failed(id, WHY_ABORTED);
                    }
                }
            }
        }
    }

    /// Closes the connection at `place` and aborts its RPCs.
    fn close(&mut self, place: usize) {
        if let Some(conn) = self.conns[place].take() {
            for rpc in conn.rpcs {
                // An RPC the engine ended already needs no abort.
                let _ = self.engine.abort(rpc);
            }
        }
    }

    /// Hands each event of the engine to the connection whose RPC it ends.
    fn events(&mut self) {
        while let Some(event) = self.engine.poll_event() {
            let (id, outcome) = match event {
                Event::Response { id } => (id, None),
                Event::Failed { id, why } => (id, Some(why)),
                _ => continue,
            };
            let owner = self.conns.iter_mut().flatten().find_map(|conn| {
                let at = conn.rpcs.iter().position(|rpc| *rpc == id)?;
                conn.rpcs.swap_remove(at);
                Some(conn)
            });
            match (owner, outcome) {
                (Some(conn), None) => {
                    if let Some(message) = self.engine.response(id) {
                        let mut bytes = vec![0u8; usize::try_from(message.len()).unwrap_or(0)];
                        let copied = message.read_at(0, &mut bytes);
                        bytes.truncate(copied);
                        conn.frame(OP_RESPONSE, &[&id.get().to_be_bytes(), &bytes]);
                    } else {
                        conn.failed(id.get(), WHY_HELPER);
                    }
                    // The release queues the acknowledgement of the response.
                    let _ = self.engine.release(id);
                }
                (Some(conn), Some(why_failed)) => conn.failed(id.get(), why(why_failed)),
                (None, None) => {
                    let _ = self.engine.release(id);
                }
                (None, Some(_)) => {}
            }
        }
    }

    /// Writes what each connection takes, closing one that failed or that
    /// holds more than it may.
    fn flush(&mut self) {
        for place in 0..self.conns.len() {
            let keep = match self.conns[place].as_mut() {
                Some(conn) => conn.flush() && conn.outbuf.len() <= MAX_BACKLOG,
                None => true,
            };
            if !keep {
                self.close(place);
            }
        }
    }

    /// Runs the helper until a system call of its sockets fails.
    fn run(&mut self) -> Stop {
        loop {
            if let Err(stop) = self.accept() {
                return stop;
            }
            self.sweep();
            let now = self.now();
            if let Err(error) = self.engine.pump(&mut self.raw, now, &mut self.rx) {
                return Stop::new(EX_UNAVAILABLE, format!("the raw socket failed: {error}"));
            }
            self.events();
            let now = self.now();
            if let Err(error) = self.engine.pump(&mut self.raw, now, &mut self.rx) {
                return Stop::new(EX_UNAVAILABLE, format!("the raw socket failed: {error}"));
            }
            self.flush();
            let now = self.now();
            let wait = self
                .engine
                .next_timeout()
                .map_or(MAX_WAIT, |at| Duration::from_nanos(at.saturating_sub(now)))
                .min(MAX_WAIT);
            if let Err(error) = self.raw.wait(wait) {
                return Stop::new(EX_UNAVAILABLE, format!("the raw socket failed: {error}"));
            }
        }
    }
}

/// Opens the engine, the raw socket and the local socket, then runs.
fn serve(args: &Args) -> Stop {
    let (first, count) = args.ports;
    let mut options = Options::new(args.encap, first);
    options.extra_ports = count - 1;
    if let Some(bind) = args.bind {
        options.bind = bind;
    }
    options.priorities = args.protection != Protection::WireguardWithoutPriorities;
    options.send_buffer = 4 << 20;
    options.recv_buffer = 4 << 20;
    let raw = match RawSocket::open(&options) {
        Ok(raw) => raw,
        Err(OpenError::NotPermitted) => {
            return Stop::new(
                EX_NOPERM,
                "NotPermitted: the process lacks CAP_NET_RAW, give the file cap_net_raw=ep or run it with AmbientCapabilities=CAP_NET_RAW",
            );
        }
        Err(OpenError::PortTaken(port)) => {
            return Stop::new(
                EX_UNAVAILABLE,
                format!("PortTaken: another process holds the Homa port {port}"),
            );
        }
        Err(OpenError::Io(error)) => {
            return Stop::new(EX_UNAVAILABLE, format!("the raw socket: {error}"))
        }
    };
    let (Ok(id), Ok(key)) = (random_bytes::<8>(), random_bytes::<16>()) else {
        return Stop::new(EX_UNAVAILABLE, "/dev/urandom cannot be read");
    };
    let first_id = (u64::from_ne_bytes(id) & !1).max(2);
    let config = Config {
        peers: 1_024,
        client_slots: 256,
        server_slots: 0,
        page_len: PAGE_LEN,
        pages: PAGES,
        max_gaps: 16,
        size_buckets: 0,
        first_id,
        hash_key: key,
        port: first,
        extra_ports: count - 1,
    };
    let mut profile = Profile::network(MTU, args.encap);
    profile.max_incoming /= args.processes;
    let mut engine = match Engine::new(config, profile, Owned::new(&config)) {
        Ok(engine) => engine,
        Err(error) => return Stop::new(EX_UNAVAILABLE, format!("the engine: {error}")),
    };
    let share = LINK_BITS_PER_S / u64::from(args.processes);
    engine.set_link_rate(share);
    engine.set_pacer(Some(homa::pacer::Rate::new(
        share,
        PACER_BACKLOG_NS,
        PACER_DEFER_MIN,
    )));
    let listener = match listen(args) {
        Ok(listener) => listener,
        Err(stop) => return stop,
    };
    println!(
        "homa-raw: serving {} on the Homa ports {} to {} over {}, protection {}, a page pool of {} bytes in {} pages the ports share",
        args.socket.display(),
        first,
        u32::from(first) + u32::from(count) - 1,
        match args.encap {
            Encap::Ip4 => "IPv4",
            Encap::Ip6 => "IPv6",
        },
        args.protection.name(),
        u64::from(PAGES) * u64::from(PAGE_LEN),
        PAGES,
    );
    // A line on standard output tells a supervisor the helper is ready.
    let _ = io::stdout().flush();
    let mut helper = Helper {
        engine,
        raw,
        listener,
        first,
        conns: (0..count).map(|_| None).collect(),
        rx: vec![0; 65_536],
        chunk: vec![0; READ_CHUNK],
        start: Instant::now(),
        encap: args.encap,
    };
    helper.run()
}

fn main() -> ExitCode {
    let stop = match parse(std::env::args_os().skip(1)) {
        Ok(args) => serve(&args),
        Err(stop) => stop,
    };
    if stop.code == 0 {
        println!("{}", stop.why);
    } else {
        eprintln!("homa-raw: {}", stop.why);
        if stop.code == EX_USAGE {
            eprintln!("{USAGE}");
        }
    }
    ExitCode::from(stop.code)
}
