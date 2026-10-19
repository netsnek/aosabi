// SPDX-License-Identifier: MIT
/*
 * wasm-client-probe: the client role of the crate homa as a page links it,
 * measured as a module for wasm32-unknown-unknown.
 *
 * The features are those graphtp-wasm links, the role client, alloc, the
 * compact form and both forms of AoFrame. The probe holds the client role to
 * at most 32 KiB of wasm with 0 imports, which
 * tests/notebooks/23_homa_budget.ipynb reads from the module bytes.
 */

//! The wasm client probe of the crate `homa`.
//!
//! Two exports of plain integers reach every call of the client role a page
//! makes: [`homa_probe_send`] builds a client endpoint over owned storage,
//! queues a request and frames each packet in `AoFrame`, and
//! [`homa_probe_exchange`] runs a request and its response through the
//! endpoint, the response's packets framed, checked by a decoder and handed
//! to the endpoint, then read, released and acknowledged. Each export carries
//! its own `#[expect(unsafe_code)]`, because a symbol name is an unsafe
//! attribute in edition 2024, and the probe holds no unsafe block.
//!
//! The probe links the standard library, whose allocator serves the storage
//! of the feature `alloc` on `wasm32-unknown-unknown`, since a global
//! allocator of the probe's own would need `unsafe`.

// An `unsafe` bump allocator would be smaller.
use homa::{
    Config,
    Endpoint,
    Event,
    Limits,
    Owned,
    Profile,
    Read,
    RequestOptions,
    RpcId,
    stream::{
        self,
        Decoder,
        Mode, //
    },
    wire::{
        Compact,
        Data,
        Form,
        Packet,
        Ports, //
    }, //
};

/// The client endpoint of a page: one peer, the link it reaches a device
/// over, storage the endpoint owns, the compact form.
type Client = Endpoint<(), Owned<()>, Compact>;

/// The bytes of a packet on the link the probe models, USB serial, whose
/// profile is `Profile::usb_serial`.
const MAX_PACKET: u16 = 512;

/// The longest message either end of the probe accepts.
const MAX_MESSAGE: u32 = 65_536;

/// Returns the configuration of the page's client: one peer, four client
/// slots, no server slot, and sixteen pages of 4,096 bytes a slot.
fn config() -> Config {
    Config {
        peers: 1,
        client_slots: 4,
        server_slots: 0,
        page_len: 4_096,
        pages: 64,
        max_gaps: 2,
        size_buckets: 0,
        first_id: 2,
        hash_key: [0x5a; 16],
        port: 0,
        extra_ports: 0,
    }
}

/// Returns what each end accepts on the link.
fn limits() -> Limits {
    Limits {
        max_packet: MAX_PACKET,
        max_message: MAX_MESSAGE,
        unsched_bytes: MAX_MESSAGE,
        max_rpcs: 4,
    }
}

/// Returns the form of `AoFrame` the export's `mode` names, 0 delimited and
/// any other value prefixed.
const fn mode_of(mode: u32) -> Mode {
    if mode == 0 {
        Mode::Delimited
    } else {
        Mode::Prefixed
    }
}

/// Returns a client endpoint with its link up, or `None` where the storage
/// does not hold the configuration.
fn client() -> Option<Client> {
    let config = config();
    let mut endpoint = Client::new(config, Profile::usb_serial(), Owned::new(&config)).ok()?;
    endpoint.link_up((), limits(), limits()).ok()?;
    Some(endpoint)
}

/// Returns `len` bytes of a message, a pattern of the byte values in turn.
fn message(len: u32) -> Vec<u8> {
    (0..len).map(|n| n.to_le_bytes()[0]).collect()
}

/// Frames every packet the endpoint hands out at `now` in `mode` and returns
/// the frames.
fn drain(endpoint: &mut Client, now: u64, mode: Mode) -> Vec<u8> {
    let mut frames = Vec::new();
    let mut out = vec![0u8; stream::frame_len(mode, usize::from(MAX_PACKET))];
    while let Some(packet) = endpoint.poll_transmit(now, |(), _| true) {
        if let Ok(written) = stream::encode(mode, packet.header, packet.payload, &mut out) {
            frames.extend_from_slice(out.get(..written).unwrap_or(&[]));
        }
    }
    frames
}

