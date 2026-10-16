// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool ast_from_json: reads an abstract syntax tree back from the
 * JSON form the parse primitive writes.
 *
 * No analogue in the reference, where a tree that went through JSON is a plain
 * object the printer walks as it is.
 */

//! The reader of the `ast` field of a `print` request.
//!
//! The oracle prints a tree a caller hands it, as an object or as the string a
//! `parse` response carries. The tree of aosabi-lang is typed, so [`node_of`]
//! rebuilds the typed node from the value and refuses a value that is not a
//! tree.
//!
//! Three readings follow the reference printer rather than the parser, because
//! a caller may hand over a tree no parser built.
//!
//! - A list the printer joins may be absent, because `join` reads an absent
//!   list as empty.
//! - A key that holds `null` reads as absent, because the printer tests a
//!   child with `wrap` and `join`, which treat `null` and `undefined` alike.
//! - `loc` is dropped, because the printer never reads it and a location of
//!   aosabi-lang points into a token arena this tree does not have.

use aosabi_json::{
    JsObject,
    JsValue, //
};
use aosabi_lang::{
    SharedSlice,
    ast::{
        ArgumentCoordinateNode,
        ArgumentNode,
        AstNode,
        BooleanValueNode,
        DefinitionNode,
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
        NullValueNode,
        NullableTypeNode,
        ObjectFieldNode,
        ObjectTypeDefinitionNode,
        ObjectTypeExtensionNode,
        ObjectValueNode,
        OperationDefinitionNode,
        OperationTypeDefinitionNode,
        OperationTypeNode,
        ScalarTypeDefinitionNode,
        ScalarTypeExtensionNode,
        SchemaDefinitionNode,
        SchemaExtensionNode,
        SelectionNode,
        SelectionSetNode,
        StringValueNode,
        TypeCoordinateNode,
        TypeNode,
        UnionTypeDefinitionNode,
        UnionTypeExtensionNode,
        ValueNode,
        VariableDefinitionNode,
        VariableNode, //
    }, //
};

use crate::json::reader;

/// A reader of one kind of node, or of one union of kinds.
type Decode<T> = fn(&JsValue) -> Result<T, String>;

/// Returns the node `value` holds, which is a tree or the JSON text of one.
///
/// # Errors
///
/// Returns the reason when the text is not JSON or the value is not a tree.
pub fn node_of(value: &JsValue) -> Result<AstNode, String> {
    let JsValue::String(text) = value else {
        return ast_node(value);
    };
    let text = text
        .as_str()
        .ok_or("the ast string carries a lone surrogate")?;
    let parsed =
        reader::parse(text.as_bytes()).map_err(|error| format!("the ast is not JSON: {error}"))?;
    ast_node(&parsed)
}

/// The fields of one node object, with the kind it was checked against.
struct Fields<'a> {
    /// The kind the object carries, which names the node in a refusal.
    kind: &'a str,
    /// The object itself.
    object: &'a JsObject,
}

impl<'a> Fields<'a> {
    /// Returns the fields of `value` when it is a node of the kind `kind`.
    fn of(value: &'a JsValue, kind: &'a str) -> Result<Self, String> {
        let found = kind_of(value)?;
        if found != kind {
            return Err(format!(
                "a {kind} node is expected where a {found} node stands"
            ));
        }
        let JsValue::Object(object) = value else {
            return Err(String::from("a node is not an object"));
        };
        Ok(Self { kind, object })
    }

