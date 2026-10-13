// SPDX-License-Identifier: MIT
/*
 * aosabi-js request: the pending table of the requests in flight and the two
 * job modes.
 *
 * No analogue in the reference.
 */

//! The requests in flight.
//!
//! The record of a request is its method, its URL, its header fields and
//! the form of its body, which the method decides: the byte 0 for a GET and a
//! HEAD, or the byte 1 followed by the handle of the reader of the body in the
//! table of the request for every other method, a POST with no body among
//! them, since the host keeps the body in its own stream. The export `fetch`
//! takes one byte after it, which names what answers the request, since the
//! hono of the host has routed it: the byte 0 for the server of the GraphQL
//! endpoint with no router, the request the handler of the glue hands in,
//! [`crate::endpoint`], and the byte 1 for the app of the engine, the routes
//! of the plugins of the engine and its not found handler.
//!
//! The host runs the routes and the middleware of an app in its own hono and
//! opens a request here before it hands it to the engine, the op `OPEN` of
//! the export `ctx`, [`open`]: the step of the request logger of the base
//! pipeline opens it with the logger of the request, a child of the root
//! with the request id, the method and the path of the request and the tag
//! `http`, as the base pipeline of a Rust app makes it, and a middleware of a
//! plugin of the engine opens it where the request reaches no such step. The
//! entry of an open request answers nothing, holds the context of its head,
//! which owns the bound loggers made while it is in place, and polls the
//! calls of the engine the app waits on, the claims of a provider of Zitadel
//! among them. The export `fetch` of an open request hands it to the server
//! of the endpoint or to the app of the engine, under the logger of
//! the request, and the entry goes back to the open state once the engine
//! has answered, so the host writes the access line through the logger of the
//! request after the answer, until the op `CLOSE` ends the request, [`close`]. The engine reads
//! that stream once, at the first read of the body, as the request parsers of
//! Yoga read it, chunk by chunk through the import `iter_next`, and keeps the
//! chunks apart until a quarter of the length its `Content-Length` names has
//! arrived, then moves them into one buffer of that length, which the entry of
//! the request keeps, so the body crosses once and lies once in the memory of
//! the module beside its first quarter for the moment of the move.
//! A body no code of the engine reads, the body a route of the app reads from
//! the stream of its host through `c.req`, never crosses. The record of an
//! answer is the coding mark, 0 for none and 2 for
//! `deflate`, with the bit 4 for the answer of a result processor of Yoga
//! that answers with a stream, the status, the header fields and the body. A
//! string or a body is a length of four little endian bytes and its bytes, a
//! count of fields is four little endian bytes, and a status is two.
//!
//! The pending table holds one entry per request in flight: the future of the
//! record of its answer, the agent its code first ran on with the context of
//! the request at that point, the promises of the host it waits on, the jobs
//! of its agents the host holds, its record and its body once the engine read
//! it, from which the names of the variables its client sent are read once a
//! call asks for them and in which the files of a multipart body stand, and
//! the reservations the host made for it, in their order. A poll runs until the
//! answer is ready or the request waits on the host, so the host and not a
//! loop of the module drives the wait, and the export `release` drops the
//! entry with all it holds. The table is a vector, searched by the number of
//! the request, since a host holds a few requests in flight at once.
//!
//! The boot record picks one of two job modes. The first import of a request
//! that runs on an agent adopts it: the agent tells the host of each job
//! through the import `host_enqueue`, and the host answers with one microtask
//! of its own that calls the export `run_job`, so each job of the agent runs as
//! one microtask in the same queue as the app's own, in the order the agent
//! enqueued them. The export grants the agent that one job and polls the
//! request, so the job runs in the drain of the loop of its agent inside that
//! poll, with the context of the request, the request that runs and the agent
//! in place, as a job the agent drains itself runs. A settlement of a promise
//! of the host runs the jobs it enqueues inside its own microtask, where V8
//! runs the reaction of the `then` of `wrapResolver` on the promise the settled one stands
//! for. A settlement a callback of the app makes inside an import rides on the
//! answer of that import and takes effect as the import returns, so in the job
//! of that call. Its reactions run at once, and the jobs they enqueue, the
//! reactions of the consumer of the engine, wait for the microtask the host
//! reserved at that call, where V8 queues the reactions of the promise of the
//! app, which calls the export `resume`. A settlement through the resolving
//! functions the host handed a thenable it adopts runs a level of reactions
//! more at once, inside an import or in the microtask of the export `settle`,
//! since the async function of a root reacts on such a settlement itself.
//! In the fast mode a settlement drives the request at once, and the jobs of
//! the agent run in the same turn.
//!
//! An answer whose body is a stream, the events of a subscription or the parts
//! of a multipart stream of a result processor of Yoga, or the body of a web
//! standard `Response` of a route the chain changed or of `c.body`, answers its
//! head alone, with the bit 64 of the mark and an empty body, and the entry
//! keeps the stream and the driver of its agent, if it has one. The host calls
//! the export `wake` once it holds the head, and every drive of the request
//! from then on polls the driver and reads the stream, and pushes each piece of
//! the body through the import `host_push` as the stream gives it, then its end
//! or its failure with the message, so the host writes each frame
//! as it comes. The release follows once the driver has
//! ended, after which the host drops the table of the request, since the agent
//! still runs code of the app until then. A host whose reader goes away calls
//! the export `release`, which drops the stream, so the stream runs its cancel
//! on its agent, as the writer of the Node adapter cancels the reader of the
//! body, and the release follows once the driver has ended. A drive of the body
//! polls again only where a waker of the poll was woken, so a body that waits
//! on a promise of the host, a job the host holds, a timer of the host or an
//! event of the pub sub of the engine leaves the drive, and the export that
//! brings what it waits for drives it again.

#![forbid(unsafe_code)]

use core::{
    cell::{
        Cell,
        RefCell, //
    },
    future::{
        Future,
        poll_fn, //
    },
    pin::Pin,
    sync::atomic::{
        AtomicBool,
        Ordering, //
    },
    task::{
        Context,
        Poll,
        Waker, //
    },
};
use std::{
    collections::VecDeque,
    ops::Range,
    rc::Rc,
    sync::Arc,
    task::Wake, //
};

use aosabi_core::context::{
    self as request_context,
    Context as RequestContext, //
};
use aosabi_core::{
    Thrown,
    app::request_id,
    logger::get_root_logger, //
};
use aosabi_execute::{
    promise::Agent,
    value::{
        Completion,
        Value, //
    },
};
use aosabi_http::{
    Body,
    BodyStream,
    Context as WebContext,
    Headers,
    LocalBoxFuture,
    Request,
    Response,
    fetch::body::BodyError,
    utils::url::get_path, //
};
use aosabi_json::JsValue;
use graphql_over_http::{
    RequestParserResult,
    request_parser::{
        get::{
            is_get_request,
            parse_get_request, //
        },
        post_form_url_encoded::{
            is_post_form_url_encoded_request,
            parse_post_form_url_encoded_request, //
        },
        post_graphql_string::{
            is_post_graphql_string_request,
            parse_post_graphql_string_request, //
        },
        post_json::{
            is_post_json_request,
            parse_post_json_request, //
        },
        post_multipart::{
            is_post_multipart_request,
            parse_post_multipart_request, //
        },
    }, //
};

