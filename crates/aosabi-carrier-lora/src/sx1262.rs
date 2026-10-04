// SPDX-License-Identifier: MIT
/*
 * aosabi-carrier-lora sx1262: the SX1262 over the SPI and the pins of the
 * card's HAL, the command set of the datasheet sent as bytes.
 *
 * The commands, their opcodes and their bytes are those of the SX1261/2
 * datasheet, revision 1.2 of June 2019, Tables 9-2, 11-1 to 11-5, 13-17 to
 * 13-19, 13-21, 13-27, 13-29, 13-30, 13-32 to 13-36, 13-47 to 13-51, 13-66 to
 * 13-73, 13-78 and 13-86, and the registers 0x0740 and 0x0741 of the sync
 * word of its register list, fetched from
 * cdn.sparkfun.com/assets/6/b/5/1/4/SX1262_datasheet.pdf with the SHA-256
 * 644b55f38e97309161ea0c952a3a093020d94268ca8276eca53e5aa933ae7068. The
 * driver is written in the project because the driver crates that exist are
 * not well known under ADR-0024, and the SPI and the pins come from the
 * card's HAL through embedded-hal.
 */

//! The SX1262 radio of a card.
//!
//! [`Sx1262`] drives the radio through the four traits of `embedded-hal` the
//! card's HAL implements: the SPI device, the pin BUSY, the pin NRESET and a
//! delay. Every command waits for BUSY to fall, for at most 10 ms, and a
//! radio that stays busy answers [`Sx1262Error::Timeout`].
//!
//! [`Radio::configure`] resets the radio, powers a TCXO over DIO3 and
//! calibrates every block again on its clock where the card names one with
//! [`Sx1262::with_tcxo`], hands DIO2 the card's RF switch where the card
//! names one with [`Sx1262::with_rf_switch`], sets `LoRa`, calibrates the
//! image for the band of Table 9-2 that holds the frequency, and sets the
//! frequency, the modulation, the packet of a preamble of 8 symbols, the
//! explicit header and the CRC, the sync word, the interrupts and the power,
//! and leaves it receiving continuously. After power on the radio holds the
//! image calibration of 902 to 928 MHz, which a frequency outside every band
//! of Table 9-2 keeps. [`Radio::transmit`] writes the payload to the
//! buffer and starts the transmission, which ends with `TxDone`, and
//! [`Radio::receive`] reads the interrupts, returns the radio to receiving
//! after a transmission, drops a packet whose header or CRC failed, and reads
//! a received payload out of the buffer. [`Radio::channel_busy`] runs channel
//! activity detection over two symbols and waits for `CadDone`.

use embedded_hal::{
    delay::DelayNs,
    digital::{
        InputPin,
        OutputPin, //
    },
    spi::{
        Operation,
        SpiDevice, //
    }, //
};

use crate::{
    MAX_PAYLOAD,
    Modulation,
    Radio, //
};

