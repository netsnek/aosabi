// SPDX-License-Identifier: MIT
/*
 * graphtp-wasm codec: every message of GraphTP/1 written and read whole
 * through the crate graphtp, as records of plain bytes the glue reads.
 *
 * No analogue in the reference. The messages, the refusals and the malformed
 * answers are those of docs/graphtp.md sections 2.4, 5.3 and 5.4, and each
 * operation answers what the operation of the same name of the primitive
 * graphtp of aosabi-devtool answers, in bytes in place of JSON.
 */

//! Every message of GraphTP/1 written and read whole.
//!
//! Each operation writes one answer record into the caller's vector. The
//! first byte of a record is its tag, and the rest its payload, every number
//! little endian:
//!
//! - [`TAG_BYTES`]: the bytes the operation answers, a message written or a
//!   request or an answer read, whose layout each operation names.
//! - [`TAG_REFUSED`]: the code a request is refused with, a status of 2 bytes
//!   and a substatus of 1.
//! - [`TAG_MALFORMED`]: the reason an answer is malformed, 1 byte by
//!   [`wire_error`].
//! - [`TAG_NONE`]: no answer, for a status with no status byte or a code with
//!   no name.

#![forbid(unsafe_code)]

use graphtp::{
    credential::Credential,
    kind::Kind,
    status::{
        Code,
        Status,
        Substatus, //
    },
    stream::{
        Entries,
        Entry,
        Target, //
    },
    wire::{
        self,
        Answer,
        Body,
        Content,
        Request,
        WireError, //
    },
    zlib,
};

/// The tag of an answer that carries bytes.
pub const TAG_BYTES: u8 = 0;

/// The tag of a refusal with its code.
pub const TAG_REFUSED: u8 = 1;

/// The tag of a malformed answer with its reason.
pub const TAG_MALFORMED: u8 = 2;

/// The tag of no answer.
pub const TAG_NONE: u8 = 3;

/// The operation that writes an OPERATION from a credential and a content.
pub const WRITE_OPERATION: u32 = 1;

/// The operation that writes a PULL.
pub const WRITE_PULL: u32 = 2;

/// The operation that writes a CANCEL of a handle, 0 for every stream.
pub const WRITE_CANCEL: u32 = 3;

/// The operation that writes an answer from a status, a substatus and a body.
pub const WRITE_ANSWER: u32 = 4;

/// The operation that reads a request whole under the bounds of a server.
pub const READ_REQUEST: u32 = 5;

/// The operation that reads an answer whole under the bound of a client.
pub const READ_ANSWER: u32 = 6;

/// The operation that reads a plain stream body.
pub const ENTRIES: u32 = 7;

/// The operation that names a code, the text of `statusText`.
pub const NAME: u32 = 8;

/// The memory the writers of the codec share, the encoder's scratch of the
/// profile, made on the first message that needs it.
#[derive(Default)]
pub struct Codec {
    /// The encoder's scratch.
    scratch: Option<zlib::Scratch>,
}

impl Codec {
    /// Returns a codec with no scratch made yet.
    #[must_use]
    pub const fn new() -> Self {
        Self { scratch: None }
    }

    /// Runs the operation `op` over the inputs `a` and `b` and the integers
    /// `x` and `y`, and writes its answer record into `out`.
    ///
    /// Returns false for an operation the codec does not know, and for a
    /// scratch or a message the allocator refuses, which write no record.
    pub fn run(&mut self, op: u32, a: &[u8], b: &[u8], x: u32, y: u32, out: &mut Vec<u8>) -> bool {
        match op {
            WRITE_OPERATION => self.write_operation(a, b, out),
            WRITE_PULL => written(out, wire::write_pull),
            WRITE_CANCEL => {
                let handle = u16::try_from(x).unwrap_or(u16::MAX);
                written(out, |sink| {
                    wire::write_cancel(sink, Target::from_u16(handle))
                })
            }
            WRITE_ANSWER => self.write_answer(x, y, b, out),
            READ_REQUEST => {
                read_request(a, u16::try_from(x).unwrap_or(u16::MAX), widen(y), out);
                true
            }
            READ_ANSWER => {
                read_answer(x, a, widen(y), out);
                true
            }
            ENTRIES => {
                entries(a, out);
                true
            }
            NAME => {
                name(x, y, out);
                true
            }
            _ => false,
        }
    }

    /// Writes an OPERATION with the credential `credential`, none when empty,
    /// and the content `content`.
    fn write_operation(&mut self, credential: &[u8], content: &[u8], out: &mut Vec<u8>) -> bool {
        let credential = match Credential::new(credential) {
            Ok(credential) => (!credential.is_empty()).then_some(credential),
            Err(code) => {
                refused(out, code);
                return true;
            }
        };
        let Some(scratch) = self.scratch() else {
            return false;
        };
        written(out, |sink| {
            wire::write_operation(sink, credential, content, scratch)
        })
    }

