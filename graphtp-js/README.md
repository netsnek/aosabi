# graphtp

`graphtpFetch` is a function with the signature of `fetch` that sends the POST a GraphQL client
builds as one OPERATION of GraphTP/1, the Graph Transfer Protocol over Homa, and returns a
`Response`. Adding GraphTP beside HTTP takes one line of existing client code, the `fetch` option
the client already has, with the import of the function and a URL whose scheme names the carrier.

```js
import {ApolloClient, HttpLink, InMemoryCache} from '@apollo/client'
import {graphtpFetch} from 'graphtp'

const client = new ApolloClient({
  link: new HttpLink({uri: 'graphtp+unix:///run/orders/graphtp.sock', fetch: graphtpFetch}),   // the one line
  cache: new InMemoryCache()
})
```

The design is `docs/graphtp-fetch.md` of the Aosabi repository, and the protocol is
`docs/graphtp.md` there. This folder is the source of the package until its repository has a
remote, and nothing of it is published.

## What it does

- A URL of GraphTP, `graphtp://`, `graphtp+unix://`, `graphtp+serial://`, `graphtp+usb://` or
  `graphtp+ble://`, goes out as one OPERATION, and every other URL goes to the global `fetch`.
- A POST sends its body as the content, byte for byte, and a GET builds the content from the
  parameters `query`, `operationName`, `variables` and `extensions` of its URL. Any other method
  answers 405, and a body under a type that is not JSON answers 415.
- The field `authorization` becomes the credential, and `graphtp-repeatable: 1` marks an OPERATION
  the function may repeat once after an unknown outcome. No other field travels.
- A content of more than 128 bytes is coded in the zlib format with a window of 4 KiB.
- The `Response` carries the status, the name of the code as `statusText`, the field
  `graphtp-status` with `status.substatus`, and the decoded body under
  `application/graphql-response+json; charset=utf-8`, or no body and no `content-type` for an
  answer with an empty body.
- A subscription the service opens as a stream, 200.3, becomes a `Response` of status 200 with
  the `statusText` `Stream Opened`, `graphtp-status: 200.3`, `cache-control: no-cache` and the
  event stream GraphQL Yoga writes under `text/event-stream`: the comment `:\n\n`, one
  `event: next` per result and `event: complete` at the end, with no ping. A request whose
  `accept` admits no `text/event-stream` gets the function's own 406, after the function cancelled
  the stream. Cancelling the body or aborting the signal sends the CANCEL of the stream, and a
  reader that stalls holds the PULLs back, so the back pressure reaches the server.
- A failure of the transport rejects with a `TypeError` whose `cause.code` is `NO_LINK`,
  `TOO_LARGE`, `UNKNOWN_OUTCOME`, `LINK_DOWN`, `MALFORMED_ANSWER`, or `NO_MEMORY` where the module
  has no memory for the message. The body of a stream errors with `ABORTED` where the source of
  the stream failed or the server cancelled a stream its reader did not cancel, and with
  `LINK_DOWN` where the link went down or the stream ended with no entry of the server. A body
  whose `length` is no integer from 0 to 0xffffffff, or that holds more bytes than its `length`
  reads, rejects with the `TypeError` or `RangeError` of the glue and no `cause.code` before
  anything is sent, and its link stays up.
- A fault of the module, a trap or a stack overflow inside one of its calls, drops the module, and
  the next request runs on a fresh module. Where the fault meets a link as the link is built,
  connects, sends an exchange, takes a chunk, runs its timer, meets an abort or loses its carrier,
  every exchange and stream of the links of the module ends with `LINK_DOWN`, with the fault as
  `cause.cause` on the link whose call met it. On the other paths, the function's own 405, 415 and
  406, the pull and the cancel of a body and `close`, the call rejects or the body errors with the
  fault itself until a later batch wraps them. A chunk a link received that the module has no
  memory for ends that link with `NO_MEMORY`. The next request opens a link of `graphtp+unix`
  again, while a link over a byte stream of the caller's own has closed that stream and answers
  `NO_LINK`. Neither a fault on the paths of a link nor a refused chunk is thrown into the event
  loop, so neither ends the process of Node.js.
- `subscriptionClient(url)` is the `wsClient` of the GraphiQL toolkit: its `subscribe` runs one
  subscription through `graphtpFetch` and hands each result to the sink. An exception the sink's
  `next` throws within a stream cancels the stream, which sends its CANCEL, and reaches the sink's
  `error`. The error path of the toolkit 0.12.1 evaluates `err instanceof CloseEvent`, which
  throws a `ReferenceError` in a runtime with no global `CloseEvent`, Node.js 22 among them,
  whatever error the client hands it.

## What it holds

The module `graphtp.wasm`, built from the crates `graphtp`, `graphtp-homa` and `homa` of the Aosabi
workspace through its crate `graphtp-wasm` for `wasm32-unknown-unknown`, writes and reads every
message, keeps the Homa engine and applies the rule of at most once, and it imports nothing. The
JavaScript moves bytes between the module and a link, builds the `Response` and reads URLs. The file
`graphtp.wasm.sha256` holds the SHA-256 of the module.

| File | Holds |
| --- | --- |
| `index.js` | `graphtpFetch`, `createGraphtpFetch`, `connect`, `close`, `subscriptionClient` |
| `index.d.ts` | The types, written by hand |
| `register.js` | The scheme aware global `fetch`, `import 'graphtp/register'` |
| `lib/load.js` | The loader of the module and the calls of its exports |
| `lib/engine.js` | One link, the engine of the module over one carrier |
| `lib/request.js` | The URL test, the key of a link, the method, the type and the content of a GET |
| `lib/response.js` | The `Response` of an answer |
| `lib/sse.js` | The stream of server sent events of a 200.3 and its reader |
| `lib/links/` | The carriers, `unix.js` over `node:net` and `stream.js` over a caller's own byte stream |

## Where it stands

The package asks for Node.js 22 or later in its field `engines`, `>=22`. On Node.js 22.22.0 and
24.13.0 the tests of the Aosabi repository load its module, send an OPERATION over the link
`graphtp+unix` to the stream socket of a server and run the error path of the GraphiQL toolkit
through `subscriptionClient`. On Node.js 24.13.0 alone they read its streams through
`graphql-sse`, urql, executor-http, Relay and Apollo Client equal to the event stream of Aosabi
over HTTP.
The helper `homa-raw` for `graphtp://`, the device carriers in Node.js and the carriers of a browser
come later, and each of their links refuses with `NO_LINK` until then.

The package has no dependency, no development dependency and no install script. The carriers of
the devices take one optional peer each, `serialport` ^13, `usb` ^3 or `node-ble` ^1, which npm never
installs by itself.

## Licence

MIT, see `LICENSE`.