use crate::{
    abi::import,
    host::{
        self,
        Reader, //
    },
    value::{
        self,
        JsPromise, //
    },
};

/// The request of the boot, whose table holds the roots.
pub const BOOT: u32 = 0;

/// The polls one turn of a request may take before it counts as stuck, a
/// bound no answer of a fixture reaches.
const POLLS: usize = 1_000_000;

/// The largest `Content-Length` the module sizes the buffer of a body from, a
/// quarter of the memory a module for `wasm32` addresses. A larger one, and a
/// body that names none, grows its buffer as its chunks come.
const LARGEST: usize = 1 << 30;

/// The share of the length a `Content-Length` names, one part in this many,
/// that must arrive before the module sizes the buffer of a body from that
/// length, so a header claims no memory before its body sends a quarter of
/// it, and an honest body takes about 1.25 times its length at its peak.
const SHARE: usize = 4;

/// The byte after the record of a request that hands it to the server of the
/// GraphQL endpoint with no router, [`crate::endpoint::answer`], the handler
/// of the glue on the host.
const TO_ENDPOINT: u8 = 0;

/// The byte after the record of a request that hands it to the app of the
/// engine, the routes of its plugins and its not found handler.
const TO_APP: u8 = 1;

/// The mark of an answer the host leaves alone.
const MARK_NONE: u8 = 0;

/// The bit of the mark of an answer of a result processor of Yoga that
/// answers with a stream, which the host hands on as a stream, so the host
/// frames it as it frames the stream of a served answer, chunked on workerd and
/// with its length on Bun, which reads a stream that has ended before it
/// writes the head.
const MARK_STREAM: u8 = 4;

/// The bit of the mark of an answer whose body is a string, which the host
/// hands its platform as a string, so a platform that writes the type of a
/// string body on the wire alone, as Bun does, writes it as it writes the
/// type of a served answer there.
const MARK_TEXT: u8 = 8;

/// The bit of the mark of the head of an answer whose body the module pushes
/// after it through the import `host_push`, whose record holds an empty body.
const MARK_PUSHED: u8 = 64;

/// The kind of a record of `host_push` that holds a piece of the body.
const PIECE: u8 = 0;

/// The kind of a record of `host_push` that ends the body.
const END: u8 = 1;

/// The kind of a record of `host_push` that fails the body with the message
/// it holds.
const FAILED: u8 = 2;

/// The kind of a record of `host_push` that releases the request, once the
/// module holds nothing of it.
const RELEASED: u8 = 3;

/// The media types of the result processors of Yoga that answer with a stream,
/// the events of `text/event-stream` and the parts of `multipart/mixed`, where
/// the processor of a regular result answers with a text.
const STREAM_MEDIA_TYPES: [&str; 2] = ["text/event-stream", "multipart/mixed"];

/// What the future of a request answers.
enum Answered {
    /// The record of a whole answer.
    Whole(Vec<u8>),
    /// The record of the head of an answer and the body the module pushes
    /// after it.
    Head(Vec<u8>, Pump),
}

/// The future of the record of an answer.
type Answer = Pin<Box<dyn Future<Output = Answered>>>;

/// The phase of a request in flight.
enum Phase {
    /// The request the host opened, which the engine answers once the host
    /// hands it to the app, or answered already while the host holds it.
    Open,
    /// The future of its answer.
    Head(Answer),
    /// The body the module pushes after the head of its answer.
    Body(Pump),
}

/// The body of an answer the module pushes to the host after its head.
struct Pump {
    /// The stream of the body, `None` once it ended or the host cancelled it.
    body: Option<BodyStream>,
    /// The driver of the stream, which runs its agent, `None` for a stream
    /// that hands out none and once it ended.
    driver: Option<LocalBoxFuture<'static, ()>>,
    /// The pieces the module read of the stream before the head, which go
    /// first.
    read: VecDeque<Vec<u8>>,
    /// The message of a failure of the stream before the head, which follows
    /// those pieces.
    failure: Option<String>,
}

impl Pump {
    /// Polls the driver and reads the stream once, pushes every piece the
    /// stream gives to the host for `request`, and answers whether the body
    /// and the driver have ended.
    ///
    /// The driver runs first, so the pieces its jobs and its timers write
    /// are read in the same poll. A body the host cancelled, `cancelled`, is
    /// dropped and pushes nothing more.
    fn poll(&mut self, request: u32, cancelled: bool, cx: &mut Context<'_>) -> bool {
        if cancelled {
            self.body = None;
            self.read.clear();
            self.failure = None;
        }
        while let Some(mut piece) = self.read.pop_front() {
            push(request, PIECE, &mut piece);
        }
        if let Some(message) = self.failure.take() {
            push(request, FAILED, &mut message.into_bytes());
        }
        if let Some(driver) = self.driver.as_mut()
            && driver.as_mut().poll(cx).is_ready()
        {
            self.driver = None;
        }
        while let Some(body) = &self.body {
            match body.poll_chunk(cx) {
                Poll::Ready(Some(Ok(mut piece))) => push(request, PIECE, &mut piece),
                Poll::Ready(Some(Err(error))) => {
                    push(request, FAILED, &mut error.to_string().into_bytes());
                    self.body = None;
                }
                Poll::Ready(None) => {
                    push(request, END, &mut []);
                    self.body = None;
                }
                Poll::Pending => break,
            }
        }
        self.body.is_none() && self.driver.is_none()
    }
}

/// The waker of a drive of a body, which notes that a future it polled
/// wants another poll.
// An `unsafe` waker over an `Rc` would skip the atomic count each clone takes.
#[derive(Default)]
struct Woken(AtomicBool);

impl Wake for Woken {
    fn wake(self: Arc<Self>) {
        self.0.store(true, Ordering::Relaxed);
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.0.store(true, Ordering::Relaxed);
    }
}

/// One request in flight.
struct Pending {
    /// The number the host gave the request.
    request: u32,
    /// The phase of the request, out of the table while a drive polls it.
    phase: Option<Phase>,
    /// Whether the head of its answer went to the host and the module pushes
    /// its body.
    pushed: bool,
    /// Whether the host stopped reading the body the module pushes.
    cancelled: bool,
    /// The agent the code of the request first ran on.
    agent: Option<Agent>,
    /// The context of the request in place at its first import, which a job
    /// that no poll of the request reaches runs with.
    context: Option<RequestContext>,
    /// The promises of the host the request waits on, by handle.
    waits: Vec<(u32, Rc<JsPromise>)>,
    /// The agents of the jobs the host was asked to run and has not run yet,
    /// in the order the agents enqueued them.
    jobs: VecDeque<Agent>,
    /// The record of the request, which the names of the variables its client
    /// sent are read from when a call first asks for them.
    record: Rc<[u8]>,
    /// The body of the request once the engine read it, the one copy of it in
    /// the memory of the module, in which the files of a multipart body stand.
    body: Option<Rc<[u8]>>,
    /// What the entry holds of the names of the variables the client sent.
    sent: Sent,
    /// The reservations of the host for the request, in the order the host
    /// made them, each with its number and the agent that keeps its jobs, or
    /// none for one that keeps no job.
    reservations: VecDeque<Option<(u32, Agent)>>,
    /// The calls of the engine the app waits on beside the answer, which
    /// every drive of the request polls after the answer, such as the claims
    /// of a provider of `aosabi/auth/zitadel`.
    side: Vec<LocalBoxFuture<'static, ()>>,
    /// The context of the head of a request the host opened, which owns the
    /// bound loggers made while it is in place and lives until the request
    /// ends, or none for a request the host handed the engine at once.
    opened: Option<WebContext>,
    /// The logger of the request the host opened it with, the two numbers of
    /// its handle, under which the app of the engine answers it.
    logger: Option<(u32, u32)>,
}

