// SPDX-License-Identifier: MIT
/*
 * sabi-theme.typ: the visual language of the generated documents as Typst
 * functions.
 *
 * The generated files hold data only and every drawing function lives here,
 * so the whole visual language is restyled by editing this one file, which a
 * rerun keeps (DOCUMENTED, docs/documentation-generator.md sections 3 and
 * 8.1).
 */

// The file carries no schema text. Every string of a schema reaches these
// functions as a string literal of a generated file, and a string is shown
// as text and never read as markup, so no description runs as Typst
// (DOCUMENTED, docs/documentation-generator.md section 8.2).
//
// A chapter imports the functions with `#import "/sabi-theme.typ": *`, and
// the main file applies the page setup with `#show: sv-setup`, or
// `#show: sv-setup.with(dark: true)` for a document in the dark theme
// (DOCUMENTED, docs/documentation-generator.md section 9, paper is light by
// default).
//
// Every coordinate of a diagram is a length in points, a centipixel of the
// scene times 3/400, so one pixel of the scene is `sv-px`.

// The monospace face of every diagram, of every name and of every code span.
// Sabi Mono is the ASCII subset of this face, so a box drawn here has the
// widths the layout measured (DOCUMENTED, docs/aoviewer.md section 4.1, and
// PROVEN, typst-assets 0.15.1, files/fonts/).
#let sv-mono = "DejaVu Sans Mono"

// The face of the prose. The Typst asset crate carries no proportional sans
// text face, and Libertinus Serif is one of its two text faces, NewCM10 the
// other (PROVEN, typst-assets 0.15.1, src/lib.rs, `fonts()`, which bundles
// Libertinus Serif, NewCM10, NewCMMath and DejaVu Sans Mono alone). The Foxit
// Sans files beside them in files/fonts/ are the PDF standard fonts of its
// `pdf` module and are not offered to a document (PROVEN, the same file,
// module `pdf`).
#let sv-serif = "Libertinus Serif"

// One pixel of a scene, which is 0.75 pt, since a pixel is 1/96 inch and a
// point 1/72 inch (DOCUMENTED, CSS Values and Units Module Level 3, the
// absolute lengths).
#let sv-px = 0.75pt

// The measures of a box, of a chip and of an edge (DOCUMENTED,
// docs/aoviewer.md sections 4.1, 4.2 and 4.4, and ASSUMED where
// crates/aoviewer-core/src/render_svg/mod.rs marks its own measure so).
#let sv-header = 30 * sv-px
#let sv-row = 22 * sv-px
#let sv-padding = 12 * sv-px
#let sv-bar = 4 * sv-px
#let sv-radius = 6 * sv-px
#let sv-border = 1 * sv-px
#let sv-focus-border = 2 * sv-px
#let sv-text-size = 13 * sv-px
#let sv-chip-size = 11 * sv-px
#let sv-chip-padding = 8 * sv-px
#let sv-chip-radius = 4 * sv-px
#let sv-edge-width = 1.25 * sv-px
#let sv-head-length = 6 * sv-px
#let sv-head-half = 2.5 * sv-px
#let sv-mark-radius = 3 * sv-px
#let sv-corner = 10 * sv-px
#let sv-hatch-step = 8 * sv-px
#let sv-double-inset = 3 * sv-px
#let sv-pill-inset = 3 * sv-px
#let sv-notch-depth = 5 * sv-px
#let sv-notch-half = 4 * sv-px

// The cells a header gives the letter badge, the cells between the name and
// the type of a row, and the cells of one mark of a row (PROVEN,
// BADGE_CELLS, GAP_CELLS and MARK_CELLS in
// crates/aoviewer-core/src/layout/measure.rs).
#let sv-badge-cells = 3
#let sv-gap-cells = 2
#let sv-mark-cells = 2

// Returns the advance of one glyph of the monospace face at `size`, 1233 of
// 2048 em (ASSUMED, docs/aoviewer.md section 4.1).
#let sv-cell(size) = size * 1233 / 2048

