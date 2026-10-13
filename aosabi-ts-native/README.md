# aosabi-ts-native

This folder holds the four platform packages of the npm package `aosabi`, one per Linux target of
the native library of the Aosabi engine. Each carries the library `libaosabi.so`, the crate
`aosabi-js` of the Aosabi workspace built as a `cdylib` from the code of `core.wasm`, which
`aosabi/bun` loads through `dlopen` of `bun:ffi` in place of `core.wasm`.

| Package | Rust target | `os` | `cpu` | `libc` |
| --- | --- | --- | --- | --- |
| `aosabi-linux-x64-gnu` | `x86_64-unknown-linux-gnu` | `linux` | `x64` | `glibc` |
| `aosabi-linux-arm64-gnu` | `aarch64-unknown-linux-gnu` | `linux` | `arm64` | `glibc` |
| `aosabi-linux-x64-musl` | `x86_64-unknown-linux-musl` | `linux` | `x64` | `musl` |
| `aosabi-linux-arm64-musl` | `aarch64-unknown-linux-musl` | `linux` | `arm64` | `musl` |

The package `aosabi` names the four as its `optionalDependencies`, so a package manager installs the
one whose `os`, `cpu` and `libc` match the machine and skips the others. The app imports from
`aosabi` alone and keeps its source, and `aosabi/bun` falls back to `core.wasm` where no platform
package is installed. The variable `AOSABI_ENGINE` of the environment picks the engine: `wasm`
forces `core.wasm` for a comparison, and `native` asks for the native library and fails where none
loads.

Each folder holds the `package.json` and the licence of its package. The library is a build product
and never enters the repository: the harness of the repository builds it with
`cargo build --locked --offline -p aosabi-js --features native --target <target> --profile release`
under the `RUSTFLAGS` that `native_rustflags` of `tests/harness/ts1.py` writes for the target, and
links it into a staged copy of the package under `target/` (`stage_native` of the same file). A
target of musl builds with `-C target-feature=-crt-static` and links through `rust-lld` with
`-C link-self-contained=yes` over the start files the toolchain carries for musl and the stubs
`libc.so` and `libgcc_s.so` that `llvm-ifs` writes under `target/harness/ts1/native/stubs/`, since
the compiler driver of the host links the C library of glibc. On an aarch64 host
`x86_64-unknown-linux-gnu` links with `x86_64-linux-gnu-gcc` over the sysroot
`/usr/x86_64-linux-gnu/sys-root`, and `aarch64-unknown-linux-gnu` links with the compiler driver of
the host. Every package is `private` at version `0.0.0` and the owner alone publishes, as with
`aosabi`. They are the project's own packages and hold no code from outside the project, so `aosabi`
still depends on no npm package outside the project.
