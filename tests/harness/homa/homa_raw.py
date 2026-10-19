# SPDX-License-Identifier: MIT
"""A client of the local protocol of the helper `homa-raw`, written from its
specification alone.

The helper runs the client role of the Network profile over raw IP 146 once
per host and offers it to any local program over a Unix stream socket, in
frames of a big endian `u32` length that counts the op and the body, then a
`u8` op and the body:

    1 REQUEST  to the helper    family u8 (4 or 6), the address in 4 or 16
                                bytes, port u16, patient u8, report_unknown
                                u8, then the message to the end of the frame
    2 ID       from the helper  id u64, once per REQUEST, in its order
    3 RESPONSE from the helper  id u64, then the message
    4 FAILED   from the helper  id u64, why u8: 1 LinkDown, 2 PeerTimeout,
                                3 TooLong, 4 Aborted, 5 RpcUnknown, 255 a
                                failure of the helper itself
    5 ABORT    to the helper    id u64

The notebook `29_homa_raw` drives the helper through this module with no
capability of its own, which is the question the helper answers. It shares
no code with the crate and imports nothing outside the standard library.
"""

from __future__ import annotations

import collections
import ipaddress
import select
import socket
import struct
import time

REQUEST = 1
ID = 2
RESPONSE = 3
FAILED = 4
ABORT = 5

WHY = {1: "LinkDown", 2: "PeerTimeout", 3: "TooLong", 4: "Aborted", 5: "RpcUnknown", 255: "Helper"}
PREFIX = 4


class ProtocolError(RuntimeError):
    """The helper sent a frame the protocol does not have."""


def encode_request(address, port, message, patient=False, report_unknown=False):
    """Return the frame of a REQUEST to `address` and its Homa `port`."""
    ip = ipaddress.ip_address(address)
    family = 4 if ip.version == 4 else 6
    body = struct.pack(">B", family) + ip.packed + struct.pack(">HBB", port, int(bool(patient)),
                                                                int(bool(report_unknown))) + bytes(message)
    return struct.pack(">IB", 1 + len(body), REQUEST) + body


def encode_abort(rpc_id):
    """Return the frame of an ABORT of `rpc_id`."""
    return struct.pack(">IBQ", 9, ABORT, rpc_id)


def encode_frame(op, body=b""):
    """Return a frame of any op, for the hostile rows."""
    return struct.pack(">IB", 1 + len(body), op) + bytes(body)


def decode_frames(buffer):
    """Take every whole frame off the front of the bytearray `buffer` and
    return them as `(op, fields)`, `fields` a dict of the op's body."""
    frames = []
    while len(buffer) >= PREFIX:
        (length,) = struct.unpack_from(">I", buffer)
        if length == 0:
            raise ProtocolError("a frame of length 0")
        if len(buffer) < PREFIX + length:
            break
        op = buffer[PREFIX]
        body = bytes(buffer[PREFIX + 1:PREFIX + length])
        del buffer[:PREFIX + length]
        if op == ID and len(body) == 8:
            frames.append((ID, {"id": struct.unpack(">Q", body)[0]}))
        elif op == RESPONSE and len(body) >= 8:
            frames.append((RESPONSE, {"id": struct.unpack(">Q", body[:8])[0], "message": body[8:]}))
        elif op == FAILED and len(body) == 9:
            rpc_id, why = struct.unpack(">QB", body)
            frames.append((FAILED, {"id": rpc_id, "why": WHY.get(why, str(why)), "code": why}))
        else:
            raise ProtocolError("op %d with a body of %d bytes" % (op, len(body)))
    return frames


class Rpc:
    """One request as the client sees it: its id once the helper names it,
    then `state` `ok` with its `response` or `failed` with its `why`."""

    def __init__(self, index, message):
        self.index = index
        self.message = message
        self.id = None
        self.state = "sent"
        self.response = None
        self.why = None
        self.sent_at = time.monotonic()
        self.ended_at = None


class Client:
    """One connection to the helper, which is one Homa peer to every server.

    `request` sends a REQUEST and returns its `Rpc`, `poll` reads what the
    helper answered, and `run` sends many requests with a window and waits
    for their ends.
    """

    def __init__(self, path, timeout=10.0):
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.settimeout(timeout)
        self.sock.connect(str(path))
        self.sock.setblocking(False)
        self.inbuf = bytearray()
        self.waiting_id = collections.deque()
        self.by_id = {}
        self.refused = collections.deque()
        self.closed = False
        self.unmatched = []
        self.count = 0

    def send_raw(self, frame):
        """Send bytes as they stand, for the hostile rows."""
        self.sock.setblocking(True)
        try:
            self.sock.sendall(frame)
        finally:
            self.sock.setblocking(False)

    def request(self, address, port, message, patient=False, report_unknown=False):
        rpc = Rpc(self.count, bytes(message))
        self.count += 1
        self.send_raw(encode_request(address, port, message, patient, report_unknown))
        self.waiting_id.append(rpc)
        return rpc

    def abort(self, rpc):
        self.send_raw(encode_abort(rpc.id))

    def poll(self, timeout=0.01):
        """Read what arrived within `timeout` and apply it to the RPCs."""
        readable, _, _ = select.select([self.sock], [], [], timeout)
        if not readable:
            return
        while True:
            try:
                data = self.sock.recv(1 << 20)
            except BlockingIOError:
                break
            except ConnectionResetError:
                data = b""
            if not data:
                self.closed = True
                break
            self.inbuf += data
        for op, fields in decode_frames(self.inbuf):
            self._apply(op, fields)

    def _apply(self, op, fields):
        now = time.monotonic()
        if op == ID:
            if not self.waiting_id:
                self.unmatched.append((op, fields))
                return
            rpc = self.waiting_id.popleft()
            rpc.id = fields["id"]
            if rpc.id:
                self.by_id[rpc.id] = rpc
            else:
                self.refused.append(rpc)
            return
        rpc_id = fields["id"]
        if rpc_id == 0:
            rpc = self.refused.popleft() if self.refused else None
        else:
            rpc = self.by_id.pop(rpc_id, None)
        if rpc is None:
            self.unmatched.append((op, fields))
            return
        rpc.ended_at = now
        if op == RESPONSE:
            rpc.state, rpc.response = "ok", fields["message"]
        else:
            rpc.state, rpc.why = "failed", fields["why"]

    def run(self, address, port, messages, window=8, timeout=60.0, patient=False, report_unknown=False):
        """Send every message with at most `window` open, and return the RPCs
        once each has ended or `timeout` has passed."""
        rpcs = []
        pending = collections.deque(messages)
        deadline = time.monotonic() + timeout
        while (pending or any(rpc.state == "sent" for rpc in rpcs)) and time.monotonic() < deadline:
            while pending and sum(1 for rpc in rpcs if rpc.state == "sent") < window:
                rpcs.append(self.request(address, port, pending.popleft(), patient, report_unknown))
            self.poll(0.005)
            if self.closed:
                break
        return rpcs

    def close(self):
        self.sock.close()
