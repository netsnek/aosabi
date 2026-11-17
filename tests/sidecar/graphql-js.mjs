// SPDX-License-Identifier: MIT
/*
 * graphql-js: the oracle side of the comparison of the language, type and
 * validation layers.
 *
 * It runs graphql 16.12.0 at its pin and answers one newline delimited JSON
 * request per line on stdin with one JSON response per line on stdout, so that the
 * notebooks judge parse, print, validate, build_schema, print_schema,
 * natural_compare and strip against the library itself rather than against a
 * server. A defect in the Rust language layer is therefore separated from a defect
 * in the HTTP layer, which is why the sidecar exists.
 *
 * The operation introspect answers the reading a client gets of an sdl, which is
 * the oracle of the model the documentation generator reads. The operation strip
 * answers stripIgnoredCharacters, which prints the query of every request a
 * federation sends.
 *
 * The operations locate and print_location answer getLocation and
 * printSourceLocation, the oracle of the table of line starts and of the excerpt
 * of an error.
 *
 * The operations introspection_query, introspection_from_schema and client_schema
 * are the oracle of the two utilities of graphql-js that the command line of
 * Aosabi ports, the introspection query `pull` posts and the client schema it
 * builds from the answer. The operation introspection_from_schema answers under a
 * name of its own, since the operation introspect answers another query.
 *
 * It binds no port, starts no server, reaches no network, reads one file, the
 * manifest of the package it verifies, and writes nothing anywhere.
 *
 * Exact command, from the repository root:
 *
 *   NODE_OPTIONS=--no-network-family-autoselection \
 *     node tests/sidecar/graphql-js.mjs
 *
 * A sidecar that serves more operations imports `OPERATIONS` and `serve` from
 * this module and serves a table of its own. The module serves only when node
 * runs it as its entry, so the import starts no loop.
 *
 * Each operation below says what its request carries and what it answers.
 */

import fs from 'node:fs'
import {createInterface} from 'node:readline'
import {fileURLToPath} from 'node:url'

import {
  Source,
  buildClientSchema,
  buildSchema,
  getIntrospectionQuery,
  getLocation,
  graphqlSync,
  introspectionFromSchema,
  parse,
  print,
  printSchema,
  printSourceLocation,
  specifiedRules,
  stripIgnoredCharacters,
  validate,
  version,
} from './node_modules/graphql/index.mjs'
import {naturalCompare} from './node_modules/graphql/jsutils/naturalCompare.mjs'

/** The graphql version this sidecar is the oracle of. */
const PINNED_VERSION = '16.12.0'

/** The manifest of the imported package, read beside the module it describes. */
const MANIFEST_URL = new URL(
  './node_modules/graphql/package.json',
  import.meta.url,
)

/** The exit status of a wrong invocation, which is the convention of `sysexits.h`. */
const EXIT_USAGE = 64

/** The exit status of a package that is not the pin the goldens were taken with. */
const EXIT_UNPINNED = 70

/**
 * A request the sidecar refuses to answer, as opposed to a GraphQL error.
 *
 * It becomes a `fatal` response line, so that a harness mistake such as a missing
 * field or an sdl that does not build stays distinguishable from a document the
 * library rejects.
 */
class RequestError extends Error {}

/** Reports whether `value` is an object, which excludes `null` as `typeof` does not. */
function isObject(value) {
  return value !== null && typeof value === 'object'
}

/** The message of a thrown value, whatever kind of value it is. */
function messageOf(error) {
  return error instanceof Error ? error.message : String(error)
}

/**
 * Serializes one error the way the wire carries it.
 *
 * A `GraphQLError` renders through its own `toJSON`, which writes `message`,
 * `locations`, `path` and `extensions` in that order, omits a key that is absent
 * and omits an empty `extensions`. Anything else that carries a string `message`,
 * which includes a plain `Error` and an object a sidecar builds for a thrown value
 * that is no error, renders as `message` alone.
 */
export function serializeError(error) {
  if (isObject(error) && typeof error.toJSON === 'function') return error.toJSON()
  if (isObject(error) && typeof error.message === 'string') return {message: error.message}
  return {message: String(error)}
}

/** The string field `name` of `request`, or a `RequestError` naming it. */
export function requireString(request, name) {
  const value = request[name]
  if (typeof value !== 'string') {
    throw new RequestError(`${request.op}: the request carries no string field ${name}`)
  }
  return value
}

