# SPDX-License-Identifier: MIT
"""Aosabi harness: the two command lines, the two registries of a pull, and a hostile server.

The notebooks of the 70 band drive the command line of each side in a directory
and compare what it printed, its exit code and the files it wrote. This module
holds the four helpers they share.

- `aosabi` and `reference` run one command of either side and answer the
  `CommandResult` of procs.py with the exit code and both streams as bytes.
  `aosabi` runs the program `aosabi` of the workspace, or `cargo-aosabi` with
  `program="cargo-aosabi"`, which `cli_binary` builds once per kernel with
  `cargo build --locked -p aosabi-cli` at the root. `reference` runs the vendored
  command line with the telemetry off and `LC_ALL=C` and `LANG=C`, because
  `inspect` sorts with `localeCompare`, which follows the process locale.
- `parse_ts_registry` reads the TypeScript registry the `pull` command of the reference writes and
  `parse_rs_registry` the Rust registry `aosabi pull` writes, each into the same
  structure, with no help from the Rust side: the types in order with their kind
  and members, each member with its type, its nullability and its arguments in
  order, the delegate keys in order with their arguments and their return, and
  the keys of `types` in order. `registry_differences` compares two of them after
  `comparable`, which drops the roots the Rust registry writes no module for, and
  reads the first, the reference's, through `lenient_records`, which makes a non
  null member of a record whose type has no value for an absent member nullable,
  the form the Rust registry writes it in, so a bare member of the Rust registry
  shows as a difference.
- `hostile_server` is a server of one connection on 127.0.0.1 that answers a
  request with one of the 15 malformed answers of step 5 of `70_pull`, named in
  `HOSTILE_CASES`.

The structure of a registry is plain data, so a notebook prints it as JSON:

    {"types": [{"name": "User", "kind": "object",
                "members": [{"name": "id", "type": "String", "nullable": false,
                             "args": null}]}],
     "delegate": [{"key": "Query.user",
                   "args": [{"name": "id", "type": "String", "nullable": false}],
                   "return": {"type": "User", "nullable": true}}],
     "type_keys": ["User"]}

A kind is `root` for `Query`, `Mutation` and `Subscription`, `object`,
`interface` for an interface with no implementation, which the Rust registry
writes as the struct of an object without the inherent constant `TYPENAME`,
`union` for a union and for an interface whose possible types the registry lists
in its place, `enum`, `input` and `scalar` for a scalar outside the twelve branded
scalars. The members of a union and of an enum are names alone.
"""

from __future__ import annotations

import re
import socket
import threading
import time
from pathlib import Path

from .paths import paths
from .procs import TELEMETRY_OFF, CommandResult, run

#: The programs of the command line of Aosabi, one library behind two names.
PROGRAMS = ("aosabi", "cargo-aosabi")

#: Data of the vendored reference the compatibility layer is held to: its folder under the root
#: and the specifier of its package.
REFERENCE_DIR = Path("vendor") / "pylon"
REFERENCE_PACKAGE = ("@getcronit", "pylon")

#: The command line of the reference, relative to the root.
REFERENCE_CLI = REFERENCE_DIR / "packages" / REFERENCE_PACKAGE[1] / "dist" / "cli" / "index.js"

#: The locale both sides of a sorted output run under.
LOCALE_C = {"LC_ALL": "C", "LANG": "C"}

#: The programs built in this kernel, by the root they were built at.
_BUILT: dict = {}


def cli_binary(cargo="cargo", program: str = "aosabi", root=None, timeout_s: float = 1800.0) -> Path:
    """The path of `program` under target/debug/, built once per kernel at the root."""
    if program not in PROGRAMS:
        raise ValueError("no program named %s" % program)
    base = paths(root).root
    if base not in _BUILT:
        run([str(cargo), "build", "--locked", "-p", "aosabi-cli"], cwd=base, timeout_s=timeout_s)
        _BUILT[base] = True
    return base / "target" / "debug" / program


