// SPDX-License-Identifier: MIT
/*
 * aosabi-type: the introspection types, their resolvers and the three meta
 * fields.
 *
 * Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
 */

//! The introspection schema of the type layer.
//!
//! The eight introspection types `__Schema`, `__Directive`,
//! `__DirectiveLocation`, `__Type`, `__Field`, `__InputValue`, `__EnumValue`
//! and `__TypeKind` are module level constants of the reference. Here each is
//! a row of read only data, with its fields or its values, and one static
//! holds the eight types and the `__schema` and `__type` fields built from
//! the rows on first use. Functions return the one shared allocation of each,
//! so that the `__Schema` a schema collects last is the `__Schema` the
//! introspection query resolves against. The field maps are thunks, as in the
//! reference, because `__Type` refers to itself and to `__Field`, which
//! refers back to it. The field order of every type and the value order of
//! both enums are the reference's, because an introspection response lists
//! fields in map order.
//!
//! # The resolvers
//!
//! The reference writes each resolver as an arrow function in the field
//! config, and the executor calls it with the source value. The `resolve`
//! slot of Aosabi is a [`ResolverHandle`] whose signature the executor
//! decides, so each introspection field borrows one variant of
//! [`IntrospectionResolver`] from its row, and the executor downcasts the
//! handle to it and calls the function of this module the variant names, one
//! per resolver of the reference and named after its type and field. A
//! resolver that reads a thunk returns a `Result`, because the thunk may
//! throw.
//!
//! # The constructors
//!
//! The constructor of an object type and of an enum type asserts the name,
//! the definition of the enum values asserts every value name, and the eight
//! names and their value names are literals that pass. A library crate of
//! Aosabi never panics, so each getter carries the `Result` of its
//! constructor rather than unwrapping it, and the error variant is never
//! produced.

use alloc::{
    boxed::Box,
    string::String,
    sync::Arc,
    vec::Vec, //
};
use core::fmt;

use aosabi_json::{
    JsObject,
    JsValue, //
};
use aosabi_lang::printer::print;
use once_cell::race::OnceBox;

use crate::{
    ast_from_value::ast_from_value,
    definition::{
        Argument,
        ArgumentConfig,
        DefinitionError,
        EnumType,
        EnumTypeConfig,
        EnumValue,
        EnumValueConfig,
        Field,
        FieldConfig,
        InputField,
        ListType,
        NamedType,
        NonNullType,
        ObjMap,
        ObjectType,
        ObjectTypeConfig,
        ResolverHandle,
        ThunkObjMap,
        Type, //
    },
    directives::{
        Directive,
        DirectiveLocation, //
    },
    scalars::{
        graphql_boolean,
        graphql_string, //
    },
    schema::Schema, //
};

/// The resolver of one introspection field, the value of its `resolve` slot.
///
/// Each variant names the function of this module that is the translation of
/// the arrow function of the reference, and the executor calls that function
/// with the source value it holds. The order is the reference's file order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IntrospectionResolver {
    /// `__Schema.description`, [`schema_description()`].
    SchemaDescription,
    /// `__Schema.types`, [`schema_types()`].
    SchemaTypes,
    /// `__Schema.queryType`, [`schema_query_type()`].
    SchemaQueryType,
    /// `__Schema.mutationType`, [`schema_mutation_type()`].
    SchemaMutationType,
    /// `__Schema.subscriptionType`, [`schema_subscription_type()`].
    SchemaSubscriptionType,
    /// `__Schema.directives`, [`schema_directives()`].
    SchemaDirectives,
    /// `__Directive.name`, [`directive_name()`].
    DirectiveName,
    /// `__Directive.description`, [`directive_description()`].
    DirectiveDescription,
    /// `__Directive.isRepeatable`, [`directive_is_repeatable()`].
    DirectiveIsRepeatable,
    /// `__Directive.locations`, [`directive_locations()`].
    DirectiveLocations,
    /// `__Directive.args`, [`directive_args()`].
    DirectiveArgs,
    /// `__Type.kind`, [`type_kind()`].
    TypeKind,
    /// `__Type.name`, [`type_name()`].
    TypeName,
    /// `__Type.description`, [`type_description()`].
    TypeDescription,
    /// `__Type.specifiedByURL`, [`type_specified_by_url()`].
    TypeSpecifiedByUrl,
    /// `__Type.fields`, [`type_fields()`].
    TypeFields,
    /// `__Type.interfaces`, [`type_interfaces()`].
    TypeInterfaces,
    /// `__Type.possibleTypes`, [`type_possible_types()`].
    TypePossibleTypes,
    /// `__Type.enumValues`, [`type_enum_values()`].
    TypeEnumValues,
    /// `__Type.inputFields`, [`type_input_fields()`].
    TypeInputFields,
    /// `__Type.ofType`, [`type_of_type()`].
    TypeOfType,
    /// `__Type.isOneOf`, [`type_is_one_of()`].
    TypeIsOneOf,
    /// `__Field.name`, [`field_name()`].
    FieldName,
    /// `__Field.description`, [`field_description()`].
    FieldDescription,
    /// `__Field.args`, [`field_args()`].
    FieldArgs,
    /// `__Field.type`, [`field_type()`].
    FieldType,
    /// `__Field.isDeprecated`, [`field_is_deprecated()`].
    FieldIsDeprecated,
    /// `__Field.deprecationReason`, [`field_deprecation_reason()`].
    FieldDeprecationReason,
    /// `__InputValue.name`, [`input_value_name()`].
    InputValueName,
    /// `__InputValue.description`, [`input_value_description()`].
    InputValueDescription,
    /// `__InputValue.type`, [`input_value_type()`].
    InputValueType,
    /// `__InputValue.defaultValue`, [`input_value_default_value()`].
    InputValueDefaultValue,
    /// `__InputValue.isDeprecated`, [`input_value_is_deprecated()`].
    InputValueIsDeprecated,
    /// `__InputValue.deprecationReason`, [`input_value_deprecation_reason()`].
    InputValueDeprecationReason,
    /// `__EnumValue.name`, [`enum_value_name()`].
    EnumValueName,
    /// `__EnumValue.description`, [`enum_value_description()`].
    EnumValueDescription,
    /// `__EnumValue.isDeprecated`, [`enum_value_is_deprecated()`].
    EnumValueIsDeprecated,
    /// `__EnumValue.deprecationReason`, [`enum_value_deprecation_reason()`].
    EnumValueDeprecationReason,
    /// `__schema`, [`schema_meta_field()`].
    SchemaMetaField,
    /// `__type`, [`type_meta_field()`].
    TypeMetaField,
    /// `__typename`, [`type_name_meta_field()`].
    TypeNameMetaField,
}

/// A named type an introspection field refers to, before it is wrapped.
#[derive(Clone, Copy)]
enum Named {
    /// The specified `String`.
    String,
    /// The specified `Boolean`.
    Boolean,
    /// `__Directive`.
    Directive,
    /// `__DirectiveLocation`.
    DirectiveLocation,
    /// `__Type`.
    Type,
    /// `__Field`.
    Field,
    /// `__InputValue`.
    InputValue,
    /// `__EnumValue`.
    EnumValue,
    /// `__TypeKind`.
    TypeKind,
}

