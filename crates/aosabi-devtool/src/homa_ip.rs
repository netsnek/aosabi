// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool homa_ip: GraphTP(Homa) on Network over raw IP 146, a server
 * of aosabi-carrier-network and a client of the crate homa, driven by a
 * notebook from two network namespaces.
 *
 * No analogue in the reference. The flood of forged and malformed datagrams
 * and the filter of the IP driver serve the hostile rows of the same notebook.
 * Its judging notebook is tests/notebooks/29_homa_raw.ipynb.
 */

//! The primitive `homa-ip` of the seam.
//!
//! `--requests PATH` answers a file of request lines, one response line each,
//! in the batch form of [`crate::batch`]. Every socket is a raw socket of IP
//! protocol 146, which needs `CAP_NET_RAW` in the network namespace of the
//! process, so a notebook runs the primitive as root inside a namespace of a
//! guest, and a socket that does not open answers `open_error` with the name
//! of the refusal, `NotPermitted`, `PortTaken` or `Io`.
//!
//! - `filter {encap, port, count}` answers the classic BPF program the IP
//!   driver attaches to admit the `count` ports from `port`.
//! - `sizes {config}` answers the configuration of an endpoint of Network,
//!   a server card's where `config` overrides nothing, the bytes of each type
//!   its storage holds, the length of each part, the bytes the storage takes
//!   and the buffers of its path.
//! - `serve {port, extra_ports, bind, protection, processes, loop, ...}`
//!   serves one endpoint of `aosabi-carrier-network` until `max_time_ms`
//!   passes or the file `stop_file` exists. Behind the endpoint stands the
//!   listener of `aosabi-core` in front of the Yoga instance `instance`, in
//!   front of the harness of this module where `harness` is true, or in front
//!   of a fetch that answers every request at once where `fixed` is true, or a
//!   responder of bytes, `echo` or `sized <n>`, under `responder`. The loop
//!   `direct` waits on the raw socket itself, as the helper does, and times
//!   the engine apart from the calls of its socket where `timed` is true, the
//!   pumps that moved a packet apart from those that moved none, the loop
//!   `runtime` runs `serve_ip_with` of the crate as a local task of the
//!   runtime of `aosabi-serve`, and the loop `driver` boots an app over
//!   the Yoga instance with the HTTP(TCP) listener of `aosabi-serve` on
//!   `http_port` and the driver of the crate, which reads the environment of
//!   the process as a served card does. A request whose bytes begin with the
//!   hex `restart_marker` rebuilds the endpoint of a responder with a new
//!   first identifier before it is answered, as a server that restarted. A
//!   responder whose answer finds no free page holds it and offers it again
//!   each turn until the endpoint takes it or the RPC is gone, while the RPC
//!   still executes and its client hears BUSY, and counts the answers that
//!   waited under `deferred` and those still held at the end under `held`.
//!   `sample_grants` sums the bytes the endpoint granted and has not received
//!   yet after each pump, read off the GRANTs it sends and the ranges of the
//!   DATA it receives, retransmissions among them, and keeps the largest sum.
//! - `load {target, port, client_port, ...}` sends requests from one client
//!   endpoint over the raw socket: `messages`, a list of hex strings answered
//!   one by one, `repeat {hex, count}`, the same message again, or `random
//!   {count, min, max, sizes, seed}`, seeded bytes, or `mutate {count, seeds,
//!   seed}`, seeded edits of the messages of `seeds`, with at most `window`
//!   open, at `rate_per_s` in an open loop where it is named, to the ports of
//!   `ports` in turn where it names them, and answers the latency of each
//!   completed RPC as percentiles, those of each port apart, the outcomes, the
//!   echo check where `check` is `echo`, and the answers of `messages`.
//! - `peers {target, port, client_port, client_ports, rounds}` opens one
//!   client endpoint that owns `client_ports` ports from `client_port` and
//!   runs `rounds`, each a list of `{port, hex, patient}` sent at once from
//!   the client port of that index and waited for, and answers each answer.
//! - `flood {target, port, count, seed, forged_permille}` sends forged and
//!   malformed datagrams from a raw socket, the forged ones packets of the
//!   full form from source ports and identifiers drawn from `seed`, and
//!   answers what it sent.
//!
//! The clock of every loop is the wall clock of this process.

use core::{
    future::{
        Future,
        poll_fn, //
    },
    mem,
    net::{
        IpAddr,
        Ipv4Addr,
        SocketAddr, //
    },
    pin::{
        Pin,
        pin, //
    },
    task::{
        Context,
        Poll,
        Waker, //
    },
    time::Duration, //
};
use std::{
    collections::BTreeSet,
    path::{
        Path,
        PathBuf, //
    },
    process::ExitCode,
    rc::Rc,
    time::Instant, //
};

use aosabi_carrier_network::{
    Counts,
    HomaNetwork,
    NetworkEndpoint,
    NetworkError,
    Protection,
    Requirement,
    Seed,
    Turn,
    path_buffers,
    seed,
    serve_ip_with, //
};
use aosabi_core::{
    App,
    BootedApp,
    Listen as _,
    graphtp::{
        Answer,
        Body,
        Broken,
        Chunks,
        Fetch,
        Listener,
        Reply,
        Request,
        fetch::{
            METHOD,
            URL, //
        },
        limits::{
            self,
            CardLimits, //
        }, //
    }, //
};
use aosabi_http::{
    Body as FetchBody,
    Env,
    Headers,
    Request as FetchRequest,
    Response,
    ResponseInit, //
};
use aosabi_json::{
    JsObject,
    JsString,
    JsValue, //
};
use aosabi_serve::{
    BuildOutput,
    TcpListen,
    runtime::{
        Runtime,
        hyper_rt::RuntimeTimer,
        spawn_local, //
    }, //
};
use graphql_over_http::{
    HttpServer,
    ServerContext,
    answer, //
};
use homa::{
    Carrier,
    Config,
    Encap,
    Endpoint,
    Event,
    Owned,
    Peer,
    PeerPrio,
    Priority,
    RequestOptions,
    RpcId,
    SendError,
    Slot,
    heap::Item,
    ip::{
        self,
        Options,
        RawSocket, //
    },
    sim::Rng,
    table::Bucket,
    wire::{
        AckEntry,
        Data,
        Form as _,
        Full,
        Packet,
        Ports, //
    }, //
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
        scenario::{
            failure_name,
            read_all,
            send_error_name,
            stats_json, //
        }, //
    },
    json::reader,
    seam,
    yoga::instances::Instances, //
};

/// The name of the primitive.
const NAME: &str = "homa-ip";

/// The stage the primitive lands at.
const LANDS_AT: &str = "H4";

/// The command line of the `homa-ip` primitive.
pub static GRAMMAR: Grammar = Grammar {
    options: &[cli::SMOKE, cli::REQUESTS],
    positional: None,
    conflicts: &[],
};

/// What the `homa-ip` primitive accepts.
#[derive(Debug)]
pub struct HomaIpArgs {
    /// Whether the call asks for the interface line.
    smoke: bool,
    /// The file of request lines of the batch form.
    requests: Option<PathBuf>,
}

impl HomaIpArgs {
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
    ("filter", op_filter),
    ("sizes", op_sizes),
    ("serve", op_serve),
    ("load", op_load),
    ("peers", op_peers),
    ("flood", op_flood),
];

/// Runs the `homa-ip` primitive.
pub fn run(args: &HomaIpArgs) -> ExitCode {
    if args.smoke {
        return seam::smoke_at(NAME, LANDS_AT);
    }
    let outcome = seam::required(args.requests.as_deref(), "--requests")
        .and_then(|path| batch::run(OPERATIONS, path));
    seam::finish(NAME, outcome)
}

/// The Homa port a request that names none serves, the HTTP port of the port
/// rule.
const DEFAULT_PORT: u16 = 3_000;

/// The longest wait of a loop on its raw socket, the helper's.
const MAX_WAIT: Duration = Duration::from_millis(1);

/// How often a loop looks whether its caller wants it to stop.
const STOP_EVERY: Duration = Duration::from_millis(50);

/// The media type of every answer of the harness but a stream.
const ANSWER_TYPE: &str = "application/graphql-response+json; charset=utf-8";

/// The media type of a stream of the harness.
const EVENT_STREAM: &str = "text/event-stream";

