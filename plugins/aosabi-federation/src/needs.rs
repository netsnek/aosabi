// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-federation: the selection a delegation fetches beside what the
 * client selects.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/core/gateway.ts,
 * Apache-2.0. Changed by the Aosabi authors.
 *
 * The types of a needs selection, and, buildSelectionsFromNeeds.
 */

//! The `needs` selection of a delegation, [`Needs`], and the field
//! selections it builds, [`build_selections_from_needs`].
//!
//! A delegation names the fields it fetches beside what the client selects,
//! which a patch or a guard reads, as an object whose keys are field names
//! and whose values are flags or nested objects, with the member `__args` of
//! a nested object for the arguments of its field. [`Needs`] holds one
//! [`NeedsNode`] per key of that object in the order of its keys. A key
//! whose value is falsy selects nothing and stays a node, and so does the
//! key `__args`, whose object is the `args` of the node above it. A needs
//! object of such keys alone is then still present, which is what
//! `Object.keys(needs).length` counts where `InjectNeedsTransform` decides
//! whether it rewrites the request.
//!
//! The typed builder of a needs selection, [`NeedsBuilder`], is what the
//! literal [󰀁](crate::needs!) expands into: a closure over a builder typed
//! by the record the delegated field returns, which names each field through
//! an accessor the builder never calls. A field the record lacks is then
//! refused by the compiler at its name. [`NeedsTarget`] finds that record
//! through the `Option`, `Vec`, `Box` and `Callable` around it, and a pulled
//! registry implements it for each of its records, since a blanket
//! implementation over `RemoteType` beside the one for `Option` is refused
//! with E0119 outside the crate of `RemoteType`.
//!
//! A key of the literal is an identifier or a string literal. An identifier
//! names the field of the record and the field of the remote at once, and a
//! raw identifier, `r#type`, names the remote field without its prefix, as
//! the derive reads the member of such a field. A string literal names the
//! remote field by its text alone, the twin of a quoted key of the
//! reference, so it reaches a field whose Rust name is no spelling of its
//! GraphQL name, `self` among them, and the compiler checks no record for
//! it. Below such a key the record is [`TextOnly`], so every key below it is
//! a string literal too.

use std::{
    borrow::Cow,
    marker::PhantomData,
    rc::Rc, //
};

use aosabi_core::{
    descriptor::{
        Any,
        Date,
        File,
        Float,
        ID,
        Int, //
    },
    remote::Callable, //
};
#[doc(hidden)]
pub use aosabi_json::JsObject as __JsObject;
use aosabi_json::{
    JsObject,
    JsValue,
    into_json::IntoJson, //
};
use aosabi_lang::{
    SharedSlice,
    ast::{
        ArgumentNode,
        FieldNode,
        NameNode,
        SelectionNode,
        SelectionSetNode, //
    },
    js_string::to_string_lossy, //
};

use crate::ast_from_js::ast_from_js_value;

/// A `needs` selection, the fields fetched beside what the client selects.
///
/// The twin of a value of `NeedsMap`, one node per key of the object in the
/// order of its keys.
#[derive(Clone, Debug, Default)]
pub struct Needs(Rc<[NeedsNode]>);

impl Needs {
    /// Returns the selection of `nodes`, one per key of the needs object.
    #[must_use]
    pub fn new(nodes: Vec<NeedsNode>) -> Self {
        Self(Rc::from(nodes))
    }

    /// Returns the nodes, one per key of the needs object.
    #[must_use]
    pub fn nodes(&self) -> &[NeedsNode] {
        &self.0
    }

