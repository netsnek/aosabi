// SPDX-License-Identifier: MIT
/*
 * aosabi-macros: the text of every refusal of the macros.
 *
 * The TypeScript inference is silent where these macros refuse, and
 * docs/schema-inference.md section 13 lists each refusal as a deliberate
 * deviation.
 */

//! The text of every refusal of the macros.
//!
//! A refusal is a `compile_error!` at the span of the tokens that cause it.
//! Every message stands here as one string literal, so a reader who meets a
//! message in a build log finds its one source with grep.

use proc_macro2::{
    Span,
    TokenStream, //
};

/// The derive stands on a `union`.
pub(crate) const DERIVE_ON_UNION: &str =
    "`#[derive(Aosabi)]` describes a struct or an enum, and a `union` has no GraphQL description";

/// The derive stands on a struct with unnamed fields.
pub(crate) const DERIVE_ON_TUPLE_STRUCT: &str = "`#[derive(Aosabi)]` describes a struct with named fields or a unit struct, so name the fields, or mark a newtype with `#[aosabi::scalar]`";

/// A variant has a shape the enum does not share, or a shape no enum may have.
pub(crate) const ENUM_MIXES_VARIANT_SHAPES: &str = "`#[derive(Aosabi)]` describes an enum whose variants all carry no data, or whose variants all carry exactly one unnamed field, and this variant has another shape";

/// A variant with a payload states a value.
pub(crate) const VALUE_ON_PAYLOAD_VARIANT: &str =
    "`value` belongs to a variant that carries no data, and this variant carries a payload";

/// A described type or a resolvers block has a lifetime or a const parameter.
pub(crate) const LIFETIME_OR_CONST_PARAMETER: &str =
    "a described type takes type parameters only, and this is a lifetime or a const parameter";

/// A tuple other than `()` stands in a field, a parameter or a return.
pub(crate) const TUPLE_TYPE: &str = "a tuple has no GraphQL description, write `Vec<T>` for a list or a struct with named fields for a record";

/// A function pointer stands in a field, a parameter or a return.
pub(crate) const FUNCTION_TYPE: &str =
    "a function has no GraphQL description, so Aosabi refuses it instead of skipping it in silence";

/// An `impl Trait` or a `dyn Trait` stands in a field, a parameter or a return.
pub(crate) const UNNAMEABLE_TYPE: &str = "a type that cannot be named has no GraphQL description, write the concrete type in place of `impl Trait` or `dyn Trait`";

/// `#[aosabi::resolvers]` stands on an item that is no `impl` block.
pub(crate) const RESOLVERS_NOT_ON_IMPL: &str =
    "`#[aosabi::resolvers]` belongs on an inherent `impl` block";

/// `#[aosabi::resolvers]` stands on a trait implementation.
pub(crate) const RESOLVERS_ON_TRAIT_IMPL: &str =
    "`#[aosabi::resolvers]` belongs on an inherent `impl` block, and this block implements a trait";

/// A type states the key `resolvers`.
pub(crate) const RESOLVERS_KEY: &str = "the derive reads the method table of the `#[aosabi::resolvers]` block by path and takes no key `resolvers`, so remove the key";

/// A type states a base as `extends(...)`.
pub(crate) const TYPE_EXTENDS: &str = "a base is a field of the base type under `#[aosabi(extends)]`, which the struct embeds, so state the base as that field and not as `extends(...)` on the type";

/// `#[aosabi::resolvers]` carries an argument.
pub(crate) const RESOLVERS_WITH_ARGUMENT: &str = "`#[aosabi::resolvers]` takes no argument";

/// A resolver has generic parameters.
pub(crate) const METHOD_WITH_GENERICS: &str =
    "a resolver takes no generic parameters, because a GraphQL field has one type";

/// A resolver takes `self`, `&mut self` or a typed receiver.
pub(crate) const RECEIVER_NOT_REF_SELF: &str = "the receiver of a resolver is `&self`";

