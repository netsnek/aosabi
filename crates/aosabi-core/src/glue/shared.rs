// SPDX-License-Identifier: MIT
/*
 * aosabi-core glue shared: an object of the executor that is a part of
 * another, and a field read through the object that holds it.
 *
 * No analogue on the reference side, where a property holds a reference to
 * an object and reading it hands out that reference, never a copy.
 */

//! [`Shared`], [`SharedObject`] and [`FieldValue`], the reading of a public
//! field through a shared reference.
//!
//! A described struct is an object of the executor behind a shared pointer,
//! and the glue reads each public field of it through a shared reference and
//! converts it, so neither the struct nor the type of a field needs `Clone`. A
//! field of plain data converts into a value of its own, as a JavaScript
//! property holds a primitive. A field that is itself a described struct,
//! directly or inside an `Option`, a list, a box, a shared pointer or a
//! variant of a payload enum, becomes an object of the executor that reaches
//! the struct through the object that holds it, as a property holds a
//! reference to an object and never a copy. That object keeps the object that
//! holds it alive, and a method of it binds to it.
//!
//! | Item | Is |
//! | --- | --- |
//! | [`Shared`] | a struct the executor reaches, the whole object behind its pointer or a part of another |
//! | [`SharedObject`] | the properties of a struct read through a [`Shared`], which the derive implements |
//! | [`FieldValue`] | a field converted through a shared reference |
//! | [`Reached`] | the return of a method whose object is reached at the call |
//!
//! A part reaches its struct again at each read, so reaching can fail only
//! where the path runs through an `Option`, a list or a variant that holds
//! no such value. The object behind a shared pointer is never written, so
//! that does not happen. Where it would, a read answers `undefined` and a
//! method throws the `TypeError` of a property read on `undefined`, and
//! nothing panics.
//!
//! A described struct with no embedded base, no spread and no kept remote
//! object has a static shape, its public fields as data members in declaration
//! order and the methods of its resolvers block, which the derive and the glue
//! of a plain crate answer through [`SharedObject::struct_shape`], the glue of
//! a plain crate naming what a member can hold through [`member_of`], since it
//! names no type of a field. Its member writer,
//! [`SharedObject::write_shared`], hands each data member a selection reads to
//! the executor by its slot through [`FieldValue::write_member`], which hands
//! a string field over as the `&str` it holds, so the executor escapes it into
//! the answer with no copy of the `String`, and a `JSON` or a `JSONObject`
//! field as the value of the JSON model it holds, which the executor writes
//! into the answer from where it stands with no value of the executor made of
//! it.

use core::convert::Infallible;
use std::{
    borrow::Cow,
    collections::VecDeque,
    rc::Rc,
    sync::Arc, //
};

use aosabi_execute::{
    execute::MemberSink,
    value::{
        Completion,
        ErrorObject,
        MemberKind,
        MemberSlot,
        ObjectLike,
        Shape,
        Value, //
    }, //
};
use aosabi_json::{
    JsObject,
    JsValue, //
};
use aosabi_scalars::JsDate;

use crate::{
    descriptor::{
        Any,
        Date,
        File,
        Float,
        ID,
        Int,
        Number,
        UserError, //
    },
    glue::{
        completion::IntoCompletion,
        into_value::IntoValue, //
    },
};

/// The step from one struct to a part of it.
trait Part<T> {
    /// Returns the part, `None` where the path holds no such part.
    fn reach(&self) -> Option<&T>;
}

/// A part of the struct `object` that `project` reads.
// A pointer to the struct beside the handle of its root would skip the walk.
struct Projection<P: 'static, F> {
    /// The struct that holds the part.
    object: Shared<P>,
    /// The read of the part.
    project: F,
}

impl<P, T, F> Part<T> for Projection<P, F>
where
    F: Fn(&P) -> Option<&T>,
{
    fn reach(&self) -> Option<&T> {
        self.object
            .reach()
            .and_then(|object| (self.project)(object))
    }
}

/// How a [`Shared`] holds its struct.
enum Holder<T: 'static> {
    /// The whole object behind its pointer.
    Whole(Rc<T>),
    /// A part of another struct.
    Part(Rc<dyn Part<T>>),
}

/// A struct the executor reaches, the whole object behind its shared pointer
/// or a part of another struct that keeps that struct alive.
pub struct Shared<T: 'static>(Holder<T>);

