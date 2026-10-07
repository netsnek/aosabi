// SPDX-License-Identifier: MIT
/*
 * graphtp: graphtpFetch, a function with the signature of fetch that sends the POST a GraphQL
 * client builds as one OPERATION of GraphTP/1 and returns a Response, with createGraphtpFetch,
 * connect, close and subscriptionClient.
 *
 * The design is docs/graphtp-fetch.md of the Aosabi repository, sections 3 to 8. The module
 * graphtp.wasm writes and reads every message, and this file and lib/ move bytes, build the
 * Response and read URLs (docs/graphtp-fetch.md section 2.1).
 */

import {Link, linkError} from './lib/engine.js'
import {isNode, load, onDrop} from './lib/load.js'
import {SCHEMES, contentOf, graphtpUrl, linkKey, readRequest} from './lib/request.js'
import {answerResponse, ownResponse, streamResponse} from './lib/response.js'
import {admitsStream, readEvents} from './lib/sse.js'

/** The bound on a decoded answer where the caller names none, 64 MiB. */
const DEFAULT_MAX_BODY = 64 * 1024 * 1024

/** The links of the process or page, by the key of their URL. */
const links = new Map()

/** The local names connect gives a link over a byte stream of the caller's own. */
let localNames = 0

/** The links a browser opens in place of those of Node.js, by the carrier of the scheme. */
const BROWSER_LINKS = {ble: 'web-bluetooth', usb: 'webusb', serial: 'web-serial'}

/**
 * Takes the links of a module the loader dropped out of the map, so the next request builds its
 * link on a fresh module, and ends what is in flight on each with LINK_DOWN at once, with no call
 * of the module.
 */
onDrop(module => {
  const error = linkError('LINK_DOWN', 'the module faulted and was dropped')
  for (const [key, link] of links) {
    if (link.module !== module) continue
    links.delete(key)
    const carrier = link.carrier
    link.closed = true
    link.carrier = null
    link.engine = 0
    clearTimeout(link.timer)
    carrier?.close()
    for (const waiter of link.waiters.values()) waiter.reject(error)
    for (const stream of link.streams.values()) stream.fail(error)
    link.waiters.clear()
    link.streams.clear()
  }
})

/**
 * Returns a function with the signature of fetch that sends every URL of GraphTP as one OPERATION
 * and every other URL to `options.fallback`, the global fetch at call time by default.
 *
 * A 200.3 becomes the event stream of docs/graphtp-fetch.md section 7 where the `accept` of the
 * request admits it, as Yoga answers a subscription, and the function's own 406 after a CANCEL of
 * the stream where it does not.
 */
export function createGraphtpFetch(options = {}) {
  const maxBody = options.maxBody ?? DEFAULT_MAX_BODY
  return async function graphtpFetch(input, init) {
    const url = graphtpUrl(input)
    if (url === null) return (options.fallback ?? globalThis.fetch)(input, init)
    const request = await readRequest(input, init ?? {}, url)
    if (request.signal?.aborted) throw request.signal.reason
    const module = await load()
    const sent = await contentOf(request)
    if (sent.status !== undefined) return ownResponse(sent.status, module.statusText(sent.status))
    const link = linkOf(module, url, maxBody)
    const answer = await link.exchange(sent.credential, sent.content, sent.repeat, request.signal, maxBody)
    if (answer.stream === undefined) return answerResponse(answer)
    if (!admitsStream(request.headers.get('accept'))) {
      await answer.stream.readable.cancel()
      return ownResponse(406, module.statusText(406))
    }
    return streamResponse(answer.stream.readable, answer.name)
  }
}

/** The function with its defaults, which stands in for fetch everywhere. */
export const graphtpFetch = createGraphtpFetch()

/**
 * Opens the link the URL `url` names and resolves to the URL that names it. With `options.stream`
 * the link runs over the caller's own byte stream and gets a local name.
 */
export async function connect(url, options = {}) {
  const module = await load()
  if (options.stream) {
    localNames += 1
    const named = new URL('graphtp+serial://link-' + localNames + '/graphql')
    const {open} = await import('./lib/links/stream.js')
    const link = new Link(module, async () => open(options.stream), DEFAULT_MAX_BODY)
    links.set(linkKey(named), link)
    await link.ready()
    return named.href
  }
  const target = graphtpUrl(url)
  if (target === null) throw new TypeError('graphtp: not a URL of GraphTP: ' + url)
  await linkOf(module, target, DEFAULT_MAX_BODY).ready()
  return target.href
}

/** Closes the link the URL `url` names. */
export async function close(url) {
  const target = graphtpUrl(url)
  if (target === null) return
  const key = linkKey(target)
  links.get(key)?.close()
  links.delete(key)
}

