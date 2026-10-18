# aosabi-build

The dev package `aosabi-build` carries the command `aosabi` of the TypeScript option of Aosabi. It
depends on `typescript` 5.9.3 alone, pinned exact at the version the lock of the reference pins,
runs on Node at build time on the machine that builds an app, and ships in no build.

This folder is the source of the package until the owner publishes it, and nothing of it is
published.

## A path to show

An app written for Pylon v3 becomes an app of Aosabi in four commands, the path a presentation
shows. The folder of the app installs `aosabi-build` beside it, or runs it from an install of its
own, and `aosabi`, which `aosabi migrate` names in place of the old dependency in its `package.json`:

```sh
npx aosabi migrate .          # the import specifiers, the dependency, the scripts and the file names
npm install                   # aosabi, its platform package of this machine and aosabi-build
npx aosabi build              # .aosabi/ with the schema, the transpiled sources and the glue
PORT=4000 bun --preload aosabi/bun .aosabi/server.mjs
```

Bun serves the default export of the glue, the app, and the preloaded entry `aosabi/bun` hands it
the engine of the native library of its platform package, or `core.wasm` where none loads, and the
IR the glue exposes. A Worker takes `.aosabi/worker.mjs` as the main of its Wrangler configuration,
with the flag `nodejs_compat_v2` and `import.meta.url` defined. On the e2e fixture `blog-app` of
the compatibility corpus, `aosabi build` writes `schema.graphql`, `schema.mjs` and `resolvers.js`
byte equal to those of the reference build of the same source, and the build answers every case
the harness sends it with 0 diffs, on workerd and on Bun in both modes. The four
commands above ran over that fixture installed from the tarballs of the three packages of this
repository with `typescript` 5.9.3 alone beside them, where the build took 0.72 s and four requests,
a query of data, an introspection that the configuration of the app refuses, a query that misses a
required argument and a mutation whose resolver answers a value the schema refuses, answered byte
equal to the reference on Node on both engines.

## The step `aosabi migrate`

```sh
npx aosabi migrate <dir>
npx aosabi migrate --reverse <dir>
```

- The step reads every `.ts`, `.mts`, `.js` and `.mjs` file under `<dir>`, outside `node_modules`
  and every directory whose name starts with a dot, through the parser of `typescript`. It rewrites
  the module specifier of four forms alone: the import declaration, the export declaration that
  names a module, the call of `import()` with a literal and the name of a `declare module` block, so
  an augmentation such as the one of `Bindings` and `Variables` in the declaration file of the app
  applies to the entry the app imports. A specifier in a string or a comment is never
  touched, and a rewrite replaces the bytes between the quotes of one literal and keeps its quote
  character and every other byte.
- A rename row of the table gives a name of a specifier the name the package exports it under,
  such as `AppConfig`, `AppConfigFactory`, `AppOptions` and `Role` of the root. The step renames the name where a named import or an
  export declaration of the specifier names it, and every identifier the checker resolves to the
  binding of such an import, and an import under an alias of its own renames the imported name
  alone. A variable row gives a variable of the environment the name the engine reads it under,
  `AOSABI_ROLE`, `AOSABI_DEV`, `AOSABI_DEBUG` and `AOSABI_LOG_FORMAT`, where the code reads or
  writes it on an object `env`, as the worker entry of a build sets the role. Each rename is one
  more edit of the log, which the reverse undoes as it undoes a specifier.
- A specifier of the old package with no entry, such as `@getcronit/pylon/db`, stops the run with
  the file, the line and the milestone that brings it, or the state that keeps it out where no milestone does,
  and then nothing is written. A package whose specifiers the table maps, such as `hono`, stops on a
  specifier with no row whatever the app names. Any other specifier, such as `node:fs`, a package of
  the app's own or a peer, is kept.
- A name of a name row of the table, such as `getSignedCookie` of the root, stops the run in the same
  way, with the milestone that brings it or the record that leaves it aside, wherever the step reads
  it. That is a named import, of types alone or not, and an export clause that names the module,
  each at the line of the statement, and a read off a namespace of the specifier, at the line of the
  name. A namespace is the binding of `import * as ns`, the expression `await import()` of the
  specifier, the first parameter of a function that `import().then()` is called with, and every
  binding whose initializer is a namespace, such as `const ns = await import()` or
  `const alias = ns`. A read is a member `ns.name` or `ns['name']`, a qualified name `ns.Name` in a
  type, in `typeof ns.name` or in `import alias = ns.Name`, and a key of an object binding pattern
  whose initializer is a namespace or which is that first parameter, such as `const {name} = ns`.
  Parentheses and the assertions `as`, `satisfies`, `<T>` and `!` around a namespace change none of
  this. The checker of `typescript` tells which declaration each identifier refers to, so a
  parameter or a variable that shadows a namespace reads no name of it.
