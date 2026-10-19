# SPDX-License-Identifier: MIT
"""Aosabi harness: the boot of one side of a fixture, captured until readiness, a dump file or exit.

A boot log is standard output and the status, so a boot is a process of its own
whose output is kept from its first byte. The helper starts the reference through
the built `server.mjs` or `worker.mjs` of the build folder of a stage of
`tests/harness/fixtures.py`, and the twin through its server binary, and waits for
the first of three things: the listen line on standard output,
the listen line of the reference and
`Aosabi running at http://localhost:<port>` of the side under test, divergence D53
of docs/byte-equality.md section 3, the dump file `AOSABI_BOOT_DUMP` names, which
the fixture's plugin of the last pass writes and which is the readiness of a worker
boot that binds no port, or the end of the process, which is how a failing boot
ends. It keeps standard output as it stood at that moment, ends a process still
running with `SIGTERM` on both sides alike, and reads what came after.

The streams go to files under `target/harness/boot/`, never under the
temporary directory of the system. The reference half of the boot report comes
from `tests/sidecar/boot-report.mjs`, which `reference_report` runs, and the
twin writes its own where `AOSABI_BOOT_REPORT` names a file.
"""

from __future__ import annotations

import json
import re
import shutil
import signal
import socket
import subprocess
import time
from dataclasses import dataclass
from pathlib import Path

from .fixtures import REFERENCE_OUT, stage_fixture, twin_binary, twin_dir, variants_of
from .masks import apply as apply_masks
from .paths import paths
from .procs import _environment, run

#: The listen line of the serving plugin, whose port the mask `boot-port` replaces,
#: with the name of the reference or of Aosabi, divergence D53 of
#: docs/byte-equality.md section 3.
LISTEN = re.compile(rb"(?:Pylon|Aosabi) running at http://localhost:([0-9]+)")

#: The listen line of the reference and the one of the side under test, divergence D53.
LISTEN_REFERENCE = b"Pylon running at "
LISTEN_AOSABI = b"Aosabi running at "

#: The variable of the role the reference reads, data of the vendored reference that the
#: compatibility layer is held to.
REFERENCE_ROLE = "PYLON_ROLE"

#: The masks a boot log is compared after.
BOOT_MASKS = ("time", "boot-port")

#: The masks a boot log of the side under test is compared after, whose listen line
#: names Aosabi, divergence D53 of docs/byte-equality.md section 3.
SIDE_MASKS = BOOT_MASKS + ("boot-port-aosabi",)

#: The prefixes of the two lines of standard error a failing boot is compared on.
ERROR_PREFIX = "Error: "
CAUSE_PREFIX = "  [cause]: "


@dataclass
class Boot:
    """What one boot left behind: its readiness, its status and its streams."""

    side: str
    label: str
    argv: list
    port: int
    ready: str
    status: int | None
    stdout: bytes
    stdout_all: bytes
    stderr: bytes
    dump: bytes | None
    port_bound: bool
    duration_s: float

    def masked_stdout(self, masks=None) -> bytes:
        """Standard output up to readiness with the masks of a boot log applied.

        The boot of the reference takes `BOOT_MASKS` and every other side
        `SIDE_MASKS` when `masks` names none.
        """
        if masks is None:
            masks = BOOT_MASKS if self.side == "reference" else SIDE_MASKS
        return apply_masks(self.stdout, masks)

    def masked_as_reference(self, masks=None) -> bytes:
        """Standard output masked as `masked_stdout` gives it, the listen line of D53 written as the reference writes it.

        A side under test writes `Aosabi running at` where the reference writes
        its own name, divergence D53 of docs/byte-equality.md section 3,
        so a comparison of its boot log with the reference's names that one
        line and holds every other byte to the reference.
        """
        return self.masked_stdout(masks).replace(LISTEN_AOSABI, LISTEN_REFERENCE)

    def stderr_lines(self) -> list:
        """Standard error as lines of text."""
        return self.stderr.decode("utf-8", errors="replace").splitlines()

    def error_line(self) -> str | None:
        """The first line of standard error that starts with `Error: `, or `None`."""
        return next((line for line in self.stderr_lines() if line.startswith(ERROR_PREFIX)), None)

    def cause_line(self) -> str | None:
        """The first line of standard error that starts with `  [cause]: `, or `None`."""
        return next((line for line in self.stderr_lines() if line.startswith(CAUSE_PREFIX)), None)

    def listened(self) -> bool:
        """Whether standard output carries the listen line."""
        return LISTEN.search(self.stdout_all) is not None


def boot_dir(label: str, root=None) -> Path:
    """A fresh directory of one boot under target/harness/boot/."""
    path = paths(root).target / "harness" / "boot" / label
    if path.exists():
        shutil.rmtree(path)
    path.mkdir(parents=True)
    return path


def _bound(port: int) -> bool:
    """Whether something accepts a connection on the loopback port."""
    try:
        with socket.create_connection(("127.0.0.1", port), timeout=0.5):
            return True
    except OSError:
        return False


