# SPDX-License-Identifier: MIT
"""tests/gen: the Zitadel corpora of `63_zitadel_auth` and the served cases of zitadel-auth-app.

Step 2 of `63_zitadel_auth` sends four generated corpora to the sidecar `tests/sidecar/zitadel.mjs`
and to the sub commands of `aosabi-devtool zitadel` and compares the two response streams:

- **The claim sets**, 200 requests of the operation `principal`, over a
  subject, roles and a user info object, some with mappers of the table of
  `tests/sidecar/zitadel.md` and some with none. The drawn sets alternate
  between the two, and the hand written edges carry mappers as each edge needs.
- **The request shapes**, 60 requests of the operation `read_token`, over an
  `Authorization` value, a query and a `Cookie` value, each optional.
- **The claim values**, 100 requests of the operation `roles`, over a roles
  claim of every JSON kind and the marker of `undefined`.
- **The parameter sets**, 40 requests of the operation `introspection_body`,
  over an issuer, a token and the extra parameters, with the client key of the
  run in either form.

A fifth corpus beside the four, `read_token_fields`, holds hand written request shapes of
`read_token` whose `cookie` is a list, the value of each `Cookie` field of a request that carries
its cookies in several, in both orders. It draws nothing and stands in files of its own, so the four
corpora keep their counts and their goldens. A claim a mapper reads keeps the type the reference's
options declare, a string or an absent email, a string or a number of the organisation, a list of
strings of the permissions, so the Rust side, whose mappers are typed function pointers, can answer
every request. Every other claim is any JSON value. The client key never enters a corpus: an
`introspection_body` request carries `{"$form": "pkcs1"}` or `{"$form": "pkcs8"}` as its `auth_key`,
which `bind_auth_keys` replaces with the record of the key of the run.

The served cases `z-01` to `z-31` of zitadel-auth-app are runs of the shape of tests/gen/login.py,
one variant and one issuer scenario served fresh each, with the hostile run of step 5 counted apart.
Case `z-18` sends no request and judges the introspection body of `z-02`. Two runs after the hostile
one send the cookies of one request in two `Cookie` fields in both orders, also counted apart:
`z-40` and `z-41` the session token beside another cookie, and the variant `with_login` the verifier
of a login into its callback, `z-42` and `z-43`, and the verifier with the session the callback set,
`z-44` and `z-45`, the cookies a browser holds for the 300 s of the verifier after a login. Every
token is a neutral test string, `t1` to `t13`, `t-admin` and the `t-login` the issuer answers, and
the email is `probe@example.invalid`.

Four runs after them hold every JWT answer of capture row C82 the plan's table leaves out, so each
observation of the row runs on both sides. The row holds each JWT case twice, behind a discovery
that names the signing algorithms, its keys with the suffix `-alg`, and behind the plain discovery,
its keys without it. The cases `z-12` and `z-20` of the plan stand for the first form, the run of
the claims adds `z-12-aud` to `z-20-typ` in that form, and the run of the plain discovery runs every
case of the row in the second, `z-12-plain` to `z-20-typ-plain`. The JWT answers of `z-12` and
`z-20` and of their plain forms carry the header and the placeholder signature of the capture, which
no key verifies, as the tokens of the capture that answer success did. The last run sends the
introspection answer cut in its middle, `z-malformed-introspect`. Each counts apart from the 31,
with the tokens `t7` and `t9` of the capture row and `t14` to `t17`.

The lower case of `toLowerCase` writes U+212A KELVIN SIGN as the ASCII `k`, so `normalizeTyp` reads
`token-introspection+jwt`, the type the check takes, and a lower case of ASCII alone would refuse
the answer.
"""

from __future__ import annotations

import json

from .login import (AUTH_KEY_BYTE_KEY, KEY_FORMS, KEY_NOT_SIGNED, KEY_REFUSED, KEY_SIZES, OTHER_COOKIE, Run, Step,
                    cookie_fields, counted_cases, get, graphql, verifier_cookie)
from .rng import Rng

#: The seed a run takes when its parameters name none.
DEFAULT_SEED = 1

#: The sizes of the four corpora.
CLAIM_SET_COUNT = 200
REQUEST_SHAPE_COUNT = 60
CLAIM_VALUE_COUNT = 100
PARAMETER_SET_COUNT = 40

#: The count of cases of the exit number of `63_zitadel_auth`.
CASE_COUNT = 31
#: The JWT cases of capture row C82 beyond the plan's table, each with the scenario of the mocked issuer and the
#: token of the row, in the order of the row.
JWT_ROW_CASES = (
    ("z-12-aud", "userinfo_jwt_bad_aud", "t7"),
    ("z-12-iss", "userinfo_jwt_bad_iss", "t7"),
    ("z-20-aud", "introspect_jwt_bad_aud", "t9"),
    ("z-20-iss", "introspect_jwt_bad_iss", "t9"),
    ("z-20-iat", "introspect_jwt_no_iat", "t9"),
    ("z-20-typ", "introspect_jwt_no_typ", "t9"),
)
#: The cases of the plain discovery, every JWT case of the row once more.
PLAIN_ROW_CASES = tuple((case + "-plain", scenario + "_plain", token) for case, scenario, token in (
    ("z-12", "userinfo_jwt", "t7"),
    *JWT_ROW_CASES[:2],
    ("z-20", "introspect_jwt", "t9"),
    *JWT_ROW_CASES[2:],
))
#: The cases of Zitadel's own shapes and the malformed introspection answer.
SHAPE_CASES = ("z-zitadel", "z-roles-project", "z-roles-per-project", "z-malformed-introspect")
HOSTILE_CASES = (tuple("z-key-%s-%s" % (name, form) for name in KEY_SIZES for form in KEY_FORMS)
                 + ("z-auth-key-byte", "z-surrogate-access-token", "z-surrogate-token-type", "z-surrogate-id-token",
                    "z-state-ff", "z-state-twice", "z-blocked-port", "z-roles-value-of"))
