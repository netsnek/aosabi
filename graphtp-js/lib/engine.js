// SPDX-License-Identifier: MIT
/*
 * graphtp lib/engine: one link of the package, the engine of the module over one carrier, which
 * moves the bytes between them, runs the timers, hands each exchange its outcome and each stream
 * its entries.
 *
 * The engine of the module writes and reads every message, keeps the Homa engine, the batch numbers
 * and the parked PULL, and applies the rule of at most once, and this file moves bytes and events
 * alone.
 */

import {EVENT, WIRE_ERRORS, linkError} from './load.js'
import {EventStream} from './sse.js'

export {linkError}

/** The longest message a link takes, the answers of the server, 1 MiB. */
const MAX_MESSAGE = 1 << 20

/**
 * One link: a carrier opened on first use and kept for later requests, since a link is a carrier and
 * not a session, and opened again after it closed.
 */
export class Link {
  /**
   * `module` is the loaded module, `open` a function that opens the carrier and resolves to it, and
   * `maxBody` the bound on a decoded body. A fault of the module in the first call of the link, the
   * statusText of a stream, throws LINK_DOWN with the fault as its cause.
   */
  constructor(module, open, maxBody) {
    this.module = module
    this.open = open
    this.maxBody = maxBody
    this.engine = 0
    this.carrier = null
    this.opening = null
    this.waiters = new Map()
    this.streams = new Map()
    this.timer = null
    this.closed = false
    this.pumping = false
    this.again = false
    try {
      this.streamName = module.statusText(200, 3)
    } catch (error) {
      throw linkFault(error)
    }
  }

  /** Opens the carrier where none is open, and brings the link up. */
  async ready() {
    if (this.carrier !== null) return
    if (this.opening === null) {
      this.opening = this.connect().finally(() => {
        this.opening = null
      })
    }
    await this.opening
  }

  /** Opens the carrier and brings the engine's link up over it, or closes it again on a failure. */
  async connect() {
    const carrier = await this.open()
    try {
      if (this.engine === 0) {
        this.engine = this.module.engineNew(carrier.profile, carrier.param, carrier.framing, MAX_MESSAGE,
          Math.min(this.maxBody, 0xffffffff), carrier.sight)
        if (this.engine === 0) throw linkError('NO_LINK', 'the module cannot build the engine of the link')
      }
      carrier.onData = bytes => {
        if (this.carrier !== carrier) return
        try {
          this.module.pushRx(this.engine, bytes)
          this.pump()
        } catch (error) {
          this.fail(error)
        }
      }
      carrier.onClose = () => this.down(carrier)
      this.carrier = carrier
      if (this.module.x.link_up(this.engine) !== 1) throw linkError('NO_LINK', 'the engine refused to bring the link up')
      this.pump()
    } catch (error) {
      if (this.carrier === carrier) this.carrier = null
      carrier.close()
      throw linkFault(error)
    }
  }

  /**
   * Sends one OPERATION and resolves to its outcome, the event of its answer or the stream it
   * opened, or rejects with the TypeError of docs/graphtp-fetch.md section 8. An abort of `signal`
   * ends the exchange at once, or the stream it opened, and a decoded body longer than `maxBody`
   * rejects with TOO_LARGE. A fault of the module ends the link and rejects with LINK_DOWN. A
   * refused buffer rejects with NO_MEMORY, and a content `put` cannot copy with its TypeError or
   * RangeError, and both leave the link up.
   */
  async exchange(credential, content, repeat, signal, maxBody = this.maxBody) {
    await this.ready()
    if (signal?.aborted) throw signal.reason
    let token
    try {
      token = this.module.operation(this.engine, credential, content, repeat)
    } catch (error) {
      if (!this.module.dead) throw error
      this.fail(error)
      throw linkFault(error)
    }
    if (token <= 0) throw linkError('NO_LINK', 'the module refused the OPERATION with ' + token)
    const outcome = new Promise((resolve, reject) => {
      const onAbort = () => {
        this.waiters.delete(token)
        try {
          this.module.x.abort(this.engine, token)
          this.idle()
        } catch (error) {
          this.fail(error)
        }
        reject(signal.reason)
      }
      signal?.addEventListener('abort', onAbort, {once: true})
      this.waiters.set(token, {
        signal,
        resolve: value => {
          signal?.removeEventListener('abort', onAbort)
          if (value.stream === undefined && value.body.length > maxBody) {
            reject(linkError('TOO_LARGE', 'the decoded answer is longer than its bound'))
            return
          }
          resolve(value)
        },
        reject: error => {
          signal?.removeEventListener('abort', onAbort)
          reject(error)
        }
      })
    })
    this.carrier?.ref()
    try {
      this.pump()
    } catch (error) {
      this.fail(error)
    }
    return outcome
  }

