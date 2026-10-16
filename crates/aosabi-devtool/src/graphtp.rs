// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool graphtp: the primitive through which a notebook reaches the
 * crate graphtp and the HTTP wrapper around its DEFLATE core, one request per
 * line and one answer per line.
 *
 * No analogue in the reference. The primitive is the notebooks' one door to the
 * codec, the status mapping, the stream bodies and the DEFLATE core of the crate
 * graphtp, and its requests follow the messages of docs/graphtp.md section 5, the
 * stream bodies of section 2.4 and the coding of section 3. The zlib wrapper of
 * HTTP in aosabi-http codes through that core, so the same door reaches it. The
 * same door reaches the substatus the port of Yoga decides beside the status.
 */

//! The primitive `graphtp` of the seam.
//!
//! The flag `--requests PATH` answers every request line of the file, one
//! response line each, in the batch form of [`crate::batch`]. The operations
//! reach the status mapping, the two heads, the credential, the first byte
//! rule, the zlib profile and the DEFLATE core of the crate `graphtp`, and
//! every byte string travels as lowercase hex.
//!
//! - The operation `status-byte {status}` answers `byte`, the status byte of
//!   the number, or null when the number has none.
//! - The operation `status-of {byte, substatus}` answers the `status`, the
//!   `class` and the `reason` of the byte, and the `code` and the `name` of the
//!   byte with the substatus, 0 when the request carries none.
//! - The operation `request-head {version, kind, compressed, credential_len}`
//!   answers `hex`, the head written through a sink, or null for a version or a
//!   kind that has no value.
//! - The operation `read-request-head {hex}` answers `head` with its four
//!   fields, or `refused` with the code of the rule of section 5.3 the message
//!   breaks.
//! - The operation `answer-head {status, substatus}` answers `hex`, the head
//!   written through a sink, or null for a status that has no byte.
//! - The operation `read-answer-head {hex}` answers the `code` and the `name`
//!   of the head and the `coding` of the body after it, or `malformed` with the
//!   reason the answer is malformed.
//! - The operation `credential {hex, max_credential}` answers `credential`,
//!   the bytes it accepts, or `refused` with the code of rule 6 or 7 of section
//!   5.3.
//! - The operation `registry {}` answers `codes`, the constants of the registry
//!   of section 2.5 with their codes and names.
//! - The operation `deflate {hex, window_bits, heads, tokens, buffer, chunk,
//!   raw}` codes the plain bytes of `hex` through the streaming encoder and
//!   answers `hex`, the coded bytes, and `scratch`, the bytes of the scratch.
//!   The stream is one zlib stream of the profile at `window_bits` of 9 to 12,
//!   12 by default, or raw DEFLATE data at 9 to 15 when `raw` is true. The
//!   sizes `heads`, `tokens` and `buffer` replace those of the window, and
//!   `chunk` hands the plain bytes to the encoder in pieces of that many
//!   bytes. Sizes that make no scratch answer `error` with the reason.
//! - The operation `inflate {hex, bound, streaming, window, raw}` decodes the
//!   stream of `hex` under the bound, whole into one buffer of `bound` bytes or,
//!   when `streaming` is true, through a ring of `window` bytes, 4,096 by
//!   default, into a vector. It answers `hex`, the plain bytes, or `refused`
//!   with the code of the refusal and `error` with its reason. Raw DEFLATE data
//!   answers `consumed` beside `hex`, and a refusal of it `error` alone.
//! - The operation `check-header {cmf, flg}` answers `ok` for a zlib header of
//!   the profile, or `refused` with the code of rule 9 of section 5.3.
//! - The operation `adler32 {hex}` answers `adler32`, the Adler-32 of the
//!   bytes.
//! - The operation `http-coding {coding, hex, window_bits, heads, tokens,
//!   buffer, chunk}` codes the plain bytes of `hex` through the zlib wrapper of
//!   `aosabi-http`, the content coding `deflate` of HTTP, which `coding` names
//!   and which is the one value it takes, since Aosabi codes no gzip, and
//!   answers `hex` and `scratch` as `deflate` does. The window is 15 bits by
//!   default, the 32 KiB of the `server` card, and the sizes and `chunk` are
//!   those of `deflate`.
//! - The operation `write-request {kind, credential, content, handle}` writes
//!   a whole request of the kind 0, 1 or 2, an OPERATION with the credential
//!   and the content, coded at >128 B, a PULL, or a CANCEL of the
//!   handle, 0 for every stream, and answers `hex`, or `refused` with the code
//!   of a credential the crate refuses.
//! - The operation `read-request {hex, max_credential, max_content}` answers
//!   the request with its `kind`, and for an OPERATION its `credential`, the
//!   `coding` it travelled in and its `content` decoded, and for a CANCEL its
//!   `handle`, or `refused` with the code of the first of the 11 rules of
//!   section 5.3 it breaks, rules 1 to 9 through `wire::decode_request` and
//!   rules 10 and 11 through `wire::decode_content` under `max_content`. The
//!   bounds default to 65,535 and 1 MiB.
//! - The operation `write-answer {status, substatus, body}` writes a whole
//!   answer, its body coded at >128 B, and answers `hex`, or null for a
//!   status that has no byte.
//! - The operation `read-answer {kind, hex, bound}` reads the answer to a
//!   request of the kind 0 to 7 and answers its shape as `answer`, `result`,
//!   `opened`, `batch` or `refused`, with its `code`, its `name`, the `coding`
//!   and the `body` decoded under `bound`, 16 MiB by default, and for a stream
//!   head or a batch its `seq`, its `entries` and the `stream` of a head, or
//!   `malformed` with the reason.
//! - The operation `build-request {credential, content}` builds an OPERATION
//!   through `Request::builder()` of the crate, with each credential of the
//!   hex string or the array of hex strings `credential` set in its order, and
//!   writes it as `write-request` does. It answers `hex`, or `refused` with the
//!   code of the first credential the builder refuses.
//! - The operation `build-answer {status, substatus, body}` builds an answer
//!   through `Response::builder()` of the crate, with each status of the
//!   number or the array of numbers `status` set in its order and then the
//!   substatus, and writes it as `write-answer` does. It answers `hex`, or
//!   `refused` with the code of the first status the builder refuses.
//! - The operation `stream {stream, seq, entries}` writes a plain stream body,
//!   the head of the stream of the handle `stream` or a batch when the request
//!   names none, with its entries, each `{stream, next}` with the hex of the
//!   value or `{stream, complete}`, `{stream, cancelled}` or
//!   `{stream, aborted}`, and answers `hex`.
//! - The operation `entries {hex}` reads a plain stream body and answers its
//!   `stream`, null for a batch, its `seq` and its `entries`, or `malformed`.
//!
//! - The operations `card {card}` and `serve {path, card, connections, ...}`
//!   reach the `GraphTP` listener of `aosabi-core`: the first answers the bounds
//!   of a card, and the second runs the card `test-graphtp`, the listener over a
//!   real Unix Socket with the service of the harness, on Linux alone.
//! - The operation `cache {card, budget, width, texts}` answers what the table of
//!   `aosabi-cache` keeps of the document of each text under the budget of a
//!   card, the tokens, the estimate and the entries it holds.
//! - The operation `substatus {result, batch, accepted}` reaches the processor of
//!   a single JSON result of `graphql-over-http` and answers the `status`, the
//!   `substatus`, the `content_type` and the `body` it makes of a result.
//!
//! A writing operation takes an optional `sink`: `vec`, the default, `slice`
//! with `capacity` bytes, or `pages` with `pages` pages of `page_size` bytes.
//! A sink that has no room answers `full` with the bytes it holds.

