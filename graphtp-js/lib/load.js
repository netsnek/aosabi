// SPDX-License-Identifier: MIT
/*
 * graphtp lib/load: the loader of graphtp.wasm and the calls of its exports.
 *
 * Outside Node.js the loader fetches the file new URL('../graphtp.wasm', import.meta.url) names,
 * and in Node.js it reads the file with readFile of node:fs/promises behind a dynamic import that
 * runs in Node.js alone, since the fetch of Node.js refuses a file: URL and an import of a .wasm
 * module is experimental there (docs/graphtp-fetch.md section 2.2). The module imports nothing, so
 * it is instantiated with an empty import object, once per process or page.
 *
 * An exception that leaves a call of an export, a trap or a stack overflow of the host inside it,
 * may leave the module with a borrowed cell or a lower stack, so the loader drops it: the Module
 * calls no export again, the next load instantiates a fresh one and each listener of onDrop learns
 * of it. A buffer the module refuses throws NO_MEMORY and one of a dropped module LINK_DOWN, which
 * lib/engine.js turns into the end of the link where the event loop entered it
 * (docs/graphtp-fetch.md section 8).
 */

/** The promise of the one module, made on the first call of load and after a drop. */
let loading = null

/** The functions onDrop registered, called with the dropped Module and the exception. */
const dropListeners = new Set()

/** The events of a link, the kinds src/link.rs of the crate graphtp-wasm writes. */
export const EVENT = Object.freeze({
  RESULT: 1,
  UNKNOWN: 2,
  LINK_DOWN: 3,
  MALFORMED: 4,
  TOO_LARGE: 5,
  OPENED: 6,
  NEXT: 7,
  COMPLETE: 8,
  CANCELLED: 9,
  ABORTED: 10,
  LOST: 11,
  NO_LINK: 12,
  MEMORY: 13
})

/** The operations of the codec, the numbers src/codec.rs of the crate graphtp-wasm names. */
export const CODEC = Object.freeze({
  WRITE_OPERATION: 1,
  WRITE_PULL: 2,
  WRITE_CANCEL: 3,
  WRITE_ANSWER: 4,
  READ_REQUEST: 5,
  READ_ANSWER: 6,
  ENTRIES: 7,
  NAME: 8
})

/** The reasons of a malformed answer, by the number the module gives each. */
export const WIRE_ERRORS = Object.freeze(['', 'short', 'coding', 'empty', 'body', 'decode', 'bound', 'stream'])

/** Whether the code runs in Node.js, where the module file is read and not fetched. */
export function isNode() {
  return (
    typeof process === 'object' &&
    process !== null &&
    typeof process.versions === 'object' &&
    typeof process.versions.node === 'string' &&
    typeof globalThis.Deno === 'undefined'
  )
}

/** Returns the one module of the package, loading it on the first call and after a drop. */
export function load() {
  if (loading === null) {
    const made = instantiate().then(
      module => {
        module.loaded = made
        return module
      },
      error => {
        if (loading === made) loading = null
        throw error
      }
    )
    loading = made
  }
  return loading
}

/** Calls `listener` with the Module and the exception each time an exception drops a module. */
export function onDrop(listener) {
  dropListeners.add(listener)
}

/** Reads, compiles and instantiates graphtp.wasm. */
async function instantiate() {
  const url = new URL('../graphtp.wasm', import.meta.url)
  let bytes
  if (isNode()) {
    const {readFile} = await import('node:fs/promises')
    bytes = await readFile(url)
  } else {
    const response = await fetch(url)
    if (!response.ok) throw new TypeError('graphtp: graphtp.wasm answered ' + response.status)
    bytes = await response.arrayBuffer()
  }
  const compiled = await WebAssembly.compile(bytes)
  const instance = await WebAssembly.instantiate(compiled, {})
  return new Module(instance.exports)
}

/** The time of the host in nanoseconds, the clock of every link. */
export function now() {
  return BigInt(Math.floor(performance.now() * 1e6))
}