def aosabi(args, cwd=None, env=None, cargo="cargo", program: str = "aosabi", stdin: bytes = None,
           timeout_s: float = 600.0, root=None) -> CommandResult:
    """Runs one command of Aosabi in `cwd` and answers its result, whatever its exit code."""
    binary = cli_binary(cargo, program, root)
    full = {"CARGO": str(cargo)} if str(cargo) != "cargo" else {}
    full.update(env or {})
    return run([str(binary), *[str(arg) for arg in args]], cwd=cwd, env=full, stdin=stdin,
               timeout_s=timeout_s, check=False)


def reference(args, cwd=None, env=None, stdin: bytes = None, timeout_s: float = 600.0, root=None) -> CommandResult:
    """Runs one command of the reference in `cwd` and answers its result, whatever its exit code."""
    cli = paths(root).root / REFERENCE_CLI
    full = dict(TELEMETRY_OFF)
    full.update(LOCALE_C)
    full.update(env or {})
    return run(["node", str(cli), *[str(arg) for arg in args]], cwd=cwd, env=full, stdin=stdin,
               timeout_s=timeout_s, check=False)


# -- The registries ---------------------------------------------------------------------------

#: The names of the three roots of a schema.
ROOTS = ("Query", "Mutation", "Subscription")

#: The seven branded aliases both registries open with, in the reference's order.
ALIASES = ("String", "Boolean", "Number", "Any", "Void", "JSONObject", "JSON")

#: The Rust names that are no GraphQL name, each with the name it stands for.
_RUST_UNDERSCORED = {"self_": "self", "super_": "super", "crate_": "crate", "Self_": "Self"}


def _registry() -> dict:
    return {"types": [], "delegate": [], "type_keys": []}


def _ts_type(text: str) -> str:
    """The type `Array<Array<String>>` written as `[[String]]`."""
    text = text.strip()
    if text.startswith("Array<") and text.endswith(">"):
        return "[%s]" % _ts_type(text[len("Array<"):-1])
    return text


def _ts_returning(text: str) -> tuple:
    """The type of a return and whether ` | null` makes it nullable."""
    text = text.strip()
    if text.endswith(" | null"):
        return _ts_type(text[: -len(" | null")]), True
    return _ts_type(text), False


def _ts_params(text: str, separator: str) -> list:
    """The arguments of a method or of an `args` object, in order."""
    params = []
    for part in [p for p in text.split(separator) if p.strip()]:
        name, _, kind = part.strip().partition(":")
        name = name.strip()
        nullable = name.endswith("?")
        params.append({"name": name.rstrip("?"), "type": _ts_type(kind), "nullable": nullable})
    return params


_TS_METHOD = re.compile(r"^  (\w+)\((.*)\): (.+);$")
_TS_FIELD = re.compile(r"^  (\w+)(\??): (.+);$")


def _ts_interface(name: str, body: list) -> dict:
    """One `export interface` block of the types, its kind read from its `__typename` line."""
    kind = "input"
    members = []
    for line in body:
        if line.startswith("  readonly __typename?: "):
            kind = "root"
            continue
        if line == "  readonly __typename: string;":
            kind = "interface"
            continue
        if line.startswith("  readonly __typename: "):
            kind = "object"
            continue
        method = _TS_METHOD.match(line)
        if method:
            returned, nullable = _ts_returning(method.group(3))
            members.append({"name": method.group(1), "type": returned, "nullable": nullable,
                            "args": _ts_params(method.group(2), ", ")})
            continue
        field = _TS_FIELD.match(line)
        if not field:
            raise ValueError("no member in the line %r of %s" % (line, name))
        members.append({"name": field.group(1), "type": _ts_type(field.group(3)),
                        "nullable": field.group(2) == "?", "args": None})
    if kind == "input":
        for member in members:
            member.pop("args")
    if kind == "root":
        for member in members:
            member["args"] = member["args"] or []
    return {"name": name, "kind": kind, "members": members}