TOKEN_TYPE_CASES = ("z-typ-kelvin",)
#: The cases that run beside the 31 and count apart.
EXTRA_CASES = (("z-hostile", "z-40", "z-41", "z-42", "z-43", "z-44", "z-45")
               + tuple(case for case, _, _ in JWT_ROW_CASES + PLAIN_ROW_CASES) + SHAPE_CASES + HOSTILE_CASES
               + TOKEN_TYPE_CASES)

#: The mapper ids of each option, the table of tests/sidecar/zitadel.md, section principal.
MAPPERS = {
    "id": ("email", "sub_number", "null"),
    "tenant": ("resourceowner", "org_id", "null"),
    "permissions": ("perm", "perms", "null"),
    "attributes": ("email", "null"),
}
#: The claim of the roles and the claim of the organisation.
ROLES_CLAIM = "urn:zitadel:iam:org:projects:roles"
RESOURCE_OWNER_CLAIM = "urn:zitadel:iam:user:resourceowner:id"
#: The marker of `undefined` on the line of the sidecar.
UNDEFINED = {"$undefined": True}
#: The extra parameters the provider passes.
PROVIDER_PARAMETERS = {"scope": "openid email profile"}
#: The issuers an introspection body names, the mocked issuer's form and two names of the reserved domain.
ISSUERS = ("https://127.0.0.1:4940", "https://127.0.0.1:4655", "https://issuer.example.invalid",
           "https://issuer.example.invalid/tenant-a")

#: Strings a generated value draws from: the plain, the empty, the numeric looking and the non ASCII.
STRINGS = ("u1", "u2", "42", "", "0", " 7 ", "1e3", "0x1f", "-5", "3.5", "abc", "probe", "Ünïcödé", "日本",
           "a b", "a\"b", "back\\slash", "tab\tin", "😀", "null", "true", "Infinity")
#: Keys a generated object draws from, with canonical integer keys among them, which V8 orders first.
KEYS = ("name", "given_name", "locale", "email_verified", "updated_at", "1", "10", "2", "0", "01", "-1",
        "4294967294", "4294967295", "ñame", "", "a b", "nested", "x")
#: Roles a principal request names.
ROLE_NAMES = ("admin", "viewer", "editor", "1", "2", "10", "0", "role:with:colons", "Ümlaut", "")


def _value(rng: Rng, purpose: str, depth: int = 0):
    """One JSON value of any kind, nested to two levels at most."""
    kind = rng.choice(purpose + ".kind", ("string", "int", "float", "bool", "null", "array", "object")
                      if depth < 2 else ("string", "int", "float", "bool", "null"))
    stream = rng.stream(purpose + ".draw")
    if kind == "string":
        return rng.choice(purpose + ".string", STRINGS)
    if kind == "int":
        return rng.choice(purpose + ".int", (0, 1, -1, 2, 7, 42, 1000, 2 ** 31, 2 ** 53 - 1, -(2 ** 31)))
    if kind == "float":
        return rng.choice(purpose + ".float", (1.5, 2.25, -0.5, 1e21, 1e-7, 0.1))
    if kind == "bool":
        return stream.random() < 0.5
    if kind == "null":
        return None
    if kind == "array":
        return [_value(rng, purpose + ".item", depth + 1) for _ in range(stream.randint(0, 3))]
    return _object(rng, purpose + ".member", depth + 1, stream.randint(0, 4))


def _object(rng: Rng, purpose: str, depth: int, count: int) -> dict:
    keys = rng.pick(purpose + ".keys", KEYS, count)
    rng.stream(purpose + ".order").shuffle(keys)
    return {key: _value(rng, purpose + ".value", depth) for key in keys}


# ── principal ────────────────────────────────────────────────────────────────


def _user(rng: Rng, index: int) -> dict:
    """A user info object whose claims a mapper reads keep the types the options declare."""
    stream = rng.stream("principal.user")
    user = {}
    if stream.random() < 0.85:
        user["sub"] = rng.choice("principal.user.sub", ("u1", "42", "7", "abc", "", " 12 ", "1e2", "0x10", "-3",
                                                        "Infinity", "4.5"))
    if stream.random() < 0.7:
        user["email"] = rng.choice("principal.user.email", ("probe@example.invalid", "other@example.invalid", "",
                                                            "ünï@example.invalid"))
    if stream.random() < 0.5:
        user[RESOURCE_OWNER_CLAIM] = rng.choice("principal.user.owner", ("orgA", "orgB", "", 7, 0))
    if stream.random() < 0.4:
        user["org_id"] = rng.choice("principal.user.org", ("orgA", "orgC", 12))
    for claim in ("perm", "perms"):
        if stream.random() < 0.45:
            user[claim] = rng.pick("principal.user." + claim, ("invoice:read", "invoice:write", "crm:export",
                                                               "admin:secure", "", "1"), stream.randint(0, 4))
    for key, value in _object(rng, "principal.user.extra", 1, stream.randint(0, 4)).items():
        user.setdefault(key, value)
    order = list(user)
    rng.stream("principal.user.order").shuffle(order)
    return {key: user[key] for key in order}


