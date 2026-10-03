// SPDX-License-Identifier: MIT
use alloc::vec::Vec;
use bytes::BytesMut;
use http::{HeaderMap, Method};
#[cfg(feature = "httparse")]
use httparse::ParserConfig;

use crate::body::DecodedLength;
use crate::proto::{BodyLength, MessageHead};

pub(crate) use self::conn::Conn;
pub(crate) use self::decode::Decoder;
pub(crate) use self::dispatch::Dispatcher;
pub(crate) use self::encode::{EncodedBuf, Encoder};
//TODO: move out of h1::io
pub(crate) use self::io::MINIMUM_MAX_BUFFER_SIZE;

mod conn;
mod decode;
pub(crate) mod dispatch;
mod encode;
mod io;
#[cfg(feature = "node-llhttp")]
pub(crate) mod node;
mod role;

// The server reads a head with the parser of llhttp of layer B, which reads
// the trailers of a chunked body under `node-chunked`, or with `httparse`.
#[cfg(all(
    feature = "server",
    not(any(feature = "httparse", feature = "node-chunked"))
))]
compile_error!("the HTTP/1 server needs the feature httparse or node-chunked");

cfg_client! {
    pub(crate) type ClientTransaction = role::Client;
}

cfg_server! {
    pub(crate) type ServerTransaction = role::Server;
}

pub(crate) trait Http1Transaction {
    type Incoming;
    type Outgoing: Default;
    #[cfg(feature = "tracing")]
    const LOG: &'static str;
    fn parse(bytes: &mut BytesMut, ctx: ParseContext<'_>) -> ParseResult<Self::Incoming>;
    fn encode(enc: Encode<'_, Self::Outgoing>, dst: &mut Vec<u8>) -> crate::Result<Encoder>;

    fn on_error(err: &crate::Error) -> Option<MessageHead<Self::Outgoing>>;

    fn is_client() -> bool {
        !Self::is_server()
    }

    fn is_server() -> bool {
        !Self::is_client()
    }

    fn should_error_on_parse_eof() -> bool {
        Self::is_client()
    }

    fn should_read_first() -> bool {
        Self::is_server()
    }

    fn update_date() {}
}

/// Result newtype for `Http1Transaction::parse`.
pub(crate) type ParseResult<T> = Result<Option<ParsedMessage<T>>, crate::error::Parse>;

#[derive(Debug)]
pub(crate) struct ParsedMessage<T> {
    head: MessageHead<T>,
    decode: DecodedLength,
    expect_continue: bool,
    keep_alive: bool,
    wants_upgrade: bool,
}

pub(crate) struct ParseContext<'a> {
    cached_headers: &'a mut Option<HeaderMap>,
    req_method: &'a mut Option<Method>,
    #[cfg(feature = "httparse")]
    h1_parser_config: ParserConfig,
    h1_max_headers: Option<usize>,
    preserve_header_case: bool,
    #[cfg(feature = "ffi")]
    preserve_header_order: bool,
    h09_responses: bool,
    #[cfg(feature = "client")]
    on_informational: &'a mut Option<crate::ext::OnInformational>,
}

/// Passed to `Http1Transaction::encode`.
pub(crate) struct Encode<'a, T> {
    head: &'a mut MessageHead<T>,
    body: Option<BodyLength>,
    #[cfg(feature = "server")]
    keep_alive: bool,
    req_method: &'a mut Option<Method>,
    title_case_headers: bool,
    #[cfg(feature = "server")]
    date_header: bool,
    /// The date of the connection without `std`, empty when its timer has no
    /// wall clock.
    #[cfg(all(feature = "server", not(feature = "std")))]
    date: &'a [u8],
}

/// Extra flags that a request "wants", like expect-continue or upgrades.
#[derive(Clone, Copy, Debug)]
struct Wants(u8);

impl Wants {
    const EMPTY: Wants = Wants(0b00);
    const EXPECT: Wants = Wants(0b01);
    const UPGRADE: Wants = Wants(0b10);

    #[must_use]
    fn add(self, other: Wants) -> Wants {
        Wants(self.0 | other.0)
    }

    fn contains(&self, other: Wants) -> bool {
        (self.0 & other.0) == other.0
    }
}
