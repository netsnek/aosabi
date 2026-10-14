// SPDX-License-Identifier: MIT
/*
 * aoviewer-docs device: the device chapter as data and its two writers.
 */

//! The device chapter.
//!
//! A [`DeviceChapter`] is everything the chapter of a device prints, as plain
//! data: the identity and the hash of the card, its signature status, the
//! blueprint as tables with the provenance mark of every fact, the payload,
//! the code that ran and the code that linked, and the knowledge annex.
//!
//! [`render_device_markdown`] writes `md/device.md` and
//! [`render_device_typst`] writes `typ/05-device.typ`, the two file names the
//! plan fixes. Both write the same parts in the same order, which one walk of
//! the chapter drives:
//!
//! - The title, then the picture. The Markdown references the original PNG of
//!   the card, and the Typst places the thumbnail, a picture the caller
//!   encoded again, never the card, since placing a PNG into a PDF drops its
//!   text chunks, and attaches the card whole with `pdf.attach`.
//! - The card: the id, the revision, the file, its SHA-256, the signature
//!   status and the epoch of the document date, so a document and a card that
//!   no longer match are visible.
//! - The blueprint, one table per [`BlueprintTable`] with the name, the
//!   value, the mark and the sources of every fact.
//! - The payload, the code that ran and the code that linked, one table each.
//! - The knowledge annex, every page under its name with its origin and pin,
//!   read as the Markdown subset of the core.
//!
//! Every name and every text of the chapter enters the Markdown through
//! [`md_text`] or [`md_cell`] and the Typst as a string literal through
//! [`typst_str`], so no card string reaches a reader as markup or the compiler
//! as code. A knowledge page is written in Markdown through [`to_markdown`]
//! with its headings demoted below the heading of its name and its raw HTML
//! escaped, and in Typst through [`to_typst`].
//!
//! The pages follow each other in one file, so a fence a page leaves open
//! would take every later page into its code. [`open_fence`] reads the page as
//! written, whose escaped HTML opens no HTML block, and the writer closes the
//! fence it answers. The rule answers none in two cases, and where no list
//! item holds the fence, it stays open and the later pages read as its code.
//! The first is a fence indented by two or three columns after any line
//! indented by at most three columns that a list marker leads, a bullet or a
//! number with a `.` or a `)` followed by a space, a tab or the end of the
//! line, which also covers a thematic break such as `* * *` or `- - -` and a
//! numbered line that continues a paragraph, when no fence and no line at the
//! margin after a blank line come in between, the margin being an indentation
//! of one column or none. The second is a fence after a line indented by at
//! most three columns that begins with a `<` the escape keeps, as the `<` of
//! an autolink, of a link destination or of a code span begun on the line
//! before.
//!
//! The attachment names the MIME type `image/png` and a description that
//! carries the hash of the card, and `api.typ` states a document date from
//! [`DocsOptions::document_date`], since PDF/A-3 refuses an attached file
//! without any of the three, while PDF/A-2 refuses every attached file. So PDF
//! 1.7 and PDF/A-3b take the attachment and PDF/A-2b refuses it, and with it
//! every document that holds the chapter. The relationship is `source`, since
//! the chapter is rendered from the card, and the PDF writes it under PDF/A-3
//! alone.
//!
//! Every loop runs over the tables, rows, sources, candidates, pages or
//! bytes of one chapter, which the caller holds in memory, or over the bytes
//! of one string.
//!
//! [`DocsOptions::document_date`]: aoviewer_core::tree::DocsOptions::document_date

use std::fmt::Write;

use aoviewer_core::{
    commonmark::{
        RawHtml,
        parse,
        to_markdown,
        to_typst, //
    },
    escape::{
        md_cell,
        md_text, //
    },
    limits::ReadLimits,
    render_markdown::open_fence,
    render_typst::typst_str,
};