// The colours of both themes: the neutrals, the brand accent of links, and
// the accent and the header tint of each of the six kinds (DOCUMENTED,
// docs/aoviewer.md section 4.3).
#let sv-palettes = (
  light: (
    canvas: rgb("#F5F6F8"),
    card: rgb("#FFFFFF"),
    ink: rgb("#14181F"),
    muted: rgb("#56606E"),
    border: rgb("#8A93A1"),
    edge: rgb("#7A8494"),
    link: rgb("#4C7A00"),
    object: (accent: rgb("#0B66E4"), tint: rgb("#E7F0FC")),
    input: (accent: rgb("#136B50"), tint: rgb("#E7F0EE")),
    interface: (accent: rgb("#5B2488"), tint: rgb("#EFE9F3")),
    union: (accent: rgb("#806C14"), tint: rgb("#F2F0E8")),
    enum: (accent: rgb("#7A2106"), tint: rgb("#F2E9E6")),
    scalar: (accent: rgb("#2D3038"), tint: rgb("#EAEAEB")),
  ),
  dark: (
    canvas: rgb("#0F1216"),
    card: rgb("#1B1F27"),
    ink: rgb("#E8EBF0"),
    muted: rgb("#A0A9B6"),
    border: rgb("#6E7887"),
    edge: rgb("#7E8898"),
    link: rgb("#A6D639"),
    object: (accent: rgb("#5B9BFF"), tint: rgb("#27354E")),
    input: (accent: rgb("#9FF0C8"), tint: rgb("#334544")),
    interface: (accent: rgb("#CF8DB0"), tint: rgb("#3B3340")),
    union: (accent: rgb("#FFD93B"), tint: rgb("#44402B")),
    enum: (accent: rgb("#FF5A1F"), tint: rgb("#442A26")),
    scalar: (accent: rgb("#D5DAE6"), tint: rgb("#3C4149")),
  ),
)

// The letter badge of each kind (DOCUMENTED, docs/aoviewer.md section 4.3).
#let sv-letters = (
  object: "O",
  input: "IN",
  interface: "I",
  union: "U",
  enum: "E",
  scalar: "S",
)

// The theme of the document, which `sv-setup` sets and every drawing reads.
#let sv-mode = state("sv-mode", "light")

// Returns the palette of the theme of the document. It reads a state, so it
// is called inside `context`.
#let sv-palette() = sv-palettes.at(sv-mode.get())

// The page setup of the document, applied as `#show: sv-setup`.
//
// The page is an A4 landscape page with margins of 2 cm, a body of 728.5 pt
// by 481.9 pt (ASSUMED for the paper and the margins). The page budget of a
// diagram limits its width to 1400 px at 13 px text and says nothing of its
// height (DOCUMENTED, docs/documentation-generator.md section 6.2). Fitted to
// the width of the body alone, a diagram of that width prints its 13 px text
// at 6.76 pt, above the 6.5 pt the same section names, and the 11 px text of
// its chips and stub page numbers at 5.72 pt, below it. The height fit of
// `sv-diagram` scales a diagram taller than 614.4 pt, 819.2 px, below two
// thirds, so its 13 px text prints below 6.5 pt although it is inside the
// budget (ASSUMED, the arithmetic of these measures and of
// `sv-diagram-share`, with the 13 px and the 11 px DOCUMENTED,
// docs/aoviewer.md section 4.1).
#let sv-setup(dark: false, body) = {
  let mode = if dark { "dark" } else { "light" }
  let p = sv-palettes.at(mode)
  set page(
    paper: "a4",
    flipped: true,
    margin: 2cm,
    fill: if dark { p.canvas } else { none },
    numbering: "1",
  )
  set text(font: sv-serif, size: 11pt, fill: p.ink)
  set line(stroke: 0.5pt + p.border)
  set table(stroke: 0.5pt + p.border, inset: 5pt)
  show raw: set text(font: sv-mono)
  show raw.where(block: true): set block(
    fill: if dark { p.card } else { p.canvas },
    inset: 6pt,
    radius: 3pt,
    width: 100%,
  )
  show link: set text(fill: p.link)
  sv-mode.update(mode)
  body
}

// Returns the content of `items`, empty content when it is empty.
#let sv-join(items) = {
  let joined = items.join()
  if joined == none { [] } else { joined }
}

