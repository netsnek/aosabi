// SPDX-License-Identifier: MIT
/*
 * aosabi-macros: the rule that tells a type path from a module path in an entry
 * of graphql!.
 *
 * No analogue on the reference side.
 */

//! The path rule of an entry.
//!
//! A reference `A::f` names a type when the segment before the last starts with
//! an upper case letter or is written `<A>`, and it reaches
//! `<A>::__AOSABI_STATIC_f`, which the resolvers block emits for every static.
//! It names a module otherwise and reaches `a::f::__AOSABI_RESOLVER`, which
//! `#[aosabi::resolver]` emits in the module `f` beside the function `f`. A
//! shorthand `f` reaches `f::__AOSABI_RESOLVER`. A reference the rule reads the
//! wrong way fails to compile and names the missing item.
//!
//! The paths stand in the hidden module of the fragment, one level below the
//! module of the invocation, so a path that opens with `self` or `super`
//! gains one `super`. The same holds for the path of a spread.

use proc_macro2::{
    Span,
    TokenStream, //
};
use quote::{
    format_ident,
    quote, //
};
use syn::{
    ExprPath,
    Ident,
    Path,
    PathSegment,
    spanned::Spanned, //
};

use crate::{
    emit::unraw,
    refuse::{
        GRAPHQL_TRAIT_PATH,
        refusal, //
    },
};

/// The prefix of the associated item a resolvers block emits for a static.
pub(crate) const STATIC_PREFIX: &str = "__AOSABI_STATIC_";

/// The item `#[aosabi::resolver]` emits in the module beside its function.
pub(crate) const RESOLVER_ITEM: &str = "__AOSABI_RESOLVER";

/// The prefix of the hidden module of a fragment.
pub(crate) const FRAGMENT_PREFIX: &str = "__aosabi_fragment_";

/// Returns the identifier of the associated item of the static `name`.
pub(crate) fn static_item(name: &Ident) -> Ident {
    format_ident!("{STATIC_PREFIX}{}", unraw(name), span = name.span())
}

/// Returns the identifier of the hidden module of the fragment `name`.
pub(crate) fn fragment_module(name: &Ident) -> Ident {
    format_ident!("{FRAGMENT_PREFIX}{}", unraw(name), span = name.span())
}

/// Returns the expression of the `EntryFunction` the reference `path`
/// reaches, from the hidden module.
///
/// # Errors
///
/// Returns the refusal of a path that names a trait item.
pub(crate) fn reference(path: &ExprPath) -> syn::Result<TokenStream> {
    if let Some(qself) = &path.qself {
        if qself.as_token.is_some() {
            return Err(refusal(path.span(), GRAPHQL_TRAIT_PATH));
        }
        let ty = &qself.ty;
        let mut rest = path.path.segments.iter().skip(qself.position);
        let (Some(last), None) = (rest.next(), rest.next()) else {
            return Err(refusal(path.span(), GRAPHQL_TRAIT_PATH));
        };
        let item = static_item(&last.ident);
        return Ok(quote!(<#ty>::#item));
    }
    let segments: Vec<&PathSegment> = path.path.segments.iter().collect();
    let names_type = segments.len() >= 2
        && segments[segments.len() - 2]
            .ident
            .to_string()
            .trim_start_matches("r#")
            .starts_with(|first: char| first.is_uppercase());
    if names_type {
        let (owner, last) = segments.split_at(segments.len() - 1);
        let owner = owned_path(path.path.leading_colon.is_some(), owner);
        let item = static_item(&last[0].ident);
        return Ok(quote!(<#owner>::#item));
    }
    let module = owned_path(path.path.leading_colon.is_some(), &segments);
    let item = Ident::new(RESOLVER_ITEM, Span::call_site());
    Ok(quote!(#module::#item))
}

/// Returns the expression of the `EntryFunction` of the shorthand `name`.
pub(crate) fn shorthand(name: &Ident) -> TokenStream {
    let item = Ident::new(RESOLVER_ITEM, Span::call_site());
    quote!(#name::#item)
}

/// Returns the path of the hidden module of the fragment the spread path
/// `path` names, from the hidden module of the invocation.
pub(crate) fn spread_module(path: &Path) -> TokenStream {
    let segments: Vec<&PathSegment> = path.segments.iter().collect();
    let Some((last, owner)) = segments.split_last() else {
        return TokenStream::new();
    };
    let module = fragment_module(&last.ident);
    if owner.is_empty() {
        return quote!(super::#module);
    }
    let owner = owned_path(path.leading_colon.is_some(), owner);
    quote!(#owner::#module)
}

/// Returns the path of `segments` as it reads from the hidden module, one
/// level below the module the segments were written in.
///
/// A path that opens with `self` or `super` gains one `super`, a path that
/// opens with `crate`, `::` or a name in scope stays as written, because the
/// hidden module imports every name of its parent.
fn owned_path(leading_colon: bool, segments: &[&PathSegment]) -> TokenStream {
    let first = segments.first().map(|segment| segment.ident.to_string());
    let tail = segments.iter();
    match first.as_deref() {
        Some("self") => {
            let rest = segments.iter().skip(1);
            quote!(super #(::#rest)*)
        }
        Some("super") => quote!(super::#(#tail)::*),
        _ if leading_colon => quote!(#(::#tail)*),
        _ => quote!(#(#tail)::*),
    }
}
