// SPDX-License-Identifier: MIT
/*
 * aosabi-execute values: the coercion of variable values and argument values.
 *
 * Derived from @graphql-tools/executor 1.5.1, cjs/execution/values.js, MIT.
 * Derived from @graphql-tools/utils 11.0.0, cjs/getArgumentValues.js, MIT.
 * Derived from graphql-js 16.12.0, src/execution/values.ts, MIT.
 */

//! The variable and argument coercion of the executor.
//!
//! The coercion itself belongs to the type layer. This module calls
//! [`type_from_ast()`], [`value_from_ast()`] and [`coerce_input_value()`] and adds
//! the messages of the executor around them.
//!
//! The executor prints a value into a message with `cross-inspect`, where the
//! type layer prints it with the `inspect` of graphql-js. The prefix of a
//! variable error is therefore printed with [`cross_inspect`], and the tail,
//! which is the message of the error the type layer made, keeps the other
//! printer. A type prints through its `toJSON` in both, which answers the
//! type string, so `inspect(varType)` and `inspect(argType)` are the
//! `Display` of a [`aosabi_type::Type`].

use core::mem;
use std::borrow::Cow;

use aosabi_json::{
    JsObject,
    JsValue,
    map::IndexSet, //
};
use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions,
    OriginalError,
    PathSegment,
    ast::{
        ArgumentNode,
        AstNode,
        FieldNode,
        ValueNode,
        VariableDefinitionNode, //
    },
    js_string::JsStringBuilder,
    printer::print, //
};
pub use aosabi_type::get_directive_values;
use aosabi_type::{
    Argument,
    DefinitionError,
    Field,
    ObjMap,
    Schema,
    coerce_input_value,
    is_input_type,
    is_non_null_type,
    print_path_array,
    type_from_ast,
    value_from_ast, //
};

use crate::{
    errors::create_graphql_error,
    value::{
        Value,
        cross_inspect, //
    }, //
};

/// What [`get_variable_values`] answers, the `{ errors }` or the `{ coerced }` of the reference.
pub enum CoercedVariableValues {
    /// The errors of a coercion that failed.
    Errors(Vec<GraphQLError>),
    /// The coerced values by variable name.
    Coerced(JsObject),
}

/// The callback `coerceVariableValues` reports an error to, which throws to stop the coercion.
type OnError<'a> = dyn FnMut(GraphQLError) -> Result<(), DefinitionError> + 'a;

/// Prepares an object map of variableValues of the correct type based on the
/// provided variable definitions and arbitrary input.
///
/// If the input cannot be parsed to match the variable definitions, the
/// errors are returned. `inputs` is the raw variable object, whose values the
/// coercion takes and makes the coerced values of where it is owned, and
/// reads into fresh coerced values where it is borrowed, and `max_errors` the
/// `maxErrors` option, past which the coercion stops with one more error.
/// Whatever the coercion throws joins the errors, as the `catch` of the
/// reference does.
#[must_use]
pub fn get_variable_values(
    schema: &Schema,
    var_def_nodes: &[VariableDefinitionNode],
    inputs: Cow<'_, JsObject>,
    max_errors: Option<usize>,
) -> CoercedVariableValues {
    let mut errors = Vec::new();
    let thrown = {
        let mut on_error = |error: GraphQLError| {
            if max_errors.is_some_and(|max_errors| errors.len() >= max_errors) {
                return Err(DefinitionError::from(create_graphql_error(
                    "Too many errors processing variables, error limit reached. Execution aborted.",
                    GraphQLErrorOptions::default(),
                )));
            }
            errors.push(error);
            Ok(())
        };
        coerce_variable_values(schema, var_def_nodes, inputs, &mut on_error)
    };
    match thrown {
        Ok(coerced) if errors.is_empty() => return CoercedVariableValues::Coerced(coerced),
        Ok(_) => {}
        Err(error) => errors.push(graphql_error_of(error)),
    }
    CoercedVariableValues::Errors(errors)
}

/// Returns the coerced value of every variable `inputs` provides or the definition defaults.
///
/// The `hasOwnProperty` of the reference is [`JsObject::contains_key`], so a
/// variable whose value is `undefined` counts as provided. A variable takes
/// its value out of `inputs` it owns, unless a document that skipped
/// validation defines a name twice, whose every definition then reads the
/// value where it stands, as each definition of the reference reads
/// `inputs[varName]` again. A variable of `inputs` another reader shares is
/// read where it stands.
fn coerce_variable_values(
    schema: &Schema,
    var_def_nodes: &[VariableDefinitionNode],
    inputs: Cow<'_, JsObject>,
    on_error: &mut OnError<'_>,
) -> Result<JsObject, DefinitionError> {
    let mut inputs = inputs;
    let taken = matches!(inputs, Cow::Owned(_)) && !defines_a_name_twice(var_def_nodes);
    let mut coerced_values = JsObject::new();
    for var_def_node in var_def_nodes {
        coerce_variable(
            schema,
            var_def_node,
            &mut inputs,
            taken,
            on_error,
            &mut coerced_values,
        )?;
    }
    Ok(coerced_values)
}

