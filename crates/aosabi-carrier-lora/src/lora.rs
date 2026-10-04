// SPDX-License-Identifier: MIT
/*
 * aosabi-carrier-lora lora: the carrier LoRa, the band, the pairing and the
 * values of a LoRa card.
 *
 * The carrier is that of the Homa design note, second version, homa/design.md
 * sections 6.3 to 6.7 and 7.9, lines 1810 to 2024 and 2816 to 2889: one compact
 * packet per LoRa packet sealed in the frame of section 6.5, no grants, the
 * airtime ledger of section 6.3, channel activity detection before every
 * transmission with a busy channel delaying the packet by a random fraction of
 * one packet's time on air, and bursts of at most 2 s of air, at least one
 * packet, followed by a listening gap of twice a RESEND's time on air.
 */

//! The carrier, the band, the pairing and the values of a card.
//!
//! [`Lora`] is a [`homa::Carrier`] whose peer is the link, over a [`Radio`],
//! the [`Counters`] of the direction it sends in, the [`Replay`] window of
//! the direction it receives in and a [`Ledger`]. It holds one packet at a
//! time: [`homa::Carrier::send`] takes a packet and transmits it when the
//! channel is free, and holds it through a busy channel, and
//! [`homa::Carrier::ready`] answers false while a packet waits, while the
//! radio sends, through a listening gap, and where the ledger has no room.
//! The driver hands the carrier its clock with [`Lora::advance`] before
//! every turn of the endpoint, which sends a held packet when it may and
//! returns when the carrier wants its next turn.
//!
//! [`homa::Carrier::ready`] knows the length of a packet and not its type,
//! so a packet of at most [`CONTROL_LEN`] bytes, which every control packet
//! of the compact form is, may take the share of the ledger control keeps,
//! and [`homa::Carrier::send`] charges every packet by its type.
//!
//! Two rules keep the air of the link for what the pairing does not know.
//! A LIMITS that names this end's limits of the pairing does not leave, since
//! both ends hold them from the pairing and no LIMITS crosses the air on a
//! healthy link, while a LIMITS that names other limits leaves, as the
//! design note's section 6.4 keeps `NEED_LIMITS` for a pairing that went
//! stale. And a carrier that has just heard a frame holds its own for two
//! turnarounds, so the next frame of a sender's burst, which starts one
//! turnaround after the last, shows in the channel activity detection and
//! the answer waits for the sender's listening gap in place of colliding
//! with it.

use core::cell::Cell;

use homa::{
    Carrier,
    Limits,
    Nanos,
    Priority,
    Profile,
    Window,
    wire::{
        Compact,
        Form,
        Packet, //
    }, //
};

use crate::{
    BUCKETS,
    CounterStore,
    Counters,
    Direction,
    FrameError,
    Ledger,
    MAX_PAYLOAD,
    MAX_SEALED,
    Modulation,
    OVERHEAD,
    Radio,
    Replay,
    SYNC_WORD,
    open,
    seal,
    time_on_air, //
};

/// The nanoseconds in a millisecond.
const MS: Nanos = 1_000_000;

/// The longest compact packet a ready may count as control: an ACK with five
/// entries, the largest control packet of the compact form.
pub const CONTROL_LEN: usize = 26;

/// The air of a burst, after which a sender listens.
pub const BURST: Nanos = 2_000 * MS;

/// The time the carrier gives its radio to switch from receiving to sending,
/// which the SX1262 takes well within.
pub const TURNAROUND: Nanos = MS;

/// The bytes of a RESEND of the compact form, whose time on air twice over
/// is the listening gap.
const RESEND_LEN: usize = 14;

/// The code of a DATA packet, the first byte of its compact header.
const DATA: u8 = homa::wire::DATA;

/// The longest response the design note gives a `LoRa` link.
const LONGEST_NOTE: u32 = 2_048;

/// The longest response a card raises its link to at SF7 to SF9 in the 10
/// percent band, the default of ADR-0026.
pub const LONGEST_RAISED: u32 = 8_192;

