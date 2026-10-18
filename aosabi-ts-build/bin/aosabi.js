#!/usr/bin/env node
// SPDX-License-Identifier: MIT
/*
 * aosabi-build bin/aosabi: the command aosabi of the dev package, which runs aosabi migrate and its
 * reverse and aosabi build, the author mode.
 *
 * The command reads its arguments and leaves every rule of a step to its module under lib/, which
 * it loads for that step alone, so typescript loads only where a step parses. Its exit code is 0
 * when the step ran, 1 when the step stopped or refused, and 2 for a call it does not know.
 */

import fs from 'node:fs'

const USAGE = `usage: aosabi migrate [--mark-unsupported] <dir>
       aosabi migrate --reverse <dir>
       aosabi build
       aosabi --version

The step aosabi migrate moves an app written for Pylon v3 in <dir> to the package aosabi. It
rewrites the import specifiers of the app to the entries of the package aosabi, and the name of a
declare module block of such a specifier, swaps the dependency of its package.json and every build
command of its scripts, points every path of its scripts into the build folder at .aosabi/,
renames its configuration file to aosabi.config.ts, .mts, .js or .mjs and its declaration file to
aosabi.d.ts, and logs every rewritten line and every renamed file in <dir>/.aosabi/migrate.json.
A specifier of the old package or of a package it pulls in by default that has no entry stops the
run, and so does a name
with no engine item in a named import, in an export clause and in a read off a namespace of its
specifier, ns.name, ns['name'], a qualified name ns.Name in a type, in typeof or in
import alias = ns.Name, or a key of an object binding pattern over a namespace, where a
namespace is the binding of import * as ns, await import(), the first parameter of a function
that import().then() is called with, or a binding whose initializer is a namespace. Each stop
names its file, its line and what keeps it out, a milestone, a record, a state or a package, and
then nothing is written. Where the nearest package.json of the app names a package pulled in by
default as its own dependency and the table maps no specifier of it, such as pg, a specifier of it is kept
and listed in the log in place of a stop. A use whose names the step cannot tell is kept and
listed in the log: a computed member, a rest element, the namespace used whole, such as f(ns),
export {ns}, typeof ns, a binding of it exported by the keyword export, in the module or in a
TypeScript namespace, or a parameter property over it, and export * and export * as ns.
A member of a plugin named as a hook of Envelop or Yoga that the engine runs no item of, every
hook of Envelop and Yoga but onExecute, stops the run as well with "the hook <hook> of a
plugin has no engine item in aosabi", where a plugin is an object literal that holds name, setup,
middleware, onExecute, dependsOn, roles or strategy, or that stands in an array literal of a
member plugins, the array written there, read through a variable of the same file whose
initializer is that array or through the shorthand member {plugins} of such a variable, and the
plugin written there or read through a variable of the same file whose initializer is that
object, while a second variable over such a variable and an assignment to a variable after its
declaration are not read. A member of that name of any other object literal is kept and listed in
the log.
The flag --mark-unsupported of the harness rewrites a specifier of the subpaths db, db/plugin,
queues, pages, pages/plugin, query and ir, which wait for M9, to
aosabi/unsupported/<subpath> in place of a stop. The flag --reverse restores every file the log
names byte for byte and removes the log.

The step aosabi build builds the app of the working directory: it makes a program of typescript
of src/ with the compiler options of its tsconfig.json, infers the schema of
the default export of src/index.ts with infer.wasm over the checker of that program, transpiles
every file of src/ and the configuration into .aosabi/, keeping every import, and writes the glue
there, schema.graphql, schema.mjs, resolvers.js, ir.json, ir.mjs, server.mjs and worker.mjs, the
entry of a Cloudflare Worker. It loads the configuration it transpiled. A source with a syntax
error, a configuration that throws when it loads and a schema that forms no valid GraphQL schema
fail the build before a file of the glue is written.`

/** Resolves to the exit code of the command line `args`. */
async function run(args) {
  const [command, ...rest] = args
  if (command === undefined || command === '--help' || command === '-h' || command === 'help') {
    console.log(USAGE)
    return command === undefined ? 2 : 0
  }
  if (command === '--version') {
    const own = JSON.parse(fs.readFileSync(new URL('../package.json', import.meta.url), 'utf8'))
    console.log(`aosabi-build ${own.version}`)
    return 0
  }
  if (command === 'build') {
    if (rest.length) {
      console.error(`aosabi build: no argument ${rest.join(', ')}, the step builds the app of the working directory\n\n${USAGE}`)
      return 2
    }
    const {buildCommand} = await import('../lib/build.js')
    return buildCommand(process.cwd())
  }
  if (command !== 'migrate') {
    console.error(`aosabi: no command ${JSON.stringify(command)}\n\n${USAGE}`)
    return 2
  }
  const flags = rest.filter(arg => arg.startsWith('-'))
  const dirs = rest.filter(arg => !arg.startsWith('-'))
  const unknown = flags.filter(flag => flag !== '--mark-unsupported' && flag !== '--reverse')
  const both = flags.includes('--mark-unsupported') && flags.includes('--reverse')
  if (unknown.length || both || dirs.length !== 1) {
    let why = 'one <dir> is needed'
    if (unknown.length) why = `no flag ${unknown.join(', ')}`
    else if (both) why = '--reverse takes no --mark-unsupported'
    console.error(`aosabi migrate: ${why}\n\n${USAGE}`)
    return 2
  }
  const [dir] = dirs
  if (!fs.existsSync(dir) || !fs.statSync(dir).isDirectory()) {
    console.error(`aosabi migrate: no directory ${dir}`)
    return 2
  }
  const {migrate, reverse} = await import('../lib/migrate.js')
  if (flags.includes('--reverse')) return reverse(dir)
  return migrate(dir, {mark: flags.includes('--mark-unsupported')})
}

process.exitCode = await run(process.argv.slice(2))
