// SPDX-License-Identifier: MIT
/*
 * aosabi-core boot check: the model a boot reads held to the SDL it boots.
 *
 * No analogue in the reference, where the build prints the SDL from the
 * model, so the two agree by construction.
 */

//! The model held to the SDL.
//!
//! A host hands the SDL and the model of one build in two texts, so a boot
//! holds them to each other before it builds the schema: each root operation
//! of the model is a field of its root type in the SDL with the arguments of
//! the model in their order, the order a resolver takes positional
//! arguments in, and each type resolver stands on a union or an interface of
//! the SDL whose members are object types of the SDL. The SDL is read through
//! the parser of `aosabi-lang`, and the schema build of
//! [`crate::create_schema()`] reads it again through `aosabi-type`.

use aosabi_lang::{
    ParseOptions,
    ast::{
        DefinitionNode,
        DocumentNode,
        FieldDefinitionNode,
        OperationTypeNode, //
    },
    js_string::to_string_lossy,
    parser::parse, //
};

use super::ir::Ir;
use crate::glue::resolve_type::ResolveTypeRule;

/// The default name of the root type of each root kind of the model.
const ROOTS: [(&str, OperationTypeNode); 3] = [
    ("Query", OperationTypeNode::Query),
    ("Mutation", OperationTypeNode::Mutation),
    ("Subscription", OperationTypeNode::Subscription),
];

/// Holds `ir` and `rules` to the SDL `type_defs`.
///
/// # Errors
///
/// Returns the message of the syntax error of the SDL, or a message that
/// names the first part of the model the SDL does not hold as the model
/// holds it.
pub fn against_sdl(type_defs: &str, ir: &Ir, rules: &[ResolveTypeRule]) -> Result<(), String> {
    let document = parse(type_defs, ParseOptions::default())
        .map_err(|error| to_string_lossy(&error.message).into_owned())?;
    for operation in &ir.operations {
        let root = root_name(&document, &operation.root).ok_or_else(|| {
            format!(
                "the IR names the root {}, which no root kind holds",
                operation.root
            )
        })?;
        let field = object_field(&document, &root, &operation.name).ok_or_else(|| {
            format!(
                "the IR names the field {root}.{}, which the SDL lacks",
                operation.name
            )
        })?;
        let args: Vec<&str> = field
            .arguments
            .iter()
            .flatten()
            .map(|argument| argument.name.value())
            .collect();
        if args != operation.args {
            return Err(format!(
                "the IR names the arguments ({}) of {root}.{}, where the SDL holds ({})",
                operation.args.join(", "),
                operation.name,
                args.join(", ")
            ));
        }
    }
    for rule in rules {
        if !is_abstract(&document, rule.type_name) {
            return Err(format!(
                "the IR resolves the type {}, which is no union and no interface of the SDL",
                rule.type_name
            ));
        }
        if let Some(name) = rule
            .valid_names
            .iter()
            .find(|name| !is_object(&document, name))
        {
            return Err(format!(
                "the IR resolves {} to {name}, which is no object type of the SDL",
                rule.type_name
            ));
        }
    }
    Ok(())
}

/// Returns the name of the root type of the root kind `root` of the model,
/// the one a schema definition of `document` names or else the default.
fn root_name(document: &DocumentNode, root: &str) -> Option<String> {
    let (default, operation) = ROOTS.iter().find(|(name, _)| *name == root)?;
    let named = document
        .definitions
        .iter()
        .find_map(|definition| match definition {
            DefinitionNode::SchemaDefinition(schema) => schema
                .operation_types
                .iter()
                .find(|entry| entry.operation == *operation)
                .map(|entry| String::from(entry.r#type.name.value())),
            _ => None,
        });
    Some(named.unwrap_or_else(|| String::from(*default)))
}

/// Returns the field `field` of the object type `type_name` of `document`.
fn object_field<'a>(
    document: &'a DocumentNode,
    type_name: &str,
    field: &str,
) -> Option<&'a FieldDefinitionNode> {
    document
        .definitions
        .iter()
        .filter_map(|definition| match definition {
            DefinitionNode::ObjectTypeDefinition(object) if object.name.value() == type_name => {
                object.fields.as_ref()
            }
            _ => None,
        })
        .flatten()
        .find(|candidate| candidate.name.value() == field)
}

/// Returns whether `document` defines `name` as a union or an interface.
fn is_abstract(document: &DocumentNode, name: &str) -> bool {
    document
        .definitions
        .iter()
        .any(|definition| match definition {
            DefinitionNode::UnionTypeDefinition(union) => union.name.value() == name,
            DefinitionNode::InterfaceTypeDefinition(interface) => interface.name.value() == name,
            _ => false,
        })
}

/// Returns whether `document` defines `name` as an object type.
fn is_object(document: &DocumentNode, name: &str) -> bool {
    document.definitions.iter().any(|definition| {
        matches!(definition, DefinitionNode::ObjectTypeDefinition(object) if object.name.value() == name)
    })
}
