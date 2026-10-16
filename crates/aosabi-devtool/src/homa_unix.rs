// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool homa_unix: a Homa server or client of the crate homa over a
 * real Unix Socket, driven against a peer in another process.
 *
 * No analogue in the reference. With the field homa false, serve answers whole
 * messages in AoFrame alone, the baseline of ADR-0023. Its judging notebooks
 * are tests/notebooks/21_homa_loopback.ipynb and
 * tests/notebooks/22_homa_lossy.ipynb.
 */

//! The `homa-unix` primitive of the seam.
//!
//! `--requests PATH` answers a file of request lines, one response line each,
//! in the batch form of [`crate::batch`]. Three operations exist, each over
//! the `form` `seqpacket`, a `SOCK_SEQPACKET` socket whose every record is
//! one Homa packet, or `stream`, a stream socket in prefixed `AoFrame`.
//!
//! - `ping` sends the bytes of `hex` from a client endpoint to a server
//!   endpoint over a pair of sockets in this process and answers the `hex`
//!   the server echoed, the smallest call of the primitive.
//! - It ends once every connection it accepted has closed, or at
//!   `max_time_ms`, and answers what each connection counted. `rebuild_after`
//!   rebuilds the engine of a connection over its open socket with the first
//!   identifier `rebuild_first_id` when that many requests of it have arrived,
//!   before the last of them is answered. `send_buffer` sets the send buffer
//!   of each accepted socket, so an answer waits in the engine while a shorter
//!   one overtakes it. `homa` false answers whole messages in `AoFrame` alone,
//!   first in first out, which on the form `seqpacket` go in records of at
//!   most a packet each.
//! - `client` connects to `path`, sends its `requests` with at most `window`
//!   open, each `{hex}` or `{len, fill}` with `patient` and
//!   `report_unknown`, and answers per request its identifier, its outcome
//!   and the SHA-256 and the length of the response it read, with the `hex`
//!   of the response where the request carried `hex`.
//!
//! A `profile` names a constructor of `homa::Profile` with `overrides`, as
//! `homa-sim` reads it, and `config`, `limits` and `assumed` name the
//! configuration, the limits this end announces and the limits it takes of
//! the peer until its LIMITS arrive. The clock of every operation is the
//! wall clock of this process, and a pass that finds nothing to do sleeps
//! until the next deadline of an engine, at most `idle_us`.

use std::{
    io,
    os::unix::net::UnixStream,
    path::{
        Path,
        PathBuf, //
    },
    process::ExitCode,
    thread,
    time::{
        Duration,
        Instant, //
    }, //
};

use aosabi_json::{
    JsObject,
    JsString,
    JsValue, //
};
use homa::{
    Carrier,
    Config,
    Endpoint,
    Event,
    Gone,
    Limits,
    Owned,
    Priority,
    Profile,
    RequestOptions,
    RpcId,
    SendError,
    stream::{
        self,
        ByteStream,
        Decoder,
        Framed,
        Mode, //
    },
    unix::{
        self,
        SeqListener,
        SeqPacket,
        StreamListener, //
    },
    wire::Compact, //
};

use crate::{
    batch::{
        self,
        Operations, //
    },
    cli::{
        self,
        Grammar,
        Matches, //
    },
    homa_packet::{
        Fields,
        count,
        hex,
        unhex, //
    },
    homa_sim::{
        config_error_name,
        config_of,
        request_profile,
        scenario::{
            Fill,
            failure_name,
            link_error_name,
            read_all,
            send_error_name,
            sha256_hex,
            stats_json, //
        }, //
    },
    seam, //
};

/// The name of the primitive.
const NAME: &str = "homa-unix";

/// The stage the primitive lands at.
const LANDS_AT: &str = "H2";

/// The command line of the `homa-unix` primitive.
pub static GRAMMAR: Grammar = Grammar {
    options: &[cli::SMOKE, cli::REQUESTS],
    positional: None,
    conflicts: &[],
};

/// What the `homa-unix` primitive accepts.
#[derive(Debug)]
pub struct HomaUnixArgs {
    /// Whether the call asks for the interface line.
    smoke: bool,
    /// The file of request lines of the batch form.
    requests: Option<PathBuf>,
}

impl HomaUnixArgs {
    /// Reads the arguments `matches` holds for [`GRAMMAR`].
    pub fn read(matches: &Matches) -> Self {
        Self {
            smoke: matches.flag("smoke"),
            requests: matches.path("requests"),
        }
    }
}

/// The operations of the batch form.
const OPERATIONS: &Operations = &[
    ("ping", op_ping),
    ("serve", op_serve),
    ("client", op_client),
];

/// Runs the `homa-unix` primitive.
pub fn run(args: &HomaUnixArgs) -> ExitCode {
    if args.smoke {
        return seam::smoke_at(NAME, LANDS_AT);
    }
    let outcome = seam::required(args.requests.as_deref(), "--requests")
        .and_then(|path| batch::run(OPERATIONS, path));
    seam::finish(NAME, outcome)
}

/// An endpoint of one connection: one peer, storage it owns, the compact
/// form.
type Engine = Endpoint<(), Owned<()>, Compact>;

/// The bytes of the receive buffer of every loop, a packet of the largest
/// profile and a whole message of the baseline with room to spare.
const RX_LEN: usize = 1 << 20;

/// The configuration of a server endpoint a request that names none takes:
/// sixteen server slots and 256 pages of 8,192 bytes, sixteen pages a slot,
/// the most a slot may take, so a request and an answer of 64 KiB each fit
/// every slot at once.
const SERVE_CONFIG: Config = Config {
    peers: 1,
    client_slots: 0,
    server_slots: 16,
    page_len: 8_192,
    pages: 256,
    max_gaps: 4,
    size_buckets: 0,
    first_id: 0x2000_0002,
    hash_key: [0x5a; 16],
    port: 0,
    extra_ports: 0,
};

/// The configuration of a client endpoint a request that names none takes:
/// sixteen client slots and 256 pages of 8,192 bytes.
const CLIENT_CONFIG: Config = Config {
    peers: 1,
    client_slots: 16,
    server_slots: 0,
    page_len: 8_192,
    pages: 256,
    max_gaps: 4,
    size_buckets: 0,
    first_id: 0x1000_0002,
    hash_key: [0x5a; 16],
    port: 0,
    extra_ports: 0,
};

