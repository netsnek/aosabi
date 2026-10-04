// SPDX-License-Identifier: MIT
/*
 * aosabi-carrier-lora radio: the radio a LoRa link runs on, its modulation
 * and the time on air of a payload.
 */

//! The radio, the modulation and the time on air.

use homa::Nanos;

/// A `LoRa` radio, the SX1262 of a card or the model port of the notebooks.
pub trait Radio {
    /// A failure of the radio or of the bus that reaches it.
    type Error;

    /// Tunes the radio to `frequency_hz` with `modulation` and `sync_word`,
    /// and leaves it receiving.
    ///
    /// # Errors
    ///
    /// Returns the radio's error where it refuses the setting.
    fn configure(
        &mut self,
        frequency_hz: u32,
        modulation: Modulation,
        sync_word: u16,
    ) -> Result<(), Self::Error>;

    /// Runs channel activity detection and returns whether the channel is
    /// busy.
    ///
    /// # Errors
    ///
    /// Returns the radio's error where the detection does not run.
    fn channel_busy(&mut self) -> Result<bool, Self::Error>;

    /// Starts the transmission of `payload`, and the radio returns to
    /// receiving once it has ended.
    ///
    /// # Errors
    ///
    /// Returns the radio's error where the transmission does not start.
    fn transmit(&mut self, payload: &[u8]) -> Result<(), Self::Error>;

    /// Takes the next payload the radio received into `buf` and returns its
    /// length, or `None` when none waits.
    ///
    /// # Errors
    ///
    /// Returns the radio's error where the radio cannot be read.
    fn receive(&mut self, buf: &mut [u8]) -> Result<Option<usize>, Self::Error>;
}

/// The private sync word, which the registers `0x0740` and `0x0741` hold, so
/// the radio filters the `LoRaWAN` traffic of the channel, whose public word
/// is `0x3444`.
pub const SYNC_WORD: u16 = 0x1424;

/// The longest payload of a `LoRa` packet, the buffer of the SX1262 of 256
/// bytes less the byte of its length.
pub const MAX_PAYLOAD: usize = 255;

/// A spreading factor, a bandwidth and a coding rate, which fix the time on
/// air.
///
/// The carrier keeps the physical parameters of `LoRaWAN`, a preamble of 8
/// symbols, the explicit header and the CRC of the payload, so the regional
/// tables and every calculator of the time on air apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Modulation {
    /// The spreading factor, 7 to 12.
    pub sf: u8,
    /// The bandwidth in hertz, 125,000 on every link of the design note.
    pub bandwidth_hz: u32,
    /// The coding rate, 1 for 4/5 to 4 for 4/8.
    pub coding_rate: u8,
}

impl Modulation {
    /// Returns the modulation of `LoRaWAN` at the spreading factor `sf`: 125
    /// kHz and the coding rate 4/5.
    #[must_use]
    pub const fn lorawan(sf: u8) -> Self {
        Self {
            sf,
            bandwidth_hz: 125_000,
            coding_rate: 1,
        }
    }

    /// Returns the spreading factor, the nearest of 7 to 12.
    #[must_use]
    pub const fn spreading_factor(self) -> u8 {
        if self.sf < 7 {
            7
        } else if self.sf > 12 {
            12
        } else {
            self.sf
        }
    }
}

/// Returns the time on air of a `LoRa` payload of `len` bytes, the SX1261/2
/// datasheet, revision 1.2, section 6.1.4.
///
/// A symbol lasts `2^SF / BW` seconds, and a packet takes the preamble of 8
/// symbols, 4.25 and 8 of the header, and the payload's
/// `ceil(max(8 * PL - 4 * SF + 44, 0) / (4 * (SF - 2 * LDRO))) * (CR + 4)`
/// symbols, with the CRC of 16 bits and the explicit header of 20 in the 44,
/// and the low data rate optimisation on where a symbol lasts 16.38 ms or
/// more, SF11 and SF12 at 125 kHz. The count is kept in quarter symbols, so
/// the arithmetic stays whole, and the time is rounded up to the nanosecond.
/// A spreading factor outside 7 to 12 takes the nearest, a coding rate
/// outside 1 to 4 the nearest, and a bandwidth of 0 is read as 1 Hz.
#[must_use]
pub const fn time_on_air(modulation: Modulation, len: u8) -> Nanos {
    let sf = modulation.spreading_factor() as u64;
    let bandwidth = if modulation.bandwidth_hz == 0 {
        1
    } else {
        modulation.bandwidth_hz as u64
    };
    let coding_rate = if modulation.coding_rate < 1 {
        1
    } else if modulation.coding_rate > 4 {
        4
    } else {
        modulation.coding_rate as u64
    };
    let symbol_scaled = (1u64 << sf) * 1_000_000_000;
    let ldro = if symbol_scaled >= 16_380_000 * bandwidth {
        1
    } else {
        0
    };
    let bits = (len as u64 * 8 + 44).saturating_sub(4 * sf);
    let per_block = 4 * (sf - 2 * ldro);
    let payload_symbols = bits.div_ceil(per_block) * (coding_rate + 4);
    let quarters = 81 + 4 * payload_symbols;
    let nanos = (quarters as u128 * symbol_scaled as u128).div_ceil(4 * bandwidth as u128);
    if nanos > Nanos::MAX as u128 {
        Nanos::MAX
    } else {
        // The time is at most Nanos::MAX where the cast runs.
        #[expect(clippy::cast_possible_truncation)]
        let nanos = nanos as Nanos;
        nanos
    }
}
