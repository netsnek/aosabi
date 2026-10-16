// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool carrier bluetooth: Bluetooth behind the primitive carrier,
 * the GATT data path of aosabi-carrier-bluetooth through BlueZ, its baseline,
 * and the channel, whose Linux socket waits.
 *
 * No analogue in the reference. The adapter is one of the virtual controllers
 * the notebook starts with btvirt in guest A. The judging notebook is
 * tests/notebooks/28_carrier_bluetooth.ipynb.
 */

//! Bluetooth behind the primitive `carrier`.
//!
//! `serve` on `gatt` registers the service of Aosabi on the adapter
//! `adapter` and runs `serve_gatt_counted` of the crate over the listener in
//! front of a Yoga instance, or one endpoint of the engine with a responder
//! of bytes over the same data path, whose link the writes, the subscription
//! and the connection of the one central it serves take up and down. It ends
//! where the bus fails, where `downs` links have gone down, or at
//! `max_time_ms`. The data path is paced in both forms at `interval_ms`,
//! `per_event` and `tx_depth`, each the value of the planning link of the
//! crate where the request names none, and `serve` answers that pace. A
//! request that names `trace`, the packets to keep at most, turns the trace
//! of the data path on before the link goes up, and `serve` answers that
//! trace beside its counts in both forms: the time of each packet on the
//! realtime clock of the machine, whether `send` handed it to the bus or
//! `recv` took it from RX, and its compact header, so a notebook sets the
//! stamps of the driver beside those of its central.

use std::{
    collections::VecDeque,
    io,
    rc::Rc,
    thread,
    time::{
        Duration,
        Instant, //
    }, //
};

use aosabi_carrier_bluetooth::{
    Bus,
    CENTRALS,
    CHANNEL_WAITS,
    Counts,
    Gatt,
    GattCounts,
    GattEvent,
    GattTrace,
    MAX_ATT_MTU,
    Message,
    Pace,
    SPSM,
    gatt_limits,
    gatt_profile,
    peers_config,
    seed,
    sent_packet,
    serve_gatt_counted, //
};
use aosabi_core::graphtp::Listener;
use aosabi_json::{
    JsObject,
    JsString,
    JsValue, //
};
use homa::{
    Carrier,
    Owned,
    Priority,
    stream::{
        Decoder,
        Framed,
        Mode, //
    }, //
};

use super::{
    Behind,
    EchoEndpoint,
    LinkTurn,
    Stopped,
    behind_of,
    card_of,
    config_refused,
    echo_run,
    endpoint_stats,
    run_served_until,
    settle_body,
    stopped_json,
    yoga_fetch, //
};
use crate::homa_packet::{
    Fields,
    count,
    hex, //
};

/// Registers the service of Aosabi on the adapter a request names under
/// `adapter`, `hci0` where it names none.
fn register(fields: &Fields<'_>) -> Result<Gatt, JsString> {
    let adapter = fields.opt_str("adapter")?.unwrap_or("hci0");
    Bus::system()
        .and_then(|bus| Gatt::register(bus, adapter, SPSM))
        .map_err(|error| fields.refuse("adapter", &format!("registers no service: {error}")))
}

