// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool lora: the frame, the seal, the counters, the ledger and the
 * time on air of the carrier LoRa, and the radios the radio model of homa-sim
 * runs the carrier over.
 *
 * No analogue in the reference. Its judging notebooks are
 * tests/notebooks/06_lora_frame.ipynb and
 * tests/notebooks/29_carrier_lora.ipynb, whose references are
 * tests/harness/homa/lora.py and tests/harness/homa/chacha.py.
 */

//! The `lora` primitive of the seam.
//!
//! `--requests PATH` answers a file of request lines, one response line each,
//! in the batch form of [`crate::batch`]. Every byte string travels as
//! lowercase hex, a `pairing` is an object with `link` and the `key` of 32
//! bytes, a `direction` is `to_device` or `to_client`, and a `band` is `P`,
//! 10 percent, or `M`, 1 percent.
//!
//! - `toa` takes `sf`, `bandwidth_hz`, 125,000 where absent, `coding_rate`,
//!   1 for 4/5 where absent, and `len`, and answers `air_ns`.
//! - `profile` takes `sf`, `band` and `max_rpcs`, and answers the values of a
//!   link of the card: `profile`, `limits`, `longest_response`, `max_frame`,
//!   `max_packet`, `air_per_hour_ns`, `longest_transmission_ns`, `burst_ns`,
//!   `gap_ns` and `full_air_ns`.
//! - `aead` takes `key`, `nonce`, `aad` and `hex` and answers `hex`, the
//!   ciphertext, and `tag`, and `aead_open` takes `tag` beside them and
//!   answers `hex`, the plaintext, or `error`, which reach the AEAD of RFC
//!   8439 the frame seals with.
//! - `seal` takes `pairing`, `direction`, `counter` and `hex`, a packet, and
//!   answers `hex`, the frame, or `error` with the name of the `FrameError`.
//! - `open` takes `pairing`, `direction` and `frames` and answers `results`,
//!   for each frame in order its `counter` and `hex`, the packet, or `error`,
//!   through one replay window, as the carrier opens them.
//! - `receive` hands `frames` to the carrier of the end that receives in
//!   `direction` over a radio that holds them, and answers `packets`, what
//!   the carrier handed Homa, and `stats`, what it counted.
//! - `counters` takes `link`, `direction` and `uses`, and boots the counters
//!   of one store once for each entry of `uses`, taking that many counters
//!   each time, and answers `boots`, for each boot its `first` counter, its
//!   `count` and the `reserved` bound of the store after it.
//! - `restarts` takes `pairing`, `direction`, `sends` and `len`, and restarts
//!   one carrier once before each entry of `sends` but the first, sends that
//!   many packets of `len` bytes each time, and answers `frames`, for every
//!   frame on the air its `boot` and its `counter`.
//! - `ledger` takes `band` and `script`, a list of `[at_ns, air_ns,
//!   control]`, and asks one ledger for each entry whether it fits, charges
//!   it where it does, and answers `results`, `charged` or `refused` with the
//!   earliest start at which it fits, `next_fit_ns`.
//! - `sizes` answers the bytes of the carrier's types on this host.

use std::{
    cell::{
        Cell,
        RefCell, //
    },
    collections::VecDeque,
    convert::Infallible,
    mem,
    path::PathBuf,
    process::ExitCode,
    rc::Rc, //
};

use aosabi_carrier_lora::{
    self as lora,
    Band,
    CounterStore,
    Counters,
    Direction,
    FrameError,
    Ledger,
    Lora,
    LoraStats,
    ModelError,
    Modulation,
    Pairing,
    Radio,
    Replay, //
};
use aosabi_json::{
    JsObject,
    JsString,
    JsValue, //
};
use homa::{
    Carrier,
    Limits,
    Nanos,
    Priority,
    sim::RadioNet, //
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
    homa_sim::profile_json,
    seam, //
};

/// The name of the primitive.
const NAME: &str = "lora";