/// Returns the nanoseconds since `start`.
fn since_ns(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

/// Returns the process CPU in user mode and in system mode, in nanoseconds,
/// read from `/proc/self/stat`, or `(0, 0)` where it cannot be read.
///
/// It counts the CPU the process used and not the wall time it waited, so the
/// cost of a packet is the same on a quiet machine and on a loaded build host.
/// The engine runs in user mode and every system call of the raw socket in
/// system mode, so the user CPU bounds the engine's own cost with the system
/// calls not counted, which the notebook reads against the budget of 1
/// microsecond a packet. A kernel that accounts the time a hypervisor took the
/// CPU away charges none of it to the process, which the guests of the
/// notebooks run.
fn process_cpu_ns() -> (u64, u64) {
    let Ok(stat) = std::fs::read_to_string("/proc/self/stat") else {
        return (0, 0);
    };
    let Some(tail) = stat.rsplit_once(')').map(|(_, tail)| tail) else {
        return (0, 0);
    };
    let fields: Vec<&str> = tail.split_whitespace().collect();
    // After the command and its state, utime is index 11 and stime 12. The
    // clock ticks a second, 100 on Linux, which the harness confirms.
    let ns = |at: usize| -> u64 {
        fields
            .get(at)
            .and_then(|field| field.parse::<u64>().ok())
            .unwrap_or(0)
            .saturating_mul(1_000_000_000)
            / 100
    };
    (ns(11), ns(12))
}

/// Returns the nanoseconds this thread ran on a CPU, in user and in system
/// mode together, read from `/proc/thread-self/schedstat`, or 0 where it
/// cannot be read.
fn thread_cpu_ns() -> u64 {
    std::fs::read_to_string("/proc/thread-self/schedstat")
        .ok()
        .and_then(|text| text.split_whitespace().next()?.parse().ok())
        .unwrap_or(0)
}

/// Returns the name of a refusal of the path.
fn network_error_name(error: &NetworkError) -> String {
    match error {
        NetworkError::NotPermitted => String::from("NotPermitted"),
        NetworkError::PortTaken(port) => format!("PortTaken {port}"),
        NetworkError::ForeignModule => String::from("ForeignModule"),
        NetworkError::NoModule => String::from("NoModule"),
        NetworkError::NoProtection => String::from("NoProtection"),
        NetworkError::Config(error) => format!("Config {}", config_error_name(*error)),
        NetworkError::Io(error) => format!("Io {error}"),
    }
}

/// Returns the payload of a socket that did not open.
fn open_refused(error: &NetworkError) -> JsObject {
    let mut payload = JsObject::with_capacity(1);
    payload.insert("open_error", network_error_name(error));
    payload
}

/// Returns the local address and its family a request names under `bind`,
/// every address of IPv4 where it names none.
fn bind_of(fields: &Fields<'_>) -> Result<(IpAddr, Encap), JsString> {
    let bind = match fields.opt_str("bind")? {
        Some(text) => text
            .parse::<IpAddr>()
            .map_err(|_| fields.refuse("bind", "names no address"))?,
        None => IpAddr::V4(Ipv4Addr::UNSPECIFIED),
    };
    Ok((bind, aosabi_carrier_network::encap_of(bind)))
}

/// Returns the protection a request names under `protection`, which has no
/// default, as a card's has none.
fn protection_of(fields: &Fields<'_>) -> Result<Protection, JsString> {
    let name = fields.str("protection")?;
    Protection::parse(name).ok_or_else(|| fields.refuse("protection", "names no protection"))
}

/// Returns the options of a path a request names: its ports, its address,
/// its protection, the processes of the host, the MTU, the requirement and
/// the overrides of the configuration under `config`.
fn network_of(
    fields: &Fields<'_>,
    port_key: &str,
    extra_key: &str,
) -> Result<HomaNetwork, JsString> {
    let port: u16 = fields.int_or(port_key, DEFAULT_PORT)?;
    let extra_ports: u16 = fields.int_or(extra_key, 0)?;
    let (bind, encap) = bind_of(fields)?;
    let protection = protection_of(fields)?;
    let mut options = Options::new(encap, port);
    options.bind = bind;
    options.extra_ports = extra_ports;
    options.send_buffer = fields.int_or("send_buffer", aosabi_carrier_network::SOCKET_BUFFER)?;
    options.recv_buffer = fields.int_or("recv_buffer", aosabi_carrier_network::SOCKET_BUFFER)?;
    let seeded = seed().map_err(|error| JsString::from(format!("seed: {error}")))?;
    let mut network = HomaNetwork::ip(options, protection, seeded);
    network.processes = fields.int_or("processes", 1)?;
    network.mtu = fields.int_or("mtu", aosabi_carrier_network::DEFAULT_MTU)?;
    network.requirement = match fields.opt_str("requirement")? {
        None | Some("optional") => Requirement::Optional,
        Some("required") => Requirement::Required,
        Some(_) => return Err(fields.refuse("requirement", "names no requirement")),
    };
    override_config(fields, &mut network.config)?;
    Ok(network)
}

/// Writes the bounds a request names under `config` over `config`: the
/// peers, the slots of each role, the pages and the gaps a message may have.
fn override_config(fields: &Fields<'_>, config: &mut Config) -> Result<(), JsString> {
    if let Some(spec) = fields.opt_object("config")? {
        let spec = Fields::new(spec, "config");
        config.peers = spec.int_or("peers", config.peers)?;
        config.client_slots = spec.int_or("client_slots", config.client_slots)?;
        config.server_slots = spec.int_or("server_slots", config.server_slots)?;
        config.page_len = spec.int_or("page_len", config.page_len)?;
        config.pages = spec.int_or("pages", config.pages)?;
        config.max_gaps = spec.int_or("max_gaps", config.max_gaps)?;
    }
    Ok(())
}

/// Answers the operation `filter`.
fn op_filter(request: &JsObject) -> Result<JsObject, JsString> {
    let fields = Fields::new(request, "filter");
    let encap = match fields.opt_str("encap")? {
        None | Some("ip4") => Encap::Ip4,
        Some("ip6") => Encap::Ip6,
        Some(_) => return Err(fields.refuse("encap", "names no family")),
    };
    let program = ip::filter(
        encap,
        fields.int_or("port", DEFAULT_PORT)?,
        fields.int_or("count", 1)?,
    );
    let instructions: Vec<JsValue> = program
        .iter()
        .map(|op| {
            let mut entry = JsObject::with_capacity(4);
            entry.insert("code", u64::from(op.code));
            entry.insert("jt", u64::from(op.jt));
            entry.insert("jf", u64::from(op.jf));
            entry.insert("k", u64::from(op.k));
            JsValue::Object(entry)
        })
        .collect();
    let mut payload = JsObject::with_capacity(2);
    payload.insert("len", count(instructions.len()));
    payload.insert("instructions", instructions);
    Ok(payload)
}

/// Answers the operation `sizes`: the configuration of an endpoint of
/// Network, a server card's with the bounds `config` overrides, the bytes of
/// each type its storage holds, the length of each part, the bytes the whole
/// storage takes and the buffers of a served path.
///
/// A peer is an address with its Homa port, and the priority state of a
/// peer counts as this build of `homa` has it.
fn op_sizes(request: &JsObject) -> Result<JsObject, JsString> {
    let fields = Fields::new(request, "sizes");
    let seeded = Seed {
        first_id: 2,
        hash_key: [0x5a; 16],
    };
    let mut config = aosabi_carrier_network::server_config(DEFAULT_PORT, 0, seeded);
    override_config(&fields, &mut config)?;
    let len = config.storage();
    let mut types = JsObject::with_capacity(7);
    types.insert("peer", count(mem::size_of::<Peer<SocketAddr>>()));
    types.insert("peer_prio", count(mem::size_of::<PeerPrio>()));
    types.insert("slot", count(mem::size_of::<Slot>()));
    types.insert("bucket", count(mem::size_of::<Bucket>()));
    types.insert("item", count(mem::size_of::<Item>()));
    types.insert("gap", count(mem::size_of::<u32>()));
    types.insert("size", count(mem::size_of::<u64>()));
    let mut parts = JsObject::with_capacity(10);
    parts.insert("peers", count(len.peers));
    parts.insert("slots", count(len.slots));
    parts.insert("peer_index", count(len.peer_index));
    parts.insert("slot_index", count(len.slot_index));
    parts.insert("tx_heap", count(len.tx_heap));
    parts.insert("grant_heap", count(len.grant_heap));
    parts.insert("pages", count(len.pages));
    parts.insert("gaps", count(len.gaps));
    parts.insert("sizes", count(len.sizes));
    parts.insert("peer_prio", count(len.peer_prio));
    let mut payload = JsObject::with_capacity(12);
    payload.insert("peers", u64::from(config.peers));
    payload.insert("client_slots", u64::from(config.client_slots));
    payload.insert("server_slots", u64::from(config.server_slots));
    payload.insert("page_len", u64::from(config.page_len));
    payload.insert("pages", u64::from(config.pages));
    payload.insert("max_gaps", u64::from(config.max_gaps));
    payload.insert("types", types);
    payload.insert("parts", parts);
    payload.insert("storage_bytes", count(len.bytes::<SocketAddr>()));
    payload.insert(
        "page_bytes",
        u64::from(config.pages) * u64::from(config.page_len),
    );
    payload.insert("path_buffers", count(path_buffers()));
    Ok(payload)
}

// The harness behind the listener.

/// What the harness of this module counted.
#[derive(Debug, Default)]
struct Tally {
    /// The OPERATIONs it answered.
    calls: core::cell::Cell<u64>,
    /// The streams it opened.
    streams: core::cell::Cell<u64>,
    /// The stream sources dropped before their end, by a CANCEL or a peer
    /// gone.
    returned: core::cell::Cell<u64>,
}

/// Returns the fetch of the harness: `subscription{tick}` answers an endless
/// event stream of one result every `interval`, tagged with the variable
/// `tag` where the request names one, `mutation{pause}` answers
/// `{"data":{"pause":true}}`, and every other query `{"data":{"ok":true}}`,
/// the calls of `docs/graphtp.md` section 6.2.
fn harness_fetch(tally: &Rc<Tally>, interval: Duration) -> Fetch {
    let tally = Rc::clone(tally);
    Rc::new(move |request: Request| -> Reply {
        tally.calls.set(tally.calls.get() + 1);
        let (query, tag) = match reader::parse(&request.content) {
            Ok(JsValue::Object(content)) => (
                content.get("query").and_then(|query| match query {
                    JsValue::String(text) => text.as_str().map(String::from),
                    _ => None,
                }),
                match content.get("variables") {
                    Some(JsValue::Object(variables)) => match variables.get("tag") {
                        Some(JsValue::String(tag)) => tag.as_str().map(String::from),
                        _ => None,
                    },
                    _ => None,
                },
            ),
            _ => {
                return ready(plain(
                    400,
                    6,
                    Some(ANSWER_TYPE),
                    br#"{"errors":[{"message":"The content is no JSON object."}]}"#.to_vec(),
                ));
            }
        };
        let answer = match query.as_deref() {
            Some("subscription{tick}") => {
                tally.streams.set(tally.streams.get() + 1);
                Answer {
                    status: 200,
                    substatus: 0,
                    content_type: Some(EVENT_STREAM.as_bytes().to_vec()),
                    body: Body::Stream(Box::new(Ticks {
                        tally: Rc::clone(&tally),
                        tag,
                        interval,
                        next_at: Instant::now(),
                        sent: 0,
                        opened: false,
                    })),
                }
            }
            Some("mutation{pause}") => plain(
                200,
                0,
                Some(ANSWER_TYPE),
                br#"{"data":{"pause":true}}"#.to_vec(),
            ),
            _ => plain(
                200,
                0,
                Some(ANSWER_TYPE),
                br#"{"data":{"ok":true}}"#.to_vec(),
            ),
        };
        ready(answer)
    })
}

/// The endless source of a stream of the harness, an event stream as Yoga
/// writes it.
struct Ticks {
    /// The counts of the harness.
    tally: Rc<Tally>,
    /// The tag each result carries.
    tag: Option<String>,
    /// The time between two results.
    interval: Duration,
    /// When the next result is due.
    next_at: Instant,
    /// The results written.
    sent: u64,
    /// Whether the opening comment is written.
    opened: bool,
}

impl Chunks for Ticks {
    fn poll_chunk(&mut self, _cx: &mut Context<'_>) -> Poll<Option<Result<Vec<u8>, Broken>>> {
        if !self.opened {
            self.opened = true;
            return Poll::Ready(Some(Ok(b":\n\n".to_vec())));
        }
        let now = Instant::now();
        if now < self.next_at {
            return Poll::Pending;
        }
        self.sent += 1;
        self.next_at = now + self.interval;
        let mut tick = JsObject::with_capacity(2);
        tick.insert("tick", self.sent);
        if let Some(tag) = &self.tag {
            tick.insert("tag", tag.as_str());
        }
        let mut data = JsObject::with_capacity(1);
        data.insert("data", tick);
        let mut chunk = b"event: next\ndata: ".to_vec();
        aosabi_json::write(&mut chunk, &JsValue::Object(data));
        chunk.extend_from_slice(b"\n\n");
        Poll::Ready(Some(Ok(chunk)))
    }
}

impl Drop for Ticks {
    fn drop(&mut self) {
        self.tally.returned.set(self.tally.returned.get() + 1);
    }
}

/// Returns the fetch that answers every request at once with
/// `{"data":{"ok":true}}` and reads nothing of it, so a loop times the
/// listener apart from any app behind it.
fn fixed_fetch() -> Fetch {
    Rc::new(|_request: Request| -> Reply {
        ready(plain(
            200,
            0,
            Some(ANSWER_TYPE),
            br#"{"data":{"ok":true}}"#.to_vec(),
        ))
    })
}

/// Returns a reply that is ready at once.
fn ready(answer: Answer) -> Reply {
    Box::pin(core::future::ready(answer))
}

/// Returns an answer with a body written whole.
fn plain(status: u16, substatus: u8, content_type: Option<&str>, body: Vec<u8>) -> Answer {
    Answer {
        status,
        substatus,
        content_type: content_type.map(|kind| kind.as_bytes().to_vec()),
        body: Body::Full(body),
    }
}

/// Returns the Yoga instance a request names under `instance`.
fn instance_of(fields: &Fields<'_>, name: &str) -> Result<Rc<HttpServer>, JsString> {
    let instances = Instances::new().map_err(JsString::from)?;
    instances
        .get(name)
        .cloned()
        .ok_or_else(|| fields.refuse("instance", &format!("names no instance: {name}")))
}

/// Returns the fetch of the listener over the Yoga instance `yoga`, the POST
/// of the fixed fields of `docs/graphtp-in-aosabi.md` section 2.1.
fn yoga_fetch(yoga: Rc<HttpServer>) -> Fetch {
    Rc::new(move |request: Request| -> Reply {
        let yoga = Rc::clone(&yoga);
        Box::pin(async move {
            let mut headers = Headers::new();
            for (name, value) in request.fields() {
                let appended = core::str::from_utf8(value)
                    .ok()
                    .and_then(|value| headers.append(name, value).ok());
                if appended.is_none() {
                    return plain(400, 0, None, Vec::new());
                }
            }
            let body = FetchBody::Full(Rc::from(request.content));
            let fetch = Rc::new(FetchRequest::new(METHOD, URL, headers, body));
            let context = ServerContext {
                env: Env::Process,
                execution_ctx: None,
            };
            let Ok(response) = answer(&yoga, fetch, context.into_context()).await else {
                return plain(500, 0, None, Vec::new());
            };
            let content_type = response.headers().get("content-type");
            match response.bytes().await {
                Ok(bytes) => Answer {
                    status: response.status(),
                    substatus: response.substatus(),
                    content_type: content_type.map(String::into_bytes),
                    body: Body::Full(bytes.to_vec()),
                },
                Err(_) => plain(500, 0, None, Vec::new()),
            }
        })
    })
}

// The responder of bytes behind the endpoint.

/// A responder of bytes over the Homa layer alone.
#[derive(Clone, Copy, Debug)]
enum Responder {
    /// The request's own bytes.
    Echo,
    /// The given number of bytes, a pattern of the byte values in turn.
    Sized(usize),
}

impl Responder {
    /// Returns the answer to `request`.
    fn answer(self, request: &[u8]) -> Vec<u8> {
        match self {
            Self::Echo => request.to_vec(),
            Self::Sized(len) => (0..len).map(|n| n.to_le_bytes()[0]).collect(),
        }
    }
}

/// The turn of a responder, which answers each whole request at once and
/// asks for a restart where a request begins with the marker.
struct Echo {
    /// What answers.
    responder: Responder,
    /// The bytes a request that asks for a restart begins with.
    marker: Option<Vec<u8>>,
    /// Whether a request asked for a restart in the last turn.
    restart: bool,
    /// The whole requests that arrived.
    requests: u64,
    /// The answers the endpoint refused for good.
    refused: u64,
    /// The answers that found no free page, each with the RPC it answers.
    held: Vec<(RpcId, Vec<u8>)>,
    /// The answers that found no free page at least once.
    deferred: u64,
}

impl Echo {
    /// Returns the turn of `responder`, which asks for a restart where a
    /// request begins with `marker`.
    fn new(responder: Responder, marker: Option<Vec<u8>>) -> Self {
        Self {
            responder,
            marker,
            restart: false,
            requests: 0,
            refused: 0,
            held: Vec::new(),
            deferred: 0,
        }
    }

    /// Hands the endpoint the answer `answer` of the RPC `id`, holds it where
    /// no page is free, and returns whether the endpoint took it.
    ///
    /// The pages of a message come back when it ends, so an answer refused
    /// as `NoSlot` waits for the next turn, while its RPC executes and its
    /// client hears BUSY. Every other refusal is for good, `NoRequest` among
    /// them, which an RPC whose peer died meets.
    fn answer(
        &mut self,
        endpoint: &mut NetworkEndpoint,
        id: RpcId,
        answer: Vec<u8>,
        first: bool,
    ) -> bool {
        match endpoint.reply(id, &answer) {
            Ok(()) => true,
            Err(SendError::NoSlot) => {
                self.deferred += u64::from(first);
                self.held.push((id, answer));
                false
            }
            Err(_) => {
                self.refused += 1;
                false
            }
        }
    }
}

impl Turn for Echo {
    fn turn(&mut self, endpoint: &mut NetworkEndpoint, _cx: &mut Context<'_>) -> u32 {
        let mut answered = 0;
        for (id, answer) in mem::take(&mut self.held) {
            answered += u32::from(self.answer(endpoint, id, answer, false));
        }
        while let Some(event) = endpoint.poll_event() {
            let Event::Request { id, .. } = event else {
                continue;
            };
            self.requests += 1;
            let request = read_all(endpoint.request_of(id)).unwrap_or_default();
            if self
                .marker
                .as_deref()
                .is_some_and(|marker| request.starts_with(marker))
            {
                self.restart = true;
                continue;
            }
            let answer = self.responder.answer(&request);
            answered += u32::from(self.answer(endpoint, id, answer, true));
        }
        answered
    }
}

/// What a request names behind the endpoint.
enum Behind {
    /// A Yoga instance or the harness behind the listener.
    Listener(Fetch, &'static CardLimits),
    /// A responder of bytes.
    Responder(Responder),
}

/// Returns what a request names behind the endpoint and the counts of the
/// harness where it names the harness.
fn behind_of(fields: &Fields<'_>, tally: &Rc<Tally>) -> Result<Behind, JsString> {
    if let Some(text) = fields.opt_str("responder")? {
        return match text.split_once(' ') {
            None if text == "echo" => Ok(Behind::Responder(Responder::Echo)),
            Some(("sized", n)) => n
                .parse()
                .map(|n| Behind::Responder(Responder::Sized(n)))
                .map_err(|_| fields.refuse("responder", "names no length after sized")),
            _ => Err(fields.refuse("responder", &format!("names no responder: {text}"))),
        };
    }
    let name = fields.opt_str("card")?.unwrap_or("server");
    let card = limits::card(name)
        .ok_or_else(|| fields.refuse("card", &format!("names no card: {name}")))?;
    if fields.bool_or("fixed", false)? {
        return Ok(Behind::Listener(fixed_fetch(), card));
    }
    if fields.bool_or("harness", false)? {
        let interval = Duration::from_millis(fields.int_or("interval_ms", 20)?);
        return Ok(Behind::Listener(harness_fetch(tally, interval), card));
    }
    let name = fields.opt_str("instance")?.unwrap_or("prod");
    Ok(Behind::Listener(
        yoga_fetch(instance_of(fields, name)?),
        card,
    ))
}

/// Returns the listener of `fetch` with the bounds `card`.
fn listener_of(fetch: Fetch, card: &'static CardLimits) -> Result<Listener<SocketAddr>, JsString> {
    Listener::new(fetch, card).map_err(|error| JsString::from(format!("listener: {error}")))
}

/// Tells whether a loop should stop: past `max_time` or once `stop_file`
/// exists.
struct Stop {
    /// When the loop started.
    start: Instant,
    /// The longest run.
    max_time: Duration,
    /// The file whose existence stops the loop.
    file: Option<PathBuf>,
    /// When the loop looked last.
    looked: Instant,
}

impl Stop {
    /// Reads `max_time_ms` and `stop_file` of a request.
    fn read(fields: &Fields<'_>) -> Result<Self, JsString> {
        let now = Instant::now();
        Ok(Self {
            start: now,
            max_time: Duration::from_millis(fields.int_or("max_time_ms", 600_000)?),
            file: fields.opt_str("stop_file")?.map(PathBuf::from),
            looked: now,
        })
    }

    /// Returns why the loop stops now, or `None`, looking at the file at most
    /// every [`STOP_EVERY`].
    fn due(&mut self) -> Option<&'static str> {
        let now = Instant::now();
        if now.duration_since(self.start) > self.max_time {
            return Some("max_time");
        }
        if now.duration_since(self.looked) < STOP_EVERY {
            return None;
        }
        self.looked = now;
        self.file
            .as_deref()
            .is_some_and(Path::exists)
            .then_some("stop_file")
    }
}

/// The raw socket of a direct loop with what it counts.
struct DirectLink<'r> {
    /// The socket.
    raw: &'r mut RawSocket,
    /// Whether the calls of the socket are timed.
    timed: bool,
    /// The nanoseconds the calls of the socket took.
    socket_ns: u64,
    /// The first Homa port of the endpoint and the count of its ports.
    ports: (u16, u16),
    /// The packets received whose destination port the endpoint does not
    /// own, which the filter of the driver keeps out.
    crossed: u64,
    /// The distinct peers that sent a packet, where the request asks for
    /// them.
    seen: Option<BTreeSet<SocketAddr>>,
    /// The packets the socket took and gave.
    moved: u64,
    /// What the endpoint granted and what arrived, where the request asks
    /// for it.
    grants: Option<Grants>,
}

