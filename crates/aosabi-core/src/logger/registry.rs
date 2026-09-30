// SPDX-License-Identifier: MIT
/*
 * aosabi-core: the table of the bound loggers a logger handle names.
 *
 * No analogue on the reference side, where a logger is a JavaScript object a
 * closure holds. The table lets a logger be a copyable handle, so a static of
 * it is `Sync`, as `static LOG: Logger = logger("billing")` needs.
 */

//! The table of bound loggers.
//!
//! A [`Logger`] of a tag or of the scope holds no state, and a bound logger,
//! the root, a child or a tagged logger, holds a handle into a table of the
//! thread, whose slot holds the sink, the tees, the level of each tag, the
//! tag and the bindings. A slot made while a request context is in place
//! belongs to that request and is freed when its context goes, so the access
//! logger of every request costs no slot for good. A handle that outlives
//! its slot resolves to the root, which is what a module that kept a child
//! of a finished request gets.
//!
//! The table also holds the scoped logger, the store of the
//! `AsyncLocalStorage` of `logger.ts:266`, which [`WithLogger`] puts in place
//! around every poll of the future it wraps, and the flag of the access line
//! and the level of the job log, which the reference keeps in module
//! variables.

use core::{
    cell::{
        Cell,
        RefCell, //
    },
    future::Future,
    pin::Pin,
    task::{
        Context,
        Poll, //
    }, //
};
use std::rc::Rc;

use aosabi_json::JsObject;

use super::logger::{
    LogLevel,
    LogSink,
    Logger, //
};
use crate::context::ambient_context;

/// The level resolver of a bound logger, the minimum rank for a tag.
pub(crate) type LevelFor = Rc<dyn Fn(Option<&str>) -> u8>;

/// What a bound logger holds, `LoggerState` of `logger.ts:184-190`.
pub(crate) struct LoggerState {
    /// Where a record goes.
    pub(crate) sink: LogSink,
    /// The sinks a record also goes to, each with its minimum rank.
    pub(crate) tees: Vec<(LogSink, u8)>,
    /// The minimum rank of a record of a tag.
    pub(crate) level_for: LevelFor,
    /// The tag, `a:b` for a tag under a tag.
    pub(crate) tag: Option<String>,
    /// The fields every record carries.
    pub(crate) bindings: JsObject,
}

/// A handle of a slot of the table, which a copy of a logger holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoggerHandle {
    /// The position of the slot.
    index: u32,
    /// The generation of the slot, which a freed slot moves on.
    generation: u32,
}

impl LoggerHandle {
    /// Returns the position and the generation of the slot, the two numbers
    /// a host that holds a logger across a boundary keeps of it.
    pub(crate) const fn to_raw(self) -> (u32, u32) {
        (self.index, self.generation)
    }

    /// Returns the handle of the position and the generation `raw`, which
    /// resolves to the root when its slot has gone.
    pub(crate) const fn from_raw(raw: (u32, u32)) -> Self {
        Self {
            index: raw.0,
            generation: raw.1,
        }
    }
}

/// One slot of the table.
struct Slot {
    /// The generation of the slot.
    generation: u32,
    /// The state, `None` for a free slot.
    state: Option<Rc<LoggerState>>,
}

/// The table of one thread.
#[derive(Default)]
struct Registry {
    /// The slots.
    slots: Vec<Slot>,
    /// The free slots.
    free: Vec<u32>,
    /// The root logger, made at its first read.
    root: Option<LoggerHandle>,
}

thread_local! {
    /// The table of the thread.
    static REGISTRY: RefCell<Registry> = RefCell::new(Registry::default());
    /// The scoped logger in place, the store of `loggerContext`.
    static SCOPE: Cell<Option<Logger>> = const { Cell::new(None) };
    /// Whether the access line is written, `accessLog` of `logger.ts:380`.
    static ACCESS_LOG: Cell<bool> = const { Cell::new(true) };
    /// The level of the job log, `jobLogLvl` of `logger.ts:318`.
    static JOB_LOG_LEVEL: Cell<LogLevel> = const { Cell::new(LogLevel::Info) };
}

/// The slots a request context owns, freed when the context goes.
#[derive(Default)]
pub(crate) struct LoggerArena(RefCell<Vec<LoggerHandle>>);

impl Drop for LoggerArena {
    fn drop(&mut self) {
        let handles = core::mem::take(self.0.get_mut());
        // The table is gone while the thread tears its locals down, and then
        // there is nothing left to free.
        let _ = REGISTRY.try_with(|registry| {
            let mut registry = registry.borrow_mut();
            for handle in handles {
                registry.release(handle);
            }
        });
    }
}

