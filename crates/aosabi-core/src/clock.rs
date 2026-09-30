// SPDX-License-Identifier: MIT
/*
 * aosabi-core: the clock capability, the timer a setup awaits outside a
 * request, and the turns of the loop that runs the clock.
 *
 * No analogue on the reference side, where the timers and the event loop of
 * the runtime are the clock. The capability is one of the five of
 * Documentation/architecture.md section 14.2.
 */

//! `install_clock`, `now_ms`, `delay`, `Delay`, `note_turn` and `turns`.
//!
//! A service asks its context for a capability rather than assuming one. The
//! timers of a request are the timers of its agent, which read the clock of
//! the host the boot hands the handler, and an agent runs only while a
//! request runs, so a setup of the boot, which runs on the runtime outside
//! any agent, awaits [`delay`], the timer of the clock the serving layer
//! installed before its first pass. The clock lives in a thread local of the
//! one runtime thread, as the network capability does. With no clock, which
//! is every build that runs no runtime, a delay is ready at once.
//!
//! The loop of the runtime counts its turns here as well. Node resolves the
//! dynamic import of a module from the file system only when its event loop
//! turns, so work the reference defers to such an import waits for a turn,
//! and the lazy sink of the logger reads [`turns`] to learn whether one has
//! passed. A runtime notes a turn each time its loop has run every task that
//! was ready, which is where the loop of Node goes on to poll.

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
    },
};
use std::rc::Rc;

use aosabi_execute::promise::agent::HostClock;

thread_local! {
    /// The clock of the runtime thread.
    static CLOCK: RefCell<Option<Rc<dyn HostClock>>> = const { RefCell::new(None) };

    /// The turns the loop of the runtime thread has noted.
    static TURNS: Cell<u64> = const { Cell::new(0) };
}

/// Installs `clock` as the clock of this thread, which [`delay`] reads.
pub fn install_clock(clock: Rc<dyn HostClock>) {
    // A thread that tears its locals down installs nothing any more.
    let _ = CLOCK.try_with(|slot| *slot.borrow_mut() = Some(clock));
}

/// Returns the milliseconds of the installed clock, or none with no clock.
///
/// A caller that bounds a sequence of awaits by one deadline reads the clock
/// here, and with no clock it sets no bound, since a delay is then ready at
/// once.
#[must_use]
pub fn now_ms() -> Option<u64> {
    CLOCK
        .try_with(|slot| slot.borrow().as_ref().map(|clock| clock.now_ms()))
        .ok()
        .flatten()
}

/// Awaits `ms` milliseconds of the installed clock,
/// `await new Promise(resolve => setTimeout(resolve, ms))` outside a request.
///
/// The deadline is taken when this function is called, as `setTimeout`
/// takes it. Inside a request the timer of its agent, `aosabi::time::sleep`,
/// keeps its place among the jobs of the agent, which this one does not.
pub fn delay(ms: u64) -> Delay {
    let clock = CLOCK.try_with(|slot| slot.borrow().clone()).ok().flatten();
    Delay {
        deadline: clock.map(|clock| (clock.now_ms().saturating_add(ms), clock)),
    }
}

/// The future of [`delay`].
#[must_use = "a delay waits only while it is awaited"]
pub struct Delay {
    /// The deadline and the clock it reads, `None` with no clock installed.
    deadline: Option<(u64, Rc<dyn HostClock>)>,
}

impl Future for Delay {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let Some((deadline, clock)) = &self.deadline else {
            return Poll::Ready(());
        };
        if clock.now_ms() >= *deadline {
            return Poll::Ready(());
        }
        clock.wake_at(*deadline, cx.waker().clone());
        Poll::Pending
    }
}

/// Notes one turn of the loop of the runtime on this thread.
pub fn note_turn() {
    // A thread that tears its locals down notes no turn any more.
    let _ = TURNS.try_with(|turns| turns.set(turns.get().wrapping_add(1)));
}

/// Returns the turns the loop of the runtime on this thread has noted, 0 on
/// a thread that runs no runtime.
#[must_use]
pub fn turns() -> u64 {
    TURNS.try_with(Cell::get).unwrap_or(0)
}
