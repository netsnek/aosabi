# The project site

This folder is the site of Aosabi at aosabi.org. Its first section is the documentation, which is
rendered from the folder Documentation of this repository, so the pages and the code and documents
they describe change together. The home page and the pages that explain the carriers are still to
come.

## Layout

- The folder `public/` is the deployed tree: plain HTML and CSS, no JavaScript, no web font and no
  request to another origin. It holds the logo, the favicon and `learn.css`, the stylesheet of the
  design system of the site with its tokens, its type and its components. The docs section,
  `public/docs/` and `public/docs.css`, is generated from `Documentation/` at every build and never
  committed.
- The file `assets-src/shell.html` holds what every page shares: the favicon, the brand of the
  header and the footer.
- The module `assets-src/glyphs.py` draws the glyph sprite, every icon of the site at 24 by 24
  pixels in strokes of the current colour.
- The script `assets-src/build_docs.py` generates the docs section, one page per Markdown file of
  `Documentation/` and an index, with the renderer `assets-src/docs_markdown.py`, written with the
  standard library of Python alone, and the rules of `assets-src/docs_extra.css`, which it appends
  to `learn.css` to make `public/docs.css`.
- The file `EDITING.md` designs editing the documentation on the site itself, as in a wiki.

## The docs section

Every Markdown file of `Documentation/` becomes one page under `/docs/`, at the address its path
gives it. The file `Documentation/graphtp.md` is the page `/docs/graphtp/`, a file in a folder keeps
the folder, as in `/docs/decisions/ADR-0027-graphtp/`, and the `README.md` of a folder is the page
of that folder. The page `/docs/` is the index: the `README.md` of `Documentation/` when there is
one, then every page, grouped by its folder, with its file and its first sentence.

Every page carries the line Edit on GitHub, which opens its file in the editor of GitHub, and the
line History, which lists every commit that changed it. A person with write access commits there,
and anybody else gets a fork and a pull request from GitHub. The file `EDITING.md` designs the
next step, editing a page in place on the site.

The builder follows five rules:

1. The pages take the brand, the favicon and the footer out of `assets-src/shell.html`, the glyph
   sprite and the kicker from `assets-src/glyphs.py`, and the design tokens from `learn.css`, so a
   docs page cannot drift from the rest of the site.
2. The Markdown is GitHub Flavored Markdown without raw HTML. A tag that a document writes is shown
   as text, a link with a scheme other than http, https or mailto is shown as its text, and an
   image is shown as a link to the image, so a page loads nothing it does not carry.
3. The id of a heading is the one GitHub gives it, so a link to a section works on the site as on
   GitHub. The first heading of level 1 is the title of the page, and a block of front matter at
   the top of a file may give the title and the description of the page instead.
4. A link to a Markdown file of `Documentation/` goes to its page, a link to any other file or
   folder of the repository goes to it on GitHub, and a code span that is exactly such a path links
   the same way. A link that names nothing in the repository is shown as its text and counted.
5. The renderer tests itself on its cases before every build, and the build fails on two files
   that would share a page, on a file that is not UTF-8 and on any word of its own that breaks the
   house style of the site.

## How the pages are regenerated

From the repository root:

```
python3 website/assets-src/build_docs.py
```

The builder needs Python 3 and nothing else.

The script `build_docs.py` writes `public/docs/` and `public/docs.css` in place. With `--site DIR`
it writes the whole site into another folder instead, a copy of `public/` with the docs section in
it, ready to be served. With `--strict` a link that names nothing in the repository, or a section
that its page does not have, fails the build, and `--report FILE` writes the counts and the house
style of every page as JSON. The script replaces a folder only when it wrote that folder itself.

To look at the result, serve the folder with any static server, for example
`python3 -m http.server --directory website/public`, since the pages link their stylesheet by an
absolute path, which is also how Cloudflare Pages serves them.

## How the site is deployed

The site is a Cloudflare Pages project. A deploy runs the builder and then uploads `public/` as it
is. The docs section is part of every deploy, so a commit to `Documentation/` reaches the site with
the next one.
