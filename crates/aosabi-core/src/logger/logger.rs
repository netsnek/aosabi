// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-core: the runtime logger.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/core/logger.ts,
 * Apache-2.0. Changed by the Aosabi authors.
 *
 * The environment is read through the alias table of docs/cli-and-targets.md
 * section 6.2.
 */

//! The runtime logger.
//!
//! A logger gates a record by the level of its tag, builds it as
//! `{time, level, msg, ...bindings, ...fields}` with the tag set last, and
//! hands it to its sink and to its tees. The key order follows from the
//! insertion order of [`JsObject`], so a key in the bindings and the fields
//! keeps the position of the bindings and the value of the fields, and the
//! writer of `aosabi-json` applies the order of integer keys.
//!
//! The root sink writes one JSON line when the process is no development
//! process, `NODE_ENV` of `production` and `AOSABI_DEV` other than `1`, and a
//! lazy development sink otherwise. The lazy sink writes [`render_line`], the
//! fallback line the reference writes until the dynamic import of the pretty
//! module has resolved, for its first record and for every record before the
//! loop of the runtime turns after it, and the pretty line for every record
//! after that turn. So the records of a process before its loop first turns
//! carry no time and every later one carries `HH:MM:SS.mmm` in front, but for
//! a later record of the request that wrote the first record, which is pretty
//! within the turn. That is the quirk the plan proposes as
//! `Q17_FIRST_PRETTY_RECORD_UNTIMED`.
//!
//! [`logger`] is a `const fn`, so a module keeps its tagged logger in a
//! `static`, and the logger resolves the scoped logger on every call, as the
//! reference's `logger(tag)` does. A value of the Rust side that is an error
//! is normalized by its caller into `{name, message, stack}`, the one
//! normalization of `logger.ts:128-131`, whose `stack` is the name, a colon, a
//! space and the message, because the frames of a V8 stack have no reading
//! here.

use core::cell::RefCell;
use std::rc::Rc;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
use std::time::{
    SystemTime,
    UNIX_EPOCH, //
};

use aosabi_execute::value::Value;
use aosabi_json::{
    JsObject,
    JsValue,
    case::lower_ascii_exact, //
};
use aosabi_lang::js_string::to_string_lossy;

use super::{
    pretty::{
        devtools_sink,
        pretty_sink, //
    },
    registry::{
        LevelFor,
        LoggerHandle,
        LoggerState,
        register,
        register_permanent,
        resolve,
        root_handle,
        scope,
        set_access_log,
        set_job_log_level,
        set_root, //
    },
};
use crate::{
    app::env_alias,
    clock::turns,
    console_log, //
};

/// The level of a record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogLevel {
    /// `trace`, rank 10.
    Trace,
    /// `debug`, rank 20.
    Debug,
    /// `info`, rank 30.
    Info,
    /// `warn`, rank 40.
    Warn,
    /// `error`, rank 50.
    Error,
    /// `fatal`, rank 60.
    Fatal,
}

impl LogLevel {
    /// The six levels in the order of their ranks.
    pub const ALL: [Self; 6] = [
        Self::Trace,
        Self::Debug,
        Self::Info,
        Self::Warn,
        Self::Error,
        Self::Fatal,
    ];

    /// Returns the rank of the level, `RANK` of `logger.ts:49-56`.
    #[must_use]
    pub const fn rank(self) -> u8 {
        match self {
            Self::Trace => 10,
            Self::Debug => 20,
            Self::Info => 30,
            Self::Warn => 40,
            Self::Error => 50,
            Self::Fatal => 60,
        }
    }

    /// Returns the name of the level, which a record carries.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
            Self::Fatal => "fatal",
        }
    }

    /// Returns the level of the name `name`, `None` for a name that is none.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|level| level.as_str() == name)
    }

    /// Returns the level of the rank `rank`, `RANK_TO_LEVEL` of `logger.ts:57-59`.
    #[must_use]
    pub fn of_rank(rank: u8) -> Option<Self> {
        Self::ALL.into_iter().find(|level| level.rank() == rank)
    }
}

