// SPDX-License-Identifier: MIT
/*
 * aoviewer-core render_svg write: the element writer of the SVG form.
 *
 * Every coordinate prints as centipixels divided by 100, exact, with trailing
 * zeros dropped.
 */

//! The SVG element writer.
//!
//! The writer appends one element per line to a `String`, with presentation
//! attributes only, no `style` element, no `class` and no baseline attribute,
//! because a Markdown host's sanitizer strips attributes and a removal breaks
//! a published diagram. A number is a length in centipixels and prints as
//! pixels without rounding, so `1250` prints `12.5` and `-5` prints `-0.05`.
//!
//! A text run carries its coordinates, its text anchor and a `textLength`
//! with its measured width, because an SVG shown through an image element
//! cannot load the embedded face and falls back to a face of the host. The
//! baseline of a run is placed by its `y` alone, so a run sits on the middle
//! of its band through the cap height of the face.
//!
//! A string of the schema is escaped for XML 1.0 here, where `&`, `<` and `>`
//! become references, a double quote inside an attribute does too, and a
//! character the `Char` production of XML 1.0 excludes becomes U+FFFD, since
//! no reference can carry it. A tab, a line feed or a carriage return inside
//! an attribute is written as a reference, which the value normalization of a
//! parser keeps.
//!
//! Every loop runs over the characters of one string, whose length the limits
//! of the reader cap, over the at most three points of one path command, or
//! over the digits of one number, of which an `i32` has at most [`DIGITS`].

use alloc::string::String;

use crate::{
    scene::{
        Point,
        Rect,
        TEXT_CP, //
    },
    text::{
        EM,
        cells,
        run_width, //
    },
};

/// The most decimal digits a length in whole pixels takes, the ten of
/// `u32::MAX`.
const DIGITS: usize = 10;

/// The cap height of Sabi Mono and of `DejaVu Sans Mono`, in units of the em.
const CAP_HEIGHT: i32 = 1493;

/// The x height of Sabi Mono and of `DejaVu Sans Mono`, in units of the em.
const X_HEIGHT: i32 = 1120;

/// The control length of a cubic segment that draws a quarter circle, in ten
/// thousandths of the radius, `4 (sqrt(2) - 1) / 3`, at which the segment
/// meets the circle at its middle.
const KAPPA: i32 = 5523;

/// The weight of the text of a header.
pub(super) const BOLD: &str = "700";

/// Appends `cp` centipixels as pixels, exact, with trailing zeros dropped.
pub(super) fn num(out: &mut String, cp: i32) {
    if cp < 0 {
        out.push('-');
    }
    let abs = cp.unsigned_abs();
    let (whole, frac) = (abs / 100, abs % 100);
    let mut digits = ['0'; DIGITS];
    let mut len = 0;
    let mut rest = whole;
    while len < DIGITS {
        digits[len] = digit(rest % 10);
        len += 1;
        rest /= 10;
        if rest == 0 {
            break;
        }
    }
    for &d in digits[..len].iter().rev() {
        out.push(d);
    }
    if frac != 0 {
        out.push('.');
        out.push(digit(frac / 10));
        if frac % 10 != 0 {
            out.push(digit(frac % 10));
        }
    }
}

/// Returns the decimal digit of `d`, which is below 10.
fn digit(d: u32) -> char {
    char::from_digit(d, 10).unwrap_or('0')
}

/// Appends `value` escaped as XML character data.
pub(super) fn text(out: &mut String, value: &str) {
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\r' => out.push_str("&#13;"),
            '\t' | '\n' => out.push(c),
            c if excluded(c) => out.push('\u{FFFD}'),
            c => out.push(c),
        }
    }
}

/// Appends `value` escaped as the value of an XML attribute in double
/// quotes.
pub(super) fn attr(out: &mut String, value: &str) {
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\t' => out.push_str("&#9;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            c if excluded(c) => out.push('\u{FFFD}'),
            c => out.push(c),
        }
    }
}

