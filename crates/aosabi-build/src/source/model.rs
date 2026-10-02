// SPDX-License-Identifier: MIT
/*
 * aosabi-build: the items of a crate as the reader keeps them.
 *
 * A TypeScript checker answers for the declarations of a program, and this
 * model holds what the plain source of a Rust crate states in their place.
 */

//! The model of a crate the reader builds.
//!
//! Every crate the reader reads, the crate of the package and each of its
//! path dependencies, is one [`Library`] with its root module and the crates
//! it names. Every module of the tree is one [`Module`] of one library, and
//! every item of a module one [`Item`], with its name, its visibility, its file
//! and line, its doc text and what it declares. Every name a `use` binds is one
//! [`Use`] of its module, with its path, its visibility and its file and line.
//! The syntax of a type, a bound, a signature and a body stays as `syn` parsed
//! it, after the strip of its file, because the reach and the entry read it
//! again in the context of the module that writes it.

use std::path::PathBuf;

use super::Place;

/// One library crate the reader read, the crate of the package it reads or
/// a path dependency of a crate it read.
#[derive(Debug)]
pub(crate) struct Library {
    /// The module of its crate root.
    pub(crate) root: usize,
    /// Its crate name, which opens the path of every item of a path
    /// dependency.
    pub(crate) name: String,
    /// The name of its package.
    pub(crate) package: String,
    /// The crates it names as dependencies, each by the name it sees the
    /// crate by, with the library the reader read for a path dependency and
    /// `None` for a crate whose source the reader does not read.
    pub(crate) externs: Vec<(String, Option<usize>)>,
}

/// One module of the tree of a crate.
#[derive(Debug)]
pub(crate) struct Module {
    /// The library the module belongs to, 0 for the crate of the package.
    pub(crate) krate: usize,
    /// The names of the modules from the crate root to this one, empty for
    /// the root.
    pub(crate) path: Vec<String>,
    /// The module that declares this one, `None` for the root.
    pub(crate) parent: Option<usize>,
    /// The items the module declares, as indexes into the items of the tree.
    pub(crate) items: Vec<usize>,
    /// The `use` declarations of the module, one per name they bind.
    pub(crate) uses: Vec<Use>,
    /// The file the items of the module stand in.
    pub(crate) file: PathBuf,
}

/// One name a `use` declaration binds, or one glob it imports.
#[derive(Debug)]
pub(crate) struct Use {
    /// Whether the path opens with `::`, which names an extern crate.
    pub(crate) leading: bool,
    /// The segments of the path, their raw prefixes taken off.
    pub(crate) path: Vec<String>,
    /// The name the declaration binds, `None` for a glob.
    pub(crate) name: Option<String>,
    /// The visibility of the declaration, which decides who a glob of the
    /// module reaches it from.
    pub(crate) vis: Vis,
    /// The place of the name or of the glob in the tree of the declaration.
    pub(crate) place: Place,
}

/// The visibility of an item or a `use` declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Vis {
    /// None, or `pub(self)`, which is visible in the module and below it.
    Private,
    /// `pub`.
    Public,
    /// `pub(crate)` or `pub(in crate)`.
    Crate,
    /// `pub(super)`.
    Super,
    /// `pub(in path)` with any other path, kept as visible in the crate,
    /// since a path the compiler accepts names an ancestor.
    Restricted,
}

/// The doc text of an item, one entry per `///` line, `/** */` block or
/// `#[doc]` attribute, with the form and the place of each.
#[derive(Debug, Clone, Default)]
pub(crate) struct Docs {
    /// The text of each attribute, as the literal holds it.
    pub(crate) lines: Vec<String>,
    /// The form and the place of each attribute of `lines`, in the same
    /// order, which the reading of a comment joins into blocks.
    pub(crate) spans: Vec<DocSpan>,
    /// The place of a `#[doc]` whose value is no string literal, such as
    /// `include_str!(..)`, whose text a macro expands.
    pub(crate) unread: Option<Place>,
    /// The place of a `#[deprecated]` on the item, which has no GraphQL form.
    pub(crate) deprecated: Option<Place>,
}

