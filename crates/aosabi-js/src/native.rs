// SPDX-License-Identifier: MIT
/*
 * aosabi-js native: the functions of the host the native library takes at
 * boot in place of the import block of the module.
 *
 * No analogue in the reference.
 */

//! The table of the functions of the host in the native library.
//!
//! The export `boot` of a native build takes fifteen typed `extern "C"`
//! function pointers, the functions of the import block of the module in its
//! order, and [`install`] keeps them in one `thread_local` cell, a plain
//! state of the one thread the host calls the library on. The module
//! [`import`] calls them under the names and the types of the import block,
//! so the code of the engine calls the host as it does in the module, and a
//! call through a pointer whose type is no `unsafe fn` is safe code.
//!
//! The host keeps the promise of each pointer as it keeps the promise of a
//! `safe fn` import: the callback of `bun:ffi` behind it runs the JavaScript
//! of the import inside a `try` and answers a throw as a tagged record, or
//! answers the request of a function that answers none with a failure once
//! the export that called it has returned, so no exception of the host
//! reaches a frame of the library, and the host writes only into the ranges
//! [`crate::host::alloc`] hands out. Before the boot the table is empty, and
//! a function of it answers 0, the record of no length, or does nothing.

#![forbid(unsafe_code)]

use core::cell::Cell;

/// The type of `js_project`, `js_get`, `js_call`, `js_error` and
/// `iter_next`: the table, the handle, the address and the length of a
/// record, and the request, to the length of the record of the answer.
pub type Record = extern "C" fn(u32, u32, usize, u32, u32) -> u32;

/// The type of `host_enqueue`: the request.
pub type Enqueue = extern "C" fn(u32);

/// The type of `host_done`: the request and the address of its answer.
pub type Done = extern "C" fn(u32, usize);

/// The type of `host_write`: the stream, the address and the length of a line.
pub type Write = extern "C" fn(u32, usize, u32);

/// The type of `js_next`: the request, the continuation, the address and the
/// length of its completion record.
pub type JsNext = extern "C" fn(u32, u32, usize, u32);

/// The type of `host_random`: the count of bytes, to the length of the
/// record of the bytes.
pub type Random = extern "C" fn(u32) -> u32;

/// The type of `host_push`: the request, the kind of the record, and the
/// address and the length of its bytes.
pub type Push = extern "C" fn(u32, u32, usize, u32);

/// The type of `host_now`: the milliseconds of the clock of the host.
pub type Now = extern "C" fn() -> f64;

/// The type of `host_timer`: the request and the delay in milliseconds.
pub type Timer = extern "C" fn(u32, u32);

/// The type of `host_fetch`: the request, and the address and the length of
/// the record of the outbound request, to the length of the record of the
/// completion.
pub type Outbound = extern "C" fn(u32, usize, u32) -> u32;

/// The type of `host_crypto`: the address and the length of the record of
/// the call, to the length of the record of the answer.
pub type Crypto = extern "C" fn(usize, u32) -> u32;

/// The fifteen functions of the host as the export `boot` takes them, each
/// `None` where the host left its pointer null.
#[derive(Clone, Copy)]
pub struct Functions {
    /// The function behind [`import::js_project`].
    pub js_project: Option<Record>,
    /// The function behind [`import::js_get`].
    pub js_get: Option<Record>,
    /// The function behind [`import::js_call`].
    pub js_call: Option<Record>,
    /// The function behind [`import::js_error`].
    pub js_error: Option<Record>,
    /// The function behind [`import::iter_next`].
    pub iter_next: Option<Record>,
    /// The function behind [`import::host_enqueue`].
    pub host_enqueue: Option<Enqueue>,
    /// The function behind [`import::host_done`].
    pub host_done: Option<Done>,
    /// The function behind [`import::host_write`].
    pub host_write: Option<Write>,
    /// The function behind [`import::js_next`].
    pub js_next: Option<JsNext>,
    /// The function behind [`import::host_random`].
    pub host_random: Option<Random>,
    /// The function behind [`import::host_push`].
    pub host_push: Option<Push>,
    /// The function behind [`import::host_now`].
    pub host_now: Option<Now>,
    /// The function behind [`import::host_timer`].
    pub host_timer: Option<Timer>,
    /// The function behind [`import::host_fetch`].
    pub host_fetch: Option<Outbound>,
    /// The function behind [`import::host_crypto`].
    pub host_crypto: Option<Crypto>,
}

