// SPDX-License-Identifier: MIT
/*
 * aosabi-type: the schema printer, which renders a schema as the SDL
 * graphql-js prints.
 *
 * Derived from graphql-js 16.12.0, src/utilities/printSchema.ts, MIT.
 */

//! The schema printer of the type layer.
//!
//! [`print_schema()`] renders every type and directive a schema defines as
//! SDL, and [`print_introspection_schema()`] renders the specified
//! directives and the introspection types instead. The build path
//! never calls `printSchema`, because its SDL comes from a hand
//! written printer over its own intermediate representation, and this port
//! is the canonical second view that explains a byte difference
//! semantically. The exit number of the milestone is byte equality of this
//! output on fifty schemas, so each function follows its line of the
//! reference and every literal is copied.
//!
//! A default value prints through [`ast_from_value()`], which turns the
//! internal value of an argument or an input field into a value node by its
//! type, and the printer of the language layer renders that node. The
//! introspection types are told apart by [`is_introspection_type()`], which
//! compares names as the reference does.
//!
//! The reference lists the types of a schema and the fields of a type with
//! `Object.values`, whose order is the array index keys ascending and then
//! insertion order. A name starts with a letter or an underscore and is
//! never an array index, so the order of every such list is the insertion
//! order of its map.

use alloc::{
    format,
    string::String,
    sync::Arc,
    vec::Vec, //
};

use aosabi_json::JsValue;
use aosabi_lang::{
    ast::StringValueNode,
    block_string::is_printable_as_block_string,
    print, //
};

use crate::{
    ast_from_value::ast_from_value,
    definition::{
        Argument,
        DefinitionError,
        EnumType,
        Field,
        InputField,
        InputObjectType,
        InterfaceType,
        NamedType,
        ObjMap,
        ObjectType,
        ScalarType,
        Type,
        UnionType, //
    },
    directives::{
        DEFAULT_DEPRECATION_REASON,
        Directive,
        is_specified_directive, //
    },
    introspection::is_introspection_type,
    scalars::is_specified_scalar_type,
    schema::Schema, //
};

/// Prints the types and the directives a schema defines as SDL.
///
/// The specified directives, the specified scalars and the introspection
/// types of `schema` are left out, and the definitions are joined by a
/// blank line.
///
/// Derived from graphql-js 16.12.0, src/utilities/printSchema.ts, MIT.
///
/// # Errors
///
/// Returns the error of a field, interface, member or value thunk of a
/// type, of the serializer of a default value, or of a default value that
/// has no AST.
pub fn print_schema(schema: &Schema) -> Result<String, DefinitionError> {
    print_filtered_schema(schema, |n| !is_specified_directive(n), is_defined_type)
}

/// Prints the specified directives and the introspection types as SDL.
///
/// Derived from graphql-js 16.12.0, src/utilities/printSchema.ts, MIT.
///
/// # Errors
///
/// Returns the error of a field, interface, member or value thunk of a
/// type, of the serializer of a default value, or of a default value that
/// has no AST.
pub fn print_introspection_schema(schema: &Schema) -> Result<String, DefinitionError> {
    print_filtered_schema(schema, is_specified_directive, is_introspection_type)
}

/// Returns whether a named type is one the schema defines, neither a
/// specified scalar nor an introspection type.
///
/// Derived from graphql-js 16.12.0, src/utilities/printSchema.ts, MIT.
fn is_defined_type(ty: &NamedType) -> bool {
    !is_specified_scalar_type(ty) && !is_introspection_type(ty)
}

