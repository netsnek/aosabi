# SPDX-License-Identifier: MIT
"""`60_plugins` sends three generated corpora to the sidecar
`tests/sidecar/plugins.mjs` and to the primitive `plugins` of aosabi-devtool
and compares the two response streams as bytes:

- **The order vectors**, 2000 requests of the operation `topo_sort`, each of 1
  to 12 plugins with names drawn from 6, a tenth of them unnamed, a tenth
  carrying the empty string as a name, with collisions, self dependencies,
  missing dependencies and a cycle in every fourth.
- **The boot descriptions**, 300 requests of the operation `boot`, over
  strategies, roles, dependencies and up to one setup that throws an `Error`
  or a value, with the landing page and the playground switched now and then.
- **The error lists**, 200 requests of the operation `map_forbidden`, over
  plain errors, refusals with the default message, their own message and a
  feature, the empty feature among them, paths of names and indices, and
  extensions.

Every draw names its purpose in the `Rng` of `tests/gen/rng.py`, so a corpus is
the same bytes on every machine for one seed. The module also holds the served
cases `pl-01` to `pl-15` of `plugins-app`, the boots of its table, and the
`ServedCase` the identity and Sentry cases of `tests/gen/identity.py` and
`tests/gen/sentry.py` are made of. Cases `pl-12` and `pl-13` send the cookie of
`pl-10` in one of two `Cookie` fields, after another cookie and before it,
which the `Headers` of each side joins before the route reads the cookie, and
they follow `pl-11`, so no case before them moves. Every name and value is a
neutral test string.
"""

from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass

from .http import HttpCase, json_body, make
from .rng import Rng

#: The seed a run takes when its parameters name none.
DEFAULT_SEED = 1

#: The sizes of the three corpora.
ORDER_VECTOR_COUNT = 2000
BOOT_DESCRIPTION_COUNT = 300
ERROR_LIST_COUNT = 200

#: The six names the generated plugins draw from.
NAMES = ("a", "b", "c", "d", "e", "f")

#: Names a dependency may carry that no generated plugin carries.
MISSING = ("identity", "database")

FAMILY = "M7"


@dataclass(frozen=True)
class ServedCase:
    """One served case of a fixture: its id, its variant, its request and what it pins."""

    id: str
    fixture: str
    variant: str
    http: HttpCase
    pins: str
    node_envs: tuple

    def raw(self) -> bytes:
        """The request as the raw client writes it."""
        return self.http.raw()

    def identity(self) -> "ServedCase":
        """The same case sent with `Accept-Encoding: identity`, under the id `<id>-identity`."""
        http = self.http
        headers = tuple(pair for pair in http.headers if pair[0].lower() != "content-length")
        length = tuple(pair for pair in http.headers if pair[0].lower() == "content-length")
        again = make(FAMILY, self.id + "-identity", http.method, http.target,
                     [pair for pair in headers if pair[0] not in ("Host", "x-request-id")]
                     + [("Accept-Encoding", "identity")] + list(length), http.body)
        return ServedCase(self.id + "-identity", self.fixture, self.variant, again, self.pins, self.node_envs)


def post(document: str, headers=(), **extra) -> tuple:
    """The method, target, fields and body of a `POST /graphql` of `document`."""
    return "POST", "/graphql", [("Content-Type", "application/json")] + list(headers), json_body(document, **extra)


def get(target: str, headers=()) -> tuple:
    """The method, target, fields and body of a `GET` of `target`."""
    return "GET", target, list(headers), b""


def served(case_id: str, fixture: str, variant: str, request: tuple, pins: str, node_envs) -> ServedCase:
    """Builds a served case with `Host` first and its own `x-request-id`, the rule of tests/gen/http.py."""
    method, target, headers, body = request
    return ServedCase(case_id, fixture, variant, make(FAMILY, case_id, method, target, headers, body), pins,
                      tuple(node_envs))


@dataclass(frozen=True)
class BootCase:
    """One boot of plugins-app: its id, the variant, the entry, the role and the node environment."""

    id: str
    variant: str
    entry: str
    role: str | None
    node_env: str | None
    dump: bool
    fails: bool
    pins: str