/// The form and the place of one doc attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DocSpan {
    /// Whether the attribute is a `/** */` block, whose lines keep their
    /// gutter, rather than a `///` line or a `#[doc]` written out.
    pub(crate) block: bool,
    /// The line the attribute starts on, counted from 1.
    pub(crate) line: usize,
    /// The column the attribute starts at, counted from 0 in characters.
    pub(crate) column: usize,
}

/// One item of a module.
pub(crate) struct Item {
    /// The name, its raw prefix taken off, empty for an impl block and a
    /// macro call.
    pub(crate) name: String,
    /// The module that declares it.
    pub(crate) module: usize,
    /// Its visibility.
    pub(crate) vis: Vis,
    /// The place of its name, or of the keyword of an impl block.
    pub(crate) place: Place,
    /// Its doc text.
    pub(crate) docs: Docs,
    /// Whether a derive of `Aosabi`, `#[aosabi::scalar]` or
    /// `#[aosabi::resolvers]` stands on it, which the twins of the derive
    /// path write.
    pub(crate) described: bool,
    /// What it declares.
    pub(crate) kind: Kind,
}

/// What an item declares.
pub(crate) enum Kind {
    /// A struct.
    Struct(Data),
    /// An enum.
    Enum(Enumeration),
    /// A union, which has no GraphQL form.
    Union,
    /// A type alias.
    Alias(Alias),
    /// A trait or a trait alias.
    Trait,
    /// A function.
    Function(Function),
    /// A constant or a static, with its type.
    Value(syn::Type),
    /// A module, as an index into the modules of the tree.
    Module(usize),
    /// An impl block.
    Impl(Impl),
    /// A macro call, or a `macro_rules!` that defines one.
    Macro(MacroCall),
    /// An `extern crate`, with the name of the crate it names, `self` for the
    /// crate itself, while the item takes the name it binds.
    ExternCrate(String),
}

/// The shape of a struct.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Shape {
    /// Named fields.
    Named,
    /// Fields by position.
    Tuple,
    /// No field at all, `struct Query;`.
    Unit,
}

/// The fields of a struct.
pub(crate) struct Data {
    /// The names of its type parameters, in order.
    pub(crate) generics: Vec<String>,
    /// The count of its lifetime parameters, which the glue writes as
    /// `'static`.
    pub(crate) lifetimes: usize,
    /// The trait bounds of its type parameters, each with the name of its
    /// parameter, from the list of parameters and from the where clause.
    pub(crate) bounds: Vec<(String, syn::TypeParamBound)>,
    /// Whether its fields are named, positional or absent.
    pub(crate) shape: Shape,
    /// Its fields, in order.
    pub(crate) fields: Vec<Field>,
}

/// One field of a struct or of a variant.
pub(crate) struct Field {
    /// The name, or the position as text for a field of a tuple.
    pub(crate) name: String,
    /// Its visibility.
    pub(crate) vis: Vis,
    /// Its type.
    pub(crate) ty: syn::Type,
    /// The place of its name, or of its type for a field of a tuple.
    pub(crate) place: Place,
    /// Its doc text.
    pub(crate) docs: Docs,
}

/// The parameters and the variants of an enum.
pub(crate) struct Enumeration {
    /// The names of its type parameters, in order.
    pub(crate) generics: Vec<String>,
    /// The count of its lifetime parameters, which the glue writes as
    /// `'static`.
    pub(crate) lifetimes: usize,
    /// Its variants, in order.
    pub(crate) variants: Vec<Variant>,
}

/// One variant of an enum.
pub(crate) struct Variant {
    /// Its name, its raw prefix taken off.
    pub(crate) name: String,
    /// The place of its name.
    pub(crate) place: Place,
    /// Its doc text.
    pub(crate) docs: Docs,
    /// Whether its fields are named, positional or absent.
    pub(crate) shape: Shape,
    /// Its fields, empty for a unit variant.
    pub(crate) fields: Vec<Field>,
}

