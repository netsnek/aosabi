# SPDX-License-Identifier: MIT
"""tests/gen: the served login cases of zitadel-login-app, the cases of `61_oidc_login`.

A step is one request with the scenario the issuer answers it under, so a run that changes the scenario between two
steps, `lg-22` and `lg-23`, says so on each step.

Every request sends `Host: app.example.invalid`, so the `redirect_uri` both sides build from the request URL is one
string, and its own `x-request-id`, so no side generates one. A step that needs a cookie a side set names it as a
placeholder in braces in its `Cookie` line and names in `carry` the step whose `Set-Cookie` holds the value, so the
notebook carries the verifier cookie of a login into a callback the way a browser carries it. A login step whose
challenge the token endpoint checks says so in `hands_challenge`, and the notebook hands the issuer the
`code_challenge` of its `Location` before the next step. A callback case that needs a verifier of its own run, a
case after `lg-07` under another scenario, starts its run with a login step of the same case.

Case `lg-13` keeps the plan's request, which answers 400 because the query carries no `code`. The extra case
`lg-13b` sends `code` beside `error`, which reaches the authorization error of the callback, and counts apart from
the 25. The hostile run sends a login while the issuer cuts the record of its answer and then `{__typename}`, step 7
of the notebook, and counts apart as well. Every run serves under `NODE_ENV=production`, where a 500 writes its log
record as one line of JSON, the records step 6 compares.

The last run carries the verifier of its login in one of two `Cookie` fields, after another cookie in `lg-40` and
before it in `lg-41`, the two orders of a client that sends one field per cookie, which RFC 9113 allows over HTTP/2,
so the `Headers` of each side joins the two fields before the callback reads the verifier. Both count apart from the
25, and the run stands after every run of the plan, so no step before it moves.

Three runs after it hold every token and refusal of capture rows C80 and C81 the plan's table leaves out, so each
observation of those rows runs on both sides: one login whose verifier the callbacks of the ID tokens `lg-11-iss`,
`lg-11-exp`, `lg-11-iat` and `lg-11-sub` carry, with the positive case `lg-zitadel` in the shape of an ID token of
Zitadel, `aud` a list with `azp`, and the malformed answers `lg-malformed-id-token` and `lg-malformed-token`, then
the malformed discovery of `lg-malformed-discovery`, and the refused connection of `lg-refused`, whose issuer port
nothing listens on. Each counts apart from the 25, and the case of each step that a row holds is the key of its
observation.

- **The key cases.** A login and its callback under each RSA key of
  `KEY_SIZES` in each form of `KEY_FORMS`, two primes of 1024 to 8192 bits and
  three primes of 3072 bits, `lg-key-rsa1024-pkcs1` to
  `lg-key-rsa3072mp-pkcs8`, whose callback signs the client assertion with
  the key, and a login under each value of `KEY_VALUES` that is no RSA private
  key, `lg-key-no-key`, `lg-key-public-pkcs1` and `lg-key-null`. A run names
  its key in `key`, and the notebook writes `AUTH_KEY` from it. The twin
  refuses at signing each key of `KEY_NOT_SIGNED`, which the reference signs,
  the entry of `NOT_PORTED` in `aosabi-compat`, so the notebook holds the
  callback of each such key to the golden of `KEY_REFUSED` in the same form,
  the key both sides refuse at signing.
- **The bytes of `AUTH_KEY`.** `lg-auth-key-byte` logs in with the key of the
  run in a variable `AUTH_KEY` that holds the byte 0xFF after the value of
  `type`, a byte that is no UTF-8, which node reads as U+FFFD.
- **The blocked port.** `lg-blocked-port` logs in against an issuer that
  listens on a port of the list of bad ports of the Fetch standard, which
  the issuer of every other run never takes.

The run of the tokens of C80 and C81 carries the verifier of its login into three more kinds of callbacks after its
malformed answers, each counted apart from the 25:

- **The audiences of an ID token.** `lg-aud-no-client`, an `aud` list without
  the client, `lg-aud-no-azp`, a list of two with no `azp`, and
  `lg-azp-other`, an `azp` of another client, the three refusals of the code
  grant beside the accepting `lg-zitadel`.
- **The forms of base64url.** `lg-b64-lenient`, a payload in the alphabet of
  base64 with `+`, `/`, white space and padding, which the forgiving decode of
  both sides takes, and `lg-malformed-b64-length` and
  `lg-malformed-b64-alphabet`, valid claims in base64url with one character
  past a multiple of four and with characters outside every alphabet of base64,
  which both refuse.
- **The hash of the access token.** `lg-zitadel-at-hash`, the ID token of
  Zitadel with the `at_hash` of another token, which neither side checks.

Every name and value is a neutral test string, the code `c1`, the states `s1`,
`s2` and `s9`, the extra parameter `session_state=x1` and the cookie `other=1`.
"""

