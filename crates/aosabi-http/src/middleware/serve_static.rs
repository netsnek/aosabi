// SPDX-License-Identifier: MIT
/*
 * aosabi-http: the middleware serveStatic of hono over a provider of content.
 *
 * Derived from hono 4.11.9, dist/middleware/serve-static/index.js, MIT.
 * Derived from hono 4.11.9, dist/middleware/serve-static/path.js, MIT.
 * Derived from hono 1.19.9, dist/serve-static.js, MIT.
 */

//! `serve_static`.
//!
//! The middleware of hono is written once over what a platform gives it, the
//! content of a path, the test for a directory and the join of two paths, and
//! the Node adapter writes its own copy over `node:fs`, the two files the
//! header names. Here the middleware is written once over a [`Content`]
//! provider, and its steps are the adapter's, because the bytes the harness
//! compares come from the adapter. The provider of a native card is `FsContent`
//! of `aosabi-serve`, and the Workers card brings its own.
//!
//! A request passes through these steps.
//!
//! 1. A finalized context passes on to `next`.
//! 2. The file name is the option `path` where it is set, which serves one
//!    file for every request, and otherwise `decodeURIComponent` of the
//!    request path. A path that does not decode or that holds a `..` segment
//!    calls `on_not_found` with the request path and passes on.
//! 3. The path is the root joined with the file name, rewritten by
//!    `rewrite_request_path` where no `path` is set. A directory is joined
//!    with `index`, `index.html` by default. A path with no file calls
//!    `on_not_found` with the path and passes on.
//! 4. `Content-Type` is the type of the extension, `; charset=utf-8` after a
//!    `text` type, and `application/octet-stream` where the table has none.
//!    With `precompressed`, a compressible type or no type, the first of
//!    `br`, `zstd` and `gzip` that `Accept-Encoding` names and whose sibling
//!    `.br`, `.zst` or `.gz` exists is served with `Content-Encoding` and
//!    `Vary: Accept-Encoding`.
//! 5. `HEAD` and `OPTIONS` answer 200 with `Content-Length` and no body. A
//!    request with no `Range` answers 200 with the whole file and
//!    `Content-Length`.
//! 6. The callback `on_found` is called with the path last, before the
//!    answer is made, so a header it sets reaches the answer.
//!
//! The answer carries no `Last-Modified` and no `ETag`.
//!
//! Four readings of the adapter are kept where HTTP or a reader expects
//! otherwise, each a quirk proposed to the register of `docs/byte-equality.md`
//! section 13. The `Date` of a range answer is the birth time of the file,
//! where HTTP gives `Date` the time the message originated. The `Range` is read
//! by `parseInt` with no check, so a suffix range `bytes=-N` reads from 0, a
//! range that starts inside the file and ends past it while it is no longer
//! than the file declares more bytes than it sends, and a start past the end or
//! below 0 fails the check of `fs.createReadStream`, whose `RangeError` the
//! error handler answers, where HTTP answers 416. An `OPTIONS` declares the
//! length of the file and sends no body. A file whose extension names a
//! property of `Object.prototype` fails in `getMimeType` with a `TypeError`.

use std::{
    rc::Rc,
    time::{
        SystemTime,
        UNIX_EPOCH, //
    }, //
};

use aosabi_json::number_to_string;

use crate::{
    compose::Next,
    context::Context,
    fetch::{
        body::{
            Body,
            LocalBoxFuture, //
        },
        response::Response, //
    },
    middleware::compress::is_compressible,
    utils::{
        filepath::{
            DEFAULT_DOCUMENT,
            has_parent_segment, //
        },
        mime::{
            InheritedProperty,
            MIMES,
            get_mime_type, //
        },
        url::decode_uri_component, //
    }, //
    web_app::RouteError,
};

/// The codings of a precompressed sibling in the order they are tried, each
/// with the extension of its sibling.
pub const ENCODINGS: [(&str, &str); 3] = [("br", ".br"), ("zstd", ".zst"), ("gzip", ".gz")];

/// The type of a file whose extension the table does not hold.
pub const OCTET_STREAM: &str = "application/octet-stream";

/// The largest integer a double holds exactly, `Number.MAX_SAFE_INTEGER`.
const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

/// The status and the body of an answer the steps prepare, which the
/// middleware makes into the answer last.
type Prepared = (u16, Body);

/// The callback of a file that is served, `onFound(path, c)`.
pub type OnFound = Rc<dyn Fn(&str, &Context)>;

