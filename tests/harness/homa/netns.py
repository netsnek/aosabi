# SPDX-License-Identifier: MIT
"""Network namespaces joined by a veth pair inside a guest, the two hosts of
the raw IP half of `29_homa_raw`.

The crate's engine runs over raw sockets between two network namespaces of guest
A, one of them with the queuing discipline of `qdisc.py` on its end of the veth
as the emulated switch. A namespace keeps its own Homa ports, its own abstract
names and its own raw sockets, so each side is a host of its own. Every command
goes through `run`, a callable that takes a shell command and returns an object
with `code`, `out` and `err`, which `guest.Guest.run` is and `local` gives for a
namespace of this machine.
"""

from __future__ import annotations

import subprocess
from dataclasses import dataclass


@dataclass(frozen=True)
class Done:
    """What a local command returned, in the shape of `guest.Result`."""

    code: int
    out: str
    err: str


def local(command, timeout=600.0, check=True):
    """Run `command` with `/bin/sh -c` on this machine."""
    done = subprocess.run(["sh", "-c", command], capture_output=True, text=True, timeout=timeout)
    if check and done.returncode != 0:
        raise RuntimeError("%r exited %d\n%s%s" % (command, done.returncode, done.stdout, done.stderr))
    return Done(done.returncode, done.stdout, done.stderr)


def quote(text):
    """Quote `text` for `sh`."""
    return "'" + str(text).replace("'", "'\\''") + "'"


class Pair:
    """Two namespaces, `a` and `b`, joined by the veth pair `<a>-veth` and
    `<b>-veth` with the IPv4 addresses `<subnet>.1` and `<subnet>.2` and the
    IPv6 addresses `<ula>::1` and `<ula>::2`.

    `create` builds them, `exec` returns a command run inside one, and
    `delete` removes both, which removes the pair as well.
    """

    def __init__(self, run, a="homa-a", b="homa-b", subnet="10.146.1", ula="fd46:146:1", mtu=1500):
        self.run = run
        self.names = (a, b)
        self.devices = (a + "-veth", b + "-veth")
        self.ipv4 = (subnet + ".1", subnet + ".2")
        self.ipv6 = (ula + "::1", ula + "::2")
        self.mtu = mtu

    def create(self):
        self.delete()
        a, b = self.names
        va, vb = self.devices
        commands = ["ip netns add %s" % a, "ip netns add %s" % b,
                    "ip link add %s netns %s mtu %d type veth peer name %s netns %s mtu %d"
                    % (va, a, self.mtu, vb, b, self.mtu)]
        for name, device, v4, v6 in zip(self.names, self.devices, self.ipv4, self.ipv6):
            commands += ["ip -n %s link set lo up" % name,
                         "ip -n %s addr add %s/24 dev %s" % (name, v4, device),
                         "ip -n %s addr add %s/64 dev %s nodad" % (name, v6, device),
                         "ip -n %s link set %s up" % (name, device)]
        self.run(" && ".join(commands))
        return self

    def exec(self, side, command):
        """Return `command` run inside namespace `side`, 0 for `a` and 1 for `b`."""
        return "ip netns exec %s sh -c %s" % (self.names[side], quote(command))

    def delete(self):
        """Remove both namespaces, which removes the pair, and any that is
        missing already."""
        for name in self.names:
            self.run("ip netns del %s 2>/dev/null || true" % name)

    def __enter__(self):
        return self.create()

    def __exit__(self, *exc):
        self.delete()
        return False
