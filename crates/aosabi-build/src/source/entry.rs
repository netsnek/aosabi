// SPDX-License-Identifier: MIT
/*
 * aosabi-build: the entry of a crate and the roots it registers.
 */

//! The entry of a crate and its registrations.
//!
//! The entry is the one public function of the crate root whose return type is
//! an app: `impl IntoApp`, the builder `App`, or a `Result` of either. It has
//! any name and carries no attribute, and the reader finds it by its return
//! type. A second such function is refused with a message that names both.
//!
//! The reader reads the crate as the walk of the module `tree` strips it over
//! the active set of the build, as rustc strips it, so every item, `use`, glob,
//! module, statement, argument and arm it meets is one the build compiles, a
//! module whose file a `cfg_attr` names is read from that file, and a block, an
//! `if`, a `match` or a loop before a last statement that a `cfg` leaves out
//! answers its block, as rustc takes it.
//!
//! The reader reads the method chain the entry answers: `App::new()`, then
//! `.query(T)`, `.mutation(T)` and `.subscription(T)`, each of which registers
//! the type `T` as a part of its root, and `.compose(child())`, whose function
//! it reads the same way, the parent's roots first and the children's after them
//! in the order of the calls, as `compose` merges them. Each of these four calls
//! takes one argument, and the reader refuses a call of another count, which
//! rustc refuses as well.
//!
//! A root is a unit struct of the crate by its name, which is its value, a
//! struct literal, a constant or a static of a struct, or a call of `new` or
//! `default` of a struct where the source states an inherent function of that
//! name of the struct at the level of the items of a module, in a block whose
//! self type names the struct by what the reader sees, that answers `Self` or
//! the struct, is no `async fn` and is visible where the call stands. The
//! compiler takes such a function before a function of a trait in scope and
//! refuses a second inherent function of that name wherever a block, a body or a
//! macro states it. Without one, a function of a trait in scope may answer
//! another type, and so may an inherent function of an impl block in a body or
//! of a macro, which rustc takes even before `default` of `Default`, an inherent
//! function the call does not see leaves the call to a function of a trait, and
//! an `async fn` answers its future. A root is a value and no reference, since
//! the app keeps a part of the type of the argument, and the glue takes a part
//! of a struct of the crate alone.
//!
//! A chain may run through `let` bindings of the body and through assignments
//! that bind a name again, each read with the bindings before it, and it may
//! stand in `Ok(..)`, which the reader takes where its path resolves to the
//! variant of `Result` of the prelude, or of `std`, `core`, `alloc` or the
//! facade, as `Result::Ok`, and not where an item or a `use` of the crate, or
//! an item a macro call may make, gives the name another function. Every
//! other call of the builder on the chain registers a route, a plugin or an
//! option, which the app runs at run time, and the reader leaves it alone,
//! while a call of a method the builder does not hold, as a method of a trait
//! of the application, may answer another app, so the reader refuses it.
//!
//! A branch, a `match`, an `if` or a block, whose every arm continues the
//! chain of one binding with calls that register no root, or leaves the
//! function with a `return`, is read as the chain of that binding, since the
//! plugins and the options of its arms are calls the app runs at run time and
//! no root depends on the branch. It is the form of a configuration whose
//! plugins differ by a variant of the environment, as
//! `match variant { "a" => app.plugin(a()), _ => app }`, and a statement of an
//! arm before its chain, a call that sets a plugin up, registers nothing
//! either. An arm continues the binding of the body only where nothing of the
//! arm binds its name again, neither the pattern of the arm nor a `let` of its
//! block, and a labeled block that breaks to its label with a value answers an
//! app the reader does not follow.
//!
//! A statement may give a binding another app where the reader cannot follow
//! it, by an assignment inside a branch, a block, a loop or a closure, or
//! through a mutable borrow: `&mut` or `&raw mut` of the binding or of a part
//! of it, a `ref mut` binding of a pattern whose place is the binding, a
//! compound assignment, whose operator borrows its left side mutably, or a
//! call of a method the builder does not hold on the binding, which autoref
//! may borrow mutably, as `borrow_mut` of `BorrowMut`. Such a change ends the
//! reading of a chain that continues the binding across it, unless the
//! assignment gives the binding its own chain with calls that register no
//! root, as `app = app.plugin(p)` under an `if` or in a loop, which changes no
//! root.
//!
//! The resolver reads the names of the modules, so a name a function binds or
//! declares itself hides the name of its module from a path of its body: a
//! parameter, a type or a const parameter of its signature, an item or a `use`
//! of the body at any depth, a glob `use` hiding every name, and a `let` or an
//! item among the arguments of a macro call, while a `let` of the body hides
//! the function a call of `compose` names. The reader takes no path whose
//! first name such a name hides, in either namespace, the builder, `Ok`, `Err`,
//! a root and a composed function among them.
//!
//! The resolver reads the items the source of a module states and expands no
//! macro, while rustc takes an item a macro call at the level of the items makes
//! before a glob, an extern crate and the prelude. So in a module that holds
//! such a call, or a glob of such a module or of a crate whose source the reader
//! does not read, the reader takes the builder, `Ok`, `Err`, a root and a
//! composed function only where an item of the module or a name a `use` of it
//! binds by name gives each segment it looks up there its meaning, and nowhere a
//! `use` binds the segment to an item the resolver does not find. An attribute
//! or a derive of another crate, which may make an item as well, is read as
//! making none that hides a name of its module.
//!
//! The reader expands no macro. It reads the arguments of a macro call by the
//! same rules, as expressions separated by commas, the form `vec!`, `assert!`
//! and `format!` take, or else as statements, whose `let` binds its names again
//! where the call stands. A named argument after the format string of a macro
//! that formats binds no name, and `addr_of_mut!`, `write!` and `writeln!`
//! borrow their first argument mutably. Where the tokens parse as neither, the
//! reader reads them as a whole at the line of the macro call: a `return` among
//! them leaves with the value of the tokens after it up to a `;` or a `,`, a
//! `break` to a label with a value breaks out of it, tokens that assign, by `=`
//! alone or after the operator of a compound assignment, that hold `mut` or
//! `let`, or that call a method the builder does not hold, `write!`, `writeln!`
//! or `addr_of_mut!`, may change every name they hold, and tokens that hold a
//! `let` or the keyword of an item may bind or declare every name they hold.
//! What a macro does with its tokens beyond what they state, as a macro of the
//! application that assigns a binding it is handed, also under the name of a
//! macro of the standard library, or the source an `include!` names, stays
//! beyond the reading.
//!
//! A registration the reader cannot read from the source is refused with the
//! file and the line of the author: a root the entry chooses at run time, on
//! a branch whose arm registers a root or composes an app, whose arms continue
//! different bindings or a binding an arm binds again, or whose labeled block
//! breaks out with a value, at the line of the branch, through a `return` that
//! answers anything but an error of `Result`, wherever it stands in the body,
//! the arguments of a macro among them, at the line of the `return` or of the
//! macro call, through a change of a binding the chain continues, at the line
//! of the change, through a call of a method the builder does not hold, at the
//! line of the call, or through a path whose first name a name of the function
//! hides, or to which an item the reader cannot see may give another meaning,
//! at the line of the path, a call that registers a root or composes an app
//! with another count of arguments than one, at the line of the call, a
//! registration on a statement whose app the function drops, a root that is a
//! value rather than a type of the crate, a reference to a root, a root
//! written as a call of `new` or `default` of a struct that no inherent
//! function the source states answers, at the line of its registration, an
//! app that `.compose(..)` takes from anything but a call of a function of the
//! crate, and the form `App::new(graphql)`, whose roots a fragment of a
//! macro holds. The strip refuses a `cfg` with a key the reader cannot
//! evaluate and a `cfg` on an expression where rustc removes none at their
//! lines, as the module `cfg` states.

use std::path::Path;

use proc_macro2::{
    Delimiter,
    Spacing,
    Span,
    TokenStream,
    TokenTree, //
};
use syn::{
    BinOp,
    Expr,
    ExprCall,
    GenericArgument,
    Lit,
    Macro,
    Pat,
    PatIdent,
    PathArguments,
    PointerMutability,
    Stmt,
    Type,
    TypeParamBound,
    UnOp,
    UseTree,
    ext::IdentExt,
    punctuated::Punctuated,
    spanned::Spanned, //
};

use super::{
    Entry,
    Place,
    Refusal,
    Registration,
    RootKind,
    model::{
        Function,
        Item,
        Kind,
        Shape,
        Vis, //
    },
    reach::self_of,
    resolve::{
        Known,
        Ns,
        Res,
        Resolver,
        facade_role,
        std_role, //
    },
    tree::path_text, //
};

/// The entry of a crate, its registrations and its refusals.
#[derive(Debug, Default)]
pub(crate) struct Registered {
    /// The entry, the first of several.
    pub(crate) entry: Option<Entry>,
    /// The parts of the roots, the parent's first and the children's after.
    pub(crate) registrations: Vec<Registration>,
    /// The forms the reader refuses, in the order it meets them.
    pub(crate) refusals: Vec<Refusal>,
}

/// The reading of the chains of one crate.
struct Reader<'a> {
    /// The resolver of the crate.
    resolver: &'a Resolver<'a>,
    /// What the reading found so far.
    found: Registered,
    /// The functions whose chain the reading is inside, which stops a cycle
    /// of `compose`.
    open: Vec<usize>,
    /// The names the function whose body the reading is in binds or declares
    /// beside the bindings of its chain.
    hiding: Hiding,
}

/// The names a function binds or declares beside the bindings its chain
/// runs through, each of which hides a name of its module from the paths of
/// its body, while the resolver reads the names of the modules alone.
#[derive(Default)]
struct Hiding {
    /// The names of its parameters and of the type and the const parameters
    /// of its signature, of the items and the `use`s of its body at any depth,
    /// of the `let`s of the statements of a macro call, and of the tokens of a
    /// macro call that hold a `let` or an item.
    names: Vec<String>,
    /// Whether a glob `use` of its body may import any name.
    all: bool,
}

/// A statement of a body as the reading of a chain meets it.
enum Bound<'b> {
    /// A `let` or an assignment that binds the name again, with its value.
    Value(String, &'b Expr),
    /// A statement that may give the name another app, at the place of the
    /// change: an assignment inside a branch, a block, a loop, a closure or
    /// the arguments of a macro whose value continues no chain of the name,
    /// or a mutable borrow.
    Changed(String, Place),
}

impl Bound<'_> {
    /// Returns the name the statement binds or changes.
    fn name(&self) -> &str {
        match self {
            Bound::Value(name, _) | Bound::Changed(name, _) => name,
        }
    }
}

/// The bindings of a body and the changes of their names, in the order of the
/// body.
type Bindings<'b> = [Bound<'b>];

