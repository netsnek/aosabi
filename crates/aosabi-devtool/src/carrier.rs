// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool carrier: the primitive through which a notebook serves
 * GraphTP(Homa) on the device carriers of Linux user space, Serial Line, USB
 * and Bluetooth, and the same bodies in AoFrame alone.
 *
 * No analogue in the reference. Its judging notebooks are
 * tests/notebooks/26_carrier_serial.ipynb, 27_carrier_usb.ipynb and
 * 28_carrier_bluetooth.ipynb.
 */

//! The primitive `carrier` of the seam.
//!
//! `--requests PATH` answers a file of request lines, one response line each,
//! in the batch form of [`crate::batch`]. A request names its `carrier`:
//! `serial`, a terminal at `path`, `usb-bulk`, the `FunctionFS` mount at
//! `path`, `usb-serial`, the tty of the ACM function at `path`, `gatt`, the
//! GATT data path of the adapter `adapter`, or `channel`, the L2CAP LE credit
//! based channel.
//!
//! - `profile` answers the profile and the limits of a link: `baud` on
//!   `serial`, `speed` `high` or `full` on `usb-bulk`, `att_mtu` on `gatt`
//!   and `peer_mps` on `channel`, with `interval_ns` on both of Bluetooth,
//!   and on `gatt` the largest packet the data path sends through
//!   `bluetoothd`.
//! - `needs-zlp {len, max_packet_size, max_packet}` answers whether a
//!   transfer of `len` bytes is followed by a zero length packet.
//! - `descriptors {speed, interface}` answers the descriptors and the strings
//!   of the bulk function as `FunctionFS` takes them on `ep0`, in hex.
//! - `dbus-decode {hex}` reads the messages of the D-Bus client of the
//!   Bluetooth carrier out of the bytes of `hex`, the parser of the bytes the
//!   system bus hands the driver, and answers each message or the refusal.
//! - `serve` runs the driver of the carrier until its link fails, the peer
//!   ends it or `max_time_ms` passes. With `instance` it serves `GraphTP`
//!   through the listener of `aosabi-core` with the bounds of `card`, in
//!   front of the Yoga instance of that name of the primitive `yoga`, so
//!   every answer is the answer of the capture schema, and it answers what
//!   the driver counted. With `responder` in its place, `echo` or `sized
//!   <n>`, it runs one endpoint of the crate's engine over the carrier's link
//!   with no listener, so a notebook judges the Homa layer of the link apart
//!   from `GraphTP`.
//! - `baseline` answers whole messages in `AoFrame` alone on the carrier's
//!   link, each request the content of an OPERATION answered through the same
//!   Yoga instance with its body, first in first out, with no Homa and no
//!   head of `GraphTP`.
//! - `sizes` answers the bytes an endpoint of the driver holds, in the
//!   configuration the driver builds, on `gatt` for the one central the data
//!   path serves, and the buffers of its framing, against the buffers
//!   `AoFrame` alone holds for a whole message each way.

mod baseline;
mod bluetooth;
mod serial;
mod usb;

use core::{
    future::{
        Future,
        poll_fn, //
    },
    pin::{
        Pin,
        pin, //
    },
    task::{
        Context,
        Poll,
        Waker, //
    }, //
};
use std::{
    path::PathBuf,
    process::ExitCode,
    rc::Rc,
    time::{
        Duration,
        Instant, //
    }, //
};

use aosabi_core::graphtp::{
    Answer,
    Body,
    Fetch,
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
};
use aosabi_http::{
    Body as FetchBody,
    Env,
    Headers,
    Request as FetchRequest, //
};
use aosabi_json::{
    JsObject,
    JsString,
    JsValue, //
};
use aosabi_serve::runtime::{
    Runtime,
    hyper_rt::RuntimeTimer, //
};
use graphql_over_http::{
    HttpServer,
    ServerContext,
    answer, //
};
use homa::{
    Limits,
    Profile,
    Read as _,
    Reader,
    Stats, //
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
        hex, //
    },
    homa_sim::{
        config_error_name,
        profile_json,
        scenario::stats_json, //
    },
    seam,
    yoga::instances::Instances, //
};

