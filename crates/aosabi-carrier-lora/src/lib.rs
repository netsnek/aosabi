// SPDX-License-Identifier: MIT
/*
 * aosabi-carrier-lora: the carrier LoRa.
 *
 * This file holds the module list.
 */

//! The carrier `LoRa`.
//!
//! One compact Homa packet travels in one `LoRa` packet, sealed with
//! ChaCha20-Poly1305 behind a header of 5 bytes that names the link and the
//! sender's counter, with the tag of 16 bytes behind it, so a frame is at
//! most 255 bytes and a packet at most 234. No `AoFrame` is needed, since a
//! `LoRa` packet keeps its boundaries and carries a CRC. The carrier keeps
//! the air of its radio inside the duty cycle of its band with an airtime
//! ledger of 61 buckets of one minute, runs channel activity detection
//! before every transmission, and sends a message of several packets in
//! bursts of at most 2 s of air with a listening gap between them.
//!
//! | Module | Holds |
//! | --- | --- |
//! | `radio` | the trait `Radio`, the private sync word, `Modulation` and `time_on_air` |
//! | `frame` | `seal`, `open`, `Direction`, `FrameError` and the window `Replay` |
//! | `counter` | the storage capability `CounterStore` and `Counters`, reserved in blocks of 1,024 |
//! | `ledger` | the airtime `Ledger` over `homa::pacer::Budget` |
//! | `lora` | the carrier `Lora`, `LoraError`, `Band`, `Pairing` and the values of a card |
//! | `sx1262` | `Sx1262` and `TcxoVoltage`, the radio of the card over its HAL, under the feature `sx1262` |
//! | `model` | the model radio of `homa::sim` as a `Radio`, under the feature `model` |
//!
//! There is no test in this crate, which is the rule of CONTRIBUTING.md
//! section 3. The judging notebooks are `tests/notebooks/06_lora_frame.ipynb`
//! and `tests/notebooks/29_carrier_lora.ipynb`.

#![no_std]
#![forbid(unsafe_code)]

mod counter;
pub use counter::*;
mod frame;
pub use frame::*;
mod ledger;
pub use ledger::*;
mod lora;
pub use lora::*;
#[cfg(feature = "model")]
mod model;
#[cfg(feature = "model")]
pub use model::*;
mod radio;
pub use radio::*;
#[cfg(feature = "sx1262")]
mod sx1262;
#[cfg(feature = "sx1262")]
pub use sx1262::*;