    /// Returns the value under `key`, where `null` reads as absent.
    fn get(&self, key: &str) -> Option<&'a JsValue> {
        match self.object.get(key) {
            None | Some(JsValue::Undefined | JsValue::Null) => None,
            Some(value) => Some(value),
        }
    }

    /// Returns the refusal of a field this node cannot do without.
    fn missing(&self, key: &str) -> String {
        format!("the {} node carries no usable {key}", self.kind)
    }

    /// Returns the child under `key`, which the node cannot do without.
    fn required<T>(&self, key: &str, decode: Decode<T>) -> Result<T, String> {
        let value = self.get(key).ok_or_else(|| self.missing(key))?;
        decode(value)
    }

    /// Returns the child under `key` when the node carries one.
    fn optional<T>(&self, key: &str, decode: Decode<T>) -> Result<Option<T>, String> {
        self.get(key).map(decode).transpose()
    }

    /// Returns the children under `key` when the node carries the list.
    fn optional_list<T>(&self, key: &str, decode: Decode<T>) -> Result<Option<Vec<T>>, String> {
        let Some(value) = self.get(key) else {
            return Ok(None);
        };
        let JsValue::Array(items) = value else {
            return Err(self.missing(key));
        };
        items.iter().map(decode).collect::<Result<_, _>>().map(Some)
    }

    /// Returns the children under `key`, where an absent list reads as empty.
    fn list<T>(&self, key: &str, decode: Decode<T>) -> Result<Vec<T>, String> {
        Ok(self.optional_list(key, decode)?.unwrap_or_default())
    }

    /// Returns the string under `key`.
    fn string(&self, key: &str) -> Result<String, String> {
        match self.get(key) {
            Some(JsValue::String(text)) => text
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| self.missing(key)),
            _ => Err(self.missing(key)),
        }
    }

    /// Returns the boolean under `key` when the node carries one.
    fn optional_boolean(&self, key: &str) -> Result<Option<bool>, String> {
        match self.get(key) {
            None => Ok(None),
            Some(JsValue::Bool(flag)) => Ok(Some(*flag)),
            Some(_) => Err(self.missing(key)),
        }
    }

    /// Returns the operation under `key`.
    fn operation(&self, key: &str) -> Result<OperationTypeNode, String> {
        match self.string(key)?.as_str() {
            "query" => Ok(OperationTypeNode::Query),
            "mutation" => Ok(OperationTypeNode::Mutation),
            "subscription" => Ok(OperationTypeNode::Subscription),
            _ => Err(self.missing(key)),
        }
    }
}

/// Returns the `kind` string of a node value.
fn kind_of(value: &JsValue) -> Result<&str, String> {
    let JsValue::Object(object) = value else {
        return Err(String::from("a node is not an object"));
    };
    match object.get("kind") {
        Some(JsValue::String(kind)) => kind
            .as_str()
            .ok_or_else(|| String::from("a node carries no usable kind")),
        _ => Err(String::from("a node carries no usable kind")),
    }
}

/// Reads any node.
///
/// The unions are asked first, and the kinds that belong to no union of
/// [`aosabi_lang::ast`] are read by their own reader.
fn ast_node(value: &JsValue) -> Result<AstNode, String> {
    let kind = kind_of(value)?;
    if let Some(node) = union_node(kind, value) {
        return node;
    }
    if let Some(node) = coordinate_node(kind, value) {
        return node;
    }
    match kind {
        "Name" => name_node(value).map(Into::into),
        "Document" => document_node(value).map(Into::into),
        "VariableDefinition" => variable_definition_node(value).map(Into::into),
        "SelectionSet" => selection_set_node(value).map(Into::into),
        "Argument" => argument_node(value).map(Into::into),
        "ObjectField" => object_field_node(value).map(Into::into),
        "Directive" => directive_node(value).map(Into::into),
        "OperationTypeDefinition" => operation_type_definition_node(value).map(Into::into),
        "FieldDefinition" => field_definition_node(value).map(Into::into),
        "InputValueDefinition" => input_value_definition_node(value).map(Into::into),
        "EnumValueDefinition" => enum_value_definition_node(value).map(Into::into),
        _ => Err(format!("{kind} is not the kind of a node")),
    }
}

/// Reads a node of one of the four unions, or returns `None` for another kind.
fn union_node(kind: &str, value: &JsValue) -> Option<Result<AstNode, String>> {
    if let Some(node) = definition_of(kind, value) {
        return Some(node.map(Into::into));
    }
    if let Some(node) = selection_of(kind, value) {
        return Some(node.map(Into::into));
    }
    if let Some(node) = value_of(kind, value) {
        return Some(node.map(Into::into));
    }
    type_of(kind, value).map(|node| node.map(Into::into))
}

/// Reads a schema coordinate, or returns `None` for another kind.
fn coordinate_node(kind: &str, value: &JsValue) -> Option<Result<AstNode, String>> {
    let node = match kind {
        "TypeCoordinate" => type_coordinate_node(value).map(Into::into),
        "MemberCoordinate" => member_coordinate_node(value).map(Into::into),
        "ArgumentCoordinate" => argument_coordinate_node(value).map(Into::into),
        "DirectiveCoordinate" => directive_coordinate_node(value).map(Into::into),
        "DirectiveArgumentCoordinate" => directive_argument_coordinate_node(value).map(Into::into),
        _ => return None,
    };
    Some(node)
}

