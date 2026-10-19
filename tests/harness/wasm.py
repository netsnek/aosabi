# SPDX-License-Identifier: MIT
"""Aosabi harness: the size of the wasm module of aoviewer and the driver of
the seam under the committed module.

The raw bytes and the gzip bytes of the module are read against their gates,
and the same call of the seam runs natively and under the module.

The seam is `aosabi-devtool viewer`, whose operations answer the batch form,
one JSON request line in and one JSON answer line out, and whose flag
`--wasm PATH` answers through the exports of the module in place of the linked
core, so one request file judges the shipped bytes.
"""

from __future__ import annotations

import base64
import hashlib
import http.client
import json
import os
import statistics
import zlib
from dataclasses import dataclass
from pathlib import Path

from .procs import run

# The committed module of aoviewer.
MODULE = "crates/aoviewer-page/assets/aoviewer.wasm"

# The gates of the module, 200 kB raw and 96 kB gzip. A kB is 1000 bytes, the
# unit the same table uses where it writes the 4572 and 4556 bytes of the two
# faces as 9.1 kB.
RAW_GATE_BYTES = 200 * 1000
GZIP_GATE_BYTES = 96 * 1000

GZIP_LEVEL = 9

# The largest window, 15 bits, plus 16, which asks zlib for the gzip container
# in place of its own.
GZIP_WINDOW_BITS = 15 + 16

# The first eight bytes of a module: the magic `\0asm` and version 1.
WASM_HEADER = b"\x00asm\x01\x00\x00\x00"

# The answer lines one run of the seam may return, the cap of the read.
MAX_ANSWERS = 1_000_000

TIMEOUT_S = 1800.0


@dataclass(frozen=True)
class Size:
    """The raw and the gzip bytes of one module and its digest."""

    raw: int
    gzip: int
    sha256: str
    zlib_version: str

    def within_gates(self) -> bool:
        """Reports whether both sizes are inside their gates."""
        return self.raw <= RAW_GATE_BYTES and self.gzip <= GZIP_GATE_BYTES

    def line(self) -> str:
        """The line a notebook prints beside the two gates."""
        return "%d bytes raw (gate %d), %d bytes gzip at level %d (gate %d), zlib %s, sha256 %s" % (
            self.raw,
            RAW_GATE_BYTES,
            self.gzip,
            GZIP_LEVEL,
            GZIP_GATE_BYTES,
            self.zlib_version,
            self.sha256,
        )


def gzip_bytes(data: bytes, level: int = GZIP_LEVEL) -> bytes:
    """Returns `data` in the gzip container at `level` through Python's zlib.

    The window bits `GZIP_WINDOW_BITS` select the gzip header, which zlib
    writes with no file name and a zero time, so the header adds no variable
    bytes and, on one zlib build, the length depends on the input and the
    level. The build is a third input: the committed
    tests/fixtures/viewer/presets/github.json of 863,156 bytes at level 9
    gives 37,143 bytes through Python's zlib 1.3.1.zlib-ng and 36,944 bytes
    through the zlib 1.3.1-470d3a2 of node v24.13.0, which is why `Size.line`
    prints the version beside the gate.
    """
    compressor = zlib.compressobj(level, zlib.DEFLATED, GZIP_WINDOW_BITS)
    return compressor.compress(data) + compressor.flush()


def size(module) -> Size:
    """Reads the module at `module` and returns its sizes.

    Raises `ValueError` when the file does not open with the header of a
    WebAssembly module of version 1.
    """
    data = Path(module).read_bytes()
    if not data.startswith(WASM_HEADER):
        raise ValueError("%s is not a WebAssembly module of version 1" % Path(module).name)
    return Size(
        raw=len(data),
        gzip=len(gzip_bytes(data)),
        sha256=hashlib.sha256(data).hexdigest(),
        zlib_version=zlib.ZLIB_RUNTIME_VERSION,
    )


