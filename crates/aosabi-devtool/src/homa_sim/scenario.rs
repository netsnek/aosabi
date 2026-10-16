// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool homa_sim scenario: the scenario a homa-sim request names,
 * read from its JSON, and the run of it over a simulated link.
 *
 * No analogue in the reference.
 */

//! The scenarios of the `homa-sim` primitive.
//!
//! A scenario puts end A, the client, and end B, the server, on a `loopback`,
//! a `lossy` link or the `baseline` of `AoFrame` alone, on a real link of
//! this process on Linux, `seqpacket`, `stream` or `pty`, which
//! [`super::socket`] and [`super::pty`] name, or on the `radio` model of
//! `LoRa`, where each end is the carrier of `aosabi-carrier-lora` over one
//! radio of `homa::sim::RadioNet` and [`RadioSpec`] says which fields the
//! object `radio` takes, and runs its `rpcs` in order.
//! Each RPC is a request of `len` bytes that A sends at `at_ns` or as soon as
//! its window allows, and B answers each request with `response_len`
//! bytes, through `reply_reserve` where the RPC sets `reserve` and through
//! `reply` otherwise. The bytes of a message are its `fill`: `{"seed": n}`
//! takes the SHA-256 of the seed and a block counter, both eight bytes little
//! endian, block after block, and `{"hex": ...}` repeats the bytes it spells.
//! An RPC that sets `lose` has every DATA packet of its request lost on the
//! way out of A until the request has left whole once or B has answered it
//! with `RPC_UNKNOWN`, and one that names `end_at_ns` is ended by A at that
//! time through `end_by`, `abort`, `release`, `rpcs_abort` or `rpcs_release`.
//!
//! A scenario that names a `frame`, `delimited` or `prefixed`, counts the
//! bytes of each packet's `AoFrame` in that form among its link bytes and
//! times the packet on the wire by them, as a byte stream carries it.
//!
//! A scenario runs both ends on the compact form unless it names the `form`
//! `full`, the form of `homa_wire.h`. Its `source_ports` hand the RPCs the
//! client's Homa ports in turn, each RPC that names no `source_port` of its
//! own taking the next.
//!
//! A scenario that names `restart_b_at_ns` restarts B at that time: its
//! endpoint is built again with a first identifier of its own and brought up
//! on the link, every request it held is forgotten, and a link that keeps
//! state of its own restarts B's side of it, which on the `radio` is the
//! carrier of a card that restarts.
//!
//! The run hands A and B every packet the link delivers, runs their timers,
//! and moves the clock to the earliest of the link's next time, both
//! deadlines, the next RPC to send and the next action a scenario names, so
//! one seed replays one run. It stops once every RPC has ended or the clock
//! passes `max_time_ns`.
//!
//! What a run answers per RPC is what the ends reported: the times, the
//! outcome, the SHA-256 of the request B read and of the response A read, the
//! deliveries on each side, the restarts after `RPC_UNKNOWN` and whether a
//! DATA packet of the answer reached A before one, and the reservations B
//! dropped. The notebooks judge it against what they sent. A `trace` names a
//! file, and each line of it is one JSON array: `T` a DATA packet with the
//! messages that could have gone, `G` a GRANT, `S` a RESEND, `R` a DATA packet
//! an end took, `D` a whole message, `F` a failed RPC, `U` an `RPC_UNKNOWN` a
//! client took, `X` a packet the scenario dropped, `M` a `START_MSG` an end
//! took and `P` a decision of an end's grant scheduler.
//!
//! The operations `fabric`, `ports` and `table` go beyond the two ends of a
//! scenario, and [`op_fabric`], [`op_ports`] and [`op_table`] name the
//! fields each takes. The trace of a fabric run names the host of a row
//! in place of its end, adds the other host of the packet or decision, and
//! writes an `O` row for every packet the switch dropped.

use std::{
    cell::RefCell,
    collections::VecDeque,
    convert::Infallible,
    fs::File,
    io::{
        BufWriter,
        Write as _, //
    },
    net::{
        IpAddr,
        Ipv4Addr,
        SocketAddr, //
    },
    path::PathBuf,
    rc::Rc, //
};

use aosabi_carrier_lora::{
    self as lora,
    Direction,
    Lora,
    LoraError,
    ModelError,
    Pairing, //
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
    Failure,
    Gone,
    Limits,
    LinkError,
    Nanos,
    Owned,
    Priority,
    Profile,
    Read,
    RequestOptions,
    RpcId,
    Rpcs,
    SendError,
    Stats,
    sim::{
        Baseline,
        Fabric,
        FabricNet,
        FabricStats,
        Faults,
        Injected,
        LinkEnd,
        LinkModel,
        Loopback,
        Lossy,
        Radio,
        RadioNet,
        Rng,
        Side,
        Trace,
        TraceEvent, //
    },
    stream::{
        self,
        Mode, //
    },
    wire::{
        Compact,
        Data,
        Form,
        Full,
        Packet, //
    }, //
};

use super::{
    DEFAULT_CONFIG,
    config_error_name,
    config_of,
    profile_json,
    request_profile, //
};
#[cfg(target_os = "linux")]
use super::{
    pty,
    socket, //
};
use crate::{
    homa_packet::{
        Fields,
        WireForm,
        check_form,
        count,
        hex,
        unhex, //
    },
    lora::{
        ModelRadio,
        SharedStore,
        band_of,
        pairing_of,
        stats_json as lora_stats_json, //
    },
    provenance::sha256::Sha256, //
};

/// An end of a run over the wire form `W`.
type Engine<W> = Endpoint<Side, Owned<Side>, W>;

/// The two ends, A first.
const SIDES: [Side; 2] = [Side::A, Side::B];

/// The most passes at one time before a run gives up, so an end that keeps
/// moving without the clock moving ends the run with a report.
const MAX_PASSES: u32 = 100_000;

/// Returns the index of an end, 0 for A and 1 for B.
const fn index_of(side: Side) -> usize {
    match side {
        Side::A => 0,
        Side::B => 1,
    }
}

/// Returns the name of an end.
const fn side_name(side: Side) -> &'static str {
    match side {
        Side::A => "A",
        Side::B => "B",
    }
}

/// Returns the name of a failure, the name of its variant.
pub(crate) fn failure_name(failure: Failure) -> &'static str {
    match failure {
        Failure::LinkDown => "LinkDown",
        Failure::PeerTimeout => "PeerTimeout",
        Failure::TooLong => "TooLong",
        Failure::Aborted => "Aborted",
        Failure::RpcUnknown => "RpcUnknown",
    }
}

/// Returns the name of a refusal to queue a message, the name of its variant.
pub(crate) fn send_error_name<E>(error: &SendError<E>) -> &'static str {
    match error {
        SendError::NoSlot => "NoSlot",
        SendError::TooLong => "TooLong",
        SendError::Full => "Full",
        SendError::NoPeer => "NoPeer",
        SendError::NoRequest => "NoRequest",
        SendError::Port => "Port",
        SendError::Write(_) => "Write",
    }
}

/// Returns the name of a failed link operation, the name of its variant.
pub(crate) fn link_error_name(error: LinkError) -> &'static str {
    match error {
        LinkError::NoPeerSlot => "NoPeerSlot",
        LinkError::Up => "Up",
        LinkError::NoPeer => "NoPeer",
        LinkError::Params => "Params",
    }
}

/// The bytes of a message.
#[derive(Debug, Clone)]
pub(crate) enum Fill {
    /// The SHA-256 of the seed and a block counter, block after block.
    Seed(u64),
    /// The bytes repeated to the length of the message.
    Pattern(Vec<u8>),
}

impl Fill {
    /// Reads a fill from the object under `key`, or answers `default`.
    pub(crate) fn read(fields: &Fields<'_>, key: &str, default: u64) -> Result<Self, JsString> {
        let Some(_) = fields.opt_object(key)? else {
            return Ok(Self::Seed(default));
        };
        let spec = fields.nested(key)?;
        if let Some(text) = spec.opt_str("hex")? {
            let bytes = unhex(text).ok_or_else(|| spec.refuse("hex", "is no hex"))?;
            return Ok(Self::Pattern(bytes));
        }
        Ok(Self::Seed(spec.u64("seed")?))
    }

    /// Returns the first `len` bytes of the fill.
    pub(crate) fn bytes(&self, len: usize) -> Vec<u8> {
        match self {
            Self::Seed(seed) => {
                let mut out = Vec::with_capacity(len);
                let mut block = 0u64;
                while out.len() < len {
                    let mut hasher = Sha256::new();
                    hasher.update(seed.to_le_bytes());
                    hasher.update(block.to_le_bytes());
                    let digest = hasher.finalize();
                    let take = (len - out.len()).min(digest.len());
                    out.extend_from_slice(&digest[..take]);
                    block += 1;
                }
                out
            }
            Self::Pattern(pattern) if pattern.is_empty() => vec![0; len],
            Self::Pattern(pattern) => pattern.iter().copied().cycle().take(len).collect(),
        }
    }
}

/// How the client ends an RPC before its answer is whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EndBy {
    /// `Endpoint::abort`.
    Abort,
    /// `Endpoint::release`.
    Release,
    /// `Rpcs::abort`.
    RpcsAbort,
    /// `Rpcs::release`.
    RpcsRelease,
}

impl EndBy {
    /// Returns the name the scenario spells.
    const fn name(self) -> &'static str {
        match self {
            Self::Abort => "abort",
            Self::Release => "release",
            Self::RpcsAbort => "rpcs_abort",
            Self::RpcsRelease => "rpcs_release",
        }
    }
}

/// One RPC of a scenario.
#[derive(Debug, Clone)]
struct Spec {
    /// The earliest time A sends the request.
    at: Nanos,
    /// Bytes of the request.
    len: usize,
    /// The bytes of the request.
    fill: Fill,
    /// Bytes of the response.
    response_len: usize,
    /// The bytes of the response.
    response_fill: Fill,
    /// How A sends the request: whether it is patient and whether it asks
    /// for `Failure::RpcUnknown` in place of a restart.
    options: RequestOptions,
    /// Whether B answers through `reply_reserve`.
    reserve: bool,
    /// The bytes B reserves.
    reserve_max: u32,
    /// Reservations B drops without a commit before it answers.
    drop_reservations: u32,
    /// The time B holds the request before it answers.
    hold: Nanos,
    /// Whether every DATA packet of the request is lost until it has left
    /// whole once or the server answered it with `RPC_UNKNOWN`.
    lose: bool,
    /// When and how A ends the RPC before its answer is whole.
    end: Option<(Nanos, EndBy)>,
}

impl Spec {
    /// Reads the RPC at `index` of a scenario whose defaults `defaults` holds.
    fn read(fields: &Fields<'_>, index: u64, defaults: &Defaults) -> Result<Self, JsString> {
        let response_len: usize = fields.int_or("response_len", 0)?;
        let end = match fields.opt_u64("end_at_ns")? {
            None => None,
            Some(time) => Some((time, end_by(fields)?)),
        };
        Ok(Self {
            at: fields.int_or("at_ns", 0)?,
            len: fields.int_or("len", 0)?,
            fill: Fill::read(fields, "fill", index.saturating_mul(2))?,
            response_len,
            response_fill: Fill::read(
                fields,
                "response_fill",
                index.saturating_mul(2).saturating_add(1),
            )?,
            options: RequestOptions {
                patient: fields.bool_or("patient", false)?,
                report_unknown: fields.bool_or("report_unknown", defaults.report_unknown)?,
                source_port: fields.int_or("source_port", 0)?,
            },
            reserve: fields.bool_or("reserve", defaults.reserve(index))?,
            reserve_max: fields.int_or(
                "reserve_max",
                u32::try_from(response_len).unwrap_or(u32::MAX),
            )?,
            drop_reservations: fields.int_or("drop_reservations", 0)?,
            hold: fields.int_or("hold_ns", 0)?,
            lose: fields.bool_or("lose", false)?,
            end,
        })
    }
}

/// Returns how an RPC ends before its answer is whole.
fn end_by(fields: &Fields<'_>) -> Result<EndBy, JsString> {
    match fields.opt_str("end_by")?.unwrap_or("abort") {
        "abort" => Ok(EndBy::Abort),
        "release" => Ok(EndBy::Release),
        "rpcs_abort" => Ok(EndBy::RpcsAbort),
        "rpcs_release" => Ok(EndBy::RpcsRelease),
        other => Err(fields.refuse("end_by", &format!("names no way to end an RPC: {other}"))),
    }
}

/// The values of a scenario every RPC takes unless it names its own.
struct Defaults {
    /// `report_unknown`.
    report_unknown: bool,
    /// `reserve`: `none`, `all` or `half`, the RPCs of odd index.
    reserve: String,
}

impl Defaults {
    /// Returns whether the RPC at `index` answers through `reply_reserve`.
    fn reserve(&self, index: u64) -> bool {
        match self.reserve.as_str() {
            "all" => true,
            "half" => index % 2 == 1,
            _ => false,
        }
    }
}

/// The link of a scenario.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LinkKind {
    /// A perfect link.
    Loopback,
    /// A link with faults.
    Lossy,
    /// Whole messages in `AoFrame` alone.
    Baseline,
    /// The two ends of a `SOCK_SEQPACKET` pair, `homa::unix::pair`.
    SeqPacket,
    /// A stream socket `homa::unix::StreamListener` accepted, in prefixed
    /// `AoFrame`.
    Stream,
    /// The two ends of a pty pair of `tests/harness/homa/pty.py`, in
    /// delimited `AoFrame`.
    Pty,
    /// Two carriers of `LoRa` over two radios of the radio model.
    Radio,
}

/// A scenario read from its request.
struct Scenario {
    /// The link.
    link: LinkKind,
    /// The wire form both ends run, the compact form unless the request
    /// names `full`.
    form: WireForm,
    /// The values of the link.
    profile: Profile,
    /// How the link moves packets.
    model: LinkModel,
    /// The configuration of A and of B.
    configs: [Config; 2],
    /// What A and B accept.
    local: [Limits; 2],
    /// What A and B take of the other end until its LIMITS arrive.
    assumed: [Limits; 2],
    /// The faults of a lossy link, and of the baseline.
    faults: Faults,
    /// The seed of the faults.
    seed: u64,
    /// RPCs A has open at most.
    window: usize,
    /// The time after which a run stops.
    max_time: Nanos,
    /// How the link frames each packet, which its link bytes count and its
    /// wire times.
    frame: Option<Mode>,
    /// The form of `AoFrame` of the baseline.
    baseline_mode: Mode,
    /// Whether the first LIMITS each end sends is lost.
    drop_first_limits: bool,
    /// The time from which B takes and sends nothing.
    silent_b_from: Option<Nanos>,
    /// Where the trace goes.
    trace: Option<PathBuf>,
    /// Where the listener of the link `stream` binds.
    #[cfg(target_os = "linux")]
    path: Option<PathBuf>,
    /// The paths of the two ends of the link `pty`, A's first.
    #[cfg(target_os = "linux")]
    ptys: Option<[PathBuf; 2]>,
    /// The send buffer of each socket of a real link, the kernel's default
    /// where the scenario names none.
    #[cfg(target_os = "linux")]
    send_buffer: Option<usize>,
    /// The radio model of the link `radio`.
    radio: Option<RadioSpec>,
    /// The time B restarts.
    restart_b_at: Option<Nanos>,
    /// The RPCs in order.
    rpcs: Vec<Spec>,
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

/// Returns the limits an end of `config` takes on a link of `profile` unless
/// the scenario names its own.
fn default_limits(profile: &Profile, config: &Config) -> Limits {
    let pages = config.page_len.saturating_mul(homa::MAX_PAGES);
    Limits {
        max_packet: profile.max_packet,
        max_message: pages.min(65_536),
        unsched_bytes: profile.unsched_bytes,
        max_rpcs: u8::try_from(config.server_slots.clamp(1, 255)).unwrap_or(u8::MAX),
    }
}

impl Scenario {
    /// Reads a scenario from its request.
    fn read(fields: &Fields<'_>) -> Result<Self, JsString> {
        let link = match fields.str("link")? {
            "loopback" => LinkKind::Loopback,
            "lossy" => LinkKind::Lossy,
            "baseline" => LinkKind::Baseline,
            "seqpacket" => LinkKind::SeqPacket,
            "stream" => LinkKind::Stream,
            "pty" => LinkKind::Pty,
            "fabric" => return Err(fields.refuse("link", "is run by read_fabric")),
            "radio" => LinkKind::Radio,
            other => return Err(fields.refuse("link", &format!("names no link: {other}"))),
        };
        let form = match fields.opt_str("form")? {
            None => WireForm::Compact,
            Some(_) => check_form(fields)?,
        };
        let mut source_ports = Vec::new();
        for value in fields.array("source_ports")? {
            let port = crate::homa_packet::whole(value)
                .and_then(|port| u16::try_from(port).ok())
                .ok_or_else(|| fields.refuse("source_ports", "holds a value that is no port"))?;
            source_ports.push(port);
        }
        let profile = request_profile(fields)?;
        let configs = Self::configs(fields)?;
        let local = [
            limits_of(fields, "limits_a", default_limits(&profile, &configs[0]))?,
            limits_of(fields, "limits_b", default_limits(&profile, &configs[1]))?,
        ];
        let assumed = [
            limits_of(fields, "assumed_a", local[1])?,
            limits_of(fields, "assumed_b", local[0])?,
        ];
        let defaults = Defaults {
            report_unknown: fields.bool_or("report_unknown", false)?,
            reserve: fields.opt_str("reserve")?.unwrap_or("none").to_owned(),
        };
        let mut rpcs = Vec::new();
        for (index, value) in fields.array("rpcs")?.iter().enumerate() {
            let JsValue::Object(object) = value else {
                return Err(fields.refuse("rpcs", "holds an entry that is no object"));
            };
            let mut spec = Spec::read(&Fields::new(object, "run"), count(index), &defaults)?;
            if spec.options.source_port == 0
                && let Some(&port) = source_ports.get(index % source_ports.len().max(1))
            {
                spec.options.source_port = port;
            }
            rpcs.push(spec);
        }
        Ok(Self {
            link,
            form,
            profile,
            model: model_of(fields)?,
            configs,
            local,
            assumed,
            faults: faults_of(fields)?,
            seed: fields.int_or("seed", 1)?,
            window: fields.int_or("window", 8)?,
            max_time: fields.int_or("max_time_ns", 3_600_000_000_000)?,
            frame: frame_of(fields, "frame")?,
            baseline_mode: frame_of(fields, "baseline_mode")?.unwrap_or(Mode::Delimited),
            drop_first_limits: fields.bool_or("drop_first_limits", false)?,
            silent_b_from: fields.opt_u64("silent_b_from_ns")?,
            trace: fields.opt_str("trace")?.map(PathBuf::from),
            #[cfg(target_os = "linux")]
            path: fields.opt_str("path")?.map(PathBuf::from),
            #[cfg(target_os = "linux")]
            ptys: match (fields.opt_str("pty_a")?, fields.opt_str("pty_b")?) {
                (Some(a), Some(b)) => Some([PathBuf::from(a), PathBuf::from(b)]),
                _ => None,
            },
            #[cfg(target_os = "linux")]
            send_buffer: fields
                .opt_u64("send_buffer")?
                .map(|bytes| usize::try_from(bytes).unwrap_or(usize::MAX)),
            radio: if link == LinkKind::Radio {
                Some(RadioSpec::read(fields)?)
            } else {
                None
            },
            restart_b_at: fields.opt_u64("restart_b_at_ns")?,
            rpcs,
        })
    }

