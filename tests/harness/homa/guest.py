# SPDX-License-Identifier: MIT
"""The harness guests: boots them under KVM from `target/guests/`, shares the
tree, runs commands and collects their output.

The host has neither `tc` nor `dummy_hcd`, which is why they run in a guest.

Every file a guest boots from or installs is pinned in
`tests/guests/guests.lock` by its URL, its SHA-256 and its size, fetched into
`target/guests/cache/` and refused where either differs, and checked again
inside the guest. Before it downloads a file, the harness looks in the guest
cache of every other worktree of the same repository and takes a copy of the
same size from there by reflink, which costs no space on a filesystem that
shares extents, and hashes the copy against the lock before it keeps it, so a
fresh worktree boots its guests where the network cannot reach the mirrors.
Every file of a guest lives under `target/guests/`, never under `/tmp`, so a
run leaves no scratch output there. A guest is defined by the directory
`tests/guests/<dir>/` that holds its `guest.toml` and its provisioning
scripts.

A guest is provisioned once: its disk is an overlay of the pinned image, a
first boot takes a NoCloud seed that sets SELinux to permissive and lets the
QEMU guest agent run commands, and each provisioning phase runs its script
from a read only disk that holds the guest's pinned artifacts, with a reboot
between phases. A record of the definition and the lock it was built from
marks the disk as provisioned, and a changed definition or lock provisions it
again. A guest that another worktree of the repository has provisioned from
the same definition and lock, which its record shows by the same fingerprint,
is not provisioned again: its disk is taken by reflink and rebased onto the
image of the local cache, which the lock has checked, so the disk reads the
same bytes as in that worktree. Every later boot runs on a fresh overlay of
the provisioned disk that is removed at shutdown, so no run changes what the
next one boots.

Commands run through the QEMU guest agent over a Unix socket under
`target/guests/`, and the tree is mounted read only at `/mnt/tree` over 9p
with a writable exchange directory at `/mnt/xfer`, so no guest takes a port
of the host. The first network interface is QEMU's user mode network, which
reaches the Fedora repositories, and a `Hub` joins guests on a private
Ethernet segment over Unix datagram sockets, the bridge the interop halves of
`29_homa_raw` and `29_homa_kmod` run on.

The host tools are the host's own where it has them, and otherwise the pinned
packages of the lock unpacked into `target/guests/tools/`, so nothing is
installed on the host. A machine without `/dev/kvm` raises
`GuestUnavailable`, which a notebook prints as the reason its guest half is
not run.

Command line, from the `tests` directory:

    python3 -m harness.homa.guest fetch
    python3 -m harness.homa.guest provision A
    python3 -m harness.homa.guest check A B C

A provisioning phase compiles kernel modules, dummy_hcd in guest A and
HomaModule in guest C, so a machine where one build runs at a time runs
`provision` under the lock of its builds, as `flock <lock> python3 -m
harness.homa.guest provision C`. The module takes no lock itself, since a
notebook that boots a guest runs under that lock already.
"""

from __future__ import annotations

import base64
import fcntl
import hashlib
import json
import os
import random
import shutil
import socket
import subprocess
import sys
import threading
import time
import tomllib
import urllib.request
from dataclasses import dataclass
from pathlib import Path

from ..paths import find_root

ROOT = find_root(Path(__file__).resolve().parent)
GUESTS = ROOT / "tests" / "guests"
LOCK = GUESTS / "guests.lock"
STATE = ROOT / "target" / "guests"
CACHE = STATE / "cache"
TOOLS = STATE / "tools"
SLOTS = STATE / "slots"
HUBS = STATE / "hubs"

MEMORY_MIB = 2048
CPUS = 2
MAX_RUNNING = 3

# 0 runs QEMU in the cgroup of its caller.
CPU_WEIGHT = 10000

# Whether QEMU runs on the cores of the highest capacity alone where the
# machine has cores of two kinds, as the performance and efficiency cores of
# Apple silicon, whose capacities the kernel reads as 1024 and 485 on the
# development machine. A guest core that the host moves to an efficiency core
# runs at about half its speed, which moves the share of a load point of
# 29_homa_raw.
FAST_CORES_ONLY = True

# The label of the read only disk that carries a guest's pinned artifacts and
# provisioning scripts, at most 11 characters as FAT allows.
ARTIFACTS_LABEL = "HOMAART"
# The label cloud-init's NoCloud source looks for.
SEED_LABEL = "cidata"
# The private segment of a hub, one address per guest.
HUB_PREFIX = "10.146.0."
HUB_OFFSET = 10

AGENT_PORT = "org.qemu.guest_agent.0"
BOOT_TIMEOUT = 300.0
SHUTDOWN_TIMEOUT = 90.0
# How long `Guest.reach` pings a neighbour on a hub before it gives up.
REACH_TIMEOUT = 60.0
CHUNK = 1 << 20

# Whether the guest cache and the provisioned disks of the other worktrees of
# the repository are taken before a download or a provisioning, see
# `peer_roots`. False makes a worktree fetch and provision on its own.
SHARE_PEERS = True

FICLONE = 0x40049409


class GuestUnavailable(RuntimeError):
    """The machine cannot boot a guest, with the reason a notebook prints."""


class GuestFailed(RuntimeError):
    """A guest did not boot, did not answer or failed a provisioning phase."""


class CommandFailed(RuntimeError):
    """A command in a guest exited with a status other than 0."""

    def __init__(self, guest, command, result):
        super().__init__("guest %s: %r exited %s\n%s%s" % (guest, command, result.code, result.out[-4000:],
                                                            result.err[-4000:]))
        self.result = result


