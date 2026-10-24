# GraphTP

GraphTP, the Graph Transfer Protocol, carries one GraphQL request from a client to a service and the
service's answer back, one exchange per Homa remote procedure call. It exists for the links where
HTTP does not fit, a serial line, USB, Bluetooth, LoRa, a Unix socket or a data centre network, and
it carries the same GraphQL over HTTP request a client already writes. It is specified apart from
Aosabi, so any GraphQL server and client can implement it. This page describes version 1, GraphTP/1,
and how Aosabi serves it.

## 1. What travels

A request is a head of 3 bytes, the credential and the content. An answer is a head of 2 bytes, the
status and the substatus, and the body. Every integer is big endian.

| Byte of a request | Bits | Field |
| --- | --- | --- |
| 1 | 7 and 6 | The version, `01` for GraphTP/1. `00` is never valid |
| 1 | 5 to 3 | The kind: `000` OPERATION, `001` PULL, `010` CANCEL |
| 1 | 2 | 1 when the content is coded, 0 when it is plain |
| 1 | 1 and 0 | Reserved, sent as 0 |
| 2 and 3 | 15 to 0 | The length of the credential, 0 for none |
| From 4 | | The credential, then the content |

- The credential is the value an HTTP client puts in `Authorization`, such as `Bearer eyJ...`,
  byte for byte. It is visible ASCII and is never compressed.
- The content of an OPERATION is the JSON body a GraphQL client sends in an HTTP POST, with `query`,
  `operationName`, `variables` and `extensions`, byte for byte.
- The first byte of an answer is the HTTP status, written as
  `64 * (status div 100 - 2) + status mod 100`, so every status from 200 to 263, 300 to 363, 400 to
  463 and 500 to 563 has one byte, and every registered status among them. The second byte is the
  substatus, 0 for none.
- The body of an answer is empty, plain JSON that starts with `{` or `[`, or one zlib stream, and
  the receiver tells which from its first byte.

GraphTP carries no method, no URL, no header field, no cookie, no session and no cache. Every
exchange stands alone, so there is no handshake before the first request.

## 2. Status and substatus

The status keeps the meaning HTTP gives it, so a client that knows no substatus reads the status
alone and is right about the class of the outcome. The substatus names what the status alone cannot.

| Code | Name | When |
| --- | --- | --- |
| 200.0 | OK | A result with data and no error |
| 200.1 | Partial | A result with data and errors |
| 200.2 | No Data | A result whose `data` is null, with errors |
| 200.3 | Stream Opened | The operation opened a stream, and the body is its head |
| 400.1 | Syntax Error | The document does not parse |
| 400.2 | Validation Error | The document fails validation against the schema |
| 400.3 | Invalid Variables | A variable is missing or fails coercion |
| 400.4 | Operation Not Found | `operationName` names no operation of the document |
| 400.5 | Query Too Complex | The document exceeds a bound of the server |
| 400.7 | Malformed Message | The message breaks a rule of the wire form |
| 400.8 | Bad Coding | The zlib stream does not decode |
| 413.0 | Content Too Large | The decoded content exceeds the bound of the server |
| 415.1 | Unsupported Coding | The zlib header asks for more than the protocol allows |
| 505.0 | HTTP Version Not Supported | The server speaks another version of GraphTP |

The crate `graphtp` holds every code of the registry with its name, the codes of streams, refusals
and gateways among them.

## 3. The coding

A content or a body of more than 128 bytes travels as one zlib stream of RFC 1950, deflate with a
window of at most 4 KiB and no preset dictionary, in both directions. One of 128 bytes or fewer
travels plain. The coding has no choice, so no field names it, and the window is small enough for a
device to hold while it decodes. The decoded bytes are the bytes the service or the client wrote, so
an answer is the same after decoding whatever its coding.

## 4. Streams

A subscription is one OPERATION that opens a stream, then PULL requests that collect its events,
then its end.

```text
client                                      server
OPERATION  40 00 00 {"query":"subscription{tick}"}
                                            00 03 {"stream":1,"seq":0,"events":[]}
PULL       48 00 00                         (parked until an event is ready)
                                            00 00 {"seq":1,"events":[{"stream":1,"next":{...}}]}
```

The server parks one PULL per client and answers it when an event of any of that client's streams is
ready, so one parked request serves every stream of the client and the server never opens a
connection of its own. Every stream answer carries `seq`, a count per client, so a client applies
answers in order even where two arrive out of order. A CANCEL ends one stream, or every stream of
the client with the handle 0, which a client also sends when it starts on a link where an earlier
run of it may have ended unseen.

## 5. At most once

Homa keeps a request until the client acknowledges its answer, so a retransmitted request never runs
twice while the server holds it. After a link goes down, or after the server forgets the request,
the outcome is unknown, and a client repeats an operation only when its caller marked it repeatable,
as an HTTP client treats a request that is not idempotent.