    /// Reads the configuration of A and of B.
    fn configs(fields: &Fields<'_>) -> Result<[Config; 2], JsString> {
        let shared = match fields.opt_object("config")? {
            Some(_) => Some(fields.nested("config")?),
            None => None,
        };
        let base = config_of(shared.as_ref(), DEFAULT_CONFIG)?;
        let mut out = [base; 2];
        out[0].first_id = fields.int_or("first_id_a", 0x1000_0002)?;
        out[1].first_id = fields.int_or("first_id_b", 0x2000_0002)?;
        for (slot, key) in out.iter_mut().zip(["config_a", "config_b"]) {
            if fields.opt_object(key)?.is_some() {
                *slot = config_of(Some(&fields.nested(key)?), *slot)?;
            }
        }
        Ok(out)
    }
}

/// Reads the link model of a scenario.
fn model_of(fields: &Fields<'_>) -> Result<LinkModel, JsString> {
    let Some(_) = fields.opt_object("model")? else {
        return Ok(LinkModel {
            bytes_per_s: 0,
            packets_per_event: 1,
            event: 0,
            latency: 0,
        });
    };
    let spec = fields.nested("model")?;
    Ok(LinkModel {
        bytes_per_s: spec.int_or("bytes_per_s", 0)?,
        packets_per_event: spec.int_or("packets_per_event", 1)?,
        event: spec.int_or("event_ns", 0)?,
        latency: spec.int_or("latency_ns", 0)?,
    })
}

/// Reads the faults of a scenario.
fn faults_of(fields: &Fields<'_>) -> Result<Faults, JsString> {
    let Some(_) = fields.opt_object("faults")? else {
        return Ok(Faults::default());
    };
    let spec = fields.nested("faults")?;
    Ok(Faults {
        drop_per_million: spec.int_or("drop_per_million", 0)?,
        drop_grants_per_million: spec.int_or("drop_grants_per_million", 0)?,
        drop_limits_per_million: spec.int_or("drop_limits_per_million", 0)?,
        duplicate_per_million: spec.int_or("duplicate_per_million", 0)?,
        corrupt_per_million: spec.int_or("corrupt_per_million", 0)?,
        link_down_at: spec.opt_u64("link_down_at_ns")?,
    })
}

/// Reads a form of `AoFrame` under `key`, `None` where it is absent or
/// `none`.
fn frame_of(fields: &Fields<'_>, key: &str) -> Result<Option<Mode>, JsString> {
    match fields.opt_str(key)? {
        None | Some("none") => Ok(None),
        Some("delimited") => Ok(Some(Mode::Delimited)),
        Some("prefixed") => Ok(Some(Mode::Prefixed)),
        Some(other) => Err(fields.refuse(key, &format!("names no form of AoFrame: {other}"))),
    }
}

/// Answers the operation `run`.
pub fn op_run(request: &JsObject) -> Result<JsObject, JsString> {
    let fields = Fields::new(request, "run");
    let scenario = Scenario::read(&fields)?;
    let trace = match &scenario.trace {
        Some(path) => Some(TraceOut::create(path)?),
        None => None,
    };
    let full = scenario.form == WireForm::Full;
    let mut payload = match scenario.link {
        LinkKind::Baseline => BaselineRun::new(&scenario, trace).run(),
        LinkKind::Loopback => {
            let mut net = Loopback::new(scenario.model).framed(scenario.frame);
            if full {
                run_engine::<Full, _>(&scenario, &mut net, trace)
            } else {
                run_engine::<Compact, _>(&scenario, &mut net, trace)
            }
        }
        LinkKind::Lossy => {
            let mut net = Lossy::new(scenario.model, scenario.faults, Rng::new(scenario.seed))
                .framed(scenario.frame);
            if full {
                run_engine::<Full, _>(&scenario, &mut net, trace)
            } else {
                run_engine::<Compact, _>(&scenario, &mut net, trace)
            }
        }
        LinkKind::Radio => {
            let Some(spec) = scenario.radio.as_ref() else {
                return Err(fields.refuse("radio", "is missing, the link radio reads it"));
            };
            let mut buckets = [[0u32; lora::BUCKETS]; 2];
            let [buckets_a, buckets_b] = &mut buckets;
            let mut net = RadioLink::new(spec, scenario.seed, buckets_a, buckets_b)
                .map_err(|error| JsString::from(format!("run: link radio: {error}")))?;
            if spec.baseline {
                RadioBaselineRun::new(&scenario, spec, trace).run(&mut net)
            } else {
                run_engine::<Compact, _>(&scenario, &mut net, trace)
            }
        }
        #[cfg(not(target_os = "linux"))]
        LinkKind::SeqPacket | LinkKind::Stream | LinkKind::Pty => {
            return Err(fields.refuse("link", "runs over the sockets of Linux alone"));
        }
        #[cfg(target_os = "linux")]
        LinkKind::SeqPacket => {
            let mut net = socket::SeqNet::open(&scenario.profile, scenario.send_buffer)
                .map_err(|error| JsString::from(format!("run: link seqpacket: {error}")))?;
            run_engine::<Compact, _>(&scenario, &mut net, trace)
        }
        #[cfg(target_os = "linux")]
        LinkKind::Stream => {
            let path = scenario
                .path
                .as_deref()
                .ok_or_else(|| fields.refuse("path", "is missing, the link stream binds it"))?;
            let mut buffers = socket::StreamNet::buffers(&scenario.profile);
            let mut net = socket::StreamNet::open(path, scenario.send_buffer, &mut buffers)
                .map_err(|error| JsString::from(format!("run: link stream: {error}")))?;
            run_engine::<Compact, _>(&scenario, &mut net, trace)
        }
        #[cfg(target_os = "linux")]
        LinkKind::Pty => {
            let paths = scenario
                .ptys
                .as_ref()
                .ok_or_else(|| fields.refuse("pty_a", "is missing, the link pty opens it"))?;
            let mut buffers = pty::PtyNet::buffers(&scenario.profile);
            let mut net = pty::PtyNet::open(paths, &mut buffers)
                .map_err(|error| JsString::from(format!("run: link pty: {error}")))?;
            run_engine::<Compact, _>(&scenario, &mut net, trace)
        }
    };
    payload.insert("profile", profile_json(&scenario.profile));
    Ok(payload)
}

/// A link between A and B, simulated or real.
pub(super) trait Net {
    /// One end as a carrier whose peer is the other end.
    type End<'a>: Carrier<Peer = Side>
    where
        Self: 'a;
    /// Returns one end as a carrier.
    fn end(&mut self, side: Side) -> Self::End<'_>;
    /// Moves every packet due by `now` and returns the next due time.
    fn step(&mut self, now: Nanos) -> Option<Nanos>;
    /// Returns what the faults injected, `None` on a perfect link.
    fn injected(&self) -> Option<Injected>;
    /// Restarts the side of the link that `side` holds, where the link keeps
    /// state of its own, as the card of that side does when it restarts.
    fn restart(&mut self, _side: Side) {}
    /// Returns what the link recorded beside the run, `None` where it
    /// records nothing.
    fn report(&self) -> Option<JsObject> {
        None
    }
}

impl Net for Loopback {
    type End<'a> = LinkEnd<'a>;

    fn end(&mut self, side: Side) -> LinkEnd<'_> {
        Loopback::end(self, side)
    }

    fn step(&mut self, now: Nanos) -> Option<Nanos> {
        Loopback::step(self, now)
    }

    fn injected(&self) -> Option<Injected> {
        None
    }
}

impl Net for Lossy {
    type End<'a> = LinkEnd<'a>;

    fn end(&mut self, side: Side) -> LinkEnd<'_> {
        Lossy::end(self, side)
    }

    fn step(&mut self, now: Nanos) -> Option<Nanos> {
        Lossy::step(self, now)
    }

    fn injected(&self) -> Option<Injected> {
        Some(Lossy::injected(self))
    }
}

/// The trace file of a run, one JSON array a line.
struct TraceOut {
    /// The file.
    out: BufWriter<File>,
    /// Where it lives, for the answer.
    path: PathBuf,
    /// The lines written.
    lines: u64,
    /// The bytes of the line being written.
    line: Vec<u8>,
}

impl TraceOut {
    /// Creates the trace file at `path`.
    fn create(path: &PathBuf) -> Result<Self, JsString> {
        let file = File::create(path).map_err(|error| {
            JsString::from(format!("run: cannot create {}: {error}", path.display()))
        })?;
        Ok(Self {
            out: BufWriter::new(file),
            path: path.clone(),
            lines: 0,
            line: Vec::new(),
        })
    }

    /// Writes one line.
    fn put(&mut self, items: Vec<JsValue>) {
        self.line.clear();
        aosabi_json::write(&mut self.line, &JsValue::Array(items));
        self.line.push(b'\n');
        // A trace that cannot be written ends in the answer's line count,
        // which the notebook compares with the file it reads.
        if self.out.write_all(&self.line).is_ok() {
            self.lines += 1;
        }
    }

    /// Flushes the file and returns what the answer says of it.
    fn finish(mut self) -> JsObject {
        let flushed = self.out.flush().is_ok();
        let mut object = JsObject::with_capacity(3);
        object.insert("path", self.path.display().to_string());
        object.insert("lines", self.lines);
        object.insert("flushed", flushed);
        object
    }
}

/// Writes one line to an optional trace.
fn trace_put(trace: &mut Option<TraceOut>, items: Vec<JsValue>) {
    if let Some(out) = trace {
        out.put(items);
    }
}

