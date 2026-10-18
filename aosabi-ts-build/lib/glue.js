// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-build lib/glue: the transpile of the sources of an app into .aosabi/ and the glue files
 * of the step aosabi build, server.mjs with the entry aosabi, the Worker entry worker.mjs, schema.mjs
 * and the IR of the build.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/cli/builder/bundler/emit-server-glue.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 */

import fs from 'node:fs'
import path from 'node:path'
import ts from 'typescript'

/** The extensions of the sources the transpile takes from src/, transpile-app.ts:21. */
const SRC_EXTS = ['.ts', '.tsx', '.mts', '.cts']

/** The extensions a relative specifier resolves to, transpile-app.ts:22. */
const RESOLVE_EXTS = ['.ts', '.tsx', '.mts', '.cts', '.js', '.jsx', '.mjs', '.cjs']

/** The configuration files of an app in the order the build looks for them. */
export const CONFIG_FILES = ['aosabi.config.ts', 'aosabi.config.mts', 'aosabi.config.js', 'aosabi.config.mjs']

/** The entry of the package the glue imports. */
const ROOT_ENTRY = 'aosabi'

/** The entry of the package whose default export is the fetch handler of a Worker over the glue. */
const WORKER_ENTRY = 'aosabi/worker'

/** Returns the configuration file of the app in `cwd`, or null, findConfigFile of build-config.ts:20-22. */
export function findConfigFile(cwd) {
  return CONFIG_FILES.map(file => path.join(cwd, file)).find(file => fs.existsSync(file)) ?? null
}

/** Writes `content` to `file` where it differs from what the file holds, update-file-if-changed.ts:3-15. */
export function updateFileIfChanged(file, content) {
  try {
    if (fs.readFileSync(file, 'utf8') === content) return false
  } catch (error) {
    if (error.code !== 'ENOENT') throw error
  }
  fs.writeFileSync(file, content, 'utf8')
  return true
}

/** Returns every file under `dir` whose name `match` takes, in the order readdir answers, transpile-app.ts:24-38. */
function walk(dir, match) {
  const out = []
  let entries
  try {
    entries = fs.readdirSync(dir, {withFileTypes: true})
  } catch {
    return out
  }
  for (const entry of entries) {
    const full = path.join(dir, entry.name)
    if (entry.isDirectory()) out.push(...walk(full, match))
    else if (match(entry.name)) out.push(full)
  }
  return out
}

/**
 * Returns the specifier a relative import `source` of the file `importer` takes in the output, the
 * rule of the plugin preserve-relative-with-ext of transpile-app.ts:46-66: a specifier that names a
 * module of the platform by its extension stays, one whose file exists under an extension of
 * RESOLVE_EXTS takes `.js`, one whose folder holds an index takes `/index.js`, and any other one
 * takes `.js`. A JSON file keeps its specifier, a bare one keeps it, and the bundle of rolldown
 * that would inline a JSON file is no part of the transpile.
 */
function outputSpecifier(source, importer) {
  if (!/^\.\.?(\/|$)/.test(source)) return source
  if (/\.json$/.test(source)) return source
  if (/\.(js|mjs|cjs|node)$/.test(source)) return source
  const abs = path.resolve(path.dirname(importer), source)
  if (RESOLVE_EXTS.some(ext => fs.existsSync(abs + ext))) return `${source}.js`
  if (RESOLVE_EXTS.some(ext => fs.existsSync(path.join(abs, `index${ext}`)))) return `${source.replace(/\/$/, '')}/index.js`
  return `${source}.js`
}

/** Returns the transformer that gives each relative specifier of the file `importer` its output form. */
function specifierTransformer(importer) {
  return context => {
    const {factory} = context
    const literal = node => factory.createStringLiteral(outputSpecifier(node.text, importer))
    const visit = node => {
      if (ts.isImportDeclaration(node) && ts.isStringLiteral(node.moduleSpecifier)) {
        return factory.updateImportDeclaration(node, node.modifiers, node.importClause, literal(node.moduleSpecifier),
          node.attributes)
      }
      if (ts.isExportDeclaration(node) && node.moduleSpecifier && ts.isStringLiteral(node.moduleSpecifier)) {
        return factory.updateExportDeclaration(node, node.modifiers, node.isTypeOnly, node.exportClause,
          literal(node.moduleSpecifier), node.attributes)
      }
      if (ts.isCallExpression(node) && node.expression.kind === ts.SyntaxKind.ImportKeyword &&
          node.arguments.length > 0 && ts.isStringLiteral(node.arguments[0])) {
        const [first, ...rest] = node.arguments
        return factory.updateCallExpression(node, node.expression, node.typeArguments,
          [literal(first), ...rest.map(argument => ts.visitNode(argument, visit))])
      }
      return ts.visitEachChild(node, visit, context)
    }
    return sourceFile => ts.visitNode(sourceFile, visit)
  }
}

