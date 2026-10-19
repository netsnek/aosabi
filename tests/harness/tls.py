# SPDX-License-Identifier: MIT
"""Aosabi harness: the test CA and the certificate of 127.0.0.1, through the openssl command line.

The HTTPS port of tests/harness/mock_ingest.py serves the certificate this
module writes, and both sides trust it through the CA file alone: the reference
through `NODE_EXTRA_CA_CERTS`, which Node reads at its start and adds to its
bundled roots, and the twin through `AOSABI_EXTRA_CA_CERTS`, the name
`aosabi-fetch` reads. No Python package makes a key or a certificate here,
because tests/pyproject.toml gains none, so every file comes from the `openssl`
of the machine, which risk 23 of the plan asks 00_environment to find present
in round 2.

`generate` writes a fresh set on every call, under target/harness/tls/ unless
the caller names a directory:

- `ca.key` and `ca.pem`, a key on the curve P-256 and a self signed CA
  certificate with `basicConstraints` CA:TRUE and `keyUsage` keyCertSign and
  cRLSign, both critical, which Node accepts as a trust anchor and the verifier
  of `rustls-webpki` reads the same way,
- `127.0.0.1.key` and `127.0.0.1.pem`, a key on P-256 and a certificate the CA
  signs, with the subject alternative name `IP:127.0.0.1`, `basicConstraints`
  CA:FALSE, `keyUsage` digitalSignature and `extendedKeyUsage` serverAuth, so a
  client that connects to the IP literal checks the IP against the certificate
  as both clients do for an IP server name.

The certificates are valid from one hour before the run until two days after
it, so a clock that moves within a run never reaches either end.

With `with_host_name` the call also writes `issuer.localhost.key` and
`issuer.localhost.pem`, a key on P-256 and a certificate the CA signs with the
subject alternative name `DNS:issuer.localhost` and the extensions of the
certificate of 127.0.0.1, the certificate of the host name mode of the mocked
issuer of tests/harness/mock_issuer.py. The name resolves to 127.0.0.1 through
`getaddrinfo` with no entry in /etc/hosts, since the resolver of the machine
answers every name under `localhost.` with the loopback addresses, so no step
needs root.

The reference reads either form through `crypto.createPrivateKey`.
`ClientKey.auth_key` answers the record `AUTH_KEY` carries,
`{"type":"application","keyId":"test-key-1","key":"<PEM>","clientId":"test-client"}`
in that key order, and `write_key_file` writes the same record as the
`key.json` a side reads from its working directory. A client key lives under
target/harness/ alone, which `client_key` asserts, so no key reaches a
committed path.

The key cases of `61_oidc_login` and `63_zitadel_auth` ask `client_key` for
keys of other sizes and of more than two primes, which `bits` and `primes`
name, and the PKCS#1 public half of a key, which `ClientKey.public_pkcs1`
answers. `CLIENT_KEY_BITS` is the size of the key of a run, which every other
case takes.
"""

from __future__ import annotations

import datetime
import json
import secrets
import shutil
from dataclasses import dataclass
from pathlib import Path

from .paths import paths
from .procs import run

#: The subject of the CA, a neutral test string.
CA_SUBJECT = "/O=Aosabi harness/CN=Aosabi harness test CA"
#: The subject of the certificate of the HTTPS port.
SERVER_SUBJECT = "/O=Aosabi harness/CN=127.0.0.1"
#: The days a certificate is valid.
DAYS = 2
#: The bits of the RSA client key, the size of a key file of Zitadel.
CLIENT_KEY_BITS = 2048
#: The key id and the client id of the record `AUTH_KEY` carries, neutral test strings.
KEY_ID = "test-key-1"
CLIENT_ID = "test-client"
#: The two forms of the private client key.
KEY_FORMS = ("pkcs1", "pkcs8")
#: The seconds openssl may take to write one key, which a key of 8192 bits needs beside the other runs of a machine.
KEY_S = 600

#: The extensions of the CA, written into the configuration of `openssl req`.
CA_EXTENSIONS = """[req]
distinguished_name = dn
x509_extensions = ca
prompt = no
[dn]
[ca]
basicConstraints = critical, CA:TRUE
keyUsage = critical, keyCertSign, cRLSign
subjectKeyIdentifier = hash
"""

#: The extensions of the certificate of 127.0.0.1, which `openssl x509 -req` reads.
SERVER_EXTENSIONS = """[server]
subjectAltName = IP:127.0.0.1
basicConstraints = critical, CA:FALSE
keyUsage = critical, digitalSignature
extendedKeyUsage = serverAuth
subjectKeyIdentifier = hash
authorityKeyIdentifier = keyid
"""

