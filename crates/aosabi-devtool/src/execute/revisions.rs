// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool execute revisions: subscribes to a topic of the pub sub of
 * one request on one road of a loaded service table and reads the revisions
 * the context of the subscription keeps after each result.
 *
 * No analogue in the reference, whose pub sub notes no revision. The crates
 * under test are aosabi-core, its pump, its carrier module and its served
 * road of GraphTP, and aosabi-execute, whose subscription entry runs the hook
 * of the events.
 */

//! The operation `revisions` of the batch form.
//!
//! A request carries `table`, a loaded table of the mode `pylon`, `road`,
//! `document`, a subscription to a topic of the pub sub of the request,
//! `publish`, a mutation that publishes one event to that topic, and
//! `events`, how many times `publish` runs, from 1 to 1024. The road is one
//! of three:
//!
//! - `subscribe` hands `document` to `Service::subscribe` of the service of
//!   the table with the policy of Network and polls the stream it answers.
//! - `http` boots an app over the roots of the table as a served build boots
//!   it, with the access line off, hands `POST /graphql` with the document
//!   and `accept: text/event-stream` to `BootedApp::fetch`, and reads the
//!   body as the serve layer reads it, with the driver of the stream run
//!   beside the reads.
//! - `graphtp` boots the same app, hands the document to the fetch a carrier
//!   hands its listener, `carrier_fetch` of `aosabi_core::graphtp::served`,
//!   whose straight road serves the app, and reads the body as the listener
//!   reads it.
//!
//! The executor of a subscription runs on its first `next`, so the probe
//! pulls once before it runs `publish` `events` times, each on an agent of
//! its own, and then reads one result per event. Right after each result it
//! reads the context that was in place when the subscription reached its
//! topic: the revision of the execution that yielded the result,
//! `take_revision` of `aosabi_core::carrier`, which no stream of these roads
//! takes otherwise, and how many revisions the context keeps whose execution
//! has not started, `noted_revisions`. The answer is `{"results": [..],
//! "revisions": [..], "noted": [..]}`, one entry per result in each: the text
//! of the result, which on the roads `http` and `graphtp` is the `data` of an
//! event `next` of the stream, its revision, `null` for none, and the count.
//! So a notebook holds the queue of revisions to at most one entry after each
//! execution on each road, and the revision of each result to the one the
//! pub sub of the request gave its publish.
//!
//! A table the run does not hold, a table of the mode `plain`, a road of no
//! such name, a subscription that answers no stream or reaches no topic, a
//! publish that answers no data and a stream that ends or stalls before its
//! last result are mistakes of the case and answer `fatal`.

use std::{
    cell::RefCell,
    future::Future,
    pin::{
        Pin,
        pin, //
    },
    rc::Rc,
    task::{
        Context as TaskContext,
        Poll,
        Waker, //
    },
};

use aosabi_core::{
    App, AppConfig, BootedApp, Fragment, HandlerOptions, IntoApp as _, LoggerOption, Resolvers,
    boot,
    carrier::{
        CarrierPolicy,
        Revision,
        noted_revisions,
        take_revision, //
    },
    context::Context,
    descriptor::NoRoots,
    graphtp::{
        self,
        Chunks,
        served, //
    },
    kernel::execute_closed::ResultStream,
};
use aosabi_execute::{
    kernel_entry::{
        Request,
        Subscribed,
        SubscriptionStream, //
    },
    promise::AgentOptions,
    result::ExecutionResult, //
};
use aosabi_http::{
    Body,
    BodyStream,
    Env,
    Headers,
    LocalBoxFuture,
    Request as HttpRequest, //
};
use aosabi_json::{
    JsObject,
    JsString,
    JsValue, //
};

use crate::{
    batch,
    execute::{
        SESSION,
        interpret::timer_mode,
        no_table,
        outcome_text,
        pub_sub::{
            begin_request,
            subscriber,
            watch_subscribers, //
        },
        result_text, //
    },
    plugins::BUILTINS,
};

/// The most times one request runs `publish`, the bound of the queue of `next` calls of the
/// repeater.
const MAX_EVENTS: f64 = 1024.0;

/// The polls of a boot, a fetch or a read the probe takes before it gives up.
///
/// Each turn polls the driver of the stream, which runs the agent of the
/// stream until nothing is left to run, so a read still pending after a turn
/// waits on nothing the probe does.
const ROUNDS: usize = 64;

/// The URL of the request of the road `http`, the endpoint of the app.
const URL: &str = "http://localhost/graphql";

/// The text of the event of a result in a stream of server sent events, up to its data.
const NEXT: &[u8] = b"event: next\ndata: ";