/// How an introspection field wraps the type it names.
#[derive(Clone, Copy)]
enum Wrapping {
    /// `X`.
    Plain,
    /// `X!`.
    NonNull,
    /// `[X!]`.
    ListOfNonNull,
    /// `[X!]!`.
    NonNullListOfNonNull,
}

/// A field of an introspection object type as the image carries it.
struct FieldRow {
    /// The name.
    name: &'static str,
    /// The description.
    description: Option<&'static str>,
    /// The named type of the field.
    named: Named,
    /// How the type of the field wraps the named type.
    wrapping: Wrapping,
    /// Whether the field takes the `includeDeprecated` argument, whose type
    /// is `Boolean` and whose default value is `false`.
    include_deprecated: bool,
    /// The resolver, which the field borrows from this row.
    resolve: IntrospectionResolver,
}

/// An introspection object type as the image carries it.
struct ObjectRow {
    /// The name.
    name: &'static str,
    /// The description.
    description: &'static str,
    /// The fields, in the reference's order.
    fields: &'static [FieldRow],
}

/// An introspection enum type as the image carries it.
struct EnumRow {
    /// The name.
    name: &'static str,
    /// The description.
    description: &'static str,
    /// The name and the description of each value, in the reference's order.
    /// The internal value of a value is its name.
    values: &'static [(&'static str, &'static str)],
}

/// A meta field as the image carries it.
///
/// The type and the arguments of a meta field name types that are built at
/// runtime, so they are written where the field is built.
struct MetaFieldRow {
    /// The name.
    name: &'static str,
    /// The description.
    description: &'static str,
    /// The resolver, which the field borrows from this row.
    resolve: IntrospectionResolver,
}

/// `__Schema`.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
static SCHEMA: ObjectRow = ObjectRow {
    name: "__Schema",
    description: "A GraphQL Schema defines the capabilities of a GraphQL server. It exposes all available types and directives on the server, as well as the entry points for query, mutation, and subscription operations.",
    fields: &[
        FieldRow {
            name: "description",
            description: None,
            named: Named::String,
            wrapping: Wrapping::Plain,
            include_deprecated: false,
            resolve: IntrospectionResolver::SchemaDescription,
        },
        FieldRow {
            name: "types",
            description: Some("A list of all types supported by this server."),
            named: Named::Type,
            wrapping: Wrapping::NonNullListOfNonNull,
            include_deprecated: false,
            resolve: IntrospectionResolver::SchemaTypes,
        },
        FieldRow {
            name: "queryType",
            description: Some("The type that query operations will be rooted at."),
            named: Named::Type,
            wrapping: Wrapping::NonNull,
            include_deprecated: false,
            resolve: IntrospectionResolver::SchemaQueryType,
        },
        FieldRow {
            name: "mutationType",
            description: Some(
                "If this server supports mutation, the type that mutation operations will be rooted at.",
            ),
            named: Named::Type,
            wrapping: Wrapping::Plain,
            include_deprecated: false,
            resolve: IntrospectionResolver::SchemaMutationType,
        },
        FieldRow {
            name: "subscriptionType",
            description: Some(
                "If this server support subscription, the type that subscription operations will be rooted at.",
            ),
            named: Named::Type,
            wrapping: Wrapping::Plain,
            include_deprecated: false,
            resolve: IntrospectionResolver::SchemaSubscriptionType,
        },
        FieldRow {
            name: "directives",
            description: Some("A list of all directives supported by this server."),
            named: Named::Directive,
            wrapping: Wrapping::NonNullListOfNonNull,
            include_deprecated: false,
            resolve: IntrospectionResolver::SchemaDirectives,
        },
    ],
};

/// `__Directive`.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
static DIRECTIVE: ObjectRow = ObjectRow {
    name: "__Directive",
    description: "A Directive provides a way to describe alternate runtime execution and type validation behavior in a GraphQL document.\n\nIn some cases, you need to provide options to alter GraphQL's execution behavior in ways field arguments will not suffice, such as conditionally including or skipping a field. Directives provide this by describing additional information to the executor.",
    fields: &[
        FieldRow {
            name: "name",
            description: None,
            named: Named::String,
            wrapping: Wrapping::NonNull,
            include_deprecated: false,
            resolve: IntrospectionResolver::DirectiveName,
        },
        FieldRow {
            name: "description",
            description: None,
            named: Named::String,
            wrapping: Wrapping::Plain,
            include_deprecated: false,
            resolve: IntrospectionResolver::DirectiveDescription,
        },
        FieldRow {
            name: "isRepeatable",
            description: None,
            named: Named::Boolean,
            wrapping: Wrapping::NonNull,
            include_deprecated: false,
            resolve: IntrospectionResolver::DirectiveIsRepeatable,
        },
        FieldRow {
            name: "locations",
            description: None,
            named: Named::DirectiveLocation,
            wrapping: Wrapping::NonNullListOfNonNull,
            include_deprecated: false,
            resolve: IntrospectionResolver::DirectiveLocations,
        },
        FieldRow {
            name: "args",
            description: None,
            named: Named::InputValue,
            wrapping: Wrapping::NonNullListOfNonNull,
            include_deprecated: true,
            resolve: IntrospectionResolver::DirectiveArgs,
        },
    ],
};

/// `__DirectiveLocation`.
///
/// Each value is the string of the [`DirectiveLocation`] it names, which is
/// its own name.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
static DIRECTIVE_LOCATION: EnumRow = EnumRow {
    name: "__DirectiveLocation",
    description: "A Directive can be adjacent to many parts of the GraphQL language, a __DirectiveLocation describes one such possible adjacencies.",
    values: &[
        (
            DirectiveLocation::Query.as_str(),
            "Location adjacent to a query operation.",
        ),
        (
            DirectiveLocation::Mutation.as_str(),
            "Location adjacent to a mutation operation.",
        ),
        (
            DirectiveLocation::Subscription.as_str(),
            "Location adjacent to a subscription operation.",
        ),
        (
            DirectiveLocation::Field.as_str(),
            "Location adjacent to a field.",
        ),
        (
            DirectiveLocation::FragmentDefinition.as_str(),
            "Location adjacent to a fragment definition.",
        ),
        (
            DirectiveLocation::FragmentSpread.as_str(),
            "Location adjacent to a fragment spread.",
        ),
        (
            DirectiveLocation::InlineFragment.as_str(),
            "Location adjacent to an inline fragment.",
        ),
        (
            DirectiveLocation::VariableDefinition.as_str(),
            "Location adjacent to a variable definition.",
        ),
        (
            DirectiveLocation::Schema.as_str(),
            "Location adjacent to a schema definition.",
        ),
        (
            DirectiveLocation::Scalar.as_str(),
            "Location adjacent to a scalar definition.",
        ),
        (
            DirectiveLocation::Object.as_str(),
            "Location adjacent to an object type definition.",
        ),
        (
            DirectiveLocation::FieldDefinition.as_str(),
            "Location adjacent to a field definition.",
        ),
        (
            DirectiveLocation::ArgumentDefinition.as_str(),
            "Location adjacent to an argument definition.",
        ),
        (
            DirectiveLocation::Interface.as_str(),
            "Location adjacent to an interface definition.",
        ),
        (
            DirectiveLocation::Union.as_str(),
            "Location adjacent to a union definition.",
        ),
        (
            DirectiveLocation::Enum.as_str(),
            "Location adjacent to an enum definition.",
        ),
        (
            DirectiveLocation::EnumValue.as_str(),
            "Location adjacent to an enum value definition.",
        ),
        (
            DirectiveLocation::InputObject.as_str(),
            "Location adjacent to an input object type definition.",
        ),
        (
            DirectiveLocation::InputFieldDefinition.as_str(),
            "Location adjacent to an input object field definition.",
        ),
    ],
};

