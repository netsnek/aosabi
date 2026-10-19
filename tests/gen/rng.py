# SPDX-License-Identifier: MIT
"""tests/gen: the seeded random source of the operation generator.

Every draw the generator makes names a purpose, and a purpose owns a stream of
its own that is derived from the seed and from the purpose text. A family that
is added later therefore draws from a new stream and leaves the numbers of the
families that already exist where they are, which is what keeps a recorded
corpus count stable across a change.
"""

from __future__ import annotations

import hashlib
import random


class Rng:
    """Holds one deterministic stream per purpose, all derived from one seed.

    `seed` is the integer a run records in its manifest. A purpose is a short
    text such as `"S3.fanout"`, and the stream it names is created on first
    use and kept, so two draws of the same purpose continue one sequence.
    """

    def __init__(self, seed: int) -> None:
        self.seed = int(seed)
        self._streams: dict[str, random.Random] = {}

    def stream(self, purpose: str) -> random.Random:
        """Returns the stream of `purpose`, creating it on first use.

        The state is seeded with the SHA-256 of the seed and the purpose text,
        so the stream of one purpose does not depend on how often another
        purpose was drawn from.
        """
        stream = self._streams.get(purpose)
        if stream is not None:
            return stream
        digest = hashlib.sha256(f"{self.seed}:{purpose}".encode()).digest()
        stream = random.Random(int.from_bytes(digest, "big"))
        self._streams[purpose] = stream
        return stream

    def choice(self, purpose: str, items):
        """Returns one element of `items`, drawn from the stream of `purpose`."""
        return self.stream(purpose).choice(list(items))

    def pick(self, purpose: str, items, count: int) -> list:
        """Returns `count` elements of `items` in their original order.

        The draw is without replacement, and the result keeps the order the
        input carries, so a capped selection prints in schema order rather
        than in draw order.
        """
        pool = list(items)
        if count >= len(pool):
            return pool
        indexes = sorted(self.stream(purpose).sample(range(len(pool)), count))
        return [pool[index] for index in indexes]
