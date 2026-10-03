# SPDX-License-Identifier: MIT
#
# tools/unicode/idna_tables.py: writes the Unicode tables of domain to ASCII
# that the grown URL parser of the web layer of aosabi-http reads.
#
# The reference of the parser is new URL of Node 24.13.0, which runs domain to
# ASCII in ada 3.3.0 (process.versions.ada), in ada's own IDNA and not in ICU.
# So the tables are ada's: the generator reads them from src/ada_idna.cpp of
# the tag v3.3.0 of https://github.com/ada-url/ada and writes them in the layout
# fetch/url.rs reads. The data of ada is of more than one Unicode version: the
# mapping of UTS 46 is of IDNA 16.0.0 and the normalization of 15.0.0, as the
# comments of the file name them, and the list of marks, the bidirectional
# classes, the viramas and the joining types are older, so a code point the
# mapping holds valid can be missing from them, U+0897 among them. The tables
# carry each as ada holds it, and a code point missing from the bidirectional
# classes has none. The data of ada derives from the data files of the Unicode
# Consortium under the Unicode License V3, and ada is under the Apache License
# 2.0 or the MIT License, taken under MIT, both notices in NOTICE.
#
# Fetch the archive of the tag, check its SHA-256 and unpack it outside the
# tree, then run the generator with the Python of tests/.venv from the root of
# the repository and the path of src/ada_idna.cpp:
#
#   curl -sSfLO https://github.com/ada-url/ada/archive/refs/tags/v3.3.0.tar.gz
#   sha256sum v3.3.0.tar.gz    # 75565e2d4cc8e3ce2dd7927f5c75cc5ebbd3b620468cb0226501dae68d8fe1cd
#   tests/.venv/bin/python tools/unicode/idna_tables.py <dir>/ada-3.3.0/src/ada_idna.cpp
#
# The generator refuses a file whose SHA-256 is not the one of ada 3.3.0, so
# its input is pinned whatever the environment holds. It writes
# crates/aosabi-http/src/fetch/url_idna_tables.rs and prints the counts it
# wrote. A second run writes the same bytes.
#
# The tables take a dense form. The mapping of UTS 46 merges adjacent ranges
# that follow one rule: a run of single code points each mapped to the code
# point at a fixed distance becomes one range whose mapping moves with the
# code point, and a run that maps every code point at an even distance from
# its first to the next code point and keeps every other valid becomes one
# range of that alternation. The first code points of the ranges are u16
# distances from the range before, with the u32 first code point of every
# 256th range as an anchor, so a lookup searches the anchors and walks at most
# 256 distances. The decompositions and the compositions are u16 within their
# plane of 65,536 code points, with the first index of each plane beside them,
# and a decomposition ends where the next begins in its pool. Before it writes,
# the generator reads the dense mapping back for every code point and refuses
# to write a table whose status or mapping differs from ada's for any of them.

import hashlib
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / "crates" / "aosabi-http" / "src" / "fetch" / "url_idna_tables.rs"
ADA_VERSION = "3.3.0"
ADA_IDNA_SHA256 = "7b9ae87da5f46107168117176046df209de541f4a17a410a5b3dd733274ed9c1"
MAX = 0x110000
# The rule of a range of the dense mapping: valid, mapped through the pool,
# mapped to one code point that moves with the code point, ignored,
# disallowed, and the alternation of a mapping to the next code point and a
# valid code point.
RULE = {"V": 0, "M": 1, "D": 2, "I": 3, "X": 4, "A": 5}
# The ranges between two anchors, and the most code points one range spans,
# the largest distance a u16 holds.
BLOCK = 256
SPAN = 0xFFFF
PLANE = 0x10000
BIDI_CODES = {"L": 1, "R": 2, "AL": 3, "AN": 4, "EN": 5, "ES": 6, "CS": 7, "ET": 8, "ON": 9, "BN": 10, "NSM": 11}
HANGUL_S_BASE = 0xAC00
HANGUL_S_COUNT = 19 * 21 * 28


