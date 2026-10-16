// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool push: runs the hub of the kernel's publish primitive over the
 * op list of the push model, with u32 payloads and no agent.
 *
 * No analogue in the reference. The crate under test is the publish module of
 * aosabi-kernel, reached through the re-export of aosabi-core, the model is
 * tests/harness/push_model.py, and the judging notebook is
 * tests/notebooks/40_pubsub.ipynb.
 */

//! The `push` primitive of the seam.
//!
//! `--requests PATH` runs every request line of the file on a fresh hub of
//! `u32` payloads and writes one line per op, in the grammar of the push model
//! of the harness, so the notebook compares the two line by line. A request
//! line is `{"id": ..., "ops": [...]}`, and the line of an op is
//! `{"id", "n", "op", ...}` with `n` counting the ops of the request from 1.
//!
//! | Op | Fields | The line after `op` |
//! | --- | --- | --- |
//! | `listen` | `topic`, `depth` | `slot`, the id, from 0 in listen order |
//! | `unlisten` | `slot` | `slot`, `waker`, whether the slot held a waker |
//! | `publish` | `topic`, `payload` | `revision`, `reached`, `coalesced`, `woken`, the slots woken in listen order |
//! | `take` | `slot`, `waker` | `slot`, `take` as `ready` with `revision` and `payload`, `empty` or `gone` |
//! | `state` | `slot` or nothing | `slot`, `pending` and `coalesced`, or `slot` and `gone`, or `revision` and `slots` |
//! | `allow` | `carrier`, `only` or nothing, `field` | `carrier`, `field`, `allowed`, and `refusal` when refused |
//! | `restart` | nothing | `revision` and `slots`, both 0 |
//! | `time` | `slots`, `depth`, `publishes`, `lock` | `slots`, `depth`, `publishes`, `lock`, `min_ns`, `median_ns`, `p99_ns` |
//!
//! A waker of the hub is a waker of the seam that writes the id of its slot
//! into a log when it is woken, so `woken` is what [`publish`] woke after it
//! released the lock, in the order it woke them. The op `time` is the seam's
//! alone and the model has no line for it: it times `publishes` publishes on a
//! hub of its own with `slots` slots of one topic at `depth`, each of them full
//! after the first publish when the depth is 1, and coalescing once its depth
//! is reached, under the lock of one thread or under the `std::sync::Mutex` of
//! the served pub sub, and answers the least, the median and the 99th
//! percentile of the times in nanoseconds.
//!
//! An op that breaks the grammar writes `fatal` with the reason the model
//! writes, and the ops after it are not run. A request line that is no JSON
//! object writes one line `{"id": null, "n": 0, "op": null, "fatal": ...}`.
//!
//! `--smoke` prints one line of type sizes in bytes before the interface line:
//! `hub` is a `Hub<u32>`, `slot` one slot of it in the list of the hub,
//! `event` one pending event of it, and `pub_sub_slot` and `pub_sub_event`
//! the same two for the hub of the served pub sub, whose payload is an `Arc`.

use std::{
    mem,
    path::PathBuf,
    process::ExitCode,
    sync::{
        Arc,
        Mutex,
        PoisonError, //
    },
    task::{
        Wake,
        Waker, //
    },
    time::Instant, //
};

use aosabi_core::{
    kernel::publish::{
        Allow,
        CarrierId,
        Delivered,
        Hub,
        LocalLock,
        Lock,
        SlotId,
        Take,
        publish,
        write_refusal, //
    },
    pub_sub::{
        SharedPayload,
        StdLock, //
    },
};
use aosabi_json::{
    JsObject,
    JsValue, //
};
use aosabi_lang::js_string::to_string_lossy;

use crate::{
    cli::{
        self,
        Grammar,
        Matches, //
    },
    json::reader,
    seam::{
        self,
        Answer, //
    }, //
};

/// The name of the primitive.
const NAME: &str = "push";