@dataclass(frozen=True)
class Artifact:
    """One pinned file of the lock."""

    name: str
    kind: str
    url: str
    file: str
    sha256: str
    size: int
    guests: tuple

    def path(self) -> Path:
        return CACHE / self.file


@dataclass(frozen=True)
class Definition:
    """One guest as `tests/guests/<dir>/guest.toml` defines it."""

    name: str
    directory: Path
    image: str
    kernel: str
    artifacts: tuple
    phases: tuple
    description: str


@dataclass(frozen=True)
class Result:
    """What a command in a guest returned."""

    code: int
    out: str
    err: str


def lock() -> dict:
    """Return the artifacts of the lock by name."""
    with LOCK.open("rb") as fh:
        data = tomllib.load(fh)
    if data.get("version") != 1:
        raise GuestFailed("%s: version %r, this harness reads version 1" % (LOCK, data.get("version")))
    out = {}
    for entry in data.get("artifact", []):
        artifact = Artifact(entry["name"], entry["kind"], entry["url"], entry["file"], entry["sha256"].lower(),
                            int(entry["size"]), tuple(entry.get("guests", ())))
        if artifact.name in out:
            raise GuestFailed("%s names %s twice" % (LOCK, artifact.name))
        out[artifact.name] = artifact
    return out


def definitions() -> dict:
    """Return every guest definition of `tests/guests/` by guest name."""
    out = {}
    for path in sorted(GUESTS.glob("*/guest.toml")):
        with path.open("rb") as fh:
            data = tomllib.load(fh)
        definition = Definition(data["name"], path.parent, data["image"], data["kernel"],
                                tuple(data.get("artifacts", ())), tuple(data.get("phases", ())),
                                data.get("description", ""))
        if definition.name in out:
            raise GuestFailed("two directories define guest %s" % definition.name)
        out[definition.name] = definition
    return out


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(CHUNK), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _verified(artifact: Artifact) -> bool:
    """Return whether the cached file matches the lock, rehashing it once per
    change of its size or time."""
    path = artifact.path()
    if not path.is_file():
        return False
    stat = path.stat()
    stamp = path.with_name(path.name + ".verified")
    mark = "%s %d %d\n" % (artifact.sha256, stat.st_size, stat.st_mtime_ns)
    if stamp.is_file() and stamp.read_text() == mark:
        return True
    if stat.st_size != artifact.size or _sha256(path) != artifact.sha256:
        return False
    stamp.write_text(mark)
    return True


def peer_roots() -> list:
    """Return the roots of the other worktrees of the repository this tree
    belongs to, the main checkout among them, read from the git metadata
    without running git, or an empty list where `SHARE_PEERS` is false.
    """
    if not SHARE_PEERS:
        return []
    dot = ROOT / ".git"
    try:
        if dot.is_dir():
            common = dot
        else:
            own = Path(dot.read_text().split("gitdir:", 1)[1].strip())
            own = own if own.is_absolute() else (ROOT / own)
            named = own / "commondir"
            common = (own / named.read_text().strip()) if named.is_file() else own
        common = common.resolve()
    except (OSError, IndexError):
        return []
    found = []
    if common.name == ".git":
        found.append(common.parent)
    for entry in sorted((common / "worktrees").glob("*/gitdir")):
        try:
            found.append(Path(entry.read_text().strip()).parent)
        except OSError:
            continue
    here = ROOT.resolve()
    out = []
    for root in found:
        try:
            root = root.resolve()
        except OSError:
            continue
        if root != here and root not in out and root.is_dir():
            out.append(root)
    return out


def clone(source: Path, target: Path) -> str:
    """Copy `source` to `target` by reflink where the filesystem allows it and
    byte by byte where it does not, and return which of the two it did."""
    with source.open("rb") as src, target.open("wb") as dst:
        try:
            fcntl.ioctl(dst.fileno(), FICLONE, src.fileno())
            return "reflink"
        except OSError:
            shutil.copyfileobj(src, dst, CHUNK)
            return "copy"


def adopt(artifact: Artifact, log=print) -> bool:
    """Take `artifact` from the guest cache of another worktree of the
    repository, and return whether a copy that matches the lock is now in
    this cache.

    The copy goes to a partial file that is hashed in full against the lock
    and renamed only where its size and its SHA-256 equal the lock's, so the
    stamp of the other worktree is never trusted.
    """
    path = artifact.path()
    for root in peer_roots():
        source = root / "target" / "guests" / "cache" / artifact.file
        try:
            if not source.is_file() or source.stat().st_size != artifact.size:
                continue
        except OSError:
            continue
        CACHE.mkdir(parents=True, exist_ok=True)
        part = path.with_name(path.name + ".part")
        try:
            how = clone(source, part)
        except OSError:
            part.unlink(missing_ok=True)
            continue
        if part.stat().st_size == artifact.size and _sha256(part) == artifact.sha256:
            part.rename(path)
            _verified(artifact)
            log("took %s from %s by %s, its SHA-256 equals the lock's" % (artifact.file, root, how))
            return True
        part.unlink(missing_ok=True)
        log("refused %s of %s, its SHA-256 differs from the lock's" % (artifact.file, root))
    return False