def principal_requests(seed: int = DEFAULT_SEED) -> list:
    """The 200 claim sets of the operation `principal`, the hand written edges first."""
    rng = Rng(seed)
    requests = [
        {"sub": "42", "roles": [], "user": {"sub": "42", "org_id": "orgA", "perms": ["invoice:write"]},
         "mappers": {"id": "sub_number", "tenant": "org_id", "permissions": "perms"}},
        {"sub": "u1", "roles": [], "user": {"sub": "x"}, "mappers": {"id": "sub_number", "attributes": "email"}},
        {"sub": "u1", "roles": ["admin"], "user": {"sub": "u1", "email": "probe@example.invalid"}},
        {"sub": "u1", "roles": ["1", "2", "admin"], "user": {"sub": "u1", "email": "probe@example.invalid",
                                                             RESOURCE_OWNER_CLAIM: "orgA",
                                                             "perm": ["invoice:read"]},
         "mappers": {"id": "email", "tenant": "resourceowner", "permissions": "perm", "attributes": "email"}},
        {"sub": "u1", "roles": [], "user": {}, "mappers": {"id": "email", "tenant": "resourceowner",
                                                           "permissions": "perm", "attributes": "email"}},
        {"sub": "u1", "roles": [], "user": {"sub": "u1"}, "mappers": {"id": "null", "tenant": "null",
                                                                      "permissions": "null", "attributes": "null"}},
        {"sub": "", "roles": [], "user": {"email": ""}, "mappers": {"id": "email"}},
        {"sub": "u1", "roles": [], "user": {"sub": " 12 "}, "mappers": {"id": "sub_number"}},
        {"sub": "u1", "roles": [], "user": {"sub": "0x10"}, "mappers": {"id": "sub_number"}},
        {"sub": "u1", "roles": [], "user": {"sub": ""}, "mappers": {"id": "sub_number"}},
        {"sub": "u1", "roles": [], "user": {"sub": "1e2"}, "mappers": {"id": "sub_number"}},
        {"sub": "u1", "roles": [], "user": {"sub": "-Infinity"}, "mappers": {"id": "sub_number"}},
        {"sub": "u1", "roles": [], "user": {RESOURCE_OWNER_CLAIM: 7}, "mappers": {"tenant": "resourceowner"}},
        {"sub": "u1", "roles": [], "user": {"perm": []}, "mappers": {"permissions": "perm"}},
        {"sub": "u1", "roles": [""], "user": {"2": "b", "1": "a", "z": None, "a": [1, {"k": True}]}},
        {"sub": "u1", "roles": [], "user": {"nested": {"10": 1, "9": 2, "b": 3}}, "mappers": {"attributes": "null"}},
    ]
    while len(requests) < CLAIM_SET_COUNT:
        index = len(requests)
        stream = rng.stream("principal.shape")
        request = {
            "sub": rng.choice("principal.sub", ("u1", "u2", "42", "", "Ünïcödé", "7")),
            "roles": rng.pick("principal.roles", ROLE_NAMES, stream.randint(0, 4)),
            "user": _user(rng, index),
        }
        if index % 2 == 0:
            if index % 8 == 0:
                options = list(MAPPERS)
            else:
                options = [option for option in MAPPERS if stream.random() < 0.5] or [
                    rng.choice("principal.option", tuple(MAPPERS))]
            request["mappers"] = {option: rng.choice("principal.mapper." + option, MAPPERS[option])
                                  for option in options}
        requests.append(request)
    return [{"id": "principal-%03d" % (index + 1), "op": "principal", **request}
            for index, request in enumerate(requests)]


# ── read_token ───────────────────────────────────────────────────────────────

TOKEN_SHAPES = (
    {},
    {"authorization": "Bearer t1"},
    {"authorization": "Bearer"},
    {"authorization": "Bearer "},
    {"authorization": "Bearer  x"},
    {"authorization": "Bearer a b"},
    {"authorization": "bearer t1"},
    {"authorization": "BEARER t1"},
    {"authorization": "Basic x"},
    {"authorization": "Token t1"},
    {"authorization": "Bearer\tt1"},
    {"authorization": " Bearer t1"},
    {"authorization": "Bearer t1 "},
    {"authorization": ""},
    {"authorization": "Bearer t%201"},
    {"authorization": "Bearer t1,Bearer t2"},
    {"query": "token=t1"},
    {"query": "token="},
    {"query": "token"},
    {"query": "token=a&token=b"},
    {"query": "token=t%2B1&token=t2"},
    {"query": "token=t+1"},
    {"query": "token=%ZZ"},
    {"query": "x=1&token=t2"},
    {"query": "Token=t1"},
    {"query": "token=%E2%9C%93"},
    {"query": ""},
    {"cookie": "aosabi-auth=t1"},
    {"cookie": "aosabi-auth="},
    {"cookie": "a=1; aosabi-auth=t3"},
    {"cookie": "aosabi-auth=t%201"},
    {"cookie": "aosabi-auth=\"t1\""},
    {"cookie": "aosabi-auth=t1; aosabi-auth=t2"},
    {"cookie": "AOSABI-AUTH=t1"},
    {"cookie": "aosabi-auth"},
    {"cookie": "aosabi-auth=a=b"},
    {"cookie": "  aosabi-auth = t4 "},
    {"authorization": "Basic x", "cookie": "aosabi-auth=t1"},
    {"authorization": "Bearer", "cookie": "aosabi-auth=t1"},
    {"authorization": "Bearer t1", "query": "token=t2", "cookie": "aosabi-auth=t3"},
    {"authorization": "Basic x", "query": "token=t2", "cookie": "aosabi-auth=t3"},
    {"query": "token=", "cookie": "aosabi-auth=t3"},
    {"query": "other=1", "cookie": "aosabi-auth=t3"},
    {"authorization": "", "query": "token=t2"},
)
#: The pools the shapes after the hand written ones draw from.
AUTHORIZATIONS = ("Bearer t1", "Bearer", "Bearer  x", "Bearer a b", "Basic x", "bearer t1", "Bearer t-admin",
                  "Bearer x y z", "Bearer t-é")
QUERIES = ("token=t2", "token=", "token=a&token=b", "a=1&token=t%20x", "token=%25", "x=y", "token=t+2")
COOKIES = ("aosabi-auth=t3", "aosabi-auth=", "x=1; aosabi-auth=t%3D3", "other=1", "aosabi-auth=\"q\"; x=2")