def _ts_alias(name: str, value: str) -> dict:
    """One `export type` line: a scalar, an enum or a union."""
    if value == "any":
        return {"name": name, "kind": "scalar", "members": []}
    parts = [part.strip() for part in value.split(" | ")]
    if all(part.startswith("'") and part.endswith("'") for part in parts):
        return {"name": name, "kind": "enum", "members": [part[1:-1] for part in parts]}
    return {"name": name, "kind": "union", "members": parts}


def _ts_remote_registry(lines: list, registry: dict) -> None:
    """The `delegate` and `types` blocks of `interface RemoteRegistry`."""
    section = None
    entry = None
    for line in lines:
        if line == "  delegate: {":
            section = "delegate"
        elif line == "  types: {":
            section = "types"
        elif section == "delegate" and line.startswith("    '") and line.endswith("': {"):
            entry = {"key": line[len("    '"):-len("': {")], "args": [], "return": None}
        elif section == "delegate" and line.startswith("      args: "):
            inner = line[len("      args: "):].rstrip(";").strip()
            inner = inner[1:-1].strip() if inner.startswith("{") else inner
            entry["args"] = _ts_params(inner, "; ")
        elif section == "delegate" and line.startswith("      return: "):
            returned, nullable = _ts_returning(line[len("      return: "):].rstrip(";"))
            entry["return"] = {"type": returned, "nullable": nullable}
        elif section == "delegate" and line == "    };":
            registry["delegate"].append(entry)
            entry = None
        elif section == "types" and line.startswith("    '"):
            registry["type_keys"].append(line.strip().split("'")[1])


def parse_ts_registry(text: str) -> dict:
    """Reads the registry the `pull` command of the reference writes into the structure of this module."""
    registry = _registry()
    lines = text.split("\n")
    at = 0
    while at < len(lines):
        line = lines[at]
        interface = re.match(r"^export interface (\w+) \{$", line)
        alias = re.match(r"^export type (\w+) = (.+);$", line)
        if interface:
            end = lines.index("}", at)
            body = lines[at + 1:end]
            if interface.group(1) == "RemoteRegistry":
                _ts_remote_registry(body, registry)
            else:
                registry["types"].append(_ts_interface(interface.group(1), body))
            at = end + 1
            continue
        if alias:
            registry["types"].append(_ts_alias(alias.group(1), alias.group(2)))
        at += 1
    return registry


_RUST_TOKEN = re.compile(
    r'\s+|//[^\n]*|"(?:\\.|[^"\\])*"|\'static|r#\w+|\w+|::|->|=>|[{}()\[\]<>,;:=&#!*.?]'
)


def _rust_tokens(text: str) -> list:
    """The tokens of a Rust file, white space and line comments left out."""
    tokens = []
    for match in _RUST_TOKEN.finditer(text):
        token = match.group(0)
        if token.isspace() or token.startswith("//"):
            continue
        tokens.append(token)
    return tokens


def _rust_name(token: str) -> str:
    """The GraphQL name of a Rust identifier, `r#type` as `type` and `self_` as `self`."""
    if token.startswith("r#"):
        return token[2:]
    return _RUST_UNDERSCORED.get(token, token)


def _rust_group(tokens: list, at: int) -> int:
    """The index behind the bracket group that opens at `at`."""
    pairs = {"{": "}", "(": ")", "[": "]", "<": ">"}
    opening = tokens[at]
    closing = pairs[opening]
    depth = 0
    for index in range(at, len(tokens)):
        if tokens[index] == opening:
            depth += 1
        elif tokens[index] == closing:
            depth -= 1
            if depth == 0:
                return index + 1
    raise ValueError("no closing %s" % closing)


def _rust_type(tokens: list) -> tuple:
    """A Rust type as the structure writes it, with whether an outer `Option` makes it nullable.
    """
    tokens = [token for token in tokens if token not in ("super", "::", "crate")]
    while tokens and tokens[0] in ("aosabi", "json", "std", "string"):
        tokens = tokens[1:]
    if not tokens:
        raise ValueError("no type")
    head = tokens[0]
    if head in ("Option", "Box", "Vec") and tokens[1] == "<" and tokens[-1] == ">":
        inner, nullable = _rust_type(tokens[2:-1])
        if head == "Option":
            return inner, True
        if head == "Vec":
            return "[%s%s]" % (inner, "?" if nullable else ""), False
        return inner, nullable
    if head == "JsValue":
        return "any", False
    return _rust_name(head), False