// Returns an array of inlines as content, every string as text.
#let sv-inlines(items) = sv-join(items.map(it => if type(it) == str { text(it) } else { it }))

// Places an anchor that `link(target)` jumps to and a stub counts the page
// of, an empty metadata element carrying the label `target`.
#let sv-anchor(target) = [#metadata(none)#target]

// The breadcrumb line of a page, its crumbs separated by a slash.
#let sv-crumbs(crumbs) = par(text(
  size: 0.85em,
  crumbs.map(it => if type(it) == str { text(it) } else { it }).join(text(" / ")),
))

// A mark: the badge of a kind, the filled mark of a deprecated member, the
// hollow mark of a member its description marks deprecated, the mark of a
// Relay connection and the mark of a root (DOCUMENTED, docs/aoviewer.md
// sections 3.10 and 4.3).
#let sv-mark(name) = context {
  let p = sv-palette()
  let r = 0.25em
  if name in sv-letters {
    let k = p.at(name)
    box(
      fill: k.tint,
      stroke: 0.5pt + k.accent,
      inset: (x: 3pt, y: 1.5pt),
      radius: 2pt,
      text(font: sv-mono, size: 0.7em, weight: "bold", fill: k.accent, name),
    )
  } else if name == "deprecated" {
    box(rect(width: 2 * r, height: 2 * r, radius: r, fill: p.ink))
  } else if name == "tag" {
    box(rect(width: 2 * r, height: 2 * r, radius: r, stroke: 0.6pt + p.ink))
  } else if name == "relay" {
    box(curve(
      fill: p.ink,
      curve.move((r, 0pt)),
      curve.line((2 * r, r)),
      curve.line((r, 2 * r)),
      curve.line((0pt, r)),
      curve.close(),
    ))
  } else if name == "root" {
    box(curve(
      fill: p.ink,
      curve.move((0pt, 0pt)),
      curve.line((2 * r, r)),
      curve.line((0pt, 2 * r)),
      curve.close(),
    ))
  }
}

// The title of a page or of a section of a root field, a heading of the
// document outline, with the badge of the kind of a type page after it.
#let sv-title(level, inlines, kind: none) = heading(level: level, {
  sv-inlines(inlines)
  if kind != none {
    h(0.5em)
    sv-mark(kind)
  }
})

// A heading of a description, whose level `sv-desc` moves below the heading
// that holds it, and the heading of a table, a name grid or the details of
// a page. Neither enters the document outline.
#let sv-h(level, inlines) = heading(
  depth: level,
  outlined: false,
  bookmarked: false,
  sv-inlines(inlines),
)

// A paragraph.
#let sv-p(inlines) = par(sv-inlines(inlines))

// Emphasis and strong emphasis.
#let sv-em(inlines) = emph(sv-inlines(inlines))
#let sv-strong(inlines) = strong(sv-inlines(inlines))

// A code span and a code block, shown as written.
#let sv-code(code) = raw(code)
#let sv-codeblock(lang, code) = raw(
  code,
  block: true,
  lang: if lang == "" { none } else { lang },
)

// A link to a URL, or to a label of the document.
#let sv-link(dest, inlines) = link(dest, sv-inlines(inlines))

// A hard line break and a thematic break.
#let sv-br() = linebreak()
#let sv-rule() = line(length: 100%)

// A block quote of an array of blocks.
#let sv-quote(blocks) = quote(block: true, sv-join(blocks))

// A list whose items are arrays of blocks, a bullet list when `start` is
// `none` and an ordered list from `start` otherwise.
#let sv-list(start, tight, items) = {
  let children = items.map(sv-join)
  if start == none {
    list(tight: tight, ..children)
  } else {
    enum(start: start, tight: tight, ..children)
  }
}

// Returns the alignment of a column of a table.
#let sv-align(align) = if align == "center" {
  center
} else if align == "right" {
  right
} else {
  start
}

