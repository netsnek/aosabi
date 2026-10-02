// SPDX-License-Identifier: MIT
/*
 * aosabi-build: the resolution of a path of a crate to the item it names.
 *
 * Stands in for the symbol table the checker of TypeScript gives the
 * inference. The rules are the path rules of the Rust Reference, chapters
 * Paths, Use declarations and Names, for a crate of the edition 2018 and
 * later.
 */

//! The resolution of paths.
//!
//! A path resolves from the module that writes it. Its first segment is
//! `crate`, `self`, `super`, a name the module declares or imports, an extern
//! crate or a name of the prelude of the standard library, in that order, and
//! every further segment is read in what the segment before it names. A module
//! sees the items it declares, the names its `use` declarations bind and,
//! behind those, the names its globs import, and no name of the module around
//! it, as Rust scopes a module.
//!
//! The reader knows four crates beside the ones it reads: `std`, `core` and
//! `alloc`, the three crates of the standard library, whose names it maps by
//! the table of [`std_role`], and the facade `aosabi`, whose names it maps by
//! the table of [`facade_role`]. It reads the source of every path dependency,
//! so a path that opens with the name a crate sees such a dependency by goes
//! on in the root module of that dependency, `crate` names the root of the
//! crate of the module that writes it, and an item of another crate is seen
//! when it is `pub`. Any other crate, of a registry or of git, is another
//! crate, whose source the reader does not read. An `extern crate` names the
//! crate it names under the name it binds, so the line `extern crate alloc;`
//! of the root of a `no_std` crate names `alloc`, and an `extern crate` of a
//! crate root adds that name to the extern prelude, which every module of its
//! crate sees. A glob of a crate whose names the reader does not hold whole
//! may import a name the reader finds nowhere else, so a lookup that ends with
//! no answer names each such glob it met with its place.

use syn::{
    GenericArgument,
    PathArguments,
    Token,
    Type,
    TypeParamBound,
    punctuated::Punctuated, //
};

use super::{
    Place,
    model::{
        Item,
        Kind,
        Library,
        Module,
        Use,
        Vis, //
    }, //
};

/// The namespace a name is looked up in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Ns {
    /// Types, traits and modules.
    Type,
    /// Functions, constants, statics and constructors.
    Value,
}

/// What a path names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Res {
    /// A module of the crate.
    Module(usize),
    /// An item of the crate.
    Item(usize),
    /// A variant of an enum of the crate, as the item and the position.
    Variant(usize, usize),
    /// A name of `std`, `core` or `alloc`, as the segments after the crate,
    /// or the name of the prelude alone.
    Std(Vec<String>),
    /// A name of the facade, as the segments after `aosabi`.
    Facade(Vec<String>),
    /// A name of a crate whose source the reader does not read, a crate of a
    /// registry or of git, as every segment, the crate first.
    Extern(Vec<String>),
    /// A primitive type.
    Prim(String),
}

/// Why a path names nothing the reader holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Miss {
    /// No module in reach declares or imports the name.
    Unknown {
        /// The segment that names nothing.
        name: String,
        /// The globs in scope of crates whose names the reader does not hold
        /// whole, any of which may import it.
        globs: Vec<Glob>,
    },
    /// A segment reaches through a type to an associated item.
    Associated,
}

/// The crate a glob imports from, whose names the reader does not hold
/// whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GlobOf {
    /// `std`, `core` or `alloc`, whose names outside the table of
    /// [`std_role`] have no GraphQL form.
    Std,
    /// The facade, whose names outside the table of [`facade_role`] have no
    /// GraphQL form.
    Facade,
    /// Another crate, whose source the reader does not read.
    Other,
}

/// A glob of a crate whose names the reader does not hold whole, which may
/// import a name the lookup finds nowhere else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Glob {
    /// The path of the glob as the `use` writes it, `std::collections::*`.
    pub(crate) path: String,
    /// The crate it imports from.
    pub(crate) of: GlobOf,
    /// The place of the glob.
    pub(crate) place: Place,
}

