# SPDX-License-Identifier: MIT
"""The emulated switch of the raw IP half of `29_homa_raw`: the `prio`
queuing discipline of 8 bands, each Homa priority level mapped to a band from
the TOS bits or the traffic class, over `netem` for delay, loss and rate.

The crate's engine writes the level of a packet into the three high bits of
the IPv4 TOS byte or the IPv6 traffic class, the level shifted left by five.
The priority map of `prio` reads the TOS field of RFC 1349 and not those
bits, so a `u32` filter per level and family puts a packet of level `l` into
band `7 - l`, band 0 draining first, and a packet no filter names into the
band of level 0. Each band holds a `netem` qdisc of its own, so the loss of 1
percent the notebook asks for falls on every band alike. The host has no
`tc`, so the commands run in guest A through the `run` of `netns.py`.
"""

from __future__ import annotations

import re

BANDS = 8
LEVEL_MASK = 0xE0


def band_of(level):
    """Return the band of a Homa level, band 0 for level 7."""
    return BANDS - 1 - level


def commands(device, netns=None, delay_ms=0.0, loss_percent=0.0, rate=None, limit=10_000):
    """Return the `tc` commands that put the switch on `device`, in the
    namespace `netns` where one is named."""
    tc = "tc -n %s" % netns if netns else "tc"
    lowest = band_of(0)
    priomap = " ".join([str(lowest)] * 16)
    out = ["%s qdisc replace dev %s root handle 1: prio bands %d priomap %s" % (tc, device, BANDS, priomap)]
    for level in range(BANDS):
        flow = "1:%d" % (band_of(level) + 1)
        out.append("%s filter add dev %s parent 1: protocol ip prio 1 u32 match ip tos 0x%02x 0x%02x flowid %s"
                   % (tc, device, level << 5, LEVEL_MASK, flow))
        out.append("%s filter add dev %s parent 1: protocol ipv6 prio 2 u32 match ip6 priority 0x%02x 0x%02x "
                   "flowid %s" % (tc, device, level << 5, LEVEL_MASK, flow))
    for band in range(BANDS):
        netem = "netem limit %d" % limit
        if delay_ms:
            netem += " delay %.3fms" % delay_ms
        if loss_percent:
            netem += " loss %.4f%%" % loss_percent
        if rate:
            netem += " rate %s" % rate
        out.append("%s qdisc add dev %s parent 1:%d handle %d: %s" % (tc, device, band + 1, 10 + band, netem))
    return out


def apply(run, device, netns=None, **netem):
    """Put the switch on `device` through `run` and return the commands."""
    lines = commands(device, netns, **netem)
    run(" && ".join(lines))
    return lines


def remove(run, device, netns=None):
    tc = "tc -n %s" % netns if netns else "tc"
    run("%s qdisc del dev %s root 2>/dev/null || true" % (tc, device))


def band_packets(run, device, netns=None):
    """Return the packets each band sent, read from `tc -s qdisc`, as a list
    indexed by band."""
    tc = "tc -n %s" % netns if netns else "tc"
    text = run("%s -s qdisc show dev %s" % (tc, device)).out
    counts = [0] * BANDS
    for match in re.finditer(r"qdisc netem (\d+): parent 1:(\d+).*?\n\s*Sent \d+ bytes (\d+) pkt", text, re.S):
        band = int(match.group(2)) - 1
        if 0 <= band < BANDS:
            counts[band] = int(match.group(3))
    return counts