/// `__Type`.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
static TYPE: ObjectRow = ObjectRow {
    name: "__Type",
    description: "The fundamental unit of any GraphQL Schema is the type. There are many kinds of types in GraphQL as represented by the `__TypeKind` enum.\n\nDepending on the kind of a type, certain fields describe information about that type. Scalar types provide no information beyond a name, description and optional `specifiedByURL`, while Enum types provide their values. Object and Interface types provide the fields they describe. Abstract types, Union and Interface, provide the Object types possible at runtime. List and NonNull types compose other types.",
    fields: &[
        FieldRow {
            name: "kind",
            description: None,
            named: Named::TypeKind,
            wrapping: Wrapping::NonNull,
            include_deprecated: false,
            resolve: IntrospectionResolver::TypeKind,
        },
        FieldRow {
            name: "name",
            description: None,
            named: Named::String,
            wrapping: Wrapping::Plain,
            include_deprecated: false,
            resolve: IntrospectionResolver::TypeName,
        },
        FieldRow {
            name: "description",
            description: None,
            named: Named::String,
            wrapping: Wrapping::Plain,
            include_deprecated: false,
            resolve: IntrospectionResolver::TypeDescription,
        },
        FieldRow {
            name: "specifiedByURL",
            description: None,
            named: Named::String,
            wrapping: Wrapping::Plain,
            include_deprecated: false,
            resolve: IntrospectionResolver::TypeSpecifiedByUrl,
        },
        FieldRow {
            name: "fields",
            description: None,
            named: Named::Field,
            wrapping: Wrapping::ListOfNonNull,
            include_deprecated: true,
            resolve: IntrospectionResolver::TypeFields,
        },
        FieldRow {
            name: "interfaces",
            description: None,
            named: Named::Type,
            wrapping: Wrapping::ListOfNonNull,
            include_deprecated: false,
            resolve: IntrospectionResolver::TypeInterfaces,
        },
        FieldRow {
            name: "possibleTypes",
            description: None,
            named: Named::Type,
            wrapping: Wrapping::ListOfNonNull,
            include_deprecated: false,
            resolve: IntrospectionResolver::TypePossibleTypes,
        },
        FieldRow {
            name: "enumValues",
            description: None,
            named: Named::EnumValue,
            wrapping: Wrapping::ListOfNonNull,
            include_deprecated: true,
            resolve: IntrospectionResolver::TypeEnumValues,
        },
        FieldRow {
            name: "inputFields",
            description: None,
            named: Named::InputValue,
            wrapping: Wrapping::ListOfNonNull,
            include_deprecated: true,
            resolve: IntrospectionResolver::TypeInputFields,
        },
        FieldRow {
            name: "ofType",
            description: None,
            named: Named::Type,
            wrapping: Wrapping::Plain,
            include_deprecated: false,
            resolve: IntrospectionResolver::TypeOfType,
        },
        FieldRow {
            name: "isOneOf",
            description: None,
            named: Named::Boolean,
            wrapping: Wrapping::Plain,
            include_deprecated: false,
            resolve: IntrospectionResolver::TypeIsOneOf,
        },
    ],
};

/// `__Field`.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
static FIELD: ObjectRow = ObjectRow {
    name: "__Field",
    description: "Object and Interface types are described by a list of Fields, each of which has a name, potentially a list of arguments, and a return type.",
    fields: &[
        FieldRow {
            name: "name",
            description: None,
            named: Named::String,
            wrapping: Wrapping::NonNull,
            include_deprecated: false,
            resolve: IntrospectionResolver::FieldName,
        },
        FieldRow {
            name: "description",
            description: None,
            named: Named::String,
            wrapping: Wrapping::Plain,
            include_deprecated: false,
            resolve: IntrospectionResolver::FieldDescription,
        },
        FieldRow {
            name: "args",
            description: None,
            named: Named::InputValue,
            wrapping: Wrapping::NonNullListOfNonNull,
            include_deprecated: true,
            resolve: IntrospectionResolver::FieldArgs,
        },
        FieldRow {
            name: "type",
            description: None,
            named: Named::Type,
            wrapping: Wrapping::NonNull,
            include_deprecated: false,
            resolve: IntrospectionResolver::FieldType,
        },
        FieldRow {
            name: "isDeprecated",
            description: None,
            named: Named::Boolean,
            wrapping: Wrapping::NonNull,
            include_deprecated: false,
            resolve: IntrospectionResolver::FieldIsDeprecated,
        },
        FieldRow {
            name: "deprecationReason",
            description: None,
            named: Named::String,
            wrapping: Wrapping::Plain,
            include_deprecated: false,
            resolve: IntrospectionResolver::FieldDeprecationReason,
        },
    ],
};

/// `__InputValue`.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
static INPUT_VALUE: ObjectRow = ObjectRow {
    name: "__InputValue",
    description: "Arguments provided to Fields or Directives and the input fields of an InputObject are represented as Input Values which describe their type and optionally a default value.",
    fields: &[
        FieldRow {
            name: "name",
            description: None,
            named: Named::String,
            wrapping: Wrapping::NonNull,
            include_deprecated: false,
            resolve: IntrospectionResolver::InputValueName,
        },
        FieldRow {
            name: "description",
            description: None,
            named: Named::String,
            wrapping: Wrapping::Plain,
            include_deprecated: false,
            resolve: IntrospectionResolver::InputValueDescription,
        },
        FieldRow {
            name: "type",
            description: None,
            named: Named::Type,
            wrapping: Wrapping::NonNull,
            include_deprecated: false,
            resolve: IntrospectionResolver::InputValueType,
        },
        FieldRow {
            name: "defaultValue",
            description: Some(
                "A GraphQL-formatted string representing the default value for this input value.",
            ),
            named: Named::String,
            wrapping: Wrapping::Plain,
            include_deprecated: false,
            resolve: IntrospectionResolver::InputValueDefaultValue,
        },
        FieldRow {
            name: "isDeprecated",
            description: None,
            named: Named::Boolean,
            wrapping: Wrapping::NonNull,
            include_deprecated: false,
            resolve: IntrospectionResolver::InputValueIsDeprecated,
        },
        FieldRow {
            name: "deprecationReason",
            description: None,
            named: Named::String,
            wrapping: Wrapping::Plain,
            include_deprecated: false,
            resolve: IntrospectionResolver::InputValueDeprecationReason,
        },
    ],
};