/**
 * Transpiles the sources of the app in `cwd` into `outDir` and returns the files of the entry and
 * of the configuration there, transpileApp of transpile-app.ts:76-130 with ts.transpileModule.
 *
 * The entries are the files of `srcDir` with an extension of SRC_EXTS and the configuration file,
 * each written to the path it holds under `cwd` with the extension `.js` under `outDir`, so the
 * output mirrors the tree. A declaration file emits nothing and is left out. Each file is
 * transpiled with the compiler options `options` of the app, the options of its tsconfig.json, to
 * an ES module at the level of the target of the app or of ES2022 where the app names a later one,
 * the level of the target node18, that keeps every import its code reads, with the helpers of a decorator written
 * into the file and its source map inline, as rolldown writes it.
 *
 * Every file is transpiled before one is written, and a file the parser of typescript finds a
 * syntax error in fails the step with each such error and its place and writes nothing. An error of
 * the compiler options names no file and fails nothing, since the transpile reads its own options
 * from tsconfig.json.
 */
export function transpileApp({cwd, srcDir, entryAbs, configAbs, outDir, options}) {
  const entryPoints = walk(srcDir, name => SRC_EXTS.some(ext => name.endsWith(ext)) && !/\.d\.[cm]?ts$/.test(name))
  if (configAbs && !entryPoints.includes(configAbs)) entryPoints.push(configAbs)
  const toOut = abs => path.join(outDir, path.relative(cwd, abs)).replace(/\.(tsx?|mts|cts|mjs|cjs|js)$/, '.js')
  const compilerOptions = {
    ...options,
    target: Math.min(options.target ?? ts.ScriptTarget.ESNext, ts.ScriptTarget.ES2022),
    module: ts.ModuleKind.ESNext,
    noEmit: false,
    declaration: false,
    sourceMap: false,
    inlineSourceMap: true,
    inlineSources: true,
    importHelpers: false,
    noEmitHelpers: false
  }
  delete compilerOptions.outDir
  delete compilerOptions.rootDir
  delete compilerOptions.outFile
  const outputs = []
  const errors = []
  for (const abs of entryPoints) {
    const transpiled = ts.transpileModule(fs.readFileSync(abs, 'utf8'), {
      compilerOptions,
      fileName: path.relative(cwd, abs),
      reportDiagnostics: true,
      transformers: {after: [specifierTransformer(abs)]}
    })
    for (const diagnostic of transpiled.diagnostics ?? []) {
      if (diagnostic.category !== ts.DiagnosticCategory.Error || !diagnostic.file) continue
      const {line, character} = diagnostic.file.getLineAndCharacterOfPosition(diagnostic.start ?? 0)
      errors.push(`${diagnostic.file.fileName}:${line + 1}:${character + 1}: ` +
        ts.flattenDiagnosticMessageText(diagnostic.messageText, '\n'))
    }
    outputs.push([toOut(abs), transpiled.outputText])
  }
  if (errors.length) throw new Error(`the transpile of the sources failed:\n${errors.join('\n')}`)
  const written = []
  for (const [out, text] of outputs) {
    fs.mkdirSync(path.dirname(out), {recursive: true})
    updateFileIfChanged(out, text)
    written.push(out)
  }
  return {entryOut: toOut(entryAbs), configOut: configAbs ? toOut(configAbs) : null, written}
}

/**
 * Returns the source of the configuration load of both entries, configLoadSource of
 * emit-server-glue.ts:73-81.
 */
function configLoadSource(configImport) {
  return configImport
    ? `const __m = await import(${JSON.stringify(configImport)})\n` +
        `const __r = __m.default ?? __m.config ?? {}\n` +
        `const __config = typeof __r === 'function' ? await __r() : __r\n`
    : `const __config = {}\n`
}

/**
 * Returns the source of server.mjs, serverBootstrap of emit-server-glue.ts:118-160 line for line
 * with the entry aosabi, and the import of ir.mjs and the line that
 * exposes the IR of the build as globalThis.__AOSABI_IR__ after the import of the app, so the
 * entries aosabi/bun and aosabi/worker hand the engine the IR of the build where no host handed
 * one.
 */