// A table, its header an array of cells and each row an array of cells,
// every cell an array of inlines. A row is cut or filled to the columns of
// the header, as a GitHub table is (DOCUMENTED, the GitHub Flavored Markdown
// specification section 4.10).
#let sv-table(aligns, header, rows) = {
  let n = calc.max(1, header.len())
  let fit(cells) = {
    let kept = cells.slice(0, calc.min(n, cells.len())).map(sv-inlines)
    kept + ([],) * (n - kept.len())
  }
  set text(size: 0.9em)
  show table.cell.where(y: 0): strong
  table(
    columns: n,
    align: (x, y) => sv-align(aligns.at(x, default: none)),
    table.header(..fit(header)),
    ..rows.map(fit).flatten(),
  )
}

// A type reference: every run of wrappers a string and every name an array
// of its text and the label of its page, or `none` when no page documents
// it.
#let sv-type(parts) = sv-join(parts.map(part => if type(part) == str {
  raw(part)
} else if part.at(1) == none {
  raw(part.at(0))
} else {
  link(part.at(1), raw(part.at(0)))
}))

// A description, an array of blocks whose headings sit `level` levels below
// their own, which a table cell shows as strong text.
#let sv-desc(blocks, level: 0, cell: false) = {
  set heading(offset: level)
  if cell {
    show heading: it => strong(it.body)
    sv-join(blocks)
  } else {
    sv-join(blocks)
  }
}

// Wraps `body` in a link to `target` when the document holds that label
// once, so a diagram drawn apart from the page of a type stays valid.
#let sv-box-link(target, body) = if target == none {
  body
} else {
  context if query(target).len() == 1 { link(target, body) } else { body }
}

// The page number of the label `target`, through a query of the document,
// or nothing when the document does not hold it.
#let sv-stub(target) = context {
  let found = query(target)
  if found.len() == 1 {
    "p. " + str(counter(page).at(found.first().location()).first())
  }
}

// Returns the dash pattern of an edge in `style` (DOCUMENTED,
// docs/aoviewer.md section 4.4).
#let sv-dash(style) = if style == "union" {
  (6 * sv-px, 4 * sv-px)
} else if style == "implements" {
  (1 * sv-px, 4 * sv-px)
} else if style == "argument" {
  (6 * sv-px, 3 * sv-px, 1 * sv-px, 3 * sv-px)
} else {
  none
}

// A head whose tip is `tip`, pointing right when `dir` is 1 and left when it
// is -1, filled for a field and a union member and hollow otherwise. A head
// grows with the stroke it ends, so it stays wider than a trunk (ASSUMED, as
// the SVG writer draws it).
#let sv-head(tip, dir, style, width, p) = {
  let extra = calc.max(0pt, width - sv-edge-width)
  let length = sv-head-length + 2 * extra
  let half = sv-head-half + extra
  let base = tip.at(0) - dir * length
  let hollow = style == "implements" or style == "argument"
  place(curve(
    fill: if hollow { p.canvas } else { p.edge },
    stroke: if hollow { sv-edge-width + p.edge } else { none },
    curve.move(tip),
    curve.line((base, tip.at(1) - half)),
    curve.line((base, tip.at(1) + half)),
    curve.close(),
  ))
}

// An edge: its cubic curve, the head at its end unless a trunk draws it, and
// a second head at its start for a pair of opposite references. An edge
// without the control points `c1` and `c2` is drawn straight.
#let sv-edge(e, p) = {
  place(curve(
    stroke: (
      paint: p.edge,
      thickness: e.at("width", default: sv-edge-width),
      dash: sv-dash(e.style),
    ),
    curve.move(e.from),
    curve.cubic(e.at("c1", default: e.from), e.at("c2", default: e.to), e.to),
  ))
  if "head" in e { sv-head(e.to, e.head, e.style, sv-edge-width, p) }
  if "tail" in e { sv-head(e.from, e.tail, e.style, sv-edge-width, p) }
}

// A trunk: the last 24 px of the edges into one target in its width, and one
// head for all of them (DOCUMENTED, docs/aoviewer.md section 3.4, item 6).
#let sv-trunk(t, p) = {
  place(curve(
    stroke: (paint: p.edge, thickness: t.width),
    curve.move(t.from),
    curve.line(t.to),
  ))
  sv-head(t.to, t.head, t.style, t.width, p)
}

