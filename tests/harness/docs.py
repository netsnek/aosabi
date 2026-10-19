# SPDX-License-Identifier: MIT
"""Aosabi harness: the comparison of a generated tree, the resolver of its
links and anchors, and its size rules.

The documentation notebooks compare a tree file by file against its golden and
against a second run, resolve every relative link and every anchor of every
Markdown file, and allow no file over 256 kB and the index under 64 kB.

A tree is read as a map from its POSIX relative paths to bytes, so the three
comparisons of a notebook, against the golden, against a second process and
against the other architecture, are one function over two maps.
"""

from __future__ import annotations

import difflib
import hashlib
import html.entities
import json
import re
import unicodedata
import urllib.parse
from dataclasses import dataclass, field
from pathlib import Path

from .diffs import first_difference

# The size rules. A kB is 1000 bytes, the unit the design uses where it writes
# the 4572 and 4556 bytes of the two faces as 9.1 kB.
MAX_FILE_BYTES = 256 * 1000
MAX_INDEX_BYTES = 64 * 1000

# The index of a tree: README.md under the split layout and API.md under the
# single file layout.
INDEX_FILES = ("README.md", "API.md")

# The file a golden directory holds beside the tree, with the generator, the
# toolchain, the options and the digest of the input, which the generated tree
# never holds.
GOLDEN_MANIFEST = "golden-manifest.json"

# The PDF the command writes beside the tree when `pdf` is among the formats.
PDF_FILE = "api.pdf"

# The version of `manifest.json`.
MANIFEST_VERSION = 1

# The files a tree may hold. The largest measured schema writes about 145
# diagrams per theme and one page per type, and the 10,000 type shape stays
# below this cap with one page and at most one diagram per theme for each type,
# about 30,000 files.
MAX_TREE_FILES = 200_000

# The lines of a unified difference a notebook prints for one text file.
MAX_DIFF_LINES = 80

# The schemes a link the tool writes may carry.
ALLOWED_SCHEMES = ("http", "https", "mailto")

SCHEME = re.compile(r"^([A-Za-z][A-Za-z0-9+.-]*):")

# A fence of three or more backticks or tildes opens a block that holds no link.
FENCE = re.compile(r"^ {0,3}(`{3,}|~{3,})")

# The characters one Markdown file may hold before the scan of its code spans
# refuses it, four times the largest file the size rule allows.
MAX_MARKDOWN_CHARS = 4 * MAX_FILE_BYTES

# The destination of an inline link or image, bare or in angle brackets, with an
# optional title.
INLINE_LINK = re.compile(r"\]\(\s*(<[^>\n]*>|[^\s()]*(?:\([^\s()]*\)[^\s()]*)*)(?:\s+\"[^\"]*\")?\s*\)")

# A link reference definition, whose destination is a link of the file as an
# inline link's is.
REFERENCE_DEFINITION = re.compile(r"^ {0,3}\[[^\]\n]+\]:[ \t]*(<[^>\n]*>|\S+)", re.MULTILINE)

# An autolink, a scheme of 2 to 32 characters, a colon and no space or angle
# bracket. It always leaves the tree, so its scheme is what step 20 of 75_docgen
# judges.
AUTOLINK = re.compile(r"<([A-Za-z][A-Za-z0-9+.-]{1,31}:[^\s<>]*)>")

# The attributes of the raw HTML the generator writes that carry a target: the
# anchors of every link, the picture element of every diagram and its source.
HTML_TARGET = re.compile(r"\b(href|src|srcset)\s*=\s*\"([^\"]*)\"")

# The explicit anchor of a type, a field, an argument or an enum value.
HTML_ID = re.compile(r"<a\s+(?:id|name)\s*=\s*\"([^\"]*)\"")

ATX_HEADING = re.compile(r"^ {0,3}(#{1,6})[ \t]+(.*?)[ \t]*#*[ \t]*$")