/// A type alias.
pub(crate) struct Alias {
    /// The names of its type parameters, in order.
    pub(crate) generics: Vec<String>,
    /// The type it names.
    pub(crate) ty: syn::Type,
}

/// The form in which a method takes `self`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Receiver {
    /// A shared reference, `&self`, `&'a self` or `self: &Self`.
    Shared,
    /// A unique reference, `&mut self` or `self: &mut Self`.
    Unique,
    /// The value, `self`, `mut self` or `self: Self`.
    Owned,
    /// Any other type of `self`, such as `self: Box<Self>`.
    Typed,
}

/// A function or a method.
pub(crate) struct Function {
    /// The form in which it takes `self`, `None` for a function with no
    /// receiver.
    pub(crate) receiver: Option<Receiver>,
    /// Whether it is an `async fn`, whose answer is a promise.
    pub(crate) asyncness: bool,
    /// The names of its type parameters, in order.
    pub(crate) generics: Vec<String>,
    /// The names of its const parameters, in order.
    pub(crate) consts: Vec<String>,
    /// Its parameters after the receiver, in order.
    pub(crate) params: Vec<Param>,
    /// Its return type, `None` for `()`.
    pub(crate) output: Option<syn::Type>,
    /// Its body.
    pub(crate) block: syn::Block,
}

/// One parameter of a function.
pub(crate) struct Param {
    /// Its name, its raw prefix taken off, or the pattern as text when the
    /// pattern is no plain name.
    pub(crate) name: String,
    /// Whether the pattern is a plain name.
    pub(crate) named: bool,
    /// Its type.
    pub(crate) ty: syn::Type,
    /// The place of its pattern.
    pub(crate) place: Place,
}

/// One method of an impl block.
pub(crate) struct Method {
    /// Its name, its raw prefix taken off.
    pub(crate) name: String,
    /// Its visibility.
    pub(crate) vis: Vis,
    /// The place of its name.
    pub(crate) place: Place,
    /// Its doc text.
    pub(crate) docs: Docs,
    /// Its signature and body.
    pub(crate) function: Function,
}

/// An impl block.
pub(crate) struct Impl {
    /// The names of its type parameters, in order.
    pub(crate) generics: Vec<String>,
    /// The trait bounds of its type parameters, each with the name of its
    /// parameter, from the list of parameters and from the where clause.
    pub(crate) bounds: Vec<(String, syn::TypeParamBound)>,
    /// Whether its where clause bounds a type that is no type parameter of
    /// the block, such as `Vec<T>: Clone`.
    pub(crate) bounds_other: bool,
    /// The type the block implements for.
    pub(crate) self_ty: syn::Type,
    /// The trait of a trait impl, `None` for an inherent block.
    pub(crate) trait_: Option<syn::Path>,
    /// Its methods, in order.
    pub(crate) methods: Vec<Method>,
}

/// A macro call at the level of items, or a `macro_rules!` definition.
#[derive(Debug)]
pub(crate) struct MacroCall {
    /// The path of the macro as written.
    pub(crate) path: String,
    /// The name a `macro_rules!` defines, `None` for a call.
    pub(crate) defines: Option<String>,
}

impl Item {
    /// Returns whether the item is a type of the type namespace, a struct, an
    /// enum, a union, an alias or a trait.
    pub(crate) fn is_type(&self) -> bool {
        matches!(
            self.kind,
            Kind::Struct(_) | Kind::Enum(_) | Kind::Union | Kind::Alias(_) | Kind::Trait
        )
    }

    /// Returns whether the item has a name in the value namespace, a
    /// function, a constant, a static, or a struct of a tuple or of no field,
    /// whose name is its constructor.
    pub(crate) fn is_value(&self) -> bool {
        match &self.kind {
            Kind::Function(_) | Kind::Value(_) => true,
            Kind::Struct(data) => data.shape != Shape::Named,
            _ => false,
        }
    }
}