from __future__ import annotations

import hashlib
from dataclasses import dataclass

from .http import HttpCase, canonical, json_body

FAMILY = "M7b"
#: The host every case sends, so the `redirect_uri` is one string on both sides.
APP_HOST = "app.example.invalid"
#: The node environments every run serves under.
NODE_ENVS = ("production",)
#: The count of cases of the exit number of `61_oidc_login`.
CASE_COUNT = 25
#: The cases of the tokens and refusals of capture rows C80 and C81 beyond the plan's table, the positive case in
#: Zitadel's shape and the malformed answers, which run after every run of the plan.
ROW_CASES = ("lg-11-iss", "lg-11-exp", "lg-11-iat", "lg-11-sub", "lg-zitadel", "lg-malformed-id-token",
             "lg-malformed-token", "lg-malformed-discovery", "lg-refused")
#: The RSA keys of the key cases, by name: the bits of the modulus and the count of its primes.
KEY_SIZES = {
    "rsa1024": (1024, 2),
    "rsa2048": (2048, 2),
    "rsa2560": (2560, 2),
    "rsa3072": (3072, 2),
    "rsa3584": (3584, 2),
    "rsa4096": (4096, 2),
    "rsa4608": (4608, 2),
    "rsa8192": (8192, 2),
    "rsa3072mp": (3072, 3),
}
#: The forms a key case hands each side, the PKCS#1 of a key file of Zitadel and PKCS#8.
KEY_FORMS = ("pkcs1", "pkcs8")
#: The keys the twin refuses at signing and the reference signs, the entry of `NOT_PORTED` in aosabi-compat on the
#: RSA keys ring 0.17.14 does not sign.
KEY_NOT_SIGNED = ("rsa2560", "rsa3584", "rsa4608", "rsa8192", "rsa3072mp")
#: The key both sides refuse at signing, a modulus under the 2048 bits oauth4webapi signs with, whose callback is the
#: golden the callback of a key of `KEY_NOT_SIGNED` is held to on the twin.
KEY_REFUSED = "rsa1024"
#: The values of `key` in `AUTH_KEY` that are no RSA private key, by name: a text with no key, the PKCS#1 public half
#: of the key of the run, which the marker `{"$public": "pkcs1"}` names, and `null`, whose destructuring throws.
KEY_VALUES = {
    "no-key": "no key here",
    "public-pkcs1": {"$public": "pkcs1"},
    "null": None,
}
#: The key of the run `lg-auth-key-byte`, the key of the run with the byte of `AUTH_KEY_BYTE` after the value of `type`.
AUTH_KEY_BYTE_KEY = "byte"
#: The byte that run writes, which is no UTF-8.
AUTH_KEY_BYTE = b"\xff"
HOSTILE_CASES = (tuple("lg-key-%s-%s" % (name, form) for name in KEY_SIZES for form in KEY_FORMS)
                 + tuple("lg-key-" + name for name in KEY_VALUES)
                 + ("lg-auth-key-byte", "lg-surrogate-access-token", "lg-surrogate-token-type", "lg-surrogate-id-token",
                    "lg-state-ff", "lg-state-twice", "lg-blocked-port"))
TOKEN_TYPE_CASES = ("lg-token-type-kelvin",)
#: The cases of the audiences of an ID token the code grant refuses, an `aud` list without the client, a list of two
#: with no `azp` and an `azp` of another client.
AUDIENCE_CASES = ("lg-aud-no-client", "lg-aud-no-azp", "lg-azp-other")
#: The cases of the forms of base64url, one the decoder takes and two it refuses.
BASE64URL_CASES = ("lg-b64-lenient", "lg-malformed-b64-length", "lg-malformed-b64-alphabet")
#: The case of an ID token of Zitadel with the `at_hash` of another token.
AT_HASH_CASES = ("lg-zitadel-at-hash",)
#: The cases that run beside the 25 and count apart.
EXTRA_CASES = (("lg-13b", "lg-hostile", "lg-40", "lg-41") + ROW_CASES + HOSTILE_CASES + TOKEN_TYPE_CASES
               + AUDIENCE_CASES + BASE64URL_CASES + AT_HASH_CASES)
