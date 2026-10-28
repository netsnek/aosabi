#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Build the docs section of the site from the folder Documentation of the repository.

    python3 website/assets-src/build_docs.py [--root DIR] [--site DIR] [--strict] [--report FILE]

Every Markdown file of Documentation/ becomes one page under /docs/, in the address its path
gives it: Documentation/graphtp.md is /docs/graphtp/, Documentation/decisions/ADR-0027-graphtp.md
is /docs/decisions/ADR-0027-graphtp/, and the README.md of a folder is the page of the folder.
/docs/ itself is the index: the README.md of Documentation/ when there is one, then every page,
grouped by its folder, with its path and its first sentence. Every page links to its file in the
editor of GitHub and to the history of the file, so the documentation is edited where it lives.
The first heading of level 1 is the title of a page. A file may start with a block of front matter
between two lines of three hyphens, and its keys title and description then stand in for a missing
heading and for the first sentence, while the block itself stays off the page.

The pages are the same design system as the rest of the site: the brand of the header, the favicon
and the footer are lifted out of shell.html beside this script, the glyph sprite and its icons come
from glyphs.py, and docs.css is public/learn.css, the stylesheet of the design system, followed by
docs_extra.css. One stylesheet, inline SVG, no script, no external resource. The Markdown is
rendered by docs_markdown.py, which escapes every tag a document writes.

A link of a document is rewritten to where it leads on the site. A link to another Markdown file of
Documentation/ goes to its page, with its section when it names one, a link to any other file or
folder of the repository goes to that file or folder on GitHub, and a link that leaves the
repository or names a path that does not exist is shown as its text alone and counted. A link to a
section that the page it names does not have is counted as well. With --strict either count fails
the build. The documents also name files in code, as in `decisions/ADR-0030-single-pass.md` or
`crates/graphtp/src/body.rs`, so a code span that is exactly such a path, with a slash in it or as
the name of a Markdown file beside the page, links to that file the same way when it exists.

Without --site the pages are written into website/public/docs/ and website/public/docs.css, which
are build output and never committed. With --site the whole site is written into DIR: a copy of
website/public/ with the docs section in it, ready to be served or deployed as it is. Either way a
folder is replaced only when this script wrote it, which the generator line of its index page
says. The first line printed names the counts, and --report writes them as JSON.

