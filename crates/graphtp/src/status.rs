// SPDX-License-Identifier: MIT
/*
 * graphtp status: the status of an answer, its byte, its substatus and the
 * registry of codes.
 *
 * No analogue in the reference. The status, its byte and the registry are those
 * of the specification, and the reason phrases are the descriptions of the IANA
 * registry of HTTP status codes.
 */

//! The status of an answer and its byte, the substatus, and the code of the two
//! with the constants of the registry.
//!
//! The status byte is `64 * (status div 100 - 2) + status mod 100`, so the 256
//! bytes are the statuses 200 to 263, 300 to 363, 400 to 463 and 500 to 563,
//! and every byte is a status. The notebook `tests/notebooks/24_graphtp.ipynb`
//! reads all 256 bytes and every registered code.

use core::fmt;

/// The HTTP status of an answer, one that has a status byte.
///
/// The value is a status of the classes 2 to 5 whose last two digits are at
/// most 63, checked in safe code, so every value has a byte and every byte is a
/// value. A status whose last two digits exceed 63, such as 294 of the GraphQL
/// over HTTP draft, and every status of class 1 have no byte and no value of
/// this type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Status(u16);

impl Status {
    /// 200 OK.
    pub const OK: Status = Status(200);
    /// 400 Bad Request.
    pub const BAD_REQUEST: Status = Status(400);
    /// 403 Forbidden.
    pub const FORBIDDEN: Status = Status(403);
    /// 404 Not Found.
    pub const NOT_FOUND: Status = Status(404);
    /// 409 Conflict.
    pub const CONFLICT: Status = Status(409);
    /// 413 Content Too Large.
    pub const CONTENT_TOO_LARGE: Status = Status(413);
    /// 415 Unsupported Media Type.
    pub const UNSUPPORTED_MEDIA_TYPE: Status = Status(415);
    /// 422 Unprocessable Content.
    pub const UNPROCESSABLE_CONTENT: Status = Status(422);
    /// 429 Too Many Requests.
    pub const TOO_MANY_REQUESTS: Status = Status(429);
    /// 431 Request Header Fields Too Large.
    pub const REQUEST_HEADER_FIELDS_TOO_LARGE: Status = Status(431);
    /// 500 Internal Server Error.
    pub const INTERNAL_SERVER_ERROR: Status = Status(500);
    /// 501 Not Implemented.
    pub const NOT_IMPLEMENTED: Status = Status(501);
    /// 502 Bad Gateway.
    pub const BAD_GATEWAY: Status = Status(502);
    /// 503 Service Unavailable.
    pub const SERVICE_UNAVAILABLE: Status = Status(503);
    /// 504 Gateway Timeout.
    pub const GATEWAY_TIMEOUT: Status = Status(504);
    /// 505 HTTP Version Not Supported.
    pub const HTTP_VERSION_NOT_SUPPORTED: Status = Status(505);

    /// Returns the status of the number `status`, or `None` when it has no
    /// byte.
    ///
    /// A number has a byte when its class, the first digit, is 2 to 5 and its
    /// last two digits are at most 63.
    #[must_use]
    pub fn from_u16(status: u16) -> Option<Status> {
        if matches!(status / 100, 2..=5) && status % 100 < 64 {
            Some(Status(status))
        } else {
            None
        }
    }

    /// Returns the number of the status, 200 to 563.
    #[must_use]
    pub const fn as_u16(self) -> u16 {
        self.0
    }

    /// Returns the status byte, `64 * (status div 100 - 2) + status mod 100`.
    #[must_use]
    pub fn to_byte(self) -> u8 {
        // The class is 2 to 5 and the last two digits are below 64, so the
        // value fits the low byte and the high byte is 0.
        let [_, byte] = ((self.0 / 100 - 2) * 64 + self.0 % 100).to_be_bytes();
        byte
    }

    /// Returns the status the byte `byte` names, `(byte div 64 + 2) * 100 +
    /// byte mod 64`.
    #[must_use]
    pub fn from_byte(byte: u8) -> Status {
        Status((u16::from(byte >> 6) + 2) * 100 + u16::from(byte & 63))
    }

    /// Returns the class of the status, its first digit, 2 to 5.
    #[must_use]
    pub fn class(self) -> u8 {
        let [_, class] = (self.0 / 100).to_be_bytes();
        class
    }