/// The two forms of a Unix Socket link.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Form {
    /// A `SOCK_SEQPACKET` socket, one Homa packet a record.
    SeqPacket,
    /// A stream socket in prefixed `AoFrame`.
    Stream,
}

/// Reads the form a request names under `form`.
fn form_of(fields: &Fields<'_>) -> Result<Form, JsString> {
    match fields.str("form")? {
        "seqpacket" => Ok(Form::SeqPacket),
        "stream" => Ok(Form::Stream),
        other => Err(fields.refuse("form", &format!("names no form of a Unix Socket: {other}"))),
    }
}

/// Returns the refusal of an operation that a system call failed.
fn io_refused(fields: &Fields<'_>, what: &str, error: &io::Error) -> JsString {
    JsString::from(format!("{}: {what}: {error}", fields.op_name()))
}

/// The byte stream of a sequenced packet socket for `AoFrame` alone: the
/// bytes written go in records of at most a packet each, and the bytes read
/// come out of the records in order.
struct SeqBytes {
    /// The socket.
    link: SeqPacket,
    /// The record read last.
    record: Vec<u8>,
    /// The first byte of `record` not yet read.
    start: usize,
    /// The end of the bytes of `record`.
    end: usize,
}

impl SeqBytes {
    /// Wraps `link`, whose records carry at most `max_packet` bytes.
    fn new(link: SeqPacket, max_packet: u16) -> Self {
        Self {
            link,
            record: vec![0; usize::from(max_packet)],
            start: 0,
            end: 0,
        }
    }
}

impl ByteStream for SeqBytes {
    type Error = io::Error;

    fn read(&mut self, buf: &mut [u8]) -> io::Result<Option<usize>> {
        if self.start >= self.end {
            match self.link.recv(&mut self.record)? {
                Some(((), len)) => {
                    self.start = 0;
                    self.end = len;
                }
                None if self.link.closed() => return Ok(Some(0)),
                None => return Ok(None),
            }
        }
        let take = buf.len().min(self.end - self.start);
        buf[..take].copy_from_slice(&self.record[self.start..self.start + take]);
        self.start += take;
        Ok(Some(take))
    }

    fn write(&mut self, bytes: &[u8]) -> io::Result<Option<usize>> {
        if !self.link.flush()? || !self.link.ready((), 0) {
            return Ok(None);
        }
        let take = bytes.len().min(self.record.len());
        self.link.send((), Priority::HIGHEST, &bytes[..take], &[])?;
        Ok(Some(take))
    }
}

/// The link of one connection.
enum Link<'b> {
    /// A sequenced packet socket, one Homa packet a record.
    Seq(SeqPacket),
    /// A stream socket in prefixed `AoFrame`.
    Stream(Framed<'b, UnixStream>),
    /// A sequenced packet socket carrying prefixed `AoFrame`, whose frames
    /// hold whole messages for the baseline.
    SeqFramed(Framed<'b, SeqBytes>),
}

impl Link<'_> {
    /// Returns whether the peer closed its end.
    fn closed(&self) -> bool {
        match self {
            Self::Seq(link) => link.closed(),
            Self::Stream(link) => link.closed(),
            Self::SeqFramed(link) => link.closed() || link.stream().link.closed(),
        }
    }

    /// Returns the records the socket dropped as longer than a packet.
    fn truncated(&self) -> u64 {
        match self {
            Self::Seq(link) => link.truncated(),
            Self::Stream(_) => 0,
            Self::SeqFramed(link) => link.stream().link.truncated(),
        }
    }

    /// Returns the packets or frames the link dropped for want of room or of
    /// a check.
    fn dropped(&self) -> u64 {
        match self {
            Self::Seq(link) => link.dropped(),
            Self::Stream(link) => link.dropped(),
            Self::SeqFramed(link) => link.dropped(),
        }
    }

    /// Returns the bytes the link adds to a packet or a whole message.
    const fn overhead(&self) -> usize {
        match self {
            Self::Seq(_) => 0,
            Self::Stream(_) | Self::SeqFramed(_) => stream::PREFIX_LEN,
        }
    }

    /// Writes what waits for the socket.
    fn flush(&mut self) -> io::Result<bool> {
        match self {
            Self::Seq(link) => link.flush(),
            Self::Stream(link) => link.flush(),
            Self::SeqFramed(link) => link.flush(),
        }
    }
}

impl Carrier for Link<'_> {
    type Peer = ();
    type Error = io::Error;

    fn ready(&self, peer: (), len: usize) -> bool {
        match self {
            Self::Seq(link) => link.ready(peer, len),
            Self::Stream(link) => link.ready(peer, len),
            Self::SeqFramed(link) => link.ready(peer, len),
        }
    }

    fn send(
        &mut self,
        peer: (),
        priority: Priority,
        header: &[u8],
        payload: &[u8],
    ) -> io::Result<()> {
        match self {
            Self::Seq(link) => link.send(peer, priority, header, payload),
            Self::Stream(link) => link.send(peer, priority, header, payload),
            Self::SeqFramed(link) => link.send(peer, priority, header, payload),
        }
    }

    fn recv(&mut self, buf: &mut [u8]) -> io::Result<Option<((), usize)>> {
        match self {
            Self::Seq(link) => link.recv(buf),
            Self::Stream(link) => link.recv(buf),
            Self::SeqFramed(link) => link.recv(buf),
        }
    }
}

/// What crossed a link in each direction.
#[derive(Clone, Copy, Debug, Default)]
struct Wire {
    /// Packets or whole messages sent.
    packets_out: u64,
    /// Their bytes.
    bytes_out: u64,
    /// Their bytes with the framing of the link.
    link_bytes_out: u64,
    /// Packets or whole messages received.
    packets_in: u64,
    /// Their bytes.
    bytes_in: u64,
    /// Their bytes with the framing of the link.
    link_bytes_in: u64,
}

impl Wire {
    /// Returns the counts as an object.
    fn json(&self) -> JsObject {
        let mut object = JsObject::with_capacity(6);
        object.insert("packets_out", self.packets_out);
        object.insert("bytes_out", self.bytes_out);
        object.insert("link_bytes_out", self.link_bytes_out);
        object.insert("packets_in", self.packets_in);
        object.insert("bytes_in", self.bytes_in);
        object.insert("link_bytes_in", self.link_bytes_in);
        object
    }
}

