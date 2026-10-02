// SPDX-License-Identifier: MIT
/*
 * aosabi-build: the walk of the module tree of a crate from its root file.
 *
 * The file resolution of `mod` follows the Rust Reference, chapter Modules, as
 * the scan of the declaration list does.
 */

//! The walk of the module tree of one crate.
//!
//! The walk parses the root file of a crate with `syn` and reads every item of
//! every module, for the crate of the package and for each of its path
//! dependencies, every file named relative to the directory of the package. A
//! `mod x;` leads to `x.rs` or `x/mod.rs`, a `#[path]` to the file it names,
//! and an inline `mod x { .. }` into its body, by the rules the scan of the
//! declaration list follows. It keeps every struct, enum, union, type alias,
//! trait, function, constant, static, impl block, macro call and
//! `extern crate` with its name, its visibility, its file and line and its doc
//! text, and every name a `use` declaration binds with its path, its
//! visibility and its file and line. `syn` turns a `///` line and a `/** */`
//! block into a `#[doc]` attribute, so the three forms of a doc comment arrive
//! as one. It does not descend into a function or into the input of a macro,
//! whose items the source does not state at the level of a module.
//!
//! The walk strips every file it parses as rustc strips it over the active set
//! of the build, before it reads an item of it, so it reads no item, field,
//! variant, method, statement or argument that a `cfg` leaves out, takes every
//! attribute a `cfg_attr` whose predicate holds names, a `#[path]` among them,
//! and reads no file of a module that a `cfg` leaves out. A file whose inner
//! attributes hold a `cfg` that holds no more adds no item, a crate root of
//! such a file holding none and a module of such a file standing nowhere, as
//! rustc takes the inner attributes of a module file as attributes of its
//! module. A module whose file is missing stands with no item, as rustc
//! refuses it.

use std::{
    fs,
    path::{
        Path,
        PathBuf, //
    }, //
};

use syn::{
    Attribute,
    Expr,
    ExprLit,
    Fields,
    FnArg,
    GenericParam,
    Generics,
    ImplItem,
    Lit,
    Meta,
    Pat,
    ReturnType,
    UseTree,
    Visibility,
    ext::IdentExt,
    punctuated::Punctuated,
    spanned::Spanned, //
};

use super::{
    Place,
    Refusal,
    SourceError,
    cfg::{
        self,
        Active, //
    },
    model::{
        Alias,
        Data,
        DocSpan,
        Docs,
        Enumeration,
        Field,
        Function,
        Impl,
        Item,
        Kind,
        MacroCall,
        Method,
        Module,
        Param,
        Receiver,
        Shape,
        Use,
        Variant,
        Vis, //
    }, //
};

/// The modules, the items and the files of a crate.
#[derive(Default)]
pub(crate) struct Tree {
    /// Every module, the crate root first.
    pub(crate) modules: Vec<Module>,
    /// Every item of every module, in the order the walk meets them.
    pub(crate) items: Vec<Item>,
    /// Every file the walk read, relative to the package, in order.
    pub(crate) files: Vec<PathBuf>,
    /// The forms the strip of the files refuses, in the order of the walk.
    pub(crate) refusals: Vec<Refusal>,
}

/// Where the walk stands in a module.
struct Frame {
    /// The module, as an index into the modules of the tree.
    module: usize,
    /// The directory a `mod x;` of the module resolves against, relative to
    /// the package.
    dir: PathBuf,
    /// The name of the file of a module that owns no directory of its own.
    relative: Option<String>,
}

/// The state of the walk.
struct Walker<'a> {
    /// The directory of the package.
    package: &'a Path,
    /// What the walk found so far.
    tree: &'a mut Tree,
    /// The library the walk reads.
    krate: usize,
    /// The active set of the build of that library.
    active: Active<'a>,
    /// The files of the modules the walk is inside, which tells a circular
    /// tree.
    chain: Vec<PathBuf>,
}

