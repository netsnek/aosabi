# SPDX-License-Identifier: MIT
"""Aosabi harness: a client of the protocol server over a child's standard input and output.

The `mcp` command of the reference and `aosabi mcp` speak JSON-RPC 2.0 over the standard input and
output of the process, one JSON message per line. `McpClient` starts one of
them in a session of its own, writes one request per line and reads the next
line the server writes as its answer, with the bytes kept as they came,
because `70_mcp` compares every answer as bytes with its golden and with the
live reference.

A request is written as `json.dumps` with no space, in the key order the
capture of the goldens wrote it, so the bytes a notebook sends equal the
`request.bin` of a golden. The ids count from 1 per client, and a notification
takes none. `raw` writes a line as it is given, such as a line that is no
JSON, which the server consumes and answers with nothing. `sent` and `read`
hold the whole transcript of the session, which `70_mcp` compares with capture
row C75. `close` closes the input, reads what the server still writes, and
ends the process group through `os.killpg`, so no child outlives a notebook.
"""

from __future__ import annotations

import json
import os
import select
import signal
import subprocess
import time

from .paths import paths
from .procs import _environment

#: The protocol version the client of 70_mcp asks for.
PROTOCOL_VERSION = "2025-06-18"

#: The client the `initialize` request names.
CLIENT_INFO = {"name": "aosabi-harness", "version": "0.0.0"}


class McpClient:
    """One protocol session over a child's standard input and output, one line per message."""

    def __init__(self, cmd, cwd=None, env=None, name: str = "mcp", timeout_s: float = 180.0, root=None) -> None:
        logs = paths(root).root / "target" / "harness" / "mcp"
        logs.mkdir(parents=True, exist_ok=True)
        self.name = name
        self.timeout_s = timeout_s
        self.err_path = logs / (name + ".err")
        self.process = subprocess.Popen(
            [str(part) for part in cmd], cwd=str(cwd) if cwd else None, env=_environment(env),
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.err_path.open("wb"),
            start_new_session=True,
        )
        self.fd = self.process.stdout.fileno()
        self.pending = b""
        self.sent = b""
        self.read = b""
        self.next_id = 1
        self.code = None

    def send(self, message) -> bytes:
        """Writes `message`, a JSON value or the bytes of one line, and returns the line written."""
        if isinstance(message, bytes):
            line = message
        else:
            line = json.dumps(message, separators=(",", ":")).encode("utf-8")
        line += b"\n"
        self.process.stdin.write(line)
        self.process.stdin.flush()
        self.sent += line
        return line

    def answer(self, timeout_s: float = None) -> bytes | None:
        """The next line the server writes with its newline, or `None` when none comes in time."""
        deadline = time.monotonic() + (self.timeout_s if timeout_s is None else timeout_s)
        while b"\n" not in self.pending:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                return None
            ready, _, _ = select.select([self.fd], [], [], remaining)
            if not ready:
                return None
            chunk = os.read(self.fd, 65536)
            if not chunk:
                return None
            self.pending += chunk
        line, _, self.pending = self.pending.partition(b"\n")
        self.read += line + b"\n"
        return line + b"\n"

    def request(self, method: str, params=None, request_id=None) -> bytes | None:
        """Sends one request with the next id, or with `request_id`, and returns its answer."""
        if request_id is None:
            request_id = self.next_id
        self.next_id = max(self.next_id, request_id) + 1
        message = {"jsonrpc": "2.0", "id": request_id, "method": method}
        if params is not None:
            message["params"] = params
        self.send(message)
        return self.answer()

    def notify(self, method: str, params=None) -> None:
        """Sends one notification, which takes no id and no answer."""
        message = {"jsonrpc": "2.0", "method": method}
        if params is not None:
            message["params"] = params
        self.send(message)

    def initialize(self, version: str = PROTOCOL_VERSION, notify: bool = True) -> bytes | None:
        """Sends `initialize` asking for `version`, then `notifications/initialized` when `notify`."""
        answer = self.request("initialize", {"protocolVersion": version, "capabilities": {},
                                             "clientInfo": dict(CLIENT_INFO)})
        if notify:
            self.notify("notifications/initialized")
        return answer

    def list_tools(self) -> bytes | None:
        """Sends `tools/list` and returns its answer."""
        return self.request("tools/list")

    def call(self, name: str, arguments=None) -> bytes | None:
        """Sends `tools/call` of the tool `name` with `arguments` and returns its answer."""
        params = {"name": name}
        if arguments is not None:
            params["arguments"] = arguments
        return self.request("tools/call", params)

    def raw(self, line: bytes, wait_s: float = 2.0) -> bytes | None:
        """Writes `line` as it is and returns what the server answers within `wait_s`, or `None`."""
        self.send(line.rstrip(b"\n"))
        return self.answer(timeout_s=wait_s)

    def stderr(self) -> bytes:
        """What the server wrote to its standard error so far."""
        return self.err_path.read_bytes() if self.err_path.exists() else b""

    def close(self, timeout_s: float = 30.0) -> int:
        """Closes the input, reads what the server still writes and ends its process group."""
        if self.code is not None:
            return self.code
        try:
            self.process.stdin.close()
        except OSError:
            pass
        deadline = time.monotonic() + timeout_s
        while time.monotonic() < deadline:
            ready, _, _ = select.select([self.fd], [], [], max(deadline - time.monotonic(), 0))
            chunk = os.read(self.fd, 65536) if ready else b""
            if not chunk:
                break
            self.pending += chunk
        self.read += self.pending
        self.pending = b""
        try:
            self.code = self.process.wait(timeout=timeout_s)
        except subprocess.TimeoutExpired:
            self.code = None
        for sig in (signal.SIGTERM, signal.SIGKILL):
            try:
                os.killpg(self.process.pid, sig)
            except ProcessLookupError:
                break
        if self.code is None:
            self.code = self.process.wait(timeout=timeout_s)
        self.process.stdout.close()
        return self.code

    def __enter__(self) -> "McpClient":
        return self

    def __exit__(self, *_) -> bool:
        self.close()
        return False

