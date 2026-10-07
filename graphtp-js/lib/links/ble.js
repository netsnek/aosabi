// SPDX-License-Identifier: MIT
/*
 * graphtp lib/links/ble: the link of graphtp+ble:// over the optional peer node-ble over BlueZ
 * in Node.js.
 */

import {linkError} from '../engine.js'

export async function open(url) {
  throw linkError('NO_LINK', 'the carrier of ' + url.protocol + ' lands with batch G8')
}
