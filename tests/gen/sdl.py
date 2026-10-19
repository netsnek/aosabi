# SPDX-License-Identifier: MIT
"""tests/gen: the schema definition reader and the value printer.

The generator has to know the shape of a schema to enumerate operations over
it, and it has to print GraphQL literals back into a document. This module is
both halves, a recursive descent reader over the SDL grammar and a printer for
the literal value model the argument and variable families build.

The reader is never an oracle. It types the schema and nothing else, and every
verdict of a run comes from the two executors the notebook drives. It reads the
definition forms the fixture schemas, the blog schema and the generated schemas
carry, which are the schema definition, scalar, enum, union, interface, object,
input object and directive definitions with descriptions, arguments, default
values and directives. A type extension is refused with a message that names
the position, because no schema of this harness carries one.
"""

from __future__ import annotations

from dataclasses import dataclass, field

PUNCTUATORS = "!$&()@[]{|}:="


class SdlError(Exception):
    """A schema text the reader cannot take, with the position it stops at."""


@dataclass(frozen=True)
class Token:
    """One lexical token, its kind and the text it carries."""

    kind: str
    text: str
    position: int


@dataclass(frozen=True)
class EnumValue:
    """An enum literal, kept apart from a string so the printer spells it bare."""

    name: str


@dataclass(frozen=True)
class Variable:
    """A variable reference inside a literal, printed as `$name`."""

    name: str


@dataclass(frozen=True)
class TypeRef:
    """A type reference, either a named type or one wrapper around another."""

    kind: str
    name: str | None = None
    of_type: "TypeRef | None" = None

    @property
    def is_non_null(self) -> bool:
        """Says whether the outermost wrapper is a non null wrapper."""
        return self.kind == "non_null"

    @property
    def is_list(self) -> bool:
        """Says whether the reference is a list, under a non null wrapper or not."""
        return self.nullable().kind == "list"

    def nullable(self) -> "TypeRef":
        """Returns the reference without its outermost non null wrapper."""
        return self.of_type if self.kind == "non_null" else self

    def item(self) -> "TypeRef":
        """Returns the item reference of a list, which the caller proves is one."""
        inner = self.nullable()
        if inner.kind != "list":
            raise SdlError(f"{inner.text()} is no list")
        return inner.of_type

    def named_name(self) -> str:
        """Returns the name of the named type at the bottom of the wrappers."""
        here = self
        while here.kind != "named":
            here = here.of_type
        return here.name

    def text(self) -> str:
        """Prints the reference the way the SDL spells it."""
        if self.kind == "named":
            return self.name
        if self.kind == "list":
            return f"[{self.of_type.text()}]"
        return f"{self.of_type.text()}!"


def named(name: str) -> TypeRef:
    """Returns a named type reference."""
    return TypeRef(kind="named", name=name)


@dataclass(frozen=True)
class InputValue:
    """One argument of a field or of a directive, or one field of an input object."""

    name: str
    type: TypeRef
    default: object = None
    has_default: bool = False


@dataclass(frozen=True)
class Field:
    """One output field, its type and its arguments in declaration order."""

    name: str
    type: TypeRef
    args: tuple[InputValue, ...] = ()


@dataclass
class TypeDef:
    """One named type of the schema and the members its kind carries."""

    name: str
    kind: str
    fields: dict[str, Field] = field(default_factory=dict)
    input_fields: dict[str, InputValue] = field(default_factory=dict)
    interfaces: tuple[str, ...] = ()
    members: tuple[str, ...] = ()
    enum_values: tuple[str, ...] = ()


@dataclass(frozen=True)
class DirectiveDef:
    """One directive definition, its arguments and the locations it names."""

    name: str
    args: tuple[InputValue, ...]
    locations: tuple[str, ...]
    repeatable: bool


@dataclass
class SchemaDocument:
    """Every definition of one schema text, in the order the text carries them."""

    types: dict[str, TypeDef] = field(default_factory=dict)
    directives: dict[str, DirectiveDef] = field(default_factory=dict)
    roots: dict[str, str] = field(default_factory=dict)