    /// Returns the reason phrase the IANA registry gives the status, or `None`
    /// for a status the registry leaves unassigned.
    ///
    /// The phrase is the description column of the registry verbatim, so 306
    /// and 418 read `(Unused)` and 510 reads `Not Extended (OBSOLETED)`. The
    /// registry lists 59 codes of the classes 2 to 5, 10, 9, 29 and 11 per
    /// class, and every one of them has a byte.
    #[must_use]
    pub fn canonical_reason(self) -> Option<&'static str> {
        let reason = match self.0 {
            200 => "OK",
            201 => "Created",
            202 => "Accepted",
            203 => "Non-Authoritative Information",
            204 => "No Content",
            205 => "Reset Content",
            206 => "Partial Content",
            207 => "Multi-Status",
            208 => "Already Reported",
            226 => "IM Used",
            300 => "Multiple Choices",
            301 => "Moved Permanently",
            302 => "Found",
            303 => "See Other",
            304 => "Not Modified",
            305 => "Use Proxy",
            306 | 418 => "(Unused)",
            307 => "Temporary Redirect",
            308 => "Permanent Redirect",
            400 => "Bad Request",
            401 => "Unauthorized",
            402 => "Payment Required",
            403 => "Forbidden",
            404 => "Not Found",
            405 => "Method Not Allowed",
            406 => "Not Acceptable",
            407 => "Proxy Authentication Required",
            408 => "Request Timeout",
            409 => "Conflict",
            410 => "Gone",
            411 => "Length Required",
            412 => "Precondition Failed",
            413 => "Content Too Large",
            414 => "URI Too Long",
            415 => "Unsupported Media Type",
            416 => "Range Not Satisfiable",
            417 => "Expectation Failed",
            421 => "Misdirected Request",
            422 => "Unprocessable Content",
            423 => "Locked",
            424 => "Failed Dependency",
            425 => "Too Early",
            426 => "Upgrade Required",
            428 => "Precondition Required",
            429 => "Too Many Requests",
            431 => "Request Header Fields Too Large",
            451 => "Unavailable For Legal Reasons",
            500 => "Internal Server Error",
            501 => "Not Implemented",
            502 => "Bad Gateway",
            503 => "Service Unavailable",
            504 => "Gateway Timeout",
            505 => "HTTP Version Not Supported",
            506 => "Variant Also Negotiates",
            507 => "Insufficient Storage",
            508 => "Loop Detected",
            510 => "Not Extended (OBSOLETED)",
            511 => "Network Authentication Required",
            _ => return None,
        };
        Some(reason)
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// The substatus of an answer, one byte of `GraphTP`, 0 for none.
///
/// A substatus is registered per status, so 400.2 and 404.2 are unrelated, and
/// a client that does not know a substatus reads the status alone. Every byte
/// is a substatus.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Substatus(u8);

impl Substatus {
    /// No substatus, the value 0.
    pub const NONE: Substatus = Substatus(0);

    /// Returns the substatus of the value `value`.
    #[must_use]
    pub const fn new(value: u8) -> Substatus {
        Substatus(value)
    }

    /// Returns the value of the substatus.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// Tells whether the substatus is 0, which is none.
    #[must_use]
    pub const fn is_none(self) -> bool {
        self.0 == 0
    }
}

impl fmt::Display for Substatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A status with its substatus, written `status.substatus`, such as `400.2`.
///
/// A code whose substatus is 0 is any `status.0`, and its name is the reason
/// phrase of its status.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Code {
    /// The HTTP status.
    pub status: Status,
    /// The substatus, which refines and never contradicts the status.
    pub substatus: Substatus,
}

