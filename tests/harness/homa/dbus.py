# SPDX-License-Identifier: MIT
"""A D-Bus client in the standard library of Python, for BlueZ's GATT client
side, the central of the emulated half of `28_carrier_bluetooth`.

The client speaks the D-Bus wire protocol of the specification over the Unix
socket of the system bus: the `EXTERNAL` authentication with the uid of the
process, the little endian marshalling of every type the BlueZ API uses,
method calls with their replies and errors, signals with match rules, and
Unix file descriptors passed beside a message, which BlueZ hands for
`AcquireWrite` and `AcquireNotify`.

A value is written as Python writes it: `y` `n` `q` `i` `u` `x` `t` and `h`
as int, `b` as bool, `d` as float, `s` `o` and `g` as str, `ay` as bytes, an
array as list, a dict of `a{..}` as dict, a struct as tuple, and a variant as
`Variant(signature, value)` when it is written and as its value when it is
read. `ObjectPath` and `Signature` mark a str that goes on the wire as `o` or
`g` inside a variant.
"""

from __future__ import annotations

import array
import collections
import os
import select
import socket
import struct
import time

SYSTEM_BUS = "/run/dbus/system_bus_socket"

METHOD_CALL = 1
METHOD_RETURN = 2
ERROR = 3
SIGNAL = 4

NO_REPLY_EXPECTED = 0x1

# The codes of the header fields.
PATH = 1
INTERFACE = 2
MEMBER = 3
ERROR_NAME = 4
REPLY_SERIAL = 5
DESTINATION = 6
SENDER = 7
SIGNATURE = 8
UNIX_FDS = 9

ALIGN = {"y": 1, "b": 4, "n": 2, "q": 2, "i": 4, "u": 4, "x": 8, "t": 8, "d": 8, "h": 4,
         "s": 4, "o": 4, "g": 1, "a": 4, "(": 8, "{": 8, "v": 1}
FIXED = {"y": "B", "b": "I", "n": "h", "q": "H", "i": "i", "u": "I", "x": "q", "t": "Q", "d": "d", "h": "I"}


class DBusError(RuntimeError):
    """A method call answered an error."""

    def __init__(self, name, message):
        super().__init__("%s: %s" % (name, message))
        self.name = name
        self.message = message


class Variant:
    """A value written as a variant of `signature`."""

    def __init__(self, signature, value):
        self.signature = signature
        self.value = value

    def __repr__(self):
        return "Variant(%r, %r)" % (self.signature, self.value)


class ObjectPath(str):
    """A str written as an object path."""


class Signature(str):
    """A str written as a signature."""


def split(signature):
    """Split a signature into its complete types."""
    out = []
    at = 0
    while at < len(signature):
        end = _type_end(signature, at)
        out.append(signature[at:end])
        at = end
    return out


def _type_end(signature, at):
    code = signature[at]
    if code == "a":
        return _type_end(signature, at + 1)
    if code in "({":
        close = ")" if code == "(" else "}"
        depth = 0
        for index in range(at, len(signature)):
            if signature[index] in "({":
                depth += 1
            elif signature[index] in ")}":
                depth -= 1
                if depth == 0:
                    if signature[index] != close:
                        raise ValueError("unbalanced signature %r" % signature)
                    return index + 1
        raise ValueError("unbalanced signature %r" % signature)
    return at + 1


def guess(value):
    """Return the signature of a Python value inside a variant."""
    if isinstance(value, Variant):
        return "v"
    if isinstance(value, bool):
        return "b"
    if isinstance(value, int):
        return "i" if -(1 << 31) <= value < (1 << 31) else "x"
    if isinstance(value, float):
        return "d"
    if isinstance(value, ObjectPath):
        return "o"
    if isinstance(value, Signature):
        return "g"
    if isinstance(value, str):
        return "s"
    if isinstance(value, (bytes, bytearray)):
        return "ay"
    if isinstance(value, dict):
        return "a{sv}"
    raise TypeError("no signature for %r, write it as a Variant" % (value,))


