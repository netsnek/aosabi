// SPDX-License-Identifier: MIT
/*
 * aosabi-federation: the request a delegation builds from the field it
 * delegates.
 *
 * Derived from @graphql-tools/delegate 12.0.9, dist/index.cjs, MIT.
 */

//! The request of a delegation, `create_request`.
//!
//! The request holds one operation whose single root field is the target's
//! field, without the alias of the field that delegates and with its
//! directives and its selections. Each argument the delegation passes becomes
//! the client's own variable node when the client sent a variable of the same
//! value, else a fresh variable named after the argument, renamed
//! `_<field>_<name>` and then `_<i>_<field>_<name>` while the name is taken,
//! typed by the argument of the target's field and valued by
//! [`project_argument_value`], or a literal when the target's field has no
//! such argument. The variables spread the client's values, shared and not
//! copied, with the projected arguments written over them, and the fragments
//! of the client's document follow the operation, shared in the member
//! `fragments` of the request until the preparation reads them.
//!
//! The comparison of a variable's value with an argument is the `===` of the
//! reference. An opaque value, the file of an upload, keeps its identity and
//! equals the very same file, so the inner delegation keeps the variable
//! `_aosabiUpload_<n>` that `InlineArgsTransform` declares for a root argument
//! of file type, as the reference does. An object or a list of the JSON model
//! has no identity and equals no other value, where the reference finds an
//! object argument equal when the root resolver passes the very object of the
//! variable. So the port names a fresh variable for such an argument where the
//! reference keeps the client's node. That happens only in the outer
//! delegation, whose request `InlineArgsTransform` rewrites right after,
//! writing every argument variable as a literal with each file lifted into a
//! variable `_aosabiUpload_<n>` and declaring no other variable, so the fresh
//! name never reaches the wire.
//!
//! The type of a fresh variable is the member `type` of the node `astFromArg`
//! makes, which is [`ast_from_type`] of the argument's type, and the
//! description, the default value and the directives of that node are never
//! read.

use std::{
    rc::Rc,
    sync::Arc, //
};

use aosabi_execute::{
    errors::thrown_definition_error,
    execute::{
        FieldNodes,
        ResolveInfo, //
    },
    value::{
        ContextValue,
        ErrorObject,
        Value,
        to_js_string, //
    }, //
};
use aosabi_json::{
    JsObject,
    JsString,
    JsValue, //
};
use aosabi_lang::{
    SharedSlice,
    ast::{
        ArgumentNode,
        DefinitionNode,
        DocumentNode,
        FieldNode,
        FragmentDefinitionNode,
        NameNode,
        OperationDefinitionNode,
        OperationTypeNode,
        SelectionNode,
        SelectionSetNode,
        ValueNode,
        VariableDefinitionNode,
        VariableNode, //
    },
    js_string::to_string_lossy, //
};
use aosabi_type::{
    NamedType,
    Schema,
    Type,
    scalars::string_to_number,
    wrapping::ast_from_type, //
};

use crate::{
    transforms::{
        ExecutionRequest,
        Variables, //
    },
    utils::{
        ast_from_value_untyped,
        get_defined_root_type, //
    }, //
};

/// The options of `createRequest`, `ICreateRequest`.
#[derive(Clone)]
pub struct CreateRequestOptions {
    /// The name of the subschema, `subgraphName`.
    pub subgraph_name: Option<String>,
    /// The fragments of the client's document, `fragments`, shared with it.
    pub fragments: Option<Rc<[Rc<FragmentDefinitionNode>]>>,
    /// The value the root fields resolve on, `rootValue`.
    pub root_value: Value,
    /// The name of the operation, `targetOperationName`.
    pub target_operation_name: Option<String>,
    /// The type of the operation, `targetOperation`.
    pub target_operation: OperationTypeNode,
    /// The schema of the target, `targetSchema`.
    pub target_schema: Option<Arc<Schema>>,
    /// The name of the target's root field, `targetFieldName`.
    pub target_field_name: Option<String>,
    /// The selection set of the root field, `selectionSet`.
    pub selection_set: Option<SelectionSetNode>,
    /// The field nodes of the field that delegates, `fieldNodes`.
    pub field_nodes: Option<FieldNodes>,
    /// The context value, `context`.
    pub context: Option<ContextValue>,
    /// The resolve information of the field that delegates, `info`.
    pub info: Option<Rc<ResolveInfo>>,
    /// The arguments of the target's field, `args`.
    pub args: Option<JsObject>,
}