use std::{
    path::PathBuf,
    process::ExitCode, //
};

use aosabi_http::coding as http;
use aosabi_json::{
    JsObject,
    JsString,
    JsValue, //
};
use graphtp::{
    body::{
        self,
        Coding, //
    },
    credential::Credential,
    deflate,
    head::{
        AnswerHead,
        RequestHead, //
    },
    kind::Kind,
    sink::{
        Full,
        Pages,
        Sink,
        Slice, //
    },
    status::{
        Code,
        Status,
        Substatus, //
    },
    stream::{
        self,
        Entries,
        Entry,
        Handle,
        Seq,
        Target, //
    },
    version::Version,
    wire::{
        self,
        Answer,
        Content,
        Request,
        WireError, //
    },
    zlib, //
};

use crate::{
    batch::{
        self,
        Operations, //
    },
    cli::{
        self,
        Grammar,
        Matches, //
    },
    homa_packet::{
        hex,
        unhex,
        whole, //
    },
    seam, //
};

mod cache;
#[cfg(target_os = "linux")]
mod card;
mod substatus;

/// The operations of the card `test-graphtp` where the Unix Socket
/// transports of `homa` do not build, each refused.
#[cfg(not(target_os = "linux"))]
mod card {
    use aosabi_json::{
        JsObject,
        JsString, //
    };

    /// Refuses the operation `card`, whose card serves on Linux alone.
    pub(super) fn op_card(_request: &JsObject) -> Result<JsObject, JsString> {
        Err(JsString::from(
            "card: the card test-graphtp serves on Linux alone",
        ))
    }

    /// Refuses the operation `serve`, whose sockets are those of Linux.
    pub(super) fn op_serve(_request: &JsObject) -> Result<JsObject, JsString> {
        Err(JsString::from(
            "serve: the card test-graphtp serves on Linux alone",
        ))
    }
}

/// The name of the primitive.
const NAME: &str = "graphtp";

/// The milestone the primitive lands at, track B.
const LANDS_AT: &str = "M12";

/// The operations the batch form answers, by the `op` that names each.
const OPERATIONS: &Operations = &[
    ("status-byte", op_status_byte),
    ("status-of", op_status_of),
    ("request-head", op_request_head),
    ("read-request-head", op_read_request_head),
    ("answer-head", op_answer_head),
    ("read-answer-head", op_read_answer_head),
    ("credential", op_credential),
    ("registry", op_registry),
    ("deflate", op_deflate),
    ("inflate", op_inflate),
    ("check-header", op_check_header),
    ("adler32", op_adler32),
    ("http-coding", op_http_coding),
    ("write-request", op_write_request),
    ("read-request", op_read_request),
    ("write-answer", op_write_answer),
    ("read-answer", op_read_answer),
    ("build-request", op_build_request),
    ("build-answer", op_build_answer),
    ("stream", op_stream),
    ("entries", op_entries),
    ("card", card::op_card),
    ("serve", card::op_serve),
    ("cache", cache::op_cache),
    ("substatus", substatus::op_substatus),
];

/// The bound on a decoded content that `read-request` takes when the request
/// names none, 1 MiB.
const DEFAULT_MAX_CONTENT: u64 = 1 << 20;

/// The bound on a decoded body that `read-answer` takes when the request names
/// none, 16 MiB.
const DEFAULT_BOUND: u64 = 1 << 24;

