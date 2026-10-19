# SPDX-License-Identifier: MIT
"""The Homa engine of the notebooks, the compact form in Python, written from its design.

The second engine of every Homa exchange of the track, sharing no code with the
crate `homa`: the notebooks drive the crate's engine against this one in both
directions, so a byte the Rust engine answers a Python request with and a byte
the Python engine answers a Rust request with are judged against the bytes each
side was asked to carry. It imports nothing outside the standard library and
the references of this package, so `tests/pyproject.toml` gains no package.

The rules are those of the Homa design note, first version, section 2.4, with
the additions of its section 1.5, the report of `RPC_UNKNOWN` that
`docs/graphtp.md` section 2.7 asks of Homa, and the values of a link of its
section 3.1:

- A message states its length in its first packet, a whole message of one
  packet carries a header of 6 bytes and a part of a longer one 14, and the
  first `unsched_bytes` of the receiver leave at once, the rest as the
  receiver grants them, a packet whose first byte is granted going out whole.
- The receiver grants at half a window, up to a window beyond the bytes it
  holds in order, and asks with RESEND for the first missing range after
  `resend_after` of silence, again every `resend_interval`.
- A sender answers a RESEND with the range again, raising its grant to the end
  of the range, and with BUSY where the range holds bytes it has not sent yet,
  or where it is a server whose RPC still runs. An end that knows no such RPC
  answers `RPC_UNKNOWN`.
- A client whose request met `RPC_UNKNOWN` before a byte of its answer sends
  the request again from offset 0, unless the request asked with
  `report_unknown` for the failure `RpcUnknown` in place of the restart.
- A client acknowledges every whole response with an ACK, a server that sent
  a whole response and has no ACK after `need_ack_after` asks with
  `NEED_ACK`, and a client answers a `NEED_ACK` of an RPC it no longer holds
  with ACK.
- Each end sends its LIMITS first and sets `NEED_LIMITS` on every packet until
  the peer's LIMITS arrive, a peer's LIMITS fail every request longer than its
  `max_message` with `TooLong`, and a packet that breaks this end's limits is
  refused and answered with LIMITS.
- A peer that left `timeout_resends` questions unanswered and was silent for
  `rpc_timeout` is dead: every client RPC fails `PeerTimeout` and every server
  RPC is dropped.

The full form of stage H4 runs the same engine with `form="full"` over a raw
socket of IP protocol 146, which needs `CAP_NET_RAW` and so runs as root in a
guest or in a namespace of its own, for the interop half of `29_homa_raw`
against HomaModule and the crate. It follows the Network profile of the design
note's second version, section 3.2, with the defaults of HomaModule at commit
50d65305: the ports of both ends in every packet, identifiers of 64 bits, a
DATA header of 56 bytes, no LIMITS, since the full form has none and each end
takes the other's limits from the profile, the scheduling all or nothing of
HomaModule since its commit acf40a4, a message above the receiver's unscheduled
bytes opening with START_MSG and sending no DATA before a GRANT, the cutoff
version of the receiver's CUTOFFS on every DATA, and a packet for a port this
end does not own dropped and counted.
"""

from __future__ import annotations

import collections
import errno
import os
import random
import select
import socket
import time

from . import aoframe, compact, full
from .cobs import CobsError, decode as cobs_decode
from .crc32c import crc32c

U32 = 0xFFFFFFFF
U64 = 0xFFFFFFFFFFFFFFFF
WHOLE_HEADER = 6
PART_HEADER = 14
FULL_DATA_HEADER = 56
MS = 0.001
IPPROTO_HOMA = 146
MAX_MESSAGE = 1_000_000


class Profile:
    """The values of a link, the durations in seconds."""

    def __init__(self, max_packet, unsched_bytes, window, resend_after, timeout_resends,
                 fifo_every=16, batch_grants=True):
        self.max_packet = max_packet
        self.unsched_bytes = unsched_bytes
        self.window = window
        self.resend_after = resend_after
        self.resend_interval = resend_after
        self.patient_after = max(5.0, resend_after)
        self.need_ack_after = 2 * resend_after
        self.rpc_timeout = resend_after * timeout_resends
        self.timeout_resends = timeout_resends
        self.fifo_every = fifo_every
        self.batch_grants = batch_grants

    def override(self, **values):
        """Return the profile with the named values replaced, durations in seconds."""
        for key, value in values.items():
            if not hasattr(self, key):
                raise KeyError("a profile has no value %r" % key)
            setattr(self, key, value)
        return self


def unix_socket(max_message=65536):
    """The Unix Socket: packets of 16,384 bytes, the whole message
    unscheduled, a RESEND after 1,000 ms and 5 before the peer is dead."""
    return Profile(16384, max_message, max_message, 1.0, 5)


def serial_line(baud=115200):
    """A serial line: packets of 256 bytes, 512 unscheduled, a window of
    1,024, a RESEND after 550 ms at 115,200 baud and 5 before the peer is dead."""
    return Profile(256, 512, 1024, max(0.55 * 115200 / baud, 0.05), 5)


def network(max_packet=1480):
    """Network, the profile of HomaModule's defaults at the pinned commit:
    packets of the path's MTU less the IP header, 1,480 bytes on IPv4 at an
    MTU of 1,500, 40,000 unscheduled bytes, a window of 200,000 bytes, which
    is the dynamic window of 400,000 bytes of `max_incoming` over one
    message and one more, a RESEND after 5 ms, 5 before the peer is dead
    after a silence of 100 ms, and a NEED_ACK after 2 ms."""
    profile = Profile(max_packet, 40_000, 200_000, 5 * MS, 5, fifo_every=0, batch_grants=False)
    return profile.override(rpc_timeout=100 * MS, need_ack_after=2 * MS, patient_after=5.0)


