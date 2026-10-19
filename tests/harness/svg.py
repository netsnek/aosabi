# SPDX-License-Identifier: MIT
"""Aosabi harness: the invariants of a drawing, read over
xml.etree.ElementTree.

A drawing has no box overlap, no run whose text length exceeds its box, no edge
through a foreign box and unique ids in the grammar of the parity table.

The checks read geometry and never pixels.

- The root is `<svg>` with `width`, `height`, `role="img"` and a `<title>`.
- A box is an element whose `id` is a type or a cluster id in the grammar
  below, outside every other box, and its first `<rect>` is its outline. A box
  lower than one header is a chip, such as a stub or a count chip.
- A row is an element inside a box whose `id` names a member, such as a field,
  an argument, an enum value, a possible type or an interface of the type. Its
  band is its own first `<rect>` when it has one, and otherwise the slot of
  its place among the rows of the box under the header.
- An edge is an element whose `id` is `<source id> => <target type id>`, and
  its first `<path>` is the curve.

The grammar is Voyager's, `TYPE::Name`, `FIELD::Type::field`,
`POSSIBLE_TYPE::Union::Member`, `DERIVED_TYPE::Interface::Type`,
`INTERFACE::Type::Interface`, and an edge as its source id, ` => ` and the id
of its target type, extended by `ARG::Type::field::arg`,
`ENUMVALUE::Enum::VALUE`, `INPUTFIELD::Input::field` and `CLUSTER::slug`. A
cluster slug is the cluster name lower cased with every run outside `[a-z0-9]`
one hyphen and a duplicate numbered, so it is read as a nonempty run of lower
case letters, digits and hyphens.
"""

from __future__ import annotations

import math
import re
import xml.etree.ElementTree as ET
from dataclasses import dataclass, field

# The measures of a box: a 30 px header, 22 px rows and 12 px of horizontal
# padding.
HEADER_PX = 30
ROW_PX = 22
PADDING_PX = 12

# The horizontal padding a text run keeps inside a chip, the 4 px base step of
# the same section, because no chip measure is written down.
CHIP_PADDING_PX = 4

# The budget of a diagram placed inside a page.
MAX_BOXES = 40
MAX_EDGES = 80
MAX_WIDTH_PX = 1400

# An edge ends in a 6 by 5 px head, so the curve may stop one head short of the
# side it enters, and one pixel more covers the stroke.
SIDE_TOLERANCE_PX = 7

# Edges that enter one target merge over their last 24 px into one trunk, so
# two first ring edges that meet there do not cross.
TRUNK_PX = 24

# The points a cubic segment is sampled at for the crossing of two first ring
# edges, which compares two polylines and not two curves.
SAMPLES_PER_SEGMENT = 32

# The check of an edge through a foreign box samples every segment finer, at
# most `sample_step` apart along the curve, which follows the smallest box or
# chip of the drawing. The fewest pixels that step may take, and the most
# samples one segment may need before the check refuses the path.
MIN_STEP_PX = 1.0
MAX_SAMPLES_PER_SEGMENT = 8192

# The elements, the nesting depth and the path tokens one drawing may hold,
# the caps of the walk.
MAX_ELEMENTS = 200_000
MAX_DEPTH = 64
MAX_PATH_TOKENS = 100_000

NAME = r"[_A-Za-z][_0-9A-Za-z]*"
SLUG = r"[a-z0-9-]+"
ELEMENT_ID = re.compile(
    r"^(?:TYPE::{n}|(?:FIELD|INPUTFIELD|POSSIBLE_TYPE|DERIVED_TYPE|INTERFACE|ENUMVALUE)::{n}::{n}"
    r"|ARG::{n}::{n}::{n}|CLUSTER::{s})$".format(n=NAME, s=SLUG)
)
EDGE_ID = re.compile(r"^(.+) => (TYPE::%s)$" % NAME)
BOX_ID = re.compile(r"^(?:TYPE::%s|CLUSTER::%s)$" % (NAME, SLUG))

# The attributes a Markdown host's sanitizer strips or that break a diagram
# there, the style and the baseline attributes, and the class, which only the
# classed form of the viewer writes while the document form writes literal
# colours.
FORBIDDEN_ATTRIBUTES = ("style", "class", "dominant-baseline", "alignment-baseline", "baseline-shift")

PATH_TOKEN = re.compile(r"[MmLlCcHhVvZz]|[-+]?(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?")


