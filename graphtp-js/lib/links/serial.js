// SPDX-License-Identifier: MIT
/*
 * graphtp lib/links/serial: the link of graphtp+serial:// over the optional peer serialport with
 * delimited AoFrame in Node.js.
 */

import {linkError} from '../engine.js'

export async function open(url) {
  throw linkError('NO_LINK', 'the carrier of ' + url.protocol + ' lands with batch G8')
}
