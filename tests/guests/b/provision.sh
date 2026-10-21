# SPDX-License-Identifier: MIT
#
# Provisions guest B, run as root by tests/harness/homa/guest.py from the read
# only disk that carries this directory. The image carries kernel-core and
# kernel-modules-core, whose modules hold 9p and the queuing disciplines, so
# guest B takes no pinned artifact beyond the image, and its tools come
# signed from the Fedora repositories.

set -eu

kernel=6.17.1-300.fc43.aarch64
test "$(uname -r)" = "$kernel"

dnf -y --setopt=install_weak_deps=False install iproute-tc tcpdump nftables python3

modprobe 9pnet_virtio
tc -V
tcpdump --version | head -1
echo "guest B: provisioned"