/// The constants of the registry of `docs/graphtp.md` section 2.5, by the name
/// each carries in the crate.
const REGISTRY: &[(&str, Code)] = &[
    ("OK", Code::OK),
    ("PARTIAL", Code::PARTIAL),
    ("NO_DATA", Code::NO_DATA),
    ("STREAM_OPENED", Code::STREAM_OPENED),
    ("SYNTAX_ERROR", Code::SYNTAX_ERROR),
    ("VALIDATION_ERROR", Code::VALIDATION_ERROR),
    ("INVALID_VARIABLES", Code::INVALID_VARIABLES),
    ("OPERATION_NOT_FOUND", Code::OPERATION_NOT_FOUND),
    ("QUERY_TOO_COMPLEX", Code::QUERY_TOO_COMPLEX),
    ("INVALID_REQUEST_BODY", Code::INVALID_REQUEST_BODY),
    ("MALFORMED_MESSAGE", Code::MALFORMED_MESSAGE),
    ("BAD_CODING", Code::BAD_CODING),
    ("STREAM_NOT_ALLOWED", Code::STREAM_NOT_ALLOWED),
    ("UNKNOWN_STREAM", Code::UNKNOWN_STREAM),
    ("PULL_ALREADY_PARKED", Code::PULL_ALREADY_PARKED),
    ("CONTENT_TOO_LARGE", Code::CONTENT_TOO_LARGE),
    ("UNSUPPORTED_CODING", Code::UNSUPPORTED_CODING),
    ("VALIDATION_ERROR_422", Code::VALIDATION_ERROR_422),
    ("INVALID_VARIABLES_422", Code::INVALID_VARIABLES_422),
    ("OPERATION_NOT_FOUND_422", Code::OPERATION_NOT_FOUND_422),
    ("QUERY_TOO_COMPLEX_422", Code::QUERY_TOO_COMPLEX_422),
    ("INVALID_REQUEST_BODY_422", Code::INVALID_REQUEST_BODY_422),
    ("TOO_MANY_STREAMS", Code::TOO_MANY_STREAMS),
    (
        "REQUEST_HEADER_FIELDS_TOO_LARGE",
        Code::REQUEST_HEADER_FIELDS_TOO_LARGE,
    ),
    ("INTERNAL_SERVER_ERROR", Code::INTERNAL_SERVER_ERROR),
    ("ANSWER_TOO_LARGE", Code::ANSWER_TOO_LARGE),
    ("STATUS_NOT_REPRESENTABLE", Code::STATUS_NOT_REPRESENTABLE),
    ("UNSUPPORTED_ANSWER_TYPE", Code::UNSUPPORTED_ANSWER_TYPE),
    ("NOT_IMPLEMENTED", Code::NOT_IMPLEMENTED),
    ("BAD_GATEWAY", Code::BAD_GATEWAY),
    ("UNKNOWN_OUTCOME", Code::UNKNOWN_OUTCOME),
    ("LINK_DOWN", Code::LINK_DOWN),
    ("INSUFFICIENT_MEMORY", Code::INSUFFICIENT_MEMORY),
    ("GATEWAY_TIMEOUT", Code::GATEWAY_TIMEOUT),
    (
        "HTTP_VERSION_NOT_SUPPORTED",
        Code::HTTP_VERSION_NOT_SUPPORTED,
    ),
];

/// The command line of the `graphtp` primitive.
pub static GRAMMAR: Grammar = Grammar {
    options: &[cli::SMOKE, cli::REQUESTS],
    positional: None,
    conflicts: &[],
};

/// What the `graphtp` primitive accepts.
#[derive(Debug)]
pub struct GraphtpArgs {
    /// Whether the call asks for the interface line.
    smoke: bool,
    /// The file of request lines of the batch form.
    requests: Option<PathBuf>,
}

impl GraphtpArgs {
    /// Reads the arguments `matches` holds for [`GRAMMAR`].
    pub fn read(matches: &Matches) -> Self {
        Self {
            smoke: matches.flag("smoke"),
            requests: matches.path("requests"),
        }
    }
}

/// Runs the `graphtp` primitive.
pub fn run(args: &GraphtpArgs) -> ExitCode {
    if args.smoke {
        return seam::smoke_at(NAME, LANDS_AT);
    }
    let outcome = seam::required(args.requests.as_deref(), "--requests")
        .and_then(|requests| batch::run(OPERATIONS, requests));
    seam::finish(NAME, outcome)
}

/// Answers the operation `status-byte`.
fn op_status_byte(request: &JsObject) -> Result<JsObject, JsString> {
    let status = integer(request, "status", u16::MAX.into())?;
    let byte = u16::try_from(status)
        .ok()
        .and_then(Status::from_u16)
        .map(Status::to_byte);
    let mut payload = JsObject::with_capacity(1);
    payload.insert(
        "byte",
        byte.map_or(JsValue::Null, |byte| u32::from(byte).into()),
    );
    Ok(payload)
}

/// Answers the operation `status-of`.
fn op_status_of(request: &JsObject) -> Result<JsObject, JsString> {
    let byte = byte_field(request, "byte")?;
    let substatus = optional_byte(request, "substatus")?.unwrap_or(0);
    let status = Status::from_byte(byte);
    let code = Code::new(status, Substatus::new(substatus));
    let mut payload = JsObject::with_capacity(5);
    payload.insert("status", u32::from(status.as_u16()));
    payload.insert("class", u32::from(status.class()));
    payload.insert("reason", text_or_null(status.canonical_reason()));
    payload.insert("code", code.to_string());
    payload.insert("name", text_or_null(code.name()));
    Ok(payload)
}

/// Answers the operation `request-head`.
fn op_request_head(request: &JsObject) -> Result<JsObject, JsString> {
    let version = byte_field(request, "version")?;
    let kind = byte_field(request, "kind")?;
    let compressed = boolean(request, "compressed")?;
    let credential_len = integer(request, "credential_len", u16::MAX.into())?;
    let credential_len =
        u16::try_from(credential_len).map_err(|_| field_error(request, "credential_len"))?;
    let (Some(version), Some(kind)) = (Version::from_bits(version), Kind::from_bits(kind)) else {
        return Ok(no_hex());
    };
    let head = RequestHead {
        version,
        kind,
        compressed,
        credential_len,
    };
    write_through_sink(request, &head.to_bytes())
}

/// Answers the operation `read-request-head`.
fn op_read_request_head(request: &JsObject) -> Result<JsObject, JsString> {
    let message = hex_field(request, "hex")?;
    let mut payload = JsObject::with_capacity(1);
    match RequestHead::parse(&message) {
        Ok(head) => {
            let mut fields = JsObject::with_capacity(4);
            fields.insert("version", u32::from(head.version.bits()));
            fields.insert("kind", u32::from(head.kind.bits()));
            fields.insert("compressed", head.compressed);
            fields.insert("credential_len", u32::from(head.credential_len));
            payload.insert("head", fields);
        }
        Err(code) => {
            payload.insert("refused", code.to_string());
        }
    }
    Ok(payload)
}

/// Answers the operation `answer-head`.
fn op_answer_head(request: &JsObject) -> Result<JsObject, JsString> {
    let status = integer(request, "status", u16::MAX.into())?;
    let substatus = byte_field(request, "substatus")?;
    let Some(status) = u16::try_from(status).ok().and_then(Status::from_u16) else {
        return Ok(no_hex());
    };
    let head = AnswerHead {
        code: Code::new(status, Substatus::new(substatus)),
    };
    write_through_sink(request, &head.to_bytes())
}