def fetch(artifact: Artifact, log=print) -> Path:
    """Fetch `artifact` into the cache unless a verified copy is there, and
    return its path.

    A copy in the guest cache of another worktree of the repository is taken
    first, see `adopt`. The download goes to a partial file that is renamed
    only once its size and its SHA-256 equal the lock's, so the cache never
    holds a file the lock does not name.
    """
    path = artifact.path()
    if _verified(artifact):
        return path
    if adopt(artifact, log):
        return path
    CACHE.mkdir(parents=True, exist_ok=True)
    part = path.with_name(path.name + ".part")
    log("fetch %s from %s" % (artifact.file, artifact.url))
    request = urllib.request.Request(artifact.url, headers={"User-Agent": "aosabi-harness/1"})
    digest = hashlib.sha256()
    size = 0
    with urllib.request.urlopen(request, timeout=60) as response, part.open("wb") as out:
        for chunk in iter(lambda: response.read(CHUNK), b""):
            digest.update(chunk)
            size += len(chunk)
            if size > artifact.size:
                break
            out.write(chunk)
    if size != artifact.size or digest.hexdigest() != artifact.sha256:
        part.unlink(missing_ok=True)
        raise GuestFailed("%s: got %d bytes with SHA-256 %s, the lock names %d bytes and %s"
                          % (artifact.url, size, digest.hexdigest(), artifact.size, artifact.sha256))
    part.rename(path)
    _verified(artifact)
    return path


def fetch_all(names=None, log=print) -> list:
    """Fetch the named artifacts, every artifact of the lock by default."""
    pinned = lock()
    return [fetch(pinned[name], log) for name in (names or pinned)]


@dataclass(frozen=True)
class Tools:
    """The QEMU binaries, the firmware and the environment they run in."""

    qemu: str
    qemu_img: str
    firmware: str
    env: dict
    origin: str

    def version(self) -> str:
        done = subprocess.run([self.qemu, "--version"], env=self.env, capture_output=True, text=True, check=True)
        return done.stdout.splitlines()[0]


HOST_FIRMWARE = Path("/usr/share/edk2/aarch64/QEMU_EFI.fd")


def tools(log=print) -> Tools:
    """Return the host's QEMU where it has one, and otherwise the pinned tool
    packages of the lock unpacked into `target/guests/tools/`."""
    qemu = shutil.which("qemu-system-aarch64")
    qemu_img = shutil.which("qemu-img")
    if qemu and qemu_img and HOST_FIRMWARE.is_file():
        return Tools(qemu, qemu_img, str(HOST_FIRMWARE), dict(os.environ), "host")
    packages = [artifact for artifact in lock().values() if artifact.kind == "tool"]
    stamp = TOOLS / "unpacked.json"
    want = sorted("%s %s" % (artifact.file, artifact.sha256) for artifact in packages)
    if not stamp.is_file() or json.loads(stamp.read_text()) != want:
        if shutil.which("rpm2cpio") is None or shutil.which("cpio") is None:
            raise GuestUnavailable("the host has no qemu-system-aarch64 and no rpm2cpio and cpio to unpack "
                                   "the pinned one")
        shutil.rmtree(TOOLS, ignore_errors=True)
        TOOLS.mkdir(parents=True)
        for artifact in packages:
            path = fetch(artifact, log)
            unpack = "rpm2cpio '%s' | cpio -idmu --quiet" % path
            subprocess.run(["sh", "-c", unpack], cwd=TOOLS, check=True)
        stamp.write_text(json.dumps(want))
    env = dict(os.environ)
    libraries = str(TOOLS / "usr" / "lib64")
    env["LD_LIBRARY_PATH"] = libraries + (":" + env["LD_LIBRARY_PATH"] if env.get("LD_LIBRARY_PATH") else "")
    return Tools(str(TOOLS / "usr" / "bin" / "qemu-system-aarch64"), str(TOOLS / "usr" / "bin" / "qemu-img"),
                 str(TOOLS / "usr" / "share" / "edk2" / "aarch64" / "QEMU_EFI.fd"), env, "lock")


_SCOPES = {}


def scope(env) -> list:
    """Return the command that runs a program in a scope of its own of the
    user's systemd with `CPU_WEIGHT`, or an empty list where the machine has
    no such systemd or the weight is 0.

    `systemd-run --scope` runs the program in its own process, so the
    program's pid, its descriptors and its exit are those of a direct start.
    The command runs `true` once in the environment `env` first and is used
    only where that exits 0.
    """
    if not CPU_WEIGHT:
        return []
    key = (CPU_WEIGHT, env.get("LD_LIBRARY_PATH"))
    if key not in _SCOPES:
        _SCOPES[key] = []
        program = shutil.which("systemd-run")
        if program and env.get("XDG_RUNTIME_DIR"):
            argv = [program, "--user", "--scope", "--quiet", "--collect", "--expand-environment=no",
                    "-p", "CPUWeight=%d" % CPU_WEIGHT, "--"]
            try:
                probe = subprocess.run(argv + ["true"], env=env, capture_output=True, timeout=60)
            except (OSError, subprocess.TimeoutExpired):
                probe = None
            if probe is not None and probe.returncode == 0:
                _SCOPES[key] = argv
    return list(_SCOPES[key])


def fast_cores() -> list:
    """Return the CPUs of the highest capacity this process may run on, or
    an empty list where they all have one capacity or the kernel names
    none, or where they are fewer than the guest's cores."""
    allowed = sorted(os.sched_getaffinity(0))
    capacity = {}
    for cpu in allowed:
        try:
            capacity[cpu] = int(Path("/sys/devices/system/cpu/cpu%d/cpu_capacity" % cpu).read_text())
        except (OSError, ValueError):
            return []
    top = max(capacity.values(), default=0)
    fast = [cpu for cpu in allowed if capacity[cpu] == top]
    return fast if CPUS <= len(fast) < len(allowed) else []