/// The callback of a path that is not served, `onNotFound(path, c)`.
pub type OnNotFound = Rc<dyn Fn(&str, &Context)>;

/// The options of [`serve_static`], the union of those of hono and of the
/// Node adapter.
///
/// The default is the default of the adapter, an empty root, which the join
/// leaves out, and every other option unset.
#[derive(Clone, Default)]
pub struct ServeStaticOptions {
    /// The directory the request path is joined to, `root`.
    pub root: String,
    /// The one file every request is answered with, `path`, which is how a
    /// single page catch all is written. An empty path is unset, as `if
    /// (optionPath)` reads it.
    pub path: Option<String>,
    /// The document of a directory, `index`, `index.html` where unset.
    pub index: Option<String>,
    /// Whether a precompressed sibling is served, `precompressed`.
    pub precompressed: bool,
    /// The extensions and types read before [`MIMES`], `mimes` of hono, which
    /// the Node adapter has not, so the empty default reads as the adapter.
    pub mimes: Vec<(String, String)>,
    /// The rewrite of the file name where no `path` is set, `rewriteRequestPath`.
    pub rewrite_request_path: Option<fn(&str) -> String>,
    /// The callback of a file that is served, `onFound`.
    pub on_found: Option<OnFound>,
    /// The callback of a path that is not served, `onNotFound`.
    pub on_not_found: Option<OnNotFound>,
}

/// What a provider knows of a path, the fields of `fs.Stats` the middleware
/// reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stat {
    /// The length of the file in bytes, `stats.size`.
    pub size: u64,
    /// Whether the path is a directory, `stats.isDirectory()`.
    pub is_dir: bool,
    /// The birth time of the file, `stats.birthtime`, where the platform
    /// knows it.
    pub birth_time: Option<SystemTime>,
}

/// A provider of the files [`serve_static`] serves, the platform half of
/// hono's `serveStatic`.
pub trait Content {
    /// Returns what the provider knows of `path`, `None` where nothing is
    /// there.
    fn stat(&self, path: &str) -> Option<Stat>;

    /// Returns the body of the file at `path`, the bytes from the first
    /// offset of `range` to its second, both included, where `range` is set.
    ///
    /// `None` passes the path on as a path with no file.
    fn open(&self, path: &str, range: Option<(u64, u64)>) -> Option<Body>;

    /// Returns `path` joined below `base`, the option `join` of hono.
    ///
    /// The default is [`default_join`], which hono takes where the platform
    /// gives no join.
    fn join(&self, base: &str, path: &str) -> String {
        default_join(&[base, path])
    }
}

/// Returns the middleware `serveStatic(options)` over `content`.
///
/// The closure is a [`MiddlewareFn`] and also what `app.use` of an app takes.
///
/// [`MiddlewareFn`]: crate::web_app::MiddlewareFn
pub fn serve_static<E: RouteError, C: Content + 'static>(
    options: ServeStaticOptions,
    content: C,
) -> impl Fn(Context, Next<E>) -> LocalBoxFuture<'static, Result<Response, E>> + 'static {
    let state = Rc::new((options, content));
    move |c: Context, next: Next<E>| -> LocalBoxFuture<'static, Result<Response, E>> {
        let state = Rc::clone(&state);
        Box::pin(async move {
            let (options, content) = &*state;
            serve(options, content, c, next).await
        })
    }
}

/// Returns `defaultJoin(...paths)`, the join of hono where the platform gives
/// none.
///
/// The non empty paths are joined with `/`, a run of `/` becomes one, a `.`
/// segment is dropped, and a `..` segment drops the segment before it unless
/// that is `..` too or there is none. An empty result is `.`.
#[must_use]
pub fn default_join(paths: &[&str]) -> String {
    let joined = paths
        .iter()
        .copied()
        .filter(|path| !path.is_empty())
        .collect::<Vec<&str>>()
        .join("/");
    let mut collapsed = String::with_capacity(joined.len());
    for c in joined.chars() {
        if c == '/' && collapsed.ends_with('/') {
            continue;
        }
        collapsed.push(c);
    }
    let mut resolved: Vec<&str> = Vec::new();
    for segment in collapsed.split('/') {
        if segment == ".." && resolved.last().is_some_and(|last| *last != "..") {
            resolved.pop();
        } else if segment != "." {
            resolved.push(segment);
        }
    }
    let result = resolved.join("/");
    if result.is_empty() {
        String::from(".")
    } else {
        result
    }
}

