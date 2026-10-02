// SPDX-License-Identifier: MIT
/*
 * aosabi-build: the types the roots of a crate reach through its plain
 * source, and what the reader cannot resolve.
 */

//! The reach of the roots of a crate.
//!
//! From each root the walk lists every struct, enum and inherent impl block it
//! reaches, depth first in the order of the source, through the public fields
//! of a struct, the payloads of the variants of an enum, and the parameters and
//! the return types of the public methods with a receiver of every inherent
//! block of a struct, which are its fields. A field or a method without `pub`
//! is hidden, as on the derive path, as is one of a type of a path dependency
//! without `pub` alone, which another crate does not see, and a member whose
//! name opens with two underscores, `__typename` among them, is no field, since
//! the inference removes it. A struct of fields by position, a variant of named
//! fields or of several fields, and an enum that mixes unit variants with
//! variants of a value have no GraphQL form, as the derive path refuses them,
//! and the walk lists each and goes no further into it. A generic type is
//! walked once per concrete use, its parameters substituted by the arguments of
//! the use, and a lifetime is ignored. An inherent block belongs to the struct
//! its self type names through every type alias, and to the uses of a generic
//! struct whose arguments it matches: a block over bare type parameters,
//! `impl<T> Page<T>`, to every use, and a block that fixes an argument,
//! `impl Page<User>`, to the uses with that argument alone. A type alias of the crate
//! is followed to the type it names. A type of a path dependency is walked as a
//! type of the crate, since the reader reads its source. A name of `std`,
//! `core`, `alloc` or the facade is mapped by the tables of `resolve`: a scalar
//! or a JSON value ends the walk, and a wrapper such as `Option`, `Vec`, `Box`
//! or `Result` goes on into the type it holds.
//!
//! Every type the walk cannot resolve is listed with the site that writes it
//! and the reason, one of [`Reason`]: a type of a crate whose source the
//! reader does not read, a generic it cannot substitute, an inherent block
//! whose uses it cannot tell among them, a trait object, an associated type, a
//! macro expansion, a type alias chain it stops following, a type with no
//! GraphQL form, or a name no item declares.
//!
//! Beside the walk the reach names every member it meets by the rule of
//! [`graphql_name`](super::names::graphql_name), keeps its doc text and its
//! line, and names every struct a parameter takes as its input by
//! [`input_name`](super::names::input_name).

use std::{
    fmt,
    rc::Rc, //
};

use syn::{
    Type,
    TypeParamBound, //
};

use super::{
    Place,
    Registration,
    RootKind,
    entry::type_arguments,
    model::{
        Data,
        Docs,
        Enumeration,
        Impl,
        Kind,
        Shape, //
    },
    names::{
        NameKind,
        graphql_name,
        input_name, //
    },
    resolve::{
        Glob,
        GlobOf,
        Known,
        Miss,
        Ns,
        Res,
        Resolver,
        facade_role,
        std_role, //
    }, //
};

/// The number of steps the walk follows a chain of type aliases.
pub(crate) const ALIAS_DEPTH: usize = 16;

/// The number of concrete uses of one generic type the walk takes.
pub(crate) const INSTANTIATIONS: usize = 16;

/// Where the reach of a crate starts.
#[derive(Debug, Clone, Default)]
pub struct Start {
    /// Whether the reach starts from the parts of the roots the entry
    /// registers.
    pub registered: bool,
    /// Whether the reach starts from every type the annotations of the
    /// derive path describe, a derive of `Aosabi`, `#[aosabi::scalar]` or
    /// `#[aosabi::resolvers]`, the start of a twin whose roots a fragment of
    /// `graphql!` holds.
    pub described: bool,
    /// The names of types to start from, such as the types of the IR of a
    /// golden, each matched to a struct or an enum of the crate of that name.
    pub names: Vec<String>,
}

/// Where a root of the reach comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootFrom {
    /// A part of a root the entry registers.
    Registered(RootKind),
    /// A type an annotation of the derive path describes.
    Described,
    /// A type of the names of [`Start`].
    Named,
}

impl RootFrom {
    /// Returns the word that names the origin in a report.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Registered(kind) => kind.as_str(),
            Self::Described => "described",
            Self::Named => "named",
        }
    }
}

/// One root of the reach.
#[derive(Debug, Clone)]
pub struct Root {
    /// The path of the type from the crate root.
    pub path: String,
    /// Where the root comes from.
    pub from: RootFrom,
    /// The place of the registration, or of the type.
    pub place: Place,
}

/// The kind of a reached item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReachedKind {
    /// A struct.
    Struct,
    /// An enum.
    Enum,
    /// An inherent impl block of a reached struct.
    Impl,
}

impl ReachedKind {
    /// Returns the word that names the kind in a report.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Struct => "struct",
            Self::Enum => "enum",
            Self::Impl => "impl",
        }
    }
}

/// One reached item.
#[derive(Debug, Clone)]
pub struct Reached {
    /// Its kind.
    pub kind: ReachedKind,
    /// The path of the type from the crate root, the type an impl block is
    /// for.
    pub path: String,
    /// The place of its name, or of the keyword of an impl block.
    pub place: Place,
}

/// One named site of a reached item: the type, a field, a method, a
/// parameter or a variant.
#[derive(Debug, Clone)]
pub struct Member {
    /// The key of the site: `Type`, `Type.field`, `Type.method`,
    /// `Type.method(param)` or `Enum.Variant`, with the path of the type from
    /// the crate root and every name as Rust writes it.
    pub key: String,
    /// The kind of the name.
    pub kind: NameKind,
    /// The name the rule of [`graphql_name`] gives it.
    pub graphql: String,
    /// The place of its name.
    pub place: Place,
    /// Its doc text, one entry per `///` line, `/** */` block or `#[doc]`.
    pub docs: Vec<String>,
    /// Whether it is an `async fn`, whose answer is a promise.
    pub asynchronous: bool,
}

/// The name of a struct that a parameter takes as its input.
#[derive(Debug, Clone)]
pub struct InputName {
    /// The key of the parameter, `Type.method(param)`.
    pub key: String,
    /// The path of the struct from the crate root.
    pub path: String,
    /// The name [`input_name`] gives the struct at that site.
    pub graphql: String,
}

/// Why the reader cannot resolve a type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// A type of a crate whose source the reader does not read, a crate of a
    /// registry or of git, or a name of `std`, `core`, `alloc` or the facade
    /// with no GraphQL form.
    OtherCrate,
    /// A type parameter no concrete use substitutes, or an inherent block of
    /// a generic struct whose uses the reader cannot tell.
    Generic,
    /// A trait object, `dyn Trait` or `impl Trait`, which names a trait and
    /// no type.
    TraitObject,
    /// An associated type, which the compiler resolves.
    Associated,
    /// A type or a doc text a macro expands.
    Macro,
    /// A chain of type aliases the reader stops following.
    AliasChain,
    /// A tuple, a function pointer, a raw pointer, a union or another type
    /// with no GraphQL form.
    Shape,
    /// A name no item of the crate declares and no macro or glob explains.
    Unknown,
}

