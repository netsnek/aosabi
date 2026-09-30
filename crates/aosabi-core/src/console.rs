// SPDX-License-Identifier: MIT
/*
 * aosabi-core: the console of the process, the lines the reference writes
 * through console.log.
 *
 * No analogue file on the reference side.
 */

//! [`console_log!`](crate::console_log), [`log_utf8`], and under the feature
//! `console-watch` the watcher of the console and the writer of a host.
//!
//! The reference writes a record of the JSON sink, of the line sink and of
//! the pretty sink, and the listen line of the serving plugin, through
//! `console.log`, which an SDK may watch, as the console integration of
//! `@sentry/node` turns every call into a breadcrumb. The devtools sink
//! writes through `process.stdout.write` and is no call of the console. A
//! Rust process has no console object, so the lines that are `console.log` on
//! the reference go through [󰀆](crate::console_log), which prints the line on
//! standard output, and the line of the JSON sink, which is bytes the writer
//! of `aosabi-json` makes, goes through [`log_utf8`], which prints those
//! bytes as they are.
//!
//! A host with no standard output of its own, the module `core.wasm` of the
//! TypeScript option, installs a writer under the same feature, which takes
//! each line in place of standard output.
//!
//! A build without the feature `console-watch` has no watcher, and the macro
//! is `println!` with the same arguments, so a service that installs no SDK
//! compiles a plain print at each call site and gains no byte for the seam.
//! Under the feature the macro hands its arguments to the function `log`,
//! which formats the line once, prints it and hands it to the watcher of the
//! thread, which `set_watcher` installs, the one seam of the console an SDK
//! reaches. The watcher runs after the print and on the thread of the caller,
//! inside the request whose code wrote the line, so it reads the context of
//! that request through [󰀄](crate::context::ambient_context).
//!
//! The feature is read where the macro is defined, so the macro follows the
//! features of this crate and not those of the crate that calls it, and a
//! service whose graph turns the feature on anywhere writes every line
//! through `log` or hands it to the watcher from [`log_utf8`].

#[cfg(feature = "console-watch")]
use core::{
    cell::RefCell,
    fmt, //
};
use std::io::Write as _;
#[cfg(feature = "console-watch")]
use std::rc::Rc;

/// Writes the line its arguments format to and a line feed on standard
/// output, `console.log(line)`.
///
/// The arguments are those of `println!`. In a build without the feature
/// `console-watch` the macro is `println!` itself.
#[cfg(not(feature = "console-watch"))]
#[macro_export]
macro_rules! console_log {
    ($($arg:tt)*) => {
        ::std::println!($($arg)*)
    };
}

/// Writes the line its arguments format to and a line feed on standard
/// output, `console.log(line)`, and hands the line to the watcher of the
/// thread.
///
/// The arguments are those of `println!`. Under the feature `console-watch`
/// the macro calls [`console::log`](crate::console::log).
#[cfg(feature = "console-watch")]
#[macro_export]
macro_rules! console_log {
    ($($arg:tt)*) => {
        $crate::console::log(::core::format_args!($($arg)*))
    };
}

/// Writes the line `args` format to and a line feed on standard output, or
/// hands the line to the writer of the host in its place when one is
/// installed, and hands the line to the watcher of the thread when one is
/// installed, with the level `log`, the call
/// [`console_log!`](crate::console_log) makes under the feature
/// `console-watch`.
#[cfg(feature = "console-watch")]
pub fn log(args: fmt::Arguments<'_>) {
    let line = std::fmt::format(args);
    match writer() {
        Some(writer) => writer(&line),
        None => println!("{line}"),
    }
    notify(&ConsoleCall {
        level: "log",
        message: &line,
    });
}

/// The writer of a host that takes the place of standard output, which takes
/// a line without its line feed, as `console.log` of the host takes it.
///
/// The module `core.wasm` of the TypeScript option has no standard output of
/// its own, since the standard library of `wasm32-unknown-unknown` drops what
/// it prints, so its wrapper hands the line to `console.log` of the host,
/// where a TypeScript service writes it on that host.
#[cfg(feature = "console-watch")]
pub type ConsoleWriter = Rc<dyn Fn(&str)>;

// An `unsafe` cell would skip the borrow flag of each line.
#[cfg(feature = "console-watch")]
thread_local! {
    /// The writer of the host of this thread, none until a host installs one.
    static WRITER: RefCell<Option<ConsoleWriter>> = const { RefCell::new(None) };
}

