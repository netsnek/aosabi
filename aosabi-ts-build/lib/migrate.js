// SPDX-License-Identifier: MIT
/*
 * aosabi-build lib/migrate: the command aosabi migrate, which rewrites the import specifiers of an
 * app to the entries of the npm package aosabi by the table migrate-map.json, swaps the dependency
 * and the build script of its package.json, points the paths of its scripts into the build folder
 * at .aosabi/, renames its configuration file and its declaration file by the list files of the
 * table, logs every rewritten line and every renamed file in .aosabi/migrate.json and restores
 * every file byte for byte and under its name from that log.
 *
 * The parser of typescript 5.9.3 finds the four forms the step rewrites, the import declaration, the
 * export declaration that names a module, the call of import() with a literal and the name of a
 * declare module block, in .ts, .mts, .js and .mjs files, so a specifier inside a string or a comment
 * is never touched. A rewrite replaces the bytes between the quotes of one literal and keeps the
 * quote character and every other byte of the file. The checker of a program of the one file tells
 * which declaration an identifier refers to, so a name of the table stops the run where the source
 * reads it off a namespace of its specifier, and a binding that shadows the namespace reads none. A
 * member of a plugin named as a hook of Envelop or of Yoga that the engine runs no item of stops the
 * run at its line as well, the plan's rule for the plugins of an app (section Routes, middleware and
 * plugins of the plan), where a plugin is an object literal that holds a key of a plugin or that
 * the source places among the plugins of a configuration, and a member of that name of any other
 * object literal is kept and listed. A specifier of a package of the list kept of the table, hono,
 * the peer of the package aosabi, and @hono/node-server, which an app on Node installs itself, is
 * kept as the app wrote it.
 *
 * The step renames that name where a named import or an export declaration of the specifier names it,
 * and every identifier of the file that the checker resolves to the binding of such an import, each
 * one more edit of the log, which the reverse undoes as it undoes a specifier. An import under an
 * alias of its own, `Name as Alias`, renames the imported name alone. A variable row gives a
 * variable of the environment the app reads the name the engine reads it under, such as
 * AOSABI_ROLE, and the step renames a read or a write of it on an object env,
 * `process.env.NAME` or `process.env['NAME']`, as the worker entry of a build sets the role.
 */

import {createHash} from 'node:crypto'
import fs from 'node:fs'
import path from 'node:path'
import ts from 'typescript'

/** The extensions of the files whose specifiers the step rewrites. */
const SOURCES = new Set(['.ts', '.mts', '.js', '.mjs'])

/** The extensions of the sources the step does not read, which its log lists. */
const UNREAD = new Set(['.tsx', '.jsx', '.cts', '.cjs'])

/** The fields of package.json that may name the dependency the step swaps. */
const SECTIONS = ['dependencies', 'devDependencies', 'peerDependencies', 'optionalDependencies']

/** The directory of the log beside the app's sources, and the log in it. */
const LOG_DIR = '.aosabi'
const LOG_FILE = 'migrate.json'

/** The entry a marked specifier is rewritten under. */
const UNSUPPORTED = 'aosabi/unsupported/'

/**
 * The hooks of Envelop and of Yoga a plugin may hold beside onExecute, in the order of
 * MEMBER_ORDER of crates/aosabi-core/src/plugin/mod.rs of the Aosabi repository. The engine runs
 * onExecute alone of them, with the done hook onExecuteDone it answers, so the boot of the engine
 * refuses a plugin that holds any other, and the step stops on a member of that name of a plugin.
 */
const HOOKS = new Set([
  'onPluginInit',
  'onEnveloped',
  'onSchemaChange',
  'onParse',
  'onValidate',
  'onContextBuilding',
  'onSubscribe',
  'onRequest',
  'onResponse',
  'onDispose',
  'instrumentation',
  'onYogaInit',
  'onRequestParse',
  'onParams',
  'onExecutionResult',
  'onResultProcess'
])

/**
 * The keys of a plugin beside the hooks of Envelop and of Yoga, its name, its setup, its
 * middleware, its hook onExecute, its dependencies, its roles and its strategy. An object literal
 * that holds one is a plugin.
 */
const PLUGIN_KEYS = new Set(['name', 'setup', 'middleware', 'onExecute', 'dependsOn', 'roles', 'strategy'])

/** An operator of the shell, after which a command starts unless it is `)`. */
const OPERATOR = /^(?:&&|\|\||[;&|()\n])/

/** A variable assignment that opens a command of the shell, such as `NODE_ENV=production`. */
const ASSIGNMENT = /^[A-Za-z_][A-Za-z0-9_]*=/

/** The reserved words of the shell that a command follows, such as `then` in `if a; then b; fi`. */
const RESERVED = new Set(['!', '{', 'if', 'then', 'elif', 'else', 'while', 'until', 'do'])

/**
 * The flags of npm, npm exec and npm x that take the next word as their value, among them the
 * shorthands `-C` of `--prefix`, `-c` of `--call`, `-w` of `--workspace` and `-L` of `--location`,
 * while `-p` is `--parseable` there and takes none.
 */
const NPM_VALUES = [
  '-C', '--prefix', '-c', '--call', '-w', '--workspace', '--package', '-L', '--location', '--userconfig',
  '--registry', '--reg', '--cache', '--loglevel', '--script-shell'
]

/** The flags of npx that take the next word as their value: those of npm, `-p` of `--package` and `--shell`. */
const NPX_VALUES = ['-p', '--shell', ...NPM_VALUES]

/** The flags of pnpm, pnpm exec and pnpm dlx that take the next word as their value. */
const PNPM_VALUES = [
  '-C', '--dir', '-F', '--filter', '--filter-prod', '--test-pattern', '--changed-files-ignore-pattern',
  '--workspace-concurrency', '--resume-from', '--reporter', '--loglevel', '--package', '--allow-build'
]

/** The flags of yarn and yarn exec, and `-p` of `--package` of yarn dlx, that take the next word as their value. */
const YARN_VALUES = [
  '--cwd', '--cache-folder', '--preferred-cache-folder', '--global-folder', '--link-folder', '--modules-folder',
  '--use-yarnrc', '--mutex', '--network-concurrency', '--network-timeout', '--otp', '--proxy', '--https-proxy',
  '--registry', '-p', '--package'
]

/**
 * The runners that run their next word as a command once their flags and variable assignments are
 * passed, each with the subcommands that stand first, which `bare` makes optional, and the flags
 * that take the next word as their value, which passes with its flag.
 */
const RUNNERS = new Map([
  ['npx', {subcommands: [], values: NPX_VALUES}],
  ['bunx', {subcommands: [], values: ['-p', '--package']}],
  ['pnpx', {subcommands: [], values: ['--package', '--allow-build']}],
  ['cross-env', {subcommands: [], values: []}],
  ['env', {subcommands: [], values: ['-a', '--argv0', '-u', '--unset', '-C', '--chdir']}],
  ['pnpm', {subcommands: ['exec', 'dlx'], bare: true, values: PNPM_VALUES}],
  ['yarn', {subcommands: ['exec', 'dlx'], bare: true, values: YARN_VALUES}],
  ['npm', {subcommands: ['exec', 'x'], values: NPM_VALUES}],
  ['bun', {subcommands: ['x'], values: ['--cwd', '-c', '--config', '--env-file', '-p', '--package']}]
])