/// Reads a definition.
fn definition_node(value: &JsValue) -> Result<DefinitionNode, String> {
    let kind = kind_of(value)?;
    definition_of(kind, value).unwrap_or_else(|| Err(format!("{kind} is not a definition")))
}

/// Reads a definition of the kind `kind`, or returns `None` for another kind.
fn definition_of(kind: &str, value: &JsValue) -> Option<Result<DefinitionNode, String>> {
    let node = match kind {
        "OperationDefinition" => operation_definition_node(value).map(Into::into),
        "FragmentDefinition" => fragment_definition_node(value).map(Into::into),
        "SchemaDefinition" => schema_definition_node(value).map(Into::into),
        "ScalarTypeDefinition" => scalar_type_definition_node(value).map(Into::into),
        "ObjectTypeDefinition" => object_type_definition_node(value).map(Into::into),
        "InterfaceTypeDefinition" => interface_type_definition_node(value).map(Into::into),
        "UnionTypeDefinition" => union_type_definition_node(value).map(Into::into),
        "EnumTypeDefinition" => enum_type_definition_node(value).map(Into::into),
        "InputObjectTypeDefinition" => input_object_type_definition_node(value).map(Into::into),
        "DirectiveDefinition" => directive_definition_node(value).map(Into::into),
        _ => return extension_of(kind, value),
    };
    Some(node)
}

/// Reads a type system extension of the kind `kind`, or returns `None` for
/// another kind.
fn extension_of(kind: &str, value: &JsValue) -> Option<Result<DefinitionNode, String>> {
    let node = match kind {
        "SchemaExtension" => schema_extension_node(value).map(Into::into),
        "ScalarTypeExtension" => scalar_type_extension_node(value).map(Into::into),
        "ObjectTypeExtension" => object_type_extension_node(value).map(Into::into),
        "InterfaceTypeExtension" => interface_type_extension_node(value).map(Into::into),
        "UnionTypeExtension" => union_type_extension_node(value).map(Into::into),
        "EnumTypeExtension" => enum_type_extension_node(value).map(Into::into),
        "InputObjectTypeExtension" => input_object_type_extension_node(value).map(Into::into),
        _ => return None,
    };
    Some(node)
}

/// Reads a selection.
fn selection_node(value: &JsValue) -> Result<SelectionNode, String> {
    let kind = kind_of(value)?;
    selection_of(kind, value).unwrap_or_else(|| Err(format!("{kind} is not a selection")))
}

/// Reads a selection of the kind `kind`, or returns `None` for another kind.
fn selection_of(kind: &str, value: &JsValue) -> Option<Result<SelectionNode, String>> {
    let node = match kind {
        "Field" => field_node(value).map(Into::into),
        "FragmentSpread" => fragment_spread_node(value).map(Into::into),
        "InlineFragment" => inline_fragment_node(value).map(Into::into),
        _ => return None,
    };
    Some(node)
}

/// Reads a value literal.
fn value_node(value: &JsValue) -> Result<ValueNode, String> {
    let kind = kind_of(value)?;
    value_of(kind, value).unwrap_or_else(|| Err(format!("{kind} is not a value")))
}

/// Reads a value literal of the kind `kind`, or returns `None` for another
/// kind.
fn value_of(kind: &str, value: &JsValue) -> Option<Result<ValueNode, String>> {
    let node = match kind {
        "Variable" => variable_node(value).map(Into::into),
        "IntValue" => int_value_node(value).map(Into::into),
        "FloatValue" => float_value_node(value).map(Into::into),
        "StringValue" => string_value_node(value).map(Into::into),
        "BooleanValue" => boolean_value_node(value).map(Into::into),
        "NullValue" => null_value_node(value).map(Into::into),
        "EnumValue" => enum_value_node(value).map(Into::into),
        "ListValue" => list_value_node(value).map(Into::into),
        "ObjectValue" => object_value_node(value).map(Into::into),
        _ => return None,
    };
    Some(node)
}

