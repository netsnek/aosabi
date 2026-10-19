# SPDX-License-Identifier: MIT
"""Aosabi harness: the three levels of a PDF and the tokenizer of the Typst
string literals.

The text, the page raster hashes and the attachments are read through the
poppler programs, and the tokenizer finds every hostile string of a schema
inside a string literal of the generated source.

A PDF is compared at three levels, bytes first, then the extracted text, then a
page raster hash at 72 dpi, so a metadata change, a content change and a layout
change stay apart. Rasters are compared on one architecture only, because
rasterisers use float paths.
"""

from __future__ import annotations

import bisect
import hashlib
import re
import shutil
from dataclasses import dataclass, field
from pathlib import Path

from .paths import paths
from .procs import run

# The four poppler programs the PDF steps read through, whose versions the
# setup cell prints.
POPPLER = ("pdftotext", "pdftoppm", "pdfinfo", "pdfdetach")

# The resolution of the page rasters.
RASTER_DPI = 72

# The pages one PDF may carry before the raster step refuses it. The largest
# measured schema prints well below it.
MAX_PAGES = 5000

TIMEOUT_S = 600.0

# The version of the record `record` returns, the form of a `pdf.json` golden.
RECORD_VERSION = 1


class PopplerMissing(RuntimeError):
    """A poppler program is not on the search path."""


def versions() -> dict:
    """Returns the first version line each poppler program prints, or `absent`."""
    found = {}
    for program in POPPLER:
        if shutil.which(program) is None:
            found[program] = "absent"
            continue
        result = run([program, "-v"], check=False, timeout_s=30)
        text = (result.err or result.out).strip().splitlines()
        found[program] = text[0] if text else "no version line"
    return found


def _need(program: str) -> str:
    path = shutil.which(program)
    if path is None:
        raise PopplerMissing("%s is not on the search path" % program)
    return path


def sha256_hex(data: bytes) -> str:
    """Returns the SHA-256 of `data` in lower case base 16."""
    return hashlib.sha256(data).hexdigest()


def info(pdf) -> dict:
    """Returns the fields `pdfinfo` prints as a map of their names to their text."""
    result = run([_need("pdfinfo"), str(pdf)], timeout_s=TIMEOUT_S)
    fields = {}
    for line in result.out.splitlines():
        name, sep, value = line.partition(":")
        if sep:
            fields[name.strip()] = value.strip()
    return fields


def text(pdf) -> str:
    """Returns the text `pdftotext` extracts in UTF-8, in reading order."""
    result = run([_need("pdftotext"), "-enc", "UTF-8", str(pdf), "-"], timeout_s=TIMEOUT_S)
    return result.stdout.decode("utf-8")


def _work_dir(work_dir) -> Path:
    """Returns `work_dir` resolved, and refuses one outside `target/`.

    The two readers below delete what an earlier run left in their work
    directory, so a wrong argument, such as a goldens directory, must fail
    before a file is touched. The repository is found from this file, not
    from the working directory of the kernel.
    """
    target = paths(Path(__file__).resolve().parent).target.resolve()
    resolved = Path(work_dir).resolve()
    if resolved != target and target not in resolved.parents:
        raise ValueError("the work directory %s lies outside target/" % resolved.name)
    resolved.mkdir(parents=True, exist_ok=True)
    return resolved


def raster_hashes(pdf, work_dir, dpi: int = RASTER_DPI) -> list:
    """Returns the SHA-256 of every page rendered by `pdftoppm` at `dpi`.

    The pages are written as PNG under `work_dir`, which is a directory under
    `target/` and never under `/tmp`, and read back in page order. A `work_dir`
    outside `target/` raises `ValueError`.
    """
    work_dir = _work_dir(work_dir)
    for old in work_dir.glob("page-*.png"):
        old.unlink()
    pages = int(info(pdf).get("Pages", "0"))
    if pages > MAX_PAGES:
        raise ValueError("the PDF has %d pages, more than %d" % (pages, MAX_PAGES))
    run(
        [_need("pdftoppm"), "-r", str(dpi), "-png", str(pdf), str(work_dir / "page")],
        timeout_s=TIMEOUT_S,
    )
    files = sorted(work_dir.glob("page-*.png"), key=lambda p: int(p.stem.rsplit("-", 1)[1]))
    if len(files) != pages:
        raise ValueError("pdftoppm wrote %d pages of %d" % (len(files), pages))
    return [sha256_hex(f.read_bytes()) for f in files]