/// The role of a known name of `std`, `core`, `alloc` or the facade in a
/// type position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Known {
    /// A scalar or a JSON value the builder maps, which reaches no type.
    Leaf,
    /// A wrapper whose type argument at the position is what it holds.
    Wrap(usize),
    /// The trait `IntoApp` of the facade, which every entry answers.
    IntoApp,
    /// The builder of an app, `App` of the facade.
    Builder,
    /// The trait `Stream` of the facade, whose binding `Item` is what a
    /// subscription field answers.
    Stream,
    /// A name outside the tables, which is no GraphQL type.
    Foreign,
}

/// The extern crates every crate sees beside its dependencies, which the
/// reader knows.
const STD_CRATES: [&str; 3] = ["std", "core", "alloc"];

/// The crate name of the facade.
pub(crate) const FACADE: &str = "aosabi";

/// The types of the prelude of the standard library, which every module
/// sees.
const PRELUDE_TYPES: [&str; 5] = ["Option", "Result", "Vec", "String", "Box"];

/// The traits of the prelude of the standard library a bound may name.
const PRELUDE_TRAITS: [&str; 23] = [
    "AsMut",
    "AsRef",
    "Clone",
    "Copy",
    "Default",
    "DoubleEndedIterator",
    "Drop",
    "Eq",
    "ExactSizeIterator",
    "Extend",
    "Fn",
    "FnMut",
    "FnOnce",
    "From",
    "FromIterator",
    "Into",
    "IntoIterator",
    "Iterator",
    "Ord",
    "PartialEq",
    "PartialOrd",
    "Send",
    "Sync",
];

/// The names the facade and its prelude export that are no GraphQL type, which
/// a glob of either imports beside the names of [`facade_role`]: the modules
/// `json` and `stream` of `json::Map` and `stream::task`, the error, the
/// context, the logger, the pub sub and the plugins an application registers.
const FACADE_NAMES: [&str; 16] = [
    "json",
    "stream",
    "Error",
    "ServiceError",
    "Key",
    "PubSub",
    "Topic",
    "get_context",
    "get_env",
    "logger",
    "get_logger",
    "use_node_server",
    "use_node_server_with",
    "use_identity",
    "use_sentry",
    "zitadel_login",
];

/// The values of the prelude of the standard library.
const PRELUDE_VALUES: [&str; 4] = ["Some", "None", "Ok", "Err"];

/// The primitive types.
const PRIMITIVES: [&str; 17] = [
    "bool", "char", "str", "f32", "f64", "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16",
    "u32", "u64", "u128", "usize",
];

/// Returns the role of the name of `std`, `core` or `alloc` whose segments
/// after the crate are `path`, by its last segment.
///
/// The table is the one the derive path maps: a string, the never type and the
/// wrappers of one type, `Option`, the lists, the boxes, `Cow` and `Result`,
/// whose first type argument is the value of a resolver. A name outside it is
/// known and has no GraphQL form.
pub(crate) fn std_role(path: &[String]) -> Known {
    match path.last().map(String::as_str) {
        Some("String" | "Infallible") => Known::Leaf,
        Some("Option" | "Vec" | "VecDeque" | "Box" | "Rc" | "Arc" | "Cow" | "Result") => {
            Known::Wrap(0)
        }
        _ => Known::Foreign,
    }
}

/// Returns the role of the name of the facade whose segments after
/// `aosabi` are `path`, by its last segment.
///
/// The table holds the names of the facade that describe a type on the derive
/// path: the markers and the JSON values, the wrappers of one type,
/// `Callable`, whose second type argument is what it answers, and the two
/// names of an app, `IntoApp` and `App`. Beside them stand the names of the
/// native layer, the marker `Number`, the JSON values `json::Map` and
/// `json::Value`, the `Result` of the prelude, whose first type argument is
/// the value as in the `Result` of the derive path, the trait `Stream`, whose
/// binding `Item` a subscription field answers, and the payload of a mutation
/// `MutationPayload`, whose type argument is the struct whose fields the
/// payload holds.
pub(crate) fn facade_role(path: &[String]) -> Known {
    match path.last().map(String::as_str) {
        Some(
            "ID" | "Int" | "Float" | "Number" | "Date" | "File" | "JSON" | "Any" | "Map" | "Value"
            | "JsObject" | "JsValue" | "JsString" | "UserError",
        ) => Known::Leaf,
        Some("Unchecked" | "Promise" | "Repeater" | "Lazy" | "Result" | "MutationPayload") => {
            Known::Wrap(0)
        }
        Some("Callable") => Known::Wrap(1),
        Some("IntoApp") => Known::IntoApp,
        Some("App") => Known::Builder,
        Some("Stream") => Known::Stream,
        _ => Known::Foreign,
    }
}