/// A record, `{time, level, msg, ...bindings, ...fields, tag}`.
pub type LogRecord = JsObject;

/// Where a built record goes, `LogSink` of `logger.ts:62`.
pub type LogSink = Rc<dyn Fn(&LogRecord)>;

/// A field of a record that a host hands in, for which the model of JSON
/// holds no one value that both lines write as the reference writes them.
///
/// The line of the reference writes a field as `String(value)`, or as
/// `JSON.stringify(value)` for an object, and its JSON line writes the whole
/// record through `JSON.stringify`, which leaves out a member whose value is
/// undefined, a function or a symbol. A value of an app of the TypeScript
/// option that is a function, a symbol or an object whose JSON is no object
/// crosses as the text of its line beside the value its JSON writes, so both
/// lines read as the reference writes them. A Rust app makes no such field.
#[cfg(feature = "host-fields")]
pub struct HostField {
    /// The text the line writes for the field.
    pub line: String,
    /// The value the JSON line writes for the field, `Undefined` where the
    /// JSON line leaves the field out.
    pub json: JsValue,
}

/// A level of a logger, a scalar or a map of tags to levels, `'*'` the default.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LevelSpec {
    /// One level for every tag.
    Scalar(LogLevel),
    /// A level per tag in the order the map was written, the most specific
    /// prefix winning.
    Map(Vec<(String, LogLevel)>),
}

/// The format of the root sink, `format` of `LoggerConfig`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogFormat {
    /// One JSON line per record.
    Json,
    /// The pretty line of a terminal.
    Pretty,
    /// The pretty line, and the record for an attached debugger, which a Rust
    /// process has none of.
    Devtools,
    /// JSON in production and the lazy development sink otherwise.
    Auto,
}

impl LogFormat {
    /// Returns the format a name spells, `None` for any other text.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "json" => Some(Self::Json),
            "pretty" => Some(Self::Pretty),
            "devtools" => Some(Self::Devtools),
            "auto" => Some(Self::Auto),
            _ => None,
        }
    }
}

/// The object form of `config.logger`, `LoggerConfig` of `logger.ts:300-316`.
#[derive(Clone, Default)]
pub struct LoggerConfig {
    /// A scalar level or a per tag map. `LOG_LEVEL` overrides it.
    pub level: Option<LevelSpec>,
    /// The format. `AOSABI_LOG_FORMAT` overrides it.
    pub format: Option<LogFormat>,
    /// Fields added to every record.
    pub base: Option<JsObject>,
    /// Dotted paths masked before the sink.
    pub redact: Option<Vec<String>>,
    /// A sink that replaces the one of the format.
    pub sink: Option<LogSink>,
    /// The level of the job log, `job.level`.
    pub job_level: Option<LogLevel>,
}

impl core::fmt::Debug for LoggerConfig {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("LoggerConfig")
            .field("level", &self.level)
            .field("format", &self.format)
            .field("base", &self.base)
            .field("redact", &self.redact)
            .field("sink", &self.sink.as_ref().map(|_| "sink"))
            .field("job_level", &self.job_level)
            .finish()
    }
}

/// What the fields of a record may be given as.
pub trait IntoLogFields {
    /// Returns the fields, `None` for no field.
    fn into_log_fields(self) -> Option<JsObject>;
}

impl IntoLogFields for () {
    fn into_log_fields(self) -> Option<JsObject> {
        None
    }
}

impl IntoLogFields for JsObject {
    fn into_log_fields(self) -> Option<JsObject> {
        Some(self)
    }
}

impl IntoLogFields for Option<JsObject> {
    fn into_log_fields(self) -> Option<JsObject> {
        self
    }
}

impl IntoLogFields for JsValue {
    /// An object gives its own keys, and any other value gives none, as a
    /// spread of a primitive adds no key.
    fn into_log_fields(self) -> Option<JsObject> {
        match self {
            Self::Object(object) => Some(object),
            _ => None,
        }
    }
}

