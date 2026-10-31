// SPDX-License-Identifier: MIT
/*
 * graphtp lib/links/helper: the link of graphtp:// over the helper homa-raw, which offers the
 * client role of the Network profile over a Unix stream socket.
 *
 * The carrier lands with a later release, with the devices and the benches it is judged on,
 * so the link refuses with NO_LINK until then.
 */

import {linkError} from '../engine.js'

/** Refuses to open the link the URL `url` names, whose carrier lands with a later release. */
export async function open(url) {
  throw linkError('NO_LINK', 'the carrier of ' + url.protocol + ' lands with a later release')
}
