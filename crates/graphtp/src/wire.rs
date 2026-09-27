// SPDX-License-Identifier: MIT
/*
 * graphtp wire: the decoders and the writers of every message of the GraphTP/1
 * wire form.
 *
 * No analogue in the reference. The messages, the order of refusal and the
 * malformed answer are those of the specification, and the sender rule is that
 * of section 3.3.
 */

//! The wire form of GraphTP/1: the decoders of a request, its content and an
//! answer, and the writers of every message.
//!
//! A server calls [`decode_request`], which checks rules 1 to 9 of
//! `docs/graphtp.md` section 5.3 in their order, and then [`decode_content`]
//! on the content of an OPERATION, which checks rules 10 and 11 as it decodes,
//! so the two calls answer the 11 rules in their order, each with its [`Code`].
//! A client calls [`decode_answer`] with the kind it sent, and [`decode_body`]
//! on the body. A received message decodes from one slice, and the decoders
//! borrow the credential, the content and the body from it.
//!
//! The writers put a message into a [`Sink`] and code exactly the contents and
//! bodies of more than [`THRESHOLD`] bytes, >128 B, through the caller's
//! [`zlib::Scratch`], with no size check, which is the sender rule of section
//! 3.3.

use core::{
    error,
    fmt, //
};

use crate::{
    body::{
        self,
        Coding,
        THRESHOLD, //
    },
    credential::Credential,
    head::{
        AnswerHead,
        RequestHead, //
    },
    kind::Kind,
    sink::{
        Full,
        Sink, //
    },
    status::Code,
    stream::Target,
    version::Version,
    zlib, //
};

/// Why a client cannot read an answer, which fails the exchange.
///
/// Each reason but [`WireError::Bound`] is a malformed answer of
/// `docs/graphtp.md` section 5.4, and [`WireError::Bound`] is the client's own
/// bound on a body.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WireError {
    /// The message holds fewer than the 2 bytes of an answer head.
    Short,
    /// The body opens with a byte of no row of the first byte rule, or with a
    /// zlib opener whose second byte completes no header of the profile.
    Coding,
    /// The answer 200.3, or the answer 200.0 of a PULL or a CANCEL, carries no
    /// body, where it must carry the head of a stream or a batch.
    Empty,
    /// The answer carries a body where its kind and its code allow none: a code
    /// the registry gives to the server alone with a nonzero substatus, 200.3
    /// apart, or a code other than 200.0 on a PULL or a CANCEL, or any code on
    /// a kind the client does not know.
    Body,
    /// The coded body breaks the zlib profile or does not decode.
    Decode,
    /// The body, decoded, passes the bound the client gave it.
    Bound,
    /// The head of a stream or a batch is no stream body of section 2.4, or the
    /// head of a stream carries no handle.
    Stream,
}

impl fmt::Display for WireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            WireError::Short => "the answer holds fewer than 2 bytes",
            WireError::Coding => "the body opens with no coding of the first byte rule",
            WireError::Empty => "the answer carries no stream head or batch",
            WireError::Body => "the answer carries a body its kind and code allow none",
            WireError::Decode => "the coded body does not decode",
            WireError::Bound => "the decoded body passes the bound",
            WireError::Stream => "the stream body is malformed",
        })
    }
}

impl error::Error for WireError {}

/// The content of an OPERATION as it travels, plain or one zlib stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Content<'a> {
    /// The GraphQL request as its client wrote it.
    Plain(&'a [u8]),
    /// One zlib stream of the GraphQL request, whose header is of the profile.
    Zlib(&'a [u8]),
}

impl<'a> Content<'a> {
    /// Returns the bytes of the content as they travel.
    #[must_use]
    pub const fn as_bytes(self) -> &'a [u8] {
        match self {
            Content::Plain(bytes) | Content::Zlib(bytes) => bytes,
        }
    }
}

