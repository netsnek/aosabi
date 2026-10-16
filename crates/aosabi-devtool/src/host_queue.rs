// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool host_queue: the scenarios of the queue of an agent that a
 * host owns, behind the feature host-queue.
 *
 * No analogue in the reference. The crate under test is the agent of
 * aosabi-execute with its feature host-queue, on which the module of the npm
 * package aosabi, crates/aosabi-js, runs its faithful job mode, and the
 * judging notebook is tests/notebooks/13_ts1_boundary.ipynb, which holds each
 * scenario to the order node keeps or to the contract the agent documents.
 */

//! The `host-queue` primitive of the seam.
//!
//! `--requests PATH` answers a file of request lines, one response line each,
//! in the batch form of [`crate::batch`], with the one operation `scenario`,
//! whose request names a scenario in `name` and whose answer is
//! `{"observations": [...]}`, the steps of the scenario in the order they
//! happened. Each scenario runs on a fresh agent with virtual timers, and the
//! host is a stand in that counts the jobs the notifier announces and runs
//! one of them per microtask through `run_one_job`.
//!
//! | Scenario | What it runs |
//! | --- | --- |
//! | `timer beside a job` | a timer of delay 0 and a job, the loop polled while the host holds it |
//! | `idle while the host holds a job` | the loop of `drain_until_idle` beside a held job |
//! | `a promise the host owes` | the loop over a declared promise, and one released unsettled |
//! | `a second notifier` | two jobs queued before a notifier, a second notifier and one job more |
//! | `the queue taken back` | two announced jobs, `None`, then `run_one_job` and the loop |
//! | `one job per microtask` | three microtasks of the host, the first job enqueuing the third |
//! | `a granted job` | two announced jobs, one grant to the drain of a poll, then a second grant |
//! | `a settlement` | the reactions of a promise the host settles beside a job the host holds |
//! | `a reserved settlement` | a settlement in a job, the jobs of its reactions in a reservation |
//! | `an adopted settlement` | the same for an adopted settlement, two levels run at once |
//! | `an adopted settlement in a drain` | an adopted settlement outside a job, beside a held job |
//!
//! A scenario this side does not know answers `fatal`.

use std::{
    cell::{
        Cell,
        RefCell, //
    },
    future::Future,
    path::PathBuf,
    pin::{
        Pin,
        pin, //
    },
    process::ExitCode,
    rc::Rc,
    task::{
        Context,
        Poll,
        Waker, //
    },
};

use aosabi_execute::{
    promise::{
        Agent,
        AgentOptions,
        Promise,
        PromiseState,
        Reaction, //
    },
    value::{
        Completion,
        Value, //
    },
};
use aosabi_json::{
    JsObject,
    JsString,
    JsValue, //
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
    seam::{
        self,
        Failure, //
    },
};

/// The name of the primitive.
const NAME: &str = "host-queue";

/// The batch the primitive lands at.
const LANDS_AT: &str = "TS1";

/// The operations of the protocol.
const OPERATIONS: &Operations = &[("scenario", op_scenario)];

/// The command line of the `host-queue` primitive.
pub static GRAMMAR: Grammar = Grammar {
    options: &[
        cli::SMOKE,
        cli::REQUESTS, //
    ],
    positional: None,
    conflicts: &[],
};

/// What the `host-queue` primitive accepts.
#[derive(Debug)]
pub struct HostQueueArgs {
    /// Whether the call asks for the interface line.
    smoke: bool,
    /// The file of request lines.
    requests: Option<PathBuf>,
}

impl HostQueueArgs {
    /// Reads the arguments `matches` holds for [`GRAMMAR`].
    pub fn read(matches: &Matches) -> Self {
        Self {
            smoke: matches.flag("smoke"),
            requests: matches.path("requests"),
        }
    }
}

/// Runs the `host-queue` primitive.
pub fn run(args: &HostQueueArgs) -> ExitCode {
    if args.smoke {
        return seam::smoke_at(NAME, LANDS_AT);
    }
    let outcome = match &args.requests {
        Some(requests) => batch::run(OPERATIONS, requests),
        None => Err(Failure::Usage(String::from("the call names no --requests"))),
    };
    seam::finish(NAME, outcome)
}

/// Answers the operation `scenario`, which runs the scenario `name` names.
fn op_scenario(request: &JsObject) -> Result<JsObject, JsString> {
    let Some(JsValue::String(name)) = request.get("name") else {
        return Err(JsString::from("the request names no scenario"));
    };
    let scenario = SCENARIOS
        .iter()
        .find(|(known, _)| *name == *known)
        .map(|(_, run)| *run)
        .ok_or_else(|| JsString::from("no scenario of that name"))?;
    let observations = scenario();
    let mut payload = JsObject::new();
    payload.insert(
        "observations",
        JsValue::Array(observations.into_iter().map(JsValue::from).collect()),
    );
    Ok(payload)
}

