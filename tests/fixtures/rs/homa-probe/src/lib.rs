// SPDX-License-Identifier: MIT
/*
 * homa-probe: the endpoint of the crate homa over the device configuration of
 * the ESP32-C3, measured as a static library for its RV32IMC core.
 *
 * The configuration is the one of the Homa design note, second version,
 * homa/design.md section 8.3, lines 2941 to 2991 in the design folder outside
 * this repository: four links, Serial Line, USB serial, Bluetooth and LoRa,
 * two server slots a link, no client slot, and a request page of 512 bytes
 * and an answer of 2,048 a slot (ASSUMED, that section, until
 * tests/notebooks/23_homa_budget.ipynb prints what the probe measures). The
 * probe measures these bytes before a card links an image.
 *
 * The feature client builds the image of a card that federates as well, the
 * role client beside the role server with one client slot a link, which
 * 23_homa_budget holds to the same bound as the server image. From stage H2
 * the probe answers through reply_reserve as well, a reservation of the card's
 * answer buffer of 8,192 bytes written in place by a producer of the bytes and
 * committed, as a listener that spares the buffer calls it. From stage H6 the
 * features pacer-budget, aead, lora and sx1262 build the image of the HT-CT62,
 * a Lora carrier of aosabi-carrier-lora on the fourth link, one part at a
 * time.
 */

//! The device probe of the crate `homa`.
//!
//! Every public function is non generic and instantiates the engine of a
//! device card: an [`Endpoint`] of the role `server` over the compact form,
//! whose peer is the index of a link, with `AoFrame` in both forms on the
//! byte streams below it. The static [`KEEP`] holds each function, and a
//! static marked `#[used]` stays in the object through the link time
//! optimization of the release profile, so the static library holds the code
//! a card that calls these functions links, and nothing the calls never reach.
//!
//! The feature `frame-only` leaves the endpoint out and keeps the functions of
//! `AoFrame` alone, so the difference of the two builds is what the engine
//! adds to a card that frames its packets anyway. The feature `client` adds
//! the role `client` of `homa` and the calls a card makes as a client, to
//! send a request on a link, read its response and release it, so its build
//! is the image of a card that federates.
//!
//! The functions are the calls a card's driver makes on every turn: build the
//! endpoint over its storage, bring a link up and down, hand it a packet, run
//! its timers, take the next packet and frame it for the link, take an event,
//! read a request and answer it whole or through a reservation written in
//! place, and push the bytes of a stream through a decoder.
//!
//! The features of the HT-CT62 each add one part to the image before them,
//! so the difference of two builds is the bytes of that part. `pacer-budget`
//! adds the budget pacer the airtime ledger runs over. `aead` adds the AEAD
//! of `chacha20poly1305` through the seal and the open of the carrier.
//! `lora` adds the carrier [`aosabi_carrier_lora::Lora`] over a radio of the
//! probe with its frame, its counters, its window, its ledger and the time on
//! air, and the turn of a card's driver over it. `sx1262` puts the driver of
//! the SX1262 over a HAL of the probe in the place of that radio, with a TCXO
//! over DIO3 and an RF switch over DIO2. The radio,
//! the HAL and the store of the probe read and write through
//! [`core::hint::black_box`], so the optimizer keeps every path of the code
//! that handles what they answer.

#![no_std]
#![forbid(unsafe_code)]

use core::panic::PanicInfo;

use homa::stream::{
    self,
    Decoder,
    FrameError,
    Mode, //
};
#[cfg(not(feature = "frame-only"))]
use homa::{
    Config,
    ConfigError,
    Endpoint,
    Event,
    Limits,
    LinkError,
    Nanos,
    NoRoom,
    Parts,
    Profile,
    Read,
    Refused,
    RpcId,
    SendError,
    wire::Compact, //
};
#[cfg(all(feature = "client", not(feature = "frame-only")))]
use homa::{
    RequestOptions,
    UnknownRpc, //
};

/// Loops forever where a panic would unwind, as a card without a console
/// does.
///
/// The engine checks every length from the wire and indexes nothing it did
/// not place itself, so no packet reaches a panic, and the handler exists
/// because a `no_std` library for a target without an operating system needs
/// one.
#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    loop {
        core::hint::spin_loop();
    }
}