/// `__EnumValue`.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
static ENUM_VALUE: ObjectRow = ObjectRow {
    name: "__EnumValue",
    description: "One possible value for a given Enum. Enum values are unique values, not a placeholder for a string or numeric value. However an Enum value is returned in a JSON response as a string.",
    fields: &[
        FieldRow {
            name: "name",
            description: None,
            named: Named::String,
            wrapping: Wrapping::NonNull,
            include_deprecated: false,
            resolve: IntrospectionResolver::EnumValueName,
        },
        FieldRow {
            name: "description",
            description: None,
            named: Named::String,
            wrapping: Wrapping::Plain,
            include_deprecated: false,
            resolve: IntrospectionResolver::EnumValueDescription,
        },
        FieldRow {
            name: "isDeprecated",
            description: None,
            named: Named::Boolean,
            wrapping: Wrapping::NonNull,
            include_deprecated: false,
            resolve: IntrospectionResolver::EnumValueIsDeprecated,
        },
        FieldRow {
            name: "deprecationReason",
            description: None,
            named: Named::String,
            wrapping: Wrapping::Plain,
            include_deprecated: false,
            resolve: IntrospectionResolver::EnumValueDeprecationReason,
        },
    ],
};

/// The kind of a type, the value of `__Type.kind`.
///
/// Each variant carries the string the reference stores, which
/// [`TypeKind::as_str`] returns.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TypeKind {
    /// `SCALAR`.
    Scalar,
    /// `OBJECT`.
    Object,
    /// `INTERFACE`.
    Interface,
    /// `UNION`.
    Union,
    /// `ENUM`.
    Enum,
    /// `INPUT_OBJECT`.
    InputObject,
    /// `LIST`.
    List,
    /// `NON_NULL`.
    NonNull,
}

impl TypeKind {
    /// Returns the string of the kind, which is its enum value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Scalar => "SCALAR",
            Self::Object => "OBJECT",
            Self::Interface => "INTERFACE",
            Self::Union => "UNION",
            Self::Enum => "ENUM",
            Self::InputObject => "INPUT_OBJECT",
            Self::List => "LIST",
            Self::NonNull => "NON_NULL",
        }
    }
}

impl fmt::Display for TypeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// `__TypeKind`.
///
/// Each value is the string of the [`TypeKind`] it names, which is its own
/// name.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
static TYPE_KIND: EnumRow = EnumRow {
    name: "__TypeKind",
    description: "An enum describing what kind of type a given `__Type` is.",
    values: &[
        (
            TypeKind::Scalar.as_str(),
            "Indicates this type is a scalar.",
        ),
        (
            TypeKind::Object.as_str(),
            "Indicates this type is an object. `fields` and `interfaces` are valid fields.",
        ),
        (
            TypeKind::Interface.as_str(),
            "Indicates this type is an interface. `fields`, `interfaces`, and `possibleTypes` are valid fields.",
        ),
        (
            TypeKind::Union.as_str(),
            "Indicates this type is a union. `possibleTypes` is a valid field.",
        ),
        (
            TypeKind::Enum.as_str(),
            "Indicates this type is an enum. `enumValues` is a valid field.",
        ),
        (
            TypeKind::InputObject.as_str(),
            "Indicates this type is an input object. `inputFields` is a valid field.",
        ),
        (
            TypeKind::List.as_str(),
            "Indicates this type is a list. `ofType` is a valid field.",
        ),
        (
            TypeKind::NonNull.as_str(),
            "Indicates this type is a non-null. `ofType` is a valid field.",
        ),
    ],
};

/// `__schema`.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
static SCHEMA_META_FIELD: MetaFieldRow = MetaFieldRow {
    name: "__schema",
    description: "Access the current type schema of this server.",
    resolve: IntrospectionResolver::SchemaMetaField,
};

/// `__type`.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
static TYPE_META_FIELD: MetaFieldRow = MetaFieldRow {
    name: "__type",
    description: "Request the type information of a single type.",
    resolve: IntrospectionResolver::TypeMetaField,
};

/// `__typename`.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
static TYPE_NAME_META_FIELD: MetaFieldRow = MetaFieldRow {
    name: "__typename",
    description: "The name of the current Object type at runtime.",
    resolve: IntrospectionResolver::TypeNameMetaField,
};

/// Returns `X!` for a named type.
fn non_null(named: NamedType) -> Type {
    Type::from(NonNullType::from(named))
}

/// Returns `[X!]` for a named type.
fn list_of_non_null(named: NamedType) -> Type {
    Type::from(ListType::new(non_null(named)))
}

/// Returns `[X!]!` for a named type.
///
/// # Errors
///
/// Returns the error of [`NonNullType::new()`], which the list never
/// produces.
fn non_null_list_of_non_null(named: NamedType) -> Result<Type, DefinitionError> {
    Ok(Type::from(NonNullType::new(list_of_non_null(named))?))
}

/// Returns the type `named` stands for.
///
/// A field thunk calls it, which runs after [`built()`] has stored the
/// introspection types, so an introspection type is read from there.
///
/// # Errors
///
/// Returns what [`built()`] throws, which is never.
fn named_type(named: Named) -> Result<NamedType, DefinitionError> {
    Ok(match named {
        Named::String => NamedType::from(graphql_string()),
        Named::Boolean => NamedType::from(graphql_boolean()),
        Named::Directive => NamedType::from(introspection_directive()?),
        Named::DirectiveLocation => NamedType::from(introspection_directive_location()?),
        Named::Type => NamedType::from(introspection_type()?),
        Named::Field => NamedType::from(introspection_field()?),
        Named::InputValue => NamedType::from(introspection_input_value()?),
        Named::EnumValue => NamedType::from(introspection_enum_value()?),
        Named::TypeKind => NamedType::from(introspection_type_kind()?),
    })
}

/// Returns the type of a field of `named` wrapped as `wrapping` says.
///
/// # Errors
///
/// Returns what [`named_type()`] or [`NonNullType::new()`] throws, which is
/// never.
fn wrapped_type(named: Named, wrapping: Wrapping) -> Result<Type, DefinitionError> {
    let named = named_type(named)?;
    Ok(match wrapping {
        Wrapping::Plain => Type::from(named),
        Wrapping::NonNull => non_null(named),
        Wrapping::ListOfNonNull => list_of_non_null(named),
        Wrapping::NonNullListOfNonNull => non_null_list_of_non_null(named)?,
    })
}