/// Everything the device chapter prints, as plain data, filled from a card.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceChapter {
    /// The path of the original PNG of the card relative to the root of the
    /// tree, such as `device-card.png`, which the Markdown references and the
    /// Typst attaches, so the document and the World of the PDF find the card
    /// there.
    pub card_file: String,
    /// The SHA-256 of the card in lower case hexadecimal, printed in the
    /// document and recorded's lock.
    pub card_sha256: String,
    /// The id of the device.
    pub id: String,
    /// The revision of the card.
    pub revision: u32,
    /// The title of the device, the title of the chapter.
    pub title: String,
    /// The signature status of the card.
    pub signature: SignatureStatus,
    /// The document date of the PDF/A-3b export in Unix seconds.
    pub source_date_epoch: i64,
    /// The tables of the blueprint in their order: the target, the carriers,
    /// the modules, the capabilities provided and withheld, the memory, the
    /// concurrency, the deflate window and the limits, the allocator, the
    /// packaging and the flashing.
    pub blueprint: Vec<BlueprintTable>,
    /// The parts of the payload in their order.
    pub payload: Vec<PayloadRow>,
    /// Every declared build script with its digest and purpose.
    pub ran: Vec<RanRow>,
    /// Every crate outside the base set with its version and digest.
    pub linked: Vec<LinkedRow>,
    /// The knowledge annex, pages of the Markdown subset with their origin and
    /// pin.
    pub knowledge: Vec<KnowledgePage>,
    /// A picture of the device encoded again for the PDF, never the card
    /// itself, whose chunks a placed picture would lose.
    pub thumbnail: Option<Vec<u8>>,
}

/// The signature status of a card.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignatureStatus {
    /// The card carries no signature.
    Unsigned,
    /// The card is signed.
    Signed {
        /// Every signer, in the order of the card.
        signers: Vec<String>,
        /// The trust tier the build saw.
        tier: String,
    },
    /// A revocation list names the card.
    Revoked {
        /// The serial of that list.
        serial: u64,
    },
}

/// One table of the blueprint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlueprintTable {
    /// The title, such as `Memory`.
    pub title: String,
    /// The facts of the table in their order.
    pub rows: Vec<Fact>,
}

/// One fact of the blueprint with its provenance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fact {
    /// The name of the fact.
    pub name: String,
    /// Its value as the card states it.
    pub value: String,
    /// How the card knows it.
    pub mark: Mark,
    /// Where the card has it from, in the order of the card.
    pub sources: Vec<String>,
}

/// The provenance mark of a fact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mark {
    /// Read in the source it cites.
    Proven,
    /// Stated in documentation.
    Documented,
    /// Recorded by a measurement.
    Measured,
    /// Read in a named private repository.
    Inventoried,
    /// Derived by reading rather than by measuring.
    Assumed,
    /// Not known yet.
    Unknown {
        /// The values the fact may take.
        candidates: Vec<String>,
        /// What would settle it.
        closes: String,
    },
}

/// One part of the payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PayloadRow {
    /// The name of the part.
    pub part: String,
    /// Its count of files.
    pub files: u32,
    /// Its bytes before packing.
    pub raw: u64,
    /// Its bytes packed.
    pub packed: u64,
    /// Its digest.
    pub digest: String,
}

/// One declared build script, the code that ran.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RanRow {
    /// The path of the script.
    pub path: String,
    /// Its digest.
    pub digest: String,
    /// What it does, one sentence.
    pub purpose: String,
}

/// One crate outside the base set, the code that linked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkedRow {
    /// The name of the crate.
    pub name: String,
    /// Its version.
    pub version: String,
    /// Its digest.
    pub digest: String,
    /// Why the card links it, one sentence.
    pub reason: String,
}

/// One page of the knowledge annex.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnowledgePage {
    /// The name of the page, its heading in the chapter.
    pub name: String,
    /// Where the page comes from.
    pub origin: String,
    /// The pin of the origin the page was taken at.
    pub pin: String,
    /// The page, Markdown that the writers read as the subset of the core.
    pub text: String,
    /// Whether the page has diverged from its origin since the pin.
    pub diverged: bool,
}

/// Writes `chapter` as `md/device.md` into `out`.
///
/// Each part is a heading of level 2, each table of the blueprint and each
/// knowledge page a heading of level 3, and the headings of a page sit below
/// that. A part without a row is the sentence `None.`, and a knowledge page
/// that writes no text adds no block. When `out` already holds text the page
/// starts after a blank line, and the page ends with one line feed.
pub fn render_device_markdown(chapter: &DeviceChapter, out: &mut String) {
    if !out.is_empty() {
        // A blank line ends the paragraph or the table the text before ends
        // with, so the title is no line of it.
        if !out.ends_with('\n') {
            out.push('\n');
        }
        if !out.ends_with("\n\n") {
            out.push('\n');
        }
    }
    let start = out.len();
    walk(chapter, &mut Markdown { out, start });
}