/// Answers the operation `read-answer-head`.
fn op_read_answer_head(request: &JsObject) -> Result<JsObject, JsString> {
    let message = hex_field(request, "hex")?;
    let mut payload = JsObject::with_capacity(3);
    let head = match AnswerHead::parse(&message) {
        Ok(head) => head,
        Err(error) => {
            payload.insert("malformed", wire_error_name(error));
            return Ok(payload);
        }
    };
    payload.insert("code", head.code.to_string());
    payload.insert("name", text_or_null(head.code.name()));
    let rest = message.get(AnswerHead::LEN..).unwrap_or_default();
    match body::coding_of(rest) {
        Ok(coding) => {
            let coding = match coding {
                None => "empty",
                Some(Coding::Plain) => "plain",
                Some(Coding::Zlib) => "zlib",
            };
            payload.insert("coding", coding);
        }
        Err(error) => {
            payload.insert("malformed", wire_error_name(error));
        }
    }
    Ok(payload)
}

/// Answers the operation `credential`.
fn op_credential(request: &JsObject) -> Result<JsObject, JsString> {
    let bytes = hex_field(request, "hex")?;
    let max = match request.get("max_credential") {
        None | Some(JsValue::Undefined | JsValue::Null) => None,
        Some(_) => {
            let max = integer(request, "max_credential", u16::MAX.into())?;
            Some(u16::try_from(max).map_err(|_| field_error(request, "max_credential"))?)
        }
    };
    let checked = Credential::new(&bytes).and_then(|credential| match max {
        Some(max) => credential.within(max),
        None => Ok(credential),
    });
    let mut payload = JsObject::with_capacity(1);
    match checked {
        Ok(credential) => {
            payload.insert("credential", hex(credential.as_bytes()));
        }
        Err(code) => {
            payload.insert("refused", code.to_string());
        }
    }
    Ok(payload)
}

/// Answers the operation `registry`.
///
/// The payload is `codes`, one object per constant of the registry with the
/// name of the constant, its code and the name of the code, in the order of
/// the rows of the registry.
// The batch form calls every operation through the one type `batch::Operation`,
// whose answer is a `Result`, and this operation reads no field that could
// refuse the request.
#[expect(clippy::unnecessary_wraps)]
fn op_registry(_request: &JsObject) -> Result<JsObject, JsString> {
    let codes = REGISTRY
        .iter()
        .map(|(constant, code)| {
            let mut entry = JsObject::with_capacity(3);
            entry.insert("constant", *constant);
            entry.insert("code", code.to_string());
            entry.insert("name", text_or_null(code.name()));
            JsValue::Object(entry)
        })
        .collect::<Vec<_>>();
    let mut payload = JsObject::with_capacity(1);
    payload.insert("codes", codes);
    Ok(payload)
}

/// Writes `bytes` through the sink the request names and answers what it
/// holds.
///
/// The answer is `hex` when the sink took the bytes, and `full` with the hex
/// of what the sink holds when it had no room.
fn write_through_sink(request: &JsObject, bytes: &[u8]) -> Result<JsObject, JsString> {
    through_sink(request, |sink| sink.put(bytes))
}

/// Runs the writer `write` into the sink the request names and answers what
/// the sink holds.
///
/// The answer is `hex` when every put of the writer fit, and `full` with the
/// hex of what the sink holds when one had no room.
fn through_sink(
    request: &JsObject,
    write: impl FnOnce(&mut dyn Sink) -> Result<(), Full>,
) -> Result<JsObject, JsString> {
    let kind = match request.get("sink") {
        None | Some(JsValue::Undefined | JsValue::Null) => "vec",
        Some(JsValue::String(kind)) => kind.as_str().unwrap_or_default(),
        Some(_) => return Err(field_error(request, "sink")),
    };
    let (taken, held) = match kind {
        "vec" => {
            let mut sink = Vec::new();
            let taken = write(&mut sink).is_ok();
            (taken, sink)
        }
        "slice" => {
            let capacity = integer(request, "capacity", u32::MAX.into())?;
            let mut buf = vec![0; usize::try_from(capacity).unwrap_or(usize::MAX)];
            let mut sink = Slice::new(&mut buf);
            let taken = write(&mut sink).is_ok();
            (taken, sink.written().to_vec())
        }
        "pages" => {
            let count = integer(request, "pages", 1 << 16)?;
            let size = integer(request, "page_size", 1 << 16)?;
            let size = usize::try_from(size).unwrap_or(usize::MAX);
            let mut pages = vec![vec![0; size]; usize::try_from(count).unwrap_or(usize::MAX)];
            let mut sink = Pages::new(&mut pages);
            let taken = write(&mut sink).is_ok();
            let len = sink.len();
            let held: Vec<u8> = pages.concat().into_iter().take(len).collect();
            (taken, held)
        }
        _ => return Err(field_error(request, "sink")),
    };
    let mut payload = JsObject::with_capacity(2);
    if taken {
        payload.insert("hex", hex(&held));
    } else {
        payload.insert("full", true);
        payload.insert("written", hex(&held));
    }
    Ok(payload)
}

/// Answers the operation `deflate`.
fn op_deflate(request: &JsObject) -> Result<JsObject, JsString> {
    let plain = hex_field(request, "hex")?;
    let raw = optional_boolean(request, "raw")?.unwrap_or(false);
    let (params, chunk) = encoder_sizes(request, zlib::WINDOW_BITS, plain.len())?;
    let (mut payload, bytes) = if raw {
        let mut scratch = match deflate::Scratch::new(params) {
            Ok(scratch) => scratch,
            Err(error) => return Ok(scratch_refused(error)),
        };
        let bytes = scratch.bytes();
        let payload = through_sink(request, |sink| {
            let mut encoder = deflate::Encoder::new(&mut scratch);
            for piece in plain.chunks(chunk) {
                encoder.write(piece, sink)?;
            }
            encoder.finish(sink)
        })?;
        (payload, bytes)
    } else {
        let mut scratch = match zlib::Scratch::new(params) {
            Ok(scratch) => scratch,
            Err(error) => return Ok(scratch_refused(error)),
        };
        let bytes = scratch.bytes();
        let payload = through_sink(request, |sink| {
            let mut encoder = zlib::Encoder::new(&mut scratch);
            for piece in plain.chunks(chunk) {
                encoder.write(piece, sink)?;
            }
            encoder.finish(sink)
        })?;
        (payload, bytes)
    };
    payload.insert("scratch", count(bytes));
    Ok(payload)
}