/**
 * The schema of `sdl`, or a `RequestError` naming the stage that failed.
 *
 * An sdl that does not build is a harness mistake here, because `build_schema` is
 * the operation that asks whether it builds.
 */
function schemaOf(request) {
  try {
    return buildSchema(requireString(request, 'sdl'))
  } catch (error) {
    if (error instanceof RequestError) throw error
    throw new RequestError(`${request.op}: the sdl does not build: ${messageOf(error)}`)
  }
}

/**
 * The document of `request`, or a `RequestError` naming the stage that failed.
 *
 * A document that does not parse is a harness mistake here, because `parse` is the
 * operation that asks whether it parses.
 */
function documentOf(request) {
  const source = requireString(request, 'document')
  try {
    return parse(source, request.options)
  } catch (error) {
    throw new RequestError(`${request.op}: the document does not parse: ${messageOf(error)}`)
  }
}

/** The specified rule named `name`, or a `RequestError` naming the miss. */
function ruleNamed(name) {
  const rule = specifiedRules.find(candidate => candidate.name === name)
  if (rule === undefined) {
    throw new RequestError(`validate: no specified rule is named ${name}`)
  }
  return rule
}

/**
 * Parses `document` and returns the rendered abstract syntax tree.
 *
 * `ast` is the exact output of `JSON.stringify(ast)` as a string, so the bytes the
 * reference renders survive the transport unchanged and a reader who wants the
 * tree parses that string. A syntax error is one `GraphQLError`, which is what
 * the parser throws on the first one it meets.
 */
function opParse(request) {
  const source = requireString(request, 'document')
  try {
    return {ast: JSON.stringify(parse(source, request.options))}
  } catch (error) {
    return {errors: [serializeError(error)]}
  }
}

/**
 * Prints an abstract syntax tree back to GraphQL source.
 *
 * The tree comes from `ast` when the request carries one, as an object or as the
 * string a `parse` response carries, and from parsing `document` otherwise.
 */
function opPrint(request) {
  if (request.ast !== undefined) {
    const ast = typeof request.ast === 'string' ? JSON.parse(request.ast) : request.ast
    return {printed: print(ast)}
  }
  const source = requireString(request, 'document')
  try {
    return {printed: print(parse(source, request.options))}
  } catch (error) {
    return {errors: [serializeError(error)]}
  }
}

/**
 * Validates `document` against the schema of `sdl`.
 *
 * All twenty seven specified rules run unless `rule` names one, in which case that
 * rule runs alone, which is how one generated invalid document is attributed to
 * one rule.
 *
 * `options` travels to `parse` inside `documentOf` and to `validate` here, which
 * reads `maxErrors` out of it and stops the run at that many errors, a hundred
 * when the request names none.
 */
function opValidate(request) {
  const schema = schemaOf(request)
  const document = documentOf(request)
  const rules = request.rule === undefined ? specifiedRules : [ruleNamed(request.rule)]
  return {errors: validate(schema, document, rules, request.options).map(serializeError)}
}

/**
 * Builds the schema of `sdl` and reports what `buildSchema` throws.
 *
 * `errors` is empty when the schema builds. It carries one entry otherwise,
 * because `assertValidSDL` joins every sdl problem into one error with a blank
 * line between the messages.
 */
function opBuildSchema(request) {
  const sdl = requireString(request, 'sdl')
  try {
    buildSchema(sdl)
  } catch (error) {
    return {errors: [serializeError(error)]}
  }
  return {errors: []}
}

/** Prints the schema of `sdl` through the graphql-js printer. */
function opPrintSchema(request) {
  const sdl = requireString(request, 'sdl')
  try {
    return {printed: printSchema(buildSchema(sdl))}
  } catch (error) {
    return {errors: [serializeError(error)]}
  }
}

/**
 * Compares `a` and `b` in the natural sort order of graphql-js.
 *
 * `order` is the raw return value and not a sign, because the function returns the
 * difference of the two lengths when no character and no digit run decides first.
 */
function opNaturalCompare(request) {
  return {order: naturalCompare(requireString(request, 'a'), requireString(request, 'b'))}
}

