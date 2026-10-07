// SPDX-License-Identifier: MIT
/*
 * graphtp lib/links/web-serial: the link of graphtp+serial:// over Web Serial in a browser, with
 * delimited AoFrame.
 */

import {linkError} from '../engine.js'

export async function open(url) {
  throw linkError('NO_LINK', 'the carrier of ' + url.protocol + ' lands with batch G8')
}