/// What a logger stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// The scoped logger with a tag, resolved on every call, `logger(tag)`.
    Tagged(&'static str),
    /// A bound logger of the table.
    Bound(LoggerHandle),
}

/// A logger. A copy holds only a tag or a handle, so a `static` of it is `Sync`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Logger(Kind);

/// Returns the module scoped, lazy, tagged logger of `tag`, `logger(tag)`.
///
/// It resolves the scoped logger on every call, so it stays correlated with
/// the request and carries the tag.
#[must_use]
pub const fn logger(tag: &'static str) -> Logger {
    Logger(Kind::Tagged(tag))
}

/// Returns the scoped logger, or the root logger outside a scope, `getLogger()`.
#[must_use]
pub fn get_logger() -> Logger {
    scope().unwrap_or_else(get_root_logger)
}

/// Returns the root logger, `getRootLogger()`.
///
/// The root is made at the first read with the level of `LOG_LEVEL` and the
/// sink of the environment.
#[must_use]
pub fn get_root_logger() -> Logger {
    Logger(Kind::Bound(root_handle(default_root_state)))
}

/// Returns the state of the root a process starts with.
pub(super) fn default_root_state() -> LoggerState {
    LoggerState {
        sink: if is_dev() {
            lazy_dev_sink()
        } else {
            json_sink()
        },
        tees: Vec::new(),
        level_for: resolver_for(&env_or_scalar()),
        tag: None,
        bindings: JsObject::new(),
    }
}

/// Builds a logger with an explicit level, sink, tag and bindings, `createLogger(opts)`.
///
/// The sink is JSON and the level `info` when none is given.
#[must_use]
pub fn create_logger(
    level: Option<&LevelSpec>,
    sink: Option<LogSink>,
    tag: Option<&str>,
    bindings: Option<JsObject>,
) -> Logger {
    let state = LoggerState {
        sink: sink.unwrap_or_else(json_sink),
        tees: Vec::new(),
        level_for: resolver_for(level.unwrap_or(&LevelSpec::Scalar(LogLevel::Info))),
        tag: tag.map(String::from),
        bindings: bindings.unwrap_or_default(),
    };
    Logger(Kind::Bound(register(state)))
}

impl Logger {
    /// Returns the bound logger this one resolves to now.
    fn resolved(self) -> Self {
        match self.0 {
            Kind::Tagged(tag) => get_logger().with_tag(tag),
            Kind::Bound(_) => self,
        }
    }

    /// Returns the handle of a bound logger, and of the logger it resolves to.
    pub(super) fn handle(self) -> LoggerHandle {
        match self.resolved().0 {
            Kind::Bound(handle) => handle,
            Kind::Tagged(_) => root_handle(default_root_state),
        }
    }

    /// Returns the two numbers of the bound logger this one resolves to now,
    /// which a host keeps across a boundary in place of the logger.
    ///
    /// The wrapper of the TypeScript option keeps them in the store of a call
    /// of the app, as the reference keeps the logger in the store of
    /// `loggerContext`, so a call of `getLogger()` after an await reads the
    /// logger of its request.
    #[must_use]
    pub fn to_raw(self) -> (u32, u32) {
        self.handle().to_raw()
    }

    /// Returns the bound logger of the two numbers `raw` that
    /// [`Logger::to_raw`] answered, the root once its slot has gone.
    #[must_use]
    pub const fn from_raw(raw: (u32, u32)) -> Self {
        Self(Kind::Bound(LoggerHandle::from_raw(raw)))
    }

    /// Returns the state the logger resolves to.
    fn state(self) -> Rc<LoggerState> {
        resolve(self.handle())
    }

    /// Writes a record at `level`, `emit(level, msg, fields)`.
    fn emit(self, level: LogLevel, msg: &str, fields: Option<JsObject>) {
        let state = self.state();
        // A tag-aware gate, before the record is built.
        if level.rank() < (state.level_for)(state.tag.as_deref()) {
            return;
        }
        let mut record = JsObject::new();
        record.insert("time", now_ms());
        record.insert("level", level.as_str());
        record.insert("msg", msg);
        for (key, value) in state.bindings.iter() {
            record.insert(key.clone(), value.clone());
        }
        if let Some(fields) = fields {
            for (key, value) in fields.iter() {
                record.insert(key.clone(), value.clone());
            }
        }
        if let Some(tag) = &state.tag {
            record.insert("tag", tag.as_str());
        }
        (state.sink)(&record);
        for (sink, min) in &state.tees {
            if level.rank() >= *min {
                sink(&record);
            }
        }
    }