/// What a request holds of the names of the variables its client sent.
#[derive(Clone)]
enum Sent {
    /// No call has asked for them yet.
    Unread,
    /// The names, as the parameters of the request hold them.
    Names(Rc<[String]>),
    /// No parser read the parameters, so every variable counts as sent.
    Unknown,
}

// An `unsafe` cell would skip each borrow flag.
thread_local! {
    /// The requests in flight.
    static PENDING: RefCell<Vec<Pending>> = const { RefCell::new(Vec::new()) };
    /// The request whose code runs now, 0 for the boot.
    static CURRENT: Cell<u32> = const { Cell::new(BOOT) };
    /// Whether the boot picked the fast mode over the faithful one.
    static FAST: Cell<bool> = const { Cell::new(false) };
    /// The number of the last reservation of the host.
    static RESERVATIONS: Cell<u32> = const { Cell::new(0) };
}

/// Runs `body` on the entry of `request`, or answers `None` when the request
/// is not in flight.
///
/// The borrow of the table lasts for `body` alone, so `body` calls no import
/// and no code of the engine.
fn with_entry<R>(request: u32, body: impl FnOnce(&mut Pending) -> R) -> Option<R> {
    PENDING.with_borrow_mut(|pending| {
        pending
            .iter_mut()
            .find(|entry| entry.request == request)
            .map(body)
    })
}

/// Sets the job mode the boot record names, fast when `fast` is true.
pub fn set_fast(fast: bool) {
    FAST.set(fast);
}

/// Adopts the agent the code of `request` runs on, at the first import of
/// the request that runs on it.
///
/// The entry keeps the first agent of the request and the context of the
/// request in place at that import. In the faithful mode the agent hands its
/// queue to the host, once per agent, and a job it enqueued before is
/// announced to the host at the adoption.
pub fn adopt_agent(request: u32) {
    if request == BOOT {
        return;
    }
    let Some(agent) = Agent::current() else {
        return;
    };
    let first = with_entry(request, |entry| entry.agent.is_none()).unwrap_or(false);
    if first {
        let context = request_context::ambient_context();
        let _kept = with_entry(request, |entry| {
            entry.agent = Some(agent.clone());
            entry.context = context;
        });
    }
    if FAST.get() || agent.has_enqueue_notifier() {
        return;
    }
    agent.set_enqueue_notifier(Some(Rc::new(move |agent: &Agent| {
        announce(agent, request);
    })));
}

/// Notes one job of `agent` for the host and asks the host for its microtask.
fn announce(agent: &Agent, request: u32) {
    let _noted = with_entry(request, |entry| entry.jobs.push_back(agent.clone()));
    import::host_enqueue(request);
}

/// Returns whether the host holds a job of `request`.
fn holds_jobs(request: u32) -> bool {
    with_entry(request, |entry| !entry.jobs.is_empty()).unwrap_or(false)
}

/// Returns whether `request` waits on a promise of the host.
fn waits(request: u32) -> bool {
    with_entry(request, |entry| !entry.waits.is_empty()).unwrap_or(false)
}

/// Returns whether `request` is in flight.
fn in_flight(request: u32) -> bool {
    with_entry(request, |_| ()).is_some()
}

/// Runs the first job the host was asked to run for `request`, one
/// microtask of the host, and drives the request.
///
/// The agent of the job takes one grant and the request one poll, so the
/// drain of the loop of the agent runs the job inside the poll, with the
/// ambient state of the request in place. A poll that does not reach that
/// drain, since the request waits on another agent, leaves the grant, and the
/// job runs through `run_one_job` with the context of the request in place.
pub fn run_job(request: u32) {
    let next = with_entry(request, |entry| entry.jobs.pop_front()).flatten();
    let Some(agent) = next else {
        return;
    };
    run_granted(request, &agent, 1);
}

/// Grants `agent` the first `count` jobs of its queue and drives `request`,
/// so the drain of the loop of the agent runs them inside a poll, and runs
/// those no poll reached through `run_one_job` with the context of the
/// request in place.
fn run_granted(request: u32, agent: &Agent, count: usize) {
    agent.grant_jobs(count);
    answer_once_ready(request, drive(request, Some(agent)));
    let left = agent.take_grants();
    if left == 0 || !in_flight(request) {
        return;
    }
    outside_a_poll(request, || {
        for _ in 0..left {
            let _ran = agent.run_one_job();
        }
    });
    drive_again(request);
}

/// Runs `body` as code of `request` outside a poll of it, with the context
/// the request had at its first import in place.
fn outside_a_poll(request: u32, body: impl FnOnce()) {
    let context = with_entry(request, |entry| entry.context.clone()).flatten();
    with_current(request, || match context {
        Some(context) => request_context::run_with_context(context, body),
        None => body(),
    });
}

/// Returns the agent of `request`, the one its first import adopted, if any.
pub fn agent_of(request: u32) -> Option<Agent> {
    with_entry(request, |entry| entry.agent.clone()).flatten()
}

/// Returns the request whose code runs now, 0 for the boot.
pub fn current() -> u32 {
    CURRENT.get()
}

/// Runs `body` as the code of `request`.
pub fn with_current<R>(request: u32, body: impl FnOnce() -> R) -> R {
    let before = CURRENT.replace(request);
    let result = body();
    CURRENT.set(before);
    result
}

/// Starts `future` beside the answer of `request`, a call of the engine the
/// app waits on, which each drive of the request polls after the answer
/// until it ends, and which the request drops with itself.
///
/// A call the app makes inside a drive of the request, inside an import it
/// answers, is polled by that drive once the import returns, while one it
/// makes outside every drive, from a microtask of its own, drives the
/// request here, since no export would reach it before the app waits on it.
pub fn spawn(request: u32, future: LocalBoxFuture<'static, ()>) {
    let idle = with_entry(request, |entry| {
        entry.side.push(future);
        entry.phase.is_some()
    })
    .unwrap_or(false);
    if idle {
        drive_again(request);
    }
}

/// Polls each call of the engine `request` started beside its answer once,
/// as the code of the request, keeps those that have not ended, and answers
/// whether one ended.
fn poll_side(request: u32, cx: &mut Context<'_>) -> bool {
    let mut side =
        with_entry(request, |entry| core::mem::take(&mut entry.side)).unwrap_or_default();
    if side.is_empty() {
        return false;
    }
    let before = side.len();
    with_current(request, || {
        side.retain_mut(|future| future.as_mut().poll(cx).is_pending());
    });
    let ended = side.len() < before;
    let _kept = with_entry(request, |entry| {
        side.append(&mut entry.side);
        entry.side = side;
    });
    ended
}