/// Returns the field configs of `rows`, which is the field thunk of an
/// introspection object type.
///
/// A config borrows the resolver of its row, and a field with the
/// `includeDeprecated` argument gets it with the type `Boolean` and the
/// default value `false`.
///
/// # Errors
///
/// Returns what [`wrapped_type()`] throws, which is never.
fn field_configs(rows: &'static [FieldRow]) -> Result<ObjMap<FieldConfig>, DefinitionError> {
    let mut configs: ObjMap<FieldConfig> = ObjMap::default();
    for row in rows {
        let mut config = FieldConfig {
            description: row.description.map(String::from),
            resolve: Some(ResolverHandle::from_static(&row.resolve)),
            ..FieldConfig::of_type(wrapped_type(row.named, row.wrapping)?)
        };
        if row.include_deprecated {
            config.args.insert(
                String::from("includeDeprecated"),
                ArgumentConfig {
                    default_value: JsValue::Bool(false),
                    ..ArgumentConfig::of_type(Type::from(graphql_boolean()))
                },
            );
        }
        configs.insert(String::from(row.name), config);
    }
    Ok(configs)
}

/// Returns the value configs of `values`, which is the value thunk of an
/// introspection enum type, with the name of each value as its value.
fn value_configs(values: &'static [(&'static str, &'static str)]) -> ObjMap<EnumValueConfig> {
    let mut configs: ObjMap<EnumValueConfig> = ObjMap::default();
    for (name, description) in values {
        configs.insert(
            String::from(*name),
            EnumValueConfig {
                description: Some(String::from(*description)),
                value: JsValue::from(*name),
                ..EnumValueConfig::default()
            },
        );
    }
    configs
}

/// Builds the object type of `row`, whose field thunk is `fields`.
///
/// # Errors
///
/// Returns the error of [`ObjectType::new()`], which the literal name never
/// produces.
fn object(
    row: &ObjectRow,
    fields: impl Fn() -> Result<ObjMap<FieldConfig>, DefinitionError> + Send + Sync + 'static,
) -> Result<Arc<ObjectType>, DefinitionError> {
    let ty = ObjectType::new(ObjectTypeConfig {
        name: String::from(row.name),
        description: Some(String::from(row.description)),
        fields: ThunkObjMap::thunk(fields),
        ..ObjectTypeConfig::default()
    })?;
    Ok(Arc::new(ty))
}

/// Builds the enum type of `row`, whose value thunk is `values`.
///
/// The values of the reference are a map, which its constructor defines at
/// once. Here they are a thunk over the row, because a map would stay in the
/// type beside the values it defines, while the thunk leaves only the values
/// on the heap. The value names are literals that pass
/// [`assert_enum_value_name()`], so the two orders define the same values and
/// throw nothing.
///
/// [`assert_enum_value_name()`]: crate::assert_name::assert_enum_value_name
///
/// # Errors
///
/// Returns the error of [`EnumType::new()`], which the literal name never
/// produces.
fn enumeration(
    row: &EnumRow,
    values: impl Fn() -> Result<ObjMap<EnumValueConfig>, DefinitionError> + Send + Sync + 'static,
) -> Result<Arc<EnumType>, DefinitionError> {
    let ty = EnumType::new(EnumTypeConfig {
        name: String::from(row.name),
        description: Some(String::from(row.description)),
        values: ThunkObjMap::thunk(values),
        ..EnumTypeConfig::default()
    })?;
    Ok(Arc::new(ty))
}

/// Builds the meta field of `row` with its type and its arguments.
///
/// Note that these are `GraphQLField` and not `GraphQLFieldConfig`, so the
/// format for args is different.
fn meta_field(row: &'static MetaFieldRow, r#type: Type, args: Vec<Argument>) -> Arc<Field> {
    Arc::new(Field {
        name: String::from(row.name),
        description: Some(String::from(row.description)),
        r#type,
        args,
        resolve: Some(ResolverHandle::from_static(&row.resolve)),
        subscribe: None,
        deprecation_reason: None,
        extensions: JsObject::new(),
        ast_node: None,
    })
}

/// Returns the one argument of `__type`, `name: String!`.
fn type_meta_field_args() -> Vec<Argument> {
    alloc::vec![Argument {
        name: String::from("name"),
        description: None,
        r#type: non_null(NamedType::from(graphql_string())),
        default_value: JsValue::Undefined,
        deprecation_reason: None,
        extensions: JsObject::new(),
        ast_node: None,
    }]
}

/// The eight introspection types, each with the type it is.
struct Types {
    /// `__Schema`.
    schema: Arc<ObjectType>,
    /// `__Directive`.
    directive: Arc<ObjectType>,
    /// `__DirectiveLocation`.
    directive_location: Arc<EnumType>,
    /// `__Type`.
    r#type: Arc<ObjectType>,
    /// `__Field`.
    field: Arc<ObjectType>,
    /// `__InputValue`.
    input_value: Arc<ObjectType>,
    /// `__EnumValue`.
    enum_value: Arc<ObjectType>,
    /// `__TypeKind`.
    type_kind: Arc<EnumType>,
}