- A use of a namespace whose names the step cannot tell is kept and listed in the log: a computed
  member `ns[key]`, a rest element, in a pattern `{...rest}` or as the parameter `(...args)` of the
  function that `import().then()` is called with, and the namespace used whole, such as `f(ns)`,
  `export {ns}` or `typeof ns`, and so are `export *` and `export * as ns` of a specifier with a
  name row. A binding of a namespace that is also read as an export or as a member is the namespace
  used whole as well, listed at its name: a declaration exported by its keyword `export`, in the
  module or in a TypeScript namespace, such as `export const alias = ns`, and a parameter property,
  such as `constructor(public api = ns)`. The reads of such a binding by its name in the file still
  stop. An import type and an `import x = require()` declaration that name a specifier of the
  table are kept and listed as well, and a `.tsx`, `.jsx`, `.cts` or `.cjs` file is not read and is
  listed.
- A member of a plugin named as one of the sixteen hooks of Envelop and of Yoga the engine runs no
  item of, every hook a plugin may hold but `onExecute`, such as `onParse` or
  `onRequest`, stops the run with the file, the line and the message "the hook <hook> of a plugin
  has no engine item in aosabi, which runs onExecute alone of the hooks of Envelop and Yoga", with
  the flag `--mark-unsupported` as without it, since the boot of the engine refuses such a plugin
  with the name of the hook. A member counts where it is a property, a method, an accessor or a
  shorthand property of an object literal, named by an identifier, a string or a computed name with
  a literal. A plugin is an object literal that holds a key of a plugin, `name`, `setup`,
  `middleware`, `onExecute`, `dependsOn`, `roles` or `strategy`, or that the source places among
  the plugins of a configuration, an element of an array literal of a member `plugins`, the array
  written there, read through a variable of the same file whose initializer is that array or read
  through the shorthand member `{plugins}` of such a variable, and the element written there or read
  through a variable of the same file whose initializer is that object. A member of that name of any
  other object literal, such as the options `{onRequest, onResponse}` of a client of HTTP, is kept
  and listed in the log. An object that reaches the plugins through a call, a spread, a condition, a
  second variable over such a variable, an assignment to a variable after its declaration, a member
  of another object or another file, or that stands in an array of plugins that reaches them so,
  and holds no key of a plugin is not read as one, and the boot of the engine refuses it with the
  name of its hook.
- In `package.json` the dependency `@getcronit/pylon` becomes `aosabi` at the version of this
  package, in its place, and every build command of a script, the row `script` of the table,
  becomes `aosabi build`. The old command, as a word or as a path to its file in a directory
  `.bin`, where a package manager links the bins of its packages, is a command where a command
  starts outside quotes: at the start of the script and after `&&`, `||`, `;`, `|`, `&`, `(` or a
  line break, after the reserved words `!`, `{`, `if`, `then`, `elif`, `else`, `while`, `until` and
  `do` and the variable assignments that open a command, and after a runner with its flags, the
  value of each flag that takes one, its variable assignments and its subcommand. The runners are
  `npx`, `bunx`, `pnpx`, `cross-env`, `env`, `pnpm` or `yarn` with or without `exec` or `dlx`,
  `npm exec`, `npm x` and `bun x`, and the flags that take the next word as their value are these,
  as the help of each tool gives them.
  - `npm`, `npm exec` and `npm x`: `-C`, `--prefix`, `-c`, `--call`, `-w`, `--workspace`,
    `--package`, `-L`, `--location`, `--userconfig`, `--registry`, `--reg`, `--cache`, `--loglevel`
    and `--script-shell`, where `-p` is `--parseable` and takes none. `npx` takes these, `-p` of
    `--package` and `--shell`.
  - `pnpm`, `pnpm exec` and `pnpm dlx`: `-C`, `--dir`, `-F`, `--filter`, `--filter-prod`,
    `--test-pattern`, `--changed-files-ignore-pattern`, `--workspace-concurrency`, `--resume-from`,
    `--reporter`, `--loglevel`, `--package` and `--allow-build`, and `pnpx` takes the last two.
  - `yarn`, `yarn exec` and `yarn dlx`: `--cwd`, `--cache-folder`, `--preferred-cache-folder`,
    `--global-folder`, `--link-folder`, `--modules-folder`, `--use-yarnrc`, `--mutex`,
    `--network-concurrency`, `--network-timeout`, `--otp`, `--proxy`, `--https-proxy` and
    `--registry`, and `-p` and `--package` of `yarn dlx`.
  - `bunx`: `-p` and `--package`, and `bun` and `bun x` take these two, `--cwd`, `-c`, `--config`
    and `--env-file`.
  - `env`: `-a`, `--argv0`, `-u`, `--unset`, `-C` and `--chdir`, and `cross-env` takes no flag.

  The old command whose next word is `build` is swapped for `aosabi`, and the value of a flag of a
  runner is kept, while the word as the argument of `echo`, as the value of `pnpm --filter` or as a
  path outside a directory `.bin` stays as it is. Every path into the old build folder, the row
  `folder` of the table, becomes a path into `.aosabi`, where the name of the folder is a word of
  its own or a segment of a path, so `node <folder>/server.mjs` and `wrangler deploy
  ./<folder>/worker.mjs` take `.aosabi`, while a name that runs on, such as `<folder>-cache`, stays.
  A script that writes such a command or such a path with escapes of JSON stops the run, since the
  step swaps no escaped bytes. Every other command of the old package, such as its dev server, is
  kept, and the dependency, the scripts, the renamed files and what is kept are each reported apart
  from the sources.