/// Finds the entry of the crate the resolver holds and reads its
/// registrations.
pub(crate) fn read(resolver: &Resolver<'_>) -> Registered {
    let mut reader = Reader {
        resolver,
        found: Registered::default(),
        open: Vec::new(),
        hiding: Hiding::default(),
    };
    let candidates: Vec<usize> = resolver.modules[0]
        .items
        .iter()
        .copied()
        .filter(|&index| {
            let item = &resolver.items[index];
            matches!(&item.kind, Kind::Function(function)
                if item.vis == Vis::Public
                    && function.output.as_ref().is_some_and(|output| reader.answers_app(output, 0)))
        })
        .collect();
    let Some((&first, others)) = candidates.split_first() else {
        return reader.found;
    };
    let entry = &resolver.items[first];
    for &other in others {
        let second = &resolver.items[other];
        reader.refuse(
            second.place.clone(),
            format!(
                "the crate root holds two entries, `{}` at {} and `{}` at {}, each a public function that answers an app, and a crate has one entry. Keep one of them public, and make the other private or move it into a module",
                entry.name, entry.place, second.name, second.place
            ),
        );
    }
    let result = match &entry.kind {
        Kind::Function(function) => function
            .output
            .as_ref()
            .is_some_and(|output| reader.answers_result(output, 0)),
        _ => false,
    };
    reader.found.entry = Some(Entry {
        function: entry.name.clone(),
        place: entry.place.clone(),
        result,
    });
    reader.function(first, &[]);
    reader.found
}