/// The body of an answer as it travels, plain JSON or one zlib stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Body<'a> {
    /// The JSON as its writer wrote it, opening with the brace or the bracket.
    Plain(&'a [u8]),
    /// One zlib stream of the JSON, whose header is of the profile.
    Zlib(&'a [u8]),
}

impl<'a> Body<'a> {
    /// Returns the bytes of the body as they travel.
    #[must_use]
    pub const fn as_bytes(self) -> &'a [u8] {
        match self {
            Body::Plain(bytes) | Body::Zlib(bytes) => bytes,
        }
    }
}

/// A request a server accepts by rules 1 to 9 of `docs/graphtp.md` section
/// 5.3, borrowed from its message.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Request<'a> {
    /// An OPERATION with its credential, `None` for none, and its content.
    Operation {
        /// The value of `Authorization`, checked by rules 6 and 7.
        credential: Option<Credential<'a>>,
        /// The content, whose zlib header rule 9 checked when it is coded.
        content: Content<'a>,
    },
    /// A PULL.
    Pull,
    /// A CANCEL of one stream or of every stream of the peer.
    Cancel(Target),
}

/// An answer a client reads, borrowed from its message.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Answer<'a> {
    /// The answer of the service to an OPERATION, with its JSON.
    Result {
        /// The status and the substatus.
        code: Code,
        /// The GraphQL response of the service.
        body: Body<'a>,
    },
    /// The answer 200.3 to an OPERATION, which opened a stream.
    Opened {
        /// The code 200.3.
        code: Code,
        /// The head of the stream, a stream body of section 2.4.
        head: Body<'a>,
    },
    /// The answer 200.0 to a PULL or a CANCEL, a batch of stream entries.
    Batch {
        /// The code 200.0.
        code: Code,
        /// The batch, a stream body of section 2.4.
        body: Body<'a>,
    },
    /// An answer with an empty body, a refusal whatever its code.
    Refused(Code),
}

/// Reads the request `message` by rules 1 to 9 of `docs/graphtp.md` section
/// 5.3, in their order, with the server's bound `max_credential` on the
/// credential.
///
/// Rule 9 reads a coded content's header in three steps: at least 2 bytes, then
/// FCHECK, then the fields, so a header that breaks both FCHECK and the profile
/// answers 400.8. The content of an OPERATION is not decoded here, and
/// [`decode_content`] checks rules 10 and 11 as it decodes it.
///
/// # Errors
///
/// Returns the [`Code`] of the first rule the message breaks: 400.7 for rules
/// 1, 4, 5, 6 and 8, 505.0 for rule 2, 501.0 for rule 3, 431.0 for rule 7, and
/// 400.8 or 415.1 for rule 9.
pub fn decode_request(message: &[u8], max_credential: u16) -> Result<Request<'_>, Code> {
    let head = RequestHead::parse(message)?;
    let end = RequestHead::LEN + usize::from(head.credential_len);
    let (Some(bytes), Some(rest)) = (message.get(RequestHead::LEN..end), message.get(end..)) else {
        return Err(Code::MALFORMED_MESSAGE);
    };
    let credential = Credential::new(bytes)?;
    if head.kind != Kind::Operation && !credential.is_empty() {
        return Err(Code::MALFORMED_MESSAGE);
    }
    let credential = credential.within(max_credential)?;
    match head.kind {
        Kind::Operation => {
            let content = if head.compressed {
                let [cmf, flg, ..] = *rest else {
                    return Err(Code::BAD_CODING);
                };
                zlib::check_header(cmf, flg)?;
                Content::Zlib(rest)
            } else {
                Content::Plain(rest)
            };
            let credential = (!credential.is_empty()).then_some(credential);
            Ok(Request::Operation {
                credential,
                content,
            })
        }
        Kind::Pull if !head.compressed && rest.is_empty() => Ok(Request::Pull),
        Kind::Cancel if !head.compressed => {
            let [high, low] = *rest else {
                return Err(Code::MALFORMED_MESSAGE);
            };
            let handle = u16::from_be_bytes([high, low]);
            Ok(Request::Cancel(Target::from_u16(handle)))
        }
        Kind::Pull | Kind::Cancel => Err(Code::MALFORMED_MESSAGE),
        Kind::Unknown(_) => Err(Code::NOT_IMPLEMENTED),
    }
}

