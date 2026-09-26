// SPDX-License-Identifier: MIT
/*
 * aosabi-type: the schema validation, the type validation rules of the
 * specification over a built schema.
 *
 * The two type comparators the interface rules call and the overlap test the
 * fragment rules of the validation layer call.
 *
 * Derived from graphql-js 16.12.0, src/type/validate.ts, MIT.
 * Derived from graphql-js 16.12.0, src/utilities/typeComparators.ts, MIT.
 */

//! The schema validation of the type layer.
//!
//! [`validate_schema()`] runs every rule of the "Type Validation" sub sections
//! of the specification over a [`Schema`] and stores the errors on it, and
//! [`assert_valid_schema()`] turns a non empty list into the one plain error
//! whose message is every message joined with a blank line, which is the text
//! the build guard prints. Every message is the reference's byte for byte,
//! and the nodes each rule blames are the reference's, in the reference's
//! order, because the locations of an error are computed from them.
//!
//! # The shape of the port
//!
//! A type of the reference reads its fields, interfaces, members and values
//! through thunks that may throw, and a throw leaves `validateSchema`. Every
//! rule that reads one returns a `Result` here for the same reason, and the
//! error is the [`DefinitionError`] of the thunk.
//!
//! The reference checks at run time that a root type is an object type, that an
//! implemented interface is an interface type and that a union member is an
//! object type, and the three slots hold any named type here as they do there,
//! so each of those checks reports as the reference's does. It also checks that
//! a directive is a directive and that a type of the type map is a named type.
//! The schema carries each of those two as its type here, so the failing branch
//! of each has no reading, and the function that holds it says so. `inspect` of
//! a type in a message is the type's `toString`, because the reference's
//! `inspect` calls the `toJSON` of an object that has one and every type
//! returns its string form from it, and that string form is the `Display` of
//! [`Type`].

use alloc::{
    format,
    string::String,
    sync::Arc,
    vec::Vec, //
};

use aosabi_json::map::Values;
use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions,
    ast::{
        AstNode,
        DirectiveNode,
        FieldDefinitionNode,
        InputValueDefinitionNode,
        NamedTypeNode,
        OperationTypeNode, //
    },
    js_string::join, //
};

use crate::{
    definition::{
        DefinitionError,
        EnumType,
        Field,
        InputField,
        InputObjectType,
        InterfaceType,
        NamedType,
        ObjMap,
        ObjectType,
        Type,
        UnionType,
        get_named_type,
        is_abstract_type,
        is_input_type,
        is_interface_type,
        is_list_type,
        is_non_null_type,
        is_object_type,
        is_output_type,
        is_required_argument,
        is_required_input_field, //
    },
    directives::graphql_deprecated_directive,
    introspection::is_introspection_type,
    schema::Schema, //
};

/// Implements the "Type Validation" sub-sections of the specification's
/// "Type System" section.
///
/// Validation runs synchronously, returning an array of encountered errors, or
/// an empty array if no errors were encountered and the Schema is valid. The
/// `assertSchema` call has no reading, because the argument type carries it.
/// The list is stored on `schema` once computed and a later call returns the
/// stored list, which is the empty list for a schema built with `assume_valid`.
///
/// Derived from graphql-js 16.12.0, src/type/validate.ts, MIT.
///
/// # Errors
///
/// Returns what a thunk of a type throws while a rule reads it.
pub fn validate_schema(schema: &Schema) -> Result<&[GraphQLError], DefinitionError> {
    // If this Schema has already been validated, return the previous results.
    if let Some(errors) = schema.validation_errors() {
        return Ok(errors);
    }
    validate_new(schema)
}

/// Validates `schema`, which holds no result of a validation yet, and stores
/// the result on it.
///
/// The body of `validateSchema` after its check of the stored result, kept
/// out of line, so a schema built with `assume_valid`, whose result stands
/// from its construction, compiles none of the rules.
///
/// # Errors
///
/// Returns what [`validate_schema()`] returns.
#[inline(never)]
fn validate_new(schema: &Schema) -> Result<&[GraphQLError], DefinitionError> {
    // Validate the schema, producing a list of errors.
    let mut context = SchemaValidationContext::new(schema);
    validate_root_types(&mut context);
    validate_directives(&mut context);
    validate_types(&mut context)?;

    // Persist the results of validation before returning to ensure validation
    // does not run multiple times for this schema.
    let errors = context.get_errors();
    Ok(schema.set_validation_errors(errors))
}

/// Utility function which asserts a schema is valid by throwing an error if
/// it is invalid.
///
/// The thrown `Error` carries every message joined with a blank line, and a
/// plain [`DefinitionError`] carries it here.
///
/// Derived from graphql-js 16.12.0, src/type/validate.ts, MIT.
///
/// # Errors
///
/// Returns the joined messages when the schema is invalid, and what
/// [`validate_schema()`] throws.
pub fn assert_valid_schema(schema: &Schema) -> Result<(), DefinitionError> {
    let errors = validate_schema(schema)?;
    if !errors.is_empty() {
        let messages = errors.iter().map(|error| &error.message);
        return Err(DefinitionError::plain(join(messages, "\n\n")));
    }
    Ok(())
}

/// The errors collected so far and the schema they are collected over.
///
/// The lifetime is the schema's, so a rule may hold a field or a type of the
/// schema across a report.
///
/// Derived from graphql-js 16.12.0, src/type/validate.ts, MIT.
struct SchemaValidationContext<'s> {
    errors: Vec<GraphQLError>,
    schema: &'s Schema,
}

impl<'s> SchemaValidationContext<'s> {
    fn new(schema: &'s Schema) -> Self {
        Self {
            errors: Vec::new(),
            schema,
        }
    }

