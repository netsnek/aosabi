# SPDX-License-Identifier: MIT
"""The checks over the trace of a run of `aosabi-devtool homa-sim`.

The properties of the engine that no second implementation gives are asserted
over what a run records rather than compared. A trace is the file a scenario
names in `trace`, one JSON array a line, written by the devtool:

    ["T", at, side, rpc, bytes_left, fifo, [[rpc, bytes_left], ...]]
        a DATA packet `side` sent, its message's bytes left before it,
        whether the FIFO share chose it, and every message that could have
        gone instead
    ["G", at, side, rpc, offset]            a GRANT `side` sent
    ["S", at, side, rpc, offset, length]    a RESEND `side` sent
    ["R", at, side, rpc, offset, len, msg_length]
                                            a DATA packet `side` took
    ["M", at, side, rpc, msg_length]        a START_MSG `side` took, which
                                            opens a message with no byte
    ["D", at, side, rpc, length]            a whole message `side` reported
    ["F", at, side, rpc, why]               a client RPC that failed
    ["U", at, side, rpc, what]              an RPC_UNKNOWN the client took
    ["X", at, side, reason]                 a packet the scenario dropped

`rpc` is the identifier the client names the RPC by, so a message is the RPC
and the side that receives it: the request is received by B and the answer
by A.

A run on a profile of the full form also writes a row for every decision of
the grant scheduler, and a run of the operation `fabric` names a host by its
number where a point to point run names a side, and writes the peer of every
packet beside:

    ["P", at, side, rpc, offset, level, rank, active, window, fifo]
        a decision of the grant scheduler of `side`: the offset it grants up
        to, the level of the GRANT, the rank of the message among the
        messages granted at once, their count, the window and whether the
        FIFO share chose it, and on a fabric the host that sends the message
        last
    ["R", at, host, rpc, offset, len, msg_length, from]
    ["M", at, host, rpc, msg_length, from]  a START_MSG `host` took
    ["G", at, host, rpc, offset, priority, to]
    ["S", at, host, rpc, offset, length, to]
    ["D", at, host, rpc]                    a whole message `host` reported
    ["F", at, host, rpc, why]
    ["O", at, to, level]                    a packet the switch dropped

A host writes the decisions a packet caused before the row of that packet,
since the devtool reads them after the engine took it, so `fabric` moves the
row of the packet ahead of them before it checks.

The rules checked are those of the Homa design note, first version, sections
1.4 and 2.4, with the FIFO share of a point to point link by count: the
sender sends the message with the fewest bytes left outside its FIFO share, a
receiver grants to one message at each decision, the one with the fewest
bytes not yet received or the oldest on a turn of the FIFO share, and a
receiver keeps the bytes it granted and did not receive within `window` at
every packet it takes and every GRANT it sends. A receiver grants only while
the bytes it expects stay within `max_incoming`, so `grants` holds the bytes
it expects, unscheduled or granted and not received, to that bound at every
GRANT, and counts apart the rows at which they exceed it at all, since the
unscheduled bytes of a message that starts arrive with no GRANT. A notebook
that holds the bound at every row asserts that count to be 0.
"""

import json


def read(path) -> list:
    """Return the rows of a trace file."""
    rows = []
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if line:
                rows.append(json.loads(line))
    return rows


def _opened(row: list) -> list:
    """Return the `M` row of a point to point run as the DATA row of no byte
    that opens the same message, and any other row as it is."""
    if row[0] == "M" and len(row) == 5:
        return ["R", row[1], row[2], row[3], 0, 0, row[4]]
    return row


