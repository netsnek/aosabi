// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool carrier baseline: AoFrame alone on the link of a carrier,
 * whole messages each in one frame, first in first out, with no Homa.
 *
 * No analogue in the reference. The baseline is the comparison ADR-0023 asks
 * the plan that owns the carriers for on each carrier, which lives in the
 * devtool and ships in no artifact.
 */

//! The baseline of `AoFrame` alone.
//!
//! [`serve_stream`] reads whole messages in `AoFrame` from a byte stream,
//! each the content of an OPERATION as text, answers each through the same
//! Yoga instance the driver serves, or through the responder of bytes the
//! request names, and writes each whole answer body in one frame, first in
//! first out. A packet carrier becomes a byte stream through
//! [`PacketStream`], which writes the frames in chunks of the carrier's
//! largest packet and reads the chunks back in order, so the baseline of a
//! link of packets is the same frames cut to its packets.

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

use aosabi_json::{
    JsObject,
    JsString, //
};
use homa::{
    Carrier,
    Priority,
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
    Kind,
    Stopped,
    behind_of,
    settle_body,
    stopped_json,
    yoga_fetch, //
};
use crate::homa_packet::Fields;

/// The longest message of the baseline where the request names none, the
/// answer buffer of the card `player`.
const DEFAULT_MAX_MESSAGE: usize = 131_072;

/// What the baseline counted.
#[derive(Debug, Default)]
struct BaselineCounts {
    /// The whole requests read.
    requests: u64,
    /// The whole answers written.
    answered: u64,
    /// The requests whose fetch did not answer.
    unanswered: u64,
}

/// Answers `baseline` over `stream` in the form `mode`, until the stream
/// ends or fails or `max_time_ms` passes.
pub(super) fn serve_stream<S>(
    fields: &Fields<'_>,
    stream: S,
    mode: Mode,
) -> Result<JsObject, JsString>
where
    S: ByteStream<Error = io::Error>,
{
    let max_message = fields.int_or("max_message", DEFAULT_MAX_MESSAGE)?;
    let max_time = Duration::from_millis(fields.int_or("max_time_ms", 600_000)?);
    let idle = Duration::from_micros(fields.int_or("idle_us", 200)?);
    let behind = behind_of(fields)?;
    let mut rx = vec![0u8; Decoder::buffer_len(mode, max_message)];
    let mut tx = vec![0u8; 2 * frame_len(mode, max_message)];
    let mut framed = Framed::new(stream, mode, &mut rx, &mut tx);
    let mut message = vec![0u8; max_message];
    let mut queue: VecDeque<Vec<u8>> = VecDeque::new();
    let mut counts = BaselineCounts::default();
    let start = Instant::now();
    let stopped = loop {
        let mut moved = false;
        loop {
            match framed.recv(&mut message) {
                Ok(Some(((), len))) => {
                    moved = true;
                    counts.requests += 1;
                    let answer = match &behind {
                        Behind::Instance(yoga) => {
                            settle_body(&yoga_fetch(Rc::clone(yoga)), &message[..len])
                        }
                        Behind::Responder(responder) => Ok(responder.answer(&message[..len])),
                    };
                    match answer {
                        Ok(body) => queue.push_back(body),
                        Err(_) => counts.unanswered += 1,
                    }
                }
                Ok(None) => break,
                Err(error) => {
                    return Ok(finish(
                        &Stopped::Link(Some(error.to_string())),
                        &counts,
                        start,
                        &framed,
                    ));
                }
            }
        }
        while queue
            .front()
            .is_some_and(|front| framed.ready((), front.len()))
        {
            let Some(answer) = queue.pop_front() else {
                break;
            };
            if let Err(error) = framed.send((), Priority::HIGHEST, &[], &answer) {
                return Ok(finish(
                    &Stopped::Link(Some(error.to_string())),
                    &counts,
                    start,
                    &framed,
                ));
            }
            counts.answered += 1;
            moved = true;
        }
        if let Err(error) = framed.flush() {
            break Stopped::Link(Some(error.to_string()));
        }
        if framed.closed() {
            break Stopped::Link(None);
        }
        if start.elapsed() > max_time {
            break Stopped::MaxTime;
        }
        if !moved {
            thread::sleep(idle);
        }
    };
    Ok(finish(&stopped, &counts, start, &framed))
}