/// The milestone the primitive lands at.
const LANDS_AT: &str = "M6";

/// The largest depth of a slot, a `u16`.
const U16: u32 = 0xffff;

/// The largest slot id and payload, a `u32`.
const U32: u32 = 0xffff_ffff;

/// The largest count of slots and of publishes the op `time` takes.
const TIME_BOUND: u32 = 1_000_000;

/// The carriers of ADR-0023 in their order, by the name an op spells.
const CARRIERS: [(&str, CarrierId); 6] = [
    ("network", CarrierId::NETWORK),
    ("unix_socket", CarrierId::UNIX_SOCKET),
    ("usb", CarrierId::USB),
    ("bluetooth", CarrierId::BLUETOOTH),
    ("serial_line", CarrierId::SERIAL_LINE),
    ("lora", CarrierId::LORA),
];

/// The command line of the `push` primitive.
pub static GRAMMAR: Grammar = Grammar {
    options: &[
        cli::SMOKE,
        cli::REQUESTS, //
    ],
    positional: None,
    conflicts: &[],
};

/// What the `push` primitive accepts.
#[derive(Debug)]
pub struct PushArgs {
    /// Whether the call asks for the interface line.
    smoke: bool,
    /// The file of request lines.
    requests: Option<PathBuf>,
}

impl PushArgs {
    /// Reads the arguments `matches` holds for [`GRAMMAR`].
    pub fn read(matches: &Matches) -> Self {
        Self {
            smoke: matches.flag("smoke"),
            requests: matches.path("requests"),
        }
    }
}

/// Runs the `push` primitive.
pub fn run(args: &PushArgs) -> ExitCode {
    if args.smoke {
        println!("{}", sizes_line());
        return seam::smoke_at(NAME, LANDS_AT);
    }
    let outcome = seam::required(args.requests.as_deref(), "--requests")
        .and_then(seam::read_bytes)
        .map(|bytes| Answer::Payload(answer_all(&bytes)));
    seam::finish(NAME, outcome)
}

/// Returns the line of type sizes `--smoke` prints before the interface line.
fn sizes_line() -> String {
    let mut sizes = JsObject::new();
    sizes.insert("hub", count(mem::size_of::<Hub<u32>>()));
    sizes.insert("slot", count(Hub::<u32>::SLOT_SIZE));
    sizes.insert("event", count(mem::size_of::<Delivered<u32>>()));
    sizes.insert("pub_sub_slot", count(Hub::<SharedPayload>::SLOT_SIZE));
    sizes.insert(
        "pub_sub_event",
        count(mem::size_of::<Delivered<SharedPayload>>()),
    );
    // An object always has a JSON form, so the writer's `None` arm is
    // unreachable here.
    aosabi_json::to_string(&JsValue::Object(sizes)).unwrap_or_default()
}

/// Returns `value` as a JSON number.
#[expect(
    clippy::cast_precision_loss,
    reason = "a size or a count of the seam stays far below 2^53"
)]
fn count(value: usize) -> JsValue {
    JsValue::Number(value as f64)
}

/// Answers every request line of `bytes`, one line per op.
fn answer_all(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for line in bytes.split(|&byte| byte == b'\n') {
        if line.trim_ascii().is_empty() {
            continue;
        }
        for written in answer(line) {
            aosabi_json::write(&mut out, &JsValue::Object(written));
            out.push(b'\n');
        }
    }
    out
}

