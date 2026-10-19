# SPDX-License-Identifier: MIT
"""A Linux central for the Bluetooth carrier: the GATT data path through
BlueZ over D-Bus, and the L2CAP LE credit based channel through
`socket.AF_BLUETOOTH`, the client side of the emulated half of
`28_carrier_bluetooth`.

The carrier publishes an encrypted characteristic that holds the PSM of its
channel, fixed at 0x00E0 or the number the kernel allocated where the bind of
that one failed, and serves the channel with prefixed AoFrame in both
directions, and beside it the GATT data path, RX written without response and
TX notified, one Homa packet per ATT value. The UUIDs of the service and its
characteristics are the carrier's, so a notebook hands them in from the
carrier's profile and this module names none.

`GattCentral` connects to a peripheral through BlueZ with the D-Bus client of
`dbus.py`, reads the PSM, and writes and receives packets, and `GattTransport`
makes it a transport of the engine of `client.py`, so the Python engine
carries Homa over GATT as over a socket. `Channel` opens the channel as a
socket of the standard library, with the security level and the L2CAP mode a
row asks for, `BT_MODE_LE_FLOWCTL` for a request with code 0x14 and
`BT_MODE_EXT_FLOWCTL` for one with code 0x17, and `ChannelTransport` carries
prefixed AoFrame over it, sending no SDU above the channel's MTU and reading
the channel either one SDU at a time or as a stream in chunks of random sizes,
the reader that finds a misframing. Nothing is imported beside the standard
library and this package.
"""

from __future__ import annotations

import collections
import random
import socket
import struct
import time

from . import dbus
from .client import StreamTransport, _Transport

BLUEZ = "org.bluez"
ADAPTER = "org.bluez.Adapter1"
DEVICE = "org.bluez.Device1"
CHARACTERISTIC = "org.bluez.GattCharacteristic1"
PROPERTIES = "org.freedesktop.DBus.Properties"

# The PSM of the channel, ADR-0023 section Fourth amendment.
SPSM = 0x00E0


AGENT_PATH = "/org/aosabi/agent"


def register_agent(bus, path=AGENT_PATH):
    """Register a default agent with the capability NoInputNoOutput that
    accepts every pairing and every authorization, so a pairing runs as Just
    Works without a user.

    BlueZ answers a pairing only through an agent and refuses it where none is
    registered. In guest A one `bluetoothd` holds both adapters, so this agent
    answers the peripheral's side as well as the central's.
    """
    def answer(message):
        if message.interface != "org.bluez.Agent1":
            raise dbus.DBusError("org.freedesktop.DBus.Error.UnknownInterface", "only org.bluez.Agent1")
        if message.member in ("RequestPinCode", "RequestPasskey"):
            raise dbus.DBusError("org.bluez.Error.Rejected", "NoInputNoOutput enters no key")
        return "", []

    bus.export(path, answer)
    bus.call(BLUEZ, "/org/bluez", "org.bluez.AgentManager1", "RegisterAgent", "os",
             [dbus.ObjectPath(path), "NoInputNoOutput"])
    bus.call(BLUEZ, "/org/bluez", "org.bluez.AgentManager1", "RequestDefaultAgent", "o", [dbus.ObjectPath(path)])


def device_path(adapter_path, address):
    """Return BlueZ's object path of the device `address` on an adapter."""
    return "%s/dev_%s" % (adapter_path, address.upper().replace(":", "_"))


