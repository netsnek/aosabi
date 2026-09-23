// SPDX-License-Identifier: MIT
/*
 * homa grant: the grant scheduler of one downlink.
 *
 * The scheduler is that of the Homa design note, second version,
 * homa/design.md sections 2.4 and 3.6, lines 335 to 350 and 607 to 620, one
 * per downlink over the grant heap in SRPT order, with the grants batched at
 * half a window and the FIFO share by count of its first version,
 * homa/design-v1.md sections 1.5 and 2.4, lines 282 to 320 and 504 to 565.
 * The window, the level of a grant and the FIFO share by interval are
 * HomaModule's at commit 50d65305, homa_grant.c lines 282 to 303, 792 to 864
 * and 1047 to 1200, at
 * https://github.com/PlatformLab/HomaModule/tree/50d65305aeef9b6ae5cf193a808c7e95f9b1dcb3,
 * read.
 */

//! The grant scheduler.
//!
//! A receiver grants to the messages it receives on one downlink: every peer
//! is its own downlink under [󰀀](crate::Scope::PerPeer), and every peer
//! shares one under [󰀁](crate::Scope::Shared). The messages it grants to at
//! once are its active messages: the ones with the fewest bytes not yet
//! received, ties to the older, one a sender, up to `max_overcommit` of them,
//! since a sender sends its shortest message first anyway. It grants each
//! active message in that order up to the bytes it received and its
//! [`window`], never past the message, once the bytes granted and not
//! received fall to half of the window where the profile batches its grants
//! and below the window where it grants as data arrives, and only while the
//! bytes the downlink expects, unscheduled or granted and not received, stay
//! within `max_incoming`.
//!
//! **The FIFO share.** On a point to point profile every `fifo_every`th grant
//! of a downlink goes to the oldest message rather than the active one, which
//! is the share by count of the first version. On a shared downlink whose
//! profile sets `fifo_permille` and no count, the oldest message is granted
//! `fifo_grant_increment` bytes more every interval in which those bytes take
//! `fifo_permille` of the link rate, as `HomaModule` times it, within
//! `max_incoming` like every other grant.
//!
//! **Levels.** The grant to the active message of rank `r` among `n` names
//! the level [`prio::scheduled`](crate::prio::scheduled) gives, which is
//! `homa_grant_priority`, a FIFO grant the level its message's rank among the
//! active messages gives, and a grant on a form without levels the highest
//! level.
//!
//! The scheduler runs whenever a DATA packet or a `START_MSG` of the downlink
//! arrives, and whenever an incoming message of the downlink ends before it
//! is whole, through an abort, a release, `RPC_UNKNOWN` or the end of its
//! peer, since a message that holds every byte it was granted runs no timer
//! and waits on the scheduler alone.

use core::hash::Hash;

use crate::{
    endpoint::{
        Cx,
        GRANT, //
    },
    heap::Item,
    pacer,
    store::to_usize,
    PeerPort,
    Priority,
    Window, //
};

/// The most messages a downlink grants to at once, `HOMA_MAX_GRANTS`.
pub const MAX_OVERCOMMIT: usize = 8;

/// Returns the window of one message, fixed or `max_incoming` over one more
/// than `active`, the messages the downlink grants to at once.
///
/// The dynamic window is `HomaModule`'s: fewer active messages give each a
/// larger window, and `max_incoming` over one more than them keeps room for a
/// new message.
#[must_use]
pub const fn window(window: Window, max_incoming: u32, active: u32) -> u32 {
    match window {
        Window::Fixed(bytes) => bytes,
        Window::Dynamic => max_incoming / active.saturating_add(1),
    }
}

/// Grants to the messages of the downlink of peer `peer` that take a grant
/// now.
pub(crate) fn schedule<P: Copy + Eq + Hash + PeerPort>(cx: &mut Cx<'_, '_, P>, peer: u32) {
    if cx.core.profile.max_overcommit > 1 {
        schedule_many(cx, peer);
    } else {
        schedule_one(cx, peer);
    }
    fifo_interval(cx, peer);
}

/// Returns whether slot `index` receives on the downlink of peer `peer`.
fn on_link<P: Copy + Eq + Hash + PeerPort>(cx: &Cx<'_, '_, P>, index: u32, peer: u32) -> bool {
    cx.shared_scope()
        || cx
            .parts
            .slots
            .get(to_usize(index))
            .is_some_and(|slot| slot.peer == peer)
}