/** The characters of the JSON escapes that take one letter. */
const ESCAPES = {b: '\b', f: '\f', n: '\n', r: '\r', t: '\t'}

/** The expressions around a value that change no value: parentheses, `as`, `satisfies`, `<T>` and `!`. */
const TRANSPARENT = new Set([
  ts.SyntaxKind.ParenthesizedExpression,
  ts.SyntaxKind.AsExpression,
  ts.SyntaxKind.SatisfiesExpression,
  ts.SyntaxKind.TypeAssertionExpression,
  ts.SyntaxKind.NonNullExpression
])

/** The table, read once from the files of this package. */
let table = null

/**
 * Returns the table of migrate-map.json with its lookups and the version of this package: `origin`,
 * the package the step swaps, whose specifiers it rewrites, `bin` and `binTo`, the command a build
 * script runs and the command it runs after the step, `folder`, the build folder whose paths the
 * scripts are pointed away from and to, and `files`, the files the step renames.
 */
function loadTable() {
  if (table) return table
  const read = name => JSON.parse(fs.readFileSync(new URL(name, import.meta.url), 'utf8'))
  const map = read('../migrate-map.json')
  const names = new Map()
  for (const row of map.names) {
    if (!names.has(row.specifier)) names.set(row.specifier, new Map())
    names.get(row.specifier).set(row.name, row)
  }
  const renames = new Map()
  for (const row of map.renames ?? []) {
    if (!renames.has(row.specifier)) renames.set(row.specifier, new Map())
    renames.get(row.specifier).set(row.name, row)
  }
  const variables = new Map((map.variables ?? []).map(row => [row.name, row]))
  const origin = map.package.from
  table = {
    map,
    origin,
    bin: map.script.from.split(' ')[0],
    binTo: map.script.to.split(' ')[0],
    folder: map.folder,
    files: map.files ?? [],
    version: read('../package.json').version,
    entries: new Map(map.entries.map(row => [row.specifier, row])),
    stops: new Map(map.stops.map(row => [row.specifier, row])),
    names,
    renames,
    variables,
    pulled: new Set(map.pulled),
    kept: new Set((map.kept ?? []).map(row => row.package)),
    mapped: new Set(map.entries.map(row => row.specifier).filter(name => !name.startsWith(origin)).map(packageOf))
  }
  return table
}

/**
 * Returns the text of `raw`, the characters between the quotes of a JSON string, with the offset in
 * `raw` of each character of the text and of its end.
 */
function jsonText(raw) {
  let text = ''
  const offsets = []
  let i = 0
  while (i < raw.length) {
    offsets.push(i)
    if (raw[i] !== '\\') {
      text += raw[i]
      i += 1
    } else if (raw[i + 1] === 'u') {
      text += String.fromCharCode(parseInt(raw.slice(i + 2, i + 6), 16))
      i += 6
    } else {
      text += ESCAPES[raw[i + 1]] ?? raw[i + 1]
      i += 2
    }
  }
  offsets.push(raw.length)
  return {text, offsets}
}

/** Returns the offset after the quoted part of `script` that opens at `start`, or the end of `script`. */
function quoteEnd(script, start) {
  const quote = script[start]
  for (let i = start + 1; i < script.length; i++) {
    if (script[i] === quote) return i + 1
    if (script[i] === '\\' && quote === '"') i++
  }
  return script.length
}

/**
 * Returns the words of the script `script` with their offsets, as the shell splits them: a blank,
 * `<` or `>` parts two words, an operator is a word of its own, and a quoted part stays inside its
 * word, so an operator inside quotes starts no command.
 */
function shellWords(script) {
  const words = []
  let i = 0
  while (i < script.length) {
    const operator = OPERATOR.exec(script.slice(i))
    if (operator) {
      words.push({text: operator[0], index: i, operator: true})
      i += operator[0].length
    } else if (/[\s<>]/.test(script[i])) {
      i += 1
    } else {
      const start = i
      while (i < script.length && !/[\s<>;&|()]/.test(script[i])) {
        if (script[i] === "'" || script[i] === '"') i = quoteEnd(script, i)
        else i += script[i] === '\\' ? 2 : 1
      }
      i = Math.min(i, script.length)
      words.push({text: script.slice(start, i), index: start, operator: false})
    }
  }
  return words
}

/**
 * Returns whether the word `text` runs the command `bin`, as the bare word or as a path to the file
 * `bin` of a directory .bin, where a package manager links the bins of its packages, such as
 * ./node_modules/.bin/<bin>.
 */
function runsBin(text, bin) {
  return text === bin || text === '.bin/' + bin || text.endsWith('/.bin/' + bin)
}

/**
 * Returns the index in `words` of the word that runs as the command which starts at `at`, after the
 * reserved words and the variable assignments that open the command and after each runner with its
 * flags, the value of each flag that takes one, its variable assignments and its subcommand, or -1
 * where a runner lacks the subcommand it needs.
 */
function commandAt(words, at) {
  const plain = i => i < words.length && !words[i].operator
  const passed = (i, values) => {
    while (plain(i) && (words[i].text.startsWith('-') || ASSIGNMENT.test(words[i].text))) {
      i += values.includes(words[i].text) && plain(i + 1) ? 2 : 1
    }
    return i
  }
  let i = at
  while (plain(i) && (RESERVED.has(words[i].text) || ASSIGNMENT.test(words[i].text))) i++
  while (plain(i) && RUNNERS.has(words[i].text)) {
    const runner = RUNNERS.get(words[i].text)
    i = passed(i + 1, runner.values)
    if (plain(i) && runner.subcommands.includes(words[i].text)) i = passed(i + 1, runner.values)
    else if (runner.subcommands.length && !runner.bare) return -1
  }
  return plain(i) ? i : -1
}

/**
 * Returns each command of the script `script` that runs the command `bin` of the table, with the
 * offset of the name `bin` in `script` and whether its next word is `build`.
 *
 * A command starts at the start of the script and after every operator of the shell but `)`,
 * outside quotes. The word `bin` anywhere else runs no command, as the argument of `echo` or as the
 * value of a flag of a runner in `pnpm --filter <bin> build`, and neither does a scoped name or a
 * path to a file `bin` outside a directory .bin, such as `apps/<bin>`.
 */
function binCommands(script, bin) {
  const words = shellWords(script)
  const commands = []
  let starts = true
  for (const [i, word] of words.entries()) {
    if (word.operator) {
      starts = word.text !== ')'
      continue
    }
    if (!starts) continue
    starts = false
    const at = commandAt(words, i)
    if (at < 0 || !runsBin(words[at].text, bin)) continue
    const next = words[at + 1]
    const index = words[at].index + words[at].text.length - bin.length
    commands.push({index, build: next !== undefined && !next.operator && next.text === 'build'})
  }
  return commands
}

/**
 * Returns the offset in `script` of each path into the build folder `folder`: the name as a word of
 * its own or as a segment of a path, after the start of the script, a blank, a quote, `=`, `:`, `,`,
 * `(` or `/` and before the end, a blank, a quote, `/`, `;`, `&`, `|`, `)` or `,`.
 */
function folderPaths(script, folder) {
  const quoted = folder.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
  const pattern = new RegExp(`(?<=^|[\\s'"\`=:,(/])${quoted}(?=$|[\\s'"\`/;&|),])`, 'g')
  return [...script.matchAll(pattern)].map(match => match.index)
}