/// The fifteen functions of the host, in the order of the import block.
#[derive(Clone, Copy)]
struct Table {
    /// The function behind [`import::js_project`].
    js_project: Record,
    /// The function behind [`import::js_get`].
    js_get: Record,
    /// The function behind [`import::js_call`].
    js_call: Record,
    /// The function behind [`import::js_error`].
    js_error: Record,
    /// The function behind [`import::iter_next`].
    iter_next: Record,
    /// The function behind [`import::host_enqueue`].
    host_enqueue: Enqueue,
    /// The function behind [`import::host_done`].
    host_done: Done,
    /// The function behind [`import::host_write`].
    host_write: Write,
    /// The function behind [`import::js_next`].
    js_next: JsNext,
    /// The function behind [`import::host_random`].
    host_random: Random,
    /// The function behind [`import::host_push`].
    host_push: Push,
    /// The function behind [`import::host_now`].
    host_now: Now,
    /// The function behind [`import::host_timer`].
    host_timer: Timer,
    /// The function behind [`import::host_fetch`].
    host_fetch: Outbound,
    /// The function behind [`import::host_crypto`].
    host_crypto: Crypto,
}

thread_local! {
    /// The functions of the host, once the boot has taken them.
    static TABLE: Cell<Option<Table>> = const { Cell::new(None) };
}

/// Keeps the fifteen functions of the host the export `boot` took, or answers
/// the message of the failure where the host left one of them null.
///
/// # Errors
///
/// Returns the message that names the first function the host left null.
pub fn install(functions: Functions) -> Result<(), &'static str> {
    let table = Table {
        js_project: functions
            .js_project
            .ok_or("the host handed no function js_project")?,
        js_get: functions
            .js_get
            .ok_or("the host handed no function js_get")?,
        js_call: functions
            .js_call
            .ok_or("the host handed no function js_call")?,
        js_error: functions
            .js_error
            .ok_or("the host handed no function js_error")?,
        iter_next: functions
            .iter_next
            .ok_or("the host handed no function iter_next")?,
        host_enqueue: functions
            .host_enqueue
            .ok_or("the host handed no function host_enqueue")?,
        host_done: functions
            .host_done
            .ok_or("the host handed no function host_done")?,
        host_write: functions
            .host_write
            .ok_or("the host handed no function host_write")?,
        js_next: functions
            .js_next
            .ok_or("the host handed no function js_next")?,
        host_random: functions
            .host_random
            .ok_or("the host handed no function host_random")?,
        host_push: functions
            .host_push
            .ok_or("the host handed no function host_push")?,
        host_now: functions
            .host_now
            .ok_or("the host handed no function host_now")?,
        host_timer: functions
            .host_timer
            .ok_or("the host handed no function host_timer")?,
        host_fetch: functions
            .host_fetch
            .ok_or("the host handed no function host_fetch")?,
        host_crypto: functions
            .host_crypto
            .ok_or("the host handed no function host_crypto")?,
    };
    TABLE.set(Some(table));
    Ok(())
}

/// The functions of the host under the names and the types of the import
/// block of the module, each a call through its pointer of the table.
pub mod import {
    use super::TABLE;

    /// Projects the object `handle` of the table `table` by the record at
    /// `at`, `len` bytes long, as the import `js_project` of the module does.
    pub fn js_project(table: u32, handle: u32, at: usize, len: u32, request: u32) -> u32 {
        TABLE
            .get()
            .map_or(0, |host| (host.js_project)(table, handle, at, len, request))
    }