/// Runs the middleware on `c`.
async fn serve<E: RouteError, C: Content>(
    options: &ServeStaticOptions,
    content: &C,
    c: Context,
    next: Next<E>,
) -> Result<Response, E> {
    if c.finalized() {
        return next.run(c).await;
    }
    let option_path = options.path.as_deref().filter(|path| !path.is_empty());
    let filename = if let Some(path) = option_path {
        String::from(path)
    } else {
        let request_path = c.req().path();
        let Some(filename) = request_filename(&request_path) else {
            not_found(options, &request_path, &c);
            return next.run(c).await;
        };
        match options.rewrite_request_path {
            Some(rewrite) => rewrite(&filename),
            None => filename,
        }
    };
    let (path, stats) = match resolve(options, content, &filename) {
        Ok(found) => found,
        Err(path) => {
            not_found(options, &path, &c);
            return next.run(c).await;
        }
    };
    let (path, prepared) = answer(options, content, path, stats, &c)?;
    let Some((code, body)) = prepared else {
        not_found(options, &path, &c);
        return next.run(c).await;
    };
    found(options, &path, &c);
    body_with(&c, body, code)
}

/// Returns the file name of the request path `request_path`, `None` where it
/// does not decode or holds a `..` segment.
///
/// The reference throws in either case and passes on in its `catch`.
fn request_filename(request_path: &str) -> Option<String> {
    decode_uri_component(request_path).filter(|filename| !has_parent_segment(filename))
}

/// Returns the path of `filename` below the root and what the provider knows
/// of it, the document of a directory in place of the directory.
///
/// # Errors
///
/// Returns the path tried last where nothing is there, the path the callback
/// of a missing file reads.
fn resolve<C: Content>(
    options: &ServeStaticOptions,
    content: &C,
    filename: &str,
) -> Result<(String, Stat), String> {
    let path = content.join(&options.root, filename);
    let Some(stats) = content.stat(&path) else {
        return Err(path);
    };
    if !stats.is_dir {
        return Ok((path, stats));
    }
    let index = options.index.as_deref().unwrap_or(DEFAULT_DOCUMENT);
    let path = content.join(&path, index);
    match content.stat(&path) {
        Some(stats) => Ok((path, stats)),
        None => Err(path),
    }
}

/// Calls `on_found` with `path` where it is set.
///
/// The middleware of hono calls it before `c.body` makes the answer, so a
/// header the callback sets with `c.header` reaches the answer on every
/// platform. The Node adapter calls it after the answer is made, and the header
/// still reaches the answer, because the adapter's `Response` keeps the
/// prepared headers of the context and reads them when it writes the answer.
/// The port's response copies the headers when it is made, so the callback runs
/// before the answer is made, which gives the order of hono and what the
/// adapter sends.
fn found(options: &ServeStaticOptions, path: &str, c: &Context) {
    if let Some(on_found) = &options.on_found {
        on_found(path, c);
    }
}

/// Calls `on_not_found` with `path` where it is set.
fn not_found(options: &ServeStaticOptions, path: &str, c: &Context) {
    if let Some(on_not_found) = &options.on_not_found {
        on_not_found(path, c);
    }
}

/// Sets the type and the coding of the file at `path` and returns the path
/// served with the status and the body of its answer, `None` where the
/// provider cannot open it.
///
/// # Errors
///
/// Returns the `TypeError` of [`InheritedProperty`] and the `RangeError` of a
/// range the check of `fs.createReadStream` refuses.
fn answer<E: RouteError, C: Content>(
    options: &ServeStaticOptions,
    content: &C,
    path: String,
    stats: Stat,
    c: &Context,
) -> Result<(String, Option<Prepared>), E> {
    let mime_type =
        mime_type_of(&path, &options.mimes).map_err(|error| E::type_error(&error.to_string()))?;
    c.header("Content-Type", mime_type.as_deref().unwrap_or(OCTET_STREAM));
    let compressible = mime_type.as_deref().is_none_or(is_compressible);
    let (path, stats) = if options.precompressed && compressible {
        precompressed(content, path, stats, c)
    } else {
        (path, stats)
    };
    let prepared = respond(content, &path, stats, c)?;
    Ok((path, prepared))
}