    /// Records an error with `message` that blames the present nodes of
    /// `nodes`.
    ///
    /// The reference takes one node or a list of nodes and keeps the present
    /// entries of a list, and a single node reaches the error as a one
    /// element list either way, so one shape covers both call forms.
    fn report_error(&mut self, message: String, nodes: impl IntoIterator<Item = Option<AstNode>>) {
        let nodes: Vec<AstNode> = nodes.into_iter().flatten().collect();
        self.errors.push(GraphQLError::new(
            message,
            GraphQLErrorOptions {
                nodes: Some(nodes),
                ..GraphQLErrorOptions::default()
            },
        ));
    }

    fn get_errors(self) -> Vec<GraphQLError> {
        self.errors
    }
}

/// Checks that the query root type is provided and that every root type is
/// an object type.
///
/// A root that is not an object type is blamed on its operation type node, and
/// on its own definition node where the schema has none.
///
/// Derived from graphql-js 16.12.0, src/type/validate.ts, MIT.
fn validate_root_types(context: &mut SchemaValidationContext<'_>) {
    let schema = context.schema;
    let query_type = schema.get_query_type();
    match query_type {
        None => context.report_error(
            String::from("Query root type must be provided."),
            [node(schema.ast_node.as_ref())],
        ),
        Some(query_type) if !is_object_type(&Type::from(query_type)) => context.report_error(
            format!("Query root type must be Object type, it cannot be {query_type}."),
            [get_operation_type_node(schema, OperationTypeNode::Query)
                .or_else(|| named_type_node(query_type))],
        ),
        Some(_) => {}
    }

    let mutation_type = schema.get_mutation_type();
    if let Some(mutation_type) = mutation_type
        && !is_object_type(&Type::from(mutation_type))
    {
        context.report_error(
            format!(
                "Mutation root type must be Object type if provided, it cannot be {mutation_type}."
            ),
            [get_operation_type_node(schema, OperationTypeNode::Mutation)
                .or_else(|| named_type_node(mutation_type))],
        );
    }

    let subscription_type = schema.get_subscription_type();
    if let Some(subscription_type) = subscription_type
        && !is_object_type(&Type::from(subscription_type))
    {
        context.report_error(
            format!(
                "Subscription root type must be Object type if provided, it cannot be {subscription_type}."
            ),
            [get_operation_type_node(schema, OperationTypeNode::Subscription)
                .or_else(|| named_type_node(subscription_type))],
        );
    }
}