# A list item marker at the start of a line, a bullet or one to nine digits and
# a period or a parenthesis, then a space, a tab or the end of the line.
LIST_MARKER = re.compile(r"^( {0,3})([-+*]|[0-9]{1,9}[.)])([ \t]+|$)")

# A thematic break, and the underline of a setext heading, after which an
# indented code block may start with no blank line.
THEMATIC_BREAK = re.compile(r"^ {0,3}([-*_])(?:[ \t]*\1){2,}[ \t]*$")
SETEXT_UNDERLINE = re.compile(r"^ {0,3}(?:=+|-+)[ \t]*$")

# The inline markup of a heading whose text GitHub keeps: a code span, a
# backslash escape of an ASCII punctuation character, a URI or an email
# autolink, an inline link or an image with its text, its destination and its
# title in any of the three quotings, and the raw HTML it drops, an open or a
# closing tag, a comment, a processing instruction, a declaration and a CDATA
# section.
CODE_SPAN = re.compile(r"(?<!`)(`+)(?!`)(.+?)(?<!`)\1(?!`)")
ESCAPE = re.compile(r"\\([!-/:-@\[-`{-~])")
EMAIL_AUTOLINK = re.compile(
    r"<([A-Za-z0-9.!#$%&'*+/=?^_`{|}~-]+@[A-Za-z0-9](?:[A-Za-z0-9.-]*[A-Za-z0-9])?)>"
)
_ATTRIBUTE = (
    r"(?:\s+[A-Za-z_:][A-Za-z0-9_.:-]*"
    r"(?:\s*=\s*(?:[^\"'=<>`\x00-\x20]+|'[^']*'|\"[^\"]*\"))?)"
)
RAW_HTML = re.compile(
    r"<[A-Za-z][A-Za-z0-9-]*" + _ATTRIBUTE + r"*\s*/?>"
    r"|</[A-Za-z][A-Za-z0-9-]*\s*>"
    r"|<!-->|<!--->|<!--[\s\S]*?-->"
    r"|<\?[\s\S]*?\?>"
    r"|<![A-Za-z]+[^>]*>"
    r"|<!\[CDATA\[[\s\S]*?\]\]>"
)
# An entity or numeric character reference, which stands for its character
# outside code spans and autolinks, where a backslash escape does not hide its
# `&`. An entity name counts when HTML5 names it, a decimal reference has 1 to 7
# digits and a hexadecimal one 1 to 6, and the code point 0, a surrogate or one
# past U+10FFFF stands for U+FFFD.
CHAR_REFERENCE = re.compile(
    r"&(?:#([0-9]{1,7})|#[xX]([0-9A-Fa-f]{1,6})|([A-Za-z][A-Za-z0-9]{0,31}));"
)
LINK_TEXT = re.compile(
    r"!?\[([^\[\]\n]*)\]\(\s*(?:<[^<>\n]*>|[^\s()]*(?:\([^\s()]*\)[^\s()]*)*)"
    r"(?:\s+(?:\"[^\"]*\"|'[^']*'|\([^()]*\)))?\s*\)"
)

# The depth of links and images nested in the text of a link that a heading
# is read to, such as a badge image inside a link.
MAX_LINK_NESTING = 8

# The first of the code points that stand in for a protected piece of a heading
# while its emphasis is read, in the private use plane 15, which no heading of
# the tool carries.
PLACEHOLDER_BASE = 0xF0000

# The delimiter runs one heading may hold before the reading of its emphasis
# refuses it.
MAX_DELIMITER_RUNS = 4096


@dataclass
class TreeDiff:
    """The differences of two trees, empty when they are equal byte for byte."""

    only_left: list = field(default_factory=list)
    only_right: list = field(default_factory=list)
    differing: list = field(default_factory=list)
    compared: int = 0

    @property
    def equal(self) -> bool:
        """Reports whether the two trees hold the same paths with the same bytes."""
        return not (self.only_left or self.only_right or self.differing)

    def lines(self) -> list:
        """The lines a notebook prints, one per difference, then the count."""
        out = ["only in the left tree: %s" % p for p in self.only_left]
        out += ["only in the right tree: %s" % p for p in self.only_right]
        for path, offset, detail in self.differing:
            out.append("differs: %s, first differing byte at offset %d" % (path, offset))
            out.extend(detail)
        out.append(
            "%d files compared, %d only left, %d only right, %d differing"
            % (self.compared, len(self.only_left), len(self.only_right), len(self.differing))
        )
        return out