impl Types {
    /// Returns the eight types as named types, in the reference's order.
    fn list(&self) -> [NamedType; 8] {
        [
            NamedType::from(Arc::clone(&self.schema)),
            NamedType::from(Arc::clone(&self.directive)),
            NamedType::from(Arc::clone(&self.directive_location)),
            NamedType::from(Arc::clone(&self.r#type)),
            NamedType::from(Arc::clone(&self.field)),
            NamedType::from(Arc::clone(&self.input_value)),
            NamedType::from(Arc::clone(&self.enum_value)),
            NamedType::from(Arc::clone(&self.type_kind)),
        ]
    }
}

/// The introspection types and the two meta fields that name one of them.
struct Introspection {
    /// The eight types.
    types: Types,
    /// The eight types as [`introspection_types()`] lists them.
    list: [NamedType; 8],
    /// The `__schema` field.
    schema_meta_field_def: Arc<Field>,
    /// The `__type` field.
    type_meta_field_def: Arc<Field>,
}

/// The introspection types built from the rows above, and the `__schema` and
/// `__type` fields.
static BUILT: OnceBox<Introspection> = OnceBox::new();

/// Builds the eight types and the two meta fields that name them.
///
/// No thunk of a type runs here, so nothing reads [`BUILT`] before it holds
/// the value.
///
/// # Errors
///
/// Returns the error of a constructor, which the literal names never
/// produce.
fn build() -> Result<Box<Introspection>, DefinitionError> {
    let types = Types {
        schema: object(&SCHEMA, || field_configs(SCHEMA.fields))?,
        directive: object(&DIRECTIVE, || field_configs(DIRECTIVE.fields))?,
        directive_location: enumeration(&DIRECTIVE_LOCATION, || {
            Ok(value_configs(DIRECTIVE_LOCATION.values))
        })?,
        r#type: object(&TYPE, || field_configs(TYPE.fields))?,
        field: object(&FIELD, || field_configs(FIELD.fields))?,
        input_value: object(&INPUT_VALUE, || field_configs(INPUT_VALUE.fields))?,
        enum_value: object(&ENUM_VALUE, || field_configs(ENUM_VALUE.fields))?,
        type_kind: enumeration(&TYPE_KIND, || Ok(value_configs(TYPE_KIND.values)))?,
    };
    Ok(Box::new(Introspection {
        list: types.list(),
        schema_meta_field_def: meta_field(
            &SCHEMA_META_FIELD,
            non_null(NamedType::from(Arc::clone(&types.schema))),
            Vec::new(),
        ),
        type_meta_field_def: meta_field(
            &TYPE_META_FIELD,
            Type::from(Arc::clone(&types.r#type)),
            type_meta_field_args(),
        ),
        types,
    }))
}

/// Returns the introspection types and the two meta fields, building them on
/// first use.
///
/// # Errors
///
/// Returns what [`build()`] throws, which is never.
fn built() -> Result<&'static Introspection, DefinitionError> {
    BUILT.get_or_try_init(build)
}

/// Returns the `__Schema` type.
///
/// Every call returns the same allocation.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
///
/// # Errors
///
/// Returns the error of any of the eight constructors that build the
/// introspection types on first use, which the literal names never produce.
pub fn introspection_schema() -> Result<Arc<ObjectType>, DefinitionError> {
    built().map(|built| Arc::clone(&built.types.schema))
}

/// Returns the `__Directive` type.
///
/// Every call returns the same allocation.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
///
/// # Errors
///
/// Returns the error of any of the eight constructors that build the
/// introspection types on first use, which the literal names never produce.
pub fn introspection_directive() -> Result<Arc<ObjectType>, DefinitionError> {
    built().map(|built| Arc::clone(&built.types.directive))
}

/// Returns the `__DirectiveLocation` type.
///
/// Each value is the string of the [`DirectiveLocation`] it names, which is
/// its own name. Every call returns the same allocation.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
///
/// # Errors
///
/// Returns the error of any of the eight constructors that build the
/// introspection types on first use, which the literal names never produce.
pub fn introspection_directive_location() -> Result<Arc<EnumType>, DefinitionError> {
    built().map(|built| Arc::clone(&built.types.directive_location))
}

/// Returns the `__Type` type.
///
/// Every call returns the same allocation.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
///
/// # Errors
///
/// Returns the error of any of the eight constructors that build the
/// introspection types on first use, which the literal names never produce.
pub fn introspection_type() -> Result<Arc<ObjectType>, DefinitionError> {
    built().map(|built| Arc::clone(&built.types.r#type))
}

/// Returns the `__Field` type.
///
/// Every call returns the same allocation.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
///
/// # Errors
///
/// Returns the error of any of the eight constructors that build the
/// introspection types on first use, which the literal names never produce.
pub fn introspection_field() -> Result<Arc<ObjectType>, DefinitionError> {
    built().map(|built| Arc::clone(&built.types.field))
}

/// Returns the `__InputValue` type.
///
/// Every call returns the same allocation.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
///
/// # Errors
///
/// Returns the error of any of the eight constructors that build the
/// introspection types on first use, which the literal names never produce.
pub fn introspection_input_value() -> Result<Arc<ObjectType>, DefinitionError> {
    built().map(|built| Arc::clone(&built.types.input_value))
}

/// Returns the `__EnumValue` type.
///
/// Every call returns the same allocation.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
///
/// # Errors
///
/// Returns the error of any of the eight constructors that build the
/// introspection types on first use, which the literal names never produce.
pub fn introspection_enum_value() -> Result<Arc<ObjectType>, DefinitionError> {
    built().map(|built| Arc::clone(&built.types.enum_value))
}

/// Returns the `__TypeKind` type.
///
/// Each value is the string of the [`TypeKind`] it names, which is its own
/// name. Every call returns the same allocation.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
///
/// # Errors
///
/// Returns the error of any of the eight constructors that build the
/// introspection types on first use, which the literal names never produce.
pub fn introspection_type_kind() -> Result<Arc<EnumType>, DefinitionError> {
    built().map(|built| Arc::clone(&built.types.type_kind))
}

/// Returns the `__schema` meta field.
///
/// Note that these are `GraphQLField` and not `GraphQLFieldConfig`, so the
/// format for args is different. Every call returns the same field, borrowed
/// from the static that holds it for the life of the program, so a read
/// counts no reference.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
///
/// # Errors
///
/// Returns the error of any of the eight constructors that build the
/// introspection types on first use, which the literal names never produce.
pub fn schema_meta_field_def() -> Result<&'static Field, DefinitionError> {
    built().map(|built| &*built.schema_meta_field_def)
}

/// Returns the `__type` meta field, with its `name` argument.
///
/// Every call returns the same field, borrowed from the static that holds it
/// for the life of the program.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
///
/// # Errors
///
/// Returns the error of any of the eight constructors that build the
/// introspection types on first use, which the literal names never produce.
pub fn type_meta_field_def() -> Result<&'static Field, DefinitionError> {
    built().map(|built| &*built.type_meta_field_def)
}

/// The one `__typename` field.
///
/// It names no introspection type, so it has a static of its own and its
/// getter cannot fail.
static TYPE_NAME_META_FIELD_DEF: OnceBox<Arc<Field>> = OnceBox::new();

/// Returns the `__typename` meta field.
///
/// Every call returns the same field, borrowed from the static that holds it
/// for the life of the program.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
#[must_use]
pub fn type_name_meta_field_def() -> &'static Field {
    TYPE_NAME_META_FIELD_DEF.get_or_init(|| {
        Box::new(meta_field(
            &TYPE_NAME_META_FIELD,
            non_null(NamedType::from(graphql_string())),
            Vec::new(),
        ))
    })
}

/// Returns the eight introspection types, in the reference's order.
///
/// Every call returns the same list.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
///
/// # Errors
///
/// Returns the error of any of the eight constructors that build the
/// introspection types on first use, which the literal names never produce.
pub fn introspection_types() -> Result<&'static [NamedType], DefinitionError> {
    built().map(|built| built.list.as_slice())
}

/// Returns whether `ty` carries the name of an introspection type.
///
/// The reference compares the name against the name of each type of the list,
/// and the eight names are the names of the rows, so the predicate reads them
/// and builds no type.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
#[must_use]
pub fn is_introspection_type(ty: &NamedType) -> bool {
    [
        SCHEMA.name,
        DIRECTIVE.name,
        DIRECTIVE_LOCATION.name,
        TYPE.name,
        FIELD.name,
        INPUT_VALUE.name,
        ENUM_VALUE.name,
        TYPE_KIND.name,
    ]
    .contains(&ty.name())
}

/* The resolvers of __Schema */

/// Resolves `__Schema.description`.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
#[must_use]
pub fn schema_description(schema: &Schema) -> Option<&str> {
    schema.description.as_deref()
}

/// Resolves `__Schema.types`, the values of the type map in map order.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
#[must_use]
pub fn schema_types(schema: &Schema) -> Vec<&NamedType> {
    schema.get_type_map().values().collect()
}

/// Resolves `__Schema.queryType`.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
#[must_use]
pub fn schema_query_type(schema: &Schema) -> Option<&NamedType> {
    schema.get_query_type()
}

/// Resolves `__Schema.mutationType`.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
#[must_use]
pub fn schema_mutation_type(schema: &Schema) -> Option<&NamedType> {
    schema.get_mutation_type()
}

/// Resolves `__Schema.subscriptionType`.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
#[must_use]
pub fn schema_subscription_type(schema: &Schema) -> Option<&NamedType> {
    schema.get_subscription_type()
}

/// Resolves `__Schema.directives`.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
#[must_use]
pub fn schema_directives(schema: &Schema) -> &[Arc<Directive>] {
    schema.get_directives()
}

/* The resolvers of __Directive */

/// Resolves `__Directive.name`.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
#[must_use]
pub fn directive_name(directive: &Directive) -> &str {
    &directive.name
}

/// Resolves `__Directive.description`.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
#[must_use]
pub fn directive_description(directive: &Directive) -> Option<&str> {
    directive.description.as_deref()
}

/// Resolves `__Directive.isRepeatable`.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
#[must_use]
pub fn directive_is_repeatable(directive: &Directive) -> bool {
    directive.is_repeatable
}

/// Resolves `__Directive.locations`.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
#[must_use]
pub fn directive_locations(directive: &Directive) -> &[DirectiveLocation] {
    &directive.locations
}

/// Resolves `__Directive.args`, every argument or the ones that are not
/// deprecated.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
#[must_use]
pub fn directive_args(directive: &Directive, include_deprecated: bool) -> Vec<&Argument> {
    args(&directive.args, include_deprecated)
}

/// Returns every argument of `args`, or the ones whose deprecation reason
/// is `null`.
///
/// The `args` resolvers of `__Directive` and `__Field` are one expression
/// in the reference, and `deprecationReason == null` is the absent reason
/// here.
fn args(args: &[Argument], include_deprecated: bool) -> Vec<&Argument> {
    if include_deprecated {
        args.iter().collect()
    } else {
        args.iter()
            .filter(|arg| arg.deprecation_reason.is_none())
            .collect()
    }
}

/* The resolvers of __Type */

/// Resolves `__Type.kind`.
///
/// The invariant of the reference for a type of no kind has no reading,
/// because the type is a closed enum.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
#[must_use]
pub fn type_kind(ty: &Type) -> TypeKind {
    match ty {
        Type::Scalar(_) => TypeKind::Scalar,
        Type::Object(_) => TypeKind::Object,
        Type::Interface(_) => TypeKind::Interface,
        Type::Union(_) => TypeKind::Union,
        Type::Enum(_) => TypeKind::Enum,
        Type::InputObject(_) => TypeKind::InputObject,
        Type::List(_) => TypeKind::List,
        Type::NonNull(_) => TypeKind::NonNull,
    }
}

/// Resolves `__Type.name`, which a wrapping type has not.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
#[must_use]
pub fn type_name(ty: &Type) -> Option<&str> {
    match ty {
        Type::Scalar(scalar) => Some(&scalar.name),
        Type::Object(object) => Some(&object.name),
        Type::Interface(interface) => Some(&interface.name),
        Type::Union(union) => Some(&union.name),
        Type::Enum(enum_type) => Some(&enum_type.name),
        Type::InputObject(input_object) => Some(&input_object.name),
        Type::List(_) | Type::NonNull(_) => None,
    }
}

/// Resolves `__Type.description`, which a wrapping type has not.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
#[must_use]
pub fn type_description(ty: &Type) -> Option<&str> {
    match ty {
        Type::Scalar(scalar) => scalar.description.as_deref(),
        Type::Object(object) => object.description.as_deref(),
        Type::Interface(interface) => interface.description.as_deref(),
        Type::Union(union) => union.description.as_deref(),
        Type::Enum(enum_type) => enum_type.description.as_deref(),
        Type::InputObject(input_object) => input_object.description.as_deref(),
        Type::List(_) | Type::NonNull(_) => None,
    }
}

/// Resolves `__Type.specifiedByURL`, which only a scalar type has.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
#[must_use]
pub fn type_specified_by_url(ty: &Type) -> Option<&str> {
    match ty {
        Type::Scalar(scalar) => scalar.specified_by_url.as_deref(),
        _ => None,
    }
}

/// Resolves `__Type.fields`, the fields of an object or interface type in
/// map order, every one or the ones that are not deprecated, and `None`
/// for every other type.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
///
/// # Errors
///
/// Returns what the field thunk throws.
pub fn type_fields(
    ty: &Type,
    include_deprecated: bool,
) -> Result<Option<Vec<&Field>>, DefinitionError> {
    let fields = match ty {
        Type::Object(object) => object.get_fields()?,
        Type::Interface(interface) => interface.get_fields()?,
        _ => return Ok(None),
    };
    Ok(Some(if include_deprecated {
        fields.values().collect()
    } else {
        fields
            .values()
            .filter(|field| field.deprecation_reason.is_none())
            .collect()
    }))
}

/// Resolves `__Type.interfaces`, the interfaces of an object or interface
/// type, and `None` for every other type.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
///
/// # Errors
///
/// Returns what the interface thunk throws.
pub fn type_interfaces(ty: &Type) -> Result<Option<&[NamedType]>, DefinitionError> {
    match ty {
        Type::Object(object) => object.get_interfaces().map(Some),
        Type::Interface(interface) => interface.get_interfaces().map(Some),
        _ => Ok(None),
    }
}

/// Resolves `__Type.possibleTypes`, the possible types of an abstract type
/// in `schema`, and `None` for every other type.
///
/// The list is copied, because the borrow of the schema's answer ends with
/// the named type this builds from `ty`.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
///
/// # Errors
///
/// Returns what the member thunk of a union throws.
pub fn type_possible_types(
    ty: &Type,
    schema: &Schema,
) -> Result<Option<Vec<NamedType>>, DefinitionError> {
    let abstract_type = match ty {
        Type::Interface(_) | Type::Union(_) => ty.as_named(),
        _ => None,
    };
    match abstract_type {
        Some(abstract_type) => Ok(Some(schema.get_possible_types(&abstract_type)?.to_vec())),
        None => Ok(None),
    }
}

/// Resolves `__Type.enumValues`, the values of an enum type in map order,
/// every one or the ones that are not deprecated, and `None` for every
/// other type.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
///
/// # Errors
///
/// Returns what the value thunk throws.
pub fn type_enum_values(
    ty: &Type,
    include_deprecated: bool,
) -> Result<Option<Vec<&EnumValue>>, DefinitionError> {
    let Type::Enum(enum_type) = ty else {
        return Ok(None);
    };
    let values = enum_type.get_values()?;
    Ok(Some(if include_deprecated {
        values.iter().collect()
    } else {
        values
            .iter()
            .filter(|field| field.deprecation_reason.is_none())
            .collect()
    }))
}

/// Resolves `__Type.inputFields`, the fields of an input object type in map
/// order, every one or the ones that are not deprecated, and `None` for
/// every other type.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
///
/// # Errors
///
/// Returns what the field thunk throws.
pub fn type_input_fields(
    ty: &Type,
    include_deprecated: bool,
) -> Result<Option<Vec<&InputField>>, DefinitionError> {
    let Type::InputObject(input_object) = ty else {
        return Ok(None);
    };
    let values = input_object.get_fields()?;
    Ok(Some(if include_deprecated {
        values.values().collect()
    } else {
        values
            .values()
            .filter(|field| field.deprecation_reason.is_none())
            .collect()
    }))
}

/// Resolves `__Type.ofType`, the wrapped type of a wrapping type.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
#[must_use]
pub fn type_of_type(ty: &Type) -> Option<&Type> {
    ty.of_type()
}

/// Resolves `__Type.isOneOf`, which only an input object type has.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
#[must_use]
pub fn type_is_one_of(ty: &Type) -> Option<bool> {
    match ty {
        Type::InputObject(input_object) => Some(input_object.is_one_of),
        _ => None,
    }
}

/* The resolvers of __Field */

/// Resolves `__Field.name`.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
#[must_use]
pub fn field_name(field: &Field) -> &str {
    &field.name
}

/// Resolves `__Field.description`.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
#[must_use]
pub fn field_description(field: &Field) -> Option<&str> {
    field.description.as_deref()
}

/// Resolves `__Field.args`, every argument or the ones that are not
/// deprecated.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
#[must_use]
pub fn field_args(field: &Field, include_deprecated: bool) -> Vec<&Argument> {
    args(&field.args, include_deprecated)
}

/// Resolves `__Field.type`.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
#[must_use]
pub fn field_type(field: &Field) -> &Type {
    &field.r#type
}

/// Resolves `__Field.isDeprecated`.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
#[must_use]
pub fn field_is_deprecated(field: &Field) -> bool {
    field.deprecation_reason.is_some()
}

/// Resolves `__Field.deprecationReason`.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
#[must_use]
pub fn field_deprecation_reason(field: &Field) -> Option<&str> {
    field.deprecation_reason.as_deref()
}

/* The resolvers of __InputValue */

/// An argument or an input field, the two definitions `__InputValue`
/// describes.
///
/// The reference types the resolvers of `__InputValue` over
/// `GraphQLInputField`, and an argument reaches them through the `args`
/// fields of `__Field` and `__Directive` with the same shape, so both are one
/// source here.
#[derive(Debug, Clone, Copy)]
pub enum InputValue<'a> {
    /// An argument of a field or a directive.
    Argument(&'a Argument),
    /// A field of an input object type.
    InputField(&'a InputField),
}

impl<'a> From<&'a Argument> for InputValue<'a> {
    fn from(argument: &'a Argument) -> Self {
        Self::Argument(argument)
    }
}

impl<'a> From<&'a InputField> for InputValue<'a> {
    fn from(input_field: &'a InputField) -> Self {
        Self::InputField(input_field)
    }
}

