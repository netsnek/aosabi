// SPDX-License-Identifier: MIT
/*
 * aosabi-macros: the procedural macros of Aosabi.
 *
 * The macros stand in for the view the TypeScript checker gives of a
 * program, and they write it as the constants of aosabi-core, module
 * descriptor. This file holds the entry points and nothing else.
 */

//! The procedural macros of Aosabi.
//!
//! A TypeScript service needs no annotation, because its build asks the
//! TypeScript checker about every type of the service. A Rust build has no
//! checker to ask, so the macros of this crate record what the checker would
//! answer as static descriptors, and the inference of `aosabi-infer` walks
//! those.
//!
//! | Macro | Stands on | Emits |
//! | --- | --- | --- |
//! | [`Aosabi`](macro@Aosabi) | a struct or an enum | `impl Described` with the declaration list of its target and one `Declaration` |
//! | [`resolvers`](macro@resolvers) | an inherent `impl` block | the block, its method table as inherent items, `impl Resolvers`, one `EntryFunction` per static, and the length check |
//! | [`scalar`](macro@scalar) | a newtype | the struct, `impl Described` with the declaration list of its target, and one `Declaration` |
//! | [`graphql!`](macro@graphql) | the roots of an application as an object literal | the fragment of the roots, its descriptors and its run time |
//! | [`resolver`](macro@resolver) | a free function | the function and the hidden module of its descriptor |
//! | [`decorate`](macro@decorate) | an entry, a method, a static or a free function | nothing, because the macro it stands under takes it off |
//! | [`plugin!`](macro@plugin) | the members of a plugin as an object literal | a `PluginObject` |
//! | [`plugins!`](macro@plugins) | a list of plugins | the plugin list with the source text and the line of each element |
//! | [`Bindings`](macro@Bindings) | a struct of bindings | `current()`, which reads each field from the environment of the context |
//! | [`Variables`](macro@Variables) | a struct of context variables | one `Key` constant per field and `impl Variables` |
//!
//! Under the default features each one answers a compile error that names its
//! native form, the plain Rust the builder of `aosabi-build` reads from the
//! source with no attribute, derive or macro of Aosabi, and a derive or an
//! attribute answers one error per key of `#[aosabi(..)]` its item carries
//! before its own.
//!
//! The declaration list of a target is the file the build script of the crate
//! writes, which calls `aosabi_build::declarations_main`, and it names the
//! `Declaration` constant of every derive and scalar attribute the scan of the
//! module tree finds. A crate with no such build script gets no list, and its
//! macros emit no `Declaration`. In a crate whose build script writes the
//! lists, a macro in a target with no list of its own is refused, because the
//! script has not seen that target.
//!
//! Each macro also emits the run time glue of its item, the implementations
//! of the traits of `aosabi-core`, module `glue`, through which the executor
//! reads a Rust value and calls a Rust method. The library `aosabi-codegen`
//! emits it, the one the builder of a plain crate calls for the source path
//! as well, and its module `glue` lists what each macro emits.
//!
//! An application reaches the macros through the facade crate `aosabi` and
//! never names this crate, and the generated code names every item through
//! `::aosabi::descriptor::` and `::aosabi::__private::`.
//!
//! | Module | Holds |
//! | --- | --- |
//! | `attrs` | the parser of `#[aosabi(...)]` on a type, a field, a variant, a resolver and a parameter |
//! | `docs` | the documentation of an item, its comment and its tags, and of a parameter |
//! | `jsdoc` | the port of the `JSDoc` parser of the TypeScript checker that `docs` reads a block with |
//! | `links` | the `TypeLink` expression of a type and the refusals the tokens decide |
//! | `list` | the path of the declaration list of the target a macro expands in |
//! | `emit` | the tokens the three macros share |
//! | `derive_struct` | the derive on a struct |
//! | `derive_enum` | the derive on an enum |
//! | `resolvers` | `#[aosabi::resolvers]` |
//! | `scalar` | `#[aosabi::scalar]` |
//! | `refuse` | the text of every refusal |
//! | `graphql` | `graphql!`, its grammar, its expansion and the rule that tells a type path from a module path |
//! | `resolver` | `#[aosabi::resolver]` on a free function |
//! | `decorate` | `#[decorate(expr)]` and the refusal of a decorator outside its sites |
//! | `plugin` | `plugin!` and `plugins!` |
//! | `bindings` | `#[derive(Bindings)]` |
//! | `variables` | `#[derive(Variables)]` |
//! | `remote` | the key `#[aosabi(arguments)]` of a type of a pulled registry |
//! | `spread` | the fields under the key `#[aosabi(spread)]` of a struct |
//! | `guards` | the leading guard calls of a body, recorded as guard facts |
//! | `retired` | the answers of the macros under the default features, a refusal that names the native form of each form and each key |
//!
//! There is no test in this crate, which is the rule of CONTRIBUTING.md
//! section 3. The judging notebooks are
//! `tests/notebooks/10_schema_tier1.ipynb` for the descriptors and
//! `tests/notebooks/31_http_errors.ipynb` for the glue.

