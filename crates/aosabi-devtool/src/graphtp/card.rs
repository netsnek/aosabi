// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool graphtp card: the card test-graphtp, the GraphTP listener of
 * aosabi-core served over a real Unix Socket, and the bounds of the cards the
 * listener holds.
 *
 * No analogue in the reference. The card is the harness server the plan of the
 * GraphTP track names for the card test-graphtp until card builds exist: the
 * listener built with the feature homa of aosabi-core, a SOCK_SEQPACKET socket
 * and, under AOSABI_UNIX_STREAM=<path>, a stream socket of the prefixed framing
 * beside it. Its service is the harness's own, which a notebook drives through
 * the member extensions.harness of each request.
 */

//! The card `test-graphtp` and the bounds of the cards.
//!
//! - The operation `card {card}` answers the bounds the listener holds for the
//!   card, `server`, `test-graphtp`, `player` or `esp32c3-devkitm1`,
//!   `scratch_made`, the bytes the encoder's scratch of the card holds once
//!   made, `cache_budget`, the budget of the request cache of the card or null
//!   for a card with none, and `cache_budgets`, the budget of every card that
//!   serves the request cache, by name, as `aosabi_core::cache_budget` holds
//!   them.
//! - The operation `serve {path, card, connections, profile, overrides,
//!   config, limits, assumed, max_time_ms, idle_us}` binds a `SOCK_SEQPACKET`
//!   listener at `path` and, where the environment names
//!   `AOSABI_UNIX_STREAM`, a stream listener of prefixed `AoFrame` at that
//!   path, accepts `connections` connections from both, and serves each with
//!   one Homa endpoint of the server role and one `GraphTP` listener with the
//!   bounds of `card`, `test-graphtp` by default, over the service of the
//!   harness. It ends once every connection it accepted has closed, or at
//!   `max_time_ms`, and answers what each connection counted, the runs of the
//!   service, the restarts and the relinks.
//!
//! The service reads the content as a GraphQL request and its member
//! `extensions.harness`. With `{"run": id, "wait_ms": w, "delay_ms": n,
//! "restart": true, "relink": true}` it counts one run of `id` `w`
//! milliseconds after the listener calls it, answers the count `n`
//! milliseconds after the run, and where `restart` is true rebuilds the
//! endpoint and the listener of the connection once per `id` as the run
//! starts, so a request in flight meets `RPC_UNKNOWN` as it meets a restarted
//! server, and a run the rebuild finds waiting never starts. Where `relink`
//! is true it takes the link of the connection down and up again once per
//! `id`, on the same endpoint and the same listener, as a carrier whose link
//! drops and returns does. With `{"answer": {status,
//! substatus, type, body, body_len, seed, delay_ms}}` it answers the status,
//! the substatus, the media type, the empty string for none, and the body, the
//! text of `body` or `body_len` bytes drawn from `seed`. With `{"stream":
//! {events, interval_ms, fail_after, tag}}` it answers an event stream as Yoga
//! writes one: a comment, `events` events `next` with
//! `{"data":{"tick":n,"tag":tag}}`, or `{"data":{"tick":n}}` with no tag, one
//! every `interval_ms`, endless for -1, and an event `complete`, or a failure
//! after `fail_after` events. A request with no such member is answered by its
//! query: `subscription{tick}` with a stream of two results, `mutation{pause}`
//! with `{"data":{"pause":true}}` and every other query with
//! `{"data":{"ok":true}}`, the calls of `docs/graphtp.md` section 6.2. The
//! clock is the wall clock of this process, and a turn that finds nothing to do
//! sleeps until the next deadline of an engine, at most `idle_us`.

use std::{
    cell::{
        Cell,
        RefCell, //
    },
    future::Future,
    io,
    os::unix::net::UnixStream,
    path::PathBuf,
    pin::Pin,
    rc::Rc,
    task::{
        Context,
        Poll,
        Waker, //
    },
    thread,
    time::{
        Duration,
        Instant, //
    }, //
};

use aosabi_core::{
    cache_budget,
    graphtp::{
        Answer, Body, Broken, Chunks, Fetch, Listener, Reply, Request,
        limits::{
            self,
            CardLimits, //
        },
    }, //
};
use aosabi_json::{
    JsObject,
    JsString,
    JsValue, //
};
use graphtp::zlib;
use homa::{
    Carrier, Config, Endpoint, Limits, Owned, Priority, Profile,
    stream::{
        self,
        Decoder,
        Framed,
        Mode, //
    },
    unix::{
        SeqListener,
        SeqPacket,
        StreamListener, //
    },
    wire::Compact,
};