@dataclass(frozen=True)
class Rect:
    """An axis aligned rectangle in pixels."""

    x: float
    y: float
    w: float
    h: float

    @property
    def right(self) -> float:
        return self.x + self.w

    @property
    def bottom(self) -> float:
        return self.y + self.h

    def overlaps(self, other: "Rect") -> bool:
        """Reports whether the two rectangles share an area, a shared side is none."""
        return (
            self.x < other.right
            and other.x < self.right
            and self.y < other.bottom
            and other.y < self.bottom
        )

    def contains(self, x: float, y: float, margin: float = 0.5) -> bool:
        """Reports whether the point lies inside, more than `margin` from every side."""
        return (
            self.x + margin < x < self.right - margin
            and self.y + margin < y < self.bottom - margin
        )


@dataclass
class Box:
    """A box or a chip of a drawing, with its rows in document order."""

    id: str
    rect: Rect
    chip: bool
    rows: list = field(default_factory=list)
    texts: list = field(default_factory=list)


@dataclass
class Edge:
    """An edge of a drawing: its source and target ids, its curve sampled at
    `SAMPLES_PER_SEGMENT` points per cubic segment, and the path data `d` a
    finer sampling reads."""

    id: str
    source: str
    target: str
    points: list
    d: str = ""


@dataclass
class Drawing:
    """What `read` finds in one SVG."""

    width: float
    height: float
    title: str
    role: str
    boxes: dict
    rows: dict
    edges: list
    ids: list
    texts_outside: list
    forbidden: list


@dataclass
class Report:
    """The counts of every invariant over one drawing, each a list of findings."""

    overlaps: list = field(default_factory=list)
    overflows: list = field(default_factory=list)
    texts_outside: list = field(default_factory=list)
    bad_ends: list = field(default_factory=list)
    through_foreign: list = field(default_factory=list)
    ring_crossings: list = field(default_factory=list)
    ring_rule_applies: bool = False
    over_budget: list = field(default_factory=list)
    duplicate_ids: list = field(default_factory=list)
    bad_ids: list = field(default_factory=list)
    document: list = field(default_factory=list)
    boxes: int = 0
    edges: int = 0

    @property
    def clean(self) -> bool:
        """Reports whether no invariant has a finding."""
        return not (
            self.overlaps
            or self.overflows
            or self.texts_outside
            or self.bad_ends
            or self.through_foreign
            or self.ring_crossings
            or self.over_budget
            or self.duplicate_ids
            or self.bad_ids
            or self.document
        )

    def counts(self) -> dict:
        """The number of findings per invariant, which is what a notebook sums."""
        return {
            "overlaps": len(self.overlaps),
            "overflows": len(self.overflows),
            "texts_outside": len(self.texts_outside),
            "bad_ends": len(self.bad_ends),
            "through_foreign": len(self.through_foreign),
            "ring_crossings": len(self.ring_crossings),
            "over_budget": len(self.over_budget),
            "duplicate_ids": len(self.duplicate_ids),
            "bad_ids": len(self.bad_ids),
            "document": len(self.document),
        }


def _local(tag: str) -> str:
    return tag.rsplit("}", 1)[-1]


def _number(value, default: float = 0.0) -> float:
    if value is None:
        return default
    return float(value.strip().removesuffix("px"))


def _first(element, name: str):
    for child in element.iter():
        if child is not element and _local(child.tag) == name:
            return child
    return None


def _rect_of(element) -> Rect:
    rect = element if _local(element.tag) == "rect" else _first(element, "rect")
    if rect is None:
        return None
    return Rect(
        _number(rect.get("x")),
        _number(rect.get("y")),
        _number(rect.get("width")),
        _number(rect.get("height")),
    )


def _segment_samples(length: float, step_px: float) -> int:
    """Returns the samples a segment needs so two neighbours lie at most
    `step_px` apart along it, when its arc is at most `length` long."""
    steps = max(1, math.ceil(length / step_px))
    if steps > MAX_SAMPLES_PER_SEGMENT:
        raise ValueError("a segment would need more than %d samples" % MAX_SAMPLES_PER_SEGMENT)
    return steps


def _line(p0, p1, steps: int) -> list:
    return [
        (p0[0] + (p1[0] - p0[0]) * i / steps, p0[1] + (p1[1] - p0[1]) * i / steps)
        for i in range(1, steps + 1)
    ]


def _cubic(p0, p1, p2, p3, steps: int) -> list:
    out = []
    for i in range(1, steps + 1):
        t = i / steps
        u = 1 - t
        out.append(
            (
                u * u * u * p0[0] + 3 * u * u * t * p1[0] + 3 * u * t * t * p2[0] + t * t * t * p3[0],
                u * u * u * p0[1] + 3 * u * u * t * p1[1] + 3 * u * t * t * p2[1] + t * t * t * p3[1],
            )
        )
    return out


