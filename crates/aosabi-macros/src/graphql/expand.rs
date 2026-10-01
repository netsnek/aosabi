// SPDX-License-Identifier: MIT
/*
 * aosabi-macros: the expansion of graphql! into its hidden module and its
 * constant.
 *
 * No analogue on the reference side.
 */

//! The expansion of `graphql!`.
//!
//! `pub const graphql = {...}` expands to a hidden module
//! `__aosabi_fragment_graphql` and the constant `graphql` of its unit type
//! `AosabiFragment`, whose type the build reads. The module holds:
//!
//! | Item | What it is |
//! | --- | --- |
//! | `Query__<name>` | the function of an inline entry, with the closure's parameters, return type and body |
//! | `__aosabi_run_<Root>_<i>` | the function of the executor of entry `i`, which reads the arguments, runs the decorators and the mutation wrapper, and calls the function the entry reaches |
//! | `__aosabi_info_<Root>_<i>` | the descriptor of the function a reference or a shorthand reaches |
//! | `__aosabi_spread_<Root>_<i>` | the root of the fragment spread at item `i` |
//! | `QUERY`, `MUTATION`, `SUBSCRIPTION` | the `FragmentRoot` of each root, an unstated one for a root the literal leaves out |
//! | `__aosabi_object_<Root>` | the object of the root at run time, built in source order |
//! | `__AOSABI_DECLARATION` | the declaration the declaration list names, when the build script of the crate writes one |
//! | `AosabiFragment` | `Roots` and `glue::Fragment` |
//!
//! The module imports every name of its parent, so an inline entry reads the
//! names of the module the invocation stands in.

use proc_macro2::TokenStream;
use quote::{
    format_ident,
    quote, //
};
use syn::{
    Attribute,
    Expr,
    Ident,
    Visibility,
    spanned::Spanned, //
};

use super::{
    parse::{
        Entry,
        Inline,
        Input,
        Item,
        Root,
        RootKey,
        RootValue,
        Spread,
        Value, //
    },
    path_rule,
};
use crate::{
    docs::{
        self,
        Documentation, //
    },
    emit::{
        source_key,
        unraw, //
    },
    glue::{
        self,
        Param, //
    },
    guards,
    lints,
    list,
    refuse::{
        NO_LIST_FOR_TARGET,
        refusal, //
    },
    resolvers::{
        param_info_of,
        returns_of, //
    }, //
};

/// The name of the unit type of a fragment in its hidden module.
const FRAGMENT_TYPE: &str = "AosabiFragment";

/// Returns the expansion of `input`.
///
/// # Errors
///
/// Returns the refusal of an entry, of a parameter and of a target whose
/// declaration list the build script of the crate did not write.
pub(crate) fn expand(input: &Input) -> syn::Result<TokenStream> {
    let module = path_rule::fragment_module(&input.name);
    let fragment = unraw(&input.name);
    let list =
        list::path().map_err(|list::Missing| refusal(input.name.span(), NO_LIST_FOR_TARGET))?;
    let source = source_key();

    let mut items = Vec::new();
    let mut statics = Vec::new();
    let mut root_infos = Vec::new();
    let mut factories = Vec::new();
    for key in RootKey::ALL {
        let root = input.roots.iter().find(|root| root.key == key);
        let expanded = expand_root(key, root, &fragment, &source)?;
        items.push(expanded.items);
        statics.push(expanded.fragment_root);
        root_infos.push(expanded.root_info);
        factories.push(expanded.factory);
    }

    let declaration = list.as_ref().map(|_| {
        quote! {
            #[doc(hidden)]
            pub(crate) const __AOSABI_DECLARATION: ::aosabi::descriptor::Declaration =
                ::aosabi::descriptor::Declaration {
                    krate: ::core::env!("CARGO_PKG_NAME"),
                    source: #source,
                    name: #fragment,
                    link: ::core::option::Option::None,
                    heritage: &[],
                };
        }
    });
    let declarations = list.map_or_else(|| quote!(&[]), |list| quote!(::core::include!(#list)));

    let fragment_type = Ident::new(FRAGMENT_TYPE, input.name.span());
    let [query, mutation, subscription] = [&root_infos[0], &root_infos[1], &root_infos[2]];
    let [query_factory, mutation_factory, subscription_factory] =
        [&factories[0], &factories[1], &factories[2]];
    let (glue_fragment, resolvers) = (
        quote!(::aosabi::__private::glue::Fragment),
        quote!(::aosabi::__private::glue::Resolvers),
    );
    let module_vis = match &input.vis {
        Visibility::Public(_) => quote!(pub),
        Visibility::Restricted(_) | Visibility::Inherited => quote!(pub(crate)),
    };
    let (docs, others): (Vec<&Attribute>, Vec<&Attribute>) = input
        .attrs
        .iter()
        .partition(|attr| attr.path().is_ident("doc"));
    let allow = lints::allow();
    let (vis, name) = (&input.vis, &input.name);
    Ok(quote! {
        #[doc(hidden)]
        #allow
        #[allow(dead_code, reason = "a root the literal leaves out is read only through a spread")]
        #(#others)*
        #module_vis mod #module {
            #[allow(unused_imports, reason = "an entry reads the names of the module of the literal")]
            use super::*;

            #(#items)*
            #(#statics)*
            #declaration

            /// The type of the constant, which carries the roots.
            #[derive(Clone, Copy, Debug)]
            pub struct #fragment_type;

            impl ::aosabi::descriptor::Roots for #fragment_type {
                const ROOTS: ::aosabi::descriptor::RootsInfo = ::aosabi::descriptor::RootsInfo {
                    query: #query,
                    mutation: #mutation,
                    subscription: #subscription,
                };
                const DECLARATIONS: &'static [::aosabi::descriptor::Declaration] = #declarations;
            }

            impl #glue_fragment for #fragment_type {
                type Roots = Self;

                fn resolvers(self) -> #resolvers {
                    #resolvers {
                        query: #query_factory,
                        mutation: #mutation_factory,
                        subscription: #subscription_factory,
                    }
                }
            }
        }

        #(#docs)*
        #(#others)*
        #allow
        #vis const #name: #module::#fragment_type = #module::#fragment_type;
    })
}