use super::cache::budget_json;
use crate::{
    homa_packet::{
        Fields,
        count, //
    },
    homa_sim::{
        config_error_name,
        config_of,
        request_profile,
        scenario::{
            link_error_name,
            stats_json, //
        }, //
    },
    json::reader,
};

/// The environment variable that names the path of the stream socket.
const STREAM_ENV: &str = "AOSABI_UNIX_STREAM";

/// The card a request that names none serves.
const DEFAULT_CARD: &str = "test-graphtp";

/// The media type of every answer of the service but a stream and one its
/// request names, the answer type of `GraphTP` as Yoga writes it.
const ANSWER_TYPE: &str = "application/graphql-response+json; charset=utf-8";

/// The media type of a stream of the service.
const EVENT_STREAM: &str = "text/event-stream";

/// The bytes of the receive buffer of every loop, a packet of the largest
/// profile and more.
const RX_LEN: usize = 1 << 20;

/// The configuration of every endpoint a request that names none takes:
/// sixteen server slots and 256 pages of 8,192 bytes, sixteen pages a slot,
/// the most a slot may take.
const SERVE_CONFIG: Config = Config {
    peers: 1,
    client_slots: 0,
    server_slots: 16,
    page_len: 8_192,
    pages: 256,
    max_gaps: 4,
    size_buckets: 0,
    first_id: 0x2000_0002,
    hash_key: [0x5a; 16],
    port: 0,
    extra_ports: 0,
};

/// The first identifier of an endpoint the service rebuilt, whose identifiers
/// differ from those of the endpoint before it.
const RESTART_FIRST_ID: u64 = 0x2100_0002;

/// Answers the operation `card`, with the bytes the encoder's scratch of the
/// card holds once made beside the bounds.
pub(super) fn op_card(request: &JsObject) -> Result<JsObject, JsString> {
    let fields = Fields::new(request, "card");
    let limits = card_of(&fields)?;
    let made = zlib::Scratch::new(limits.coding).map_err(|error| {
        JsString::from(format!("card: the scratch of {}: {error}", limits.card))
    })?;
    let mut payload = limits_json(limits);
    payload.insert("scratch_made", count(made.bytes()));
    let own = cache_budget::CARDS
        .iter()
        .find(|(card, _)| *card == limits.card)
        .map_or(JsValue::Null, |(_, budget)| {
            JsValue::from(budget_json(budget))
        });
    payload.insert("cache_budget", own);
    let mut every = JsObject::with_capacity(cache_budget::CARDS.len());
    for (card, budget) in &cache_budget::CARDS {
        every.insert(*card, budget_json(budget));
    }
    payload.insert("cache_budgets", every);
    Ok(payload)
}

