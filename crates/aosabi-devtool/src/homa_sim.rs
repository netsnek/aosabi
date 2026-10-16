// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool homa_sim: runs a scenario of Homa RPCs over a simulated link
 * and reports the RPCs, the counters and the trace.
 *
 * No analogue in the reference. Its judging notebooks are
 * tests/notebooks/21_homa_loopback.ipynb, tests/notebooks/22_homa_lossy.ipynb
 * and tests/notebooks/23_homa_budget.ipynb, whose checks over a trace are
 * tests/harness/homa/trace.py.
 */

//! The `homa-sim` primitive of the seam.
//!
//! `--requests PATH` answers a file of request lines, one response line each,
//! in the batch form of [`crate::batch`]. Seven operations exist.
//!
//! - `run` runs one scenario between two ends, [`scenario`] says which
//!   fields it takes, and answers `rpcs`, `stats` and the path and length of
//!   its `trace`.
//! - `fabric` runs RPCs between the hosts of a simulated rack, one
//!   endpoint of the full form a host, [`scenario::op_fabric`] says which
//!   fields it takes.
//! - `ports` runs the case of one client that owns several Homa ports
//!   against one server whose peers are addresses with their Homa port,
//!   [`scenario::op_ports`] says which fields it takes.
//! - `table` answers the longest probe of the peer table of the engine at a
//!   number of peers, [`scenario::op_table`] says which fields it takes.
//! - `sizes` answers the bytes of the types of an endpoint on this host and
//!   the parts of the storage a `config` needs in a card's build, so a
//!   notebook reads the budget of section 8.3 of the design note from the
//!   crate.
//! - `build` builds an endpoint of `config` and `profile` over owned storage
//!   and answers `ok` or `error` with the name of the `ConfigError`.
//! - `profile` answers every value of the `profile` it names.
//!
//! A `profile` names a constructor of [`homa::Profile`] with its arguments,
//! and `overrides` replaces single values of it, as a card writes its own
//! values into the fields where it knows better. A `config` names the fields
//! of [`homa::Config`].

#[cfg(target_os = "linux")]
mod pty;
pub(crate) mod scenario;
#[cfg(target_os = "linux")]
mod socket;

use std::{
    mem,
    net::SocketAddr,
    path::PathBuf,
    process::ExitCode, //
};

use aosabi_json::{
    JsObject,
    JsString,
    JsValue, //
};
use homa::{
    Config,
    ConfigError,
    Encap,
    Endpoint,
    Owned,
    Parts,
    Peer,
    Profile,
    Scheduling,
    Slot,
    StorageLen,
    Timers,
    Window,
    heap::Item,
    sim::Side,
    stream::{
        Decoder,
        Mode, //
    },
    table::Bucket,
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
        LANDS_AT,
        count,
        hex,
        unhex, //
    },
    seam, //
};

/// The name of the primitive.
const NAME: &str = "homa-sim";

/// The command line of the `homa-sim` primitive.
pub static GRAMMAR: Grammar = Grammar {
    options: &[cli::SMOKE, cli::REQUESTS],
    positional: None,
    conflicts: &[],
};

/// What the `homa-sim` primitive accepts.
#[derive(Debug)]
pub struct HomaSimArgs {
    /// Whether the call asks for the interface line.
    smoke: bool,
    /// The file of request lines of the batch form.
    requests: Option<PathBuf>,
}

impl HomaSimArgs {
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
    ("run", scenario::op_run),
    ("fabric", scenario::op_fabric),
    ("ports", scenario::op_ports),
    ("table", scenario::op_table),
    ("sizes", op_sizes),
    ("build", op_build),
    ("profile", op_profile),
];

/// Runs the `homa-sim` primitive.
pub fn run(args: &HomaSimArgs) -> ExitCode {
    if args.smoke {
        return seam::smoke_at(NAME, LANDS_AT);
    }
    let outcome = seam::required(args.requests.as_deref(), "--requests")
        .and_then(|path| batch::run(OPERATIONS, path));
    seam::finish(NAME, outcome)
}

