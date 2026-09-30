// SPDX-License-Identifier: MIT
/*
 * aosabi-codegen: the run time glue of the source path, which the builder
 * writes for the types of a plain crate.
 *
 * No analogue on the reference side, where a class instance is an object of
 * the executor already and a method is a function already. The generated code
 * implements the traits of aosabi-core, module glue, as the glue of the
 * derive path does.
 */

//! The run time glue of a plain crate.
//!
//! The builder reads the source of the crate, and for every root, type and
//! inherent block its roots reach it writes the same glue the derive and the
//! resolvers attribute write on the derive path, into one generated module
//! that the generated copy of the crate adds to its crate root. The module
//! stands beside the items of the crate, so its implementations see every type
//! the source states, and the application writes nothing.
//!
//! | Item | Glue |
//! | --- | --- |
//! | [`PlainObject`] | `impl ObjectLike`, `impl SharedObject`, `impl FieldValue`, `impl IntoValue` and `impl ResolverMethods` over the public `&self` methods of its blocks, a method that answers the payload of a mutation run under the mutation wrapper, the `__typename` of a use a variant of an enum holds, with a public field its static shape and its member writer, and `impl FromArgument` where an argument reaches it |
//! | [`PlainEnum`] | `impl IntoValue`, `impl FieldValue` and `impl FromArgument` over the value text of each variant |
//! | [`PlainUnion`] | `impl IntoValue` and `impl FieldValue` that answer the payload of each variant, whose struct states the GraphQL name of its type as `__typename` |
//! | [`PlainEntry`] | the root object of every registered part and the function `entry`, which hands them to the app the entry answers |
//!
//! The glue differs from the glue of the derive path in five ways, four of
//! them because the crate compiles on its own without it. A member answers
//! under its GraphQL name, which the builder names by the rule of the source
//! path. The glue writes no type it can leave to inference, so it never names
//! a type in the context of another module. It writes no `From` of a payload,
//! since the crate converts its own values. And the method table is the
//! implementation of `ResolverMethods` itself, since no block of the crate
//! carries the inherent items a resolvers block emits. The glue names a type
//! of the crate by its path from `crate`, as the builder writes it in the
//! target of an implementation, and every other item by a path that opens with
//! `::core::` or `::aosabi::`, never `::std::` or a name of the prelude of the
//! standard library. The string and the list of `own_keys` and every type of
//! `alloc` an argument of a generic use names go through the re-exports of the
//! facade in `__private`, so a crate without the standard library takes the
//! same glue.

use proc_macro2::{
    Span,
    TokenStream, //
};
use quote::{
    format_ident,
    quote, //
};
use syn::{
    Generics,
    Ident, //
};

use crate::glue::{
    Method,
    Param,
    ShapeMember,
    TYPENAME_KEY,
    enum_from_argument_impl,
    enum_into_value_impl,
    glue,
    method_arm,
    object_into_value_impl,
    shape_methods,
    shared_object_impl,
    union_impls, //
};