/// Returns the sizes of the scratch a request names, over the defaults of
/// `default_bits` when it names no window, and the pieces the plain bytes of
/// `len` bytes travel in, all of them in one when it names no `chunk`.
fn encoder_sizes(
    request: &JsObject,
    default_bits: u8,
    len: usize,
) -> Result<(deflate::Params, usize), JsString> {
    let window_bits = match optional_integer(request, "window_bits", u8::MAX.into())? {
        Some(bits) => u8::try_from(bits).map_err(|_| field_error(request, "window_bits"))?,
        None => default_bits,
    };
    let mut params = deflate::Params::new(window_bits);
    for (name, size) in [
        ("heads", &mut params.heads),
        ("tokens", &mut params.tokens),
        ("buffer", &mut params.buffer),
    ] {
        if let Some(value) = optional_integer(request, name, u32::MAX.into())? {
            *size = usize::try_from(value).map_err(|_| field_error(request, name))?;
        }
    }
    let chunk = match optional_integer(request, "chunk", u32::MAX.into())? {
        Some(0) => return Err(field_error(request, "chunk")),
        Some(chunk) => usize::try_from(chunk).map_err(|_| field_error(request, "chunk"))?,
        None => len.max(1),
    };
    Ok((params, chunk))
}

/// Returns the payload of sizes that make no scratch, `error` with the
/// reason.
fn scratch_refused(error: deflate::ScratchError) -> JsObject {
    let mut payload = JsObject::with_capacity(1);
    payload.insert(
        "error",
        match error {
            deflate::ScratchError::Params => "params",
            deflate::ScratchError::Memory => "memory",
        },
    );
    payload
}

/// Answers the operation `inflate`.
fn op_inflate(request: &JsObject) -> Result<JsObject, JsString> {
    let stream = hex_field(request, "hex")?;
    let bound = integer(request, "bound", u32::MAX.into())?;
    let bound = usize::try_from(bound).map_err(|_| field_error(request, "bound"))?;
    let streaming = optional_boolean(request, "streaming")?.unwrap_or(false);
    let raw = optional_boolean(request, "raw")?.unwrap_or(false);
    let window = match optional_integer(request, "window", u32::MAX.into())? {
        Some(window) => usize::try_from(window).map_err(|_| field_error(request, "window"))?,
        None => zlib::WINDOW,
    };
    let mut memory = zeroed(if streaming { window } else { bound })?;
    let mut plain = Vec::new();
    let mut payload = JsObject::with_capacity(2);
    if raw {
        let decoder = deflate::Decoder::new(bound);
        let outcome = if streaming {
            decoder.streaming(&stream, &mut memory, &mut plain)
        } else {
            decoder.whole(&stream, &mut memory)
        };
        match outcome {
            Ok(read) => {
                let plain = if streaming {
                    &plain
                } else {
                    memory.get(..read.produced).unwrap_or_default()
                };
                payload.insert("hex", hex(plain));
                payload.insert("consumed", count(read.consumed));
            }
            Err(error) => {
                payload.insert("error", deflate_error_name(error));
            }
        }
        return Ok(payload);
    }
    let decoder = zlib::Decoder::new(bound);
    let outcome = if streaming {
        decoder.streaming(&stream, &mut memory, &mut plain)
    } else {
        decoder.whole(&stream, &mut memory)
    };
    match outcome {
        Ok(produced) => {
            let plain = if streaming {
                &plain
            } else {
                memory.get(..produced).unwrap_or_default()
            };
            payload.insert("hex", hex(plain));
        }
        Err(error) => {
            payload.insert("refused", error.code().to_string());
            payload.insert("error", zlib_error_name(error));
        }
    }
    Ok(payload)
}

/// Answers the operation `check-header`.
fn op_check_header(request: &JsObject) -> Result<JsObject, JsString> {
    let cmf = byte_field(request, "cmf")?;
    let flg = byte_field(request, "flg")?;
    let mut payload = JsObject::with_capacity(1);
    match zlib::check_header(cmf, flg) {
        Ok(()) => {
            payload.insert("ok", true);
        }
        Err(code) => {
            payload.insert("refused", code.to_string());
        }
    }
    Ok(payload)
}

/// Answers the operation `adler32`.
fn op_adler32(request: &JsObject) -> Result<JsObject, JsString> {
    let bytes = hex_field(request, "hex")?;
    let mut payload = JsObject::with_capacity(1);
    payload.insert("adler32", zlib::adler32(&bytes));
    Ok(payload)
}

/// Answers the operation `http-coding`.
fn op_http_coding(request: &JsObject) -> Result<JsObject, JsString> {
    let plain = hex_field(request, "hex")?;
    match request.get("coding") {
        Some(JsValue::String(name)) if name.as_str() == Some("deflate") => {}
        _ => return Err(field_error(request, "coding")),
    }
    let (params, chunk) = encoder_sizes(request, http::SERVER_WINDOW_BITS, plain.len())?;
    let mut scratch = match deflate::Scratch::new(params) {
        Ok(scratch) => scratch,
        Err(error) => return Ok(scratch_refused(error)),
    };
    let bytes = scratch.bytes();
    let mut payload = through_sink(request, |sink| {
        let mut encoder = http::zlib::Encoder::new(&mut scratch);
        for piece in plain.chunks(chunk) {
            encoder.write(piece, sink)?;
        }
        encoder.finish(sink)
    })?;
    payload.insert("scratch", count(bytes));
    Ok(payload)
}

