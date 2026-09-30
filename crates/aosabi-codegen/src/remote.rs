// SPDX-License-Identifier: MIT
/*
 * aosabi-codegen: the glue of the key `#[aosabi(remote)]` and of a spread
 * field, which read a delegated value into a type of a pulled registry.
 *
 * No analogue on the reference side, where a type of a pulled registry is a
 * TypeScript type the delegation hands back with the remote object it was
 * read from, and a field with arguments is a method of its interface.
 */

//! The glue of the key `#[aosabi(remote)]`, which keeps the remote object a
//! delegated record was read from, and of a struct with a spread field, which
//! reads the object its patch made. The key `#[aosabi(arguments)]`, which
//! describes the arguments of a remote field as a parameter list, is a
//! descriptor of `aosabi-macros` and has no glue.
//!
//! A struct under `remote` is an object of the remote. Its typename is its
//! identifier, its field `__typename: TypeName` is the slot that keeps the
//! remote object and no member of its description, as the reference skips
//! every member that starts with `__`, and a field `Callable<A, R>` is
//! described as the callable it stands for. The derive implements
//! `FromDelegated<Self>`, which reads each field as a member of the remote
//! object, leniently, and keeps the object in the slot, and the object of the
//! struct answers the kept object where there is one, so a member no field
//! names, an aliased one among them, reads as the remote carries it. A field
//! a patch or a resolver assigns, which differs from the member it was read
//! from, is answered from the field, as the reference's merge answers what a
//! patch assigned on the copy of the object it was handed.
//!
//! An enum under `remote` whose variants each carry one payload is an
//! interface with possible types or a union of the remote, which `aosabi pull`
//! writes as the union of its members. Its `FromDelegated<Self>` reads the
//! variant whose payload carries the `__typename` of the remote object.
//!
//! A struct under `arguments` implements `Params` with one parameter per
//! field in field order. A field `Option<T>` is the optional parameter
//! `name?: T`, which is how the reference writes a nullable argument.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    Fields,
    Generics,
    Ident,
    Type,
    Variant,
    WhereClause,
    parse_quote, //
};

use crate::{
    TypeItem,
    glue::{
        TYPENAME_KEY,
        glue, //
    },
    spread::Spread,
    unraw, //
};