/// The name of the primitive.
const NAME: &str = "carrier";

/// The stage the primitive lands at.
const LANDS_AT: &str = "H3";

/// The card a request that names none serves.
const DEFAULT_CARD: &str = "test-graphtp";

/// The polls a fetch of the baseline may take before it is given up, since
/// a resolver of the capture schemas answers at once.
const POLLS: usize = 1_000_000;

/// The command line of the `carrier` primitive.
pub static GRAMMAR: Grammar = Grammar {
    options: &[cli::SMOKE, cli::REQUESTS],
    positional: None,
    conflicts: &[],
};

/// What the `carrier` primitive accepts.
#[derive(Debug)]
pub struct CarrierArgs {
    /// Whether the call asks for the interface line.
    smoke: bool,
    /// The file of request lines of the batch form.
    requests: Option<PathBuf>,
}

impl CarrierArgs {
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
    ("profile", op_profile),
    ("needs-zlp", usb::op_needs_zlp),
    ("descriptors", usb::op_descriptors),
    ("dbus-decode", bluetooth::op_dbus_decode),
    ("serve", op_serve),
    ("baseline", op_baseline),
    ("sizes", op_sizes),
];

/// Runs the `carrier` primitive.
pub fn run(args: &CarrierArgs) -> ExitCode {
    if args.smoke {
        return seam::smoke_at(NAME, LANDS_AT);
    }
    let outcome = seam::required(args.requests.as_deref(), "--requests")
        .and_then(|path| batch::run(OPERATIONS, path));
    seam::finish(NAME, outcome)
}

/// The carriers a request names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// Serial Line on a terminal.
    Serial,
    /// USB bulk through `FunctionFS`.
    UsbBulk,
    /// USB serial on the tty of the ACM function.
    UsbSerial,
    /// The GATT data path of Bluetooth.
    Gatt,
    /// The L2CAP LE credit based channel of Bluetooth.
    Channel,
}

/// Returns the carrier a request names under `carrier`.
fn kind_of(fields: &Fields<'_>) -> Result<Kind, JsString> {
    match fields.str("carrier")? {
        "serial" => Ok(Kind::Serial),
        "usb-bulk" => Ok(Kind::UsbBulk),
        "usb-serial" => Ok(Kind::UsbSerial),
        "gatt" => Ok(Kind::Gatt),
        "channel" => Ok(Kind::Channel),
        other => Err(fields.refuse("carrier", &format!("names no carrier: {other}"))),
    }
}

/// Returns the profile of the link a request names, with the values its
/// fields give.
fn link_profile(fields: &Fields<'_>, kind: Kind) -> Result<Profile, JsString> {
    Ok(match kind {
        Kind::Serial => aosabi_carrier_serial::profile(fields.int_or("baud", 115_200)?),
        Kind::UsbBulk => aosabi_carrier_usb::bulk_profile(usb::speed_of(fields)?),
        Kind::UsbSerial => aosabi_carrier_usb::serial_profile(),
        Kind::Gatt => aosabi_carrier_bluetooth::gatt_profile(
            fields.int_or("att_mtu", aosabi_carrier_bluetooth::DEFAULT_ATT_MTU)?,
            fields.int_or("interval_ns", 0)?,
        ),
        Kind::Channel => aosabi_carrier_bluetooth::channel_profile(
            fields.int_or("peer_mps", 247)?,
            fields.int_or("interval_ns", 0)?,
        ),
    })
}