/// A link that counts what crosses it.
struct Counted<'b> {
    /// The link.
    link: Link<'b>,
    /// What crossed it.
    wire: Wire,
}

impl Carrier for Counted<'_> {
    type Peer = ();
    type Error = io::Error;

    fn ready(&self, peer: (), len: usize) -> bool {
        self.link.ready(peer, len)
    }

    fn send(
        &mut self,
        peer: (),
        priority: Priority,
        header: &[u8],
        payload: &[u8],
    ) -> io::Result<()> {
        let len = count(header.len() + payload.len());
        self.wire.packets_out += 1;
        self.wire.bytes_out += len;
        self.wire.link_bytes_out += len + count(self.link.overhead());
        self.link.send(peer, priority, header, payload)
    }

    fn recv(&mut self, buf: &mut [u8]) -> io::Result<Option<((), usize)>> {
        let got = self.link.recv(buf)?;
        if let Some(((), len)) = got {
            self.wire.packets_in += 1;
            self.wire.bytes_in += count(len);
            self.wire.link_bytes_in += count(len + self.link.overhead());
        }
        Ok(got)
    }
}

/// The buffers of the framing of one stream: one frame received and the
/// frames queued for the stream.
struct FrameBuffers {
    /// The receive buffer.
    rx: Vec<u8>,
    /// The transmit buffer.
    tx: Vec<u8>,
}

impl FrameBuffers {
    /// Returns the buffers of frames of at most `largest` bytes each.
    fn new(largest: usize) -> Self {
        Self {
            rx: vec![0; Decoder::buffer_len(Mode::Prefixed, largest)],
            tx: vec![0; stream::frame_len(Mode::Prefixed, largest)],
        }
    }
}

/// Returns the link of a sequenced packet socket, framed for the baseline
/// where `baseline` holds.
fn seq_link(
    link: SeqPacket,
    max_packet: u16,
    buffers: &mut FrameBuffers,
    baseline: bool,
) -> Link<'_> {
    if baseline {
        Link::SeqFramed(Framed::new(
            SeqBytes::new(link, max_packet),
            Mode::Prefixed,
            &mut buffers.rx,
            &mut buffers.tx,
        ))
    } else {
        Link::Seq(link)
    }
}

/// Returns the link of a stream socket in prefixed `AoFrame`.
fn stream_link(link: UnixStream, buffers: &mut FrameBuffers) -> Link<'_> {
    Link::Stream(Framed::new(
        link,
        Mode::Prefixed,
        &mut buffers.rx,
        &mut buffers.tx,
    ))
}

/// Returns the nanoseconds since `start`.
fn since(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

/// Sleeps until `due` or for `idle_us` at most, whichever comes first.
fn idle(start: Instant, due: Option<u64>, idle_us: u64) {
    let now = since(start);
    let cap = idle_us.saturating_mul(1_000);
    let wait = due.map_or(cap, |due| due.saturating_sub(now).min(cap));
    if wait > 0 {
        thread::sleep(Duration::from_nanos(wait));
    }
}

/// Returns the limits of an end from the object under `key`, each absent
/// field taking the value of `defaults`.
fn limits_of(fields: &Fields<'_>, key: &str, defaults: Limits) -> Result<Limits, JsString> {
    let Some(_) = fields.opt_object(key)? else {
        return Ok(defaults);
    };
    let spec = fields.nested(key)?;
    Ok(Limits {
        max_packet: spec.int_or("max_packet", defaults.max_packet)?,
        max_message: spec.int_or("max_message", defaults.max_message)?,
        unsched_bytes: spec.int_or("unsched_bytes", defaults.unsched_bytes)?,
        max_rpcs: spec.int_or("max_rpcs", defaults.max_rpcs)?,
    })
}

/// Returns the limits an end announces on a link of `profile` unless the
/// request names its own: messages of 64 KiB and sixteen RPCs.
fn default_limits(profile: &Profile) -> Limits {
    Limits {
        max_packet: profile.max_packet,
        max_message: 65_536,
        unsched_bytes: profile.unsched_bytes,
        max_rpcs: 16,
    }
}

/// Reads the configuration of a request under `config`, each absent field
/// taking the value of `defaults`.
fn request_config(fields: &Fields<'_>, defaults: Config) -> Result<Config, JsString> {
    let spec = match fields.opt_object("config")? {
        Some(_) => Some(fields.nested("config")?),
        None => None,
    };
    config_of(spec.as_ref(), defaults)
}

/// Builds an engine of `config` and `profile` with its link up.
fn engine(
    config: Config,
    profile: Profile,
    local: Limits,
    assumed: Limits,
) -> Result<Engine, String> {
    let mut built = Engine::new(config, profile, Owned::new(&config)).map_err(config_error_name)?;
    built
        .link_up((), local, assumed)
        .map_err(|error| format!("link:{}", link_error_name(error)))?;
    Ok(built)
}

/// How a server answers a request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Responder {
    /// The request's own bytes.
    Echo,
    /// The request's bytes in reverse order.
    Reverse,
    /// The given number of bytes, a pattern of the byte values in turn.
    Sized(usize),
}

impl Responder {
    /// Reads the responder a request names under `responder`, `echo` where it
    /// names none.
    fn read(fields: &Fields<'_>) -> Result<Self, JsString> {
        let text = fields.opt_str("responder")?.unwrap_or("echo");
        match text.split_once(' ') {
            None if text == "echo" => Ok(Self::Echo),
            None if text == "reverse" => Ok(Self::Reverse),
            Some(("sized", n)) => n
                .parse()
                .map(Self::Sized)
                .map_err(|_| fields.refuse("responder", "names no length after sized")),
            _ => Err(fields.refuse("responder", &format!("names no responder: {text}"))),
        }
    }

    /// Returns the answer to `request`.
    fn answer(self, request: &[u8]) -> Vec<u8> {
        match self {
            Self::Echo => request.to_vec(),
            Self::Reverse => request.iter().rev().copied().collect(),
            Self::Sized(len) => (0..len).map(|n| n.to_le_bytes()[0]).collect(),
        }
    }
}

