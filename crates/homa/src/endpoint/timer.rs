// SPDX-License-Identifier: MIT
/*
 * homa endpoint timer: the deadlines of an endpoint, peer death and patient
 * RPCs.
 *
 * The rules are those of retransmission, cleanup and peer death in the Homa
 * design note, first version, homa/design-v1.md section 2.4, lines 504 to 565,
 * with the patient RPCs of its section 1.5, lines 282 to 320, and the
 * durations as values of a profile rather than ticks of the second version,
 * homa/design.md section 3.2, lines 479 to 508.
 */

//! The timers of the endpoint.
//!
//! The engine keeps no tick of its own: it computes the earliest deadline of
//! its RPCs, which [`Endpoint::next_timeout`] returns, and the caller runs
//! [`Endpoint::handle_timeout`] at that time or later. Where the rate pacer
//! holds a DATA packet, the deadline is no later than the time it lets the
//! packet go, and the caller's next call of `poll_transmit` sends it.
//!
//! - A receiver that expects data of a message and has heard nothing of the
//!   RPC for `resend_after` asks for the first missing range with a RESEND,
//!   and again every `resend_interval` while nothing arrives. A client whose
//!   request has left and whose response has not begun asks the same way,
//!   after `patient_after` in place of `resend_after` where the request is
//!   patient. A message that holds every byte it was granted runs no timer,
//!   since it waits on the grant scheduler of its downlink, which runs again
//!   whenever a message before it arrives or ends.
//! - A server that sent its whole response and has no acknowledgement after
//!   `need_ack_after` asks with `NEED_ACK`, and again every `need_ack_after`.
//! - One RESEND or `NEED_ACK` is outstanding a peer at a time: a peer that was
//!   asked is asked again only after `resend_interval`, and each pass of the
//!   timers starts at the next slot, which rotates the questions among the
//!   RPCs of a peer.
//! - A peer that left `timeout_resends` of them unanswered and was silent for
//!   `rpc_timeout` is dead: every client RPC with it fails with
//!   [`Failure::PeerTimeout`](crate::Failure::PeerTimeout), every server RPC
//!   is dropped, and [`Event::PeerGone`](crate::Event::PeerGone) reports it.
//!   Any packet from the peer answers, and a BUSY restarts the timer of its
//!   RPC.

use core::hash::Hash;

use crate::{
    endpoint::{
        Core,
        Cx,
        Due,
        Endpoint,
        ASKED,
        C_IN,
        NEED_ACK,
        RESEND,
        RESENT,
        SENT,
        S_IN,
        S_OUT,
        UP, //
    },
    store::to_usize,
    wire::Form,
    ClockBackwards,
    Gone,
    Nanos,
    Peer,
    PeerPort,
    Slot,
    Store, //
};

#[cfg(feature = "client")]
use crate::endpoint::{
    C_OUT,
    PATIENT, //
};

impl<P, S, W> Endpoint<P, S, W>
where
    P: Copy + Eq + Hash + PeerPort,
    S: Store<P>,
    W: Form,
{
    /// Returns when [`Endpoint::handle_timeout`] must run next.
    pub fn next_timeout(&self) -> Option<Nanos> {
        if let Due::Known(cached) = self.core.deadline.get() {
            return cached;
        }
        let view = self.store.view();
        let mut earliest: Option<Nanos> = paced(&self.core);
        for slot in view.slots {
            let Some(peer) = view.peers.get(to_usize(slot.peer)) else {
                continue;
            };
            if let Some(due) = due(&self.core, peer, slot) {
                earliest = Some(earliest.map_or(due, |e| e.min(due)));
            }
        }
        self.core.deadline.set(Due::Known(earliest));
        earliest
    }

    /// Runs the timers due at `now`.
    ///
    /// # Errors
    ///
    /// Returns [`ClockBackwards`] when `now` is earlier than a time passed
    /// before, and runs no timer.
    pub fn handle_timeout(&mut self, now: Nanos) -> Result<(), ClockBackwards> {
        if now < self.core.now {
            return Err(ClockBackwards);
        }
        self.core.now = now;
        if self.next_timeout().is_none_or(|due| due > now) {
            return Ok(());
        }
        let mut cx = self.cx();
        let count = u32::try_from(cx.parts.slots.len()).unwrap_or(u32::MAX);
        if count == 0 {
            return Ok(());
        }
        let from = cx.core.scan_from % count;
        cx.core.scan_from = from.wrapping_add(1) % count;
        for step in 0..count {
            let index = (from + step) % count;
            cx.expire(index, now);
        }
        cx.dirty();
        Ok(())
    }
}

