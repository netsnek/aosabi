// SPDX-License-Identifier: MIT
/*
 * aosabi-execute value object: an object with properties as the executor reads it.
 *
 * No analogue in the reference. The three operations are the ECMA-262 abstract
 * operations Get, https://tc39.es/ecma262/#sec-get-o-p, Set,
 * https://tc39.es/ecma262/#sec-set-o-p-v-throw, and the key list of
 * Object.keys, https://tc39.es/ecma262/#sec-object.keys, in the order of
 * OrdinaryOwnPropertyKeys,
 * https://tc39.es/ecma262/#sec-ordinaryownpropertykeys.
 */

//! The object value.
//!
//! The executor reads a property of a source object by name, the resolver
//! wrapping of `aosabi-core` lists the keys of a root object and projects an
//! object onto the fields a document selects, and the arguments of a field
//! are an object the executor fills key by key. [`ObjectLike`] is those
//! operations, which a Rust type answers for its fields and methods, and
//! [`PlainObject`] is the object the executor makes. The result of a
//! selection set is an object of the arena of the execution (see
//! [`crate::arena`]), which a reader of the data of a result meets as a
//! [`PlainObject`] of its members.
//!
//! A projection reads a member as the projection of `wrapResolver`
//! reads it, up to three times, through [`read_member`], with the [`Read`]
//! that says what the reader takes of the value, and names the members it
//! reads through [`select_members`] before its first read. With the feature
//! `host-reads`, which the builds of the TypeScript option take, an object of
//! a host answers each read through the provided method `ObjectLike::read`,
//! whose getter may throw, and may read the members it learns through the
//! provided method `ObjectLike::select` in one call, and a read that throws
//! answers the value it threw, a [`Thrown`]. Without the feature every object
//! is of a Rust type, a read is [`ObjectLike::get`], which answers the same
//! value at every read and throws nothing, and a [`Thrown`] has no value, so
//! a build of the Rust option holds neither the two methods nor a path for a
//! throw.
//!
//! The same feature gives [`ObjectLike`] the provided methods `own_keys_js`
//! and `get_js`, the keys of an object and the member of a key as JavaScript
//! strings, which the conversion of a value to the JSON model reads, so a key
//! of an object of a host that holds a lone surrogate reaches the JSON of a
//! value as `JSON.stringify` writes it. A key of the model of the Rust option
//! is a string of Rust, which holds no lone surrogate.
//!
//! An object of a Rust type with a static shape answers its [`Shape`], its
//! data members in the order of their slots and the names of its methods, and
//! writes the data members a selection reads through
//! [`ObjectLike::write_members`] by their slots, so the executor reads them
//! with no lookup by name and no copy of a member.

use alloc::{
    rc::Rc,
    string::String,
    vec::Vec, //
};
use core::cell::RefCell;

use aosabi_json::{
    map::IndexMap,
    value::{
        KeyHasherBuilder,
        array_index, //
    }, //
};

#[cfg(feature = "host-reads")]
use aosabi_json::JsString;

#[cfg(feature = "host-reads")]
use crate::value::Completion;
use crate::{
    arena::Answer,
    execute::MemberSink,
    value::{
        Function,
        Value, //
    }, //
};

/// What a reader takes of a property it reads.
///
/// An object of a host hands over no more of a member than the reader takes,
/// so a test runs no code of the value it tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Read {
    /// The reader tests the value for `undefined`, for its truth or for the
    /// type `function`, and keeps nothing else of it.
    Test,
    /// The reader takes the value itself.
    Value,
}

/// A member a projection reads, as the projection names it before its reads.
///
/// The projection reads each member it selects once for a test of its truth,
/// once more for a test of its type when that answer is true, and once more
/// for its value, but where it routes the member and the member is no
/// function, which a later call of the field reads again.
#[derive(Clone, Copy, Debug)]
pub struct Selected<'a> {
    /// The name of the member.
    pub key: &'a str,
    /// Whether the projection reads no value of the member when the member is
    /// no function.
    pub routed: bool,
    /// Whether the projection may read members of the value of the member
    /// before it reads the next member.
    pub nested: bool,
}

/// What a read of an object throws, the value an object of a host threw.
///
/// The feature `host-reads` holds objects of a host, whose getters may throw.
#[cfg(feature = "host-reads")]
pub type Thrown = Value;

