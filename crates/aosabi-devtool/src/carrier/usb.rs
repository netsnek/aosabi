// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool carrier usb: USB behind the primitive carrier, the bulk
 * function of aosabi-carrier-usb over a mounted FunctionFS and its serial
 * function on the tty of the ACM function, and their baselines.
 *
 * No analogue in the reference. The gadget is the one the card's setup
 * composes in configfs, which the notebook composes in guest A over
 * dummy_hcd. The judging notebook is tests/notebooks/27_carrier_usb.ipynb.
 */

//! USB behind the primitive `carrier`.
//!
//! `serve` on `usb-bulk` opens the bulk function at the mount `path` at
//! `speed` and runs `serve_bulk_counted` of the crate over the listener in
//! front of a Yoga instance, or one endpoint of the engine with a responder
//! of bytes over the same function, whose link the events of `ep0` take up
//! and down. `serve` on `usb-serial` opens the tty of the ACM function at
//! `path` and runs `serve_serial_counted` or a responder over delimited
//! `AoFrame`. Each ends where its link fails, where `downs` links have gone
//! down, or at `max_time_ms`. `baseline` answers whole messages in `AoFrame`
//! alone: prefixed frames cut to transfers of the largest packet on bulk,
//! and delimited frames on serial. `descriptors` answers what the bulk
//! function writes to `ep0`, and `needs-zlp` the rule of the zero length
//! packet.

use std::{
    io,
    path::Path,
    rc::Rc,
    time::Duration, //
};

use aosabi_carrier_usb::{
    Bulk,
    BulkCounts,
    Counts,
    FfsEvent,
    GadgetTty,
    Speed,
    bulk_profile,
    descriptors,
    limits as link_limits,
    link_config,
    needs_zlp,
    seed,
    serial_profile,
    serve_bulk_counted,
    serve_serial_counted,
    strings, //
};
use aosabi_core::graphtp::Listener;
use aosabi_json::{
    JsObject,
    JsString, //
};
use homa::{
    Owned,
    stream::{
        ByteStream,
        Decoder,
        Framed,
        Mode,
        frame_len, //
    }, //
};

use super::{
    Behind,
    EchoEndpoint,
    LinkTurn,
    baseline::{
        self,
        PacketStream, //
    },
    behind_of,
    card_of,
    config_refused,
    echo_run,
    endpoint_stats,
    hex_of,
    run_served_until,
    stopped_json,
    yoga_fetch, //
};
use crate::homa_packet::Fields;

/// Returns the speed a request names under `speed`, `high` where it names
/// none.
pub(super) fn speed_of(fields: &Fields<'_>) -> Result<Speed, JsString> {
    match fields.opt_str("speed")?.unwrap_or("high") {
        "high" => Ok(Speed::High),
        "full" => Ok(Speed::Full),
        other => Err(fields.refuse("speed", &format!("names no speed: {other}"))),
    }
}

/// Answers the operation `needs-zlp`.
pub(super) fn op_needs_zlp(request: &JsObject) -> Result<JsObject, JsString> {
    let fields = Fields::new(request, "needs-zlp");
    let zlp = needs_zlp(
        fields.int("len")?,
        fields.int("max_packet_size")?,
        fields.int("max_packet")?,
    );
    let mut payload = JsObject::with_capacity(1);
    payload.insert("zlp", zlp);
    Ok(payload)
}

/// Answers the operation `descriptors`.
pub(super) fn op_descriptors(request: &JsObject) -> Result<JsObject, JsString> {
    let fields = Fields::new(request, "descriptors");
    let speed = speed_of(&fields)?;
    let max_packet_size = fields.int_or("max_packet_size", speed.max_packet_size())?;
    let interface = fields.opt_str("interface")?.unwrap_or("Aosabi");
    let mut payload = JsObject::with_capacity(2);
    payload.insert("descriptors", hex_of(&descriptors(speed, max_packet_size)));
    payload.insert("strings", hex_of(&strings(interface)));
    Ok(payload)
}

/// Opens the bulk function at the mount a request names under `path`.
fn open_bulk(fields: &Fields<'_>) -> Result<Bulk, JsString> {
    let path = fields.str("path")?;
    Bulk::open(Path::new(path), speed_of(fields)?)
        .map_err(|error| fields.refuse("path", &format!("opens no bulk function: {error}")))
}

