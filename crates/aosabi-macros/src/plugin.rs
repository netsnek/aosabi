// SPDX-License-Identifier: MIT
/*
 * aosabi-macros: plugin! and plugins!.
 */

//! `plugin!` and `plugins!`.
//!
//! `plugin!` reads `key: expr` pairs and one optional spread `..expr`. The
//! keys are the eight members of a plugin in snake case and every hook
//! of the plugin contract, and each value goes through the helper of
//! `glue::__wrap` of its key, so a closure reaches the boxed type of its
//! member. Any other key is refused at its token, `inner` among them, which
//! is the field the spread fills, and a second spread is refused at its
//! value. A `setup`, a `build` or a `middleware` whose value is an `async`
//! closure goes through the asynchronous helper, the macro choosing by the
//! `async` token. The spread is the twin of `...other`, a plugin whose
//! members answer every key the literal leaves out, and it stands anywhere in
//! the literal, since a key of the literal wins over the spread in either
//! order. The expansion is a `PluginObject` literal with
//! `..Default::default()`, every key it writes a field that is `Some`, so the
//! members of the plugin are the keys of the literal and the members of its
//! spread.
//!
//! The keys `roles`, `strategy` and `depends_on` also take the reference's
//! text, which the macro reads at compile time into the value the field
//! stores:
//!
//! | Key | Literal form | Field | Refused at the token |
//! | --- | --- | --- | --- |
//! | `roles` | `["web"]`, `["worker"]`, `["web", "worker"]` | `Some(vec![PluginRole::Web, ...])` | any other string |
//! | `strategy` | `"first"` or `"last"` | `Some(Strategy::First)` or `Some(Strategy::Last)` | any other string |
//! | `depends_on` | `["identity", "database"]` | `Some(vec![String::from("identity"), ...])` | an element that is no string literal |
//!
//! A `roles` array is the literal form when it is empty or holds a string
//! literal, and a `depends_on` array always is. Any other value of the three
//! keys goes through the helper of its key, which wraps it in `Some`, so a
//! computed value and the enum form `Strategy::Last` compile, and check C4 of
//! `12_authoring_parity` reports a twin that writes the enum where its source
//! writes the literal.
//! The hook `on_execute` goes through its helper like every hook, whose
//! closure answers the execute hooks of `on_execute_done(...)` or a `Result`
//! of them.
//!
//! `plugins![a, b]` expands to `Some(PluginList)` with one `PluginSlot` per
//! element, which records the source text and the line of the element for
//! the boot report of a served binary and for check C4 of
//! `12_authoring_parity`.

use proc_macro2::TokenStream;
use quote::{
    ToTokens,
    format_ident,
    quote, //
};
use syn::{
    Expr,
    ExprArray,
    Ident,
    LitStr,
    Token,
    parse::{
        Parse,
        ParseStream,
        Parser, //
    },
    punctuated::Punctuated,
    spanned::Spanned, //
};

use crate::{
    graphql::line_of,
    guards::{
        string,
        unwrap, //
    },
    refuse::{
        PLUGIN_DEPENDENCY,
        PLUGIN_DUPLICATE_KEY,
        PLUGIN_ROLE,
        PLUGIN_SECOND_SPREAD,
        PLUGIN_STRATEGY,
        PLUGIN_UNKNOWN_KEY,
        refusal, //
    },
};

/// The members of a plugin in snake case, whose value goes through the
/// helper of the same name.
const MEMBERS: [&str; 5] = ["name", "roles", "strategy", "depends_on", "options"];

/// The members whose value is a closure that may be `async`.
const CLOSURES: [&str; 3] = ["setup", "build", "middleware"];

/// The hooks of the plugin contract in snake case, in the order of the
/// fields of `PluginObject`.
const HOOKS: [&str; 16] = [
    "on_plugin_init",
    "on_enveloped",
    "on_schema_change",
    "on_context_building",
    "on_parse",
    "on_validate",
    "on_execute",
    "on_subscribe",
    "on_request",
    "on_response",
    "on_dispose",
    "on_yoga_init",
    "on_request_parse",
    "on_params",
    "on_execution_result",
    "on_result_process",
];

/// One member of the literal.
enum Member {
    /// `key: value`.
    Key(Ident, Expr),
    /// `..other`.
    Spread(Expr),
}

impl Parse for Member {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        if input.peek(Token![..]) {
            input.parse::<Token![..]>()?;
            return Ok(Self::Spread(input.parse()?));
        }
        let key: Ident = input.parse()?;
        input.parse::<Token![:]>()?;
        Ok(Self::Key(key, input.parse()?))
    }
}

/// Expands one invocation of `plugin!`.
pub(crate) fn object(input: &TokenStream) -> TokenStream {
    expand_object(input).unwrap_or_else(|error| error.to_compile_error())
}

