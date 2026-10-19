# SPDX-License-Identifier: MIT
"""Aosabi harness: the families of a binary, read in Python.

This module reads an ELF file, its program headers, its sections, its symbols
and its relocations, and a wasm module, its sections, its code bodies and its
name section. It demangles the names of Rust in the v0 scheme and in the legacy
scheme. It decodes the aarch64 instructions `adrp` and `add` of `.text`, which
name an address of the read only data, and `mov`, `bl`, `b` and `ret`, which
carry a value to the call that takes it, and it decodes the bodies of a wasm
module for their direct calls. Over those readers it attributes the bytes of a
binary to crates, to the generic families of the standard library and of the
map, to the code reachable only through a vtable or a function pointer, to
`Debug`, to the float code of `core`, to the Unicode case tables, to the vtable
slots of the validation rules, to the relocations by target, to the unwind
tables and to the panic locations with their file names and the checks they
belong to.

The formats are the ELF specification of the System V ABI and its aarch64
supplement, the WebAssembly core specification, section Binary Format, and the
v0 symbol format of Rust. The notebook 17_framework_size builds each probe a
second time with `-C symbol-mangling-version=v0` and `strip = "none"` for these
readers, so every function has a name to read.

`variant` builds one variant of a card with a set of the levers of
tests/harness/toolchain.py, the port of the driver variant.sh of the facts of
that plan, and `elf_buckets` and `wasm_buckets` bucket a build by the fixed
costs Rust adds over C, the port of its driver buckets.py, with no LLVM tool.
The ELF reader gives the symbols a file imports from its shared libraries with
their versions, and the wasm reader the addresses its code names, by which
`wasm_data_items` attributes the data section to the functions that read it, and
the target features of a module.

`rom_script` reads the addresses a linker script of the ROM assigns,
`absolute_symbols` the names such a script defines in a linked image,
`riscv_targets` the targets of the direct calls and jumps of the code of the
image, `archive_closure` the bytes an archive such as `compiler_builtins` would
give a set of names through the relocations of its objects, and
`undefined_names` the names the objects of an archive leave to the link.
"""

from __future__ import annotations

import bisect
import os
import re
import struct
import time
from collections import Counter, defaultdict
from dataclasses import dataclass
from pathlib import Path

from . import toolchain as tc
from .procs import run

STEP = 65_536

PT_LOAD = 1
PT_GNU_RELRO = 0x6474E552
SHF_EXECINSTR = 4
SHT_SYMTAB = 2
SHT_NOBITS = 8
SHT_DYNSYM = 11
SHT_GNU_VERNEED = 0x6FFFFFFE
SHT_GNU_VERSYM = 0x6FFFFFFF
STT_OBJECT = 1
STT_SECTION = 3
STT_FILE = 4
R_AARCH64_RELATIVE = 1027

UNWIND_SECTIONS = (".eh_frame", ".eh_frame_hdr", ".gcc_except_table")

# The traits whose methods fill the vtables of the validation rules, the
# Visitor of one method per kind and direction and the EnterLeave of two.
RULE_TRAITS = re.compile(r"aosabi_lang::visitor::(Visitor|EnterLeave)>")


# The ELF reader.


@dataclass(frozen=True)
class Section:
    """A section header of an ELF file."""

    name: str
    kind: int
    flags: int
    addr: int
    offset: int
    size: int
    link: int
    entsize: int


@dataclass(frozen=True)
class Segment:
    """A program header of an ELF file."""

    kind: int
    flags: int
    offset: int
    vaddr: int
    filesz: int
    memsz: int


@dataclass(frozen=True)
class Symbol:
    """A defined symbol of the symbol table, its raw name demangled into `name`."""

    raw: str
    name: str
    addr: int
    size: int
    section: str
    kind: int


class Elf:
    """An ELF file of 32 or 64 bits, little endian, read from its bytes.

    The file is read as the headers describe it and nothing is assumed of the
    order of its sections, so a linked binary, an object of an archive and a
    file with or without its symbol table are read alike.
    """

    def __init__(self, data: bytes) -> None:
        assert data[:4] == b"\x7fELF", "no ELF file"
        assert data[5] == 1, "no little endian ELF file"
        self.data = data
        self.wide = data[4] == 2
        if self.wide:
            phoff, shoff = struct.unpack_from("<QQ", data, 0x20)
            phentsize, phnum, shentsize, shnum, shstrndx = struct.unpack_from("<HHHHH", data, 0x36)
            header = "<IIQQQQIIQQ"
        else:
            phoff, shoff = struct.unpack_from("<II", data, 0x1C)
            phentsize, phnum, shentsize, shnum, shstrndx = struct.unpack_from("<HHHHH", data, 0x2A)
            header = "<IIIIIIIIII"
        raw = []
        for index in range(shnum):
            raw.append(struct.unpack_from(header, data, shoff + index * shentsize))
        names_at = raw[shstrndx][4] if raw else 0
        self.sections = []
        for name_off, kind, flags, addr, offset, size, link, _info, _align, entsize in raw:
            start = names_at + name_off
            name = data[start : data.index(b"\0", start)].decode("ascii")
            self.sections.append(Section(name, kind, flags, addr, offset, size, link, entsize))
        self.segments = []
        for index in range(phnum):
            at = phoff + index * phentsize
            if self.wide:
                fields = struct.unpack_from("<IIQQQQQQ", data, at)
                kind, flags, offset, vaddr, _paddr, filesz, memsz, _align = fields
            else:
                fields = struct.unpack_from("<IIIIIIII", data, at)
                kind, offset, vaddr, _paddr, filesz, memsz, flags, _align = fields
            self.segments.append(Segment(kind, flags, offset, vaddr, filesz, memsz))

    @classmethod
    def read(cls, path) -> Elf:
        """Read the ELF file at `path`."""
        return cls(Path(path).read_bytes())

    def section(self, name: str) -> Section | None:
        """Return the first section called `name`, or None."""
        return next((section for section in self.sections if section.name == name), None)

    def section_at(self, addr: int) -> Section | None:
        """Return the allocated section that holds the address `addr`, or None.

        The template of the thread local data without bytes in the file,
        `.tbss`, shares its addresses with the sections after it, so a
        section with bytes in the file comes first.
        """
        found = None
        for section in self.sections:
            if section.addr and section.addr <= addr < section.addr + section.size:
                if section.kind != SHT_NOBITS:
                    return section
                found = found or section
        return found

    def read_at(self, addr: int, count: int) -> bytes | None:
        """Return `count` bytes the file holds at the address `addr`, or None when no section holds them."""
        section = self.section_at(addr)
        if section is None or section.kind == SHT_NOBITS:
            return None
        start = section.offset + addr - section.addr
        return self.data[start : start + count]

    def symbols(self) -> list[Symbol]:
        """Return the defined symbols of the symbol table, each name demangled.

        A symbol of no section, of a special section, of a file or of a
        section, and the mapping symbols `$x` and `$d` of aarch64 are left out.
        """
        found = []
        for table in self.sections:
            if table.kind != SHT_SYMTAB:
                continue
            strings = self.sections[table.link]
            size = 24 if self.wide else 16
            for at in range(table.offset, table.offset + table.size, size):
                if self.wide:
                    fields = struct.unpack_from("<IBBHQQ", self.data, at)
                    name_off, info, _other, shndx, value, length = fields
                else:
                    fields = struct.unpack_from("<IIIBBH", self.data, at)
                    name_off, value, length, info, _other, shndx = fields
                kind = info & 0xF
                if shndx == 0 or shndx >= 0xFF00 or kind in (STT_FILE, STT_SECTION):
                    continue
                start = strings.offset + name_off
                raw = self.data[start : self.data.index(b"\0", start)].decode("utf-8", "replace")
                if not raw or raw.startswith("$"):
                    continue
                found.append(Symbol(raw, demangle(raw), value, length, self.sections[shndx].name, kind))
        return found

    def relocations(self) -> dict[int, int]:
        """Return the relative relocations as a map from the place to the address it receives.

        The section `.rela.dyn` carries them as `R_AARCH64_RELATIVE` with the
        address as the addend, and the section `.relr.dyn` of a file linked with
        packed relative relocations as a list of places, whose word in the file
        holds the address.
        """
        found = {}
        rela = self.section(".rela.dyn")
        if rela is not None:
            for at in range(rela.offset, rela.offset + rela.size, 24):
                place, info, addend = struct.unpack_from("<QQq", self.data, at)
                if info & 0xFFFFFFFF == R_AARCH64_RELATIVE:
                    found[place] = addend
        relr = self.section(".relr.dyn")
        if relr is not None:
            base = 0
            for at in range(relr.offset, relr.offset + relr.size, 8):
                entry = struct.unpack_from("<Q", self.data, at)[0]
                if entry & 1 == 0:
                    found[entry] = struct.unpack("<Q", self.read_at(entry, 8))[0]
                    base = entry + 8
                    continue
                bits = entry >> 1
                for index in range(63):
                    if bits >> index & 1:
                        place = base + 8 * index
                        found[place] = struct.unpack("<Q", self.read_at(place, 8))[0]
                base += 63 * 8
        return found

    def dynamic_imports(self) -> list[tuple[str, str]]:
        """Return the symbols the file takes from its shared libraries, each with the version it asks.

        A symbol of `.dynsym` with no section is one the loader resolves in a
        library, and the entry of `.gnu.version` beside it names its version
        through the auxiliary entries of `.gnu.version_r`.
        """
        dynsym = next((section for section in self.sections if section.kind == SHT_DYNSYM), None)
        if dynsym is None:
            return []
        strings = self.sections[dynsym.link]
        size = 24 if self.wide else 16
        versions = {}
        needed = next((section for section in self.sections if section.kind == SHT_GNU_VERNEED), None)
        if needed is not None:
            names = self.sections[needed.link]
            at = needed.offset
            while True:
                _version, count, _file, aux, following = struct.unpack_from("<HHIII", self.data, at)
                cursor = at + aux
                for _ in range(count):
                    _hash, _flags, other, name_off, next_aux = struct.unpack_from("<IHHII", self.data, cursor)
                    start = names.offset + name_off
                    versions[other] = self.data[start : self.data.index(b"\0", start)].decode("ascii", "replace")
                    if not next_aux:
                        break
                    cursor += next_aux
                if not following:
                    break
                at += following
        versym = next((section for section in self.sections if section.kind == SHT_GNU_VERSYM), None)
        found = []
        for index, at in enumerate(range(dynsym.offset, dynsym.offset + dynsym.size, size)):
            if self.wide:
                name_off, _info, _other, shndx, _value, _length = struct.unpack_from("<IBBHQQ", self.data, at)
            else:
                name_off, _value, _length, _info, _other, shndx = struct.unpack_from("<IIIBBH", self.data, at)
            if index == 0 or shndx != 0:
                continue
            start = strings.offset + name_off
            name = self.data[start : self.data.index(b"\0", start)].decode("utf-8", "replace")
            version = ""
            if versym is not None:
                tag = struct.unpack_from("<H", self.data, versym.offset + 2 * index)[0] & 0x7FFF
                version = versions.get(tag, "")
            found.append((name, version))
        return found

    def other_relocations(self) -> Counter:
        """Count the dynamic relocations that are not relative, by their type number."""
        counts = Counter()
        for name in (".rela.dyn", ".rela.plt"):
            rela = self.section(name)
            if rela is None:
                continue
            for at in range(rela.offset, rela.offset + rela.size, 24):
                kind = struct.unpack_from("<Q", self.data, at + 8)[0] & 0xFFFFFFFF
                if kind != R_AARCH64_RELATIVE:
                    counts[kind] += 1
        return counts


def elf_measure(path, names, step: int = STEP) -> dict:
    """Measure an ELF64 little endian file: its loads, its step of `step` bytes and the sections `names`.

    The loaded bytes are the sum of the bytes the load segments carry in the
    file, the padding is what lies between them, and a section that occurs
    more than once counts with the sum of its sizes.
    """
    elf = Elf.read(path)
    assert elf.wide, "%s is no ELF64 little endian file" % Path(path).name
    loads = sorted((segment.offset, segment.filesz) for segment in elf.segments if segment.kind == PT_LOAD)
    relro_end = 0
    for segment in elf.segments:
        if segment.kind == PT_GNU_RELRO:
            relro_end = segment.vaddr + segment.memsz
    padding = sum(after[0] - (before[0] + before[1]) for before, after in zip(loads, loads[1:]))
    sizes = Counter()
    for section in elf.sections:
        sizes[section.name] += section.size
    return {
        "file": len(elf.data),
        "loads": len(loads),
        "loaded": sum(filesz for _, filesz in loads),
        "padding": padding,
        "to drop a step": step - padding % step,
        "relro end": relro_end,
        "sections": {name: sizes.get(name, 0) for name in names},
    }