impl Carrier for DirectLink<'_> {
    type Peer = SocketAddr;
    type Error = std::io::Error;

    fn ready(&self, peer: SocketAddr, len: usize) -> bool {
        self.raw.ready(peer, len)
    }

    fn send(
        &mut self,
        peer: SocketAddr,
        priority: Priority,
        header: &[u8],
        payload: &[u8],
    ) -> std::io::Result<()> {
        let start = self.timed.then(Instant::now);
        let sent = self.raw.send(peer, priority, header, payload);
        if let Some(start) = start {
            self.socket_ns = self.socket_ns.saturating_add(since_ns(start));
        }
        self.moved += 1;
        if let Some(grants) = self.grants.as_mut() {
            grants.sent(peer, header);
        }
        sent
    }

    fn recv(&mut self, buf: &mut [u8]) -> std::io::Result<Option<(SocketAddr, usize)>> {
        let start = self.timed.then(Instant::now);
        let received = self.raw.recv(buf);
        if let Some(start) = start {
            self.socket_ns = self.socket_ns.saturating_add(since_ns(start));
        }
        if let Ok(Some((peer, len))) = &received {
            self.moved += 1;
            if let Some(grants) = self.grants.as_mut() {
                grants.received(*peer, buf.get(..*len).unwrap_or(&[]));
            }
            let dst = buf
                .get(2..4)
                .filter(|_| *len >= 4)
                .map_or(0, |port| u16::from_be_bytes([port[0], port[1]]));
            let (first, ports) = self.ports;
            if u32::from(dst) < u32::from(first)
                || u32::from(dst) >= u32::from(first) + u32::from(ports)
            {
                self.crossed += 1;
            }
            if let Some(seen) = self.seen.as_mut() {
                seen.insert(*peer);
            }
        }
        received
    }
}