/// The band a link transmits in, which fixes the duty cycle and the longest
/// packet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Band {
    /// 869.4 to 869.65 MHz, 10 percent.
    P,
    /// 868.0 to 868.6 MHz, 1 percent, packets within the `LoRaWAN` ceiling of
    /// the spreading factor.
    M,
}

impl Band {
    /// Returns the duty cycle per mille, 100 in the band P and 10 in the band
    /// M.
    #[must_use]
    pub const fn duty_permille(self) -> u16 {
        match self {
            Self::P => 100,
            Self::M => 10,
        }
    }

    /// Returns the air one transmitter may take in any continuous hour, 360
    /// s in the band P and 36 s in the band M.
    #[must_use]
    pub const fn air_per_hour(self) -> Nanos {
        crate::HOUR / 1_000 * self.duty_permille() as Nanos
    }

    /// Returns the longest single transmission the band advises, 36 s in the
    /// band P and 3.6 s in the band M.
    #[must_use]
    pub const fn longest_transmission(self) -> Nanos {
        match self {
            Self::P => 36_000 * MS,
            Self::M => 3_600 * MS,
        }
    }

    /// Returns the longest frame at the spreading factor `sf`: a whole `LoRa`
    /// payload in the band P, and in the band M the `LoRaWAN` ceiling of the
    /// spreading factor, 255 bytes at SF7 and SF8, 128 at SF9 and 64 above.
    #[must_use]
    pub const fn max_frame(self, sf: u8) -> usize {
        match self {
            Self::P => MAX_PAYLOAD,
            Self::M if sf <= 8 => MAX_PAYLOAD,
            Self::M if sf == 9 => 128,
            Self::M => 64,
        }
    }

    /// Returns the longest compact packet at the spreading factor `sf`, the
    /// longest frame less the 21 bytes the frame adds: 234, 107 or 43.
    #[must_use]
    pub const fn max_packet(self, sf: u8) -> u16 {
        // The frame is at most 255 bytes, so the difference fits.
        #[expect(clippy::cast_possible_truncation)]
        let packet = (self.max_frame(sf) - OVERHEAD) as u16;
        packet
    }
}

/// What the pairing wrote into both ends.
#[derive(Clone, Copy)]
pub struct Pairing {
    /// The link, 1 to 255, the first byte of every frame.
    pub link: u8,
    /// The key of ChaCha20-Poly1305.
    pub key: [u8; 32],
    /// The channel.
    pub frequency_hz: u32,
    /// The band of the channel.
    pub band: Band,
    /// The modulation of the link.
    pub modulation: Modulation,
    /// What this end accepts.
    pub local: Limits,
    /// What the other end accepts.
    pub peer: Limits,
}

impl core::fmt::Debug for Pairing {
    /// Writes every field but the key, which a log never holds.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Pairing")
            .field("link", &self.link)
            .field("frequency_hz", &self.frequency_hz)
            .field("band", &self.band)
            .field("modulation", &self.modulation)
            .field("local", &self.local)
            .field("peer", &self.peer)
            .finish_non_exhaustive()
    }
}

/// Returns the longest response of a link of `modulation` in `band`: 8,192
/// bytes at SF7 to SF9 in the band P, where a card raises it for introspection
/// by ADR-0026, and elsewhere the longest message of [`Profile::lora`], at most
/// 2,048 bytes and a quarter of the hour's air of the band.
#[must_use]
pub const fn longest_response(modulation: Modulation, band: Band) -> u32 {
    let sf = modulation.spreading_factor();
    let note = Profile::lora(sf, band.duty_permille()).unsched_bytes;
    match band {
        Band::P if sf <= 9 => LONGEST_RAISED,
        _ if note < LONGEST_NOTE => note,
        _ => LONGEST_NOTE,
    }
}