def tokenize(text: str) -> list[Token]:
    """Returns the tokens of a schema text, with comments and commas dropped.

    Ignored tokens are the unicode byte order mark, white space, a comma and a
    comment, which is what the GraphQL grammar calls ignored.
    """
    tokens: list[Token] = []
    position = 0
    size = len(text)
    while position < size:
        char = text[position]
        if char in " \t\r\n﻿,":
            position += 1
            continue
        if char == "#":
            while position < size and text[position] not in "\r\n":
                position += 1
            continue
        if text.startswith('"""', position):
            end = text.find('"""', position + 3)
            if end < 0:
                raise SdlError(f"unterminated block string at {position}")
            tokens.append(Token("string", text[position + 3 : end], position))
            position = end + 3
            continue
        if char == '"':
            start = position
            value, position = _read_string(text, position)
            tokens.append(Token("string", value, start))
            continue
        if text.startswith("...", position):
            tokens.append(Token("spread", "...", position))
            position += 3
            continue
        if char in PUNCTUATORS:
            tokens.append(Token("punct", char, position))
            position += 1
            continue
        if char == "-" or char.isdigit():
            start = position
            value, kind, position = _read_number(text, position)
            tokens.append(Token(kind, value, start))
            continue
        if char == "_" or char.isalpha():
            start = position
            while position < size and (text[position] == "_" or text[position].isalnum()):
                position += 1
            tokens.append(Token("name", text[start:position], start))
            continue
        raise SdlError(f"unexpected character {char!r} at {position}")
    return tokens


def _read_string(text: str, position: int) -> tuple[str, int]:
    """Reads one single quoted string and returns its value and the next position."""
    out: list[str] = []
    position += 1
    while position < len(text):
        char = text[position]
        if char == '"':
            return "".join(out), position + 1
        if char == "\\":
            escape = text[position + 1]
            simple = {"n": "\n", "r": "\r", "t": "\t", "b": "\b", "f": "\f"}
            if escape == "u":
                out.append(chr(int(text[position + 2 : position + 6], 16)))
                position += 6
                continue
            out.append(simple.get(escape, escape))
            position += 2
            continue
        out.append(char)
        position += 1
    raise SdlError(f"unterminated string at {position}")


def _read_number(text: str, position: int) -> tuple[str, str, int]:
    """Reads one numeric token and returns its text, its kind and the next position."""
    start = position
    kind = "int"
    size = len(text)
    if text[position] == "-":
        position += 1
    while position < size and text[position].isdigit():
        position += 1
    if position < size and text[position] == ".":
        kind = "float"
        position += 1
        while position < size and text[position].isdigit():
            position += 1
    if position < size and text[position] in "eE":
        kind = "float"
        position += 1
        if position < size and text[position] in "+-":
            position += 1
        while position < size and text[position].isdigit():
            position += 1
    return text[start:position], kind, position


class Reader:
    """A cursor over the tokens of one schema text."""

    def __init__(self, text: str) -> None:
        self.tokens = tokenize(text)
        self.position = 0

    def peek(self, offset: int = 0) -> Token | None:
        """Returns the token `offset` places ahead, or `None` at the end."""
        index = self.position + offset
        return self.tokens[index] if index < len(self.tokens) else None

    def next(self) -> Token:
        """Returns the next token and steps over it."""
        token = self.peek()
        if token is None:
            raise SdlError("the schema text ends early")
        self.position += 1
        return token

    def at(self, kind: str, text: str | None = None) -> bool:
        """Says whether the next token has that kind and, when given, that text."""
        token = self.peek()
        if token is None or token.kind != kind:
            return False
        return text is None or token.text == text

    def expect(self, kind: str, text: str | None = None) -> Token:
        """Steps over the next token and refuses anything but the one asked for."""
        token = self.next()
        if token.kind != kind or (text is not None and token.text != text):
            wanted = text or kind
            raise SdlError(f"expected {wanted} at {token.position}, found {token.text!r}")
        return token

    def skip(self, kind: str, text: str) -> bool:
        """Steps over the next token when it matches and says whether it did."""
        if self.at(kind, text):
            self.position += 1
            return True
        return False


def parse_schema(text: str) -> SchemaDocument:
    """Reads a schema text and returns its definitions.

    The result carries the named types in the order the text declares them, the
    directive definitions, and the root operation types, which default to the
    types named `Query`, `Mutation` and `Subscription` when the text carries no
    schema definition.
    """
    reader = Reader(text)
    document = SchemaDocument()
    while reader.peek() is not None:
        _parse_definition(reader, document)
    for operation, type_name in (
        ("query", "Query"),
        ("mutation", "Mutation"),
        ("subscription", "Subscription"),
    ):
        if operation not in document.roots and type_name in document.types:
            document.roots[operation] = type_name
    return document


