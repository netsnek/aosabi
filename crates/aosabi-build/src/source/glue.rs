// SPDX-License-Identifier: MIT
/*
 * aosabi-build: the generated module of a plain crate, the glue of every
 * root, type and inherent block its roots reach.
 *
 * A TypeScript service hands its objects to the executor as JavaScript
 * values, and the glue is what makes a Rust value one, which the macros
 * write on the derive path.
 */

//! The glue of a plain crate.
//!
//! The lowering of the module `describe` records, beside the dump, every use of
//! a struct and of an enum the roots reach, with the GraphQL name of each
//! member, each method and each parameter. [`glue`] turns that plan into the
//! one module the generated copy of the crate adds to its crate root, through
//! the library `aosabi-codegen`, which the macros of the derive path call as
//! well.
//!
//! The module is a child of the crate root, so it names every type by its path
//! from the crate root and sees what the crate root and its descendants see. A
//! type, a field or a method the module cannot name is refused at the line of
//! the author with what to write instead: an item that a module below the crate
//! root keeps private, and a type of a path dependency, whose glue would
//! implement a trait of the facade for a type of another crate, which the
//! orphan rule of Rust refuses. A generic struct or enum takes one glue for
//! each Rust type its uses name, its arguments written from the crate root
//! through every type alias and through `Self`, which names the self type of
//! its block or of its item, the uses that a field, a method or a payload of
//! another use names under the arguments of that use among them, which the
//! lowering reaches. A struct reads an input object only where an argument
//! reaches its Rust type, which the lowering marks. An argument of `std`,
//! `core` or `alloc` is written by a path a crate without the standard library
//! resolves as well, from `::core` for a type of `core` and through the
//! re-exports of the facade for a type of `alloc`, and an argument outside the
//! ten types of the three the builder describes is refused at its line. A
//! lifetime parameter is `'static` in the glue, since the executor holds a
//! value with no borrow, which the bound `'static` of the traits of the glue
//! states, so a struct that borrows a slice is reached in its `'static` form.

use std::{
    collections::BTreeSet,
    rc::Rc, //
};

use aosabi_codegen::plain::{
    self,
    PlainEntry,
    PlainEnum,
    PlainField,
    PlainObject,
    PlainUnion, //
};
use aosabi_infer::SchemaBuilder;
use aosabi_json::{
    JsValue,
    map::IndexMap, //
};
use proc_macro2::TokenStream;
use syn::{
    GenericArgument,
    PathArguments,
    Type,
    spanned::Spanned, //
};

use super::{
    Crate,
    Refusal,
    SourceTypes,
    describe::{
        self,
        Plan,
        PlanEnum,
        PlanObject, //
    },
    entry::type_arguments,
    model::{
        Alias,
        Kind,
        Vis, //
    },
    reach::{
        ALIAS_DEPTH,
        Arg,
        Cx,
        zip, //
    },
    resolve::{
        Ns,
        Res,
        Resolver, //
    }, //
};

/// The name of the generated module in the crate root of the copy.
pub const MODULE: &str = "__aosabi";

/// The generated module of a plain crate.
#[derive(Debug, Clone)]
pub struct Glue {
    /// The text of the module, which a `#[path]` module of the crate root of
    /// the generated copy includes.
    pub text: String,
    /// The path of the entry from the crate root, its one function name.
    pub entry: String,
}