impl Reason {
    /// Every reason, in the order of a report.
    pub const ALL: [Self; 8] = [
        Self::OtherCrate,
        Self::Generic,
        Self::TraitObject,
        Self::Associated,
        Self::Macro,
        Self::AliasChain,
        Self::Shape,
        Self::Unknown,
    ];

    /// Returns the words that name the reason in a report.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OtherCrate => "a type of another crate",
            Self::Generic => "a generic it cannot substitute",
            Self::TraitObject => "a trait object",
            Self::Associated => "an associated type",
            Self::Macro => "a macro expansion",
            Self::AliasChain => "a type alias chain",
            Self::Shape => "a type with no GraphQL form",
            Self::Unknown => "a name no item declares",
        }
    }
}

impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One type the reader cannot resolve.
#[derive(Debug, Clone)]
pub struct Unresolved {
    /// The key of the site that writes it, as [`Member::key`].
    pub site: String,
    /// The type as the source writes it.
    pub ty: String,
    /// Why the reader cannot resolve it.
    pub reason: Reason,
    /// What the reader found, in one sentence.
    pub detail: String,
    /// The place of the site.
    pub place: Place,
}

/// What the roots of a crate reach.
#[derive(Debug, Clone, Default)]
pub struct Reach {
    /// The roots, each once, in the order of the start.
    pub roots: Vec<Root>,
    /// The names of [`Start`] no struct or enum of the crate declares.
    pub unmatched: Vec<String>,
    /// Every reached struct, enum and inherent impl block, in the order the
    /// walk reaches them.
    pub reached: Vec<Reached>,
    /// Every named site of the reached items.
    pub members: Vec<Member>,
    /// Every struct a parameter takes as its input, with its name.
    pub inputs: Vec<InputName>,
    /// Every type the reader cannot resolve.
    pub unresolved: Vec<Unresolved>,
}

/// The context a type is written in: its module, the type parameters in
/// scope with what substitutes them, and what `Self` names.
#[derive(Clone)]
pub(crate) struct Cx {
    /// The module that writes the type.
    pub(crate) module: usize,
    /// The type parameters in scope, each with its argument when a concrete
    /// use gives one.
    pub(crate) params: Rc<Params>,
    /// The item `Self` names with its arguments.
    pub(crate) self_item: Option<(usize, Rc<Vec<Option<Arg>>>)>,
    /// The type parameters in scope belong to this item.
    pub(crate) owner: Rc<str>,
    /// How many type aliases the walk follows to reach this type.
    pub(crate) aliases: usize,
}

/// The type parameters in scope, each with its argument when a concrete use
/// gives one.
pub(crate) type Params = Vec<(String, Option<Arg>)>;

/// A type argument, with the context of the use that writes it.
#[derive(Clone)]
pub(crate) struct Arg {
    /// The type as the use writes it.
    pub(crate) ty: Type,
    /// The context of the use.
    pub(crate) cx: Cx,
}

/// A site that writes a type.
struct Site {
    /// The key of the site, as [`Member::key`].
    key: String,
    /// The place of the site.
    place: Place,
}

/// The state of the walk.
struct Walk<'a> {
    /// The resolver of the crate.
    resolver: &'a Resolver<'a>,
    /// The inherent impl blocks of each item.
    impls: Vec<Vec<Block>>,
    /// The macro calls at the level of items, with their places and their
    /// libraries.
    macros: Vec<(String, Place, usize)>,
    /// Whether the walk listed each impl block, which it lists the first time
    /// the block belongs to a use it walks.
    listed: Vec<bool>,
    /// Every item with the text of its arguments the walk took.
    seen: Vec<(usize, String)>,
    /// Whether the walk reached each item.
    reached: Vec<bool>,
    /// What the walk found so far.
    reach: Reach,
}

/// Returns the reach of the crate the resolver holds from `start`, with the
/// registrations of its entry.
pub(crate) fn reach(
    resolver: &Resolver<'_>,
    registrations: &[Registration],
    start: &Start,
) -> Reach {
    let mut walk = Walk {
        resolver,
        impls: blocks(resolver),
        macros: macro_calls(resolver),
        listed: vec![false; resolver.items.len()],
        seen: Vec::new(),
        reached: vec![false; resolver.items.len()],
        reach: Reach::default(),
    };

    let mut roots: Vec<usize> = Vec::new();
    if start.registered {
        for registration in registrations {
            walk.root(
                &mut roots,
                registration.item,
                RootFrom::Registered(registration.kind),
                &registration.place,
            );
        }
    }
    if start.described {
        for (index, item) in resolver.items.iter().enumerate() {
            if !item.described || resolver.krate(index) != 0 {
                continue;
            }
            match &item.kind {
                Kind::Struct(_) | Kind::Enum(_) => {
                    walk.root(&mut roots, index, RootFrom::Described, &item.place);
                }
                Kind::Impl(block) => {
                    if let Some((owner, _)) = self_of(resolver, block, item.module) {
                        walk.root(&mut roots, owner, RootFrom::Described, &item.place);
                    }
                }
                _ => {}
            }
        }
    }
    for name in &start.names {
        let declared = resolver.items.iter().enumerate().position(|(index, item)| {
            item.name == *name
                && matches!(item.kind, Kind::Struct(_) | Kind::Enum(_))
                && resolver.krate(index) == 0
        });
        match declared {
            Some(index) => {
                let place = resolver.items[index].place.clone();
                walk.root(&mut roots, index, RootFrom::Named, &place);
            }
            None => walk.reach.unmatched.push(name.clone()),
        }
    }

    // A generic root is walked through its concrete uses first, and a root
    // no use reaches is walked with its parameters unsubstituted, which
    // lists each of them where a member writes it.
    for &root in &roots {
        if walk.generics(root).is_empty() {
            walk.item(root, Vec::new());
        }
    }
    for &root in &roots {
        let count = walk.generics(root).len();
        if count > 0 && !walk.reached[root] {
            walk.item(root, vec![None; count]);
        }
    }
    walk.reach
}