/// Returns whether the `Char` production of XML 1.0 excludes `c`, a control
/// character other than the tab, the line feed and the carriage return, or
/// one of the two noncharacters U+FFFE and U+FFFF.
fn excluded(c: char) -> bool {
    matches!(c, '\u{0}'..='\u{8}' | '\u{B}' | '\u{C}' | '\u{E}'..='\u{1F}')
        || matches!(c, '\u{FFFE}' | '\u{FFFF}')
}

/// Appends ` name="value"` with a length `cp` in centipixels.
pub(super) fn len_attr(out: &mut String, name: &str, cp: i32) {
    out.push(' ');
    out.push_str(name);
    out.push_str("=\"");
    num(out, cp);
    out.push('"');
}

/// Appends ` name="value"` with a literal `value` of the writer, a colour,
/// a weight or a dash pattern, which needs no escape.
pub(super) fn lit_attr(out: &mut String, name: &str, value: &str) {
    out.push(' ');
    out.push_str(name);
    out.push_str("=\"");
    out.push_str(value);
    out.push('"');
}

/// Appends a `<g>` that opens a group, with the id `id` when one is given.
pub(super) fn open_group(out: &mut String, id: Option<&str>) {
    out.push_str("<g");
    if let Some(id) = id {
        out.push_str(" id=\"");
        attr(out, id);
        out.push('"');
    }
    out.push_str(">\n");
}

/// Appends the `</g>` that closes a group.
pub(super) fn close_group(out: &mut String) {
    out.push_str("</g>\n");
}

/// How a shape is filled and stroked.
#[derive(Clone, Copy, Debug)]
pub(super) struct Paint<'a> {
    /// The fill colour, or `none`.
    pub(super) fill: &'a str,
    /// The stroke colour, when the shape has a stroke.
    pub(super) stroke: Option<&'a str>,
    /// The width of the stroke in centipixels.
    pub(super) width: i32,
    /// The dash pattern of the stroke in pixels, when it is not solid.
    pub(super) dash: Option<&'static str>,
}

impl Paint<'_> {
    /// Returns a paint that fills with `fill` and draws no stroke.
    pub(super) fn fill(fill: &str) -> Paint<'_> {
        Paint {
            fill,
            stroke: None,
            width: 0,
            dash: None,
        }
    }

    /// Returns a paint that fills nothing and strokes `stroke` at `width`.
    pub(super) fn stroke(stroke: &str, width: i32) -> Paint<'_> {
        Paint {
            fill: "none",
            stroke: Some(stroke),
            width,
            dash: None,
        }
    }
}

/// Appends the paint attributes of `paint`.
fn paint_attrs(out: &mut String, paint: Paint<'_>) {
    lit_attr(out, "fill", paint.fill);
    if let Some(stroke) = paint.stroke {
        lit_attr(out, "stroke", stroke);
        len_attr(out, "stroke-width", paint.width);
    }
    if let Some(dash) = paint.dash {
        lit_attr(out, "stroke-dasharray", dash);
    }
}

/// Appends a `<rect>` over `r` with corners of the radius `radius`.
pub(super) fn rect(out: &mut String, r: Rect, radius: i32, paint: Paint<'_>) {
    out.push_str("<rect");
    len_attr(out, "x", r.x);
    len_attr(out, "y", r.y);
    len_attr(out, "width", r.w);
    len_attr(out, "height", r.h);
    if radius > 0 {
        len_attr(out, "rx", radius);
    }
    paint_attrs(out, paint);
    out.push_str("/>\n");
}

/// Appends a `<path>` whose data is `d`, with the tooltip `tip` when one is
/// given.
pub(super) fn path(out: &mut String, d: &str, paint: Paint<'_>, tip: Option<&str>) {
    out.push_str("<path d=\"");
    out.push_str(d);
    out.push('"');
    paint_attrs(out, paint);
    close_shape(out, "path", tip);
}

/// Appends a `<circle>` of the radius `r` around `c`, with the tooltip `tip`
/// when one is given.
pub(super) fn circle(out: &mut String, c: Point, r: i32, paint: Paint<'_>, tip: Option<&str>) {
    out.push_str("<circle");
    len_attr(out, "cx", c.x);
    len_attr(out, "cy", c.y);
    len_attr(out, "r", r);
    paint_attrs(out, paint);
    close_shape(out, "circle", tip);
}