/// Returns the lines of one request line.
fn answer(line: &[u8]) -> Vec<JsObject> {
    let Ok(JsValue::Object(request)) = reader::parse(line) else {
        let mut head = JsObject::new();
        head.insert("id", JsValue::Null);
        head.insert("n", 0_u32);
        head.insert("op", JsValue::Null);
        head.insert("fatal", "the request line is not a JSON object");
        return vec![head];
    };
    let id = request.get("id").cloned().unwrap_or(JsValue::Null);
    let ops = match request.get("ops") {
        Some(JsValue::Array(ops)) => ops.as_slice(),
        _ => &[],
    };
    let mut run = Run::new();
    let mut lines = Vec::new();
    for (n, op) in (1_u32..).zip(ops) {
        let mut line = JsObject::new();
        line.insert("id", id.clone());
        line.insert("n", n);
        let JsValue::Object(op) = op else {
            line.insert("op", JsValue::Null);
            line.insert("fatal", "the op is no object");
            lines.push(line);
            break;
        };
        line.insert("op", op.get("op").cloned().unwrap_or(JsValue::Null));
        match run.apply(op) {
            Ok(fields) => {
                for (key, value) in &fields {
                    line.insert(key.clone(), value.clone());
                }
                lines.push(line);
            }
            Err(reason) => {
                line.insert("fatal", reason);
                lines.push(line);
                break;
            }
        }
    }
    lines
}

/// The waker of one slot, which logs the id of its slot when it is woken.
struct SlotWaker {
    /// The id of the slot whose take kept the waker.
    slot: u32,
    /// The log the publish reads, shared by every waker of one run.
    log: Arc<Mutex<Vec<u32>>>,
}

impl Wake for SlotWaker {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.log
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(self.slot);
    }
}

/// The hub of one request and the log its wakers write.
struct Run {
    /// The hub under test, behind the lock of one thread.
    hub: LocalLock<Hub<u32>>,
    /// The ids of the slots woken since the last read, in the order they woke.
    log: Arc<Mutex<Vec<u32>>>,
}

impl Run {
    /// Returns a run with a fresh hub.
    fn new() -> Self {
        Self {
            hub: LocalLock::new(Hub::new()),
            log: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Runs one op and returns the fields of its line after `op`.
    fn apply(&mut self, op: &JsObject) -> Result<JsObject, String> {
        let mut line = JsObject::new();
        match op.get("op") {
            Some(JsValue::String(name)) if *name == "listen" => {
                let topic = text(op, "topic")?;
                let depth = integer(op, "depth", U16)?;
                // The bound of the check above keeps the depth inside a `u16`.
                let depth = u16::try_from(depth).unwrap_or(u16::MAX);
                let slot = self.hub.with(|hub| hub.listen(&topic, depth));
                line.insert("slot", slot.get());
            }
            Some(JsValue::String(name)) if *name == "unlisten" => {
                let slot = integer(op, "slot", U32)?;
                let waker = self.hub.with(|hub| hub.unlisten(SlotId::new(slot)));
                line.insert("slot", slot);
                line.insert("waker", waker.is_some());
            }
            Some(JsValue::String(name)) if *name == "publish" => {
                let topic = text(op, "topic")?;
                let payload = integer(op, "payload", U32)?;
                let published = publish(&self.hub, &topic, &payload);
                let woken =
                    mem::take(&mut *self.log.lock().unwrap_or_else(PoisonError::into_inner));
                line.insert("revision", revision(published.revision.0));
                line.insert("reached", published.reached);
                line.insert("coalesced", published.coalesced);
                line.insert(
                    "woken",
                    JsValue::Array(woken.into_iter().map(JsValue::from).collect()),
                );
            }
            Some(JsValue::String(name)) if *name == "take" => {
                let Some(JsValue::Bool(waker)) = op.get("waker") else {
                    return Err(String::from("waker is no boolean"));
                };
                let slot = integer(op, "slot", U32)?;
                self.take(slot, *waker, &mut line);
            }
            Some(JsValue::String(name)) if *name == "state" => self.state(op, &mut line)?,
            Some(JsValue::String(name)) if *name == "allow" => allow(op, &mut line)?,
            Some(JsValue::String(name)) if *name == "restart" => {
                self.hub = LocalLock::new(Hub::new());
                let (revision_now, slots) =
                    self.hub.with(|hub| (hub.revision().0, hub.slot_count()));
                line.insert("revision", revision(revision_now));
                line.insert("slots", count(slots));
            }
            Some(JsValue::String(name)) if *name == "time" => time(op, &mut line)?,
            name => {
                let printed = name
                    .and_then(aosabi_json::to_string)
                    .unwrap_or_else(|| String::from("null"));
                return Err(format!("unknown op {printed}"));
            }
        }
        Ok(line)
    }

    /// Takes the oldest event of `slot`, keeping a waker of the seam when `waker` is true.
    fn take(&self, slot: u32, waker: bool, line: &mut JsObject) {
        let kept = waker.then(|| {
            Waker::from(Arc::new(SlotWaker {
                slot,
                log: Arc::clone(&self.log),
            }))
        });
        let taken = self
            .hub
            .with(|hub| hub.take(SlotId::new(slot), kept.as_ref()));
        line.insert("slot", slot);
        match taken {
            Take::Ready(event) => {
                line.insert("take", "ready");
                line.insert("revision", revision(event.revision.0));
                line.insert("payload", event.payload);
            }
            Take::Empty => {
                line.insert("take", "empty");
            }
            Take::Gone => {
                line.insert("take", "gone");
            }
        }
    }

    /// Writes the state of the slot `op` names, or of the hub when it names none.
    fn state(&self, op: &JsObject, line: &mut JsObject) -> Result<(), String> {
        if !op.contains_key("slot") {
            let (revision_now, slots) = self.hub.with(|hub| (hub.revision().0, hub.slot_count()));
            line.insert("revision", revision(revision_now));
            line.insert("slots", count(slots));
            return Ok(());
        }
        let slot = integer(op, "slot", U32)?;
        line.insert("slot", slot);
        match self.hub.with(|hub| hub.state(SlotId::new(slot))) {
            Some(state) => {
                line.insert("pending", u32::from(state.pending));
                line.insert("coalesced", state.coalesced);
            }
            None => {
                line.insert("gone", true);
            }
        }
        Ok(())
    }
}

/// Returns a revision as a JSON number.
#[expect(
    clippy::cast_precision_loss,
    reason = "a revision of the seam stays far below 2^53"
)]
fn revision(revision: u64) -> JsValue {
    JsValue::Number(revision as f64)
}

