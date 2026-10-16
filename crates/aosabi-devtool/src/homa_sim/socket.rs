// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool homa_sim socket: the links seqpacket and stream of a
 * homa-sim scenario, both ends in this process over a real Unix Socket.
 *
 * No analogue in the reference. The links run the ends of a scenario over the
 * Unix Socket transports of homa::unix, the plan's section The devtool gives
 * them to stage H2, and the judging notebook is
 * tests/notebooks/21_homa_loopback.ipynb, whose real socket half runs them.
 */

//! The socket links of the `homa-sim` primitive.
//!
//! The link `seqpacket` puts end A and end B on the two sockets of
//! `homa::unix::pair`, whose every record is one Homa packet. The link
//! `stream` binds a `homa::unix::StreamListener` at the scenario's `path`,
//! connects a stream to it and runs the two streams in prefixed `AoFrame`
//! through `homa::stream::Framed`, then removes the socket file. Either link
//! takes the packet of the scenario's profile, and `send_buffer` sets the
//! send buffer of each socket where the scenario names one, so a message can
//! wait on a full socket while another overtakes it.
//!
//! The kernel moves a record or the bytes of a stream from one socket to the
//! other within the send that queues them, so a packet never waits between two
//! passes of a run, and the link names no due time of its own: the clock of
//! the run moves only to the deadlines of the ends and to the times the
//! scenario names, as on a simulated link.

use std::{
    io,
    os::unix::net::UnixStream,
    path::Path, //
};

use super::scenario::Net;
use homa::{
    Carrier,
    Nanos,
    Priority,
    Profile,
    sim::{
        Injected,
        Side, //
    },
    stream::{
        self,
        Decoder,
        Framed,
        Mode, //
    },
    unix::{
        self,
        SeqPacket,
        StreamListener, //
    }, //
};

/// One end of a real link of one peer, seen by its engine as the end on the
/// other side.
pub struct SideEnd<'a, C> {
    /// The carrier of this end.
    inner: &'a mut C,
    /// The other end, which every packet of this end comes from and goes to.
    other: Side,
}

impl<C: Carrier<Peer = ()>> Carrier for SideEnd<'_, C> {
    type Peer = Side;
    type Error = C::Error;

    fn ready(&self, _peer: Side, len: usize) -> bool {
        self.inner.ready((), len)
    }

    fn send(
        &mut self,
        _peer: Side,
        priority: Priority,
        header: &[u8],
        payload: &[u8],
    ) -> Result<(), C::Error> {
        self.inner.send((), priority, header, payload)
    }

    fn recv(&mut self, buf: &mut [u8]) -> Result<Option<(Side, usize)>, C::Error> {
        let other = self.other;
        self.inner
            .recv(buf)
            .map(|got| got.map(|((), len)| (other, len)))
    }
}

impl<'a, C> SideEnd<'a, C> {
    /// Returns the end whose carrier is `inner` and whose peer is `other`.
    pub fn new(inner: &'a mut C, other: Side) -> Self {
        Self { inner, other }
    }
}

/// Returns the end of `side` of a link whose ends are `a` and `b`.
fn end_of<'a, C>(a: &'a mut C, b: &'a mut C, side: Side) -> SideEnd<'a, C> {
    match side {
        Side::A => SideEnd::new(a, Side::B),
        Side::B => SideEnd::new(b, Side::A),
    }
}

/// Sets the send buffer of `socket` where `bytes` names one.
pub fn size_send_buffer<S: std::os::fd::AsFd>(socket: &S, bytes: Option<usize>) -> io::Result<()> {
    match bytes {
        Some(bytes) => unix::set_send_buffer(socket, bytes),
        None => Ok(()),
    }
}

/// The link `seqpacket`: the two sockets of `homa::unix::pair`.
pub struct SeqNet {
    /// End A.
    a: SeqPacket,
    /// End B.
    b: SeqPacket,
}

impl SeqNet {
    /// Opens the pair with the packet of `profile` and the send buffer the
    /// scenario names.
    pub fn open(profile: &Profile, send_buffer: Option<usize>) -> io::Result<Self> {
        let (a, b) = unix::pair(profile.max_packet)?;
        size_send_buffer(&a, send_buffer)?;
        size_send_buffer(&b, send_buffer)?;
        Ok(Self { a, b })
    }
}

impl Net for SeqNet {
    type End<'a> = SideEnd<'a, SeqPacket>;

    fn end(&mut self, side: Side) -> SideEnd<'_, SeqPacket> {
        end_of(&mut self.a, &mut self.b, side)
    }

    fn step(&mut self, _now: Nanos) -> Option<Nanos> {
        None
    }

    fn injected(&self) -> Option<Injected> {
        None
    }
}

/// The buffers of the two ends of the link `stream`: for each end one frame
/// received and two frames queued for the stream.
pub struct StreamBuffers {
    /// The receive and the transmit buffer of A, then of B.
    parts: [Vec<u8>; 4],
}

/// The link `stream`: two streams of one connection that a
/// `homa::unix::StreamListener` accepted, in prefixed `AoFrame`.
pub struct StreamNet<'b> {
    /// End A, the stream that connected.
    a: Framed<'b, UnixStream>,
    /// End B, the stream the listener accepted.
    b: Framed<'b, UnixStream>,
}

impl<'b> StreamNet<'b> {
    /// Returns the buffers the link needs for the packet of `profile`.
    pub fn buffers(profile: &Profile) -> StreamBuffers {
        let max_packet = usize::from(profile.max_packet);
        let rx = Decoder::buffer_len(Mode::Prefixed, max_packet);
        let tx = stream::frame_len(Mode::Prefixed, max_packet).saturating_mul(2);
        StreamBuffers {
            parts: [vec![0; rx], vec![0; tx], vec![0; rx], vec![0; tx]],
        }
    }

    /// Binds a listener at `path`, connects a stream to it, accepts the
    /// connection and removes the socket file, and frames both streams over
    /// `buffers`.
    pub fn open(
        path: &Path,
        send_buffer: Option<usize>,
        buffers: &'b mut StreamBuffers,
    ) -> io::Result<Self> {
        let listener = StreamListener::bind(path)?;
        let connected = UnixStream::connect(path)?;
        connected.set_nonblocking(true)?;
        let accepted = loop {
            if let Some(stream) = listener.accept()? {
                break stream;
            }
        };
        drop(listener);
        std::fs::remove_file(path)?;
        size_send_buffer(&connected, send_buffer)?;
        size_send_buffer(&accepted, send_buffer)?;
        let [rx_a, tx_a, rx_b, tx_b] = &mut buffers.parts;
        Ok(Self {
            a: Framed::new(connected, Mode::Prefixed, rx_a, tx_a),
            b: Framed::new(accepted, Mode::Prefixed, rx_b, tx_b),
        })
    }
}

impl<'b> Net for StreamNet<'b> {
    type End<'a>
        = SideEnd<'a, Framed<'b, UnixStream>>
    where
        Self: 'a;

    fn end(&mut self, side: Side) -> SideEnd<'_, Framed<'b, UnixStream>> {
        end_of(&mut self.a, &mut self.b, side)
    }

    fn step(&mut self, _now: Nanos) -> Option<Nanos> {
        None
    }

    fn injected(&self) -> Option<Injected> {
        None
    }
}