/// Reads a type reference.
fn type_node(value: &JsValue) -> Result<TypeNode, String> {
    let kind = kind_of(value)?;
    type_of(kind, value).unwrap_or_else(|| Err(format!("{kind} is not a type reference")))
}

/// Reads a type reference of the kind `kind`, or returns `None` for another
/// kind.
fn type_of(kind: &str, value: &JsValue) -> Option<Result<TypeNode, String>> {
    let node = match kind {
        "NamedType" => named_type_node(value).map(Into::into),
        "ListType" => list_type_node(value).map(Into::into),
        "NonNullType" => non_null_type_node(value).map(Into::into),
        _ => return None,
    };
    Some(node)
}

/// Reads the type a non null type wraps, which is never non null itself.
fn nullable_type_node(value: &JsValue) -> Result<NullableTypeNode, String> {
    match kind_of(value)? {
        "NamedType" => named_type_node(value).map(Into::into),
        "ListType" => list_type_node(value).map(Into::into),
        kind => Err(format!("{kind} is not a nullable type reference")),
    }
}

/// Reads a `Name` node.
fn name_node(value: &JsValue) -> Result<NameNode, String> {
    let fields = Fields::of(value, "Name")?;
    Ok(NameNode::new(None, fields.string("value")?))
}

/// Reads a `Document` node.
fn document_node(value: &JsValue) -> Result<DocumentNode, String> {
    let fields = Fields::of(value, "Document")?;
    Ok(DocumentNode {
        loc: None,
        definitions: fields.list("definitions", definition_node)?,
        token_count: None,
    })
}

/// Reads an `OperationDefinition` node.
fn operation_definition_node(value: &JsValue) -> Result<OperationDefinitionNode, String> {
    let fields = Fields::of(value, "OperationDefinition")?;
    Ok(OperationDefinitionNode {
        description: fields.optional("description", string_value_node)?,
        loc: None,
        operation: fields.operation("operation")?,
        name: fields.optional("name", name_node)?,
        variable_definitions: fields
            .optional_list("variableDefinitions", variable_definition_node)?,
        directives: fields.optional_list("directives", directive_node)?,
        selection_set: fields.required("selectionSet", selection_set_node)?,
    })
}

/// Reads a `VariableDefinition` node.
fn variable_definition_node(value: &JsValue) -> Result<VariableDefinitionNode, String> {
    let fields = Fields::of(value, "VariableDefinition")?;
    Ok(VariableDefinitionNode {
        description: fields.optional("description", string_value_node)?,
        loc: None,
        variable: fields.required("variable", variable_node)?,
        r#type: fields.required("type", type_node)?,
        default_value: fields.optional("defaultValue", value_node)?,
        directives: fields.optional_list("directives", directive_node)?,
    })
}

/// Reads a `Variable` node.
fn variable_node(value: &JsValue) -> Result<VariableNode, String> {
    let fields = Fields::of(value, "Variable")?;
    Ok(VariableNode {
        loc: None,
        name: fields.required("name", name_node)?,
    })
}

/// Reads a `SelectionSet` node.
fn selection_set_node(value: &JsValue) -> Result<SelectionSetNode, String> {
    let fields = Fields::of(value, "SelectionSet")?;
    Ok(SelectionSetNode {
        loc: None,
        selections: SharedSlice::from(fields.list("selections", selection_node)?),
    })
}

/// Reads a `Field` node.
fn field_node(value: &JsValue) -> Result<FieldNode, String> {
    let fields = Fields::of(value, "Field")?;
    Ok(FieldNode {
        loc: None,
        alias: fields.optional("alias", name_node)?,
        name: fields.required("name", name_node)?,
        arguments: fields.optional_list("arguments", argument_node)?,
        directives: fields.optional_list("directives", directive_node)?,
        selection_set: fields.optional("selectionSet", selection_set_node)?,
    })
}

/// Reads an `Argument` node.
fn argument_node(value: &JsValue) -> Result<ArgumentNode, String> {
    let fields = Fields::of(value, "Argument")?;
    Ok(ArgumentNode {
        loc: None,
        name: fields.required("name", name_node)?,
        value: fields.required("value", value_node)?,
    })
}