def attachments(pdf, work_dir) -> dict:
    """Returns every embedded file of the PDF as its name and its SHA-256.

    `pdfdetach -saveall` writes the files into `work_dir`, a directory under
    `target/`, which is how the card of the device chapter is read back. Every
    file an earlier run left there is deleted first, so a `work_dir` outside
    `target/` raises `ValueError` before anything is touched.
    """
    work_dir = _work_dir(work_dir)
    for old in work_dir.iterdir():
        if old.is_file():
            old.unlink()
    run([_need("pdfdetach"), "-saveall", "-o", str(work_dir), str(pdf)], timeout_s=TIMEOUT_S)
    return {
        f.name: sha256_hex(f.read_bytes())
        for f in sorted(work_dir.iterdir(), key=lambda p: p.name.encode("utf-8"))
        if f.is_file()
    }


def record(pdf, work_dir) -> dict:
    """Returns the three levels of one PDF, the form of a `pdf.json` golden.

    The keys are `version`, `sha256` and `bytes` of the file, `text_sha256` and
    `text`, the extracted text itself, which step 6 of 76_docs_pdf compares
    against `pdf.json`, so a text mismatch can print what changed, then
    `raster_dpi` and `pages`, the raster hash per page.
    """
    data = Path(pdf).read_bytes()
    extracted = text(pdf)
    return {
        "version": RECORD_VERSION,
        "sha256": sha256_hex(data),
        "bytes": len(data),
        "text_sha256": sha256_hex(extracted.encode("utf-8")),
        "text": extracted,
        "raster_dpi": RASTER_DPI,
        "pages": raster_hashes(pdf, work_dir),
    }


def first_level(found: dict, golden: dict) -> str:
    """Returns the most specific level at which `found` differs from
    `golden`, or None when the bytes are equal.

    The text is judged first, then the raster, then the bytes alone: `text`
    means a real content change, `raster` with equal text a layout change, and
    `bytes` with equal text and raster a metadata or compiler change.
    """
    if found["sha256"] == golden["sha256"]:
        return None
    if found["text_sha256"] != golden["text_sha256"]:
        return "text"
    if found["pages"] != golden["pages"]:
        return "raster"
    return "bytes"


# The escapes of a Typst string literal.
ESCAPES = {"\\": "\\", '"': '"', "n": "\n", "r": "\r", "t": "\t"}

# An identifier of Typst starts with a letter or `_` and continues with
# letters, digits, `_` and `-`. Python's word class stands in for those
# classes.
IDENT_START = re.compile(r"[^\W\d]")
IDENT = re.compile(r"[\w-]*")

# The keywords after `#` that open a statement, which runs to a line break, a
# semicolon or the `]` of the content block around it.
STATEMENTS = ("let", "set", "show", "import", "include", "return")

# The keywords after `#` that open an atomic expression which ends after its
# block, and for `if` after the block of its `else` on the same line, with the
# rest of the line markup again: `if` and `while` read a condition and a block,
# `for` a pattern, `in`, an expression and a block. `context` reads one atomic
# expression.
BLOCK_KEYWORDS = ("if", "while", "for")
CONTEXT = "context"
ELSE = "else"

# The characters Typst reads as a line break, which end a statement, a line
# comment and a keyword expression of markup.
NEWLINES = "\n\x0b\x0c\r\x85\u2028\u2029"

# The trivia between the parts of a keyword expression on one line.
BLANKS = " \t"

# The frame each opening character pushes in code: a parenthesis and a brace
# open code, a bracket opens markup and a dollar opens math.
OPENERS = {"(": ("code", ")"), "{": ("code", "}"), "[": ("markup", "]"), "$": ("math", "$")}