def _rust_split(tokens: list, separator: str = ",") -> list:
    """The tokens split at the separators that stand outside every bracket."""
    parts, current, depth = [], [], 0
    for token in tokens:
        if token in ("<", "(", "[", "{"):
            depth += 1
        elif token in (">", ")", "]", "}"):
            depth -= 1
        if token == separator and depth == 0:
            parts.append(current)
            current = []
        else:
            current.append(token)
    if current:
        parts.append(current)
    return parts


def _rust_fields(tokens: list) -> list:
    """The fields `pub name: Type` of a struct body, in order, as name and type tokens."""
    fields = []
    for part in _rust_split(tokens):
        part = [token for token in part if token != "pub"]
        if not part:
            continue
        colon = part.index(":")
        fields.append((_rust_name(part[colon - 1]), part[colon + 1:]))
    return fields


def _rust_items(tokens: list) -> list:
    """The items of a module body in order, each `(kind, name, body tokens, head tokens, attributes)`.

    The attributes are the outer attributes `#[...]` written before the item,
    each as the tokens between its brackets, so `#[aosabi(remote)]` reads
    `["aosabi", "(", "remote", ")"]`. An inner attribute `#![...]` belongs to
    no item.
    """
    items = []
    attributes = []
    at = 0
    while at < len(tokens):
        token = tokens[at]
        if token == "#":
            inner = tokens[at + 1] == "!"
            opening = at + 2 if inner else at + 1
            end = _rust_group(tokens, opening)
            if not inner:
                attributes.append(tokens[opening + 1:end - 1])
            at = end
            continue
        if token == "use":
            at = tokens.index(";", at) + 1
            continue
        if token == "pub":
            at += 1
            continue
        if token in ("mod", "struct", "enum", "impl", "type", "const", "fn"):
            start = at
            while at < len(tokens) and tokens[at] not in ("{", ";"):
                if tokens[at] == "<" and token != "type":
                    at = _rust_group(tokens, at)
                    continue
                at += 1
            head = tokens[start:at]
            if tokens[at] == "{":
                end = _rust_group(tokens, at)
                body = tokens[at + 1:end - 1]
                at = end
            else:
                body = None
                at += 1
            name = head[1] if token != "impl" else " ".join(head[1:])
            items.append((token, name, body, head, attributes))
            attributes = []
            continue
        at += 1
    return items


def _rust_str_list(tokens: list) -> list:
    """The strings of `&["a", "b"]`."""
    return [token[1:-1] for token in tokens if token.startswith('"')]


def _rust_derive(attributes: list) -> dict:
    """What the attributes of an item state: the traits of `#[derive(...)]` and the keys of `#[aosabi(...)]`.

    A key with a value, `typename = "User"` or `value = "self"`, is kept with
    its string in `values`.
    """
    derive, keys, values = [], [], {}
    for attribute in attributes:
        if len(attribute) < 3 or attribute[1] != "(":
            continue
        inner = attribute[2:-1]
        if attribute[0] == "derive":
            derive.extend(part[-1] for part in _rust_split(inner) if part)
        elif attribute[0] == "aosabi":
            for part in _rust_split(inner):
                if len(part) == 3 and part[1] == "=" and part[2].startswith('"'):
                    values[part[0]] = part[2][1:-1]
                elif part:
                    keys.append(part[0])
    return {"derive": derive, "keys": keys, "values": values}


def _rust_variants(body: list) -> list:
    """The variants of an enum body in order, each `(GraphQL name, tokens)`.

    The attribute `#[aosabi(value = "...")]` of a variant states the GraphQL
    name of a value whose Rust name is not its name.
    """
    variants = []
    for part in _rust_split(body):
        attributes = []
        while len(part) > 1 and part[0] == "#":
            end = _rust_group(part, 1)
            attributes.append(part[2:end - 1])
            part = part[end:]
        if not part:
            continue
        stated = _rust_derive(attributes)["values"].get("value")
        variants.append((stated if stated is not None else _rust_name(part[0]), part))
    return variants