// Places `body` with its left side at `x`, centred on the height `mid`.
#let sv-run(x, mid, body) = place(dx: x, dy: mid - sv-row / 2, box(
  height: sv-row,
  align(left + horizon, body),
))

// Places a mark centred on `(x, y)`: a filled or hollow round mark, the
// diamond of a Relay connection, or the triangle of a root.
#let sv-glyph(name, x, y, p) = {
  let r = sv-mark-radius
  if name == "relay" {
    place(curve(
      fill: p.ink,
      curve.move((x, y - r)),
      curve.line((x + r, y)),
      curve.line((x, y + r)),
      curve.line((x - r, y)),
      curve.close(),
    ))
  } else if name == "root" {
    place(curve(
      fill: p.ink,
      curve.move((x - r, y - r)),
      curve.line((x + r, y)),
      curve.line((x - r, y + r)),
      curve.close(),
    ))
  } else {
    let hollow = name == "tag"
    place(dx: x - r, dy: y - r, rect(
      width: 2 * r,
      height: 2 * r,
      radius: r,
      fill: if hollow { p.card } else { p.ink },
      stroke: if hollow { sv-border + p.ink } else { none },
    ))
  }
}

// The grey corner mark of a type no root reaches, in the top right corner of
// a box of `w` by `h` whose corner radius is `radius`.
#let sv-corner-mark(w, h, radius, p) = {
  let leg = calc.max(sv-corner, radius)
  place(block(width: w, height: h, radius: radius, clip: true, place(
    top + right,
    curve(
      fill: p.border,
      curve.move((0pt, 0pt)),
      curve.line((leg, 0pt)),
      curve.line((leg, leg)),
      curve.close(),
    ),
  )))
}

// The frame of a box of every kind but a scalar: the accent under the bar,
// the card, the header tint or the pill of an enum, the corner mark, the
// border, and the inner border of a union or the notch of an input.
#let sv-frame(b, k, p, border) = {
  let (w, h) = (b.w, b.h)
  let body = calc.max(0pt, w - sv-bar)
  place(rect(width: w, height: h, radius: sv-radius, fill: k.accent))
  place(dx: sv-bar, rect(width: body, height: h, radius: (right: sv-radius), fill: p.card))
  if b.kind == "enum" {
    let pill = sv-header - 2 * sv-pill-inset
    place(dx: sv-bar + sv-pill-inset, dy: sv-pill-inset, rect(
      width: calc.max(0pt, body - 2 * sv-pill-inset),
      height: pill,
      radius: pill / 2,
      fill: k.tint,
    ))
  } else {
    let bottom = if h <= sv-header { sv-radius } else { 0pt }
    place(dx: sv-bar, rect(
      width: body,
      height: calc.min(sv-header, h),
      radius: (top-right: sv-radius, bottom-right: bottom),
      fill: k.tint,
    ))
  }
  if b.at("unreachable", default: false) { sv-corner-mark(w, h, sv-radius, p) }
  place(rect(width: w, height: h, radius: sv-radius, stroke: (
    paint: k.accent,
    thickness: border,
    dash: if b.kind == "interface" { (4 * sv-px, 3 * sv-px) } else { none },
  )))
  if b.kind == "union" {
    let d = sv-double-inset
    place(dx: d, dy: d, rect(
      width: calc.max(0pt, w - 2 * d),
      height: calc.max(0pt, h - 2 * d),
      radius: sv-radius - d,
      stroke: sv-border + k.accent,
    ))
  } else if b.kind == "input" {
    let mid = sv-header / 2
    place(curve(
      fill: p.canvas,
      stroke: sv-border + k.accent,
      curve.move((0pt, mid - sv-notch-half)),
      curve.line((sv-notch-depth, mid)),
      curve.line((0pt, mid + sv-notch-half)),
    ))
  }
}

// The frame of a scalar, a pill in its tint with no body (DOCUMENTED,
// docs/aoviewer.md section 4.3).
#let sv-pill(b, k, p, border) = {
  let radius = calc.min(b.h, sv-header) / 2
  place(rect(width: b.w, height: b.h, radius: radius, fill: k.tint))
  if b.at("unreachable", default: false) { sv-corner-mark(b.w, b.h, radius, p) }
  place(rect(width: b.w, height: b.h, radius: radius, stroke: border + k.accent))
}