/// Walks the crate of the library `krate`, whose root file is `root`,
/// relative to the package directory `package`, into `tree`, every file
/// stripped over the active set `active` of its build, and answers the module
/// of its crate root.
///
/// # Errors
///
/// Returns [`SourceError::Io`] when a file of the tree cannot be read,
/// [`SourceError::Parse`] with the file and the line when a file does not
/// parse as Rust, and [`SourceError::Circular`] when a file is its own module.
pub(crate) fn walk(
    tree: &mut Tree,
    package: &Path,
    root: &Path,
    krate: usize,
    active: Active<'_>,
) -> Result<usize, SourceError> {
    let module = tree.modules.len();
    tree.modules.push(Module {
        krate,
        path: Vec::new(),
        parent: None,
        items: Vec::new(),
        uses: Vec::new(),
        file: root.to_path_buf(),
    });
    let mut walker = Walker {
        package,
        tree,
        krate,
        active,
        chain: Vec::new(),
    };
    let frame = Frame {
        module,
        dir: root.parent().map(Path::to_path_buf).unwrap_or_default(),
        relative: None,
    };
    if let Some(parsed) = walker.parse(root)? {
        walker.file(&frame, &parsed.items, root)?;
    }
    Ok(module)
}

impl Walker<'_> {
    /// Reads the file `file` and parses it, and answers it stripped over the
    /// active set of the build, or `None` where a `cfg` among its inner
    /// attributes holds no more.
    fn parse(&mut self, file: &Path) -> Result<Option<syn::File>, SourceError> {
        if self.chain.iter().any(|open| open == file) {
            return Err(SourceError::Circular(file.to_path_buf()));
        }
        let path = self.package.join(file);
        let text = fs::read_to_string(&path).map_err(|error| SourceError::Io(path, error))?;
        self.tree.files.push(file.to_path_buf());
        // A byte order mark is no token, and rustc skips it. The parser `syn`
        // skips a shebang itself.
        let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
        let mut parsed = syn::parse_file(text).map_err(|error| SourceError::Parse {
            place: Place {
                file: file.to_path_buf(),
                line: error.span().start().line,
            },
            message: error.to_string(),
        })?;
        let kept = cfg::file(&mut parsed, self.active, file, &mut self.tree.refusals);
        Ok(kept.then_some(parsed))
    }

    /// Walks the items `items` of the file `file` of the module of `frame`.
    fn file(&mut self, frame: &Frame, items: &[syn::Item], file: &Path) -> Result<(), SourceError> {
        self.chain.push(file.to_path_buf());
        let walked = self.items(frame, items, file);
        self.chain.pop();
        walked
    }

    /// Walks the items of the module of `frame`, which stand in `file`.
    fn items(
        &mut self,
        frame: &Frame,
        items: &[syn::Item],
        file: &Path,
    ) -> Result<(), SourceError> {
        for item in items {
            self.item(frame, item, file)?;
        }
        Ok(())
    }

    /// Keeps one item of the module of `frame`.
    fn item(&mut self, frame: &Frame, item: &syn::Item, file: &Path) -> Result<(), SourceError> {
        match item {
            syn::Item::Mod(item) => return self.module(frame, item, file),
            syn::Item::Use(item) => {
                let declared = DeclaredUse {
                    leading: item.leading_colon.is_some(),
                    vis: vis(&item.vis),
                    file,
                };
                let uses = &mut self.tree.modules[frame.module].uses;
                flatten(&item.tree, Vec::new(), &declared, uses);
                return Ok(());
            }
            _ => {}
        }
        let Some(declared) = declaration(item, file) else {
            return Ok(());
        };
        let index = self.tree.items.len();
        self.tree.items.push(Item {
            name: declared.name,
            module: frame.module,
            vis: declared.vis,
            place: place(file, declared.span),
            docs: docs(declared.attrs, file),
            described: described(declared.attrs),
            kind: declared.kind,
        });
        self.tree.modules[frame.module].items.push(index);
        Ok(())
    }

    /// Keeps the module `item` of the module of `frame` and walks it.
    ///
    /// The strip of the file of the parent left out a module whose `cfg`
    /// holds no more, so the walk reads the file of every module it meets, by
    /// the `#[path]` a `cfg_attr` may have given it, and keeps no module whose
    /// own file the strip does not keep.
    fn module(
        &mut self,
        frame: &Frame,
        item: &syn::ItemMod,
        file: &Path,
    ) -> Result<(), SourceError> {
        let name = unraw(&item.ident);
        let named = path_attribute(&item.attrs);
        if let Some((_, items)) = &item.content {
            let dir = match &named {
                Some(named) => frame.dir.join(named),
                None => owned_dir(frame).join(&name),
            };
            let index = self.declare(frame, item, &name, file);
            let inner = Frame {
                module: index,
                dir,
                relative: None,
            };
            return self.items(&inner, items, file);
        }
        // A module whose file is missing is left to rustc, which refuses it,
        // and stands with no item.
        let Some((child, dir, relative)) = self.module_file(frame, named.as_deref(), &name) else {
            self.declare(frame, item, &name, file);
            return Ok(());
        };
        let Some(parsed) = self.parse(&child)? else {
            return Ok(());
        };
        let index = self.declare(frame, item, &name, file);
        self.tree.modules[index].file.clone_from(&child);
        let inner = Frame {
            module: index,
            dir,
            relative,
        };
        self.file(&inner, &parsed.items, &child)
    }

    /// Adds the module `item` named `name` of the module of `frame`, which
    /// stands in `file`, with the item that declares it, and answers the
    /// module.
    fn declare(&mut self, frame: &Frame, item: &syn::ItemMod, name: &str, file: &Path) -> usize {
        let index = self.tree.modules.len();
        let mut path = self.tree.modules[frame.module].path.clone();
        path.push(String::from(name));
        self.tree.modules.push(Module {
            krate: self.krate,
            path,
            parent: Some(frame.module),
            items: Vec::new(),
            uses: Vec::new(),
            file: file.to_path_buf(),
        });
        let declared = self.tree.items.len();
        self.tree.items.push(Item {
            name: String::from(name),
            module: frame.module,
            vis: vis(&item.vis),
            place: place(file, item.ident.span()),
            docs: docs(&item.attrs, file),
            described: false,
            kind: Kind::Module(index),
        });
        self.tree.modules[frame.module].items.push(declared);
        index
    }

    /// Returns the file of the module `name` that the module of `frame`
    /// declares with no body, with the directory and the relative name the
    /// module owns.
    ///
    /// A `#[path]` resolves against the directory of its module and names a
    /// file that owns its own directory. Without one, the module is `x.rs`
    /// beside the directory the parent owns, which owns the directory `x`, or
    /// `x/mod.rs`, which owns its own directory.
    fn module_file(
        &self,
        frame: &Frame,
        named: Option<&str>,
        name: &str,
    ) -> Option<(PathBuf, PathBuf, Option<String>)> {
        if let Some(named) = named {
            let file = frame.dir.join(named);
            let dir = file.parent()?.to_path_buf();
            return self
                .package
                .join(&file)
                .is_file()
                .then_some((file, dir, None));
        }
        let dir = owned_dir(frame);
        let flat = dir.join(format!("{name}.rs"));
        if self.package.join(&flat).is_file() {
            return Some((flat, dir, Some(String::from(name))));
        }
        let nested = dir.join(name).join("mod.rs");
        let owned = dir.join(name);
        self.package
            .join(&nested)
            .is_file()
            .then_some((nested, owned, None))
    }
}