def affinity() -> list:
    """Return the command that runs a program on `fast_cores` alone, or an
    empty list where `FAST_CORES_ONLY` is false, the cores are of one kind or
    the machine has no `taskset`."""
    program = shutil.which("taskset")
    cores = fast_cores() if FAST_CORES_ONLY else []
    if not program or not cores:
        return []
    return [program, "-c", ",".join(str(cpu) for cpu in cores)]


def kvm_reason():
    """Return why this machine cannot run a guest under KVM, or None."""
    if not os.path.exists("/dev/kvm"):
        return "no /dev/kvm on this machine"
    if not os.access("/dev/kvm", os.R_OK | os.W_OK):
        return "/dev/kvm is not readable and writable for this user"
    if os.uname().machine != "aarch64":
        return "the guests are aarch64 and KVM runs them on an aarch64 host alone, this host is %s" % os.uname().machine
    return None


class Slot:
    """One of the `MAX_RUNNING` places a guest runs in, held by a lock file
    that the QEMU process inherits, so a place stays taken for as long as its
    guest runs, whoever started it."""

    def __init__(self, timeout=600.0):
        SLOTS.mkdir(parents=True, exist_ok=True)
        deadline = time.monotonic() + timeout
        while True:
            for index in range(MAX_RUNNING):
                fd = os.open(SLOTS / ("slot-%d.lock" % index), os.O_RDWR | os.O_CREAT, 0o644)
                try:
                    fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
                except BlockingIOError:
                    os.close(fd)
                    continue
                self.fd = fd
                self.index = index
                return
            if time.monotonic() > deadline:
                raise GuestFailed("%d guests run already and none ended within %.0f s" % (MAX_RUNNING, timeout))
            time.sleep(1.0)

    def release(self):
        if self.fd is not None:
            os.close(self.fd)
            self.fd = None


class Agent:
    """A client of the QEMU guest agent over its Unix socket."""

    def __init__(self, path: Path, timeout=30.0):
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.settimeout(timeout)
        self.sock.connect(str(path))
        self.file = self.sock.makefile("rwb")
        token = random.randrange(1, 1 << 30)
        self._send({"execute": "guest-sync-delimited", "arguments": {"id": token}}, prefix=b"\xff")
        while True:
            byte = self.file.read(1)
            if not byte:
                raise ConnectionError("the guest agent closed the socket")
            if byte == b"\xff":
                break
        while True:
            answer = self._read()
            if answer.get("return") == token:
                break

    def _send(self, message, prefix=b""):
        self.file.write(prefix + json.dumps(message).encode() + b"\n")
        self.file.flush()

    def _read(self):
        line = self.file.readline()
        if not line:
            raise ConnectionError("the guest agent closed the socket")
        return json.loads(line)

    def call(self, command, **arguments):
        message = {"execute": command}
        if arguments:
            message["arguments"] = arguments
        self._send(message)
        answer = self._read()
        if "error" in answer:
            raise GuestFailed("%s: %s" % (command, answer["error"].get("desc", answer["error"])))
        return answer.get("return")

    def notify(self, command, **arguments):
        """Send a command that answers nothing, as a shutdown."""
        message = {"execute": command}
        if arguments:
            message["arguments"] = arguments
        self._send(message)

    def close(self):
        try:
            self.file.close()
        finally:
            self.sock.close()


SEED_USER_DATA = """#cloud-config
hostname: {hostname}
bootcmd:
  - [sh, -c, "setenforce 0 || true; sed -i 's/^SELINUX=enforcing$/SELINUX=permissive/' /etc/selinux/config"]
write_files:
  - path: /etc/sysconfig/qemu-ga
    content: |
      FILTER_RPC_ARGS=""
runcmd:
  - [systemctl, restart, qemu-guest-agent]
  - [touch, /run/homa-guest-ready]
"""


class Hub:
    """A private Ethernet segment of guests over Unix datagram sockets.

    Each guest attaches a second network interface whose frames QEMU sends as
    datagrams from a socket of its own to the hub's socket, and the hub
    forwards each frame to every other guest, as a repeater does. A guest is
    attached by the path of its socket when it starts, so the first frame
    sent to it, an ARP request among them, reaches it before it has sent a
    frame of its own. A sender the hub was not told of is attached by its
    first frame. `frames` counts the frames the hub took and `dropped` the
    copies it could not hand a guest, one whose socket is not bound yet or
    whose queue stayed full for the send timeout. Nothing of it touches the
    host's network or its ports.
    """

    def __init__(self, name="bridge"):
        self.name = name
        self.dir = HUBS / name
        self.dir.mkdir(parents=True, exist_ok=True)
        self.path = self.dir / "hub.sock"
        self.path.unlink(missing_ok=True)
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM)
        self.sock.bind(str(self.path))
        self.sock.settimeout(0.2)
        self.peers = []
        self.lock = threading.Lock()
        self.frames = 0
        self.dropped = 0
        self.stopped = threading.Event()
        self.thread = threading.Thread(target=self._relay, name="hub-" + name, daemon=True)
        self.thread.start()

    def attach(self, path):
        """Forward the segment's frames to the socket at `path`."""
        with self.lock:
            if str(path) not in self.peers:
                self.peers.append(str(path))

    def detach(self, path):
        """Stop forwarding to the socket at `path`."""
        with self.lock:
            if str(path) in self.peers:
                self.peers.remove(str(path))

    def _relay(self):
        while not self.stopped.is_set():
            try:
                frame, sender = self.sock.recvfrom(65536)
            except TimeoutError:
                continue
            except OSError:
                return
            if sender:
                self.attach(sender)
            self.frames += 1
            with self.lock:
                peers = list(self.peers)
            for peer in peers:
                if peer == sender:
                    continue
                try:
                    self.sock.sendto(frame, peer)
                except OSError:
                    self.dropped += 1

    def close(self):
        self.stopped.set()
        self.thread.join(timeout=2)
        self.sock.close()
        self.path.unlink(missing_ok=True)


