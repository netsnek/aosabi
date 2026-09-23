// SPDX-License-Identifier: MIT
/*
 * homa sim radio: half duplex LoRa radios with a time on air and collisions.
 *
 * The model is the Radio of the Homa design note, second version,
 * homa/design.md section 7.8, lines 2784 to 2815: LoRa with the time on air
 * of its section 6.2, lines 1754 to 1809, half duplex radios, channel
 * activity detection, collisions of packets that overlap in time at the same
 * spreading factor, and loss. The time on air is the formula of the SX1261/2
 * datasheet, revision 1.2, section 6.1.4, at 125 kHz, the coding rate 4/5, a
 * preamble of 8 symbols, the explicit header and the CRC, as section 6.2 of
 * the note reads it.
 */

//! The simulated `LoRa` radios.
//!
//! A [`Radio`] builds a [`RadioNet`] of radios that all hear each other, the
//! last `interferers` of which transmit at random and never listen. A radio
//! moves raw `LoRa` payloads of 1 to [`MAX_PAYLOAD`] bytes through its
//! [`RadioPort`], and a payload takes the time on air of its spreading
//! factor, [`Radio::time_on_air`].
//!
//! A transmission starts `turnaround` after the call that asks for it, the
//! time a radio takes to switch from receiving to sending, and occupies its
//! radio from the call to its end, so a radio sends one payload at a time and
//! hears nothing while it sends. Two transmissions at the same spreading
//! factor that overlap in time collide, and every radio loses both. A radio
//! hears a transmission of another radio at its own spreading factor that
//! collided with none, unless its own transmission overlapped it or the
//! stream "loss" of the generator drops it, `loss_permille` in a thousand
//! for each radio that would have heard it. What a radio heard waits in its
//! inbox until its port takes it.
//!
//! Channel activity detection, [`RadioPort::channel_busy`], finds a
//! transmission of another radio at the same spreading factor on the air at
//! the time the net reads, which a transmission inside its `turnaround` is
//! not yet, so two radios that detect a free channel at the same moment still
//! collide, as radios do.
//!
//! An interferer sends payloads of 16 to 255 random bytes with silences drawn
//! from the stream "interference", so its share of the air is
//! `duty_permille` in a thousand on average, the most a transmitter of its
//! band may take.
//!
//! [`RadioNet::step`] moves the net to a time, ends every transmission due by
//! then, delivers it and writes a [`TraceEvent::Air`] for every
//! transmission that ended and a [`TraceEvent::Lost`] for every radio that
//! lost one, in the order they happened, and returns the time of the next
//! event.

use alloc::{
    collections::VecDeque,
    vec::Vec, //
};

use crate::{
    sim::{
        Rng,
        Trace,
        TraceEvent, //
    },
    Nanos, //
};

/// The longest payload a radio moves, the buffer of the SX1262 less the byte
/// of its length.
pub const MAX_PAYLOAD: usize = 255;

/// The longest time on air of any payload at any spreading factor, 255 bytes
/// at SF12, rounded up, which bounds the transmissions a delivery looks back
/// over.
const LONGEST_AIR: Nanos = 9_100_000_000;

/// The shortest payload an interferer sends.
const INTERFERENCE_MIN: u64 = 16;

/// Half duplex radios with a time on air per packet, channel activity
/// detection and collisions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Radio {
    /// The spreading factor, 7 to 12, of every radio the net builds.
    pub sf: u8,
    /// The share of the air an interferer takes, per mille.
    pub duty_permille: u16,
    /// The time from the call that asks for a transmission to its start.
    pub turnaround: Nanos,
    /// The receptions lost to noise, per mille of the radios that would
    /// have heard a transmission.
    pub loss_permille: u16,
}