@dataclass(frozen=True)
class DeadLink:
    """A link whose file or anchor the tree does not hold."""

    source: str
    line: int
    target: str
    reason: str

    def line_text(self) -> str:
        """The line a notebook prints for it."""
        return "%s:%d  %s  (%s)" % (self.source, self.line, self.target, self.reason)


def _utf8_key(text: str) -> bytes:
    return text.encode("utf-8")


def read_tree(directory) -> dict:
    """Returns every file under `directory` as its relative path and its bytes.

    The keys are POSIX relative paths in the order of their UTF-8 bytes, the
    order the manifest lists them in.
    """
    directory = Path(directory)
    files = {}
    for path in sorted(directory.rglob("*"), key=lambda p: _utf8_key(p.as_posix())):
        if path.is_file():
            if len(files) >= MAX_TREE_FILES:
                raise ValueError("the tree holds more than %d files" % MAX_TREE_FILES)
            files[path.relative_to(directory).as_posix()] = path.read_bytes()
    return files


def _is_text(path: str) -> bool:
    return path.endswith((".md", ".typ", ".svg", ".json", ".graphql", ".txt"))


def _text_detail(path: str, left: bytes, right: bytes) -> list:
    try:
        a = left.decode("utf-8").splitlines(keepends=True)
        b = right.decode("utf-8").splitlines(keepends=True)
    except UnicodeDecodeError:
        return []
    diff = list(difflib.unified_diff(a, b, "left/" + path, "right/" + path, n=2))
    lines = [line.rstrip("\n") for line in diff[:MAX_DIFF_LINES]]
    if len(diff) > MAX_DIFF_LINES:
        lines.append("... %d more lines of difference" % (len(diff) - MAX_DIFF_LINES))
    return lines


def compare_trees(left: dict, right: dict, paths=None, ignore=(GOLDEN_MANIFEST,)) -> TreeDiff:
    """Compares two trees read by `read_tree` file for file.

    A text file that differs carries a unified difference of at most
    `MAX_DIFF_LINES` lines, and every file its first differing byte offset,
    which is what a notebook prints. `paths`, when given, limits the comparison
    to those paths, which is how `manifest.json` alone is compared where the
    golden rule says so. The paths of `ignore` are left out on both sides, by
    default the golden manifest a golden directory holds beside its tree.
    """
    result = TreeDiff()
    skipped = set(ignore)
    names = sorted((set(left) | set(right)) - skipped, key=_utf8_key)
    if paths is not None:
        wanted = set(paths)
        names = [n for n in names if n in wanted]
    for name in names:
        if name not in right:
            result.only_left.append(name)
            continue
        if name not in left:
            result.only_right.append(name)
            continue
        result.compared += 1
        a, b = left[name], right[name]
        if a == b:
            continue
        detail = _text_detail(name, a, b) if _is_text(name) else []
        result.differing.append((name, first_difference(a, b), detail))
    return result


def _backtick_run(text: str, i: int) -> int:
    """Returns the length of the run of backticks that starts at `i`."""
    j = i
    while j < len(text) and text[j] == "`":
        j += 1
    return j - i