/// Returns the path of `item` in the module `delegated` of the facade.
fn delegated(item: &str) -> TokenStream {
    let item = quote::format_ident!("{item}");
    quote!(::aosabi::remote::delegated::#item)
}

/// Returns the named field `__typename` of `fields`, the slot of the remote
/// object of a struct under `#[aosabi(remote)]`.
#[must_use]
pub fn typename_slot(fields: &Fields) -> Option<&Ident> {
    fields
        .iter()
        .filter_map(|field| field.ident.as_ref())
        .find(|ident| unraw(ident) == TYPENAME_KEY)
}

/// Returns `generics` with the bound `FromDelegated<T>` on each type `T` of
/// `types` when the item has type parameters.
///
/// A type with no parameter takes no bound, so a pulled type that reaches
/// itself through a field, a `User` with a field `Box<User>` among them,
/// compiles, since a bound on that field would make its implementation depend
/// on itself, which the trait solver refuses.
fn delegated_generics<'a>(generics: &Generics, types: impl Iterator<Item = &'a Type>) -> Generics {
    let mut generics = generics.clone();
    if generics.params.is_empty() {
        return generics;
    }
    let from_delegated = quote!(::aosabi::remote::FromDelegated);
    let clause: &mut WhereClause = generics.make_where_clause();
    for ty in types {
        clause
            .predicates
            .push(parse_quote!(#ty: #from_delegated<#ty>));
    }
    generics
}

/// Returns `impl FromDelegated<Self>` of the struct `item` under
/// `#[aosabi(remote)]` with the named `fields`, whose field `slot` keeps the
/// remote object.
///
/// Each other field reads the member of its name through `read_member`,
/// leniently: an absent member, a member that holds an error of the remote
/// and a member the field type cannot read each read as the default of the
/// type where it has one, so a record of a registry `aosabi pull` writes
/// stands wherever its value is an object, and the slot takes the value
/// last, after every field has read it.
pub(crate) fn struct_from_delegated(
    item: &TypeItem<'_>,
    fields: &Fields,
    slot: &Ident,
) -> TokenStream {
    let ident = item.ident;
    let name = unraw(ident);
    let members: Vec<(&Ident, &Type)> = fields
        .iter()
        .filter_map(|field| field.ident.as_ref().map(|member| (member, &field.ty)))
        .filter(|(member, _)| *member != slot)
        .collect();
    let generics = delegated_generics(item.generics, members.iter().map(|(_, ty)| *ty));
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
    let (read_member, require_object) = (delegated("read_member"), delegated("require_object"));
    let (value, thrown) = (glue("Value"), glue("Thrown"));
    let reads = members.iter().map(|(member, ty)| {
        let key = unraw(member);
        quote!(#member: #read_member::<#ty>(&value, #key)?,)
    });
    quote! {
        #[automatically_derived]
        impl #impl_generics ::aosabi::remote::FromDelegated<Self> for #ident #type_generics #where_clause {
            fn from_delegated(value: #value) -> ::core::result::Result<Self, #thrown> {
                #require_object(&value, #name)?;
                ::core::result::Result::Ok(Self {
                    #(#reads)*
                    #slot: ::aosabi::remote::TypeName::keep(value),
                })
            }
        }
    }
}

/// Returns `impl FromDelegated<Rec>` of the struct `item` with the named
/// `fields`, of which `spreads` stand under `#[aosabi(spread)]`, for the
/// type `Rec` of the first spread field in source order.
///
/// One implementation stands for the struct, never one per spread type. A
/// macro sees the spelling of a type and not the type, so two spread fields
/// of one type written two ways, `users::User` and
/// `crate::generated::users::User` or a type alias, would give two
/// implementations of one trait and fail with E0119. The plan names one
/// `FromDelegated<Rec>` for the spread's type `Rec`, and the record a patch
/// extends is taken to be the spread it writes first.
///
/// A public own field reads the member of its name strictly, a member the
/// value lacks throwing the `TypeError` of `read_own_member`, because the
/// patch that built the value always carries its own members. A spread field
/// reads the whole value through its type, which keeps the remote object, and
/// a private field, which is no member, takes its default.
///
/// The implementation holds for every struct with a spread and bounds no
/// field type. Each field picks its reader by method resolution over
/// references, as the reader of an input object does in the module `glue`,
/// so a field whose type cannot be read, a plain struct that keeps no
/// remote object among them, throws the `TypeError` of `unreadable_member`
/// when a delegated value reaches it, and a struct with a spread that is
/// never delegated compiles as it did.
pub(crate) fn spread_from_delegated(
    item: &TypeItem<'_>,
    fields: &Fields,
    spreads: &[Spread<'_>],
) -> TokenStream {
    let ident = item.ident;
    let name = unraw(ident);
    let require_object = delegated("require_object");
    let (value, thrown) = (glue("Value"), glue("Thrown"));
    let from_delegated = quote!(::aosabi::remote::FromDelegated);
    let is_spread = |member: &Ident| spreads.iter().any(|spread| spread.ident == member);
    let reads: Vec<TokenStream> = fields
        .iter()
        .filter_map(|field| {
            let member = field.ident.as_ref()?;
            let ty = &field.ty;
            let marker = quote!((&&::core::marker::PhantomData::<#ty>));
            let key = unraw(member);
            Some(if is_spread(member) {
                quote!(#member: #marker.aosabi_read_delegated(&value, ::core::option::Option::None, #key, #name)?,)
            } else if matches!(field.vis, syn::Visibility::Inherited) {
                quote!(#member: #marker.aosabi_default_delegated(#key, #name)?,)
            } else {
                quote!(#member: #marker.aosabi_read_delegated(&value, ::core::option::Option::Some(#key), #key, #name)?,)
            })
        })
        .collect();
    let readers = delegated_readers();
    let (impl_generics, type_generics, where_clause) = item.generics.split_for_impl();
    let first = fields
        .iter()
        .find(|field| field.ident.as_ref().is_some_and(is_spread));
    let impls = first.map(|field| {
        let ty = &field.ty;
        quote! {
            #[automatically_derived]
            impl #impl_generics #from_delegated<#ty> for #ident #type_generics #where_clause {
                fn from_delegated(value: #value) -> ::core::result::Result<Self, #thrown> {
                    #require_object(&value, #name)?;
                    ::core::result::Result::Ok(Self {
                        #(#reads)*
                    })
                }
            }
        }
    });
    quote! {
        const _: () = {
            #readers
            #impls
        };
    }
}

/// Returns the readers of a field of a struct with a spread, the one of a
/// type with `FromDelegated` and the one of a type with `Default`, each
/// beside a fallback that throws the `TypeError` of `unreadable_member`.
fn delegated_readers() -> TokenStream {
    let (value, thrown) = (glue("Value"), glue("Thrown"));
    let from_delegated = quote!(::aosabi::remote::FromDelegated);
    let (read_own, unreadable) = (delegated("read_own_member"), delegated("unreadable_member"));
    let result = quote!(::core::result::Result<T, #thrown>);
    let key = quote!(::core::option::Option<&'static str>);
    quote! {
        trait AosabiReadDelegated<T> {
            fn aosabi_read_delegated(&self, value: &#value, key: #key, member: &'static str, owner: &'static str) -> #result;
        }

        impl<T: #from_delegated<T>> AosabiReadDelegated<T> for &::core::marker::PhantomData<T> {
            fn aosabi_read_delegated(&self, value: &#value, key: #key, _: &'static str, _: &'static str) -> #result {
                match key {
                    ::core::option::Option::Some(key) => #read_own::<T>(value, key),
                    ::core::option::Option::None => <T as #from_delegated<T>>::from_delegated(::core::clone::Clone::clone(value)),
                }
            }
        }

        trait AosabiReadDelegatedFallback<T> {
            fn aosabi_read_delegated(&self, value: &#value, key: #key, member: &'static str, owner: &'static str) -> #result;
        }

        impl<T> AosabiReadDelegatedFallback<T> for ::core::marker::PhantomData<T> {
            fn aosabi_read_delegated(&self, _: &#value, _: #key, member: &'static str, owner: &'static str) -> #result {
                ::core::result::Result::Err(#unreadable(member, owner))
            }
        }

        trait AosabiDefaultDelegated<T> {
            fn aosabi_default_delegated(&self, member: &'static str, owner: &'static str) -> #result;
        }

        impl<T: ::core::default::Default> AosabiDefaultDelegated<T> for &::core::marker::PhantomData<T> {
            fn aosabi_default_delegated(&self, _: &'static str, _: &'static str) -> #result {
                ::core::result::Result::Ok(::core::default::Default::default())
            }
        }

        trait AosabiDefaultDelegatedFallback<T> {
            fn aosabi_default_delegated(&self, member: &'static str, owner: &'static str) -> #result;
        }

        impl<T> AosabiDefaultDelegatedFallback<T> for ::core::marker::PhantomData<T> {
            fn aosabi_default_delegated(&self, member: &'static str, owner: &'static str) -> #result {
                ::core::result::Result::Err(#unreadable(member, owner))
            }
        }
    }
}

/// Returns `impl FromDelegated<Self>` and `impl SameMember` of the enum
/// `item` under `#[aosabi(remote)]` whose variants each carry one payload,
/// which read the variant whose payload carries the `__typename` of the
/// value.
pub(crate) fn union_from_delegated(item: &TypeItem<'_>, variants: &[&Variant]) -> TokenStream {
    let ident = item.ident;
    let name = unraw(ident);
    let payloads: Vec<(&Ident, &Type)> = variants
        .iter()
        .filter_map(|variant| match &variant.fields {
            Fields::Unnamed(fields) if fields.unnamed.len() == 1 => fields
                .unnamed
                .first()
                .map(|field| (&variant.ident, &field.ty)),
            _ => None,
        })
        .collect();
    let generics = delegated_generics(item.generics, payloads.iter().map(|(_, ty)| *ty));
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
    let (typename_of, described_typename, unknown) = (
        delegated("typename_of"),
        delegated("described_typename"),
        delegated("unknown_typename"),
    );
    let (value, thrown) = (glue("Value"), glue("Thrown"));
    let from_delegated = quote!(::aosabi::remote::FromDelegated);
    let arms = payloads.iter().map(|(variant, ty)| {
        quote! {
            if typename.is_some() && typename.as_deref() == #described_typename::<#ty>() {
                return <#ty as #from_delegated<#ty>>::from_delegated(value).map(Self::#variant);
            }
        }
    });
    let same = payloads.iter().map(|(variant, ty)| {
        quote! {
            Self::#variant(payload) => (&&::core::marker::PhantomData::<#ty>).aosabi_same_member(payload, member),
        }
    });
    quote! {
        #[automatically_derived]
        impl #impl_generics #from_delegated<Self> for #ident #type_generics #where_clause {
            fn from_delegated(value: #value) -> ::core::result::Result<Self, #thrown> {
                let typename = #typename_of(&value);
                #(#arms)*
                ::core::result::Result::Err(#unknown(typename.as_deref(), #name))
            }
        }

        #[automatically_derived]
        impl #impl_generics ::aosabi::remote::delegated::SameMember for #ident #type_generics #where_clause {
            fn same_member(&self, member: &#value) -> bool {
                use ::aosabi::remote::delegated::{
                    SameMemberFallback as _,
                    SameMemberRead as _, //
                };
                match self {
                    #(#same)*
                }
            }
        }
    }
}