/// Installs `writer` as the writer of the host of this thread, or removes it
/// for `None`, so every later line goes to it in place of standard output.
#[cfg(feature = "console-watch")]
pub fn set_writer(writer: Option<ConsoleWriter>) {
    // A thread that is being torn down keeps no writer.
    let _ = WRITER.try_with(|slot| *slot.borrow_mut() = writer);
}

/// Returns the writer of the host of this thread, if one is installed.
#[cfg(feature = "console-watch")]
fn writer() -> Option<ConsoleWriter> {
    WRITER.try_with(|slot| slot.borrow().clone()).ok().flatten()
}

/// Writes the line whose UTF-8 bytes `line` holds and a line feed on
/// standard output, `console.log(line)`, and under the feature
/// `console-watch` hands the line to the watcher of the thread.
///
/// The JSON sink of the logger writes its record this way, as the bytes the
/// writer of `aosabi-json` appends, which are UTF-8 by construction, so the
/// line leaves as it was written, with no check of its text and no copy into
/// a string. The line and its line feed are written under one lock of
/// standard output, as `println!` writes its line. A watcher reads the line
/// as text, so while one is installed the line is checked once as UTF-8,
/// with no copy, to hand it on. Under the same feature a writer of a host,
/// when one is installed, takes the line in place of standard output, as
/// `log` hands it its line, as text checked the same way.
///
/// # Panics
///
/// Panics when standard output refuses the line, with the message of
/// `println!`.
pub fn log_utf8(line: &[u8]) {
    #[cfg(feature = "console-watch")]
    if let Some(writer) = writer() {
        writer(&String::from_utf8_lossy(line));
        notify_utf8("log", line);
        return;
    }
    {
        let mut out = std::io::stdout().lock();
        if let Err(error) = out.write_all(line).and_then(|()| out.write_all(b"\n")) {
            panic!("failed printing to stdout: {error}");
        }
    }
    #[cfg(feature = "console-watch")]
    notify_utf8("log", line);
}

/// One call of the console as a watcher sees it, the level of the method and
/// the text the arguments format to.
#[cfg(feature = "console-watch")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConsoleCall<'a> {
    /// The name of the method, `log` for `console.log`.
    pub level: &'a str,
    /// The text of the call, `util.format(...args)`, which is the line itself
    /// for a call with one string.
    pub message: &'a str,
}

/// The watcher of the console, which sees every call after its line is
/// written.
#[cfg(feature = "console-watch")]
pub type ConsoleWatcher = Rc<dyn Fn(&ConsoleCall<'_>)>;

#[cfg(feature = "console-watch")]
thread_local! {
    /// The watcher of this thread, none until an SDK installs one.
    static WATCHER: RefCell<Option<ConsoleWatcher>> = const { RefCell::new(None) };
}

/// Installs `watcher` as the watcher of the console of this thread, or
/// removes it for `None`, the handler `addConsoleInstrumentationHandler`
/// adds.
///
/// A second call replaces the watcher of the first, so one SDK watches the
/// console at a time.
#[cfg(feature = "console-watch")]
pub fn set_watcher(watcher: Option<ConsoleWatcher>) {
    // A thread that is being torn down keeps no watcher.
    let _ = WATCHER.try_with(|slot| *slot.borrow_mut() = watcher);
}

/// Hands `call` to the watcher of this thread.
///
/// The watcher is taken out of its cell before it runs, so a line it writes
/// itself reaches no watcher and no borrow of the cell is held while it runs.
#[cfg(feature = "console-watch")]
fn notify(call: &ConsoleCall<'_>) {
    let Some(watcher) = WATCHER
        .try_with(|slot| slot.borrow_mut().take())
        .ok()
        .flatten()
    else {
        return;
    };
    watcher(call);
    let _ = WATCHER.try_with(|slot| {
        let mut slot = slot.borrow_mut();
        // A watcher the call installed in the meantime wins.
        if slot.is_none() {
            *slot = Some(watcher);
        }
    });
}

/// Hands the line whose UTF-8 bytes `line` holds to the watcher of this
/// thread with the level `level`.
///
/// The text of the call is made only while a watcher is installed, by one
/// check of the bytes as UTF-8, which borrows them with no copy when they
/// are UTF-8, as the bytes of the writer of `aosabi-json` are. A process no
/// SDK watches never reads the line again.
// A `from_utf8_unchecked` would skip this pass.
#[cfg(feature = "console-watch")]
fn notify_utf8(level: &str, line: &[u8]) {
    let watching = WATCHER
        .try_with(|slot| slot.borrow().is_some())
        .unwrap_or(false);
    if watching {
        notify(&ConsoleCall {
            level,
            message: &String::from_utf8_lossy(line),
        });
    }
}