/// Returns the type node of the operation type definition of `operation`,
/// read from the schema definition and its extensions.
///
/// It is the node the root type rules blame when a root type is not an object
/// type.
///
/// Derived from graphql-js 16.12.0, src/type/validate.ts, MIT.
#[must_use]
pub fn get_operation_type_node(schema: &Schema, operation: OperationTypeNode) -> Option<AstNode> {
    let definition = schema
        .ast_node
        .iter()
        .flat_map(|schema_node| schema_node.operation_types.iter());
    let extensions = schema
        .extension_ast_nodes
        .iter()
        .flat_map(|schema_node| schema_node.operation_types.iter().flatten());
    definition
        .chain(extensions)
        .find(|operation_node| operation_node.operation == operation)
        .map(|operation_node| AstNode::from(operation_node.r#type.clone()))
}

/// Checks the name, the locations and the arguments of every directive.
///
/// The `isDirective` branch has no reading, because the schema carries
/// directives and nothing else.
///
/// Derived from graphql-js 16.12.0, src/type/validate.ts, MIT.
fn validate_directives(context: &mut SchemaValidationContext<'_>) {
    let schema = context.schema;
    for directive in schema.get_directives() {
        // Ensure they are named correctly.
        validate_name(context, &directive.name, || {
            node(directive.ast_node.as_ref())
        });

        let has_location = directive
            .location_names
            .as_ref()
            .map_or(!directive.locations.is_empty(), |names| !names.is_empty());
        if !has_location {
            context.report_error(
                format!(
                    "Directive @{} must include 1 or more locations.",
                    directive.name
                ),
                [node(directive.ast_node.as_ref())],
            );
        }

        // Ensure the arguments are valid.
        for arg in &directive.args {
            // Ensure they are named correctly.
            validate_name(context, &arg.name, || node(arg.ast_node.as_ref()));

            // Ensure the type is an input type.
            if !is_input_type(&arg.r#type) {
                context.report_error(
                    format!(
                        "The type of @{}({}:) must be Input Type but got: {}.",
                        directive.name, arg.name, arg.r#type
                    ),
                    [node(arg.ast_node.as_ref())],
                );
            }

            if is_required_argument(arg) && arg.deprecation_reason.is_some() {
                context.report_error(
                    format!(
                        "Required argument @{}({}:) cannot be deprecated.",
                        directive.name, arg.name
                    ),
                    [
                        get_deprecated_directive_node(arg.ast_node.as_ref()).map(AstNode::from),
                        input_value_type_node(arg.ast_node.as_ref()),
                    ],
                );
            }
        }
    }
}

/// Reports `name` when it begins with two underscores.
///
/// The reference reads the node off the definition, and `ast_node` produces it
/// here, so the node is cloned into the error only when the rule fires.
///
/// Derived from graphql-js 16.12.0, src/type/validate.ts, MIT.
fn validate_name(
    context: &mut SchemaValidationContext<'_>,
    name: &str,
    ast_node: impl FnOnce() -> Option<AstNode>,
) {
    // Ensure names are valid, however introspection types opt out.
    if name.starts_with("__") {
        context.report_error(
            format!(
                "Name \"{name}\" must not begin with \"__\", which is reserved by GraphQL introspection."
            ),
            [ast_node()],
        );
    }
}

/// Checks every type of the type map by its kind.
///
/// The `isNamedType` branch has no reading, because the type map holds named
/// types and nothing else.
///
/// Derived from graphql-js 16.12.0, src/type/validate.ts, MIT.
fn validate_types(context: &mut SchemaValidationContext<'_>) -> Result<(), DefinitionError> {
    let mut validate_input_object_circular_refs = InputObjectCircularRefsValidator::new();
    let schema = context.schema;
    let type_map = schema.get_type_map();
    for ty in type_map.values() {
        // Ensure it is named correctly (excluding introspection types).
        if !is_introspection_type(ty) {
            validate_name(context, ty.name(), || named_type_node(ty));
        }

        match ty {
            NamedType::Object(object) => {
                let ty = ObjectOrInterface::Object(object);
                // Ensure fields are valid
                validate_fields(context, ty)?;

                // Ensure objects implement the interfaces they claim to.
                validate_interfaces(context, ty)?;
            }
            NamedType::Interface(interface) => {
                let ty = ObjectOrInterface::Interface(interface);
                // Ensure fields are valid.
                validate_fields(context, ty)?;

                // Ensure interfaces implement the interfaces they claim to.
                validate_interfaces(context, ty)?;
            }
            NamedType::Union(union) => {
                // Ensure Unions include valid member types.
                validate_union_members(context, union)?;
            }
            NamedType::Enum(enum_type) => {
                // Ensure Enums have valid values.
                validate_enum_values(context, enum_type)?;
            }
            NamedType::InputObject(input_obj) => {
                // Ensure Input Object fields are valid.
                validate_input_fields(context, input_obj)?;

                // Ensure Input Objects do not contain non-nullable circular references
                validate_input_object_circular_refs.detect_cycle_recursive(context, input_obj)?;
            }
            NamedType::Scalar(_) => {}
        }
    }
    Ok(())
}

/// An object type or an interface type, the two kinds that carry fields and
/// implement interfaces.
///
/// The reference types the field and interface rules over
/// `GraphQLObjectType | GraphQLInterfaceType`, and this enum is that union.
/// The lifetime is the schema's.
#[derive(Clone, Copy)]
enum ObjectOrInterface<'s> {
    Object(&'s ObjectType),
    Interface(&'s InterfaceType),
}

impl<'s> ObjectOrInterface<'s> {
    fn name(self) -> &'s str {
        match self {
            Self::Object(object) => &object.name,
            Self::Interface(interface) => &interface.name,
        }
    }

    fn get_fields(self) -> Result<&'s ObjMap<Field>, DefinitionError> {
        match self {
            Self::Object(object) => object.get_fields(),
            Self::Interface(interface) => interface.get_fields(),
        }
    }

    fn get_interfaces(self) -> Result<&'s [NamedType], DefinitionError> {
        match self {
            Self::Object(object) => object.get_interfaces(),
            Self::Interface(interface) => interface.get_interfaces(),
        }
    }

    /// Returns whether the type is `other`, which the reference tests with
    /// `===`.
    ///
    /// Two types are the same value when they are the same allocation, and
    /// types of two kinds never are.
    fn is(self, other: &NamedType) -> bool {
        match (self, other) {
            (Self::Object(object), NamedType::Object(other)) => {
                core::ptr::eq(object, Arc::as_ptr(other))
            }
            (Self::Interface(interface), NamedType::Interface(other)) => {
                core::ptr::eq(interface, Arc::as_ptr(other))
            }
            _ => false,
        }
    }

    /// Returns `[astNode, ...extensionASTNodes]`, the nodes a rule over the
    /// whole type blames.
    fn definition_nodes(self) -> Vec<Option<AstNode>> {
        match self {
            Self::Object(object) => {
                definition_nodes(object.ast_node.as_ref(), &object.extension_ast_nodes)
            }
            Self::Interface(interface) => {
                definition_nodes(interface.ast_node.as_ref(), &interface.extension_ast_nodes)
            }
        }
    }

    /// Returns the `interfaces` entries of the definition node and of every
    /// extension node, in that order.
    ///
    /// The reference builds `[astNode, ...extensionASTNodes]` when the
    /// definition node exists and `extensionASTNodes` otherwise, and reads
    /// `typeNode.interfaces ?? []` of each, which is one chain here.
    fn implements_interface_nodes(self) -> Vec<&'s NamedTypeNode> {
        match self {
            Self::Object(object) => object
                .ast_node
                .iter()
                .flat_map(|type_node| type_node.interfaces.iter().flatten())
                .chain(
                    object
                        .extension_ast_nodes
                        .iter()
                        .flat_map(|type_node| type_node.interfaces.iter().flatten()),
                )
                .collect(),
            Self::Interface(interface) => interface
                .ast_node
                .iter()
                .flat_map(|type_node| type_node.interfaces.iter().flatten())
                .chain(
                    interface
                        .extension_ast_nodes
                        .iter()
                        .flat_map(|type_node| type_node.interfaces.iter().flatten()),
                )
                .collect(),
        }
    }
}