class GattCentral:
    """A central that reaches one peripheral through BlueZ.

    `connect` powers the adapter, finds the device, connects and waits for
    its services, `read_psm` reads the PSM characteristic, `write` sends one
    packet to RX without a response, `start_notify` subscribes to TX, and
    `notifications` returns the values that arrived.

    `pace` holds both directions to a number of packets a connection event
    of a given interval, the link a controller keeps and the emulated
    controllers of `btvirt` do not: a value that arrived is handed out at the
    first event after it, and a write goes out at the first event after it
    was made, each direction at most that number an event.

    Every packet is stamped on the realtime clock, `time.time_ns`, the clock
    the trace of the carrier's driver stamps in the same kernel, so the stamps
    of one packet at both ends compare. Each value is an entry of `reads`,
    `[read, hand out, value]`, read when `notifications` takes its
    `PropertiesChanged`, which is its arrival in `arrived`, and handed out
    when `_pump` hands it on. Each write is an entry of `writes`, `[made,
    written, packet]`, made when `write` takes it and written when `_pump`
    writes it at its event. A link that is not paced hands a value out as it
    is read and writes a packet as it is made.
    """

    def __init__(self, address, service_uuid, psm_uuid, rx_uuid, tx_uuid, adapter="hci0", bus=None):
        self.bus = bus or dbus.Bus()
        self.address = address.upper()
        self.uuids = {"service": service_uuid.lower(), "psm": psm_uuid.lower(), "rx": rx_uuid.lower(),
                      "tx": tx_uuid.lower()}
        self.adapter = "/org/bluez/" + adapter
        self.device = device_path(self.adapter, self.address)
        self.paths = {}
        self.values = collections.deque()
        self.handed = collections.deque()
        self.notifying = False
        self.pacing = None
        self.arrived = collections.deque()
        self.outgoing = collections.deque()
        self.began = 0.0
        self.event = 0
        self.reads = []
        self.writes = []

    def _objects(self):
        return dbus.get_managed_objects(self.bus)

    def power(self, on=True):
        dbus.set_property(self.bus, BLUEZ, self.adapter, ADAPTER, "Powered", dbus.Variant("b", on))

    def discover(self, timeout=20.0):
        """Scan until the device appears, and return its properties."""
        self.bus.add_match("type='signal',sender='org.bluez',interface='org.freedesktop.DBus.ObjectManager'")
        self.bus.call(BLUEZ, self.adapter, ADAPTER, "SetDiscoveryFilter", "a{sv}",
                      [{"Transport": dbus.Variant("s", "le")}])
        self.bus.call(BLUEZ, self.adapter, ADAPTER, "StartDiscovery")
        try:
            deadline = time.monotonic() + timeout
            while time.monotonic() < deadline:
                objects = self._objects()
                if self.device in objects:
                    return objects[self.device].get(DEVICE, {})
                self.bus.poll(0.5)
            raise TimeoutError("no device %s within %.0f s" % (self.address, timeout))
        finally:
            try:
                self.bus.call(BLUEZ, self.adapter, ADAPTER, "StopDiscovery")
            except dbus.DBusError:
                pass

    def connect(self, timeout=30.0):
        """Connect and wait until the device's services are resolved, then
        find the characteristics by their UUIDs."""
        self.power(True)
        if self.device not in self._objects():
            self.discover(timeout)
        self.bus.call(BLUEZ, self.device, DEVICE, "Connect", timeout=timeout)
        deadline = time.monotonic() + timeout
        while not dbus.get_property(self.bus, BLUEZ, self.device, DEVICE, "ServicesResolved"):
            if time.monotonic() > deadline:
                raise TimeoutError("the services of %s were not resolved within %.0f s" % (self.address, timeout))
            self.bus.poll(0.2)
        self._find()

    def _find(self):
        objects = self._objects()
        services = [path for path, interfaces in objects.items()
                    if path.startswith(self.device + "/")
                    and interfaces.get("org.bluez.GattService1", {}).get("UUID", "").lower() == self.uuids["service"]]
        if len(services) != 1:
            raise LookupError("%d services of %s on %s" % (len(services), self.uuids["service"], self.address))
        for path, interfaces in objects.items():
            props = interfaces.get(CHARACTERISTIC)
            if props is None or props.get("Service") != services[0]:
                continue
            for role in ("psm", "rx", "tx"):
                if props.get("UUID", "").lower() == self.uuids[role]:
                    self.paths[role] = path
        missing = [role for role in ("psm", "rx", "tx") if role not in self.paths]
        if missing:
            raise LookupError("no characteristic for %s on %s" % (", ".join(missing), self.address))

    def read_psm(self):
        """Read the PSM characteristic, a little endian `u16`."""
        value = self.bus.call(BLUEZ, self.paths["psm"], CHARACTERISTIC, "ReadValue", "a{sv}", [{}])[0]
        return struct.unpack("<H", value[:2])[0]

    def att_mtu(self):
        """Return the ATT_MTU BlueZ negotiated, which it names on each
        characteristic."""
        return dbus.get_property(self.bus, BLUEZ, self.paths["tx"], CHARACTERISTIC, "MTU")

    def pace(self, interval, per_event):
        """Pace both directions at `per_event` packets a connection event of
        `interval` seconds from now on."""
        self.pacing = (interval, per_event)
        self.began = time.monotonic()
        self.event = 0

    def _pump(self):
        """Hand out the values and send the writes of the events that passed."""
        if self.pacing is None:
            return
        interval, per_event = self.pacing
        last = int((time.monotonic() - self.began) / interval)
        while self.event < last:
            if not self.arrived and not self.outgoing:
                self.event = last
                break
            self.event += 1
            at = self.began + self.event * interval
            for _ in range(per_event):
                if self.arrived and self.arrived[0][0] <= at:
                    self._hand_out(self.arrived.popleft()[1])
            for _ in range(per_event):
                if self.outgoing and self.outgoing[0][0] <= at:
                    self._write_now(self.outgoing.popleft()[1])

    def _hand_out(self, entry):
        """Hand out the value of the entry `entry` of `reads`, stamped now, or
        the end of the link where `entry` is None."""
        if entry is not None:
            entry[1] = time.time_ns()
        self.values.append(None if entry is None else entry[2])
        self.handed.append(entry)

    def write(self, packet):
        """Write one packet to RX without a response, at the next event where
        the link is paced."""
        entry = [time.time_ns(), None, bytes(packet)]
        self.writes.append(entry)
        if self.pacing is None:
            self._write_now(entry)
        else:
            self.outgoing.append((time.monotonic(), entry))
            self._pump()

    def _write_now(self, entry):
        """Write the packet of the entry `entry` of `writes`, stamped now, and
        read a write BlueZ refuses with `org.bluez.Error.Failed` as the link
        going down, which a disconnect answers before its signal arrives."""
        entry[1] = time.time_ns()
        try:
            self.bus.call(BLUEZ, self.paths["rx"], CHARACTERISTIC, "WriteValue", "aya{sv}",
                          [entry[2], {"type": dbus.Variant("s", "command")}])
        except dbus.DBusError as error:
            if error.name != "org.bluez.Error.Failed":
                raise
            self.outgoing.clear()
            self._hand_out(None)

    def start_notify(self):
        for path in (self.paths["tx"], self.device):
            self.bus.add_match("type='signal',sender='org.bluez',interface='%s',member='PropertiesChanged',"
                               "path='%s'" % (PROPERTIES, path))
        self.bus.call(BLUEZ, self.paths["tx"], CHARACTERISTIC, "StartNotify")
        self.notifying = True

    def stop_notify(self):
        if self.notifying:
            self.bus.call(BLUEZ, self.paths["tx"], CHARACTERISTIC, "StopNotify")
            self.notifying = False

    def notifications(self, timeout=0.0, stamped=False):
        """Return every TX value handed out within `timeout`, in order, with
        None where the link went down, or with `stamped` the entry of `reads`
        of each value in its place."""
        deadline = time.monotonic() + timeout
        while True:
            wait = max(0.0, deadline - time.monotonic())
            if self.pacing is not None:
                wait = min(wait, self.pacing[0] / 4)
            self.bus.poll(wait)
            now, read = time.monotonic(), time.time_ns()
            while self.bus.signals:
                message = self.bus.signals.popleft()
                if message.path == self.paths.get("tx") and message.member == "PropertiesChanged":
                    interface, changed = message.body[0], message.body[1]
                    if interface == CHARACTERISTIC and "Value" in changed:
                        entry = [read, None, bytes(changed["Value"])]
                        self.reads.append(entry)
                        if self.pacing is None:
                            self._hand_out(entry)
                        else:
                            self.arrived.append((now, entry))
                elif message.path == self.device and message.member == "PropertiesChanged":
                    if message.body[1].get("Connected") is False:
                        self._hand_out(None)
            self._pump()
            if self.values or time.monotonic() >= deadline:
                break
        out = list(self.handed if stamped else self.values)
        self.values.clear()
        self.handed.clear()
        return out

    def disconnect(self):
        try:
            self.stop_notify()
        finally:
            self.bus.call(BLUEZ, self.device, DEVICE, "Disconnect")

    def fileno(self):
        return self.bus.fileno()