/// A resolver parameter is a pattern other than one identifier.
pub(crate) const PARAMETER_NOT_IDENTIFIER: &str =
    "a resolver parameter is one identifier, because its name is the name of the GraphQL argument";

/// An unknown key stands in `#[aosabi(...)]` on a type.
pub(crate) const UNKNOWN_TYPE_KEY: &str = "unknown key in `#[aosabi(...)]` on a type, the keys are `typename = \"Name\"`, `anonymous`, `enum`, `implements(Base, ...)`, `remote`, `arguments` and `select = Type`";

/// The key `remote` stands on a struct with no field `__typename`.
pub(crate) const REMOTE_WITHOUT_TYPENAME: &str = "a type of a pulled registry keeps the remote object it was read from in its field `__typename: TypeName`, and this struct has no field `__typename`";

/// The key `remote` stands on a unit enum, a unit struct or beside a key
/// that shapes the type another way.
pub(crate) const REMOTE_SHAPE: &str = "`remote` belongs on a struct with named fields or on an enum whose variants each carry one payload, the object, the interface or the union of a pulled registry, and on no struct with an embedded base or a spread";

/// The key `arguments` stands on an enum.
pub(crate) const ARGUMENTS_SHAPE: &str = "`arguments` belongs on a struct, the arguments of a remote field in the order the remote declares them";

/// The key `enum` stands on a struct, on an enum whose variants carry a
/// payload or beside `anonymous`.
pub(crate) const ENUM_KEY_NOT_ON_UNIT_ENUM: &str = "`enum` belongs on an enum whose variants carry no data, the twin of a TypeScript `enum` declaration, which always has a name";

/// A `#[doc]` on a described item has a value that is no string literal.
pub(crate) const DOC_NOT_LITERAL: &str = "the doc comment of a described item is text of the schema, and the macro reads a string literal only, so write the text as `///` lines";

/// `#[deprecated]` stands on a described item.
pub(crate) const DEPRECATED_ON_DESCRIBED: &str = "`#[deprecated]` has no GraphQL form, because Aosabi emits no deprecation, so write `@deprecated` in the doc comment";

/// A key stands in `#[aosabi(...)]` on a field.
pub(crate) const UNKNOWN_FIELD_KEY: &str =
    "unknown key in `#[aosabi(...)]` on a field, the keys are `extends` and `spread`";

/// A field carries both `#[aosabi(extends)]` and `#[aosabi(spread)]`.
pub(crate) const SPREAD_AND_EXTENDS: &str = "a field stands for an embedded base or for an object spread, and this one carries both `extends` and `spread`";

/// A struct carries a field under `#[aosabi(spread)]` beside one under
/// `#[aosabi(extends)]`.
pub(crate) const SPREAD_BESIDE_BASE: &str = "a struct with a field under `#[aosabi(spread)]` is the twin of an object literal, which extends no class, so it embeds no base under `#[aosabi(extends)]`";

/// Two fields of a struct carry `#[aosabi(extends)]`.
pub(crate) const SECOND_EMBEDDED_BASE: &str =
    "a struct extends one base, as a class does, and another field carries `#[aosabi(extends)]`";

/// A field under `#[aosabi(extends)]` has no name.
pub(crate) const EMBEDDED_BASE_UNNAMED: &str =
    "the base of a struct is a named field under `#[aosabi(extends)]`";

/// An unknown key stands in `#[aosabi(...)]` on a variant.
pub(crate) const UNKNOWN_VARIANT_KEY: &str =
    "unknown key in `#[aosabi(...)]` on a variant, the key is `value = \"text\"`";

/// A key stands in `#[aosabi(...)]` on a resolver.
pub(crate) const UNKNOWN_METHOD_KEY: &str =
    "unknown key in `#[aosabi(...)]` on a resolver, which takes no key";