def srpt(rows: list, fifo_every: int) -> dict:
    """Count the transmit decisions against SRPT outside the FIFO share.

    A decision is against SRPT when the message sent had more bytes left
    than another message that could have gone, and the FIFO share did not
    choose it. The share itself is checked too: the decisions the share took
    on one side stay within one in `fifo_every` of that side's decisions,
    plus one for the turn a count starts on.
    """
    decisions = {"A": 0, "B": 0}
    fifo = {"A": 0, "B": 0}
    against = []
    for row in rows:
        if row[0] != "T":
            continue
        _, at, side, rpc, left, by_fifo, could = row
        decisions[side] += 1
        if by_fifo:
            fifo[side] += 1
            continue
        fewest = min((bytes_left for _, bytes_left in could), default=left)
        if left > fewest:
            against.append({"at": at, "side": side, "rpc": rpc, "left": left, "fewest": fewest})
    over_share = {
        side: fifo[side] > (decisions[side] // fifo_every + 1 if fifo_every else 0)
        for side in decisions
    }
    return {
        "decisions": sum(decisions.values()),
        "fifo": sum(fifo.values()),
        "against": len(against),
        "examples": against[:5],
        "share_exceeded": [side for side, over in over_share.items() if over],
    }


class _Message:
    """A message as its receiver sees it: its length, what arrived and what
    it granted."""

    def __init__(self, msg_length: int):
        self.length = msg_length
        self.seen = bytearray(msg_length)
        self.received = 0
        self.granted = 0
        self.done = False

    def take(self, offset: int, length: int):
        end = min(offset + length, self.length)
        if end <= offset:
            return
        fresh = self.seen[offset:end].count(0)
        self.seen[offset:end] = b"\x01" * (end - offset)
        self.received += fresh

    def outstanding(self, unsched: int, all_or_nothing: bool = False) -> int:
        granted = min(max(self.granted, unscheduled(self.length, unsched, all_or_nothing)), self.length)
        return max(granted - self.received, 0)


def unscheduled(length: int, unsched: int, all_or_nothing: bool = False) -> int:
    """Return the bytes of a message of `length` bytes its sender sends without
    a grant: its first `unsched` bytes, or under all or nothing scheduling
    the whole message where it fits them and none where it does not."""
    if all_or_nothing:
        return length if length <= unsched else 0
    return min(unsched, length)


def grants(rows: list, unsched: dict, window: dict, max_incoming: dict,
           all_or_nothing: bool = False) -> dict:
    """Check the bytes granted and not received against `window` and the
    bytes a receiver expects against `max_incoming`.

    `unsched`, `window` and `max_incoming` map a receiving side, `A` or `B`,
    to its own values: a sender takes the receiver's unscheduled bytes, and
    the receiver's window and bound hold its grants. A message enters when
    its first DATA packet arrives, which is when the receiver learns of it,
    and leaves when the receiver reports it whole or the client RPC fails.
    `all_or_nothing` reads the unscheduled bytes of a message as that
    scheduling mode sends them.

    The window is checked at every packet taken and every GRANT sent. The
    bound is checked at every GRANT sent, since a receiver grants only while
    the bytes it expects, unscheduled or granted and not received, stay
    within `max_incoming`, while the unscheduled bytes of a message that
    starts reach it with no grant, whatever it expects. A receiver takes
    every packet that arrived at one time before it sends, so the GRANT that
    leaves at that time was decided before the first packet of a message that
    started at that time may have arrived, and the check at a GRANT leaves
    out the messages that started at its own time. The most a receiver
    expected at any row, every unscheduled byte included, is returned beside,
    with the rows it was above the bound.
    """
    messages = {}
    worst_window = {"A": 0, "B": 0}
    worst_incoming = {"A": 0, "B": 0}
    window_over = []
    incoming_over = []
    unscheduled_over = []
    live = {"A": set(), "B": set()}
    started = {}

    def expected(side, leave_out_started_at=None):
        return sum(
            messages[key].outstanding(unsched[side], all_or_nothing)
            for key in live[side]
            if leave_out_started_at is None or started.get(key) != leave_out_started_at
        )

    for row in map(_opened, rows):
        kind = row[0]
        if kind == "R":
            _, at, side, rpc, offset, length, msg_length = row
            key = (side, rpc)
            message = messages.get(key)
            if message is None or message.done or message.length != msg_length:
                message = _Message(msg_length)
                messages[key] = message
                live[side].add(key)
                started[key] = at
            message.take(offset, length)
        elif kind == "G":
            _, at, side, rpc, offset = row
            key = (side, rpc)
            message = messages.get(key)
            if message is None:
                continue
            message.granted = max(message.granted, offset)
        elif kind == "D":
            _, at, side, rpc, _length = row
            key = (side, rpc)
            if key in messages:
                messages[key].done = True
                live[side].discard(key)
            continue
        elif kind == "F":
            _, at, side, rpc, _why = row
            for key in (("A", rpc), ("B", rpc)):
                if key in messages:
                    messages[key].done = True
                    live[key[0]].discard(key)
            continue
        else:
            continue
        side = row[2]
        if key not in live[side]:
            continue
        held = message.outstanding(unsched[side], all_or_nothing)
        worst_window[side] = max(worst_window[side], held)
        if held > window[side]:
            window_over.append({"at": at, "side": side, "rpc": row[3], "held": held})
        total = expected(side)
        worst_incoming[side] = max(worst_incoming[side], total)
        if total > max_incoming[side]:
            unscheduled_over.append({"at": at, "side": side, "expected": total, "row": kind})
        if kind == "G":
            decided = expected(side, leave_out_started_at=at) + (
                message.outstanding(unsched[side], all_or_nothing) if started.get(key) == at else 0
            )
            if decided > max_incoming[side]:
                incoming_over.append({"at": at, "side": side, "expected": decided})
    return {
        "messages": len(messages),
        "worst_window": worst_window,
        "window_over": len(window_over),
        "window_examples": window_over[:5],
        "worst_incoming": worst_incoming,
        "incoming_over": len(incoming_over),
        "incoming_examples": incoming_over[:5],
        "expected_over_any_row": len(unscheduled_over),
        "expected_over_examples": unscheduled_over[:5],
    }


def grant_order(rows: list, unsched: dict, fifo_every: dict, all_or_nothing: bool = False,
                fifo_allowed: dict = None) -> dict:
    """Check that every GRANT goes to the one message the scheduler names.

    The scheduler of a downlink grants to one message at each decision, the
    one with the fewest bytes not yet received, ties to the older, or the
    oldest on a turn of the FIFO share. On a point to point link every
    receiver has one sender, so this is the plan's check of at most one
    granted message per sender, read per decision. A receiver decides after
    every DATA packet it takes and sends the GRANT when its link is ready, so
    a GRANT must name a message that SRPT or the FIFO share named at some
    DATA packet the receiver took since the last GRANT of that message.

    The trace does not say which decision was a turn of the share, so the
    share is checked by its count, as `srpt` checks the share of the sender.
    A turn comes once in `fifo_every` counted grants of a downlink, and a
    grant to the oldest on a turn is one of them, so the GRANTs a receiving
    side sent to the oldest message where SRPT named another stay within one
    in `fifo_every` of every GRANT that side sent, plus one as `srpt` allows,
    and none where `fifo_every` is 0. `fifo_every` maps a receiving side to
    the share of its own profile.

    A message enters the choice at its first DATA packet when it is longer
    than the receiver's unscheduled bytes `unsched`, and leaves it once it is
    whole, granted whole or its RPC fails. A decision may grant a message
    whole before its GRANT leaves, so each choice is also taken without the
    messages whose next GRANT grants them whole, and for each of those
    messages with it alone among them, since the decision that grants it
    whole may come after another such decision whose GRANT has not left. A
    GRANT whose message became
    whole after the decision leaves all the same and is judged by that
    decision, and a GRANT of a message the trace holds no DATA of is counted
    apart as not judged.

    `all_or_nothing` reads the unscheduled bytes of a message as that
    scheduling mode sends them, and `fifo_allowed`, where it is given, maps a
    receiving side to the decisions its FIFO share took by the `P` rows of a
    run of the full form, whose share runs by interval rather than by count,
    in place of the count of `fifo_every`.
    """
    rows = [_opened(row) for row in rows]
    full_at = {}
    lengths = {}
    for index, row in enumerate(rows):
        if row[0] == "R":
            lengths[(row[2], row[3])] = row[6]
        elif row[0] == "G":
            key = (row[2], row[3])
            full = key in lengths and row[4] >= lengths[key]
            full_at.setdefault(key, []).append((index, full))
    next_grant = {key: 0 for key in full_at}

    def whole_next(key, index):
        """Whether the next GRANT of `key` after row `index` grants it whole."""
        grants = full_at.get(key, [])
        at = next_grant.get(key, 0)
        while at < len(grants) and grants[at][0] < index:
            at += 1
        next_grant[key] = at
        return at < len(grants) and grants[at][1]

    messages = {}
    live = {"A": set(), "B": set()}
    judged = {"srpt": 0, "fifo": 0, "against": 0, "not_judged": 0}
    against = []
    side_grants = {"A": 0, "B": 0}
    side_fifo = {"A": 0, "B": 0}

    def end(key):
        if key in messages:
            messages[key].done = True
        live[key[0]].discard(key)

    for index, row in enumerate(rows):
        kind = row[0]
        if kind == "R":
            _, _at, side, rpc, offset, length, msg_length = row
            key = (side, rpc)
            message = messages.get(key)
            if message is None or message.done or message.length != msg_length:
                message = _Message(msg_length)
                message.age = index
                message.chosen = set()
                message.high = 0
                message.in_choice = unscheduled(msg_length, unsched[side], all_or_nothing) < msg_length
                messages[key] = message
                live[side].add(key)
            message.take(offset, length)
            message.high = max(message.high, min(offset + length, message.length))
            granted = max(message.granted, unscheduled(message.length, unsched[side], all_or_nothing),
                          message.high)
            if message.received >= message.length or granted >= message.length:
                message.in_choice = False
            choice = [k for k in live[side] if messages[k].in_choice]
            if not choice:
                continue
            open_choice = [k for k in choice if not whole_next(k, index)]
            pools = [choice, open_choice] + [
                [k] + open_choice for k in choice if whole_next(k, index)
            ]
            for pool in pools:
                if not pool:
                    continue
                srpt = min(
                    pool,
                    key=lambda k: (messages[k].length - messages[k].received, messages[k].age),
                )
                oldest = min(pool, key=lambda k: messages[k].age)
                messages[srpt].chosen.add("srpt")
                messages[oldest].chosen.add("fifo")
        elif kind == "G":
            _, at, side, rpc, offset = row
            key = (side, rpc)
            side_grants[side] += 1
            message = messages.get(key)
            if message is None:
                judged["not_judged"] += 1
                continue
            if "srpt" in message.chosen:
                judged["srpt"] += 1
            elif "fifo" in message.chosen:
                judged["fifo"] += 1
                side_fifo[side] += 1
            else:
                judged["against"] += 1
                against.append({"at": at, "side": side, "rpc": rpc, "offset": offset})
            message.chosen = set()
            message.granted = max(message.granted, offset)
            if offset >= message.length:
                message.in_choice = False
        elif kind == "S":
            _, _at, side, rpc, offset, length = row
            message = messages.get((side, rpc))
            if message is not None and not message.done:
                message.granted = max(message.granted, min(offset + length, message.length))
        elif kind == "D":
            end((row[2], row[3]))
        elif kind == "F":
            end(("A", row[3]))
            end(("B", row[3]))
    if fifo_allowed is not None:
        share = {side: fifo_allowed.get(side, 0) for side in side_grants}
    else:
        share = {
            side: side_grants[side] // fifo_every[side] + 1 if fifo_every[side] else 0
            for side in side_grants
        }
    return {
        "grants": judged["srpt"] + judged["fifo"] + judged["against"],
        "by_srpt": judged["srpt"],
        "by_fifo": judged["fifo"],
        "against": judged["against"],
        "not_judged": judged["not_judged"],
        "examples": against[:5],
        "fifo_share": sum(share.values()),
        "share_exceeded": [side for side in side_fifo if side_fifo[side] > share[side]],
    }


def granted_per_sender(rows: list, unsched: dict, all_or_nothing: bool = False) -> dict:
    """Return the most messages of one sender that held granted bytes not yet
    received beyond their unscheduled bytes when a GRANT left.

    On a point to point link every receiver has one sender, so the count is
    per receiving side. It is printed beside the grant checks, since SRPT
    lets a shorter message take the grants while an older one still waits for
    bytes it was granted. A message leaves the count once it is whole or its
    RPC fails, as in `grants`.
    """
    messages = {}
    most = {"A": 0, "B": 0}
    for row in map(_opened, rows):
        kind = row[0]
        if kind == "R":
            _, _at, side, rpc, offset, length, msg_length = row
            key = (side, rpc)
            message = messages.get(key)
            if message is None or message.done or message.length != msg_length:
                message = _Message(msg_length)
                messages[key] = message
            message.take(offset, length)
        elif kind == "D":
            key = (row[2], row[3])
            if key in messages:
                messages[key].done = True
        elif kind == "F":
            for key in (("A", row[3]), ("B", row[3])):
                if key in messages:
                    messages[key].done = True
        elif kind == "G":
            _, _at, side, rpc, offset = row
            key = (side, rpc)
            if key in messages:
                messages[key].granted = max(messages[key].granted, offset)
            holding = sum(
                1
                for (s, _), m in messages.items()
                if s == side
                and not m.done
                and m.granted > unscheduled(m.length, unsched[side], all_or_nothing)
                and m.granted > m.received
            )
            most[side] = max(most[side], holding)
    return most


def counts(rows: list) -> dict:
    """Return how many rows of each kind a trace holds."""
    out = {}
    for row in rows:
        out[row[0]] = out.get(row[0], 0) + 1
    return out


def decision_share(rows: list) -> dict:
    """Return the decisions of the FIFO share each receiving side took, by
    the `P` rows of a run of the full form, whose share runs by interval
    rather than by count."""
    out = {"A": 0, "B": 0}
    for row in rows:
        if row[0] == "P" and row[9]:
            out[row[2]] = out.get(row[2], 0) + 1
    return out


def _fabric_order(rows: list) -> list:
    """Return the rows of a fabric trace with the row of every packet ahead
    of the decisions it caused."""
    out = []
    held = []
    for row in rows:
        if row[0] == "P":
            if held and (held[-1][1], held[-1][2]) != (row[1], row[2]):
                out.extend(held)
                held = []
            held.append(row)
            continue
        if held and row[0] in ("R", "M") and (row[1], row[2]) == (held[0][1], held[0][2]):
            out.append(row)
            out.append(("group",))
            out.extend(held)
            held = []
            continue
        if held:
            out.append(("group",))
            out.extend(held)
            held = []
        out.append(row)
    if held:
        out.append(("group",))
        out.extend(held)
    return out


class _Incoming:
    """A message as its receiver on a fabric sees it: its length, the bytes
    it took, deduplicated by offset, and the offset granted."""

    __slots__ = ("length", "offsets", "received", "granted")

    def __init__(self, length: int, granted: int):
        self.length = length
        self.offsets = {}
        self.received = 0
        self.granted = granted

    def take(self, offset: int, length: int):
        if offset not in self.offsets:
            self.offsets[offset] = length
            self.received += length

    def owed(self) -> int:
        return max(min(self.granted, self.length) - self.received, 0)


def fabric(rows: list, levels: int, max_incoming: int, unsched: int, homa_grant_priority) -> dict:
    """Check the grants of a fabric run of all or nothing scheduling.

    - Every decision the grant scheduler of a host took at one packet or one
      timer grants to at most one message a sender outside the FIFO share,
      which is the plan's check of 0 grants to more than one message per
      sender, read per decision.
    - The level of every decision equals `homa_grant_priority(rank, active,
      max_sched)` with the highest scheduled level of `levels`, and the
      priority of every GRANT a host sent equals the level of the last
      decision for its message.
    - The bytes a host expects, granted and not received over every message
      it takes, stay within `max_incoming` at every decision, the grant of
      the decision included. A message enters at its first packet: with the
      whole message granted where that packet is DATA, which under all or
      nothing scheduling sends it whole unscheduled, and with none where it
      is `START_MSG`. `unsched` bounds the first case, and a message longer
      than it that opens with DATA is counted apart. A message leaves once it
      is whole or its RPC failed at the client.

    `homa_grant_priority` is the reference's function of rank and active
    messages, passed in so that this module stays free of the reference.
    """
    max_sched = levels // 2 - 1 if levels >= 2 else 0
    messages = {}
    last_level = {}
    group = []
    result = {
        "decisions": 0,
        "fifo": 0,
        "groups": 0,
        "sender_twice": 0,
        "sender_examples": [],
        "level_mismatch": 0,
        "level_examples": [],
        "grants": 0,
        "grant_level_mismatch": 0,
        "grant_examples": [],
        "grants_without_decision": 0,
        "worst_expected": 0,
        "expected_over": 0,
        "expected_examples": [],
        "unscheduled_too_long": 0,
    }

    def close(group):
        if not group:
            return
        result["groups"] += 1
        senders = [row[10] for row in group if not row[9]]
        if len(senders) != len(set(senders)):
            result["sender_twice"] += 1
            if len(result["sender_examples"]) < 5:
                result["sender_examples"].append(group[:4])

    expected = {}

    def settle(host, message, before):
        expected[host] = expected.get(host, 0) - before + message.owed()

    def leave(key):
        message = messages.pop(key, None)
        if message is not None:
            expected[key[0]] = expected.get(key[0], 0) - message.owed()

    for row in _fabric_order(rows):
        kind = row[0]
        if kind != "P":
            close(group)
            group = []
        if kind == "group":
            continue
        if kind == "R":
            _, _at, host, rpc, offset, length, msg_length, _from = row
            key = (host, rpc)
            message = messages.get(key)
            if message is None or message.length != msg_length:
                leave(key)
                if msg_length > unsched:
                    result["unscheduled_too_long"] += 1
                message = _Incoming(msg_length, msg_length)
                messages[key] = message
                settle(host, message, 0)
            before = message.owed()
            message.take(offset, length)
            settle(host, message, before)
        elif kind == "M":
            _, _at, host, rpc, msg_length, _from = row
            key = (host, rpc)
            if key not in messages or messages[key].length != msg_length:
                leave(key)
                messages[key] = _Incoming(msg_length, 0)
        elif kind == "P":
            _, at, host, rpc, offset, level, rank, active, _window, fifo, _sender = row
            group.append(row)
            result["decisions"] += 1
            result["fifo"] += 1 if fifo else 0
            want = homa_grant_priority(rank, active, max_sched)
            if level != want:
                result["level_mismatch"] += 1
                if len(result["level_examples"]) < 5:
                    result["level_examples"].append({"row": row, "want": want})
            key = (host, rpc)
            last_level[key] = level
            message = messages.get(key)
            if message is not None:
                before = message.owed()
                message.granted = max(message.granted, offset)
                settle(host, message, before)
            held = expected.get(host, 0)
            result["worst_expected"] = max(result["worst_expected"], held)
            if held > max_incoming:
                result["expected_over"] += 1
                if len(result["expected_examples"]) < 5:
                    result["expected_examples"].append({"at": at, "host": host, "expected": held})
        elif kind == "G":
            _, _at, host, rpc, _offset, priority, _to = row
            result["grants"] += 1
            key = (host, rpc)
            if key not in last_level:
                result["grants_without_decision"] += 1
            elif priority != last_level[key]:
                result["grant_level_mismatch"] += 1
                if len(result["grant_examples"]) < 5:
                    result["grant_examples"].append({"row": row, "level": last_level[key]})
        elif kind in ("D", "F"):
            leave((row[2], row[3]))
    close(group)
    return result


def decisions(rows: list, unsched: dict, max_incoming: dict, levels: int, homa_grant_priority,
              all_or_nothing: bool = False) -> dict:
    """Check the decisions of the grant schedulers of a point to point run of
    the full form, its `P` rows.

    - Every decision outside the FIFO share grants its message at most
      `window` bytes beyond what it received, the window the decision names,
      and that window is `max_incoming` divided by one more than the messages
      granted at once, the dynamic window of Network. A decision of the FIFO
      share grants `fifo_grant_increment` bytes more whatever the window, as
      HomaModule's does, and is counted apart.
    - The level of every decision equals `homa_grant_priority(rank, active,
      max_sched)` with the highest scheduled level of `levels`.
    - The bytes a side expects, granted and not received over every message
      it takes, stay within `max_incoming` at every decision, the grant of
      the decision included, as `fabric` checks them.

    A side writes the decisions a packet caused before the row of that
    packet, so the row of the packet is taken first.
    """
    max_sched = levels // 2 - 1 if levels >= 2 else 0
    ordered = []
    held = []
    for row in map(_opened, rows):
        if row[0] == "P":
            if held and (held[-1][1], held[-1][2]) != (row[1], row[2]):
                ordered.extend(held)
                held = []
            held.append(row)
            continue
        if held and row[0] == "R" and (row[1], row[2]) == (held[0][1], held[0][2]):
            ordered.append(row)
            ordered.extend(held)
            held = []
            continue
        ordered.extend(held)
        held = []
        ordered.append(row)
    ordered.extend(held)
    messages = {}
    result = {
        "decisions": 0,
        "fifo": 0,
        "window_over": 0,
        "window_examples": [],
        "window_formula": 0,
        "worst_beyond": 0,
        "level_mismatch": 0,
        "level_examples": [],
        "worst_expected": {"A": 0, "B": 0},
        "expected_over": 0,
        "expected_examples": [],
    }
    for row in ordered:
        kind = row[0]
        if kind == "R":
            _, _at, side, rpc, offset, length, msg_length = row
            key = (side, rpc)
            message = messages.get(key)
            if message is None or message.done or message.length != msg_length:
                message = _Message(msg_length)
                messages[key] = message
            message.take(offset, length)
        elif kind == "P":
            _, at, side, rpc, offset, level, rank, active, window, fifo = row[:10]
            result["decisions"] += 1
            message = messages.get((side, rpc))
            want = homa_grant_priority(rank, active, max_sched)
            if level != want:
                result["level_mismatch"] += 1
                if len(result["level_examples"]) < 5:
                    result["level_examples"].append({"row": row, "want": want})
            if message is not None:
                message.granted = max(message.granted, offset)
            if fifo:
                result["fifo"] += 1
            else:
                if window != max_incoming[side] // (active + 1):
                    result["window_formula"] += 1
                beyond = offset - (message.received if message is not None else 0)
                result["worst_beyond"] = max(result["worst_beyond"], beyond)
                if beyond > window:
                    result["window_over"] += 1
                    if len(result["window_examples"]) < 5:
                        result["window_examples"].append({"row": row, "beyond": beyond})
            total = sum(
                m.outstanding(unsched[s], all_or_nothing)
                for (s, _), m in messages.items()
                if s == side and not m.done
            )
            result["worst_expected"][side] = max(result["worst_expected"][side], total)
            if total > max_incoming[side]:
                result["expected_over"] += 1
                if len(result["expected_examples"]) < 5:
                    result["expected_examples"].append({"at": at, "side": side, "expected": total})
        elif kind == "D":
            key = (row[2], row[3])
            if key in messages:
                messages[key].done = True
        elif kind == "F":
            for key in (("A", row[3]), ("B", row[3])):
                if key in messages:
                    messages[key].done = True
    return result