/// `SetStandby`, Table 11-1.
const SET_STANDBY: u8 = 0x80;
/// `SetTx`, Table 11-1.
const SET_TX: u8 = 0x83;
/// `SetRx`, Table 11-1.
const SET_RX: u8 = 0x82;
/// `SetCad`, Table 11-1.
const SET_CAD: u8 = 0xC5;
/// `Calibrate`, Table 11-1.
const CALIBRATE: u8 = 0x89;
/// `CalibrateImage`, Table 11-1.
const CALIBRATE_IMAGE: u8 = 0x98;
/// `SetPaConfig`, Table 11-1.
const SET_PA_CONFIG: u8 = 0x95;
/// `WriteRegister`, Table 11-2.
const WRITE_REGISTER: u8 = 0x0D;
/// `WriteBuffer`, Table 11-2.
const WRITE_BUFFER: u8 = 0x0E;
/// `ReadBuffer`, Table 11-2.
const READ_BUFFER: u8 = 0x1E;
/// `SetDioIrqParams`, Table 11-3.
const SET_DIO_IRQ_PARAMS: u8 = 0x08;
/// `GetIrqStatus`, Table 11-3.
const GET_IRQ_STATUS: u8 = 0x12;
/// `ClearIrqStatus`, Table 11-3.
const CLEAR_IRQ_STATUS: u8 = 0x02;
/// `SetDIO2AsRfSwitchCtrl`, Table 11-3.
const SET_DIO2_AS_RF_SWITCH_CTRL: u8 = 0x9D;
/// `SetDIO3AsTcxoCtrl`, Table 11-3.
const SET_DIO3_AS_TCXO_CTRL: u8 = 0x97;
/// `SetRfFrequency`, Table 11-4.
const SET_RF_FREQUENCY: u8 = 0x86;
/// `SetPacketType`, Table 11-4.
const SET_PACKET_TYPE: u8 = 0x8A;
/// `SetTxParams`, Table 11-4.
const SET_TX_PARAMS: u8 = 0x8E;
/// `SetModulationParams`, Table 11-4.
const SET_MODULATION_PARAMS: u8 = 0x8B;
/// `SetPacketParams`, Table 11-4.
const SET_PACKET_PARAMS: u8 = 0x8C;
/// `SetCadParams`, Table 11-4.
const SET_CAD_PARAMS: u8 = 0x88;
/// `SetBufferBaseAddress`, Table 11-4.
const SET_BUFFER_BASE_ADDRESS: u8 = 0x8F;
/// `GetRxBufferStatus`, Table 11-5.
const GET_RX_BUFFER_STATUS: u8 = 0x13;
/// `ClearDeviceErrors`, Table 11-5.
const CLEAR_DEVICE_ERRORS: u8 = 0x07;

/// Every block `Calibrate` calibrates, bits 0 to 6 of Table 13-18.
const CALIBRATE_ALL: u8 = 0x7F;
/// The bands of the image calibration, from and to in Hz, with the `freq1`
/// and the `freq2` of `CalibrateImage`, Table 9-2.
const IMAGE_BANDS: [(u32, u32, u8, u8); 5] = [
    (430_000_000, 440_000_000, 0x6B, 0x6F),
    (470_000_000, 510_000_000, 0x75, 0x81),
    (779_000_000, 787_000_000, 0xC1, 0xC5),
    (863_000_000, 870_000_000, 0xD7, 0xDB),
    (902_000_000, 928_000_000, 0xE1, 0xE9),
];

/// The packet type of `LoRa`, Table 13-37.
const PACKET_TYPE_LORA: u8 = 0x01;
/// The register of the most significant byte of the sync word, whose next
/// register holds the least significant byte.
const SYNC_WORD_REGISTER: u16 = 0x0740;

/// `TxDone`, bit 0 of Table 13-29.
const IRQ_TX_DONE: u16 = 1 << 0;
/// `RxDone`, bit 1.
const IRQ_RX_DONE: u16 = 1 << 1;
/// `HeaderErr`, bit 5.
const IRQ_HEADER_ERR: u16 = 1 << 5;
/// `CrcErr`, bit 6.
const IRQ_CRC_ERR: u16 = 1 << 6;
/// `CadDone`, bit 7.
const IRQ_CAD_DONE: u16 = 1 << 7;
/// `CadDetected`, bit 8.
const IRQ_CAD_DETECTED: u16 = 1 << 8;
/// `Timeout`, bit 9.
const IRQ_TIMEOUT: u16 = 1 << 9;
/// Every interrupt the driver reads.
const IRQ_USED: u16 = IRQ_TX_DONE
    | IRQ_RX_DONE
    | IRQ_HEADER_ERR
    | IRQ_CRC_ERR
    | IRQ_CAD_DONE
    | IRQ_CAD_DETECTED
    | IRQ_TIMEOUT;