/// Where one RPC ended.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Outcome {
    /// Not ended yet, or never sent.
    Pending,
    /// The response arrived whole.
    Ok,
    /// A reported failure.
    Failed(&'static str),
    /// A refusal to queue the request.
    Refused(&'static str),
    /// The client ended it before its answer was whole.
    Aborted(&'static str),
    /// The baseline lost it, for the reason named.
    Lost(&'static str),
}

/// What a run records of one RPC.
#[derive(Debug, Clone)]
struct Record {
    /// The identifier A named it by.
    id: Option<u64>,
    /// Where it ended.
    outcome: Outcome,
    /// When A sent the request.
    issued: Option<Nanos>,
    /// When B reported the whole request first.
    request_at: Option<Nanos>,
    /// When A reported the whole response.
    response_at: Option<Nanos>,
    /// When it ended.
    ended_at: Option<Nanos>,
    /// The SHA-256 of the request B read.
    request_sha: Option<String>,
    /// The SHA-256 of the response A read.
    response_sha: Option<String>,
    /// The length of the response A read.
    response_len: Option<u32>,
    /// Requests B reported for it.
    server_requests: u32,
    /// Responses A reported for it.
    client_responses: u32,
    /// Server RPCs B dropped for it.
    server_dropped: u32,
    /// Restarts after `RPC_UNKNOWN`.
    restarts: u32,
    /// Restarts after a DATA packet of its answer reached A.
    restarts_after_answer: u32,
    /// Reports of `Failure::RpcUnknown`.
    unknown_reported: u32,
    /// Whether a DATA packet of its answer reached A.
    answer_seen: bool,
    /// Reservations B dropped without a commit.
    dropped_reservations: u32,
    /// Dropped reservations after which the next reservation or reply took
    /// its pages.
    returned_reservations: u32,
    /// Dropped reservations after which B still held the request.
    kept_requests: u32,
    /// Why B could not answer, the name of the refusal.
    reply_error: Option<&'static str>,
    /// Whether its lost request has left whole once or met `RPC_UNKNOWN`.
    lose_done: bool,
    /// GRANTs of the request B sent and A took, and of the response A sent
    /// and B took.
    grants: [u32; 4],
    /// RESENDs B sent for the request and A sent for the response.
    resends: [u32; 2],
}

impl Record {
    /// Returns the record of an RPC not sent yet.
    const fn new() -> Self {
        Self {
            id: None,
            outcome: Outcome::Pending,
            issued: None,
            request_at: None,
            response_at: None,
            ended_at: None,
            request_sha: None,
            response_sha: None,
            response_len: None,
            server_requests: 0,
            client_responses: 0,
            server_dropped: 0,
            restarts: 0,
            restarts_after_answer: 0,
            unknown_reported: 0,
            answer_seen: false,
            dropped_reservations: 0,
            returned_reservations: 0,
            kept_requests: 0,
            reply_error: None,
            lose_done: false,
            grants: [0; 4],
            resends: [0; 2],
        }
    }

    /// Returns whether the RPC has ended.
    fn ended(&self) -> bool {
        self.outcome != Outcome::Pending
    }

    /// Returns the record as the object the notebooks read.
    fn json(&self, index: usize) -> JsObject {
        let (outcome, why) = match self.outcome {
            Outcome::Pending => ("pending", None),
            Outcome::Ok => ("ok", None),
            Outcome::Failed(why) => ("failed", Some(why)),
            Outcome::Refused(why) => ("refused", Some(why)),
            Outcome::Aborted(why) => ("aborted", Some(why)),
            Outcome::Lost(why) => ("lost", Some(why)),
        };
        let time = |value: Option<Nanos>| value.map_or(JsValue::Null, JsValue::from);
        let text = |value: &Option<String>| value.clone().map_or(JsValue::Null, JsValue::from);
        let mut object = JsObject::new();
        object.insert("index", count(index));
        object.insert("id", self.id.map_or(JsValue::Null, JsValue::from));
        object.insert("outcome", outcome);
        object.insert("why", why.map_or(JsValue::Null, JsValue::from));
        object.insert("issued_ns", time(self.issued));
        object.insert("request_ns", time(self.request_at));
        object.insert("response_ns", time(self.response_at));
        object.insert("ended_ns", time(self.ended_at));
        object.insert("request_sha", text(&self.request_sha));
        object.insert("response_sha", text(&self.response_sha));
        object.insert(
            "response_len",
            self.response_len.map_or(JsValue::Null, JsValue::from),
        );
        object.insert("server_requests", self.server_requests);
        object.insert("client_responses", self.client_responses);
        object.insert("server_dropped", self.server_dropped);
        object.insert("restarts", self.restarts);
        object.insert("restarts_after_answer", self.restarts_after_answer);
        object.insert("unknown_reported", self.unknown_reported);
        object.insert("dropped_reservations", self.dropped_reservations);
        object.insert("returned_reservations", self.returned_reservations);
        object.insert("kept_requests", self.kept_requests);
        object.insert(
            "reply_error",
            self.reply_error.map_or(JsValue::Null, JsValue::from),
        );
        object.insert("lose_done", self.lose_done);
        let grants: Vec<JsValue> = self.grants.iter().map(|&n| JsValue::from(n)).collect();
        object.insert("grants", grants);
        let resends: Vec<JsValue> = self.resends.iter().map(|&n| JsValue::from(n)).collect();
        object.insert("resends", resends);
        object
    }
}

/// What one end put on the link.
#[derive(Debug, Clone, Copy, Default)]
struct Tally {
    /// Packets handed to the link.
    packets: u64,
    /// Bytes of those packets, headers and payloads.
    bytes: u64,
    /// Bytes of those packets as the link frames them.
    frame_bytes: u64,
    /// DATA packets.
    data: u64,
    /// Bytes of DATA headers.
    data_header_bytes: u64,
    /// DATA packets whose header was the 6 bytes of a whole message.
    data_whole: u64,
    /// DATA packets whose header was the 14 bytes of a part.
    data_part: u64,
    /// DATA packets with any other header length.
    data_other: u64,
    /// Packets the scenario dropped.
    dropped: u64,
    /// Events `LIMITS` reported at this end.
    limits_events: u64,
    /// Events `PeerGone` reported at this end, by link down.
    gone_link_down: u64,
    /// Events `PeerGone` reported at this end, by timeout.
    gone_timeout: u64,
    /// Restarts at this end no packet named.
    unattributed_restarts: u64,
}

impl Tally {
    /// Returns the tally as an object.
    fn json(&self) -> JsObject {
        let mut object = JsObject::new();
        object.insert("packets", self.packets);
        object.insert("bytes", self.bytes);
        object.insert("frame_bytes", self.frame_bytes);
        object.insert("data", self.data);
        object.insert("data_header_bytes", self.data_header_bytes);
        object.insert("data_whole", self.data_whole);
        object.insert("data_part", self.data_part);
        object.insert("data_other", self.data_other);
        object.insert("dropped", self.dropped);
        object.insert("limits_events", self.limits_events);
        object.insert("gone_link_down", self.gone_link_down);
        object.insert("gone_timeout", self.gone_timeout);
        object.insert("unattributed_restarts", self.unattributed_restarts);
        object
    }
}

/// The state of one run over the engine with the wire form `W`.
struct Run<'s, W: Form> {
    /// The scenario.
    s: &'s Scenario,
    /// The time the clock reads.
    now: Nanos,
    /// A and B.
    ends: [Engine<W>; 2],
    /// One record a RPC.
    records: Vec<Record>,
    /// The client identifier of each sent RPC with its index, in order.
    ids: Vec<(u64, usize)>,
    /// The next RPC to send.
    next_issue: usize,
    /// RPCs sent and not ended.
    open: usize,
    /// RPCs not ended.
    left: usize,
    /// Answers B holds, with their time, their RPC and the identifier.
    held: Vec<(Nanos, usize, RpcId)>,
    /// The trace.
    trace: Option<TraceOut>,
    /// What each end put on the link.
    tallies: [Tally; 2],
    /// The SHA-256 of every packet each end put on the link, in order.
    wire: [Sha256; 2],
    /// Whether each end's first LIMITS was dropped.
    limits_dropped: [bool; 2],
    /// Whether the link went down.
    down_done: bool,
    /// Changes at the time the clock reads.
    progress: u64,
    /// The receive buffer.
    rx: Vec<u8>,
    /// The messages that could go, before a transmit decision.
    candidates: Vec<(u64, u32)>,
    /// Whether B restarted.
    restarted: bool,
}

/// Runs a scenario over the engine with the wire form `W` on `net`.
fn run_engine<W: Form, N: Net>(s: &Scenario, net: &mut N, trace: Option<TraceOut>) -> JsObject {
    let build = |side: Side| -> Result<Engine<W>, String> {
        let config = s.configs[index_of(side)];
        Endpoint::new(config, s.profile, Owned::new(&config)).map_err(config_error_name)
    };
    let ends = match (build(Side::A), build(Side::B)) {
        (Ok(a), Ok(b)) => [a, b],
        (Err(error), _) | (_, Err(error)) => {
            let mut payload = JsObject::with_capacity(1);
            payload.insert("config_error", error);
            return payload;
        }
    };
    let slots = s
        .configs
        .iter()
        .map(|c| c.client_slots + c.server_slots)
        .max();
    let mut run: Run<'_, W> = Run {
        s,
        now: 0,
        ends,
        records: vec![Record::new(); s.rpcs.len()],
        ids: Vec::new(),
        next_issue: 0,
        open: 0,
        left: s.rpcs.len(),
        held: Vec::new(),
        trace,
        tallies: [Tally::default(); 2],
        wire: [Sha256::new(), Sha256::new()],
        limits_dropped: [false; 2],
        down_done: false,
        progress: 0,
        rx: vec![0u8; 1 << 17],
        candidates: vec![(0, 0); usize::try_from(slots.unwrap_or(1)).unwrap_or(1)],
        restarted: false,
    };
    for side in SIDES {
        let index = index_of(side);
        let linked = run.ends[index].link_up(side.other(), s.local[index], s.assumed[index]);
        if let Err(error) = linked {
            let mut payload = JsObject::with_capacity(1);
            payload.insert("config_error", format!("link:{}", link_error_name(error)));
            return payload;
        }
    }
    let stopped = run.drive(net);
    let recorded = net.report();
    run.report(net.injected(), recorded, stopped)
}

impl<W: Form> Run<'_, W> {
    /// Moves the run through time until every RPC ended, and returns why it
    /// stopped.
    fn drive<N: Net>(&mut self, net: &mut N) -> &'static str {
        let mut passes = 0u32;
        loop {
            let before = self.progress;
            if !self.restarted && self.s.restart_b_at.is_some_and(|at| self.now >= at) {
                self.restart_b(net);
            }
            self.actions();
            net.step(self.now);
            for side in SIDES {
                self.pump(net, side);
            }
            self.events();
            if self.left == 0 {
                return "done";
            }
            if self.progress != before {
                passes += 1;
                if passes > MAX_PASSES {
                    return "livelock";
                }
                continue;
            }
            passes = 0;
            let Some(upcoming) = self.next_time(net) else {
                return "stuck";
            };
            if upcoming > self.s.max_time {
                return "max_time";
            }
            self.now = upcoming;
        }
    }

    /// Returns the earliest time after now at which anything happens.
    fn next_time<N: Net>(&self, net: &mut N) -> Option<Nanos> {
        let now = self.now;
        let silent = self.silent();
        let mut times = vec![net.step(now), self.ends[0].next_timeout()];
        if !silent {
            times.push(self.ends[1].next_timeout());
        }
        if let Some(spec) = self.s.rpcs.get(self.next_issue) {
            times.push(Some(spec.at));
        }
        times.extend(self.held.iter().map(|&(time, _, _)| Some(time)));
        for (spec, record) in self.s.rpcs.iter().zip(&self.records) {
            if let Some((time, _)) = spec.end
                && !record.ended()
            {
                times.push(Some(time));
            }
        }
        if !self.down_done {
            times.push(self.s.faults.link_down_at);
        }
        if let Some(from) = self.s.silent_b_from {
            times.push(Some(from));
        }
        if !self.restarted {
            times.push(self.s.restart_b_at);
        }
        times.into_iter().flatten().filter(|&time| time > now).min()
    }

    /// Restarts B: builds its endpoint again with a first identifier of its
    /// own, brings it up on the link, forgets the requests it held, and
    /// restarts B's side of the link.
    fn restart_b<N: Net>(&mut self, net: &mut N) {
        self.restarted = true;
        self.progress += 1;
        let mut config = self.s.configs[1];
        config.first_id = config.first_id.wrapping_add(0x0100_0000);
        let Ok(mut engine) = Endpoint::new(config, self.s.profile, Owned::new(&config)) else {
            return;
        };
        // The link was up before the restart, so it comes up again.
        let _ = engine.link_up(Side::A, self.s.local[1], self.s.assumed[1]);
        self.ends[1] = engine;
        self.held.clear();
        net.restart(Side::B);
        trace_put(
            &mut self.trace,
            vec!["B".into(), self.now.into(), "B".into(), "restart".into()],
        );
    }

    /// Returns whether B is silent now.
    fn silent(&self) -> bool {
        self.s.silent_b_from.is_some_and(|from| self.now >= from)
    }

    /// Runs what the scenario names for the time the clock reads: the link
    /// going down, the RPCs A ends, the answers B held and the requests A
    /// sends.
    fn actions(&mut self) {
        if !self.down_done && self.s.faults.link_down_at.is_some_and(|t| self.now >= t) {
            self.down_done = true;
            // Either end may have taken its link down already, after a peer
            // death, which the report of that end shows.
            let _ = self.ends[0].link_down(Side::B);
            let _ = self.ends[1].link_down(Side::A);
            self.progress += 1;
        }
        self.end_rpcs();
        self.release_held();
        self.issue();
    }

    /// Ends the RPCs whose end time has come.
    fn end_rpcs(&mut self) {
        for index in 0..self.records.len() {
            let Some((time, by)) = self.s.rpcs[index].end else {
                continue;
            };
            let record = &self.records[index];
            let (Some(id), false) = (record.id, record.ended()) else {
                continue;
            };
            if self.now < time {
                continue;
            }
            let id = RpcId::new(id);
            let client = &mut self.ends[0];
            let done = match by {
                EndBy::Abort => client.abort(id).is_ok(),
                EndBy::Release => client.release(id).is_ok(),
                EndBy::RpcsAbort => Rpcs::abort(client, id).is_ok(),
                EndBy::RpcsRelease => Rpcs::release(client, id).is_ok(),
            };
            let why = if done { by.name() } else { "not live" };
            self.finish(index, Outcome::Aborted(why));
        }
    }

    /// Answers the requests B held whose time has come.
    fn release_held(&mut self) {
        let now = self.now;
        let due: Vec<(Nanos, usize, RpcId)> = self
            .held
            .iter()
            .copied()
            .filter(|&(time, _, _)| time <= now)
            .collect();
        self.held.retain(|&(time, _, _)| time > now);
        for (_, index, id) in due {
            self.answer(index, id);
        }
    }

    /// Sends the requests whose time has come while the window allows.
    fn issue(&mut self) {
        let s = self.s;
        while let Some(spec) = s.rpcs.get(self.next_issue) {
            if spec.at > self.now || self.open >= self.s.window {
                return;
            }
            let index = self.next_issue;
            let bytes = spec.fill.bytes(spec.len);
            match self.ends[0].request(Side::B, &bytes, spec.options) {
                Ok(id) => {
                    let record = &mut self.records[index];
                    record.id = Some(id.get());
                    record.issued = Some(self.now);
                    let at = self.ids.partition_point(|&(known, _)| known < id.get());
                    self.ids.insert(at, (id.get(), index));
                    self.open += 1;
                }
                Err(SendError::NoSlot | SendError::Full) => return,
                Err(error) => {
                    self.records[index].issued = Some(self.now);
                    self.finish(index, Outcome::Refused(send_error_name(&error)));
                }
            }
            self.next_issue += 1;
            self.progress += 1;
        }
    }

    /// Records the end of an RPC.
    fn finish(&mut self, index: usize, outcome: Outcome) {
        let record = &mut self.records[index];
        if record.ended() {
            return;
        }
        if record.id.is_some() {
            self.open = self.open.saturating_sub(1);
        }
        record.outcome = outcome;
        record.ended_at = Some(self.now);
        self.left = self.left.saturating_sub(1);
        self.progress += 1;
    }

    /// Returns the RPC a wire identifier names, whichever end names it.
    fn rpc_of(&self, id: u64) -> Option<usize> {
        let key = id & !1;
        let at = self.ids.partition_point(|&(known, _)| known < key);
        self.ids
            .get(at)
            .filter(|&&(known, _)| known == key)
            .map(|&(_, index)| index)
    }
}