/// Returns the glue of every root, type and inherent block the roots of
/// `krate` reach, and of its entry.
///
/// # Errors
///
/// Returns every refusal of the reader and of the lowering, which the build
/// of the schema names first, and every type, field or method the module
/// cannot name, each at the place of the author.
pub fn glue(krate: &Crate) -> Result<Glue, Vec<Refusal>> {
    let mut refusals = krate.refusals().to_vec();
    let Some(entry) = krate.entry() else {
        return Err(refusals);
    };
    let resolver = krate.resolver();
    let (dump, lowered, plan) = describe::lower(
        &resolver,
        krate.package(),
        &entry.function,
        krate.registrations(),
    );
    refusals.extend(lowered);
    if !refusals.is_empty() {
        return Err(refusals);
    }
    let names = output_names(&dump);
    let members: BTreeSet<u32> = plan
        .enums
        .iter()
        .flat_map(|enumeration| enumeration.members.iter().copied())
        .collect();
    let mut writer = Writer {
        resolver: &resolver,
        refusals: Vec::new(),
        targets: BTreeSet::new(),
        typenames: members
            .iter()
            .filter_map(|node| names.get(node).map(|name| (*node, name.clone())))
            .collect(),
    };
    let objects = writer.objects(&plan);
    let (enums, unions) = writer.enums(&plan);
    let mut parts: Vec<String> = Vec::new();
    for registration in krate.registrations() {
        let target = format!("crate::{}", registration.path);
        if !parts.contains(&target) {
            parts.push(target);
        }
    }
    if !writer.refusals.is_empty() {
        return Err(writer.refusals);
    }
    let root = PlainEntry {
        function: tokens(&format!("crate::{}", entry.function)),
        result: entry.result,
        parts: parts.iter().map(|part| tokens(part)).collect(),
    };
    let module = plain::module(&objects, &enums, &unions, &root);
    Ok(Glue {
        text: module_text(krate.package(), &module),
        entry: entry.function.clone(),
    })
}

/// Returns the GraphQL name of every node of the dump `dump` that the
/// inference names as an output type, by the position of the node.
///
/// The names are those the inference gives the types it meets, a generic use
/// named after its arguments and a use whose name another type holds among
/// them, so a value names the type the schema prints for it. A dump the
/// inference refuses names nothing, since the build of its schema refuses
/// the crate with the same error.
fn output_names(dump: &JsValue) -> IndexMap<u32, String> {
    let mut out = IndexMap::default();
    let Ok(types) = SourceTypes::from_dump(dump) else {
        return out;
    };
    let Ok((_, names)) = SchemaBuilder::new(&types).build_named() else {
        return out;
    };
    for (r#type, name) in names {
        if let Some(node) = types.node_of(r#type) {
            out.entry(node).or_insert(name);
        }
    }
    out
}

/// Returns the text of the generated module of the package `package` with
/// the items `module`, one item per line.
fn module_text(package: &str, module: &TokenStream) -> String {
    let mut text = format!(
        "// The glue of the plain crate {package}, which cargo aosabi writes from its source. The next build writes it again.\n\
         //! The run time glue of the roots, the types and the inherent blocks the\n\
         //! roots of the crate reach, which the generated copy of the crate adds\n\
         //! to its crate root and the application never writes.\n\
         #![allow(clippy::all, clippy::pedantic, reason = \"the module is generated\")]\n\n"
    );
    for item in split_items(module) {
        text.push_str(&item);
        text.push_str("\n\n");
    }
    text
}

