// SPDX-License-Identifier: MIT
/*
 * graphtp lib/links/stream: the link over a byte stream of the caller's own, a readable and a
 * writable stream of bytes that connect takes, with delimited AoFrame.
 *
 * A caller who holds a byte stream of its own, a serial port it opened itself among them, hands it
 * to connect, which names the link by a local name (docs/graphtp-fetch.md section 3). The stream may
 * lose bytes, so its frames are delimited, and the server may not see the end of an earlier run, so
 * the link keeps the start rule.
 */

import {linkError} from '../engine.js'

/** The profile of the module for USB serial, packets of 512 bytes. */
const PROFILE_USB_SERIAL = 1

/** The framing of the module for a stream that may lose bytes, delimited AoFrame. */
const FRAMING_DELIMITED = 1

/** The sight of the module for a link whose end the server may not see. */
const SIGHT_UNSEEN = 1

/**
 * Returns a chunk of the caller's stream as a Uint8Array, a view of the bytes of an ArrayBuffer or
 * another view, and a copy of anything else, so the glue reads the length of the bytes it copies.
 */
function bytesOf(value) {
  if (value instanceof Uint8Array) return value
  if (ArrayBuffer.isView(value)) return new Uint8Array(value.buffer, value.byteOffset, value.byteLength)
  return new Uint8Array(value)
}

/**
 * Returns the carrier over the streams `readable` and `writable` of the caller, or throws NO_LINK
 * where either is locked, held by a reader or a writer of the caller or by the carrier that closed
 * them, so a link whose stream closed answers NO_LINK (docs/graphtp-fetch.md section 8).
 */
export function open({readable, writable}) {
  if (readable.locked || writable.locked) throw linkError('NO_LINK', 'the byte stream of the caller is closed or held')
  const writer = writable.getWriter()
  const reader = readable.getReader()
  let open = true
  const carrier = {
    profile: PROFILE_USB_SERIAL,
    param: 0,
    framing: FRAMING_DELIMITED,
    sight: SIGHT_UNSEEN,
    onData: null,
    onClose: null,
    send(bytes) {
      if (open) writer.write(bytes).catch(() => carrier.close())
    },
    close() {
      if (!open) return
      open = false
      reader.cancel().catch(() => {})
      writer.close().catch(() => {})
      carrier.onClose?.()
    },
    ref() {},
    unref() {}
  }
  const read = async () => {
    try {
      for (;;) {
        const {value, done} = await reader.read()
        if (done) break
        carrier.onData?.(bytesOf(value))
      }
    } catch {
      // A stream that fails ends the link as one that ends does.
    }
    carrier.close()
  }
  read()
  return carrier
}
