# SPDX-License-Identifier: MIT
"""`61_oidc_login` and `63_zitadel_auth` serve `zitadel-login-app` and `zitadel-auth-app` on both sides with
`OIDC_ISSUER` naming this issuer, `https://127.0.0.1:<port>`, so every request of the login routes and of the
provider reaches a harness process on loopback and nothing else. The issuer is harness code and never a golden
source: the golden is what the reference sends and answers.

It is built on `http.server` and `ssl` of the standard library, and no Python package enters tests/pyproject.toml.
The certificate of 127.0.0.1 and the CA come from tests/harness/tls.py, which the reference trusts through
`NODE_EXTRA_CA_CERTS` and the twin through `AOSABI_EXTRA_CA_CERTS`. Each connection runs its TLS session through
`ssl.SSLObject` over two memory buffers, the way tests/harness/mock_ingest.py runs it, and hands the decrypted
stream to the request handler of `http.server`, so the issuer sees each record it sends and its hostile mode can cut
one: under `hostile="answer"` it reads and records the request and sends the first half of the record that carries
the answer before it closes the socket, and under `hostile="handshake"` it sends the first half of its first flight
and closes.

That closing issuer is the default and the issuer of `61_oidc_login` and `63_zitadel_auth`. Each request keeps its
span, from the accept of its connection, or from the first byte of a later request on a kept connection, to the end
of its answer, the close of the connection where the answer closes it, so the wait of a thread of the issuer for its
turn counts to the issuer, where a span from the start of that thread booked it to the side. Where a notebook sets
`watch` to a reader of the processor time of the side the issuer serves, each span also keeps that time at both of
its ends, so the work of the side within the span, its part of each TLS handshake among it, is booked to the side
and not to the issuer. `hold` keeps the answers of an endpoint until `release`, so every request a side sends
meanwhile waits on one call to the issuer.

The issuer answers each endpoint from its scenario, which a notebook sets with `control` in process or with a `POST`
of JSON to the control endpoint `/__harness/control`, `{"scenario": ..., "challenge": ..., "hostile": ...}`, each
key optional. The scenarios are those of the plan's tables: `ok`, `no_expiry`, `invalid_grant`, `id_token`,
`id_token_bad_aud`, `wrong_issuer` and `discovery_500` of the login routes, and `introspect_ok`,
`introspect_inactive`, `introspect_no_sub`, `introspect_401`, `introspect_400_json`, `introspect_jwt`,
`userinfo_sub_mismatch`, `userinfo_challenge`, `userinfo_jwt`, `userinfo_not_json`, `roles_numeric`, `roles_string`,
`roles_array` and `login_and_introspect` of the provider. The discovery of a login scenario names the endpoints the
login routes reach, and the discovery of a provider scenario adds `introspection_endpoint`, `userinfo_endpoint` and
the signing algorithms of a JWT answer, which oauth4webapi needs to read a JWT user info answer at all.

Beside them stands a scenario for each token and refusal capture rows C80 to C82 hold, so every observation of those
rows runs against both sides: the ID tokens of `id_token_bad_iss`, `id_token_expired`, `id_token_no_iat` and
`id_token_no_sub`, the JWT answers of `userinfo_jwt_bad_aud`, `userinfo_jwt_bad_iss`, `introspect_jwt_bad_aud`,
`introspect_jwt_bad_iss`, `introspect_jwt_no_iat` and `introspect_jwt_no_typ`, and each JWT scenario of the provider
once more with the suffix `_plain`, whose discovery is the plain document of the capture issuer, the issuer, the
four endpoints and the key set with no signing algorithm and no other field. A scenario answers as the scenario its
name names without the suffix, and the suffix changes the discovery alone, which a side reads once per process, so a
run of the plain discovery takes such a scenario from its first step on. The ID token of `id_token_malformed`, whose
payload decodes to no JSON, carries no signature of the key either, and every other JWT the issuer writes carries a
signature of its key.

The shapes Zitadel writes stand as scenarios too, built as the code of Zitadel builds them: `id_token_zitadel`, the
ID token of a code exchange, the claims in the order of the struct of zitadel/oidc, `aud` the client and its
project, `amr`, `azp` and `client_id` the client, the `at_hash` of the access token and the session `sid`, and no
`acr`, `id_token_zitadel_at_hash`, the same with the `at_hash` of another token, which neither side checks,
`introspect_zitadel`, an introspection answer with the claims Zitadel writes for the scope `openid email profile` of
a person and both claims of the roles, its keys sorted, as the map of Go writes them once a claim beyond the struct
is present, and its body closed by the line feed of its encoder, and `roles_project` and `roles_per_project`, the
roles under `urn:zitadel:iam:org:project:roles` and under the form of one project alone. Three ID tokens in the
shape of Zitadel hold the audiences the code grant refuses, `id_token_aud_no_client`, a list without the client,
`id_token_aud_no_azp`, a list of two with no `azp`, and `id_token_azp_other`, an `azp` of another client, and
`id_token_b64_lenient` holds a payload in the alphabet of base64 with `+`, `/`, white space and padding, which the
forgiving decode of both sides takes. The scenarios of `MALFORMED` answer bytes no parser of a side takes, one or
more for each parser of an answer and each form it refuses, which `61_oidc_budget` requires per parser file and
form.

escapes as `\\ud800`, and `roles_value_of` answers an introspection whose roles
claim holds an own key `valueOf`.

Two scenarios hold U+212A KELVIN SIGN, whose lower case of `toLowerCase` is the ASCII `k`, in a type each side
lowers before it compares: `token_type_kelvin` answers the token request with the token type `Bearer` and the sign,
which lowers to `bearerk`, a type no side takes, and `introspect_jwt_typ_kelvin` answers the introspection with a
JWT whose `typ` is `Application/To<U+212A>en-Introspection+JWT`, which lowers to
`application/token-introspection+jwt`, the type the check takes.

The token and the introspection endpoint verify the client assertion before they answer: the signature with
`openssl dgst -sha256 -verify` and the public half of the client key whose `kid` the header names, `exp - iat`
against 60, `aud` against the issuer, an `nbf` that is the integer `iat`, the shape both libraries write, and the
freshness. A side on the clock of the host writes the floor of an earlier time and never trips it, while a ceiling
in the place of the floor or a tolerance in the place of a skew does. R25 of the audit adds an `exp` at or before
the issuer's clock, which holds the past side, since `exp - iat` is held to 60, and a `jti` the issuer has seen in
an earlier assertion, and the two catch a side whose clock reads 0. A request whose assertion fails answers 401
`invalid_client`, and its record and the description of the answer say why. The record keeps the drawn `jti` beside
the masked claims, so a notebook can show that each assertion draws its own. Every ID token, JWT introspection
answer and JWT user info answer but those of the form of the capture and the ID token of `id_token_malformed`, which
carries no signature of the key, is signed with `openssl dgst -sha256 -sign` and an RSA key the issuer writes per
run under target/harness/issuer/.

Every request is recorded with its method, its target, its header lines in wire order and spelling, its body, the
endpoint it reached, the scenario that answered it, and the outcome of the assertion and the PKCE check. A control
request is recorded apart and never among the requests of a side. The issuer binds only the ports of `PORT_RANGES`,
4655 to 4674 of `61_oidc_login` and 4940 to 4949 of `63_zitadel_auth`, plus the `port_offset` of the runner, or with
`blocked` a port of `BLOCKED_PORTS`, the ports of the list of bad ports of the Fetch standard a user may bind, where
a case puts the issuer that the fetch of each side refuses to reach.
"""

from __future__ import annotations

import base64
import hashlib
import http.server
import io
import json
import re
import secrets
import socket
import ssl
import subprocess
import threading
import time
from dataclasses import dataclass, field
from pathlib import Path
from urllib.parse import parse_qsl, urlsplit

from .paths import paths
from .tls import ISSUER_HOST

#: The host the issuer binds, the loopback of IPv4.
HOST = "127.0.0.1"
#: The ports the issuer may bind before the offset of the runner, both ends included.
PORT_RANGES = ((4655, 4674), (4940, 4949))
#: The ports of the list of bad ports of the Fetch standard above 1023, which a user may bind, and which a case
#: binds with `blocked` as they are, outside the ranges and the offset.
BLOCKED_PORTS = (1719, 1720, 1723, 2049, 3659, 4045, 4190, 5060, 5061, 6000, 6566, 6665, 6666, 6667, 6668, 6669,
                 6679, 6697, 10080)