/// Returns the type of `path`, read in `mimes` and then in [`MIMES`], with an
/// empty type read as none, as `||` reads it.
///
/// It is the call of the adapter where `mimes` is empty.
fn mime_type_of(
    path: &str,
    mimes: &[(String, String)],
) -> Result<Option<String>, InheritedProperty> {
    if !mimes.is_empty()
        && let Some(mime_type) =
            get_mime_type(path, mimes)?.filter(|mime_type| !mime_type.is_empty())
    {
        return Ok(Some(mime_type));
    }
    Ok(get_mime_type(path, &MIMES)?.filter(|mime_type| !mime_type.is_empty()))
}

/// Returns the precompressed sibling of `path` the request accepts and what
/// the provider knows of it, setting its coding, and `path` and `stats`
/// where there is none.
///
/// The codings of `Accept-Encoding` are its parts between commas with the white
/// space of `trim` removed, each matched whole, so `gzip;q=1` names no coding.
fn precompressed<C: Content>(
    content: &C,
    path: String,
    stats: Stat,
    c: &Context,
) -> (String, Stat) {
    let accepted = c.req().header("Accept-Encoding").unwrap_or_default();
    let accepted: Vec<&str> = accepted
        .split(',')
        .map(|encoding| encoding.trim_matches(is_js_space))
        .collect();
    for (encoding, extension) in ENCODINGS {
        if !accepted.contains(&encoding) {
            continue;
        }
        let sibling = format!("{path}{extension}");
        if let Some(precompressed_stats) = content.stat(&sibling) {
            c.header("Content-Encoding", encoding);
            c.header_append("Vary", "Accept-Encoding");
            return (sibling, precompressed_stats);
        }
    }
    (path, stats)
}

/// Sets the headers of the answer of the file at `path` and returns its
/// status and its body, `None` where the provider cannot open it.
///
/// # Errors
///
/// Returns the `RangeError` of a range the check of `fs.createReadStream`
/// refuses, and the `TypeError` a refused header left.
fn respond<E: RouteError, C: Content>(
    content: &C,
    path: &str,
    stats: Stat,
    c: &Context,
) -> Result<Option<Prepared>, E> {
    let size = stats.size.to_string();
    let range = c.req().header("range").unwrap_or_default();
    let method = c.req().method();
    if method == "HEAD" || method == "OPTIONS" {
        c.header("Content-Length", &size);
        return Ok(Some((200, Body::Empty)));
    }
    if range.is_empty() {
        c.header("Content-Length", &size);
        return Ok(content.open(path, None).map(|body| (200, body)));
    }
    respond_range(content, path, stats, &range, c)
}

/// Sets the headers of the answer of the part of the file at `path` that
/// `range` names and returns its status and its body.
///
/// # Errors
///
/// Returns the `RangeError` of a range the check of `fs.createReadStream`
/// refuses, and the `TypeError` a refused header left.
fn respond_range<E: RouteError, C: Content>(
    content: &C,
    path: &str,
    stats: Stat,
    range: &str,
    c: &Context,
) -> Result<Option<Prepared>, E> {
    c.header("Accept-Ranges", "bytes");
    c.header("Date", &to_utc_string(stats.birth_time));
    let (start, end) = read_range(range, js_number(stats.size));
    let (start, end) = check_read_stream(start, end).map_err(|message| E::error(&message))?;
    let Some(body) = content.open(path, Some((start, end))) else {
        return Ok(None);
    };
    c.header("Content-Length", &(end - start + 1).to_string());
    c.header(
        "Content-Range",
        &format!("bytes {start}-{end}/{}", stats.size),
    );
    Ok(Some((206, body)))
}

/// Returns `c.body(body, status)`.
///
/// The context has no helper that takes a body and a status, so the status
/// is prepared first, where it stays for the rest of the request. For a
/// `HEAD` and an `OPTIONS` this is `c.status(200)` and `c.body(null)` of the
/// reference.
fn body_with<E: RouteError>(c: &Context, body: Body, status: u16) -> Result<Response, E> {
    c.status(status);
    c.body(body)
}