impl Registry {
    /// Puts `state` into a free slot and returns its handle.
    fn insert(&mut self, state: Rc<LoggerState>) -> LoggerHandle {
        if let Some(index) = self.free.pop()
            && let Some(slot) = self.slots.get_mut(index as usize)
        {
            slot.state = Some(state);
            return LoggerHandle {
                index,
                generation: slot.generation,
            };
        }
        let index = u32::try_from(self.slots.len()).unwrap_or(u32::MAX);
        self.slots.push(Slot {
            generation: 0,
            state: Some(state),
        });
        LoggerHandle {
            index,
            generation: 0,
        }
    }

    /// Frees the slot of `handle` when it is still the one the handle names.
    fn release(&mut self, handle: LoggerHandle) {
        let Some(slot) = self.slots.get_mut(handle.index as usize) else {
            return;
        };
        if slot.generation != handle.generation || slot.state.is_none() {
            return;
        }
        slot.state = None;
        slot.generation = slot.generation.wrapping_add(1);
        self.free.push(handle.index);
    }

    /// Returns the state `handle` names, `None` for a freed slot.
    fn get(&self, handle: LoggerHandle) -> Option<Rc<LoggerState>> {
        let slot = self.slots.get(handle.index as usize)?;
        if slot.generation != handle.generation {
            return None;
        }
        slot.state.clone()
    }
}

/// Registers `state` and returns its handle, owned by the request whose
/// context is in place, or for good outside a request.
pub(crate) fn register(state: LoggerState) -> LoggerHandle {
    let handle = REGISTRY.with(|registry| registry.borrow_mut().insert(Rc::new(state)));
    if let Some(context) = ambient_context() {
        let arena = context.extension::<LoggerArena>().unwrap_or_else(|| {
            let arena = Rc::new(LoggerArena::default());
            context.set_extension(Rc::clone(&arena));
            arena
        });
        arena.0.borrow_mut().push(handle);
    }
    handle
}

/// Registers `state` for good, whatever context is in place.
pub(crate) fn register_permanent(state: LoggerState) -> LoggerHandle {
    REGISTRY.with(|registry| registry.borrow_mut().insert(Rc::new(state)))
}

/// Returns the state `handle` names, or the root's for a freed slot.
pub(crate) fn resolve(handle: LoggerHandle) -> Rc<LoggerState> {
    if let Some(state) = REGISTRY.with(|registry| registry.borrow().get(handle)) {
        return state;
    }
    let root = root_handle(super::logger::default_root_state);
    REGISTRY
        .with(|registry| registry.borrow().get(root))
        .unwrap_or_else(|| Rc::new(super::logger::default_root_state()))
}

/// Returns the handle of the root logger, made at the first read.
pub(crate) fn root_handle(make: impl FnOnce() -> LoggerState) -> LoggerHandle {
    if let Some(root) = REGISTRY.with(|registry| registry.borrow().root) {
        return root;
    }
    let root = register_permanent(make());
    REGISTRY.with(|registry| registry.borrow_mut().root = Some(root));
    root
}

/// Replaces the root logger, `__setRootLogger` of `logger.ts:258-260`.
pub(crate) fn set_root(handle: LoggerHandle) {
    REGISTRY.with(|registry| registry.borrow_mut().root = Some(handle));
}

/// Returns the scoped logger in place, `loggerContext.getStore()`.
pub(crate) fn scope() -> Option<Logger> {
    SCOPE.try_with(Cell::get).ok().flatten()
}

/// Returns whether the access line is written, `accessLogEnabled()`.
#[must_use]
pub fn access_log_enabled() -> bool {
    ACCESS_LOG.try_with(Cell::get).unwrap_or(true)
}

/// Turns the access line on or off, `setAccessLog(on)`.
pub fn set_access_log(on: bool) {
    let _ = ACCESS_LOG.try_with(|access| access.set(on));
}

/// Returns the level of the job log, `jobLogLevel()`.
#[must_use]
pub fn job_log_level() -> LogLevel {
    JOB_LOG_LEVEL.try_with(Cell::get).unwrap_or(LogLevel::Info)
}

/// Sets the level of the job log.
pub(crate) fn set_job_log_level(level: LogLevel) {
    let _ = JOB_LOG_LEVEL.try_with(|job| job.set(level));
}

/// A future that runs with a scoped logger in place around every poll.
///
/// It is the twin of `loggerContext.run(log, fn)`, whose store follows the
/// async function across every await.
pub struct WithLogger<F> {
    /// The logger put in place.
    log: Logger,
    /// The future.
    future: Pin<Box<F>>,
}

impl<F: Future> Future for WithLogger<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<F::Output> {
        let this = self.get_mut();
        let previous = SCOPE
            .try_with(|scope| scope.replace(Some(this.log)))
            .ok()
            .flatten();
        let poll = this.future.as_mut().poll(cx);
        let _ = SCOPE.try_with(|scope| scope.set(previous));
        poll
    }
}

/// Runs `future` with `log` as the scoped logger, `runWithLogger(log, fn)`.
pub fn run_with_logger<F: Future>(log: Logger, future: F) -> WithLogger<F> {
    WithLogger {
        log,
        future: Box::pin(future),
    }
}