#: The cookie of the PKCE verifier and the cookie of the session.
VERIFIER_COOKIE = "aosabi_code_verifier"
SESSION_COOKIE = "aosabi-auth"
#: A cookie of the host beside the cookies the routes read, which a browser sends with them.
OTHER_COOKIE = "other=1"


@dataclass(frozen=True)
class Step:
    """One request of a run: the case it belongs to, the scenario the issuer answers it under and what it pins.

    `carry` holds triples of a placeholder, the id of an earlier step and the
    name of a cookie that step set, whose value replaces `{placeholder}` in the
    request. `hostile` is the hostile mode of the issuer during the step, and a
    step with no request, `http` None, judges the records of the step `checks`
    names.
    """

    id: str
    case: str
    scenario: str | None
    http: HttpCase | None
    pins: str
    carry: tuple = ()
    hands_challenge: bool = False
    hostile: str | None = None
    checks: str | None = None

    def raw(self, values: dict | None = None) -> bytes:
        """The request as the raw client writes it, with every placeholder of `carry` replaced from `values`."""
        if self.http is None:
            raise ValueError("the step %s sends no request" % self.id)
        data = self.http.raw()
        for placeholder, _, _ in self.carry:
            if values is None or placeholder not in values:
                raise KeyError("the step %s needs the value of {%s}" % (self.id, placeholder))
            data = data.replace(b"{%s}" % placeholder.encode("ascii"), values[placeholder].encode("latin-1"))
        return data


@dataclass(frozen=True)
class Run:
    """One variant served fresh on both sides with a mocked issuer started fresh, and its steps in order.

    `issuer` is `running`, `closed` for a run whose issuer port nothing
    listens on, or `blocked` for a run whose issuer listens on a port of the
    list of bad ports of the Fetch standard. `key` names the key `AUTH_KEY`
    holds where it is not the key of the run: a key case of `KEY_SIZES` with
    its form, `rsa2560-pkcs8`, a value of `KEY_VALUES`, or
    `AUTH_KEY_BYTE_KEY`.
    """

    id: str
    fixture: str
    variant: str
    issuer: str
    steps: tuple
    node_envs: tuple = NODE_ENVS
    key: str | None = None

    @property
    def first_scenario(self) -> str | None:
        """The scenario the issuer starts in."""
        return next((step.scenario for step in self.steps if step.scenario is not None), None)

    def cases(self) -> list:
        """The case ids of the run, each once, in their order."""
        seen = []
        for step in self.steps:
            if step.case not in seen:
                seen.append(step.case)
        return seen


def request(name: str, method: str, target: str, headers=(), body: bytes = b"") -> HttpCase:
    """A case with `Host: app.example.invalid` first, its `x-request-id` and its length, the rule of tests/gen/http.py."""
    fields = [("Host", APP_HOST), ("x-request-id", "%s-%s" % (FAMILY.lower(), name))]
    fields.extend(headers)
    if body and not any(key.lower() in ("content-length", "transfer-encoding") for key, _ in fields):
        fields.append(("Content-Length", str(len(body))))
    canonical_request = {
        "method": method,
        "target": target,
        "headers": [list(pair) for pair in fields],
        "body_hex": body.hex(),
    }
    ident = hashlib.sha256(canonical(canonical_request)).hexdigest()
    return HttpCase(ident, FAMILY, name, method, target, tuple(fields), body)


def get(name: str, target: str, headers=()) -> HttpCase:
    """A `GET` of `target`."""
    return request(name, "GET", target, headers)


def graphql(name: str, document: str, headers=(), target: str = "/graphql") -> HttpCase:
    """A `POST` of `document` to `target`."""
    return request(name, "POST", target, [("Content-Type", "application/json"), *headers], json_body(document))


def verifier_cookie(placeholder: str = "verifier") -> list:
    """The `Cookie` line of a verifier a login set."""
    return [("Cookie", "%s={%s}" % (VERIFIER_COOKIE, placeholder))]


def cookie_fields(*values) -> list:
    """One `Cookie` line per value, in order, the fields of a request that carries its cookies in several."""
    return [("Cookie", value) for value in values]