/// What a read of an object throws, which is nothing.
///
/// Without the feature `host-reads` every object is of a Rust type and reads
/// with no throw, so a caller of [`read_member`] holds no path for a throw.
#[cfg(not(feature = "host-reads"))]
pub type Thrown = core::convert::Infallible;

/// The position of a data member in the [`Shape`] of its object, by which
/// the member writer of the object reads it.
pub type MemberSlot = u16;

/// The slot that names no data member, a field the executor reads as it
/// reads a field of any object.
pub const NO_MEMBER: MemberSlot = MemberSlot::MAX;

/// What a data member of a [`Shape`] can hold, or a list of what it can hold.
///
/// The resolver wrapping of `aosabi-core` reads it to tell whether the
/// projection of an object would hand the executor anything but the members
/// the object holds.
#[derive(Clone, Copy, Debug)]
pub enum MemberKind {
    /// A string, a number, a boolean, `null` or a date, and no object, no
    /// function and no promise.
    Leaf,
    /// A value of the JSON model, whose objects hold plain data and no
    /// function and no promise.
    Json,
    /// An object of a type whose shape the function answers, `None` for a
    /// type with no static shape.
    Object(fn() -> Option<&'static Shape>),
    /// Any value of the executor, a function and a promise among them.
    Dynamic,
}

/// One data member of a [`Shape`], its name and what it can hold.
#[derive(Clone, Copy, Debug)]
pub struct MemberShape {
    /// The name of the member, the key the object answers it under.
    name: &'static str,
    /// What the member can hold.
    kind: MemberKind,
}

impl MemberShape {
    /// Returns the member `name`, which holds what `kind` names.
    #[must_use]
    pub const fn new(name: &'static str, kind: MemberKind) -> Self {
        Self { name, kind }
    }

    /// Returns the name of the member.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Returns what the member can hold.
    #[must_use]
    pub const fn kind(&self) -> MemberKind {
        self.kind
    }
}

/// The static shape of an object: its data members in the order of their
/// slots and the names of its methods.
///
/// A data member is a property whose value the object holds, which a read
/// returns with no side effect. A method is a property whose value is a
/// function bound to the object. One constant holds the shape of every
/// object of one type, and the executor keeps what it reads of a shape by
/// the address of that constant.
#[derive(Debug)]
pub struct Shape {
    /// The data members, the slot of each its position.
    members: &'static [MemberShape],
    /// The names of the methods.
    methods: &'static [&'static str],
}

impl Shape {
    /// Returns the shape of the data members `members`, in the order of
    /// their slots, and of the methods `methods`.
    #[must_use]
    pub const fn new(members: &'static [MemberShape], methods: &'static [&'static str]) -> Self {
        Self { members, methods }
    }

    /// Returns the data members in the order of their slots.
    #[must_use]
    pub const fn members(&self) -> &'static [MemberShape] {
        self.members
    }

    /// Returns the names of the methods.
    #[must_use]
    pub const fn methods(&self) -> &'static [&'static str] {
        self.methods
    }

    /// Returns the slot of the data member `name`, `None` for a name that is
    /// no data member and for a position no [`MemberSlot`] other than
    /// [`NO_MEMBER`] holds.
    #[must_use]
    pub fn slot(&self, name: &str) -> Option<MemberSlot> {
        self.members
            .iter()
            .position(|member| member.name == name)
            .and_then(|at| MemberSlot::try_from(at).ok())
            .filter(|slot| *slot != NO_MEMBER)
    }

    /// Returns the data member `name`.
    #[must_use]
    pub fn member(&self, name: &str) -> Option<&'static MemberShape> {
        self.members.iter().find(|member| member.name == name)
    }

    /// Answers whether `name` is a method.
    #[must_use]
    pub fn has_method(&self, name: &str) -> bool {
        self.methods.contains(&name)
    }
}

/// An object the executor reads by property name.
pub trait ObjectLike {
    /// Returns `object[key]`, which is `Value::Undefined` for a missing key.
    ///
    /// The receiver is the `Rc`, because a method the object answers is a
    /// bound function that keeps the object alive.
    fn get(self: Rc<Self>, key: &str) -> Value;

