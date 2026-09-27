# graphtp-homa

GraphTP over Homa: the binding of the crate `graphtp` to the crate `homa`, as hyper stands beside
`http`. One Homa RPC carries one exchange, a stream is served by one parked PULL per peer with
batches numbered by `seq`, and a client that cannot know the outcome of an exchange says so.

The modules `server`, `handler` and `client` are sans I/O: they decide every answer and every
message and make no call of Homa, so a server, a client and a notebook drive them with the calls of
the engine they have. The module `rpcs` is the one adapter that calls Homa.

## The build

| Property | Value |
| --- | --- |
| Standard library | None, `#![no_std]` with `alloc`, so it builds for `riscv32imc-unknown-none-elf` and `wasm32-unknown-unknown` |
| Dependencies | `graphtp` and `homa`, the owner's crates, and no other |
| Features | `server` over the server role of `homa`, `client` over its client role, both by default |
| `unsafe` | None, `#![forbid(unsafe_code)]` |
| Edition and lowest Rust | Those of the workspace, edition 2024 and Rust 1.95.0 |
| Tests | None in Rust. The judge in this repository is the Jupyter notebook `tests/notebooks/25_carrier_unix.ipynb`, and a notebook of the project that runs outside it holds the limits (section What the judges measured) |
| Licence | MIT |
| Publishing | `publish = false`, since publishing is the owner's step, once `homa` has a release |

## The modules

| Module | Holds |
| --- | --- |
| `server` | `Server<P>`, keyed by the peer type of the caller's `homa`, with `Config`, `Step`, `Exchange`, `Operation` and `Flow` |
| `handler` | `Handler<P>`, the one trait a GraphQL executor serves through, which a closure implements as well |
| `client` | `Client` with `Config`, `Link`, `Repeat`, `Outgoing`, `Incoming` and `Event` |
| `rpcs` | `serve` over `homa::Rpcs`, `serve_endpoint` over `homa::Endpoint` with the server role alone, and `send` and `drive` for a client over `homa::Rpcs` |

## A server

`Server::request` takes each whole request with its peer and its RPC. It checks the 11 rules of
section 5.3 in their order through `graphtp::wire`, answers a refusal with its 2 bytes, answers a
PULL with the batch of the peer's ready entries or parks it, refuses a second PULL beside a parked
one with 409.1 and a PULL of a peer with no stream with 404.1, and answers a CANCEL with one
`cancelled` entry per stream it ends. A CANCEL that ends the peer's last stream first answers the
peer's parked PULL with 404.1. An OPERATION that passes goes to the handler as an `Exchange` with
its credential and its decoded content.

The handler answers with `Server::answer`, or opens a stream with `Server::open_stream`, which
answers 200.3 with the head of the stream, and hands the stream's results to `Server::next`, its
end to `Server::complete` and a failure to `Server::aborted`, at once or later. Each stream has a
bounded queue: `next` returns `Flow::Wait` when the queue is full, so the source waits, and a result
that finds it full ends the stream with `aborted`. `Server::poll` yields every answer to send, and
a batch for each parked PULL whose peer has an entry ready, with every ready entry of every stream
of the peer that fits the answer buffer, in the order they became ready. `Server::link_down` ends
a peer, its streams, its parked PULL and its count of `seq`, and `Server::poll_ended` names each
stream a CANCEL or a link down ended while its source was read.

```rust,ignore
use graphtp::zlib;
use graphtp_homa::{rpcs, server::{Config, Server}};

let mut server = Server::new(Config {
    max_credential: 1_024,
    max_content: 4_096,
    streams_per_peer: 4,
    answer_buffer: 8_192,
    queue: 16,
    peers: 8,
    coding: zlib::PARAMS,
})?;
let mut handler = |exchange, _credential: Option<&[u8]>, content: &[u8], server: &mut Server<_>| {
    let body = execute(content);
    server.answer(exchange, graphtp::status::Code::OK, &body);
};
loop {
    endpoint.pump(&mut carrier, now(), &mut rx)?;
    rpcs::serve_endpoint(&mut endpoint, &mut server, &mut handler);
    endpoint.pump(&mut carrier, now(), &mut rx)?;
}
```

## A client

`Client::operation` writes an OPERATION with the repeat mark of its caller. An OPERATION that is
not marked sets `report_unknown` on its Homa request, so `RPC_UNKNOWN` reaches the client, which
reports the outcome as unknown, and a marked one keeps Homa's restart and is handed back to be sent
again after a dead peer. `Client::poll` yields the CANCEL of handle 0 of the start rule, on a link
whose end the server may not see, and a PULL while a stream is open, and `Client::answer` applies
stream answers in the order of `seq`, holding one that is ahead until the gap fills. A PULL that
meets 404.1 while the client holds a stream it has not cancelled means the server forgot the peer
and its count, so the client drops the answers it holds, ends a stream one of them opened as opened
and lost, and counts on from the answer of the CANCEL of handle 0 that `Client::poll` then yields.

```rust,ignore
use graphtp::zlib;
use graphtp_homa::{client::{Client, Config, Link, Repeat}, rpcs};

let mut client = Client::new(Config { max_body: 1 << 20, link: Link::Seen, coding: zlib::PARAMS })?;
let outgoing = client.operation(Some(b"Bearer eyJ..."), br#"{"query":"{__typename}"}"#, Repeat::No)?;
rpcs::send(&mut rpcs, peer, &mut client, outgoing)?;
let mut out = Vec::new();
rpcs::drive(&mut rpcs, peer, &mut client, &mut out)?;
```

## What the judges measured

The server of the crate runs under the GraphTP listener of `aosabi-core` as the card `test-graphtp`
over a real Unix Socket, on a `SOCK_SEQPACKET` socket and on the stream socket of the prefixed
framing, with `tests/harness/homa/graphtp.py` over the Python Homa engine as its client. The server
rows of `25_carrier_unix` hold on both forms: the sizes of the exchanges of `docs/graphtp.md`
section 6.2, one parked PULL serving ten streams, 409.1 on a second PULL, the stream answers applied
by `seq` in 200 orders with 0 lost events, CANCEL of the peer's only handle, of its last handle and
of handle 0 answering the parked PULL with 404.1 first while CANCEL of one handle of two leaves it
parked, a restarted client taking its count from CANCEL of handle 0, the restart cases of section
2.7 with 0 double runs of an unmarked OPERATION and every unknown outcome reported, 429.1 at the
bound, back pressure with its queue and a link down ending every stream of the peer and, on one
listener whose link goes down and up again, setting the peer's count back to 0. A notebook of the
project that runs outside this repository holds the bounds of `Config` at their value and one past
it, 413.0, 431.0, 429.1 and 500.1, and 2,000 hostile messages a card with 0 panics. The client rows of `25_carrier_unix`
join with `aosabi-link`, the client the bridge runs over.