class Guest:
    """One guest of `tests/guests/`, booted by `boot` or as a context manager.

    `run` runs a shell command as root and returns its `Result`, `put` places
    a file of the host in the guest's exchange directory and returns its
    path in the guest, and `shutdown` ends the guest and removes its run
    overlay.
    """

    def __init__(self, name, hubs=(), log=print):
        found = definitions()
        if name not in found:
            raise GuestFailed("no guest %s under %s, the guests are %s" % (name, GUESTS, sorted(found)))
        self.name = name
        self.definition = found[name]
        self.hubs = tuple(hubs)
        self.log = log
        self.dir = STATE / name
        self.proc = None
        self.slot = None
        self.agent = None
        self.boot_seconds = None
        # The CPU weight of the scope QEMU runs in, None where it runs in the
        # cgroup of its caller, and the host's CPUs it runs on, None where it
        # may run on every one.
        self.cpu_weight = None
        self.host_cpus = None

    # The definition and its record.

    def _fingerprint(self) -> str:
        """Hash the definition's files and the lock entries it takes."""
        digest = hashlib.sha256()
        for path in sorted(self.definition.directory.rglob("*")):
            if path.is_file():
                digest.update(path.relative_to(GUESTS).as_posix().encode() + b"\0" + path.read_bytes() + b"\0")
        pinned = lock()
        for name in (self.definition.image,) + self.definition.artifacts:
            artifact = pinned[name]
            digest.update(("%s %s %s\0" % (artifact.name, artifact.file, artifact.sha256)).encode())
        return digest.hexdigest()

    def provisioned(self) -> bool:
        record = self.dir / "provisioned.json"
        if not (record.is_file() and (self.dir / "base.qcow2").is_file()):
            return False
        return json.loads(record.read_text()).get("fingerprint") == self._fingerprint()

    # Provisioning.

    def adopt_disk(self) -> bool:
        """Take the provisioned disk of this guest from another worktree of
        the repository whose record names the same fingerprint, and return
        whether one was taken.

        The disk is an overlay whose backing file is the image in the cache
        of the worktree that provisioned it. The image of this cache is
        fetched first, which takes it from a peer and hashes it against the
        lock, the disk is copied by reflink, and an unsafe rebase, which
        rewrites the name of the backing file alone, points it at that
        image, whose bytes the lock proves equal. A disk whose record or
        file changes during the copy is left, since its worktree is
        provisioning it again.
        """
        fingerprint = None
        for root in peer_roots():
            peer = root / "target" / "guests" / self.name
            record, base = peer / "provisioned.json", peer / "base.qcow2"
            try:
                text = record.read_text()
                kept = json.loads(text)
                before = base.stat()
            except (OSError, ValueError):
                continue
            if fingerprint is None:
                fingerprint = self._fingerprint()
            if kept.get("fingerprint") != fingerprint:
                continue
            image = fetch(lock()[self.definition.image], self.log)
            qemu = tools(self.log)
            shutil.rmtree(self.dir, ignore_errors=True)
            self.dir.mkdir(parents=True)
            part = self.dir / "base.qcow2.part"
            try:
                how = clone(base, part)
                after = base.stat()
                if (record.read_text() != text or (before.st_size, before.st_mtime_ns)
                        != (after.st_size, after.st_mtime_ns)):
                    raise OSError("the disk of %s changed during the copy" % peer)
                info = json.loads(subprocess.run([qemu.qemu_img, "info", "--output=json", "-U", str(part)],
                                                 env=qemu.env, capture_output=True, text=True,
                                                 check=True).stdout)
                if Path(info.get("backing-filename", "")).name != image.name:
                    raise OSError("the disk of %s is backed by %r, not by %s"
                                  % (peer, info.get("backing-filename"), image.name))
                subprocess.run([qemu.qemu_img, "rebase", "-u", "-f", "qcow2", "-F", "qcow2", "-b", str(image),
                                str(part)], env=qemu.env, check=True, capture_output=True)
            except (OSError, ValueError, subprocess.CalledProcessError) as error:
                part.unlink(missing_ok=True)
                self.log("guest %s: left the disk of %s, %s" % (self.name, peer, error))
                continue
            part.rename(self.dir / "base.qcow2")
            (self.dir / "provisioned.json").write_text(json.dumps({
                "fingerprint": fingerprint, "kernel": kept.get("kernel"),
                "at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
                "taken_from": str(peer), "provisioned_at": kept.get("at")}, indent=1) + "\n")
            self.log("guest %s: took the provisioned disk of %s by %s and rebased it onto %s"
                     % (self.name, peer, how, image.relative_to(ROOT)))
            return True
        return False

    def provision(self, force=False, phase_timeout=3600.0):
        """Build the provisioned disk of the guest unless it stands, taking it
        from another worktree of the repository where one stands there."""
        if not force and self.provisioned():
            return
        if not force and self.adopt_disk():
            return
        reason = kvm_reason()
        if reason:
            raise GuestUnavailable(reason)
        pinned = lock()
        image = fetch(pinned[self.definition.image], self.log)
        for name in self.definition.artifacts:
            fetch(pinned[name], self.log)
        shutil.rmtree(self.dir, ignore_errors=True)
        self.dir.mkdir(parents=True)
        qemu = tools(self.log)
        subprocess.run([qemu.qemu_img, "create", "-q", "-f", "qcow2", "-F", "qcow2", "-b", str(image),
                        str(self.dir / "base.qcow2"), "20G"], env=qemu.env, check=True)
        seed = self.dir / "seed"
        seed.mkdir()
        fingerprint = self._fingerprint()
        (seed / "meta-data").write_text("instance-id: homa-%s-%s\nlocal-hostname: homa-%s\n"
                                        % (self.name.lower(), fingerprint[:12], self.name.lower()))
        (seed / "user-data").write_text(SEED_USER_DATA.format(hostname="homa-" + self.name.lower()))
        carried = self.dir / "artifacts"
        carried.mkdir()
        lines = []
        for name in self.definition.artifacts:
            artifact = pinned[name]
            try:
                os.link(artifact.path(), carried / artifact.file)
            except OSError:
                shutil.copy2(artifact.path(), carried / artifact.file)
            lines.append("%s  %s" % (artifact.sha256, artifact.file))
        (carried / "SHA256SUMS").write_text("".join(line + "\n" for line in lines))
        for path in self.definition.directory.iterdir():
            if path.is_file():
                shutil.copy2(path, carried / path.name)
        log_path = self.dir / "provision.log"
        with log_path.open("w") as record:
            for index, phase in enumerate(self.definition.phases):
                self._start(self.dir / "base.qcow2", seed=seed, artifacts=carried)
                try:
                    # The seed's runcmd, which lets the agent run commands,
                    # runs on the first boot of the instance alone, and its
                    # settings stay for every boot after it.
                    self._wait_ready(provisioning=index == 0)
                    self.run("cloud-init status --wait >/dev/null 2>&1 || true", timeout=600, check=False)
                    script = ("mkdir -p /mnt/homa && (mountpoint -q /mnt/homa || mount -o ro LABEL=%s /mnt/homa)"
                              " && cd /mnt/homa && { ! test -s SHA256SUMS || sha256sum -c --quiet SHA256SUMS; }"
                              " && sh ./%s" % (ARTIFACTS_LABEL, phase))
                    self.log("guest %s: provisioning phase %d of %d, %s"
                             % (self.name, index + 1, len(self.definition.phases), phase))
                    started = time.monotonic()
                    result = self.run(script, timeout=phase_timeout, check=False)
                    record.write("== phase %s exit %s after %.1f s\n%s\n== stderr\n%s\n"
                                 % (phase, result.code, time.monotonic() - started, result.out, result.err))
                    record.flush()
                    if result.code != 0:
                        raise GuestFailed("guest %s: provisioning phase %s exited %s, see %s\n%s"
                                          % (self.name, phase, result.code, log_path, result.err[-3000:]))
                finally:
                    self._stop()
        self._start(self.dir / "base.qcow2")
        try:
            self._wait_ready(provisioning=False)
            kernel = self.run("uname -r").out.strip()
        finally:
            self._stop()
        if kernel != self.definition.kernel:
            raise GuestFailed("guest %s runs kernel %s after provisioning, its definition names %s"
                              % (self.name, kernel, self.definition.kernel))
        shutil.rmtree(seed)
        shutil.rmtree(carried)
        (self.dir / "provisioned.json").write_text(json.dumps({
            "fingerprint": fingerprint, "kernel": kernel,
            "at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}, indent=1) + "\n")

    # Booting.

    def _start(self, disk, seed=None, artifacts=None):
        reason = kvm_reason()
        if reason:
            raise GuestUnavailable(reason)
        qemu = tools(self.log)
        self.slot = Slot()
        (self.dir / "xfer").mkdir(parents=True, exist_ok=True)
        for name in ("qga.sock", "qemu.pid"):
            (self.dir / name).unlink(missing_ok=True)
        index = sorted(definitions()).index(self.name) + 1
        argv = [qemu.qemu, "-name", "homa-" + self.name,
                "-machine", "virt,accel=kvm,gic-version=3", "-cpu", "host",
                "-smp", str(CPUS), "-m", str(MEMORY_MIB),
                "-nographic", "-monitor", "none", "-display", "none",
                "-serial", "file:console.log", "-pidfile", "qemu.pid",
                "-bios", qemu.firmware,
                "-drive", "file=%s,if=virtio,format=qcow2" % os.path.relpath(disk, self.dir),
                "-netdev", "user,id=net0",
                "-device", "virtio-net-pci,netdev=net0,romfile=,mac=52:54:00:14:60:%02x" % index,
                "-object", "rng-random,filename=/dev/urandom,id=rng0",
                "-device", "virtio-rng-pci,rng=rng0",
                "-virtfs", "local,path=%s,mount_tag=tree,security_model=none,readonly=on" % ROOT,
                "-virtfs", "local,path=xfer,mount_tag=xfer,security_model=none",
                "-chardev", "socket,path=qga.sock,server=on,wait=off,id=qga0",
                "-device", "virtio-serial-pci",
                "-device", "virtserialport,chardev=qga0,name=" + AGENT_PORT]
        for label, directory in ((SEED_LABEL, seed), (ARTIFACTS_LABEL, artifacts)):
            if directory is not None:
                node = label.lower()
                argv += ["-blockdev", "driver=vvfat,node-name=%s,dir=%s,label=%s,read-only=on"
                         % (node, os.path.relpath(directory, self.dir), label),
                         "-device", "virtio-blk-pci,drive=" + node]
        for number, hub in enumerate(self.hubs):
            # The hub answers each guest at the address its datagrams come
            # from, so the guest's socket takes an absolute path.
            local = self.dir / ("hub-%s.sock" % hub.name)
            local.unlink(missing_ok=True)
            hub.attach(local)
            argv += ["-netdev", "dgram,id=hub%d,local.type=unix,local.path=%s,remote.type=unix,remote.path=%s"
                     % (number, local, hub.path),
                     "-device", "virtio-net-pci,netdev=hub%d,romfile=,mac=52:54:00:14:%02x:%02x"
                     % (number, 0x61 + number, index)]
        (self.dir / "console.log").unlink(missing_ok=True)
        pinned = affinity()
        wrapper = scope(qemu.env)
        self.cpu_weight = CPU_WEIGHT if wrapper else None
        self.host_cpus = pinned[-1] if pinned else None
        wrapper = pinned + wrapper
        self.started = time.monotonic()
        with (self.dir / "qemu.log").open("w") as out:
            self.proc = subprocess.Popen(wrapper + argv, cwd=self.dir, env=qemu.env, stdin=subprocess.DEVNULL,
                                         stdout=out, stderr=subprocess.STDOUT, pass_fds=(self.slot.fd,))

    def _alive(self):
        if self.proc is not None and self.proc.poll() is not None:
            raise GuestFailed("guest %s: QEMU exited %s\n%s" % (self.name, self.proc.returncode,
                                                               (self.dir / "qemu.log").read_text()[-3000:]))

    def _wait_ready(self, provisioning):
        deadline = self.started + BOOT_TIMEOUT
        while True:
            self._alive()
            if time.monotonic() > deadline:
                raise GuestFailed("guest %s: no answer of its agent within %.0f s, see %s"
                                  % (self.name, BOOT_TIMEOUT, self.dir / "console.log"))
            try:
                self.agent = Agent(self.dir / "qga.sock", timeout=5.0)
                self.agent.call("guest-ping")
                if provisioning:
                    probe = self.run("test -e /run/homa-guest-ready", timeout=30, check=False)
                    if probe.code != 0:
                        raise ConnectionError("cloud-init has not finished")
                else:
                    self.run("true", timeout=30)
                break
            except (OSError, ConnectionError, GuestFailed, json.JSONDecodeError):
                if self.agent is not None:
                    self.agent.close()
                    self.agent = None
                time.sleep(1.0)
        self.agent.sock.settimeout(60.0)
        self.boot_seconds = time.monotonic() - self.started

    def boot(self):
        """Boot the provisioned guest on a fresh overlay, mount the shares
        and bring up its hub interfaces, and return the guest."""
        if not self.provisioned():
            self.provision()
        qemu = tools(self.log)
        run_disk = self.dir / "run.qcow2"
        run_disk.unlink(missing_ok=True)
        subprocess.run([qemu.qemu_img, "create", "-q", "-f", "qcow2", "-F", "qcow2", "-b", "base.qcow2",
                        str(run_disk)], cwd=self.dir, env=qemu.env, check=True)
        self._start(run_disk)
        try:
            self._wait_ready(provisioning=False)
            self.run("modprobe 9pnet_virtio && mkdir -p /mnt/tree /mnt/xfer"
                     " && (mountpoint -q /mnt/tree || mount -t 9p -o trans=virtio,version=9p2000.L,ro tree /mnt/tree)"
                     " && (mountpoint -q /mnt/xfer || mount -t 9p -o trans=virtio,version=9p2000.L xfer /mnt/xfer)")
            index = sorted(definitions()).index(self.name) + 1
            for number, hub in enumerate(self.hubs):
                # NetworkManager starts DHCP on an interface that comes up
                # without an address and removes every address of it when
                # DHCP gives up after 45 s, so the hub interface is left to
                # the harness once NetworkManager runs, which the agent may
                # answer before, and takes its address before its link is up.
                mac = "52:54:00:14:%02x:%02x" % (0x61 + number, index)
                self.run("dev=$(ip -o link | awk -v m=%s '$0 ~ m {sub(\":\", \"\", $2); print $2}')"
                         " && if command -v nmcli >/dev/null; then"
                         " for i in $(seq 120); do systemctl -q is-active NetworkManager && break; sleep 0.5; done;"
                         " nmcli device set \"$dev\" managed no; fi"
                         " && ip addr add %s/24 dev \"$dev\" && ip link set \"$dev\" up"
                         % (mac, self.hub_address()))
        except BaseException:
            self.shutdown()
            raise
        return self

    def hub_address(self) -> str:
        """Return this guest's address on the segment of a hub."""
        index = sorted(definitions()).index(self.name) + 1
        return "%s%d" % (HUB_PREFIX, HUB_OFFSET + index)

    def reach(self, address, seconds=REACH_TIMEOUT) -> int:
        """Ping `address` until it answers or `seconds` pass, and return the
        rounds it took.

        A round is `ping -c 3 -W 2`, which answers where one of its three
        echoes returns, so a neighbour whose interface or address is still
        coming up, or a frame the hub dropped, costs a round and not the
        check. Raises `CommandFailed` with the last round where no round
        answers before the deadline.
        """
        command = "ping -c 3 -W 2 %s >/dev/null" % address
        deadline = time.monotonic() + seconds
        rounds = 0
        while True:
            rounds += 1
            result = self.run(command, timeout=30, check=False)
            if result.code == 0:
                return rounds
            if time.monotonic() > deadline:
                raise CommandFailed(self.name, command, result)
            time.sleep(1.0)

    def __enter__(self):
        return self.boot()

    def __exit__(self, *exc):
        self.shutdown()
        return False

    # Commands and files.

    def run(self, command, timeout=600.0, check=True, stdin=None) -> Result:
        """Run `command` with `/bin/sh -c` as root and return what it returned."""
        if self.agent is None:
            raise GuestFailed("guest %s is not running" % self.name)
        arguments = {"path": "/bin/sh",
                     "arg": ["-c", "export PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin; "
                             + command],
                     "capture-output": True}
        if stdin is not None:
            arguments["input-data"] = base64.b64encode(stdin).decode()
        pid = self.agent.call("guest-exec", **arguments)["pid"]
        deadline = time.monotonic() + timeout
        pause = 0.02
        while True:
            status = self.agent.call("guest-exec-status", pid=pid)
            if status.get("exited"):
                break
            self._alive()
            if time.monotonic() > deadline:
                self.run("kill -9 %d 2>/dev/null || true" % pid, timeout=30, check=False)
                raise GuestFailed("guest %s: %r ran past %.0f s" % (self.name, command, timeout))
            time.sleep(pause)
            pause = min(pause * 2, 0.5)
        result = Result(status.get("exitcode", -status.get("signal", 1)),
                        base64.b64decode(status.get("out-data", "")).decode(errors="replace"),
                        base64.b64decode(status.get("err-data", "")).decode(errors="replace"))
        if check and result.code != 0:
            raise CommandFailed(self.name, command, result)
        return result

    def put(self, path, name=None) -> str:
        """Copy a file of the host into the exchange directory and return its
        path in the guest."""
        name = name or Path(path).name
        shutil.copy2(path, self.dir / "xfer" / name)
        return "/mnt/xfer/" + name

    def xfer(self) -> Path:
        """Return the host side of the exchange directory, `/mnt/xfer` in the guest."""
        return self.dir / "xfer"

    # Shutting down.

    def _stop(self):
        for hub in self.hubs:
            hub.detach(self.dir / ("hub-%s.sock" % hub.name))
        if self.proc is not None:
            if self.proc.poll() is None and self.agent is not None:
                try:
                    self.agent.notify("guest-shutdown", mode="powerdown")
                except OSError:
                    pass
            try:
                self.proc.wait(timeout=SHUTDOWN_TIMEOUT)
            except subprocess.TimeoutExpired:
                self.proc.terminate()
                try:
                    self.proc.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    self.proc.kill()
                    self.proc.wait()
            self.proc = None
        if self.agent is not None:
            self.agent.close()
            self.agent = None
        if self.slot is not None:
            self.slot.release()
            self.slot = None

    def shutdown(self):
        """End the guest and remove its run overlay."""
        self._stop()
        (self.dir / "run.qcow2").unlink(missing_ok=True)


def placement(guest: Guest) -> str:
    """Return where the host runs the QEMU of a booted guest, the CPU weight
    of its scope and the host's CPUs it may run on, so a run shows whether
    the host took `CPU_WEIGHT` and `FAST_CORES_ONLY` or refused them."""
    return "QEMU: CPU weight %s, host CPUs %s" % (guest.cpu_weight or "of its caller", guest.host_cpus or "any")


def describe(guest: Guest) -> dict:
    """Return what a notebook's environment cell prints of a running guest."""
    probe = guest.run("uname -r; getconf PAGESIZE; nproc; awk '/MemTotal/ {print $2}' /proc/meminfo;"
                      " . /etc/os-release; echo \"$PRETTY_NAME\"").out.split("\n")
    return {"guest": guest.name, "kernel": probe[0], "page_size": int(probe[1]), "cpus": int(probe[2]),
            "memory_kib": int(probe[3]), "os": probe[4], "boot_seconds": round(guest.boot_seconds or 0.0, 1),
            "cpu_weight": guest.cpu_weight, "host_cpus": guest.host_cpus}


def main(argv):
    """The command line: `fetch`, `tools`, `provision NAME... [--force]` and
    `check NAME...`."""
    if not argv:
        print(__doc__)
        return 64
    command = argv[0]
    names = [name for name in argv[1:] if not name.startswith("--")]
    if command == "fetch":
        for path in fetch_all():
            print("verified", path.relative_to(ROOT))
        return 0
    if command == "tools":
        found = tools()
        print(found.origin, found.version(), found.firmware)
        return 0
    reason = kvm_reason()
    if reason:
        print("not run: " + reason)
        return 69
    if command == "provision":
        for name in names:
            Guest(name).provision(force="--force" in argv)
            print("guest %s provisioned" % name)
        return 0
    if command == "check":
        hub = Hub("check")
        guests = [Guest(name, hubs=(hub,)) for name in names]
        try:
            for guest in guests:
                guest.boot()
                print(json.dumps(describe(guest)))
            for guest in guests:
                rounds = [guest.reach(other.hub_address()) for other in guests if other is not guest]
                print("guest %s reaches %d others on the hub, in %s rounds of ping"
                      % (guest.name, len(rounds), ", ".join(str(n) for n in rounds)))
            print("the hub took %d frames and dropped %d copies" % (hub.frames, hub.dropped))
        finally:
            for guest in guests:
                guest.shutdown()
            hub.close()
        return 0
    print("unknown command %r" % command)
    return 64


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
