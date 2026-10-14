// SPDX-License-Identifier: MIT
/*
 * aosabi-cli: the table of the built in cards.
 *
 * No analogue in the reference. The rows are those of docs/cli-and-targets.md
 * section 5.0.
 */

//! The table of the built in cards.
//!
//! | Card | Class | Carriers and protocols | Compiles | Target and profile |
//! | --- | --- | --- | --- | --- |
//! | `server` | server | Network with HTTP(TCP), Unix Socket with HTTP(Unix Socket) | the generated server package with `serve`, `unix-socket`, `deflate` and `linux`, with the lean link and packed relocations in a release build for `aarch64-unknown-linux-gnu` and `x86_64-unknown-linux-gnu` by the linker of the host, and in every release build a copy stripped by the `rust-objcopy` of the toolchain in `target/aosabi/dist/`, the binary it ships | the host, dev or release under `--release` |
//! | `server-legacy` | server | as `server` | as `server`, with the lean link and without packed relocations in such a build, for a loader older than glibc 2.36 | as `server` |
//! | `docker` | container | as `server` | as `server` on the host with no copy in `target/aosabi/dist/` and with no flag of the environment, as the builder stage passes none, and again in the builder stage of the image, whose release build for `aarch64-unknown-linux-gnu` takes the lean link and packed relocations by the LLD of the toolchain and whose binary the `rust-objcopy` of the toolchain strips before the image copies it | the host, release |
//! | `worker` | worker | Network with HTTP(TCP), served by the platform | the library of the service with the facade's `platform-coding` alone | `wasm32-unknown-unknown`, as `server` |
//! | `esp32c3-devkitm1` | mcu | USB, Serial Line and Bluetooth with GraphTP(Homa) | `aosabi-kernel` without its default features and `aosabi-http` from the workspace of a path facade | `riscv32imc-unknown-none-elf`, as `server` |
//!
//! The encoding of every card is JSON. On HTTP it is coded identity or
//! deflate, and on GraphTP(Homa) deflate in the zlib format from 128 bytes on, which is no axis of a card, so the table
//! carries the one value. The `esp32c3-devkitm1` row is its default image, and
//! its benchmark variant `http-bench`, which serves HTTP(TCP) on Network, is
//! the benchmark harness's and no product of `cargo aosabi build`.
//!
//! The field `source` holds the set a release build takes at the switch
//! `AOSABI_STD` at `source` ([`crate::std_source`]).

use crate::{
    cards::{
        Builds,
        Card,
        Carrier,
        Class,
        Encoding,
        Profile,
        Protocol, //
    },
    generate::{
        LINUX_FEATURES,
        ServerPackage, //
    },
    std_source::LINUX_SET,
};

/// The carriers of the cards that serve on Linux.
const LINUX_CARRIERS: &[(Carrier, &[Protocol])] = &[
    (Carrier::Network, &[Protocol::HttpTcp]),
    (Carrier::UnixSocket, &[Protocol::HttpUnixSocket]),
];

/// The server package of `server` and `docker`, whose release build for
/// `aarch64-unknown-linux-gnu` asks for packed relocations, which the linker
/// of the host links for both where it takes the request, as the GNU ld 2.45
/// of Fedora 43 does, and the LLD of the toolchain links in the builder stage
/// of the image of `docker`, whose GNU ld 2.40 ignores it.
const SERVER_PACKAGE: ServerPackage = ServerPackage {
    features: LINUX_FEATURES,
    linux_card: "Server",
};

/// The server package of `server-legacy`, whose release build links
/// without packed relocations.
const SERVER_LEGACY_PACKAGE: ServerPackage = ServerPackage {
    features: LINUX_FEATURES,
    linux_card: "ServerLegacy",
};

/// The built in cards in the order of the table.
pub const BUILTIN: [Card; 5] = [
    Card {
        name: "server",
        class: Class::Server,
        triple: None,
        profile: Profile::Dev,
        carriers: LINUX_CARRIERS,
        encoding: Encoding::Json,
        builds: Builds::ServerPackage(SERVER_PACKAGE),
        serves: true,
        source: Some(LINUX_SET),
    },
    Card {
        name: "server-legacy",
        class: Class::Server,
        triple: None,
        profile: Profile::Dev,
        carriers: LINUX_CARRIERS,
        encoding: Encoding::Json,
        builds: Builds::ServerPackage(SERVER_LEGACY_PACKAGE),
        serves: true,
        source: Some(LINUX_SET),
    },
    Card {
        name: "docker",
        class: Class::Container,
        triple: None,
        profile: Profile::Release,
        carriers: LINUX_CARRIERS,
        encoding: Encoding::Json,
        builds: Builds::ServerPackage(SERVER_PACKAGE),
        serves: true,
        source: Some(LINUX_SET),
    },
    Card {
        name: "worker",
        class: Class::Worker,
        triple: Some("wasm32-unknown-unknown"),
        profile: Profile::Dev,
        carriers: &[(Carrier::Network, &[Protocol::HttpTcp])],
        encoding: Encoding::Json,
        builds: Builds::AppForTarget {
            features: &["aosabi/platform-coding"],
        },
        serves: false,
        source: None,
    },
    Card {
        name: "esp32c3-devkitm1",
        class: Class::Mcu,
        triple: Some("riscv32imc-unknown-none-elf"),
        profile: Profile::Dev,
        carriers: &[
            (Carrier::Usb, &[Protocol::GraphtpHoma]),
            (Carrier::SerialLine, &[Protocol::GraphtpHoma]),
            (Carrier::Bluetooth, &[Protocol::GraphtpHoma]),
        ],
        encoding: Encoding::Json,
        builds: Builds::KernelForTarget {
            packages: &[("aosabi-kernel", true), ("aosabi-http", false)],
        },
        serves: false,
        source: None,
    },
];