/// Reads a `FragmentSpread` node.
fn fragment_spread_node(value: &JsValue) -> Result<FragmentSpreadNode, String> {
    let fields = Fields::of(value, "FragmentSpread")?;
    Ok(FragmentSpreadNode {
        loc: None,
        name: fields.required("name", name_node)?,
        directives: fields.optional_list("directives", directive_node)?,
    })
}

/// Reads an `InlineFragment` node.
fn inline_fragment_node(value: &JsValue) -> Result<InlineFragmentNode, String> {
    let fields = Fields::of(value, "InlineFragment")?;
    Ok(InlineFragmentNode {
        loc: None,
        type_condition: fields.optional("typeCondition", named_type_node)?,
        directives: fields.optional_list("directives", directive_node)?,
        selection_set: fields.required("selectionSet", selection_set_node)?,
    })
}

/// Reads a `FragmentDefinition` node.
fn fragment_definition_node(value: &JsValue) -> Result<FragmentDefinitionNode, String> {
    let fields = Fields::of(value, "FragmentDefinition")?;
    Ok(FragmentDefinitionNode {
        description: fields.optional("description", string_value_node)?,
        loc: None,
        name: fields.required("name", name_node)?,
        variable_definitions: fields
            .optional_list("variableDefinitions", variable_definition_node)?,
        type_condition: fields.required("typeCondition", named_type_node)?,
        directives: fields.optional_list("directives", directive_node)?,
        selection_set: fields.required("selectionSet", selection_set_node)?,
    })
}

/// Reads an `IntValue` node.
fn int_value_node(value: &JsValue) -> Result<IntValueNode, String> {
    let fields = Fields::of(value, "IntValue")?;
    Ok(IntValueNode {
        loc: None,
        value: fields.string("value")?,
    })
}

/// Reads a `FloatValue` node.
fn float_value_node(value: &JsValue) -> Result<FloatValueNode, String> {
    let fields = Fields::of(value, "FloatValue")?;
    Ok(FloatValueNode {
        loc: None,
        value: fields.string("value")?,
    })
}

/// Reads a `StringValue` node.
fn string_value_node(value: &JsValue) -> Result<StringValueNode, String> {
    let fields = Fields::of(value, "StringValue")?;
    Ok(StringValueNode {
        loc: None,
        value: fields.string("value")?,
        block: fields.optional_boolean("block")?,
    })
}

/// Reads a `BooleanValue` node.
fn boolean_value_node(value: &JsValue) -> Result<BooleanValueNode, String> {
    let fields = Fields::of(value, "BooleanValue")?;
    let flag = fields.optional_boolean("value")?;
    Ok(BooleanValueNode {
        loc: None,
        value: flag.ok_or_else(|| fields.missing("value"))?,
    })
}

/// Reads a `NullValue` node.
fn null_value_node(value: &JsValue) -> Result<NullValueNode, String> {
    Fields::of(value, "NullValue")?;
    Ok(NullValueNode { loc: None })
}

/// Reads an `EnumValue` node.
fn enum_value_node(value: &JsValue) -> Result<EnumValueNode, String> {
    let fields = Fields::of(value, "EnumValue")?;
    Ok(EnumValueNode {
        loc: None,
        value: fields.string("value")?,
    })
}

/// Reads a `ListValue` node.
fn list_value_node(value: &JsValue) -> Result<ListValueNode, String> {
    let fields = Fields::of(value, "ListValue")?;
    Ok(ListValueNode {
        loc: None,
        values: fields.list("values", value_node)?,
    })
}

/// Reads an `ObjectValue` node.
fn object_value_node(value: &JsValue) -> Result<ObjectValueNode, String> {
    let fields = Fields::of(value, "ObjectValue")?;
    Ok(ObjectValueNode {
        loc: None,
        fields: fields.list("fields", object_field_node)?,
    })
}

/// Reads an `ObjectField` node.
fn object_field_node(value: &JsValue) -> Result<ObjectFieldNode, String> {
    let fields = Fields::of(value, "ObjectField")?;
    Ok(ObjectFieldNode {
        loc: None,
        name: fields.required("name", name_node)?,
        value: fields.required("value", value_node)?,
    })
}

/// Reads a `Directive` node.
fn directive_node(value: &JsValue) -> Result<DirectiveNode, String> {
    let fields = Fields::of(value, "Directive")?;
    Ok(DirectiveNode {
        loc: None,
        name: fields.required("name", name_node)?,
        arguments: fields.optional_list("arguments", argument_node)?,
    })
}