/// Decodes the content `content` into `sink` under the server's decoded bound
/// `bound` and returns the count of its plain bytes.
///
/// This is rules 10 and 11 of `docs/graphtp.md` section 5.3. A plain content is
/// put into `sink` as it is, and a coded one is decoded through a window of
/// 4,096 bytes the function holds on its stack, so the plain bytes leave
/// through `sink` as they come.
///
/// # Errors
///
/// Returns 413.0 as soon as the plain bytes pass `bound` or the room of `sink`,
/// and 400.8 when the stream does not decode, its Adler-32 fails or a byte
/// follows it. A coded content that no [`decode_request`] checked answers the
/// codes of rule 9 as well. The plain bytes put into `sink` before a refusal
/// are no content.
pub fn decode_content(
    content: Content<'_>,
    bound: usize,
    sink: &mut dyn Sink,
) -> Result<usize, Code> {
    match content {
        Content::Plain(bytes) => {
            if bytes.len() > bound {
                return Err(Code::CONTENT_TOO_LARGE);
            }
            sink.put(bytes).map_err(|_| Code::CONTENT_TOO_LARGE)?;
            Ok(bytes.len())
        }
        Content::Zlib(stream) => {
            let mut window = [0; zlib::WINDOW];
            zlib::Decoder::new(bound)
                .streaming(stream, &mut window, sink)
                .map_err(zlib::Error::code)
        }
    }
}

/// Reads the answer `message` to a request of the kind `kind`.
///
/// The head comes first, then the coding of the body by the first byte rule,
/// then the shape the kind and the code give it. An empty body is a refusal
/// whatever its code, but for 200.3 and the 200.0 of a PULL or a CANCEL, which
/// must carry a stream head or a batch. The body is not decoded here, and
/// [`decode_body`] decodes it.
///
/// # Errors
///
/// Returns [`WireError::Short`] for a message of fewer than 2 bytes,
/// [`WireError::Coding`] for a body of no coding of the first byte rule,
/// [`WireError::Empty`] for 200.3, or the 200.0 of a PULL or a CANCEL, with an
/// empty body, and [`WireError::Body`] for a body under a code the registry
/// gives to the server alone with a nonzero substatus, 200.3 apart, under a
/// code other than 200.0 on a PULL or a CANCEL, or on a kind the crate does not
/// know.
pub fn decode_answer(kind: Kind, message: &[u8]) -> Result<Answer<'_>, WireError> {
    let code = AnswerHead::parse(message)?.code;
    let rest = message.get(AnswerHead::LEN..).unwrap_or_default();
    let body = match body::coding_of(rest)? {
        None => {
            let batch = code == Code::OK && matches!(kind, Kind::Pull | Kind::Cancel);
            if code == Code::STREAM_OPENED || batch {
                return Err(WireError::Empty);
            }
            return Ok(Answer::Refused(code));
        }
        Some(Coding::Plain) => Body::Plain(rest),
        Some(Coding::Zlib) => Body::Zlib(rest),
    };
    match kind {
        Kind::Operation if code == Code::STREAM_OPENED => Ok(Answer::Opened { code, head: body }),
        Kind::Operation if !code.is_server_only() => Ok(Answer::Result { code, body }),
        Kind::Pull | Kind::Cancel if code == Code::OK => Ok(Answer::Batch { code, body }),
        _ => Err(WireError::Body),
    }
}

/// Decodes the body `body` into `sink` under the client's bound `bound` and
/// returns the count of its plain bytes.
///
/// A plain body is put into `sink` as it is, and a coded one is decoded through
/// a window of 4,096 bytes the function holds on its stack.
///
/// # Errors
///
/// Returns [`WireError::Bound`] as soon as the plain bytes pass `bound` or the
/// room of `sink`, and [`WireError::Decode`] when the coded body breaks the
/// zlib profile or does not decode.
pub fn decode_body(body: Body<'_>, bound: usize, sink: &mut dyn Sink) -> Result<usize, WireError> {
    match body {
        Body::Plain(bytes) => {
            if bytes.len() > bound {
                return Err(WireError::Bound);
            }
            sink.put(bytes).map_err(|_| WireError::Bound)?;
            Ok(bytes.len())
        }
        Body::Zlib(stream) => {
            let mut window = [0; zlib::WINDOW];
            zlib::Decoder::new(bound)
                .streaming(stream, &mut window, sink)
                .map_err(|error| match error {
                    zlib::Error::Bound => WireError::Bound,
                    _ => WireError::Decode,
                })
        }
    }
}

