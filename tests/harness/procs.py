# SPDX-License-Identifier: MIT
"""Aosabi harness: the two ways a notebook drives another process.

`run` is a command that ends, such as a devtool subcommand or a build. `serve`
is a server that keeps running, such as the reference on one port and Aosabi on another.
Both capture stdout and stderr and hand them to the notebook, because a notebook
captures the output of every process it drives so that a failing run explains
itself without a second run. `lock_dir` holds a directory while a notebook runs
a step that entries running at once share, such as an install.
"""

from __future__ import annotations

import contextlib
import fcntl
import os
import subprocess
import tempfile
import time
from dataclasses import dataclass, field
from pathlib import Path

# The machine carries a dead IPv6 route, so node picks the unreachable family
# first unless this is set.
NODE_OPTIONS = "--no-network-family-autoselection"

# The reference compares this environment value against the exact string "true".
TELEMETRY_OFF = {"PYLON_DISABLE_TELEMETRY": "true"}


@contextlib.contextmanager
def lock_dir(directory):
    """Hold an exclusive lock on `directory` while the block runs.

    Entries that run at once under `tests/run.py --jobs` can reach a step they
    share together, such as the install of `tests/clients`, so the step runs
    inside this block and another entry waits for it and then finds its result
    whole. The lock is `flock` on the directory itself, so it leaves no file,
    no child process inherits it, and it ends with the process that holds it.
    """
    fd = os.open(directory, os.O_RDONLY | os.O_DIRECTORY)
    try:
        fcntl.flock(fd, fcntl.LOCK_EX)
        yield Path(directory)
    finally:
        os.close(fd)


def _argv(cmd) -> list:
    if isinstance(cmd, (str, bytes, Path)):
        raise TypeError("pass the command as a list, the harness starts no shell")
    return [str(part) for part in cmd]


def _environment(env=None, argv=None) -> dict:
    """The child environment: this process, then the two harness pins, then env.
    """
    merged = dict(os.environ)
    merged.update(TELEMETRY_OFF)
    existing = merged.get("NODE_OPTIONS", "")
    if NODE_OPTIONS not in existing:
        merged["NODE_OPTIONS"] = (existing + " " + NODE_OPTIONS).strip()
    if env:
        for key, value in env.items():
            if value is None:
                merged.pop(key, None)
            else:
                merged[key] = str(value)
    if argv:
        from .toolchain import cli_toolchain

        merged.update(cli_toolchain(argv, merged))
    return merged


def _text(raw: bytes) -> str:
    return raw.decode("utf-8", errors="replace")


@dataclass
class CommandResult:
    """What a finished command left behind, with both streams as bytes."""

    cmd: list
    returncode: int
    stdout: bytes
    stderr: bytes
    duration_s: float

    @property
    def out(self) -> str:
        """stdout as text, for printing, never for the byte verdict."""
        return _text(self.stdout)

    @property
    def err(self) -> str:
        """stderr as text, for printing, never for the byte verdict."""
        return _text(self.stderr)

    def report(self) -> str:
        """Both streams under one header, which is what a failing cell prints."""
        return "\n".join(
            [
                "command: %s" % " ".join(self.cmd),
                "exit code: %s" % self.returncode,
                "stdout:",
                self.out.rstrip("\n"),
                "stderr:",
                self.err.rstrip("\n"),
            ]
        )


class CommandFailed(RuntimeError):
    """A command exited with an unexpected code or ran out of time."""

    def __init__(self, result: CommandResult, reason: str) -> None:
        self.result = result
        self.reason = reason
        super().__init__("%s\n%s" % (reason, result.report()))