    /// Returns the completion of a read of `object[key]`, a normal completion
    /// of the value or a throw completion of what the read threw.
    ///
    /// The [`Read`] says what the reader takes of the value. For a
    /// [`Read::Test`] the value of the completion may stand in for the member:
    /// it is `undefined` exactly when the member is, a function exactly when
    /// the member is one and true exactly when the member is, so an object of
    /// a host answers the test with no code of the member run. The default
    /// answers the normal completion of [`ObjectLike::get`] for either, since
    /// a Rust type reads a member with no throw and the same value each time.
    #[cfg(feature = "host-reads")]
    fn read(self: Rc<Self>, key: &str, _read: Read) -> Completion {
        Completion::Normal(self.get(key))
    }

    /// Names the members a projection of the object reads, in the order of
    /// its reads, before its first read.
    ///
    /// An object of a host may make the reads of several members in one
    /// call and answer [`ObjectLike::read`] from them in their order. The
    /// default reads nothing ahead.
    #[cfg(feature = "host-reads")]
    fn select(&self, _members: &mut dyn Iterator<Item = Selected<'_>>) {}

    /// Returns `Object.keys(object)`.
    ///
    /// A Rust type lists its fields first and its methods after them, each
    /// in declaration order, which is the order a class instance of the
    /// reference lists them in when its constructor assigns the fields.
    fn own_keys(&self) -> Vec<String>;

    /// Returns `Object.keys(object)` as JavaScript strings, which keep a key
    /// that holds a lone surrogate.
    ///
    /// An object of a host may hold such a key, which [`ObjectLike::own_keys`]
    /// cannot answer, and the conversion of a value to the JSON model reads
    /// the keys through this method. The default answers the keys of
    /// [`ObjectLike::own_keys`].
    #[cfg(feature = "host-reads")]
    fn own_keys_js(&self) -> Vec<JsString> {
        self.own_keys().into_iter().map(JsString::from).collect()
    }

    /// Returns `object[key]` for a key that [`ObjectLike::own_keys_js`]
    /// answered.
    ///
    /// The default answers [`ObjectLike::get`] of a key that is well formed
    /// UTF-16, and `undefined` for any other, which no key of a Rust type is.
    #[cfg(feature = "host-reads")]
    fn get_js(self: Rc<Self>, key: &JsString) -> Value {
        key.as_str().map_or(Value::Undefined, |key| self.get(key))
    }

    /// Returns the names the projection of a canary build holds of
    /// the object, its own property names and those of its chain of
    /// prototypes up to `Object.prototype`, but `constructor`.
    ///
    /// The type resolver of that build tests a value with `in` alone over that
    /// projection. A Rust type lists its fields and its methods in
    /// [`ObjectLike::own_keys`], as the fields and the prototype of a class
    /// instance list them, so the default answers those.
    #[cfg(feature = "host-reads")]
    fn property_names(&self) -> Vec<String> {
        self.own_keys()
    }

    /// Walks the keys of `Object.keys(object)` in the order
    /// [`ObjectLike::own_keys`] lists them, with no list where the object
    /// needs none.
    ///
    /// The default walks the list [`ObjectLike::own_keys`] answers. A
    /// [`PlainObject`] walks its keys where they stand, in the order of
    /// `OrdinaryOwnPropertyKeys`, as `JsObject::own_keys` of `aosabi-json`
    /// walks an object of the JSON model, and holds its properties for the
    /// length of the walk, so `walk` reads no property of the object and
    /// writes none. A caller that keeps the keys or reads the value of each
    /// key takes the list [`ObjectLike::own_keys`] answers instead, whose keys
    /// it owns with no copy, and reads each value after the list is made.
    fn walk_own_keys(&self, walk: &mut dyn FnMut(&str)) {
        walk_list(self.own_keys(), walk);
    }

    /// Returns whether `key` is an own property, `Object.prototype.hasOwnProperty.call(object, key)`.
    ///
    /// The default scans [`ObjectLike::own_keys`]. A type that keeps its
    /// properties in a map answers from the map, so a caller that asks once
    /// per property of a wide object pays no scan per question.
    fn has_own(&self, key: &str) -> bool {
        self.own_keys().iter().any(|known| known == key)
    }

    /// Performs `object[key] = value`.
    ///
    /// The default ignores the write, as a frozen object does outside strict
    /// mode.
    fn set(&self, _key: &str, _value: Value) {}

    /// Returns the name of the constructor, which the value printers write
    /// for an object below their depth limit.
    ///
    /// The default is `Object`, the tag of an object literal and of an object
    /// without a prototype. A type that stands for a class instance of the
    /// reference answers the class name.
    fn constructor_name(&self) -> String {
        String::from("Object")
    }

    /// Returns `object[Symbol.asyncIterator]`, or `None` when the object has no such method.
    ///
    /// The key `Symbol.asyncIterator` is a symbol and no string, so
    /// [`ObjectLike::get`] cannot answer it. The default is `None`, the plain
    /// object. A type that stands for an async iterable answers the method,
    /// whose call returns the async iterator, and the module
    /// [󰀃](crate::value::iter) reads it.
    fn async_iterator(self: Rc<Self>) -> Option<Function> {
        None
    }

    /// Returns whether a value [`ObjectLike::get`] answers stays valid after the request that read it.
    ///
    /// The default is `true`, a Rust object, whose values hold what they
    /// name. An object of a host that answers each value as a handle of the
    /// request that reads it, which the host drops with that request,
    /// answers `false`, so a reader that keeps a value beyond its request,
    /// the table of root members of `aosabi-core` among them, reads it again
    /// in each request.
    fn values_outlive_request(&self) -> bool {
        true
    }

    /// Takes the answer of an execution out of the result object that holds it.
    ///
    /// The default is `None`, an object that holds no answer. The result
    /// object of an execution answers its arena once, as the settled
    /// [`Answer`], and `None` after, and `None` once a reader read its data
    /// as a tree, so [`ExecutionResult::from_value`] writes the data the
    /// arena holds and reads every other object as the tree it is.
    ///
    /// [`ExecutionResult::from_value`]: crate::result::ExecutionResult::from_value
    fn take_answer(&self) -> Option<Answer> {
        None
    }

    /// Returns the static shape of the object, its data members by slot and
    /// the names of its methods.
    ///
    /// The default is `None`, an object whose members no type fixes, which
    /// the executor reads by name. A type that answers a shape writes its data
    /// members by slot through [`ObjectLike::write_members`], and a read of
    /// a data member through [`ObjectLike::get`] answers the value that
    /// writer writes for it.
    fn shape(&self) -> Option<&'static Shape> {
        None
    }