def path_points(d: str, step_px: float = None) -> list:
    """Returns the sampled points of a path in absolute or relative commands.

    Without `step_px`, straight segments contribute their end points and cubic
    segments `SAMPLES_PER_SEGMENT` points each. The first and the last point
    are the two ends of the path.
    """
    tokens = PATH_TOKEN.findall(d)
    if len(tokens) > MAX_PATH_TOKENS:
        raise ValueError("a path of more than %d tokens" % MAX_PATH_TOKENS)
    points = []
    x = y = 0.0
    start = (0.0, 0.0)
    command = None
    i = 0
    while i < len(tokens):
        token = tokens[i]
        if token.isalpha():
            command = token
            i += 1
            if command in "Zz":
                if step_px is None:
                    points.append(start)
                else:
                    length = math.hypot(start[0] - x, start[1] - y)
                    points.extend(_line((x, y), start, _segment_samples(length, step_px)))
                x, y = start
            continue
        relative = command.islower()
        upper = command.upper()
        if upper in ("M", "L", "H", "V"):
            if upper in ("M", "L"):
                nx, ny = float(tokens[i]), float(tokens[i + 1])
                i += 2
                if relative:
                    nx, ny = x + nx, y + ny
            else:
                value = float(tokens[i])
                i += 1
                nx, ny = x, y
                if upper == "H":
                    nx = x + value if relative else value
                else:
                    ny = y + value if relative else value
            if upper == "M" or step_px is None:
                points.append((nx, ny))
            else:
                length = math.hypot(nx - x, ny - y)
                points.extend(_line((x, y), (nx, ny), _segment_samples(length, step_px)))
            x, y = nx, ny
            if upper == "M":
                start = (x, y)
                command = "l" if relative else "L"
        elif upper == "C":
            values = [float(tokens[i + k]) for k in range(6)]
            i += 6
            if relative:
                values = [v + (x if k % 2 == 0 else y) for k, v in enumerate(values)]
            p0 = (x, y)
            p1, p2, p3 = (values[0], values[1]), (values[2], values[3]), (values[4], values[5])
            steps = SAMPLES_PER_SEGMENT
            if step_px is not None:
                legs = ((p0, p1), (p1, p2), (p2, p3))
                leg = max(math.hypot(b[0] - a[0], b[1] - a[1]) for a, b in legs)
                steps = max(SAMPLES_PER_SEGMENT, _segment_samples(3 * leg, step_px))
            points.extend(_cubic(p0, p1, p2, p3, steps))
            x, y = p3
        else:
            raise ValueError("the path command %s is not read here" % command)
    return points


def read(data) -> Drawing:
    """Parses one SVG, bytes or text, into its boxes, rows and edges."""
    root = ET.fromstring(data)
    if _local(root.tag) != "svg":
        raise ValueError("the root element is %s and not svg" % _local(root.tag))
    boxes = {}
    rows = {}
    edges = []
    ids = []
    forbidden = []
    texts_outside = []
    title_element = None
    for child in root:
        if _local(child.tag) == "title":
            title_element = child
            break

    visited = 0

    def visit(element, box, depth_budget):
        nonlocal texts_outside, visited
        visited += 1
        if visited > MAX_ELEMENTS or depth_budget == 0:
            raise ValueError("the drawing is deeper or larger than the caps")
        tag = _local(element.tag)
        if tag == "style":
            forbidden.append("a style element")
        for attribute in element.attrib:
            if _local(attribute) in FORBIDDEN_ATTRIBUTES:
                forbidden.append("%s on %s" % (_local(attribute), tag))
        element_id = element.get("id")
        if element_id is not None:
            ids.append(element_id)
        current = box
        if element_id is not None:
            edge = EDGE_ID.match(element_id)
            if edge:
                path = element if tag == "path" else _first(element, "path")
                d = path.get("d", "") if path is not None else ""
                points = path_points(d) if d else []
                edges.append(Edge(element_id, edge.group(1), edge.group(2), points, d))
            elif box is None and BOX_ID.match(element_id):
                rect = _rect_of(element)
                if rect is not None:
                    current = Box(element_id, rect, rect.h < HEADER_PX)
                    boxes[element_id] = current
            elif box is not None and ELEMENT_ID.match(element_id):
                box.rows.append((element_id, _rect_of(element)))
                rows[element_id] = box
        if tag == "text":
            if element.get("textLength") is None:
                forbidden.append("a text run without textLength")
            if current is not None:
                current.texts.append(_number(element.get("textLength"), 0.0))
            else:
                texts_outside.append("".join(element.itertext())[:40])
        for child in element:
            visit(child, current, depth_budget - 1)

    visit(root, None, MAX_DEPTH)
    if title_element is None:
        forbidden.append("no title element")
    return Drawing(
        width=_number(root.get("width")),
        height=_number(root.get("height")),
        title=(title_element.text or "") if title_element is not None else "",
        role=root.get("role", ""),
        boxes=boxes,
        rows=rows,
        edges=edges,
        ids=ids,
        texts_outside=texts_outside,
        forbidden=forbidden,
    )


