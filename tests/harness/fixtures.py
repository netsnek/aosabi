# SPDX-License-Identifier: MIT
"""Aosabi harness: the staging, building and serving of both sides of a fixture.

The TypeScript side of a fixture is staged under target/harness/fixtures/, built
there with the vendored command line and served with the overlay
tests/sidecar/config-overlay.mjs in place of its built configuration, so
the build of the reference never writes into vendor/ or tests/fixtures/ and the port reaches
the reference through the plugin option. The Rust side of a fixture is its twin
under tests/fixtures/rs/, built with `cargo build --locked` and served with an
explicit, non zero `PORT`, because the twin keeps `use_node_server()` with no
option and the rule of the port is the same on both sides. A served twin writes
its boot report where `AOSABI_BOOT_REPORT` names a file, and `read_boot_report`
reads it. HTTP(Unix Socket) gets a fresh runtime directory of mode 0700 under
target/harness/.

A twin is built the way its layout asks. A twin with a `server/` crate of its own
is a cargo workspace, built with `cargo build --locked` in its directory. The
binary sits in the twin's `target/` under the name of its server package,
`<package>-server` for a generated one, at `target/debug/` in the dev profile and
at `target/<host>/release/` in the release profile, because a release build of the
command line passes `--target` with the host for the lean link. A helper that is
handed no cargo runs the cargo of the rustup home, `rustup_cargo`, so a notebook
builds a twin through `build_twin` and names no cargo of its own.

The opt ins `tls` and `h2c` of the card server stay out of a build of the byte
contract, because h2c answers the preface of HTTP/2 that Node refuses with 400. A
twin that offers them names them as features of its own package, which pins their
packages in its `Cargo.lock`, and `build_twin` with `opt_ins` builds a copy of the
twin under `target/harness/opt-ins/<opt ins>/<twin>/` whose table of the facade
for a Unix target names them, the form an application that wants them writes until
the card names them. The copy keeps the lock of the twin, so the lock check of
`cargo aosabi build` holds it to the same pins.

`issuer_environment` writes what one side of a variant needs: `OIDC_ISSUER`, the
CA variable of the side, and `AUTH_KEY` or the key files the variant asks for in
the working directory of the side, from the client key of tests/harness/tls.py.
The working directory of the reference is its stage, and that of the twin is a
directory of its own under target/harness/issuer-cwd/, which `serve_twin` takes as
`cwd`, because a key file never enters tests/fixtures/rs/.

Entries that run at once under `tests/run.py --jobs` can stage one fixture
together, so the stage writes its links under a lock of its directory and each
fixture stages and builds under a lock of its own, `.locks/<fixture>` of the
stage, held with `lock_dir` of procs.py. Another entry waits for that lock and
then finds the stage whole.

Every build of a twin takes its environment from `build_env`, whose one call of
`harness_env` of tests/harness/toolchain.py adds the remap of the source of the
standard library of the toolchain the build runs after the flags the caller gives.
A build on a toolchain with the component rust-src writes the panic locations of
the standard library with the path the prebuilt one carries, and a build without
the component writes the bytes it wrote before.

A later build at `prebuilt` puts its own binary back, since cargo links the
artifact of a binary to that place on every build, a fresh one included.
"""

from __future__ import annotations

import hashlib
import json
import os
import re
import shutil
import stat
from pathlib import Path

from .cli import REFERENCE_DIR, REFERENCE_PACKAGE, cli_binary
from .paths import paths, socket_dir
from .procs import CommandFailed, Server, lock_dir, run, serve
from .toolchain import harness_env, switch_env

#: The version of the decorator helper the vendored lock holds.
OXC_RUNTIME_VERSION = "0.101.0"
#: The version of @sentry/node the vendored lock holds, which the fixtures link.
SENTRY_NODE_VERSION = "8.55.0"
#: The variable each side reads the file of an extra CA from, by side.
CA_VARIABLES = {"reference": "NODE_EXTRA_CA_CERTS", "twin": "AOSABI_EXTRA_CA_CERTS"}

