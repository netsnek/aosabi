# Embedded targets

Aosabi is built to run where a GraphQL server usually does not fit: on small Linux devices and on
boards with no operating system. Rust is the language of a service on these targets, and the layers
every build links are written for the smallest of them first. This page says what builds and runs on
each kind of device in this release, and what is still in progress on the
[roadmap](../../roadmap/embedded.md).

## 1. What runs where

| Target | Triple | In this release |
| --- | --- | --- |
| Linux servers and containers | The host, `aarch64` or `x86_64` | Builds and runs |
| Small Linux devices | `mipsel-unknown-linux-musl` | Builds as one static binary |
| The ESP32-C3 | `riscv32imc-unknown-none-elf` | The core crates build |
| RISC-V cores with atomics | `riscv32imac-unknown-none-elf` | Also the types and validation |

A build for a board is the build of the crates alone today. No image of the engine runs on a board
in the test suite yet, and no number on this page was measured on one.

## 2. Small Linux devices

A small Linux device, such as a portable audio player on a MIPS core, takes the same native server
as a Linux server, built as one static binary for `mipsel-unknown-linux-musl`. The target has no
prebuilt standard library, so the build compiles it from source on the pinned stable toolchain, with
soft float for a core without a floating point unit. The profile `player` of the workspace builds
for size and strips the symbols. The request cache of such a build keeps at most 512 KiB of parsed
documents, where a server keeps 64 MiB.

The suite of the card builds, which runs in the project's own pipeline, builds the server package of
a service for this target. A card that builds it from the command line comes with the device cards
of the roadmap.

## 3. Boards with no operating system

The built in card `esp32c3-devkitm1` names the ESP32-C3, a RISC-V core without atomic operations and
with a few hundred kilobytes of RAM. Its build today compiles the kernel crate and the HTTP byte
layer for `riscv32imc-unknown-none-elf`. Beside them these crates build for that target with no
standard library and no atomic operation:

- [crates/aosabi-kernel](../../crates/aosabi-kernel/), the framing of packets on a byte stream, the
  publish primitive of server push and the entry traits, with allocation optional.
- [crates/aosabi-json](../../crates/aosabi-json/), the writer of JSON bytes with its own writer of
  doubles and its own insertion ordered map.
- [crates/graphtp](../../crates/graphtp/), the codec of GraphTP with its encoder and decoder.
- [crates/homa](../../crates/homa/), the Homa transport with the compact form of the device
  carriers.
- The request cache of `aosabi-cache` and the authorization contract of `aosabi-auth-contract`.

The type system and the validation build for `riscv32imac-unknown-none-elf`, the same family of
cores with atomic operations, and so does the core of the single pass, which names `core` and
`alloc` alone. On the ESP32-C3 their build stops at the shared pointers and the cells that need
atomic operations of pointer width, which that core lacks, and the work that builds the engine on
such cores is the next step of the roadmap.

## 4. Why GraphTP on a device

A device is meant to answer GraphQL over GraphTP on the links it has, a serial line, USB serial and
Bluetooth LE, with the same request a client sends over HTTP. The heads of GraphTP take 3 bytes on a
request and 2 on an answer, where HTTP/1.1 writes a request line and its header fields as text, and
the code is small:

| What | Bytes | Suite, day |
| --- | ---: | --- |
| The two heads of GraphTP with the mapping of the status | 364 | The GraphTP suite, 2026-09-27 |
| The codec of GraphTP with its encoder and decoder | 19,264 | The GraphTP suite, 2026-09-27 |
| The single pass with its table and its lexer | 44,043 | The single pass budget, 2026-10-20 |
| The HTTP/1.1 server path of the native card | 97,796 | The ESP32-C3 benchmark, 2026-10-02 |

Every row counts bytes of code and read only data built for size, for `riscv32imc-unknown-none-elf`
in the first, second and last row and for `riscv32imac-unknown-none-elf` in the third. The first two
rows read the object file of the crate, and the last two read an image. The last row is the server
path that a benchmark build of the card links to compare the two protocols, and the default image of
the card links no HTTP server. The suites behind the table use parts of the test harness that this
repository does not carry, so they run in the project's own pipeline.

## 5. Writing a service for a device

A service for a device is the same plain Rust as a service for a server, with the limits of the
target in mind.

- The engine starts no thread on any target, so a service written for a server needs no change for a
  target with one core.
- A part of the facade that needs an operating system stands under `cfg(unix)`, such as the file
  server `serve_static` and the serving plugin, and a service writes its call of such a part under
  `#[cfg(unix)]` as well.
- The outbound HTTP call `aosabi::fetch` answers an error on a target with no network driver, so a
  service can name it and still build for a board.
- The single pass bounds nesting at 64 levels by default, and `AOSABI_SINGLE_PASS_DEPTH` sets the
  bound of a build from 1 to 1,024, which also bounds the stack the walk takes.

Parts with about 2 KB of RAM, such as a classic Arduino, stay outside: a parser, a validator and a
JSON codec do not fit there.