/// Returns the profile the object `spec` names, with the values of
/// `overrides` written into it and the scheduling `mode` names.
fn profile_of(
    spec: &Fields<'_>,
    overrides: Option<&Fields<'_>>,
    mode: Option<&str>,
) -> Result<Profile, JsString> {
    let mut profile = match spec.str("name")? {
        "unix_socket" => Profile::unix_socket(spec.int_or("max_message", 65_536)?),
        "usb_bulk" => Profile::usb_bulk(spec.bool_or("high_speed", true)?),
        "usb_serial" => Profile::usb_serial(),
        "bluetooth_channel" => Profile::bluetooth_channel(
            spec.int_or("peer_mps", 247)?,
            spec.int_or("interval_ns", 30_000_000)?,
        ),
        "bluetooth_gatt" => Profile::bluetooth_gatt(
            spec.int_or("att_mtu", 185)?,
            spec.int_or("interval_ns", 30_000_000)?,
        ),
        "serial_line" => Profile::serial_line(spec.int_or("baud", 115_200)?),
        "lora" => Profile::lora(spec.int_or("sf", 7)?, spec.int_or("band_permille", 100)?),
        "network" => {
            let encap = if spec.bool_or("ipv6", false)? {
                Encap::Ip6
            } else {
                Encap::Ip4
            };
            Profile::network(spec.int_or("mtu", 1_500)?, encap)
        }
        other => return Err(spec.refuse("name", &format!("names no profile: {other}"))),
    };
    if let Some(values) = overrides {
        apply_overrides(&mut profile, values)?;
    }
    match mode {
        None => {}
        Some("prefix") => profile.scheduling = Scheduling::Prefix,
        Some("all-or-nothing") => profile.scheduling = Scheduling::AllOrNothing,
        Some(other) => {
            return Err(spec.refuse("mode", &format!("names no scheduling mode: {other}")));
        }
    }
    Ok(profile)
}

/// Writes the values `values` names into `profile`.
fn apply_overrides(profile: &mut Profile, values: &Fields<'_>) -> Result<(), JsString> {
    profile.max_packet = values.int_or("max_packet", profile.max_packet)?;
    profile.unsched_bytes = values.int_or("unsched_bytes", profile.unsched_bytes)?;
    if let Some(window) = values.opt_u64("window")? {
        let bytes =
            u32::try_from(window).map_err(|_| values.refuse("window", "is out of range"))?;
        profile.window = Window::Fixed(bytes);
    }
    profile.max_incoming = values.int_or("max_incoming", profile.max_incoming)?;
    profile.max_overcommit = values.int_or("max_overcommit", profile.max_overcommit)?;
    profile.fifo_permille = values.int_or("fifo_permille", profile.fifo_permille)?;
    profile.fifo_grant_increment =
        values.int_or("fifo_grant_increment", profile.fifo_grant_increment)?;
    profile.fifo_every = values.int_or("fifo_every", profile.fifo_every)?;
    if let Some(incast) = values.opt_object("incast")? {
        let incast = Fields::new(incast, values.op_name());
        profile.incast = Some(homa::Incast {
            threshold: incast.int_or("threshold", 8)?,
            unsched_bytes: incast.int_or("unsched_bytes", 300)?,
        });
    }
    profile.batch_grants = values.bool_or("batch_grants", profile.batch_grants)?;
    profile.resend_raises_grant =
        values.bool_or("resend_raises_grant", profile.resend_raises_grant)?;
    let timers = &mut profile.timers;
    timers.resend_after = values.int_or("resend_after_ns", timers.resend_after)?;
    timers.resend_interval = values.int_or("resend_interval_ns", timers.resend_interval)?;
    timers.patient_after = values.int_or("patient_after_ns", timers.patient_after)?;
    timers.need_ack_after = values.int_or("need_ack_after_ns", timers.need_ack_after)?;
    timers.rpc_timeout = values.int_or("rpc_timeout_ns", timers.rpc_timeout)?;
    timers.timeout_resends = values.int_or("timeout_resends", timers.timeout_resends)?;
    Ok(())
}

/// Returns the profile of a request, from its fields `profile`, `overrides`
/// and `mode`.
pub(crate) fn request_profile(fields: &Fields<'_>) -> Result<Profile, JsString> {
    let spec = fields.nested("profile")?;
    let overrides = match fields.opt_object("overrides")? {
        Some(_) => Some(fields.nested("overrides")?),
        None => None,
    };
    profile_of(&spec, overrides.as_ref(), fields.opt_str("mode")?)
}

/// Returns the configuration `spec` names, each field absent taking its value
/// from `defaults`.
pub(crate) fn config_of(spec: Option<&Fields<'_>>, defaults: Config) -> Result<Config, JsString> {
    let Some(spec) = spec else {
        return Ok(defaults);
    };
    let hash_key = match spec.opt_str("hash_key")? {
        Some(text) => {
            let bytes = unhex(text)
                .ok_or_else(|| spec.refuse("hash_key", "is no even run of hex digits"))?;
            <[u8; 16]>::try_from(bytes.as_slice())
                .map_err(|_| spec.refuse("hash_key", "is not 16 bytes"))?
        }
        None => defaults.hash_key,
    };
    Ok(Config {
        peers: spec.int_or("peers", defaults.peers)?,
        client_slots: spec.int_or("client_slots", defaults.client_slots)?,
        server_slots: spec.int_or("server_slots", defaults.server_slots)?,
        page_len: spec.int_or("page_len", defaults.page_len)?,
        pages: spec.int_or("pages", defaults.pages)?,
        max_gaps: spec.int_or("max_gaps", defaults.max_gaps)?,
        size_buckets: spec.int_or("size_buckets", defaults.size_buckets)?,
        first_id: spec.int_or("first_id", defaults.first_id)?,
        hash_key,
        port: spec.int_or("port", defaults.port)?,
        extra_ports: spec.int_or("extra_ports", defaults.extra_ports)?,
    })
}