/**
 * The calls of the exports of graphtp.wasm, with the bytes moved in and out of its memory. `x`
 * holds each export behind the guard that drops the module on an exception, and `raw` the exports
 * themselves, where a test may put a fault that the guarded export then calls.
 */
export class Module {
  constructor(exports) {
    this.raw = {...exports}
    this.dead = false
    this.told = false
    this.fault = null
    this.loaded = null
    // A drop resolves this promise, since the handler of a stack overflow has room for the call of
    // a resolve function and little more, and its reaction tells the listeners in a microtask.
    this.ring = null
    new Promise(resolve => {
      this.ring = resolve
    }).then(() => this.tell())
    this.x = {}
    for (const [name, value] of Object.entries(exports)) {
      if (typeof value !== 'function') {
        this.x[name] = value
        continue
      }
      const slot = {fn: value}
      Object.defineProperty(this.raw, name, {
        get: () => slot.fn,
        set: fn => {
          slot.fn = fn
        },
        enumerable: true
      })
      this.x[name] = this.guard(slot, DEAD[name] ?? -1)
    }
  }

  /**
   * Returns the export of `slot` behind the guard, two closures built once that take seven
   * arguments, the most an export takes, so a call makes no array and no spread. A dead module
   * calls nothing and answers `dead`, what the module answers for no buffer and no link. The inner
   * closure marks the module dead on an exception, and the outer one resolves the drop one frame
   * higher, where more stack is left.
   */
  guard(slot, dead) {
    const call = (a, b, c, d, e, f, g) => {
      if (this.dead) {
        this.tell()
        return dead
      }
      try {
        return slot.fn(a, b, c, d, e, f, g)
      } catch (error) {
        this.dead = true
        this.fault = error
        if (loading === this.loaded) loading = null
        throw error
      }
    }
    return (a, b, c, d, e, f, g) => {
      try {
        return call(a, b, c, d, e, f, g)
      } catch (error) {
        if (this.dead) {
          try {
            this.ring()
          } catch {
            // The next call of the dead module tells the listeners.
          }
        }
        throw error
      }
    }
  }

  /** Tells every listener of onDrop once that the module is dropped. */
  tell() {
    if (!this.dead || this.told) return
    for (const listener of dropListeners) listener(this, this.fault)
    this.told = true
  }

  /**
   * Copies the `len` bytes of `bytes` into a new buffer of the module and returns its address, 0
   * for no byte, or throws NO_MEMORY where the module refuses the buffer and LINK_DOWN where it was
   * dropped. `len` is the length its caller read once and made a number, and a TypeError refuses
   * one that is no integer from 0 to 0xffffffff before any buffer is made, so the buffer, the view
   * and the call all take that one length, a longer source makes `set` throw before it writes, and
   * the buffer is dropped.
   */
  put(bytes, len) {
    if (!Number.isInteger(len) || len < 0 || len > 0xffffffff) {
      throw new TypeError('graphtp: the length of the bytes is no integer from 0 to 0xffffffff')
    }
    if (len === 0) return 0
    if (this.dead) throw linkError('LINK_DOWN', 'the module faulted and was dropped', this.fault)
    const addr = this.x.alloc(len) >>> 0
    if (addr === 0) throw linkError('NO_MEMORY', 'the module has no memory for ' + len + ' bytes')
    try {
      new Uint8Array(this.x.memory.buffer, addr, len).set(bytes)
    } catch (error) {
      this.drop(addr)
      throw error
    }
    return addr
  }

  /** Drops the buffer at `addr`, which a dropped module no longer holds. */
  drop(addr) {
    if (addr !== 0 && !this.dead) this.x.free(addr)
  }

  /** Returns a copy of the `len` bytes of the answer buffer. */
  answer(len) {
    if (len <= 0) return new Uint8Array(0)
    const addr = this.x.answer() >>> 0
    return new Uint8Array(this.x.memory.buffer, addr, len).slice()
  }