#: The variants of docs/harness.md section 8, which the overlay spreads by name,
#: and the two configurations of subscription-app, `identity` and `database`,
#: which the overlay lets through as names that spread no key, because the fixture
#: selects its plugins by `AOSABI_CONFIG` itself.
VARIANTS = ("default", "graphiql_false", "landing_false", "both", "logger_false", "identity", "database",
            "switches_on", "cors_star", "cors_list")

#: The failure variants of plugins-app never finish a boot, so
#: tests/harness/boot.py boots them and serve_reference does not. The app
#: zitadel-providers-app of tests/ts1/apps, which no fixture holds, selects its
#: providers the same way for section 5b of `39_ts1_parity`.
FIXTURE_VARIANTS = {
    "plugins-app": ("default", "fail_named", "fail_unnamed", "fail_value", "fail_last", "cycle"),
    "use-identity-app": ("default", "async", "numeric", "failing", "none", "twice"),
    "sentry-app": ("default", "null_key", "custom_key", "skip", "env_dsn", "initialized"),
    "zitadel-login-app": ("default", "endpoint", "http_issuer", "bad_key", "key_file", "no_key", "pkcs8_key"),
    "zitadel-auth-app": ("default", "mapped", "key_file", "two_keys", "with_login"),
    "zitadel-providers-app": ("default", "unused_first", "second_first", "per_request", "streamed"),
}

KEY_SOURCES = {
    "zitadel-login-app": {"default": "pkcs1", "endpoint": "pkcs1", "http_issuer": "pkcs1", "bad_key": "bad_json",
                          "key_file": "file", "no_key": "none", "pkcs8_key": "pkcs8"},
    "zitadel-auth-app": {"default": "pkcs1", "mapped": "pkcs1", "key_file": "file", "two_keys": "files_ab",
                         "with_login": "pkcs1"},
    "zitadel-providers-app": {"default": "pkcs1", "unused_first": "pkcs1", "second_first": "files_ab",
                              "per_request": "pkcs1", "streamed": "pkcs1"},
}
#: The value of `AUTH_KEY` under `bad_json`, a text that is no JSON.
BAD_AUTH_KEY = "not json"
#: The key files a variant may find in a working directory, which `issuer_environment` removes where it asks none.
KEY_FILES = ("key.json", "a.json", "b.json")

#: Data of the vendored reference the compatibility layer is held to: the folder its build writes
#: and the configuration file of a fixture.
REFERENCE_OUT = ".pylon"
REFERENCE_CONFIG = "pylon.config.ts"

#: The TypeScript fixtures of tests/fixtures/ts/, the rest are the reference's own.
OWN_FIXTURES = (
    "async-order-app", "docs-app", "errors-app", "federation-boundary-app", "federation-catalogue-app",
    "identity-app", "numbers-app", "plugins-app", "pubsub-app", "roots-app", "sentry-app",
    "static-pages-app", "subscription-app", "surface-app", "use-identity-app", "zitadel-auth-app",
    "zitadel-login-app",
)

#: The port the variant `down` of the boundary pair points the front at under
#: `--jobs 1`. It is the last port of the range of 90_federation, which no case
#: binds, so a delegation there meets a closed port, and a run with jobs shifts it
#: by the `port_offset` of its slot as it shifts every port of the notebook.
FEDERATION_DOWN_PORT = 4929


def federation_variants(port_offset: int = 0) -> dict:
    """Each changes the federation of the front and not its configuration, so the
    overlay serves each as `default`, and the variant travels on both sides as the
    environment below, which the front reads at boot.
    """
    return {
        "default": {},
        "no_strict": {"CATALOGUE_STRICT": "false"},
        "down": {"CATALOGUE_URL": "http://127.0.0.1:%d/graphql" % (FEDERATION_DOWN_PORT + port_offset)},
        "headers": {"CATALOGUE_HEADERS": "node"},
        "shared": {"CATALOGUE_SECOND": "service"},
    }


