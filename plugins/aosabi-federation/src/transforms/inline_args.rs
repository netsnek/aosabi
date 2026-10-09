// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-federation: the transform that writes the arguments of a delegation
 * into its document as literals.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/core/gateway.ts,
 * Apache-2.0. Changed by the Aosabi authors.
 */

//! The transform that inlines the arguments, [`InlineArgsTransform`].
//!
//! The transform writes the arguments the delegating resolver passes into the
//! root field as literals, each replacing an argument of the same name,
//! removes every argument and input object field whose variable is absent or
//! `undefined`, writes every other variable as the literal of its value and
//! empties the variable definitions. A pass with the type information of the
//! target schema then writes a string literal that stands at a position of an
//! enum type as an enum value, and declares every file the literals lifted,
//! the variable `_aosabiUpload_<n>` of [`ast_from_js_value`], with the input
//! type of the position it stands at. The files are the only variables the
//! request keeps, so a request without a file carries an empty variable
//! object.
//!
//! The root field is the first field the walk meets that bears the name of
//! the delegated field. A variable is present when the variables hold it as
//! an own member, where the reference asks the operator `in`, which also
//! finds a member of `Object.prototype`, so a variable named `constructor` or
//! `toString` that the request leaves out is removed here and written as the
//! text of that member there.

use std::{
    cell::RefCell,
    rc::Rc, //
};

use aosabi_core::Thrown;
use aosabi_execute::{
    errors::thrown_definition_error,
    value::Value, //
};
use aosabi_json::{
    JsObject,
    JsValue, //
};
use aosabi_lang::{
    ast::{
        ArgumentNode,
        DocumentNode,
        EnumValueNode,
        FieldNode,
        NameNode,
        OperationDefinitionNode,
        ValueNode,
        VariableDefinitionNode,
        VariableNode, //
    },
    js_string::to_string_lossy,
    visitor::NodeRef,
    visitor_edit::{
        Edit,
        EditContext,
        EditVisitor,
        visit_edit, //
    }, //
};
use aosabi_type::{
    NamedType,
    Schema,
    Type,
    get_named_type,
    type_info::TypeInfo,
    type_info_edit::visit_with_type_info_edit,
    wrapping::ast_from_type, //
};

use crate::{
    ast_from_js::{
        UploadCollector,
        ast_from_js_value, //
    },
    transforms::{
        DelegationContext,
        ExecutionRequest,
        Transform,
        Variables, //
    }, //
};

/// The transform that writes the arguments of a delegation into its document as literals, `InlineArgsTransform`.
#[derive(Clone, Debug)]
pub struct InlineArgsTransform {
    /// The arguments the delegating resolver passes, `wrapperArgs`.
    wrapper_args: Option<JsObject>,
}

impl InlineArgsTransform {
    /// Returns the transform of the arguments `wrapper_args`.
    #[must_use]
    pub const fn new(wrapper_args: Option<JsObject>) -> Self {
        Self { wrapper_args }
    }

    /// Returns the argument nodes of the wrapper's arguments that are not `undefined`, lifting their files into `uploads`.
    fn root_inline_arguments(&self, uploads: &mut UploadCollector) -> Vec<ArgumentNode> {
        let Some(wrapper_args) = self.wrapper_args.as_ref().filter(|args| !args.is_empty()) else {
            return Vec::new();
        };
        wrapper_args
            .own_keys()
            .filter(|(_, value)| !value.is_undefined())
            .map(|(key, value)| ArgumentNode {
                loc: None,
                name: NameNode::new(None, to_string_lossy(key)),
                value: ast_from_js_value(value, Some(&mut *uploads)),
            })
            .collect()
    }
}