#: The path of the control endpoint, which no side ever reaches.
CONTROL_PATH = "/__harness/control"
#: The modes of the connections of the issuer: one request a connection, every answer with `connection: close`,
#: or a connection kept for the next request of its client.
CONNECTION_MODES = ("close", "keep-alive")
#: The counts the issuer sets as `num_tickets` of its context: none, so the first record after the handshake
#: carries the answer, or 2, the default of OpenSSL, which then sends two TLS 1.3 session tickets after a full
#: handshake and one after a resumed handshake.
TICKET_COUNTS = (0, 2)
#: The host names the issuer may take in place of its IP literal, the name of the certificate of tls.py.
HOST_NAMES = (ISSUER_HOST,)
#: The seconds a held answer waits at most for its release, so no thread of the issuer waits forever.
HOLD_S = 300.0
#: The paths of the endpoints under the issuer, the paths Zitadel serves.
PATHS = {
    "discovery": "/.well-known/openid-configuration",
    "authorize": "/oauth/v2/authorize",
    "token": "/oauth/v2/token",
    "introspect": "/oauth/v2/introspect",
    "userinfo": "/oidc/v1/userinfo",
    "keys": "/oauth/v2/keys",
}
#: The scenarios of the login routes, whose discovery names no introspection or user info endpoint.
LOGIN_SCENARIOS = (
    "ok",
    "no_expiry",
    "invalid_grant",
    "id_token",
    "id_token_bad_aud",
    "wrong_issuer",
    "discovery_500",
    "id_token_bad_iss",
    "id_token_expired",
    "id_token_no_iat",
    "id_token_no_sub",
    "id_token_zitadel",
    "id_token_malformed",
    "token_malformed",
    "discovery_malformed",
    "surrogate_access_token",
    "surrogate_token_type",
    "surrogate_id_token",
    "token_type_kelvin",
    "id_token_aud_no_client",
    "id_token_aud_no_azp",
    "id_token_azp_other",
    "id_token_b64_lenient",
    "id_token_b64_length",
    "id_token_b64_alphabet",
    "id_token_zitadel_at_hash",
)
#: The JWT scenarios of the provider, which C82 holds behind the discovery of both forms.
JWT_SCENARIOS = (
    "introspect_jwt",
    "introspect_jwt_bad_aud",
    "introspect_jwt_bad_iss",
    "introspect_jwt_no_iat",
    "introspect_jwt_no_typ",
    "userinfo_jwt",
    "userinfo_jwt_bad_aud",
    "userinfo_jwt_bad_iss",
)
#: The suffix of a scenario whose discovery names no signing algorithm.
PLAIN = "_plain"
#: The scenarios of the provider, whose discovery adds those endpoints.
PROVIDER_SCENARIOS = (
    "introspect_ok",
    "introspect_inactive",
    "introspect_no_sub",
    "introspect_401",
    "introspect_400_json",
    "introspect_jwt",
    "userinfo_sub_mismatch",
    "userinfo_challenge",
    "userinfo_jwt",
    "userinfo_not_json",
    "roles_numeric",
    "roles_string",
    "roles_array",
    "login_and_introspect",
    "introspect_jwt_bad_aud",
    "introspect_jwt_bad_iss",
    "introspect_jwt_no_iat",
    "introspect_jwt_no_typ",
    "userinfo_jwt_bad_aud",
    "userinfo_jwt_bad_iss",
    *(name + PLAIN for name in JWT_SCENARIOS),
    "introspect_zitadel",
    "roles_project",
    "roles_per_project",
    "introspect_malformed",
    "roles_value_of",
    "introspect_jwt_typ_kelvin",
)
#: Every scenario. `discovery_500` serves both halves, cases lg-22 and z-16.
SCENARIOS = LOGIN_SCENARIOS + PROVIDER_SCENARIOS
#: The scenarios that answer bytes no parser takes, each with the endpoint that answers them and the form of what it
#: answers: a compact JWT whose payload decodes to no JSON, `jwt`, whose payload is valid claims in base64url with
#: characters past a multiple of four that leave one, `jwt base64url length`, or with characters outside the alphabet
#: of base64 among them, `jwt base64url alphabet`, which the forgiving decode of both sides refuses, or a JSON document
#: cut in its middle, `json`, `not json` for user info.
MALFORMED = {
    "id_token_malformed": ("token", "jwt"),
    "token_malformed": ("token", "json"),
    "discovery_malformed": ("discovery", "json"),
    "introspect_malformed": ("introspect", "json"),
    "userinfo_not_json": ("userinfo", "json"),
    "id_token_b64_length": ("token", "jwt base64url length"),
    "id_token_b64_alphabet": ("token", "jwt base64url alphabet"),
}
#: The hostile modes.
HOSTILE_MODES = (None, "answer", "handshake")
#: The type a client assertion names.
ASSERTION_TYPE = "urn:ietf:params:oauth:client-assertion-type:jwt-bearer"
#: The lifetime of a client assertion both libraries write, `exp - iat`.
ASSERTION_LIFETIME = 60
#: The seconds an `iat` may lie ahead of the issuer's clock, 0, the clock skew of a client of Zitadel by default,
#: whose token endpoint refuses an `iat` ahead of its clock rounded to the second.
ASSERTION_SKEW = 0
#: The access token every token answer carries, which the session cookie `aosabi-auth` then holds.
ACCESS_TOKEN = "t-login"
#: The lone surrogate the scenarios of the prefix `surrogate_` write into a token answer, which its JSON escapes.
LONE_SURROGATE = "\ud800"
#: U+212A KELVIN SIGN, which the scenarios of the suffix `_kelvin` write into a type, whose lower case is `k`.
KELVIN_SIGN = "\u212a"
#: The lifetime of the access token of `ok`, the `Max-Age=120` of case lg-07.
EXPIRES_IN = 120
#: The subject of every active token but `TOKENS` names another, and the subject of every ID token.
SUBJECT = "u1"
#: The email every user info answer carries, a neutral test value.
EMAIL = "probe@example.invalid"
#: The claim of the roles, the name the provider reads.
ROLES_CLAIM = "urn:zitadel:iam:org:projects:roles"
#: The id of the project of the client, in the form of the ids Zitadel writes, a neutral test value.
PROJECT_ID = "300000000000000001"
#: The id of the organisation a role of Zitadel's own shape names, a neutral test value.
ORG_ID = "300000000000000002"
#: The claims of the roles Zitadel sends, the name of the project the token is for and the form of one project,
#: which the provider does not read.
ZITADEL_ROLES_CLAIM = "urn:zitadel:iam:org:project:roles"
ZITADEL_PROJECT_ROLES_CLAIM = "urn:zitadel:iam:org:project:%s:roles" % PROJECT_ID
#: The roles of Zitadel's own shape, a role with the organisations that grant it and their primary domain.
ZITADEL_ROLES = {"admin": {ORG_ID: "example.invalid"}, "viewer": {ORG_ID: "example.invalid"}}
#: The claim of the organisation, the tenant mapper of the variant `mapped` reads it.
RESOURCE_OWNER_CLAIM = "urn:zitadel:iam:user:resourceowner:id"
#: An issuer no side was given, the document of `wrong_issuer`.
OTHER_ISSUER = "https://issuer.example.invalid"
#: The key id of the signing key of the issuer.
ISSUER_KEY_ID = "issuer-key-1"
#: The key id and the signature of every token of the capture issuer, which no key verifies.
CAPTURE_KEY_ID = "capture-key"
CAPTURE_SIGNATURE = base64.urlsafe_b64encode(b"capture").rstrip(b"=").decode("ascii")
CAPTURE_FORMS = {"id_token": None, "userinfo_jwt": None, "introspect_jwt": "token-introspection+jwt"}
#: The session id of an ID token of Zitadel and the id of an access token in the form Zitadel writes,
#: `V2_<session>-at_<token>`, neutral test values.
ZITADEL_SESSION_ID = "300000000000000005"
ZITADEL_TOKEN_ID = "V2_300000000000000003-at_300000000000000004"
#: The profile and the email Zitadel writes for the scope `openid email profile` of a person, neutral test values.
ZITADEL_PROFILE = {"name": "Probe User", "given_name": "Probe", "family_name": "User", "locale": "en"}
#: The seconds of an ID token and of an access token of Zitadel, the twelve hours of their default lifetimes.
ZITADEL_TOKEN_S = 43200
#: The scenarios whose ID token takes the shape Zitadel writes, with the change each names.
ZITADEL_ID_TOKENS = ("id_token_zitadel", "id_token_zitadel_at_hash", "id_token_aud_no_client", "id_token_aud_no_azp",
                     "id_token_azp_other")
#: The white space the lenient form of a payload holds, a space, a tab, U+00A0, U+FEFF and U+2028 LINE SEPARATOR,
#: each of which the class `\s` of the decoder of oauth4webapi removes before `atob` on a node with no
#: `Uint8Array.fromBase64`.
B64_SPACES = (" ", "\t", "\u00a0", "\ufeff", "\u2028")
#: The roles claim of each scenario that sets one, by scenario.
SCENARIO_ROLES = {
    "roles_numeric": {"2": {"orgA": "example.invalid"}, "admin": {"orgA": "example.invalid"}, "1": {"orgA": "example.invalid"}},
    "roles_string": "ab",
    "roles_array": ["x", "y"],
    "roles_value_of": {"admin": {"orgA": "example.invalid"}, "valueOf": {"orgA": "example.invalid"}},
}
#: The tokens `introspect_ok` and the scenarios on its documents tell apart. Any other token is the subject `u1`
#: with the role `viewer`.
TOKENS = {
    "t-admin": {"sub": "u2", "roles": {"admin": {"orgA": "example.invalid"}}},
}
#: The roles claim of an active token that `TOKENS` does not name.
DEFAULT_ROLES = {"viewer": {"orgA": "example.invalid"}}
#: The seconds a connection may stay silent before the issuer closes it.
IDLE_S = 10.0
#: The seconds an openssl call may take.
OPENSSL_S = 30.0