#: The fixtures whose own variants change their environment and not the
#: configuration `AOSABI_CONFIG` names, each with the function of its table.
ENVIRONMENT_VARIANTS = {"federation-boundary-app": federation_variants}


def variant_environment(name: str, variant: str, port_offset: int = 0) -> tuple[str, dict]:
    """The configuration and the environment one variant of a fixture serves with.

    A variant of docs/harness.md section 8 or of `FIXTURE_VARIANTS` is the
    configuration `AOSABI_CONFIG` names and adds no environment. A variant of
    `ENVIRONMENT_VARIANTS` is served as the overlay's `default` with the
    environment of its table, whose ports `port_offset` shifts.
    """
    table = ENVIRONMENT_VARIANTS.get(name)
    if table is not None:
        own = table(port_offset)
        if variant in own:
            return "default", dict(own[variant])
    if variant in variants_of(name):
        return variant, {}
    raise ValueError("no variant named %s for %s" % (variant, name))


def variants_of(name: str) -> tuple:
    """The variants a fixture is served under: its own, or the seven of the overlay."""
    return FIXTURE_VARIANTS.get(name, VARIANTS)


def fixture_source(name: str, root=None) -> Path:
    """The source directory of a TypeScript fixture, the repository's own or the reference's, or of an app of the
    TypeScript option under tests/ts1/apps that no fixture holds, such as zitadel-providers-app."""
    base = paths(root).root
    own = base / "tests" / "fixtures" / "ts" / name
    if own.is_dir():
        return own
    vendored = base / REFERENCE_DIR / "e2e" / "fixtures" / name
    if vendored.is_dir():
        return vendored
    app = base / "tests" / "ts1" / "apps" / name
    if app.is_dir():
        return app
    raise FileNotFoundError("no TypeScript fixture named %s" % name)


def stage_root(root=None) -> Path:
    """The directory every staged fixture sits in, with its package.json and node_modules."""
    base = paths(root).root
    stage = base / "target" / "harness" / "fixtures"
    modules = stage / "node_modules"
    stage.mkdir(parents=True, exist_ok=True)
    package = base / REFERENCE_DIR / "packages" / REFERENCE_PACKAGE[1]
    links = {
        modules.joinpath(*REFERENCE_PACKAGE): package,
        modules / "graphql-yoga": package / "node_modules" / "graphql-yoga",
        modules / "@sentry" / "node": base / REFERENCE_DIR / "node_modules" / ".pnpm"
        / ("@sentry+node@" + SENTRY_NODE_VERSION) / "node_modules" / "@sentry" / "node",
        modules / "@oxc-project" / "runtime": base / REFERENCE_DIR / "node_modules" / ".pnpm"
        / ("@oxc-project+runtime@" + OXC_RUNTIME_VERSION) / "node_modules" / "@oxc-project" / "runtime",
        modules / "@hono" / "node-server": package / "node_modules" / "@hono" / "node-server",
    }
    with lock_dir(stage):
        if not (stage / "package.json").exists():
            (stage / "package.json").write_text(
                json.dumps({"name": "aosabi-harness-stage", "private": True, "type": "module"}) + "\n"
            )
        for link, target in links.items():
            if not link.exists():
                link.parent.mkdir(parents=True, exist_ok=True)
                link.symlink_to(target)
    return stage


def source_digest(source: Path) -> str:
    """The SHA-256 over the paths and bytes of a fixture's sources, which decides a rebuild."""
    digest = hashlib.sha256()
    for path in sorted(Path(source).rglob("*")):
        if path.is_file() and REFERENCE_OUT not in path.parts and "node_modules" not in path.parts:
            digest.update(str(path.relative_to(source)).encode() + b"\0" + path.read_bytes() + b"\0")
    return digest.hexdigest()