def ar_members(data: bytes):
    """Yield the name and the bytes of every member of an ar archive."""
    assert data[:8] == b"!<arch>\n", "no ar archive"
    at = 8
    while at + 60 <= len(data):
        header = data[at : at + 60]
        size = int(header[48:58].decode("ascii").strip())
        yield header[:16].decode("ascii").strip(), data[at + 60 : at + 60 + size]
        at += 60 + size + (size & 1)


# The wasm reader.


def leb128(data: bytes, at: int) -> tuple[int, int]:
    """Read an unsigned LEB128 at `at` and return it with the offset after it."""
    value = shift = 0
    while True:
        byte = data[at]
        at += 1
        value |= (byte & 0x7F) << shift
        shift += 7
        if byte < 0x80:
            return value, at


def sleb128(data: bytes, at: int) -> tuple[int, int]:
    """Read a signed LEB128 at `at` and return it with the offset after it."""
    value = shift = 0
    while True:
        byte = data[at]
        at += 1
        value |= (byte & 0x7F) << shift
        shift += 7
        if byte < 0x80:
            if byte & 0x40:
                value -= 1 << shift
            return value, at


def wasm_name(data: bytes, at: int) -> tuple[str, int]:
    """Read a name of a wasm module at `at` and return it with the offset after it."""
    length, at = leb128(data, at)
    return data[at : at + length].decode("utf-8", "replace"), at + length


@dataclass(frozen=True)
class Body:
    """A function body of the code section, with its bytes in the section, its size field included."""

    index: int
    start: int
    end: int
    size: int


class Wasm:
    """A wasm module of version 1, read from its bytes.

    The index space of the functions starts with the imported functions, and
    subsection 1 of the custom section "name" maps a function index to its name.
    """

    def __init__(self, data: bytes) -> None:
        assert data[:8] == b"\0asm\x01\0\0\0", "no wasm module of version 1"
        self.data = data
        self.sections = []
        self.payload = Counter()
        self.imports = []
        self.imported_functions = 0
        self.exports = {}
        self.bodies = []
        self.names = {}
        self.element = set()
        self.segments = []
        self.data_ranges = []
        self.custom = {}
        at = 8
        while at < len(data):
            kind = data[at]
            size, start = leb128(data, at + 1)
            end = start + size
            self.payload[kind] += size
            label = kind
            if kind == 0:
                label, cursor = wasm_name(data, start)
                self.custom[label] = data[cursor:end]
                if label == "name":
                    self._names(cursor, end)
            elif kind == 2:
                self._imports(start)
            elif kind == 7:
                self._exports(start)
            elif kind == 9:
                self._elements(start)
            elif kind == 10:
                self._bodies(start)
            elif kind == 11:
                self._segments(start)
            self.sections.append((label, end - at))
            at = end

    @classmethod
    def read(cls, path) -> Wasm:
        """Read the wasm module at `path`."""
        return cls(Path(path).read_bytes())

    def _names(self, cursor: int, end: int) -> None:
        data = self.data
        while cursor < end:
            sub = data[cursor]
            size, start = leb128(data, cursor + 1)
            if sub == 1:
                count, at = leb128(data, start)
                for _ in range(count):
                    index, at = leb128(data, at)
                    self.names[index], at = wasm_name(data, at)
            cursor = start + size

    def _imports(self, at: int) -> None:
        data = self.data
        count, at = leb128(data, at)
        for _ in range(count):
            module, at = wasm_name(data, at)
            field, at = wasm_name(data, at)
            self.imports.append("%s.%s" % (module, field))
            kind = data[at]
            at += 1
            if kind == 0:
                _, at = leb128(data, at)
                self.imported_functions += 1
            elif kind in (1, 2):
                if kind == 1:
                    at += 1
                flags, at = leb128(data, at)
                _, at = leb128(data, at)
                if flags & 1:
                    _, at = leb128(data, at)
            elif kind == 3:
                at += 2
            else:
                raise ValueError("import kind %d is not read" % kind)

    def _exports(self, at: int) -> None:
        data = self.data
        count, at = leb128(data, at)
        for _ in range(count):
            name, at = wasm_name(data, at)
            kind = data[at]
            index, at = leb128(data, at + 1)
            self.exports[name] = (kind, index)

    def _elements(self, at: int) -> None:
        data = self.data
        count, at = leb128(data, at)
        for _ in range(count):
            flags, at = leb128(data, at)
            if flags != 0:
                raise ValueError("an element segment with flags %d is not read" % flags)
            count_of, at = leb128(data, self._offset(at))
            for _ in range(count_of):
                index, at = leb128(data, at)
                self.element.add(index)

    def _offset(self, at: int) -> int:
        """Step over the offset expression of a segment at `at` and return the offset after it.

        An offset is one constant instruction and the end byte 0x0B: `i32.const`
        or `i64.const` with a signed LEB128, or `global.get` with an index.
        """
        return self._offset_value(at)[1]

    def _offset_value(self, at: int) -> tuple[int | None, int]:
        """Return the value of the offset expression at `at`, None for a `global.get`, and the offset after it."""
        data = self.data
        op = data[at]
        value = None
        if op in (0x41, 0x42):
            value, at = sleb128(data, at + 1)
        elif op == 0x23:
            _, at = leb128(data, at + 1)
        else:
            raise ValueError("an offset expression with opcode 0x%02x is not read" % op)
        if data[at] != 0x0B:
            raise ValueError("an offset expression of more than one instruction is not read")
        return value, at + 1

    def _bodies(self, at: int) -> None:
        data = self.data
        count, at = leb128(data, at)
        for index in range(count):
            size, start = leb128(data, at)
            self.bodies.append(Body(self.imported_functions + index, start, start + size, start + size - at))
            at = start + size

    def _segments(self, at: int) -> None:
        data = self.data
        count, at = leb128(data, at)
        for _ in range(count):
            flags, at = leb128(data, at)
            address = None
            if flags in (0, 2):
                if flags == 2:
                    _, at = leb128(data, at)
                address, at = self._offset_value(at)
            length, at = leb128(data, at)
            self.segments.append(data[at : at + length])
            self.data_ranges.append((address, length))
            at += length

    def calls(self, body: Body) -> tuple[list[int], bool]:
        """Return the direct call targets of `body` and whether it calls through the table.

        Every instruction of the MVP and of the proposals the toolchain turns
        on by default for `wasm32-unknown-unknown` is stepped over by its
        immediates, sign extension, the saturating conversions, bulk memory,
        reference types and multiple values, and any other opcode stops the
        read with an error rather than guess its length.
        """
        calls, indirect, _ = self.operands(body)
        return calls, indirect

    def operands(self, body: Body) -> tuple[list[int], bool, list[int]]:
        """Return the direct call targets of `body`, whether it calls through the table, and its addresses.

        The addresses are the values of its `i32.const` and the offsets of its
        loads and stores, which is how the code of a wasm32 module names the
        address of a static in linear memory, the start of a table or one of
        its entries, as `calls` reads the instructions.
        """
        data = self.data
        groups, at = leb128(data, body.start)
        for _ in range(groups):
            _, at = leb128(data, at)
            at += 1
        calls = []
        addresses = []
        indirect = False
        end = body.end
        while at < end:
            op = data[at]
            at += 1
            if op in _NO_IMMEDIATE:
                continue
            if op in _ONE_INDEX:
                if op == 0x10:
                    target, at = leb128(data, at)
                    calls.append(target)
                else:
                    _, at = leb128(data, at)
            elif op in (0x02, 0x03, 0x04):
                kind = data[at]
                if kind == 0x40 or kind >= 0x6F:
                    at += 1
                else:
                    _, at = sleb128(data, at)
            elif op == 0x11:
                _, at = leb128(data, at)
                _, at = leb128(data, at)
                indirect = True
            elif op == 0x0E:
                count, at = leb128(data, at)
                for _ in range(count + 1):
                    _, at = leb128(data, at)
            elif op == 0x1C:
                count, at = leb128(data, at)
                at += count
            elif 0x28 <= op <= 0x3E:
                _, at = leb128(data, at)
                offset, at = leb128(data, at)
                addresses.append(offset)
            elif op in (0x3F, 0x40, 0xD0):
                at += 1
            elif op == 0x41:
                value, at = sleb128(data, at)
                addresses.append(value & 0xFFFFFFFF)
            elif op == 0x42:
                _, at = sleb128(data, at)
            elif op == 0x43:
                at += 4
            elif op == 0x44:
                at += 8
            elif op == 0xFC:
                sub, at = leb128(data, at)
                if sub == 8:
                    _, at = leb128(data, at)
                    at += 1
                elif sub in (9, 13, 15, 16, 17):
                    _, at = leb128(data, at)
                elif sub == 10:
                    at += 2
                elif sub == 11:
                    at += 1
                elif sub in (12, 14):
                    _, at = leb128(data, at)
                    _, at = leb128(data, at)
                elif sub > 17:
                    raise ValueError("opcode 0xfc %d at %d is not read" % (sub, at))
            else:
                raise ValueError("opcode 0x%02x at %d is not read" % (op, at - 1))
        return calls, indirect, addresses


# The opcodes without an immediate and those with one unsigned index, as the
# core specification, section Instructions of Binary Format, lists them.
_NO_IMMEDIATE = frozenset([0x00, 0x01, 0x05, 0x0B, 0x0F, 0x1A, 0x1B, 0xD1, *range(0x45, 0xC5)])
_ONE_INDEX = frozenset([0x0C, 0x0D, 0x10, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0xD2])


def dominators(succ: dict, root) -> tuple[dict, set]:
    """Return the immediate dominators of the nodes reachable from `root` and those nodes.

    The algorithm is the iterative one of Cooper, Harvey and Kennedy, "A
    Simple, Fast Dominance Algorithm", over the reverse post order of a
    depth first walk.
    """
    order = []
    seen = {root}
    stack = [(root, iter(succ.get(root, ())))]
    while stack:
        node, children = stack[-1]
        for child in children:
            if child not in seen:
                seen.add(child)
                stack.append((child, iter(succ.get(child, ()))))
                break
        else:
            order.append(node)
            stack.pop()
    rpo = list(reversed(order))
    number = {node: index for index, node in enumerate(rpo)}
    pred = defaultdict(list)
    for node in rpo:
        for child in succ.get(node, ()):
            if child in number:
                pred[child].append(node)
    idom = {root: root}
    changed = True
    while changed:
        changed = False
        for node in rpo[1:]:
            new = None
            for parent in pred[node]:
                if parent not in idom:
                    continue
                if new is None:
                    new = parent
                    continue
                a, b = parent, new
                while a != b:
                    while number[a] > number[b]:
                        a = idom[a]
                    while number[b] > number[a]:
                        b = idom[b]
                new = a
            if idom.get(node) != new:
                idom[node] = new
                changed = True
    return idom, seen


# The demangler of Rust names.


class _Invalid(ValueError):
    """A name that is no valid Rust v0 name."""


_BASIC = {
    "a": "i8", "b": "bool", "c": "char", "d": "f64", "e": "str", "f": "f32", "h": "u8", "i": "isize",
    "j": "usize", "l": "i32", "m": "u32", "n": "i128", "o": "u128", "s": "i16", "t": "u16", "u": "()",
    "v": "...", "x": "i64", "y": "u64", "z": "!", "p": "_",
}
_SIGNED = {"a", "s", "l", "x", "n", "i"}
_INTEGERS = {"h", "t", "m", "y", "o", "j"} | _SIGNED
_BASE62 = "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ"