/// Writes the answer of the allowlist of the carrier `op` names for its field.
///
/// The fields are read in the order of the model, the carrier, the field and
/// the list, so a request that breaks two of them fails on the same one.
fn allow(op: &JsObject, line: &mut JsObject) -> Result<(), String> {
    let carrier = match op.get("carrier") {
        Some(JsValue::String(name)) => CARRIERS.iter().find(|(spelt, _)| *name == **spelt),
        _ => None,
    };
    let Some(&(_, carrier)) = carrier else {
        let names: Vec<&str> = CARRIERS.iter().map(|(name, _)| *name).collect();
        return Err(format!("carrier is none of {}", names.join(", ")));
    };
    let field = text(op, "field")?;
    let allowlist = match op.get("only") {
        None | Some(JsValue::Null) => Allow::All,
        Some(JsValue::Array(fields)) => Allow::Only(leak_fields(fields)?),
        Some(_) => return Err(String::from("only is no list of strings")),
    };
    let allowed = allowlist.allows(&field);
    line.insert("carrier", carrier.name());
    line.insert("field", field.as_str());
    line.insert("allowed", allowed);
    if !allowed {
        let mut refusal = String::new();
        // A write into a `String` cannot fail.
        let _ = write_refusal(&field, carrier, &mut refusal);
        line.insert("refusal", refusal);
    }
    Ok(())
}