    /// Reads the member the record at `record` names, as the import `js_get`
    /// of the module does.
    pub fn js_get(table: u32, handle: u32, record: usize, len: u32, request: u32) -> u32 {
        TABLE
            .get()
            .map_or(0, |host| (host.js_get)(table, handle, record, len, request))
    }

    /// Calls the function `handle` with the record at `args`, as the import
    /// `js_call` of the module does.
    pub fn js_call(table: u32, handle: u32, args: usize, len: u32, request: u32) -> u32 {
        TABLE
            .get()
            .map_or(0, |host| (host.js_call)(table, handle, args, len, request))
    }

    /// Reads the error `handle` in the form the record at `record` names, as
    /// the import `js_error` of the module does.
    pub fn js_error(table: u32, handle: u32, record: usize, len: u32, request: u32) -> u32 {
        TABLE.get().map_or(0, |host| {
            (host.js_error)(table, handle, record, len, request)
        })
    }

    /// Iterates the value `handle` in the form the record at `record`
    /// names, as the import `iter_next` of the module does.
    pub fn iter_next(table: u32, handle: u32, record: usize, len: u32, request: u32) -> u32 {
        TABLE.get().map_or(0, |host| {
            (host.iter_next)(table, handle, record, len, request)
        })
    }

    /// Asks the host for one microtask that runs one job of `request`, as the
    /// import `host_enqueue` of the module does.
    pub fn host_enqueue(request: u32) {
        if let Some(host) = TABLE.get() {
            (host.host_enqueue)(request);
        }
    }

    /// Tells the host that `request` has answered at `answer`, as the import
    /// `host_done` of the module does.
    pub fn host_done(request: u32, answer: usize) {
        if let Some(host) = TABLE.get() {
            (host.host_done)(request, answer);
        }
    }

    /// Writes the line at `at` to the stream `stream` of the host, as the
    /// import `host_write` of the module does.
    pub fn host_write(stream: u32, at: usize, len: u32) {
        if let Some(host) = TABLE.get() {
            (host.host_write)(stream, at, len);
        }
    }

    /// Settles the promise of the continuation `next` of `request` with the
    /// record at `at`, as the import `js_next` of the module does.
    pub fn js_next(request: u32, next: u32, at: usize, len: u32) {
        if let Some(host) = TABLE.get() {
            (host.js_next)(request, next, at, len);
        }
    }

    /// Writes `len` random bytes into the input buffer, as the import
    /// `host_random` of the module does.
    pub fn host_random(len: u32) -> u32 {
        TABLE.get().map_or(0, |host| (host.host_random)(len))
    }

    /// Pushes a record of the kind `kind` with the bytes at `at` to the body
    /// of `request`, as the import `host_push` of the module does.
    pub fn host_push(request: u32, kind: u32, at: usize, len: u32) {
        if let Some(host) = TABLE.get() {
            (host.host_push)(request, kind, at, len);
        }
    }

    /// Returns the milliseconds of the clock of the host, as the import
    /// `host_now` of the module does, and 0 before the boot.
    pub fn host_now() -> f64 {
        TABLE.get().map_or(0.0, |host| (host.host_now)())
    }

    /// Asks the host to wake `request` after `delay` milliseconds, as the
    /// import `host_timer` of the module does.
    pub fn host_timer(request: u32, delay: u32) {
        if let Some(host) = TABLE.get() {
            (host.host_timer)(request, delay);
        }
    }

    /// Hands the host the outbound request of `request` at `at`, as the
    /// import `host_fetch` of the module does.
    pub fn host_fetch(request: u32, at: usize, len: u32) -> u32 {
        TABLE
            .get()
            .map_or(0, |host| (host.host_fetch)(request, at, len))
    }

    /// Hands the host the call of its crypto at `at`, as the import
    /// `host_crypto` of the module does.
    pub fn host_crypto(at: usize, len: u32) -> u32 {
        TABLE.get().map_or(0, |host| (host.host_crypto)(at, len))
    }
}