/// Answers the operation `ping`.
fn op_ping(request: &JsObject) -> Result<JsObject, JsString> {
    let fields = Fields::new(request, "ping");
    let form = form_of(&fields)?;
    let message = fields.hex("hex")?;
    let profile = Profile::unix_socket(65_536);
    let limits = default_limits(&profile);
    let (a, b) = match form {
        Form::SeqPacket => {
            let (a, b) = unix::pair(profile.max_packet)
                .map_err(|error| io_refused(&fields, "pair", &error))?;
            (Pair::Seq(a), Pair::Seq(b))
        }
        Form::Stream => {
            let (a, b) = UnixStream::pair().map_err(|error| io_refused(&fields, "pair", &error))?;
            for end in [&a, &b] {
                end.set_nonblocking(true)
                    .map_err(|error| io_refused(&fields, "nonblocking", &error))?;
            }
            (Pair::Stream(a), Pair::Stream(b))
        }
    };
    let largest = usize::from(profile.max_packet);
    let mut buffers = [FrameBuffers::new(largest), FrameBuffers::new(largest)];
    let [ba, bb] = &mut buffers;
    let mut links = [
        a.link(ba, profile.max_packet),
        b.link(bb, profile.max_packet),
    ];
    let mut client = engine(CLIENT_CONFIG, profile, limits, limits).map_err(JsString::from)?;
    let mut server = engine(SERVE_CONFIG, profile, limits, limits).map_err(JsString::from)?;
    let id = client
        .request((), &message, RequestOptions::default())
        .map_err(|error| fields.refuse("hex", send_error_name(&error)))?;
    let start = Instant::now();
    let mut rx = vec![0u8; RX_LEN];
    let [link_a, link_b] = &mut links;
    loop {
        let now = since(start);
        client
            .pump(link_a, now, &mut rx)
            .map_err(|error| io_refused(&fields, "client", &error))?;
        server
            .pump(link_b, now, &mut rx)
            .map_err(|error| io_refused(&fields, "server", &error))?;
        while let Some(event) = server.poll_event() {
            if let Event::Request { id, .. } = event {
                let bytes = read_all(server.request_of(id)).unwrap_or_default();
                server
                    .reply(id, &bytes)
                    .map_err(|error| fields.refuse("hex", send_error_name(&error)))?;
            }
        }
        server
            .pump(link_b, now, &mut rx)
            .map_err(|error| io_refused(&fields, "server", &error))?;
        while let Some(event) = client.poll_event() {
            match event {
                Event::Response { id: done } if done == id => {
                    let bytes = read_all(client.response(id)).unwrap_or_default();
                    let mut payload = JsObject::with_capacity(1);
                    payload.insert("hex", hex(&bytes));
                    return Ok(payload);
                }
                Event::Failed { why, .. } => {
                    return Err(JsString::from(format!(
                        "ping: failed {}",
                        failure_name(why)
                    )));
                }
                _ => {}
            }
        }
        if start.elapsed() > Duration::from_secs(10) {
            return Err(JsString::from("ping: no answer within 10 s"));
        }
    }
}

/// One end of a pair of sockets in this process.
enum Pair {
    /// A sequenced packet socket.
    Seq(SeqPacket),
    /// A stream socket.
    Stream(UnixStream),
}

impl Pair {
    /// Returns the link of the end over `buffers`.
    fn link(self, buffers: &mut FrameBuffers, max_packet: u16) -> Link<'_> {
        match self {
            Self::Seq(link) => seq_link(link, max_packet, buffers, false),
            Self::Stream(link) => stream_link(link, buffers),
        }
    }
}

/// How a server answers the RPCs of a connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reserve {
    /// Every answer through `reply`.
    None,
    /// Every answer through `reply_reserve`.
    All,
    /// The answers of the requests of odd index through `reply_reserve`.
    Half,
}

/// What `serve` reads from its request.
struct ServeSpec {
    /// The form of the socket.
    form: Form,
    /// The path the listener binds.
    path: PathBuf,
    /// The profile of every endpoint.
    profile: Profile,
    /// The configuration of every endpoint.
    config: Config,
    /// What the server announces.
    local: Limits,
    /// What it takes of a client until its LIMITS arrive.
    assumed: Limits,
    /// How it answers.
    responder: Responder,
    /// Which answers go through `reply_reserve`.
    reserve: Reserve,
    /// Whether the connections run Homa, or `AoFrame` alone.
    homa: bool,
    /// The connections it accepts.
    connections: usize,
    /// The request of a connection before which its engine is rebuilt.
    rebuild_after: Option<u64>,
    /// The first identifier of a rebuilt engine.
    rebuild_first_id: u64,
    /// The send buffer of every accepted socket.
    send_buffer: Option<usize>,
    /// The longest the operation runs.
    max_time: Duration,
    /// The longest a pass with nothing to do sleeps.
    idle_us: u64,
}

impl ServeSpec {
    /// Reads the fields of `serve`.
    fn read(fields: &Fields<'_>) -> Result<Self, JsString> {
        let profile = request_profile(fields)?;
        let local = limits_of(fields, "limits", default_limits(&profile))?;
        let reserve = match fields.opt_str("reserve")?.unwrap_or("half") {
            "none" => Reserve::None,
            "all" => Reserve::All,
            "half" => Reserve::Half,
            other => return Err(fields.refuse("reserve", &format!("names no choice: {other}"))),
        };
        Ok(Self {
            form: form_of(fields)?,
            path: PathBuf::from(fields.str("path")?),
            profile,
            config: request_config(fields, SERVE_CONFIG)?,
            local,
            assumed: limits_of(fields, "assumed", local)?,
            responder: Responder::read(fields)?,
            reserve,
            homa: fields.bool_or("homa", true)?,
            connections: fields.int_or("connections", 1)?,
            rebuild_after: fields.opt_u64("rebuild_after")?,
            rebuild_first_id: fields.int_or("rebuild_first_id", 0x2100_0002)?,
            send_buffer: fields
                .opt_u64("send_buffer")?
                .map(|bytes| usize::try_from(bytes).unwrap_or(usize::MAX)),
            max_time: Duration::from_millis(fields.int_or("max_time_ms", 60_000)?),
            idle_us: fields.int_or("idle_us", 200)?,
        })
    }
}

