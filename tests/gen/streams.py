# SPDX-License-Identifier: MIT
"""tests/gen: the served cases of `40_sse`, a fixed list with their canonical requests.

No case of this module is drawn.

- **S01 to S31**, the stream cases over `subscription-app`, each sent under
  `NODE_ENV` unset and `test`, and **N01 to N04**, the four subscriptions whose
  `Accept` names no event stream, which answer 406 and are no stream.
- **P01 to P09**, the fan out over `pubsub-app`, each a scenario of steps on one
  boot, in that order, so the revisions of both sides agree.
- **MAP1 to MAP4**, the confirmation that the two error mappers of the reference
  remap a single result and no stream, on the variants `identity` and `database`
  of `subscription-app` beside `default`.
- **CAD0 to CAD2**, the keepalive cadence, each with the period and the
  tolerance a gap is held to.

A canonical request is the object `request.json` of a golden holds:
`{"method", "path", "headers", "body"}`, with `headers` in the order the fields
are sent and `body` the JSON text Python's `json.dumps` writes with its default
separators, the form of the golden `03d1b0055e5b`, which is case S01 under
`test`. A case id is the first twelve hex digits of the SHA-256 of that object
with its keys sorted, the function of `tests/harness/sse.py`. A scenario is
identified by the same function over `{"scenario": [...]}`, which is what its
`request.json` holds.

A scenario is a list of steps the notebook runs in order.

| Step | Meaning |
| --- | --- |
| `{"open": "A", "request": {...}}` | subscriber `A` sends a request on a connection of its own |
| `{"prelude": "A"}` | waits for the first frame of `A` |
| `{"wait_ms": n}` | waits `n` milliseconds, where the listeners attach on both sides |
| `{"send": {...}}` | sends a request on a new connection and reads its answer whole |
| `{"frames": "A", "count": n}` | waits until `A` holds `n` frames after the prelude that are no ping |
| `{"close": "A"}` | the client of `A` closes its connection |

The 50 milliseconds after the preludes are where the listener is attached on
both sides.
"""

from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass
from urllib.parse import quote

SUBSCRIPTION_APP = "subscription-app"
PUBSUB_APP = "pubsub-app"

#: The two environments every stream case runs under.
NODE_ENVS = ("unset", "test")

#: The wait after the preludes of a scenario, in milliseconds.
ATTACH_WAIT_MS = 50

EVENT_STREAM = "text/event-stream"


def case_id(request) -> str:
    """The first twelve hex digits of the SHA-256 of the canonical request, keys sorted."""
    canonical = json.dumps(request, sort_keys=True, separators=(",", ":"))
    return hashlib.sha256(canonical.encode("utf8")).hexdigest()[:12]


def post(document: str, accept: str | None = EVENT_STREAM, variables=None, operation_name=None,
         headers=()) -> dict:
    """A `POST` of a JSON body, `Content-Type` first and `Accept` after it when given."""
    body = {"query": document}
    if variables is not None:
        body["variables"] = variables
    if operation_name is not None:
        body["operationName"] = operation_name
    fields = {"Content-Type": "application/json"}
    if accept is not None:
        fields["Accept"] = accept
    fields.update(headers)
    return {"method": "POST", "path": "/graphql", "headers": fields, "body": json.dumps(body)}


def get(document: str, accept: str = EVENT_STREAM) -> dict:
    """A `GET` with the document percent encoded in the search string."""
    return {
        "method": "GET",
        "path": "/graphql?query=" + quote(document, safe=""),
        "headers": {"Accept": accept},
        "body": None,
    }


@dataclass(frozen=True)
class StreamCase:
    """One request of a stream case or of a 406 case.

    `counted` says whether the case is a stream the exit number counts, which
    N01 to N04 are not. `close_after` is the number of frames after the prelude
    that are no ping after which the client closes, and `then` names the case
    sent next on a new connection.
    """

    name: str
    fixture: str
    question: str
    request: dict
    counted: bool = True
    close_after: int | None = None
    then: str | None = None
    node_envs: tuple = NODE_ENVS

    @property
    def id(self) -> str:
        return case_id(self.request)

    def as_json(self) -> dict:
        return {
            "name": self.name,
            "id": self.id,
            "fixture": self.fixture,
            "question": self.question,
            "request": self.request,
            "counted": self.counted,
            "close_after": self.close_after,
            "then": self.then,
            "node_envs": list(self.node_envs),
        }