impl Walk<'_> {
    /// Adds the root `index` from `from` at `place`, once.
    fn root(&mut self, roots: &mut Vec<usize>, index: usize, from: RootFrom, place: &Place) {
        if roots.contains(&index) {
            return;
        }
        roots.push(index);
        self.reach.roots.push(Root {
            path: self.path_of(index),
            from,
            place: place.clone(),
        });
    }

    /// Returns the type parameters of the struct or the enum `index`.
    fn generics(&self, index: usize) -> &[String] {
        match &self.resolver.items[index].kind {
            Kind::Struct(data) => &data.generics,
            Kind::Enum(variants) => &variants.generics,
            _ => &[],
        }
    }

    /// Returns the path of the item `index` from the crate root.
    fn path_of(&self, index: usize) -> String {
        self.resolver.path_of(index)
    }

    /// Walks the struct or the enum `index` with the arguments `args`.
    fn item(&mut self, index: usize, args: Vec<Option<Arg>>) {
        let text = args
            .iter()
            .map(|arg| {
                arg.as_ref()
                    .map_or_else(|| String::from("_"), |arg| type_text(&arg.ty))
            })
            .collect::<Vec<_>>()
            .join(", ");
        if self
            .seen
            .iter()
            .any(|(seen, seen_text)| *seen == index && *seen_text == text)
        {
            return;
        }
        let resolver = self.resolver;
        let item = &resolver.items[index];
        let path = self.path_of(index);
        if self.seen.iter().filter(|(seen, _)| *seen == index).count() >= INSTANTIATIONS {
            let site = Site {
                key: path.clone(),
                place: item.place.clone(),
            };
            let detail = format!(
                "`{path}` takes more than {INSTANTIATIONS} concrete uses, and the reader stops substituting it"
            );
            self.unresolved(&site, &format!("{path}<{text}>"), Reason::Generic, detail);
            return;
        }
        self.seen.push((index, text));
        let first = !self.reached[index];
        self.reached[index] = true;
        let args = Rc::new(args);
        match &item.kind {
            Kind::Struct(data) => self.structure(index, data, &path, &args, first),
            Kind::Enum(variants) => self.enumeration(index, variants, &path, &args, first),
            _ => {}
        }
    }

    /// Adds the reached struct or enum `index` of the kind `kind` at `path`
    /// and names it as a type.
    fn reached(&mut self, index: usize, kind: ReachedKind, path: &str) {
        let item = &self.resolver.items[index];
        self.reach.reached.push(Reached {
            kind,
            path: String::from(path),
            place: item.place.clone(),
        });
        self.member(
            String::from(path),
            NameKind::Type,
            &item.name,
            &item.place,
            &item.docs,
        );
    }

    /// Walks the struct `index` at `path`, whose fields are `data`, with the
    /// arguments `args`, naming its members when `first`.
    fn structure(
        &mut self,
        index: usize,
        data: &Data,
        path: &str,
        args: &Rc<Vec<Option<Arg>>>,
        first: bool,
    ) {
        let item = &self.resolver.items[index];
        if first {
            self.reached(index, ReachedKind::Struct, path);
        }
        if data.shape == Shape::Tuple && !data.fields.is_empty() {
            let site = Site {
                key: String::from(path),
                place: item.place.clone(),
            };
            let detail = format!(
                "`{path}` is a struct of fields by position, which has no GraphQL form of an object, and the derive path describes such a newtype only as a scalar"
            );
            self.unresolved(&site, path, Reason::Shape, detail);
            return;
        }
        let cx = Cx {
            module: item.module,
            params: Rc::new(zip(&data.generics, args)),
            self_item: Some((index, Rc::clone(args))),
            owner: Rc::from(path),
            aliases: 0,
        };
        for field in &data.fields {
            if !self.resolver.exposes(index, &field.vis) || forbidden(&field.name) {
                continue;
            }
            let key = format!("{path}.{}", field.name);
            if first {
                self.member(
                    key.clone(),
                    NameKind::Field,
                    &field.name,
                    &field.place,
                    &field.docs,
                );
            }
            let site = Site {
                key,
                place: field.place.clone(),
            };
            self.ty(&field.ty, &cx, &site);
        }
        let blocks = self.impls[index].clone();
        for block in &blocks {
            self.block(block, index, path, args);
        }
    }

    /// Walks the enum `index` at `path`, whose variants are `variants`, with
    /// the arguments `args`, naming its members when `first`.
    fn enumeration(
        &mut self,
        index: usize,
        variants: &Enumeration,
        path: &str,
        args: &Rc<Vec<Option<Arg>>>,
        first: bool,
    ) {
        let item = &self.resolver.items[index];
        if first {
            self.reached(index, ReachedKind::Enum, path);
        }
        let units = variants
            .variants
            .iter()
            .filter(|variant| variant.shape == Shape::Unit)
            .count();
        if units > 0 && units < variants.variants.len() {
            let site = Site {
                key: String::from(path),
                place: item.place.clone(),
            };
            let detail = format!(
                "`{path}` mixes unit variants and variants that hold a value, and a GraphQL enum holds unit variants alone and a union variants of one type each"
            );
            self.unresolved(&site, path, Reason::Shape, detail);
        }
        let cx = Cx {
            module: item.module,
            params: Rc::new(zip(&variants.generics, args)),
            self_item: Some((index, Rc::clone(args))),
            owner: Rc::from(path),
            aliases: 0,
        };
        for variant in &variants.variants {
            let key = format!("{path}.{}", variant.name);
            if first {
                self.member(
                    key.clone(),
                    NameKind::Variant,
                    &variant.name,
                    &variant.place,
                    &variant.docs,
                );
            }
            if variant.shape == Shape::Named || variant.fields.len() > 1 {
                let site = Site {
                    key: key.clone(),
                    place: variant.place.clone(),
                };
                let detail = format!(
                    "`{key}` holds named fields or several fields, and a variant of a GraphQL union holds one type"
                );
                self.unresolved(&site, &key, Reason::Shape, detail);
                continue;
            }
            for field in &variant.fields {
                let site = Site {
                    key: key.clone(),
                    place: field.place.clone(),
                };
                self.ty(&field.ty, &cx, &site);
            }
        }
    }

    /// Walks the inherent impl block `block` of the struct `owner` at `path`
    /// with the arguments `args` when it belongs to that use, naming its
    /// members the first time it does, and lists a block whose uses the
    /// reader cannot tell once.
    fn block(&mut self, block: &Block, owner: usize, path: &str, args: &Rc<Vec<Option<Arg>>>) {
        let resolver = self.resolver;
        let item = &resolver.items[block.item];
        let Kind::Impl(imp) = &item.kind else {
            return;
        };
        let first = !self.listed[block.item];
        let bound = match applies(resolver, block, owner, args) {
            Ok(Some(bound)) => bound,
            Ok(None) => return,
            Err(unmatched) => {
                if first {
                    self.unmatched(block.item, path, &unmatched);
                }
                return;
            }
        };
        self.listed[block.item] = true;
        if first {
            self.reach.reached.push(Reached {
                kind: ReachedKind::Impl,
                path: String::from(path),
                place: item.place.clone(),
            });
        }
        for method in &imp.methods {
            if !resolver.exposes(owner, &method.vis)
                || method.function.receiver.is_none()
                || forbidden(&method.name)
            {
                continue;
            }
            let mut params = bound.clone();
            params.extend(
                method
                    .function
                    .generics
                    .iter()
                    .map(|name| (name.clone(), None)),
            );
            let cx = Cx {
                module: item.module,
                params: Rc::new(params),
                self_item: Some((owner, Rc::clone(args))),
                owner: Rc::from(format!("{path}.{}", method.name).as_str()),
                aliases: 0,
            };
            let key = format!("{path}.{}", method.name);
            if first {
                self.member(
                    key.clone(),
                    NameKind::Method,
                    &method.name,
                    &method.place,
                    &method.docs,
                );
                if let Some(member) = self.reach.members.last_mut() {
                    member.asynchronous = method.function.asyncness;
                }
            }
            for param in &method.function.params {
                let param_key = format!("{key}({})", param.name);
                if first && param.named {
                    self.member(
                        param_key.clone(),
                        NameKind::Argument,
                        &param.name,
                        &param.place,
                        &Docs::default(),
                    );
                    if let Some(input) = self.input(&param.ty, &cx, 0) {
                        let field = graphql_name(NameKind::Method, &method.name);
                        let argument = graphql_name(NameKind::Argument, &param.name);
                        self.reach.inputs.push(InputName {
                            key: param_key.clone(),
                            path: self.path_of(input),
                            graphql: input_name(&field, &argument),
                        });
                    }
                }
                let site = Site {
                    key: param_key,
                    place: param.place.clone(),
                };
                self.ty(&param.ty, &cx, &site);
            }
            if let Some(output) = &method.function.output {
                let site = Site {
                    key,
                    place: method.place.clone(),
                };
                self.ty(output, &cx, &site);
            }
        }
    }

    /// Lists the inherent block `block` of the struct at `path` once, whose
    /// uses the reader cannot tell for the reason `unmatched`.
    ///
    /// Each block is listed apart from the others, so two blocks of one self
    /// type are two entries.
    fn unmatched(&mut self, block: usize, path: &str, unmatched: &Unmatched) {
        self.listed[block] = true;
        let item = &self.resolver.items[block];
        let Kind::Impl(imp) = &item.kind else {
            return;
        };
        let ty = type_text(&imp.self_ty);
        self.reach.unresolved.push(Unresolved {
            site: String::from(path),
            detail: unmatched.detail(&ty, path, "reader"),
            ty,
            reason: Reason::Generic,
            place: item.place.clone(),
        });
    }

    /// Adds the member `key` of the kind `kind`, named `name` in Rust, with
    /// its place and its doc text, and lists a doc text a macro expands.
    fn member(&mut self, key: String, kind: NameKind, name: &str, place: &Place, docs: &Docs) {
        if let Some(unread) = &docs.unread {
            let site = Site {
                key: key.clone(),
                place: unread.clone(),
            };
            let detail =
                String::from("its doc text is the value of a macro, which the reader cannot read");
            self.unresolved(&site, "#[doc = ..]", Reason::Macro, detail);
        }
        self.reach.members.push(Member {
            key,
            kind,
            graphql: graphql_name(kind, name),
            place: place.clone(),
            docs: docs.lines.clone(),
            asynchronous: false,
        });
    }

    /// Adds one unresolved type at `site`, once.
    fn unresolved(&mut self, site: &Site, ty: &str, reason: Reason, detail: String) {
        let known = self
            .reach
            .unresolved
            .iter()
            .any(|seen| seen.site == site.key && seen.ty == ty && seen.reason == reason);
        if known {
            return;
        }
        self.reach.unresolved.push(Unresolved {
            site: site.key.clone(),
            ty: String::from(ty),
            reason,
            detail,
            place: site.place.clone(),
        });
    }

    /// Walks the type `ty`, written in the context `cx` at `site`.
    fn ty(&mut self, ty: &Type, cx: &Cx, site: &Site) {
        match ty {
            Type::Paren(inner) => self.ty(&inner.elem, cx, site),
            Type::Group(inner) => self.ty(&inner.elem, cx, site),
            Type::Reference(inner) => self.ty(&inner.elem, cx, site),
            Type::Slice(inner) => self.ty(&inner.elem, cx, site),
            Type::Array(inner) => self.ty(&inner.elem, cx, site),
            Type::Never(_) => {}
            Type::Tuple(tuple) if tuple.elems.is_empty() => {}
            Type::Path(path) => self.path(path, ty, cx, site),
            Type::ImplTrait(bounds) => {
                // A subscription method answers `impl Stream<Item = T>`, which
                // reaches what its items reach.
                if let Some(item) = self.resolver.stream_item(cx.module, &bounds.bounds) {
                    self.ty(item, cx, site);
                    return;
                }
                let text = type_text(ty);
                let detail = format!("`{text}` names a trait and no type");
                self.unresolved(site, &text, Reason::TraitObject, detail);
            }
            Type::TraitObject(_) => {
                let text = type_text(ty);
                let detail = format!("`{text}` names a trait and no type");
                self.unresolved(site, &text, Reason::TraitObject, detail);
            }
            Type::Macro(_) => {
                let text = type_text(ty);
                let detail = format!("`{text}` is a type a macro expands");
                self.unresolved(site, &text, Reason::Macro, detail);
            }
            _ => {
                let text = type_text(ty);
                let form = match ty {
                    Type::Tuple(_) => "a tuple",
                    Type::BareFn(_) => "a function pointer",
                    Type::Ptr(_) => "a raw pointer",
                    Type::Infer(_) => "a type the compiler infers",
                    _ => "a form of a type",
                };
                let detail = format!("`{text}` is {form}, which has no GraphQL form");
                self.unresolved(site, &text, Reason::Shape, detail);
            }
        }
    }

    /// Walks the path type `path`, which is `ty`, written in `cx` at `site`.
    fn path(&mut self, path: &syn::TypePath, ty: &Type, cx: &Cx, site: &Site) {
        let text = type_text(ty);
        if path.qself.is_some() {
            let detail = format!("`{text}` is an associated type, which the compiler resolves");
            self.unresolved(site, &text, Reason::Associated, detail);
            return;
        }
        let segments = Resolver::segments(&path.path);
        let leading = path.path.leading_colon.is_some();
        if !leading
            && segments.len() == 1
            && type_arguments(&path.path).is_empty()
            && let Some((name, arg)) = cx.params.iter().find(|(name, _)| *name == segments[0])
        {
            if let Some(arg) = arg {
                let arg = arg.clone();
                self.ty(&arg.ty, &arg.cx, site);
            } else {
                let detail = format!(
                    "`{name}` is a type parameter of `{}` that no concrete use substitutes",
                    cx.owner
                );
                self.unresolved(site, &text, Reason::Generic, detail);
            }
            return;
        }
        if segments.first().is_some_and(|first| first == "Self") && !leading {
            if segments.len() > 1 {
                let detail = format!("`{text}` is an associated type, which the compiler resolves");
                self.unresolved(site, &text, Reason::Associated, detail);
            } else if let Some((owner, args)) = &cx.self_item {
                let args = args.as_ref().clone();
                self.item(*owner, args);
            }
            return;
        }
        let arguments: Vec<Arg> = type_arguments(&path.path)
            .into_iter()
            .map(|argument| Arg {
                ty: argument.clone(),
                cx: cx.clone(),
            })
            .collect();
        match self
            .resolver
            .resolve(cx.module, leading, &segments, Ns::Type)
        {
            Ok(Res::Item(index)) => self.named(index, arguments, &text, cx, site),
            Ok(Res::Std(named)) => match std_role(&named) {
                Known::Wrap(position) => self.wrapped(&arguments, position, site),
                Known::Leaf => {}
                _ => {
                    let detail =
                        format!("`{text}` is a type of the standard library with no GraphQL form");
                    self.unresolved(site, &text, Reason::OtherCrate, detail);
                }
            },
            Ok(Res::Facade(named)) => match facade_role(&named) {
                Known::Wrap(position) => self.wrapped(&arguments, position, site),
                Known::Leaf => {}
                _ => {
                    let detail = format!("`{text}` is a name of the facade with no GraphQL form");
                    self.unresolved(site, &text, Reason::OtherCrate, detail);
                }
            },
            Ok(Res::Prim(_)) => {}
            Ok(Res::Extern(named)) => {
                let detail = format!(
                    "`{}` is a type of the crate `{}`, which the manifest names with no path, a crate of a registry or of git, whose source the reader does not read",
                    named.join("::"),
                    named.first().map_or("", String::as_str)
                );
                self.unresolved(site, &text, Reason::OtherCrate, detail);
            }
            Ok(Res::Module(_) | Res::Variant(..)) => {
                let detail = format!("`{text}` names no type");
                self.unresolved(site, &text, Reason::Unknown, detail);
            }
            Err(Miss::Associated) => {
                let detail = format!("`{text}` is an associated type, which the compiler resolves");
                self.unresolved(site, &text, Reason::Associated, detail);
            }
            Err(Miss::Unknown { name, globs }) => {
                self.unknown(&name, &globs, &text, site, cx.module);
            }
        }
    }

    /// Walks the item `index` a type `text` names with the arguments
    /// `arguments`, written in `cx` at `site`.
    fn named(&mut self, index: usize, arguments: Vec<Arg>, text: &str, cx: &Cx, site: &Site) {
        let resolver = self.resolver;
        let item = &resolver.items[index];
        match &item.kind {
            Kind::Struct(_) | Kind::Enum(_) => {
                let count = self.generics(index).len();
                let mut args: Vec<Option<Arg>> = arguments.into_iter().map(Some).collect();
                args.resize(count, None);
                self.item(index, args);
            }
            Kind::Alias(alias) => {
                if cx.aliases >= ALIAS_DEPTH {
                    let detail = format!(
                        "the type the site writes leads through more than {ALIAS_DEPTH} type aliases, and the reader stops at the alias `{text}` after it follows {ALIAS_DEPTH} of them"
                    );
                    self.unresolved(site, text, Reason::AliasChain, detail);
                    return;
                }
                let args: Vec<Option<Arg>> = arguments.into_iter().map(Some).collect();
                let target = Cx {
                    module: item.module,
                    params: Rc::new(zip(&alias.generics, &args)),
                    self_item: None,
                    owner: Rc::from(self.path_of(index).as_str()),
                    aliases: cx.aliases + 1,
                };
                self.ty(&alias.ty, &target, site);
            }
            Kind::Trait => {
                let detail = format!("`{text}` names the trait `{}` and no type", item.name);
                self.unresolved(site, text, Reason::TraitObject, detail);
            }
            Kind::Union => {
                let detail = format!("`{text}` is a union, which has no GraphQL form");
                self.unresolved(site, text, Reason::Shape, detail);
            }
            _ => {
                let detail = format!("`{text}` names no type");
                self.unresolved(site, text, Reason::Unknown, detail);
            }
        }
    }

    /// Walks the type argument at `position` of a wrapper, which holds the
    /// value a resolver answers.
    fn wrapped(&mut self, arguments: &[Arg], position: usize, site: &Site) {
        if let Some(arg) = arguments.get(position) {
            self.ty(&arg.ty, &arg.cx, site);
        }
    }

    /// Lists the name `name` of the type `text` at `site` in the module
    /// `module`, which no item of its crate declares, with what may make it:
    /// each glob in scope of a crate whose names the reader does not hold
    /// whole, or a macro call of the crate.
    fn unknown(&mut self, name: &str, globs: &[Glob], text: &str, site: &Site, module: usize) {
        if !globs.is_empty() {
            let named: Vec<String> = globs
                .iter()
                .map(|glob| {
                    let of = match glob.of {
                        GlobOf::Std => "the standard library",
                        GlobOf::Facade => "the facade",
                        GlobOf::Other => "another crate",
                    };
                    format!("`{}` of {of} at {}", glob.path, glob.place)
                })
                .collect();
            let detail = format!(
                "`{name}` names nothing the crate declares, and a glob may import it, {}",
                named.join(" or ")
            );
            self.unresolved(site, text, Reason::OtherCrate, detail);
            return;
        }
        let krate = self.resolver.modules[module].krate;
        let calls: Vec<String> = self
            .macros
            .iter()
            .filter(|(_, _, of)| *of == krate)
            .map(|(path, place, _)| format!("{path}! at {place}"))
            .collect();
        if !calls.is_empty() {
            let detail = format!(
                "`{name}` names nothing the crate declares, beside the macro calls {}, whose items the reader cannot see",
                calls.join(", ")
            );
            self.unresolved(site, text, Reason::Macro, detail);
            return;
        }
        let detail = format!("`{name}` names nothing the crate declares");
        self.unresolved(site, text, Reason::Unknown, detail);
    }

    /// Returns the struct of the crate a parameter of the type `ty` takes as
    /// its input, through references and the wrappers of a value.
    fn input(&self, ty: &Type, cx: &Cx, depth: usize) -> Option<usize> {
        if depth > ALIAS_DEPTH {
            return None;
        }
        match ty {
            Type::Paren(inner) => self.input(&inner.elem, cx, depth + 1),
            Type::Group(inner) => self.input(&inner.elem, cx, depth + 1),
            Type::Reference(inner) => self.input(&inner.elem, cx, depth + 1),
            Type::Slice(inner) => self.input(&inner.elem, cx, depth + 1),
            Type::Array(inner) => self.input(&inner.elem, cx, depth + 1),
            Type::Path(path) if path.qself.is_none() => {
                let segments = Resolver::segments(&path.path);
                let leading = path.path.leading_colon.is_some();
                if !leading
                    && segments.len() == 1
                    && let Some((_, arg)) = cx.params.iter().find(|(name, _)| *name == segments[0])
                {
                    return arg
                        .as_ref()
                        .and_then(|arg| self.input(&arg.ty, &arg.cx, depth + 1));
                }
                let inner = |position: usize| {
                    type_arguments(&path.path)
                        .get(position)
                        .and_then(|argument| self.input(argument, cx, depth + 1))
                };
                match self
                    .resolver
                    .resolve(cx.module, leading, &segments, Ns::Type)
                    .ok()?
                {
                    Res::Item(index) => match &self.resolver.items[index].kind {
                        Kind::Struct(_) => Some(index),
                        _ => None,
                    },
                    Res::Std(named) => match std_role(&named) {
                        Known::Wrap(position) => inner(position),
                        _ => None,
                    },
                    Res::Facade(named) => match facade_role(&named) {
                        Known::Wrap(position) => inner(position),
                        _ => None,
                    },
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

/// Returns whether the member `name` is no field of the schema, a name that
/// opens with two underscores, such as `__typename`, which the inference
/// removes from every type it builds.
pub(crate) fn forbidden(name: &str) -> bool {
    name.starts_with("__")
}

/// Returns the type parameters `names` with the arguments `args`, a missing
/// argument left unsubstituted.
pub(crate) fn zip(names: &[String], args: &[Option<Arg>]) -> Params {
    names
        .iter()
        .enumerate()
        .map(|(position, name)| (name.clone(), args.get(position).cloned().flatten()))
        .collect()
}

/// Returns every macro call at the level of the items of every crate the
/// resolver holds, with its path, its place and its library.
pub(crate) fn macro_calls(resolver: &Resolver<'_>) -> Vec<(String, Place, usize)> {
    resolver
        .items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| match &item.kind {
            Kind::Macro(call) if call.defines.is_none() => {
                Some((call.path.clone(), item.place.clone(), resolver.krate(index)))
            }
            _ => None,
        })
        .collect()
}

/// An inherent impl block with the type arguments its self type writes.
#[derive(Clone)]
pub(crate) struct Block {
    /// The impl block, as an index into the items.
    pub(crate) item: usize,
    /// The type arguments its self type writes for the struct or the enum it
    /// is for, read through the type aliases of the crate, each in the
    /// context that writes it, where a type parameter of the block stands
    /// with no argument.
    pub(crate) args: Vec<Arg>,
}

/// Why the reader cannot tell which uses of a generic struct an inherent
/// impl block belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Unmatched {
    /// The self type writes this argument, which holds a type parameter of
    /// the block inside another type, `Page<Vec<T>>`.
    Nested(String),
    /// The self type writes no argument for this type parameter of the
    /// struct, which a default fills.
    Missing(String),
    /// The block bounds this type parameter with this trait, which the
    /// struct does not bound its parameter with.
    Bound(String, String),
    /// The where clause of the block bounds a type that is no type parameter
    /// of the block.
    Other,
}

impl Unmatched {
    /// Returns what `who`, the reader or the builder, found of the block
    /// whose self type is `text` on the struct at `path`, in one sentence.
    pub(crate) fn detail(&self, text: &str, path: &str, who: &str) -> String {
        match self {
            Self::Nested(argument) => format!(
                "the impl block of `{text}` writes the argument `{argument}`, which holds a type parameter of the block inside another type, so the {who} cannot tell which uses of `{path}` it belongs to"
            ),
            Self::Missing(parameter) => format!(
                "the impl block of `{text}` writes no argument for the type parameter `{parameter}` of `{path}`, which a default fills and the {who} does not read"
            ),
            Self::Bound(parameter, bound) => format!(
                "the impl block of `{text}` bounds `{parameter}` with `{bound}`, which `{path}` does not bound its parameter with, so its methods belong to some uses of `{path}` alone"
            ),
            Self::Other => format!(
                "the impl block of `{text}` holds a where clause that bounds a type other than a type parameter of the block, which the {who} cannot check"
            ),
        }
    }
}

/// Returns the inherent impl blocks of every struct and enum the resolver
/// holds, by item, in the order of the walk.
pub(crate) fn blocks(resolver: &Resolver<'_>) -> Vec<Vec<Block>> {
    let mut blocks = vec![Vec::new(); resolver.items.len()];
    for (index, item) in resolver.items.iter().enumerate() {
        if let Kind::Impl(imp) = &item.kind
            && imp.trait_.is_none()
            && let Some((owner, args)) = self_of(resolver, imp, item.module)
        {
            blocks[owner].push(Block { item: index, args });
        }
    }
    blocks
}

/// Returns the struct or the enum of the crate the inherent impl block `imp`
/// of the module `module` is for, with the type arguments its self type
/// writes, through every type alias of the crate the self type names.
///
/// The block `impl Totals` of `type Totals = Stats;` is a block of `Stats`,
/// and `impl UserPage` of `type UserPage = Page<User>;` a block of `Page` that
/// fixes its argument to `User`.
pub(crate) fn self_of(
    resolver: &Resolver<'_>,
    imp: &Impl,
    module: usize,
) -> Option<(usize, Vec<Arg>)> {
    let mut cx = Cx {
        module,
        params: Rc::new(
            imp.generics
                .iter()
                .map(|name| (name.clone(), None))
                .collect(),
        ),
        self_item: None,
        owner: Rc::from(""),
        aliases: 0,
    };
    let mut ty = imp.self_ty.clone();
    for _ in 0..4 * ALIAS_DEPTH {
        let path = match ty {
            Type::Paren(inner) => {
                ty = *inner.elem;
                continue;
            }
            Type::Group(inner) => {
                ty = *inner.elem;
                continue;
            }
            Type::Path(path) if path.qself.is_none() => path,
            _ => return None,
        };
        let segments = Resolver::segments(&path.path);
        let leading = path.path.leading_colon.is_some();
        let written = type_arguments(&path.path);
        if !leading
            && segments.len() == 1
            && written.is_empty()
            && let Some((_, Some(arg))) = cx.params.iter().find(|(name, _)| *name == segments[0])
        {
            let arg = arg.clone();
            ty = arg.ty;
            cx = arg.cx;
            continue;
        }
        let arguments: Vec<Arg> = written
            .into_iter()
            .map(|argument| Arg {
                ty: argument.clone(),
                cx: cx.clone(),
            })
            .collect();
        let Ok(Res::Item(index)) = resolver.resolve(cx.module, leading, &segments, Ns::Type) else {
            return None;
        };
        match &resolver.items[index].kind {
            Kind::Struct(_) | Kind::Enum(_) => return Some((index, arguments)),
            Kind::Alias(alias) if cx.aliases < ALIAS_DEPTH => {
                let args: Vec<Option<Arg>> = arguments.into_iter().map(Some).collect();
                cx = Cx {
                    module: resolver.items[index].module,
                    params: Rc::new(zip(&alias.generics, &args)),
                    self_item: None,
                    owner: Rc::from(resolver.path_of(index).as_str()),
                    aliases: cx.aliases + 1,
                };
                ty = alias.ty.clone();
            }
            _ => return None,
        }
    }
    None
}

/// Returns the type parameters of the inherent impl block `block` of the
/// struct `owner` bound to the arguments `args` of one use of the struct, or
/// `None` when the block fixes an argument that use does not take.
///
/// A block over bare type parameters, `impl<T> Page<T>`, belongs to every
/// use and binds `T` to the argument of the use, a block that fixes an
/// argument, `impl Page<User>`, belongs to the uses whose argument is the
/// same type, and a parameter written twice, `impl<T> Pair<T, T>`, to the
/// uses whose two arguments are one type. A bound of a parameter of the block
/// holds for every use when the struct bounds the parameter of that place
/// with it as well, since rustc asks that bound of every use.
///
/// # Errors
///
/// Returns the [`Unmatched`] that says why the reader cannot tell which
/// uses the block belongs to, whatever the use.
pub(crate) fn applies(
    resolver: &Resolver<'_>,
    block: &Block,
    owner: usize,
    args: &[Option<Arg>],
) -> Result<Option<Params>, Unmatched> {
    let Kind::Impl(imp) = &resolver.items[block.item].kind else {
        return Ok(None);
    };
    let empty: &[(String, syn::TypeParamBound)] = &[];
    let (generics, bounds) = match &resolver.items[owner].kind {
        Kind::Struct(data) => (data.generics.as_slice(), data.bounds.as_slice()),
        Kind::Enum(variants) => (variants.generics.as_slice(), empty),
        _ => (&[][..], empty),
    };
    if imp.bounds_other {
        return Err(Unmatched::Other);
    }
    if let Some(missing) = generics.get(block.args.len()) {
        return Err(Unmatched::Missing(missing.clone()));
    }
    let written: Vec<Key> = block
        .args
        .iter()
        .take(generics.len())
        .map(|arg| key(resolver, &arg.ty, &arg.cx, 0))
        .collect();
    for (position, key) in written.iter().enumerate() {
        if key == &Key::Open {
            return Err(Unmatched::Nested(type_text(&block.args[position].ty)));
        }
    }
    for (parameter, bound) in &imp.bounds {
        let Some(text) = bound_text(bound) else {
            continue;
        };
        let places: Vec<&String> = written
            .iter()
            .zip(generics)
            .filter(|(key, _)| matches!(key, Key::Param(name) if name == parameter))
            .map(|(_, own)| own)
            .collect();
        let holds = places.iter().any(|own| {
            bounds
                .iter()
                .any(|(name, theirs)| name == *own && bound_text(theirs).as_deref() == Some(&text))
        });
        if !places.is_empty() && !holds {
            return Err(Unmatched::Bound(parameter.clone(), text));
        }
    }
    let mut bound: Params = imp
        .generics
        .iter()
        .map(|name| (name.clone(), None))
        .collect();
    let mut taken: Vec<(&str, Key)> = Vec::new();
    for (position, key_written) in written.iter().enumerate() {
        let arg = args.get(position).cloned().flatten();
        let used = arg
            .as_ref()
            .map_or(Key::Open, |arg| key(resolver, &arg.ty, &arg.cx, 0));
        match key_written {
            Key::Param(name) => {
                if let Some((_, first)) = taken.iter().find(|(known, _)| known == name) {
                    if !matches!(first, Key::Type(_)) || *first != used {
                        return Ok(None);
                    }
                } else {
                    if let Some(slot) = bound.iter_mut().find(|(known, _)| known == name) {
                        slot.1 = arg;
                    }
                    taken.push((name, used));
                }
            }
            Key::Type(_) if *key_written == used => {}
            _ => return Ok(None),
        }
    }
    Ok(Some(bound))
}

/// What an argument of a self type or of a use of a struct is, for the match
/// of an inherent block.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Key {
    /// A type parameter of the block, by its name.
    Param(String),
    /// A type, by the text of what it resolves to.
    Type(String),
    /// A type that holds a type parameter with no argument inside it, or that
    /// the reader cannot resolve.
    Open,
}

/// Returns the key of the type `ty` written in `cx`.
fn key(resolver: &Resolver<'_>, ty: &Type, cx: &Cx, depth: usize) -> Key {
    if depth <= ALIAS_DEPTH {
        match ty {
            Type::Paren(inner) => return key(resolver, &inner.elem, cx, depth + 1),
            Type::Group(inner) => return key(resolver, &inner.elem, cx, depth + 1),
            Type::Path(path) if path.qself.is_none() && path.path.leading_colon.is_none() => {
                let segments = Resolver::segments(&path.path);
                if segments.len() == 1
                    && type_arguments(&path.path).is_empty()
                    && let Some((name, arg)) =
                        cx.params.iter().find(|(name, _)| *name == segments[0])
                {
                    return match arg {
                        Some(arg) => key(resolver, &arg.ty, &arg.cx, depth + 1),
                        None => Key::Param(name.clone()),
                    };
                }
            }
            _ => {}
        }
    }
    identity(resolver, ty, cx, 0).map_or(Key::Open, Key::Type)
}

/// Returns the text of what the type `ty` written in `cx` resolves to, one
/// text for every spelling of one type, or `None` when a type parameter with
/// no argument stands in it or the reader cannot resolve it.
fn identity(resolver: &Resolver<'_>, ty: &Type, cx: &Cx, depth: usize) -> Option<String> {
    if depth > 4 * ALIAS_DEPTH {
        return None;
    }
    let inner = |ty: &Type| identity(resolver, ty, cx, depth + 1);
    match ty {
        Type::Paren(paren) => inner(&paren.elem),
        Type::Group(group) => inner(&group.elem),
        Type::Reference(reference) => {
            let unique = if reference.mutability.is_some() {
                "mut "
            } else {
                ""
            };
            Some(format!("&{unique}{}", inner(&reference.elem)?))
        }
        Type::Slice(slice) => Some(format!("[{}]", inner(&slice.elem)?)),
        Type::Array(array) => {
            let syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Int(length),
                ..
            }) = &array.len
            else {
                return None;
            };
            Some(format!(
                "[{}; {}]",
                inner(&array.elem)?,
                length.base10_digits()
            ))
        }
        Type::Tuple(tuple) => {
            let elems: Vec<&Type> = tuple.elems.iter().collect();
            Some(format!("({})", identities(resolver, &elems, cx, depth)?))
        }
        Type::Never(_) => Some(String::from("!")),
        Type::Path(path) if path.qself.is_none() => path_identity(resolver, path, cx, depth),
        _ => None,
    }
}

