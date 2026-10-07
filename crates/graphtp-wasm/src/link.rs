// SPDX-License-Identifier: MIT
/*
 * graphtp-wasm link: the engine of one link of the package, the client role
 * of homa, the client of graphtp-homa and the framing of AoFrame, driven by
 * the bytes and the time the glue hands it.
 *
 * No analogue in the reference. The exchanges, the repeat mark, the start
 * rule and the order of seq are those of docs/graphtp.md sections 2.4 and
 * 2.7, and the outcomes the glue turns into a Response or a rejection those
 * of docs/graphtp-fetch.md sections 6 and 8.
 */

//! The engine of one link.
//!
//! A [`Link`] holds one Homa endpoint of the client role toward one server,
//! the sans I/O client of `graphtp-homa` over it, and the framing of the
//! carrier: prefixed `AoFrame` on a stream socket and the Bluetooth channel,
//! delimited `AoFrame` on a serial line, and one packet per record where the
//! carrier keeps packet boundaries. The glue hands it the bytes the link
//! received and takes the bytes it sends, and it hands the glue one event
//! record per outcome, so the glue never reads a byte of the protocol.
//!
//! Each OPERATION the glue asks for gets a token, and every outcome of it
//! arrives as an event that names the token: its answer, an unknown outcome, a
//! link down, a malformed answer, a message longer than the peer takes, or no
//! link. A marked OPERATION is repeated once after `RPC_UNKNOWN` or a dead
//! peer, and reported as unknown after a second one. The CANCEL of handle 0 of
//! the start rule leaves on its own, and a PULL leaves once the glue asks for
//! one, so the glue's queues bound how far ahead the streams run.
//!
//! An event record opens with a head of 16 bytes, every number little endian:
//! the kind, the substatus, the status in 2 bytes, the token in 4, the handle
//! of a stream in 2, the length of the name in 1, a byte of 0, and the length
//! of the bytes in 4. The name of the code follows, then the bytes, the
//! decoded body of a result or the value of a `next`.

#![forbid(unsafe_code)]

use std::collections::VecDeque;

use graphtp::{kind::Kind, status::Code, stream::Target, wire::WireError, zlib};
use graphtp_homa::client::{
    Client,
    Config as ClientConfig,
    Error as ClientError,
    Event as StreamEvent,
    Incoming,
    Link as Sight,
    Outgoing,
    Repeat, //
};
use homa::{
    Config, Endpoint, Event, Failure, Gone, Limits, Nanos, Owned, Profile, Read, RpcId, SendError,
    stream::{
        self,
        Decoder,
        Mode,
        Place, //
    },
    wire::{
        Compact,
        Form, //
    },
};

use crate::codec;

/// The endpoint of a link: one peer, storage it owns, the compact form.
type Engine = Endpoint<(), Owned<()>, Compact>;

/// The event of a result of an OPERATION or of a CANCEL the glue sent.
pub const RESULT: u8 = 1;

/// The event of an OPERATION whose outcome is unknown.
pub const UNKNOWN: u8 = 2;

/// The event of an exchange the link going down or the peer dying ended.
pub const LINK_DOWN: u8 = 3;

/// The event of a malformed answer, with its reason as the substatus, by
/// [`codec::wire_error`].
pub const MALFORMED: u8 = 4;

/// The event of a request longer than the peer takes, or of an answer whose
/// decoded body passes the bound.
pub const TOO_LARGE: u8 = 5;

/// The event of a stream an OPERATION opened, with its handle.
pub const OPENED: u8 = 6;

/// The event of one result of a stream.
pub const NEXT: u8 = 7;

/// The event of a stream its source ended.
pub const COMPLETE: u8 = 8;

/// The event of a stream a CANCEL ended.
pub const CANCELLED: u8 = 9;

/// The event of a stream whose source failed.
pub const ABORTED: u8 = 10;

/// The event of a stream that ended with no entry of the server.
pub const LOST: u8 = 11;