@dataclass(frozen=True)
class Scenario:
    """One fan out case, a script of steps on one boot."""

    name: str
    fixture: str
    question: str
    steps: tuple
    subscribers: tuple

    @property
    def id(self) -> str:
        return case_id({"scenario": list(self.steps)})

    def as_json(self) -> dict:
        return {
            "name": self.name,
            "id": self.id,
            "fixture": self.fixture,
            "question": self.question,
            "subscribers": list(self.subscribers),
            "steps": list(self.steps),
        }


@dataclass(frozen=True)
class MapperCase:
    """One confirmation of a mapper: cases sent under a variant and under `default`.

    `expect` is `remapped` where the variant answers otherwise than `default`,
    and `unchanged` where it answers the same bytes.
    """

    name: str
    variant: str
    cases: tuple
    expect: str
    node_env: str = "test"

    def as_json(self) -> dict:
        return {
            "name": self.name,
            "variant": self.variant,
            "cases": list(self.cases),
            "expect": self.expect,
            "node_env": self.node_env,
        }


@dataclass(frozen=True)
class CadenceCase:
    """One cadence case: a stream, its environment and the bound on its ping gaps."""

    name: str
    request: dict
    node_env: str
    period_ms: int
    tolerance_ms: int
    least_gaps: int

    @property
    def id(self) -> str:
        return case_id(self.request)

    def as_json(self) -> dict:
        return {
            "name": self.name,
            "id": self.id,
            "request": self.request,
            "node_env": self.node_env,
            "period_ms": self.period_ms,
            "tolerance_ms": self.tolerance_ms,
            "least_gaps": self.least_gaps,
        }


def ticks(count: int, interval: int) -> str:
    """The document of `ticks` in the spelling of the golden `03d1b0055e5b`."""
    return "subscription{ticks(count: %d, intervalMs: %d)}" % (count, interval)


def _s(name: str, question: str, request: dict, **extra) -> StreamCase:
    return StreamCase(name, SUBSCRIPTION_APP, question, request, **extra)


#: The fields S11 to S21 ask one question each of, in the order of the table of
#: the plan, section The TypeScript side, first.
FIELD_CASES = (
    ("S11", "failBeforeYield", "an error before the first yield, status 500"),
    ("S12", "failInExecutor", "the executor's rejection surfacing through the last iteration"),
    ("S13", "okThenFail", "a masked error in the middle of a stream"),
    ("S14", "okThenServiceError", "an unmasked error in the middle of a stream"),
    ("S15", "stopWithError", "stop with an error"),
    ("S16", "emptyStream", "the prelude and the terminator alone"),
    ("S17", "pushNull", "a per event execution error with data null"),
    ("S18", "nullableTicks", "a null event value in a nullable field"),
    ("S19", "objectTicks", "the projection of an object typed payload, capture row CR2"),
    ("S20", "contextProbe", "which context a repeater executor sees, capture row CR4"),
)


