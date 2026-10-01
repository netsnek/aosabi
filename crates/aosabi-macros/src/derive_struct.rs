// SPDX-License-Identifier: MIT
/*
 * aosabi-macros: `#[derive(Aosabi)]` on a struct.
 *
 * No analogue on the reference side.
 */

//! `#[derive(Aosabi)]` on a struct.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    DataStruct,
    DeriveInput,
    Field,
    Fields,
    Ident,
    spanned::Spanned, //
};

use aosabi_codegen::TypeItem;

use crate::{
    attrs,
    docs,
    emit::{
        Descriptor,
        Kind,
        Lists,
        unraw,
        visibility, //
    },
    glue,
    links::type_link,
    refuse::{
        DERIVE_ON_TUPLE_STRUCT,
        EMBEDDED_BASE_UNNAMED,
        REMOTE_SHAPE,
        REMOTE_WITHOUT_TYPENAME,
        SECOND_EMBEDDED_BASE,
        SPREAD_BESIDE_BASE,
        refusal, //
    },
    remote,
    spread, //
};

/// Returns the code the derive emits for the struct `input` with `data`.
///
/// A struct with named fields lists them in declaration order, and a unit
/// struct lists none. A private field is recorded with its visibility and not
/// left out, because the inference decides what it skips. The run time glue
/// of the struct follows its descriptor, and for a struct with named fields
/// it includes the reader of an input object, so the struct stands as a
/// resolver parameter.
///
/// A field under `#[aosabi(extends)]` is the embedded base of the struct. It
/// is no field of the descriptor, it states the heritage `Embedded` first,
/// ahead of the bases of the type attribute, and the derive implements
/// `Deref` to it.
///
/// A field under `#[aosabi(spread)]` stands for an object spread. It is a
/// field of the descriptor with `spread` set, at its place in the source,
/// which the inference replaces by the members of its type, and the object of
/// the struct answers a key through it.
///
/// A struct under `#[aosabi(remote)]` is an object of a pulled registry. Its
/// typename is its identifier unless the type states one, its field
/// `__typename` is the slot of the remote object and no field of the
/// descriptor, and its object answers the kept object. A struct under
/// `#[aosabi(arguments)]` implements `Params` over its fields in field order,
/// a unit struct over none.
///
/// # Errors
///
/// Returns the refusal of a tuple struct, of an unknown key, of a second
/// embedded base, of a spread beside an embedded base, of `remote` on a
/// struct with no field `__typename`, with an embedded base, with a spread
/// or with no field, of a field type that
/// has no description and of a lifetime or a const parameter.
pub(crate) fn expand(input: &DeriveInput, data: &DataStruct) -> syn::Result<TokenStream> {
    let keys = attrs::type_attrs(&input.attrs)?;
    let (fields, embedded, spreads, kept) = match &data.fields {
        Fields::Named(fields) => {
            let embedded = embedded_base(&fields.named)?;
            let spreads = spread::fields(&data.fields)?;
            if let (Some(_), Some(first)) = (embedded, spreads.first()) {
                return Err(refusal(first.span, SPREAD_BESIDE_BASE));
            }
            let kept = match keys.remote {
                Some(span) if embedded.is_some() || !spreads.is_empty() => {
                    return Err(refusal(span, REMOTE_SHAPE));
                }
                Some(span) => Some(
                    aosabi_codegen::remote::typename_slot(&data.fields)
                        .ok_or_else(|| refusal(span, REMOTE_WITHOUT_TYPENAME))?,
                ),
                None => None,
            };
            let infos = fields
                .named
                .iter()
                .filter(|field| embedded.is_none_or(|base| !core::ptr::eq(base, *field)))
                .filter(|field| kept.is_none() || field.ident.as_ref() != kept)
                .map(field_info)
                .collect::<syn::Result<_>>()?;
            (infos, embedded, spreads, kept)
        }
        Fields::Unit => {
            if let Some(span) = keys.remote {
                return Err(refusal(span, REMOTE_SHAPE));
            }
            (Vec::new(), None, Vec::new(), None)
        }
        Fields::Unnamed(fields) => return Err(refusal(fields.span(), DERIVE_ON_TUPLE_STRUCT)),
    };
    let lists = Lists {
        fields,
        ..Lists::default()
    };
    let mut descriptor = Descriptor::of_derive(input, Kind::Object, lists)?;
    if kept.is_some() && descriptor.typename.is_none() {
        descriptor.typename = Some(unraw(&input.ident));
    }
    let deref = match embedded {
        Some(base) => {
            descriptor.heritage.insert(0, embedded_heritage(base)?);
            deref_impl(input, base)
        }
        None => TokenStream::new(),
    };
    let params = match keys.arguments {
        Some(_) => remote::params_impl(input, &data.fields)?,
        None => TokenStream::new(),
    };
    let described = descriptor.emit()?;
    let item = TypeItem {
        ident: &input.ident,
        generics: &input.generics,
    };
    let typename = keys.typename.as_ref().map(syn::LitStr::value);
    let glue = glue::object(
        &item,
        &data.fields,
        typename.as_deref(),
        embedded,
        &spreads,
        kept,
    );
    Ok(quote!(#described #glue #deref #params))
}

/// Returns the field under `#[aosabi(extends)]`, `None` when no field
/// carries the key.
///
/// # Errors
///
/// Returns the refusal of a second such field and of an unknown key on a
/// field.
fn embedded_base<'a>(
    fields: impl IntoIterator<Item = &'a Field>,
) -> syn::Result<Option<&'a Field>> {
    let mut base: Option<&Field> = None;
    for field in fields {
        let Some(span) = attrs::field_attrs(&field.attrs)?.extends else {
            continue;
        };
        if base.is_some() {
            return Err(refusal(span, SECOND_EMBEDDED_BASE));
        }
        if field.ident.is_none() {
            return Err(refusal(span, EMBEDDED_BASE_UNNAMED));
        }
        base = Some(field);
    }
    Ok(base)
}