impl Reader<'_> {
    /// Adds the refusal `message` at `place`.
    fn refuse(&mut self, place: Place, message: String) {
        self.found.refusals.push(Refusal { place, message });
    }

    /// Returns whether `ty`, written in the module `module`, is an app:
    /// `impl IntoApp`, the builder `App`, or a `Result` of either.
    fn answers_app(&self, ty: &Type, module: usize) -> bool {
        match ty {
            Type::Paren(inner) => self.answers_app(&inner.elem, module),
            Type::Group(inner) => self.answers_app(&inner.elem, module),
            Type::ImplTrait(bounds) => bounds.bounds.iter().any(|bound| match bound {
                TypeParamBound::Trait(bound) => {
                    let segments = Resolver::segments(&bound.path);
                    let leading = bound.path.leading_colon.is_some();
                    matches!(self.resolver.resolve(module, leading, &segments, Ns::Type),
                        Ok(Res::Facade(path)) if facade_role(&path) == Known::IntoApp)
                }
                _ => false,
            }),
            Type::Path(path) if path.qself.is_none() => {
                let segments = Resolver::segments(&path.path);
                let leading = path.path.leading_colon.is_some();
                let result = match self.resolver.resolve(module, leading, &segments, Ns::Type) {
                    Ok(Res::Facade(named)) => match facade_role(&named) {
                        Known::Builder => return true,
                        Known::Wrap(0) => named.last().is_some_and(|last| last == "Result"),
                        _ => false,
                    },
                    Ok(Res::Std(named)) => {
                        std_role(&named) == Known::Wrap(0)
                            && named.last().is_some_and(|last| last == "Result")
                    }
                    _ => false,
                };
                result
                    && first_type_argument(&path.path)
                        .is_some_and(|argument| self.answers_app(argument, module))
            }
            _ => false,
        }
    }

    /// Returns whether `ty`, written in the module `module`, is a `Result`,
    /// of the standard library or of the facade.
    fn answers_result(&self, ty: &Type, module: usize) -> bool {
        match ty {
            Type::Paren(inner) => self.answers_result(&inner.elem, module),
            Type::Group(inner) => self.answers_result(&inner.elem, module),
            Type::Path(path) if path.qself.is_none() => {
                let segments = Resolver::segments(&path.path);
                let leading = path.path.leading_colon.is_some();
                match self.resolver.resolve(module, leading, &segments, Ns::Type) {
                    Ok(Res::Facade(named) | Res::Std(named)) => {
                        named.last().is_some_and(|last| last == "Result")
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }

    /// Reads the chain the function `index` answers, through the calls of
    /// `compose` named by `through`.
    fn function(&mut self, index: usize, through: &[String]) {
        if self.open.contains(&index) {
            return;
        }
        let resolver = self.resolver;
        let item = &resolver.items[index];
        let Kind::Function(function) = &item.kind else {
            return;
        };
        self.open.push(index);
        let file = item.place.file.clone();
        let (bindings, tail) = self.body(item, function, &file);
        if let Some(tail) = tail {
            let mut children = Vec::new();
            self.chain(tail, item, &bindings, through, &mut children, 0);
            for (child, path) in children {
                let mut deeper = through.to_vec();
                deeper.push(path);
                self.function(child, &deeper);
            }
        }
        self.open.pop();
    }

    /// Returns the bindings of the body of `function` and the expression it
    /// answers, refusing every exit of the body that answers anything but an
    /// error of `Result`, wherever it stands, and every registration on a
    /// statement whose app the function drops.
    ///
    /// Each statement adds the changes it makes to a name before what it
    /// binds, since its value runs before the name is bound. The names the
    /// function binds or declares beside its bindings become the hiding of the
    /// reading, which the reading of its chain and its exits take.
    fn body<'f>(
        &mut self,
        item: &Item,
        function: &'f Function,
        file: &Path,
    ) -> (Vec<Bound<'f>>, Option<&'f Expr>) {
        self.hiding = hiding(function);
        let mut bindings = Vec::new();
        let mut tail = None;
        let statements = &function.block.stmts;
        for (position, statement) in statements.iter().enumerate() {
            let last = position + 1 == statements.len();
            let mut effects = Effects::default();
            match statement {
                // The `return` of the last statement answers the chain, and an
                // assignment that binds a name again is a binding.
                Stmt::Expr(Expr::Return(answer), _) if last => {
                    effects.maybe(answer.expr.as_deref(), false);
                }
                Stmt::Expr(Expr::Assign(assign), _) if plain_name(&assign.left).is_some() => {
                    effects.expr(&assign.right, false);
                }
                _ => effects.statement(statement, false),
            }
            for exit in &effects.exits {
                if exit
                    .call
                    .as_ref()
                    .is_some_and(|path| self.names_variant(path, item.module, "Err"))
                {
                    continue;
                }
                self.refuse(
                    at(file, exit.span),
                    format!(
                        "the entry `{}` answers an app on a branch, a root chosen at run time, which the builder cannot read from the source. Register every root on the one chain the function answers, and choose at run time inside a resolver",
                        item.name
                    ),
                );
            }
            bindings.extend(effects.changed(file));
            match statement {
                Stmt::Local(local) => {
                    if let Some(init) = &local.init {
                        let mut names = Vec::new();
                        pattern_names(&local.pat, &mut names);
                        for name in names {
                            bindings.push(Bound::Value(name, &init.expr));
                        }
                    }
                }
                Stmt::Expr(Expr::Return(answer), _) if last => tail = answer.expr.as_deref(),
                Stmt::Expr(expr, None) if last => tail = Some(expr),
                Stmt::Expr(Expr::Assign(assign), _) => {
                    if let Some(name) = plain_name(&assign.left) {
                        bindings.push(Bound::Value(name, &assign.right));
                    }
                }
                Stmt::Expr(Expr::MethodCall(call), _) if registers(&call.method.to_string()) => {
                    self.refuse(
                        at(file, call.method.span()),
                        format!(
                            "`.{}(..)` registers on a statement of `{}` whose app the function drops, so its root reaches no app. Register every root on the one chain the function answers",
                            call.method, item.name
                        ),
                    );
                }
                _ => {}
            }
        }
        (bindings, tail)
    }

    /// Reads the chain `expr` in the function `item`, whose bindings are
    /// `bindings`, and adds each child of `compose` to `children` with the
    /// path of its function.
    ///
    /// Returns whether the chain opens with `App::new()`.
    fn chain(
        &mut self,
        expr: &Expr,
        item: &Item,
        bindings: &Bindings<'_>,
        through: &[String],
        children: &mut Vec<(usize, String)>,
        depth: usize,
    ) -> bool {
        let file = item.place.file.clone();
        match expr {
            Expr::Paren(inner) => self.chain(&inner.expr, item, bindings, through, children, depth),
            Expr::Group(inner) => self.chain(&inner.expr, item, bindings, through, children, depth),
            Expr::Try(inner) => self.chain(&inner.expr, item, bindings, through, children, depth),
            Expr::Call(call) => self.call(call, item, bindings, through, children, depth),
            Expr::MethodCall(call) => {
                if !self.chain(&call.receiver, item, bindings, through, children, depth) {
                    return false;
                }
                let method = call.method.to_string();
                let place = at(&file, call.method.span());
                if !BUILDER_METHODS.contains(&method.as_str()) {
                    self.refuse(
                        place,
                        format!(
                            "`.{method}(..)` on the chain of `{}` is no call of the builder `App`, so the app it answers may hold roots the builder cannot read from the source. Answer one chain of the calls of the builder, and call any other method inside a resolver or a plugin",
                            item.name
                        ),
                    );
                    return false;
                }
                let kind = match method.as_str() {
                    "query" => Some(RootKind::Query),
                    "mutation" => Some(RootKind::Mutation),
                    "subscription" => Some(RootKind::Subscription),
                    _ => None,
                };
                // Each of these calls takes one argument, which the strip
                // leaves as rustc compiles it.
                if registers(&method) && call.args.len() != 1 {
                    self.refuse(
                        place,
                        format!(
                            "`.{method}(..)` on the chain of `{}` takes another count of arguments than one, which the builder takes no call of, so the builder cannot read from the source what the call registers. Write one argument, as `.query(Query)` or `.compose(admin())`",
                            item.name
                        ),
                    );
                } else if let Some(kind) = kind {
                    self.root(kind, &call.args[0], item, bindings, place, through);
                } else if method == "compose" {
                    self.compose(&call.args[0], item, bindings, place, children);
                }
                true
            }
            Expr::Path(path)
                if path.qself.is_none() && path.path.get_ident().is_some() && depth < 16 =>
            {
                let name = path.path.segments[0].ident.to_string();
                // A binding sees the bindings before it alone, so an
                // assignment `app = app.query(Query)` reads the `app` it
                // replaces.
                let found = bindings
                    .iter()
                    .enumerate()
                    .rev()
                    .find_map(|(at, bound)| match bound {
                        Bound::Value(bound, value) if *bound == name => Some((at, *value)),
                        _ => None,
                    });
                if let Some((at, value)) = found {
                    if self.changed(&bindings[at + 1..], &name, item) {
                        return false;
                    }
                    return self.chain(value, item, &bindings[..at], through, children, depth + 1);
                }
                self.runtime(expr, item);
                false
            }
            Expr::Match(_) | Expr::If(_) | Expr::Block(_) if depth < 16 => {
                // A branch whose arms add plugins and options to one chain is
                // read as that chain.
                let mut bases = Vec::new();
                if arms_continue(expr, &[], &mut bases)
                    && let Some((first, rest)) = bases.split_first()
                    && rest.iter().all(|base| same_binding(base, first))
                {
                    return self.chain(first, item, bindings, through, children, depth + 1);
                }
                self.runtime(expr, item);
                false
            }
            _ => {
                self.runtime(expr, item);
                false
            }
        }
    }

    /// Reads the call `call` on the chain of the function `item`, as
    /// [`Reader::chain`] reads an expression: `Ok(..)` of `Result`, whose
    /// argument it reads, and `App::new()`, which opens the chain.
    ///
    /// A call of a path that a name of the function hides, or to which an item
    /// the reader cannot see may give another meaning, and any other call,
    /// answers an app the reader does not read.
    fn call(
        &mut self,
        call: &ExprCall,
        item: &Item,
        bindings: &Bindings<'_>,
        through: &[String],
        children: &mut Vec<(usize, String)>,
        depth: usize,
    ) -> bool {
        let file = item.place.file.clone();
        let func = match &*call.func {
            Expr::Path(func) if func.qself.is_none() => Some(&func.path),
            _ => None,
        };
        if let Some(name) = func.and_then(|func| self.hidden(func)) {
            self.refuse_hidden(at(&file, call.span()), &name, item);
            return false;
        }
        let builder = self.is_builder_new(&call.func, item.module);
        if builder && !call.args.is_empty() {
            self.refuse(
                at(&file, call.span()),
                String::from(
                    "`App::new(..)` registers the roots of a fragment, which a macro expands and the builder cannot read from the source. Register each root on the chain of `App::new()`, as `App::new().query(Query)`",
                ),
            );
            return false;
        }
        let ok = call.args.len() == 1
            && func.is_some_and(|func| self.resolves_variant(func, item.module, "Ok"));
        if (builder || ok)
            && let Some(name) = func.and_then(|func| self.unseen(func, item.module, Ns::Value))
        {
            self.refuse_unseen(at(&file, call.span()), &name, item);
            return false;
        }
        if ok {
            return self.chain(&call.args[0], item, bindings, through, children, depth);
        }
        if !builder {
            self.unread(call.span(), item);
            return false;
        }
        true
    }

    /// Returns whether `path`, written in the module `module`, names the
    /// variant `variant` of `Result`: the name of the prelude, or a path into
    /// the `Result` of `std`, `core`, `alloc` or the facade, by what the
    /// resolver finds, so an item or a `use` of the crate of that name names
    /// no variant, nor does a path whose first name the function hides, or to
    /// which an item the reader cannot see may give another meaning.
    fn names_variant(&self, path: &syn::Path, module: usize, variant: &str) -> bool {
        self.hidden(path).is_none()
            && self.unseen(path, module, Ns::Value).is_none()
            && self.resolves_variant(path, module, variant)
    }

    /// Returns whether the resolver resolves `path`, written in the module
    /// `module`, to the variant `variant` of `Result` of the prelude, of
    /// `std`, `core` or `alloc`, or of the facade.
    fn resolves_variant(&self, path: &syn::Path, module: usize, variant: &str) -> bool {
        let segments = Resolver::segments(path);
        let leading = path.leading_colon.is_some();
        match self.resolver.resolve(module, leading, &segments, Ns::Value) {
            Ok(Res::Std(named)) => named.last().is_some_and(|last| last == variant),
            Ok(Res::Facade(named)) => {
                matches!(named.as_slice(), [.., result, last] if result == "Result" && last == variant)
            }
            _ => false,
        }
    }

    /// Returns the first name of `path` where a name the function binds or
    /// declares in its signature or its body hides it, in either namespace,
    /// so the resolver, which reads the names of the modules, cannot tell what
    /// it names.
    ///
    /// A path that opens with `::`, `crate`, `self` or `super` names no name
    /// of the function.
    fn hidden(&self, path: &syn::Path) -> Option<String> {
        if path.leading_colon.is_some() {
            return None;
        }
        let first = path.segments.first()?.ident.unraw().to_string();
        if matches!(first.as_str(), "crate" | "self" | "super") {
            return None;
        }
        (self.hiding.all || self.hiding.names.contains(&first)).then_some(first)
    }

    /// Returns the segment of `path`, written in the module `module` and read
    /// in the namespace `ns`, to which an item the reader cannot see may give
    /// another meaning than the resolver finds.
    fn unseen(&self, path: &syn::Path, module: usize, ns: Ns) -> Option<String> {
        let segments = Resolver::segments(path);
        self.unseen_at(
            &segments,
            path.leading_colon.is_some(),
            module,
            ns,
            0,
            false,
        )
    }

    /// Returns the segment of the path of `segments`, written in the module
    /// `module`, `leading` where it opens with `::`, and read in the namespace
    /// `ns`, to which an item the reader cannot see may give another meaning,
    /// `depth` counting the `use`s and globs the walk follows, and `import`
    /// where the path is the path of a `use`.
    ///
    /// Each segment the resolver looks up in a module of a crate it reads is one
    /// where the reader may not see what gives it its meaning there,
    /// [`Reader::hides`]. A path that opens with `::` names an extern crate
    /// first, which no item of a module hides, and so does the first segment of
    /// the path of a `use` that the resolver resolves to an extern crate, since
    /// rustc refuses a `use` whose first name an item of the module may give
    /// another meaning as ambiguous.
    fn unseen_at(
        &self,
        segments: &[String],
        leading: bool,
        module: usize,
        ns: Ns,
        depth: usize,
        import: bool,
    ) -> Option<String> {
        let mut inside = (!leading).then_some(module);
        for (position, segment) in segments.iter().enumerate() {
            let last = position + 1 == segments.len();
            if let Some(at) = inside
                && !matches!(segment.as_str(), "crate" | "self" | "super")
                && !(import && position == 0 && self.names_crate(at, segment))
                && self.hides(at, segment, if last { ns } else { Ns::Type }, depth)
            {
                return Some(segment.clone());
            }
            if last {
                break;
            }
            inside = match self
                .resolver
                .resolve(module, leading, &segments[..=position], Ns::Type)
            {
                Ok(Res::Module(inner)) => Some(inner),
                _ => None,
            };
        }
        None
    }

    /// Returns whether an item the reader cannot see may give `name` its
    /// meaning in the module `module` and the namespace `ns`, `depth` counting
    /// the `use`s and globs the walk follows.
    fn hides(&self, module: usize, name: &str, ns: Ns, depth: usize) -> bool {
        self.hides_in(module, name, ns, depth, &mut Vec::new())
    }

    /// Returns whether an item the reader cannot see may give `name` its
    /// meaning in the module `module` and the namespace `ns`, as
    /// [`Reader::hides`] does, `visited` holding the modules whose globs the
    /// walk already met.
    ///
    /// The reader sees the meaning where an item of the module gives it, which
    /// rustc takes first, and where a `use` of the module binds the name to
    /// what the resolver resolves through a path the reader sees segment by
    /// segment. A `use` that binds it by name to what the resolver resolves in
    /// no namespace, such as an item a macro call makes, hides it. Otherwise a
    /// glob, an extern crate or the prelude gives it its meaning, which the
    /// reader sees where the module and every module its globs import from give
    /// it so, [`Reader::whole`], while an item a macro call of the module makes,
    /// or a glob imports from a module the reader does not see so, would hide
    /// it.
    fn hides_in(
        &self,
        module: usize,
        name: &str,
        ns: Ns,
        depth: usize,
        visited: &mut Vec<usize>,
    ) -> bool {
        if depth > UNSEEN_DEPTH {
            return true;
        }
        let declared = &self.resolver.modules[module];
        let named = |index: usize| {
            let item = &self.resolver.items[index];
            item.name == name
                && match ns {
                    Ns::Type => {
                        item.is_type()
                            || matches!(item.kind, Kind::Module(_) | Kind::ExternCrate(_))
                    }
                    Ns::Value => item.is_value(),
                }
        };
        if declared.items.iter().any(|&index| named(index)) {
            return false;
        }
        let mut hidden = false;
        for import in declared
            .uses
            .iter()
            .filter(|import| import.name.as_deref() == Some(name))
        {
            if self
                .resolver
                .resolve(module, import.leading, &import.path, ns)
                .is_ok()
            {
                return self
                    .unseen_at(&import.path, import.leading, module, ns, depth + 1, true)
                    .is_some();
            }
            hidden |= [Ns::Type, Ns::Value].into_iter().all(|other| {
                self.resolver
                    .resolve(module, import.leading, &import.path, other)
                    .is_err()
            });
        }
        hidden || !self.whole(module, name, ns, depth, visited)
    }

    /// Returns whether the reader sees every item that may give the name
    /// `name` of the module `module` its meaning in the namespace `ns` through
    /// a glob, an extern crate or the prelude: the module holds no macro call
    /// at the level of its items, whose items the reader does not expand, and
    /// every glob of it imports from `std`, `core`, `alloc` or the facade,
    /// whose names the reader knows, from an enum, or from a module in which
    /// the reader sees what gives `name` its meaning, [`Reader::hides_in`],
    /// and from no crate whose source the reader does not read.
    ///
    /// The walk counts the `use`s and globs it follows in `depth`, and holds
    /// the modules the walk of the globs already met in `visited`.
    fn whole(
        &self,
        module: usize,
        name: &str,
        ns: Ns,
        depth: usize,
        visited: &mut Vec<usize>,
    ) -> bool {
        if visited.contains(&module) {
            return true;
        }
        if depth > UNSEEN_DEPTH {
            return false;
        }
        visited.push(module);
        let declared = &self.resolver.modules[module];
        if declared.items.iter().any(|&index| {
            matches!(&self.resolver.items[index].kind, Kind::Macro(call) if call.defines.is_none())
        }) {
            return false;
        }
        for glob in declared.uses.iter().filter(|glob| glob.name.is_none()) {
            match self
                .resolver
                .resolve(module, glob.leading, &glob.path, Ns::Type)
            {
                Ok(Res::Module(inner)) => {
                    if self.hides_in(inner, name, ns, depth + 1, visited) {
                        return false;
                    }
                }
                Ok(Res::Std(_) | Res::Facade(_) | Res::Item(_)) => {}
                _ => return false,
            }
        }
        true
    }

    /// Returns whether the resolver resolves the first segment `name` of a
    /// path written in the module `module` to an extern crate: `std`, `core`,
    /// `alloc`, the facade, a crate whose source the reader does not read, or
    /// the root of another crate it reads.
    fn names_crate(&self, module: usize, name: &str) -> bool {
        let segments = [String::from(name)];
        match self.resolver.resolve(module, false, &segments, Ns::Type) {
            Ok(Res::Std(named) | Res::Facade(named)) => named.is_empty(),
            Ok(Res::Extern(named)) => named.len() == 1,
            Ok(Res::Module(inner)) => {
                let krate = self.resolver.modules[inner].krate;
                krate != self.resolver.modules[module].krate
                    && self.resolver.libraries[krate].root == inner
            }
            _ => false,
        }
    }

    /// Refuses at `place` the name `name` of the function `item`, which a
    /// name its body binds or declares hides.
    fn refuse_hidden(&mut self, place: Place, name: &str, item: &Item) {
        self.refuse(
            place,
            format!(
                "`{name}` in `{}` names what the function binds or declares itself, a parameter, a type or a const parameter of its signature, a binding, or an item, a `use` or a `let` of its body or of the arguments of a macro call, which hides the name of its module in either namespace, so the builder cannot read from the source what the path names. Declare the item in a module of the crate, and give the parameter or the binding another name",
                item.name
            ),
        );
    }

    /// Refuses at `place` the name `name` of the function `item`, to which an
    /// item the reader cannot see may give another meaning.
    fn refuse_unseen(&mut self, place: Place, name: &str, item: &Item) {
        self.refuse(
            place,
            format!(
                "`{name}` in `{}` may name an item the builder cannot see, since the module it is looked up in holds a macro call at the level of its items, or a glob of such a module or of a crate whose source the builder does not read, and neither an item nor a `use` of that module names it, or a `use` of that module names an item the builder cannot see, so the builder cannot read from the source what the path names. Name the item by a `use` of the module, as `use aosabi::prelude::App;` or `use core::result::Result::{{Err, Ok}};`, or call the macro in another module",
                item.name
            ),
        );
    }

    /// Refuses the expression `expr` of the function `item`, which answers no
    /// chain the reader reads.
    fn runtime(&mut self, expr: &Expr, item: &Item) {
        if !matches!(expr, Expr::If(_) | Expr::Match(_)) {
            self.unread(expr.span(), item);
            return;
        }
        self.refuse(
            at(&item.place.file, expr.span()),
            format!(
                "`{}` chooses its roots on a branch, a root chosen at run time, which the builder cannot read from the source. Register every root on the one chain the function answers, and choose at run time inside a resolver",
                item.name
            ),
        );
    }

    /// Refuses the expression at `span` of the function `item`, which answers
    /// an app through an expression the reader does not read.
    fn unread(&mut self, span: Span, item: &Item) {
        self.refuse(
            at(&item.place.file, span),
            format!(
                "`{}` answers an app through an expression the builder cannot read from the source. Answer one chain that opens with `App::new()` and registers every root",
                item.name
            ),
        );
    }

    /// Refuses every change of the binding `name` among `after`, the
    /// statements between the binding and the chain of the function `item`
    /// that continues it, and returns whether there is one.
    fn changed(&mut self, after: &Bindings<'_>, name: &str, item: &Item) -> bool {
        let mut changed = false;
        for bound in after {
            match bound {
                Bound::Changed(bound, place) if bound == name => {
                    changed = true;
                    self.refuse(
                        place.clone(),
                        format!(
                            "the binding `{name}` of `{}` takes another app in a statement the builder cannot follow, an assignment inside a branch, a block, a loop, a closure or the arguments of a macro, or a mutable borrow through `&mut`, `ref mut`, a compound assignment or a method the builder does not hold, a root chosen at run time, which the builder cannot read from the source. Assign `{name}` only its own chain with calls that register no root, as `{name} = {name}.plugin(..)`, and choose at run time inside a resolver",
                            item.name
                        ),
                    );
                }
                _ => {}
            }
        }
        changed
    }

    /// Returns whether `func` names `new` or `default` of the builder of the
    /// facade, the two ways to make an empty app.
    fn is_builder_new(&self, func: &Expr, module: usize) -> bool {
        let Expr::Path(path) = func else {
            return false;
        };
        let segments = Resolver::segments(&path.path);
        let Some((last, ty)) = segments.split_last() else {
            return false;
        };
        matches!(last.as_str(), "new" | "default")
            && !ty.is_empty()
            && matches!(
                self.resolver.resolve(module, path.path.leading_colon.is_some(), ty, Ns::Type),
                Ok(Res::Facade(named)) if facade_role(&named) == Known::Builder
            )
    }

    /// Reads the root `argument` that a call of `kind` at `place` registers in
    /// the function `item`.
    fn root(
        &mut self,
        kind: RootKind,
        argument: &Expr,
        item: &Item,
        bindings: &Bindings<'_>,
        place: Place,
        through: &[String],
    ) {
        if let Some(name) = root_path(argument).and_then(|(path, _)| self.hidden(path)) {
            self.refuse_hidden(place, &name, item);
            return;
        }
        // The app keeps a part of the type of the argument, so a reference to
        // a root is a part of a reference type, which the glue of no struct
        // of the crate takes.
        if referenced(argument) {
            self.refuse(
                place,
                format!(
                    "the root that `.{}(..)` registers is a reference, which the app keeps as a part of a reference type that the glue of no struct of the crate takes, so the app fails at its boot. Register the struct by its value, as `.{}(Query)`",
                    kind.as_str(),
                    kind.as_str()
                ),
            );
            return;
        }
        match self.root_type(argument, item.module, bindings) {
            Some(root) => {
                if let Some(name) =
                    root_path(argument).and_then(|(path, ns)| self.unseen(path, item.module, ns))
                {
                    self.refuse_unseen(place, &name, item);
                    return;
                }
                if let Some(path) = constructor(argument)
                    && !self.constructs(root, path, item.module)
                {
                    self.refuse_constructor(place, path, root, item);
                    return;
                }
                self.found.registrations.push(Registration {
                    kind,
                    path: self.path_of(root),
                    place,
                    through: through.to_vec(),
                    item: root,
                });
            }
            None => self.refuse(
                place,
                format!(
                    "the root that `.{}(..)` registers is a value the builder cannot read from the source, such as a root chosen at run time. Register a unit struct of the crate by its name, as `.{}(Query)`, or a struct by its literal",
                    kind.as_str(),
                    kind.as_str()
                ),
            ),
        }
    }

    /// Returns the struct of the crate the root `argument` names, written in
    /// the module `module`: the name of a unit struct, which is its value, a
    /// struct literal, a call of `default` or `new` of a struct with no
    /// argument, whose function [`Reader::constructs`] reads, or a constant or
    /// a static of such a type.
    ///
    /// The name of a struct of a tuple is its constructor, a function and no
    /// value of the struct, so it names no root.
    fn root_type(&self, argument: &Expr, module: usize, bindings: &Bindings<'_>) -> Option<usize> {
        match argument {
            Expr::Paren(inner) => self.root_type(&inner.expr, module, bindings),
            Expr::Group(inner) => self.root_type(&inner.expr, module, bindings),
            Expr::Path(path) if path.qself.is_none() => {
                if let Some(ident) = path.path.get_ident() {
                    let name = ident.to_string();
                    if bindings.iter().any(|bound| bound.name() == name) {
                        return None;
                    }
                }
                let segments = Resolver::segments(&path.path);
                let leading = path.path.leading_colon.is_some();
                match self
                    .resolver
                    .resolve(module, leading, &segments, Ns::Value)
                    .ok()?
                {
                    Res::Item(index) => match &self.resolver.items[index].kind {
                        Kind::Struct(data) if data.shape == Shape::Unit => Some(index),
                        Kind::Value(ty) => self.struct_of(ty, self.resolver.items[index].module),
                        _ => None,
                    },
                    _ => None,
                }
            }
            Expr::Struct(literal) if literal.qself.is_none() => {
                self.struct_at(&literal.path, module)
            }
            Expr::Call(call) if call.args.is_empty() => {
                let Expr::Path(path) = &*call.func else {
                    return None;
                };
                if path.qself.is_some() {
                    return None;
                }
                let segments = Resolver::segments(&path.path);
                let (last, ty) = segments.split_last()?;
                if !matches!(last.as_str(), "default" | "new") || ty.is_empty() {
                    return None;
                }
                let leading = path.path.leading_colon.is_some();
                match self.resolver.resolve(module, leading, ty, Ns::Type).ok()? {
                    Res::Item(index)
                        if matches!(self.resolver.items[index].kind, Kind::Struct(_)) =>
                    {
                        Some(index)
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// Returns the struct of the crate the type `ty` of a constant names.
    fn struct_of(&self, ty: &Type, module: usize) -> Option<usize> {
        match ty {
            Type::Path(path) if path.qself.is_none() => self.struct_at(&path.path, module),
            _ => None,
        }
    }

    /// Returns the struct of the crate the path `path` names in the type
    /// namespace of the module `module`.
    fn struct_at(&self, path: &syn::Path, module: usize) -> Option<usize> {
        let segments = Resolver::segments(path);
        match self
            .resolver
            .resolve(module, path.leading_colon.is_some(), &segments, Ns::Type)
            .ok()?
        {
            Res::Item(index) if matches!(self.resolver.items[index].kind, Kind::Struct(_)) => {
                Some(index)
            }
            _ => None,
        }
    }

    /// Returns whether the call of the function `path`, `new` or `default` of
    /// the struct `owner`, written in the module `from`, answers `owner` by
    /// what the source states: an inherent function of that name of the
    /// struct at the level of the items of a module, which answers `Self` or
    /// the struct, is no `async fn`, stands in a block whose self type names
    /// the struct by what the reader sees, and is visible from `from`.
    ///
    /// The compiler takes an inherent function the call sees before a function
    /// of a trait in scope and refuses a second inherent function of one name,
    /// whatever block, body or macro states it, so such a stated function is the
    /// one the call names. Without one, a function of a trait in scope may
    /// answer another type, and so may an inherent function of an impl block in
    /// a body or of a macro, which rustc takes before `default` of `Default` as
    /// well, an inherent function the call does not see leaves the call to a
    /// function of a trait, and an `async fn` answers its future, a part of no
    /// type of the crate.
    fn constructs(&self, owner: usize, path: &syn::Path, from: usize) -> bool {
        let Some(last) = path.segments.last() else {
            return false;
        };
        let name = last.ident.unraw().to_string();
        let resolver = self.resolver;
        let mut stated = false;
        for block_item in resolver.items {
            let Kind::Impl(block) = &block_item.kind else {
                continue;
            };
            if block.trait_.is_some()
                || self_of(resolver, block, block_item.module).is_none_or(|(of, _)| of != owner)
            {
                continue;
            }
            for method in block.methods.iter().filter(|method| method.name == name) {
                stated = true;
                let function = &method.function;
                let generics = [block.generics.as_slice(), function.generics.as_slice()];
                let answers = function.receiver.is_none()
                    && !function.asyncness
                    && function.output.as_ref().is_some_and(|output| {
                        self.answers_self(output, owner, block_item.module, &generics)
                    });
                if !answers
                    || self.unseen_type(&block.self_ty, block_item.module)
                    || !self.visible(&method.vis, block_item.module, from)
                {
                    return false;
                }
            }
        }
        stated
    }

    /// Returns whether the return type `output` of a function of an inherent
    /// block of the struct `owner`, written in the module `module`, is the
    /// struct: `Self`, or a path to it whose first name no type parameter of
    /// `generics` binds and to which no item the reader cannot see may give
    /// another meaning.
    fn answers_self(
        &self,
        output: &Type,
        owner: usize,
        module: usize,
        generics: &[&[String]],
    ) -> bool {
        match output {
            Type::Paren(inner) => self.answers_self(&inner.elem, owner, module, generics),
            Type::Group(inner) => self.answers_self(&inner.elem, owner, module, generics),
            Type::Path(path) if path.qself.is_none() => {
                if path.path.is_ident("Self") {
                    return true;
                }
                let parameter = path.path.leading_colon.is_none()
                    && path.path.segments.first().is_some_and(|first| {
                        let first = first.ident.unraw().to_string();
                        generics.iter().any(|names| names.contains(&first))
                    });
                !parameter
                    && self.unseen(&path.path, module, Ns::Type).is_none()
                    && self.struct_at(&path.path, module) == Some(owner)
            }
            _ => false,
        }
    }

    /// Returns whether an item the reader cannot see may give the self type
    /// `ty` of an impl block of the module `module` another meaning than the
    /// resolver finds, so the block may belong to another type.
    fn unseen_type(&self, ty: &Type, module: usize) -> bool {
        match ty {
            Type::Paren(inner) => self.unseen_type(&inner.elem, module),
            Type::Group(inner) => self.unseen_type(&inner.elem, module),
            Type::Path(path) if path.qself.is_none() => {
                self.unseen(&path.path, module, Ns::Type).is_some()
            }
            _ => true,
        }
    }

    /// Returns whether a member of the visibility `vis` of an impl block of
    /// the module `declared` is visible from the module `from`, as the Rust
    /// Reference, chapter Visibility and privacy, scopes it. A path of
    /// `pub(in path)` counts as visible in the module of the block and below
    /// it alone, which is no more than the compiler lets it reach.
    fn visible(&self, vis: &Vis, declared: usize, from: usize) -> bool {
        let modules = self.resolver.modules;
        let scope = match vis {
            Vis::Public => return true,
            Vis::Crate => return modules[declared].krate == modules[from].krate,
            Vis::Private | Vis::Restricted => declared,
            Vis::Super => modules[declared].parent.unwrap_or(declared),
        };
        let mut at = Some(from);
        while let Some(module) = at {
            if module == scope {
                return true;
            }
            at = modules[module].parent;
        }
        false
    }

    /// Refuses at `place` the root `path()` of the function `item`, a call of
    /// `new` or `default` of the struct `owner` that no inherent function the
    /// source states answers.
    fn refuse_constructor(&mut self, place: Place, path: &syn::Path, owner: usize, item: &Item) {
        let named = &self.resolver.items[owner].name;
        let function = path
            .segments
            .last()
            .map(|last| last.ident.unraw().to_string())
            .unwrap_or_default();
        let message = format!(
            "`{}()` in `{}` calls a function the builder cannot read as one that answers `{named}`, since the source states no inherent `{function}` of `{named}` at the level of the items of a module that answers `Self` or `{named}`, is no `async fn` and is visible where the call stands, while a function of a trait in scope, of an impl block in a body or of a macro may answer another type. Register a unit struct by its name, as `.query(Query)`, or a struct by its literal, or give `{named}` an inherent `pub fn new() -> Self` and call it",
            path_text(path),
            item.name
        );
        self.refuse(place, message);
    }

    /// Reads the child `argument` that a call of `compose` at `place` takes in
    /// the function `item`, whose bindings are `bindings`, and adds its
    /// function to `children`.
    fn compose(
        &mut self,
        argument: &Expr,
        item: &Item,
        bindings: &Bindings<'_>,
        place: Place,
        children: &mut Vec<(usize, String)>,
    ) {
        // A `let` of the body hides the function of the module of its name.
        if let Expr::Call(call) = argument
            && let Expr::Path(path) = &*call.func
            && path.qself.is_none()
            && let Some(name) = self.hidden(&path.path).or_else(|| {
                let name = path.path.get_ident()?.to_string();
                bindings
                    .iter()
                    .any(|bound| matches!(bound, Bound::Value(bound, _) if *bound == name))
                    .then_some(name)
            })
        {
            self.refuse_hidden(place, &name, item);
            return;
        }
        let child = match argument {
            Expr::Call(call) => match &*call.func {
                Expr::Path(path) if path.qself.is_none() => {
                    let segments = Resolver::segments(&path.path);
                    let leading = path.path.leading_colon.is_some();
                    match self
                        .resolver
                        .resolve(item.module, leading, &segments, Ns::Value)
                    {
                        Ok(Res::Item(index)) => match &self.resolver.items[index].kind {
                            Kind::Function(function)
                                if function.output.as_ref().is_some_and(|output| {
                                    self.answers_app(output, self.resolver.items[index].module)
                                }) =>
                            {
                                Some(index)
                            }
                            _ => None,
                        },
                        _ => None,
                    }
                }
                _ => None,
            },
            _ => None,
        };
        // An item the reader cannot see may give the name of the function
        // another meaning.
        let unseen = match argument {
            Expr::Call(call) => match &*call.func {
                Expr::Path(path) if path.qself.is_none() => {
                    self.unseen(&path.path, item.module, Ns::Value)
                }
                _ => None,
            },
            _ => None,
        };
        match (child, unseen) {
            (Some(_), Some(name)) => self.refuse_unseen(place, &name, item),
            (Some(index), None) => children.push((index, self.path_of(index))),
            (None, _) => self.refuse(
                place,
                String::from(
                    "the app that `.compose(..)` registers is no call of a function of the crate that answers an app, so the builder cannot read its roots from the source. Compose the call of such a function, as `.compose(admin())`",
                ),
            ),
        }
    }

    /// Returns the path of the item `index` from the crate root, without
    /// `crate`.
    fn path_of(&self, index: usize) -> String {
        self.resolver.path_of(index)
    }
}

/// Returns the first type argument of the last segment of `path`.
pub(crate) fn first_type_argument(path: &syn::Path) -> Option<&Type> {
    type_arguments(path).into_iter().next()
}

/// Returns the type arguments of the last segment of `path`, in order.
pub(crate) fn type_arguments(path: &syn::Path) -> Vec<&Type> {
    let Some(last) = path.segments.last() else {
        return Vec::new();
    };
    let PathArguments::AngleBracketed(arguments) = &last.arguments else {
        return Vec::new();
    };
    arguments
        .args
        .iter()
        .filter_map(|argument| match argument {
            GenericArgument::Type(ty) => Some(ty),
            _ => None,
        })
        .collect()
}

/// The methods of the builder of the facade, each of which takes the builder by
/// value and answers it.
///
/// A method of another name may answer another app, as a method of a trait of
/// the application on the builder may, and may borrow its receiver mutably
/// through autoref, as `borrow_mut` of `BorrowMut` does, so the reader
/// follows a chain through these calls alone.
const BUILDER_METHODS: [&str; 29] = [
    "query",
    "mutation",
    "subscription",
    "base_path",
    "name",
    "gate",
    "gate_async",
    "get",
    "post",
    "put",
    "delete",
    "patch",
    "options",
    "all",
    "on",
    "middleware",
    "route",
    "not_found",
    "on_error",
    "compose",
    "plugin",
    "graphiql",
    "landing_page",
    "viewer",
    "logger",
    "masked_errors",
    "graphql_endpoint",
    "health_check_endpoint",
    "multipart",
];

/// The deepest the walk of [`Reader::unseen`] follows the `use`s and the globs
/// of the modules, beyond which it takes a name as one the reader cannot see.
const UNSEEN_DEPTH: usize = 16;

/// The macros of the standard library that format, whose arguments after the
/// format string may name a value, `name = value`, a name the format string
/// alone sees.
const FORMATTING: [&str; 18] = [
    "format",
    "format_args",
    "print",
    "println",
    "eprint",
    "eprintln",
    "write",
    "writeln",
    "panic",
    "assert",
    "assert_eq",
    "assert_ne",
    "debug_assert",
    "debug_assert_eq",
    "debug_assert_ne",
    "unreachable",
    "todo",
    "unimplemented",
];

/// Returns whether a call of the method `method` registers a root or a
/// composed app.
fn registers(method: &str) -> bool {
    matches!(method, "query" | "mutation" | "subscription" | "compose")
}

/// Returns whether a call of the method `method` on a chain keeps its roots:
/// a method of the builder that registers no root.
fn keeps_roots(method: &str) -> bool {
    BUILDER_METHODS.contains(&method) && !registers(method)
}

/// Adds to `bases` the binding each arm of the branch `expr` continues, and
/// returns whether every arm either continues the chain of a binding with
/// calls that register no root or leaves the function with a `return`.
///
/// A `match`, an `if` with its `else` and a block are branches, a block whose
/// statements before its last expression register nothing on a statement.
/// The names of `shadowed` are bound again inside the branch, so an arm that
/// continues one of them continues no binding of the body. An arm that
/// registers a root or composes an app, an arm that calls a method the
/// builder does not hold, an arm that continues a name its pattern, the `let`
/// of its `if` or a `let` of its block binds again, an `if` with no `else`, a
/// labeled block that breaks to its label with a value and an arm that ends in
/// anything but a chain of a binding answer false. The `return` of an arm is
/// the body's, which refuses it unless it answers an error.
fn arms_continue<'e>(expr: &'e Expr, shadowed: &[String], bases: &mut Vec<&'e Expr>) -> bool {
    match expr {
        Expr::Paren(inner) => arms_continue(&inner.expr, shadowed, bases),
        Expr::Group(inner) => arms_continue(&inner.expr, shadowed, bases),
        Expr::Return(_) => true,
        Expr::Match(branch) => branch.arms.iter().all(|arm| {
            let mut names = shadowed.to_vec();
            pattern_names(&arm.pat, &mut names);
            arms_continue(&arm.body, &names, bases)
        }),
        Expr::If(branch) => {
            let Some((_, otherwise)) = &branch.else_branch else {
                return false;
            };
            let mut names = shadowed.to_vec();
            condition_names(&branch.cond, &mut names);
            block_continues(&branch.then_branch, &names, bases)
                && arms_continue(otherwise, shadowed, bases)
        }
        Expr::Block(inner) => {
            !inner
                .label
                .as_ref()
                .is_some_and(|label| breaks_to(&inner.block, label))
                && block_continues(&inner.block, shadowed, bases)
        }
        _ => {
            let mut chain = expr;
            loop {
                match chain {
                    Expr::MethodCall(call) if keeps_roots(&call.method.to_string()) => {
                        chain = &call.receiver;
                    }
                    Expr::Paren(inner) => chain = &inner.expr,
                    Expr::Group(inner) => chain = &inner.expr,
                    Expr::Path(path)
                        if path.qself.is_none()
                            && path.path.get_ident().is_some_and(|ident| {
                                !shadowed.iter().any(|name| ident == name)
                            }) =>
                    {
                        bases.push(chain);
                        return true;
                    }
                    _ => return false,
                }
            }
        }
    }
}

/// Returns whether the block `block` is an arm of [`arms_continue`]: its
/// statements register nothing, and its last statement continues a chain of
/// a binding none of `shadowed` and none of its own `let`s binds again, or
/// leaves with a `return`.
fn block_continues<'e>(
    block: &'e syn::Block,
    shadowed: &[String],
    bases: &mut Vec<&'e Expr>,
) -> bool {
    let Some((last, before)) = block.stmts.split_last() else {
        return false;
    };
    let mut names = shadowed.to_vec();
    for statement in before {
        match statement {
            Stmt::Local(local) => {
                // A binding of the block hides the one the chain continues,
                // so a name it binds again ends the reading.
                pattern_names(&local.pat, &mut names);
                if local
                    .init
                    .as_ref()
                    .is_some_and(|init| registers_call(&init.expr))
                {
                    return false;
                }
            }
            Stmt::Expr(expr, _) if registers_call(expr) => return false,
            _ => {}
        }
    }
    match last {
        Stmt::Expr(expr, None) => arms_continue(expr, &names, bases),
        Stmt::Expr(Expr::Return(_), Some(_)) => true,
        _ => false,
    }
}

/// Returns whether a `break` inside the block `block` leaves it through its
/// label `label` with a value, which the block then answers.
fn breaks_to(block: &syn::Block, label: &syn::Label) -> bool {
    let mut effects = Effects::default();
    effects.block(block, false);
    effects
        .broken
        .iter()
        .any(|broken| label.name.ident == broken)
}

/// Returns whether `value`, assigned to the binding `name`, continues the
/// chain of `name` with calls of the builder that register no root, so it
/// leaves the roots of the binding as they stand.
fn continues(value: &Expr, name: &str) -> bool {
    let mut bases = Vec::new();
    arms_continue(value, &[], &mut bases)
        && !bases.is_empty()
        && bases
            .iter()
            .all(|base| matches!(base, Expr::Path(path) if path.path.is_ident(name)))
}

/// Returns whether `expr` is a call of a method that registers a root or a
/// composed app.
fn registers_call(expr: &Expr) -> bool {
    matches!(expr, Expr::MethodCall(call) if registers(&call.method.to_string()))
}

/// Returns whether the two bases of a branch name the same binding.
fn same_binding(one: &Expr, other: &Expr) -> bool {
    match (one, other) {
        (Expr::Path(one), Expr::Path(other)) => one.path.get_ident() == other.path.get_ident(),
        _ => false,
    }
}

/// Returns the path of the function `expr` calls, where `expr` is one call of
/// a path with one argument, as `Err(..)`.
fn call_path(expr: &Expr) -> Option<syn::Path> {
    match expr {
        Expr::Call(call) if call.args.len() == 1 => match &*call.func {
            Expr::Path(path) if path.qself.is_none() => Some(path.path.clone()),
            _ => None,
        },
        _ => None,
    }
}

/// Returns the path a root `argument` names its struct by, with the namespace
/// its last segment is looked up in: its name, a value, the path of its
/// literal, a type, or the path of the function a call of it calls, a value.
fn root_path(argument: &Expr) -> Option<(&syn::Path, Ns)> {
    match argument {
        Expr::Paren(inner) => root_path(&inner.expr),
        Expr::Group(inner) => root_path(&inner.expr),
        Expr::Reference(inner) => root_path(&inner.expr),
        Expr::Path(path) if path.qself.is_none() => Some((&path.path, Ns::Value)),
        Expr::Struct(literal) if literal.qself.is_none() => Some((&literal.path, Ns::Type)),
        Expr::Call(call) => match &*call.func {
            Expr::Path(path) if path.qself.is_none() => Some((&path.path, Ns::Value)),
            _ => None,
        },
        _ => None,
    }
}

/// Returns the path of the function a root `argument` calls where the root is
/// a call of `new` or `default` of a type with no argument, as
/// `Query::new()`, which [`Reader::root_type`] takes as the struct it names.
fn constructor(argument: &Expr) -> Option<&syn::Path> {
    match argument {
        Expr::Paren(inner) => constructor(&inner.expr),
        Expr::Group(inner) => constructor(&inner.expr),
        Expr::Call(call) if call.args.is_empty() => match &*call.func {
            Expr::Path(path)
                if path.qself.is_none()
                    && path.path.segments.len() > 1
                    && path.path.segments.last().is_some_and(|last| {
                        matches!(last.ident.unraw().to_string().as_str(), "new" | "default")
                    }) =>
            {
                Some(&path.path)
            }
            _ => None,
        },
        _ => None,
    }
}

/// Returns whether the root `argument` is a reference, as `&Query`, also in
/// parentheses.
fn referenced(argument: &Expr) -> bool {
    match argument {
        Expr::Paren(inner) => referenced(&inner.expr),
        Expr::Group(inner) => referenced(&inner.expr),
        Expr::Reference(_) => true,
        _ => false,
    }
}

/// Returns the names `function` binds or declares beside the bindings its
/// chain runs through: its parameters and the type and the const parameters
/// of its signature, the items and the `use`s of its body at any depth, the
/// `let`s of the statements of a macro call, and every name of the tokens of a
/// macro call that hold a `let` or an item.
fn hiding(function: &Function) -> Hiding {
    let mut scan = Effects::default();
    scan.block(&function.block, false);
    let mut names = scan.declared;
    names.extend(function.generics.iter().chain(&function.consts).cloned());
    for param in &function.params {
        if param.named {
            names.push(param.name.clone());
        } else if let Ok(tokens) = param.name.parse::<TokenStream>() {
            declared_names(tokens, &mut names);
        }
    }
    Hiding {
        names,
        all: scan.globbed,
    }
}

/// Returns the name `expr` is, a path of one segment.
fn plain_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Path(path) if path.qself.is_none() => path.path.get_ident().map(ToString::to_string),
        Expr::Paren(inner) => plain_name(&inner.expr),
        Expr::Group(inner) => plain_name(&inner.expr),
        _ => None,
    }
}

/// Returns the name whose value the place `expr` is or holds: a name, a field
/// or an element of it, or what it points to.
fn place_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Field(inner) => place_name(&inner.base),
        Expr::Index(inner) => place_name(&inner.expr),
        Expr::Unary(inner) if matches!(inner.op, UnOp::Deref(_)) => place_name(&inner.expr),
        Expr::Paren(inner) => place_name(&inner.expr),
        Expr::Group(inner) => place_name(&inner.expr),
        _ => plain_name(expr),
    }
}

/// Calls `found` with every binding of the pattern `pat`.
fn each_binding<F: FnMut(&PatIdent)>(pat: &Pat, found: &mut F) {
    match pat {
        Pat::Ident(ident) => {
            found(ident);
            if let Some((_, inner)) = &ident.subpat {
                each_binding(inner, found);
            }
        }
        Pat::Or(or) => or.cases.iter().for_each(|case| each_binding(case, found)),
        Pat::Paren(inner) => each_binding(&inner.pat, found),
        Pat::Reference(inner) => each_binding(&inner.pat, found),
        Pat::Slice(slice) => slice
            .elems
            .iter()
            .for_each(|elem| each_binding(elem, found)),
        Pat::Struct(fields) => fields
            .fields
            .iter()
            .for_each(|field| each_binding(&field.pat, found)),
        Pat::Tuple(tuple) => tuple
            .elems
            .iter()
            .for_each(|elem| each_binding(elem, found)),
        Pat::TupleStruct(tuple) => tuple
            .elems
            .iter()
            .for_each(|elem| each_binding(elem, found)),
        Pat::Type(typed) => each_binding(&typed.pat, found),
        _ => {}
    }
}

/// Adds every name the pattern `pat` binds to `names`.
fn pattern_names(pat: &Pat, names: &mut Vec<String>) {
    each_binding(pat, &mut |ident| names.push(ident.ident.to_string()));
}

/// Adds the span of every binding of the pattern `pat` that borrows its
/// place mutably, `ref mut`, to `spans`.
fn ref_muts(pat: &Pat, spans: &mut Vec<Span>) {
    each_binding(pat, &mut |ident| {
        if ident.by_ref.is_some() && ident.mutability.is_some() {
            spans.push(ident.span());
        }
    });
}

/// Adds every name the `let`s of the condition `cond` of an `if` bind to
/// `names`, the names its first block sees.
fn condition_names(cond: &Expr, names: &mut Vec<String>) {
    match cond {
        Expr::Let(binding) => pattern_names(&binding.pat, names),
        Expr::Binary(both) => {
            condition_names(&both.left, names);
            condition_names(&both.right, names);
        }
        Expr::Paren(inner) => condition_names(&inner.expr, names),
        Expr::Group(inner) => condition_names(&inner.expr, names),
        _ => {}
    }
}

/// Adds every name the assignee `expr` of a destructuring assignment or of an
/// assignment of a place names to `names`.
fn assignee_names(expr: &Expr, names: &mut Vec<String>) {
    match expr {
        Expr::Array(array) => array
            .elems
            .iter()
            .for_each(|elem| assignee_names(elem, names)),
        Expr::Tuple(tuple) => tuple
            .elems
            .iter()
            .for_each(|elem| assignee_names(elem, names)),
        Expr::Call(call) => call.args.iter().for_each(|arg| assignee_names(arg, names)),
        Expr::Struct(literal) => literal
            .fields
            .iter()
            .for_each(|field| assignee_names(&field.expr, names)),
        _ => names.extend(place_name(expr)),
    }
}

/// A `return` that leaves the function.
struct Exit {
    /// The span of the `return`, or of the macro call whose tokens hold it.
    span: Span,
    /// The path of the function its value calls, where the value is one call
    /// of a path, as `Err` of `Err(..)`, which the reader resolves.
    call: Option<syn::Path>,
}

/// What a part of a body does beside the value it answers: the `return`s that
/// leave the function, the names it gives another value, the labels it breaks
/// out of with a value and the names it declares.
#[derive(Default)]
struct Effects {
    /// Every `return` outside a closure and an async block, whose `return`
    /// leaves the closure or the block.
    exits: Vec<Exit>,
    /// Every name an assignment or a mutable borrow may give another app, at
    /// the span of the change, an assignment of the name's own chain with
    /// calls of the builder that register no root left out. A closure and an
    /// async block count, since either may change a binding it captures.
    changes: Vec<(String, Span)>,
    /// Every label a `break` with a value leaves through, outside a closure
    /// and an async block, which no `break` leaves.
    broken: Vec<String>,
    /// Every name an item or a `use` declares, a `let` of the statements of a
    /// macro call binds, or the tokens of a macro call that hold a `let` or an
    /// item hold, at any depth.
    declared: Vec<String>,
    /// Whether a glob `use` may import any name.
    globbed: bool,
}

impl Effects {
    /// Returns the changes that may give a name another app, at their places
    /// in `file`, each place of a name once, since an assignment of a call of
    /// a method the builder does not hold changes its name twice on one line.
    fn changed<'b>(&self, file: &Path) -> Vec<Bound<'b>> {
        let mut changed = Vec::new();
        for (name, span) in &self.changes {
            let place = at(file, *span);
            if !changed.iter().any(
                |bound| matches!(bound, Bound::Changed(seen, at) if seen == name && *at == place),
            ) {
                changed.push(Bound::Changed(name.clone(), place));
            }
        }
        changed
    }

    /// Walks the statement `statement`, inside a closure or an async block
    /// when `nested` holds.
    fn statement(&mut self, statement: &Stmt, nested: bool) {
        match statement {
            Stmt::Local(local) => {
                if let Some(init) = &local.init {
                    self.borrowed(&local.pat, &init.expr);
                    self.expr(&init.expr, nested);
                    if let Some((_, diverge)) = &init.diverge {
                        self.expr(diverge, nested);
                    }
                }
            }
            Stmt::Expr(expr, _) => self.expr(expr, nested),
            Stmt::Macro(call) => self.mac(&call.mac, nested),
            Stmt::Item(syn::Item::Macro(call)) if call.ident.is_none() => {
                self.mac(&call.mac, nested);
            }
            Stmt::Item(item) => self.declare(item),
        }
    }

    /// Records the name the item `item` declares, every name a `use` binds
    /// and the glob of a `use`.
    fn declare(&mut self, item: &syn::Item) {
        let name = match item {
            syn::Item::Use(declaration) => {
                self.uses(&declaration.tree, None);
                return;
            }
            syn::Item::ForeignMod(block) => {
                for foreign in &block.items {
                    match foreign {
                        syn::ForeignItem::Fn(item) => self.declared.push(unraw(&item.sig.ident)),
                        syn::ForeignItem::Static(item) => self.declared.push(unraw(&item.ident)),
                        syn::ForeignItem::Type(item) => self.declared.push(unraw(&item.ident)),
                        _ => {}
                    }
                }
                return;
            }
            syn::Item::Verbatim(tokens) => {
                declared_names(tokens.clone(), &mut self.declared);
                return;
            }
            syn::Item::Const(item) => &item.ident,
            syn::Item::Enum(item) => &item.ident,
            syn::Item::ExternCrate(item) => item
                .rename
                .as_ref()
                .map_or(&item.ident, |(_, rename)| rename),
            syn::Item::Fn(item) => &item.sig.ident,
            syn::Item::Macro(item) => match &item.ident {
                Some(ident) => ident,
                None => return,
            },
            syn::Item::Mod(item) => &item.ident,
            syn::Item::Static(item) => &item.ident,
            syn::Item::Struct(item) => &item.ident,
            syn::Item::Trait(item) => &item.ident,
            syn::Item::TraitAlias(item) => &item.ident,
            syn::Item::Type(item) => &item.ident,
            syn::Item::Union(item) => &item.ident,
            _ => return,
        };
        self.declared.push(unraw(name));
    }

    /// Records every name the tree `tree` of a `use` binds, `parent` being
    /// the segment before it, which `self` binds, and its glob.
    fn uses(&mut self, tree: &UseTree, parent: Option<&proc_macro2::Ident>) {
        match tree {
            UseTree::Path(path) => self.uses(&path.tree, Some(&path.ident)),
            UseTree::Name(name) if name.ident == "self" => {
                self.declared.extend(parent.map(unraw));
            }
            UseTree::Name(name) => self.declared.push(unraw(&name.ident)),
            UseTree::Rename(rename) => self.declared.push(unraw(&rename.rename)),
            UseTree::Glob(_) => self.globbed = true,
            UseTree::Group(group) => {
                for tree in &group.items {
                    self.uses(tree, parent);
                }
            }
        }
    }

    /// Walks the statements of the block `block`.
    fn block(&mut self, block: &syn::Block, nested: bool) {
        for statement in &block.stmts {
            self.statement(statement, nested);
        }
    }

    /// Walks `expr` when there is one.
    fn maybe(&mut self, expr: Option<&Expr>, nested: bool) {
        if let Some(expr) = expr {
            self.expr(expr, nested);
        }
    }

    /// Walks each of `exprs`.
    fn each<'x>(&mut self, exprs: impl IntoIterator<Item = &'x Expr>, nested: bool) {
        for expr in exprs {
            self.expr(expr, nested);
        }
    }

    /// Records a change of the name the place `place` holds at each binding
    /// of the pattern `pat` that borrows it mutably, `ref mut`.
    fn borrowed(&mut self, pat: &Pat, place: &Expr) {
        if let Some(name) = place_name(place) {
            let mut spans = Vec::new();
            ref_muts(pat, &mut spans);
            self.changes
                .extend(spans.into_iter().map(|span| (name.clone(), span)));
        }
    }

    /// Records what `expr` itself does: a `return`, a `break` with a value,
    /// an assignment, a compound assignment or a mutable borrow, by `&mut`,
    /// by a call of a method the builder does not hold or by a `ref mut`
    /// binding of a `match` or a `let` condition.
    fn record(&mut self, expr: &Expr, nested: bool) {
        match expr {
            Expr::Return(answer) if !nested => self.exits.push(Exit {
                span: expr.span(),
                call: answer.expr.as_deref().and_then(call_path),
            }),
            Expr::Break(jump) if !nested && jump.expr.is_some() => {
                self.broken
                    .extend(jump.label.as_ref().map(|label| label.ident.to_string()));
            }
            Expr::Assign(assign) => {
                // An assignment of a name alone keeps its roots where its
                // value continues the chain of the name.
                let keeps =
                    plain_name(&assign.left).is_some_and(|name| continues(&assign.right, &name));
                if !keeps {
                    let mut names = Vec::new();
                    assignee_names(&assign.left, &mut names);
                    self.changes
                        .extend(names.into_iter().map(|name| (name, expr.span())));
                }
            }
            // The operator of a compound assignment borrows its left side
            // mutably.
            Expr::Binary(binary) if compound(&binary.op) => {
                self.changes
                    .extend(place_name(&binary.left).map(|name| (name, expr.span())));
            }
            Expr::Reference(reference) if reference.mutability.is_some() => {
                self.changes
                    .extend(place_name(&reference.expr).map(|name| (name, expr.span())));
            }
            Expr::RawAddr(raw) if matches!(raw.mutability, PointerMutability::Mut(_)) => {
                self.changes
                    .extend(place_name(&raw.expr).map(|name| (name, expr.span())));
            }
            // Autoref may borrow the receiver of any method the builder does
            // not hold mutably.
            Expr::MethodCall(call)
                if !BUILDER_METHODS.contains(&call.method.to_string().as_str()) =>
            {
                self.changes
                    .extend(place_name(&call.receiver).map(|name| (name, call.method.span())));
            }
            Expr::Match(branch) => {
                for arm in &branch.arms {
                    self.borrowed(&arm.pat, &branch.expr);
                }
            }
            Expr::Let(binding) => self.borrowed(&binding.pat, &binding.expr),
            _ => {}
        }
    }

    /// Walks the arguments of the macro call `mac`, which the reader does not
    /// expand, as expressions separated by commas, else as statements, else
    /// as tokens.
    ///
    /// A named argument after the format string of a macro that formats binds
    /// no name, and the first argument of `addr_of_mut!`, `write!` and
    /// `writeln!` is borrowed mutably, by a pointer or by the call of
    /// `write_fmt` on it.
    fn mac(&mut self, mac: &Macro, nested: bool) {
        let name = mac
            .path
            .segments
            .last()
            .map(|last| last.ident.to_string())
            .unwrap_or_default();
        let formats = FORMATTING.contains(&name.as_str());
        if let Ok(arguments) =
            mac.parse_body_with(Punctuated::<Expr, syn::Token![,]>::parse_terminated)
        {
            let mut formatted = false;
            for argument in &arguments {
                match argument {
                    Expr::Assign(named) if formatted && plain_name(&named.left).is_some() => {
                        self.expr(&named.right, nested);
                    }
                    _ => self.expr(argument, nested),
                }
                formatted |= formats
                    && matches!(argument, Expr::Lit(literal) if matches!(literal.lit, Lit::Str(_)));
            }
            if matches!(name.as_str(), "addr_of_mut" | "write" | "writeln") {
                self.changes.extend(
                    arguments
                        .first()
                        .and_then(place_name)
                        .map(|first| (first, mac.path.span())),
                );
            }
        } else if let Ok(statements) = mac.parse_body_with(syn::Block::parse_within) {
            for statement in &statements {
                // A `let` of the statements binds its names where the call
                // stands, beside the bindings the reading follows.
                if let Stmt::Local(local) = statement {
                    each_binding(&local.pat, &mut |ident| {
                        self.changes.push((ident.ident.to_string(), ident.span()));
                        self.declared.push(unraw(&ident.ident));
                    });
                }
                self.statement(statement, nested);
            }
        } else {
            self.tokens(mac.tokens.clone(), mac.path.span(), nested);
        }
    }

    /// Records what the tokens `tokens` of a macro call at `call` state, where
    /// they parse as no expression and no statement, at the line of the call.
    ///
    /// A `return` leaves the function with the value of the tokens after it up
    /// to a `;` or a `,`, and a `break` to a label with a value breaks out of
    /// it. Tokens that assign, by `=` alone or after the operator of a
    /// compound assignment, that hold `mut`, as `&mut`, `&raw mut` and
    /// `ref mut` do, or a `let`, that call a method the builder does not
    /// hold, or that call `write!`, `writeln!` or `addr_of_mut!`, may change
    /// every name they hold, and tokens that hold a `let` or the keyword of an
    /// item may bind or declare every name they hold.
    fn tokens(&mut self, tokens: TokenStream, call: Span, nested: bool) {
        self.jumps(tokens.clone(), call, nested);
        let mut found = Marks::default();
        marks(tokens.clone(), &mut found);
        if found.change {
            let mut names = Vec::new();
            token_names(tokens.clone(), &mut names);
            self.changes
                .extend(names.into_iter().map(|name| (name, call)));
        }
        if found.declaration {
            declared_names(tokens, &mut self.declared);
        }
    }

    /// Records every `return` and every `break` to a label with a value among
    /// the tokens `tokens` of a macro call at `call`.
    fn jumps(&mut self, tokens: TokenStream, call: Span, nested: bool) {
        let trees: Vec<TokenTree> = tokens.into_iter().collect();
        for (index, tree) in trees.iter().enumerate() {
            let after = |step: usize| trees.get(index + step);
            match tree {
                TokenTree::Group(group) => self.jumps(group.stream(), call, nested),
                TokenTree::Ident(word) if word == "return" && !nested => {
                    let value: TokenStream = trees[index + 1..]
                        .iter()
                        .take_while(|tree| !is_punct(tree, &[';', ',']))
                        .cloned()
                        .collect();
                    let answer = syn::parse2::<Expr>(value).ok();
                    self.exits.push(Exit {
                        span: call,
                        call: answer.as_ref().and_then(call_path),
                    });
                }
                TokenTree::Ident(word) if word == "break" => {
                    if let (Some(quote), Some(TokenTree::Ident(label)), Some(value)) =
                        (after(1), after(2), after(3))
                        && !nested
                        && is_punct(quote, &['\''])
                        && !is_punct(value, &[';', ','])
                    {
                        self.broken.push(label.to_string());
                    }
                }
                _ => {}
            }
        }
    }

    /// Walks `expr` and every expression inside it, the arguments of a macro
    /// among them, but no item.
    fn expr(&mut self, expr: &Expr, nested: bool) {
        self.record(expr, nested);
        match expr {
            Expr::Return(answer) => self.maybe(answer.expr.as_deref(), nested),
            Expr::Break(jump) => self.maybe(jump.expr.as_deref(), nested),
            Expr::Yield(inner) => self.maybe(inner.expr.as_deref(), nested),
            Expr::Closure(closure) => self.expr(&closure.body, true),
            Expr::Async(inner) => self.block(&inner.block, true),
            Expr::Block(inner) => self.block(&inner.block, nested),
            Expr::Unsafe(inner) => self.block(&inner.block, nested),
            Expr::TryBlock(inner) => self.block(&inner.block, nested),
            Expr::Loop(inner) => self.block(&inner.body, nested),
            Expr::While(inner) => {
                self.expr(&inner.cond, nested);
                self.block(&inner.body, nested);
            }
            Expr::ForLoop(inner) => {
                self.expr(&inner.expr, nested);
                self.block(&inner.body, nested);
            }
            Expr::If(branch) => {
                self.expr(&branch.cond, nested);
                self.block(&branch.then_branch, nested);
                self.maybe(
                    branch
                        .else_branch
                        .as_ref()
                        .map(|(_, otherwise)| &**otherwise),
                    nested,
                );
            }
            Expr::Match(branch) => {
                self.expr(&branch.expr, nested);
                for arm in &branch.arms {
                    self.maybe(arm.guard.as_ref().map(|(_, guard)| &**guard), nested);
                    self.expr(&arm.body, nested);
                }
            }
            Expr::Call(call) => {
                self.expr(&call.func, nested);
                self.each(&call.args, nested);
            }
            Expr::MethodCall(call) => {
                self.expr(&call.receiver, nested);
                self.each(&call.args, nested);
            }
            Expr::Macro(inner) => self.mac(&inner.mac, nested),
            Expr::Array(array) => self.each(&array.elems, nested),
            Expr::Tuple(tuple) => self.each(&tuple.elems, nested),
            Expr::Struct(literal) => {
                self.each(literal.fields.iter().map(|field| &field.expr), nested);
                self.maybe(literal.rest.as_deref(), nested);
            }
            Expr::Assign(assign) => self.each([&*assign.left, &*assign.right], nested),
            Expr::Binary(binary) => self.each([&*binary.left, &*binary.right], nested),
            Expr::Index(index) => self.each([&*index.expr, &*index.index], nested),
            Expr::Repeat(repeat) => self.each([&*repeat.expr, &*repeat.len], nested),
            Expr::Range(range) => {
                self.maybe(range.start.as_deref(), nested);
                self.maybe(range.end.as_deref(), nested);
            }
            Expr::Await(inner) => self.expr(&inner.base, nested),
            Expr::Field(inner) => self.expr(&inner.base, nested),
            Expr::Cast(inner) => self.expr(&inner.expr, nested),
            Expr::Group(inner) => self.expr(&inner.expr, nested),
            Expr::Let(inner) => self.expr(&inner.expr, nested),
            Expr::Paren(inner) => self.expr(&inner.expr, nested),
            Expr::RawAddr(inner) => self.expr(&inner.expr, nested),
            Expr::Reference(inner) => self.expr(&inner.expr, nested),
            Expr::Try(inner) => self.expr(&inner.expr, nested),
            Expr::Unary(inner) => self.expr(&inner.expr, nested),
            _ => {}
        }
    }
}

