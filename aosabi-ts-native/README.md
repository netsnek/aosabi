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

The four are released through the repository of Aosabi alone, so the package `aosabi` names none of
them. An app installs the package of its machine from a checkout of the Aosabi repository by the
path of its folder here, or as the tarball `npm pack` makes of that folder, once the library is
built and copied into the folder as `libaosabi.so`, beside the packages `aosabi` and `aosabi-build`
from the same checkout. On an arm64 machine with glibc the checkout builds the library and copies it
into the folder:

```sh
cargo build --locked -p aosabi-js --features native --profile release \
  --target aarch64-unknown-linux-gnu
cp target/aarch64-unknown-linux-gnu/release/libaosabi_js.so \
  aosabi-ts-native/aosabi-linux-arm64-gnu/libaosabi.so
```

The app then installs that folder by its path:

```sh
npm install <checkout>/aosabi-ts-native/aosabi-linux-arm64-gnu
```

The entry `aosabi/bun` opens the library only where the `package.json` of the platform package
carries the version of `aosabi`. That holds the two packages to one version and catches a platform
package installed as the tarball of another version, and it sees no stale library: a folder
installed by its path moves with the checkout, so there only the build and copy of the library
after every update of the checkout keeps it current, since nothing in the library names the commit
it was built from. Where the two versions differ, the app installs `aosabi` and the platform package
from the same checkout again.

The entry `aosabi/bun` looks for the package in two folders alone and in no folder above them: the
folder `node_modules` that holds `aosabi`, where an install of the tarballs puts the two side by
side, and the folder `node_modules` of the root of the app, where the command above installs it
while `aosabi` is a link into a checkout, whose files Bun loads from the checkout, or into the store
of a package manager such as pnpm. It takes the root of the app from the entry Bun runs and never
from the working directory: for an entry in a folder `.aosabi` the folder that holds it, for any
other entry the folder of the entry, and for an entry Bun runs from no file, such as the code of
`bun -e`, none, so the entry reads the first folder alone. So a package of that name elsewhere on
the machine never loads, and an app whose `aosabi` is a link starts from an entry in its folder
`.aosabi` or at its root. The command `bun --preload aosabi/bun .aosabi/server.mjs` starts in the
folder of the app, since Bun resolves the preloaded entry from the working directory.

Under npm workspaces npm installs the packages of every workspace into the folder `node_modules` of
the root of the workspaces, which the entry reads only as the folder that holds `aosabi`. So an app
in such a workspace installs `aosabi` as its tarball, which npm puts beside the platform package
there, or keeps the platform package in its own folder `node_modules`, where
`npm install --install-strategy=nested` puts it.

The app imports from `aosabi` alone and keeps its source, and `aosabi/bun` falls back to `core.wasm`
where no platform package is installed, where the folder of the platform package holds no library
and where the platform package carries another version than `aosabi`, with a reason that names the
folder and the cause. The variable `AOSABI_ENGINE` of the environment picks the engine: `wasm`
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
the host. Every package is `private` at version `0.1.1`. They are the project's own packages and
hold no code from outside the project, so `aosabi` still depends on no npm package outside the
project.