/// Returns what one end of a link of `modulation` in `band` accepts: the
/// longest packet of the band, the longest response as the longest message,
/// sent whole with no grant, and `max_rpcs` RPCs, 1 or 2 where
/// subscriptions are allowed.
#[must_use]
pub const fn limits(modulation: Modulation, band: Band, max_rpcs: u8) -> Limits {
    let longest = longest_response(modulation, band);
    Limits {
        max_packet: band.max_packet(modulation.spreading_factor()),
        max_message: longest,
        unsched_bytes: longest,
        max_rpcs,
    }
}

/// The unanswered RESEND and `NEED_ACK` packets after which a peer of a
/// `LoRa` link is dead.
///
/// The design note takes 5, the first version's floor, as a value for the model
/// of `29_carrier_lora` to measure, and the carrier takes 10, since collisions
/// and a busy channel add to the loss of a shared band. At 10 percent loss an
/// exchange of a RESEND and its answer fails with the likelihood 1 - 0.9^2,
/// 0.19, so five failed exchanges in a row end about one recovery in 4,000 and
/// ten about one in 16 million, before collisions and a busy channel raise the
/// loss.
pub const TIMEOUT_RESENDS: u8 = 10;

/// The bytes of a BUSY of the compact form, the answer a device gives a
/// RESEND while its answer waits.
const BUSY_LEN: usize = 6;

/// Returns the values of a link of `modulation` in `band`: [`Profile::lora`]
/// with the longest response of [`longest_response`] unscheduled and as its
/// window, so no grant travels, and [`TIMEOUT_RESENDS`] unanswered RESENDs
/// before a peer is dead.
///
/// A peer is dead once it left them unanswered and was silent for the longer of
/// their RESEND intervals and the time the band needs to earn the air of those
/// RESENDs and of the BUSY each draws, at its duty cycle. A ledger that
/// recovery has filled holds every packet of a live link until air ages out of
/// its hour, so a silence of the RESEND intervals alone can end a live link
/// whose packets wait in its ledger.
#[must_use]
pub const fn profile(modulation: Modulation, band: Band) -> Profile {
    let longest = longest_response(modulation, band);
    let mut profile = Profile::lora(modulation.spreading_factor(), band.duty_permille());
    profile.unsched_bytes = longest;
    profile.window = Window::Fixed(longest);
    profile.timers.timeout_resends = TIMEOUT_RESENDS;
    let intervals = profile
        .timers
        .resend_after
        .saturating_mul(TIMEOUT_RESENDS as Nanos);
    #[expect(clippy::cast_possible_truncation)]
    let exchange = time_on_air(modulation, (RESEND_LEN + OVERHEAD) as u8)
        + time_on_air(modulation, (BUSY_LEN + OVERHEAD) as u8);
    let earn = exchange
        .saturating_mul(TIMEOUT_RESENDS as Nanos)
        .saturating_mul(1_000)
        / band.duty_permille() as Nanos;
    profile.timers.rpc_timeout = if earn > intervals { earn } else { intervals };
    profile
}

/// Why the carrier failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoraError<R, S> {
    /// The radio failed.
    Radio(R),
    /// The storage of the counters failed.
    Store(S),
    /// A frame could not be sealed.
    Frame(FrameError),
}

impl<R: core::fmt::Display, S: core::fmt::Display> core::fmt::Display for LoraError<R, S> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Radio(error) => write!(f, "the radio failed: {error}"),
            Self::Store(error) => write!(f, "the storage of the counters failed: {error}"),
            Self::Frame(error) => write!(f, "the frame failed: {error}"),
        }
    }
}

/// What a carrier counted since it was built.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LoraStats {
    /// Frames transmitted.
    pub sent: u64,
    /// The air of those frames.
    pub air: Nanos,
    /// Packets handed to Homa.
    pub received: u64,
    /// Frames of another link, dropped before their tag.
    pub dropped_link: u64,
    /// Frames whose tag did not verify.
    pub dropped_tag: u64,
    /// Frames whose counter the window refused.
    pub dropped_replay: u64,
    /// Frames too short or too long for a frame.
    pub dropped_length: u64,
    /// Frames with the spent counter.
    pub dropped_counter: u64,
    /// Channel activity detections that found the channel busy.
    pub busy: u64,
    /// Packets the ledger held.
    pub held: u64,
    /// Listening gaps after a burst.
    pub gaps: u64,
    /// LIMITS that named the pairing's own limits and stayed off the air.
    pub limits_kept: u64,
}