def _rust_root(name: str, body: list) -> dict:
    """One root module: a struct per field, each with its `Delegate` implementation."""
    members = []
    returns = {}
    for kind, item, inner, _, _ in _rust_items(body):
        if kind == "struct":
            fields = [] if inner is None else _rust_fields(inner)
            args = []
            for field, type_tokens in fields:
                rendered, nullable = _rust_type(type_tokens)
                args.append({"name": field, "type": rendered, "nullable": nullable})
            members.append({"name": _rust_name(item), "args": args})
        elif kind == "impl" and inner is not None:
            target = _rust_name(item.split(" for ")[-1].strip())
            if "Return" in inner:
                at = inner.index("Return")
                end = inner.index(";", at)
                returns[target] = _rust_type(inner[at + 2:end])
    out = []
    for member in members:
        rendered, nullable = returns.get(member["name"], ("?", False))
        out.append({"name": member["name"], "type": rendered, "nullable": nullable, "args": member["args"]})
    return {"name": name, "kind": "root", "members": out}


def _rust_object(name: str, fields: list, arguments: dict) -> dict:
    """One struct of the types: an object, an interface with no implementation or an input."""
    kind = "input"
    members = []
    for field, type_tokens in fields:
        if field == "__typename":
            kind = "interface" if type_tokens == ["String"] else "object"
            continue
        if type_tokens and type_tokens[0] == "Callable":
            parts = _rust_split(type_tokens[2:-1])
            args_name = _rust_name(parts[0][-1])
            rendered, nullable = _rust_type(parts[1])
            members.append({"name": field, "type": rendered, "nullable": nullable,
                            "args": arguments.get(args_name, [])})
            continue
        rendered, nullable = _rust_type(type_tokens)
        members.append({"name": field, "type": rendered, "nullable": nullable, "args": None})
    if kind == "input":
        for member in members:
            member.pop("args")
    return {"name": name, "kind": kind, "members": members}