/// Returns the items of `module`, each as one line of text.
///
/// An item ends after a top level group of braces, which every
/// implementation and function of the module is, and a semicolon that
/// follows the group stays with it.
fn split_items(module: &TokenStream) -> Vec<String> {
    let mut items: Vec<TokenStream> = Vec::new();
    let mut current = TokenStream::new();
    for tree in module.clone() {
        let semicolon =
            matches!(&tree, proc_macro2::TokenTree::Punct(punct) if punct.as_char() == ';');
        if semicolon
            && current.is_empty()
            && let Some(last) = items.last_mut()
        {
            last.extend([tree]);
            continue;
        }
        let ends = matches!(&tree, proc_macro2::TokenTree::Group(group)
            if group.delimiter() == proc_macro2::Delimiter::Brace);
        current.extend([tree]);
        if ends {
            items.push(core::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        items.push(current);
    }
    items.iter().map(ToString::to_string).collect()
}

/// Returns the tokens of the path text `text`.
fn tokens(text: &str) -> TokenStream {
    text.parse().unwrap_or_default()
}

/// The writer of the glue of the uses of a plan.
struct Writer<'a> {
    /// The resolver of the crate.
    resolver: &'a Resolver<'a>,
    /// The refusals so far.
    refusals: Vec<Refusal>,
    /// The targets written so far, each once.
    targets: BTreeSet<String>,
    /// The GraphQL name every use of a struct that a variant of an enum
    /// holds states as its `__typename`, by the node of the use.
    typenames: IndexMap<u32, String>,
}

impl Writer<'_> {
    /// Returns the glue of every use of a struct of `plan`.
    fn objects(&mut self, plan: &Plan) -> Vec<PlainObject> {
        let mut out = Vec::new();
        for object in &plan.objects {
            if let Some(written) = self.object(object) {
                out.push(written);
            }
        }
        out
    }

    /// Returns the glue of the use `object`, or `None` for a use already
    /// written or refused.
    fn object(&mut self, object: &PlanObject) -> Option<PlainObject> {
        let target = self.target(object.item, &object.args)?;
        if !self.targets.insert(target.clone()) {
            return None;
        }
        let resolver = self.resolver;
        let item = &resolver.items[object.item];
        let module = item.module;
        let owner = resolver.path_of(object.item);
        let mut fields = Vec::new();
        if object.named
            && let Kind::Struct(data) = &item.kind
        {
            for (field, (name, key)) in data.fields.iter().zip(&object.fields) {
                let seen = self.sees(&field.vis, module);
                if key.is_some() && !seen {
                    self.refuse_member(&field.place, &owner, name);
                    return None;
                }
                if key.is_none() && object.input && !seen {
                    self.refusals.push(Refusal {
                        place: field.place.clone(),
                        message: format!(
                            "the field `{name}` of `{owner}` is private to its module, and an argument reaches the struct, whose glue in the crate root gives the field its default. Make the field `pub(crate)`, or declare the struct in the crate root"
                        ),
                    });
                    return None;
                }
                fields.push(PlainField {
                    ident: plain::ident(name),
                    key: key.clone(),
                });
            }
        }
        let mut methods = Vec::new();
        for method in &object.methods {
            if !self.method_seen(object.item, &method.name) {
                self.refuse_member(&item.place, &owner, &method.name);
                return None;
            }
            methods.push(
                aosabi_codegen::glue::Method::plain(
                    plain::ident(&method.name),
                    method.key.clone(),
                    method.asyncness,
                    method
                        .params
                        .iter()
                        .map(|param| param.name.clone())
                        .collect(),
                )
                .streaming(method.stream)
                .mutation_payload(method.payload),
            );
        }
        Some(PlainObject {
            target: tokens(&target),
            generics: syn::Generics::default(),
            name: item.name.clone(),
            typename: self.typenames.get(&object.node).cloned(),
            fields,
            methods,
            input: object.input,
        })
    }

    /// Returns the glue of every use of an enum of `plan`, the enums of unit
    /// variants apart from the enums of payloads.
    fn enums(&mut self, plan: &Plan) -> (Vec<PlainEnum>, Vec<PlainUnion>) {
        let mut enums = Vec::new();
        let mut unions = Vec::new();
        for enumeration in &plan.enums {
            let Some(target) = self.target(enumeration.item, &enumeration.args) else {
                continue;
            };
            if !self.targets.insert(target.clone()) {
                continue;
            }
            let resolver = self.resolver;
            let item = &resolver.items[enumeration.item];
            if is_declaration(enumeration) {
                enums.push(PlainEnum {
                    target: tokens(&target),
                    generics: syn::Generics::default(),
                    ident: plain::ident(&item.name),
                    values: enumeration
                        .variants
                        .iter()
                        .filter_map(|(name, value)| {
                            value.clone().map(|value| (plain::ident(name), value))
                        })
                        .collect(),
                });
            } else {
                unions.push(PlainUnion {
                    target: tokens(&target),
                    generics: syn::Generics::default(),
                    variants: enumeration
                        .variants
                        .iter()
                        .map(|(name, _)| plain::ident(name))
                        .collect(),
                });
            }
        }
        (enums, unions)
    }

    /// Returns the path the module names the use of the item `index` with
    /// the arguments `args` by, `'static` for each of its lifetime
    /// parameters, refusing a type it cannot name.
    fn target(&mut self, index: usize, args: &[Option<Arg>]) -> Option<String> {
        let resolver = self.resolver;
        let item = &resolver.items[index];
        let path = resolver.path_of(index);
        if resolver.krate(index) != 0 {
            self.refusals.push(Refusal {
                place: item.place.clone(),
                message: format!(
                    "the type `{path}` of a path dependency is reached, and its glue would implement a trait of the facade for a type of another crate, which the orphan rule of Rust refuses. Declare the type in the crate, with `From` from the type of the dependency"
                ),
            });
            return None;
        }
        if !self.item_seen(index) {
            self.refusals.push(Refusal {
                place: item.place.clone(),
                message: format!(
                    "the type `{path}` is private to its module, and its glue stands in a module of the crate root, which cannot name it. Make the type and the modules that hold it `pub(crate)`"
                ),
            });
            return None;
        }
        match use_text(resolver, index, args) {
            Ok(text) => Some(text),
            Err(None) => None,
            Err(Some(arg)) => {
                let line = arg.ty.span().start().line;
                let place = if line == 0 {
                    item.place.clone()
                } else {
                    super::Place {
                        file: resolver.modules[arg.cx.module].file.clone(),
                        line,
                    }
                };
                self.refusals.push(Refusal {
                    place,
                    message: format!(
                        "a use of the generic type `{path}` takes the argument `{}`, which names a type the glue in the crate root cannot name. Write every type the argument names as a type of the crate, a name of the facade, or one of the types of `std`, `core` and `alloc` the builder describes, {}",
                        super::reach::type_text(&arg.ty),
                        DESCRIBED_STD
                    ),
                });
                None
            }
        }
    }

    /// Refuses the member `name` of `owner` at `place`, which the module of
    /// the glue cannot read.
    fn refuse_member(&mut self, place: &super::Place, owner: &str, name: &str) {
        self.refusals.push(Refusal {
            place: place.clone(),
            message: format!(
                "the member `{name}` of `{owner}` is visible to a module the glue in the crate root does not stand in. Make it `pub(crate)` or `pub`"
            ),
        });
    }

    /// Returns whether the visibility `vis` of a member declared in the module
    /// `module` lets a child of the crate root see the member.
    fn sees(&self, vis: &Vis, module: usize) -> bool {
        self.restricted_to(vis, module) == self.root()
    }

    /// Returns whether the method `name` of every inherent block of the item
    /// `index` that declares it is seen from a child of the crate root.
    fn method_seen(&self, index: usize, name: &str) -> bool {
        let resolver = self.resolver;
        resolver.items.iter().all(|item| {
            let Kind::Impl(imp) = &item.kind else {
                return true;
            };
            if imp.trait_.is_some() {
                return true;
            }
            imp.methods.iter().all(|method| {
                method.name != name
                    || !self.block_of(item, index)
                    || self.sees(&method.vis, item.module)
            })
        })
    }

    /// Returns whether the impl block `item` is a block of the item `index`.
    fn block_of(&self, item: &super::model::Item, index: usize) -> bool {
        let Kind::Impl(imp) = &item.kind else {
            return false;
        };
        super::reach::self_of(self.resolver, imp, item.module)
            .is_some_and(|(owner, _)| owner == index)
    }

    /// Returns whether a child of the crate root sees the item `index` and
    /// every module that holds it.
    fn item_seen(&self, index: usize) -> bool {
        let resolver = self.resolver;
        let item = &resolver.items[index];
        if !self.sees(&item.vis, item.module) {
            return false;
        }
        let mut module = item.module;
        while let Some(parent) = resolver.modules[module].parent {
            let declared = resolver.modules[parent].items.iter().find_map(|&at| {
                let declaring = &resolver.items[at];
                matches!(declaring.kind, Kind::Module(child) if child == module)
                    .then_some(&declaring.vis)
            });
            if let Some(vis) = declared
                && self.restricted_to(vis, parent) != self.root()
            {
                return false;
            }
            module = parent;
        }
        true
    }

    /// Returns the module whose descendants see an item of the visibility
    /// `vis` declared in the module `module`.
    fn restricted_to(&self, vis: &Vis, module: usize) -> usize {
        match vis {
            Vis::Private => module,
            Vis::Super => self.resolver.modules[module]
                .parent
                .unwrap_or_else(|| self.root()),
            Vis::Public | Vis::Crate | Vis::Restricted => self.root(),
        }
    }

    /// Returns the module of the crate root of the package.
    fn root(&self) -> usize {
        self.resolver.libraries[0].root
    }
}