use proc_macro::TokenStream;
use syn::{
    Data,
    DeriveInput,
    parse_macro_input, //
};

use aosabi_codegen::{
    glue,
    lints, //
};

mod attrs;
mod bindings;
mod decorate;
mod derive_enum;
mod derive_struct;
mod docs;
mod emit;
mod graphql;
mod guards;
mod jsdoc;
mod links;
mod list;
mod plugin;
mod refuse;
mod remote;
mod resolver;
mod resolvers;
mod retired;
mod scalar;
mod spread;
mod variables;

/// Describes a struct or an enum to the schema inference.
///
/// The derive accepts a struct with named fields, a unit struct, an enum whose
/// variants all carry no data, and an enum whose variants all carry exactly
/// one unnamed field. A struct is the twin of a TypeScript class, the first
/// kind of enum is the twin of a union of string literals, and the second is
/// the twin of a union of object types. The name of the type and of every
/// member is the identifier with a raw prefix removed and nothing else
/// changed. The documentation comment of the type and of every member becomes
/// its description by the rule of `docs/authoring.md` section 29.5: only the
/// last block counts, the gutter goes against the margin of the first text,
/// and the text splits into the comment and the tags the TypeScript checker
/// reads. A field without `pub` is recorded as private, and the inference
/// skips it.
///
/// The keys of `#[aosabi(...)]` on the type are `typename = "Name"`, the twin
/// of the `__typename` literal of a TypeScript class, `anonymous`, which states
/// that the TypeScript twin has no alias, on a struct the twin of an object
/// type literal and on an enum whose variants carry a payload the twin of an
/// inferred union, which the inference names by the property it is reached
/// through, `enum` on an enum whose variants carry no data, the twin of a
/// TypeScript `enum` declaration, whose comment the inference reads where it
/// reads none of a union of string literals, `implements(Base, ...)`,
/// `remote`, `arguments`, and `select = Type`, which belongs to the derive of
/// the typed client and which this derive accepts and leaves to it. The key
/// `remote` marks a type of a registry `aosabi pull` writes: a struct takes
/// its identifier as its typename, keeps the remote object it is read from
/// in its field `__typename: TypeName`, which is no GraphQL field, reads its
/// fields from it leniently, and answers that object as its value while no
/// field differs from the member it was read from, a field a patch or a
/// resolver assigns being answered from the field, and an enum whose
/// variants carry a payload reads a delegated value as the variant its
/// `__typename` names.
/// The key `arguments` marks the arguments struct of a remote field, which
/// implements `Params` with one parameter per field in field order, a field
/// `Option<T>` the optional parameter `name?: T`. The keys on a field are `extends`
/// and `spread`. The key `extends` is the twin of the `extends` clause of a
/// class, which makes the field the embedded base of the struct: the field
/// is no GraphQL field, the struct states the heritage `Embedded` first, the
/// derive implements `Deref` to the field, and a member the struct lacks
/// reads through it. The key `spread` is the twin of a spread `{...value}`
/// in an object literal, at the place the field stands: the field is no
/// GraphQL field, the inference presents the members of its type in its
/// place by the spread order of the checker, and the object of the struct
/// answers a key the later segments of the literal do not answer through
/// it. A struct with a spread embeds no base. The key on a variant that
/// carries no data is `value = "text"`, the string the variant stands for,
/// which is the name of the variant otherwise.
///
/// The derive emits `impl Described` and one `Declaration` constant that the
/// declaration list of the crate names, on every target. The implementation
/// of `Described` carries that list as `DECLARATIONS`, and a crate whose
/// build script writes no list gets neither. Every type parameter gains the
/// bound `Described`, and a generic declaration has no link. The methods of
/// the descriptor are the table the resolvers block of the type emits, read
/// by path, and the empty table of `ResolversFallback` for a type with no
/// block.
///
/// For the run time the derive emits `impl ObjectLike`, `impl SharedObject`,
/// `impl FieldValue` and `impl IntoValue` on a struct, whose public fields
/// have `FieldValue`, and `impl ResolverMethods`, which reads the method
/// table of the block the same way. A public field is read through a shared
/// reference, so neither the struct nor a field type needs `Clone`, and a
/// field that is a described struct is an object that reaches it through the
/// object that holds it. An object lists the typename of `typename = "Name"`
/// first under the key `__typename`, then its public fields, then the methods
/// of its block, then each key of its embedded base it lacks, which it reads
/// through a shared reference as well. An object with a spread lists the
/// keys of each spread at its place among the public fields, a key that
/// stands twice once, and reads a key from its later occurrence. A struct
/// with no embedded base, no spread and no kept remote object answers its
/// static shape, its public fields as data members in declaration order and
/// the methods of its block, and writes the data members a selection reads
/// by their slots, a field of a string type as the `&str` it holds. On a
/// struct with named fields the derive also emits `impl FromArgument`, so the
/// struct stands as a resolver parameter and as the type of an input field,
/// the twin of an object parameter of a TypeScript resolver. It reads each
/// public field from the property of its name, a missing property as
/// `undefined`, and gives a private field its `Default`. It exists for every
/// such struct whose type parameters have `FromArgument`, so a struct that
/// reaches itself reads as well, and a public field whose type has no
/// `FromArgument` or a private one whose type has no `Default` throws the
/// `TypeError` of the struct when an argument reaches it. On an enum whose
/// variants carry no data the derive emits `impl IntoValue`,
/// `impl FieldValue`, `impl FromArgument` and `impl FromDelegated<Self>` over
/// the value text, and on an enum whose variants carry a payload it emits
/// `impl IntoValue` and `impl FieldValue` that delegate to the payload and
/// `impl From` of each payload type that no payload type of another variant
/// can equal for any choice of the type parameters. A struct and an enum
/// under `remote` gain `impl FromDelegated<Self>`, and a struct with a spread
/// gains `impl FromDelegated<Rec>` for the type `Rec` of its first spread
/// field in source order, which reads its own public fields strictly and
/// throws the `TypeError`
/// `the delegated value carries no member "<name>"` for one the value lacks.
///
/// The derive refuses a `union`, a tuple struct, an enum that mixes variant
/// shapes, a lifetime and a const parameter, an unknown key, the key
/// `resolvers`, which states nothing since the derive reads the method table
/// of an [`#[aosabi::resolvers]`](macro@resolvers) block by path, a base
/// stated as `extends(...)` on the type, which is a field under
/// `#[aosabi(extends)]`, a field under both `extends` and `spread`, a
/// spread beside an embedded base, the key `remote` on a struct with no
/// field `__typename`, with an embedded base, with a spread or with no
/// field and on an enum whose variants carry no data, the key `arguments`
/// on an enum, the key `enum` anywhere but on a named
/// enum whose variants carry no data, a `#[doc]` whose value is no string
/// literal, a `#[deprecated]`, and a member whose type holds a tuple other
/// than `()`, a function pointer, `impl Trait` or `dyn Trait`. Each refusal
/// is a compile error at the tokens that cause it. A member whose type
/// implements `Described` nowhere is refused by the compiler with the
/// message of that trait. A refused type with no parameters still implements
/// `Described`, so no error of that trait follows its refusal where the type
/// stands.
///
/// # Examples
///
/// ```ignore
/// use aosabi::Aosabi;
///
/// /// A user.
/// #[derive(Aosabi)]
/// pub struct User {
///     /// The display name.
///     pub name: String,
///     pub friends: Vec<User>,
///     secret: f64,
/// }
///
/// #[derive(Aosabi)]
/// pub enum Role {
///     #[aosabi(value = "admin")]
///     Admin,
///     Member,
/// }
/// ```
#[proc_macro_derive(Aosabi, attributes(aosabi))]
pub fn derive_aosabi(input: TokenStream) -> TokenStream {
    if !cfg!(feature = "mirror") {
        return retired::derive(&input.into(), refuse::RETIRED_DERIVE).into();
    }
    let input = parse_macro_input!(input as DeriveInput);
    let expanded = match &input.data {
        Data::Struct(data) => derive_struct::expand(&input, data),
        Data::Enum(data) => derive_enum::expand(&input, data),
        Data::Union(data) => Err(refuse::refusal(
            data.union_token.span,
            refuse::DERIVE_ON_UNION,
        )),
    };
    expanded
        .unwrap_or_else(|error| {
            let mut refused = error.to_compile_error();
            refused.extend(emit::refused_described(&input));
            refused.extend(emit::refused_declaration(&input.ident));
            refused
        })
        .into()
}