/**
 * Returns the names of the packages the app in `dir` depends on itself, every key of the fields of
 * SECTIONS of the nearest package.json at or above `dir`, the one that holds the files of the app,
 * as the resolution of Node finds it, or an empty set where none holds them.
 */
function ownDependencies(dir) {
  let at = path.resolve(dir)
  for (;;) {
    const file = path.join(at, 'package.json')
    if (fs.existsSync(file)) {
      let manifest
      try {
        manifest = JSON.parse(fs.readFileSync(file, 'utf8'))
      } catch {
        return new Set()
      }
      const names = new Set()
      for (const section of SECTIONS) {
        const listed = manifest?.[section]
        if (listed && typeof listed === 'object') for (const name of Object.keys(listed)) names.add(name)
      }
      return names
    }
    const parent = path.dirname(at)
    if (parent === at) return new Set()
    at = parent
  }
}

/** Returns the name of the package a bare specifier imports, its scope included. */
function packageOf(specifier) {
  const parts = specifier.split('/')
  return specifier.startsWith('@') ? parts.slice(0, 2).join('/') : parts[0]
}

/**
 * Returns what the step does with `specifier`: rewrite it to an entry, stop on it, or keep it. A
 * specifier of the package the step swaps, or of a package it pulls in by default, with no row of
 * the table stops as well, unless the app depends on that package itself, `own` naming the packages its package.json
 * names, and the table maps no specifier of the package, whose engine item then stands in its
 * place: then it is the app's own dependency, kept and listed in the form `own`. A specifier of a
 * package of the list kept of the table is kept, hono, the peer of the package aosabi, and
 * @hono/node-server, which an app on Node installs itself. Any other specifier, a Node built in, a
 * package of the app's own, a peer or a relative path, is kept, as the plan keeps `node:fs`
 * (owner question 9 of the plan).
 */
function classify(specifier, own = new Set()) {
  const {entries, stops, pulled, mapped, kept, origin} = loadTable()
  const entry = entries.get(specifier)
  if (entry) return {kind: 'rewrite', to: entry.entry}
  const stop = stops.get(specifier)
  if (stop) return {kind: 'stop', stop}
  if (specifier === origin || specifier.startsWith(origin + '/')) {
    return {kind: 'stop', stop: {specifier, mark: false}}
  }
  const name = packageOf(specifier)
  if (kept.has(name)) return {kind: 'keep'}
  if (pulled.has(name) && own.has(name) && !mapped.has(name)) return {kind: 'own', name}
  if (pulled.has(name)) {
    const why = `is of ${name}, a package ${origin} pulls in by default, and has no entry of aosabi`
    return {kind: 'stop', stop: {specifier, mark: false, why}}
  }
  return {kind: 'keep'}
}

/** Returns why the row `row` of the table keeps a specifier or a name out of the package aosabi. */
function reason(row) {
  if (row.why) return row.why
  if (row.until) return 'has no engine item in aosabi before ' + row.until
  if (row.record) return 'is left aside by ' + row.record + ' of the Aosabi repository'
  if (row.state) return 'has no entry of aosabi, ' + row.state
  return 'has no entry of aosabi'
}

/** Returns the SHA-256 of `bytes` in hex. */
function sha256(bytes) {
  return createHash('sha256').update(bytes).digest('hex')
}

/**
 * Returns the text of `bytes` decoded as UTF-8, or null when the decode is not exact, since the
 * step keeps every other byte and cannot keep the bytes of a file it cannot decode exactly.
 */
function decode(bytes) {
  const text = bytes.toString('utf8')
  return Buffer.from(text, 'utf8').equals(bytes) ? text : null
}

/** Returns the offsets in `bytes` at which each line starts, the first line at 0. */
function lineStarts(bytes) {
  const starts = [0]
  for (let i = 0; i < bytes.length; i++) if (bytes[i] === 0x0a) starts.push(i + 1)
  return starts
}

/** Returns the line of the byte at `offset`, counted from 1 by line feeds, as an editor counts it. */
function lineOf(starts, offset) {
  let low = 0
  let high = starts.length - 1
  while (low < high) {
    const middle = (low + high + 1) >> 1
    if (starts[middle] <= offset) low = middle
    else high = middle - 1
  }
  return low + 1
}

/** Returns the text of line `line` of `bytes` without its line break. */
function lineText(bytes, starts, line) {
  const end = line < starts.length ? starts[line] - 1 : bytes.length
  return bytes.subarray(starts[line - 1], end).toString('utf8').replace(/\r$/, '')
}

/**
 * Returns the paths under `dir`, relative and with forward slashes, of the files the step reads
 * and of the sources it does not read. It enters no directory named node_modules, no directory
 * whose name starts with a dot, such as .git, an old build folder or its own .aosabi, and no
 * symbolic link.
 */
function walk(dir) {
  const read = []
  const unread = []
  const visit = relative => {
    const entries = fs.readdirSync(path.join(dir, relative), {withFileTypes: true})
    entries.sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0))
    for (const entry of entries) {
      const name = relative ? relative + '/' + entry.name : entry.name
      if (entry.isDirectory()) {
        if (entry.name !== 'node_modules' && !entry.name.startsWith('.')) visit(name)
      } else if (entry.isFile()) {
        const extension = path.extname(entry.name)
        if (SOURCES.has(extension)) read.push(name)
        else if (UNREAD.has(extension)) unread.push(name)
      }
    }
  }
  visit('')
  return {read, unread}
}

/**
 * Returns the literal of the module specifier of `node` with the form it has, or null. The import
 * declaration, the export declaration with a module, the call of import() with one literal and the
 * name of a `declare module` block are rewritten, the last so that an augmentation of the app, such
 * as the one of Bindings and Variables in the declaration file of the app, applies to the entry
 * the app imports, which the checker of aosabi build reads. An import type and an
 * `import x = require()` declaration name a module as well and are kept and listed.
 */
function specifierOf(node) {
  if (ts.isImportDeclaration(node) && ts.isStringLiteral(node.moduleSpecifier)) {
    return {literal: node.moduleSpecifier, form: 'import'}
  }
  if (ts.isExportDeclaration(node) && node.moduleSpecifier && ts.isStringLiteral(node.moduleSpecifier)) {
    return {literal: node.moduleSpecifier, form: 'export'}
  }
  if (ts.isCallExpression(node) && node.expression.kind === ts.SyntaxKind.ImportKeyword) {
    const argument = node.arguments[0]
    if (argument && (ts.isStringLiteral(argument) || ts.isNoSubstitutionTemplateLiteral(argument))) {
      return {literal: argument, form: 'import()'}
    }
    return null
  }
  if (ts.isModuleDeclaration(node) && ts.isStringLiteral(node.name)) {
    return {literal: node.name, form: 'declare module'}
  }
  if (ts.isImportTypeNode(node) && ts.isLiteralTypeNode(node.argument) && ts.isStringLiteral(node.argument.literal)) {
    return {literal: node.argument.literal, form: 'import type', kept: true}
  }
  if (
    ts.isImportEqualsDeclaration(node) &&
    ts.isExternalModuleReference(node.moduleReference) &&
    ts.isStringLiteral(node.moduleReference.expression)
  ) {
    return {literal: node.moduleReference.expression, form: 'import =', kept: true}
  }
  return null
}