def stream_cases() -> list[StreamCase]:
    """S01 to S31 over `subscription-app` and N01 to N04, in that order."""
    cases = [
        _s("S01", "ticks, the golden 03d1b0055e5b under test", post(ticks(3, 40))),
        _s("S02", "ticks with count 1 and interval 0", post(ticks(1, 0))),
        _s("S03", "ticks with count 0, the prelude and the terminator", post(ticks(0, 0))),
        _s("S04", "ticks with count 5 and interval 10", post(ticks(5, 10))),
        _s("S05", "ticks through two variables", post(
            "subscription($count: Number!, $intervalMs: Number!){ticks(count: $count, intervalMs: $intervalMs)}",
            variables={"count": 2, "intervalMs": 10})),
        _s("S06", "ticks over GET with the query in the search string", get(ticks(2, 10))),
        _s("S07", "ticks under an alias", post("subscription{tick: ticks(count: 2, intervalMs: 10)}")),
        _s("S08", "ticks chosen by operationName out of two operations", post(
            "subscription One{ticks(count: 1, intervalMs: 0)} subscription Two{ticks(count: 2, intervalMs: 10)}",
            operation_name="Two")),
        _s("S09", "ticks with no Accept header", post(ticks(2, 10), accept=None)),
        _s("S10", "a variable the Number scalar refuses, status 400", post(
            "subscription($count: Number!){ticks(count: $count, intervalMs: 0)}",
            variables={"count": "three"})),
    ]
    for name, field_name, question in FIELD_CASES:
        selection = "{seq label}" if field_name == "objectTicks" else ""
        cases.append(_s(name, question, post("subscription{%s%s}" % (field_name, selection))))
    cases.extend([
        _s("S21", "@inContext on a subscription, capture row CR3", post(
            'subscription @inContext(locale: "de"){localeProbe}')),
        _s("S22", "a subscription selecting a field that does not exist", post("subscription{nope}")),
        _s("S23", "a subscription selecting two root fields", post(
            "subscription{ticks(count: 1, intervalMs: 0) emptyStream}")),
        _s("S24", "the query status under Accept: text/event-stream", post("{status}")),
        _s("S25", "a ForbiddenError before the first yield", post("subscription{guardBeforeYield}")),
        _s("S26", "a ForbiddenError in the middle of a stream", post("subscription{guardMidStream}")),
        _s("S27", "ticks with Accept: */*", post(ticks(2, 10), accept="*/*")),
        _s("S28", "ticks with Accept-Encoding: gzip, answered uncompressed", post(
            ticks(2, 10), headers={"Accept-Encoding": "gzip"})),
        _s("S29", "ticks closed by the client after the second frame, then S02 on a new connection",
           post(ticks(5, 40)), close_after=2, then="S02"),
        _s("S30", "okThenFail closed by the client after the first frame", post(
            "subscription{okThenFail}"), close_after=1),
        _s("S31", "@inContext with the locale as a variable, capture row CR3", post(
            "subscription Probe($locale: String) @inContext(locale: $locale){localeProbe}",
            variables={"locale": "de"})),
        _s("N01", "ticks with Accept: application/json, capture row CR1", post(
            ticks(2, 10), accept="application/json"), counted=False),
        _s("N02", "ticks with Accept: multipart/mixed, capture row CR1", post(
            ticks(2, 10), accept="multipart/mixed"), counted=False),
        _s("N03", "ticks over GET with Accept: application/json, capture row CR1", get(
            ticks(2, 10), accept="application/json"), counted=False),
        StreamCase("N04", PUBSUB_APP, "changes with Accept: application/json, capture row CR1", post(
            "subscription{changes}", accept="application/json"), counted=False),
    ])
    return cases


def bump(section: str) -> dict:
    """The mutation `bump` of one section, answered as one JSON result."""
    return post('mutation{bump(section: "%s")}' % section, accept="application/json")


def _open(name: str, request: dict) -> dict:
    return {"open": name, "request": request}


def _scenario(name: str, question: str, subscribers: tuple, opens: list, acts: list,
              reads: dict) -> Scenario:
    """A scenario that opens its subscribers, waits for their preludes, acts and reads."""
    steps = [_open(subscriber, request) for subscriber, request in opens]
    steps.extend({"prelude": subscriber} for subscriber, _ in opens)
    steps.append({"wait_ms": ATTACH_WAIT_MS})
    steps.extend(acts)
    steps.extend({"frames": subscriber, "count": count} for subscriber, count in reads.items())
    steps.extend({"close": subscriber} for subscriber in subscribers)
    return Scenario(name, PUBSUB_APP, question, tuple(steps), subscribers)


