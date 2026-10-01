// SPDX-License-Identifier: MIT
/*
 * aosabi-macros: the tokens the three macros share.
 *
 * The descriptor written here is what the TypeScript checker answers the
 * inference about a declaration.
 */

//! The tokens the three macros share: the `Described` implementation with its
//! `TypeInfo` and the declaration list of its target, and the constant of the
//! declaration.
//!
//! The generated code names every item of Aosabi through
//! `::aosabi::descriptor::` and `::aosabi::__private::`, and every item of the
//! language through `::core::`, so it compiles in a crate that shadows a
//! prelude name and in a crate with no standard library.

use proc_macro2::{
    Span,
    TokenStream, //
};
use quote::{
    format_ident,
    quote, //
};
use syn::{
    DeriveInput,
    GenericParam,
    Generics,
    Ident,
    Visibility,
    parse_quote,
    spanned::Spanned, //
};

use crate::{
    attrs,
    docs::{
        self,
        Documentation, //
    },
    links::type_link,
    list,
    refuse::{
        ENUM_KEY_NOT_ON_UNIT_ENUM,
        LIFETIME_OR_CONST_PARAMETER,
        NO_LIST_FOR_TARGET,
        refusal, //
    },
};

/// The `TypeKind` a macro writes.
#[derive(Clone, Copy)]
pub(crate) enum Kind {
    /// A struct.
    Object,
    /// An enum whose variants carry no data.
    Enum,
    /// An enum whose variants carry no data under `#[aosabi(enum)]`.
    EnumDeclaration,
    /// An enum whose variants each carry one payload.
    Union,
    /// A newtype under `#[aosabi::scalar]`.
    Scalar,
}

impl Kind {
    /// Returns the identifier of the `TypeKind` variant.
    fn ident(self) -> Ident {
        let name = match self {
            Self::Object => "Object",
            Self::Enum => "Enum",
            Self::EnumDeclaration => "EnumDeclaration",
            Self::Union => "Union",
            Self::Scalar => "Scalar",
        };
        Ident::new(name, Span::call_site())
    }
}

/// The lists of a descriptor that depend on the shape of the item.
///
/// Each element is the token stream of one record, so `fields` holds
/// `FieldInfo` literals, `variants` holds `VariantInfo` literals and `members`
/// holds `TypeLink` expressions.
#[derive(Default)]
pub(crate) struct Lists {
    /// The fields of a struct.
    pub(crate) fields: Vec<TokenStream>,
    /// The variants of an enum whose variants carry no data.
    pub(crate) variants: Vec<TokenStream>,
    /// The payload types of an enum whose variants each carry one.
    pub(crate) members: Vec<TokenStream>,
}

/// Everything a macro knows about the type it describes.
pub(crate) struct Descriptor<'a> {
    /// The identifier of the item.
    pub(crate) ident: &'a Ident,
    /// The generic parameters of the item.
    pub(crate) generics: &'a Generics,
    /// What the type is to the inference.
    pub(crate) kind: Kind,
    /// The name the descriptor states.
    pub(crate) name: String,
    /// The documentation of the item.
    pub(crate) docs: Documentation,
    /// The name override.
    pub(crate) typename: Option<String>,
    /// Whether the TypeScript twin has no alias.
    pub(crate) anonymous: bool,
    /// The lists that depend on the shape of the item.
    pub(crate) lists: Lists,
    /// The `HeritageInfo` literals, in attribute order.
    pub(crate) heritage: Vec<TokenStream>,
}

impl<'a> Descriptor<'a> {
    /// Returns the descriptor of a derive input of `kind` with `lists`.
    ///
    /// An enum whose variants carry no data becomes an enum declaration under
    /// the key `enum`.
    ///
    /// # Errors
    ///
    /// Returns the refusals of `#[aosabi(...)]` on the type, of the key `enum`
    /// on anything but a named enum whose variants carry no data, of the
    /// documentation of the type and of a base type that has no
    /// description.
    pub(crate) fn of_derive(input: &'a DeriveInput, kind: Kind, lists: Lists) -> syn::Result<Self> {
        let attrs = attrs::type_attrs(&input.attrs)?;
        let kind = match (kind, attrs.enum_declaration) {
            (Kind::Enum, Some(_)) if !attrs.anonymous => Kind::EnumDeclaration,
            (_, Some(span)) => return Err(refusal(span, ENUM_KEY_NOT_ON_UNIT_ENUM)),
            (kind, None) => kind,
        };
        let heritage = attrs
            .implements
            .iter()
            .map(implements_info)
            .collect::<syn::Result<_>>()?;
        Ok(Self {
            ident: &input.ident,
            generics: &input.generics,
            kind,
            name: unraw(&input.ident),
            docs: docs::item(&input.attrs)?,
            typename: attrs.typename.map(|name| name.value()),
            anonymous: attrs.anonymous,
            lists,
            heritage,
        })
    }

