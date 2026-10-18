// SPDX-License-Identifier: MIT
/*
 * aosabi-build lib/build: the command aosabi build, the author mode of the TypeScript option, which
 * infers the schema of an app over the live checker of typescript with the module infer.wasm,
 * transpiles its sources and writes the glue into .aosabi/.
 *
 * It fails on an app with no entry or no default export, on a refusal of the inference, on a source
 * with a syntax error, on a configuration that throws when it loads and on a schema that is no valid
 * GraphQL schema. The inference runs as the crate aosabi-infer inside infer.wasm, whose every question lib/checker.js answers from the checker of that program.
 */

import fs from 'node:fs'
import path from 'node:path'
import {pathToFileURL} from 'node:url'
import ts from 'typescript'
import {ROOTS, createSource} from './checker.js'
import {emitGlue, findConfigFile, transpileApp} from './glue.js'

/** The entry of an app and the folder of the build. */
const SFI_FILE_PATH = './src/index.ts'
const OUTPUT_DIR = './.aosabi'

/** The module of the inference beside this folder in the package. */
const INFER_WASM = new URL('../infer.wasm', import.meta.url)

/** A refusal of the step, whose message the command prints before it exits with 1. */
export class BuildError extends Error {}

/**
 * Returns the compiler options of the app whose entry is `sfiFilePath`, loadTsConfigOptions of
 * builder.ts:298-345: the options of the nearest tsconfig.json with strictNullChecks forced, or
 * the defaults below where none is found or none reads.
 */
function loadTsConfigOptions(sfiFilePath, warn) {
  const defaultOptions = {
    target: ts.ScriptTarget.ESNext,
    module: ts.ModuleKind.CommonJS,
    strict: true,
    esModuleInterop: true,
    skipLibCheck: false,
    forceConsistentCasingInFileNames: true,
    noImplicitAny: true
  }
  const configPath = ts.findConfigFile(path.dirname(sfiFilePath), ts.sys.fileExists, 'tsconfig.json')
  if (!configPath) return defaultOptions
  const configFile = ts.readConfigFile(configPath, ts.sys.readFile)
  if (configFile.error) {
    warn('Could not read tsconfig.json — using default compiler options')
    return defaultOptions
  }
  const parsedConfig = ts.parseJsonConfigFileContent(configFile.config, ts.sys, path.dirname(configPath))
  return {...parsedConfig.options, strictNullChecks: true}
}

/**
 * Returns the program of the app whose entry is `sfiFilePath` with its checker, the source file of
 * the entry and its default export, the constructor of the SchemaBuilder of builder.ts:240-296: the
 * TypeScript files of the folder of the entry and `aosabi.d.ts` beside that folder as the roots of
 * the program, and the default export of the entry the app it reads.
 */
function makeProgram(sfiFilePath, warn) {
  const filesInSfiDir = ts.sys
    .readDirectory(path.dirname(sfiFilePath), ['.ts'], ['.d.ts'])
    .concat([path.join(path.dirname(sfiFilePath), '..', 'aosabi.d.ts')])
  const options = loadTsConfigOptions(sfiFilePath, warn)
  const host = ts.createCompilerHost(options)
  const program = ts.createProgram(filesInSfiDir, options, host)
  const checker = program.getTypeChecker()
  const sfiFile = program.getSourceFiles().find(file => path.resolve(file.fileName) === sfiFilePath)
  if (!sfiFile) throw new BuildError('Could not find index.ts (aosabi entrypoint)')
  const sfiFileSymbol = checker.getSymbolAtLocation(sfiFile)
  const sfiFileExports = checker.getExportsOfModule(sfiFileSymbol)
  const sfi = sfiFileExports.find(s => s.escapedName === 'default')
  if (!sfi) {
    throw new BuildError('Aosabi entry must `export default` the app (e.g. `export default new App(...)`).')
  }
  return {program, checker, sfiFile, sfi, options}
}