/// Returns the bounds of the card the request names under `card`.
fn card_of(fields: &Fields<'_>) -> Result<&'static CardLimits, JsString> {
    let name = fields.opt_str("card")?.unwrap_or(DEFAULT_CARD);
    limits::card(name).ok_or_else(|| fields.refuse("card", &format!("names no card: {name}")))
}

/// Returns the bounds of a card as an object.
fn limits_json(limits: &CardLimits) -> JsObject {
    let mut coding = JsObject::with_capacity(5);
    coding.insert("window_bits", u64::from(limits.coding.window_bits));
    coding.insert("heads", count(limits.coding.heads));
    coding.insert("tokens", count(limits.coding.tokens));
    coding.insert("buffer", count(limits.coding.buffer));
    coding.insert("scratch", count(limits.coding.bytes()));
    let mut object = JsObject::with_capacity(8);
    object.insert("card", limits.card);
    object.insert("max_credential", u64::from(limits.max_credential));
    object.insert("request_buffer", count(limits.request_buffer));
    object.insert("answer_buffer", count(limits.answer_buffer));
    object.insert("streams_per_peer", u64::from(limits.streams_per_peer));
    object.insert("queue", count(limits.queue));
    object.insert("peers", count(limits.peers));
    object.insert("coding", coding);
    object
}

/// What the service counted, shared by every listener of the card.
#[derive(Debug, Default)]
struct Tally {
    /// The runs of each identifier, in the order of the first run of each.
    runs: RefCell<Vec<(String, u64)>>,
    /// The identifiers whose run rebuilt a connection already.
    restarted: RefCell<Vec<String>>,
    /// Whether a run asks the connection whose turn called it to be rebuilt.
    restart: Cell<bool>,
    /// The identifiers whose run took a link down and up already.
    relinked: RefCell<Vec<String>>,
    /// Whether a run asks the connection whose turn called it to take its
    /// link down and up again.
    relink: Cell<bool>,
    /// The calls of the fetch.
    calls: Cell<u64>,
    /// The streams opened by the service.
    streams: Cell<u64>,
    /// The stream sources dropped before their end, by a CANCEL, a link down
    /// or a rebuild.
    returned: Cell<u64>,
    /// The stream sources that ran to their end.
    ended: Cell<u64>,
}

impl Tally {
    /// Counts one run of `id` and returns the count after it.
    fn run(&self, id: &str) -> u64 {
        let mut runs = self.runs.borrow_mut();
        if let Some((_, runs)) = runs.iter_mut().find(|(known, _)| known == id) {
            *runs += 1;
            return *runs;
        }
        runs.push((String::from(id), 1));
        1
    }

    /// Asks for a rebuild on the first run of `id` alone.
    fn restart_once(&self, id: &str) {
        once(&self.restarted, &self.restart, id);
    }

    /// Asks for a link down and up on the first run of `id` alone.
    fn relink_once(&self, id: &str) {
        once(&self.relinked, &self.relink, id);
    }

    /// Returns the counts as an object.
    fn json(&self) -> JsObject {
        let mut runs = JsObject::new();
        for (id, count) in self.runs.borrow().iter() {
            runs.insert(id.as_str(), *count);
        }
        let mut object = JsObject::with_capacity(7);
        object.insert("calls", self.calls.get());
        object.insert("runs", runs);
        object.insert("streams", self.streams.get());
        object.insert("returned", self.returned.get());
        object.insert("ended", self.ended.get());
        object.insert("restarted", ids_json(&self.restarted));
        object.insert("relinked", ids_json(&self.relinked));
        object
    }
}

/// Sets `flag` on the first call for `id` alone, which `done` remembers.
fn once(done: &RefCell<Vec<String>>, flag: &Cell<bool>, id: &str) {
    let mut done = done.borrow_mut();
    if done.iter().any(|known| known == id) {
        return;
    }
    done.push(String::from(id));
    flag.set(true);
}

/// Returns the identifiers `ids` as an array.
fn ids_json(ids: &RefCell<Vec<String>>) -> Vec<JsValue> {
    ids.borrow()
        .iter()
        .map(|id| JsValue::from(id.as_str()))
        .collect()
}

/// Returns the fetch of the service of the harness over `tally`.
fn service(tally: &Rc<Tally>) -> Fetch {
    let tally = Rc::clone(tally);
    Rc::new(move |request: Request| -> Reply { answer(&tally, &request) })
}

/// Answers one request of the service.
fn answer(tally: &Rc<Tally>, request: &Request) -> Reply {
    tally.calls.set(tally.calls.get() + 1);
    let harness = match reader::parse(&request.content) {
        Ok(JsValue::Object(content)) => harness_of(&content),
        _ => {
            return ready(full(
                400,
                6,
                Some(ANSWER_TYPE),
                br#"{"errors":[{"message":"The content is no JSON object."}]}"#.to_vec(),
            ));
        }
    };
    let Some(harness) = harness else {
        return named(tally, query_of(&request.content).as_deref());
    };
    let fields = Fields::new(&harness, "harness");
    match spec_of(&fields, tally) {
        Ok(reply) => reply,
        Err(message) => {
            let mut error = JsObject::with_capacity(1);
            error.insert("message", message);
            let mut body = JsObject::with_capacity(1);
            body.insert("errors", vec![JsValue::Object(error)]);
            ready(full(
                400,
                6,
                Some(ANSWER_TYPE),
                json_bytes(&JsValue::Object(body)),
            ))
        }
    }
}

/// Returns the member `query` of the content, if it holds a string.
fn query_of(content: &[u8]) -> Option<String> {
    let Ok(JsValue::Object(content)) = reader::parse(content) else {
        return None;
    };
    match content.get("query") {
        Some(JsValue::String(query)) => query.as_str().map(String::from),
        _ => None,
    }
}

/// Answers a request with no member `extensions.harness` by its query: the
/// two calls of `docs/graphtp.md` section 6.2, `subscription{tick}` with a
/// stream of two results 100 milliseconds apart and `mutation{pause}` with
/// `{"data":{"pause":true}}`, and every other query with
/// `{"data":{"ok":true}}`.
fn named(tally: &Rc<Tally>, query: Option<&str>) -> Reply {
    match query {
        Some("subscription{tick}") => {
            let ticks = Ticks::new(tally, None, Some(2), Duration::from_millis(100), None);
            ready(ticks.answer())
        }
        Some("mutation{pause}") => ready(full(
            200,
            0,
            Some(ANSWER_TYPE),
            br#"{"data":{"pause":true}}"#.to_vec(),
        )),
        _ => ready(full(
            200,
            0,
            Some(ANSWER_TYPE),
            br#"{"data":{"ok":true}}"#.to_vec(),
        )),
    }
}

/// Returns the member `extensions.harness` of the content, if it holds one.
fn harness_of(content: &JsObject) -> Option<JsObject> {
    let Some(JsValue::Object(extensions)) = content.get("extensions") else {
        return None;
    };
    match extensions.get("harness") {
        Some(JsValue::Object(harness)) => Some(harness.clone()),
        _ => None,
    }
}

/// Returns the reply the member `extensions.harness` asks for.
fn spec_of(fields: &Fields<'_>, tally: &Rc<Tally>) -> Result<Reply, JsString> {
    if let Some(id) = fields.opt_str("run")? {
        return Ok(Box::pin(Run {
            tally: Rc::clone(tally),
            id: String::from(id),
            restart: fields.bool_or("restart", false)?,
            relink: fields.bool_or("relink", false)?,
            start_at: Instant::now() + Duration::from_millis(fields.int_or("wait_ms", 0u64)?),
            delay_ms: fields.int_or("delay_ms", 0u64)?,
            answer: None,
        }));
    }
    if fields.opt_object("answer")?.is_some() {
        let spec = fields.nested("answer")?;
        let body = match spec.opt_str("body")? {
            Some(text) => text.as_bytes().to_vec(),
            None => drawn(spec.int_or("body_len", 0usize)?, spec.int_or("seed", 1u64)?),
        };
        let kind = match spec.opt_str("type")? {
            Some("") => None,
            Some(kind) => Some(kind),
            None => Some(ANSWER_TYPE),
        };
        let answer = full(
            spec.int_or("status", 200u16)?,
            spec.int_or("substatus", 0u8)?,
            kind,
            body,
        );
        return Ok(later(answer, spec.int_or("delay_ms", 0u64)?));
    }
    if fields.opt_object("stream")?.is_some() {
        let spec = fields.nested("stream")?;
        let events = match spec.get("events") {
            Some(JsValue::Number(value)) if *value < 0.0 => None,
            _ => Some(spec.int_or("events", 3u64)?),
        };
        let ticks = Ticks::new(
            tally,
            spec.opt_str("tag")?.map(String::from),
            events,
            Duration::from_millis(spec.int_or("interval_ms", 0u64)?),
            spec.opt_u64("fail_after")?,
        );
        return Ok(ready(ticks.answer()));
    }
    Err(fields.refuse("harness", "names none of run, answer and stream"))
}

/// Returns an answer with a body written whole.
fn full(status: u16, substatus: u8, kind: Option<&str>, body: Vec<u8>) -> Answer {
    Answer {
        status,
        substatus,
        content_type: kind.map(|kind| kind.as_bytes().to_vec()),
        body: Body::Full(body),
    }
}

/// Returns the bytes of a JSON value.
fn json_bytes(value: &JsValue) -> Vec<u8> {
    let mut bytes = Vec::new();
    aosabi_json::write(&mut bytes, value);
    bytes
}

/// Returns `len` bytes drawn from `seed` by xorshift64*, which
/// `tests/harness/homa/graphtp.py` draws alike.
fn drawn(len: usize, seed: u64) -> Vec<u8> {
    let mut state = seed.max(1);
    (0..len)
        .map(|_| {
            state ^= state >> 12;
            state ^= state << 25;
            state ^= state >> 27;
            let [top, ..] = state.wrapping_mul(0x2545_F491_4F6C_DD1D).to_be_bytes();
            top
        })
        .collect()
}

/// Returns a reply that is ready at once.
fn ready(answer: Answer) -> Reply {
    later(answer, 0)
}

/// Returns a reply that is ready after `delay_ms` milliseconds.
fn later(answer: Answer, delay_ms: u64) -> Reply {
    Box::pin(Later {
        at: Instant::now() + Duration::from_millis(delay_ms),
        answer: Some(answer),
    })
}

/// An answer the service gives at a time.
struct Later {
    /// When the answer is ready.
    at: Instant,
    /// The answer, until it is taken.
    answer: Option<Answer>,
}

impl Future for Later {
    type Output = Answer;

    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Answer> {
        if Instant::now() < self.at {
            return Poll::Pending;
        }
        match self.answer.take() {
            Some(answer) => Poll::Ready(answer),
            None => Poll::Pending,
        }
    }
}

/// A run of the service, which starts after its wait and answers after its
/// delay.
struct Run {
    /// The counts of the service.
    tally: Rc<Tally>,
    /// The identifier whose runs the service counts.
    id: String,
    /// Whether the run asks for a rebuild of its connection.
    restart: bool,
    /// Whether the run asks for a link down and up of its connection.
    relink: bool,
    /// When the run starts.
    start_at: Instant,
    /// The milliseconds from the start of the run to its answer.
    delay_ms: u64,
    /// The answer, once the run has started.
    answer: Option<Later>,
}

impl Future for Run {
    type Output = Answer;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Answer> {
        if self.answer.is_none() {
            if Instant::now() < self.start_at {
                return Poll::Pending;
            }
            let runs = self.tally.run(&self.id);
            if self.restart {
                self.tally.restart_once(&self.id);
            }
            if self.relink {
                self.tally.relink_once(&self.id);
            }
            let mut result = JsObject::with_capacity(2);
            result.insert("run", self.id.as_str());
            result.insert("count", runs);
            let mut data = JsObject::with_capacity(1);
            data.insert("data", result);
            let body = json_bytes(&JsValue::Object(data));
            self.answer = Some(Later {
                at: Instant::now() + Duration::from_millis(self.delay_ms),
                answer: Some(full(200, 0, Some(ANSWER_TYPE), body)),
            });
        }
        match self.answer.as_mut() {
            Some(later) => Pin::new(later).poll(cx),
            None => Poll::Pending,
        }
    }
}

/// The source of a stream of the service, an event stream as Yoga writes it.
struct Ticks {
    /// The counts of the service.
    tally: Rc<Tally>,
    /// The tag each result carries, none for `None`.
    tag: Option<String>,
    /// The events before `complete`, `None` for endless.
    events: Option<u64>,
    /// The time between two events.
    interval: Duration,
    /// The events after which the source fails.
    fail_after: Option<u64>,
    /// When the next event is due.
    next_at: Instant,
    /// The events written.
    sent: u64,
    /// Whether the opening comment is written.
    opened: bool,
    /// Whether the source ran to its end.
    finished: bool,
}

impl Ticks {
    /// Returns the source of a stream of `events` results, endless for
    /// `None`, one every `interval`, failing after `fail_after` results, each
    /// with the tag `tag`, and counts it as opened.
    fn new(
        tally: &Rc<Tally>,
        tag: Option<String>,
        events: Option<u64>,
        interval: Duration,
        fail_after: Option<u64>,
    ) -> Self {
        tally.streams.set(tally.streams.get() + 1);
        Self {
            tally: Rc::clone(tally),
            tag,
            events,
            interval,
            fail_after,
            next_at: Instant::now(),
            sent: 0,
            opened: false,
            finished: false,
        }
    }

    /// Returns the answer of the stream, an event stream over this source.
    fn answer(self) -> Answer {
        Answer {
            status: 200,
            substatus: 0,
            content_type: Some(EVENT_STREAM.as_bytes().to_vec()),
            body: Body::Stream(Box::new(self)),
        }
    }
}

impl Chunks for Ticks {
    fn poll_chunk(&mut self, _cx: &mut Context<'_>) -> Poll<Option<Result<Vec<u8>, Broken>>> {
        if self.finished {
            return Poll::Ready(None);
        }
        if !self.opened {
            self.opened = true;
            return Poll::Ready(Some(Ok(b":\n\n".to_vec())));
        }
        if self.fail_after == Some(self.sent) {
            self.finished = true;
            self.tally.ended.set(self.tally.ended.get() + 1);
            return Poll::Ready(Some(Err(Broken)));
        }
        if self.events == Some(self.sent) {
            self.finished = true;
            self.tally.ended.set(self.tally.ended.get() + 1);
            return Poll::Ready(Some(Ok(b"event: complete\ndata:\n\n".to_vec())));
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
        if !self.finished {
            self.tally.returned.set(self.tally.returned.get() + 1);
        }
    }
}

/// The link of one connection.
enum Link<'b> {
    /// A `SOCK_SEQPACKET` socket, one Homa packet a record.
    Seq(SeqPacket),
    /// A stream socket in prefixed `AoFrame`.
    Stream(Framed<'b, UnixStream>),
}

impl Link<'_> {
    /// Returns the name of the link's form.
    const fn form(&self) -> &'static str {
        match self {
            Self::Seq(_) => "seqpacket",
            Self::Stream(_) => "stream",
        }
    }

    /// Returns whether the peer closed its end.
    fn closed(&self) -> bool {
        match self {
            Self::Seq(link) => link.closed(),
            Self::Stream(link) => link.closed(),
        }
    }
}