The renderer tests itself on its cases before it reads a document, and a build fails on a page that
two files would share, on a file that is not UTF-8, and on a shell text of its own that breaks the
house style of the site.
"""

from __future__ import annotations

import argparse
import html
import json
import os
import posixpath
import re
import shutil
import sys
import urllib.parse
from pathlib import Path

# The builder reads the repository and writes nothing into it but its output, so it leaves no
# bytecode beside the modules it imports.
sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent                  # The folder website/assets-src
sys.path.insert(0, str(HERE))
import docs_markdown as md                               # noqa: E402

REPOSITORY = "netsnek/aosabi"
BRANCH = "main"
DOCS = "Documentation"
BASE = "/docs/"
GENERATOR = "website/assets-src/build_docs.py"
CSS_NAME = "docs.css"

# The words of the shell are checked against the house style of the site, as the learn builder
# checks its own: no dash, no semicolon, no exclamation mark, the third person, and a capital at
# the start of every label.
BAD = ("\u2014", "\u2013", ";", "!")
VOICE = re.compile(r"\b((?i:you|your|yours|yourself|we|our|ours|me|my|mine)|I)\b")
# Every word the builder writes itself. check_words() holds them to the house style before a build.
WORDS = {
    "nav": "All docs",
    "edit": "Edit on GitHub",
    "new": "Add a page on GitHub",
    "history": "History",
    "toc": "On this page, %d sections",
    "pager": "The other pages of the documentation",
    "all": "All pages",
    "count": ("%d page", "%d pages"),
    "top": "Documentation",
    "title": "%s | Aosabi docs",
    "index_title": "Docs | Aosabi",
    "index_tone": "The documentation",
    "index_rest": "of Aosabi",
    "index_lead": ("Every page of this section is a file of the folder %s of the repository, rendered as it "
                   "stands on the branch %s. The link Edit on GitHub of a page opens its file in the editor of "
                   "GitHub, and the link History lists every change to it."),
    "index_desc": "The documentation of Aosabi, rendered from the folder %s of its repository.",
}
# The house style of the documents themselves is counted and reported, never enforced here: a
# dash or a semicolon in running text, and a heading that begins with a word in lower case.
PROSE_MARKS = {"em dash": "\u2014", "en dash": "\u2013", "semicolon": ";"}


class BuildError(Exception):
    """A failure of the build, printed without a traceback."""


def esc(s):
    return html.escape(s, quote=True)


def github(kind, path=""):
    """An address on GitHub: blob, tree, edit, commits or new, for a path of the repository."""
    quoted = urllib.parse.quote(path, safe="/")
    return "https://github.com/%s/%s/%s/%s" % (REPOSITORY, kind, BRANCH, quoted) if path else \
        "https://github.com/%s/%s/%s" % (REPOSITORY, kind, BRANCH)


# ---------------------------------------------------------------------- The site
class Site:
    """What the pages take from the rest of the site."""

    def __init__(self, root):
        self.root = root
        website = root / "website"
        public = website / "public"
        assets = website / "assets-src"
        for need in (assets / "shell.html", assets / "glyphs.py", public / "learn.css"):
            if not need.is_file():
                raise BuildError("%s is missing, the docs section is built into a site that has it" % need)
        sys.path.insert(0, str(assets))
        import glyphs                                    # noqa: E402  The sprite and the icons
        self.glyphs = glyphs
        shell = (assets / "shell.html").read_text(encoding="utf-8")
        self.brand = self.lift(shell, r'<a class="brand".*?</a>', "the brand")
        self.favicon = self.lift(shell, r'<link rel="icon"[^>]*>', "the favicon")
        # The footer is the one component every page ends with. An anchor of its own points at a
        # section of the page the site serves at its root, so it gains the leading slash here.
        footer = self.lift(shell, r'<footer class="site-footer".*?</footer>', "the footer")
        self.footer = re.sub(r'href="#', 'href="/#', footer)
        self.public = public
        self.css = (public / "learn.css").read_text(encoding="utf-8") + (HERE / "docs_extra.css").read_text(encoding="utf-8")

    @staticmethod
    def lift(text, pattern, what):
        m = re.search(pattern, text, re.S)
        if not m:
            raise BuildError("website/assets-src/shell.html holds no %s" % what)
        return m.group(0)

    def kick(self, text):
        """The kicker above a heading: the cross of the glyph sprite and a short line."""
        return '<p class="kick">%s<span>%s</span></p>' % (self.glyphs.icon("x"), text)


# ---------------------------------------------------------------------- The documents
class Doc:
    """One Markdown file of Documentation/ and the page it becomes."""

    def __init__(self, rel):
        self.rel = rel                                  # The path in the repository, with forward slashes
        inner = rel[len(DOCS) + 1:]
        folder, name = posixpath.split(inner)
        self.folder = folder                            # The folder inside Documentation/, empty at the top
        self.readme = name.lower() == "readme.md"
        stem = name[:-3]
        self.url = BASE + (folder + "/" if folder else "") + ("" if self.readme else stem + "/")
        self.href = urllib.parse.quote(self.url)        # The address as a link writes it
        self.stem = stem
        self.page = None
        self.meta = {}                                  # The keys of its front matter, if it has one
        self.dead = []                                  # The destinations shown as text alone
        self.anchors = []                               # The address of a page and a fragment, to check

    @property
    def title(self):
        return self.page.title[1] if self.page.title else self.meta.get("title") or self.stem

    @property
    def lead(self):
        return self.meta.get("description") or self.page.lead

    def ids(self):
        out = {h[1] for h in self.page.headings}
        if self.page.title_id:
            out.add(self.page.title_id)
        return out


FRONT = re.compile(r"\A---[ \t]*\n(.*?)\n---[ \t]*\n", re.S)


def front_matter(text):
    """The keys of a front matter block at the top of a file, and the text without it. Only plain
    lines of the form key: value are read, which is what a title and a description need."""
    m = FRONT.match(text)
    if not m:
        return {}, text
    meta = {}
    for line in m.group(1).split("\n"):
        k = re.match(r"^([A-Za-z_][\w-]*):[ \t]*(.*?)[ \t]*$", line)
        if k:
            value = k.group(2)
            if len(value) >= 2 and value[0] == value[-1] and value[0] in "\"'":
                value = value[1:-1]
            meta[k.group(1).lower()] = value
    return meta, text[m.end():]


def documents(root):
    base = root / DOCS
    if not base.is_dir():
        raise BuildError("%s is missing" % base)
    docs, others = [], []
    for directory, dirs, files in os.walk(base):
        dirs[:] = sorted(d for d in dirs if not d.startswith("."))
        for name in sorted(files):
            rel = Path(directory, name).relative_to(root).as_posix()
            (docs if name.lower().endswith(".md") else others).append(rel)
    if not docs:
        raise BuildError("%s holds no Markdown file" % base)
    return [Doc(rel) for rel in docs], others


def order(docs):
    """The order of the index and the pager: the top folder first, then each folder by name, and in
    a folder its README, then the files whose name is in capitals, then the rest by name."""
    def key(d):
        return (d.folder != "", d.folder.lower(), not d.readme, not d.stem.isupper(), d.stem.lower())
    return sorted(docs, key=key)


# ---------------------------------------------------------------------- The links
class Links:
    """Where the destination of a link leads."""

    def __init__(self, root, docs):
        self.root = root
        self.by_path = {d.rel: d for d in docs}
        self.by_folder = {(DOCS + "/" + d.folder).rstrip("/"): d for d in docs if d.readme}

    def inside(self, rel):
        return rel != ".." and not rel.startswith("../") and not posixpath.isabs(rel)

    def resolve(self, doc, dest):
        """The address for one destination of a link in doc, or None to show its text alone."""
        if dest.startswith("#"):
            if len(dest) > 1:
                doc.anchors.append((doc.url, dest[1:]))
            return dest
        if re.match(r"^[A-Za-z][A-Za-z0-9+.-]*:", dest) or dest.startswith("//"):
            return dest
        path, _, fragment = dest.partition("#")
        path = urllib.parse.unquote(path.partition("?")[0])
        if not path:
            return dest
        if path.startswith("/"):
            rel = posixpath.normpath(path.lstrip("/"))
        else:
            rel = posixpath.normpath(posixpath.join(posixpath.dirname(doc.rel), path))
        if rel == DOCS:
            return BASE + ("#" + fragment if fragment else "")
        target = self.by_path.get(rel) or self.by_folder.get(rel.rstrip("/"))
        if target is not None and self.inside(rel):
            if fragment:
                doc.anchors.append((target.url, fragment))
            return target.href + ("#" + fragment if fragment else "")
        place = self.root / rel
        if self.inside(rel) and rel != "." and (place.is_file() or place.is_dir()):
            return github("tree" if place.is_dir() else "blob", rel) + ("#" + fragment if fragment else "")
        doc.dead.append(dest)
        return None

    def code(self, doc, text):
        """The address for a code span that names a file or a folder of the repository: a path with
        a slash, read from the folder of the page and else from the root, or the name of a Markdown
        file beside the page. Anything else stays code, and nothing here counts as dead."""
        if not re.fullmatch(r"(?:\.{1,2}/)*[\w@+-][\w.@+-]*(?:/[\w.@+-]+)*/?", text):
            return None
        if "/" not in text.rstrip("/") and not text.endswith(".md"):
            return None
        bases = [posixpath.dirname(doc.rel)] + ([""] if "/" in text.rstrip("/") else [])
        for base in bases:
            rel = posixpath.normpath(posixpath.join(base, text))
            if not self.inside(rel) or rel == ".":
                continue
            if rel == DOCS:
                return BASE
            target = self.by_path.get(rel) or self.by_folder.get(rel)
            if target is not None:
                return target.href
            place = self.root / rel
            if place.is_file() or place.is_dir():
                return github("tree" if place.is_dir() else "blob", rel)
        return None


# ---------------------------------------------------------------------- The shell
SHELL = """<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>%(title)s</title>
<meta name="description" content="%(desc)s">
<meta name="theme-color" content="#ffffff">
<meta name="generator" content="%(generator)s">
%(favicon)s
<link rel="stylesheet" href="/%(css)s">
</head>
<body>
%(sprite)s
<header class="site-head">
  %(brand)s
  <nav><a href="%(base)s">%(nav)s</a></nav>