class _V0:
    """The parser and printer of one name in the v0 scheme, as RFC 2603 defines it.

    Backreferences point into the name after `_R`, and a lifetime bound by a
    `for<...>` binder prints by its index from the innermost binder, so the
    printer keeps the depth of the binders it opened.
    """

    def __init__(self, text: str) -> None:
        self.text = text
        self.at = 0
        self.bound = 0
        self.depth = 0

    def peek(self) -> str:
        return self.text[self.at] if self.at < len(self.text) else ""

    def eat(self, char: str) -> bool:
        if self.peek() == char:
            self.at += 1
            return True
        return False

    def next(self) -> str:
        char = self.peek()
        if not char:
            raise _Invalid("the name ends early")
        self.at += 1
        return char

    def base62(self) -> int:
        if self.eat("_"):
            return 0
        value = 0
        while True:
            char = self.next()
            if char == "_":
                return value + 1
            digit = _BASE62.find(char)
            if digit < 0:
                raise _Invalid("no base 62 digit")
            value = value * 62 + digit

    def tagged(self, tag: str) -> int:
        return self.base62() + 1 if self.eat(tag) else 0

    def decimal(self) -> int:
        start = self.at
        if self.eat("0"):
            return 0
        while self.peek().isdigit():
            self.at += 1
        if start == self.at:
            raise _Invalid("no decimal number")
        return int(self.text[start : self.at])

    def ident(self) -> str:
        coded = self.eat("u")
        length = self.decimal()
        self.eat("_")
        text = self.text[self.at : self.at + length]
        if len(text) != length:
            raise _Invalid("an identifier runs past the end")
        self.at += length
        if not coded:
            return text
        split = text.rfind("_")
        basic, extended = (text[:split], text[split + 1 :]) if split >= 0 else ("", text)
        coded_text = (basic + "-" + extended) if basic else extended
        try:
            return coded_text.encode("ascii").decode("punycode")
        except (UnicodeError, ValueError) as error:
            raise _Invalid("an identifier in punycode that does not decode") from error

    def backref(self):
        start = self.at - 1
        target = self.base62()
        if target >= start:
            raise _Invalid("a backreference that points forward")
        saved = self.at
        self.at = target
        self.depth += 1
        if self.depth > 500:
            raise _Invalid("backreferences nest too deep")
        return saved

    def returning(self, saved: int) -> None:
        self.at = saved
        self.depth -= 1

    def path(self, value: bool) -> str:
        tag = self.next()
        if tag == "C":
            self.tagged("s")
            return self.ident()
        if tag == "N":
            space = self.next()
            if not space.isalpha():
                raise _Invalid("no namespace")
            inner = self.path(value)
            index = self.tagged("s")
            name = self.ident()
            if space.isupper():
                label = {"C": "closure", "S": "shim"}.get(space, space)
                return "%s::{%s%s#%d}" % (inner, label, ":" + name if name else "", index)
            return "%s::%s" % (inner, name) if name else inner
        if tag == "M":
            self.tagged("s")
            self.path(False)
            return "<%s>" % self.type()
        if tag == "X":
            self.tagged("s")
            self.path(False)
            subject = self.type()
            return "<%s as %s>" % (subject, self.path(False))
        if tag == "Y":
            subject = self.type()
            return "<%s as %s>" % (subject, self.path(False))
        if tag == "I":
            inner = self.path(value)
            arguments = []
            while not self.eat("E"):
                arguments.append(self.generic())
            return "%s%s<%s>" % (inner, "::" if value else "", ", ".join(arguments))
        if tag == "B":
            saved = self.backref()
            text = self.path(value)
            self.returning(saved)
            return text
        raise _Invalid("no path at %r" % tag)

    def generic(self) -> str:
        if self.eat("L"):
            return self.lifetime(self.base62())
        if self.eat("K"):
            return self.const()
        return self.type()

    def lifetime(self, index: int) -> str:
        if index == 0:
            return "'_"
        depth = self.bound - index
        if depth < 0:
            raise _Invalid("a lifetime outside its binder")
        return "'" + (chr(ord("a") + depth) if depth < 26 else "_%d" % depth)

    def binder(self) -> str:
        count = self.tagged("G")
        if not count:
            return ""
        names = []
        for _ in range(count):
            self.bound += 1
            names.append(self.lifetime(1))
        return "for<%s> " % ", ".join(names)

    def type(self) -> str:
        tag = self.next()
        if tag in _BASIC:
            return _BASIC[tag]
        if tag == "R" or tag == "Q":
            lifetime = ""
            if self.eat("L"):
                index = self.base62()
                if index:
                    lifetime = self.lifetime(index) + " "
            return "&%s%s%s" % (lifetime, "mut " if tag == "Q" else "", self.type())
        if tag == "P":
            return "*const " + self.type()
        if tag == "O":
            return "*mut " + self.type()
        if tag == "A":
            element = self.type()
            return "[%s; %s]" % (element, self.const())
        if tag == "S":
            return "[%s]" % self.type()
        if tag == "T":
            members = []
            while not self.eat("E"):
                members.append(self.type())
            return "(%s,)" % members[0] if len(members) == 1 else "(%s)" % ", ".join(members)
        if tag == "F":
            bound = self.bound
            prefix = self.binder()
            if self.eat("U"):
                prefix += "unsafe "
            if self.eat("K"):
                abi = "C" if self.eat("C") else self.ident().replace("_", "-")
                prefix += 'extern "%s" ' % abi
            arguments = []
            while not self.eat("E"):
                arguments.append(self.type())
            result = self.type()
            self.bound = bound
            return "%sfn(%s)%s" % (prefix, ", ".join(arguments), "" if result == "()" else " -> " + result)
        if tag == "D":
            bound = self.bound
            prefix = self.binder()
            traits = []
            while not self.eat("E"):
                traits.append(self.dyn_trait())
            self.bound = bound
            if not self.eat("L"):
                raise _Invalid("no lifetime of a trait object")
            index = self.base62()
            text = "dyn " + prefix + " + ".join(traits)
            return text + (" + " + self.lifetime(index) if index else "")
        if tag == "B":
            saved = self.backref()
            text = self.type()
            self.returning(saved)
            return text
        if tag == "W":
            raise _Invalid("a pattern type is not read")
        self.at -= 1
        return self.path(False)

    def dyn_trait(self) -> str:
        text, open_generics = self.open_path()
        bindings = []
        while self.eat("p"):
            name = self.ident()
            bindings.append("%s = %s" % (name, self.type()))
        if not bindings:
            return text + (">" if open_generics else "")
        return text + (", " if open_generics else "<") + ", ".join(bindings) + ">"

    def open_path(self) -> tuple[str, bool]:
        if self.eat("B"):
            saved = self.backref()
            result = self.open_path()
            self.returning(saved)
            return result
        if self.eat("I"):
            inner = self.path(False)
            arguments = []
            while not self.eat("E"):
                arguments.append(self.generic())
            return "%s<%s" % (inner, ", ".join(arguments)), True
        return self.path(False), False

    def const(self) -> str:
        if self.eat("p"):
            return "_"
        if self.eat("B"):
            saved = self.backref()
            text = self.const()
            self.returning(saved)
            return text
        tag = self.next()
        if tag in _INTEGERS:
            negative = tag in _SIGNED and self.eat("n")
            value = self.hex()
            return ("-" if negative else "") + str(value)
        if tag == "b":
            value = self.hex()
            if value > 1:
                raise _Invalid("a bool constant above 1")
            return "true" if value else "false"
        if tag == "c":
            return repr(chr(self.hex()))
        if tag == "e":
            return '*"%s"' % self.hex_bytes().decode("utf-8", "replace")
        if tag in "RQ":
            if tag == "R" and self.eat("e"):
                return '"%s"' % self.hex_bytes().decode("utf-8", "replace")
            return ("&mut " if tag == "Q" else "&") + self.const()
        if tag == "A":
            items = []
            while not self.eat("E"):
                items.append(self.const())
            return "[%s]" % ", ".join(items)
        if tag == "T":
            items = []
            while not self.eat("E"):
                items.append(self.const())
            return "(%s,)" % items[0] if len(items) == 1 else "(%s)" % ", ".join(items)
        if tag == "V":
            name = self.path(True)
            kind = self.next()
            if kind == "U":
                return name
            if kind == "T":
                items = []
                while not self.eat("E"):
                    items.append(self.const())
                return "%s(%s)" % (name, ", ".join(items))
            if kind == "S":
                fields = []
                while not self.eat("E"):
                    self.tagged("s")
                    field = self.ident()
                    fields.append("%s: %s" % (field, self.const()))
                return "%s { %s }" % (name, ", ".join(fields))
            raise _Invalid("no variant constant")
        raise _Invalid("no constant at %r" % tag)

    def hex(self) -> int:
        start = self.at
        while self.peek() and self.peek() in "0123456789abcdef":
            self.at += 1
        if not self.eat("_"):
            raise _Invalid("a constant without its end")
        digits = self.text[start : self.at - 1]
        return int(digits, 16) if digits else 0

    def hex_bytes(self) -> bytes:
        start = self.at
        while self.peek() and self.peek() in "0123456789abcdef":
            self.at += 1
        if not self.eat("_"):
            raise _Invalid("a string constant without its end")
        return bytes.fromhex(self.text[start : self.at - 1])


def demangle_v0(raw: str) -> str:
    """Return the Rust v0 name `raw` in its printed form, or `raw` itself when it is none.

    The instantiating crate and a suffix of LLVM after a dot are not printed,
    and a name that does not parse stays raw, so an unknown form never
    raises.
    """
    for prefix in ("_R", "R", "__R"):
        if raw.startswith(prefix) and len(raw) > len(prefix) and raw[len(prefix)].isupper():
            body = raw[len(prefix) :]
            break
    else:
        return raw
    body = re.split(r"[.$]", body, maxsplit=1)[0]
    try:
        return _V0(body).path(True)
    except (_Invalid, IndexError, ValueError, RecursionError):
        return raw


_LEGACY_ESCAPES = {"SP": "@", "BP": "*", "RF": "&", "LT": "<", "GT": ">", "LP": "(", "RP": ")", "C": ","}


def _unescape_legacy(ident: str) -> str:
    if ident.startswith("_$"):
        ident = ident[1:]
    out = []
    index = 0
    while index < len(ident):
        if ident[index] == "$":
            end = ident.index("$", index + 1)
            code = ident[index + 1 : end]
            out.append(_LEGACY_ESCAPES.get(code) or chr(int(code[1:], 16)))
            index = end + 1
        elif ident.startswith("..", index):
            out.append("::")
            index += 2
        else:
            out.append(ident[index])
            index += 1
    return "".join(out)


def demangle_legacy(raw: str) -> str:
    """Return the Rust legacy name `raw` without its hash, or `raw` itself when it is none.

    The legacy scheme is the Itanium form `_ZN` of length prefixed
    identifiers and `E`, with the escapes `$LT$` and the like inside an
    identifier and a last identifier `h` with sixteen hex digits.
    """
    name = re.sub(r"\.llvm\.\d+$", "", raw)
    if not name.startswith("_ZN") or not name.endswith("E"):
        return raw
    parts = []
    index = 3
    try:
        while name[index] != "E":
            digits = re.match(r"\d+", name[index:]).group(0)
            index += len(digits)
            length = int(digits)
            parts.append(_unescape_legacy(name[index : index + length]))
            index += length
    except (AttributeError, ValueError, IndexError):
        return raw
    if parts and re.fullmatch(r"h[0-9a-f]{16}", parts[-1]):
        parts.pop()
    return "::".join(parts)


def demangle(raw: str) -> str:
    """Return the printed form of the symbol name `raw` in either scheme of Rust, or `raw`."""
    if raw.startswith(("_R", "R", "__R")):
        return demangle_v0(raw)
    if raw.startswith("_ZN"):
        return demangle_legacy(raw)
    return raw


# The attribution. The one rule of the families that differs: a list of generic
# arguments closes at its own `>` and never at the arrow `->`, and a `<` inside
# a list opens a nested one, so `generic_family` gives one key to a family the
# planning scripts split into several. The function `split_impl` ends a list the
# same way and gives the subject and the trait of an impl. The crate of a
# function differs in two narrow cases: `home_crate` gives an impl whose subject
# has no first segment its trait's crate, and a name without a path the C
# runtime and the entry.

WORKSPACE_CRATE = re.compile(r"\b(aosabi(?:_[a-z_]+)?|aoviewer_[a-z_]+|graphql_over_(?:http|graphtp)|in_context_app"
                             r"|errors_app[a-z_]*|size_(?:app|engine|facade|subset|hello)|graphtp|homa)\b")
STD = {"core", "alloc", "std", "compiler_builtins", "panic_abort", "__rustc", "std_detect", "unwind",
       "dlmalloc", "rustc_std_workspace_core", "rustc_std_workspace_alloc"}
DEPS = {"hashbrown", "indexmap", "once_cell", "equivalent"}
PRIMITIVES = {"str", "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128", "isize",
              "f32", "f64", "bool", "char", "_", "T", "F"}