/// Grants to the one active message of the downlink of peer `peer`, or to
/// the oldest message on a turn of the FIFO share by count.
fn schedule_one<P: Copy + Eq + Hash + PeerPort>(cx: &mut Cx<'_, '_, P>, peer: u32) {
    let len = to_usize(cx.core.grant_len);
    let mut srpt: Option<Item> = None;
    let mut oldest: Option<Item> = None;
    for i in 0..len {
        let Some(item) = cx.parts.grant_heap.get(i).copied() else {
            break;
        };
        if !on_link(cx, item.slot(), peer) {
            continue;
        }
        if srpt.is_none_or(|best| item.precedes(best)) {
            srpt = Some(item);
        }
        if oldest.is_none_or(|old| item.age() < old.age()) {
            oldest = Some(item);
        }
    }
    let (Some(srpt), Some(oldest)) = (srpt, oldest) else {
        return;
    };
    let profile = cx.core.profile;
    let window = window(profile.window, profile.max_incoming, 1);
    let turn = pacer::fifo_turn(counter(cx, peer), profile.fifo_every);
    let order = if turn { [oldest, srpt] } else { [srpt, oldest] };
    for (i, item) in order.iter().enumerate() {
        let fifo = turn && i == 0;
        let rank = u8::from(fifo && item.slot() != srpt.slot());
        let level = level_of(cx, rank, 1);
        if grant(cx, item.slot(), window, level, (rank, 1, fifo)) {
            if fifo || !turn {
                advance(cx, peer);
            }
            return;
        }
        if !turn {
            return;
        }
    }
}

/// Grants to the active messages of the downlink of peer `peer`: up to
/// `max_overcommit` of them, one a sender, in SRPT order.
fn schedule_many<P: Copy + Eq + Hash + PeerPort>(cx: &mut Cx<'_, '_, P>, peer: u32) {
    let limit = usize::from(cx.core.profile.max_overcommit).clamp(1, MAX_OVERCOMMIT);
    let mut active = [(Item::VACANT, 0u32); MAX_OVERCOMMIT];
    let mut count = 0usize;
    let len = to_usize(cx.core.grant_len);
    for i in 0..len {
        let Some(item) = cx.parts.grant_heap.get(i).copied() else {
            break;
        };
        let Some(sender) = cx.parts.slots.get(to_usize(item.slot())).map(|s| s.peer) else {
            continue;
        };
        if !on_link(cx, item.slot(), peer) {
            continue;
        }
        let taken = active.get(..count).unwrap_or(&[]);
        let place = match taken.iter().position(|&(_, other)| other == sender) {
            Some(at) if item.precedes(active[at].0) => at,
            None if count < limit => {
                count += 1;
                count - 1
            }
            None if item.precedes(active[count - 1].0) => count - 1,
            Some(_) | None => continue,
        };
        active[place] = (item, sender);
        let mut at = place;
        while at > 0 && active[at].0.precedes(active[at - 1].0) {
            active.swap(at, at - 1);
            at -= 1;
        }
        while at + 1 < count && active[at + 1].0.precedes(active[at].0) {
            active.swap(at, at + 1);
            at += 1;
        }
    }
    let n = u8::try_from(count).unwrap_or(u8::MAX);
    let profile = cx.core.profile;
    let window = window(profile.window, profile.max_incoming, u32::from(n));
    for (rank, &(item, _)) in active.iter().take(count).enumerate() {
        let rank = u8::try_from(rank).unwrap_or(u8::MAX);
        let level = level_of(cx, rank, n);
        grant(cx, item.slot(), window, level, (rank, n, false));
    }
}

/// Gives the FIFO share by interval of a downlink: where its interval has
/// passed, grants the oldest message of the downlink of peer `peer`
/// `fifo_grant_increment` bytes more.
#[cfg(feature = "wire-full")]
fn fifo_interval<P: Copy + Eq + Hash + PeerPort>(cx: &mut Cx<'_, '_, P>, peer: u32) {
    let profile = cx.core.profile;
    let increment = profile.fifo_grant_increment;
    if profile.fifo_every != 0 || profile.fifo_permille == 0 || increment == 0 {
        return;
    }
    let now = cx.core.now;
    if now < cx.core.shared.fifo_at {
        return;
    }
    cx.core.shared.fifo_at = now.saturating_add(fifo_gap(
        increment,
        profile.fifo_permille,
        cx.core.link_bits_per_s,
    ));
    let len = to_usize(cx.core.grant_len);
    let mut oldest: Option<Item> = None;
    for i in 0..len {
        let Some(item) = cx.parts.grant_heap.get(i).copied() else {
            break;
        };
        if on_link(cx, item.slot(), peer) && oldest.is_none_or(|old| item.age() < old.age()) {
            oldest = Some(item);
        }
    }
    let Some(oldest) = oldest else {
        return;
    };
    let (rank, active) = rank_of(cx, oldest, peer);
    let level = level_of(cx, rank, active);
    let Some(slot) = cx.parts.slots.get(to_usize(oldest.slot())).copied() else {
        return;
    };
    let target = slot.in_granted.saturating_add(increment).min(slot.in_len);
    give(cx, oldest.slot(), target, level, (rank, active, true), 0);
}