</header>
<main id="content" class="doc docs"><div class="wrap">
%(head)s
<article class="prose">
%(body)s
</article>
%(after)s
</div></main>
%(footer)s
</body>
</html>
"""


def sentence(text, limit=220):
    """The first sentence of a text, clipped to the limit."""
    text = " ".join(text.split())
    m = re.search(r"(?<=[.?!])\s+(?=[A-Z0-9])", text)
    first = text[:m.start()] if m else text
    return first if len(first) <= limit else first[:limit - 1].rstrip(" ,") + "\u2026"


def split_title(raw):
    """The two lines of the heading recipe of the site: a grey first line and a second line in ink.
    A title splits at its first colon, which the line break replaces, or else at its first comma,
    which the grey line keeps. A title without either is one line."""
    code = [(m.start(), m.end()) for m in re.finditer(r"`[^`]*`", raw)]

    def outside(i):
        return not any(a <= i < b for a, b in code)

    for mark, keep in ((": ", False), (", ", True)):
        for m in re.finditer(re.escape(mark), raw):
            if outside(m.start()) and raw[:m.start()].strip() and raw[m.end():].strip():
                return raw[:m.start()] + ("," if keep else ""), raw[m.end():]
    return None, raw


def heading(raw_title, plain_title, title_id):
    tone, rest = split_title(raw_title)
    stop = "" if re.search(r"[.?!]$", plain_title) else "<i>.</i>"
    line = md.inline_html(rest) + stop
    if tone:
        line = '<span class="tone">%s</span><br>%s' % (md.inline_html(tone), line)
    return '<h1 id="%s">%s</h1>' % (esc(title_id), line)


def doc_meta(rel, go, edit=None, label=None):
    """The line under the title: the file, the link to the editor of GitHub, the history."""
    return ('<div class="docmeta"><span class="path">%s</span><a class="edit" href="%s">%s %s</a>'
            '<a href="%s">%s</a></div>'
            % (esc(rel), esc(edit or github("edit", rel)), label or WORDS["edit"], go,
               esc(github("commits", rel.rstrip("/"))), WORDS["history"]))


def toc(headings):
    """The sections of a page, its headings of level 2, folded."""
    rows = [h for h in headings if h[0] == 2]
    if len(rows) < 2:
        return ""
    items = "".join('<li><a href="#%s">%s</a></li>' % (esc(h[1]), re.sub(r"</?a\b[^>]*>", "", h[2])) for h in rows)
    return ('<details class="toc"><summary>%s</summary><ul>%s</ul></details>'
            % (WORDS["toc"] % len(rows), items))


def check_words():
    """The house style on the words the builder and its renderer write themselves: no dash, no
    semicolon, no exclamation mark, the third person, and a capital at the start of each."""
    texts = []
    for key, value in WORDS.items():
        if key != "index_rest":                         # The second line of a heading goes on its first
            texts.extend(value if isinstance(value, tuple) else (value,))
    texts += ["Code, scrolls sideways", "Code in rust, scrolls sideways", "Table, scrolls sideways"]
    for text in texts:
        words = text.replace("%s", "Documentation").replace("%d", "2")
        for bad in BAD:
            if bad in words:
                raise BuildError("the words %r of the builder hold %r, which the house style forbids" % (text, bad))
        m = VOICE.search(words)
        if m:
            raise BuildError("the words %r of the builder speak in the first or second person" % text)
        if not (words[:1].isupper() or words[:1].isdigit()):
            raise BuildError("the words %r of the builder do not start with a capital" % text)


# A word that keeps its spelling, so a heading may begin with it in lower case: a name of code, a
# path, a number or a unit, a word with a capital after its first letter, or a call.
NOTATION = re.compile(r"[\d._/+<>:#@=()\[\]]|^.\S*[A-Z]")


def starts_in_lower_case(words):
    """Whether a heading begins with a word in lower case that is no notation."""
    first = words.split()[0].strip("`'\",") if words.split() else ""
    return first[:1].islower() and NOTATION.search(first) is None


def style_counts(body, headings):
    """The house style of a document, counted on its running text outside code."""
    text = md.plain(re.sub(r"<pre.*?</pre>|<code>.*?</code>", " ", body, flags=re.S))
    counts = {name: text.count(mark) for name, mark in PROSE_MARKS.items()}
    lower = [h[3] for h in headings if starts_in_lower_case(h[3])]
    counts["headings in lower case"] = len(lower)
    return counts, lower


# ---------------------------------------------------------------------- The build
def build(root, out, strict=False):
    bad = md.self_test()
    if bad:
        raise BuildError("the Markdown renderer fails %d of its cases, the first: %r" % (len(bad), bad[0][0]))
    check_words()
    site = Site(root)
    docs, others = documents(root)
    docs = order(docs)

    seen = {}
    for d in docs:
        key = d.url.lower()
        if key in seen:
            raise BuildError("%s and %s would both be the page %s" % (seen[key].rel, d.rel, d.url))
        seen[key] = d

    links = Links(root, docs)
    for d in docs:
        try:
            text = (root / d.rel).read_text(encoding="utf-8")
        except UnicodeDecodeError as error:
            raise BuildError("%s is not UTF-8: %s" % (d.rel, error)) from None
        d.meta, text = front_matter(text)
        d.page = md.render(text, link=lambda dest, d=d: links.resolve(d, dest),
                           codelink=lambda span, d=d: links.code(d, span))

    by_url = {d.url: d for d in docs}
    missing = []
    for d in docs:
        for url, fragment in d.anchors:
            target = by_url.get(url)
            frag = urllib.parse.unquote(fragment).lower()
            if target is not None and frag not in target.ids() and not re.fullmatch(r"l\d+(-l\d+)?", frag):
                missing.append((d.rel, url + "#" + fragment))

    go = site.glyphs.icon("go")
    folders = {}
    for d in docs:
        folders.setdefault(d.folder, []).append(d)

    def group_title(folder):
        if not folder:
            return WORDS["top"]
        readme = next((d for d in folders.get(folder, []) if d.readme), None)
        if readme is not None and readme.page.title:
            return readme.page.title[1]
        return posixpath.basename(folder).replace("-", " ").replace("_", " ").capitalize()

    docs_dir = out / "docs"
    pages = {}
    report = []
    for i, d in enumerate(docs):
        prev = docs[i - 1] if i else None
        nxt = docs[i + 1] if i + 1 < len(docs) else None
        if d.rel == DOCS + "/README.md":
            continue                                    # The README of Documentation is the index
        kicker = group_title(d.folder if not d.readme else posixpath.dirname(d.folder))
        title_md = d.page.title_md if d.page.title else md.escape_md(d.title)
        head = "\n".join((
            site.kick(esc(kicker)),
            heading(title_md, d.title, d.page.title_id or md.slug(d.stem)),
            doc_meta(d.rel, go),
            toc(d.page.headings),
        ))
        nav = []
        if prev is not None:
            nav.append('<a href="%s">&larr; %s</a>' % (esc(prev.href), esc(prev.title)))
        if nxt is not None:
            nav.append('<a class="next" href="%s">%s &rarr;</a>' % (esc(nxt.href), esc(nxt.title)))
        after = '<nav class="pager" aria-label="%s">%s</nav>' % (WORDS["pager"], "".join(nav))
        shell = {"title": esc(WORDS["title"] % d.title), "desc": esc(sentence(d.lead) or d.title),
                 "generator": GENERATOR, "favicon": site.favicon, "css": CSS_NAME, "sprite": site.glyphs.sprite(),
                 "brand": site.brand, "base": BASE, "nav": WORDS["nav"], "head": head, "body": "%(body)s",
                 "after": after, "footer": site.footer}
        pages[d.url] = (SHELL % shell).replace("%(body)s", d.page.body)
        counts, lower = style_counts(d.page.body, d.page.headings)
        report.append({"file": d.rel, "page": d.url, "title": d.title, "headings": len(d.page.headings),
                       "dead_links": d.dead, "style": counts, "lower_case_headings": lower})

    # The index: the README of Documentation, then every page by its folder
    readme = next((d for d in docs if d.rel == DOCS + "/README.md"), None)
    groups = []
    for folder in sorted(folders, key=lambda f: (f != "", f.lower())):
        rows = []
        for d in folders[folder]:
            if d is readme:
                continue
            rows.append('<li><div><a href="%s">%s</a><span class="path">%s</span></div><p>%s</p></li>'
                        % (esc(d.href), esc(d.title), esc(d.rel), esc(sentence(d.lead, 200))))
        if rows:
            count = WORDS["count"][len(rows) != 1] % len(rows)
            groups.append('<h2 id="%s">%s <span class="count">%s</span></h2><ul class="docs-index">%s</ul>'
                          % (esc(md.slug("all " + (folder or "documentation"))), esc(group_title(folder)), count, "".join(rows)))
    if readme is not None:
        title_md = readme.page.title_md if readme.page.title else md.escape_md(readme.meta.get("title") or WORDS["top"])
        head = "\n".join((site.kick(WORDS["top"]), heading(title_md, readme.title, readme.page.title_id or "documentation"),
                          doc_meta(readme.rel, go)))
        body = readme.page.body
        lead = sentence(readme.lead)
    else:
        head = "\n".join((
            site.kick(WORDS["top"]),
            '<h1><span class="tone">%s</span><br>%s<i>.</i></h1>' % (WORDS["index_tone"], WORDS["index_rest"]),
            doc_meta(DOCS + "/", go, edit=github("new", DOCS), label=WORDS["new"]),
        ))
        body = '<div class="lead"><p>%s</p></div>' % esc(WORDS["index_lead"] % (DOCS, BRANCH))
        lead = WORDS["index_desc"] % DOCS
    after = '<section class="all" aria-label="%s">%s</section>' % (WORDS["all"], "".join(groups))
    shell = {"title": WORDS["index_title"], "desc": esc(lead), "generator": GENERATOR, "favicon": site.favicon,
             "css": CSS_NAME, "sprite": site.glyphs.sprite(), "brand": site.brand, "base": BASE, "nav": WORDS["nav"],
             "head": head, "body": "%(body)s", "after": after, "footer": site.footer}
    pages[BASE] = (SHELL % shell).replace("%(body)s", body)
    if readme is not None:
        counts, lower = style_counts(readme.page.body, readme.page.headings)
        report.append({"file": readme.rel, "page": BASE, "title": readme.title, "headings": len(readme.page.headings),
                       "dead_links": readme.dead, "style": counts, "lower_case_headings": lower})

    # A fresh folder beside the old one, then the swap, so a failed build leaves the old one
    fresh = docs_dir.with_name(docs_dir.name + ".building")
    if fresh.exists():
        shutil.rmtree(fresh)
    for url, text in pages.items():
        place = fresh / url[len(BASE):] / "index.html"
        place.parent.mkdir(parents=True, exist_ok=True)
        place.write_text(text, encoding="utf-8")
    if docs_dir.exists():
        if not ours(docs_dir):
            shutil.rmtree(fresh)
            raise BuildError("%s exists and was not written by this script, so it stays" % docs_dir)
        shutil.rmtree(docs_dir)
    fresh.rename(docs_dir)
    (out / CSS_NAME).write_text(site.css, encoding="utf-8")

    dead = [(r["file"], x) for r in report for x in r["dead_links"]]
    totals = {"pages": len(pages), "files": len(docs), "other_files": others, "dead_links": len(dead),
              "missing_sections": len(missing),
              "style": {k: sum(r["style"][k] for r in report) for k in list(PROSE_MARKS) + ["headings in lower case"]}}
    summary = {"totals": totals, "missing_sections": missing, "pages": report}
    if strict and (dead or missing):
        raise BuildError("%d dead links and %d links to missing sections, the first: %s"
                         % (len(dead), len(missing), (dead or missing)[0]))
    return summary


def ours(folder):
    index = folder / "index.html"
    return index.is_file() and ('<meta name="generator" content="%s">' % GENERATOR) in index.read_text(encoding="utf-8")


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--root", type=Path, default=HERE.parent.parent,
                        help="the repository, the one this script lies in by default")
    parser.add_argument("--site", type=Path, help="write the whole site into this folder instead of website/public")
    parser.add_argument("--strict", action="store_true", help="fail on a dead link or a link to a missing section")
    parser.add_argument("--report", type=Path, help="write the counts as JSON to this file")
    args = parser.parse_args(argv)
    root = args.root.resolve()
    try:
        if args.site is None:
            out = root / "website" / "public"
            summary = build(root, out, args.strict)
        else:
            out = args.site.resolve()
            public = (root / "website" / "public").resolve()
            if out == public or public in out.parents:
                raise BuildError("%s lies inside %s, which it would copy into itself" % (out, public))
            if out.exists() and not (out.is_dir() and ours(out / "docs")):
                raise BuildError("%s exists and is no site this script wrote, so it stays" % out)
            fresh = out.with_name(out.name + ".building")
            if fresh.exists():
                shutil.rmtree(fresh)
            shutil.copytree(public, fresh)
            try:
                summary = build(root, fresh, args.strict)
            except BuildError:
                shutil.rmtree(fresh)
                raise
            if out.exists():
                shutil.rmtree(out)
            fresh.rename(out)
    except BuildError as error:
        print("build_docs: %s" % error, file=sys.stderr)
        return 1
    t = summary["totals"]
    print("build_docs: %d pages from %d files into %s, %d dead links, %d links to missing sections, "
          "house style: %s" % (t["pages"], t["files"], out, t["dead_links"], t["missing_sections"],
                               ", ".join("%d %s" % (v, k) for k, v in t["style"].items())))
    if t["other_files"]:
        print("build_docs: %d files of %s are no Markdown and get no page: %s"
              % (len(t["other_files"]), DOCS, ", ".join(t["other_files"][:10])))
    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(summary, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