/// Prints the directives and the types of `schema` that pass the two
/// filters, after the schema definition.
///
/// The `filter(Boolean)` of the reference drops the absent schema
/// definition and any empty string.
///
/// Derived from graphql-js 16.12.0, src/utilities/printSchema.ts, MIT.
fn print_filtered_schema(
    schema: &Schema,
    directive_filter: impl Fn(&Directive) -> bool,
    type_filter: impl Fn(&NamedType) -> bool,
) -> Result<String, DefinitionError> {
    let directives: Vec<&Arc<Directive>> = schema
        .get_directives()
        .iter()
        .filter(|directive| directive_filter(directive))
        .collect();
    let types: Vec<&NamedType> = schema
        .get_type_map()
        .values()
        .filter(|ty| type_filter(ty))
        .collect();

    let mut parts: Vec<Option<String>> = Vec::new();
    parts.push(print_schema_definition(schema));
    for directive in directives {
        parts.push(Some(print_directive(directive)?));
    }
    for ty in types {
        parts.push(Some(print_type(ty)?));
    }
    Ok(parts
        .into_iter()
        .flatten()
        .filter(|part| !part.is_empty())
        .collect::<Vec<String>>()
        .join("\n\n"))
}

/// Prints the `schema { ... }` definition, or nothing when the schema
/// has no description and its root types carry the common names.
///
/// Derived from graphql-js 16.12.0, src/utilities/printSchema.ts, MIT.
fn print_schema_definition(schema: &Schema) -> Option<String> {
    if schema.description.is_none() && is_schema_of_common_names(schema) {
        return None;
    }

    let mut operation_types: Vec<String> = Vec::new();

    let query_type = schema.get_query_type();
    if let Some(query_type) = query_type {
        operation_types.push(format!("  query: {}", query_type.name()));
    }

    let mutation_type = schema.get_mutation_type();
    if let Some(mutation_type) = mutation_type {
        operation_types.push(format!("  mutation: {}", mutation_type.name()));
    }

    let subscription_type = schema.get_subscription_type();
    if let Some(subscription_type) = subscription_type {
        operation_types.push(format!("  subscription: {}", subscription_type.name()));
    }

    Some(format!(
        "{}schema {{\n{}\n}}",
        print_description(schema.description.as_deref(), "", true),
        operation_types.join("\n")
    ))
}

/// Returns whether every root type of the schema carries its common name.
///
/// GraphQL schema define root types for each type of operation. These types
/// are the same as any other type and can be named in any manner, however
/// there is a common naming convention:
///
/// ```graphql
///   schema {
///     query: Query
///     mutation: Mutation
///     subscription: Subscription
///   }
/// ```
///
/// When using this naming convention, the schema description can be
/// omitted.
///
/// Derived from graphql-js 16.12.0, src/utilities/printSchema.ts, MIT.
fn is_schema_of_common_names(schema: &Schema) -> bool {
    let query_type = schema.get_query_type();
    if query_type.is_some_and(|query_type| query_type.name() != "Query") {
        return false;
    }

    let mutation_type = schema.get_mutation_type();
    if mutation_type.is_some_and(|mutation_type| mutation_type.name() != "Mutation") {
        return false;
    }

    let subscription_type = schema.get_subscription_type();
    if subscription_type.is_some_and(|subscription_type| subscription_type.name() != "Subscription")
    {
        return false;
    }

    true
}

/// Prints one named type as its SDL definition.
///
/// The `invariant` the reference ends in has no reading, because the match
/// is exhaustive.
///
/// Derived from graphql-js 16.12.0, src/utilities/printSchema.ts, MIT.
///
/// # Errors
///
/// Returns the error of a field, interface, member or value thunk of `ty`,
/// of the serializer of a default value, or of a default value that has no
/// AST.
pub fn print_type(ty: &NamedType) -> Result<String, DefinitionError> {
    match ty {
        NamedType::Scalar(scalar) => Ok(print_scalar(scalar)),
        NamedType::Object(object) => print_object(object),
        NamedType::Interface(interface) => print_interface(interface),
        NamedType::Union(union) => print_union(union),
        NamedType::Enum(enum_type) => print_enum(enum_type),
        NamedType::InputObject(input_object) => print_input_object(input_object),
    }
}