/// Writes `chapter` as `typ/05-device.typ`, with the `pdf.attach` call of the
/// card, into `out`.
///
/// The file opens with the import of `/sabi-theme.typ`, since an included
/// file sees no binding of the file that includes it, and holds one call of
/// the theme per line. The title is `#sv-title(1, ..)` after the label
/// `device`, each part `#sv-title(2, ..)` and each table of the blueprint and
/// each knowledge page `#sv-h(3, ..)`, outside the outline. The thumbnail,
/// when there is one, is `#sv-image(bytes(..))`, and the card is attached by
/// its path from the root of the tree as `image/png` with a description that
/// names the title and the hash. When `out` already holds text the file
/// starts on a line of its own.
pub fn render_device_typst(chapter: &DeviceChapter, out: &mut String) {
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("#import \"/sabi-theme.typ\": *\n");
    walk(chapter, &mut Typst { out });
}

/// The columns of the table of the card.
const CARD_COLUMNS: [&str; 2] = ["Field", "Value"];

/// The columns of a table of the blueprint.
const FACT_COLUMNS: [&str; 4] = ["Fact", "Value", "Mark", "Sources"];

/// The columns of the table of the payload.
const PAYLOAD_COLUMNS: [&str; 5] = ["Part", "Files", "Raw bytes", "Packed bytes", "Digest"];

/// The columns of the table of the code that ran.
const RAN_COLUMNS: [&str; 3] = ["Path", "Digest", "Purpose"];

/// The columns of the table of the code that linked.
const LINKED_COLUMNS: [&str; 4] = ["Crate", "Version", "Digest", "Reason"];

/// The sentence of a part without a row.
const NONE: &str = "None.";

/// The level of the title of the chapter, the level of a page that opens a
/// chapter.
const TITLE_LEVEL: u8 = 1;

/// The level of the heading of a part.
const PART_LEVEL: u8 = 2;

/// The level of the heading of a table of the blueprint and of a knowledge
/// page, which the headings of the page sit below, as demotes them.
const HEADING_LEVEL: u8 = 3;

/// The label of the chapter in the Typst document, which no anchor of an
/// element and no label of a page of the core takes, since an anchor holds the
/// word of its kind and a hyphen and a page label is one of six words or
/// begins with `cluster-`.
const LABEL: &str = "device";

/// The bytes of a path the link to the card keeps as they are besides the
/// letters and the digits, the unreserved characters of a URL and the
/// separator of its segments.
const PATH_BYTES: &[u8] = b"-._~/";

/// One cell of a table, its lines, which a writer joins with a line break.
type Cell = Vec<String>;

/// One row of a table.
type Row = Vec<Cell>;

/// The writer of one format, which [`walk`] drives through the parts of a
/// chapter in their order.
trait Writer {
    /// Writes the title of the chapter.
    fn title(&mut self, title: &str);

    /// Writes the picture of the chapter: the card in Markdown, and the
    /// thumbnail and the attachment of the card in Typst.
    fn picture(&mut self, chapter: &DeviceChapter);

    /// Writes the heading of a part.
    fn part(&mut self, title: &str);

    /// Writes the heading of a table of the blueprint or of a knowledge page
    /// inside a part.
    fn heading(&mut self, title: &str);

    /// Writes a table with its header row.
    fn table(&mut self, columns: &[&str], rows: &[Row]);

    /// Writes a paragraph of the one text `text`.
    fn note(&mut self, text: &str);

    /// Writes a knowledge page, the Markdown text `text`.
    fn page(&mut self, text: &str);
}

