# SPDX-License-Identifier: MIT
"""A pty pair in raw mode with a noise injector and a cut, the link of the
`pty` scenario of `homa-sim` and of the pty half of `26_carrier_serial`.

Two pseudo terminals from `os.openpty`, whose slaves are put in raw mode with
`tty.setraw`, and a relay thread that moves every byte one master reads to the
other master, so a program that opens the path of one slave and a program that
opens the path of the other talk as over a null modem cable. The devtool opens
the two slave paths through `std::fs::File` and calls no `openpty`, which keeps
`unsafe` out of the crate. The relay takes a hook that sees every chunk it
moves and returns what goes on.

`Noise` flips bits at a seeded bit error rate, 10 to the minus 4 in the pty
half of `26_carrier_serial`, and `Cut` drops every byte for a window, the cut
of 1 s of the same half, which a serial line without link events shows as
silence. `chain` runs hooks one after the other, so a relay can take both.
"""

from __future__ import annotations

import errno
import math
import os
import random
import select
import threading
import time
import tty


class Noise:
    """A hook that flips each bit it moves with the probability `ber`,
    drawn from a generator seeded with `seed`.

    The gap to the next flipped bit is drawn from the geometric distribution
    of the rate, so a chunk costs one draw per flip and not one per bit.
    `sides` names the directions it acts on, 0 for the bytes of slave A's
    side and 1 for those of B's, and `flipped` and `moved` count per side.
    """

    def __init__(self, seed, ber=1e-4, sides=(0, 1)):
        if not 0.0 < ber < 1.0:
            raise ValueError("a bit error rate lies between 0 and 1, not %r" % ber)
        self.rng = random.Random(seed)
        self.ber = ber
        self.sides = tuple(sides)
        self.flipped = [0, 0]
        self.moved = [0, 0]
        self.gap = [self._draw(), self._draw()]
        self.lock = threading.Lock()

    def _draw(self):
        """Return the count of bits before the next flipped one."""
        return int(math.log(1.0 - self.rng.random()) / math.log(1.0 - self.ber))

    def __call__(self, side, chunk):
        with self.lock:
            self.moved[side] += len(chunk)
            if side not in self.sides:
                return chunk
            bits = len(chunk) * 8
            at = self.gap[side]
            if at >= bits:
                self.gap[side] = at - bits
                return chunk
            damaged = bytearray(chunk)
            while at < bits:
                damaged[at // 8] ^= 1 << (at % 8)
                self.flipped[side] += 1
                at += 1 + self._draw()
            self.gap[side] = at - bits
            return bytes(damaged)


class Cut:
    """A hook that drops every byte while a cut lasts.

    `cut(seconds)` starts a cut on the relay's clock, `active` says whether
    one lasts, and `dropped` counts the bytes each side lost to it.
    """

    def __init__(self, clock=time.monotonic):
        self.clock = clock
        self.until = None
        self.dropped = [0, 0]
        self.cuts = 0
        self.lock = threading.Lock()

    def cut(self, seconds):
        with self.lock:
            self.until = self.clock() + seconds
            self.cuts += 1

    @property
    def active(self):
        with self.lock:
            return self.until is not None and self.clock() < self.until

    def __call__(self, side, chunk):
        if self.active:
            with self.lock:
                self.dropped[side] += len(chunk)
            return b""
        return chunk


def chain(*hooks):
    """Return a hook that runs `hooks` in order, each on what the one before
    it let through."""

    def hook(side, chunk):
        for one in hooks:
            if not chunk:
                break
            chunk = one(side, chunk)
        return chunk

    return hook


class PtyPair:
    """Two pseudo terminals joined by a relay between their masters.

    `path_a` and `path_b` name the slaves, `moved` counts the bytes the relay
    moved each way, and `close` stops the relay and closes every descriptor.
    """

    def __init__(self, hook=None):
        self.hook = hook
        self.masters = []
        self.slaves = []
        for _ in range(2):
            master, slave = os.openpty()
            tty.setraw(slave)
            os.set_blocking(master, False)
            self.masters.append(master)
            self.slaves.append(slave)
        self.path_a = os.ttyname(self.slaves[0])
        self.path_b = os.ttyname(self.slaves[1])
        self.moved = [0, 0]
        self.stopped = threading.Event()
        self.thread = threading.Thread(target=self._relay, name="pty-relay", daemon=True)
        self.thread.start()

    def _relay(self):
        """Move the bytes of each master to the other until `close`."""
        outgoing = [bytearray(), bytearray()]
        while not self.stopped.is_set():
            writers = [self.masters[1 - side] for side in (0, 1) if outgoing[side]]
            readable, writable, _ = select.select(self.masters, writers, [], 0.01)
            for side in (0, 1):
                if self.masters[side] not in readable:
                    continue
                try:
                    chunk = os.read(self.masters[side], 1 << 16)
                except BlockingIOError:
                    continue
                except OSError as error:
                    if error.errno == errno.EIO:
                        continue
                    raise
                if self.hook is not None:
                    chunk = self.hook(side, chunk)
                outgoing[side] += chunk
            for side in (0, 1):
                target = self.masters[1 - side]
                if outgoing[side] and target in writable:
                    try:
                        written = os.write(target, outgoing[side])
                    except BlockingIOError:
                        continue
                    self.moved[side] += written
                    del outgoing[side][:written]

    def close(self):
        """Stop the relay and close every descriptor of the pair."""
        self.stopped.set()
        self.thread.join(timeout=5)
        for fd in self.masters + self.slaves:
            try:
                os.close(fd)
            except OSError:
                pass