def b64url(data: bytes) -> str:
    """`data` in base64url with no padding, the segment of a compact JWS."""
    return base64.urlsafe_b64encode(data).rstrip(b"=").decode("ascii")


def b64url_decode(text: str) -> bytes:
    """The bytes of a base64url segment with no padding."""
    return base64.urlsafe_b64decode(text + "=" * (-len(text) % 4))


def s256(verifier: str) -> str:
    """The S256 challenge of a PKCE verifier, base64url of its SHA-256 (RFC 7636 section 4.2)."""
    return b64url(hashlib.sha256(verifier.encode("ascii")).digest())


def allowed_port(port: int, port_offset: int = 0) -> bool:
    """Whether `port` lies in a range of `PORT_RANGES` once the offset of the runner is taken off."""
    base = port - port_offset
    return any(first <= base <= last for first, last in PORT_RANGES)


def challenge_of(location: str) -> str | None:
    """The `code_challenge` of the `Location` a login answered, or None when it names none."""
    for name, value in parse_qsl(urlsplit(location).query, keep_blank_values=True):
        if name == "code_challenge":
            return value
    return None


def base_scenario(scenario: str) -> str:
    """The scenario whose answers `scenario` gives, its name without the suffix of the plain discovery."""
    return scenario[: -len(PLAIN)] if scenario.endswith(PLAIN) else scenario