// The hatching of a deprecated row whose band starts at `top`, lines at 45
// degrees between the bar and the right side (DOCUMENTED, docs/aoviewer.md
// section 3.10).
#let sv-hatch(w, top, p) = {
  let span = calc.max(0pt, w - sv-bar)
  place(dx: sv-bar, dy: top, block(width: span, height: sv-row, clip: true, {
    for i in range(calc.ceil((span + sv-row) / sv-hatch-step)) {
      let s = i * sv-hatch-step - sv-row
      place(curve(
        stroke: sv-border + p.border,
        curve.move((s, sv-row)),
        curve.line((s + sv-row, 0pt)),
      ))
    }
  }))
}

// A row of a box at position `i`: its hatching and strike when it is
// deprecated, its name, its type in the muted ink and its marks, each mark in
// the second of its two cells. A row without the height of its port sits in
// the band of its position under the header, where the layout puts every
// port (PROVEN, `port` in crates/aoviewer-core/src/layout/measure.rs).
#let sv-box-row(b, i, row, p) = {
  let (name, type-text) = (row.at(0), row.at(1))
  let y = row.at(2, default: b.y + sv-header + i * sv-row + sv-row / 2)
  let marks = row.at(3, default: ())
  let mid = y - b.y
  let x = sv-padding
  let cell = sv-cell(sv-text-size)
  let deprecated = "deprecated" in marks
  if deprecated { sv-hatch(b.w, mid - sv-row / 2, p) }
  sv-run(x, mid, text(fill: p.ink, name))
  let n = name.codepoints().len()
  if deprecated {
    place(curve(
      stroke: sv-border + p.ink,
      curve.move((x, mid)),
      curve.line((x + n * cell, mid)),
    ))
  }
  if type-text != "" {
    sv-run(x + (n + sv-gap-cells) * cell, mid, text(fill: p.muted, type-text))
    n = n + sv-gap-cells + type-text.codepoints().len()
  }
  if "relay" in marks {
    sv-glyph("relay", x + (n + 1.5) * cell, mid, p)
    n = n + sv-mark-cells
  }
  if deprecated {
    let mark = if "tag" in marks { "tag" } else { "deprecated" }
    sv-glyph(mark, x + (n + 1.5) * cell, mid, p)
  }
}

// Returns the text of the row of the `count` inherited fields a box folds
// (PROVEN, `inherited_label` in crates/aoviewer-core/src/scene/mod.rs).
#let sv-inherited-text(count) = str(count) + " inherited"

// Returns the text of the row that counts the `count` rows the box `b`
// leaves out: `+N types` in a cluster box of the map, `+N values` in an enum
// and `+N fields` otherwise (PROVEN, `more_label` in
// crates/aoviewer-core/src/scene/mod.rs).
#let sv-more-text(b, count) = {
  let what = if b.at("cluster", default: false) {
    "types"
  } else if b.kind == "enum" {
    "values"
  } else {
    "fields"
  }
  "+" + str(count) + " " + what
}

// A box of a diagram: its frame, its header with the letter badge, the name
// linked to the page of its type and the root mark, its rows, and the rows
// of its folded inherited fields and of what it leaves out, which the data
// gives as the counts `inherited` and `more` and the theme words, so both
// texts change here. The layout measured the width of a box with these
// words (PROVEN, `size` in crates/aoviewer-core/src/layout/measure.rs).
#let sv-box(b, p) = {
  let k = p.at(b.kind)
  let border = if b.at("focus", default: false) { sv-focus-border } else { sv-border }
  let cell = sv-cell(sv-text-size)
  let x = sv-padding
  let mid = sv-header / 2
  place(dx: b.x, dy: b.y, box(width: b.w, height: b.h, {
    if b.kind == "scalar" { sv-pill(b, k, p, border) } else { sv-frame(b, k, p, border) }
    let badge = if b.at("cluster", default: false) { 0 } else { sv-badge-cells }
    if badge > 0 {
      sv-run(x, mid, text(weight: "bold", fill: k.accent, sv-letters.at(b.kind)))
    }
    sv-run(x + badge * cell, mid, sv-box-link(
      b.at("link", default: none),
      text(weight: "bold", fill: p.ink, b.name),
    ))
    if b.at("root", default: false) {
      let n = badge + b.name.codepoints().len() + 1
      sv-glyph("root", x + (n + 0.5) * cell, mid, p)
    }
    let rows = b.at("rows", default: ())
    for (i, row) in rows.enumerate() { sv-box-row(b, i, row, p) }
    let inherited = b.at("inherited", default: 0)
    let more = b.at("more", default: 0)
    let notes = ()
    if inherited > 0 { notes.push(sv-inherited-text(inherited)) }
    if more > 0 { notes.push(sv-more-text(b, more)) }
    for (i, note) in notes.enumerate() {
      let band = rows.len() + i
      sv-run(x, sv-header + band * sv-row + sv-row / 2, text(fill: p.muted, note))
    }
  }))
}

