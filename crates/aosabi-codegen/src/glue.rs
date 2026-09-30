// SPDX-License-Identifier: MIT
/*
 * aosabi-codegen: the run time glue of the derive path, which the three
 * macros of aosabi-macros emit.
 *
 * No analogue on the reference side, where a class instance is an object of
 * the executor already and a method is a function already. The generated code
 * implements the traits of aosabi-core, module glue.
 */

//! The run time glue of the derive path, which the three macros of
//! `aosabi-macros` emit beside the descriptor.
//!
//! The descriptor of a type serves the schema build, and the glue serves the
//! executor. A derived struct answers its public fields and its methods as an
//! object of the executor, a method of a `#[aosabi::resolvers]` block becomes
//! a function over positional arguments, and an enum and a scalar convert to
//! and from a value of the executor.
//!
//! | Macro | Stands on | Glue |
//! | --- | --- | --- |
//! | `#[derive(Aosabi)]` | a struct | `impl ObjectLike`, `impl SharedObject`, `impl FieldValue`, `impl IntoValue`, and `impl ResolverMethods` over the method table of its block, and with no embedded base and no kept remote object its static shape and its member writer |
//! | `#[derive(Aosabi)]` | a struct with named fields | `impl FromArgument`, which reads an input object into the struct |
//! | `#[derive(Aosabi)]` | a struct with a field under `#[aosabi(spread)]` | the impls of a struct, whose object answers a key through each spread as the module `spread` writes it, and `impl FromDelegated<Rec>` for the type `Rec` of its first spread field |
//! | `#[derive(Aosabi)]` | a struct under `#[aosabi(remote)]` | the impls of a struct, whose object, `IntoValue` and `FieldValue` answer the remote object its slot keeps, and `impl FromDelegated<Self>` |
//! | `#[derive(Aosabi)]` | an enum whose variants carry no data | `impl IntoValue`, `impl FieldValue`, `impl FromArgument` and `impl FromDelegated<Self>` over the value text |
//! | `#[derive(Aosabi)]` | an enum whose variants carry one payload | `impl IntoValue` and `impl FieldValue` that delegate to the payload, `impl From` per payload type that no other can equal, and under `#[aosabi(remote)]` `impl FromDelegated<Self>` by the `__typename` of the value |
//! | `#[aosabi::resolvers]` | an `impl` block | the method table and one `EntryFunction` per static |
//! | `#[aosabi::scalar]` | a newtype | `impl IntoValue`, `impl FieldValue` and `impl FromArgument` over the one field |
//! | `graphql!` and `#[aosabi::resolver]` | an entry and a free function | the function of the executor of each, through [`call_body`], [`decorated_body`] and [`mutation_body`] |
//!
//! A public field is read through a shared reference and converted by
//! `FieldValue`, so a described struct needs no `Clone`, and a field that is
//! a described struct becomes an object that reaches it through the object
//! that holds it. The static shape of a struct names its public fields as
//! data members in declaration order, each with what its type can hold, and
//! its member writer writes the data members a selection reads by their
//! slots, one jump table per field in code that knows the struct, a field of
//! a string type as the `&str` it holds.
//!
//! Every generated path starts with `::aosabi::__private::glue::`. A field, a
//! parameter or a return whose type the glue cannot carry refuses at compile
//! time with the message of the trait it lacks, which is the refusal through
//! trait bounds of `docs/schema-inference.md` section 13.

use proc_macro2::TokenStream;
use quote::{
    format_ident,
    quote, //
};
use syn::{
    Attribute,
    Expr,
    Field,
    Fields,
    FnArg,
    GenericParam,
    Generics,
    Ident,
    ImplItem,
    ImplItemFn,
    ItemImpl,
    ItemStruct,
    Pat,
    Type,
    Variant,
    Visibility,
    parse_quote, //
};

use crate::{
    TypeItem,
    lints,
    remote,
    spread::{
        self,
        Spread, //
    },
    unify::can_equal,
    unraw, //
};

/// The key a type answers its typename under, the discriminator property of a
/// TypeScript class.
pub const TYPENAME_KEY: &str = "__typename";