- The files of the list `files` of the table at the root of `<dir>`, the configuration file of the
  app with the extension `.ts`, `.mts`, `.js` or `.mjs` and its declaration file, take the names
  `aosabi.config.ts`, `.mts`, `.js` or `.mjs` and `aosabi.d.ts`, after the step rewrote their
  specifiers. A file whose new name is taken stops the run.
- The log `<dir>/.aosabi/migrate.json` holds every rewritten line with its offsets and its text
  before and after, the SHA-256 of each changed file before and after and every renamed file. The
  step `--reverse` restores every file the log names byte for byte and under its old name and
  removes the log.
- The flag `--mark-unsupported` belongs to the harness. Every other specifier with no entry, such as
  `@getcronit/pylon/queues/plugin`, and every name with no engine item stay a stop with the flag.

The exit code is 0 when the step ran, 1 when it stopped or refused and 2 for a call the command does
not know.

## The step `aosabi build`

```sh
npx aosabi build
```

The step builds the app of the working directory in the order of the inference, the transpile and
the glue, and loads the configuration of the app:

- The program. It makes a `ts.Program` of the `.ts` files of `src/` and `aosabi.d.ts` beside that
  folder as its roots, the compiler options of the nearest `tsconfig.json` with `strictNullChecks`
  forced, or the defaults of the step where none reads, and the default export of `src/index.ts` as
  the app, whose property `graphql` holds the three roots. A missing entry or default export fails
  the step with a message that names it.
- The inference. The module prints the schema text, writes the IR as `JSON.stringify` writes it,
  writes `resolvers.js` and validates the schema with the writers of `aosabi-build`, the crate of
  the Rust option. A call of the checker that throws, a trap of the module, inside the answer of an
  import as well, or an answer that does not read fails the step, and a trapped instance is called
  no more. Each warning of the inference is written to stderr.
- The transpile. Each `.ts`, `.tsx`, `.mts` and `.cts` file of `src/` and the configuration
  `aosabi.config.ts`, `.mts`, `.js` or `.mjs` take their file under `.aosabi/` at the path they hold
  in the app, transpiled by `ts.transpileModule` to an ES module at the target of the app, or at
  ES2022 where it names a later one, with the options of the app's `tsconfig.json`, its decorators
  among them, the helpers inside the file and the source map inline. Every import stays an import,
  and a relative specifier takes the extension a module of the platform resolves, `./x.js` or
  `./x/index.js`, while a JSON file stays an import of its own file. A file whose syntax the parser
  of `typescript` refuses fails the step with each error and its place before a file is written.
- The configuration. The step loads the configuration it transpiled, `.aosabi/aosabi.config.js`,
  on Node: its default export, else its export `config`, called where it is a function. A load that
  throws fails the step with the message `Failed to load aosabi.config` and the cause after it,
  before a file of the glue is written. The step loads it after the transpile, since plain Node
  imports no TypeScript, and calls no hook `build` of a plugin, which only `usePages` holds.
- The glue. A schema that forms no valid GraphQL schema fails the step with the message
  `Aosabi generated an invalid GraphQL schema:` and the errors, before a file of the glue is
  written, while the transpiled sources stand. Otherwise the step writes `schema.graphql`,
  `schema.mjs`, `resolvers.js`, `server.mjs`, `worker.mjs`, `ir.json` and `ir.mjs`, each only where
  its bytes changed. `server.mjs` imports the entry `aosabi`, boots the app with its configuration
  and two lines more, the import of `ir.mjs` and the line that exposes
  the IR of the build as `globalThis.__AOSABI_IR__`, which `aosabi/bun` and `aosabi/worker` hand the
  engine. `worker.mjs` is the entry of a Cloudflare Worker: it imports `aosabi/worker` first, so the
  app takes `core.wasm` before the glue runs, then the glue, and exports the fetch handler of
  `aosabi/worker`.

The declarations of the package `aosabi` keep the bare specifiers of `hono`,
`graphql` and `graphql-yoga`, which an app installs none of, so the checker reads those types as
unresolved where a type of the app names them, and the class `Repeater` of the root comes from the
declaration of `@repeaterjs/repeater` the package carries, the test the inference makes of a
stream.

The exit code is 0 when the step built the app, 1 when it failed, with the message on stderr, and 2
for a call it does not know.

## The name `aosabi`

The Rust command line of the Aosabi repository, the crate `aosabi-cli`, builds a binary of the same
name. An app reaches this command through its own `node_modules/.bin`, so `npx aosabi` and the
scripts of `package.json` run it, and a shell runs the Rust binary where its path names it.