impl Code {
    /// 200.0 OK: a result with data and no error, or the batch of a PULL or
    /// a CANCEL.
    pub const OK: Code = Code::of(Status::OK, 0);
    /// 200.1 Partial: a result with data and errors.
    pub const PARTIAL: Code = Code::of(Status::OK, 1);
    /// 200.2 No Data: a result whose `data` is null, with errors.
    pub const NO_DATA: Code = Code::of(Status::OK, 2);
    /// 200.3 Stream Opened: the OPERATION opened a stream, and the body is its
    /// head.
    pub const STREAM_OPENED: Code = Code::of(Status::OK, 3);
    /// 400.1 Syntax Error: the document does not parse.
    pub const SYNTAX_ERROR: Code = Code::of(Status::BAD_REQUEST, 1);
    /// 400.2 Validation Error: the document fails validation against the
    /// schema.
    pub const VALIDATION_ERROR: Code = Code::of(Status::BAD_REQUEST, 2);
    /// 400.3 Invalid Variables: a variable is missing or fails coercion.
    pub const INVALID_VARIABLES: Code = Code::of(Status::BAD_REQUEST, 3);
    /// 400.4 Operation Not Found: `operationName` names no operation of the
    /// document, or the document holds several and names none.
    pub const OPERATION_NOT_FOUND: Code = Code::of(Status::BAD_REQUEST, 4);
    /// 400.5 Query Too Complex: the document exceeds a bound of tokens, depth,
    /// fields or aliases.
    pub const QUERY_TOO_COMPLEX: Code = Code::of(Status::BAD_REQUEST, 5);
    /// 400.6 Invalid Request Body: the content is not a GraphQL request.
    pub const INVALID_REQUEST_BODY: Code = Code::of(Status::BAD_REQUEST, 6);
    /// 400.7 Malformed Message: the message breaks a rule of section 5.3.
    pub const MALFORMED_MESSAGE: Code = Code::of(Status::BAD_REQUEST, 7);
    /// 400.8 Bad Coding: the zlib header fails its FCHECK, the zlib stream
    /// does not decode, its Adler-32 fails or a byte follows it.
    pub const BAD_CODING: Code = Code::of(Status::BAD_REQUEST, 8);
    /// 403.1 Stream Not Allowed: the server allows no stream on this carrier
    /// or for this root field.
    pub const STREAM_NOT_ALLOWED: Code = Code::of(Status::FORBIDDEN, 1);
    /// 404.1 Unknown Stream: a CANCEL of a handle the peer does not hold, a
    /// PULL from a peer that holds no stream, or the parked PULL of a peer
    /// whose last stream a CANCEL ends.
    pub const UNKNOWN_STREAM: Code = Code::of(Status::NOT_FOUND, 1);
    /// 409.1 Pull Already Parked: a PULL while the peer's PULL is parked.
    pub const PULL_ALREADY_PARKED: Code = Code::of(Status::CONFLICT, 1);
    /// 413.0 Content Too Large: the content, decoded, exceeds the server's
    /// bound.
    pub const CONTENT_TOO_LARGE: Code = Code::of(Status::CONTENT_TOO_LARGE, 0);
    /// 415.1 Unsupported Coding: a zlib header with CINFO above 4, FDICT set or
    /// CM other than 8.
    pub const UNSUPPORTED_CODING: Code = Code::of(Status::UNSUPPORTED_MEDIA_TYPE, 1);
    /// 422.2 Validation Error: the document fails validation, and the handler
    /// answers 422.
    pub const VALIDATION_ERROR_422: Code = Code::of(Status::UNPROCESSABLE_CONTENT, 2);
    /// 422.3 Invalid Variables: a variable is missing or fails coercion, and
    /// the handler answers 422.
    pub const INVALID_VARIABLES_422: Code = Code::of(Status::UNPROCESSABLE_CONTENT, 3);
    /// 422.4 Operation Not Found: the operation to execute cannot be
    /// determined unambiguously, and the handler answers 422.
    pub const OPERATION_NOT_FOUND_422: Code = Code::of(Status::UNPROCESSABLE_CONTENT, 4);
    /// 422.5 Query Too Complex: a bound the handler applies as a rule of
    /// validation refuses the document, and the handler answers 422.
    pub const QUERY_TOO_COMPLEX_422: Code = Code::of(Status::UNPROCESSABLE_CONTENT, 5);
    /// 422.6 Invalid Request Body: the content is JSON but no well formed
    /// GraphQL over HTTP request, and the handler answers 422.
    pub const INVALID_REQUEST_BODY_422: Code = Code::of(Status::UNPROCESSABLE_CONTENT, 6);
    /// 429.1 Too Many Streams: the peer already holds the streams one peer may
    /// hold.
    pub const TOO_MANY_STREAMS: Code = Code::of(Status::TOO_MANY_REQUESTS, 1);
    /// 431.0 Request Header Fields Too Large: the credential exceeds the
    /// server's bound.
    pub const REQUEST_HEADER_FIELDS_TOO_LARGE: Code =
        Code::of(Status::REQUEST_HEADER_FIELDS_TOO_LARGE, 0);
    /// 500.0 Internal Server Error.
    pub const INTERNAL_SERVER_ERROR: Code = Code::of(Status::INTERNAL_SERVER_ERROR, 0);
    /// 500.1 Answer Too Large: the answer outgrows the server's answer buffer
    /// or the client's longest message.
    pub const ANSWER_TOO_LARGE: Code = Code::of(Status::INTERNAL_SERVER_ERROR, 1);
    /// 500.2 Status Not Representable: the service answered a status with last
    /// two digits above 63 other than 294.
    pub const STATUS_NOT_REPRESENTABLE: Code = Code::of(Status::INTERNAL_SERVER_ERROR, 2);
    /// 500.3 Unsupported Answer Type: the service answered a status of class 2
    /// or 3 that has a byte under a media type other than the answer type and
    /// `text/event-stream`, or under none.
    pub const UNSUPPORTED_ANSWER_TYPE: Code = Code::of(Status::INTERNAL_SERVER_ERROR, 3);
    /// 501.0 Not Implemented: the kind is one the server does not know.
    pub const NOT_IMPLEMENTED: Code = Code::of(Status::NOT_IMPLEMENTED, 0);
    /// 502.0 Bad Gateway.
    pub const BAD_GATEWAY: Code = Code::of(Status::BAD_GATEWAY, 0);
    /// 502.1 Unknown Outcome: the transport failed after the request left, and
    /// the caller did not mark it repeatable.
    pub const UNKNOWN_OUTCOME: Code = Code::of(Status::BAD_GATEWAY, 1);
    /// 502.2 Link Down: the carrier went down during the exchange, or the
    /// target could not be reached.
    pub const LINK_DOWN: Code = Code::of(Status::BAD_GATEWAY, 2);
    /// 503.1 Insufficient Memory: the server stopped the request at its memory
    /// floor.
    pub const INSUFFICIENT_MEMORY: Code = Code::of(Status::SERVICE_UNAVAILABLE, 1);
    /// 504.0 Gateway Timeout.
    pub const GATEWAY_TIMEOUT: Code = Code::of(Status::GATEWAY_TIMEOUT, 0);
    /// 505.0 HTTP Version Not Supported: the version is one the server does not
    /// speak.
    pub const HTTP_VERSION_NOT_SUPPORTED: Code = Code::of(Status::HTTP_VERSION_NOT_SUPPORTED, 0);