/// The pumps of one kind and the engine's nanoseconds in them.
#[derive(Clone, Copy, Debug, Default)]
struct Split {
    /// The pumps.
    pumps: u64,
    /// The nanoseconds of the engine in them, the calls of the socket not
    /// counted.
    ns: u64,
}

impl Split {
    /// Counts one pump whose engine took `ns`.
    fn add(&mut self, ns: u64) {
        self.pumps += 1;
        self.ns = self.ns.saturating_add(ns);
    }
}

/// One incoming message an endpoint has granted or received bytes of.
#[derive(Clone, Debug)]
struct Open {
    /// The peer and the client's identifier of its RPC.
    key: (SocketAddr, u64),
    /// The first byte not granted.
    granted: u32,
    /// The ranges of the message that arrived, sorted and apart.
    ranges: Vec<(u32, u32)>,
    /// The bytes those ranges hold.
    arrived: u32,
    /// Whether every byte of the message arrived.
    done: bool,
}

impl Open {
    /// Adds the range of `len` bytes from `offset` to what arrived and
    /// returns the bytes the ranges hold now.
    fn arrive(&mut self, offset: u32, len: u32) -> u32 {
        if len == 0 {
            return self.arrived;
        }
        self.ranges.push((offset, offset.saturating_add(len)));
        self.ranges.sort_unstable();
        let mut merged: Vec<(u32, u32)> = Vec::with_capacity(self.ranges.len());
        for &(from, to) in &self.ranges {
            match merged.last_mut() {
                Some(last) if from <= last.1 => last.1 = last.1.max(to),
                _ => merged.push((from, to)),
            }
        }
        self.arrived = merged.iter().map(|&(from, to)| to - from).sum();
        self.ranges = merged;
        self.arrived
    }
}

/// What an endpoint granted of each incoming message and what of it
/// arrived, read off the packets it sends and receives.
///
/// A GRANT the endpoint sends raises the granted offset of its message, and
/// each DATA it receives adds its range to the ranges that arrived, a
/// retransmission filling a gap and a duplicate adding nothing, until the
/// whole message is there and the message closes. An RPC is keyed by its peer
/// and its client's identifier, the even one, since each end names it by its
/// own identifier and the two differ in the lowest bit alone.
#[derive(Debug, Default)]
struct Grants {
    /// The messages seen, closed ones among them, so a GRANT that crosses
    /// the last DATA of its message opens nothing. A load sends few messages
    /// to one server, so a list searched from the start serves.
    open: Vec<Open>,
    /// The largest sum of bytes granted and not arrived seen.
    max_outstanding: u64,
}

impl Grants {
    /// Returns the message `key`, opened where it is new.
    fn entry(&mut self, key: (SocketAddr, u64)) -> &mut Open {
        let at = if let Some(at) = self.open.iter().position(|open| open.key == key) {
            at
        } else {
            self.open.push(Open {
                key,
                granted: 0,
                ranges: Vec::new(),
                arrived: 0,
                done: false,
            });
            self.open.len() - 1
        };
        &mut self.open[at]
    }

    /// Reads a packet the endpoint sent to `peer`.
    fn sent(&mut self, peer: SocketAddr, header: &[u8]) {
        if let Ok(decoded) = Full::decode(header)
            && let Packet::Grant { id, offset, .. } = decoded.packet
        {
            let open = self.entry((peer, id & !1));
            if !open.done {
                open.granted = open.granted.max(offset);
            }
        }
    }

    /// Reads a packet the endpoint received from `peer`.
    fn received(&mut self, peer: SocketAddr, packet: &[u8]) {
        let Ok(decoded) = Full::decode(packet) else {
            return;
        };
        let Packet::Data(data) = decoded.packet else {
            return;
        };
        let open = self.entry((peer, data.id & !1));
        if open.done {
            return;
        }
        let len = u32::try_from(data.payload.len()).unwrap_or(u32::MAX);
        if open.arrive(data.offset, len) >= data.msg_length {
            open.done = true;
            open.ranges = Vec::new();
        }
    }

    /// Adds up the bytes granted and not arrived of the open messages now
    /// and keeps the largest sum.
    fn sample(&mut self) {
        let now: u64 = self
            .open
            .iter()
            .filter(|open| !open.done)
            .map(|open| u64::from(open.granted.saturating_sub(open.arrived)))
            .sum();
        self.max_outstanding = self.max_outstanding.max(now);
    }
}

/// Returns the peers of `seen` the table of `endpoint` holds now.
fn peers_held(endpoint: &NetworkEndpoint, seen: &BTreeSet<SocketAddr>) -> u64 {
    count(
        seen.iter()
            .filter(|peer| endpoint.peer_cutoffs(**peer).is_some())
            .count(),
    )
}

/// Answers the operation `serve`.
fn op_serve(request: &JsObject) -> Result<JsObject, JsString> {
    let fields = Fields::new(request, "serve");
    match fields.opt_str("loop")?.unwrap_or("direct") {
        "direct" => serve_direct(&fields),
        "runtime" => serve_runtime(&fields),
        "driver" => serve_driver(&fields),
        other => Err(fields.refuse("loop", &format!("names no loop: {other}"))),
    }
}