/// Reads a `NamedType` node.
fn named_type_node(value: &JsValue) -> Result<NamedTypeNode, String> {
    let fields = Fields::of(value, "NamedType")?;
    Ok(NamedTypeNode {
        loc: None,
        name: fields.required("name", name_node)?,
    })
}

/// Reads a `ListType` node.
fn list_type_node(value: &JsValue) -> Result<ListTypeNode, String> {
    let fields = Fields::of(value, "ListType")?;
    Ok(ListTypeNode {
        loc: None,
        r#type: Box::new(fields.required("type", type_node)?),
    })
}

/// Reads a `NonNullType` node.
fn non_null_type_node(value: &JsValue) -> Result<NonNullTypeNode, String> {
    let fields = Fields::of(value, "NonNullType")?;
    Ok(NonNullTypeNode {
        loc: None,
        r#type: fields.required("type", nullable_type_node)?,
    })
}

/// Reads a `SchemaDefinition` node.
fn schema_definition_node(value: &JsValue) -> Result<SchemaDefinitionNode, String> {
    let fields = Fields::of(value, "SchemaDefinition")?;
    Ok(SchemaDefinitionNode {
        loc: None,
        description: fields.optional("description", string_value_node)?,
        directives: fields.optional_list("directives", directive_node)?,
        operation_types: fields.list("operationTypes", operation_type_definition_node)?,
    })
}

/// Reads an `OperationTypeDefinition` node.
fn operation_type_definition_node(value: &JsValue) -> Result<OperationTypeDefinitionNode, String> {
    let fields = Fields::of(value, "OperationTypeDefinition")?;
    Ok(OperationTypeDefinitionNode {
        loc: None,
        operation: fields.operation("operation")?,
        r#type: fields.required("type", named_type_node)?,
    })
}

/// Reads a `ScalarTypeDefinition` node.
fn scalar_type_definition_node(value: &JsValue) -> Result<ScalarTypeDefinitionNode, String> {
    let fields = Fields::of(value, "ScalarTypeDefinition")?;
    Ok(ScalarTypeDefinitionNode {
        loc: None,
        description: fields.optional("description", string_value_node)?,
        name: fields.required("name", name_node)?,
        directives: fields.optional_list("directives", directive_node)?,
    })
}

/// Reads an `ObjectTypeDefinition` node.
fn object_type_definition_node(value: &JsValue) -> Result<ObjectTypeDefinitionNode, String> {
    let fields = Fields::of(value, "ObjectTypeDefinition")?;
    Ok(ObjectTypeDefinitionNode {
        loc: None,
        description: fields.optional("description", string_value_node)?,
        name: fields.required("name", name_node)?,
        interfaces: fields.optional_list("interfaces", named_type_node)?,
        directives: fields.optional_list("directives", directive_node)?,
        fields: fields.optional_list("fields", field_definition_node)?,
    })
}

/// Reads a `FieldDefinition` node.
fn field_definition_node(value: &JsValue) -> Result<FieldDefinitionNode, String> {
    let fields = Fields::of(value, "FieldDefinition")?;
    Ok(FieldDefinitionNode {
        loc: None,
        description: fields.optional("description", string_value_node)?,
        name: fields.required("name", name_node)?,
        arguments: fields.optional_list("arguments", input_value_definition_node)?,
        r#type: fields.required("type", type_node)?,
        directives: fields.optional_list("directives", directive_node)?,
    })
}

/// Reads an `InputValueDefinition` node.
fn input_value_definition_node(value: &JsValue) -> Result<InputValueDefinitionNode, String> {
    let fields = Fields::of(value, "InputValueDefinition")?;
    Ok(InputValueDefinitionNode {
        loc: None,
        description: fields.optional("description", string_value_node)?,
        name: fields.required("name", name_node)?,
        r#type: fields.required("type", type_node)?,
        default_value: fields.optional("defaultValue", value_node)?,
        directives: fields.optional_list("directives", directive_node)?,
    })
}

