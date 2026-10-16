// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool yoga serve: the flag --graphtp of the primitive yoga, which
 * serves the Yoga instances of the in process capture through the GraphTP
 * listener of aosabi-core over real Unix Sockets.
 *
 * No analogue in the reference.
 */

//! The flag `--graphtp DIR` of the primitive `yoga`.
//!
//! Each instance of [`super::instances`] becomes a booted app: an app of
//! `aosabi-core` whose one middleware on `*` is the instance's Yoga, mounted
//! as `handler` of `aosabi-core` mounts its Yoga, with no base pipeline,
//! since the in process goldens were recorded on the bare instances with no
//! base pipeline. The listeners of an instance take
//! `aosabi_core::graphtp::served::fetch` of its app, the fetch a served build
//! hands its listeners, which calls `BootedApp::fetch` with the request of
//! the fixed fields and copies the substatus the port of Yoga decides into
//! the answer.
//!
//! For each instance `NAME` the flag binds a `SOCK_SEQPACKET` socket at
//! `DIR/NAME.homa` and a stream socket of prefixed `AoFrame` at
//! `DIR/NAME.stream`, the two forms of the card `test-graphtp`, the second of
//! which `node:net` opens for `graphtp+unix`. Each accepted socket gets an
//! endpoint of the server role and a listener with the bounds of
//! `test-graphtp`, over the profile of Unix Socket with messages of up to 1
//! MiB. The flag serves until the file `DIR/stop` stands and every connection
//! it accepted has closed, or for [`MAX_TIME`] at most, removes its sockets
//! and the file `stop`, and answers one line: its end under `stopped`, `done`
//! or `max_time`, the `accepted` connections, the stream sockets it `refused`
//! past the 64 of a run, per connection its `instance`, its `form`, the
//! answers it handed its engine and its `error`, and per instance the
//! `fetches` its app answered.

use std::{
    cell::Cell,
    fs,
    io,
    os::unix::net::UnixStream,
    path::{
        Path,
        PathBuf, //
    },
    rc::Rc,
    task::{
        Context,
        Waker, //
    },
    thread,
    time::{
        Duration,
        Instant, //
    }, //
};

use aosabi_core::{
    App,
    BootedApp,
    Next,
    Thrown,
    graphtp::{
        Fetch,
        Listener,
        Reply,
        Request,
        limits::{
            self,
            CardLimits, //
        },
        served, //
    }, //
};
use aosabi_json::{
    JsObject,
    JsValue, //
};
use graphql_over_http::{
    HttpServer,
    ServerContext,
    answer, //
};
use homa::{
    Carrier,
    Config,
    Endpoint,
    Limits,
    Owned,
    Priority,
    Profile,
    stream::{
        self,
        Decoder,
        Framed,
        Mode, //
    },
    unix::{
        SeqListener,
        SeqPacket,
        StreamListener, //
    },
    wire::Compact, //
};

use super::instances::{
    Instances,
    NAMES, //
};
use crate::{
    homa_sim::{
        config_error_name,
        scenario::link_error_name, //
    },
    seam::{
        Answer,
        Failure, //
    },
};

/// The card whose bounds every listener of the flag holds.
const CARD: &str = "test-graphtp";

/// The longest message either end takes, 1 MiB, the bound of the card
/// `test-graphtp` and of the package's link over `graphtp+unix`.
const MAX_MESSAGE: u32 = 1 << 20;

/// The longest the flag serves, ten minutes, the timeout of one call of a
/// notebook.
pub const MAX_TIME: Duration = Duration::from_mins(10);

/// The longest a turn with nothing to do sleeps.
const IDLE: Duration = Duration::from_micros(500);

/// The stream sockets the flag accepts in one run. Each takes a pair of
/// framing buffers from a pool of this size and keeps it until the flag ends,
/// so the stream socket past the pool is refused even when every earlier one
/// has closed. A finished connection of either form keeps the pages of its
/// endpoint as well, 256 of 8,192 bytes, until the flag ends.
const STREAM_BUFFERS: usize = 64;

/// The bytes of the receive buffer of the loop, a packet of the largest
/// profile and more.
const RX_LEN: usize = 1 << 20;

/// The file whose presence ends the flag once its connections have closed.
const STOP: &str = "stop";