def _dump_complete(path: Path | None) -> bool:
    """Whether the dump file exists and holds one whole JSON text and its newline."""
    if path is None or not path.is_file():
        return False
    data = path.read_bytes()
    if not data.endswith(b"\n"):
        return False
    try:
        json.loads(data)
    except ValueError:
        return False
    return True


def boot_process(argv, cwd, env: dict, port: int, label: str, side: str, dump: Path | None = None,
                 root=None, timeout_s: float = 40.0, poll_s: float = 0.02, report: Path | None = None,
                 listen: bool = True) -> Boot:
    """Starts `argv` and returns its boot once it listened, wrote its dump or ended.

    `dump` names the file a worker boot writes, and its presence makes the dump
    the readiness. A boot of a host that writes no listen line, the reference's build
    on Bun, takes `listen` false and is ready once `port` accepts a
    connection, the readiness `bound`. A boot that reaches none of the three
    within `timeout_s` ends with the readiness `timeout`, which a notebook
    reports visibly.
    `report` names the boot report a twin writes after both passes, which is
    after its listen line and its dump, so a ready process runs on until the
    report holds one whole JSON text before the harness ends it, while the
    standard output compared stays the one of the moment of readiness.
    """
    directory = boot_dir(label, root)
    out_path, err_path = directory / "stdout", directory / "stderr"
    started = time.monotonic()
    with out_path.open("wb") as out, err_path.open("wb") as err:
        process = subprocess.Popen([str(part) for part in argv], cwd=str(cwd), env=_environment(env),
                                   stdout=out, stderr=err)
        ready = "timeout"
        snapshot = b""
        port_bound = False
        deadline = started + timeout_s
        while time.monotonic() < deadline:
            code = process.poll()
            data = out_path.read_bytes()
            if dump is not None and _dump_complete(dump):
                ready, snapshot = "dump", data
                port_bound = _bound(port)
                break
            if dump is None and listen and LISTEN.search(data):
                ready, snapshot = "listen", data
                port_bound = _bound(port)
                break
            if dump is None and not listen and _bound(port):
                ready, snapshot, port_bound = "bound", out_path.read_bytes(), True
                break
            if code is not None:
                ready, snapshot = "exit", out_path.read_bytes()
                break
            time.sleep(poll_s)
        if ready == "timeout":
            snapshot = out_path.read_bytes()
        if report is not None and ready in ("listen", "dump", "bound"):
            while time.monotonic() < deadline and process.poll() is None and not _dump_complete(report):
                time.sleep(poll_s)
        # A process the harness ends carries no status of its own, so a boot
        # that was ready keeps `None` and only a boot that ended keeps its code.
        status = process.poll()
        if status is None:
            process.send_signal(signal.SIGTERM)
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=5)
    return Boot(
        side=side,
        label=label,
        argv=[str(part) for part in argv],
        port=port,
        ready=ready,
        status=status,
        stdout=snapshot,
        stdout_all=out_path.read_bytes(),
        stderr=err_path.read_bytes(),
        dump=dump.read_bytes() if dump is not None and dump.is_file() else None,
        port_bound=port_bound,
        duration_s=time.monotonic() - started,
    )


def boot_reference(name: str, port: int, variant: str = "default", node_env: str | None = "test",
                   role: str | None = None, entry: str = "server", dump: bool = False, env: dict | None = None,
                   root=None, timeout_s: float = 40.0) -> Boot:
    """Boots the staged reference of a fixture through its server or worker entry.

    `role` is written into `REFERENCE_ROLE`, the variable the reference reads, `None`
    for unset, and the worker entry sets `worker` itself. With `dump` the boot names a fresh dump file in
    `AOSABI_BOOT_DUMP` and waits for it. The port reaches the reference through
    the overlay, the rule of docs/harness.md section 2.1.
    """
    if variant not in variants_of(name):
        raise ValueError("no variant named %s" % variant)
    staged = stage_fixture(name, root)
    label = "reference-%s-%s-%s-%s-%s" % (name, variant, entry, role or "unset", node_env or "unset")
    dump_path = (paths(root).target / "harness" / "boot" / (label + ".dump.json")) if dump else None
    if dump_path is not None and dump_path.exists():
        dump_path.unlink()
    full = {"AOSABI_OVERLAY_PORT": str(port), "AOSABI_CONFIG": variant, "NODE_ENV": node_env,
            REFERENCE_ROLE: role, "PORT": None, "AOSABI_BOOT_DUMP": str(dump_path) if dump_path else None}
    full.update(env or {})
    argv = ["node", "--enable-source-maps", "%s/%s.mjs" % (REFERENCE_OUT, entry)]
    return boot_process(argv, staged, full, port, label,
                        "reference", dump_path, root, timeout_s)