/// The end of each event of a stream of server sent events.
const BLANK_LINE: &[u8] = b"\n\n";

thread_local! {
    /// The tasks the spawner of the app and the fetch of the carrier hand the probe.
    static TASKS: RefCell<Vec<LocalBoxFuture<'static, ()>>> = const { RefCell::new(Vec::new()) };
    /// The roots [`TableRoots`] answers while the probe makes its app.
    static ROOTS: RefCell<Resolvers> = RefCell::new(Resolvers::default());
}

/// The fragment of the app of the probe, whose roots are those of the table.
#[derive(Clone, Copy)]
struct TableRoots;

impl Fragment for TableRoots {
    type Roots = NoRoots;

    fn resolvers(self) -> Resolvers {
        ROOTS.with_borrow(Clone::clone)
    }
}

/// What the probe read, one entry per result in each list.
#[derive(Default)]
struct Readings {
    /// The text of each result.
    results: Vec<JsValue>,
    /// The revision of the execution of each result, `null` for none.
    revisions: Vec<JsValue>,
    /// How many revisions the context kept after each result whose execution had not started.
    noted: Vec<JsValue>,
}

impl Readings {
    /// Notes the result `text` with what `context` keeps right after it.
    fn note(&mut self, text: String, context: &Context) {
        self.results.push(JsValue::from(text));
        let revision = take_revision(context).map_or(JsValue::Null, |Revision(n)| JsValue::from(n));
        self.revisions.push(revision);
        let noted = u64::try_from(noted_revisions(context)).unwrap_or(u64::MAX);
        self.noted.push(JsValue::from(noted));
    }
}

/// What one probe runs: the table, the two documents and the count of publishes.
struct Probe<'r> {
    /// The name of the table.
    table: &'r str,
    /// The subscription.
    document: &'r str,
    /// The mutation that publishes one event.
    publish: &'r str,
    /// How many times `publish` runs.
    events: usize,
}

/// Answers the operation `revisions`.
pub fn op_revisions(request: &JsObject) -> Result<JsObject, JsString> {
    let probe = Probe {
        table: batch::require_string(request, "table")?,
        document: batch::require_string(request, "document")?,
        publish: batch::require_string(request, "publish")?,
        events: events_of(request)?,
    };
    let road = batch::require_string(request, "road")?;
    begin_request();
    watch_subscribers(true);
    let answered = match road {
        "subscribe" => on_service(&probe),
        "http" | "graphtp" => on_app(&probe, road),
        other => Err(format!("no road named {other}")),
    };
    watch_subscribers(false);
    TASKS.take();
    let readings = answered.map_err(|reason| format!("revisions: {reason}"))?;
    let mut payload = JsObject::new();
    payload.insert("results", JsValue::Array(readings.results));
    payload.insert("revisions", JsValue::Array(readings.revisions));
    payload.insert("noted", JsValue::Array(readings.noted));
    Ok(payload)
}

/// Returns the field `events` of `request`, a whole number from 1 to [`MAX_EVENTS`].
fn events_of(request: &JsObject) -> Result<usize, String> {
    match request.get("events") {
        Some(JsValue::Number(events))
            if events.fract() == 0.0 && (1.0..=MAX_EVENTS).contains(events) =>
        {
            // The checks above keep the value a whole number up to 1024.
            #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            Ok(*events as usize)
        }
        _ => Err(format!(
            "revisions: the field events is no integer from 1 to {MAX_EVENTS}"
        )),
    }
}

/// Runs the probe on the road `subscribe`, the stream of `Service::subscribe`.
fn on_service(probe: &Probe<'_>) -> Result<Readings, String> {
    let request = Request {
        document: probe.document,
        variables: None,
        operation_name: None,
        validate: true,
    };
    let options = AgentOptions {
        timers: timer_mode(),
        trace: false,
    };
    let subscribed = SESSION.with_borrow(|session| {
        let service = session
            .service(probe.table)
            .ok_or_else(|| refused_table(session.schema(probe.table).is_some(), probe.table))?;
        Ok::<_, String>(service.subscribe(&request, options, CarrierPolicy::NETWORK))
    })?;
    let mut stream = match subscribed {
        Ok(Subscribed::Stream(stream)) => stream,
        Ok(Subscribed::One(result)) => {
            return Err(format!(
                "the subscription answered no stream: {}",
                result_text(&result)
            ));
        }
        Err(failure) => {
            return Err(format!(
                "the subscription answered no stream: {}",
                outcome_text(Err(failure))
            ));
        }
    };
    // The first pull runs the executor of the subscription, which listens to its topic.
    if poll_stream(&mut stream).is_ready() {
        return Err(String::from("the stream ended before its first publish"));
    }
    let context =
        subscriber().ok_or("the subscription reached no topic with a context in place")?;
    publish(probe)?;
    let mut readings = Readings::default();
    while readings.results.len() < probe.events {
        match poll_stream(&mut stream) {
            Poll::Ready(Some(result)) => readings.note(result_text(&result), &context),
            Poll::Ready(None) => {
                return Err(String::from("the stream ended before its last result"));
            }
            Poll::Pending => return Err(String::from("the stream stalled before its last result")),
        }
    }
    stream.close();
    Ok(readings)
}

