// SPDX-License-Identifier: MIT
/*
 * aosabi-execute arena: the answer of one execution as nodes, entries and bytes.
 *
 * No analogue in the reference, whose result is a tree of JavaScript objects
 * that JSON.stringify walks once the execution settled.
 */

//! The arena of the answer of one execution.
//!
//! An execution writes its answer into one `Arena` of three vectors, the
//! nodes, the entries and the bytes, and names a node by a handle of 32 bits.
//! A node is `undefined`, `null`, a boolean, a number, a string held as its
//! escaped JSON literal in the bytes, an object whose entries are linked in
//! the order they were appended, a list whose items are entries side by
//! side, a value of the JSON model held as its JSON text in the bytes, or a
//! leaf that the writer of `aosabi-json` writes once the execution settled.
//! An entry holds the encoded key of its member, `"key":`, in the bytes, the
//! node of its value and the entry after it.
//!
//! The executor appends an entry to an object at the moment its value settles,
//! so the order of the entries is the settle order of the members, the order
//! the reference assigns the keys in. A completed value travels to the node
//! that holds it as a [`NodeRef`] inside a value, through every promise of the
//! job model, so no promise and no job is added or moved. A field whose error
//! bubbles writes one `null` into the nearest nullable position, and the nodes
//! of the values below it stay in the arena unread.
//!
//! A value of the JSON model enters with no conversion into a value of the
//! executor: the value a serializer answers, a `JSON` or a `JSONObject` value
//! among them, moves in as it is, and the value a member of an object holds is
//! written as its JSON text from where it stands, or moves in as a copy where
//! its text would read back as another value. A date a serializer answers as
//! the text of its `toJSON` enters as that text, written with no string made.
//!
//! An arena holds its leaves as the executor completed them until the
//! execution settles. A result then takes it as an [`Answer`], whose leaves
//! are values of the JSON model, a value of the executor by the rules of
//! [`Value::to_js`]. An answer is `Send` and `Sync` and is what a result
//! holds and writes. A reader that reads the data of the result object as a
//! tree gets a tree built from the arena, whose leaves are the values the
//! executor completed, and a value held as its JSON text reads back as the
//! value [`Value::to_js`] makes of the value it was written from.
//!
//! The arena and its handles are public items, so a crate may build an arena
//! of its own and hold the handles of its nodes. The executor places only the
//! completed values of its own execution, so a handle it reads in
//! [`Arena::place`] names a node its own completion made, and a handle that
//! any other value carries is a host value like any other, which the
//! conversion of [`Value::to_js`] writes as `{}`.
//!
//! The vectors of the nodes, the entries, the bytes and the items of the lists
//! stay with the thread between executions. An arena or an answer that drops
//! leaves them, emptied, in one slot of its thread, up to [`KEPT_BYTES`] of
//! them in all, and the arena [`Arena::new`] makes next takes them, so an
//! execution whose answer fits the room of the last one grows none of them.
//! The vectors hold `Copy` values and bytes and no value of a request, so a
//! kept vector carries nothing from one execution into the next but its room.
//!
//! The module names `core` and `alloc`, and of `std` the slot of each thread
//! alone, which `thread_local!` gives.

use alloc::{
    rc::Rc,
    string::String,
    vec::Vec, //
};
use core::{
    cell::Cell,
    fmt,
    mem::{
        self,
        size_of, //
    },
    str, //
};

use aosabi_json::{
    JsObject,
    JsString,
    JsValue,
    number,
    reader,
    string,
    writer, //
};
use aosabi_scalars::JsDate;

use crate::value::{
    HostValue,
    PlainObject,
    Value, //
};

/// The handle that names no entry, which ends the entries of an object.
const NONE: u32 = u32::MAX;

/// The bits of the number `-0`, which the writer writes as `0`.
const NEGATIVE_ZERO: u64 = (-0.0_f64).to_bits();

/// The bytes the writer appends per node beyond the bytes of the arena, a
/// bound on the punctuation and the short numbers of an answer.
const BYTES_PER_NODE: usize = 4;

/// The most bytes the vectors a thread keeps between executions hold in all,
/// 256 KiB in a Worker, the default of for a module of `wasm32`.
#[cfg(target_family = "wasm")]
pub const KEPT_BYTES: usize = 256 * 1024;

/// The most bytes the vectors a thread keeps between executions hold in all, 1
/// MiB natively, the default of.
#[cfg(not(target_family = "wasm"))]
pub const KEPT_BYTES: usize = 1024 * 1024;

/// The handle of a node of an arena, a completed value on its way to the node that holds it.
///
/// An arena hands out the handles of its own nodes. The executor places only
/// the completed values of its own execution, so a handle it reads in
/// [`Arena::place`] is one its own completion made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NodeRef(u32);

impl From<NodeRef> for Value {
    fn from(node: NodeRef) -> Self {
        Self::Host(HostValue::Node(node))
    }
}

