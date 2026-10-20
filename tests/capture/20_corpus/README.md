# The corpus of the executor notebook

This directory holds the operations and the behaviour tables that the executor
notebook of the project sends to the node sidecar and to
`aosabi-devtool execute`. One file carries one schema in one mode, and
`manifest.json` carries the manifest of each of them beside the call that wrote
them all.

The files are written by the script beside them, from the repository root:

```
tests/.venv/bin/python tests/capture/20_corpus/write_corpus.py
```

The generator in `tests/gen/` is seeded and deterministic, so two calls of that line write the same
bytes. The notebook regenerates every corpus before it judges anything and compares it with the
committed file byte for byte, so a stale corpus and a defect of the executor stay apart, and a
corpus that shrinks is visible rather than silent.

## What one file holds

| Key | What it carries |
| --- | --- |
| `manifest` | the version, the seed, the parameters, the digest of the schema text, the reachable field count, whether the enumeration was exhaustive, the table names, the count per family and the total |
| `tables` | every behaviour table the run registers, in the order it registers them |
| `cases` | every request, in the order the run sends them, each with its id, its family, its table, its document, its variables, its operation name and its validate flag |

A case id is the SHA-256 of the canonical form of the request, so the same request carries the same
name in every run and a changed document is a changed id.

## The ten corpora

| Schema | Where it comes from | Cases in `plain` | Cases in `pylon` |
| --- | --- | --- | --- |
| `rich` | `tests/gen/schemas.py`, one of every construct | 1212 | 1224 |
| `blog-app` |  | 604 | 604 |
| `errors-app` | the built fixture under `tests/fixtures/ts/errors-app` | 155 | 155 |
| `async-order-app` | the built fixture under `tests/fixtures/ts/async-order-app` | 158 | 158 |
| `numbers-app` | the built fixture under `tests/fixtures/ts/numbers-app` | 350 | 350 |

That is 4970 cases over 268 behaviour tables. The budget `max_ops` is 5000 per corpus and no corpus
reaches it, so every enumeration is exhaustive and none of the ten is a sample, which the `sampled`
field of each manifest records.

The three fixture schemas are read from the built fixture rather than copied, so
a fixture that is not built yet makes the script fail rather than write a corpus
of a schema that drifted from the fixture.
