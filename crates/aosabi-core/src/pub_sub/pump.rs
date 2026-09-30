// SPDX-License-Identifier: MIT
/*
 * aosabi-core pub_sub pump: the task that moves an event from the hub into
 * the repeater of its subscriber.
 *
 * The pump awaits each push, so a backlog waits in the hub, bounded and
 * coalesced.
 */

//! The pump from the hub into a subscriber's repeater.
//!
//! The pump is a future spawned on the agent of the stream. It takes the
//! oldest event of its slot, keeping the waker of its task in the slot when
//! the slot is empty, so a publish from any thread wakes it through the
//! foreign waker path of the agent. It notes the revision of the event on the
//! request context, turns the payload into a value on the agent of the
//! stream, pushes it into the repeater and awaits the push before it takes
//! the next event. The repeater therefore holds at most one pending push, and
//! every stream carries its events in publish order.
//!
//! The pump ends when its slot is gone, which the stop of the repeater brings
//! about. A value thrown into the repeater reaches the push the pump awaits,
//! and the pump stops the repeater with it, where the floating push of the
//! reference rejects the repeater with the same value. The pump never keeps
//! the agent across an await, because the agent keeps the pump.

use core::{
    future::poll_fn,
    task::{
        Context as TaskContext,
        Poll, //
    }, //
};

use aosabi_execute::{
    promise::{
        Agent,
        Promise, //
    },
    repeater::{
        PushHandle,
        StopHandle, //
    },
    value::{
        Completion,
        Value, //
    }, //
};
use aosabi_kernel::publish::{
    Delivered,
    Lock,
    SlotId,
    Take, //
};

use super::{
    PubSub,
    SharedPayload, //
};
use crate::{
    carrier::note_revision,
    context::Context, //
};

/// Spawns the pump of `slot` into the repeater of `push` and `stop` on `agent`.
///
/// The promise of the pump is never read, because the executor of the
/// reference answers `undefined` and keeps no handle on its listener.
pub(super) fn spawn(
    agent: &Agent,
    pub_sub: &'static PubSub,
    slot: SlotId,
    push: PushHandle,
    stop: StopHandle,
    context: Option<Context>,
) {
    let pump = async move {
        while let Some(event) = poll_fn(|task| take(pub_sub, slot, task)).await {
            if let Some(context) = &context {
                note_revision(context, event.revision);
            }
            let value = event.payload.to_value();
            drop(event);
            let Some(pushed) = push_value(&push, value) else {
                break;
            };
            if let Err(error) = pushed.await {
                stop_with(&stop, error);
                break;
            }
        }
        Completion::Normal(Value::Undefined)
    };
    let _ = agent.spawn(Box::pin(pump));
}

/// Takes the oldest event of `slot`, keeping the waker of `task` in the slot when it is empty.
///
/// `Ready(None)` answers a slot that is gone.
fn take(
    pub_sub: &PubSub,
    slot: SlotId,
    task: &mut TaskContext<'_>,
) -> Poll<Option<Delivered<SharedPayload>>> {
    match pub_sub.hub.with(|hub| hub.take(slot, Some(task.waker()))) {
        Take::Ready(event) => Poll::Ready(Some(event)),
        Take::Empty => Poll::Pending,
        Take::Gone => Poll::Ready(None),
    }
}

/// Pushes `value` and returns the promise of the await of its answer.
///
/// `None` answers a push with no agent to run on and a push that overflows,
/// which a pump never meets, because it holds at most one pending push.
fn push_value(push: &PushHandle, value: Value) -> Option<Promise> {
    let agent = Agent::current()?;
    let pushed = push.push(&agent, value).ok()?;
    Some(pushed.awaited(&agent))
}

/// Stops the repeater of `stop` with `error`, on the agent of the pump.
fn stop_with(stop: &StopHandle, error: Value) {
    if let Some(agent) = Agent::current() {
        stop.stop(&agent, Some(error));
    }
}