/// The stage of the Homa track the primitive lands at.
pub const LANDS_AT: &str = "H6";

/// The channel of a pairing that names none, 869.525 MHz in the band P.
const DEFAULT_FREQUENCY: u32 = 869_525_000;

/// The link of a pairing that names none.
const DEFAULT_LINK: u8 = 1;

/// The command line of the `lora` primitive.
pub static GRAMMAR: Grammar = Grammar {
    options: &[cli::SMOKE, cli::REQUESTS],
    positional: None,
    conflicts: &[],
};

/// What the `lora` primitive accepts.
#[derive(Debug)]
pub struct LoraArgs {
    /// Whether the call asks for the interface line.
    smoke: bool,
    /// The file of request lines of the batch form.
    requests: Option<PathBuf>,
}

impl LoraArgs {
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
    ("toa", op_toa),
    ("profile", op_profile),
    ("aead", op_aead),
    ("aead_open", op_aead_open),
    ("seal", op_seal),
    ("open", op_open),
    ("receive", op_receive),
    ("counters", op_counters),
    ("restarts", op_restarts),
    ("ledger", op_ledger),
    ("sizes", op_sizes),
];

/// Runs the `lora` primitive.
pub fn run(args: &LoraArgs) -> ExitCode {
    if args.smoke {
        return seam::smoke_at(NAME, LANDS_AT);
    }
    let outcome = seam::required(args.requests.as_deref(), "--requests")
        .and_then(|path| batch::run(OPERATIONS, path));
    seam::finish(NAME, outcome)
}

/// Returns the modulation a request names: `sf`, `bandwidth_hz` and
/// `coding_rate`, those of `LoRaWAN` where absent.
pub(crate) fn modulation_of(fields: &Fields<'_>) -> Result<Modulation, JsString> {
    Ok(Modulation {
        sf: fields.int_or("sf", 7)?,
        bandwidth_hz: fields.int_or("bandwidth_hz", 125_000)?,
        coding_rate: fields.int_or("coding_rate", 1)?,
    })
}

/// Returns the band a request names under `key`, `P` where absent.
pub(crate) fn band_of(fields: &Fields<'_>, key: &str) -> Result<Band, JsString> {
    match fields.opt_str(key)?.unwrap_or("P") {
        "P" => Ok(Band::P),
        "M" => Ok(Band::M),
        other => Err(fields.refuse(key, &format!("names no band: {other}"))),
    }
}

/// Returns the direction a request names under `key`.
pub(crate) fn direction_of(fields: &Fields<'_>, key: &str) -> Result<Direction, JsString> {
    match fields.str(key)? {
        "to_device" => Ok(Direction::ToDevice),
        "to_client" => Ok(Direction::ToClient),
        other => Err(fields.refuse(key, &format!("names no direction: {other}"))),
    }
}

/// Returns the name of a direction.
pub(crate) fn direction_name(direction: Direction) -> &'static str {
    match direction {
        Direction::ToDevice => "to_device",
        Direction::ToClient => "to_client",
    }
}

/// Returns the name of a frame error, the name of its variant.
pub(crate) fn frame_error_name(error: FrameError) -> &'static str {
    match error {
        FrameError::Short => "Short",
        FrameError::Link => "Link",
        FrameError::Tag => "Tag",
        FrameError::Replay => "Replay",
        FrameError::TooLong => "TooLong",
        FrameError::Counter => "Counter",
    }
}

/// Returns `N` bytes the hex under `key` spells.
fn fixed<const N: usize>(fields: &Fields<'_>, key: &str) -> Result<[u8; N], JsString> {
    let bytes = fields.hex(key)?;
    <[u8; N]>::try_from(bytes.as_slice())
        .map_err(|_| fields.refuse(key, &format!("is not {N} bytes")))
}