def _cut(document) -> bytes:
    """The JSON text of `document` cut in its middle, a body no JSON reader takes."""
    text = json.dumps(document, separators=(",", ":")).encode("utf-8")
    return text[: len(text) // 2]


def _openssl(argv: list, data: bytes) -> subprocess.CompletedProcess:
    return subprocess.run(["openssl", *[str(part) for part in argv]], input=data, capture_output=True,
                          timeout=OPENSSL_S, check=False)


def decode_jws(jws: str) -> tuple:
    """The header, the claims and the signature of a compact JWS, the claims in their order."""
    parts = jws.split(".")
    if len(parts) != 3:
        raise ValueError("a compact JWS has three segments and this has %d" % len(parts))
    header = json.loads(b64url_decode(parts[0]))
    claims = json.loads(b64url_decode(parts[1]))
    return header, claims, b64url_decode(parts[2])


def verify_jws(jws: str, public_key, work: Path) -> bool:
    """Whether the RS256 signature of `jws` verifies under the PEM file `public_key`, through openssl."""
    signing_input = jws.rsplit(".", 1)[0].encode("ascii")
    _, _, signature = decode_jws(jws)
    work.mkdir(parents=True, exist_ok=True)
    signature_file = work / ("sig-%s.bin" % secrets.token_hex(8))
    signature_file.write_bytes(signature)
    try:
        done = _openssl(["dgst", "-sha256", "-verify", public_key, "-signature", signature_file], signing_input)
    finally:
        signature_file.unlink(missing_ok=True)
    return done.returncode == 0 and b"Verified OK" in done.stdout


class SeenJti:
    """The `jti` values of the assertions an issuer has judged, each kept once."""

    def __init__(self) -> None:
        self._lock = threading.Lock()
        self._values: set = set()

    def first(self, jti: str) -> bool:
        """Keeps `jti` and answers whether it is new."""
        with self._lock:
            if jti in self._values:
                return False
            self._values.add(jti)
            return True

    def __len__(self) -> int:
        with self._lock:
            return len(self._values)


def sign_rs256(private_key, header: dict, claims: dict) -> str:
    """A compact JWS of `header` and `claims` signed RS256 with the PEM file `private_key`, through openssl."""
    signing_input = "%s.%s" % (b64url(json.dumps(header, separators=(",", ":")).encode("utf-8")),
                               b64url(json.dumps(claims, separators=(",", ":")).encode("utf-8")))
    done = _openssl(["dgst", "-sha256", "-sign", private_key], signing_input.encode("ascii"))
    if done.returncode != 0:
        raise RuntimeError("openssl signed nothing: %s" % done.stderr.decode("utf-8", "replace"))
    return signing_input + "." + b64url(done.stdout)


def capture_jwt(claims: dict, typ: str | None) -> str:
    """A compact JWS of `claims` in the form of the capture, the key id `capture-key`, `typ` where it is given, and the
    placeholder signature no key verifies."""
    header = {"alg": "RS256", "kid": CAPTURE_KEY_ID}
    if typ is not None:
        header["typ"] = typ
    return "%s.%s.%s" % (b64url(json.dumps(header, separators=(",", ":")).encode("utf-8")),
                         b64url(json.dumps(claims, separators=(",", ":")).encode("utf-8")), CAPTURE_SIGNATURE)


def at_hash(token: str) -> str:
    """The `at_hash` of `token` for RS256, base64url of the left half of its SHA-256, as Zitadel writes it."""
    return b64url(hashlib.sha256(token.encode("ascii")).digest()[:16])


def b64_segment(scenario: str, claims: dict) -> str:
    """The payload segment of `claims` in the form of the scenario `scenario` of the prefix `id_token_b64_`.

    `id_token_b64_lenient` writes base64 with `+`, `/` and its padding and
    puts a white space of `B64_SPACES` between each 16 characters, which the
    forgiving decode of both sides takes. `id_token_b64_length` writes
    base64url and one character more, so the length leaves one over a
    multiple of four, and `id_token_b64_alphabet` puts four characters `*`,
    which no alphabet of base64 holds, after the first eight, so a decoder
    that dropped what it does not take would read the claims whole.
    """
    text = json.dumps(claims, separators=(",", ":")).encode("utf-8")
    if scenario == "id_token_b64_lenient":
        segment = base64.b64encode(text).decode("ascii")
        if "+" not in segment or "/" not in segment or not segment.endswith("="):
            raise ValueError("the payload of %s holds no + and / or no padding: %s" % (scenario, segment))
        chunks = [segment[index:index + 16] for index in range(0, len(segment), 16)]
        return "".join(chunk + B64_SPACES[index % len(B64_SPACES)] for index, chunk in enumerate(chunks[:-1])) + \
            chunks[-1]
    segment = b64url(text)
    if len(segment) % 4:
        raise ValueError("the payload of %s leaves %d characters over a group" % (scenario, len(segment) % 4))
    if scenario == "id_token_b64_length":
        return segment + "A"
    if scenario == "id_token_b64_alphabet":
        return segment[:8] + "****" + segment[8:]
    raise ValueError("no form of a payload named %r" % scenario)


def client_assertion(client_key, audience: str, iat: int, exp: int | None = None, jti: str | None = None,
                     nbf: int | None = None) -> str:
    """A client assertion of `client_key` for `audience` in the shape both libraries write, its times and `jti` given.

    `exp` defaults to `iat` plus the lifetime of 60, `nbf` to `iat` and
    `jti` to a fresh draw of 32 bytes, so a notebook can write an assertion
    that is stale, early, apart from its `nbf` or repeats a `jti` and show
    that the issuer flags it.
    """
    claims = {"jti": jti or b64url(secrets.token_bytes(32)), "aud": audience,
              "exp": iat + ASSERTION_LIFETIME if exp is None else exp, "iat": iat, "nbf": iat if nbf is None else nbf,
              "iss": client_key.client_id, "sub": client_key.client_id}
    return sign_rs256(client_key.pkcs1, {"alg": "RS256", "kid": client_key.key_id}, claims)


def assertion_report(jws: str, client_keys, audience: str, work: Path, now: float | None = None,
                     seen=None) -> dict:
    """The client assertion decoded, verified and masked, the rule of section The oracle of the plan.

    The answer holds the header, the claims in their order with `jti`, `iat`,
    `nbf` and `exp` replaced by `<jti>`, `<iat>`, `<nbf>` and `<exp>`, the
    lifetime `exp - iat`, the key id the header names, whether a key of
    `client_keys` with that id verifies the signature, the drawn `jti` as
    sent, and `problems`, every rule the assertion breaks: a signature that
    does not verify, a lifetime other than 60, an `aud` other than the issuer,
    an `iss` or a `sub` other than the client id, a `jti` that is no base64url
    of 32 bytes, an `nbf` that is no integer equal to `iat`, an `exp` at or
    before `now`, an `iat` more than `ASSERTION_SKEW` seconds ahead of `now`,
    and a `jti` that `seen` holds. `now` is the clock of the issuer, the epoch
    seconds of the host where it is None, and `seen` a `SeenJti`, which a
    `jti` of the right shape enters once it is judged.
    """
    problems = []
    now = time.time() if now is None else now
    try:
        header, claims, _ = decode_jws(jws)
    except (ValueError, UnicodeDecodeError) as error:
        return {"header": None, "payload": None, "lifetime": None, "kid": None, "verified": False, "jti": None,
                "problems": ["undecodable: %s" % error]}
    kid = header.get("kid") if isinstance(header, dict) else None
    key = next((key for key in client_keys if key.key_id == kid), None)
    verified = key is not None and verify_jws(jws, key.public, work)
    if key is None:
        problems.append("no client key has the kid %r" % kid)
    elif not verified:
        problems.append("the signature does not verify under the key %r" % kid)
    if header.get("alg") != "RS256":
        problems.append("alg %r" % header.get("alg"))
    lifetime = None
    if isinstance(claims.get("exp"), int) and isinstance(claims.get("iat"), int):
        lifetime = claims["exp"] - claims["iat"]
    if lifetime != ASSERTION_LIFETIME:
        problems.append("lifetime %r" % lifetime)
    if claims.get("aud") != audience:
        problems.append("aud %r" % claims.get("aud"))
    if key is not None and (claims.get("iss") != key.client_id or claims.get("sub") != key.client_id):
        problems.append("iss %r and sub %r" % (claims.get("iss"), claims.get("sub")))
    jti = claims.get("jti")
    drawn = isinstance(jti, str) and re.fullmatch(r"[A-Za-z0-9_-]{43}", jti) and len(b64url_decode(jti)) == 32
    if not drawn:
        problems.append("jti %r" % jti)
    exp, iat, nbf = claims.get("exp"), claims.get("iat"), claims.get("nbf")
    if not (isinstance(nbf, int) and not isinstance(nbf, bool) and nbf == iat):
        problems.append("nbf %r is not the integer iat %r" % (nbf, iat))
    if isinstance(exp, (int, float)) and not isinstance(exp, bool) and exp <= now:
        problems.append("exp %r is past, the issuer's clock reads %d" % (exp, now))
    if isinstance(iat, (int, float)) and not isinstance(iat, bool) and iat > now + ASSERTION_SKEW:
        problems.append("iat %r lies ahead of the issuer's clock, which reads %d" % (iat, now))
    if drawn and seen is not None and not seen.first(jti):
        problems.append("jti %r was seen in an earlier assertion" % jti)
    payload = {name: ("<%s>" % name if name in ("jti", "iat", "nbf", "exp") else value) for name, value in claims.items()}
    return {"header": header, "payload": payload, "lifetime": lifetime, "kid": kid, "verified": verified,
            "jti": jti if isinstance(jti, str) else None, "problems": problems}


@dataclass
class Request:
    """One request a side sent the issuer, and how the issuer answered it."""

    index: int
    at: float
    method: str = ""
    target: str = ""
    version: str = ""
    header_lines: list = field(default_factory=list)
    body: bytes = b""
    endpoint: str | None = None
    scenario: str | None = None
    status: int | None = None
    form: list | None = None
    assertion: dict | None = None
    pkce: str | None = None
    tls_version: str | None = None
    alpn: str | None = None
    hostile: str | None = None
    complete: bool = False
    error: str | None = None
    # The connection the request came on, by the order of the accepts, whether an earlier request of its
    # connection was answered, whether its handshake resumed a session, and the span of the request in
    # nanoseconds of `time.perf_counter_ns`, from the accept of its connection or the first byte of a later
    # request on a kept connection to the end of its answer, with the processor time of the watched side at
    # both ends where a notebook set `MockIssuer.watch`. `record` leaves them out, since no golden holds them.
    connection: int = -1
    reused: bool = False
    resumed: bool | None = None
    began_ns: int | None = None
    ended_ns: int | None = None
    side_began_ns: int | None = None
    side_ended_ns: int | None = None

    def header(self, name: str) -> str | None:
        """The value of the first header line named `name`, the name case folded."""
        for key, value in self.header_lines:
            if key.lower() == name.lower():
                return value
        return None

    def record(self) -> dict:
        """The request as plain data, the body as text where it is UTF-8."""
        try:
            text = self.body.decode("utf-8")
        except UnicodeDecodeError:
            text = None
        return {
            "index": self.index,
            "method": self.method,
            "target": self.target,
            "version": self.version,
            "header_lines": [[name, value] for name, value in self.header_lines],
            "body_text": text,
            "body_bytes": len(self.body),
            "endpoint": self.endpoint,
            "scenario": self.scenario,
            "status": self.status,
            "form": self.form,
            "assertion": self.assertion,
            "pkce": self.pkce,
            "tls_version": self.tls_version,
            "alpn": self.alpn,
            "hostile": self.hostile,
            "complete": self.complete,
            "error": self.error,
        }


class _Cut(Exception):
    """A connection ended or broke before its exchange was whole."""


class _TlsChannel:
    """One TLS session over a socket through `ssl.SSLObject`, so each record the issuer sends is visible."""

    def __init__(self, connection: socket.socket, context: ssl.SSLContext, began: tuple, moment) -> None:
        self.connection = connection
        self.incoming = ssl.MemoryBIO()
        self.outgoing = ssl.MemoryBIO()
        self.tls = context.wrap_bio(self.incoming, self.outgoing, server_side=True)
        # The start of the span of the request being read, the moment and the processor time of the watched
        # side at the accept for the first request of the connection, and none after an answer on a kept
        # connection until the next byte arrives, which `moment` reads.
        self.began: tuple | None = began
        self.moment = moment
        # Whether a byte of the request line of the request being read arrived.
        self.started = False
        self.entry: Request | None = None

    def flush(self, cut: str | None = None) -> None:
        """Sends what the session wrote, or under `cut` its first half and then closes."""
        data = self.outgoing.read()
        if cut is not None:
            half = max(1, len(data) // 2)
            self.connection.sendall(data[:half])
            raise _Cut("closed in the middle of %s of %d bytes, %d sent" % (cut, len(data), half))
        if data:
            self.connection.sendall(data)

    def feed(self) -> None:
        data = self.connection.recv(65536)
        if not data:
            raise _Cut("the peer closed")
        if self.began is None:
            self.began = self.moment()
        self.incoming.write(data)

    def handshake(self, hostile: str | None) -> None:
        while True:
            try:
                self.tls.do_handshake()
                break
            except ssl.SSLWantReadError:
                if hostile == "handshake" and self.outgoing.pending:
                    self.flush(cut="the first flight of the handshake")
                self.flush()
                self.feed()
        self.flush()

    def read(self, size: int) -> bytes:
        while True:
            try:
                return self.tls.read(size)
            except ssl.SSLWantReadError:
                self.flush()
                self.feed()
            except ssl.SSLZeroReturnError:
                return b""

    def write(self, data: bytes, cut: bool) -> None:
        """Writes `data` as one write of the session, and cuts its record in the middle under `cut`."""
        self.tls.write(data)
        if cut:
            self.flush(cut="the record of the answer")
        self.flush()

    def send(self, data: bytes, cut: bool) -> None:
        """Writes `data` as `write` does and ends the session with its close alert."""
        self.write(data, cut)
        try:
            self.tls.unwrap()
        except (ssl.SSLWantReadError, ssl.SSLError):
            pass
        self.flush()


class _Reader(io.RawIOBase):
    """The decrypted request stream a handler of `http.server` reads."""

    def __init__(self, channel: _TlsChannel) -> None:
        self.channel = channel

    def readable(self) -> bool:
        return True

    def readinto(self, buffer) -> int:
        data = self.channel.read(len(buffer))
        buffer[: len(data)] = data
        return len(data)


class _Handler(http.server.BaseHTTPRequestHandler):
    """The handler of one request, whose answer the issuer sends as one write of the TLS session."""

    protocol_version = "HTTP/1.1"
    server_version = "aosabi-mock-issuer"
    sys_version = ""

    def setup(self) -> None:
        channel = self.request
        self.connection = channel.connection
        self.rfile = io.BufferedReader(_Reader(channel))
        self.wfile = io.BytesIO()

    def finish(self) -> None:
        pass

    def log_message(self, format, *args) -> None:
        pass

    def handle_one_request(self) -> None:
        issuer = self.server.issuer
        self.raw_requestline = self.rfile.readline(65537)
        if not self.raw_requestline:
            self.close_connection = True
            return
        self.request.started = True
        if not self.parse_request():
            return
        length = int(self.headers.get("content-length") or 0)
        body = self.rfile.read(length) if length else b""
        issuer._handle(self, body)
        if not issuer.keeps:
            self.close_connection = True
        elif not self.close_connection:
            issuer._send_kept(self)

    def answer(self, status: int, headers, body: bytes) -> None:
        """Writes the status line, `headers` in order, the length, the connection field and the body.

        The closing issuer writes `connection: close` on every answer, and the
        issuer that keeps connections only where the request asks to close or
        is no request of HTTP/1.1, as section 9.3 of RFC 9112 keeps a
        connection.
        """
        self.send_response_only(status)
        for name, value in headers:
            self.send_header(name, value)
        self.send_header("content-length", str(len(body)))
        if not self.server.issuer.keeps or self.close_connection:
            self.send_header("connection", "close")
        self.end_headers()
        self.wfile.write(body)

    def send_response_only(self, code, message=None) -> None:
        # No `Server` and no `Date` line, so an answer is the same bytes on every run.
        if message is None:
            message = self.responses.get(code, ("Answer",))[0]
        if not hasattr(self, "_headers_buffer"):
            self._headers_buffer = []
        self._headers_buffer.append(("%s %d %s\r\n" % (self.protocol_version, code, message)).encode("latin-1"))


class _Server(http.server.HTTPServer):
    """The listener: a plain socket whose connections the issuer wraps in TLS itself."""

    allow_reuse_address = True
    daemon_threads = True

    def __init__(self, issuer: "MockIssuer", port: int) -> None:
        self.issuer = issuer
        super().__init__((HOST, port), _Handler)

    def get_request(self) -> tuple:
        """Accepts a connection and answers it with its address and the moment of the accept."""
        connection, address = self.socket.accept()
        return connection, (address, self.issuer.moment())

    def process_request(self, request, client_address) -> None:
        address, accepted = client_address
        thread = threading.Thread(target=self.issuer._connection, args=(request, address, accepted),
                                  daemon=True, name="mock-issuer %d" % self.server_address[1])
        thread.start()


class MockIssuer:
    """The mocked issuer on one HTTPS port, answering from its scenario and recording every request.

    `tls_files` are the files of tests/harness/tls.py and `client_keys` the
    client keys whose assertions it accepts, the key of the run first, and a
    second key for the variant `two_keys` where a notebook gives one.
    `port_offset` is the offset of the runner, which `port` already carries,
    and `blocked` binds a port of `BLOCKED_PORTS` as it is. `connection`,
    `tickets` and `host_name` are the modes of `CONNECTION_MODES`,
    `TICKET_COUNTS` and `HOST_NAMES`, the closing issuer on the IP literal
    with no ticket by default, and a host name takes the files of
    `tls.generate` with `with_host_name`. `watch` is None until a notebook
    sets a reader of the processor time of the side, which `moment` calls.
    """

    def __init__(self, port: int, tls_files, client_keys=(), scenario: str = "ok", port_offset: int = 0,
                 hostile: str | None = None, root=None, blocked: bool = False, connection: str = "close",
                 tickets: int = 0, host_name: str | None = None) -> None:
        if blocked and port not in BLOCKED_PORTS:
            raise ValueError("the issuer of a blocked port binds one of %s and not %d" % (BLOCKED_PORTS, port))
        if not blocked and not allowed_port(port, port_offset):
            raise ValueError("the issuer binds the ports %s plus the offset %d and not %d"
                             % (", ".join("%d to %d" % pair for pair in PORT_RANGES), port_offset, port))
        if scenario not in SCENARIOS:
            raise ValueError("no scenario named %r" % scenario)
        if hostile not in HOSTILE_MODES:
            raise ValueError("no hostile mode named %r" % hostile)
        if tls_files is None:
            raise ValueError("the issuer serves HTTPS and needs the files of tests/harness/tls.py")
        if connection not in CONNECTION_MODES:
            raise ValueError("no connection mode named %r" % connection)
        if tickets not in TICKET_COUNTS:
            raise ValueError("the issuer sends one of %s tickets and not %r" % (TICKET_COUNTS, tickets))
        if host_name is not None and host_name not in HOST_NAMES:
            raise ValueError("the issuer takes the host names %s and not %r" % (HOST_NAMES, host_name))
        if host_name is not None and getattr(tls_files, "host_cert", None) is None:
            raise ValueError("the host name %s needs the certificate of tls.generate with with_host_name" % host_name)
        self.port = port
        self.connection_mode = connection
        self.keeps = connection == "keep-alive"
        self.tickets = tickets
        self.host_name = host_name
        self.issuer = "https://%s:%d" % (host_name or HOST, port)
        self.client_keys = tuple(client_keys) or ((tls_files.client,) if getattr(tls_files, "client", None) else ())
        self.work = paths(root).target / "harness" / "issuer" / str(port)
        self._lock = threading.Lock()
        self._scenario = scenario
        self._challenge: str | None = None
        self._hostile = hostile
        self._received: list[Request] = []
        self._controls: list[dict] = []
        self._signing_key: Path | None = None
        self._seen_jti = SeenJti()
        self._connections: set[socket.socket] = set()
        self._accepted = 0
        self._held: dict[str, threading.Event] = {}
        self._waiting: dict[str, int] = {}
        self.watch = None
        context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        context.minimum_version = ssl.TLSVersion.TLSv1_2
        if host_name is None:
            context.load_cert_chain(str(tls_files.cert), str(tls_files.key))
        else:
            context.load_cert_chain(str(tls_files.host_cert), str(tls_files.host_key))
        context.set_alpn_protocols(["http/1.1"])
        # With no ticket after the handshake the first record after it carries the answer. The mode of
        # tickets sends its tickets right after the handshake, so the answer's record still comes first
        # in the write of the answer.
        context.num_tickets = tickets
        self._context = context
        self._server = _Server(self, port)
        self._thread = threading.Thread(target=self._server.serve_forever, kwargs={"poll_interval": 0.1}, daemon=True,
                                        name="mock-issuer accept %d" % port)
        self._thread.start()

    # ── control ──────────────────────────────────────────────────────────────

    def control(self, scenario: str | None = None, challenge: str | None = None, hostile="unchanged",
                clear_challenge: bool = False) -> dict:
        """Sets the scenario, the PKCE challenge the token endpoint expects and the hostile mode, and answers the state.

        A key left at its default stays as it was. `hostile=None` turns the
        hostile mode off.
        """
        if scenario is not None and scenario not in SCENARIOS:
            raise ValueError("no scenario named %r" % scenario)
        if hostile != "unchanged" and hostile not in HOSTILE_MODES:
            raise ValueError("no hostile mode named %r" % hostile)
        with self._lock:
            if scenario is not None:
                self._scenario = scenario
            if clear_challenge:
                self._challenge = None
            if challenge is not None:
                self._challenge = challenge
            if hostile != "unchanged":
                self._hostile = hostile
            return self.state()

    def expect_challenge(self, location: str) -> str:
        """Hands the token endpoint the `code_challenge` of a `Location`, and answers it."""
        challenge = challenge_of(location)
        if challenge is None:
            raise ValueError("the Location names no code_challenge: %s" % location)
        self.control(challenge=challenge)
        return challenge

    def state(self) -> dict:
        """The scenario, the challenge and the hostile mode, as the control endpoint answers them."""
        return {"scenario": self._scenario, "challenge": self._challenge, "hostile": self._hostile}

    @property
    def scenario(self) -> str:
        return self._scenario

    def controls(self) -> list:
        """Every control request the endpoint answered, in order."""
        with self._lock:
            return list(self._controls)

    # ── the records ──────────────────────────────────────────────────────────

    def received(self) -> list:
        """Every request of a side so far, in the order they arrived."""
        with self._lock:
            return list(self._received)

    def mark(self) -> int:
        """The count of requests so far, which `since` takes to name the requests of one case."""
        with self._lock:
            return len(self._received)

    def since(self, mark: int) -> list:
        """The requests that arrived after `mark`."""
        with self._lock:
            return list(self._received[mark:])

    def settle(self, mark: int, quiet_s: float = 0.3, timeout_s: float = 10.0) -> list:
        """The requests after `mark` once every one is whole or broken and `quiet_s` passed with no new one."""
        deadline = time.monotonic() + timeout_s
        count, changed = -1, time.monotonic()
        while time.monotonic() < deadline:
            current = self.since(mark)
            if len(current) != count:
                count, changed = len(current), time.monotonic()
            done = all(entry.complete or entry.error is not None for entry in current)
            if done and time.monotonic() - changed >= quiet_s:
                return current
            time.sleep(0.02)
        return self.since(mark)

    def hold(self, endpoint: str) -> None:
        """Holds every answer of `endpoint` until `release`, at most `HOLD_S` seconds."""
        if endpoint not in PATHS:
            raise ValueError("no endpoint named %r" % endpoint)
        with self._lock:
            self._held[endpoint] = threading.Event()
            self._waiting[endpoint] = 0

    def waiting(self, endpoint: str) -> int:
        """The requests of `endpoint` that reached the hold since `hold`."""
        with self._lock:
            return self._waiting.get(endpoint, 0)

    def release(self, endpoint: str) -> None:
        """Answers the held requests of `endpoint` and every later one at once."""
        with self._lock:
            gate = self._held.pop(endpoint, None)
        if gate is not None:
            gate.set()

    def spans(self, entries, timeout_s: float = 10.0) -> list:
        """The span of each of `entries` in nanoseconds, once every one has ended or `timeout_s` passed.

        A span runs from the accept of the connection of the request, or the
        first byte of a later request on a kept connection, to the end of its
        answer, the close of the connection where the answer closes it. An
        entry that has not ended within `timeout_s` takes no span.
        """
        self._ended(entries, timeout_s)
        return [entry.ended_ns - entry.began_ns for entry in entries
                if entry.ended_ns is not None and entry.began_ns is not None]

    def side_times(self, entries, timeout_s: float = 10.0) -> list:
        """The processor time the watched side spent within the span of each of `entries`, in nanoseconds.

        `watch` reads it at both ends of a span, so it is the work of the
        side, such as its part of a TLS handshake, while the issuer holds the
        connection open. An entry with no reading at either end takes none.
        """
        self._ended(entries, timeout_s)
        return [entry.side_ended_ns - entry.side_began_ns for entry in entries
                if entry.side_ended_ns is not None and entry.side_began_ns is not None]

    def moment(self) -> tuple:
        """The moment of `time.perf_counter_ns` and the processor time of the watched side, None with no watch.

        `watch` is None or a callable a notebook sets that answers the
        processor time of the side the issuer serves in nanoseconds, which
        the issuer reads at both ends of each span, so a notebook can book the
        work of the side within a span to the side and not to the issuer.
        """
        watch = self.watch
        if watch is None:
            return time.perf_counter_ns(), None
        try:
            side = watch()
        except (OSError, ValueError):
            side = None
        return time.perf_counter_ns(), side

    def _ended(self, entries, timeout_s: float) -> None:
        deadline = time.monotonic() + timeout_s
        while any(entry.ended_ns is None for entry in entries) and time.monotonic() < deadline:
            time.sleep(0.0002)

    # ── the connection ───────────────────────────────────────────────────────

    def _connection(self, connection: socket.socket, client_address, accepted: tuple | None = None) -> None:
        connection.settimeout(IDLE_S)
        began = self.moment() if accepted is None else accepted
        with self._lock:
            self._connections.add(connection)
            hostile = self._hostile
            entry = Request(index=-1, at=time.monotonic(), hostile=hostile, connection=self._accepted,
                            began_ns=began[0], side_began_ns=began[1])
            self._accepted += 1
        channel = _TlsChannel(connection, self._context, began, self.moment)
        channel.entry = entry
        try:
            channel.handshake(hostile)
            entry.tls_version = channel.tls.version()
            entry.alpn = channel.tls.selected_alpn_protocol()
            entry.resumed = channel.tls.session_reused
            handler = _Handler(channel, client_address, self._server)
            # A kept connection hands each answered request a fresh entry, so the entry of the
            # connection is the last one, and its client may close it before a new request.
            entry = channel.entry
            if entry.index < 0 and entry.endpoint != "control" and (channel.started or not entry.reused):
                entry.error = "no request was read whole"
                self._append(entry)
            data = handler.wfile.getvalue()
            if data:
                channel.send(data, cut=entry.hostile == "answer" and entry.endpoint != "control")
            entry.complete = entry.complete or entry.endpoint == "control"
        except _Cut as cut:
            entry = channel.entry
            entry.error = str(cut)
            if entry.index < 0 and entry.endpoint != "control" and (channel.started or not entry.reused):
                self._append(entry)
        except (OSError, ssl.SSLError, ValueError) as error:
            entry = channel.entry
            entry.error = "%s: %s" % (type(error).__name__, error)
            if entry.index < 0 and entry.endpoint != "control" and (channel.started or not entry.reused):
                self._append(entry)
        finally:
            with self._lock:
                self._connections.discard(connection)
            try:
                connection.close()
            except OSError:
                pass
            if entry.ended_ns is None:
                entry.side_ended_ns = self.moment()[1]
                entry.ended_ns = time.perf_counter_ns()

    def _send_kept(self, handler: _Handler) -> None:
        """Sends the answer of a request on a kept connection and readies the entry of its next request."""
        channel = handler.request
        entry = channel.entry
        data = handler.wfile.getvalue()
        handler.wfile = io.BytesIO()
        channel.write(data, cut=entry.hostile == "answer" and entry.endpoint != "control")
        entry.side_ended_ns = self.moment()[1]
        entry.ended_ns = time.perf_counter_ns()
        entry.complete = entry.complete or entry.endpoint == "control"
        with self._lock:
            hostile = self._hostile
        channel.entry = Request(index=-1, at=time.monotonic(), hostile=hostile, tls_version=entry.tls_version,
                                alpn=entry.alpn, connection=entry.connection, reused=True, resumed=entry.resumed)
        channel.began = None
        channel.started = False

    def _append(self, entry: Request) -> None:
        with self._lock:
            entry.index = len(self._received)
            self._received.append(entry)

    def _handle(self, handler: _Handler, body: bytes) -> None:
        entry: Request = handler.request.entry
        if entry.began_ns is None:
            # A request whose bytes came with those of the request before it begins its span here.
            entry.began_ns, entry.side_began_ns = handler.request.began or self.moment()
        entry.method = handler.command
        entry.target = handler.path
        entry.version = handler.request_version
        entry.header_lines = [(name, value) for name, value in handler.headers.items()]
        entry.body = body
        path = urlsplit(handler.path).path
        if path == CONTROL_PATH:
            entry.endpoint = "control"
            status, headers, answer = self._control_answer(handler.command, body)
            with self._lock:
                self._controls.append({"method": handler.command, "body": body.decode("utf-8", "replace"),
                                       "status": status})
            handler.answer(status, headers, answer)
            return
        with self._lock:
            scenario = self._scenario
            entry.scenario = scenario
        entry.endpoint = next((name for name, value in PATHS.items() if value == path), None)
        with self._lock:
            gate = self._held.get(entry.endpoint)
            if gate is not None:
                self._waiting[entry.endpoint] += 1
        if gate is not None:
            gate.wait(HOLD_S)
        try:
            status, headers, answer = self._answer(entry, scenario)
        except Exception as error:  # A defect of the issuer answers 599 and stays visible in the record.
            entry.error = "the issuer failed: %s: %s" % (type(error).__name__, error)
            status, headers, answer = 599, [("content-type", "text/plain")], entry.error.encode("utf-8")
        entry.status = status
        entry.complete = True
        self._append(entry)
        handler.answer(status, headers, answer)

    def _control_answer(self, method: str, body: bytes) -> tuple:
        if method == "GET":
            return _json(200, self.state())
        if method != "POST":
            return _json(405, {"error": "the control endpoint takes GET and POST"})
        try:
            order = json.loads(body or b"{}")
            if not isinstance(order, dict):
                raise ValueError("the order is no object")
            state = self.control(scenario=order.get("scenario"), challenge=order.get("challenge"),
                                 hostile=order.get("hostile", "unchanged"),
                                 clear_challenge=bool(order.get("clear_challenge")))
        except ValueError as error:
            return _json(400, {"error": str(error)})
        return _json(200, state)

    # ── the answers ──────────────────────────────────────────────────────────

    def discovery(self, scenario: str) -> dict:
        """The discovery document of `scenario`, in the key order of the discovery of Zitadel.

        A scenario of the plain discovery answers the document the capture
        issuer serves to the cases of C82 without the suffix `-alg`, the
        issuer, the four endpoints and the key set alone, so it names no
        signing algorithm and none of the other fields of the document of
        every other scenario.
        """
        base = self.issuer
        provider = scenario in PROVIDER_SCENARIOS or scenario == "discovery_500"
        algorithms = not scenario.endswith(PLAIN)
        document = {
            "issuer": OTHER_ISSUER if scenario == "wrong_issuer" else base,
            "authorization_endpoint": base + PATHS["authorize"],
            "token_endpoint": base + PATHS["token"],
        }
        if provider:
            document["introspection_endpoint"] = base + PATHS["introspect"]
            document["userinfo_endpoint"] = base + PATHS["userinfo"]
        document["jwks_uri"] = base + PATHS["keys"]
        if not algorithms:
            return document
        document["scopes_supported"] = ["openid", "profile", "email"]
        document["response_types_supported"] = ["code"]
        document["grant_types_supported"] = ["authorization_code"]
        document["subject_types_supported"] = ["public"]
        document["id_token_signing_alg_values_supported"] = ["RS256"]
        document["token_endpoint_auth_methods_supported"] = ["private_key_jwt"]
        document["code_challenge_methods_supported"] = ["S256"]
        if provider:
            document["introspection_endpoint_auth_methods_supported"] = ["private_key_jwt"]
            document["userinfo_signing_alg_values_supported"] = ["RS256"]
            document["introspection_signing_alg_values_supported"] = ["RS256"]
        return document

    def _answer(self, entry: Request, scenario: str) -> tuple:
        endpoint = entry.endpoint
        if endpoint == "discovery" and entry.method == "GET":
            if scenario == "discovery_500":
                return _json(500, {"error": "server_error", "error_description": "the issuer is down"})
            if scenario == "discovery_malformed":
                return 200, [("content-type", "application/json")], _cut(self.discovery(scenario))
            return _json(200, self.discovery(scenario))
        if endpoint == "keys" and entry.method == "GET":
            return _json(200, {"keys": [self._jwk()]})
        if endpoint == "token" and entry.method == "POST":
            return self._token(entry, scenario)
        if endpoint == "introspect" and entry.method == "POST":
            return self._introspect(entry, scenario)
        if endpoint == "userinfo" and entry.method == "GET":
            return self._userinfo(entry, scenario)
        return _json(404, {"error": "not_found"})

    def _client_authentication(self, entry: Request) -> tuple | None:
        """Parses the form and checks the client assertion, and answers the 401 a failure gets."""
        form = parse_qsl(entry.body.decode("utf-8", "replace"), keep_blank_values=True)
        entry.form = [[name, value] for name, value in form]
        values = dict(form)
        if values.get("client_assertion_type") != ASSERTION_TYPE or "client_assertion" not in values:
            entry.assertion = {"problems": ["no client assertion"], "verified": False}
            return _json(401, {"error": "invalid_client", "error_description": "no client assertion"})
        entry.assertion = assertion_report(values["client_assertion"], self.client_keys, self.issuer, self.work,
                                           seen=self._seen_jti)
        if entry.assertion["problems"]:
            return _json(401, {"error": "invalid_client", "error_description": "; ".join(entry.assertion["problems"])})
        return None

    def _token(self, entry: Request, scenario: str) -> tuple:
        refused = self._client_authentication(entry)
        if refused is not None:
            return refused
        values = dict(entry.form)
        with self._lock:
            challenge = self._challenge
        verifier = values.get("code_verifier")
        if challenge is None:
            entry.pkce = "unset"
        elif verifier is not None and s256(verifier) == challenge:
            entry.pkce = "match"
        else:
            entry.pkce = "mismatch"
            return _json(400, {"error": "invalid_grant", "error_description": "the code_verifier does not match"})
        if values.get("grant_type") != "authorization_code" or not values.get("code"):
            return _json(400, {"error": "invalid_request", "error_description": "no authorization code grant"})
        if scenario == "invalid_grant":
            return _json(400, {"error": "invalid_grant", "error_description": "the code is not valid"})
        document = {"access_token": ACCESS_TOKEN, "token_type": "Bearer"}
        if scenario != "no_expiry":
            document["expires_in"] = EXPIRES_IN
        client_id = values.get("client_id") or self.client_keys[0].client_id
        if scenario.startswith("id_token"):
            document["id_token"] = self.id_token(scenario, client_id)
        if scenario == "surrogate_access_token":
            document["access_token"] = ACCESS_TOKEN + LONE_SURROGATE
        elif scenario == "surrogate_token_type":
            document["token_type"] = "Bearer" + LONE_SURROGATE
        elif scenario == "token_type_kelvin":
            document["token_type"] = "Bearer" + KELVIN_SIGN
        elif scenario == "surrogate_id_token":
            signed = self.sign({"alg": "RS256", "kid": ISSUER_KEY_ID, "typ": "JWT"},
                               self.id_token_claims("id_token", client_id))
            header, payload, signature = signed.split(".")
            document["id_token"] = "%s.%s%s.%s" % (header, payload, LONE_SURROGATE, signature)
        if scenario == "token_malformed":
            return 200, [("content-type", "application/json")], _cut(document)
        return _json(200, document)

    def id_token(self, scenario: str, client_id: str) -> str:
        """The ID token of `scenario` for `client_id`, in the form and with the claims the scenario names.

        `id_token` writes the form of the capture, whose signature no key
        verifies, `id_token_malformed` a compact JWT whose payload decodes to
        no JSON, the scenarios of the prefix `id_token_b64_` a payload in a
        form of `b64_segment`, and every other scenario the claims of
        `id_token_claims` signed with the key of the issuer.
        """
        header = {"alg": "RS256", "kid": ISSUER_KEY_ID, "typ": "JWT"}
        if scenario == "id_token_malformed":
            return "%s.%s.%s" % (b64url(json.dumps(header, separators=(",", ":")).encode("utf-8")), b64url(b"not json"),
                                 b64url(b"malformed"))
        claims = self.id_token_claims(scenario, client_id)
        if scenario in CAPTURE_FORMS:
            return capture_jwt(claims, CAPTURE_FORMS[scenario])
        if scenario.startswith("id_token_b64_"):
            signing_input = "%s.%s" % (b64url(json.dumps(header, separators=(",", ":")).encode("utf-8")),
                                       b64_segment(scenario, claims))
            return signing_input + "." + b64url(self.sign_bytes(signing_input.encode("utf-8")))
        return self.sign(header, claims)

    def id_token_claims(self, scenario: str, client_id: str) -> dict:
        """The claims of the ID token of `scenario` for `client_id`.

        A scenario of the capture changes the claims of the capture issuer as
        it says, `id_token_zitadel` and the scenarios of `ZITADEL_ID_TOKENS`
        take the claims of `zitadel_id_token` with the change each names, and
        the scenarios of the prefix `id_token_b64_` the claims of the capture
        issuer with a claim `note` whose text holds `?` and `>`, which base64
        writes as `/` and `+`, at a length that leaves the bytes the form of
        the scenario asks for.
        """
        now = int(time.time())
        if scenario in ZITADEL_ID_TOKENS:
            return self.zitadel_id_token(scenario, client_id, now)
        claims = {"iss": self.issuer, "sub": SUBJECT, "aud": client_id, "exp": now + 300, "iat": now, "auth_time": now}
        if scenario == "id_token_bad_aud":
            claims["aud"] = "other-client"
        elif scenario == "id_token_bad_iss":
            claims["iss"] = OTHER_ISSUER
        elif scenario == "id_token_expired":
            claims["exp"] = now - 3600
        elif scenario == "id_token_no_iat":
            del claims["iat"]
        elif scenario == "id_token_no_sub":
            del claims["sub"]
        elif scenario.startswith("id_token_b64_"):
            # The lenient form needs padding, a length of no multiple of
            # three, and the two refused forms a length of a multiple of
            # three, whose base64url leaves no character over a group.
            for extra in ("", "?", "??"):
                claims["note"] = "???>>>~~~" + extra
                length = len(json.dumps(claims, separators=(",", ":")).encode("utf-8")) % 3
                if (length != 0) == (scenario == "id_token_b64_lenient"):
                    break
        return claims

    def zitadel_id_token(self, scenario: str, client_id: str, now: int) -> dict:
        """The claims of the ID token of a code exchange as the code of Zitadel writes them, changed as `scenario` says.

        The claims stand in the order of the struct of zitadel/oidc, which
        writes no claim beyond it with the assertions of the user info and of
        the roles off in the ID token, the default of a client: `aud` the
        client and its project, `amr`, `azp` and `client_id` the client, no
        `acr`, the `at_hash` of the access token and the session `sid` (the
        module documentation names the code). `id_token_zitadel_at_hash`
        takes the `at_hash` of another token, `id_token_aud_no_client` an
        `aud` of the project alone, `id_token_aud_no_azp` no `azp` and
        `id_token_azp_other` an `azp` of another client.
        """
        hashed = ACCESS_TOKEN if scenario != "id_token_zitadel_at_hash" else "t-other"
        claims = {"iss": self.issuer, "sub": SUBJECT, "aud": [client_id, PROJECT_ID], "exp": now + ZITADEL_TOKEN_S,
                  "iat": now, "auth_time": now, "amr": ["pwd"], "azp": client_id, "client_id": client_id,
                  "at_hash": at_hash(hashed), "sid": ZITADEL_SESSION_ID}
        if scenario == "id_token_aud_no_client":
            claims["aud"] = [PROJECT_ID]
        elif scenario == "id_token_aud_no_azp":
            del claims["azp"]
        elif scenario == "id_token_azp_other":
            claims["azp"] = "other-client"
        return claims

    def _token_claims(self, token: str, scenario: str) -> dict:
        """The introspection document of an active `token` under `scenario`.

        `introspect_zitadel` writes the introspection answer the code of
        Zitadel writes for a token of the scope `openid email profile` of a
        person, `aud` the client and its project, the claims of the profile and
        the email, and the roles under both names of the project, its keys
        sorted as the map of Go writes them once a claim beyond the struct is
        present (the module documentation names the code), and
        `roles_project` and `roles_per_project` the roles under one of those
        names alone. None of the three carries the name the provider reads.
        """
        known = TOKENS.get(token, {})
        sub = known.get("sub", SUBJECT)
        roles = SCENARIO_ROLES.get(scenario, known.get("roles", DEFAULT_ROLES))
        client_id = self.client_keys[0].client_id if self.client_keys else "test-client"
        if scenario == "introspect_zitadel":
            now = int(time.time())
            claims = {"active": True, "scope": "openid email profile", "client_id": client_id, "token_type": "Bearer",
                      "exp": now + ZITADEL_TOKEN_S, "iat": now, "auth_time": now, "nbf": now, "sub": sub,
                      "aud": [client_id, PROJECT_ID], "amr": ["pwd"], "iss": self.issuer, "jti": ZITADEL_TOKEN_ID,
                      "username": EMAIL, **ZITADEL_PROFILE, "updated_at": now - 86400, "preferred_username": EMAIL,
                      "email": EMAIL, "email_verified": True, ZITADEL_ROLES_CLAIM: ZITADEL_ROLES,
                      ZITADEL_PROJECT_ROLES_CLAIM: ZITADEL_ROLES}
            return dict(sorted(claims.items()))
        document = {"active": True, "scope": "openid email profile", "client_id": client_id, "token_type": "Bearer",
                    "sub": sub}
        if scenario == "roles_project":
            document[ZITADEL_ROLES_CLAIM] = ZITADEL_ROLES
        elif scenario == "roles_per_project":
            document[ZITADEL_PROJECT_ROLES_CLAIM] = ZITADEL_ROLES
        else:
            document[ROLES_CLAIM] = roles
        return document

    def _introspect(self, entry: Request, scenario: str) -> tuple:
        refused = self._client_authentication(entry)
        if refused is not None:
            return refused
        token = dict(entry.form).get("token", "")
        scenario = base_scenario(scenario)
        if scenario == "introspect_401":
            return _json(401, {"error": "invalid_client", "error_description": "the client is not known"})
        if scenario == "introspect_400_json":
            return _json(400, {"error": "invalid_request", "error_description": "the request is not valid"})
        if scenario == "introspect_inactive" or not token:
            return _json(200, {"active": False})
        if scenario == "login_and_introspect" and token != ACCESS_TOKEN:
            return _json(200, {"active": False})
        document = self._token_claims(token, scenario)
        if scenario == "introspect_no_sub":
            del document["sub"]
        if scenario == "introspect_malformed":
            return 200, [("content-type", "application/json")], _cut(document)
        if scenario.startswith("introspect_jwt"):
            now = int(time.time())
            header = {"alg": "RS256", "kid": ISSUER_KEY_ID, "typ": "token-introspection+jwt"}
            claims = {"iss": self.issuer, "aud": document["client_id"], "iat": now, "token_introspection": document}
            if scenario == "introspect_jwt_bad_aud":
                claims["aud"] = "other-client"
            elif scenario == "introspect_jwt_bad_iss":
                claims["iss"] = OTHER_ISSUER
            elif scenario == "introspect_jwt_no_iat":
                del claims["iat"]
            elif scenario == "introspect_jwt_no_typ":
                del header["typ"]
            elif scenario == "introspect_jwt_typ_kelvin":
                header["typ"] = "Application/To" + KELVIN_SIGN + "en-Introspection+JWT"
            if scenario in CAPTURE_FORMS:
                jwt = capture_jwt(claims, CAPTURE_FORMS[scenario])
            else:
                jwt = self.sign(header, claims)
            return 200, [("content-type", "application/token-introspection+jwt")], jwt.encode("ascii")
        status, headers, body = _json(200, document)
        # The encoder of Go closes the body Zitadel writes with a line feed.
        return status, headers, body + (b"\n" if scenario == "introspect_zitadel" else b"")

    def _userinfo(self, entry: Request, scenario: str) -> tuple:
        authorization = entry.header("authorization") or ""
        kind, _, token = authorization.partition(" ")
        scenario = base_scenario(scenario)
        if kind != "Bearer" or not token:
            return 401, [("www-authenticate", 'Bearer error="invalid_request"'), ("content-type", "application/json")], \
                json.dumps({"error": "invalid_request"}).encode("utf-8")
        if scenario == "userinfo_challenge":
            return 401, [("www-authenticate", 'Bearer error="invalid_token"'), ("content-type", "application/json")], \
                json.dumps({"error": "invalid_token"}).encode("utf-8")
        if scenario == "userinfo_not_json":
            return 200, [("content-type", "text/plain")], b"not json"
        sub = TOKENS.get(token, {}).get("sub", SUBJECT)
        if scenario == "userinfo_sub_mismatch":
            sub = "u-other"
        user = {"sub": sub, "email": EMAIL, RESOURCE_OWNER_CLAIM: "orgA", "perm": ["invoice:read"]}
        if scenario.startswith("userinfo_jwt"):
            client_id = self.client_keys[0].client_id if self.client_keys else "test-client"
            claims = {"iss": self.issuer, "aud": client_id, **user}
            if scenario == "userinfo_jwt_bad_aud":
                claims["aud"] = "other-client"
            elif scenario == "userinfo_jwt_bad_iss":
                claims["iss"] = OTHER_ISSUER
            if scenario in CAPTURE_FORMS:
                jwt = capture_jwt(claims, CAPTURE_FORMS[scenario])
            else:
                jwt = self.sign({"alg": "RS256", "kid": ISSUER_KEY_ID, "typ": "JWT"}, claims)
            return 200, [("content-type", "application/jwt")], jwt.encode("ascii")
        return _json(200, user)

    # ── the signing key of the issuer ────────────────────────────────────────

    def signing_key(self) -> Path:
        """The RSA key the issuer signs with, written on first use under target/harness/issuer/<port>/."""
        with self._lock:
            if self._signing_key is None:
                self.work.mkdir(parents=True, exist_ok=True)
                key = self.work / "issuer.key.pem"
                done = _openssl(["genpkey", "-algorithm", "RSA", "-pkeyopt", "rsa_keygen_bits:2048", "-out", key], b"")
                if done.returncode != 0:
                    raise RuntimeError("openssl wrote no signing key: %s" % done.stderr.decode("utf-8", "replace"))
                self._signing_key = key
            return self._signing_key

    def sign(self, header: dict, claims: dict) -> str:
        """A compact JWS of `header` and `claims` signed RS256 with the signing key of the issuer."""
        return sign_rs256(self.signing_key(), header, claims)

    def sign_bytes(self, data: bytes) -> bytes:
        """The RS256 signature of `data` with the signing key of the issuer, through openssl."""
        done = _openssl(["dgst", "-sha256", "-sign", self.signing_key()], data)
        if done.returncode != 0:
            raise RuntimeError("openssl signed nothing: %s" % done.stderr.decode("utf-8", "replace"))
        return done.stdout

    def _jwk(self) -> dict:
        """The public half of the signing key as a JWK, its modulus read with openssl."""
        done = _openssl(["rsa", "-in", self.signing_key(), "-noout", "-modulus"], b"")
        modulus = bytes.fromhex(done.stdout.decode("ascii").strip().split("=", 1)[1])
        return {"use": "sig", "kty": "RSA", "kid": ISSUER_KEY_ID, "alg": "RS256", "n": b64url(modulus), "e": "AQAB"}

    # ── the end ──────────────────────────────────────────────────────────────

    def close(self) -> None:
        """Stops the listener, answers every held request and closes every open connection."""
        self._server.shutdown()
        self._server.server_close()
        with self._lock:
            gates = list(self._held.values())
            self._held.clear()
        for gate in gates:
            gate.set()
        with self._lock:
            open_ = list(self._connections)
        for connection in open_:
            try:
                connection.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
        self._thread.join(timeout=2)

    def __enter__(self) -> "MockIssuer":
        return self

    def __exit__(self, exc_type, exc, tb) -> bool:
        self.close()
        return False


def _json(status: int, document) -> tuple:
    return status, [("content-type", "application/json")], json.dumps(document, separators=(",", ":")).encode("utf-8")


def port_closed(port: int) -> bool:
    """Whether nothing listens on `port` of loopback, the state of case z-21."""
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as probe:
        probe.settimeout(0.5)
        return probe.connect_ex((HOST, port)) != 0


def records(entries) -> list:
    """The plain data of `entries`, in their order."""
    return [entry.record() for entry in entries]


__all__ = [
    "ACCESS_TOKEN",
    "ASSERTION_LIFETIME",
    "ASSERTION_SKEW",
    "B64_SPACES",
    "BLOCKED_PORTS",
    "CAPTURE_FORMS",
    "CAPTURE_KEY_ID",
    "CAPTURE_SIGNATURE",
    "CONNECTION_MODES",
    "CONTROL_PATH",
    "EMAIL",
    "EXPIRES_IN",
    "HOLD_S",
    "HOST",
    "HOSTILE_MODES",
    "HOST_NAMES",
    "JWT_SCENARIOS",
    "LOGIN_SCENARIOS",
    "LONE_SURROGATE",
    "MALFORMED",
    "MockIssuer",
    "PATHS",
    "PLAIN",
    "PORT_RANGES",
    "PROJECT_ID",
    "PROVIDER_SCENARIOS",
    "Request",
    "SCENARIOS",
    "SUBJECT",
    "SeenJti",
    "TICKET_COUNTS",
    "ZITADEL_ID_TOKENS",
    "ZITADEL_PROJECT_ROLES_CLAIM",
    "ZITADEL_ROLES_CLAIM",
    "allowed_port",
    "assertion_report",
    "at_hash",
    "b64_segment",
    "base_scenario",
    "capture_jwt",
    "challenge_of",
    "client_assertion",
    "decode_jws",
    "port_closed",
    "records",
    "s256",
    "sign_rs256",
    "verify_jws",
]
