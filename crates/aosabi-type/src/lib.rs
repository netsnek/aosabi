// SPDX-License-Identifier: MIT
/*
 * aosabi-type: the GraphQL type layer of Aosabi.
 *
 * This file holds the module list and the names the crate exports at its root.
 *
 * Derived from graphql-js 16.12.0, src/type, MIT.
 */

//! The GraphQL type layer of Aosabi.
//!
//! The crate is a literal translation of the type directory of graphql-js
//! 16.12.0, the version the wire is pinned to. Its module [`wrapping`]
//! translates `createWrappingSchema` of `@graphql-tools/wrap` 11.1.9 and
//! `astFromType` of `@graphql-tools/utils` 11.0.0, the wrapping schema of a
//! remote. Every function keeps the name of the function it translates in snake
//! case and follows its source line by line, because the exit numbers of the
//! milestone are byte equality of the printed schema and of the invalid schema
//! message.
//!
//! | Module | Translates | Holds |
//! | --- | --- | --- |
//! | [`assert_name`](mod@assert_name) | `assertName.ts` | [`assert_name()`] and [`assert_enum_value_name()`] |
//! | [`definition`] | `definition.ts` | every type, [`Type`], [`NamedType`], the predicates and [`DefinitionError`] |
//! | [`schema`] | `schema.ts` | [`Schema`] and [`SchemaConfig`] |
//! | [`directives`] | `directives.ts`, `directiveLocation.ts` | [`Directive`], [`DirectiveLocation`] and the specified directives |
//! | [`scalars`] | `scalars.ts` | the five specified scalars and `Number(string)` |
//! | [`introspection`] | `introspection.ts` | the eight introspection types, [`IntrospectionResolver`], [`TypeKind`] and the three meta fields |
//! | [`validate`] | `validate.ts`, `typeComparators.ts` | [`validate_schema()`], [`assert_valid_schema()`], the two type comparators and [`do_types_overlap()`] |
//! | [`natural_compare`](mod@natural_compare) | `jsutils/naturalCompare.ts` | [`natural_compare()`] |
//! | [`suggestion_list`](mod@suggestion_list) | `jsutils/suggestionList.ts` | [`suggestion_list()`] and the lexical distance |
//! | [`did_you_mean`](mod@did_you_mean) | `jsutils/didYouMean.ts` | [`did_you_mean()`] |
//! | [`inspect`](mod@inspect) | `jsutils/inspect.ts` | [`inspect()`], the value printer of a message |
//! | [`print_path_array`](mod@print_path_array) | `jsutils/printPathArray.ts` | [`print_path_array()`] |
//! | [`print_schema`](mod@print_schema) | `utilities/printSchema.ts` | [`print_schema()`], [`print_introspection_schema()`] and [`print_type()`] |
//! | [`type_info`] | `utilities/TypeInfo.ts` | [`TypeInfo`], [`get_field_def()`] and [`visit_with_type_info()`] |
//! | [`type_info_edit`](mod@type_info_edit) | `utilities/TypeInfo.ts`, `visitWithTypeInfo` over the edit path | [`visit_with_type_info_edit()`](type_info_edit::visit_with_type_info_edit) |
//! | [`build_ast_schema`](mod@build_ast_schema) | `utilities/buildASTSchema.ts` | [`build_ast_schema()`], [`build_schema()`], their bound forms and [`BuildSchemaOptions`] |
//! | [`schema_bindings`](mod@schema_bindings) | `@graphql-tools/schema` `addResolversToSchema.js`, the two places that change a built type | [`SchemaBindings`], [`ScalarBinding`] and [`FieldBinding`] |
//! | [`extend_schema`](mod@extend_schema) | `utilities/extendSchema.ts`, `execution/values.ts` | [`extend_schema()`], [`extend_schema_impl()`], [`get_directive_values()`] and [`get_argument_values()`] |
//! | [`type_from_ast`](mod@type_from_ast) | `utilities/typeFromAST.ts` | [`type_from_ast()`] |
//! | [`value_from_ast`](mod@value_from_ast) | `utilities/valueFromAST.ts` | [`value_from_ast()`] |
//! | [`value_from_ast_untyped`](mod@value_from_ast_untyped) | `utilities/valueFromASTUntyped.ts` | [`value_from_ast_untyped()`] |
//! | [`ast_from_value`](mod@ast_from_value) | `utilities/astFromValue.ts` | [`ast_from_value()`] |
//! | [`coerce_input_value`](mod@coerce_input_value) | `utilities/coerceInputValue.ts`, `jsutils/Path.ts` | [`coerce_input_value()`], [`Path`], [`PathKey`], [`add_path()`] and [`path_to_array()`] |
//! | [`get_introspection_query`](mod@get_introspection_query) | `utilities/getIntrospectionQuery.ts` | [`get_introspection_query()`] and [`IntrospectionOptions`] |
//! | [`build_client_schema`](mod@build_client_schema) | `utilities/buildClientSchema.ts`, `jsutils/keyValMap.ts`, `jsutils/isObjectLike.ts` | [`build_client_schema()`] and [`BuildClientSchemaOptions`] |
//! | [`wrapping`] | `@graphql-tools/wrap` `createWrappingSchema` and `astFromType` of `@graphql-tools/utils` | [`create_wrapping_schema()`](wrapping::create_wrapping_schema) and [`ast_from_type()`](wrapping::ast_from_type) |
//!
//! # The shape of the port
//!
//! A named type is shared through an `Arc` and compared by identity, a wrapping
//! type shares the type it wraps and is compared by structure, and every thunk
//! resolves once behind a race cell. The specified scalars, the specified
//! directives and the introspection types are rows of read only data, and one
//! static per module holds the types built from them on first use, so that the
//! one `Boolean` a directive argument refers to is the one a schema lists. A
//! value a scalar or an enum coerces is a `JsValue` of the JSON writer. The
//! `resolve`, `subscribe`, `isTypeOf` and `resolveType` slots hold a
//! [`ResolverHandle`], because their signature is the executor's decision.
//!
//! The crate builds with no standard library and needs an allocator, because
//! the bare metal build of a microcontroller card links it. The `std` feature
//! is the configuration symbol of a host build and gates nothing today. There
//! is no test in this crate, which is the rule of CONTRIBUTING.md section 3,
//! and the judging notebook is `tests/notebooks/02_language.ipynb`.