/// Answers the operation `profile`.
fn op_profile(request: &JsObject) -> Result<JsObject, JsString> {
    let fields = Fields::new(request, "profile");
    let kind = kind_of(&fields)?;
    let profile = link_profile(&fields, kind)?;
    let mut payload = JsObject::with_capacity(2);
    payload.insert("profile", profile_json(&profile));
    if kind == Kind::Channel {
        payload.insert(
            "channel_max_packet",
            u32::from(aosabi_carrier_bluetooth::channel_max_packet(
                fields.int_or("peer_mps", 247)?,
            )),
        );
    }
    if kind == Kind::Gatt {
        payload.insert(
            "sent_packet",
            u32::from(aosabi_carrier_bluetooth::sent_packet(
                fields.int_or("att_mtu", aosabi_carrier_bluetooth::DEFAULT_ATT_MTU)?,
            )),
        );
    }
    Ok(payload)
}

/// Answers the operation `serve`.
fn op_serve(request: &JsObject) -> Result<JsObject, JsString> {
    let fields = Fields::new(request, "serve");
    match kind_of(&fields)? {
        Kind::Serial => serial::serve(&fields),
        Kind::UsbBulk => usb::serve_bulk(&fields),
        Kind::UsbSerial => usb::serve_serial(&fields),
        Kind::Gatt => bluetooth::serve_gatt(&fields),
        Kind::Channel => Ok(bluetooth::serve_channel()),
    }
}

/// Answers the operation `baseline`.
fn op_baseline(request: &JsObject) -> Result<JsObject, JsString> {
    let fields = Fields::new(request, "baseline");
    match kind_of(&fields)? {
        Kind::Serial => serial::baseline(&fields),
        Kind::UsbBulk => usb::baseline_bulk(&fields),
        Kind::UsbSerial => usb::baseline_serial(&fields),
        Kind::Gatt => bluetooth::baseline_gatt(&fields),
        Kind::Channel => Ok(bluetooth::serve_channel()),
    }
}

/// Answers the operation `sizes`.
///
/// The endpoint of a driver holds the storage of its configuration, which
/// the size of a card's build counts without the priority state of a peer,
/// and the framing of a byte stream holds a receive buffer of one frame and
/// a transmit buffer of four. `AoFrame` alone holds a whole message each way,
/// the receive buffer of one frame of `max_message` bytes and the transmit
/// buffer of one.
fn op_sizes(request: &JsObject) -> Result<JsObject, JsString> {
    let fields = Fields::new(request, "sizes");
    let kind = kind_of(&fields)?;
    let profile = link_profile(&fields, kind)?;
    let card = card_of(&fields)?;
    let (first_id, hash_key) = (2, [0x5a; 16]);
    // Each link takes the configuration its driver builds, so the storage is
    // the one the driver allocates, the GATT data path's for its one central.
    let (config, max_message) = match kind {
        Kind::Serial => aosabi_carrier_serial::line_config(
            card,
            aosabi_carrier_serial::Seed { first_id, hash_key },
        ),
        Kind::UsbBulk | Kind::UsbSerial => {
            aosabi_carrier_usb::link_config(card, aosabi_carrier_usb::Seed { first_id, hash_key })
        }
        Kind::Gatt => aosabi_carrier_bluetooth::peers_config(
            card,
            aosabi_carrier_bluetooth::Seed { first_id, hash_key },
            aosabi_carrier_bluetooth::CENTRALS,
        ),
        Kind::Channel => aosabi_carrier_bluetooth::link_config(
            card,
            aosabi_carrier_bluetooth::Seed { first_id, hash_key },
        ),
    };
    let mut len = config.storage();
    len.peer_prio = 0;
    let (framing, alone) = baseline::buffers(kind, profile.max_packet, max_message);
    let mut payload = JsObject::with_capacity(5);
    payload.insert("max_message", max_message);
    payload.insert("storage_bytes", count(len.bytes::<u32>()));
    payload.insert("framing_bytes", count(framing));
    payload.insert("resident_bytes", count(len.bytes::<u32>() + framing));
    payload.insert("aoframe_alone_bytes", count(alone));
    Ok(payload)
}

