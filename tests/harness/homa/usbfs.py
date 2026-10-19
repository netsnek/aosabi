# SPDX-License-Identifier: MIT
"""A USB host over usbfs through `fcntl.ioctl`: bulk transfers, halts cleared
and resets, the host side of the emulated half of `27_carrier_usb`.

In guest A the gadget of `aosabi-carrier-usb` runs on `dummy_hcd`, whose
emulated host controller the guest's own USB core enumerates, so the device
appears under `/dev/bus/usb/` of the same guest and this module drives it the
way a program with no driver of its own does. Every request is an ioctl of
`include/uapi/linux/usbdevice_fs.h`, whose numbers the module computes with
the rule of `include/uapi/asm-generic/ioctl.h`. The one structure that holds a
pointer, the bulk transfer, is built with `ctypes`, so the module imports
nothing outside the standard library.

A failed transfer raises the `OSError` of the ioctl, whose `errno` names what
happened: `ETIMEDOUT` for a transfer that did not end within its timeout,
`EPIPE` for a halted endpoint, and `ENODEV` or `ESHUTDOWN` for a device that
left the bus, which a notebook reads as the link going down.
"""

from __future__ import annotations

import collections
import ctypes
import errno
import fcntl
import os
import threading
import time
from pathlib import Path

from .client import _Fd, _Transport

SYSFS = Path("/sys/bus/usb/devices")
DEVFS = Path("/dev/bus/usb")

_IOC_NONE = 0
_IOC_WRITE = 1
_IOC_READ = 2
_USB = ord("U")


def _ioc(direction, nr, size):
    """Return the request number `_IOC(direction, 'U', nr, size)`, whose
    fields are 8 bits of number, 8 of type, 14 of size and 2 of direction."""
    return (direction << 30) | (size << 16) | (_USB << 8) | nr


class BulkTransfer(ctypes.Structure):
    """`struct usbdevfs_bulktransfer`."""

    _fields_ = [("ep", ctypes.c_uint), ("len", ctypes.c_uint), ("timeout", ctypes.c_uint),
                ("data", ctypes.c_void_p)]


class CtrlTransfer(ctypes.Structure):
    """`struct usbdevfs_ctrltransfer`."""

    _fields_ = [("bRequestType", ctypes.c_uint8), ("bRequest", ctypes.c_uint8), ("wValue", ctypes.c_uint16),
                ("wIndex", ctypes.c_uint16), ("wLength", ctypes.c_uint16), ("timeout", ctypes.c_uint32),
                ("data", ctypes.c_void_p)]


class UsbIoctl(ctypes.Structure):
    """`struct usbdevfs_ioctl`, which passes a request to the driver of one
    interface."""

    _fields_ = [("ifno", ctypes.c_int), ("ioctl_code", ctypes.c_int), ("data", ctypes.c_void_p)]


_UINT = ctypes.sizeof(ctypes.c_uint)
USBDEVFS_CONTROL = _ioc(_IOC_READ | _IOC_WRITE, 0, ctypes.sizeof(CtrlTransfer))
USBDEVFS_BULK = _ioc(_IOC_READ | _IOC_WRITE, 2, ctypes.sizeof(BulkTransfer))
USBDEVFS_SETCONFIGURATION = _ioc(_IOC_READ, 5, _UINT)
USBDEVFS_CLAIMINTERFACE = _ioc(_IOC_READ, 15, _UINT)
USBDEVFS_RELEASEINTERFACE = _ioc(_IOC_READ, 16, _UINT)
USBDEVFS_IOCTL = _ioc(_IOC_READ | _IOC_WRITE, 18, ctypes.sizeof(UsbIoctl))
USBDEVFS_RESET = _ioc(_IOC_NONE, 20, 0)
USBDEVFS_CLEAR_HALT = _ioc(_IOC_READ, 21, _UINT)
USBDEVFS_DISCONNECT = _ioc(_IOC_NONE, 22, 0)
USBDEVFS_CONNECT = _ioc(_IOC_NONE, 23, 0)

GONE = (errno.ENODEV, errno.ESHUTDOWN)


def _read(path):
    try:
        return path.read_text().strip()
    except OSError:
        return None