def _blank_spans(text: str) -> str:
    """Returns `text` with every code span turned into spaces.

    A span opens at a run of backticks that no backslash escapes and closes at
    the next run of exactly the same length before the next empty line, and a
    run without such a closer is literal text.
    """
    if len(text) > MAX_MARKDOWN_CHARS:
        raise ValueError("a Markdown file of more than %d characters" % MAX_MARKDOWN_CHARS)
    out = list(text)
    i = 0
    n = len(text)
    for _ in range(n + 1):
        if i >= n:
            break
        ch = text[i]
        if ch == "\\":
            i += 2
            continue
        if ch != "`":
            i += 1
            continue
        length = _backtick_run(text, i)
        end = text.find("\n\n", i)
        limit = n if end < 0 else end
        j = i + length
        close = -1
        while j < limit:
            j = text.find("`" * length, j, limit)
            if j < 0:
                break
            run = _backtick_run(text, j)
            if run == length:
                close = j
                break
            j += run
        if close < 0:
            i += length
            continue
        for k in range(i, close + length):
            if out[k] != "\n":
                out[k] = " "
        i = close + length
    return "".join(out)


def _blank_fences(text: str) -> str:
    """Returns `text` with every line of a fenced block emptied.

    Line breaks are kept, so a line number found afterwards is the line of the
    file.
    """
    lines = text.split("\n")
    fence = None
    for i, line in enumerate(lines):
        match = FENCE.match(line)
        if fence is None and match:
            fence = match.group(1)
            lines[i] = ""
        elif fence is not None:
            if match and match.group(1)[0] == fence[0] and len(match.group(1)) >= len(fence):
                fence = None
            lines[i] = ""
    return "\n".join(lines)


def _columns(line: str) -> int:
    """Returns the columns of the leading spaces and tabs of `line`, a tab stopping
    at the next multiple of four."""
    column = 0
    for ch in line:
        if ch == " ":
            column += 1
        elif ch == "\t":
            column += 4 - column % 4
        else:
            break
    return column


def _blank_indented(text: str) -> str:
    """Returns `text` with every line of an indented code block emptied.

    A line indented by four columns or more opens an indented code block after a
    blank line, at the start of the file, or after a heading or a thematic
    break, because it cannot interrupt a paragraph, and the block runs on over
    blank lines while its lines stay indented. Inside a list item the same
    indentation continues the item and opens no block, so no line is emptied
    from a list marker until a line after a blank line is indented less than the
    content of the item. An indented code block nested in a list item or in a
    block quote is not emptied, which is a limit of this reading. Line breaks
    are kept.
    """
    lines = text.split("\n")
    item_column = None
    may_open = True
    in_code = False
    for i, line in enumerate(lines):
        if not line.strip(" \t"):
            may_open = True
            continue
        column = _columns(line)
        marker = LIST_MARKER.match(line) if column < 4 else None
        block_start = bool(ATX_HEADING.match(line) or THEMATIC_BREAK.match(line))
        if item_column is not None and column < item_column and (may_open or block_start or marker):
            item_column = None
        if item_column is None and column >= 4 and (may_open or in_code):
            lines[i] = ""
            in_code = True
            continue
        in_code = False
        opens_item = item_column is None or column < item_column
        if marker and not THEMATIC_BREAK.match(line) and opens_item:
            rest = line[marker.end(2) :]
            spaces = _columns(rest) if rest.strip(" \t") else 1
            width = len(marker.group(1)) + len(marker.group(2))
            item_column = width + (spaces if spaces <= 4 else 1)
        may_open = block_start or bool(SETEXT_UNDERLINE.match(line))
    return "\n".join(lines)


def _blank_code(text: str) -> str:
    """Returns `text` with fenced blocks, indented code blocks and code spans
    turned into spaces."""
    return _blank_spans(_blank_indented(_blank_fences(text)))


def _is_space(ch: str) -> bool:
    return ch == "" or ch in "\t\n\x0c\r" or unicodedata.category(ch) == "Zs"


def _is_punctuation(ch: str) -> bool:
    return ch != "" and unicodedata.category(ch)[0] in "PS"