/// Returns the bounds of the card a request names under `card`.
fn card_of(fields: &Fields<'_>) -> Result<&'static CardLimits, JsString> {
    let name = fields.opt_str("card")?.unwrap_or(DEFAULT_CARD);
    limits::card(name).ok_or_else(|| fields.refuse("card", &format!("names no card: {name}")))
}

/// What a request names behind the driver: a Yoga instance behind the
/// listener, or a responder of bytes with no listener.
enum Behind {
    /// The Yoga instance of the given name behind the `GraphTP` listener.
    Instance(Rc<HttpServer>),
    /// A responder of bytes over the Homa layer alone.
    Responder(Responder),
}

/// A responder of bytes over the Homa layer alone.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Responder {
    /// The request's own bytes.
    Echo,
    /// The given number of bytes, a pattern of the byte values in turn.
    Sized(usize),
}

impl Responder {
    /// Returns the answer to `request`.
    pub(crate) fn answer(self, request: &[u8]) -> Vec<u8> {
        match self {
            Self::Echo => request.to_vec(),
            Self::Sized(len) => (0..len).map(|n| n.to_le_bytes()[0]).collect(),
        }
    }
}

/// Returns what a request names behind the driver: `instance`, `prod` where
/// the request names neither, or `responder`, `echo` or `sized <n>`.
fn behind_of(fields: &Fields<'_>) -> Result<Behind, JsString> {
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
    Ok(Behind::Instance(instance_of(fields)?))
}

/// Returns the Yoga instance a request names under `instance`, `prod` where
/// it names none.
fn instance_of(fields: &Fields<'_>) -> Result<Rc<HttpServer>, JsString> {
    let name = fields.opt_str("instance")?.unwrap_or("prod");
    let instances = Instances::new().map_err(JsString::from)?;
    instances
        .get(name)
        .cloned()
        .ok_or_else(|| fields.refuse("instance", &format!("names no instance: {name}")))
}

/// Returns the fetch function of the listener over the Yoga instance `yoga`,
/// the POST of the fixed fields of `docs/graphtp-in-aosabi.md` section 2.1.
fn yoga_fetch(yoga: Rc<HttpServer>) -> Fetch {
    Rc::new(move |request: Request| -> Reply {
        let yoga = Rc::clone(&yoga);
        Box::pin(async move { yoga_answer(&yoga, request).await })
    })
}

/// Sends one request through a Yoga instance and reads its answer whole.
async fn yoga_answer(yoga: &Rc<HttpServer>, request: Request) -> Answer {
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
    let Ok(response) = answer(yoga, fetch, context.into_context()).await else {
        return plain(500, 0, None, Vec::new());
    };
    let content_type = response.headers().get("content-type");
    match response.bytes().await {
        Ok(bytes) => plain(
            response.status(),
            response.substatus(),
            content_type,
            bytes.to_vec(),
        ),
        Err(_) => plain(500, 0, None, Vec::new()),
    }
}

/// Returns an answer with a body written whole.
fn plain(status: u16, substatus: u8, content_type: Option<String>, body: Vec<u8>) -> Answer {
    Answer {
        status,
        substatus,
        content_type: content_type.map(String::into_bytes),
        body: Body::Full(body),
    }
}

/// Answers `content` through `fetch` with no runtime, polling the answer
/// until it settles, and returns its body, which the baseline sends.
fn settle_body(fetch: &Fetch, content: &[u8]) -> Result<Vec<u8>, String> {
    let request = Request {
        credential: None,
        content: content.to_vec(),
    };
    let mut reply = pin!(fetch(request));
    let mut cx = Context::from_waker(Waker::noop());
    for _ in 0..POLLS {
        if let Poll::Ready(answer) = reply.as_mut().poll(&mut cx) {
            return match answer.body {
                Body::Full(bytes) => Ok(bytes),
                Body::Stream(_) => Err(String::from("the baseline answers no stream")),
            };
        }
    }
    Err(String::from("the fetch did not settle"))
}