/**
 * Returns a client of the shape of graphql-ws whose subscribe runs one subscription through
 * graphtpFetch and feeds each event to the sink, for the GraphiQL toolkit, which calls subscribe
 * and uses what it returns as the teardown and nothing else (docs/graphtp-fetch.md section 12.4).
 *
 * Each `next` event of the stream reaches `sink.next` as its parsed result and the `complete` event
 * `sink.complete`. An answer that is no stream, the JSON answer to a query or a mutation, reaches
 * `sink.next` once and then `sink.complete`, and one with errors and no data reaches `sink.error`
 * with its errors, as graphql-ws hands on the errors of a request. A rejection of the function, a
 * stream that errors and an answer with no result reach `sink.error`, and the teardown aborts the
 * subscription, which sends the CANCEL of its stream, and completes the sink, as the client of
 * graphql-ws completes it. An exception `sink.next` throws within a stream, and a data line that is
 * no JSON, cancel the body, which sends the CANCEL of the stream, so a stream nobody reads holds no
 * PULL of its link back, and the exception then reaches `sink.error`. The end of a stream reaches
 * the sink one task after its last result, as each message of graphql-ws arrives in a task of its
 * own, since the iterator the toolkit builds over the sink drops the results it still holds once
 * the sink completes or errs. An exception that `sink.complete` throws in the teardown reaches the
 * caller of the teardown. Elsewhere an exception that `sink.error` or `sink.complete` throws, or
 * `sink.next` on an answer that is no stream, rejects a promise nobody awaits, as an exception of
 * `sink.error` does inside the client of graphql-ws.
 */
export function subscriptionClient(url, options = {}) {
  return {
    subscribe(payload, sink) {
      const controller = new AbortController()
      let done = false
      const once = call => (...args) => {
        if (done) return
        done = true
        call(...args)
      }
      const guarded = {
        next: value => {
          if (!done) sink.next(value)
        },
        error: once(error => sink.error(error)),
        complete: once(() => sink.complete())
      }
      subscribe(url, options, payload, controller.signal, guarded)
      return () => {
        if (done) return
        controller.abort()
        guarded.complete()
      }
    }
  }
}

/** Runs one subscription of subscriptionClient into the guarded sink `sink`. */
async function subscribe(url, options, payload, signal, sink) {
  let response
  try {
    response = await graphtpFetch(url, {
      method: 'POST',
      headers: {...options.headers, 'content-type': 'application/json', accept: 'text/event-stream'},
      body: JSON.stringify(payload),
      signal
    })
  } catch (error) {
    if (!signal.aborted) sink.error(error)
    return
  }
  if ((response.headers.get('content-type') ?? '').startsWith('text/event-stream')) {
    let completed = false
    let failure = null
    try {
      await readEvents(response.body, (type, data) => {
        if (type === 'next') sink.next(JSON.parse(data))
        if (type !== 'complete') return true
        completed = true
        return false
      })
    } catch (error) {
      failure = error
    }
    if (!completed) await response.body.cancel().catch(() => {})
    await nextTask()
    if (signal.aborted) return
    if (failure !== null) sink.error(failure)
    else if (completed) sink.complete()
    else sink.error(linkError('ABORTED', 'the stream ended without its complete event'))
    return
  }
  let result = null
  try {
    result = JSON.parse(await response.text())
  } catch {
    result = null
  }
  if (result === null || typeof result !== 'object' || Array.isArray(result)) {
    const code = response.headers.get('graphtp-status') ?? String(response.status)
    sink.error(linkError('REFUSED', 'the service answered ' + code + ' ' + response.statusText + ' with no result'))
    return
  }
  if (!('data' in result) && Array.isArray(result.errors)) {
    sink.error(result.errors)
    return
  }
  sink.next(result)
  await nextTask()
  if (!signal.aborted) sink.complete()
}

/** Resolves in a later task, after every microtask the sink's consumer queued. */
function nextTask() {
  return new Promise(resolve => setTimeout(resolve, 0))
}

/**
 * Returns the link of the URL `url`, made on its first use. One link serves every function that
 * names its target, so its engine decodes up to the larger of `maxBody` and the default, and each
 * exchange holds its answer to the bound of its own function.
 */
function linkOf(module, url, maxBody) {
  const key = linkKey(url)
  let link = links.get(key)
  if (link === undefined) {
    link = new Link(module, () => openCarrier(url), Math.max(maxBody, DEFAULT_MAX_BODY))
    links.set(key, link)
  }
  return link
}

/** Opens the carrier of the URL `url` through the link of its scheme, loaded on first use. */
async function openCarrier(url) {
  let carrier = SCHEMES[url.protocol]
  if (!isNode()) {
    carrier = BROWSER_LINKS[carrier]
    if (carrier === undefined) throw linkError('NO_LINK', 'a page opens no link of ' + url.protocol)
  }
  const {open} = await import('./lib/links/' + carrier + '.js')
  return open(url)
}
