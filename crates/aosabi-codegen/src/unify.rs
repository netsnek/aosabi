// SPDX-License-Identifier: MIT
/*
 * aosabi-codegen: whether two payload types of a generic enum can be equal.
 *
 * No analogue on the reference side. The test mirrors the overlap check of
 * rustc, which unifies the headers of two implementations with an occurs
 * check, https://rustc-dev-guide.rust-lang.org/coherence.html.
 */

//! [`can_equal`], whether some choice of the type parameters of an enum makes
//! two of its payload types one type.
//!
//! The derive gives a payload enum `impl From<Payload>` per variant, and two
//! of them conflict exactly where some choice of the parameters makes their
//! payload types equal, `From<T>` beside `From<String>` at `T = String`. The
//! test unifies the two types by their syntax: a type parameter takes any
//! type that does not contain it, a path compares its last segment and its
//! arguments, and a reference, a pointer, a slice, an array and a tuple
//! compare their parts. A path compares by its last segment, so
//! `std::string::String` equals `String`. A part it cannot read, a projection
//! of a parameter, `Self`, a trait object, `impl Trait`, a function pointer
//! or a macro, equals the other side where it names a parameter and where its
//! text is the same, so the test errs towards equal and the derive then
//! withholds a conversion rather than emitting one the overlap check refuses.

use proc_macro2::{
    TokenStream,
    TokenTree, //
};
use quote::ToTokens;
use syn::{
    Expr,
    GenericArgument,
    Ident,
    PathArguments,
    ReturnType,
    Type,
    TypePath, //
};

/// Returns whether some choice of the type parameters `params` makes `a` and
/// `b` one type.
pub(crate) fn can_equal(a: &Type, b: &Type, params: &[&Ident]) -> bool {
    Unifier {
        params,
        binding: Vec::new(),
    }
    .unify(a, b)
}

/// The state of one unification.
struct Unifier<'a> {
    /// The type parameters of the enum.
    params: &'a [&'a Ident],
    /// The type each bound parameter stands for.
    binding: Vec<(Ident, Type)>,
}