    /// Writes an answer of the status `status` and the substatus `substatus`
    /// with the body `body`, or no answer for a status with no byte.
    fn write_answer(
        &mut self,
        status: u32,
        substatus: u32,
        body: &[u8],
        out: &mut Vec<u8>,
    ) -> bool {
        let status = u16::try_from(status).ok().and_then(Status::from_u16);
        let (Some(status), Ok(substatus)) = (status, u8::try_from(substatus)) else {
            out.push(TAG_NONE);
            return true;
        };
        let code = Code::new(status, Substatus::new(substatus));
        let Some(scratch) = self.scratch() else {
            return false;
        };
        written(out, |sink| wire::write_answer(sink, code, body, scratch))
    }

    /// Returns the encoder's scratch of the profile, made on first use, or
    /// `None` where the allocator refuses it.
    fn scratch(&mut self) -> Option<&mut zlib::Scratch> {
        if self.scratch.is_none() {
            self.scratch = zlib::Scratch::new(zlib::PARAMS).ok();
        }
        self.scratch.as_mut()
    }
}

/// Writes the message `write` puts into its sink as a record of bytes, and
/// returns false where the allocator refuses the message.
fn written(
    out: &mut Vec<u8>,
    write: impl FnOnce(&mut dyn graphtp::sink::Sink) -> Result<(), graphtp::sink::Full>,
) -> bool {
    out.push(TAG_BYTES);
    write(out).is_ok()
}

/// Reads the request `message` by the 11 rules of `docs/graphtp.md` section
/// 5.3 under the bounds `max_credential` and `max_content`.
///
/// The payload of a request that passes is its kind, then for an OPERATION the
/// coding of its content, 0 plain and 1 zlib, a byte that tells whether it
/// carries a credential, the length of the credential in 2 bytes, the
/// credential and the decoded content, and for a CANCEL its handle in 2 bytes.
fn read_request(message: &[u8], max_credential: u16, max_content: usize, out: &mut Vec<u8>) {
    let request = match wire::decode_request(message, max_credential) {
        Ok(request) => request,
        Err(code) => return refused(out, code),
    };
    match request {
        Request::Operation {
            credential,
            content,
        } => {
            let mut plain = Vec::new();
            if let Err(code) = wire::decode_content(content, max_content, &mut plain) {
                return refused(out, code);
            }
            let coding = u8::from(matches!(content, Content::Zlib(_)));
            let credential = credential.map(Credential::as_bytes);
            out.extend_from_slice(&[TAG_BYTES, Kind::Operation.bits(), coding]);
            out.push(u8::from(credential.is_some()));
            let credential = credential.unwrap_or_default();
            put_u16(out, u16::try_from(credential.len()).unwrap_or(u16::MAX));
            out.extend_from_slice(credential);
            out.extend_from_slice(&plain);
        }
        Request::Pull => out.extend_from_slice(&[TAG_BYTES, Kind::Pull.bits()]),
        Request::Cancel(target) => {
            out.extend_from_slice(&[TAG_BYTES, Kind::Cancel.bits()]);
            put_u16(out, target.to_u16());
        }
    }
}

/// Reads the answer `message` to a request of the kind `kind` under the bound
/// `bound` on its decoded body.
///
/// The payload is the shape of the answer, 0 a refusal, 1 a result, 2 an
/// opened stream and 3 a batch, the status in 2 bytes, the substatus, and the
/// name of the code as [`put_named`] writes it. A refusal ends there. Every other shape adds the coding of its body, 0 plain
/// and 1 zlib, the length of the decoded body in 4 bytes and the body, and a
/// stream or a batch adds its stream body as [`entries`] writes its payload.
fn read_answer(kind: u32, message: &[u8], bound: usize, out: &mut Vec<u8>) {
    let Some(kind) = u8::try_from(kind).ok().and_then(Kind::from_bits) else {
        out.push(TAG_NONE);
        return;
    };
    let answer = match wire::decode_answer(kind, message) {
        Ok(answer) => answer,
        Err(error) => return malformed(out, error),
    };
    let (shape, code, body) = match answer {
        Answer::Refused(code) => {
            out.extend_from_slice(&[TAG_BYTES, 0]);
            return put_named(out, code);
        }
        Answer::Result { code, body } => (1, code, body),
        Answer::Opened { code, head } => (2, code, head),
        Answer::Batch { code, body } => (3, code, body),
    };
    let mut plain = Vec::new();
    if let Err(error) = wire::decode_body(body, bound, &mut plain) {
        return malformed(out, error);
    }
    let mut record = vec![TAG_BYTES, shape];
    put_named(&mut record, code);
    record.push(u8::from(matches!(body, Body::Zlib(_))));
    put_u32(&mut record, len_u32(plain.len()));
    record.extend_from_slice(&plain);
    if shape != 1 {
        let mut stream = Vec::new();
        if let Err(error) = stream_body(&plain, &mut stream) {
            return malformed(out, error);
        }
        if shape == 2 && stream.first() == Some(&0) {
            return malformed(out, WireError::Stream);
        }
        record.extend_from_slice(&stream);
    }
    out.extend_from_slice(&record);
}