/// The carrier `LoRa`: the radio, the frames, the counters and the ledger.
pub struct Lora<'b, R: Radio, S: CounterStore> {
    /// The radio.
    radio: R,
    /// The pairing of the link.
    pairing: Pairing,
    /// The direction this end sends in.
    sends: Direction,
    /// The counters of the frames it sends.
    counters: Counters<S>,
    /// The window of the frames it receives.
    replay: Replay,
    /// The air it may take.
    ledger: Ledger<'b>,
    /// The time the driver last handed it.
    now: Nanos,
    /// When its radio has ended its last transmission.
    busy_until: Nanos,
    /// Until when it holds its packet, through a busy channel or a gap.
    hold_until: Nanos,
    /// The air of the burst it sends.
    burst: Nanos,
    /// When its last transmission ended.
    last_end: Nanos,
    /// The earliest time a packet `ready` refused for the ledger fits.
    wake: Cell<Nanos>,
    /// The packet it holds.
    pending: [u8; MAX_SEALED],
    /// The bytes of the packet it holds, 0 for none.
    pending_len: usize,
    /// Whether the packet it holds is a control packet.
    pending_control: bool,
    /// The state of the generator of the delays after a busy channel.
    rng: u32,
    /// The frame it sends or receives.
    frame: [u8; MAX_PAYLOAD],
    /// What it counted.
    stats: LoraStats,
}

impl<R: Radio, S: CounterStore> core::fmt::Debug for Lora<'_, R, S> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Lora")
            .field("pairing", &self.pairing)
            .field("sends", &self.sends)
            .field("now", &self.now)
            .field("pending_len", &self.pending_len)
            .field("stats", &self.stats)
            .finish_non_exhaustive()
    }
}

impl<'b, R: Radio, S: CounterStore> Lora<'b, R, S> {
    /// Builds the carrier of one end of the link `pairing` names, which
    /// sends in the direction `sends`, over `radio`, the counters of `store`
    /// and a ledger over `buckets`, and tunes the radio. `seed` starts the
    /// generator of the delays after a busy channel, from the randomness of
    /// the card.
    ///
    /// # Errors
    ///
    /// Returns the error of the radio or of the store, where the radio
    /// refuses its setting or the counters cannot be reserved.
    pub fn new(
        mut radio: R,
        pairing: Pairing,
        sends: Direction,
        store: S,
        buckets: &'b mut [u32; BUCKETS],
        seed: u32,
    ) -> Result<Self, LoraError<R::Error, S::Error>> {
        radio
            .configure(pairing.frequency_hz, pairing.modulation, SYNC_WORD)
            .map_err(LoraError::Radio)?;
        let counters = Counters::boot(store, pairing.link, sends).map_err(LoraError::Store)?;
        Ok(Self {
            radio,
            pairing,
            sends,
            counters,
            replay: Replay::new(),
            ledger: Ledger::new(buckets, pairing.band),
            now: 0,
            busy_until: 0,
            hold_until: 0,
            burst: 0,
            last_end: 0,
            wake: Cell::new(Nanos::MAX),
            pending: [0; MAX_SEALED],
            pending_len: 0,
            pending_control: false,
            rng: seed | 1,
            frame: [0; MAX_PAYLOAD],
            stats: LoraStats::default(),
        })
    }

    /// Restarts the carrier as a card that restarts does: the counters start
    /// after the last reserved block, the window of received counters starts
    /// empty, the held packet is lost and the radio is tuned again. The
    /// ledger keeps its buckets, which a card holds in memory that a restart
    /// keeps.
    ///
    /// # Errors
    ///
    /// Returns the error of the radio or of the store.
    pub fn reboot(&mut self) -> Result<(), LoraError<R::Error, S::Error>> {
        self.pending_len = 0;
        self.burst = 0;
        self.hold_until = self.now;
        self.replay = Replay::new();
        self.radio
            .configure(
                self.pairing.frequency_hz,
                self.pairing.modulation,
                SYNC_WORD,
            )
            .map_err(LoraError::Radio)?;
        self.counters.reboot().map_err(LoraError::Store)
    }