/// Opens the tty of the ACM function a request names under `path`.
fn open_tty(fields: &Fields<'_>) -> Result<GadgetTty, JsString> {
    let path = fields.str("path")?;
    GadgetTty::open(Path::new(path))
        .map_err(|error| fields.refuse("path", &format!("opens no tty: {error}")))
}

/// Returns the links that may go down before `serve` ends, none where the
/// request names no `downs`.
fn downs_of(fields: &Fields<'_>) -> Result<Option<u64>, JsString> {
    fields.opt_u64("downs")
}

/// Answers `serve` on USB bulk.
pub(super) fn serve_bulk(fields: &Fields<'_>) -> Result<JsObject, JsString> {
    let max_time = Duration::from_millis(fields.int_or("max_time_ms", 600_000)?);
    let downs = downs_of(fields)?;
    let card = card_of(fields)?;
    let mut bulk = open_bulk(fields)?;
    match behind_of(fields)? {
        Behind::Instance(yoga) => {
            let listener = Listener::new(yoga_fetch(yoga), card)
                .map_err(|error| JsString::from(format!("listener: {error}")))?;
            let counts = Rc::new(Counts::default());
            let served = serve_bulk_counted(bulk, listener, Rc::clone(&counts));
            let watched = Rc::clone(&counts);
            let until = move || downs.is_some_and(|downs| watched.downs.get() >= downs);
            let (stopped, time_ms) =
                run_served_until(served, max_time, until).map_err(JsString::from)?;
            let mut payload = JsObject::new();
            stopped_json(&mut payload, &stopped, time_ms);
            payload.insert("counts", counts_json(&counts, Some(counts.bulk.get())));
            Ok(payload)
        }
        Behind::Responder(responder) => {
            let seeded = seed().map_err(|error| JsString::from(format!("seed: {error}")))?;
            let (config, max_message) = link_config(card, seeded);
            let profile = bulk_profile(bulk.speed());
            let local = link_limits(&profile, max_message);
            let mut endpoint =
                EchoEndpoint::new(config, profile, Owned::new(&config)).map_err(config_refused)?;
            let idle = Duration::from_micros(fields.int_or("idle_us", 200)?);
            let mut up = false;
            let (stopped, counts, time_ms) = echo_run(
                &mut bulk,
                &mut endpoint,
                responder,
                max_time,
                idle,
                |link, endpoint, counts| {
                    while let Some(event) = link.poll_event().map_err(|error| error.to_string())? {
                        if event == FfsEvent::Enable && !up {
                            endpoint
                                .link_up((), local, local)
                                .map_err(|error| format!("link: {error}"))?;
                            up = true;
                            counts.ups += 1;
                        }
                    }
                    if up && !link.enabled() {
                        // A peer whose link is down already needs nothing.
                        let _ = endpoint.link_down(());
                        while endpoint.poll_event().is_some() {}
                        up = false;
                        counts.downs += 1;
                        if downs.is_some_and(|downs| counts.downs >= downs) {
                            return Ok(LinkTurn::Ended);
                        }
                    }
                    Ok(LinkTurn::Running)
                },
            );
            let mut payload = JsObject::new();
            stopped_json(&mut payload, &stopped, time_ms);
            payload.insert("echo", counts.json());
            payload.insert("bulk", bulk_json(&bulk.counts()));
            payload.insert("max_packet_size", u32::from(bulk.max_packet_size()));
            payload.insert("stats", endpoint_stats(&endpoint.stats()));
            Ok(payload)
        }
    }
}