/// Checks that the type has fields and that every field and argument is
/// well formed.
///
/// Derived from graphql-js 16.12.0, src/type/validate.ts, MIT.
fn validate_fields<'s>(
    context: &mut SchemaValidationContext<'s>,
    ty: ObjectOrInterface<'s>,
) -> Result<(), DefinitionError> {
    let fields = ty.get_fields()?;

    // Objects and Interfaces both must define one or more fields.
    if fields.is_empty() {
        context.report_error(
            format!("Type {} must define one or more fields.", ty.name()),
            ty.definition_nodes(),
        );
    }

    for field in fields.values() {
        // Ensure they are named correctly.
        validate_name(context, &field.name, || node(field.ast_node.as_ref()));

        // Ensure the type is an output type
        if !is_output_type(&field.r#type) {
            context.report_error(
                format!(
                    "The type of {}.{} must be Output Type but got: {}.",
                    ty.name(),
                    field.name,
                    field.r#type
                ),
                [field_type_node(field.ast_node.as_ref())],
            );
        }

        // Ensure the arguments are valid
        for arg in &field.args {
            let arg_name = &arg.name;

            // Ensure they are named correctly.
            validate_name(context, arg_name, || node(arg.ast_node.as_ref()));

            // Ensure the type is an input type
            if !is_input_type(&arg.r#type) {
                context.report_error(
                    format!(
                        "The type of {}.{}({}:) must be Input Type but got: {}.",
                        ty.name(),
                        field.name,
                        arg_name,
                        arg.r#type
                    ),
                    [input_value_type_node(arg.ast_node.as_ref())],
                );
            }

            if is_required_argument(arg) && arg.deprecation_reason.is_some() {
                context.report_error(
                    format!(
                        "Required argument {}.{}({}:) cannot be deprecated.",
                        ty.name(),
                        field.name,
                        arg_name
                    ),
                    [
                        get_deprecated_directive_node(arg.ast_node.as_ref()).map(AstNode::from),
                        input_value_type_node(arg.ast_node.as_ref()),
                    ],
                );
            }
        }
    }
    Ok(())
}

/// Checks that the type implements each of its interfaces once, and
/// implements each one and its ancestors.
///
/// The set of seen names holds `true` under every name it has, so a key test is
/// the reference's truthiness test.
///
/// Derived from graphql-js 16.12.0, src/type/validate.ts, MIT.
fn validate_interfaces<'s>(
    context: &mut SchemaValidationContext<'s>,
    ty: ObjectOrInterface<'s>,
) -> Result<(), DefinitionError> {
    let mut iface_type_names: ObjMap<bool> = ObjMap::default();
    for iface_type in ty.get_interfaces()? {
        let NamedType::Interface(iface) = iface_type else {
            context.report_error(
                format!(
                    "Type {} must only implement Interface types, it cannot implement {}.",
                    ty.name(),
                    iface_type
                ),
                get_all_implements_interface_nodes(ty, iface_type.name())
                    .into_iter()
                    .map(AstNode::from)
                    .map(Some),
            );
            continue;
        };

        if ty.is(iface_type) {
            context.report_error(
                format!(
                    "Type {} cannot implement itself because it would create a circular reference.",
                    ty.name()
                ),
                get_all_implements_interface_nodes(ty, &iface.name)
                    .into_iter()
                    .map(AstNode::from)
                    .map(Some),
            );
            continue;
        }

        if iface_type_names.contains_key(iface.name.as_str()) {
            context.report_error(
                format!("Type {} can only implement {} once.", ty.name(), iface.name),
                get_all_implements_interface_nodes(ty, &iface.name)
                    .into_iter()
                    .map(AstNode::from)
                    .map(Some),
            );
            continue;
        }

        iface_type_names.insert(iface.name.clone(), true);

        validate_type_implements_ancestors(context, ty, iface)?;
        validate_type_implements_interface(context, ty, iface)?;
    }
    Ok(())
}

/// Checks that every field and argument of `iface` is provided by `ty` with
/// a compatible type, and that `ty` adds no required argument.
///
/// Derived from graphql-js 16.12.0, src/type/validate.ts, MIT.
fn validate_type_implements_interface<'s>(
    context: &mut SchemaValidationContext<'s>,
    ty: ObjectOrInterface<'s>,
    iface: &'s InterfaceType,
) -> Result<(), DefinitionError> {
    let type_field_map = ty.get_fields()?;

    // Assert each interface field is implemented.
    for iface_field in iface.get_fields()?.values() {
        let field_name = &iface_field.name;

        // Assert interface field exists on type.
        let Some(type_field) = type_field_map.get(field_name.as_str()) else {
            context.report_error(
                format!(
                    "Interface field {}.{} expected but {} does not provide it.",
                    iface.name,
                    field_name,
                    ty.name()
                ),
                [node(iface_field.ast_node.as_ref())]
                    .into_iter()
                    .chain(ty.definition_nodes()),
            );
            continue;
        };

        // Assert interface field type is satisfied by type field type, by being
        // a valid subtype. (covariant)
        if !is_type_sub_type_of(context.schema, &type_field.r#type, &iface_field.r#type)? {
            context.report_error(
                format!(
                    "Interface field {}.{} expects type {} but {}.{} is type {}.",
                    iface.name,
                    field_name,
                    iface_field.r#type,
                    ty.name(),
                    field_name,
                    type_field.r#type
                ),
                [
                    field_type_node(iface_field.ast_node.as_ref()),
                    field_type_node(type_field.ast_node.as_ref()),
                ],
            );
        }

        // Assert each interface field arg is implemented.
        for iface_arg in &iface_field.args {
            let arg_name = &iface_arg.name;

            // Assert interface field arg exists on object field.
            let Some(type_arg) = type_field.args.iter().find(|arg| &arg.name == arg_name) else {
                context.report_error(
                    format!(
                        "Interface field argument {}.{}({}:) expected but {}.{} does not provide it.",
                        iface.name,
                        field_name,
                        arg_name,
                        ty.name(),
                        field_name
                    ),
                    [node(iface_arg.ast_node.as_ref()), node(type_field.ast_node.as_ref())],
                );
                continue;
            };

            // Assert interface field arg type matches object field arg type.
            // (invariant)
            if !is_equal_type(&iface_arg.r#type, &type_arg.r#type) {
                context.report_error(
                    format!(
                        "Interface field argument {}.{}({}:) expects type {} but {}.{}({}:) is type {}.",
                        iface.name,
                        field_name,
                        arg_name,
                        iface_arg.r#type,
                        ty.name(),
                        field_name,
                        arg_name,
                        type_arg.r#type
                    ),
                    [
                        input_value_type_node(iface_arg.ast_node.as_ref()),
                        input_value_type_node(type_arg.ast_node.as_ref()),
                    ],
                );
            }
        }

        // Assert additional arguments must not be required.
        for type_arg in &type_field.args {
            let arg_name = &type_arg.name;
            let iface_arg = iface_field.args.iter().find(|arg| &arg.name == arg_name);
            if iface_arg.is_none() && is_required_argument(type_arg) {
                context.report_error(
                    format!(
                        "Object field {}.{} includes required argument {} that is missing from the Interface field {}.{}.",
                        ty.name(),
                        field_name,
                        arg_name,
                        iface.name,
                        field_name
                    ),
                    [node(type_arg.ast_node.as_ref()), node(iface_field.ast_node.as_ref())],
                );
            }
        }
    }
    Ok(())
}