    /// Writes the data members `slots` names into `sink`, one call of the
    /// sink per slot in the order of `slots`.
    ///
    /// `slots` holds the slot of each field of a selection in its collected
    /// order, [`NO_MEMBER`] for a field that reads no data member of the
    /// [`ObjectLike::shape`] of the object. The writer hands a string member
    /// to [`MemberSink::str`], any other data member to [`MemberSink::value`]
    /// as [`ObjectLike::get`] answers it, and calls [`MemberSink::field`] for
    /// a field with no data member. The executor reads every field the writer
    /// leaves before it returns as it reads the fields of any object. The
    /// default writes nothing, so the executor reads every field by name.
    fn write_members(self: Rc<Self>, slots: &[MemberSlot], sink: &mut MemberSink<'_>) {
        let _ = (slots, sink);
    }
}

/// Returns `object[key]` as a projection reads it for `kind`, or what the
/// read threw.
///
/// The read is [`ObjectLike::read`], which an object of a host answers with
/// a getter that may throw.
///
/// # Errors
///
/// Returns what the read threw.
#[cfg(feature = "host-reads")]
pub fn read_member(object: &Rc<dyn ObjectLike>, key: &str, kind: Read) -> Result<Value, Thrown> {
    Rc::clone(object).read(key, kind).into_result()
}

/// Returns `object[key]` as a projection reads it for `kind`.
///
/// Without the feature `host-reads` the read is [`ObjectLike::get`] for
/// either kind, which a Rust type answers with the same value at every read
/// and no throw.
///
/// # Errors
///
/// None, a [`Thrown`] has no value.
#[cfg(not(feature = "host-reads"))]
pub fn read_member(object: &Rc<dyn ObjectLike>, key: &str, _kind: Read) -> Result<Value, Thrown> {
    Ok(Rc::clone(object).get(key))
}

/// Whether a read of a member may run a getter of a host.
///
/// A getter of a host may answer another value at each read, or throw, so a
/// reader that tests a member and then takes its value reads it each time,
/// as the reference reads it. Only a build with the feature `host-reads`
/// holds an object of a host. In every other build [`read_member`] is
/// [`ObjectLike::get`], which a Rust type answers with the same value at
/// every read and no throw, so a reader takes one read of a member for its
/// tests and its value and answers the same bytes.
pub const HOST_GETTERS: bool = cfg!(feature = "host-reads");