/// Returns the pairing the object under `key` names: `link`, `key`,
/// `frequency_hz`, `band`, the modulation and the limits of both ends, each
/// absent limit the card's own for the modulation and the band.
pub(crate) fn pairing_of(fields: &Fields<'_>, key: &str) -> Result<Pairing, JsString> {
    let spec = fields.nested(key)?;
    let modulation = modulation_of(&spec)?;
    let band = band_of(&spec, "band")?;
    let limits = lora::limits(modulation, band, spec.int_or("max_rpcs", 1)?);
    Ok(Pairing {
        link: spec.int_or("link", DEFAULT_LINK)?,
        key: fixed::<32>(&spec, "key")?,
        frequency_hz: spec.int_or("frequency_hz", DEFAULT_FREQUENCY)?,
        band,
        modulation,
        local: limits,
        peer: limits,
    })
}

/// Returns the limits as an object.
fn limits_json(limits: &Limits) -> JsObject {
    let mut object = JsObject::with_capacity(4);
    object.insert("max_packet", u32::from(limits.max_packet));
    object.insert("max_message", limits.max_message);
    object.insert("unsched_bytes", limits.unsched_bytes);
    object.insert("max_rpcs", u32::from(limits.max_rpcs));
    object
}

/// Returns what a carrier counted as an object.
pub(crate) fn stats_json(stats: &LoraStats) -> JsObject {
    let mut object = JsObject::new();
    object.insert("sent", stats.sent);
    object.insert("air_ns", stats.air);
    object.insert("received", stats.received);
    object.insert("dropped_link", stats.dropped_link);
    object.insert("dropped_tag", stats.dropped_tag);
    object.insert("dropped_replay", stats.dropped_replay);
    object.insert("dropped_length", stats.dropped_length);
    object.insert("dropped_counter", stats.dropped_counter);
    object.insert("busy", stats.busy);
    object.insert("held", stats.held);
    object.insert("gaps", stats.gaps);
    object.insert("limits_kept", stats.limits_kept);
    object
}

/// The counters of one link, kept across the restarts of its carrier as the
/// storage of a card keeps them, one value for each direction.
#[derive(Clone, Debug, Default)]
pub(crate) struct SharedStore {
    /// The first counter no reservation has taken, by the byte of the
    /// direction.
    reserved: Rc<[Cell<u32>; 2]>,
}

impl SharedStore {
    /// Returns the first counter of `direction` no reservation has taken.
    pub(crate) fn value(&self, direction: Direction) -> u32 {
        self.reserved[usize::from(direction.byte())].get()
    }
}

impl CounterStore for SharedStore {
    type Error = Infallible;

    fn reserved(&mut self, _link: u8, direction: Direction) -> Result<u32, Infallible> {
        Ok(self.value(direction))
    }

    fn reserve(&mut self, _link: u8, direction: Direction, up_to: u32) -> Result<(), Infallible> {
        self.reserved[usize::from(direction.byte())].set(up_to);
        Ok(())
    }
}

/// A radio that holds the frames a request names, hears no other and finds
/// the channel free, and records every frame it sends.
#[derive(Debug, Default)]
struct ScriptRadio {
    /// The frames it hands the carrier, in order.
    inbox: VecDeque<Vec<u8>>,
    /// The frames the carrier sent.
    sent: Vec<Vec<u8>>,
}

impl Radio for ScriptRadio {
    type Error = Infallible;

    fn configure(&mut self, _: u32, _: Modulation, _: u16) -> Result<(), Infallible> {
        Ok(())
    }

    fn channel_busy(&mut self) -> Result<bool, Infallible> {
        Ok(false)
    }

    fn transmit(&mut self, payload: &[u8]) -> Result<(), Infallible> {
        self.sent.push(payload.to_vec());
        Ok(())
    }

    fn receive(&mut self, buf: &mut [u8]) -> Result<Option<usize>, Infallible> {
        let Some(frame) = self.inbox.pop_front() else {
            return Ok(None);
        };
        let len = frame.len().min(buf.len());
        buf[..len].copy_from_slice(&frame[..len]);
        Ok(Some(len))
    }
}