impl Unifier<'_> {
    /// Returns whether `a` and `b` unify, binding parameters on the way.
    fn unify(&mut self, a: &Type, b: &Type) -> bool {
        let (a, b) = (self.resolve(a), self.resolve(b));
        match (self.param_of(&a), self.param_of(&b)) {
            (Some(x), Some(y)) if x == y => return true,
            (Some(x), _) => return self.bind(x, b),
            (_, Some(y)) => return self.bind(y, a),
            _ => {}
        }
        match (&a, &b) {
            (Type::Path(x), Type::Path(y)) if self.is_plain(x) && self.is_plain(y) => {
                self.unify_paths(x, y)
            }
            (Type::Reference(x), Type::Reference(y)) => {
                x.mutability.is_some() == y.mutability.is_some() && self.unify(&x.elem, &y.elem)
            }
            (Type::Ptr(x), Type::Ptr(y)) => {
                x.mutability.is_some() == y.mutability.is_some() && self.unify(&x.elem, &y.elem)
            }
            (Type::Slice(x), Type::Slice(y)) => self.unify(&x.elem, &y.elem),
            (Type::Array(x), Type::Array(y)) => {
                lengths_can_equal(&x.len, &y.len) && self.unify(&x.elem, &y.elem)
            }
            (Type::Tuple(x), Type::Tuple(y)) => {
                x.elems.len() == y.elems.len()
                    && x.elems.iter().zip(&y.elems).all(|(x, y)| self.unify(x, y))
            }
            (Type::Never(_), Type::Never(_)) => true,
            _ if self.is_opaque(&a) || self.is_opaque(&b) => {
                self.mentions_param(&a) || self.mentions_param(&b) || text(&a) == text(&b)
            }
            _ => false,
        }
    }

    /// Returns `ty` without parentheses and with each bound parameter
    /// replaced by its type.
    fn resolve(&self, ty: &Type) -> Type {
        let mut ty = strip(ty).clone();
        while let Some(param) = self.param_of(&ty) {
            match self.binding.iter().find(|(bound, _)| bound == param) {
                Some((_, bound)) => ty = strip(bound).clone(),
                None => break,
            }
        }
        ty
    }

    /// Binds the parameter `param` to `ty`, which fails where `ty` contains
    /// the parameter, the occurs check.
    ///
    /// A type with a part the test does not read binds nothing and counts as
    /// equal, since a projection in it may stand for any type.
    fn bind(&mut self, param: &Ident, ty: Type) -> bool {
        if !self.is_readable(&ty) {
            return true;
        }
        if self.occurs(param, &ty) {
            return false;
        }
        self.binding.push((param.clone(), ty));
        true
    }

    /// Returns whether the parameter `param` occurs in the readable type
    /// `ty`, through the types bound to the parameters it names.
    fn occurs(&self, param: &Ident, ty: &Type) -> bool {
        let ty = self.resolve(ty);
        if let Some(found) = self.param_of(&ty) {
            return found == param;
        }
        match &ty {
            Type::Path(path) => path
                .path
                .segments
                .iter()
                .any(|segment| match &segment.arguments {
                    PathArguments::None => false,
                    PathArguments::AngleBracketed(arguments) => {
                        arguments.args.iter().any(|argument| match argument {
                            GenericArgument::Type(inner) => self.occurs(param, inner),
                            _ => false,
                        })
                    }
                    PathArguments::Parenthesized(arguments) => {
                        arguments
                            .inputs
                            .iter()
                            .any(|inner| self.occurs(param, inner))
                            || match &arguments.output {
                                ReturnType::Default => false,
                                ReturnType::Type(_, inner) => self.occurs(param, inner),
                            }
                    }
                }),
            Type::Reference(inner) => self.occurs(param, &inner.elem),
            Type::Ptr(inner) => self.occurs(param, &inner.elem),
            Type::Slice(inner) => self.occurs(param, &inner.elem),
            Type::Array(inner) => self.occurs(param, &inner.elem),
            Type::Tuple(tuple) => tuple.elems.iter().any(|inner| self.occurs(param, inner)),
            _ => false,
        }
    }

    /// Returns whether the test reads every part of `ty`.
    fn is_readable(&self, ty: &Type) -> bool {
        match strip(ty) {
            Type::Path(path) => {
                self.is_plain(path)
                    && path
                        .path
                        .segments
                        .iter()
                        .all(|segment| match &segment.arguments {
                            PathArguments::None => true,
                            PathArguments::AngleBracketed(arguments) => {
                                arguments.args.iter().all(|argument| match argument {
                                    GenericArgument::Type(inner) => self.is_readable(inner),
                                    GenericArgument::Lifetime(_) | GenericArgument::Const(_) => {
                                        true
                                    }
                                    _ => false,
                                })
                            }
                            PathArguments::Parenthesized(arguments) => {
                                arguments.inputs.iter().all(|inner| self.is_readable(inner))
                                    && match &arguments.output {
                                        ReturnType::Default => true,
                                        ReturnType::Type(_, inner) => self.is_readable(inner),
                                    }
                            }
                        })
            }
            Type::Reference(inner) => self.is_readable(&inner.elem),
            Type::Ptr(inner) => self.is_readable(&inner.elem),
            Type::Slice(inner) => self.is_readable(&inner.elem),
            Type::Array(inner) => self.is_readable(&inner.elem),
            Type::Tuple(tuple) => tuple.elems.iter().all(|inner| self.is_readable(inner)),
            Type::Never(_) => true,
            _ => false,
        }
    }

    /// Returns the parameter `ty` names, `None` for any other type.
    fn param_of<'p>(&self, ty: &'p Type) -> Option<&'p Ident> {
        let Type::Path(path) = ty else {
            return None;
        };
        let ident = path.path.get_ident()?;
        (path.qself.is_none() && self.params.contains(&ident)).then_some(ident)
    }

    /// Returns whether `path` is a path the test compares, one without a
    /// qualified self, that is not `Self` and that is no projection of a
    /// parameter.
    fn is_plain(&self, path: &TypePath) -> bool {
        let Some(first) = path.path.segments.first() else {
            return false;
        };
        path.qself.is_none()
            && first.ident != "Self"
            && !(path.path.segments.len() > 1 && self.params.contains(&&first.ident))
    }

    /// Returns whether `ty` is a type the test does not read.
    fn is_opaque(&self, ty: &Type) -> bool {
        match ty {
            Type::Path(path) => !self.is_plain(path),
            Type::Reference(_)
            | Type::Ptr(_)
            | Type::Slice(_)
            | Type::Array(_)
            | Type::Tuple(_)
            | Type::Never(_) => false,
            _ => true,
        }
    }

    /// Returns whether `ty` names a type parameter anywhere.
    fn mentions_param(&self, ty: &Type) -> bool {
        let tokens = ty.to_token_stream();
        self.params.iter().any(|param| mentions(&tokens, param))
    }

    /// Returns whether two paths unify by their last segments.
    fn unify_paths(&mut self, a: &TypePath, b: &TypePath) -> bool {
        let (Some(x), Some(y)) = (a.path.segments.last(), b.path.segments.last()) else {
            return false;
        };
        if x.ident != y.ident {
            return false;
        }
        match (&x.arguments, &y.arguments) {
            (PathArguments::AngleBracketed(x), PathArguments::AngleBracketed(y))
                if x.args.len() == y.args.len() =>
            {
                x.args
                    .iter()
                    .zip(&y.args)
                    .all(|(x, y)| self.unify_arguments(x, y))
            }
            (PathArguments::Parenthesized(x), PathArguments::Parenthesized(y)) => {
                x.inputs.len() == y.inputs.len()
                    && x.inputs
                        .iter()
                        .zip(&y.inputs)
                        .all(|(x, y)| self.unify(x, y))
                    && match (&x.output, &y.output) {
                        (ReturnType::Default, ReturnType::Default) => true,
                        (ReturnType::Type(_, x), ReturnType::Type(_, y)) => self.unify(x, y),
                        _ => false,
                    }
            }
            // No arguments on either side, or a different count or kind,
            // which a default parameter left out can explain.
            _ => true,
        }
    }

    /// Returns whether two generic arguments unify.
    fn unify_arguments(&mut self, a: &GenericArgument, b: &GenericArgument) -> bool {
        match (a, b) {
            (GenericArgument::Type(x), GenericArgument::Type(y)) => self.unify(x, y),
            (GenericArgument::Const(x), GenericArgument::Const(y)) => lengths_can_equal(x, y),
            // A lifetime unifies with any lifetime, as the overlap check
            // treats lifetimes, and an argument the test does not read
            // errs towards equal.
            _ => true,
        }
    }
}

/// Returns whether two constant expressions can be one value, which only
/// two different literals rule out.
fn lengths_can_equal(a: &Expr, b: &Expr) -> bool {
    match (a, b) {
        (Expr::Lit(x), Expr::Lit(y)) => {
            x.to_token_stream().to_string() == y.to_token_stream().to_string()
        }
        _ => true,
    }
}

/// Returns `ty` without the parentheses and the invisible groups around it.
fn strip(ty: &Type) -> &Type {
    match ty {
        Type::Paren(inner) => strip(&inner.elem),
        Type::Group(inner) => strip(&inner.elem),
        _ => ty,
    }
}

/// Returns the text of `ty`.
fn text(ty: &Type) -> String {
    ty.to_token_stream().to_string()
}

/// Returns whether `tokens` hold the identifier `ident` at any depth.
fn mentions(tokens: &TokenStream, ident: &Ident) -> bool {
    tokens.clone().into_iter().any(|tree| match tree {
        TokenTree::Ident(found) => found == *ident,
        TokenTree::Group(group) => mentions(&group.stream(), ident),
        TokenTree::Punct(_) | TokenTree::Literal(_) => false,
    })
}