def boot_twin(name: str, port: int, variant: str = "default", node_env: str | None = "test",
              role: str | None = None, dump: bool = False, report: Path | None = None, env: dict | None = None,
              release: bool = True, root=None, timeout_s: float = 40.0) -> Boot:
    """Boots the built twin of a fixture under a role, the twin of both entries of the reference.
    """
    label = "twin-%s-%s-%s-%s" % (name, variant, role or "unset", node_env or "unset")
    dump_path = (paths(root).target / "harness" / "boot" / (label + ".dump.json")) if dump else None
    if dump_path is not None and dump_path.exists():
        dump_path.unlink()
    full = {"PORT": str(port), "AOSABI_CONFIG": variant, "NODE_ENV": node_env, "AOSABI_ROLE": role,
            "AOSABI_BOOT_DUMP": str(dump_path) if dump_path else None,
            "AOSABI_BOOT_REPORT": str(report) if report is not None else None}
    full.update(env or {})
    return boot_process([twin_binary(name, release, root)], twin_dir(name, root), full, port, label, "twin",
                        dump_path, root, timeout_s, report=report)


def reference_report(name: str, port: int, variant: str = "default", node_env: str | None = "test",
                     role: str | None = None, entry: str = "server", root=None, timeout_s: float = 60.0) -> dict:
    """The reference half of the boot report, written by tests/sidecar/boot-report.mjs.

    The report carries the declared plugins of the configuration, the array
    executeConfig wrote back after both passes and the routes of the booted
    app. The server entry binds `port` for the moment the script runs.
    """
    base = paths(root).root
    staged = stage_fixture(name, root)
    out = boot_dir("report-%s-%s-%s-%s" % (name, variant, entry, role or "unset"), root) / "report.json"
    env = {"AOSABI_OVERLAY_PORT": str(port), "AOSABI_CONFIG": variant, "NODE_ENV": node_env, "PORT": None,
           REFERENCE_ROLE: None}
    run(["node", str(base / "tests" / "sidecar" / "boot-report.mjs"), str(staged), entry, role or "unset", str(out)],
        cwd=base, env=env, timeout_s=timeout_s)
    return json.loads(out.read_text(encoding="utf-8"))


def boot_bun(name: str, glue: Path, port: int, variant: str = "default", node_env: str | None = "test",
             role: str | None = None, entry: str = "server", dump: bool = False, side: str = "side",
             module: Path | None = None, ir: Path | None = None, engine: str = "wasm", env: dict | None = None,
             root=None, timeout_s: float = 40.0) -> Boot:
    """Boots a glue of the reference's build of a fixture on Bun, under a role.

    `glue` is the folder of the glue, the build folder of the staged fixture for the reference's
    build on Bun, the reference of the host, which tests/ts1/bun.js serves, or
    `glue/` of the stage of the judge mode for the TypeScript side, which
    tests/ts1/bun-glue.js runs on the entry aosabi/bun of the staged package with
    the IR `ir` and the engine `engine` of aosabi/bun, `module` naming the module
    of the stage the side takes. The reference's build on Bun is ready once the port
    accepts a connection, since its serving plugin binds on Node alone and Bun
    writes no listen line, and the side once it wrote the listen line, since
    aosabi/bun serves its plugin useNodeServer over Bun.serve as that plugin serves
    on Node. The worker entry is ready once it wrote its dump, the worker entry
    setting `REFERENCE_ROLE` to worker itself, which aosabi migrate renames
    `AOSABI_ROLE` in the glue of the side. `role` is written into `REFERENCE_ROLE` for
    the reference's build and into `AOSABI_ROLE` for the side. The port reaches the glue
    through the overlay, as it reaches the reference.
    """
    if variant not in variants_of(name):
        raise ValueError("no variant named %s" % variant)
    base = paths(root).root
    label = "bun-%s-%s-%s-%s-%s-%s" % (side, name, variant, entry, role or "unset", node_env or "unset")
    dump_path = (paths(root).target / "harness" / "boot" / (label + ".dump.json")) if dump else None
    if dump_path is not None and dump_path.exists():
        dump_path.unlink()
    bun = Path.home() / ".bun" / "bin" / "bun"
    if module is not None:
        argv = [bun, base / "tests" / "ts1" / "bun-glue.js", Path(glue) / ("%s.mjs" % entry), ir]
    else:
        argv = [bun, base / "tests" / "ts1" / "bun.js", Path(glue) / ("%s.mjs" % entry), port]
    role_name = REFERENCE_ROLE if module is None else "AOSABI_ROLE"
    full = {"AOSABI_OVERLAY_PORT": str(port), "AOSABI_CONFIG": variant, "NODE_ENV": node_env,
            role_name: role, "PORT": None, "AOSABI_BOOT_DUMP": str(dump_path) if dump_path else None,
            "AOSABI_ENGINE": engine if module is not None else None}
    full.update(env or {})
    return boot_process(argv, Path(glue).parent, full, port, label, side, dump_path, root, timeout_s,
                        listen=module is not None)