/// Notes that `request` waits on the promise `handle` of the host.
pub fn wait_on(request: u32, handle: u32, promise: Rc<JsPromise>) {
    let _noted = with_entry(request, |entry| entry.waits.push((handle, promise)));
}

/// Answers the request record in the first `len` bytes of the input buffer
/// as `request`, and returns the address of the answer or 0 while it waits.
///
/// The last byte of the input names what answers the request,
/// [`TO_ENDPOINT`] or [`TO_APP`], and the bytes before it are the record of
/// the request, none for a request the host opened.
pub fn fetch(len: u32, request: u32) -> usize {
    let mut record = host::input(len);
    let to = record.pop();
    if with_entry(request, |entry| entry.opened.is_some()).unwrap_or(false) {
        aosabi_core::clock::note_turn();
        return fetch_opened(request, to);
    }
    let record: Rc<[u8]> = Rc::from(record);
    // A request reaches the host in a task of its loop of its own, after the
    // loop has turned, which the lazy sink of the logger reads.
    aosabi_core::clock::note_turn();
    #[cfg(feature = "execute")]
    let answer = match (crate::boot::app(), execute_request(&record, request)) {
        (Some(_), Some(parsed)) => Ok(Box::pin(async move {
            let body = parsed.bytes().await.unwrap_or_else(|_| Rc::from(&[][..]));
            Answered::Whole(failure(200, &crate::execute::answer(body).await))
        }) as Answer),
        _ => answer_of(&record, request, to),
    };
    #[cfg(not(feature = "execute"))]
    let answer = answer_of(&record, request, to);
    let answer = match answer {
        Ok(answer) => answer,
        Err(failed) => return host::output(&failed),
    };
    drop_request(request);
    PENDING.with_borrow_mut(|pending| {
        pending.push(Pending::new(request, record, Phase::Head(answer)));
    });
    drive(request, None).map_or(0, |answer| host::output(&answer))
}

/// Answers the request `request` the host opened, which it hands the engine
/// with no record of its own to what `to` names, and returns the address of
/// the answer or 0 while it waits.
fn fetch_opened(request: u32, to: Option<u8>) -> usize {
    let Some(record) = with_entry(request, |entry| {
        matches!(entry.phase, Some(Phase::Open)).then(|| Rc::clone(&entry.record))
    })
    .flatten() else {
        return host::output(&failure(500, b"the request is answered already"));
    };
    let answer = match answer_of(&record, request, to) {
        Ok(answer) => answer,
        Err(failed) => return host::output(&failed),
    };
    let _kept = with_entry(request, |entry| entry.phase = Some(Phase::Head(answer)));
    drive(request, None).map_or(0, |answer| host::output(&answer))
}

impl Pending {
    /// Returns the entry of `request` in the phase `phase` with its record
    /// `record`.
    fn new(request: u32, record: Rc<[u8]>, phase: Phase) -> Self {
        Self {
            request,
            phase: Some(phase),
            pushed: false,
            cancelled: false,
            agent: None,
            context: None,
            waits: Vec::new(),
            jobs: VecDeque::new(),
            record,
            body: None,
            sent: Sent::Unread,
            reservations: VecDeque::new(),
            side: Vec::new(),
            opened: None,
            logger: None,
        }
    }
}

/// Opens the request `request` with the record `record`, the op `OPEN` of the
/// export `ctx`: a byte 1 where the host asks for the logger of the request and
/// 0 otherwise, then the record of the request, which the export `fetch` of the
/// request takes in place of a record of its own. The context of its head is
/// noted for the request, so the calls of the engine on the request read it,
/// and the logger of the request, a child of the root with the request id, the
/// method and the path of the request and the tag `http`, is made while that
/// context is in place, so its bound loggers end with the request, as the
/// access logger of the base pipeline makes it. The answer is the two
/// numbers of the handle of the logger, made at the first open that asks for
/// it, or `undefined`.
///
/// # Errors
///
/// Answers the error of a broken record.
pub fn open(request: u32, record: &[u8]) -> Result<Value, Thrown> {
    let (&wants_logger, record) = record
        .split_first()
        .ok_or_else(|| Thrown::error(BROKEN_OPEN))?;
    if !in_flight(request) {
        let record: Rc<[u8]> = Rc::from(record);
        let parsed = request_of(&record, request).ok_or_else(|| Thrown::error(BROKEN_OPEN))?;
        let path = get_path(&parsed.url);
        let context = WebContext::new(
            Rc::new(parsed),
            path,
            Rc::from([]),
            crate::boot::env(),
            None,
        );
        crate::context::note_context(request, &context);
        let mut entry = Pending::new(request, record, Phase::Open);
        entry.opened = Some(context);
        PENDING.with_borrow_mut(|pending| pending.push(entry));
    }
    if wants_logger == 1 {
        let made = with_entry(request, |entry| {
            entry
                .logger
                .is_none()
                .then(|| entry.opened.clone())
                .flatten()
        })
        .flatten();
        if let Some(context) = made {
            let logger = request_context::run_with_context(context.clone(), || {
                get_root_logger()
                    .child(aosabi_json::json!({
                        requestId: request_id(&context),
                        method: context.req().method(),
                        path: context.req().path(),
                    }))
                    .with_tag("http")
            });
            let _kept = with_entry(request, |entry| entry.logger = Some(logger.to_raw()));
        }
    }
    Ok(with_entry(request, |entry| entry.logger).flatten().map_or(
        Value::Undefined,
        |(index, generation)| {
            Value::List(Rc::new(vec![
                Value::Number(f64::from(index)),
                Value::Number(f64::from(generation)),
            ]))
        },
    ))
}

/// Ends the request `request` the host opened, the op `CLOSE` of the export
/// `ctx`: drops a request in its open state, and leaves any other to the end
/// of what it runs, the answer the app of the engine makes or the body the
/// module pushes after the head of its answer, which drops it then.
pub fn close(request: u32) {
    let idle = with_entry(request, |entry| {
        entry.opened = None;
        matches!(entry.phase, Some(Phase::Open))
    });
    if idle == Some(true) {
        drop_request(request);
    }
}

/// Returns the logger of the request `request`, the two numbers of the
/// handle the host opened it with.
pub fn logger_of(request: u32) -> Option<(u32, u32)> {
    with_entry(request, |entry| entry.logger).flatten()
}

/// The message of a broken record of the op `OPEN`.
const BROKEN_OPEN: &str = "the host opened a request with a broken record";

/// Returns the request `request` of the executor of a test build, a `POST`
/// to the path of [`crate::execute::PATH`] whose body the host holds, or
/// `None` for any other request.
#[cfg(feature = "execute")]
fn execute_request(record: &[u8], request: u32) -> Option<Request> {
    let parsed = request_of(record, request)?;
    let url = parsed.url.as_str();
    let after_scheme = url.find("://").map_or(url, |at| &url[at + 3..]);
    let path = after_scheme.find('/').map_or("", |at| &after_scheme[at..]);
    if parsed.method != "POST" || path.split('?').next() != Some(crate::execute::PATH) {
        return None;
    }
    Some(parsed)
}

