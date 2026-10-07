// SPDX-License-Identifier: MIT
/*
 * graphtp lib/links/webusb: the link of graphtp+usb:// over WebUSB in a browser, one Homa packet
 * per bulk transfer.
 */

import {linkError} from '../engine.js'

export async function open(url) {
  throw linkError('NO_LINK', 'the carrier of ' + url.protocol + ' lands with batch G8')
}