  /** Runs the operation `op` of the codec over `a` and `b` and the integers `x` and `y`. */
  codec(op, a = EMPTY, b = EMPTY, x = 0, y = 0) {
    const aLen = Number(a.length)
    const bLen = Number(b.length)
    const at = this.put(a, aLen)
    let bt = 0
    try {
      bt = this.put(b, bLen)
      const len = this.x.codec(op, at, aLen, bt, bLen, x >>> 0, y >>> 0)
      if (len < 0) throw new TypeError('graphtp: the codec refused operation ' + op + ' with ' + len)
      return this.answer(len)
    } finally {
      this.drop(at)
      this.drop(bt)
    }
  }

  /** Returns the text of statusText for a status and a substatus, the empty string for none. */
  statusText(status, substatus = 0) {
    const record = this.codec(CODEC.NAME, EMPTY, EMPTY, status, substatus)
    return record[0] === 0 ? latin1(record.subarray(1)) : ''
  }

  /** Builds a link and returns its number, 0 where the module cannot build it. */
  engineNew(profile, param, framing, maxMessage, maxBody, sight) {
    const seed = new BigUint64Array(1)
    globalThis.crypto.getRandomValues(seed)
    return this.x.engine_new(profile, param, framing, maxMessage >>> 0, maxBody >>> 0, sight, seed[0]) >>> 0
  }

  /** Hands the link `engine` the bytes its carrier received. */
  pushRx(engine, bytes) {
    const len = Number(bytes.length)
    const addr = this.put(bytes, len)
    try {
      return this.x.push_rx(engine, addr, len, now())
    } finally {
      this.drop(addr)
    }
  }

  /** Returns the bytes the carrier of the link `engine` sends now, empty for none. */
  pollTx(engine) {
    return this.answer(this.x.poll_tx(engine, now()))
  }

  /** Returns the next event of the link `engine`, or null. */
  pollEvent(engine) {
    const len = this.x.poll_event(engine)
    return len > 0 ? parseEvent(this.answer(len)) : null
  }

  /** Asks the link `engine` for an OPERATION and returns its token. */
  operation(engine, credential, content, repeat) {
    const cLen = Number(credential.length)
    const bLen = Number(content.length)
    const ct = this.put(credential, cLen)
    let bt = 0
    try {
      bt = this.put(content, bLen)
      return this.x.operation(engine, ct, cLen, bt, bLen, repeat ? 1 : 0)
    } finally {
      this.drop(ct)
      this.drop(bt)
    }
  }

  /** Returns when the link `engine` next needs its timers run, in nanoseconds, or -1n. */
  nextTimeout(engine) {
    return this.x.next_timeout(engine)
  }

  /** Runs the timers of the link `engine`. */
  handleTimeout(engine) {
    this.x.handle_timeout(engine, now())
  }
}

/** What a dead module answers where it is not -1, the answers of src/host.rs of graphtp-wasm. */
const DEAD = {alloc: 0, answer: 0, engine_new: 0, next_timeout: -1n, codec: -3}

/** The empty input of a call. */
const EMPTY = new Uint8Array(0)

/** Reads an event record of the module into its fields. */
function parseEvent(record) {
  const view = new DataView(record.buffer, record.byteOffset, record.byteLength)
  const nameLen = record[10]
  const len = view.getUint32(12, true)
  return {
    kind: record[0],
    substatus: record[1],
    status: view.getUint16(2, true),
    token: view.getUint32(4, true),
    handle: view.getUint16(8, true),
    name: latin1(record.subarray(16, 16 + nameLen)),
    bytes: record.subarray(16 + nameLen, 16 + nameLen + len)
  }
}

/** Returns the TypeError of a failure of the transport, whose cause names its code and fault. */
export function linkError(code, message, fault) {
  return new TypeError('graphtp: ' + message, {cause: fault === undefined ? {code} : {code, cause: fault}})
}

/** Returns the text of bytes that are each one character, as a reason phrase is. */
export function latin1(bytes) {
  let text = ''
  for (const byte of bytes) text += String.fromCharCode(byte)
  return text
}