/// The configuration of every endpoint: sixteen server slots and 256 pages of
/// 8,192 bytes, the endpoint of the card `test-graphtp` in the operation
/// `serve` of the primitive `graphtp`.
const CONFIG: Config = Config {
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

/// An endpoint of one connection: one peer, storage it owns, the compact
/// form.
type Engine = Endpoint<(), Owned<()>, Compact>;

/// Serves every instance over `GraphTP` in `dir` and answers the line of
/// what it served.
///
/// # Errors
///
/// Returns [`Failure::Data`] when an instance does not build or a socket
/// cannot be bound, accepted or run.
pub fn serve(dir: &Path) -> Result<Answer, Failure> {
    let limits = limits::card(CARD).ok_or_else(|| Failure::Data(format!("no card {CARD}")))?;
    let built = Instances::new().map_err(Failure::Data)?;
    let io_failure = |what: &str, error: &io::Error| Failure::Data(format!("{what}: {error}"));
    fs::create_dir_all(dir).map_err(|error| io_failure("the directory", &error))?;
    let stop = dir.join(STOP);
    remove(&stop);
    let mut served = Vec::with_capacity(NAMES.len());
    for name in NAMES {
        let yoga = built
            .get(name)
            .ok_or_else(|| Failure::Data(format!("no instance {name}")))?;
        match Served::bind(name, dir, yoga) {
            Ok(instance) => served.push(instance),
            Err(error) => {
                // A path past the bound of a socket's address refuses the
                // bind, and the sockets bound before it go with the call.
                unbind(&served);
                return Err(io_failure(name, &error));
            }
        }
    }
    let spec = Spec::new();
    let mut buffers: Vec<FrameBuffers> = (0..STREAM_BUFFERS)
        .map(|_| FrameBuffers::new(usize::from(spec.profile.max_packet)))
        .collect();
    let mut free = buffers.iter_mut();
    let mut conns: Vec<Conn<'_>> = Vec::new();
    let mut refused = 0u64;
    let mut rx = vec![0u8; RX_LEN];
    let mut cx = Context::from_waker(Waker::noop());
    let start = Instant::now();
    let stopped = 'serve: loop {
        let now = since(start);
        let mut moved = false;
        for (index, instance) in served.iter().enumerate() {
            let pass = Accept {
                index,
                instance,
                spec: &spec,
                limits,
            };
            match pass.take(&mut free, &mut conns, &mut refused) {
                Ok(taken) => moved |= taken,
                Err(failure) => break 'serve Err(failure),
            }
        }
        for conn in &mut conns {
            let was = conn.finished;
            moved |= conn.turn(now, &mut rx, &mut cx);
            if conn.finished && !was {
                conn.ended_ms = Some(since(start) / 1_000_000);
            }
        }
        if conns.iter().all(|conn| conn.finished) && stop.exists() {
            break Ok("done");
        }
        if start.elapsed() > MAX_TIME {
            break Ok("max_time");
        }
        if !moved {
            let due = conns
                .iter()
                .filter(|conn| !conn.finished)
                .filter_map(|conn| conn.engine.next_timeout())
                .min();
            let cap = u64::try_from(IDLE.as_nanos()).unwrap_or(u64::MAX);
            let wait = due.map_or(cap, |due| due.saturating_sub(since(start)).min(cap));
            thread::sleep(Duration::from_nanos(wait));
        }
    };
    // The sockets go with the call whether it served to its end or failed.
    unbind(&served);
    remove(&stop);
    let payload = report(stopped?, since(start), &served, &conns, refused, limits);
    let mut bytes = Vec::new();
    aosabi_json::write(&mut bytes, &JsValue::Object(payload));
    bytes.push(b'\n');
    Ok(Answer::Payload(bytes))
}

/// One pass of accepting over the two listening sockets of one instance.
struct Accept<'a> {
    /// The index of the instance.
    index: usize,
    /// The instance.
    instance: &'a Served,
    /// The Homa values of every endpoint.
    spec: &'a Spec,
    /// The bounds of every listener.
    limits: &'static CardLimits,
}

impl Accept<'_> {
    /// Accepts at most one socket of each form, pushes its connection onto
    /// `conns` and tells whether a socket was accepted. A stream socket past
    /// the framing buffers `free` holds is closed and counted in `refused`.
    fn take<'b>(
        &self,
        free: &mut impl Iterator<Item = &'b mut FrameBuffers>,
        conns: &mut Vec<Conn<'b>>,
        refused: &mut u64,
    ) -> Result<bool, Failure> {
        let failed = |what: &str, error: &io::Error| Failure::Data(format!("{what}: {error}"));
        let mut taken = false;
        let fetch = &self.instance.fetch;
        if let Some(link) = self
            .instance
            .seq
            .accept(self.spec.profile.max_packet)
            .map_err(|error| failed("accept", &error))?
        {
            let conn = Conn::new(self.index, Link::Seq(link), self.spec, fetch, self.limits)
                .map_err(Failure::Data)?;
            conns.push(conn);
            taken = true;
        }
        if let Some(link) = self
            .instance
            .stream
            .accept()
            .map_err(|error| failed("accept stream", &error))?
        {
            taken = true;
            let Some(pair) = free.next() else {
                *refused += 1;
                return Ok(taken);
            };
            let framed = Framed::new(link, Mode::Prefixed, &mut pair.rx, &mut pair.tx);
            let conn = Conn::new(
                self.index,
                Link::Stream(framed),
                self.spec,
                fetch,
                self.limits,
            )
            .map_err(Failure::Data)?;
            conns.push(conn);
        }
        Ok(taken)
    }
}