/// One node of an answer.
#[derive(Clone, Copy, Debug)]
enum Node {
    /// `undefined`, which an object leaves out and a list writes as `null`.
    Undefined,
    /// `null`.
    Null,
    /// A boolean.
    Bool(bool),
    /// A number, written by `Number::toString`, and as `null` when it is not finite.
    Number(f64),
    /// A string, its escaped JSON literal with its quotation marks in the bytes.
    Str {
        /// The offset of the literal in the bytes.
        start: u32,
        /// The length of the literal.
        len: u32,
    },
    /// An object, whose entries are linked in the order they were appended.
    Object {
        /// The first entry, [`NONE`] for an empty object.
        head: u32,
        /// The last entry, [`NONE`] for an empty object.
        tail: u32,
        /// The count of the entries.
        len: u32,
    },
    /// A list, whose items are the entries side by side from `first` on.
    List {
        /// The entry of the first item.
        first: u32,
        /// The count of the items.
        len: u32,
    },
    /// A value of the JSON model held as its JSON text in the bytes, which
    /// the writer of `aosabi-json` wrote when the value completed.
    Json {
        /// The offset of the text in the bytes.
        start: u32,
        /// The length of the text.
        len: u32,
    },
    /// A leaf the writer of `aosabi-json` writes once the execution settled,
    /// by its index among the leaves.
    Leaf(u32),
}

/// A leaf an arena holds until the execution settles.
enum Held {
    /// A value of the executor, which becomes a value of the JSON model by
    /// the rules of [`Value::to_js`] when the execution settles.
    Value(Value),
    /// A value of the JSON model, which the arena holds as it is.
    Json(JsValue),
}

impl Held {
    /// Returns the value of the JSON model the leaf is once the execution settled.
    fn into_js(self) -> JsValue {
        match self {
            Self::Value(value) => value.to_js(),
            Self::Json(value) => value,
        }
    }

    /// Returns the leaf as a value of the executor.
    fn to_value(&self) -> Value {
        match self {
            Self::Value(value) => value.clone(),
            Self::Json(value) => Value::from_js(value),
        }
    }
}

/// A value of the JSON model that its holder keeps, which an arena writes
/// from where it stands.
#[derive(Clone, Copy)]
pub enum Kept<'a> {
    /// Any value of the JSON model.
    Value(&'a JsValue),
    /// An object of the JSON model.
    Object(&'a JsObject),
}

impl Kept<'_> {
    /// Returns whether the value is `null` or `undefined`.
    #[must_use]
    pub fn is_nullish(self) -> bool {
        matches!(self, Self::Value(JsValue::Null | JsValue::Undefined))
    }

    /// Returns whether the value is an object of the JSON model.
    #[must_use]
    pub fn is_object(self) -> bool {
        matches!(self, Self::Object(_) | Self::Value(JsValue::Object(_)))
    }

    /// Returns the value of the executor [`Value::from_js`] makes of the value.
    #[must_use]
    pub fn to_value(self) -> Value {
        match self {
            Self::Value(value) => Value::from_js(value),
            Self::Object(object) => Value::from_js_object(object),
        }
    }
}

/// One entry of an object or one item of a list.
#[derive(Clone, Copy, Debug)]
struct Entry {
    /// The offset of the encoded key, `"key":`, in the bytes, unread for an item.
    key: u32,
    /// The length of the encoded key, 0 for an item.
    key_len: u32,
    /// The node of the value.
    value: u32,
    /// The next entry of an object, [`NONE`] after the last one.
    next: u32,
}

/// The encoded key of an entry, `"key":`, where an arena holds it among its bytes.
#[derive(Clone, Copy, Debug)]
pub struct Key {
    /// The offset of the encoded key in the bytes.
    start: u32,
    /// The length of the encoded key.
    len: u32,
}

/// Returns the handle of the element at `at` of a vector of an arena.
///
/// An answer the reference writes holds fewer than 2^30 code units, the
/// length V8 gives a string at most, so no vector of an arena of an answer it
/// can write reaches 2^32 elements, and the handle that would saturate never
/// occurs.
fn handle(at: usize) -> u32 {
    u32::try_from(at).unwrap_or(NONE)
}

/// Returns the index a handle names.
fn index(at: u32) -> usize {
    at as usize
}

/// The nodes, the entries and the bytes of an arena or an answer, which every walk reads.
#[derive(Clone, Copy)]
struct Shape<'a> {
    /// The nodes.
    nodes: &'a [Node],
    /// The entries.
    entries: &'a [Entry],
    /// The bytes.
    bytes: &'a [u8],
}

impl<'a> Shape<'a> {
    /// Returns the node `at` names, and `undefined` for a handle that names none.
    // A `get_unchecked` on a handle proven in range would skip the bounds check.
    fn node(self, at: u32) -> Node {
        self.nodes
            .get(index(at))
            .copied()
            .unwrap_or(Node::Undefined)
    }

    /// Returns the `len` bytes from `start` on.
    fn text(self, start: u32, len: u32) -> &'a [u8] {
        let start = index(start);
        self.bytes
            .get(start..start.saturating_add(index(len)))
            .unwrap_or_default()
    }

    /// Returns the members of the object whose first entry is `head`, in the order they were appended.
    fn members(self, head: u32) -> Members<'a> {
        Members {
            shape: self,
            at: head,
        }
    }

    /// Returns the `len` entries of the items of a list from `first` on.
    fn items(self, first: u32, len: u32) -> &'a [Entry] {
        let first = index(first);
        self.entries
            .get(first..first.saturating_add(index(len)))
            .unwrap_or_default()
    }
}

