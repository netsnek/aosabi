// SPDX-License-Identifier: MIT
/*
 * graphtp lib/links/usb: the link of graphtp+usb:// over the optional peer usb, whose webusb
 * export runs the WebUSB driver in Node.js.
 *
 * The carrier lands with a later release, with the devices and the benches it is judged on,
 * so the link refuses with NO_LINK until then.
 */

import {linkError} from '../engine.js'

/** Refuses to open the link the URL `url` names, whose carrier lands with a later release. */
export async function open(url) {
  throw linkError('NO_LINK', 'the carrier of ' + url.protocol + ' lands with a later release')
}
