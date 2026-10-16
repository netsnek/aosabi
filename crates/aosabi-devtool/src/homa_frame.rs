// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool homa_frame: frames, checks and resynchronises Homa packets on
 * a byte stream with AoFrame.
 *
 * No analogue in the reference. Its judging notebook is
 * tests/notebooks/04_aoframe.ipynb, whose references are
 * tests/harness/homa/{aoframe,cobs,crc32c}.py.
 */

//! The `homa-frame` primitive of the seam.
//!
//! `--requests PATH` answers a file of request lines, one response line each,
//! in the batch form of [`crate::batch`]. A `mode` is `delimited` or
//! `prefixed`, and every byte string travels as lowercase hex.
//!
//! - `encode` takes `mode` and `hex`, one packet, and answers `hex`, its
//!   frame, or `error` with the name `NoRoom`.
//! - `decode` takes `mode`, `hex`, a byte stream, and `max_packet`, pushes the
//!   stream through one [`Decoder`] whose buffer holds a frame of a
//!   `max_packet` byte packet, and answers `frames`, the checked packets in
//!   order, `dropped`, the count of frames dropped, and `errors`, the name of
//!   each drop in order.
//! - `feed` takes the same and answers `buffer`, the length of the decoder's
//!   buffer in bytes, and `events`, one object for each frame that ended, with `at`,
//!   the offset of the byte that ended it, and `packet` or `error`.

use std::{
    path::PathBuf,
    process::ExitCode, //
};

use aosabi_json::{
    JsObject,
    JsString,
    JsValue, //
};
use homa::stream::{
    self,
    Decoder,
    FrameError,
    Mode, //
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
        Fields,
        LANDS_AT,
        count,
        hex, //
    },
    seam, //
};

/// The name of the primitive.
const NAME: &str = "homa-frame";

/// The command line of the `homa-frame` primitive.
pub static GRAMMAR: Grammar = Grammar {
    options: &[cli::SMOKE, cli::REQUESTS],
    positional: None,
    conflicts: &[],
};

/// What the `homa-frame` primitive accepts.
#[derive(Debug)]
pub struct HomaFrameArgs {
    /// Whether the call asks for the interface line.
    smoke: bool,
    /// The file of request lines of the batch form.
    requests: Option<PathBuf>,
}

impl HomaFrameArgs {
    /// Reads the arguments `matches` holds for [`GRAMMAR`].
    pub fn read(matches: &Matches) -> Self {
        Self {
            smoke: matches.flag("smoke"),
            requests: matches.path("requests"),
        }
    }
}

/// The operations of the batch form.
const OPERATIONS: &Operations = &[
    ("encode", op_encode),
    ("decode", op_decode),
    ("feed", op_feed),
];

/// Runs the `homa-frame` primitive.
pub fn run(args: &HomaFrameArgs) -> ExitCode {
    if args.smoke {
        return seam::smoke_at(NAME, LANDS_AT);
    }
    let outcome = seam::required(args.requests.as_deref(), "--requests")
        .and_then(|path| batch::run(OPERATIONS, path));
    seam::finish(NAME, outcome)
}

/// Returns the form a request names.
pub fn mode_of(fields: &Fields<'_>) -> Result<Mode, JsString> {
    match fields.str("mode")? {
        "delimited" => Ok(Mode::Delimited),
        "prefixed" => Ok(Mode::Prefixed),
        other => Err(fields.refuse("mode", &format!("names no form of AoFrame: {other}"))),
    }
}

/// Returns the name of a frame error, the name of its variant.
pub fn frame_error_name(error: FrameError) -> &'static str {
    match error {
        FrameError::TooLong => "TooLong",
        FrameError::Cobs => "Cobs",
        FrameError::Crc => "Crc",
    }
}

/// Answers the operation `encode`.
fn op_encode(request: &JsObject) -> Result<JsObject, JsString> {
    let fields = Fields::new(request, "encode");
    let mode = mode_of(&fields)?;
    let packet = fields.hex("hex")?;
    let mut out = vec![0u8; stream::frame_len(mode, packet.len())];
    let mut payload = JsObject::with_capacity(1);
    match stream::encode(mode, &packet, &[], &mut out) {
        Ok(len) => payload.insert("hex", hex(out.get(..len).unwrap_or(&[]))),
        Err(_) => payload.insert("error", "NoRoom"),
    };
    Ok(payload)
}

/// One frame that ended in a stream: the offset of the byte that ended it and
/// its packet or the reason it was dropped.
type Ended = (usize, Result<Vec<u8>, FrameError>);

/// Pushes the stream of a request through one decoder and returns the buffer
/// length and every frame that ended.
fn push_stream(fields: &Fields<'_>) -> Result<(usize, Vec<Ended>), JsString> {
    let mode = mode_of(fields)?;
    let bytes = fields.hex("hex")?;
    let max_packet: usize = fields.int("max_packet")?;
    let mut buf = vec![0u8; Decoder::buffer_len(mode, max_packet)];
    let buffer = buf.len();
    let mut decoder = Decoder::new(mode, &mut buf);
    let mut ended = Vec::new();
    for (at, &byte) in bytes.iter().enumerate() {
        if let Some(outcome) = decoder.push(byte) {
            ended.push((at, outcome.map(<[u8]>::to_vec)));
        }
    }
    Ok((buffer, ended))
}

/// Answers the operation `decode`.
fn op_decode(request: &JsObject) -> Result<JsObject, JsString> {
    let (_, ended) = push_stream(&Fields::new(request, "decode"))?;
    let mut frames = Vec::new();
    let mut errors = Vec::new();
    for (_, outcome) in ended {
        match outcome {
            Ok(packet) => frames.push(JsValue::from(hex(&packet))),
            Err(error) => errors.push(JsValue::from(frame_error_name(error))),
        }
    }
    let mut payload = JsObject::with_capacity(3);
    payload.insert("frames", frames);
    payload.insert("dropped", count(errors.len()));
    payload.insert("errors", errors);
    Ok(payload)
}

/// Answers the operation `feed`.
fn op_feed(request: &JsObject) -> Result<JsObject, JsString> {
    let (buffer, ended) = push_stream(&Fields::new(request, "feed"))?;
    let events: Vec<JsValue> = ended
        .into_iter()
        .map(|(at, outcome)| {
            let mut event = JsObject::with_capacity(2);
            event.insert("at", count(at));
            match outcome {
                Ok(packet) => event.insert("packet", hex(&packet)),
                Err(error) => event.insert("error", frame_error_name(error)),
            };
            JsValue::Object(event)
        })
        .collect();
    let mut payload = JsObject::with_capacity(2);
    payload.insert("buffer", count(buffer));
    payload.insert("events", events);
    Ok(payload)
}