/// Records the methods of an inherent `impl` block as the resolvers of its
/// type.
///
/// A method whose receiver is `&self` becomes one `MethodInfo`, in declaration
/// order. A public function with no receiver is a static, the twin of a
/// static member, which never becomes a field, because the inference walks
/// instance types only. It becomes the associated constant `__AOSABI_STATIC_<name>`,
/// an `EntryFunction` an entry of [`graphql!`](macro@graphql) references as
/// `Type::name`. A private function with no receiver stays a helper. A
/// `#[decorate(expr)]` on a method or a static is taken off and runs in the
/// glue around it, the outermost first. An `async fn` is recorded as such and links its return as
/// `Promise<R>`, and a method that states no return type links `()`. A
/// parameter under `#[aosabi(default = expr)]` is recorded as optional, the
/// twin of a TypeScript parameter with an initializer, and the attribute is
/// taken off the emitted block. A doc comment on a parameter is taken off as
/// well, and it composes with the `@param` tags of the method that name the
/// parameter into the parameter documentation, which reaches `ir.json` and
/// never the schema. The block is emitted unchanged otherwise.
///
/// The attribute emits the method table after the block, the inherent items
/// `__AOSABI_METHODS`, `__AOSABI_NAMES` and `__aosabi_method` of the type,
/// which the derive reads by path, and `impl Resolvers`. For a type without
/// generic parameters it also emits a check that the path the derive reads
/// reaches the table of the block.
///
/// For the run time the method table holds one function of the executor per
/// public method. The function reads each parameter from
/// its positional argument through `FromArgument`, takes the default of a
/// parameter for a missing and for an `undefined` argument, and completes
/// with the return through `IntoCompletion`. An `async fn` becomes a promise
/// on the agent of the call. A parameter or a return whose type is neither
/// refuses at compile time with the message of the trait.
///
/// The attribute refuses an argument, an item that is no `impl` block, a trait
/// implementation, a method with generic parameters, a receiver other than
/// `&self`, a parameter that is no plain identifier, an unknown key, a
/// `#[doc]` whose value is no string literal, a `#[deprecated]` on a method,
/// and a
/// parameter or a return whose type holds a tuple other than `()`, a function
/// pointer, `impl Trait` or `dyn Trait`.
///
/// # Examples
///
/// ```ignore
/// #[aosabi::resolvers]
/// impl Query {
///     /// Greets the caller.
///     pub fn greeting(&self) -> String {
///         String::from("Hello")
///     }
///
///     pub async fn user(&self, id: f64, #[aosabi(default = 10.0)] first: f64) -> Option<User> {
///         find_user(id, first).await
///     }
/// }
/// ```
#[proc_macro_attribute]
pub fn resolvers(args: TokenStream, item: TokenStream) -> TokenStream {
    if !cfg!(feature = "mirror") {
        return retired::attribute(&item.into(), refuse::RETIRED_RESOLVERS).into();
    }
    resolvers::expand(&args.into(), &item.into()).into()
}

