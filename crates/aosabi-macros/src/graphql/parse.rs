// SPDX-License-Identifier: MIT
/*
 * aosabi-macros: the grammar of graphql! and the refusal at the token that
 * breaks it.
 *
 * No analogue on the reference side.
 */

//! The grammar of `graphql!`.
//!
//! ```text
//! graphql! {
//!     <attr>* <vis> const <name> = {
//!         <root>,*
//!     };
//! }
//! <root>     := <RootKey> : { <item>,* }  |  <RootKey> : <type path>  |  <RootKey>
//! <RootKey>  := Query | Mutation | Subscription
//! <item>     := <doc>* <decorate>* <name> : <value>  |  <doc>* <decorate>* <name>  |  .. <path> . <RootKey>
//! <value>    := |<params>| -> <type> <block>  |  async |<params>| -> <type> <block>  |  <path>  |  mutation ( <value> )
//! <decorate> := #[decorate( <expr> )]
//! ```
//!
//! A token that breaks it is refused with a compile error at that token.

use proc_macro2::{
    Span,
    TokenStream, //
};
use quote::ToTokens;
use syn::{
    Attribute,
    Block,
    Expr,
    ExprClosure,
    ExprPath,
    Ident,
    Pat,
    Path,
    ReturnType,
    Token,
    Type,
    Visibility,
    braced,
    parenthesized,
    parse::{
        Parse,
        ParseStream, //
    },
    spanned::Spanned,
    token, //
};

use crate::refuse::{
    DECORATE_FORM,
    GRAPHQL_DUPLICATE_ROOT,
    GRAPHQL_ENTRY_ATTRIBUTE,
    GRAPHQL_ENTRY_VALUE,
    GRAPHQL_INLINE_MODIFIER,
    GRAPHQL_INLINE_PARAMETER,
    GRAPHQL_INLINE_RETURN,
    GRAPHQL_NOT_CONST,
    GRAPHQL_SPREAD_ATTRIBUTE,
    GRAPHQL_SPREAD_FORM,
    GRAPHQL_UNKNOWN_ROOT,
    refusal, //
};

/// The three roots of an application.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum RootKey {
    /// `Query`.
    Query,
    /// `Mutation`.
    Mutation,
    /// `Subscription`.
    Subscription,
}

impl RootKey {
    /// The three roots in the order of the fields of `RootsInfo`.
    pub(crate) const ALL: [Self; 3] = [Self::Query, Self::Mutation, Self::Subscription];

    /// Returns the name of the root.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Query => "Query",
            Self::Mutation => "Mutation",
            Self::Subscription => "Subscription",
        }
    }

    /// Returns the name of the static of the root in the hidden module.
    pub(crate) const fn upper(self) -> &'static str {
        match self {
            Self::Query => "QUERY",
            Self::Mutation => "MUTATION",
            Self::Subscription => "SUBSCRIPTION",
        }
    }

    /// Returns the root the identifier `ident` names, `None` for any other
    /// identifier.
    fn of(ident: &Ident) -> Option<Self> {
        Self::ALL.into_iter().find(|key| ident == key.name())
    }
}

/// The input of one invocation.
pub(crate) struct Input {
    /// The attributes above the constant, its doc comments among them.
    pub(crate) attrs: Vec<Attribute>,
    /// The visibility of the constant.
    pub(crate) vis: Visibility,
    /// The name of the constant, the twin of the exported name.
    pub(crate) name: Ident,
    /// The roots in source order.
    pub(crate) roots: Vec<Root>,
}

/// One root of the literal.
pub(crate) struct Root {
    /// Which root.
    pub(crate) key: RootKey,
    /// What the root holds.
    pub(crate) value: RootValue,
}

/// What a root holds.
pub(crate) enum RootValue {
    /// A block of entries and spreads in source order.
    Entries(Vec<Item>),
    /// A type path, or the key alone, which names the struct of its name.
    Type(Box<Type>),
}

/// One item of a block of entries.
pub(crate) enum Item {
    /// An entry, a key and its value.
    Entry(Box<Entry>),
    /// A spread of the root of another fragment.
    Spread(Spread),
}

/// One entry of a root.
pub(crate) struct Entry {
    /// The doc comments above the entry.
    pub(crate) docs: Vec<Attribute>,
    /// The decorators above the entry, in source order.
    pub(crate) decorators: Vec<Decorate>,
    /// The key of the entry.
    pub(crate) name: Ident,
    /// The value of the entry.
    pub(crate) value: Value,
}

/// One `#[decorate(expr)]`.
pub(crate) struct Decorate {
    /// The expression of the decorator.
    pub(crate) expr: Expr,
    /// The expression as written, which the fact of the decorator carries.
    pub(crate) text: String,
    /// The source line of the attribute.
    pub(crate) line: u32,
}