def token_requests(seed: int = DEFAULT_SEED) -> list:
    """The 60 request shapes of the operation `read_token`, the hand written edges first."""
    rng = Rng(seed)
    shapes = [dict(shape) for shape in TOKEN_SHAPES]
    while len(shapes) < REQUEST_SHAPE_COUNT:
        stream = rng.stream("token.shape")
        shape = {}
        if stream.random() < 0.6:
            shape["authorization"] = rng.choice("token.authorization", AUTHORIZATIONS)
        if stream.random() < 0.6:
            shape["query"] = rng.choice("token.query", QUERIES)
        if stream.random() < 0.6:
            shape["cookie"] = rng.choice("token.cookie", COOKIES)
        if shape and shape not in shapes:
            shapes.append(shape)
    for shape in shapes:
        _check_shape(shape)
    return [{"id": "token-%03d" % (index + 1), "op": "read_token", **shape} for index, shape in enumerate(shapes)]


def _check_shape(shape: dict) -> None:
    """Refuses a shape no served request can carry: a header value `Headers` refuses or a query a URL cuts."""
    for name in ("authorization", "cookie"):
        value = shape.get(name)
        for one in value if isinstance(value, list) else [value]:
            if one is not None and any(ch in "\r\n\0" or ord(ch) > 0xFF for ch in one):
                raise ValueError("no request carries the %s %r" % (name, one))
    query = shape.get("query")
    if query is not None and any(ch in "#? " or ord(ch) < 0x21 or ord(ch) > 0x7E for ch in query):
        raise ValueError("no request carries the query %r" % query)


# ── read_token over the cookies of one request in several fields ─────────────

#: The request shapes whose cookies arrive in several `Cookie` fields, each in both orders where the
#: order moves the answer: the session after another cookie and before it, after the verifier of a
#: login and before it, twice, among three fields, beside a field of two cookies, beside an empty
#: field, behind a header that is no Bearer, a Bearer header and a query, and one field as a list.
COOKIE_FIELD_SHAPES = (
    {"cookie": ["a=1", "aosabi-auth=t1"]},
    {"cookie": ["aosabi-auth=t1", "a=1"]},
    {"cookie": ["aosabi_code_verifier=v1", "aosabi-auth=t3"]},
    {"cookie": ["aosabi-auth=t3", "aosabi_code_verifier=v1"]},
    {"cookie": ["aosabi-auth=t1", "aosabi-auth=t2"]},
    {"cookie": ["aosabi-auth=t2", "aosabi-auth=t1"]},
    {"cookie": ["a=1", "b=2", "aosabi-auth=t4"]},
    {"cookie": ["aosabi-auth=t4", "a=1", "b=2"]},
    {"cookie": ["a=1; b=2", "aosabi-auth=t%201"]},
    {"cookie": ["aosabi-auth=t%201", "a=1; b=2"]},
    {"cookie": ["", "aosabi-auth=t1"]},
    {"cookie": ["aosabi-auth=t1", ""]},
    {"cookie": ["a=1", "b=2"]},
    {"cookie": ["aosabi-auth=t5"]},
    {"authorization": "Basic x", "cookie": ["a=1", "aosabi-auth=t1"]},
    {"authorization": "Bearer t1", "cookie": ["a=1", "aosabi-auth=t3"]},
    {"query": "token=t2", "cookie": ["aosabi-auth=t3", "a=1"]},
    {"query": "other=1", "cookie": ["a=1", "aosabi-auth=t3"]},
)


def cookie_field_requests() -> list:
    """The request shapes of the operation `read_token` whose cookies arrive in several fields, `fields-001` on.

    The shapes stand in a corpus of their own beside the four of the plan, so
    those keep their counts and every line of their goldens.
    """
    shapes = [dict(shape) for shape in COOKIE_FIELD_SHAPES]
    for shape in shapes:
        _check_shape(shape)
    return [{"id": "fields-%03d" % (index + 1), "op": "read_token", **shape} for index, shape in enumerate(shapes)]


# ── roles ────────────────────────────────────────────────────────────────────

ROLE_CLAIMS = (
    UNDEFINED,
    None,
    {},
    [],
    "",
    "ab",
    "a",
    "😀",
    "é",
    "日本語",
    0,
    1,
    -1,
    1.5,
    True,
    False,
    ["x", "y"],
    [None, None, None],
    [[], {}],
    {"2": {}, "admin": {}, "1": {}},
    {"admin": {"orgA": "example.invalid"}},
    {"b": 1, "a": 2},
    {"10": 1, "9": 2, "1": 3},
    {"01": 1, "1": 2},
    {"-1": 1, "0": 2},
    {"1.5": 1, "1": 2},
    {"4294967294": 1, "4294967295": 2, "3": 3},
    {"9007199254740993": 1, "2": 2},
    {"": 1, " ": 2},
    {"admin": None, "viewer": False, "editor": 0},
    {"Ümlaut": {}, "z": {}, "a": {}},
    {"role:with:colons": {"orgA": "x"}},
)


def role_requests(seed: int = DEFAULT_SEED) -> list:
    """The 100 claim values of the operation `roles`, the hand written edges first."""
    rng = Rng(seed)
    claims = list(ROLE_CLAIMS)
    texts = {json.dumps(claim, sort_keys=False) for claim in claims}
    while len(claims) < CLAIM_VALUE_COUNT:
        kind = rng.choice("roles.kind", ("object", "object", "object", "string", "array", "scalar"))
        stream = rng.stream("roles.draw")
        if kind == "object":
            keys = rng.pick("roles.keys", KEYS + ROLE_NAMES, stream.randint(1, 6))
            stream.shuffle(keys)
            claim = {key: _value(rng, "roles.member", 1) for key in keys}
        elif kind == "string":
            claim = "".join(rng.choice("roles.char", ("a", "b", "1", "é", "😀", " ")) for _ in range(stream.randint(1, 6)))
        elif kind == "array":
            claim = [_value(rng, "roles.item", 1) for _ in range(stream.randint(1, 5))]
        else:
            claim = _value(rng, "roles.scalar", 2)
        text = json.dumps(claim, sort_keys=False)
        if text not in texts:
            texts.add(text)
            claims.append(claim)
    return [{"id": "roles-%03d" % (index + 1), "op": "roles", "claim": claim} for index, claim in enumerate(claims)]