/// Returns whether the operator `op` is a compound assignment, `+=` and its
/// kin.
fn compound(op: &BinOp) -> bool {
    matches!(
        op,
        BinOp::AddAssign(_)
            | BinOp::SubAssign(_)
            | BinOp::MulAssign(_)
            | BinOp::DivAssign(_)
            | BinOp::RemAssign(_)
            | BinOp::BitXorAssign(_)
            | BinOp::BitAndAssign(_)
            | BinOp::BitOrAssign(_)
            | BinOp::ShlAssign(_)
            | BinOp::ShrAssign(_)
    )
}

/// Returns whether `tree` is a punctuation mark of `marks`.
fn is_punct(tree: &TokenTree, marks: &[char]) -> bool {
    matches!(tree, TokenTree::Punct(punct) if marks.contains(&punct.as_char()))
}

/// The keywords that open a `let` or an item, which bind or declare a name.
const DECLARING: [&str; 13] = [
    "let",
    "fn",
    "struct",
    "enum",
    "union",
    "trait",
    "type",
    "mod",
    "static",
    "const",
    "use",
    "extern",
    "macro_rules",
];

/// What the tokens of a macro call hold that the reader takes as a mark of
/// the whole call.
#[derive(Default)]
struct Marks {
    /// A mark of a change: an assignment, a compound assignment, `mut`, a
    /// `let`, which may bind a name again, a call of a method the builder
    /// does not hold, or a call of `write!`, `writeln!` or `addr_of_mut!`.
    change: bool,
    /// A `let` or the keyword of an item.
    declaration: bool,
}