/** Returns the checker of a program of the one source file `source`, which resolves no module and reads no lib. */
function checkerOf(source) {
  const name = source.fileName
  const host = {
    getSourceFile: file => (file === name ? source : undefined),
    getDefaultLibFileName: () => 'lib.d.ts',
    writeFile: () => {},
    getCurrentDirectory: () => '',
    getCanonicalFileName: file => file,
    useCaseSensitiveFileNames: () => true,
    getNewLine: () => '\n',
    fileExists: file => file === name,
    readFile: () => undefined
  }
  const options = {noResolve: true, noLib: true, types: [], allowJs: true, noEmit: true}
  return ts.createProgram({rootNames: [name], options, host}).getTypeChecker()
}

/**
 * Returns whether `identifier` reads the binding that `declaration` declares.
 *
 * The name of a declaration, of a member or of a property reads none, and the local side of
 * `export {ns}` and a shorthand property `{ns}` read their binding.
 */
function refersTo(checker, identifier, declaration) {
  const parent = identifier.parent
  let symbol
  if (ts.isShorthandPropertyAssignment(parent) && parent.name === identifier) {
    symbol = checker.getShorthandAssignmentValueSymbol(parent)
  } else if (ts.isExportSpecifier(parent)) {
    if ((parent.propertyName ?? parent.name) !== identifier || parent.parent.parent.moduleSpecifier) return false
    symbol = checker.getExportSpecifierLocalTargetSymbol(parent)
  } else if (parent.name === identifier) {
    return false
  } else {
    symbol = checker.getSymbolAtLocation(identifier)
  }
  return Boolean(symbol?.declarations?.includes(declaration))
}

/**
 * Returns the node above `node` past the parentheses and the assertions of a type or of non null
 * around it, which change no value, with its child that holds `node`.
 */
function outer(node) {
  let child = node
  while (TRANSPARENT.has(child.parent.kind)) child = child.parent
  return {child, parent: child.parent}
}

/** Returns the text of `node` where it is a string, a template without substitutions or a number, or null. */
function literal(node) {
  const text = ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node) || ts.isNumericLiteral(node)
  return text ? node.text : null
}

/** Returns `node` past the parentheses and the assertions of a type or of non null around it. */
function inner(node) {
  let value = node
  while (TRANSPARENT.has(value.kind)) value = value.expression
  return value
}

/**
 * Returns the name of `node` where it is a member of an object literal, a property, a method, an
 * accessor or a shorthand property, by an identifier, a string or a computed name with a literal,
 * or null.
 */
function memberName(node) {
  if (!node.parent || !ts.isObjectLiteralExpression(node.parent)) return null
  const member =
    ts.isPropertyAssignment(node) ||
    ts.isMethodDeclaration(node) ||
    ts.isShorthandPropertyAssignment(node) ||
    ts.isGetAccessorDeclaration(node) ||
    ts.isSetAccessorDeclaration(node)
  if (!member) return null
  const name = node.name
  return ts.isComputedPropertyName(name) ? literal(name.expression) : (literal(name) ?? name.text ?? null)
}

/** Returns the hook of `HOOKS` that `node` names where it is a member of an object literal, or null. */
function hookOf(node) {
  const text = memberName(node)
  return text !== null && HOOKS.has(text) ? text : null
}

/**
 * Returns the object literals of `source` that are plugins: each that holds a member named as a key
 * of `PLUGIN_KEYS`, and each that the source places among the plugins of a configuration, an
 * element of an array literal that a member `plugins` of an object literal holds, the array written
 * there, read through an identifier, or read through the shorthand member `{plugins}`, and the
 * element written there or read through an identifier, where each identifier names a declaration in
 * the same file that initializes a variable with the array or the element.
 *
 * A plugin is an object literal in every fixture and in every example the step was written against,
 * and each holds a key of a plugin or stands among the plugins. An object that reaches the plugins
 * through a call, a spread, a condition, a member of another object or another file, or an array of
 * plugins that does, and holds no key of a plugin is none here, which the boot of the engine refuses
 * with the name of its hook.
 */
function pluginsOf(source) {
  const plugins = new Set()
  let checker = null
  const initializers = symbol => {
    const found = []
    for (const declaration of symbol?.declarations ?? []) {
      const initializer = ts.isVariableDeclaration(declaration) && declaration.initializer
      if (initializer) found.push(inner(initializer))
    }
    return found
  }
  const named = identifier => {
    checker ??= checkerOf(source)
    return initializers(checker.getSymbolAtLocation(identifier))
  }
  const place = element => {
    const value = inner(element)
    if (ts.isObjectLiteralExpression(value)) {
      plugins.add(value)
      return
    }
    if (!ts.isIdentifier(value)) return
    for (const initializer of named(value)) {
      if (ts.isObjectLiteralExpression(initializer)) plugins.add(initializer)
    }
  }
  const list = array => {
    if (ts.isArrayLiteralExpression(array)) for (const element of array.elements) place(element)
  }
  const visit = node => {
    if (ts.isObjectLiteralExpression(node) && node.properties.some(member => PLUGIN_KEYS.has(memberName(member)))) {
      plugins.add(node)
    }
    if (ts.isPropertyAssignment(node) && memberName(node) === 'plugins') {
      const value = inner(node.initializer)
      if (ts.isIdentifier(value)) for (const initializer of named(value)) list(initializer)
      else list(value)
    }
    if (ts.isShorthandPropertyAssignment(node) && memberName(node) === 'plugins') {
      checker ??= checkerOf(source)
      for (const initializer of initializers(checker.getShorthandAssignmentValueSymbol(node))) list(initializer)
    }
    ts.forEachChild(node, visit)
  }
  visit(source)
  return plugins
}

/**
 * Returns whether the binding that `declaration` declares is also read where the checker of the one
 * file follows no identifier to it: a declaration exported by its own keyword `export`, which another
 * file imports, or as a member of a TypeScript namespace, read as `N.name`, and a parameter property,
 * read as a member of its class.
 */
function leaves(declaration) {
  if (ts.isParameter(declaration) && ts.isParameterPropertyDeclaration(declaration, declaration.parent)) return true
  return (ts.getCombinedModifierFlags(declaration) & ts.ModifierFlags.Export) !== 0
}

/**
 * Returns the stops and the kept forms of the names that the source `source` of `file` takes from
 * a specifier with name rows, given in `uses` each import, export and call of import() of such a
 * specifier with the rows of its names, and `lineAt`, which gives the line of a node.
 *
 * A name of a row stops the run in a named import, of types alone or not, and in an export clause
 * that names the module, at the line of the statement, and where it is read off a namespace of its
 * specifier, at the line of the name. A namespace is the binding of `import * as ns`, the
 * expression `await import()`, the first parameter of a function that `import().then()` is called
 * with, and every binding whose initializer is a namespace, such as `const ns = await import()`. A
 * name is read off a namespace by a member `ns.name` or `ns['name']`, by a qualified name `ns.Name`
 * in a type, in `typeof ns.name` or in `import alias = ns.Name`, and by a key of an object binding
 * pattern whose initializer is a namespace or which is that first parameter.
 *
 * A use whose names the step cannot tell is kept and listed: a computed member, a rest element of
 * such a pattern or as that first parameter, a namespace used whole, and `export *` and
 * `export * as ns` of such a specifier. A namespace is used whole where it is any other value, such
 * as `f(ns)`, `export {ns}` or `typeof ns`, and where a binding of it is also read as an export or a
 * member, as `leaves` gives it, such as `export const alias = ns`, listed at the name of the
 * binding, whose reads by its name inside the file still stop.
 */