/// Closes a shape whose tag is `tag`, with a `<title>` child that carries
/// `tip` when one is given, which is the tooltip of the shape.
fn close_shape(out: &mut String, tag: &str, tip: Option<&str>) {
    let Some(tip) = tip else {
        out.push_str("/>\n");
        return;
    };
    out.push_str("><title>");
    text(out, tip);
    out.push_str("</title></");
    out.push_str(tag);
    out.push_str(">\n");
}

/// A text run, its place, its size, its weight and its colour.
#[derive(Clone, Copy, Debug)]
pub(super) struct Run<'t> {
    /// The left end of the run.
    pub(super) x: i32,
    /// The middle of the band the run sits in.
    pub(super) mid: i32,
    /// The size of the text in centipixels.
    pub(super) size: i32,
    /// Whether the run is set in the bold weight.
    pub(super) bold: bool,
    /// The fill colour.
    pub(super) fill: &'t str,
}

/// Appends a `<text>` that sets `content` as the run `run`.
///
/// The run starts at its `x` with the anchor `start`, its baseline lies half
/// the cap height under the middle of its band, and its `textLength` is its
/// width in cells at its size. A run at the size of the text of a box
/// inherits its size from the root element.
pub(super) fn run(out: &mut String, run: Run<'_>, content: &str) {
    out.push_str("<text");
    len_attr(out, "x", run.x);
    len_attr(out, "y", baseline(run.mid, run.size));
    if run.size != TEXT_CP {
        len_attr(out, "font-size", run.size);
    }
    if run.bold {
        lit_attr(out, "font-weight", BOLD);
    }
    lit_attr(out, "fill", run.fill);
    lit_attr(out, "text-anchor", "start");
    len_attr(out, "textLength", run_width(cells(content), run.size));
    out.push('>');
    text(out, content);
    out.push_str("</text>\n");
}

/// Returns the baseline of a run of the size `size` whose capitals are
/// centred on `mid`.
pub(super) fn baseline(mid: i32, size: i32) -> i32 {
    mid.saturating_add(scale(size, CAP_HEIGHT, 2 * EM))
}

/// Returns the height of a line that strikes through the lower case letters
/// of a run of the size `size` on the baseline `base`, half the x height
/// above it.
pub(super) fn strike(base: i32, size: i32) -> i32 {
    base.saturating_sub(scale(size, X_HEIGHT, 2 * EM))
}

/// Returns `value` times `num` over `den` as an integer division whose
/// remainder is dropped, saturated to an `i32`.
fn scale(value: i32, num: i32, den: i32) -> i32 {
    let product = i64::from(value) * i64::from(num) / i64::from(den);
    i32::try_from(product).unwrap_or(if product < 0 { i32::MIN } else { i32::MAX })
}

/// The path data of a shape, built of absolute moves, lines and cubic
/// segments alone.
#[derive(Debug, Default)]
pub(super) struct PathData {
    /// The data so far.
    pub(super) d: String,
}

impl PathData {
    /// Appends a command letter and the points `points`.
    fn command(&mut self, letter: char, points: &[Point]) {
        if !self.d.is_empty() {
            self.d.push(' ');
        }
        self.d.push(letter);
        for (i, p) in points.iter().enumerate() {
            if i > 0 {
                self.d.push(' ');
            }
            num(&mut self.d, p.x);
            self.d.push(' ');
            num(&mut self.d, p.y);
        }
    }

    /// Moves to `p`.
    pub(super) fn move_to(&mut self, p: Point) {
        self.command('M', &[p]);
    }

    /// Draws a line to `p`.
    pub(super) fn line_to(&mut self, p: Point) {
        self.command('L', &[p]);
    }

    /// Draws a cubic segment over `c1` and `c2` to `p`.
    pub(super) fn cubic_to(&mut self, c1: Point, c2: Point, p: Point) {
        self.command('C', &[c1, c2, p]);
    }