def stage_fixture(name: str, root=None, rebuild: bool = False, timeout_s: float = 300.0) -> Path:
    """Stages, builds and overlays one TypeScript fixture, and returns its staged root.

    A fixture that ships no configuration file is staged with an empty one,
    `export default {}`, because the entry of a build imports a configuration only
    when the fixture has one, and the overlay then serves it with the serving
    plugin alone. The build runs once and is kept while the digest of the
    fixture's sources is the one its stage recorded, and a changed source or
    `rebuild` stages and builds it again. The whole step runs under the lock of
    the fixture, so two entries that run at once stage and build it once.
    """
    base = paths(root).root
    stage = stage_root(root)
    target = stage / name
    source = fixture_source(name, root)
    lock = stage / ".locks" / name
    lock.mkdir(parents=True, exist_ok=True)
    with lock_dir(lock):
        digest = source_digest(source)
        stamp = target / ".aosabi-stage-digest"
        if target.exists() and (rebuild or not stamp.exists() or stamp.read_text() != digest):
            shutil.rmtree(target)
        if not target.exists():
            shutil.copytree(source, target, ignore=shutil.ignore_patterns(REFERENCE_OUT))
            stamp.write_text(digest)
            if not (target / REFERENCE_CONFIG).exists():
                (target / REFERENCE_CONFIG).write_text(
                    "// Written by tests/harness/fixtures.py for a fixture with no configuration.\n"
                    "export default {}\n"
                )
        built = target / REFERENCE_OUT / REFERENCE_CONFIG.replace(".ts", ".js")
        overlay = base / "tests" / "sidecar" / "config-overlay.mjs"
        if not (target / REFERENCE_OUT / "server.mjs").exists():
            cli = base / REFERENCE_DIR / "packages" / REFERENCE_PACKAGE[1] / "dist" / "cli" / "index.js"
            run(["node", str(cli), "build"], cwd=target, env={"AOSABI_CONFIG": "default"}, timeout_s=timeout_s)
            built.rename(target / REFERENCE_OUT / REFERENCE_CONFIG.replace(".ts", ".orig.js"))
            shutil.copyfile(overlay, built)
        elif built.read_bytes() != overlay.read_bytes():
            # The overlay is harness code and not a source of the fixture, so its
            # digest decides no rebuild, and a stage keeps the overlay of today.
            shutil.copyfile(overlay, built)
    return target


def ca_environment(side: str, ca_cert) -> dict:
    """The environment that makes `side`, `reference` or `twin`, trust the CA file `ca_cert` beside its own roots."""
    if side not in CA_VARIABLES:
        raise ValueError("no side named %s" % side)
    return {CA_VARIABLES[side]: str(Path(ca_cert))}


def side_cwd(name: str, side: str, root=None) -> Path:
    """The working directory a side of a fixture runs in: the stage of the reference,
    or a directory of the twin under target/harness/issuer-cwd/, which a key file
    may enter."""
    if side == "reference":
        return stage_fixture(name, root)
    if side != "twin":
        raise ValueError("no side named %s" % side)
    path = paths(root).target / "harness" / "issuer-cwd" / name
    path.mkdir(parents=True, exist_ok=True)
    return path


def issuer_environment(name: str, variant: str, side: str, issuer: str, tls_files, keys=None,
                       root=None) -> dict:
    """`issuer` is the URL of the mocked issuer, `https://127.0.0.1:<port>`, which
    both sides read as `OIDC_ISSUER`, `tls_files` the files of tls.py whose CA
    the side trusts, and `keys` the client keys, the key of the run first and,
    for `two_keys`, the key of `b.json` second, which defaults to the first.
    The call writes `key.json` or `a.json` and `b.json` into the working
    directory of the side, `side_cwd`, and removes those a variant asks none
    of, so a stage never hands one variant the key file of another. A variant
    that reads a file sets `AUTH_KEY` to None, which unsets it.
    """
    sources = KEY_SOURCES.get(name)
    if sources is None or variant not in sources:
        raise ValueError("no issuer variant %s of %s" % (variant, name))
    keys = list(keys) if keys else ([tls_files.client] if getattr(tls_files, "client", None) else [])
    source = sources[variant]
    if source != "none" and source != "bad_json" and not keys:
        raise ValueError("the variant %s of %s needs a client key" % (variant, name))
    cwd = side_cwd(name, side, root)
    for stale in KEY_FILES:
        (cwd / stale).unlink(missing_ok=True)
    env = {"OIDC_ISSUER": issuer, **ca_environment(side, tls_files.ca_cert), "AUTH_KEY": None}
    if source in ("pkcs1", "pkcs8"):
        env["AUTH_KEY"] = keys[0].auth_key_json(source)
    elif source == "bad_json":
        env["AUTH_KEY"] = BAD_AUTH_KEY
    elif source == "file":
        keys[0].write_key_file(cwd / "key.json")
    elif source == "files_ab":
        keys[0].write_key_file(cwd / "a.json")
        (keys[1] if len(keys) > 1 else keys[0]).write_key_file(cwd / "b.json")
    return env