/// Polls `stream` once, which runs its agent until a result is ready or nothing is left to run.
fn poll_stream(stream: &mut SubscriptionStream) -> Poll<Option<ExecutionResult>> {
    Pin::new(stream).poll_next(&mut TaskContext::from_waker(Waker::noop()))
}

/// Runs `publish` `events` times, each on an agent of its own, against the table of `probe`.
fn publish(probe: &Probe<'_>) -> Result<(), String> {
    let request = Request {
        document: probe.publish,
        variables: None,
        operation_name: None,
        validate: true,
    };
    for _ in 0..probe.events {
        let options = AgentOptions {
            timers: timer_mode(),
            trace: false,
        };
        let executed = SESSION
            .with_borrow(|session| session.execute(probe.table, &request, options))
            .ok_or_else(|| no_table(probe.table))?;
        match executed.outcome {
            Ok(result) if result.errors.is_empty() && result.has_data() => {}
            outcome => return Err(format!("the publish answered {}", outcome_text(outcome))),
        }
    }
    Ok(())
}

/// Runs the probe on the road `http` or `graphtp`, over an app booted over the table.
fn on_app(probe: &Probe<'_>, road: &str) -> Result<Readings, String> {
    let (type_defs, roots) = SESSION.with_borrow(|session| {
        session
            .app_parts(probe.table)
            .ok_or_else(|| refused_table(session.schema(probe.table).is_some(), probe.table))
    })?;
    let app = booted(type_defs, roots)?;
    let mut operation = br#"{"query":"#.to_vec();
    aosabi_json::write(&mut operation, &JsValue::from(probe.document));
    operation.push(b'}');
    let mut body = if road == "http" {
        http_body(&app, operation)?
    } else {
        graphtp_body(app, operation)?
    };
    // The read of the prelude marks the first pull, and one turn runs it,
    // which runs the executor of the subscription, which listens to its topic.
    if next_piece(&mut body)?.as_deref() != Some(b":\n\n".as_slice()) {
        return Err(String::from("the stream opened with no prelude"));
    }
    turn();
    let context =
        subscriber().ok_or("the subscription reached no topic with a context in place")?;
    publish(probe)?;
    let mut readings = Readings::default();
    let mut pending = Vec::new();
    while readings.results.len() < probe.events {
        let Some(piece) = next_piece(&mut body)? else {
            return Err(String::from("the stream ended before its last result"));
        };
        pending.extend_from_slice(&piece);
        while let Some(end) = find(&pending, BLANK_LINE) {
            let event: Vec<u8> = pending.drain(..end + BLANK_LINE.len()).collect();
            // A ping and the end of the stream carry no result.
            if let Some(data) = event.strip_prefix(NEXT) {
                let data = &data[..data.len() - BLANK_LINE.len()];
                readings.note(String::from_utf8_lossy(data).into_owned(), &context);
            }
        }
    }
    drop(body);
    turn();
    Ok(readings)
}

/// Returns the refusal of the table `name`, which the session holds as a
/// table of the mode `plain` when `plain` is true, and not at all otherwise.
fn refused_table(plain: bool, name: &str) -> String {
    if plain {
        format!("the table {name} is of the mode plain")
    } else {
        no_table(name)
    }
}

/// Boots an app over `roots` and the schema `type_defs` as a served build
/// boots it, with the spawner of the probe and the access line off.
///
/// The configuration is the one by default otherwise, so the boot adds no
/// plugin but the two the core makes itself, and the straight road of
/// `GraphTP` serves the app.
fn booted(type_defs: &'static str, roots: Resolvers) -> Result<Rc<BootedApp>, String> {
    ROOTS.set(roots);
    let app = App::new(TableRoots).into_app();
    ROOTS.take();
    let config = AppConfig {
        logger: Some(LoggerOption::Enabled(false)),
        ..AppConfig::default()
    };
    let options = HandlerOptions {
        type_defs,
        resolve_types: &[],
        graphql: Resolvers::default(),
        host_clock: None,
        spawner: Some(Rc::new(spawn_local)),
        parser_and_validation_cache_budget: None,
    };
    let (app, _listeners) = run(boot(app, config, options, &BUILTINS))
        .ok_or("the boot did not settle")?
        .map_err(|error| format!("the boot failed: {error}"))?;
    Ok(Rc::new(app))
}