    /// Writes a record at `trace`.
    pub fn trace(&self, msg: &str, fields: impl IntoLogFields) {
        self.resolved()
            .emit(LogLevel::Trace, msg, fields.into_log_fields());
    }

    /// Writes a record at `debug`.
    pub fn debug(&self, msg: &str, fields: impl IntoLogFields) {
        self.resolved()
            .emit(LogLevel::Debug, msg, fields.into_log_fields());
    }

    /// Writes a record at `info`.
    pub fn info(&self, msg: &str, fields: impl IntoLogFields) {
        self.resolved()
            .emit(LogLevel::Info, msg, fields.into_log_fields());
    }

    /// Writes a record at `warn`.
    pub fn warn(&self, msg: &str, fields: impl IntoLogFields) {
        self.resolved()
            .emit(LogLevel::Warn, msg, fields.into_log_fields());
    }

    /// Writes a record at `error`.
    pub fn error(&self, msg: &str, fields: impl IntoLogFields) {
        self.resolved()
            .emit(LogLevel::Error, msg, fields.into_log_fields());
    }

    /// Writes a record at `fatal`.
    pub fn fatal(&self, msg: &str, fields: impl IntoLogFields) {
        self.resolved()
            .emit(LogLevel::Fatal, msg, fields.into_log_fields());
    }

    /// Returns a logger that carries `bindings` on every record, `child(bindings)`.
    #[must_use]
    pub fn child(&self, bindings: impl IntoLogFields) -> Self {
        let state = self.resolved().state();
        let mut merged = state.bindings.clone();
        if let Some(bindings) = bindings.into_log_fields() {
            for (key, value) in bindings.iter() {
                merged.insert(key.clone(), value.clone());
            }
        }
        Self(Kind::Bound(register(LoggerState {
            sink: Rc::clone(&state.sink),
            tees: state.tees.clone(),
            level_for: Rc::clone(&state.level_for),
            tag: state.tag.clone(),
            bindings: merged,
        })))
    }

    /// Returns a logger tagged with `tag`, which composes with `:`, `withTag(tag)`.
    #[must_use]
    pub fn with_tag(&self, tag: &str) -> Self {
        let state = self.resolved().state();
        let tag = match &state.tag {
            Some(outer) => format!("{outer}:{tag}"),
            None => String::from(tag),
        };
        Self(Kind::Bound(register(LoggerState {
            sink: Rc::clone(&state.sink),
            tees: state.tees.clone(),
            level_for: Rc::clone(&state.level_for),
            tag: Some(tag),
            bindings: state.bindings.clone(),
        })))
    }

    /// Returns a logger that also hands each record at or above `min_level`
    /// to `sink`, `tee(sink, minLevel)`.
    #[must_use]
    pub fn tee(&self, sink: LogSink, min_level: Option<LogLevel>) -> Self {
        let state = self.resolved().state();
        let mut tees = state.tees.clone();
        tees.push((sink, min_level.unwrap_or(LogLevel::Info).rank()));
        Self(Kind::Bound(register(LoggerState {
            sink: Rc::clone(&state.sink),
            tees,
            level_for: Rc::clone(&state.level_for),
            tag: state.tag.clone(),
            bindings: state.bindings.clone(),
        })))
    }

    /// Returns the effective minimum level for the tag of this logger, `level`.
    #[must_use]
    pub fn level(&self) -> LogLevel {
        let state = self.resolved().state();
        LogLevel::of_rank((state.level_for)(state.tag.as_deref())).unwrap_or(LogLevel::Info)
    }
}

