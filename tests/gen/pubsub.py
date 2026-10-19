# SPDX-License-Identifier: MIT
"""tests/gen: the pub sub, repeater and hub scripts of `40_pubsub`, drawn from a seed.

Three kinds of script, each drawn from a stream of its own of `rng.py`, so a
kind added later leaves the draws of the others where they are.

- **Pub sub scripts**, in the language of `tests/sidecar/pubsub.md`, which
  `aosabi-devtool pubsub` reads as well: the topic rule and the payload rule of
  `publish` with the nullish payload quirk, the listener a subscription adds
  only once it is pulled, the order per subscriber, `return` removing the
  listener, several subscribers, and ids as strings, numbers and booleans.
- **Repeater scripts** in the same language: a push before and after a pull, a
  stop with and without an error, a throw before and after a push, `return`,
  `throw`, and the overflow at the 1025th pending push and at the 1025th
  pending `next`. A push overflow throws inside the executor, so the script
  pulls past the 1024 queued values to the call that observes the error.
- **Hub op lists** for `tests/harness/push_model.py` and `aosabi-devtool push`,
  over the depths 1, 2, 8 and 1024, with the allowlist and a restart among the
  ops.

A pub sub script never leaves 1024 pushes pending on one subscriber, because the
reference ends its process there, which the overflow probe of the sidecar
records apart. A script's id is the SHA-256 of its kind and its canonical steps,
and the id of a hub op list is the SHA-256 of its kind, its depth and its ops.
"""

from __future__ import annotations

from .cases import case_id
from .rng import Rng

#: The topics a pub sub script draws from, the two of `pubsub-app` and a third.
TOPICS = ("changed", "changedIn", "tick")

#: The ids a keyed topic is drawn with: strings, numbers and booleans.
IDS = ("playback", "queue", "button", 0, 7, -1, 2.5, True, False)

#: The payloads a publish or a push is drawn with, the marker of `undefined`
#: among them.
PAYLOADS = (
    {"section": "playback", "revision": 1},
    {"section": "queue"},
    "button",
    0,
    42,
    True,
    None,
    [],
    {"$undefined": True},
)

#: The depths of the hub op lists.
HUB_DEPTHS = (1, 2, 8, 1024)

#: The carriers of the allowlist ops, by the name `push_model.py` reads.
CARRIERS = ("network", "unix_socket", "usb", "bluetooth", "serial_line", "lora")

#: The fields an allowlist op asks about.
FIELDS = ("changes", "changesIn", "frames", "ticks")

#: The payloads of a hub op at the ends of the `u32` the devtool carries.
PAYLOAD_EDGES = (0, (1 << 32) - 1)

#: The pushes that fill the queue of a repeater, one more than it holds.
OVERFLOW = 1025

#: The pulls of a push overflow: the 1024 queued values, the call that observes
#: the overflow error and the call that observes done.
OVERFLOW_PULLS = OVERFLOW + 1

DEFAULT_PUBSUB_SCRIPTS = 200
DEFAULT_REPEATER_SCRIPTS = 200
DEFAULT_HUB_OP_LISTS = 2000


def _request(kind: str, steps: list) -> dict:
    return {"id": case_id({"kind": kind, "steps": steps}), "op": "script", "steps": steps}


def pubsub_script(rng: Rng, index: int) -> dict:
    """One pub sub script, subscribers on drawn topics and publishes of drawn arguments."""
    stream = rng.stream("pubsub.script")
    steps: list = [{"create": "p"}]
    subscribers = []
    for number in range(stream.randint(1, 3)):
        name = "s%d" % number
        step = {"subscribe": name, "pubsub": "p", "key": stream.choice(TOPICS)}
        if stream.random() < 0.5:
            step["id"] = stream.choice(IDS)
        steps.append(step)
        subscribers.append(name)
    pending = {name: 0 for name in subscribers}
    for _ in range(stream.randint(2, 12)):
        roll = stream.random()
        name = stream.choice(subscribers)
        if roll < 0.3:
            count = stream.randint(1, 3)
            steps.append({"pull": name, "count": count})
            pending[name] += count
        elif roll < 0.8:
            args = [stream.choice(PAYLOADS)]
            if stream.random() < 0.5:
                args = [stream.choice(IDS), stream.choice(PAYLOADS)]
            steps.append({"publish": "p", "key": stream.choice(TOPICS), "args": args})
        elif roll < 0.9:
            steps.append({"return": name})
        else:
            steps.append({"settle": True})
    return _request("pubsub", steps)