    /// Hands the carrier the time `now`, sends the held packet where the
    /// channel, the burst and the ledger let it, and returns when the
    /// carrier wants its next turn, `None` where nothing waits.
    ///
    /// # Errors
    ///
    /// Returns the error of the radio or of the store, and the held packet
    /// is lost then, which Homa recovers.
    pub fn advance(&mut self, now: Nanos) -> Result<Option<Nanos>, LoraError<R::Error, S::Error>> {
        self.now = self.now.max(now);
        if self.wake.get() <= self.now {
            self.wake.set(Nanos::MAX);
        }
        self.try_send()?;
        let mut next = self.wake.get();
        for time in [self.busy_until, self.hold_until] {
            if time > self.now {
                next = next.min(time);
            }
        }
        Ok((next != Nanos::MAX).then_some(next))
    }

    /// Returns the pairing of the link.
    #[must_use]
    pub const fn pairing(&self) -> &Pairing {
        &self.pairing
    }

    /// Returns the counter the next frame takes.
    #[must_use]
    pub const fn next_counter(&self) -> u32 {
        self.counters.peek()
    }

    /// Returns what the carrier counted.
    #[must_use]
    pub const fn stats(&self) -> LoraStats {
        self.stats
    }

    /// Returns the radio.
    pub fn radio(&mut self) -> &mut R {
        &mut self.radio
    }

    /// Returns the time on air of a frame of `len` bytes on the link.
    fn air(&self, len: usize) -> Nanos {
        time_on_air(
            self.pairing.modulation,
            u8::try_from(len).unwrap_or(u8::MAX),
        )
    }

    /// Returns the longest frame of the link.
    fn max_frame(&self) -> usize {
        self.pairing
            .band
            .max_frame(self.pairing.modulation.spreading_factor())
    }

    /// Returns whether `packet` is a LIMITS of the compact form that names
    /// this end's limits of the pairing.
    fn names_own_limits(&self, packet: &[u8]) -> bool {
        Compact::decode(packet).is_ok_and(|decoded| match decoded.packet {
            Packet::Limits(limits) => limits == self.pairing.local,
            _ => false,
        })
    }

    /// Returns the listening gap after a burst, twice a RESEND's time on
    /// air.
    fn gap(&self) -> Nanos {
        self.air(RESEND_LEN + OVERHEAD).saturating_mul(2)
    }

    /// Returns a delay of a random fraction of `air`, at least a
    /// thousandth of it, from a xorshift generator of 32 bits.
    fn backoff(&mut self, air: Nanos) -> Nanos {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        air.saturating_mul(Nanos::from(x % 1_000 + 1)) / 1_000
    }

    /// Transmits the held packet where the channel, the burst and the ledger
    /// let it, and drops it where the radio or the store fails.
    fn try_send(&mut self) -> Result<(), LoraError<R::Error, S::Error>> {
        let sent = self.transmit_held();
        if sent.is_err() {
            self.pending_len = 0;
        }
        sent
    }