/// What one item of a module declares, before the walk keeps it.
struct Declaration<'a> {
    /// The name, its raw prefix taken off.
    name: String,
    /// The visibility.
    vis: Vis,
    /// The span of the name, or of the keyword of an impl block.
    span: proc_macro2::Span,
    /// The outer attributes.
    attrs: &'a [Attribute],
    /// What it declares.
    kind: Kind,
}

impl<'a> Declaration<'a> {
    /// Returns the declaration of the item named `ident`.
    fn named(
        ident: &proc_macro2::Ident,
        item_vis: &Visibility,
        attrs: &'a [Attribute],
        kind: Kind,
    ) -> Self {
        Self {
            name: unraw(ident),
            vis: vis(item_vis),
            span: ident.span(),
            attrs,
            kind,
        }
    }
}

/// Returns what the item `item` of `file` declares, `None` for a module, a
/// `use` and an item the reader keeps no record of.
fn declaration<'a>(item: &'a syn::Item, file: &Path) -> Option<Declaration<'a>> {
    let declared = match item {
        syn::Item::Struct(item) => {
            let (shape, fields) = fields(&item.fields, file);
            let generics = type_params(&item.generics);
            let (bounds, _) = bounds(&item.generics);
            let kind = Kind::Struct(Data {
                generics,
                lifetimes: item.generics.lifetimes().count(),
                bounds,
                shape,
                fields,
            });
            Declaration::named(&item.ident, &item.vis, &item.attrs, kind)
        }
        syn::Item::Enum(item) => {
            let variants = item
                .variants
                .iter()
                .map(|variant| {
                    let (shape, fields) = fields(&variant.fields, file);
                    Variant {
                        name: unraw(&variant.ident),
                        place: place(file, variant.ident.span()),
                        docs: docs(&variant.attrs, file),
                        shape,
                        fields,
                    }
                })
                .collect();
            let generics = type_params(&item.generics);
            let kind = Kind::Enum(Enumeration {
                generics,
                lifetimes: item.generics.lifetimes().count(),
                variants,
            });
            Declaration::named(&item.ident, &item.vis, &item.attrs, kind)
        }
        syn::Item::Union(item) => {
            Declaration::named(&item.ident, &item.vis, &item.attrs, Kind::Union)
        }
        syn::Item::Type(item) => {
            let generics = type_params(&item.generics);
            let kind = Kind::Alias(Alias {
                generics,
                ty: (*item.ty).clone(),
            });
            Declaration::named(&item.ident, &item.vis, &item.attrs, kind)
        }
        syn::Item::Trait(item) => {
            Declaration::named(&item.ident, &item.vis, &item.attrs, Kind::Trait)
        }
        syn::Item::TraitAlias(item) => {
            Declaration::named(&item.ident, &item.vis, &item.attrs, Kind::Trait)
        }
        syn::Item::Fn(item) => {
            let kind = Kind::Function(function(&item.sig, &item.block, file));
            Declaration::named(&item.sig.ident, &item.vis, &item.attrs, kind)
        }
        syn::Item::Const(item) => {
            let kind = Kind::Value((*item.ty).clone());
            Declaration::named(&item.ident, &item.vis, &item.attrs, kind)
        }
        syn::Item::Static(item) => {
            let kind = Kind::Value((*item.ty).clone());
            Declaration::named(&item.ident, &item.vis, &item.attrs, kind)
        }
        syn::Item::Impl(item) => impl_block(item, file),
        syn::Item::Macro(item) => {
            let defines = item.ident.as_ref().map(unraw);
            Declaration {
                name: defines.clone().unwrap_or_default(),
                vis: Vis::Private,
                span: item
                    .ident
                    .as_ref()
                    .map_or_else(|| item.mac.path.span(), proc_macro2::Ident::span),
                attrs: &item.attrs,
                kind: Kind::Macro(MacroCall {
                    path: path_text(&item.mac.path),
                    defines,
                }),
            }
        }
        syn::Item::ExternCrate(item) => {
            let kind = Kind::ExternCrate(unraw(&item.ident));
            let mut declared = Declaration::named(&item.ident, &item.vis, &item.attrs, kind);
            if let Some((_, rename)) = &item.rename {
                declared.name = unraw(rename);
            }
            declared
        }
        _ => return None,
    };
    Some(declared)
}