/// Returns the path by which a child of the crate root names the use of the
/// item `index` of the crate with the arguments `args`, `'static` for each of
/// its lifetime parameters.
///
/// # Errors
///
/// Returns the first argument the module cannot name, or `None` for an
/// argument no use substitutes, which the lowering refuses.
fn use_text<'a>(
    resolver: &Resolver<'_>,
    index: usize,
    args: &'a [Option<Arg>],
) -> Result<String, Option<&'a Arg>> {
    let text = format!("crate::{}", resolver.path_of(index));
    let lifetimes = match &resolver.items[index].kind {
        Kind::Struct(data) => data.lifetimes,
        Kind::Enum(variants) => variants.lifetimes,
        _ => 0,
    };
    if lifetimes + args.len() == 0 {
        return Ok(text);
    }
    let mut written = vec![String::from("'static"); lifetimes];
    for arg in args {
        let arg = arg.as_ref().ok_or(None)?;
        written.push(type_text(resolver, &arg.ty, &arg.cx).ok_or(Some(arg))?);
    }
    Ok(format!("{text}<{}>", written.join(", ")))
}

/// Returns whether the enum of `enumeration` is one of unit variants.
fn is_declaration(enumeration: &PlanEnum) -> bool {
    enumeration
        .variants
        .iter()
        .all(|(_, value)| value.is_some())
}

