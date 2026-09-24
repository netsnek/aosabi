// SPDX-License-Identifier: MIT
/*
 * aosabi-lang: the printer that turns a tree back into GraphQL text.
 *
 * Derived from graphql-js 16.12.0, src/language/printer.ts, MIT.
 */

//! The printer of the language layer.
//!
//! [`print`] converts a node into a string, using one set of reasonable
//! formatting rules. The reference folds the tree through `visit` with a
//! reducer whose `leave` functions turn a node whose children are already
//! strings into a string. None of those functions has an `enter`, and none
//! reads the key, the parent, the path or the ancestors, so the fold is a
//! function of the children strings alone. The port folds by direct recursion
//! over the same children in the same order, and each `leave` keeps its name
//! as a function of this module with the `leave` prefix.
//!
//! An absent optional child and an empty string print the same, because
//! `join` drops both and `wrap` prints nothing for either. The port therefore
//! reduces an absent child to the empty string and an absent list to the
//! empty list.
//!
//! One place counts. The width check of a field's arguments compares `length`
//! with the line limit, and `length` counts UTF-16 code units, so the port
//! counts code units there. The line feed that `hasMultilineItems` looks for
//! and that `indent` replaces is one code unit and one byte alike.

use alloc::{
    format,
    string::String,
    vec::Vec, //
};

use crate::{
    ast::{
        ArgumentCoordinateNode,
        ArgumentNode,
        BooleanValueNode,
        DirectiveArgumentCoordinateNode,
        DirectiveCoordinateNode,
        DirectiveDefinitionNode,
        DirectiveNode,
        DocumentNode,
        EnumTypeDefinitionNode,
        EnumTypeExtensionNode,
        EnumValueDefinitionNode,
        EnumValueNode,
        FieldDefinitionNode,
        FieldNode,
        FloatValueNode,
        FragmentDefinitionNode,
        FragmentSpreadNode,
        InlineFragmentNode,
        InputObjectTypeDefinitionNode,
        InputObjectTypeExtensionNode,
        InputValueDefinitionNode,
        IntValueNode,
        InterfaceTypeDefinitionNode,
        InterfaceTypeExtensionNode,
        ListTypeNode,
        ListValueNode,
        MemberCoordinateNode,
        NameNode,
        NamedTypeNode,
        NonNullTypeNode,
        ObjectFieldNode,
        ObjectTypeDefinitionNode,
        ObjectTypeExtensionNode,
        ObjectValueNode,
        OperationDefinitionNode,
        OperationTypeDefinitionNode,
        ScalarTypeDefinitionNode,
        ScalarTypeExtensionNode,
        SchemaDefinitionNode,
        SchemaExtensionNode,
        SelectionSetNode,
        StringValueNode,
        TypeCoordinateNode,
        UnionTypeDefinitionNode,
        UnionTypeExtensionNode,
        VariableDefinitionNode,
        VariableNode, //
    },
    block_string::print_block_string,
    print_string::print_string,
    visitor::NodeRef, //
};

/// Converts an AST into a string, using one set of reasonable formatting
/// rules.
///
/// `ast` is any node or union of the tree.
///
/// Derived from graphql-js 16.12.0, src/language/printer.ts, MIT.
#[must_use]
pub fn print<'a>(ast: impl Into<NodeRef<'a>>) -> String {
    print_doc_ast_reducer(ast.into())
}

/// The width above which the arguments of a field go one per line.
const MAX_LINE_LENGTH: usize = 80;