/// The members of an object, each its encoded key and the node of its value.
struct Members<'a> {
    /// The arena the object stands in.
    shape: Shape<'a>,
    /// The next entry.
    at: u32,
}

impl<'a> Iterator for Members<'a> {
    type Item = (&'a [u8], u32);

    fn next(&mut self) -> Option<Self::Item> {
        let entry = self.shape.entries.get(index(self.at))?;
        self.at = entry.next;
        Some((self.shape.text(entry.key, entry.key_len), entry.value))
    }
}

/// Returns the response key an encoded key `"key":` spells.
///
/// A response key is a name of the GraphQL grammar, ASCII letters, digits and
/// the underscore, so its literal holds no escape.
// A `str::from_utf8_unchecked` would skip this check of bytes copied from a `str`.
fn key_text(encoded: &[u8]) -> &str {
    let name = encoded
        .get(1..encoded.len().saturating_sub(2))
        .unwrap_or_default();
    str::from_utf8(name).unwrap_or_default()
}

/// Returns the string an escaped JSON literal spells.
///
/// The literal is what [`string::escape`] wrote, valid JSON. The escape
/// writes a quotation mark, a backslash, a control character and a lone
/// surrogate as an escape and every other code point as its UTF-8, so a
/// literal with no backslash holds the bytes of its string between its
/// quotation marks, which are taken as they are. The reader of `aosabi-json`
/// reads the escapes of any other literal back to the code units they write,
/// a lone surrogate among them.
// A `str::from_utf8_unchecked` would skip this check of bytes the escape wrote as UTF-8.
fn string_of(literal: &[u8]) -> JsString {
    let inner = literal
        .get(1..literal.len().saturating_sub(1))
        .unwrap_or_default();
    if !inner.contains(&b'\\')
        && let Ok(text) = str::from_utf8(inner)
    {
        return JsString::from(text);
    }
    match aosabi_json::parse(literal) {
        Ok(JsValue::String(text)) => text,
        _ => JsString::new(),
    }
}

/// Returns the value of the JSON model the JSON text `text` of a node spells.
///
/// The writer of `aosabi-json` wrote the text from a value whose text reads
/// back (see [`reads_back`]), so the reader of `aosabi-json` reads it, and
/// `undefined` stands in where it would not.
// A `str::from_utf8_unchecked` would skip the check of UTF-8 `aosabi_json::parse` makes of the text the writer wrote as UTF-8.
fn json_of(text: &[u8]) -> JsValue {
    aosabi_json::parse(text).unwrap_or_default()
}

/// Answers whether the JSON text of `value`, `depth` levels of lists and
/// objects deep, reads back as the value [`normalized`] makes of it.
///
/// The writer of `aosabi-json` leaves out a property that is `undefined`,
/// writes the keys of an object in the order of `OrdinaryOwnPropertyKeys`, and
/// writes every string, every key and every finite number other than `-0` so
/// that its reader reads back the code units and the number written. A number
/// that is `-0` or not finite, an item of a list that is `undefined` and a
/// host object are written as `0`, `null`, `null` and `{}`, which read back as
/// other values, and the reader refuses a value nested deeper than
/// [`reader::MAX_DEPTH`].
fn reads_back(value: &JsValue, depth: usize) -> bool {
    match value {
        JsValue::Number(number) => number.is_finite() && number.to_bits() != NEGATIVE_ZERO,
        JsValue::Array(items) => {
            depth < reader::MAX_DEPTH
                && items
                    .iter()
                    .all(|item| !item.is_undefined() && reads_back(item, depth + 1))
        }
        JsValue::Object(object) => reads_back_object(object, depth),
        JsValue::Opaque(_) => false,
        JsValue::Undefined | JsValue::Null | JsValue::Bool(_) | JsValue::String(_) => true,
    }
}

/// Answers whether the JSON text of the object `object`, `depth` levels deep,
/// reads back as the object [`normalized_object`] makes of it (see
/// [`reads_back`]).
fn reads_back_object(object: &JsObject, depth: usize) -> bool {
    depth < reader::MAX_DEPTH
        && object
            .iter()
            .all(|(_, member)| reads_back(member, depth + 1))
}

/// Returns a copy of the value of the JSON model `value` as the conversions
/// of the executor read it, a value [`Value::to_js`] makes of the value
/// [`Value::from_js`] makes of `value`, with every key kept as its code units.
///
/// Each object holds its keys in the order of `OrdinaryOwnPropertyKeys` and
/// leaves out a property that is `undefined`, and every other value is
/// copied as it is.
fn normalized(value: &JsValue) -> JsValue {
    match value {
        JsValue::Array(items) => JsValue::Array(items.iter().map(normalized).collect()),
        JsValue::Object(object) => JsValue::Object(normalized_object(object)),
        other => other.clone(),
    }
}

/// Returns a copy of the object `object` as [`normalized`] copies an object.
fn normalized_object(object: &JsObject) -> JsObject {
    let mut copy = JsObject::with_capacity(object.len());
    for (key, member) in object.own_keys() {
        if !member.is_undefined() {
            copy.insert(key.clone(), normalized(member));
        }
    }
    copy
}

/// The vectors an arena leaves for the next execution of its thread, each empty.
struct Spare {
    /// The nodes.
    nodes: Vec<Node>,
    /// The entries.
    entries: Vec<Entry>,
    /// The nodes of the items of the lists being completed.
    items: Vec<u32>,
    /// The bytes.
    bytes: Vec<u8>,
}

impl Spare {
    /// The set of no vector.
    const NONE: Self = Self {
        nodes: Vec::new(),
        entries: Vec::new(),
        items: Vec::new(),
        bytes: Vec::new(),
    };