/// Why a served link stopped.
enum Stopped {
    /// The driver ended, with the error of its link where it failed.
    Link(Option<String>),
    /// The time of the request passed.
    MaxTime,
    /// The condition the request named held.
    Until,
}

/// How often a served link looks whether its caller wants it to stop.
const UNTIL_EVERY: Duration = Duration::from_millis(50);

/// Runs `served` on a runtime of its own until it ends, `until` holds or
/// `max_time` passes, and returns why it stopped and the milliseconds it ran.
fn run_served_until<F, U>(served: F, max_time: Duration, until: U) -> Result<(Stopped, u64), String>
where
    F: Future<Output = std::io::Result<()>>,
    U: Fn() -> bool,
{
    let runtime = Runtime::new().map_err(|error| format!("runtime: {error}"))?;
    let start = Instant::now();
    let stopped = runtime.block_on(async {
        let timer = match RuntimeTimer::current() {
            Ok(timer) => timer,
            Err(error) => return Stopped::Link(Some(error.to_string())),
        };
        let mut sleep = timer.sleep_for(max_time);
        let mut tick = timer.sleep_for(UNTIL_EVERY);
        let mut served = pin!(served);
        poll_fn(|cx| {
            if let Poll::Ready(result) = served.as_mut().poll(cx) {
                return Poll::Ready(Stopped::Link(result.err().map(|error| error.to_string())));
            }
            if Pin::new(&mut sleep).poll(cx).is_ready() {
                return Poll::Ready(Stopped::MaxTime);
            }
            while Pin::new(&mut tick).poll(cx).is_ready() {
                if until() {
                    return Poll::Ready(Stopped::Until);
                }
                tick = timer.sleep_for(UNTIL_EVERY);
            }
            Poll::Pending
        })
        .await
    });
    let elapsed = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
    Ok((stopped, elapsed))
}

/// Writes why a served link stopped into `payload`.
fn stopped_json(payload: &mut JsObject, stopped: &Stopped, time_ms: u64) {
    match stopped {
        Stopped::Link(error) => {
            payload.insert("stopped", "link");
            payload.insert("error", error.clone().map_or(JsValue::Null, JsValue::from));
        }
        Stopped::MaxTime => {
            payload.insert("stopped", "max_time");
            payload.insert("error", JsValue::Null);
        }
        Stopped::Until => {
            payload.insert("stopped", "until");
            payload.insert("error", JsValue::Null);
        }
    }
    payload.insert("time_ms", time_ms);
}

/// Returns the counters of an endpoint as an object.
fn endpoint_stats(stats: &Stats) -> JsObject {
    stats_json(stats)
}

/// Reads a whole message of the engine.
pub(crate) fn read_message(reader: Option<Reader<'_>>) -> Vec<u8> {
    let Some(reader) = reader else {
        return Vec::new();
    };
    let mut bytes = vec![0; reader.len() as usize];
    let read = reader.read_at(0, &mut bytes);
    bytes.truncate(read);
    bytes
}

/// Returns the limits of an end from the object under `key`, each absent
/// field taking the value of `defaults`.
fn limits_of(fields: &Fields<'_>, key: &str, defaults: Limits) -> Result<Limits, JsString> {
    if fields.opt_object(key)?.is_none() {
        return Ok(defaults);
    }
    let spec = fields.nested(key)?;
    Ok(Limits {
        max_packet: spec.int_or("max_packet", defaults.max_packet)?,
        max_message: spec.int_or("max_message", defaults.max_message)?,
        unsched_bytes: spec.int_or("unsched_bytes", defaults.unsched_bytes)?,
        max_rpcs: spec.int_or("max_rpcs", defaults.max_rpcs)?,
    })
}

/// Returns the name of a configuration error of an endpoint as the text of a
/// refusal.
fn config_refused(error: homa::ConfigError) -> JsString {
    JsString::from(format!("config: {}", config_error_name(error)))
}

/// Returns the bytes of `bytes` as lowercase hex.
fn hex_of(bytes: &[u8]) -> String {
    hex(bytes)
}