/// Reduces a node to its text, the dispatch of the reducer.
///
/// Every arm reduces the children first and then calls the `leave` of the
/// kind.
///
/// Derived from graphql-js 16.12.0, src/language/printer.ts, MIT.
fn print_doc_ast_reducer(node: NodeRef<'_>) -> String {
    match node {
        NodeRef::Name(node) => leave_name(node),
        NodeRef::Variable(node) => leave_variable(node),
        NodeRef::Document(node) => leave_document(node),
        NodeRef::OperationDefinition(node) => leave_operation_definition(node),
        NodeRef::VariableDefinition(node) => leave_variable_definition(node),
        NodeRef::SelectionSet(node) => leave_selection_set(node),
        NodeRef::Field(node) => leave_field(node),
        NodeRef::Argument(node) => leave_argument(node),
        NodeRef::FragmentSpread(node) => leave_fragment_spread(node),
        NodeRef::InlineFragment(node) => leave_inline_fragment(node),
        NodeRef::FragmentDefinition(node) => leave_fragment_definition(node),
        NodeRef::IntValue(node) => leave_int_value(node),
        NodeRef::FloatValue(node) => leave_float_value(node),
        NodeRef::StringValue(node) => leave_string_value(node),
        NodeRef::BooleanValue(node) => leave_boolean_value(node),
        NodeRef::NullValue(_) => leave_null_value(),
        NodeRef::EnumValue(node) => leave_enum_value(node),
        NodeRef::ListValue(node) => leave_list_value(node),
        NodeRef::ObjectValue(node) => leave_object_value(node),
        NodeRef::ObjectField(node) => leave_object_field(node),
        NodeRef::Directive(node) => leave_directive(node),
        NodeRef::NamedType(node) => leave_named_type(node),
        NodeRef::ListType(node) => leave_list_type(node),
        NodeRef::NonNullType(node) => leave_non_null_type(node),
        NodeRef::SchemaDefinition(node) => leave_schema_definition(node),
        NodeRef::OperationTypeDefinition(node) => leave_operation_type_definition(node),
        NodeRef::ScalarTypeDefinition(node) => leave_scalar_type_definition(node),
        NodeRef::ObjectTypeDefinition(node) => leave_object_type_definition(node),
        NodeRef::FieldDefinition(node) => leave_field_definition(node),
        NodeRef::InputValueDefinition(node) => leave_input_value_definition(node),
        NodeRef::InterfaceTypeDefinition(node) => leave_interface_type_definition(node),
        NodeRef::UnionTypeDefinition(node) => leave_union_type_definition(node),
        NodeRef::EnumTypeDefinition(node) => leave_enum_type_definition(node),
        NodeRef::EnumValueDefinition(node) => leave_enum_value_definition(node),
        NodeRef::InputObjectTypeDefinition(node) => leave_input_object_type_definition(node),
        NodeRef::DirectiveDefinition(node) => leave_directive_definition(node),
        NodeRef::SchemaExtension(node) => leave_schema_extension(node),
        NodeRef::ScalarTypeExtension(node) => leave_scalar_type_extension(node),
        NodeRef::ObjectTypeExtension(node) => leave_object_type_extension(node),
        NodeRef::InterfaceTypeExtension(node) => leave_interface_type_extension(node),
        NodeRef::UnionTypeExtension(node) => leave_union_type_extension(node),
        NodeRef::EnumTypeExtension(node) => leave_enum_type_extension(node),
        NodeRef::InputObjectTypeExtension(node) => leave_input_object_type_extension(node),
        NodeRef::TypeCoordinate(node) => leave_type_coordinate(node),
        NodeRef::MemberCoordinate(node) => leave_member_coordinate(node),
        NodeRef::ArgumentCoordinate(node) => leave_argument_coordinate(node),
        NodeRef::DirectiveCoordinate(node) => leave_directive_coordinate(node),
        NodeRef::DirectiveArgumentCoordinate(node) => leave_directive_argument_coordinate(node),
    }
}

/// Reduces one child to its text.
fn reduce<'a, T>(child: &'a T) -> String
where
    NodeRef<'a>: From<&'a T>,
{
    print_doc_ast_reducer(NodeRef::from(child))
}

/// Reduces an optional child to its text, the empty string when absent.
fn reduce_optional<'a, T>(child: Option<&'a T>) -> String
where
    NodeRef<'a>: From<&'a T>,
{
    child.map_or_else(String::new, reduce)
}