    /// Returns the set that holds, of each kind, the vector of `self` or of
    /// `other` with more room that fits the room [`KEPT_BYTES`] leaves after
    /// the kinds before it, emptied, and frees every other.
    fn merged(self, other: Self) -> Self {
        let mut room = KEPT_BYTES;
        Self {
            nodes: kept(self.nodes, other.nodes, &mut room),
            entries: kept(self.entries, other.entries, &mut room),
            items: kept(self.items, other.items, &mut room),
            bytes: kept(self.bytes, other.bytes, &mut room),
        }
    }
}

/// Returns the vector of `one` and `two` with more room that fits `room`
/// bytes, emptied, with `room` less its bytes, and frees the other.
fn kept<T>(one: Vec<T>, two: Vec<T>, room: &mut usize) -> Vec<T> {
    let (wide, narrow) = if one.capacity() >= two.capacity() {
        (one, two)
    } else {
        (two, one)
    };
    for mut vector in [wide, narrow] {
        let bytes = vector.capacity().saturating_mul(size_of::<T>());
        if bytes <= *room {
            *room -= bytes;
            vector.clear();
            return vector;
        }
    }
    Vec::new()
}

std::thread_local! {
    /// The vectors the arenas and answers that dropped on this thread left,
    /// which the next arena takes.
    static SPARE: Cell<Spare> = const { Cell::new(Spare::NONE) };
}

/// Leaves `spare` in the slot of this thread, beside what the slot holds.
///
/// A thread whose slot is gone, since the thread ends, frees the vectors.
fn leave(spare: Spare) {
    let _ = SPARE.try_with(|slot| {
        let held = slot.replace(Spare::NONE);
        slot.set(held.merged(spare));
    });
}

/// The arena an execution writes its answer into.
///
/// The executor makes one per execution with [`Arena::new`] and fills it as
/// the values of the fields complete: an object takes a member when its
/// value settles, a list takes its items once they all completed, and a leaf
/// takes a node of its own. The data of the execution is the node its root
/// object completes as, and [`Arena::into_answer`] hands the arena over as
/// the [`Answer`] a result writes. An arena that drops leaves its vectors to
/// its thread (see the module documentation), and [`Arena::default`] is an
/// arena of no vector, which takes nothing the thread keeps.
#[derive(Default)]
pub struct Arena {
    /// The nodes.
    nodes: Vec<Node>,
    /// The entries of the objects and the items of the lists.
    entries: Vec<Entry>,
    /// The escaped strings, the encoded keys and the JSON texts of values.
    bytes: Vec<u8>,
    /// The leaves the writer of `aosabi-json` writes, as the executor completed them.
    leaves: Vec<Held>,
    /// The nodes of the items of the lists being completed, the innermost list last.
    items: Vec<u32>,
}

impl Drop for Arena {
    fn drop(&mut self) {
        leave(Spare {
            nodes: mem::take(&mut self.nodes),
            entries: mem::take(&mut self.entries),
            items: mem::take(&mut self.items),
            bytes: mem::take(&mut self.bytes),
        });
    }
}

impl Arena {
    /// Returns an arena whose vectors are the ones the last arena of this
    /// thread left, empty, or new vectors where it left none.
    #[must_use]
    pub fn new() -> Self {
        let spare = SPARE
            .try_with(|slot| slot.replace(Spare::NONE))
            .unwrap_or(Spare::NONE);
        Self {
            nodes: spare.nodes,
            entries: spare.entries,
            bytes: spare.bytes,
            leaves: Vec::new(),
            items: spare.items,
        }
    }

    /// Returns the nodes, the entries and the bytes of the arena.
    fn shape(&self) -> Shape<'_> {
        Shape {
            nodes: &self.nodes,
            entries: &self.entries,
            bytes: &self.bytes,
        }
    }

    /// Appends `node` and returns its handle.
    fn push(&mut self, node: Node) -> u32 {
        let at = handle(self.nodes.len());
        self.nodes.push(node);
        at
    }

    /// Returns a new empty object.
    pub fn object(&mut self) -> NodeRef {
        NodeRef(self.push(Node::Object {
            head: NONE,
            tail: NONE,
            len: 0,
        }))
    }

    /// Returns the encoded key `encoded`, `"key":`, copied among the bytes.
    pub fn key(&mut self, encoded: &str) -> Key {
        let start = handle(self.bytes.len());
        self.bytes.extend_from_slice(encoded.as_bytes());
        Key {
            start,
            len: handle(encoded.len()),
        }
    }

    /// Appends the member `key` with the completed value `value` to `object`.
    pub fn append(&mut self, object: NodeRef, key: Key, value: Value) {
        let value = self.place(value);
        self.link(object.0, key, value.0);
    }

    /// Appends the member `encoded` with the completed value `value` to `object`, its key copied first.
    #[inline]
    pub fn put(&mut self, object: NodeRef, encoded: &str, value: Value) {
        let key = self.key(encoded);
        self.append(object, key, value);
    }