def devices():
    """Return every USB device of the system as a dict of its sysfs name,
    vendor, product, bus and device number, speed and node."""
    found = []
    for entry in sorted(SYSFS.iterdir()) if SYSFS.is_dir() else []:
        vendor = _read(entry / "idVendor")
        if vendor is None or ":" in entry.name:
            continue
        bus, dev = int(_read(entry / "busnum")), int(_read(entry / "devnum"))
        found.append({"sysfs": entry.name, "vendor": int(vendor, 16), "product": int(_read(entry / "idProduct"), 16),
                      "bus": bus, "device": dev, "speed": _read(entry / "speed"),
                      "node": str(DEVFS / ("%03d" % bus) / ("%03d" % dev))})
    return found


def endpoints(sysfs_name, interface):
    """Return the endpoints of `interface` of the device `sysfs_name` as
    dicts of their address, type and `wMaxPacketSize`, read from sysfs."""
    directory = SYSFS / ("%s:1.%d" % (sysfs_name, interface))
    out = []
    for entry in sorted(directory.glob("ep_*")):
        out.append({"address": int(_read(entry / "bEndpointAddress"), 16), "type": _read(entry / "type"),
                    "max_packet_size": int(_read(entry / "wMaxPacketSize"), 16),
                    "direction": _read(entry / "direction")})
    return out


class Device:
    """One USB device opened through usbfs.

    `claim` takes an interface for this process, `detach` first moves the
    kernel's driver off it where one holds it, `bulk_out` and `bulk_in` move
    one transfer, `clear_halt` clears a halted endpoint and `reset` resets
    the device.
    """

    def __init__(self, node):
        self.node = str(node)
        self.fd = os.open(self.node, os.O_RDWR)
        self.claimed = []

    @classmethod
    def find(cls, vendor, product):
        """Open the one device of `vendor` and `product`, and return it with
        the dict `devices` gives it."""
        matches = [d for d in devices() if d["vendor"] == vendor and d["product"] == product]
        if len(matches) != 1:
            raise LookupError("%d devices of %04x:%04x, one expected" % (len(matches), vendor, product))
        device = cls(matches[0]["node"])
        device.info = matches[0]
        return device

    def _uint(self, request, value):
        buffer = ctypes.c_uint(value)
        fcntl.ioctl(self.fd, request, buffer)

    def detach(self, interface):
        """Move the kernel's driver off `interface`, where one holds it."""
        command = UsbIoctl(interface, USBDEVFS_DISCONNECT, None)
        try:
            fcntl.ioctl(self.fd, USBDEVFS_IOCTL, command)
        except OSError as error:
            if error.errno != errno.ENODATA:
                raise

    def claim(self, interface, detach=True):
        if detach:
            self.detach(interface)
        self._uint(USBDEVFS_CLAIMINTERFACE, interface)
        self.claimed.append(interface)

    def release(self, interface):
        self._uint(USBDEVFS_RELEASEINTERFACE, interface)
        self.claimed.remove(interface)

    def bulk_out(self, endpoint, data, timeout_ms=1000):
        """Send `data` as one transfer to the OUT `endpoint` and return the
        bytes taken. A transfer of no bytes is a zero length packet."""
        buffer = ctypes.create_string_buffer(bytes(data), len(data)) if data else None
        transfer = BulkTransfer(endpoint & 0x7F, len(data), timeout_ms,
                                ctypes.addressof(buffer) if buffer is not None else None)
        return fcntl.ioctl(self.fd, USBDEVFS_BULK, transfer)

    def bulk_in(self, endpoint, length, timeout_ms=1000):
        """Receive one transfer of at most `length` bytes from the IN
        `endpoint`, which a short packet ends."""
        buffer = ctypes.create_string_buffer(length)
        transfer = BulkTransfer(endpoint | 0x80, length, timeout_ms, ctypes.addressof(buffer))
        count = fcntl.ioctl(self.fd, USBDEVFS_BULK, transfer)
        return buffer.raw[:count]

    def clear_halt(self, endpoint):
        self._uint(USBDEVFS_CLEAR_HALT, endpoint)

    def control(self, request_type, request, value, index, data=b"", timeout_ms=1000):
        """Send one control transfer with no data stage or an OUT data
        stage of `data`, and return the bytes the device took."""
        buffer = ctypes.create_string_buffer(bytes(data), len(data)) if data else None
        transfer = CtrlTransfer(request_type, request, value, index, len(data), timeout_ms,
                                ctypes.addressof(buffer) if buffer is not None else None)
        return fcntl.ioctl(self.fd, USBDEVFS_CONTROL, transfer)

    def set_halt(self, endpoint):
        """Halt `endpoint` with SET_FEATURE(ENDPOINT_HALT), the standard
        request of the USB 2.0 specification, section 9.4.9, so its next
        transfer stalls until the host clears the halt."""
        self.control(0x02, 0x03, 0, endpoint)

    def reset(self):
        fcntl.ioctl(self.fd, USBDEVFS_RESET, 0)

    def close(self):
        for interface in list(self.claimed):
            try:
                self.release(interface)
            except OSError:
                pass
        os.close(self.fd)

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        self.close()
        return False