/// Returns the texts of what the types `types` written in `cx` resolve to,
/// joined by commas, as [`identity`] gives each.
fn identities(resolver: &Resolver<'_>, types: &[&Type], cx: &Cx, depth: usize) -> Option<String> {
    types
        .iter()
        .map(|ty| identity(resolver, ty, cx, depth + 1))
        .collect::<Option<Vec<String>>>()
        .map(|texts| texts.join(", "))
}

/// Returns the text of what the path type `path` written in `cx` resolves
/// to, as [`identity`] gives it.
fn path_identity(
    resolver: &Resolver<'_>,
    path: &syn::TypePath,
    cx: &Cx,
    depth: usize,
) -> Option<String> {
    let segments = Resolver::segments(&path.path);
    let leading = path.path.leading_colon.is_some();
    let written = type_arguments(&path.path);
    if !leading
        && segments.len() == 1
        && written.is_empty()
        && let Some((_, arg)) = cx.params.iter().find(|(name, _)| *name == segments[0])
    {
        return arg
            .as_ref()
            .and_then(|arg| identity(resolver, &arg.ty, &arg.cx, depth + 1));
    }
    if !leading && segments.first().is_some_and(|first| first == "Self") {
        let (owner, args) = cx.self_item.as_ref()?;
        if segments.len() > 1 {
            return None;
        }
        let texts = args
            .iter()
            .map(|arg| {
                arg.as_ref()
                    .and_then(|arg| identity(resolver, &arg.ty, &arg.cx, depth + 1))
            })
            .collect::<Option<Vec<String>>>()?;
        return Some(format!("item:{owner}<{}>", texts.join(", ")));
    }
    let arguments = || identities(resolver, &written, cx, depth);
    match resolver
        .resolve(cx.module, leading, &segments, Ns::Type)
        .ok()?
    {
        Res::Item(index) => match &resolver.items[index].kind {
            Kind::Alias(alias) => {
                let given: Vec<Option<Arg>> = written
                    .iter()
                    .map(|argument| {
                        Some(Arg {
                            ty: (*argument).clone(),
                            cx: cx.clone(),
                        })
                    })
                    .collect();
                let target = Cx {
                    module: resolver.items[index].module,
                    params: Rc::new(zip(&alias.generics, &given)),
                    self_item: None,
                    owner: Rc::from(""),
                    aliases: cx.aliases + 1,
                };
                identity(resolver, &alias.ty, &target, depth + 1)
            }
            _ => Some(format!("item:{index}<{}>", arguments()?)),
        },
        Res::Std(named) => Some(format!("std:{}<{}>", named.last()?, arguments()?)),
        Res::Facade(named) => Some(format!("aosabi:{}<{}>", named.last()?, arguments()?)),
        Res::Prim(name) => Some(name),
        Res::Extern(named) => Some(format!("{}<{}>", named.join("::"), arguments()?)),
        Res::Module(_) | Res::Variant(..) => None,
    }
}