BUILTINS = re.compile(r"^(__[a-z0-9_]+|memcmp|memcpy|memset|memmove|bcmp|strlen)$")


def first_segment(text: str) -> str:
    """Return the first path segment of the type or path `text`, past references and brackets."""
    text = text.strip()
    while True:
        for prefix in ("&mut ", "&", "*const ", "*mut ", "dyn ", "[", "(", "<", "mut "):
            if text.startswith(prefix):
                text = text[len(prefix) :].lstrip()
                break
        else:
            break
    found = re.match(r"[A-Za-z_][A-Za-z0-9_]*", text)
    return found.group(0) if found else "?"


def split_impl(name: str):
    """Return `(T, Trait)` for `<T as Trait>::rest`, `(T, None)` for `<T>::rest` and None otherwise."""
    if not name.startswith("<"):
        return None
    depth = 0
    for index, char in enumerate(name):
        if char == "<":
            depth += 1
        elif char == ">" and name[index - 1] != "-":
            depth -= 1
            if depth == 0:
                inner = name[1:index]
                break
    else:
        return None
    depth = 0
    for index, char in enumerate(inner):
        if char in "<([":
            depth += 1
        elif char in ">)]" and not (char == ">" and inner[index - 1] == "-"):
            depth -= 1
        elif depth == 0 and inner.startswith(" as ", index):
            return inner[:index], inner[index + 4 :]
    return inner, None


def home_crate(name: str) -> str:
    """Return the crate whose source the function `name` comes from.

    A name that is no path of Rust belongs to the runtime of Rust, to the
    builtins of the compiler, to the machine outliner of LLVM, or to the C
    runtime and the entry of the binary.
    """
    if re.match(r"(__rust_|rust_eh_personality$|rust_begin_unwind$|rust_panic$)", name):
        return "std"
    if BUILTINS.match(name) or name.startswith("__aarch64_"):
        return "compiler_builtins"
    if name.startswith("OUTLINED_FUNCTION_"):
        return "(the machine outliner)"
    impl = split_impl(name)
    if impl:
        subject, trait = impl
        crate = first_segment(subject)
        if crate in PRIMITIVES or crate == "?":
            crate = first_segment(trait) if trait else "core"
            if crate in PRIMITIVES or crate == "?":
                crate = "core"
        return crate
    if "::" not in name:
        return "(the C runtime and the entry)"
    return first_segment(name)


def charged_crate(name: str, home: str) -> str:
    """Return the workspace crate a generic of the standard library or of a dependency is made for.

    A function whose home is the standard library or a dependency and whose
    name carries a type or a closure of a workspace crate is charged to the
    first workspace crate its name carries, and every other function stays
    with its home.
    """
    if home in STD or home in DEPS:
        found = WORKSPACE_CRATE.search(name)
        if found:
            return found.group(1)
    return home


def generic_family(name: str) -> str:
    """Return `name` without its lists of generic arguments, which groups the instances of one generic."""
    result = []
    depth = 0
    index = 0
    while index < len(name):
        char = name[index]
        if char == "<":
            before = name[index - 1] if index else " "
            if before.isalnum() or before == "_" or before == ":" or depth > 0:
                depth += 1
                index += 1
                continue
        if char == ">" and depth > 0 and name[index - 1] != "-":
            depth -= 1
            index += 1
            continue
        if depth == 0:
            result.append(char)
        index += 1
    text = "".join(result).replace("::::", "::")
    return re.sub(r"::$", "", text.replace("::(", "("))


def item_path(name: str) -> str:
    """Return the path of the item `name` belongs to: the type of an impl, or the path itself."""
    impl = split_impl(name)
    if impl:
        subject = impl[0]
        while subject.startswith(("&mut ", "&", "*const ", "*mut ")):
            subject = subject.split(" ", 1)[1] if subject.startswith(("*", "&mut")) else subject[1:]
        return generic_family(subject)
    return generic_family(name)


# The families of generic code the plan names. A function may belong to more
# than one. The vector of entries of a map hands the index table its hashes
# through the trait `Hashes` of the table, so a method of that trait counts for
# the table whatever the type that implements it.
HASHES_METHOD = " as aosabi_json::map::table::Hashes>::"
FAMILIES = (
    ("the map of aosabi-json",
     lambda name: item_path(name).startswith(("aosabi_json::map::", "hashbrown::", "indexmap::"))
     or HASHES_METHOD in name),
    ("its index table",
     lambda name: item_path(name).startswith(("aosabi_json::map::table::", "hashbrown::raw::"))
     or HASHES_METHOD in name),
    ("drop_in_place", lambda name: name.startswith("core::ptr::drop_in_place")),
    ("Vec::from_iter", lambda name: generic_family(name).startswith("<alloc::vec::Vec as ")
     and generic_family(name).endswith("::from_iter")),
    ("Vec::clone", lambda name: generic_family(name) == "<alloc::vec::Vec as core::clone::Clone>::clone"),
    ("closures", lambda name: "{closure#" in name or "{closure:" in name),
)

# The groups of code that decide no answer or that the plan weighs one by one.
GROUPS = (
    ("Debug", lambda name: " as core::fmt::Debug>::fmt" in name),
    ("<&T as Debug>::fmt", lambda name: name.startswith("<&") and " as core::fmt::Debug>::fmt" in name),
    ("float formatting of core", lambda name: bool(re.match(
        r"(core::fmt::float::|core::num::flt2dec::|core::num::bignum::|core::num::diy_float::"
        r"|<core::num::bignum::|<f(32|64) as core::fmt::(Debug|Display|LowerExp|UpperExp)>)", name))),
    ("float parsing of core", lambda name: bool(re.match(
        r"(core::num::dec2flt::|<core::num::dec2flt::|<f(32|64) as core::str::traits::FromStr>)", name))),
    ("Unicode case conversion", lambda name: bool(re.match(
        r"(<str>::to_(lower|upper)case|alloc::str::<impl str>::to_(lower|upper)case"
        r"|core::unicode::unicode_data::conversions::)", name))),
    ("Unicode, other tables", lambda name: bool(re.match(
        r"(core::unicode::(unicode_data::(?!conversions::)|printable::)|core::char::methods::)", name))),
)

# The symbols exit line 4 of the plan holds no v0 names build to, unless the
# close page names the call that keeps them.
EXIT_FOUR = re.compile(r"^(<str>::to_(lower|upper)case|alloc::str::<impl str>::to_(lower|upper)case"
                       r"|core::unicode::unicode_data::conversions::|core::fmt::float::"
                       r"|core::num::flt2dec::)")

# The functions a panic location is handed to, by what they check. A bounds
# check is the index of a slice or an array out of its length, a range of a
# slice out of its length or out of order, and a range of a str out of its
# length or off a character boundary.
PANICS = (
    ("bounds check, an index", re.compile(r"^core::panicking::panic_bounds_check$")),
    ("bounds check, a range of a slice", re.compile(
        r"^core::slice::index::slice_(start_index_len|end_index_len|index_order|start_index_overflow"
        r"|end_index_overflow|index)_fail")),
    ("bounds check, a range of a str", re.compile(r"^core::str::(slice_error_fail|str_index_overflow_fail)")),
    ("unwrap or expect of an Option", re.compile(r"^core::option::(unwrap_failed|expect_failed)$")),
    ("unwrap or expect of a Result", re.compile(r"^core::result::unwrap_failed$")),
    ("a length that does not match, copy_from_slice", re.compile(
        r"^core::slice::copy_from_slice_impl::len_mismatch_fail")),
    ("an async fn resumed after it completed", re.compile(
        r"^core::panicking::panic_const::panic_const_(async_fn|coroutine|gen_fn|async_gen_fn)_resumed")),
    ("arithmetic", re.compile(r"^core::panicking::panic_const::panic_const_")),
    ("a RefCell borrowed twice", re.compile(r"^core::cell::panic_already_(mutably_)?borrowed$")),
    ("an assertion", re.compile(r"^core::panicking::assert_failed")),
    ("panic!, unreachable! and the other explicit panics", re.compile(
        r"^(core::panicking::(panic|panic_fmt|panic_str|panic_display|panic_nounwind|panic_explicit"
        r"|unreachable_display)|std::panicking::begin_panic)")),
    ("the allocator and the capacity", re.compile(
        r"^(alloc::raw_vec::handle_error|alloc::raw_vec::capacity_overflow"
        r"|alloc::alloc::handle_alloc_error)$")),
)
BOUNDS = tuple(label for label, _ in PANICS if label.startswith("bounds check"))
PANIC_KINDS = frozenset(label for label, _ in PANICS)


def _top(counter: Counter, count: int) -> list:
    return [[key, value] for key, value in counter.most_common(count)]


def _functions(pairs, top: int) -> dict:
    """Attribute the functions `pairs` of name and size by crate, family and group."""
    by_home = Counter()
    by_charged = Counter()
    count_home = Counter()
    families = defaultdict(lambda: [0, 0])
    named = {label: [0, 0] for label, _ in FAMILIES}
    groups = {label: [0, 0] for label, _ in GROUPS}
    map_families = defaultdict(lambda: [0, 0])
    table_families = defaultdict(lambda: [0, 0])
    exit_four = []
    generic = 0
    library = 0
    made_for = 0
    for name, size in pairs:
        home = home_crate(name)
        by_home[home] += size
        count_home[home] += 1
        charged = charged_crate(name, home)
        by_charged[charged] += size
        if home in STD or home in DEPS:
            library += size
            if charged != home:
                made_for += size
        key = generic_family(name)
        if key != name:
            generic += size
        families[key][0] += 1
        families[key][1] += size
        for label, match in FAMILIES:
            if match(name):
                named[label][0] += 1
                named[label][1] += size
                if label == "the map of aosabi-json":
                    map_families[key][0] += 1
                    map_families[key][1] += size
                if label == "its index table":
                    table_families[key][0] += 1
                    table_families[key][1] += size
        for label, match in GROUPS:
            if match(name):
                groups[label][0] += 1
                groups[label][1] += size
        if EXIT_FOUR.match(name):
            exit_four.append([name, size])
    total = sum(by_home.values())
    ranked = sorted(([key, count, size] for key, (count, size) in families.items() if count > 1),
                    key=lambda row: -row[2])
    ranked_map = sorted(([key, count, size] for key, (count, size) in map_families.items()),
                        key=lambda row: -row[1])
    return {
        "functions": len(pairs),
        "bytes": total,
        "generic bytes": generic,
        "standard library and dependencies": library,
        "of it made for a workspace crate": made_for,
        "by home crate": _top(by_home, None),
        "functions by home crate": dict(count_home.most_common()),
        "by charged crate": _top(by_charged, None),
        "families": named,
        "largest families": ranked[:top],
        "map families by count": ranked_map[:8],
        "index table families": sorted([key, count, size] for key, (count, size) in table_families.items()),
        "groups": groups,
        "exit line 4 symbols": sorted(exit_four, key=lambda row: -row[1]),
    }


# The aarch64 decoder of the instructions the attribution follows.


def a64_decode(word: int, pc: int):
    """Decode `word` at the address `pc` as one of the six instructions the attribution follows.
    """
    if word & 0x9F000000 == 0x90000000:
        immediate = ((word >> 5) & 0x7FFFF) << 2 | (word >> 29) & 3
        if immediate & 0x100000:
            immediate -= 0x200000
        return ("adrp", word & 31, (pc & ~0xFFF) + (immediate << 12))
    if word & 0xFF800000 == 0x91000000:
        immediate = (word >> 10) & 0xFFF
        if word & 0x400000:
            immediate <<= 12
        return ("add", word & 31, (word >> 5) & 31, immediate)
    if word & 0xFFE0FFE0 == 0xAA0003E0:
        return ("mov", word & 31, (word >> 16) & 31)
    if word & 0x7C000000 == 0x14000000:
        offset = word & 0x3FFFFFF
        if offset & 0x2000000:
            offset -= 0x4000000
        return ("bl" if word & 0x80000000 else "b", pc + 4 * offset)
    if word & 0xFFFFFC1F == 0xD65F0000:
        return ("ret",)
    return None


RETURNS = "returns"