/// Writes an OPERATION with the credential `credential` and the content
/// `plain` into `sink`.
///
/// A content of more than [`THRESHOLD`] bytes is coded through `scratch` and
/// the compressed bit is set, and one of at most [`THRESHOLD`] bytes travels
/// plain.
///
/// # Errors
///
/// Returns [`Full`] when `sink` has no room.
pub fn write_operation(
    sink: &mut dyn Sink,
    credential: Option<Credential<'_>>,
    plain: &[u8],
    scratch: &mut zlib::Scratch,
) -> Result<(), Full> {
    let compressed = plain.len() > THRESHOLD;
    let head = RequestHead {
        version: Version::V1,
        kind: Kind::Operation,
        compressed,
        credential_len: credential.map_or(0, Credential::len),
    };
    sink.put(&head.to_bytes())?;
    if let Some(credential) = credential {
        sink.put(credential.as_bytes())?;
    }
    write_coded(sink, plain, scratch)
}

/// Writes a PULL, `48 00 00`, into `sink`.
///
/// # Errors
///
/// Returns [`Full`] when `sink` has no room.
pub fn write_pull(sink: &mut dyn Sink) -> Result<(), Full> {
    sink.put(&bare(Kind::Pull).to_bytes())
}

/// Writes a CANCEL of the target `target` into `sink`, its head and the handle
/// in one put.
///
/// # Errors
///
/// Returns [`Full`] when `sink` has no room.
pub fn write_cancel(sink: &mut dyn Sink, target: Target) -> Result<(), Full> {
    let [first, high, low] = bare(Kind::Cancel).to_bytes();
    let [handle_high, handle_low] = target.to_u16().to_be_bytes();
    sink.put(&[first, high, low, handle_high, handle_low])
}

/// Writes an answer of the code `code` with the body `plain` into `sink`.
///
/// A body of more than [`THRESHOLD`] bytes is coded through `scratch`, one of
/// at most [`THRESHOLD`] bytes travels plain, and an empty body writes the head
/// alone, which is a refusal.
///
/// # Errors
///
/// Returns [`Full`] when `sink` has no room.
pub fn write_answer(
    sink: &mut dyn Sink,
    code: Code,
    plain: &[u8],
    scratch: &mut zlib::Scratch,
) -> Result<(), Full> {
    sink.put(&AnswerHead { code }.to_bytes())?;
    write_coded(sink, plain, scratch)
}

/// Writes the refusal `code`, its 2 bytes, into `sink`.
///
/// # Errors
///
/// Returns [`Full`] when `sink` has no room.
pub fn write_refusal(sink: &mut dyn Sink, code: Code) -> Result<(), Full> {
    sink.put(&AnswerHead { code }.to_bytes())
}

/// Returns the head of a request of the kind `kind` with no credential and a
/// plain content.
fn bare(kind: Kind) -> RequestHead {
    RequestHead {
        version: Version::V1,
        kind,
        compressed: false,
        credential_len: 0,
    }
}

/// Puts the plain bytes `plain` into `sink`, coded through `scratch` when they
/// are more than [`THRESHOLD`] bytes.
fn write_coded(sink: &mut dyn Sink, plain: &[u8], scratch: &mut zlib::Scratch) -> Result<(), Full> {
    if plain.len() <= THRESHOLD {
        return sink.put(plain);
    }
    let mut encoder = zlib::Encoder::new(scratch);
    encoder.write(plain, sink)?;
    encoder.finish(sink)
}
