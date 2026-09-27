# graphtp

GraphTP, the Graph Transfer Protocol: the types and the wire codec of GraphQL requests and answers
over a message transport. The crate is a protocol crate in the shape of the crate `http`: it holds
the types of GraphTP and the codec of its wire form GraphTP/1, and nothing that moves a byte. The
binding to Homa is the crate `graphtp-homa`, as hyper stands beside `http`.

The guide to GraphTP is [Documentation/guides/graphtp.md](../../Documentation/guides/graphtp.md).
This page says what the crate holds, how a server and a client use it, and what its judges
measured.

## The build

| Property | Value |
| --- | --- |
| Standard library | None, `#![no_std]` with `alloc`, so it builds for `riscv32imc-unknown-none-elf` |
| Dependencies | None in any build |
| Features | None. The DEFLATE encoder and decoder in the zlib format are always part of the crate, dynamic blocks and lazy matching included |
| `unsafe` | None, `#![forbid(unsafe_code)]` |
| Edition and lowest Rust | Edition 2024, Rust 1.95.0 |
| Tests | No test in Rust. Two notebooks of the project judge the codec and its content coding, and they run outside this repository |
| Licence | MIT |
| Publishing | `publish = false`, since publishing is the owner's step |

## The wire form in brief

A request is a head of 3 bytes, the credential and the content. The first byte holds the version in
bits 7 and 6, `01` for GraphTP/1, the kind in bits 5 to 3, OPERATION, PULL or CANCEL, the compressed
bit C and two reserved bits, and the next 2 bytes hold the length of the credential. The credential
is the value of `Authorization`, every byte `0x20` to `0x7E`, never compressed. The content of an
OPERATION is the body of a GraphQL over HTTP POST, and a CANCEL carries a handle of 2 bytes, 0 for
every stream of the peer.

An answer is a head of 2 bytes, the status byte `64 * (status div 100 - 2) + status mod 100` and the
substatus, then the body. An empty body is a refusal whatever its code, but for 200.3 and the 200.0
of a PULL or a CANCEL, which are malformed without the head of a stream or the batch they carry, and
a body opens with the brace or the bracket of plain JSON or with the header of one zlib stream.

A sender codes exactly the contents and bodies of more than 128 bytes, >128 B, into one zlib stream
with a window of 4 KiB and sends every one of at most 128 bytes plain, and a receiver reads the
coding from the compressed bit of a request or from the first byte of a body.

## The modules

| Module | Holds |
| --- | --- |
| `version` | `Version`, the two bits of a request head, with `00` refused and `11` kept |
| `kind` | `Kind`, OPERATION, PULL and CANCEL, and the unassigned values 3 to 7 |
| `status` | `Status` with its byte and its reason phrase, `Substatus`, and `Code` with the 35 constants of the registry and `name()` |
| `head` | `RequestHead` of 3 bytes and `AnswerHead` of 2 |
| `credential` | `Credential`, checked for its bytes and its length |
| `body` | `THRESHOLD`, 128, the longest plain content or body, `Coding` and `coding_of`, the first byte rule |
| `sink` | `Sink`, where the writers put bytes, over a vector, a slice or pages the caller owns |
| `deflate` | The DEFLATE core of RFC 1951, the streaming encoder in the caller's scratch and the decoder, whole or through the caller's window, each with a bound |
| `zlib` | The zlib profile: `check_header`, `Scratch`, `Encoder`, `Decoder` with the decoded bound, `Error` with its `Code`, and Adler-32 |
| `wire` | `decode_request`, `decode_content`, `decode_answer`, `decode_body` and the writers of every message |
| `stream` | `Handle`, `Target`, `Seq`, the writers of the stream head, the batch and the entries, and `Entries`, the reader of a plain stream body with no allocation |
| `request`, `response` | `Request<B>` of an OPERATION and `Response<B>`, generic over the body, each with `Parts` and a `Builder` |

## A server

A server reads a request in two calls, which answer the 11 rules of refusal of the specification,
section 5.3, in their order, each with its `Code`.
`wire::decode_request` checks rules 1 to 9 on the message, and `wire::decode_content` checks rules
10 and 11 as it decodes the content into the caller's sink under the decoded bound. A refusal is its
2 bytes, and an answer of the service is coded at >128 B through a scratch the server makes once.

