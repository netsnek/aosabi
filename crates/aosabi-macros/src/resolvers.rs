// SPDX-License-Identifier: MIT
/*
 * aosabi-macros: `#[aosabi::resolvers]`.
 *
 * No analogue on the reference side.
 */

//! `#[aosabi::resolvers]`.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    Attribute,
    FnArg,
    Ident,
    ImplItem,
    ImplItemFn,
    Item,
    ItemImpl,
    Pat,
    PatType,
    Receiver,
    ReturnType,
    Signature,
    Type,
    spanned::Spanned, //
};

use crate::{
    attrs,
    docs::{
        self,
        Documentation, //
    },
    emit::{
        described_generics,
        unraw,
        visibility, //
    },
    glue,
    graphql,
    guards,
    links::type_link,
    lints,
    refuse::{
        METHOD_WITH_GENERICS,
        PARAMETER_NOT_IDENTIFIER,
        RECEIVER_NOT_REF_SELF,
        RESOLVERS_NOT_ON_IMPL,
        RESOLVERS_ON_TRAIT_IMPL,
        RESOLVERS_WITH_ARGUMENT,
        refusal,
        refuse, //
    }, //
};

/// Returns the `impl` block without its helper attributes, followed by the
/// method table, `impl Resolvers`, the statics and the length check.
///
/// `args` is the argument list of the attribute and `item` the item it stands
/// on. A refusal keeps the block in the output and still implements both
/// traits, with no method, so the one error a person reads is the refusal and
/// not a cascade about missing methods and an unsatisfied bound.
pub(crate) fn expand(args: &TokenStream, item: &TokenStream) -> TokenStream {
    let mut block = match syn::parse2(item.clone()) {
        Ok(Item::Impl(block)) => block,
        Ok(other) => {
            let refused = refuse(other.span(), RESOLVERS_NOT_ON_IMPL);
            return quote!(#item #refused);
        }
        Err(error) => {
            let refused = error.to_compile_error();
            return quote!(#item #refused);
        }
    };
    let recorded = record(args, &mut block).unwrap_or_else(|error| error.to_compile_error());
    quote!(#block #recorded)
}

/// Takes the helper attributes off `block` and returns what follows it.
///
/// # Errors
///
/// Returns the refusal of a lifetime and of a const parameter, the one refusal
/// after which no method table can be written. Every other refusal is part of
/// the returned tokens.
fn record(args: &TokenStream, block: &mut ItemImpl) -> syn::Result<TokenStream> {
    // The glue reads the default of a parameter and the decorators, which
    // live in helper attributes, so it reads the block before those are taken
    // off.
    let glue_methods = glue::methods(block);
    let glue_statics = glue::statics(block);
    let infos = method_infos(block);
    let generics = described_generics(&block.generics)?;
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let (infos, refused) = split(no_argument(args).and(inherent(block)).and(infos));
    let self_type = &block.self_ty;
    let (glue_methods, glue_statics) = if refused.is_empty() {
        (glue_methods, glue_statics)
    } else {
        (Vec::new(), Vec::new())
    };
    let table = glue::method_table(block, &glue_methods);
    let statics = glue::static_items(block, &generics, &glue_statics, &infos.statics);
    let check = length_check(block, infos.methods.len());
    let methods = &infos.methods;
    let allow = lints::allow();
    Ok(quote! {
        #allow
        impl #impl_generics #self_type #where_clause {
            #[doc(hidden)]
            pub const __AOSABI_METHODS: &'static [::aosabi::descriptor::MethodInfo] = &[#(#methods),*];
        }
        impl #impl_generics ::aosabi::descriptor::Resolvers for #self_type #where_clause {
            const METHODS: &'static [::aosabi::descriptor::MethodInfo] = <Self>::__AOSABI_METHODS;
        }
        #table
        #statics
        #check
        #refused
    })
}

/// The descriptors of a block.
#[derive(Default)]
struct Infos {
    /// The `MethodInfo` literal of every method with a receiver, in
    /// declaration order.
    methods: Vec<TokenStream>,
    /// The identifier and the `MethodInfo` literal of every public function
    /// with no receiver, in declaration order.
    statics: Vec<(Ident, TokenStream)>,
}

/// Returns the check that the derive reads the table of the block.
///
/// The derive reads `__AOSABI_METHODS` by path, and a bound of the block
/// that the derive does not state would make the path fall back to the empty
/// table of `ResolversFallback`. For a type with no parameters the check
/// compares the table the path reaches with the length of the block, so such
/// a block fails the build instead of describing a type without methods. A
/// generic block has no one type to check.
fn length_check(block: &ItemImpl, count: usize) -> TokenStream {
    if !block.generics.params.is_empty() {
        return TokenStream::new();
    }
    let self_type = &block.self_ty;
    quote! {
        const _: () = {
            #[allow(unused_imports, reason = "the check reads the table the derive reads")]
            use ::aosabi::__private::glue::ResolversFallback as _;
            ::core::assert!(<#self_type>::__AOSABI_METHODS.len() == #count);
        };
    }
}

/// Returns the descriptors beside no refusal, or none beside the refusal.
fn split(infos: syn::Result<Infos>) -> (Infos, TokenStream) {
    match infos {
        Ok(infos) => (infos, TokenStream::new()),
        Err(error) => (Infos::default(), error.to_compile_error()),
    }
}

/// Refuses an argument of the attribute.
fn no_argument(args: &TokenStream) -> syn::Result<()> {
    if args.is_empty() {
        return Ok(());
    }
    Err(refusal(args.span(), RESOLVERS_WITH_ARGUMENT))
}

/// Refuses a trait implementation.
fn inherent(block: &ItemImpl) -> syn::Result<()> {
    match &block.trait_ {
        None => Ok(()),
        Some((_, path, _)) => Err(refusal(path.span(), RESOLVERS_ON_TRAIT_IMPL)),
    }
}

/// Returns the `MethodInfo` literal of every resolver and of every public
/// static of `block`, in declaration order.
///
/// Every function of the block loses its helper attributes before the first
/// refusal returns, because the emitted block has no derive in scope that
/// would accept them.
fn method_infos(block: &mut ItemImpl) -> syn::Result<Infos> {
    let mut taken = Vec::new();
    for item in &mut block.items {
        if let ImplItem::Fn(method) = item {
            taken.push((take_attrs(method), &*method));
        }
    }
    let mut infos = Infos::default();
    for (helpers, method) in taken {
        if method.sig.receiver().is_some() {
            infos.methods.push(method_info(method, &helpers)?);
        } else if !matches!(method.vis, syn::Visibility::Inherited) {
            infos
                .statics
                .push((method.sig.ident.clone(), static_info(method, &helpers)?));
        }
    }
    Ok(infos)
}

/// The helper attributes [`take_attrs`] removed from one function.
struct Helpers {
    /// The attributes of the function itself.
    method: Vec<Attribute>,
    /// The decorators of the function, in source order.
    decorators: Vec<Attribute>,
    /// The attributes of each typed parameter, in declaration order.
    params: Vec<ParamHelpers>,
}

/// The attributes [`take_attrs`] removed from one parameter.
struct ParamHelpers {
    /// Every `#[aosabi(...)]`.
    keys: Vec<Attribute>,
    /// Every doc comment, which the compiler refuses on a parameter.
    docs: Vec<Attribute>,
}

/// Removes every `#[aosabi(...)]` and every `#[decorate(...)]` from `method`,
/// every `#[aosabi(...)]` from its parameters, and every doc comment from
/// its parameters.
fn take_attrs(method: &mut ImplItemFn) -> Helpers {
    let (decorators, kept) = core::mem::take(&mut method.attrs)
        .into_iter()
        .partition(|attr: &Attribute| attr.path().is_ident("decorate"));
    method.attrs = kept;
    let params = method
        .sig
        .inputs
        .iter_mut()
        .filter_map(|arg| match arg {
            FnArg::Typed(param) => Some(ParamHelpers {
                keys: attrs::take(&mut param.attrs),
                docs: docs::take(&mut param.attrs),
            }),
            FnArg::Receiver(_) => None,
        })
        .collect();
    Helpers {
        method: attrs::take(&mut method.attrs),
        decorators,
        params,
    }
}

/// Returns the `MethodInfo` literal of the method `method`, whose receiver
/// is `&self`.
fn method_info(method: &ImplItemFn, helpers: &Helpers) -> syn::Result<TokenStream> {
    if let Some(receiver) = method.sig.receiver() {
        check_signature(&method.sig, receiver)?;
    }
    function_info(method, helpers, false)
}

/// Returns the `MethodInfo` literal of the static `method`, a public
/// function with no receiver, which an entry of `graphql!` references.
///
/// The inference walks instance types only, so a static never becomes a
/// field of its type, and its descriptor stands on the associated item
/// `__AOSABI_STATIC_<name>` rather than in the method table.
fn static_info(method: &ImplItemFn, helpers: &Helpers) -> syn::Result<TokenStream> {
    if !method.sig.generics.params.is_empty() {
        return Err(refusal(method.sig.generics.span(), METHOD_WITH_GENERICS));
    }
    function_info(method, helpers, true)
}

/// Returns the `MethodInfo` literal of `method`, a method or a static.
fn function_info(
    method: &ImplItemFn,
    helpers: &Helpers,
    receiverless: bool,
) -> syn::Result<TokenStream> {
    attrs::method_attrs(&helpers.method)?;
    let name = unraw(&method.sig.ident);
    let documentation = docs::item(&method.attrs)?;
    let docs = documentation.to_tokens();
    let site = documentation.site_tokens(method.sig.ident.span());
    let visibility = visibility(&method.vis);
    let is_async = method.sig.asyncness.is_some();
    let params = param_infos(&method.sig, &helpers.params, &documentation)?;
    let returns = returns(&method.sig)?;
    let decorators = helpers
        .decorators
        .iter()
        .map(graphql::decorate)
        .collect::<syn::Result<Vec<_>>>()?;
    let facts = decorators.iter().map(|decorate| {
        let (text, line) = (&decorate.text, decorate.line);
        quote!(::aosabi::descriptor::DecoratorFact { text: #text, line: #line })
    });
    let guards = guards::facts(&method.block);
    Ok(quote! {
        ::aosabi::descriptor::MethodInfo {
            name: #name,
            docs: #docs,
            site: #site,
            visibility: #visibility,
            is_async: #is_async,
            params: &[#(#params),*],
            returns: #returns,
            receiverless: #receiverless,
            decorators: &[#(#facts),*],
            guards: #guards,
        }
    })
}

/// Refuses generic parameters and a receiver other than `&self`.
fn check_signature(sig: &Signature, receiver: &Receiver) -> syn::Result<()> {
    if !sig.generics.params.is_empty() {
        return Err(refusal(sig.generics.span(), METHOD_WITH_GENERICS));
    }
    let is_ref_self = receiver.colon_token.is_none()
        && receiver.mutability.is_none()
        && matches!(&receiver.reference, Some((_, None)));
    if !is_ref_self {
        return Err(refusal(receiver.span(), RECEIVER_NOT_REF_SELF));
    }
    Ok(())
}

/// Returns the `ParamInfo` literal of every typed parameter of `sig`.
///
/// `helpers` holds the attributes taken off each typed parameter, in the
/// same order, and `method` is the documentation of the method.
fn param_infos(
    sig: &Signature,
    helpers: &[ParamHelpers],
    method: &Documentation,
) -> syn::Result<Vec<TokenStream>> {
    sig.inputs
        .iter()
        .filter_map(|arg| match arg {
            FnArg::Typed(param) => Some(param),
            FnArg::Receiver(_) => None,
        })
        .zip(helpers)
        .map(|(param, helpers)| param_info(param, helpers, method))
        .collect()
}

/// Returns the `ParamInfo` literal of `param`, whose documentation composes
/// its own comment with the `@param` tags of `method` that name it.
fn param_info(
    param: &PatType,
    helpers: &ParamHelpers,
    method: &Documentation,
) -> syn::Result<TokenStream> {
    let Pat::Ident(pattern) = &*param.pat else {
        return Err(refusal(param.pat.span(), PARAMETER_NOT_IDENTIFIER));
    };
    if pattern.by_ref.is_some() || pattern.subpat.is_some() {
        return Err(refusal(param.pat.span(), PARAMETER_NOT_IDENTIFIER));
    }
    param_info_of(
        &pattern.ident,
        &param.ty,
        &helpers.keys,
        &helpers.docs,
        method,
    )
}

/// Returns the `ParamInfo` literal of the parameter `ident` of type `ty`,
/// whose `#[aosabi(...)]` keys are `keys` and whose doc comments are `docs`,
/// of a function whose documentation is `method`.
///
/// # Errors
///
/// Returns the refusals of the type, of the keys and of the doc comments.
pub(crate) fn param_info_of(
    ident: &Ident,
    ty: &Type,
    keys: &[Attribute],
    docs: &[Attribute],
    method: &Documentation,
) -> syn::Result<TokenStream> {
    let name = unraw(ident);
    let link = type_link(ty)?;
    let optional = attrs::param_attrs(keys)?.default;
    let documentation = docs::parameter(docs, &name, method)?;
    let docs = documentation.to_tokens();
    let site = documentation.site_tokens(ident.span());
    Ok(quote! {
        ::aosabi::descriptor::ParamInfo {
            name: #name,
            r#type: #link,
            optional: #optional,
            docs: #docs,
            site: #site,
        }
    })
}

/// Returns the link to the return type of `sig`.
///
/// A missing return type links `()`, and an `async fn` links its return
/// through `Promise`, because its TypeScript twin returns a `Promise<T>` that
/// the inference unwraps.
fn returns(sig: &Signature) -> syn::Result<TokenStream> {
    let ty = match &sig.output {
        ReturnType::Default => syn::parse_quote!(()),
        ReturnType::Type(_, ty) => (**ty).clone(),
    };
    returns_of(&ty, sig.asyncness.is_some())
}

/// Returns the link to the return type `ty` of a function, through `Promise`
/// when the function is `async`.
///
/// # Errors
///
/// Returns the refusal of the type.
pub(crate) fn returns_of(ty: &Type, is_async: bool) -> syn::Result<TokenStream> {
    if !is_async {
        return type_link(ty);
    }
    type_link(ty)?;
    Ok(quote!(::aosabi::descriptor::TypeLink::of::<::aosabi::descriptor::Promise<#ty>>()))
}