/// The frames a radio sent, each with the time it was asked for.
pub(crate) type SentFrames = Rc<RefCell<Vec<(Nanos, Vec<u8>)>>>;

/// One radio of the net of the radio model of `homa-sim`, which records
/// every frame its carrier sends with its time.
#[derive(Debug)]
pub(crate) struct ModelRadio {
    /// The net.
    net: Rc<RefCell<RadioNet>>,
    /// The radio in the net.
    r: u8,
    /// The frames its carrier sent, each with the time it was asked for.
    sent: SentFrames,
}

impl ModelRadio {
    /// Returns radio `r` of `net`, which records what it sends in `sent`.
    pub(crate) fn new(net: Rc<RefCell<RadioNet>>, r: u8, sent: SentFrames) -> Self {
        Self { net, r, sent }
    }
}

impl Radio for ModelRadio {
    type Error = ModelError;

    fn configure(
        &mut self,
        frequency_hz: u32,
        modulation: Modulation,
        sync_word: u16,
    ) -> Result<(), ModelError> {
        let mut net = self.net.borrow_mut();
        Radio::configure(&mut net.radio(self.r), frequency_hz, modulation, sync_word)
    }

    fn channel_busy(&mut self) -> Result<bool, ModelError> {
        Radio::channel_busy(&mut self.net.borrow_mut().radio(self.r))
    }

    fn transmit(&mut self, payload: &[u8]) -> Result<(), ModelError> {
        let mut net = self.net.borrow_mut();
        let now = net.now();
        Radio::transmit(&mut net.radio(self.r), payload)?;
        self.sent.borrow_mut().push((now, payload.to_vec()));
        Ok(())
    }

    fn receive(&mut self, buf: &mut [u8]) -> Result<Option<usize>, ModelError> {
        Radio::receive(&mut self.net.borrow_mut().radio(self.r), buf)
    }
}

/// Answers the operation `toa`.
fn op_toa(request: &JsObject) -> Result<JsObject, JsString> {
    let fields = Fields::new(request, "toa");
    let modulation = modulation_of(&fields)?;
    let len: u8 = fields.int("len")?;
    let mut payload = JsObject::with_capacity(1);
    payload.insert("air_ns", lora::time_on_air(modulation, len));
    Ok(payload)
}

/// Answers the operation `profile`.
fn op_profile(request: &JsObject) -> Result<JsObject, JsString> {
    let fields = Fields::new(request, "profile");
    let modulation = modulation_of(&fields)?;
    let band = band_of(&fields, "band")?;
    let sf = modulation.spreading_factor();
    let max_frame = band.max_frame(sf);
    let resend = 14 + lora::OVERHEAD;
    let mut payload = JsObject::new();
    payload.insert("profile", profile_json(&lora::profile(modulation, band)));
    payload.insert(
        "limits",
        limits_json(&lora::limits(
            modulation,
            band,
            fields.int_or("max_rpcs", 1)?,
        )),
    );
    payload.insert("longest_response", lora::longest_response(modulation, band));
    payload.insert("max_frame", count(max_frame));
    payload.insert("max_packet", u32::from(band.max_packet(sf)));
    payload.insert("air_per_hour_ns", band.air_per_hour());
    payload.insert("longest_transmission_ns", band.longest_transmission());
    payload.insert("burst_ns", lora::BURST);
    payload.insert(
        "gap_ns",
        lora::time_on_air(modulation, u8::try_from(resend).unwrap_or(u8::MAX)) * 2,
    );
    payload.insert(
        "full_air_ns",
        lora::time_on_air(modulation, u8::try_from(max_frame).unwrap_or(u8::MAX)),
    );
    payload.insert("turnaround_ns", lora::TURNAROUND);
    Ok(payload)
}

