// SPDX-License-Identifier: MIT
/*
 * graphtp lib/response: the Response the function builds from an answer of GraphTP, the event
 * stream of a 200.3, and its own answers 405, 406 and 415.
 *
 * The rules are those of docs/graphtp-fetch.md section 6: the status is HTTP's, the statusText the
 * name the registry gives the code, the header graphtp-status carries status.substatus, 200.0 too,
 * and an answer with a body carries the answer type of GraphTP with the length of the plain body,
 * while an answer with an empty body carries graphtp-status alone and no content-type, whatever its
 * code.
 */

/** The answer type of GraphTP, the one media type of every body the function hands on. */
export const ANSWER_TYPE = 'application/graphql-response+json; charset=utf-8'

/** The statuses whose answer carries no body by the Fetch standard. */
const NULL_BODY = new Set([204, 205, 304])

/**
 * Returns the Response of the answer `answer`, its status, its substatus, the name of its code and
 * its decoded body, byte for byte the service's.
 */
export function answerResponse(answer) {
  const headers = new Headers()
  let body = null
  if (answer.body.length > 0 && !NULL_BODY.has(answer.status)) {
    headers.set('content-type', ANSWER_TYPE)
    headers.set('content-length', String(answer.body.length))
    body = answer.body
  }
  headers.set('graphtp-status', answer.status + '.' + answer.substatus)
  return new Response(body, {status: answer.status, statusText: answer.name, headers})
}

/**
 * Returns the Response of a stream, 200 with the name of 200.3 as its statusText and the event
 * stream `readable` as its body under text/event-stream, with cache-control no-cache and
 * graphtp-status 200.3 (docs/graphtp-fetch.md sections 6 and 7).
 */
export function streamResponse(readable, statusText) {
  const headers = new Headers()
  headers.set('content-type', 'text/event-stream')
  headers.set('cache-control', 'no-cache')
  headers.set('graphtp-status', '200.3')
  return new Response(readable, {status: 200, statusText, headers})
}

/** Returns the function's own answer of the status `status`, with no field and no body. */
export function ownResponse(status, statusText) {
  return new Response(null, {status, statusText})
}