/// Returns the path of `item` in the glue module of the facade.
#[must_use]
pub fn glue(item: &str) -> TokenStream {
    let item = format_ident!("{item}");
    quote!(::aosabi::__private::glue::#item)
}

/// Returns `generics` with `bounds` on every type parameter.
///
/// A lifetime and a const parameter stay as they are, because the descriptor
/// of the same item refuses them with their own message.
pub(crate) fn bounded_generics(generics: &Generics, bounds: &[TokenStream]) -> Generics {
    let mut generics = generics.clone();
    for param in &mut generics.params {
        if let GenericParam::Type(param) = param {
            for bound in bounds {
                param.bounds.push(parse_quote!(#bound));
            }
        }
    }
    generics
}

/// Returns the bounds a type parameter of a derived struct carries.
///
/// A field of the parameter type is read through a shared reference and
/// converted, a method may return it, and the object sits behind a shared
/// pointer without a lifetime.
pub(crate) fn object_bounds() -> [TokenStream; 3] {
    [glue("IntoValue"), glue("FieldValue"), quote!('static)]
}

/// Returns the glue of the struct `item` with `fields`, whose typename is
/// `typename`, whose field `embedded` is its embedded base, whose fields
/// `spreads` stand under `#[aosabi(spread)]` and whose field `kept` is the
/// slot of the remote object of a struct under `#[aosabi(remote)]`.
///
/// The slot is no property, the typename of a remote struct is its identifier
/// unless the type states one, and its object answers the kept remote object
/// where there is one. Every key is the identifier of its member with a raw
/// prefix removed, the names of the derive path.
#[must_use]
pub fn object(
    item: &TypeItem<'_>,
    fields: &Fields,
    typename: Option<&str>,
    embedded: Option<&Field>,
    spreads: &[Spread<'_>],
    kept: Option<&Ident>,
) -> TokenStream {
    let typename = typename
        .map(str::to_owned)
        .or_else(|| kept.map(|_| unraw(item.ident)));
    let base = embedded.and_then(|field| field.ident.as_ref());
    let generics = bounded_generics(item.generics, &object_bounds());
    let target = {
        let ident = item.ident;
        let (_, type_generics, _) = generics.split_for_impl();
        quote!(#ident #type_generics)
    };
    let object = if spreads.is_empty() {
        let public: Vec<(&Ident, String)> = public_fields(fields)
            .into_iter()
            .filter(|field| Some(*field) != base && Some(*field) != kept)
            .map(|field| (field, unraw(field)))
            .collect();
        let kept = kept.map(|slot| Kept {
            slot,
            fields: public
                .iter()
                .filter_map(|(field, _)| field_type(fields, field).map(|ty| (*field, ty)))
                .collect(),
        });
        // A struct with no embedded base and no kept remote object answers
        // its public fields and its methods alone, so they are its static
        // shape, its fields its data members in declaration order.
        let shape: Option<Vec<ShapeMember<'_>>> = (base.is_none() && kept.is_none())
            .then(|| {
                public
                    .iter()
                    .filter_map(|(field, key)| {
                        field_type(fields, field)
                            .map(|ty| ShapeMember::of_type(field, key.clone(), ty))
                    })
                    .collect::<Vec<_>>()
            })
            .filter(|members| !members.is_empty());
        let object_like = object_like_impl(
            &target,
            &generics,
            &public,
            typename.is_some(),
            base,
            kept.as_ref(),
            shape.is_some(),
        );
        let shared_object = shared_object_impl(
            &target,
            &generics,
            typename.as_deref(),
            &public,
            base,
            kept.as_ref(),
            shape.as_deref(),
        );
        let changed = kept
            .as_ref()
            .map(|kept| changed_impl(&target, &generics, kept));
        quote!(#object_like #shared_object #changed)
    } else {
        spread::object_impls(&target, &generics, typename.as_deref(), fields, spreads)
    };
    let into_value = object_into_value_impl(&target, &generics, kept);
    let methods = resolver_methods_impl(&target, &generics);
    let from_argument = match fields {
        Fields::Named(_) => {
            let inherited: Vec<&Ident> = base
                .into_iter()
                .chain(spreads.iter().map(|spread| spread.ident))
                .collect();
            let target = {
                let ident = item.ident;
                let generics = bounded_generics(item.generics, &[glue("FromArgument")]);
                let (_, type_generics, _) = generics.split_for_impl();
                quote!(#ident #type_generics)
            };
            object_from_argument_impl(
                &target,
                item.ident,
                item.generics,
                fields,
                &inherited,
                &unraw,
            )
        }
        Fields::Unit | Fields::Unnamed(_) => TokenStream::new(),
    };
    let from_delegated = match kept {
        Some(slot) => remote::struct_from_delegated(item, fields, slot),
        None if !spreads.is_empty() => remote::spread_from_delegated(item, fields, spreads),
        None => TokenStream::new(),
    };
    quote!(#object #into_value #methods #from_argument #from_delegated)
}

/// Returns the public named fields of `fields`, in declaration order.
///
/// A private field is no property the executor reads, because the inference
/// leaves it out of the schema.
pub(crate) fn public_fields(fields: &Fields) -> Vec<&Ident> {
    fields
        .iter()
        .filter(|field| !matches!(field.vis, Visibility::Inherited))
        .filter_map(|field| field.ident.as_ref())
        .collect()
}

/// Returns the type of the named field `field` of `fields`.
fn field_type<'a>(fields: &'a Fields, field: &Ident) -> Option<&'a Type> {
    fields
        .iter()
        .find(|candidate| candidate.ident.as_ref() == Some(field))
        .map(|candidate| &candidate.ty)
}

/// The slot of the remote object of a struct under `#[aosabi(remote)]`, with
/// the public fields its object answers and their types.
pub(crate) struct Kept<'a> {
    /// The field `__typename: TypeName` that keeps the remote object.
    slot: &'a Ident,
    /// The public fields, each with its type, in declaration order.
    fields: Vec<(&'a Ident, &'a Type)>,
}

/// Returns the expression that asks whether the field `field` of `owner`
/// still holds what the read made of the member `member`.
///
/// The question is `SameMember` of `aosabi::remote::delegated` over the
/// field type through a reference, so a field whose type has no such reader
/// reads as unchanged and the record answers the kept member, and the `use`
/// line of [`same_member_readers`] stands in the body that asks.
fn same_member(owner: &TokenStream, field: &Ident, ty: &Type, member: &TokenStream) -> TokenStream {
    quote! {
        (&&::core::marker::PhantomData::<#ty>).aosabi_same_member(&#owner.#field, #member)
    }
}

/// Returns the `use` line of the two readers [`same_member`] asks through.
fn same_member_readers() -> TokenStream {
    quote! {
        use ::aosabi::remote::delegated::{
            SameMemberFallback as _,
            SameMemberRead as _, //
        };
    }
}

/// Returns the inherent `aosabi_changed` and `impl SameMember` of the struct
/// `ident` under `#[aosabi(remote)]`, whose slot and fields `kept` names.
///
/// A field differs from the member of the kept object it was read from once a
/// patch or a resolver assigns it, and then the object of the record answers
/// it from the field, as the merge of the reference answers what a patch
/// assigned on the copy it was handed. A record is the same as a member of a
/// delegated object when it keeps that very object and no field of it
/// changed.
fn changed_impl(target: &TokenStream, generics: &Generics, kept: &Kept<'_>) -> TokenStream {
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let value = glue("Value");
    let slot = kept.slot;
    let body = if kept.fields.is_empty() {
        quote!(false)
    } else {
        let readers = same_member_readers();
        let owner = quote!(self);
        let checks = kept.fields.iter().map(|(field, ty)| {
            let name = unraw(field);
            let member = quote!(&self.#slot.member(#name).unwrap_or(#value::Undefined));
            let same = same_member(&owner, field, ty, &member);
            quote!(!#same)
        });
        quote! {
            #readers
            if self.#slot.data().is_none() {
                return false;
            }
            #(#checks)||*
        }
    };
    let kept_is = quote!(::aosabi::remote::delegated::kept_is);
    quote! {
        #[automatically_derived]
        impl #impl_generics #target #where_clause {
            /// Returns whether a field differs from the member of the kept object it was read from.
            #[doc(hidden)]
            fn aosabi_changed(&self) -> bool {
                #body
            }
        }

        #[automatically_derived]
        impl #impl_generics ::aosabi::remote::delegated::SameMember for #target #where_clause {
            fn same_member(&self, member: &#value) -> bool {
                #kept_is(self.#slot.data(), member) && !self.aosabi_changed()
            }
        }
    }
}

/// Returns `impl ObjectLike` of the struct `target`, whose properties are
/// those of its `impl SharedObject` over the whole object.
///
/// The keys list the typename first, then the public fields by their keys,
/// then the methods of the resolvers block, each in declaration order, then
/// each key of the embedded base the struct lacks. A struct with a static
/// shape, `has_shape`, answers the shape and writes its data members through
/// its `impl SharedObject`.
fn object_like_impl(
    target: &TokenStream,
    generics: &Generics,
    fields: &[(&Ident, String)],
    has_typename: bool,
    base: Option<&Ident>,
    kept: Option<&Kept<'_>>,
    has_shape: bool,
) -> TokenStream {
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let (object_like, shared_object, shared) =
        (glue("ObjectLike"), glue("SharedObject"), glue("Shared"));
    let (value, rc, methods) = (glue("Value"), glue("Rc"), glue("ResolverMethods"));
    let typename_key = has_typename.then_some(TYPENAME_KEY);
    let keys = typename_key
        .into_iter()
        .chain(fields.iter().map(|(_, key)| key.as_str()));
    let base_keys = base.map(|base| {
        let member_keys = quote!(::aosabi::__private::glue::embedded::member_keys);
        quote!(#member_keys(&self.#base, &mut keys);)
    });
    // A field a patch assigns where the kept object has no member of its
    // name follows the keys of the object, as an assignment of a new key
    // appends it to the keys of an object.
    let kept_keys = kept.map(|kept| {
        let slot = kept.slot;
        let readers = (!kept.fields.is_empty()).then(same_member_readers);
        let owner = quote!(self);
        let undefined = quote!(&#value::Undefined);
        let appends = kept.fields.iter().map(|(field, ty)| {
            let name = unraw(field);
            let same = same_member(&owner, field, ty, &undefined);
            quote! {
                if !keys.iter().any(|known| known == #name) && !#same {
                    keys.push(::std::string::String::from(#name));
                }
            }
        });
        quote! {
            #[allow(unused_mut, reason = "a record with no field appends no key")]
            if let ::core::option::Option::Some(mut keys) = self.#slot.keys() {
                #readers
                #(#appends)*
                return keys;
            }
        }
    });
    let shape = has_shape.then(shape_methods);
    quote! {
        #[automatically_derived]
        impl #impl_generics #object_like for #target #where_clause {
            fn get(self: #rc<Self>, key: &str) -> #value {
                <Self as #shared_object>::get_shared(&#shared::from(self), key)
            }

            fn own_keys(&self) -> ::std::vec::Vec<::std::string::String> {
                #kept_keys
                const FIELDS: &[&str] = &[#(#keys),*];
                let mut keys: ::std::vec::Vec<::std::string::String> = FIELDS
                    .iter()
                    .chain(<Self as #methods>::NAMES)
                    .map(|name| ::std::string::String::from(*name))
                    .collect();
                #base_keys
                keys
            }

            #shape
        }
    }
}

/// Returns `shape` and `write_members` of the `impl ObjectLike` of a struct
/// with a static shape, which answer the shape and write the data members
/// through the `impl SharedObject` of the struct, the same for the derive
/// path and the source path.
pub(crate) fn shape_methods() -> TokenStream {
    let (shared_object, shared, rc) = (glue("SharedObject"), glue("Shared"), glue("Rc"));
    let (shape, member_slot, member_sink) = (glue("Shape"), glue("MemberSlot"), glue("MemberSink"));
    quote! {
        fn shape(&self) -> ::core::option::Option<&'static #shape> {
            <Self as #shared_object>::struct_shape()
        }

        fn write_members(
            self: #rc<Self>,
            slots: &[#member_slot],
            sink: &mut #member_sink<'_>,
        ) {
            <Self as #shared_object>::write_shared(&#shared::from(self), slots, sink);
        }
    }
}

/// One data member of the static shape of a struct: its field, the key it
/// answers under and what it can hold.
pub(crate) struct ShapeMember<'a> {
    /// The public field of the struct.
    pub(crate) field: &'a Ident,
    /// The key the field answers under, the name of the member.
    pub(crate) key: String,
    /// The expression of the `MemberKind` of what the field can hold, the
    /// `MEMBER` of the `impl FieldValue` of its type.
    pub(crate) kind: TokenStream,
}

impl<'a> ShapeMember<'a> {
    /// Returns the member of the field `field` of the type `ty`, which
    /// answers under `key`, as the derive path names the type of a field.
    fn of_type(field: &'a Ident, key: String, ty: &Type) -> Self {
        let field_value = glue("FieldValue");
        Self {
            field,
            key,
            kind: quote!(<#ty as #field_value>::MEMBER),
        }
    }
}

/// Returns `impl SharedObject` and `impl FieldValue` of the struct `target`
/// with no spread field, whose public fields answer under their keys.
///
/// A property reads the typename, a public field through a shared reference
/// and a method bound to the struct `this` reaches, the whole object or a
/// part of another. A key the struct lacks reads through the embedded base,
/// as a member of a derived class reads through its prototype. As a field of
/// another struct, the struct is the object of its part.
///
/// A struct with the static shape `shape`, its public fields with what each
/// can hold, answers it and writes its data members by their slots (see
/// [`shape_items`]).
pub(crate) fn shared_object_impl(
    target: &TokenStream,
    generics: &Generics,
    typename: Option<&str>,
    fields: &[(&Ident, String)],
    base: Option<&Ident>,
    kept: Option<&Kept<'_>>,
    shape: Option<&[ShapeMember<'_>]>,
) -> TokenStream {
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let (shared_object, shared, field_value) =
        (glue("SharedObject"), glue("Shared"), glue("FieldValue"));
    let (value, into_value, methods) = (glue("Value"), glue("IntoValue"), glue("ResolverMethods"));
    let typename_arm =
        typename.map(|name| quote!(#TYPENAME_KEY => #into_value::into_value(#name),));
    let names: Vec<&str> = fields.iter().map(|(_, key)| key.as_str()).collect();
    let fields: Vec<&Ident> = fields.iter().map(|(field, _)| *field).collect();
    // A struct without public fields reads no field, and its methods reach
    // the struct when they are called. A remote struct reads its slot.
    let reach = (!fields.is_empty() || kept.is_some()).then(|| {
        quote! {
            let ::core::option::Option::Some(object) = this.reach() else {
                return #value::Undefined;
            };
        }
    });
    // A field that differs from the member it was read from answers from
    // the field, and every other key from the kept object.
    let kept_member = kept.map(|kept| {
        let slot = kept.slot;
        let owner = quote!(object);
        let member = quote!(&member);
        let arms = kept.fields.iter().map(|(field, ty)| {
            let name = unraw(field);
            let same = same_member(&owner, field, ty, &member);
            quote! {
                #name => {
                    if !#same {
                        return #field_value::field_value(
                            &object.#field,
                            &|| this.part(|object: &Self| ::core::option::Option::Some(&object.#field)),
                        );
                    }
                }
            }
        });
        let changed = (!kept.fields.is_empty()).then(|| {
            let readers = same_member_readers();
            quote! {
                #readers
                match key {
                    #(#arms)*
                    _ => {}
                }
            }
        });
        quote! {
            if let ::core::option::Option::Some(member) = object.#slot.member(key) {
                #changed
                return member;
            }
        }
    });
    let fallback = base.map_or_else(
        || quote!(|| #value::Undefined),
        |base| {
            let member = quote!(::aosabi::__private::glue::embedded::member);
            quote!(|| #member(this, |object: &Self| &object.#base, key))
        },
    );
    let field_value_impl = field_value_impl(target, generics, kept.map(|kept| kept.slot));
    let shape_items = shape.map(shape_items);
    quote! {
        #[automatically_derived]
        impl #impl_generics #shared_object for #target #where_clause {
            fn get_shared(this: &#shared<Self>, key: &str) -> #value {
                #reach
                #kept_member
                match key {
                    #typename_arm
                    #(#names => #field_value::field_value(
                        &object.#fields,
                        &|| this.part(|object: &Self| ::core::option::Option::Some(&object.#fields)),
                    ),)*
                    _ => <Self as #methods>::method(this, key)
                        .map_or_else(#fallback, #value::Function),
                }
            }

            #shape_items
        }

        #field_value_impl
    }
}

/// Returns `struct_shape` and `write_shared` of `impl SharedObject` of a
/// struct whose data members are `members`, its public fields with what each
/// can hold in declaration order.
///
/// The shape names each member by its key with what its type can hold, the
/// `MEMBER` of its `impl FieldValue`, and the methods of the resolvers block,
/// in one constant per instance of the struct. The list of the members is a block
/// of its own, so the constant holds it beside the shape for as long as the
/// program runs, where an argument of the constructor would be dropped. The
/// member writer reaches the struct once and writes the field of each slot
/// through its `write_member`, one jump table per field in code that knows
/// the struct, and calls `MemberSink::field` for a slot that names no
/// member. A part that reaches no struct writes nothing, and the executor
/// reads every field of it by name, which reads `undefined`.
fn shape_items(members: &[ShapeMember<'_>]) -> TokenStream {
    let (shape, member_shape, member_slot) =
        (glue("Shape"), glue("MemberShape"), glue("MemberSlot"));
    let (member_sink, field_value, shared) =
        (glue("MemberSink"), glue("FieldValue"), glue("Shared"));
    let methods = glue("ResolverMethods");
    let shapes = members
        .iter()
        .map(|ShapeMember { key, kind, .. }| quote!(#member_shape::new(#key, #kind)));
    let arms = members.iter().enumerate().filter_map(|(at, member)| {
        let field = member.field;
        let slot = u16::try_from(at).ok().filter(|slot| *slot != u16::MAX)?;
        let slot = proc_macro2::Literal::u16_unsuffixed(slot);
        Some(quote! {
            #slot => #field_value::write_member(
                &object.#field,
                sink,
                &|| this.part(|object: &Self| ::core::option::Option::Some(&object.#field)),
            ),
        })
    });
    quote! {
        fn struct_shape() -> ::core::option::Option<&'static #shape> {
            ::core::option::Option::Some(const {
                &#shape::new(const { &[#(#shapes),*] }, <Self as #methods>::NAMES)
            })
        }

        fn write_shared(
            this: &#shared<Self>,
            slots: &[#member_slot],
            sink: &mut #member_sink<'_>,
        ) {
            let ::core::option::Option::Some(object) = this.reach() else {
                return;
            };
            for &slot in slots {
                match slot {
                    #(#arms)*
                    _ => sink.field(),
                }
            }
        }
    }
}

/// Returns `impl FieldValue` of the struct `target`, which as a field of
/// another struct is the object of its part, or the remote object its slot
/// `kept` keeps.
///
/// What a field of the struct holds is an object of the shape of the
/// struct, which is none for a struct with no static shape.
pub(crate) fn field_value_impl(
    target: &TokenStream,
    generics: &Generics,
    kept: Option<&Ident>,
) -> TokenStream {
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let (field_value, shared, value, rc) = (
        glue("FieldValue"),
        glue("Shared"),
        glue("Value"),
        glue("Rc"),
    );
    let (member_kind, shared_object) = (glue("MemberKind"), glue("SharedObject"));
    let kept_value = kept_value(kept);
    quote! {
        #[automatically_derived]
        impl #impl_generics #field_value for #target #where_clause {
            const MEMBER: #member_kind =
                #member_kind::Object(<Self as #shared_object>::struct_shape);

            fn field_value(&self, part: &dyn ::core::ops::Fn() -> #shared<Self>) -> #value {
                #kept_value
                #value::Object(#rc::new(part()))
            }
        }
    }
}

/// Returns the statement that answers the remote object the slot `kept`
/// of `self` keeps while no field differs from its member, nothing for a
/// struct with no slot.
///
/// A record a patch or a resolver changed is the object of the struct, which
/// answers each changed field from the field and every other key from the
/// kept object.
fn kept_value(kept: Option<&Ident>) -> Option<TokenStream> {
    kept.map(|kept| {
        quote! {
            if let ::core::option::Option::Some(value) = self.#kept.data() {
                if !self.aosabi_changed() {
                    return ::core::clone::Clone::clone(value);
                }
            }
        }
    })
}

/// Returns `impl IntoValue` of the struct `target`, which is the object
/// behind a shared pointer, or the remote object its slot `kept` keeps.
pub(crate) fn object_into_value_impl(
    target: &TokenStream,
    generics: &Generics,
    kept: Option<&Ident>,
) -> TokenStream {
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let (into_value, value, rc) = (glue("IntoValue"), glue("Value"), glue("Rc"));
    let kept_value = kept_value(kept);
    quote! {
        #[automatically_derived]
        impl #impl_generics #into_value for #target #where_clause {
            fn into_value(self) -> #value {
                #kept_value
                #value::Object(#rc::new(self))
            }
        }
    }
}

/// Returns `impl FromArgument` of the struct `target`, whose identifier is
/// `ident` and whose generics are `generics`, with the named `fields`, which
/// reads an input object into the struct, each public field from the
/// property `key_of` names.
///
/// A public field reads the property of its name through the `FromArgument`
/// of its type, and a property the object lacks reads as `undefined`, which
/// is what a TypeScript resolver sees for an input field the request leaves
/// out. A private field is no input field, so it takes its `Default`. The
/// fields of `inherited`, the embedded base and the spread fields, read
/// their members from the same object.
///
/// The implementation holds for every struct and bounds only the type
/// parameters. A bound on a field type would make the implementation of a
/// struct that reaches itself, directly or through another struct, depend on
/// itself, and the trait solver refuses such a cycle instead of proving it.
/// Each field therefore picks its reader by method resolution over
/// references, which prefers the reader of a type that can be read and falls
/// back to one that refuses. A public field whose type has no `FromArgument`
/// or a private field whose type has no `Default` makes the read throw the
/// `TypeError` of the struct when an argument reaches it, so a struct that
/// only stands in output position compiles as it did.
pub(crate) fn object_from_argument_impl(
    target: &TokenStream,
    ident: &Ident,
    generics: &Generics,
    fields: &Fields,
    inherited: &[&Ident],
    key_of: &dyn Fn(&Ident) -> String,
) -> TokenStream {
    let (from_argument, mismatch, value) = (
        glue("FromArgument"),
        glue("ArgumentMismatch"),
        glue("Value"),
    );
    let generics = bounded_generics(generics, std::slice::from_ref(&from_argument));
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let expected = format!("an input object of the type `{}`", unraw(ident));
    let is_inherited = |field: &Field| {
        field
            .ident
            .as_ref()
            .is_some_and(|ident| inherited.contains(&ident))
    };
    let has_private = fields
        .iter()
        .any(|field| matches!(field.vis, Visibility::Inherited) && !is_inherited(field));
    let has_public = !inherited.is_empty()
        || fields
            .iter()
            .any(|field| !matches!(field.vis, Visibility::Inherited));
    let readers = [
        has_public.then(field_readers),
        has_private.then(default_readers),
    ];
    let members = fields
        .iter()
        .filter_map(|field| input_member(field, &expected, inherited, key_of));
    quote! {
        const _: () = {
            #(#readers)*

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
        };
    }
}

/// Returns the initializer of the named `field` in the reader of an input
/// object, which reads the property `key_of` names, or `None` for a field
/// with no name.
///
/// The receiver is the marker of the field type behind two references, so
/// method resolution tries the reader that needs the trait first and the
/// fallback after one dereference.
fn input_member(
    field: &Field,
    expected: &str,
    inherited: &[&Ident],
    key_of: &dyn Fn(&Ident) -> String,
) -> Option<TokenStream> {
    let member = field.ident.as_ref()?;
    let ty = &field.ty;
    let marker = quote!((&&::core::marker::PhantomData::<#ty>));
    // The embedded base and a spread read their members from the same
    // object, as the input object of a derived class carries the inherited
    // fields and an object literal the members it spreads.
    if inherited.contains(&member) {
        return Some(quote!(#member: #marker.aosabi_read_field(value, #expected)?));
    }
    if matches!(field.vis, Visibility::Inherited) {
        return Some(quote!(#member: #marker.aosabi_default_field(#expected)?));
    }
    let (object_like, rc) = (glue("ObjectLike"), glue("Rc"));
    let key = key_of(member);
    Some(quote! {
        #member: #marker.aosabi_read_field(
            &#object_like::get(#rc::clone(object), #key),
            #expected,
        )?
    })
}

/// Returns the two readers of a public field, the one of a type with
/// `FromArgument` and the fallback that refuses with the message of the
/// struct.
fn field_readers() -> TokenStream {
    let (from_argument, mismatch, value) = (
        glue("FromArgument"),
        glue("ArgumentMismatch"),
        glue("Value"),
    );
    let result = quote!(::core::result::Result<T, #mismatch>);
    quote! {
        trait AosabiReadField<T> {
            fn aosabi_read_field(&self, value: &#value, expected: &'static str) -> #result;
        }

        impl<T: #from_argument> AosabiReadField<T> for &::core::marker::PhantomData<T> {
            fn aosabi_read_field(&self, value: &#value, _: &'static str) -> #result {
                <T as #from_argument>::from_argument(value)
            }
        }

        trait AosabiReadFieldFallback<T> {
            fn aosabi_read_field(&self, value: &#value, expected: &'static str) -> #result;
        }

        impl<T> AosabiReadFieldFallback<T> for ::core::marker::PhantomData<T> {
            fn aosabi_read_field(&self, _: &#value, expected: &'static str) -> #result {
                ::core::result::Result::Err(#mismatch { expected })
            }
        }
    }
}

/// Returns the two readers of a private field, the one of a type with
/// `Default` and the fallback that refuses with the message of the struct.
fn default_readers() -> TokenStream {
    let mismatch = glue("ArgumentMismatch");
    let result = quote!(::core::result::Result<T, #mismatch>);
    quote! {
        trait AosabiDefaultField<T> {
            fn aosabi_default_field(&self, expected: &'static str) -> #result;
        }

        impl<T: ::core::default::Default> AosabiDefaultField<T> for &::core::marker::PhantomData<T> {
            fn aosabi_default_field(&self, _: &'static str) -> #result {
                ::core::result::Result::Ok(::core::default::Default::default())
            }
        }

        trait AosabiDefaultFieldFallback<T> {
            fn aosabi_default_field(&self, expected: &'static str) -> #result;
        }

        impl<T> AosabiDefaultFieldFallback<T> for ::core::marker::PhantomData<T> {
            fn aosabi_default_field(&self, expected: &'static str) -> #result {
                ::core::result::Result::Err(#mismatch { expected })
            }
        }
    }
}

/// Returns the glue of the enum `item` with `variants`, every variant of a
/// payload enum, of which the type attribute says whether it stands under
/// `#[aosabi(remote)]`, or every variant of an enum whose variants carry no
/// data with its value text, `values`.
///
/// An enum whose first variant carries data is a payload enum, and every
/// other enum takes the value texts.
#[must_use]
pub fn enumeration<'a>(
    item: &TypeItem<'_>,
    variants: impl IntoIterator<Item = &'a Variant>,
    remote: bool,
    values: &[(&Ident, String)],
) -> TokenStream {
    let variants: Vec<&Variant> = variants.into_iter().collect();
    let carries_payload = variants
        .first()
        .is_some_and(|first| !matches!(first.fields, Fields::Unit));
    let target = {
        let ident = item.ident;
        let (_, type_generics, _) = item.generics.split_for_impl();
        quote!(#ident #type_generics)
    };
    if carries_payload {
        let union = union_into_value_impl(item, &variants);
        let from_delegated = remote.then(|| remote::union_from_delegated(item, &variants));
        return quote!(#union #from_delegated);
    }
    let into_value = enum_into_value_impl(&target, item.generics, values);
    let from_argument = enum_from_argument_impl(&target, item.ident, item.generics, values);
    let from_delegated = enum_from_delegated_impl(item, values);
    quote!(#into_value #from_argument #from_delegated)
}

/// Returns `impl FromDelegated<Self>` and `impl SameMember` of an enum whose
/// variants carry no data, which read the value text of a variant, the value
/// of a remote enum the delegation hands back.
fn enum_from_delegated_impl(item: &TypeItem<'_>, variants: &[(&Ident, String)]) -> TokenStream {
    let ident = item.ident;
    let (impl_generics, type_generics, where_clause) = item.generics.split_for_impl();
    let (value, thrown) = (glue("Value"), glue("Thrown"));
    let message = format!(
        "the delegated value is not a value of the enum \"{}\"",
        unraw(ident)
    );
    let tests = variants.iter().map(|(variant, text)| {
        quote! {
            if *text == #text {
                return ::core::result::Result::Ok(Self::#variant);
            }
        }
    });
    let same = variants
        .iter()
        .map(|(variant, text)| quote!(Self::#variant => *text == #text,));
    quote! {
        #[automatically_derived]
        impl #impl_generics ::aosabi::remote::FromDelegated<Self> for #ident #type_generics #where_clause {
            fn from_delegated(value: #value) -> ::core::result::Result<Self, #thrown> {
                if let #value::String(text) = &value {
                    #(#tests)*
                }
                ::core::result::Result::Err(#thrown::type_error(#message))
            }
        }

        #[automatically_derived]
        impl #impl_generics ::aosabi::remote::delegated::SameMember for #ident #type_generics #where_clause {
            fn same_member(&self, member: &#value) -> bool {
                let #value::String(text) = member else {
                    return false;
                };
                match *self {
                    #(#same)*
                }
            }
        }
    }
}

/// Returns `impl IntoValue` and `impl FieldValue` of the enum `target` whose
/// variants carry no data, which is the value text of the variant.
///
/// As a data member of a shape the variant writes its value text as the
/// `&str` it is, so the executor escapes it into the answer, or names the
/// enum value it is, with no string built.
pub(crate) fn enum_into_value_impl(
    target: &TokenStream,
    generics: &Generics,
    variants: &[(&Ident, String)],
) -> TokenStream {
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let (into_value, field_value) = (glue("IntoValue"), glue("FieldValue"));
    let (value, shared) = (glue("Value"), glue("Shared"));
    let (member_kind, member_sink) = (glue("MemberKind"), glue("MemberSink"));
    let arms: Vec<TokenStream> = variants
        .iter()
        .map(|(variant, text)| quote!(Self::#variant => #into_value::into_value(#text),))
        .collect();
    let texts = variants
        .iter()
        .map(|(variant, text)| quote!(Self::#variant => sink.str(#text),));
    quote! {
        #[automatically_derived]
        impl #impl_generics #into_value for #target #where_clause {
            fn into_value(self) -> #value {
                match self {
                    #(#arms)*
                }
            }
        }

        #[automatically_derived]
        impl #impl_generics #field_value for #target #where_clause {
            const MEMBER: #member_kind = #member_kind::Leaf;

            fn field_value(&self, _: &dyn ::core::ops::Fn() -> #shared<Self>) -> #value {
                match *self {
                    #(#arms)*
                }
            }

            fn write_member(
                &self,
                sink: &mut #member_sink<'_>,
                _: &dyn ::core::ops::Fn() -> #shared<Self>,
            ) {
                match *self {
                    #(#texts)*
                }
            }
        }
    }
}

/// Returns `impl FromArgument` of the enum `target`, whose identifier is
/// `ident`, whose variants carry no data, which reads the value text of a
/// variant.
pub(crate) fn enum_from_argument_impl(
    target: &TokenStream,
    ident: &Ident,
    generics: &Generics,
    variants: &[(&Ident, String)],
) -> TokenStream {
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let (from_argument, mismatch, value) = (
        glue("FromArgument"),
        glue("ArgumentMismatch"),
        glue("Value"),
    );
    let expected = format!("a value of the enum `{}`", unraw(ident));
    let tests = variants.iter().map(|(variant, text)| {
        quote! {
            if *text == #text {
                return ::core::result::Result::Ok(Self::#variant);
            }
        }
    });
    quote! {
        #[automatically_derived]
        impl #impl_generics #from_argument for #target #where_clause {
            fn from_argument(value: &#value) -> ::core::result::Result<Self, #mismatch> {
                if let #value::String(text) = value {
                    #(#tests)*
                }
                ::core::result::Result::Err(#mismatch { expected: #expected })
            }
        }
    }
}

/// Returns `impl IntoValue` and `impl FieldValue` of an enum whose variants
/// each carry one payload, which are the value of the payload, and the
/// conversions of [`from_variants`].
///
/// As a field, a payload that is a described struct is the object of its
/// part, which reaches the payload through the variant that holds it.
fn union_into_value_impl(item: &TypeItem<'_>, variants: &[&Variant]) -> TokenStream {
    let ident = item.ident;
    let generics = bounded_generics(item.generics, &[glue("IntoValue")]);
    let (_, type_generics, _) = generics.split_for_impl();
    let target = quote!(#ident #type_generics);
    let field_generics = bounded_generics(item.generics, &[glue("FieldValue")]);
    let names: Vec<&Ident> = variants.iter().map(|variant| &variant.ident).collect();
    let impls = union_impls(&target, &generics, &field_generics, &names);
    let from = from_variants(item, variants);
    quote! {
        #impls

        #from
    }
}

/// Returns `impl IntoValue` and `impl FieldValue` of the payload enum
/// `target` with the variants `variants`, `IntoValue` under `generics` and
/// `FieldValue` under `field_generics`.
pub(crate) fn union_impls(
    target: &TokenStream,
    generics: &Generics,
    field_generics: &Generics,
    variants: &[&Ident],
) -> TokenStream {
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let (into_value, field_value) = (glue("IntoValue"), glue("FieldValue"));
    let (value, shared) = (glue("Value"), glue("Shared"));
    let (field_impl_generics, _, field_where_clause) = field_generics.split_for_impl();
    let field_arms = variants.iter().map(|variant| {
        let others = variants.iter().filter(|other| *other != variant);
        quote! {
            Self::#variant(payload) => #field_value::field_value(
                payload,
                &|| part().part(|union: &Self| match union {
                    Self::#variant(payload) => ::core::option::Option::Some(payload),
                    #(Self::#others { .. } => ::core::option::Option::None,)*
                }),
            ),
        }
    });
    quote! {
        #[automatically_derived]
        impl #impl_generics #into_value for #target #where_clause {
            fn into_value(self) -> #value {
                match self {
                    #(Self::#variants(payload) => #into_value::into_value(payload),)*
                }
            }
        }

        #[automatically_derived]
        impl #field_impl_generics #field_value for #target #field_where_clause {
            fn field_value(&self, part: &dyn ::core::ops::Fn() -> #shared<Self>) -> #value {
                match self {
                    #(#field_arms)*
                }
            }
        }
    }
}

/// Returns `impl From<Payload>` of an enum whose variants each carry one
/// payload, one per variant whose payload type no payload type of another
/// variant can equal.
///
/// A resolver that returns the union then returns a member with `.into()`, as
/// a TypeScript resolver returns an object of a member type. Two payload
/// types that some choice of the type parameters makes one type would give
/// the enum two conversions from it, which the overlap check refuses, so
/// neither gets one, `From<T>` beside `From<String>` at `T = String`. The
/// reflexive conversion of the standard library converts the enum into
/// itself, which no payload is, so a generic enum gets a conversion from a
/// parameter, from `Vec<T>` or from a concrete type wherever no other payload
/// can equal it.
fn from_variants(item: &TypeItem<'_>, variants: &[&Variant]) -> TokenStream {
    let ident = item.ident;
    let (impl_generics, type_generics, where_clause) = item.generics.split_for_impl();
    let params: Vec<&Ident> = item
        .generics
        .type_params()
        .map(|param| &param.ident)
        .collect();
    let payloads: Vec<(&Ident, &Type)> = variants
        .iter()
        .filter_map(|variant| match &variant.fields {
            Fields::Unnamed(fields) if fields.unnamed.len() == 1 => fields
                .unnamed
                .first()
                .map(|field| (&variant.ident, &field.ty)),
            _ => None,
        })
        .collect();
    let impls = payloads.iter().enumerate().filter_map(|(index, (variant, ty))| {
        let alone = payloads
            .iter()
            .enumerate()
            .all(|(other, (_, other_ty))| other == index || !can_equal(ty, other_ty, &params));
        alone.then(|| {
            quote! {
                #[automatically_derived]
                impl #impl_generics ::core::convert::From<#ty> for #ident #type_generics #where_clause {
                    fn from(payload: #ty) -> Self {
                        Self::#variant(payload)
                    }
                }
            }
        })
    });
    quote!(#(#impls)*)
}

/// Returns the glue of the newtype `item` under `#[aosabi::scalar]`, which is
/// the conversions over its one field.
///
/// As a data member the newtype holds what its field holds and writes its
/// field as its field writes itself.
#[must_use]
pub fn scalar(item: &ItemStruct) -> TokenStream {
    let ident = &item.ident;
    let (into_value, from_argument) = (glue("IntoValue"), glue("FromArgument"));
    let (field_value, shared) = (glue("FieldValue"), glue("Shared"));
    let (mismatch, value) = (glue("ArgumentMismatch"), glue("Value"));
    let (member_kind, member_sink) = (glue("MemberKind"), glue("MemberSink"));
    // The newtype carries one unnamed field, which `newtype` of the module
    // `scalar` checked before the glue is emitted.
    let member = item.fields.iter().next().map(|field| {
        let inner = &field.ty;
        quote!(const MEMBER: #member_kind = <#inner as #field_value>::MEMBER;)
    });
    quote! {
        #[automatically_derived]
        impl #into_value for #ident {
            fn into_value(self) -> #value {
                #into_value::into_value(self.0)
            }
        }

        #[automatically_derived]
        impl #field_value for #ident {
            #member

            fn field_value(&self, part: &dyn ::core::ops::Fn() -> #shared<Self>) -> #value {
                #field_value::field_value(
                    &self.0,
                    &|| part().part(|scalar: &Self| ::core::option::Option::Some(&scalar.0)),
                )
            }

            fn write_member(
                &self,
                sink: &mut #member_sink<'_>,
                part: &dyn ::core::ops::Fn() -> #shared<Self>,
            ) {
                #field_value::write_member(
                    &self.0,
                    sink,
                    &|| part().part(|scalar: &Self| ::core::option::Option::Some(&scalar.0)),
                );
            }
        }

        #[automatically_derived]
        impl #from_argument for #ident {
            fn from_argument(value: &#value) -> ::core::result::Result<Self, #mismatch> {
                #from_argument::from_argument(value).map(Self)
            }
        }
    }
}

/// One parameter of a resolver method as the glue reads it.
pub struct Param {
    /// The name the `TypeError` of a mismatch prints.
    pub name: String,
    /// The declared type, or `None` where the glue leaves the type to the
    /// call that takes the argument, as the source path does.
    pub ty: Option<Type>,
    /// The expression of `#[aosabi(default = expr)]`.
    pub default: Option<Expr>,
}

/// One resolver method or static as the glue reads it.
pub struct Method {
    /// The identifier the call names.
    pub(crate) ident: Ident,
    /// The key the method answers under, its name in the schema.
    pub(crate) key: String,
    /// Whether the method is an `async fn`.
    pub(crate) is_async: bool,
    /// The typed parameters, in declaration order.
    pub(crate) params: Vec<Param>,
    /// The expressions of the decorators, in source order.
    pub(crate) decorators: Vec<Expr>,
    /// Whether the method answers a stream, `impl Stream<Item = T>` or a
    /// `Result` of one, which the source path alone writes.
    pub(crate) stream: bool,
    /// Whether the method answers the payload of a mutation,
    /// `MutationPayload<T>` or a `Result` of one, which the source path alone
    /// writes.
    pub(crate) payload: bool,
}

/// Returns the public methods of `block` whose receiver is `&self`, in
/// declaration order.
///
/// The function reads the block before the helper attributes are taken off
/// it, because the default of a parameter and a decorator live in one. It
/// refuses nothing. A method the descriptor refuses is left to that refusal,
/// and the caller emits no method beside one.
#[must_use]
pub fn methods(block: &ItemImpl) -> Vec<Method> {
    block
        .items
        .iter()
        .filter_map(|item| match item {
            ImplItem::Fn(method) if method.sig.receiver().is_some() => glue_method(method),
            _ => None,
        })
        .collect()
}

/// Returns the public functions of `block` with no receiver, its statics, in
/// declaration order.
///
/// A private function with no receiver stays a helper of the block and gets
/// no glue, as a private method gets none.
#[must_use]
pub fn statics(block: &ItemImpl) -> Vec<Method> {
    block
        .items
        .iter()
        .filter_map(|item| match item {
            ImplItem::Fn(method) if method.sig.receiver().is_none() => glue_method(method),
            _ => None,
        })
        .collect()
}

/// Returns `method` as the glue reads it, `None` for a private function.
///
/// A private method is no field of the schema, so a root that listed it among
/// its keys would bind a resolver to a field that does not exist.
fn glue_method(method: &ImplItemFn) -> Option<Method> {
    if matches!(method.vis, Visibility::Inherited) {
        return None;
    }
    let params = method.sig.inputs.iter().filter_map(glue_param).collect();
    let decorators = method
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("decorate"))
        .filter_map(|attr| attr.parse_args::<Expr>().ok())
        .collect();
    Some(Method {
        ident: method.sig.ident.clone(),
        key: unraw(&method.sig.ident),
        is_async: method.sig.asyncness.is_some(),
        params,
        decorators,
        stream: false,
        payload: false,
    })
}

/// Returns a typed parameter as the glue reads it, `None` for the receiver
/// and for a pattern that is no identifier.
fn glue_param(arg: &FnArg) -> Option<Param> {
    let FnArg::Typed(param) = arg else {
        return None;
    };
    let Pat::Ident(pattern) = &*param.pat else {
        return None;
    };
    Some(Param {
        name: unraw(&pattern.ident),
        ty: Some((*param.ty).clone()),
        default: default_of(&param.attrs),
    })
}

/// Returns the expression of `#[aosabi(default = expr)]` among `attrs`.
///
/// A key that does not parse is left to the refusal of the descriptor.
#[must_use]
pub fn default_of(attrs: &[Attribute]) -> Option<Expr> {
    let mut default = None;
    for attr in attrs.iter().filter(|attr| attr.path().is_ident("aosabi")) {
        let parsed = attr.parse_nested_meta(|meta| {
            let expr: Expr = meta.value()?.parse()?;
            if meta.path.is_ident("default") {
                default = Some(expr);
            }
            Ok(())
        });
        if parsed.is_err() {
            return None;
        }
    }
    default
}

/// Returns the method table of the resolvers block `block` with `methods`,
/// as the inherent items `__AOSABI_NAMES` and `__aosabi_method` of its type.
///
/// The items shadow those of `ResolversFallback`, and the derive reads them
/// by path in its `impl ResolverMethods`.
#[must_use]
pub fn method_table(block: &ItemImpl, methods: &[Method]) -> TokenStream {
    let generics = bounded_generics(&block.generics, &object_bounds());
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let self_type = &block.self_ty;
    let (shared, function) = (glue("Shared"), glue("Function"));
    let names: Vec<&str> = methods.iter().map(|method| method.key.as_str()).collect();
    let arms = methods.iter().map(method_arm);
    let allow = lints::allow();
    quote! {
        #allow
        impl #impl_generics #self_type #where_clause {
            #[doc(hidden)]
            pub const __AOSABI_NAMES: &'static [&'static str] = &[#(#names),*];

            #[doc(hidden)]
            pub fn __aosabi_method(this: &#shared<Self>, key: &str) -> ::core::option::Option<#function> {
                match key {
                    #(#arms)*
                    _ => ::core::option::Option::None,
                }
            }
        }
    }
}

/// Returns the statics of the resolvers block `block`, one `EntryFunction`
/// `__AOSABI_STATIC_<name>` and its function of the executor per static,
/// with the `MethodInfo` literal of each in `infos`.
#[must_use]
pub fn static_items(
    block: &ItemImpl,
    generics: &Generics,
    statics: &[Method],
    infos: &[(Ident, TokenStream)],
) -> TokenStream {
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let self_type = &block.self_ty;
    let entry = glue("EntryFunction");
    let items = statics.iter().filter_map(|method| {
        let (_, info) = infos.iter().find(|(ident, _)| *ident == method.ident)?;
        let item = format_ident!("__AOSABI_STATIC_{}", unraw(&method.ident));
        let run = format_ident!("__aosabi_static_{}", unraw(&method.ident));
        let ident = &method.ident;
        let call = call_body(&quote!(Self::#ident), &method.params, method.is_async);
        let decorators: Vec<&Expr> = method.decorators.iter().collect();
        let body = decorated_body(&decorators, &call);
        let run_fn = run_fn(&quote!(pub), &run, &body);
        Some(quote! {
            #[doc(hidden)]
            pub const #item: #entry = #entry {
                info: &#info,
                run: Self::#run,
            };

            #run_fn
        })
    });
    let allow = lints::allow();
    quote! {
        #allow
        impl #impl_generics #self_type #where_clause {
            #(#items)*
        }
    }
}

/// Returns `impl ResolverMethods` of the derived struct `self_type`, which
/// reads the method table of its resolvers block, or the empty table of
/// `ResolversFallback` where it has none.
fn resolver_methods_impl(self_type: &impl quote::ToTokens, generics: &Generics) -> TokenStream {
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let (trait_path, shared, function, fallback) = (
        glue("ResolverMethods"),
        glue("Shared"),
        glue("Function"),
        glue("ResolversFallback"),
    );
    quote! {
        #[automatically_derived]
        impl #impl_generics #trait_path for #self_type #where_clause {
            const NAMES: &'static [&'static str] = {
                #[allow(unused_imports, reason = "a type with a resolvers block reads its own table")]
                use #fallback as _;
                <Self>::__AOSABI_NAMES
            };

            fn method(this: &#shared<Self>, key: &str) -> ::core::option::Option<#function> {
                #[allow(unused_imports, reason = "a type with a resolvers block reads its own table")]
                use #fallback as _;
                <Self>::__aosabi_method(this, key)
            }
        }
    }
}

/// Returns the arm of `method`, a function bound to the object that reads its
/// positional arguments and completes with the return of the method.
///
/// The function holds the object as a `Shared` and reaches the struct when
/// it is called. A decorated method runs under its decorators, whose wrapper
/// holds its own handle of the object.
pub(crate) fn method_arm(method: &Method) -> TokenStream {
    let (shared, function) = (glue("Shared"), glue("Function"));
    let name = &method.key;
    let locals: Vec<Ident> = (0..method.params.len())
        .map(|index| format_ident!("argument_{index}"))
        .collect();
    let bindings = method
        .params
        .iter()
        .zip(&locals)
        .enumerate()
        .map(|(index, (param, local))| binding(index, param, local));
    let completion = completion(method, &locals);
    let inner = quote! {
        #(#bindings)*
        #completion
    };
    let inner = if method.payload {
        payload_body(&inner)
    } else {
        inner
    };
    let decorators: Vec<&Expr> = method.decorators.iter().collect();
    let body = if decorators.is_empty() {
        inner
    } else {
        let decorated = decorated_body(&decorators, &inner);
        quote! {
            let this = #shared::clone(&this);
            #decorated
        }
    };
    // A method that is no `async fn`, has no decorator and answers no
    // payload of a mutation never reads the agent of the call.
    let agent = if method.is_async || !decorators.is_empty() || method.payload {
        format_ident!("agent")
    } else {
        format_ident!("_agent")
    };
    quote! {
        #name => {
            let this = #shared::clone(this);
            ::core::option::Option::Some(#function::new(#name, move |#agent, args| {
                #body
            }))
        }
    }
}

/// Returns `inner` run under the mutation wrapper, the body of a method that
/// answers the payload of a mutation, which maps the value of the method and
/// its expected failures to the payload.
///
/// The wrapper outlives the call, so it owns the arguments and a handle of
/// the object, which `inner` reads as `args` and `this`. The list of the
/// arguments is named through the re-export of the facade, so a crate
/// without the standard library takes the same body.
fn payload_body(inner: &TokenStream) -> TokenStream {
    let (wrapper, agent, value) = (glue("mutation_wrapper"), glue("Agent"), glue("Value"));
    let (completion, shared) = (glue("Completion"), glue("Shared"));
    quote! {
        let this = #shared::clone(&this);
        let args: ::aosabi::__private::Vec<#value> = args.to_vec();
        #wrapper(agent, move |agent: &#agent| -> #completion {
            let args: &[#value] = &args;
            #inner
        })
    }
}

/// Returns the statement that reads the parameter `param` at `index` into
/// `local`.
///
/// A mismatch returns the throw completion of its `TypeError`. A parameter
/// with a default takes it for a missing and for an `undefined` argument,
/// which is when a TypeScript initializer runs.
fn binding(index: usize, param: &Param, local: &Ident) -> TokenStream {
    let (argument, completion, value) = (glue("argument"), glue("Completion"), glue("Value"));
    let Param { name, ty, default } = param;
    // A parameter whose type the glue does not write takes the type of the
    // call it is handed to.
    let (turbofish, annotation) = match ty {
        Some(ty) => (quote!(::<#ty>), quote!(: #ty)),
        None => (TokenStream::new(), TokenStream::new()),
    };
    let read = quote! {
        match #argument #turbofish(args, #index, #name) {
            ::core::result::Result::Ok(value) => value,
            ::core::result::Result::Err(error) => return #completion::Throw(error),
        }
    };
    let Some(default) = default else {
        return quote!(let #local #annotation = #read;);
    };
    quote! {
        let #local #annotation = match args.get(#index) {
            ::core::option::Option::None
            | ::core::option::Option::Some(#value::Undefined) => #default,
            ::core::option::Option::Some(_) => #read,
        };
    }
}

/// Returns the expression that calls `method` with `locals` and completes.
///
/// The call reaches the struct of `this` first, and an object that is no
/// longer reached throws the `TypeError` of a property read on `undefined`.
/// An `async fn` becomes a promise on the agent of the call. The future owns
/// a handle of the object, because it outlives the call, and reaches the
/// struct inside.
fn completion(method: &Method, locals: &[Ident]) -> TokenStream {
    let (complete, complete_future) = (glue("complete"), glue("complete_future"));
    let (shared, reached, unreached) = (glue("Shared"), glue("Reached"), glue("unreached"));
    let ident = &method.ident;
    let name = &method.key;
    if method.stream {
        // The future owns a handle of the object, since the stream the method
        // answers may borrow the object, and `complete_stream` polls it once
        // now, so the method runs when the field subscribes.
        let (complete_stream, start) = (glue("complete_stream"), glue("StreamStart"));
        return quote! {
            #complete_stream({
                let this = #shared::clone(&this);
                move |start: #start| async move {
                    match this.reach() {
                        ::core::option::Option::Some(object) => {
                            start.run(object.#ident(#(#locals),*)).await
                        }
                        ::core::option::Option::None => #unreached(#name),
                    }
                }
            })
        };
    }
    if !method.is_async {
        return quote! {
            let ::core::option::Option::Some(object) = this.reach() else {
                return #unreached(#name);
            };
            #complete(object.#ident(#(#locals),*))
        };
    }
    quote! {
        #complete_future(agent, {
            let this = #shared::clone(&this);
            async move {
                match this.reach() {
                    ::core::option::Option::Some(object) => {
                        #reached::Object(object.#ident(#(#locals),*).await)
                    }
                    ::core::option::Option::None => #reached::Unreached(#name),
                }
            }
        })
    }
}

/// Returns the body of a function of the executor that reads `params` from
/// its positional arguments, calls `callee` with them and completes with its
/// return.
///
/// The body reads the agent of the call as `agent` and the arguments as
/// `args`. An `async fn` becomes a promise on the agent of the call, whose
/// future owns the arguments.
#[must_use]
pub fn call_body(callee: &TokenStream, params: &[Param], is_async: bool) -> TokenStream {
    let (complete, complete_future) = (glue("complete"), glue("complete_future"));
    let locals: Vec<Ident> = (0..params.len())
        .map(|index| format_ident!("argument_{index}"))
        .collect();
    let bindings = params
        .iter()
        .zip(&locals)
        .enumerate()
        .map(|(index, (param, local))| binding(index, param, local));
    let call = if is_async {
        quote!(#complete_future(agent, #callee(#(#locals),*)))
    } else {
        quote!(#complete(#callee(#(#locals),*)))
    };
    quote! {
        #(#bindings)*
        #call
    }
}

/// Returns `inner` run under the decorators `decorators`, the outermost
/// first.
///
/// Each decorator is one async wrapper that awaits its callback with the
/// positional arguments before it calls the next, and the decorators of a
/// site are built once per thread on the first call. Without a decorator the
/// body is `inner` as it stands.
#[must_use]
pub fn decorated_body(decorators: &[&Expr], inner: &TokenStream) -> TokenStream {
    if decorators.is_empty() {
        return inner.clone();
    }
    let (cell, decorated, of_site) = (glue("DecoratorCell"), glue("decorated"), glue("decorators"));
    let (rc, agent, value, completion) =
        (glue("Rc"), glue("Agent"), glue("Value"), glue("Completion"));
    quote! {
        ::std::thread_local! {
            static __AOSABI_DECORATORS: #cell = const { #cell::new() };
        }
        #decorated(
            agent,
            #of_site(&__AOSABI_DECORATORS, || ::std::vec![#(#decorators),*]),
            args,
            #rc::new(move |agent: &#agent, args: &[#value]| -> #completion { #inner }),
        )
    }
}

/// Returns `inner` run under the mutation wrapper, which maps its result
/// and its expected failures to the payload of the mutation.
#[must_use]
pub fn mutation_body(inner: &TokenStream) -> TokenStream {
    let (wrapper, agent, value, completion) = (
        glue("mutation_wrapper"),
        glue("Agent"),
        glue("Value"),
        glue("Completion"),
    );
    quote! {
        let args: ::std::vec::Vec<#value> = args.to_vec();
        #wrapper(agent, move |agent: &#agent| -> #completion {
            let args: &[#value] = &args;
            #inner
        })
    }
}

/// Returns the function `name` of the executor with the body `body`, which
/// reads the agent as `agent` and the arguments as `args`.
#[must_use]
pub fn run_fn(vis: &TokenStream, name: &Ident, body: &TokenStream) -> TokenStream {
    let (agent, value, completion) = (glue("Agent"), glue("Value"), glue("Completion"));
    quote! {
        #[doc(hidden)]
        #vis fn #name(agent: &#agent, args: &[#value]) -> #completion {
            let _ = (agent, args);
            #body
        }
    }
}