def _drop_emphasis(text: str) -> str:
    """Returns `text` without the underscores that delimit emphasis.

    A run of `_` opens when it is left flanking and not right flanking or
    follows punctuation, closes when it is right flanking and not left flanking
    or precedes punctuation, and each closer takes the nearest opener it may
    pair with, two underscores when both runs hold two and one otherwise, under
    the rule of three. What no pair takes stays literal. The `*` of emphasis is
    punctuation the slug drops anyway, so its runs are not paired, which reads a
    heading that interleaves `*` and `_` pairs apart from the specification.
    """
    runs = []
    for match in re.finditer(r"_+", text):
        if len(runs) >= MAX_DELIMITER_RUNS:
            raise ValueError("a heading of more than %d delimiter runs" % MAX_DELIMITER_RUNS)
        before = text[match.start() - 1] if match.start() > 0 else ""
        after = text[match.end()] if match.end() < len(text) else ""
        left = not _is_space(after) and (
            not _is_punctuation(after) or _is_space(before) or _is_punctuation(before)
        )
        right = not _is_space(before) and (
            not _is_punctuation(before) or _is_space(after) or _is_punctuation(after)
        )
        runs.append(
            {
                "start": match.start(),
                "length": len(match.group(0)),
                "left": len(match.group(0)),
                "open": left and (not right or _is_punctuation(before)),
                "close": right and (not left or _is_punctuation(after)),
                "active": True,
            }
        )
    for c, closer in enumerate(runs):
        if not closer["close"]:
            continue
        for _ in range(closer["length"]):
            if closer["left"] == 0:
                break
            opener_index = None
            for o in range(c - 1, -1, -1):
                opener = runs[o]
                if not (opener["active"] and opener["open"] and opener["left"] > 0):
                    continue
                both = opener["close"] or closer["open"]
                total = opener["length"] + closer["length"]
                thirds = opener["length"] % 3 == 0 and closer["length"] % 3 == 0
                if both and total % 3 == 0 and not thirds:
                    continue
                opener_index = o
                break
            if opener_index is None:
                break
            opener = runs[opener_index]
            used = 2 if opener["left"] >= 2 and closer["left"] >= 2 else 1
            opener["left"] -= used
            closer["left"] -= used
            for between in runs[opener_index + 1 : c]:
                between["active"] = False
            if opener["left"] == 0:
                opener["active"] = False
    out = []
    at = 0
    for run in runs:
        out.append(text[at : run["start"]])
        out.append("_" * run["left"])
        at = run["start"] + run["length"]
    out.append(text[at:])
    return "".join(out)


def _code_span_text(content: str) -> str:
    """Returns the text of a code span: one space is stripped from each end when
    both ends hold one and the content is not all spaces."""
    if len(content) >= 2 and content[0] == " " and content[-1] == " " and content.strip(" "):
        return content[1:-1]
    return content


def _char_reference(match) -> str:
    """Returns the character a reference of `CHAR_REFERENCE` stands for, or
    the reference as written when its name is not one of HTML5."""
    if match.group(3) is not None:
        return html.entities.html5.get(match.group(3) + ";", match.group(0))
    code = int(match.group(1)) if match.group(1) is not None else int(match.group(2), 16)
    if code == 0 or 0xD800 <= code <= 0xDFFF or code > 0x10FFFF:
        return "\ufffd"
    return chr(code)


def _heading_text(heading: str) -> str:
    """Returns the text of the inline content of a heading, its markup read
    as `heading_slug` says."""
    pieces = []

    def keep(text: str) -> str:
        pieces.append(text)
        return chr(PLACEHOLDER_BASE + len(pieces) - 1)

    text = CODE_SPAN.sub(lambda m: keep(_code_span_text(m.group(2))), heading)
    text = ESCAPE.sub(lambda m: keep(m.group(1)), text)
    text = AUTOLINK.sub(lambda m: keep(m.group(1)), text)
    text = EMAIL_AUTOLINK.sub(lambda m: keep(m.group(1)), text)
    text = RAW_HTML.sub("", text)
    # A decoded character is text and never markup, so it is protected too.
    text = CHAR_REFERENCE.sub(lambda m: keep(_char_reference(m)), text)
    for _ in range(MAX_LINK_NESTING):
        text, found = LINK_TEXT.subn(lambda m: m.group(1), text)
        if not found:
            break
    text = _drop_emphasis(text)
    last = PLACEHOLDER_BASE + len(pieces)
    return "".join(
        pieces[ord(ch) - PLACEHOLDER_BASE] if PLACEHOLDER_BASE <= ord(ch) < last else ch
        for ch in text
    )