/// Checks that `ty` implements every interface that `iface` implements.
///
/// The `includes` of the reference compares by identity, which is the identity
/// of the allocation here, and an ancestor is whatever named type the interface
/// list of `iface` holds.
///
/// Derived from graphql-js 16.12.0, src/type/validate.ts, MIT.
fn validate_type_implements_ancestors<'s>(
    context: &mut SchemaValidationContext<'s>,
    ty: ObjectOrInterface<'s>,
    iface: &'s InterfaceType,
) -> Result<(), DefinitionError> {
    let iface_interfaces = ty.get_interfaces()?;
    for transitive in iface.get_interfaces()? {
        if !iface_interfaces.contains(transitive) {
            let message = if ty.is(transitive) {
                format!(
                    "Type {} cannot implement {} because it would create a circular reference.",
                    ty.name(),
                    iface.name
                )
            } else {
                format!(
                    "Type {} must implement {} because it is implemented by {}.",
                    ty.name(),
                    transitive.name(),
                    iface.name
                )
            };
            context.report_error(
                message,
                get_all_implements_interface_nodes(
                    ObjectOrInterface::Interface(iface),
                    transitive.name(),
                )
                .into_iter()
                .chain(get_all_implements_interface_nodes(ty, &iface.name))
                .map(AstNode::from)
                .map(Some),
            );
        }
    }
    Ok(())
}

/// Checks that the union has members and lists each one once.
///
/// The set of included names holds `true` under every name it has, so a key
/// test is the reference's truthiness test, and `String(memberType)` is the
/// name of the member.
///
/// Derived from graphql-js 16.12.0, src/type/validate.ts, MIT.
fn validate_union_members<'s>(
    context: &mut SchemaValidationContext<'s>,
    union: &'s UnionType,
) -> Result<(), DefinitionError> {
    let member_types = union.get_types()?;

    if member_types.is_empty() {
        context.report_error(
            format!(
                "Union type {} must define one or more member types.",
                union.name
            ),
            definition_nodes(union.ast_node.as_ref(), &union.extension_ast_nodes),
        );
    }

    let mut included_type_names: ObjMap<bool> = ObjMap::default();
    for member_type in member_types {
        if included_type_names.contains_key(member_type.name()) {
            context.report_error(
                format!(
                    "Union type {} can only include type {} once.",
                    union.name,
                    member_type.name()
                ),
                get_union_member_type_nodes(union, member_type.name())
                    .into_iter()
                    .map(AstNode::from)
                    .map(Some),
            );
            continue;
        }
        included_type_names.insert(String::from(member_type.name()), true);
        if !is_object_type(&Type::from(member_type)) {
            context.report_error(
                format!(
                    "Union type {} can only include Object types, it cannot include {}.",
                    union.name, member_type
                ),
                get_union_member_type_nodes(union, member_type.name())
                    .into_iter()
                    .map(AstNode::from)
                    .map(Some),
            );
        }
    }
    Ok(())
}

/// Checks that the enum has values and that each is named validly.
///
/// Derived from graphql-js 16.12.0, src/type/validate.ts, MIT.
fn validate_enum_values<'s>(
    context: &mut SchemaValidationContext<'s>,
    enum_type: &'s EnumType,
) -> Result<(), DefinitionError> {
    let enum_values = enum_type.get_values()?;

    if enum_values.is_empty() {
        context.report_error(
            format!(
                "Enum type {} must define one or more values.",
                enum_type.name
            ),
            definition_nodes(enum_type.ast_node.as_ref(), &enum_type.extension_ast_nodes),
        );
    }

    for enum_value in enum_values {
        // Ensure valid name.
        validate_name(context, &enum_value.name, || {
            node(enum_value.ast_node.as_ref())
        });
    }
    Ok(())
}