/// Describes a newtype as a custom scalar.
///
/// The attribute accepts a struct with exactly one unnamed field and no generic
/// parameter. The optional argument `name = "Name"` states the name of the
/// scalar, which is the identifier of the struct otherwise. The documentation
/// comment of the struct is the documentation of the type. The printer writes
/// the `scalar Name` line with no description, so the comment reaches the
/// schema only as the description of a non null input field typed by the
/// scalar, which takes the documentation of its type (quirk `Q15`). The built
/// in scalars have markers of their own in the facade, `ID`, `Int`, `Float`,
/// `Date`, `File`, `JSON` and `Any`, and a service names those.
///
/// The attribute emits the struct unchanged, `impl Described` with
/// `TypeKind::Scalar` and every list empty, and one `Declaration` constant
/// that the declaration list of the crate names, on every target, as the
/// derive does. The type of the one field is not linked, because a scalar is
/// opaque to the schema. For the run time it emits `impl IntoValue` and `impl FromArgument`,
/// both over the one field.
///
/// # Examples
///
/// ```ignore
/// /// A point in time.
/// #[aosabi::scalar(name = "Date")]
/// pub struct Timestamp(pub i64);
/// ```
#[proc_macro_attribute]
pub fn scalar(args: TokenStream, item: TokenStream) -> TokenStream {
    if !cfg!(feature = "mirror") {
        return retired::attribute(&item.into(), refuse::RETIRED_SCALAR).into();
    }
    scalar::expand(args.into(), &item.into()).into()
}

