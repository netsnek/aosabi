// SPDX-License-Identifier: MIT
/*
 * aosabi-macros: the link from a member to its type.
 *
 * The link stands in for the `ts.Type` the checker hands the inference for a
 * member, and the refusals are rows of the table of
 * docs/schema-inference.md section 13.
 */

//! The link from a member to its type, and the refusals a macro can decide
//! from the tokens alone.
//!
//! A link to a foreign type, to an alias and to a generic instantiation is the
//! same expression over the tokens of the type. The compiler resolves the
//! alias and the instantiation, and a type links when it implements
//! `Described` and refuses otherwise, with the message that trait carries. The
//! macro never inspects what the tokens name. What it does refuse is a shape
//! whose message would mislead when it came from the trait: a tuple, a
//! function pointer, `impl Trait` and `dyn Trait`, in any depth of the type.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    GenericArgument,
    PathArguments,
    ReturnType,
    Type,
    TypePath,
    spanned::Spanned, //
};

use crate::refuse::{
    FUNCTION_TYPE,
    TUPLE_TYPE,
    UNNAMEABLE_TYPE,
    refusal, //
};

/// Returns the expression `::aosabi::descriptor::TypeLink::of::<T>()` for `ty`.
///
/// # Errors
///
/// Returns the refusal of a tuple other than `()`, of a function pointer, of
/// `impl Trait` and of `dyn Trait`, at the span of those tokens.
pub(crate) fn type_link(ty: &Type) -> syn::Result<TokenStream> {
    check(ty)?;
    Ok(quote!(::aosabi::descriptor::TypeLink::of::<#ty>()))
}

/// Refuses the shapes of `ty` that have no description, in any depth.
fn check(ty: &Type) -> syn::Result<()> {
    match ty {
        Type::Tuple(tuple) if tuple.elems.is_empty() => Ok(()),
        Type::Tuple(tuple) => Err(refusal(tuple.span(), TUPLE_TYPE)),
        Type::BareFn(function) => Err(refusal(function.span(), FUNCTION_TYPE)),
        Type::ImplTrait(_) | Type::TraitObject(_) => Err(refusal(ty.span(), UNNAMEABLE_TYPE)),
        Type::Array(array) => check(&array.elem),
        Type::Slice(slice) => check(&slice.elem),
        Type::Reference(reference) => check(&reference.elem),
        Type::Paren(paren) => check(&paren.elem),
        Type::Group(group) => check(&group.elem),
        Type::Path(path) => check_path(path),
        _ => Ok(()),
    }
}

/// Checks the qualified self type and every generic argument of `path`.
fn check_path(path: &TypePath) -> syn::Result<()> {
    if let Some(qself) = &path.qself {
        check(&qself.ty)?;
    }
    path.path
        .segments
        .iter()
        .try_for_each(|segment| check_arguments(&segment.arguments))
}

/// Checks every type among the arguments of one path segment.
fn check_arguments(arguments: &PathArguments) -> syn::Result<()> {
    match arguments {
        PathArguments::None => Ok(()),
        PathArguments::AngleBracketed(list) => list.args.iter().try_for_each(|arg| match arg {
            GenericArgument::Type(ty) => check(ty),
            _ => Ok(()),
        }),
        PathArguments::Parenthesized(list) => {
            list.inputs.iter().try_for_each(check)?;
            match &list.output {
                ReturnType::Default => Ok(()),
                ReturnType::Type(_, ty) => check(ty),
            }
        }
    }
}
