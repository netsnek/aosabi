// SPDX-License-Identifier: MIT
/*
 * graphtp: the types of the package, written by hand, as docs/graphtp-fetch.md section 3 of the
 * Aosabi repository gives them.
 */

/**
 * Sends a URL of GraphTP as one OPERATION of GraphTP/1 and resolves to its Response, and sends
 * every other URL to the global fetch at call time. It rejects with a TypeError whose `cause.code`
 * names the failure of the transport: NO_LINK, TOO_LARGE, UNKNOWN_OUTCOME, LINK_DOWN,
 * MALFORMED_ANSWER, or NO_MEMORY where the module has no memory for the message or for a chunk its
 * link received, which ends the link. A body whose length is no integer from 0 to 0xffffffff, or
 * that holds more bytes than its length reads, rejects with the TypeError or RangeError of the glue
 * and no `cause.code` before anything is sent, and its link stays up. A fault of the module drops
 * it, and the next request runs on a fresh module. Where the fault meets a link as the link is
 * built, connects, sends an exchange, takes a chunk, runs its timer, meets an abort or loses its
 * carrier, the request rejects with LINK_DOWN, with the fault as `cause.cause` on the link whose
 * call met it, while the function's own 405, 415 and 406 still reject with the fault itself until a
 * later batch wraps them. A stream the service opens, 200.3, resolves to a Response whose body is
 * the event stream Yoga writes, where the `accept` of the request admits text/event-stream, and to
 * the function's own 406 otherwise. Its body errors with a TypeError whose `cause.code` is ABORTED
 * where the source of the stream failed or the server cancelled a stream its reader did not cancel,
 * LINK_DOWN where the link went down or the stream ended with no entry of the server, NO_MEMORY or
 * LINK_DOWN where the link ended as above, with the fault itself where a fault of the module met
 * its own pull, and with the reason of the signal where the signal of the request aborts.
 */
export function graphtpFetch(input: RequestInfo | URL, init?: RequestInit): Promise<Response>

/** Returns a graphtpFetch with the options `options`. */
export function createGraphtpFetch(options?: GraphtpFetchOptions): typeof fetch

/** The options of createGraphtpFetch. */
export interface GraphtpFetchOptions {
  /** The fetch of every URL that is not GraphTP's, globalThis.fetch at call time by default. */
  fallback?: typeof fetch
  /** Node.js, the socket of homa-raw, HOMA_RAW_SOCKET or /run/homa-raw.sock. */
  helper?: string
  /** The bound on a decoded answer, 64 MiB by default. */
  maxBody?: number
}

/** Opens a link and resolves to the URL that names it. */
export function connect(url: string, options?: ConnectOptions): Promise<string>

/** The options of connect. */
export interface ConnectOptions {
  /** Web Bluetooth, the service of the device. */
  filters?: BluetoothLEScanFilter[]
  /** Web Serial and serialport. */
  baudRate?: number
  /**
   * A byte stream of the caller's own, framed with delimiters, which serves its link until it
   * closes, after which the link answers NO_LINK.
   */
  stream?: {readable: ReadableStream<Uint8Array>, writable: WritableStream<Uint8Array>}
}

/** Closes every link the URL `url` names. */
export function close(url: string): Promise<void>

/**
 * A client of the shape of graphql-ws whose subscribe runs one subscription through graphtpFetch,
 * for the wsClient of the GraphiQL toolkit. Each result reaches `sink.next` and the end of the
 * stream `sink.complete`, the errors of a request with no data and every failure `sink.error`, the
 * latter a TypeError whose `cause.code` is one of graphtpFetch's or REFUSED where the answer holds
 * no result. An exception `sink.next` throws within a stream cancels the stream and reaches
 * `sink.error` as it was thrown. The function it returns aborts the subscription.
 */
export function subscriptionClient(url: string, options?: {headers?: Record<string, string>}): {
  subscribe(
    payload: object,
    sink: {next(value: unknown): void, error(error: unknown): void, complete(): void}
  ): () => void
}

/** The filter of a device of Web Bluetooth, which a page without the DOM types names here. */
type BluetoothLEScanFilter = Record<string, unknown>