    /// Returns the implementations and the declaration of the type.
    ///
    /// The declaration and the list exist when the build script of the crate
    /// wrote the list of the target the macro expands in.
    ///
    /// # Errors
    ///
    /// Returns the refusal of a lifetime and of a const parameter, and the
    /// refusal of a target whose list the build script of the crate did not
    /// write.
    pub(crate) fn emit(&self) -> syn::Result<TokenStream> {
        let generics = described_generics(self.generics)?;
        let list =
            list::path().map_err(|list::Missing| refusal(self.ident.span(), NO_LIST_FOR_TARGET))?;
        let described = self.described_impl(&generics, list.as_deref());
        let declaration = list.map(|_| self.declaration());
        Ok(quote!(#described #declaration))
    }

    /// Returns `impl Described` with the `TypeInfo` of the type, and with the
    /// declaration list at the path `list` when there is one.
    fn described_impl(&self, generics: &Generics, list: Option<&str>) -> TokenStream {
        let ident = self.ident;
        let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
        let kind = self.kind.ident();
        let name = &self.name;
        let docs = self.docs.to_tokens();
        let site = self.docs.site_tokens(self.ident.span());
        let typename = option_str(self.typename.as_deref());
        let anonymous = self.anonymous;
        let source = source_key();
        let Lists {
            fields,
            variants,
            members,
        } = &self.lists;
        let methods = Self::methods(&quote!(#ident #type_generics));
        let type_arguments = type_arguments(generics);
        let heritage = &self.heritage;
        let declarations = list.map(|list| {
            quote! {
                const DECLARATIONS: &'static [::aosabi::descriptor::Declaration] =
                    ::core::include!(#list);
            }
        });
        quote! {
            impl #impl_generics ::aosabi::descriptor::Described for #ident #type_generics #where_clause {
                const TYPE: ::aosabi::descriptor::TypeExpr =
                    ::aosabi::descriptor::TypeExpr::Named(&::aosabi::descriptor::TypeInfo {
                        kind: ::aosabi::descriptor::TypeKind::#kind,
                        name: #name,
                        krate: ::core::env!("CARGO_PKG_NAME"),
                        source: #source,
                        docs: #docs,
                        site: #site,
                        typename: #typename,
                        anonymous: #anonymous,
                        fields: &[#(#fields),*],
                        methods: #methods,
                        variants: &[#(#variants),*],
                        members: &[#(#members),*],
                        type_arguments: &[#(#type_arguments),*],
                        heritage: &[#(#heritage),*],
                    });
                #declarations
            }
        }
    }

    /// Returns the expression of the `methods` list of `self_type`.
    ///
    /// The derive cannot see the resolvers block, so it reads the constant the
    /// block writes, which the compiler evaluates after both macros ran. It
    /// reads the constant by path with `ResolversFallback` in scope, so a type
    /// with no block reads the empty table of that trait and a type states no
    /// key for its block.
    fn methods(self_type: &TokenStream) -> TokenStream {
        quote!({
            #[allow(unused_imports, reason = "a type with a resolvers block reads its own table")]
            use ::aosabi::__private::glue::ResolversFallback as _;
            <#self_type>::__AOSABI_METHODS
        })
    }

    /// Returns the constant of the declaration of the type, which the
    /// declaration list of the crate names.
    ///
    /// A generic declaration names no type before it is instantiated, so it
    /// declares no link and no heritage. A base type may name a type
    /// parameter, and a constant has none in scope.
    ///
    /// The constant is `pub(crate)`, because the list stands in every
    /// `Described` implementation of the crate and names it from there. Its
    /// name is `__AOSABI_DECLARATION_` and the name of the item in upper case,
    /// which the build script writes into the list as well. A declaration the
    /// scan of the build script does not reach, one inside a function or one a
    /// declarative macro writes, is in no list and drops out of the heritage
    /// sweep silently. The compiler reports no unused constant for it, because
    /// the constant stands in the expansion of a procedural macro.
    fn declaration(&self) -> TokenStream {
        let (link, heritage) = if self.generics.params.is_empty() {
            let ident = self.ident;
            let link = quote!(::aosabi::descriptor::TypeLink::of::<#ident>());
            (option(Some(&link)), self.heritage.as_slice())
        } else {
            (option(None), [].as_slice())
        };
        declaration_constant(self.ident, &self.name, &link, heritage)
    }
}

/// Returns the constant of a declaration of `ident` whose name is `name`, with
/// the `Option<TypeLink>` expression `link` and the `HeritageInfo` literals
/// `heritage`.
fn declaration_constant(
    ident: &Ident,
    name: &str,
    link: &TokenStream,
    heritage: &[TokenStream],
) -> TokenStream {
    let source = source_key();
    let item = format_ident!("__AOSABI_DECLARATION_{}", unraw(ident).to_uppercase());
    quote! {
        pub(crate) const #item: ::aosabi::descriptor::Declaration = ::aosabi::descriptor::Declaration {
            krate: ::core::env!("CARGO_PKG_NAME"),
            source: #source,
            name: #name,
            link: #link,
            heritage: &[#(#heritage),*],
        };
    }
}

/// Returns the constant a refused macro emits for the item `ident` beside its
/// error, when the target has a declaration list.
///
/// The build stops at the refusal, so the constant is never read. The list
/// names it, and it keeps the list from adding an error of a missing name to
/// the refusal. It declares no link, because the refused item may be what the
/// link needs.
pub(crate) fn refused_declaration(ident: &Ident) -> TokenStream {
    if !matches!(list::path(), Ok(Some(_))) {
        return TokenStream::new();
    }
    declaration_constant(ident, &unraw(ident), &option(None), &[])
}

/// Returns the `HeritageInfo` literal of one base type of `implements`.
fn implements_info(base: &syn::Type) -> syn::Result<TokenStream> {
    let base = type_link(base)?;
    Ok(quote! {
        ::aosabi::descriptor::HeritageInfo {
            kind: ::aosabi::descriptor::HeritageKind::Implements,
            base: #base,
        }
    })
}

/// The name of an identifier with a raw prefix removed and nothing else
/// changed, the field name policy of `docs/schema-inference.md` section 5.3,
/// which the glue of `aosabi-codegen` reads by the same rule.
pub(crate) use aosabi_codegen::unraw;

/// Returns the `Visibility` literal of `vis`.
///
/// `pub` and every restricted `pub(...)` are public, and a member with no
/// visibility keyword is private.
pub(crate) fn visibility(vis: &Visibility) -> TokenStream {
    match vis {
        Visibility::Inherited => quote!(::aosabi::descriptor::Visibility::Private),
        Visibility::Public(_) | Visibility::Restricted(_) => {
            quote!(::aosabi::descriptor::Visibility::Public)
        }
    }
}

/// Returns `generics` with the bound `Described` on every type parameter.
///
/// # Errors
///
/// Returns the refusal of a lifetime and of a const parameter, at the
/// parameter.
pub(crate) fn described_generics(generics: &Generics) -> syn::Result<Generics> {
    let mut generics = generics.clone();
    for param in &mut generics.params {
        match param {
            GenericParam::Type(param) => {
                param
                    .bounds
                    .push(parse_quote!(::aosabi::descriptor::Described));
            }
            GenericParam::Lifetime(_) | GenericParam::Const(_) => {
                return Err(refusal(param.span(), LIFETIME_OR_CONST_PARAMETER));
            }
        }
    }
    Ok(generics)
}

/// Returns the `impl Described` a refused derive emits beside its error.
///
/// The build stops at the refusal, so the description `TypeExpr::Never` of
/// the refused type `input` is never read. It keeps a field, a parameter or a
/// return of that type from adding the error of the missing `Described` to
/// the refusal. A type with parameters gets none, because a parameter may be
/// what the derive refuses.
pub(crate) fn refused_described(input: &DeriveInput) -> TokenStream {
    if !input.generics.params.is_empty() {
        return TokenStream::new();
    }
    let ident = &input.ident;
    quote! {
        impl ::aosabi::descriptor::Described for #ident {
            const TYPE: ::aosabi::descriptor::TypeExpr = ::aosabi::descriptor::TypeExpr::Never;
        }
    }
}

/// Returns one `TypeLink` expression per type parameter of `generics`.
fn type_arguments(generics: &Generics) -> Vec<TokenStream> {
    generics
        .type_params()
        .map(|param| {
            let ident = &param.ident;
            quote!(::aosabi::descriptor::TypeLink::of::<#ident>())
        })
        .collect()
}

/// Returns the `SourceKey` literal, which the compiler fills in at the macro.
pub(crate) fn source_key() -> TokenStream {
    quote! {
        ::aosabi::descriptor::SourceKey {
            file: ::core::file!(),
            line: ::core::line!(),
            column: ::core::column!(),
        }
    }
}

/// Returns the `Option` expression over the tokens of `value`.
fn option(value: Option<&TokenStream>) -> TokenStream {
    value.map_or_else(
        || quote!(::core::option::Option::None),
        |value| quote!(::core::option::Option::Some(#value)),
    )
}

/// Returns the `Option<&'static str>` expression over `value`.
fn option_str(value: Option<&str>) -> TokenStream {
    option(value.map(|text| quote!(#text)).as_ref())
}