class GattTransport(_Transport):
    """The GATT data path as a transport of `client.Engine`: each packet one
    write to RX without a response and one notification of TX, and a
    disconnect of the device closes the transport."""

    def __init__(self, central, max_packet, hook=None):
        super().__init__(central, hook)
        self.central = central
        self.max_packet = max_packet
        if not central.notifying:
            central.start_notify()

    def _queue(self, packet):
        self.pending.append(packet)
        self.counts["link_bytes_out"] += len(packet)

    def flush(self):
        while self.pending and not self.closed:
            self.central.write(self.pending.popleft())
        return True

    def recv(self, limit=None):
        out = []
        for value in self.central.notifications(0.0):
            if value is None:
                self.closed = True
                break
            self.counts["link_bytes_in"] += len(value)
            out.append(value)
        return self.received(out[:limit] if limit is not None else out)


# The constants of the kernel's Bluetooth sockets, which the socket module of
# Python names from 3.14 on and which stand here for an older interpreter.
SOL_BLUETOOTH = 274
BT_SECURITY = 4
BT_SNDMTU = 12
BT_RCVMTU = 13
BT_MODE = 15
BDADDR_LE_PUBLIC = 0x01
BDADDR_LE_RANDOM = 0x02
BT_SECURITY_LEVELS = {"low": 1, "medium": 2, "high": 3}
MODES = {"le": 0x03, "ecred": 0x04}