def sample_step(boxes) -> float:
    """Returns the step at which an edge is sampled for the check of a
    foreign box, half the shorter side of the smallest box or chip less one
    pixel.

    A curve that passes through the body of a box, from one side to the
    opposite one, runs longer than that shorter side less the 0.5 px margin on
    each side inside it, so one sample of that step falls inside. A pass over a
    corner may be shorter and is not held to that.
    """
    sides = [min(b.rect.w, b.rect.h) for b in boxes if b.rect.w > 0 and b.rect.h > 0]
    if not sides:
        return float(MAX_WIDTH_PX)
    return max(MIN_STEP_PX, (min(sides) - 1) / 2)


def _row_band(box: Box, row_id: str):
    for index, (rid, rect) in enumerate(box.rows):
        if rid == row_id:
            if rect is not None:
                return rect.y, rect.bottom
            top = box.rect.y + HEADER_PX + ROW_PX * index
            return top, top + ROW_PX
    return None


def _on_side(rect: Rect, x: float, y: float, top: float, bottom: float) -> bool:
    if not (top - 0.5 <= y <= bottom + 0.5):
        return False
    return abs(x - rect.x) <= SIDE_TOLERANCE_PX or abs(x - rect.right) <= SIDE_TOLERANCE_PX


def _on_border(rect: Rect, x: float, y: float) -> bool:
    inside_x = rect.x - SIDE_TOLERANCE_PX <= x <= rect.right + SIDE_TOLERANCE_PX
    inside_y = rect.y - SIDE_TOLERANCE_PX <= y <= rect.bottom + SIDE_TOLERANCE_PX
    return inside_x and inside_y and not rect.contains(x, y, SIDE_TOLERANCE_PX)


def _source_box(drawing: Drawing, source: str):
    if source in drawing.boxes:
        return drawing.boxes[source], None
    if source in drawing.rows:
        return drawing.rows[source], source
    return None, None


def _segments_cross(a1, a2, b1, b2) -> bool:
    def orient(p, q, r):
        value = (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0])
        return (value > 0) - (value < 0)

    o1, o2 = orient(a1, a2, b1), orient(a1, a2, b2)
    o3, o4 = orient(b1, b2, a1), orient(b1, b2, a2)
    return o1 * o2 < 0 and o3 * o4 < 0


def _away_from_ends(points: list, distance: float) -> list:
    if not points:
        return []
    first, last = points[0], points[-1]
    return [
        p
        for p in points
        if abs(p[0] - first[0]) + abs(p[1] - first[1]) > distance
        and abs(p[0] - last[0]) + abs(p[1] - last[1]) > distance
    ]


def _curves_cross(a: list, b: list) -> bool:
    for i in range(len(a) - 1):
        for j in range(len(b) - 1):
            if _segments_cross(a[i], a[i + 1], b[j], b[j + 1]):
                return True
    return False