/// Removes the socket files of every instance in `served`.
fn unbind(served: &[Served]) {
    for instance in served {
        remove(&instance.seq_path);
        remove(&instance.stream_path);
    }
}

/// Removes the file at `path`, which the flag owns, where it stands.
fn remove(path: &Path) {
    // A file that is gone already needs nothing.
    let _ = fs::remove_file(path);
}

/// Returns the line of what the flag served.
fn report(
    stopped: &str,
    elapsed: u64,
    served: &[Served],
    conns: &[Conn<'_>],
    refused: u64,
    limits: &CardLimits,
) -> JsObject {
    let mut fetches = JsObject::with_capacity(served.len());
    for instance in served {
        fetches.insert(instance.name, instance.fetches.get());
    }
    let connections: Vec<JsValue> = conns
        .iter()
        .map(|conn| {
            let mut object = JsObject::with_capacity(5);
            object.insert(
                "instance",
                served
                    .get(conn.instance)
                    .map_or("", |instance| instance.name),
            );
            object.insert("form", conn.link.form());
            object.insert("answered", conn.answered);
            object.insert(
                "ended_ms",
                conn.ended_ms.map_or(JsValue::Null, JsValue::from),
            );
            object.insert(
                "error",
                conn.error.clone().map_or(JsValue::Null, JsValue::from),
            );
            JsValue::Object(object)
        })
        .collect();
    let mut object = JsObject::with_capacity(8);
    object.insert("stopped", stopped);
    object.insert("time_ms", elapsed / 1_000_000);
    object.insert("card", limits.card);
    object.insert(
        "instances",
        served
            .iter()
            .map(|instance| JsValue::from(instance.name))
            .collect::<Vec<_>>(),
    );
    object.insert("accepted", u64::try_from(conns.len()).unwrap_or(u64::MAX));
    object.insert("refused", refused);
    object.insert("connections", connections);
    object.insert("fetches", fetches);
    object
}

/// One instance served: its fetch, its count of fetches and its two
/// listening sockets.
struct Served {
    /// The name of the instance.
    name: &'static str,
    /// The fetch of its booted app, counted.
    fetch: Fetch,
    /// The fetches its app answered.
    fetches: Rc<Cell<u64>>,
    /// The `SOCK_SEQPACKET` listener.
    seq: SeqListener,
    /// The stream listener of prefixed `AoFrame`.
    stream: StreamListener,
    /// The path of the `SOCK_SEQPACKET` socket.
    seq_path: PathBuf,
    /// The path of the stream socket.
    stream_path: PathBuf,
}

impl Served {
    /// Boots `yoga` as the app of the instance `name` and binds its two
    /// sockets in `dir`.
    fn bind(name: &'static str, dir: &Path, yoga: &Rc<HttpServer>) -> io::Result<Self> {
        let seq_path = dir.join(format!("{name}.homa"));
        let stream_path = dir.join(format!("{name}.stream"));
        remove(&seq_path);
        remove(&stream_path);
        let inner = served::fetch(booted(yoga));
        let fetches = Rc::new(Cell::new(0u64));
        let counted = Rc::clone(&fetches);
        let fetch: Fetch = Rc::new(move |request: Request| -> Reply {
            counted.set(counted.get() + 1);
            inner(request)
        });
        let seq = SeqListener::bind(&seq_path)?;
        let stream = StreamListener::bind(&stream_path).inspect_err(|_| remove(&seq_path))?;
        Ok(Self {
            name,
            fetch,
            fetches,
            seq,
            stream,
            seq_path,
            stream_path,
        })
    }
}

/// Returns the booted app whose one middleware is `yoga`, mounted as
/// `handler` of `aosabi-core` mounts its Yoga: a 404 of Yoga passes
/// the request on, and every other answer becomes the response of the context
/// through `c.newResponse`, which keeps the substatus.
fn booted(yoga: &Rc<HttpServer>) -> Rc<BootedApp> {
    let yoga = Rc::clone(yoga);
    let mut app = App::default();
    app.r#use("*", move |c: aosabi_http::Context, next: Next| {
        let yoga = Rc::clone(&yoga);
        async move {
            let server_context = ServerContext {
                env: c.env(),
                execution_ctx: c.execution_ctx(),
            };
            let response = answer(&yoga, c.req().raw(), server_context.into_context())
                .await
                .map_err(Thrown)?;
            if response.status() == 404 {
                return next.run(c).await;
            }
            c.new_response(response.body(), &response)
        }
    });
    let (app, _) = BootedApp::new(app);
    Rc::new(app)
}