/// Returns the text of the trait bound `bound`, by which a bound of a block
/// and a bound of its struct compare, or `None` for a lifetime and for
/// `?Sized`, which narrow no use.
fn bound_text(bound: &syn::TypeParamBound) -> Option<String> {
    match bound {
        syn::TypeParamBound::Trait(bound) => {
            if matches!(bound.modifier, syn::TraitBoundModifier::Maybe(_)) {
                return None;
            }
            let segments: Vec<&syn::PathSegment> = bound.path.segments.iter().collect();
            let mut text = String::new();
            write_segments(&mut text, &segments, bound.path.leading_colon.is_some());
            Some(text)
        }
        syn::TypeParamBound::Verbatim(tokens) => Some(tokens.to_string()),
        _ => None,
    }
}

/// Returns the type `ty` as text, lifetimes left out, for a report.
pub(crate) fn type_text(ty: &Type) -> String {
    let mut out = String::new();
    write_type(&mut out, ty);
    out
}

/// Writes the type `ty` into `out`.
fn write_type(out: &mut String, ty: &Type) {
    match ty {
        Type::Paren(inner) => {
            out.push('(');
            write_type(out, &inner.elem);
            out.push(')');
        }
        Type::Group(inner) => write_type(out, &inner.elem),
        Type::Reference(inner) => {
            out.push('&');
            if inner.mutability.is_some() {
                out.push_str("mut ");
            }
            write_type(out, &inner.elem);
        }
        Type::Slice(inner) => {
            out.push('[');
            write_type(out, &inner.elem);
            out.push(']');
        }
        Type::Array(inner) => {
            out.push('[');
            write_type(out, &inner.elem);
            out.push_str("; _]");
        }
        Type::Tuple(tuple) => {
            out.push('(');
            for (position, elem) in tuple.elems.iter().enumerate() {
                if position > 0 {
                    out.push_str(", ");
                }
                write_type(out, elem);
            }
            if tuple.elems.len() == 1 {
                out.push(',');
            }
            out.push(')');
        }
        Type::Never(_) => out.push('!'),
        Type::Infer(_) => out.push('_'),
        Type::Ptr(pointer) => {
            out.push_str(if pointer.mutability.is_some() {
                "*mut "
            } else {
                "*const "
            });
            write_type(out, &pointer.elem);
        }
        Type::Path(path) => {
            if let Some(qself) = &path.qself {
                out.push('<');
                write_type(out, &qself.ty);
                let written: Vec<&syn::PathSegment> = path.path.segments.iter().collect();
                if qself.position > 0 {
                    out.push_str(" as ");
                    write_segments(
                        out,
                        &written[..qself.position],
                        path.path.leading_colon.is_some(),
                    );
                }
                out.push('>');
                for segment in &written[qself.position..] {
                    out.push_str("::");
                    write_segment(out, segment);
                }
            } else {
                let written: Vec<&syn::PathSegment> = path.path.segments.iter().collect();
                write_segments(out, &written, path.path.leading_colon.is_some());
            }
        }
        Type::ImplTrait(bounds) => {
            out.push_str("impl ");
            write_bounds(out, bounds.bounds.iter());
        }
        Type::TraitObject(bounds) => {
            out.push_str("dyn ");
            write_bounds(out, bounds.bounds.iter());
        }
        Type::BareFn(function) => {
            out.push_str("fn(");
            for (position, input) in function.inputs.iter().enumerate() {
                if position > 0 {
                    out.push_str(", ");
                }
                write_type(out, &input.ty);
            }
            out.push(')');
            if let syn::ReturnType::Type(_, output) = &function.output {
                out.push_str(" -> ");
                write_type(out, output);
            }
        }
        Type::Macro(mac) => {
            out.push_str(&super::tree::path_text(&mac.mac.path));
            out.push_str("!(..)");
        }
        _ => out.push_str("(type)"),
    }
}

