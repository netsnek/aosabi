// SPDX-License-Identifier: MIT
/*
 * graphtp-wasm host: the state of the module and the safe calls behind each
 * export, over plain integers.
 *
 * No analogue in the reference. The calls are those docs/graphtp-fetch.md
 * section 2.2 names, alloc, free, engine_new, link_up, link_down, push_rx,
 * poll_tx, poll_event, operation, pull, cancel, answer, handle_timeout and
 * next_timeout, with engine_free, abort and codec beside them.
 */

//! The state of the module and the calls behind the exports.
//!
//! The buffers, the links and the codec live in one cell, which a module
//! without threads holds as plain state. A link is named by the number
//! [`engine_new`] returns, from 1, and a call that names no live link answers
//! [`NO_ENGINE`]. A call that reads a buffer the module does not hold answers
//! [`NO_BUFFER`]. Every call that answers bytes puts them into the answer
//! buffer and returns their length, which the host reads at the address
//! [`answer`] returns.

#![forbid(unsafe_code)]

use std::cell::RefCell;

use graphtp_homa::client::{
    Link as Sight,
    Repeat, //
};

use crate::{
    buffer::Buffers,
    codec::{
        self,
        Codec, //
    },
    link::{
        Framing,
        Link,
        Settings, //
    },
};

/// The answer of a call that names no live link.
pub const NO_ENGINE: i32 = -1;

/// The answer of a call that names a buffer the module does not hold.
pub const NO_BUFFER: i32 = -2;

/// The answer of a call whose arguments the module refuses, or whose answer
/// the allocator refuses.
pub const REFUSED: i32 = -3;

/// The answer of `next_timeout` for a link with no timer.
pub const NO_TIMEOUT: i64 = -1;

/// The state of the module.
struct State {
    /// The buffers the host shares with the module.
    buffers: Buffers,
    /// The links, by their number less one, `None` for a freed one.
    links: Vec<Option<Link>>,
    /// The codec of the whole messages.
    codec: Codec,
}

thread_local! {
    /// The one state of the module.
    static STATE: RefCell<State> = const {
        RefCell::new(State {
            buffers: Buffers::new(),
            links: Vec::new(),
            codec: Codec::new(),
        })
    };
}

/// Runs `call` over the state, or returns `fallback` where the state is
/// taken, which a module without threads never meets.
fn with<R>(fallback: R, call: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|state| match state.try_borrow_mut() {
        Ok(mut state) => call(&mut state),
        Err(_) => fallback,
    })
}

/// Runs `call` over the link `engine` and the buffers, or returns
/// [`NO_ENGINE`].
fn with_link(engine: u32, call: impl FnOnce(&mut Link, &mut Buffers) -> i32) -> i32 {
    with(NO_ENGINE, |state| {
        let Some(index) = engine.checked_sub(1).and_then(|n| usize::try_from(n).ok()) else {
            return NO_ENGINE;
        };
        match state.links.get_mut(index) {
            Some(Some(link)) => call(link, &mut state.buffers),
            _ => NO_ENGINE,
        }
    })
}

/// Returns the address of a new buffer of `len` bytes the host writes into,
/// 0 where none is made.
#[must_use]
pub fn alloc(len: u32) -> u32 {
    with(0, |state| state.buffers.alloc(len))
}

/// Drops the buffer at `addr`.
pub fn free(addr: u32) {
    with((), |state| {
        state.buffers.free(addr);
    });
}

/// Returns the address of the answer buffer, which holds what the last call
/// answered.
#[must_use]
pub fn answer() -> u32 {
    with(0, |state| state.buffers.answer_addr())
}

/// Builds a link of the profile `profile` with its parameter `param`, the
/// framing `framing`, the longest message `max_message`, the bound
/// `max_body` on a decoded body, the sight `sight`, 0 where the server sees
/// the end of the client's run and 1 where it may not, and the randomness
/// `seed`, and returns its number, 0 where it cannot be built.
///
/// The numbers of the profiles are those of [`crate::link::profile_of`] and
/// of the framings those of [`Framing::from_u32`].
#[must_use]
pub fn engine_new(
    profile: u32,
    param: u32,
    framing: u32,
    max_message: u32,
    max_body: u32,
    sight: u32,
    seed: u64,
) -> u32 {
    let (Some(profile), Some(framing)) = (
        crate::link::profile_of(profile, param, max_message),
        Framing::from_u32(framing),
    ) else {
        return 0;
    };
    let settings = Settings {
        profile,
        framing,
        max_message,
        max_body: codec::widen(max_body),
        sight: if sight == 0 {
            Sight::Seen
        } else {
            Sight::Unseen
        },
        seed,
    };
    let Ok(link) = Link::new(&settings) else {
        return 0;
    };
    with(0, |state| {
        let free = state
            .links
            .iter_mut()
            .enumerate()
            .find(|(_, slot)| slot.is_none());
        if let Some((index, slot)) = free {
            *slot = Some(link);
            return u32::try_from(index + 1).unwrap_or(0);
        }
        state.links.push(Some(link));
        u32::try_from(state.links.len()).unwrap_or(0)
    })
}

