// SPDX-License-Identifier: MIT
/*
 * aosabi-macros: the scan of the leading guard calls of a body.
 *
 * No analogue on the reference side. The sidecar operation authoring_map
 * mirrors the scan on the TypeScript side, and the authoring check C3 compares
 * the two.
 */

//! The guard facts of the calls to `require_role` and `authorize` that open a
//! body.
//!
//! A TypeScript resolver states its permission in its first statements,
//! `requireRole('editor', 'admin')` or `authorize(p => ...)`, and check C3 of
//! `tests/notebooks/12_authoring_parity.ipynb` compares those calls of a pair
//! per path of the intermediate representation.
//!
//! 1. The leading run is the statements from the start of the body while each
//!    is an expression statement whose expression is a call of a path whose
//!    last segment is `require_role` or `authorize`, with any prefix before
//!    the name. A statement that ends in a semicolon is a guard through `?`
//!    alone, because without it the statement drops the result of the guard
//!    and guards nothing. The tail of the body, which has no semicolon, is a
//!    guard with or without `?`, because the body answers the result of the
//!    guard. The first other statement ends the run, so a `let` before a guard
//!    hides the guard on both sides alike, and check C3 reports a dropped
//!    guard against the call of the reference.
//! 2. `require_role(&[...])` records the string literals of the slice in
//!    order. Any other argument records `args` as `None`, because the
//!    reference side cannot see through a variable either.
//! 3. `authorize(|p| ...)` records, in source order, every call inside the
//!    closure whose path ends in `has_role` or `has_permission`, with the
//!    string literals among its arguments, the elements of a literal slice
//!    among them, and counts every other call and every comparison as
//!    `others`. A method call on the parameter of the closure counts as no
//!    call, because it is the Rust form of the optional chain `p?.` of the
//!    reference, for which Rust has no operator. A macro call is opaque to the
//!    scan and counts as nothing, as a template literal counts as nothing on
//!    the reference side.
//! 4. The line of a guard is the line `proc_macro::Span::line` gives the call.
//! 5. A guard inside a decorator is a decorator fact and never a guard fact,
//!    so the scan reads the body alone.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    BinOp,
    Block,
    Expr,
    ExprCall,
    Lit,
    LitStr,
    Pat,
    Stmt,
    spanned::Spanned, //
};

use crate::graphql::line_of;

/// The last path segment of the guard that requires a role.
const REQUIRE_ROLE: &str = "require_role";

/// The last path segment of the guard that runs a check.
const AUTHORIZE: &str = "authorize";

/// The last path segments of the checks a guard fact names inside an
/// `authorize`.
const CHECKS: [&str; 2] = ["has_role", "has_permission"];

/// Which call a guard is.
#[derive(Clone, Copy)]
enum Kind {
    /// `require_role`.
    RequireRole,
    /// `authorize`.
    Authorize,
}

/// One guard of the leading run.
struct Guard {
    /// Which call the guard is.
    kind: Kind,
    /// The string literals of a literal slice of `require_role`.
    args: Option<Vec<String>>,
    /// The checks of the closure of an `authorize` with their string
    /// literals.
    calls: Vec<(String, Vec<String>)>,
    /// The other calls and the comparisons of the closure.
    others: u32,
    /// The source line of the call.
    line: u32,
}

