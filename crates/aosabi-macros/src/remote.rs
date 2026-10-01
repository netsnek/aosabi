// SPDX-License-Identifier: MIT
/*
 * aosabi-macros: the key `#[aosabi(arguments)]` of the types of a pulled
 * registry.
 *
 * No analogue on the reference side, where a field with arguments is a method
 * of the interface of a pulled registry.
 */

//! The key `#[aosabi(arguments)]`, which describes the arguments of a remote
//! field as a parameter list.
//!
//! A struct under `arguments` implements `Params` with one parameter per
//! field in field order. A field `Option<T>` is the optional parameter
//! `name?: T`, which is how the reference writes a nullable argument. The key
//! `#[aosabi(remote)]` has a glue alone, which the module `remote` of
//! `aosabi-codegen` writes.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    DeriveInput,
    Fields,
    GenericArgument,
    PathArguments,
    Type, //
};

use crate::{
    docs,
    emit::unraw,
    links::type_link, //
};

/// Returns `impl Params` of the struct `input` under `#[aosabi(arguments)]`
/// with the named `fields`, one `ParamInfo` per field in field order.
///
/// # Errors
///
/// Returns the refusals of a field type that has no description and of the
/// doc comments of a field.
pub(crate) fn params_impl(input: &DeriveInput, fields: &Fields) -> syn::Result<TokenStream> {
    let ident = &input.ident;
    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();
    let params = fields
        .iter()
        .filter_map(|field| field.ident.as_ref().map(|member| (member, field)))
        .map(|(member, field)| {
            let name = unraw(member);
            let (ty, optional) =
                option_inner(&field.ty).map_or((&field.ty, false), |inner| (inner, true));
            let link = type_link(ty)?;
            let documentation = docs::item(&field.attrs)?;
            let docs = documentation.to_tokens();
            let site = documentation.site_tokens(member.span());
            Ok(quote! {
                ::aosabi::descriptor::ParamInfo {
                    name: #name,
                    r#type: #link,
                    optional: #optional,
                    docs: #docs,
                    site: #site,
                }
            })
        })
        .collect::<syn::Result<Vec<TokenStream>>>()?;
    Ok(quote! {
        #[automatically_derived]
        impl #impl_generics ::aosabi::remote::Params for #ident #type_generics #where_clause {
            const PARAMS: &'static [::aosabi::descriptor::ParamInfo] = &[#(#params),*];
        }
    })
}

/// Returns `T` of a type written `Option<T>`, with or without a path to
/// `Option`.
fn option_inner(ty: &Type) -> Option<&Type> {
    let Type::Path(path) = ty else {
        return None;
    };
    let last = path.path.segments.last()?;
    if last.ident != "Option" {
        return None;
    }
    let PathArguments::AngleBracketed(arguments) = &last.arguments else {
        return None;
    };
    match arguments.args.first() {
        Some(GenericArgument::Type(inner)) if arguments.args.len() == 1 => Some(inner),
        _ => None,
    }
}
