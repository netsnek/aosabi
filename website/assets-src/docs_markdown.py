# SPDX-License-Identifier: MIT
"""Markdown to HTML for the docs section of the site.

The documentation is written in GitHub Flavored Markdown, and this module renders the part of it
that a page of Documentation/ uses, with the standard library alone:

1. Blocks: ATX and setext headings, paragraphs, block quotes, bullet and ordered lists, tight and
   loose, fenced and indented code, thematic breaks, the tables of GitHub Flavored Markdown with
   their alignment, and link reference definitions.
2. Inlines: code spans, emphasis and strong emphasis by the delimiter rules of CommonMark,
   strikethrough with two tildes, inline and reference links, autolinks in angle brackets, the bare
   web addresses GitHub links, hard line breaks and backslash escapes.

Raw HTML is never passed through. A tag that a document writes, such as the placeholder <name> in
a path, is shown as the text it is, and so is an HTML entity. An image is shown as a link to the
image, because a page of the site loads nothing it does not carry itself. A link whose scheme is
not http, https or mailto is shown as its text.

The id of a heading is made the way GitHub makes it, so a link to a section of a file works on
the site as it works on GitHub: the text in lower case, every character dropped that is neither a
letter, a digit, a mark, a space, a hyphen nor an underscore, the spaces turned into hyphens, and a
second heading of the same text numbered -1, -2 and on.

    page = render(text, link=None, codelink=None)

The function link, when given, receives the destination of every link and returns the address the
page links to, or None to show the link text without a link. The function codelink, when given,
receives the text of every code span outside a link and returns an address when the span names
something to link to, such as a file of the repository, or None to leave it as code alone. The result carries the title, the
first heading of level 1, which the page shows in its head and not in its body, the body as HTML,
the headings with their ids, and the text of the first paragraph. self_test() renders the cases of
CASES and returns the ones that come out differently.
"""

from __future__ import annotations

import html
import re
import unicodedata
import urllib.parse

__all__ = ["Page", "render", "inline_html", "escape_md", "slug", "plain", "self_test", "CASES"]

PUNCT = frozenset("!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~")

FENCE = re.compile(r"^( {0,3})(`{3,}|~{3,})(.*)$")
ATX = re.compile(r"^ {0,3}(#{1,6})(?=[ \t]|$)(.*)$")
HR = re.compile(r"^ {0,3}([-*_])(?:[ \t]*\1){2,}[ \t]*$")
SETEXT = re.compile(r"^ {0,3}(=+|-+)[ \t]*$")
QUOTE = re.compile(r"^ {0,3}>")
QUOTE_MARK = re.compile(r"^ {0,3}> ?")
BULLET = re.compile(r"^( {0,3})([-+*])(?=[ \t]|$)")
ORDERED = re.compile(r"^( {0,3})(\d{1,9})([.)])(?=[ \t]|$)")
DELIM_ROW = re.compile(r"^ {0,3}\|?[ \t]*:?-+:?[ \t]*(?:\|[ \t]*:?-+:?[ \t]*)*\|?[ \t]*$")
REFDEF = re.compile(
    r"^ {0,3}\[((?:[^\[\]\\]|\\.)+)\]:[ \t]*(<[^<>\n]*>|\S+)"
    r"(?:[ \t]+(\"(?:[^\"\\]|\\.)*\"|'(?:[^'\\]|\\.)*'|\((?:[^()\\]|\\.)*\)))?[ \t]*(?:\n|$)")
AUTOLINK = re.compile(r"<([A-Za-z][A-Za-z0-9+.-]{1,31}:[^\s<>]*)>")
EMAIL = re.compile(r"<([A-Za-z0-9.!#$%&'*+/=?^_`{|}~-]+@[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?"
                   r"(?:\.[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?)*)>")
WEB = re.compile(r"(?:https?://|www\.)[^\s<]*")
SCHEMES = ("http:", "https:", "mailto:")
SAFE_URL = "!#$%&'()*+,-./:;=?@[]_~"