class Channel:
    """The L2CAP LE credit based channel to `address` on `psm`, a socket of
    the standard library.

    `security` is the level the socket asks for before it connects, `low`
    for a request on a link without encryption, `mode` is `le` for a request
    with code 0x14 and `ecred` for one with code 0x17, and `kind` is
    `SOCK_SEQPACKET`, one SDU a read, or `SOCK_STREAM`, a stream across SDUs.
    """

    def __init__(self, address, psm=SPSM, address_type=BDADDR_LE_PUBLIC, security="medium", mode="le",
                 kind=socket.SOCK_SEQPACKET, timeout=10.0):
        self.sock = socket.socket(socket.AF_BLUETOOTH, kind, socket.BTPROTO_L2CAP)
        self.sock.bind((socket.BDADDR_ANY, 0, 0, address_type))
        self.sock.setsockopt(SOL_BLUETOOTH, BT_SECURITY, struct.pack("BB", BT_SECURITY_LEVELS[security], 0))
        self.sock.setsockopt(SOL_BLUETOOTH, BT_MODE, struct.pack("B", MODES[mode]))
        self.sock.settimeout(timeout)
        self.sock.connect((address, psm, 0, address_type))
        self.kind = kind

    def mtu(self):
        """Return the send and the receive MTU of the channel."""
        send = struct.unpack("H", self.sock.getsockopt(SOL_BLUETOOTH, BT_SNDMTU, 2))[0]
        recv = struct.unpack("H", self.sock.getsockopt(SOL_BLUETOOTH, BT_RCVMTU, 2))[0]
        return send, recv

    def close(self):
        self.sock.close()


class _Chunked:
    """A socket whose every read takes at most a size drawn from a range,
    so a stream decoder meets frames split and joined anywhere."""

    def __init__(self, sock, chunks, seed):
        self.sock = sock
        self.low, self.high = chunks
        self.rng = random.Random(seed)

    def recv(self, size):
        return self.sock.recv(max(1, min(size, self.rng.randint(self.low, self.high))))

    def send(self, data):
        return self.sock.send(data)

    def setblocking(self, flag):
        self.sock.setblocking(flag)

    def fileno(self):
        return self.sock.fileno()


class ChannelTransport(StreamTransport):
    """Prefixed AoFrame over the channel as a transport of `client.Engine`.

    A write never exceeds the channel's send MTU, one SDU each. A read takes
    what the socket holds, or at most a size drawn from `chunks`, a range such
    as `(1, 700)` with the seed `seed`, which on a `SOCK_STREAM` channel reads
    the channel as a stream across SDUs.
    """

    def __init__(self, channel, max_packet, hook=None, chunks=None, seed=0):
        sock = channel.sock if chunks is None else _Chunked(channel.sock, chunks, seed)
        super().__init__(sock, max_packet, hook)
        self.channel = channel
        self.send_mtu, self.recv_mtu = channel.mtu()

    def flush(self):
        while self.outbuf and not self.closed:
            try:
                sent = self.sock.send(bytes(self.outbuf[:self.send_mtu]))
            except BlockingIOError:
                return False
            except (BrokenPipeError, ConnectionResetError, ConnectionAbortedError):
                self.closed = True
                return False
            del self.outbuf[:sent]
        return True