def request_lines(requests) -> bytes:
    """Returns the request file of `requests`, one JSON object per line.

    A request without an `id` takes its place in the list as its id, because
    the batch form refuses a request that carries none. Every line is ASCII,
    because the encoder escapes a character outside ASCII and a lone
    surrogate as `\\uXXXX`, which keeps a string the schema carries intact on
    its way to the seam.
    """
    lines = []
    for index, request in enumerate(requests):
        body = dict(request) if "id" in request else {"id": index, **request}
        lines.append(json.dumps(body, ensure_ascii=True, separators=(",", ":")))
    return ("\n".join(lines) + "\n").encode("ascii")


def drive(
    devtool,
    requests,
    work_dir,
    module=None,
    name: str = "requests",
    timeout_s: float = TIMEOUT_S,
) -> list:
    """Runs the `viewer` seam over `requests` and returns the parsed answers.

    The request file is written under `work_dir`, a directory under `target/`,
    and `module`, when given, is passed as `--wasm`, so the same requests run
    once natively and once under the module. The answers are returned in the
    order of the requests, each the object of one answer line.
    """
    work_dir = Path(work_dir)
    work_dir.mkdir(parents=True, exist_ok=True)
    path = work_dir / ("%s.ndjson" % name)
    path.write_bytes(request_lines(requests))
    argv = [str(devtool), "viewer", "--requests", str(path)]
    if module is not None:
        argv += ["--wasm", str(module)]
    result = run(argv, timeout_s=timeout_s)
    answers = []
    for line in result.stdout.split(b"\n"):
        if not line.strip():
            continue
        if len(answers) >= MAX_ANSWERS:
            raise ValueError("more than %d answer lines" % MAX_ANSWERS)
        answers.append(json.loads(line))
    return answers


def raw_answers(devtool, request_file, module=None, timeout_s: float = TIMEOUT_S) -> bytes:
    """Runs the seam over a request file and returns its stdout as bytes.

    This is the form the byte comparison of native against the module reads,
    because a parse would hide a difference of formatting.
    """
    argv = [str(devtool), "viewer", "--requests", str(request_file)]
    if module is not None:
        argv += ["--wasm", str(module)]
    return run(argv, timeout_s=timeout_s).stdout


def compare(native: list, module: list) -> list:
    """Returns the ids of the answers that differ between the two runs.

    An answer present on one side alone counts as differing.
    """
    left = {json.dumps(a.get("id")): a for a in native}
    right = {json.dumps(a.get("id")): a for a in module}
    differing = []
    for key in sorted(set(left) | set(right)):
        if left.get(key) != right.get(key):
            differing.append(json.loads(key))
    return differing


def smoke(devtool) -> dict:
    """Returns the interface line of the `viewer` primitive under `--smoke`."""
    return json.loads(run([str(devtool), "viewer", "--smoke"], timeout_s=60).stdout)


# The memory and the size of a workerd guest.
#
# The functions above stay as they are, and procs.py is untouched.

STATUS_FIELDS = ("VmRSS", "VmHWM")
STATUS_UNIT = 1024

# The path the gate entry of a guest answers with the memory report of the
# wasm modules it instantiated.
REPORT_PATH = "/.ts1-gate/memory"


@dataclass(frozen=True)
class RawGzip:
    """The raw bytes of one file and its bytes under gzip at `GZIP_LEVEL`."""

    raw: int
    gzip: int

    def __add__(self, other: "RawGzip") -> "RawGzip":
        return RawGzip(raw=self.raw + other.raw, gzip=self.gzip + other.gzip)


def raw_and_gzip(source) -> RawGzip:
    """Returns the raw size and the gzip size of `source` as two named fields.

    `source` is bytes or a path. The gzip size is the length of
    `gzip_bytes` at level 9, so the zlib build of this process decides it,
    and `zlib.ZLIB_RUNTIME_VERSION` names that build beside the number.
    """
    data = source if isinstance(source, (bytes, bytearray)) else Path(source).read_bytes()
    return RawGzip(raw=len(data), gzip=len(gzip_bytes(bytes(data))))


@dataclass(frozen=True)
class Memory:
    """The resident set of one process and its high water mark, in bytes."""

    vm_rss: int
    vm_hwm: int

    def __sub__(self, other: "Memory") -> "Memory":
        return Memory(vm_rss=self.vm_rss - other.vm_rss, vm_hwm=self.vm_hwm - other.vm_hwm)