/// The value of an entry.
pub(crate) enum Value {
    /// A closure written inline.
    Inline(Box<Inline>),
    /// A path to a static or to a function under `#[aosabi::resolver]`.
    Reference(ExprPath),
    /// The key alone, the name of a function in scope.
    Shorthand,
    /// `mutation(...)` around another value, with the identifier of the call.
    Mutation(Ident, Box<Self>),
}

/// A closure written inline.
pub(crate) struct Inline {
    /// Whether the closure is `async`.
    pub(crate) is_async: bool,
    /// The typed parameters in declaration order.
    pub(crate) params: Vec<Param>,
    /// The return type.
    pub(crate) output: Type,
    /// The body.
    pub(crate) body: Block,
}

/// One parameter of an inline entry.
pub(crate) struct Param {
    /// The doc comments on the parameter.
    pub(crate) docs: Vec<Attribute>,
    /// The `#[aosabi(...)]` keys on the parameter.
    pub(crate) keys: Vec<Attribute>,
    /// The pattern, one identifier with an optional `mut`.
    pub(crate) pat: Pat,
    /// The identifier.
    pub(crate) ident: Ident,
    /// The type.
    pub(crate) ty: Type,
}

/// A spread of the root of another fragment.
pub(crate) struct Spread {
    /// The path of the constant of the other fragment.
    pub(crate) path: Path,
    /// The root spread.
    pub(crate) key: RootKey,
}

impl Parse for Input {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        let vis: Visibility = input.parse()?;
        if !input.peek(Token![const]) {
            return Err(refusal(input.span(), GRAPHQL_NOT_CONST));
        }
        input.parse::<Token![const]>()?;
        let name: Ident = input.parse()?;
        input.parse::<Token![=]>()?;
        let content;
        braced!(content in input);
        let roots = roots(&content)?;
        input.parse::<Token![;]>()?;
        if !input.is_empty() {
            return Err(refusal(input.span(), GRAPHQL_NOT_CONST));
        }
        Ok(Self {
            attrs,
            vis,
            name,
            roots,
        })
    }
}

/// Parses the roots of the literal.
fn roots(input: ParseStream<'_>) -> syn::Result<Vec<Root>> {
    let mut roots: Vec<Root> = Vec::new();
    while !input.is_empty() {
        let ident: Ident = input.parse()?;
        let Some(key) = RootKey::of(&ident) else {
            return Err(refusal(ident.span(), GRAPHQL_UNKNOWN_ROOT));
        };
        if roots.iter().any(|root| root.key == key) {
            return Err(refusal(ident.span(), GRAPHQL_DUPLICATE_ROOT));
        }
        let value = if input.peek(Token![:]) {
            input.parse::<Token![:]>()?;
            if input.peek(token::Brace) {
                let content;
                braced!(content in input);
                RootValue::Entries(items(&content)?)
            } else {
                RootValue::Type(Box::new(input.parse()?))
            }
        } else {
            let ty: Type = syn::parse2(ident.to_token_stream())?;
            RootValue::Type(Box::new(ty))
        };
        roots.push(Root { key, value });
        if input.is_empty() {
            break;
        }
        input.parse::<Token![,]>()?;
    }
    Ok(roots)
}

/// Parses the items of a block of entries.
fn items(input: ParseStream<'_>) -> syn::Result<Vec<Item>> {
    let mut items = Vec::new();
    while !input.is_empty() {
        let attrs = input.call(Attribute::parse_outer)?;
        if input.peek(Token![..]) {
            if let Some(attr) = attrs.first() {
                return Err(refusal(attr.span(), GRAPHQL_SPREAD_ATTRIBUTE));
            }
            items.push(Item::Spread(spread(input)?));
        } else {
            items.push(Item::Entry(Box::new(entry(input, attrs)?)));
        }
        if input.is_empty() {
            break;
        }
        input.parse::<Token![,]>()?;
    }
    Ok(items)
}

/// Parses a spread, `..path.Root`.
fn spread(input: ParseStream<'_>) -> syn::Result<Spread> {
    let dots = input.parse::<Token![..]>()?;
    let path = Path::parse_mod_style(input)?;
    if !input.peek(Token![.]) {
        return Err(refusal(dots.span(), GRAPHQL_SPREAD_FORM));
    }
    input.parse::<Token![.]>()?;
    let root: Ident = input.parse()?;
    let Some(key) = RootKey::of(&root) else {
        return Err(refusal(root.span(), GRAPHQL_SPREAD_FORM));
    };
    Ok(Spread { path, key })
}

