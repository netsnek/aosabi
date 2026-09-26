// SPDX-License-Identifier: MIT
/*
 * aosabi-validate: the rule `PossibleTypeExtensionsRule`.
 *
 * Derived from graphql-js 16.12.0, src/validation/rules/PossibleTypeExtensionsRule.ts, MIT.
 */

//! The `PossibleTypeExtensionsRule` of the validation layer.

use alloc::{
    boxed::Box,
    format,
    string::String,
    vec,
    vec::Vec, //
};

use aosabi_json::JsValue;
use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions,
    Kind,
    ast::{
        AstNode,
        DefinitionNode,
        EnumTypeExtensionNode,
        InputObjectTypeExtensionNode,
        InterfaceTypeExtensionNode,
        NameNode,
        ObjectTypeExtensionNode,
        ScalarTypeExtensionNode,
        UnionTypeExtensionNode, //
    },
    predicates::is_type_definition_node,
    visitor::{
        EnterLeave,
        NodeRef,
        VisitContext,
        VisitResult,
        Visitor, //
    }, //
};
use aosabi_type::{
    DefinitionError,
    NamedType,
    ObjMap,
    Schema,
    did_you_mean,
    inspect,
    suggestion_list, //
};

use crate::{
    rules::known_type_names_rule::type_definition_name,
    validation_context::{
        Rule,
        SdlValidationContext, //
    }, //
};

/// Possible type extension
///
/// A type extension is only valid if the type is defined and has the same kind.
///
/// The six entries of the visitor are one function, `checkExtension`, which
/// is the private method `check_extension` here.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/PossibleTypeExtensionsRule.ts, MIT.
pub struct PossibleTypeExtensionsRule<'c, 'a> {
    /// The context the rule reads and reports to.
    context: &'c SdlValidationContext<'a>,
    /// The schema the document extends, the `schema` of the reference.
    schema: Option<&'a Schema>,
    /// The type definitions of the document by name, the `definedTypes` of
    /// the reference.
    ///
    /// The reference assigns to a property of a null prototype object, so of
    /// two definitions with one name the later one is the value and the
    /// earlier one keeps the position, which is what an insertion into this
    /// map does.
    defined_types: ObjMap<&'a DefinitionNode>,
}

impl<'c, 'a> Rule<'c, 'a> for PossibleTypeExtensionsRule<'c, 'a> {
    type Context = &'c SdlValidationContext<'a>;

    fn new(context: Self::Context) -> Self {
        let schema = context.get_schema();
        let mut defined_types = ObjMap::default();

        for def in &context.get_document().definitions {
            if is_type_definition_node(def.into())
                && let Some(name) = type_definition_name(def)
            {
                defined_types.insert(String::from(name.value()), def);
            }
        }

        Self {
            context,
            schema,
            defined_types,
        }
    }
}

impl<'a> PossibleTypeExtensionsRule<'_, 'a> {
    /// Reports an extension of a type that is not defined or that has
    /// another kind.
    ///
    /// `node` is the extension and `name` is its `name` property, which
    /// every type extension node carries.
    ///
    /// Derived from graphql-js 16.12.0, src/validation/rules/PossibleTypeExtensionsRule.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the invariant of `extensionKindToTypeName`, which no node a
    /// hook of this rule passes reaches.
    fn check_extension<N>(&self, node: &'a N, name: &NameNode) -> Result<(), DefinitionError>
    where
        N: Clone + Into<AstNode>,
        &'a N: Into<NodeRef<'a>>,
    {
        let type_name = name.value();
        let def_node = self.defined_types.get(type_name).copied();
        let existing_type = self.schema.and_then(|schema| schema.get_type(type_name));

        let mut expected_kind = None;
        if let Some(def_node) = def_node {
            expected_kind = def_kind_to_ext_kind(def_node.kind());
        } else if let Some(existing_type) = existing_type {
            expected_kind = Some(type_to_ext_kind(existing_type));
        }

        let Some(expected_kind) = expected_kind else {
            self.report_undefined_type(type_name, name);
            return Ok(());
        };
        let kind = node.into().kind();
        if expected_kind != kind {
            let kind_str = extension_kind_to_type_name(kind)?;
            let nodes = match def_node {
                Some(def_node) => vec![AstNode::from(def_node.clone()), node.clone().into()],
                None => vec![node.clone().into()],
            };
            self.context.report_error(GraphQLError::new(
                format!("Cannot extend non-{kind_str} type \"{type_name}\"."),
                GraphQLErrorOptions {
                    nodes: Some(nodes),
                    ..GraphQLErrorOptions::default()
                },
            ));
        }
        Ok(())
    }

    /// Reports an extension of a type that neither the document nor the
    /// schema defines, with the names that come close.
    ///
    /// `type_name` is the name the extension gives and `name` is the node
    /// that carries it, which the error blames.
    ///
    /// Derived from graphql-js 16.12.0, src/validation/rules/PossibleTypeExtensionsRule.ts, MIT.
    ///
    /// The reference spreads `definedTypes` and then the type map of the
    /// schema into one object and takes its keys. A spread defines each
    /// property in turn, so a name of both keeps the position `definedTypes`
    /// gave it, and the keys come in insertion order, because a name starts
    /// with a letter or an underscore and is never an array index.
    fn report_undefined_type(&self, type_name: &str, name: &NameNode) {
        let mut all_types: ObjMap<()> = ObjMap::default();
        for defined_name in self.defined_types.keys() {
            all_types.insert(defined_name.clone(), ());
        }
        if let Some(schema) = self.schema {
            for schema_name in schema.get_type_map().keys() {
                all_types.insert(schema_name.clone(), ());
            }
        }
        let all_type_names: Vec<String> = all_types.into_keys().collect();

        let suggested_types = suggestion_list(type_name, &all_type_names);
        self.context.report_error(GraphQLError::new(
            format!("Cannot extend type \"{type_name}\" because it is not defined.")
                + &did_you_mean(None, &suggested_types),
            GraphQLErrorOptions {
                nodes: Some(vec![AstNode::from(name.clone())]),
                ..GraphQLErrorOptions::default()
            },
        ));
    }

    /// Runs `checkExtension` as a hook, where an error it returns is the
    /// exception of the reference.
    fn enter_extension<N>(&self, node: &'a N, name: &NameNode) -> VisitResult
    where
        N: Clone + Into<AstNode>,
        &'a N: Into<NodeRef<'a>>,
    {
        match self.check_extension(node, name) {
            Ok(()) => VisitResult::Continue,
            Err(error) => self.context.throw_error(error),
        }
    }
}

/// Returns the kind of the extension that extends a definition of `kind`.
///
/// `None` is the `undefined` a kind outside the object reads.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/PossibleTypeExtensionsRule.ts, MIT.
const fn def_kind_to_ext_kind(kind: Kind) -> Option<Kind> {
    match kind {
        Kind::ScalarTypeDefinition => Some(Kind::ScalarTypeExtension),
        Kind::ObjectTypeDefinition => Some(Kind::ObjectTypeExtension),
        Kind::InterfaceTypeDefinition => Some(Kind::InterfaceTypeExtension),
        Kind::UnionTypeDefinition => Some(Kind::UnionTypeExtension),
        Kind::EnumTypeDefinition => Some(Kind::EnumTypeExtension),
        Kind::InputObjectTypeDefinition => Some(Kind::InputObjectTypeExtension),
        _ => None,
    }
}

/// Returns the kind of the extension that extends `type`.
///
/// The six variants are the whole enum, so the invariant of the reference
/// has no reading.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/PossibleTypeExtensionsRule.ts, MIT.
const fn type_to_ext_kind(r#type: &NamedType) -> Kind {
    match r#type {
        NamedType::Scalar(_) => Kind::ScalarTypeExtension,
        NamedType::Object(_) => Kind::ObjectTypeExtension,
        NamedType::Interface(_) => Kind::InterfaceTypeExtension,
        NamedType::Union(_) => Kind::UnionTypeExtension,
        NamedType::Enum(_) => Kind::EnumTypeExtension,
        NamedType::InputObject(_) => Kind::InputObjectTypeExtension,
    }
}