/// The listener of `serve`.
enum Listener {
    /// A listener of sequenced packet sockets.
    Seq(SeqListener),
    /// A listener of stream sockets.
    Stream(StreamListener),
}

/// A socket `serve` accepted, before its link is built.
enum Accepted {
    /// A sequenced packet socket.
    Seq(SeqPacket),
    /// A stream socket.
    Stream(UnixStream),
}

impl Listener {
    /// Binds the listener of `form` at `path`.
    fn bind(form: Form, path: &Path) -> io::Result<Self> {
        match form {
            Form::SeqPacket => SeqListener::bind(path).map(Self::Seq),
            Form::Stream => StreamListener::bind(path).map(Self::Stream),
        }
    }

    /// Accepts one socket whose records carry at most `max_packet` bytes, or
    /// `None` where none waits, with its send buffer set.
    fn accept(&self, max_packet: u16, send_buffer: Option<usize>) -> io::Result<Option<Accepted>> {
        let accepted = match self {
            Self::Seq(listener) => listener.accept(max_packet)?.map(Accepted::Seq),
            Self::Stream(listener) => listener.accept()?.map(Accepted::Stream),
        };
        if let (Some(socket), Some(bytes)) = (&accepted, send_buffer) {
            match socket {
                Accepted::Seq(link) => unix::set_send_buffer(link, bytes)?,
                Accepted::Stream(link) => unix::set_send_buffer(link, bytes)?,
            }
        }
        Ok(accepted)
    }
}

/// The identifiers one engine delivered, sorted, and the ones it delivered
/// again.
#[derive(Debug, Default)]
struct Delivered {
    /// Every identifier delivered, sorted.
    ids: Vec<u64>,
    /// Identifiers delivered more than once.
    again: u64,
}

impl Delivered {
    /// Notes the delivery of `id`.
    fn note(&mut self, id: u64) {
        let at = self.ids.partition_point(|&known| known < id);
        if self.ids.get(at) == Some(&id) {
            self.again += 1;
        } else {
            self.ids.insert(at, id);
        }
    }
}

/// What one connection of `serve` counted.
#[derive(Debug, Default)]
struct ServeCounts {
    /// Requests delivered, by the generation of the engine.
    requests: Vec<u64>,
    /// Answers queued, by the generation of the engine.
    answered: Vec<u64>,
    /// Answers queued through `reply_reserve`.
    reserved: u64,
    /// Answers the engine refused, by the name of the refusal.
    reply_errors: Vec<&'static str>,
    /// Server RPCs dropped without an answer.
    dropped: u64,
    /// The reasons of every `PeerGone`.
    gone: Vec<&'static str>,
    /// LIMITS the engine reported.
    limits: u64,
    /// The time the first `PeerGone` came, in milliseconds.
    gone_ms: Option<u64>,
}

/// One connection of `serve`.
struct ServeConn<'b> {
    /// Its link.
    link: Counted<'b>,
    /// Its engine, `None` for a baseline connection.
    engine: Option<Engine>,
    /// The generation of the engine, 0 before a rebuild.
    generation: usize,
    /// The deliveries of each generation.
    delivered: Vec<Delivered>,
    /// The whole messages of a baseline connection waiting to be written.
    queue: std::collections::VecDeque<Vec<u8>>,
    /// What it counted.
    counts: ServeCounts,
    /// The error that ended its link, if one did.
    error: Option<String>,
    /// Whether it has ended.
    finished: bool,
    /// Requests of the first generation, which decide the rebuild.
    first_requests: u64,
}

impl<'b> ServeConn<'b> {
    /// Returns a connection over `link`, with an engine where `spec` runs
    /// Homa.
    fn new(link: Link<'b>, spec: &ServeSpec) -> Result<Self, String> {
        let engine = if spec.homa {
            Some(engine(spec.config, spec.profile, spec.local, spec.assumed)?)
        } else {
            None
        };
        Ok(Self {
            link: Counted {
                link,
                wire: Wire::default(),
            },
            engine,
            generation: 0,
            delivered: vec![Delivered::default()],
            queue: std::collections::VecDeque::new(),
            counts: ServeCounts {
                requests: vec![0],
                answered: vec![0],
                ..ServeCounts::default()
            },
            error: None,
            finished: false,
            first_requests: 0,
        })
    }

    /// Runs one pass at `now` and returns whether anything moved.
    fn turn(&mut self, now: u64, spec: &ServeSpec, rx: &mut [u8], start: Instant) -> bool {
        if self.finished {
            return false;
        }
        let before = self.link.wire;
        let moved = if self.engine.is_some() {
            self.turn_homa(now, spec, rx, start)
        } else {
            self.turn_baseline(spec, rx)
        };
        if self.link.link.closed() || self.error.is_some() {
            self.end(start);
        }
        moved
            || before.packets_in != self.link.wire.packets_in
            || before.packets_out != self.link.wire.packets_out
    }

    /// Runs one pass of a Homa connection.
    fn turn_homa(&mut self, now: u64, spec: &ServeSpec, rx: &mut [u8], start: Instant) -> bool {
        let Some(engine) = self.engine.as_mut() else {
            return false;
        };
        if let Err(error) = engine.pump(&mut self.link, now, rx) {
            self.error = Some(error.to_string());
            return true;
        }
        let mut moved = false;
        let mut rebuild = false;
        while let Some(event) = engine.poll_event() {
            moved = true;
            match event {
                Event::Request { id, .. } => {
                    let generation = self.generation;
                    self.counts.requests[generation] += 1;
                    self.delivered[generation].note(id.get());
                    if generation == 0 {
                        self.first_requests += 1;
                    }
                    if generation == 0 && spec.rebuild_after == Some(self.first_requests) {
                        rebuild = true;
                        break;
                    }
                    answer(engine, id, spec, &mut self.counts, generation);
                }
                Event::Dropped { .. } => self.counts.dropped += 1,
                Event::PeerGone { why, .. } => {
                    self.counts.gone.push(gone_name(why));
                    self.counts.gone_ms.get_or_insert(since(start) / 1_000_000);
                }
                Event::Limits { .. } => self.counts.limits += 1,
                _ => {}
            }
        }
        if rebuild {
            let mut config = spec.config;
            config.first_id = spec.rebuild_first_id;
            match self::engine(config, spec.profile, spec.local, spec.assumed) {
                Ok(fresh) => {
                    self.engine = Some(fresh);
                    self.generation += 1;
                    self.delivered.push(Delivered::default());
                    self.counts.requests.push(0);
                    self.counts.answered.push(0);
                }
                Err(error) => self.error = Some(error),
            }
            return true;
        }
        if let Some(engine) = self.engine.as_mut()
            && let Err(error) = engine.pump(&mut self.link, now, rx)
        {
            self.error = Some(error.to_string());
        }
        moved
    }