/// The ten types of `std`, `core` and `alloc` the builder describes, which
/// the glue names as a type argument, as a refusal lists them.
const DESCRIBED_STD: &str =
    "`String`, `Vec`, `VecDeque`, `Box`, `Rc`, `Arc`, `Cow`, `Option`, `Result` and `Infallible`";

/// Returns the path by which a child of the crate root names the type of
/// `std`, `core` or `alloc` whose segments after the crate are `named`, or
/// whose name of the prelude is `named` alone, a path a crate without the
/// standard library resolves as well, or `None` for a type outside the ten
/// the builder describes.
///
/// `std` re-exports each of the ten under the module path `core` or `alloc`
/// gives it, so the segments after the crate name one type whichever of the
/// three a path opens with. A type of `core` takes its path from `::core`,
/// which every crate sees, and a type of `alloc` the re-export of the facade in
/// `__private`, since a crate, with the standard library or without it, sees
/// `alloc` only by an `extern crate` of its own.
fn std_path(named: &[String]) -> Option<&'static str> {
    let named: Vec<&str> = named.iter().map(String::as_str).collect();
    Some(match named.as_slice() {
        ["Option"] | ["option", "Option"] => "::core::option::Option",
        ["Result"] | ["result", "Result"] => "::core::result::Result",
        ["convert", "Infallible"] => "::core::convert::Infallible",
        ["String"] | ["string", "String"] => "::aosabi::__private::String",
        ["Vec"] | ["vec", "Vec"] => "::aosabi::__private::Vec",
        ["Box"] | ["boxed", "Box"] => "::aosabi::__private::Box",
        ["collections", "VecDeque"] | ["collections", "vec_deque", "VecDeque"] => {
            "::aosabi::__private::VecDeque"
        }
        ["rc", "Rc"] => "::aosabi::__private::Rc",
        ["sync", "Arc"] => "::aosabi::__private::Arc",
        ["borrow", "Cow"] => "::aosabi::__private::Cow",
        _ => return None,
    })
}