/// The configuration a request that names none takes: one peer, eight slots
/// of each role and sixteen pages of 4,096 bytes a slot.
const DEFAULT_CONFIG: Config = Config {
    peers: 1,
    client_slots: 8,
    server_slots: 8,
    page_len: 4_096,
    pages: 256,
    max_gaps: 2,
    size_buckets: 0,
    first_id: 2,
    hash_key: [0x5a; 16],
    port: 0,
    extra_ports: 0,
};

/// Returns the name of a configuration error, the name of its variant with
/// the feature a `Missing` names.
pub(crate) fn config_error_name(error: ConfigError) -> String {
    match error {
        ConfigError::TooSmall => String::from("TooSmall"),
        ConfigError::Empty => String::from("Empty"),
        ConfigError::BadFirstId => String::from("BadFirstId"),
        ConfigError::Missing(feature) => format!("Missing:{feature}"),
    }
}

/// Answers the operation `build`.
fn op_build(request: &JsObject) -> Result<JsObject, JsString> {
    let fields = Fields::new(request, "build");
    let profile = request_profile(&fields)?;
    let spec = match fields.opt_object("config")? {
        Some(_) => Some(fields.nested("config")?),
        None => None,
    };
    let config = config_of(spec.as_ref(), DEFAULT_CONFIG)?;
    let built: Result<Endpoint<Side, Owned<Side>, Compact>, ConfigError> =
        Endpoint::new(config, profile, Owned::new(&config));
    let mut payload = JsObject::with_capacity(1);
    match built {
        Ok(_) => payload.insert("ok", true),
        Err(error) => payload.insert("error", config_error_name(error)),
    };
    Ok(payload)
}

/// Answers the operation `profile`.
fn op_profile(request: &JsObject) -> Result<JsObject, JsString> {
    let fields = Fields::new(request, "profile");
    let profile = request_profile(&fields)?;
    let mut payload = JsObject::with_capacity(1);
    payload.insert("profile", profile_json(&profile));
    Ok(payload)
}

/// Returns every value of a profile as an object.
pub fn profile_json(profile: &Profile) -> JsObject {
    let mut object = JsObject::new();
    object.insert(
        "scheduling",
        match profile.scheduling {
            Scheduling::Prefix => "prefix",
            Scheduling::AllOrNothing => "all-or-nothing",
        },
    );
    object.insert(
        "scope",
        match profile.scope {
            homa::Scope::PerPeer => "per-peer",
            homa::Scope::Shared => "shared",
        },
    );
    object.insert(
        "admission",
        match profile.admission {
            homa::Admission::Linked => "linked",
            homa::Admission::Open => "open",
        },
    );
    object.insert("max_packet", u32::from(profile.max_packet));
    object.insert("unsched_bytes", profile.unsched_bytes);
    match profile.window {
        Window::Fixed(bytes) => object.insert("window", bytes),
        Window::Dynamic => object.insert("window", "dynamic"),
    };
    object.insert("max_incoming", profile.max_incoming);
    object.insert("max_overcommit", u32::from(profile.max_overcommit));
    object.insert("fifo_permille", u32::from(profile.fifo_permille));
    object.insert("fifo_grant_increment", profile.fifo_grant_increment);
    object.insert("fifo_every", u32::from(profile.fifo_every));
    object.insert("batch_grants", profile.batch_grants);
    object.insert("resend_raises_grant", profile.resend_raises_grant);
    object.insert("timers", timers_json(&profile.timers));
    object.insert(
        "incast",
        profile.incast.map_or(JsValue::Null, |incast| {
            let mut value = JsObject::with_capacity(2);
            value.insert("threshold", u32::from(incast.threshold));
            value.insert("unsched_bytes", incast.unsched_bytes);
            JsValue::Object(value)
        }),
    );
    object
}