/// Returns what the impl block `item` of `file` declares.
fn impl_block<'a>(item: &'a syn::ItemImpl, file: &Path) -> Declaration<'a> {
    let (bounds, bounds_other) = bounds(&item.generics);
    Declaration {
        name: String::new(),
        vis: Vis::Private,
        span: item.impl_token.span,
        attrs: &item.attrs,
        kind: Kind::Impl(Impl {
            generics: type_params(&item.generics),
            bounds,
            bounds_other,
            self_ty: (*item.self_ty).clone(),
            trait_: item.trait_.as_ref().map(|(_, path, _)| path.clone()),
            methods: methods(&item.items, file),
        }),
    }
}

/// Returns the directory the modules of the module of `frame` resolve
/// against when they have no `#[path]`.
fn owned_dir(frame: &Frame) -> PathBuf {
    match &frame.relative {
        Some(relative) => frame.dir.join(relative),
        None => frame.dir.clone(),
    }
}

/// Returns the place of `span` in `file`.
fn place(file: &Path, span: proc_macro2::Span) -> Place {
    Place {
        file: file.to_path_buf(),
        line: span.start().line,
    }
}

/// Returns the name of `ident` with its raw prefix taken off.
fn unraw(ident: &proc_macro2::Ident) -> String {
    ident.unraw().to_string()
}