/// Prints a scalar type.
///
/// Derived from graphql-js 16.12.0, src/utilities/printSchema.ts, MIT.
fn print_scalar(ty: &ScalarType) -> String {
    format!(
        "{}scalar {}{}",
        print_description(ty.description.as_deref(), "", true),
        ty.name,
        print_specified_by_url(ty)
    )
}

/// A type that implements interfaces and declares fields, the
/// `GraphQLObjectType | GraphQLInterfaceType` union of the reference.
trait ObjectOrInterface {
    /// Returns the interfaces the type implements.
    fn get_interfaces(&self) -> Result<&[NamedType], DefinitionError>;

    /// Returns the fields of the type.
    fn get_fields(&self) -> Result<&ObjMap<Field>, DefinitionError>;
}

impl ObjectOrInterface for ObjectType {
    fn get_interfaces(&self) -> Result<&[NamedType], DefinitionError> {
        Self::get_interfaces(self)
    }

    fn get_fields(&self) -> Result<&ObjMap<Field>, DefinitionError> {
        Self::get_fields(self)
    }
}

impl ObjectOrInterface for InterfaceType {
    fn get_interfaces(&self) -> Result<&[NamedType], DefinitionError> {
        Self::get_interfaces(self)
    }

    fn get_fields(&self) -> Result<&ObjMap<Field>, DefinitionError> {
        Self::get_fields(self)
    }
}

/// Prints the `implements` clause of an object or an interface type.
///
/// Derived from graphql-js 16.12.0, src/utilities/printSchema.ts, MIT.
fn print_implemented_interfaces(ty: &impl ObjectOrInterface) -> Result<String, DefinitionError> {
    let interfaces = ty.get_interfaces()?;
    Ok(if interfaces.is_empty() {
        String::new()
    } else {
        format!(
            " implements {}",
            interfaces
                .iter()
                .map(NamedType::name)
                .collect::<Vec<&str>>()
                .join(" & ")
        )
    })
}

/// Prints an object type.
///
/// Derived from graphql-js 16.12.0, src/utilities/printSchema.ts, MIT.
fn print_object(ty: &ObjectType) -> Result<String, DefinitionError> {
    Ok(format!(
        "{}type {}{}{}",
        print_description(ty.description.as_deref(), "", true),
        ty.name,
        print_implemented_interfaces(ty)?,
        print_fields(ty)?
    ))
}

/// Prints an interface type.
///
/// Derived from graphql-js 16.12.0, src/utilities/printSchema.ts, MIT.
fn print_interface(ty: &InterfaceType) -> Result<String, DefinitionError> {
    Ok(format!(
        "{}interface {}{}{}",
        print_description(ty.description.as_deref(), "", true),
        ty.name,
        print_implemented_interfaces(ty)?,
        print_fields(ty)?
    ))
}

/// Prints a union type.
///
/// The `join` of the reference calls `toString` on each member, which is
/// its name.
///
/// Derived from graphql-js 16.12.0, src/utilities/printSchema.ts, MIT.
fn print_union(ty: &UnionType) -> Result<String, DefinitionError> {
    let types = ty.get_types()?;
    let possible_types = if types.is_empty() {
        String::new()
    } else {
        format!(
            " = {}",
            types
                .iter()
                .map(NamedType::name)
                .collect::<Vec<&str>>()
                .join(" | ")
        )
    };
    Ok(format!(
        "{}union {}{}",
        print_description(ty.description.as_deref(), "", true),
        ty.name,
        possible_types
    ))
}

/// Prints an enum type.
///
/// Derived from graphql-js 16.12.0, src/utilities/printSchema.ts, MIT.
fn print_enum(ty: &EnumType) -> Result<String, DefinitionError> {
    let values: Vec<String> = ty
        .get_values()?
        .iter()
        .enumerate()
        .map(|(i, value)| {
            format!(
                "{}  {}{}",
                print_description(value.description.as_deref(), "  ", i == 0),
                value.name,
                print_deprecated(value.deprecation_reason.as_deref())
            )
        })
        .collect();

    Ok(format!(
        "{}enum {}{}",
        print_description(ty.description.as_deref(), "", true),
        ty.name,
        print_block(&values)
    ))
}