class Code:
    """The references, the moves and the control flow of the functions of `.text` of an aarch64 ELF file.

    A reference is an `adrp` and an `add` that name one address, followed
    per function and per register, and kept with the register it lands in. A
    `bl` clobbers the registers x0 to x18, which the procedure call standard
    lets a callee change, so the page of such a register is forgotten at a
    call. The events of a function are its calls, a `bl` or a `b` that
    leaves the function, which is a tail call, its jumps, a `b` inside the
    function, and its returns. The moves from one register to another are
    kept, since a value in a register a callee preserves reaches a call
    through a move into an argument register.
    """

    def __init__(self, elf: Elf, functions: list[Symbol]) -> None:
        text = elf.section(".text")
        self.functions = sorted(functions, key=lambda symbol: symbol.addr)
        self.starts = [symbol.addr for symbol in self.functions]
        self.references = []
        self.events = []
        self.moves = []
        words = struct.iter_unpack("<I", elf.data[text.offset : text.offset + text.size - text.size % 4])
        pages = {}
        owner = None
        for index, (word,) in enumerate(words):
            pc = text.addr + 4 * index
            function = self.function_at(pc)
            if function is not owner:
                pages = {}
                owner = function
            decoded = a64_decode(word, pc)
            if decoded is None:
                continue
            kind = decoded[0]
            if kind == "adrp":
                pages[decoded[1]] = decoded[2]
            elif kind == "add":
                _, rd, rn, immediate = decoded
                if rn in pages:
                    self.references.append((pages[rn] + immediate, pc, rd))
                    if rd == rn:
                        del pages[rn]
                pages.pop(rd, None)
            elif kind == "mov":
                self.moves.append((pc, decoded[1], decoded[2]))
                pages.pop(decoded[1], None)
            elif kind == "ret":
                self.events.append((pc, "ret", 0))
            else:
                target = decoded[1]
                inside = owner is not None and owner.addr < target < owner.addr + owner.size
                self.events.append((pc, "jump" if kind == "b" and inside else "call", target))
                if kind == "bl":
                    for register in range(19):
                        pages.pop(register, None)
        self.event_pcs = [pc for pc, _, _ in self.events]
        self.calls = [(pc, target) for pc, kind, target in self.events if kind == "call"]
        self.move_pcs = [pc for pc, _, _ in self.moves]
        self.callers = defaultdict(list)
        for pc, target in self.calls:
            self.callers[target].append(pc)
        self._leads = {}

    def function_at(self, addr: int) -> Symbol | None:
        """Return the function whose bytes hold `addr`, or None."""
        index = bisect.bisect_right(self.starts, addr) - 1
        if index >= 0:
            function = self.functions[index]
            if function.addr <= addr < function.addr + max(function.size, 4):
                return function
        return None

    def calls_in(self, function: Symbol) -> list[Symbol]:
        """Return the functions `function` calls, in the order of their calls."""
        first = bisect.bisect_left(self.event_pcs, function.addr)
        last = bisect.bisect_left(self.event_pcs, function.addr + function.size)
        found = []
        for _, kind, target in self.events[first:last]:
            callee = self.function_at(target) if kind == "call" else None
            if callee is not None:
                found.append(callee)
        return found

    def leads_to(self, function: Symbol, depth: int = 3) -> frozenset:
        """Return the kinds of panic `function` hands a location to, following its calls `depth` deep.

        A function of the panic machinery is its own kind. Any other function
        leads to the kinds its calls lead to, which is how a function that
        takes the location of its caller, a `#[track_caller]` index or
        borrow, and a sequence the machine outliner moved into a function of
        its own reach the panic they end in.
        """
        key = (function.addr, depth)
        if key in self._leads:
            return self._leads[key]
        kind = panic_kind(function.name)
        if kind is not None:
            found = frozenset([kind])
        elif depth == 0:
            found = frozenset()
        else:
            self._leads[key] = frozenset()
            callees = self.calls_in(function)
            found = frozenset().union(*[self.leads_to(callee, depth - 1) for callee in callees])
        self._leads[key] = found
        return found

    def consumer(self, pc: int, function: Symbol):
        """Return the function the flow from `pc` in `function` calls first, `RETURNS` or None.

        The flow follows the jumps of the function and passes over a call of
        a sequence the machine outliner moved into a function
        `OUTLINED_FUNCTION_<n>` that leads to no panic, since such a sequence
        only sets up registers for the call after it. A conditional branch
        is not decoded, so the flow takes the path that falls through.
        """
        end = function.addr + function.size
        index = bisect.bisect_left(self.event_pcs, pc)
        hops = 0
        while index < len(self.events) and self.events[index][0] < end:
            _, kind, target = self.events[index]
            if kind == "ret":
                return RETURNS
            if kind == "jump":
                hops += 1
                if hops > 16:
                    return None
                index = bisect.bisect_left(self.event_pcs, target)
                continue
            callee = self.function_at(target)
            index += 1
            if callee is None:
                continue
            if callee.name.startswith("OUTLINED_FUNCTION") and not self.leads_to(callee):
                continue
            return callee
        return None

    def handed_to(self, pc: int, register: int, depth: int = 2) -> str:
        """Return the kind of panic the location an `add` at `pc` puts in `register` is handed to.

        A location in an argument or a scratch register goes to the next
        call. One in a register a callee preserves, x19 to x28, is kept
        across calls and reaches a call through a move into an argument
        register, so the call after such a move decides, and a move later in
        the function comes before one earlier in it, which a loop reaches. A
        location an outlined sequence sets up before it returns goes to the
        call that follows each call of that sequence.
        """
        function = self.function_at(pc)
        if function is None:
            return "outside every function"
        if register < 19:
            starts = [pc]
        else:
            first = bisect.bisect_left(self.move_pcs, function.addr)
            last = bisect.bisect_left(self.move_pcs, function.addr + function.size)
            moves = [at for at, _, source in self.moves[first:last] if source == register]
            starts = [at for at in moves if at > pc] + [at for at in moves if at < pc]
        seen = None
        for start in starts:
            callee = self.consumer(start, function)
            if callee == RETURNS:
                if not function.name.startswith("OUTLINED_FUNCTION") or depth == 0:
                    continue
                kinds = Counter()
                for site in self.callers.get(function.addr, ()):
                    kinds[self.handed_to(site + 4, register, depth - 1)] += 1
                known = [label for label in kinds if label in PANIC_KINDS]
                if len(known) == 1:
                    return known[0]
                continue
            if callee is None:
                continue
            seen = seen or callee
            kinds = self.leads_to(callee)
            if len(kinds) == 1:
                return next(iter(kinds))
        if seen is None:
            return "no call follows"
        return "handed to %s" % generic_family(seen.name)[:60]


def panic_kind(name: str) -> str | None:
    """Return the kind of panic the function `name` raises, or None outside the panic machinery."""
    for label, pattern in PANICS:
        if pattern.match(name):
            return label
    return None


def _file_root(path: str) -> str:
    """Return where the file name of a panic location comes from, by its prefix."""
    if path.startswith("aosabi/"):
        return "the tree, aosabi/"
    if path.startswith("cargo/"):
        return "the cargo home, cargo/"
    if path.startswith("/rustc/"):
        return "the standard library, /rustc/"
    if path.startswith("/rust/deps/"):
        return "the crates of the standard library, /rust/deps/"
    if path.startswith("/"):
        return "another absolute path"
    return "a relative path, a member of the twin's workspace"


def panic_locations(elf: Elf, relocations: dict | None = None) -> dict[int, str]:
    """Return the panic locations of the linked ELF file `elf`, from the place of each to its file name.

    A location is a `core::panic::Location` in `.data.rel.ro`, a word that a
    relative relocation fills with the address of a file name in `.rodata`,
    then the length of the name, its line and its column. It needs no symbol,
    so a stripped file is read as well as one with its names.
    """
    if relocations is None:
        relocations = elf.relocations()
    rodata = elf.section(".rodata")
    locations = {}
    if rodata is None:
        return locations
    for place, target in relocations.items():
        if not rodata.addr <= target < rodata.addr + rodata.size:
            continue
        rest = elf.read_at(place + 8, 16)
        if not rest or len(rest) < 16:
            continue
        length, line, column = struct.unpack("<QII", rest)
        if not (3 < length < 400 and 0 < line < 200_000 and column < 4_000):
            continue
        name = elf.read_at(target, length)
        if name and name.endswith(b".rs") and b"\n" not in name:
            locations[place] = name.decode("utf-8", "replace")
    return locations


def location_summary(locations: dict[int, str]) -> dict:
    """Return the count, the bytes and the file names of the panic locations `locations`.

    A location takes 24 bytes of `.data.rel.ro`, and a file name its bytes and
    the zero after them once, however many locations name it.
    """
    files = Counter(locations.values())
    roots = Counter()
    for name, count in files.items():
        roots[_file_root(name)] += count
    return {
        "count": len(locations),
        "bytes of the locations": 24 * len(locations),
        "files": len(files),
        "bytes of the file names": sum(len(name.encode("utf-8")) + 1 for name in files),
        "by root of the file": dict(roots.most_common()),
        "files by root": dict(Counter(_file_root(name) for name in files).most_common()),
        "largest files": _top(files, 10),
        "machine paths": sum(1 for name in files if name.startswith("/home/")),
    }