/// Checks that the input object has fields and that every field is well
/// formed.
///
/// Derived from graphql-js 16.12.0, src/type/validate.ts, MIT.
fn validate_input_fields<'s>(
    context: &mut SchemaValidationContext<'s>,
    input_obj: &'s InputObjectType,
) -> Result<(), DefinitionError> {
    let fields = input_obj.get_fields()?;

    if fields.is_empty() {
        context.report_error(
            format!(
                "Input Object type {} must define one or more fields.",
                input_obj.name
            ),
            definition_nodes(input_obj.ast_node.as_ref(), &input_obj.extension_ast_nodes),
        );
    }

    // Ensure the arguments are valid
    for field in fields.values() {
        // Ensure they are named correctly.
        validate_name(context, &field.name, || node(field.ast_node.as_ref()));

        // Ensure the type is an input type
        if !is_input_type(&field.r#type) {
            context.report_error(
                format!(
                    "The type of {}.{} must be Input Type but got: {}.",
                    input_obj.name, field.name, field.r#type
                ),
                [input_value_type_node(field.ast_node.as_ref())],
            );
        }

        if is_required_input_field(field) && field.deprecation_reason.is_some() {
            context.report_error(
                format!(
                    "Required input field {}.{} cannot be deprecated.",
                    input_obj.name, field.name
                ),
                [
                    get_deprecated_directive_node(field.ast_node.as_ref()).map(AstNode::from),
                    input_value_type_node(field.ast_node.as_ref()),
                ],
            );
        }

        if input_obj.is_one_of {
            validate_one_of_input_object_field(input_obj, field, context);
        }
    }
    Ok(())
}

/// Checks that a field of a one of input object is nullable and has no
/// default value.
///
/// The reference tests `defaultValue !== undefined`, and the undefined variant
/// of the value model is that absence.
///
/// Derived from graphql-js 16.12.0, src/type/validate.ts, MIT.
fn validate_one_of_input_object_field(
    ty: &InputObjectType,
    field: &InputField,
    context: &mut SchemaValidationContext<'_>,
) {
    if is_non_null_type(&field.r#type) {
        context.report_error(
            format!(
                "OneOf input field {}.{} must be nullable.",
                ty.name, field.name
            ),
            [input_value_type_node(field.ast_node.as_ref())],
        );
    }

    if !field.default_value.is_undefined() {
        context.report_error(
            format!(
                "OneOf input field {}.{} cannot have a default value.",
                ty.name, field.name
            ),
            [node(field.ast_node.as_ref())],
        );
    }
}

/// The deepest chain of input objects through non null fields the search of
/// the reference follows before its stack ends.
///
/// The reference searches by recursion, one call per input object of the chain,
/// and its stack ends with the `RangeError` `Maximum call stack size exceeded`
/// past this depth: a served front of the reference validates the wrapping
/// schema of a remote whose chain holds 3,822 input objects and throws for
/// 3,823, under production and under development alike. The search here holds
/// its chain on the heap, so no chain ends the stack of a card, and it throws
/// the same message past the same depth, which the reference's stack sets on
/// that machine and no other.
const INPUT_CYCLE_DEPTH: usize = 3822;

/// The state of the search for input object cycles through non null fields.
///
/// The reference returns a closure over three variables and the context, and a
/// struct with a method carries the three here, because the caller's loop holds
/// the context mutably as well. Modified copy of algorithm from
/// `src/validation/rules/NoFragmentCycles.js`. The lifetime is the schema's,
/// because the path holds its fields.
///
/// Derived from graphql-js 16.12.0, src/type/validate.ts, MIT.
struct InputObjectCircularRefsValidator<'s> {
    /// Tracks already visited types to maintain O(N) and to ensure that
    /// cycles are not redundantly reported.
    visited_types: ObjMap<bool>,
    /// Array of types nodes used to produce meaningful errors.
    field_path: Vec<&'s InputField>,
    /// Position in the type path.
    field_path_index_by_type_name: ObjMap<Option<usize>>,
}

/// One input object of the chain the search follows, and the fields it reads next.
struct Visit<'s> {
    /// The input object.
    input_obj: &'s InputObjectType,
    /// Its fields in order, from the one the search reads next.
    fields: Values<'s, String, InputField>,
}

impl<'s> InputObjectCircularRefsValidator<'s> {
    fn new() -> Self {
        Self {
            visited_types: ObjMap::default(),
            field_path: Vec::new(),
            field_path_index_by_type_name: ObjMap::default(),
        }
    }

    /// Reports every cycle of non null input object fields that starts at
    /// `input_obj`.
    ///
    /// This does a straight-forward DFS to find cycles. It does not
    /// terminate when a cycle was found but continues to explore the graph
    /// to find all possible cycles. The reference clears the position of a
    /// type by assigning `undefined`, and a key with `None` is that entry,
    /// so a missing key and a cleared key read the same. The reference
    /// recurses once per input object of the chain, and the chain here
    /// stands in a list, which visits the types, reads their fields and
    /// reports the cycles in the order of the recursion.
    ///
    /// # Errors
    ///
    /// Returns what a thunk of the fields of a visited type throws, and the
    /// plain error `Maximum call stack size exceeded` for a chain deeper than
    /// [`INPUT_CYCLE_DEPTH`], which the reference throws there.
    fn detect_cycle_recursive(
        &mut self,
        context: &mut SchemaValidationContext<'_>,
        input_obj: &'s InputObjectType,
    ) -> Result<(), DefinitionError> {
        let Some(first) = self.enter(input_obj)? else {
            return Ok(());
        };
        let mut chain = Vec::new();
        chain.push(first);
        while let Some(visit) = chain.last_mut() {
            let Some(field) = visit.fields.next() else {
                self.field_path_index_by_type_name
                    .insert(visit.input_obj.name.clone(), None);
                chain.pop();
                if !chain.is_empty() {
                    self.field_path.pop();
                }
                continue;
            };
            let Some(field_type) = non_null_input_object_type(&field.r#type) else {
                continue;
            };
            let cycle_index = self
                .field_path_index_by_type_name
                .get(field_type.name.as_str())
                .copied()
                .flatten();
            self.field_path.push(field);
            match cycle_index {
                None => {
                    if self.visited_types.contains_key(field_type.name.as_str()) {
                        self.field_path.pop();
                        continue;
                    }
                    if chain.len() >= INPUT_CYCLE_DEPTH {
                        return Err(DefinitionError::plain("Maximum call stack size exceeded"));
                    }
                    if let Some(next) = self.enter(field_type)? {
                        chain.push(next);
                    }
                }
                Some(cycle_index) => {
                    let cycle_path = &self.field_path[cycle_index..];
                    let mut path_str = String::new();
                    for (index, field_obj) in cycle_path.iter().enumerate() {
                        if index > 0 {
                            path_str.push('.');
                        }
                        path_str.push_str(&field_obj.name);
                    }
                    context.report_error(
                        format!(
                            "Cannot reference Input Object \"{}\" within itself through a series of non-null fields: \"{}\".",
                            field_type.name, path_str
                        ),
                        cycle_path.iter().map(|field_obj| node(field_obj.ast_node.as_ref())),
                    );
                    self.field_path.pop();
                }
            }
        }
        Ok(())
    }