class Writer:
    """Marshals values in little endian, aligned from the start of the
    message."""

    def __init__(self, start=0):
        self.buf = bytearray()
        self.start = start

    def pad(self, alignment):
        while (self.start + len(self.buf)) % alignment:
            self.buf.append(0)

    def write(self, signature, value):
        code = signature[0]
        if code in FIXED:
            self.pad(ALIGN[code])
            self.buf += struct.pack("<" + FIXED[code], int(value) if code != "d" else value)
        elif code in "so":
            raw = value.encode()
            self.pad(4)
            self.buf += struct.pack("<I", len(raw)) + raw + b"\0"
        elif code == "g":
            raw = value.encode()
            self.buf += struct.pack("<B", len(raw)) + raw + b"\0"
        elif code == "v":
            inner = value.signature if isinstance(value, Variant) else guess(value)
            payload = value.value if isinstance(value, Variant) else value
            self.write("g", inner)
            self.write(inner, payload)
        elif code == "a":
            element = signature[1:]
            self.pad(4)
            at = len(self.buf)
            self.buf += b"\0\0\0\0"
            self.pad(ALIGN[element[0]])
            begin = len(self.buf)
            if element == "y":
                self.buf += bytes(value)
            elif element[0] == "{":
                key, item = split(element[1:-1])
                for k, v in value.items():
                    self.pad(8)
                    self.write(key, k)
                    self.write(item, v)
            else:
                for item in value:
                    self.write(element, item)
            struct.pack_into("<I", self.buf, at, len(self.buf) - begin)
        elif code == "(":
            self.pad(8)
            for part, item in zip(split(signature[1:-1]), value):
                self.write(part, item)
        else:
            raise ValueError("no type %r" % code)

    def write_all(self, signature, values):
        for part, value in zip(split(signature), values):
            self.write(part, value)
        return bytes(self.buf)


class Reader:
    """Unmarshals little endian values, aligned from the start of the
    message."""

    def __init__(self, data, at=0, start=0):
        self.data = data
        self.at = at
        self.start = start

    def pad(self, alignment):
        self.at += (-(self.at - self.start)) % alignment

    def read(self, signature):
        code = signature[0]
        if code in FIXED:
            self.pad(ALIGN[code])
            fmt = "<" + FIXED[code]
            (value,) = struct.unpack_from(fmt, self.data, self.at)
            self.at += struct.calcsize(fmt)
            return bool(value) if code == "b" else value
        if code in "so":
            self.pad(4)
            (length,) = struct.unpack_from("<I", self.data, self.at)
            value = bytes(self.data[self.at + 4:self.at + 4 + length]).decode()
            self.at += 4 + length + 1
            return value
        if code == "g":
            length = self.data[self.at]
            value = bytes(self.data[self.at + 1:self.at + 1 + length]).decode()
            self.at += 1 + length + 1
            return value
        if code == "v":
            return self.read(self.read("g"))
        if code == "a":
            element = signature[1:]
            self.pad(4)
            (length,) = struct.unpack_from("<I", self.data, self.at)
            self.at += 4
            self.pad(ALIGN[element[0]])
            end = self.at + length
            if element == "y":
                value = bytes(self.data[self.at:end])
                self.at = end
                return value
            if element[0] == "{":
                key, item = split(element[1:-1])
                out = {}
                while self.at < end:
                    self.pad(8)
                    k = self.read(key)
                    out[k] = self.read(item)
                return out
            out = []
            while self.at < end:
                out.append(self.read(element))
            return out
        if code == "(":
            self.pad(8)
            return tuple(self.read(part) for part in split(signature[1:-1]))
        raise ValueError("no type %r" % code)

    def read_all(self, signature):
        return [self.read(part) for part in split(signature)]


class Message:
    """One message as it arrived."""

    def __init__(self, kind, flags, serial, fields, body, fds):
        self.kind = kind
        self.flags = flags
        self.serial = serial
        self.fields = fields
        self.body = body
        self.fds = fds

    @property
    def path(self):
        return self.fields.get(PATH)

    @property
    def interface(self):
        return self.fields.get(INTERFACE)

    @property
    def member(self):
        return self.fields.get(MEMBER)

    @property
    def sender(self):
        return self.fields.get(SENDER)

    def __repr__(self):
        return "Message(%d, %s %s.%s, %r)" % (self.kind, self.path, self.interface, self.member, self.body)