    /// Returns the code of the status `status` and the substatus `substatus`.
    #[must_use]
    pub const fn new(status: Status, substatus: Substatus) -> Code {
        Code { status, substatus }
    }

    /// Returns the code of a status and the value of its substatus, for the
    /// constants of the registry.
    const fn of(status: Status, substatus: u8) -> Code {
        Code::new(status, Substatus(substatus))
    }

    /// Tells whether the registry gives the code to the server alone with a
    /// nonzero substatus, G alone in the column Who of `docs/graphtp.md`
    /// section 2.5.
    ///
    /// Such a code is a refusal of the server with an empty body, or 200.3,
    /// whose body is the head of a stream, so any other of them with a body is
    /// a malformed answer. A `status.0` is never one of them, since the service
    /// stands in every row of a `status.0`.
    #[must_use]
    pub const fn is_server_only(self) -> bool {
        matches!(
            (self.status.0, self.substatus.0),
            (200, 3) | (400, 7 | 8) | (403 | 404 | 409 | 415 | 429 | 503, 1) | (500, 1..=3)
        )
    }

    /// Returns the name of the code, or `None` for a `status.0` whose status
    /// has no reason phrase, such as 262.0, and for a nonzero substatus the
    /// registry does not hold.
    ///
    /// A `status.0` is named by the reason phrase of its status, which
    /// [`Status::canonical_reason`] gives, and every other code by the name of
    /// its row of the registry. Each name is visible ASCII with no CR and no
    /// LF, which a reason phrase allows.
    #[must_use]
    pub fn name(self) -> Option<&'static str> {
        let name = match (self.status.0, self.substatus.0) {
            (_, 0) => return self.status.canonical_reason(),
            (200, 1) => "Partial",
            (200, 2) => "No Data",
            (200, 3) => "Stream Opened",
            (400, 1) => "Syntax Error",
            (400 | 422, 2) => "Validation Error",
            (400 | 422, 3) => "Invalid Variables",
            (400 | 422, 4) => "Operation Not Found",
            (400 | 422, 5) => "Query Too Complex",
            (400 | 422, 6) => "Invalid Request Body",
            (400, 7) => "Malformed Message",
            (400, 8) => "Bad Coding",
            (403, 1) => "Stream Not Allowed",
            (404, 1) => "Unknown Stream",
            (409, 1) => "Pull Already Parked",
            (415, 1) => "Unsupported Coding",
            (429, 1) => "Too Many Streams",
            (500, 1) => "Answer Too Large",
            (500, 2) => "Status Not Representable",
            (500, 3) => "Unsupported Answer Type",
            (502, 1) => "Unknown Outcome",
            (502, 2) => "Link Down",
            (503, 1) => "Insufficient Memory",
            _ => return None,
        };
        Some(name)
    }
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.status, self.substatus)
    }
}