/// Resolves `__InputValue.name`.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
#[must_use]
pub fn input_value_name(input_value: InputValue<'_>) -> &str {
    match input_value {
        InputValue::Argument(argument) => &argument.name,
        InputValue::InputField(input_field) => &input_field.name,
    }
}

/// Resolves `__InputValue.description`.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
#[must_use]
pub fn input_value_description(input_value: InputValue<'_>) -> Option<&str> {
    match input_value {
        InputValue::Argument(argument) => argument.description.as_deref(),
        InputValue::InputField(input_field) => input_field.description.as_deref(),
    }
}

/// Resolves `__InputValue.type`.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
#[must_use]
pub fn input_value_type(input_value: InputValue<'_>) -> &Type {
    match input_value {
        InputValue::Argument(argument) => &argument.r#type,
        InputValue::InputField(input_field) => &input_field.r#type,
    }
}

/// Resolves `__InputValue.defaultValue`, the default value printed as a
/// GraphQL literal, and `None` when there is none.
///
/// The value goes through `astFromValue` and the printer, so the string is
/// the one the reference sends.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
///
/// # Errors
///
/// Returns what [`ast_from_value()`] throws.
pub fn input_value_default_value(
    input_value: InputValue<'_>,
) -> Result<Option<String>, DefinitionError> {
    let (r#type, default_value) = match input_value {
        InputValue::Argument(argument) => (&argument.r#type, &argument.default_value),
        InputValue::InputField(input_field) => (&input_field.r#type, &input_field.default_value),
    };
    let value_ast = ast_from_value(default_value, r#type)?;
    Ok(value_ast.map(|value_ast| print(&value_ast)))
}

/// Resolves `__InputValue.isDeprecated`.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
#[must_use]
pub fn input_value_is_deprecated(input_value: InputValue<'_>) -> bool {
    input_value_deprecation_reason(input_value).is_some()
}

/// Resolves `__InputValue.deprecationReason`.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
#[must_use]
pub fn input_value_deprecation_reason(input_value: InputValue<'_>) -> Option<&str> {
    match input_value {
        InputValue::Argument(argument) => argument.deprecation_reason.as_deref(),
        InputValue::InputField(input_field) => input_field.deprecation_reason.as_deref(),
    }
}

/* The resolvers of __EnumValue */

/// Resolves `__EnumValue.name`.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
#[must_use]
pub fn enum_value_name(enum_value: &EnumValue) -> &str {
    &enum_value.name
}

/// Resolves `__EnumValue.description`.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
#[must_use]
pub fn enum_value_description(enum_value: &EnumValue) -> Option<&str> {
    enum_value.description.as_deref()
}

/// Resolves `__EnumValue.isDeprecated`.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
#[must_use]
pub fn enum_value_is_deprecated(enum_value: &EnumValue) -> bool {
    enum_value.deprecation_reason.is_some()
}

/// Resolves `__EnumValue.deprecationReason`.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
#[must_use]
pub fn enum_value_deprecation_reason(enum_value: &EnumValue) -> Option<&str> {
    enum_value.deprecation_reason.as_deref()
}

/* The resolvers of the meta fields */

/// Resolves `__schema`, the schema of the execution.
///
/// Reads the schema off the resolve info.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
#[must_use]
pub fn schema_meta_field(schema: &Schema) -> &Schema {
    schema
}

/// Resolves `__type`, the type of `schema` named by the `name` argument.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
#[must_use]
pub fn type_meta_field<'a>(schema: &'a Schema, name: &str) -> Option<&'a NamedType> {
    schema.get_type(name)
}

/// Resolves `__typename`, the name of the parent object type.
///
/// Reads the parent type off the resolve info.
///
/// Derived from graphql-js 16.12.0, src/type/introspection.ts, MIT.
#[must_use]
pub fn type_name_meta_field(parent_type: &ObjectType) -> &str {
    &parent_type.name
}