class Ada:
    """The arrays of src/ada_idna.cpp, read by their names."""

    def __init__(self, path):
        data = Path(path).read_bytes()
        digest = hashlib.sha256(data).hexdigest()
        if digest != ADA_IDNA_SHA256:
            sys.exit("idna_tables.py: %s has SHA-256 %s, not the one of ada %s" % (path, digest, ADA_VERSION))
        self.text = data.decode("utf-8")

    def body(self, name):
        """Return the braced initializer of the array called name."""
        found = re.search(r"\b" + re.escape(name) + r"(\[[^\]]*\])*\s*=\s*\{", self.text)
        if not found:
            sys.exit("idna_tables.py: ada_idna.cpp holds no array %s" % name)
        start = found.end() - 1
        depth = 0
        for at in range(start, len(self.text)):
            if self.text[at] == "{":
                depth += 1
            elif self.text[at] == "}":
                depth -= 1
                if depth == 0:
                    return self.text[start:at + 1]
        sys.exit("idna_tables.py: the array %s of ada_idna.cpp does not end" % name)

    def numbers(self, name):
        """Return the numbers of the array called name, flattened."""
        return [int(x, 0) for x in re.findall(r"0[xX][0-9a-fA-F]+|\d+", self.body(name))]

    def sorted_set(self, name):
        """Return the members of a sorted list that ada searches by bisection."""
        values = self.numbers(name)
        assert values == sorted(set(values)), name
        return values


def ranges_of(members):
    """Return the runs of consecutive code points of a sorted list as pairs."""
    out = []
    for cp in members:
        if out and out[-1][1] == cp - 1:
            out[-1] = (out[-1][0], cp)
        else:
            out.append((cp, cp))
    return out


def pool_offsets(strings):
    """Return the pool of the strings and the offset of each in it."""
    pool = bytearray()
    offsets = {}
    for text in strings:
        if text in offsets:
            continue
        data = text.encode("utf-8")
        found = pool.find(data)
        if found < 0:
            found = len(pool)
            pool.extend(data)
        offsets[text] = found
    return bytes(pool), offsets


def mapping_ranges(ada):
    """Return the ranges of the mapping as the first code point, the status and the mapping."""
    mappings = ada.numbers("mappings")
    flat = ada.numbers("table")
    table = list(zip(flat[0::2], flat[1::2]))
    assert [start for start, _ in table] == sorted({start for start, _ in table}) and table[0][0] == 0
    ranges = []
    for start, descriptor in table:
        code = descriptor & 0xFF
        if code == 0:
            entry = ("I", None)
        elif code == 1:
            entry = ("V", None)
        elif code == 2:
            entry = ("X", None)
        else:
            count = descriptor >> 24
            index = (descriptor >> 8) & 0xFFFF
            entry = ("M", "".join(chr(cp) for cp in mappings[index:index + count]))
        if not ranges or ranges[-1][1] != entry:
            ranges.append((start, entry))
    return ranges


def normalization(ada):
    """Return the combining classes, the decompositions and the compositions of ada."""
    decomposition_index = ada.numbers("decomposition_index")
    decomposition_block = ada.numbers("decomposition_block")
    decomposition_data = ada.numbers("decomposition_data")
    ccc_index = ada.numbers("canonical_combining_class_index")
    ccc_block = ada.numbers("canonical_combining_class_block")
    composition_index = ada.numbers("composition_index")
    composition_block = ada.numbers("composition_block")
    composition_data = ada.numbers("composition_data")

    def ccc(cp):
        return ccc_block[ccc_index[cp >> 8] * 256 + cp % 256]

    def decomposition(cp):
        at = decomposition_index[cp >> 8] * 257 + cp % 256
        first, second = decomposition_block[at], decomposition_block[at + 1]
        length = (second >> 2) - (first >> 2)
        if length > 0 and first & 1:
            length = 0
        return decomposition_data[(first >> 2):(first >> 2) + length]

    classes = []
    decompositions = []
    compositions = []
    for cp in range(MAX):
        klass = ccc(cp)
        if klass:
            if classes and classes[-1][1] == cp - 1 and classes[-1][2] == klass:
                classes[-1] = (classes[-1][0], cp, klass)
            else:
                classes.append((cp, cp, klass))
        if HANGUL_S_BASE <= cp < HANGUL_S_BASE + HANGUL_S_COUNT:
            continue
        parts = decomposition(cp)
        if parts:
            decompositions.append((cp, "".join(map(chr, parts))))
        at = composition_index[cp >> 8] * 257 + cp % 256
        left, right = composition_block[at], composition_block[at + 1]
        pairs = composition_data[left:right]
        seconds = pairs[0::2]
        assert seconds == sorted(set(seconds)), hex(cp)
        if pairs:
            assert ccc(cp) == 0, hex(cp)
        for second, composite in zip(pairs[0::2], pairs[1::2]):
            compositions.append((cp, second, composite))
    compositions.sort()
    return classes, decompositions, compositions