/// Queues a request of `len` bytes, frames its packets in the form `mode`
/// names and returns the bytes of the frames, 0 where the request is refused.
fn send(len: u32, mode: u32) -> u32 {
    let Some(mut endpoint) = client() else {
        return 0;
    };
    let mode = mode_of(mode);
    if endpoint
        .request((), &message(len), RequestOptions::default())
        .is_err()
    {
        return 0;
    }
    let frames = drain(&mut endpoint, 0, mode);
    u32::try_from(frames.len()).unwrap_or(u32::MAX)
}

/// Frames the packets of a response of `len` bytes to the client RPC `id`,
/// as the device would send them, in `mode`.
fn response_frames(id: RpcId, len: u32, mode: Mode) -> Vec<u8> {
    let body = message(len);
    let mut frames = Vec::new();
    let mut header = [0u8; Compact::MAX_HEADER];
    let mut out = vec![0u8; stream::frame_len(mode, usize::from(MAX_PACKET))];
    let room = usize::from(MAX_PACKET) - Compact::MAX_HEADER;
    let mut offset = 0usize;
    loop {
        let end = body.len().min(offset + room);
        let payload = body.get(offset..end).unwrap_or(&[]);
        let data = Data {
            id: id.get() | 1,
            msg_length: len,
            offset: u32::try_from(offset).unwrap_or(u32::MAX),
            retransmit: false,
            ack: None,
            cutoff_version: 0,
            incast: false,
            payload,
        };
        let Ok(head) = Compact::header(&Packet::Data(data), Ports::default(), false, &mut header)
        else {
            break;
        };
        let head = header.get(..head).unwrap_or(&[]);
        if let Ok(written) = stream::encode(mode, head, payload, &mut out) {
            frames.extend_from_slice(out.get(..written).unwrap_or(&[]));
        }
        offset = end;
        if offset >= body.len() {
            break;
        }
    }
    frames
}

/// Runs a request of `len` bytes and a response of `len` bytes through the
/// client in the form `mode` names, and returns the bytes of the response it
/// read, 0 where the exchange fails.
fn exchange(len: u32, mode: u32) -> u32 {
    let Some(mut endpoint) = client() else {
        return 0;
    };
    let mode = mode_of(mode);
    let Ok(id) = endpoint.request((), &message(len), RequestOptions::default()) else {
        return 0;
    };
    let _ = drain(&mut endpoint, 0, mode);
    let frames = response_frames(id, len, mode);
    let mut buf = vec![0u8; Decoder::buffer_len(mode, usize::from(MAX_PACKET))];
    let mut decoder = Decoder::new(mode, &mut buf);
    for byte in frames {
        if let Some(Ok(packet)) = decoder.push(byte) {
            // A packet the endpoint refuses changes nothing, and the answer
            // below reads 0.
            let _ = endpoint.handle(1, (), packet);
        }
    }
    let mut read = 0;
    while let Some(event) = endpoint.poll_event() {
        if let Event::Response { id: done } = event {
            let mut out = vec![0u8; usize::try_from(len).unwrap_or(0)];
            read = endpoint
                .response(done)
                .map_or(0, |response| response.read_at(0, &mut out));
            // The acknowledgement leaves with the packets drained below.
            let _ = endpoint.release(done);
        }
    }
    if let Some(due) = endpoint.next_timeout() {
        let _ = endpoint.handle_timeout(due);
    }
    let _ = drain(&mut endpoint, 2, mode);
    u32::try_from(read).unwrap_or(u32::MAX)
}

/// Queues a request of `len` bytes and returns the bytes of its frames in
/// `AoFrame`, delimited for a `mode` of 0 and prefixed otherwise.
#[expect(
    unsafe_code,
    reason = "a wasm export needs its symbol name, the default of owner question 20 of the M5 plan"
)]
#[unsafe(no_mangle)]
pub extern "C" fn homa_probe_send(len: u32, mode: u32) -> u32 {
    send(len, mode)
}

/// Runs a request and a response of `len` bytes each through the client
/// role, framed as `mode` names, and returns the bytes of the response read.
#[expect(
    unsafe_code,
    reason = "a wasm export needs its symbol name, the default of owner question 20 of the M5 plan"
)]
#[unsafe(no_mangle)]
pub extern "C" fn homa_probe_exchange(len: u32, mode: u32) -> u32 {
    exchange(len, mode)
}