/// Returns the text of the type `ty`, written in `cx`, as a child of the
/// crate root names it, or `None` for a type it cannot name.
///
/// A type parameter takes the argument of its use, a type of the crate its
/// path from the crate root, a type of `std`, `core` or `alloc` the path of
/// [`std_path`], a primitive its name, and a name of the facade its path
/// from `::aosabi`, so the text names no item through `::std` or the prelude
/// of the standard library. A type alias takes the text of the type it names
/// where the module can name that type, and `Self` the text of the self type
/// of its block or of its item, so a use through the alias or through `Self`
/// and a use of the type it names are one Rust type to the glue as to rustc.
pub(crate) fn type_text(resolver: &Resolver<'_>, ty: &Type, cx: &Cx) -> Option<String> {
    match ty {
        Type::Paren(inner) => type_text(resolver, &inner.elem, cx),
        Type::Group(inner) => type_text(resolver, &inner.elem, cx),
        Type::Reference(reference) => {
            let lifetime = match &reference.lifetime {
                None => String::new(),
                Some(lifetime) if lifetime.ident == "static" => String::from("'static "),
                Some(_) => return None,
            };
            let mutability = if reference.mutability.is_some() {
                "mut "
            } else {
                ""
            };
            let inner = type_text(resolver, &reference.elem, cx)?;
            Some(format!("&{lifetime}{mutability}{inner}"))
        }
        Type::Slice(slice) => Some(format!("[{}]", type_text(resolver, &slice.elem, cx)?)),
        Type::Array(array) => {
            let syn::Expr::Lit(length) = &array.len else {
                return None;
            };
            let length = quote::ToTokens::to_token_stream(&length.lit).to_string();
            Some(format!(
                "[{}; {length}]",
                type_text(resolver, &array.elem, cx)?
            ))
        }
        Type::Tuple(tuple) => {
            let items: Option<Vec<String>> = tuple
                .elems
                .iter()
                .map(|item| type_text(resolver, item, cx))
                .collect();
            let items = items?;
            Some(if items.len() == 1 {
                format!("({},)", items[0])
            } else {
                format!("({})", items.join(", "))
            })
        }
        Type::Path(path) if path.qself.is_none() => path_text(resolver, &path.path, cx),
        _ => None,
    }
}

/// Returns the text of the type path `path`, written in `cx`, as a child of
/// the crate root names it.
fn path_text(resolver: &Resolver<'_>, path: &syn::Path, cx: &Cx) -> Option<String> {
    if path.leading_colon.is_none()
        && path.segments.len() == 1
        && matches!(path.segments[0].arguments, PathArguments::None)
    {
        let name = path.segments[0].ident.to_string();
        if let Some((_, arg)) = cx.params.iter().find(|(param, _)| *param == name) {
            let arg = arg.as_ref()?;
            return type_text(resolver, &arg.ty, &arg.cx);
        }
        // `Self` names the self type of its block or of its item, with the
        // arguments of the use the lowering reached it through, as the
        // lowering resolves it.
        if name == "Self" {
            let (owner, args) = cx.self_item.as_ref()?;
            if resolver.krate(*owner) != 0 {
                return None;
            }
            return use_text(resolver, *owner, args).ok();
        }
    }
    let segments = Resolver::segments(path);
    let leading = path.leading_colon.is_some();
    let base = match resolver
        .resolve(cx.module, leading, &segments, Ns::Type)
        .ok()?
    {
        Res::Item(index) => {
            if let Kind::Alias(alias) = &resolver.items[index].kind
                && let Some(text) = alias_text(resolver, index, alias, path, cx)
            {
                return Some(text);
            }
            if resolver.krate(index) != 0 {
                return None;
            }
            format!("crate::{}", resolver.path_of(index))
        }
        Res::Std(named) => String::from(std_path(&named)?),
        Res::Facade(named) => format!("::aosabi::{}", named.join("::")),
        Res::Prim(name) => name,
        _ => return None,
    };
    let last = path.segments.last()?;
    let PathArguments::AngleBracketed(arguments) = &last.arguments else {
        return Some(base);
    };
    let mut written = Vec::new();
    for argument in &arguments.args {
        match argument {
            GenericArgument::Type(ty) => written.push(type_text(resolver, ty, cx)?),
            GenericArgument::Lifetime(lifetime) if lifetime.ident == "static" => {
                written.push(String::from("'static"));
            }
            _ => return None,
        }
    }
    Some(format!("{base}<{}>", written.join(", ")))
}