    /// Marks `input_obj` visited and answers its visit, or none for a type
    /// visited before, the head of one call of the recursion of the
    /// reference.
    ///
    /// # Errors
    ///
    /// Returns what the thunk of the fields of `input_obj` throws.
    fn enter(
        &mut self,
        input_obj: &'s InputObjectType,
    ) -> Result<Option<Visit<'s>>, DefinitionError> {
        if self.visited_types.contains_key(input_obj.name.as_str()) {
            return Ok(None);
        }

        self.visited_types.insert(input_obj.name.clone(), true);
        self.field_path_index_by_type_name
            .insert(input_obj.name.clone(), Some(self.field_path.len()));

        Ok(Some(Visit {
            input_obj,
            fields: input_obj.get_fields()?.values(),
        }))
    }
}

/// Returns the input object type under a non null wrapper, when `ty` is one.
///
/// This is `isNonNullType(field.type) && isInputObjectType(field.type.ofType)`
/// with the type it narrows to.
fn non_null_input_object_type(ty: &Type) -> Option<&InputObjectType> {
    let non_null = ty.as_non_null()?;
    non_null.of_type().as_input_object().map(Arc::as_ref)
}

/// Returns the `implements` nodes of `ty` that name `iface_name`.
///
/// The reference takes the implemented type and reads its name alone, and a
/// caller hands it a type that is no interface type as well, so the name is the
/// argument here.
///
/// Derived from graphql-js 16.12.0, src/type/validate.ts, MIT.
fn get_all_implements_interface_nodes(
    ty: ObjectOrInterface<'_>,
    iface_name: &str,
) -> Vec<NamedTypeNode> {
    ty.implements_interface_nodes()
        .into_iter()
        .filter(|iface_node| iface_node.name.value() == iface_name)
        .cloned()
        .collect()
}

/// Returns the member type nodes of `union` that name `type_name`.
///
/// The reference builds `[astNode, ...extensionASTNodes]` when the definition
/// node exists and `extensionASTNodes` otherwise, and reads
/// `unionNode.types ?? []` of each, which is one chain here.
///
/// Derived from graphql-js 16.12.0, src/type/validate.ts, MIT.
fn get_union_member_type_nodes(union: &UnionType, type_name: &str) -> Vec<NamedTypeNode> {
    union
        .ast_node
        .iter()
        .flat_map(|union_node| union_node.types.iter().flatten())
        .chain(
            union
                .extension_ast_nodes
                .iter()
                .flat_map(|union_node| union_node.types.iter().flatten()),
        )
        .filter(|type_node| type_node.name.value() == type_name)
        .cloned()
        .collect()
}

/// Returns the `@deprecated` directive node of an input value definition.
///
/// The reference accepts any definition with a `directives` list, and the two
/// callers pass the node of an argument and the node of an input field, which
/// are one node type.
///
/// Derived from graphql-js 16.12.0, src/type/validate.ts, MIT.
fn get_deprecated_directive_node(
    definition_node: Option<&InputValueDefinitionNode>,
) -> Option<DirectiveNode> {
    definition_node?
        .directives
        .as_ref()?
        .iter()
        .find(|node| node.name.value() == graphql_deprecated_directive().name)
        .cloned()
}

/// Returns the definition node of a named type, when it has one.
fn named_type_node(ty: &NamedType) -> Option<AstNode> {
    match ty {
        NamedType::Scalar(scalar) => node(scalar.ast_node.as_ref()),
        NamedType::Object(object) => node(object.ast_node.as_ref()),
        NamedType::Interface(interface) => node(interface.ast_node.as_ref()),
        NamedType::Union(union) => node(union.ast_node.as_ref()),
        NamedType::Enum(enum_type) => node(enum_type.ast_node.as_ref()),
        NamedType::InputObject(input_object) => node(input_object.ast_node.as_ref()),
    }
}

/// Returns `[astNode, ...extensionASTNodes]` of a type.
fn definition_nodes<D, E>(ast_node: Option<&D>, extension_ast_nodes: &[E]) -> Vec<Option<AstNode>>
where
    D: Clone + Into<AstNode>,
    E: Clone + Into<AstNode>,
{
    let mut nodes = alloc::vec![node(ast_node)];
    nodes.extend(
        extension_ast_nodes
            .iter()
            .cloned()
            .map(Into::into)
            .map(Some),
    );
    nodes
}

/// Returns the node of `ast_node` as an [`AstNode`], when there is one.
fn node<N: Clone + Into<AstNode>>(ast_node: Option<&N>) -> Option<AstNode> {
    ast_node.cloned().map(Into::into)
}