/// Returns whether a glob of the facade or of its prelude imports the name
/// of the facade whose segments after `aosabi` are `path`: a name of
/// [`facade_role`] or of [`FACADE_NAMES`].
fn facade_exports(path: &[String]) -> bool {
    facade_role(path) != Known::Foreign
        || path
            .last()
            .is_some_and(|last| FACADE_NAMES.contains(&last.as_str()))
}

/// Returns whether `name` is a primitive type.
pub(crate) fn is_primitive(name: &str) -> bool {
    PRIMITIVES.contains(&name)
}

/// Returns whether the crate name `name` is `std`, `core`, `alloc` or the
/// facade, which the reader maps by their tables and whose source it does not
/// read.
pub(crate) fn is_known_crate(name: &str) -> bool {
    known_crate(name).is_some()
}

/// Returns `std`, `core` or `alloc` or the facade by the crate name `name`.
fn known_crate(name: &str) -> Option<Res> {
    if STD_CRATES.contains(&name) {
        Some(Res::Std(Vec::new()))
    } else if name == FACADE {
        Some(Res::Facade(Vec::new()))
    } else {
        None
    }
}

/// Returns the crate a glob of `target` imports from when the reader does
/// not hold its names whole, and `None` for a module or an enum of the crate.
fn glob_of(target: &Res) -> Option<GlobOf> {
    match target {
        Res::Std(_) => Some(GlobOf::Std),
        Res::Facade(_) => Some(GlobOf::Facade),
        Res::Extern(_) => Some(GlobOf::Other),
        Res::Module(_) | Res::Item(_) | Res::Variant(..) | Res::Prim(_) => None,
    }
}

/// Returns the path of the glob `glob` as its declaration writes it.
fn glob_path(glob: &Use) -> String {
    let leading = if glob.leading { "::" } else { "" };
    format!("{leading}{}::*", glob.path.join("::"))
}

/// The resolver of the paths of one crate and of its path dependencies.
pub(crate) struct Resolver<'a> {
    /// The modules of every crate the reader read.
    pub(crate) modules: &'a [Module],
    /// The items of every crate the reader read.
    pub(crate) items: &'a [Item],
    /// The crates the reader read, the crate of the package first.
    pub(crate) libraries: &'a [Library],
}

/// The `use` declarations a resolution is inside, which stops a cycle, and
/// the globs of crates whose names the reader does not hold whole it met.
#[derive(Default)]
struct Guard {
    /// The open declarations, as the module and the position of the use.
    open: Vec<(usize, usize)>,
    /// The globs the lookup met that gave no answer.
    globs: Vec<Glob>,
}

