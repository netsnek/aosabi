// SPDX-License-Identifier: MIT
/*
 * aosabi-kernel publish hub: the fan out of one publish to every slot of its
 * topic, with the revision and the coalescing per slot.
 *
 * No analogue in the reference, whose listeners are those of an EventTarget. A
 * slot listened or unlistened during a dispatch is not reached by it, which is
 * the rule of
 * https://dom.spec.whatwg.org/#concept-event-listener-inner-invoke, and a slow
 * slot keeps the latest value of its section, which is the coalescing of
 * ADR-0017. The hub needs alloc.
 */

//! The fan out of the publish primitive.
//!
//! A [`Hub`] holds a [`Revision`] and one slot per listener in listen order.
//! A slot listens on one topic, so it is one section of the state, and it
//! keeps the events published to that topic until its subscriber takes them.
//! The hub runs no task and wakes none itself: a publish hands the wakers of
//! the empty slots it reached to its caller, and [`publish`] over a [`Lock`]
//! wakes them once the lock is released.
//!
//! # The rules
//!
//! The rules carry the numbers of the plan's section KERNEL, and the push
//! model of the harness states them in the same words.
//!
//! 1. [`Hub::publish`] increments the revision once, walks the slots in listen
//!    order, and for each slot of the topic appends a clone of the payload
//!    with the new revision. A slot is one section, since it listens on one
//!    topic. The revision increments in this one place, once per changed
//!    section.
//! 2. A slot keeps at most `max(depth, 1)` pending events in publish order. At
//!    the bound, the newest pending event is replaced by the new one and
//!    `coalesced` goes up by one, so a slow subscriber loses an intermediate
//!    value of its section and never the latest one. A depth of 1 is exactly
//!    one latest value per section.
//! 3. A slot that was empty and holds a waker gives the waker to `wake`, and
//!    the caller wakes after it releases whatever lock guards the hub, which
//!    [`publish`] over a [`Lock`] does, so a waker that polls at once never
//!    deadlocks on the lock it was woken under. The slot holds no waker after
//!    that.
//! 4. A slot listened during a publish is not reached by it, and a slot
//!    unlistened during it is not reached either, which is the dispatch rule
//!    of the DOM. The hub meets it because a publish holds the one mutable
//!    borrow of the hub for its whole walk, so the slots it walks are the ones
//!    that exist when it starts, and a task it wakes listens or unlistens only
//!    after the walk.
//! 5. A publish to a topic nobody listens on does the revision increment and
//!    nothing else, so a service with no subscriber pays one lock per
//!    publish.
//!
//! [`Revision`]: super::Revision
//! [`Lock`]: super::Lock

use alloc::{
    collections::VecDeque,
    string::String,
    vec::Vec, //
};
use core::task::Waker;

use super::{
    Revision,
    lock::Lock, //
};

/// The id of a slot, counted from 0 in listen order.
///
/// The hub never hands out an id that a live slot holds, so an id names one
/// slot from its listen to its unlisten, and a take after the unlisten
/// answers [`Take::Gone`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SlotId(u32);

impl SlotId {
    /// Creates the id of the number `id`, as a caller outside the hub spells it.
    ///
    /// An id the hub did not hand out names no slot, and the hub answers it as
    /// a slot that is gone.
    #[must_use]
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    /// Returns the number of the id.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// An event a slot delivers: the payload and the revision of its publish.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Delivered<P> {
    /// The revision of the publish that carried the payload, which reaches no frame by rule 8 of [the module](super).
    pub revision: Revision,
    /// The payload of that publish.
    pub payload: P,
}

/// What one publish did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Published {
    /// The revision the publish took.
    pub revision: Revision,
    /// The count of slots of the topic, those that coalesced included.
    pub reached: u32,
    /// The count of slots at their bound, whose newest pending event the publish replaced.
    pub coalesced: u32,
}