/// The links of the card, which name the peers of its endpoint.
#[cfg(not(feature = "frame-only"))]
pub mod link {
    /// The UART with delimited `AoFrame`.
    pub const SERIAL_LINE: u8 = 0;
    /// The CDC-ACM function of USB with delimited `AoFrame`.
    pub const USB_SERIAL: u8 = 1;
    /// The Bluetooth link with prefixed `AoFrame` on its channel.
    pub const BLUETOOTH: u8 = 2;
    /// The `LoRa` link, which carries one packet per radio packet.
    pub const LORA: u8 = 3;
}

/// The endpoint of the card: a link index as its peer, storage the card
/// owns, the compact form.
#[cfg(not(feature = "frame-only"))]
pub type DeviceEndpoint<'a> = Endpoint<u8, Parts<'a, u8>, Compact>;

/// The client slots of the card, one a link where it federates and none
/// where it serves alone.
#[cfg(not(feature = "frame-only"))]
const CLIENT_SLOTS: u32 = if cfg!(feature = "client") { 4 } else { 0 };

/// Returns the configuration of the card: four links, eight server slots,
/// no client slot, and a pool of 40 pages of 512 bytes, one request page and
/// four answer pages a slot.
///
/// With the feature `client` the card holds a client slot a link and five
/// more pages each, so every slot keeps the pages of a server slot.
#[cfg(not(feature = "frame-only"))]
#[must_use]
pub fn config() -> Config {
    Config {
        peers: 4,
        client_slots: CLIENT_SLOTS,
        server_slots: 8,
        page_len: 512,
        pages: 40 + 5 * CLIENT_SLOTS,
        max_gaps: 2,
        size_buckets: 0,
        first_id: 2,
        hash_key: [0x5a; 16],
        port: 0,
        extra_ports: 0,
    }
}