/// Returns the milliseconds since the epoch, `Date.now()`.
///
/// A Worker reads no clock of the system from `std`, whose `SystemTime::now`
/// panics on `wasm32-unknown-unknown`, so a Worker build reads 0 until the
/// glue of the Worker hands the host's time, as the file of
/// `graphql-over-http` does.
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
fn now_ms() -> f64 {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    // A time value of JavaScript is an integer count of milliseconds, which a
    // double holds exactly up to 2^53, far beyond any wall clock.
    #[expect(
        clippy::cast_precision_loss,
        reason = "the milliseconds of a wall clock are far below 2^53"
    )]
    let ms = elapsed.as_millis() as f64;
    ms
}

/// Returns 0, the time of a Worker build, see the other variant.
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
fn now_ms() -> f64 {
    0.0
}

/// The default sink, one JSON line through the console, `jsonSink` of `logger.ts:66`.
///
/// The line is the bytes of `JSON.stringify(record)`, which the writer of
/// `aosabi-json` appends from the record where it stands, and the console
/// prints those bytes as they are, since they are UTF-8 by construction, so
/// neither the record nor its text is copied. No pass checks the text unless
/// a watcher of the console is installed, as `init` of `aosabi-sentry`
/// installs one, and then the console checks the line once as UTF-8 to hand
/// it to the watcher as text. With the feature `host-fields` the bytes are
/// those of the record `json_record` makes, each field of a host as the
/// value its JSON writes.
fn json_sink() -> LogSink {
    Rc::new(|record: &LogRecord| {
        let mut line = Vec::new();
        #[cfg(feature = "host-fields")]
        aosabi_json::writer::write_object(&mut line, &json_record(record));
        #[cfg(not(feature = "host-fields"))]
        aosabi_json::writer::write_object(&mut line, record);
        crate::console::log_utf8(&line);
    })
}

/// Returns the record the JSON line writes, each field of a host as the
/// value its JSON writes.
#[cfg(feature = "host-fields")]
fn json_record(record: &LogRecord) -> LogRecord {
    let mut written = JsObject::new();
    for (key, value) in record.iter() {
        let value = host_field(value).map_or_else(|| value.clone(), |field| field.json.clone());
        written.insert(key.clone(), value);
    }
    written
}

/// Returns the field of a host that `value` holds, if it holds one.
#[cfg(feature = "host-fields")]
fn host_field(value: &JsValue) -> Option<&HostField> {
    match value {
        JsValue::Opaque(opaque) => opaque.downcast_ref::<HostField>(),
        _ => None,
    }
}

/// The fallback development sink, the line of [`render_line`] through the
/// console, `lineSink` of `logger.ts:75`.
fn line_sink() -> LogSink {
    Rc::new(|record: &LogRecord| console_log!("{}", render_line(record)))
}

/// The turn of the first record of a lazy sink and the request id that record
/// carries, if any, `None` before the first record.
type FirstRecord = Option<(u64, Option<JsValue>)>;

/// The lazy development sink, `lazyDevSink(mode)` of `logger.ts:95-112`.
///
/// The reference writes the fallback line until the dynamic import of the
/// pretty module has resolved. The first record starts the import, and Node
/// resolves it from the file system only when its event loop turns, so every
/// record before that turn is the fallback line and every record after it is
/// pretty. The sink reads the turns the runtime notes,
/// [󰀄](crate::clock::turns), and a process that runs no runtime notes none, so
/// its sink stays on the fallback line. A later record of the request whose
/// record started the import, the one that carries the same `requestId`, is
/// pretty within the turn as well, since the chain of the request in the
/// reference outlasts the import, so the access line of the first request of a
/// fresh process carries the time while the record of its error before it
/// carries none. The two modes of the reference write the same terminal line
/// here, because a Rust process has no inspector to send the record to.
fn lazy_dev_sink() -> LogSink {
    let first: Rc<RefCell<FirstRecord>> = Rc::new(RefCell::new(None));
    let fallback = line_sink();
    let real = if inspector_active() {
        devtools_sink()
    } else {
        pretty_sink()
    };
    Rc::new(move |record: &LogRecord| {
        let now = turns();
        let loaded = match &*first.borrow() {
            Some((turn, request)) => {
                *turn < now
                    || request
                        .as_ref()
                        .is_some_and(|id| record.get("requestId") == Some(id))
            }
            None => false,
        };
        if loaded {
            real(record);
            return;
        }
        fallback(record);
        let mut first = first.borrow_mut();
        if first.is_none() {
            *first = Some((now, record.get("requestId").cloned()));
        }
    })
}

