// SPDX-License-Identifier: MIT
//! An SPSC broadcast channel.
//!
//! - The value can only be a `usize`.
//! - The consumer is only notified if the value is different.
//! - The value `0` is reserved for closed.

use core::task;

use super::shared::{Number, Share, WakerSlot};

type Value = usize;

pub(crate) const CLOSED: usize = 0;

pub(crate) fn channel(initial: Value) -> (Sender, Receiver) {
    debug_assert!(
        initial != CLOSED,
        "watch::channel initial state of 0 is reserved"
    );

    let shared = Share::new(Shared {
        value: Number::new(initial),
        waker: WakerSlot::new(),
    });

    (
        Sender {
            shared: shared.clone(),
        },
        Receiver { shared },
    )
}

pub(crate) struct Sender {
    shared: Share<Shared>,
}

pub(crate) struct Receiver {
    shared: Share<Shared>,
}

struct Shared {
    value: Number,
    waker: WakerSlot,
}

impl Sender {
    pub(crate) fn send(&mut self, value: Value) {
        if self.shared.value.swap(value) != value {
            self.shared.waker.wake();
        }
    }
}

impl Drop for Sender {
    fn drop(&mut self) {
        self.send(CLOSED);
    }
}

impl Receiver {
    pub(crate) fn load(&mut self, cx: &mut task::Context<'_>) -> Value {
        self.shared.waker.register(cx.waker());
        self.shared.value.load()
    }

    pub(crate) fn peek(&self) -> Value {
        self.shared.value.peek()
    }
}