# The characters one source may hold before the tokenizer refuses it.
MAX_SOURCE_CHARS = 64 * 1024 * 1024


@dataclass(frozen=True)
class Literal:
    """A string literal of a Typst source: its span and its decoded value."""

    start: int
    end: int
    value: str


@dataclass
class Tokens:
    """What the tokenizer finds in one source."""

    literals: list = field(default_factory=list)
    outside: str = ""


def _string(source: str, i: int) -> tuple:
    """Reads the literal opening at `i` and returns its value and its end."""
    out = []
    j = i + 1
    n = len(source)
    while j < n:
        ch = source[j]
        if ch == '"':
            return "".join(out), j + 1
        if ch == "\\" and j + 1 < n:
            nxt = source[j + 1]
            if nxt in ESCAPES:
                out.append(ESCAPES[nxt])
                j += 2
                continue
            if nxt == "u" and source.startswith("{", j + 2):
                close = source.find("}", j + 3)
                if close < 0:
                    raise ValueError("an unclosed unicode escape at %d" % j)
                out.append(chr(int(source[j + 3 : close], 16)))
                j = close + 1
                continue
            raise ValueError("the escape \\%s at %d is not one of Typst" % (nxt, j))
        out.append(ch)
        j += 1
    raise ValueError("an unclosed string literal at %d" % i)


def _comment_end(source: str, i: int) -> int:
    """Returns the end of the comment at `i`, a line or a nested block comment.

    A line comment ends before the first character Typst reads as a line
    break.
    """
    if source.startswith("//", i):
        j = i + 2
        n = len(source)
        while j < n and source[j] not in NEWLINES:
            j += 1
        return j
    depth = 0
    j = i
    n = len(source)
    while j < n:
        if source.startswith("/*", j):
            depth += 1
            j += 2
        elif source.startswith("*/", j):
            depth -= 1
            j += 2
            if depth == 0:
                return j
        else:
            j += 1
    raise ValueError("an unclosed block comment at %d" % i)


def _raw_end(source: str, i: int) -> int:
    """Returns the end of the raw text of backticks opening at `i`."""
    j = i
    while j < len(source) and source[j] == "`":
        j += 1
    run_length = j - i
    if run_length == 2:
        return i + 2
    fence = "`" * run_length
    end = source.find(fence, i + run_length)
    if end < 0:
        raise ValueError("an unclosed raw text at %d" % i)
    return end + run_length


def _word_at(source: str, i: int, word: str) -> bool:
    """Reports whether the identifier at `i` is exactly `word`."""
    if not source.startswith(word, i):
        return False
    end = i + len(word)
    return end >= len(source) or not IDENT.match(source, end).group(0)