def _callback(step_id: str, case: str, scenario: str, query: str, pins: str, carry_from: str,
              cookie: bool = True) -> Step:
    headers = verifier_cookie() if cookie else []
    carry = (("verifier", carry_from, VERIFIER_COOKIE),) if cookie else ()
    return Step(step_id, case, scenario, get(step_id, "/auth/callback" + query, headers), pins, carry)


def _login(step_id: str, case: str, scenario: str | None, pins: str, endpoint: str = "/auth",
           hands_challenge: bool = True, hostile: str | None = None) -> Step:
    return Step(step_id, case, scenario, get(step_id, endpoint + "/login"), pins, (), hands_challenge, hostile)


def login_runs() -> list:
    """Every run of zitadel-login-app in the order of the plan's table, the extra and the hostile run included."""
    fixture = "zitadel-login-app"
    runs = []

    def add(run_id, variant, steps, issuer="running", key=None):
        runs.append(Run(run_id, fixture, variant, issuer, tuple(steps), key=key))

    q = "?code=c1&state=s1"
    add("login-default-ok", "default", [
        _login("lg-01", "lg-01", "ok", "302, the Location with the parameters of fact 8 in their order, the verifier "
               "cookie, one discovery request"),
        _login("lg-02", "lg-02", "ok", "no second discovery, fact 2", hands_challenge=False),
        Step("lg-03", "lg-03", "ok", get("lg-03", "/auth/logout"), "302 to / and aosabi-auth=; Max-Age=0; Path=/"),
        _callback("lg-04", "lg-04", "ok", "", "400 Missing authorization code or state", "lg-01", cookie=False),
        _callback("lg-05", "lg-05", "ok", "?code=c1", "400 Missing authorization code or state", "lg-01",
                  cookie=False),
        _callback("lg-06", "lg-06", "ok", q, "400 Missing code verifier", "lg-01", cookie=False),
        _callback("lg-07", "lg-07", "ok", q, "the token request of facts 12 and 13, 302 to / and "
                  "aosabi-auth=<token>; Max-Age=120; Path=/; HttpOnly", "lg-01"),
        _callback("lg-12", "lg-12", "ok", q + "&session_state=x1", "the extra parameter in the token body",
                  "lg-01"),
        _callback("lg-13", "lg-13", "ok", "?error=access_denied&state=s1",
                  "400 Missing authorization code or state, the query carries no code", "lg-01"),
        _callback("lg-13b", "lg-13b", "ok", "?code=c1&error=access_denied&state=s1",
                  "the authorization error of the callback, 500, and no token request", "lg-01"),
        _callback("lg-14", "lg-14", "ok", "?code=c1&state=s9", "302 all the same for a state the login never "
                  "issued", "lg-01"),
    ])
    for case, scenario, pins in (
        ("lg-08", "no_expiry", "Max-Age=3600"),
        ("lg-09", "invalid_grant", "500 and the log record of fact 15"),
        ("lg-10", "id_token", "302 with a valid ID token in the answer"),
        ("lg-11", "id_token_bad_aud", "500 for an ID token of another audience"),
    ):
        add("login-default-" + scenario, "default", [
            _login(case + "-login", case, scenario, "the login whose verifier the callback of %s carries" % case),
            _callback(case, case, scenario, q, pins, case + "-login"),
        ])
    add("login-default-wrong_issuer", "default", [
        _login("lg-20", "lg-20", "wrong_issuer", "500 and one discovery request", hands_challenge=False),
        _login("lg-21", "lg-21", "wrong_issuer", "500 again and no second discovery request",
               hands_challenge=False),
    ])
    add("login-default-discovery_500", "default", [
        _login("lg-22", "lg-22", "discovery_500", "500 while discovery answers 500", hands_challenge=False),
        _login("lg-23", "lg-23", "ok", "500 after the issuer recovers and no new discovery",
               hands_challenge=False),
    ])
    add("login-default-wrong_issuer-callback", "default", [
        _callback("lg-24", "lg-24", "wrong_issuer", "", "500 and not 400, the bootstrap runs before the query, "
                  "fact 10", "lg-24", cookie=False),
    ])
    add("login-http_issuer", "http_issuer", [
        _login("lg-30", "lg-30", "ok", "500 and no request at the issuer, fact 5", hands_challenge=False),
    ])
    add("login-bad_key", "bad_key", [
        _login("lg-31", "lg-31", "ok", "500 and Error reading AUTH_KEY. Make sure it is valid JSON.",
               hands_challenge=False),
    ])
    add("login-key_file", "key_file", [
        _login("lg-32", "lg-32", "ok", "302 from the key file", hands_challenge=False),
    ])
    add("login-no_key", "no_key", [
        _login("lg-33", "lg-33", "ok", "500 and Error reading key file \"key.json\". Make sure it exists and is "
               "valid JSON.", hands_challenge=False),
    ])
    add("login-endpoint", "endpoint", [
        _login("lg-34", "lg-34", "ok", "the redirect_uri under the custom endpoint", endpoint="/login-custom",
               hands_challenge=False),
    ])
    add("login-pkcs8_key", "pkcs8_key", [
        _login("lg-35-login", "lg-35", "ok", "the login under the PKCS#8 key of fact 4"),
        _callback("lg-35-callback", "lg-35", "ok", q, "the PKCS#8 key of fact 4 signing the assertion",
                  "lg-35-login"),
    ])
    add("login-hostile", "default", [
        _login("lg-hostile", "lg-hostile", "ok", "500 while the issuer cuts the record of its answer",
               hands_challenge=False, hostile="answer"),
        Step("lg-hostile-after", "lg-hostile", "ok", graphql("lg-hostile-after", "{__typename}"),
             "the side answers {__typename} after the cut"),
    ])
    verifier = "%s={verifier}" % VERIFIER_COOKIE
    carried = (("verifier", "lg-40-login", VERIFIER_COOKIE),)
    add("login-cookie-fields", "default", [
        _login("lg-40-login", "lg-40", "ok", "the login whose verifier the callbacks of lg-40 and lg-41 carry in one "
               "of two Cookie fields"),
        Step("lg-40", "lg-40", "ok", get("lg-40", "/auth/callback" + q, cookie_fields(OTHER_COOKIE, verifier)),
             "the token request of lg-07 and 302 with aosabi-auth=<token>, the verifier in the second of two Cookie "
             "fields, which the Headers joins with 0x3B 0x20 as undici joins them", carried),
        Step("lg-41", "lg-41", "ok", get("lg-41", "/auth/callback" + q, cookie_fields(verifier, OTHER_COOKIE)),
             "the token request of lg-07 and 302 with aosabi-auth=<token>, the verifier in the first of two Cookie "
             "fields", carried),
    ])
    carried = "lg-11-iss-login"
    add("login-default-id_token-claims", "default", [
        _login(carried, "lg-11-iss", "id_token_bad_iss", "the login whose verifier every callback of the run "
               "carries"),
        _callback("lg-11-iss", "lg-11-iss", "id_token_bad_iss", q, "500 for an ID token of another issuer, C80",
                  carried),
        _callback("lg-11-exp", "lg-11-exp", "id_token_expired", q, "500 for an ID token whose exp is past, C80",
                  carried),
        _callback("lg-11-iat", "lg-11-iat", "id_token_no_iat", q, "500 for an ID token with no iat, C80", carried),
        _callback("lg-11-sub", "lg-11-sub", "id_token_no_sub", q, "500 for an ID token with no sub, C80", carried),
        _callback("lg-zitadel", "lg-zitadel", "id_token_zitadel", q, "302 with an ID token in the shape of Zitadel, "
                  "aud the client and its project with azp the client", carried),
        _callback("lg-malformed-id-token", "lg-malformed-id-token", "id_token_malformed", q, "500 for an ID token "
                  "whose payload is no JSON", carried),
        _callback("lg-malformed-token", "lg-malformed-token", "token_malformed", q, "500 for a token answer cut in "
                  "its middle", carried),
        _callback("lg-aud-no-client", "lg-aud-no-client", "id_token_aud_no_client", q, "500 for an ID token whose "
                  "aud list does not hold the client", carried),
        _callback("lg-aud-no-azp", "lg-aud-no-azp", "id_token_aud_no_azp", q, "500 for an ID token whose aud list "
                  "holds two audiences and which names no azp", carried),
        _callback("lg-azp-other", "lg-azp-other", "id_token_azp_other", q, "500 for an ID token whose azp names "
                  "another client", carried),
        _callback("lg-b64-lenient", "lg-b64-lenient", "id_token_b64_lenient", q, "302 with an ID token whose payload "
                  "holds +, /, white space and padding, which the forgiving decode takes", carried),
        _callback("lg-malformed-b64-length", "lg-malformed-b64-length", "id_token_b64_length", q, "500 for an ID "
                  "token whose payload leaves one character over a multiple of four", carried),
        _callback("lg-malformed-b64-alphabet", "lg-malformed-b64-alphabet", "id_token_b64_alphabet", q, "500 for an "
                  "ID token whose payload holds characters outside every alphabet of base64", carried),
        _callback("lg-zitadel-at-hash", "lg-zitadel-at-hash", "id_token_zitadel_at_hash", q, "302 with an ID token "
                  "of Zitadel whose at_hash is the hash of another token, which no side checks", carried),
    ])
    add("login-default-discovery_malformed", "default", [
        _login("lg-malformed-discovery", "lg-malformed-discovery", "discovery_malformed", "500 for a discovery "
               "document cut in its middle", hands_challenge=False),
    ])
    add("login-default-refused", "default", [
        _login("lg-refused", "lg-refused", None, "500 and fetch failed with the issuer's port closed, C81",
               hands_challenge=False),
    ], issuer="closed")
    for name, (bits, primes) in KEY_SIZES.items():
        for form in KEY_FORMS:
            case = "lg-key-%s-%s" % (name, form)
            add("login-key-%s-%s" % (name, form), "default", [
                _login(case + "-login", case, "ok", "the login under the key of %d bits and %d primes in the form "
                       "%s, whose import both sides keep" % (bits, primes, form)),
                _callback(case, case, "ok", q, "the token request whose client assertion the key of %d bits and %d "
                          "primes signs, or the refusal at signing" % (bits, primes), case + "-login"),
            ], key="%s-%s" % (name, form))
    for name in KEY_VALUES:
        add("login-key-" + name, "default", [
            _login("lg-key-" + name, "lg-key-" + name, "ok", "500 and the error of createPrivateKey for the key value "
                   "%s" % name, hands_challenge=False),
        ], key=name)
    add("login-auth-key-byte", "default", [
        _login("lg-auth-key-byte", "lg-auth-key-byte", "ok", "302 with the key of AUTH_KEY, whose byte that is no "
               "UTF-8 node reads as U+FFFD", hands_challenge=False),
    ], key=AUTH_KEY_BYTE_KEY)
    carried = "lg-surrogate-login"
    add("login-lone-surrogate", "default", [
        _login(carried, "lg-surrogate-access-token", "surrogate_access_token", "the login whose verifier every "
               "callback of the run carries"),
        _callback("lg-surrogate-access-token", "lg-surrogate-access-token", "surrogate_access_token", q, "500 and the "
                  "URIError of encodeURIComponent, since setCookie meets the lone surrogate of the access token",
                  carried),
        _callback("lg-surrogate-token-type", "lg-surrogate-token-type", "surrogate_token_type", q, "500 for a token "
                  "type with a lone surrogate, which names no type", carried),
        _callback("lg-surrogate-id-token", "lg-surrogate-id-token", "surrogate_id_token", q, "500 for an ID token "
                  "with a lone surrogate in its payload, which decodes to no JSON", carried),
    ])
    carried = "lg-state-login"
    add("login-state-odd", "default", [
        _login(carried, "lg-state-ff", "ok", "the login whose verifier both callbacks of the run carry"),
        _callback("lg-state-ff", "lg-state-ff", "ok", "?code=c1&state=%FF", "500 and no token request, since the "
                  "query of hono keeps %FF and the grant reads U+FFFD, R27", carried),
        _callback("lg-state-twice", "lg-state-twice", "ok", "?code=c1&state=s1&state=s2", "500 and no token "
                  "request, since the grant takes no state twice, R27", carried),
    ])
    add("login-blocked-port", "default", [
        _login("lg-blocked-port", "lg-blocked-port", "ok", "500 and fetch failed with no request at the issuer, "
               "whose port the Fetch standard blocks, R23", hands_challenge=False),
    ], issuer="blocked")
    case = "lg-token-type-kelvin"
    add("login-token-type-kelvin", "default", [
        _login(case + "-login", case, "token_type_kelvin", "the login whose verifier the callback of %s carries"
               % case),
        _callback(case, case, "token_type_kelvin", q, "500 after one token request, the token type Bearer with U+212A "
                  "KELVIN SIGN, which lowers to bearerk, a type neither side takes", case + "-login"),
    ])
    return runs