/// Returns `astNode?.type` of an argument or an input field.
fn input_value_type_node(ast_node: Option<&InputValueDefinitionNode>) -> Option<AstNode> {
    ast_node.map(|node| AstNode::from(node.r#type.clone()))
}

/// Returns `astNode?.type` of a field.
fn field_type_node(ast_node: Option<&FieldDefinitionNode>) -> Option<AstNode> {
    ast_node.map(|node| AstNode::from(node.r#type.clone()))
}

/// Provided two types, return true if the types are equal (invariant).
///
/// The identity test of the reference is `===`, which holds for one named type
/// reached twice and for one wrapper object reached twice. The test here reads
/// the named types alone, so a wrapper the reference would find identical is
/// found equal one level down, where its inner types are identical, whether the
/// two wrappers share one allocation or not.
///
/// Derived from graphql-js 16.12.0, src/utilities/typeComparators.ts, MIT.
#[must_use]
pub fn is_equal_type(type_a: &Type, type_b: &Type) -> bool {
    // Equivalent types are equal.
    if is_same_named_type(type_a, type_b) {
        return true;
    }

    // If either type is non-null, the other must also be non-null.
    if let (Some(a), Some(b)) = (type_a.as_non_null(), type_b.as_non_null()) {
        return is_equal_type(a.of_type(), b.of_type());
    }

    // If either type is a list, the other must also be a list.
    if let (Some(a), Some(b)) = (type_a.as_list(), type_b.as_list()) {
        return is_equal_type(&a.of_type, &b.of_type);
    }

    // Otherwise the types are not equal.
    false
}

/// Provided a type and a super type, return true if the first type is either
/// equal or a subset of the second super type (covariant).
///
/// The identity test is the one [`is_equal_type()`] states.
///
/// Derived from graphql-js 16.12.0, src/utilities/typeComparators.ts, MIT.
///
/// # Errors
///
/// Returns what the member thunk of a union throws.
pub fn is_type_sub_type_of(
    schema: &Schema,
    maybe_sub_type: &Type,
    super_type: &Type,
) -> Result<bool, DefinitionError> {
    // Equivalent type is a valid subtype
    if is_same_named_type(maybe_sub_type, super_type) {
        return Ok(true);
    }

    // If superType is non-null, maybeSubType must also be non-null.
    if let Some(super_non_null) = super_type.as_non_null() {
        if let Some(sub_non_null) = maybe_sub_type.as_non_null() {
            return is_type_sub_type_of(schema, sub_non_null.of_type(), super_non_null.of_type());
        }
        return Ok(false);
    }
    if let Some(sub_non_null) = maybe_sub_type.as_non_null() {
        // If superType is nullable, maybeSubType may be non-null or nullable.
        return is_type_sub_type_of(schema, sub_non_null.of_type(), super_type);
    }

    // If superType type is a list, maybeSubType type must also be a list.
    if let Some(super_list) = super_type.as_list() {
        if let Some(sub_list) = maybe_sub_type.as_list() {
            return is_type_sub_type_of(schema, &sub_list.of_type, &super_list.of_type);
        }
        return Ok(false);
    }
    if is_list_type(maybe_sub_type) {
        // If superType is not a list, maybeSubType must also be not a list.
        return Ok(false);
    }

    // If superType type is an abstract type, check if it is super type of maybeSubType.
    // Otherwise, the child type is not a valid subtype of the parent type.
    Ok(is_abstract_type(super_type)
        && (is_interface_type(maybe_sub_type) || is_object_type(maybe_sub_type))
        && schema.is_sub_type(&get_named_type(super_type), &get_named_type(maybe_sub_type))?)
}

/// Provided two composite types, determine if they "overlap".
///
/// Two composite types overlap when the Sets of possible concrete types for
/// each intersect. This is often used to determine if a fragment of a given
/// type could possibly be visited in a context of another type. This function
/// is commutative. `type_a` and `type_b` are composite types, which are named
/// types, and the identity test of the reference is the identity a
/// [`NamedType`] compares by.
///
/// Derived from graphql-js 16.12.0, src/utilities/typeComparators.ts, MIT.
///
/// # Errors
///
/// Returns what the member thunk of a union throws.
pub fn do_types_overlap(
    schema: &Schema,
    type_a: &NamedType,
    type_b: &NamedType,
) -> Result<bool, DefinitionError> {
    // Equivalent types overlap
    if type_a == type_b {
        return Ok(true);
    }

    if is_abstract_named_type(type_a) {
        if is_abstract_named_type(type_b) {
            // If both types are abstract, then determine if there is any intersection
            // between possible concrete types of each.
            for ty in schema.get_possible_types(type_a)? {
                if schema.is_sub_type(type_b, ty)? {
                    return Ok(true);
                }
            }
            return Ok(false);
        }
        // Determine if the latter type is a possible concrete type of the former.
        return schema.is_sub_type(type_a, type_b);
    }

    if is_abstract_named_type(type_b) {
        // Determine if the former type is a possible concrete type of the latter.
        return schema.is_sub_type(type_b, type_a);
    }

    // Otherwise the types do not overlap.
    Ok(false)
}

/// Returns whether the named type is an interface or a union, the
/// `isAbstractType` of the reference over a type that is known to be named.
const fn is_abstract_named_type(ty: &NamedType) -> bool {
    matches!(ty, NamedType::Interface(_) | NamedType::Union(_))
}

/// Returns whether both types are one named type, the `===` of the
/// comparators over named types.
fn is_same_named_type(type_a: &Type, type_b: &Type) -> bool {
    match (type_a.as_named(), type_b.as_named()) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}