class Page:
    """What render() returns."""

    def __init__(self, title, title_md, title_id, body, headings, lead):
        self.title = title          # The inner HTML and plain text of the first heading of level 1, or None
        self.title_md = title_md    # Its Markdown
        self.title_id = title_id    # Its id, the one GitHub gives it
        self.body = body            # The HTML of everything else
        self.headings = headings    # The level, id, inner HTML and plain text of every heading of the body
        self.lead = lead            # The plain text of the first paragraph of the body, or an empty string


def esc(s):
    return html.escape(s, quote=True)


def plain(fragment):
    """The text of an HTML fragment, without its tags."""
    return html.unescape(re.sub(r"<[^>]*>", "", fragment))


def slug(text):
    """The id GitHub gives a heading with this text, before the numbering of a repeated one."""
    out = []
    for ch in text.lower():
        if ch in " -_" or ch.isalnum() or unicodedata.category(ch) in ("Mn", "Mc", "Me", "Nd", "Pc"):
            out.append(ch)
    return "".join(out).replace(" ", "-")


def _blank(line):
    return not line.strip()


def _indent(line):
    return len(line) - len(line.lstrip(" "))


def _is_space(c):
    return c == "" or c.isspace()


def _is_punct(c):
    return bool(c) and (c in PUNCT or unicodedata.category(c)[0] in "PS")


def _norm(label):
    return " ".join(label.split()).casefold()


def _unescape(s):
    return re.sub(r"\\([!-/:-@\[-`{-~])", r"\1", s)


def _cells(line):
    """The cells of one row of a table. A pipe written as \\| belongs to its cell."""
    s = line.strip()
    if s.startswith("|"):
        s = s[1:]
    if s.endswith("|") and not s.endswith("\\|"):
        s = s[:-1]
    cells, cur, i = [], [], 0
    while i < len(s):
        if s[i] == "\\" and i + 1 < len(s) and s[i + 1] == "|":
            cur.append("|")
            i += 2
            continue
        if s[i] == "|":
            cells.append("".join(cur).strip())
            cur = []
        else:
            cur.append(s[i])
        i += 1
    cells.append("".join(cur).strip())
    return cells


def _aligns(line):
    out = []
    for c in _cells(line):
        left, right = c.startswith(":"), c.endswith(":")
        out.append("center" if left and right else "right" if right else "left" if left else None)
    return out


class _Item:
    """The start of a list item: its kind, its number, the indent of its content and its first line."""

    def __init__(self, line):
        m = BULLET.match(line)
        self.ordered = m is None
        if m is None:
            m = ORDERED.match(line)
        self.ok = m is not None and not HR.match(line)
        if not self.ok:
            return
        self.kind = ("ol", m.group(3)) if self.ordered else ("ul", m.group(2))
        self.number = int(m.group(2)) if self.ordered else 1
        end = m.end()
        rest = line[end:]
        spaces = len(rest) - len(rest.lstrip(" "))
        if not rest.strip():
            self.indent, self.content, self.empty = end + 1, "", True
        elif spaces > 4:
            self.indent, self.content, self.empty = end + 1, rest[1:], False
        else:
            self.indent, self.content, self.empty = end + spaces, rest[spaces:], False


def _item(line):
    it = _Item(line)
    return it if it.ok else None