def graphql_ready(port: int) -> bool:
    """The readiness probe of docs/harness.md section 2.1, over the raw client."""
    from .http import send, tcp

    answer, _ = send(tcp(port), b'POST /graphql HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\n'
                     b'Content-Length: 24\r\n\r\n{"query":"{__typename}"}', "POST", timeout=3.0)
    return answer is not None and answer.status == 200 and b'"__typename"' in answer.decoded


def serve_reference(name: str, port: int, variant: str = "default", node_env: str | None = "test",
                    env: dict | None = None, root=None, timeout_s: float = 40.0, port_offset: int = 0) -> Server:
    """Serves the staged reference of a fixture on `port` through its own entry and the overlay.

    `node_env` is `test`, `production` or `None` for unset, never `development`,
    where the serving plugin binds nothing, which `serve_development` covers.
    The environment of a variant of `ENVIRONMENT_VARIANTS` is applied after
    `env`, so the variant `down` keeps its closed port whatever URL the caller
    names, shifted by `port_offset` as the notebook shifts its own ports.
    """
    overlay, variant_env = variant_environment(name, variant, port_offset)
    if node_env == "development":
        raise ValueError("the development variant is served by serve_development")
    staged = stage_fixture(name, root)
    full = {"AOSABI_OVERLAY_PORT": str(port), "AOSABI_CONFIG": overlay, "NODE_ENV": node_env, "PORT": None}
    full.update(env or {})
    full.update(variant_env)
    return serve(["node", "--enable-source-maps", REFERENCE_OUT + "/server.mjs"], port, graphql_ready, cwd=staged,
                 env=full, name="%s %s %s" % (name, variant, node_env or "unset"), timeout_s=timeout_s)


def serve_development(name: str, port: int, variant: str = "default", env: dict | None = None, root=None,
                      timeout_s: float = 40.0, port_offset: int = 0) -> Server:
    """Serves the development variant of a fixture through tests/sidecar/serve-dev.mjs.

    A variant and `env` reach the front as they reach `serve_reference`: the
    overlay's variant or a fixture's own in `AOSABI_CONFIG`, then `env`, then
    the environment of a variant of `ENVIRONMENT_VARIANTS`, so the variants
    `no_strict` and `down` of the boundary pair serve under development as
    well.
    """
    overlay, variant_env = variant_environment(name, variant, port_offset)
    base = paths(root).root
    staged = stage_fixture(name, root)
    full = {"AOSABI_OVERLAY_PORT": str(port), "AOSABI_CONFIG": overlay, "NODE_ENV": None}
    full.update(env or {})
    full.update(variant_env)
    return serve(["node", str(base / "tests" / "sidecar" / "serve-dev.mjs"), str(staged), str(port)], port,
                 graphql_ready, cwd=base, env=full,
                 name="%s %s development" % (name, variant), timeout_s=timeout_s)