def bidi_ranges(ada):
    """Return the ranges of the bidirectional classes the rule of ada reads."""
    entries = re.findall(r"\{\s*(0x[0-9a-fA-F]+),\s*(0x[0-9a-fA-F]+),\s*direction::(\w+)\s*\}", ada.body("dir_table"))
    out = []
    last = -1
    for first, final, name in entries:
        first, final = int(first, 16), int(final, 16)
        assert last < first <= final
        last = final
        code = BIDI_CODES.get(name)
        if code is None:
            continue
        if out and out[-1][1] == first - 1 and out[-1][2] == code:
            out[-1] = (out[-1][0], final, code)
        else:
            out.append((first, final, code))
    return out


def joining_ranges(ada):
    """Return the ranges of the joining types L, D and R of the check of the joiners of ada."""
    members = []
    for name, code in (("L", 1), ("D", 2), ("R", 3)):
        members.extend((cp, code) for cp in ada.sorted_set(name))
    members.sort()
    assert len({cp for cp, _ in members}) == len(members)
    out = []
    for cp, code in members:
        if out and out[-1][1] == cp - 1 and out[-1][2] == code:
            out[-1] = (out[-1][0], cp, code)
        else:
            out.append((cp, cp, code))
    return out


def single(start, end, entry):
    """Return the code point a range of one code point maps to alone, or None."""
    status, mapping = entry
    if status == "M" and end - start == 1 and len(mapping) == 1:
        return ord(mapping)
    return None


def dense_ranges(ranges):
    """Return the ranges of the mapping merged by their rules, each as the first code point and the entry.

    An entry is the rule and its datum: the mapping text for M, the code point
    the first code point maps to for D, and None for the others. The pass of
    the alternation runs first, the pass of the moving mapping second, and a
    range longer than SPAN code points is split, so every distance between two
    first code points fits a u16.
    """
    starts = [start for start, _ in ranges] + [MAX]
    segments = [(start, starts[i + 1], entry) for i, (start, entry) in enumerate(ranges)]
    alternated = []
    k = 0
    while k < len(segments):
        j = k
        while j < len(segments):
            start, end, entry = segments[j]
            even = (j - k) % 2 == 0
            if start != segments[k][0] + (j - k) or end - start != 1:
                break
            if even and single(start, end, entry) != start + 1:
                break
            if not even and entry != ("V", None):
                break
            j += 1
        if j - k >= 2:
            alternated.append((segments[k][0], segments[j - 1][1], ("A", None)))
            k = j
        else:
            alternated.append(segments[k])
            k += 1
    moved = []
    for start, end, entry in alternated:
        target = single(start, end, entry)
        if target is not None:
            last = moved[-1] if moved else None
            if (last and last[2][0] == "D" and last[1] == start
                    and last[2][1] + (start - last[0]) == target):
                moved[-1] = (last[0], end, last[2])
            else:
                moved.append((start, end, ("D", target)))
        elif moved and moved[-1][2] == entry and moved[-1][1] == start:
            moved[-1] = (moved[-1][0], end, entry)
        else:
            moved.append((start, end, entry))
    out = []
    for start, end, (rule, datum) in moved:
        at = start
        while at < end:
            stop = min(end, at + SPAN)
            if rule == "A" and stop < end:
                stop -= (stop - start) % 2
            out.append((at, (rule, datum + (at - start) if rule == "D" else datum)))
            at = stop
    return out


def read_back(dense, cp_ranges):
    """Refuse a dense mapping whose status or mapping differs from ada's for any code point.

    `cp_ranges` are the ranges of ada as mapping_ranges returns them. Each
    code point is read through the rule of its dense range, as the module
    idna of fetch/url.rs reads it.
    """
    old = [start for start, _ in cp_ranges] + [MAX]
    new = [start for start, _ in dense] + [MAX]
    i = j = 0
    for cp in range(MAX):
        while old[i + 1] <= cp:
            i += 1
        while new[j + 1] <= cp:
            j += 1
        status, mapping = cp_ranges[i][1]
        rule, datum = dense[j][1]
        offset = cp - dense[j][0]
        if rule == "D":
            got = ("M", chr(datum + offset))
        elif rule == "A":
            got = ("M", chr(cp + 1)) if offset % 2 == 0 else ("V", None)
        else:
            got = (rule, datum)
        if got != (status, mapping):
            sys.exit("idna_tables.py: the dense mapping reads %r for U+%04X where ada holds %r" % (
                got, cp, (status, mapping)))