/// Returns when the rate pacer lets the DATA packet it held go, which the
/// caller's next call of `poll_transmit` sends.
#[cfg(feature = "pacer-rate")]
fn paced(core: &Core) -> Option<Nanos> {
    core.rate.wake
}

/// Returns `None`, since a build without the feature `pacer-rate` holds no
/// packet.
#[cfg(not(feature = "pacer-rate"))]
fn paced(_core: &Core) -> Option<Nanos> {
    None
}

/// Returns when the timer of `slot` with peer `peer` is due, or `None` where
/// it runs no timer.
fn due<P>(core: &Core, peer: &Peer<P>, slot: &Slot) -> Option<Nanos> {
    let timers = &core.profile.timers;
    let resent = slot.flags & RESENT != 0;
    let wait = match slot.state {
        #[cfg(feature = "client")]
        C_OUT => {
            let transmittable = slot.out_next < slot.out_len.min(slot.out_granted)
                || (slot.out_len == 0 && slot.flags & SENT == 0);
            if transmittable {
                return None;
            }
            if resent {
                timers.resend_interval
            } else if slot.flags & (PATIENT | SENT) == PATIENT | SENT {
                timers.patient_after
            } else {
                timers.resend_after
            }
        }
        C_IN | S_IN => {
            if slot.in_recv >= slot.in_granted.min(slot.in_len) {
                return None;
            }
            if resent {
                timers.resend_interval
            } else {
                timers.resend_after
            }
        }
        S_OUT if slot.flags & SENT != 0 => timers.need_ack_after,
        _ => return None,
    };
    let mut at = slot.stamp.saturating_add(wait);
    if peer.flags & UP == 0 {
        return None;
    }
    if peer.tries >= timers.timeout_resends {
        at = at.max(peer.stamp.saturating_add(timers.rpc_timeout));
    } else if peer.flags & ASKED != 0 {
        at = at.max(peer.resend_at.saturating_add(timers.resend_interval));
    }
    Some(at)
}

impl<P: Copy + Eq + Hash + PeerPort> Cx<'_, '_, P> {
    /// Runs the timer of slot `index` where it is due at `now`: a RESEND, a
    /// `NEED_ACK`, or the death of its peer.
    fn expire(&mut self, index: u32, now: Nanos) {
        let Some(slot) = self.parts.slots.get(to_usize(index)).copied() else {
            return;
        };
        let Some(peer) = self.parts.peers.get(to_usize(slot.peer)) else {
            return;
        };
        let Some(at) = due(self.core, peer, &slot) else {
            return;
        };
        if at > now {
            return;
        }
        if peer.tries >= self.core.profile.timers.timeout_resends {
            self.end_peer(slot.peer, Gone::Timeout);
            return;
        }
        let bit = if slot.state == S_OUT {
            NEED_ACK
        } else {
            RESEND
        };
        self.owe_slot(index, bit);
        if let Some(entry) = self.parts.slots.get_mut(to_usize(index)) {
            entry.stamp = now;
            entry.flags |= RESENT;
        }
        if let Some(entry) = self.parts.peers.get_mut(to_usize(slot.peer)) {
            entry.resend_at = now;
            entry.flags |= ASKED;
            entry.tries = entry.tries.saturating_add(1);
        }
    }
}