/// Returns the word a message uses for the type an extension of `kind`
/// extends.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/PossibleTypeExtensionsRule.ts, MIT.
///
/// # Errors
///
/// Returns the invariant of the reference for a kind that is no type
/// extension, a plain `Error` whose message is the literal of line 141 and
/// the `inspect` of the kind.
fn extension_kind_to_type_name(kind: Kind) -> Result<&'static str, DefinitionError> {
    match kind {
        Kind::ScalarTypeExtension => Ok("scalar"),
        Kind::ObjectTypeExtension => Ok("object"),
        Kind::InterfaceTypeExtension => Ok("interface"),
        Kind::UnionTypeExtension => Ok("union"),
        Kind::EnumTypeExtension => Ok("enum"),
        Kind::InputObjectTypeExtension => Ok("input object"),
        // Not reachable. All possible types have been considered
        _ => Err(DefinitionError::plain(
            String::from("Unexpected kind: ") + &inspect(&JsValue::from(kind.as_str())),
        )),
    }
}

impl<'a> Visitor<'a> for PossibleTypeExtensionsRule<'_, 'a> {
    fn enter_scalar_type_extension(
        &mut self,
        node: &'a ScalarTypeExtensionNode,
        _: &VisitContext<'a, '_>,
    ) -> VisitResult {
        self.enter_extension(node, &node.name)
    }

    fn enter_object_type_extension(
        &mut self,
        node: &'a ObjectTypeExtensionNode,
        _: &VisitContext<'a, '_>,
    ) -> VisitResult {
        self.enter_extension(node, &node.name)
    }

    fn enter_interface_type_extension(
        &mut self,
        node: &'a InterfaceTypeExtensionNode,
        _: &VisitContext<'a, '_>,
    ) -> VisitResult {
        self.enter_extension(node, &node.name)
    }

    fn enter_union_type_extension(
        &mut self,
        node: &'a UnionTypeExtensionNode,
        _: &VisitContext<'a, '_>,
    ) -> VisitResult {
        self.enter_extension(node, &node.name)
    }

    fn enter_enum_type_extension(
        &mut self,
        node: &'a EnumTypeExtensionNode,
        _: &VisitContext<'a, '_>,
    ) -> VisitResult {
        self.enter_extension(node, &node.name)
    }

    fn enter_input_object_type_extension(
        &mut self,
        node: &'a InputObjectTypeExtensionNode,
        _: &VisitContext<'a, '_>,
    ) -> VisitResult {
        self.enter_extension(node, &node.name)
    }
}

/// Creates the visitor of `PossibleTypeExtensionsRule` over `context`.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/PossibleTypeExtensionsRule.ts, MIT.
#[must_use]
pub fn possible_type_extensions_rule<'c, 'a>(
    context: &'c SdlValidationContext<'a>,
) -> Box<dyn EnterLeave<'a> + 'c> {
    PossibleTypeExtensionsRule::boxed(context)
}