/// Reads an `InterfaceTypeDefinition` node.
fn interface_type_definition_node(value: &JsValue) -> Result<InterfaceTypeDefinitionNode, String> {
    let fields = Fields::of(value, "InterfaceTypeDefinition")?;
    Ok(InterfaceTypeDefinitionNode {
        loc: None,
        description: fields.optional("description", string_value_node)?,
        name: fields.required("name", name_node)?,
        interfaces: fields.optional_list("interfaces", named_type_node)?,
        directives: fields.optional_list("directives", directive_node)?,
        fields: fields.optional_list("fields", field_definition_node)?,
    })
}

/// Reads a `UnionTypeDefinition` node.
fn union_type_definition_node(value: &JsValue) -> Result<UnionTypeDefinitionNode, String> {
    let fields = Fields::of(value, "UnionTypeDefinition")?;
    Ok(UnionTypeDefinitionNode {
        loc: None,
        description: fields.optional("description", string_value_node)?,
        name: fields.required("name", name_node)?,
        directives: fields.optional_list("directives", directive_node)?,
        types: fields.optional_list("types", named_type_node)?,
    })
}

/// Reads an `EnumTypeDefinition` node.
fn enum_type_definition_node(value: &JsValue) -> Result<EnumTypeDefinitionNode, String> {
    let fields = Fields::of(value, "EnumTypeDefinition")?;
    Ok(EnumTypeDefinitionNode {
        loc: None,
        description: fields.optional("description", string_value_node)?,
        name: fields.required("name", name_node)?,
        directives: fields.optional_list("directives", directive_node)?,
        values: fields.optional_list("values", enum_value_definition_node)?,
    })
}

/// Reads an `EnumValueDefinition` node.
fn enum_value_definition_node(value: &JsValue) -> Result<EnumValueDefinitionNode, String> {
    let fields = Fields::of(value, "EnumValueDefinition")?;
    Ok(EnumValueDefinitionNode {
        loc: None,
        description: fields.optional("description", string_value_node)?,
        name: fields.required("name", name_node)?,
        directives: fields.optional_list("directives", directive_node)?,
    })
}

/// Reads an `InputObjectTypeDefinition` node.
fn input_object_type_definition_node(
    value: &JsValue,
) -> Result<InputObjectTypeDefinitionNode, String> {
    let fields = Fields::of(value, "InputObjectTypeDefinition")?;
    Ok(InputObjectTypeDefinitionNode {
        loc: None,
        description: fields.optional("description", string_value_node)?,
        name: fields.required("name", name_node)?,
        directives: fields.optional_list("directives", directive_node)?,
        fields: fields.optional_list("fields", input_value_definition_node)?,
    })
}

/// Reads a `DirectiveDefinition` node.
///
/// An absent `repeatable` reads as `false`, because the printer tests it for
/// truth.
fn directive_definition_node(value: &JsValue) -> Result<DirectiveDefinitionNode, String> {
    let fields = Fields::of(value, "DirectiveDefinition")?;
    Ok(DirectiveDefinitionNode {
        loc: None,
        description: fields.optional("description", string_value_node)?,
        name: fields.required("name", name_node)?,
        arguments: fields.optional_list("arguments", input_value_definition_node)?,
        repeatable: fields.optional_boolean("repeatable")?.unwrap_or(false),
        locations: fields.list("locations", name_node)?,
    })
}

/// Reads a `SchemaExtension` node.
fn schema_extension_node(value: &JsValue) -> Result<SchemaExtensionNode, String> {
    let fields = Fields::of(value, "SchemaExtension")?;
    Ok(SchemaExtensionNode {
        loc: None,
        directives: fields.optional_list("directives", directive_node)?,
        operation_types: fields.optional_list("operationTypes", operation_type_definition_node)?,
    })
}

/// Reads a `ScalarTypeExtension` node.
fn scalar_type_extension_node(value: &JsValue) -> Result<ScalarTypeExtensionNode, String> {
    let fields = Fields::of(value, "ScalarTypeExtension")?;
    Ok(ScalarTypeExtensionNode {
        loc: None,
        name: fields.required("name", name_node)?,
        directives: fields.optional_list("directives", directive_node)?,
    })
}