/// The event of an OPERATION sent on no link.
pub const NO_LINK: u8 = 12;

/// The event of a message the allocator refused.
pub const MEMORY: u8 = 13;

/// The bytes of the head of an event record.
pub const HEAD_LEN: usize = 16;

/// The client slots of the endpoint, the OPERATIONs, the PULL and the CANCELs
/// in flight at once.
const CLIENT_SLOTS: u32 = 8;

/// The bytes of one page of the endpoint's pool, so a message of up to 1 MiB
/// takes the 16 pages a slot may span.
const PAGE_LEN: u32 = 65_536;

/// The pages of the pool, eight a slot, 4 MiB in all.
const PAGES: u32 = 64;

/// The RPCs this end lets a server have open at it, which no server uses.
const MAX_RPCS: u8 = 16;

/// The carrier of a link and its framing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Framing {
    /// Prefixed `AoFrame`, for a stream socket and the Bluetooth channel.
    Prefixed,
    /// Delimited `AoFrame`, for a serial line.
    Delimited,
    /// One packet per record, for a carrier that keeps packet boundaries.
    Packet,
}

impl Framing {
    /// Returns the framing the number `value` names, 0 prefixed, 1 delimited
    /// and 2 one packet per record.
    #[must_use]
    pub const fn from_u32(value: u32) -> Option<Framing> {
        match value {
            0 => Some(Framing::Prefixed),
            1 => Some(Framing::Delimited),
            2 => Some(Framing::Packet),
            _ => None,
        }
    }

    /// Returns the form of `AoFrame` of the framing, `None` for one packet per
    /// record.
    const fn mode(self) -> Option<Mode> {
        match self {
            Framing::Prefixed => Some(Mode::Prefixed),
            Framing::Delimited => Some(Mode::Delimited),
            Framing::Packet => None,
        }
    }
}

/// What a link is built with.
#[derive(Clone, Copy, Debug)]
pub struct Settings {
    /// The profile of the carrier.
    pub profile: Profile,
    /// The framing of the carrier.
    pub framing: Framing,
    /// The longest message this end takes, the answers of the server.
    pub max_message: u32,
    /// The longest decoded body the client reads.
    pub max_body: usize,
    /// Whether the server sees the end of an earlier run of the client.
    pub sight: Sight,
    /// The randomness of the glue, which picks the first identifier and the
    /// key of the table hash.
    pub seed: u64,
}

/// Returns the profile the number `value` names with its parameter `param`,
/// 0 Unix Socket, 1 USB serial, 2 USB bulk at high speed where `param` is not
/// 0 and at full speed otherwise, and 3 a serial line at `param` baud.
#[must_use]
pub const fn profile_of(value: u32, param: u32, max_message: u32) -> Option<Profile> {
    match value {
        0 => Some(Profile::unix_socket(max_message)),
        1 => Some(Profile::usb_serial()),
        2 => Some(Profile::usb_bulk(param != 0)),
        3 => Some(Profile::serial_line(param)),
        _ => None,
    }
}

/// A request in flight, with the token the glue knows it by.
#[derive(Clone, Copy, Debug)]
struct Sent {
    /// The RPC that carries it.
    id: RpcId,
    /// Its token, 0 for a request the link sent on its own.
    token: u32,
    /// Its kind.
    kind: Kind,
    /// Whether it was repeated once already.
    repeated: bool,
}

/// An OPERATION that waits for the count of `seq` or a free slot.
#[derive(Debug)]
struct Waiting {
    /// Its token.
    token: u32,
    /// Its credential, empty for none.
    credential: Vec<u8>,
    /// Its content.
    content: Vec<u8>,
    /// Its mark.
    repeat: Repeat,
}