/// An unknown key stands in `#[aosabi(...)]` on a parameter.
pub(crate) const UNKNOWN_PARAMETER_KEY: &str =
    "unknown key in `#[aosabi(...)]` on a parameter, the key is `default = expr`";

/// An unknown key stands in the argument list of `#[aosabi::scalar]`.
pub(crate) const UNKNOWN_SCALAR_KEY: &str =
    "unknown key in `#[aosabi::scalar(...)]`, the key is `name = \"Name\"`";

/// A key that takes one value stands twice.
pub(crate) const DUPLICATE_KEY: &str = "this key stands twice, and it takes one value";

/// `#[aosabi::scalar]` stands on an item that is no newtype.
pub(crate) const SCALAR_NOT_NEWTYPE: &str =
    "`#[aosabi::scalar]` belongs on a struct with exactly one unnamed field";

/// `#[aosabi::scalar]` stands on a struct with generic parameters.
pub(crate) const SCALAR_WITH_GENERICS: &str =
    "a scalar takes no generic parameters, because a GraphQL scalar has one name";

/// The build script of the crate writes declaration lists and wrote none for
/// the target the derive or the scalar attribute expands in.
pub(crate) const NO_LIST_FOR_TARGET: &str = "the build script of this crate wrote no declaration list for this target, which is newer than the last run of the script or stated in a form of the manifest the script does not read, so touch `Cargo.toml` and build again, and if the error stays, write the table of the target with one key per line";

/// The input of `graphql!` is no constant.
pub(crate) const GRAPHQL_NOT_CONST: &str =
    "`graphql!` takes one constant, `pub const graphql = { Query: { ... }, ... };`";

/// A key of the literal of `graphql!` is no root.
pub(crate) const GRAPHQL_UNKNOWN_ROOT: &str =
    "a key of the roots is `Query`, `Mutation` or `Subscription`";

/// A root of `graphql!` stands twice.
pub(crate) const GRAPHQL_DUPLICATE_ROOT: &str =
    "this root stands twice, and the literal states each root once";

/// An entry of `graphql!` carries an attribute other than a doc comment or a
/// decorator.
pub(crate) const GRAPHQL_ENTRY_ATTRIBUTE: &str =
    "an entry takes doc comments and `#[decorate(...)]`, and no other attribute";

/// A spread of `graphql!` carries an attribute.
pub(crate) const GRAPHQL_SPREAD_ATTRIBUTE: &str =
    "a spread takes no attribute, because it is no field";

/// An inline entry of `graphql!` states no return type.
pub(crate) const GRAPHQL_INLINE_RETURN: &str = "an inline entry states its return type, `|a: A| -> R { ... }`, because the type is the type of the field";

/// A parameter of an inline entry of `graphql!` states no type or is no
/// identifier.
pub(crate) const GRAPHQL_INLINE_PARAMETER: &str = "a parameter of an inline entry is one identifier with its type, `id: ID`, because its name is the name of the GraphQL argument";

/// An inline entry of `graphql!` is a `const` or a `static` closure, or
/// binds lifetimes.
pub(crate) const GRAPHQL_INLINE_MODIFIER: &str =
    "an inline entry is `|...| -> R { ... }` or `async |...| -> R { ... }`";

/// The value of an entry of `graphql!` is no closure, path or mutation.
pub(crate) const GRAPHQL_ENTRY_VALUE: &str = "the value of an entry is a closure, an `async` closure, the path of a static or of a function under `#[aosabi::resolver]`, or `mutation(...)` around one of them";

/// A reference of `graphql!` names a trait item.
pub(crate) const GRAPHQL_TRAIT_PATH: &str =
    "an entry names a static of a type or a function, and `<T as Trait>::f` names a trait item";

/// A spread of `graphql!` names no root of a fragment.
pub(crate) const GRAPHQL_SPREAD_FORM: &str =
    "a spread is `..module::graphql.Query`, the path of a fragment and one of its roots";