/// Names `members`, the members a projection of `object` reads in the order
/// of its reads, to the object before the first read, through
/// [`ObjectLike::select`].
#[cfg(feature = "host-reads")]
pub fn select_members<'a>(
    object: &dyn ObjectLike,
    mut members: impl Iterator<Item = Selected<'a>>,
) {
    object.select(&mut members);
}

/// Drops the members a projection of an object reads, since without the
/// feature `host-reads` no object reads a member ahead.
#[cfg(not(feature = "host-reads"))]
pub fn select_members<'a>(_object: &dyn ObjectLike, _members: impl Iterator<Item = Selected<'a>>) {}

/// Walks the keys `keys`, the list [`ObjectLike::own_keys`] answers, for an
/// object that walks no keys of its own.
///
/// One body serves every type, so the provided walk of a type is a call of
/// its list and of this function.
// Kept out of line, since the provided walk of every type of an app would
// otherwise hold a copy of the loop.
#[inline(never)]
fn walk_list(keys: Vec<String>, walk: &mut dyn FnMut(&str)) {
    for key in keys {
        walk(&key);
    }
}

/// The properties of a [`PlainObject`] in insertion order.
type Properties = IndexMap<String, Value, KeyHasherBuilder>;

/// An object with no prototype and with insertion order, `Object.create(null)`.
///
/// This is the object of the arguments of a field, the object a JSON shaped
/// value becomes and the object of the data of a result read as a tree.
/// Writing a key that exists keeps its position, which is what assigning an
/// existing property does.
pub struct PlainObject(RefCell<Properties>);

impl PlainObject {
    /// Creates an empty object.
    #[must_use]
    pub fn new() -> Rc<Self> {
        Rc::new(Self(RefCell::new(Properties::default())))
    }

    /// Creates an object from `entries` in their order.
    ///
    /// A key that appears twice keeps its first position and its last value.
    #[must_use]
    pub fn from_entries(entries: Vec<(String, Value)>) -> Rc<Self> {
        Rc::new(Self(RefCell::new(entries.into_iter().collect())))
    }

    /// Returns whether the object has the own property `key`.
    ///
    /// A property that holds `Value::Undefined` is present, which
    /// [`ObjectLike::get`] cannot tell from a missing one.
    #[must_use]
    pub fn contains_key(&self, key: &str) -> bool {
        self.0.borrow().contains_key(key)
    }

    /// Returns the number of own properties.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.borrow().len()
    }

    /// Returns whether the object has no own property.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.borrow().is_empty()
    }
}

impl ObjectLike for PlainObject {
    fn get(self: Rc<Self>, key: &str) -> Value {
        self.0
            .borrow()
            .get(key)
            .cloned()
            .unwrap_or(Value::Undefined)
    }

    /// Returns the keys in the order of `OrdinaryOwnPropertyKeys`, the order
    /// [`PlainObject::walk_own_keys`] walks them in.
    fn own_keys(&self) -> Vec<String> {
        let mut keys = Vec::with_capacity(self.len());
        self.walk_own_keys(&mut |key| keys.push(String::from(key)));
        keys
    }

    /// Walks the keys in the order of `OrdinaryOwnPropertyKeys`.
    ///
    /// The keys that are array indices come first in ascending numeric order,
    /// and the other keys follow in insertion order. An object with no key
    /// that is an array index is walked in insertion order where its keys
    /// stand, and only an object with such a key lists their positions to sort
    /// them.
    fn walk_own_keys(&self, walk: &mut dyn FnMut(&str)) {
        let properties = self.0.borrow();
        if properties
            .keys()
            .all(|key| array_index(key.as_bytes()).is_none())
        {
            for key in properties.keys() {
                walk(key);
            }
            return;
        }
        let mut indexed: Vec<(u32, usize)> = properties
            .keys()
            .enumerate()
            .filter_map(|(position, key)| {
                array_index(key.as_bytes()).map(|index| (index, position))
            })
            .collect();
        indexed.sort_unstable();
        for (_, position) in indexed {
            if let Some((key, _)) = properties.get_index(position) {
                walk(key);
            }
        }
        for key in properties
            .keys()
            .filter(|key| array_index(key.as_bytes()).is_none())
        {
            walk(key);
        }
    }

    fn has_own(&self, key: &str) -> bool {
        self.contains_key(key)
    }

    fn set(&self, key: &str, value: Value) {
        self.0.borrow_mut().insert(String::from(key), value);
    }
}