impl Carrier for Link<'_> {
    type Peer = ();
    type Error = io::Error;

    fn ready(&self, peer: (), len: usize) -> bool {
        match self {
            Self::Seq(link) => link.ready(peer, len),
            Self::Stream(link) => link.ready(peer, len),
        }
    }

    fn send(
        &mut self,
        peer: (),
        priority: Priority,
        header: &[u8],
        payload: &[u8],
    ) -> io::Result<()> {
        match self {
            Self::Seq(link) => link.send(peer, priority, header, payload),
            Self::Stream(link) => link.send(peer, priority, header, payload),
        }
    }

    fn recv(&mut self, buf: &mut [u8]) -> io::Result<Option<((), usize)>> {
        match self {
            Self::Seq(link) => link.recv(buf),
            Self::Stream(link) => link.recv(buf),
        }
    }
}

/// The buffers of the framing of one stream socket.
struct FrameBuffers {
    /// The receive buffer.
    rx: Vec<u8>,
    /// The transmit buffer.
    tx: Vec<u8>,
}

impl FrameBuffers {
    /// Returns the buffers of frames of at most `largest` bytes each.
    fn new(largest: usize) -> Self {
        Self {
            rx: vec![0; Decoder::buffer_len(Mode::Prefixed, largest)],
            tx: vec![0; stream::frame_len(Mode::Prefixed, largest)],
        }
    }
}