class _Parser:
    """The block structure of a document. The reference definitions of the whole document are
    collected here, so the inlines are parsed after the blocks, with every definition known."""

    def __init__(self):
        self.refs = {}

    def starts_block(self, line):
        """Whether a line starts a block that ends a paragraph before it."""
        if ATX.match(line) or HR.match(line) or QUOTE.match(line):
            return True
        m = FENCE.match(line)
        if m and not (m.group(2)[0] == "`" and "`" in m.group(3)):
            return True
        it = _item(line)
        return bool(it and not it.empty and (not it.ordered or it.number == 1))

    def take_refdefs(self, text):
        while True:
            m = REFDEF.match(text)
            if not m:
                return text
            label = _norm(m.group(1))
            dest = m.group(2)
            if dest.startswith("<"):
                dest = dest[1:-1]
            title = m.group(3)[1:-1] if m.group(3) else None
            self.refs.setdefault(label, (_unescape(dest), _unescape(title) if title else None))
            text = text[m.end():]

    @staticmethod
    def ends_in_paragraph(body):
        """Whether the lines so far end inside a paragraph, which a lazy line may continue."""
        if not body or _blank(body[-1]):
            return False
        fences = 0
        for line in body:
            if FENCE.match(line):
                fences += 1
        last = body[-1]
        return fences % 2 == 0 and not (ATX.match(last) or HR.match(last) or FENCE.match(last))

    def blocks(self, lines):
        """The blocks of a container, and whether a blank line separates two of them."""
        out, para = [], []
        state = {"blank": False, "between": False}

        def add(block):
            if out and state["blank"]:
                state["between"] = True
            state["blank"] = False
            out.append(block)

        def close():
            if para:
                text = "\n".join(line.lstrip(" ") for line in para).rstrip()
                para.clear()
                text = self.take_refdefs(text)
                if text.strip():
                    add(("para", text))

        i, n = 0, len(lines)
        while i < n:
            line = lines[i]
            if _blank(line):
                close()
                state["blank"] = True
                i += 1
                continue
            if not para and _indent(line) >= 4:
                code = []
                while i < n and (_blank(lines[i]) or _indent(lines[i]) >= 4):
                    code.append(lines[i][4:])
                    i += 1
                while code and not code[-1].strip():
                    code.pop()
                add(("code", "", "\n".join(code) + "\n"))
                continue
            m = FENCE.match(line)
            if m and not (m.group(2)[0] == "`" and "`" in m.group(3)):
                close()
                pad, fence, info = len(m.group(1)), m.group(2), m.group(3).strip()
                closing = re.compile(r"^ {0,3}" + re.escape(fence[0]) + "{%d,}[ \t]*$" % len(fence))
                body = []
                i += 1
                while i < n and not closing.match(lines[i]):
                    text = lines[i]
                    k = 0
                    while k < pad and k < len(text) and text[k] == " ":
                        k += 1
                    body.append(text[k:])
                    i += 1
                i += 1
                add(("code", info, "".join(b + "\n" for b in body)))
                continue
            m = ATX.match(line)
            if m:
                close()
                text = m.group(2).strip()
                text = "" if re.fullmatch(r"#*", text) else re.sub(r"[ \t]+#+$", "", text)
                add(("heading", len(m.group(1)), text.strip()))
                i += 1
                continue
            if para and DELIM_ROW.match(line) and ("|" in line or "|" in para[-1]):
                header, aligns = _cells(para[-1]), _aligns(line)
                if len(header) == len(aligns):
                    para.pop()
                    close()
                    rows = []
                    i += 1
                    while i < n and not _blank(lines[i]) and "|" in lines[i] and not self.starts_block(lines[i]):
                        rows.append(_cells(lines[i]))
                        i += 1
                    add(("table", aligns, header, rows))
                    continue
            if para and SETEXT.match(line):
                text = "\n".join(p.strip() for p in para)
                para.clear()
                add(("heading", 1 if line.strip()[0] == "=" else 2, text))
                i += 1
                continue
            if HR.match(line):
                close()
                add(("hr",))
                i += 1
                continue
            if QUOTE.match(line):
                close()
                body = []
                while i < n:
                    text = lines[i]
                    if QUOTE.match(text):
                        body.append(QUOTE_MARK.sub("", text, count=1))
                    elif not _blank(text) and self.ends_in_paragraph(body) and not self.starts_block(text):
                        body.append(text)
                    else:
                        break
                    i += 1
                kids, _ = self.blocks(body)
                add(("quote", kids))
                continue
            it = _item(line)
            if it and (not para or (not it.empty and (not it.ordered or it.number == 1))):
                close()
                block, i = self.list(lines, i)
                add(block)
                continue
            para.append(line)
            i += 1
        close()
        return out, state["between"]

    def list(self, lines, i):
        n = len(lines)
        first = _item(lines[i])
        items, loose = [], False
        while i < n:
            it = _item(lines[i])
            if not it or it.kind != first.kind:
                break
            body = [it.content]
            i += 1
            while i < n:
                text = lines[i]
                if _blank(text):
                    body.append("")
                elif _indent(text) >= it.indent:
                    body.append(text[it.indent:])
                elif self.ends_in_paragraph(body) and not self.starts_block(text) and not _item(text):
                    body.append(text.lstrip(" "))
                else:
                    break
                i += 1
            trailing = 0
            while body and body[-1] == "":
                body.pop()
                trailing += 1
            kids, between = self.blocks(body)
            loose = loose or between
            items.append(kids)
            if trailing and i < n:
                nxt = _item(lines[i])
                if nxt and nxt.kind == first.kind:
                    loose = True
        return ("list", first.ordered, first.number, not loose, items), i