def serve_ts1(name: str, port: int, host: str, module, variant: str = "default", node_env: str | None = "test",
              mode: str = "faithful", read: str = "get", root=None, timeout_s: float = 90.0, env=None,
              ready=None) -> Server:
    """The side is the reference's own build of the fixture with only its import specifiers
    rewritten, booted on the module `module` of `aosabi-js` through the staged
    package `aosabi`, which tests/harness/ts1.py stages and serves. A variant and
    `node_env` reach it as they reach `serve_reference`, through the configuration
    overlay, and the variables of `env` and the readiness check `ready` as
    `ts1.serve_judge` takes them.
    """
    from . import ts1

    stage = ts1.stage_judge(name, Path(module), root)
    return ts1.serve_judge(stage, host, port, variant, node_env, mode, read, root, timeout_s, env=env, ready=ready)


def twin_dir(name: str, root=None) -> Path:
    """The workspace of the Rust twin of a fixture."""
    path = paths(root).root / "tests" / "fixtures" / "rs" / name
    if not (path / "twin.toml").is_file():
        raise FileNotFoundError("no twin named %s" % name)
    return path


def twin_layout(name: str, root=None) -> str:
    """The layout of a twin, `workspace` with a `server/` crate of its own, `scaffold` as one package."""
    directory = twin_dir(name, root)
    return "workspace" if (directory / "server" / "Cargo.toml").is_file() else "scaffold"


def is_served_twin(name: str, root=None) -> bool:
    """The main a build generates for a twin in the scaffold layout calls one of them
    always, `aosabi::serve` with the configuration of `aosabi.config.rs` and
    `aosabi::serve_app` with the configuration the app of a twin without that file
    carries.
    """
    if twin_layout(name, root) == "scaffold":
        return True
    main = twin_dir(name, root) / "server" / "src" / "main.rs"
    if not main.is_file():
        return False
    text = main.read_text(encoding="utf-8")
    return "aosabi::serve(" in text or "aosabi::serve_app(" in text


def is_graphtp_twin(name: str, root=None) -> bool:
    """Whether a twin serves its app through the GraphTP listener, the `test-graphtp`
    twin of its fixture that the served goldens of capture row C61 need: a binary
    `graphtp` in its server package. A twin that calls `aosabi::serve` serves HTTP
    and no GraphTP, so `is_served_twin` is no answer to it."""
    try:
        return (twin_dir(name, root) / "server" / "src" / "bin" / "graphtp.rs").is_file()
    except FileNotFoundError:
        return False


def rustup_cargo() -> Path:
    """The cargo of the rustup home, which pins the toolchain of rust-toolchain.toml."""
    return Path(os.environ.get("CARGO_HOME") or Path.home() / ".cargo") / "bin" / "cargo"


_HOSTS = {}


def host_triple(cargo=None) -> str:
    """The host `rustc -vV` names beside `cargo`, where a release build of the command line writes."""
    rustc = Path(cargo or rustup_cargo()).parent / "rustc"
    if rustc not in _HOSTS:
        listed = run([str(rustc), "-vV"], timeout_s=60).out
        _HOSTS[rustc] = next(line.split(":", 1)[1].strip() for line in listed.splitlines()
                             if line.startswith("host:"))
    return _HOSTS[rustc]


#: The table of the facade for a Unix target, as a twin of the scaffold layout writes it.
_UNIX_TABLE = re.compile(r'(\[target\.\'cfg\(unix\)\'\.dependencies\]\naosabi = \{ path = )"([^"]*)"(, features = \[)([^\]]*)\]')