    /// Transmits the held packet where the channel, the burst and the ledger
    /// let it.
    fn transmit_held(&mut self) -> Result<(), LoraError<R::Error, S::Error>> {
        let now = self.now;
        if self.pending_len == 0 || now < self.busy_until || now < self.hold_until {
            return Ok(());
        }
        let len = self.pending_len + OVERHEAD;
        let air = self.air(len);
        let start = now + TURNAROUND;
        if !self.ledger.fits(start, air, self.pending_control) {
            self.stats.held += 1;
            self.hold_until = self
                .ledger
                .next_fit(start, air)
                .saturating_sub(TURNAROUND)
                .max(now + 1);
            return Ok(());
        }
        if self.radio.channel_busy().map_err(LoraError::Radio)? {
            self.stats.busy += 1;
            self.hold_until = now + TURNAROUND + self.backoff(air);
            return Ok(());
        }
        let counter = self.counters.next().map_err(LoraError::Store)?;
        let sealed = seal(
            &self.pairing,
            self.sends,
            counter,
            &self.pending[..self.pending_len],
            &mut self.frame,
        );
        self.pending_len = 0;
        let sealed = sealed.map_err(LoraError::Frame)?;
        // The ledger admitted the packet above, so the charge fits.
        let _ = self.ledger.charge(start, air);
        self.radio
            .transmit(&self.frame[..sealed])
            .map_err(LoraError::Radio)?;
        self.stats.sent += 1;
        self.stats.air += air;
        let gap = self.gap();
        if now > self.last_end.saturating_add(gap) {
            self.burst = 0;
        }
        self.burst += air;
        self.busy_until = start + air;
        self.last_end = self.busy_until;
        let full = self.air(self.max_frame());
        if self.burst + full > BURST {
            self.stats.gaps += 1;
            self.burst = 0;
            self.hold_until = self.busy_until + gap;
        }
        Ok(())
    }
}

impl<R: Radio, S: CounterStore> Carrier for Lora<'_, R, S> {
    type Peer = u8;
    type Error = LoraError<R::Error, S::Error>;

    fn ready(&self, peer: u8, len: usize) -> bool {
        if peer != self.pairing.link
            || self.pending_len > 0
            || self.now < self.busy_until
            || self.now < self.hold_until
        {
            return false;
        }
        let frame = len + OVERHEAD;
        if len > MAX_SEALED || frame > self.max_frame() {
            return false;
        }
        let air = self.air(frame);
        if air > self.pairing.band.longest_transmission() {
            return false;
        }
        let start = self.now + TURNAROUND;
        if self.ledger.fits(start, air, len <= CONTROL_LEN) {
            return true;
        }
        let fits = self.ledger.next_fit(start, air).saturating_sub(TURNAROUND);
        self.wake.set(self.wake.get().min(fits.max(self.now + 1)));
        false
    }

    fn send(
        &mut self,
        peer: u8,
        _priority: Priority,
        header: &[u8],
        payload: &[u8],
    ) -> Result<(), Self::Error> {
        if peer != self.pairing.link {
            return Err(LoraError::Frame(FrameError::Link));
        }
        let len = header.len() + payload.len();
        if len > MAX_SEALED || len + OVERHEAD > self.max_frame() || self.pending_len > 0 {
            return Err(LoraError::Frame(FrameError::TooLong));
        }
        if payload.is_empty() && self.names_own_limits(header) {
            self.stats.limits_kept += 1;
            return Ok(());
        }
        self.pending[..header.len()].copy_from_slice(header);
        self.pending[header.len()..len].copy_from_slice(payload);
        self.pending_len = len;
        self.pending_control = header.first() != Some(&DATA);
        self.try_send()
    }

    fn recv(&mut self, buf: &mut [u8]) -> Result<Option<(u8, usize)>, Self::Error> {
        let receives = self.sends.other();
        loop {
            let Some(len) = self
                .radio
                .receive(&mut self.frame)
                .map_err(LoraError::Radio)?
            else {
                return Ok(None);
            };
            self.hold_until = self.hold_until.max(self.now + 2 * TURNAROUND);
            let opened = open(&self.pairing, receives, &self.frame[..len], buf).and_then(
                |(counter, packet)| {
                    self.replay.accept(counter)?;
                    Ok(packet.len())
                },
            );
            match opened {
                Ok(len) => {
                    self.stats.received += 1;
                    return Ok(Some((self.pairing.link, len)));
                }
                Err(FrameError::Link) => self.stats.dropped_link += 1,
                Err(FrameError::Tag) => self.stats.dropped_tag += 1,
                Err(FrameError::Replay) => self.stats.dropped_replay += 1,
                Err(FrameError::Counter) => self.stats.dropped_counter += 1,
                Err(FrameError::Short | FrameError::TooLong) => self.stats.dropped_length += 1,
            }
        }
    }
}