#![no_std]

extern crate alloc;

pub mod assert_name;
pub mod ast_from_value;
pub mod build_ast_schema;
pub mod build_client_schema;
pub mod coerce_input_value;
pub mod definition;
pub mod did_you_mean;
pub mod directives;
pub mod extend_schema;
pub mod get_introspection_query;
pub mod inspect;
pub mod introspection;
pub mod natural_compare;
pub mod print_path_array;
pub mod print_schema;
pub mod scalars;
pub mod schema;
pub mod schema_bindings;
pub mod suggestion_list;
pub mod type_from_ast;
pub mod type_info;
pub mod type_info_edit;
pub mod validate;
pub mod value_from_ast;
pub mod value_from_ast_untyped;
pub mod wrapping;

pub use crate::{
    assert_name::{
        assert_enum_value_name,
        assert_name, //
    },
    ast_from_value::ast_from_value,
    build_ast_schema::{
        AssertValidSdl,
        BuildSchemaOptions,
        build_ast_schema,
        build_ast_schema_with_bindings,
        build_schema,
        build_schema_with_bindings, //
    },
    build_client_schema::{
        BuildClientSchemaOptions,
        build_client_schema, //
    },
    coerce_input_value::{
        Path,
        PathKey,
        add_path,
        coerce_input_value,
        default_on_error,
        path_to_array, //
    },
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
        FieldResolver,
        InputField,
        InputFieldConfig,
        InputObjectType,
        InputObjectTypeConfig,
        InterfaceType,
        InterfaceTypeConfig,
        IsTypeOfFn,
        ListType,
        NamedType,
        NonNullType,
        ObjMap,
        ObjectType,
        ObjectTypeConfig,
        Parsed,
        ResolverHandle,
        ScalarLiteralParser,
        ScalarSerializer,
        ScalarType,
        ScalarTypeConfig,
        ScalarValueParser,
        SharedType,
        ThunkFn,
        ThunkObjMap,
        ThunkReadonlyArray,
        Type,
        TypeResolver,
        UnionType,
        UnionTypeConfig,
        get_named_type,
        get_nullable_type,
        is_abstract_type,
        is_composite_type,
        is_enum_type,
        is_input_object_type,
        is_input_type,
        is_interface_type,
        is_leaf_type,
        is_list_type,
        is_named_type,
        is_non_null_type,
        is_nullable_type,
        is_object_type,
        is_output_type,
        is_required_argument,
        is_required_input_field,
        is_scalar_type,
        is_union_type,
        is_wrapping_type, //
    },
    did_you_mean::did_you_mean,
    directives::{
        DEFAULT_DEPRECATION_REASON,
        Directive,
        DirectiveConfig,
        DirectiveLocation,
        is_specified_directive,
        specified_directives, //
    },
    extend_schema::{
        AssertValidSdlExtension,
        ExtendSchemaOptions,
        extend_schema,
        extend_schema_impl,
        get_argument_values,
        get_directive_values, //
    },
    get_introspection_query::{
        IntrospectionOptions,
        get_introspection_query, //
    },
    inspect::inspect,
    introspection::{
        InputValue,
        IntrospectionResolver,
        TypeKind,
        introspection_directive,
        introspection_directive_location,
        introspection_enum_value,
        introspection_field,
        introspection_input_value,
        introspection_schema,
        introspection_type,
        introspection_type_kind,
        introspection_types,
        is_introspection_type,
        schema_meta_field_def,
        type_meta_field_def,
        type_name_meta_field_def, //
    },
    natural_compare::natural_compare,
    print_path_array::print_path_array,
    print_schema::{
        print_introspection_schema,
        print_schema,
        print_type, //
    },
    scalars::{
        GRAPHQL_MAX_INT,
        GRAPHQL_MIN_INT,
        graphql_boolean,
        graphql_float,
        graphql_id,
        graphql_int,
        graphql_string,
        is_specified_scalar_name,
        is_specified_scalar_type,
        specified_scalar_types, //
    },
    schema::{
        InterfaceImplementations,
        Schema,
        SchemaConfig, //
    },
    schema_bindings::{
        FieldBinding,
        ScalarBinding,
        SchemaBindings, //
    },
    suggestion_list::suggestion_list,
    type_from_ast::type_from_ast,
    type_info::{
        FieldDef,
        GetFieldDefFn,
        TypeInfo,
        VisitWithTypeInfo,
        get_field_def,
        visit_with_type_info, //
    },
    validate::{
        assert_valid_schema,
        do_types_overlap,
        get_operation_type_node,
        is_equal_type,
        is_type_sub_type_of,
        validate_schema, //
    },
    value_from_ast::value_from_ast,
    value_from_ast_untyped::value_from_ast_untyped, //
};