/// Answers `serve` on the GATT data path.
pub(super) fn serve_gatt(fields: &Fields<'_>) -> Result<JsObject, JsString> {
    let max_time = Duration::from_millis(fields.int_or("max_time_ms", 600_000)?);
    let downs = fields.opt_u64("downs")?;
    let card = card_of(fields)?;
    let pace = pace_of(fields)?;
    let mut gatt = register(fields)?;
    gatt.set_pace(Some(pace));
    let trace = trace_on(fields, &mut gatt)?;
    match behind_of(fields)? {
        Behind::Instance(yoga) => {
            let listener = Listener::new(yoga_fetch(yoga), card)
                .map_err(|error| JsString::from(format!("listener: {error}")))?;
            let counts = Rc::new(Counts::default());
            let served = serve_gatt_counted(gatt, listener, Rc::clone(&counts));
            let watched = Rc::clone(&counts);
            let until = move || downs.is_some_and(|downs| watched.downs.get() >= downs);
            let (stopped, time_ms) =
                run_served_until(served, max_time, until).map_err(JsString::from)?;
            let mut payload = JsObject::new();
            stopped_json(&mut payload, &stopped, time_ms);
            let mut object = JsObject::new();
            object.insert("answered", counts.answered.get());
            object.insert("ups", counts.ups.get());
            object.insert("downs", counts.downs.get());
            object.insert("gatt", gatt_json(&counts.gatt.get()));
            object.insert("stats", endpoint_stats(&counts.stats.get()));
            payload.insert("counts", object);
            payload.insert("pace", pace_json(&pace));
            insert_trace(&mut payload, trace.as_deref());
            Ok(payload)
        }
        Behind::Responder(responder) => {
            let seeded = seed().map_err(|error| JsString::from(format!("seed: {error}")))?;
            let (config, max_message) = peers_config(card, seeded, CENTRALS);
            let profile = gatt_profile(MAX_ATT_MTU, 0);
            let mut endpoint =
                EchoEndpoint::new(config, profile, Owned::new(&config)).map_err(config_refused)?;
            let idle = Duration::from_micros(fields.int_or("idle_us", 200)?);
            let (stopped, counts, time_ms) = echo_run(
                &mut gatt,
                &mut endpoint,
                responder,
                max_time,
                idle,
                |link, endpoint, counts| {
                    link.poll().map_err(|error| error.to_string())?;
                    while let Some(event) = link.poll_event() {
                        match event {
                            GattEvent::Up { peer, att_mtu } => {
                                let (local, assumed) = gatt_limits(att_mtu, max_message);
                                endpoint
                                    .link_up(peer, local, assumed)
                                    .map_err(|error| format!("link: {error}"))?;
                                counts.ups += 1;
                            }
                            GattEvent::Down { peer } => {
                                // A peer whose link is down already needs nothing.
                                let _ = endpoint.link_down(peer);
                                while endpoint.poll_event().is_some() {}
                                counts.downs += 1;
                                if downs.is_some_and(|downs| counts.downs >= downs) {
                                    return Ok(LinkTurn::Ended);
                                }
                            }
                        }
                    }
                    Ok(LinkTurn::Running)
                },
            );
            let mut payload = JsObject::new();
            stopped_json(&mut payload, &stopped, time_ms);
            payload.insert("echo", counts.json());
            payload.insert("gatt", gatt_json(&gatt.counts()));
            payload.insert("stats", endpoint_stats(&endpoint.stats()));
            payload.insert("pace", pace_json(&pace));
            insert_trace(&mut payload, trace.as_deref());
            Ok(payload)
        }
    }
}

/// Returns the pace a request names for the data path: `interval_ms`, the
/// connection interval in milliseconds, `per_event`, the packets an event
/// carries, and `tx_depth`, the packets handed over and not yet carried at
/// most, each the value of [`Pace::PLANNING`] where the request names none.
fn pace_of(fields: &Fields<'_>) -> Result<Pace, JsString> {
    let planning = Pace::PLANNING;
    let interval_ms: u64 = fields.int_or("interval_ms", planning.interval / 1_000_000)?;
    let per_event: u16 = fields.int_or("per_event", planning.per_event)?;
    let tx_depth: u16 = fields.int_or("tx_depth", planning.tx_depth)?;
    for (key, value) in [
        ("interval_ms", interval_ms),
        ("per_event", u64::from(per_event)),
        ("tx_depth", u64::from(tx_depth)),
    ] {
        if value == 0 {
            return Err(fields.refuse(key, "is 0, which would hold every packet"));
        }
    }
    let interval = interval_ms
        .checked_mul(1_000_000)
        .ok_or_else(|| fields.refuse("interval_ms", "is out of range"))?;
    Ok(Pace {
        interval,
        per_event,
        tx_depth,
    })
}

/// Returns the pace of the data path as an object, its interval in
/// milliseconds.
fn pace_json(pace: &Pace) -> JsObject {
    let mut object = JsObject::with_capacity(3);
    object.insert("interval_ms", pace.interval / 1_000_000);
    object.insert("per_event", u32::from(pace.per_event));
    object.insert("tx_depth", u32::from(pace.tx_depth));
    object
}

/// Turns the trace of the data path `gatt` on where a request names
/// `trace`, the packets it keeps at most, and returns it.
fn trace_on(fields: &Fields<'_>, gatt: &mut Gatt) -> Result<Option<Rc<GattTrace>>, JsString> {
    let Some(capacity) = fields.opt_u64("trace")? else {
        return Ok(None);
    };
    let capacity = usize::try_from(capacity)
        .map_err(|_| fields.refuse("trace", "names more packets than this machine holds"))?;
    Ok(Some(gatt.trace_on(capacity)))
}