/// Returns the text of the type the alias `alias`, the item `index`, names
/// with the arguments of `path`, written in `cx`, as a child of the crate
/// root names it, or `None` for a type it cannot name.
///
/// A use through the alias and a use of the type it names are one Rust
/// type, which the glue implements once, so the text follows the alias as
/// the lowering follows it, through at most [`ALIAS_DEPTH`] aliases. The
/// glue names a lifetime `'static` alone, so a use whose lifetime arguments
/// are all `'static` makes every lifetime of the type the alias names
/// `'static`, since an alias declares every lifetime it names but `'static`,
/// and a use with another lifetime names no type of the glue.
fn alias_text(
    resolver: &Resolver<'_>,
    index: usize,
    alias: &Alias,
    path: &syn::Path,
    cx: &Cx,
) -> Option<String> {
    if cx.aliases >= ALIAS_DEPTH {
        return None;
    }
    if let PathArguments::AngleBracketed(arguments) = &path.segments.last()?.arguments
        && arguments.args.iter().any(|argument| {
            matches!(argument, GenericArgument::Lifetime(lifetime) if lifetime.ident != "static")
        })
    {
        return None;
    }
    let mut ty = alias.ty.clone();
    static_lifetimes(&mut ty);
    let args: Vec<Option<Arg>> = type_arguments(path)
        .into_iter()
        .map(|ty| {
            Some(Arg {
                ty: ty.clone(),
                cx: cx.clone(),
            })
        })
        .collect();
    let target = Cx {
        module: resolver.items[index].module,
        params: Rc::new(zip(&alias.generics, &args)),
        self_item: None,
        owner: Rc::from(resolver.path_of(index).as_str()),
        aliases: cx.aliases + 1,
    };
    type_text(resolver, &ty, &target)
}

/// Makes every lifetime the type `ty` writes `'static`, in the forms
/// [`type_text`] names.
fn static_lifetimes(ty: &mut Type) {
    match ty {
        Type::Paren(inner) => static_lifetimes(&mut inner.elem),
        Type::Group(inner) => static_lifetimes(&mut inner.elem),
        Type::Reference(reference) => {
            if let Some(lifetime) = &mut reference.lifetime {
                lifetime.ident = proc_macro2::Ident::new("static", lifetime.ident.span());
            }
            static_lifetimes(&mut reference.elem);
        }
        Type::Slice(slice) => static_lifetimes(&mut slice.elem),
        Type::Array(array) => static_lifetimes(&mut array.elem),
        Type::Tuple(tuple) => tuple.elems.iter_mut().for_each(static_lifetimes),
        Type::Path(path) => {
            for segment in &mut path.path.segments {
                let PathArguments::AngleBracketed(arguments) = &mut segment.arguments else {
                    continue;
                };
                for argument in &mut arguments.args {
                    match argument {
                        GenericArgument::Lifetime(lifetime) => {
                            lifetime.ident =
                                proc_macro2::Ident::new("static", lifetime.ident.span());
                        }
                        GenericArgument::Type(inner) => static_lifetimes(inner),
                        _ => {}
                    }
                }
            }
        }
        _ => {}
    }
}