class _Delim:
    __slots__ = ("ch", "n", "orig", "opens", "closes")

    def __init__(self, ch, n, opens, closes):
        self.ch, self.n, self.orig, self.opens, self.closes = ch, n, n, opens, closes


class _Renderer:
    def __init__(self, refs, link, codelink=None):
        self.refs = refs
        self.link = link
        self.codelink = codelink
        self.seen = {}
        self.headings = []
        self.lead = ""

    # ------------------------------------------------------------- The ids
    def unique(self, text):
        base = slug(text)
        result = base
        while result in self.seen:
            self.seen[base] += 1
            result = "%s-%d" % (base, self.seen[base])
        self.seen[result] = 0
        return result

    # ------------------------------------------------------------- The links
    def href(self, dest):
        """The address a destination links to, or None for none."""
        dest = dest.strip()
        scheme = re.match(r"^([A-Za-z][A-Za-z0-9+.-]*):", dest)
        if scheme and not dest.lower().startswith(SCHEMES):
            return None
        if self.link is not None:
            dest = self.link(dest)
            if dest is None:
                return None
        return urllib.parse.quote(dest, safe=SAFE_URL)

    def anchor(self, dest, inner, title=None):
        href = self.href(dest)
        if href is None:
            return '<span class="nolink">%s</span>' % inner
        attrs = ' title="%s"' % esc(title) if title else ""
        if re.match(r"^https?://", href) and not re.match(r"^https?://(?:www\.)?aosabi\.org(?:/|$)", href):
            attrs += ' rel="noopener noreferrer"'
        return '<a href="%s"%s>%s</a>' % (esc(href), attrs, inner)

    def text(self, s, autolink):
        """Text, with the bare web addresses GitHub links."""
        if not autolink:
            return esc(s)
        out, pos = [], 0
        for m in WEB.finditer(s):
            start = m.start()
            if start > 0 and not (s[start - 1].isspace() or s[start - 1] in "*_~("):
                continue
            url = m.group(0)
            while url and url[-1] in "?!.,:*_~'\"":
                url = url[:-1]
            while url.endswith(")") and url.count(")") > url.count("("):
                url = url[:-1]
            host = re.sub(r"^(?:https?://)", "", url).split("/")[0]
            if "." not in host or host.endswith("."):
                continue
            out.append(esc(s[pos:start]))
            out.append(self.anchor(url if not url.startswith("www.") else "http://" + url, esc(url)))
            pos = start + len(url)
        out.append(esc(s[pos:]))
        return "".join(out)

    # ------------------------------------------------------------- The inlines
    def join(self, nodes, autolink):
        out = []
        for x in nodes:
            if isinstance(x, _Delim):
                out.append(esc(x.ch * x.n))
            elif x[0] == "t":
                out.append(self.text(x[1], autolink))
            elif x[0] == "code":
                code = "<code>%s</code>" % esc(x[1])
                href = self.codelink(x[1]) if self.codelink is not None and autolink else None
                out.append('<a class="code" href="%s">%s</a>' % (esc(href), code) if href else code)
            else:
                out.append(x[1])
        return "".join(out)

    def emphasis(self, nodes, autolink):
        """The emphasis of CommonMark: each closer, from the left, takes the nearest opener of its
        character, under the rule of three, and what lies between them becomes its content."""
        while True:
            pair = None
            for k, x in enumerate(nodes):
                if not (isinstance(x, _Delim) and x.closes and x.n):
                    continue
                for j in range(k - 1, -1, -1):
                    o = nodes[j]
                    if not (isinstance(o, _Delim) and o.ch == x.ch and o.opens and o.n):
                        continue
                    if x.ch == "~":
                        if o.n != x.n:
                            continue
                    elif (o.closes or x.opens) and (o.orig + x.orig) % 3 == 0 \
                            and not (o.orig % 3 == 0 and x.orig % 3 == 0):
                        continue
                    pair = (j, k)
                    break
                if pair:
                    break
                x.closes = False
            if pair is None:
                return nodes
            j, k = pair
            o, x = nodes[j], nodes[k]
            if x.ch == "~":
                use, tag = x.n, "del"
            else:
                use = 2 if o.n >= 2 and x.n >= 2 else 1
                tag = "strong" if use == 2 else "em"
            inner = self.join(nodes[j + 1:k], autolink)
            o.n -= use
            x.n -= use
            nodes[j + 1:k] = [("h", "<%s>%s</%s>" % (tag, inner, tag))]
            if x.n == 0:
                del nodes[j + 2]
            if o.n == 0:
                del nodes[j]

    @staticmethod
    def skip_ws(s, j):
        newline = False
        while j < len(s) and s[j] in " \t\n":
            if s[j] == "\n":
                if newline:
                    break
                newline = True
            j += 1
        return j

    def link_tail(self, s, i):
        """The (destination "title") after the closing bracket of a link, or None."""
        n = len(s)
        if i >= n or s[i] != "(":
            return None
        j = self.skip_ws(s, i + 1)
        if j < n and s[j] == ")":
            return "", None, j + 1
        buf = []
        if j < n and s[j] == "<":
            k = j + 1
            while k < n and s[k] not in "<>\n":
                if s[k] == "\\" and k + 1 < n and s[k + 1] in PUNCT:
                    buf.append(s[k + 1])
                    k += 2
                    continue
                buf.append(s[k])
                k += 1
            if k >= n or s[k] != ">":
                return None
            j = k + 1
        else:
            depth, k = 0, j
            while k < n:
                c = s[k]
                if c == "\\" and k + 1 < n and s[k + 1] in PUNCT:
                    buf.append(s[k + 1])
                    k += 2
                    continue
                if c.isspace() or ord(c) < 0x20:
                    break
                if c == "(":
                    depth += 1
                elif c == ")":
                    if depth == 0:
                        break
                    depth -= 1
                buf.append(c)
                k += 1
            if depth:
                return None
            j = k
        dest = "".join(buf)
        title = None
        k = self.skip_ws(s, j)
        if k > j and k < n and s[k] in "\"'(":
            close = ")" if s[k] == "(" else s[k]
            t, k = [], k + 1
            while k < n and s[k] != close:
                if s[k] == "\\" and k + 1 < n and s[k + 1] in PUNCT:
                    t.append(s[k + 1])
                    k += 2
                    continue
                t.append(s[k])
                k += 1
            if k >= n:
                return None
            title = "".join(t)
            k = self.skip_ws(s, k + 1)
        if k < n and s[k] == ")":
            return dest, title, k + 1
        return None

    def inline(self, s, autolink=True):
        nodes, stack, buf = [], [], []
        i, n = 0, len(s)

        def flush():
            if buf:
                nodes.append(("t", "".join(buf)))
                buf.clear()

        while i < n:
            c = s[i]
            if c == "\\":
                if i + 1 < n and s[i + 1] in PUNCT:
                    buf.append(s[i + 1])
                    i += 2
                    continue
                if i + 1 < n and s[i + 1] == "\n":
                    flush()
                    nodes.append(("h", "<br>\n"))
                    i += 2
                    continue
                buf.append(c)
                i += 1
                continue
            if c == "`":
                j = i
                while j < n and s[j] == "`":
                    j += 1
                ticks, k, found = j - i, j, -1
                while True:
                    k = s.find("`" * ticks, k)
                    if k < 0:
                        break
                    e = k + ticks
                    if e < n and s[e] == "`":
                        while e < n and s[e] == "`":
                            e += 1
                        k = e
                        continue
                    found = k
                    break
                if found < 0:
                    buf.append("`" * ticks)
                    i = j
                    continue
                code = s[j:found].replace("\n", " ")
                if len(code) >= 2 and code[0] == " " and code[-1] == " " and code.strip(" "):
                    code = code[1:-1]
                flush()
                nodes.append(("code", code))
                i = found + ticks
                continue
            if c == "<":
                m = AUTOLINK.match(s, i)
                mail = None if m else EMAIL.match(s, i)
                if m or mail:
                    flush()
                    target = (m or mail).group(1)
                    nodes.append(("h", self.anchor(target if m else "mailto:" + target, esc(target))))
                    i = (m or mail).end()
                    continue
                buf.append(c)
                i += 1
                continue
            if c in "*_" or (c == "~" and s.startswith("~~", i)):
                j = i
                while j < n and s[j] == c:
                    j += 1
                prev = s[i - 1] if i > 0 else ""
                nxt = s[j] if j < n else ""
                left = not _is_space(nxt) and (not _is_punct(nxt) or _is_space(prev) or _is_punct(prev))
                right = not _is_space(prev) and (not _is_punct(prev) or _is_space(nxt) or _is_punct(nxt))
                if c == "_":
                    opens = left and (not right or _is_punct(prev))
                    closes = right and (not left or _is_punct(nxt))
                else:
                    opens, closes = left, right
                if c == "~" and j - i != 2:
                    buf.append(s[i:j])
                    i = j
                    continue
                flush()
                nodes.append(_Delim(c, j - i, opens, closes))
                i = j
                continue
            if c == "!" and i + 1 < n and s[i + 1] == "[":
                flush()
                stack.append([len(nodes), True, True, i + 2])
                nodes.append(("t", "!["))
                i += 2
                continue
            if c == "[":
                flush()
                stack.append([len(nodes), False, True, i + 1])
                nodes.append(("t", "["))
                i += 1
                continue
            if c == "]":
                flush()
                if not stack:
                    nodes.append(("t", "]"))
                    i += 1
                    continue
                idx, image, active, start = stack.pop()
                found = None
                if active:
                    found = self.link_tail(s, i + 1)
                    if found is None:
                        label = s[start:i]
                        if i + 1 < n and s[i + 1] == "[":
                            close = s.find("]", i + 2)
                            if close >= 0 and "[" not in s[i + 2:close]:
                                ref = self.refs.get(_norm(s[i + 2:close] or label))
                                if ref:
                                    found = (ref[0], ref[1], close + 1)
                        elif label.strip():
                            ref = self.refs.get(_norm(label))
                            if ref:
                                found = (ref[0], ref[1], i + 1)
                if found is None:
                    nodes.append(("t", "]"))
                    i += 1
                    continue
                dest, title, end = found
                kids = nodes[idx + 1:]
                self.emphasis(kids, False)
                inner = self.join(kids, False)
                del nodes[idx:]
                if image:
                    alt = plain(inner).strip() or "Image"
                    nodes.append(("h", self.anchor(dest, esc(alt), title)))
                else:
                    nodes.append(("h", self.anchor(dest, inner, title)))
                    for entry in stack:
                        if not entry[1]:
                            entry[2] = False
                i = end
                continue
            if c == "\n":
                text = "".join(buf)
                stripped = text.rstrip(" ")
                buf.clear()
                if stripped:
                    buf.append(stripped)
                flush()
                nodes.append(("h", "<br>\n") if len(text) - len(stripped) >= 2 else ("t", "\n"))
                i += 1
                while i < n and s[i] == " ":
                    i += 1
                continue
            buf.append(c)
            i += 1
        flush()
        self.emphasis(nodes, autolink)
        return self.join(nodes, autolink)

    # ------------------------------------------------------------- The blocks
    def blocks(self, blocks, tight=False, top=False):
        out = []
        for b in blocks:
            kind = b[0]
            if kind == "para":
                inner = self.inline(b[1])
                if top and not self.lead:
                    self.lead = " ".join(plain(inner).split())
                out.append(inner if tight else "<p>%s</p>" % inner)
            elif kind == "heading":
                inner = self.inline(b[2])
                text = " ".join(plain(inner).split())
                hid = self.unique(text)
                self.headings.append((b[1], hid, inner, text))
                out.append('<h%d id="%s">%s</h%d>' % (b[1], esc(hid), inner, b[1]))
            elif kind == "code":
                info = b[1].split()[0] if b[1].split() else ""
                lang = ' class="language-%s"' % esc(_unescape(info)) if info else ""
                label = "Code%s, scrolls sideways" % (" in " + esc(_unescape(info)) if info else "")
                out.append('<pre tabindex="0" role="group" aria-label="%s"><code%s>%s</code></pre>'
                           % (label, lang, esc(b[2])))
            elif kind == "hr":
                out.append("<hr>")
            elif kind == "quote":
                out.append("<blockquote>\n%s\n</blockquote>" % self.blocks(b[1]))
            elif kind == "list":
                ordered, start, tight_list, items = b[1], b[2], b[3], b[4]
                tag = "ol" if ordered else "ul"
                attr = ' start="%d"' % start if ordered and start != 1 else ""
                lis = "".join("<li>%s</li>" % self.blocks(kids, tight=tight_list) for kids in items)
                out.append("<%s%s>%s</%s>" % (tag, attr, lis, tag))
            elif kind == "table":
                aligns, header, rows = b[1], b[2], b[3]

                def cell(tag, text, align):
                    style = ' style="text-align:%s"' % align if align else ""
                    return "<%s%s>%s</%s>" % (tag, style, self.inline(text), tag)

                head = "".join(cell("th", c, a) for c, a in zip(header, aligns))
                body = "".join("<tr>%s</tr>" % "".join(
                    cell("td", (row + [""] * len(aligns))[k], a) for k, a in enumerate(aligns)) for row in rows)
                out.append('<div class="tab c%d" tabindex="0" role="group" aria-label="Table, scrolls sideways">'
                           "<table><thead><tr>%s</tr></thead>%s</table></div>"
                           % (min(len(aligns), 6), head, "<tbody>%s</tbody>" % body if body else ""))
        return "\n".join(out)


