// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool carrier serial: Serial Line behind the primitive carrier,
 * the driver of aosabi-carrier-serial on a terminal and its baseline.
 *
 * No analogue in the reference. The terminal is one slave of the pair that
 * tests/harness/homa/pty.py creates and puts in raw mode, which the driver
 * opens by its path through std::fs::File, so the devtool calls no openpty
 * and holds no unsafe. The judging notebook is
 * tests/notebooks/26_carrier_serial.ipynb.
 */

//! Serial Line behind the primitive `carrier`.
//!
//! `serve` opens the terminal at `path` and runs `serve_counted` of the
//! crate at `baud` over the listener in front of a Yoga instance, or one
//! endpoint of the engine with a responder of bytes over the same framing,
//! until the terminal fails, which a pair whose other end closed answers
//! with `EIO`, or `max_time_ms` passes. `baseline` answers whole messages in
//! delimited `AoFrame` alone on the same terminal.

use std::{
    path::Path,
    rc::Rc,
    time::Duration, //
};

use aosabi_carrier_serial::{
    Counts,
    Tty,
    limits as line_limits,
    line_config,
    profile,
    seed,
    serve_counted, //
};
use aosabi_core::graphtp::Listener;
use aosabi_json::{
    JsObject,
    JsString, //
};
use homa::{
    Owned,
    stream::{
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
    baseline,
    behind_of,
    card_of,
    config_refused,
    echo_run,
    endpoint_stats,
    limits_of,
    run_served_until,
    stopped_json,
    yoga_fetch, //
};
use crate::homa_packet::Fields;

/// Opens the terminal a request names under `path`.
fn open(fields: &Fields<'_>) -> Result<Tty, JsString> {
    let path = fields.str("path")?;
    Tty::open(Path::new(path))
        .map_err(|error| fields.refuse("path", &format!("opens no terminal: {error}")))
}

/// Answers `serve` on Serial Line.
pub(super) fn serve(fields: &Fields<'_>) -> Result<JsObject, JsString> {
    let baud: u32 = fields.int_or("baud", 115_200)?;
    let max_time = Duration::from_millis(fields.int_or("max_time_ms", 600_000)?);
    let card = card_of(fields)?;
    let tty = open(fields)?;
    match behind_of(fields)? {
        Behind::Instance(yoga) => {
            let listener = Listener::new(yoga_fetch(yoga), card)
                .map_err(|error| JsString::from(format!("listener: {error}")))?;
            let counts = Rc::new(Counts::default());
            let served = serve_counted(tty, baud, listener, Rc::clone(&counts));
            let (stopped, time_ms) =
                run_served_until(served, max_time, || false).map_err(JsString::from)?;
            let mut payload = JsObject::new();
            stopped_json(&mut payload, &stopped, time_ms);
            payload.insert("counts", counts_json(&counts));
            Ok(payload)
        }
        Behind::Responder(responder) => {
            let seeded = seed().map_err(|error| JsString::from(format!("seed: {error}")))?;
            let (config, max_message) = line_config(card, seeded);
            let line = profile(baud);
            let local = limits_of(fields, "limits", line_limits(baud, max_message))?;
            let mut endpoint =
                EchoEndpoint::new(config, line, Owned::new(&config)).map_err(config_refused)?;
            endpoint
                .link_up((), local, local)
                .map_err(|error| JsString::from(format!("link: {error}")))?;
            let largest = usize::from(line.max_packet);
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

/// Returns what a served line counted as an object.
fn counts_json(counts: &Counts) -> JsObject {
    let mut object = JsObject::with_capacity(5);
    object.insert("bytes_in", counts.bytes_in.get());
    object.insert("bytes_out", counts.bytes_out.get());
    object.insert("dropped", counts.dropped.get());
    object.insert("answered", counts.answered.get());
    object.insert("stats", endpoint_stats(&counts.stats.get()));
    object
}

/// Answers `baseline` on Serial Line: whole messages in delimited `AoFrame`
/// alone on the terminal.
pub(super) fn baseline(fields: &Fields<'_>) -> Result<JsObject, JsString> {
    let tty = open(fields)?;
    baseline::serve_stream(fields, tty, Mode::Delimited)
}