/// Returns whether a debugger attaches to the process, `inspectorActive()`.
///
/// A Rust process opens no inspector, and the flag the reference also reads,
/// `--inspect` in `NODE_OPTIONS`, is read as it is.
fn inspector_active() -> bool {
    std::env::var("NODE_OPTIONS").is_ok_and(|options| options.contains("--inspect"))
}

/// The keys the line leaves out, `RESERVED` of `logger.ts:114`.
const RESERVED: [&str; 4] = ["time", "level", "msg", "tag"];

/// Returns a record as a single human line, `renderLine(record)`.
///
/// The level in upper case padded to five, the tag in brackets, the message
/// and the other fields as `  k=v` pairs, an object value as its JSON and any
/// other as its string.
#[must_use]
pub fn render_line(record: &LogRecord) -> String {
    let tag = match record.get("tag") {
        Some(JsValue::String(tag)) if !tag.is_empty() => {
            format!("[{}] ", to_string_lossy(tag))
        }
        _ => String::new(),
    };
    // The level reaches the line, and a field of the record may set it to
    // any text, so the upper case keeps every mapping of `toUpperCase`.
    let level = match record.get("level") {
        Some(JsValue::String(level)) => upper_level(&to_string_lossy(level)),
        _ => String::new(),
    };
    let msg = record.get("msg").map(field_text).unwrap_or_default();
    format!("{level:<5} {tag}{msg}{}", fields_tail(record, &RESERVED))
}

/// Returns the level `level` of a record in upper case, as `toUpperCase`
/// writes it, through the form of ASCII where the level is ASCII, as every
/// level of the logger is, which writes the same.
fn upper_level(level: &str) -> String {
    if level.is_ascii() {
        return level.to_ascii_uppercase();
    }
    upper_level_unicode(level)
}

/// Returns `level` in upper case by the case mapping of Unicode.
// Out of line, so a line compiles the tables of the mapping where its level
// needs them. A host `toUpperCase` would drop the tables.
#[cold]
#[inline(never)]
fn upper_level_unicode(level: &str) -> String {
    level.to_uppercase()
}

/// Returns the extra fields of a record as `  k=v` pairs, `fieldsTail(record)`.
pub(super) fn fields_tail(record: &LogRecord, reserved: &[&str]) -> String {
    let parts: Vec<String> = record
        .own_keys()
        .filter(|(key, _)| {
            !reserved
                .iter()
                .any(|reserved| key.as_wtf8() == reserved.as_bytes())
        })
        .map(|(key, value)| format!("{}={}", to_string_lossy(key), field_text(value)))
        .collect();
    if parts.is_empty() {
        return String::new();
    }
    format!("  {}", parts.join(" "))
}

/// Returns a field as the line writes it, an object as its JSON and any
/// other value as `String(value)`, and a field of a host as the text the
/// host gave.
pub(super) fn field_text(value: &JsValue) -> String {
    #[cfg(feature = "host-fields")]
    if let Some(field) = host_field(value) {
        return field.line.clone();
    }
    match value {
        JsValue::Array(_) | JsValue::Object(_) | JsValue::Opaque(_) => {
            aosabi_json::to_string(value).unwrap_or_default()
        }
        other => aosabi_scalars::js_string::to_string(other),
    }
}