# ── introspection_body ───────────────────────────────────────────────────────

PARAMETER_SETS = (
    (ISSUERS[0], "t1", PROVIDER_PARAMETERS, "pkcs1"),
    (ISSUERS[0], "t1", PROVIDER_PARAMETERS, "pkcs8"),
    (ISSUERS[0], "t1", {}, "pkcs1"),
    (ISSUERS[0], "t 1&x", {"b": "2", "1": "one", "client_id": "spoof"}, "pkcs1"),
    (ISSUERS[0], "t1", {"token": "spoof", "scope": "openid"}, "pkcs1"),
    (ISSUERS[0], "t1", {"client_assertion": "spoof", "client_assertion_type": "spoof"}, "pkcs1"),
    (ISSUERS[1], "a+b", {"10": "x", "2": "y", "01": "z"}, "pkcs1"),
    (ISSUERS[2], "ü", {"ü": "ä", "a b": "c d", "a&b": "c=d"}, "pkcs8"),
    (ISSUERS[3], "=", {"scope": "", "": ""}, "pkcs1"),
    (ISSUERS[0], "t" * 200, {"x": "%", "y": "+", "z": "😀"}, "pkcs1"),
    (ISSUERS[0], "t1", {"4294967295": "big", "4294967294": "index", "-1": "neg"}, "pkcs1"),
    (ISSUERS[2], "t-admin", {"scope": "openid email profile", "resource": "https://api.example.invalid/x?y=1"},
     "pkcs8"),
)
#: The pools the parameter sets after the hand written ones draw from.
PARAMETER_NAMES = ("scope", "resource", "audience", "token_type_hint", "1", "2", "10", "a b", "ü", "x", "client_id",
                   "token")
PARAMETER_VALUES = ("openid email profile", "", "x y", "ä", "😀", "%", "&=", "1", "a+b", "line")
TOKENS = ("t1", "t2", "t 1", "t&x", "ünï", "a.b.c", "x" * 64)


def introspection_body_requests(seed: int = DEFAULT_SEED) -> list:
    """The 40 parameter sets of the operation `introspection_body`, the hand written edges first.

    Each request carries `{"$form": <form>}` in place of its `auth_key`, which
    `bind_auth_keys` replaces with the record of the key of the run.
    """
    rng = Rng(seed)
    sets = [tuple(entry) for entry in PARAMETER_SETS]
    while len(sets) < PARAMETER_SET_COUNT:
        stream = rng.stream("body.shape")
        names = rng.pick("body.names", PARAMETER_NAMES, stream.randint(0, 5))
        stream.shuffle(names)
        parameters = {name: rng.choice("body.value", PARAMETER_VALUES) for name in names}
        sets.append((rng.choice("body.issuer", ISSUERS), rng.choice("body.token", TOKENS), parameters,
                     "pkcs8" if stream.random() < 0.25 else "pkcs1"))
    return [{"id": "body-%03d" % (index + 1), "op": "introspection_body", "issuer": issuer, "token": token,
             "parameters": dict(parameters), "auth_key": {"$form": form}}
            for index, (issuer, token, parameters, form) in enumerate(sets)]


def bind_auth_keys(requests, client_key) -> list:
    """`requests` with each `{"$form": <form>}` replaced by `client_key.auth_key(form)`, the record of the run."""
    bound = []
    for request in requests:
        key = request.get("auth_key")
        if isinstance(key, dict) and "$form" in key:
            request = {**request, "auth_key": client_key.auth_key(key["$form"])}
        bound.append(request)
    return bound


def corpora(seed: int = DEFAULT_SEED) -> dict:
    """The four corpora of step 2, by operation."""
    return {
        "principal": principal_requests(seed),
        "read_token": token_requests(seed),
        "roles": role_requests(seed),
        "introspection_body": introspection_body_requests(seed),
    }


def corpora_manifest(seed: int = DEFAULT_SEED) -> dict:
    """The counts and the seed of the four corpora, which the notebook prints and asserts."""
    sets = corpora(seed)
    counts = {name: len(requests) for name, requests in sets.items()}
    expected = {"principal": CLAIM_SET_COUNT, "read_token": REQUEST_SHAPE_COUNT, "roles": CLAIM_VALUE_COUNT,
                "introspection_body": PARAMETER_SET_COUNT}
    if counts != expected:
        raise AssertionError("the corpora hold %r where the plan counts %r" % (counts, expected))
    with_mappers = sum(1 for request in sets["principal"] if "mappers" in request)
    return {"seed": seed, "counts": counts, "principal_with_mappers": with_mappers,
            "principal_without_mappers": counts["principal"] - with_mappers}


# ── the served cases of zitadel-auth-app ─────────────────────────────────────


def _bearer(token: str) -> list:
    return [("Authorization", "Bearer " + token)]