    /// Closes the shape.
    pub(super) fn close(&mut self) {
        self.d.push_str(" Z");
    }

    /// Draws a quarter circle from `from` round the corner `corner` to `to`.
    fn corner(&mut self, from: Point, corner: Point, to: Point) {
        let pull = |a: i32, b: i32| a.saturating_add(scale(b.saturating_sub(a), KAPPA, 10_000));
        let c1 = pt(pull(from.x, corner.x), pull(from.y, corner.y));
        let c2 = pt(pull(to.x, corner.x), pull(to.y, corner.y));
        self.cubic_to(c1, c2, to);
    }
}

/// The radii of the four corners of a rounded rectangle, from the top left
/// clockwise.
#[derive(Clone, Copy, Debug)]
pub(super) struct Corners {
    /// The top left corner.
    pub(super) tl: i32,
    /// The top right corner.
    pub(super) tr: i32,
    /// The bottom right corner.
    pub(super) br: i32,
    /// The bottom left corner.
    pub(super) bl: i32,
}

/// Returns the path data of the rectangle `r` with the rounded corners
/// `corners`, each drawn as a quarter circle, and a radius of 0 as a sharp
/// corner.
pub(super) fn rounded(r: Rect, corners: Corners) -> String {
    let (left, top) = (r.x, r.y);
    let (right, bottom) = (r.x.saturating_add(r.w), r.y.saturating_add(r.h));
    let mut p = PathData::default();
    p.move_to(pt(left.saturating_add(corners.tl), top));
    p.line_to(pt(right.saturating_sub(corners.tr), top));
    if corners.tr > 0 {
        let to = pt(right, top.saturating_add(corners.tr));
        p.corner(
            pt(right.saturating_sub(corners.tr), top),
            pt(right, top),
            to,
        );
    }
    p.line_to(pt(right, bottom.saturating_sub(corners.br)));
    if corners.br > 0 {
        let to = pt(right.saturating_sub(corners.br), bottom);
        p.corner(
            pt(right, bottom.saturating_sub(corners.br)),
            pt(right, bottom),
            to,
        );
    }
    p.line_to(pt(left.saturating_add(corners.bl), bottom));
    if corners.bl > 0 {
        let to = pt(left, bottom.saturating_sub(corners.bl));
        p.corner(
            pt(left.saturating_add(corners.bl), bottom),
            pt(left, bottom),
            to,
        );
    }
    p.line_to(pt(left, top.saturating_add(corners.tl)));
    if corners.tl > 0 {
        let to = pt(left.saturating_add(corners.tl), top);
        p.corner(pt(left, top.saturating_add(corners.tl)), pt(left, top), to);
    }
    p.close();
    p.d
}

/// Returns the path data of the wedge in the top right corner of `r` whose
/// legs are `size` long, or `radius` long when that is longer, with the
/// corner rounded by `radius` as the corner of the shape is.
///
/// Legs as long as the radius leave the curve of the corner and its chord
/// alone, which lie inside every shape whose corner is drawn with that curve,
/// a box. Against the true quarter circle of the end of a pill, the curve
/// strays at most 0.03 percent of the radius beyond it between those three
/// points, about 0.004 px at 15 px, under the border of the pill.
pub(super) fn corner_wedge(r: Rect, size: i32, radius: i32) -> String {
    let legs = size.max(radius);
    let (right, top) = (r.x.saturating_add(r.w), r.y);
    let mut p = PathData::default();
    p.move_to(pt(right.saturating_sub(legs), top));
    if legs > radius {
        p.line_to(pt(right.saturating_sub(radius), top));
    }
    let to = pt(right, top.saturating_add(radius));
    p.corner(pt(right.saturating_sub(radius), top), pt(right, top), to);
    if legs > radius {
        p.line_to(pt(right, top.saturating_add(legs)));
    }
    p.close();
    p.d
}

/// Returns the point at `x` and `y`.
pub(super) fn pt(x: i32, y: i32) -> Point {
    Point { x, y }
}