def plugin_cases() -> list:
    """The served cases `pl-01` to `pl-15` of plugins-app, under `default` and `NODE_ENV=test`."""
    envs = ("test",)
    short = [("x-short", "1")]
    return [
        served("pl-01", "plugins-app", "default", get("/trace"), "the middleware order, the sorted order of the first pass", envs),
        served("pl-02", "plugins-app", "default", post("{hello}"),
               "extensions.hooks in the raw array order of the envelop hooks, and a member added last", envs),
        served("pl-03", "plugins-app", "default", get("/late/first"), "a first pass route before a last pass route", envs),
        served("pl-04", "plugins-app", "default", get("/late/other"), "the last pass route", envs),
        served("pl-05", "plugins-app", "default", get("/anything", short),
               "the short circuit lost behind the middlewares that await without returning, 500 and Internal Server Error",
               envs),
        served("pl-06", "plugins-app", "default", post("{hello}", short),
               "the GraphQL handler skipped and the short circuit lost, 500 and Internal Server Error", envs),
        served("pl-07", "plugins-app", "default", get("/plugin/ping"), "x-after: yes on a route answer", envs),
        served("pl-08", "plugins-app", "default", get("/plugin/cookie"), "the attribute order of Set-Cookie, fact 51", envs),
        served("pl-09", "plugins-app", "default", get("/plugin/redirect"), "302, Location and the null body of fact 52", envs),
        served("pl-10", "plugins-app", "default", get("/plugin/cookie-read", [("Cookie", "k=v%20w")]),
               "the decoded value and the deleting Set-Cookie", envs),
        served("pl-11", "plugins-app", "default", get("/viewer/claimed"),
               "the application's route below the reserved prefix, fact 65", envs),
        served("pl-12", "plugins-app", "default", get("/plugin/cookie-read", [("Cookie", "a=1"), ("Cookie", "k=v%20w")]),
               "the cookie of pl-10 in the second of two Cookie fields, which the Headers joins with 0x3B 0x20 as "
               "undici joins them", envs),
        served("pl-13", "plugins-app", "default", get("/plugin/cookie-read", [("Cookie", "k=v%20w"), ("Cookie", "a=1")]),
               "the cookie of pl-10 in the first of two Cookie fields", envs),
        served("pl-14", "plugins-app", "default", post("query ReadsData { hello }", operationName="ReadsData"),
               "a done hook that reads data, data.hello copied into extensions.read, item X11 of EXEC1", envs),
        served("pl-15", "plugins-app", "default", post("query WritesData { hello }", operationName="WritesData"),
               "a done hook that writes data, data.hello in upper case, item X11 of EXEC1", envs),
    ]


def boot_cases() -> list:
    """The boots of plugins-app: 8 that come up, under three roles and the worker entry, and 5 that fail."""
    cases = []
    for role, name in ((("web", "bt-web"), ("unset", "bt-unset"), ("all", "bt-all"))):
        for node_env in ("production", "test"):
            cases.append(BootCase("%s-%s" % (name, node_env), "default", "server",
                                  None if role == "unset" else role, node_env, False, False,
                                  "the boot log, the readiness and the boot report"))
    for node_env in ("production", "test"):
        cases.append(BootCase("bt-worker-%s" % node_env, "default", "worker", "worker", node_env, True, False,
                              "the boot log up to the dump file, no listen line, the setup order of the dump"))
    for variant, name, pins in (
        ("fail_named", "bt-fail-named", "a named setup failure and its cause"),
        ("fail_unnamed", "bt-fail-unnamed", "the label #3 (unnamed) and the cause of a thrown string"),
        ("fail_value", "bt-fail-value", "a thrown value that is no Error"),
        ("fail_last", "bt-fail-last", "a failure of the last pass after the handler mount"),
        ("cycle", "bt-cycle", "the cycle message and no cause line"),
    ):
        cases.append(BootCase(name, variant, "server", None, "test", False, True, pins))
    return cases