function names(file, source, lineAt, uses) {
  const stops = []
  const kept = []
  const namespaces = new Map()
  const queue = []
  const read = (use, name, node) => {
    const row = use.banned.get(name)
    if (row) stops.push({file, line: lineAt(node), message: `the name ${name} of '${use.specifier}' ${reason(row)}`})
  }
  const list = (use, form, node) => kept.push({file, line: lineAt(node), form, specifier: use.specifier})
  const pattern = (use, binding) => {
    for (const element of binding.elements) {
      const key = element.propertyName ?? element.name
      if (element.dotDotDotToken) {
        list(use, 'rest element', element)
      } else if (ts.isComputedPropertyName(key)) {
        const name = literal(key.expression)
        if (name === null) list(use, 'computed member', key)
        else read(use, name, key)
      } else {
        read(use, key.text, key)
      }
    }
  }
  const bind = (use, name, declaration) => {
    if (ts.isIdentifier(name)) {
      if (namespaces.has(declaration)) return
      if (leaves(declaration)) list(use, 'namespace used whole', name)
      namespaces.set(declaration, use)
      queue.push(declaration)
    } else if (ts.isObjectBindingPattern(name)) {
      pattern(use, name)
    } else {
      list(use, 'namespace used whole', name)
    }
  }
  const value = (use, expression) => {
    const {child, parent} = outer(expression)
    const assigned = ts.isBinaryExpression(parent) && parent.operatorToken.kind === ts.SyntaxKind.EqualsToken
    if (ts.isPropertyAccessExpression(parent) && parent.expression === child) {
      read(use, parent.name.text, parent.name)
    } else if (ts.isElementAccessExpression(parent) && parent.expression === child) {
      const name = literal(parent.argumentExpression)
      if (name === null) list(use, 'computed member', parent.argumentExpression)
      else read(use, name, parent.argumentExpression)
    } else if (ts.isQualifiedName(parent) && parent.left === child) {
      read(use, parent.right.text, parent.right)
    } else if (
      (ts.isVariableDeclaration(parent) || ts.isParameter(parent) || ts.isBindingElement(parent)) &&
      parent.initializer === child
    ) {
      bind(use, parent.name, parent)
    } else if (!ts.isExpressionStatement(parent) && !(assigned && parent.left === child)) {
      list(use, 'namespace used whole', child)
    }
  }
  const dynamic = (use, call) => {
    const {child, parent} = outer(call)
    if (ts.isAwaitExpression(parent)) return value(use, parent)
    if (ts.isExpressionStatement(parent)) return
    const then = parent.parent
    if (
      ts.isPropertyAccessExpression(parent) &&
      parent.expression === child &&
      parent.name.text === 'then' &&
      ts.isCallExpression(then) &&
      then.expression === parent
    ) {
      const callback = then.arguments[0]
      if (callback && (ts.isArrowFunction(callback) || ts.isFunctionExpression(callback))) {
        const [first, second] = callback.parameters
        const parameter = first && ts.isIdentifier(first.name) && first.name.text === 'this' ? second : first
        if (parameter?.dotDotDotToken) list(use, 'rest element', parameter)
        else if (parameter) bind(use, parameter.name, parameter)
        return
      }
    }
    list(use, 'namespace used whole', call)
  }
  for (const {node, use} of uses) {
    if (ts.isImportDeclaration(node)) {
      const clause = node.importClause
      if (clause?.name) read(use, 'default', node)
      const bindings = clause?.namedBindings
      if (bindings && ts.isNamedImports(bindings)) {
        for (const element of bindings.elements) read(use, (element.propertyName ?? element.name).text, node)
      } else if (bindings) {
        bind(use, bindings.name, bindings)
      }
    } else if (ts.isExportDeclaration(node)) {
      const clause = node.exportClause
      if (!clause) list(use, 'export *', node)
      else if (ts.isNamespaceExport(clause)) list(use, 'export * as', node)
      else for (const element of clause.elements) read(use, (element.propertyName ?? element.name).text, node)
    } else {
      dynamic(use, node)
    }
  }
  if (!queue.length) return {stops, kept}
  const checker = checkerOf(source)
  const identifiers = new Map()
  const collect = node => {
    if (ts.isIdentifier(node)) {
      if (!identifiers.has(node.text)) identifiers.set(node.text, [])
      identifiers.get(node.text).push(node)
    }
    ts.forEachChild(node, collect)
  }
  collect(source)
  while (queue.length) {
    const declaration = queue.shift()
    for (const identifier of identifiers.get(declaration.name.text) ?? []) {
      if (refersTo(checker, identifier, declaration)) value(namespaces.get(declaration), identifier)
    }
  }
  return {stops, kept}
}

/**
 * Reads the source `file` of `dir` and returns its edits, its stops and the forms it keeps and
 * lists, with `own` the packages the app depends on itself.
 *
 * With `mark` a stop row whose names lib/unsupported.js of the package aosabi carries becomes a
 * rewrite to aosabi/unsupported/<subpath>. A name with no engine item stays a stop with the flag or
 * without it, in the forms `names` gives. A stop of a specifier or of a name in an import or an
 * export names the line of the statement, as G8 of 36_ts1_gate counts it, and a stop of a name read
 * off a namespace names the line of the name.
 */
function scan(dir, file, mark, own) {
  const bytes = fs.readFileSync(path.join(dir, file))
  const text = decode(bytes)
  if (text === null) {
    const stop = {file, line: 1, message: 'not UTF-8, so the step cannot keep its bytes'}
    return {file, bytes, edits: [], stops: [stop], kept: []}
  }
  const starts = lineStarts(bytes)
  const source = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true)
  const at = position => Buffer.byteLength(text.slice(0, position), 'utf8')
  const lineAt = node => lineOf(starts, at(node.getStart(source)))
  const found = []
  const stops = []
  const kept = []
  const uses = []
  const imports = []
  const variables = []
  const plugins = pluginsOf(source)
  const visit = node => {
    const variable = variableOf(node)
    if (variable) variables.push(variable)
    const hook = hookOf(node)
    if (hook && plugins.has(node.parent)) {
      const why = 'has no engine item in aosabi, which runs onExecute alone of the hooks of Envelop and Yoga'
      stops.push({file, line: lineAt(node), message: `the hook ${hook} of a plugin ${why}`})
    } else if (hook) {
      kept.push({file, line: lineAt(node), form: 'a member of no plugin named as the hook', specifier: hook})
    }
    const named = specifierOf(node)
    if (named) {
      const specifier = named.literal.text
      const verdict = classify(specifier, own)
      if (named.kept) {
        if (verdict.kind !== 'keep') kept.push({file, line: lineAt(named.literal), form: named.form, specifier})
      } else if (verdict.kind === 'own') {
        kept.push({file, line: lineAt(node), form: `a specifier of ${verdict.name}, a dependency of the app`, specifier})
      } else if (verdict.kind === 'rewrite') {
        found.push({literal: named.literal, form: named.form, specifier, to: verdict.to})
        const banned = loadTable().names.get(specifier)
        if (banned && named.form !== 'declare module') uses.push({node, use: {specifier, banned}})
        const renamed = loadTable().renames.get(specifier)
        if (renamed && (named.form === 'import' || named.form === 'export')) imports.push({node, specifier, renamed})
      } else if (verdict.kind === 'stop' && mark && verdict.stop.mark) {
        const to = UNSUPPORTED + specifier.slice(loadTable().origin.length + 1)
        found.push({literal: named.literal, form: named.form, specifier, to, marked: verdict.stop.until})
      } else if (verdict.kind === 'stop') {
        stops.push({file, line: lineAt(node), message: `the specifier '${specifier}' ${reason(verdict.stop)}`})
      }
    }
    ts.forEachChild(node, visit)
  }
  visit(source)
  const named = names(file, source, lineAt, uses)
  stops.push(...named.stops)
  kept.push(...named.kept)
  const byLine = (a, b) => a.line - b.line
  const edits = found.map(edit => {
    const start = at(edit.literal.getStart(source)) + 1
    const end = at(edit.literal.getEnd()) - 1
    return {
      line: lineOf(starts, start),
      offset: start,
      form: edit.form,
      specifier: edit.specifier,
      from: bytes.subarray(start, end).toString('utf8'),
      to: edit.to,
      marked: edit.marked
    }
  })
  for (const {node, row} of variables) {
    const start = at(node.getStart(source)) + (ts.isIdentifier(node) ? 0 : 1)
    edits.push({line: lineOf(starts, start), offset: start, form: 'variable', specifier: 'env', from: row.name, to: row.to})
  }
  for (const identifier of renamedNames(source, imports)) {
    const start = at(identifier.node.getStart(source))
    edits.push({
      line: lineOf(starts, start),
      offset: start,
      form: 'rename',
      specifier: identifier.specifier,
      from: identifier.node.text,
      to: identifier.to
    })
  }
  edits.sort((a, b) => a.offset - b.offset)
  return {file, bytes, starts, edits, stops: stops.sort(byLine), kept: kept.sort(byLine)}
}