/// Writes the parts of `chapter` in their order through `w`.
fn walk(chapter: &DeviceChapter, w: &mut impl Writer) {
    w.title(&chapter.title);
    w.picture(chapter);
    w.part("Card");
    w.table(&CARD_COLUMNS, &card_rows(chapter));
    w.part("Blueprint");
    if chapter.blueprint.is_empty() {
        w.note(NONE);
    }
    for table in &chapter.blueprint {
        w.heading(&table.title);
        let rows: Vec<Row> = table.rows.iter().map(fact_row).collect();
        table_or_none(w, &FACT_COLUMNS, &rows);
    }
    w.part("Payload");
    let rows: Vec<Row> = chapter.payload.iter().map(payload_row).collect();
    table_or_none(w, &PAYLOAD_COLUMNS, &rows);
    w.part("Code that ran");
    let rows: Vec<Row> = chapter.ran.iter().map(ran_row).collect();
    table_or_none(w, &RAN_COLUMNS, &rows);
    w.part("Code that linked");
    let rows: Vec<Row> = chapter.linked.iter().map(linked_row).collect();
    table_or_none(w, &LINKED_COLUMNS, &rows);
    w.part("Knowledge");
    if chapter.knowledge.is_empty() {
        w.note(NONE);
    }
    for page in &chapter.knowledge {
        w.heading(&page.name);
        w.note(&origin_line(page));
        w.page(&page.text);
    }
}

/// Writes `rows` under `columns`, or the sentence of a part without a row.
fn table_or_none(w: &mut impl Writer, columns: &[&str], rows: &[Row]) {
    if rows.is_empty() {
        w.note(NONE);
    } else {
        w.table(columns, rows);
    }
}

/// Returns the cell of the one line `text`.
fn cell(text: impl Into<String>) -> Cell {
    vec![text.into()]
}

/// Returns the rows of the table of the card.
fn card_rows(chapter: &DeviceChapter) -> Vec<Row> {
    vec![
        vec![cell("Device"), cell(chapter.id.as_str())],
        vec![cell("Revision"), cell(chapter.revision.to_string())],
        vec![cell("Card"), cell(chapter.card_file.as_str())],
        vec![cell("Card SHA-256"), cell(chapter.card_sha256.as_str())],
        vec![cell("Signature"), cell(signature_text(&chapter.signature))],
        vec![
            cell("Source date epoch"),
            cell(chapter.source_date_epoch.to_string()),
        ],
    ]
}

/// Returns the words of the signature status `signature`.
fn signature_text(signature: &SignatureStatus) -> String {
    match signature {
        SignatureStatus::Unsigned => String::from("unsigned"),
        SignatureStatus::Signed { signers, tier } if signers.is_empty() => {
            format!("signed, tier {tier}")
        }
        SignatureStatus::Signed { signers, tier } => {
            format!("signed by {}, tier {tier}", signers.join(", "))
        }
        SignatureStatus::Revoked { serial } => {
            format!("revoked, revocation list serial {serial}")
        }
    }
}

/// Returns the row of the fact `fact`.
fn fact_row(fact: &Fact) -> Row {
    vec![
        cell(fact.name.as_str()),
        cell(fact.value.as_str()),
        mark_cell(&fact.mark),
        cell(fact.sources.join(", ")),
    ]
}

/// Returns the cell of the mark `mark`: its word in lower case, and for an
/// unknown fact a line of its candidates and a line of what closes it when the
/// card names them.
fn mark_cell(mark: &Mark) -> Cell {
    let word = match mark {
        Mark::Proven => "proven",
        Mark::Documented => "documented",
        Mark::Measured => "measured",
        Mark::Inventoried => "inventoried",
        Mark::Assumed => "assumed",
        Mark::Unknown { candidates, closes } => {
            let mut lines = cell("unknown");
            if !candidates.is_empty() {
                lines.push(format!("candidates {}", candidates.join(", ")));
            }
            if !closes.is_empty() {
                lines.push(format!("closes {closes}"));
            }
            return lines;
        }
    };
    cell(word)
}

/// Returns the row of the part `row` of the payload.
fn payload_row(row: &PayloadRow) -> Row {
    vec![
        cell(row.part.as_str()),
        cell(row.files.to_string()),
        cell(row.raw.to_string()),
        cell(row.packed.to_string()),
        cell(row.digest.as_str()),
    ]
}

/// Returns the row of the build script `row`.
fn ran_row(row: &RanRow) -> Row {
    vec![
        cell(row.path.as_str()),
        cell(row.digest.as_str()),
        cell(row.purpose.as_str()),
    ]
}

/// Returns the row of the crate `row`.
fn linked_row(row: &LinkedRow) -> Row {
    vec![
        cell(row.name.as_str()),
        cell(row.version.as_str()),
        cell(row.digest.as_str()),
        cell(row.reason.as_str()),
    ]
}