/// Answers the operation `write-request`.
fn op_write_request(request: &JsObject) -> Result<JsObject, JsString> {
    match byte_field(request, "kind")? {
        0 => {
            let credential = optional_hex(request, "credential")?.unwrap_or_default();
            let content = hex_field(request, "content")?;
            let credential = match Credential::new(&credential) {
                Ok(credential) => (!credential.is_empty()).then_some(credential),
                Err(code) => return Ok(refused(code)),
            };
            let mut scratch = profile_scratch()?;
            through_sink(request, |sink| {
                wire::write_operation(sink, credential, &content, &mut scratch)
            })
        }
        1 => through_sink(request, wire::write_pull),
        2 => {
            let handle = integer(request, "handle", u16::MAX.into())?;
            let handle = u16::try_from(handle).map_err(|_| field_error(request, "handle"))?;
            through_sink(request, |sink| {
                wire::write_cancel(sink, Target::from_u16(handle))
            })
        }
        _ => Err(field_error(request, "kind")),
    }
}

/// Answers the operation `read-request`.
fn op_read_request(request: &JsObject) -> Result<JsObject, JsString> {
    let message = hex_field(request, "hex")?;
    let max_credential =
        optional_integer(request, "max_credential", u16::MAX.into())?.unwrap_or(u16::MAX.into());
    let max_credential =
        u16::try_from(max_credential).map_err(|_| field_error(request, "max_credential"))?;
    let max_content =
        optional_integer(request, "max_content", u32::MAX.into())?.unwrap_or(DEFAULT_MAX_CONTENT);
    let max_content = usize::try_from(max_content).unwrap_or(usize::MAX);
    let decoded = match wire::decode_request(&message, max_credential) {
        Ok(decoded) => decoded,
        Err(code) => return Ok(refused(code)),
    };
    let mut payload = JsObject::with_capacity(4);
    match decoded {
        Request::Operation {
            credential,
            content,
        } => {
            let mut plain = Vec::new();
            if let Err(code) = wire::decode_content(content, max_content, &mut plain) {
                return Ok(refused(code));
            }
            let coding = match content {
                Content::Plain(_) => "plain",
                Content::Zlib(_) => "zlib",
            };
            payload.insert("kind", 0);
            payload.insert(
                "credential",
                credential.map_or(JsValue::Null, |credential| {
                    hex(credential.as_bytes()).into()
                }),
            );
            payload.insert("coding", coding);
            payload.insert("content", hex(&plain));
        }
        Request::Pull => {
            payload.insert("kind", 1);
        }
        Request::Cancel(target) => {
            payload.insert("kind", 2);
            payload.insert("handle", u32::from(target.to_u16()));
        }
    }
    Ok(payload)
}

/// Answers the operation `write-answer`.
fn op_write_answer(request: &JsObject) -> Result<JsObject, JsString> {
    let status = integer(request, "status", u16::MAX.into())?;
    let substatus = byte_field(request, "substatus")?;
    let body = hex_field(request, "body")?;
    let Some(status) = u16::try_from(status).ok().and_then(Status::from_u16) else {
        return Ok(no_hex());
    };
    let code = Code::new(status, Substatus::new(substatus));
    let mut scratch = profile_scratch()?;
    through_sink(request, |sink| {
        wire::write_answer(sink, code, &body, &mut scratch)
    })
}

/// Answers the operation `read-answer`.
fn op_read_answer(request: &JsObject) -> Result<JsObject, JsString> {
    let kind = byte_field(request, "kind")?;
    let kind = Kind::from_bits(kind).ok_or_else(|| field_error(request, "kind"))?;
    let message = hex_field(request, "hex")?;
    let bound = optional_integer(request, "bound", u32::MAX.into())?.unwrap_or(DEFAULT_BOUND);
    let bound = usize::try_from(bound).unwrap_or(usize::MAX);
    let answer = match wire::decode_answer(kind, &message) {
        Ok(answer) => answer,
        Err(error) => return Ok(malformed(error)),
    };
    let (shape, code, body) = match answer {
        Answer::Refused(code) => {
            let mut payload = JsObject::with_capacity(3);
            payload.insert("answer", "refused");
            payload.insert("code", code.to_string());
            payload.insert("name", text_or_null(code.name()));
            return Ok(payload);
        }
        Answer::Result { code, body } => ("result", code, body),
        Answer::Opened { code, head } => ("opened", code, head),
        Answer::Batch { code, body } => ("batch", code, body),
    };
    let mut plain = Vec::new();
    if let Err(error) = wire::decode_body(body, bound, &mut plain) {
        return Ok(malformed(error));
    }
    let mut payload = JsObject::with_capacity(8);
    payload.insert("answer", shape);
    payload.insert("code", code.to_string());
    payload.insert("name", text_or_null(code.name()));
    payload.insert(
        "coding",
        match body {
            wire::Body::Plain(_) => "plain",
            wire::Body::Zlib(_) => "zlib",
        },
    );
    payload.insert("body", hex(&plain));
    if shape == "result" {
        return Ok(payload);
    }
    let read = match read_stream_body(&plain) {
        Ok(read) => read,
        Err(error) => return Ok(malformed(error)),
    };
    if shape == "opened" {
        let Some(handle) = read.handle else {
            return Ok(malformed(WireError::Stream));
        };
        payload.insert("stream", u32::from(handle.get()));
    }
    payload.insert("seq", read.seq.get());
    payload.insert("entries", read.entries);
    Ok(payload)
}

/// Answers the operation `build-request`.
///
/// The writer checks the credential of the built request again, as it checks
/// every credential it is handed, since the parts of a request are public.
fn op_build_request(request: &JsObject) -> Result<JsObject, JsString> {
    let credentials = hex_values(request, "credential")?;
    let content = hex_field(request, "content")?;
    let builder = credentials
        .into_iter()
        .fold(graphtp::request::Request::builder(), |builder, value| {
            builder.credential(value)
        });
    let built = match builder.body(content) {
        Ok(built) => built,
        Err(code) => return Ok(refused(code)),
    };
    let credential = match built.credential().map(Credential::new).transpose() {
        Ok(credential) => credential,
        Err(code) => return Ok(refused(code)),
    };
    let mut scratch = profile_scratch()?;
    through_sink(request, |sink| {
        wire::write_operation(sink, credential, built.body(), &mut scratch)
    })
}

