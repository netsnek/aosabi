# SPDX-License-Identifier: MIT
#
# Provisions guest C, the first phase, run as root by
# tests/harness/homa/guest.py from the read only disk that carries this
# directory and the pinned artifacts of tests/guests/guests.lock, whose
# SHA-256 the harness checked inside the guest before it started this
# script. It installs the koji kernel 6.17.8-300.fc43 with its modules and
# its build tree and makes it the default, and the harness reboots the guest
# into it before the second phase, build.sh.

set -eu

kernel=6.17.8-300.fc43.aarch64

dnf -y --setopt=install_weak_deps=False install \
    ./kernel-core-6.17.8-300.fc43.aarch64.rpm \
    ./kernel-modules-core-6.17.8-300.fc43.aarch64.rpm \
    ./kernel-modules-6.17.8-300.fc43.aarch64.rpm \
    ./kernel-devel-6.17.8-300.fc43.aarch64.rpm \
    make gcc gcc-c++ tcpdump iproute-tc nftables python3

grubby --set-default "/boot/vmlinuz-$kernel"
test "$(grubby --default-kernel)" = "/boot/vmlinuz-$kernel"
echo "guest C: kernel $kernel installed as the default"