/// Writes the trace of the data path into `payload` where a request turned
/// it on: the packets it keeps at most, the packets past that bound it did
/// not keep, and each packet it kept as its time, `send` or `recv` and its
/// compact header in hex.
///
/// The time is the nanoseconds since the UNIX epoch on the realtime clock in
/// decimal text, since a number of JSON holds 53 bits and the time takes 61.
fn insert_trace(payload: &mut JsObject, trace: Option<&GattTrace>) {
    let Some(trace) = trace else {
        return;
    };
    let packets: Vec<JsValue> = trace
        .packets()
        .iter()
        .map(|packet| {
            JsValue::from(vec![
                JsValue::from(packet.at_ns.to_string()),
                JsValue::from(if packet.sent { "send" } else { "recv" }),
                JsValue::from(hex(packet.header())),
            ])
        })
        .collect();
    let mut object = JsObject::with_capacity(3);
    object.insert("capacity", count(trace.capacity()));
    object.insert("dropped", trace.dropped());
    object.insert("packets", packets);
    payload.insert("trace", object);
}

/// Answers the operation `dbus-decode`: reads the messages of `hex` one after
/// the other with the D-Bus client of the crate, and answers for each its
/// kind, its serial, its header fields and the signature of its body, or
/// the error that refused it and the offset it stopped at.
pub(super) fn op_dbus_decode(request: &JsObject) -> Result<JsObject, JsString> {
    let fields = Fields::new(request, "dbus-decode");
    let bytes = fields.hex("hex")?;
    let mut at = 0;
    let mut messages = Vec::new();
    let mut payload = JsObject::with_capacity(3);
    while at < bytes.len() {
        match Message::decode(&bytes[at..]) {
            Ok(Some((message, used))) => {
                messages.push(JsValue::Object(message_json(&message)));
                at += used;
            }
            Ok(None) => {
                payload.insert("incomplete", count(bytes.len() - at));
                break;
            }
            Err(error) => {
                payload.insert("refused", error.to_string());
                break;
            }
        }
    }
    payload.insert("messages", messages);
    payload.insert("offset", count(at));
    Ok(payload)
}

/// Returns a message the D-Bus client read as an object.
fn message_json(message: &Message) -> JsObject {
    let text = |value: &Option<String>| value.clone().map_or(JsValue::Null, JsValue::from);
    let mut object = JsObject::with_capacity(9);
    object.insert("kind", format!("{:?}", message.kind));
    object.insert("serial", message.serial);
    object.insert("path", text(&message.path));
    object.insert("interface", text(&message.interface));
    object.insert("member", text(&message.member));
    object.insert("error_name", text(&message.error_name));
    object.insert(
        "reply_serial",
        message.reply_serial.map_or(JsValue::Null, JsValue::from),
    );
    object.insert("signature", message.signature());
    object.insert("values", count(message.body.len()));
    object
}

/// Answers `serve` and `baseline` on the channel, whose Linux socket waits of
/// the plan.
pub(super) fn serve_channel() -> JsObject {
    let mut payload = JsObject::with_capacity(2);
    payload.insert("stopped", "not run");
    payload.insert("error", CHANNEL_WAITS);
    payload
}

/// The central of the baseline and the bytes of the stream it wrote.
struct BaselineCentral {
    /// Its peer.
    peer: u32,
    /// Its largest packet, the one the data path sends at the `ATT_MTU` of
    /// its first write.
    max_packet: usize,
}