/// The preamble in symbols, the length `LoRaWAN` keeps.
const PREAMBLE: u16 = 8;
/// The polls of BUSY before a command gives up, 1,000 of 10 microseconds.
const BUSY_POLLS: u32 = 1_000;
/// The polls of `CadDone`, 200 of 1 millisecond, longer than two symbols at
/// SF12, 65.5 ms.
const CAD_POLLS: u32 = 200;
/// The reference of the SX1262, a crystal or a TCXO of 32 MHz, whose
/// frequency over 2^25 is the step of `SetRfFrequency`.
const XTAL_HZ: u64 = 32_000_000;

/// Why the SX1262 failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sx1262Error<S, B, R> {
    /// The SPI device failed.
    Spi(S),
    /// The pin BUSY could not be read.
    Busy(B),
    /// The pin NRESET could not be set.
    Reset(R),
    /// The radio stayed busy, or a detection did not end, within its bound.
    Timeout,
    /// A payload was empty or longer than a `LoRa` payload.
    Length,
    /// The modulation names a bandwidth or a coding rate the radio has no
    /// code for.
    Modulation,
}

/// The supply DIO3 gives the TCXO of a card, `tcxoVoltage` of Table 13-35.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TcxoVoltage {
    /// 1.6 V.
    V1_6 = 0x00,
    /// 1.7 V.
    V1_7 = 0x01,
    /// 1.8 V.
    V1_8 = 0x02,
    /// 2.2 V.
    V2_2 = 0x03,
    /// 2.4 V.
    V2_4 = 0x04,
    /// 2.7 V.
    V2_7 = 0x05,
    /// 3.0 V.
    V3_0 = 0x06,
    /// 3.3 V.
    V3_3 = 0x07,
}

/// The SX1262 over the SPI of the card's HAL, the command set of the
/// datasheet sent as bytes.
#[derive(Debug)]
pub struct Sx1262<Spi, Busy, Reset, Delay> {
    /// The SPI device with its chip select.
    spi: Spi,
    /// The pin BUSY.
    busy: Busy,
    /// The pin NRESET.
    reset: Reset,
    /// The delay.
    delay: Delay,
    /// The output power in dBm.
    power_dbm: i8,
    /// The spreading factor, which sets the sensitivity of a detection.
    sf: u8,
    /// The supply of a TCXO that DIO3 powers and the microseconds its clock
    /// takes to settle, `None` on a card with a crystal.
    tcxo: Option<(TcxoVoltage, u32)>,
    /// Whether DIO2 drives the card's RF switch.
    rf_switch: bool,
}

/// The error of an [`Sx1262`] over its four parts.
type Error<Spi, Busy, Reset> = Sx1262Error<
    <Spi as embedded_hal::spi::ErrorType>::Error,
    <Busy as embedded_hal::digital::ErrorType>::Error,
    <Reset as embedded_hal::digital::ErrorType>::Error,
>;