/// Serves in the direct loop, which waits on the raw socket itself.
#[expect(
    clippy::too_many_lines,
    reason = "one loop with its counters, kept whole so a reader sees every step of a turn"
)]
fn serve_direct(fields: &Fields<'_>) -> Result<JsObject, JsString> {
    let options = network_of(fields, "port", "extra_ports")?;
    let tally = Rc::new(Tally::default());
    let behind = behind_of(fields, &tally)?;
    let marker = match fields.opt_str("restart_marker")? {
        Some(text) => {
            Some(unhex(text).ok_or_else(|| fields.refuse("restart_marker", "is no hex"))?)
        }
        None => None,
    };
    let timed = fields.bool_or("timed", false)?;
    let sample = fields.bool_or("sample_grants", false)?;
    let track = fields.bool_or("track_peers", false)?;
    let mut stop = Stop::read(fields)?;
    let mut raw = match aosabi_carrier_network::open(&options) {
        Ok(raw) => raw,
        Err(error) => return Ok(open_refused(&error)),
    };
    let mut endpoint = aosabi_carrier_network::endpoint(&options)
        .map_err(|error| JsString::from(network_error_name(&error)))?;
    let (mut listener, mut echo) = match behind {
        Behind::Listener(fetch, card) => (Some(listener_of(fetch, card)?), None),
        Behind::Responder(responder) => (None, Some(Echo::new(responder, marker))),
    };
    let ports = (
        options.config.port,
        options.config.extra_ports.saturating_add(1),
    );
    let mut rx = vec![0u8; 65_536];
    let mut answered = 0u64;
    let mut pump_ns = 0u64;
    let mut pumps = 0u64;
    let mut busy = Split::default();
    let mut idle = Split::default();
    let mut restarts = 0u64;
    let mut peer_samples: Vec<u64> = Vec::new();
    let mut next_sample = 1u64 << 17;
    let mut link = DirectLink {
        raw: &mut raw,
        timed,
        socket_ns: 0,
        ports,
        crossed: 0,
        seen: track.then(BTreeSet::new),
        moved: 0,
        grants: sample.then(Grants::default),
    };
    let mut cx = Context::from_waker(Waker::noop());
    let (user_before, sys_before) = process_cpu_ns();
    let cpu_before = thread_cpu_ns();
    let start = Instant::now();
    let stopped = loop {
        if let Some(why) = stop.due() {
            break why;
        }
        for _ in 0..2 {
            let (socket_before, moved_before) = (link.socket_ns, link.moved);
            let begun = timed.then(Instant::now);
            pumps += 1;
            if let Err(error) = endpoint.pump(&mut link, since_ns(start), &mut rx) {
                return Err(JsString::from(format!("the raw socket failed: {error}")));
            }
            if let Some(begun) = begun {
                let took = since_ns(begun);
                pump_ns = pump_ns.saturating_add(took);
                // The engine's part of the pump is its time less the calls of
                // the socket, kept apart for the pumps that moved a packet.
                let engine = took.saturating_sub(link.socket_ns.saturating_sub(socket_before));
                if link.moved == moved_before {
                    idle.add(engine);
                } else {
                    busy.add(engine);
                }
            }
            if let Some(grants) = link.grants.as_mut() {
                grants.sample();
            }
            match (&mut listener, &mut echo) {
                (Some(listener), _) => {
                    answered += u64::from(listener.turn_endpoint(&mut endpoint, &mut cx).answered);
                }
                (None, Some(echo)) => {
                    answered += u64::from(echo.turn(&mut endpoint, &mut cx));
                }
                (None, None) => {}
            }
        }
        if echo.as_ref().is_some_and(|echo| echo.restart) {
            if let Some(echo) = echo.as_mut() {
                echo.restart = false;
            }
            restarts += 1;
            let mut fresh = options.clone();
            fresh.config.first_id = seed()
                .map_or(fresh.config.first_id ^ 0x5a5a_5a5a_0000, |seeded| {
                    seeded.first_id
                });
            endpoint = aosabi_carrier_network::endpoint(&fresh)
                .map_err(|error| JsString::from(network_error_name(&error)))?;
        }
        if let Some(seen) = link.seen.as_ref() {
            let received = link.raw.counts().received;
            if received >= next_sample {
                peer_samples.push(peers_held(&endpoint, seen));
                next_sample = next_sample.saturating_add(1 << 17);
            }
        }
        let now = since_ns(start);
        let wait = endpoint
            .next_timeout()
            .map_or(MAX_WAIT, |due| {
                Duration::from_nanos(due.saturating_sub(now))
            })
            .min(MAX_WAIT);
        if !link.raw.flush() {
            continue;
        }
        if let Err(error) = link.raw.wait(wait) {
            return Err(JsString::from(format!("the raw socket failed: {error}")));
        }
    };
    let (user_after, sys_after) = process_cpu_ns();
    let cpu_after = thread_cpu_ns();
    let time_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
    let stats = endpoint.stats();
    let mut payload = JsObject::new();
    payload.insert("stopped", stopped);
    payload.insert("time_ms", time_ms);
    payload.insert("answered", answered);
    payload.insert("cpu_user_ns", user_after.saturating_sub(user_before));
    payload.insert("cpu_sys_ns", sys_after.saturating_sub(sys_before));
    payload.insert("cpu_ns", cpu_after.saturating_sub(cpu_before));
    payload.insert("stats", stats_json(&stats));
    payload.insert("driver", driver_json(&link.raw.counts()));
    payload.insert("crossed", link.crossed);
    payload.insert("pumps", pumps);
    payload.insert("restarts", restarts);
    if timed {
        payload.insert("pump_ns", pump_ns);
        payload.insert("socket_ns", link.socket_ns);
        payload.insert("engine_ns", pump_ns.saturating_sub(link.socket_ns));
        payload.insert("packets", stats.sent.saturating_add(stats.received));
        payload.insert("moved", link.moved);
        payload.insert("busy_pumps", busy.pumps);
        payload.insert("busy_engine_ns", busy.ns);
        payload.insert("idle_pumps", idle.pumps);
        payload.insert("idle_engine_ns", idle.ns);
    }
    if sample {
        payload.insert(
            "max_outstanding",
            link.grants
                .as_ref()
                .map_or(0, |grants| grants.max_outstanding),
        );
        payload.insert(
            "max_incoming",
            u64::from(aosabi_carrier_network::profile(&options).max_incoming),
        );
    }
    if let Some(seen) = link.seen.as_ref() {
        payload.insert("peers_seen", count(seen.len()));
        payload.insert("peers_held", peers_held(&endpoint, seen));
        payload.insert(
            "peers_held_samples",
            peer_samples
                .into_iter()
                .map(JsValue::from)
                .collect::<Vec<_>>(),
        );
        payload.insert("peers_size", u64::from(options.config.peers));
    }
    if let Some(echo) = echo.as_ref() {
        payload.insert("requests", echo.requests);
        payload.insert("refused", echo.refused);
        payload.insert("deferred", echo.deferred);
        payload.insert("held", count(echo.held.len()));
    }
    payload.insert("harness", tally_json(&tally));
    Ok(payload)
}

/// Returns what the harness counted as an object.
fn tally_json(tally: &Tally) -> JsObject {
    let mut object = JsObject::with_capacity(3);
    object.insert("calls", tally.calls.get());
    object.insert("streams", tally.streams.get());
    object.insert("returned", tally.returned.get());
    object
}

/// Returns what the raw socket counted as an object.
fn driver_json(counts: &ip::Counts) -> JsObject {
    let mut object = JsObject::with_capacity(7);
    object.insert("received", counts.received);
    object.insert("sent", counts.sent);
    object.insert("malformed", counts.malformed);
    object.insert("truncated", counts.truncated);
    object.insert("other_family", counts.other_family);
    object.insert("send_failed", counts.send_failed);
    object.insert("dropped", counts.dropped);
    object
}

/// Runs `served` on a runtime of its own until it ends or `stop` is due,
/// and returns why it stopped.
fn run_until<F>(served: F, mut stop: Stop) -> Result<(String, u64), JsString>
where
    F: Future<Output = Result<(), String>>,
{
    let runtime = Runtime::new().map_err(|error| JsString::from(format!("runtime: {error}")))?;
    let start = Instant::now();
    let stopped = runtime.block_on(async {
        let timer = match RuntimeTimer::current() {
            Ok(timer) => timer,
            Err(error) => return format!("runtime: {error}"),
        };
        let mut tick = timer.sleep_for(STOP_EVERY);
        let mut served = pin!(served);
        poll_fn(|cx| {
            if let Poll::Ready(result) = served.as_mut().poll(cx) {
                return Poll::Ready(match result {
                    Ok(()) => String::from("ended"),
                    Err(error) => error,
                });
            }
            while Pin::new(&mut tick).poll(cx).is_ready() {
                if let Some(why) = stop.due() {
                    return Poll::Ready(String::from(why));
                }
                tick = timer.sleep_for(STOP_EVERY);
            }
            Poll::Pending
        })
        .await
    });
    Ok((
        stopped,
        u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX),
    ))
}

/// Serves in the runtime loop, `serve_ip_with` of the crate as a local task
/// of the runtime of `aosabi-serve`.
fn serve_runtime(fields: &Fields<'_>) -> Result<JsObject, JsString> {
    let options = network_of(fields, "port", "extra_ports")?;
    let tally = Rc::new(Tally::default());
    let behind = behind_of(fields, &tally)?;
    let stop = Stop::read(fields)?;
    let counts = Rc::new(Counts::default());
    counts.timed.set(fields.bool_or("timed", false)?);
    let served: Pin<Box<dyn Future<Output = Result<(), NetworkError>>>> = match behind {
        Behind::Listener(fetch, card) => {
            serve_ip_with(options, listener_of(fetch, card)?, Rc::clone(&counts))
        }
        Behind::Responder(responder) => {
            serve_ip_with(options, Echo::new(responder, None), Rc::clone(&counts))
        }
    };
    let (stopped, time_ms) = run_until(
        async move { served.await.map_err(|error| network_error_name(&error)) },
        stop,
    )?;
    let stats = counts.stats.get();
    let mut payload = JsObject::new();
    payload.insert("stopped", stopped);
    payload.insert("time_ms", time_ms);
    payload.insert("opened", counts.opened.get());
    payload.insert("answered", counts.answered.get());
    payload.insert("stats", stats_json(&stats));
    payload.insert("driver", driver_json(&counts.driver.get()));
    payload.insert("pumps", counts.pumps.get());
    if counts.timed.get() {
        payload.insert("pump_ns", counts.pump_ns.get());
        payload.insert("socket_ns", counts.socket_ns.get());
        payload.insert("engine_ns", counts.engine_ns());
        payload.insert("packets", stats.sent.saturating_add(stats.received));
    }
    payload.insert("harness", tally_json(&tally));
    Ok(payload)
}

/// Returns the answer of 500 with no body, which the app of the loop
/// `driver` gives where the Yoga instance throws.
fn error_response() -> Response {
    Response::new(FetchBody::Empty, Some(ResponseInit::status(500)))
        .unwrap_or_else(|_| Response::from_text(String::new()))
}

/// The schema text of the app of the loop `driver`, which no request reads,
/// since the app routes `POST /graphql` to the Yoga instance itself.
const DRIVER_SCHEMA: &str = "type Query { ok: Boolean }";

/// Serves in the loop `driver`: an app over the Yoga instance, booted
/// with the HTTP(TCP) listener on `http_port` and the driver of the crate,
/// which reads the environment of the process as a served card does.
fn serve_driver(fields: &Fields<'_>) -> Result<JsObject, JsString> {
    let name = fields.opt_str("instance")?.unwrap_or("prod");
    let yoga = instance_of(fields, name)?;
    let http_port: u16 = fields.int_or("http_port", DEFAULT_PORT)?;
    let stop = Stop::read(fields)?;
    let mut app = App::default();
    app.post("/graphql", move |c| {
        let yoga = Rc::clone(&yoga);
        async move {
            let context = ServerContext {
                env: Env::Process,
                execution_ctx: None,
            };
            Ok(answer(&yoga, c.req().raw(), context.into_context())
                .await
                .unwrap_or_else(|_| error_response()))
        }
    });
    app.get(
        "/",
        |c| async move { c.text("aosabi-devtool homa-ip driver") },
    );
    let (booted, _) = BootedApp::new(app);
    let booted = Rc::new(booted);
    let build = BuildOutput {
        schema: DRIVER_SCHEMA,
        resolve_types: &[],
        name: "aosabi-devtool",
    };
    let homa = aosabi_carrier_network::driver(&build)
        .map_err(|error| JsString::from(format!("driver: {error}")))?;
    let started = homa.is_some();
    let (stopped, time_ms) = run_until(
        async move {
            // The listener of TCP adds its socket to the reactor of the
            // runtime, so it binds inside the runtime.
            let tcp =
                TcpListen::bind(http_port, None).map_err(|error| format!("http_port: {error}"))?;
            spawn_local(Box::new(tcp).run(Rc::clone(&booted)));
            if let Some(homa) = homa {
                spawn_local(homa.run(booted));
            }
            core::future::pending::<()>().await;
            Ok(())
        },
        stop,
    )?;
    let mut payload = JsObject::with_capacity(4);
    payload.insert("stopped", stopped);
    payload.insert("time_ms", time_ms);
    payload.insert("homa_driver", started);
    payload.insert("http_port", u64::from(http_port));
    Ok(payload)
}