```rust,ignore
use graphtp::{wire, zlib};

let mut scratch = zlib::Scratch::new(zlib::PARAMS)?;
let mut answer = Vec::new();
match wire::decode_request(&message, max_credential) {
    Ok(wire::Request::Operation { credential, content }) => {
        let mut plain = Vec::new();
        match wire::decode_content(content, max_content, &mut plain) {
            Ok(_) => {
                let (code, body) = service(credential, &plain);
                wire::write_answer(&mut answer, code, &body, &mut scratch)?;
            }
            Err(code) => wire::write_refusal(&mut answer, code)?,
        }
    }
    Ok(wire::Request::Pull) | Ok(wire::Request::Cancel(_)) => streams(&message, &mut answer),
    Err(code) => wire::write_refusal(&mut answer, code)?,
}
```

## A client

A client writes an OPERATION, reads the answer with the kind it sent, and decodes the body. An
answer 200.3 carries the head of a stream and the answer 200.0 of a PULL or a CANCEL a batch, and
`stream::Entries` reads either with no allocation, slicing each `next` value by the depth of its
brackets.

```rust,ignore
use graphtp::{credential::Credential, kind::Kind, stream::Entries, wire, zlib};

let mut scratch = zlib::Scratch::new(zlib::PARAMS)?;
let mut request = Vec::new();
let credential = Credential::new(b"Bearer eyJhbGciOiJIUzI1NiJ9.e30.c2ln")?;
wire::write_operation(&mut request, Some(credential), br#"{"query":"{__typename}"}"#, &mut scratch)?;

let answer = transport(&request);
match wire::decode_answer(Kind::Operation, &answer)? {
    wire::Answer::Result { code, body } => {
        let mut json = Vec::new();
        wire::decode_body(body, bound, &mut json)?;
        deliver(code, json);
    }
    wire::Answer::Opened { head, .. } => {
        let mut plain = Vec::new();
        wire::decode_body(head, bound, &mut plain)?;
        let entries = Entries::new(&plain)?;
        let (handle, seq) = (entries.handle(), entries.seq());
        for entry in entries {
            apply(handle, seq, entry?);
        }
    }
    wire::Answer::Refused(code) => refused(code),
    // An OPERATION is never answered with a batch.
    wire::Answer::Batch { .. } => {}
}
```

## What the judges measured

Every number below comes from a run of the notebooks of the suite named in the row, and the two rows
of the sender rule from a run under the rule >128 B, with the crate built by rustc 1.95.0. The side
the crate is judged against is `tests/harness/homa/graphtp.py`, written from the specification with
Python's `zlib` and never from the crate.

| What | Result | Suite |
| --- | --- | --- |
| Status bytes that round trip | 256 of 256 | codec |
| Registered codes of the classes 2 to 5 that round trip | 59 of 59 | codec |
| Requests and answers written by the crate and read by `graphtp.py`, and the other way, the Japanese contents and bodies of capture row C62 among them | 108 requests and 946 answers each way, 0 differences | codec |
| Requests and answers built through `Request::builder()` and `Response::builder()`, each refusal with the code of `graphtp.py` | 32 requests, 12 refused with 400.7 and 4 with 431.0, and 1,112 answers, 750 refused with 500.2, 0 differences | codec |
| Refusal rules of section 5.3 raised with their code | 11 of 11, and the 62,801 zlib headers that break both FCHECK and the profile answer 400.8 | codec |
| Test vectors of section 9.2 |  | codec |
| Hostile messages to each decoder, `read-request`, `read-answer` and `entries` | 1,000,000 each, 0 panics | codec |
| Contents and bodies coded exactly when they hold >128 B | 1,612 of 1,612, 838 of them coded | codec |
| A content and a body of 128 bytes plain and of 129 bytes coded, each written by one side and read by the other | 4 of 4 | codec |
| Streams of the crate decoded by C zlib at `wbits` 12, whole and in pieces of 1 KiB | 1,900 of 1,900 | content coding |
| Streams of C zlib at `wbits` 9 to 12 decoded by the crate | 58,752 of 58,752 | content coding |
| The heads with the status mapping | 364 bytes of `.text` on `riscv32imc-unknown-none-elf`, 440 on `aarch64-unknown-linux-gnu` | codec |
| The full codec, a request and an answer read and written, by the object file | 19,264 bytes of `.text` and read only data on `riscv32imc-unknown-none-elf`, 22,587 on `aarch64-unknown-linux-gnu` | codec |
| The full codec added to an image with fat LTO on `aarch64-unknown-linux-gnu` | 16,048 bytes of `.text`, `.rodata` and `.eh_frame` | codec |
| The scratch of the encoder at the defaults of the profile | 49,152 bytes, its buffer of 8,192 plain bytes included | content coding |

The notebooks print the parts of their exit numbers that wait for the npm package `graphtp` and for
the binding to Homa as not run.