/// What a take of a slot answers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Take<P> {
    /// The oldest pending event of the slot, which the take removes.
    Ready(Delivered<P>),
    /// No event is pending.
    Empty,
    /// No slot of the id exists.
    Gone,
}

/// The state of one slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlotState {
    /// The id of the slot.
    pub id: SlotId,
    /// The count of events the slot holds.
    pub pending: u16,
    /// The count of publishes that replaced the newest pending event of the slot, rule 2.
    pub coalesced: u32,
}

/// One listener: its topic, its bound, its pending events and its waker.
#[derive(Debug)]
struct Slot<P> {
    /// The id the listen answered.
    id: SlotId,
    /// The topic the slot listens on.
    topic: String,
    /// The bound of rule 2, `max(depth, 1)`.
    depth: u16,
    /// The pending events, oldest first.
    pending: VecDeque<Delivered<P>>,
    /// The count of publishes that replaced the newest pending event.
    coalesced: u32,
    /// The waker the last take of an empty slot kept.
    waker: Option<Waker>,
}

/// The fan out: a revision, and one slot per listener in listen order.
#[derive(Debug)]
pub struct Hub<P> {
    /// The revision of the last publish, 0 before the first.
    revision: Revision,
    /// The id the next listen tries first.
    next: u32,
    /// Whether the ids have wrapped past `u32::MAX`, after which they are no
    /// longer ascending in listen order.
    wrapped: bool,
    /// The slots in listen order.
    slots: Vec<Slot<P>>,
}

impl<P> Hub<P> {
    /// The bytes one slot takes in the list of the hub.
    ///
    /// The bytes of its topic and of its pending events, each a
    /// [`Delivered`], lie outside the list and are not counted, so a caller
    /// that prints what a slot costs, the devtool among them, adds those.
    pub const SLOT_SIZE: usize = core::mem::size_of::<Slot<P>>();

    /// Creates a hub at revision 0 with no slot.
    ///
    /// The function is `const`, so a hub behind a lock can sit in a `static`.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            revision: Revision(0),
            next: 0,
            wrapped: false,
            slots: Vec::new(),
        }
    }

    /// Listens on `topic` with the bound `depth`, `addEventListener(topic, ...)`.
    ///
    /// The slot is appended after every slot that exists, so a publish walks
    /// it last, and it keeps at most `max(depth, 1)` pending events (rule 2).
    pub fn listen(&mut self, topic: &str, depth: u16) -> SlotId {
        let id = self.fresh_id();
        self.slots.push(Slot {
            id,
            topic: String::from(topic),
            depth: depth.max(1),
            pending: VecDeque::new(),
            coalesced: 0,
            waker: None,
        });
        id
    }

    /// Removes the slot `slot`, `removeEventListener`.
    ///
    /// What the slot still holds is dropped, and the waker it kept is
    /// answered, so the caller wakes the task that waits on the slot once it
    /// has released the lock of the hub. `None` answers a slot that is gone or
    /// that kept no waker.
    pub fn unlisten(&mut self, slot: SlotId) -> Option<Waker> {
        let index = self.position(slot)?;
        self.slots.remove(index).waker
    }

    /// Returns the oldest pending event of `slot`, keeping `waker` for the next publish when empty.
    ///
    /// A take of an empty slot with a `waker` keeps it in place of the waker
    /// the slot held, and one with none leaves the held waker as it is. A take
    /// that answers an event leaves the held waker as it is too.
    pub fn take(&mut self, slot: SlotId, waker: Option<&Waker>) -> Take<P> {
        let Some(slot) = self.slot_mut(slot) else {
            return Take::Gone;
        };
        if let Some(event) = slot.pending.pop_front() {
            return Take::Ready(event);
        }
        if let Some(waker) = waker
            && !slot
                .waker
                .as_ref()
                .is_some_and(|held| held.will_wake(waker))
        {
            slot.waker = Some(waker.clone());
        }
        Take::Empty
    }

    /// Returns the revision of the last publish, 0 before the first.
    #[must_use]
    pub fn revision(&self) -> Revision {
        self.revision
    }

    /// Returns the state of `slot`, or `None` for a slot that is gone.
    #[must_use]
    pub fn state(&self, slot: SlotId) -> Option<SlotState> {
        let slot = self.slots.get(self.position(slot)?)?;
        Some(SlotState {
            id: slot.id,
            // A slot holds at most its depth, a `u16`.
            pending: u16::try_from(slot.pending.len()).unwrap_or(u16::MAX),
            coalesced: slot.coalesced,
        })
    }

    /// Returns the count of slots.
    #[must_use]
    pub fn slot_count(&self) -> usize {
        self.slots.len()
    }

    /// Returns an id no live slot holds, the next in listen order while the ids have not wrapped.
    fn fresh_id(&mut self) -> SlotId {
        loop {
            let id = SlotId(self.next);
            self.next = self.next.wrapping_add(1);
            self.wrapped |= self.next == 0;
            if !self.wrapped || self.position(id).is_none() {
                return id;
            }
        }
    }

    /// Returns the index of `slot` in the slots, by a binary search while the ids ascend.
    fn position(&self, slot: SlotId) -> Option<usize> {
        if self.wrapped {
            return self.slots.iter().position(|one| one.id == slot);
        }
        self.slots
            .binary_search_by_key(&slot.0, |one| one.id.0)
            .ok()
    }

    /// Returns the slot `slot`, or `None` for a slot that is gone.
    fn slot_mut(&mut self, slot: SlotId) -> Option<&mut Slot<P>> {
        let index = self.position(slot)?;
        self.slots.get_mut(index)
    }
}