/// Returns the fields of an allowlist as the `'static` list a card names.
///
/// A card writes its allowlist as a constant, so the kernel takes a list
/// that lives as long as the program, and the seam leaks the few bytes of
/// each list it reads, which live as long as its one run.
fn leak_fields(fields: &[JsValue]) -> Result<&'static [&'static str], String> {
    let mut leaked = Vec::with_capacity(fields.len());
    for field in fields {
        let JsValue::String(field) = field else {
            return Err(String::from("only is no list of strings"));
        };
        let text: &'static str = Box::leak(to_string_lossy(field).into_owned().into_boxed_str());
        leaked.push(text);
    }
    Ok(Box::leak(leaked.into_boxed_slice()))
}

/// Times publishes to a hub of its own, the op `time`.
fn time(op: &JsObject, line: &mut JsObject) -> Result<(), String> {
    let slots = integer(op, "slots", TIME_BOUND)?;
    let depth = integer(op, "depth", U16)?;
    let publishes = integer(op, "publishes", TIME_BOUND)?.max(1);
    let lock = match op.get("lock") {
        None => "local",
        Some(JsValue::String(name)) if *name == "local" => "local",
        Some(JsValue::String(name)) if *name == "std" => "std",
        Some(_) => return Err(String::from("lock is neither local nor std")),
    };
    // The bound of the check above keeps the depth inside a `u16`.
    let depth = u16::try_from(depth).unwrap_or(u16::MAX);
    let mut times = if lock == "std" {
        time_under(&StdLock::new(Hub::new()), slots, depth, publishes)
    } else {
        time_under(&LocalLock::new(Hub::new()), slots, depth, publishes)
    };
    times.sort_unstable();
    line.insert("slots", slots);
    line.insert("depth", u32::from(depth));
    line.insert("publishes", publishes);
    line.insert("lock", lock);
    line.insert("min_ns", nanos(times.first().copied()));
    line.insert("median_ns", nanos(times.get(times.len() / 2).copied()));
    line.insert("p99_ns", nanos(times.get(times.len() * 99 / 100).copied()));
    Ok(())
}

/// Returns the times of `publishes` publishes to `slots` slots of one topic under `lock`.
///
/// One publish before the timed ones fills every slot up to its depth when
/// the depth is 1, so the timed publishes walk slots that coalesce.
fn time_under<L: Lock<Hub<u32>>>(lock: &L, slots: u32, depth: u16, publishes: u32) -> Vec<u128> {
    lock.with(|hub| {
        for _ in 0..slots {
            hub.listen("changed", depth);
        }
    });
    publish(lock, "changed", &0);
    let mut times = Vec::with_capacity(usize::try_from(publishes).unwrap_or(0));
    for payload in 1..=publishes {
        let started = Instant::now();
        publish(lock, "changed", &payload);
        times.push(started.elapsed().as_nanos());
    }
    times
}

/// Returns a time in nanoseconds as a JSON number, or null for no time.
#[expect(
    clippy::cast_precision_loss,
    reason = "a time of one publish stays far below 2^53 nanoseconds"
)]
fn nanos(time: Option<u128>) -> JsValue {
    time.map_or(JsValue::Null, |time| JsValue::Number(time as f64))
}

/// Returns the string field `key` of `op`, the `_text` of the model.
fn text(op: &JsObject, key: &str) -> Result<String, String> {
    match op.get(key) {
        Some(JsValue::String(value)) => Ok(to_string_lossy(value).into_owned()),
        _ => Err(format!("{key} is no string")),
    }
}

/// Returns the integer field `key` of `op` from 0 to `bound`, the `_integer` of the model.
fn integer(op: &JsObject, key: &str, bound: u32) -> Result<u32, String> {
    let refusal = || format!("{key} is no integer from 0 to {bound}");
    let Some(JsValue::Number(value)) = op.get(key) else {
        return Err(refusal());
    };
    if value.fract() != 0.0 || !(0.0..=f64::from(bound)).contains(value) {
        return Err(refusal());
    }
    // The checks above keep the value a whole number inside a `u32`.
    #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(*value as u32)
}