/// Answers the operation `build-answer`.
fn op_build_answer(request: &JsObject) -> Result<JsObject, JsString> {
    let statuses = numbers(request, "status", u16::MAX)?;
    let substatus = byte_field(request, "substatus")?;
    let body = hex_field(request, "body")?;
    let builder = statuses
        .into_iter()
        .fold(graphtp::response::Response::builder(), |builder, status| {
            builder.status(status)
        });
    let built = match builder.substatus(substatus).body(body) {
        Ok(built) => built,
        Err(code) => return Ok(refused(code)),
    };
    let mut scratch = profile_scratch()?;
    through_sink(request, |sink| {
        wire::write_answer(sink, built.code(), built.body(), &mut scratch)
    })
}

/// Answers the operation `stream`.
fn op_stream(request: &JsObject) -> Result<JsObject, JsString> {
    let handle = match optional_integer(request, "stream", u16::MAX.into())? {
        Some(value) => Some(handle_of(request, "stream", value)?),
        None => None,
    };
    let seq = integer(request, "seq", u32::MAX.into())?;
    let seq = Seq::new(u32::try_from(seq).map_err(|_| field_error(request, "seq"))?);
    let Some(JsValue::Array(items)) = request.get("entries") else {
        return Err(field_error(request, "entries"));
    };
    let mut values = Vec::with_capacity(items.len());
    for item in items {
        values.push(entry_of(request, item)?);
    }
    through_sink(request, |sink| {
        let mut events = match handle {
            Some(handle) => stream::write_head(sink, handle, seq)?,
            None => stream::write_batch(sink, seq)?,
        };
        for (handle, mark, value) in &values {
            let entry = match *mark {
                "next" => Entry::Next(*handle, value),
                "complete" => Entry::Complete(*handle),
                "cancelled" => Entry::Cancelled(*handle),
                _ => Entry::Aborted(*handle),
            };
            stream::write_entry(sink, &mut events, &entry)?;
        }
        stream::close(sink, events).map(|_| ())
    })
}

/// Answers the operation `entries`.
fn op_entries(request: &JsObject) -> Result<JsObject, JsString> {
    let body = hex_field(request, "hex")?;
    let read = match read_stream_body(&body) {
        Ok(read) => read,
        Err(error) => return Ok(malformed(error)),
    };
    let mut payload = JsObject::with_capacity(3);
    payload.insert(
        "stream",
        read.handle
            .map_or(JsValue::Null, |handle| u32::from(handle.get()).into()),
    );
    payload.insert("seq", read.seq.get());
    payload.insert("entries", read.entries);
    Ok(payload)
}

/// A plain stream body as `read-answer` and `entries` answer it.
struct StreamBody {
    /// The handle of a head, `None` for a batch.
    handle: Option<Handle>,
    /// The batch number.
    seq: Seq,
    /// The entries, each as the object a response carries.
    entries: Vec<JsValue>,
}

/// Reads the plain stream body `body` with [`Entries`].
fn read_stream_body(body: &[u8]) -> Result<StreamBody, WireError> {
    let entries = Entries::new(body)?;
    let handle = entries.handle();
    let seq = entries.seq();
    let mut read = Vec::new();
    for entry in entries {
        let entry = entry?;
        let mut object = JsObject::with_capacity(2);
        object.insert("stream", u32::from(entry.handle().get()));
        let (mark, value) = match entry {
            Entry::Next(_, value) => ("next", JsValue::from(hex(value))),
            Entry::Complete(_) => ("complete", JsValue::Bool(true)),
            Entry::Cancelled(_) => ("cancelled", JsValue::Bool(true)),
            Entry::Aborted(_) => ("aborted", JsValue::Bool(true)),
        };
        object.insert(mark, value);
        read.push(JsValue::Object(object));
    }
    Ok(StreamBody {
        handle,
        seq,
        entries: read,
    })
}

/// Returns the handle, the mark and the value of the entry object `item` of
/// the operation `stream`.
fn entry_of(
    request: &JsObject,
    item: &JsValue,
) -> Result<(Handle, &'static str, Vec<u8>), JsString> {
    let JsValue::Object(item) = item else {
        return Err(field_error(request, "entries"));
    };
    let value =
        integer(item, "stream", u16::MAX.into()).map_err(|_| field_error(request, "entries"))?;
    let handle = handle_of(request, "entries", value)?;
    if item.get("next").is_some() {
        let value = hex_field(item, "next").map_err(|_| field_error(request, "entries"))?;
        return Ok((handle, "next", value));
    }
    for mark in ["complete", "cancelled", "aborted"] {
        if item.get(mark) == Some(&JsValue::Bool(true)) {
            return Ok((handle, mark, Vec::new()));
        }
    }
    Err(field_error(request, "entries"))
}

/// Returns the handle of the number `value` of the field `name`.
fn handle_of(request: &JsObject, name: &str, value: u64) -> Result<Handle, JsString> {
    u16::try_from(value)
        .ok()
        .and_then(Handle::new)
        .ok_or_else(|| field_error(request, name))
}

/// Returns the scratch of the zlib profile, or the `fatal` of an allocation
/// the devtool cannot make.
fn profile_scratch() -> Result<zlib::Scratch, JsString> {
    zlib::Scratch::new(zlib::PARAMS)
        .map_err(|error| JsString::from(format!("the devtool cannot make a scratch: {error}")))
}

/// Returns the payload of a request the crate refuses, `refused` with the
/// code.
fn refused(code: Code) -> JsObject {
    let mut payload = JsObject::with_capacity(1);
    payload.insert("refused", code.to_string());
    payload
}

/// Returns the payload of an answer the crate cannot read, `malformed` with
/// the reason.
fn malformed(error: WireError) -> JsObject {
    let mut payload = JsObject::with_capacity(1);
    payload.insert("malformed", wire_error_name(error));
    payload
}

/// Returns the name a response gives the reason raw DEFLATE data was not
/// decoded.
fn deflate_error_name(error: deflate::Error) -> &'static str {
    match error {
        deflate::Error::Bound => "bound",
        deflate::Error::Corrupt => "corrupt",
    }
}

/// Returns the name a response gives the reason a zlib stream was refused.
fn zlib_error_name(error: zlib::Error) -> &'static str {
    match error {
        zlib::Error::Bound => "bound",
        zlib::Error::Unsupported => "unsupported",
        zlib::Error::Corrupt => "corrupt",
        zlib::Error::Checksum => "checksum",
        zlib::Error::Trailing => "trailing",
    }
}