/// The tokens one root adds to the hidden module.
struct ExpandedRoot {
    /// The functions of the entries, the spreads and the object.
    items: TokenStream,
    /// The static `FragmentRoot` of the root.
    fragment_root: TokenStream,
    /// The `RootInfo` expression of the root.
    root_info: TokenStream,
    /// The `Option<RootFactory>` expression of the root.
    factory: TokenStream,
}

/// Returns the tokens of the root `key`, which the literal states as `root`
/// or leaves out.
fn expand_root(
    key: RootKey,
    root: Option<&Root>,
    fragment: &str,
    source: &TokenStream,
) -> syn::Result<ExpandedRoot> {
    let name = key.name();
    let upper = format_ident!("{}", key.upper());
    let object = object_fn(key);
    let (fragment_root_type, value, object_like) = (
        quote!(::aosabi::descriptor::FragmentRoot),
        glue_path("Value"),
        glue_path("IntoValue"),
    );
    let (root_object, factory) = (glue_path("root_object"), glue_path("factory"));
    let unstated = quote! {
        #[doc(hidden)]
        pub static #upper: #fragment_root_type = #fragment_root_type::unstated(
            #name,
            #fragment,
            ::core::env!("CARGO_PKG_NAME"),
            #source,
        );
    };
    let Some(root) = root else {
        return Ok(ExpandedRoot {
            items: quote! {
                #[doc(hidden)]
                pub fn #object() -> #value {
                    #root_object(&[])
                }
            },
            fragment_root: unstated,
            root_info: quote!(::aosabi::descriptor::RootInfo::Absent),
            factory: quote!(::core::option::Option::None),
        });
    };
    let entries = match &root.value {
        RootValue::Type(ty) => {
            return Ok(ExpandedRoot {
                items: quote! {
                    #[doc(hidden)]
                    pub fn #object() -> #value {
                        #object_like::into_value(<#ty as ::core::default::Default>::default())
                    }
                },
                fragment_root: unstated,
                root_info: quote! {
                    ::aosabi::descriptor::RootInfo::Type(::aosabi::descriptor::TypeLink::of::<#ty>())
                },
                factory: quote!(::core::option::Option::Some(#factory(#object))),
            });
        }
        RootValue::Entries(entries) => entries,
    };

    let mut items = TokenStream::new();
    let mut segments = Vec::new();
    let mut run = Vec::new();
    let mut parts = Vec::new();
    let mut inline_names: Vec<String> = Vec::new();
    for (index, item) in entries.iter().enumerate() {
        match item {
            Item::Entry(entry) => {
                let expanded = expand_entry(key, index, entry, &mut inline_names)?;
                items.extend(expanded.items);
                run.push(expanded.info);
                parts.push(expanded.part);
            }
            Item::Spread(spread) => {
                if !run.is_empty() {
                    segments.push(entries_segment(&run));
                    run.clear();
                }
                let expanded = expand_spread(key, index, spread);
                items.extend(expanded.items);
                segments.push(expanded.segment);
                parts.push(expanded.part);
            }
        }
    }
    if !run.is_empty() {
        segments.push(entries_segment(&run));
    }
    items.extend(quote! {
        #[doc(hidden)]
        pub fn #object() -> #value {
            #root_object(&[#(#parts),*])
        }
    });
    Ok(ExpandedRoot {
        items,
        fragment_root: quote! {
            #[doc(hidden)]
            pub static #upper: #fragment_root_type = #fragment_root_type {
                name: #name,
                fragment: #fragment,
                krate: ::core::env!("CARGO_PKG_NAME"),
                source: #source,
                stated: true,
                segments: &[#(#segments),*],
            };
        },
        root_info: quote!(::aosabi::descriptor::RootInfo::Fragment(&#upper)),
        factory: quote!(::core::option::Option::Some(#factory(#object))),
    })
}

/// Returns the path of `item` in the glue module of the facade.
fn glue_path(item: &str) -> TokenStream {
    let item = format_ident!("{item}");
    quote!(::aosabi::__private::glue::#item)
}

/// Returns the name of the object function of the root `key`.
fn object_fn(key: RootKey) -> Ident {
    format_ident!("__aosabi_object_{}", key.name())
}

/// Returns the segment of a run of entries.
fn entries_segment(run: &[TokenStream]) -> TokenStream {
    quote!(::aosabi::descriptor::Segment::Entries(&[#(#run),*]))
}

/// The tokens one entry adds.
struct ExpandedEntry {
    /// The functions of the entry.
    items: TokenStream,
    /// The `EntryInfo` literal.
    info: TokenStream,
    /// The `Part` of the object of the root.
    part: TokenStream,
}

/// The tokens one value of an entry adds.
struct ExpandedValue {
    /// The functions of the value.
    items: TokenStream,
    /// The `EntryKind` expression.
    kind: TokenStream,
    /// The body of the function of the executor, over `agent` and `args`.
    body: TokenStream,
}

/// Returns the tokens of `entry`, item `index` of the root `key`.
///
/// `inline_names` holds the names of the inline functions of the root so
/// far, and a name that stands twice takes the index as a suffix.
fn expand_entry(
    key: RootKey,
    index: usize,
    entry: &Entry,
    inline_names: &mut Vec<String>,
) -> syn::Result<ExpandedEntry> {
    let documentation = docs::item(&entry.docs)?;
    let name = unraw(&entry.name);
    let docs = documentation.to_tokens();
    let site = documentation.site_tokens(entry.name.span());
    let value = expand_value(
        key,
        index,
        entry,
        &entry.value,
        &documentation,
        inline_names,
    )?;
    let decorators: Vec<&Expr> = entry
        .decorators
        .iter()
        .map(|decorate| &decorate.expr)
        .collect();
    let facts = entry.decorators.iter().map(|decorate| {
        let (text, line) = (&decorate.text, decorate.line);
        quote!(::aosabi::descriptor::DecoratorFact { text: #text, line: #line })
    });
    let body = glue::decorated_body(&decorators, &value.body);
    let run = format_ident!("__aosabi_run_{}_{index}", key.name());
    let run_fn = glue::run_fn(&quote!(pub), &run, &body);
    let kind = value.kind;
    let mut items = value.items;
    items.extend(run_fn);
    let part = glue_path("Part");
    Ok(ExpandedEntry {
        items,
        info: quote! {
            ::aosabi::descriptor::EntryInfo {
                name: #name,
                docs: #docs,
                site: #site,
                kind: #kind,
                decorators: &[#(#facts),*],
            }
        },
        part: quote!(#part::Entry(#name, #run)),
    })
}

/// Returns the tokens of `value`, the value of `entry` or a value under its
/// `mutation(...)`.
fn expand_value(
    key: RootKey,
    index: usize,
    entry: &Entry,
    value: &Value,
    documentation: &Documentation,
    inline_names: &mut Vec<String>,
) -> syn::Result<ExpandedValue> {
    let kind = quote!(::aosabi::descriptor::EntryKind);
    match value {
        Value::Inline(inline) => expand_inline(key, entry, inline, documentation, inline_names),
        Value::Reference(path) => {
            let target = path_rule::reference(path)?;
            Ok(reference(key, index, &target, &quote!(#kind::Reference)))
        }
        Value::Shorthand => {
            let target = path_rule::shorthand(&entry.name);
            Ok(reference(key, index, &target, &quote!(#kind::Shorthand)))
        }
        Value::Mutation(call, inner) => {
            let inner = expand_value(key, index, entry, inner, documentation, inline_names)?;
            let inner_kind = inner.kind;
            let wrapped = glue::mutation_body(&inner.body);
            // The call names `mutation` as the reference names the import
            // of its wrapper, so the import is used and a literal without it
            // fails where the reference fails.
            Ok(ExpandedValue {
                items: inner.items,
                kind: quote!(#kind::Mutation(&#inner_kind)),
                body: quote! {
                    let _ = #call::<()>;
                    #wrapped
                },
            })
        }
    }
}

/// Returns the tokens of a reference or a shorthand whose `EntryFunction` is
/// the expression `target`, with the `EntryKind` variant `variant`.
fn reference(
    key: RootKey,
    index: usize,
    target: &TokenStream,
    variant: &TokenStream,
) -> ExpandedValue {
    let info = format_ident!("__aosabi_info_{}_{index}", key.name());
    ExpandedValue {
        items: quote! {
            #[doc(hidden)]
            pub fn #info() -> &'static ::aosabi::descriptor::MethodInfo {
                #target.info
            }
        },
        kind: quote!(#variant(#info)),
        body: quote!((#target.run)(agent, args)),
    }
}

/// Returns the tokens of the inline entry `inline` of `entry`, which becomes
/// the function `<Root>__<name>` of the hidden module.
fn expand_inline(
    key: RootKey,
    entry: &Entry,
    inline: &Inline,
    documentation: &Documentation,
    inline_names: &mut Vec<String>,
) -> syn::Result<ExpandedValue> {
    let name = unraw(&entry.name);
    let base = format!("{}__{name}", key.name());
    let function = if inline_names.contains(&base) {
        format!("{base}_{}", inline_names.len())
    } else {
        base
    };
    inline_names.push(function.clone());
    let function = Ident::new(&function, entry.name.span());

    let mut infos = Vec::new();
    let mut params = Vec::new();
    let mut signature = Vec::new();
    for param in &inline.params {
        infos.push(param_info_of(
            &param.ident,
            &param.ty,
            &param.keys,
            &param.docs,
            documentation,
        )?);
        params.push(Param {
            name: unraw(&param.ident),
            ty: Some(param.ty.clone()),
            default: glue::default_of(&param.keys),
        });
        let (pat, ty) = (&param.pat, &param.ty);
        signature.push(quote!(#pat: #ty));
    }
    let returns = returns_of(&inline.output, inline.is_async)?;
    let (docs, site) = (
        documentation.to_tokens(),
        documentation.site_tokens(entry.name.span()),
    );
    let is_async = inline.is_async;
    let asyncness = is_async.then(|| quote!(async));
    let (output, body) = (&inline.output, &inline.body);
    let guards = guards::facts(body);
    let call = glue::call_body(&quote!(#function), &params, is_async);
    let item = quote! {
        #[doc(hidden)]
        pub #asyncness fn #function(#(#signature),*) -> #output #body
    };
    Ok(ExpandedValue {
        items: item,
        kind: quote! {
            ::aosabi::descriptor::EntryKind::Inline(&::aosabi::descriptor::MethodInfo {
                name: #name,
                docs: #docs,
                site: #site,
                visibility: ::aosabi::descriptor::Visibility::Public,
                is_async: #is_async,
                params: &[#(#infos),*],
                returns: #returns,
                receiverless: true,
                decorators: &[],
                guards: #guards,
            })
        },
        body: call,
    })
}

/// The tokens one spread adds.
struct ExpandedSpread {
    /// The function that answers the root of the other fragment.
    items: TokenStream,
    /// The `Segment` of the spread.
    segment: TokenStream,
    /// The `Part` of the object of the root.
    part: TokenStream,
}

/// Returns the tokens of `spread`, item `index` of the root `key`.
fn expand_spread(key: RootKey, index: usize, spread: &Spread) -> ExpandedSpread {
    let module = path_rule::spread_module(&spread.path);
    let upper = format_ident!("{}", spread.key.upper(), span = spread.path.span());
    let object = object_fn(spread.key);
    let function = format_ident!("__aosabi_spread_{}_{index}", key.name());
    let part = glue_path("Part");
    ExpandedSpread {
        items: quote! {
            #[doc(hidden)]
            pub fn #function() -> &'static ::aosabi::descriptor::FragmentRoot {
                &#module::#upper
            }
        },
        segment: quote!(::aosabi::descriptor::Segment::Spread(#function)),
        part: quote!(#part::Spread(#module::#object)),
    }
}