/// The body of a stream on the road `http` or `graphtp`.
enum StreamBody {
    /// The body of the answer of the fetch of the app.
    Http(BodyStream),
    /// The body of the answer of the fetch of a carrier.
    Graphtp(Box<dyn Chunks>),
}

impl StreamBody {
    /// Polls the next piece of the body once.
    fn poll_piece(&mut self) -> Poll<Option<Result<Vec<u8>, String>>> {
        let mut context = TaskContext::from_waker(Waker::noop());
        match self {
            Self::Http(stream) => stream
                .poll_chunk(&mut context)
                .map(|piece| piece.map(|piece| piece.map_err(|error| error.to_string()))),
            Self::Graphtp(chunks) => chunks.poll_chunk(&mut context).map(|piece| {
                piece.map(|piece| piece.map_err(|_| String::from("the source of the body failed")))
            }),
        }
    }
}

/// Hands `POST /graphql` with `content` to the fetch of `app` and returns the stream of its answer.
///
/// The driver of the stream runs as a task of the probe, as the serve layer
/// runs it beside its reads.
fn http_body(app: &BootedApp, content: Vec<u8>) -> Result<StreamBody, String> {
    let mut headers = Headers::new();
    for (name, value) in [
        ("content-type", "application/json"),
        ("accept", "text/event-stream"),
    ] {
        headers
            .append(name, value)
            .map_err(|_| format!("the field {name} is refused"))?;
    }
    let request = Rc::new(HttpRequest::new(
        "POST",
        URL,
        headers,
        Body::Full(Rc::from(content)),
    ));
    let response = run(app.fetch(request, Env::Process, None))
        .ok_or("the fetch did not settle")?
        .map_err(|_| String::from("the fetch rejected"))?;
    let Body::Stream(stream) = response.body() else {
        return Err(format!(
            "the fetch answered {} with no stream",
            response.status()
        ));
    };
    if let Some(driver) = stream.take_driver() {
        spawn_local(driver);
    }
    Ok(StreamBody::Http(stream))
}

/// Hands `content` to the fetch a carrier hands its listener and returns the stream of its answer.
///
/// The fetch hands the driver of the stream to the spawner of the probe.
fn graphtp_body(app: Rc<BootedApp>, content: Vec<u8>) -> Result<StreamBody, String> {
    let fetch = served::carrier_fetch(app, spawn_local);
    let answer = run(fetch(graphtp::Request {
        credential: None,
        content,
    }))
    .ok_or("the fetch did not settle")?;
    match answer.body {
        graphtp::Body::Stream(chunks) => Ok(StreamBody::Graphtp(chunks)),
        graphtp::Body::Full(_) => Err(format!(
            "the fetch answered {} with no stream",
            answer.status
        )),
    }
}

/// Returns the next piece of `body`, `None` at its end, turning the tasks while it waits.
fn next_piece(body: &mut StreamBody) -> Result<Option<Vec<u8>>, String> {
    for _ in 0..ROUNDS {
        match body.poll_piece() {
            Poll::Ready(piece) => return piece.transpose(),
            Poll::Pending => turn(),
        }
    }
    Err(String::from("the stream stalled before its last result"))
}

/// Returns where `needle` starts in `haystack` first.
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Keeps `task` as a task of the probe, the spawner of the app and of the carrier.
fn spawn_local(task: LocalBoxFuture<'static, ()>) {
    TASKS.with_borrow_mut(|tasks| tasks.push(task));
}

/// Polls every task of the probe once and keeps the ones that are pending.
fn turn() {
    let mut context = TaskContext::from_waker(Waker::noop());
    let taken = TASKS.take();
    let mut kept = Vec::with_capacity(taken.len());
    for mut task in taken {
        if task.as_mut().poll(&mut context).is_pending() {
            kept.push(task);
        }
    }
    TASKS.with_borrow_mut(|tasks| {
        kept.append(tasks);
        *tasks = kept;
    });
}

/// Polls `future` beside the tasks of the probe until it is ready, `None`
/// when it is pending after [`ROUNDS`] turns.
fn run<F: Future>(future: F) -> Option<F::Output> {
    let mut future = pin!(future);
    let mut context = TaskContext::from_waker(Waker::noop());
    for _ in 0..ROUNDS {
        if let Poll::Ready(output) = future.as_mut().poll(&mut context) {
            return Some(output);
        }
        turn();
    }
    None
}