/// Declares the roots of an application as an object literal.
///
/// The macro is the twin of `export const graphql = {...}` of a TypeScript
/// service. Its input is `<vis> const <name> = { <root>, ... };`, where a root
/// is `Query`, `Mutation` or `Subscription` with a block of entries, a type
/// path, or its key alone, which names the struct of that name. An entry is a
/// closure, an `async` closure, a path to a static or to a function under
/// [`#[aosabi::resolver]`](macro@resolver), `mutation(...)` around any of them,
/// or `..path.Root`, which spreads the root of another `graphql!`. Doc comments
/// and [`#[decorate(...)]`](macro@decorate) stand above an entry, and the macro
/// takes the decorators off and hands them to the glue in source order.
///
/// The macro emits a hidden module with one function per inline entry, the
/// descriptor of each root and the run time of the roots, and the constant of
/// the given name, whose type the build reads. A token that breaks the grammar
/// is refused with a compile error at that token.
///
/// # Examples
///
/// ```ignore
/// aosabi::graphql! {
///     pub const graphql = {
///         Query: {
///             /// Greets the caller.
///             greeting: || -> String { String::from("Hello") },
///         },
///     };
/// }
/// ```
#[proc_macro]
pub fn graphql(input: TokenStream) -> TokenStream {
    if !cfg!(feature = "mirror") {
        return retired::call(refuse::RETIRED_GRAPHQL).into();
    }
    graphql::entry(&input.into()).into()
}

/// Describes a free function as a resolver an entry of `graphql!` names.
///
/// The attribute emits the function unchanged and, beside it, a hidden module
/// of the same name with the descriptor of the function, which a reference or
/// a shorthand entry of [`graphql!`](macro@graphql) reaches. It stands above
/// any [`#[decorate(...)]`](macro@decorate) of the function and takes those
/// off, because an attribute macro expands before the attributes below it.
///
/// # Examples
///
/// ```ignore
/// #[aosabi::resolver]
/// pub async fn stock(id: ID) -> Result<f64> {
///     Ok(1.0)
/// }
/// ```
#[proc_macro_attribute]
pub fn resolver(args: TokenStream, item: TokenStream) -> TokenStream {
    if !cfg!(feature = "mirror") {
        return retired::attribute(&item.into(), refuse::RETIRED_RESOLVER).into();
    }
    resolver::expand(&args.into(), &item.into()).into()
}

