// SPDX-License-Identifier: MIT
/*
 * graphtp lib/links/web-bluetooth: the link of graphtp+ble:// over Web Bluetooth in a browser,
 * one Homa packet per ATT value.
 *
 * The carrier lands with a later release, with the devices and the benches it is judged on,
 * so the link refuses with NO_LINK until then.
 */

import {linkError} from '../engine.js'

/** Refuses to open the link the URL `url` names, whose carrier lands with a later release. */
export async function open(url) {
  throw linkError('NO_LINK', 'the carrier of ' + url.protocol + ' lands with a later release')
}