impl<W: Form> Run<'_, W> {
    /// Takes what arrived at `side`, runs its timers and puts on the link what
    /// it hands out while the link is ready.
    fn pump<N: Net>(&mut self, net: &mut N, side: Side) {
        let index = index_of(side);
        let silent = side == Side::B && self.silent();
        loop {
            let got = net.end(side).recv(&mut self.rx);
            let Ok(Some((peer, len))) = got else {
                break;
            };
            self.progress += 1;
            if silent {
                self.tallies[index].dropped += 1;
                trace_put(
                    &mut self.trace,
                    vec![
                        "X".into(),
                        self.now.into(),
                        side_name(side).into(),
                        "silent".into(),
                    ],
                );
                continue;
            }
            let packet = self.rx[..len].to_vec();
            self.take(side, peer, &packet);
        }
        if silent {
            return;
        }
        // A clock that went backwards runs no timer, and the clock of a run
        // only moves forward.
        let _ = self.ends[index].handle_timeout(self.now);
        self.decisions(side);
        loop {
            let candidates = self.ends[index].transmittable(&mut self.candidates);
            let mut end = net.end(side);
            let Some(packet) = self.ends[index].poll_transmit(self.now, |p, l| end.ready(p, l))
            else {
                break;
            };
            let bytes = [packet.header, packet.payload].concat();
            let payload_len = packet.payload.len();
            let (peer, priority) = (packet.peer, packet.priority);
            self.progress += 1;
            if self.outgoing(side, &bytes, payload_len, candidates) {
                // The link of a simulation never fails.
                let _ = end.send(peer, priority, &bytes, &[]);
            }
        }
    }

    /// Hands one packet that arrived at `side` from `peer` to its end, and
    /// records what it was.
    fn take(&mut self, side: Side, peer: Side, packet: &[u8]) {
        let index = index_of(side);
        let before = self.ends[index].stats();
        let taken = self.ends[index].handle(self.now, peer, packet).is_ok();
        let after = self.ends[index].stats();
        let restarts = after.restarts - before.restarts;
        let reported = after.unknown_reported - before.unknown_reported;
        self.decisions(side);
        let Ok(decoded) = W::decode(packet) else {
            self.tallies[index].unattributed_restarts += restarts;
            return;
        };
        let now = self.now;
        let name = side_name(side);
        match decoded.packet {
            Packet::Data(data) if taken => {
                let rpc = data.id & !1;
                let row = vec![
                    "R".into(),
                    now.into(),
                    name.into(),
                    rpc.into(),
                    data.offset.into(),
                    count(data.payload.len()).into(),
                    data.msg_length.into(),
                ];
                trace_put(&mut self.trace, row);
                if side == Side::A
                    && let Some(rpc) = self.rpc_of(data.id)
                    && !self.records[rpc].ended()
                {
                    self.records[rpc].answer_seen = true;
                }
            }
            Packet::StartMsg { id, msg_length } if taken => {
                let row = vec![
                    "M".into(),
                    now.into(),
                    name.into(),
                    (id & !1).into(),
                    msg_length.into(),
                ];
                trace_put(&mut self.trace, row);
            }
            Packet::Grant { id, .. } => {
                if let Some(rpc) = self.rpc_of(id) {
                    self.records[rpc].grants[if side == Side::A { 1 } else { 3 }] += 1;
                }
            }
            Packet::RpcUnknown { id } if side == Side::A => {
                self.unknown(id, restarts, reported);
                return;
            }
            _ => {}
        }
        self.tallies[index].unattributed_restarts += restarts;
    }

    /// Records an `RPC_UNKNOWN` A took for `id`, with the restarts and the
    /// reports it caused.
    fn unknown(&mut self, id: u64, restarts: u64, reported: u64) {
        let what = if restarts > 0 {
            "restart"
        } else if reported > 0 {
            "report"
        } else {
            "none"
        };
        let row = vec![
            "U".into(),
            self.now.into(),
            "A".into(),
            (id & !1).into(),
            what.into(),
        ];
        trace_put(&mut self.trace, row);
        let Some(rpc) = self.rpc_of(id) else {
            self.tallies[0].unattributed_restarts += restarts;
            return;
        };
        let record = &mut self.records[rpc];
        // The server answered the lost request, so the request it sends again
        // goes on the link.
        record.lose_done = true;
        let restarts = u32::try_from(restarts).unwrap_or(u32::MAX);
        record.restarts += restarts;
        if record.answer_seen {
            record.restarts_after_answer += restarts;
        }
        record.unknown_reported += u32::try_from(reported).unwrap_or(u32::MAX);
    }

    /// Records one packet `side` hands out and returns whether it goes on the
    /// link, which it does unless the scenario loses it.
    fn outgoing(
        &mut self,
        side: Side,
        bytes: &[u8],
        payload_len: usize,
        candidates: usize,
    ) -> bool {
        let index = index_of(side);
        let Ok(decoded) = W::decode(bytes) else {
            return self.put_on_link(side, bytes, None);
        };
        match decoded.packet {
            Packet::Data(data) => {
                if side == Side::A && self.lose(data.id, data.offset, payload_len, data.msg_length)
                {
                    return false;
                }
                self.transmit_row(side, data.id, candidates);
                return self.put_on_link(side, bytes, Some(bytes.len() - payload_len));
            }
            Packet::Grant { id, offset, .. } => {
                let row = vec![
                    "G".into(),
                    self.now.into(),
                    side_name(side).into(),
                    (id & !1).into(),
                    offset.into(),
                ];
                trace_put(&mut self.trace, row);
                if let Some(rpc) = self.rpc_of(id) {
                    self.records[rpc].grants[if side == Side::B { 0 } else { 2 }] += 1;
                }
            }
            Packet::Resend {
                id, offset, length, ..
            } => {
                let row = vec![
                    "S".into(),
                    self.now.into(),
                    side_name(side).into(),
                    (id & !1).into(),
                    offset.into(),
                    length.into(),
                ];
                trace_put(&mut self.trace, row);
                if let Some(rpc) = self.rpc_of(id) {
                    self.records[rpc].resends[index] += 1;
                }
            }
            Packet::Limits(_) if self.s.drop_first_limits && !self.limits_dropped[index] => {
                self.limits_dropped[index] = true;
                self.dropped(side, "limits");
                return false;
            }
            _ => {}
        }
        self.put_on_link(side, bytes, None)
    }

    /// Returns whether the DATA packet of the request `id` is lost because its
    /// RPC loses its request, and records it.
    fn lose(&mut self, id: u64, offset: u32, payload_len: usize, msg_length: u32) -> bool {
        let Some(rpc) = self.rpc_of(id) else {
            return false;
        };
        if !self.s.rpcs[rpc].lose || self.records[rpc].lose_done {
            return false;
        }
        let end = u64::from(offset) + count(payload_len);
        if end >= u64::from(msg_length) {
            self.records[rpc].lose_done = true;
        }
        self.dropped(Side::A, "lose");
        true
    }

    /// Writes a `P` row for every decision of the grant scheduler of
    /// `side` since the last call: the RPC, the offset it grants up to, the
    /// level, the rank of the message among the messages granted at once,
    /// their count, the window and whether the FIFO share chose it.
    fn decisions(&mut self, side: Side) {
        let taken = self.ends[index_of(side)].grant_decisions();
        if self.trace.is_none() {
            return;
        }
        for decision in taken {
            let row = vec![
                "P".into(),
                self.now.into(),
                side_name(side).into(),
                (decision.id.get() & !1).into(),
                decision.offset.into(),
                u32::from(decision.level).into(),
                u32::from(decision.rank).into(),
                u32::from(decision.active).into(),
                decision.window.into(),
                decision.fifo.into(),
            ];
            trace_put(&mut self.trace, row);
        }
    }

    /// Records a packet of `side` the scenario dropped.
    fn dropped(&mut self, side: Side, reason: &'static str) {
        self.tallies[index_of(side)].dropped += 1;
        let row = vec![
            "X".into(),
            self.now.into(),
            side_name(side).into(),
            reason.into(),
        ];
        trace_put(&mut self.trace, row);
    }

    /// Writes the trace line of a DATA packet of `id` that `side` sends, with
    /// the messages that could have gone.
    fn transmit_row(&mut self, side: Side, id: u64, candidates: usize) {
        if self.trace.is_none() {
            return;
        }
        let Some((_, left, fifo)) = self.ends[index_of(side)].last_data() else {
            return;
        };
        let could: Vec<JsValue> = self.candidates[..candidates]
            .iter()
            .map(|&(rpc, bytes)| JsValue::Array(vec![(rpc & !1).into(), bytes.into()]))
            .collect();
        let row = vec![
            "T".into(),
            self.now.into(),
            side_name(side).into(),
            (id & !1).into(),
            left.into(),
            fifo.into(),
            could.into(),
        ];
        trace_put(&mut self.trace, row);
    }

    /// Counts a packet `side` puts on the link, whose DATA header is
    /// `data_header` bytes where it is DATA, and returns true.
    fn put_on_link(&mut self, side: Side, bytes: &[u8], data_header: Option<usize>) -> bool {
        let index = index_of(side);
        let len = count(bytes.len());
        let framed = match self.s.frame {
            None => len,
            Some(mode) => {
                let mut out = vec![0u8; stream::frame_len(mode, bytes.len())];
                count(stream::encode(mode, bytes, &[], &mut out).unwrap_or(0))
            }
        };
        let tally = &mut self.tallies[index];
        tally.packets += 1;
        tally.bytes += len;
        tally.frame_bytes += framed;
        if let Some(header) = data_header {
            tally.data += 1;
            tally.data_header_bytes += count(header);
            match header {
                6 => tally.data_whole += 1,
                14 => tally.data_part += 1,
                _ => tally.data_other += 1,
            }
        }
        let wire = &mut self.wire[index];
        wire.update(u32::try_from(bytes.len()).unwrap_or(u32::MAX).to_be_bytes());
        wire.update(bytes);
        true
    }
}

impl<W: Form> Run<'_, W> {
    /// Takes the events of both ends.
    fn events(&mut self) {
        while let Some(event) = self.ends[1].poll_event() {
            self.progress += 1;
            match event {
                Event::Request { id, .. } => self.request(id),
                Event::Dropped { id } => {
                    if let Some(rpc) = self.rpc_of(id.get()) {
                        self.records[rpc].server_dropped += 1;
                    }
                }
                other => self.peer_event(Side::B, other),
            }
        }
        while let Some(event) = self.ends[0].poll_event() {
            self.progress += 1;
            match event {
                Event::Response { id } => self.response(id),
                Event::Failed { id, why } => {
                    let row = vec![
                        "F".into(),
                        self.now.into(),
                        "A".into(),
                        id.get().into(),
                        failure_name(why).into(),
                    ];
                    trace_put(&mut self.trace, row);
                    if let Some(rpc) = self.rpc_of(id.get()) {
                        self.finish(rpc, Outcome::Failed(failure_name(why)));
                    }
                }
                other => self.peer_event(Side::A, other),
            }
        }
    }

    /// Counts an event of a peer at `side`.
    fn peer_event(&mut self, side: Side, event: Event<Side>) {
        let tally = &mut self.tallies[index_of(side)];
        match event {
            Event::Limits { .. } => tally.limits_events += 1,
            Event::PeerGone {
                why: Gone::LinkDown,
                ..
            } => tally.gone_link_down += 1,
            Event::PeerGone {
                why: Gone::Timeout, ..
            } => tally.gone_timeout += 1,
            _ => {}
        }
    }

    /// Takes a whole request B reported.
    fn request(&mut self, id: RpcId) {
        let Some(rpc) = self.rpc_of(id.get()) else {
            return;
        };
        let bytes = read_all(self.ends[1].request_of(id));
        let record = &mut self.records[rpc];
        record.server_requests += 1;
        if record.request_at.is_none() {
            record.request_at = Some(self.now);
            record.request_sha = bytes.as_deref().map(sha256_hex);
        }
        let row = vec![
            "D".into(),
            self.now.into(),
            "B".into(),
            (id.get() & !1).into(),
            count(bytes.as_ref().map_or(0, Vec::len)).into(),
        ];
        trace_put(&mut self.trace, row);
        let hold = self.s.rpcs[rpc].hold;
        if hold > 0 {
            self.held.push((self.now.saturating_add(hold), rpc, id));
        } else {
            self.answer(rpc, id);
        }
    }

    /// Answers the request of `rpc` that B names `id`.
    fn answer(&mut self, rpc: usize, id: RpcId) {
        let s = self.s;
        let spec = &s.rpcs[rpc];
        let bytes = spec.response_fill.bytes(spec.response_len);
        let server = &mut self.ends[1];
        let record = &mut self.records[rpc];
        let mut dropped_before = false;
        for _ in 0..spec.drop_reservations {
            let mut writer = match server.reply_reserve(id, spec.reserve_max) {
                Ok(writer) => writer,
                Err(error) => {
                    record.reply_error = Some(send_error_name(&error));
                    return;
                }
            };
            if dropped_before {
                record.returned_reservations += 1;
            }
            // A write past the reservation fails and changes nothing, and the
            // reservation is dropped either way.
            let _ = writer.write(&bytes[..bytes.len().min(100)]);
            drop(writer);
            record.dropped_reservations += 1;
            dropped_before = true;
            if server.request_of(id).is_some() {
                record.kept_requests += 1;
            }
        }
        let answered = if spec.reserve {
            reply_reserved(server, id, spec.reserve_max, &bytes)
        } else {
            server
                .reply(id, &bytes)
                .map_err(|error| send_error_name(&error))
        };
        match answered {
            Ok(()) => {
                if dropped_before {
                    record.returned_reservations += 1;
                }
            }
            Err(why) => record.reply_error = Some(why),
        }
    }

    /// Takes a whole response A reported.
    fn response(&mut self, id: RpcId) {
        let bytes = read_all(self.ends[0].response(id));
        // The response is read, so A frees it and owes its ACK.
        let _ = self.ends[0].release(id);
        let Some(rpc) = self.rpc_of(id.get()) else {
            return;
        };
        let row = vec![
            "D".into(),
            self.now.into(),
            "A".into(),
            id.get().into(),
            count(bytes.as_ref().map_or(0, Vec::len)).into(),
        ];
        trace_put(&mut self.trace, row);
        let record = &mut self.records[rpc];
        record.client_responses += 1;
        if record.response_at.is_none() {
            record.response_at = Some(self.now);
            record.response_sha = bytes.as_deref().map(sha256_hex);
            record.response_len = bytes
                .as_ref()
                .map(|b| u32::try_from(b.len()).unwrap_or(u32::MAX));
        }
        self.finish(rpc, Outcome::Ok);
    }

    /// Returns what a run answers.
    fn report(
        self,
        injected: Option<Injected>,
        recorded: Option<JsObject>,
        stopped: &'static str,
    ) -> JsObject {
        let rpcs: Vec<JsValue> = self
            .records
            .iter()
            .enumerate()
            .map(|(index, record)| JsValue::Object(record.json(index)))
            .collect();
        let mut stats = JsObject::with_capacity(2);
        stats.insert("A", stats_json(&self.ends[0].stats()));
        stats.insert("B", stats_json(&self.ends[1].stats()));
        let mut links = JsObject::with_capacity(2);
        links.insert("A", self.tallies[0].json());
        links.insert("B", self.tallies[1].json());
        let [wire_a, wire_b] = self.wire;
        let mut wire = JsObject::with_capacity(2);
        wire.insert("A", hex(&wire_a.finalize()));
        wire.insert("B", hex(&wire_b.finalize()));
        let mut payload = JsObject::new();
        payload.insert("stopped", stopped);
        payload.insert("end_ns", self.now);
        payload.insert("rpcs", rpcs);
        payload.insert("stats", stats);
        payload.insert("links", links);
        payload.insert("wire_sha", wire);
        payload.insert(
            "injected",
            injected.map_or(JsValue::Null, |i| JsValue::Object(injected_json(&i))),
        );
        payload.insert("restarted_b", self.restarted);
        payload.insert("radio", recorded.map_or(JsValue::Null, JsValue::Object));
        payload.insert(
            "trace",
            self.trace
                .map_or(JsValue::Null, |out| JsValue::Object(out.finish())),
        );
        payload
    }
}

/// Answers through a reservation of `max` bytes written in chunks and
/// committed, and returns the name of the refusal where it fails.
fn reply_reserved<W: Form>(
    server: &mut Engine<W>,
    id: RpcId,
    max: u32,
    bytes: &[u8],
) -> Result<(), &'static str> {
    let mut writer = server
        .reply_reserve(id, max)
        .map_err(|error| send_error_name(&error))?;
    let (first, rest) = bytes.split_at(bytes.len().min(7));
    writer.write(first).map_err(|_| "NoRoom")?;
    for chunk in rest.chunks(1_000) {
        writer.write(chunk).map_err(|_| "NoRoom")?;
    }
    writer.commit().map_err(|_| "Gone")
}

/// Returns every byte of a message, `None` where the endpoint holds none.
pub(crate) fn read_all(message: Option<homa::Reader<'_>>) -> Option<Vec<u8>> {
    let message = message?;
    let mut out = vec![0u8; usize::try_from(message.len()).unwrap_or(0)];
    let copied = message.read_at(0, &mut out);
    out.truncate(copied);
    Some(out)
}

/// Returns the SHA-256 of `bytes` as lowercase hex.
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

/// Returns the counters of an end as an object.
pub(crate) fn stats_json(stats: &Stats) -> JsObject {
    let mut object = JsObject::new();
    object.insert("sent", stats.sent);
    object.insert("received", stats.received);
    object.insert("refused", stats.refused);
    object.insert("resends", stats.resends);
    object.insert("restarts", stats.restarts);
    object.insert("limits_sent", stats.limits_sent);
    object.insert("peer_deaths", stats.peer_deaths);
    let levels: Vec<JsValue> = stats.per_priority.iter().map(|&n| n.into()).collect();
    object.insert("per_priority", levels);
    object.insert("grants_sent", stats.grants_sent);
    object.insert("start_msg_sent", stats.start_msg_sent);
    object.insert("incast_marked", stats.incast_marked);
    object.insert("incast_honoured", stats.incast_honoured);
    object.insert("cutoffs_sent", stats.cutoffs_sent);
    object.insert("probes_beyond_first", stats.probes_beyond_first);
    object.insert("freezes_dropped", stats.freezes_dropped);
    object.insert("unknown_reported", stats.unknown_reported);
    object.insert("wrong_port", stats.wrong_port);
    object
}

/// Returns what a lossy link injected as an object.
fn injected_json(injected: &Injected) -> JsObject {
    let mut object = JsObject::new();
    object.insert("dropped", injected.dropped);
    object.insert("grants_dropped", injected.grants_dropped);
    object.insert("limits_dropped", injected.limits_dropped);
    object.insert("duplicated", injected.duplicated);
    object.insert("corrupted", injected.corrupted);
    object.insert("corrupted_delivered", injected.corrupted_delivered);
    object.insert("down", injected.down);
    object.insert("truncated", injected.truncated);
    object
}

/// One message of the baseline on its way: its RPC and whether it is the
/// request.
type Carried = (usize, bool);

/// The state of one run over the baseline of `AoFrame` alone.
///
/// A sends each request whole in one frame, B answers it whole in one frame
/// once it arrives, and nothing is retransmitted, so a lost message loses its
/// RPC.
struct BaselineRun<'s> {
    /// The scenario.
    s: &'s Scenario,
    /// The link.
    base: Baseline,
    /// The time the clock reads.
    now: Nanos,
    /// One record a RPC.
    records: Vec<Record>,
    /// The messages on their way from A and from B, in order.
    queues: [VecDeque<Carried>; 2],
    /// What each end put on the link.
    tallies: [Tally; 2],
    /// Answers B holds, with their time and their RPC.
    held: Vec<(Nanos, usize)>,
    /// The next RPC to send.
    next_issue: usize,
    /// RPCs not ended.
    left: usize,
    /// The trace.
    trace: Option<TraceOut>,
}

impl<'s> BaselineRun<'s> {
    /// Returns the run of a scenario over the baseline.
    fn new(s: &'s Scenario, trace: Option<TraceOut>) -> Self {
        Self {
            s,
            base: Baseline::new(s.model, s.faults, Rng::new(s.seed), s.baseline_mode),
            now: 0,
            records: vec![Record::new(); s.rpcs.len()],
            queues: [VecDeque::new(), VecDeque::new()],
            tallies: [Tally::default(); 2],
            held: Vec::new(),
            next_issue: 0,
            left: s.rpcs.len(),
            trace,
        }
    }