impl Radio {
    /// Builds `radios` radios, the last `interferers` of which transmit at
    /// random, drawing the losses from the stream "loss" of `rng` and the
    /// interference from its stream "interference".
    #[must_use]
    pub fn build(self, radios: u8, interferers: u8, rng: Rng) -> RadioNet {
        let mut interference = rng.stream("interference");
        let first_interferer = radios.saturating_sub(interferers);
        let stations = (0..radios)
            .map(|index| {
                let interferer = index >= first_interferer;
                let next_start = if interferer {
                    self.silence(&mut interference, self.time_on_air(MAX_PAYLOAD))
                } else {
                    Nanos::MAX
                };
                Station {
                    sf: self.sf,
                    inbox: VecDeque::new(),
                    busy_until: 0,
                    interferer,
                    next_start,
                }
            })
            .collect();
        RadioNet {
            radio: self,
            loss: rng.stream("loss"),
            interference,
            now: 0,
            stations,
            air: Vec::new(),
            live: Vec::new(),
            collisions: 0,
            stats: RadioStats::default(),
        }
    }

    /// Returns the time on air of a payload of `len` bytes at the spreading
    /// factor of the net.
    #[must_use]
    pub const fn time_on_air(&self, len: usize) -> Nanos {
        air(self.sf, len)
    }

    /// Returns the silence an interferer keeps after a payload of `air`, so
    /// that its share of the air is `duty_permille` on average, and none at
    /// all where it is 0.
    fn silence(&self, rng: &mut Rng, air: Nanos) -> Nanos {
        let duty = u64::from(self.duty_permille.min(1_000));
        if duty == 0 {
            return Nanos::MAX;
        }
        let mean = u128::from(air) * u128::from(1_000 - duty) / u128::from(duty);
        let drawn = mean * u128::from(rng.below(2_001)) / 1_000;
        Nanos::try_from(drawn).unwrap_or(Nanos::MAX)
    }
}

/// Returns the time on air of a `LoRa` payload of `len` bytes at the
/// spreading factor `sf`, at 125 kHz, the coding rate 4/5, a preamble of 8
/// symbols, the explicit header and the CRC.
///
/// The symbols are the preamble of 8, 4.25 and 8 of the header, and the
/// payload's
/// `ceil(max(8 * PL - 4 * SF + 44, 0) / (4 * (SF - 2 * LDRO))) * 5`, with the
/// low data rate optimisation on at SF11 and SF12, where a symbol lasts 16.38
/// ms or more, and a symbol at 125 kHz lasts `2^SF * 8` microseconds. The
/// count is kept in quarter symbols, so the arithmetic stays whole. A
/// spreading factor outside 7 to 12 takes the nearest.
const fn air(sf: u8, len: usize) -> Nanos {
    let sf = if sf < 7 {
        7
    } else if sf > 12 {
        12
    } else {
        sf
    };
    let sf = sf as u64;
    let ldro = if sf >= 11 { 1 } else { 0 };
    let bits = (len as u64 * 8 + 44).saturating_sub(4 * sf);
    let per_block = 4 * (sf - 2 * ldro);
    let payload_symbols = bits.div_ceil(per_block) * 5;
    let quarters = 81 + 4 * payload_symbols;
    quarters * (1 << sf) * 2_000
}

/// Why a radio refused a transmission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransmitError {
    /// The radio is still sending its last payload.
    Transmitting,
    /// The payload is empty or longer than [`MAX_PAYLOAD`].
    Length,
}

impl core::fmt::Display for TransmitError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::Transmitting => "the radio is still sending",
            Self::Length => "the payload is empty or longer than 255 bytes",
        })
    }
}

impl core::error::Error for TransmitError {}

/// What a net counted since it was built.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RadioStats {
    /// Transmissions that ended.
    pub transmissions: u64,
    /// Payloads a radio took into its inbox.
    pub delivered: u64,
    /// Receptions lost because the transmission collided.
    pub lost_collision: u64,
    /// Receptions lost because the receiving radio was sending.
    pub lost_half_duplex: u64,
    /// Receptions lost to noise.
    pub lost_noise: u64,
}