## 6. Over Homa and over each carrier

One Homa message is one GraphTP message, and one exchange is one Homa remote procedure call. The
crate [homa](../../crates/homa/) implements Homa in the project's own code, with a compact form for
the device carriers and the full form for raw IP.

| Carrier | Below Homa | Form of Homa |
| --- | --- | --- |
| Network | Raw IP protocol 146 | Full |
| Unix Socket | A `SOCK_SEQPACKET` socket, or a stream socket with prefixed packets | Compact |
| USB | Bulk transfers, one packet each, or USB serial with delimited packets | Compact |
| Bluetooth | The L2CAP LE channel with prefixed packets, or one packet per GATT value | Compact |
| Serial Line | A UART with delimited packets, COBS with a CRC-32C | Compact |
| LoRa | One sealed LoRa frame per packet | Compact |

On a byte stream the framing marks where each Homa packet starts and ends, so GraphTP never meets a
byte stream itself.

## 7. Sizes

The GraphTP suite of the project, which runs in its own pipeline, wrote and read back each message
through the crate `graphtp` on 2026-09-27, with no credential and no header of a transport:

| Exchange | Request and answer |
| --- | ---: |
| `{__typename}` | 27 + 33 = 60 bytes |
| A mutation with variables | 209 + 137 = 346 bytes |
| A partial result with an error | 28 + 147 = 175 bytes |
| A PULL, a CANCEL and a refusal | 3, 5 and 2 bytes |

On the same day it measured the two heads with the mapping of the status at 364 bytes of code on
`riscv32imc-unknown-none-elf` and 440 on `aarch64-unknown-linux-gnu`, built for size, and the whole
codec with its encoder and decoder at 19,264 bytes of code and read only data in the object file for
`riscv32imc-unknown-none-elf`. The suite of the content coding coded the 597 answers of more than
128 bytes of the test corpus on 2026-09-27, from 1,628,748 bytes to 294,771.

## 8. The crates and the client

- [crates/graphtp](../../crates/graphtp/) holds the types and the wire codec of GraphTP/1 with its
  encoder and decoder, with no standard library, no dependency and no feature.
- [crates/graphtp-homa](../../crates/graphtp-homa/) binds it to Homa as a server and a client that
  do no input or output of their own.
- The npm package `graphtp` in [graphtp-js/](../../graphtp-js/) gives JavaScript `graphtpFetch`, a
  function with the signature of `fetch`, so a GraphQL client adds GraphTP through the `fetch`
  option it already has:

```js
import {HttpLink} from '@apollo/client'
import {graphtpFetch} from 'graphtp'

const link = new HttpLink({uri: 'graphtp+unix:///run/app/graphtp.sock', fetch: graphtpFetch})
```

The package carries the codec and the Homa client as one WebAssembly module with no import, asks for
Node.js 22 or later, and has no dependency of its own. It speaks over a Unix socket and over a byte
stream the caller hands it today, and its links for serial, USB and Bluetooth refuse with `NO_LINK`
until they land.

## 9. Aosabi as a GraphTP server

Aosabi serves GraphTP beside HTTP from the same app. Its GraphTP server checks each request, decodes
its content and hands it to the app as an HTTP POST with fixed fields, so a document gets the same
status and body over GraphTP as over HTTP. On Linux the carriers are features of the facade, each
started by one environment variable:

| Feature | Variable | What it serves |
| --- | --- | --- |
| `serial` | `AOSABI_SERIAL` | A tty, named with its baud rate as `<tty>:<baud>` |
| `usb` | `AOSABI_USB_FFS`, `AOSABI_USB_ACM` | The bulk and the serial function of a USB gadget |
| `bluetooth` | `AOSABI_BLUETOOTH` | The GATT data path through BlueZ |
| `homa-network` | `AOSABI_HOMA_NETWORK` | Raw IP 146, which needs `CAP_NET_RAW` |

The test suite runs each carrier against a client written apart from the crates: the serial line
over a pair of pseudo terminals with injected noise, USB through an emulated gadget, Bluetooth
through two virtual controllers of BlueZ and raw IP between two network namespaces. LoRa runs over a
model of two radios. None of these runs on hardware in the test suite yet.

## 10. Security

- The credential stands outside the zlib stream, so no secret shares a compression context with
  bytes a caller chooses, which is the attack of CRIME and BREACH.
- A server bounds the decoded content and answers 413.0 as soon as the decoder passes the bound, so
  a stream that expands a thousand fold costs the bound and no more.
- GraphTP encrypts nothing itself, so a carrier or the layer below protects the bytes, as pairing
  does on Bluetooth, and a client across the internet reaches a service over HTTPS.