def parse_rs_registry(text: str) -> dict:
    """Reads the registry `aosabi pull` writes into the structure of this module.

    `comparable` leaves `derived` out, since the reference writes no derive.
    """
    registry = _registry()
    items = _rust_items(_rust_tokens(text))
    arguments = {}
    derived = {"types": {}, "args": {}}
    for kind, name, body, _, attributes in items:
        if kind == "mod" and name == "args" and body is not None:
            for inner_kind, inner_name, inner_body, _, inner_attributes in _rust_items(body):
                if inner_kind == "struct":
                    arguments[_rust_name(inner_name)] = [
                        dict(zip(("name", "type", "nullable"), (field, *_rust_type(type_tokens))))
                        for field, type_tokens in ([] if inner_body is None else _rust_fields(inner_body))
                    ]
                    stated = _rust_derive(inner_attributes)
                    derived["args"][_rust_name(inner_name)] = {"derive": stated["derive"], "keys": stated["keys"]}
    constants = set()
    remote_types = {}
    needs_targets = set()
    for kind, name, body, _, _ in items:
        if kind != "impl" or body is None:
            continue
        if name.startswith("RemoteType for "):
            target = name[len("RemoteType for "):]
            values = {}
            for part in _rust_split(body, ";"):
                if part and part[0] == "type" and "=" in part:
                    values["registry"] = " ".join(part[part.index("=") + 1:])
                elif part and part[0] == "const" and part[-1].startswith('"'):
                    values["typename"] = part[-1][1:-1]
            remote_types[target] = values
        elif name.startswith("NeedsTarget for "):
            needs_targets.add(name[len("NeedsTarget for "):])
        elif " " not in name and "TYPENAME" in body:
            constants.add(name)
    roots = {}
    for kind, name, body, head, attributes in items:
        stated = _rust_derive(attributes)
        rendered = stated["values"].get("typename", _rust_name(name))
        entry = None
        if kind == "mod" and name in ROOTS and body is not None:
            root = _rust_root(name, body)
            roots[name] = root
            registry["types"].append(root)
        elif kind == "struct" and name not in ("Registry",) and body is not None:
            entry = _rust_object(rendered, _rust_fields(body), arguments)
            if entry["kind"] == "object" and name not in constants and "remote" in stated["keys"]:
                entry["kind"] = "interface"
            registry["types"].append(entry)
        elif kind == "enum" and body is not None:
            variants = _rust_variants(body)
            payload = all("(" in tokens for _, tokens in variants)
            entry = {"name": rendered, "kind": "union" if payload else "enum",
                     "members": [variant for variant, _ in variants]}
            registry["types"].append(entry)
        elif kind == "type" and rendered not in ALIASES:
            entry = {"name": rendered, "kind": "scalar", "members": []}
            registry["types"].append(entry)
        elif kind == "impl" and name.endswith("for Registry") and body is not None:
            for key in ("DELEGATE", "TYPES"):
                if key in body:
                    at = body.index(key)
                    end = body.index(";", at)
                    values = _rust_str_list(body[at:end])
                    if key == "TYPES":
                        registry["type_keys"] = values
                    else:
                        registry["delegate"] = [{"key": value} for value in values]
        if entry is not None:
            record = {"derive": stated["derive"], "keys": stated["keys"],
                      "remote_type": remote_types.get(name), "needs_target": name in needs_targets}
            if kind == "type":
                record["alias"] = "".join(head[3:]) if head[2:3] == ["="] else None
            derived["types"][rendered] = record
    for entry in registry["delegate"]:
        root, _, field = entry["key"].partition(".")
        member = next((m for m in roots.get(root, {"members": []})["members"] if m["name"] == field), None)
        entry["args"] = [] if member is None else member["args"]
        entry["return"] = None if member is None else {"type": member["type"], "nullable": member["nullable"]}
    registry["derived"] = derived
    return registry


def derived_differences(registry: dict) -> list:
    """`NeedsTarget` stands beside every struct and enum a delegation can answer.
    `registry` is what `parse_rs_registry` answers.
    """
    out = []
    derived = registry.get("derived", {"types": {}, "args": {}})
    for kind in registry["types"]:
        if kind["kind"] == "root":
            continue
        name = kind["name"]
        record = derived["types"].get(name)
        if record is None:
            out.append("%s: no record of its derive" % name)
            continue
        if kind["kind"] == "scalar":
            if record.get("alias") != "aosabi::Any":
                out.append("%s: the scalar names %s and not aosabi::Any" % (name, record.get("alias")))
            continue
        if "Aosabi" not in record["derive"]:
            out.append("%s: no #[derive(Aosabi)]" % name)
        remote = kind["kind"] in ("object", "interface", "union")
        if remote != ("remote" in record["keys"]):
            out.append("%s: the %s %s the key remote" % (name, kind["kind"], "lacks" if remote else "carries"))
        if kind["kind"] in ("object", "interface"):
            expected = {"registry": "Registry", "typename": name}
            if record["remote_type"] != expected:
                out.append("%s: RemoteType reads %r where it should read %r" % (name, record["remote_type"], expected))
        elif record["remote_type"] is not None:
            out.append("%s: the %s carries a RemoteType" % (name, kind["kind"]))
        if (kind["kind"] != "input") != record["needs_target"]:
            out.append("%s: the %s %s NeedsTarget" % (name, kind["kind"], "lacks" if kind["kind"] != "input" else "carries"))
    for name, record in derived["args"].items():
        if "Aosabi" not in record["derive"] or "arguments" not in record["keys"]:
            out.append("args::%s: the derive reads %r with the keys %r" % (name, record["derive"], record["keys"]))
    return out


