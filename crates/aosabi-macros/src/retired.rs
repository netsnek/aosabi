// SPDX-License-Identifier: MIT
/*
 * aosabi-macros: the answers of the macros under the default features, where
 * every form of the derive path is retired for the native layer.
 *
 * No analogue on the reference side.
 */

//! The answers of the macros under the default features.
//!
//! Without the feature `mirror`, which the facade turns on for the twins that
//! wait for the TypeScript judge alone, every macro of this crate answers a
//! compile error that names the native form of what it wrote, the plain Rust
//! the builder of `aosabi-build` reads from the source with no attribute,
//! derive or macro of Aosabi. A derive or an attribute answers one error per
//! key of `#[aosabi(..)]` its item carries first, each with the native form of
//! its key, and the error of its own form after them. An attribute keeps its
//! item with the helper attributes it would take off removed, a key of
//! `aosabi(..)`, a decorator and the doc comment of a parameter, so its errors
//! are the errors of the site.

use proc_macro2::{
    Span,
    TokenStream,
    TokenTree, //
};
use quote::ToTokens;
use syn::{
    Attribute,
    FnArg,
    ImplItem,
    Item,
    Meta,
    spanned::Spanned, //
};

use crate::refuse::{
    RETIRED_KEY_ANONYMOUS,
    RETIRED_KEY_DEFAULT,
    RETIRED_KEY_ENUM,
    RETIRED_KEY_EXTENDS,
    RETIRED_KEY_IMPLEMENTS,
    RETIRED_KEY_OTHER,
    RETIRED_KEY_REGISTRY,
    RETIRED_KEY_SPREAD,
    RETIRED_KEY_TYPENAME,
    RETIRED_KEY_VALUE,
    refuse, //
};

/// Returns the message of the key `key` of `#[aosabi(..)]`, the native form
/// of what it states.
fn key_message(key: &str) -> &'static str {
    match key {
        "typename" => RETIRED_KEY_TYPENAME,
        "anonymous" => RETIRED_KEY_ANONYMOUS,
        "enum" => RETIRED_KEY_ENUM,
        "implements" => RETIRED_KEY_IMPLEMENTS,
        "extends" => RETIRED_KEY_EXTENDS,
        "spread" => RETIRED_KEY_SPREAD,
        "value" => RETIRED_KEY_VALUE,
        "default" => RETIRED_KEY_DEFAULT,
        "remote" | "arguments" | "select" => RETIRED_KEY_REGISTRY,
        _ => RETIRED_KEY_OTHER,
    }
}

/// Returns whether `attr` is the helper attribute `#[aosabi(..)]`.
fn is_key(attr: &Attribute) -> bool {
    attr.path().is_ident("aosabi")
}

/// Returns whether `attr` is a decorator, `#[decorate(..)]` or
/// `#[aosabi::decorate(..)]`.
fn is_decorator(attr: &Attribute) -> bool {
    attr.path()
        .segments
        .last()
        .is_some_and(|segment| segment.ident == "decorate")
}

/// Returns one refusal per key of every `#[aosabi(..)]` among `attrs`, each at
/// its key.
///
/// A key is the first token of each part of the list that a comma at its top
/// level ends, so a key keeps its place whatever value it carries. An
/// attribute that carries no list is refused at its path.
fn key_refusals(attrs: &[Attribute]) -> TokenStream {
    let mut out = TokenStream::new();
    for attr in attrs.iter().filter(|attr| is_key(attr)) {
        let Meta::List(list) = &attr.meta else {
            out.extend(refuse(attr.path().span(), RETIRED_KEY_OTHER));
            continue;
        };
        let mut at_key = true;
        for token in list.tokens.clone() {
            match &token {
                TokenTree::Punct(punct) if punct.as_char() == ',' => at_key = true,
                TokenTree::Ident(ident) if at_key => {
                    out.extend(refuse(ident.span(), key_message(&ident.to_string())));
                    at_key = false;
                }
                _ => at_key = false,
            }
        }
    }
    out
}

/// Answers a derive of the derive path under the default features: one
/// refusal per key of `#[aosabi(..)]` on the type, its fields and its
/// variants, then the refusal `form` of the derive.
pub(crate) fn derive(input: &TokenStream, form: &str) -> TokenStream {
    let mut out = TokenStream::new();
    if let Ok(input) = syn::parse2::<syn::DeriveInput>(input.clone()) {
        out.extend(key_refusals(&input.attrs));
        match &input.data {
            syn::Data::Struct(data) => {
                for field in &data.fields {
                    out.extend(key_refusals(&field.attrs));
                }
            }
            syn::Data::Enum(data) => {
                for variant in &data.variants {
                    out.extend(key_refusals(&variant.attrs));
                    for field in &variant.fields {
                        out.extend(key_refusals(&field.attrs));
                    }
                }
            }
            syn::Data::Union(data) => {
                for field in &data.fields.named {
                    out.extend(key_refusals(&field.attrs));
                }
            }
        }
    }
    out.extend(refuse(Span::call_site(), form));
    out
}

/// Answers an attribute of the derive path under the default features: one
/// refusal per key of `#[aosabi(..)]` in the item, the refusal `form` of the
/// attribute, and the item without the helper attributes the attribute
/// would take off.
///
/// The helpers are a key on a method or a parameter of an `impl` block, a
/// decorator on a method or on a free function, and the doc comment of a
/// parameter, which rustc refuses where no macro takes it off. An item that
/// does not parse follows the refusal unchanged.
pub(crate) fn attribute(item: &TokenStream, form: &str) -> TokenStream {
    let Ok(mut parsed) = syn::parse2::<Item>(item.clone()) else {
        let mut out = refuse(Span::call_site(), form);
        out.extend(item.clone());
        return out;
    };
    let mut out = TokenStream::new();
    match &mut parsed {
        Item::Impl(block) => {
            for member in &mut block.items {
                let ImplItem::Fn(method) = member else {
                    continue;
                };
                out.extend(key_refusals(&method.attrs));
                method
                    .attrs
                    .retain(|attr| !is_key(attr) && !is_decorator(attr));
                for input in &mut method.sig.inputs {
                    if let FnArg::Typed(parameter) = input {
                        out.extend(key_refusals(&parameter.attrs));
                        parameter
                            .attrs
                            .retain(|attr| !is_key(attr) && !attr.path().is_ident("doc"));
                    }
                }
            }
        }
        Item::Fn(function) => {
            function.attrs.retain(|attr| !is_decorator(attr));
            for input in &mut function.sig.inputs {
                if let FnArg::Typed(parameter) = input {
                    out.extend(key_refusals(&parameter.attrs));
                    parameter
                        .attrs
                        .retain(|attr| !is_key(attr) && !attr.path().is_ident("doc"));
                }
            }
        }
        _ => {}
    }
    out.extend(refuse(Span::call_site(), form));
    out.extend(parsed.into_token_stream());
    out
}

/// Answers a function-like macro of the derive path under the default
/// features, the refusal `form` at the call.
pub(crate) fn call(form: &str) -> TokenStream {
    refuse(Span::call_site(), form)
}