impl Transform for InlineArgsTransform {
    /// Returns the request with its arguments and its variables written as literals and its files lifted.
    ///
    /// The variables of the answer are the lifted files keyed by their
    /// variable names, `Object.fromEntries(uploads)`, which the request also
    /// carries as its uploads.
    ///
    /// # Errors
    ///
    /// Returns the error of a type the type information cannot read in the
    /// target schema.
    fn transform_request(
        &self,
        original_request: ExecutionRequest,
        delegation_context: &DelegationContext,
    ) -> Result<ExecutionRequest, Thrown> {
        let mut uploads = UploadCollector::new();
        let root_inline_arguments = self.root_inline_arguments(&mut uploads);
        let no_variables = Variables::default();
        let mut inline = InlineVariables {
            root_inline_arguments: &root_inline_arguments,
            target_field_name: &delegation_context.field_name,
            target_field_found: false,
            variables: original_request.variables.as_ref().unwrap_or(&no_variables),
            uploads: &mut uploads,
        };
        let inline_document = visit_edit(&original_request.document, &mut inline)
            .map_err(|error| Thrown::error(&error.to_string()))?;
        let (mut inline_document, upload_defs) = type_the_literals(
            &inline_document,
            &delegation_context.target_schema,
            &uploads,
        )?;
        if !upload_defs.is_empty() {
            let mut declare = DeclareUploads {
                upload_defs: &upload_defs,
            };
            inline_document = visit_edit(&inline_document, &mut declare)
                .map_err(|error| Thrown::error(&error.to_string()))?;
        }
        let variables: JsObject = uploads
            .iter()
            .map(|(name, file)| (name, file.clone()))
            .collect();
        let lifted = uploads
            .iter()
            .map(|(name, file)| (String::from(name), Value::from_js(file)))
            .collect();
        Ok(ExecutionRequest {
            document: Rc::new(inline_document),
            variables: Some(Variables::from(variables)),
            uploads: lifted,
            ..original_request
        })
    }
}

/// Returns the document with every string at an enum position written as an enum value, and the definitions of the lifted files.
///
/// A file is declared once per place it stands at, in the order of the walk,
/// with the input type of that place, and a file at a place the target schema
/// types with nothing stays undeclared.
fn type_the_literals(
    document: &DocumentNode,
    target_schema: &Schema,
    uploads: &UploadCollector,
) -> Result<(DocumentNode, Vec<VariableDefinitionNode>), Thrown> {
    let type_info = RefCell::new(TypeInfo::new(target_schema, None, None));
    let mut typed = TypedLiterals {
        type_info: &type_info,
        uploads,
        upload_defs: Vec::new(),
        error: None,
    };
    let mut visitor = visit_with_type_info_edit(&type_info, &mut typed);
    let walked = visit_edit(document, &mut visitor);
    if let Some(error) = visitor.into_error() {
        return Err(Thrown(thrown_definition_error(error)));
    }
    if let Some(error) = typed.error.take() {
        return Err(Thrown(error));
    }
    let document = walked.map_err(|error| Thrown::error(&error.to_string()))?;
    Ok((document, typed.upload_defs))
}

/// Returns a copy of an operation with `variable_definitions` in place of its own.
fn with_variable_definitions(
    node: &OperationDefinitionNode,
    variable_definitions: Vec<VariableDefinitionNode>,
) -> OperationDefinitionNode {
    OperationDefinitionNode {
        description: node.description.clone(),
        loc: node.loc.clone(),
        operation: node.operation,
        name: node.name.clone(),
        variable_definitions: Some(variable_definitions),
        directives: node.directives.clone(),
        selection_set: node.selection_set.clone(),
    }
}

/// The visitor of the first pass, which inlines the arguments and the variables.
struct InlineVariables<'a> {
    /// The argument nodes of the wrapper's arguments, `rootInlineArguments`.
    root_inline_arguments: &'a [ArgumentNode],
    /// The name of the delegated field, `targetFieldName`.
    target_field_name: &'a str,
    /// Whether the root field has been met, `targetFieldFound`.
    target_field_found: bool,
    /// The variables of the request, `variables`.
    variables: &'a Variables,
    /// The files lifted so far, `uploads`.
    uploads: &'a mut UploadCollector,
}

