// SPDX-License-Identifier: MIT
/*
 * aosabi-macros: the parser of `#[aosabi(...)]`.
 *
 * No analogue on the reference side. The keys state what a TypeScript class
 * states in its own syntax: the `__typename` literal, the `extends` and
 * `implements` clauses, the `enum` keyword of a declaration, the string of a
 * literal union member, the initializer of a parameter and the spread
 * `{...value}` of an object literal.
 */

//! The parser of `#[aosabi(...)]` on a type, a field, a variant, a resolver and
//! a parameter.
//!
//! Each position has a closed set of keys, and a key outside the set is a
//! refusal at the key. The keys `remote` and `arguments` on a type state that
//! it belongs to a registry `aosabi pull` writes, a type of the remote that
//! keeps the remote object it is read from and the arguments of a remote
//! field. The key `select = Type` on a type belongs to the derive
//! `Select` of the typed client, which reads the same helper attribute, so
//! the derive `Aosabi` accepts it and leaves it to that derive. The derive
//! declares `aosabi` as its helper attribute, so the compiler accepts it under
//! `#[derive(Aosabi)]`. Inside an `#[aosabi::resolvers]` block no derive is in
//! scope, so the attribute macro takes the helper off every resolver and
//! parameter before it emits the block.

use proc_macro2::Span;
use syn::{
    Attribute,
    Expr,
    LitStr,
    Token,
    Type,
    meta::ParseNestedMeta,
    punctuated::Punctuated,
    spanned::Spanned, //
};

use crate::refuse::{
    DUPLICATE_KEY,
    RESOLVERS_KEY,
    SPREAD_AND_EXTENDS,
    TYPE_EXTENDS,
    UNKNOWN_FIELD_KEY,
    UNKNOWN_METHOD_KEY,
    UNKNOWN_PARAMETER_KEY,
    UNKNOWN_TYPE_KEY,
    UNKNOWN_VARIANT_KEY,
    refusal, //
};

/// What `#[aosabi(...)]` states on a type.
#[derive(Default)]
pub(crate) struct TypeAttrs {
    /// `typename = "Name"`, the name override.
    pub(crate) typename: Option<LitStr>,
    /// `anonymous`, the statement that the TypeScript twin has no alias.
    pub(crate) anonymous: bool,
    /// `enum`, the statement that the TypeScript twin is an `enum`
    /// declaration, with the span of the key for a refusal.
    pub(crate) enum_declaration: Option<Span>,
    /// Every base type of `implements(Base, ...)`, in attribute order.
    pub(crate) implements: Vec<Type>,
    /// `remote`, the statement that the type belongs to a pulled registry and
    /// keeps the remote object it is read from, with the span of the key.
    pub(crate) remote: Option<Span>,
    /// `arguments`, the statement that the struct holds the arguments of a
    /// remote field and describes them as a parameter list, with the span of
    /// the key.
    pub(crate) arguments: Option<Span>,
}

/// What `#[aosabi(...)]` states on a field.
#[derive(Default)]
pub(crate) struct FieldAttrs {
    /// `extends`, the statement that the field embeds the base of the
    /// struct, with the span of the key.
    pub(crate) extends: Option<Span>,
    /// The key `spread`, the statement that the field stands for an object
    /// spread `{...value}`, whose members the inference presents in place of
    /// the field, with the span of the key.
    pub(crate) spread: Option<Span>,
}

/// What `#[aosabi(...)]` states on a variant.
#[derive(Default)]
pub(crate) struct VariantAttrs {
    /// `value = "text"`, the string the variant stands for.
    pub(crate) value: Option<LitStr>,
}

/// What `#[aosabi(...)]` states on a parameter.
#[derive(Default)]
pub(crate) struct ParamAttrs {
    /// `default = expr`, which makes the parameter optional.
    pub(crate) default: bool,
}

/// Returns whether `attr` is the helper attribute `#[aosabi(...)]`.
fn is_aosabi(attr: &Attribute) -> bool {
    attr.path().is_ident("aosabi")
}

/// Removes every `#[aosabi(...)]` from `attrs` and returns them in order.
pub(crate) fn take(attrs: &mut Vec<Attribute>) -> Vec<Attribute> {
    let (taken, kept) = core::mem::take(attrs).into_iter().partition(is_aosabi);
    *attrs = kept;
    taken
}

/// Returns what `#[aosabi(...)]` states on the type that carries `attrs`.
///
/// # Errors
///
/// Returns the refusal of an unknown key, of the key `resolvers`, of a base
/// stated as `extends(...)`, of `typename` stated twice, and the error of
/// `syn` for a value of the wrong form.
pub(crate) fn type_attrs(attrs: &[Attribute]) -> syn::Result<TypeAttrs> {
    let mut out = TypeAttrs::default();
    for attr in attrs.iter().filter(|attr| is_aosabi(attr)) {
        attr.parse_nested_meta(|meta| type_key(&mut out, &meta))?;
    }
    Ok(out)
}