/**
 * Returns the types of the three roots of the app, the first lines of build of builder.ts:347-394:
 * the type of the default export, or the type of its property graphql where it holds no root of its
 * own, and the type of each root property as the entry file sees it, undefined for none.
 */
function rootTypes({checker, sfiFile, sfi}) {
  let sfiType = checker.getTypeOfSymbolAtLocation(sfi, sfiFile)
  const hasRootOp = sfiType.getProperty('Query') || sfiType.getProperty('Mutation') || sfiType.getProperty('Subscription')
  if (!hasRootOp) {
    const gType = checker.getTypeOfPropertyOfType?.(sfiType, 'graphql')
    if (gType) sfiType = gType
  }
  const roots = {}
  for (const name of ROOTS) {
    const property = sfiType.getProperty(name)
    roots[name] = property ? checker.getTypeOfSymbolAtLocation(property, sfiFile) : undefined
  }
  return roots
}

/** Returns the text the module holds at `at`, its length in the four bytes before it, and the reader of its fields. */
function reader(exports, at) {
  const view = new DataView(exports.memory.buffer)
  const len = view.getUint32(at, true)
  const bytes = new Uint8Array(exports.memory.buffer, at + 4, len).slice()
  const decoder = new TextDecoder('utf-8', {fatal: true})
  const data = new DataView(bytes.buffer)
  let offset = 0
  const take = count => {
    if (offset + count > bytes.length) throw new BuildError('the answer of infer.wasm ends early')
    const start = offset
    offset += count
    return start
  }
  return {
    byte: () => bytes[take(1)],
    word: () => data.getUint32(take(4), true),
    text() {
      const length = this.word()
      const start = take(length)
      return decoder.decode(bytes.subarray(start, start + length))
    },
    done: () => offset === bytes.length
  }
}

/**
 * Runs infer.wasm over the type source of `made`, the program of the app, and returns its answer:
 * the schema text, the JSON text of the IR, the text of resolvers.js, the warnings, the message of
 * an invalid schema or null, and the counts of the calls of the checker. A call of the checker that
 * threw, a trap of the module or an answer that does not read fails the step with its message. A
 * trap of the export infer and a trap inside an import, which the source kept, end the use of the
 * instance, which the step drops unread.
 */
function runInference(made, wasm) {
  const source = createSource({program: made.program, checker: made.checker, entry: made.sfiFile,
    roots: rootTypes(made)})
  const instance = new WebAssembly.Instance(new WebAssembly.Module(wasm), {aosabi_host: source.imports})
  source.bind(instance.exports)
  const failed = () =>
    source.failures.length ? `, after the failed calls of the checker: ${source.failures.join('; ')}` : ''
  let at
  try {
    at = instance.exports.infer()
  } catch (error) {
    throw new BuildError(`infer.wasm trapped: ${error instanceof Error ? error.message : String(error)}${failed()}`)
  }
  if (source.trapped() !== null) {
    throw new BuildError(`infer.wasm trapped: ${source.trapped()}${failed()}`)
  }
  if (source.failures.length) {
    throw new BuildError(`the checker failed: ${source.failures.join('; ')}`)
  }
  const answer = reader(instance.exports, at)
  const refused = answer.byte()
  let built = null
  let refusal = null
  if (refused === 0) {
    const typeDefs = answer.text()
    const irJson = answer.text()
    const resolversJs = answer.text()
    const warnings = Array.from({length: answer.word()}, () => answer.text())
    const invalid = answer.byte() === 1 ? answer.text() : null
    built = {typeDefs, irJson, resolversJs, warnings, invalid}
  } else {
    refusal = answer.text()
  }
  const unread = answer.byte() === 1 ? answer.text() : null
  if (!answer.done()) throw new BuildError('the answer of infer.wasm holds bytes past its end')
  if (unread !== null) throw new BuildError(`infer.wasm read no answer of the checker: ${unread}`)
  if (refusal !== null) throw new BuildError(refusal)
  return {...built, counts: source.counts()}
}

