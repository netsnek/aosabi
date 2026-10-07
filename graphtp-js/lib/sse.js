// SPDX-License-Identifier: MIT
/*
 * graphtp lib/sse: the stream of server sent events a 200.3 becomes, frame for frame the event
 * stream GraphQL Yoga writes over HTTP, and the reader of such a stream that subscriptionClient
 * runs (docs/graphtp-fetch.md sections 7 and 12.4).
 *
 * A stream opens with the comment `:\n\n`, writes `event: next\ndata: ` with the value of each
 * `next` entry and `\n\n`, and ends with `event: complete\ndata:\n\n`, as Yoga 5.18.0 writes them.
 * It writes no ping, since a parked PULL is a patient RPC that Homa keeps alive. The queue of the
 * stream bounds how far ahead its link pulls, so a slow reader slows the PULLs and the back
 * pressure reaches the server.
 */

const encoder = new TextEncoder()

/** The opening comment of a stream, as Yoga writes it. */
export const OPENING = ':\n\n'

/** The frame of the `complete` entry, after which the stream closes. */
export const COMPLETE = 'event: complete\ndata:\n\n'

/** The frames the queue of a stream holds before its link parks no further PULL. */
export const QUEUE = 16

const OPENING_BYTES = encoder.encode(OPENING)
const COMPLETE_BYTES = encoder.encode(COMPLETE)
const EVENT_NEXT = encoder.encode('event: next\n')
const DATA = encoder.encode('data: ')
const LF = 0x0a
const CR = 0x0d

/**
 * Returns the frame of one `next` entry, the bytes `value` of the result as the service wrote them.
 * A value with no line break, which is every value Yoga writes, gives Yoga's frame
 * `event: next\ndata: <value>\n\n`, and each further line of a value gets a data line of its own,
 * which a reader joins back with a line feed.
 */
export function nextFrame(value) {
  const lines = []
  let start = 0
  for (let i = 0; i < value.length; i++) {
    if (value[i] !== LF && value[i] !== CR) continue
    lines.push(value.subarray(start, i))
    if (value[i] === CR && value[i + 1] === LF) i += 1
    start = i + 1
  }
  lines.push(value.subarray(start))
  let length = EVENT_NEXT.length + 1
  for (const line of lines) length += DATA.length + line.length + 1
  const frame = new Uint8Array(length)
  frame.set(EVENT_NEXT, 0)
  let at = EVENT_NEXT.length
  for (const line of lines) {
    frame.set(DATA, at)
    frame.set(line, at + DATA.length)
    at += DATA.length + line.length
    frame[at++] = LF
  }
  frame[at] = LF
  return frame
}

/**
 * Tells whether the `accept` of a request admits the stream: text/event-stream, text/* or * / *, or
 * no accept at all, as Yoga reads a missing accept (docs/graphtp-fetch.md section 7).
 */
export function admitsStream(accept) {
  if (accept === null || accept.trim() === '') return true
  return accept.split(',').some(range => {
    const media = range.split(';')[0].trim().toLowerCase()
    return media === 'text/event-stream' || media === 'text/*' || media === '*/*'
  })
}

/**
 * One stream of a link, the body of its Response. Its link hands it the entries of its handle in
 * the order of `seq`, and it calls `onCancel` once its reader cancels it and `onPull` whenever its
 * queue has room.
 */
export class EventStream {
  constructor(handle, onCancel, onPull) {
    this.handle = handle
    this.ended = false
    this.cancelling = false
    this.failure = null
    this.onEnd = null
    this.readable = new ReadableStream({
      start: controller => {
        this.controller = controller
        controller.enqueue(OPENING_BYTES.slice())
      },
      pull: () => {
        if (this.failure !== null && this.drained) {
          this.controller.error(this.failure)
          this.failure = null
          return
        }
        onPull()
      },
      cancel: () => {
        this.failure = null
        if (this.ended) return
        this.end()
        onCancel()
      }
    }, new CountQueuingStrategy({highWaterMark: QUEUE}))
  }

  /** Whether the reader has read every frame of the queue. */
  get drained() {
    return this.controller.desiredSize === QUEUE
  }

  /** Whether the queue of the stream takes more frames. */
  get room() {
    return !this.ended && this.controller.desiredSize > 0
  }

  /** Writes the frame of a `next` entry with the bytes `value`. */
  next(value) {
    if (!this.ended) this.controller.enqueue(nextFrame(value))
  }

  /** Writes the frame of the `complete` entry and closes the stream. */
  complete() {
    if (this.ended) return
    this.controller.enqueue(COMPLETE_BYTES.slice())
    this.controller.close()
    this.end()
  }

  /**
   * Errors the stream with `error` once its reader has read the frames that arrived before the
   * failure, as a reader over HTTP reads the bytes Yoga sent before its source threw.
   */
  fail(error) {
    if (this.ended) return
    if (this.drained) this.controller.error(error)
    else this.failure = error
    this.end()
  }

  /** Errors the stream with `reason` at once, as fetch errors a body when its signal aborts. */
  abort(reason) {
    if (this.ended) return
    this.controller.error(reason)
    this.end()
  }

  /** Marks the stream ended. */
  end() {
    this.ended = true
    this.onEnd?.()
    this.onEnd = null
  }
}

/**
 * Reads the event stream `body` and hands each event to `onEvent` with its type and its data, the
 * data lines joined with a line feed, until the stream ends or `onEvent` answers false. A comment
 * and an event with no data line are skipped.
 */
export async function readEvents(body, onEvent) {
  const reader = body.getReader()
  const decoder = new TextDecoder()
  let text = ''
  let type = ''
  let data = null
  try {
    for (;;) {
      const {done, value} = await reader.read()
      text += done ? decoder.decode() : decoder.decode(value, {stream: true})
      let end
      while ((end = lineEnd(text, done)) >= 0) {
        const line = text.slice(0, end)
        text = text.slice(end + (text[end] === '\r' && text[end + 1] === '\n' ? 2 : 1))
        if (line === '') {
          if (data !== null && onEvent(type || 'message', data) === false) return
          type = ''
          data = null
          continue
        }
        if (line.startsWith(':')) continue
        const colon = line.indexOf(':')
        const field = colon < 0 ? line : line.slice(0, colon)
        let content = colon < 0 ? '' : line.slice(colon + 1)
        if (content.startsWith(' ')) content = content.slice(1)
        if (field === 'event') type = content
        if (field === 'data') data = data === null ? content : data + '\n' + content
      }
      if (done) return
    }
  } finally {
    reader.releaseLock()
  }
}

/**
 * Returns the index of the first line break of `text`, or -1. A CR at the end of the text waits for
 * the next piece, which may begin with its LF, unless the stream ended.
 */
function lineEnd(text, done) {
  for (let i = 0; i < text.length; i++) {
    if (text[i] === '\n') return i
    if (text[i] === '\r') return i + 1 < text.length || done ? i : -1
  }
  return -1
}