/// Returns an error as the record carries it, `{name, message, stack}`.
///
/// The stack is the name, a colon, a space and the message, the first line of
/// a V8 stack, because the frames of a stack have no reading here. A value
/// that is no error is the JSON of its value.
#[must_use]
pub fn normalize_error(value: &Value) -> JsValue {
    let Value::Error(error) = value else {
        return value.to_js();
    };
    let name = to_string_lossy(&error.name()).into_owned();
    let message = to_string_lossy(&error.message()).into_owned();
    let stack = stack_line(&name, &message);
    let mut object = JsObject::new();
    object.insert("name", name);
    object.insert("message", message);
    object.insert("stack", stack);
    JsValue::Object(object)
}

/// Returns the first line of a V8 stack, `name: message`, or the name alone
/// for an empty message.
fn stack_line(name: &str, message: &str) -> String {
    if message.is_empty() {
        return String::from(name);
    }
    format!("{name}: {message}")
}

/// Returns whether the process is a development process, `isDev()`.
fn is_dev() -> bool {
    env_alias::var("NODE_ENV").as_deref() != Some("production")
        || env_alias::var("AOSABI_DEV").as_deref() == Some("1")
}

/// Returns the level of the root, `envOrScalar()` of `logger.ts:244-247`.
fn env_or_scalar() -> LevelSpec {
    match env_alias::var("LOG_LEVEL") {
        Some(spec) if !spec.is_empty() => parse_level_spec(&spec),
        _ => LevelSpec::Scalar(LogLevel::Info),
    }
}

/// Parses `LOG_LEVEL`, a scalar or a comma map, `parseLevelSpec(spec)`.
///
/// The reference lower cases a level with `toLowerCase` and compares it with
/// the ASCII names of the levels alone, so the port lower cases it with
/// [`lower_ascii_exact()`], which keeps every mapping that holds ASCII.
#[must_use]
pub fn parse_level_spec(spec: &str) -> LevelSpec {
    if !spec.contains(',') && !spec.contains('=') {
        let level = lower_ascii_exact(spec.trim());
        return LevelSpec::Scalar(LogLevel::parse(&level).unwrap_or(LogLevel::Info));
    }
    let mut map: Vec<(String, LogLevel)> = Vec::new();
    for part in spec.split(',') {
        let (raw_key, raw_value) = match part.split_once('=') {
            Some((key, value)) => (key, value.split('=').next().unwrap_or_default()),
            None => ("*", part),
        };
        let level = lower_ascii_exact(raw_value.trim());
        if let Some(level) = LogLevel::parse(&level) {
            let key = String::from(raw_key.trim());
            match map.iter_mut().find(|(known, _)| *known == key) {
                Some(entry) => entry.1 = level,
                None => map.push((key, level)),
            }
        }
    }
    if !map.iter().any(|(key, _)| key == "*") {
        map.push((String::from("*"), LogLevel::Info));
    }
    LevelSpec::Map(map)
}

/// Returns the resolver of a level, `resolverFor(level)` of `logger.ts:163-164`.
///
/// A map answers the rank of the most specific prefix of the tag, `a:b` before
/// `a`, and the rank of `'*'` otherwise.
fn resolver_for(level: &LevelSpec) -> LevelFor {
    match level {
        LevelSpec::Scalar(level) => {
            let rank = level.rank();
            Rc::new(move |_| rank)
        }
        LevelSpec::Map(map) => {
            let default = map
                .iter()
                .find(|(key, _)| key == "*")
                .map_or(LogLevel::Info, |(_, level)| *level)
                .rank();
            let ranks: Vec<(String, u8)> = map
                .iter()
                .map(|(key, level)| (key.clone(), level.rank()))
                .collect();
            Rc::new(move |tag: Option<&str>| {
                let mut current = tag.unwrap_or_default();
                while !current.is_empty() {
                    if let Some((_, rank)) = ranks.iter().find(|(key, _)| key == current) {
                        return *rank;
                    }
                    match current.rfind(':') {
                        Some(end) => current = &current[..end],
                        None => break,
                    }
                }
                default
            })
        }
    }
}

/// Returns the sink of a format, `sinkForFormat(format)` of `logger.ts:322-331`.
fn sink_for_format(format: LogFormat) -> LogSink {
    match format {
        LogFormat::Pretty | LogFormat::Devtools => lazy_dev_sink(),
        LogFormat::Auto if is_dev() => lazy_dev_sink(),
        LogFormat::Json | LogFormat::Auto => json_sink(),
    }
}