/// Returns the request of a delegation for one root field of the target.
///
/// # Errors
///
/// Returns the plain error `Either "targetFieldName" or a non empty
/// "fieldNodes" array must be provided.` without a root field name, the
/// refusal of [`get_defined_root_type`] for an operation the target cannot
/// run, and the `TypeError` of [`ast_from_value_untyped`] for an argument the
/// target does not declare whose value it cannot write.
pub fn create_request(options: CreateRequestOptions) -> Result<ExecutionRequest, Value> {
    let field_nodes: &[Rc<FieldNode>] = options.field_nodes.as_deref().unwrap_or(&[]);
    let new_selection_set = options
        .selection_set
        .clone()
        .or_else(|| selections_of(field_nodes));
    let field_node = field_nodes.first();
    let Some(root_field_name) = options
        .target_field_name
        .clone()
        .or_else(|| field_node.map(|node| String::from(node.name.value())))
    else {
        return Err(Value::from(ErrorObject::error(
            "Either \"targetFieldName\" or a non empty \"fieldNodes\" array must be provided.",
        )));
    };
    let info = options.info.as_deref();
    let mut arguments = Arguments {
        new_variables: info.map_or_else(Variables::default, |info| {
            Variables::spreading(Rc::clone(&info.variable_values))
        }),
        variable_definitions: info
            .and_then(|info| info.operation.variable_definitions.clone())
            .unwrap_or_default(),
        arg_nodes: Vec::new(),
    };
    if let Some(args) = &options.args {
        let root_type = match &options.target_schema {
            Some(target_schema) => Some(get_defined_root_type(
                target_schema,
                options.target_operation,
                None,
            )?),
            None => None,
        };
        let root_field_args = root_field_args(root_type.as_ref(), &root_field_name)?;
        for (arg_name, arg_value) in args.own_keys() {
            let arg_name = String::from_utf8_lossy(arg_name.as_wtf8()).into_owned();
            arguments.add(
                &arg_name,
                arg_value,
                root_field_args.as_deref(),
                field_node.map(Rc::as_ref),
                &root_field_name,
                info,
            )?;
        }
    }
    let rootfield_node = FieldNode {
        loc: None,
        alias: None,
        name: NameNode::new(None, root_field_name),
        arguments: Some(arguments.arg_nodes),
        directives: field_node.and_then(|node| node.directives.clone()),
        selection_set: new_selection_set,
    };
    let document = request_document(
        options.target_operation,
        options.target_operation_name.as_deref(),
        arguments.variable_definitions,
        rootfield_node,
    );
    Ok(ExecutionRequest {
        document: Rc::new(document),
        fragments: options.fragments.filter(|fragments| !fragments.is_empty()),
        variables: Some(arguments.new_variables),
        operation_name: options.target_operation_name,
        extensions: None,
        context: options.context,
        info: options.info,
        root_value: options.root_value,
        uploads: Vec::new(),
        subgraph_name: options.subgraph_name,
        operation_type: Some(options.target_operation),
    })
}

/// The variables, the variable definitions and the argument nodes a request collects, one argument at a time.
struct Arguments {
    /// The variable values, `newVariables`.
    new_variables: Variables,
    /// The variable definitions, `variableDefinitions`.
    variable_definitions: Vec<VariableDefinitionNode>,
    /// The argument nodes of the root field, `argNodes`.
    arg_nodes: Vec<ArgumentNode>,
}

