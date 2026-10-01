// SPDX-License-Identifier: MIT
/*
 * aosabi-macros: #[derive(Bindings)].
 *
 * No analogue on the reference side.
 */

//! `#[derive(Bindings)]`.
//!
//! The derive implements `Bindings` with `current()`, which reads every
//! field from the environment of the context of the running request under
//! the name of the field. A field is `String`, which throws when the binding
//! is not set, or `Option<String>`.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    Data,
    DeriveInput,
    Fields,
    GenericArgument,
    PathArguments,
    Type,
    spanned::Spanned, //
};

use crate::{
    emit::unraw,
    refuse::{
        BINDING_FIELD_TYPE,
        CONTEXT_DERIVE_NOT_NAMED,
        refusal, //
    },
};

/// Expands one use of `#[derive(Bindings)]`.
pub(crate) fn expand(input: &TokenStream) -> TokenStream {
    syn::parse2::<DeriveInput>(input.clone())
        .and_then(|input| bindings(&input))
        .unwrap_or_else(|error| error.to_compile_error())
}

/// Returns `impl Bindings` of `input`.
fn bindings(input: &DeriveInput) -> syn::Result<TokenStream> {
    let Data::Struct(data) = &input.data else {
        return Err(refusal(input.ident.span(), CONTEXT_DERIVE_NOT_NAMED));
    };
    let Fields::Named(fields) = &data.fields else {
        return Err(refusal(input.ident.span(), CONTEXT_DERIVE_NOT_NAMED));
    };
    let mut reads = Vec::new();
    for field in &fields.named {
        if !is_string(&field.ty) && !is_optional_string(&field.ty) {
            return Err(refusal(field.ty.span(), BINDING_FIELD_TYPE));
        }
        let Some(member) = &field.ident else {
            continue;
        };
        let (name, ty) = (unraw(member), &field.ty);
        reads.push(quote!(#member: ::aosabi::__private::glue::binding::<#ty>(#name)?));
    }
    let ident = &input.ident;
    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();
    Ok(quote! {
        #[automatically_derived]
        impl #impl_generics ::aosabi::__private::glue::Bindings for #ident #type_generics #where_clause {
            fn current() -> ::core::result::Result<Self, ::aosabi::__private::glue::Thrown> {
                ::core::result::Result::Ok(Self {
                    #(#reads,)*
                })
            }
        }
    })
}

/// Returns whether `ty` is the path `String`.
fn is_string(ty: &Type) -> bool {
    let Type::Path(path) = ty else {
        return false;
    };
    path.qself.is_none()
        && path
            .path
            .segments
            .last()
            .is_some_and(|last| last.ident == "String" && last.arguments.is_none())
}

/// Returns whether `ty` is `Option<String>`.
fn is_optional_string(ty: &Type) -> bool {
    let Type::Path(path) = ty else {
        return false;
    };
    let Some(last) = path.path.segments.last() else {
        return false;
    };
    if path.qself.is_some() || last.ident != "Option" {
        return false;
    }
    let PathArguments::AngleBracketed(arguments) = &last.arguments else {
        return false;
    };
    let mut arguments = arguments.args.iter();
    matches!(
        (arguments.next(), arguments.next()),
        (Some(GenericArgument::Type(inner)), None) if is_string(inner)
    )
}
