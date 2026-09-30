// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-core: the pretty formatter of the runtime logger.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/core/logger-pretty.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 */

//! The pretty line of a record.
//!
//! The line is `HH:MM:SS.mmm LEVEL [tag] msg  k=v`, the time of the record in
//! UTC, the level label padded to five, the tag in brackets and the fields but
//! `err` as `k=v` pairs, and a normalized error puts its stack, or its
//! message, on the line after it. Colours are ANSI escapes written only when
//! standard output is a terminal and `NO_COLOR` is unset, and the terminal is
//! asked with `std::io::IsTerminal`.
//!
//! The devtools sink of the reference writes the same terminal line and hands
//! the record to an attached inspector, which a Rust process has none of, so
//! the two sinks write the same bytes here. The pretty sink writes through the
//! console and the devtools sink through standard output, as the reference
//! writes them with `console.log` and `process.stdout.write`, so a watcher of
//! the console sees the lines of the first alone.

use std::{
    io::IsTerminal,
    rc::Rc, //
};

use aosabi_json::JsValue;
use aosabi_lang::js_string::to_string_lossy;

use super::logger::{
    LogRecord,
    LogSink,
    field_text, //
};
use crate::console_log;

/// The reset escape, `C.reset`.
const RESET: &str = "\x1b[0m";
/// The dim escape, `C.dim`.
const DIM: &str = "\x1b[2m";
/// The cyan escape of a tag, `C.cyan`.
const CYAN: &str = "\x1b[36m";

/// The keys the fields of the line leave out, `RESERVED` of `logger-pretty.ts:51`.
const RESERVED: [&str; 5] = ["time", "level", "msg", "tag", "err"];

/// Returns the colour of a level, `LEVEL_COLOR` of `logger-pretty.ts:26-33`.
fn level_color(level: &str) -> &'static str {
    match level {
        "trace" | "debug" => "\x1b[90m",
        "info" => "\x1b[32m",
        "warn" => "\x1b[33m",
        "error" => "\x1b[31m",
        "fatal" => "\x1b[35m",
        _ => "",
    }
}

/// Returns the label of a level, `LEVEL_LABEL` of `logger-pretty.ts:34-41`.
fn level_label(level: &str) -> String {
    match level {
        "trace" => String::from("TRACE"),
        "debug" => String::from("DEBUG"),
        "info" => String::from("INFO "),
        "warn" => String::from("WARN "),
        "error" => String::from("ERROR"),
        "fatal" => String::from("FATAL"),
        // The label reaches the line, and a field of the record may set the
        // level to any text, so the upper case keeps every mapping of
        // `toUpperCase`.
        other => other.to_uppercase(),
    }
}

/// Returns whether colours are written, `colorOn()`.
fn color_on() -> bool {
    std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none()
}

/// Returns `text` in the colour `code` when colours are on, `paint(code, s)`.
fn paint(code: &str, text: &str) -> String {
    if color_on() {
        format!("{code}{text}{RESET}")
    } else {
        String::from(text)
    }
}

/// Returns the fields of a record as the line writes them, `renderFields(record)`.
fn render_fields(record: &LogRecord) -> String {
    let parts: Vec<String> = record
        .own_keys()
        .filter(|(key, _)| {
            !RESERVED
                .iter()
                .any(|reserved| key.as_wtf8() == reserved.as_bytes())
        })
        .map(|(key, value)| {
            let key = format!("{}=", to_string_lossy(key));
            format!("{}{}", paint(DIM, &key), field_text(value))
        })
        .collect();
    if parts.is_empty() {
        return String::new();
    }
    format!("  {}", parts.join(" "))
}

/// Returns the time of a record as `HH:MM:SS.mmm` in UTC.
fn time_of(record: &LogRecord) -> String {
    let time = match record.get("time") {
        Some(JsValue::Number(time)) if time.is_finite() => *time,
        _ => 0.0,
    };
    // The time value of a record is an integer count of milliseconds.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the time value of a record is an integer count of milliseconds"
    )]
    let ms = time as i64;
    let day = ms.rem_euclid(86_400_000);
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        day / 3_600_000,
        day / 60_000 % 60,
        day / 1000 % 60,
        day % 1000
    )
}

/// Returns the human lines of a record, `prettyLines(record)`.
#[must_use]
pub fn pretty_lines(record: &LogRecord) -> Vec<String> {
    let text = |key: &str| match record.get(key) {
        Some(JsValue::String(text)) => to_string_lossy(text).into_owned(),
        Some(other) => field_text(other),
        None => String::new(),
    };
    let level = text("level");
    let label = paint(level_color(&level), &level_label(&level));
    let tag = match record.get("tag") {
        Some(JsValue::String(tag)) if !tag.is_empty() => {
            paint(CYAN, &format!("[{}] ", to_string_lossy(tag)))
        }
        _ => String::new(),
    };
    let mut lines = vec![format!(
        "{} {label} {tag}{}{}",
        paint(DIM, &time_of(record)),
        text("msg"),
        render_fields(record)
    )];

    // Normalized errors carry a `stack`, printed dimmed on its own lines.
    if let Some(JsValue::Object(err)) = record.get("err") {
        match (err.get("stack"), err.get("message")) {
            (Some(JsValue::String(stack)), _) => {
                lines.push(paint(DIM, &to_string_lossy(stack)));
            }
            (_, Some(message)) if truthy(message) => lines.push(paint(DIM, &field_text(message))),
            _ => {}
        }
    }
    lines
}

/// Returns whether a value is truthy, `ToBoolean` of ECMA-262.
fn truthy(value: &JsValue) -> bool {
    match value {
        JsValue::Undefined | JsValue::Null => false,
        JsValue::Bool(value) => *value,
        JsValue::Number(number) => *number != 0.0 && !number.is_nan(),
        JsValue::String(text) => !text.is_empty(),
        JsValue::Array(_) | JsValue::Object(_) | JsValue::Opaque(_) => true,
    }
}

/// The pretty sink, each line of [`pretty_lines`] through the console, `prettySink`.
#[must_use]
pub fn pretty_sink() -> LogSink {
    Rc::new(|record: &LogRecord| {
        for line in pretty_lines(record) {
            console_log!("{line}");
        }
    })
}

/// The devtools sink, `devtoolsSink` of `logger-pretty.ts:103-117`, which
/// writes the terminal lines of the pretty sink on standard output.
///
/// Without the watcher of the console the two sinks write the same bytes the
/// same way, so the devtools sink is the pretty sink and a build links one
/// closure.
#[cfg(not(feature = "console-watch"))]
#[must_use]
pub fn devtools_sink() -> LogSink {
    pretty_sink()
}

/// The devtools sink, `devtoolsSink` of `logger-pretty.ts:103-117`, which
/// writes the terminal lines of the pretty sink on standard output.
///
/// Under the feature `console-watch` the pretty sink writes through
/// [`console_log!`](crate::console_log), while this sink prints each line
/// itself, as `devtoolsSink` writes through `process.stdout.write` and no
/// call of the console, so the watcher never sees a line of it.
#[cfg(feature = "console-watch")]
#[must_use]
pub fn devtools_sink() -> LogSink {
    Rc::new(|record: &LogRecord| {
        for line in pretty_lines(record) {
            println!("{line}");
        }
    })
}