    /// Appends the member `encoded` with the leaf `value` to `object`, its key copied first.
    ///
    /// This is [`Arena::put`] of the node [`Arena::leaf`] makes of `value`.
    pub fn put_leaf(&mut self, object: NodeRef, encoded: &str, value: Value) {
        let key = self.key(encoded);
        let value = self.leaf_node(value);
        self.link(object.0, key, value);
    }

    /// Appends the member `encoded` with the string `text` to `object`, its key copied first.
    ///
    /// This is [`Arena::put`] of the node [`Arena::text`] makes of `text`.
    pub fn put_text(&mut self, object: NodeRef, encoded: &str, text: &str) {
        let key = self.key(encoded);
        let value = self.text(text);
        self.link(object.0, key, value.0);
    }

    /// Appends the member `encoded` with the string whose escaped JSON
    /// literal is `literal` to `object`, its key and its literal copied.
    ///
    /// The literal holds its quotation marks and is the one
    /// [`string::escape_str`] writes of its string, so the node equals the
    /// one [`Arena::text`] makes of that string with no scan of its bytes.
    /// The executor appends the value of `__typename` this way, the literal
    /// its plan holds for the type of a node (see
    /// [`PlanNode::typename`](crate::plan::PlanNode::typename)).
    pub(crate) fn put_literal(&mut self, object: NodeRef, encoded: &str, literal: &str) {
        let key = self.key(encoded);
        let start = self.bytes.len();
        self.bytes.extend_from_slice(literal.as_bytes());
        let value = self.push(Node::Str {
            start: handle(start),
            len: handle(literal.len()),
        });
        self.link(object.0, key, value);
    }

    /// Links a new entry of `key` and the node `value` after the last entry of `object`.
    fn link(&mut self, object: u32, key: Key, value: u32) {
        let at = handle(self.entries.len());
        let Some(Node::Object { head, tail, len }) = self.nodes.get_mut(index(object)) else {
            return;
        };
        self.entries.push(Entry {
            key: key.start,
            key_len: key.len,
            value,
            next: NONE,
        });
        if *head == NONE {
            *head = at;
        } else if let Some(last) = self.entries.get_mut(index(*tail)) {
            last.next = at;
        }
        *tail = at;
        *len = len.saturating_add(1);
    }

    /// Returns the leaf `value` as a node of the arena.
    ///
    /// A string is escaped into the bytes, `undefined`, `null`, a boolean and
    /// a number are a node of their own, and every other value is a leaf the
    /// writer of `aosabi-json` writes once the execution settled.
    pub fn leaf(&mut self, value: Value) -> NodeRef {
        NodeRef(self.leaf_node(value))
    }

    /// Returns the string `text` as a node of the arena, its escaped literal among the bytes.
    ///
    /// The node equals the one [`Arena::leaf`] makes of the string of the
    /// same text, since the escape of a Rust string writes the bytes of the
    /// escape of a string with no lone surrogate (see [`string::escape_str`]).
    pub fn text(&mut self, text: &str) -> NodeRef {
        let start = self.bytes.len();
        string::escape_str(&mut self.bytes, text);
        NodeRef(self.push(Node::Str {
            start: handle(start),
            len: handle(self.bytes.len() - start),
        }))
    }

    /// Returns the value of the JSON model `value` as a node of the arena, moved in with no copy.
    ///
    /// A string is escaped into the bytes, `undefined`, `null`, a boolean
    /// and a number are a node of their own, and a list, an object and a
    /// host object are a leaf the arena holds as they are, which the writer
    /// of `aosabi-json` writes once the execution settled.
    pub fn json(&mut self, value: JsValue) -> NodeRef {
        NodeRef(self.json_node(value))
    }

    /// Appends the member `encoded` with the value of the JSON model `value` to `object`, its key copied first.
    ///
    /// This is [`Arena::put`] of the node [`Arena::json`] makes of `value`.
    pub fn put_json(&mut self, object: NodeRef, encoded: &str, value: JsValue) {
        let key = self.key(encoded);
        let value = self.json_node(value);
        self.link(object.0, key, value);
    }

    /// Returns the value of the JSON model `value`, which its holder keeps,
    /// as a node of the arena, written from where it stands.
    ///
    /// A string is escaped into the bytes, `undefined`, `null`, a boolean
    /// and a number are a node of their own, and a list and an object are
    /// written as their JSON text into the bytes, which reads back as the
    /// value [`Value::to_js`] makes of the value [`Value::from_js`] makes of
    /// `value`, its keys kept as their code units. A list or an object whose
    /// text would read back as another value, since it holds a number that
    /// is `-0` or not finite, an item that is `undefined`, a host object or
    /// more nesting than the reader of `aosabi-json` reads, is held as that
    /// value, a copy, and a host object alone is held as it is.
    pub fn kept(&mut self, value: Kept<'_>) -> NodeRef {
        NodeRef(self.kept_node(value))
    }

    /// Appends the member `encoded` with the value of the JSON model `value`, which its holder keeps, to `object`, its key copied first.
    ///
    /// This is [`Arena::put`] of the node [`Arena::kept`] makes of `value`.
    pub fn put_kept(&mut self, object: NodeRef, encoded: &str, value: Kept<'_>) {
        let key = self.key(encoded);
        let value = self.kept_node(value);
        self.link(object.0, key, value);
    }