def order_vectors(seed: int = DEFAULT_SEED, count: int = ORDER_VECTOR_COUNT) -> list:
    """The generated requests of the operation `topo_sort`, `gv-0001` on.

    Every fourth vector carries a cycle: two plugins of two distinct names that
    depend on each other, each the last plugin of its name, so the sort meets
    one of them while it visits the other whatever the rest of the vector is.
    """
    rng = Rng(seed)
    vectors = []
    for index in range(count):
        size = rng.stream("M7.order.size").randint(1, 12)
        plugins = []
        for _ in range(size):
            roll = rng.stream("M7.order.name").random()
            name = None if roll < 0.1 else "" if roll < 0.2 else rng.choice("M7.order.pick", NAMES)
            depends_on = None
            if rng.stream("M7.order.deps").random() >= 0.4:
                width = rng.stream("M7.order.width").randint(0, 3)
                depends_on = [rng.choice("M7.order.dep", NAMES + MISSING) for _ in range(width)]
                if name and rng.stream("M7.order.self").random() < 0.15:
                    depends_on.append(name)
            plugins.append({"name": name, "depends_on": depends_on})
        if index % 4 == 3:
            first, second = rng.stream("M7.order.cycle").sample(NAMES, 2)
            if len(plugins) < 2:
                plugins.append({"name": None, "depends_on": None})
            at = rng.stream("M7.order.cycle.at").sample(range(len(plugins)), 2)
            for plugin in plugins:
                if plugin["name"] in (first, second):
                    plugin["name"] = None
            plugins[at[0]] = {"name": first, "depends_on": [second] + (plugins[at[0]]["depends_on"] or [])}
            plugins[at[1]] = {"name": second, "depends_on": [first]}
        vectors.append({"id": "gv-%04d" % (index + 1), "op": "topo_sort", "plugins": plugins})
    return vectors


def order_counts(vectors: list) -> dict:
    """The counts step 5 of `60_plugins` prints: collisions, self and missing dependencies, cycles planned."""
    counts = {"vectors": len(vectors), "collisions": 0, "self": 0, "missing": 0, "cycles planned": 0,
              "unnamed": 0, "empty names": 0}
    for index, vector in enumerate(vectors):
        names = [plugin["name"] for plugin in vector["plugins"] if plugin["name"]]
        counts["collisions"] += len(names) != len(set(names))
        counts["self"] += any(plugin["name"] and plugin["name"] in (plugin["depends_on"] or [])
                              for plugin in vector["plugins"])
        counts["missing"] += any(dep not in names for plugin in vector["plugins"] for dep in plugin["depends_on"] or [])
        counts["cycles planned"] += index % 4 == 3
        counts["unnamed"] += sum(1 for plugin in vector["plugins"] if plugin["name"] is None)
        counts["empty names"] += sum(1 for plugin in vector["plugins"] if plugin["name"] == "")
    return counts


def boot_descriptions(seed: int = DEFAULT_SEED, count: int = BOOT_DESCRIPTION_COUNT) -> list:
    """The generated requests of the operation `boot`, `gb-001` on.

    A description carries 0 to 8 plugins, each with or without a name, the
    empty name among them, a strategy, roles, dependencies, a setup and a
    middleware, and three descriptions in ten make one setup throw, an `Error`
    or a value. The landing page and the playground are switched off in some,
    which moves the always on plugins of a pass.
    """
    rng = Rng(seed)
    descriptions = []
    for index in range(count):
        size = rng.stream("M7.boot.size").randint(0, 8)
        plugins = []
        for _ in range(size):
            row = {}
            roll = rng.stream("M7.boot.name").random()
            if roll >= 0.2:
                row["name"] = rng.choice("M7.boot.pick", NAMES)
            elif roll >= 0.15:
                row["name"] = ""
            if rng.stream("M7.boot.deps").random() < 0.3:
                width = rng.stream("M7.boot.width").randint(1, 2)
                row["depends_on"] = [rng.choice("M7.boot.dep", NAMES + MISSING) for _ in range(width)]
            strategy = rng.stream("M7.boot.strategy").random()
            if strategy >= 0.7:
                row["strategy"] = "last"
            elif strategy >= 0.5:
                row["strategy"] = "first"
            roles = rng.stream("M7.boot.roles").random()
            if roles >= 0.9:
                row["roles"] = ["web", "worker"]
            elif roles >= 0.75:
                row["roles"] = ["worker"]
            elif roles >= 0.6:
                row["roles"] = ["web"]
            if rng.stream("M7.boot.setup").random() < 0.7:
                row["setup"] = "ok"
            if rng.stream("M7.boot.middleware").random() < 0.3:
                row["middleware"] = True
            plugins.append(row)
        if plugins and rng.stream("M7.boot.throw").random() < 0.3:
            at = rng.stream("M7.boot.throw.at").randrange(len(plugins))
            plugins[at]["setup"] = ("throw:setup failed" if rng.stream("M7.boot.throw.kind").random() < 0.5
                                    else "throw-value:plain value")
        request = {"id": "gb-%03d" % (index + 1), "op": "boot", "plugins": plugins}
        landing = rng.stream("M7.boot.landing").random()
        if landing >= 0.85:
            request["landing_page"] = True
        elif landing >= 0.7:
            request["landing_page"] = False
        graphiql = rng.stream("M7.boot.graphiql").random()
        if graphiql >= 0.9:
            request["graphiql"] = True
        elif graphiql >= 0.7:
            request["graphiql"] = False
        descriptions.append(request)
    return descriptions