    /// Runs one pass of a baseline connection: every whole message read is
    /// answered whole, first in first out.
    fn turn_baseline(&mut self, spec: &ServeSpec, rx: &mut [u8]) -> bool {
        let mut moved = false;
        loop {
            match self.link.recv(rx) {
                Ok(Some(((), len))) => {
                    moved = true;
                    self.counts.requests[0] += 1;
                    self.queue.push_back(spec.responder.answer(&rx[..len]));
                }
                Ok(None) => break,
                Err(error) => {
                    self.error = Some(error.to_string());
                    return true;
                }
            }
        }
        while let Some(front) = self.queue.front() {
            if !self.link.ready((), front.len()) {
                break;
            }
            let answer = self.queue.pop_front().unwrap_or_default();
            if let Err(error) = self.link.send((), Priority::HIGHEST, &[], &answer) {
                self.error = Some(error.to_string());
                return true;
            }
            self.counts.answered[0] += 1;
            moved = true;
        }
        if let Err(error) = self.link.link.flush() {
            self.error = Some(error.to_string());
        }
        moved
    }

    /// Ends the connection, taking its link down in its engine.
    fn end(&mut self, start: Instant) {
        self.finished = true;
        let Some(engine) = self.engine.as_mut() else {
            return;
        };
        // A peer the engine declared dead has its link up still, and a peer
        // whose link is down already answers NoPeer, which needs nothing.
        let _ = engine.link_down(());
        while let Some(event) = engine.poll_event() {
            match event {
                Event::Dropped { .. } => self.counts.dropped += 1,
                Event::PeerGone { why, .. } => {
                    self.counts.gone.push(gone_name(why));
                    self.counts.gone_ms.get_or_insert(since(start) / 1_000_000);
                }
                _ => {}
            }
        }
    }

    /// Returns what the connection counted as an object.
    fn json(&self) -> JsObject {
        let numbers =
            |values: &[u64]| -> Vec<JsValue> { values.iter().map(|&n| n.into()).collect() };
        let mut object = JsObject::new();
        object.insert("requests", numbers(&self.counts.requests));
        object.insert(
            "duplicates",
            self.delivered.iter().map(|d| d.again).sum::<u64>(),
        );
        object.insert("answered", numbers(&self.counts.answered));
        object.insert("reserved", self.counts.reserved);
        object.insert(
            "reply_errors",
            self.counts
                .reply_errors
                .iter()
                .map(|&name| JsValue::from(name))
                .collect::<Vec<_>>(),
        );
        object.insert("dropped", self.counts.dropped);
        object.insert(
            "gone",
            self.counts
                .gone
                .iter()
                .map(|&name| JsValue::from(name))
                .collect::<Vec<_>>(),
        );
        object.insert(
            "gone_ms",
            self.counts.gone_ms.map_or(JsValue::Null, JsValue::from),
        );
        object.insert("limits", self.counts.limits);
        object.insert("rebuilds", count(self.generation));
        let listed: Vec<JsValue> = self
            .delivered
            .iter()
            .map(|d| {
                if d.ids.len() <= 64 {
                    numbers(&d.ids).into()
                } else {
                    JsValue::Null
                }
            })
            .collect();
        object.insert("ids", listed);
        object.insert("truncated", self.link.link.truncated());
        object.insert("link_dropped", self.link.link.dropped());
        object.insert("wire", self.link.wire.json());
        object.insert(
            "error",
            self.error.clone().map_or(JsValue::Null, JsValue::from),
        );
        if let Some(engine) = &self.engine {
            object.insert("stats", stats_json(&engine.stats()));
        }
        object
    }
}

/// Returns the name of why a peer is gone.
const fn gone_name(why: Gone) -> &'static str {
    match why {
        Gone::LinkDown => "LinkDown",
        Gone::Timeout => "Timeout",
    }
}

/// Answers the server RPC `id` of `engine` through the responder, by
/// `reply_reserve` or `reply` as `spec` says, and counts it.
fn answer(
    engine: &mut Engine,
    id: RpcId,
    spec: &ServeSpec,
    counts: &mut ServeCounts,
    generation: usize,
) {
    let request = read_all(engine.request_of(id)).unwrap_or_default();
    let bytes = spec.responder.answer(&request);
    let reserve = match spec.reserve {
        Reserve::None => false,
        Reserve::All => true,
        Reserve::Half => counts.answered.iter().sum::<u64>() % 2 == 1,
    };
    let result = if reserve {
        reserved(engine, id, &bytes).inspect(|()| counts.reserved += 1)
    } else {
        engine
            .reply(id, &bytes)
            .map_err(|error| send_error_name(&error))
    };
    match result {
        Ok(()) => counts.answered[generation] += 1,
        Err(name) => counts.reply_errors.push(name),
    }
}

/// Answers through a reservation of the answer's length, written in chunks
/// and committed.
fn reserved(engine: &mut Engine, id: RpcId, bytes: &[u8]) -> Result<(), &'static str> {
    let len = u32::try_from(bytes.len()).map_err(|_| "TooLong")?;
    let mut writer = engine
        .reply_reserve(id, len)
        .map_err(|error| send_error_name(&error))?;
    for chunk in bytes.chunks(1_000) {
        writer.write(chunk).map_err(|_| "NoRoom")?;
    }
    writer.commit().map_err(|_| "Gone")
}