/// Returns the answer of the baseline.
fn finish<S: ByteStream>(
    stopped: &Stopped,
    counts: &BaselineCounts,
    start: Instant,
    framed: &Framed<'_, S>,
) -> JsObject {
    let mut payload = JsObject::new();
    let time_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
    stopped_json(&mut payload, stopped, time_ms);
    payload.insert("requests", counts.requests);
    payload.insert("answered", counts.answered);
    payload.insert("unanswered", counts.unanswered);
    payload.insert("dropped", framed.dropped());
    payload
}

/// A byte stream over a carrier of packets: a write goes in packets of at
/// most `max_packet` bytes and a read takes the packets back in order.
pub(super) struct PacketStream<C: Carrier<Peer = ()>> {
    /// The carrier.
    carrier: C,
    /// The largest packet of the carrier.
    max_packet: usize,
    /// The packet read and not yet taken.
    held: Vec<u8>,
    /// The first byte of `held` not yet taken.
    at: usize,
}

impl<C: Carrier<Peer = ()>> PacketStream<C> {
    /// Returns the byte stream over `carrier`, whose packets are at most
    /// `max_packet` bytes.
    pub(super) fn new(carrier: C, max_packet: u16) -> Self {
        Self {
            carrier,
            max_packet: usize::from(max_packet),
            held: vec![0; usize::from(max_packet)],
            at: usize::from(max_packet),
        }
    }

    /// Returns the carrier.
    pub(super) const fn carrier_mut(&mut self) -> &mut C {
        &mut self.carrier
    }
}

impl<C: Carrier<Peer = ()>> ByteStream for PacketStream<C> {
    type Error = C::Error;

    fn read(&mut self, buf: &mut [u8]) -> Result<Option<usize>, C::Error> {
        if self.at >= self.held.len() {
            self.held.resize(self.max_packet, 0);
            let Some(((), len)) = self.carrier.recv(&mut self.held)? else {
                self.at = self.held.len();
                return Ok(None);
            };
            self.held.truncate(len);
            self.at = 0;
        }
        let take = (self.held.len() - self.at).min(buf.len());
        buf[..take].copy_from_slice(&self.held[self.at..self.at + take]);
        self.at += take;
        Ok(Some(take))
    }

    fn write(&mut self, bytes: &[u8]) -> Result<Option<usize>, C::Error> {
        let len = bytes.len().min(self.max_packet);
        if len == 0 || !self.carrier.ready((), len) {
            return Ok(None);
        }
        self.carrier
            .send((), Priority::HIGHEST, &[], &bytes[..len])?;
        Ok(Some(len))
    }
}

/// Returns the bytes the framing of a driver holds for packets of
/// `max_packet` bytes and the bytes `AoFrame` alone holds for a whole message
/// of `max_message` bytes each way, on the link of `kind`.
pub(super) fn buffers(kind: Kind, max_packet: u16, max_message: u32) -> (usize, usize) {
    let packet = usize::from(max_packet);
    let message = usize::try_from(max_message).unwrap_or(usize::MAX);
    let (framing, mode) = match kind {
        Kind::Serial | Kind::UsbSerial => (
            Decoder::buffer_len(Mode::Delimited, packet) + 4 * frame_len(Mode::Delimited, packet),
            Mode::Delimited,
        ),
        Kind::Channel => (
            Decoder::buffer_len(Mode::Prefixed, packet) + 4 * frame_len(Mode::Prefixed, packet),
            Mode::Prefixed,
        ),
        Kind::UsbBulk | Kind::Gatt => (packet, Mode::Prefixed),
    };
    let alone = Decoder::buffer_len(mode, message) + frame_len(mode, message);
    (framing, alone)
}