/// Returns `len` zero bytes, or the `fatal` of an allocation the devtool
/// cannot make.
fn zeroed(len: usize) -> Result<Vec<u8>, JsString> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(len)
        .map_err(|_| JsString::from(format!("the devtool cannot allocate {len} bytes")))?;
    bytes.resize(len, 0);
    Ok(bytes)
}

/// Returns the count `value` as a number.
fn count(value: usize) -> JsValue {
    u64::try_from(value).map_or(JsValue::Null, JsValue::from)
}

/// Returns the payload of a head that has no bytes, `hex` as null.
fn no_hex() -> JsObject {
    let mut payload = JsObject::with_capacity(1);
    payload.insert("hex", JsValue::Null);
    payload
}

/// Returns the name a response gives the reason an answer is malformed.
fn wire_error_name(error: WireError) -> &'static str {
    match error {
        WireError::Short => "short",
        WireError::Coding => "coding",
        WireError::Empty => "empty",
        WireError::Body => "body",
        WireError::Decode => "decode",
        WireError::Bound => "bound",
        WireError::Stream => "stream",
    }
}

/// Returns `text` as a string value, or null for none.
fn text_or_null(text: Option<&str>) -> JsValue {
    text.map_or(JsValue::Null, JsValue::from)
}

/// Returns the bytes of the hex string field `name` of `request`.
///
/// # Errors
///
/// Returns a `fatal` when the field is missing, is not a string or is not an
/// even count of hex digits.
fn hex_field(request: &JsObject, name: &str) -> Result<Vec<u8>, JsString> {
    let text = batch::require_string(request, name)?;
    unhex(text).ok_or_else(|| field_error(request, name))
}

/// Returns the byte strings of the field `name` of `request`, one for a hex
/// string, each of an array of hex strings in its order, and none when the
/// request carries no such field or null.
///
/// # Errors
///
/// Returns a `fatal` when the field or an item of it is not a hex string.
fn hex_values(request: &JsObject, name: &str) -> Result<Vec<Vec<u8>>, JsString> {
    let bytes = |value: &JsValue| match value {
        JsValue::String(text) => text.as_str().and_then(unhex),
        _ => None,
    };
    match request.get(name) {
        None | Some(JsValue::Undefined | JsValue::Null) => Some(Vec::new()),
        Some(JsValue::Array(items)) => items.iter().map(bytes).collect(),
        Some(value) => bytes(value).map(|value| vec![value]),
    }
    .ok_or_else(|| field_error(request, name))
}

/// Returns the numbers of the field `name` of `request` as integers of 0 to
/// `max`, one for a number and each of an array of numbers in its order.
///
/// # Errors
///
/// Returns a `fatal` when the field is missing, or when it or an item of it is
/// not an integer of 0 to `max`.
fn numbers(request: &JsObject, name: &str, max: u16) -> Result<Vec<u16>, JsString> {
    let number = |value: &JsValue| {
        whole(value)
            .filter(|&value| value <= u64::from(max))
            .and_then(|value| u16::try_from(value).ok())
    };
    match request.get(name) {
        Some(JsValue::Array(items)) => items.iter().map(number).collect(),
        Some(value) => number(value).map(|value| vec![value]),
        None => None,
    }
    .ok_or_else(|| field_error(request, name))
}

/// Returns the number field `name` of `request` as an integer of 0 to `max`.
///
/// # Errors
///
/// Returns a `fatal` when the field is missing, is not a number, or is not an
/// integer of 0 to `max`.
fn integer(request: &JsObject, name: &str, max: u64) -> Result<u64, JsString> {
    request
        .get(name)
        .and_then(whole)
        .filter(|&value| value <= max)
        .ok_or_else(|| field_error(request, name))
}

/// Returns the number field `name` of `request` as a byte.
fn byte_field(request: &JsObject, name: &str) -> Result<u8, JsString> {
    let value = integer(request, name, u8::MAX.into())?;
    u8::try_from(value).map_err(|_| field_error(request, name))
}

/// Returns the bytes of the hex string field `name` of `request`, or `None`
/// when the request carries no such field or null.
fn optional_hex(request: &JsObject, name: &str) -> Result<Option<Vec<u8>>, JsString> {
    match request.get(name) {
        None | Some(JsValue::Undefined | JsValue::Null) => Ok(None),
        Some(_) => hex_field(request, name).map(Some),
    }
}

/// Returns the number field `name` of `request` as a byte, or `None` when the
/// request carries no such field.
fn optional_byte(request: &JsObject, name: &str) -> Result<Option<u8>, JsString> {
    match request.get(name) {
        None | Some(JsValue::Undefined | JsValue::Null) => Ok(None),
        Some(_) => byte_field(request, name).map(Some),
    }
}

/// Returns the number field `name` of `request` as an integer of 0 to `max`,
/// or `None` when the request carries no such field.
fn optional_integer(request: &JsObject, name: &str, max: u64) -> Result<Option<u64>, JsString> {
    match request.get(name) {
        None | Some(JsValue::Undefined | JsValue::Null) => Ok(None),
        Some(_) => integer(request, name, max).map(Some),
    }
}

/// Returns the boolean field `name` of `request`, or `None` when the request
/// carries no such field.
fn optional_boolean(request: &JsObject, name: &str) -> Result<Option<bool>, JsString> {
    match request.get(name) {
        None | Some(JsValue::Undefined | JsValue::Null) => Ok(None),
        Some(_) => boolean(request, name).map(Some),
    }
}

/// Returns the boolean field `name` of `request`.
fn boolean(request: &JsObject, name: &str) -> Result<bool, JsString> {
    match request.get(name) {
        Some(JsValue::Bool(value)) => Ok(*value),
        _ => Err(field_error(request, name)),
    }
}

/// Returns the `fatal` of a field that is missing or holds no value the
/// operation reads.
fn field_error(request: &JsObject, name: &str) -> JsString {
    let op = match request.get("op") {
        Some(JsValue::String(op)) => op.as_str().unwrap_or("?").to_owned(),
        _ => String::from("?"),
    };
    format!("{op}: the field {name} holds no value this operation reads").into()
}