/// Returns the future of the answer of the booted app to the request record
/// `record` of `request`, which the server of the GraphQL endpoint or the app
/// of the engine answers as `to` names, or the record of the failure the
/// module answers at once where it is not booted or the record is not valid.
fn answer_of(record: &Rc<[u8]>, request: u32, to: Option<u8>) -> Result<Answer, Vec<u8>> {
    match (crate::boot::app(), request_of(record, request), to) {
        (None, _, _) => Err(failure(503, b"the module is not booted")),
        (Some(booted), Some(parsed), Some(TO_ENDPOINT)) => Ok(Box::pin(async move {
            record_of_answer(crate::endpoint::answer(booted, Rc::new(parsed), request).await).await
        })),
        (Some(booted), Some(parsed), Some(TO_APP)) => Ok(Box::pin(async move {
            let answer = booted
                .app
                .fetch(Rc::new(parsed), crate::boot::env(), None)
                .await;
            record_of_answer(answer).await
        })),
        _ => Err(failure(400, b"the record of the request is not valid")),
    }
}

/// Returns the record of `answer`, the answer of the app or the value it
/// threw, which answers 500.
async fn record_of_answer(answer: Result<Response, Thrown>) -> Answered {
    match answer {
        Ok(response) => record_of(&response).await,
        Err(_) => Answered::Whole(failure(500, b"the application threw")),
    }
}

/// Runs the jobs of the oldest reservation of the host for `request`, one
/// microtask of the host, and drives the request.
///
/// The host made the reservation at the call in which a callback of the app
/// settled a promise of the host inside an import, and the module kept the
/// jobs that settlement held for it, which run here before every job of the
/// queue, in a poll of the request or through `run_one_job` where no poll
/// reaches the drain.
pub fn resume(request: u32) {
    let oldest = with_entry(request, |entry| entry.reservations.pop_front())
        .flatten()
        .flatten();
    if let Some((reservation, agent)) = oldest {
        let count = agent.release_reserved(reservation);
        if count > 0 {
            run_granted(request, &agent, count);
            return;
        }
    }
    drive_again(request);
}

/// Drives `request` again and hands its answer to the host once it is ready.
fn drive_again(request: u32) {
    answer_once_ready(request, drive(request, None));
}

/// Hands the host the answer of `request` that a drive answered, if any.
fn answer_once_ready(request: u32, answer: Option<Vec<u8>>) {
    if let Some(answer) = answer {
        import::host_done(request, host::output(&answer));
    }
}

/// Settles the promise `handle` of `request` with the value record in the
/// first `len` bytes of the input buffer, fulfilled when `ok` is true and
/// rejected when it is false, and drives the request.
///
/// The host walked a fulfilled value before it wrote the record, as the
/// reaction of `wrapResolver` walks it, and a walk that threw there
/// arrives as a rejection with what it threw. In the faithful mode the
/// reactions the settlement enqueues run inside this microtask of the host,
/// in the drain of the poll that follows, or here where no poll reaches that
/// drain. A settlement through the resolving functions the host handed a
/// thenable it adopts, `adopted`, runs the jobs those reactions enqueue there
/// as well, as [`apply_settlement`] says.
pub fn settle(request: u32, handle: u32, ok: bool, adopted: bool, len: u32) {
    let record = host::input(len);
    let Some(promise) = unwait(request, handle) else {
        return;
    };
    let (completion, _whole) = settled(request, &promise, ok, &mut Reader::new(&record));
    if FAST.get() {
        promise.settle(completion);
        drive_again(request);
        return;
    }
    let agent = promise.agent();
    apply_settlement(&agent, None, adopted, || promise.settle(completion));
    answer_once_ready(request, drive(request, Some(&agent)));
    if !agent.has_held_jobs() || !in_flight(request) {
        return;
    }
    outside_a_poll(request, || agent.run_held_jobs());
    drive_again(request);
}

/// Settles the promise `handle` of `request` with the value record at the
/// cursor of `reader`, a settlement the host carried after the record of an
/// import, and answers whether that value record was whole.
///
/// A callback of the app made the settlement inside the import, and it takes
/// effect as the import returns, before the code that called the import goes
/// on, so in the job of that call. The host reserved a microtask at that
/// call, where V8 queues the reactions of the promise of the app, which calls
/// the export `resume`, and this notes the reservation for the request. In
/// the faithful mode the reactions the settlement enqueues, the reaction of
/// the step 3 the engine applies to the promise among them, run at once,
/// with the context of the request, and the jobs they enqueue, the reactions
/// of the consumer of the engine, wait for that reservation. A settlement
/// through the resolving functions the host handed a thenable it adopts,
/// `adopted`, runs the jobs those reactions enqueue at once as well, and the
/// jobs those enqueue wait for the reservation, as [`apply_settlement`] says.
/// In the fast mode the reactions go to the queue, which the poll that made
/// the call drains, and the reservation drives the request. The value record
/// of a promise the request waits on no more is read and dropped.
pub fn settle_carried(
    request: u32,
    handle: u32,
    ok: bool,
    adopted: bool,
    reader: &mut Reader<'_>,
) -> bool {
    let Some(promise) = unwait(request, handle) else {
        let whole = with_current(request, || value::decode(reader, request)).is_some();
        reserve(request, None);
        return whole;
    };
    let (completion, whole) = settled(request, &promise, ok, reader);
    if FAST.get() {
        promise.settle(completion);
        reserve(request, None);
        return whole;
    }
    let agent = promise.agent();
    let reservation = RESERVATIONS.get().wrapping_add(1);
    RESERVATIONS.set(reservation);
    apply_settlement(&agent, Some(reservation), adopted, || {
        promise.settle(completion);
    });
    outside_a_poll(request, || agent.run_reserved_settlement(reservation));
    reserve(request, Some((reservation, agent)));
    whole
}

/// Runs `body`, the settlement of a promise of the host on `agent`, under
/// `reservation`, with a level of held jobs more when `adopted` holds.
///
/// The host adopts a thenable of the app that a `then` answered as a promise
/// adopts it, through resolving functions of its own, so a settlement through
/// them, `adopted`, stands for one through the resolving functions the
/// async function of a root hands that thenable itself, whose consumer reacts
/// on the settlement at once. The consumer of the engine reaches the settlement
/// through two reactions, the step 3 the engine applies to the promise of the
/// field and the adoption of the promise that step derives by the async
/// function of the root of the engine, so such a settlement runs both through
/// `Agent::run_adopted_settlement`, while any other runs the first through
/// `Agent::run_settlement`, the step a callback of the call of `wrapResolver` stands for.
fn apply_settlement(agent: &Agent, reservation: Option<u32>, adopted: bool, body: impl FnOnce()) {
    if adopted {
        agent.run_adopted_settlement(reservation, body);
    } else {
        agent.run_settlement(reservation, body);
    }
}

/// Notes the next reservation of the host for `request`.
fn reserve(request: u32, reservation: Option<(u32, Agent)>) {
    let _noted = with_entry(request, |entry| entry.reservations.push_back(reservation));
}