def heading_slug(heading: str) -> str:
    """Returns the anchor GitHub derives from the text of a heading.

    Markup is dropped and its text kept, letters are lower cased, spaces become
    hyphens and every other punctuation character is dropped. The markup read
    here is the code span, whose text is kept as written, the backslash escape,
    the autolink, whose address is its text, the inline link and the image,
    whose destination and title are dropped and whose text is kept, the raw HTML
    tag, which is dropped, the entity and numeric character reference, which
    becomes its character, and the underscores of emphasis. A reference link
    keeps its label beside its text, a link nested in the text of a link is not
    read, and emphasis that crosses the brackets of a link is paired as if the
    link were text. The underscore of the text is kept, which is what GitHub
    does although that page does not say so.
    """
    text = "".join(ch for ch in _heading_text(heading).lower() if ch.isalnum() or ch in " -_")
    return text.replace(" ", "-")


def anchors(markdown: str, headings: bool = True) -> set:
    """Returns the anchors a Markdown file declares.

    Every explicit `<a id="..">` counts, and with `headings` the slug of every
    ATX heading too, numbered `-1`, `-2` on repetition as GitHub numbers them,
    because a host resolves both.
    """
    found = set()
    for match in HTML_ID.finditer(_blank_code(markdown)):
        found.add(match.group(1))
    if headings:
        seen = {}
        # A heading keeps its code spans, whose text is part of its slug.
        for line in _blank_fences(markdown).split("\n"):
            match = ATX_HEADING.match(line)
            if not match:
                continue
            slug = heading_slug(match.group(2))
            count = seen.get(slug, 0)
            seen[slug] = count + 1
            found.add(slug if count == 0 else "%s-%d" % (slug, count))
    return found


def links(markdown: str) -> list:
    """Returns every link target of a Markdown file with its line number.

    Inline links and images count, the destinations of link reference
    definitions, autolinks, and the `href`, `src` and `srcset` of raw HTML. A
    link inside a fenced block, an indented code block or a code span is text
    and not a link, within the limits `_blank_indented` names.
    """
    text = _blank_code(markdown)
    found = []
    for pattern in (INLINE_LINK, REFERENCE_DEFINITION):
        for match in pattern.finditer(text):
            target = match.group(1)
            if target.startswith("<") and target.endswith(">"):
                target = target[1:-1]
            found.append((text.count("\n", 0, match.start()) + 1, target))
    for match in AUTOLINK.finditer(text):
        found.append((text.count("\n", 0, match.start()) + 1, match.group(1)))
    for match in HTML_TARGET.finditer(text):
        line = text.count("\n", 0, match.start()) + 1
        values = [match.group(2)]
        if match.group(1) == "srcset":
            values = [part.strip().split(" ")[0] for part in match.group(2).split(",")]
        found.extend((line, value) for value in values if value)
    found.sort()
    return found