def tokenize(source: str) -> Tokens:
    """Finds every string literal of a Typst source by the mode it stands in.

    Markup is the mode of a file. `#` enters code for one atomic expression: an
    identifier, a literal, a parenthesised expression, a code or content block
    or an equation, followed by any call arguments `(..)` and `[..]` and field
    accesses `.name` that touch it. A keyword of `STATEMENTS` after `#` runs to
    a line break, a semicolon or the `]` around it. `if`, `while` and `for` run
    to the end of their block, `if` on through an `else` and its block on the
    same line, and `context` takes one atomic expression. When the expression
    ends, the rest of the line is markup again. `(` and `{` open code, `[`
    opens markup and `$` math. A string literal exists in code and in math,
    while `"` in markup is a quote of the text. Comments and raw text hold no
    literal in any mode. The walk keeps a stack of modes and is linear in the
    length of the source.

    Two readings are simpler than the parser: the head of a keyword expression
    ends at the first `{` or the first `[` that does not touch the token before
    it, where the parser reads a whole code expression, and `else` is looked
    for after blanks and not after a comment.

    `outside` is the source with every literal replaced by spaces of the same
    length, the text the compiler reads as syntax, which a notebook prints
    beside a finding.
    """
    if len(source) > MAX_SOURCE_CHARS:
        raise ValueError("a source of more than %d characters" % MAX_SOURCE_CHARS)
    tokens = Tokens()
    blanked = list(source)
    # Each frame is a mode and its state: ("markup", None) for the file,
    # ("markup", "]") for a content block, ("code", ")") and ("code", "}"),
    # ("math", "$"), ("line", None) for a statement, ("expr", phase) for one
    # atomic expression of markup or math, where the phase is "start",
    # "context" or "after" its primary, and ("keyword", (word, phase)) for
    # `if`, `while` and `for`, where the phase is "head0" before the head,
    # "head" inside it, "body" once the block closed, "else" after `else` and
    # "last" once the block of the `else` closed.
    stack = [("markup", None)]
    i = 0
    n = len(source)

    def literal(at: int) -> int:
        value, end = _string(source, at)
        tokens.literals.append(Literal(at, end, value))
        blanked[at:end] = " " * (end - at)
        return end

    def opens(ch: str):
        """The frame a bracket, a brace, a parenthesis or a dollar opens in code."""
        return OPENERS.get(ch)

    # Every step advances `i` or pops a frame, and every push advances `i`, so
    # the walk takes at most about twice the length of the source in steps.
    for _ in range(4 * n + 4):
        if i >= n:
            break
        mode, state = stack[-1]
        ch = source[i]
        if source.startswith("//", i) or source.startswith("/*", i):
            # In markup `//` after a colon is part of a URL, which Typst links,
            # and no comment.
            if mode != "markup" or source.startswith("/*", i) or i == 0 or source[i - 1] != ":":
                i = _comment_end(source, i)
                continue
        if mode == "markup":
            if ch == "`":
                i = _raw_end(source, i)
            elif ch == "\\" and i + 1 < n:
                i += 2
            elif ch == "#":
                stack.append(("expr", "start"))
                i += 1
            elif ch == "$":
                stack.append(("math", "$"))
                i += 1
            elif ch == "[":
                stack.append(("markup", "]"))
                i += 1
            elif ch == "]" and state == "]":
                stack.pop()
                i += 1
            else:
                i += 1
            continue
        if mode == "math":
            if ch == "\\" and i + 1 < n:
                i += 2
            elif ch == '"':
                i = literal(i)
            elif ch == "#":
                stack.append(("expr", "start"))
                i += 1
            elif ch == "$":
                stack.pop()
                i += 1
            else:
                i += 1
            continue
        if mode == "expr":
            if state == "context" and ch in BLANKS:
                i += 1
                continue
            if state in ("start", "context"):
                # The primary of the expression.
                if ch == "`":
                    stack[-1] = ("expr", "after")
                    i = _raw_end(source, i)
                elif ch == '"':
                    stack[-1] = ("expr", "after")
                    i = literal(i)
                elif opens(ch):
                    stack[-1] = ("expr", "after")
                    stack.append(opens(ch))
                    i += 1
                elif IDENT_START.match(source, i):
                    word = IDENT.match(source, i).group(0)
                    i += len(word)
                    if word in STATEMENTS:
                        stack[-1] = ("line", None)
                    elif word in BLOCK_KEYWORDS:
                        stack[-1] = ("expr", "after")
                        stack.append(("keyword", (word, "head0")))
                    elif word == CONTEXT:
                        stack[-1] = ("expr", "context")
                    else:
                        stack[-1] = ("expr", "after")
                else:
                    stack.pop()
                continue
            # After the primary only what touches it continues the expression.
            if ch == "(" or ch == "[":
                stack.append(opens(ch))
                i += 1
            elif ch == "." and IDENT_START.match(source, i + 1):
                i += 1 + len(IDENT.match(source, i + 1).group(0))
            else:
                stack.pop()
            continue
        if mode == "keyword":
            word, phase = state
            if phase in ("head0", "head"):
                if ch in NEWLINES or ch == "]":
                    stack.pop()
                elif phase == "head0" and ch in BLANKS:
                    i += 1
                elif ch == "`":
                    stack[-1] = ("keyword", (word, "head"))
                    i = _raw_end(source, i)
                elif ch == '"':
                    stack[-1] = ("keyword", (word, "head"))
                    i = literal(i)
                elif ch == "{" or (ch == "[" and source[i - 1] in BLANKS):
                    # The block, unless the head has not begun, where a block
                    # is the start of the condition.
                    stack[-1] = ("keyword", (word, "head" if phase == "head0" else "body"))
                    stack.append(opens(ch))
                    i += 1
                elif opens(ch):
                    stack[-1] = ("keyword", (word, "head"))
                    stack.append(opens(ch))
                    i += 1
                else:
                    stack[-1] = ("keyword", (word, "head"))
                    i += 1
                continue
            if phase == "body" and word == "if":
                j = i
                while j < n and source[j] in BLANKS:
                    j += 1
                if _word_at(source, j, ELSE):
                    stack[-1] = ("keyword", (word, "else"))
                    i = j + len(ELSE)
                else:
                    stack.pop()
                continue
            if phase == "else":
                if ch in BLANKS:
                    i += 1
                elif _word_at(source, i, "if"):
                    stack[-1] = ("keyword", (word, "head0"))
                    i += 2
                elif ch == "{" or ch == "[":
                    stack[-1] = ("keyword", (word, "last"))
                    stack.append(opens(ch))
                    i += 1
                else:
                    stack.pop()
                continue
            # The block of a loop or of the `else` closed.
            stack.pop()
            continue
        if mode == "line":
            if ch in NEWLINES or ch == "]":
                stack.pop()
                continue
            if ch == ";":
                stack.pop()
                i += 1
                continue
        # Code, and a statement, read literals, brackets and closers.
        if ch == "`":
            i = _raw_end(source, i)
        elif ch == '"':
            i = literal(i)
        elif opens(ch):
            stack.append(opens(ch))
            i += 1
        elif ch == state:
            stack.pop()
            i += 1
        else:
            i += 1
    else:
        raise ValueError("the tokenizer did not finish inside its cap")
    tokens.outside = "".join(blanked)
    return tokens


