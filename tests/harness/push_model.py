# SPDX-License-Identifier: MIT
"""Aosabi harness: the push model, the rules of the kernel's publish module as code a notebook runs.

The kernel's `publish` module has no reference, so its contract is the record of
ADR-0017, which is the exception ADR-0008 grants every surface with no
counterpart in the reference, and this module is where that contract is written
as code. `aosabi-devtool push` runs the hub of `aosabi-kernel` over the same op
lists with `u32` payloads and writes the same lines, so the notebook `40_pubsub`
compares the two line by line. The model covers the hub and the allowlist alone,
because the section queue, its sessions and `take_first` are withdrawn.

The rules, in the words of the plan, section KERNEL:

1. A publish increments the revision once, walks the slots in listen order, and
   every slot of the topic takes the payload with the new revision.
2. A slot keeps at most `max(depth, 1)` pending events in publish order. At the
   bound the newest pending event is replaced by the new one and the slot's
   `coalesced` goes up by one, so a slow subscriber loses an intermediate value
   and never the latest one.
3. A slot that was empty and holds a waker gives the waker to the publish, which
   wakes it, and the slot holds no waker after that.
4. A slot listened or unlistened during a publish is not reached by it, which
   the one thread of this model meets by construction.
5. A publish to a topic nobody listens on increments the revision and does
   nothing else.

A request is `{"id": <any>, "ops": [...]}`, and every op writes one line
`{"id", "n", "op", ...}` with `n` counting the ops of the request from 1. A
waker is a flag of the slot here and a `Waker` in the kernel.

| Op | Fields | The line after `op` |
| --- | --- | --- |
| `listen` | `topic`, `depth` | `slot`, the id, from 0 in listen order |
| `unlisten` | `slot` | `slot`, `waker`, whether the slot held a waker |
| `publish` | `topic`, `payload` | `revision`, `reached`, `coalesced`, `woken`, the slots woken in listen order |
| `take` | `slot`, `waker` | `slot`, `take` as `ready` with `revision` and `payload`, `empty` or `gone` |
| `state` | `slot` or nothing | `slot`, `pending` and `coalesced`, or `slot` and `gone`, or `revision` and `slots` |
| `allow` | `carrier`, `only` or nothing, `field` | `carrier`, `field`, `allowed`, and `refusal` when refused |
| `restart` | nothing | `revision` and `slots`, both 0 |

A `take` with `waker` true on an empty slot stores the waker in place of any
waker the slot held, and one with `waker` false leaves the stored waker as it
is. An op that breaks this grammar writes `fatal` with its reason, and the ops
after it are not run.
"""

from __future__ import annotations

import json

#: The carriers of ADR-0023 in their order, by the name an op spells and the
#: proper name a refusal writes.
CARRIERS = {
    "network": "Network",
    "unix_socket": "Unix Socket",
    "usb": "USB",
    "bluetooth": "Bluetooth",
    "serial_line": "Serial Line",
    "lora": "LoRa",
}

#: The depth of `CarrierPolicy::NETWORK`, the queue bound of the reference
#: repeater.
NETWORK_DEPTH = 1024

U16 = (1 << 16) - 1
U32 = (1 << 32) - 1


class Slot:
    """One listener: its topic, its bound, its pending events and its waker."""

    def __init__(self, ident: int, topic: str, depth: int):
        self.id = ident
        self.topic = topic
        self.depth = max(depth, 1)
        self.pending: list[tuple[int, int]] = []
        self.coalesced = 0
        self.waker = False