/// A `#[decorate]` carries no expression.
pub(crate) const DECORATE_FORM: &str = "`#[decorate(expr)]` takes the expression of one decorator";

/// `#[aosabi::resolver]` stands on an item that is no free function.
pub(crate) const RESOLVER_NOT_ON_FN: &str = "`#[aosabi::resolver]` belongs on a free function";

/// `#[aosabi::resolver]` carries an argument.
pub(crate) const RESOLVER_WITH_ARGUMENT: &str = "`#[aosabi::resolver]` takes no argument";

/// A key of `plugin!` is no member of a plugin and no hook.
pub(crate) const PLUGIN_UNKNOWN_KEY: &str = "unknown key of a plugin, the keys are `name`, `roles`, `strategy`, `depends_on`, `options`, `setup`, `build`, `middleware` and the hooks of the plugin contract in snake case";

/// A key of `plugin!` stands twice.
pub(crate) const PLUGIN_DUPLICATE_KEY: &str =
    "this key stands twice, and a plugin states each key once";

/// A second spread stands in `plugin!`.
pub(crate) const PLUGIN_SECOND_SPREAD: &str =
    "a plugin holds one spread `..other`, and this is a second one";

/// An element of the literal form of `roles` is no role of the reference.
pub(crate) const PLUGIN_ROLE: &str = "a plugin role is \"web\" or \"worker\"";

/// The literal form of `strategy` is no strategy of the reference.
pub(crate) const PLUGIN_STRATEGY: &str = "a plugin strategy is \"first\" or \"last\"";

/// An element of the literal form of `depends_on` is no string literal.
pub(crate) const PLUGIN_DEPENDENCY: &str =
    "a plugin dependency is the name of a plugin as a string literal";

/// A derive of `Bindings` or `Variables` stands on no struct with named fields.
pub(crate) const CONTEXT_DERIVE_NOT_NAMED: &str =
    "the derive belongs on a struct with named fields";

/// A field of a `Bindings` struct has a type the environment cannot answer.
pub(crate) const BINDING_FIELD_TYPE: &str = "a binding is `String` or `Option<String>` until M12, which reads the typed bindings of a platform";

/// `graphql!` stands outside the feature `mirror`.
pub(crate) const RETIRED_GRAPHQL: &str = "`graphql!` is retired, and an application registers its roots on the chain of its entry, a public function of the crate root that answers an app, `App::new().query(Query)`, as docs/authoring.md section 4 writes it";

/// `#[derive(Aosabi)]` stands outside the feature `mirror`.
pub(crate) const RETIRED_DERIVE: &str = "`#[derive(Aosabi)]` is retired, and the builder reads a plain struct or enum from the source with no derive, as docs/authoring.md sections 8 and 17 write it";

/// `#[aosabi::resolvers]` stands outside the feature `mirror`.
pub(crate) const RETIRED_RESOLVERS: &str = "`#[aosabi::resolvers]` is retired, and the builder reads the methods of a plain inherent `impl` block from the source, as docs/authoring.md section 9 writes it";

/// `#[aosabi::resolver]` stands outside the feature `mirror`.
pub(crate) const RETIRED_RESOLVER: &str = "`#[aosabi::resolver]` is retired, and a method of the root calls the function, as docs/authoring.md section 12 writes it";

/// `#[aosabi::scalar]` stands outside the feature `mirror`.
pub(crate) const RETIRED_SCALAR: &str = "`#[aosabi::scalar]` is retired with no native form, so a value takes a marker of the facade or `json::Map`, as docs/authoring.md section 20.4 writes it";

/// `#[decorate(..)]` stands outside the feature `mirror`.
pub(crate) const RETIRED_DECORATE: &str = "`#[decorate(..)]` is retired, and the body of a method calls its guard function first, as docs/authoring.md section 23 writes it";

