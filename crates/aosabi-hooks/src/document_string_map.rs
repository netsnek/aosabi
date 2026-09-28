// SPDX-License-Identifier: MIT
/*
 * aosabi-hooks: the map from a parsed document to its source text.
 */

//! `DocumentStringMap`.
//!
//! The hook layer keeps the source text of every document it parsed, as the
//! reference keeps it in a weak map keyed by the document, so a plugin that
//! needs the text of a document finds it without printing it again. A document
//! here is shared through an `Arc`, so the map keys it by the address of that
//! `Arc` and keeps a weak handle beside it. The map belongs to the thread, as
//! the module's map belongs to the process of the reference.
//!
//! The orchestrator sets the text of every document it parses, a hit of the
//! request cache included, and the parse keeps that text as the body of the
//! document's own source. An entry therefore keeps a text only where the
//! text differs from that body, which [`Source::body_eq`] decides, and the
//! map answers the body for an entry that keeps none. A text differs where a
//! parse function answers a document of another text or one without a
//! location, and where [`get_document_string`] keeps a printed text. A parse
//! hook that compared the body with the text already, as the request cache
//! does on a hit, marks the document, and [`DocumentStringMap::set_of_source`]
//! keeps the body with no second compare.
//!
//! The entries stand in an index of open addressing with linear probing, a
//! power of two of slots at most half full. The entry of a dropped document
//! stays until the index would grow. Then every such entry leaves, and the
//! index doubles only where the live entries still fill a quarter of it, so
//! each sweep that keeps the size frees a quarter of the slots or more.

use core::cell::RefCell;
use std::sync::{
    Arc,
    Weak, //
};

use aosabi_execute::execute::SharedDocument;
use aosabi_lang::{
    Source,
    ast::DocumentNode, //
};

/// The multiplier of the home slot, the golden ratio in fixed point at the width of `usize`.
///
/// The home slot takes the top bits of the address times this odd number,
/// and those bits depend on every bit of the address, where the low bits of
/// the product depend on the low bits of the address alone.
#[cfg(target_pointer_width = "64")]
const GOLDEN: usize = 0x9E37_79B9_7F4A_7C15;

/// The multiplier of the home slot, the golden ratio in fixed point at the width of `usize`.
#[cfg(target_pointer_width = "32")]
const GOLDEN: usize = 0x9E37_79B9;

/// The slots of the index when it takes its first entry.
const FIRST_SLOTS: usize = 64;

/// The entry of one parsed document.
struct Entry {
    /// The address of the document, which no other document takes while
    /// `document` stands.
    address: usize,
    /// The weak handle of the document.
    document: Weak<DocumentNode>,
    /// The text set for the document, none where it is the body of the
    /// document's own source.
    text: Option<Box<str>>,
}

/// The source texts of the parsed documents of one thread.
#[derive(Default)]
pub struct DocumentStringMap {
    /// The slots of the index, none or a power of two of them.
    slots: Vec<Option<Entry>>,
    /// The entries the slots hold, of live and of dropped documents.
    len: usize,
}

/// Returns the source `document` was parsed from, none for a document without a location.
fn own_source(document: &DocumentNode) -> Option<&Source> {
    document.loc.as_ref().map(|loc| &*loc.source)
}

/// Returns the address that keys `document` in the map.
fn address_of(document: &Arc<DocumentNode>) -> usize {
    Arc::as_ptr(document).addr()
}

impl DocumentStringMap {
    /// Sets the source text of `document`, `documentStringMap.set(document, source)`.
    ///
    /// The text is kept only where it differs from the body of the
    /// document's own source. The entries of dropped documents leave the map
    /// before it grows, as the entries of a weak map leave once their key is
    /// collected.
    pub fn set(&mut self, document: &Arc<DocumentNode>, source: &str) {
        let text = match own_source(document) {
            Some(own) if own.body_eq(source) => None,
            _ => Some(Box::from(source)),
        };
        self.keep(document, text);
    }

    /// Sets the source text of `document` to the body of the document's own source.
    ///
    /// This is `documentStringMap.set(document, source)` for a source a parse
    /// hook compared with that body and found equal, so the map keeps no text
    /// and compares nothing.
    pub fn set_of_source(&mut self, document: &SharedDocument) {
        self.keep(document, None);
    }

    /// Keeps `text` as the text of `document`, none for the body of its own source.
    fn keep(&mut self, document: &SharedDocument, text: Option<Box<str>>) {
        let address = address_of(document);
        match self.entry_mut(address) {
            Some(entry) => entry.text = text,
            None => self.insert(Entry {
                address,
                document: SharedDocument::downgrade(document),
                text,
            }),
        }
    }