/// An endpoint of one connection: one peer, storage it owns, the compact
/// form.
type Engine = Endpoint<(), Owned<()>, Compact>;

/// What `serve` reads from its request.
struct ServeSpec {
    /// The path of the `SOCK_SEQPACKET` socket.
    path: PathBuf,
    /// The path of the stream socket, from the environment.
    stream_path: Option<PathBuf>,
    /// The bounds of the card.
    card: &'static CardLimits,
    /// The profile of every endpoint.
    profile: Profile,
    /// The configuration of every endpoint.
    config: Config,
    /// What the server announces.
    local: Limits,
    /// What it takes of a client until its LIMITS arrive.
    assumed: Limits,
    /// The connections it accepts.
    connections: usize,
    /// The longest the operation runs.
    max_time: Duration,
    /// The longest a turn with nothing to do sleeps.
    idle_us: u64,
}

impl ServeSpec {
    /// Reads the fields of `serve`.
    fn read(fields: &Fields<'_>) -> Result<Self, JsString> {
        let profile = request_profile(fields)?;
        let defaults = Limits {
            max_packet: profile.max_packet,
            max_message: 65_536,
            unsched_bytes: profile.unsched_bytes,
            max_rpcs: 16,
        };
        let local = limits_of(fields, "limits", defaults)?;
        let config = match fields.opt_object("config")? {
            Some(_) => config_of(Some(&fields.nested("config")?), SERVE_CONFIG)?,
            None => SERVE_CONFIG,
        };
        Ok(Self {
            path: PathBuf::from(fields.str("path")?),
            stream_path: std::env::var_os(STREAM_ENV).map(PathBuf::from),
            card: card_of(fields)?,
            profile,
            config,
            local,
            assumed: limits_of(fields, "assumed", local)?,
            connections: fields.int_or("connections", 1)?,
            max_time: Duration::from_millis(fields.int_or("max_time_ms", 60_000)?),
            idle_us: fields.int_or("idle_us", 200)?,
        })
    }
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

/// Builds an engine of `config` with its link up.
fn engine(spec: &ServeSpec, config: Config) -> Result<Engine, String> {
    let mut built =
        Engine::new(config, spec.profile, Owned::new(&config)).map_err(config_error_name)?;
    built
        .link_up((), spec.local, spec.assumed)
        .map_err(|error| format!("link:{}", link_error_name(error)))?;
    Ok(built)
}

/// One connection of `serve`.
struct Conn<'b> {
    /// Its link.
    link: Link<'b>,
    /// Its endpoint.
    engine: Engine,
    /// Its listener.
    listener: Listener<()>,
    /// The answers its listeners handed the engine.
    answered: u64,
    /// The rebuilds of its endpoint and listener.
    restarts: u64,
    /// The links it took down and up on the same endpoint and listener.
    relinks: u64,
    /// The error that ended its link, if one did.
    error: Option<String>,
    /// Whether it has ended.
    finished: bool,
    /// The milliseconds from the start of `serve` to its end.
    ended_ms: Option<u64>,
}

