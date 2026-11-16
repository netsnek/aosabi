# SPDX-License-Identifier: MIT
"""Aosabi harness: the named masks a comparison applies to both sides.

The list of normalizations is closed: compressed bodies are compared after
decompression, which tests/harness/http.py does, and clocks and generated
identifiers are masked by name. Each mask here is one such name with its regular
expressions over bytes and its replacement, and a golden that a mask touched lists
the mask in its manifest, so the reader of a golden sees which bytes were replaced
and by what rule.

The masks of the closed list are `time`, `durationMs`, `request-id`, `snowflake`,
`sentryEventId`, `x-yoga-id` and `keepalive-arrival`. The mask `time` carries a
second expression for the pretty prefix `HH:MM:SS.mmm`, the same clock. The mask
`date` replaces the value of `Date` in a head, which a comparison ignores by
name, so a recorded head keeps its line and loses its clock. The mask `err-stack`
replaces every line after the first of a `stack` in a log record and in a pretty
line, and is applied to log lines alone and never to a response. In a pretty text
it replaces the run of frame lines and keeps the line end of the last, so the
line after a stack stays a line of its own and the masks give the same bytes in
any order. The function `mask_paths` replaces machine paths in a recorded log,
because no machine path enters a committed file, and is no normalization of a
comparison.

Two masks are of the class of generated identifiers the list already holds:
`sentry-event-id`, the Sentry event id in the extensions of an answer and in the
`event_id` of an envelope, and `boot-port`, the port of the listen line of a
boot, which each side was given. The mask `boot-port-aosabi` replaces the port in
the listen line of the side under test, which names Aosabi where the reference
names Pylon, so `boot-port` keeps the bytes the goldens of the reference record.

Four masks of the same class each replace a value a side draws per request:
`pkce-verifier`, the verifier of the cookie `aosabi_code_verifier` and the
`code_verifier` of a token body, `pkce-challenge`, the `code_challenge` of a
`Location`, and `oidc-state`, its `state`, each 43 characters of `[A-Za-z0-9_-]`,
the base64url of 32 random bytes with no padding, and `client-assertion`, the
compact JWS a client authenticates with in a token or an introspection body. A
mask replaces a value of exactly that shape and no other, so a verifier of the
wrong length stays visible as a difference. The assertion is masked only after
tests/harness/mock_issuer.py decoded it, compared its header and its claims and
verified its signature, which `assertion_report` there answers.

Two masks replace what each side picks on its own. `federation-url` replaces the
URL of a remote inside a message or an extension, and the bare address of the
loopback a refused connection names, because each side reaches its remotes
through proxies on ports of its own. `formdata-boundary` replaces the boundary of
a multipart request in its head and in every delimiter line of its body, because
the Fetch implementation of each side draws its own.
"""

from __future__ import annotations

import os
import re
from dataclasses import dataclass


@dataclass(frozen=True)
class Mask:
    """One named mask: its expressions over bytes, the replacement and where it applies."""

    name: str
    patterns: tuple[bytes, ...]
    replacement: bytes
    applies_to: str
    rule: str

    def apply(self, data: bytes) -> bytes:
        """`data` with every match of every expression replaced."""
        for pattern in self.patterns:
            data = re.sub(pattern, self.replacement, data)
        return data

    def manifest(self) -> dict:
        """The entry a golden's manifest carries for this mask."""
        return {
            "name": self.name,
            "patterns": [pattern.decode("latin-1") for pattern in self.patterns],
            "replacement": self.replacement.decode("latin-1"),
            "applies_to": self.applies_to,
            "rule": self.rule,
        }