class BulkTransport(_Transport):
    """USB bulk as a transport of `client.Engine`: one Homa packet a
    transfer, a transfer of an exact multiple of `wMaxPacketSize` below the
    largest packet followed by a zero length packet.

    A thread of the harness reads the IN endpoint with a blocking transfer at
    a time, so no transfer is cut by a timeout while its bytes are on the
    bus, and hands each to the engine's thread through a queue, counting in
    `exact_in` the transfers of an exact multiple below the largest packet,
    each of which only a zero length packet of the device ends. A transfer
    that stalls, where the device's endpoint is halted, clears the halt and
    is counted in `halts_cleared`, and the transfer tried again. A device that
    left the bus closes the transport, which the engine reads as its link
    going down.
    """

    def __init__(self, device, endpoint_in, endpoint_out, max_packet, max_packet_size, hook=None):
        self.wake_read, self.wake_write = os.pipe()
        os.set_blocking(self.wake_read, False)
        super().__init__(_Fd(self.wake_read), hook)
        self.device = device
        self.endpoint_in = endpoint_in
        self.endpoint_out = endpoint_out
        self.max_packet = max_packet
        self.max_packet_size = max_packet_size
        self.incoming = collections.deque()
        self.lock = threading.Lock()
        self.counts.update({"halts_cleared": 0, "zero_length_out": 0, "exact_in": 0, "gone_errno": 0})
        self.stopped = threading.Event()
        self.reader = threading.Thread(target=self._read, name="usbfs-bulk-in", daemon=True)
        self.reader.start()

    def _gone(self, error):
        with self.lock:
            self.counts["gone_errno"] = error.errno
        self.closed = True
        os.write(self.wake_write, b"\x00")

    def _read(self):
        while not self.stopped.is_set() and not self.closed:
            try:
                data = self.device.bulk_in(self.endpoint_in, self.max_packet, timeout_ms=200)
            except OSError as error:
                if error.errno == errno.ETIMEDOUT:
                    continue
                if error.errno == errno.EPIPE:
                    self.device.clear_halt(self.endpoint_in | 0x80)
                    with self.lock:
                        self.counts["halts_cleared"] += 1
                    continue
                self._gone(error)
                return
            if data:
                with self.lock:
                    self.incoming.append(data)
                    if len(data) % self.max_packet_size == 0 and len(data) < self.max_packet:
                        # Only a zero length packet ends such a transfer
                        # before the buffer of max_packet bytes is full.
                        self.counts["exact_in"] += 1
                os.write(self.wake_write, b"\x00")

    def _queue(self, packet):
        self.pending.append(packet)
        self.counts["link_bytes_out"] += len(packet)

    def _out(self, data):
        while True:
            try:
                self.device.bulk_out(self.endpoint_out, data, timeout_ms=5000)
                return True
            except OSError as error:
                if error.errno == errno.EPIPE:
                    self.device.clear_halt(self.endpoint_out & 0x7F)
                    with self.lock:
                        self.counts["halts_cleared"] += 1
                    continue
                self._gone(error)
                return False

    def flush(self):
        while self.pending and not self.closed:
            packet = self.pending.popleft()
            if not self._out(packet):
                return False
            if len(packet) and len(packet) % self.max_packet_size == 0 and len(packet) < self.max_packet:
                self.counts["zero_length_out"] += 1
                if not self._out(b""):
                    return False
        return True

    def recv(self, limit=None):
        try:
            os.read(self.wake_read, 1 << 16)
        except BlockingIOError:
            pass
        out = []
        with self.lock:
            while self.incoming and (limit is None or len(out) < limit):
                packet = self.incoming.popleft()
                self.counts["link_bytes_in"] += len(packet)
                out.append(packet)
        return self.received(out)

    def close(self):
        self.stopped.set()
        self.reader.join(timeout=2)
        os.close(self.wake_read)
        os.close(self.wake_write)