/// Returns the path `path` as written, its raw prefixes taken off.
pub(crate) fn path_text(path: &syn::Path) -> String {
    let segments: Vec<String> = path
        .segments
        .iter()
        .map(|segment| unraw(&segment.ident))
        .collect();
    let joined = segments.join("::");
    if path.leading_colon.is_some() {
        format!("::{joined}")
    } else {
        joined
    }
}

/// Returns the visibility `vis`.
fn vis(vis: &Visibility) -> Vis {
    match vis {
        Visibility::Public(_) => Vis::Public,
        Visibility::Inherited => Vis::Private,
        Visibility::Restricted(restricted) => match path_text(&restricted.path).as_str() {
            "crate" => Vis::Crate,
            "super" => Vis::Super,
            "self" => Vis::Private,
            _ => Vis::Restricted,
        },
    }
}

/// Returns the names of the type parameters of `generics`, in order.
fn type_params(generics: &Generics) -> Vec<String> {
    generics
        .params
        .iter()
        .filter_map(|param| match param {
            GenericParam::Type(param) => Some(unraw(&param.ident)),
            _ => None,
        })
        .collect()
}

/// Returns the names of the const parameters of `generics`, in order.
fn const_params(generics: &Generics) -> Vec<String> {
    generics
        .params
        .iter()
        .filter_map(|param| match param {
            GenericParam::Const(param) => Some(unraw(&param.ident)),
            _ => None,
        })
        .collect()
}

/// Returns the trait bounds of the type parameters of `generics`, each with
/// the name of its parameter, from the list of parameters and from the where
/// clause, and whether the where clause bounds a type that is no type
/// parameter.
///
/// A lifetime and the lifetime bound of a parameter are left out, as the
/// reader ignores lifetimes.
fn bounds(generics: &Generics) -> (Vec<(String, syn::TypeParamBound)>, bool) {
    let names = type_params(generics);
    let mut bounds = Vec::new();
    for param in &generics.params {
        if let GenericParam::Type(param) = param {
            let name = unraw(&param.ident);
            bounds.extend(
                param
                    .bounds
                    .iter()
                    .map(|bound| (name.clone(), bound.clone())),
            );
        }
    }
    let mut other = false;
    for predicate in generics
        .where_clause
        .iter()
        .flat_map(|clause| clause.predicates.iter())
    {
        let syn::WherePredicate::Type(predicate) = predicate else {
            continue;
        };
        let parameter = match &predicate.bounded_ty {
            syn::Type::Path(path) if path.qself.is_none() => path
                .path
                .get_ident()
                .map(unraw)
                .filter(|name| names.contains(name)),
            _ => None,
        };
        match parameter {
            Some(name) => bounds.extend(
                predicate
                    .bounds
                    .iter()
                    .map(|bound| (name.clone(), bound.clone())),
            ),
            None => other = true,
        }
    }
    (bounds, other)
}