/// Reads the plain stream body `body`.
///
/// The payload is a byte that tells whether the body is the head of a stream,
/// its handle in 2 bytes, 0 for a batch, the batch number in 4 bytes, the count
/// of entries in 4 bytes, and each entry as its mark, 0 `next`, 1 `complete`,
/// 2 `cancelled` and 3 `aborted`, its handle in 2 bytes, the length of its
/// value in 4 bytes and the value, empty but for `next`.
fn entries(body: &[u8], out: &mut Vec<u8>) {
    let mut stream = Vec::new();
    match stream_body(body, &mut stream) {
        Ok(()) => {
            out.push(TAG_BYTES);
            out.extend_from_slice(&stream);
        }
        Err(error) => malformed(out, error),
    }
}

/// Writes the payload of the stream body `body` into `out`, or the reason it
/// is malformed.
fn stream_body(body: &[u8], out: &mut Vec<u8>) -> Result<(), WireError> {
    let entries = Entries::new(body)?;
    out.push(u8::from(entries.handle().is_some()));
    put_u16(
        out,
        entries.handle().map_or(0, graphtp::stream::Handle::get),
    );
    put_u32(out, entries.seq().get());
    let mut read = Vec::new();
    let mut count = 0u32;
    for entry in entries {
        let entry = entry?;
        let (mark, value): (u8, &[u8]) = match entry {
            Entry::Next(_, value) => (0, value),
            Entry::Complete(_) => (1, &[]),
            Entry::Cancelled(_) => (2, &[]),
            Entry::Aborted(_) => (3, &[]),
        };
        read.push(mark);
        put_u16(&mut read, entry.handle().get());
        put_u32(&mut read, len_u32(value.len()));
        read.extend_from_slice(value);
        count = count.saturating_add(1);
    }
    put_u32(out, count);
    out.extend_from_slice(&read);
    Ok(())
}

/// Writes the name of the code of the status `status` and the substatus
/// `substatus`: the name of its row of the registry, the reason phrase of the
/// status for a `status.0` and for a substatus the registry does not hold, or
/// no answer where neither exists.
fn name(status: u32, substatus: u32, out: &mut Vec<u8>) {
    match text_of(status, substatus) {
        Some(text) => {
            out.push(TAG_BYTES);
            out.extend_from_slice(text.as_bytes());
        }
        None => out.push(TAG_NONE),
    }
}

/// Returns the text of `statusText` for the status `status` and the substatus
/// `substatus`, by `docs/graphtp-fetch.md` section 6.
#[must_use]
pub fn text_of(status: u32, substatus: u32) -> Option<&'static str> {
    let status = u16::try_from(status).ok().and_then(Status::from_u16)?;
    let substatus = Substatus::new(u8::try_from(substatus).ok()?);
    Code::new(status, substatus)
        .name()
        .or_else(|| status.canonical_reason())
}

/// Writes the refusal `code` as a record.
fn refused(out: &mut Vec<u8>, code: Code) {
    out.push(TAG_REFUSED);
    put_code(out, code);
}

/// Writes the malformed answer `error` as a record.
fn malformed(out: &mut Vec<u8>, error: WireError) {
    out.clear();
    out.extend_from_slice(&[TAG_MALFORMED, wire_error(error)]);
}

/// Returns the number of the reason an answer is malformed, 1 `short`, 2
/// `coding`, 3 `empty`, 4 `body`, 5 `decode`, 6 `bound` and 7 `stream`.
#[must_use]
pub fn wire_error(error: WireError) -> u8 {
    match error {
        WireError::Short => 1,
        WireError::Coding => 2,
        WireError::Empty => 3,
        WireError::Body => 4,
        WireError::Decode => 5,
        WireError::Bound => 6,
        WireError::Stream => 7,
    }
}

/// Appends the status in 2 bytes and the substatus of `code`.
pub fn put_code(out: &mut Vec<u8>, code: Code) {
    put_u16(out, code.status.as_u16());
    out.push(code.substatus.get());
}

/// Appends the status in 2 bytes and the substatus of `code`, then the length
/// of its name in 1 byte and the name, a length of 0 for a code with no name.
pub fn put_named(out: &mut Vec<u8>, code: Code) {
    put_code(out, code);
    let name = code.name().unwrap_or_default().as_bytes();
    out.push(u8::try_from(name.len()).unwrap_or(0));
    out.extend_from_slice(name);
}

/// Appends `value` in 2 bytes, little endian.
pub fn put_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

/// Appends `value` in 4 bytes, little endian.
pub fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

/// Returns the length `len` as a `u32`, `u32::MAX` where it passes it.
#[must_use]
pub fn len_u32(len: usize) -> u32 {
    u32::try_from(len).unwrap_or(u32::MAX)
}

/// Returns the bound `bound` as a `usize`, `usize::MAX` where it passes it.
#[must_use]
pub fn widen(bound: u32) -> usize {
    usize::try_from(bound).unwrap_or(usize::MAX)
}