impl Guard {
    /// Returns the `GuardFact` literal of the guard.
    fn to_tokens(&self) -> TokenStream {
        let kind = match self.kind {
            Kind::RequireRole => quote!(::aosabi::descriptor::GuardKind::RequireRole),
            Kind::Authorize => quote!(::aosabi::descriptor::GuardKind::Authorize),
        };
        let args = self.args.as_ref().map_or_else(
            || quote!(::core::option::Option::None),
            |args| quote!(::core::option::Option::Some(&[#(#args),*])),
        );
        let calls = self.calls.iter().map(|(name, args)| {
            quote!(::aosabi::descriptor::GuardCall { name: #name, args: &[#(#args),*] })
        });
        let (others, line) = (self.others, self.line);
        quote! {
            ::aosabi::descriptor::GuardFact {
                kind: #kind,
                args: #args,
                calls: &[#(#calls),*],
                others: #others,
                line: #line,
            }
        }
    }
}

/// Returns the `&[GuardFact]` expression of the leading run of `body`.
pub(crate) fn facts(body: &Block) -> TokenStream {
    let facts = scan(body).iter().map(Guard::to_tokens).collect::<Vec<_>>();
    quote!(&[#(#facts),*])
}

/// Returns the guards of the leading run of `body`, in source order.
fn scan(body: &Block) -> Vec<Guard> {
    body.stmts
        .iter()
        .map_while(|stmt| match stmt {
            Stmt::Expr(expr, semi) => guard(expr, semi.is_none()),
            Stmt::Local(_) | Stmt::Item(_) | Stmt::Macro(_) => None,
        })
        .collect()
}

/// Returns the guard of the expression of one statement, or `None` when it
/// is no guard call.
///
/// `tail` is whether the statement is the tail of the body, which is a guard
/// without `?` as well, while a statement that ends in a semicolon is a guard
/// only through `?`.
fn guard(expr: &Expr, tail: bool) -> Option<Guard> {
    let mut current = expr;
    let mut tried = false;
    loop {
        current = match current {
            Expr::Try(inner) => {
                tried = true;
                &inner.expr
            }
            Expr::Paren(inner) => &inner.expr,
            Expr::Group(inner) => &inner.expr,
            _ => break,
        };
    }
    if !tried && !tail {
        return None;
    }
    let Expr::Call(call) = current else {
        return None;
    };
    let line = line_of(call.span());
    match callee(call)?.as_str() {
        REQUIRE_ROLE => Some(Guard {
            kind: Kind::RequireRole,
            args: roles(call),
            calls: Vec::new(),
            others: 0,
            line,
        }),
        AUTHORIZE => {
            let mut check = Check::default();
            let arguments = call.args.iter().collect::<Vec<_>>();
            if let [argument] = arguments.as_slice()
                && let Expr::Closure(closure) = unwrap(argument)
            {
                check.params = closure.inputs.iter().filter_map(param_name).collect();
                check.expr(&closure.body);
            }
            Some(Guard {
                kind: Kind::Authorize,
                args: None,
                calls: check.calls,
                others: check.others,
                line,
            })
        }
        _ => None,
    }
}

/// Returns `expr` without the parentheses and the invisible groups around it.
pub(crate) fn unwrap(expr: &Expr) -> &Expr {
    match expr {
        Expr::Paren(inner) => unwrap(&inner.expr),
        Expr::Group(inner) => unwrap(&inner.expr),
        _ => expr,
    }
}

/// Returns the last path segment of the function `call` calls, or `None`
/// when the callee is no path.
fn callee(call: &ExprCall) -> Option<String> {
    let Expr::Path(path) = unwrap(&call.func) else {
        return None;
    };
    path.path
        .segments
        .last()
        .map(|segment| segment.ident.to_string())
}

/// Returns the string literal `expr` is, or `None` for any other expression.
pub(crate) fn string(expr: &Expr) -> Option<&LitStr> {
    match unwrap(expr) {
        Expr::Lit(literal) => match &literal.lit {
            Lit::Str(text) => Some(text),
            _ => None,
        },
        _ => None,
    }
}

/// Returns the elements of the literal slice `&[...]`, or `None` for any
/// other expression.
fn slice(expr: &Expr) -> Option<Vec<&Expr>> {
    let Expr::Reference(reference) = unwrap(expr) else {
        return None;
    };
    if reference.mutability.is_some() {
        return None;
    }
    match unwrap(&reference.expr) {
        Expr::Array(array) => Some(array.elems.iter().collect()),
        _ => None,
    }
}

/// Returns the roles of a `require_role` whose one argument is a literal
/// slice of string literals.
fn roles(call: &ExprCall) -> Option<Vec<String>> {
    let arguments = call.args.iter().collect::<Vec<_>>();
    let [argument] = arguments.as_slice() else {
        return None;
    };
    slice(argument)?
        .into_iter()
        .map(|element| string(element).map(LitStr::value))
        .collect()
}

/// Returns the string literals among the arguments of a check, each string
/// literal argument and each string literal element of a literal slice, in
/// source order.
fn literals(call: &ExprCall) -> Vec<String> {
    let mut found = Vec::new();
    for argument in &call.args {
        if let Some(text) = string(argument) {
            found.push(text.value());
        } else if let Some(elements) = slice(argument) {
            found.extend(
                elements
                    .into_iter()
                    .filter_map(|element| string(element).map(LitStr::value)),
            );
        }
    }
    found
}

/// Returns the name a closure parameter binds, `None` for a pattern that
/// binds no one name.
fn param_name(pat: &Pat) -> Option<String> {
    match pat {
        Pat::Ident(ident) => Some(ident.ident.to_string()),
        Pat::Type(typed) => param_name(&typed.pat),
        _ => None,
    }
}

/// The walk of the closure of an `authorize`.
#[derive(Default)]
struct Check {
    /// The names the parameters of the closure bind.
    params: Vec<String>,
    /// The checks found so far, in source order.
    calls: Vec<(String, Vec<String>)>,
    /// The other calls and the comparisons found so far.
    others: u32,
}

impl Check {
    /// Returns whether `expr` is a parameter of the closure by its bare name.
    fn is_param(&self, expr: &Expr) -> bool {
        let Expr::Path(path) = unwrap(expr) else {
            return false;
        };
        path.qself.is_none()
            && path
                .path
                .get_ident()
                .is_some_and(|ident| self.params.iter().any(|param| ident == param))
    }

    /// Walks `expr`, a node before its children and the children in source
    /// order, as the sidecar walks the check of the reference.
    fn expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Call(call) => {
                match callee(call) {
                    Some(name) if CHECKS.contains(&name.as_str()) => {
                        self.calls.push((name, literals(call)));
                    }
                    _ => self.others += 1,
                }
                self.expr(&call.func);
                self.exprs(call.args.iter());
            }
            Expr::MethodCall(call) => {
                if !self.is_param(&call.receiver) {
                    self.others += 1;
                }
                self.expr(&call.receiver);
                self.exprs(call.args.iter());
            }
            Expr::Binary(binary) => {
                if matches!(
                    binary.op,
                    BinOp::Eq(_)
                        | BinOp::Ne(_)
                        | BinOp::Lt(_)
                        | BinOp::Le(_)
                        | BinOp::Gt(_)
                        | BinOp::Ge(_)
                ) {
                    self.others += 1;
                }
                self.expr(&binary.left);
                self.expr(&binary.right);
            }
            Expr::Array(array) => self.exprs(array.elems.iter()),
            Expr::Tuple(tuple) => self.exprs(tuple.elems.iter()),
            Expr::Assign(assign) => {
                self.expr(&assign.left);
                self.expr(&assign.right);
            }
            Expr::Index(index) => {
                self.expr(&index.expr);
                self.expr(&index.index);
            }
            Expr::Repeat(repeat) => {
                self.expr(&repeat.expr);
                self.expr(&repeat.len);
            }
            Expr::Range(range) => {
                self.exprs(range.start.iter().map(AsRef::as_ref));
                self.exprs(range.end.iter().map(AsRef::as_ref));
            }
            Expr::Await(inner) => self.expr(&inner.base),
            Expr::Cast(inner) => self.expr(&inner.expr),
            Expr::Field(inner) => self.expr(&inner.base),
            Expr::Group(inner) => self.expr(&inner.expr),
            Expr::Let(inner) => self.expr(&inner.expr),
            Expr::Paren(inner) => self.expr(&inner.expr),
            Expr::RawAddr(inner) => self.expr(&inner.expr),
            Expr::Reference(inner) => self.expr(&inner.expr),
            Expr::Try(inner) => self.expr(&inner.expr),
            Expr::Unary(inner) => self.expr(&inner.expr),
            Expr::Closure(inner) => self.expr(&inner.body),
            Expr::Break(inner) => self.exprs(inner.expr.iter().map(AsRef::as_ref)),
            Expr::Return(inner) => self.exprs(inner.expr.iter().map(AsRef::as_ref)),
            Expr::Yield(inner) => self.exprs(inner.expr.iter().map(AsRef::as_ref)),
            Expr::Struct(inner) => {
                self.exprs(inner.fields.iter().map(|field| &field.expr));
                self.exprs(inner.rest.iter().map(AsRef::as_ref));
            }
            Expr::Block(inner) => self.block(&inner.block),
            Expr::Async(inner) => self.block(&inner.block),
            Expr::Const(inner) => self.block(&inner.block),
            Expr::Loop(inner) => self.block(&inner.body),
            Expr::TryBlock(inner) => self.block(&inner.block),
            Expr::Unsafe(inner) => self.block(&inner.block),
            Expr::ForLoop(inner) => {
                self.expr(&inner.expr);
                self.block(&inner.body);
            }
            Expr::While(inner) => {
                self.expr(&inner.cond);
                self.block(&inner.body);
            }
            Expr::If(inner) => {
                self.expr(&inner.cond);
                self.block(&inner.then_branch);
                self.exprs(inner.else_branch.iter().map(|(_, other)| other.as_ref()));
            }
            Expr::Match(inner) => {
                self.expr(&inner.expr);
                for arm in &inner.arms {
                    self.exprs(arm.guard.iter().map(|(_, guard)| guard.as_ref()));
                    self.expr(&arm.body);
                }
            }
            _ => {}
        }
    }

    /// Walks each expression of `exprs` in order.
    fn exprs<'a>(&mut self, exprs: impl Iterator<Item = &'a Expr>) {
        for expr in exprs {
            self.expr(expr);
        }
    }

    /// Walks the statements of `block` in order.
    fn block(&mut self, block: &Block) {
        for stmt in &block.stmts {
            match stmt {
                Stmt::Local(local) => {
                    if let Some(init) = &local.init {
                        self.expr(&init.expr);
                        self.exprs(init.diverge.iter().map(|(_, other)| other.as_ref()));
                    }
                }
                Stmt::Expr(expr, _) => self.expr(expr),
                Stmt::Item(_) | Stmt::Macro(_) => {}
            }
        }
    }
}
