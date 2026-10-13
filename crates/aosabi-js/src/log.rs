// SPDX-License-Identifier: MIT
/*
 * aosabi-js log: the logger of the engine as the logger of the host.
 *
 * No analogue in the reference.
 */

//! The logger of the host.
//!
//! A logger of the app is a handle of a bound logger of the engine, the two
//! numbers [`Logger::to_raw`] answers, or the root, and a logger of a tag
//! resolves to the logger of the scope with that tag at each call, as
//! `logger(tag)` resolves it. The context of each call of the app
//! carries the logger of the scope in place, which the store of the host keeps,
//! so `getLogger()` answers the logger of the request after any await, and
//! `runWithLogger` of the host puts another one in that store. The function
//! [`log`] behind the export `logger` writes a record, derives a child or a
//! tagged logger, makes a logger of `createLogger` or reads a level, inside the
//! context of the request the call names, so a logger it derives lives as long
//! as that request, as a logger the engine derives does. The engine renders the
//! record as it renders a record of a Rust app and writes the line through the
//! import `host_write`, which the console of [`aosabi_core::console`] takes in
//! place of standard output.
//!
//! The record of a call is the number of the request, 0 outside one, a byte 1
//! where the loop of the host turned since its last call and 0 otherwise, the
//! op, the logger, the byte 0 for the root or 1 and its two numbers, a byte and
//! the tag of a logger of a tag, and the arguments of the op. The answer is the
//! completion record of the value the op answers. A turn of the loop of the
//! host is a turn of the runtime of the engine, which the lazy sink of the
//! development format reads, so the records of a boot before the first request
//! write the fallback line until the loop turns, until the dynamic import of
//! the pretty module resolves. The op `CONFIGURE` applies
//! the option `logger` of the configuration at the start of a pass of
//! `executeConfig` of the host, `configureLogger(cfg)` of `logger.ts:358-376`,
//! which makes a new root logger with a new lazy sink at
//! each pass. The fields of a record and the bindings of a logger keep a member
//! that is undefined, and a function, a symbol and an object whose JSON is no
//! object cross as the text of their line beside the value their JSON line
//! writes. The
//! wrapper of the TypeScript option reads the level of a logger through the op
//! `LEVEL` before it writes a record, so a record below the level hands the
//! engine no field, and it keeps the bindings of `child()` and `createLogger()`
//! of the app and writes them into the fields of each record it writes, so its
//! records of `CHILD` and `CREATE` hold no binding.

#![forbid(unsafe_code)]

use std::{
    rc::Rc,
    sync::Arc, //
};

use aosabi_core::{
    console,
    context::run_with_context,
    logger::{
        HostField,
        LevelSpec,
        LogLevel,
        Logger,
        LoggerOption,
        configure_logger,
        create_logger,
        get_root_logger,
        parse_level_spec, //
    },
};
use aosabi_execute::value::{
    ErrorObject,
    Value, //
};
use aosabi_json::{
    JsObject,
    JsValue,
    Opaque, //
};
use aosabi_lang::js_string::to_string_lossy;

use crate::{
    abi::import,
    host::{
        self,
        Reader, //
    },
    request::BOOT,
    value::{
        self,
        NORMAL,
        THROW, //
    },
};

/// The ops of the record of the export `logger`.
mod op {
    /// Writes a record at a level, `logger.info(msg, fields)` and its kin.
    pub const EMIT: u8 = 0;
    /// Derives a logger with bindings, `logger.child(bindings)`.
    pub const CHILD: u8 = 1;
    /// Derives a logger with a tag, `logger.withTag(tag)`.
    pub const WITH_TAG: u8 = 2;
    /// Makes a logger, `createLogger({level, tag, bindings})`.
    pub const CREATE: u8 = 3;
    /// Reads the level of a logger, `logger.level`.
    pub const LEVEL: u8 = 4;
    /// Applies the option `logger` of the configuration at the start of a
    /// pass of `executeConfig`, `configureLogger(config?.logger)`.
    pub const CONFIGURE: u8 = 5;
}

/// The forms of a member of the fields of a call.
mod field {
    /// A value of JSON, which the line writes as the engine writes a field.
    pub const VALUE: u8 = 0;
    /// The text the line writes and the value the JSON line writes, a
    /// [`HostField`](aosabi_core::logger::HostField).
    pub const TEXT: u8 = 1;
}

/// The stream of `console.log`, where the engine writes a line.
const STDOUT: u32 = 1;

/// The stream of `console.error`, where the logger of Yoga writes a warning
/// and an error.
const STDERR: u32 = 2;

/// The message of a broken record of the logger.
const BROKEN: &str = "the host called the logger with a broken record";

/// Answers the call of the logger the record in the first `len` bytes of the
/// input buffer names, and returns the address of its completion record in
/// the output buffer.
pub fn log(len: u32) -> usize {
    let record = host::input(len);
    let mut reader = Reader::new(&record);
    let request = reader.word();
    if reader.byte() == Some(1) {
        aosabi_core::clock::note_turn();
    }
    let context = request
        .filter(|request| *request != BOOT)
        .and_then(crate::context::context_of);
    let answered = match context {
        Some(context) => run_with_context(context, || call(&mut reader)),
        None => call(&mut reader),
    };
    let mut out = Vec::new();
    if let Some(value) = answered {
        out.push(NORMAL);
        host::encode(&mut out, &value);
    } else {
        out.push(THROW);
        host::encode(&mut out, &Value::from(ErrorObject::error(BROKEN)));
    }
    host::output(&out)
}

