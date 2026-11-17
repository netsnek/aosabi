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
sidecar `tests/sidecar/graphql-js.mjs`, which imports the library from the package beside it that
`tests/sidecar/package.json` and its lock `tests/sidecar/package-lock.json` pin. The script
`tests/fetch_oracles.py` clones graphql-js at the commit that `tests/oracles.toml` pins, and the
notebooks read its own test documents from there. The sidecar reads one JSON request per line on
its standard input and writes one JSON answer per line, so a whole corpus travels through one
process. It binds no port and reaches no network, and before its first answer it reads the version
of the library it loaded and stops with an error when the version is not the pinned one, since an
oracle at another version would define correctness by accident.

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

The public suites run on Linux with Python 3.12 or later and the Rust toolchain that
`rust-toolchain.toml` pins, 1.95.0, with the component `llvm-tools`, whose `llvm-size` and
`llvm-nm` read the sizes of the builds for the ESP32-C3. The Python side is pinned in
[tests/pyproject.toml](../../tests/pyproject.toml): papermill 2.7.0, nbclient 0.11.0, nbformat
5.11.1, ipykernel 7.3.0, jupyter_client 8.10.0, PyYAML 6.0.3 and requests 2.34.2. The file names no
build backend, so its dependencies install as a list into a virtual environment, which registers the
kernel `aosabi` the notebooks run on:

```sh
rustup toolchain install 1.95.0 --profile minimal --component llvm-tools \
  --target wasm32-unknown-unknown --target riscv32imc-unknown-none-elf
python3 -m venv tests/.venv
tests/.venv/bin/python -c 'import tomllib; print("\n".join(tomllib.load(open("tests/pyproject.toml", "rb"))["project"]["dependencies"]))' > tests/.venv/requirements.txt
tests/.venv/bin/python -m pip install --requirement tests/.venv/requirements.txt
tests/.venv/bin/python -m ipykernel install --sys-prefix --name aosabi
```

The component installs `llvm-size` and `llvm-nm` in the folder of the toolchain's own tools, which
is not on `PATH`, and the notebooks look them up on `PATH`. The runner keeps the locks of its job
slots under `XDG_RUNTIME_DIR`, and under `/tmp` where the variable is not set, as in a shell over
SSH. Continuous integration sets both before it runs the matrix, and a shell does the same with:

```sh
export PATH="$(rustc --print sysroot)/lib/rustlib/$(rustc -vV | sed -n 's/^host: //p')/bin:$PATH"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-$(mktemp -d)}"
```

The suite `80_embed` asks the sidecar of section 2 for the error of a document past the token cap,
so the package of the sidecar installs before a run, with the npm of the Node.js 24.13.0 that
`.node-version` pins. `npm ci` takes graphql 16.12.0 from the lock beside the sidecar, refuses a
package whose integrity is not the one the lock records and runs no script of it:

```sh
npm --prefix tests/sidecar ci --ignore-scripts --no-audit --no-fund
```

The runner sweeps the matrix of entries and writes each executed notebook and a report under
`tests/notebooks/out/`, which is never committed:

```sh
tests/.venv/bin/python tests/run.py --matrix tests/matrix.yaml
tests/.venv/bin/python tests/run.py --matrix tests/matrix.yaml --jobs 4
```

The last line runs up to four entries at once, each in a kernel of its own and on ports of its own.
An entry that times something or holds a shared resource runs alone after the others. The exit code
is non zero when any notebook failed.

Continuous integration installs the package of the sidecar and runs ten entries that need no
network, no privileges and no device and end in seconds: `03_homa_packets`, `04_aoframe`,
`05_homa_wire_full-synthetic`, `06_lora_frame`, `80_embed`, `22_homa_lossy-engine`,
`23_homa_budget-client`, `23_homa_budget-graphtp`, `23_homa_budget-lora` and
`29_carrier_lora-model`. The same subset runs locally with
`python3 -I .github/scripts/fast_tests.py --python tests/.venv/bin/python`.

`25_carrier_unix` binds its sockets under `tests/notebooks/out/` where their paths fit the 108 bytes
of the address of a Unix socket, and otherwise in a folder of its own with the mode 0700 under
`XDG_RUNTIME_DIR`, or under `/tmp` where the variable is not set, so it passes in a checkout at any
path. One entry needs more than the others: `28_carrier_bluetooth-emulated` boots two guests under
KVM, so it runs on an aarch64 Linux host with `/dev/kvm` open for reading and writing,
`qemu-system-aarch64`, `qemu-img` and the EDK2 firmware for aarch64, and it fetches the 829 MiB of
guest images that `tests/guests/guests.lock` pins.

The suites of section 3 read graphql-js from a clone at the commit that `tests/oracles.toml` pins,
which `python3 tests/fetch_oracles.py` makes. No public suite needs it.

A notebook that judges speed prints its times beside its verdict and does not assert them on a
shared machine. Its numbers are those of the machine it ran on, and the page that quotes one names
the suite and the day.