/// A scenario, which answers its observations.
type Scenario = fn() -> Vec<String>;

/// The scenarios by name.
const SCENARIOS: &[(&str, Scenario)] = &[
    ("timer beside a job", timer_beside_a_job),
    ("idle while the host holds a job", idle_while_held),
    ("a promise the host owes", promise_the_host_owes),
    ("a second notifier", second_notifier),
    ("the queue taken back", queue_taken_back),
    ("one job per microtask", one_job_per_microtask),
    ("a granted job", granted_job),
    ("a settlement", settlement),
    ("a reserved settlement", reserved_settlement),
    ("an adopted settlement", adopted_settlement),
    (
        "an adopted settlement in a drain",
        adopted_settlement_in_a_drain,
    ),
];

/// The notes of a scenario in the order they happen.
type Log = Rc<RefCell<Vec<String>>>;

/// A host stand in that owns the queue of one agent.
struct Host {
    /// The agent.
    agent: Agent,
    /// The jobs the notifier announced that the host has not run yet.
    held: Rc<Cell<usize>>,
    /// The notes of the scenario.
    log: Log,
}

impl Host {
    /// Returns a fresh agent with virtual timers whose queue the host owns.
    fn new() -> Self {
        let agent = Agent::new(AgentOptions::default());
        let held = Rc::new(Cell::new(0));
        let counted = Rc::clone(&held);
        agent.set_enqueue_notifier(Some(Rc::new(move |_agent: &Agent| {
            counted.set(counted.get() + 1);
        })));
        Self {
            agent,
            held,
            log: Rc::new(RefCell::new(Vec::new())),
        }
    }

    /// Runs one microtask the host holds, which runs one job of the agent,
    /// and returns whether the host held one.
    fn microtask(&self) -> bool {
        let held = self.held.get();
        if held == 0 {
            return false;
        }
        self.held.set(held - 1);
        let _ran = self.agent.run_one_job();
        true
    }

    /// Notes `text`.
    fn note(&self, text: impl Into<String>) {
        self.log.borrow_mut().push(text.into());
    }

    /// Returns the notes.
    fn notes(&self) -> Vec<String> {
        self.log.borrow().clone()
    }
}

/// Returns a reaction that notes `text` and passes its value through.
fn noting(log: &Log, text: &'static str) -> Reaction {
    let log = Rc::clone(log);
    Box::new(move |_agent, value| {
        log.borrow_mut().push(String::from(text));
        Completion::Normal(value)
    })
}

/// Enqueues a job that notes `text`, the reaction of a settled promise, and
/// returns the promise it settles.
fn job(agent: &Agent, log: &Log, text: &'static str) -> Promise {
    Promise::resolve(agent, Value::Undefined).then(agent, Some(noting(log, text)), None)
}

/// Polls `future` once with a waker that does nothing.
fn poll_once<F: Future>(future: Pin<&mut F>) -> Poll<F::Output> {
    future.poll(&mut Context::from_waker(Waker::noop()))
}

/// Returns the note of one poll of the loop of [`Agent::drive`].
fn drive_note<E>(poll: &Poll<Result<PromiseState, E>>) -> &'static str {
    match poll {
        Poll::Pending => "the loop parks",
        Poll::Ready(Ok(_)) => "the loop answers",
        Poll::Ready(Err(_)) => "the loop reports a stall",
    }
}

/// A timer of delay 0 and a job: the loop parks while the host holds the
/// job, fires no timer before it, and the timer fires after the job, as node
/// fires a timer after the microtasks that wait.
fn timer_beside_a_job() -> Vec<String> {
    let host = Host::new();
    let log = Rc::clone(&host.log);
    let _timer = host.agent.set_timeout(
        0,
        Box::new(move |_agent| log.borrow_mut().push(String::from("timer"))),
    );
    let done = job(&host.agent, &host.log, "job");
    let mut drive = pin!(host.agent.drive(&done));
    host.note(drive_note(&poll_once(drive.as_mut())));
    while host.microtask() {}
    host.note(drive_note(&poll_once(drive.as_mut())));
    let mut idle = pin!(host.agent.drain_until_idle());
    if poll_once(idle.as_mut()).is_ready() {
        host.note("the agent is idle");
    }
    host.notes()
}

/// The loop of `drain_until_idle` waits while the host holds a job and is
/// idle once the host has run it.
fn idle_while_held() -> Vec<String> {
    let host = Host::new();
    let _done = job(&host.agent, &host.log, "job");
    let mut idle = pin!(host.agent.drain_until_idle());
    let first = poll_once(idle.as_mut());
    host.note(if first.is_ready() {
        "the agent is idle"
    } else {
        "the agent waits"
    });
    while host.microtask() {}
    if poll_once(idle.as_mut()).is_ready() {
        host.note("the agent is idle");
    }
    host.notes()
}