impl<T: 'static> Shared<T> {
    /// Returns the struct, `None` where a part does not reach it.
    #[must_use]
    pub fn reach(&self) -> Option<&T> {
        match &self.0 {
            Holder::Whole(object) => Some(object),
            Holder::Part(part) => part.reach(),
        }
    }

    /// Returns the part of this struct that `project` reads.
    ///
    /// The part keeps this struct alive and reads it again at each reach.
    #[must_use]
    pub fn part<U, F>(&self, project: F) -> Shared<U>
    where
        U: 'static,
        F: Fn(&T) -> Option<&U> + 'static,
    {
        Shared(Holder::Part(Rc::new(Projection {
            object: self.clone(),
            project,
        })))
    }
}

impl<T: 'static> Clone for Shared<T> {
    fn clone(&self) -> Self {
        Self(match &self.0 {
            Holder::Whole(object) => Holder::Whole(Rc::clone(object)),
            Holder::Part(part) => Holder::Part(Rc::clone(part)),
        })
    }
}

impl<T: 'static> From<Rc<T>> for Shared<T> {
    fn from(object: Rc<T>) -> Self {
        Self(Holder::Whole(object))
    }
}

/// The properties of a described struct read through a [`Shared`].
///
/// The derive implements it beside `ObjectLike`, whose `get` reads the whole
/// object through it, so a part answers the same properties as the whole.
pub trait SharedObject: ObjectLike + Sized + 'static {
    /// Returns `object[key]` of the struct `this` reaches.
    fn get_shared(this: &Shared<Self>, key: &str) -> Value;

    /// Returns the static shape of the struct, `None` for a struct whose
    /// properties no static shape holds.
    ///
    /// The default is `None`. The derive answers the shape of a struct with
    /// no embedded base, no spread and no kept remote object, its public
    /// fields as data members in declaration order and the methods of its
    /// resolvers block.
    #[must_use]
    fn struct_shape() -> Option<&'static Shape> {
        None
    }

    /// Writes the data members `slots` names of the struct `this` reaches
    /// into `sink`, as [`ObjectLike::write_members`] says.
    ///
    /// The default writes nothing, so the executor reads every field by
    /// name. The derive writes the field of each slot through
    /// [`FieldValue::write_member`] and calls [`MemberSink::field`] for a
    /// slot that names no data member, and a part that reaches no struct
    /// writes nothing.
    fn write_shared(this: &Shared<Self>, slots: &[MemberSlot], sink: &mut MemberSink<'_>) {
        let _ = (this, slots, sink);
    }
}

impl<T: SharedObject> ObjectLike for Shared<T> {
    fn get(self: Rc<Self>, key: &str) -> Value {
        T::get_shared(&self, key)
    }

    fn own_keys(&self) -> Vec<String> {
        self.reach().map_or_else(Vec::new, ObjectLike::own_keys)
    }

    fn set(&self, key: &str, value: Value) {
        if let Some(object) = self.reach() {
            object.set(key, value);
        }
    }

    fn constructor_name(&self) -> String {
        self.reach()
            .map_or_else(|| String::from("Object"), ObjectLike::constructor_name)
    }

    fn shape(&self) -> Option<&'static Shape> {
        T::struct_shape()
    }

    fn write_members(self: Rc<Self>, slots: &[MemberSlot], sink: &mut MemberSink<'_>) {
        T::write_shared(&self, slots, sink);
    }
}

/// A field that converts into a value of the executor through a shared
/// reference.
///
/// `part` returns the field as a part of the struct that holds it, which a
/// described struct becomes the object of, and which plain data never calls.
#[diagnostic::on_unimplemented(
    message = "`{Self}` has no value of the executor",
    note = "derive `aosabi::Aosabi` on the type, or mark a newtype with `#[aosabi::scalar]`"
)]
pub trait FieldValue: Sized + 'static {
    /// What a field of the type can hold, which the shape of a struct names
    /// for each of its data members.
    ///
    /// The default is [`MemberKind::Dynamic`], any value of the executor.
    const MEMBER: MemberKind = MemberKind::Dynamic;

    /// Returns the value of the field `self`.
    fn field_value(&self, part: &dyn Fn() -> Shared<Self>) -> Value;

    /// Writes the field `self` into `sink` as the member of the field the
    /// sink completes next.
    ///
    /// The default writes the value [`FieldValue::field_value`] answers, the
    /// value a read of the field through `get` answers. A string type writes
    /// the `&str` it holds, which the executor escapes into the answer with
    /// no copy before.
    fn write_member(&self, sink: &mut MemberSink<'_>, part: &dyn Fn() -> Shared<Self>) {
        sink.value(self.field_value(part));
    }
}