/// Reduces a list of children to their texts.
fn reduce_list<'a, T>(items: &'a [T]) -> Vec<String>
where
    NodeRef<'a>: From<&'a T>,
{
    items.iter().map(reduce).collect()
}

/// Reduces an optional list of children to their texts, the empty list when
/// absent.
fn reduce_optional_list<'a, T>(items: Option<&'a [T]>) -> Vec<String>
where
    NodeRef<'a>: From<&'a T>,
{
    items.map_or_else(Vec::new, reduce_list)
}

/// The `Name` leave.
fn leave_name(node: &NameNode) -> String {
    String::from(node.value())
}

/// The `Variable` leave.
fn leave_variable(node: &VariableNode) -> String {
    let name = reduce(&node.name);
    format!("${name}")
}

/// The `Document` leave.
fn leave_document(node: &DocumentNode) -> String {
    join(&reduce_list(&node.definitions), "\n\n")
}

/// The `OperationDefinition` leave.
fn leave_operation_definition(node: &OperationDefinitionNode) -> String {
    let variable_definitions = reduce_optional_list(node.variable_definitions.as_deref());
    let var_defs = if has_multiline_items(&variable_definitions) {
        wrap("(\n", &join(&variable_definitions, "\n"), "\n)")
    } else {
        wrap("(", &join(&variable_definitions, ", "), ")")
    };
    let prefix = wrap("", &reduce_optional(node.description.as_ref()), "\n")
        + &join(
            &[
                String::from(node.operation.as_str()),
                join(&[reduce_optional(node.name.as_ref()), var_defs], ""),
                join(&reduce_optional_list(node.directives.as_deref()), " "),
            ],
            " ",
        );

    // Anonymous queries with no directives or variable definitions can use
    // the query short form.
    let head = if prefix == "query" {
        String::new()
    } else {
        prefix + " "
    };
    head + &reduce(&node.selection_set)
}