// The client.

/// The client endpoint of a load: the full form over owned storage.
type ClientEndpoint = Endpoint<SocketAddr, Owned<SocketAddr>, Full>;

/// Returns the client endpoint of a request: `client_ports` ports from
/// `client_port`, `window` slots, and pages of `page_len` bytes enough for
/// `window` messages of `max_len` bytes each way.
fn client_of(
    fields: &Fields<'_>,
    options: &HomaNetwork,
    window: u32,
    max_len: u32,
) -> Result<ClientEndpoint, JsString> {
    let seeded = seed().map_err(|error| JsString::from(format!("seed: {error}")))?;
    let page_len: u32 = fields.int_or("page_len", aosabi_carrier_network::PAGE_LEN)?;
    let per_message = max_len.div_ceil(page_len.max(1)).max(1);
    let slots = window.max(1);
    let pages = slots
        .saturating_mul(per_message)
        .saturating_mul(2)
        .clamp(16, homa::MAX_POOL);
    let config = Config {
        peers: 64,
        client_slots: slots,
        server_slots: 1,
        page_len,
        pages: pages.min(slots.saturating_add(1).saturating_mul(homa::MAX_PAGES)),
        max_gaps: 16,
        size_buckets: 0,
        first_id: seeded.first_id,
        hash_key: seeded.hash_key,
        port: options.config.port,
        extra_ports: options.config.extra_ports,
    };
    let mut endpoint = ClientEndpoint::new(
        config,
        aosabi_carrier_network::profile(options),
        Owned::new(&config),
    )
    .map_err(|error| JsString::from(format!("config: {}", config_error_name(error))))?;
    let share = aosabi_carrier_network::link_share(options);
    endpoint.set_link_rate(share);
    endpoint.set_pacer(Some(homa::pacer::Rate::new(
        share,
        aosabi_carrier_network::PACER_BACKLOG_NS,
        aosabi_carrier_network::PACER_DEFER_MIN,
    )));
    Ok(endpoint)
}

/// The requests of a load.
enum Requests {
    /// A list answered one by one, whose answers the load keeps.
    Messages(Vec<Vec<u8>>),
    /// The same message again.
    Repeat(Vec<u8>, u64),
    /// Seeded bytes of lengths from a list or a range.
    Random {
        /// How many.
        count: u64,
        /// The lengths taken in turn, or none.
        sizes: Vec<usize>,
        /// The shortest length of the range.
        min: usize,
        /// The longest length of the range.
        max: usize,
        /// The generator of the lengths and the bytes.
        rng: Rng,
    },
    /// Seeded mutations of a list of messages, the hostile messages of a
    /// flood of the listener.
    Mutate {
        /// How many.
        count: u64,
        /// The messages each mutation starts from.
        seeds: Vec<Vec<u8>>,
        /// The generator of the picks and the edits.
        rng: Rng,
    },
}

/// The edits a mutation applies at most to its message.
const MAX_EDITS: u64 = 3;

/// The bytes an edit appends at most.
const MAX_APPEND: u64 = 64;

/// Returns the longest message a mutation of `seeds` can give.
fn mutated_cap(seeds: &[Vec<u8>]) -> usize {
    let longest = seeds.iter().map(Vec::len).max().unwrap_or(0);
    longest
        .saturating_mul(2)
        .saturating_add(usize::try_from(MAX_EDITS * MAX_APPEND).unwrap_or(0))
}

/// Applies one seeded edit to `message`: a byte flipped, a byte set to 0 or
/// 255, the message cut short, drawn bytes appended, four bytes overwritten
/// with drawn ones, or its tail repeated, the last held to `cap` bytes.
fn mutate(message: &mut Vec<u8>, rng: &mut Rng, cap: usize) {
    let len = u64::try_from(message.len()).unwrap_or(0);
    let at = usize::try_from(rng.below(len.max(1))).unwrap_or(0);
    match rng.below(6) {
        0 => {
            if let Some(byte) = message.get_mut(at) {
                *byte ^= u8::try_from(rng.below(255) + 1).unwrap_or(1);
            }
        }
        1 => {
            if let Some(byte) = message.get_mut(at) {
                *byte = if rng.below(2) == 0 { 0 } else { u8::MAX };
            }
        }
        2 => message.truncate(at),
        3 => {
            let more = rng.below(MAX_APPEND) + 1;
            message.extend((0..more).map(|_| rng.next_u64().to_le_bytes()[0]));
        }
        4 => {
            for byte in message.iter_mut().skip(at).take(4) {
                *byte = rng.next_u64().to_le_bytes()[0];
            }
        }
        _ => {
            let tail = message.get(at..).map(<[u8]>::to_vec).unwrap_or_default();
            message.extend_from_slice(&tail);
        }
    }
    message.truncate(cap);
}

impl Requests {
    /// Reads the requests of a load.
    fn read(fields: &Fields<'_>) -> Result<Self, JsString> {
        if fields.opt_object("repeat")?.is_some() {
            let spec = fields.nested("repeat")?;
            return Ok(Self::Repeat(spec.hex("hex")?, spec.int("count")?));
        }
        if fields.opt_object("random")?.is_some() {
            let spec = fields.nested("random")?;
            let sizes = match spec.get("sizes") {
                Some(JsValue::Array(values)) => values
                    .iter()
                    .map(|value| {
                        crate::homa_packet::whole(value).and_then(|n| usize::try_from(n).ok())
                    })
                    .collect::<Option<Vec<_>>>()
                    .ok_or_else(|| spec.refuse("sizes", "holds a value that is no length"))?,
                _ => Vec::new(),
            };
            return Ok(Self::Random {
                count: spec.int("count")?,
                sizes,
                min: spec.int_or("min", 0)?,
                max: spec.int_or("max", 1_000)?,
                rng: Rng::new(spec.int_or("seed", 1)?).stream("homa-ip random"),
            });
        }
        if fields.opt_object("mutate")?.is_some() {
            let spec = fields.nested("mutate")?;
            let seeds = spec
                .array("seeds")?
                .iter()
                .map(|value| match value {
                    JsValue::String(text) => text.as_str().and_then(unhex),
                    _ => None,
                })
                .collect::<Option<Vec<_>>>()
                .filter(|seeds| !seeds.is_empty())
                .ok_or_else(|| {
                    spec.refuse("seeds", "holds no message or a value that is no hex")
                })?;
            return Ok(Self::Mutate {
                count: spec.int("count")?,
                seeds,
                rng: Rng::new(spec.int_or("seed", 1)?).stream("homa-ip mutate"),
            });
        }
        let messages = fields
            .array("messages")?
            .iter()
            .map(|value| match value {
                JsValue::String(text) => text.as_str().and_then(unhex),
                _ => None,
            })
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| fields.refuse("messages", "holds a value that is no hex"))?;
        Ok(Self::Messages(messages))
    }

    /// Returns how many requests the load sends.
    fn len(&self) -> u64 {
        match self {
            Self::Messages(messages) => count(messages.len()),
            Self::Repeat(_, n) | Self::Random { count: n, .. } | Self::Mutate { count: n, .. } => {
                *n
            }
        }
    }

    /// Returns the longest message of the load.
    fn max_len(&self) -> usize {
        match self {
            Self::Messages(messages) => messages.iter().map(Vec::len).max().unwrap_or(0),
            Self::Repeat(message, _) => message.len(),
            Self::Random { sizes, max, .. } => {
                sizes.iter().copied().max().unwrap_or(*max).max(*max)
            }
            Self::Mutate { seeds, .. } => mutated_cap(seeds),
        }
    }

    /// Returns the request of index `n`.
    fn get(&mut self, n: u64) -> Vec<u8> {
        match self {
            Self::Messages(messages) => usize::try_from(n)
                .ok()
                .and_then(|n| messages.get(n))
                .cloned()
                .unwrap_or_default(),
            Self::Repeat(message, _) => message.clone(),
            Self::Random {
                sizes,
                min,
                max,
                rng,
                ..
            } => {
                let len = if sizes.is_empty() {
                    let span = u64::try_from(max.saturating_sub(*min)).unwrap_or(0);
                    min.saturating_add(
                        usize::try_from(rng.below(span.saturating_add(1))).unwrap_or(0),
                    )
                } else {
                    let at = usize::try_from(n % count(sizes.len())).unwrap_or(0);
                    sizes.get(at).copied().unwrap_or(0)
                };
                (0..len).map(|_| rng.next_u64().to_le_bytes()[0]).collect()
            }
            Self::Mutate { seeds, rng, .. } => {
                let cap = mutated_cap(seeds);
                let pick = rng.below(count(seeds.len()));
                let mut message = usize::try_from(pick)
                    .ok()
                    .and_then(|at| seeds.get(at))
                    .cloned()
                    .unwrap_or_default();
                for _ in 0..=rng.below(MAX_EDITS) {
                    mutate(&mut message, rng, cap);
                }
                message
            }
        }
    }
}

/// One RPC of a load in flight.
struct InFlight {
    /// The index of the request.
    index: u64,
    /// The index of the port it went to.
    port: usize,
    /// When it was queued.
    sent: Instant,
    /// The request, kept where the answer is checked as an echo.
    request: Option<Vec<u8>>,
}