    /// Moves the run through time until every RPC ended, and returns what it
    /// answers.
    fn run(mut self) -> JsObject {
        let mut events = Trace::default();
        let stopped = loop {
            self.issue();
            self.release_held();
            let next_arrival = self.base.step(self.now, &mut events);
            for event in events.events.drain(..) {
                self.event(&event);
            }
            if self.left == 0 {
                break "done";
            }
            if self.held.iter().any(|&(time, _)| time <= self.now) {
                continue;
            }
            let issue = self.s.rpcs.get(self.next_issue).map(|spec| spec.at);
            let held = self.held.iter().map(|&(time, _)| time).min();
            let next = [next_arrival, issue, held]
                .into_iter()
                .flatten()
                .filter(|&time| time > self.now)
                .min();
            match next {
                None => break "stuck",
                Some(time) if time > self.s.max_time => break "max_time",
                Some(time) => self.now = time,
            }
        };
        self.report(stopped)
    }

    /// Sends the requests whose time has come.
    fn issue(&mut self) {
        let s = self.s;
        while let Some(spec) = s.rpcs.get(self.next_issue) {
            if spec.at > self.now {
                return;
            }
            let bytes = spec.fill.bytes(spec.len);
            self.send(Side::A, &bytes);
            self.queues[0].push_back((self.next_issue, true));
            self.records[self.next_issue].issued = Some(self.now);
            self.next_issue += 1;
        }
    }

    /// Answers the requests B held whose time has come.
    fn release_held(&mut self) {
        let now = self.now;
        let due: Vec<usize> = self
            .held
            .iter()
            .filter(|&&(time, _)| time <= now)
            .map(|&(_, rpc)| rpc)
            .collect();
        self.held.retain(|&(time, _)| time > now);
        let s = self.s;
        for rpc in due {
            let spec = &s.rpcs[rpc];
            let bytes = spec.response_fill.bytes(spec.response_len);
            self.send(Side::B, &bytes);
            self.queues[1].push_back((rpc, false));
        }
    }

    /// Puts one message on the stream from `side` and counts it.
    fn send(&mut self, side: Side, message: &[u8]) {
        let mut out = vec![0u8; stream::frame_len(self.s.baseline_mode, message.len())];
        let framed = stream::encode(self.s.baseline_mode, &[], message, &mut out).unwrap_or(0);
        let tally = &mut self.tallies[index_of(side)];
        tally.packets += 1;
        tally.bytes += count(message.len());
        tally.frame_bytes += count(framed);
        self.base.send(side, self.now, message);
    }

    /// Takes one event of the baseline: a message delivered or lost.
    fn event(&mut self, event: &TraceEvent) {
        let (time, to, delivered) = match *event {
            TraceEvent::Deliver { at, to, length, .. } => (at, to, Ok(length)),
            TraceEvent::Drop { at, to, reason } => (at, to, Err(reason)),
            _ => return,
        };
        let Some((rpc, is_request)) = self.queues[index_of(to.other())].pop_front() else {
            return;
        };
        let record = &mut self.records[rpc];
        let row = match delivered {
            Ok(length) => {
                if is_request {
                    record.server_requests += 1;
                    record.request_at = Some(time);
                    self.held
                        .push((time.saturating_add(self.s.rpcs[rpc].hold), rpc));
                } else {
                    record.client_responses += 1;
                    record.response_at = Some(time);
                    record.response_len = Some(length);
                    record.outcome = Outcome::Ok;
                    record.ended_at = Some(time);
                    self.left = self.left.saturating_sub(1);
                }
                vec![
                    "D".into(),
                    time.into(),
                    side_name(to).into(),
                    count(rpc).into(),
                    length.into(),
                ]
            }
            Err(reason) => {
                record.outcome = Outcome::Lost(reason);
                record.ended_at = Some(time);
                self.left = self.left.saturating_sub(1);
                vec!["X".into(), time.into(), side_name(to).into(), reason.into()]
            }
        };
        trace_put(&mut self.trace, row);
    }

    /// Returns what the run answers.
    fn report(self, stopped: &'static str) -> JsObject {
        let rpcs: Vec<JsValue> = self
            .records
            .iter()
            .enumerate()
            .map(|(index, record)| JsValue::Object(record.json(index)))
            .collect();
        let mut links = JsObject::with_capacity(2);
        links.insert("A", self.tallies[0].json());
        links.insert("B", self.tallies[1].json());
        let mut payload = JsObject::new();
        payload.insert("stopped", stopped);
        payload.insert("end_ns", self.now);
        payload.insert("rpcs", rpcs);
        payload.insert("links", links);
        payload.insert(
            "trace",
            self.trace
                .map_or(JsValue::Null, |out| JsValue::Object(out.finish())),
        );
        payload
    }
}

/// A host of a fabric run.
type Host = Endpoint<u16, Owned<u16>, Full>;

/// One RPC of a fabric run: its client and server hosts, the time the
/// client sends it, and the bytes of its request and of its response.
#[derive(Debug, Clone, Copy)]
struct FabricRpc {
    /// The host that sends the request.
    client: u16,
    /// The host that answers it.
    server: u16,
    /// The time the client sends the request.
    at: Nanos,
    /// Bytes of the request.
    len: u32,
    /// Bytes of the response.
    response_len: u32,
}

/// A fabric scenario read from its request.
struct FabricScenario {
    /// The shape of the fabric.
    fabric: Fabric,
    /// The values every host runs with.
    profile: Profile,
    /// The configuration of every host, whose first identifier the run sets
    /// per host.
    configs: Vec<Config>,
    /// The levels in use.
    levels: u8,
    /// The cutoffs every host announces from the start, or its defaults.
    cutoffs: Option<[u32; 8]>,
    /// The time every host recomputes its cutoffs from what it received.
    recompute_at: Option<Nanos>,
    /// The rate pacer of every host: its rate, its backlog and the shortest
    /// message it holds.
    pacer: Option<(u64, Nanos, u32)>,
    /// The RPCs, in the order of their time.
    rpcs: Vec<FabricRpc>,
    /// The time the counters of the switch are cleared at.
    measure_from: Nanos,
    /// The time the counters of the switch are read at.
    measure_to: Option<Nanos>,
    /// The time after which the run stops.
    max_time: Nanos,
    /// The seed of the fabric's delays.
    seed: u64,
    /// Where the trace goes.
    trace: Option<PathBuf>,
}

/// The first identifier of host `h`, which gives every host its own range
/// of identifiers, so an identifier names one RPC in the whole fabric and
/// the client of a request is its identifier over 2^40, less 1.
const fn first_id_of(h: u16) -> u64 {
    ((h as u64) + 1) << 40
}

/// Returns the client host of the RPC a request's identifier names.
fn client_of(id: u64) -> u16 {
    u16::try_from((id >> 40).saturating_sub(1)).unwrap_or(u16::MAX)
}

impl FabricScenario {
    /// Reads a fabric scenario from its request.
    fn read(fields: &Fields<'_>) -> Result<Self, JsString> {
        let fabric = fabric_of(fields)?;
        let mut profile = request_profile(fields)?;
        if !fields.bool_or("incast", true)? {
            profile.incast = None;
        }
        Ok(Self {
            fabric,
            profile,
            configs: host_configs(fields, fabric.hosts)?,
            levels: fields.int_or("levels", 8)?,
            cutoffs: cutoffs_of(fields)?,
            recompute_at: fields.opt_u64("recompute_at_ns")?,
            pacer: pacer_of(fields)?,
            rpcs: fabric_rpcs(fields, fabric.hosts)?,
            measure_from: fields.int_or("measure_from_ns", 0)?,
            measure_to: fields.opt_u64("measure_to_ns")?,
            max_time: fields.int_or("max_time_ns", 10_000_000_000)?,
            seed: fields.int_or("seed", 1)?,
            trace: fields.opt_str("trace")?.map(PathBuf::from),
        })
    }
}

/// Reads the shape of the fabric under `fabric`.
fn fabric_of(fields: &Fields<'_>) -> Result<Fabric, JsString> {
    let spec = fields.nested("fabric")?;
    Ok(Fabric {
        hosts: spec.int("hosts")?,
        link_bits_per_s: spec.u64("link_bits_per_s")?,
        propagation: spec.int_or("propagation_ns", 0)?,
        buffer_bytes: spec.int_or("buffer_bytes", u32::MAX)?,
        priority_queues: spec.int_or("priority_queues", 8)?,
        spray: spec.bool_or("spray", false)?,
    })
}

/// Reads the configuration of every host: `config` for all, and each entry
/// of `host_configs` for the host it names, with a first identifier of its
/// own and a peer a host at least.
fn host_configs(fields: &Fields<'_>, hosts: u16) -> Result<Vec<Config>, JsString> {
    let shared = match fields.opt_object("config")? {
        Some(_) => Some(fields.nested("config")?),
        None => None,
    };
    let base = config_of(shared.as_ref(), DEFAULT_CONFIG)?;
    let mut configs = vec![base; usize::from(hosts)];
    for value in fields.array("host_configs")? {
        let JsValue::Object(object) = value else {
            return Err(fields.refuse("host_configs", "holds an entry that is no object"));
        };
        let entry = Fields::new(object, "fabric");
        let host: usize = entry.int("host")?;
        let Some(slot) = configs.get_mut(host) else {
            return Err(entry.refuse("host", "names no host of the fabric"));
        };
        *slot = config_of(Some(&entry), base)?;
    }
    for (h, config) in configs.iter_mut().enumerate() {
        config.first_id = first_id_of(u16::try_from(h).unwrap_or(u16::MAX));
        config.peers = config.peers.max(u32::from(hosts));
    }
    Ok(configs)
}

/// Reads the cutoffs every host announces from the start, `None` where
/// `cutoffs` is absent or empty.
fn cutoffs_of(fields: &Fields<'_>) -> Result<Option<[u32; 8]>, JsString> {
    let values = fields.array("cutoffs")?;
    if values.is_empty() {
        return Ok(None);
    }
    let mut out = [0u32; 8];
    for (slot, value) in out.iter_mut().zip(values) {
        *slot = crate::homa_packet::whole(value)
            .and_then(|v| u32::try_from(v).ok())
            .ok_or_else(|| fields.refuse("cutoffs", "holds a value that is no u32"))?;
    }
    Ok(Some(out))
}

/// Reads the rate pacer of every host under `pacer`, `None` where it is
/// absent.
fn pacer_of(fields: &Fields<'_>) -> Result<Option<(u64, Nanos, u32)>, JsString> {
    if fields.opt_object("pacer")?.is_none() {
        return Ok(None);
    }
    let spec = fields.nested("pacer")?;
    Ok(Some((
        spec.u64("bits_per_s")?,
        spec.int_or("max_backlog_ns", 5_000)?,
        spec.int_or("defer_min_bytes", 1_000)?,
    )))
}

/// Reads the RPCs of a fabric run, each an array of the client, the server,
/// the time, the bytes of the request and of the response, in time order.
fn fabric_rpcs(fields: &Fields<'_>, hosts: u16) -> Result<Vec<FabricRpc>, JsString> {
    let mut rpcs = Vec::new();
    for value in fields.array("rpcs")? {
        let JsValue::Array(items) = value else {
            return Err(fields.refuse("rpcs", "holds an entry that is no array"));
        };
        let item = |i: usize| items.get(i).and_then(crate::homa_packet::whole);
        let (Some(client), Some(server), Some(at), Some(len), Some(response_len)) =
            (item(0), item(1), item(2), item(3), item(4))
        else {
            return Err(fields.refuse("rpcs", "holds an entry that is no five numbers"));
        };
        let host = |value: u64| {
            u16::try_from(value)
                .ok()
                .filter(|&h| h < hosts)
                .ok_or_else(|| fields.refuse("rpcs", "names no host of the fabric"))
        };
        let bytes = |value: u64| {
            u32::try_from(value).map_err(|_| fields.refuse("rpcs", "names a length past u32"))
        };
        rpcs.push(FabricRpc {
            client: host(client)?,
            server: host(server)?,
            at,
            len: bytes(len)?,
            response_len: bytes(response_len)?,
        });
    }
    rpcs.sort_by_key(|rpc| rpc.at);
    Ok(rpcs)
}

/// Answers the operation `fabric`: runs RPCs between the hosts of a fabric.
///
/// Every host runs one endpoint of the full form over its port of the
/// fabric, both roles, with the levels, the cutoffs and the pacer the
/// request names, and answers every request with `response_len` zero bytes
/// as soon as it is whole. The answer carries each RPC as the times it was
/// sent, its request was whole at the server and its response was whole at
/// the client, and its outcome: 0 pending, 1 answered, 2 failed and 3 not
/// sent, since a request the client refused never leaves.
///
/// Where the request names `recompute_at_ns`, every host recomputes its
/// cutoffs from the lengths it received at that time, or as the run ends
/// where it ends earlier, and the answer carries them under `cutoffs`.
pub fn op_fabric(request: &JsObject) -> Result<JsObject, JsString> {
    let fields = Fields::new(request, "fabric");
    let scenario = FabricScenario::read(&fields)?;
    let trace = match &scenario.trace {
        Some(path) => Some(TraceOut::create(path)?),
        None => None,
    };
    let mut run = match FabricRun::new(&scenario, trace) {
        Ok(run) => run,
        Err(error) => {
            let mut payload = JsObject::with_capacity(1);
            payload.insert("config_error", error);
            return Ok(payload);
        }
    };
    let stopped = run.drive();
    let mut payload = run.report(stopped);
    payload.insert("profile", profile_json(&scenario.profile));
    Ok(payload)
}

/// The outcome codes of the RPCs of a fabric run.
const PENDING: u8 = 0;
/// An RPC whose response arrived whole.
const ANSWERED: u8 = 1;
/// An RPC that failed.
const FAILED: u8 = 2;
/// An RPC whose request the client refused.
const NOT_SENT: u8 = 3;

/// The state of one fabric run.
struct FabricRun<'s> {
    /// The scenario.
    s: &'s FabricScenario,
    /// The fabric.
    net: FabricNet,
    /// One endpoint a host.
    hosts: Vec<Host>,
    /// The time the clock reads.
    now: Nanos,
    /// When each RPC was sent.
    issued: Vec<Nanos>,
    /// When each RPC's request was whole at its server.
    delivered: Vec<Nanos>,
    /// When each RPC ended.
    done: Vec<Nanos>,
    /// The outcome of each RPC.
    outcome: Vec<u8>,
    /// For each client host its identifiers with their RPC, in order.
    ids: Vec<Vec<(u64, usize)>>,
    /// The next RPC to send.
    next: usize,
    /// For each client host the RPCs that waited for a slot.
    waiting: Vec<VecDeque<usize>>,
    /// RPCs not ended.
    left: usize,
    /// The trace.
    trace: Option<TraceOut>,
    /// The drops of the switch since the last read.
    dropped: Trace,
    /// The receive buffer.
    rx: Vec<u8>,
    /// The bytes every response carries.
    zeros: Vec<u8>,
    /// Whether the cutoffs were recomputed.
    recomputed: bool,
    /// The cutoffs each host announced after the recomputation.
    cutoffs_after: Vec<[u32; 8]>,
    /// Whether the counters of the switch were cleared.
    cleared: bool,
    /// The counters of the switch read at the end of the measure.
    snapshot: Option<FabricStats>,
    /// The hosts to pump at the time the clock reads.
    todo: Vec<bool>,
    /// Changes at the time the clock reads.
    progress: u64,
    /// The drops of the switch in all.
    overflow: u64,
}

impl<'s> FabricRun<'s> {
    /// Builds the hosts of a fabric run.
    fn new(s: &'s FabricScenario, trace: Option<TraceOut>) -> Result<Self, String> {
        let mut hosts = Vec::with_capacity(s.configs.len());
        for config in &s.configs {
            let mut host: Host =
                Endpoint::new(*config, s.profile, Owned::new(config)).map_err(config_error_name)?;
            host.set_link_rate(s.fabric.link_bits_per_s);
            if !host.set_levels(homa::prio::Levels::split(s.levels)) {
                return Err(String::from("levels"));
            }
            if let Some(bytes) = s.cutoffs {
                host.set_cutoffs(homa::prio::Cutoffs { bytes, version: 1 })
                    .map_err(config_error_name)?;
            }
            if let Some((rate, backlog, defer)) = s.pacer {
                host.set_pacer(Some(homa::pacer::Rate::new(rate, backlog, defer)));
            }
            hosts.push(host);
        }
        let count = s.rpcs.len();
        let largest = s.rpcs.iter().map(|rpc| rpc.response_len).max().unwrap_or(0);
        Ok(Self {
            s,
            net: s.fabric.build(Rng::new(s.seed)),
            hosts,
            now: 0,
            issued: vec![0; count],
            delivered: vec![0; count],
            done: vec![0; count],
            outcome: vec![PENDING; count],
            ids: vec![Vec::new(); s.configs.len()],
            next: 0,
            waiting: vec![VecDeque::new(); s.configs.len()],
            left: count,
            trace,
            dropped: Trace::default(),
            rx: vec![0u8; 1 << 17],
            zeros: vec![0u8; usize::try_from(largest).unwrap_or(0)],
            recomputed: false,
            cutoffs_after: Vec::new(),
            cleared: false,
            snapshot: None,
            todo: vec![false; s.configs.len()],
            progress: 0,
            overflow: 0,
        })
    }