def elf_families(path, top: int = 15) -> dict:
    """Attribute the aarch64 ELF file at `path`, which carries its symbol table, and return the numbers.

    The code is the sized functions of `.text` and the named read only data
    the sized objects of `.rodata`, each address counted once with the
    largest symbol that starts there.
    """
    elf = Elf.read(path)
    assert elf.section(".symtab") is not None, "%s carries no symbol table" % Path(path).name
    best = {}
    for symbol in elf.symbols():
        key = (symbol.section, symbol.addr)
        if key not in best or symbol.size > best[key].size:
            best[key] = symbol
    symbols = [symbol for symbol in best.values() if symbol.size > 0]
    functions = [symbol for symbol in symbols if symbol.section == ".text"]
    result = _functions([(symbol.name, symbol.size) for symbol in functions], top)
    objects = [symbol for symbol in symbols if symbol.section == ".rodata" and symbol.kind == STT_OBJECT]
    result["exit line 4 symbols"] += sorted(([symbol.name, symbol.size] for symbol in objects
                                             if EXIT_FOUR.match(symbol.name)), key=lambda row: -row[1])
    names = (".text", ".rodata", ".data.rel.ro", ".rela.dyn", ".relr.dyn", *UNWIND_SECTIONS)
    measured = elf_measure(path, names)
    result["loaded"] = measured["loaded"]
    result["relro end"] = measured["relro end"]
    result["padding"] = measured["padding"]
    result["sections"] = measured["sections"]
    result["unwind tables"] = sum(measured["sections"][name] for name in UNWIND_SECTIONS)
    result["text bytes in no function"] = measured["sections"][".text"] - result["bytes"]

    code = Code(elf, functions)
    relocations = elf.relocations()
    text = elf.section(".text")
    rodata = elf.section(".rodata")
    relro = elf.section(".data.rel.ro")

    def inside(addr, section):
        return section is not None and section.addr <= addr < section.addr + section.size

    by_addr = {symbol.addr: symbol for symbol in functions}

    # The read only data. A named object, an address code refers to and an
    # address a relocation receives each open an item, which runs to the next
    # one, and an item belongs to the functions whose code refers to it. A
    # table of core that the symbol table does not name is found by the
    # functions that read it.
    named = {symbol.addr: symbol for symbol in objects}
    readers = defaultdict(set)
    from_data = set()
    for target, pc, _ in code.references:
        if inside(target, rodata):
            function = code.function_at(pc)
            readers[target].add(function.addr if function is not None else -1)
    for target in relocations.values():
        if inside(target, rodata):
            from_data.add(target)
    ordered = sorted(set(named) | set(readers) | from_data)
    items = []
    if ordered and ordered[0] > rodata.addr:
        items.append((rodata.addr, ordered[0] - rodata.addr))
    for index, start in enumerate(ordered):
        end = ordered[index + 1] if index + 1 < len(ordered) else rodata.addr + rodata.size
        items.append((start, end - start))
    rodata_crates = Counter()
    rodata_groups = {label: [0, 0] for label, _ in GROUPS}
    for start, length in items:
        functions_of = [by_addr[addr] for addr in readers.get(start, ()) if addr in by_addr]
        if functions_of:
            crates = {charged_crate(function.name, home_crate(function.name)) for function in functions_of}
            rodata_crates[crates.pop() if len(crates) == 1 else "shared by more than one crate"] += length
            for label, match in GROUPS:
                if all(match(function.name) for function in functions_of):
                    rodata_groups[label][0] += 1
                    rodata_groups[label][1] += length
        elif start in named:
            rodata_crates[home_crate(named[start].name)] += length
        elif start in from_data:
            rodata_crates["referred to by data alone"] += length
        else:
            rodata_crates["referred to by nothing decoded"] += length
    result["read only data"] = {
        "items": len(items),
        "by charged crate": _top(rodata_crates, top),
        "read only by the functions of a group": rodata_groups,
    }

    locations = panic_locations(elf, relocations)
    # A location is one expression of the source, so the sites that refer
    # to it, one per copy of a generic function, hand it to one kind of
    # panic, and the most frequent kind among them decides.
    handed = defaultdict(Counter)
    for target, pc, register in code.references:
        if target in locations:
            handed[target][code.handed_to(pc, register)] += 1
    kinds = Counter()
    for place in locations:
        found = handed.get(place)
        if not found:
            kinds["no reference of code decoded"] += 1
            continue
        known = Counter({label: count for label, count in found.items() if label in PANIC_KINDS})
        kinds[(known or found).most_common(1)[0][0]] += 1
    result["panic locations"] = dict(
        location_summary(locations),
        **{"by the function they are handed to": dict(kinds.most_common()),
           "bounds checks": sum(kinds[label] for label in BOUNDS)})

    # The relocations by what they point at, with the vtable slots of the
    # validation rules, whose trait is aosabi_lang::visitor::Visitor or the
    # trait of two methods aosabi_lang::visitor::EnterLeave.
    by_target = Counter()
    slots = Counter()
    slot_places = []
    debug_slots = []
    for place, target in relocations.items():
        if place in locations:
            by_target["the file name of a panic location"] += 1
        elif inside(target, text):
            function = by_addr.get(target)
            name = function.name if function else ""
            if inside(place, relro) and RULE_TRAITS.search(name):
                by_target["a vtable slot of a rule"] += 1
                slots[target] += 1
                slot_places.append(place)
            else:
                by_target["another function pointer"] += 1
                if name.endswith(" as core::fmt::Debug>::fmt"):
                    debug_slots.append(place)
        elif inside(target, rodata):
            by_target["a pointer into .rodata"] += 1
        elif inside(target, relro):
            by_target["a pointer into .data.rel.ro"] += 1
        else:
            section = elf.section_at(target)
            by_target["a pointer into %s" % (section.name if section else "no section")] += 1
    default, default_slots = slots.most_common(1)[0] if slots else (None, 0)
    # A vtable holds its methods in consecutive words, so a run of slots is
    # one vtable, one rule. The empty defaults of every rule are one function
    # in the file, which carries the name of one of them.
    slot_places.sort()
    runs = sum(1 for index, place in enumerate(slot_places)
               if index == 0 or place - slot_places[index - 1] != 8)
    others = elf.other_relocations()
    result["relocations"] = {
        "relative": len(relocations),
        "by target": dict(by_target.most_common()),
        "other dynamic relocations": sum(others.values()),
    }
    result["rule vtables"] = {
        "slots": sum(slots.values()),
        "vtables": runs,
        "slots at the most common target": default_slots,
        "most common target": by_addr[default].name if default is not None else "",
        "its bytes": by_addr[default].size if default is not None else 0,
        "distinct targets": len(slots),
    }

    # The functions whose code refers to a vtable with a Debug slot, the
    # method after the drop, the size and the alignment of the vtable.
    tables = set(debug_slots) | {place - 24 for place in debug_slots}
    debug_referrers = Counter()
    for target, pc, _ in code.references:
        if target in tables:
            function = code.function_at(pc)
            if function is not None and not function.name.endswith(" as core::fmt::Debug>::fmt"):
                debug_referrers[generic_family(function.name)[:110]] += 1
    result["referrers of Debug vtables"] = _top(debug_referrers, 12)

    # The callers of the case conversions and of the float formatting.
    result["callers"] = {}
    for label in ("Unicode case conversion", "float formatting of core"):
        match = dict(GROUPS)[label]
        members = {symbol.addr for symbol in functions if match(symbol.name)}
        callers = Counter()
        for pc, target in code.calls:
            if target in members:
                function = code.function_at(pc)
                if function is not None and function.addr not in members:
                    callers[function.name[:140]] += 1
        result["callers"][label] = _top(callers, 12)
    return result


TABLE_KINDS = ("validation rules and visitors", "closures, the resolvers, the hooks and the thunks",
               "formatting, Debug, Display and the fmt machinery", "drop glue", "other")


def _table_kind(name: str) -> str:
    """Return the kind of a function reachable only through the table, as the measurement named them.

    The tests are those of the measurement, in its order: a path of
    `core::fmt` before the first list of generic arguments is formatting,
    so `<core::fmt::Formatter>::pad` is, a constructor `Rule>::new` is a
    rule, and every name with `::drop` in it is drop glue.
    """
    if (re.search(r" as core::fmt::(Debug|Display|LowerHex|UpperHex|Write)>", name)
            or name.startswith("core::fmt")
            or "core::fmt::" in name.split("::<")[0]
            or "flt2dec" in name or "dec2flt" in name or "float_to_" in name):
        return TABLE_KINDS[2]
    if ("aosabi_lang::visitor::" in name or "Rule>::new" in name
            or " as aosabi_validate::validation_context::Rule>" in name
            or "aosabi_validate::rules::" in name):
        return TABLE_KINDS[0]
    if "drop_in_place" in name or "::drop" in name:
        return TABLE_KINDS[3]
    if "{closure" in name:
        return TABLE_KINDS[1]
    return TABLE_KINDS[4]


def wasm_families(path, top: int = 15) -> dict:
    """Attribute the wasm module at `path`, which carries its name section, and return the numbers.

    The code reachable only through the table is what the dominator tree of
    the direct call graph puts below the table: the root reaches every
    exported function, the `main` of every crate outside the standard
    library, which the runtime of a binary calls through a function pointer
    and which is the entry of the program, and one node for the table, which
    reaches every function of the element section, so a function the table
    dominates is called only through a vtable or a function pointer.
    """
    module = Wasm.read(path)
    assert module.names, "%s carries no name section" % Path(path).name
    names = {index: demangle(raw) for index, raw in module.names.items()}
    sizes = {body.index: body.size for body in module.bodies}
    result = _functions([(names.get(body.index, "?"), body.size) for body in module.bodies], top)
    result["unnamed functions"] = sum(1 for body in module.bodies if body.index not in module.names)
    result["code section"] = module.payload[10]
    result["data section"] = module.payload[11]
    result["file"] = len(module.data)

    succ = {}
    table_callers = 0
    for body in module.bodies:
        calls, indirect = module.calls(body)
        succ[body.index] = sorted(set(calls))
        table_callers += indirect
    root, table = -1, -2
    entries = {index for kind, index in module.exports.values() if kind == 0}
    entries |= {index for index, name in names.items()
                if re.fullmatch(r"[a-z_][a-z0-9_]*::main", name) and home_crate(name) not in STD}
    succ[root] = sorted(entries) + [table]
    succ[table] = sorted(module.element)
    idom, reached = dominators(succ, root)
    children = defaultdict(list)
    for node, parent in idom.items():
        if node != root:
            children[parent].append(node)
    below = []
    stack = list(children[table])
    while stack:
        node = stack.pop()
        below.append(node)
        stack.extend(children[node])
    kinds = Counter()
    crates = Counter()
    for node in below:
        name = names.get(node, "?")
        kinds[_table_kind(name)] += sizes.get(node, 0)
        crates[charged_crate(name, home_crate(name))] += sizes.get(node, 0)
    total = sum(sizes.values())
    only = sum(sizes.get(node, 0) for node in below)
    result["reachable only through the table"] = {
        "bytes": only,
        "share of the code": round(only / total, 4) if total else 0,
        "functions": len(below),
        "in the element section": len(module.element),
        "bodies that call through the table": table_callers,
        "by kind": _top(kinds, 8),
        "by charged crate": _top(crates, 8),
    }
    unreached = [index for index in sizes if index not in reached]
    result["reached by no call"] = [len(unreached), sum(sizes[index] for index in unreached)]

    callers = Counter()
    match = dict(GROUPS)["Unicode case conversion"]
    members = {index for index, name in names.items() if match(name)}
    for index, targets in succ.items():
        if index >= 0 and index not in members and any(target in members for target in targets):
            callers[names.get(index, "?")[:140]] += 1
    result["callers"] = {"Unicode case conversion": _top(callers, 12)}
    return result


# The fixed costs Rust adds over C, by bucket.


#: The buckets of the fixed costs of Rust over C, in the order in which the
#: first pattern a demangled name matches decides, as the driver buckets.py of
#: the facts named them.
BUCKETS = (
    ("the symbolizer, gimli, addr2line, object, miniz_oxide, rustc_demangle and std::backtrace",
     re.compile(r"(^|[<\s(&])(gimli|addr2line|object|miniz_oxide|rustc_demangle)::|std::backtrace"
                r"|std::sys::backtrace|backtrace_rs|std::sys_common::backtrace|<std::backtrace|::backtrace::")),
    ("the panic machinery, std::panicking, core::panicking, the hook and rust_panic",
     re.compile(r"std::panicking|core::panicking|std::panic::|rust_panic|__rust_start_panic|rust_begin_unwind"
                r"|__rustc::rust_begin_unwind|panic_abort|core::panic::|::panic_fmt|begin_panic|std::rt::"
                r"|default_hook|lang_start|__rust_abort|__rust_foreign_exception|rust_eh_personality|_Unwind"
                r"|\bunwind::")),
    ("the float formatting of core, flt2dec and fmt::float", re.compile(r"core::fmt::float|core::num::flt2dec")),
    ("the float parsing of core, dec2flt", re.compile(r"core::num::dec2flt")),
    ("the machinery of core::fmt, the Formatter, write, pad and the integers",
     re.compile(r"core::fmt::|alloc::fmt::format|<core::fmt::|::fmt::write|Formatter|core::fmt$")),
    ("the Debug methods", re.compile(r" as core::fmt::Debug>::fmt|<&T as core::fmt::Debug>|::Debug>::fmt"
                                     r"|DebugStruct|DebugTuple|DebugList|DebugMap|DebugSet")),
    ("the Display methods", re.compile(r" as core::fmt::Display>::fmt")),
    ("std::io and io::Error", re.compile(r"std::io::|<std::io::")),
    ("std::sys, fs, env, net, thread, sync, time, os, ffi, path, process, alloc and collections",
     re.compile(r"std::sys::|std::fs::|std::env::|std::net::|std::thread::|std::sync::|std::time::|std::os::"
                r"|std::ffi::|std::path::|std::process::|std::alloc::|std::collections::")),
    ("the allocator, alloc::alloc, raw_vec, dlmalloc and __rust_alloc",
     re.compile(r"__rust_alloc|__rust_dealloc|__rust_realloc|__rdl_|__rg_|alloc::alloc::|raw_vec::|dlmalloc"
                r"|alloc::raw_vec|handle_alloc_error|capacity_overflow")),
    ("the stable sort of core", re.compile(r"core::slice::sort::stable")),
    ("the unstable sort of core", re.compile(r"core::slice::sort::")),
    ("drop_in_place", re.compile(r"core::ptr::drop_in_place")),
    ("other code of core", re.compile(r"^core::|<core::")),
    ("other code of alloc", re.compile(r"^alloc::|<alloc::")),
    ("other code of std", re.compile(r"^std::|<std::")),
)
#: The bucket of every name no pattern of BUCKETS matches.
OTHER_BUCKET = "the workspace, its dependencies and the rest"


def bucket_of(name: str) -> str:
    """Return the bucket of BUCKETS whose pattern the demangled name `name` matches first."""
    for label, pattern in BUCKETS:
        if pattern.search(name):
            return label
    return OTHER_BUCKET


def _buckets(code, data) -> dict:
    """Sum the code pairs `code` and the data pairs `data` of name and size by bucket."""
    table = {label: [0, 0, 0] for label in [label for label, _ in BUCKETS] + [OTHER_BUCKET]}
    for name, size in code:
        table[bucket_of(name)][0] += size
        table[bucket_of(name)][1] += 1
    for name, size in data:
        table[bucket_of(name)][2] += size
    return {
        "code": sum(size for _, size in code),
        "data": sum(size for _, size in data),
        "buckets": [[label, *values] for label, values in table.items()],
    }