/// Reads one key of a type into `out`.
fn type_key(out: &mut TypeAttrs, meta: &ParseNestedMeta<'_>) -> syn::Result<()> {
    if meta.path.is_ident("resolvers") {
        return Err(meta.error(RESOLVERS_KEY));
    } else if meta.path.is_ident("anonymous") {
        out.anonymous = true;
    } else if meta.path.is_ident("enum") {
        out.enum_declaration = Some(meta.path.span());
    } else if meta.path.is_ident("typename") {
        set_once(&mut out.typename, meta)?;
    } else if meta.path.is_ident("extends") {
        return Err(meta.error(TYPE_EXTENDS));
    } else if meta.path.is_ident("implements") {
        bases(out, meta)?;
    } else if meta.path.is_ident("remote") {
        out.remote = Some(meta.path.span());
    } else if meta.path.is_ident("arguments") {
        out.arguments = Some(meta.path.span());
    } else if meta.path.is_ident("select") {
        meta.value()?.parse::<Type>()?;
    } else {
        return Err(meta.error(UNKNOWN_TYPE_KEY));
    }
    Ok(())
}

/// Reads the `= "text"` of a key into `slot`, which has to be empty.
fn set_once(slot: &mut Option<LitStr>, meta: &ParseNestedMeta<'_>) -> syn::Result<()> {
    if slot.is_some() {
        return Err(meta.error(DUPLICATE_KEY));
    }
    *slot = Some(meta.value()?.parse()?);
    Ok(())
}

/// Reads the `(Base, ...)` of `implements` into `out`.
fn bases(out: &mut TypeAttrs, meta: &ParseNestedMeta<'_>) -> syn::Result<()> {
    let content;
    syn::parenthesized!(content in meta.input);
    let types = Punctuated::<Type, Token![,]>::parse_terminated(&content)?;
    out.implements.extend(types);
    Ok(())
}

/// Returns what `#[aosabi(...)]` states on the variant that carries `attrs`.
///
/// # Errors
///
/// Returns the refusal of an unknown key and of `value` stated twice.
pub(crate) fn variant_attrs(attrs: &[Attribute]) -> syn::Result<VariantAttrs> {
    let mut out = VariantAttrs::default();
    for attr in attrs.iter().filter(|attr| is_aosabi(attr)) {
        attr.parse_nested_meta(|meta| {
            if !meta.path.is_ident("value") {
                return Err(meta.error(UNKNOWN_VARIANT_KEY));
            }
            set_once(&mut out.value, &meta)
        })?;
    }
    Ok(out)
}

/// Returns what `#[aosabi(...)]` states on a parameter.
///
/// `attrs` is what [`take`] removed from the parameter.
///
/// # Errors
///
/// Returns the refusal of an unknown key and the error of `syn` for a default
/// that is no expression.
pub(crate) fn param_attrs(attrs: &[Attribute]) -> syn::Result<ParamAttrs> {
    let mut out = ParamAttrs::default();
    for attr in attrs {
        attr.parse_nested_meta(|meta| {
            if !meta.path.is_ident("default") {
                return Err(meta.error(UNKNOWN_PARAMETER_KEY));
            }
            meta.value()?.parse::<Expr>()?;
            out.default = true;
            Ok(())
        })?;
    }
    Ok(out)
}

/// Returns what `#[aosabi(...)]` states on a field, whose keys are `extends`
/// and `spread`.
///
/// # Errors
///
/// Returns the refusal of an unknown key, of a key that stands twice and of
/// `extends` beside `spread`.
pub(crate) fn field_attrs(attrs: &[Attribute]) -> syn::Result<FieldAttrs> {
    let mut out = FieldAttrs::default();
    for attr in attrs.iter().filter(|attr| is_aosabi(attr)) {
        attr.parse_nested_meta(|meta| {
            let bare = meta.input.is_empty() || meta.input.peek(Token![,]);
            if !bare {
                return Err(meta.error(UNKNOWN_FIELD_KEY));
            }
            let slot = if meta.path.is_ident("extends") {
                &mut out.extends
            } else if meta.path.is_ident("spread") {
                &mut out.spread
            } else {
                return Err(meta.error(UNKNOWN_FIELD_KEY));
            };
            if slot.is_some() {
                return Err(meta.error(DUPLICATE_KEY));
            }
            *slot = Some(meta.path.span());
            Ok(())
        })?;
    }
    if let (Some(_), Some(span)) = (out.extends, out.spread) {
        return Err(refusal(span, SPREAD_AND_EXTENDS));
    }
    Ok(out)
}

/// Refuses every key of `#[aosabi(...)]` on a resolver.
///
/// `attrs` is what [`take`] removed from the resolver.
///
/// # Errors
///
/// Returns the refusal of the first key.
pub(crate) fn method_attrs(attrs: &[Attribute]) -> syn::Result<()> {
    no_key(attrs, UNKNOWN_METHOD_KEY)
}

/// Refuses with `message` at the first key of an `#[aosabi(...)]` in `attrs`.
fn no_key(attrs: &[Attribute], message: &str) -> syn::Result<()> {
    for attr in attrs.iter().filter(|attr| is_aosabi(attr)) {
        attr.parse_nested_meta(|meta| Err(meta.error(message)))?;
    }
    Ok(())
}