    /// Returns the date `date` as a node of the arena, the value of its `toJSON`.
    ///
    /// A valid date is the string of `toISOString`, whose digits are written
    /// into the bytes with no string made, and an invalid date is `null`,
    /// which `toJSON` answers for it.
    pub fn date(&mut self, date: JsDate) -> NodeRef {
        NodeRef(self.date_node(date))
    }

    /// Appends the member `encoded` with the date `date` to `object`, its key copied first.
    ///
    /// This is [`Arena::put`] of the node [`Arena::date`] makes of `date`.
    pub fn put_date(&mut self, object: NodeRef, encoded: &str, date: JsDate) {
        let key = self.key(encoded);
        let value = self.date_node(date);
        self.link(object.0, key, value);
    }

    /// Returns the handle of the node of the leaf `value`.
    fn leaf_node(&mut self, value: Value) -> u32 {
        let node = match value {
            Value::Undefined => Node::Undefined,
            Value::Null => Node::Null,
            Value::Bool(boolean) => Node::Bool(boolean),
            Value::Number(number) => Node::Number(number),
            Value::String(text) => self.escaped(&text),
            leaf => self.hold(Held::Value(leaf)),
        };
        self.push(node)
    }

    /// Returns the handle of the node of the value of the JSON model `value`, moved in.
    fn json_node(&mut self, value: JsValue) -> u32 {
        let node = match value {
            JsValue::Undefined => Node::Undefined,
            JsValue::Null => Node::Null,
            JsValue::Bool(boolean) => Node::Bool(boolean),
            JsValue::Number(number) => Node::Number(number),
            JsValue::String(text) => self.escaped(&text),
            leaf => self.hold(Held::Json(leaf)),
        };
        self.push(node)
    }

    /// Returns the handle of the node of the value of the JSON model `value`, which its holder keeps.
    fn kept_node(&mut self, value: Kept<'_>) -> u32 {
        let node = match value {
            Kept::Value(JsValue::Undefined) => Node::Undefined,
            Kept::Value(JsValue::Null) => Node::Null,
            Kept::Value(JsValue::Bool(boolean)) => Node::Bool(*boolean),
            Kept::Value(JsValue::Number(number)) => Node::Number(*number),
            Kept::Value(JsValue::String(text)) => self.escaped(text),
            Kept::Value(JsValue::Object(object)) | Kept::Object(object) => self.kept_object(object),
            Kept::Value(list @ JsValue::Array(_)) if reads_back(list, 0) => {
                let start = self.bytes.len();
                writer::write(&mut self.bytes, list);
                self.text_since(start)
            }
            Kept::Value(other) => self.hold(Held::Json(normalized(other))),
        };
        self.push(node)
    }

    /// Returns the node of the object `object`, which its holder keeps, its
    /// JSON text where it reads back and a copy otherwise.
    fn kept_object(&mut self, object: &JsObject) -> Node {
        if !reads_back_object(object, 0) {
            return self.hold(Held::Json(JsValue::Object(normalized_object(object))));
        }
        let start = self.bytes.len();
        writer::write_object(&mut self.bytes, object);
        self.text_since(start)
    }

    /// Returns the handle of the node of the value of `toJSON` of the date `date`.
    fn date_node(&mut self, date: JsDate) -> u32 {
        let start = self.bytes.len();
        self.bytes.push(b'"');
        let node = if date.write_iso_string(&mut self.bytes) {
            self.bytes.push(b'"');
            Node::Str {
                start: handle(start),
                len: handle(self.bytes.len() - start),
            }
        } else {
            self.bytes.truncate(start);
            Node::Null
        };
        self.push(node)
    }

    /// Returns the node of the string `text`, its escaped literal appended to the bytes.
    fn escaped(&mut self, text: &JsString) -> Node {
        let start = self.bytes.len();
        string::escape(&mut self.bytes, text);
        Node::Str {
            start: handle(start),
            len: handle(self.bytes.len() - start),
        }
    }

    /// Returns the node of the JSON text the bytes hold from `start` on.
    fn text_since(&self, start: usize) -> Node {
        Node::Json {
            start: handle(start),
            len: handle(self.bytes.len() - start),
        }
    }

    /// Holds the leaf `leaf` and returns its node.
    fn hold(&mut self, leaf: Held) -> Node {
        let at = handle(self.leaves.len());
        self.leaves.push(leaf);
        Node::Leaf(at)
    }

    /// Returns the node of the completed value `value`.
    ///
    /// A completed object, list or leaf is a [`NodeRef`] already, `null` and
    /// `undefined` become a node of their own, and a list is the list
    /// `Promise.all` fulfilled with the completed values of the items of a
    /// list that held a promise, whose items are placed in their order.
    pub fn place(&mut self, value: Value) -> NodeRef {
        match value {
            Value::Host(HostValue::Node(node)) => node,
            Value::List(items) => {
                let mark = self.items.len();
                for item in items.iter() {
                    let at = self.place(item.clone());
                    self.items.push(at.0);
                }
                self.list(mark)
            }
            value => NodeRef(self.leaf_node(value)),
        }
    }

    /// Returns the count of the items the lists being completed hold, the mark of a list that starts now.
    #[must_use]
    pub fn items_mark(&self) -> usize {
        self.items.len()
    }