def fanout_cases() -> list[Scenario]:
    """P01 to P09 over `pubsub-app`, in the order they run on one boot."""
    changes = post("subscription{changes}")
    sections = ("playback", "queue", "button")
    return [
        _scenario("P01", "A and B on changes, and A's client sends bump on a second connection",
                  ("A", "B"), [("A", changes), ("B", changes)], [{"send": bump("playback")}],
                  {"A": 1, "B": 1}),
        _scenario("P02", "a publish from a timer callback outside every request", ("A", "B"),
                  [("A", changes), ("B", changes)],
                  [{"send": post('mutation{bumpLater(section: "button", delayMs: 50)}',
                                 accept="application/json")}],
                  {"A": 1, "B": 1}),
        _scenario("P03", "two sections, each subscriber receives its own", ("A", "B"),
                  [("A", post('subscription{changesIn(section: "playback")}')),
                   ("B", post('subscription{changesIn(section: "queue")}'))],
                  [{"send": bump("playback")}, {"send": bump("queue")}], {"A": 1, "B": 1}),
        Scenario("P04", PUBSUB_APP, "A disconnects, then bump, and B alone receives it", (
            _open("A", changes), _open("B", changes), {"prelude": "A"}, {"prelude": "B"},
            {"wait_ms": ATTACH_WAIT_MS}, {"close": "A"}, {"wait_ms": ATTACH_WAIT_MS},
            {"send": bump("playback")}, {"frames": "B", "count": 1}, {"close": "B"},
        ), ("A", "B")),
        _scenario("P05", "three subscribers and ten bumps in a row", ("A", "B", "C"),
                  [("A", changes), ("B", changes), ("C", changes)],
                  [{"send": bump(sections[index % 3])} for index in range(10)],
                  {"A": 10, "B": 10, "C": 10}),
        _scenario("P06", "changes over GET", ("A",), [("A", get("subscription{changes}"))],
                  [{"send": bump("queue")}], {"A": 1}),
        _scenario("P07", "changes with Accept: */*", ("A",), [("A", post("subscription{changes}", accept="*/*"))],
                  [{"send": bump("button")}], {"A": 1}),
        _scenario("P08", "changes with Accept-Encoding: gzip, answered uncompressed", ("A",),
                  [("A", post("subscription{changes}", headers={"Accept-Encoding": "gzip"}))],
                  [{"send": bump("playback")}], {"A": 1}),
        _scenario("P09", "two bumps before A reads, both frames in order", ("A",), [("A", changes)],
                  [{"send": bump("playback")}, {"send": bump("queue")}], {"A": 2}),
    ]


#: The query whose answer each mapper remaps.
GUARD_QUERY = StreamCase("G01", SUBSCRIPTION_APP, "guardQuery as a single result, which a mapper remaps",
                         post("{guardQuery}", accept="application/json"), counted=False)


def mapper_cases() -> list[MapperCase]:
    """MAP1 to MAP4 on the reference side of `subscription-app`."""
    return [
        MapperCase("MAP1", "identity", ("G01",), "remapped"),
        MapperCase("MAP2", "database", ("G01",), "remapped"),
        MapperCase("MAP3", "identity", ("S25", "S26"), "unchanged"),
        MapperCase("MAP4", "database", ("S25", "S26"), "unchanged"),
    ]


def cadence_cases() -> list[CadenceCase]:
    """CAD0 to CAD2, the ping every 300 ms under `test` and every 12000 ms otherwise.

    The period is the one the processor reads from `NODE_ENV`, CAD0 is the
    request of capture row C35, and a gap is counted from the prelude.
    """
    return [
        CadenceCase("CAD0", post(ticks(2, 1400)), "test", 300, 50, 4),
        CadenceCase("CAD1", post(ticks(2, 2000)), "test", 300, 50, 5),
        CadenceCase("CAD2", post(ticks(2, 12500)), "unset", 12000, 250, 1),
    ]


def counts() -> dict:
    """The number of cases per kind, which the corpus manifest records."""
    streams = stream_cases()
    fanout = fanout_cases()
    return {
        "stream": sum(1 for case in streams if case.counted),
        "not_acceptable": sum(1 for case in streams if not case.counted),
        "fanout": len(fanout),
        "fanout_subscribers": sum(len(case.subscribers) for case in fanout),
        "mapper": len(mapper_cases()),
        "cadence": len(cadence_cases()),
    }


def as_json() -> dict:
    """The served cases as the committed corpus holds them."""
    return {
        "manifest": {"version": 1, "counts": counts()},
        "streams": [case.as_json() for case in stream_cases()],
        "guard": GUARD_QUERY.as_json(),
        "fanout": [case.as_json() for case in fanout_cases()],
        "mappers": [case.as_json() for case in mapper_cases()],
        "cadence": [case.as_json() for case in cadence_cases()],
    }