/// Returns the completion of the value record at the cursor of `reader` for
/// `promise`, a promise of the host of `request`, fulfilled when `ok` holds,
/// and whether the record was whole.
///
/// The record is read inside the agent of the promise, so a promise of the
/// host in it, the promise of all items of an array among them, finds its
/// agent, a fulfilled value at the place the promise stands at, and a broken
/// record settles the promise with `undefined`.
fn settled(
    request: u32,
    promise: &JsPromise,
    ok: bool,
    reader: &mut Reader<'_>,
) -> (Completion, bool) {
    let agent = promise.agent();
    let settled = agent.enter(|| with_current(request, || promise.decode(reader, request, ok)));
    let whole = settled.is_some();
    let settled = settled.unwrap_or(Value::Undefined);
    let completion = if ok {
        Completion::Normal(settled)
    } else {
        Completion::Throw(settled)
    };
    (completion, whole)
}

/// Returns whether the client of `request` sent the variable `name`.
///
/// The names are read once per request from its record, with the request
/// parsers of `graphql-over-http` that the server of the facade runs, in the
/// order of its default list and the last that matches winning, as the hooks
/// `onRequestParse` of Yoga set the parser. A request no parser reads, a batch
/// and a request no more in flight count every variable as sent.
pub fn sent_variable(request: u32, name: &str) -> bool {
    let Some((sent, record, body)) = with_entry(request, |entry| {
        (
            entry.sent.clone(),
            Rc::clone(&entry.record),
            entry.body.clone(),
        )
    }) else {
        return true;
    };
    let sent = match sent {
        Sent::Unread => {
            let read =
                sent_names(&record, body).map_or(Sent::Unknown, |names| Sent::Names(names.into()));
            let _kept = with_entry(request, |entry| entry.sent = read.clone());
            read
        }
        known => known,
    };
    match sent {
        Sent::Names(names) => names.iter().any(|sent| sent == name),
        Sent::Unread | Sent::Unknown => true,
    }
}

/// Returns the names of the variables the parameters of the request record
/// `record` with its body `body`, the body the engine read, hold, or `None`
/// when no parser reads one set of parameters.
///
/// A body the engine has not read is none here, since no parser of Yoga read
/// the request then, and the request is read again from the bytes the entry
/// keeps, so no read of the host follows.
fn sent_names(record: &[u8], body: Option<Rc<[u8]>>) -> Option<Vec<String>> {
    let head = head_of(record)?;
    let request = Rc::new(Request::new(
        head.method,
        head.url,
        head.headers,
        body.map_or(Body::Empty, Body::Full),
    ));
    let mut chosen = None;
    if is_get_request(&request) {
        chosen = Some(parse_get_request as fn(_) -> _);
    }
    if is_post_json_request(&request) {
        chosen = Some(parse_post_json_request);
    }
    if is_post_multipart_request(&request) {
        chosen = Some(parse_post_multipart_request);
    }
    if is_post_graphql_string_request(&request) {
        chosen = Some(parse_post_graphql_string_request);
    }
    if is_post_form_url_encoded_request(&request) {
        chosen = Some(parse_post_form_url_encoded_request);
    }
    let mut parsed = chosen?(request);
    let Poll::Ready(Ok(RequestParserResult::Single(params))) = parsed
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    else {
        return None;
    };
    let Some(JsValue::Object(variables)) = params.variables() else {
        return Some(Vec::new());
    };
    Some(
        variables
            .iter()
            .filter_map(|(name, _)| name.as_str().map(String::from))
            .collect(),
    )
}

/// Takes the promise `handle` out of the promises `request` waits on, and
/// returns its stand in.
pub fn unwait(request: u32, handle: u32) -> Option<Rc<JsPromise>> {
    with_entry(request, |entry| {
        let at = entry.waits.iter().position(|(held, _)| *held == handle)?;
        Some(entry.waits.remove(at).1)
    })
    .flatten()
}

/// Drops `request` with its answer, every promise it waits on and every job
/// the host holds of it.
///
/// A job the host holds after the answer would run code of the app against
/// a table the host has dropped, so it is dropped with the request. The
/// entry leaves the table before it is dropped, so no drop runs inside the
/// borrow of the table.
fn drop_request(request: u32) {
    let gone = PENDING.with_borrow_mut(|pending| {
        pending
            .iter()
            .position(|entry| entry.request == request)
            .map(|at| pending.swap_remove(at))
    });
    drop(gone);
    crate::context::release(request);
    crate::clock::forget(request);
    crate::federation::forget(request);
    crate::sentry::forget(request);
}

/// Answers the export `release` of the host for `request`: drops a request
/// whose answer the host has read, or cancels the body the module pushes
/// after the head of its answer, whose reader of the host went away.
///
/// The stream of a cancelled body is dropped, so its cancel runs on its
/// agent, the `return` of the iterator of the app among it, which the drive
/// that follows starts and the exports that bring what it waits for go on
/// with, and the release of the request follows once the driver has ended.
/// The context of the request holds the answer as `c.res`, a second holder
/// of the stream, which the context gives up for an empty answer first,
/// since nothing reads `c.res` once the head went to the host.
pub fn release_by_host(request: u32) {
    let pushed = with_entry(request, |entry| {
        if entry.pushed {
            entry.cancelled = true;
        }
        entry.pushed
    })
    .unwrap_or(false);
    if !pushed {
        drop_request(request);
        return;
    }
    let (body, context) = with_entry(request, |entry| {
        let body = match entry.phase.as_mut() {
            Some(Phase::Body(pump)) => pump.body.take(),
            _ => None,
        };
        (body, entry.context.clone())
    })
    .unwrap_or((None, None));
    if let Some(context) = context {
        context.set_res(Response::empty(200));
    }
    drop(body);
    drive_again(request);
}

/// Drives `request` at a deadline of the timers of its agent, the export
/// `wake`, which the host calls in the task of its timer, and the first
/// time after the host took the head of an answer whose body the module
/// pushes.
pub fn wake(request: u32) {
    crate::clock::woke(request);
    drive_again(request);
}

/// Drives every request whose body the module pushes, after a publish to the
/// pub sub of the engine, whose pumps of the subscribers wait on the agents
/// of those requests.
pub fn drive_pushed() {
    let pushed: Vec<u32> = PENDING.with_borrow(|pending| {
        pending
            .iter()
            .filter(|entry| entry.pushed)
            .map(|entry| entry.request)
            .collect()
    });
    for request in pushed {
        drive_again(request);
    }
}

