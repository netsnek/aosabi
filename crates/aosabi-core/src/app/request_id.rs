// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-core: the correlation id of a request.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/app/index.ts,
 * Apache-2.0. Changed by the Aosabi authors.
 */

//! `request_id` and `new_id`.
//!
//! The id of a request is its `x-request-id`, else the trace id of a W3C
//! `traceparent`, else a fresh id. The fresh id is a version 4 UUID over the bytes
//! of the randomness capability of [`crate::random`], as `crypto.randomUUID()`
//! answers one, or over the bytes of `/dev/urandom` where no provider is installed,
//! and where the host has neither it is the time and counter form of `index.ts:24`,
//! the time in base 36 and eight more characters, which the reference takes when no
//! Web Crypto exists.

use core::{
    cell::Cell,
    fmt::Write as _, //
};
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
use std::time::{
    SystemTime,
    UNIX_EPOCH, //
};
use std::{
    fs::File,
    io::Read, //
};

use aosabi_http::Context;

thread_local! {
    /// The counter of the time and counter form.
    static COUNTER: Cell<u64> = const { Cell::new(0) };
}

/// Returns the correlation id of the request of `c`, `requestId(c)`.
///
/// The `x-request-id`, else field 1 of a `traceparent` split on `-`, else a fresh
/// id. A header that is present and empty is an id, as `??` keeps an empty string.
#[must_use]
pub fn request_id(c: &Context) -> String {
    if let Some(id) = c.req().header("x-request-id") {
        return id;
    }
    if let Some(traceparent) = c.req().header("traceparent")
        && let Some(trace_id) = traceparent.split('-').nth(1)
    {
        return String::from(trace_id);
    }
    new_id()
}

/// Returns a fresh id, `newId()`.
///
/// A version 4 UUID when a randomness provider is installed or the host has a
/// random source, and the time and counter form otherwise. The random source of the
/// host is the read of `/dev/urandom` an app takes where no boot installed a
/// provider, such as the booted apps of `aosabi-devtool`.
#[must_use]
pub fn new_id() -> String {
    let mut bytes = [0_u8; 16];
    let random = crate::random::fill(&mut bytes).is_ok()
        || File::open("/dev/urandom")
            .and_then(|mut source| source.read_exact(&mut bytes))
            .is_ok();
    if random {
        return uuid_v4(bytes);
    }
    let millis = epoch_millis();
    let counter = COUNTER
        .try_with(|counter| {
            let next = counter.get().wrapping_add(1);
            counter.set(next);
            next
        })
        .unwrap_or(0);
    let mut tail = base36(counter.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ millis);
    while tail.len() < 8 {
        tail.insert(0, '0');
    }
    tail.truncate(8);
    format!("{}{tail}", base36(millis))
}

/// Returns the whole milliseconds since the epoch of the system clock.
///
/// A Worker reads no clock of the system from `std`, whose `SystemTime::now` panics
/// on `wasm32-unknown-unknown`, so a Worker build reads 0 until the glue of the
/// Worker hands the host's time, as the file of `graphql-over-http` does.
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
fn epoch_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        })
}

/// Returns 0, the time of a Worker build, see the other variant.
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
fn epoch_millis() -> u64 {
    0
}

/// Returns the text of a version 4 UUID over the random `bytes`.
fn uuid_v4(mut bytes: [u8; 16]) -> String {
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex = bytes
        .iter()
        .fold(String::with_capacity(32), |mut hex, byte| {
            // Writing into a `String` does not fail.
            let _ = write!(hex, "{byte:02x}");
            hex
        });
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// Returns `value` in base 36 with lower case digits, `toString(36)`.
fn base36(mut value: u64) -> String {
    const DIGITS: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if value == 0 {
        return String::from("0");
    }
    let mut digits = Vec::new();
    while value > 0 {
        digits.push(DIGITS[usize::try_from(value % 36).unwrap_or(0)]);
        value /= 36;
    }
    digits.reverse();
    String::from_utf8(digits).unwrap_or_default()
}