/// Parses an entry whose attributes `attrs` stand above it.
fn entry(input: ParseStream<'_>, attrs: Vec<Attribute>) -> syn::Result<Entry> {
    let mut docs = Vec::new();
    let mut decorators = Vec::new();
    for attr in attrs {
        if attr.path().is_ident("doc") {
            docs.push(attr);
        } else if attr.path().is_ident("decorate") {
            decorators.push(decorate(&attr)?);
        } else {
            return Err(refusal(attr.span(), GRAPHQL_ENTRY_ATTRIBUTE));
        }
    }
    let name: Ident = input.parse()?;
    let value = if input.peek(Token![:]) {
        input.parse::<Token![:]>()?;
        value(input)?
    } else {
        Value::Shorthand
    };
    Ok(Entry {
        docs,
        decorators,
        name,
        value,
    })
}

/// Parses the value of an entry.
fn value(input: ParseStream<'_>) -> syn::Result<Value> {
    if input.peek(Token![async])
        || input.peek(Token![|])
        || input.peek(Token![||])
        || input.peek(Token![move])
        || input.peek(Token![const])
        || input.peek(Token![for])
    {
        let closure: ExprClosure = input.parse()?;
        return Ok(Value::Inline(Box::new(inline(closure)?)));
    }
    if input.peek(Ident) && input.peek2(token::Paren) {
        let fork = input.fork();
        let ident: Ident = fork.parse()?;
        if ident == "mutation" {
            let ident: Ident = input.parse()?;
            let content;
            parenthesized!(content in input);
            let inner = value(&content)?;
            if !content.is_empty() {
                return Err(refusal(content.span(), GRAPHQL_ENTRY_VALUE));
            }
            return Ok(Value::Mutation(ident, Box::new(inner)));
        }
    }
    let span = input.span();
    let path: ExprPath = input
        .parse()
        .map_err(|_| refusal(span, GRAPHQL_ENTRY_VALUE))?;
    if !input.is_empty() && !input.peek(Token![,]) {
        return Err(refusal(input.span(), GRAPHQL_ENTRY_VALUE));
    }
    Ok(Value::Reference(path))
}

/// Returns the inline entry of `closure`.
fn inline(closure: ExprClosure) -> syn::Result<Inline> {
    if let Some(lifetimes) = &closure.lifetimes {
        return Err(refusal(lifetimes.span(), GRAPHQL_INLINE_MODIFIER));
    }
    if let Some(constness) = &closure.constness {
        return Err(refusal(constness.span(), GRAPHQL_INLINE_MODIFIER));
    }
    let ReturnType::Type(_, output) = closure.output else {
        return Err(refusal(closure.or2_token.span(), GRAPHQL_INLINE_RETURN));
    };
    let body = match *closure.body {
        Expr::Block(block) if block.label.is_none() && block.attrs.is_empty() => block.block,
        other => return Err(refusal(other.span(), GRAPHQL_INLINE_RETURN)),
    };
    let params = closure
        .inputs
        .into_iter()
        .map(param)
        .collect::<syn::Result<_>>()?;
    Ok(Inline {
        is_async: closure.asyncness.is_some(),
        params,
        output: *output,
        body,
    })
}

/// Returns the parameter of the closure input `pat`.
fn param(pat: Pat) -> syn::Result<Param> {
    let span = pat.span();
    let Pat::Type(typed) = pat else {
        return Err(refusal(span, GRAPHQL_INLINE_PARAMETER));
    };
    let Pat::Ident(pattern) = *typed.pat else {
        return Err(refusal(span, GRAPHQL_INLINE_PARAMETER));
    };
    if pattern.by_ref.is_some() || pattern.subpat.is_some() || !pattern.attrs.is_empty() {
        return Err(refusal(span, GRAPHQL_INLINE_PARAMETER));
    }
    let (docs, rest): (Vec<Attribute>, Vec<Attribute>) = typed
        .attrs
        .into_iter()
        .partition(|attr| attr.path().is_ident("doc"));
    let ident = pattern.ident.clone();
    Ok(Param {
        docs,
        keys: rest,
        pat: Pat::Ident(pattern),
        ident,
        ty: *typed.ty,
    })
}

/// Returns the decorator of the attribute `attr`, `#[decorate(expr)]`.
pub(crate) fn decorate(attr: &Attribute) -> syn::Result<Decorate> {
    let expr: Expr = attr
        .parse_args()
        .map_err(|_| refusal(attr.span(), DECORATE_FORM))?;
    let text = attr
        .meta
        .require_list()
        .map_or_else(|_| expr.to_token_stream(), |list| list.tokens.clone())
        .to_string();
    Ok(Decorate {
        expr,
        text,
        line: line_of(attr.span()),
    })
}

/// Returns the source line of `span`, counted from 1.
pub(crate) fn line_of(span: Span) -> u32 {
    u32::try_from(span.unwrap().line()).unwrap_or(u32::MAX)
}

/// Parses the input of one invocation.
pub(crate) fn parse(input: TokenStream) -> syn::Result<Input> {
    syn::parse2(input)
}