@dataclass
class HostileReport:
    """Where each hostile string of a schema was found in the generated sources."""

    in_literal: dict = field(default_factory=dict)
    outside: dict = field(default_factory=dict)

    @property
    def missing(self) -> list:
        """The strings found in no literal of any source."""
        return [s for s, count in self.in_literal.items() if count == 0]

    @property
    def injected(self) -> list:
        """The strings whose text stands in some source outside a literal."""
        return [s for s, count in self.outside.items() if count > 0]


def _inside_literal(literals: list, starts: list, start: int, end: int) -> bool:
    """Reports whether the span from `start` to `end` lies between the quotes
    of one literal of `literals`, whose starts `starts` lists in order."""
    index = bisect.bisect_right(starts, start) - 1
    if index < 0:
        return False
    literal = literals[index]
    return literal.start < start and end <= literal.end - 1


def find_hostile(sources, strings) -> HostileReport:
    """Looks for every string of `strings` in the literals of `sources`.

    `sources` is a map from a path to the text of one generated `.typ` file.
    A string counts as found when a decoded literal holds it. It counts as
    injected at every place its text stands verbatim in a source and not
    between the quotes of one literal, which is where the compiler would read
    it as syntax, including a string whose own quotes opened a literal of the
    injected code. A string shorter than two characters, such as one control
    character, is looked for in the literals alone, because its text outside
    a literal says nothing.
    """
    report = HostileReport()
    tokenized = {path: tokenize(text) for path, text in sources.items()}
    starts_of = {path: [lit.start for lit in t.literals] for path, t in tokenized.items()}
    for value in strings:
        report.in_literal[value] = sum(
            1 for t in tokenized.values() for lit in t.literals if value in lit.value
        )
        report.outside[value] = 0
        if len(value) < 2:
            continue
        for path, text in sources.items():
            literals = tokenized[path].literals
            starts = starts_of[path]
            at = text.find(value)
            for _ in range(len(text) + 1):
                if at < 0:
                    break
                if not _inside_literal(literals, starts, at, at + len(value)):
                    report.outside[value] += 1
                at = text.find(value, at + 1)
    return report


