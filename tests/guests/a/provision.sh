# SPDX-License-Identifier: MIT
#
# Provisions guest A, run as root by tests/harness/homa/guest.py from the read
# only disk that carries this directory and the pinned artifacts of
# tests/guests/guests.lock, whose SHA-256 the harness checked inside the guest
# before it started this script.
#
# The image carries kernel-core and kernel-modules-core, whose modules hold
# 9p, veth, the queuing disciplines and the gadget functions, and leaves out
# kernel-modules with Bluetooth and hci_vhci, which the script installs.
# Fedora builds no dummy_hcd, so the script builds it out of tree from the
# stable source of the kernel's tag against the kernel's own build tree. The
# rustc is the one the kernel's .config names in CONFIG_RUSTC_VERSION_TEXT.
# Fedora packages no btvirt, the emulator of BlueZ that runs virtual
# controllers over hci_vhci, so the script builds it from the source of the
# release the repositories install, and nothing else of that source.

set -eu

kernel=6.17.1-300.fc43.aarch64
test "$(uname -r)" = "$kernel"

dnf -y --setopt=install_weak_deps=False install \
    ./kernel-modules-6.17.1-300.fc43.aarch64.rpm \
    ./kernel-devel-6.17.1-300.fc43.aarch64.rpm \
    ./rust-1.90.0-1.fc43.aarch64.rpm \
    ./rust-std-static-1.90.0-1.fc43.aarch64.rpm \
    ./clippy-1.90.0-1.fc43.aarch64.rpm \
    iproute-tc bluez tcpdump nftables make gcc python3 glib2-devel dbus-devel pkgconf-pkg-config

src=/usr/local/src/dummy_hcd
mkdir -p "$src"
cp dummy_hcd-v6.17.1.c "$src/dummy_hcd.c"
printf 'obj-m := dummy_hcd.o\n' > "$src/Kbuild"
make -C "/usr/src/kernels/$kernel" M="$src" modules
install -D -m 0644 "$src/dummy_hcd.ko" "/lib/modules/$kernel/extra/dummy_hcd.ko"
depmod -a "$kernel"

modprobe dummy_hcd
ls /sys/class/udc
modprobe -r dummy_hcd
modprobe 9pnet_virtio
modprobe hci_vhci
test -c /dev/vhci
modprobe -r hci_vhci

bluez=/usr/local/src/bluez
mkdir -p "$bluez"
tar -xf bluez-5.87.tar.xz -C "$bluez" --strip-components=1
(cd "$bluez" && ./configure --enable-testing --disable-obex --disable-client --disable-mesh \
    --disable-cups --disable-manpages --disable-udev --disable-systemd --disable-datafiles \
    --disable-monitor --disable-tools > configure.log 2>&1 && make emulator/btvirt > make.log 2>&1) || {
    tail -n 40 "$bluez/configure.log" "$bluez/make.log" 2>/dev/null
    exit 1
}
install -D -m 0755 "$bluez/emulator/btvirt" /usr/local/bin/btvirt
rpm -q --qf '%{VERSION}\n' bluez | grep -q '^5\.87'
/usr/local/bin/btvirt --version
modprobe hci_vhci
/usr/local/bin/btvirt -l2 > /dev/null 2>&1 &
btvirt=$!
for i in $(seq 50); do [ -e /sys/class/bluetooth/hci1 ] && break; sleep 0.1; done
ls /sys/class/bluetooth
kill "$btvirt"
wait "$btvirt" || true
modprobe -r hci_vhci

required=$(sed -n 's/^CONFIG_RUSTC_VERSION_TEXT="\(.*\)"$/\1/p' "/usr/src/kernels/$kernel/.config")
installed=$(rustc --version)
echo "guest A: the kernel names $required, the guest has $installed"
test "$installed" = "$required"
tc -V
/usr/libexec/bluetooth/bluetoothd --version
tcpdump --version | head -1
echo "guest A: provisioned"