impl Resolver<'_> {
    /// Returns what the path of `segments` names from the module `module` in
    /// the namespace `ns` of its last segment, `leading` when it opens with
    /// `::`.
    ///
    /// # Errors
    ///
    /// Returns the [`Miss`] that says why the path names nothing.
    pub(crate) fn resolve(
        &self,
        module: usize,
        leading: bool,
        segments: &[String],
        ns: Ns,
    ) -> Result<Res, Miss> {
        let mut guard = Guard::default();
        self.walk(module, leading, segments, ns, &mut guard)
            .map_err(|miss| match miss {
                Miss::Unknown { name, mut globs } => {
                    for glob in guard.globs {
                        if !globs.contains(&glob) {
                            globs.push(glob);
                        }
                    }
                    Miss::Unknown { name, globs }
                }
                Miss::Associated => Miss::Associated,
            })
    }

    /// Returns the library of the item `index`, 0 for the crate of the
    /// package.
    pub(crate) fn krate(&self, index: usize) -> usize {
        self.modules[self.items[index].module].krate
    }

    /// Returns the path of the item `index` from its crate root, which opens
    /// with the name of its crate for an item of a path dependency.
    pub(crate) fn path_of(&self, index: usize) -> String {
        let item = &self.items[index];
        let module = &self.modules[item.module];
        let mut segments = Vec::with_capacity(module.path.len() + 2);
        if module.krate != 0 {
            segments.push(self.libraries[module.krate].name.clone());
        }
        segments.extend(module.path.iter().cloned());
        segments.push(item.name.clone());
        segments.join("::")
    }

    /// Returns whether a field or a method of the item `index` with the
    /// visibility `vis` is a member another crate reaches: any visibility
    /// but a private one in the crate of the package, whose glue the builder
    /// writes inside it, and `pub` alone in a path dependency.
    pub(crate) fn exposes(&self, index: usize, vis: &Vis) -> bool {
        match vis {
            Vis::Private => false,
            Vis::Public => true,
            Vis::Crate | Vis::Super | Vis::Restricted => self.krate(index) == 0,
        }
    }

    /// Returns the segments of `path`, their raw prefixes taken off.
    pub(crate) fn segments(path: &syn::Path) -> Vec<String> {
        use syn::ext::IdentExt;
        path.segments
            .iter()
            .map(|segment| segment.ident.unraw().to_string())
            .collect()
    }

    /// Returns the type `T` of `impl Stream<Item = T>`, written in the module
    /// `module`, when a bound of `bounds` is the trait `Stream` of the facade
    /// with the binding `Item`.
    pub(crate) fn stream_item<'t>(
        &self,
        module: usize,
        bounds: &'t Punctuated<TypeParamBound, Token![+]>,
    ) -> Option<&'t Type> {
        bounds.iter().find_map(|bound| {
            let TypeParamBound::Trait(bound) = bound else {
                return None;
            };
            let segments = Self::segments(&bound.path);
            let leading = bound.path.leading_colon.is_some();
            let Ok(Res::Facade(named)) = self.resolve(module, leading, &segments, Ns::Type) else {
                return None;
            };
            if facade_role(&named) != Known::Stream {
                return None;
            }
            let PathArguments::AngleBracketed(arguments) = &bound.path.segments.last()?.arguments
            else {
                return None;
            };
            arguments.args.iter().find_map(|argument| match argument {
                GenericArgument::AssocType(binding) if binding.ident == "Item" => Some(&binding.ty),
                _ => None,
            })
        })
    }

    /// Resolves `segments` from `module` under `guard`.
    fn walk(
        &self,
        module: usize,
        leading: bool,
        segments: &[String],
        ns: Ns,
        guard: &mut Guard,
    ) -> Result<Res, Miss> {
        let Some((first, rest)) = segments.split_first() else {
            return Ok(Res::Module(module));
        };
        let first_ns = if rest.is_empty() { ns } else { Ns::Type };
        let unknown = |name: &str| Miss::Unknown {
            name: String::from(name),
            globs: Vec::new(),
        };
        let mut res = if leading {
            self.extern_crate(module, first)
                .ok_or_else(|| unknown(first))?
        } else {
            match first.as_str() {
                "crate" => Res::Module(self.root_of(module)),
                "self" => Res::Module(module),
                "super" => self.modules[module]
                    .parent
                    .map(Res::Module)
                    .ok_or_else(|| unknown(first))?,
                _ => self.lexical(module, first, first_ns, guard)?,
            }
        };
        for (position, segment) in rest.iter().enumerate() {
            let segment_ns = if position + 1 == rest.len() {
                ns
            } else {
                Ns::Type
            };
            res = match res {
                Res::Module(inner) => match segment.as_str() {
                    "super" => self.modules[inner]
                        .parent
                        .map(Res::Module)
                        .ok_or_else(|| unknown(segment))?,
                    "self" => Res::Module(inner),
                    _ => self
                        .in_module(inner, segment, segment_ns, None, guard)
                        .ok_or_else(|| unknown(segment))?,
                },
                Res::Item(item) => match &self.items[item].kind {
                    Kind::Enum(variants) => variants
                        .variants
                        .iter()
                        .position(|variant| variant.name == *segment)
                        .map(|position| Res::Variant(item, position))
                        .ok_or(Miss::Associated)?,
                    _ => return Err(Miss::Associated),
                },
                Res::Std(mut path) => {
                    path.push(segment.clone());
                    Res::Std(path)
                }
                Res::Facade(mut path) => {
                    path.push(segment.clone());
                    Res::Facade(path)
                }
                Res::Extern(mut path) => {
                    path.push(segment.clone());
                    Res::Extern(path)
                }
                Res::Variant(..) | Res::Prim(_) => return Err(Miss::Associated),
            };
        }
        Ok(res)
    }

    /// Returns what the first segment `name` of a path names in `module`: a
    /// name the module declares or imports, an extern crate, a name of the
    /// prelude or a primitive type.
    fn lexical(&self, module: usize, name: &str, ns: Ns, guard: &mut Guard) -> Result<Res, Miss> {
        if let Some(found) = self.in_module(module, name, ns, Some(module), guard) {
            return Ok(found);
        }
        if let Some(found) = self.extern_crate(module, name) {
            return Ok(found);
        }
        let prelude = match ns {
            Ns::Type => PRELUDE_TYPES.contains(&name) || PRELUDE_TRAITS.contains(&name),
            Ns::Value => PRELUDE_VALUES.contains(&name),
        };
        if prelude {
            return Ok(Res::Std(vec![String::from(name)]));
        }
        if ns == Ns::Type && is_primitive(name) {
            return Ok(Res::Prim(String::from(name)));
        }
        Err(Miss::Unknown {
            name: String::from(name),
            globs: Vec::new(),
        })
    }

    /// Returns the root module of the crate of the module `module`.
    fn root_of(&self, module: usize) -> usize {
        self.libraries[self.modules[module].krate].root
    }

    /// Returns the extern crate `name`, which the module `module` sees
    /// whatever it declares: `std`, `core`, `alloc`, the facade, a dependency
    /// of its crate, or the name an `extern crate` of its crate root binds,
    /// which the root adds to the extern prelude.
    fn extern_crate(&self, module: usize, name: &str) -> Option<Res> {
        if let Some(known) = known_crate(name) {
            return Some(known);
        }
        if let Some(found) = self.dependency(module, name) {
            return Some(found);
        }
        let root = self.root_of(module);
        self.modules[root].items.iter().find_map(|&index| {
            let item = &self.items[index];
            match &item.kind {
                Kind::ExternCrate(krate) if item.name == name => {
                    Some(self.crate_named(module, krate))
                }
                _ => None,
            }
        })
    }

    /// Returns the dependency `name` of the crate of the module `module`: the
    /// root module of a path dependency the reader read, and any other crate
    /// as one whose source the reader does not read.
    fn dependency(&self, module: usize, name: &str) -> Option<Res> {
        let library = &self.libraries[self.modules[module].krate];
        library
            .externs
            .iter()
            .find(|(known, _)| known == name)
            .map(|(_, read)| match read {
                Some(read) => Res::Module(self.libraries[*read].root),
                None => Res::Extern(vec![String::from(name)]),
            })
    }

    /// Returns the crate an `extern crate` of the crate name `krate` names in
    /// the module `module`: the crate root for `self`, `std`, `core`, `alloc`
    /// and the facade by their tables, a dependency of the crate of the
    /// module, and any other crate as one whose source the reader does not
    /// read.
    fn crate_named(&self, module: usize, krate: &str) -> Res {
        if krate == "self" {
            return Res::Module(self.root_of(module));
        }
        known_crate(krate)
            .or_else(|| self.dependency(module, krate))
            .unwrap_or_else(|| Res::Extern(vec![String::from(krate)]))
    }

    /// Returns what `name` names in the module `module`: an item it declares,
    /// a name a `use` of it binds, or a name one of its globs imports.
    ///
    /// `from` is the module a glob imports into, which sees an item by its
    /// visibility, and `None` for a path the compiler already checked.
    fn in_module(
        &self,
        module: usize,
        name: &str,
        ns: Ns,
        from: Option<usize>,
        guard: &mut Guard,
    ) -> Option<Res> {
        let declared = &self.modules[module];
        for &index in &declared.items {
            let item = &self.items[index];
            if item.name != name || !self.visible(module, &item.vis, from) {
                continue;
            }
            let found = match (&item.kind, ns) {
                (Kind::Module(inner), Ns::Type) => Some(Res::Module(*inner)),
                (Kind::ExternCrate(krate), Ns::Type) => Some(self.crate_named(module, krate)),
                _ if ns == Ns::Type && item.is_type() => Some(Res::Item(index)),
                _ if ns == Ns::Value && item.is_value() => Some(Res::Item(index)),
                _ => None,
            };
            if found.is_some() {
                return found;
            }
        }
        for (position, declared_use) in declared.uses.iter().enumerate() {
            if declared_use.name.as_deref() != Some(name)
                || !self.visible(module, &declared_use.vis, from)
                || guard.open.contains(&(module, position))
            {
                continue;
            }
            guard.open.push((module, position));
            let found = self.walk(module, declared_use.leading, &declared_use.path, ns, guard);
            guard.open.pop();
            if let Ok(found) = found {
                return Some(found);
            }
        }
        for (position, declared_use) in declared.uses.iter().enumerate() {
            if declared_use.name.is_some()
                || !self.visible(module, &declared_use.vis, from)
                || guard.open.contains(&(module, position))
            {
                continue;
            }
            guard.open.push((module, position));
            let met = guard.globs.len();
            let target = self.walk(
                module,
                declared_use.leading,
                &declared_use.path,
                Ns::Type,
                guard,
            );
            let found = match target {
                Ok(target) => {
                    // A glob the lookup of this path met may import a
                    // segment of the path and not `name`, and the path
                    // resolved without it.
                    guard.globs.truncate(met);
                    let of = glob_of(&target);
                    let found = self.through_glob(target, name, ns, module, guard);
                    if let (None, Some(of)) = (&found, of) {
                        let glob = Glob {
                            path: glob_path(declared_use),
                            of,
                            place: declared_use.place.clone(),
                        };
                        if !guard.globs.contains(&glob) {
                            guard.globs.push(glob);
                        }
                    }
                    found
                }
                Err(_) => None,
            };
            guard.open.pop();
            if found.is_some() {
                return found;
            }
        }
        None
    }

    /// Returns what `name` names through a glob of `target` that the module
    /// `into` imports.
    ///
    /// A glob of `std`, `core`, `alloc` or the facade answers the names of
    /// their tables alone, and a glob of another crate answers none, since
    /// the reader does not know what else they export.
    fn through_glob(
        &self,
        target: Res,
        name: &str,
        ns: Ns,
        into: usize,
        guard: &mut Guard,
    ) -> Option<Res> {
        match target {
            Res::Module(inner) => self.in_module(inner, name, ns, Some(into), guard),
            Res::Item(item) => match &self.items[item].kind {
                Kind::Enum(variants) => variants
                    .variants
                    .iter()
                    .position(|variant| variant.name == name)
                    .map(|position| Res::Variant(item, position)),
                _ => None,
            },
            Res::Facade(path) => {
                let mut named = path;
                named.push(String::from(name));
                facade_exports(&named).then_some(Res::Facade(named))
            }
            Res::Std(path) => {
                let mut named = path;
                named.push(String::from(name));
                (std_role(&named) != Known::Foreign).then_some(Res::Std(named))
            }
            Res::Extern(_) | Res::Variant(..) | Res::Prim(_) => None,
        }
    }

    /// Returns whether an item of the module `owner` with the visibility
    /// `vis` is visible from the module `from`, every item when `from` is
    /// `None`.
    ///
    /// A private item is visible in its module and below it, an item of
    /// `pub(super)` in the parent of its module and below that, and any other
    /// in the whole crate, while another crate sees an item of `pub` alone.
    fn visible(&self, owner: usize, vis: &Vis, from: Option<usize>) -> bool {
        let Some(from) = from else {
            return true;
        };
        if self.modules[from].krate != self.modules[owner].krate {
            return *vis == Vis::Public;
        }
        match vis {
            Vis::Public | Vis::Crate | Vis::Restricted => true,
            Vis::Private => self.within(from, owner),
            Vis::Super => self.modules[owner]
                .parent
                .is_none_or(|parent| self.within(from, parent)),
        }
    }

    /// Returns whether the module `inner` is `outer` or stands below it.
    fn within(&self, inner: usize, outer: usize) -> bool {
        let mut at = Some(inner);
        while let Some(module) = at {
            if module == outer {
                return true;
            }
            at = self.modules[module].parent;
        }
        false
    }
}