impl<P: Clone> Hub<P> {
    /// Publishes `payload` to every slot of `topic`, one change of one section.
    ///
    /// The revision goes up by one and every slot of `topic` takes a clone of
    /// the payload with it (rule 1), a slot at its bound replaces its newest
    /// pending event (rule 2), and the waker of each slot that was empty is
    /// pushed onto `wake` in listen order for the caller to wake once it has
    /// released the lock of the hub (rule 3). A topic with no slot costs the
    /// increment alone (rule 5).
    pub fn publish(&mut self, topic: &str, payload: &P, wake: &mut Vec<Waker>) -> Published {
        self.revision = Revision(self.revision.0.wrapping_add(1));
        let revision = self.revision;
        let mut reached = 0_u32;
        let mut coalesced = 0_u32;
        for slot in self.slots.iter_mut().filter(|one| one.topic == topic) {
            reached = reached.saturating_add(1);
            let event = Delivered {
                revision,
                payload: payload.clone(),
            };
            if slot.pending.len() >= usize::from(slot.depth) {
                if let Some(newest) = slot.pending.back_mut() {
                    *newest = event;
                }
                slot.coalesced = slot.coalesced.saturating_add(1);
                coalesced = coalesced.saturating_add(1);
                continue;
            }
            if slot.pending.is_empty()
                && let Some(waker) = slot.waker.take()
            {
                wake.push(waker);
            }
            slot.pending.push_back(event);
        }
        Published {
            revision,
            reached,
            coalesced,
        }
    }
}

impl<P> Default for Hub<P> {
    /// Returns [`Hub::new`].
    fn default() -> Self {
        Self::new()
    }
}

/// Publishes `payload` to `topic` under `lock` and wakes every waker after the lock is released.
///
/// This is the publish a service calls from any part of it, and it is rule 3
/// in code: [`Hub::publish`] runs inside [`Lock::with`], and the wakers it
/// collects are woken after `with` has returned.
pub fn publish<P: Clone, L: Lock<Hub<P>>>(lock: &L, topic: &str, payload: &P) -> Published {
    let mut wake = Vec::new();
    let published = lock.with(|hub| hub.publish(topic, payload, &mut wake));
    for waker in wake {
        waker.wake();
    }
    published
}
