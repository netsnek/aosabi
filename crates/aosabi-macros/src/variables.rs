// SPDX-License-Identifier: MIT
/*
 * aosabi-macros: #[derive(Variables)].
 *
 * No analogue on the reference side.
 */

//! `#[derive(Variables)]`.
//!
//! The derive gives every field of the struct a typed key, the associated
//! constant `<field>: Key<<type>>` named as the field, and implements
//! `Variables`, so `c.set(Variables::requestId, id)` reads as
//! `c.set('requestId', id)` does and is checked. The constants carry the
//! doc comments of their fields and stand under the lint allowances,
//! because a key is named as the variable of the reference.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    Data,
    DeriveInput,
    Fields, //
};

use crate::{
    emit::unraw,
    lints,
    refuse::{
        CONTEXT_DERIVE_NOT_NAMED,
        refusal, //
    },
};

/// Expands one use of `#[derive(Variables)]`.
pub(crate) fn expand(input: &TokenStream) -> TokenStream {
    syn::parse2::<DeriveInput>(input.clone())
        .and_then(|input| variables(&input))
        .unwrap_or_else(|error| error.to_compile_error())
}

/// Returns the keys of `input` and `impl Variables`.
fn variables(input: &DeriveInput) -> syn::Result<TokenStream> {
    let Data::Struct(data) = &input.data else {
        return Err(refusal(input.ident.span(), CONTEXT_DERIVE_NOT_NAMED));
    };
    let Fields::Named(fields) = &data.fields else {
        return Err(refusal(input.ident.span(), CONTEXT_DERIVE_NOT_NAMED));
    };
    let key = quote!(::aosabi::__private::glue::Key);
    let constants = fields.named.iter().filter_map(|field| {
        let member = field.ident.as_ref()?;
        let (name, ty) = (unraw(member), &field.ty);
        let docs = field
            .attrs
            .iter()
            .filter(|attr| attr.path().is_ident("doc"));
        Some(quote! {
            #(#docs)*
            pub const #member: #key<#ty> = #key::new(#name);
        })
    });
    let ident = &input.ident;
    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();
    let allow = lints::allow();
    Ok(quote! {
        #allow
        impl #impl_generics #ident #type_generics #where_clause {
            #(#constants)*
        }

        #[automatically_derived]
        impl #impl_generics ::aosabi::__private::glue::Variables for #ident #type_generics #where_clause {}
    })
}