def comparable(registry: dict) -> dict:
    """The registry without the roots the Rust registry writes no module for.

    The Rust registry writes a module for `Query` and `Mutation` when the root
    has a field and nothing for `Subscription` or a root with no field, where the
    reference writes an interface for each.
    """
    types = [t for t in registry["types"]
             if not (t["kind"] == "root" and (t["name"] == "Subscription" or not t["members"]))]
    return {"types": types, "delegate": registry["delegate"], "type_keys": registry["type_keys"]}


def lenient_records(registry: dict) -> dict:
    """The registry with every non null member of a record whose type has no absent value made nullable.

    The read of a pulled record takes a member the remote leaves out as an absent
    one, and an enum, an object, an interface, a union and `Date` have no value
    for an absent member, so `aosabi pull` writes a non null member of such a
    type of an object or an interface as `Option<T>` where the reference writes it
    bare. A list keeps its nullability, since the empty list is its absent value.
    The registry of the reference alone passes through this, so a member the Rust
    registry writes bare where the rule asks for `Option<T>` shows as a
    difference.
    """
    absent_less = {t["name"] for t in registry["types"] if t["kind"] in ("enum", "object", "interface", "union")}
    absent_less.add("Date")
    types = []
    for kind in registry["types"]:
        if kind["kind"] in ("object", "interface"):
            members = []
            for member in kind["members"]:
                if member.get("args") is None and member["type"] in absent_less:
                    member = dict(member, nullable=True)
                members.append(member)
            kind = dict(kind, members=members)
        types.append(kind)
    return dict(registry, types=types)


def registry_differences(left: dict, right: dict) -> list:
    """Every difference of the reference's registry `left` and the Rust registry `right`, as one line each.

    Both pass through `comparable`, and `left` alone through `lenient_records`,
    so the Rust registry is held to the rule of `reads_no_absent`.
    """
    a, b = comparable(lenient_records(left)), comparable(right)
    out = []
    names_a = [t["name"] for t in a["types"]]
    names_b = [t["name"] for t in b["types"]]
    if names_a != names_b:
        out.append("type order %r and %r" % (names_a, names_b))
    by_name = {t["name"]: t for t in b["types"]}
    for kind in a["types"]:
        other = by_name.get(kind["name"])
        if other is None:
            continue
        if kind != other:
            out.append("type %s: %r and %r" % (kind["name"], kind, other))
    if a["delegate"] != b["delegate"]:
        out.append("delegate %r and %r" % (a["delegate"], b["delegate"]))
    if a["type_keys"] != b["type_keys"]:
        out.append("types keys %r and %r" % (a["type_keys"], b["type_keys"]))
    return out


# -- The hostile server -----------------------------------------------------------------------

#: The 15 malformed answers of step 5 of 70_pull, in the order of the plan.
HOSTILE_CASES = (
    "truncated-status-line",
    "head-over-64k",
    "negative-chunk-length",
    "oversized-chunk-length",
    "body-shorter-than-length",
    "body-over-16mib",
    "redirect",
    "status-500",
    "invalid-json",
    "json-too-deep",
    "data-missing",
    "errors-present",
    "data-wrong-type",
    "no-schema",
    "closed-before-head",
)

#: The largest head and body the client of `pull` reads.
HEAD_LIMIT = 64 * 1024
BODY_LIMIT = 16 * 1024 * 1024


def _json_answer(body: bytes, status: str = "200 OK") -> bytes:
    return (b"HTTP/1.1 " + status.encode() + b"\r\nContent-Type: application/json\r\n"
            b"Content-Length: %d\r\nConnection: close\r\n\r\n" % len(body)) + body


