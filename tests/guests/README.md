# The harness guests

The virtual machines the carrier notebooks of the Homa track run in, booted under KVM by
[󰀀](../harness/homa/guest.py) from `target/guests/` and never from `/tmp`, each with 2 GiB and 2
cores, at most three at once.

- [`guests.lock`](guests.lock) pins every image, kernel package, source and host tool a guest boots
  from or installs by its URL, its SHA-256 and its size. The harness refuses a file that differs,
  and a guest's provisioning checks each hash again inside the guest.
- Each directory holds one guest: `guest.toml` names the guest, its image, the kernel it runs after
  provisioning, the pinned artifacts it takes and its provisioning phases, and each phase is a shell
  script the guest runs as root from a read only disk that carries the directory and the artifacts,
  with a reboot between two phases.

A guest is provisioned once, and the record of its definition and of the lock marks the disk, so a
changed file of its directory or a changed pin provisions it again.

A fresh worktree takes what another worktree of the same repository holds before it reaches the
network. A pinned file of the lock is copied by reflink from the guest cache of any other worktree
where a file of the same name and size stands, hashed in full against the lock and kept only where
its SHA-256 equals the lock's, and fetched from its URL only where no worktree holds it. A guest
whose record in another worktree names the same fingerprint of its definition and of the lock is
not provisioned again: its disk is copied by reflink and rebased onto the image of the local cache,
which the lock has checked, with `qemu-img rebase -u`, which rewrites the name of the backing file
alone. `SHARE_PEERS` in `guest.py` turns both off, and `provision --force` provisions anew.

Every run boots a fresh overlay of the provisioned disk. Commands run through the QEMU guest agent
over a Unix socket, the tree is mounted read only at `/mnt/tree` and `target/guests/<guest>/xfer`
at `/mnt/xfer`, and the private segment of a hub joins guests over Unix datagram sockets, so no
guest takes a port of the host.

From the `tests` directory:

```sh
python3 -m harness.homa.guest fetch          # fetch and check every pin
python3 -m harness.homa.guest provision A    # provision one guest
python3 -m harness.homa.guest check A B C    # boot them at once and join them on a hub
```

The host's own `qemu-system-aarch64`, `qemu-img` and edk2 firmware are taken where it has them, and
otherwise the pinned packages of the lock are unpacked into `target/guests/tools/`, so nothing is
installed on the host.