class Bus:
    """One connection to a message bus.

    `call` sends a method call and returns the body of its reply, `add_match`
    subscribes to signals, which `signals` holds in arrival order, `export`
    answers the method calls to an object of this client, and `poll` reads
    what arrived within a timeout.
    """

    def __init__(self, path=None, timeout=10.0):
        address = os.environ.get("DBUS_SYSTEM_BUS_ADDRESS", "")
        if path is None and address.startswith("unix:path="):
            path = address.split("=", 1)[1].split(",")[0]
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.settimeout(timeout)
        self.sock.connect(path or SYSTEM_BUS)
        self.timeout = timeout
        self.serial = 0
        self.inbuf = bytearray()
        self.infds = []
        self.replies = {}
        self.signals = collections.deque()
        self.exported = {}
        self.last_fds = []
        self._authenticate()
        self.unique_name = self.call("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus",
                                     "Hello")[0]

    def fileno(self):
        return self.sock.fileno()

    def _line(self):
        while b"\r\n" not in self.inbuf:
            chunk = self.sock.recv(4096)
            if not chunk:
                raise ConnectionError("the bus closed the connection during authentication")
            self.inbuf += chunk
        line, _, rest = bytes(self.inbuf).partition(b"\r\n")
        self.inbuf = bytearray(rest)
        return line.decode()

    def _authenticate(self):
        uid = str(os.getuid()).encode().hex()
        self.sock.sendall(b"\0AUTH EXTERNAL " + uid.encode() + b"\r\n")
        answer = self._line()
        if not answer.startswith("OK "):
            raise ConnectionError("the bus refused EXTERNAL: %s" % answer)
        self.sock.sendall(b"NEGOTIATE_UNIX_FD\r\n")
        self.fd_passing = self._line() == "AGREE_UNIX_FD"
        self.sock.sendall(b"BEGIN\r\n")

    def send(self, kind, fields, signature="", body=(), flags=0, fds=()):
        """Send one message and return its serial."""
        self.serial += 1
        payload = Writer().write_all(signature, body) if signature else b""
        header_fields = [(code, Variant(sig, value)) for code, sig, value in fields]
        if signature:
            header_fields.append((SIGNATURE, Variant("g", signature)))
        if fds:
            header_fields.append((UNIX_FDS, Variant("u", len(fds))))
        head = Writer()
        head.buf += struct.pack("<cBBBII", b"l", kind, flags, 1, len(payload), self.serial)
        head.write("a(yv)", header_fields)
        head.pad(8)
        data = bytes(head.buf) + payload
        if fds:
            self.sock.sendmsg([data], [(socket.SOL_SOCKET, socket.SCM_RIGHTS, array.array("i", fds))])
        else:
            self.sock.sendall(data)
        return self.serial

    def _receive(self, timeout):
        """Read what arrives within `timeout` into the queues."""
        ready, _, _ = select.select([self.sock], [], [], timeout)
        if not ready:
            return
        data, ancillary, _, _ = self.sock.recvmsg(1 << 16, socket.CMSG_SPACE(16 * 4))
        if not data:
            raise ConnectionError("the bus closed the connection")
        for level, kind, payload in ancillary:
            if level == socket.SOL_SOCKET and kind == socket.SCM_RIGHTS:
                fds = array.array("i")
                fds.frombytes(payload[:len(payload) - len(payload) % fds.itemsize])
                self.infds.extend(fds)
        self.inbuf += data
        while True:
            message = self._take()
            if message is None:
                break
            if message.kind in (METHOD_RETURN, ERROR):
                self.replies[message.fields.get(REPLY_SERIAL)] = message
            elif message.kind == SIGNAL:
                self.signals.append(message)
            elif message.kind == METHOD_CALL:
                self._answer(message)

    def export(self, path, handler):
        """Answer the method calls to the object `path` with `handler`, which
        takes the message and returns the signature and the values of the
        reply, or raises `DBusError` with the name and the text of an error."""
        self.exported[path] = handler

    def _answer(self, message):
        """Answer one method call with the handler of its object, or with the
        error that no such object exists."""
        handler = self.exported.get(message.path)
        reply = [(REPLY_SERIAL, "u", message.serial), (DESTINATION, "s", message.sender)]
        try:
            if handler is None:
                raise DBusError("org.freedesktop.DBus.Error.UnknownMethod", "this client exports no such object")
            signature, body = handler(message)
        except DBusError as error:
            if not message.flags & NO_REPLY_EXPECTED:
                self.send(ERROR, reply + [(ERROR_NAME, "s", error.name)], "s", [error.message])
            return
        if not message.flags & NO_REPLY_EXPECTED:
            self.send(METHOD_RETURN, reply, signature, body)

    def _take(self):
        if len(self.inbuf) < 16:
            return None
        if self.inbuf[0:1] != b"l":
            raise ConnectionError("a message in big endian, which this client does not read")
        _, kind, flags, _, length, serial, fields_length = struct.unpack_from("<cBBBIII", self.inbuf)
        header_end = 16 + fields_length
        body_at = header_end + (-header_end) % 8
        if len(self.inbuf) < body_at + length:
            return None
        reader = Reader(bytes(self.inbuf[:body_at + length]), at=12)
        fields = dict((code, value) for code, value in reader.read("a(yv)"))
        signature = fields.get(SIGNATURE, "")
        body = Reader(bytes(self.inbuf[:body_at + length]), at=body_at).read_all(signature) if signature else []
        count = fields.get(UNIX_FDS, 0)
        fds, self.infds = self.infds[:count], self.infds[count:]
        del self.inbuf[:body_at + length]
        return Message(kind, flags, serial, fields, body, fds)

    def call(self, destination, path, interface, member, signature="", body=(), timeout=None, fds=()):
        """Call a method and return the body of its reply, raising
        `DBusError` for an error reply."""
        fields = [(PATH, "o", path), (MEMBER, "s", member), (DESTINATION, "s", destination)]
        if interface:
            fields.append((INTERFACE, "s", interface))
        serial = self.send(METHOD_CALL, fields, signature, body, fds=fds)
        deadline = time.monotonic() + (timeout or self.timeout)
        while serial not in self.replies:
            left = deadline - time.monotonic()
            if left <= 0:
                raise TimeoutError("%s.%s on %s: no reply within the timeout" % (interface, member, path))
            self._receive(left)
        reply = self.replies.pop(serial)
        if reply.kind == ERROR:
            raise DBusError(reply.fields.get(ERROR_NAME), reply.body[0] if reply.body else "")
        # A reply of type `h` names its descriptors by index into these.
        self.last_fds = reply.fds
        return reply.body

    def add_match(self, rule):
        self.call("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", "AddMatch", "s", [rule])

    def poll(self, timeout=0.0):
        """Read what arrives within `timeout`."""
        self._receive(timeout)

    def next_signal(self, timeout, predicate=lambda message: True):
        """Return the first queued or arriving signal `predicate` accepts,
        or None when none arrives within `timeout`."""
        deadline = time.monotonic() + timeout
        while True:
            for index, message in enumerate(self.signals):
                if predicate(message):
                    del self.signals[index]
                    return message
            left = deadline - time.monotonic()
            if left <= 0:
                return None
            self._receive(left)

    def close(self):
        self.sock.close()


def get_managed_objects(bus, service="org.bluez"):
    """Return the objects of `service` as {path: {interface: {property: value}}}."""
    return bus.call(service, "/", "org.freedesktop.DBus.ObjectManager", "GetManagedObjects")[0]


def get_property(bus, service, path, interface, name):
    return bus.call(service, path, "org.freedesktop.DBus.Properties", "Get", "ss", [interface, name])[0]


def set_property(bus, service, path, interface, name, value):
    bus.call(service, path, "org.freedesktop.DBus.Properties", "Set", "ssv", [interface, name, value])