  /**
   * Opens the stream `handle` of an exchange whose OPERATION opened it, which an abort of `signal`
   * ends with the reason of the signal and a CANCEL of the handle.
   */
  openStream(handle, signal) {
    const stream = new EventStream(handle, () => this.cancelStream(stream), () => this.pump())
    this.streams.set(handle, stream)
    if (signal) {
      const onAbort = () => {
        stream.abort(signal.reason)
        try {
          this.cancelStream(stream)
        } catch (error) {
          this.fail(error)
        }
      }
      signal.addEventListener('abort', onAbort, {once: true})
      stream.onEnd = () => signal.removeEventListener('abort', onAbort)
    }
    this.carrier?.ref()
    return stream
  }

  /**
   * Sends the CANCEL of the stream `stream` once, whose entries the link drops until its end
   * arrives, as Yoga returns its source when the client closes the connection
   * (docs/graphtp-fetch.md section 7).
   */
  cancelStream(stream) {
    if (stream.cancelling || this.streams.get(stream.handle) !== stream) return
    stream.cancelling = true
    this.cancel(stream.handle)
  }

  /** Sends a CANCEL of the stream `handle`. */
  cancel(handle) {
    if (this.engine === 0) return
    this.module.x.cancel(this.engine, handle)
    this.pump()
  }

  /**
   * Hands every event to its exchange or its stream, parks the next PULL where every stream has
   * room, writes every frame the carrier sends and sets the timer. A call made while the link pumps,
   * from the pull of a stream whose queue took a frame, runs the loop once more.
   */
  pump() {
    if (this.engine === 0) return
    if (this.pumping) {
      this.again = true
      return
    }
    this.pumping = true
    try {
      do {
        this.again = false
        for (let event = this.module.pollEvent(this.engine); event; event = this.module.pollEvent(this.engine)) {
          this.dispatch(event)
        }
        this.park()
        if (this.carrier !== null) {
          for (let bytes = this.module.pollTx(this.engine); bytes.length > 0; bytes = this.module.pollTx(this.engine)) {
            this.carrier.send(bytes)
          }
        }
      } while (this.again && this.engine !== 0)
    } finally {
      this.pumping = false
    }
    this.schedule()
    this.idle()
  }

  /**
   * Asks the module for the next PULL where the link holds a stream its reader keeps, and every such
   * stream has room in its queue, so a reader that stalls holds the PULLs back
   * (docs/graphtp-fetch.md section 7).
   */
  park() {
    let open = 0
    for (const stream of this.streams.values()) {
      if (stream.cancelling) continue
      if (!stream.room) return
      open += 1
    }
    if (open > 0) this.module.x.pull(this.engine)
  }

  /** Hands one event of the module to the exchange or the stream it names. */
  dispatch(event) {
    const waiter = event.token === 0 ? undefined : this.waiters.get(event.token)
    if (waiter === undefined) {
      this.streamEvent(event)
      return
    }
    this.waiters.delete(event.token)
    switch (event.kind) {
      case EVENT.RESULT:
        waiter.resolve({
          status: event.status,
          substatus: event.substatus,
          name: event.name,
          body: event.bytes.slice()
        })
        break
      case EVENT.OPENED:
        waiter.resolve({
          status: 200,
          substatus: 3,
          name: this.streamName,
          stream: this.openStream(event.handle, waiter.signal)
        })
        break
      case EVENT.UNKNOWN:
        waiter.reject(linkError('UNKNOWN_OUTCOME', 'the server may or may not have run the OPERATION'))
        break
      case EVENT.LINK_DOWN:
        waiter.reject(linkError('LINK_DOWN', 'the link went down, or the peer was declared dead'))
        break
      case EVENT.MALFORMED:
        waiter.reject(linkError('MALFORMED_ANSWER', 'the answer is malformed: ' + WIRE_ERRORS[event.substatus]))
        break
      case EVENT.TOO_LARGE:
        waiter.reject(linkError('TOO_LARGE', 'the request or the decoded answer is longer than its bound'))
        break
      case EVENT.NO_LINK:
        waiter.reject(linkError('NO_LINK', 'the link is not up'))
        break
      default:
        waiter.reject(linkError('NO_MEMORY', 'the module has no memory for the message'))
    }
  }