impl InlineVariables<'_> {
    /// Injects the wrapper's arguments into the root field, step A.
    fn inject(&mut self, node: &FieldNode) -> Edit {
        if self.target_field_found || node.name.value() != self.target_field_name {
            return Edit::Keep;
        }
        self.target_field_found = true;
        let mut merged_args = node.arguments.clone().unwrap_or_default();
        for new_arg in self.root_inline_arguments {
            match merged_args
                .iter_mut()
                .find(|arg| arg.name.value() == new_arg.name.value())
            {
                Some(existing) => *existing = new_arg.clone(),
                None => merged_args.push(new_arg.clone()),
            }
        }
        Edit::replace(FieldNode {
            loc: node.loc.clone(),
            alias: node.alias.clone(),
            name: node.name.clone(),
            arguments: Some(merged_args),
            directives: node.directives.clone(),
            selection_set: node.selection_set.clone(),
        })
    }

    /// Removes an argument or an input object field whose variable is absent or `undefined`, steps B and C.
    fn prune(&self, value: &ValueNode) -> Edit {
        let ValueNode::Variable(variable) = value else {
            return Edit::Keep;
        };
        let var_name = variable.name.value();
        if self.uploads.has(var_name) {
            return Edit::Keep;
        }
        match self.variables.get(var_name) {
            None | Some(JsValue::Undefined) => Edit::Remove,
            Some(_) => Edit::Keep,
        }
    }

    /// Writes a variable as the literal of its value, step D.
    ///
    /// A variable this transform made for a file stays, because the request
    /// carries no value for it and inlining it would write `null` over the
    /// upload.
    fn inline(&mut self, node: &VariableNode) -> Edit {
        let var_name = node.name.value();
        if self.uploads.has(var_name) {
            return Edit::Keep;
        }
        let undefined = JsValue::Undefined;
        let value = self.variables.get(var_name).unwrap_or(&undefined);
        Edit::replace(ast_from_js_value(value, Some(&mut *self.uploads)))
    }
}

impl EditVisitor for InlineVariables<'_> {
    fn enter(&mut self, node: NodeRef<'_>, _context: &EditContext<'_>) -> Edit {
        match node {
            NodeRef::Field(node) => self.inject(node),
            NodeRef::Argument(node) => self.prune(&node.value),
            NodeRef::ObjectField(node) => self.prune(&node.value),
            NodeRef::Variable(node) => self.inline(node),
            // Step E, the definitions of the lifted files come back once the
            // pass with type information knows their types.
            NodeRef::OperationDefinition(node) => {
                Edit::replace(with_variable_definitions(node, Vec::new()))
            }
            _ => Edit::Keep,
        }
    }
}

/// The visitor of the pass with type information, which writes enum values and declares the files.
struct TypedLiterals<'a, 's> {
    /// The type information the walk moves.
    type_info: &'a RefCell<TypeInfo<'s>>,
    /// The files the first pass lifted.
    uploads: &'a UploadCollector,
    /// The definitions of the lifted files, `uploadDefs`.
    upload_defs: Vec<VariableDefinitionNode>,
    /// The error of a type no definition can be written for.
    error: Option<Value>,
}

impl TypedLiterals<'_, '_> {
    /// Returns the input type of the visited position, `typeInfo.getInputType()`.
    fn input_type(&self) -> Option<Type> {
        self.type_info
            .try_borrow()
            .ok()
            .and_then(|type_info| type_info.get_input_type().cloned())
    }

    /// Declares a lifted file with the input type of its position.
    fn declare(&mut self, node: &VariableNode) -> Edit {
        if !self.uploads.has(node.name.value()) {
            return Edit::Keep;
        }
        let Some(input_type) = self.input_type() else {
            return Edit::Keep;
        };
        match ast_from_type(&input_type) {
            Ok(r#type) => {
                self.upload_defs.push(VariableDefinitionNode {
                    description: None,
                    loc: None,
                    variable: node.clone(),
                    r#type,
                    default_value: None,
                    directives: None,
                });
                Edit::Keep
            }
            Err(error) => {
                self.error = Some(thrown_definition_error(error));
                Edit::Break
            }
        }
    }
}

impl EditVisitor for TypedLiterals<'_, '_> {
    fn enter(&mut self, node: NodeRef<'_>, _context: &EditContext<'_>) -> Edit {
        match node {
            NodeRef::StringValue(node) => {
                let is_enum = self.input_type().is_some_and(|input_type| {
                    matches!(get_named_type(&input_type), NamedType::Enum(_))
                });
                if is_enum {
                    Edit::replace(EnumValueNode {
                        loc: None,
                        value: node.value.clone(),
                    })
                } else {
                    Edit::Keep
                }
            }
            NodeRef::Variable(node) => self.declare(node),
            _ => Edit::Keep,
        }
    }
}

/// The visitor of the last pass, which declares the lifted files on every operation.
struct DeclareUploads<'a> {
    /// The definitions of the lifted files.
    upload_defs: &'a [VariableDefinitionNode],
}

impl EditVisitor for DeclareUploads<'_> {
    fn enter(&mut self, node: NodeRef<'_>, _context: &EditContext<'_>) -> Edit {
        match node {
            NodeRef::OperationDefinition(node) => {
                Edit::replace(with_variable_definitions(node, self.upload_defs.to_vec()))
            }
            _ => Edit::Keep,
        }
    }
}