/// The loop parks on a promise the host declares, answers once the host
/// settles it, and reports a stall for a declared promise the host releases
/// unsettled.
fn promise_the_host_owes() -> Vec<String> {
    let host = Host::new();
    let agent = &host.agent;
    let (promise, resolving) = Promise::with_resolvers(agent);
    agent.hold_host_work();
    let mut drive = pin!(agent.drive(&promise));
    host.note(drive_note(&poll_once(drive.as_mut())));
    agent.run_settlement(None, || {
        agent.release_host_work();
        resolving.resolve(agent, Value::Undefined);
    });
    host.note(drive_note(&poll_once(drive.as_mut())));
    let (dropped, _resolving) = Promise::with_resolvers(agent);
    agent.hold_host_work();
    agent.release_host_work();
    let mut stalled = pin!(agent.drive(&dropped));
    host.note(drive_note(&poll_once(stalled.as_mut())));
    host.notes()
}

/// The first notifier is told of every queued job once, a second notifier
/// replaces it and is told of no queued job again, and the next job goes to
/// the second alone.
fn second_notifier() -> Vec<String> {
    let agent = Agent::new(AgentOptions::default());
    let log: Log = Rc::new(RefCell::new(Vec::new()));
    let _a = job(&agent, &log, "a");
    let _b = job(&agent, &log, "b");
    let first = Rc::new(Cell::new(0_usize));
    let counted = Rc::clone(&first);
    agent.set_enqueue_notifier(Some(Rc::new(move |_agent: &Agent| {
        counted.set(counted.get() + 1);
    })));
    let mut notes = vec![format!(
        "the first notifier is told of {} jobs",
        first.get()
    )];
    let second = Rc::new(Cell::new(0_usize));
    let counted = Rc::clone(&second);
    agent.set_enqueue_notifier(Some(Rc::new(move |_agent: &Agent| {
        counted.set(counted.get() + 1);
    })));
    notes.push(format!(
        "the second notifier is told of {} jobs",
        second.get()
    ));
    let _c = job(&agent, &log, "c");
    notes.push(format!(
        "after one job more the first is told of {} and the second of {}",
        first.get(),
        second.get()
    ));
    notes
}

/// With `None` the agent takes its queue back: `run_one_job` runs no job and
/// the loop drains every announced job in order.
fn queue_taken_back() -> Vec<String> {
    let host = Host::new();
    let _a = job(&host.agent, &host.log, "a");
    let done = job(&host.agent, &host.log, "b");
    host.agent.set_enqueue_notifier(None);
    host.note(if host.agent.run_one_job() {
        "run_one_job runs a job"
    } else {
        "run_one_job runs no job"
    });
    let mut drive = pin!(host.agent.drive(&done));
    host.note(drive_note(&poll_once(drive.as_mut())));
    host.notes()
}

/// Each microtask of the host runs one job, a job enqueued inside a job waits
/// for a microtask of its own, and the order is the order of the queue, as
/// node runs `queueMicrotask`.
fn one_job_per_microtask() -> Vec<String> {
    let host = Host::new();
    let agent = &host.agent;
    let log = Rc::clone(&host.log);
    let first: Reaction = Box::new(move |agent, value| {
        log.borrow_mut().push(String::from("a"));
        let _c = job(agent, &log, "c");
        Completion::Normal(value)
    });
    let _a = Promise::resolve(agent, Value::Undefined).then(agent, Some(first), None);
    let _b = job(agent, &host.log, "b");
    let mut count = 0;
    while host.microtask() {
        count += 1;
        let ran = host.log.borrow().last().cloned().unwrap_or_default();
        host.note(format!("microtask {count} ran {ran}"));
    }
    host.notes()
}

/// A grant lets the drain of one poll run one job of the queue the host owns
/// and no more.
fn granted_job() -> Vec<String> {
    let host = Host::new();
    let agent = &host.agent;
    let _a = job(agent, &host.log, "a");
    let done = job(agent, &host.log, "b");
    agent.grant_jobs(1);
    let mut drive = pin!(agent.drive(&done));
    host.note(drive_note(&poll_once(drive.as_mut())));
    host.note(if agent.has_grants() {
        "a grant is left"
    } else {
        "no grant is left"
    });
    agent.grant_jobs(1);
    host.note(drive_note(&poll_once(drive.as_mut())));
    host.notes()
}

