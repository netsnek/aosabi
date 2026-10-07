// SPDX-License-Identifier: MIT
/*
 * graphtp lib/links/unix: the link graphtp+unix, a stream socket of the server opened with
 * node:net, which carries one Homa packet per frame of prefixed AoFrame.
 *
 * A Unix Socket server of GraphTP offers a stream socket of the prefixed framing beside
 * SOCK_SEQPACKET, and Node.js reaches it with node:net, which opens stream sockets alone
 * (docs/graphtp-fetch.md section 9, and docs/graphtp.md sections 4.2 and 11). The module of the
 * package frames and unframes, and this file moves the bytes.
 */

import {linkError} from '../engine.js'

/** The profile of the module for a Unix Socket. */
const PROFILE_UNIX_SOCKET = 0

/** The framing of the module for a stream socket, prefixed AoFrame. */
const FRAMING_PREFIXED = 0

/** The sight of the module for a link whose end both sides see. */
const SIGHT_SEEN = 0

/**
 * Opens the socket the URL `url` names, the whole path of a graphtp+unix URL with an empty
 * authority, and resolves to its carrier, or rejects with NO_LINK.
 */
export async function open(url) {
  if (url.host !== '') throw linkError('NO_LINK', 'a graphtp+unix URL has an empty authority: ' + url.href)
  const path = decodeURIComponent(url.pathname)
  const net = await import('node:net')
  const socket = await new Promise((resolve, reject) => {
    const opened = net.connect({path})
    const refuse = error => {
      opened.destroy()
      reject(linkError('NO_LINK', 'the socket ' + path + ' does not open: ' + (error.code ?? error.message)))
    }
    opened.once('error', refuse)
    opened.once('connect', () => {
      opened.off('error', refuse)
      resolve(opened)
    })
  })
  socket.setNoDelay?.(true)
  const carrier = {
    profile: PROFILE_UNIX_SOCKET,
    param: 0,
    framing: FRAMING_PREFIXED,
    sight: SIGHT_SEEN,
    onData: null,
    onClose: null,
    send(bytes) {
      if (!socket.destroyed) socket.write(bytes)
    },
    close() {
      socket.destroy()
    },
    ref() {
      socket.ref()
    },
    unref() {
      socket.unref()
    }
  }
  socket.on('data', chunk => carrier.onData?.(new Uint8Array(chunk.buffer, chunk.byteOffset, chunk.byteLength)))
  // An error closes the socket, and the close takes the link down.
  socket.on('error', () => {})
  socket.on('close', () => carrier.onClose?.())
  return carrier
}
