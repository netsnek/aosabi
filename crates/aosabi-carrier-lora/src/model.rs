// SPDX-License-Identifier: MIT
/*
 * aosabi-carrier-lora model: the radio model of homa::sim as a radio of the
 * carrier.
 *
 * The devtool alone builds it.
 */

//! The model radio of the notebooks.

use homa::sim::{
    RadioPort,
    TransmitError, //
};

use crate::{
    Modulation,
    Radio, //
};

/// The bandwidth the model moves payloads at.
const MODEL_BANDWIDTH: u32 = 125_000;

/// The coding rate the model moves payloads at, 4/5.
const MODEL_CODING_RATE: u8 = 1;

/// Why the model radio refused a call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelError {
    /// The modulation is not the one the model times, 125 kHz and the coding
    /// rate 4/5.
    Modulation,
    /// The radio refused the transmission.
    Transmit(TransmitError),
}

impl core::fmt::Display for ModelError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Modulation => {
                f.write_str("the model times 125 kHz and the coding rate 4/5 alone")
            }
            Self::Transmit(error) => write!(f, "the model radio refused: {error}"),
        }
    }
}

/// A port of the radio model of `homa::sim` is a radio of the carrier. The
/// frequency and the sync word do not change what the model hears, since
/// every radio of a net shares one channel and one word.
impl Radio for RadioPort<'_> {
    type Error = ModelError;

    fn configure(
        &mut self,
        _frequency_hz: u32,
        modulation: Modulation,
        _sync_word: u16,
    ) -> Result<(), ModelError> {
        if modulation.bandwidth_hz != MODEL_BANDWIDTH || modulation.coding_rate != MODEL_CODING_RATE
        {
            return Err(ModelError::Modulation);
        }
        self.set_sf(modulation.spreading_factor());
        Ok(())
    }

    fn channel_busy(&mut self) -> Result<bool, ModelError> {
        Ok(RadioPort::channel_busy(self))
    }

    fn transmit(&mut self, payload: &[u8]) -> Result<(), ModelError> {
        RadioPort::transmit(self, payload)
            .map(|_| ())
            .map_err(ModelError::Transmit)
    }

    fn receive(&mut self, buf: &mut [u8]) -> Result<Option<usize>, ModelError> {
        Ok(RadioPort::receive(self, buf))
    }
}
