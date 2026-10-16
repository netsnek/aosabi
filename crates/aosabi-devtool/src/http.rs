// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool http: the primitive through which a notebook reads the Node
 * tables of the HTTP layer and drives the Node connection of the fork of
 * hyper in process.
 *
 * No analogue in the reference.
 */

//! The primitive `http` of the seam.
//!
//! - The flag `--tables` prints `aosabi_http::node::STATUS_CODES` as the object
//!   `STATUS_CODES` of Node, `hyper::ext::NODE_METHODS` as `METHODS`, the
//!   bound of a head as `maxHeaderSize`, and every bound of `NodeLimits`,
//!   which `30_http_tier1` compares with the pinned Node.
//! - The flag `--requests PATH` answers every request line of the file, one
//!   response line each. A request line is
//!   `{"id", "writes": [base64, ...], "pauses_ms": [...], "shut"}`, the
//!   writes a client makes on one connection, the pause before each and
//!   whether the client shuts its write half after the last, and a response
//!   line is `{"id", "written": base64, "closed": bool, "requests":
//!   [{"method", "target", "body_sha256"}]}`, every byte the connection wrote,
//!   whether it ended, and the requests it handed the service. The module
//!   [`wire`] drives the connection.
//! - The flag `--card esp32c3` runs both with the bounds of the card
//!   `esp32c3-devkitm1`, `NodeConfig::ESP32C3`, in place of Node's.

pub mod tables;
pub mod wire;

use std::{
    path::PathBuf,
    process::ExitCode, //
};

use aosabi_json::JsValue;
use hyper::server::conn::node1::NodeConfig;

use crate::{
    cli::{
        self,
        Choice,
        Grammar,
        Matches,
        Opt,
        Refusal, //
    },
    seam::{
        self,
        Answer,
        Failure, //
    }, //
};

/// The name of the primitive.
const NAME: &str = "http";

/// The milestone the primitive lands at.
const LANDS_AT: &str = "M5";

/// The command line of the `http` primitive.
pub static GRAMMAR: Grammar = Grammar {
    options: &[
        cli::SMOKE,
        Opt::flag(
            "tables",
            "Print the status codes, the methods and the bounds of Node as JSON",
        ),
        Opt::path(
            "requests",
            "PATH",
            "Answer every request line of this file through the Node connection",
        ),
        Opt::choice(
            "card",
            "CARD",
            "The card whose bounds the connection takes",
            &Card::CHOICES,
        )
        .default("server"),
    ],
    positional: None,
    conflicts: &[],
};

/// The card whose bounds the connection takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Card {
    /// The server cards, Node's bounds.
    Server,
    /// The card `esp32c3-devkitm1`.
    Esp32c3,
}

impl Card {
    /// The name of each card on the command line and the line the help of
    /// `--card` describes it with, in the order of the variants.
    const CHOICES: [Choice; 2] = [
        Choice {
            name: "server",
            help: "The server cards, Node's bounds",
        },
        Choice {
            name: "esp32c3",
            help: "The card `esp32c3-devkitm1`",
        },
    ];

    /// Returns the card the command line calls `name`.
    fn named(name: &str) -> Option<Self> {
        match name {
            "server" => Some(Self::Server),
            "esp32c3" => Some(Self::Esp32c3),
            _ => None,
        }
    }

    /// Returns the configuration of the connection of the card.
    const fn config(self) -> NodeConfig {
        match self {
            Self::Server => NodeConfig::NODE,
            Self::Esp32c3 => NodeConfig::ESP32C3,
        }
    }
}

/// What the `http` primitive accepts.
#[derive(Debug)]
pub struct HttpArgs {
    /// Whether the call asks for the interface line.
    smoke: bool,
    /// Whether the call asks for the tables of Node.
    tables: bool,
    /// The file of request lines of the batch form.
    requests: Option<PathBuf>,
    /// The card whose bounds the connection takes.
    card: Card,
}

impl HttpArgs {
    /// Reads the arguments `matches` holds for [`GRAMMAR`].
    ///
    /// The parser takes no other card than the ones of [`Card::CHOICES`] and
    /// reads `server` when the command line names none.
    ///
    /// # Errors
    ///
    /// Returns a [`Refusal`] when the name of the card is not UTF-8.
    pub fn read(matches: &Matches) -> Result<Self, Refusal> {
        let card = matches.text("card")?;
        Ok(Self {
            smoke: matches.flag("smoke"),
            tables: matches.flag("tables"),
            requests: matches.path("requests"),
            card: card
                .as_deref()
                .and_then(Card::named)
                .unwrap_or(Card::Server),
        })
    }
}

/// Runs the `http` primitive.
pub fn run(args: &HttpArgs) -> ExitCode {
    if args.smoke {
        return seam::smoke_at(NAME, LANDS_AT);
    }
    let config = args.card.config();
    let outcome = match (&args.requests, args.tables) {
        (None, true) => Ok(Answer::Payload(json_bytes(&tables::tables(config.limits)))),
        (Some(requests), false) => seam::read_bytes(requests)
            .map(|bytes| Answer::Payload(wire::answer_all(config, &bytes))),
        (Some(_), true) => Err(Failure::Usage(String::from(
            "the call names both --tables and --requests",
        ))),
        (None, false) => Err(Failure::Usage(String::from(
            "the call names neither --tables nor --requests",
        ))),
    };
    seam::finish(NAME, outcome)
}

/// Returns `value` as JSON bytes.
fn json_bytes(value: &JsValue) -> Vec<u8> {
    let mut out = Vec::new();
    aosabi_json::write(&mut out, value);
    out
}