  /**
   * Hands one entry of a stream to the stream of its handle. A stream no exchange waits for, opened
   * by an OPERATION its caller aborted, is cancelled at once, and the entries of a stream its reader
   * cancelled are dropped until its end arrives.
   */
  streamEvent(event) {
    const stream = this.streams.get(event.handle)
    switch (event.kind) {
      case EVENT.OPENED:
        this.module.x.cancel(this.engine, event.handle)
        return
      case EVENT.NEXT:
        if (stream !== undefined && !stream.cancelling) stream.next(event.bytes.slice())
        return
      case EVENT.COMPLETE:
        stream?.complete()
        break
      case EVENT.CANCELLED:
        stream?.fail(linkError('ABORTED', 'the server ended the stream'))
        break
      case EVENT.ABORTED:
        stream?.fail(linkError('ABORTED', 'the source of the stream failed'))
        break
      case EVENT.LOST:
        stream?.fail(linkError('LINK_DOWN', 'the stream ended with no entry of the server'))
        break
      default:
        return
    }
    this.streams.delete(event.handle)
  }

  /** Sets the one timer of the link to the next deadline of its engine. */
  schedule() {
    if (this.timer !== null) {
      clearTimeout(this.timer)
      this.timer = null
    }
    const due = this.module.nextTimeout(this.engine)
    if (due < 0n) return
    const wait = Math.max(0, Number(due - BigInt(Math.floor(performance.now() * 1e6))) / 1e6)
    this.timer = setTimeout(() => {
      this.timer = null
      try {
        this.module.handleTimeout(this.engine)
        this.pump()
      } catch (error) {
        this.fail(error)
      }
    }, Math.ceil(wait))
    if (this.waiters.size === 0 && this.streams.size === 0) this.timer.unref?.()
  }

  /** Lets an idle link keep no process alive, as the idle connections of fetch do. */
  idle() {
    if (this.waiters.size > 0 || this.streams.size > 0) return
    this.carrier?.unref()
    this.timer?.unref?.()
  }

  /** Takes the link down after its carrier closed, and ends every exchange and stream in flight. */
  down(carrier) {
    if (this.carrier !== carrier) return
    this.carrier = null
    let cause
    try {
      this.module.x.link_down(this.engine)
      this.pump()
    } catch (error) {
      cause = error
    }
    this.end(linkError('LINK_DOWN', 'the link went down', cause))
  }

  /**
   * Ends the link at once after `error` met it where the event loop entered it, a data event, the
   * timer or an abort, since an exception thrown on there ends the process of Node.js: rejects
   * each exchange and fails each stream with NO_MEMORY for a refused buffer and with LINK_DOWN
   * otherwise, the fault as the cause, and closes the carrier. The next request opens a carrier of
   * the package again, while a byte stream of the caller's own stays closed and answers NO_LINK.
   */
  fail(error) {
    const carrier = this.carrier
    this.carrier = null
    clearTimeout(this.timer)
    this.timer = null
    this.end(linkFault(error))
    try {
      this.module.x.link_down(this.engine)
      this.pump()
    } catch {
      // A module that faults again is dropped, and the drop ends the rest.
    }
    carrier?.close()
  }

  /** Rejects every exchange and fails every stream of the link with `error`. */
  end(error) {
    for (const waiter of this.waiters.values()) waiter.reject(error)
    this.waiters.clear()
    for (const stream of this.streams.values()) stream.fail(error)
    this.streams.clear()
  }

  /** Closes the carrier and drops the engine. */
  close() {
    this.closed = true
    const carrier = this.carrier
    if (carrier !== null) {
      carrier.close()
      this.down(carrier)
    }
    if (this.timer !== null) clearTimeout(this.timer)
    this.timer = null
    if (this.engine !== 0) this.module.x.engine_free(this.engine)
    this.engine = 0
  }
}

/** Returns `error` where it names its code, and otherwise the LINK_DOWN whose cause it is. */
function linkFault(error) {
  return typeof error?.cause?.code === 'string' ? error : linkError('LINK_DOWN', 'the link ended on a fault', error)
}
