// SPDX-License-Identifier: MIT
/*
 * aosabi-macros: #[aosabi::resolver] on a free function.
 *
 * The attribute is the twin of a function a TypeScript service names in its
 * object literal of roots, which the TypeScript checker reads by its
 * signature.
 */

//! `#[aosabi::resolver]`.
//!
//! The attribute emits the function without its helper attributes and,
//! beside it, a hidden module of the same name whose one public item is
//! `__AOSABI_RESOLVER`, the `EntryFunction` of the function, which a
//! reference or a shorthand entry of `graphql!` reaches. A `use` of the
//! function imports the module too, because a `use` imports a name in every
//! namespace it has.
//!
//! The attribute stands above every `#[decorate(...)]` of the function,
//! because an attribute macro expands before the attributes below it, and it
//! takes them off in source order.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    Attribute,
    FnArg,
    Item,
    ItemFn,
    Pat,
    spanned::Spanned, //
};

use crate::{
    attrs,
    docs,
    emit::{
        unraw,
        visibility, //
    },
    glue::{
        self,
        Param, //
    },
    graphql::{
        self,
        path_rule::RESOLVER_ITEM, //
    },
    guards,
    lints,
    refuse::{
        METHOD_WITH_GENERICS,
        PARAMETER_NOT_IDENTIFIER,
        RESOLVER_NOT_ON_FN,
        RESOLVER_WITH_ARGUMENT,
        refusal, //
    },
    resolvers::{
        param_info_of,
        returns_of, //
    }, //
};

/// Expands one use of `#[aosabi::resolver]`.
///
/// A refusal keeps the item in the output, so the one error a person reads
/// is the refusal.
pub(crate) fn expand(args: &TokenStream, item: &TokenStream) -> TokenStream {
    let mut function = match syn::parse2(item.clone()) {
        Ok(Item::Fn(function)) => function,
        Ok(other) => {
            let refused = refusal(other.span(), RESOLVER_NOT_ON_FN).to_compile_error();
            return quote!(#item #refused);
        }
        Err(error) => {
            let refused = error.to_compile_error();
            return quote!(#item #refused);
        }
    };
    let helpers = take_helpers(&mut function);
    let module = if args.is_empty() {
        module(&function, &helpers)
    } else {
        Err(refusal(args.span(), RESOLVER_WITH_ARGUMENT))
    };
    let module = module.unwrap_or_else(|error| error.to_compile_error());
    quote!(#function #module)
}

/// The helper attributes taken off one function.
struct Helpers {
    /// The `#[aosabi(...)]` keys of the function.
    keys: Vec<Attribute>,
    /// The decorators in source order.
    decorators: Vec<Attribute>,
    /// The `#[aosabi(...)]` keys and the doc comments of each typed
    /// parameter.
    params: Vec<(Vec<Attribute>, Vec<Attribute>)>,
}

/// Takes the helper attributes off `function` and its parameters.
fn take_helpers(function: &mut ItemFn) -> Helpers {
    let (decorators, kept) = core::mem::take(&mut function.attrs)
        .into_iter()
        .partition(|attr: &Attribute| attr.path().is_ident("decorate"));
    function.attrs = kept;
    let keys = attrs::take(&mut function.attrs);
    let params = function
        .sig
        .inputs
        .iter_mut()
        .filter_map(|arg| match arg {
            FnArg::Typed(param) => {
                Some((attrs::take(&mut param.attrs), docs::take(&mut param.attrs)))
            }
            FnArg::Receiver(_) => None,
        })
        .collect();
    Helpers {
        keys,
        decorators,
        params,
    }
}

/// Returns the hidden module of `function`.
fn module(function: &ItemFn, helpers: &Helpers) -> syn::Result<TokenStream> {
    attrs::method_attrs(&helpers.keys)?;
    let sig = &function.sig;
    if !sig.generics.params.is_empty() {
        return Err(refusal(sig.generics.span(), METHOD_WITH_GENERICS));
    }
    if let Some(receiver) = sig.receiver() {
        return Err(refusal(receiver.span(), RESOLVER_NOT_ON_FN));
    }
    let documentation = docs::item(&function.attrs)?;
    let mut infos = Vec::new();
    let mut params = Vec::new();
    let typed = sig.inputs.iter().filter_map(|arg| match arg {
        FnArg::Typed(param) => Some(param),
        FnArg::Receiver(_) => None,
    });
    for (param, (keys, docs)) in typed.zip(&helpers.params) {
        let Pat::Ident(pattern) = &*param.pat else {
            return Err(refusal(param.pat.span(), PARAMETER_NOT_IDENTIFIER));
        };
        if pattern.by_ref.is_some() || pattern.subpat.is_some() {
            return Err(refusal(param.pat.span(), PARAMETER_NOT_IDENTIFIER));
        }
        infos.push(param_info_of(
            &pattern.ident,
            &param.ty,
            keys,
            docs,
            &documentation,
        )?);
        params.push(Param {
            name: unraw(&pattern.ident),
            ty: Some((*param.ty).clone()),
            default: glue::default_of(keys),
        });
    }
    let output = match &sig.output {
        syn::ReturnType::Default => syn::parse_quote!(()),
        syn::ReturnType::Type(_, ty) => (**ty).clone(),
    };
    let is_async = sig.asyncness.is_some();
    let returns = returns_of(&output, is_async)?;
    let decorators = helpers
        .decorators
        .iter()
        .map(graphql::decorate)
        .collect::<syn::Result<Vec<_>>>()?;
    let facts = decorators.iter().map(|decorate| {
        let (text, line) = (&decorate.text, decorate.line);
        quote!(::aosabi::descriptor::DecoratorFact { text: #text, line: #line })
    });
    let exprs: Vec<&syn::Expr> = decorators.iter().map(|decorate| &decorate.expr).collect();
    let guards = guards::facts(&function.block);
    let ident = &sig.ident;
    let call = glue::call_body(&quote!(super::#ident), &params, is_async);
    let body = glue::decorated_body(&exprs, &call);
    let run = syn::Ident::new("__aosabi_run", ident.span());
    let run_fn = glue::run_fn(&TokenStream::new(), &run, &body);
    let name = unraw(ident);
    let docs = documentation.to_tokens();
    let site = documentation.site_tokens(ident.span());
    let vis = &function.vis;
    let described_vis = visibility(vis);
    let item = syn::Ident::new(RESOLVER_ITEM, ident.span());
    let allow = lints::allow();
    Ok(quote! {
        #[doc(hidden)]
        #allow
        #[allow(dead_code, reason = "a function no entry references leaves its descriptor unread")]
        #vis mod #ident {
            #[allow(unused_imports, reason = "the descriptor reads the names of the module of the function")]
            use super::*;

            #run_fn

            /// The function of the resolver and its descriptor.
            pub const #item: ::aosabi::__private::glue::EntryFunction =
                ::aosabi::__private::glue::EntryFunction {
                    info: &::aosabi::descriptor::MethodInfo {
                        name: #name,
                        docs: #docs,
                        site: #site,
                        visibility: #described_vis,
                        is_async: #is_async,
                        params: &[#(#infos),*],
                        returns: #returns,
                        receiverless: true,
                        decorators: &[#(#facts),*],
                        guards: #guards,
                    },
                    run: #run,
                };
        }
    })
}