/// One transmission.
#[derive(Clone, Debug)]
struct Transmission {
    /// The radio that sent it.
    radio: u8,
    /// Its spreading factor.
    sf: u8,
    /// The call that asked for it, from which its radio is busy.
    call: Nanos,
    /// When its first symbol went on the air.
    start: Nanos,
    /// When its last symbol left the air.
    end: Nanos,
    /// The payload.
    payload: Vec<u8>,
    /// Whether another transmission at its spreading factor overlapped it.
    collided: bool,
}

/// One radio of the net.
#[derive(Clone, Debug)]
struct Station {
    /// Its spreading factor.
    sf: u8,
    /// The payloads it heard and its port has not taken.
    inbox: VecDeque<Vec<u8>>,
    /// The end of its last transmission.
    busy_until: Nanos,
    /// Whether it transmits at random and never listens.
    interferer: bool,
    /// When an interferer starts its next transmission.
    next_start: Nanos,
}

/// The radios a [`Radio`] builds.
#[derive(Clone, Debug)]
pub struct RadioNet {
    /// The shape of the net.
    radio: Radio,
    /// The generator of the losses to noise.
    loss: Rng,
    /// The generator of the interference.
    interference: Rng,
    /// The time of the last step.
    now: Nanos,
    /// The radios.
    stations: Vec<Station>,
    /// Every transmission, in the order of the calls that asked for them.
    air: Vec<Transmission>,
    /// The indices in `air` of the transmissions that have not ended.
    live: Vec<usize>,
    /// Transmissions lost to a collision.
    collisions: u64,
    /// What the net counted.
    stats: RadioStats,
}