def _parse_definition(reader: Reader, document: SchemaDocument) -> None:
    """Reads one top level definition into `document`."""
    if reader.at("string"):
        reader.next()
    keyword = reader.expect("name").text
    if keyword == "schema":
        _parse_directives(reader)
        reader.expect("punct", "{")
        while not reader.skip("punct", "}"):
            operation = reader.expect("name").text
            reader.expect("punct", ":")
            document.roots[operation] = reader.expect("name").text
        return
    if keyword == "directive":
        _parse_directive_definition(reader, document)
        return
    if keyword == "extend":
        token = reader.peek()
        raise SdlError(f"a type extension at {token.position} is out of scope")
    _parse_type_definition(reader, document, keyword)


def _parse_directive_definition(reader: Reader, document: SchemaDocument) -> None:
    """Reads a directive definition and stores it under its name."""
    reader.expect("punct", "@")
    name = reader.expect("name").text
    args = _parse_argument_definitions(reader)
    repeatable = reader.at("name", "repeatable")
    if repeatable:
        reader.next()
    reader.expect("name", "on")
    reader.skip("punct", "|")
    locations = [reader.expect("name").text]
    while reader.skip("punct", "|"):
        locations.append(reader.expect("name").text)
    document.directives[name] = DirectiveDef(
        name=name, args=args, locations=tuple(locations), repeatable=repeatable
    )


def _parse_type_definition(reader: Reader, document: SchemaDocument, keyword: str) -> None:
    """Reads one named type definition of the kind `keyword` names."""
    name = reader.expect("name").text
    definition = TypeDef(name=name, kind=_KINDS[keyword])
    if keyword == "type" or keyword == "interface":
        definition.interfaces = _parse_implements(reader)
    _parse_directives(reader)
    if keyword == "union":
        definition.members = _parse_union_members(reader)
    elif keyword == "enum":
        definition.enum_values = _parse_enum_values(reader)
    elif keyword == "input":
        definition.input_fields = _parse_input_fields(reader)
    elif keyword in ("type", "interface"):
        definition.fields = _parse_fields(reader)
    elif keyword != "scalar":
        raise SdlError(f"unknown definition keyword {keyword}")
    document.types[name] = definition


_KINDS = {
    "scalar": "scalar",
    "type": "object",
    "interface": "interface",
    "union": "union",
    "enum": "enum",
    "input": "input",
}


def _parse_implements(reader: Reader) -> tuple[str, ...]:
    """Reads the interface list of an object or interface definition."""
    if not reader.at("name", "implements"):
        return ()
    reader.next()
    reader.skip("punct", "&")
    names = [reader.expect("name").text]
    while reader.skip("punct", "&"):
        names.append(reader.expect("name").text)
    return tuple(names)


def _parse_union_members(reader: Reader) -> tuple[str, ...]:
    """Reads the member list of a union definition."""
    if not reader.skip("punct", "="):
        return ()
    reader.skip("punct", "|")
    names = [reader.expect("name").text]
    while reader.skip("punct", "|"):
        names.append(reader.expect("name").text)
    return tuple(names)


def _parse_enum_values(reader: Reader) -> tuple[str, ...]:
    """Reads the value list of an enum definition."""
    reader.expect("punct", "{")
    values: list[str] = []
    while not reader.skip("punct", "}"):
        if reader.at("string"):
            reader.next()
        values.append(reader.expect("name").text)
        _parse_directives(reader)
    return tuple(values)


def _parse_fields(reader: Reader) -> dict[str, Field]:
    """Reads the field list of an object or interface definition."""
    reader.expect("punct", "{")
    fields: dict[str, Field] = {}
    while not reader.skip("punct", "}"):
        if reader.at("string"):
            reader.next()
        name = reader.expect("name").text
        args = _parse_argument_definitions(reader)
        reader.expect("punct", ":")
        type_ref = _parse_type_ref(reader)
        _parse_directives(reader)
        fields[name] = Field(name=name, type=type_ref, args=args)
    return fields


def _parse_input_fields(reader: Reader) -> dict[str, InputValue]:
    """Reads the field list of an input object definition."""
    reader.expect("punct", "{")
    fields: dict[str, InputValue] = {}
    while not reader.skip("punct", "}"):
        if reader.at("string"):
            reader.next()
        value = _parse_input_value(reader)
        fields[value.name] = value
    return fields


def _parse_argument_definitions(reader: Reader) -> tuple[InputValue, ...]:
    """Reads the argument list of a field or of a directive definition."""
    if not reader.skip("punct", "("):
        return ()
    args: list[InputValue] = []
    while not reader.skip("punct", ")"):
        if reader.at("string"):
            reader.next()
        args.append(_parse_input_value(reader))
    return tuple(args)