/// The Homa values every endpoint of the flag takes.
struct Spec {
    /// The profile of Unix Socket.
    profile: Profile,
    /// What the server announces and takes of a client until its LIMITS
    /// arrive.
    limits: Limits,
}

impl Spec {
    /// Returns the values of the profile of Unix Socket with messages of up
    /// to [`MAX_MESSAGE`] bytes.
    const fn new() -> Self {
        let profile = Profile::unix_socket(MAX_MESSAGE);
        Self {
            profile,
            limits: Limits {
                max_packet: profile.max_packet,
                max_message: MAX_MESSAGE,
                unsched_bytes: profile.unsched_bytes,
                max_rpcs: 16,
            },
        }
    }
}

/// The link of one connection.
enum Link<'b> {
    /// A `SOCK_SEQPACKET` socket, one Homa packet a record.
    Seq(SeqPacket),
    /// A stream socket in prefixed `AoFrame`.
    Stream(Framed<'b, UnixStream>),
}

impl Link<'_> {
    /// Returns the name of the link's form.
    const fn form(&self) -> &'static str {
        match self {
            Self::Seq(_) => "seqpacket",
            Self::Stream(_) => "stream",
        }
    }

    /// Returns whether the peer closed its end.
    fn closed(&self) -> bool {
        match self {
            Self::Seq(link) => link.closed(),
            Self::Stream(link) => link.closed(),
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
        }
    }

    fn recv(&mut self, buf: &mut [u8]) -> io::Result<Option<((), usize)>> {
        match self {
            Self::Seq(link) => link.recv(buf),
            Self::Stream(link) => link.recv(buf),
        }
    }
}

/// The buffers of the framing of one stream socket.
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

/// One connection of the flag.
struct Conn<'b> {
    /// The index of its instance.
    instance: usize,
    /// Its link.
    link: Link<'b>,
    /// Its endpoint.
    engine: Engine,
    /// Its listener.
    listener: Listener<()>,
    /// The answers its listener handed the engine.
    answered: u64,
    /// The error that ended its link, if one did.
    error: Option<String>,
    /// Whether it has ended.
    finished: bool,
    /// The milliseconds from the start of the flag to its end.
    ended_ms: Option<u64>,
}

impl<'b> Conn<'b> {
    /// Returns a connection of the instance `instance` over `link`, served by
    /// a listener of `fetch` with the bounds `limits`.
    fn new(
        instance: usize,
        link: Link<'b>,
        spec: &Spec,
        fetch: &Fetch,
        limits: &'static CardLimits,
    ) -> Result<Self, String> {
        let mut engine =
            Engine::new(CONFIG, spec.profile, Owned::new(&CONFIG)).map_err(config_error_name)?;
        engine
            .link_up((), spec.limits, spec.limits)
            .map_err(|error| format!("link:{}", link_error_name(error)))?;
        let listener = Listener::new(Rc::clone(fetch), limits)
            .map_err(|error| format!("listener: {error}"))?;
        Ok(Self {
            instance,
            link,
            engine,
            listener,
            answered: 0,
            error: None,
            finished: false,
            ended_ms: None,
        })
    }

    /// Runs one turn at `now` and returns whether anything moved: the link
    /// into the engine, the listener, and the engine's answers onto the link.
    fn turn(&mut self, now: u64, rx: &mut [u8], cx: &mut Context<'_>) -> bool {
        if self.finished {
            return false;
        }
        if let Err(error) = self.engine.pump(&mut self.link, now, rx) {
            self.error = Some(error.to_string());
        }
        let progress = self.listener.turn_endpoint(&mut self.engine, cx);
        self.answered += u64::from(progress.answered);
        if let Err(error) = self.engine.pump(&mut self.link, now, rx) {
            self.error = Some(error.to_string());
        }
        if self.link.closed() || self.error.is_some() {
            self.finished = true;
            // A peer whose link is down already answers NoPeer, which needs
            // nothing.
            let _ = self.engine.link_down(());
            let progress = self.listener.turn_endpoint(&mut self.engine, cx);
            self.answered += u64::from(progress.answered);
        }
        progress.answered > 0
    }
}

/// Returns the nanoseconds since `start`.
fn since(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX)
}