/// Writes the path of `segments` into `out`.
fn write_segments(out: &mut String, segments: &[&syn::PathSegment], leading: bool) {
    if leading {
        out.push_str("::");
    }
    for (position, segment) in segments.iter().enumerate() {
        if position > 0 {
            out.push_str("::");
        }
        write_segment(out, segment);
    }
}

/// Writes one segment of a path with its arguments into `out`.
fn write_segment(out: &mut String, segment: &syn::PathSegment) {
    use syn::ext::IdentExt;
    out.push_str(&segment.ident.unraw().to_string());
    match &segment.arguments {
        syn::PathArguments::None => {}
        syn::PathArguments::AngleBracketed(arguments) => {
            let written: Vec<String> = arguments
                .args
                .iter()
                .filter_map(|argument| match argument {
                    syn::GenericArgument::Type(ty) => Some(type_text(ty)),
                    syn::GenericArgument::AssocType(assoc) => {
                        Some(format!("{} = {}", assoc.ident, type_text(&assoc.ty)))
                    }
                    syn::GenericArgument::Const(_) => Some(String::from("_")),
                    _ => None,
                })
                .collect();
            if !written.is_empty() {
                out.push('<');
                out.push_str(&written.join(", "));
                out.push('>');
            }
        }
        syn::PathArguments::Parenthesized(arguments) => {
            out.push('(');
            let inputs: Vec<String> = arguments.inputs.iter().map(type_text).collect();
            out.push_str(&inputs.join(", "));
            out.push(')');
            if let syn::ReturnType::Type(_, output) = &arguments.output {
                out.push_str(" -> ");
                write_type(out, output);
            }
        }
    }
}

/// Writes the trait bounds `bounds` into `out`, lifetimes left out.
fn write_bounds<'b>(out: &mut String, bounds: impl Iterator<Item = &'b TypeParamBound>) {
    let written: Vec<String> = bounds
        .filter_map(|bound| match bound {
            TypeParamBound::Trait(bound) => {
                let segments: Vec<&syn::PathSegment> = bound.path.segments.iter().collect();
                let mut text = String::new();
                write_segments(&mut text, &segments, bound.path.leading_colon.is_some());
                Some(text)
            }
            _ => None,
        })
        .collect();
    out.push_str(&written.join(" + "));
}