/// Reads an `ObjectTypeExtension` node.
fn object_type_extension_node(value: &JsValue) -> Result<ObjectTypeExtensionNode, String> {
    let fields = Fields::of(value, "ObjectTypeExtension")?;
    Ok(ObjectTypeExtensionNode {
        loc: None,
        name: fields.required("name", name_node)?,
        interfaces: fields.optional_list("interfaces", named_type_node)?,
        directives: fields.optional_list("directives", directive_node)?,
        fields: fields.optional_list("fields", field_definition_node)?,
    })
}

/// Reads an `InterfaceTypeExtension` node.
fn interface_type_extension_node(value: &JsValue) -> Result<InterfaceTypeExtensionNode, String> {
    let fields = Fields::of(value, "InterfaceTypeExtension")?;
    Ok(InterfaceTypeExtensionNode {
        loc: None,
        name: fields.required("name", name_node)?,
        interfaces: fields.optional_list("interfaces", named_type_node)?,
        directives: fields.optional_list("directives", directive_node)?,
        fields: fields.optional_list("fields", field_definition_node)?,
    })
}

/// Reads a `UnionTypeExtension` node.
fn union_type_extension_node(value: &JsValue) -> Result<UnionTypeExtensionNode, String> {
    let fields = Fields::of(value, "UnionTypeExtension")?;
    Ok(UnionTypeExtensionNode {
        loc: None,
        name: fields.required("name", name_node)?,
        directives: fields.optional_list("directives", directive_node)?,
        types: fields.optional_list("types", named_type_node)?,
    })
}

/// Reads an `EnumTypeExtension` node.
fn enum_type_extension_node(value: &JsValue) -> Result<EnumTypeExtensionNode, String> {
    let fields = Fields::of(value, "EnumTypeExtension")?;
    Ok(EnumTypeExtensionNode {
        loc: None,
        name: fields.required("name", name_node)?,
        directives: fields.optional_list("directives", directive_node)?,
        values: fields.optional_list("values", enum_value_definition_node)?,
    })
}

/// Reads an `InputObjectTypeExtension` node.
fn input_object_type_extension_node(
    value: &JsValue,
) -> Result<InputObjectTypeExtensionNode, String> {
    let fields = Fields::of(value, "InputObjectTypeExtension")?;
    Ok(InputObjectTypeExtensionNode {
        loc: None,
        name: fields.required("name", name_node)?,
        directives: fields.optional_list("directives", directive_node)?,
        fields: fields.optional_list("fields", input_value_definition_node)?,
    })
}

/// Reads a `TypeCoordinate` node.
fn type_coordinate_node(value: &JsValue) -> Result<TypeCoordinateNode, String> {
    let fields = Fields::of(value, "TypeCoordinate")?;
    Ok(TypeCoordinateNode {
        loc: None,
        name: fields.required("name", name_node)?,
    })
}

/// Reads a `MemberCoordinate` node.
fn member_coordinate_node(value: &JsValue) -> Result<MemberCoordinateNode, String> {
    let fields = Fields::of(value, "MemberCoordinate")?;
    Ok(MemberCoordinateNode {
        loc: None,
        name: fields.required("name", name_node)?,
        member_name: fields.required("memberName", name_node)?,
    })
}

/// Reads an `ArgumentCoordinate` node.
fn argument_coordinate_node(value: &JsValue) -> Result<ArgumentCoordinateNode, String> {
    let fields = Fields::of(value, "ArgumentCoordinate")?;
    Ok(ArgumentCoordinateNode {
        loc: None,
        name: fields.required("name", name_node)?,
        field_name: fields.required("fieldName", name_node)?,
        argument_name: fields.required("argumentName", name_node)?,
    })
}

/// Reads a `DirectiveCoordinate` node.
fn directive_coordinate_node(value: &JsValue) -> Result<DirectiveCoordinateNode, String> {
    let fields = Fields::of(value, "DirectiveCoordinate")?;
    Ok(DirectiveCoordinateNode {
        loc: None,
        name: fields.required("name", name_node)?,
    })
}

/// Reads a `DirectiveArgumentCoordinate` node.
fn directive_argument_coordinate_node(
    value: &JsValue,
) -> Result<DirectiveArgumentCoordinateNode, String> {
    let fields = Fields::of(value, "DirectiveArgumentCoordinate")?;
    Ok(DirectiveArgumentCoordinateNode {
        loc: None,
        name: fields.required("name", name_node)?,
        argument_name: fields.required("argumentName", name_node)?,
    })
}