/// Builds the endpoint of the card over `parts` with the values of `profile`.
///
/// # Errors
///
/// Returns what [`Endpoint::new`] returns for storage that does not hold the
/// configuration of [`config`].
#[cfg(not(feature = "frame-only"))]
pub fn endpoint(parts: Parts<'_, u8>, profile: Profile) -> Result<DeviceEndpoint<'_>, ConfigError> {
    Endpoint::new(config(), profile, parts)
}

/// Brings up `link` with the limits this end announces on it, taking the
/// same of the peer until its LIMITS arrive.
///
/// # Errors
///
/// Returns what [`Endpoint::link_up`] returns.
#[cfg(not(feature = "frame-only"))]
pub fn link_up(
    endpoint: &mut DeviceEndpoint<'_>,
    link: u8,
    local: Limits,
) -> Result<(), LinkError> {
    endpoint.link_up(link, local, local)
}

/// Ends `link`, dropping every RPC on it.
///
/// # Errors
///
/// Returns what [`Endpoint::link_down`] returns.
#[cfg(not(feature = "frame-only"))]
pub fn link_down(endpoint: &mut DeviceEndpoint<'_>, link: u8) -> Result<(), LinkError> {
    endpoint.link_down(link)
}

/// Hands the endpoint one packet that arrived on `link` at `now`.
///
/// # Errors
///
/// Returns why the packet was not taken.
#[cfg(not(feature = "frame-only"))]
pub fn handle(
    endpoint: &mut DeviceEndpoint<'_>,
    now: Nanos,
    link: u8,
    packet: &[u8],
) -> Result<(), Refused> {
    endpoint.handle(now, link, packet)
}

/// Runs the timers due at `now` and returns when they run next.
#[cfg(not(feature = "frame-only"))]
pub fn timers(endpoint: &mut DeviceEndpoint<'_>, now: Nanos) -> Option<Nanos> {
    // A clock that went backwards runs no timer, and the deadline below is
    // still the one the card waits for.
    let _ = endpoint.handle_timeout(now);
    endpoint.next_timeout()
}

/// Returns the form of `AoFrame` a link frames its packets in, `None` for a
/// link that keeps packet boundaries.
#[cfg(not(feature = "frame-only"))]
const fn mode_of(link: u8) -> Option<Mode> {
    match link {
        link::SERIAL_LINE | link::USB_SERIAL => Some(Mode::Delimited),
        link::BLUETOOTH => Some(Mode::Prefixed),
        _ => None,
    }
}

/// Takes the next packet for a link that `ready` accepts and writes it into
/// `out` as the link carries it, and returns the link and the bytes written.
///
/// A packet that finds no room in `out` is dropped, as a driver whose
/// transmit buffer is full drops it, and Homa's RESEND recovers it.
#[cfg(not(feature = "frame-only"))]
pub fn transmit(
    endpoint: &mut DeviceEndpoint<'_>,
    now: Nanos,
    ready: [bool; 4],
    out: &mut [u8],
) -> Option<(u8, usize)> {
    let packet = endpoint.poll_transmit(now, |link, _len| {
        ready.get(usize::from(link)).copied().unwrap_or(false)
    })?;
    let written = match mode_of(packet.peer) {
        Some(mode) => stream::encode(mode, packet.header, packet.payload, out),
        None => copy_packet(packet.header, packet.payload, out),
    };
    Some((packet.peer, written.unwrap_or(0)))
}

/// Copies a packet into `out` as a link that keeps packet boundaries carries
/// it.
#[cfg(not(feature = "frame-only"))]
fn copy_packet(header: &[u8], payload: &[u8], out: &mut [u8]) -> Result<usize, NoRoom> {
    let end = header.len().checked_add(payload.len()).ok_or(NoRoom)?;
    let target = out.get_mut(..end).ok_or(NoRoom)?;
    let (first, second) = target.split_at_mut(header.len());
    first.copy_from_slice(header);
    second.copy_from_slice(payload);
    Ok(end)
}

/// Returns the next event of the endpoint.
#[cfg(not(feature = "frame-only"))]
pub fn event(endpoint: &mut DeviceEndpoint<'_>) -> Option<Event<u8>> {
    endpoint.poll_event()
}

/// Copies the request of the server RPC `id` into `out` and returns its
/// length, 0 where no request is held.
#[cfg(not(feature = "frame-only"))]
pub fn request(endpoint: &DeviceEndpoint<'_>, id: RpcId, out: &mut [u8]) -> usize {
    endpoint
        .request_of(id)
        .map_or(0, |message| message.read_at(0, out))
}

/// Queues `answer` as the response of the server RPC `id`.
///
/// # Errors
///
/// Returns what [`Endpoint::reply`] returns.
#[cfg(not(feature = "frame-only"))]
pub fn reply(endpoint: &mut DeviceEndpoint<'_>, id: RpcId, answer: &[u8]) -> Result<(), SendError> {
    endpoint.reply(id, answer)
}

/// The card's answer buffer, the longest answer of the ESP32-C3 by the limits
/// of ADR-0026, which a reservation takes in the pages in its place
/// (DOCUMENTED, docs/graphtp-in-aosabi.md section 3).
#[cfg(not(feature = "frame-only"))]
pub const LONGEST_ANSWER: u32 = 8_192;

/// The bytes a producer of the answer hands the reservation at once, the
/// size of an output block of an encoder, which lives on the stack.
#[cfg(not(feature = "frame-only"))]
const CHUNK: usize = 64;

/// Answers the server RPC `id` with `len` bytes through a reservation of the
/// card's longest answer, written in place in pieces of [`CHUNK`] bytes by a
/// producer, here a counter, and committed with the length written, and
/// returns whether the answer was queued.
///
/// No buffer of the answer exists beside the pages of the reservation, which
/// is what [`Endpoint::reply_reserve`] spares a card (decision P31).
#[cfg(not(feature = "frame-only"))]
pub fn reply_reserved(endpoint: &mut DeviceEndpoint<'_>, id: RpcId, len: u32) -> bool {
    let Ok(mut writer) = endpoint.reply_reserve(id, LONGEST_ANSWER) else {
        return false;
    };
    let mut chunk = [0u8; CHUNK];
    let mut left = len.min(LONGEST_ANSWER);
    let mut next = 0u8;
    while left > 0 {
        let take = chunk.len().min(usize::try_from(left).unwrap_or(CHUNK));
        for byte in chunk.iter_mut().take(take) {
            *byte = next;
            next = next.wrapping_add(1);
        }
        if writer.write(chunk.get(..take).unwrap_or(&[])).is_err() {
            return false;
        }
        left -= u32::try_from(take).unwrap_or(left);
    }
    writer.commit().is_ok()
}

/// Queues `message` as a request to the peer at the end of `link`.
///
/// # Errors
///
/// Returns what [`Endpoint::request`] returns.
#[cfg(all(feature = "client", not(feature = "frame-only")))]
pub fn send(
    endpoint: &mut DeviceEndpoint<'_>,
    link: u8,
    message: &[u8],
    options: RequestOptions,
) -> Result<RpcId, SendError> {
    endpoint.request(link, message, options)
}

/// Copies the response of the client RPC `id` into `out` and returns its
/// length, 0 where no whole response is held.
#[cfg(all(feature = "client", not(feature = "frame-only")))]
pub fn response(endpoint: &DeviceEndpoint<'_>, id: RpcId, out: &mut [u8]) -> usize {
    endpoint
        .response(id)
        .map_or(0, |message| message.read_at(0, out))
}

/// Frees the client RPC `id` and queues its acknowledgement.
///
/// # Errors
///
/// Returns what [`Endpoint::release`] returns.
#[cfg(all(feature = "client", not(feature = "frame-only")))]
pub fn release(endpoint: &mut DeviceEndpoint<'_>, id: RpcId) -> Result<(), UnknownRpc> {
    endpoint.release(id)
}

/// Writes one frame of the packet made of `header` and `payload` into `out`.
///
/// # Errors
///
/// Returns what [`stream::encode`] returns.
pub fn frame(
    mode: Mode,
    header: &[u8],
    payload: &[u8],
    out: &mut [u8],
) -> Result<usize, homa::NoRoom> {
    stream::encode(mode, header, payload, out)
}

/// Pushes the bytes of `bytes` through `decoder` and returns the length of
/// the last checked packet, and the count of frames it dropped.
pub fn unframe(decoder: &mut Decoder<'_>, bytes: &[u8]) -> (usize, u32) {
    let mut last = 0;
    let mut dropped = 0u32;
    for &byte in bytes {
        match decoder.push(byte) {
            None => {}
            Some(Ok(packet)) => last = packet.len(),
            Some(Err(FrameError::TooLong | FrameError::Cobs | FrameError::Crc)) => {
                dropped = dropped.saturating_add(1);
            }
        }
    }
    (last, dropped)
}

/// Returns a decoder of `mode` over `buf`.
pub fn decoder(mode: Mode, buf: &mut [u8]) -> Decoder<'_> {
    Decoder::new(mode, buf)
}

