// SPDX-License-Identifier: MIT
/*
 * aosabi-codegen: the glue of the key `#[aosabi(spread)]` of a field that
 * stands for an object spread.
 *
 * No analogue on the reference side, where the spread is the syntax {..value}
 * of an object literal, whose members the TypeScript checker presents in
 * place of the spread.
 */

//! The glue of the key `#[aosabi(spread)]`, which presents the members of
//! the field in place of the field.
//!
//! A struct with a field under the key is the twin of an object literal with
//! a spread, `{...u, fullName, org}`, its fields in the order of the literal
//! and the spread field at the place the spread stands. The descriptor
//! records the field with `spread` set, and the inference presents the
//! members of the field's type in its place by the spread order of the
//! checker.
//!
//! The object of the struct answers what the object of the literal answers.
//! Its keys are the typename, then the keys of each segment in source order,
//! a key that stands twice at the place of its first occurrence, then the
//! methods of its block, which is the order of the own keys of an object
//! literal. A key reads the typename, then the public fields after the last
//! spread, then the methods, then the segments before them in reverse source
//! order, so a name in two segments reads the value of its later occurrence,
//! as the literal keeps it. A spread reads a key through its own object, and
//! a key it does not answer, which reads `undefined` there, falls to the
//! segment before it. The spread field itself is no key.
//!
//! A delegation hands the struct back from the object its patch made, so the
//! struct implements `FromDelegated<Rec>` for the type `Rec` of its first
//! spread field, which the module `remote` writes: an own field reads its
//! member strictly and the spread reads the whole object through `Rec`, which
//! keeps it. One implementation stands for the struct, so two spread fields
//! of one type written two ways, a path and a type alias among them, do not
//! give two implementations of one trait.
//!
//! [copy]: https://tc39.es/ecma262/#sec-copydataproperties
//! [keys]: https://tc39.es/ecma262/#sec-ordinaryownpropertykeys

use proc_macro2::{
    Span,
    TokenStream, //
};
use quote::quote;
use syn::{
    Fields,
    Generics,
    Ident,
    Visibility, //
};

use crate::{
    glue::{
        TYPENAME_KEY,
        field_value_impl,
        glue, //
    },
    unraw, //
};

/// One field under `#[aosabi(spread)]`, which the derive reads from the
/// attributes of the field.
pub struct Spread<'a> {
    /// The identifier of the field.
    pub ident: &'a Ident,
    /// The span of the key, where a refusal of the spread points.
    pub span: Span,
}

