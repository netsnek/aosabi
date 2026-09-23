# homa

The Homa transport protocol over any carrier that moves whole packets, from a UART to raw IP and
the Linux kernel.

Homa is a receiver driven RPC transport: every message states its length before its first byte,
the receiver grants the bytes beyond an unscheduled prefix, the shortest remaining message goes
first, and a lost packet comes back through RESEND. The crate carries it as a sans I/O engine,
driven by the packets that arrive, the packets it hands out and a clock value the caller passes,
over a carrier trait that any link implements, so the same engine serves a serial line, a USB
function, a Bluetooth channel, a Unix Socket and an IP network.

## What the crate holds

The crate grows in stages, H0 to H7, and the table names the stage at which each part lands. A
module whose part has not landed is a file that holds its header and the lines of its submodules
alone.

| Module | Holds | Lands at |
| --- | --- | --- |
| The root | The types every engine module shares, the traits `PeerPort`, `Carrier`, `Rpcs` and `Read`, the storage and the endpoint | H0, H1 the data centre mechanisms of the endpoint |
| `wire` | The two wire forms: the compact form, version 0 of the first version of the Homa design note, for point to point links, and the full form of `homa_wire.h` for Network | H0 the compact form, H1 the full form |
| `table`, `heap` | Open addressing over caller owned buckets with a keyed SipHash-1-3, and binary heaps over caller owned items | H0 |
| `siphash` | SipHash-1-3, the keyed hash of `table`, public so that another table hashes with the same one | H0 |
| `stream` | AoFrame, the framing of one packet on a byte stream, delimited with COBS and CRC-32C or prefixed with a four byte length | H0 |
| `grant`, `pacer`, `prio` | The grant scheduler, the pacers and the priority machinery | H0 the grant scheduler and the readiness of the pacer, H1 the rate and budget pacers and `prio` |
| `sim` | The loopback, lossy, baseline, fabric and radio simulators, behind the feature `sim` and never in a served build | H0 the loopback, lossy and baseline simulators, H1 the fabric, H6 the radio |
| `unix`, `ip`, `ksock` | The Unix Socket transports, the IP driver over raw IP of protocol 146, the `IPPROTO_HOMA` of HomaModule, and the client of the socket of the crate's kernel module, Linux only, behind their features | H2 `unix`, H4 `ip`, H5 `ksock` |
| `abi` | The socket interface of the crate's kernel module, which `ksock` and the module share, compiled in every build on every target | H5 |

The crate is `no_std`, allocation is optional, the roles `client` and `server` are features, and
`#![forbid(unsafe_code)]` holds in every build cargo makes. The engine keeps edition 2021 and rustc
1.85 and uses no floating point and no atomic type, so the same files build on an RV32IMC core
without atomics and, from stage H5, as a Rust for Linux kernel module under `kernel/`.

## The full form and HomaModule

The full form is `homa_wire.h` of [HomaModule] at commit `50d65305aeef9b6ae5cf193a808c7e95f9b1dcb3`,
the pin every capture of the full form is judged against. It was checked against the head of `main`
twice, against `bfef3bcf114dc2c230d87d39cdd3fd24b1a21ce1`, read and nine commits later, and against
`1c59d7b65b4cf2640be94c368f1c43cd45479dc1`, read and sixteen commits later, and neither head changes
`homa_wire.h`. The pin moves only by a change that runs `05_homa_wire_full` and the notebooks of
the raw socket and of HomaModule against the new commit.

## Where the facts come from

The Homa design note, in a first version `homa/design-v1.md` and a second `homa/design.md`, is the
design the crate follows, and it lives in a design folder outside this repository.

## How the crate is judged

The crate carries no test in Rust. Jupyter notebooks judge it, each from the stage of the part it
judges, through a command line seam against references in Python that share no code with the
crate: `03_homa_packets` and `04_aoframe` for the byte formats, `21_homa_loopback` and
`22_homa_lossy` for the engine, `23_homa_budget` for the budget of every target,
`05_homa_wire_full` and `29_homa_fabric` for the full form and the data centre mechanisms, and two
notebooks of the project that run outside this repository, over the raw socket and over
HomaModule, for Network.

## Scope

The crate names nothing of the project it grew in. It knows no application protocol above Homa, so
a subtree split of this directory is a crate of its own.

## Licence

MIT, see [LICENSE](LICENSE).

[HomaModule]: https://github.com/PlatformLab/HomaModule
[homa.h]: https://github.com/PlatformLab/HomaModule/blob/50d65305aeef9b6ae5cf193a808c7e95f9b1dcb3/homa.h#L16
[compare-bfef3bcf]: https://api.github.com/repos/PlatformLab/HomaModule/compare/50d65305aeef9b6ae5cf193a808c7e95f9b1dcb3...bfef3bcf114dc2c230d87d39cdd3fd24b1a21ce1
[compare-1c59d7b6]: https://api.github.com/repos/PlatformLab/HomaModule/compare/50d65305aeef9b6ae5cf193a808c7e95f9b1dcb3...1c59d7b65b4cf2640be94c368f1c43cd45479dc1