/// Adds to `found` the marks of the tokens `tokens`, in every group.
///
/// A word after `.` names a field or a method and a word after `'` a label
/// or a lifetime, as `'static`, so neither is a keyword.
fn marks(tokens: TokenStream, found: &mut Marks) {
    let trees: Vec<TokenTree> = tokens.into_iter().collect();
    for (index, tree) in trees.iter().enumerate() {
        let after = |step: usize| trees.get(index + step);
        let behind = index
            .checked_sub(1)
            .is_some_and(|before| is_punct(&trees[before], &['.', '\'']));
        match tree {
            TokenTree::Group(group) => marks(group.stream(), found),
            TokenTree::Punct(mark) if mark.as_char() == '=' => {
                found.change |= assigns(&trees, index);
            }
            TokenTree::Punct(dot) if dot.as_char() == '.' => {
                if let (Some(TokenTree::Ident(method)), Some(next)) = (after(1), after(2)) {
                    found.change |= !BUILDER_METHODS.contains(&method.to_string().as_str())
                        && (matches!(next, TokenTree::Group(group) if group.delimiter() == Delimiter::Parenthesis)
                            || is_punct(next, &[':']));
                }
            }
            TokenTree::Ident(word) if !behind => {
                let word = word.to_string();
                let borrows = matches!(word.as_str(), "write" | "writeln" | "addr_of_mut")
                    && after(1).is_some_and(|next| is_punct(next, &['!']));
                found.change |= borrows || word == "mut" || word == "let";
                found.declaration |= DECLARING.contains(&word.as_str());
            }
            _ => {}
        }
    }
}

