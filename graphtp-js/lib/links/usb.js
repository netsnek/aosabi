// SPDX-License-Identifier: MIT
/*
 * graphtp lib/links/usb: the link of graphtp+usb:// over the optional peer usb, whose webusb
 * export runs the WebUSB driver in Node.js.
 */

import {linkError} from '../engine.js'

export async function open(url) {
  throw linkError('NO_LINK', 'the carrier of ' + url.protocol + ' lands with batch G8')
}
