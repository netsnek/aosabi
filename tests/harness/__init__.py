# SPDX-License-Identifier: MIT
"""Aosabi harness: the Python helpers every notebook imports.

Every test case in this project is a notebook executed by papermill, there is no
test in Rust, and a notebook drives the Rust binaries and node through
subprocesses and compares their bytes. This package holds what all of those
notebooks share, so that one comparison, one process helper and one golden layout
serve every family.

A notebook imports it with the repository tests directory on the path:

    import sys
    from pathlib import Path

    for _base in [Path.cwd(), *Path.cwd().parents]:
        if (_base / "tests" / "harness" / "__init__.py").is_file():
            sys.path.insert(0, str(_base / "tests"))
            break

    from harness import bytediff, golden_read, golden_write, paths, run, serve

tests/run.py also exports the tests directory on PYTHONPATH, so under the
runner the loop above finds the package on the first try.

The seven helpers of the documentation notebooks are modules of their own,
reached as `from harness import docs, svg, pdf, wasm, arch, estate, fetch`.

A notebook imports the last two by name.

`harness.probelock` holds the lock of a scratch workspace that a notebook
writes with `cargo generate-lockfile --offline` to the versions of the locks
of the tree, so a package version that only the shared cargo cache holds
reaches no probe.

Three modules for the notebooks of the 70 band are reached as
`from harness import cli, mcp, devloop`: `cli`, the runners of both command
lines, the parsers of both registries of a pull and the hostile server, `mcp`,
the client of the protocol server over a child's standard input and output, and
`devloop`, one development loop with its edits, probes and rebuilds.

Two modules a federation notebook imports are `harness.recording_proxy`, the
proxy in front of one remote that records every outbound request and answers in
the modes of the judging notebook, and `harness.remote_stub`, the remote that
answers the delegation corpus with canned answers.

`harness.toolchain` holds the one flag set of a shipped build of the standard
library from source on the pin, `RUSTC_BOOTSTRAP=1`, the `-Z` flags and the
features of the standard library per card and the remap of its source, with the
private copy of the pin that tools/size_toolchain.py makes, and the remap every
build of a twin takes through `fixtures.build_env`. Every call of the command
line of a notebook at the switch AOSABI_STD at source takes the private copy of
the pin as its toolchain, through `cli_toolchain` of `harness.toolchain` in
`run` and `serve`, since the command line refuses the shared pin with no
rust-src for now.

One module is reached as `from harness import divergences`, the rewrites of the
named divergences that change the bytes of an answer of the side under test by a
fixed rule, which a comparison asserts and then reads as the reference writes
them.
"""

from . import (
    arch,
    cli,
    devloop,
    divergences,
    docs,
    estate,
    fetch,
    mcp,
    pdf,
    probelock,
    push_model,
    recording_proxy,
    remote_stub,
    size,
    sse,
    svg,
    toolchain,
    wasm,
)
from .boot import Boot, boot_reference, boot_twin, reference_report
from .callers import CALLERS, callers_of, with_caller
from .diffs import SENTINEL, ByteDiff, bytediff, first_difference, hexdump
from .goldens import (
    CASE_FILES,
    GoldenMissing,
    GoldenRefused,
    golden_case_dir,
    golden_path,
    golden_read,
    golden_write,
    recorded_by_the_command_line,
)
from .http import Answer, Endpoint, exchange, head_report, send, send_case, tcp, unix
from .masks import MASKS, OIDC_MASKS, Mask
from .mock_issuer import MockIssuer
from .paths import Paths, RootNotFound, find_root, paths, socket_dir
from .procs import (
    NODE_OPTIONS,
    TELEMETRY_OFF,
    CommandFailed,
    CommandResult,
    Server,
    ServerFailed,
    lock_dir,
    run,
    serve,
)
from .recording_proxy import Exchange, RecordingProxy, hostile_responses
from .remote_stub import Canned, Recorded, RemoteStub
from .sse import PRELUDE, TERMINATOR, Stream, StreamReader, read_stream

__all__ = [
    "Answer",
    "Boot",
    "ByteDiff",
    "CALLERS",
    "CASE_FILES",
    "Canned",
    "CommandFailed",
    "CommandResult",
    "Endpoint",
    "Exchange",
    "GoldenMissing",
    "GoldenRefused",
    "MASKS",
    "Mask",
    "MockIssuer",
    "NODE_OPTIONS",
    "OIDC_MASKS",
    "PRELUDE",
    "Paths",
    "Recorded",
    "RecordingProxy",
    "RemoteStub",
    "RootNotFound",
    "SENTINEL",
    "Server",
    "ServerFailed",
    "Stream",
    "StreamReader",
    "TELEMETRY_OFF",
    "TERMINATOR",
    "arch",
    "boot_reference",
    "boot_twin",
    "bytediff",
    "callers_of",
    "cli",
    "devloop",
    "docs",
    "estate",
    "exchange",
    "fetch",
    "find_root",
    "first_difference",
    "golden_case_dir",
    "golden_path",
    "golden_read",
    "golden_write",
    "head_report",
    "hexdump",
    "hostile_responses",
    "lock_dir",
    "mcp",
    "paths",
    "pdf",
    "probelock",
    "push_model",
    "read_stream",
    "recording_proxy",
    "reference_report",
    "remote_stub",
    "run",
    "send",
    "send_case",
    "serve",
    "size",
    "socket_dir",
    "sse",
    "svg",
    "tcp",
    "toolchain",
    "unix",
    "wasm",
    "with_caller",
]