def run(
    cmd,
    cwd=None,
    env=None,
    stdin: bytes = None,
    timeout_s: float = 60.0,
    check: bool = True,
    expect_returncode: int = 0,
) -> CommandResult:
    """Run `cmd` to completion and return its result.

    `cmd` is a list and no shell is started. The streams are captured as bytes,
    because a golden is bytes and a text decode would hide a byte difference. A
    non zero exit raises CommandFailed carrying both streams, unless `check` is
    false, which is how a notebook judges a build that must fail loud.
    """
    argv = _argv(cmd)
    started = time.monotonic()
    try:
        completed = subprocess.run(
            argv,
            cwd=str(cwd) if cwd else None,
            env=_environment(env, argv),
            input=stdin,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout_s,
        )
    except subprocess.TimeoutExpired as expired:
        result = CommandResult(
            cmd=argv,
            returncode=None,
            stdout=expired.stdout or b"",
            stderr=expired.stderr or b"",
            duration_s=time.monotonic() - started,
        )
        raise CommandFailed(result, "no exit within %ss" % timeout_s) from None
    except FileNotFoundError as missing:
        result = CommandResult(
            cmd=argv,
            returncode=None,
            stdout=b"",
            stderr=str(missing).encode("utf-8"),
            duration_s=time.monotonic() - started,
        )
        raise CommandFailed(result, "command not found") from None

    result = CommandResult(
        cmd=argv,
        returncode=completed.returncode,
        stdout=completed.stdout,
        stderr=completed.stderr,
        duration_s=time.monotonic() - started,
    )
    if check and result.returncode != expect_returncode:
        raise CommandFailed(
            result, "exit code %s, expected %s" % (result.returncode, expect_returncode)
        )
    return result


class ServerFailed(RuntimeError):
    """A server exited before it was ready, or never became ready."""

    def __init__(self, reason: str, output: str) -> None:
        self.reason = reason
        self.output = output
        super().__init__("%s\n%s" % (reason, output))


@dataclass
class Server:
    """A running server, its port, and the output it has produced so far."""

    cmd: list
    port: int
    name: str
    process: subprocess.Popen
    _stdout_file: object = field(repr=False, default=None)
    _stderr_file: object = field(repr=False, default=None)
    _closed: bool = False

    def _read(self, handle) -> str:
        if handle is None:
            return ""
        handle.flush()
        handle.seek(0)
        return _text(handle.read())

    def output(self) -> str:
        """Both streams of the server so far, under one header."""
        return "\n".join(
            [
                "server %s on port %d: %s" % (self.name, self.port, " ".join(self.cmd)),
                "stdout:",
                self._read(self._stdout_file).rstrip("\n"),
                "stderr:",
                self._read(self._stderr_file).rstrip("\n"),
            ]
        )

    def close(self, print_output: bool = True) -> str:
        """Stop the server and print what it wrote. Calling it twice is safe."""
        if self._closed:
            captured = ""
        else:
            if self.process.poll() is None:
                self.process.terminate()
                try:
                    self.process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    self.process.kill()
                    self.process.wait(timeout=5)
            captured = self.output()
            self._closed = True
            if print_output:
                print(captured)
        for handle in (self._stdout_file, self._stderr_file):
            if handle is not None and not handle.closed:
                handle.close()
        return captured

    def __enter__(self) -> "Server":
        return self

    def __exit__(self, exc_type, exc, tb) -> bool:
        self.close()
        return False


def serve(
    cmd,
    port: int,
    readiness,
    cwd=None,
    env=None,
    name: str = None,
    timeout_s: float = 30.0,
    poll_s: float = 0.1,
) -> Server:
    """Start a server on `port` and return a handle once it answers.

    `readiness` is called as `readiness(port)` and is truthy when the server
    answers. For the reference that probe is `POST /graphql {"query":"{__typename}"}`
    until the response is ok, and the console line is asserted rather than
    parsed for the port.

    The port is always explicit and never zero, because the serving plugin
    computes `options.port ?? (Number(process.env.PORT) || 3000)`, so zero binds
    3000 and two parallel runs collide.
    """
    argv = _argv(cmd)
    if not port:
        raise ValueError("the port is explicit and never 0")

    stdout_file = tempfile.TemporaryFile()
    stderr_file = tempfile.TemporaryFile()
    process = subprocess.Popen(
        argv,
        cwd=str(cwd) if cwd else None,
        env=_environment(env, argv),
        stdout=stdout_file,
        stderr=stderr_file,
    )
    server = Server(
        cmd=argv,
        port=port,
        name=name or Path(argv[0]).name,
        process=process,
        _stdout_file=stdout_file,
        _stderr_file=stderr_file,
    )

    deadline = time.monotonic() + timeout_s
    while time.monotonic() < deadline:
        code = process.poll()
        if code is not None:
            output = server.output()
            server.close(print_output=False)
            raise ServerFailed("the server exited with code %s before it was ready" % code, output)
        try:
            if readiness(port):
                return server
        except Exception:
            pass
        time.sleep(poll_s)

    output = server.output()
    server.close(print_output=False)
    raise ServerFailed("no readiness within %ss on port %d" % (timeout_s, port), output)