/// The `VariableDefinition` leave.
fn leave_variable_definition(node: &VariableDefinitionNode) -> String {
    let directives = reduce_optional_list(node.directives.as_deref());
    wrap("", &reduce_optional(node.description.as_ref()), "\n")
        + &reduce(&node.variable)
        + ": "
        + &reduce(&node.r#type)
        + &wrap(" = ", &reduce_optional(node.default_value.as_ref()), "")
        + &wrap(" ", &join(&directives, " "), "")
}

/// The `SelectionSet` leave.
fn leave_selection_set(node: &SelectionSetNode) -> String {
    block(&reduce_list(&node.selections))
}

/// The `Field` leave.
fn leave_field(node: &FieldNode) -> String {
    let args = reduce_optional_list(node.arguments.as_deref());
    let prefix = wrap("", &reduce_optional(node.alias.as_ref()), ": ") + &reduce(&node.name);
    let mut args_line = prefix.clone() + &wrap("(", &join(&args, ", "), ")");

    if args_line.encode_utf16().count() > MAX_LINE_LENGTH {
        args_line = prefix + &wrap("(\n", &indent(&join(&args, "\n")), "\n)");
    }

    join(
        &[
            args_line,
            join(&reduce_optional_list(node.directives.as_deref()), " "),
            reduce_optional(node.selection_set.as_ref()),
        ],
        " ",
    )
}

/// The `Argument` leave.
fn leave_argument(node: &ArgumentNode) -> String {
    reduce(&node.name) + ": " + &reduce(&node.value)
}

/// The `FragmentSpread` leave.
fn leave_fragment_spread(node: &FragmentSpreadNode) -> String {
    let directives = reduce_optional_list(node.directives.as_deref());
    String::from("...") + &reduce(&node.name) + &wrap(" ", &join(&directives, " "), "")
}

/// The `InlineFragment` leave.
fn leave_inline_fragment(node: &InlineFragmentNode) -> String {
    join(
        &[
            String::from("..."),
            wrap("on ", &reduce_optional(node.type_condition.as_ref()), ""),
            join(&reduce_optional_list(node.directives.as_deref()), " "),
            reduce(&node.selection_set),
        ],
        " ",
    )
}

/// The `FragmentDefinition` leave.
fn leave_fragment_definition(node: &FragmentDefinitionNode) -> String {
    let name = reduce(&node.name);
    let type_condition = reduce(&node.type_condition);
    let variable_definitions = reduce_optional_list(node.variable_definitions.as_deref());
    let directives = reduce_optional_list(node.directives.as_deref());
    let selection_set = reduce(&node.selection_set);
    let var_defs = wrap("(", &join(&variable_definitions, ", "), ")");
    let directives = wrap("", &join(&directives, " "), " ");
    wrap("", &reduce_optional(node.description.as_ref()), "\n")
        // Note: fragment variable definitions are experimental and may be
        // changed or removed in the future.
        + &format!("fragment {name}{var_defs} ")
        + &format!("on {type_condition} {directives}")
        + &selection_set
}

/// The `IntValue` leave.
fn leave_int_value(node: &IntValueNode) -> String {
    node.value.clone()
}

/// The `FloatValue` leave.
fn leave_float_value(node: &FloatValueNode) -> String {
    node.value.clone()
}

/// The `StringValue` leave.
///
/// The `block` flag is read for truth the way `?:` reads it, so only `true`
/// selects the block form.
fn leave_string_value(node: &StringValueNode) -> String {
    let is_block_string = node.block == Some(true);
    if is_block_string {
        print_block_string(&node.value, false)
    } else {
        print_string(&node.value)
    }
}

/// The `BooleanValue` leave.
fn leave_boolean_value(node: &BooleanValueNode) -> String {
    String::from(if node.value { "true" } else { "false" })
}

/// The `NullValue` leave.
fn leave_null_value() -> String {
    String::from("null")
}

/// The `EnumValue` leave.
fn leave_enum_value(node: &EnumValueNode) -> String {
    node.value.clone()
}

/// The `ListValue` leave.
fn leave_list_value(node: &ListValueNode) -> String {
    let values = join(&reduce_list(&node.values), ", ");
    format!("[{values}]")
}

/// The `ObjectValue` leave.
fn leave_object_value(node: &ObjectValueNode) -> String {
    let fields = join(&reduce_list(&node.fields), ", ");
    format!("{{{fields}}}")
}

/// The `ObjectField` leave.
fn leave_object_field(node: &ObjectFieldNode) -> String {
    reduce(&node.name) + ": " + &reduce(&node.value)
}

/// The `Directive` leave.
fn leave_directive(node: &DirectiveNode) -> String {
    let args = reduce_optional_list(node.arguments.as_deref());
    String::from("@") + &reduce(&node.name) + &wrap("(", &join(&args, ", "), ")")
}

/// The `NamedType` leave.
fn leave_named_type(node: &NamedTypeNode) -> String {
    reduce(&node.name)
}

/// The `ListType` leave.
fn leave_list_type(node: &ListTypeNode) -> String {
    let r#type = reduce(&*node.r#type);
    format!("[{type}]")
}

/// The `NonNullType` leave.
fn leave_non_null_type(node: &NonNullTypeNode) -> String {
    reduce(&node.r#type) + "!"
}

/// The `SchemaDefinition` leave.
fn leave_schema_definition(node: &SchemaDefinitionNode) -> String {
    let directives = reduce_optional_list(node.directives.as_deref());
    let operation_types = reduce_list(&node.operation_types);
    wrap("", &reduce_optional(node.description.as_ref()), "\n")
        + &join(
            &[
                String::from("schema"),
                join(&directives, " "),
                block(&operation_types),
            ],
            " ",
        )
}

/// The `OperationTypeDefinition` leave.
fn leave_operation_type_definition(node: &OperationTypeDefinitionNode) -> String {
    String::from(node.operation.as_str()) + ": " + &reduce(&node.r#type)
}

/// The `ScalarTypeDefinition` leave.
fn leave_scalar_type_definition(node: &ScalarTypeDefinitionNode) -> String {
    let directives = reduce_optional_list(node.directives.as_deref());
    wrap("", &reduce_optional(node.description.as_ref()), "\n")
        + &join(
            &[
                String::from("scalar"),
                reduce(&node.name),
                join(&directives, " "),
            ],
            " ",
        )
}

/// The `ObjectTypeDefinition` leave.
fn leave_object_type_definition(node: &ObjectTypeDefinitionNode) -> String {
    let interfaces = reduce_optional_list(node.interfaces.as_deref());
    let directives = reduce_optional_list(node.directives.as_deref());
    let fields = reduce_optional_list(node.fields.as_deref());
    wrap("", &reduce_optional(node.description.as_ref()), "\n")
        + &join(
            &[
                String::from("type"),
                reduce(&node.name),
                wrap("implements ", &join(&interfaces, " & "), ""),
                join(&directives, " "),
                block(&fields),
            ],
            " ",
        )
}

/// The `FieldDefinition` leave.
fn leave_field_definition(node: &FieldDefinitionNode) -> String {
    let args = reduce_optional_list(node.arguments.as_deref());
    let directives = reduce_optional_list(node.directives.as_deref());
    wrap("", &reduce_optional(node.description.as_ref()), "\n")
        + &reduce(&node.name)
        + &(if has_multiline_items(&args) {
            wrap("(\n", &indent(&join(&args, "\n")), "\n)")
        } else {
            wrap("(", &join(&args, ", "), ")")
        })
        + ": "
        + &reduce(&node.r#type)
        + &wrap(" ", &join(&directives, " "), "")
}

/// The `InputValueDefinition` leave.
fn leave_input_value_definition(node: &InputValueDefinitionNode) -> String {
    let directives = reduce_optional_list(node.directives.as_deref());
    wrap("", &reduce_optional(node.description.as_ref()), "\n")
        + &join(
            &[
                reduce(&node.name) + ": " + &reduce(&node.r#type),
                wrap("= ", &reduce_optional(node.default_value.as_ref()), ""),
                join(&directives, " "),
            ],
            " ",
        )
}

/// The `InterfaceTypeDefinition` leave.
fn leave_interface_type_definition(node: &InterfaceTypeDefinitionNode) -> String {
    let interfaces = reduce_optional_list(node.interfaces.as_deref());
    let directives = reduce_optional_list(node.directives.as_deref());
    let fields = reduce_optional_list(node.fields.as_deref());
    wrap("", &reduce_optional(node.description.as_ref()), "\n")
        + &join(
            &[
                String::from("interface"),
                reduce(&node.name),
                wrap("implements ", &join(&interfaces, " & "), ""),
                join(&directives, " "),
                block(&fields),
            ],
            " ",
        )
}

/// The `UnionTypeDefinition` leave.
fn leave_union_type_definition(node: &UnionTypeDefinitionNode) -> String {
    let directives = reduce_optional_list(node.directives.as_deref());
    let types = reduce_optional_list(node.types.as_deref());
    wrap("", &reduce_optional(node.description.as_ref()), "\n")
        + &join(
            &[
                String::from("union"),
                reduce(&node.name),
                join(&directives, " "),
                wrap("= ", &join(&types, " | "), ""),
            ],
            " ",
        )
}

/// The `EnumTypeDefinition` leave.
fn leave_enum_type_definition(node: &EnumTypeDefinitionNode) -> String {
    let directives = reduce_optional_list(node.directives.as_deref());
    let values = reduce_optional_list(node.values.as_deref());
    wrap("", &reduce_optional(node.description.as_ref()), "\n")
        + &join(
            &[
                String::from("enum"),
                reduce(&node.name),
                join(&directives, " "),
                block(&values),
            ],
            " ",
        )
}

/// The `EnumValueDefinition` leave.
fn leave_enum_value_definition(node: &EnumValueDefinitionNode) -> String {
    let directives = reduce_optional_list(node.directives.as_deref());
    wrap("", &reduce_optional(node.description.as_ref()), "\n")
        + &join(&[reduce(&node.name), join(&directives, " ")], " ")
}

/// The `InputObjectTypeDefinition` leave.
fn leave_input_object_type_definition(node: &InputObjectTypeDefinitionNode) -> String {
    let directives = reduce_optional_list(node.directives.as_deref());
    let fields = reduce_optional_list(node.fields.as_deref());
    wrap("", &reduce_optional(node.description.as_ref()), "\n")
        + &join(
            &[
                String::from("input"),
                reduce(&node.name),
                join(&directives, " "),
                block(&fields),
            ],
            " ",
        )
}

/// The `DirectiveDefinition` leave.
fn leave_directive_definition(node: &DirectiveDefinitionNode) -> String {
    let args = reduce_optional_list(node.arguments.as_deref());
    let locations = reduce_list(&node.locations);
    wrap("", &reduce_optional(node.description.as_ref()), "\n")
        + "directive @"
        + &reduce(&node.name)
        + &(if has_multiline_items(&args) {
            wrap("(\n", &indent(&join(&args, "\n")), "\n)")
        } else {
            wrap("(", &join(&args, ", "), ")")
        })
        + (if node.repeatable { " repeatable" } else { "" })
        + " on "
        + &join(&locations, " | ")
}

/// The `SchemaExtension` leave.
fn leave_schema_extension(node: &SchemaExtensionNode) -> String {
    let directives = reduce_optional_list(node.directives.as_deref());
    let operation_types = reduce_optional_list(node.operation_types.as_deref());
    join(
        &[
            String::from("extend schema"),
            join(&directives, " "),
            block(&operation_types),
        ],
        " ",
    )
}

/// The `ScalarTypeExtension` leave.
fn leave_scalar_type_extension(node: &ScalarTypeExtensionNode) -> String {
    let directives = reduce_optional_list(node.directives.as_deref());
    join(
        &[
            String::from("extend scalar"),
            reduce(&node.name),
            join(&directives, " "),
        ],
        " ",
    )
}

/// The `ObjectTypeExtension` leave.
fn leave_object_type_extension(node: &ObjectTypeExtensionNode) -> String {
    let interfaces = reduce_optional_list(node.interfaces.as_deref());
    let directives = reduce_optional_list(node.directives.as_deref());
    let fields = reduce_optional_list(node.fields.as_deref());
    join(
        &[
            String::from("extend type"),
            reduce(&node.name),
            wrap("implements ", &join(&interfaces, " & "), ""),
            join(&directives, " "),
            block(&fields),
        ],
        " ",
    )
}

/// The `InterfaceTypeExtension` leave.
fn leave_interface_type_extension(node: &InterfaceTypeExtensionNode) -> String {
    let interfaces = reduce_optional_list(node.interfaces.as_deref());
    let directives = reduce_optional_list(node.directives.as_deref());
    let fields = reduce_optional_list(node.fields.as_deref());
    join(
        &[
            String::from("extend interface"),
            reduce(&node.name),
            wrap("implements ", &join(&interfaces, " & "), ""),
            join(&directives, " "),
            block(&fields),
        ],
        " ",
    )
}

/// The `UnionTypeExtension` leave.
fn leave_union_type_extension(node: &UnionTypeExtensionNode) -> String {
    let directives = reduce_optional_list(node.directives.as_deref());
    let types = reduce_optional_list(node.types.as_deref());
    join(
        &[
            String::from("extend union"),
            reduce(&node.name),
            join(&directives, " "),
            wrap("= ", &join(&types, " | "), ""),
        ],
        " ",
    )
}

/// The `EnumTypeExtension` leave.
fn leave_enum_type_extension(node: &EnumTypeExtensionNode) -> String {
    let directives = reduce_optional_list(node.directives.as_deref());
    let values = reduce_optional_list(node.values.as_deref());
    join(
        &[
            String::from("extend enum"),
            reduce(&node.name),
            join(&directives, " "),
            block(&values),
        ],
        " ",
    )
}

/// The `InputObjectTypeExtension` leave.
fn leave_input_object_type_extension(node: &InputObjectTypeExtensionNode) -> String {
    let directives = reduce_optional_list(node.directives.as_deref());
    let fields = reduce_optional_list(node.fields.as_deref());
    join(
        &[
            String::from("extend input"),
            reduce(&node.name),
            join(&directives, " "),
            block(&fields),
        ],
        " ",
    )
}

/// The `TypeCoordinate` leave.
fn leave_type_coordinate(node: &TypeCoordinateNode) -> String {
    reduce(&node.name)
}

/// The `MemberCoordinate` leave.
fn leave_member_coordinate(node: &MemberCoordinateNode) -> String {
    join(
        &[
            reduce(&node.name),
            wrap(".", &reduce(&node.member_name), ""),
        ],
        "",
    )
}

/// The `ArgumentCoordinate` leave.
fn leave_argument_coordinate(node: &ArgumentCoordinateNode) -> String {
    join(
        &[
            reduce(&node.name),
            wrap(".", &reduce(&node.field_name), ""),
            wrap("(", &reduce(&node.argument_name), ":)"),
        ],
        "",
    )
}

/// The `DirectiveCoordinate` leave.
fn leave_directive_coordinate(node: &DirectiveCoordinateNode) -> String {
    join(&[String::from("@"), reduce(&node.name)], "")
}

/// The `DirectiveArgumentCoordinate` leave.
fn leave_directive_argument_coordinate(node: &DirectiveArgumentCoordinateNode) -> String {
    join(
        &[
            String::from("@"),
            reduce(&node.name),
            wrap("(", &reduce(&node.argument_name), ":)"),
        ],
        "",
    )
}

/// Given `maybe_array`, prints an empty string if it is null or empty,
/// otherwise prints all items together separated by `separator`.
///
/// The reference keeps the items `filter((x) => x)` passes, and a string
/// passes when it is not empty, so an absent item and an empty item drop
/// alike.
///
/// Derived from graphql-js 16.12.0, src/language/printer.ts, MIT.
fn join(maybe_array: &[String], separator: &str) -> String {
    maybe_array
        .iter()
        .filter(|x| !x.is_empty())
        .map(String::as_str)
        .collect::<Vec<&str>>()
        .join(separator)
}

/// Given `array`, prints each item on its own line, wrapped in an indented
/// `{ }` block.
///
/// Derived from graphql-js 16.12.0, src/language/printer.ts, MIT.
fn block(array: &[String]) -> String {
    wrap("{\n", &indent(&join(array, "\n")), "\n}")
}

/// If `maybe_string` is not null or empty, then wraps it with `start` and
/// `end`, otherwise prints an empty string.
///
/// Derived from graphql-js 16.12.0, src/language/printer.ts, MIT.
fn wrap(start: &str, maybe_string: &str, end: &str) -> String {
    if maybe_string.is_empty() {
        String::new()
    } else {
        format!("{start}{maybe_string}{end}")
    }
}

/// Indents every line of `text` by two spaces.
///
/// Derived from graphql-js 16.12.0, src/language/printer.ts, MIT.
fn indent(text: &str) -> String {
    wrap("  ", &text.replace('\n', "\n  "), "")
}

/// Returns whether any item of `maybe_array` spans more than one line.
///
/// Derived from graphql-js 16.12.0, src/language/printer.ts, MIT.
fn has_multiline_items(maybe_array: &[String]) -> bool {
    maybe_array.iter().any(|text| text.contains('\n'))
}