/**
 * Returns the inference of the app in `cwd` with no file written, the schema derivation of
 * getBuildDefs of cli/builder/index.ts:79-89: the schema text, the JSON text of the IR, the text of
 * resolvers.js, the warnings in their order, the message of an invalid schema or null, the counts
 * of the calls of the checker and the compiler options of the program. `wasm` is the bytes of the
 * module of the inference, the infer.wasm of the package where left out, and `warn` writes a
 * warning of the reading of tsconfig.json, to stderr where left out.
 */
export function inferApp(cwd, {wasm, warn = message => console.error(message)} = {}) {
  const made = makeProgram(path.join(cwd, SFI_FILE_PATH), warn)
  const inferred = runInference(made, wasm ?? fs.readFileSync(INFER_WASM))
  return {...inferred, options: made.options}
}

/**
 * Loads the configuration the transpile wrote at `configOut` and returns it, initBuildPlugins of
 * bundler.ts:52-79 over the transpiled file, since plain Node imports no TypeScript: the default
 * export, else the export config, else an empty one, called where it is a function. A load that
 * throws fails the step.
 */
async function loadConfig(configOut) {
  if (!configOut) return {}
  try {
    const m = await import(pathToFileURL(configOut).href)
    const resolved = m.default ?? m.config ?? {}
    return typeof resolved === 'function' ? await resolved() : resolved
  } catch (e) {
    throw new BuildError(
      'Failed to load aosabi.config, aborting build (the app would otherwise run ' +
        `with NO plugins). Cause: ${e instanceof Error ? e.stack ?? e.message : String(e)}`
    )
  }
}

/**
 * Builds the app in `cwd` into `cwd/.aosabi/`, the command build in the author mode, and
 * resolves to what it wrote. `wasm` and `warn` are those of inferApp, and `warn` writes each
 * warning of the inference as well.
 *
 * The inference runs first, then the transpile of the sources, then the load of the configuration
 * and then the glue, which writes nothing of an invalid schema and fails the step with the message
 * of the invalid schema. The step loads the configuration after the transpile, whose file plain
 * Node imports, fails on a load that throws, and calls no member build of a plugin, which only the
 * plugin usePages holds, since the package carries no entry of the pages. The typed client of the
 * subpath query waits with that subpath.
 */
export async function build(cwd, {wasm, warn = message => console.error(message)} = {}) {
  const sfiFilePath = path.join(cwd, SFI_FILE_PATH)
  const outDir = path.join(cwd, OUTPUT_DIR)
  const inferred = inferApp(cwd, {wasm, warn})
  for (const warning of inferred.warnings) warn(warning)
  const configAbs = findConfigFile(cwd)
  fs.mkdirSync(outDir, {recursive: true})
  const {entryOut, configOut, written} = transpileApp({
    cwd,
    srcDir: path.dirname(sfiFilePath),
    entryAbs: sfiFilePath,
    configAbs,
    outDir,
    options: inferred.options
  })
  await loadConfig(configOut)
  if (inferred.invalid !== null) throw new BuildError(inferred.invalid)
  const relative = file => './' + path.relative(outDir, file).split(path.sep).join('/')
  const {schemaChanged} = emitGlue({
    typeDefs: inferred.typeDefs,
    irJson: inferred.irJson,
    resolversJs: inferred.resolversJs,
    outputDir: outDir,
    appImport: relative(entryOut),
    configImport: configOut ? relative(configOut) : null
  })
  return {schemaChanged, transpiled: written, warnings: inferred.warnings, counts: inferred.counts}
}

/** Resolves to the exit code of the command aosabi build in `cwd`, 0 where it built and 1 where it failed. */
export async function buildCommand(cwd) {
  try {
    const built = await build(cwd)
    console.log(`aosabi build: ${built.transpiled.length} files transpiled and the glue written to ${OUTPUT_DIR}`)
    return 0
  } catch (error) {
    console.error(`aosabi build: ${error instanceof Error ? error.message : String(error)}`)
    return 1
  }
}