impl<'b> Conn<'b> {
    /// Returns a connection over `link` served by a listener of `fetch`.
    fn new(link: Link<'b>, spec: &ServeSpec, fetch: &Fetch) -> Result<Self, String> {
        Ok(Self {
            link,
            engine: engine(spec, spec.config)?,
            listener: listener(fetch, spec.card)?,
            answered: 0,
            restarts: 0,
            relinks: 0,
            error: None,
            finished: false,
            ended_ms: None,
        })
    }

    /// Runs one turn at `now` and returns whether anything moved.
    fn turn(
        &mut self,
        now: u64,
        rx: &mut [u8],
        cx: &mut Context<'_>,
        spec: &ServeSpec,
        fetch: &Fetch,
        tally: &Tally,
    ) -> bool {
        if self.finished {
            return false;
        }
        if let Err(error) = self.engine.pump(&mut self.link, now, rx) {
            self.error = Some(error.to_string());
        }
        let progress = self.listener.turn_endpoint(&mut self.engine, cx);
        self.answered += u64::from(progress.answered);
        if let Err(error) = self.engine.pump(&mut self.link, now, rx) {
            self.error = Some(error.to_string());
        }
        if tally.restart.replace(false) {
            self.restart(spec, fetch);
        }
        if tally.relink.replace(false) {
            self.relink(spec, cx);
        }
        if self.link.closed() || self.error.is_some() {
            self.end(cx);
        }
        progress.answered > 0
    }