def elf_buckets(path) -> dict:
    """Bucket the named code and data of the ELF file at `path` by the fixed costs Rust adds over C.

    The port of the driver buckets.py of the facts with no LLVM tool: every
    defined symbol with a size counts once, the largest at its address, as code
    in an executable section and as data in any other, in the first bucket
    whose pattern its demangled name matches. The file keeps its symbol table,
    a build with `strip = "none"`.
    """
    elf = Elf.read(path)
    assert elf.section(".symtab") is not None, "%s carries no symbol table" % Path(path).name
    best = {}
    for symbol in elf.symbols():
        if symbol.size <= 0:
            continue
        key = (symbol.section, symbol.addr)
        if key not in best or symbol.size > best[key].size:
            best[key] = symbol
    executable = {section.name for section in elf.sections if section.flags & SHF_EXECINSTR}
    code = [(symbol.name, symbol.size) for symbol in best.values() if symbol.section in executable]
    data = [(symbol.name, symbol.size) for symbol in best.values() if symbol.section not in executable]
    return _buckets(code, data)


def wasm_buckets(path) -> dict:
    """Bucket the code of the wasm module at `path` by the fixed costs Rust adds over C, by its name section.

    A function counts with the bytes of its body, and the data section, whose
    segments carry no name in a linked module, counts as one sum beside the
    buckets.
    """
    module = Wasm.read(path)
    assert module.names, "%s carries no name section" % Path(path).name
    code = [(demangle(module.names.get(body.index, "?")), body.size) for body in module.bodies]
    result = _buckets(code, [])
    result["data"] = module.payload[11]
    return result


# The variants of a card, the port of the driver variant.sh of the facts.


#: The linker script of the lean link of aosabi-build, which a notebook gives a
#: link itself where the build script of a tree does not act, as the part x86 of
#: 17_framework_size does for a target before its lean link takes it.
DISCARD_UNWIND = ("SECTIONS {\n  /DISCARD/ : { *(.eh_frame) *(.eh_frame_hdr) *(.gcc_except_table*) }\n}\n"
                  "INSERT AFTER .text;\n")
#: The words of a failed build that a download over the network explains, so a later try may pass.
NETWORK = re.compile(r"download|network|spurious|timed out|failed to get|offline", re.IGNORECASE)


def cargo_home() -> Path:
    """Return the cargo home, whose registry and proxies every build reads."""
    return Path(os.environ.get("CARGO_HOME") or Path.home() / ".cargo")


def placeless(text, places) -> str:
    """Return `text` with each path of the mapping `places` written as its name."""
    text = str(text)
    for path, name in sorted(places.items(), key=lambda item: -len(str(item[0]))):
        text = text.replace(str(path), name)
    return text


#: The root of the tree this harness belongs to.
HARNESS_ROOT = Path(__file__).resolve().parents[2]


def variant(name, *, root, work, manifest, package, target, profile, artifact, options=None, card=None,
            link="plain", strip="symbols", features=(), config=(), flags=(), env=None, toolchain_root=None,
            timeout_s=1800.0, tries=3) -> dict:
    """Build one variant of a card into a target directory of its own and return what it built.

    The build runs the cargo of the rustup home with `--locked` under
    RUSTUP_TOOLCHAIN, the pin for a set of `options` that needs no unstable flag
    and the private copy of the pin with rust-src, or `toolchain_root`, for one
    that does, with `RUSTC_BOOTSTRAP=1` in the environment of that call alone. A
    tree without a copy of its own, such as a commit the ledger extracts, builds
    on the copy of the tree this harness belongs to. Its RUSTFLAGS remap the
    root of the tree, the cargo home and the source of the standard library, the
    last after the tree, since the copy lies inside it, unless the set takes
    `-Z trim-paths` in their place. For `aarch64-unknown-linux-gnu`, `link` is
    `lean` for the lean link of `card` and `plain` for the plain link. A lean
    build passes `-C panic=abort`, which the lean link reads from
    CARGO_CFG_PANIC, and a lean build with the immediate abort passes its own
    strategy, which the lean link reads as well. `strip` is the strip of
    `profile` passed with `--config`, None for the profile's own. The artifact
    is `artifact` under `<target>/<profile>/` of the target directory
    `work/name`. A build that fails over the network is tried again online up to
    `tries` times, and a failure returns its error text with no path of the
    machine.
    """
    options = options or tc.STOCK
    root, work = Path(root), Path(work)
    out = work / name
    chosen = toolchain_root or ((tc.private_copy(root) or tc.private_copy(HARNESS_ROOT)) if options.bootstrap
                                else tc.PIN)
    if chosen is None:
        return {"name": name, "options": options.name, "ok": False,
                "error": "no private copy of the pin with rust-src, which tools/size_toolchain.py makes"}
    sysroot = tc.sysroot(chosen)
    places = {root: "<tree>", out: "<build>", sysroot: "<toolchain>", cargo_home(): "<cargo>", Path.home(): "~"}
    rustflags = []
    if not options.trim_paths:
        rustflags += ["--remap-path-prefix=%s=aosabi" % root, "--remap-path-prefix=%s=cargo" % cargo_home(),
                      tc.source_remap(sysroot)]
    full = {"NO_COLOR": "1", "RUSTUP_TOOLCHAIN": str(chosen), "AOSABI_ARTIFACT_DIR": str(out / "aosabi"),
            "RUSTC_WRAPPER": None, "CARGO_ENCODED_RUSTFLAGS": None}
    if target == tc.HOST:
        if link == "lean":
            full["AOSABI_LINUX_CARD"] = card
            if not options.immediate_abort:
                rustflags += ["-C", "panic=abort"]
        elif card == "server":
            rustflags.append("-Clink-arg=-Wl,-z,pack-relative-relocs")
        rustflags += options.link_flags(sysroot)
    rustflags += options.rustflags()
    rustflags += list(flags)
    full["RUSTFLAGS"] = " ".join(rustflags)
    if options.bootstrap:
        full.update(tc.BOOTSTRAP)
    if options.per_crate:
        full["RUSTC_WRAPPER"] = str(tc.per_crate_wrapper(out / "rustc-z24.sh", tc.framework_crates(root)))
    full.update(env or {})
    std_names = tc.std_packages(sysroot) if options.std_level is not None else []
    command = [tc.rustup_tool("cargo"), "build", "--locked", *options.cargo_args(), "--manifest-path", Path(manifest),
               "-p", package, "--target", target, "--profile", profile, "--target-dir", out]
    if strip is not None:
        command += ["--config", 'profile.%s.strip="%s"' % (profile, strip)]
    command += options.config(profile, std_names) + list(config)
    if features:
        command += ["--features", ",".join(features)]
    started = time.monotonic()
    done = None
    for attempt in range(tries):
        argv = command[:3] + (["--offline"] if attempt == 0 else []) + command[3:]
        done = run(argv, cwd=root, env=full, timeout_s=timeout_s, check=False)
        if done.returncode == 0 or not NETWORK.search(done.err):
            break
        time.sleep(5 * (attempt + 1))
    path = out / target / profile / artifact
    result = {
        "name": name,
        "options": options.name,
        "toolchain": "the pin" if chosen == tc.PIN else "the private copy of the pin",
        "bootstrap": options.bootstrap,
        "seconds": round(time.monotonic() - started, 1),
        "ok": done.returncode == 0 and path.is_file(),
        "path": str(path),
        "command": [placeless(part, places) for part in command],
        "rustflags": placeless(full["RUSTFLAGS"], places),
    }
    if not result["ok"]:
        result["error"] = placeless((done.err or done.out).strip()[-2000:], places)
    return result


# The data of a wasm module by the code that names it.


def wasm_data_items(module: Wasm, groups) -> dict:
    """Attribute the active data segments of `module` to the functions whose code names their addresses.

    An address that an `i32.const` or the offset of a load or a store names
    inside an active segment opens an item, which runs to the next such
    address or to the end of its segment, and an item belongs to the functions
    that name its start. `groups` maps a label to a test of a demangled name,
    and each group gets the items and the bytes that only its functions name,
    as `elf_families` gives a group the read only data only its functions
    refer to. A table the code reaches through a pointer stored in data opens
    no item of its own and counts with the item before it.
    """
    ranges = sorted((start, start + length) for start, length in module.data_ranges if start is not None)
    starts_of = [start for start, _ in ranges]
    names = {index: demangle(raw) for index, raw in module.names.items()}

    def segment_end(address):
        index = bisect.bisect_right(starts_of, address) - 1
        if index >= 0 and ranges[index][0] <= address < ranges[index][1]:
            return ranges[index][1]
        return None

    readers = defaultdict(set)
    for body in module.bodies:
        _, _, addresses = module.operands(body)
        for address in addresses:
            if segment_end(address) is not None:
                readers[address].add(body.index)
    starts = sorted(readers)
    items = []
    for index, start in enumerate(starts):
        end = segment_end(start)
        if index + 1 < len(starts) and starts[index + 1] < end:
            end = starts[index + 1]
        items.append((start, end - start, readers[start]))
    result = {"segments": sum(end - start for start, end in ranges), "named by code": sum(size for _, size, _ in items),
              "items": len(items), "groups": {}}
    for label, match in groups:
        own = [(start, size) for start, size, functions in items
               if functions and all(match(names.get(function, "?")) for function in functions)]
        result["groups"][label] = {"items": len(own), "bytes": sum(size for _, size in own),
                                   "largest": max((size for _, size in own), default=0)}
    return result


def wasm_target_features(module: Wasm) -> list[str]:
    """Return the target features of the custom section `target_features` of `module`, each with its prefix.

    The section is a vector of a prefix byte, `+` for a feature the module uses,
    `-` for one it must not meet and `=` for one it needs, and a name.
    """
    payload = module.custom.get("target_features")
    if payload is None:
        return []
    count, at = leb128(payload, 0)
    found = []
    for _ in range(count):
        prefix = chr(payload[at])
        name, at = wasm_name(payload, at + 1)
        found.append(prefix + name)
    return found



#: The named families of Z0, each as the count of its instances and their bytes.
Z0_FAMILIES = (
    ("GraphQLError::new", lambda name: generic_family(name) == "<aosabi_lang::error::GraphQLError>::new"),
    ("enter_for_kind", lambda name: generic_family(name) == "aosabi_lang::visitor::enter_for_kind"),
    ("leave_for_kind", lambda name: generic_family(name) == "aosabi_lang::visitor::leave_for_kind"),
    ("OnceBox::init", lambda name: generic_family(name) == "<once_cell::race::once_box::OnceBox>::init"),
    ("serve_stream::{closure#0}", lambda name: generic_family(name).endswith("serve_stream::{closure#0}")),
    ("the instances of VisitWithTypeInfo", lambda name: "VisitWithTypeInfo" in name),
    ("the instances of StopGuard", lambda name: "StopGuard" in name),
    ("the instances of ParallelVisitor", lambda name: "ParallelVisitor" in name),
)
#: The crates of the symbolizer of std, whose instances of the stable sort the standard library makes.
SYMBOLIZER = re.compile(r"(^|[<\s(&])(addr2line|gimli|object|miniz_oxide|rustc_demangle)::|backtrace")
#: The candidates of the owner's note N2 that the host of a Worker holds, each
#: as a pattern of a demangled name.
HOST_CANDIDATES = (
    ("the number parse, dec2flt of core and the number readers of the workspace",
     re.compile(r"core::num::dec2flt|<f(32|64) as core::str::traits::FromStr>|aosabi_json::reader::\w*num"
                r"|aosabi_json::number::parse")),
    ("the number formatting, the writer of aosabi-json and the integers of core::fmt",
     re.compile(r"aosabi_json::number::|core::fmt::num")),
    ("the case conversion of Unicode", re.compile(r"(<str>::to_(lower|upper)case|alloc::str::<impl str>::to_"
                                                  r"(lower|upper)case|core::unicode::unicode_data::conversions::)")),
    ("normalization", re.compile(r"normaliz|::nfc|::nfd")),
    ("date and Intl formatting", re.compile(r"aosabi_core::scalars::date|aosabi_scalars::date_time|::date::"
                                            r"|iso_string|to_iso|Intl|DateTime")),
    ("new URL, aosabi_http::fetch::url", re.compile(r"aosabi_http::fetch::url")),
    ("the conversions between UTF-8 and UTF-16", re.compile(r"utf16|utf_16|Utf16|EncodeUtf16|DecodeUtf16")),
    ("the crypto of M7b, ring, rsa, sha2 and the signer of aosabi-auth-zitadel",
     re.compile(r"(^|[<\s(&])(ring|rsa|sha2|hmac|untrusted)::|aosabi_auth_zitadel::crypto")),
)