/// Decorates an entry, a method, a static or a free function.
///
/// The attribute is the twin of a TypeScript decorator made by
/// `createDecorator`, and its argument is the expression of the decorator.
/// [`graphql!`](macro@graphql), [`#[aosabi::resolvers]`](macro@resolvers) and
/// [`#[aosabi::resolver]`](macro@resolver) take it off the items they read, so
/// it never expands where it belongs. Where it expands, it stands under none
/// of them, and it answers a compile error that names the sites it belongs on.
#[proc_macro_attribute]
pub fn decorate(args: TokenStream, item: TokenStream) -> TokenStream {
    if !cfg!(feature = "mirror") {
        return retired::attribute(&item.into(), refuse::RETIRED_DECORATE).into();
    }
    decorate::refuse_outside(&args.into(), &item.into()).into()
}

/// Writes a plugin as an object literal.
///
/// The keys are the eight members of a plugin in snake case and every
/// hook name of the plugin contract, and an unknown key is refused at its
/// token. A `setup`, a `build` or a `middleware` that is an `async` closure
/// goes through the asynchronous helper of the glue and any other closure
/// through the synchronous one. The macro expands to a `PluginObject` literal
/// with `..Default::default()`.
///
/// The keys `roles`, `strategy` and `depends_on` take the text of the
/// reference, `roles: ["web"]`, `strategy: "last"` and
/// `depends_on: ["identity"]`, which the macro reads at compile time, and it
/// refuses a role other than `"web"` and `"worker"`, a strategy other than
/// `"first"` and `"last"` and a dependency that is no string literal at its
/// token. Any other value of the three keys, a computed list or
/// `Strategy::Last`, goes through the helper of its key, which wraps it in
/// `Some`. The closure of `on_execute` answers `on_execute_done(...)` or a
/// `Result` of it.
///
/// # Examples
///
/// ```ignore
/// let plugin = aosabi::plugin! {
///     name: "greeter",
///     setup: |app| {
///         app.get("/hello", |c| async move { c.text("hello") });
///     },
/// };
/// ```
#[proc_macro]
pub fn plugin(input: TokenStream) -> TokenStream {
    if !cfg!(feature = "mirror") {
        return retired::call(refuse::RETIRED_PLUGIN).into();
    }
    plugin::object(&input.into()).into()
}

/// Writes the plugin list of a configuration.
///
/// The macro expands `plugins![a, b]` to `Some(PluginList)` and records the
/// source text and the line of every element, which the boot report of a
/// served binary writes.
///
/// # Examples
///
/// ```ignore
/// plugins: aosabi::plugins![use_greeter(), use_logger()],
/// ```
#[proc_macro]
pub fn plugins(input: TokenStream) -> TokenStream {
    if !cfg!(feature = "mirror") {
        return retired::call(refuse::RETIRED_PLUGINS).into();
    }
    plugin::list(&input.into()).into()
}

/// Reads the bindings of a service from the environment of its context.
///
/// The derive stands on a struct with named fields and emits `current()`,
/// which reads every field of type `String` or `Option<String>` from the
/// environment of the context of the request. Any other field type is refused
/// with a compile error.
///
/// # Examples
///
/// ```ignore
/// #[derive(aosabi::Bindings)]
/// pub struct Bindings {
///     pub database_url: String,
/// }
/// ```
#[proc_macro_derive(Bindings)]
pub fn derive_bindings(input: TokenStream) -> TokenStream {
    if !cfg!(feature = "mirror") {
        return retired::derive(&input.into(), refuse::RETIRED_BINDINGS).into();
    }
    bindings::expand(&input.into()).into()
}

/// Declares the variables of a context as typed keys.
///
/// The derive stands on a struct with named fields and emits one associated
/// constant per field, a `Key` of the field's type named after the field, and
/// `impl Variables` for the struct.
///
/// # Examples
///
/// ```ignore
/// #[derive(aosabi::Variables)]
/// pub struct Variables {
///     pub user_id: String,
/// }
/// ```
#[proc_macro_derive(Variables)]
pub fn derive_variables(input: TokenStream) -> TokenStream {
    if !cfg!(feature = "mirror") {
        return retired::derive(&input.into(), refuse::RETIRED_VARIABLES).into();
    }
    variables::expand(&input.into()).into()
}