/// The reactions a settlement of the host enqueues run first in the next
/// drain, in order, beside a job the host holds, which waits for its own
/// microtask.
fn settlement() -> Vec<String> {
    let host = Host::new();
    let agent = &host.agent;
    let (promise, resolving) = Promise::with_resolvers(agent);
    let _x = promise.then(agent, Some(noting(&host.log, "x")), None);
    let done = promise.then(agent, Some(noting(&host.log, "y")), None);
    let _z = job(agent, &host.log, "z");
    agent.run_settlement(None, || resolving.resolve(agent, Value::Undefined));
    host.note(if agent.has_held_jobs() {
        "the settlement holds its reactions"
    } else {
        "the settlement holds no reaction"
    });
    let mut drive = pin!(agent.drive(&done));
    host.note(drive_note(&poll_once(drive.as_mut())));
    while host.microtask() {}
    host.notes()
}

/// A settlement a job makes for a reservation, as the module of the npm
/// package `aosabi` makes one for a callback of the app inside a call of that
/// job, runs the reaction of the settled promise at once, inside the job, and
/// keeps the job that reaction enqueues, untold to the host, until the
/// reservation hands it to the front of the queue, so it runs in the
/// microtask the host reserved at the call, before a job the job enqueued
/// after the settlement.
fn reserved_settlement() -> Vec<String> {
    let host = Host::new();
    let agent = &host.agent;
    let (promise, resolving) = Promise::with_resolvers(agent);
    let derived = promise.then(agent, Some(noting(&host.log, "x")), None);
    let _y = derived.then(agent, Some(noting(&host.log, "y")), None);
    let log = Rc::clone(&host.log);
    let settling: Reaction = Box::new(move |agent, value| {
        log.borrow_mut().push(String::from("a"));
        agent.run_settlement(Some(1), || resolving.resolve(agent, Value::Undefined));
        agent.run_reserved_settlement(1);
        let _b = job(agent, &log, "b");
        Completion::Normal(value)
    });
    let _a = Promise::resolve(agent, Value::Undefined).then(agent, Some(settling), None);
    let _ran = host.microtask();
    let released = agent.release_reserved(1);
    host.note(format!("the reservation holds {released} job"));
    let _reserved = agent.run_one_job();
    while host.microtask() {}
    host.notes()
}

/// An adopted settlement a job makes for a reservation, as the module makes
/// one for a resolving function it handed a thenable of the app inside a
/// call of that job, runs the reaction of the settled promise and the
/// reaction that one enqueues at once, inside the job, and keeps the job the
/// second enqueues, untold to the host, until the reservation hands it to the
/// front of the queue, before a job the job enqueued after the settlement.
fn adopted_settlement() -> Vec<String> {
    let host = Host::new();
    let agent = &host.agent;
    let (promise, resolving) = Promise::with_resolvers(agent);
    let derived = promise.then(agent, Some(noting(&host.log, "x")), None);
    let adopted = derived.then(agent, Some(noting(&host.log, "y")), None);
    let _z = adopted.then(agent, Some(noting(&host.log, "z")), None);
    let log = Rc::clone(&host.log);
    let settling: Reaction = Box::new(move |agent, value| {
        log.borrow_mut().push(String::from("a"));
        agent.run_adopted_settlement(Some(2), || resolving.resolve(agent, Value::Undefined));
        agent.run_reserved_settlement(2);
        let _b = job(agent, &log, "b");
        Completion::Normal(value)
    });
    let _a = Promise::resolve(agent, Value::Undefined).then(agent, Some(settling), None);
    let _ran = host.microtask();
    let released = agent.release_reserved(2);
    host.note(format!("the reservation holds {released} job"));
    let _reserved = agent.run_one_job();
    while host.microtask() {}
    host.notes()
}

/// An adopted settlement outside a job, as the module makes one in the
/// microtask of the export `settle`, runs the reaction of the settled promise
/// and the reaction that one enqueues in the next drain, at once and in
/// order, and the job the second enqueues waits for a microtask of its own,
/// after a job the host held before the settlement.
fn adopted_settlement_in_a_drain() -> Vec<String> {
    let host = Host::new();
    let agent = &host.agent;
    let (promise, resolving) = Promise::with_resolvers(agent);
    let derived = promise.then(agent, Some(noting(&host.log, "x")), None);
    let adopted = derived.then(agent, Some(noting(&host.log, "y")), None);
    let done = adopted.then(agent, Some(noting(&host.log, "z")), None);
    let _w = job(agent, &host.log, "w");
    agent.run_adopted_settlement(None, || resolving.resolve(agent, Value::Undefined));
    let mut drive = pin!(agent.drive(&done));
    host.note(drive_note(&poll_once(drive.as_mut())));
    while host.microtask() {}
    host.note(drive_note(&poll_once(drive.as_mut())));
    host.notes()
}