/// Answers the operation `aead`.
fn op_aead(request: &JsObject) -> Result<JsObject, JsString> {
    let fields = Fields::new(request, "aead");
    let key = fixed::<32>(&fields, "key")?;
    let nonce = fixed::<12>(&fields, "nonce")?;
    let aad = fields.hex("aad")?;
    let mut buffer = fields.hex("hex")?;
    let mut payload = JsObject::with_capacity(2);
    match lora::seal_detached(&key, &nonce, &aad, &mut buffer) {
        Ok(tag) => {
            payload.insert("hex", hex(&buffer));
            payload.insert("tag", hex(&tag));
        }
        Err(error) => {
            payload.insert("error", frame_error_name(error));
        }
    }
    Ok(payload)
}

/// Answers the operation `aead_open`.
fn op_aead_open(request: &JsObject) -> Result<JsObject, JsString> {
    let fields = Fields::new(request, "aead_open");
    let key = fixed::<32>(&fields, "key")?;
    let nonce = fixed::<12>(&fields, "nonce")?;
    let tag = fixed::<16>(&fields, "tag")?;
    let aad = fields.hex("aad")?;
    let mut buffer = fields.hex("hex")?;
    let mut payload = JsObject::with_capacity(1);
    match lora::open_detached(&key, &nonce, &aad, &mut buffer, &tag) {
        Ok(()) => payload.insert("hex", hex(&buffer)),
        Err(error) => payload.insert("error", frame_error_name(error)),
    };
    Ok(payload)
}

/// Answers the operation `seal`.
fn op_seal(request: &JsObject) -> Result<JsObject, JsString> {
    let fields = Fields::new(request, "seal");
    let pairing = pairing_of(&fields, "pairing")?;
    let direction = direction_of(&fields, "direction")?;
    let counter: u32 = fields.int("counter")?;
    let packet = fields.hex("hex")?;
    let mut out = [0u8; lora::MAX_PAYLOAD];
    let mut payload = JsObject::with_capacity(1);
    match lora::seal(&pairing, direction, counter, &packet, &mut out) {
        Ok(len) => payload.insert("hex", hex(&out[..len])),
        Err(error) => payload.insert("error", frame_error_name(error)),
    };
    Ok(payload)
}

/// Returns the frames of the array under `key`.
fn frames_of(fields: &Fields<'_>, key: &str) -> Result<Vec<Vec<u8>>, JsString> {
    fields
        .array(key)?
        .iter()
        .map(|value| match value {
            JsValue::String(text) => text
                .as_str()
                .and_then(unhex)
                .ok_or_else(|| fields.refuse(key, "holds a frame that is no hex")),
            _ => Err(fields.refuse(key, "holds a frame that is no string")),
        })
        .collect()
}

/// Answers the operation `open`.
fn op_open(request: &JsObject) -> Result<JsObject, JsString> {
    let fields = Fields::new(request, "open");
    let pairing = pairing_of(&fields, "pairing")?;
    let direction = direction_of(&fields, "direction")?;
    let mut replay = Replay::new();
    let mut results = Vec::new();
    for frame in frames_of(&fields, "frames")? {
        let mut out = [0u8; lora::MAX_PAYLOAD];
        let mut result = JsObject::with_capacity(2);
        let opened =
            lora::open(&pairing, direction, &frame, &mut out).and_then(|(counter, packet)| {
                replay.accept(counter)?;
                Ok((counter, packet))
            });
        match opened {
            Ok((counter, packet)) => {
                result.insert("counter", counter);
                result.insert("hex", hex(packet));
            }
            Err(error) => {
                result.insert("error", frame_error_name(error));
            }
        }
        results.push(JsValue::Object(result));
    }
    let mut payload = JsObject::with_capacity(1);
    payload.insert("results", results);
    Ok(payload)
}