    /// Returns the source text of `document`, `documentStringMap.get(document)`.
    ///
    /// The text is the one [`DocumentStringMap::set`] last set for
    /// `document`, read from the entry or from the body of the document's own
    /// source, which holds the same bytes. A document the map holds no entry
    /// for answers none.
    #[must_use]
    pub fn get(&self, document: &Arc<DocumentNode>) -> Option<String> {
        let entry = self.entry(address_of(document))?;
        match &entry.text {
            Some(text) => Some(String::from(&**text)),
            None => own_source(document).map(|own| String::from(own.slice(0, own.bytes().len()))),
        }
    }

    /// Returns the entry of `address`, none where the map holds none.
    fn entry(&self, address: usize) -> Option<&Entry> {
        if self.slots.is_empty() {
            return None;
        }
        self.slots[self.probe(address)].as_ref()
    }

    /// Returns the entry of `address` to change, none where the map holds none.
    fn entry_mut(&mut self, address: usize) -> Option<&mut Entry> {
        if self.slots.is_empty() {
            return None;
        }
        let i = self.probe(address);
        self.slots[i].as_mut()
    }

    /// Adds `entry`, whose address the map holds no entry for.
    fn insert(&mut self, entry: Entry) {
        if 2 * (self.len + 1) > self.slots.len() {
            self.make_room();
        }
        let i = self.probe(entry.address);
        self.slots[i] = Some(entry);
        self.len += 1;
    }

    /// Drops the entries of dropped documents, and doubles the index where
    /// the live entries still fill a quarter of it.
    fn make_room(&mut self) {
        let live = self
            .slots
            .iter()
            .flatten()
            .filter(|entry| entry.document.strong_count() > 0)
            .count();
        let mut size = self.slots.len().max(FIRST_SLOTS);
        if 4 * (live + 1) > size {
            size *= 2;
        }
        self.rebuild(size);
    }

    /// Moves the entries of live documents into an index of `size` slots and
    /// drops the others.
    fn rebuild(&mut self, size: usize) {
        let mut slots = Vec::new();
        slots.resize_with(size, || None);
        let old = core::mem::replace(&mut self.slots, slots);
        self.len = 0;
        for entry in old.into_iter().flatten() {
            if entry.document.strong_count() > 0 {
                let i = self.probe(entry.address);
                self.slots[i] = Some(entry);
                self.len += 1;
            }
        }
    }

    /// Returns the first slot the probe of `address` reads.
    fn home(&self, address: usize) -> usize {
        address.wrapping_mul(GOLDEN) >> (usize::BITS - self.slots.len().trailing_zeros())
    }

    /// Returns the slot that holds the entry of `address`, or else the free
    /// slot that ends its probe.
    ///
    /// The index holds slots and is at most half full, so every probe ends.
    fn probe(&self, address: usize) -> usize {
        let mask = self.slots.len() - 1;
        let mut i = self.home(address);
        while let Some(entry) = &self.slots[i] {
            if entry.address == address {
                break;
            }
            i = (i + 1) & mask;
        }
        i
    }
}

// An `unsafe` cell would skip the borrow flag.
std::thread_local! {
    /// The map of the thread, `documentStringMap` of the module.
    static DOCUMENT_STRING_MAP: RefCell<DocumentStringMap> = RefCell::new(DocumentStringMap::default());
}

/// Sets the source text of `document` in the map of the thread.
pub fn set_document_string(document: &Arc<DocumentNode>, source: &str) {
    DOCUMENT_STRING_MAP.with(|map| map.borrow_mut().set(document, source));
}

/// Sets the source text of `document` in the map of the thread to the body of its own source.
///
/// A parse hook that compared the body with the source marks the document,
/// and the orchestrator calls this in place of [`set_document_string`].
pub fn set_document_string_of_source(document: &SharedDocument) {
    DOCUMENT_STRING_MAP.with(|map| map.borrow_mut().set_of_source(document));
}

/// Returns the source text of `document`, printing it with `print` when the map holds none.
///
/// A printed text is kept for the next call.
#[must_use]
pub fn get_document_string(
    document: &Arc<DocumentNode>,
    print: Option<fn(&DocumentNode) -> String>,
) -> Option<String> {
    let known = DOCUMENT_STRING_MAP.with(|map| map.borrow().get(document));
    if known.is_some() {
        return known;
    }
    let printed = print?(document);
    set_document_string(document, &printed);
    Some(printed)
}