/// Returns the shape and the fields of `fields`, which stand in `file`.
fn fields(fields: &Fields, file: &Path) -> (Shape, Vec<Field>) {
    match fields {
        Fields::Named(named) => {
            let kept = named
                .named
                .iter()
                .map(|field| {
                    let ident = field.ident.as_ref();
                    Field {
                        name: ident.map(unraw).unwrap_or_default(),
                        vis: vis(&field.vis),
                        ty: field.ty.clone(),
                        place: place(
                            file,
                            ident.map_or_else(|| field.ty.span(), proc_macro2::Ident::span),
                        ),
                        docs: docs(&field.attrs, file),
                    }
                })
                .collect();
            (Shape::Named, kept)
        }
        Fields::Unnamed(unnamed) => {
            let kept = unnamed
                .unnamed
                .iter()
                .enumerate()
                .map(|(position, field)| Field {
                    name: position.to_string(),
                    vis: vis(&field.vis),
                    ty: field.ty.clone(),
                    place: place(file, field.ty.span()),
                    docs: docs(&field.attrs, file),
                })
                .collect();
            (Shape::Tuple, kept)
        }
        Fields::Unit => (Shape::Unit, Vec::new()),
    }
}

/// Returns the function of the signature `sig` and the body `block`.
fn function(sig: &syn::Signature, block: &syn::Block, file: &Path) -> Function {
    let params = sig
        .inputs
        .iter()
        .filter_map(|input| match input {
            FnArg::Typed(typed) => Some(param(typed, file)),
            FnArg::Receiver(_) => None,
        })
        .collect();
    Function {
        receiver: sig.receiver().map(receiver),
        asyncness: sig.asyncness.is_some(),
        generics: type_params(&sig.generics),
        consts: const_params(&sig.generics),
        params,
        output: match &sig.output {
            ReturnType::Default => None,
            ReturnType::Type(_, ty) => Some((**ty).clone()),
        },
        block: block.clone(),
    }
}

/// Returns the form in which `receiver` takes `self`.
///
/// `syn` writes the type of every receiver, `Self` for `self`, `&Self` for
/// `&self` and `&mut Self` for `&mut self`, so the form is read from that
/// type whether the source writes the short form or `self: T`.
fn receiver(receiver: &syn::Receiver) -> Receiver {
    match &*receiver.ty {
        syn::Type::Reference(reference) if is_self(&reference.elem) => {
            if reference.mutability.is_some() {
                Receiver::Unique
            } else {
                Receiver::Shared
            }
        }
        ty if is_self(ty) => Receiver::Owned,
        _ => Receiver::Typed,
    }
}

/// Returns whether `ty` is the type `Self`.
fn is_self(ty: &syn::Type) -> bool {
    matches!(ty, syn::Type::Path(path) if path.qself.is_none() && path.path.is_ident("Self"))
}

/// Returns the parameter `typed`.
fn param(typed: &syn::PatType, file: &Path) -> Param {
    let (name, named) = match &*typed.pat {
        Pat::Ident(ident) => (unraw(&ident.ident), true),
        Pat::Wild(_) => (String::from("_"), false),
        _ => (String::from("(pattern)"), false),
    };
    Param {
        name,
        named,
        ty: (*typed.ty).clone(),
        place: place(file, typed.pat.span()),
    }
}

/// Returns the methods of the items `items` of an impl block.
fn methods(items: &[ImplItem], file: &Path) -> Vec<Method> {
    items
        .iter()
        .filter_map(|item| match item {
            ImplItem::Fn(method) => Some(Method {
                name: unraw(&method.sig.ident),
                vis: vis(&method.vis),
                place: place(file, method.sig.ident.span()),
                docs: docs(&method.attrs, file),
                function: function(&method.sig, &method.block, file),
            }),
            _ => None,
        })
        .collect()
}

/// What every name of one `use` declaration shares.
struct DeclaredUse<'a> {
    /// Whether the path opens with `::`.
    leading: bool,
    /// The visibility of the declaration.
    vis: Vis,
    /// The file the declaration stands in.
    file: &'a Path,
}