/// Answers the operation `receive`.
fn op_receive(request: &JsObject) -> Result<JsObject, JsString> {
    let fields = Fields::new(request, "receive");
    let pairing = pairing_of(&fields, "pairing")?;
    let direction = direction_of(&fields, "direction")?;
    let radio = ScriptRadio {
        inbox: frames_of(&fields, "frames")?.into(),
        sent: Vec::new(),
    };
    let mut buckets = [0u32; lora::BUCKETS];
    let mut carrier = Lora::new(
        radio,
        pairing,
        direction.other(),
        SharedStore::default(),
        &mut buckets,
        1,
    )
    .map_err(|_| fields.refuse("pairing", "builds no carrier"))?;
    let mut packets = Vec::new();
    let mut buf = [0u8; lora::MAX_PAYLOAD];
    while let Ok(Some((link, len))) = carrier.recv(&mut buf) {
        let mut packet = JsObject::with_capacity(2);
        packet.insert("link", u32::from(link));
        packet.insert("hex", hex(&buf[..len]));
        packets.push(JsValue::Object(packet));
    }
    let mut payload = JsObject::with_capacity(2);
    payload.insert("packets", packets);
    payload.insert("stats", stats_json(&carrier.stats()));
    Ok(payload)
}

/// Answers the operation `counters`.
fn op_counters(request: &JsObject) -> Result<JsObject, JsString> {
    let fields = Fields::new(request, "counters");
    let link: u8 = fields.int_or("link", DEFAULT_LINK)?;
    let direction = direction_of(&fields, "direction")?;
    let store = SharedStore::default();
    let mut boots = Vec::new();
    for value in fields.array("uses")? {
        let uses = crate::homa_packet::whole(value)
            .ok_or_else(|| fields.refuse("uses", "holds a value that is no count"))?;
        let Ok(mut counters) = Counters::boot(store.clone(), link, direction);
        let first = counters.peek();
        let mut taken = 0u64;
        for _ in 0..uses {
            let Ok(counter) = counters.next();
            if counter == u32::MAX {
                break;
            }
            taken += 1;
        }
        let mut boot = JsObject::with_capacity(3);
        boot.insert("first", first);
        boot.insert("count", taken);
        boot.insert("reserved", store.value(direction));
        boots.push(JsValue::Object(boot));
    }
    let mut payload = JsObject::with_capacity(1);
    payload.insert("boots", boots);
    Ok(payload)
}

/// Answers the operation `restarts`.
fn op_restarts(request: &JsObject) -> Result<JsObject, JsString> {
    let fields = Fields::new(request, "restarts");
    let pairing = pairing_of(&fields, "pairing")?;
    let direction = direction_of(&fields, "direction")?;
    let len: usize = fields.int_or("len", 30)?;
    if len == 0 || len > lora::MAX_SEALED {
        return Err(fields.refuse("len", "is no length of a packet"));
    }
    let mut buckets = [0u32; lora::BUCKETS];
    let mut carrier = Lora::new(
        ScriptRadio::default(),
        pairing,
        direction,
        SharedStore::default(),
        &mut buckets,
        1,
    )
    .map_err(|_| fields.refuse("pairing", "builds no carrier"))?;
    let packet = vec![0x10; len];
    let mut now: Nanos = 0;
    let mut frames = Vec::new();
    for (boot, value) in fields.array("sends")?.iter().enumerate() {
        let sends = crate::homa_packet::whole(value)
            .ok_or_else(|| fields.refuse("sends", "holds a value that is no count"))?;
        if boot > 0 && carrier.reboot().is_err() {
            return Err(fields.refuse("pairing", "restarts no carrier"));
        }
        for _ in 0..sends {
            let mut waited = 0;
            while !carrier.ready(pairing.link, len) {
                let next = carrier
                    .advance(now)
                    .ok()
                    .flatten()
                    .unwrap_or(now + 1_000_000);
                now = next.max(now + 1);
                waited += 1;
                if waited > 1_000 {
                    return Err(fields.refuse("sends", "never finds the carrier ready"));
                }
            }
            let before = carrier.radio().sent.len();
            let _ = carrier.send(pairing.link, Priority::HIGHEST, &packet, &[]);
            for frame in &carrier.radio().sent[before..] {
                let mut row = JsObject::with_capacity(2);
                row.insert("boot", count(boot));
                row.insert(
                    "counter",
                    u32::from_be_bytes([frame[1], frame[2], frame[3], frame[4]]),
                );
                frames.push(JsValue::Object(row));
            }
            let _ = carrier.advance(now);
        }
    }
    let mut payload = JsObject::with_capacity(2);
    payload.insert("direction", direction_name(direction));
    payload.insert("frames", frames);
    Ok(payload)
}

