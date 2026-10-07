// SPDX-License-Identifier: MIT
/*
 * graphtp lib/links/helper: the link of graphtp:// over the helper homa-raw, which offers the
 * client role of the Network profile over a Unix stream socket, section 9.1.
 */

import {linkError} from '../engine.js'

export async function open(url) {
  throw linkError('NO_LINK', 'the carrier of ' + url.protocol + ' lands with batch G8')
}