/// Answers `serve` on USB serial.
pub(super) fn serve_serial(fields: &Fields<'_>) -> Result<JsObject, JsString> {
    let max_time = Duration::from_millis(fields.int_or("max_time_ms", 600_000)?);
    let card = card_of(fields)?;
    let tty = open_tty(fields)?;
    match behind_of(fields)? {
        Behind::Instance(yoga) => {
            let listener = Listener::new(yoga_fetch(yoga), card)
                .map_err(|error| JsString::from(format!("listener: {error}")))?;
            let counts = Rc::new(Counts::default());
            let served = serve_serial_counted(tty, listener, Rc::clone(&counts));
            let (stopped, time_ms) =
                run_served_until(served, max_time, || false).map_err(JsString::from)?;
            let mut payload = JsObject::new();
            stopped_json(&mut payload, &stopped, time_ms);
            payload.insert("counts", counts_json(&counts, None));
            Ok(payload)
        }
        Behind::Responder(responder) => {
            let seeded = seed().map_err(|error| JsString::from(format!("seed: {error}")))?;
            let (config, max_message) = link_config(card, seeded);
            let profile = serial_profile();
            let local = link_limits(&profile, max_message);
            let mut endpoint =
                EchoEndpoint::new(config, profile, Owned::new(&config)).map_err(config_refused)?;
            endpoint
                .link_up((), local, local)
                .map_err(|error| JsString::from(format!("link: {error}")))?;
            let largest = usize::from(profile.max_packet);
            let mut rx = vec![0u8; Decoder::buffer_len(Mode::Delimited, largest)];
            let mut tx = vec![0u8; 4 * frame_len(Mode::Delimited, largest)];
            let mut framed = Framed::new(tty, Mode::Delimited, &mut rx, &mut tx);
            let idle = Duration::from_micros(fields.int_or("idle_us", 200)?);
            let (stopped, counts, time_ms) = echo_run(
                &mut framed,
                &mut endpoint,
                responder,
                max_time,
                idle,
                |link, _, _| {
                    Ok(if link.closed() {
                        LinkTurn::Ended
                    } else {
                        LinkTurn::Running
                    })
                },
            );
            let mut payload = JsObject::new();
            stopped_json(&mut payload, &stopped, time_ms);
            payload.insert("echo", counts.json());
            payload.insert("dropped", framed.dropped());
            payload.insert("stats", endpoint_stats(&endpoint.stats()));
            Ok(payload)
        }
    }
}

/// Answers `baseline` on USB bulk: prefixed frames of whole messages cut to
/// transfers of the largest packet, on a function the host has enabled.
pub(super) fn baseline_bulk(fields: &Fields<'_>) -> Result<JsObject, JsString> {
    let mut bulk = open_bulk(fields)?;
    let max_packet = bulk.max_packet();
    let deadline = std::time::Instant::now() + Duration::from_mins(1);
    while !bulk.enabled() {
        if bulk
            .poll_event()
            .map_err(|error| JsString::from(error.to_string()))?
            .is_none()
        {
            if std::time::Instant::now() > deadline {
                return Err(JsString::from(
                    "baseline: the host did not enable the function",
                ));
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    let stream = EnabledBulk(PacketStream::new(bulk, max_packet));
    baseline::serve_stream(fields, stream, Mode::Prefixed)
}

/// The byte stream of the bulk baseline, which reads the events of `ep0`
/// ahead of each read and fails once the host disabled the function, so the
/// baseline ends with the link.
struct EnabledBulk(PacketStream<Bulk>);

impl ByteStream for EnabledBulk {
    type Error = io::Error;

    fn read(&mut self, buf: &mut [u8]) -> io::Result<Option<usize>> {
        let bulk = self.0.carrier_mut();
        while bulk.poll_event()?.is_some() {}
        if !bulk.enabled() {
            return Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "the host disabled the function",
            ));
        }
        self.0.read(buf)
    }

    fn write(&mut self, bytes: &[u8]) -> io::Result<Option<usize>> {
        self.0.write(bytes)
    }
}

/// Answers `baseline` on USB serial: delimited frames of whole messages on
/// the tty.
pub(super) fn baseline_serial(fields: &Fields<'_>) -> Result<JsObject, JsString> {
    let tty = open_tty(fields)?;
    baseline::serve_stream(fields, tty, Mode::Delimited)
}

/// Returns what a served function counted as an object.
fn counts_json(counts: &Counts, bulk: Option<BulkCounts>) -> JsObject {
    let mut object = JsObject::new();
    object.insert("answered", counts.answered.get());
    object.insert("ups", counts.ups.get());
    object.insert("downs", counts.downs.get());
    object.insert("dropped", counts.dropped.get());
    if let Some(bulk) = bulk {
        object.insert("bulk", bulk_json(&bulk));
    }
    object.insert("stats", endpoint_stats(&counts.stats.get()));
    object
}

/// Returns what the bulk function counted as an object.
fn bulk_json(counts: &BulkCounts) -> JsObject {
    let mut object = JsObject::with_capacity(8);
    object.insert("transfers_in", counts.transfers_in);
    object.insert("transfers_out", counts.transfers_out);
    object.insert("zero_length", counts.zero_length);
    object.insert("dropped_down", counts.dropped_down);
    object.insert("relays_started", counts.relays_started);
    object.insert("relays_ended", counts.relays_ended);
    object.insert("events", counts.events);
    object.insert("stalled", counts.stalled);
    object
}
