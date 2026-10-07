// SPDX-License-Identifier: MIT
/*
 * graphtp lib/links/web-bluetooth: the link of graphtp+ble:// over Web Bluetooth in a browser,
 * one Homa packet per ATT value.
 */

import {linkError} from '../engine.js'

export async function open(url) {
  throw linkError('NO_LINK', 'the carrier of ' + url.protocol + ' lands with batch G8')
}