def network_limits(profile):
    """The limits each end of Network takes of the other, which the full form
    carries no LIMITS for: the profile's packet and unscheduled bytes, the
    longest message of HomaModule, 1,000,000 bytes, and no bound of RPCs that a
    notebook meets."""
    return limits(profile, max_message=MAX_MESSAGE, max_rpcs=1 << 16)


def limits(profile, max_message=65536, max_rpcs=16):
    """The LIMITS an end of `profile` announces."""
    return {"max_packet": profile.max_packet, "max_message": max_message,
            "unsched_bytes": profile.unsched_bytes, "max_rpcs": max_rpcs}


class Loss:
    """A hook that drops, duplicates or damages packets with a seeded draw.

    Each rate is per million packets, drawn per packet in each direction it
    names, `out`, `in` or both. A damaged packet has one bit flipped at a drawn
    offset, which a delimited AoFrame catches as a failed CRC and a socket
    delivers as it is.
    """

    def __init__(self, seed, drop_ppm=0, duplicate_ppm=0, corrupt_ppm=0, directions=("out", "in")):
        self.rng = random.Random(seed)
        self.drop_ppm = drop_ppm
        self.duplicate_ppm = duplicate_ppm
        self.corrupt_ppm = corrupt_ppm
        self.directions = directions
        self.counts = {"dropped": 0, "duplicated": 0, "corrupted": 0, "passed": 0}

    def __call__(self, direction, packet):
        if direction not in self.directions:
            return [packet]
        draw = self.rng.randrange(1_000_000)
        if draw < self.drop_ppm:
            self.counts["dropped"] += 1
            return []
        draw -= self.drop_ppm
        if draw < self.duplicate_ppm:
            self.counts["duplicated"] += 1
            return [packet, packet]
        draw -= self.duplicate_ppm
        if draw < self.corrupt_ppm and packet:
            self.counts["corrupted"] += 1
            damaged = bytearray(packet)
            bit = self.rng.randrange(len(damaged) * 8)
            damaged[bit // 8] ^= 1 << (bit % 8)
            return [bytes(damaged)]
        self.counts["passed"] += 1
        return [packet]


class _Transport:
    """What the three transports share: the hook, the queue of bytes the
    socket did not take and the counts of what crossed."""

    def __init__(self, fd_owner, hook):
        self.owner = fd_owner
        self.hook = hook
        self.pending = collections.deque()
        self.closed = False
        self.counts = {"packets_out": 0, "bytes_out": 0, "link_bytes_out": 0,
                       "packets_in": 0, "bytes_in": 0, "link_bytes_in": 0,
                       "truncated": 0, "dropped_frames": 0}

    def fileno(self):
        return self.owner.fileno()

    def send(self, packet):
        """Queue one packet, through the hook, and write what the socket takes."""
        packets = self.hook("out", packet) if self.hook else [packet]
        for one in packets:
            self.counts["packets_out"] += 1
            self.counts["bytes_out"] += len(one)
            self._queue(one)
        self.flush()

    def can_send(self):
        return not self.pending and not self.closed

    def received(self, packets):
        out = []
        for packet in packets:
            for one in (self.hook("in", packet) if self.hook else [packet]):
                self.counts["packets_in"] += 1
                self.counts["bytes_in"] += len(one)
                out.append(one)
        return out


class SeqPacketTransport(_Transport):
    """A `SOCK_SEQPACKET` socket, one packet a record, non blocking.

    A record longer than `max_packet` arrives with `MSG_TRUNC` in the flags
    of `recvmsg` and is dropped and counted in `truncated`.
    """

    def __init__(self, sock, max_packet, hook=None):
        super().__init__(sock, hook)
        self.sock = sock
        self.max_packet = max_packet
        sock.setblocking(False)

    def _queue(self, record):
        self.pending.append(record)
        self.counts["link_bytes_out"] += len(record)

    def flush(self):
        while self.pending and not self.closed:
            try:
                self.sock.send(self.pending[0], socket.MSG_NOSIGNAL)
            except BlockingIOError:
                return False
            except (BrokenPipeError, ConnectionResetError):
                self.closed = True
                return False
            self.pending.popleft()
        return True

    def send_raw(self, record):
        """Send one record as it stands, past the hook, blocking until it leaves."""
        self.sock.setblocking(True)
        try:
            self.sock.send(record, socket.MSG_NOSIGNAL)
        finally:
            self.sock.setblocking(False)

    def recv(self, limit=None):
        """Return the packets waiting, at most `limit` of them, and mark the
        transport closed at its end."""
        out = []
        while not self.closed and (limit is None or len(out) < limit):
            try:
                data, _, flags, _ = self.sock.recvmsg(self.max_packet)
            except BlockingIOError:
                break
            except ConnectionResetError:
                self.closed = True
                break
            if flags & socket.MSG_TRUNC:
                self.counts["truncated"] += 1
                continue
            if not data:
                self.closed = True
                break
            self.counts["link_bytes_in"] += len(data)
            out.append(data)
        return self.received(out)


class StreamTransport(_Transport):
    """A stream socket in prefixed AoFrame, non blocking.

    The decoder is the rule of `aoframe.Decoder` in the prefixed form, a
    length refused above `max_packet` before a byte of the packet is stored
    and the bytes of a refused packet skipped, applied to whole chunks of the
    stream so a run of megabytes costs no call a byte.
    """

    def __init__(self, sock, max_packet, hook=None):
        super().__init__(sock, hook)
        self.sock = sock
        self.max_packet = max_packet
        self.inbuf = bytearray()
        self.skip = 0
        self.outbuf = bytearray()
        sock.setblocking(False)

    def _queue(self, packet):
        frame = aoframe.encode(aoframe.PREFIXED, packet)
        self.counts["link_bytes_out"] += len(frame)
        self.outbuf += frame

    def can_send(self):
        return len(self.outbuf) < 4 * (self.max_packet + aoframe.PREFIX_LEN) and not self.closed

    def flush(self):
        while self.outbuf and not self.closed:
            try:
                sent = self.sock.send(self.outbuf, socket.MSG_NOSIGNAL)
            except BlockingIOError:
                return False
            except (BrokenPipeError, ConnectionResetError):
                self.closed = True
                return False
            del self.outbuf[:sent]
        return True

    def recv(self, limit=None):
        """Return the packets waiting, at most `limit` of them, reading no
        more than the frames of `limit` packets can hold, and mark the
        transport closed at its end."""
        chunks = []
        room = None if limit is None else max(0, limit * (self.max_packet + aoframe.PREFIX_LEN) - len(self.inbuf))
        while not self.closed and room != 0:
            try:
                data = self.sock.recv(1 << 20 if room is None else room)
            except BlockingIOError:
                break
            except ConnectionResetError:
                self.closed = True
                break
            if not data:
                self.closed = True
                break
            chunks.append(data)
            if room is not None:
                room -= len(data)
        for data in chunks:
            self.counts["link_bytes_in"] += len(data)
            self.inbuf += data
        out = []
        while limit is None or len(out) < limit:
            if self.skip:
                take = min(self.skip, len(self.inbuf))
                del self.inbuf[:take]
                self.skip -= take
                if self.skip:
                    break
            if len(self.inbuf) < aoframe.PREFIX_LEN:
                break
            length = int.from_bytes(self.inbuf[:aoframe.PREFIX_LEN], "big")
            if length > self.max_packet:
                del self.inbuf[:aoframe.PREFIX_LEN]
                self.skip = length
                self.counts["dropped_frames"] += 1
                continue
            if len(self.inbuf) < aoframe.PREFIX_LEN + length:
                break
            out.append(bytes(self.inbuf[aoframe.PREFIX_LEN:aoframe.PREFIX_LEN + length]))
            del self.inbuf[:aoframe.PREFIX_LEN + length]
        return self.received(out)


class PtyTransport(_Transport):
    """One end of a pty in delimited AoFrame, non blocking.

    A frame is the bytes between two zero bytes, decoded from COBS and
    checked by its CRC-32C as `aoframe.Decoder` does, and a frame that fails
    either or runs past the buffer is dropped and counted.
    """

    def __init__(self, fd, max_packet, hook=None):
        super().__init__(_Fd(fd), hook)
        self.fd = fd
        self.max_packet = max_packet
        self.capacity = aoframe.buffer_len(aoframe.DELIMITED, max_packet)
        self.inbuf = bytearray()
        self.outbuf = bytearray()
        os.set_blocking(fd, False)

    def _queue(self, packet):
        frame = aoframe.encode(aoframe.DELIMITED, packet)
        self.counts["link_bytes_out"] += len(frame)
        self.outbuf += frame

    def can_send(self):
        return len(self.outbuf) < 4 * aoframe.frame_len(aoframe.DELIMITED, self.max_packet)

    def flush(self):
        while self.outbuf and not self.closed:
            try:
                written = os.write(self.fd, self.outbuf)
            except BlockingIOError:
                return False
            except OSError:
                self.closed = True
                return False
            del self.outbuf[:written]
        return True

    def recv(self, limit=None):
        """Return the packets waiting, at most `limit` of them, reading no
        more than the frames of `limit` packets can hold, and mark the
        transport closed at its end."""
        room = None if limit is None else max(0, limit * aoframe.frame_len(aoframe.DELIMITED, self.max_packet)
                                              - len(self.inbuf))
        while not self.closed and room != 0:
            try:
                data = os.read(self.fd, 1 << 16 if room is None else min(room, 1 << 16))
            except BlockingIOError:
                break
            except OSError as error:
                if error.errno == errno.EIO:
                    self.closed = True
                    break
                raise
            if not data:
                self.closed = True
                break
            self.counts["link_bytes_in"] += len(data)
            self.inbuf += data
            if room is not None:
                room -= len(data)
        out = []
        while limit is None or len(out) < limit:
            end = self.inbuf.find(0)
            if end < 0:
                if len(self.inbuf) > self.capacity:
                    self.inbuf.clear()
                    self.counts["dropped_frames"] += 1
                break
            frame = bytes(self.inbuf[:end])
            del self.inbuf[:end + 1]
            if not frame:
                continue
            if len(frame) > self.capacity:
                self.counts["dropped_frames"] += 1
                continue
            try:
                decoded = cobs_decode(frame)
            except CobsError:
                self.counts["dropped_frames"] += 1
                continue
            packet, sent = decoded[:-4], decoded[-4:]
            if len(decoded) < 4 or crc32c(packet) != int.from_bytes(sent, "little"):
                self.counts["dropped_frames"] += 1
                continue
            out.append(packet)
        return self.received(out)


class RawTransport(_Transport):
    """A raw socket of IP protocol 146 that carries the full form to one
    peer, non blocking.

    The socket receives every Homa packet of its network namespace, so the
    transport keeps those for `local_port` from the peer alone and counts the
    rest in `foreign`. A client names its server as `peer`, an address and a
    Homa port, and a server with no `peer` takes the first address and source
    port that reach its port. On IPv4 a receive holds the IP header, which the
    transport strips, and a send names the address with port 0, since the port
    of a raw send names the protocol on IPv6.
    """

    def __init__(self, local_port, peer=None, family=socket.AF_INET, bind=None, max_packet=1480, hook=None,
                 tos=None):
        sock = socket.socket(family, socket.SOCK_RAW, IPPROTO_HOMA)
        if bind is not None:
            sock.bind((bind, 0))
        if tos is not None and family == socket.AF_INET:
            sock.setsockopt(socket.IPPROTO_IP, socket.IP_TOS, tos)
        super().__init__(sock, hook)
        self.sock = sock
        self.family = family
        self.local_port = local_port
        self.peer = peer
        self.max_packet = max_packet
        self.counts["foreign"] = 0
        sock.setblocking(False)

    @property
    def peer_port(self):
        return self.peer[1] if self.peer else 0

    def _queue(self, packet):
        self.pending.append(packet)
        self.counts["link_bytes_out"] += len(packet)

    def flush(self):
        while self.pending and not self.closed:
            if self.peer is None:
                self.pending.clear()
                break
            try:
                self.sock.sendto(self.pending[0], (self.peer[0], 0))
            except BlockingIOError:
                return False
            self.pending.popleft()
        return True

    def recv(self, limit=None):
        out = []
        while limit is None or len(out) < limit:
            try:
                data, source = self.sock.recvfrom(1 << 16)
            except BlockingIOError:
                break
            if self.family == socket.AF_INET:
                header = (data[0] & 0x0F) * 4 if data else 0
                data = data[header:]
            if len(data) < 4:
                self.counts["foreign"] += 1
                continue
            sport = int.from_bytes(data[0:2], "big")
            dport = int.from_bytes(data[2:4], "big")
            address = source[0]
            if dport != self.local_port or (self.peer is not None and (address, sport) != tuple(self.peer)):
                self.counts["foreign"] += 1
                continue
            if self.peer is None:
                self.peer = (address, sport)
            self.counts["link_bytes_in"] += len(data)
            out.append(data)
        return self.received(out)


class _Fd:
    """A plain descriptor as `select` takes it."""

    def __init__(self, fd):
        self.fd = fd

    def fileno(self):
        return self.fd


class Rpc:
    """One RPC of the engine, as this end names it."""

    def __init__(self, local, client, now):
        self.local = local
        self.client = client
        self.state = "out" if client else "in"
        self.stamp = now
        self.resent = False
        self.age = 0
        # The outgoing message: the request of a client, the answer of a server.
        self.out = b""
        self.out_next = 0
        self.out_max = 0
        self.out_granted = 0
        self.sent_all = False
        # The incoming message: the answer of a client, the request of a server.
        self.in_len = None
        self.in_buf = None
        self.ranges = []
        self.in_granted = 0
        # What the caller reads.
        self.patient = False
        self.report_unknown = False
        self.restarts = 0
        self.unknown_reported = 0
        self.why = None
        self.response = None
        self.request = None
        self.answer_seen = False
        # When the RPC ended, and its place among the RPCs of its engine
        # that ended, which orders two that ended in the same poll.
        self.ended_at = None
        self.ended = None

    def received(self):
        """Return the bytes received in order from offset 0."""
        if self.ranges and self.ranges[0][0] == 0:
            return self.ranges[0][1]
        return 0

    def high(self):
        return self.ranges[-1][1] if self.ranges else 0


def _add_range(ranges, start, end):
    """Merge the received range [start, end) into the sorted list `ranges`."""
    merged = []
    placed = False
    for a, b in ranges:
        if b < start:
            merged.append((a, b))
        elif end < a:
            if not placed:
                merged.append((start, end))
                placed = True
            merged.append((a, b))
        else:
            start, end = min(a, start), max(b, end)
    if not placed:
        merged.append((start, end))
    merged.sort()
    return merged


class Engine:
    """One Homa endpoint of one peer, both roles, over one transport.

    `request` queues a request and returns its `Rpc`, `poll` moves packets and
    timers, and the caller reads each client RPC's `state`, `ok`, `failed` or
    `refused`, with its `response` or its `why`. A server answers every whole
    request with `responder(request)` at once. `events` lists what the engine
    reported, as `(kind, local id, detail)`.
    """

    def __init__(self, transport, profile, local, assumed=None, first_id=0x30000002,
                 responder=None, clock=time.monotonic, form="compact"):
        if form not in ("compact", "full"):
            raise ValueError("no form is named %r" % form)
        self.t = transport
        self.p = profile
        self.form = form
        self.full = form == "full"
        self.mask = U64 if self.full else U32
        self.local = dict(local)
        self.remote = dict(assumed or local)
        self.known = self.full
        self.cutoff_version = 0
        self.clock = clock
        self.now = clock()
        self.next_id = first_id & self.mask
        self.rpcs = {}
        self.age = 0
        self.ended = 0
        self.responder = responder
        self.events = []
        self.acks = []
        self.control = collections.deque()
        self.limits_at = None
        self.peer_stamp = self.now
        self.tries = 0
        self.asked_at = None
        self.dead = False
        self.silent = False
        self.data_budget = None
        self.fifo_count = 0
        self.stats = collections.Counter()
        if not self.full:
            self._owe_limits(force=True)

    # The caller's side.

    def request(self, message, patient=False, report_unknown=False):
        """Queue a request and return its RPC, `refused` with `TooLong` or
        `Full` where the engine does not take it."""
        rpc = Rpc(self.next_id, True, self.now)
        self.next_id = (self.next_id + 2) & self.mask or 2
        rpc.patient = patient
        rpc.report_unknown = report_unknown
        rpc.out = bytes(message)
        if self.known and len(rpc.out) > self.remote["max_message"]:
            rpc.state, rpc.why = "refused", "TooLong"
            return rpc
        open_rpcs = sum(1 for r in self.rpcs.values() if r.client and r.state in ("out", "in"))
        if open_rpcs >= self.remote["max_rpcs"]:
            rpc.state, rpc.why = "refused", "Full"
            return rpc
        self.age += 1
        rpc.age = self.age
        self._start_out(rpc)
        self.rpcs[rpc.local] = rpc
        return rpc

    def open_rpcs(self):
        return sum(1 for r in self.rpcs.values() if r.client and r.state in ("out", "in"))

    def poll(self, timeout=0.0, max_packets=None):
        """Wait up to `timeout` for the socket or a timer, then take what
        arrived, at most `max_packets` packets, run the timers and send what
        the transport takes."""
        if self.silent:
            time.sleep(timeout)
            self.now = self.clock()
            return
        self.t.flush()
        due = self.next_timeout()
        wait = timeout if due is None else max(0.0, min(timeout, due - self.clock()))
        if wait > 0:
            writers = [self.t] if (self.t.pending or getattr(self.t, "outbuf", None)) else []
            select.select([self.t], writers, [], wait)
        self.now = self.clock()
        for packet in self.t.recv(max_packets):
            self.handle(packet)
        self.run_timers()
        self.transmit()

    def done(self, rpcs):
        return all(rpc.state not in ("out", "in") for rpc in rpcs)

    # Receiving.

    def handle(self, data):
        """Take one packet from the peer."""
        if self.full:
            decoded, error = full.decode(data)
            if error is not None:
                self.stats["refused"] += 1
                self.stats["malformed_" + error] += 1
                return
            if decoded["ports"]["dst"] != self.t.local_port:
                self.stats["wrong_port"] += 1
                return
        else:
            try:
                decoded = compact.decode(data)
            except compact.DecodeError as error:
                self.stats["refused"] += 1
                self.stats["malformed_" + error.name] += 1
                return
        self.stats["received"] += 1
        self.peer_stamp = self.now
        self.tries = 0
        self.asked_at = None
        if decoded["need_limits"]:
            self._owe_limits()
        packet = decoded["packet"]
        kind = packet["type"]
        if kind == "DATA":
            self._on_data(packet)
        elif kind == "GRANT":
            self._on_grant(packet)
        elif kind == "RESEND":
            self._on_resend(packet)
        elif kind == "RPC_UNKNOWN":
            self._on_unknown(packet["id"] ^ 1)
        elif kind == "BUSY":
            rpc = self.rpcs.get(packet["id"] ^ 1)
            if rpc is not None:
                rpc.stamp, rpc.resent = self.now, False
        elif kind == "NEED_ACK":
            local = packet["id"] ^ 1
            rpc = self.rpcs.get(local)
            if rpc is None or rpc.state not in ("out", "in"):
                self._send({"type": "ACK", "id": 0 if self.full else local,
                            "acks": [{"client_id": local, "server_port": self._server_port()}]})
        elif kind == "ACK":
            if packet["id"]:
                self._on_ack(packet["id"])
            for entry in packet["acks"]:
                self._on_ack(entry["client_id"])
        elif kind == "LIMITS":
            self._on_limits(packet)
        elif kind == "START_MSG":
            self._on_start(packet)
        elif kind == "CUTOFFS":
            self.cutoff_version = packet["version"]
            self.stats["cutoffs"] += 1

    def _server_port(self):
        """The server port an ACK entry names, the peer's on the full form."""
        return self.t.peer_port if self.full else 0

    def _on_start(self, packet):
        """A message above the unscheduled bytes opens with START_MSG, whose
        receiver grants its first window."""
        local = packet["id"] ^ 1
        rpc = self.rpcs.get(local)
        length = packet["msg_length"]
        if length > self.local["max_message"]:
            self.stats["refused"] += 1
            return
        if local & 1 == 0:
            if rpc is None:
                self._send({"type": "RPC_UNKNOWN", "id": local})
                return
            if rpc.state != "out":
                return
            rpc.state = "in"
            rpc.answer_seen = True
        else:
            if rpc is None:
                if self.responder is None:
                    self._send({"type": "RPC_UNKNOWN", "id": local})
                    return
                rpc = Rpc(local, False, self.now)
                self.age += 1
                rpc.age = self.age
                self.rpcs[local] = rpc
            elif rpc.state != "in" or rpc.in_len is not None:
                return
        rpc.in_len = length
        rpc.in_buf = bytearray(length)
        rpc.ranges = []
        rpc.in_granted = 0
        rpc.stamp, rpc.resent = self.now, False
        self._grant(rpc)

    def _grant(self, rpc):
        """Grant up to a window beyond the bytes received in order."""
        granted = min(rpc.in_len, rpc.received() + self.p.window)
        if granted > rpc.in_granted:
            rpc.in_granted = granted
            self._send({"type": "GRANT", "id": rpc.local, "offset": granted,
                        "priority": 0 if self.full else 7})
            self.stats["grants_sent"] += 1

    def _start_out(self, rpc):
        """Start an outgoing message: its unscheduled bytes leave at once, and
        on the full form a message above them opens with START_MSG and waits
        for its first GRANT."""
        rpc.out_next = rpc.out_max = 0
        rpc.sent_all = False
        unsched = self.remote["unsched_bytes"]
        if self.full and len(rpc.out) > unsched:
            rpc.out_granted = 0
            self._send({"type": "START_MSG", "id": rpc.local, "msg_length": len(rpc.out)})
            self.stats["start_msg_sent"] += 1
        else:
            rpc.out_granted = min(len(rpc.out), unsched)

    def _on_data(self, packet):
        if packet["ack"] is not None:
            self._on_ack(packet["ack"]["client_id"])
        local = packet["id"] ^ 1
        rpc = self.rpcs.get(local)
        if local & 1 == 0:
            if rpc is None:
                self._send({"type": "RPC_UNKNOWN", "id": local})
                return
            if rpc.state == "out":
                if not self._admit(packet):
                    return
                rpc.state = "in"
                rpc.answer_seen = True
                self._begin_incoming(rpc, packet["msg_length"])
            elif rpc.state != "in":
                return
        else:
            if rpc is None:
                if self.responder is None:
                    self._send({"type": "RPC_UNKNOWN", "id": local})
                    return
                if not self._admit(packet):
                    return
                servers = sum(1 for r in self.rpcs.values() if not r.client)
                if servers >= self.local["max_rpcs"]:
                    self._owe_limits()
                    self.stats["refused"] += 1
                    return
                rpc = Rpc(local, False, self.now)
                self.age += 1
                rpc.age = self.age
                self.rpcs[local] = rpc
                self._begin_incoming(rpc, packet["msg_length"])
            elif rpc.state != "in":
                return
        self._absorb(rpc, packet)

    def _admit(self, packet):
        """Check the first packet of a message against this end's limits."""
        length = packet["msg_length"]
        unsched = min(length, self.local["unsched_bytes"])
        if length > self.local["max_message"] or (packet["offset"] > 0 and packet["offset"] >= unsched):
            self._owe_limits()
            self.stats["refused"] += 1
            return False
        return True

    def _begin_incoming(self, rpc, length):
        rpc.in_len = length
        rpc.in_buf = bytearray(length)
        rpc.ranges = []
        rpc.in_granted = min(length, self.local["unsched_bytes"])

    def _absorb(self, rpc, packet):
        payload = bytes.fromhex(packet["payload"])
        offset = packet["offset"]
        end = offset + len(payload)
        if packet["msg_length"] != rpc.in_len or end > rpc.in_len:
            self.stats["refused"] += 1
            return
        if offset > 0 and offset >= rpc.in_granted:
            self._owe_limits()
            self.stats["refused"] += 1
            return
        rpc.stamp, rpc.resent = self.now, False
        if end > offset:
            rpc.in_buf[offset:end] = payload
            rpc.ranges = _add_range(rpc.ranges, offset, end)
        if rpc.high() > rpc.in_granted:
            rpc.in_granted = min(rpc.high(), rpc.in_len)
        if rpc.received() >= rpc.in_len:
            self._complete(rpc)
            return
        window = self.p.window
        if rpc.in_granted < rpc.in_len and rpc.in_granted - rpc.received() <= window // 2:
            rpc.in_granted = min(rpc.in_len, rpc.received() + window)
            self._send({"type": "GRANT", "id": rpc.local, "offset": rpc.in_granted, "priority": 7})
            self.stats["grants_sent"] += 1

    def _complete(self, rpc):
        message = bytes(rpc.in_buf)
        rpc.in_buf = None
        if rpc.client:
            rpc.state = "ok"
            rpc.response = message
            rpc.ended_at = self.now
            self.ended += 1
            rpc.ended = self.ended
            self.events.append(("response", rpc.local, len(message)))
            self.acks.append(rpc.local)
            del self.rpcs[rpc.local]
            return
        rpc.state = "exec"
        rpc.request = message
        self.events.append(("request", rpc.local, len(message)))
        answer = self.responder(message)
        rpc.state = "answered"
        rpc.out = bytes(answer)
        self._start_out(rpc)
        rpc.stamp = self.now

    def _on_grant(self, packet):
        local = packet["id"] ^ 1
        rpc = self.rpcs.get(local)
        if rpc is None:
            if local & 1:
                self._send({"type": "RPC_UNKNOWN", "id": local})
            return
        if (rpc.client and rpc.state == "out") or (not rpc.client and rpc.state == "answered"):
            rpc.stamp, rpc.resent = self.now, False
            rpc.out_granted = max(rpc.out_granted, min(packet["offset"], len(rpc.out)))

    def _on_resend(self, packet):
        local = packet["id"] ^ 1
        rpc = self.rpcs.get(local)
        if rpc is None:
            self._send({"type": "RPC_UNKNOWN", "id": local})
            return
        if not rpc.client and rpc.state in ("in", "exec"):
            rpc.stamp, rpc.resent = self.now, False
            self._send({"type": "BUSY", "id": local})
            return
        if (rpc.client and rpc.state == "out") or (not rpc.client and rpc.state == "answered"):
            offset, length = packet["offset"], packet["length"]
            end = rpc.out_max if length == U32 else offset + length
            end = min(end, len(rpc.out))
            if end > rpc.out_granted:
                rpc.out_granted = end
            unsent = end > rpc.out_next
            if offset < rpc.out_next:
                rpc.out_next = offset
            rpc.stamp, rpc.resent = self.now, False
            self.stats["resends_answered"] += 1
            if unsent or end <= offset:
                self._send({"type": "BUSY", "id": local})

    def _on_unknown(self, local):
        rpc = self.rpcs.get(local)
        if rpc is None:
            return
        if rpc.client and rpc.state == "out":
            if rpc.report_unknown:
                rpc.unknown_reported += 1
                self.stats["unknown_reported"] += 1
                self._end_client(rpc, "RpcUnknown")
                return
            if self.known and len(rpc.out) > self.remote["max_message"]:
                self._end_client(rpc, "TooLong")
                return
            self._start_out(rpc)
            rpc.stamp, rpc.resent = self.now, False
            rpc.restarts += 1
            self.stats["restarts"] += 1
        elif not rpc.client and rpc.state in ("in", "exec"):
            del self.rpcs[local]
            self.events.append(("dropped", local, None))
        elif not rpc.client and rpc.state == "answered":
            del self.rpcs[local]

    def _on_ack(self, client_id):
        local = client_id | 1
        rpc = self.rpcs.get(local)
        if rpc is not None and not rpc.client and rpc.state == "answered":
            del self.rpcs[local]
            self.stats["acked"] += 1

    def _on_limits(self, packet):
        self.remote = {key: packet[key] for key in ("max_packet", "max_message", "unsched_bytes", "max_rpcs")}
        self.known = True
        self.events.append(("limits", 0, dict(self.remote)))
        for rpc in list(self.rpcs.values()):
            outgoing = (rpc.client and rpc.state == "out") or (not rpc.client and rpc.state == "answered")
            if not outgoing:
                continue
            if len(rpc.out) > self.remote["max_message"]:
                if rpc.client:
                    self._end_client(rpc, "TooLong")
                else:
                    del self.rpcs[rpc.local]
                    self.events.append(("dropped", rpc.local, None))
                continue
            rpc.out_granted = max(rpc.out_granted, min(len(rpc.out), self.remote["unsched_bytes"]))

    def _end_client(self, rpc, why):
        rpc.state, rpc.why, rpc.ended_at = "failed", why, self.now
        self.ended += 1
        rpc.ended = self.ended
        self.rpcs.pop(rpc.local, None)
        self.events.append(("failed", rpc.local, why))

    # Sending.

    def _owe_limits(self, force=False):
        if self.full:
            return
        if not force and self.limits_at is not None and self.now < self.limits_at + self.p.resend_after:
            return
        self.limits_at = self.now
        self.control.append(dict(self.local, type="LIMITS"))
        self.stats["limits_sent"] += 1

    def _send(self, packet):
        self.control.append(packet)

    def _put(self, packet):
        if self.full:
            self.t.send(full.encode(packet, {"src": self.t.local_port, "dst": self.t.peer_port}))
        else:
            self.t.send(compact.encode(packet, need_limits=not self.known))
        self.stats["sent"] += 1

    def transmit(self):
        """Send the control packets first, then DATA by SRPT with the FIFO share,
        while the transport takes them."""
        while self.control:
            self._put(self.control.popleft())
        if self.acks:
            for start in range(0, len(self.acks), 5):
                chunk = self.acks[start:start + 5]
                self._put({"type": "ACK", "id": 0,
                           "acks": [{"client_id": c, "server_port": self._server_port()} for c in chunk]})
            self.acks = []
        while self.t.can_send():
            rpc = self._pick()
            if rpc is None:
                break
            if self.data_budget is not None:
                if self.data_budget <= 0:
                    break
                self.data_budget -= 1
            self._send_data(rpc)

    def _sendable(self, rpc):
        outgoing = (rpc.client and rpc.state == "out") or (not rpc.client and rpc.state == "answered")
        if not outgoing:
            return False
        if len(rpc.out) == 0:
            return not rpc.sent_all
        return rpc.out_next < min(len(rpc.out), rpc.out_granted)

    def _pick(self):
        ready = [rpc for rpc in self.rpcs.values() if self._sendable(rpc)]
        if not ready:
            return None
        every = self.p.fifo_every
        self.fifo_count += 1
        if every and self.fifo_count % every == 0:
            return min(ready, key=lambda r: r.age)
        return min(ready, key=lambda r: (len(r.out) - r.out_next, r.age))

    def _send_data(self, rpc):
        room = min(self.remote["max_packet"], self.p.max_packet)
        length = len(rpc.out)
        offset = rpc.out_next
        if self.full:
            payload = rpc.out[offset:offset + room - FULL_DATA_HEADER]
        elif offset == 0 and length + WHOLE_HEADER <= room:
            payload = rpc.out
        else:
            payload = rpc.out[offset:offset + room - PART_HEADER]
        end = offset + len(payload)
        packet = {"type": "DATA", "id": rpc.local, "msg_length": length, "offset": offset,
                  "retransmit": offset < rpc.out_max, "ack": None, "cutoff_version": self.cutoff_version,
                  "incast": False, "payload": payload.hex()}
        self._put(packet)
        rpc.out_next = end
        rpc.out_max = max(rpc.out_max, end)
        if end >= length:
            rpc.sent_all = True
        rpc.stamp, rpc.resent = self.now, False
        self.stats["data_sent"] += 1

    # Timers.

    def _due(self, rpc):
        p = self.p
        if rpc.client and rpc.state == "out":
            if self._sendable(rpc):
                return None
            if rpc.resent:
                wait = p.resend_interval
            elif rpc.patient and rpc.sent_all:
                wait = p.patient_after
            else:
                wait = p.resend_after
        elif rpc.state == "in":
            if rpc.received() >= min(rpc.in_granted, rpc.in_len):
                return None
            wait = p.resend_interval if rpc.resent else p.resend_after
        elif not rpc.client and rpc.state == "answered" and rpc.sent_all:
            wait = p.need_ack_after
        else:
            return None
        at = rpc.stamp + wait
        if self.tries >= p.timeout_resends:
            at = max(at, self.peer_stamp + p.rpc_timeout)
        elif self.asked_at is not None:
            at = max(at, self.asked_at + p.resend_interval)
        return at

    def next_timeout(self):
        times = [t for t in (self._due(rpc) for rpc in self.rpcs.values()) if t is not None]
        return min(times) if times else None

    def run_timers(self):
        for rpc in list(self.rpcs.values()):
            at = self._due(rpc)
            if at is None or at > self.now:
                continue
            if self.tries >= self.p.timeout_resends:
                self._peer_dead()
                return
            if rpc.state == "answered":
                self._send({"type": "NEED_ACK", "id": rpc.local})
            else:
                self._send({"type": "RESEND", "id": rpc.local, "offset": 0, "length": U32, "priority": 7}
                           if rpc.client and rpc.state == "out" else self._resend_of(rpc))
                self.stats["resends"] += 1
            rpc.stamp, rpc.resent = self.now, True
            self.asked_at = self.now
            self.tries += 1

    def _resend_of(self, rpc):
        received = rpc.received()
        if received < rpc.high():
            gap_end = next((a for a, _ in rpc.ranges if a > received), rpc.high())
            return {"type": "RESEND", "id": rpc.local, "offset": received, "length": gap_end - received, "priority": 7}
        expected = min(rpc.in_granted, rpc.in_len)
        return {"type": "RESEND", "id": rpc.local, "offset": received, "length": expected - received, "priority": 7}

    def _peer_dead(self):
        self.dead = True
        self.stats["peer_deaths"] += 1
        for rpc in list(self.rpcs.values()):
            if rpc.client:
                self._end_client(rpc, "PeerTimeout")
            else:
                del self.rpcs[rpc.local]
                if rpc.state in ("in", "exec"):
                    self.events.append(("dropped", rpc.local, None))
        self.events.append(("peer_gone", 0, "Timeout"))
        self.tries = 0

    def link_down(self):
        """End every RPC as the carrier's link went down."""
        for rpc in list(self.rpcs.values()):
            if rpc.client:
                self._end_client(rpc, "LinkDown")
            else:
                del self.rpcs[rpc.local]
                if rpc.state in ("in", "exec"):
                    self.events.append(("dropped", rpc.local, None))
        self.events.append(("peer_gone", 0, "LinkDown"))


def open_transport(form, target, max_packet, hook=None):
    """Connect to the listener at `target` in the form `seqpacket` or
    `stream`, or wrap a pty descriptor for `pty`, and return the transport."""
    if form == "seqpacket":
        sock = socket.socket(socket.AF_UNIX, socket.SOCK_SEQPACKET)
        sock.connect(str(target))
        return SeqPacketTransport(sock, max_packet, hook)
    if form == "stream":
        sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        sock.connect(str(target))
        return StreamTransport(sock, max_packet, hook)
    if form == "pty":
        return PtyTransport(target, max_packet, hook)
    raise ValueError("no transport is named %r" % form)


def wait_for(path, timeout=10.0):
    """Wait until a socket file exists at `path`."""
    deadline = time.monotonic() + timeout
    while not os.path.exists(path):
        if time.monotonic() > deadline:
            raise TimeoutError("no socket appeared at %s" % path)
        time.sleep(0.005)


def run_requests(engine, messages, window=4, timeout=60.0, patient=False, report_unknown=False,
                 on_step=None):
    """Send every message as a request with at most `window` open and return
    the RPCs once each has ended or `timeout` has passed."""
    rpcs = []
    pending = collections.deque(enumerate(messages))
    open_rpcs = []
    deadline = time.monotonic() + timeout
    while pending or open_rpcs:
        while pending and len(open_rpcs) < window:
            index, message = pending.popleft()
            rpc = engine.request(message, patient=patient, report_unknown=report_unknown)
            rpc.index = index
            rpcs.append(rpc)
            if rpc.state == "out":
                open_rpcs.append(rpc)
        engine.poll(0.002)
        if on_step is not None:
            on_step(engine)
        open_rpcs = [rpc for rpc in open_rpcs if rpc.state in ("out", "in")]
        if engine.t.closed and not open_rpcs and pending:
            break
        if time.monotonic() > deadline:
            break
    return rpcs


def echo(message):
    """The responder that answers a request with its own bytes."""
    return message


class Server:
    """A Python server of the engine at a path, one engine a connection.

    `serve_one` accepts one connection and answers its requests through the
    responder until the peer closes it or `timeout` passes, and returns the
    engine, whose `stats`, `events` and transport counts the notebook reads.
    """

    def __init__(self, form, path, profile, local, responder=echo, first_id=0x40000002, hook=None):
        self.form = form
        self.path = str(path)
        self.profile = profile
        self.local = local
        self.responder = responder
        self.first_id = first_id
        self.hook = hook
        kind = socket.SOCK_SEQPACKET if form == "seqpacket" else socket.SOCK_STREAM
        if os.path.exists(self.path):
            os.unlink(self.path)
        self.listener = socket.socket(socket.AF_UNIX, kind)
        self.listener.bind(self.path)
        self.listener.listen(8)

    def serve_one(self, timeout=60.0):
        self.listener.settimeout(timeout)
        sock, _ = self.listener.accept()
        if self.form == "seqpacket":
            transport = SeqPacketTransport(sock, self.profile.max_packet, self.hook)
        else:
            transport = StreamTransport(sock, self.profile.max_packet, self.hook)
        engine = Engine(transport, self.profile, self.local, first_id=self.first_id, responder=self.responder)
        deadline = time.monotonic() + timeout
        while not transport.closed and time.monotonic() < deadline:
            engine.poll(0.002)
        engine.link_down()
        sock.close()
        return engine

    def close(self):
        self.listener.close()
        if os.path.exists(self.path):
            os.unlink(self.path)


class BaselineClient:
    """AoFrame alone: whole messages each in one prefixed frame, first in
    first out, the baseline of ADR-0023, over a stream socket or over the
    records of a `SOCK_SEQPACKET` socket of at most `max_packet` bytes each.
    """

    def __init__(self, form, path, max_packet, max_message):
        kind = socket.SOCK_SEQPACKET if form == "seqpacket" else socket.SOCK_STREAM
        self.form = form
        self.sock = socket.socket(socket.AF_UNIX, kind)
        self.sock.connect(str(path))
        self.max_packet = max_packet
        self.max_message = max_message
        self.inbuf = bytearray()
        self.counts = {"link_bytes_out": 0, "link_bytes_in": 0}

    def send(self, message):
        frame = aoframe.encode(aoframe.PREFIXED, message)
        self.counts["link_bytes_out"] += len(frame)
        if self.form == "seqpacket":
            for start in range(0, len(frame), self.max_packet):
                self.sock.send(frame[start:start + self.max_packet])
        else:
            self.sock.sendall(frame)

    def receive(self, timeout=10.0):
        """Return the next whole message, waiting up to `timeout`."""
        deadline = time.monotonic() + timeout
        while True:
            if len(self.inbuf) >= aoframe.PREFIX_LEN:
                length = int.from_bytes(self.inbuf[:aoframe.PREFIX_LEN], "big")
                if len(self.inbuf) >= aoframe.PREFIX_LEN + length:
                    message = bytes(self.inbuf[aoframe.PREFIX_LEN:aoframe.PREFIX_LEN + length])
                    del self.inbuf[:aoframe.PREFIX_LEN + length]
                    return message
            left = deadline - time.monotonic()
            if left <= 0:
                raise TimeoutError("no whole message within %.1f s" % timeout)
            self.sock.settimeout(left)
            data = self.sock.recv(1 << 20)
            if not data:
                raise ConnectionError("the server closed the socket")
            self.counts["link_bytes_in"] += len(data)
            self.inbuf += data

    def close(self):
        self.sock.close()
