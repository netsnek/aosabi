// SPDX-License-Identifier: MIT
/*
 * aosabi-execute promise ambient: the running agents of the thread and the
 * ambient state that follows a future across an await.
 *
 * No clause of ECMA-262. The second half is the behaviour of
 * AsyncLocalStorage of node, https://nodejs.org/api/async_context.html, and
 * the first is what a free function needs to find the agent it runs on.
 */

//! The ambient state of a running agent.
//!
//! Two things are ambient. The first is the agent itself. A free function
//! such as [󰀀](crate::promise::sleep) takes no agent, so the agent that runs
//! pushes itself on a stack of the thread for as long as it runs, and
//! [`Agent::current`] reads the top. The second is the state of the layer
//! above, the context and the resolve info of a request in a store that
//! follows an async function across every await. The layer above keeps that
//! store in a cell of its own and hands the
//! agent two hooks. [`Agent::spawn`] captures the store once, and every poll
//! of the spawned future runs with the captured store in place.

use core::{
    any::Any,
    cell::RefCell, //
};
use std::rc::Rc;

use crate::promise::Agent;

/// Returns the ambient state that is in place now, for a future to keep.
pub type AmbientCapture = Rc<dyn Fn() -> Option<Rc<dyn Any>>>;

/// Puts the given ambient state in place and returns what was in place before.
pub type AmbientRestore = Rc<dyn Fn(Option<Rc<dyn Any>>) -> Option<Rc<dyn Any>>>;

/// The two hooks of [`Agent::set_ambient_hooks`].
#[derive(Clone)]
pub(super) struct AmbientHooks {
    /// Reads the state in place.
    capture: AmbientCapture,
    /// Swaps the state in place.
    restore: AmbientRestore,
}

/// One entry of the stack of running agents.
struct Frame {
    /// The agent that runs.
    agent: Agent,
    /// The spawned future that is being polled, if the frame is a poll.
    task: Option<usize>,
}

thread_local! {
    /// The agents that run on this thread, the innermost last.
    static RUNNING: RefCell<Vec<Frame>> = const { RefCell::new(Vec::new()) };
}

/// Pops the frame it stands for when it goes out of scope.
///
/// The pop sits in a destructor so that a panic that unwinds through a body
/// leaves the stack of the thread as it found it.
struct FrameGuard;

impl Drop for FrameGuard {
    fn drop(&mut self) {
        // The access fails only while the thread tears its locals down, and
        // then there is no stack left to restore.
        let _ = RUNNING.try_with(|running| running.borrow_mut().pop());
    }
}

impl Agent {
    /// Runs `body` with this agent as the current one of the thread.
    ///
    /// The host loop, every drain of the job queue and every poll of a
    /// spawned future run inside this function. A caller needs it only
    /// around code that runs outside of those and calls [`Agent::current`].
    pub fn enter<R>(&self, body: impl FnOnce() -> R) -> R {
        self.enter_frame(None, body)
    }

    /// Runs `body` as the poll of the spawned future `task`.
    pub(super) fn enter_task<R>(&self, task: usize, body: impl FnOnce() -> R) -> R {
        self.enter_frame(Some(task), body)
    }

    /// Pushes a frame, runs `body` and pops the frame.
    fn enter_frame<R>(&self, task: Option<usize>, body: impl FnOnce() -> R) -> R {
        let frame = Frame {
            agent: self.clone(),
            task,
        };
        let pushed = RUNNING.try_with(|running| running.borrow_mut().push(frame));
        let _guard = pushed.ok().map(|()| FrameGuard);
        body()
    }

    /// Returns the innermost agent that runs on this thread.
    ///
    /// A future that keeps the returned agent across an await keeps the
    /// agent alive for as long as the agent keeps the future, so the agent
    /// is read where it is needed and not stored.
    #[must_use]
    pub fn current() -> Option<Self> {
        Self::current_frame().map(|(agent, _)| agent)
    }

    /// Returns the innermost agent and the spawned future it polls, if any.
    pub(super) fn current_frame() -> Option<(Self, Option<usize>)> {
        RUNNING
            .try_with(|running| {
                let running = running.borrow();
                let frame = running.last()?;
                Some((frame.agent.clone(), frame.task))
            })
            .ok()
            .flatten()
    }

    /// Sets the two hooks that carry the ambient state of the layer above.
    ///
    /// `capture` answers the state in place, and `restore` puts a state in
    /// place and answers the one it replaced. The agent calls `capture` once
    /// when a future is spawned, and it calls `restore` before and after
    /// every poll of that future.
    pub fn set_ambient_hooks(&self, capture: AmbientCapture, restore: AmbientRestore) {
        *self.0.ambient_hooks.borrow_mut() = Some(AmbientHooks { capture, restore });
    }

    /// Returns the ambient state in place, or `None` without hooks.
    pub(super) fn capture_ambient(&self) -> Option<Rc<dyn Any>> {
        let hooks = self.0.ambient_hooks.borrow().clone()?;
        (hooks.capture)()
    }

    /// Puts `state` in place and returns what it replaced, or `None` without hooks.
    pub(super) fn swap_ambient(&self, state: Option<Rc<dyn Any>>) -> Option<Rc<dyn Any>> {
        let hooks = self.0.ambient_hooks.borrow().clone()?;
        (hooks.restore)(state)
    }
}