#: The host name of the host name mode of the mocked issuer, a name under
#: `localhost.` that the resolver of the machine answers with the loopback
#: addresses.
ISSUER_HOST = "issuer.localhost"
#: The subject of the certificate of that host name.
HOST_SUBJECT = "/O=Aosabi harness/CN=" + ISSUER_HOST
#: The extensions of the certificate of that host name, those of the certificate of 127.0.0.1 with its name.
HOST_EXTENSIONS = SERVER_EXTENSIONS.replace("IP:127.0.0.1", "DNS:" + ISSUER_HOST)


@dataclass(frozen=True)
class ClientKey:
    """The RSA client key of a run in its two forms, its public half and the ids of its record."""

    directory: Path
    pkcs1: Path
    pkcs8: Path
    public: Path
    key_id: str = KEY_ID
    client_id: str = CLIENT_ID

    def pem(self, form: str = "pkcs1") -> str:
        """The private key as PEM text, `form` `pkcs1` or `pkcs8`."""
        if form not in KEY_FORMS:
            raise ValueError("no key form named %r" % form)
        return (self.pkcs1 if form == "pkcs1" else self.pkcs8).read_text(encoding="ascii")

    def auth_key(self, form: str = "pkcs1") -> dict:
        """The record of `AUTH_KEY` and of a key file, in the key order of the plan."""
        return {"type": "application", "keyId": self.key_id, "key": self.pem(form), "clientId": self.client_id}

    def auth_key_json(self, form: str = "pkcs1") -> str:
        """The record as the one line of JSON the variable `AUTH_KEY` holds."""
        return json.dumps(self.auth_key(form), separators=(",", ":"))

    def write_key_file(self, path, form: str = "pkcs1") -> Path:
        """Writes the record to `path`, the `key.json` a side reads from its working directory."""
        path = Path(path)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(self.auth_key_json(form) + "\n", encoding="ascii")
        return path

    def public_pkcs1(self) -> str:
        """The public half as PEM text in the PKCS#1 form, `-----BEGIN RSA PUBLIC KEY-----`, as openssl writes it."""
        done = run(["openssl", "rsa", "-in", self.pkcs1, "-RSAPublicKey_out"], cwd=self.directory, timeout_s=60)
        text = done.out
        if not text.startswith("-----BEGIN RSA PUBLIC KEY-----"):
            raise ValueError("openssl wrote no PKCS#1 public key of %s" % self.pkcs1)
        return text


@dataclass(frozen=True)
class TlsFiles:
    """The files one call of `generate` wrote."""

    directory: Path
    ca_key: Path
    ca_cert: Path
    key: Path
    cert: Path
    openssl: str
    client: ClientKey | None = None
    host_key: Path | None = None
    host_cert: Path | None = None

    def environment(self, side: str) -> dict:
        """The environment that makes `side`, `reference` or `twin`, trust the CA, in the names of fixtures.py."""
        from .fixtures import ca_environment

        return ca_environment(side, self.ca_cert)


def openssl_version() -> str:
    """The version line of the `openssl` on the search path."""
    return run(["openssl", "version"], timeout_s=30).out.strip()


def generate(root=None, directory=None, with_client_key: bool = False, with_host_name: bool = False) -> TlsFiles:
    """Writes a fresh CA and a certificate of 127.0.0.1 signed by it, and returns their paths.

    `directory` defaults to target/harness/tls/ of the tree, which the call
    empties first, so a set never mixes files of two runs. With
    `with_client_key` the call also writes the client key of the run into its
    subdirectory `client/`, which `TlsFiles.client` names, and with
    `with_host_name` the key and the certificate of `ISSUER_HOST`, which
    `TlsFiles.host_key` and `TlsFiles.host_cert` name.
    """
    base = (Path(directory) if directory is not None else paths(root).target / "harness" / "tls").resolve()
    if base.exists():
        shutil.rmtree(base)
    base.mkdir(parents=True)
    files = TlsFiles(
        directory=base,
        ca_key=base / "ca.key",
        ca_cert=base / "ca.pem",
        key=base / "127.0.0.1.key",
        cert=base / "127.0.0.1.pem",
        openssl=openssl_version(),
    )
    (base / "ca.cnf").write_text(CA_EXTENSIONS, encoding="ascii")
    (base / "server.cnf").write_text(SERVER_EXTENSIONS, encoding="ascii")
    curve = ["-newkey", "ec", "-pkeyopt", "ec_paramgen_curve:prime256v1", "-noenc"]
    run(["openssl", "req", "-x509", *curve, "-keyout", files.ca_key, "-out", files.ca_cert,
         "-days", str(DAYS), "-subj", CA_SUBJECT, "-config", base / "ca.cnf",
         "-not_before", _hours_ago(1)], cwd=base, timeout_s=60)
    run(["openssl", "req", "-new", *curve, "-keyout", files.key, "-out", base / "127.0.0.1.csr",
         "-subj", SERVER_SUBJECT, "-config", base / "ca.cnf"], cwd=base, timeout_s=60)
    run(["openssl", "x509", "-req", "-in", base / "127.0.0.1.csr", "-CA", files.ca_cert, "-CAkey", files.ca_key,
         "-set_serial", "0x" + secrets.token_hex(16), "-days", str(DAYS), "-not_before", _hours_ago(1),
         "-extfile", base / "server.cnf", "-extensions", "server", "-out", files.cert], cwd=base, timeout_s=60)
    run(["openssl", "verify", "-CAfile", files.ca_cert, "-x509_strict", files.cert], cwd=base, timeout_s=60)
    host_key = host_cert = None
    if with_host_name:
        host_key, host_cert = base / (ISSUER_HOST + ".key"), base / (ISSUER_HOST + ".pem")
        (base / "host.cnf").write_text(HOST_EXTENSIONS, encoding="ascii")
        run(["openssl", "req", "-new", *curve, "-keyout", host_key, "-out", base / (ISSUER_HOST + ".csr"),
             "-subj", HOST_SUBJECT, "-config", base / "ca.cnf"], cwd=base, timeout_s=60)
        run(["openssl", "x509", "-req", "-in", base / (ISSUER_HOST + ".csr"), "-CA", files.ca_cert, "-CAkey",
             files.ca_key, "-set_serial", "0x" + secrets.token_hex(16), "-days", str(DAYS), "-not_before",
             _hours_ago(1), "-extfile", base / "host.cnf", "-extensions", "server", "-out", host_cert],
            cwd=base, timeout_s=60)
        run(["openssl", "verify", "-CAfile", files.ca_cert, "-x509_strict", host_cert], cwd=base, timeout_s=60)
    if with_client_key or with_host_name:
        files = TlsFiles(files.directory, files.ca_key, files.ca_cert, files.key, files.cert, files.openssl,
                         client_key(root, directory=base / "client") if with_client_key else None, host_key,
                         host_cert)
    return files