export function serverBootstrap(appImport, configImport) {
  return (
    `// GENERATED by aosabi build. Do not edit. The unbundled server entry.\n` +
    `import {executeConfig as __executeConfig, handler as __handler} from '${ROOT_ENTRY}'\n` +
    `import {fileURLToPath as __fileURLToPath} from 'node:url'\n` +
    `import {dirname as __dirname} from 'node:path'\n` +
    `import {typeDefs as __typeDefs} from './schema.mjs'\n` +
    `import {resolvers as __resolvers} from './resolvers.js'\n` +
    `import __aosabiApp from ${JSON.stringify(appImport)}\n` +
    `import {ir as __aosabiIr} from './ir.mjs'\n` +
    `\n` +
    `globalThis.__AOSABI_IR__ = __aosabiIr\n` +
    `\n` +
    `// The app root (dir that CONTAINS \`.aosabi\`) derived from THIS entry's own location:\n` +
    `// server.mjs lives at <root>/.aosabi/server.mjs. The usePages runtime resolves its build\n` +
    `// artifacts (manifests, SSR chunks, static/public) against this, so the app runs from ANY\n` +
    `// cwd (e.g. a standalone deploy) without depending on process.cwd().\n` +
    `globalThis.__AOSABI_ROOT__ = __dirname(__dirname(__fileURLToPath(import.meta.url)))\n` +
    `\n` +
    configLoadSource(configImport) +
    `\n` +
    `// Expose the booted app so in-process callers (usePages SSR fetcher) hit THIS\n` +
    `// instance — the one with the handler + plugins mounted.\n` +
    `globalThis.__AOSABI_APP__ = __aosabiApp\n` +
    `\n` +
    `// 'first' plugins (auth/identity/db/queues) → GraphQL handler → 'last' plugins\n` +
    `// (usePages catch-all). Serving is handled below (Node) or by the host (Bun/edge).\n` +
    `await __executeConfig(__config, undefined, __aosabiApp)\n` +
    `__aosabiApp.use(__handler({typeDefs: __typeDefs, graphql: __aosabiApp.graphql, resolvers: __resolvers}, __aosabiApp))\n` +
    `await __executeConfig(__config, {pluginsStrategy: 'last'}, __aosabiApp)\n` +
    `\n` +
    `// The entry is PURE: it assembles the app and exports it — it does NOT serve as a\n` +
    `// side effect of import. Serving is explicit + app-owned: on Node, add \`useNodeServer()\`\n` +
    `// to your aosabi.config (a 'last' plugin that binds the port during executeConfig above);\n` +
    `// Bun (\`bun run\`), workerd (Wrangler) and Deno auto-serve this default export. In dev,\n` +
    `// \`aosabi dev\` runs the app in-process (it never imports this glue). This keeps the\n` +
    `// artifact runtime-agnostic and free of import side effects.\n` +
    `export default __aosabiApp\n`
  )
}

/**
 * Returns the source of worker.mjs, the entry of a Cloudflare Worker that Wrangler names as its
 * main: it imports the entry aosabi/worker first, so every app of the isolate takes core.wasm
 * before the glue runs, then the glue server.mjs, which exposes the app and the IR of the build,
 * and exports the fetch handler of aosabi/worker over them.
 */
export function workerBootstrap() {
  return (
    `// GENERATED by aosabi build. The entry of a Cloudflare Worker over the glue server.mjs.\n` +
    `import __worker from '${WORKER_ENTRY}'\n` +
    `import './server.mjs'\n` +
    `\n` +
    `export default __worker\n`
  )
}

/**
 * Writes the glue of a build into `outputDir`, emitServerGlue of emit-server-glue.ts:162-205:
 * schema.graphql, schema.mjs, resolvers.js and server.mjs in that order, then worker.mjs, the
 * Worker entry, and then ir.json and ir.mjs, the IR of the build. The schema has been validated
 * before, so no file of an invalid schema is written. Returns whether the schema text changed
 * since the last build.
 */
export function emitGlue({typeDefs, irJson, resolversJs, outputDir, appImport, configImport}) {
  const schemaPath = path.join(outputDir, 'schema.graphql')
  let prev = null
  try {
    prev = fs.readFileSync(schemaPath, 'utf8')
  } catch {
    // The first build.
  }
  const schemaChanged = prev !== typeDefs
  fs.mkdirSync(outputDir, {recursive: true})
  updateFileIfChanged(schemaPath, typeDefs)
  updateFileIfChanged(path.join(outputDir, 'schema.mjs'), `export const typeDefs = ${JSON.stringify(typeDefs)}\n`)
  updateFileIfChanged(path.join(outputDir, 'resolvers.js'), resolversJs)
  updateFileIfChanged(path.join(outputDir, 'server.mjs'), serverBootstrap(appImport, configImport))
  updateFileIfChanged(path.join(outputDir, 'worker.mjs'), workerBootstrap())
  updateFileIfChanged(path.join(outputDir, 'ir.json'), `${irJson}\n`)
  updateFileIfChanged(path.join(outputDir, 'ir.mjs'), `export const ir = ${JSON.stringify(irJson)}\n`)
  return {schemaChanged}
}
