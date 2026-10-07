// SPDX-License-Identifier: MIT
/*
 * graphtp lib/request: what the function does with a request, the URL test and its repair, the
 * key of a link, the method, the content type, the fields and the content of a GET.
 */

/** The schemes of GraphTP, each with the carrier its link opens. */
export const SCHEMES = Object.freeze({
  'graphtp:': 'helper',
  'graphtp+unix:': 'unix',
  'graphtp+serial:': 'serial',
  'graphtp+usb:': 'usb',
  'graphtp+ble:': 'ble'
})

/** A scheme of GraphTP at the start of a string, and one directly after a single slash. */
const SCHEME_AT_START = /^graphtp(\+(unix|serial|usb|ble))?:\/\//i
const REPAIRABLE = /^\/graphtp(\+(unix|serial|usb|ble))?:\/\//i

/** The parameters of a GET that are the request, in the order the content writes them. */
const GET_PARAMETERS = ['query', 'operationName', 'variables', 'extensions']

/** The parameters of a GET whose value is JSON, parsed and written again. */
const JSON_PARAMETERS = new Set(['variables', 'extensions'])

/** The query parameters of a URL that are carrier options, part of the key of a link. */
const CARRIER_OPTIONS = ['serial', 'baud']

/**
 * Returns the URL of GraphTP the first argument of fetch names, or null for any other URL.
 *
 * A string that is one slash directly followed by a scheme of GraphTP and `://` is repaired by
 * dropping that slash, since executor-http prefixes an endpoint that does not start with http with
 * a dummy host for a GET and strips the host again. Any other relative URL is not GraphTP's.
 */
export function graphtpUrl(input) {
  let text
  if (typeof input === 'string') {
    text = input
  } else if (input instanceof URL) {
    text = input.href
  } else if (input !== null && typeof input === 'object' && typeof input.url === 'string') {
    text = input.url
  } else {
    return null
  }
  if (REPAIRABLE.test(text)) text = text.slice(1)
  if (!SCHEME_AT_START.test(text)) return null
  const url = new URL(text)
  return Object.hasOwn(SCHEMES, url.protocol) ? url : null
}

/**
 * Returns the key of the link of the URL `url`: the scheme, the authority, the path where it names
 * the socket or the device, and the carrier options, never the parameters of a GET.
 */
export function linkKey(url) {
  let key = url.protocol + '//' + url.host
  const carrier = SCHEMES[url.protocol]
  if ((carrier === 'unix' || carrier === 'serial') && url.host === '') key += decodeURIComponent(url.pathname)
  const options = CARRIER_OPTIONS.filter(name => url.searchParams.has(name))
    .map(name => name + '=' + url.searchParams.get(name))
  if (options.length > 0) key += '?' + options.join('&')
  return key
}

/**
 * Reads the request the arguments of fetch describe: its method, its fields, its body as bytes, its
 * signal and its URL, the fields of `init` over those of a Request.
 */
export async function readRequest(input, init = {}, url) {
  const base = typeof input === 'object' && input !== null && !(input instanceof URL) && typeof input.url === 'string'
    ? input
    : null
  const method = String(init.method ?? base?.method ?? 'GET').toUpperCase()
  const headers = new Headers(init.headers ?? base?.headers ?? undefined)
  const signal = init.signal ?? base?.signal ?? null
  let body
  if (init.body !== undefined && init.body !== null) {
    body = init.body
  } else if (base !== null && init.body === undefined && base.body !== null && typeof base.arrayBuffer === 'function') {
    body = await base.arrayBuffer()
  } else {
    body = null
  }
  return {url, method, headers, body, signal}
}

/**
 * Returns what the request `request` sends, the credential, the content and the mark, or the status
 * of the function's own answer, 405 for a method other than POST and GET and 415 for a body under a
 * type other than JSON.
 */
export async function contentOf(request) {
  const credential = latin1Bytes(request.headers.get('authorization') ?? '')
  const repeat = request.headers.get('graphtp-repeatable') === '1'
  if (request.method === 'GET') {
    return {credential, content: getContent(request.url), repeat}
  }
  if (request.method !== 'POST') return {status: 405}
  if (!jsonType(request.headers.get('content-type'), request.body)) return {status: 415}
  return {credential, content: await bytesOf(request.body), repeat}
}

/**
 * Returns the content of a GET, the parameters query, operationName, variables and extensions of
 * its URL in that order, variables and extensions parsed from their JSON and written again, and
 * nothing else. A value that is no JSON travels as its text, so the service answers it.
 */
export function getContent(url) {
  const request = {}
  for (const name of GET_PARAMETERS) {
    if (!url.searchParams.has(name)) continue
    const value = url.searchParams.get(name)
    if (JSON_PARAMETERS.has(name)) {
      try {
        request[name] = JSON.parse(value)
      } catch {
        request[name] = value
      }
    } else {
      request[name] = value
    }
  }
  return new TextEncoder().encode(JSON.stringify(request))
}

/** Tells whether a body of the type `type` travels as the content: JSON, a +json type, or none. */
function jsonType(type, body) {
  if (type === null) {
    return !(typeof FormData !== 'undefined' && body instanceof FormData) &&
      !(typeof URLSearchParams !== 'undefined' && body instanceof URLSearchParams) &&
      !(typeof Blob !== 'undefined' && body instanceof Blob && body.type !== '' && !jsonType(body.type, null))
  }
  const media = type.split(';')[0].trim().toLowerCase()
  return media === 'application/json' || media.endsWith('+json')
}

/** Returns the bytes of a body of fetch, byte for byte as its writer wrote it. */
async function bytesOf(body) {
  if (body === null || body === undefined) return new Uint8Array(0)
  if (typeof body === 'string') return new TextEncoder().encode(body)
  if (body instanceof Uint8Array) return body.slice()
  if (body instanceof ArrayBuffer) return new Uint8Array(body.slice(0))
  if (ArrayBuffer.isView(body)) {
    return new Uint8Array(body.buffer.slice(body.byteOffset, body.byteOffset + body.byteLength))
  }
  if (typeof Blob !== 'undefined' && body instanceof Blob) return new Uint8Array(await body.arrayBuffer())
  if (typeof body.getReader === 'function') return new Uint8Array(await new Response(body).arrayBuffer())
  return new TextEncoder().encode(String(body))
}

/** Returns the bytes of a field value, each character one byte, as the byte strings of fetch are. */
function latin1Bytes(text) {
  const bytes = new Uint8Array(text.length)
  for (let i = 0; i < text.length; i++) bytes[i] = text.charCodeAt(i) & 0xff
  return bytes
}