/// `plugin!` stands outside the feature `mirror`.
pub(crate) const RETIRED_PLUGIN: &str = "`plugin!` is retired, and a plugin is built with `PluginObject::new(name)` and its calls or is a type that implements `Plugin`, as docs/authoring.md section 7 writes it";

/// `plugins!` stands outside the feature `mirror`.
pub(crate) const RETIRED_PLUGINS: &str = "`plugins!` is retired, and the chain of the entry registers each plugin with `.plugin(..)`, as docs/authoring.md section 7 writes it";

/// `#[derive(Bindings)]` stands outside the feature `mirror`.
pub(crate) const RETIRED_BINDINGS: &str = "`#[derive(Bindings)]` is retired, and a service reads its environment with `get_env().var(..)`, as docs/authoring.md section 22 writes it";

/// `#[derive(Variables)]` stands outside the feature `mirror`.
pub(crate) const RETIRED_VARIABLES: &str = "`#[derive(Variables)]` is retired, and a service declares each variable of a request as a `Key` constant, as docs/authoring.md section 22 writes it";

/// The key `typename` stands outside the feature `mirror`.
pub(crate) const RETIRED_KEY_TYPENAME: &str = "the key `typename` is retired, and a type takes the name of its struct or of a variant of a payload enum, as docs/authoring.md section 16 writes it";

/// The key `anonymous` stands outside the feature `mirror`.
pub(crate) const RETIRED_KEY_ANONYMOUS: &str = "the key `anonymous` is retired, and every type of the native layer is a named struct, as docs/authoring.md section 8 writes it";

/// The key `enum` stands outside the feature `mirror`.
pub(crate) const RETIRED_KEY_ENUM: &str = "the key `enum` is retired, and the builder reads the doc comment of every enum of unit variants, as docs/authoring.md section 17 writes it";

/// The key `implements` stands outside the feature `mirror`.
pub(crate) const RETIRED_KEY_IMPLEMENTS: &str = "the key `implements` is retired with no native form, as docs/authoring.md section 14 writes it";

/// The key `extends` stands outside the feature `mirror`.
pub(crate) const RETIRED_KEY_EXTENDS: &str = "the key `extends` is retired, and a struct writes the shared fields of its base, as docs/authoring.md section 14 writes it";

/// The key `spread` stands outside the feature `mirror`.
pub(crate) const RETIRED_KEY_SPREAD: &str = "the key `spread` is retired with no native form yet, as docs/authoring.md section 28 writes it";

/// The key `value` stands outside the feature `mirror`.
pub(crate) const RETIRED_KEY_VALUE: &str = "the key `value` is retired, and a variant of an enum takes its verbatim spelling, as docs/authoring.md section 17 writes it";

/// The key `default` stands outside the feature `mirror`.
pub(crate) const RETIRED_KEY_DEFAULT: &str = "the key `default` is retired, and a parameter takes `Option<T>` with `unwrap_or` in the body, as docs/authoring.md section 19 writes it";

/// One of the keys of a pulled registry stands outside the feature `mirror`.
pub(crate) const RETIRED_KEY_REGISTRY: &str = "the keys `remote`, `arguments` and `select` belong to the registries `aosabi pull` writes, which stay on the derive path until wave 2, as docs/authoring.md section 28 writes it";

/// A key no form of the derive path knows stands outside the feature
/// `mirror`.
pub(crate) const RETIRED_KEY_OTHER: &str = "the attribute `aosabi(..)` is retired, and the native layer writes no key, as docs/authoring.md section 1.4 writes it";

/// Returns the refusal with `message` at `span`, for a caller that returns a `Result`.
pub(crate) fn refusal(span: Span, message: &str) -> syn::Error {
    syn::Error::new(span, message)
}

/// Returns the `compile_error!` with `message` at `span`.
pub(crate) fn refuse(span: Span, message: &str) -> TokenStream {
    refusal(span, message).to_compile_error()
}
