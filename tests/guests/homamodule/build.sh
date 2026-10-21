# SPDX-License-Identifier: MIT
# Builds HomaModule at 50d65305 in guest C, the second phase, on the kernel
# 6.17.8-300.fc43 the first phase installed: the module with its own
# Makefile against the kernel's build tree and the tools of util/, as the
# module's INSTALL.md describes, then loads the module, reads its protocol
# from /proc/net/protocols and unloads it again.
#
# A module that does not build or load fails the phase, and util tools that do
# not build leave the guest with the module alone.

set -eu

kernel=6.17.8-300.fc43.aarch64
commit=50d65305aeef9b6ae5cf193a808c7e95f9b1dcb3
test "$(uname -r)" = "$kernel"

src=/usr/local/src
rm -rf "$src/HomaModule-$commit" "$src/HomaModule"
tar -xzf "HomaModule-$commit.tar.gz" -C "$src"
mv "$src/HomaModule-$commit" "$src/HomaModule"
cd "$src/HomaModule"

fallback() {
    echo "homamodule: $1 on aarch64 under $kernel"
    echo "homamodule: fallback of risk 3, the interop halves of 29_homa_raw and 29_homa_kmod run on an x86-64 host the owner names"
}

if ! make all KDIR="/usr/src/kernels/$kernel" > /var/log/homamodule-build.log 2>&1; then
    tail -n 60 /var/log/homamodule-build.log
    fallback "the module does not build"
    exit 1
fi
tail -n 5 /var/log/homamodule-build.log
modinfo ./homa.ko | grep -E '^(filename|vermagic|license):'
echo "homamodule: homa.ko built at $commit"

if make -C util > /var/log/homamodule-util.log 2>&1; then
    ls util/cp_node util/homa_prio util/server util/homa_test
    echo "homamodule: util built"
else
    grep -m 3 ': error:' /var/log/homamodule-util.log || true
    fallback "the util tools do not build"
fi

if ! insmod ./homa.ko; then
    fallback "the module does not load"
    exit 1
fi
grep -i '^homa' /proc/net/protocols || true
lsmod | grep '^homa '
rmmod homa
echo "homamodule: homa.ko loaded and unloaded under $kernel"