def provider_runs() -> list:
    """Every run of zitadel-auth-app in the order of the plan's table, the hostile run and the runs of the cookie
    fields last."""
    fixture = "zitadel-auth-app"
    runs = []
    me = "{me{id tenant roles permissions email}}"

    def add(run_id, variant, steps, issuer="running", key=None):
        runs.append(Run(run_id, fixture, variant, issuer, tuple(steps), key=key))

    def ask(step_id, case, scenario, headers, pins, document=me, target="/graphql", carry=()):
        return Step(step_id, case, scenario, graphql(step_id, document, headers, target), pins, tuple(carry))

    ok = "introspect_ok"
    add("auth-default-introspect_ok", "default", [
        ask("z-01", "z-01", ok, [], "null and no request at the issuer", "{me{id}}"),
        ask("z-02", "z-02", ok, _bearer("t1"), "the principal, one discovery, one introspection and one user info "
            "request with the bodies of facts 21 and 22"),
        Step("z-18", "z-18", ok, None, "scope=openid+email+profile first and the client assertion verified with the "
             "key of the run", checks="z-02"),
        ask("z-03", "z-03", ok, [], "the query token of fact 18", target="/graphql?token=t1"),
        ask("z-04", "z-04", ok, [("Cookie", "aosabi-auth=t1")], "the cookie token"),
        ask("z-05", "z-05", ok, [("Authorization", "Basic x"), ("Cookie", "aosabi-auth=t1")],
            "the cookie, because the header is no Bearer"),
        ask("z-06", "z-06", ok, [("Authorization", "Bearer"), ("Cookie", "aosabi-auth=t1")], "null, fact 18"),
        ask("z-14-without", "z-14", ok, _bearer("t1"), "FORBIDDEN without the role", "{adminOnly}"),
        ask("z-14-with", "z-14", ok, _bearer("t-admin"), "ok with the role", "{adminOnly}"),
        ask("z-19-first", "z-19", ok, _bearer("t1"), "an introspection and a user info request"),
        ask("z-19-second", "z-19", ok, _bearer("t1"), "two more, because the reference caches nothing per token"),
        ask("z-24", "z-24", ok, [("Authorization", "Bearer a b")], "the token a, fact 18"),
        ask("z-25", "z-25", ok, [("Authorization", "Bearer  x")], "null and no request at the issuer"),
    ])
    for case, scenario, token, pins, document in (
        ("z-07", "introspect_inactive", "t2", "null", me),
        ("z-08", "introspect_no_sub", "t3", "null and no user info request", me),
        ("z-09", "introspect_401", "t4", "500 and {\"error\":\"Internal Server Error\"}, fact 23", me),
        ("z-10", "userinfo_sub_mismatch", "t5", "500", me),
        ("z-11", "userinfo_challenge", "t6", "500, the WWW-Authenticate challenge", me),
        ("z-12", "userinfo_jwt", "t7", "the principal from an unverified JWT user info answer, Q10-4", me),
        ("z-13", "roles_numeric", "t8", "the roles [\"1\", \"2\", \"admin\"], fact 20", "{me{id roles}}"),
    ):
        add("auth-default-" + scenario, "default", [ask(case, case, scenario, _bearer(token), pins, document)])
    add("auth-mapped", "mapped", [
        ask("z-15", "z-15", ok, _bearer("t1"), "the four mappers", "{me{id tenant permissions email}}"),
    ])
    add("auth-default-discovery_500", "default", [
        ask("z-16", "z-16", "discovery_500", _bearer("t1"), "500 while discovery answers 500"),
        ask("z-17", "z-17", ok, _bearer("t1"), "500 again and one discovery in all"),
    ])
    add("auth-default-introspect_jwt", "default", [
        ask("z-20", "z-20", "introspect_jwt", _bearer("t9"), "the principal from an unverified "
            "token-introspection+jwt answer chosen by its content type, Q10-4"),
    ])
    add("auth-default-closed", "default", [
        ask("z-21", "z-21", None, _bearer("t1"), "500 with the issuer's port closed"),
    ], issuer="closed")
    add("auth-two_keys", "two_keys", [
        ask("z-22", "z-22", ok, _bearer("t1"), "one bootstrap, both client assertions signed with the key of "
            "a.json, Q10-3"),
    ])
    add("auth-key_file", "key_file", [
        ask("z-23", "z-23", ok, _bearer("t1"), "the key read from key.json"),
    ])
    add("auth-default-roles", "default", [
        ask("z-26-string", "z-26", "roles_string", _bearer("t10"), "the roles [\"0\", \"1\"] of the string \"ab\"",
            "{me{id roles}}"),
        ask("z-26-array", "z-26", "roles_array", _bearer("t11"), "the roles [\"0\", \"1\"] of an array of two",
            "{me{id roles}}"),
    ])
    add("auth-default-introspect_400_json", "default", [
        ask("z-27", "z-27", "introspect_400_json", _bearer("t12"), "500, the ResponseBodyError in the log record"),
    ])
    add("auth-default-userinfo_not_json", "default", [
        ask("z-28", "z-28", "userinfo_not_json", _bearer("t13"), "500"),
    ])
    both = "login_and_introspect"
    add("auth-with_login", "with_login", [
        Step("z-29-login", "z-29", both, get("z-29-login", "/auth/login"), "302 and the verifier cookie",
             hands_challenge=True),
        Step("z-29-callback", "z-29", both, get("z-29-callback", "/auth/callback?code=c1&state=s1", verifier_cookie()),
             "302 and the cookie aosabi-auth holding the token of the scenario",
             (("verifier", "z-29-login", "aosabi_code_verifier"),)),
        ask("z-30", "z-30", both, [("Cookie", "aosabi-auth={session}")], "the principal of the introspected token, "
            "and one discovery for the login and the provider together, fact 2", "{me{id roles email}}",
            carry=(("session", "z-29-callback", "aosabi-auth"),)),
        Step("z-31-logout", "z-31", both, get("z-31-logout", "/auth/logout", [("Cookie", "aosabi-auth={session}")]),
             "the cookie aosabi-auth deleted as in lg-03", (("session", "z-29-callback", "aosabi-auth"),)),
        ask("z-31-me", "z-31", both, [("Cookie", "aosabi-auth={cleared}")], "null with no request at the issuer",
            "{me{id}}", carry=(("cleared", "z-31-logout", "aosabi-auth"),)),
    ])
    add("auth-hostile", "default", [
        Step("z-hostile", "z-hostile", ok, graphql("z-hostile", me, _bearer("t1")),
             "500 while the issuer cuts the record of its answer", hostile="answer"),
        ask("z-hostile-after", "z-hostile", ok, [], "the side answers {me{id}} with no token after the cut",
            "{me{id}}"),
    ])
    add("auth-cookie-fields", "default", [
        ask("z-40", "z-40", ok, cookie_fields(OTHER_COOKIE, "aosabi-auth=t1"), "the principal of the cookie token "
            "in the second of two Cookie fields, which the Headers joins with 0x3B 0x20 as undici joins them"),
        ask("z-41", "z-41", ok, cookie_fields("aosabi-auth=t1", OTHER_COOKIE), "the principal of the cookie token "
            "in the first of two Cookie fields"),
    ])
    verifier = "aosabi_code_verifier={verifier}"
    session = "aosabi-auth={session}"
    from_login = ("verifier", "z-42-login", "aosabi_code_verifier")
    from_callback = ("session", "z-42", "aosabi-auth")
    callback = "/auth/callback?code=c1&state=s1"
    add("auth-with_login-cookie-fields", "with_login", [
        Step("z-42-login", "z-42", both, get("z-42-login", "/auth/login"), "302 and the verifier cookie the steps "
             "of z-42 to z-45 carry", hands_challenge=True),
        Step("z-42", "z-42", both, get("z-42", callback, cookie_fields(OTHER_COOKIE, verifier)),
             "302 and the cookie aosabi-auth, the verifier in the second of two Cookie fields", (from_login,)),
        Step("z-43", "z-43", both, get("z-43", callback, cookie_fields(verifier, OTHER_COOKIE)),
             "302 and the cookie aosabi-auth, the verifier in the first of two Cookie fields", (from_login,)),
        ask("z-44", "z-44", both, cookie_fields(verifier, session), "the principal of the session in the second "
            "of two Cookie fields, behind the verifier a browser keeps for 300 s", "{me{id roles email}}",
            carry=(from_login, from_callback)),
        ask("z-45", "z-45", both, cookie_fields(session, verifier), "the principal of the session in the first "
            "of two Cookie fields", "{me{id roles email}}", carry=(from_login, from_callback)),
    ])
    add("auth-default-jwt-claims", "default", [
        ask(case, case, scenario, _bearer(token), "500 and the error of C82 for the case %s-alg" % case)
        for case, scenario, token in JWT_ROW_CASES
    ])
    add("auth-default-jwt-plain", "default", [
        ask(case, case, scenario, _bearer(token), "the answer of C82 for the case %s behind a discovery that names "
            "no signing algorithm" % case[: -len("-plain")])
        for case, scenario, token in PLAIN_ROW_CASES
    ])
    add("auth-default-zitadel-shape", "default", [
        ask("z-zitadel", "z-zitadel", "introspect_zitadel", _bearer("t14"), "the principal of an introspection "
            "answer in the shape of Zitadel, aud a list, with the roles [] of the names Zitadel sends",
            "{me{id roles email}}"),
        ask("z-roles-project", "z-roles-project", "roles_project", _bearer("t15"), "the roles [] of "
            "urn:zitadel:iam:org:project:roles, which the provider does not read", "{me{id roles}}"),
        ask("z-roles-per-project", "z-roles-per-project", "roles_per_project", _bearer("t16"), "the roles [] of the "
            "claim of one project, which the provider does not read", "{me{id roles}}"),
    ])
    add("auth-default-introspect_malformed", "default", [
        ask("z-malformed-introspect", "z-malformed-introspect", "introspect_malformed", _bearer("t17"), "500 for an "
            "introspection answer cut in its middle"),
    ])
    for name, (bits, primes) in KEY_SIZES.items():
        for form in KEY_FORMS:
            case = "z-key-%s-%s" % (name, form)
            add("auth-key-%s-%s" % (name, form), "default", [
                ask(case, case, ok, _bearer("t1"), "the principal whose introspection the key of %d bits and %d primes "
                    "in the form %s signs, or the refusal at signing" % (bits, primes, form), "{me{id}}"),
            ], key="%s-%s" % (name, form))
    add("auth-auth-key-byte", "default", [
        ask("z-auth-key-byte", "z-auth-key-byte", ok, _bearer("t1"), "the principal with the key of AUTH_KEY, whose "
            "byte that is no UTF-8 node reads as U+FFFD", "{me{id}}"),
    ], key=AUTH_KEY_BYTE_KEY)
    from_login = (("verifier", "z-surrogate-login", "aosabi_code_verifier"),)
    add("auth-with_login-lone-surrogate", "with_login", [
        Step("z-surrogate-login", "z-surrogate-access-token", "surrogate_access_token",
             get("z-surrogate-login", "/auth/login"), "302 and the verifier every callback of the run carries",
             hands_challenge=True),
        Step("z-surrogate-access-token", "z-surrogate-access-token", "surrogate_access_token",
             get("z-surrogate-access-token", callback, verifier_cookie()), "500 and the URIError of "
             "encodeURIComponent for the lone surrogate of the access token", from_login),
        Step("z-surrogate-token-type", "z-surrogate-token-type", "surrogate_token_type",
             get("z-surrogate-token-type", callback, verifier_cookie()), "500 for a token type with a lone surrogate",
             from_login),
        Step("z-surrogate-id-token", "z-surrogate-id-token", "surrogate_id_token",
             get("z-surrogate-id-token", callback, verifier_cookie()), "500 for an ID token with a lone surrogate in "
             "its payload", from_login),
    ])
    from_login = (("verifier", "z-state-login", "aosabi_code_verifier"),)
    add("auth-with_login-state-odd", "with_login", [
        Step("z-state-login", "z-state-ff", both, get("z-state-login", "/auth/login"), "302 and the verifier both "
             "callbacks of the run carry", hands_challenge=True),
        Step("z-state-ff", "z-state-ff", both, get("z-state-ff", "/auth/callback?code=c1&state=%FF", verifier_cookie()),
             "500 and no token request, the state %FF the query keeps and the grant reads as U+FFFD", from_login),
        Step("z-state-twice", "z-state-twice", both, get("z-state-twice", "/auth/callback?code=c1&state=s1&state=s2",
                                                         verifier_cookie()),
             "500 and no token request, since the grant takes no state twice", from_login),
    ])
    add("auth-blocked-port", "default", [
        ask("z-blocked-port", "z-blocked-port", ok, _bearer("t1"), "500 and fetch failed with no request at the "
            "issuer, whose port the Fetch standard blocks"),
    ], issuer="blocked")
    add("auth-default-roles_value_of", "default", [
        ask("z-roles-value-of", "z-roles-value-of", "roles_value_of", _bearer("t18"), "500 and the TypeError of "
            "valueOf, since the roles claim holds an own key valueOf", "{me{id roles}}"),
    ])
    add("auth-default-introspect_jwt_typ_kelvin", "default", [
        ask("z-typ-kelvin", "z-typ-kelvin", "introspect_jwt_typ_kelvin", _bearer("t19"), "the principal of a "
            "token-introspection+jwt answer whose typ holds U+212A KELVIN SIGN, which lowers to the ASCII k"),
    ])
    return runs


