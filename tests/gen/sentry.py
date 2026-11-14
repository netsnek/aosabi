# SPDX-License-Identifier: MIT
"""tests/gen: the served cases of sentry-app, without a DSN and with one.

Each case of [`sentry_cases`] is one request that runs with no SENTRY_DSN, so no
event leaves the process: `st-01`, `st-02`, `st-06` and `st-09` under the
variant `default`, `st-03` under `custom_key`, `st-04` under `null_key` and
`st-05` under `skip`. Case `st-09` sends two documents, `{now later}` and
`{later now}`, so its two requests are `st-09-now-later` and `st-09-later-now`.
Case `st-05` names its operation in the parameter `operationName` as well as in
the document, because the callback `skip` reads the arguments of the execution,
whose operation name is the parameter of the request and not the name the
document gives. Case `st-08` is the declaration of the plugin in the boot report
and no request. Every case runs on both sides under `NODE_ENV` `test` and
`development`, and is sent as it stands and again with
`Accept-Encoding: identity`.

Round 2 serves the variants `default`, `skip`, `env_dsn` and `initialized` with
`SENTRY_DSN` naming the mock ingest of tests/harness/mock_ingest.py, and each of
its cases names the port of the ingest its DSN reaches: `http` for `st-05`,
`st-06`, `st-07`, `st-10` and `st-11`, `https` for `st-12`, and the HTTPS port
in its hostile mode for `st-14`. Cases `st-05` and `st-06` send the bytes of
their round 1 requests, so their answers are held to the goldens of round 1.
Case `st-13` runs on the twin alone and compares no byte, so it has no request
here. A case names the count of envelopes it expects of the reference, or None
where a capture row decides it, `st-11` for CP10 and `st-14` for the hostile
ingest.
"""

from __future__ import annotations

from dataclasses import dataclass

from .plugins import ServedCase, get, post, served

#: The node environments every case runs under.
NODE_ENVS = ("test", "development")


def sentry_cases() -> list:
    """Every served case of sentry-app without a DSN, in the order of their ids."""
    cases = []

    def add(case_id, variant, request, pins):
        cases.append(served(case_id, "sentry-app", variant, request, pins, NODE_ENVS))

    add("st-01", "default", post("{boom}"), "the event id under development, a masked error without it under test")
    add("st-02", "default", post("{service gql}"), "no event id on an original GraphQL error")
    add("st-03", "custom_key", post("{boom}"), "the key stays sentryEventId")
    add("st-04", "null_key", post("{boom}"), "no event id")
    add("st-05", "skip", post("query Skipped {boom}", operationName="Skipped"), "no event id for a skipped operation")
    add("st-06", "default", get("/route-boom"), "500 unchanged by the capture of the error of the context")
    add("st-09-now-later", "default", post("{now later}"), "the settle order under the plugin, unchanged from M4")
    add("st-09-later-now", "default", post("{later now}"), "the settle order under the plugin, unchanged from M4")
    return cases


#: The variants of the cases without a DSN, each served fresh.
VARIANTS = ("default", "null_key", "custom_key", "skip")

#: The variants of the cases with a DSN, each served fresh with a fresh mock ingest.
VARIANTS_ROUND_2 = ("default", "skip", "env_dsn", "initialized")

#: The ports of the mock ingest a DSN names.
INGEST_PORTS = ("http", "https", "https-hostile")


@dataclass(frozen=True)
class IngestCase:
    """One served case with a DSN: the case, the port of the ingest its DSN names and the envelopes it expects."""

    case: ServedCase
    ingest: str
    envelopes: int | None


def sentry_cases_round_2() -> list:
    """Every served case of sentry-app with a DSN that the reference runs, in the order of their ids."""
    cases = []

    def add(case_id, variant, request, pins, ingest, envelopes):
        if ingest not in INGEST_PORTS:
            raise ValueError("no ingest port named %s" % ingest)
        cases.append(IngestCase(served(case_id, "sentry-app", variant, request, pins, NODE_ENVS), ingest, envelopes))

    add("st-05", "skip", post("query Skipped {boom}", operationName="Skipped"),
        "no event id for a skipped operation and no envelope", "http", 0)
    add("st-06", "default", get("/route-boom"), "500 unchanged and one envelope from the middleware", "http", 1)
    add("st-07", "initialized", post("{boom}"),
        "one envelope from the GraphQL half with the fingerprint, the tags, the breadcrumb and the GraphQL context",
        "http", 1)
    add("st-10", "default", post("{boom}"), "no envelope, because no client exists without init", "http", 0)
    add("st-11", "env_dsn", get("/route-boom"), "whether an environment DSN reaches the middleware, capture row CP10",
        "http", None)
    add("st-12", "default", get("/route-boom"), "one envelope from the middleware over TLS 1.3, capture row CP12",
        "https", 1)
    add("st-14", "default", get("/route-boom"), "500 as st-06 answers while the HTTPS port cuts its answer",
        "https-hostile", None)
    return cases


__all__ = ["INGEST_PORTS", "IngestCase", "NODE_ENVS", "VARIANTS", "VARIANTS_ROUND_2", "sentry_cases",
           "sentry_cases_round_2"]
