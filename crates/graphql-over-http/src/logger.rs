// SPDX-License-Identifier: MIT
/*
 * graphql-over-http: the Yoga logger.
 *
 * Derived from graphql-yoga 2.0.1, cjs/index.js, MIT.
 */

//! The logger of Yoga.
//!
//! The logger prints through the console with a colored prefix, `DEBUG` and
//! `INFO` on standard output through `console.debug` and `console.info`,
//! `WARN` and `ERR` on standard error through `console.warn` and
//! `console.error`, and a level silences every call below it. The level is
//! `info` unless the environment sets `DEBUG=1` when the logger is made,
//! which makes it `debug`. Yoga's own logger is made from its option
//! `logging`, and the key `logger` that the config of an app spreads into Yoga's
//! options is not that option.
//!
//! The console of Node prints an error with its stack, which has no reading
//! here, so an error prints as `Error.prototype.toString` gives it. The
//! notebook `65_logs` reports these lines and never fails on them. Under the
//! feature `host-env` a host of the TypeScript option prints them through the
//! writer of its console, `host_env::set_writer`, since the standard
//! streams of a module for `wasm32-unknown-unknown` write nowhere.

use std::io::Write as _;

use aosabi_execute::{
    Value,
    value::inspect::inspect, //
};

/// The prefix of a warning, yellow `WARN`.
pub const WARN_PREFIX: &str = "\x1b[33mWARN\x1b[0m";

/// The prefix of an information, cyan `INFO`.
pub const INFO_PREFIX: &str = "\x1b[36mINFO\x1b[0m";

/// The prefix of an error, red `ERR`.
pub const ERROR_PREFIX: &str = "\x1b[31mERR\x1b[0m";

/// The prefix of a debug line, magenta `DEBUG`.
pub const DEBUG_PREFIX: &str = "\x1b[35mDEBUG\x1b[0m";

/// The level of the logger, `logLevel`, in the order of its scores.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    /// `debug`, score 0.
    Debug,
    /// `info`, score 1.
    Info,
    /// `warn`, score 2.
    Warn,
    /// `error`, score 3.
    Error,
    /// `silent`, score 4.
    Silent,
}

/// The logger of Yoga.
#[derive(Clone, Copy, Debug)]
pub struct HttpLogger {
    /// The level below which a call prints nothing.
    level: LogLevel,
}

/// Returns the logger of `level`, `createLogger(logLevel)`.
///
/// No level takes `debug` when the environment sets `DEBUG=1` and `info`
/// otherwise.
#[must_use]
pub fn create_logger(level: Option<LogLevel>) -> HttpLogger {
    let level = level.unwrap_or_else(|| {
        if debug_var().is_some_and(|debug| debug == "1") {
            LogLevel::Debug
        } else {
            LogLevel::Info
        }
    });
    HttpLogger { level }
}

impl HttpLogger {
    /// Returns the level of the logger.
    #[must_use]
    pub fn level(&self) -> LogLevel {
        self.level
    }

    /// Prints `message` with the `DEBUG` prefix on standard output, `logger.debug(message)`.
    pub fn debug(&self, message: &str) {
        if self.level <= LogLevel::Debug {
            print_line(false, DEBUG_PREFIX, message);
        }
    }

    /// Prints `message` with the `INFO` prefix on standard output, `logger.info(message)`.
    pub fn info(&self, message: &str) {
        if self.level <= LogLevel::Info {
            print_line(false, INFO_PREFIX, message);
        }
    }

    /// Prints `message` with the `WARN` prefix on standard error, `logger.warn(message)`.
    pub fn warn(&self, message: &str) {
        if self.level <= LogLevel::Warn {
            print_line(true, WARN_PREFIX, message);
        }
    }

    /// Prints `message` with the `ERR` prefix on standard error, `logger.error(message)`.
    pub fn error(&self, message: &str) {
        if self.level <= LogLevel::Error {
            print_line(true, ERROR_PREFIX, message);
        }
    }

    /// Prints the thrown `value` with the `ERR` prefix on standard error, `logger.error(error)`.
    pub fn error_value(&self, value: &Value) {
        if self.level <= LogLevel::Error {
            let text = match value {
                Value::Error(error) => error.to_js_string(),
                value => inspect(value),
            };
            print_line(true, ERROR_PREFIX, &text);
        }
    }
}

/// Returns `DEBUG` of the process, `process.env.DEBUG`.
#[cfg(not(feature = "host-env"))]
fn debug_var() -> Option<String> {
    std::env::var("DEBUG").ok()
}

/// Returns `DEBUG` of the process, the one the host handed in first.
#[cfg(feature = "host-env")]
fn debug_var() -> Option<String> {
    crate::host_env::var("DEBUG")
}

/// Writes `prefix` and `message` joined by a space as one line, as the console joins its arguments.
///
/// A failed write is dropped, as the console of Node drops a write to a
/// closed stream. Under the feature `host-env` a line goes through the writer
/// of the console of the host where the host set one.
fn print_line(to_stderr: bool, prefix: &str, message: &str) {
    #[cfg(feature = "host-env")]
    if let Some(writer) = crate::host_env::writer() {
        writer(to_stderr, &format!("{prefix} {message}"));
        return;
    }
    let line = format!("{prefix} {message}\n");
    if to_stderr {
        let _ = std::io::stderr().lock().write_all(line.as_bytes());
    } else {
        let _ = std::io::stdout().lock().write_all(line.as_bytes());
    }
}