def refusal_step(step_id: str) -> str | None:
    """The step of `KEY_REFUSED` in the form of the step `step_id` of a key of `KEY_NOT_SIGNED`, or None.

    The twin answers the principal under such a key as both sides answer it
    under the key both refuse at signing, the golden it is held to.
    """
    for name in KEY_NOT_SIGNED:
        for form in KEY_FORMS:
            if step_id == "z-key-%s-%s" % (name, form):
                return "z-key-%s-%s" % (KEY_REFUSED, form)
    return None


def provider_h2_run() -> Run:
    """The run of the cookies over HTTP/2 of `63_zitadel_auth`: a login, its callback with the verifier in one field
    and in one of two fields in both orders, and the principal with the verifier and the session in two fields in
    both orders, which the twin answers over HTTP/2 and the reference over HTTP/1.1.

    The run has no golden. The reference serves HTTP/1.1 alone, so each answer of the twin over HTTP/2 is
    held to the reference's answer over HTTP/1.1 to the same fields.
    """
    both = "login_and_introspect"
    verifier = "aosabi_code_verifier={verifier}"
    session = "aosabi-auth={session}"
    from_login = ("verifier", "z-h2-login", "aosabi_code_verifier")
    from_callback = ("session", "z-h2-one", "aosabi-auth")
    callback = "/auth/callback?code=c1&state=s1"
    me = "{me{id roles email}}"
    return Run("auth-h2-cookie-fields", "zitadel-auth-app", "with_login", "running", (
        Step("z-h2-login", "z-h2", both, get("z-h2-login", "/auth/login"), "302 and the verifier the callbacks carry",
             hands_challenge=True),
        Step("z-h2-one", "z-h2", both, get("z-h2-one", callback, cookie_fields(verifier)),
             "302 and the cookie aosabi-auth, the verifier in one field", (from_login,)),
        Step("z-h2-second", "z-h2", both, get("z-h2-second", callback, cookie_fields(OTHER_COOKIE, verifier)),
             "302 and the cookie aosabi-auth, the verifier in the second of two fields", (from_login,)),
        Step("z-h2-first", "z-h2", both, get("z-h2-first", callback, cookie_fields(verifier, OTHER_COOKIE)),
             "302 and the cookie aosabi-auth, the verifier in the first of two fields", (from_login,)),
        Step("z-h2-me-second", "z-h2", both, graphql("z-h2-me-second", me, cookie_fields(verifier, session)),
             "the principal of the session in the second of two fields", (from_login, from_callback)),
        Step("z-h2-me-first", "z-h2", both, graphql("z-h2-me-first", me, cookie_fields(session, verifier)),
             "the principal of the session in the first of two fields", (from_login, from_callback)),
    ))