/// One segment of the fields of a struct with a spread, in source order.
enum Segment<'a> {
    /// A run of public fields the struct declares.
    Own(Vec<&'a Ident>),
    /// A field under `#[aosabi(spread)]`.
    Spread(&'a Ident),
}

/// Returns the segments of the named `fields`, the runs of public fields and
/// the fields of `spreads`, in source order.
///
/// A private field is no key, so it stands in no run.
fn segments<'a>(fields: &'a Fields, spreads: &[Spread<'_>]) -> Vec<Segment<'a>> {
    let mut segments = Vec::new();
    for field in fields {
        let Some(ident) = &field.ident else {
            continue;
        };
        if spreads.iter().any(|spread| spread.ident == ident) {
            segments.push(Segment::Spread(ident));
        } else if !matches!(field.vis, Visibility::Inherited) {
            match segments.last_mut() {
                Some(Segment::Own(run)) => run.push(ident),
                _ => segments.push(Segment::Own(vec![ident])),
            }
        }
    }
    segments
}

/// Returns `impl ObjectLike`, `impl SharedObject` and `impl FieldValue` of
/// the struct `target` with the named `fields`, of which `spreads` stand
/// under `#[aosabi(spread)]`, with the typename `typename`.
///
/// The generics carry the bounds of the object glue. The spread type of each
/// spread field implements `SharedObject`, which every described struct
/// does, and a spread type that does not is refused by the compiler with
/// the message of that trait.
pub(crate) fn object_impls(
    target: &TokenStream,
    generics: &Generics,
    typename: Option<&str>,
    fields: &Fields,
    spreads: &[Spread<'_>],
) -> TokenStream {
    let segments = segments(fields, spreads);
    let object_like = object_like_impl(target, generics, typename.is_some(), &segments);
    let shared_object = shared_object_impl(target, generics, typename, &segments);
    let field_value = field_value_impl(target, generics, None);
    quote!(#object_like #shared_object #field_value)
}

/// Returns `impl ObjectLike` of the struct, whose keys are those of the
/// object literal it is the twin of.
fn object_like_impl(
    target: &TokenStream,
    generics: &Generics,
    has_typename: bool,
    segments: &[Segment<'_>],
) -> TokenStream {
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let (object_like, shared_object, shared) =
        (glue("ObjectLike"), glue("SharedObject"), glue("Shared"));
    let (value, rc, methods) = (glue("Value"), glue("Rc"), glue("ResolverMethods"));
    let member_keys = quote!(::aosabi::__private::glue::embedded::member_keys);
    let typename = has_typename.then(|| quote!(add(&mut keys, #TYPENAME_KEY);));
    let steps = segments.iter().map(|segment| match segment {
        Segment::Own(run) => {
            let names = run.iter().map(|field| unraw(field));
            quote!(#(add(&mut keys, #names);)*)
        }
        Segment::Spread(field) => quote!(#member_keys(&self.#field, &mut keys);),
    });
    quote! {
        #[automatically_derived]
        impl #impl_generics #object_like for #target #where_clause {
            fn get(self: #rc<Self>, key: &str) -> #value {
                <Self as #shared_object>::get_shared(&#shared::from(self), key)
            }

            fn own_keys(&self) -> ::std::vec::Vec<::std::string::String> {
                fn add(keys: &mut ::std::vec::Vec<::std::string::String>, key: &str) {
                    if !keys.iter().any(|known| known == key) {
                        keys.push(::std::string::String::from(key));
                    }
                }
                let mut keys = ::std::vec::Vec::new();
                #typename
                #(#steps)*
                for name in <Self as #methods>::NAMES {
                    add(&mut keys, name);
                }
                keys
            }
        }
    }
}

/// Returns `impl SharedObject` of the struct, whose property of a key is the
/// one of its later occurrence in the object literal.
///
/// The segments split at the last spread. The typename and the public fields
/// after it come first, then the methods, then the last spread and the
/// segments before it in reverse source order.
fn shared_object_impl(
    target: &TokenStream,
    generics: &Generics,
    typename: Option<&str>,
    segments: &[Segment<'_>],
) -> TokenStream {
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let (shared_object, shared, value) = (glue("SharedObject"), glue("Shared"), glue("Value"));
    let (into_value, methods) = (glue("IntoValue"), glue("ResolverMethods"));
    let last_spread = segments
        .iter()
        .rposition(|segment| matches!(segment, Segment::Spread(_)))
        .unwrap_or_default();
    let (before, from_last) = segments.split_at(last_spread);
    let (last, tail) = from_last.split_at(from_last.len().min(1));
    let typename_arm =
        typename.map(|name| quote!(#TYPENAME_KEY => return #into_value::into_value(#name),));
    let tail_arms: Vec<TokenStream> = tail
        .iter()
        .flat_map(|segment| match segment {
            Segment::Own(run) => run.clone(),
            Segment::Spread(_) => Vec::new(),
        })
        .map(own_arm)
        .collect();
    let head = (typename_arm.is_some() || !tail_arms.is_empty()).then(|| {
        quote! {
            match key {
                #typename_arm
                #(#tail_arms)*
                _ => {}
            }
        }
    });
    let steps = last.iter().chain(before.iter().rev()).map(step);
    quote! {
        #[automatically_derived]
        impl #impl_generics #shared_object for #target #where_clause {
            fn get_shared(this: &#shared<Self>, key: &str) -> #value {
                let ::core::option::Option::Some(object) = this.reach() else {
                    return #value::Undefined;
                };
                #head
                if let ::core::option::Option::Some(method) = <Self as #methods>::method(this, key) {
                    return #value::Function(method);
                }
                #(#steps)*
                #value::Undefined
            }
        }
    }
}

/// Returns the step of the lookup through one segment, which returns the
/// property the segment answers and lets a key it does not answer fall to
/// the step after it.
fn step(segment: &Segment<'_>) -> TokenStream {
    match segment {
        Segment::Spread(field) => spread_step(field),
        Segment::Own(run) => {
            let arms = run.iter().copied().map(own_arm);
            quote! {
                match key {
                    #(#arms)*
                    _ => {}
                }
            }
        }
    }
}

/// Returns the arm that answers the public field `field` of the struct
/// `object` reaches, read through a shared reference.
fn own_arm(field: &Ident) -> TokenStream {
    let field_value = glue("FieldValue");
    let name = unraw(field);
    quote! {
        #name => return #field_value::field_value(
            &object.#field,
            &|| this.part(|object: &Self| ::core::option::Option::Some(&object.#field)),
        ),
    }
}

/// Returns the step that answers a key through the spread field `field`,
/// and lets a key it reads as `undefined` fall to the step after it.
fn spread_step(field: &Ident) -> TokenStream {
    let value = glue("Value");
    let member = quote!(::aosabi::__private::glue::embedded::member);
    quote! {
        let found = #member(this, |object: &Self| &object.#field, key);
        if !::core::matches!(found, #value::Undefined) {
            return found;
        }
    }
}
