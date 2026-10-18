// SPDX-License-Identifier: MIT
/*
 * aosabi-build test/migrate: the paths of the scripts into the build folder and the renamed files of
 * aosabi migrate, and their reverse. Run with node --test test/.
 */

import assert from 'node:assert/strict'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import {test} from 'node:test'
import {migrate, reverse} from '../lib/migrate.js'

const map = JSON.parse(fs.readFileSync(new URL('../migrate-map.json', import.meta.url), 'utf8'))
const {version} = JSON.parse(fs.readFileSync(new URL('../package.json', import.meta.url), 'utf8'))
const OLD = map.folder.from
const NEW = map.folder.to
const BIN = map.script.from.split(' ')[0]
const [CONFIG, DECLARATIONS] = [map.files[0], map.files.at(-1)]

/** Makes an app in a new folder from `files`, a map of a relative path to its text. */
function app(files) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'aosabi-migrate-'))
  for (const [name, text] of Object.entries(files)) {
    fs.mkdirSync(path.dirname(path.join(dir, name)), {recursive: true})
    fs.writeFileSync(path.join(dir, name), text)
  }
  return dir
}

/** Returns the text of every file under `dir`, keyed by its relative path. */
function snapshot(dir) {
  return Object.fromEntries(
    fs.readdirSync(dir, {recursive: true})
      .filter(name => fs.statSync(path.join(dir, name)).isFile())
      .sort()
      .map(name => [name, fs.readFileSync(path.join(dir, name), 'utf8')])
  )
}

test('points the scripts at the new build folder, renames the files and reverses both', () => {
  const pkg = {
    name: 'app',
    dependencies: {[map.package.from]: '^2.9.5'},
    scripts: {
      build: `${BIN} build`,
      start: `node ${OLD}/server.mjs`,
      deploy: `wrangler deploy ./${OLD}/worker.mjs --name=app`,
      clean: `rm -rf ${OLD} && echo done`,
      nested: `cd app/${OLD}/x;ls '${OLD}/a'`,
      dev: `${BIN} dev`,
      other: `echo x${OLD}/y ${OLD}-cache`
    }
  }
  const files = {
    'package.json': JSON.stringify(pkg, null, 2) + '\n',
    [CONFIG.from]: 'export default {}\n',
    [DECLARATIONS.from]: 'export {}\n',
    'src/index.ts': 'export default {}\n'
  }
  const dir = app(files)
  const before = snapshot(dir)
  assert.equal(migrate(dir), 0)
  const after = JSON.parse(fs.readFileSync(path.join(dir, 'package.json'), 'utf8'))
  assert.deepEqual(after.dependencies, {[map.package.to]: version})
  assert.deepEqual(after.scripts, {
    build: map.script.to,
    start: `node ${NEW}/server.mjs`,
    deploy: `wrangler deploy ./${NEW}/worker.mjs --name=app`,
    clean: `rm -rf ${NEW} && echo done`,
    nested: `cd app/${NEW}/x;ls '${NEW}/a'`,
    dev: `${BIN} dev`,
    other: `echo x${OLD}/y ${OLD}-cache`
  })
  assert.ok(!fs.existsSync(path.join(dir, CONFIG.from)))
  assert.equal(fs.readFileSync(path.join(dir, CONFIG.to), 'utf8'), files[CONFIG.from])
  assert.ok(!fs.existsSync(path.join(dir, DECLARATIONS.from)))
  assert.ok(fs.existsSync(path.join(dir, DECLARATIONS.to)))
  const log = JSON.parse(fs.readFileSync(path.join(dir, '.aosabi', 'migrate.json'), 'utf8'))
  assert.deepEqual(log.renamed, [{from: CONFIG.from, to: CONFIG.to}, {from: DECLARATIONS.from, to: DECLARATIONS.to}])
  assert.equal(reverse(dir), 0)
  assert.deepEqual(snapshot(dir), before)
  assert.ok(!fs.existsSync(path.join(dir, '.aosabi')))
  fs.rmSync(dir, {recursive: true})
})

test('rewrites the specifiers of a renamed file and gives it its bytes and name back', () => {
  const config = `import '${map.package.from}'\nexport default {}\n`
  const dir = app({[CONFIG.from]: config, 'src/index.ts': 'export default {}\n'})
  assert.equal(migrate(dir), 0)
  assert.equal(fs.readFileSync(path.join(dir, CONFIG.to), 'utf8'), `import '${map.entries[0].entry}'\nexport default {}\n`)
  assert.equal(reverse(dir), 0)
  assert.equal(fs.readFileSync(path.join(dir, CONFIG.from), 'utf8'), config)
  assert.ok(!fs.existsSync(path.join(dir, CONFIG.to)))
  fs.rmSync(dir, {recursive: true})
})

test('stops where the new name is taken and writes nothing', () => {
  const files = {[CONFIG.from]: 'export default {}\n', [CONFIG.to]: 'export default {}\n'}
  const dir = app(files)
  assert.equal(migrate(dir), 1)
  assert.deepEqual(snapshot(dir), files)
  fs.rmSync(dir, {recursive: true})
})

test('stops on a path into the build folder written with escapes of JSON', () => {
  const text = `{"scripts": {"start": "node \\u002e${OLD.slice(1)}/server.mjs"}}\n`
  const dir = app({'package.json': text})
  assert.equal(migrate(dir), 1)
  assert.equal(fs.readFileSync(path.join(dir, 'package.json'), 'utf8'), text)
  fs.rmSync(dir, {recursive: true})
})

test('stops the reverse where the old name is taken again', () => {
  const dir = app({[CONFIG.from]: 'export default {}\n'})
  assert.equal(migrate(dir), 0)
  fs.writeFileSync(path.join(dir, CONFIG.from), 'export default {}\n')
  assert.equal(reverse(dir), 1)
  assert.ok(fs.existsSync(path.join(dir, '.aosabi', 'migrate.json')))
  fs.rmSync(dir, {recursive: true})
})