/**
 * Returns the name of a variable of a variable row that `node` reads or writes on an object env,
 * with its row, or null: the identifier of `process.env.NAME` or the string literal of
 * `process.env['NAME']`.
 */
function variableOf(node) {
  const {variables} = loadTable()
  const onEnv = expression => ts.isPropertyAccessExpression(expression) && expression.name.text === 'env'
  if (ts.isPropertyAccessExpression(node) && onEnv(node.expression) && variables.has(node.name.text)) {
    return {node: node.name, row: variables.get(node.name.text)}
  }
  if (ts.isElementAccessExpression(node) && onEnv(node.expression) && ts.isStringLiteral(node.argumentExpression) &&
      variables.has(node.argumentExpression.text)) {
    return {node: node.argumentExpression, row: variables.get(node.argumentExpression.text)}
  }
  return null
}

/**
 * Returns the identifiers of `source` that a rename row renames, each with its specifier and its
 * new name: the imported name of a named import or of an export declaration of `imports`, and every
 * identifier the checker resolves to the binding of such an import that keeps the imported name.
 */
function renamedNames(source, imports) {
  const out = []
  const bindings = []
  for (const {node, specifier, renamed} of imports) {
    const clause = ts.isImportDeclaration(node) ? node.importClause?.namedBindings : node.exportClause
    const elements = clause && (ts.isNamedImports(clause) || ts.isNamedExports(clause)) ? clause.elements : []
    for (const element of elements) {
      const imported = element.propertyName ?? element.name
      const row = renamed.get(imported.text)
      if (!row) continue
      out.push({node: imported, specifier, to: row.to})
      if (ts.isImportSpecifier(element) && !element.propertyName) bindings.push({element, specifier, to: row.to})
    }
  }
  if (!bindings.length) return out
  const checker = checkerOf(source)
  const visit = node => {
    if (ts.isIdentifier(node)) {
      for (const binding of bindings) {
        if (node !== binding.element.name && node.text === binding.element.name.text &&
            refersTo(checker, node, binding.element)) {
          out.push({node, specifier: binding.specifier, to: binding.to})
        }
      }
    }
    ts.forEachChild(node, visit)
  }
  visit(source)
  return out
}

/**
 * Returns `bytes` with every edit of `edits` applied, each edit replacing the bytes of its `from`
 * at its `offset` by its `to`, and sets on each edit the offset `at` of its `to` in the result.
 */
function apply(bytes, edits) {
  const parts = []
  let last = 0
  let shift = 0
  for (const edit of [...edits].sort((a, b) => a.offset - b.offset)) {
    const from = Buffer.from(edit.from, 'utf8')
    const to = Buffer.from(edit.to, 'utf8')
    parts.push(bytes.subarray(last, edit.offset), to)
    edit.at = edit.offset + shift
    shift += to.length - from.length
    last = edit.offset + from.length
  }
  parts.push(bytes.subarray(last))
  return Buffer.concat(parts)
}

/** Returns the property named `name` of the object literal `object`, or null. */
function property(object, name) {
  if (!object || !ts.isObjectLiteralExpression(object)) return null
  for (const member of object.properties) {
    if (ts.isPropertyAssignment(member) && ts.isStringLiteral(member.name) && member.name.text === name) return member
  }
  return null
}

/** Returns the root object of the JSON text `text` named `file`, or null when it does not parse. */
function jsonRoot(file, text) {
  const json = ts.parseJsonText(file, text)
  if (json.parseDiagnostics?.length) return null
  const root = json.statements[0]?.expression
  return root && ts.isObjectLiteralExpression(root) ? {json, root} : null
}

/**
 * Reads the package.json of `dir` and returns its edits and what it reports apart: the dependency
 * on the package of the table swapped for the package aosabi at the version of this package, each
 * build command of a script swapped for `aosabi build`, each path of a script into the build folder
 * of the table pointed at .aosabi, and every other script that runs the old command, such as its
 * dev server, and every other dependency on a package pulled in by default or of the scope of the
 * swapped package kept.
 *
 * A script runs the old command where `binCommands` finds it as a command. A path into the build
 * folder is the name of the folder as a word of its own or as a segment of a path, `<folder>`,
 * `./<folder>/server.mjs` or `app/<folder>/worker.mjs`, while a name that runs on, such as
 * `<folder>-cache` or `x<folder>`, is none.
 */