/// Returns the first and the last offset `range` names in a file of `size`
/// bytes, before any check.
///
/// The first `bytes=` is removed, the text is split at `-`, each of the first
/// two parts is read by `parseInt` with the radix 10, a start of `NaN` or 0 is
/// 0, an end of `NaN` or 0 is the last offset, and an end past the file is the
/// last offset.
fn read_range(range: &str, size: f64) -> (f64, f64) {
    let spec = range.replacen("bytes=", "", 1);
    let mut parts = spec.split('-');
    let start = truthy_or(parts.next().map_or(f64::NAN, parse_int), 0.0);
    let mut end = truthy_or(parts.next().map_or(f64::NAN, parse_int), size - 1.0);
    if size < end - start + 1.0 {
        end = size - 1.0;
    }
    (start, end)
}

/// Returns `value || fallback` for a number, whose falsy values are `NaN`
/// and both zeros.
fn truthy_or(value: f64, fallback: f64) -> f64 {
    if value.is_nan() || value == 0.0 {
        fallback
    } else {
        value
    }
}

/// Returns `parseInt(text, 10)`.
///
/// The leading white space is dropped, a sign is read, and the longest run of
/// decimal digits after it is the number, which is `NaN` where there is none.
/// The digits round to the nearest double, which V8 does for the radix 10, and
/// a run too long for a double is an infinity.
fn parse_int(text: &str) -> f64 {
    let text = text.trim_start_matches(is_js_space);
    let (sign, digits) = match text.as_bytes().first() {
        Some(b'-') => (-1.0, &text[1..]),
        Some(b'+') => (1.0, &text[1..]),
        _ => (1.0, text),
    };
    let end = digits
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(digits.len());
    if end == 0 {
        return f64::NAN;
    }
    sign * digits[..end].parse::<f64>().unwrap_or(f64::NAN)
}

/// Returns the two offsets of a read of the file from `start` to `end` as
/// integers, or the message of the `RangeError` the check of
/// `fs.createReadStream` throws.
///
/// The check reads `start` and then `end` as integers from 0 to
/// `Number.MAX_SAFE_INTEGER` and refuses a start past the end.
///
/// [streams.js]: https://github.com/nodejs/node/blob/v24.13.0/lib/internal/fs/streams.js
/// [validators.js]: https://github.com/nodejs/node/blob/v24.13.0/lib/internal/validators.js
fn check_read_stream(start: f64, end: f64) -> Result<(u64, u64), String> {
    validate_integer(start, "start")?;
    validate_integer(end, "end")?;
    if start > end {
        let range = format!("<= \"end\" (here: {})", number_to_string(end));
        return Err(out_of_range("start", &range, start));
    }
    Ok((to_offset(start), to_offset(end)))
}

/// Checks that `value` is an integer from 0 to `Number.MAX_SAFE_INTEGER`,
/// `validateInteger(value, name, 0)`.
fn validate_integer(value: f64, name: &str) -> Result<(), String> {
    if !is_integer(value) {
        return Err(out_of_range(name, "an integer", value));
    }
    if !(0.0..=MAX_SAFE_INTEGER).contains(&value) {
        return Err(out_of_range(name, ">= 0 && <= 9007199254740991", value));
    }
    Ok(())
}

/// Returns the message of `ERR_OUT_OF_RANGE` for the value `input` of `name`
/// outside `range`.
///
/// An integer beyond 2^32 in magnitude is written with `_` between each group
/// of three digits from the right, and any other number as `String` writes it.
///
/// [errors.js]: https://github.com/nodejs/node/blob/v24.13.0/lib/internal/errors.js
fn out_of_range(name: &str, range: &str, input: f64) -> String {
    let mut received = number_to_string(input);
    if is_integer(input) && input.abs() > 4_294_967_296.0 {
        received = add_numerical_separator(&received);
    }
    format!("The value of \"{name}\" is out of range. It must be {range}. Received {received}")
}

/// Returns `val` with `_` before each group of three characters from the
/// right, a leading `-` apart, `addNumericalSeparator(val)`.
fn add_numerical_separator(val: &str) -> String {
    let start = usize::from(val.starts_with('-'));
    let mut i = val.len();
    let mut res = String::new();
    while i >= start + 4 {
        res = format!("_{}{res}", &val[i - 3..i]);
        i -= 3;
    }
    format!("{}{res}", &val[..i])
}

/// Returns whether `value` is an integer, `Number.isInteger(value)`.
fn is_integer(value: f64) -> bool {
    value.is_finite() && value.fract() == 0.0
}

/// Returns an offset that passed [`check_read_stream`] as an integer.
// The value is an integer from 0 to 2^53 - 1, which a u64 holds exactly.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the check bounds the value to the integers from 0 to 2^53 - 1"
)]
fn to_offset(value: f64) -> u64 {
    value as u64
}