/// Implements [`FieldValue`] for plain data, whose value is its own and
/// holds what `$kind` names.
macro_rules! data_field_value {
    ($kind:expr; $($type:ty),+ $(,)?) => {
        $(
            impl FieldValue for $type {
                const MEMBER: MemberKind = $kind;

                fn field_value(&self, _: &dyn Fn() -> Shared<Self>) -> Value {
                    IntoValue::into_value(Clone::clone(self))
                }
            }
        )+
    };
}

/// Implements [`FieldValue`] for a string type, whose value is the string it
/// holds and whose member is written from the `&str` it holds.
macro_rules! text_field_value {
    ($($type:ty),+ $(,)?) => {
        $(
            impl FieldValue for $type {
                const MEMBER: MemberKind = MemberKind::Leaf;

                fn field_value(&self, _: &dyn Fn() -> Shared<Self>) -> Value {
                    IntoValue::into_value(Clone::clone(self))
                }

                fn write_member(&self, sink: &mut MemberSink<'_>, _: &dyn Fn() -> Shared<Self>) {
                    sink.str(self);
                }
            }
        )+
    };
}

data_field_value!(MemberKind::Dynamic; Value, File, Any, UserError);

data_field_value!(
    MemberKind::Leaf;
    (),
    bool,
    char,
    f64,
    f32,
    i8,
    i16,
    i32,
    i64,
    i128,
    isize,
    u8,
    u16,
    u32,
    u64,
    u128,
    usize,
    JsDate,
    Int,
    Float,
    Number,
    Date,
);

text_field_value!(String, &'static str, Box<str>, Cow<'static, str>);

impl FieldValue for ID {
    const MEMBER: MemberKind = MemberKind::Leaf;

    fn field_value(&self, _: &dyn Fn() -> Shared<Self>) -> Value {
        IntoValue::into_value(Clone::clone(self))
    }

    fn write_member(&self, sink: &mut MemberSink<'_>, _: &dyn Fn() -> Shared<Self>) {
        sink.str(&self.0);
    }
}

impl FieldValue for Infallible {
    const MEMBER: MemberKind = MemberKind::Leaf;

    fn field_value(&self, _: &dyn Fn() -> Shared<Self>) -> Value {
        match *self {}
    }
}

impl FieldValue for JsValue {
    const MEMBER: MemberKind = MemberKind::Json;

    fn field_value(&self, _: &dyn Fn() -> Shared<Self>) -> Value {
        Value::from_js(self)
    }

    /// Hands the value over as the value of the JSON model it is, which the
    /// executor writes into the answer from where it stands where a `JSON` or
    /// a `JSONObject` leaf takes it as itself.
    fn write_member(&self, sink: &mut MemberSink<'_>, _: &dyn Fn() -> Shared<Self>) {
        sink.json(self);
    }
}

impl FieldValue for JsObject {
    const MEMBER: MemberKind = MemberKind::Json;

    fn field_value(&self, _: &dyn Fn() -> Shared<Self>) -> Value {
        Value::from_js_object(self)
    }

    /// Hands the object over as the object of the JSON model it is, as the
    /// member writer of a [`JsValue`] hands its value over.
    fn write_member(&self, sink: &mut MemberSink<'_>, _: &dyn Fn() -> Shared<Self>) {
        sink.json_object(self);
    }
}

impl<T: FieldValue> FieldValue for Option<T> {
    const MEMBER: MemberKind = T::MEMBER;

    fn field_value(&self, part: &dyn Fn() -> Shared<Self>) -> Value {
        match self {
            None => Value::Null,
            Some(item) => item.field_value(&|| part().part(Option::as_ref)),
        }
    }

    fn write_member(&self, sink: &mut MemberSink<'_>, part: &dyn Fn() -> Shared<Self>) {
        match self {
            None => sink.value(Value::Null),
            Some(item) => item.write_member(sink, &|| part().part(Option::as_ref)),
        }
    }
}

/// Returns the list of the items `items` of a field `part` reaches, each
/// item read again by its index with `get`.
fn list_field_value<'a, L, T>(
    items: impl IntoIterator<Item = &'a T>,
    part: &dyn Fn() -> Shared<L>,
    get: fn(&L, usize) -> Option<&T>,
) -> Value
where
    L: 'static,
    T: FieldValue,
{
    Value::from(
        items
            .into_iter()
            .enumerate()
            .map(|(index, item)| {
                item.field_value(&|| part().part(move |list: &L| get(list, index)))
            })
            .collect::<Vec<_>>(),
    )
}

impl<T: FieldValue> FieldValue for Vec<T> {
    const MEMBER: MemberKind = T::MEMBER;

    fn field_value(&self, part: &dyn Fn() -> Shared<Self>) -> Value {
        list_field_value(self, part, |list, index| list.get(index))
    }
}