function scanPackage(dir) {
  const file = 'package.json'
  const full = path.join(dir, file)
  if (!fs.existsSync(full)) return null
  const {map, version, pulled, origin, bin, binTo, folder} = loadTable()
  const scope = origin.slice(0, origin.indexOf('/') + 1)
  const bytes = fs.readFileSync(full)
  const result = {file, bytes, edits: [], stops: [], dependency: [], scripts: [], keptScripts: [], keptDependencies: []}
  const text = decode(bytes)
  const parsed = text === null ? null : jsonRoot(file, text)
  if (!parsed) {
    result.stops.push({file, line: 1, message: 'not a JSON object in UTF-8, so the step cannot keep its bytes'})
    return result
  }
  const {json, root} = parsed
  const starts = lineStarts(bytes)
  const at = position => Buffer.byteLength(text.slice(0, position), 'utf8')
  const inner = literal => {
    const start = at(literal.getStart(json)) + 1
    const end = at(literal.getEnd()) - 1
    return {offset: start, line: lineOf(starts, start), from: bytes.subarray(start, end).toString('utf8')}
  }
  for (const section of SECTIONS) {
    const listed = property(root, section)?.initializer
    if (!listed || !ts.isObjectLiteralExpression(listed)) continue
    for (const member of listed.properties) {
      if (!ts.isPropertyAssignment(member) || !ts.isStringLiteral(member.name)) continue
      const name = member.name.text
      const spec = ts.isStringLiteral(member.initializer) ? member.initializer.text : null
      if (name !== map.package.from) {
        if (pulled.has(name) || name.startsWith(scope)) result.keptDependencies.push({section, name, spec})
        continue
      }
      if (spec === null || property(listed, map.package.to)) {
        const why = spec === null ? 'no version string' : `'${map.package.to}' beside it`
        result.stops.push({file, line: inner(member.name).line, message: `${section} names '${name}' with ${why}`})
        continue
      }
      const key = inner(member.name)
      result.edits.push({...key, to: map.package.to, what: 'dependency'})
      result.edits.push({...inner(member.initializer), to: version, what: 'dependency'})
      result.dependency.push({section, line: key.line, from: {name, spec}, to: {name: map.package.to, spec: version}})
    }
  }
  const scripts = property(root, 'scripts')?.initializer
  if (scripts && ts.isObjectLiteralExpression(scripts)) {
    for (const member of scripts.properties) {
      if (!ts.isPropertyAssignment(member) || !ts.isStringLiteral(member.name)) continue
      if (!ts.isStringLiteral(member.initializer)) continue
      const value = inner(member.initializer)
      const decoded = jsonText(value.from)
      const commands = binCommands(decoded.text, bin)
      const name = member.name.text
      const swap = (index, from, to, what) => {
        const start = decoded.offsets[index]
        if (value.from.slice(start, decoded.offsets[index + from.length]) !== from) {
          result.stops.push({file, line: value.line, message: `the script ${name} writes ${from} with escapes of JSON`})
          return false
        }
        const offset = value.offset + Buffer.byteLength(value.from.slice(0, start), 'utf8')
        result.edits.push({offset, line: value.line, from, to, what})
        return true
      }
      for (const command of commands.filter(one => one.build)) swap(command.index, bin, binTo, 'script')
      const paths = folder ? folderPaths(decoded.text, folder.from) : []
      for (const index of paths) swap(index, folder.from, folder.to, 'path')
      const script = {name, line: value.line}
      if (commands.some(one => one.build) || paths.length) {
        result.scripts.push({...script, before: member.initializer.text})
      }
      if (commands.some(one => !one.build)) result.keptScripts.push(script)
    }
  }
  return result
}

/**
 * Returns the files of the app in `dir` the step renames, `renamed`, each row of the list files of
 * the table whose file `from` is a file there, and `stops`, one for each such row whose file `to` is
 * there as well.
 */
function scanRenames(dir) {
  const renamed = []
  const stops = []
  for (const row of loadTable().files) {
    const from = path.join(dir, row.from)
    if (!fs.existsSync(from) || !fs.lstatSync(from).isFile()) continue
    if (fs.existsSync(path.join(dir, row.to))) {
      stops.push({file: row.from, line: 1, message: `'${row.to}' beside it, so the step cannot rename it`})
    } else {
      renamed.push({from: row.from, to: row.to})
    }
  }
  return {renamed, stops}
}

/** Returns the log entry of the source `found` after its edits, and its new bytes. */
function logSource(found) {
  const migrated = apply(found.bytes, found.edits)
  const after = lineStarts(migrated)
  const edits = found.edits.map(edit => ({
    line: edit.line,
    offset: edit.offset,
    at: edit.at,
    form: edit.form,
    specifier: edit.specifier,
    from: edit.from,
    to: edit.to,
    before: lineText(found.bytes, found.starts, edit.line),
    after: lineText(migrated, after, edit.line)
  }))
  return {entry: {file: found.file, sha256: sha256(found.bytes), migratedSha256: sha256(migrated), edits}, migrated}
}

/** Returns the log entry of the package.json `pkg` after its edits, and its new bytes. */
function logPackage(pkg) {
  const migrated = apply(pkg.bytes, pkg.edits)
  const starts = lineStarts(pkg.bytes)
  const after = lineStarts(migrated)
  const scripts = property(jsonRoot(pkg.file, migrated.toString('utf8')).root, 'scripts')?.initializer
  const entry = {
    file: pkg.file,
    sha256: sha256(pkg.bytes),
    migratedSha256: sha256(migrated),
    dependency: pkg.dependency,
    scripts: pkg.scripts.map(script => ({...script, after: property(scripts, script.name).initializer.text})),
    keptScripts: pkg.keptScripts.map(script => ({...script, command: property(scripts, script.name).initializer.text})),
    keptDependencies: pkg.keptDependencies,
    edits: pkg.edits.map(edit => ({
      line: edit.line,
      offset: edit.offset,
      at: edit.at,
      what: edit.what,
      from: edit.from,
      to: edit.to,
      before: lineText(pkg.bytes, starts, edit.line),
      after: lineText(migrated, after, edit.line)
    }))
  }
  return {entry, migrated}
}

/** Appends `heading` and the lines of `lines` to `out`, or nothing when there are no lines. */
function section(out, heading, lines) {
  if (!lines.length) return
  out.push(heading)
  for (const line of lines) out.push('  ' + line)
}

/** Returns `count` with the singular or the plural of `noun`. */
function counted(count, noun) {
  return count + ' ' + (count === 1 ? noun : noun + 's')
}

/** Returns the report of a run that wrote `log`, each kind of change apart from the others. */
function report(log) {
  const out = []
  const rewrites = log.files.flatMap(entry => entry.edits.map(edit => ({file: entry.file, ...edit})))
  const renames = rewrites.filter(edit => edit.form === 'rename' || edit.form === 'variable').length
  const specifiers = counted(rewrites.length - renames, 'specifier')
  const rewritten = renames ? `${specifiers} and ${counted(renames, 'name')}` : specifiers
  const files = counted(log.files.length, 'file')
  out.push(`aosabi migrate: ${rewritten} rewritten in ${files}, the log in ${LOG_DIR}/${LOG_FILE}`)
  section(out, 'the sources:', rewrites.map(edit => `${edit.file}:${edit.line}: '${edit.from}' becomes '${edit.to}'`))
  section(
    out,
    'marked by --mark-unsupported, a flag of the harness:',
    log.marked.map(entry => {
      return `${entry.file}:${entry.line}: '${entry.from}' becomes '${entry.to}', no engine item before ${entry.until}`
    })
  )
  section(
    out,
    'kept and listed:',
    log.kept.map(entry => `${entry.file}:${entry.line}: ${entry.form} '${entry.specifier}'`)
  )
  section(out, 'not read, since only .ts, .mts, .js and .mjs files are:', log.unread)
  section(out, 'renamed:', (log.renamed ?? []).map(entry => `'${entry.from}' becomes '${entry.to}'`))
  const pkg = log.packageJson
  if (!pkg) return out.join('\n')
  const quoted = JSON.stringify
  section(
    out,
    'package.json, the dependency:',
    pkg.dependency.map(entry => {
      const was = `'${entry.from.name}' ${quoted(entry.from.spec)}`
      const now = `'${entry.to.name}' ${quoted(entry.to.spec)}`
      return `${pkg.file}:${entry.line}: ${entry.section} ${was} becomes ${now}`
    })
  )
  section(
    out,
    'package.json, the scripts:',
    pkg.scripts.map(entry => {
      return `${pkg.file}:${entry.line}: ${entry.name} ${quoted(entry.before)} becomes ${quoted(entry.after)}`
    })
  )
  section(out, 'package.json, kept:', [
    ...pkg.keptScripts.map(entry => {
      const script = `${entry.name} ${quoted(entry.command)}`
      return `${pkg.file}:${entry.line}: ${script}, a command that aosabi-build does not carry`
    }),
    ...pkg.keptDependencies.map(entry => `${entry.section} '${entry.name}' ${quoted(entry.spec)}`)
  ])
  if (!pkg.edits.length) {
    const {map} = loadTable()
    out.push(`package.json: no dependency on ${map.package.from} and no script that runs ${map.script.from}`)
  }
  return out.join('\n')
}