def resolve_links(tree: dict, headings: bool = True) -> tuple:
    """Resolves every relative link and every anchor of every Markdown file.

    Returns the dead links, the number of links resolved and the links with a
    scheme, the last as `(source, line, target)`. A link with a scheme leaves
    the tree and is not resolved here. `scheme_violations` judges its scheme.
    A relative target must name a file of the tree without leaving it, and a
    fragment must name an anchor of that file, or of the file itself for a
    bare `#fragment`.
    """
    declared = {}
    for path, data in tree.items():
        if path.endswith(".md"):
            declared[path] = anchors(data.decode("utf-8"), headings)
    dead = []
    external = []
    resolved = 0
    for source in sorted(declared, key=_utf8_key):
        base = source.rsplit("/", 1)[0] if "/" in source else ""
        for line, target in links(tree[source].decode("utf-8")):
            if SCHEME.match(target) or target.startswith("//"):
                external.append((source, line, target))
                continue
            path_part, _, fragment = target.partition("#")
            # A host decodes the percent escapes of a relative path before it
            # looks the file up.
            path_part = urllib.parse.unquote(path_part)
            if path_part.startswith("/"):
                dead.append(DeadLink(source, line, target, "an absolute path"))
                continue
            if path_part:
                parts = []
                escaped = False
                for part in (base + "/" + path_part if base else path_part).split("/"):
                    if part in ("", "."):
                        continue
                    if part == "..":
                        if not parts:
                            escaped = True
                            break
                        parts.pop()
                    else:
                        parts.append(part)
                if escaped:
                    dead.append(DeadLink(source, line, target, "leaves the tree"))
                    continue
                resolved_path = "/".join(parts)
            else:
                resolved_path = source
            if resolved_path not in tree:
                dead.append(DeadLink(source, line, target, "no such file"))
                continue
            if fragment:
                if resolved_path not in declared:
                    dead.append(DeadLink(source, line, target, "an anchor into a file that is not Markdown"))
                    continue
                if fragment not in declared[resolved_path]:
                    dead.append(DeadLink(source, line, target, "no such anchor"))
                    continue
            resolved += 1
    return dead, resolved, external


def scheme_violations(external: list) -> list:
    """Returns the links of `external` whose scheme is not http, https or mailto."""
    bad = []
    for source, line, target in external:
        match = SCHEME.match(target)
        scheme = match.group(1).lower() if match else ""
        if scheme not in ALLOWED_SCHEMES:
            bad.append((source, line, target))
    return bad


def size_violations(tree: dict, max_file: int = MAX_FILE_BYTES, max_index: int = MAX_INDEX_BYTES) -> list:
    """Returns every file over its size rule as `(path, bytes, limit)`.

    No file is over `max_file` bytes and the index, `README.md` or `API.md` at
    the root of the tree, is under `max_index`.
    """
    bad = []
    for path in sorted(tree, key=_utf8_key):
        size = len(tree[path])
        if path in INDEX_FILES:
            if size >= max_index:
                bad.append((path, size, max_index))
        elif size > max_file:
            bad.append((path, size, max_file))
    return bad


def manifest_mismatches(
    tree: dict, manifest_path: str = "manifest.json", ignore=(GOLDEN_MANIFEST, PDF_FILE)
) -> list:
    """Checks every entry of the manifest of a tree against the files it holds.

    The manifest lists `[path, sha256, bytes]` per file in path order. Returns
    one line per entry that names a missing file, another digest or another
    length, and one per file of the tree the manifest does not list, the
    manifest itself and the paths of `ignore` excepted. By default those are the
    golden manifest and the PDF, which the command writes after the tree and its
    check mode does not compare.
    """
    document = json.loads(tree[manifest_path].decode("utf-8"))
    bad = []
    if document.get("version") != MANIFEST_VERSION:
        bad.append("the manifest carries version %r, not %d" % (document.get("version"), MANIFEST_VERSION))
    listed = set()
    for path, digest, size in document["files"]:
        listed.add(path)
        data = tree.get(path)
        if data is None:
            bad.append("%s is listed and missing" % path)
            continue
        found = hashlib.sha256(data).hexdigest()
        if found != digest:
            bad.append("%s carries sha256 %s, the manifest %s" % (path, found, digest))
        if len(data) != size:
            bad.append("%s has %d bytes, the manifest %d" % (path, len(data), size))
    order = [entry[0] for entry in document["files"]]
    if order != sorted(order, key=_utf8_key):
        bad.append("the files of the manifest are not in path order")
    for path in tree:
        if path != manifest_path and path not in listed and path not in ignore:
            bad.append("%s is in the tree and not in the manifest" % path)
    return bad