impl<Spi, Busy, Reset, Delay> Sx1262<Spi, Busy, Reset, Delay>
where
    Spi: SpiDevice,
    Busy: InputPin,
    Reset: OutputPin,
    Delay: DelayNs,
{
    /// Returns the driver over the card's SPI device, its pins BUSY and
    /// NRESET and a delay, at 14 dBm, the power of the band of 1 percent.
    pub const fn new(spi: Spi, busy: Busy, reset: Reset, delay: Delay) -> Self {
        Self {
            spi,
            busy,
            reset,
            delay,
            power_dbm: 14,
            sf: 7,
            tcxo: None,
            rf_switch: false,
        }
    }

    /// Returns the driver set to `power_dbm`, which the next
    /// [`Radio::configure`] applies, 14, 17, 20 or 22 dBm and the next of
    /// them above any other value.
    #[must_use]
    pub const fn with_power(mut self, power_dbm: i8) -> Self {
        self.power_dbm = power_dbm;
        self
    }

    /// Returns the driver of a card whose TCXO DIO3 powers at `voltage` and
    /// whose clock the radio gates for `delay_us` after it powers it, which
    /// the next [`Radio::configure`] applies, Tables 13-34 and 13-35. The
    /// delay counts in steps of 15.625 microseconds, at most `0xFF_FFFF` of
    /// them.
    #[must_use]
    pub const fn with_tcxo(mut self, voltage: TcxoVoltage, delay_us: u32) -> Self {
        self.tcxo = Some((voltage, delay_us));
        self
    }

    /// Returns the driver of a card whose RF switch DIO2 drives, high while
    /// the radio sends and low otherwise, which the next
    /// [`Radio::configure`] applies, Tables 13-32 and 13-33.
    #[must_use]
    pub const fn with_rf_switch(mut self) -> Self {
        self.rf_switch = true;
        self
    }

    /// Waits until the radio takes a command.
    fn wait(&mut self) -> Result<(), Error<Spi, Busy, Reset>> {
        for _ in 0..BUSY_POLLS {
            if !self.busy.is_high().map_err(Sx1262Error::Busy)? {
                return Ok(());
            }
            self.delay.delay_us(10);
        }
        Err(Sx1262Error::Timeout)
    }

    /// Sends one command with its parameters.
    fn command(&mut self, bytes: &[u8]) -> Result<(), Error<Spi, Busy, Reset>> {
        self.wait()?;
        self.spi.write(bytes).map_err(Sx1262Error::Spi)
    }

    /// Sends `command` and reads the bytes that follow it into `out`.
    fn read(&mut self, command: &[u8], out: &mut [u8]) -> Result<(), Error<Spi, Busy, Reset>> {
        self.wait()?;
        self.spi
            .transaction(&mut [Operation::Write(command), Operation::Read(out)])
            .map_err(Sx1262Error::Spi)
    }

    /// Returns the interrupts that fired, Table 13-30: the status, then the
    /// interrupts, most significant byte first.
    fn irq(&mut self) -> Result<u16, Error<Spi, Busy, Reset>> {
        let mut out = [0u8; 3];
        self.read(&[GET_IRQ_STATUS], &mut out)?;
        Ok(u16::from_be_bytes([out[1], out[2]]))
    }

    /// Clears the interrupts of `mask`.
    fn clear(&mut self, mask: u16) -> Result<(), Error<Spi, Busy, Reset>> {
        let [high, low] = mask.to_be_bytes();
        self.command(&[CLEAR_IRQ_STATUS, high, low])
    }

    /// Sets the packet of `len` bytes: the preamble, the explicit header, the
    /// CRC on and the standard IQ, Tables 13-66 to 13-70.
    fn packet(&mut self, len: u8) -> Result<(), Error<Spi, Busy, Reset>> {
        let [high, low] = PREAMBLE.to_be_bytes();
        self.command(&[SET_PACKET_PARAMS, high, low, 0x00, len, 0x01, 0x00])
    }

    /// Puts the radio in standby on its RC oscillator.
    fn standby(&mut self) -> Result<(), Error<Spi, Busy, Reset>> {
        self.command(&[SET_STANDBY, 0x00])
    }

    /// Puts the radio in continuous receiving, the timeout 0xFFFFFF of Table
    /// 13-8, for payloads of any length.
    fn listen(&mut self) -> Result<(), Error<Spi, Busy, Reset>> {
        self.packet(u8::MAX)?;
        self.command(&[SET_RX, 0xFF, 0xFF, 0xFF])
    }

    /// Returns the settings of the power amplifier for the output power,
    /// Table 13-21: `paDutyCycle`, `hpMax`, `deviceSel` and `paLut`, with
    /// the power of `SetTxParams` at 22 dBm in every row.
    const fn pa_config(power_dbm: i8) -> [u8; 4] {
        if power_dbm <= 14 {
            [0x02, 0x02, 0x00, 0x01]
        } else if power_dbm <= 17 {
            [0x02, 0x03, 0x00, 0x01]
        } else if power_dbm <= 20 {
            [0x03, 0x05, 0x00, 0x01]
        } else {
            [0x04, 0x07, 0x00, 0x01]
        }
    }
}