def process_memory(pid: int) -> Memory:
    """Reads VmRSS and VmHWM of the process `pid` from /proc/<pid>/status.

    Raises `ValueError` when a field is missing, which is the case for a
    process that has exited and left a zombie.
    """
    found = {}
    for line in Path("/proc/%d/status" % pid).read_text(encoding="ascii").splitlines():
        name, _, value = line.partition(":")
        if name in STATUS_FIELDS:
            number, unit = value.split()
            if unit != "kB":
                raise ValueError("%s of process %d is in %s, not kB" % (name, pid, unit))
            found[name] = int(number) * STATUS_UNIT
    missing = [name for name in STATUS_FIELDS if name not in found]
    if missing:
        raise ValueError("process %d reports no %s" % (pid, ", ".join(missing)))
    return Memory(vm_rss=found["VmRSS"], vm_hwm=found["VmHWM"])


def median_memory(rounds) -> Memory:
    """Returns the median of each field over the readings of `rounds`.

    The gate takes three rounds, an odd count, so each median is one of the
    readings and no mean of two.
    """
    rounds = list(rounds)
    if not rounds:
        raise ValueError("no reading to take the median of")
    return Memory(
        vm_rss=int(statistics.median(m.vm_rss for m in rounds)),
        vm_hwm=int(statistics.median(m.vm_hwm for m in rounds)),
    )


def buffer_byte_lengths(port: int, host: str = "127.0.0.1", timeout_s: float = 10.0) -> list:
    """Returns `memory.buffer.byteLength` of each wasm instance of a guest.

    The guest reports it itself: its gate entry answers `GET REPORT_PATH`
    with `{"modules":[...]}`, one length in bytes per instance it made, in
    the order it made them, and an empty list for a guest without wasm.
    """
    connection = http.client.HTTPConnection(host, port, timeout=timeout_s)
    try:
        connection.request("GET", REPORT_PATH, headers={"accept-encoding": "identity"})
        answer = connection.getresponse()
        body = answer.read()
    finally:
        connection.close()
    if answer.status != 200:
        raise ValueError("the guest on port %d answers %d to %s" % (port, answer.status, REPORT_PATH))
    modules = json.loads(body)["modules"]
    if not all(isinstance(length, int) and length >= 0 for length in modules):
        raise ValueError("the guest on port %d reports %r" % (port, modules))
    return modules


# The sequential latency of a served side.
#
# The client runs as a process of its own, so the timing of the kernel of the
# notebook adds nothing.

# The marker of a DISTINCT body, where the client writes its counter.
DISTINCT_MARK = "@@@@@@@"


@dataclass(frozen=True)
class Latency:
    """The sequential latency of one side for one body, in microseconds."""

    p50_us: float
    p90_us: float
    p99_us: float
    count: int
    statuses: dict
    differ: int
    status: int
    body: bytes


def sequential(client, port: int, body: str, requests: int, warm: int, base: int = 1_000_000,
               timeout_s: float = 900.0) -> Latency:
    """Times `requests` sequential POSTs of `body` to /graphql on `port` after `warm` untimed ones.

    `client` is the path of load.mjs. A body that holds `DISTINCT_MARK`
    becomes a new document for every request, numbered from `base`. The
    answer of the first request comes back as its status and its body, and
    `differ` counts the later answers whose status or body is another.
    """
    spec = {"mode": "seq", "port": port, "body": body, "base": base, "n": requests, "warm": warm}
    got = run(["node", str(client), json.dumps(spec)], timeout_s=timeout_s)
    out = json.loads(got.stdout.strip().splitlines()[-1])
    first = out["first"]
    return Latency(p50_us=out["p50_us"], p90_us=out["p90_us"], p99_us=out["p99_us"], count=out["count"],
                   statuses=out["statuses"], differ=out["differ"], status=first["status"],
                   body=base64.b64decode(first["body"]))


def load_average() -> tuple:
    """Returns the load average of the machine over 1, 5 and 15 minutes."""
    return os.getloadavg()


def median_of(values) -> float:
    """Returns the median of `values`, one of them for an odd count."""
    values = list(values)
    if not values:
        raise ValueError("no value to take the median of")
    return statistics.median(values)