/// Answers `baseline` on the GATT data path: whole messages in prefixed
/// `AoFrame` alone, the stream of each direction cut to values of the largest
/// packet of the central, first in first out, with no Homa and no pace, so
/// the data path hands the bus every value while the bus has room.
pub(super) fn baseline_gatt(fields: &Fields<'_>) -> Result<JsObject, JsString> {
    let max_time = Duration::from_millis(fields.int_or("max_time_ms", 600_000)?);
    let max_message: usize = fields.int_or("max_message", 131_072)?;
    let idle = Duration::from_micros(fields.int_or("idle_us", 200)?);
    let downs = fields.opt_u64("downs")?;
    let mut gone = 0u64;
    let (responder, fetch) = match behind_of(fields)? {
        Behind::Responder(responder) => (Some(responder), None),
        Behind::Instance(yoga) => (None, Some(yoga_fetch(yoga))),
    };
    let mut gatt = register(fields)?;
    gatt.set_pace(None);
    let mut central: Option<BaselineCentral> = None;
    let mut rx = vec![0u8; Decoder::buffer_len(Mode::Prefixed, max_message)];
    let mut unused = vec![0u8; 1];
    let mut decoder = Framed::new(Inbox::default(), Mode::Prefixed, &mut rx, &mut unused);
    let mut outgoing: VecDeque<u8> = VecDeque::new();
    let mut message = vec![0u8; max_message];
    let mut packet = vec![0u8; usize::from(MAX_ATT_MTU)];
    let (mut requests, mut answered) = (0u64, 0u64);
    let start = Instant::now();
    let stopped = loop {
        if start.elapsed() > max_time {
            break Stopped::MaxTime;
        }
        if let Err(error) = gatt.poll() {
            break Stopped::Link(Some(error.to_string()));
        }
        while let Some(event) = gatt.poll_event() {
            match event {
                GattEvent::Up { peer, att_mtu } => {
                    central = Some(BaselineCentral {
                        peer,
                        max_packet: usize::from(sent_packet(att_mtu)),
                    });
                }
                GattEvent::Down { .. } => {
                    central = None;
                    gone += 1;
                }
            }
        }
        if downs.is_some_and(|downs| gone >= downs) {
            break Stopped::Link(None);
        }
        let mut moved = false;
        while let Ok(Some((_, len))) = gatt.recv(&mut packet) {
            decoder.stream_mut().bytes.extend(&packet[..len]);
            moved = true;
        }
        while let Ok(Some(((), len))) = decoder.recv(&mut message) {
            requests += 1;
            let request = &message[..len];
            let body = match (responder, &fetch) {
                (Some(responder), _) => responder.answer(request),
                (None, Some(fetch)) => settle_body(fetch, request).unwrap_or_default(),
                (None, None) => Vec::new(),
            };
            outgoing.extend((u32::try_from(body.len()).unwrap_or(u32::MAX)).to_be_bytes());
            outgoing.extend(body);
            answered += 1;
        }
        if let Some(central) = &central {
            while !outgoing.is_empty() && gatt.ready(central.peer, central.max_packet) {
                let take = outgoing.len().min(central.max_packet);
                let chunk: Vec<u8> = outgoing.drain(..take).collect();
                if let Err(error) = gatt.send(central.peer, Priority::HIGHEST, &[], &chunk) {
                    return Ok(finish(
                        &Stopped::Link(Some(error.to_string())),
                        requests,
                        answered,
                        &gatt,
                        start,
                    ));
                }
                moved = true;
            }
        }
        if !moved {
            thread::sleep(idle);
        }
    };
    Ok(finish(&stopped, requests, answered, &gatt, start))
}

/// Returns the answer of the baseline on the GATT data path.
fn finish(
    stopped: &Stopped,
    requests: u64,
    answered: u64,
    gatt: &Gatt,
    start: Instant,
) -> JsObject {
    let mut payload = JsObject::new();
    let time_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
    stopped_json(&mut payload, stopped, time_ms);
    payload.insert("requests", requests);
    payload.insert("answered", answered);
    payload.insert("gatt", gatt_json(&gatt.counts()));
    payload
}

/// The bytes of the stream the central wrote, which the framing reads.
#[derive(Default)]
struct Inbox {
    /// The bytes not yet read.
    bytes: VecDeque<u8>,
}

impl homa::stream::ByteStream for Inbox {
    type Error = io::Error;

    fn read(&mut self, buf: &mut [u8]) -> io::Result<Option<usize>> {
        if self.bytes.is_empty() {
            return Ok(None);
        }
        let take = self.bytes.len().min(buf.len());
        for (slot, byte) in buf.iter_mut().zip(self.bytes.drain(..take)) {
            *slot = byte;
        }
        Ok(Some(take))
    }

    fn write(&mut self, _bytes: &[u8]) -> io::Result<Option<usize>> {
        Ok(None)
    }
}

/// Returns what the data path counted as an object.
fn gatt_json(counts: &GattCounts) -> JsObject {
    let mut object = JsObject::with_capacity(7);
    object.insert("writes", counts.writes);
    object.insert("notifications", counts.notifications);
    object.insert("dropped", counts.dropped);
    object.insert("psm_reads", counts.psm_reads);
    object.insert("refused", counts.refused);
    object.insert("ups", counts.ups);
    object.insert("downs", counts.downs);
    object
}