def error_lists(seed: int = DEFAULT_SEED, count: int = ERROR_LIST_COUNT) -> list:
    """The generated requests of the operation `map_forbidden`, `ge-001` on.

    A list holds 0 to 6 errors.
    """
    rng = Rng(seed)
    fields = ("adminOnly", "report", "secret", "reports", "mixed", "guarded")
    lists = []
    for index in range(count):
        size = rng.stream("M7.errors.size").randint(0, 6)
        errors = []
        for _ in range(size):
            depth = rng.stream("M7.errors.depth").randint(0, 3)
            path = []
            for level in range(depth):
                if level and rng.stream("M7.errors.index").random() < 0.3:
                    path.append(rng.stream("M7.errors.at").randint(0, 2))
                else:
                    path.append(rng.choice("M7.errors.field", fields))
            error = {"message": rng.choice("M7.errors.message", ("Unexpected error.", "plain failure", "denied"))}
            if path:
                error["path"] = path
            kind = rng.stream("M7.errors.kind").random()
            if kind < 0.5:
                forbidden = {}
                shape = rng.stream("M7.errors.forbidden").random()
                if shape >= 0.3:
                    forbidden["message"] = rng.choice("M7.errors.forbidden.message", ("Not permitted.", "Billing is off.", "No access."))
                if shape >= 0.6:
                    forbidden["feature"] = rng.choice("M7.errors.feature", ("billing", "exports", ""))
                error["forbidden"] = forbidden
            if rng.stream("M7.errors.extensions").random() < 0.3:
                error["extensions"] = {"code": rng.choice("M7.errors.code", ("BAD_INPUT", "LIMIT", "INTERNAL"))}
            errors.append(error)
        lists.append({"id": "ge-%03d" % (index + 1), "op": "map_forbidden", "errors": errors})
    return lists


def corpus_digest(requests: list) -> str:
    """The SHA-256 of the request lines of a corpus, the witness a golden's manifest keeps."""
    text = "".join(json.dumps(request, separators=(",", ":"), ensure_ascii=False) + "\n" for request in requests)
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def plugins_manifest(seed: int = DEFAULT_SEED) -> dict:
    """The counts and the digests of the three corpora and of the served cases at `seed`."""
    vectors, boots, errors = order_vectors(seed), boot_descriptions(seed), error_lists(seed)
    return {
        "seed": seed,
        "order_vectors": {"count": len(vectors), "digest": corpus_digest(vectors), **order_counts(vectors)},
        "boot_descriptions": {"count": len(boots), "digest": corpus_digest(boots)},
        "error_lists": {"count": len(errors), "digest": corpus_digest(errors)},
        "served_cases": [case.id for case in plugin_cases()],
        "boots": [case.id for case in boot_cases()],
    }


__all__ = [
    "BOOT_DESCRIPTION_COUNT",
    "BootCase",
    "ERROR_LIST_COUNT",
    "ORDER_VECTOR_COUNT",
    "ServedCase",
    "boot_cases",
    "boot_descriptions",
    "corpus_digest",
    "error_lists",
    "get",
    "order_counts",
    "order_vectors",
    "plugin_cases",
    "plugins_manifest",
    "post",
    "served",
]