impl RadioNet {
    /// Returns radio `r` as a port, which acts at the time of the last step.
    ///
    /// # Panics
    ///
    /// Panics where `r` names no radio of the net, which is a fault of the
    /// caller that built it.
    pub fn radio(&mut self, r: u8) -> RadioPort<'_> {
        assert!(
            usize::from(r) < self.stations.len(),
            "radio {r} is not in the net"
        );
        RadioPort { net: self, r }
    }

    /// Returns the radios of the net.
    #[must_use]
    pub fn radios(&self) -> u8 {
        u8::try_from(self.stations.len()).unwrap_or(u8::MAX)
    }

    /// Returns the time of the last step.
    #[must_use]
    pub const fn now(&self) -> Nanos {
        self.now
    }

    /// Moves the net to `now`, starts the transmissions of the interferers
    /// and ends every transmission due by then, in the order they happen, and
    /// returns the time of the next event.
    ///
    /// A time before the last step moves nothing.
    pub fn step(&mut self, now: Nanos, trace: &mut Trace) -> Option<Nanos> {
        let now = now.max(self.now);
        loop {
            let start = self.next_interference();
            let end = self.next_end();
            match (start, end) {
                (Some((at, r)), Some((ends, _))) if at <= now && at < ends => {
                    self.interfere(at, r);
                }
                (Some((at, r)), None) if at <= now => self.interfere(at, r),
                (_, Some((ends, index))) if ends <= now => self.deliver(index, trace),
                _ => break,
            }
        }
        self.now = now;
        let start = self.next_interference().map(|(at, _)| at);
        let end = self.next_end().map(|(at, _)| at);
        match (start, end) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    /// Returns the transmissions lost to a collision since the net was built.
    #[must_use]
    pub const fn collisions(&self) -> u64 {
        self.collisions
    }

    /// Returns the air radio `r` used in the last `window` before the time
    /// of the last step, the parts of its transmissions inside that window.
    #[must_use]
    pub fn air_used(&self, r: u8, window: Nanos) -> Nanos {
        let from = self.now.saturating_sub(window);
        self.air
            .iter()
            .filter(|t| t.radio == r && t.end > from && t.start < self.now)
            .map(|t| t.end.min(self.now) - t.start.max(from))
            .sum()
    }

    /// Returns what the net counted since it was built.
    #[must_use]
    pub const fn stats(&self) -> RadioStats {
        self.stats
    }

    /// Returns the earliest start of an interferer and its radio.
    fn next_interference(&self) -> Option<(Nanos, u8)> {
        self.stations
            .iter()
            .enumerate()
            .filter(|(_, s)| s.interferer && s.next_start != Nanos::MAX)
            .map(|(r, s)| (s.next_start, u8::try_from(r).unwrap_or(u8::MAX)))
            .min()
    }

    /// Returns the earliest end among the transmissions that have not ended
    /// and its index in `air`.
    fn next_end(&self) -> Option<(Nanos, usize)> {
        self.live
            .iter()
            .filter_map(|&index| self.air.get(index).map(|t| (t.end, index)))
            .min()
    }

    /// Starts the next transmission of interferer `r` at `at` and draws the
    /// silence after it.
    fn interfere(&mut self, at: Nanos, r: u8) {
        let span = u64::try_from(MAX_PAYLOAD).unwrap_or(255) - INTERFERENCE_MIN + 1;
        let len = usize::try_from(INTERFERENCE_MIN + self.interference.below(span)).unwrap_or(0);
        let mut payload = Vec::with_capacity(len);
        while payload.len() < len {
            payload.extend_from_slice(&self.interference.next_u64().to_le_bytes());
        }
        payload.truncate(len);
        let air = self.radio.time_on_air(len);
        self.begin(r, at, payload);
        let silence = self.radio.silence(&mut self.interference, air);
        if let Some(station) = self.stations.get_mut(usize::from(r)) {
            station.next_start = at
                .saturating_add(self.radio.turnaround)
                .saturating_add(air)
                .saturating_add(silence);
        }
    }

    /// Records a transmission of radio `r` asked for at `call`, marks the
    /// transmissions it overlaps at its spreading factor as collided, and
    /// returns its time on air.
    fn begin(&mut self, r: u8, call: Nanos, payload: Vec<u8>) -> Nanos {
        let sf = self
            .stations
            .get(usize::from(r))
            .map_or(self.radio.sf, |s| s.sf);
        let air = air(sf, payload.len());
        let start = call.saturating_add(self.radio.turnaround);
        let end = start.saturating_add(air);
        let mut collided = false;
        for &index in &self.live {
            let Some(other) = self.air.get_mut(index) else {
                continue;
            };
            if other.sf == sf && other.start < end && start < other.end {
                if !other.collided {
                    self.collisions += 1;
                }
                other.collided = true;
                collided = true;
            }
        }
        if collided {
            self.collisions += 1;
        }
        if let Some(station) = self.stations.get_mut(usize::from(r)) {
            station.busy_until = end;
        }
        self.live.push(self.air.len());
        self.air.push(Transmission {
            radio: r,
            sf,
            call,
            start,
            end,
            payload,
            collided,
        });
        air
    }

    /// Ends the transmission at `index` of `air`, hands its payload to every
    /// radio that hears it, and records what happened.
    fn deliver(&mut self, index: usize, trace: &mut Trace) {
        self.live.retain(|&live| live != index);
        let Some(t) = self.air.get(index).cloned() else {
            return;
        };
        self.stats.transmissions += 1;
        trace.events.push(TraceEvent::Air {
            at: t.start,
            radio: t.radio,
            air: t.end - t.start,
            len: u16::try_from(t.payload.len()).unwrap_or(u16::MAX),
            collided: t.collided,
        });
        for r in 0..self.stations.len() {
            let receiver = u8::try_from(r).unwrap_or(u8::MAX);
            let (interferer, sf) = (self.stations[r].interferer, self.stations[r].sf);
            if receiver == t.radio || interferer || sf != t.sf {
                continue;
            }
            let reason = if t.collided {
                self.stats.lost_collision += 1;
                "collision"
            } else if self.sent_during(receiver, t.start, t.end) {
                self.stats.lost_half_duplex += 1;
                "half duplex"
            } else if self.loss.below(1_000) < u64::from(self.radio.loss_permille) {
                self.stats.lost_noise += 1;
                "noise"
            } else {
                self.stations[r].inbox.push_back(t.payload.clone());
                self.stats.delivered += 1;
                continue;
            };
            trace.events.push(TraceEvent::Lost {
                at: t.end,
                radio: receiver,
                from: t.radio,
                reason,
            });
        }
    }

    /// Returns whether radio `r` was busy with a transmission of its own at
    /// any time from `start` to `end`.
    fn sent_during(&self, r: u8, start: Nanos, end: Nanos) -> bool {
        let horizon = start
            .saturating_sub(self.radio.turnaround)
            .saturating_sub(LONGEST_AIR);
        self.air
            .iter()
            .rev()
            .take_while(|t| t.call >= horizon)
            .any(|t| t.radio == r && t.call < end && t.end > start)
    }
}

