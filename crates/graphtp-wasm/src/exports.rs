// SPDX-License-Identifier: MIT
/*
 * graphtp-wasm exports: the exports of the module, each a line over the safe
 * call of the same name in host.rs.
 *
 * No analogue in the reference.
 */

//! The exports of the module.
//!
//! A symbol name is an unsafe attribute in edition 2024, so each export is
//! `#[unsafe(no_mangle)]` under `#[expect(unsafe_code)]` with its reason, the
//! one place of the crate where the lint that the crate denies is expected.
//! Every export takes and answers plain integers, a `u64` is a `BigInt` on the
//! side of the host, and forwards to [`crate::host`].

use crate::host;

/// Returns the address of a new buffer of `len` bytes, 0 where none is made.
#[expect(
    unsafe_code,
    reason = "a wasm export needs its symbol name, owner question 15 of ADR-0027"
)]
#[unsafe(no_mangle)]
pub extern "C" fn alloc(len: u32) -> u32 {
    host::alloc(len)
}

/// Drops the buffer at `addr`.
#[expect(
    unsafe_code,
    reason = "a wasm export needs its symbol name, owner question 15 of ADR-0027"
)]
#[unsafe(no_mangle)]
pub extern "C" fn free(addr: u32) {
    host::free(addr);
}

/// Returns the address of the answer buffer.
#[expect(
    unsafe_code,
    reason = "a wasm export needs its symbol name, owner question 15 of ADR-0027"
)]
#[unsafe(no_mangle)]
pub extern "C" fn answer() -> u32 {
    host::answer()
}

/// Builds a link and returns its number, 0 where it cannot be built.
#[expect(
    unsafe_code,
    reason = "a wasm export needs its symbol name, owner question 15 of ADR-0027"
)]
#[unsafe(no_mangle)]
pub extern "C" fn engine_new(
    profile: u32,
    param: u32,
    framing: u32,
    max_message: u32,
    max_body: u32,
    sight: u32,
    seed: u64,
) -> u32 {
    host::engine_new(profile, param, framing, max_message, max_body, sight, seed)
}

/// Drops the link `engine`.
#[expect(
    unsafe_code,
    reason = "a wasm export needs its symbol name, owner question 15 of ADR-0027"
)]
#[unsafe(no_mangle)]
pub extern "C" fn engine_free(engine: u32) {
    host::engine_free(engine);
}

/// Brings the link `engine` up.
#[expect(
    unsafe_code,
    reason = "a wasm export needs its symbol name, owner question 15 of ADR-0027"
)]
#[unsafe(no_mangle)]
pub extern "C" fn link_up(engine: u32) -> i32 {
    host::link_up(engine)
}

/// Takes the link `engine` down.
#[expect(
    unsafe_code,
    reason = "a wasm export needs its symbol name, owner question 15 of ADR-0027"
)]
#[unsafe(no_mangle)]
pub extern "C" fn link_down(engine: u32) -> i32 {
    host::link_down(engine)
}

/// Hands the link `engine` the bytes its carrier received at `now`.
#[expect(
    unsafe_code,
    reason = "a wasm export needs its symbol name, owner question 15 of ADR-0027"
)]
#[unsafe(no_mangle)]
pub extern "C" fn push_rx(engine: u32, addr: u32, len: u32, now: u64) -> i32 {
    host::push_rx(engine, addr, len, now)
}

/// Answers the bytes the carrier of the link `engine` sends at `now`.
#[expect(
    unsafe_code,
    reason = "a wasm export needs its symbol name, owner question 15 of ADR-0027"
)]
#[unsafe(no_mangle)]
pub extern "C" fn poll_tx(engine: u32, now: u64) -> i32 {
    host::poll_tx(engine, now)
}

/// Answers the next event record of the link `engine`.
#[expect(
    unsafe_code,
    reason = "a wasm export needs its symbol name, owner question 15 of ADR-0027"
)]
#[unsafe(no_mangle)]
pub extern "C" fn poll_event(engine: u32) -> i32 {
    host::poll_event(engine)
}

/// Asks the link `engine` for an OPERATION and answers its token.
#[expect(
    unsafe_code,
    reason = "a wasm export needs its symbol name, owner question 15 of ADR-0027"
)]
#[unsafe(no_mangle)]
pub extern "C" fn operation(
    engine: u32,
    credential: u32,
    credential_len: u32,
    content: u32,
    content_len: u32,
    repeat: u32,
) -> i32 {
    host::operation(
        engine,
        credential,
        credential_len,
        content,
        content_len,
        repeat,
    )
}

/// Asks the link `engine` for one PULL.
#[expect(
    unsafe_code,
    reason = "a wasm export needs its symbol name, owner question 15 of ADR-0027"
)]
#[unsafe(no_mangle)]
pub extern "C" fn pull(engine: u32) -> i32 {
    host::pull(engine)
}

/// Sends a CANCEL of the stream `handle` on the link `engine`.
#[expect(
    unsafe_code,
    reason = "a wasm export needs its symbol name, owner question 15 of ADR-0027"
)]
#[unsafe(no_mangle)]
pub extern "C" fn cancel(engine: u32, handle: u32) -> i32 {
    host::cancel(engine, handle)
}

/// Ends the exchange of the token `token` on the link `engine`.
#[expect(
    unsafe_code,
    reason = "a wasm export needs its symbol name, owner question 15 of ADR-0027"
)]
#[unsafe(no_mangle)]
pub extern "C" fn abort(engine: u32, token: u32) -> i32 {
    host::abort(engine, token)
}

/// Runs the timers of the link `engine` at `now`.
#[expect(
    unsafe_code,
    reason = "a wasm export needs its symbol name, owner question 15 of ADR-0027"
)]
#[unsafe(no_mangle)]
pub extern "C" fn handle_timeout(engine: u32, now: u64) -> i32 {
    host::handle_timeout(engine, now)
}

/// Answers when the link `engine` next needs its timers run.
#[expect(
    unsafe_code,
    reason = "a wasm export needs its symbol name, owner question 15 of ADR-0027"
)]
#[unsafe(no_mangle)]
pub extern "C" fn next_timeout(engine: u32) -> i64 {
    host::next_timeout(engine)
}

/// Runs an operation of the codec and answers the length of its record.
#[expect(
    unsafe_code,
    reason = "a wasm export needs its symbol name, owner question 15 of ADR-0027"
)]
#[unsafe(no_mangle)]
pub extern "C" fn codec(op: u32, a: u32, a_len: u32, b: u32, b_len: u32, x: u32, y: u32) -> i32 {
    host::codec(op, a, a_len, b, b_len, x, y)
}
