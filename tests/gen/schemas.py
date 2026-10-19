# SPDX-License-Identifier: MIT
"""tests/gen: the schemas the generated runs are written against.

The rich schema below carries one of every construct the executor reaches, and
its root fields carry the names [`NAMED_BEHAVIOURS`] of `tables.py` turns into
one of every behaviour kind, so one corpus over it touches every path of the
port.

A schema of the compatibility layer declares seven scalars that no specification builds in, and the
seven bound scalars of `aosabi-core` need them to exist before a resolver map
binds them. [`ensure_branded_scalars`] adds the ones a schema text does not
declare, so a generated schema runs in `pylon` mode as a built fixture does.
"""

from __future__ import annotations

from pathlib import Path

#: The twelve branded scalars a schema of the compatibility layer declares, the five of the specification
#: first, in the order the built schema writes them. The five are declared too,
#: because the compatibility layer binds `ID` and the reference builder refuses a binding the
#: schema text does not declare.
BRANDED_SCALARS = (
    "String",
    "ID",
    "Int",
    "Float",
    "Boolean",
    "Number",
    "Any",
    "Void",
    "JSONObject",
    "File",
    "Date",
    "JSON",
)

#: The directive a schema of the compatibility layer declares on an operation. The descriptions of
#: the built schema are left out, because a description changes no execution
#: and the text of the reference belongs to the reference.
IN_CONTEXT_DIRECTIVE = (
    "directive @inContext(\n"
    "  locale: String\n"
    "  context: String\n"
    ") on QUERY | MUTATION | SUBSCRIPTION\n"
)

#: The schemas of the repository a run names beside the rich one.
SCHEMA_PATHS = {
    "blog-app": "tests/capture/02_corpus/blog-app-schema.graphql",
    "errors-app": "tests/fixtures/ts/errors-app/.aosabi/schema.graphql",
    "async-order-app": "tests/fixtures/ts/async-order-app/.aosabi/schema.graphql",
    "numbers-app": "tests/fixtures/ts/numbers-app/.aosabi/schema.graphql",
}

RICH_SDL = """schema {
  query: Query
  mutation: Mutation
}

scalar Number

scalar Date

scalar JSON

scalar JSONObject

scalar Void

scalar Any

scalar File

enum Colour {
  RED
  GREEN
  BLUE
}

interface Node {
  id: ID!
}

interface Named {
  name: String!
}

type Leafs implements Node & Named {
  id: ID!
  name: String!
  text: String
  count: Int
  ratio: Float
  flag: Boolean
  amount: Number
  when: Date
  payload: JSON
  bag: JSONObject
  nothing: Void
  anything: Any
  colour: Colour
  texts: [String!]
  matrix: [[Int!]!]
}

type Branch implements Node {
  id: ID!
  label: String!
  leafs: Leafs!
  children: [Branch!]!
  maybeChild: Branch
  peers: [Node!]
}

union Anything = Leafs | Branch

input Inner {
  flag: Boolean!
}

input Filter {
  required: String!
  optional: Int
  limit: Int = 10
  tags: [String!]
  colour: Colour
  nested: Inner
}

type Query {
  leafs: Leafs!
  branch: Branch
  node: Node
  nodes: [Node!]!
  anything: Anything
  search(filter: Filter, tag: String, count: Int, colour: Colour, ids: [ID!], when: Date): [Leafs!]
  strictSearch(filter: Filter!, tag: String!, count: Int!, colour: Colour!, ids: [ID!]!): [Leafs!]!
  valueLeaf: String
  undefinedLeaf: String
  nanLeaf: Number
  infinityLeaf: Number
  fnLeaf: String
  argLeaf(text: String!): String
  resolvedLeaf: String
  rejectedLeaf: String
  afterLeaf: String
  afterErrorLeaf: String
  throwLeaf: String
  serviceLeaf: String
  graphqlLeaf: String
  aggregateLeaf: String
  errorValueLeaf: String
}

type Mutation {
  touch(input: Filter!): Leafs!
  rename(id: ID!, name: String!): Branch
}
"""


def repository_root(start: Path | None = None) -> Path:
    """Returns the root of this repository, walked up from this file.

    The root is the directory that carries the workspace manifest and the
    tests directory, which is the same rule `tests/harness/paths.py` follows.
    """
    here = (start or Path(__file__)).resolve()
    for base in [here, *here.parents]:
        if (base / "Cargo.toml").is_file() and (base / "tests").is_dir():
            return base
    raise FileNotFoundError("no repository root above this file")


def named_sdl(name: str, root: Path | None = None) -> str:
    """Returns the schema text of one name the generator knows.

    The name `rich` answers with [`RICH_SDL`], and every other name is read
    from the path [`SCHEMA_PATHS`] gives it.
    """
    if name == "rich":
        return RICH_SDL
    relative = SCHEMA_PATHS.get(name)
    if relative is None:
        raise KeyError(f"no schema named {name}")
    return (repository_root() if root is None else root).joinpath(relative).read_text()


def schema_names() -> list[str]:
    """Returns the names a run can ask a schema for."""
    return ["rich", *SCHEMA_PATHS]


def ensure_branded_scalars(text: str) -> str:
    """Returns the schema text with the scalars and the directive the compatibility layer declares.

    A scalar the text already declares is left alone, and so is the directive,
    so a built fixture schema passes through unchanged.
    """
    additions = [f"scalar {name}\n" for name in BRANDED_SCALARS if not _declares_scalar(text, name)]
    if "directive @inContext" not in text:
        additions.append(IN_CONTEXT_DIRECTIVE)
    if not additions:
        return text
    return text.rstrip("\n") + "\n\n" + "\n".join(additions)


def _declares_scalar(text: str, name: str) -> bool:
    """Says whether a schema text declares the scalar `name`."""
    for line in text.splitlines():
        if line.strip() == f"scalar {name}":
            return True
    return False