    /// Rebuilds the endpoint and the listener over the open link, as a
    /// server that restarted holds no RPC and no stream.
    fn restart(&mut self, spec: &ServeSpec, fetch: &Fetch) {
        let mut config = spec.config;
        config.first_id = RESTART_FIRST_ID.wrapping_add(self.restarts.wrapping_mul(0x0010_0000));
        let rebuilt =
            engine(spec, config).and_then(|engine| Ok((engine, listener(fetch, spec.card)?)));
        match rebuilt {
            Ok((engine, listener)) => {
                self.engine = engine;
                self.listener = listener;
                self.restarts += 1;
            }
            Err(error) => self.error = Some(error),
        }
    }

    /// Takes the link down and up again on the same endpoint and listener,
    /// so the listener ends the peer's streams and forgets its count of
    /// `seq` while the socket stays open.
    fn relink(&mut self, spec: &ServeSpec, cx: &mut Context<'_>) {
        // A peer whose link is down already answers NoPeer, which needs
        // nothing.
        let _ = self.engine.link_down(());
        let progress = self.listener.turn_endpoint(&mut self.engine, cx);
        self.answered += u64::from(progress.answered);
        match self.engine.link_up((), spec.local, spec.assumed) {
            Ok(()) => self.relinks += 1,
            Err(error) => self.error = Some(format!("link:{}", link_error_name(error))),
        }
    }

    /// Ends the connection, taking its link down in its engine, so the
    /// listener ends the peer's streams.
    fn end(&mut self, cx: &mut Context<'_>) {
        self.finished = true;
        // A peer whose link is down already answers NoPeer, which needs
        // nothing.
        let _ = self.engine.link_down(());
        let progress = self.listener.turn_endpoint(&mut self.engine, cx);
        self.answered += u64::from(progress.answered);
    }

    /// Returns what the connection counted as an object.
    fn json(&self) -> JsObject {
        let mut object = JsObject::new();
        object.insert("form", self.link.form());
        object.insert("answered", self.answered);
        object.insert("restarts", self.restarts);
        object.insert("relinks", self.relinks);
        object.insert(
            "ended_ms",
            self.ended_ms.map_or(JsValue::Null, JsValue::from),
        );
        object.insert(
            "error",
            self.error.clone().map_or(JsValue::Null, JsValue::from),
        );
        object.insert("stats", stats_json(&self.engine.stats()));
        object
    }
}

/// Returns a listener of `fetch` with the bounds of `card`.
fn listener(fetch: &Fetch, card: &'static CardLimits) -> Result<Listener<()>, String> {
    Listener::new(Rc::clone(fetch), card).map_err(|error| format!("listener: {error}"))
}