/// Answers the operation `ledger`.
fn op_ledger(request: &JsObject) -> Result<JsObject, JsString> {
    let fields = Fields::new(request, "ledger");
    let band = band_of(&fields, "band")?;
    let mut buckets = [0u32; lora::BUCKETS];
    let mut ledger = Ledger::new(&mut buckets, band);
    let mut results = Vec::new();
    for entry in fields.array("script")? {
        let JsValue::Array(items) = entry else {
            return Err(fields.refuse("script", "holds an entry that is no array"));
        };
        let (Some(at), Some(air), Some(JsValue::Bool(control))) = (
            items.first().and_then(crate::homa_packet::whole),
            items.get(1).and_then(crate::homa_packet::whole),
            items.get(2),
        ) else {
            return Err(fields.refuse(
                "script",
                "holds an entry that is no [at_ns, air_ns, control]",
            ));
        };
        let mut result = JsObject::with_capacity(2);
        if ledger.fits(at, air, *control) && ledger.charge(at, air).is_ok() {
            result.insert("state", "charged");
        } else {
            result.insert("state", "refused");
            result.insert("next_fit_ns", ledger.next_fit(at, air));
        }
        results.push(JsValue::Object(result));
    }
    let mut payload = JsObject::with_capacity(2);
    payload.insert("air_per_hour_ns", band.air_per_hour());
    payload.insert("results", results);
    Ok(payload)
}

/// A radio of no state, whose carrier's size is the carrier's own.
#[derive(Debug)]
struct NoRadio;

impl Radio for NoRadio {
    type Error = Infallible;

    fn configure(&mut self, _: u32, _: Modulation, _: u16) -> Result<(), Infallible> {
        Ok(())
    }

    fn channel_busy(&mut self) -> Result<bool, Infallible> {
        Ok(false)
    }

    fn transmit(&mut self, _: &[u8]) -> Result<(), Infallible> {
        Ok(())
    }

    fn receive(&mut self, _: &mut [u8]) -> Result<Option<usize>, Infallible> {
        Ok(None)
    }
}

/// A store of no state.
#[derive(Debug)]
struct NoStore;

impl CounterStore for NoStore {
    type Error = Infallible;

    fn reserved(&mut self, _: u8, _: Direction) -> Result<u32, Infallible> {
        Ok(0)
    }

    fn reserve(&mut self, _: u8, _: Direction, _: u32) -> Result<(), Infallible> {
        Ok(())
    }
}

/// Answers the operation `sizes`.
///
/// The sizes are those of this host, whose pointers and slices are twice as
/// long as those of the RV32IMC core, and the notebook names which fields
/// hold one.
// Every operation of the batch form answers a result, and this one refuses
// nothing.
#[expect(clippy::unnecessary_wraps)]
fn op_sizes(_request: &JsObject) -> Result<JsObject, JsString> {
    let mut payload = JsObject::new();
    payload.insert(
        "carrier",
        count(mem::size_of::<Lora<'static, NoRadio, NoStore>>()),
    );
    payload.insert("ledger", count(mem::size_of::<Ledger<'static>>()));
    payload.insert(
        "ledger_buckets",
        count(mem::size_of::<[u32; lora::BUCKETS]>()),
    );
    payload.insert("replay", count(mem::size_of::<Replay>()));
    payload.insert("counters", count(mem::size_of::<Counters<NoStore>>()));
    payload.insert("pairing", count(mem::size_of::<Pairing>()));
    payload.insert("frame_overhead", count(lora::OVERHEAD));
    payload.insert("max_sealed", count(lora::MAX_SEALED));
    payload.insert("pointer", count(mem::size_of::<usize>()));
    Ok(payload)
}