/// One radio moving raw `LoRa` payloads of up to 255 bytes with the time on
/// air of its spreading factor.
#[derive(Debug)]
pub struct RadioPort<'n> {
    /// The net.
    net: &'n mut RadioNet,
    /// The radio.
    r: u8,
}

impl RadioPort<'_> {
    /// Returns the index of the radio in its net.
    #[must_use]
    pub const fn index(&self) -> u8 {
        self.r
    }

    /// Returns the time the net reads, the time of its last step.
    #[must_use]
    pub const fn now(&self) -> Nanos {
        self.net.now
    }

    /// Returns the spreading factor of the radio.
    #[must_use]
    pub fn sf(&self) -> u8 {
        self.station().sf
    }

    /// Sets the spreading factor of the radio, which hears only
    /// transmissions at its own.
    pub fn set_sf(&mut self, sf: u8) {
        if let Some(station) = self.net.stations.get_mut(usize::from(self.r)) {
            station.sf = sf.clamp(7, 12);
        }
    }

    /// Returns the time on air of a payload of `len` bytes at the spreading
    /// factor of the radio.
    #[must_use]
    pub fn time_on_air(&self, len: usize) -> Nanos {
        air(self.sf(), len)
    }

    /// Runs channel activity detection and returns whether a transmission
    /// of another radio at the spreading factor of this one is on the air.
    #[must_use]
    pub fn channel_busy(&self) -> bool {
        let now = self.net.now;
        let sf = self.sf();
        self.net.live.iter().any(|&index| {
            self.net
                .air
                .get(index)
                .is_some_and(|t| t.radio != self.r && t.sf == sf && t.start <= now && now < t.end)
        })
    }

    /// Returns whether the radio is still busy with its last transmission,
    /// its turnaround included.
    #[must_use]
    pub fn transmitting(&self) -> bool {
        self.net.now < self.station().busy_until
    }

    /// Starts a transmission of `payload` after the turnaround and returns
    /// its time on air.
    ///
    /// # Errors
    ///
    /// Returns [`TransmitError::Transmitting`] while the last transmission
    /// has not ended and [`TransmitError::Length`] for an empty payload or
    /// one longer than [`MAX_PAYLOAD`], and sends nothing.
    pub fn transmit(&mut self, payload: &[u8]) -> Result<Nanos, TransmitError> {
        if payload.is_empty() || payload.len() > MAX_PAYLOAD {
            return Err(TransmitError::Length);
        }
        if self.transmitting() {
            return Err(TransmitError::Transmitting);
        }
        let now = self.net.now;
        Ok(self.net.begin(self.r, now, payload.to_vec()))
    }

    /// Takes the oldest payload the radio heard into `buf` and returns its
    /// length, or `None` when it heard none. A payload longer than `buf`
    /// keeps the bytes that fit.
    pub fn receive(&mut self, buf: &mut [u8]) -> Option<usize> {
        let station = self.net.stations.get_mut(usize::from(self.r))?;
        let payload = station.inbox.pop_front()?;
        let len = payload.len().min(buf.len());
        buf[..len].copy_from_slice(&payload[..len]);
        Some(len)
    }

    /// Returns the payloads the radio heard and has not taken.
    #[must_use]
    pub fn pending(&self) -> usize {
        self.station().inbox.len()
    }

    /// Returns the station of the radio.
    fn station(&self) -> &Station {
        &self.net.stations[usize::from(self.r)]
    }
}