/**
 * The introspection query of the reading a client gets, with the four options
 * below switched on and `schemaDescription` and `oneOf` left at their default of
 * off.
 */
const INTROSPECTION_QUERY = getIntrospectionQuery({
  descriptions: true,
  specifiedByUrl: true,
  directiveIsRepeatable: true,
  inputValueDeprecation: true,
})

/**
 * Builds the schema of `sdl` and answers the introspection query against it.
 *
 * `introspection` is the exact output of `JSON.stringify` of the execution result,
 * `{"data":{"__schema":..}}`, as a string, for the reason `parse` gives: the bytes
 * the reference renders survive the transport, and a golden written from it is
 * the reference's own rendering. An sdl that does not build answers `errors` as
 * `print_schema` does, and the execution result carries its own `errors` when the
 * query fails, which a valid schema never does.
 */
function opIntrospect(request) {
  const sdl = requireString(request, 'sdl')
  let schema
  try {
    schema = buildSchema(sdl)
  } catch (error) {
    return {errors: [serializeError(error)]}
  }
  return {introspection: JSON.stringify(graphqlSync({schema, source: INTROSPECTION_QUERY}))}
}

/**
 * Strips the ignored characters of `source`, as the HTTP executor of a federation
 * does with every printed document it sends.
 *
 * `stripped` is the text, and a source the lexer refuses answers `errors` with
 * that one syntax error. The function appends to its text while it lexes, but a
 * lexer error throws before it returns, so the partial text is never answered.
 */
function opStrip(request) {
  const source = requireString(request, 'source')
  try {
    return {stripped: stripIgnoredCharacters(source)}
  } catch (error) {
    return {errors: [serializeError(error)]}
  }
}

/**
 * The options object of `request`, or `undefined` when the request names none.
 *
 * The object travels to the utility unchanged, so a request with no options
 * answers what the utility answers with its defaults.
 */
function optionsOf(request) {
  const options = request.options
  if (options === undefined) return undefined
  if (!isObject(options) || Array.isArray(options)) {
    throw new RequestError(`${request.op}: the options field is not an object`)
  }
  return options
}

/** The class name of a thrown value, `TypeError` for a message V8 writes. */
function nameOf(error) {
  return error instanceof Error ? error.name : typeof error
}

/**
 * The introspection query graphql-js builds, with the options of the request.
 *
 * With no options the query carries descriptions and leaves the other five options
 * off, which is the query `pull` posts.
 */
function opIntrospectionQuery(request) {
  return {query: getIntrospectionQuery(optionsOf(request))}
}

/**
 * Builds the schema of `sdl` and answers `introspectionFromSchema` of it.
 *
 * `introspection` is the exact output of `JSON.stringify` of the result, which is
 * the `data` member of an execution result, `{"__schema":..}`, the input
 * `client_schema` reads. With no options the utility switches on the five options
 * beyond descriptions. The operation answers under a name of its own, since the
 * operation introspect above answers another query.
 */
function opIntrospectionFromSchema(request) {
  const sdl = requireString(request, 'sdl')
  let schema
  try {
    schema = buildSchema(sdl)
  } catch (error) {
    return {errors: [serializeError(error)]}
  }
  return {introspection: JSON.stringify(introspectionFromSchema(schema, optionsOf(request)))}
}

/**
 * Builds the client schema of an introspection result and prints it.
 *
 * `introspection` is the JSON text of the `data` member of an execution result,
 * parsed here and handed to `buildClientSchema` with the options of the request. A
 * result the utility refuses answers `errors` with its message and `error_name`
 * with the class of what it threw, `Error` for a check of graphql-js and
 * `TypeError` for a member of the wrong shape that V8 meets first.
 */
function opClientSchema(request) {
  const text = requireString(request, 'introspection')
  let introspection
  try {
    introspection = JSON.parse(text)
  } catch (error) {
    throw new RequestError(`client_schema: the introspection field is not JSON: ${messageOf(error)}`)
  }
  try {
    return {printed: printSchema(buildClientSchema(introspection, optionsOf(request)))}
  } catch (error) {
    return {errors: [serializeError(error)], error_name: nameOf(error)}
  }
}

/** The array field `positions` of `request`, or a `RequestError` naming it. */
function positionsOf(request) {
  const positions = request.positions
  if (!Array.isArray(positions)) {
    throw new RequestError(`${request.op}: the request carries no array field positions`)
  }
  return positions
}

