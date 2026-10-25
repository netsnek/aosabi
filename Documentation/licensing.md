# Licensing

Aosabi is licensed under the MIT License, whose text is in [LICENSE](../LICENSE). This page explains
how every file of the repository states its license, where the license texts live, and which files
carry work of other projects.

## 1. The SPDX line

Every source file states its license on its first line with an SPDX identifier, after the
placement rules of the Linux kernel. A script that starts with a `#!` line carries it on its second
line. The comment style follows the language of the file:

```
// SPDX-License-Identifier: MIT        Rust, C, TypeScript, JavaScript and Typst
# SPDX-License-Identifier: MIT         Python, shell, TOML, YAML and Cap'n Proto
```

A file that carries no SPDX line, such as a document, a test fixture or a data file, is licensed
under the MIT License, unless section 4 names it with another license or it belongs to a fork of
section 6. A manifest states its license in its own field, `license` in a `Cargo.toml` or a
`package.json`.

## 2. The license texts

The folder [LICENSES](../LICENSES) holds the text of every license a file of the repository uses,
in the format of the Linux kernel. Each text opens with a short header that names its SPDX
identifier and says when to use it. [LICENSES/preferred/MIT](../LICENSES/preferred/MIT) is the
license of Aosabi's own code, and a new file always takes it. The texts under
[LICENSES/other](../LICENSES/other) are the licenses of work of other projects that Aosabi carries
or derives from, and no new file of Aosabi takes them.

## 3. Pylon

Pylon v3 apps run unchanged on Aosabi through its compatibility layer, `aosabi/pylon`. Pylon, by
Nico Schett, is licensed under the Apache License 2.0. A file of Aosabi that copies or translates
code that was added to Pylon on or after 2024-05-29, the day Pylon took that license, is licensed
under it as well, in the compatibility layer or elsewhere. Its SPDX line names `MIT AND Apache-2.0`
where Aosabi's own code is mixed in and `Apache-2.0` where the file is Pylon's code alone, and its
header credits Nico Schett and Pylon and says that the Aosabi authors changed it, as section 4 of
that license asks:

```
// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * auth.js: the entry aosabi/auth of the package aosabi.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/auth/index.ts,
 * Apache-2.0. Changed by the Aosabi authors.
 */
```

[NOTICE](../NOTICE) credits them as well and reproduces the NOTICE file of Pylon. Every other file
of Aosabi is licensed under the MIT License, but for the files that section 4 names.

## 4. Files that keep the bytes of another project

Some files carry the text of another project byte for byte, because Aosabi answers exactly as the
reference does, or ships the declaration files of the packages an app imports. An SPDX line would
change their bytes, so they carry none, and this table states their license. NOTICE names the
project of each.

| Files | License |
| --- | --- |
| `aosabi-ts/types/@repeaterjs/` | MIT |
| `aosabi-ts/types/graphql/` | MIT |
| `aosabi-ts/types/pylon/` | Apache-2.0 |
| `crates/aosabi-build/src/schema_invalid_message.rs` | Apache-2.0 |
| `crates/aosabi-core/src/descriptor/library/` | Apache-2.0 |
| `crates/aosabi-core/src/in_context.graphql` | Apache-2.0 |
| `crates/aosabi-core/src/number_scalar_messages.rs` | Apache-2.0 |
| `crates/aosabi-query/src/index_js.segments` | Apache-2.0 |
| `crates/aosabi-query/src/index_ts.tmpl` | Apache-2.0 |
| `crates/aosabi-serve/src/config_abort_message.rs` | Apache-2.0 |
| `crates/graphql-over-http/src/graphiql/` | MIT |
| `plugins/aosabi-landing/src/landing.html.tmpl` | Apache-2.0 |

## 5. Code derived from other projects

Parts of Aosabi are written after the code of other projects, mostly as translations into Rust.
Such a file says so in a short attribution comment in its header, in the form `Derived from
<project> <version>, <file>, <license>`, and carries the SPDX line of Aosabi, `MIT`, unless
section 3 gives it Pylon's license. [NOTICE](../NOTICE) names every project Aosabi derives code
from, with its version, its license and the copyright lines of its license file, as those licenses
ask. Where a project is licensed under the BSD 2-Clause, the BSD 3-Clause, the Boost Software
License 1.0 or the Unicode License V3, which ask for their notice alone as MIT does, NOTICE keeps
that notice and LICENSES/other keeps the text.

The test data that the tests copy whole from other projects keeps its own license, never reaches a
crate, a build or a package, and is named in NOTICE as well.

## 6. Forks

The crates under [third_party](../third_party) are forks of other projects, kept with the files of
their published archives. Each keeps its own license file, and each of its source files names that
license in its SPDX line.

| Fork | License |
| --- | --- |
| `third_party/rust/h2/` | MIT |
| `third_party/rust/http/` | MIT OR Apache-2.0 |
| `third_party/rust/http-body/` | MIT |
| `third_party/rust/hyper/` | MIT |

## 7. Code that Aosabi writes into a project

A file that a command of Aosabi writes into a user's project, such as the scaffold of `aosabi new`,
carries no SPDX line, because the license of this repository is not the license of the user's code.

## 8. Contributing

A contribution is licensed under the license of the file it changes, and a new file takes the MIT
License with its SPDX line. A change to a file under Pylon's license keeps its SPDX line and its
credit of Pylon.