def opt_in_dir(name: str, opt_ins, root=None) -> Path:
    """The copy of a twin whose table of the facade for a Unix target also names `opt_ins`.

    The copy holds the manifest with the paths of the facade and of the build side
    made relative to it, the lock, the build script, `src/`, the configuration and
    `twin.toml`, each written only where its bytes changed, so cargo keeps its
    build of the copy.
    """
    source = twin_dir(name, root)
    base = paths(root).root
    copy = base / "target" / "harness" / "opt-ins" / "-".join(opt_ins) / name
    up = "../" * len(copy.relative_to(base).parts)
    manifest = (source / "Cargo.toml").read_text(encoding="utf-8").replace('"../../../../crates/', '"%scrates/' % up)
    table = _UNIX_TABLE.search(manifest)
    if table is None:
        raise ValueError("the twin %s names no table of the facade for a Unix target" % name)
    named = table.group(4) + "".join(', "%s"' % opt_in for opt_in in opt_ins)
    manifest = manifest[:table.start(4)] + named + manifest[table.end(4):]
    files = {"Cargo.toml": manifest.encode("utf-8")}
    for path in sorted(source.rglob("*")):
        relative = path.relative_to(source)
        if relative.parts[0] in ("target", "Cargo.toml") or not path.is_file():
            continue
        files[str(relative)] = path.read_bytes()
    for relative, data in files.items():
        dest = copy / relative
        dest.parent.mkdir(parents=True, exist_ok=True)
        if not dest.is_file() or dest.read_bytes() != data:
            dest.write_bytes(data)
    held = []
    for top, dirs, names in os.walk(copy):
        if Path(top) == copy:
            dirs[:] = [name for name in dirs if name != "target"]
        held.append(Path(top))
        for name in names:
            path = Path(top) / name
            if str(path.relative_to(copy)) not in files:
                path.unlink()
    for directory in reversed(held):
        if directory != copy and not any(directory.iterdir()):
            directory.rmdir()
    return copy


def build_env(cargo, env=None) -> dict:
    """The environment of a build of a twin: the cargo it runs, `env`, and the remap of the source of std.

    The remap stands after the flags `env` gives, so a caller's RUSTFLAGS keep
    their place, and it names the sysroot of the toolchain the build runs,
    the one RUSTUP_TOOLCHAIN names or the pin.
    """
    full = {"CARGO": str(cargo)}
    full.update(env or {})
    return harness_env(full)


def build_project(directory, cargo=None, release: bool = True, root=None, timeout_s: float = 1800.0, env=None,
                  check: bool = True):
    """Builds the service of the layout of the scaffold at `directory` with `cargo-aosabi build --locked`.

    The build runs from the root under a lock of `directory`, since two entries
    that run at once write the same generated workspace. A release build takes the
    switch AOSABI_STD, which the command line reads, and `switch_env` gives it the
    private copy of the pin at `source`, so the command line builds the card
    `server` with its set and puts the binary where a build at `prebuilt` puts its
    own, and a dev build takes nothing of the switch.
    """
    cargo = Path(cargo or rustup_cargo())
    directory = Path(directory)
    full = build_env(cargo, switch_env(env, paths(root).root) if release else env)
    program = cli_binary(cargo, "cargo-aosabi", root, timeout_s=timeout_s)
    argv = [str(program), "build", "--locked", "--manifest-path", str(directory / "Cargo.toml")]
    argv += ["--release"] if release else []
    with lock_dir(directory):
        done = run(argv, cwd=paths(root).root, env=full, timeout_s=timeout_s, check=False)
    if check and done.returncode != 0:
        raise CommandFailed(done, "exit code %s, expected 0" % done.returncode)
    return done


def build_twin(name: str, cargo=None, release: bool = True, root=None, timeout_s: float = 1800.0, env=None,
               check: bool = True, opt_ins=()):
    """Builds a twin with `--locked`, the rule of every cargo call of the harness, as its layout asks.

    A twin of the scaffold layout builds with `build_project`, and `opt_ins`
    builds the copy of `opt_in_dir` instead.
    """
    if twin_layout(name, root) == "workspace":
        cargo = Path(cargo or rustup_cargo())
        full = build_env(cargo, env)
        return run([str(cargo), "build", "--locked"] + (["--release"] if release else []), cwd=twin_dir(name, root),
                   env=full, timeout_s=timeout_s, check=check)
    directory = opt_in_dir(name, opt_ins, root) if opt_ins else twin_dir(name, root)
    return build_project(directory, cargo, release, root, timeout_s, env, check)