/// Drops the link `engine`, whose number a later link may take.
pub fn engine_free(engine: u32) {
    with((), |state| {
        let index = engine.checked_sub(1).and_then(|n| usize::try_from(n).ok());
        if let Some(slot) = index.and_then(|index| state.links.get_mut(index)) {
            *slot = None;
        }
    });
}

/// Brings the link `engine` up, and answers 1 where it is up.
#[must_use]
pub fn link_up(engine: u32) -> i32 {
    with_link(engine, |link, _| i32::from(link.link_up()))
}

/// Takes the link `engine` down.
#[must_use]
pub fn link_down(engine: u32) -> i32 {
    with_link(engine, |link, _| {
        link.link_down();
        0
    })
}

/// Hands the link `engine` the `len` bytes at `addr` its carrier received at
/// `now`, and answers the packets they completed.
#[must_use]
pub fn push_rx(engine: u32, addr: u32, len: u32, now: u64) -> i32 {
    with_link(engine, |link, buffers| {
        let Some(bytes) = buffers.get(addr, len) else {
            return NO_BUFFER;
        };
        i32::try_from(link.push_rx(bytes, now)).unwrap_or(i32::MAX)
    })
}

/// Answers the bytes the carrier of the link `engine` sends at `now`, 0 where
/// none is ready.
#[must_use]
pub fn poll_tx(engine: u32, now: u64) -> i32 {
    with_link(engine, |link, buffers| {
        link.poll_tx(now, buffers.answer_mut());
        buffers.answer_len()
    })
}

/// Answers the next event record of the link `engine`, 0 where none waits.
#[must_use]
pub fn poll_event(engine: u32) -> i32 {
    with_link(engine, |link, buffers| {
        if link.poll_event(buffers.answer_mut()) {
            buffers.answer_len()
        } else {
            0
        }
    })
}

/// Asks the link `engine` for an OPERATION with the credential of
/// `credential_len` bytes at `credential`, the content of `content_len` bytes
/// at `content` and the mark `repeat`, 1 for repeatable, and answers its
/// token.
#[must_use]
pub fn operation(
    engine: u32,
    credential: u32,
    credential_len: u32,
    content: u32,
    content_len: u32,
    repeat: u32,
) -> i32 {
    with_link(engine, |link, buffers| {
        let (Some(credential), Some(content)) = (
            buffers.get(credential, credential_len),
            buffers.get(content, content_len),
        ) else {
            return NO_BUFFER;
        };
        let repeat = if repeat == 0 { Repeat::No } else { Repeat::Yes };
        token(link.operation(credential, content, repeat))
    })
}

/// Asks the link `engine` for one PULL.
#[must_use]
pub fn pull(engine: u32) -> i32 {
    with_link(engine, |link, _| {
        link.pull();
        0
    })
}

/// Sends a CANCEL of the stream `handle`, of every stream for 0, on the link
/// `engine`, and answers its token.
#[must_use]
pub fn cancel(engine: u32, handle: u32) -> i32 {
    with_link(engine, |link, _| match u16::try_from(handle) {
        Ok(handle) => token(link.cancel(handle)),
        Err(_) => REFUSED,
    })
}

/// Ends the exchange of the token `token` on the link `engine`, and answers
/// 1 where one was in flight or waiting.
#[must_use]
pub fn abort(engine: u32, token: u32) -> i32 {
    with_link(engine, |link, _| i32::from(link.abort(token)))
}

/// Runs the timers of the link `engine` at `now`.
#[must_use]
pub fn handle_timeout(engine: u32, now: u64) -> i32 {
    with_link(engine, |link, _| {
        link.handle_timeout(now);
        0
    })
}

/// Answers when the link `engine` next needs its timers run, in nanoseconds
/// of the host's clock, [`NO_TIMEOUT`] for no timer and [`NO_ENGINE`] for no
/// such link.
#[must_use]
pub fn next_timeout(engine: u32) -> i64 {
    with(i64::from(NO_ENGINE), |state| {
        let index = engine.checked_sub(1).and_then(|n| usize::try_from(n).ok());
        match index.and_then(|index| state.links.get(index)) {
            Some(Some(link)) => link
                .next_timeout()
                .map_or(NO_TIMEOUT, |due| i64::try_from(due).unwrap_or(i64::MAX)),
            _ => i64::from(NO_ENGINE),
        }
    })
}

/// Runs the operation `op` of [`crate::codec`] over the buffers of `a_len`
/// bytes at `a` and of `b_len` bytes at `b` and the integers `x` and `y`, and
/// answers the length of its record.
#[must_use]
pub fn codec(op: u32, a: u32, a_len: u32, b: u32, b_len: u32, x: u32, y: u32) -> i32 {
    with(REFUSED, |state| {
        let State { buffers, codec, .. } = state;
        let (Some(a), Some(b)) = (buffers.get(a, a_len), buffers.get(b, b_len)) else {
            return NO_BUFFER;
        };
        let (a, b) = (a.to_vec(), b.to_vec());
        if codec.run(op, &a, &b, x, y, buffers.answer_mut()) {
            buffers.answer_len()
        } else {
            REFUSED
        }
    })
}

/// Returns a token as the exports answer it.
fn token(token: u32) -> i32 {
    i32::try_from(token).unwrap_or(REFUSED)
}