/// Gives no FIFO share by interval in a build without the full form, whose
/// profiles take the share by count.
#[cfg(not(feature = "wire-full"))]
fn fifo_interval<P: Copy + Eq + Hash + PeerPort>(_cx: &mut Cx<'_, '_, P>, _peer: u32) {}

/// Returns the nanoseconds in which `increment` bytes take `permille` of a
/// link of `bits_per_s`, the interval of the FIFO share.
#[cfg(feature = "wire-full")]
fn fifo_gap(increment: u32, permille: u16, bits_per_s: u64) -> crate::Nanos {
    let share = u128::from(bits_per_s) * u128::from(permille);
    if share == 0 {
        return crate::Nanos::MAX;
    }
    let bits = u128::from(increment) * 8 * 1_000 * 1_000_000_000;
    crate::Nanos::try_from(bits.div_ceil(share)).unwrap_or(crate::Nanos::MAX)
}

/// Returns the rank of `item` among the messages the downlink of peer `peer`
/// grants to at once, and their count, as `schedule_many` chooses them.
#[cfg(feature = "wire-full")]
fn rank_of<P: Copy + Eq + Hash + PeerPort>(cx: &Cx<'_, '_, P>, item: Item, peer: u32) -> (u8, u8) {
    let limit = usize::from(cx.core.profile.max_overcommit).clamp(1, MAX_OVERCOMMIT);
    let mut best = [(Item::VACANT, 0u32); MAX_OVERCOMMIT];
    let mut count = 0usize;
    for i in 0..to_usize(cx.core.grant_len) {
        let Some(other) = cx.parts.grant_heap.get(i).copied() else {
            break;
        };
        let Some(sender) = cx.parts.slots.get(to_usize(other.slot())).map(|s| s.peer) else {
            continue;
        };
        if !on_link(cx, other.slot(), peer) {
            continue;
        }
        match best.iter().take(count).position(|&(_, p)| p == sender) {
            Some(at) if other.precedes(best[at].0) => best[at] = (other, sender),
            Some(_) => {}
            None if count < limit => {
                best[count] = (other, sender);
                count += 1;
            }
            None => {
                if let Some(worst) = (0..count).max_by(|&a, &b| {
                    if best[a].0.precedes(best[b].0) {
                        core::cmp::Ordering::Less
                    } else {
                        core::cmp::Ordering::Greater
                    }
                }) {
                    if other.precedes(best[worst].0) {
                        best[worst] = (other, sender);
                    }
                }
            }
        }
    }
    let ahead = best
        .iter()
        .take(count)
        .filter(|&&(other, _)| other.slot() != item.slot() && other.precedes(item))
        .count();
    let n = u8::try_from(count).unwrap_or(u8::MAX);
    (u8::try_from(ahead).unwrap_or(u8::MAX), n.max(1))
}

/// Returns the level of the grant to the message of `rank` among `active`,
/// as `homa_grant_priority` gives it, or the highest level on a form
/// without levels.
#[cfg(feature = "prio")]
fn level_of<P: Copy + Eq + Hash + PeerPort>(cx: &Cx<'_, '_, P>, rank: u8, active: u8) -> u8 {
    if !cx.core.full_form {
        return Priority::HIGHEST.level();
    }
    crate::prio::scheduled(cx.core.own.levels, rank, active).level()
}

/// Returns the highest level, the one level of a build without the feature
/// `prio`.
#[cfg(not(feature = "prio"))]
fn level_of<P: Copy + Eq + Hash + PeerPort>(_cx: &Cx<'_, '_, P>, _rank: u8, _active: u8) -> u8 {
    Priority::HIGHEST.level()
}