/// Returns the sentence of the origin and the pin of `page`.
fn origin_line(page: &KnowledgePage) -> String {
    let diverged = if page.diverged {
        ", diverged from the origin"
    } else {
        ""
    };
    format!("Origin {}, pin {}{diverged}.", page.origin, page.pin)
}

/// Returns the description of the attachment of the card, which names the
/// title and the hash.
fn attachment_description(chapter: &DeviceChapter) -> String {
    format!(
        "The device card of {}, SHA-256 {}",
        chapter.title, chapter.card_sha256
    )
}

/// The writer of `md/device.md`.
struct Markdown<'o> {
    /// The text written.
    out: &'o mut String,
    /// The length of `out` before the page.
    start: usize,
}

impl Markdown<'_> {
    /// Starts a new block, one blank line after the text before it.
    fn gap(&mut self) {
        if self.out.len() == self.start {
            return;
        }
        if !self.out.ends_with('\n') {
            self.out.push('\n');
        }
        if !self.out[self.start..].ends_with("\n\n") {
            self.out.push('\n');
        }
    }

    /// Writes a heading of `level` that reads `text`.
    fn heading_of(&mut self, level: u8, text: &str) {
        self.gap();
        for _ in 0..level {
            self.out.push('#');
        }
        self.out.push(' ');
        md_text(text, self.out);
        self.out.push('\n');
    }

    /// Writes one row of a table, every cell its lines joined by `<br>`.
    fn row(&mut self, cells: &[Cell]) {
        self.out.push('|');
        for lines in cells {
            let mark = self.out.len();
            self.out.push(' ');
            for (i, line) in lines.iter().enumerate() {
                if i > 0 {
                    self.out.push_str("<br>");
                }
                md_cell(line, self.out);
            }
            if self.out.len() == mark + 1 {
                self.out.truncate(mark);
            }
            self.out.push_str(" |");
        }
        self.out.push('\n');
    }
}

impl Writer for Markdown<'_> {
    fn title(&mut self, title: &str) {
        self.heading_of(TITLE_LEVEL, title);
    }

    fn picture(&mut self, chapter: &DeviceChapter) {
        self.gap();
        self.out.push_str("![");
        md_text(&format!("The device card of {}", chapter.title), self.out);
        self.out.push_str("](../");
        push_path(&chapter.card_file, self.out);
        self.out.push_str(")\n");
    }

    fn part(&mut self, title: &str) {
        self.heading_of(PART_LEVEL, title);
    }

    fn heading(&mut self, title: &str) {
        self.heading_of(HEADING_LEVEL, title);
    }

    fn table(&mut self, columns: &[&str], rows: &[Row]) {
        self.gap();
        let header: Vec<Cell> = columns.iter().map(|&c| cell(c)).collect();
        self.row(&header);
        self.out.push('|');
        for _ in columns {
            self.out.push_str(" --- |");
        }
        self.out.push('\n');
        for row in rows {
            self.row(row);
        }
    }

    fn note(&mut self, text: &str) {
        self.gap();
        md_text(text, self.out);
        self.out.push('\n');
    }

    fn page(&mut self, text: &str) {
        let doc = parse(text, &ReadLimits::DEFAULT);
        let mut page = String::new();
        to_markdown(&doc, text, HEADING_LEVEL, RawHtml::Escaped, &mut page);
        // The rule reads the page as written, whose escaped HTML opens no
        // HTML block that would hold a fence line as its text.
        let fence = open_fence(&page);
        let body = page.trim_end_matches(['\n', '\r']).len();
        page.truncate(body);
        if let Some((byte, len)) = fence {
            page.push('\n');
            for _ in 0..len {
                page.push(char::from(byte));
            }
        }
        if page.is_empty() {
            return;
        }
        self.gap();
        self.out.push_str(&page);
        self.out.push('\n');
    }
}

/// Writes `path` as the path of a relative link, every byte outside the
/// letters, the digits and `PATH_BYTES` percent encoded.
///
/// The parentheses, the backslash, `&`, `%`, `:` and every space and control
/// character are encoded among them, so the destination needs no angle
/// brackets, holds no entity and names no scheme.
///
/// [CommonMark specification]: https://spec.commonmark.org/0.31.2/
fn push_path(path: &str, out: &mut String) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for b in path.bytes() {
        if b.is_ascii_alphanumeric() || PATH_BYTES.contains(&b) {
            out.push(char::from(b));
        } else {
            out.push('%');
            out.push(char::from(HEX[usize::from(b >> 4)]));
            out.push(char::from(HEX[usize::from(b & 0x0f)]));
        }
    }
}

