// SPDX-License-Identifier: MIT
/*
 * graphql-over-graphtp limits: the bounds a card sets on its GraphTP listener.
 *
 * No analogue in the reference. The buffers are those of
 * docs/decisions/ADR-0026-runtime-graphql-over-aoenvelope.md, section Limits
 * answer outside the API, and docs/graphtp-in-aosabi.md sections 3 and 8, and
 * each bound answers its code of docs/graphtp.md section 5.5.
 */

//! The bounds of a card's listener.
//!
//! A bound of the listener answers its code when a request or an answer passes
//! it: the credential 431.0, the decoded content 413.0, the streams of a peer
//! 429.1 and the answer 500.1. The bounds on tokens, depth, fields and aliases
//! of a document are the card's limit hook in the parse step of
//! `graphql-over-http`, which answers 400.5, and the arena floor of the
//! executor answers 503.1, so neither is a bound of this type.

use graphtp::deflate::{
    Params,
    encode::LOOKAHEAD, //
};
use graphtp_homa::server::Config;

/// The bounds a card sets on its listener.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CardLimits {
    /// The name of the card.
    pub card: &'static str,
    /// The longest credential, 431.0 above it.
    pub max_credential: u16,
    /// The request buffer, the longest content once decoded, 413.0 above it.
    pub request_buffer: usize,
    /// The answer buffer, the longest answer, 500.1 above it.
    pub answer_buffer: usize,
    /// The streams one peer may hold, 429.1 at the bound.
    pub streams_per_peer: u16,
    /// The `next` entries one stream's queue holds between two batches.
    pub queue: usize,
    /// The peers whose streams and batch number the listener keeps.
    pub peers: usize,
    /// The sizes of the encoder's scratch.
    pub coding: Params,
}

impl CardLimits {
    /// Returns the bounds of the server of `graphtp-homa` that serves the
    /// card.
    #[must_use]
    pub const fn server_config(&self) -> Config {
        Config {
            max_credential: self.max_credential,
            max_content: self.request_buffer,
            streams_per_peer: self.streams_per_peer,
            answer_buffer: self.answer_buffer,
            queue: self.queue,
            peers: self.peers,
            coding: self.coding,
        }
    }
}

/// The longest message of Homa, 1,000,000 bytes, which bounds every request
/// and answer of a card that sets no bound of its own.
pub const HOMA_MESSAGE: usize = 1_000_000;

/// The window of the zlib profile, 4,096 bytes, as the base 2 logarithm.
const WINDOW_BITS: u8 = graphtp::zlib::WINDOW_BITS;

/// The cards `server` and `docker`, which set no bound of their own, so they
/// answer every document an app answers: the longest message of Homa for the
/// content and the answer, every handle of a peer, and the encoder's default
/// sizes. The queue of a stream and the peers are the listener's own choice.
pub const SERVER: CardLimits = CardLimits {
    card: "server",
    max_credential: u16::MAX,
    request_buffer: HOMA_MESSAGE,
    answer_buffer: HOMA_MESSAGE,
    streams_per_peer: u16::MAX,
    queue: 256,
    peers: 4096,
    coding: Params::new(WINDOW_BITS),
};

/// The harness card `test-graphtp`, the bounds of `server` with 16 streams a
/// peer and a queue of 8 entries a stream, so the notebooks reach both bounds
/// with few requests.
pub const TEST_GRAPHTP: CardLimits = CardLimits {
    card: "test-graphtp",
    streams_per_peer: 16,
    queue: 8,
    ..SERVER
};

/// The player, 8,192 bytes of request buffer and 131,072 of answer buffer, one
/// stream a peer, and the encoder's default sizes.
pub const PLAYER: CardLimits = CardLimits {
    card: "player",
    max_credential: 2048,
    request_buffer: 8192,
    answer_buffer: 131_072,
    streams_per_peer: 1,
    queue: 16,
    peers: 4,
    coding: Params::new(WINDOW_BITS),
};

/// The ESP32-C3 and the HT-CT62, 4,096 bytes of request buffer and 8,192 of
/// answer buffer, and the encoder with 1,024 hash heads, blocks of 2,048
/// tokens and a buffer of plain bytes of the window and its lookahead, 4,358
/// bytes.
pub const ESP32C3: CardLimits = CardLimits {
    card: "esp32c3-devkitm1",
    max_credential: 2048,
    request_buffer: 4096,
    answer_buffer: 8192,
    streams_per_peer: 4,
    queue: 4,
    peers: 2,
    coding: Params {
        window_bits: WINDOW_BITS,
        heads: 1024,
        tokens: 2048,
        buffer: graphtp::zlib::WINDOW + LOOKAHEAD,
    },
};

/// Every card whose bounds this module holds.
pub const CARDS: [&CardLimits; 4] = [&SERVER, &TEST_GRAPHTP, &PLAYER, &ESP32C3];

/// Returns the bounds of the card named `name`, or `None` for a card this
/// module does not hold.
#[must_use]
pub fn card(name: &str) -> Option<&'static CardLimits> {
    CARDS.into_iter().find(|limits| limits.card == name)
}