    /// Returns whether the needs object has no key, `Object.keys(needs).length === 0`.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// One key of a `needs` selection, a field with its arguments and the fields it needs below it.
///
/// A value `true` is a node that selects its field with no argument and no
/// child, a falsy value is a node that selects nothing, and an object is a
/// node that selects its field with the arguments of its member `__args` and
/// one child per key.
#[derive(Clone, Debug)]
pub struct NeedsNode {
    /// The name of the field, the key.
    pub name: Cow<'static, str>,
    /// Whether the key selects its field, which a falsy value does not.
    pub selected: bool,
    /// The arguments of the field, the object of the member `__args`.
    pub args: Option<JsObject>,
    /// The keys of an object value, one node per key in the order of the keys.
    pub children: Rc<[NeedsNode]>,
}

/// Returns the field selections a needs selection asks for.
///
/// A node that selects nothing and a node named `__args` are passed over.
/// Every other node is a field of its name with no alias, with the selection
/// set of its children when they build at least one selection, and with one
/// argument per member of its `args` in the order of the keys, whose value
/// is [`ast_from_js_value`] with no collector, so a member that is
/// `undefined` writes `null` and a file writes an input object with no
/// field.
#[must_use]
pub fn build_selections_from_needs(needs: &[NeedsNode]) -> Vec<SelectionNode> {
    let mut selections = Vec::new();
    for node in needs {
        if node.name == "__args" || !node.selected {
            continue;
        }
        let nested_selections = build_selections_from_needs(&node.children);
        let selection_set = (!nested_selections.is_empty()).then_some(SelectionSetNode {
            loc: None,
            selections: SharedSlice::from(nested_selections),
        });
        let args_nodes = node.args.as_ref().map(|args| {
            args.own_keys()
                .map(|(arg_name, arg_val)| ArgumentNode {
                    loc: None,
                    name: NameNode::new(None, to_string_lossy(arg_name)),
                    value: ast_from_js_value(arg_val, None),
                })
                .collect()
        });
        selections.push(SelectionNode::Field(FieldNode {
            loc: None,
            alias: None,
            name: NameNode::new(None, node.name.as_ref()),
            arguments: args_nodes,
            directives: None,
            selection_set,
        }));
    }
    selections
}

/// A value a `needs` selection reaches through, with the record whose fields it names.
///
/// A pulled record is its own record, and an `Option`, a `Vec`, a `Box` or a
/// `Callable` reaches the record of what it holds or returns. A leaf is its
/// own record as well, so a delegation of a field that answers a scalar
/// takes the empty literal `needs! {}`.
pub trait NeedsTarget {
    /// The record whose fields a selection under this value names.
    type Record;
}

/// Implements [`NeedsTarget`] of a leaf as its own record.
macro_rules! leaf_target {
    ($($type:ty),+ $(,)?) => {
        $(
            impl NeedsTarget for $type {
                type Record = Self;
            }
        )+
    };
}

leaf_target!(
    String,
    bool,
    f64,
    (),
    ID,
    Int,
    Float,
    Date,
    File,
    Any,
    JsValue,
    JsObject
);

impl<X: NeedsTarget> NeedsTarget for Option<X> {
    type Record = X::Record;
}

impl<X: NeedsTarget> NeedsTarget for Vec<X> {
    type Record = X::Record;
}

impl<X: NeedsTarget> NeedsTarget for Box<X> {
    type Record = X::Record;
}

impl<A, X: NeedsTarget> NeedsTarget for Callable<A, X> {
    type Record = X::Record;
}

/// The builder [`needs!`](crate::needs!) expands into, typed by the record whose fields it names.
///
/// Each field is named twice, by its text, which the selection sends, and
/// by an accessor of the record, which the builder never calls and which
/// makes the compiler refuse a name the record lacks.
pub struct NeedsBuilder<Rec> {
    /// The nodes in the order of the keys.
    nodes: Vec<NeedsNode>,
    /// The arguments of the field this builder selects below, the member `__args`.
    args: Option<JsObject>,
    /// The record whose fields the builder names.
    record: PhantomData<fn(&Rec)>,
}

/// The record of a selection below a key written as a string literal, which has no field.
///
/// No accessor names the field of such a key, so the record of the
/// selection below it is unknown, and an identifier key there is refused at
/// its name with E0609, while a string literal key names a field by its text.
#[derive(Clone, Copy, Debug)]
pub enum TextOnly {}

impl<Rec> Default for NeedsBuilder<Rec> {
    fn default() -> Self {
        Self::new()
    }
}

impl<Rec> NeedsBuilder<Rec> {
    /// Returns a builder that names no field, the object literal `{}`.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            nodes: Vec::new(),
            args: None,
            record: PhantomData,
        }
    }

    /// Selects the field `name` with no argument and no child, the value `true`.
    #[must_use]
    pub fn field<V>(mut self, name: &'static str, check: fn(&Rec) -> &V) -> Self {
        let _ = check;
        self.nodes.push(leaf(name, true));
        self
    }

    /// Names the field `name` and selects nothing for it, the value `false`.
    #[must_use]
    pub fn unselected<V>(mut self, name: &'static str, check: fn(&Rec) -> &V) -> Self {
        let _ = check;
        self.nodes.push(leaf(name, false));
        self
    }

    /// Selects the field `name` with the selection `inner` builds below it, a nested object.
    #[must_use]
    pub fn nested<V: NeedsTarget>(
        mut self,
        name: &'static str,
        check: fn(&Rec) -> &V,
        inner: impl FnOnce(NeedsBuilder<V::Record>) -> NeedsBuilder<V::Record>,
    ) -> Self {
        let _ = check;
        let built = inner(NeedsBuilder::new());
        self.nodes.push(NeedsNode {
            name: Cow::Borrowed(name),
            selected: true,
            args: built.args,
            children: Rc::from(built.nodes),
        });
        self
    }

    /// Selects the field the text `name` names with no argument and no child, a quoted key with the value `true`.
    ///
    /// The record checks no name, so the text reaches a remote field whose
    /// Rust name is no spelling of its GraphQL name.
    #[must_use]
    pub fn field_named(mut self, name: &'static str) -> Self {
        self.nodes.push(leaf(name, true));
        self
    }

    /// Names the field the text `name` names and selects nothing for it, a quoted key with the value `false`.
    #[must_use]
    pub fn unselected_named(mut self, name: &'static str) -> Self {
        self.nodes.push(leaf(name, false));
        self
    }

    /// Selects the field the text `name` names with the selection `inner` builds below it, a quoted key with a nested object.
    ///
    /// The record below the key is [`TextOnly`], since no accessor names the
    /// field, so every key `inner` names is a string literal as well.
    #[must_use]
    pub fn nested_named(
        mut self,
        name: &'static str,
        inner: impl FnOnce(NeedsBuilder<TextOnly>) -> NeedsBuilder<TextOnly>,
    ) -> Self {
        let built = inner(NeedsBuilder::new());
        self.nodes.push(NeedsNode {
            name: Cow::Borrowed(name),
            selected: true,
            args: built.args,
            children: Rc::from(built.nodes),
        });
        self
    }

    /// Sets the arguments of the field this builder selects below, the member `__args`.
    #[must_use]
    pub fn args(mut self, args: JsObject) -> Self {
        self.nodes.push(leaf("__args", true));
        self.args = Some(args);
        self
    }

    /// Returns the selection the builder names.
    #[must_use]
    pub fn build(self) -> Needs {
        Needs::new(self.nodes)
    }
}