/// Answers the operation `serve`.
fn op_serve(request: &JsObject) -> Result<JsObject, JsString> {
    let fields = Fields::new(request, "serve");
    let spec = ServeSpec::read(&fields)?;
    let listener = Listener::bind(spec.form, &spec.path)
        .map_err(|error| io_refused(&fields, "bind", &error))?;
    let largest = if spec.homa {
        usize::from(spec.profile.max_packet)
    } else {
        usize::try_from(spec.local.max_message).unwrap_or(usize::MAX)
    };
    let mut buffers: Vec<FrameBuffers> = (0..spec.connections)
        .map(|_| FrameBuffers::new(largest))
        .collect();
    let mut free = buffers.iter_mut();
    let mut conns: Vec<ServeConn<'_>> = Vec::new();
    let mut rx = vec![0u8; RX_LEN];
    let start = Instant::now();
    let stopped = loop {
        let now = since(start);
        let mut moved = false;
        if conns.len() < spec.connections {
            match listener.accept(spec.profile.max_packet, spec.send_buffer) {
                Ok(Some(socket)) => {
                    let Some(pair) = free.next() else {
                        break "buffers";
                    };
                    let link = match socket {
                        Accepted::Seq(link) => {
                            seq_link(link, spec.profile.max_packet, pair, !spec.homa)
                        }
                        Accepted::Stream(link) => stream_link(link, pair),
                    };
                    let conn = ServeConn::new(link, &spec).map_err(JsString::from)?;
                    conns.push(conn);
                    moved = true;
                }
                Ok(None) => {}
                Err(error) => return Err(io_refused(&fields, "accept", &error)),
            }
        }
        for conn in &mut conns {
            moved |= conn.turn(now, &spec, &mut rx, start);
        }
        if conns.len() == spec.connections && conns.iter().all(|conn| conn.finished) {
            break "done";
        }
        if start.elapsed() > spec.max_time {
            break "max_time";
        }
        if !moved {
            let due = conns
                .iter()
                .filter_map(|conn| conn.engine.as_ref().and_then(Engine::next_timeout))
                .min();
            idle(start, due, spec.idle_us);
        }
    };
    drop(listener);
    // The listener's file is the operation's own, and a file gone already
    // needs nothing.
    let _ = std::fs::remove_file(&spec.path);
    let mut payload = JsObject::new();
    payload.insert("stopped", stopped);
    payload.insert("accepted", count(conns.len()));
    payload.insert("time_ms", since(start) / 1_000_000);
    payload.insert(
        "connections",
        conns
            .iter()
            .map(|conn| JsValue::Object(conn.json()))
            .collect::<Vec<_>>(),
    );
    Ok(payload)
}

/// One request of `client`.
struct ClientRpc {
    /// The bytes of the request.
    bytes: Vec<u8>,
    /// Whether the request carried `hex`, so its answer carries it too.
    echo_hex: bool,
    /// How the request is sent.
    options: RequestOptions,
}

/// Where one request of `client` ended.
#[derive(Debug, Default)]
struct ClientRecord {
    /// The identifier the engine named it by.
    id: Option<u64>,
    /// `pending`, `ok`, `failed` or `refused`.
    outcome: &'static str,
    /// The name of the failure or the refusal.
    why: Option<&'static str>,
    /// The response read.
    response: Option<Vec<u8>>,
    /// When it ended, in milliseconds.
    ended_ms: Option<u64>,
}

/// Reads the requests of `client`.
fn client_rpcs(fields: &Fields<'_>) -> Result<Vec<ClientRpc>, JsString> {
    let mut rpcs = Vec::new();
    for (index, value) in fields.array("requests")?.iter().enumerate() {
        let JsValue::Object(object) = value else {
            return Err(fields.refuse("requests", "holds an entry that is no object"));
        };
        let entry = Fields::new(object, "client");
        if entry.int_or("peer", 0u64)? != 0 {
            return Err(entry.refuse("peer", "names a peer other than the one connection"));
        }
        let (bytes, echo_hex) = match entry.opt_str("hex")? {
            Some(text) => (
                unhex(text).ok_or_else(|| entry.refuse("hex", "is no even run of hex digits"))?,
                true,
            ),
            None => (
                Fill::read(&entry, "fill", count(index).saturating_mul(2))?
                    .bytes(entry.int_or("len", 0)?),
                false,
            ),
        };
        rpcs.push(ClientRpc {
            bytes,
            echo_hex,
            options: RequestOptions {
                patient: entry.bool_or("patient", false)?,
                report_unknown: entry.bool_or("report_unknown", false)?,
                source_port: 0,
            },
        });
    }
    Ok(rpcs)
}

/// Connects to the listener of `form` at `path`, trying until `wait` has
/// passed, since the server may still be binding.
fn connect(form: Form, path: &Path, max_packet: u16, wait: Duration) -> io::Result<Accepted> {
    let start = Instant::now();
    loop {
        let tried = match form {
            Form::SeqPacket => SeqPacket::connect(path, max_packet).map(Accepted::Seq),
            Form::Stream => UnixStream::connect(path).and_then(|stream| {
                stream.set_nonblocking(true)?;
                Ok(Accepted::Stream(stream))
            }),
        };
        match tried {
            Ok(socket) => return Ok(socket),
            Err(error) if start.elapsed() < wait => {
                let _ = error;
                thread::sleep(Duration::from_millis(5));
            }
            Err(error) => return Err(error),
        }
    }
}

/// The requests of `client` in flight and the records of their ends.
struct ClientState {
    /// One record a request.
    records: Vec<ClientRecord>,
    /// The identifier of every request sent with its index, sorted.
    ids: Vec<(u64, usize)>,
    /// The next request to send.
    next: usize,
    /// Requests sent and not ended.
    open: usize,
    /// Requests not ended.
    left: usize,
}

impl ClientState {
    /// Returns the state of `count` requests, none sent.
    fn new(count: usize) -> Self {
        Self {
            records: (0..count)
                .map(|_| ClientRecord {
                    outcome: "pending",
                    ..ClientRecord::default()
                })
                .collect(),
            ids: Vec::new(),
            next: 0,
            open: 0,
            left: count,
        }
    }