/// The path of an item of the standard library the facade re-exports for
/// the glue, so a crate without the standard library names it too.
fn facade_std(item: &str) -> TokenStream {
    let item = format_ident!("{item}");
    quote!(::aosabi::__private::#item)
}

/// One named field of a struct of a plain crate.
pub struct PlainField {
    /// The identifier of the field.
    pub ident: Ident,
    /// The key it answers under, its GraphQL name, or `None` for a field the
    /// API does not see, which answers under no key.
    pub key: Option<String>,
}

/// A struct of a plain crate, one use of it.
pub struct PlainObject {
    /// The type the glue implements the traits for, the path of the struct
    /// from the crate root with the arguments of the use.
    pub target: TokenStream,
    /// The generics of the implementations, empty for a struct with no type
    /// parameter and for a use whose arguments are concrete.
    pub generics: Generics,
    /// The name of the struct, which the `TypeError` of an input names.
    pub name: String,
    /// The GraphQL name the object states as its `__typename`, for a use of
    /// a struct a variant of an enum holds, which the type resolver of a
    /// union or of its interface reads first, or `None`.
    pub typename: Option<String>,
    /// The named fields of the struct, empty for a unit struct.
    pub fields: Vec<PlainField>,
    /// The public `&self` methods of every inherent block of the use, in the
    /// order of the walk.
    pub methods: Vec<Method>,
    /// Whether an argument reaches the struct, which then reads an input
    /// object into it.
    pub input: bool,
}

/// An enum of a plain crate whose variants carry no data.
pub struct PlainEnum {
    /// The type the glue implements the traits for.
    pub target: TokenStream,
    /// The generics of the implementations.
    pub generics: Generics,
    /// The identifier of the enum, which the `TypeError` of an argument names.
    pub ident: Ident,
    /// Each variant with its value text, its GraphQL name.
    pub values: Vec<(Ident, String)>,
}

/// An enum of a plain crate whose variants each carry one payload.
pub struct PlainUnion {
    /// The type the glue implements the traits for.
    pub target: TokenStream,
    /// The generics of the implementations, empty for a concrete use.
    pub generics: Generics,
    /// The identifiers of the variants.
    pub variants: Vec<Ident>,
}

/// The entry of a plain crate and the parts of its roots.
pub struct PlainEntry {
    /// The path of the entry from the crate root, `crate::app`.
    pub function: TokenStream,
    /// Whether the entry answers a `Result` of an app.
    pub result: bool,
    /// The type of every registered part, each once.
    pub parts: Vec<TokenStream>,
}

impl Method {
    /// Returns the method `ident` of a block of a plain crate, which answers
    /// under `key`, is an `async fn` when `is_async` says so and takes one
    /// argument per key of `params`, whose type the call gives.
    #[must_use]
    pub fn plain(ident: Ident, key: String, is_async: bool, params: Vec<String>) -> Self {
        Self {
            ident,
            key,
            is_async,
            params: params
                .into_iter()
                .map(|name| Param {
                    name,
                    ty: None,
                    default: None,
                })
                .collect(),
            decorators: Vec::new(),
            stream: false,
            payload: false,
        }
    }

    /// Returns the method as one that answers a stream when `stream` says
    /// so, `impl Stream<Item = T>` or a `Result` of one, whose glue calls it
    /// through `complete_stream`.
    #[must_use]
    pub fn streaming(mut self, stream: bool) -> Self {
        self.stream = stream;
        self
    }

    /// Returns the method as one that answers the payload of a mutation when
    /// `payload` says so, `MutationPayload<T>` or a `Result` of one, whose
    /// glue runs it under the mutation wrapper, as the derive path runs an
    /// entry of `graphql!` under `mutation(...)`.
    #[must_use]
    pub fn mutation_payload(mut self, payload: bool) -> Self {
        self.payload = payload;
        self
    }
}

/// Returns the identifier `name` in the source, raw where the name is a
/// keyword of Rust.
#[must_use]
pub fn ident(name: &str) -> Ident {
    const KEYWORDS: &[&str] = &[
        "abstract", "as", "async", "await", "become", "box", "break", "const", "continue", "do",
        "dyn", "else", "enum", "extern", "false", "final", "fn", "for", "gen", "if", "impl", "in",
        "let", "loop", "macro", "match", "mod", "move", "mut", "override", "priv", "pub", "ref",
        "return", "static", "struct", "trait", "true", "try", "type", "typeof", "unsafe",
        "unsized", "use", "virtual", "where", "while", "yield",
    ];
    if KEYWORDS.contains(&name) {
        Ident::new_raw(name, Span::call_site())
    } else {
        Ident::new(name, Span::call_site())
    }
}

/// Returns the glue of the struct `object`.
#[must_use]
pub fn object(object: &PlainObject) -> TokenStream {
    let PlainObject {
        target, generics, ..
    } = object;
    let public: Vec<(&Ident, String)> = object
        .fields
        .iter()
        .filter_map(|field| field.key.clone().map(|key| (&field.ident, key)))
        .collect();
    let keys: Vec<&str> = object
        .typename
        .as_ref()
        .map(|_| TYPENAME_KEY)
        .into_iter()
        .chain(public.iter().map(|(_, key)| key.as_str()))
        .collect();
    // The public fields are the data members of the static shape in the
    // order of the source, and what each can hold is the `MEMBER` of the
    // type the read of the field infers.
    let member_of = glue("member_of");
    let shape: Vec<ShapeMember<'_>> = public
        .iter()
        .map(|(field, key)| ShapeMember {
            field,
            key: key.clone(),
            kind: quote!(#member_of(|object: &Self| &object.#field)),
        })
        .collect();
    let shape = (!shape.is_empty()).then_some(shape.as_slice());
    let object_like = object_like_impl(target, generics, &keys, shape.is_some());
    let shared_object = shared_object_impl(
        target,
        generics,
        object.typename.as_deref(),
        &public,
        None,
        None,
        shape,
    );
    let into_value = object_into_value_impl(target, generics, None);
    let methods = methods_impl(target, generics, &object.methods);
    let from_argument = object
        .input
        .then(|| from_argument_impl(target, generics, &object.name, &object.fields));
    quote! {
        #object_like
        #shared_object
        #into_value
        #methods
        #from_argument
    }
}

/// Returns `impl ObjectLike` of the struct `target`, whose properties are
/// those of its `impl SharedObject` over the whole object.
///
/// The keys list the typename of a use a variant holds, then the public
/// fields by their keys, then the methods, each in the order of the source.
/// A struct with a static shape, `has_shape`, answers the shape and writes
/// its data members through its `impl SharedObject`, as the derive path
/// writes them.
fn object_like_impl(
    target: &TokenStream,
    generics: &Generics,
    keys: &[&str],
    has_shape: bool,
) -> TokenStream {
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let (object_like, shared_object, shared) =
        (glue("ObjectLike"), glue("SharedObject"), glue("Shared"));
    let (value, rc, methods) = (glue("Value"), glue("Rc"), glue("ResolverMethods"));
    let (vec, string) = (facade_std("Vec"), facade_std("String"));
    let shape = has_shape.then(shape_methods);
    quote! {
        #[automatically_derived]
        impl #impl_generics #object_like for #target #where_clause {
            fn get(self: #rc<Self>, key: &str) -> #value {
                <Self as #shared_object>::get_shared(&#shared::from(self), key)
            }

            fn own_keys(&self) -> #vec<#string> {
                const FIELDS: &[&str] = &[#(#keys),*];
                FIELDS
                    .iter()
                    .chain(<Self as #methods>::NAMES)
                    .map(|name| #string::from(*name))
                    .collect()
            }

            #shape
        }
    }
}

/// Returns `impl ResolverMethods` of the struct `target`, the method table of
/// the public `&self` methods of its blocks.
fn methods_impl(target: &TokenStream, generics: &Generics, methods: &[Method]) -> TokenStream {
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let (trait_path, shared, function) =
        (glue("ResolverMethods"), glue("Shared"), glue("Function"));
    let keys = methods.iter().map(|method| method.key.as_str());
    let arms = methods.iter().map(method_arm);
    quote! {
        #[automatically_derived]
        impl #impl_generics #trait_path for #target #where_clause {
            const NAMES: &'static [&'static str] = &[#(#keys),*];

            fn method(this: &#shared<Self>, key: &str) -> ::core::option::Option<#function> {
                match key {
                    #(#arms)*
                    _ => ::core::option::Option::None,
                }
            }
        }
    }
}

/// Returns `impl FromArgument` of the struct `target` named `name` with the
/// named `fields`, which reads an input object into the struct.
///
/// A public field reads the property of its key through the `FromArgument`
/// of its type, and a property the object lacks reads as `undefined`, which
/// is what a TypeScript resolver sees for an input field the request leaves
/// out. A field the API does not see is no input field, so it takes its
/// `Default`. The type of each field is the one the struct literal gives it.
fn from_argument_impl(
    target: &TokenStream,
    generics: &Generics,
    name: &str,
    fields: &[PlainField],
) -> TokenStream {
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let (from_argument, mismatch, value) = (
        glue("FromArgument"),
        glue("ArgumentMismatch"),
        glue("Value"),
    );
    let (object_like, rc) = (glue("ObjectLike"), glue("Rc"));
    let expected = format!("an input object of the type `{name}`");
    let members = fields.iter().map(|field| {
        let member = &field.ident;
        field.key.as_ref().map_or_else(
            || quote!(#member: ::core::default::Default::default()),
            |key| {
                quote! {
                    #member: #from_argument::from_argument(&#object_like::get(#rc::clone(object), #key))?
                }
            },
        )
    });
    quote! {
        #[automatically_derived]
        impl #impl_generics #from_argument for #target #where_clause {
            fn from_argument(value: &#value) -> ::core::result::Result<Self, #mismatch> {
                let #value::Object(object) = value else {
                    return ::core::result::Result::Err(#mismatch { expected: #expected });
                };
                ::core::result::Result::Ok(Self {
                    #(#members,)*
                })
            }
        }
    }
}

/// Returns the glue of the enum `enumeration`, whose variants carry no data.
#[must_use]
pub fn enumeration(enumeration: &PlainEnum) -> TokenStream {
    let values: Vec<(&Ident, String)> = enumeration
        .values
        .iter()
        .map(|(variant, value)| (variant, value.clone()))
        .collect();
    let into_value = enum_into_value_impl(&enumeration.target, &enumeration.generics, &values);
    let from_argument = enum_from_argument_impl(
        &enumeration.target,
        &enumeration.ident,
        &enumeration.generics,
        &values,
    );
    quote!(#into_value #from_argument)
}

/// Returns the glue of the enum `union`, whose variants each carry one
/// payload.
#[must_use]
pub fn union(union: &PlainUnion) -> TokenStream {
    let variants: Vec<&Ident> = union.variants.iter().collect();
    union_impls(&union.target, &union.generics, &union.generics, &variants)
}

/// Returns the glue of the entry `entry`: the root object of each registered
/// part and the function `entry`, which hands them to the app the entry of
/// the crate answers.
///
/// The app keeps every part once and lends it to each request, so the root
/// object of a part is the object of its struct over the value the app keeps.
/// The parts reach the app as values of `Any`, and the glue tells each by its
/// type, so the builder of the facade needs no bound beyond `'static` on a
/// part. An entry that answers a `Result` keeps its error, which the boot
/// prints as the cause of a configuration that throws at load, so the function
/// `entry` answers the `Result` with the app inside it, and the error of the
/// crate is a type with `Display` or the thrown value of the `Result` of the
/// facade, which `IntoApp` takes.
#[must_use]
pub fn entry(entry: &PlainEntry) -> TokenStream {
    let (rc, value) = (glue("Rc"), glue("Value"));
    let parts = entry.parts.iter().map(|part| {
        quote! {
            if let ::core::result::Result::Ok(part) = #rc::clone(part).downcast::<#part>() {
                return ::core::option::Option::Some(#value::Object(part));
            }
        }
    });
    let function = &entry.function;
    let (answer, app) = if entry.result {
        (
            quote!(impl ::aosabi::IntoEntry),
            quote! {
                ::core::result::Result::map(#function(), |app| {
                    ::aosabi::IntoApp::into_app_with(app, part_object)
                })
            },
        )
    } else {
        (
            quote!(impl ::aosabi::IntoApp),
            quote!(::aosabi::IntoApp::into_app_with(#function(), part_object)),
        )
    };
    quote! {
        /// Returns the root object of the registered part `part`, the object
        /// of its struct over the value the app keeps, or `None` for a value
        /// of no registered type.
        fn part_object(
            part: &#rc<dyn ::core::any::Any>,
        ) -> ::core::option::Option<#value> {
            #(#parts)*
            ::core::option::Option::None
        }

        /// Returns the app of the entry of the crate with the root object of
        /// every registered part, which the generated packages serve.
        pub fn entry() -> #answer {
            #app
        }
    }
}

/// Returns the whole generated module of a plain crate: the glue of its
/// structs, enums and payload enums and the glue of its entry.
#[must_use]
pub fn module(
    objects: &[PlainObject],
    enums: &[PlainEnum],
    unions: &[PlainUnion],
    root: &PlainEntry,
) -> TokenStream {
    let objects = objects.iter().map(object);
    let enums = enums.iter().map(enumeration);
    let unions = unions.iter().map(union);
    let entry = entry(root);
    quote! {
        #(#objects)*
        #(#enums)*
        #(#unions)*
        #entry
    }
}