    /// Adds the completed value `value` as the next item of the list being completed.
    pub fn push_item(&mut self, value: Value) {
        let at = self.place(value);
        self.items.push(at.0);
    }

    /// Moves the items of the list that started at `mark` into `values`, each as its completed value.
    pub fn take_items(&mut self, mark: usize, values: &mut Vec<Value>) {
        let start = mark.min(self.items.len());
        values.extend(self.items.drain(start..).map(|at| Value::from(NodeRef(at))));
    }

    /// Drops the items of the list that started at `mark`, whose completion failed.
    pub fn drop_items(&mut self, mark: usize) {
        self.items.truncate(mark);
    }

    /// Returns the list of the items added since `mark`, its items placed side by side.
    pub fn list(&mut self, mark: usize) -> NodeRef {
        let first = handle(self.entries.len());
        let start = mark.min(self.items.len());
        let len = handle(self.items.len() - start);
        let Self { entries, items, .. } = self;
        entries.extend(items.drain(start..).map(|value| Entry {
            key: 0,
            key_len: 0,
            value,
            next: NONE,
        }));
        NodeRef(self.push(Node::List { first, len }))
    }

    /// Returns the answer of the settled execution whose data is the node `root`.
    ///
    /// Each leaf that is a value of the executor becomes the value of the
    /// JSON model `JSON.stringify` sees, by the rules of [`Value::to_js`], at
    /// the moment a result takes the arena, and each value of the JSON model
    /// moves over as it is.
    #[must_use]
    pub fn into_answer(mut self, root: NodeRef) -> Answer {
        Answer {
            leaves: mem::take(&mut self.leaves)
                .into_iter()
                .map(Held::into_js)
                .collect(),
            nodes: mem::take(&mut self.nodes),
            entries: mem::take(&mut self.entries),
            bytes: mem::take(&mut self.bytes),
            root: root.0,
        }
    }

    /// Returns the data whose node is `root` as a tree of executor values.
    ///
    /// An object is a [`PlainObject`] of its members in the order they were
    /// appended, a list is a list, a leaf is the value the executor
    /// completed, and a value held as its JSON text is the value
    /// [`Value::from_js_owned`] makes of the value the text reads back as, so
    /// a reader walks the data as values of the executor and may change them.
    #[must_use]
    pub fn to_value(&self, root: NodeRef) -> Value {
        self.value_of(self.shape(), root.0)
    }

    /// Returns the executor value of the node `at`.
    fn value_of(&self, shape: Shape<'_>, at: u32) -> Value {
        match shape.node(at) {
            Node::Undefined => Value::Undefined,
            Node::Null => Value::Null,
            Node::Bool(boolean) => Value::Bool(boolean),
            Node::Number(number) => Value::Number(number),
            Node::Str { start, len } => Value::String(string_of(shape.text(start, len))),
            Node::Object { head, len, .. } => {
                let mut entries = Vec::with_capacity(index(len));
                for (key, value) in shape.members(head) {
                    entries.push((String::from(key_text(key)), self.value_of(shape, value)));
                }
                Value::from(PlainObject::from_entries(entries))
            }
            Node::List { first, len } => Value::List(Rc::new(
                shape
                    .items(first, len)
                    .iter()
                    .map(|item| self.value_of(shape, item.value))
                    .collect(),
            )),
            Node::Json { start, len } => Value::from_js_owned(json_of(shape.text(start, len))),
            Node::Leaf(at) => self
                .leaves
                .get(index(at))
                .map_or(Value::Undefined, Held::to_value),
        }
    }
}

/// The answer of one settled execution, which a result holds and writes.
///
/// It holds the nodes, the entries and the bytes of the arena of the
/// execution with each leaf a value of the JSON model, and the node of the
/// data. An answer that drops leaves its vectors to its thread, as an arena
/// does.
pub struct Answer {
    /// The nodes.
    nodes: Vec<Node>,
    /// The entries of the objects and the items of the lists.
    entries: Vec<Entry>,
    /// The escaped strings, the encoded keys and the JSON texts of values.
    bytes: Vec<u8>,
    /// The leaves the writer of `aosabi-json` writes.
    leaves: Vec<JsValue>,
    /// The node of the data.
    root: u32,
}

impl Drop for Answer {
    fn drop(&mut self) {
        leave(Spare {
            nodes: mem::take(&mut self.nodes),
            entries: mem::take(&mut self.entries),
            items: Vec::new(),
            bytes: mem::take(&mut self.bytes),
        });
    }
}