/// Returns `sink` with the dotted `paths` masked before it, `redactSink(paths, sink)`.
///
/// Each level on a path is copied, so the fields of the caller never change,
/// and the last key present is `[REDACTED]`.
fn redact_sink(paths: Vec<String>, sink: LogSink) -> LogSink {
    Rc::new(move |record: &LogRecord| {
        let mut clone = JsValue::Object(record.clone());
        for path in &paths {
            let segments: Vec<&str> = path.split('.').collect();
            redact_path(&mut clone, &segments);
        }
        if let JsValue::Object(clone) = clone {
            sink(&clone);
        }
    })
}

/// Masks the last key of `segments` below `root`.
fn redact_path(root: &mut JsValue, segments: &[&str]) {
    let Some((last, walk)) = segments.split_last() else {
        return;
    };
    let mut node = root;
    for segment in walk {
        let next = match node {
            JsValue::Object(object) => object.get_mut(segment),
            JsValue::Array(items) => segment
                .parse::<usize>()
                .ok()
                .and_then(|index| items.get_mut(index)),
            _ => None,
        };
        match next {
            Some(next @ (JsValue::Object(_) | JsValue::Array(_))) => node = next,
            _ => return,
        }
    }
    match node {
        JsValue::Object(object) if object.contains_key(last) => {
            object.insert(*last, "[REDACTED]");
        }
        JsValue::Array(items) => {
            if let Some(item) = last
                .parse::<usize>()
                .ok()
                .and_then(|index| items.get_mut(index))
            {
                *item = JsValue::from("[REDACTED]");
            }
        }
        _ => {}
    }
}

/// The option `config.logger`, `boolean | LoggerConfig`.
#[derive(Clone, Debug)]
pub enum LoggerOption {
    /// `true` for the defaults, `false` for no access line.
    Enabled(bool),
    /// The object form.
    Config(LoggerConfig),
}

impl From<bool> for LoggerOption {
    fn from(enabled: bool) -> Self {
        Self::Enabled(enabled)
    }
}

impl From<LoggerConfig> for LoggerOption {
    fn from(config: LoggerConfig) -> Self {
        Self::Config(config)
    }
}

/// Applies `config.logger` at boot, `configureLogger(cfg)`.
///
/// `false` turns the access line off and keeps the root, and anything else
/// turns it on and rebuilds the root from the object form, `LOG_LEVEL` and
/// `AOSABI_LOG_FORMAT` overriding the level and the format.
pub fn configure_logger(option: Option<&LoggerOption>) {
    if matches!(option, Some(LoggerOption::Enabled(false))) {
        set_access_log(false);
        return;
    }
    set_access_log(true);
    let config = match option {
        Some(LoggerOption::Config(config)) => config.clone(),
        _ => LoggerConfig::default(),
    };

    let level = match env_alias::var("LOG_LEVEL") {
        Some(spec) if !spec.is_empty() => parse_level_spec(&spec),
        _ => config
            .level
            .clone()
            .unwrap_or(LevelSpec::Scalar(LogLevel::Info)),
    };
    let format = match env_alias::var("AOSABI_LOG_FORMAT") {
        Some(format) => LogFormat::parse(&format).unwrap_or(LogFormat::Auto),
        None => config.format.unwrap_or(LogFormat::Auto),
    };
    let mut sink = config
        .sink
        .clone()
        .unwrap_or_else(|| sink_for_format(format));
    if let Some(redact) = &config.redact
        && !redact.is_empty()
    {
        sink = redact_sink(redact.clone(), sink);
    }

    set_job_log_level(config.job_level.unwrap_or(LogLevel::Info));

    let root = register_permanent(LoggerState {
        sink,
        tees: Vec::new(),
        level_for: resolver_for(&level),
        tag: None,
        bindings: config.base.clone().unwrap_or_default(),
    });
    set_root(root);
}