/// The writer of `typ/05-device.typ`.
struct Typst<'o> {
    /// The text written.
    out: &'o mut String,
}

/// An array literal being written, which counts its elements.
///
/// An array of one element keeps a trailing comma, since `(x)` is the
/// parenthesized `x` and `(x,)` the array.
struct Seq {
    /// The elements written so far.
    count: u32,
}

impl Seq {
    /// Opens an array literal in `out`.
    fn open(out: &mut String) -> Seq {
        out.push('(');
        Seq { count: 0 }
    }

    /// Starts the next element in `out`.
    fn next(&mut self, out: &mut String) {
        if self.count > 0 {
            out.push_str(", ");
        }
        self.count = self.count.saturating_add(1);
    }

    /// Closes the array literal in `out`.
    fn close(self, out: &mut String) {
        if self.count == 1 {
            out.push(',');
        }
        out.push(')');
    }
}

impl Typst<'_> {
    /// Writes the call `name` of the one text `text`, such as
    /// `#sv-p(("text",))`, on a line of its own.
    fn call(&mut self, name: &str, level: Option<u8>, text: &str) {
        self.out.push('#');
        self.out.push_str(name);
        self.out.push('(');
        if let Some(level) = level {
            // Writing into a `String` cannot fail.
            let _ = write!(self.out, "{level}, ");
        }
        self.out.push('(');
        typst_str(text, self.out);
        self.out.push_str(",))\n");
    }

    /// Writes one cell of a table as an array of inlines, its lines
    /// separated by `sv-br()`.
    fn cell(&mut self, lines: &[String]) {
        let mut seq = Seq::open(self.out);
        for (i, line) in lines.iter().enumerate() {
            if i > 0 {
                seq.next(self.out);
                self.out.push_str("sv-br()");
            }
            seq.next(self.out);
            typst_str(line, self.out);
        }
        seq.close(self.out);
    }
}

impl Writer for Typst<'_> {
    fn title(&mut self, title: &str) {
        self.out.push_str("#sv-anchor(label(");
        typst_str(LABEL, self.out);
        self.out.push_str("))\n");
        self.call("sv-title", Some(TITLE_LEVEL), title);
    }

    fn picture(&mut self, chapter: &DeviceChapter) {
        if let Some(bytes) = chapter.thumbnail.as_deref().filter(|b| !b.is_empty()) {
            self.out.push_str("#sv-image(bytes(");
            let mut seq = Seq::open(self.out);
            for b in bytes {
                seq.next(self.out);
                let _ = write!(self.out, "{b}");
            }
            seq.close(self.out);
            self.out.push_str("))\n");
        }
        self.out.push_str("#pdf.attach(");
        typst_str(&format!("/{}", chapter.card_file), self.out);
        self.out
            .push_str(", relationship: \"source\", mime-type: \"image/png\", description: ");
        typst_str(&attachment_description(chapter), self.out);
        self.out.push_str(")\n");
    }

    fn part(&mut self, title: &str) {
        self.call("sv-title", Some(PART_LEVEL), title);
    }

    fn heading(&mut self, title: &str) {
        self.call("sv-h", Some(HEADING_LEVEL), title);
    }

    fn table(&mut self, columns: &[&str], rows: &[Row]) {
        self.out.push_str("#sv-table((), ");
        let mut header = Seq::open(self.out);
        for &column in columns {
            header.next(self.out);
            self.cell(&cell(column));
        }
        header.close(self.out);
        self.out.push_str(", ");
        let mut body = Seq::open(self.out);
        for row in rows {
            body.next(self.out);
            let mut cells = Seq::open(self.out);
            for lines in row {
                cells.next(self.out);
                self.cell(lines);
            }
            cells.close(self.out);
        }
        body.close(self.out);
        self.out.push_str(")\n");
    }

    fn note(&mut self, text: &str) {
        self.call("sv-p", None, text);
    }

    fn page(&mut self, text: &str) {
        let doc = parse(text, &ReadLimits::DEFAULT);
        self.out.push_str("#sv-desc(");
        to_typst(&doc, text, self.out);
        let _ = writeln!(self.out, ", level: {HEADING_LEVEL})");
    }
}
