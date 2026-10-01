// SPDX-License-Identifier: MIT
/*
 * aosabi-macros: `#[derive(Aosabi)]` on an enum.
 *
 * No analogue on the reference side.
 */

//! `#[derive(Aosabi)]` on an enum.

use aosabi_codegen::TypeItem;
use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    DataEnum,
    DeriveInput,
    Fields,
    Type,
    Variant,
    spanned::Spanned, //
};

use crate::{
    attrs,
    docs,
    emit::{
        Descriptor,
        Kind,
        Lists,
        unraw, //
    },
    glue,
    links::type_link,
    refuse::{
        ARGUMENTS_SHAPE,
        ENUM_MIXES_VARIANT_SHAPES,
        REMOTE_SHAPE,
        VALUE_ON_PAYLOAD_VARIANT,
        refusal, //
    }, //
};

/// Returns the code the derive emits for the enum `input` with `data`.
///
/// The first variant decides the shape every variant has to share. Variants
/// with no data give `TypeKind::Enum` with one `VariantInfo` each, or
/// `TypeKind::EnumDeclaration` under `#[aosabi(enum)]`, and
/// variants with one unnamed field give `TypeKind::Union` with one member
/// link each, both in declaration order. The run time glue of the enum
/// follows its descriptor.
///
/// The key `anonymous` on an enum whose variants carry a payload makes it the
/// twin of a union TypeScript infers and no alias names, the patch of a
/// federation that answers one of two object literals among them, so the
/// union presents no alias symbol and the inference names it by the property
/// it is reached through.
///
/// The key `remote` on an enum whose variants carry a payload makes it an
/// interface with possible types or a union of a pulled registry, which reads
/// a delegated value as the variant whose payload carries its `__typename`.
/// Every enum whose variants carry no data reads a delegated value by its
/// value text, the value of a remote enum.
///
/// # Errors
///
/// Returns the refusal of a variant of another shape, of an unknown key, of
/// `remote` on an enum whose variants carry no data, of `arguments` on an
/// enum, of a payload type that has no description and of a lifetime or a
/// const parameter.
pub(crate) fn expand(input: &DeriveInput, data: &DataEnum) -> syn::Result<TokenStream> {
    let carries_payload = data
        .variants
        .first()
        .is_some_and(|first| payload(first).is_some());
    let keys = attrs::type_attrs(&input.attrs)?;
    if let Some(span) = keys.arguments {
        return Err(refusal(span, ARGUMENTS_SHAPE));
    }
    if let (Some(span), false) = (keys.remote, carries_payload) {
        return Err(refusal(span, REMOTE_SHAPE));
    }
    let (kind, lists) = if carries_payload {
        let members = data
            .variants
            .iter()
            .map(member)
            .collect::<syn::Result<_>>()?;
        (
            Kind::Union,
            Lists {
                members,
                ..Lists::default()
            },
        )
    } else {
        let variants = data
            .variants
            .iter()
            .map(variant_info)
            .collect::<syn::Result<_>>()?;
        (
            Kind::Enum,
            Lists {
                variants,
                ..Lists::default()
            },
        )
    };
    let described = Descriptor::of_derive(input, kind, lists)?.emit()?;
    let values = if carries_payload {
        Vec::new()
    } else {
        data.variants
            .iter()
            .map(|variant| {
                let value = attrs::variant_attrs(&variant.attrs)?
                    .value
                    .map_or_else(|| unraw(&variant.ident), |value| value.value());
                Ok((&variant.ident, value))
            })
            .collect::<syn::Result<Vec<_>>>()?
    };
    let item = TypeItem {
        ident: &input.ident,
        generics: &input.generics,
    };
    let glue = glue::enumeration(&item, &data.variants, keys.remote.is_some(), &values);
    Ok(quote!(#described #glue))
}

/// Returns the payload type of `variant` when it carries exactly one unnamed
/// field.
fn payload(variant: &Variant) -> Option<&Type> {
    match &variant.fields {
        Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
            fields.unnamed.first().map(|field| &field.ty)
        }
        _ => None,
    }
}

/// Returns the `VariantInfo` literal of a variant that carries no data.
fn variant_info(variant: &Variant) -> syn::Result<TokenStream> {
    if !matches!(variant.fields, Fields::Unit) {
        return Err(refusal(variant.span(), ENUM_MIXES_VARIANT_SHAPES));
    }
    let name = unraw(&variant.ident);
    let value = attrs::variant_attrs(&variant.attrs)?
        .value
        .map_or_else(|| name.clone(), |value| value.value());
    let documentation = docs::item(&variant.attrs)?;
    let docs = documentation.to_tokens();
    let site = documentation.site_tokens(variant.ident.span());
    Ok(quote! {
        ::aosabi::descriptor::VariantInfo {
            name: #name,
            value: #value,
            docs: #docs,
            site: #site,
        }
    })
}

/// Returns the member link of a variant that carries one payload.
fn member(variant: &Variant) -> syn::Result<TokenStream> {
    let Some(ty) = payload(variant) else {
        return Err(refusal(variant.span(), ENUM_MIXES_VARIANT_SHAPES));
    };
    if let Some(value) = attrs::variant_attrs(&variant.attrs)?.value {
        return Err(refusal(value.span(), VALUE_ON_PAYLOAD_VARIANT));
    }
    type_link(ty)
}