/// Returns the node of a key whose value is `true` or `false`.
fn leaf(name: &'static str, selected: bool) -> NeedsNode {
    NeedsNode {
        name: Cow::Borrowed(name),
        selected,
        args: None,
        children: Rc::from(Vec::new()),
    }
}

/// Returns the value of one member of `__args`, a value of any JSON kind.
#[doc(hidden)]
pub fn __needs_arg(value: impl IntoJson) -> JsValue {
    value.into_json()
}

/// Builds the selection of a delegation from a literal of field names, the `needs` object of the reference.
///
/// The literal expands into a closure over the [`NeedsBuilder`] of the
/// record the delegated field returns, so every name is checked against
/// that record and a name it lacks is refused at that name. A key is a
/// field name, alone or with `true`, which selects the field, with `false`,
/// which names it and selects nothing, or with a nested literal, which
/// selects the field with the selection below it. The key `__args` of a
/// nested literal holds the arguments of its field, each a value of any
/// JSON kind.
///
/// A key is an identifier, checked against the record, or a string literal,
/// which names the remote field by its text with no check, the form of a
/// field whose GraphQL name no Rust identifier spells. A raw identifier
/// names the field without its prefix, so `r#type` selects the field
/// `type`, and the name of an argument of `__args` is read the same way.
///
/// # Examples
///
/// ```ignore
/// users().delegate(
///     users::Query::user { id },
///     needs! { id, email, orders: { __args: { status: "ACTIVE" }, id } },
/// )
/// ```
#[macro_export]
macro_rules! needs {
    ($($body:tt)*) => {
        |builder| $crate::__federation_needs!(builder; $($body)*)
    };
}