/// Adds to `uses` every name and glob the tree `tree` binds under the path
/// `prefix`, each at the place of its own part of the tree.
///
/// `self` in a group binds the module of the prefix, and a rename to `_`
/// binds no name.
fn flatten(tree: &UseTree, prefix: Vec<String>, declared: &DeclaredUse<'_>, uses: &mut Vec<Use>) {
    let mut add = |path: Vec<String>, name: Option<String>| {
        uses.push(Use {
            leading: declared.leading,
            path,
            name,
            vis: declared.vis.clone(),
            place: place(declared.file, tree.span()),
        });
    };
    match tree {
        UseTree::Path(path) => {
            let mut prefix = prefix;
            prefix.push(unraw(&path.ident));
            flatten(&path.tree, prefix, declared, uses);
        }
        UseTree::Name(name) => {
            let ident = unraw(&name.ident);
            if ident == "self" {
                let bound = prefix.last().cloned();
                add(prefix, bound);
            } else {
                let mut path = prefix;
                path.push(ident.clone());
                add(path, Some(ident));
            }
        }
        UseTree::Rename(rename) => {
            let bound = unraw(&rename.rename);
            if bound == "_" {
                return;
            }
            let ident = unraw(&rename.ident);
            let mut path = prefix;
            if ident != "self" {
                path.push(ident);
            }
            add(path, Some(bound));
        }
        UseTree::Glob(_) => add(prefix, None),
        UseTree::Group(group) => {
            for tree in &group.items {
                flatten(tree, prefix.clone(), declared, uses);
            }
        }
    }
}

/// Returns the doc text of the outer attributes `attrs`, which stand in
/// `file`, with the form and the place of each attribute and the place of a
/// `#[deprecated]`.
///
/// The lexer of `proc-macro2` gives every token of a doc comment the span of
/// the whole comment, so the text of the span of its literal opens with `/**`
/// for a block and with `///` for a line, and a `#[doc]` written out spans its
/// string literal alone.
fn docs(attrs: &[Attribute], file: &Path) -> Docs {
    let mut docs = Docs::default();
    for attr in attrs {
        if !matches!(attr.style, syn::AttrStyle::Outer) {
            continue;
        }
        if attr.path().is_ident("deprecated") {
            docs.deprecated
                .get_or_insert_with(|| place(file, attr.span()));
            continue;
        }
        if !attr.path().is_ident("doc") {
            continue;
        }
        let Meta::NameValue(pair) = &attr.meta else {
            continue;
        };
        match &pair.value {
            Expr::Lit(ExprLit {
                lit: Lit::Str(text),
                ..
            }) => {
                let span = text.span();
                let start = span.start();
                docs.lines.push(text.value());
                docs.spans.push(DocSpan {
                    block: span
                        .source_text()
                        .is_some_and(|source| source.starts_with("/**")),
                    line: start.line,
                    column: start.column,
                });
            }
            _ => {
                docs.unread.get_or_insert_with(|| place(file, attr.span()));
            }
        }
    }
    docs
}

/// Returns whether a derive of `Aosabi`, `#[aosabi::scalar]` or
/// `#[aosabi::resolvers]` stands among `attrs`, the annotations of the
/// derive path.
fn described(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        let path = attr.path();
        if path.is_ident("derive") {
            let Ok(derived) =
                attr.parse_args_with(Punctuated::<syn::Path, syn::Token![,]>::parse_terminated)
            else {
                return false;
            };
            return derived.iter().any(|path| {
                path.segments
                    .last()
                    .is_some_and(|last| last.ident == "Aosabi")
            });
        }
        matches!(
            path_text(path).as_str(),
            "aosabi::scalar" | "scalar" | "aosabi::resolvers" | "resolvers"
        )
    })
}

/// Returns the value of a `#[path = ".."]` among `attrs`.
fn path_attribute(attrs: &[Attribute]) -> Option<String> {
    attrs.iter().find_map(|attr| {
        if !attr.path().is_ident("path") {
            return None;
        }
        let Meta::NameValue(pair) = &attr.meta else {
            return None;
        };
        match &pair.value {
            Expr::Lit(ExprLit {
                lit: Lit::Str(text),
                ..
            }) => Some(text.value()),
            _ => None,
        }
    })
}