def generated_manifest(name: str, root=None, cargo=None, opt_ins=()) -> Path:
    """The manifest of the workspace `cargo aosabi build` generates for a twin of the scaffold layout.

    A notebook builds the server package of that workspace with cargo itself where
    it needs a profile the command line does not build, such as `size`, with
    `--target-dir` the twin's `target/`, so the build script writes the artifacts
    where the command line writes them.
    """
    directory = opt_in_dir(name, opt_ins, root) if opt_ins else twin_dir(name, root)
    build_twin(name, cargo, release=False, root=root, opt_ins=opt_ins, check=False)
    return directory / "target" / "aosabi" / "Cargo.toml"


def package_name(manifest: Path) -> str:
    """The `name` of the `[package]` table of a `Cargo.toml`."""
    table = None
    for line in manifest.read_text(encoding="utf-8").splitlines():
        stripped = line.strip()
        if stripped.startswith("["):
            table = stripped
        elif table == "[package]" and stripped.startswith("name") and "=" in stripped:
            return stripped.split("=", 1)[1].strip().strip('"')
    raise ValueError("no [package] name in %s" % manifest)


def twin_binary(name: str, release: bool = True, root=None, opt_ins=()) -> Path:
    """The server binary a twin's build writes, named after its server package."""
    directory = twin_dir(name, root)
    if twin_layout(name, root) == "workspace":
        package = package_name(directory / "server" / "Cargo.toml")
        return directory / "target" / ("release" if release else "debug") / package
    package = package_name(directory / "Cargo.toml") + "-server"
    if opt_ins:
        directory = opt_in_dir(name, opt_ins, root)
    if release:
        return directory / "target" / host_triple() / "release" / package
    return directory / "target" / "debug" / package


def serve_twin(name: str, port: int, variant: str = "default", node_env: str | None = "test",
               env: dict | None = None, boot_report=None, unix_socket=None, release: bool = True,
               root=None, timeout_s: float = 40.0, cwd=None, port_offset: int = 0) -> Server:
    """Serves a built twin on `port` with the environment of its variant.

    `boot_report` names the file the boot writes its report into, and
    `unix_socket` a runtime directory from `runtime_dir` under which the twin also
    answers HTTP(Unix Socket) through `AOSABI_UNIX_SOCKET=1`. `cwd` is the working
    directory, the twin's workspace unless a caller names another, such as the
    directory of `side_cwd` that holds the key file of a variant. A variant
    reaches the twin as it reaches the reference, the configuration in
    `AOSABI_CONFIG` and the environment of a variant of `ENVIRONMENT_VARIANTS`
    last, its ports shifted by `port_offset`.
    """
    overlay, variant_env = variant_environment(name, variant, port_offset)
    full = {"PORT": str(port), "AOSABI_CONFIG": overlay, "NODE_ENV": node_env}
    if boot_report is not None:
        full["AOSABI_BOOT_REPORT"] = str(boot_report)
    if unix_socket is not None:
        full["AOSABI_UNIX_SOCKET"] = "1"
        full["XDG_RUNTIME_DIR"] = str(unix_socket)
    full.update(env or {})
    full.update(variant_env)
    return serve([str(twin_binary(name, release, root))], port, graphql_ready,
                 cwd=cwd if cwd is not None else twin_dir(name, root),
                 env=full, name="%s twin %s %s" % (name, variant, node_env or "unset"), timeout_s=timeout_s)


def read_boot_report(path) -> dict | None:
    """The boot report a twin wrote, its plugins and its routes, or `None` when it wrote none."""
    path = Path(path)
    if not path.is_file():
        return None
    return json.loads(path.read_text(encoding="utf-8"))


def runtime_dir(label: str, root=None, longest_name: int = 32) -> Path:
    """A fresh runtime directory of mode 0700 for HTTP(Unix Socket).

    It sits under target/harness/, or under `socket_dir` of paths.py when a
    socket name of `longest_name` bytes there would not fit `sun_path`.
    """
    path = socket_dir(paths(root).root / "target" / "harness" / "runtime" / label, longest_name)
    if path.exists():
        shutil.rmtree(path)
    path.mkdir(parents=True)
    os.chmod(path, stat.S_IRWXU)
    return path