/// Returns the size of a file as the number `stats.size` is.
// A file holds far fewer than 2^53 bytes, so the conversion keeps every
// digit.
#[expect(
    clippy::cast_precision_loss,
    reason = "a file length below 2^53 converts exactly"
)]
fn js_number(size: u64) -> f64 {
    size as f64
}

/// Returns whether `c` is white space or a line terminator, the characters
/// `trim` and `parseInt` drop.
fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{0B}' | '\u{0C}' | '\r' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

/// The names of the days of `toUTCString`, from Sunday.
const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/// The names of the months of `toUTCString`, from January.
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// The constant `msPerDay` of
/// <https://tc39.es/ecma262/#sec-day-number-and-time-within-day>.
const MS_PER_DAY: i64 = 86_400_000;

/// Returns `stats.birthtime.toUTCString()` for the birth time `birth_time`.
///
/// Node makes the date of `Math.round` of the milliseconds of the time spec,
/// `sec * 1000 + nsec / 1e6` as doubles, so a birth time within half a
/// millisecond of the next second prints that second. A platform that knows no
/// birth time reads the zero time spec, the epoch, as libuv copies the zeroed
/// `stx_btime` of `statx`. The text has the shape
/// `Thu, 01 Jan 1970 00:00:00 GMT`.
///
/// [utils.js]: https://github.com/nodejs/node/blob/v24.13.0/lib/internal/fs/utils.js
fn to_utc_string(birth_time: Option<SystemTime>) -> String {
    let time = js_round(birth_time.map_or(0.0, ms_from_time_spec));
    if !time.is_finite() || time.abs() > 8.64e15 {
        return String::from("Invalid Date");
    }
    // The time is an integer within 8.64e15 of zero, which an i64 holds.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the time value is an integer within 8.64e15 of zero"
    )]
    let time = time as i64;
    let day = time.div_euclid(MS_PER_DAY);
    let within = time.rem_euclid(MS_PER_DAY);
    let weekday = name_at(&DAYS, (day + 4).rem_euclid(7));
    let (year, month, date) = civil_from_days(day);
    let month = name_at(&MONTHS, month - 1);
    let year = if year >= 0 {
        format!("{year:04}")
    } else {
        format!("-{:04}", -year)
    };
    let hour = within / 3_600_000;
    let minute = within / 60_000 % 60;
    let second = within / 1_000 % 60;
    format!("{weekday}, {date:02} {month} {year} {hour:02}:{minute:02}:{second:02} GMT")
}

/// Returns the name at `index` of `names`, empty past its end.
fn name_at(names: &[&'static str], index: i64) -> &'static str {
    usize::try_from(index)
        .ok()
        .and_then(|i| names.get(i))
        .copied()
        .unwrap_or_default()
}

/// Returns the milliseconds of `time` as Node reads them from a time spec,
/// `sec * 1000 + nsec / 1e6`, with a whole second and a positive fraction
/// before the epoch as `timespec` holds it.
// The seconds of a time a file system holds are far below 2^53, so the
// conversion keeps every digit.
#[expect(
    clippy::cast_precision_loss,
    reason = "the seconds of a file time are below 2^53"
)]
fn ms_from_time_spec(time: SystemTime) -> f64 {
    let (sec, nsec) = match time.duration_since(UNIX_EPOCH) {
        Ok(after) => (after.as_secs() as f64, after.subsec_nanos()),
        Err(before) => {
            let before = before.duration();
            match before.subsec_nanos() {
                0 => (-(before.as_secs() as f64), 0),
                nanos => (-(before.as_secs() as f64) - 1.0, 1_000_000_000 - nanos),
            }
        }
    };
    sec * 1e3 + f64::from(nsec) / 1e6
}

/// Returns `Math.round(value)`, which rounds a half toward positive infinity.
fn js_round(value: f64) -> f64 {
    let floor = value.floor();
    if value - floor >= 0.5 {
        floor + 1.0
    } else {
        floor
    }
}

/// Returns the year, the month from 1 and the day of the month of the day
/// `day` after the epoch in the proleptic Gregorian calendar.
///
/// The arithmetic is the one of `civil_from_days`, whose result is the one of
/// `YearFromTime`, `MonthFromTime` and `DateFromTime`.
fn civil_from_days(day: i64) -> (i64, i64, i64) {
    let z = day + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let date = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, date)
}
