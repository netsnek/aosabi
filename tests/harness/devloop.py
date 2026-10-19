# SPDX-License-Identifier: MIT
"""Aosabi harness: one development loop, its edits, its probes and its rebuilds.

Step 5 of `70_cli` starts the `dev` command of the reference and `cargo aosabi dev` beside each other
on explicit ports, edits one source on both sides and polls until the probe
answers the changed schema. `start_dev` runs the command in a session of its
own with its two streams in files under target/harness/devloop/, `edit` writes
a source, `wait_for` polls a probe until it answers, `rebuilds` counts the
lines `[dev] rebuilding` the supervisor of Aosabi prints, and `close` ends the
whole process group through `os.killpg`, first with SIGINT, as a Ctrl C does,
then with SIGKILL, so no child of either supervisor outlives a notebook.
"""

from __future__ import annotations

import os
import re
import signal
import subprocess
import time
from pathlib import Path

from .paths import paths
from .procs import _environment

#: The line the supervisor of Aosabi prints before every rebuild.
REBUILDING = "[dev] rebuilding"

#: The line it prints after a rebuild that restarted the child.
RESTARTED = "[dev] restarted"


class DevLoop:
    """One running development loop, in a session of its own."""

    def __init__(self, cmd, cwd, port: int, env=None, name: str = "dev", root=None) -> None:
        if not port:
            raise ValueError("the port is explicit and never 0")
        logs = paths(root).root / "target" / "harness" / "devloop"
        logs.mkdir(parents=True, exist_ok=True)
        self.cmd = [str(part) for part in cmd]
        self.cwd = Path(cwd)
        self.port = port
        self.name = name
        self.out_path = logs / (name + ".out")
        self.err_path = logs / (name + ".err")
        full = {"PORT": str(port)}
        full.update(env or {})
        self.process = subprocess.Popen(
            self.cmd, cwd=str(self.cwd), env=_environment(full), stdin=subprocess.DEVNULL,
            stdout=self.out_path.open("wb"), stderr=self.err_path.open("wb"), start_new_session=True,
        )
        self.started = time.monotonic()
        self.code = None

    def stdout(self) -> bytes:
        """What the loop wrote to its standard output so far."""
        return self.out_path.read_bytes() if self.out_path.exists() else b""

    def stderr(self) -> bytes:
        """What the loop wrote to its standard error so far."""
        return self.err_path.read_bytes() if self.err_path.exists() else b""

    def lines(self) -> list:
        """Every line of both streams so far, standard output first, as text."""
        text = (self.stdout() + b"\n" + self.stderr()).decode("utf-8", errors="replace")
        return [line for line in text.split("\n") if line]

    def output(self) -> str:
        """Both streams under one header, which is what a failing cell prints."""
        return "\n".join([
            "dev loop %s on port %d: %s" % (self.name, self.port, " ".join(self.cmd)),
            "stdout:", self.stdout().decode("utf-8", errors="replace").rstrip("\n"),
            "stderr:", self.stderr().decode("utf-8", errors="replace").rstrip("\n"),
        ])

    def count(self, pattern: str) -> int:
        """The lines of both streams that match the regular expression `pattern`."""
        compiled = re.compile(pattern)
        return sum(1 for line in self.lines() if compiled.search(line))

    def rebuilds(self) -> int:
        """The rebuilds the supervisor of Aosabi started, counted from its lines."""
        return sum(1 for line in self.lines() if line.strip() == REBUILDING)

    def restarts(self) -> int:
        """The restarts the supervisor of Aosabi finished, counted from its lines."""
        return sum(1 for line in self.lines() if line.strip() == RESTARTED)

    def running(self) -> bool:
        """Whether the supervisor still runs."""
        return self.process.poll() is None

    def edit(self, path, text: str) -> float:
        """Writes `text` into `path`, relative to the loop's directory, and returns the time of the save."""
        target = Path(path)
        if not target.is_absolute():
            target = self.cwd / target
        target.write_text(text, encoding="utf-8")
        return time.monotonic()

    def wait_for(self, probe, timeout_s: float = 30.0, poll_s: float = 0.1):
        """Calls `probe()` until it answers a truthy value and returns it, or raises after `timeout_s`.

        A probe that raises counts as no answer yet, as a refused connection
        does while the child restarts. A supervisor that ends before the probe
        answers raises with its output.
        """
        deadline = time.monotonic() + timeout_s
        last = None
        while time.monotonic() < deadline:
            try:
                value = probe()
            except Exception as error:  # A probe of a restarting child fails until it binds.
                value, last = None, error
            if value:
                return value
            if not self.running():
                raise RuntimeError("the dev loop %s exited with %s\n%s" % (
                    self.name, self.process.returncode, self.output()))
            time.sleep(poll_s)
        raise TimeoutError("no answer within %ss, last error %r\n%s" % (timeout_s, last, self.output()))

    def close(self, grace_s: float = 10.0) -> int:
        """Ends the process group, SIGINT first and SIGKILL after `grace_s`, and returns the exit code."""
        if self.code is not None:
            return self.code
        for sig in (signal.SIGINT, signal.SIGKILL):
            try:
                os.killpg(self.process.pid, sig)
            except ProcessLookupError:
                break
            try:
                self.process.wait(timeout=grace_s)
                break
            except subprocess.TimeoutExpired:
                continue
        try:
            os.killpg(self.process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        self.code = self.process.wait(timeout=grace_s)
        return self.code

    def __enter__(self) -> "DevLoop":
        return self

    def __exit__(self, *_) -> bool:
        self.close()
        return False


def start_dev(cmd, cwd, port: int, env=None, name: str = "dev", root=None) -> DevLoop:
    """Starts the development loop `cmd` in `cwd` on `port` in a new session and returns it."""
    return DevLoop(cmd, cwd, port, env=env, name=name, root=root)