def provider_manifest() -> dict:
    """The counts of the provider runs, which the notebook prints and asserts."""
    runs = provider_runs()
    cases = counted_cases(runs, EXTRA_CASES)
    if len(cases) != CASE_COUNT:
        raise AssertionError("the provider runs hold %d cases where the plan counts %d" % (len(cases), CASE_COUNT))
    return {
        "runs": len(runs),
        "steps": sum(len(run.steps) for run in runs),
        "cases": len(cases),
        "extra": list(EXTRA_CASES),
        "variants": sorted({run.variant for run in runs}),
        "scenarios": sorted({step.scenario for run in runs for step in run.steps if step.scenario}),
    }


__all__ = [
    "CASE_COUNT",
    "CLAIM_SET_COUNT",
    "CLAIM_VALUE_COUNT",
    "COOKIE_FIELD_SHAPES",
    "DEFAULT_SEED",
    "EXTRA_CASES",
    "HOSTILE_CASES",
    "JWT_ROW_CASES",
    "MAPPERS",
    "PARAMETER_SET_COUNT",
    "PLAIN_ROW_CASES",
    "REQUEST_SHAPE_COUNT",
    "SHAPE_CASES",
    "TOKEN_TYPE_CASES",
    "bind_auth_keys",
    "cookie_field_requests",
    "corpora",
    "corpora_manifest",
    "introspection_body_requests",
    "principal_requests",
    "provider_h2_run",
    "provider_manifest",
    "provider_runs",
    "refusal_step",
    "role_requests",
    "token_requests",
]