# The cases the tokenizer is held to. Each source compiles with the installed
# typst 0.14.2, and each string of the last column is read as syntax there,
# because its text after `#let` is absent from what pdftotext extracts. In the
# case `literals in code` six of the eight strings are printed from a literal.
# The other two are never printed, `#let a = 1` because `s` is never shown and
# `#let b = 2` because it is only the operand of a comparison, and the compile
# succeeding is what holds them as literals. The first two cases put a keyword
# expression before markup on the same line.
TOKENIZER_CASES = (
    (
        "for then a quote",
        '#for x in (1,) [a] "x #let x = 1\n" #x\n',
        ("#let x = 1",),
        ("#let x = 1",),
    ),
    ("if then an open quote", '#if true [a] He said "hi\n', (), ()),
    (
        "if else then a quote",
        '#if false [a] else [b] "c #let y = 2\n"\n',
        ("#let y = 2",),
        ("#let y = 2",),
    ),
    (
        "else if then a quote",
        '#if false [a] else if true [b] else [c] "d #let y = 2\n"\n',
        ("#let y = 2",),
        ("#let y = 2",),
    ),
    (
        "else on the next line",
        '#if false [a]\nelse [b] "e #let y = 2\n"\n',
        ("#let y = 2",),
        ("#let y = 2",),
    ),
    (
        "while then a quote",
        '#let i = 0\n#while i < 1 { i += 1; [w] } "f #let z = 3\n"\n',
        ("#let z = 3",),
        ("#let z = 3",),
    ),
    ("context then a quote", '#context [k] "g #let q = 4\n"\n', ("#let q = 4",), ("#let q = 4",)),
    (
        "context of a field",
        '#context text.lang "h #let q = 4\n"\n',
        ("#let q = 4",),
        ("#let q = 4",),
    ),
    (
        "a statement ends at ]",
        '#box[#let x = 1] "i #let w = 5\n"\n',
        ("#let w = 5",),
        ("#let w = 5",),
    ),
    (
        "a statement ends at U+2028",
        '#let v = 1\u2028"j #let u = 5\n"\n',
        ("#let u = 5",),
        ("#let u = 5",),
    ),
    (
        "an identifier then a quote",
        '#let x = 1\n#x"k #let t = 6\n"\n',
        ("#let t = 6",),
        ("#let t = 6",),
    ),
    (
        "literals in code",
        '#let s = "#let a = 1"\n#if s == "#let b = 2" [x] else [#"#let c = 3"]\n'
        '#for c in ("#let e = 5",) [#c]\n#context text(lang: "de")[#"#let f = 6"]\n'
        '$"#let g = 7"$\n#box(width: 1cm)[#"#let h = 8"]\n'
        '#let n = if true { "#let m = 9" } else { "" }\n#n\n',
        (
            "#let a = 1",
            "#let b = 2",
            "#let c = 3",
            "#let e = 5",
            "#let f = 6",
            "#let g = 7",
            "#let h = 8",
            "#let m = 9",
        ),
        (),
    ),
)


def check_tokenizer() -> list:
    """Runs `find_hostile` over every case of `TOKENIZER_CASES`.

    Returns one line per case whose injected strings differ from the case, or
    whose other strings stand in no literal, and an empty list when every
    case holds, which a notebook asserts before it trusts the tokenizer.
    """
    failures = []
    for name, source, strings, injected in TOKENIZER_CASES:
        try:
            report = find_hostile({name: source}, strings)
        except ValueError as refused:
            failures.append("%s: %s" % (name, refused))
            continue
        expected_missing = [s for s in strings if s in injected]
        if report.injected != list(injected) or report.missing != expected_missing:
            failures.append(
                "%s: injected %s and missing %s, the case says %s"
                % (name, report.injected, report.missing, list(injected))
            )
    return failures