/// Returns the count of grants given on the downlink of peer `peer`.
fn counter<P: Copy + Eq + Hash + PeerPort>(cx: &Cx<'_, '_, P>, peer: u32) -> u8 {
    if cx.shared_scope() {
        cx.core.shared.grants
    } else {
        cx.parts
            .peers
            .get(to_usize(peer))
            .map_or(0, |entry| entry.grants)
    }
}

/// Counts one grant given on the downlink of peer `peer`.
fn advance<P: Copy + Eq + Hash + PeerPort>(cx: &mut Cx<'_, '_, P>, peer: u32) {
    let every = cx.core.profile.fifo_every;
    if cx.shared_scope() {
        cx.core.shared.grants = pacer::advance(cx.core.shared.grants, every);
    } else if let Some(entry) = cx.parts.peers.get_mut(to_usize(peer)) {
        entry.grants = pacer::advance(entry.grants, every);
    }
}

/// Grants more of the incoming message of slot `index` at `level` where it
/// takes a grant now, and returns whether it did.
///
/// `decision` is the rank of the message among the messages granted at
/// once, their count and whether the FIFO share chose it, which the
/// simulators trace.
fn grant<P: Copy + Eq + Hash + PeerPort>(
    cx: &mut Cx<'_, '_, P>,
    index: u32,
    window: u32,
    level: u8,
    decision: (u8, u8, bool),
) -> bool {
    let Some(slot) = cx.parts.slots.get(to_usize(index)).copied() else {
        return false;
    };
    if slot.in_granted >= slot.in_len {
        return false;
    }
    let received = cx.received(index);
    let outstanding = slot.in_granted.saturating_sub(received);
    let threshold = if cx.core.profile.batch_grants {
        window / 2
    } else {
        window.saturating_sub(1)
    };
    if outstanding > threshold {
        return false;
    }
    let target = received.saturating_add(window).min(slot.in_len);
    give(cx, index, target, level, decision, window)
}

/// Raises the granted offset of the incoming message of slot `index` toward
/// `target` at `level`, within the room `max_incoming` leaves the downlink,
/// owes the GRANT, and returns whether it gave any byte.
fn give<P: Copy + Eq + Hash + PeerPort>(
    cx: &mut Cx<'_, '_, P>,
    index: u32,
    target: u32,
    level: u8,
    decision: (u8, u8, bool),
    window: u32,
) -> bool {
    let Some(slot) = cx.parts.slots.get(to_usize(index)).copied() else {
        return false;
    };
    if target <= slot.in_granted {
        return false;
    }
    let expected = cx.expected_mut(slot.peer).map_or(0, |expected| *expected);
    let room = cx.core.profile.max_incoming.saturating_sub(expected);
    let bytes = (target - slot.in_granted).min(room);
    if bytes == 0 {
        return false;
    }
    let before = cx.owed(index);
    if let Some(entry) = cx.parts.slots.get_mut(to_usize(index)) {
        entry.in_granted = entry.in_granted.saturating_add(bytes);
        entry.levels = (entry.levels & !0b0000_0111) | (level & 0b0000_0111);
    }
    cx.settle(index, before);
    cx.owe_slot(index, GRANT);
    record(cx, index, level, decision, window);
    let granted_all = cx
        .parts
        .slots
        .get(to_usize(index))
        .is_some_and(|entry| entry.in_granted >= entry.in_len);
    if granted_all {
        cx.heap_out(index);
    }
    true
}

/// Records a decision of the scheduler for the simulators.
#[cfg(feature = "sim")]
fn record<P: Copy + Eq + Hash + PeerPort>(
    cx: &mut Cx<'_, '_, P>,
    index: u32,
    level: u8,
    decision: (u8, u8, bool),
    window: u32,
) {
    let Some(slot) = cx.parts.slots.get(to_usize(index)).copied() else {
        return;
    };
    let (rank, active, fifo) = decision;
    cx.core.decisions.push(crate::GrantDecision {
        id: crate::RpcId::new(slot.id),
        offset: slot.in_granted,
        level,
        rank,
        active,
        window,
        fifo,
    });
}

/// Records nothing in a build without the feature `sim`.
#[cfg(not(feature = "sim"))]
fn record<P: Copy + Eq + Hash + PeerPort>(
    _cx: &mut Cx<'_, '_, P>,
    _index: u32,
    _level: u8,
    _decision: (u8, u8, bool),
    _window: u32,
) {
}
