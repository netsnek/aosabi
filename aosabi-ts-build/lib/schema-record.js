// SPDX-License-Identifier: MIT
/*
 * aosabi-build lib/schema-record: the schema record of a build, which the boot of an app reads in
 * place of the IR with no lexer, no parser and no validation of the schema at its start.
 *
 * No analogue in the reference, whose Worker builds its schema from the SDL text at every start.
 */

import fs from 'node:fs'

/** The step of the export that writes the record of the SDL and the IR, crates/aosabi-js/src/prebuilt.rs. */
const WRITE = 0

/** The step of a test build with the feature print that prints the schema of a record and of its SDL. */
const PRINT = 1

const encoder = new TextEncoder()
const decoder = new TextDecoder('utf-8', {fatal: true})

/**
 * Resolves to an instance of the module at `modulePath` whose imports refuse every call, since the
 * steps of the schema record call no function of the host.
 */
async function instance(modulePath) {
  const module = await WebAssembly.compile(fs.readFileSync(modulePath))
  const imports = {}
  for (const {module: owner, name, kind} of WebAssembly.Module.imports(module)) {
    if (kind !== 'function') continue
    imports[owner] ??= {}
    imports[owner][name] = () => {
      throw new Error(`the schema record of a build calls no function of the host, and it called ${name}`)
    }
  }
  return (await WebAssembly.instantiate(module, imports)).exports
}

/** Returns the record of the step `step` with the texts `texts`, each with its length in four bytes. */
function stepRecord(step, texts) {
  const encoded = texts.map(text => encoder.encode(text))
  const bytes = new Uint8Array(1 + encoded.reduce((sum, text) => sum + 4 + text.length, 0))
  const view = new DataView(bytes.buffer)
  bytes[0] = step
  let at = 1
  for (const text of encoded) {
    view.setUint32(at, text.length, true)
    bytes.set(text, at + 4)
    at += 4 + text.length
  }
  return bytes
}

/**
 * Resolves to the answer of the step `step` over `texts` of the module at `modulePath`: the bytes
 * the step answers, or throws its message.
 */
async function runStep(modulePath, step, texts) {
  const exports = await instance(modulePath)
  if (typeof exports.schema_record !== 'function') {
    throw new Error(`the module ${modulePath} exports no schema_record, so it writes no schema record`)
  }
  const record = stepRecord(step, texts)
  const address = exports.alloc(record.length)
  if (address === 0) throw new Error('the module made no input buffer for the record of the step')
  new Uint8Array(exports.memory.buffer, address, record.length).set(record)
  const answer = exports.schema_record(record.length)
  const length = new DataView(exports.memory.buffer).getUint32(answer, true)
  const bytes = new Uint8Array(exports.memory.buffer.slice(answer + 4, answer + 4 + length))
  if (bytes[0] !== 0) throw new Error(new TextDecoder().decode(bytes.subarray(1)))
  return bytes.subarray(1)
}

/**
 * Resolves to the schema record of the SDL `sdl` and the JSON text of the IR `ir` of one build, a
 * string, which the module at `module`, the core.wasm of the package the app boots on, writes.
 * Throws the message of the engine where the build takes no record, such as a schema that is not
 * valid, which boots from its SDL.
 */
export async function schemaRecord({sdl, ir, module}) {
  return decoder.decode(await runStep(module, WRITE, [sdl, typeof ir === 'string' ? ir : JSON.stringify(ir)]))
}

/** Returns the text of a module of JavaScript whose default export is `record`, a record or null. */
export function recordModule(record, why) {
  const head = '// The schema record of the build, written by lib/schema-record.js of aosabi-build.\n'
  if (record === null) return `${head}// The build takes none: ${why.replace(/\s+/g, ' ')}\nexport default null;\n`
  return `${head}export default ${JSON.stringify(record)};\n`
}

/**
 * Writes the module of JavaScript at `out` whose default export is the schema record of the SDL
 * `sdl` and the IR `ir` that the module at `module` writes, or null with the message of the engine
 * where the build takes no record, and resolves to {record, why}.
 */
export async function writeSchemaRecord({sdl, ir, module, out}) {
  let record = null
  let why = null
  try {
    record = await schemaRecord({sdl, ir, module})
  } catch (error) {
    why = String(error?.message ?? error)
  }
  fs.writeFileSync(out, recordModule(record, why))
  return {record, why}
}

/**
 * Resolves to the three schemas a test build with the feature print of crates/aosabi-js prints of
 * the record `record` of the SDL `sdl`, {record, sdl, unbound}: the schema the record builds and the
 * schema the SDL builds, each with the scalars every boot binds and followed by its type map and its
 * directives, and the schema the record builds with no bindings.
 */
export async function printSchemaRecord({sdl, record, module}) {
  const bytes = await runStep(module, PRINT, [sdl, record])
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength)
  const texts = []
  let at = 0
  while (at < bytes.length) {
    const length = view.getUint32(at, true)
    texts.push(decoder.decode(bytes.subarray(at + 4, at + 4 + length)))
    at += 4 + length
  }
  const [built, fromSdl, unbound] = texts
  return {record: built, sdl: fromSdl, unbound}
}