def key_case(name: str):
    """The size, the count of primes and the form of the key case `name`, `rsa2560-pkcs8`, or None for another key."""
    size, _, form = name.rpartition("-")
    if size in KEY_SIZES and form in KEY_FORMS:
        return (*KEY_SIZES[size], form)
    return None


def refusal_step(step_id: str) -> str | None:
    """The callback of `KEY_REFUSED` in the form of the callback `step_id` of a key of `KEY_NOT_SIGNED`, or None.

    The twin answers the callback of such a key as both sides answer the
    callback of the key both refuse at signing, the golden it is held to.
    """
    for name in KEY_NOT_SIGNED:
        for form in KEY_FORMS:
            if step_id == "lg-key-%s-%s" % (name, form):
                return "lg-key-%s-%s" % (KEY_REFUSED, form)
    return None


def login_h2_run() -> Run:
    """The run of step 9 of `61_oidc_login`: a login and its callback with the verifier in one field and in one of
    two fields in both orders, which the twin answers over HTTP/2 and the reference over HTTP/1.1.

    The run has no golden. The reference serves HTTP/1.1 alone, so each answer of the twin over HTTP/2 is held to the
    reference's answer over HTTP/1.1 to the same fields.
    """
    verifier = "%s={verifier}" % VERIFIER_COOKIE
    carried = (("verifier", "lg-h2-login", VERIFIER_COOKIE),)
    callback = "/auth/callback?code=c1&state=s1"
    return Run("login-h2-cookie-fields", "zitadel-login-app", "default", "running", (
        _login("lg-h2-login", "lg-h2", "ok", "302 and the verifier the callbacks carry"),
        Step("lg-h2-one", "lg-h2", "ok", get("lg-h2-one", callback, cookie_fields(verifier)),
             "302 and aosabi-auth=<token>, the verifier in one field", carried),
        Step("lg-h2-second", "lg-h2", "ok", get("lg-h2-second", callback, cookie_fields(OTHER_COOKIE, verifier)),
             "302 and aosabi-auth=<token>, the verifier in the second of two fields", carried),
        Step("lg-h2-first", "lg-h2", "ok", get("lg-h2-first", callback, cookie_fields(verifier, OTHER_COOKIE)),
             "302 and aosabi-auth=<token>, the verifier in the first of two fields", carried),
    ))