/// Reads `ports`, the Homa ports a load sends its requests to in turn, or
/// `port` alone where the request names none.
fn ports_of(fields: &Fields<'_>, port: u16) -> Result<Vec<u16>, JsString> {
    if fields.get("ports").is_none() {
        return Ok(vec![port]);
    }
    let ports = fields
        .array("ports")?
        .iter()
        .map(|value| {
            crate::homa_packet::whole(value)
                .and_then(|n| u16::try_from(n).ok())
                .filter(|&port| port != 0)
        })
        .collect::<Option<Vec<_>>>()
        .filter(|ports| !ports.is_empty())
        .ok_or_else(|| fields.refuse("ports", "holds no port or a value that is no port"))?;
    Ok(ports)
}

/// Returns the percentiles of `latencies` in nanoseconds, nearest rank.
fn percentiles(latencies: &mut [u64]) -> JsObject {
    latencies.sort_unstable();
    let n = latencies.len();
    let rank = |permille: usize| -> u64 {
        if n == 0 {
            return 0;
        }
        let at = (n * permille).div_ceil(1000).max(1) - 1;
        latencies.get(at.min(n - 1)).copied().unwrap_or(0)
    };
    let sum: u64 = latencies.iter().sum();
    let mut object = JsObject::with_capacity(7);
    object.insert("count", count(n));
    object.insert("min", latencies.first().copied().unwrap_or(0));
    object.insert("p50", rank(500));
    object.insert("p99", rank(990));
    object.insert("p999", rank(999));
    object.insert("max", latencies.last().copied().unwrap_or(0));
    object.insert("mean", if n == 0 { 0 } else { sum / count(n) });
    object
}

/// Returns an exponential gap with a mean of `mean_ns`, drawn from `rng`.
fn exponential_ns(rng: &mut Rng, mean_ns: u64) -> u64 {
    // A uniform draw of 53 bits in (0, 1], whose logarithm the gap scales.
    #[expect(
        clippy::cast_precision_loss,
        reason = "53 bits of a draw and a mean below 2^53 nanoseconds are exact in a double"
    )]
    let (draw, mean) = (
        ((rng.next_u64() >> 11) + 1) as f64 / (1u64 << 53) as f64,
        mean_ns as f64,
    );
    let gap = -draw.ln() * mean;
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a gap of positive nanoseconds below 2^63, clamped first"
    )]
    let gap = gap.clamp(0.0, 9.0e18) as u64;
    gap
}

/// Answers the operation `load`.
#[expect(
    clippy::too_many_lines,
    reason = "one client loop with its counters, kept whole so a reader sees every step"
)]
fn op_load(request: &JsObject) -> Result<JsObject, JsString> {
    let fields = Fields::new(request, "load");
    let target: IpAddr = fields
        .str("target")?
        .parse()
        .map_err(|_| fields.refuse("target", "names no address"))?;
    let port: u16 = fields.int_or("port", DEFAULT_PORT)?;
    let ports = ports_of(&fields, port)?;
    let options = network_of(&fields, "client_port", "client_extra_ports")?;
    let window: u32 = fields.int_or("window", 1)?;
    let mut requests = Requests::read(&fields)?;
    let total = requests.len();
    let max_len =
        u32::try_from(requests.max_len().max(fields.int_or("answer_len", 0)?)).unwrap_or(u32::MAX);
    let check_echo = fields.opt_str("check")? == Some("echo");
    let keep_answers = matches!(requests, Requests::Messages(_));
    let rpc_options = RequestOptions {
        patient: fields.bool_or("patient", false)?,
        report_unknown: fields.bool_or("report_unknown", false)?,
        source_port: 0,
    };
    let rate: u64 = fields.int_or("rate_per_s", 0)?;
    let timeout = Duration::from_millis(fields.int_or("timeout_ms", 120_000)?);
    let mut rng = Rng::new(fields.int_or("seed", 1)?).stream("homa-ip arrivals");
    let mut raw = match aosabi_carrier_network::open(&options) {
        Ok(raw) => raw,
        Err(error) => return Ok(open_refused(&error)),
    };
    let mut endpoint = client_of(&fields, &options, window, max_len)?;
    let mut rx = vec![0u8; 65_536];
    let mut flying: Vec<(RpcId, InFlight)> = Vec::new();
    let mut latencies: Vec<u64> =
        Vec::with_capacity(usize::try_from(total.min(4_000_000)).unwrap_or(0));
    let mut by_port: Vec<Vec<u64>> = ports.iter().map(|_| Vec::new()).collect();
    let mut answers: Vec<JsValue> = if keep_answers {
        (0..total).map(|_| JsValue::Null).collect()
    } else {
        Vec::new()
    };
    let mut outcomes = JsObject::new();
    let mut statuses = JsObject::new();
    let mut mismatched = 0u64;
    let mut refused = 0u64;
    let mut next = 0u64;
    let start = Instant::now();
    let mut next_at = Duration::ZERO;
    let mut last_progress = Instant::now();
    let note = |object: &mut JsObject, key: &str| {
        let value = match object.get(key) {
            Some(JsValue::Number(n)) => *n + 1.0,
            _ => 1.0,
        };
        object.insert(key, value);
    };
    while (next < total || !flying.is_empty()) && last_progress.elapsed() < timeout {
        let now_ns = since_ns(start);
        let mut moved = false;
        while next < total && count(flying.len()) < u64::from(window) {
            if rate > 0 && start.elapsed() < next_at {
                break;
            }
            let message = requests.get(next);
            moved = true;
            let at = usize::try_from(next % count(ports.len())).unwrap_or(0);
            let peer = SocketAddr::new(target, ports.get(at).copied().unwrap_or(port));
            // An open loop times a request from its arrival, so a request
            // that waits in the client for a free slot counts its wait.
            let sent = if rate > 0 {
                start + next_at
            } else {
                Instant::now()
            };
            match endpoint.request(peer, &message, rpc_options) {
                Ok(id) => flying.push((
                    id,
                    InFlight {
                        index: next,
                        port: at,
                        sent,
                        request: check_echo.then(|| message.clone()),
                    },
                )),
                Err(error) => {
                    refused += 1;
                    note(
                        &mut outcomes,
                        &format!("refused {}", send_error_name(&error)),
                    );
                }
            }
            next += 1;
            if rate > 0 {
                next_at +=
                    Duration::from_nanos(exponential_ns(&mut rng, 1_000_000_000 / rate.max(1)));
            }
        }
        if let Err(error) = endpoint.pump(&mut raw, now_ns, &mut rx) {
            return Err(JsString::from(format!("the raw socket failed: {error}")));
        }
        while let Some(event) = endpoint.poll_event() {
            let (id, failure) = match event {
                Event::Response { id } => (id, None),
                Event::Failed { id, why } => (id, Some(why)),
                _ => continue,
            };
            let Some(at) = flying.iter().position(|(rpc, _)| *rpc == id) else {
                continue;
            };
            let (_, rpc) = flying.swap_remove(at);
            moved = true;
            last_progress = Instant::now();
            match failure {
                None => {
                    let took = since_ns(rpc.sent);
                    latencies.push(took);
                    if let Some(own) = by_port.get_mut(rpc.port) {
                        own.push(took);
                    }
                    let answer = read_all(endpoint.response(id)).unwrap_or_default();
                    // The release queues the acknowledgement of the response.
                    let _ = endpoint.release(id);
                    note(&mut outcomes, "ok");
                    if let Some(head) = answer.first() {
                        note(&mut statuses, &format!("{head:02x}"));
                    }
                    if let Some(request) = rpc.request.as_ref()
                        && *request != answer
                    {
                        mismatched += 1;
                    }
                    if keep_answers
                        && let Some(slot) = usize::try_from(rpc.index)
                            .ok()
                            .and_then(|n| answers.get_mut(n))
                    {
                        *slot = JsValue::from(hex(&answer));
                    }
                }
                Some(why) => {
                    note(&mut outcomes, failure_name(why));
                    if keep_answers
                        && let Some(slot) = usize::try_from(rpc.index)
                            .ok()
                            .and_then(|n| answers.get_mut(n))
                    {
                        let mut failed = JsObject::with_capacity(1);
                        failed.insert("failed", failure_name(why));
                        *slot = JsValue::Object(failed);
                    }
                }
            }
        }
        if let Err(error) = endpoint.pump(&mut raw, since_ns(start), &mut rx) {
            return Err(JsString::from(format!("the raw socket failed: {error}")));
        }
        let now = since_ns(start);
        let wait = endpoint
            .next_timeout()
            .map_or(MAX_WAIT, |due| {
                Duration::from_nanos(due.saturating_sub(now))
            })
            .min(MAX_WAIT);
        // A pass that ended an RPC may open the next one at once, so it
        // waits on the socket only where nothing moved.
        if !raw.flush() || moved {
            continue;
        }
        // A timed wait of the socket sleeps whole ticks of the kernel, at
        // least a millisecond, so an open loop with arrivals still to come
        // spins until the next one is due.
        if rate > 0 && next < total {
            std::thread::yield_now();
            continue;
        }
        if wait > Duration::ZERO
            && let Err(error) = raw.wait(wait)
        {
            return Err(JsString::from(format!("the raw socket failed: {error}")));
        }
    }
    let time_ns = since_ns(start);
    let completed = count(latencies.len());
    let mut payload = JsObject::new();
    payload.insert("sent", next.saturating_sub(refused));
    payload.insert("refused", refused);
    payload.insert("completed", completed);
    payload.insert("unfinished", count(flying.len()));
    payload.insert("outcomes", outcomes);
    payload.insert("answer_heads", statuses);
    payload.insert("mismatched", mismatched);
    payload.insert("time_ns", time_ns);
    payload.insert("latency_ns", percentiles(&mut latencies));
    if ports.len() > 1 {
        let mut each = JsObject::with_capacity(ports.len());
        for (port, own) in ports.iter().zip(by_port.iter_mut()) {
            each.insert(port.to_string().as_str(), percentiles(own));
        }
        payload.insert("latency_by_port", each);
    }
    payload.insert("stats", stats_json(&endpoint.stats()));
    payload.insert("driver", driver_json(&raw.counts()));
    if keep_answers {
        payload.insert("answers", answers);
    }
    Ok(payload)
}