/// The two listening sockets of the card.
struct Sockets {
    /// The `SOCK_SEQPACKET` listener.
    seq: SeqListener,
    /// The stream listener, where the environment names one.
    stream: Option<StreamListener>,
}

/// A socket the card accepted, before its link is built.
enum Accepted {
    /// A `SOCK_SEQPACKET` socket.
    Seq(SeqPacket),
    /// A stream socket.
    Stream(UnixStream),
}

impl Sockets {
    /// Accepts one socket from either listener, or `None` where none waits.
    fn accept(&self, max_packet: u16) -> io::Result<Option<Accepted>> {
        if let Some(link) = self.seq.accept(max_packet)? {
            return Ok(Some(Accepted::Seq(link)));
        }
        match &self.stream {
            Some(listener) => Ok(listener.accept()?.map(Accepted::Stream)),
            None => Ok(None),
        }
    }
}

/// Returns the nanoseconds since `start`.
fn since(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

/// Answers the operation `serve`.
pub(super) fn op_serve(request: &JsObject) -> Result<JsObject, JsString> {
    let fields = Fields::new(request, "serve");
    let spec = ServeSpec::read(&fields)?;
    let refused = |what: &str, error: &io::Error| JsString::from(format!("serve: {what}: {error}"));
    let sockets = Sockets {
        seq: SeqListener::bind(&spec.path).map_err(|error| refused("bind", &error))?,
        stream: match &spec.stream_path {
            Some(path) => {
                Some(StreamListener::bind(path).map_err(|error| refused("bind stream", &error))?)
            }
            None => None,
        },
    };
    let tally = Rc::new(Tally::default());
    let fetch = service(&tally);
    let mut buffers: Vec<FrameBuffers> = (0..spec.connections)
        .map(|_| FrameBuffers::new(usize::from(spec.profile.max_packet)))
        .collect();
    let mut free = buffers.iter_mut();
    let mut conns: Vec<Conn<'_>> = Vec::new();
    let mut rx = vec![0u8; RX_LEN];
    let mut cx = Context::from_waker(Waker::noop());
    let start = Instant::now();
    let stopped = loop {
        let now = since(start);
        let mut moved = false;
        if conns.len() < spec.connections {
            match sockets.accept(spec.profile.max_packet) {
                Ok(Some(Accepted::Seq(link))) => {
                    conns.push(Conn::new(Link::Seq(link), &spec, &fetch).map_err(JsString::from)?);
                    moved = true;
                }
                Ok(Some(Accepted::Stream(link))) => {
                    let Some(pair) = free.next() else {
                        break "buffers";
                    };
                    let framed = Framed::new(link, Mode::Prefixed, &mut pair.rx, &mut pair.tx);
                    conns.push(
                        Conn::new(Link::Stream(framed), &spec, &fetch).map_err(JsString::from)?,
                    );
                    moved = true;
                }
                Ok(None) => {}
                Err(error) => return Err(refused("accept", &error)),
            }
        }
        for conn in &mut conns {
            let was = conn.finished;
            moved |= conn.turn(now, &mut rx, &mut cx, &spec, &fetch, &tally);
            if conn.finished && !was {
                conn.ended_ms = Some(since(start) / 1_000_000);
            }
        }
        if conns.len() == spec.connections && conns.iter().all(|conn| conn.finished) {
            break "done";
        }
        if start.elapsed() > spec.max_time {
            break "max_time";
        }
        if !moved {
            let due = conns
                .iter()
                .filter(|conn| !conn.finished)
                .filter_map(|conn| conn.engine.next_timeout())
                .min();
            let cap = spec.idle_us.saturating_mul(1_000);
            let wait = due.map_or(cap, |due| due.saturating_sub(since(start)).min(cap));
            thread::sleep(Duration::from_nanos(wait));
        }
    };
    drop(sockets);
    // The socket files are the operation's own, and a file gone already needs
    // nothing.
    let _ = std::fs::remove_file(&spec.path);
    if let Some(path) = &spec.stream_path {
        let _ = std::fs::remove_file(path);
    }
    let mut payload = JsObject::new();
    payload.insert("stopped", stopped);
    payload.insert("card", limits_json(spec.card));
    payload.insert("stream_socket", spec.stream_path.is_some());
    payload.insert("accepted", count(conns.len()));
    payload.insert("time_ms", since(start) / 1_000_000);
    payload.insert(
        "connections",
        conns
            .iter()
            .map(|conn| JsValue::Object(conn.json()))
            .collect::<Vec<_>>(),
    );
    payload.insert("service", tally.json());
    Ok(payload)
}