def login_steps() -> list:
    """Every step of every login run, in order."""
    return [step for run in login_runs() for step in run.steps]


def counted_cases(runs, extra=EXTRA_CASES) -> list:
    """The case ids of `runs` that count toward an exit number, each once, the cases of `extra` left out."""
    seen = []
    for run in runs:
        for case in run.cases():
            if case not in extra and case not in seen:
                seen.append(case)
    return seen


def login_manifest() -> dict:
    """The counts of the login runs, which the notebook prints and asserts."""
    runs = login_runs()
    cases = counted_cases(runs)
    if len(cases) != CASE_COUNT:
        raise AssertionError("the login runs hold %d cases where the plan counts %d" % (len(cases), CASE_COUNT))
    return {
        "runs": len(runs),
        "steps": sum(len(run.steps) for run in runs),
        "cases": len(cases),
        "extra": list(EXTRA_CASES),
        "variants": sorted({run.variant for run in runs}),
        "scenarios": sorted({step.scenario for run in runs for step in run.steps if step.scenario}),
    }


__all__ = [
    "APP_HOST",
    "AT_HASH_CASES",
    "AUDIENCE_CASES",
    "AUTH_KEY_BYTE",
    "AUTH_KEY_BYTE_KEY",
    "BASE64URL_CASES",
    "CASE_COUNT",
    "EXTRA_CASES",
    "FAMILY",
    "HOSTILE_CASES",
    "KEY_FORMS",
    "KEY_NOT_SIGNED",
    "KEY_REFUSED",
    "KEY_SIZES",
    "KEY_VALUES",
    "NODE_ENVS",
    "OTHER_COOKIE",
    "ROW_CASES",
    "Run",
    "SESSION_COOKIE",
    "Step",
    "TOKEN_TYPE_CASES",
    "VERIFIER_COOKIE",
    "cookie_fields",
    "counted_cases",
    "get",
    "graphql",
    "key_case",
    "login_h2_run",
    "login_manifest",
    "login_runs",
    "login_steps",
    "refusal_step",
    "request",
    "verifier_cookie",
]