    /// Sends the requests the window allows at `now` and returns whether
    /// one left or was refused.
    fn issue(&mut self, client: &mut Engine, rpcs: &[ClientRpc], window: usize, now: u64) -> bool {
        let mut moved = false;
        while self.next < rpcs.len() && self.open < window {
            let rpc = &rpcs[self.next];
            match client.request((), &rpc.bytes, rpc.options) {
                Ok(id) => {
                    self.records[self.next].id = Some(id.get());
                    let at = self.ids.partition_point(|&(known, _)| known < id.get());
                    self.ids.insert(at, (id.get(), self.next));
                    self.open += 1;
                }
                Err(SendError::NoSlot | SendError::Full) => break,
                Err(refusal) => {
                    let record = &mut self.records[self.next];
                    record.outcome = "refused";
                    record.why = Some(send_error_name(&refusal));
                    record.ended_ms = Some(now / 1_000_000);
                    self.left -= 1;
                }
            }
            self.next += 1;
            moved = true;
        }
        moved
    }

    /// Returns the index of the request the client RPC `id` sent.
    fn index_of(&self, id: RpcId) -> Option<usize> {
        let at = self.ids.partition_point(|&(known, _)| known < id.get());
        self.ids
            .get(at)
            .filter(|&&(known, _)| known == id.get())
            .map(|&(_, index)| index)
    }

    /// Records the end of the request at `index` with `outcome` at `now`.
    fn end(&mut self, index: usize, outcome: &'static str, now: u64) {
        let record = &mut self.records[index];
        record.outcome = outcome;
        record.ended_ms = Some(now / 1_000_000);
        self.open -= 1;
        self.left -= 1;
    }

    /// Takes every event of `client` at `now` and returns whether one came.
    fn take_events(&mut self, client: &mut Engine, now: u64) -> bool {
        let mut moved = false;
        while let Some(event) = client.poll_event() {
            moved = true;
            match event {
                Event::Response { id } => {
                    let bytes = read_all(client.response(id)).unwrap_or_default();
                    // The response is read, and the release queues its
                    // acknowledgement.
                    let _ = client.release(id);
                    if let Some(index) = self.index_of(id) {
                        self.records[index].response = Some(bytes);
                        self.end(index, "ok", now);
                    }
                }
                Event::Failed { id, why } => {
                    if let Some(index) = self.index_of(id) {
                        self.records[index].why = Some(failure_name(why));
                        self.end(index, "failed", now);
                    }
                }
                _ => {}
            }
        }
        moved
    }
}

/// Returns what `client` answers of the request at `index`.
fn record_json(index: usize, record: &ClientRecord, rpc: &ClientRpc) -> JsValue {
    let mut object = JsObject::new();
    object.insert("index", count(index));
    object.insert("id", record.id.map_or(JsValue::Null, JsValue::from));
    object.insert("outcome", record.outcome);
    object.insert("why", record.why.map_or(JsValue::Null, JsValue::from));
    object.insert(
        "ended_ms",
        record.ended_ms.map_or(JsValue::Null, JsValue::from),
    );
    if let Some(bytes) = &record.response {
        object.insert("len", count(bytes.len()));
        object.insert("sha256", sha256_hex(bytes));
        if rpc.echo_hex {
            object.insert("hex", hex(bytes));
        }
    } else {
        object.insert("len", JsValue::Null);
        object.insert("sha256", JsValue::Null);
    }
    JsValue::Object(object)
}

/// Answers the operation `client`.
fn op_client(request: &JsObject) -> Result<JsObject, JsString> {
    let fields = Fields::new(request, "client");
    let form = form_of(&fields)?;
    let path = PathBuf::from(fields.str("path")?);
    let profile = request_profile(&fields)?;
    let config = request_config(&fields, CLIENT_CONFIG)?;
    let local = limits_of(&fields, "limits", default_limits(&profile))?;
    let assumed = limits_of(&fields, "assumed", local)?;
    let window: usize = fields.int_or("window", 4)?;
    let max_time = Duration::from_millis(fields.int_or("max_time_ms", 60_000)?);
    let idle_us: u64 = fields.int_or("idle_us", 200)?;
    let wait = Duration::from_millis(fields.int_or("connect_ms", 10_000)?);
    let rpcs = client_rpcs(&fields)?;
    let socket = connect(form, &path, profile.max_packet, wait)
        .map_err(|error| io_refused(&fields, "connect", &error))?;
    let mut buffers = FrameBuffers::new(usize::from(profile.max_packet));
    let link = match socket {
        Accepted::Seq(link) => seq_link(link, profile.max_packet, &mut buffers, false),
        Accepted::Stream(link) => stream_link(link, &mut buffers),
    };
    let mut link = Counted {
        link,
        wire: Wire::default(),
    };
    let mut client = engine(config, profile, local, assumed).map_err(JsString::from)?;
    let mut state = ClientState::new(rpcs.len());
    let mut rx = vec![0u8; RX_LEN];
    let start = Instant::now();
    let mut error = None;
    let stopped = loop {
        let now = since(start);
        let mut moved = state.issue(&mut client, &rpcs, window, now);
        if let Err(failed) = client.pump(&mut link, now, &mut rx) {
            error = Some(failed.to_string());
            let _ = client.link_down(());
        }
        moved |= state.take_events(&mut client, now);
        if let Err(failed) = client.pump(&mut link, now, &mut rx) {
            error.get_or_insert(failed.to_string());
        }
        if state.left == 0 {
            break "done";
        }
        if error.is_some() && state.open == 0 && state.next >= rpcs.len() {
            break "link";
        }
        if start.elapsed() > max_time {
            break "max_time";
        }
        if !moved {
            idle(start, client.next_timeout(), idle_us);
        }
    };
    // The acknowledgements of the last responses leave before the socket
    // closes, and a link that fails here has nothing more to carry.
    let _ = client.pump(&mut link, since(start), &mut rx);
    let _ = link.link.flush();
    let mut payload = JsObject::new();
    payload.insert("stopped", stopped);
    payload.insert("time_ms", since(start) / 1_000_000);
    let answers: Vec<JsValue> = state
        .records
        .iter()
        .zip(&rpcs)
        .enumerate()
        .map(|(index, (record, rpc))| record_json(index, record, rpc))
        .collect();
    payload.insert("rpcs", answers);
    payload.insert("stats", stats_json(&client.stats()));
    payload.insert("wire", link.wire.json());
    payload.insert("truncated", link.link.truncated());
    payload.insert("error", error.map_or(JsValue::Null, JsValue::from));
    Ok(payload)
}