def z0_families(pairs, top: int = 30) -> dict:
    """Return the families of Z0 over the pairs `pairs` of a demangled name and a size.

    The thirty largest functions, the closures by the function they belong
    to, the closures of aosabi-execute by size, among them the bodies of its
    async fns, which a v0 name writes as a closure of the fn, the instances of
    the stable sort of core split into those made for a type of a workspace
    crate, those the standard library makes for its symbolizer and the rest,
    and the named families of Z0_FAMILIES.
    """
    pairs = sorted(pairs, key=lambda pair: (-pair[1], pair[0]))
    parents = defaultdict(lambda: [0, 0])
    execute = []
    split = {"made for a workspace type": [0, 0], "made for the symbolizer of std": [0, 0], "other": [0, 0]}
    named = {label: [0, 0] for label, _ in Z0_FAMILIES}
    for name, size in pairs:
        family = generic_family(name)
        if "{closure#" in family:
            parent = family.split("::{closure#", 1)[0]
            parents[parent][0] += 1
            parents[parent][1] += size
            if family.startswith("aosabi_execute::"):
                execute.append([family, size])
        if name.startswith("core::slice::sort::stable::"):
            key = ("made for the symbolizer of std" if SYMBOLIZER.search(name)
                   else "made for a workspace type" if WORKSPACE_CRATE.search(name) else "other")
            split[key][0] += 1
            split[key][1] += size
        for label, match in Z0_FAMILIES:
            if match(name):
                named[label][0] += 1
                named[label][1] += size
    return {
        "largest": [[name, size] for name, size in pairs[:top]],
        "closures by parent": sorted(([key, *value] for key, value in parents.items()), key=lambda row: -row[2])[:top],
        "closures": sum(value[0] for value in parents.values()),
        "closure bytes": sum(value[1] for value in parents.values()),
        "closures of aosabi-execute": execute,
        "stable sort": split,
        "named": named,
    }


def host_candidates(pairs) -> dict:
    """Return the functions and bytes of each candidate of HOST_CANDIDATES among the pairs `pairs`."""
    found = {label: [0, 0] for label, _ in HOST_CANDIDATES}
    for name, size in pairs:
        for label, pattern in HOST_CANDIDATES:
            if pattern.search(name):
                found[label][0] += 1
                found[label][1] += size
    return found


def named_pairs(path) -> list[tuple[str, int]]:
    """Return the demangled name and the size of every function of the ELF file or wasm module at `path`.

    An ELF file names each address once, by its largest symbol, and a wasm
    module names each body once.
    """
    data = Path(path).read_bytes()
    if data[:4] == b"\0asm":
        module = Wasm(data)
        return [(demangle(module.names.get(body.index, "?")), body.size) for body in module.bodies]
    elf = Elf(data)
    best = {}
    for symbol in elf.symbols():
        key = (symbol.section, symbol.addr)
        if symbol.size > 0 and (key not in best or symbol.size > best[key].size):
            best[key] = symbol
    executable = {section.name for section in elf.sections if section.flags & SHF_EXECINSTR}
    return [(symbol.name, symbol.size) for symbol in best.values() if symbol.section in executable]


def section_sizes(path) -> Counter:
    """Return the size of every section of the ELF file of 32 or 64 bits at `path`, by name."""
    sizes = Counter()
    for section in Elf.read(path).sections:
        sizes[section.name] += section.size
    return sizes


#: The section index of an absolute symbol.
SHN_ABS = 0xFFF1
#: The type of a section of relocations with addends.
SHT_RELA = 4
#: An assignment of a linker script, `name = 0x...;` with or without `PROVIDE( ... )` around it.
ROM_ASSIGNMENT = re.compile(r"^\s*(?:PROVIDE\s*\(\s*)?([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(0x[0-9A-Fa-f]+)\s*\)?\s*;")


def absolute_symbols(path) -> dict[str, int]:
    """Return the absolute symbols of the ELF file at `path` by name, such as the routines a ROM holds.

    A linker script that assigns a name an address, as the scripts of
    `esp-rom-sys` assign the routines of the ROM, defines it as an absolute
    symbol, which `Elf.symbols` leaves out with the other special sections.
    """
    elf = Elf.read(path)
    found = {}
    for table in elf.sections:
        if table.kind != SHT_SYMTAB:
            continue
        strings = elf.sections[table.link]
        size = 24 if elf.wide else 16
        for at in range(table.offset, table.offset + table.size, size):
            if elf.wide:
                name_off, info, _other, shndx, value, _length = struct.unpack_from("<IBBHQQ", elf.data, at)
            else:
                name_off, value, _length, info, _other, shndx = struct.unpack_from("<IIIBBH", elf.data, at)
            if shndx != SHN_ABS or info & 0xF in (STT_FILE, STT_SECTION):
                continue
            start = strings.offset + name_off
            name = elf.data[start : elf.data.index(b"\0", start)].decode("utf-8", "replace")
            if name:
                found[name] = value
    return found


def rom_script(path) -> dict[str, int]:
    """Return the names and the addresses a linker script of a ROM assigns.

    An assignment of one name to another, such as `PROVIDE ( strnlen =
    __strnlen )`, names no address and is left out.
    """
    found = {}
    for line in Path(path).read_text(encoding="utf-8", errors="replace").splitlines():
        match = ROM_ASSIGNMENT.match(line)
        if match:
            found[match.group(1)] = int(match.group(2), 16)
    return found


def _signed(value: int, bits: int) -> int:
    """Return the `bits` bit field `value` as a signed number."""
    return value - (1 << bits) if value & (1 << (bits - 1)) else value


def riscv_targets(path) -> Counter:
    """Count the direct calls and jumps of the code of a RISC-V image of 32 bits by the address they reach.

    The reader takes every executable section in order, a compressed instruction
    where the low two bits of a half word are not both set and an instruction of
    32 bits where they are, and takes `jal`, `c.jal` and `c.j`, whose target is
    the address of the instruction and its offset, and the pair `auipc` and
    `jalr` of the same register that `call` and `tail` write for a target beyond
    the reach of `jal`, as the ROM lies too far from the flash of the ESP32-C3,
    and the pair `lui` and `jalr` of an absolute address. An indirect call
    through a register no such pair sets names no target.
    """
    elf = Elf.read(path)
    assert not elf.wide, "a RISC-V image of 32 bits"
    found = Counter()
    for section in elf.sections:
        if not section.flags & SHF_EXECINSTR or section.kind == SHT_NOBITS or not section.size:
            continue
        code = elf.data[section.offset : section.offset + section.size]
        at, pending = 0, None
        while at + 2 <= len(code):
            pc = section.addr + at
            half = struct.unpack_from("<H", code, at)[0]
            if half & 3 != 3:
                op, funct3 = half & 3, half >> 13
                if op == 1 and funct3 in (1, 5):
                    imm = ((half >> 12 & 1) << 11 | (half >> 11 & 1) << 4 | (half >> 9 & 3) << 8
                           | (half >> 8 & 1) << 10 | (half >> 7 & 1) << 6 | (half >> 6 & 1) << 7
                           | (half >> 3 & 7) << 1 | (half >> 2 & 1) << 5)
                    found[pc + _signed(imm, 12)] += 1
                pending = None
                at += 2
                continue
            if at + 4 > len(code):
                break
            inst = struct.unpack_from("<I", code, at)[0]
            opcode, rd = inst & 0x7F, inst >> 7 & 0x1F
            if opcode == 0x6F:
                imm = ((inst >> 31 & 1) << 20 | (inst >> 12 & 0xFF) << 12 | (inst >> 20 & 1) << 11
                       | (inst >> 21 & 0x3FF) << 1)
                found[(pc + _signed(imm, 21)) & 0xFFFFFFFF] += 1
                pending = None
            elif opcode in (0x17, 0x37):
                upper = _signed(inst & 0xFFFFF000, 32)
                pending = (rd, (pc if opcode == 0x17 else 0) + upper)
            elif opcode == 0x67 and inst >> 12 & 7 == 0:
                rs1 = inst >> 15 & 0x1F
                if pending is not None and pending[0] == rs1:
                    found[(pending[1] + _signed(inst >> 20, 12)) & 0xFFFFFFFF] += 1
                pending = None
            else:
                pending = None
            at += 4
    return found


def _relocatable(member: bytes):
    """Read a relocatable ELF object of 32 bits: its sections, its symbols and the targets of its relocations.

    Each section is its name, type, flags, size, link and info, each symbol
    its name, binding, type and section, and each relocated section the set
    of the symbol indices its relocations name.
    """
    shoff = struct.unpack_from("<I", member, 0x20)[0]
    shentsize, shnum, shstrndx = struct.unpack_from("<HHH", member, 0x2E)
    raw = [struct.unpack_from("<IIIIIIIIII", member, shoff + index * shentsize) for index in range(shnum)]
    names_at = raw[shstrndx][4] if raw else 0

    def text(base, offset):
        start = base + offset
        return member[start : member.index(b"\0", start)].decode("utf-8", "replace")

    sections = [(text(names_at, entry[0]), entry[1], entry[2], entry[5], entry[6], entry[7]) for entry in raw]
    symbols, references = [], {}
    for index, (_name, kind, _flags, size, link, info) in enumerate(sections):
        offset = raw[index][4]
        if kind == SHT_SYMTAB:
            strings = raw[link][4]
            for at in range(offset, offset + size, 16):
                name_off, _value, _length, sym_info, _other, shndx = struct.unpack_from("<IIIBBH", member, at)
                symbols.append((text(strings, name_off), sym_info >> 4, sym_info & 0xF, shndx))
        elif kind == SHT_RELA:
            targets = references.setdefault(info, set())
            for at in range(offset, offset + size, 12):
                targets.add(struct.unpack_from("<I", member, at + 4)[0] >> 8)
    return sections, symbols, references


def archive_closure(rlib, roots) -> tuple[int, dict[str, int]]:
    """Return the bytes of code and read only data the objects of the archive `rlib` reach from `roots`.

    The reader starts at the section that defines each root, follows the
    relocations of every section it reaches to the sections they name, in the
    same object or, for a name the object leaves undefined, in the object of
    the archive that defines it globally, and sums each section of code or
    read only data once, so it prices what a link would take from the archive
    for those names. It returns the bytes of all roots together and of each
    root alone.
    """
    objects = [_relocatable(member) for _name, member in ar_members(Path(rlib).read_bytes())
               if member[:4] == b"\x7fELF" and member[4] == 1 and member[16] == 1]
    defined = {}
    for index, (_sections, symbols, _references) in enumerate(objects):
        for name, binding, _kind, shndx in symbols:
            if binding in (1, 2) and 0 < shndx < 0xFF00 and name not in defined:
                defined[name] = (index, shndx)

    def reach(start):
        seen, pending = set(), [start]
        while pending:
            place = pending.pop()
            if place in seen:
                continue
            seen.add(place)
            index, shndx = place
            sections, symbols, references = objects[index]
            for target in references.get(shndx, ()):
                if target >= len(symbols):
                    continue
                name, _binding, kind, target_shndx = symbols[target]
                if 0 < target_shndx < 0xFF00:
                    pending.append((index, target_shndx))
                elif target_shndx == 0 and kind != STT_SECTION and name in defined:
                    pending.append(defined[name])
        total = 0
        for index, shndx in seen:
            name, kind, _flags, size, _link, _info = objects[index][0][shndx]
            if kind != SHT_NOBITS and name.startswith((".text", ".rodata", ".srodata")):
                total += size
        return seen, total

    every, each = set(), {}
    for root in roots:
        if root not in defined:
            each[root] = 0
            continue
        seen, total = reach(defined[root])
        each[root] = total
        every |= seen
    total = 0
    for index, shndx in every:
        name, kind, _flags, size, _link, _info = objects[index][0][shndx]
        if kind != SHT_NOBITS and name.startswith((".text", ".rodata", ".srodata")):
            total += size
    return total, each


def undefined_names(rlib) -> set[str]:
    """Return the names the relocatable objects of 32 bits of the archive `rlib` use and leave undefined."""
    names = set()
    for _name, member in ar_members(Path(rlib).read_bytes()):
        if member[:4] != b"\x7fELF" or member[4] != 1 or member[16] != 1:
            continue
        _sections, symbols, _references = _relocatable(member)
        names.update(name for name, _binding, kind, shndx in symbols if shndx == 0 and name and kind != STT_SECTION)
    return names