/**
 * Locates every position of `positions` in `document`.
 *
 * `locations` holds the line and the column `getLocation` answers for each
 * position as a pair, in order. A request may carry `walk`, which the Rust
 * side reads to answer by its second judge, and which this oracle leaves
 * unread, since the reference locates one way.
 */
function opLocate(request) {
  const source = new Source(requireString(request, 'document'))
  return {
    locations: positionsOf(request).map(position => {
      const {line, column} = getLocation(source, position)
      return [line, column]
    }),
  }
}

/**
 * Prints the excerpt of every position of `positions` in `document`.
 *
 * Each position is located and printed by `printSourceLocation`, which is what
 * `printLocation` prints for a node that starts there. A request that carries
 * `name` and `locationOffset` builds the source with them.
 */
function opPrintLocation(request) {
  const source = new Source(requireString(request, 'document'), request.name, request.locationOffset)
  return {
    printed: positionsOf(request).map(position => printSourceLocation(source, getLocation(source, position))),
  }
}

/** The operations, on a null prototype so that no inherited key answers a request. */
export const OPERATIONS = {
  __proto__: null,
  parse: opParse,
  print: opPrint,
  validate: opValidate,
  build_schema: opBuildSchema,
  print_schema: opPrintSchema,
  natural_compare: opNaturalCompare,
  introspect: opIntrospect,
  strip: opStrip,
  introspection_query: opIntrospectionQuery,
  introspection_from_schema: opIntrospectionFromSchema,
  client_schema: opClientSchema,
  locate: opLocate,
  print_location: opPrintLocation,
}

/** The response object for one request line to `operations`, `id` and `op` first and payload after. */
function answer(operations, line) {
  let request
  try {
    request = JSON.parse(line)
  } catch (error) {
    return {id: null, op: null, fatal: `the request line is not JSON: ${messageOf(error)}`}
  }
  if (!isObject(request) || Array.isArray(request)) {
    return {id: null, op: null, fatal: 'the request line is not a JSON object'}
  }
  const id = request.id === undefined ? null : request.id
  const op = typeof request.op === 'string' ? request.op : null
  const operation = op === null ? undefined : operations[op]
  if (operation === undefined) {
    return {id, op, fatal: `unknown op ${JSON.stringify(request.op)}`}
  }
  if (request.id === undefined) {
    return {id, op, fatal: 'the request carries no id'}
  }
  try {
    return {id, op, ...operation(request)}
  } catch (error) {
    return {id, op, fatal: messageOf(error)}
  }
}

/**
 * Refuses to answer as an oracle when the package is not the pinned one, with
 * the message of the sidecar `name`.
 */
function assertPinnedVersion(name) {
  const manifest = JSON.parse(fs.readFileSync(MANIFEST_URL, 'utf8'))
  if (manifest.version === PINNED_VERSION && version === PINNED_VERSION) return
  process.stderr.write(
    `${name}: expected graphql ${PINNED_VERSION}, ` +
      `found ${manifest.version} in the manifest and ${version} in the module\n`,
  )
  process.exit(EXIT_UNPINNED)
}

/**
 * Serves `operations` on stdin and stdout as the sidecar `name`, its path from
 * the repository root, which the usage line and the refusal of the pin name.
 *
 * One line in, one line out, in order. Every operation is synchronous, so the
 * loop needs no queue to keep a response beside its request.
 */
export async function serve(operations, name) {
  if (process.argv.length > 2) {
    process.stderr.write(`usage: node ${name} < requests.ndjson\n`)
    process.exit(EXIT_USAGE)
  }

  assertPinnedVersion(name)

  const lines = createInterface({input: process.stdin, crlfDelay: Infinity})
  for await (const line of lines) {
    if (line.trim() === '') continue
    process.stdout.write(`${JSON.stringify(answer(operations, line))}\n`)
  }
}

/** Reports whether node runs this module as its entry rather than a sidecar importing it. */
function isEntry() {
  if (process.argv[1] === undefined) return false
  try {
    return fs.realpathSync(process.argv[1]) === fileURLToPath(import.meta.url)
  } catch {
    return false
  }
}

if (isEntry()) await serve(OPERATIONS, 'tests/sidecar/graphql-js.mjs')