/// An endpoint of the Homa layer alone over a carrier's link.
type EchoEndpoint<P> = homa::Endpoint<P, homa::Owned<P>, homa::wire::Compact>;

/// What the Homa layer of a link counted while a responder answered it.
#[derive(Debug, Default)]
struct EchoCounts {
    /// The whole requests that arrived.
    requests: u64,
    /// The answers the endpoint took.
    answered: u64,
    /// The names of the answers the endpoint refused.
    refused: Vec<String>,
    /// The links that went up.
    ups: u64,
    /// The links that went down.
    downs: u64,
}

impl EchoCounts {
    /// Returns the counts as an object.
    fn json(&self) -> JsObject {
        let mut object = JsObject::with_capacity(5);
        object.insert("requests", self.requests);
        object.insert("answered", self.answered);
        object.insert(
            "refused",
            self.refused
                .iter()
                .map(|name| JsValue::from(name.as_str()))
                .collect::<Vec<_>>(),
        );
        object.insert("ups", self.ups);
        object.insert("downs", self.downs);
        object
    }
}

/// What a pass of the link of an echo run asks of the loop.
enum LinkTurn {
    /// The link runs on.
    Running,
    /// The link ended with no error, the peer closed it.
    Ended,
}

/// Serves the Homa layer of `link` with `responder` until the link fails or
/// ends or `max_time` passes, and returns why it stopped and what it
/// counted.
///
/// Each pass hands `link` and the endpoint to `events` first, which takes
/// the links of the carrier up and down, then pumps the endpoint, answers
/// every whole request, and pumps again, and a pass that moves nothing
/// sleeps until the next deadline of the endpoint, at most `idle`.
fn echo_run<P, C, E>(
    link: &mut C,
    endpoint: &mut EchoEndpoint<P>,
    responder: Responder,
    max_time: Duration,
    idle: Duration,
    mut events: E,
) -> (Stopped, EchoCounts, u64)
where
    P: Copy + Eq + core::hash::Hash + homa::PeerPort,
    C: homa::Carrier<Peer = P>,
    C::Error: core::fmt::Display,
    E: FnMut(&mut C, &mut EchoEndpoint<P>, &mut EchoCounts) -> Result<LinkTurn, String>,
{
    let start = Instant::now();
    let mut counts = EchoCounts::default();
    let mut rx = vec![0u8; 1 << 17];
    let since = |start: Instant| u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX);
    let stopped = loop {
        match events(link, endpoint, &mut counts) {
            Ok(LinkTurn::Running) => {}
            Ok(LinkTurn::Ended) => break Stopped::Link(None),
            Err(error) => break Stopped::Link(Some(error)),
        }
        let before = endpoint.stats();
        if let Err(error) = endpoint.pump(link, since(start), &mut rx) {
            break Stopped::Link(Some(error.to_string()));
        }
        let mut moved = false;
        while let Some(event) = endpoint.poll_event() {
            moved = true;
            if let homa::Event::Request { id, .. } = event {
                counts.requests += 1;
                let request = read_message(endpoint.request_of(id));
                match endpoint.reply(id, &responder.answer(&request)) {
                    Ok(()) => counts.answered += 1,
                    Err(error) => counts.refused.push(format!("{error:?}")),
                }
            }
        }
        if let Err(error) = endpoint.pump(link, since(start), &mut rx) {
            break Stopped::Link(Some(error.to_string()));
        }
        let after = endpoint.stats();
        moved |= after.sent != before.sent || after.received != before.received;
        if start.elapsed() > max_time {
            break Stopped::MaxTime;
        }
        if !moved {
            let now = since(start);
            let wait = endpoint.next_timeout().map_or(idle, |due| {
                Duration::from_nanos(due.saturating_sub(now)).min(idle)
            });
            std::thread::sleep(wait);
        }
    };
    let elapsed = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
    (stopped, counts, elapsed)
}
