// SPDX-License-Identifier: MIT
/*
 * aosabi-macros: `#[aosabi::scalar]`.
 *
 * No analogue on the reference side.
 */

//! `#[aosabi::scalar]`.
//!
//! The documentation comment of the newtype is the documentation of its type
//! and never the description of the `scalar` line, which the printer writes
//! bare. It reaches the schema as the description of a non null input field
//! typed by the scalar, quirk `Q15`.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    Fields,
    Item,
    ItemStruct,
    LitStr,
    spanned::Spanned, //
};

use crate::{
    docs,
    emit::{
        self,
        Descriptor,
        Kind,
        Lists,
        unraw, //
    },
    glue,
    refuse::{
        DUPLICATE_KEY,
        SCALAR_NOT_NEWTYPE,
        SCALAR_WITH_GENERICS,
        UNKNOWN_SCALAR_KEY,
        refusal, //
    },
};

/// Returns the item unchanged, followed by its description as a scalar.
///
/// `args` is the argument list of the attribute and `item` the item it stands
/// on. A refusal keeps the item in the output, so the one error a person reads
/// is the refusal and not a cascade about a missing type.
pub(crate) fn expand(args: TokenStream, item: &TokenStream) -> TokenStream {
    let described = describe(args, item.clone()).unwrap_or_else(|error| {
        let mut refused = error.to_compile_error();
        refused.extend(refused_declaration(item));
        refused
    });
    quote!(#item #described)
}

/// Returns the declaration constant a refused attribute emits, when `item` is
/// a struct, an enum or a union, the items the scan of the declaration list
/// takes the attribute on.
fn refused_declaration(item: &TokenStream) -> TokenStream {
    match syn::parse2(item.clone()) {
        Ok(Item::Struct(item)) => emit::refused_declaration(&item.ident),
        Ok(Item::Enum(item)) => emit::refused_declaration(&item.ident),
        Ok(Item::Union(item)) => emit::refused_declaration(&item.ident),
        _ => TokenStream::new(),
    }
}

/// Returns `impl Described`, the declaration constant and the run time glue
/// of the newtype `item`.
fn describe(args: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let name = scalar_name(args)?;
    let item = newtype(syn::parse2(item)?)?;
    let descriptor = Descriptor {
        ident: &item.ident,
        generics: &item.generics,
        kind: Kind::Scalar,
        name: name.map_or_else(|| unraw(&item.ident), |name| name.value()),
        docs: docs::item(&item.attrs)?,
        typename: None,
        anonymous: false,
        lists: Lists::default(),
        heritage: Vec::new(),
    };
    let described = descriptor.emit()?;
    let glue = glue::scalar(&item);
    Ok(quote!(#described #glue))
}

/// Returns the `name = "Name"` of the argument list, when it states one.
fn scalar_name(args: TokenStream) -> syn::Result<Option<LitStr>> {
    let mut name = None;
    let parser = syn::meta::parser(|meta| {
        if !meta.path.is_ident("name") {
            return Err(meta.error(UNKNOWN_SCALAR_KEY));
        }
        if name.is_some() {
            return Err(meta.error(DUPLICATE_KEY));
        }
        name = Some(meta.value()?.parse()?);
        Ok(())
    });
    syn::parse::Parser::parse2(parser, args)?;
    Ok(name)
}

/// Returns `item` when it is a struct with exactly one unnamed field and no
/// generic parameter.
fn newtype(item: Item) -> syn::Result<ItemStruct> {
    let Item::Struct(item) = item else {
        return Err(refusal(item.span(), SCALAR_NOT_NEWTYPE));
    };
    if !matches!(&item.fields, Fields::Unnamed(fields) if fields.unnamed.len() == 1) {
        return Err(refusal(item.ident.span(), SCALAR_NOT_NEWTYPE));
    }
    if !item.generics.params.is_empty() {
        return Err(refusal(item.generics.span(), SCALAR_WITH_GENERICS));
    }
    Ok(item)
}