/// Polls the answer of `request` until it is ready or waits on the host, a
/// promise of the host or a job the host holds, and returns it once ready,
/// or drives the body the module pushes after the head of the answer.
///
/// The poll also stops while `watched`, the agent of a job or a settlement
/// of the host, keeps a grant or held jobs that the poll did not reach, which
/// its caller then runs outside a poll, and while the agent waits for a timer
/// of the host. The future leaves the table while it runs, so an import or
/// an export called meanwhile finds the entry and not the future. A head
/// whose body the module pushes is the answer, and the entry keeps the body
/// for the drives that follow.
fn drive(request: u32, watched: Option<&Agent>) -> Option<Vec<u8>> {
    let mut answer = match with_entry(request, |entry| entry.phase.take()).flatten()? {
        Phase::Head(answer) => answer,
        Phase::Body(pump) => {
            drive_body(request, pump);
            return None;
        }
        Phase::Open => {
            drive_open(request);
            return None;
        }
    };
    let mut cx = Context::from_waker(Waker::noop());
    for _ in 0..POLLS {
        match with_current(request, || answer.as_mut().poll(&mut cx)) {
            Poll::Ready(Answered::Whole(record)) => {
                answered(request);
                return Some(record);
            }
            Poll::Ready(Answered::Head(record, pump)) => {
                let _kept = with_entry(request, |entry| {
                    entry.phase = Some(Phase::Body(pump));
                    entry.pushed = true;
                });
                return Some(record);
            }
            Poll::Pending => {}
        }
        if poll_side(request, &mut cx) {
            continue;
        }
        let unreached = watched.is_some_and(|agent| agent.has_grants() || agent.has_held_jobs());
        if waits(request) || holds_jobs(request) || unreached || crate::clock::waits(request) {
            let _kept = with_entry(request, |entry| entry.phase = Some(Phase::Head(answer)));
            return None;
        }
    }
    drop_request(request);
    Some(failure(500, b"the answer did not end"))
}

/// Ends the dispatch of `request` once its whole answer is ready: a request
/// the host opened goes back to its open state, so the host writes the
/// access line through its logger and then closes it, and any other is
/// dropped.
fn answered(request: u32) {
    let opened = with_entry(request, |entry| {
        let opened = entry.opened.is_some();
        if opened {
            entry.phase = Some(Phase::Open);
        }
        opened
    })
    .unwrap_or(false);
    if !opened {
        drop_request(request);
    }
}

/// Ends the body `request` pushed: a request the host opened goes back to its
/// open state until the host ends it, so the access line the host writes
/// after the head still finds the logger of the request, and any other is
/// dropped. The host drops its table of the request at the release pushed
/// after this either way, since the engine calls the app no more for it.
fn body_ended(request: u32) {
    let opened = with_entry(request, |entry| {
        let opened = entry.opened.is_some();
        if opened {
            entry.phase = Some(Phase::Open);
            entry.pushed = false;
        }
        opened
    })
    .unwrap_or(false);
    if !opened {
        drop_request(request);
    }
}

/// Polls each call of the engine an open request `request` waits on, the
/// claims of a provider of Zitadel among them, until each waits on the host,
/// and keeps the open state.
fn drive_open(request: u32) {
    let mut cx = Context::from_waker(Waker::noop());
    for _ in 0..POLLS {
        if !poll_side(request, &mut cx) {
            break;
        }
    }
    let _kept = with_entry(request, |entry| entry.phase = Some(Phase::Open));
}

/// Drives the body `pump` of `request`, which the module pushes after the
/// head of its answer, until it ends or waits.
///
/// Each poll polls the driver and reads the stream, and then each call of
/// the engine the request started beside its answer, so the claims of a
/// provider the app calls while the body streams, from a pull of the stream
/// of a route among them, settle as they settle before the head. A poll
/// whose waker no future woke and in which no such call ended leaves the
/// drive, since what the body waits for comes with a later export. A body
/// whose stream and driver have ended releases the request, which the host
/// learns through the release pushed last, and drops the calls that have not
/// ended with it.
fn drive_body(request: u32, mut pump: Pump) {
    let woken = Arc::new(Woken::default());
    let waker = Waker::from(Arc::clone(&woken));
    let mut cx = Context::from_waker(&waker);
    for _ in 0..POLLS {
        woken.0.store(false, Ordering::Relaxed);
        let cancelled = with_entry(request, |entry| entry.cancelled).unwrap_or(true);
        if with_current(request, || pump.poll(request, cancelled, &mut cx)) {
            body_ended(request);
            push(request, RELEASED, &mut []);
            return;
        }
        if poll_side(request, &mut cx) {
            continue;
        }
        if !woken.0.load(Ordering::Relaxed) {
            break;
        }
    }
    let _kept = with_entry(request, |entry| entry.phase = Some(Phase::Body(pump)));
}

/// Pushes a record of the kind `kind` with the bytes `bytes` to the body of
/// `request` through the import `host_push`, which the host copies out of
/// the memory of the module before it returns, so a piece crosses as the
/// stream gave it, with no copy of the module.
fn push(request: u32, kind: u8, bytes: &mut [u8]) {
    import::host_push(
        request,
        u32::from(kind),
        host::address(bytes),
        host::length(bytes.len()),
    );
}

/// The head of the record of a request.
struct Head {
    /// The method.
    method: String,
    /// The URL.
    url: String,
    /// The header fields.
    headers: Headers,
    /// The handle of the reader of the body in the table of the request, or
    /// none for a GET and a HEAD, whose form of the body is the byte 0.
    body: Option<u32>,
}

/// Returns the head of the request record `record`.
fn head_of(record: &[u8]) -> Option<Head> {
    let mut reader = Reader::new(record);
    let method = reader.text()?.to_owned();
    let url = reader.text()?.to_owned();
    let mut headers = Headers::new();
    for _ in 0..reader.count()? {
        let name = reader.text()?;
        let value = reader.text()?;
        headers.append(name, value).ok()?;
    }
    let body = match reader.byte()? {
        0 => None,
        1 => Some(reader.word()?),
        _ => return None,
    };
    Some(Head {
        method,
        url,
        headers,
        body,
    })
}

/// Returns the request `request` of `record`, whose body, where it has one,
/// stays with the host until the first read of the body reads it.
fn request_of(record: &[u8], request: u32) -> Option<Request> {
    let head = head_of(record)?;
    let length = head
        .headers
        .get("content-length")
        .and_then(|value| value.trim().parse::<usize>().ok());
    let parsed = Request::new(head.method, head.url, head.headers, Body::Empty);
    Some(match head.body {
        Some(handle) => parsed.with_body_loader(Box::new(move || {
            Box::pin(read_body(request, handle, length))
        })),
        None => parsed,
    })
}

/// Reads the body of `request` the host holds behind its reader `handle`,
/// once, into one buffer, which the entry of the request keeps.
///
/// A body whose `Content-Length` names `length`, at most [`LARGEST`], is read
/// into a buffer of that length once a share of it, one part in [`SHARE`],
/// has arrived, so its bytes lie once in the memory of the module with no
/// growth, while a header that names more than its body sends claims no
/// memory beyond the bytes that came. Each chunk crosses through the import
/// `iter_next` as a body of a route crosses, and the host hands a chunk of
/// its stream in pieces of at most 64 KiB.
async fn read_body(
    request: u32,
    handle: u32,
    length: Option<usize>,
) -> Result<Rc<[u8]>, BodyError> {
    let stream = crate::context::host_stream(request, handle);
    let body = match length.filter(|length| *length <= LARGEST) {
        Some(length) => fill(&stream, length).await?,
        None => Rc::from(gather(&stream, Vec::new()).await?),
    };
    let _kept = with_entry(request, |entry| entry.body = Some(Rc::clone(&body)));
    Ok(body)
}