/// The engine of one link.
pub struct Link {
    /// The Homa endpoint.
    engine: Engine,
    /// The client of `GraphTP`.
    client: Client,
    /// The framing of the carrier.
    framing: Framing,
    /// What this end announces and assumes of the server.
    limits: Limits,
    /// Whether the link is up.
    up: bool,
    /// The buffer the frame decoder collects a packet in.
    frame: Vec<u8>,
    /// Where the frame decoder stands in the received bytes, `None` where the
    /// carrier keeps packet boundaries.
    place: Option<Place>,
    /// The buffer a frame is written in.
    tx: Vec<u8>,
    /// The requests in flight.
    pending: Vec<Sent>,
    /// The tokens of the OPERATIONs whose stream opened and whose event
    /// `Opened` waits for its turn in the order of `seq`.
    opening: Vec<(RpcId, u32)>,
    /// The OPERATIONs that wait.
    waiting: VecDeque<Waiting>,
    /// The event records the glue has not taken.
    events: VecDeque<Vec<u8>>,
    /// The last token handed out.
    token: u32,
    /// Whether the glue asked for a PULL.
    want_pull: bool,
    /// The frames the decoder dropped.
    dropped: u64,
}

impl Link {
    /// Returns a link of the settings `settings`, with its link down.
    ///
    /// # Errors
    ///
    /// Returns the name of what cannot be built, the endpoint or the
    /// encoder's scratch of the client.
    pub fn new(settings: &Settings) -> Result<Link, &'static str> {
        let config = config(settings.seed);
        let engine =
            Engine::new(config, settings.profile, Owned::new(&config)).map_err(|_| "endpoint")?;
        let client = Client::new(ClientConfig {
            max_body: settings.max_body,
            link: settings.sight,
            coding: zlib::PARAMS,
        })
        .map_err(|_| "scratch")?;
        let max_packet = usize::from(settings.profile.max_packet);
        let frame = match settings.framing.mode() {
            Some(mode) => vec![0; Decoder::buffer_len(mode, max_packet)],
            None => Vec::new(),
        };
        Ok(Link {
            engine,
            client,
            framing: settings.framing,
            limits: Limits {
                max_packet: settings.profile.max_packet,
                max_message: settings.max_message,
                unsched_bytes: settings.profile.unsched_bytes,
                max_rpcs: MAX_RPCS,
            },
            up: false,
            frame,
            place: settings.framing.mode().map(Place::start),
            tx: Vec::new(),
            pending: Vec::new(),
            opening: Vec::new(),
            waiting: VecDeque::new(),
            events: VecDeque::new(),
            token: 0,
            want_pull: false,
            dropped: 0,
        })
    }

    /// Brings the link up once the carrier is open, and tells whether it is
    /// up.
    pub fn link_up(&mut self) -> bool {
        if !self.up {
            self.place = self.framing.mode().map(Place::start);
            self.up = self.engine.link_up((), self.limits, self.limits).is_ok();
        }
        self.up
    }

    /// Takes the link down once the carrier closed: every exchange in flight
    /// and every waiting OPERATION ends with its event, and every stream is
    /// lost.
    pub fn link_down(&mut self) {
        if self.up {
            self.up = false;
            // A peer whose link is down already needs nothing.
            let _ = self.engine.link_down(());
            self.drive();
        }
        while let Some(waiting) = self.waiting.pop_front() {
            self.event(LINK_DOWN, waiting.token, None, 0, &[]);
        }
        self.place = self.framing.mode().map(Place::start);
    }

    /// Takes the bytes `bytes` the carrier received at `now` and returns the
    /// packets it handed the endpoint.
    pub fn push_rx(&mut self, bytes: &[u8], now: Nanos) -> u32 {
        let taken = match self.place {
            Some(place) => self.unframe(place, bytes, now),
            None => u32::from(self.engine.handle(now, (), bytes).is_ok()),
        };
        self.drive();
        self.flush();
        self.drive();
        taken
    }

    /// Writes the bytes the carrier sends at `now` into `out`, every frame
    /// that is ready, or the one next packet where the carrier keeps packet
    /// boundaries.
    pub fn poll_tx(&mut self, now: Nanos, out: &mut Vec<u8>) {
        self.flush();
        self.drive();
        let Self {
            engine,
            framing,
            tx,
            ..
        } = self;
        while let Some(packet) = engine.poll_transmit(now, |(), _| true) {
            let Some(mode) = framing.mode() else {
                out.extend_from_slice(packet.header);
                out.extend_from_slice(packet.payload);
                return;
            };
            let len = packet.header.len() + packet.payload.len();
            tx.resize(stream::frame_len(mode, len), 0);
            if let Ok(written) = stream::encode(mode, packet.header, packet.payload, tx) {
                out.extend_from_slice(tx.get(..written).unwrap_or_default());
            }
        }
    }

    /// Moves the next event record into `out`, and tells whether there was
    /// one.
    pub fn poll_event(&mut self, out: &mut Vec<u8>) -> bool {
        match self.events.pop_front() {
            Some(record) => {
                out.extend_from_slice(&record);
                true
            }
            None => false,
        }
    }

    /// Asks for an OPERATION with the credential `credential`, none when
    /// empty, and the content `content`, marked by `repeat`, and returns its
    /// token.
    ///
    /// The OPERATION leaves at once, or waits for the count of `seq` or a free
    /// slot, and every outcome of it arrives as an event with the token.
    pub fn operation(&mut self, credential: &[u8], content: &[u8], repeat: Repeat) -> u32 {
        let token = self.next_token();
        if !self.up {
            self.event(NO_LINK, token, None, 0, &[]);
            return token;
        }
        self.waiting.push_back(Waiting {
            token,
            credential: credential.to_vec(),
            content: content.to_vec(),
            repeat,
        });
        self.flush();
        self.drive();
        token
    }

    /// Asks for one PULL, which leaves once the client holds a stream, no
    /// PULL is in flight and the count of `seq` is known.
    pub fn pull(&mut self) {
        self.want_pull = true;
        self.flush();
    }

    /// Sends a CANCEL of the stream `handle`, of every stream for 0, and
    /// returns its token.
    pub fn cancel(&mut self, handle: u16) -> u32 {
        let token = self.next_token();
        match self.client.cancel(Target::from_u16(handle)) {
            Ok(outgoing) => self.send(token, outgoing, false),
            Err(_) => self.event(MEMORY, token, None, 0, &[]),
        }
        self.drive();
        token
    }

    /// Ends the exchange of the token `token` without telling the server,
    /// and tells whether one was in flight or waiting.
    ///
    /// The server learns of it from the `RPC_UNKNOWN` this end answers when it
    /// next asks, and no event of the token follows.
    pub fn abort(&mut self, token: u32) -> bool {
        if let Some(index) = self.waiting.iter().position(|w| w.token == token) {
            self.waiting.remove(index);
            return true;
        }
        let Some(index) = self.pending.iter().position(|sent| sent.token == token) else {
            return false;
        };
        let sent = self.pending.remove(index);
        // An RPC that ended already needs nothing.
        let _ = self.engine.abort(sent.id);
        let _ = self.client.failure(sent.id, Failure::Aborted);
        true
    }

    /// Runs the timers of the endpoint at `now`.
    pub fn handle_timeout(&mut self, now: Nanos) {
        // A clock that went backwards moves no timer.
        let _ = self.engine.handle_timeout(now);
        self.drive();
        self.flush();
    }

    /// Returns when the endpoint next needs its timers run.
    #[must_use]
    pub fn next_timeout(&self) -> Option<Nanos> {
        self.engine.next_timeout()
    }

    /// Returns the frames the decoder dropped.
    #[must_use]
    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    /// Hands the endpoint every packet the framed bytes `bytes` complete, with
    /// the decoder at `place`, and returns how many it took.
    ///
    /// The decoder takes up the stream where the last call left it, so each
    /// received byte passes it once, and a frame split across calls is
    /// collected in the buffer of the link, the only received bytes the link
    /// keeps. A prefixed frame longer than that buffer is refused at its prefix
    /// and its bytes are skipped as they arrive, and a delimited frame that
    /// overflows it is dropped at its next delimiter, so no prefix a peer sends
    /// makes the link keep more. A corrupt prefix loses the frames inside the
    /// length it announces, and on a prefixed stream the frames after it until
    /// a prefix reads true again, and the timers of Homa then end the exchanges
    /// as on a dead peer, with [`LINK_DOWN`], while the delimited form finds
    /// the next frame at its delimiter. No received byte grows the memory of
    /// the module here, and an allocation refused elsewhere in the call traps
    /// the export, which the glue reports as a fault: it drops the module and
    /// ends each link with `LINK_DOWN`.
    fn unframe(&mut self, place: Place, bytes: &[u8], now: Nanos) -> u32 {
        let Self {
            engine,
            frame,
            place: kept,
            dropped,
            ..
        } = self;
        let mut decoder = Decoder::resume(frame, place);
        let mut taken = 0u32;
        for &byte in bytes {
            match decoder.push(byte) {
                Some(Ok(packet)) => taken += u32::from(engine.handle(now, (), packet).is_ok()),
                Some(Err(_)) => *dropped += 1,
                None => {}
            }
        }
        *kept = Some(decoder.place());
        taken
    }

    /// Sends what the client needs sent: the CANCEL of handle 0 while the
    /// count of `seq` is unknown, a PULL the glue asked for, and the waiting
    /// OPERATIONs in their order.
    fn flush(&mut self) {
        if !self.up {
            return;
        }
        while let Some(outgoing) = self.client.poll() {
            if outgoing.kind() == Kind::Pull && !self.want_pull {
                break;
            }
            let kind = outgoing.kind();
            match self.engine.request((), &outgoing.message, outgoing.options) {
                Ok(id) => {
                    self.client.sent(id, outgoing);
                    self.pending.push(Sent {
                        id,
                        token: 0,
                        kind,
                        repeated: false,
                    });
                    if kind == Kind::Pull {
                        self.want_pull = false;
                    }
                }
                // A full endpoint takes the request on a later turn.
                Err(_) => break,
            }
        }
        while self.take_waiting() {}
    }

    /// Sends the first waiting OPERATION, and tells whether the next may
    /// follow.
    fn take_waiting(&mut self) -> bool {
        let Some(waiting) = self.waiting.front() else {
            return false;
        };
        let token = waiting.token;
        let written =
            self.client
                .operation(Some(&waiting.credential), &waiting.content, waiting.repeat);
        let outgoing = match written {
            Ok(outgoing) => outgoing,
            Err(ClientError::Count | ClientError::Parked) => return false,
            Err(ClientError::Credential(code)) => {
                self.waiting.pop_front();
                self.event(RESULT, token, Some(code), 0, &[]);
                return true;
            }
            Err(ClientError::Memory) => {
                self.waiting.pop_front();
                self.event(MEMORY, token, None, 0, &[]);
                return true;
            }
        };
        match self.engine.request((), &outgoing.message, outgoing.options) {
            Ok(id) => {
                self.waiting.pop_front();
                self.client.sent(id, outgoing);
                self.pending.push(Sent {
                    id,
                    token,
                    kind: Kind::Operation,
                    repeated: false,
                });
                true
            }
            Err(SendError::TooLong) => {
                self.waiting.pop_front();
                self.event(TOO_LARGE, token, None, 0, &[]);
                true
            }
            Err(SendError::NoPeer) => {
                self.waiting.pop_front();
                self.event(NO_LINK, token, None, 0, &[]);
                true
            }
            // A full endpoint takes the OPERATION on a later turn.
            Err(_) => false,
        }
    }

    /// Sends the request `outgoing` of the token `token`, a repeat where
    /// `repeated` is true, or reports why it cannot leave.
    fn send(&mut self, token: u32, outgoing: Outgoing, repeated: bool) {
        let kind = outgoing.kind();
        match self.engine.request((), &outgoing.message, outgoing.options) {
            Ok(id) => {
                self.client.sent(id, outgoing);
                self.pending.push(Sent {
                    id,
                    token,
                    kind,
                    repeated,
                });
            }
            Err(SendError::TooLong) => self.event(TOO_LARGE, token, None, 0, &[]),
            Err(SendError::NoPeer) => self.event(NO_LINK, token, None, 0, &[]),
            Err(_) => self.event(UNKNOWN, token, None, 0, &[]),
        }
    }

    /// Takes every event of the endpoint and turns each into what the client
    /// reads and the glue receives.
    fn drive(&mut self) {
        while let Some(event) = self.engine.poll_event() {
            match event {
                Event::Response { id } => {
                    let message = self.engine.response(id).map(|reader| whole(&reader));
                    // The client holds what it needs of the answer.
                    let _ = self.engine.release(id);
                    if let Some(message) = message {
                        let incoming = self.client.answer(id, &message);
                        self.deliver(id, incoming, None);
                    }
                }
                Event::Failed { id, why } => {
                    let incoming = self.client.failure(id, why);
                    self.deliver(id, incoming, Some(why));
                }
                Event::PeerGone {
                    why: Gone::LinkDown,
                    ..
                } => {
                    let lost = self.client.link_down();
                    self.streams(&lost);
                }
                _ => {}
            }
        }
    }

    /// Hands the glue what the answer or the failure `why` of the RPC `id`
    /// gave, as `incoming`.
    ///
    /// A request longer than the peer takes is the event [`TOO_LARGE`] where
    /// the client reads 413.0, since nothing of it reached the server.
    fn deliver(&mut self, id: RpcId, incoming: Incoming, why: Option<Failure>) {
        let Some(index) = self.pending.iter().position(|sent| sent.id == id) else {
            return;
        };
        let sent = self.pending.remove(index);
        let token = sent.token;
        if token == 0 {
            self.deliver_own(sent, incoming);
            return;
        }
        if why == Some(Failure::TooLong) {
            self.event(TOO_LARGE, token, None, 0, &[]);
            return;
        }
        match incoming {
            Incoming::Result(response) => {
                let (parts, body) = response.into_parts();
                self.event(RESULT, token, Some(parts.code()), 0, &body);
            }
            Incoming::Stream(events) => {
                if sent.kind == Kind::Operation {
                    self.opening.push((id, token));
                }
                self.streams(&events);
            }
            Incoming::Repeat(outgoing) if !sent.repeated => self.send(token, outgoing, true),
            Incoming::Unknown | Incoming::Repeat(_) => {
                let kind = if why == Some(Failure::PeerTimeout) {
                    LINK_DOWN
                } else {
                    UNKNOWN
                };
                self.event(kind, token, None, 0, &[]);
            }
            Incoming::LinkDown => self.event(LINK_DOWN, token, None, 0, &[]),
            Incoming::Malformed(WireError::Bound) => self.event(TOO_LARGE, token, None, 0, &[]),
            Incoming::Malformed(error) => {
                self.event(MALFORMED, token, None, codec::wire_error(error).into(), &[]);
            }
            Incoming::Stray => {}
        }
    }

    /// Hands on what a request the link sent on its own gave, the CANCEL of
    /// handle 0 of the start rule or a PULL: the stream events of its answer,
    /// and its repeat once.
    ///
    /// Every other outcome needs nothing, since the client yields the request
    /// again while it still needs it.
    fn deliver_own(&mut self, sent: Sent, incoming: Incoming) {
        match incoming {
            Incoming::Stream(events) => self.streams(&events),
            Incoming::Repeat(outgoing) if !sent.repeated => self.send(0, outgoing, true),
            _ => {}
        }
    }

    /// Hands the glue the stream events `events` in their order.
    fn streams(&mut self, events: &[StreamEvent]) {
        for event in events {
            match event {
                StreamEvent::Opened(id, handle) => {
                    let token = match self.opening.iter().position(|(held, _)| held == id) {
                        Some(index) => self.opening.remove(index).1,
                        None => 0,
                    };
                    self.event(OPENED, token, None, handle.get(), &[]);
                }
                StreamEvent::Next(handle, value) => self.event(NEXT, 0, None, handle.get(), value),
                StreamEvent::Complete(handle) => self.event(COMPLETE, 0, None, handle.get(), &[]),
                StreamEvent::Cancelled(handle) => {
                    self.event(CANCELLED, 0, None, handle.get(), &[]);
                }
                StreamEvent::Aborted(handle) => self.event(ABORTED, 0, None, handle.get(), &[]),
                StreamEvent::Lost(handle) => self.event(LOST, 0, None, handle.get(), &[]),
            }
        }
    }

    /// Queues the event record of the kind `kind` for the token `token`, with
    /// the code `code`, the handle `handle` and the bytes `bytes`.
    ///
    /// A malformed answer carries its reason in `handle`, which the record
    /// writes as its substatus.
    fn event(&mut self, kind: u8, token: u32, code: Option<Code>, handle: u16, bytes: &[u8]) {
        let (status, substatus) =
            code.map_or((0, 0), |code| (code.status.as_u16(), code.substatus.get()));
        let (substatus, handle) = if kind == MALFORMED {
            (u8::try_from(handle).unwrap_or(u8::MAX), 0)
        } else {
            (substatus, handle)
        };
        let name = code
            .and_then(|code| {
                codec::text_of(code.status.as_u16().into(), code.substatus.get().into())
            })
            .unwrap_or_default()
            .as_bytes();
        let name_len = u8::try_from(name.len()).unwrap_or(0);
        let mut record = Vec::with_capacity(HEAD_LEN + name.len() + bytes.len());
        record.extend_from_slice(&[kind, substatus]);
        codec::put_u16(&mut record, status);
        codec::put_u32(&mut record, token);
        codec::put_u16(&mut record, handle);
        record.extend_from_slice(&[name_len, 0]);
        codec::put_u32(&mut record, codec::len_u32(bytes.len()));
        record.extend_from_slice(name.get(..usize::from(name_len)).unwrap_or_default());
        record.extend_from_slice(bytes);
        self.events.push_back(record);
    }

    /// Returns the next token, never 0.
    fn next_token(&mut self) -> u32 {
        self.token = self.token.wrapping_add(1).max(1);
        self.token
    }
}