    /// Moves the run through time until every RPC ended, and returns why it
    /// stopped.
    fn drive(&mut self) -> &'static str {
        let mut passes = 0u32;
        loop {
            let before = self.progress;
            self.actions();
            self.issue();
            self.net.step(self.now, &mut self.dropped);
            self.overflows();
            for h in self.net.touched() {
                self.mark(h);
            }
            for h in 0..self.hosts.len() {
                let due = self.hosts[h]
                    .next_timeout()
                    .is_some_and(|at| at <= self.now);
                if due {
                    self.todo[h] = true;
                }
            }
            for h in 0..self.hosts.len() {
                if core::mem::replace(&mut self.todo[h], false) {
                    self.pump(u16::try_from(h).unwrap_or(u16::MAX));
                }
            }
            if self.left == 0 {
                // A run that ends before the time of its recomputation
                // recomputes as it ends.
                self.recompute();
                return "done";
            }
            if self.progress != before {
                passes += 1;
                if passes > MAX_PASSES {
                    return "livelock";
                }
                continue;
            }
            passes = 0;
            let Some(upcoming) = self.next_time() else {
                return "stuck";
            };
            if upcoming > self.s.max_time {
                return "max_time";
            }
            self.now = upcoming;
        }
    }

    /// Marks host `h` to pump at the time the clock reads.
    fn mark(&mut self, h: u16) {
        if let Some(todo) = self.todo.get_mut(usize::from(h)) {
            *todo = true;
        }
    }

    /// Returns the earliest time after now at which anything happens.
    fn next_time(&mut self) -> Option<Nanos> {
        let now = self.now;
        let mut times = vec![self.net.step(now, &mut self.dropped)];
        times.extend(self.hosts.iter().map(Host::next_timeout));
        times.push(self.s.rpcs.get(self.next).map(|rpc| rpc.at));
        if !self.recomputed {
            times.push(self.s.recompute_at);
        }
        if !self.cleared {
            times.push(Some(self.s.measure_from));
        }
        if self.snapshot.is_none() {
            times.push(self.s.measure_to);
        }
        times.into_iter().flatten().filter(|&time| time > now).min()
    }

    /// Runs what the scenario names for the time the clock reads: the
    /// counters of the switch and the recomputation of the cutoffs.
    fn actions(&mut self) {
        if !self.cleared && self.now >= self.s.measure_from {
            self.cleared = true;
            self.net.clear_counters(self.now);
            self.progress += 1;
        }
        if self.snapshot.is_none() && self.s.measure_to.is_some_and(|at| self.now >= at) {
            self.snapshot = Some(self.net.queues());
            self.progress += 1;
        }
        if self.s.recompute_at.is_some_and(|at| self.now >= at) {
            self.recompute();
        }
    }

    /// Recomputes the cutoffs of every host from what it received, once.
    fn recompute(&mut self) {
        if self.recomputed || self.s.recompute_at.is_none() {
            return;
        }
        self.recomputed = true;
        let levels = homa::prio::Levels::split(self.s.levels);
        self.cutoffs_after = self
            .hosts
            .iter_mut()
            .map(|host| host.recompute_cutoffs(levels).bytes)
            .collect();
        self.progress += 1;
    }

    /// Records the drops of the switch since the last call.
    fn overflows(&mut self) {
        for event in self.dropped.events.drain(..) {
            if let TraceEvent::Overflow { at, to, level } = event {
                self.overflow += 1;
                trace_put(
                    &mut self.trace,
                    vec![
                        "O".into(),
                        at.into(),
                        u32::from(to).into(),
                        u32::from(level).into(),
                    ],
                );
            }
        }
    }

    /// Sends the requests whose time has come, and those that waited for a
    /// slot where one is free.
    fn issue(&mut self) {
        while let Some(rpc) = self.s.rpcs.get(self.next) {
            if rpc.at > self.now {
                break;
            }
            let index = self.next;
            self.next += 1;
            self.progress += 1;
            let client = usize::from(rpc.client);
            if self.waiting[client].is_empty() {
                self.send(index);
            } else {
                self.waiting[client].push_back(index);
            }
        }
    }

    /// Sends the request of RPC `index`, or keeps it waiting for a slot.
    fn send(&mut self, index: usize) -> bool {
        let rpc = self.s.rpcs[index];
        let client = usize::from(rpc.client);
        let len = usize::try_from(rpc.len).unwrap_or(0);
        let zeros = vec![0u8; len];
        match self.hosts[client].request(rpc.server, &zeros, RequestOptions::default()) {
            Ok(id) => {
                self.issued[index] = self.now;
                let list = &mut self.ids[client];
                let at = list.partition_point(|&(known, _)| known < id.get());
                list.insert(at, (id.get(), index));
                self.mark(rpc.client);
                true
            }
            Err(SendError::NoSlot | SendError::Full) => {
                self.waiting[client].push_back(index);
                false
            }
            Err(_) => {
                self.issued[index] = self.now;
                self.finish(index, NOT_SENT);
                false
            }
        }
    }

    /// Sends the waiting requests of client host `h` while slots are free.
    fn retry(&mut self, h: u16) {
        let client = usize::from(h);
        while let Some(&index) = self.waiting[client].front() {
            self.waiting[client].pop_front();
            if !self.send(index) {
                // `send` put the request back at the end, so it goes first
                // again at the next free slot.
                if let Some(back) = self.waiting[client].pop_back() {
                    self.waiting[client].push_front(back);
                }
                break;
            }
        }
    }

    /// Records the end of RPC `index`.
    fn finish(&mut self, index: usize, outcome: u8) {
        if self.outcome[index] != PENDING {
            return;
        }
        self.outcome[index] = outcome;
        self.done[index] = self.now;
        self.left = self.left.saturating_sub(1);
        self.progress += 1;
    }

    /// Returns the RPC the identifier `id` names, whichever end names it.
    fn rpc_of(&self, id: u64) -> Option<usize> {
        let key = id & !1;
        let list = self.ids.get(usize::from(client_of(key)))?;
        let at = list.partition_point(|&(known, _)| known < key);
        list.get(at)
            .filter(|&&(known, _)| known == key)
            .map(|&(_, index)| index)
    }

    /// Takes what arrived at host `h`, runs its timers, puts on its link
    /// what it hands out while the link is free, and takes its events.
    fn pump(&mut self, h: u16) {
        let index = usize::from(h);
        loop {
            let got = self.net.host(h).recv(&mut self.rx);
            let Ok(Some((from, len))) = got else {
                break;
            };
            self.progress += 1;
            let packet = self.rx[..len].to_vec();
            self.take(h, from, &packet);
        }
        // A clock that went backwards runs no timer, and the clock of a run
        // only moves forward.
        let _ = self.hosts[index].handle_timeout(self.now);
        self.decisions(h);
        self.transmit(h);
        let mut again = false;
        while let Some(event) = self.hosts[index].poll_event() {
            self.progress += 1;
            match event {
                Event::Request { id, .. } => {
                    self.answer(h, id);
                    again = true;
                }
                Event::Response { id } => {
                    // The response is read whole, so the client frees it and
                    // owes its ACK.
                    let _ = self.hosts[index].release(id);
                    if let Some(rpc) = self.rpc_of(id.get()) {
                        trace_put(
                            &mut self.trace,
                            vec![
                                "D".into(),
                                self.now.into(),
                                u32::from(h).into(),
                                id.get().into(),
                            ],
                        );
                        self.finish(rpc, ANSWERED);
                    }
                    self.retry(h);
                    again = true;
                }
                Event::Failed { id, why } => {
                    trace_put(
                        &mut self.trace,
                        vec![
                            "F".into(),
                            self.now.into(),
                            u32::from(h).into(),
                            id.get().into(),
                            failure_name(why).into(),
                        ],
                    );
                    if let Some(rpc) = self.rpc_of(id.get()) {
                        self.finish(rpc, FAILED);
                    }
                    self.retry(h);
                    again = true;
                }
                _ => {}
            }
        }
        if again {
            self.transmit(h);
        }
    }

    /// Hands one packet host `h` took from host `from` to its endpoint, and
    /// traces what it was.
    fn take(&mut self, h: u16, from: u16, packet: &[u8]) {
        let index = usize::from(h);
        let taken = self.hosts[index].handle(self.now, from, packet).is_ok();
        self.decisions(h);
        if self.trace.is_none() || !taken {
            return;
        }
        let Ok(decoded) = Full::decode(packet) else {
            return;
        };
        let row = match decoded.packet {
            Packet::Data(data) => vec![
                "R".into(),
                self.now.into(),
                u32::from(h).into(),
                (data.id & !1).into(),
                data.offset.into(),
                count(data.payload.len()).into(),
                data.msg_length.into(),
                u32::from(from).into(),
            ],
            Packet::StartMsg { id, msg_length } => vec![
                "M".into(),
                self.now.into(),
                u32::from(h).into(),
                (id & !1).into(),
                msg_length.into(),
                u32::from(from).into(),
            ],
            _ => return,
        };
        trace_put(&mut self.trace, row);
    }

    /// Puts on the link of host `h` what its endpoint hands out while the
    /// link is free.
    fn transmit(&mut self, h: u16) {
        let index = usize::from(h);
        loop {
            let mut port = self.net.host(h);
            let Some(packet) = self.hosts[index].poll_transmit(self.now, |p, l| port.ready(p, l))
            else {
                break;
            };
            let bytes = [packet.header, packet.payload].concat();
            let (peer, priority) = (packet.peer, packet.priority);
            self.progress += 1;
            // The fabric never fails a send.
            let _ = port.send(peer, priority, &bytes, &[]);
            if self.trace.is_some() {
                self.outgoing(h, peer, &bytes);
            }
        }
    }

    /// Traces a control packet host `h` sent to host `to`.
    fn outgoing(&mut self, h: u16, to: u16, bytes: &[u8]) {
        let Ok(decoded) = Full::decode(bytes) else {
            return;
        };
        let row = match decoded.packet {
            Packet::Grant {
                id,
                offset,
                priority,
            } => vec![
                "G".into(),
                self.now.into(),
                u32::from(h).into(),
                (id & !1).into(),
                offset.into(),
                u32::from(priority.level()).into(),
                u32::from(to).into(),
            ],
            Packet::Resend {
                id, offset, length, ..
            } => vec![
                "S".into(),
                self.now.into(),
                u32::from(h).into(),
                (id & !1).into(),
                offset.into(),
                length.into(),
                u32::from(to).into(),
            ],
            _ => return,
        };
        trace_put(&mut self.trace, row);
    }

    /// Writes a `P` row for every decision of the grant scheduler of host
    /// `h` since the last call, with the host that sends the message.
    fn decisions(&mut self, h: u16) {
        let taken = self.hosts[usize::from(h)].grant_decisions();
        if self.trace.is_none() {
            return;
        }
        for decision in taken {
            let id = decision.id.get();
            let sender = if id & 1 == 1 {
                client_of(id & !1)
            } else {
                self.rpc_of(id)
                    .map_or(u16::MAX, |rpc| self.s.rpcs[rpc].server)
            };
            let row = vec![
                "P".into(),
                self.now.into(),
                u32::from(h).into(),
                (id & !1).into(),
                decision.offset.into(),
                u32::from(decision.level).into(),
                u32::from(decision.rank).into(),
                u32::from(decision.active).into(),
                decision.window.into(),
                decision.fifo.into(),
                u32::from(sender).into(),
            ];
            trace_put(&mut self.trace, row);
        }
    }

    /// Answers the whole request server host `h` names `id`.
    fn answer(&mut self, h: u16, id: RpcId) {
        let Some(rpc) = self.rpc_of(id.get()) else {
            return;
        };
        if self.delivered[rpc] == 0 {
            self.delivered[rpc] = self.now;
        }
        trace_put(
            &mut self.trace,
            vec![
                "D".into(),
                self.now.into(),
                u32::from(h).into(),
                (id.get() & !1).into(),
            ],
        );
        let len = usize::try_from(self.s.rpcs[rpc].response_len).unwrap_or(0);
        let bytes = self.zeros.get(..len).unwrap_or(&[]);
        // A reply the server cannot queue leaves the client to its timers,
        // which the outcome of the RPC shows.
        let _ = self.hosts[usize::from(h)].reply(id, bytes);
    }

    /// Returns what the run answers.
    fn report(self, stopped: &'static str) -> JsObject {
        let rpcs: Vec<JsValue> = (0..self.outcome.len())
            .map(|i| {
                JsValue::Array(vec![
                    self.issued[i].into(),
                    self.delivered[i].into(),
                    self.done[i].into(),
                    u32::from(self.outcome[i]).into(),
                ])
            })
            .collect();
        let mut sum = Stats::default();
        let mut per_host = Vec::new();
        for host in &self.hosts {
            let stats = host.stats();
            sum.sent += stats.sent;
            sum.received += stats.received;
            sum.refused += stats.refused;
            sum.resends += stats.resends;
            sum.restarts += stats.restarts;
            sum.peer_deaths += stats.peer_deaths;
            sum.grants_sent += stats.grants_sent;
            sum.start_msg_sent += stats.start_msg_sent;
            sum.incast_marked += stats.incast_marked;
            sum.incast_honoured += stats.incast_honoured;
            sum.cutoffs_sent += stats.cutoffs_sent;
            sum.probes_beyond_first += stats.probes_beyond_first;
            sum.freezes_dropped += stats.freezes_dropped;
            sum.wrong_port += stats.wrong_port;
            for (total, level) in sum.per_priority.iter_mut().zip(stats.per_priority) {
                *total += level;
            }
            per_host.push(JsValue::Object(stats_json(&stats)));
        }
        let cutoffs: Vec<JsValue> = self
            .cutoffs_after
            .iter()
            .map(|bytes| JsValue::Array(bytes.iter().map(|&b| JsValue::from(b)).collect()))
            .collect();
        let final_stats = self.net.queues();
        let mut payload = JsObject::new();
        payload.insert("stopped", stopped);
        payload.insert("end_ns", self.now);
        payload.insert("rpcs", rpcs);
        payload.insert("stats", stats_json(&sum));
        payload.insert("host_stats", per_host);
        payload.insert("cutoffs", cutoffs);
        payload.insert("drops", self.overflow);
        payload.insert("fabric", fabric_json(&final_stats));
        payload.insert(
            "window",
            self.snapshot
                .map_or(JsValue::Null, |stats| JsValue::Object(fabric_json(&stats))),
        );
        let most: Vec<JsValue> = (0..self.s.fabric.hosts)
            .map(|h| JsValue::from(self.net.port_most(h)))
            .collect();
        payload.insert("port_most", most);
        payload.insert(
            "trace",
            self.trace
                .map_or(JsValue::Null, |out| JsValue::Object(out.finish())),
        );
        payload
    }
}

/// Returns what the switch of a fabric saw as an object.
fn fabric_json(stats: &FabricStats) -> JsObject {
    let mut object = JsObject::with_capacity(3);
    object.insert("max_queue_bytes", stats.max_queue_bytes);
    object.insert("drops", stats.drops);
    let busy: Vec<JsValue> = stats
        .downlink_busy_permille
        .iter()
        .map(|&permille| JsValue::from(u32::from(permille)))
        .collect();
    object.insert("downlink_busy_permille", busy);
    object
}

/// The client and the server of the ports case, and the address each has.
const CLIENT_ADDR: [u8; 4] = [10, 0, 0, 1];
/// The address of the server of the ports case.
const SERVER_ADDR: [u8; 4] = [10, 0, 0, 2];

/// An end of the ports case, whose peers are addresses with their Homa port.
type Addressed = Endpoint<SocketAddr, Owned<SocketAddr>, Full>;

/// Returns the address of an end with the Homa port `port`.
fn addr(ip: [u8; 4], port: u16) -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::from(ip)), port)
}

/// Moves every packet the two ends of the ports case hand out to the other
/// end, naming the peer of a packet by the address of its sender and the
/// source port of its Homa header, as the IP driver of Network names it, and
/// returns the packets moved.
fn exchange(client: &mut Addressed, server: &mut Addressed, now: Nanos) -> u64 {
    let mut moved = 0u64;
    loop {
        let mut packets: Vec<(bool, Vec<u8>)> = Vec::new();
        while let Some(packet) = client.poll_transmit(now, |_, _| true) {
            packets.push((true, [packet.header, packet.payload].concat()));
        }
        while let Some(packet) = server.poll_transmit(now, |_, _| true) {
            packets.push((false, [packet.header, packet.payload].concat()));
        }
        if packets.is_empty() {
            return moved;
        }
        for (to_server, bytes) in packets {
            moved += 1;
            let sport = u16::from_be_bytes([bytes[0], bytes[1]]);
            // A refused packet is counted by the endpoint that refused it.
            let _ = if to_server {
                server.handle(now, addr(CLIENT_ADDR, sport), &bytes)
            } else {
                client.handle(now, addr(SERVER_ADDR, sport), &bytes)
            };
        }
    }
}