MASKS = {
    mask.name: mask
    for mask in (
        Mask(
            "date",
            (rb"(?<=\r\nDate: )[A-Z][a-z]{2}, [0-9]{2} [A-Z][a-z]{2} [0-9]{4} [0-9]{2}:[0-9]{2}:[0-9]{2} GMT(?=\r\n)",
             rb"(?i)(?<=\r\ndate: )[A-Z][a-z]{2}, [0-9]{2} [A-Z][a-z]{2} [0-9]{4} [0-9]{2}:[0-9]{2}:[0-9]{2} GMT(?=\r\n)"),
            b"<date>",
            "head",
            "the value of Date in a head",
        ),
        Mask(
            "time",
            (rb'(?<="time":)[0-9]+', rb"(?m)^[0-9]{2}:[0-9]{2}:[0-9]{2}\.[0-9]{3}(?= )"),
            b"<time>",
            "log",
            "the field time of a record and the pretty prefix HH:MM:SS.mmm",
        ),
        Mask(
            "durationMs",
            (rb'(?<="durationMs":)[0-9]+', rb"(?<=durationMs=)[0-9]+"),
            b"<n>",
            "log",
            "the field durationMs of an access line",
        ),
        Mask(
            "request-id",
            (rb"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}",),
            b"<request-id>",
            "log",
            "a request id a side generated because the fixture sent none",
        ),
        Mask(
            "snowflake",
            (rb'(?<="id":")[0-9]{17,19}(?=")',),
            b"<snowflake>",
            "body",
            "a snowflake local id",
        ),
        Mask(
            "sentryEventId",
            (rb'(?<="sentryEventId":")[0-9a-f]{32}(?=")',),
            b"<sentry-event-id>",
            "body",
            "the Sentry event id",
        ),
        Mask(
            "x-yoga-id",
            (rb"(?i)(?<=\r\nx-yoga-id: )[^\r\n]*",),
            b"<x-yoga-id>",
            "head",
            "the Yoga instance id header, whose value the capture records as the constant yoga",
        ),
        Mask(
            "keepalive-arrival",
            (rb'(?<="at_ms":)[0-9]+',),
            b"<ms>",
            "stream",
            "the arrival time of a keepalive frame",
        ),
        Mask(
            "sentry-event-id",
            (rb'(?<="sentryEventId":")[0-9a-f]{32}(?=")', rb'(?<="event_id":")[0-9a-f]{32}(?=")'),
            b"<sentry-event-id>",
            "body",
            "the Sentry event id of extensions.sentryEventId and of the event_id of an envelope",
        ),
        Mask(
            "boot-port",
            (rb"(?<=Pylon running at http://localhost:)[0-9]+",),
            b"<port>",
            "log",
            "the port of the listen line of a boot",
        ),
        Mask(
            "boot-port-aosabi",
            (rb"(?<=Aosabi running at http://localhost:)[0-9]+",),
            b"<port>",
            "log",
            "the port of the listen line of a boot of the side under test, which names Aosabi",
        ),
        Mask(
            "err-stack",
            (rb'(?:\\n\s*at (?:[^"\\]|\\[^n])*)+', rb"(?m)^[ \t]+at .*(?:\n[ \t]+at .*)*"),
            b"<err-stack>",
            "log",
            "every line after the first of a stack, in a record and in a pretty line",
        ),
        Mask(
            "pkce-verifier",
            (rb"(?<=code_verifier=)[A-Za-z0-9_-]{43}(?![A-Za-z0-9_-])",),
            b"<pkce-verifier>",
            "head and outbound body",
            "the cookie aosabi_code_verifier and the code_verifier of a token body",
        ),
        Mask(
            "pkce-challenge",
            (rb"(?<=[?&]code_challenge=)[A-Za-z0-9_-]{43}(?![A-Za-z0-9_-])",),
            b"<pkce-challenge>",
            "head",
            "the code_challenge parameter of a Location",
        ),
        Mask(
            "oidc-state",
            (rb"(?<=[?&]state=)[A-Za-z0-9_-]{43}(?![A-Za-z0-9_-])",),
            b"<oidc-state>",
            "head",
            "the state parameter of a Location",
        ),
        Mask(
            "client-assertion",
            (rb"(?<=client_assertion=)[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+",),
            b"<client-assertion>",
            "outbound body",
            "the compact JWS of a token or an introspection body, verified before it is masked",
        ),
        Mask(
            "federation-url",
            (rb"http://(?:127\.0\.0\.1|localhost|\[::1\]):[0-9]{1,5}(?:/[A-Za-z0-9._~/-]*)?",
             rb"(?<![0-9.])(?:127\.0\.0\.1|::1):[0-9]{1,5}(?![0-9])"),
            b"<federation-url>",
            "body",
            "the URL of a remote in a message or an extension and the loopback address of a refused connection",
        ),
        Mask(
            "formdata-boundary",
            (rb'(?<=boundary=")[^"\r\n]{1,70}(?=")',
             rb"(?<=boundary=)[0-9A-Za-z'()+_,./:=?-]{1,70}",
             rb"(?m)(?<=^--)[0-9A-Za-z'()+_,./:=?-]{1,70}?(?=(?:--)?\r?$)"),
            b"<boundary>",
            "upstream",
            "the boundary of a multipart request in its head and its delimiter lines",
        ),
    )
}

#: The masks of the values a side draws per request, in the order a comparison of an outbound
#: request applies them.
OIDC_MASKS = ("pkce-verifier", "pkce-challenge", "oidc-state", "client-assertion")


def apply(data: bytes, names) -> bytes:
    """`data` with the masks `names` applied in the order given."""
    for name in names:
        data = MASKS[name].apply(data)
    return data


def manifest(names) -> list:
    """The manifest entries of the masks `names`, in the order given."""
    return [MASKS[name].manifest() for name in names]


def mask_paths(text: str, root) -> str:
    """A text with the repository and the home directory replaced by `<repo>` and `<home>`."""
    text = text.replace(str(root), "<repo>")
    return text.replace(os.path.expanduser("~"), "<home>")