impl<T: FieldValue> FieldValue for Box<[T]> {
    const MEMBER: MemberKind = T::MEMBER;

    fn field_value(&self, part: &dyn Fn() -> Shared<Self>) -> Value {
        list_field_value(self.iter(), part, |list, index| list.get(index))
    }
}

impl<T: FieldValue> FieldValue for VecDeque<T> {
    const MEMBER: MemberKind = T::MEMBER;

    fn field_value(&self, part: &dyn Fn() -> Shared<Self>) -> Value {
        list_field_value(self, part, VecDeque::get)
    }
}

impl<T: FieldValue, const N: usize> FieldValue for [T; N] {
    const MEMBER: MemberKind = T::MEMBER;

    fn field_value(&self, part: &dyn Fn() -> Shared<Self>) -> Value {
        list_field_value(self, part, |list, index| list.get(index))
    }
}

/// A field that borrows a slice for the life of the program, such as a field
/// `&'a [T]` of a struct the glue of a plain crate reaches as its `'static`
/// form, is a list as an array is, and holds what its items hold.
impl<T: FieldValue> FieldValue for &'static [T] {
    const MEMBER: MemberKind = T::MEMBER;

    fn field_value(&self, part: &dyn Fn() -> Shared<Self>) -> Value {
        list_field_value(self.iter(), part, |list, index| list.get(index))
    }
}

impl<T: FieldValue> FieldValue for Box<T> {
    const MEMBER: MemberKind = T::MEMBER;

    fn field_value(&self, part: &dyn Fn() -> Shared<Self>) -> Value {
        (**self).field_value(&|| part().part(|boxed: &Self| Some(&**boxed)))
    }

    fn write_member(&self, sink: &mut MemberSink<'_>, part: &dyn Fn() -> Shared<Self>) {
        (**self).write_member(sink, &|| part().part(|boxed: &Self| Some(&**boxed)));
    }
}

impl<T: FieldValue> FieldValue for Rc<T> {
    const MEMBER: MemberKind = T::MEMBER;

    /// A struct behind the pointer is reached through the pointer alone,
    /// so its object does not keep the struct that holds the field alive.
    fn field_value(&self, _: &dyn Fn() -> Shared<Self>) -> Value {
        (**self).field_value(&|| Shared::from(Rc::clone(self)))
    }

    fn write_member(&self, sink: &mut MemberSink<'_>, _: &dyn Fn() -> Shared<Self>) {
        (**self).write_member(sink, &|| Shared::from(Rc::clone(self)));
    }
}

impl<T: FieldValue> FieldValue for Arc<T> {
    const MEMBER: MemberKind = T::MEMBER;

    fn field_value(&self, part: &dyn Fn() -> Shared<Self>) -> Value {
        (**self).field_value(&|| part().part(|shared: &Self| Some(&**shared)))
    }

    fn write_member(&self, sink: &mut MemberSink<'_>, part: &dyn Fn() -> Shared<Self>) {
        (**self).write_member(sink, &|| part().part(|shared: &Self| Some(&**shared)));
    }
}

/// Returns what the field `read` reads can hold, the [`FieldValue::MEMBER`] of
/// its type.
///
/// The glue of a plain crate names no type of a field, since the type as the
/// source writes it may name an item of another module, so the static shape of
/// a struct of such a crate takes what each data member can hold from the read
/// of its field, whose type the compiler infers, where the derive names the
/// type.
#[must_use]
pub const fn member_of<S, T: FieldValue>(read: fn(&S) -> &T) -> MemberKind {
    let _ = read;
    T::MEMBER
}

/// The return of a method whose object is reached when the call runs.
///
/// The asynchronous method of a part reaches its struct inside the future
/// it answers, which then completes with the return of the method or with
/// the `TypeError` of the method `name` read on `undefined`.
pub enum Reached<R> {
    /// The return of the method.
    Object(R),
    /// The object is not reached, and `name` is the method.
    Unreached(&'static str),
}

impl<R: IntoCompletion> IntoCompletion for Reached<R> {
    fn into_completion(self) -> Completion {
        match self {
            Self::Object(result) => result.into_completion(),
            Self::Unreached(name) => unreached(name),
        }
    }
}

/// Returns the completion of the method `name` of an object that is no
/// longer reached, the `TypeError` JavaScript throws for a property read on
/// `undefined`.
#[must_use]
pub fn unreached(name: &str) -> Completion {
    Completion::Throw(Value::from(ErrorObject::type_error(format!(
        "Cannot read properties of undefined (reading '{name}')"
    ))))
}