/// Answers the operation `ports`: one client that owns `client_ports` Homa
/// ports sends `rpcs_per_port` requests from each to one server, the server
/// answers each with its own bytes, `foreign_packets` DATA packets go to a
/// port the server does not own, and `outside_requests` requests name a
/// source port the client does not own.
///
/// The answer names every peer the server saw a request from with the
/// identifiers of its RPCs, the responses the client took by source port,
/// the refusals of the requests from outside the client's ports, and the
/// counters of both ends. With `down_case` it also runs the case of a shared
/// downlink whose granted message ends with its peer: two requests from two
/// source ports under one granted message, the link of the first taken down
/// after its first packets, and the second completing whole.
pub fn op_ports(request: &JsObject) -> Result<JsObject, JsString> {
    let fields = Fields::new(request, "ports");
    let server_port: u16 = fields.int_or("server_port", 3_000)?;
    let client_port: u16 = fields.int_or("client_port", 4_000)?;
    let ports: u16 = fields.int_or("client_ports", 4)?;
    let per_port: u32 = fields.int_or("rpcs_per_port", 25)?;
    let foreign: u32 = fields.int_or("foreign_packets", 1_000)?;
    let outside: u32 = fields.int_or("outside_requests", 100)?;
    let profile = request_profile(&fields)?;
    let config = |port: u16, extra: u16, first_id: u64| Config {
        peers: 16,
        client_slots: 64,
        server_slots: 64,
        page_len: 16_384,
        pages: 512,
        max_gaps: 16,
        size_buckets: 0,
        first_id,
        hash_key: [0x5a; 16],
        port,
        extra_ports: extra,
    };
    let client_config = config(client_port, ports.saturating_sub(1), 0x1000_0002);
    let server_config = config(server_port, 0, 0x2000_0002);
    let build = |config: Config| -> Result<Addressed, JsString> {
        Endpoint::new(config, profile, Owned::new(&config))
            .map_err(|error| JsString::from(format!("ports: {}", config_error_name(error))))
    };
    let mut client = build(client_config)?;
    let mut server = build(server_config)?;
    let target = addr(SERVER_ADDR, server_port);
    let mut sent = Vec::new();
    let mut answered = Vec::new();
    let mut taken = Vec::new();
    let mut now: Nanos = 0;
    for n in 0..per_port {
        for p in 0..ports {
            let port = client_port.saturating_add(p);
            let body = format!("port {port} request {n}").into_bytes();
            let options = RequestOptions {
                source_port: port,
                ..RequestOptions::default()
            };
            let id = client
                .request(target, &body, options)
                .map_err(|error| JsString::from(format!("ports: {}", send_error_name(&error))))?;
            sent.push((port, id.get()));
        }
        now += MS_STEP;
        settle(&mut client, &mut server, now, &mut answered, &mut taken);
    }
    // The timers of both ends run long enough for a request the server
    // refused while it held too many RPCs of a port to come back through
    // RESEND, and for the acknowledgements that free them.
    for _ in 0..SETTLE_STEPS {
        now += MS_STEP;
        settle(&mut client, &mut server, now, &mut answered, &mut taken);
    }
    let mut peers: Vec<(u16, Vec<u64>)> = Vec::new();
    for (peer, id) in answered {
        match peers.iter_mut().find(|(port, _)| *port == peer.port()) {
            Some((_, ids)) => ids.push(id),
            None => peers.push((peer.port(), vec![id])),
        }
    }
    take_responses(&mut client, &mut taken);
    let responses = client_responses(&sent, &taken);
    let foreign_counted = send_foreign(&mut server, foreign, client_port, server_port, now)?;
    let refusals = send_outside(
        &mut client,
        target,
        outside,
        client_port.wrapping_add(ports),
    );
    let mut payload = JsObject::new();
    let seen: Vec<JsValue> = peers
        .iter()
        .map(|(port, ids)| {
            let mut object = JsObject::with_capacity(2);
            object.insert("port", u32::from(*port));
            object.insert(
                "ids",
                ids.iter()
                    .map(|&id| JsValue::from(id))
                    .collect::<Vec<JsValue>>(),
            );
            JsValue::Object(object)
        })
        .collect();
    payload.insert("server_peers", seen);
    payload.insert("responses", responses);
    payload.insert("sent", count(sent.len()));
    payload.insert("foreign_counted", foreign_counted);
    payload.insert("outside", refusals);
    payload.insert("client_stats", stats_json(&client.stats()));
    payload.insert("server_stats", stats_json(&server.stats()));
    if fields.bool_or("down_case", false)? {
        payload.insert("down_case", down_case(profile, &config)?);
    }
    Ok(payload)
}

/// Hands `server` `count` DATA packets for a port it does not own, 1,000
/// above its own, and returns how many it counted in
/// [`Stats::wrong_port`].
fn send_foreign(
    server: &mut Addressed,
    count: u32,
    client_port: u16,
    server_port: u16,
    start: Nanos,
) -> Result<u64, JsString> {
    let before = server.stats().wrong_port;
    let mut now = start;
    for n in 0..count {
        let packet = Packet::Data(Data {
            id: 0x3000_0000 + 2 * u64::from(n),
            msg_length: 4,
            offset: 0,
            retransmit: false,
            ack: None,
            cutoff_version: 0,
            incast: false,
            payload: b"lost",
        });
        let mut header = [0u8; Full::MAX_HEADER];
        let ports = homa::wire::Ports {
            src: client_port,
            dst: server_port.wrapping_add(1_000),
        };
        let len = Full::header(&packet, ports, false, &mut header)
            .map_err(|_| JsString::from("ports: the foreign packet does not encode"))?;
        let mut bytes = header[..len].to_vec();
        bytes.extend_from_slice(b"lost");
        now += 10;
        // A packet to a foreign port is counted and never refused.
        let _ = server.handle(now, addr(CLIENT_ADDR, client_port), &bytes);
    }
    Ok(server.stats().wrong_port - before)
}

/// Sends `count` requests of `client` from source ports from `first` on,
/// which the client does not own, and returns what became of each.
fn send_outside(
    client: &mut Addressed,
    target: SocketAddr,
    count: u32,
    first: u16,
) -> Vec<JsValue> {
    (0..count)
        .map(|n| {
            let options = RequestOptions {
                source_port: first.wrapping_add(u16::try_from(n % 50).unwrap_or(0)),
                ..RequestOptions::default()
            };
            let outcome = match client.request(target, b"outside", options) {
                Ok(_) => "sent",
                Err(error) => send_error_name(&error),
            };
            JsValue::from(outcome)
        })
        .collect()
}

/// Answers every whole request of `server` with its own bytes, and returns
/// the peer and the identifier of each.
fn answer_all(server: &mut Addressed) -> Vec<(SocketAddr, u64)> {
    let mut answered = Vec::new();
    while let Some(event) = server.poll_event() {
        if let Event::Request { peer, id } = event {
            let body = read_all(server.request_of(id)).unwrap_or_default();
            // A reply that cannot be queued leaves the client to its timers.
            let _ = server.reply(id, &body);
            answered.push((peer, id.get()));
        }
    }
    answered
}

/// The step of the clock of the ports case, a millisecond.
const MS_STEP: Nanos = 1_000_000;

/// The steps the ports case runs after its last request, beyond the RPC
/// timeout of the Network profile.
const SETTLE_STEPS: u32 = 120;

/// Runs the timers of both ends at `now`, moves every packet, answers every
/// whole request and takes every whole response.
fn settle(
    client: &mut Addressed,
    server: &mut Addressed,
    now: Nanos,
    answered: &mut Vec<(SocketAddr, u64)>,
    taken: &mut Vec<(u64, Vec<u8>)>,
) {
    // A clock of the case only moves forward.
    let _ = client.handle_timeout(now);
    let _ = server.handle_timeout(now);
    exchange(client, server, now);
    answered.extend(answer_all(server));
    exchange(client, server, now);
    take_responses(client, taken);
}

/// Takes every whole response of `client` into `taken` with its identifier,
/// and frees its slot.
fn take_responses(client: &mut Addressed, taken: &mut Vec<(u64, Vec<u8>)>) {
    while let Some(event) = client.poll_event() {
        if let Event::Response { id } = event {
            let bytes = read_all(client.response(id)).unwrap_or_default();
            taken.push((id.get(), bytes));
            let _ = client.release(id);
        }
    }
}

/// Returns, for each source port, the responses the client took and whether
/// each equals its request.
fn client_responses(sent: &[(u16, u64)], taken: &[(u64, Vec<u8>)]) -> Vec<JsValue> {
    let mut by_port: Vec<(u16, u32, u32)> = Vec::new();
    for &(port, id) in sent {
        let got = taken.iter().find(|(taken_id, _)| *taken_id == id);
        let expect = sent
            .iter()
            .filter(|&&(p, _)| p == port)
            .position(|&(_, i)| i == id)
            .map(|n| format!("port {port} request {n}").into_bytes());
        let equal = got.is_some_and(|(_, bytes)| Some(bytes) == expect.as_ref());
        match by_port.iter_mut().find(|(p, _, _)| *p == port) {
            Some((_, answered, same)) => {
                *answered += u32::from(got.is_some());
                *same += u32::from(equal);
            }
            None => by_port.push((port, u32::from(got.is_some()), u32::from(equal))),
        }
    }
    by_port
        .into_iter()
        .map(|(port, answered, same)| {
            let mut object = JsObject::with_capacity(3);
            object.insert("port", u32::from(port));
            object.insert("answered", answered);
            object.insert("equal", same);
            JsValue::Object(object)
        })
        .collect()
}

/// Runs the case of a shared downlink whose granted message ends with its
/// peer, and answers what became of the second message.
fn down_case(
    profile: Profile,
    config: &dyn Fn(u16, u16, u64) -> Config,
) -> Result<JsObject, JsString> {
    let mut shared = profile;
    shared.max_overcommit = 1;
    shared.unsched_bytes = 1_000;
    shared.scheduling = homa::Scheduling::Prefix;
    shared.window = homa::Window::Fixed(1_000);
    shared.incast = None;
    let client_config = config(4_000, 1, 0x1000_0002);
    let server_config = config(3_000, 0, 0x2000_0002);
    let build = |config: Config| -> Result<Addressed, JsString> {
        Endpoint::new(config, shared, Owned::new(&config))
            .map_err(|error| JsString::from(format!("ports: {}", config_error_name(error))))
    };
    let mut client = build(client_config)?;
    let mut server = build(server_config)?;
    let target = addr(SERVER_ADDR, 3_000);
    let first = RequestOptions {
        source_port: 4_000,
        ..RequestOptions::default()
    };
    let second = RequestOptions {
        source_port: 4_001,
        ..RequestOptions::default()
    };
    let map = |error: SendError| JsString::from(format!("ports: {}", send_error_name(&error)));
    let a = client.request(target, &[1u8; 3_000], first).map_err(map)?;
    let b = client.request(target, &[2u8; 4_000], second).map_err(map)?;
    let mut now: Nanos = 1_000;
    exchange(&mut client, &mut server, now);
    let granted_first = server
        .grant_decisions()
        .first()
        .map(|decision| decision.id.get() ^ 1 == a.get());
    // The link of the first request's port goes down after its first
    // packets, and the scheduler of the shared downlink must grant the
    // second, since no packet of the first will run it again.
    let down = server.link_down(addr(CLIENT_ADDR, 4_000)).is_ok();
    let mut whole = false;
    for _ in 0..16 {
        now += 1_000;
        exchange(&mut client, &mut server, now);
        let _ = server.handle_timeout(now);
        let _ = client.handle_timeout(now);
        while let Some(event) = server.poll_event() {
            if let Event::Request { id, .. } = event {
                whole |= id.get() ^ 1 == b.get();
            }
        }
    }
    let mut object = JsObject::with_capacity(3);
    object.insert(
        "first_granted_first",
        granted_first.map_or(JsValue::Null, JsValue::from),
    );
    object.insert("link_down", down);
    object.insert("second_whole", whole);
    Ok(object)
}

/// Answers the operation `table`: the longest probe of the peer table at
/// `peers` peers, each an address of the Homa port `port` chosen without the
/// key, one address after the other from 10.0.0.1.
///
/// The table is the open addressing table of the engine, keyed by SipHash-1-3
/// under `hash_key`, over the buckets [`homa::Config::storage`] gives
/// `peers` peers. The answer names the probes beyond the first of the
/// longest lookup, their mean and their sum, and the probes an endpoint of
/// the same size counted while it admitted every peer from its first packet.
pub fn op_table(request: &JsObject) -> Result<JsObject, JsString> {
    let fields = Fields::new(request, "table");
    let peers: u32 = fields.int_or("peers", 4_096)?;
    let port: u16 = fields.int_or("port", 3_000)?;
    let key = match fields.opt_str("hash_key")? {
        Some(text) => {
            let bytes = unhex(text).ok_or_else(|| fields.refuse("hash_key", "is no hex"))?;
            <[u8; 16]>::try_from(bytes.as_slice())
                .map_err(|_| fields.refuse("hash_key", "is not 16 bytes"))?
        }
        None => [0x5a; 16],
    };
    let address = |n: u32| {
        let host = n + 1;
        let octets = [
            10,
            0,
            u8::try_from(host >> 8).unwrap_or(u8::MAX),
            u8::try_from(host & 0xff).unwrap_or(0),
        ];
        addr(octets, port)
    };
    let capacity = homa::table::capacity(peers)
        .ok_or_else(|| fields.refuse("peers", "is past the table's reach"))?;
    let mut buckets = vec![homa::table::Bucket::VACANT; usize::try_from(capacity).unwrap_or(0)];
    let mut inserted = 0u32;
    for n in 0..peers {
        let mut probes = 0u64;
        let hash = homa::table::hash(&key, &address(n));
        if homa::table::insert(&mut buckets, hash, n, &mut probes).is_ok() {
            inserted += 1;
        }
    }
    let mut longest = 0u64;
    let mut sum = 0u64;
    for n in 0..peers {
        let mut probes = 0u64;
        let hash = homa::table::hash(&key, &address(n));
        let found = homa::table::find(&buckets, hash, &mut probes, |i| i == n);
        if found.is_some() {
            longest = longest.max(probes);
            sum += probes;
        }
    }
    let config = Config {
        peers,
        client_slots: 4,
        server_slots: 4,
        page_len: 1_024,
        pages: 8,
        max_gaps: 2,
        size_buckets: 0,
        first_id: 2,
        hash_key: key,
        port,
        extra_ports: 0,
    };
    let profile = Profile::network(1_500, homa::Encap::Ip4);
    let mut endpoint: Addressed = Endpoint::new(config, profile, Owned::new(&config))
        .map_err(|error| JsString::from(format!("table: {}", config_error_name(error))))?;
    let mut header = [0u8; Full::MAX_HEADER];
    let mut admitted = 0u32;
    for n in 0..peers {
        let packet = Packet::Busy {
            id: 2 * u64::from(n) + 2,
        };
        let ports = homa::wire::Ports {
            src: port,
            dst: port,
        };
        let len = Full::header(&packet, ports, false, &mut header)
            .map_err(|_| JsString::from("table: BUSY does not encode"))?;
        if endpoint
            .handle(u64::from(n), address(n), &header[..len])
            .is_ok()
        {
            admitted += 1;
        }
    }
    let mut payload = JsObject::new();
    payload.insert("peers", peers);
    payload.insert("buckets", capacity);
    payload.insert("inserted", inserted);
    payload.insert("longest_probe", longest);
    payload.insert("probes_sum", sum);
    payload.insert(
        "mean_probe_milli",
        if peers == 0 {
            0
        } else {
            sum * 1_000 / u64::from(peers)
        },
    );
    payload.insert("admitted", admitted);
    payload.insert(
        "endpoint_probes_beyond_first",
        endpoint.stats().probes_beyond_first,
    );
    Ok(payload)
}