class Hub:
    """The fan out: a revision and one slot per listener in listen order."""

    def __init__(self):
        self.revision = 0
        self.next_id = 0
        self.slots: list[Slot] = []

    def find(self, ident: int) -> Slot | None:
        return next((slot for slot in self.slots if slot.id == ident), None)

    def listen(self, topic: str, depth: int) -> int:
        slot = Slot(self.next_id, topic, depth)
        self.next_id += 1
        self.slots.append(slot)
        return slot.id

    def unlisten(self, ident: int) -> bool:
        slot = self.find(ident)
        if slot is None:
            return False
        self.slots.remove(slot)
        return slot.waker

    def publish(self, topic: str, payload: int) -> dict:
        self.revision += 1
        reached = coalesced = 0
        woken = []
        for slot in list(self.slots):
            if slot.topic != topic:
                continue
            reached += 1
            if len(slot.pending) >= slot.depth:
                slot.pending[-1] = (self.revision, payload)
                slot.coalesced += 1
                coalesced += 1
                continue
            if not slot.pending and slot.waker:
                slot.waker = False
                woken.append(slot.id)
            slot.pending.append((self.revision, payload))
        return {"revision": self.revision, "reached": reached, "coalesced": coalesced, "woken": woken}

    def take(self, ident: int, waker: bool) -> dict:
        slot = self.find(ident)
        if slot is None:
            return {"slot": ident, "take": "gone"}
        if not slot.pending:
            if waker:
                slot.waker = True
            return {"slot": ident, "take": "empty"}
        revision, payload = slot.pending.pop(0)
        return {"slot": ident, "take": "ready", "revision": revision, "payload": payload}


def refusal(field: str, carrier: str) -> str:
    """The text `write_refusal` writes for a field refused on a carrier."""
    return 'Subscription field "%s" is refused on %s.' % (field, carrier)


def _integer(op: dict, key: str, bound: int) -> int:
    value = op.get(key)
    if not isinstance(value, int) or isinstance(value, bool) or not 0 <= value <= bound:
        raise ValueError("%s is no integer from 0 to %d" % (key, bound))
    return value


def _text(op: dict, key: str) -> str:
    value = op.get(key)
    if not isinstance(value, str):
        raise ValueError("%s is no string" % key)
    return value


def apply(hub: Hub, op: dict) -> tuple[Hub, dict]:
    """Runs one op on `hub` and returns the hub after it and the fields of its line."""
    name = op.get("op")
    if name == "listen":
        return hub, {"slot": hub.listen(_text(op, "topic"), _integer(op, "depth", U16))}
    if name == "unlisten":
        ident = _integer(op, "slot", U32)
        return hub, {"slot": ident, "waker": hub.unlisten(ident)}
    if name == "publish":
        return hub, hub.publish(_text(op, "topic"), _integer(op, "payload", U32))
    if name == "take":
        waker = op.get("waker")
        if not isinstance(waker, bool):
            raise ValueError("waker is no boolean")
        return hub, hub.take(_integer(op, "slot", U32), waker)
    if name == "state":
        if "slot" not in op:
            return hub, {"revision": hub.revision, "slots": len(hub.slots)}
        ident = _integer(op, "slot", U32)
        slot = hub.find(ident)
        if slot is None:
            return hub, {"slot": ident, "gone": True}
        return hub, {"slot": ident, "pending": len(slot.pending), "coalesced": slot.coalesced}
    if name == "allow":
        carrier = CARRIERS.get(op.get("carrier"))
        if carrier is None:
            raise ValueError("carrier is none of %s" % ", ".join(CARRIERS))
        field = _text(op, "field")
        only = op.get("only")
        if only is not None and not (isinstance(only, list) and all(isinstance(one, str) for one in only)):
            raise ValueError("only is no list of strings")
        line = {"carrier": carrier, "field": field, "allowed": only is None or field in only}
        if not line["allowed"]:
            line["refusal"] = refusal(field, carrier)
        return hub, line
    if name == "restart":
        return Hub(), {"revision": 0, "slots": 0}
    raise ValueError("unknown op %s" % json.dumps(name))


def run(request: dict) -> list[dict]:
    """Runs one op list on a fresh hub and returns one line object per op run."""
    hub = Hub()
    lines = []
    for n, op in enumerate(request.get("ops") or [], start=1):
        head = {"id": request.get("id"), "n": n, "op": op.get("op") if isinstance(op, dict) else None}
        try:
            if not isinstance(op, dict):
                raise ValueError("the op is no object")
            hub, fields = apply(hub, op)
        except ValueError as error:
            lines.append({**head, "fatal": str(error)})
            break
        lines.append({**head, **fields})
    return lines


def lines(request: dict) -> list[str]:
    """The lines of one op list as the devtool writes them, compact JSON."""
    return [json.dumps(line, separators=(",", ":")) for line in run(request)]