/// Returns the `PluginObject` literal of the members `input`.
fn expand_object(input: &TokenStream) -> syn::Result<TokenStream> {
    let members = Punctuated::<Member, Token![,]>::parse_terminated.parse2(input.clone())?;
    let wrap = quote!(::aosabi::__private::glue::__wrap);
    let mut seen: Vec<String> = Vec::new();
    let mut fields = Vec::new();
    let mut spread = false;
    for member in members {
        let (key, value) = match member {
            Member::Key(key, value) => (key, value),
            Member::Spread(value) => {
                if spread {
                    return Err(refusal(value.span(), PLUGIN_SECOND_SPREAD));
                }
                spread = true;
                let inner = format_ident!("inner", span = value.span());
                fields.push(quote!(#inner: #wrap::#inner(#value)));
                continue;
            }
        };
        let name = key.to_string();
        if seen.contains(&name) {
            return Err(refusal(key.span(), PLUGIN_DUPLICATE_KEY));
        }
        let helper = if MEMBERS.contains(&name.as_str()) || HOOKS.contains(&name.as_str()) {
            format_ident!("{name}", span = key.span())
        } else if CLOSURES.contains(&name.as_str()) {
            let is_async = matches!(&value, Expr::Closure(closure) if closure.asyncness.is_some());
            if is_async {
                format_ident!("{name}_async", span = key.span())
            } else {
                format_ident!("{name}", span = key.span())
            }
        } else {
            return Err(refusal(key.span(), PLUGIN_UNKNOWN_KEY));
        };
        let field = format_ident!("{name}", span = key.span());
        match literal(&name, &value, &wrap)? {
            Some(stored) => fields.push(quote!(#field: #stored)),
            None => fields.push(quote!(#field: #wrap::#helper(#value))),
        }
        seen.push(name);
    }
    Ok(quote! {
        ::aosabi::__private::glue::PluginObject {
            #(#fields,)*
            ..::core::default::Default::default()
        }
    })
}

/// Returns the value the field `key` stores for the literal form of `value`,
/// or `None` when `value` is no literal form and goes through its helper.
///
/// # Errors
///
/// Returns the refusal at the token of a role, a strategy or a dependency
/// the reference does not know.
fn literal(key: &str, value: &Expr, wrap: &TokenStream) -> syn::Result<Option<TokenStream>> {
    let some = quote!(::core::option::Option::Some);
    match key {
        "roles" => {
            let Some(array) = array(value) else {
                return Ok(None);
            };
            if !array.elems.is_empty()
                && array.elems.iter().all(|element| string(element).is_none())
            {
                return Ok(None);
            }
            let roles = array
                .elems
                .iter()
                .map(
                    |element| match string(element).map(LitStr::value).as_deref() {
                        Some("web") => Ok(quote!(#wrap::PluginRole::Web)),
                        Some("worker") => Ok(quote!(#wrap::PluginRole::Worker)),
                        _ => Err(refusal(element.span(), PLUGIN_ROLE)),
                    },
                )
                .collect::<syn::Result<Vec<_>>>()?;
            Ok(Some(quote!(#some(::std::vec![#(#roles),*]))))
        }
        "strategy" => {
            let Some(text) = string(value) else {
                return Ok(None);
            };
            let strategy = match text.value().as_str() {
                "first" => quote!(#wrap::Strategy::First),
                "last" => quote!(#wrap::Strategy::Last),
                _ => return Err(refusal(text.span(), PLUGIN_STRATEGY)),
            };
            Ok(Some(quote!(#some(#strategy))))
        }
        "depends_on" => {
            let Some(array) = array(value) else {
                return Ok(None);
            };
            let names = array
                .elems
                .iter()
                .map(|element| {
                    string(element)
                        .map(|name| quote!(::std::string::String::from(#name)))
                        .ok_or_else(|| refusal(element.span(), PLUGIN_DEPENDENCY))
                })
                .collect::<syn::Result<Vec<_>>>()?;
            Ok(Some(quote!(#some(::std::vec![#(#names),*]))))
        }
        _ => Ok(None),
    }
}

/// Returns the array literal `expr` is, or `None`.
fn array(expr: &Expr) -> Option<&ExprArray> {
    match unwrap(expr) {
        Expr::Array(array) => Some(array),
        _ => None,
    }
}

/// Expands one invocation of `plugins!`.
pub(crate) fn list(input: &TokenStream) -> TokenStream {
    expand_list(input).unwrap_or_else(|error| error.to_compile_error())
}

/// Returns the `Some(PluginList)` of the elements `input`.
fn expand_list(input: &TokenStream) -> syn::Result<TokenStream> {
    let elements = Punctuated::<Expr, Token![,]>::parse_terminated.parse2(input.clone())?;
    let slots = elements.iter().map(|element| {
        let text = element.to_token_stream().to_string();
        let line = line_of(element.span());
        quote!(::aosabi::__private::glue::PluginSlot::of(#element, #text, #line))
    });
    Ok(quote! {
        ::core::option::Option::Some(::aosabi::__private::glue::PluginList::new(
            ::std::vec![#(#slots),*]
        ))
    })
}