impl Answer {
    /// Returns the nodes, the entries and the bytes of the answer.
    fn shape(&self) -> Shape<'_> {
        Shape {
            nodes: &self.nodes,
            entries: &self.entries,
            bytes: &self.bytes,
        }
    }

    /// Returns whether the data is `null`.
    ///
    /// It reads the kind of the node of the data and builds no tree.
    #[must_use]
    pub fn is_null(&self) -> bool {
        match self.shape().node(self.root) {
            Node::Null => true,
            Node::Leaf(at) => matches!(self.leaves.get(index(at)), Some(JsValue::Null)),
            _ => false,
        }
    }

    /// Returns the bytes the JSON of the data is about to take, which a writer reserves at once.
    ///
    /// The bytes of the arena hold every string and every key the answer
    /// writes, and the punctuation, the literals and the numbers take a few
    /// bytes per node.
    #[must_use]
    pub fn size_hint(&self) -> usize {
        self.bytes
            .len()
            .saturating_add(self.nodes.len().saturating_mul(BYTES_PER_NODE))
    }

    /// Appends the JSON of the data to `out`, the bytes `JSON.stringify` writes of its tree.
    ///
    /// It answers `false` and appends nothing when the data is `undefined`,
    /// which `JSON.stringify` leaves out of the object that holds it. A
    /// string, a key and the JSON text of a value are copied as the arena
    /// holds them, a number is written by `Number::toString`, and a leaf goes
    /// through the writer of `aosabi-json`, which, as it did for the JSON
    /// text, writes the keys of a `JSON` value in the order of
    /// `OrdinaryOwnPropertyKeys`.
    pub fn write(&self, out: &mut Vec<u8>) -> bool {
        if self.is_absent(self.root) {
            return false;
        }
        self.write_node(out, self.root);
        true
    }

    /// Returns whether `JSON.stringify` leaves the node `at` out of an object, `undefined` or a leaf that is.
    fn is_absent(&self, at: u32) -> bool {
        match self.shape().node(at) {
            Node::Undefined => true,
            Node::Leaf(at) => self.leaves.get(index(at)).is_none_or(JsValue::is_undefined),
            _ => false,
        }
    }

    /// Appends the JSON of the node `at`.
    fn write_node(&self, out: &mut Vec<u8>, at: u32) {
        let shape = self.shape();
        match shape.node(at) {
            Node::Undefined => {}
            Node::Null => out.extend_from_slice(b"null"),
            Node::Bool(true) => out.extend_from_slice(b"true"),
            Node::Bool(false) => out.extend_from_slice(b"false"),
            Node::Number(number) => number::write_f64(out, number),
            Node::Str { start, len } | Node::Json { start, len } => {
                out.extend_from_slice(shape.text(start, len));
            }
            Node::Object { head, .. } => self.write_object(out, head),
            Node::List { first, len } => self.write_list(out, first, len),
            Node::Leaf(at) => {
                if let Some(leaf) = self.leaves.get(index(at)) {
                    writer::write(out, leaf);
                }
            }
        }
    }

    /// Appends the object whose first entry is `head`, a member that is absent left out.
    fn write_object(&self, out: &mut Vec<u8>, head: u32) {
        out.push(b'{');
        let mut first = true;
        for (key, value) in self.shape().members(head) {
            if self.is_absent(value) {
                continue;
            }
            if !first {
                out.push(b',');
            }
            first = false;
            out.extend_from_slice(key);
            self.write_node(out, value);
        }
        out.push(b'}');
    }

    /// Appends the list of the `len` items from `first` on, an absent item written as `null`.
    fn write_list(&self, out: &mut Vec<u8>, first: u32, len: u32) {
        out.push(b'[');
        for (position, item) in self.shape().items(first, len).iter().enumerate() {
            if position > 0 {
                out.push(b',');
            }
            if self.is_absent(item.value) {
                out.extend_from_slice(b"null");
            } else {
                self.write_node(out, item.value);
            }
        }
        out.push(b']');
    }

    /// Returns the data as the value of the JSON model `JSON.stringify` sees.
    ///
    /// The tree follows the rules of [`Value::to_js`]: an object holds its
    /// members in the order they were appended with every `undefined` member
    /// left out, a list holds every item, a leaf is its value, and a value
    /// held as its JSON text is the value the text reads back as.
    #[must_use]
    pub fn to_js(&self) -> JsValue {
        self.js_of(self.shape(), self.root)
    }

    /// Returns the value of the JSON model of the node `at`.
    fn js_of(&self, shape: Shape<'_>, at: u32) -> JsValue {
        match shape.node(at) {
            Node::Undefined => JsValue::Undefined,
            Node::Null => JsValue::Null,
            Node::Bool(boolean) => JsValue::Bool(boolean),
            Node::Number(number) => JsValue::Number(number),
            Node::Str { start, len } => JsValue::String(string_of(shape.text(start, len))),
            Node::Object { head, len, .. } => {
                let mut object = JsObject::with_capacity(index(len));
                for (key, value) in shape.members(head) {
                    let value = self.js_of(shape, value);
                    if !value.is_undefined() {
                        object.insert(key_text(key), value);
                    }
                }
                JsValue::Object(object)
            }
            Node::List { first, len } => JsValue::Array(
                shape
                    .items(first, len)
                    .iter()
                    .map(|item| self.js_of(shape, item.value))
                    .collect(),
            ),
            Node::Json { start, len } => json_of(shape.text(start, len)),
            Node::Leaf(at) => self.leaves.get(index(at)).cloned().unwrap_or_default(),
        }
    }

    /// Returns the data as a tree of executor values, the value [`Value::from_js`] makes of [`Answer::to_js`].
    #[must_use]
    pub fn to_value(&self) -> Value {
        Value::from_js(&self.to_js())
    }
}

impl fmt::Debug for Answer {
    /// Writes the counts of the nodes, the entries, the bytes and the leaves.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Answer")
            .field("nodes", &self.nodes.len())
            .field("entries", &self.entries.len())
            .field("bytes", &self.bytes.len())
            .field("leaves", &self.leaves.len())
            .finish_non_exhaustive()
    }
}