/**
 * Runs aosabi migrate over the app in `dir` with the flag `options.mark` and returns its exit code.
 *
 * A specifier of the package the step swaps, or of a package it pulls in by default, with no row
 * stops the run. So
 * does a name of a name row in a named import, in an export clause that names its module and in a
 * read off a namespace of its specifier, `ns.name`, `ns['name']`, a qualified name `ns.Name` in a
 * type, in `typeof ns.name` or in `import alias = ns.Name`, or a key of an object binding pattern
 * over a namespace, where a namespace is the binding of `import * as ns`, `await import()`, the
 * first parameter of a function that `import().then()` is called with, or a binding whose
 * initializer is a namespace, as `names` gives them, and so does a member of a plugin named as a
 * hook of `HOOKS`, which `hookOf` and `pluginsOf` give, while a member of that name of any other
 * object literal is kept and listed. Each stop names its file, its line and what keeps it out, a
 * milestone, a record, a state, a package or the hooks the engine runs, and so does a file of the
 * list files of the table whose new name is taken, and then no file is written. Otherwise the log
 * .aosabi/migrate.json is written first, the changed files after it and the renames last, so the
 * reverse finds the log of every file the run changed or renamed.
 */
export function migrate(dir, options = {}) {
  const mark = Boolean(options.mark)
  const {version} = loadTable()
  const logDir = path.join(dir, LOG_DIR)
  const logFile = path.join(logDir, LOG_FILE)
  if (fs.existsSync(logFile)) {
    const log = `${LOG_DIR}/${LOG_FILE}`
    console.error(`aosabi migrate: ${log} exists, so the app is migrated, and aosabi migrate --reverse undoes it`)
    return 1
  }
  const {read, unread} = walk(dir)
  const own = ownDependencies(dir)
  const sources = read.map(file => scan(dir, file, mark, own))
  const pkg = scanPackage(dir)
  const renames = scanRenames(dir)
  const stops = [...sources.flatMap(found => found.stops), ...(pkg?.stops ?? []), ...renames.stops]
  if (stops.length) {
    for (const stop of stops) console.error(`${stop.file}:${stop.line}: ${stop.message}`)
    console.error(`aosabi migrate: ${counted(stops.length, 'stop')}, nothing written`)
    return 1
  }
  const log = {
    tool: 'aosabi migrate',
    aosabiBuild: version,
    typescript: ts.version,
    markUnsupported: mark,
    createdLogDirectory: !fs.existsSync(logDir),
    files: [],
    marked: [],
    kept: sources.flatMap(found => found.kept),
    unread,
    packageJson: null,
    renamed: renames.renamed
  }
  const changed = []
  for (const found of sources.filter(one => one.edits.length)) {
    const {entry, migrated} = logSource(found)
    log.files.push(entry)
    for (const edit of found.edits.filter(one => one.marked)) {
      log.marked.push({file: found.file, line: edit.line, from: edit.specifier, to: edit.to, until: edit.marked})
    }
    changed.push([path.join(dir, found.file), migrated])
  }
  if (pkg) {
    const {entry, migrated} = logPackage(pkg)
    log.packageJson = entry
    if (pkg.edits.length) changed.push([path.join(dir, pkg.file), migrated])
  }
  fs.mkdirSync(logDir, {recursive: true})
  fs.writeFileSync(logFile, JSON.stringify(log, null, 2) + '\n')
  for (const [file, bytes] of changed) fs.writeFileSync(file, bytes)
  for (const rename of log.renamed) fs.renameSync(path.join(dir, rename.from), path.join(dir, rename.to))
  console.log(report(log))
  return 0
}

/**
 * Runs the reverse of aosabi migrate over the app in `dir` and returns its exit code. Every file
 * of the log gets the bytes it had before the run and every renamed file its name, the log is
 * removed, and so is .aosabi when the run made it and nothing else is in it. A file whose bytes at
 * a logged offset are not the logged ones stops the reverse, and so does a renamed file that is
 * missing or whose old name is taken, and then no file is written.
 */
export function reverse(dir) {
  const logDir = path.join(dir, LOG_DIR)
  const logFile = path.join(logDir, LOG_FILE)
  if (!fs.existsSync(logFile)) {
    console.error(`aosabi migrate --reverse: ${LOG_DIR}/${LOG_FILE} is missing, so there is nothing to reverse`)
    return 1
  }
  const log = JSON.parse(fs.readFileSync(logFile, 'utf8'))
  const logged = [...log.files, ...(log.packageJson?.edits.length ? [log.packageJson] : [])]
  const renamed = log.renamed ?? []
  const renamedTo = new Map(renamed.map(rename => [rename.from, rename.to]))
  const restored = []
  const conflicts = []
  for (const rename of renamed) {
    if (!fs.existsSync(path.join(dir, rename.to))) conflicts.push(`${rename.to}: missing`)
    else if (fs.existsSync(path.join(dir, rename.from))) conflicts.push(`${rename.from}: exists beside ${rename.to}`)
  }
  let later = 0
  for (const entry of logged) {
    const now = renamedTo.get(entry.file) ?? entry.file
    const file = path.join(dir, now)
    if (!fs.existsSync(file)) {
      conflicts.push(`${now}: missing`)
      continue
    }
    const bytes = fs.readFileSync(file)
    if (sha256(bytes) === entry.sha256) continue
    const back = entry.edits.map(edit => ({offset: edit.at, from: edit.to, to: edit.from, line: edit.line}))
    const moved = back.filter(edit => {
      const expected = Buffer.from(edit.from, 'utf8')
      return !bytes.subarray(edit.offset, edit.offset + expected.length).equals(expected)
    })
    for (const edit of moved) {
      conflicts.push(`${now}:${edit.line}: '${edit.from}' is no longer at byte ${edit.offset}`)
    }
    if (moved.length) continue
    const original = apply(bytes, back)
    if (sha256(original) !== entry.sha256) later++
    restored.push([file, original])
  }
  if (conflicts.length) {
    for (const conflict of conflicts) console.error(conflict)
    console.error(`aosabi migrate --reverse: ${counted(conflicts.length, 'conflict')}, nothing written`)
    return 1
  }
  for (const [file, bytes] of restored) fs.writeFileSync(file, bytes)
  for (const rename of renamed) fs.renameSync(path.join(dir, rename.to), path.join(dir, rename.from))
  fs.rmSync(logFile)
  if (log.createdLogDirectory && fs.readdirSync(logDir).length === 0) fs.rmdirSync(logDir)
  const names = renamed.length ? `, ${counted(renamed.length, 'name')} given back` : ''
  console.log(`aosabi migrate --reverse: ${counted(restored.length, 'file')} restored${names}, the log removed`)
  if (later) {
    console.log(`aosabi migrate --reverse: ${later} of them differ from their bytes before the run by later changes`)
  }
  return 0
}