/// Coerces the variable `var_def_node` defines into `coerced_values`, from
/// `inputs`, out of it where `taken`, or its default.
///
/// The body of the loop of `coerceVariableValues` at `values.js:41-76`.
// Out of line, so an operation with no variable compiles none of it.
#[inline(never)]
fn coerce_variable(
    schema: &Schema,
    var_def_node: &VariableDefinitionNode,
    inputs: &mut Cow<'_, JsObject>,
    taken: bool,
    on_error: &mut OnError<'_>,
    coerced_values: &mut JsObject,
) -> Result<(), DefinitionError> {
    let var_name = var_def_node.variable.name.value();
    let var_type = type_from_ast(schema, &var_def_node.r#type);

    let Some(var_type) = var_type.filter(is_input_type) else {
        // Must use input types for variables. This should be caught during
        // validation, however is checked again here for safety.
        let var_type_str = print(&var_def_node.r#type);
        on_error(create_graphql_error(
            format!(
                "Variable \"${var_name}\" expected value of type \"{var_type_str}\" which cannot be used as an input type."
            ),
            blame(AstNode::from(var_def_node.r#type.clone())),
        ))?;
        return Ok(());
    };

    if !inputs.contains_key(var_name) {
        if let Some(default_value) = &var_def_node.default_value {
            let value = value_from_ast(Some(default_value), &var_type, None)?;
            coerced_values.insert(var_name, value);
        } else if is_non_null_type(&var_type) {
            on_error(create_graphql_error(
                format!(
                    "Variable \"${var_name}\" of required type \"{var_type}\" was not provided."
                ),
                blame(AstNode::from(var_def_node.clone())),
            ))?;
        }
        return Ok(());
    }

    let value = match &mut *inputs {
        Cow::Owned(inputs) if taken => {
            Cow::Owned(inputs.get_mut(var_name).map(mem::take).unwrap_or_default())
        }
        inputs => Cow::Borrowed(inputs.get(var_name).unwrap_or(&JsValue::Undefined)),
    };
    if matches!(*value, JsValue::Null) && is_non_null_type(&var_type) {
        on_error(create_graphql_error(
            format!("Variable \"${var_name}\" of non-null type \"{var_type}\" must not be null."),
            blame(AstNode::from(var_def_node.clone())),
        ))?;
        return Ok(());
    }

    let coerced = coerce_input_value(value, &var_type, |path, invalid_value, error| {
        on_error(invalid_value_error(
            var_def_node,
            path,
            invalid_value,
            error,
        ))
    })?;
    coerced_values.insert(var_name, coerced);
    Ok(())
}

/// The most variable definitions [`defines_a_name_twice`] compares pair by
/// pair, 120 compares of a name at most.
const PAIRWISE_DEFINITIONS: usize = 16;

/// Tells whether two of `var_def_nodes` define the same variable name.
///
/// Validation refuses such a document, and the executor reads a value per
/// definition all the same, so a document that skipped validation reads it as
/// the reference does. Up to [`PAIRWISE_DEFINITIONS`] definitions the names
/// are compared pair by pair, which allocates nothing and hashes no name, and
/// more go through a set, so the test stays linear in the definitions.
fn defines_a_name_twice(var_def_nodes: &[VariableDefinitionNode]) -> bool {
    if var_def_nodes.len() <= PAIRWISE_DEFINITIONS {
        let mut rest = var_def_nodes;
        while let Some((first, tail)) = rest.split_first() {
            let name = first.variable.name.value();
            if tail.iter().any(|other| other.variable.name.value() == name) {
                return true;
            }
            rest = tail;
        }
        return false;
    }
    let mut names: IndexSet<&str> = IndexSet::default();
    !var_def_nodes
        .iter()
        .all(|var_def_node| names.insert(var_def_node.variable.name.value()))
}

/// Returns the error of a variable value the type layer refuses with `error` at `path`.
///
/// The message of `error` is a JavaScript string, so the message is joined as
/// one.
fn invalid_value_error(
    var_def_node: &VariableDefinitionNode,
    path: &[PathSegment],
    invalid_value: &JsValue,
    error: GraphQLError,
) -> GraphQLError {
    let var_name = var_def_node.variable.name.value();
    let mut prefix = format!(
        "Variable \"${var_name}\" got invalid value {}",
        cross_inspect(&Value::from_js(invalid_value))
    );
    if !path.is_empty() {
        prefix = format!("{prefix} at \"{var_name}{}\"", print_path_array(path));
    }
    let mut message = JsStringBuilder::new();
    message
        .push_str(&prefix)
        .push_str("; ")
        .push(&error.message);
    create_graphql_error(
        message.finish(),
        GraphQLErrorOptions {
            nodes: Some(vec![AstNode::from(var_def_node.clone())]),
            original_error: Some(OriginalError::from(error)),
            ..GraphQLErrorOptions::default()
        },
    )
}

/// Prepares an object map of argument values given a list of argument
/// definitions and list of argument AST nodes.
///
/// The shape for a directive is [`aosabi_type::get_argument_values`].
/// `variable_values` is tested with `hasOwnProperty`, so a key that holds
/// `undefined` counts as provided and reads as null. The keys of the result
/// follow the argument definitions.
///
/// # Errors
///
/// Returns the four `GraphQLError`s of the reference, for a required
/// argument that is missing, a required argument whose variable has no
/// runtime value, a non null argument given null, and an argument whose
/// literal does not coerce, and what `valueFromAST` throws.
// The error is a `GraphQLError`, which the caller throws as it is, so a box
// would be unwrapped at the one place that reads it, and the error path runs
// at most once per field. The lint depends on the target, since the error
// reaches its threshold of 128 bytes on a 64-bit target and stays under it in
// the build of the crate for wasm32, so it is allowed, as the parser of
// aosabi-lang allows it for the same error.
#[allow(clippy::result_large_err)]
pub fn get_argument_values(
    def: &Field,
    node: &FieldNode,
    variable_values: &JsObject,
) -> Result<JsObject, GraphQLError> {
    let mut coerced_values = JsObject::new();
    let argument_nodes = node.arguments.as_deref().unwrap_or(&[]);
    let mut arg_node_map: ObjMap<&ArgumentNode> = ObjMap::default();
    for arg in argument_nodes {
        arg_node_map.insert(String::from(arg.name.value()), arg);
    }

    for arg_def in &def.args {
        coerce_argument(
            arg_def,
            &arg_node_map,
            node,
            variable_values,
            &mut coerced_values,
        )?;
    }
    Ok(coerced_values)
}

/// Coerces the argument `arg_def` of the field `node` into `coerced_values`,
/// from its node in `arg_node_map` or its default.
///
/// The body of the loop of `getArgumentValues` at `values.js:154-232`.
// Out of line, so a field with no argument compiles none of it. The error is
// the `GraphQLError` of `get_argument_values`, which throws it as it is, and
// the lint is allowed as there, since it depends on the target.
#[allow(clippy::result_large_err)]
#[inline(never)]
fn coerce_argument(
    arg_def: &Argument,
    arg_node_map: &ObjMap<&ArgumentNode>,
    node: &FieldNode,
    variable_values: &JsObject,
    coerced_values: &mut JsObject,
) -> Result<(), GraphQLError> {
    let name = &arg_def.name;
    let arg_type = &arg_def.r#type;
    let default_value = &arg_def.default_value;
    let Some(argument_node) = arg_node_map.get(name) else {
        if !default_value.is_undefined() {
            coerced_values.insert(name.as_str(), default_value.clone());
        } else if is_non_null_type(arg_type) {
            return Err(create_graphql_error(
                format!("Argument \"{name}\" of required type \"{arg_type}\" was not provided."),
                blame(AstNode::from(node.clone())),
            ));
        }
        return Ok(());
    };

    let value_node = &argument_node.value;
    let mut is_null = matches!(value_node, ValueNode::NullValue(_));

    if let ValueNode::Variable(variable) = value_node {
        let variable_name = variable.name.value();
        if !variable_values.contains_key(variable_name) {
            if !default_value.is_undefined() {
                coerced_values.insert(name.as_str(), default_value.clone());
            } else if is_non_null_type(arg_type) {
                return Err(create_graphql_error(
                    format!(
                        "Argument \"{name}\" of required type \"{arg_type}\" was provided the variable \"${variable_name}\" which was not provided a runtime value."
                    ),
                    blame(AstNode::from(value_node.clone())),
                ));
            }
            return Ok(());
        }
        is_null = variable_values
            .get(variable_name)
            .is_none_or(|value| matches!(value, JsValue::Null | JsValue::Undefined));
    }

    if is_null && is_non_null_type(arg_type) {
        return Err(create_graphql_error(
            format!("Argument \"{name}\" of non-null type \"{arg_type}\" must not be null."),
            blame(AstNode::from(value_node.clone())),
        ));
    }

    let coerced_value = value_from_ast(Some(value_node), arg_type, Some(variable_values))
        .map_err(graphql_error_of)?;
    if coerced_value.is_undefined() {
        // Note: ValuesOfCorrectTypeRule validation should catch this before
        // execution. This is a runtime check to ensure execution does not
        // continue with an invalid argument value.
        return Err(create_graphql_error(
            format!(
                "Argument \"{name}\" has invalid value {}.",
                print(value_node)
            ),
            blame(AstNode::from(value_node.clone())),
        ));
    }
    coerced_values.insert(name.as_str(), coerced_value);
    Ok(())
}

/// Returns the options that blame `node`, which is `{ nodes: node }` and `{ nodes: [node] }`.
fn blame(node: AstNode) -> GraphQLErrorOptions {
    GraphQLErrorOptions {
        nodes: Some(vec![node]),
        ..GraphQLErrorOptions::default()
    }
}

/// Returns the `GraphQLError` a throw of the type layer joins an error list as.
///
/// An error list of the executor holds `GraphQLError`s. A plain `Error`
/// keeps its message, the one member a response shows.
fn graphql_error_of(error: DefinitionError) -> GraphQLError {
    match error {
        DefinitionError::GraphQL(error) => *error,
        DefinitionError::Plain(message) => {
            create_graphql_error(message, GraphQLErrorOptions::default())
        }
    }
}