/// The type of [`frame`].
pub type FrameFn = fn(Mode, &[u8], &[u8], &mut [u8]) -> Result<usize, homa::NoRoom>;

/// The type of [`transmit`].
#[cfg(not(feature = "frame-only"))]
pub type TransmitFn =
    fn(&mut DeviceEndpoint<'_>, Nanos, [bool; 4], &mut [u8]) -> Option<(u8, usize)>;

/// The type of [`send`].
#[cfg(all(feature = "client", not(feature = "frame-only")))]
pub type SendFn =
    fn(&mut DeviceEndpoint<'_>, u8, &[u8], RequestOptions) -> Result<RpcId, SendError>;

/// The functions of the probe that `AoFrame` alone takes.
pub struct FrameProbe {
    /// [`frame`].
    pub frame: FrameFn,
    /// [`unframe`].
    pub unframe: fn(&mut Decoder<'_>, &[u8]) -> (usize, u32),
    /// [`decoder`].
    pub decoder: for<'a> fn(Mode, &'a mut [u8]) -> Decoder<'a>,
}

/// The functions of the probe that the endpoint adds.
#[cfg(not(feature = "frame-only"))]
pub struct EndpointProbe {
    /// [`config`].
    pub config: fn() -> Config,
    /// [`endpoint`].
    pub endpoint: for<'a> fn(Parts<'a, u8>, Profile) -> Result<DeviceEndpoint<'a>, ConfigError>,
    /// [`link_up`].
    pub link_up: fn(&mut DeviceEndpoint<'_>, u8, Limits) -> Result<(), LinkError>,
    /// [`link_down`].
    pub link_down: fn(&mut DeviceEndpoint<'_>, u8) -> Result<(), LinkError>,
    /// [`handle`].
    pub handle: fn(&mut DeviceEndpoint<'_>, Nanos, u8, &[u8]) -> Result<(), Refused>,
    /// [`timers`].
    pub timers: fn(&mut DeviceEndpoint<'_>, Nanos) -> Option<Nanos>,
    /// [`transmit`].
    pub transmit: TransmitFn,
    /// [`event`].
    pub event: fn(&mut DeviceEndpoint<'_>) -> Option<Event<u8>>,
    /// [`request`].
    pub request: fn(&DeviceEndpoint<'_>, RpcId, &mut [u8]) -> usize,
    /// [`reply`].
    pub reply: fn(&mut DeviceEndpoint<'_>, RpcId, &[u8]) -> Result<(), SendError>,
    /// [`reply_reserved`].
    pub reply_reserved: fn(&mut DeviceEndpoint<'_>, RpcId, u32) -> bool,
}

/// Every function of `AoFrame` the probe measures.
#[used]
pub static KEEP_FRAME: FrameProbe = FrameProbe {
    frame,
    unframe,
    decoder,
};

/// Every function of the endpoint the probe measures.
#[cfg(not(feature = "frame-only"))]
#[used]
pub static KEEP: EndpointProbe = EndpointProbe {
    config,
    endpoint,
    link_up,
    link_down,
    handle,
    timers,
    transmit,
    event,
    request,
    reply,
    reply_reserved,
};

/// The functions of the probe that the role `client` adds.
#[cfg(all(feature = "client", not(feature = "frame-only")))]
pub struct ClientProbe {
    /// [`send`].
    pub send: SendFn,
    /// [`response`].
    pub response: fn(&DeviceEndpoint<'_>, RpcId, &mut [u8]) -> usize,
    /// [`release`].
    pub release: fn(&mut DeviceEndpoint<'_>, RpcId) -> Result<(), UnknownRpc>,
}

/// Every function of the role `client` the probe measures.
#[cfg(all(feature = "client", not(feature = "frame-only")))]
#[used]
pub static KEEP_CLIENT: ClientProbe = ClientProbe {
    send,
    response,
    release,
};

/// The budget of an hour of air of the band P in microseconds, the ledger's
/// limit of the HT-CT62 (ASSUMED, the Homa design note, second version,
/// section 6.3).
#[cfg(feature = "pacer-budget")]
const AIR_PER_HOUR_US: u32 = 360_000_000;

/// Asks the budget of the card's ledger whether `amount` of data fits at
/// `now`, charges it where it does, and returns when that amount fits next.
#[cfg(feature = "pacer-budget")]
pub fn budget(buckets: &mut [u32; 61], now: Nanos, amount: u32) -> Nanos {
    let mut budget = homa::pacer::Budget::new(buckets, 3_600_000_000_000, AIR_PER_HOUR_US, 100);
    if budget.fits(now, amount, false) {
        // The charge fits, since the budget said so.
        let _ = budget.charge(now, amount);
    }
    budget.next_fit(now, amount)
}

/// Seals `buffer` in place with the AEAD of the carrier and returns the tag,
/// `None` where the AEAD refuses the length.
#[cfg(feature = "aead")]
pub fn seal_in_place(
    key: &[u8; 32],
    nonce: &[u8; 12],
    aad: &[u8],
    buffer: &mut [u8],
) -> Option<[u8; 16]> {
    aosabi_carrier_lora::seal_detached(key, nonce, aad, buffer).ok()
}

/// Opens `buffer` in place with the AEAD of the carrier and returns whether
/// the tag verified.
#[cfg(feature = "aead")]
pub fn open_in_place(
    key: &[u8; 32],
    nonce: &[u8; 12],
    aad: &[u8],
    buffer: &mut [u8],
    tag: &[u8; 16],
) -> bool {
    aosabi_carrier_lora::open_detached(key, nonce, aad, buffer, tag).is_ok()
}

/// The type of [`seal_in_place`].
#[cfg(feature = "aead")]
pub type SealFn = fn(&[u8; 32], &[u8; 12], &[u8], &mut [u8]) -> Option<[u8; 16]>;

/// The type of [`open_in_place`].
#[cfg(feature = "aead")]
pub type OpenFn = fn(&[u8; 32], &[u8; 12], &[u8], &mut [u8], &[u8; 16]) -> bool;

/// The functions of the budget and the AEAD the probe measures.
#[cfg(feature = "pacer-budget")]
pub struct AirProbe {
    /// [`budget`].
    pub budget: fn(&mut [u32; 61], Nanos, u32) -> Nanos,
    /// [`seal_in_place`].
    #[cfg(feature = "aead")]
    pub seal: SealFn,
    /// [`open_in_place`].
    #[cfg(feature = "aead")]
    pub open: OpenFn,
}

/// Every function of the budget and the AEAD the probe measures.
#[cfg(feature = "pacer-budget")]
#[used]
pub static KEEP_AIR: AirProbe = AirProbe {
    budget,
    #[cfg(feature = "aead")]
    seal: seal_in_place,
    #[cfg(feature = "aead")]
    open: open_in_place,
};

/// The parts of the carrier of the HT-CT62 on their own: the frame, the
/// window, the counters, the ledger and the time on air.
#[cfg(feature = "lora-frame")]
pub mod lora_frame {
    use core::{
        convert::Infallible,
        hint::black_box, //
    };

    use aosabi_carrier_lora::{
        BUCKETS,
        Band,
        CounterStore,
        Counters,
        Direction,
        Ledger,
        Modulation,
        Pairing,
        Replay, //
    };
    use homa::{
        Limits,
        Nanos, //
    };

    /// The modulation of the card, SF7 at 125 kHz and the coding rate 4/5.
    pub const MODULATION: Modulation = Modulation::lorawan(7);

    /// The band of the card, 869.525 MHz in the band P.
    pub const BAND: Band = Band::P;

    /// The limits of both ends of the card's link, a constant of the card.
    pub const LIMITS: Limits = aosabi_carrier_lora::limits(MODULATION, BAND, 1);

    /// The storage of the card's counters, which the probe reads and writes
    /// through a box the optimizer cannot see into.
    #[derive(Debug)]
    pub struct ProbeStore;

    impl CounterStore for ProbeStore {
        type Error = Infallible;

        fn reserved(&mut self, link: u8, direction: Direction) -> Result<u32, Infallible> {
            Ok(black_box(u32::from(link) + u32::from(direction.byte())))
        }

        fn reserve(
            &mut self,
            link: u8,
            direction: Direction,
            up_to: u32,
        ) -> Result<(), Infallible> {
            black_box((link, direction, up_to));
            Ok(())
        }
    }

    /// Returns the pairing of the card's link with `key`.
    #[must_use]
    pub const fn pairing(key: [u8; 32]) -> Pairing {
        Pairing {
            link: crate::link::LORA,
            key,
            frequency_hz: 869_525_000,
            band: BAND,
            modulation: MODULATION,
            local: LIMITS,
            peer: LIMITS,
        }
    }

    /// Seals `packet` into `out` with the next counter of `counters` and
    /// returns the bytes of the frame, 0 where it does not seal.
    pub fn seal(
        pairing: &Pairing,
        counters: &mut Counters<ProbeStore>,
        packet: &[u8],
        out: &mut [u8],
    ) -> usize {
        let Ok(counter) = counters.next();
        aosabi_carrier_lora::seal(pairing, Direction::ToClient, counter, packet, out).unwrap_or(0)
    }

    /// Opens `frame` into `out` through `replay` and returns the bytes of its
    /// packet, 0 where it is refused.
    pub fn open(pairing: &Pairing, replay: &mut Replay, frame: &[u8], out: &mut [u8]) -> usize {
        match aosabi_carrier_lora::open(pairing, Direction::ToDevice, frame, out) {
            Ok((counter, packet)) if replay.accept(counter).is_ok() => packet.len(),
            _ => 0,
        }
    }

    /// Boots the counters of the card's link.
    #[must_use]
    pub fn counters(link: u8) -> Counters<ProbeStore> {
        let Ok(counters) = Counters::boot(ProbeStore, link, Direction::ToClient);
        counters
    }

    /// Asks the ledger over `buckets` whether a frame of `len` bytes fits at
    /// `now` as data, charges it where it does, and returns when it fits
    /// next.
    pub fn ledger(buckets: &mut [u32; BUCKETS], now: Nanos, len: u8) -> Nanos {
        let air = aosabi_carrier_lora::time_on_air(MODULATION, len);
        let mut ledger = Ledger::new(buckets, BAND);
        if ledger.fits(now, air, false) {
            // The charge fits, since the ledger said so.
            let _ = ledger.charge(now, air);
        }
        ledger.next_fit(now, air)
    }

    /// The type of [`seal`].
    pub type SealFn = fn(&Pairing, &mut Counters<ProbeStore>, &[u8], &mut [u8]) -> usize;

    /// The type of [`open`].
    pub type OpenFn = fn(&Pairing, &mut Replay, &[u8], &mut [u8]) -> usize;

    /// The functions of the parts the probe measures.
    pub struct FrameProbe {
        /// [`pairing`].
        pub pairing: fn([u8; 32]) -> Pairing,
        /// [`seal`].
        pub seal: SealFn,
        /// [`open`].
        pub open: OpenFn,
        /// [`counters`].
        pub counters: fn(u8) -> Counters<ProbeStore>,
        /// [`ledger`].
        pub ledger: fn(&mut [u32; BUCKETS], Nanos, u8) -> Nanos,
    }

    /// Every function of the parts the probe measures.
    #[used]
    pub static KEEP_LORA_FRAME: FrameProbe = FrameProbe {
        pairing,
        seal,
        open,
        counters,
        ledger,
    };
}

/// The carrier of the HT-CT62 on its fourth link and the turn of the card's
/// driver over it.
#[cfg(all(feature = "lora", not(feature = "frame-only")))]
pub mod lora {
    use core::{
        convert::Infallible,
        hint::black_box, //
    };

    use aosabi_carrier_lora::{
        BUCKETS,
        Direction,
        Lora,
        Pairing, //
    };
    use homa::{
        Carrier,
        Nanos,
        Priority,
        Profile, //
    };

    use crate::{
        DeviceEndpoint,
        link,
        lora_frame::{
            BAND,
            LIMITS,
            MODULATION,
            ProbeStore, //
        }, //
    };

    /// The profile of the card's link, a constant of the card.
    pub const PROFILE: Profile = aosabi_carrier_lora::profile(MODULATION, BAND);

    /// A radio of the probe, whose every answer the optimizer cannot see.
    #[cfg(not(feature = "sx1262"))]
    #[derive(Debug)]
    pub struct ProbeRadio;

    #[cfg(not(feature = "sx1262"))]
    impl aosabi_carrier_lora::Radio for ProbeRadio {
        type Error = Infallible;

        fn configure(
            &mut self,
            frequency_hz: u32,
            modulation: aosabi_carrier_lora::Modulation,
            sync_word: u16,
        ) -> Result<(), Infallible> {
            black_box((frequency_hz, modulation, sync_word));
            Ok(())
        }

        fn channel_busy(&mut self) -> Result<bool, Infallible> {
            Ok(black_box(false))
        }

        fn transmit(&mut self, payload: &[u8]) -> Result<(), Infallible> {
            black_box(payload);
            Ok(())
        }

        fn receive(&mut self, buf: &mut [u8]) -> Result<Option<usize>, Infallible> {
            let len = black_box(0usize).min(buf.len());
            if len == 0 {
                return Ok(None);
            }
            buf[..len].fill(black_box(0));
            Ok(Some(len))
        }
    }

    /// The SPI device, the pins and the delay of the probe's HAL.
    #[cfg(feature = "sx1262")]
    #[derive(Debug)]
    pub struct ProbeHal;

    #[cfg(feature = "sx1262")]
    impl embedded_hal::spi::ErrorType for ProbeHal {
        type Error = Infallible;
    }

    #[cfg(feature = "sx1262")]
    impl embedded_hal::spi::SpiDevice for ProbeHal {
        fn transaction(
            &mut self,
            operations: &mut [embedded_hal::spi::Operation<'_, u8>],
        ) -> Result<(), Infallible> {
            use embedded_hal::spi::Operation;
            for operation in operations {
                match operation {
                    Operation::Read(buf) | Operation::TransferInPlace(buf) => {
                        buf.fill(black_box(0));
                    }
                    Operation::Write(buf) => {
                        black_box(&*buf);
                    }
                    Operation::Transfer(read, write) => {
                        black_box(&*write);
                        read.fill(black_box(0));
                    }
                    Operation::DelayNs(ns) => {
                        black_box(*ns);
                    }
                }
            }
            Ok(())
        }
    }

    #[cfg(feature = "sx1262")]
    impl embedded_hal::digital::ErrorType for ProbeHal {
        type Error = Infallible;
    }

    #[cfg(feature = "sx1262")]
    impl embedded_hal::digital::InputPin for ProbeHal {
        fn is_high(&mut self) -> Result<bool, Infallible> {
            Ok(black_box(false))
        }

        fn is_low(&mut self) -> Result<bool, Infallible> {
            Ok(black_box(true))
        }
    }

    #[cfg(feature = "sx1262")]
    impl embedded_hal::digital::OutputPin for ProbeHal {
        fn set_low(&mut self) -> Result<(), Infallible> {
            black_box(0u8);
            Ok(())
        }

        fn set_high(&mut self) -> Result<(), Infallible> {
            black_box(1u8);
            Ok(())
        }
    }

    #[cfg(feature = "sx1262")]
    impl embedded_hal::delay::DelayNs for ProbeHal {
        fn delay_ns(&mut self, ns: u32) {
            black_box(ns);
        }
    }

    /// The radio of the card: the probe's radio, or the SX1262 over the
    /// probe's HAL.
    #[cfg(not(feature = "sx1262"))]
    pub type CardRadio = ProbeRadio;

    /// The radio of the card: the probe's radio, or the SX1262 over the
    /// probe's HAL.
    #[cfg(feature = "sx1262")]
    pub type CardRadio = aosabi_carrier_lora::Sx1262<ProbeHal, ProbeHal, ProbeHal, ProbeHal>;

    /// Returns the radio of the card. The SX1262 powers a TCXO over DIO3 and
    /// drives an RF switch over DIO2, so the driver's bytes hold every
    /// command its configuration sends, whichever of the two the HT-CT62
    /// wires.
    fn card_radio() -> CardRadio {
        #[cfg(not(feature = "sx1262"))]
        {
            ProbeRadio
        }
        #[cfg(feature = "sx1262")]
        {
            aosabi_carrier_lora::Sx1262::new(ProbeHal, ProbeHal, ProbeHal, ProbeHal)
                .with_tcxo(
                    black_box(aosabi_carrier_lora::TcxoVoltage::V1_8),
                    black_box(5_000),
                )
                .with_rf_switch()
        }
    }

    /// The carrier of the card.
    pub type CardLora<'b> = Lora<'b, CardRadio, ProbeStore>;

    /// Builds the carrier of the card over `buckets`, the device side of the
    /// link, which sends toward the client.
    pub fn carrier(
        pairing: Pairing,
        buckets: &mut [u32; BUCKETS],
        seed: u32,
    ) -> Option<CardLora<'_>> {
        Lora::new(
            card_radio(),
            pairing,
            Direction::ToClient,
            ProbeStore,
            buckets,
            seed,
        )
        .ok()
    }

    /// Runs one turn of the card's driver on the link: hands the carrier the
    /// time, hands the endpoint every packet the carrier opened, takes the
    /// next packet for the link through the same [`crate::transmit`] every
    /// link of the card takes its packets through, puts it on the air, and
    /// returns when the carrier wants its next turn.
    pub fn turn(
        endpoint: &mut DeviceEndpoint<'_>,
        lora: &mut CardLora<'_>,
        now: Nanos,
        buf: &mut [u8],
    ) -> Option<Nanos> {
        let wake = lora.advance(now).ok().flatten();
        while let Ok(Some((peer, len))) = lora.recv(buf) {
            // A refusal is counted by the endpoint and needs nothing more.
            let _ = crate::handle(endpoint, now, peer, buf.get(..len).unwrap_or(&[]));
        }
        let ready = lora.ready(link::LORA, usize::from(LIMITS.max_packet));
        if let Some((peer, len)) = crate::transmit(endpoint, now, [false, false, false, ready], buf)
        {
            // A failure of the carrier ends in a packet Homa recovers.
            let _ = lora.send(peer, Priority::HIGHEST, buf.get(..len).unwrap_or(&[]), &[]);
        }
        wake
    }

    /// Restarts the carrier as the card restarts, and returns whether it did.
    pub fn reboot(lora: &mut CardLora<'_>) -> bool {
        lora.reboot().is_ok()
    }

    /// The type of [`carrier`].
    pub type CarrierFn = for<'b> fn(Pairing, &'b mut [u32; BUCKETS], u32) -> Option<CardLora<'b>>;

    /// The type of [`turn`].
    pub type TurnFn =
        fn(&mut DeviceEndpoint<'_>, &mut CardLora<'_>, Nanos, &mut [u8]) -> Option<Nanos>;

    /// The functions of the carrier the probe measures.
    pub struct LoraProbe {
        /// [`carrier`].
        pub carrier: CarrierFn,
        /// [`turn`].
        pub turn: TurnFn,
        /// [`reboot`].
        pub reboot: fn(&mut CardLora<'_>) -> bool,
    }

    /// Every function of the carrier the probe measures.
    #[used]
    pub static KEEP_LORA: LoraProbe = LoraProbe {
        carrier,
        turn,
        reboot,
    };
}