/// Prints an input object type.
///
/// Derived from graphql-js 16.12.0, src/utilities/printSchema.ts, MIT.
fn print_input_object(ty: &InputObjectType) -> Result<String, DefinitionError> {
    let mut fields: Vec<String> = Vec::new();
    for (i, f) in ty.get_fields()?.values().enumerate() {
        fields.push(format!(
            "{}  {}",
            print_description(f.description.as_deref(), "  ", i == 0),
            print_input_value(&InputValue::from(f))?
        ));
    }
    Ok(format!(
        "{}input {}{}{}",
        print_description(ty.description.as_deref(), "", true),
        ty.name,
        if ty.is_one_of { " @oneOf" } else { "" },
        print_block(&fields)
    ))
}

/// Prints the field block of an object or an interface type.
///
/// `String(f.type)` is the `Display` of the type.
///
/// Derived from graphql-js 16.12.0, src/utilities/printSchema.ts, MIT.
fn print_fields(ty: &impl ObjectOrInterface) -> Result<String, DefinitionError> {
    let mut fields: Vec<String> = Vec::new();
    for (i, f) in ty.get_fields()?.values().enumerate() {
        fields.push(format!(
            "{}  {}{}: {}{}",
            print_description(f.description.as_deref(), "  ", i == 0),
            f.name,
            print_args(&f.args, "  ")?,
            f.r#type,
            print_deprecated(f.deprecation_reason.as_deref())
        ));
    }
    Ok(print_block(&fields))
}

/// Prints a brace block of items, one per line, or nothing for no item.
///
/// Derived from graphql-js 16.12.0, src/utilities/printSchema.ts, MIT.
fn print_block(items: &[String]) -> String {
    if items.is_empty() {
        String::new()
    } else {
        format!(" {{\n{}\n}}", items.join("\n"))
    }
}

/// Prints the argument list of a field or a directive.
///
/// The reference reads `!arg.description` for truth, and an empty string is
/// false, so an argument with an empty description counts as one without.
///
/// Derived from graphql-js 16.12.0, src/utilities/printSchema.ts, MIT.
fn print_args(args: &[Argument], indentation: &str) -> Result<String, DefinitionError> {
    if args.is_empty() {
        return Ok(String::new());
    }

    // If every arg does not have a description, print them on one line.
    if args
        .iter()
        .all(|arg| arg.description.as_deref().is_none_or(str::is_empty))
    {
        let mut printed: Vec<String> = Vec::new();
        for arg in args {
            printed.push(print_input_value(&InputValue::from(arg))?);
        }
        return Ok(format!("({})", printed.join(", ")));
    }

    let mut printed: Vec<String> = Vec::new();
    for (i, arg) in args.iter().enumerate() {
        printed.push(format!(
            "{}  {}{}",
            print_description(
                arg.description.as_deref(),
                &format!("  {indentation}"),
                i == 0
            ),
            indentation,
            print_input_value(&InputValue::from(arg))?
        ));
    }
    Ok(format!("(\n{}\n{indentation})", printed.join("\n")))
}

/// The shape `printInputValue` reads, which an argument and an input field
/// share in the reference.
struct InputValue<'a> {
    /// The name.
    name: &'a str,
    /// The type.
    r#type: &'a Type,
    /// The default value, undefined when there is none.
    default_value: &'a JsValue,
    /// The deprecation reason.
    deprecation_reason: Option<&'a str>,
}