/// Reads `stream` into a buffer of `length` bytes, or into a buffer that
/// grows where the stream gives more, and answers a body of the bytes it gave
/// where it gives fewer.
///
/// The chunks stay apart until a share of `length`, one part in [`SHARE`],
/// has arrived, and the buffer of `length` bytes takes them then, so a body
/// that ends or stalls before that share holds only the bytes it sent, and an
/// honest body holds its first quarter twice for the moment of the move, at
/// most 1.25 times its length beside the memory before it.
// A buffer not yet written would skip the zero of each byte before the host
// fills it.
async fn fill(stream: &BodyStream, length: usize) -> Result<Rc<[u8]>, BodyError> {
    let mut early = Vec::new();
    let mut filled = 0;
    while filled < length.div_ceil(SHARE) {
        let Some(chunk) = stream.next().await else {
            return Ok(joined(&early, filled));
        };
        let chunk = chunk?;
        filled += chunk.len();
        early.push(chunk);
    }
    if filled > length {
        let bytes = early.concat();
        drop(early);
        return Ok(Rc::from(gather(stream, bytes).await?));
    }
    let mut body = joined(&early, length);
    drop(early);
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        let end = filled + chunk.len();
        let Some(place) = Rc::get_mut(&mut body).and_then(|bytes| bytes.get_mut(filled..end))
        else {
            let mut bytes = body.get(..filled).unwrap_or_default().to_vec();
            drop(body);
            bytes.extend_from_slice(&chunk);
            return Ok(Rc::from(gather(stream, bytes).await?));
        };
        place.copy_from_slice(&chunk);
        filled = end;
    }
    Ok(if filled == length {
        body
    } else {
        Rc::from(body.get(..filled).unwrap_or_default())
    })
}

/// Returns one buffer of `length` bytes that begins with the bytes of
/// `chunks`, which hold at most `length` bytes, and holds zeros after them.
fn joined(chunks: &[Vec<u8>], length: usize) -> Rc<[u8]> {
    let mut bytes: Rc<[u8]> = core::iter::repeat_n(0, length).collect();
    if let Some(place) = Rc::get_mut(&mut bytes) {
        let mut at = 0;
        for chunk in chunks {
            let end = at + chunk.len();
            if let Some(part) = place.get_mut(at..end) {
                part.copy_from_slice(chunk);
            }
            at = end;
        }
    }
    bytes
}

/// Reads the rest of `stream` after `bytes`.
async fn gather(stream: &BodyStream, mut bytes: Vec<u8>) -> Result<Vec<u8>, BodyError> {
    while let Some(chunk) = stream.next().await {
        bytes.extend_from_slice(&chunk?);
    }
    Ok(bytes)
}

/// Returns the address and the length of the bytes at `part` of the body of
/// `request`, the bytes of a file of its multipart body, which the host reads
/// in place before the call that names them returns, or `None` where the
/// entry holds no such part.
///
/// The body is shared and never written again, and the entry keeps it until
/// the request ends, so the range stays as it is while the host reads it.
pub fn body_part(request: u32, part: &Range<usize>) -> Option<(usize, usize)> {
    with_entry(request, |entry| {
        let bytes = entry.body.as_deref()?.get(part.clone())?;
        Some((bytes.as_ptr().expose_provenance(), bytes.len()))
    })
    .flatten()
}

/// Returns the record of `response`.
///
/// The answer that is a web standard `Response` a route of the host made,
/// unchanged, carries the handle of that response in place of its body. An
/// answer whose body is a stream is its head with the bit [`MARK_PUSHED`] and
/// an empty body, beside the stream and its driver, whose pieces the module
/// pushes after it, so the host writes the head first and each piece as it
/// comes, as the Node adapter writes the head of a response whose body
/// has not ended within its first reads and then each chunk of the body as it
/// comes. Any other answer carries its body.
async fn record_of(response: &Response) -> Answered {
    let mut mark = MARK_NONE;
    let streamed = streamed(response);
    if streamed {
        mark |= MARK_STREAM;
    }
    let fields = response.headers().entries();
    if let Body::Stream(stream) = response.made_body() {
        // The pieces a stream gives at once, as the Node adapter reads a body
        // before it writes the head: a stream that ends there is a whole
        // body, the text of a result of Yoga among them, and any other the
        // head with the pieces after it. A stream of a result processor of
        // Yoga is pushed whole or not.
        let mut read = VecDeque::new();
        let mut failure = None;
        let mut ended = false;
        if !streamed {
            loop {
                match poll_fn(|cx| Poll::Ready(stream.poll_chunk(cx))).await {
                    Poll::Ready(Some(Ok(piece))) => read.push_back(piece),
                    Poll::Ready(Some(Err(error))) => {
                        failure = Some(error.to_string());
                        break;
                    }
                    Poll::Ready(None) => {
                        ended = true;
                        break;
                    }
                    Poll::Pending => break,
                }
            }
        }
        if ended {
            let mut out = Vec::with_capacity(read.iter().map(Vec::len).sum::<usize>() + 64);
            out.push(mark);
            out.extend_from_slice(&response.status().to_le_bytes());
            host::push_count(&mut out, fields.len());
            for (name, value) in &fields {
                host::push_bytes(&mut out, name.as_bytes());
                host::push_bytes(&mut out, value.as_bytes());
            }
            host::push_bytes(&mut out, &read.into_iter().flatten().collect::<Vec<u8>>());
            return Answered::Whole(out);
        }
        let driver = stream.take_driver();
        let mut out = Vec::with_capacity(64);
        out.push(mark | MARK_PUSHED);
        out.extend_from_slice(&response.status().to_le_bytes());
        host::push_count(&mut out, fields.len());
        for (name, value) in &fields {
            host::push_bytes(&mut out, name.as_bytes());
            host::push_bytes(&mut out, value.as_bytes());
        }
        host::push_bytes(&mut out, &[]);
        let body = failure.is_none().then_some(stream);
        return Answered::Head(
            out,
            Pump {
                body,
                driver,
                read,
                failure,
            },
        );
    }
    if matches!(response.made_body(), Body::Text(_) | Body::Html(_)) {
        mark |= MARK_TEXT;
    }
    let body = response
        .bytes()
        .await
        .unwrap_or_else(|_| Rc::from(Vec::new()));
    let mut out = Vec::with_capacity(body.len() + 64);
    out.push(mark);
    out.extend_from_slice(&response.status().to_le_bytes());
    host::push_count(&mut out, fields.len());
    for (name, value) in &fields {
        host::push_bytes(&mut out, name.as_bytes());
        host::push_bytes(&mut out, value.as_bytes());
    }
    host::push_bytes(&mut out, &body);
    Answered::Whole(out)
}

/// Returns whether `response` answers with a media type of
/// [`STREAM_MEDIA_TYPES`], the answer of a result processor of Yoga that
/// answers with a stream.
fn streamed(response: &Response) -> bool {
    response.headers().get("content-type").is_some_and(|value| {
        let media = value.split(';').next().unwrap_or_default().trim();
        STREAM_MEDIA_TYPES
            .iter()
            .any(|kind| media.eq_ignore_ascii_case(kind))
    })
}

/// Returns the record of an answer the module writes itself.
fn failure(status: u16, text: &[u8]) -> Vec<u8> {
    let mut out = vec![MARK_NONE];
    out.extend_from_slice(&status.to_le_bytes());
    host::push_count(&mut out, 0);
    host::push_bytes(&mut out, text);
    out
}