// A chip: a count chip, or a stub chip whose page number follows it in a
// printed document (DOCUMENTED, docs/documentation-generator.md section
// 6.2).
#let sv-chip(c, p) = {
  let target = c.at("target", default: none)
  place(dx: c.x, dy: c.y, rect(
    width: c.w,
    height: c.h,
    radius: sv-chip-radius,
    fill: p.card,
    stroke: sv-border + p.border,
  ))
  let label = sv-box-link(target, text(size: sv-chip-size, fill: p.ink, c.text))
  place(dx: c.x + sv-chip-padding, dy: c.y, box(height: c.h, align(left + horizon, label)))
  if c.at("stub", default: false) and target != none {
    place(dx: c.x + c.w + 2 * sv-px, dy: c.y, box(height: c.h, align(
      left + horizon,
      text(size: sv-chip-size, fill: p.muted, sv-stub(target)),
    )))
  }
}

// The share of the height of a page a diagram takes at most, which leaves
// room for the title and the breadcrumb above it on the same page (ASSUMED).
// On the page of `sv-setup` it holds a diagram to 409.6 pt of height, so the
// 13 px text of a diagram taller than 614.4 pt, and the 11 px text of one
// taller than 519.9 pt, prints below 6.5 pt (ASSUMED, 0.85 of 481.9 pt, and
// 9.75 pt or 8.25 pt of text times 409.6 over the height).
#let sv-diagram-share = 0.85

// A diagram, the call a generated file writes for a scene (DOCUMENTED,
// docs/documentation-generator.md section 8.1): the edges, the trunks, the
// boxes and the chips, drawn from the back to the front on the canvas, and
// scaled down to the width of the page and to `sv-diagram-share` of its
// height when it is larger. It is never scaled up. The width fit keeps the
// 13 px text of a diagram inside the page budget at 6.5 pt or larger but not
// the 11 px text of its chips and stub page numbers, and the height fit keeps
// neither, as `sv-setup` and `sv-diagram-share` say.
#let sv-diagram(
  width: 0pt,
  height: 0pt,
  boxes: (),
  edges: (),
  trunks: (),
  chips: (),
) = {
  let drawing = context {
    let p = sv-palette()
    set text(font: sv-mono, size: sv-text-size, hyphenate: false)
    box(width: width, height: height, fill: p.canvas, {
      for e in edges { sv-edge(e, p) }
      for t in trunks { sv-trunk(t, p) }
      for b in boxes { sv-box(b, p) }
      for c in chips { sv-chip(c, p) }
    })
  }
  layout(size => {
    let fit = (1.0,)
    if width > 0pt { fit.push(size.width / width) }
    if height > 0pt { fit.push(sv-diagram-share * size.height / height) }
    block(breakable: false, scale(calc.min(..fit) * 100%, reflow: true, drawing))
  })
}

// An image of a diagram in place of its primitives, the light SVG file of
// the tree under the flag `--typst-diagrams svg` (DOCUMENTED,
// docs/documentation-generator.md section 8.1). `path` is a path from the
// root of the tree, such as `/svg/type/Product.light.svg`.
#let sv-image(path, width: auto) = block(breakable: false, image(path, width: width))