def by_plane(entries, plane_of):
    """Return the first index of each plane in `entries`, sorted by plane, and the count after the last plane."""
    planes = max(plane_of(entry) for entry in entries) + 1
    firsts = [0] * (planes + 1)
    for entry in entries:
        firsts[plane_of(entry) + 1] += 1
    for plane in range(planes):
        firsts[plane + 1] += firsts[plane]
    return firsts


def main():
    if len(sys.argv) != 2:
        sys.exit("usage: idna_tables.py <path of src/ada_idna.cpp of ada %s>" % ADA_VERSION)
    ada = Ada(sys.argv[1])

    ranges = mapping_ranges(ada)
    dense = dense_ranges(ranges)
    read_back(dense, ranges)
    pool, offsets = pool_offsets(datum for _, (rule, datum) in dense if rule == "M")
    anchors = []
    deltas = []
    values = []
    for i, (start, (rule, datum)) in enumerate(dense):
        if i % BLOCK == 0:
            anchors.append(start)
            deltas.append(0)
        else:
            delta = start - dense[i - 1][0]
            assert 0 < delta <= SPAN
            deltas.append(delta)
        value = RULE[rule] << 28
        if rule == "M":
            data = datum.encode("utf-8")
            assert len(data) < 256 and offsets[datum] < (1 << 20)
            value |= offsets[datum] << 8 | len(data)
        elif rule == "D":
            assert datum < MAX
            value |= datum
        values.append(value)

    classes, decompositions, compositions = normalization(ada)
    decomposition_planes = by_plane(decompositions, lambda entry: entry[0] >> 16)
    decomposition_pool = "".join(text for _, text in decompositions).encode("utf-8")
    at = 0
    decomposition_offsets = []
    for _, text in decompositions:
        decomposition_offsets.append(at)
        at += len(text.encode("utf-8"))
    decomposition_offsets.append(at)
    assert at == len(decomposition_pool) and at <= 0xFFFF
    for a, b, composite in compositions:
        assert a >> 16 == b >> 16 == composite >> 16, (hex(a), hex(b), hex(composite))
    composition_planes = by_plane(compositions, lambda entry: entry[0] >> 16)
    marks = ranges_of(ada.sorted_set("combining"))
    viramas = ada.sorted_set("virama")
    bidi = bidi_ranges(ada)
    joining = joining_ranges(ada)

    lines = []
    emit = lines.append
    emit("// SPDX-License-Identifier: MIT")
    emit("/*")
    emit(" * aosabi-http: the Unicode tables of domain to ASCII, written by")
    emit(" * tools/unicode/idna_tables.py and by nothing else.")
    emit(" *")
    emit(" * Derived from src/ada_idna.cpp of ada %s, the IDNA of new URL of Node" % ADA_VERSION)
    emit(" * 24.13.0, whose SHA-256 is")
    emit(" * %s: the" % ADA_IDNA_SHA256)
    emit(" * mapping of UTS 46 of IDNA 16.0.0, the normalization of 15.0.0, and ada's")
    emit(" * lists of marks, bidirectional classes, viramas and joining types, each as")
    emit(" * ada holds it. The data derives from the data files of the Unicode")
    emit(" * Consortium under the Unicode License V3, and ada is taken under the MIT")
    emit(" * License of its two, both notices in NOTICE.")
    emit(" */")
    emit("")
    emit("// The file is included by fetch/url.rs behind the feature node-server.")
    emit("")

    def array(name, doc, kind, items):
        emit("/// %s" % doc)
        emit("static %s: [%s; %d] = [" % (name, kind, len(items)))
        row = []
        for item in items:
            row.append("{:_},".format(item))
            if len(row) == 12:
                emit("    " + " ".join(row))
                row = []
        if row:
            emit("    " + " ".join(row))
        emit("];")
        emit("")

    def pool_const(name, doc, data):
        emit("/// %s" % doc)
        text = data.decode("utf-8")
        emit("static %s: &str = \"%s\";" % (name, "".join(
            c if 0x20 <= ord(c) < 0x7F and c not in "\"\\" else "\\u{%x}" % ord(c) for c in text)))
        emit("")

    array("IDNA_ANCHORS", "The first code point of every %dth range of the mapping of UTS 46, from the first range on, where a walk over the distances starts." % BLOCK, "u32", anchors)
    array("IDNA_DELTAS", "The distance of the first code point of each range from that of the range before it, 0 for a range whose first code point is an anchor.", "u16", deltas)
    array("IDNA_VALUES", "The rule of each range in bits 28 to 31, 0 valid, 1 mapped through the pool, 2 mapped to one code point that moves with the code point, 3 ignored, 4 disallowed, and 5 mapped to the next code point at an even distance from the first code point of the range and valid at an odd one. Rule 1 holds the offset of its mapping in the pool in bits 8 to 27 and its length in bytes in bits 0 to 7, and rule 2 the code point the first code point of the range maps to in bits 0 to 20.", "u32", values)
    pool_const("IDNA_POOL", "The mappings of the ranges of rule 1, one after the other.", pool)
    array("CCC", "The ranges of a canonical combining class other than 0, as the first code point, the last and the class.", "u32", [x for r in classes for x in r])
    array("DECOMPOSITION_PLANES", "The index in `DECOMPOSITION_KEYS` of the first key of each plane of 65,536 code points, and the count of the keys after the last plane.", "u16", decomposition_planes)
    array("DECOMPOSITION_KEYS", "The code points with a canonical decomposition, Hangul syllables apart, each as its low 16 bits, in order within its plane.", "u16", [cp & 0xFFFF for cp, _ in decompositions])
    array("DECOMPOSITION_OFFSETS", "The offset in the pool of the full canonical decomposition of each key, and the length of the pool after the last, so a decomposition ends where the next begins.", "u16", decomposition_offsets)
    pool_const("DECOMPOSITION_POOL", "The full canonical decompositions in the order of their keys, one after the other.", decomposition_pool)
    array("COMPOSITION_PLANES", "The index in `COMPOSITIONS` of the first composite of each plane of 65,536 code points, counted in composites of three entries, and the count after the last plane. The two code points and the composite of a composition lie in one plane.", "u16", composition_planes)
    array("COMPOSITIONS", "The primary composites as the first code point, the second and the composite, each as its low 16 bits, in order within their plane.", "u16", [x & 0xFFFF for c in compositions for x in c])
    array("MARKS", "The ranges of the marks a label must not start with, as the first code point and the last.", "u32", [x for r in marks for x in r])
    array("VIRAMAS", "The viramas a joiner may follow, in order.", "u32", viramas)
    array("BIDI", "The ranges of the bidirectional classes the rule reads, as the first code point, the last and the class: 1 L, 2 R, 3 AL, 4 AN, 5 EN, 6 ES, 7 CS, 8 ET, 9 ON, 10 BN, 11 NSM. A code point outside them has none of these classes.", "u32", [x for r in bidi for x in r])
    array("JOINING", "The ranges of a joining type, as the first code point, the last and the type: 1 L, 2 D, 3 R.", "u32", [x for r in joining for x in r])

    TARGET.write_text("\n".join(lines).rstrip("\n") + "\n", encoding="utf-8")
    print("ada %s, ada_idna.cpp %s" % (ADA_VERSION, ADA_IDNA_SHA256))
    print("mapping ranges %d of ada's %d, anchors %d, pool %d bytes, ccc ranges %d, decompositions %d with a pool of %d bytes, compositions %d, marks %d, viramas %d, bidi %d, joining %d" % (
        len(dense), len(ranges), len(anchors), len(pool), len(classes), len(decompositions), len(decomposition_pool), len(compositions), len(marks), len(viramas), len(bidi), len(joining)))
    print("mapping ranges by rule: %s" % ", ".join("%s %d" % (rule, sum(1 for _, (r, _) in dense if r == rule)) for rule in RULE))
    print("wrote", TARGET.relative_to(ROOT), TARGET.stat().st_size, "bytes")


if __name__ == "__main__":
    main()
