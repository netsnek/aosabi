# Conformance with graphql-js

Aosabi answers a GraphQL request with the bytes graphql-js 16.12.0 gives for it: the same parse
errors, the same validation messages with their locations and their suggestions, the same order of
results and errors, and the same JSON. This page says how the test suite holds the engine to that,
and how to run it.

## 1. Tests are notebooks

Every test of Aosabi is a Jupyter notebook in Python under
[tests/notebooks/](../../tests/notebooks/), run by papermill. The workspace holds no `#[test]` and
no doc test that asserts behaviour, so every assertion stands in one language and one place, and a
failing run explains itself in the cells of its notebook.

The crate [aosabi-devtool](../../crates/aosabi-devtool/) reaches the internals of the engine from
Python, one byte level primitive per subcommand: `json`, `parse`, `print`, `validate`, `pass`,
`build-schema`, `validate-schema`, `print-schema`, `execute` and more. A notebook checks one
primitive at a time, so a byte that differs points at one crate rather than at a whole server.

## 2. The oracle

The other side of each comparison is graphql-js 16.12.0 itself, run on Node.js 24.13.0 by the
sidecar `tests/sidecar/graphql-js.mjs`. The script `tests/fetch_oracles.py` clones graphql-js at the
commit that `tests/oracles.toml` pins, and the notebooks read its own test documents from there. The
sidecar reads one JSON request per line on its standard input and writes one JSON answer per line,
so a whole corpus travels through one process. It binds no port and reaches no network, and before
its first answer it reads the version of the library it loaded and stops with an error when the
version is not the pinned one, since an oracle at another version would define correctness by
accident.

Node.js runs in the test suite beside the engine, for the oracles and for the JavaScript clients the
suite drives, and in the dev package of the TypeScript layer at build time. No crate links a
JavaScript runtime, and no build of a Rust service needs Node.js.

## 3. What is compared

| Suite | What it holds to the oracle |
| --- | --- |
| The JSON writer | The JSON writer, over seeded doubles, strings and objects |
| The language | The lexer, the parser, the printer, the 27 rules and the schema builder |
| The single pass | The single pass, which may accept only what graphql-js accepts |
| The executor | The executor, its results and the order of its errors under async resolvers |

These four suites read recorded answers and parts of the test harness that this repository does not
carry, so they run in the project's own pipeline, while [tests/notebooks/](../../tests/notebooks/)
holds the public suites of Homa, of its framing and of the carriers. The suite of the JSON writer
holds the writer to `JSON.stringify` of Node.js 24.13.0, which a JavaScript GraphQL server writes
its answers with. The suite of the language reads the test documents of graphql-js's own lexer and
parser and holds every token, every node and every location to the library, generates invalid
documents for every validation rule so that each rule is broken and answered by its message, and
prints and builds schemas through both sides. The suite of the single pass holds the single pass to
graphql-js and to the 27 rules three ways, over generated documents of many schemas, and runs a
mutation round that weakens one check of the walk at a time to show that the comparison would catch
it. On 2026-10-20 it found 0 false acceptances and 0 false refusals over 604,800 generated documents
of 28 schemas.

## 4. How a comparison is made

- Answers are compared as bytes and never parsed for the verdict. The parsed view is printed for the
  reader alone.
- Each corpus is generated from a recorded seed, and every case carries a stable id, the SHA-256 of
  its request, so a failing case runs again by its id.
- A generated corpus of operations is bounded on purpose: depth 3 by default, at most 6 fields per
  level, arguments varied one at a time and then in pairs, and one invalid document per validation
  rule and per error kind of the lexer and the parser, named close enough to a valid name that the
  `Did you mean` suggestions fire.
- A failure prints the first byte where the two sides differ, as a hexdump of both.

## 5. Running the suite

The Python side is pinned in [tests/pyproject.toml](../../tests/pyproject.toml): papermill,
nbclient, nbformat, ipykernel, jupyter_client, PyYAML and requests. The kernel the notebooks run on
is registered as `aosabi`. The script `tests/fetch_oracles.py` clones the oracles and installs
nothing, and the packages the JavaScript side of a suite needs install from the lockfile of its
oracle with pnpm 10.34.5 and `pnpm install --frozen-lockfile`, on the Node.js 24.13.0 that
`.node-version` pins. The oracles come first, then the runner sweeps the matrix of entries and
writes each executed notebook and a report under `tests/notebooks/out/`, which is never committed:

```sh
python3 tests/fetch_oracles.py
python3 tests/run.py --matrix tests/matrix.yaml
python3 tests/run.py --matrix tests/matrix.yaml --jobs 4
```

The last line runs up to four entries at once, each in a kernel of its own and on ports of its own.
An entry that times something or holds a shared resource runs alone after the others. The exit code
is non zero when any notebook failed.

A notebook that judges speed prints its times beside its verdict and does not assert them on a
shared machine. Its numbers are those of the machine it ran on, and the page that quotes one names
the suite and the day.