/// Returns the timers of a profile as an object, every duration in
/// nanoseconds.
fn timers_json(timers: &Timers) -> JsObject {
    let mut object = JsObject::with_capacity(6);
    object.insert("resend_after_ns", timers.resend_after);
    object.insert("resend_interval_ns", timers.resend_interval);
    object.insert("patient_after_ns", timers.patient_after);
    object.insert("need_ack_after_ns", timers.need_ack_after);
    object.insert("rpc_timeout_ns", timers.rpc_timeout);
    object.insert("timeout_resends", u32::from(timers.timeout_resends));
    object
}

/// Answers the operation `sizes`.
///
/// The sizes are those of this host, which the budget of the design note
/// counts on the RV32IMC core of the ESP32-C3 as well, since no type of the
/// engine holds a pointer.
///
/// The storage is that of a card's build, which leaves out the feature
/// `prio` and with it the part `peer_prio`, unless the request sets `prio`.
/// The devtool builds `homa` with `sim`, which turns `prio` on, so
/// [`homa::Config::storage`] of this build alone would count the priority
/// state of every peer, which a card never allocates.
fn op_sizes(request: &JsObject) -> Result<JsObject, JsString> {
    let fields = Fields::new(request, "sizes");
    let spec = match fields.opt_object("config")? {
        Some(_) => Some(fields.nested("config")?),
        None => None,
    };
    let config = config_of(spec.as_ref(), DEFAULT_CONFIG)?;
    let mut peers = JsObject::with_capacity(6);
    peers.insert("unit", count(mem::size_of::<Peer<()>>()));
    peers.insert("u8", count(mem::size_of::<Peer<u8>>()));
    peers.insert("u16", count(mem::size_of::<Peer<u16>>()));
    peers.insert("u32", count(mem::size_of::<Peer<u32>>()));
    peers.insert("side", count(mem::size_of::<Peer<Side>>()));
    peers.insert("socket_addr", count(mem::size_of::<Peer<SocketAddr>>()));
    let mut len = config.storage();
    if !fields.bool_or("prio", false)? {
        len.peer_prio = 0;
    }
    let mut bytes = JsObject::with_capacity(3);
    bytes.insert("unit", count(len.bytes::<()>()));
    bytes.insert("u8", count(len.bytes::<u8>()));
    bytes.insert("side", count(len.bytes::<Side>()));
    let mut payload = JsObject::new();
    payload.insert("slot", count(mem::size_of::<Slot>()));
    payload.insert("peer", peers);
    payload.insert("bucket", count(mem::size_of::<Bucket>()));
    payload.insert("item", count(mem::size_of::<Item>()));
    payload.insert(
        "endpoint_u8",
        count(mem::size_of::<Endpoint<u8, Parts<'static, u8>, Compact>>()),
    );
    payload.insert("storage", storage_json(&len));
    payload.insert("storage_bytes", bytes);
    payload.insert("buffers", buffers_json(&fields)?);
    payload.insert("hash_key", hex(&config.hash_key));
    Ok(payload)
}

/// Returns the parts of a storage as an object.
fn storage_json(len: &StorageLen) -> JsObject {
    let mut object = JsObject::with_capacity(9);
    object.insert("peers", count(len.peers));
    object.insert("slots", count(len.slots));
    object.insert("peer_index", count(len.peer_index));
    object.insert("slot_index", count(len.slot_index));
    object.insert("tx_heap", count(len.tx_heap));
    object.insert("grant_heap", count(len.grant_heap));
    object.insert("pages", count(len.pages));
    object.insert("gaps", count(len.gaps));
    object.insert("sizes", count(len.sizes));
    object.insert("peer_prio", count(len.peer_prio));
    object
}

/// Returns the decoder buffer of each form for every packet length the
/// request names in `max_packets`.
fn buffers_json(fields: &Fields<'_>) -> Result<Vec<JsValue>, JsString> {
    let mut out = Vec::new();
    for value in fields.array("max_packets")? {
        let max_packet = crate::homa_packet::whole(value)
            .and_then(|n| usize::try_from(n).ok())
            .ok_or_else(|| fields.refuse("max_packets", "holds a value that is no length"))?;
        let mut object = JsObject::with_capacity(5);
        object.insert("max_packet", count(max_packet));
        object.insert(
            "delimited",
            count(Decoder::buffer_len(Mode::Delimited, max_packet)),
        );
        object.insert(
            "prefixed",
            count(Decoder::buffer_len(Mode::Prefixed, max_packet)),
        );
        object.insert(
            "frame_delimited",
            count(homa::stream::frame_len(Mode::Delimited, max_packet)),
        );
        object.insert(
            "frame_prefixed",
            count(homa::stream::frame_len(Mode::Prefixed, max_packet)),
        );
        out.push(JsValue::Object(object));
    }
    Ok(out)
}