def hostile_answer(case: str, port: int) -> list:
    """The pieces one hostile case writes, in order, before it closes the connection."""
    if case == "truncated-status-line":
        return [b"HTTP/1.1 2"]
    if case == "head-over-64k":
        return [b"HTTP/1.1 200 OK\r\nX-Pad: " + b"a" * (HEAD_LIMIT + 1024) + b"\r\n\r\n{}"]
    if case == "negative-chunk-length":
        return [b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n-1\r\n{}\r\n0\r\n\r\n"]
    if case == "oversized-chunk-length":
        return [b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n"
                + b"%x\r\n" % (BODY_LIMIT + 1) + b"{}"]
    if case == "body-shorter-than-length":
        return [b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 1000\r\n"
                b"Connection: close\r\n\r\n{\"data\":"]
    if case == "body-over-16mib":
        head = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"data\":\""
        return [head] + [b"a" * (1024 * 1024)] * 17 + [b"\"}"]
    if case == "redirect":
        return [b"HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:%d/elsewhere\r\nContent-Length: 0\r\n"
                b"Connection: close\r\n\r\n" % port]
    if case == "status-500":
        return [b"HTTP/1.1 500 Internal Server Error\r\nContent-Type: text/plain\r\nContent-Length: 21\r\n"
                b"Connection: close\r\n\r\nInternal Server Error"]
    if case == "invalid-json":
        return [_json_answer(b"{not json")]
    if case == "json-too-deep":
        return [_json_answer(b"{\"data\":" + b"[" * 2000 + b"]" * 2000 + b"}")]
    if case == "data-missing":
        return [_json_answer(b"{\"other\":1}")]
    if case == "errors-present":
        return [_json_answer(b"{\"data\":null,\"errors\":[{\"message\":\"Introspection is disabled\"}]}")]
    if case == "data-wrong-type":
        return [_json_answer(b"{\"data\":\"a string\"}")]
    if case == "no-schema":
        return [_json_answer(b"{\"data\":{}}")]
    if case == "closed-before-head":
        return []
    raise ValueError("no hostile case named %s" % case)


class HostileServer:
    """A server of one connection that answers one hostile case and closes.

    It reads the request head and as much of the body as its `Content-Length`
    names, at most 5 seconds, writes the pieces of the case and closes. A
    client that hangs up early ends the writing quietly. `request` holds what
    the client sent, and `close` waits for the thread.
    """

    def __init__(self, case: str, port: int, host: str = "127.0.0.1") -> None:
        if not port:
            raise ValueError("the port is explicit and never 0")
        self.case = case
        self.port = port
        self.url = "http://%s:%d/graphql" % (host, port)
        self.request = b""
        self.pieces = hostile_answer(case, port)
        self._socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self._socket.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self._socket.bind((host, port))
        self._socket.listen(1)
        self._socket.settimeout(60)
        self._thread = threading.Thread(target=self._serve, daemon=True)
        self._thread.start()

    def _read_request(self, connection) -> None:
        connection.settimeout(5)
        deadline = time.monotonic() + 5
        while b"\r\n\r\n" not in self.request and time.monotonic() < deadline:
            chunk = connection.recv(65536)
            if not chunk:
                return
            self.request += chunk
        head, _, body = self.request.partition(b"\r\n\r\n")
        match = re.search(rb"(?i)\r\ncontent-length:\s*(\d+)", head)
        length = int(match.group(1)) if match else 0
        while len(body) < length and time.monotonic() < deadline:
            chunk = connection.recv(65536)
            if not chunk:
                return
            body += chunk
            self.request += chunk

    def _serve(self) -> None:
        try:
            connection, _ = self._socket.accept()
        except OSError:
            return
        with connection:
            try:
                self._read_request(connection)
                for piece in self.pieces:
                    connection.sendall(piece)
                connection.shutdown(socket.SHUT_WR)
                connection.settimeout(5)
                while connection.recv(65536):
                    pass
            except OSError:
                pass

    def close(self) -> None:
        """Stops listening and waits at most 10 seconds for the connection to end."""
        try:
            self._socket.close()
        finally:
            self._thread.join(timeout=10)

    def __enter__(self) -> "HostileServer":
        return self

    def __exit__(self, *_) -> bool:
        self.close()
        return False


def hostile_server(case: str, port: int, host: str = "127.0.0.1") -> HostileServer:
    """Starts the hostile server of `case` on `port` and returns it, a context manager."""
    if case not in HOSTILE_CASES:
        raise ValueError("no hostile case named %s" % case)
    return HostileServer(case, port, host)