def _parse_input_value(reader: Reader) -> InputValue:
    """Reads one named input value with its type, default and directives."""
    name = reader.expect("name").text
    reader.expect("punct", ":")
    type_ref = _parse_type_ref(reader)
    default = None
    has_default = False
    if reader.skip("punct", "="):
        default = parse_value(reader)
        has_default = True
    _parse_directives(reader)
    return InputValue(name=name, type=type_ref, default=default, has_default=has_default)


def _parse_type_ref(reader: Reader) -> TypeRef:
    """Reads one type reference with its list and non null wrappers."""
    if reader.skip("punct", "["):
        inner = _parse_type_ref(reader)
        reader.expect("punct", "]")
        type_ref = TypeRef(kind="list", of_type=inner)
    else:
        type_ref = named(reader.expect("name").text)
    if reader.skip("punct", "!"):
        return TypeRef(kind="non_null", of_type=type_ref)
    return type_ref


def _parse_directives(reader: Reader) -> None:
    """Steps over the directives of a definition, which the generator does not read."""
    while reader.skip("punct", "@"):
        reader.expect("name")
        if reader.skip("punct", "("):
            while not reader.skip("punct", ")"):
                reader.expect("name")
                reader.expect("punct", ":")
                parse_value(reader)


def parse_value(reader: Reader) -> object:
    """Reads one literal and returns it in the value model of this module.

    A null is `None`, an enum literal is an [`EnumValue`], a variable is a
    [`Variable`], a list is a Python list and an input object is a Python
    dictionary in the order the text writes the fields.
    """
    token = reader.next()
    if token.kind == "int":
        return int(token.text)
    if token.kind == "float":
        return float(token.text)
    if token.kind == "string":
        return token.text
    if token.kind == "punct" and token.text == "$":
        return Variable(reader.expect("name").text)
    if token.kind == "punct" and token.text == "[":
        items = []
        while not reader.skip("punct", "]"):
            items.append(parse_value(reader))
        return items
    if token.kind == "punct" and token.text == "{":
        fields: dict[str, object] = {}
        while not reader.skip("punct", "}"):
            key = reader.expect("name").text
            reader.expect("punct", ":")
            fields[key] = parse_value(reader)
        return fields
    if token.kind == "name":
        if token.text == "null":
            return None
        if token.text == "true":
            return True
        if token.text == "false":
            return False
        return EnumValue(token.text)
    raise SdlError(f"unexpected token {token.text!r} at {token.position}")


def print_value(value: object) -> str:
    """Prints one value of the model as the GraphQL literal it stands for.

    A float prints through the shortest Python form that round trips, which is
    the form the generated documents carry into both sides of a comparison.
    """
    if value is None:
        return "null"
    if value is True:
        return "true"
    if value is False:
        return "false"
    if isinstance(value, EnumValue):
        return value.name
    if isinstance(value, Variable):
        return f"${value.name}"
    if isinstance(value, str):
        return print_string(value)
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, int):
        return str(value)
    if isinstance(value, float):
        return repr(value)
    if isinstance(value, (list, tuple)):
        return "[" + ", ".join(print_value(item) for item in value) + "]"
    if isinstance(value, dict):
        inner = ", ".join(f"{key}: {print_value(item)}" for key, item in value.items())
        return "{" + inner + "}"
    raise SdlError(f"no literal for {value!r}")


_STRING_ESCAPES = {
    '"': '\\"',
    "\\": "\\\\",
    "\n": "\\n",
    "\r": "\\r",
    "\t": "\\t",
    "\b": "\\b",
    "\f": "\\f",
}


def print_string(text: str) -> str:
    """Prints a string as a GraphQL string literal with the escapes of the grammar."""
    out = ['"']
    for char in text:
        escape = _STRING_ESCAPES.get(char)
        if escape is not None:
            out.append(escape)
        elif ord(char) < 0x20:
            out.append(f"\\u{ord(char):04x}")
        else:
            out.append(char)
    out.append('"')
    return "".join(out)


def json_value(value: object) -> object:
    """Turns a value of the model into what a variables object carries.

    An enum literal becomes its name, because a variable of an enum type is a
    string on the wire. A variable reference has no JSON form and is refused.
    """
    if isinstance(value, EnumValue):
        return value.name
    if isinstance(value, Variable):
        raise SdlError("a variable reference has no JSON form")
    if isinstance(value, (list, tuple)):
        return [json_value(item) for item in value]
    if isinstance(value, dict):
        return {key: json_value(item) for key, item in value.items()}
    return value