/// Returns the `HeritageInfo` literal of the embedded base `field`, the
/// `extends` clause of the struct.
fn embedded_heritage(field: &Field) -> syn::Result<TokenStream> {
    let name = field.ident.as_ref().map(unraw).unwrap_or_default();
    let base = type_link(&field.ty)?;
    Ok(quote! {
        ::aosabi::descriptor::HeritageInfo {
            kind: ::aosabi::descriptor::HeritageKind::Embedded { field: #name },
            base: #base,
        }
    })
}

/// Returns `impl Deref` to the embedded base `field`, so a member of the base
/// reads through the struct as `this.id` reads an inherited member.
fn deref_impl(input: &DeriveInput, field: &Field) -> TokenStream {
    let ident = &input.ident;
    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();
    let (member, ty) = (&field.ident, &field.ty);
    quote! {
        #[automatically_derived]
        impl #impl_generics ::core::ops::Deref for #ident #type_generics #where_clause {
            type Target = #ty;

            fn deref(&self) -> &Self::Target {
                &self.#member
            }
        }
    }
}

/// Returns the `FieldInfo` literal of `field`, with `spread` set for a field
/// under `#[aosabi(spread)]`.
fn field_info(field: &Field) -> syn::Result<TokenStream> {
    let spread = attrs::field_attrs(&field.attrs)?.spread.is_some();
    let name = field.ident.as_ref().map(unraw);
    let documentation = docs::item(&field.attrs)?;
    let docs = documentation.to_tokens();
    let site = documentation.site_tokens(
        field
            .ident
            .as_ref()
            .map_or_else(|| field.ty.span(), Ident::span),
    );
    let visibility = visibility(&field.vis);
    let link = type_link(&field.ty)?;
    Ok(quote! {
        ::aosabi::descriptor::FieldInfo {
            name: #name,
            docs: #docs,
            site: #site,
            visibility: #visibility,
            r#type: #link,
            spread: #spread,
        }
    })
}