/// The radio model of a scenario on the link `radio`, read from its object
/// `radio`.
///
/// The object names the pairing of both carriers, `link`, the `key` of 32
/// bytes, `sf`, `band`, `frequency_hz` and `max_rpcs` as the primitive `lora`
/// reads a pairing, and the model: `turnaround_ns`, the carrier's
/// [`lora::TURNAROUND`] where absent, `loss_permille`, 0 where absent,
/// `interferers`, the radios that transmit at random beside A and B, 0 where
/// absent, `interference_permille`, the share of the air each interferer
/// takes, the duty cycle of the band where absent, and `baseline`, which
/// sends whole messages in sealed frames with no Homa in place of the
/// endpoints, false where absent.
#[derive(Debug, Clone, Copy)]
struct RadioSpec {
    /// The pairing of both carriers.
    pairing: Pairing,
    /// The time a radio takes from a call to the start of its transmission.
    turnaround: Nanos,
    /// The receptions lost to noise, per mille.
    loss_permille: u16,
    /// The radios that transmit at random.
    interferers: u8,
    /// The share of the air each interferer takes, per mille.
    interference_permille: u16,
    /// Whether whole messages travel in sealed frames with no Homa.
    baseline: bool,
}

impl RadioSpec {
    /// Reads the radio model from the object `radio` of a scenario.
    fn read(fields: &Fields<'_>) -> Result<Self, JsString> {
        let pairing = pairing_of(fields, "radio")?;
        let spec = fields.nested("radio")?;
        let band = band_of(&spec, "band")?;
        Ok(Self {
            pairing,
            turnaround: spec.int_or("turnaround_ns", lora::TURNAROUND)?,
            loss_permille: spec.int_or("loss_permille", 0)?,
            interferers: spec.int_or("interferers", 0)?,
            interference_permille: spec.int_or("interference_permille", band.duty_permille())?,
            baseline: spec.bool_or("baseline", false)?,
        })
    }
}

/// The frames one radio of a run sent, each with the time it was asked for.
type Sent = Rc<RefCell<Vec<(Nanos, Vec<u8>)>>>;

/// Two carriers of `LoRa` over two radios of one net of the radio model, A
/// the client, which sends toward the device, and B the device, which sends
/// toward the client, beside the interferers of the scenario.
struct RadioLink<'b> {
    /// The net of the radios.
    net: Rc<RefCell<RadioNet>>,
    /// The carriers of A and of B.
    ends: [Lora<'b, ModelRadio, SharedStore>; 2],
    /// The frames each carrier sent.
    sent: [Sent; 2],
    /// The storage of the counters of each carrier, which a restart keeps.
    stores: [SharedStore; 2],
    /// What the net recorded.
    trace: Trace,
    /// The boots of each carrier.
    boots: [u32; 2],
    /// The calls of a carrier that failed.
    errors: u64,
    /// The link of the pairing.
    link: u8,
}

impl<'b> RadioLink<'b> {
    /// Builds the net and both carriers over the ledgers `a` and `b`, the
    /// net from the seed, the delays of each carrier from its own draw of it.
    fn new(
        spec: &RadioSpec,
        seed: u64,
        a: &'b mut [u32; lora::BUCKETS],
        b: &'b mut [u32; lora::BUCKETS],
    ) -> Result<Self, String> {
        let radio = Radio {
            sf: spec.pairing.modulation.spreading_factor(),
            duty_permille: spec.interference_permille,
            turnaround: spec.turnaround,
            loss_permille: spec.loss_permille,
        };
        let radios = spec.interferers.saturating_add(2);
        let net = Rc::new(RefCell::new(radio.build(
            radios,
            spec.interferers,
            Rng::new(seed),
        )));
        let sent: [Sent; 2] = [Rc::default(), Rc::default()];
        let stores = [SharedStore::default(), SharedStore::default()];
        let mut delays = Rng::new(seed).stream("backoff");
        let mut seed_of = || u32::try_from(delays.next_u64() >> 32).unwrap_or(1);
        let end_a = Lora::new(
            ModelRadio::new(Rc::clone(&net), 0, Rc::clone(&sent[0])),
            spec.pairing,
            Direction::ToDevice,
            stores[0].clone(),
            a,
            seed_of(),
        )
        .map_err(|error| format!("carrier A: {error}"))?;
        let end_b = Lora::new(
            ModelRadio::new(Rc::clone(&net), 1, Rc::clone(&sent[1])),
            spec.pairing,
            Direction::ToClient,
            stores[1].clone(),
            b,
            seed_of(),
        )
        .map_err(|error| format!("carrier B: {error}"))?;
        Ok(Self {
            net,
            ends: [end_a, end_b],
            sent,
            stores,
            trace: Trace::default(),
            boots: [1, 1],
            errors: 0,
            link: spec.pairing.link,
        })
    }
}

/// One end of the radio link as a carrier whose peer is the other end.
struct RadioEnd<'a, 'b> {
    /// The carrier of the end.
    lora: &'a mut Lora<'b, ModelRadio, SharedStore>,
    /// The end.
    side: Side,
    /// The link of the pairing.
    link: u8,
    /// The count of calls that failed.
    errors: &'a mut u64,
}

impl Carrier for RadioEnd<'_, '_> {
    type Peer = Side;
    type Error = LoraError<ModelError, Infallible>;

    fn ready(&self, _peer: Side, len: usize) -> bool {
        self.lora.ready(self.link, len)
    }

    fn send(
        &mut self,
        _peer: Side,
        priority: Priority,
        header: &[u8],
        payload: &[u8],
    ) -> Result<(), Self::Error> {
        let sent = self.lora.send(self.link, priority, header, payload);
        if sent.is_err() {
            *self.errors += 1;
        }
        sent
    }

    fn recv(&mut self, buf: &mut [u8]) -> Result<Option<(Side, usize)>, Self::Error> {
        Ok(self
            .lora
            .recv(buf)?
            .map(|(_, len)| (self.side.other(), len)))
    }
}

impl<'b> Net for RadioLink<'b> {
    type End<'a>
        = RadioEnd<'a, 'b>
    where
        Self: 'a;

    fn end(&mut self, side: Side) -> RadioEnd<'_, 'b> {
        RadioEnd {
            lora: &mut self.ends[index_of(side)],
            side,
            link: self.link,
            errors: &mut self.errors,
        }
    }

    fn step(&mut self, now: Nanos) -> Option<Nanos> {
        self.net.borrow_mut().step(now, &mut self.trace);
        let mut next: Option<Nanos> = None;
        for end in &mut self.ends {
            match end.advance(now) {
                Ok(Some(time)) => next = Some(next.map_or(time, |n| n.min(time))),
                Ok(None) => {}
                Err(_) => self.errors += 1,
            }
        }
        let upcoming = self.net.borrow_mut().step(now, &mut self.trace);
        match (next, upcoming) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    fn injected(&self) -> Option<Injected> {
        None
    }

    fn restart(&mut self, side: Side) {
        let index = index_of(side);
        if self.ends[index].reboot().is_err() {
            self.errors += 1;
        }
        self.boots[index] += 1;
    }

    fn report(&self) -> Option<JsObject> {
        let mut air = Vec::new();
        let mut lost = [0u64; 3];
        for event in &self.trace.events {
            match *event {
                TraceEvent::Air {
                    at,
                    radio,
                    air: time,
                    len,
                    collided,
                } => air.push(JsValue::Array(vec![
                    u32::from(radio).into(),
                    at.into(),
                    time.into(),
                    u32::from(len).into(),
                    collided.into(),
                ])),
                TraceEvent::Lost { reason, .. } => match reason {
                    "collision" => lost[0] += 1,
                    "half duplex" => lost[1] += 1,
                    _ => lost[2] += 1,
                },
                _ => {}
            }
        }
        let mut losses = JsObject::with_capacity(3);
        losses.insert("collision", lost[0]);
        losses.insert("half_duplex", lost[1]);
        losses.insert("noise", lost[2]);
        let mut carriers = JsObject::with_capacity(2);
        let mut frames = JsObject::with_capacity(2);
        let mut boots = JsObject::with_capacity(2);
        let mut stores = JsObject::with_capacity(2);
        for side in SIDES {
            let index = index_of(side);
            carriers.insert(side_name(side), lora_stats_json(&self.ends[index].stats()));
            let rows: Vec<JsValue> = self.sent[index]
                .borrow()
                .iter()
                .map(|(at, frame)| {
                    let counter = frame.get(1..5).map_or(u32::MAX, |bytes| {
                        u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
                    });
                    JsValue::Array(vec![
                        (*at).into(),
                        counter.into(),
                        count(frame.len()).into(),
                    ])
                })
                .collect();
            frames.insert(side_name(side), rows);
            boots.insert(side_name(side), self.boots[index]);
            let direction = if side == Side::A {
                Direction::ToDevice
            } else {
                Direction::ToClient
            };
            stores.insert(side_name(side), self.stores[index].value(direction));
        }
        let net = self.net.borrow();
        let stats = net.stats();
        let mut payload = JsObject::new();
        payload.insert("air", air);
        payload.insert("lost", losses);
        payload.insert("collisions", net.collisions());
        payload.insert("transmissions", stats.transmissions);
        payload.insert("delivered", stats.delivered);
        payload.insert("carriers", carriers);
        payload.insert("frames", frames);
        payload.insert("boots", boots);
        payload.insert("reserved", stores);
        payload.insert("errors", self.errors);
        Some(payload)
    }
}

/// One message on its way out of an end of the radio baseline: its RPC,
/// whether it is the request, and the bytes left to send.
type Outgoing = (usize, bool, Vec<u8>);

/// The state of one run of whole messages in sealed frames with no Homa over
/// the radio model, the baseline of `LoRa`.
///
/// Each message travels as its length in 4 bytes big endian and its bytes,
/// cut into packets of the link's `max_packet` and sealed one a frame by the
/// carrier, first in first out. A sends each request at its `at_ns`, B
/// answers each request whole once it arrived, and nothing is sent again, so
/// a lost frame loses its message and the stream behind it.
struct RadioBaselineRun<'s> {
    /// The scenario.
    s: &'s Scenario,
    /// The time the clock reads.
    now: Nanos,
    /// One record a RPC.
    records: Vec<Record>,
    /// The messages each end sends, in order.
    outbox: [VecDeque<Outgoing>; 2],
    /// The bytes each end received and has not read as a message.
    inbox: [Vec<u8>; 2],
    /// The RPCs whose messages each end expects, in order.
    expect: [VecDeque<usize>; 2],
    /// What each end put on the link.
    tallies: [Tally; 2],
    /// The next RPC to send.
    next_issue: usize,
    /// RPCs not ended.
    left: usize,
    /// The longest packet of the link.
    max_packet: usize,
    /// Changes at the time the clock reads.
    progress: u64,
    /// The trace.
    trace: Option<TraceOut>,
}

impl<'s> RadioBaselineRun<'s> {
    /// Returns the run of a scenario over the radio baseline.
    fn new(s: &'s Scenario, spec: &RadioSpec, trace: Option<TraceOut>) -> Self {
        let sf = spec.pairing.modulation.spreading_factor();
        Self {
            s,
            now: 0,
            records: vec![Record::new(); s.rpcs.len()],
            outbox: [VecDeque::new(), VecDeque::new()],
            inbox: [Vec::new(), Vec::new()],
            expect: [VecDeque::new(), VecDeque::new()],
            tallies: [Tally::default(); 2],
            next_issue: 0,
            left: s.rpcs.len(),
            max_packet: usize::from(spec.pairing.band.max_packet(sf)),
            progress: 0,
            trace,
        }
    }

    /// Returns `message` behind its length in 4 bytes big endian.
    fn prefixed(message: &[u8]) -> Vec<u8> {
        let len = u32::try_from(message.len()).unwrap_or(u32::MAX);
        let mut out = Vec::with_capacity(message.len() + 4);
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(message);
        out
    }

    /// Moves the run through time until every RPC ended and returns what it
    /// answers.
    fn run(mut self, net: &mut RadioLink<'_>) -> JsObject {
        let mut passes = 0u32;
        let stopped = loop {
            let before = self.progress;
            self.issue();
            net.step(self.now);
            for side in SIDES {
                self.pump(net, side);
            }
            if self.left == 0 {
                break "done";
            }
            if self.progress != before {
                passes += 1;
                if passes > MAX_PASSES {
                    break "livelock";
                }
                continue;
            }
            passes = 0;
            let issue = self.s.rpcs.get(self.next_issue).map(|spec| spec.at);
            let upcoming = [net.step(self.now), issue]
                .into_iter()
                .flatten()
                .filter(|&time| time > self.now)
                .min();
            let Some(upcoming) = upcoming else {
                break "stuck";
            };
            if upcoming > self.s.max_time {
                break "max_time";
            }
            self.now = upcoming;
        };
        let rpcs: Vec<JsValue> = self
            .records
            .iter()
            .enumerate()
            .map(|(index, record)| JsValue::Object(record.json(index)))
            .collect();
        let mut links = JsObject::with_capacity(2);
        links.insert("A", self.tallies[0].json());
        links.insert("B", self.tallies[1].json());
        let mut payload = JsObject::new();
        payload.insert("stopped", stopped);
        payload.insert("end_ns", self.now);
        payload.insert("rpcs", rpcs);
        payload.insert("links", links);
        payload.insert("radio", net.report().map_or(JsValue::Null, JsValue::Object));
        payload.insert(
            "trace",
            self.trace
                .map_or(JsValue::Null, |out| JsValue::Object(out.finish())),
        );
        payload
    }

    /// Queues the requests whose time has come on A.
    fn issue(&mut self) {
        while let Some(spec) = self.s.rpcs.get(self.next_issue) {
            if spec.at > self.now {
                return;
            }
            let index = self.next_issue;
            let bytes = Self::prefixed(&spec.fill.bytes(spec.len));
            self.outbox[0].push_back((index, true, bytes));
            self.expect[1].push_back(index);
            self.records[index].issued = Some(self.now);
            self.next_issue += 1;
            self.progress += 1;
        }
    }

    /// Takes what arrived at `side`, reads every whole message out of it, and
    /// puts the next packets of its messages on the link while it is ready.
    fn pump(&mut self, net: &mut RadioLink<'_>, side: Side) {
        let index = index_of(side);
        let mut buf = [0u8; lora::MAX_PAYLOAD];
        loop {
            let got = net.end(side).recv(&mut buf);
            let Ok(Some((_, len))) = got else {
                break;
            };
            self.inbox[index].extend_from_slice(&buf[..len]);
            self.progress += 1;
        }
        while self.inbox[index].len() >= 4 {
            let head = &self.inbox[index];
            let len = u32::from_be_bytes([head[0], head[1], head[2], head[3]]);
            let len = usize::try_from(len).unwrap_or(usize::MAX);
            if head.len() - 4 < len {
                break;
            }
            let message: Vec<u8> = self.inbox[index].drain(..4 + len).skip(4).collect();
            self.deliver(side, &message);
        }
        while let Some((_, _, bytes)) = self.outbox[index].front() {
            let chunk = bytes.len().min(self.max_packet);
            let mut end = net.end(side);
            if !end.ready(side.other(), chunk) {
                break;
            }
            let sent = end.send(side.other(), Priority::HIGHEST, &bytes[..chunk], &[]);
            let tally = &mut self.tallies[index];
            tally.packets += 1;
            tally.bytes += count(chunk);
            tally.frame_bytes += count(chunk + lora::OVERHEAD);
            if sent.is_err() {
                tally.dropped += 1;
            }
            self.progress += 1;
            if let Some((_, _, bytes)) = self.outbox[index].front_mut() {
                bytes.drain(..chunk);
                if bytes.is_empty() {
                    self.outbox[index].pop_front();
                }
            }
        }
    }

    /// Takes a whole message that arrived at `side`.
    fn deliver(&mut self, side: Side, message: &[u8]) {
        let index = index_of(side);
        let Some(rpc) = self.expect[index].pop_front() else {
            return;
        };
        let now = self.now;
        let record = &mut self.records[rpc];
        if side == Side::B {
            record.server_requests += 1;
            record.request_at = Some(now);
            record.request_sha = Some(sha256_hex(message));
            let spec = &self.s.rpcs[rpc];
            let answer = Self::prefixed(&spec.response_fill.bytes(spec.response_len));
            self.outbox[1].push_back((rpc, false, answer));
            self.expect[0].push_back(rpc);
        } else {
            record.client_responses += 1;
            record.response_at = Some(now);
            record.response_sha = Some(sha256_hex(message));
            record.response_len = Some(u32::try_from(message.len()).unwrap_or(u32::MAX));
            if !record.ended() {
                record.outcome = Outcome::Ok;
                record.ended_at = Some(now);
                self.left = self.left.saturating_sub(1);
            }
        }
        let row = vec![
            "D".into(),
            now.into(),
            side_name(side).into(),
            count(rpc).into(),
            count(message.len()).into(),
        ];
        trace_put(&mut self.trace, row);
        self.progress += 1;
    }
}
