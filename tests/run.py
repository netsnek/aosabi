# SPDX-License-Identifier: MIT
"""Aosabi harness: the runner that sweeps the notebook matrix.

The whole matrix runs through this file, which uses papermill over nbclient for
kernel control and per cell timeouts, writes one executed notebook per matrix
entry into tests/notebooks/out/ and a report.json that carries, per failure, the
first differing byte offset rendered as a hexdump of both sides.

    python tests/run.py --matrix tests/matrix.yaml

With --jobs N above 1 the runner runs up to N entries at once, each in a
process and a kernel of its own, and the report and the executed copies stay
one per entry. Entries of one notebook take turns, because they share the
notebook's out directory. An entry marked exclusive in the matrix runs alone
after the others. The run takes up to N slots of 1 to MAX_JOBS that no other
run on the machine holds, each job runs in one of them, and a notebook that
declares port_offset in its parameters cell gets PORT_STRIDE times its slot,
so no two entries that run together, in this run or in another, bind a
common port. With --jobs 1, the default, the runner sweeps the matrix in
order in this process and injects no parameter of its own.

    python tests/run.py --matrix tests/matrix.yaml --jobs 4

The exit code is non zero when any notebook failed, which is what the diff
workflow of docs/harness.md section 11 reads.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

# The harness package sits beside this file. Bytecode stays out of the tree,
# because a __pycache__ directory under tests/ is untracked output of a run and
# the repository carries no ignore rule for it.
sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parent))

from harness.diffs import SENTINEL  # noqa: E402
from harness.paths import find_root  # noqa: E402
from harness.procs import NODE_OPTIONS, TELEMETRY_OFF  # noqa: E402

REPORT_SCHEMA = "aosabi-harness-report-1"
DEFAULT_KERNEL = "aosabi"
DEFAULT_CELL_TIMEOUT = 600
DEFAULT_START_TIMEOUT = 120

#: The distance between the ports of two job slots. A notebook adds its
#: parameter port_offset to every fixed port it binds or connects to, and under
#: --jobs above 1 the runner passes this distance times the slot. Every fixed
#: port of a notebook lies in 4000 to 5999, so slot k owns 4000 plus k times
#: this distance up to 1999 above it, and no two slots and no run under --jobs 1
#: share a port.
PORT_STRIDE = 2000

#: The most entries at once. The highest port of the last slot, 5999 plus 12
#: times PORT_STRIDE, is 29999, below 32768, where the ephemeral ports of Linux
#: begin.
MAX_JOBS = 12

#: The parameter a notebook declares to take the port offset of its slot.
PORT_PARAMETER = "port_offset"

#: The ports the kernel of a job binds, the top of its slot's ports, from these
#: two plus PORT_STRIDE times the slot. Left to itself jupyter_client lets the
#: system pick the kernel's ports by binding port 0 and closing it, and the
#: kernel binds them only once its process runs, so a sibling job that starts a
#: kernel or a client in between can take one, and the kernel dies before it
#: answers kernel_info. The system picks no port below 32768, no other slot
#: reaches these, and the plans name ports up to 5789 and none of 5980 to 5999.
KERNEL_PORT_FIRST = 5980
KERNEL_PORT_LAST = 5999

#: The ports of a kernel: shell, iopub, stdin, control and heartbeat.
KERNEL_PORTS = 5

#: The directory of the locks of the slots, one file a slot, under
#: $XDG_RUNTIME_DIR, which every session of the user on the machine shares. A
#: run with jobs holds the lock of each slot it uses for as long as it runs,
#: so two runs on one machine, of one checkout or of two, never use one slot,
#: and no two of their entries bind one port.
SLOT_LOCKS = "aosabi-harness-slots"

ANSI = re.compile(r"\x1b\[[0-9;]*[A-Za-z]")


def strip_ansi(text: str) -> str:
    """Tracebacks arrive coloured from the kernel and report.json holds text."""
    return ANSI.sub("", text)


def relative(root: Path, path: Path) -> str:
    """A path as the report writes it, repository relative where it can be.

    A path outside the repository stays as it is, which happens when a run
    points at a scratch matrix, and a scratch path never enters a committed
    file.
    """
    try:
        return str(Path(path).relative_to(root))
    except ValueError:
        return str(path)


def load_matrix(path: Path) -> list:
    """Read the matrix file and return its entries.

    The file is a list of entries, each with a notebook and its parameters. A
    mapping with an `entries` key and an optional `defaults` mapping merged
    under every entry's parameters is accepted as well, because one sweep over
    many fixtures repeats the same pins.

    An entry with `sweep: false` stays out of a run unless `--only` names its
    id whole, which keeps an entry that needs the network or the owner's
    credentials out of the closing sweep and still runs it on demand. An entry
    with `exclusive: true` runs alone under --jobs above 1, and the key changes
    nothing under --jobs 1.
    """
    import yaml

    document = yaml.safe_load(path.read_text(encoding="utf-8"))
    if document is None:
        return []
    defaults = {}
    if isinstance(document, dict):
        defaults = document.get("defaults") or {}
        document = document.get("entries") or document.get("notebooks") or []
    if not isinstance(document, list):
        raise ValueError("%s holds neither a list of entries nor an entries key" % path)

    entries = []
    seen = {}
    for position, raw in enumerate(document):
        if not isinstance(raw, dict) or "notebook" not in raw:
            raise ValueError("entry %d of %s carries no notebook key" % (position, path))
        parameters = dict(defaults)
        parameters.update(raw.get("parameters") or {})
        name = raw.get("id") or Path(raw["notebook"]).stem
        seen[name] = seen.get(name, 0) + 1
        if seen[name] > 1:
            name = "%s-%d" % (name, seen[name])
        entries.append(
            {
                "id": name,
                "notebook": raw["notebook"],
                "parameters": parameters,
                "cell_timeout": raw.get("cell_timeout"),
                "skip": bool(raw.get("skip")),
                "sweep": raw.get("sweep", True) is not False,
                "exclusive": bool(raw.get("exclusive")),
            }
        )
    return entries


def resolve_notebook(root: Path, notebook: str) -> Path:
    """A matrix entry names a repository relative path or a bare notebook name."""
    candidate = Path(notebook)
    if candidate.is_absolute():
        return candidate
    direct = root / candidate
    if direct.is_file():
        return direct
    if candidate.suffix != ".ipynb":
        candidate = candidate.with_suffix(".ipynb")
    return root / "tests" / "notebooks" / candidate.name


def first_error(notebook_path: Path) -> dict:
    """The first error output of an executed notebook, as plain data.

    The kernel runs in another process, so the exception object never reaches
    this one and the failure is read back as the text the kernel produced. The
    cell index counts the cells of the executed copy, which is the file a
    reader opens, and papermill adds its injected parameters cell and its error
    markers to that copy, so the number is larger than the index in the source
    notebook.
    """
    import nbformat

    try:
        executed = nbformat.read(str(notebook_path), as_version=4)
    except (OSError, ValueError):
        return None
    for index, cell in enumerate(executed.cells):
        for output in cell.get("outputs") or []:
            if output.get("output_type") != "error":
                continue
            traceback = [strip_ansi(line) for line in output.get("traceback") or []]
            failure = {
                "cell_index": index,
                "ename": output.get("ename", ""),
                "evalue": strip_ansi(output.get("evalue", "")),
                "traceback": traceback,
            }
            payload = parse_bytediff(failure["evalue"]) or parse_bytediff(
                "\n".join(traceback)
            )
            if payload:
                failure["bytediff"] = payload
            return failure
    return None


def parse_bytediff(text: str) -> dict:
    """Recover the ByteDiff fields from the message the kernel printed."""
    for line in text.splitlines():
        marker = line.find(SENTINEL)
        if marker < 0:
            continue
        try:
            return json.loads(line[marker + len(SENTINEL) :])
        except json.JSONDecodeError:
            return None
    return None


def kernel_environment(root: Path) -> None:
    """Pin the environment every kernel and every process below it inherits."""
    tests = str(root / "tests")
    parts = [part for part in os.environ.get("PYTHONPATH", "").split(os.pathsep) if part]
    if tests not in parts:
        os.environ["PYTHONPATH"] = os.pathsep.join([tests, *parts])
    os.environ["PYTHONDONTWRITEBYTECODE"] = "1"
    os.environ.update(TELEMETRY_OFF)
    node_options = os.environ.get("NODE_OPTIONS", "")
    if NODE_OPTIONS not in node_options:
        os.environ["NODE_OPTIONS"] = (node_options + " " + NODE_OPTIONS).strip()


def execute(entry: dict, root: Path, out_dir: Path, args, engine=None) -> dict:
    """Run one matrix entry and return its report row.

    `engine` holds further arguments of the papermill engine, which a job of
    --jobs above 1 passes, and --jobs 1 passes none.
    """
    import papermill
    from papermill.exceptions import PapermillExecutionError

    source = resolve_notebook(root, entry["notebook"])
    target = out_dir / ("%s.ipynb" % entry["id"])
    row = {
        "id": entry["id"],
        "notebook": relative(root, source),
        "output": relative(root, target),
        "parameters": entry["parameters"],
        "status": "skipped",
        "duration_s": 0.0,
    }
    if entry["skip"]:
        return row
    if not source.is_file():
        row["status"] = "error"
        row["failure"] = {"ename": "FileNotFoundError", "evalue": "no notebook at %s" % row["notebook"]}
        return row

    started = time.monotonic()
    row["status"] = "passed"
    try:
        papermill.execute_notebook(
            str(source),
            str(target),
            parameters=entry["parameters"],
            kernel_name=args.kernel,
            cwd=str(root),
            progress_bar=False,
            log_output=args.log_output,
            start_timeout=args.start_timeout,
            execution_timeout=entry["cell_timeout"] or args.cell_timeout,
            request_save_on_cell_execute=True,
            **(engine or {}),
        )
    except PapermillExecutionError as failed:
        row["status"] = "failed"
        row["failure"] = {
            "cell_index": failed.cell_index,
            "ename": failed.ename,
            "evalue": strip_ansi(failed.evalue or ""),
            "traceback": [strip_ansi(line) for line in failed.traceback or []],
        }
        payload = parse_bytediff(row["failure"]["evalue"])
        if payload:
            row["failure"]["bytediff"] = payload
    except Exception as broken:  # the kernel never started, or papermill refused
        row["status"] = "error"
        row["failure"] = {
            "ename": type(broken).__name__,
            "evalue": strip_ansi(str(broken)),
            "traceback": [],
        }
    row["duration_s"] = round(time.monotonic() - started, 3)

    # The executed copy is the source of truth for a failure, because it holds
    # the cell index and the traceback even when papermill raised something else.
    found = first_error(target)
    if found:
        if row["status"] == "passed":
            row["status"] = "failed"
        failure = dict(row.get("failure") or {})
        failure.update(found)
        row["failure"] = failure
    return row


def print_row(row: dict) -> None:
    line = "%-8s %-28s %7.2fs" % (row["status"], row["id"], row["duration_s"])
    print(line, flush=True)
    failure = row.get("failure")
    if not failure:
        return
    print("    cell %s raised %s" % (failure.get("cell_index", "?"), failure.get("ename", "?")))
    first_line = (failure.get("evalue") or "").splitlines()
    if first_line:
        print("    %s" % first_line[0])
    diff = failure.get("bytediff")
    if diff:
        print("    byte difference at offset %d, %d against %d bytes"
              % (diff["offset"], diff["left_len"], diff["right_len"]))


def declares_port_offset(source: Path) -> bool:
    """Whether the parameters cell of a notebook assigns the port offset."""
    try:
        document = json.loads(source.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return False
    assignment = re.compile(r"^%s\s*=" % PORT_PARAMETER, re.M)
    for cell in document.get("cells") or []:
        if "parameters" not in (cell.get("metadata") or {}).get("tags", []):
            continue
        text = cell.get("source") or ""
        if isinstance(text, list):
            text = "".join(text)
        return bool(assignment.search(text))
    return False


def slot_kernel_manager(slot: int):
    """Return the kernel manager class of a job in `slot`.

    Its kernel binds five free ports of KERNEL_PORT_FIRST to KERNEL_PORT_LAST
    shifted by PORT_STRIDE times the slot, never ports the system picks. A
    port counts as free when a socket with SO_REUSEADDR binds it, as the
    kernel's ZMQ sockets do, so the connections a former kernel of the slot
    left in TIME_WAIT keep no port, and a listener does.
    """
    import socket

    from jupyter_client import AsyncKernelManager

    first = KERNEL_PORT_FIRST + PORT_STRIDE * slot
    last = KERNEL_PORT_LAST + PORT_STRIDE * slot

    def free(ip: str, port: int) -> bool:
        with socket.socket() as probe:
            probe.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            try:
                probe.bind((ip, port))
            except OSError:
                return False
        return True

    class SlotKernelManager(AsyncKernelManager):
        def __init__(self, **kwargs):
            super().__init__(**kwargs)
            ports = [port for port in range(first, last + 1) if free(self.ip, port)][:KERNEL_PORTS]
            if len(ports) < KERNEL_PORTS:
                raise RuntimeError("slot %d has %d free kernel ports in %d to %d and needs %d"
                                   % (slot, len(ports), first, last, KERNEL_PORTS))
            self.cache_ports = False
            self.shell_port, self.iopub_port, self.stdin_port, self.control_port, self.hb_port = ports

    return SlotKernelManager


def slot_lock_dir() -> Path:
    """The directory of the locks of the slots, the same for every run of
    the user on this machine."""
    runtime = os.environ.get("XDG_RUNTIME_DIR")
    if runtime:
        return Path(runtime) / SLOT_LOCKS
    return Path("/tmp") / ("%s-%d" % (SLOT_LOCKS, os.getuid()))


def hold_slots(count: int) -> list:
    """Take the locks of up to `count` slots of 1 to MAX_JOBS that no other
    run on this machine holds and return the slots with the descriptors that
    hold them, lowest first.

    A run that finds every slot held waits until one is free. The lock is a
    flock on the file of the slot, which the system lets go when the run ends
    in any way.
    """
    import fcntl

    directory = slot_lock_dir()
    directory.mkdir(mode=0o700, parents=True, exist_ok=True)
    held = []
    told = False
    while True:
        for slot in range(1, MAX_JOBS + 1):
            if len(held) == count:
                break
            fd = os.open(str(directory / ("slot-%d.lock" % slot)), os.O_RDWR | os.O_CREAT, 0o600)
            try:
                fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError:
                os.close(fd)
                continue
            held.append((slot, fd))
        if held:
            return held
        if not told:
            print("jobs: other runs on this machine hold every slot of 1 to %d, waiting" % MAX_JOBS, flush=True)
            told = True
        time.sleep(1.0)


def log_job_output(path: Path) -> None:
    """Send the output papermill logs in this job process to one file.

    Papermill logs the cell output under --log-output through its logger, and
    the file takes all of it, from the start of each cell to its streams and
    results. The logger stops propagating, so nothing of it reaches the
    terminal the jobs share.
    """
    import logging

    handler = logging.FileHandler(str(path), mode="w", encoding="utf-8")
    handler.setFormatter(logging.Formatter("%(message)s"))
    logger = logging.getLogger("papermill")
    logger.addHandler(handler)
    logger.setLevel(logging.INFO)
    logger.propagate = False


def follow_runner() -> None:
    """End this job process when the runner that started it ends.

    The locks of the slots belong to the runner alone, and the system lets
    them go when the runner dies in any way, SIGKILL and the OOM killer as
    well. A job that ran on would keep its kernel and its ports in a slot the
    next run can take, so Linux sends the job SIGTERM once its parent ends
    (PR_SET_PDEATHSIG of prctl(2)), and a job whose runner ended before the
    call exits at once. Its kernel ends with it, since the kernel watches
    its parent process.
    """
    import ctypes
    import multiprocessing
    import signal

    pr_set_pdeathsig = 1
    try:
        libc = ctypes.CDLL(None, use_errno=True)
        if libc.prctl(pr_set_pdeathsig, int(signal.SIGTERM), 0, 0, 0) != 0:
            return
    except (OSError, AttributeError):
        return
    parent = multiprocessing.parent_process()
    if parent is not None and os.getppid() != parent.pid:
        os._exit(1)


def run_job(entry: dict, slot: int, root: Path, out_dir: Path, args, sender) -> None:
    """The body of a job process: run one entry in `slot` and send its report row back.

    The job ends when its runner ends. The kernel binds the kernel ports of
    the slot. A failure of the runner's own code becomes an error row, so the
    parent always gets a row while the process lives long enough to send
    one. With --log-output the cell output of the entry goes to
    <out>/<id>.log beside its executed copy, because the jobs share one
    terminal and their lines would interleave there.
    """
    follow_runner()
    try:
        if args.log_output:
            log_job_output(out_dir / ("%s.log" % entry["id"]))
        row = execute(entry, root, out_dir, args, {"kernel_manager_class": slot_kernel_manager(slot)})
    except Exception as broken:  # a defect of the runner, reported as a row
        row = {
            "id": entry["id"],
            "notebook": relative(root, resolve_notebook(root, entry["notebook"])),
            "output": relative(root, out_dir / ("%s.ipynb" % entry["id"])),
            "parameters": entry["parameters"],
            "status": "error",
            "duration_s": 0.0,
            "failure": {"ename": type(broken).__name__, "evalue": strip_ansi(str(broken)), "traceback": []},
        }
    sender.send(row)
    sender.close()


def execute_jobs(entries: list, root: Path, out_dir: Path, args, slots: list) -> list:
    """Run the entries with one job a slot of `slots` at once and return their rows in matrix order.

    The entries that are not exclusive start first, in matrix order, as soon
    as a slot is free and no running entry has the same notebook. The
    exclusive entries follow one at a time in the lowest slot, each with
    nothing else running.
    Each job is a process of its own, started with spawn, so no state of one
    kernel or of papermill reaches another, and it sends its row back over a
    pipe. A row is printed when its entry ends.
    """
    import multiprocessing
    from multiprocessing.connection import wait

    context = multiprocessing.get_context("spawn")
    rows = [None] * len(entries)
    declares = {}

    def start(index: int, slot: int) -> dict:
        entry = dict(entries[index])
        source = resolve_notebook(root, entry["notebook"])
        if source not in declares:
            declares[source] = declares_port_offset(source)
        if declares[source]:
            entry["parameters"] = dict(entry["parameters"], **{PORT_PARAMETER: PORT_STRIDE * slot})
        receiver, sender = context.Pipe(duplex=False)
        process = context.Process(target=run_job, args=(entry, slot, root, out_dir, args, sender), name=entry["id"])
        process.start()
        sender.close()
        return {"index": index, "slot": slot, "source": source, "process": process, "receiver": receiver,
                "entry": entry}

    def finish(job: dict) -> None:
        row = None
        try:
            if job["receiver"].poll():
                row = job["receiver"].recv()
        except (EOFError, OSError):
            row = None
        job["receiver"].close()
        job["process"].join()
        if row is None:
            entry = job["entry"]
            row = {
                "id": entry["id"],
                "notebook": relative(root, job["source"]),
                "output": relative(root, out_dir / ("%s.ipynb" % entry["id"])),
                "parameters": entry["parameters"],
                "status": "error",
                "duration_s": 0.0,
                "failure": {
                    "ename": "JobLost",
                    "evalue": "the job process ended with exit code %s and sent no row" % job["process"].exitcode,
                    "traceback": [],
                },
            }
        row["slot"] = job["slot"]
        rows[job["index"]] = row
        print_row(row)

    def skipped(index: int) -> bool:
        if not entries[index]["skip"]:
            return False
        rows[index] = execute(entries[index], root, out_dir, args)
        print_row(rows[index])
        return True

    pending = [index for index, entry in enumerate(entries) if not entry["exclusive"]]
    alone = [index for index, entry in enumerate(entries) if entry["exclusive"]]
    running = []
    free = sorted(slots)
    try:
        while pending or running:
            for index in list(pending):
                if not free:
                    break
                if skipped(index):
                    pending.remove(index)
                    continue
                source = resolve_notebook(root, entries[index]["notebook"])
                if any(job["source"] == source for job in running):
                    continue
                pending.remove(index)
                running.append(start(index, free.pop(0)))
            if not running:
                continue
            ready = wait([job["receiver"] for job in running] + [job["process"].sentinel for job in running])
            for job in list(running):
                if job["receiver"] in ready or job["process"].sentinel in ready:
                    finish(job)
                    running.remove(job)
                    free.append(job["slot"])
                    free.sort()
        for index in alone:
            if skipped(index):
                continue
            job = start(index, min(slots))
            running.append(job)
            wait([job["receiver"], job["process"].sentinel])
            finish(job)
            running.remove(job)
    finally:
        for job in running:
            if job["process"].is_alive():
                job["process"].terminate()
            job["process"].join()
    return rows


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description="run the Aosabi notebook matrix")
    parser.add_argument("--matrix", default="tests/matrix.yaml", help="the matrix file")
    parser.add_argument("--out", default="tests/notebooks/out", help="where executed notebooks go")
    parser.add_argument("--report", default=None, help="the report file, default <out>/report.json")
    parser.add_argument("--kernel", default=DEFAULT_KERNEL, help="the kernel name")
    parser.add_argument("--cell-timeout", type=int, default=DEFAULT_CELL_TIMEOUT, help="per cell timeout in seconds")
    parser.add_argument("--start-timeout", type=int, default=DEFAULT_START_TIMEOUT, help="kernel start timeout in seconds")
    parser.add_argument(
        "--only",
        action="append",
        default=[],
        help="run the entries whose id contains this text, and an entry outside the sweep whose id is this text",
    )
    parser.add_argument("--list", action="store_true", help="print the entries and exit")
    parser.add_argument("--log-output", action="store_true",
                        help="stream cell output while a notebook runs, under --jobs above 1 into <out>/<id>.log")
    parser.add_argument("--jobs", type=int, default=1,
                        help="run up to this many entries at once, each in its own kernel process, default 1")
    args = parser.parse_args(argv)
    if not 1 <= args.jobs <= MAX_JOBS:
        parser.error("--jobs takes 1 to %d, got %d" % (MAX_JOBS, args.jobs))

    root = find_root()
    matrix_path = Path(args.matrix)
    if not matrix_path.is_absolute():
        matrix_path = root / matrix_path
    entries = load_matrix(matrix_path)
    if args.only:
        entries = [
            e for e in entries if any(text == e["id"] if not e["sweep"] else text in e["id"] for text in args.only)
        ]
    else:
        entries = [e for e in entries if e["sweep"]]

    if args.list:
        for entry in entries:
            print("%-28s %s" % (entry["id"], entry["notebook"]))
        return 0

    out_dir = Path(args.out)
    if not out_dir.is_absolute():
        out_dir = root / out_dir
    out_dir.mkdir(parents=True, exist_ok=True)
    report_path = Path(args.report) if args.report else out_dir / "report.json"
    if not report_path.is_absolute():
        report_path = root / report_path

    kernel_environment(root)
    print("matrix: %s" % relative(root, matrix_path))
    print("kernel: %s, per cell timeout %ss" % (args.kernel, args.cell_timeout))
    print("entries: %d" % len(entries), flush=True)
    alone = [entry["id"] for entry in entries if entry["exclusive"]]
    held = []
    if args.jobs > 1:
        held = hold_slots(args.jobs)
        print("jobs: %d at once in slots %s, then %d exclusive entries alone"
              % (len(held), ", ".join(str(slot) for slot, _ in held), len(alone)), flush=True)

    rows = []
    started = time.monotonic()
    if args.jobs == 1:
        for entry in entries:
            rows.append(execute(entry, root, out_dir, args))
            print_row(rows[-1])
    else:
        try:
            rows = execute_jobs(entries, root, out_dir, args, [slot for slot, _ in held])
        finally:
            for _, fd in held:
                os.close(fd)

    counts = {}
    for row in rows:
        counts[row["status"]] = counts.get(row["status"], 0) + 1
    report = {
        "schema": REPORT_SCHEMA,
        "generated": datetime.now(timezone.utc).isoformat(timespec="seconds"),
        "matrix": relative(root, matrix_path),
        "kernel": args.kernel,
        "cell_timeout_s": args.cell_timeout,
        "duration_s": round(time.monotonic() - started, 3),
        "counts": counts,
        "notebooks": rows,
    }
    if args.jobs > 1:
        # Under --jobs 1 the report keeps the keys it always had, and a run
        # with jobs adds how many ran at once, which entries ran alone and the
        # slots it held.
        report = dict(report, jobs=args.jobs, exclusive=alone, slots=[slot for slot, _ in held])
    report_path.write_text(json.dumps(report, indent=2, sort_keys=False) + "\n", encoding="utf-8")
    print("report: %s" % relative(root, report_path))
    print("counts: %s" % ", ".join("%s %d" % (k, v) for k, v in sorted(counts.items())))
    return 1 if (counts.get("failed", 0) or counts.get("error", 0)) else 0


if __name__ == "__main__":
    raise SystemExit(main())