/// Runs the op of the record at the cursor of `reader`, or answers `None` for
/// a broken record.
fn call(reader: &mut Reader<'_>) -> Option<Value> {
    let code = reader.byte()?;
    let base = match reader.byte()? {
        0 => get_root_logger(),
        _ => Logger::from_raw((reader.word()?, reader.word()?)),
    };
    let logger = match reader.byte()? {
        0 => base,
        _ => base.with_tag(reader.text()?),
    };
    match code {
        op::EMIT => {
            let level = level_of(reader.byte()?)?;
            let msg = to_string_lossy(&reader.string()?).into_owned();
            let fields = fields(reader)?;
            match level {
                LogLevel::Trace => logger.trace(&msg, fields),
                LogLevel::Debug => logger.debug(&msg, fields),
                LogLevel::Info => logger.info(&msg, fields),
                LogLevel::Warn => logger.warn(&msg, fields),
                LogLevel::Error => logger.error(&msg, fields),
                LogLevel::Fatal => logger.fatal(&msg, fields),
            }
            Some(Value::Undefined)
        }
        op::CHILD => Some(raw(logger.child(fields(reader)?))),
        op::WITH_TAG => Some(raw(logger.with_tag(reader.text()?))),
        op::CREATE => {
            let level = match reader.byte()? {
                0 => None,
                1 => Some(parse_level_spec(reader.text()?)),
                _ => {
                    let count = reader.count()?;
                    let mut map = Vec::with_capacity(count.min(16));
                    for _ in 0..count {
                        let tag = String::from(reader.text()?);
                        map.push((tag, LogLevel::parse(reader.text()?)?));
                    }
                    Some(LevelSpec::Map(map))
                }
            };
            let tag = match reader.byte()? {
                0 => None,
                _ => Some(String::from(reader.text()?)),
            };
            let bindings = fields(reader)?;
            Some(raw(create_logger(
                level.as_ref(),
                None,
                tag.as_deref(),
                Some(bindings),
            )))
        }
        op::LEVEL => Some(js(&JsValue::from(logger.level().as_str()))),
        op::CONFIGURE => {
            let option = match reader.byte()? {
                0 => None,
                1 => Some(LoggerOption::Enabled(true)),
                2 => Some(LoggerOption::Enabled(false)),
                _ => Some(LoggerOption::Config(crate::boot::logger_config(
                    &value::decode(reader, BOOT)?.to_js(),
                ))),
            };
            configure_logger(option.as_ref());
            crate::boot::note_configured();
            Some(Value::Undefined)
        }
        _ => None,
    }
}

/// Reads the fields of a call at the cursor of `reader`, or answers `None`
/// for a broken record.
///
/// The fields are the byte 0 for none, which are no member, or the byte 1, a
/// count and each member, its key, the byte of its form and the value record of
/// its value, with the text of its line before the value for the form
/// [`field::TEXT`]. A member keeps a value that is undefined, which the line
/// writes as `undefined` and the JSON line leaves out, and a member of the form [`field::TEXT`] is a
/// [`HostField`], whose line and whose JSON the logger of the engine writes
/// apart.
fn fields(reader: &mut Reader<'_>) -> Option<JsObject> {
    let mut object = JsObject::new();
    if reader.byte()? == 0 {
        return Some(object);
    }
    let count = reader.count()?;
    for _ in 0..count {
        let key = reader.string()?;
        let value = match reader.byte()? {
            field::VALUE => value::decode(reader, BOOT)?.to_js(),
            field::TEXT => {
                let line = to_string_lossy(&reader.string()?).into_owned();
                let json = value::decode(reader, BOOT)?.to_js();
                // An `unsafe` `Rc` would skip the atomic count each copy takes.
                JsValue::Opaque(Opaque(Arc::new(HostField { line, json })))
            }
            _ => return None,
        };
        object.insert(key, value);
    }
    Some(object)
}

/// Returns the level of the byte `byte`, 0 for `trace` to 5 for `fatal`.
fn level_of(byte: u8) -> Option<LogLevel> {
    LogLevel::ALL.get(usize::from(byte)).copied()
}

/// Returns the two numbers of the bound logger `logger` as an array of two
/// numbers, the handle the host keeps of it.
fn raw(logger: Logger) -> Value {
    let (index, generation) = logger.to_raw();
    Value::List(Rc::new(vec![
        Value::Number(f64::from(index)),
        Value::Number(f64::from(generation)),
    ]))
}

/// Installs the writers of the console of the engine and of the logger of
/// Yoga, which hand every line to the import `host_write` in place of the
/// standard streams, a record of the logger of the engine to `console.log`
/// of the host and a warning or an error of the logger of Yoga to
/// `console.error`.
pub fn install_writer() {
    console::set_writer(Some(Rc::new(|line: &str| {
        let mut bytes = line.as_bytes().to_vec();
        import::host_write(STDOUT, host::address(&mut bytes), host::length(bytes.len()));
    })));
    graphql_over_http::host_env::set_writer(Some(Rc::new(|to_stderr: bool, line: &str| {
        let mut bytes = line.as_bytes().to_vec();
        let stream = if to_stderr { STDERR } else { STDOUT };
        import::host_write(stream, host::address(&mut bytes), host::length(bytes.len()));
    })));
}

/// Returns the value of the engine of the JSON value `value`.
fn js(value: &JsValue) -> Value {
    Value::from_js(value)
}