/// Returns the code of a bandwidth of Table 13-48.
const fn bandwidth_code(hz: u32) -> Option<u8> {
    match hz {
        7_810 => Some(0x00),
        10_420 => Some(0x08),
        15_630 => Some(0x01),
        20_830 => Some(0x09),
        31_250 => Some(0x02),
        41_670 => Some(0x0A),
        62_500 => Some(0x03),
        125_000 => Some(0x04),
        250_000 => Some(0x05),
        500_000 => Some(0x06),
        _ => None,
    }
}

/// Returns the `freq1` and the `freq2` of `CalibrateImage` for the band of
/// Table 9-2 that holds `frequency_hz`, or `None` outside every band.
fn image_band(frequency_hz: u32) -> Option<[u8; 2]> {
    IMAGE_BANDS
        .iter()
        .find(|&&(from, to, _, _)| (from..=to).contains(&frequency_hz))
        .map(|&(_, _, freq1, freq2)| [freq1, freq2])
}

/// Returns the step of `SetRfFrequency` for `frequency_hz`, the frequency
/// times 2^25 over the crystal, Table 13-36.
const fn frequency_step(frequency_hz: u32) -> u32 {
    let step = ((frequency_hz as u64) << 25) / XTAL_HZ;
    if step > u32::MAX as u64 {
        u32::MAX
    } else {
        // The step is at most u32::MAX where the cast runs.
        #[expect(clippy::cast_possible_truncation)]
        let step = step as u32;
        step
    }
}

