# SPDX-License-Identifier: MIT
"""Each case is one request of the table of the plan, `id-01` to `id-31` under
the variant `default`, `as-01` to `as-04` under `async`, `nu-01` and `nu-02`
under `numeric`, `fa-01` to `fa-03` under `failing`, `no-01` to `no-03` under
`none` and `tw-01` under `twice`. Every case runs under `NODE_ENV` `test`,
unset and `development`, and is sent as it stands and again with
`Accept-Encoding: identity`. The principal of a case is the trusted headers
the provider of the fixture reads, `x-user-id`, `x-org`, `x-role` and
`x-perm`, as the header lists of this module write them. Every identifier is a
neutral test string, `u1`, `u2`, `42`, `orgA` and `orgB`, and names no
customer.
"""

from __future__ import annotations

from .http import get_target
from .plugins import get, post, served

#: The node environments every case of the fixture runs under.
NODE_ENVS = ("test", "unset", "development")

#: The trusted headers of the principals the cases send.
U1 = [("x-user-id", "u1"), ("x-org", "orgA"), ("x-role", "admin,crm,billing"), ("x-perm", "crm:export,admin:secure")]
U1_ORG_B = [("x-user-id", "u1"), ("x-org", "orgB"), ("x-role", "editor,viewer"), ("x-perm", "crm:export")]
U1_PLAIN = [("x-user-id", "u1")]
U2 = [("x-user-id", "u2")]
U2_DELETE = [("x-user-id", "u2"), ("x-perm", "accounts:delete")]


def role(name: str) -> list:
    """A principal `u2` holding the one role `name`."""
    return [("x-user-id", "u2"), ("x-role", name)]


def identity_cases() -> list:
    """Every served case of use-identity-app in the order of the table of the plan."""
    cases = []

    def add(case_id, variant, request, pins):
        cases.append(served(case_id, "use-identity-app", variant, request, pins, NODE_ENVS))

    add("id-01", "default", post("{hello}"), "an open field")
    add("id-02", "default", post("{whoami}"), "the absence of a principal")
    add("id-03", "default", post("{whoami}", U1), "the bound principal")
    add("id-04", "default", post("{me{id tenant roles permissions}}", U1_ORG_B), "the fields of the principal")
    add("id-05", "default", post("{adminOnly}"), "FORBIDDEN with path and locations and a null data")
    add("id-06", "default", post("{adminOnly}", role("admin")), "the answer to an admin")
    add("id-07", "default", post("{editorOrAdmin}", role("editor")), "any of the roles, the first")
    add("id-08", "default", post("{editorOrAdmin}", role("viewer")), "any of the roles, none held")
    add("id-09", "default", post("{noRoles}", role("admin")), "requireRole() refuses everyone")
    add("id-10", "default", post("{canExport hasAnyRole}", U1), "hasPermission and hasRole with getPrincipal()")
    add("id-11", "default", post("{report{title secret}}", role("viewer")), "a nested refusal and its path")
    add("id-12", "default", post("{reports{secret}}", role("viewer")), "two refusals, both mapped, in order")
    add("id-13", "default", post("{featureOff}"), "the message and the feature extension")
    add("id-14", "default", post("{mixed{guarded failing}}"), "one error mapped beside one left as it is")
    add("id-15", "default", post("{asyncGuard}"), "a refusal after an await")
    add("id-16", "default", post('mutation{deleteAccount(id: "u1")}', U1_PLAIN), "authorize with a closure over the argument, the owner")
    add("id-17", "default", post('mutation{deleteAccount(id: "u1")}', U2), "authorize with a closure over the argument, another")
    add("id-18", "default", post('mutation{deleteAccount(id: "u1")}', U2_DELETE), "authorize with a closure over the argument, a permission")
    add("id-19", "default", post("{invoices}", role("billing")), "the gate on every root entry of its app, passed")
    add("id-20", "default", post("{invoices}", role("viewer")), "the gate on every root entry of its app, refused")
    add("id-21", "default", post("mutation{issue(amount: 100)}", role("viewer")), "the gate on a mutation of its app")
    add("id-22", "default", get("/admin/report"), "the 403 route, {\"error\":\"Not permitted.\"}")
    add("id-23", "default", get("/admin/report", role("admin")), "the route's answer")
    add("id-24", "default", get("/admin/whoami", U1), "the principal in a route")
    add("id-25", "default", get("/admin/secure/data", U1_PLAIN), "a refusing middleware of a child app")
    add("id-26", "default", get("/admin/secure/data", U1_PLAIN + [("x-perm", "admin:secure")]),
        "a middleware of a child app that passes")
    add("id-27", "default", get("/early/whoami", U1),
        "{\"id\":null} on the reference, the principal's id on the twin, divergence D75")
    add("id-28", "default", post("{whoami}", [("x-user-id", "")]), "an empty header is anonymous")
    add("id-29", "default", get(get_target("{adminOnly}")), "the mapping over GET")
    add("id-30", "default", get("/admin/export", role("viewer")), "the branch of the route example, 403")
    add("id-31", "default", get("/admin/export", role("admin")), "the branch of the route example, ok")
    add("as-01", "async", post("{whoami}", U1), "id-03 through the asynchronous provider")
    add("as-02", "async", post("{adminOnly}"), "id-05 through the asynchronous provider")
    add("as-03", "async", post("{adminOnly}", role("admin")), "id-06 through the asynchronous provider")
    add("as-04", "async", get("/admin/report"), "id-22 through the asynchronous provider")
    add("nu-01", "numeric", post("{whoami}", [("x-user-id", "42")]), "the number id, \"42\"")
    add("nu-02", "numeric", post('mutation{deleteAccount(id: "42")}', [("x-user-id", "42")]),
        "=== refusing 42 against \"42\"")
    add("fa-01", "failing", post("{hello}", [("x-fail", "1")]),
        "the ServiceError of the provider before the GraphQL handler, 500 and Internal Server Error, "
        "because the root onError reads no statusCode of its own")
    add("fa-02", "failing", get("/admin/whoami", [("x-fail", "1")]),
        "the ServiceError of the provider before a route, 500 and Internal Server Error, "
        "because the root onError reads no statusCode of its own")
    add("fa-03", "failing", post("{whoami}", U1), "the provider when it succeeds")
    add("no-01", "none", post("{whoami}", U1), "no principal without the plugin")
    add("no-02", "none", post("{adminOnly}"), "the unmapped ForbiddenError, masked or kept by NODE_ENV")
    add("no-03", "none", get("/admin/report"), "the 403 route without the plugin")
    add("tw-01", "twice", post("{adminOnly}"), "two identity plugins, one mapping")
    return cases


def log_case():
    """Case `log-01` of `65_logs`: `{adminOnly}` anonymous under `NODE_ENV=production`, quirk."""
    return served("log-01", "use-identity-app", "default", post("{adminOnly}"),
                  "the graphql error record of a refusal before the remap", ("production",))


__all__ = ["NODE_ENVS", "identity_cases", "log_case"]
