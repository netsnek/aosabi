# SPDX-License-Identifier: MIT
"""tests/gen: family I, the introspection documents a client sends.

The family sends the full introspection document, the narrow form that asks for
the root type names alone, `__type` on every named type the schema declares and
`__typename` at the root. The full document is written here and is no copy of
the text `getIntrospectionQuery` builds, because the comparison is the two
executors over one document and not the spelling of one client.

The nesting of `fields`, `interfaces`, `possibleTypes` and `inputFields` stays
at one level, which is what the specified depth rule of the pinned library
allows.
"""

from __future__ import annotations

from .cases import Case, make_case
from .schema import Schema
from .sdl import print_string

FULL_DOCUMENT = """query Introspection {
  __schema {
    description
    queryType {
      name
    }
    mutationType {
      name
    }
    subscriptionType {
      name
    }
    types {
      ...FullType
    }
    directives {
      name
      description
      locations
      isRepeatable
      args {
        ...InputValue
      }
    }
  }
}

fragment FullType on __Type {
  kind
  name
  description
  specifiedByURL
  fields(includeDeprecated: true) {
    name
    description
    args {
      ...InputValue
    }
    type {
      ...TypeRef
    }
    isDeprecated
    deprecationReason
  }
  inputFields {
    ...InputValue
  }
  interfaces {
    ...TypeRef
  }
  enumValues(includeDeprecated: true) {
    name
    description
    isDeprecated
    deprecationReason
  }
  possibleTypes {
    ...TypeRef
  }
}

fragment InputValue on __InputValue {
  name
  description
  type {
    ...TypeRef
  }
  defaultValue
}

fragment TypeRef on __Type {
  kind
  name
  ofType {
    kind
    name
    ofType {
      kind
      name
      ofType {
        kind
        name
      }
    }
  }
}
"""

ROOT_NAMES_DOCUMENT = """{
  __schema {
    queryType {
      name
    }
    mutationType {
      name
    }
    subscriptionType {
      name
    }
  }
}
"""

TYPENAME_DOCUMENT = """{
  __typename
}
"""


def type_document(name: str) -> str:
    """Returns the `__type` document for one named type."""
    return (
        "{\n"
        f"  __type(name: {print_string(name)}) {{\n"
        "    kind\n"
        "    name\n"
        "    description\n"
        "    specifiedByURL\n"
        "    fields(includeDeprecated: true) {\n"
        "      name\n"
        "      isDeprecated\n"
        "      deprecationReason\n"
        "    }\n"
        "    inputFields {\n"
        "      name\n"
        "      defaultValue\n"
        "    }\n"
        "    interfaces {\n"
        "      name\n"
        "    }\n"
        "    enumValues(includeDeprecated: true) {\n"
        "      name\n"
        "      isDeprecated\n"
        "    }\n"
        "    possibleTypes {\n"
        "      name\n"
        "    }\n"
        "  }\n"
        "}\n"
    )


def introspection_cases(schema: Schema, rng, table: str) -> list[Case]:
    """Returns family I over the types the schema declares."""
    cases = [
        make_case("I", table, FULL_DOCUMENT, operation_name="Introspection"),
        make_case("I", table, ROOT_NAMES_DOCUMENT),
        make_case("I", table, TYPENAME_DOCUMENT),
    ]
    for name in schema.document.types:
        cases.append(make_case("I", table, type_document(name)))
    return cases