def check(data, focus: str = None, budget: bool = True) -> Report:
    """Checks one SVG against the invariants of step 11 of `51_viewer_core`.

    `focus` is the id of the focus box of an ego view, such as `TYPE::Order`,
    and is what the first ring rule is judged around. Without it the rule is
    not judged and `ring_rule_applies` stays false. `budget` holds the drawing
    to the page budget of 40 boxes, 80 edges and 1400 px, which a poster and
    the whole graph do not keep.

    A text run is held to the width of its box less `PADDING_PX` on each
    side, and of its chip less `CHIP_PADDING_PX`, and a run outside every box
    is a finding of its own. An edge is sampled at `sample_step` for the
    check of a foreign box.
    """
    drawing = read(data)
    report = Report()
    boxes = [b for b in drawing.boxes.values()]
    report.boxes = sum(1 for b in boxes if not b.chip)
    report.edges = len(drawing.edges)

    for i in range(len(boxes)):
        for j in range(i + 1, len(boxes)):
            if boxes[i].rect.overlaps(boxes[j].rect):
                report.overlaps.append("%s and %s" % (boxes[i].id, boxes[j].id))

    for box in boxes:
        inner = box.rect.w - 2 * (CHIP_PADDING_PX if box.chip else PADDING_PX)
        for length in box.texts:
            if length > inner:
                report.overflows.append("%s: a run of %g px in %g px" % (box.id, length, inner))

    # Every text run fits its box, so a run outside every box is one.
    report.texts_outside = ["a run outside every box: %r" % text for text in drawing.texts_outside]

    step = sample_step(boxes)

    for edge in drawing.edges:
        if len(edge.points) < 2:
            report.bad_ends.append("%s: no curve" % edge.id)
            continue
        (sx, sy), (tx, ty) = edge.points[0], edge.points[-1]
        source_box, row = _source_box(drawing, edge.source)
        target_box = drawing.boxes.get(edge.target)
        if source_box is None:
            report.bad_ends.append("%s: the source is not in the drawing" % edge.id)
        else:
            if row is not None:
                top, bottom = _row_band(source_box, row)
            else:
                top, bottom = source_box.rect.y, source_box.rect.y + HEADER_PX
            if not _on_side(source_box.rect, sx, sy, top, bottom):
                report.bad_ends.append("%s: starts off a row port and a header side" % edge.id)
        if target_box is None:
            report.bad_ends.append("%s: the target is not in the drawing" % edge.id)
        elif target_box.chip:
            if not _on_border(target_box.rect, tx, ty):
                report.bad_ends.append("%s: ends off its chip" % edge.id)
        elif not _on_side(target_box.rect, tx, ty, target_box.rect.y, target_box.rect.y + HEADER_PX):
            report.bad_ends.append("%s: ends off the header side" % edge.id)
        own = {id(source_box), id(target_box)}
        fine = path_points(edge.d, step) if edge.d else edge.points
        left = min(x for x, _ in fine)
        right = max(x for x, _ in fine)
        top_y = min(y for _, y in fine)
        bottom_y = max(y for _, y in fine)
        for box in boxes:
            if id(box) in own:
                continue
            beside = box.rect.right < left or box.rect.x > right
            if beside or box.rect.bottom < top_y or box.rect.y > bottom_y:
                continue
            if any(box.rect.contains(x, y) for x, y in fine):
                report.through_foreign.append("%s through %s" % (edge.id, box.id))

    if focus is not None and focus in drawing.boxes:
        ring = {}
        for edge in drawing.edges:
            source_box, _ = _source_box(drawing, edge.source)
            source_id = source_box.id if source_box is not None else None
            if source_id == focus and edge.target != focus:
                ring.setdefault(edge.target, []).append(edge)
            elif edge.target == focus and source_id not in (None, focus):
                ring.setdefault(source_id, []).append(edge)
        report.ring_rule_applies = bool(ring) and all(len(v) == 1 for v in ring.values())
        if report.ring_rule_applies:
            first_ring = [v[0] for v in ring.values()]
            trimmed = [_away_from_ends(e.points, TRUNK_PX + SIDE_TOLERANCE_PX) for e in first_ring]
            for i in range(len(first_ring)):
                for j in range(i + 1, len(first_ring)):
                    if _curves_cross(trimmed[i], trimmed[j]):
                        report.ring_crossings.append(
                            "%s and %s" % (first_ring[i].id, first_ring[j].id)
                        )

    if budget:
        if report.boxes > MAX_BOXES:
            report.over_budget.append("%d boxes, the budget is %d" % (report.boxes, MAX_BOXES))
        if report.edges > MAX_EDGES:
            report.over_budget.append("%d edges, the budget is %d" % (report.edges, MAX_EDGES))
        if drawing.width > MAX_WIDTH_PX:
            report.over_budget.append("%g px wide, the budget is %d" % (drawing.width, MAX_WIDTH_PX))

    seen = set()
    for element_id in drawing.ids:
        if element_id in seen:
            report.duplicate_ids.append(element_id)
        seen.add(element_id)
        if not (ELEMENT_ID.match(element_id) or EDGE_ID.match(element_id)):
            report.bad_ids.append(element_id)
        edge = EDGE_ID.match(element_id)
        if edge and not ELEMENT_ID.match(edge.group(1)):
            report.bad_ids.append(element_id)

    report.document = list(drawing.forbidden)
    if drawing.role != "img":
        report.document.append("the root carries role %r and not img" % drawing.role)
    return report