/// Returns the configuration of the endpoint of a link, whose first
/// identifier and hash key come from `seed`.
fn config(seed: u64) -> Config {
    let mask = if Compact::ID_BITS >= 64 {
        u64::MAX
    } else {
        (1u64 << Compact::ID_BITS) - 1
    };
    let first_id = match (seed << 1) & mask {
        0 => 2,
        id => id,
    };
    let key = seed.rotate_left(17) ^ 0x9e37_79b9_7f4a_7c15;
    let mut hash_key = [0u8; 16];
    hash_key[..8].copy_from_slice(&seed.to_le_bytes());
    hash_key[8..].copy_from_slice(&key.to_le_bytes());
    Config {
        peers: 1,
        client_slots: CLIENT_SLOTS,
        server_slots: 0,
        page_len: PAGE_LEN,
        pages: PAGES,
        max_gaps: 4,
        size_buckets: 0,
        first_id,
        hash_key,
        port: 0,
        extra_ports: 0,
    }
}

/// Returns the whole message `message`, from one page or copied from
/// several.
fn whole<M: Read>(message: &M) -> Vec<u8> {
    if let Some(bytes) = message.as_slice() {
        return bytes.to_vec();
    }
    let len = usize::try_from(message.len()).unwrap_or(0);
    let mut bytes = vec![0; len];
    let copied = message.read_at(0, &mut bytes);
    bytes.truncate(copied);
    bytes
}