impl<'a> From<&'a Argument> for InputValue<'a> {
    fn from(arg: &'a Argument) -> Self {
        Self {
            name: &arg.name,
            r#type: &arg.r#type,
            default_value: &arg.default_value,
            deprecation_reason: arg.deprecation_reason.as_deref(),
        }
    }
}

impl<'a> From<&'a InputField> for InputValue<'a> {
    fn from(field: &'a InputField) -> Self {
        Self {
            name: &field.name,
            r#type: &field.r#type,
            default_value: &field.default_value,
            deprecation_reason: field.deprecation_reason.as_deref(),
        }
    }
}

/// Prints an argument or an input field with its default value.
///
/// Derived from graphql-js 16.12.0, src/utilities/printSchema.ts, MIT.
fn print_input_value(arg: &InputValue<'_>) -> Result<String, DefinitionError> {
    let default_ast = ast_from_value(arg.default_value, arg.r#type)?;
    let mut arg_decl = format!("{}: {}", arg.name, arg.r#type);
    if let Some(default_ast) = default_ast {
        arg_decl.push_str(" = ");
        arg_decl.push_str(&print(&default_ast));
    }
    Ok(arg_decl + &print_deprecated(arg.deprecation_reason))
}

/// Prints a directive definition.
///
/// Derived from graphql-js 16.12.0, src/utilities/printSchema.ts, MIT.
fn print_directive(directive: &Directive) -> Result<String, DefinitionError> {
    Ok(format!(
        "{}directive @{}{}{} on {}",
        print_description(directive.description.as_deref(), "", true),
        directive.name,
        print_args(&directive.args, "")?,
        if directive.is_repeatable {
            " repeatable"
        } else {
            ""
        },
        match &directive.location_names {
            Some(names) => names.join(" | "),
            None => directive
                .locations
                .iter()
                .map(|location| location.as_str())
                .collect::<Vec<&str>>()
                .join(" | "),
        }
    ))
}

/// Prints the `@deprecated` directive of a field, an argument, an input
/// field or an enum value.
///
/// Derived from graphql-js 16.12.0, src/utilities/printSchema.ts, MIT.
fn print_deprecated(reason: Option<&str>) -> String {
    let Some(reason) = reason else {
        return String::new();
    };
    if reason != DEFAULT_DEPRECATION_REASON {
        let ast_value = print(&StringValueNode {
            loc: None,
            value: String::from(reason),
            block: None,
        });
        return format!(" @deprecated(reason: {ast_value})");
    }
    String::from(" @deprecated")
}

/// Prints the `@specifiedBy` directive of a scalar.
///
/// Derived from graphql-js 16.12.0, src/utilities/printSchema.ts, MIT.
fn print_specified_by_url(scalar: &ScalarType) -> String {
    let Some(specified_by_url) = &scalar.specified_by_url else {
        return String::new();
    };
    let ast_value = print(&StringValueNode {
        loc: None,
        value: specified_by_url.clone(),
        block: None,
    });
    format!(" @specifiedBy(url: {ast_value})")
}

/// Prints a description above a definition, as a block string where the
/// text allows it.
///
/// `description` is the description of the definition, `indentation` the
/// text before each line of it, and `first_in_block` whether the definition
/// opens its block, because a later definition of a block gets a blank line
/// before its description. The reference reads `indentation` for truth, and
/// an empty string is false.
///
/// Derived from graphql-js 16.12.0, src/utilities/printSchema.ts, MIT.
fn print_description(description: Option<&str>, indentation: &str, first_in_block: bool) -> String {
    let Some(description) = description else {
        return String::new();
    };

    let block_string = print(&StringValueNode {
        loc: None,
        value: String::from(description),
        block: Some(is_printable_as_block_string(description)),
    });

    let prefix = if !indentation.is_empty() && !first_in_block {
        format!("\n{indentation}")
    } else {
        String::from(indentation)
    };

    format!(
        "{prefix}{}\n",
        block_string.replace('\n', &format!("\n{indentation}"))
    )
}