def render(text, link=None, codelink=None):
    """The page of one Markdown document."""
    lines = text.replace("\r\n", "\n").replace("\r", "\n").expandtabs(4).split("\n")
    parser = _Parser()
    blocks, _ = parser.blocks(lines)
    r = _Renderer(parser.refs, link, codelink)
    title = title_md = title_id = None
    for k, b in enumerate(blocks):
        if b[0] == "heading" and b[1] == 1:
            inner = r.inline(b[2])
            title, title_md = (inner, " ".join(plain(inner).split())), b[2]
            title_id = r.unique(title[1])
            del blocks[k]
            break
    body = r.blocks(blocks, top=True)
    return Page(title, title_md, title_id, body, r.headings, r.lead)


def escape_md(text):
    """Text with every character that Markdown reads as syntax escaped, so it renders as itself."""
    return re.sub(r"([\\`*_\[\]<>~#!|])", r"\\\1", text)


def inline_html(text, link=None):
    """One line of Markdown as inline HTML, such as a part of a title."""
    return _Renderer({}, link).inline(text)


# What render() has to write, case by case: the blocks, the inlines, the escaping of raw HTML and
# of unsafe links, and the ids of GitHub. self_test() renders each case and compares the body.
CASES = (
    ("A *b* **c** _d_ __e__ ~~f~~.", "<p>A <em>b</em> <strong>c</strong> <em>d</em> <strong>e</strong> <del>f</del>.</p>"),
    ("snake_case_name and 2*3*4", "<p>snake_case_name and 2<em>3</em>4</p>"),
    ("**DOCUMENTED** means *stated*", "<p><strong>DOCUMENTED</strong> means <em>stated</em></p>"),
    ("***both***", "<p><em><strong>both</strong></em></p>"),
    ("Code `a <b> & c` and ``x ` y``.", "<p>Code <code>a &lt;b&gt; &amp; c</code> and <code>x ` y</code>.</p>"),
    ("A <name> tag &amp; <b>bold</b>", "<p>A &lt;name&gt; tag &amp;amp; &lt;b&gt;bold&lt;/b&gt;</p>"),
    ("[A](x.md#y \"T\") and [b](javascript:alert(1))", '<p><a href="x.md#y" title="T">A</a> and <span class="nolink">b</span></p>'),
    ("See [one][r] and [r].\n\n[r]: https://example.org/a_b",
     '<p>See <a href="https://example.org/a_b" rel="noopener noreferrer">one</a> and '
     '<a href="https://example.org/a_b" rel="noopener noreferrer">r</a>.</p>'),
    ("Go to https://example.org/x. Or www.example.org", '<p>Go to <a href="https://example.org/x" rel="noopener noreferrer">'
     'https://example.org/x</a>. Or <a href="http://www.example.org" rel="noopener noreferrer">www.example.org</a></p>'),
    ("<https://aosabi.org/>", '<p><a href="https://aosabi.org/">https://aosabi.org/</a></p>'),
    ("![A figure](f.png)", '<p><a href="f.png">A figure</a></p>'),
    ("Line one  \nline two\\\nthree", "<p>Line one<br>\nline two<br>\nthree</p>"),
    ("# Title\n\n## 1. Tokens\n\n## 1. Tokens\n\n### `graphtpFetch`, the >128 B rule",
     '<h2 id="1-tokens">1. Tokens</h2>\n<h2 id="1-tokens-1">1. Tokens</h2>\n'
     '<h3 id="graphtpfetch-the-128-b-rule"><code>graphtpFetch</code>, the &gt;128 B rule</h3>'),
    ("Setext\n---\n\n---", '<h2 id="setext">Setext</h2>\n<hr>'),
    ("- a\n- b\n  - c\n\n1. x\n2. y", "<ul><li>a</li><li>b\n<ul><li>c</li></ul></li></ul>\n<ol><li>x</li><li>y</li></ol>"),
    ("- a\n\n- b", "<ul><li><p>a</p></li><li><p>b</p></li></ul>"),
    ("3. three\n4. four", '<ol start="3"><li>three</li><li>four</li></ol>'),
    ("Text\n- item", "<p>Text</p>\n<ul><li>item</li></ul>"),
    ("Text\n2. not a list", "<p>Text\n2. not a list</p>"),
    ("- item\n  ```rust\n  let a = 1;\n  ```", '<ul><li>item\n<pre tabindex="0" role="group" aria-label="Code in rust, '
     'scrolls sideways"><code class="language-rust">let a = 1;\n</code></pre></li></ul>'),
    ("```\n<x> & y\n```", '<pre tabindex="0" role="group" aria-label="Code, scrolls sideways"><code>&lt;x&gt; &amp; y\n</code></pre>'),
    ("> Quote\nlazy\n\n> - a", "<blockquote>\n<p>Quote\nlazy</p>\n</blockquote>\n<blockquote>\n<ul><li>a</li></ul>\n</blockquote>"),
    ("| A | B |\n| :-- | --: |\n| `x \\| y` | 2 |\n| 3 |", '<div class="tab c2" tabindex="0" role="group" aria-label="Table, '
     'scrolls sideways"><table><thead><tr><th style="text-align:left">A</th><th style="text-align:right">B</th></tr>'
     '</thead><tbody><tr><td style="text-align:left"><code>x | y</code></td><td style="text-align:right">2</td></tr>'
     '<tr><td style="text-align:left">3</td><td style="text-align:right"></td></tr></tbody></table></div>'),
    ("Lead line\n| A |\n| --- |\n| b |", '<p>Lead line</p>\n<div class="tab c1" tabindex="0" role="group" aria-label="Table, '
     'scrolls sideways"><table><thead><tr><th>A</th></tr></thead><tbody><tr><td>b</td></tr></tbody></table></div>'),
    ("    indented code", '<pre tabindex="0" role="group" aria-label="Code, scrolls sideways"><code>indented code\n</code></pre>'),
    ("[link *with* `code`](a.md) \\*not\\*", '<p><a href="a.md">link <em>with</em> <code>code</code></a> *not*</p>'),
    ("[outer [inner](b.md)](a.md)", '<p>[outer <a href="b.md">inner</a>](a.md)</p>'),
    ("Brackets [x] and a[i] stay", "<p>Brackets [x] and a[i] stay</p>"),
)


def self_test():
    """The cases of CASES that render differently than they must, as (case, got, want)."""
    bad = []
    for text, want in CASES:
        got = render(text).body
        if got != want:
            bad.append((text, got, want))
    return bad


if __name__ == "__main__":
    import sys
    failures = self_test()
    for text, got, want in failures:
        print("case %r\n  got  %r\n  want %r" % (text, got, want))
    print("docs_markdown: %d cases, %d rendered wrong" % (len(CASES), len(failures)))
    sys.exit(1 if failures else 0)