/// Adds the keys of a [`needs!`](crate::needs!) literal to a builder, one at a time.
#[doc(hidden)]
#[macro_export]
macro_rules! __federation_needs {
    ($builder:expr;) => {
        $builder
    };
    ($builder:expr; __args : { $($name:tt : $value:expr),* $(,)? } $(, $($rest:tt)*)?) => {
        $crate::__federation_needs!(
            $builder.args({
                #[allow(unused_mut, reason = "an empty literal sets no argument")]
                let mut args = $crate::needs::__JsObject::new();
                $(args.insert($crate::__federation_key!($name), $crate::needs::__needs_arg($value));)*
                args
            });
            $($($rest)*)?
        )
    };
    ($builder:expr; "__args" : { $($inner:tt)* } $(, $($rest:tt)*)?) => {
        $crate::__federation_needs!($builder; __args : { $($inner)* } $(, $($rest)*)?)
    };
    ($builder:expr; $name:ident : true $(, $($rest:tt)*)?) => {
        $crate::__federation_needs!(
            $builder.field($crate::__federation_key!($name), |record| &record.$name);
            $($($rest)*)?
        )
    };
    ($builder:expr; $name:ident : false $(, $($rest:tt)*)?) => {
        $crate::__federation_needs!(
            $builder.unselected($crate::__federation_key!($name), |record| &record.$name);
            $($($rest)*)?
        )
    };
    ($builder:expr; $name:ident : { $($inner:tt)* } $(, $($rest:tt)*)?) => {
        $crate::__federation_needs!(
            $builder.nested(
                $crate::__federation_key!($name),
                |record| &record.$name,
                |builder| $crate::__federation_needs!(builder; $($inner)*),
            );
            $($($rest)*)?
        )
    };
    ($builder:expr; $name:ident $(, $($rest:tt)*)?) => {
        $crate::__federation_needs!(
            $builder.field($crate::__federation_key!($name), |record| &record.$name);
            $($($rest)*)?
        )
    };
    ($builder:expr; $name:literal : true $(, $($rest:tt)*)?) => {
        $crate::__federation_needs!($builder.field_named($name); $($($rest)*)?)
    };
    ($builder:expr; $name:literal : false $(, $($rest:tt)*)?) => {
        $crate::__federation_needs!($builder.unselected_named($name); $($($rest)*)?)
    };
    ($builder:expr; $name:literal : { $($inner:tt)* } $(, $($rest:tt)*)?) => {
        $crate::__federation_needs!(
            $builder.nested_named($name, |builder| $crate::__federation_needs!(builder; $($inner)*));
            $($($rest)*)?
        )
    };
    ($builder:expr; $name:literal $(, $($rest:tt)*)?) => {
        $crate::__federation_needs!($builder.field_named($name); $($($rest)*)?)
    };
}
