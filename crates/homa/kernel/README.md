# The kernel module layer of homa

Stage H5 of the Homa track puts in this directory the Rust for Linux kernel module that runs the
engine of the crate in the kernel. Until H5 lands, the directory holds this README alone.

Kbuild builds the module H5 adds with `make -C <kernel build tree> M=$(PWD) LLVM=1` and cargo never
does, so every build cargo makes keeps `#![forbid(unsafe_code)]`. The module compiles the engine
files of `../src/` under the same module names, with the Network profile's features as `--cfg`
flags, and its one file with `unsafe`, `glue.rs`, binds the engine to the kernel's protocol
handlers, sockets and timers. It carries Homa and nothing above it.

The module never runs beside HomaModule or the raw path of the crate's IP driver on one host, since
all three answer IP protocol 146, the `IPPROTO_HOMA` of HomaModule.

[homa.h]: https://github.com/PlatformLab/HomaModule/blob/50d65305aeef9b6ae5cf193a808c7e95f9b1dcb3/homa.h#L16