/// Answers the operation `peers`.
fn op_peers(request: &JsObject) -> Result<JsObject, JsString> {
    let fields = Fields::new(request, "peers");
    let target: IpAddr = fields
        .str("target")?
        .parse()
        .map_err(|_| fields.refuse("target", "names no address"))?;
    let port: u16 = fields.int_or("port", DEFAULT_PORT)?;
    let mut options = network_of(&fields, "client_port", "unused_extra_ports")?;
    let client_ports: u16 = fields.int_or("client_ports", 2)?;
    options.config.extra_ports = client_ports.saturating_sub(1);
    let aosabi_carrier_network::Path::Ip(path) = &mut options.path;
    path.extra_ports = client_ports.saturating_sub(1);
    let round_timeout = Duration::from_millis(fields.int_or("round_timeout_ms", 10_000)?);
    let gap = Duration::from_millis(fields.int_or("gap_ms", 0)?);
    let mut raw = match aosabi_carrier_network::open(&options) {
        Ok(raw) => raw,
        Err(error) => return Ok(open_refused(&error)),
    };
    let mut endpoint = client_of(&fields, &options, 64, 65_536)?;
    let peer = SocketAddr::new(target, port);
    let first = options.config.port;
    let start = Instant::now();
    let mut rounds_out: Vec<JsValue> = Vec::new();
    for round in fields.array("rounds")? {
        let JsValue::Array(round) = round else {
            return Err(fields.refuse("rounds", "holds a round that is no array"));
        };
        let mut flying: Vec<(RpcId, usize, Instant)> = Vec::new();
        let mut results: Vec<JsValue> = (0..round.len()).map(|_| JsValue::Null).collect();
        for (at, step) in round.iter().enumerate() {
            let JsValue::Object(step) = step else {
                return Err(fields.refuse("rounds", "holds a step that is no object"));
            };
            let step = Fields::new(step, "peers");
            let index: u16 = step.int_or("port", 0)?;
            let message = step.hex("hex")?;
            let rpc_options = RequestOptions {
                patient: step.bool_or("patient", false)?,
                report_unknown: step.bool_or("report_unknown", false)?,
                source_port: first.saturating_add(index),
            };
            match endpoint.request(peer, &message, rpc_options) {
                Ok(id) => flying.push((id, at, Instant::now())),
                Err(error) => {
                    let mut failed = JsObject::with_capacity(1);
                    failed.insert("refused", send_error_name(&error));
                    if let Some(slot) = results.get_mut(at) {
                        *slot = JsValue::Object(failed);
                    }
                }
            }
        }
        wait_round(
            &mut endpoint,
            &mut raw,
            &mut flying,
            &mut results,
            round_timeout,
            start,
        )?;
        for (id, ..) in flying {
            let _ = endpoint.abort(id);
        }
        rounds_out.push(JsValue::Array(results));
        std::thread::sleep(gap);
    }
    let mut payload = JsObject::with_capacity(4);
    payload.insert("rounds", rounds_out);
    payload.insert("client_ports", u64::from(client_ports));
    payload.insert("first_port", u64::from(first));
    payload.insert("stats", stats_json(&endpoint.stats()));
    Ok(payload)
}

/// Waits until every RPC of a round of `peers` ended or `timeout` passed, and
/// writes each answer, or the failure, into `results` at the index of its
/// step.
fn wait_round(
    endpoint: &mut ClientEndpoint,
    raw: &mut RawSocket,
    flying: &mut Vec<(RpcId, usize, Instant)>,
    results: &mut [JsValue],
    timeout: Duration,
    start: Instant,
) -> Result<(), JsString> {
    let mut rx = vec![0u8; 65_536];
    let began = Instant::now();
    while !flying.is_empty() && began.elapsed() < timeout {
        if let Err(error) = endpoint.pump(raw, since_ns(start), &mut rx) {
            return Err(JsString::from(format!("the raw socket failed: {error}")));
        }
        while let Some(event) = endpoint.poll_event() {
            let (id, failure) = match event {
                Event::Response { id } => (id, None),
                Event::Failed { id, why } => (id, Some(why)),
                _ => continue,
            };
            let Some(at) = flying.iter().position(|(rpc, ..)| *rpc == id) else {
                continue;
            };
            let (_, index, sent) = flying.swap_remove(at);
            let mut result = JsObject::with_capacity(3);
            match failure {
                None => {
                    let answer = read_all(endpoint.response(id)).unwrap_or_default();
                    let _ = endpoint.release(id);
                    result.insert("hex", hex(&answer));
                }
                Some(why) => {
                    result.insert("failed", failure_name(why));
                }
            }
            result.insert("ms", since_ns(sent) / 1_000_000);
            if let Some(slot) = results.get_mut(index) {
                *slot = JsValue::Object(result);
            }
        }
        if let Err(error) = endpoint.pump(raw, since_ns(start), &mut rx) {
            return Err(JsString::from(format!("the raw socket failed: {error}")));
        }
        let now = since_ns(start);
        let wait = endpoint
            .next_timeout()
            .map_or(MAX_WAIT, |due| {
                Duration::from_nanos(due.saturating_sub(now))
            })
            .min(MAX_WAIT);
        if raw.flush()
            && let Err(error) = raw.wait(wait)
        {
            return Err(JsString::from(format!("the raw socket failed: {error}")));
        }
    }
    Ok(())
}

/// Returns one datagram of a flood to the Homa port `port` and whether it is
/// forged, forged with a chance of `forged_permille` in a thousand and
/// counted in `kinds` by its type, malformed otherwise, or `None` for a
/// packet the form has no layout for.
fn datagram(
    rng: &mut Rng,
    port: u16,
    forged_permille: u64,
    kinds: &mut [u64; 6],
) -> Option<(bool, Vec<u8>)> {
    if rng.below(1_000) >= forged_permille {
        let len = usize::try_from(rng.below(101)).unwrap_or(0);
        let mut bytes: Vec<u8> = (0..len).map(|_| rng.next_u64().to_le_bytes()[0]).collect();
        if len >= 4 {
            bytes[2..4].copy_from_slice(&port.to_be_bytes());
        }
        if len > 11 && rng.below(3) == 0 {
            bytes[11] = u8::try_from(0x10 + rng.below(10)).unwrap_or(0x10);
        }
        return Some((false, bytes));
    }
    let src = u16::try_from(rng.below(65_535) + 1).unwrap_or(1);
    let id = rng.next_u64() & !1;
    let ports = Ports { src, dst: port };
    let kind = usize::try_from(rng.below(6)).unwrap_or(0);
    let len = u32::try_from(rng.below(2_000_000)).unwrap_or(0);
    let payload = [0x5au8; 1_400];
    let take = usize::try_from(rng.below(1_400) + 1).unwrap_or(1);
    let packet = match kind {
        0 | 1 => {
            let whole = kind == 0;
            Packet::Data(Data {
                id,
                msg_length: if whole {
                    u32::try_from(take).unwrap_or(1)
                } else {
                    len.max(2_000)
                },
                offset: if whole {
                    0
                } else {
                    u32::try_from(rng.below(1_000_000)).unwrap_or(0)
                },
                retransmit: false,
                ack: (rng.below(2) == 0).then_some(AckEntry {
                    client_id: rng.next_u64(),
                    server_port: port,
                }),
                cutoff_version: 1,
                incast: false,
                payload: payload.get(..take).unwrap_or(&[]),
            })
        }
        2 => Packet::Grant {
            id,
            offset: len,
            priority: Priority::new(u8::try_from(rng.below(8)).unwrap_or(0))
                .unwrap_or(Priority::HIGHEST),
        },
        3 => Packet::Resend {
            id,
            offset: 0,
            length: len,
            priority: Priority::HIGHEST,
        },
        4 => Packet::NeedAck { id },
        _ => Packet::StartMsg {
            id,
            msg_length: len.max(50_000),
        },
    };
    let mut header = [0u8; Full::MAX_HEADER];
    let head = Full::header(&packet, ports, false, &mut header).ok()?;
    kinds[kind] += 1;
    let mut bytes = header.get(..head)?.to_vec();
    if let Packet::Data(data) = packet {
        bytes.extend_from_slice(data.payload);
    }
    Some((true, bytes))
}

/// Answers the operation `flood`: forged and malformed datagrams from a raw
/// socket to `target`.
///
/// A forged datagram is a packet of the full form from a source port and an
/// identifier drawn from the seed: DATA of a new request, whole or a part of
/// a longer one, or a control packet of an RPC the server never saw. A
/// malformed one is a run of drawn bytes of 0 to 100 bytes, a third of them
/// with a type of the form in byte 11.
fn op_flood(request: &JsObject) -> Result<JsObject, JsString> {
    let fields = Fields::new(request, "flood");
    let target: IpAddr = fields
        .str("target")?
        .parse()
        .map_err(|_| fields.refuse("target", "names no address"))?;
    let port: u16 = fields.int_or("port", DEFAULT_PORT)?;
    let total: u64 = fields.int("count")?;
    let forged_permille: u64 = fields.int_or("forged_permille", 500)?;
    let options = network_of(&fields, "local_port", "unused_extra_ports")?;
    let mut raw = match aosabi_carrier_network::open(&options) {
        Ok(raw) => raw,
        Err(error) => return Ok(open_refused(&error)),
    };
    let mut rng = Rng::new(fields.int_or("seed", 1)?).stream("homa-ip flood");
    let peer = SocketAddr::new(target, 0);
    let mut forged = 0u64;
    let mut malformed = 0u64;
    let mut kinds = [0u64; 6];
    let start = Instant::now();
    for _ in 0..total {
        let Some((is_forged, bytes)) = datagram(&mut rng, port, forged_permille, &mut kinds) else {
            continue;
        };
        if is_forged {
            forged += 1;
        } else {
            malformed += 1;
        }
        while !raw.flush() {
            std::thread::yield_now();
        }
        if let Err(error) = raw.send(peer, Priority::HIGHEST, &bytes, &[]) {
            return Err(JsString::from(format!("the raw socket failed: {error}")));
        }
    }
    while !raw.flush() {
        std::thread::yield_now();
    }
    let mut payload = JsObject::with_capacity(6);
    payload.insert("forged", forged);
    payload.insert("malformed", malformed);
    payload.insert(
        "forged_kinds",
        kinds.iter().map(|&n| JsValue::from(n)).collect::<Vec<_>>(),
    );
    payload.insert("time_ns", since_ns(start));
    payload.insert("driver", driver_json(&raw.counts()));
    Ok(payload)
}