impl Arguments {
    /// Adds the argument `arg_name` of value `arg_value`, the body of the loop at `index.cjs:1833-1905`.
    fn add(
        &mut self,
        arg_name: &str,
        arg_value: &JsValue,
        root_field_args: Option<&[aosabi_type::Argument]>,
        field_node: Option<&FieldNode>,
        root_field_name: &str,
        info: Option<&ResolveInfo>,
    ) -> Result<(), Value> {
        let arg_instance =
            root_field_args.and_then(|args| args.iter().find(|arg| arg.name == arg_name));
        let existing_arg_node = field_node
            .and_then(|node| node.arguments.as_ref())
            .and_then(|arguments| arguments.iter().find(|arg| arg.name.value() == arg_name));
        if let Some(existing_arg_node) = existing_arg_node
            && let ValueNode::Variable(variable) = &existing_arg_node.value
        {
            let var_value = self
                .new_variables
                .get(variable.name.value())
                .cloned()
                .unwrap_or(JsValue::Undefined);
            if strict_equals(&var_value, arg_value) {
                self.arg_nodes.push(existing_arg_node.clone());
                return Ok(());
            }
        }
        let Some(arg_instance) = arg_instance else {
            if let Some(value_node) = ast_from_value_untyped(arg_value)? {
                self.arg_nodes.push(ArgumentNode {
                    loc: None,
                    name: NameNode::new(None, arg_name),
                    value: value_node,
                });
            }
            return Ok(());
        };
        let var_name =
            fresh_variable_name(arg_name, root_field_name, &self.variable_definitions, info);
        self.variable_definitions.push(VariableDefinitionNode {
            description: None,
            loc: None,
            variable: variable_node(&var_name),
            r#type: ast_from_type(&arg_instance.r#type).map_err(thrown_definition_error)?,
            default_value: None,
            directives: None,
        });
        let var_value = project_argument_value(arg_value, &arg_instance.r#type)?;
        if !var_value.is_undefined() {
            self.new_variables.insert(&var_name, var_value);
        }
        self.arg_nodes.push(ArgumentNode {
            loc: None,
            name: NameNode::new(None, arg_name),
            value: ValueNode::Variable(variable_node(&var_name)),
        });
        Ok(())
    }
}

/// Returns the selections of every field node in one selection set, `None` when there is none.
fn selections_of(field_nodes: &[Rc<FieldNode>]) -> Option<SelectionSetNode> {
    let selections: Vec<SelectionNode> = field_nodes
        .iter()
        .filter_map(|field_node2| field_node2.selection_set.as_ref())
        .flat_map(|set| set.selections.iter().cloned())
        .collect();
    (!selections.is_empty()).then_some(SelectionSetNode {
        loc: None,
        selections: SharedSlice::from(selections),
    })
}

/// Returns the document of one operation over the root field.
///
/// The fragments of the client follow it in the member `fragments` of the
/// request.
fn request_document(
    target_operation: OperationTypeNode,
    target_operation_name: Option<&str>,
    variable_definitions: Vec<VariableDefinitionNode>,
    rootfield_node: FieldNode,
) -> DocumentNode {
    let operation_definition = OperationDefinitionNode {
        description: None,
        loc: None,
        operation: target_operation,
        name: target_operation_name
            .filter(|name| !name.is_empty())
            .map(|name| NameNode::new(None, name)),
        variable_definitions: Some(variable_definitions),
        directives: None,
        selection_set: SelectionSetNode {
            loc: None,
            selections: SharedSlice::from([SelectionNode::Field(rootfield_node)]),
        },
    };
    DocumentNode {
        loc: None,
        definitions: vec![DefinitionNode::OperationDefinition(operation_definition)],
        token_count: None,
    }
}

/// Returns an argument's value as the variable of the target's argument type carries it.
///
/// A null stays as it is, a list type maps each item of `asArray` of the
/// value, which is the value itself for a list, the value wrapped in a list
/// when it is truthy and an empty list when it is falsy, an input object keeps
/// the fields its type declares and drops the undefined ones, `Boolean`,
/// `Int`, `Float` and `String` convert the value as the functions of those
/// names do, and any other type keeps it. A host object carried through
/// unread, the file of an upload, projects to an input object with no field.
/// The reference walks a `File` with `for...in`, which yields `name`,
/// `lastModified`, `type` and `size` and then the methods of a `Blob`, so it
/// copies those of the four members the input object type declares. No root
/// field of a federation pair takes an input object where a file stands.
///
/// # Errors
///
/// Returns what the field thunk of an input object type throws.
pub fn project_argument_value(arg_value: &JsValue, arg_type: &Type) -> Result<JsValue, Value> {
    if matches!(arg_value, JsValue::Undefined | JsValue::Null) {
        return Ok(arg_value.clone());
    }
    match arg_type {
        Type::NonNull(non_null) => project_argument_value(arg_value, non_null.of_type()),
        Type::List(list) => {
            let items = match arg_value {
                JsValue::Array(items) => items.clone(),
                other if to_boolean(other) => vec![other.clone()],
                _ => Vec::new(),
            };
            let mut projected = Vec::with_capacity(items.len());
            for item in &items {
                projected.push(project_argument_value(item, &list.of_type)?);
            }
            Ok(JsValue::Array(projected))
        }
        Type::InputObject(input_object)
            if matches!(
                arg_value,
                JsValue::Object(_) | JsValue::Array(_) | JsValue::Opaque(_)
            ) =>
        {
            let fields = input_object.get_fields().map_err(thrown_definition_error)?;
            let mut projected_value = JsObject::new();
            if let JsValue::Object(object) = arg_value {
                for (key, value) in object.own_keys() {
                    let key = String::from_utf8_lossy(key.as_wtf8()).into_owned();
                    if let Some(field) = fields.get(&key) {
                        let var_value = project_argument_value(value, &field.r#type)?;
                        if !var_value.is_undefined() {
                            projected_value.insert(key, var_value);
                        }
                    }
                }
            }
            Ok(JsValue::Object(projected_value))
        }
        named => Ok(match named.as_named().as_ref().map(NamedType::name) {
            Some("Boolean") => JsValue::Bool(to_boolean(arg_value)),
            Some("Int" | "Float") => JsValue::Number(to_number(arg_value)),
            Some("String") => JsValue::String(to_js_string_of(arg_value)),
            _ => arg_value.clone(),
        }),
    }
}

/// Returns the arguments of the target's root field `name`, `rootType?.getFields()[name]?.args`.
fn root_field_args(
    root_type: Option<&NamedType>,
    name: &str,
) -> Result<Option<Vec<aosabi_type::Argument>>, Value> {
    let fields = match root_type {
        Some(NamedType::Object(object)) => object.get_fields(),
        Some(NamedType::Interface(interface)) => interface.get_fields(),
        _ => return Ok(None),
    };
    Ok(fields
        .map_err(thrown_definition_error)?
        .get(name)
        .map(|field| field.args.clone()))
}

/// Returns the name of a fresh variable for `arg_name`, renamed while the name is taken.
///
/// A name is taken when a variable definition holds it or when the client's
/// variables hold a value that is no null under it, which keeps a variable of
/// the client that has nothing to do with this argument.
fn fresh_variable_name(
    arg_name: &str,
    root_field_name: &str,
    variable_definitions: &[VariableDefinitionNode],
    info: Option<&ResolveInfo>,
) -> String {
    let var_exists = |var_name2: &str| {
        variable_definitions
            .iter()
            .any(|var_def| var_def.variable.name.value() == var_name2)
            || info
                .and_then(|info| info.variable_values.get(var_name2))
                .is_some_and(|value| !matches!(value, JsValue::Undefined | JsValue::Null))
    };
    let mut var_name = String::from(arg_name);
    if var_exists(&var_name) {
        var_name = format!("_{root_field_name}_{arg_name}");
        let mut i = 0_usize;
        while var_exists(&var_name) {
            var_name = format!("_{i}_{root_field_name}_{arg_name}");
            i += 1;
        }
    }
    var_name
}

/// Returns the variable node of `name`.
fn variable_node(name: &str) -> VariableNode {
    VariableNode {
        loc: None,
        name: NameNode::new(None, name),
    }
}

/// Returns `a === b` over two values of the JSON model.
///
/// An opaque value equals the same host object, and an object or a list of
/// the model, which has no identity, equals nothing.
fn strict_equals(a: &JsValue, b: &JsValue) -> bool {
    match (a, b) {
        (JsValue::Undefined, JsValue::Undefined) | (JsValue::Null, JsValue::Null) => true,
        (JsValue::Bool(a), JsValue::Bool(b)) => a == b,
        #[expect(
            clippy::float_cmp,
            reason = "the `===` of two numbers compares them exactly"
        )]
        (JsValue::Number(a), JsValue::Number(b)) => a == b,
        (JsValue::String(a), JsValue::String(b)) => a == b,
        (JsValue::Opaque(a), JsValue::Opaque(b)) => a == b,
        _ => false,
    }
}

/// Returns `Boolean(value)`, ECMA-262 `ToBoolean`.
fn to_boolean(value: &JsValue) -> bool {
    match value {
        JsValue::Undefined | JsValue::Null => false,
        JsValue::Bool(boolean) => *boolean,
        JsValue::Number(number) => *number != 0.0 && !number.is_nan(),
        JsValue::String(string) => !string.is_empty(),
        JsValue::Array(_) | JsValue::Object(_) | JsValue::Opaque(_) => true,
    }
}

/// Returns `Number(value)`, ECMA-262 `ToNumber`, an object read through its text.
fn to_number(value: &JsValue) -> f64 {
    match value {
        JsValue::Undefined => f64::NAN,
        JsValue::Null => 0.0,
        JsValue::Bool(boolean) => f64::from(u8::from(*boolean)),
        JsValue::Number(number) => *number,
        other => string_to_number(&to_string_lossy(&to_js_string_of(other))),
    }
}

/// Returns `String(value)`, ECMA-262 `ToString`.
fn to_js_string_of(value: &JsValue) -> JsString {
    to_js_string(&Value::from_js(value))
}
