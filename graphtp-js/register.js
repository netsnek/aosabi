// SPDX-License-Identifier: MIT
/*
 * graphtp/register: the scheme aware global fetch, which sends every URL of GraphTP through
 * graphtpFetch and every other URL through the fetch it replaced.
 *
 * The test of the URL is the one of graphtpFetch, the repaired form included, so a relative string
 * that is one slash before a scheme of GraphTP goes to GraphTP and any other relative URL to the
 * fetch it replaced. A process imports it once, as `import 'graphtp/register'` or
 * `node --import graphtp/register`.
 */

import {createGraphtpFetch} from './index.js'

/** The fetch this module replaces. */
const replaced = globalThis.fetch

globalThis.fetch = createGraphtpFetch({
  fallback: (input, init) => replaced.call(globalThis, input, init)
})