def client_key(root=None, directory=None, key_id: str = KEY_ID, client_id: str = CLIENT_ID,
               bits: int = CLIENT_KEY_BITS, primes: int = 2) -> ClientKey:
    """Writes a fresh RSA client key in both forms with its public half, and returns their paths.

    `directory` defaults to target/harness/tls/client/ of the tree and must lie
    under target/harness/, so a key never reaches a committed path. The call
    empties it first. `key_id` and `client_id` are the ids of the record, which
    a second key of one run, the key of `b.json` of the variant `two_keys`,
    sets apart from the first. `bits` is the size of the modulus, 2048 by
    default, and `primes` the count of its primes, which `openssl genrsa`
    takes as `-primes`.
    """
    harness = (paths(root).target / "harness").resolve()
    base = (Path(directory) if directory is not None else harness / "tls" / "client").resolve()
    if not base.is_relative_to(harness):
        raise ValueError("a client key lives under %s and never at %s" % (harness, base))
    if base.exists():
        shutil.rmtree(base)
    base.mkdir(parents=True)
    key = ClientKey(directory=base, pkcs1=base / "client.pkcs1.pem", pkcs8=base / "client.pkcs8.pem",
                    public=base / "client.pub.pem", key_id=key_id, client_id=client_id)
    run(["openssl", "genrsa", "-traditional", "-primes", str(primes), "-out", key.pkcs1, str(bits)], cwd=base,
        timeout_s=KEY_S)
    run(["openssl", "pkcs8", "-topk8", "-nocrypt", "-in", key.pkcs1, "-out", key.pkcs8], cwd=base, timeout_s=60)
    run(["openssl", "rsa", "-in", key.pkcs1, "-pubout", "-out", key.public], cwd=base, timeout_s=60)
    if not key.pem("pkcs1").startswith("-----BEGIN RSA PRIVATE KEY-----"):
        raise ValueError("openssl wrote no PKCS#1 key at %s" % key.pkcs1)
    if not key.pem("pkcs8").startswith("-----BEGIN PRIVATE KEY-----"):
        raise ValueError("openssl wrote no PKCS#8 key at %s" % key.pkcs8)
    return key


def _hours_ago(hours: int) -> str:
    """The time `hours` before now in the form `YYYYMMDDHHMMSSZ` that `-not_before` reads."""
    moment = datetime.datetime.now(datetime.timezone.utc) - datetime.timedelta(hours=hours)
    return moment.strftime("%Y%m%d%H%M%SZ")


__all__ = [
    "CA_SUBJECT",
    "CLIENT_ID",
    "CLIENT_KEY_BITS",
    "ClientKey",
    "DAYS",
    "HOST_EXTENSIONS",
    "HOST_SUBJECT",
    "ISSUER_HOST",
    "KEY_FORMS",
    "KEY_ID",
    "KEY_S",
    "SERVER_SUBJECT",
    "TlsFiles",
    "client_key",
    "generate",
    "openssl_version",
]