impl<Spi, Busy, Reset, Delay> Radio for Sx1262<Spi, Busy, Reset, Delay>
where
    Spi: SpiDevice,
    Busy: InputPin,
    Reset: OutputPin,
    Delay: DelayNs,
{
    type Error = Error<Spi, Busy, Reset>;

    fn configure(
        &mut self,
        frequency_hz: u32,
        modulation: Modulation,
        sync_word: u16,
    ) -> Result<(), Self::Error> {
        let bandwidth = bandwidth_code(modulation.bandwidth_hz).ok_or(Sx1262Error::Modulation)?;
        if !(1..=4).contains(&modulation.coding_rate) {
            return Err(Sx1262Error::Modulation);
        }
        let sf = modulation.spreading_factor();
        // The low data rate optimisation is on where a symbol lasts 16.38 ms
        // or more, Table 13-50, as crate::time_on_air counts it.
        let ldro = u8::from(
            (1u64 << sf) * 1_000_000_000 >= 16_380_000 * u64::from(modulation.bandwidth_hz),
        );
        self.reset.set_low().map_err(Sx1262Error::Reset)?;
        self.delay.delay_us(200);
        self.reset.set_high().map_err(Sx1262Error::Reset)?;
        self.delay.delay_ms(5);
        self.standby()?;
        if let Some((voltage, delay_us)) = self.tcxo {
            // The steps of 15.625 us, 8 in 125 us. The calibration at power
            // on ran without the clock of the TCXO and failed, so the radio
            // clears that error and calibrates every block again once DIO3
            // powers it, section 9.2.1.
            let [_, high, middle, low] = (delay_us.saturating_mul(8) / 125)
                .min(0x00FF_FFFF)
                .to_be_bytes();
            self.command(&[SET_DIO3_AS_TCXO_CTRL, voltage as u8, high, middle, low])?;
            self.command(&[CLEAR_DEVICE_ERRORS, 0x00, 0x00])?;
            self.command(&[CALIBRATE, CALIBRATE_ALL])?;
            self.delay.delay_us(delay_us);
        }
        if self.rf_switch {
            self.command(&[SET_DIO2_AS_RF_SWITCH_CTRL, 0x01])?;
        }
        self.command(&[SET_PACKET_TYPE, PACKET_TYPE_LORA])?;
        if let Some([freq1, freq2]) = image_band(frequency_hz) {
            self.command(&[CALIBRATE_IMAGE, freq1, freq2])?;
        }
        let step = frequency_step(frequency_hz).to_be_bytes();
        self.command(&[SET_RF_FREQUENCY, step[0], step[1], step[2], step[3]])?;
        self.command(&[SET_BUFFER_BASE_ADDRESS, 0x00, 0x00])?;
        self.command(&[
            SET_MODULATION_PARAMS,
            sf,
            bandwidth,
            modulation.coding_rate,
            ldro,
        ])?;
        let [register_high, register_low] = SYNC_WORD_REGISTER.to_be_bytes();
        let [word_high, word_low] = sync_word.to_be_bytes();
        self.command(&[
            WRITE_REGISTER,
            register_high,
            register_low,
            word_high,
            word_low,
        ])?;
        let [irq_high, irq_low] = IRQ_USED.to_be_bytes();
        self.command(&[
            SET_DIO_IRQ_PARAMS,
            irq_high,
            irq_low,
            irq_high,
            irq_low,
            0x00,
            0x00,
            0x00,
            0x00,
        ])?;
        let pa = Self::pa_config(self.power_dbm);
        self.command(&[SET_PA_CONFIG, pa[0], pa[1], pa[2], pa[3]])?;
        self.command(&[SET_TX_PARAMS, 22, 0x04])?;
        self.sf = sf;
        self.clear(IRQ_USED)?;
        self.listen()
    }

    fn channel_busy(&mut self) -> Result<bool, Self::Error> {
        let peak = match self.sf {
            7 | 8 => 22,
            9 => 23,
            10 => 24,
            11 => 25,
            _ => 28,
        };
        self.standby()?;
        self.command(&[SET_CAD_PARAMS, 0x01, peak, 10, 0x00, 0x00, 0x00, 0x00])?;
        self.clear(IRQ_USED)?;
        self.command(&[SET_CAD])?;
        for _ in 0..CAD_POLLS {
            let irq = self.irq()?;
            if irq & IRQ_CAD_DONE != 0 {
                self.clear(IRQ_USED)?;
                self.listen()?;
                return Ok(irq & IRQ_CAD_DETECTED != 0);
            }
            self.delay.delay_ms(1);
        }
        self.listen()?;
        Err(Sx1262Error::Timeout)
    }

    fn transmit(&mut self, payload: &[u8]) -> Result<(), Self::Error> {
        let len = u8::try_from(payload.len()).map_err(|_| Sx1262Error::Length)?;
        if payload.is_empty() || payload.len() > MAX_PAYLOAD {
            return Err(Sx1262Error::Length);
        }
        self.standby()?;
        self.packet(len)?;
        self.wait()?;
        self.spi
            .transaction(&mut [
                Operation::Write(&[WRITE_BUFFER, 0x00]),
                Operation::Write(payload),
            ])
            .map_err(Sx1262Error::Spi)?;
        self.clear(IRQ_USED)?;
        self.command(&[SET_TX, 0x00, 0x00, 0x00])
    }

    fn receive(&mut self, buf: &mut [u8]) -> Result<Option<usize>, Self::Error> {
        let irq = self.irq()?;
        if irq & IRQ_TX_DONE != 0 {
            self.clear(IRQ_TX_DONE)?;
            self.listen()?;
            return Ok(None);
        }
        if irq & (IRQ_HEADER_ERR | IRQ_CRC_ERR) != 0 {
            self.clear(IRQ_RX_DONE | IRQ_HEADER_ERR | IRQ_CRC_ERR)?;
            return Ok(None);
        }
        if irq & IRQ_RX_DONE == 0 {
            return Ok(None);
        }
        let mut status = [0u8; 3];
        self.read(&[GET_RX_BUFFER_STATUS], &mut status)?;
        let len = usize::from(status[1]).min(buf.len());
        self.read(&[READ_BUFFER, status[2], 0x00], &mut buf[..len])?;
        self.clear(IRQ_RX_DONE)?;
        Ok(Some(len))
    }
}