/// Returns whether the mark `=` at `index` of `trees` assigns, alone or after
/// the operator of a compound assignment, rather than standing in `==`, `!=`,
/// `<=`, `>=`, `=>` or `..=`.
fn assigns(trees: &[TokenTree], index: usize) -> bool {
    let joint = |at: usize| match trees.get(at) {
        Some(TokenTree::Punct(punct)) if punct.spacing() == Spacing::Joint => Some(punct.as_char()),
        _ => None,
    };
    // `==` and `=>` join this mark to the next.
    if joint(index).is_some()
        && trees
            .get(index + 1)
            .is_some_and(|next| is_punct(next, &['=', '>']))
    {
        return false;
    }
    let Some(before) = index.checked_sub(1) else {
        return true;
    };
    match joint(before) {
        Some('=' | '!' | '.') => false,
        // `<=` and `>=` compare, while `<<=` and `>>=` assign.
        Some(op @ ('<' | '>')) => before
            .checked_sub(1)
            .is_some_and(|first| joint(first) == Some(op)),
        _ => true,
    }
}

/// Adds every name the tokens `tokens` hold to `names`.
fn token_names(tokens: TokenStream, names: &mut Vec<String>) {
    for tree in tokens {
        match tree {
            TokenTree::Group(group) => token_names(group.stream(), names),
            TokenTree::Ident(name) => names.push(name.to_string()),
            TokenTree::Punct(_) | TokenTree::Literal(_) => {}
        }
    }
}

/// Adds every name the tokens `tokens` hold to `names` as a path names it,
/// its raw prefix taken off.
fn declared_names(tokens: TokenStream, names: &mut Vec<String>) {
    for tree in tokens {
        match tree {
            TokenTree::Group(group) => declared_names(group.stream(), names),
            TokenTree::Ident(name) => names.push(unraw(&name)),
            TokenTree::Punct(_) | TokenTree::Literal(_) => {}
        }
    }
}

/// Returns the name `ident`, its raw prefix taken off, as the resolver reads
/// the segments of a path.
fn unraw(ident: &proc_macro2::Ident) -> String {
    ident.unraw().to_string()
}

/// Returns the place of `span` in `file`.
fn at(file: &Path, span: proc_macro2::Span) -> Place {
    Place {
        file: file.to_path_buf(),
        line: span.start().line,
    }
}