def repeater_script(rng: Rng, index: int) -> dict:
    """One repeater script, an executor of drawn steps and drawn calls on it.

    Every twentieth script is an overflow: 1025 pushes that are not awaited
    before the first pull, or 1025 pulls on an executor that never pushes. The
    1025th push throws inside the executor, whose rejection stops the repeater
    only after the 1024 queued values, so the push overflow pulls 1026 times
    and observes the values, the overflow error and done. A pull of 1024 or
    fewer would answer the same bytes on a repeater with no bound.
    """
    stream = rng.stream("repeater.script")
    if index % 20 == 19:
        if (index // 20) % 2 == 0:
            inner = [{"push": value % 7, "await": False} for value in range(OVERFLOW)]
            return _request("repeater", [{"repeater": "r", "script": inner + [{"stop": None}]},
                                         {"pull": "r", "count": OVERFLOW_PULLS}])
        return _request("repeater", [{"repeater": "r", "script": []}, {"pull": "r", "count": OVERFLOW}])
    inner = []
    for _ in range(stream.randint(0, 6)):
        roll = stream.random()
        if roll < 0.55:
            step = {"push": stream.choice(PAYLOADS)}
            if stream.random() < 0.3:
                step["await"] = False
            inner.append(step)
        elif roll < 0.7:
            inner.append({"sleep": stream.randint(0, 3)})
        elif roll < 0.8:
            inner.append({"tick": True})
        elif roll < 0.9:
            inner.append({"stop": stream.choice((None, "stopped with a reason"))})
        else:
            inner.append({"throw": "thrown by the executor"})
    if stream.random() < 0.7:
        inner.append({"stop": None})
    steps: list = [{"repeater": "r", "script": inner}]
    for _ in range(stream.randint(1, 5)):
        roll = stream.random()
        if roll < 0.6:
            steps.append({"pull": "r", "count": stream.randint(1, 3)})
        elif roll < 0.75:
            steps.append({"return": "r"})
        elif roll < 0.9:
            steps.append({"throw_into": "r", "error": "thrown into the repeater"})
        else:
            steps.append({"settle": True})
    return _request("repeater", steps)


def hub_op_list(rng: Rng, index: int) -> dict:
    """One op list of the hub at the depth of its index, cycling through the depths."""
    stream = rng.stream("hub.ops")
    depth = HUB_DEPTHS[index % len(HUB_DEPTHS)]
    topics = TOPICS[: stream.randint(1, len(TOPICS))]
    ops: list = []
    slots = 0
    for _ in range(stream.randint(1, 4)):
        ops.append({"op": "listen", "topic": stream.choice(topics), "depth": depth})
        slots += 1
    burst = depth + 3 if depth == 1024 and index % 500 == 3 else 0
    for payload in range(burst):
        ops.append({"op": "publish", "topic": topics[0], "payload": payload})
    for _ in range(stream.randint(4, 16)):
        roll = stream.random()
        slot = stream.randrange(slots + 1)
        if roll < 0.35:
            payload = stream.choice(PAYLOAD_EDGES) if stream.random() < 0.05 else stream.randrange(1000)
            ops.append({"op": "publish", "topic": stream.choice(topics), "payload": payload})
        elif roll < 0.6:
            ops.append({"op": "take", "slot": slot, "waker": stream.random() < 0.5})
        elif roll < 0.68:
            ops.append({"op": "listen", "topic": stream.choice(topics), "depth": depth})
            slots += 1
        elif roll < 0.74:
            ops.append({"op": "unlisten", "slot": slot})
        elif roll < 0.84:
            ops.append({"op": "state", "slot": slot} if stream.random() < 0.7 else {"op": "state"})
        elif roll < 0.97:
            op = {"op": "allow", "carrier": stream.choice(CARRIERS), "field": stream.choice(FIELDS)}
            if stream.random() < 0.6:
                op["only"] = list(stream.sample(FIELDS, stream.randint(0, 2)))
            ops.append(op)
        else:
            ops.append({"op": "restart"})
            slots = 0
    return {"id": case_id({"kind": "hub", "depth": depth, "ops": ops}), "depth": depth, "ops": ops}


def pubsub_scripts(seed: int = 1, count: int = DEFAULT_PUBSUB_SCRIPTS) -> list[dict]:
    """`count` pub sub scripts drawn from `seed`."""
    rng = Rng(seed)
    return [pubsub_script(rng, index) for index in range(count)]


def repeater_scripts(seed: int = 1, count: int = DEFAULT_REPEATER_SCRIPTS) -> list[dict]:
    """`count` repeater scripts drawn from `seed`."""
    rng = Rng(seed)
    return [repeater_script(rng, index) for index in range(count)]


def hub_op_lists(seed: int = 1, count: int = DEFAULT_HUB_OP_LISTS) -> list[dict]:
    """`count` hub op lists drawn from `seed`, a quarter at each depth."""
    rng = Rng(seed)
    return [hub_op_list(rng, index) for index in range(count)]
