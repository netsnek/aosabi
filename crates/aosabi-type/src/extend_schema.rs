// SPDX-License-Identifier: MIT
/*
 * aosabi-type: the extension of a schema by the definitions and extensions of
 * a document, and the builders that turn type system nodes into definitions.
 *
 * Derived from graphql-js 16.12.0, src/utilities/extendSchema.ts, MIT.
 * Derived from graphql-js 16.12.0, src/execution/values.ts, MIT.
 */

//! A schema extended by a document.
//!
//! `extendSchemaImpl` of the reference is one function whose builders close
//! over the type map, the extension map and the standard type map. Those
//! builders run inside the thunks of the types they build, after the function
//! has returned, so the port keeps that scope in a shared `Scope` that every
//! thunk holds, and the type map lands in a race cell once every named type of
//! the document exists, because a thunk of a type refers to types that are
//! built after it.
//!
//! # Slots of the wrong kind
//!
//! The reference puts whatever named type a node names into a root slot, an
//! interface list or a union member list, and `validateSchema` reports the
//! wrong kind afterwards. The configs of this crate hold a [`NamedType`] in
//! each of those slots, so the builders make no assertion either, and the wrong
//! kind is one entry among the others of the validation.
//!
//! An extension of one kind applied to a type of another kind is rejected by
//! the SDL validation before the builders run. Under `assume_valid_sdl` the
//! reference reads the members the extension kind carries, and the port reads
//! only the members of the matching kind, because a field config cannot hold an
//! input value node as its node.

use alloc::{
    boxed::Box,
    format,
    string::{
        String,
        ToString, //
    },
    sync::Arc,
    vec::Vec, //
};

use aosabi_json::{
    JsObject,
    JsValue, //
};
use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions,
    ast::{
        ArgumentNode,
        AstNode,
        DefinitionNode,
        DirectiveDefinitionNode,
        DirectiveNode,
        DocumentNode,
        EnumTypeDefinitionNode,
        EnumTypeExtensionNode,
        EnumValueDefinitionNode,
        FieldDefinitionNode,
        InputObjectTypeDefinitionNode,
        InputObjectTypeExtensionNode,
        InputValueDefinitionNode,
        InterfaceTypeDefinitionNode,
        InterfaceTypeExtensionNode,
        NamedTypeNode,
        ObjectTypeDefinitionNode,
        ObjectTypeExtensionNode,
        OperationTypeDefinitionNode,
        OperationTypeNode,
        ScalarTypeDefinitionNode,
        ScalarTypeExtensionNode,
        SchemaDefinitionNode,
        SchemaExtensionNode,
        StringValueNode,
        TypeDefinitionNode,
        TypeExtensionNode,
        TypeNode,
        UnionTypeDefinitionNode,
        UnionTypeExtensionNode,
        ValueNode, //
    },
    print, //
};
use once_cell::race::OnceBox;

use crate::{
    definition::{
        ArgumentConfig,
        DefinitionError,
        EnumType,
        EnumTypeConfig,
        EnumValueConfig,
        FieldConfig,
        InputFieldConfig,
        InputObjectType,
        InputObjectTypeConfig,
        InterfaceType,
        InterfaceTypeConfig,
        ListType,
        NamedType,
        NonNullType,
        ObjMap,
        ObjectType,
        ObjectTypeConfig,
        ScalarType,
        ScalarTypeConfig,
        ThunkObjMap,
        ThunkReadonlyArray,
        Type,
        UnionType,
        UnionTypeConfig,
        blame,
        get_named_type,
        is_non_null_type,
        resolve_obj_map_thunk, //
    },
    directives::{
        Directive,
        DirectiveConfig,
        DirectiveLocation,
        graphql_deprecated_directive,
        graphql_one_of_directive,
        graphql_specified_by_directive,
        specified_directives, //
    },
    introspection::{
        introspection_schema,
        introspection_types,
        is_introspection_type, //
    },
    scalars::{
        is_specified_scalar_type,
        specified_scalar_types, //
    },
    schema::{
        Schema,
        SchemaConfig, //
    },
    schema_bindings::SchemaBindings,
    value_from_ast::value_from_ast, //
};

/// The options of `extendSchema`.
///
/// Extends `GraphQLSchemaValidationOptions` with `assumeValidSDL`. Both default
/// to false.
///
/// Derived from graphql-js 16.12.0, src/utilities/extendSchema.ts, MIT.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExtendSchemaOptions {
    /// When true, no type system validation runs on the result.
    pub assume_valid: bool,
    /// When true, the SDL is not validated before it is built.
    pub assume_valid_sdl: bool,
}

/// The SDL validation of an extension document, which the validation crate
/// provides.
///
/// `assertValidSDLExtension` validates the document against the schema and
/// throws a plain `Error` whose message joins every validation message with a
/// blank line. The crate that owns the rules hands that function in, and this
/// crate calls it before it builds.
pub type AssertValidSdlExtension = fn(&DocumentNode, &Schema) -> Result<(), DefinitionError>;

/// Produces a new schema given an existing schema and a document which may
/// contain GraphQL type extensions and definitions.
///
/// The original schema remains unaltered. Because a schema represents a graph
/// of references, a schema cannot be extended without effectively making an
/// entire copy, and this algorithm copies the provided schema, applying
/// extensions while producing the copy.
///
/// Derived from graphql-js 16.12.0, src/utilities/extendSchema.ts, MIT.
///
/// `schema` is the schema to extend, `document_ast` the document,
/// `options` the two flags and `assert_valid_sdl_extension` the SDL
/// validation the validation crate provides. The result is `None` when the
/// document holds no definition and no extension, which is when the
/// reference returns `schema` itself.
///
/// # Errors
///
/// Returns what `assert_valid_sdl_extension` returns, what a builder throws
/// while it reads the document, and what the schema constructor throws.
pub fn extend_schema(
    schema: &Schema,
    document_ast: &DocumentNode,
    options: ExtendSchemaOptions,
    assert_valid_sdl_extension: AssertValidSdlExtension,
) -> Result<Option<Schema>, DefinitionError> {
    if !options.assume_valid && !options.assume_valid_sdl {
        assert_valid_sdl_extension(document_ast, schema)?;
    }

    let schema_config = schema.to_config();
    let Some(extended_config) = extend_schema_impl(&schema_config, document_ast, options)? else {
        return Ok(None);
    };
    Ok(Some(Schema::new(
        extended_config,
        &introspection_schema()?,
    )?))
}

/// Extends a schema config by a document.
///
/// `schema_config` is the normalized config of the schema to extend, where
/// `directives` of `None` reads as the specified directives, which is what the
/// schema constructor puts there. The result is `None` when the document holds
/// no definition and no extension, which is when the reference returns
/// `schema_config` itself.
///
/// Derived from graphql-js 16.12.0, src/utilities/extendSchema.ts, MIT.
///
/// # Errors
///
/// Returns what a thunk of an existing type throws, the reference's plain
/// `Error` `Unknown type: "X".` for a named type node no type answers to,
/// what `valueFromAST` throws for a default value, and what the reading of a
/// `@deprecated`, `@specifiedBy` or `@oneOf` directive throws.
pub fn extend_schema_impl(
    schema_config: &SchemaConfig,
    document_ast: &DocumentNode,
    options: ExtendSchemaOptions,
) -> Result<Option<SchemaConfig>, DefinitionError> {
    extend_schema_impl_with_bindings(
        schema_config,
        document_ast,
        options,
        &SchemaBindings::default(),
    )
}

/// Extends a schema config by a document, with `bindings` laid over the
/// types the builders make.
///
/// With empty bindings it is [`extend_schema_impl`].
///
/// # Errors
///
/// Returns what [`extend_schema_impl`] returns.
pub(crate) fn extend_schema_impl_with_bindings(
    schema_config: &SchemaConfig,
    document_ast: &DocumentNode,
    options: ExtendSchemaOptions,
    bindings: &SchemaBindings,
) -> Result<Option<SchemaConfig>, DefinitionError> {
    // Collect the type definitions and extensions found in the document.
    let mut type_defs: Vec<TypeDefinitionNode> = Vec::new();
    let mut type_extensions_map: ObjMap<Vec<TypeExtensionNode>> = ObjMap::default();

    // New directives and types are separate because a directives and types can
    // have the same name. For example, a type named "skip".
    let mut directive_defs: Vec<&DirectiveDefinitionNode> = Vec::new();

    let mut schema_def: Option<&SchemaDefinitionNode> = None;
    // Schema extensions are collected which may add additional operation types.
    let mut schema_extensions: Vec<&SchemaExtensionNode> = Vec::new();

    for def in &document_ast.definitions {
        if let DefinitionNode::SchemaDefinition(node) = def {
            schema_def = Some(node);
        } else if let DefinitionNode::SchemaExtension(node) = def {
            schema_extensions.push(node);
        } else if let Some(node) = type_definition(def) {
            type_defs.push(node);
        } else if let Some(node) = type_extension(def) {
            let extended_type_name = extension_name(&node).to_string();
            type_extensions_map
                .entry(extended_type_name)
                .or_default()
                .push(node);
        } else if let DefinitionNode::DirectiveDefinition(node) = def {
            directive_defs.push(node);
        }
    }

    // If this document contains no new types, extensions, or directives then
    // return the same unmodified GraphQLSchema instance.
    if type_extensions_map.is_empty()
        && type_defs.is_empty()
        && directive_defs.is_empty()
        && schema_extensions.is_empty()
        && schema_def.is_none()
    {
        return Ok(None);
    }

    let scope = Arc::new(Scope::new(type_extensions_map, bindings.clone())?);
    let mut type_map: ObjMap<NamedType> = ObjMap::default();
    for existing_type in &schema_config.types {
        type_map.insert(
            existing_type.name().to_string(),
            scope.extend_named_type(existing_type)?,
        );
    }

    for type_node in &type_defs {
        let name = definition_name(type_node);
        let ty = match scope.std_type_map.get(name) {
            Some(std_type) => std_type.clone(),
            None => scope.build_type(type_node)?,
        };
        type_map.insert(name.to_string(), ty);
    }
    scope.set_type_map(type_map);

    let operation_types = scope.operation_types(schema_config, schema_def, &schema_extensions)?;

    // Then produce and return a Schema config with these types.
    let mut directives: Vec<Arc<Directive>> = Vec::new();
    let existing_directives = match &schema_config.directives {
        Some(directives) => directives.as_slice(),
        None => specified_directives(),
    };
    for directive in existing_directives {
        directives.push(scope.replace_directive(directive)?);
    }
    for node in &directive_defs {
        directives.push(scope.build_directive(node)?);
    }

    let mut extension_ast_nodes = schema_config.extension_ast_nodes.clone();
    extension_ast_nodes.extend(schema_extensions.iter().map(|node| (*node).clone()));

    Ok(Some(SchemaConfig {
        description: description_of(schema_def.and_then(|node| node.description.as_ref())),
        query: operation_types.query,
        mutation: operation_types.mutation,
        subscription: operation_types.subscription,
        types: scope.type_map()?.values().cloned().collect(),
        directives: Some(directives),
        extensions: JsObject::new(),
        ast_node: schema_def
            .cloned()
            .or_else(|| schema_config.ast_node.clone()),
        extension_ast_nodes,
        assume_valid: options.assume_valid,
    }))
}

/// The three root types a schema definition or extension names.
struct OperationTypes {
    query: Option<NamedType>,
    mutation: Option<NamedType>,
    subscription: Option<NamedType>,
}

/// The scope the builders of `extendSchemaImpl` close over.
///
/// The type map is written once, after every named type of the document exists,
/// and read by the thunks and by the root type lookups afterwards. The bindings
/// are what the builders lay over the types and the fields they make.
// The three map fields keep the names `typeMap`, `stdTypeMap` and
// `typeExtensionsMap` of the reference, which a reader of both files looks for.
struct Scope {
    type_map: OnceBox<ObjMap<NamedType>>,
    std_type_map: ObjMap<NamedType>,
    type_extensions_map: ObjMap<Vec<TypeExtensionNode>>,
    bindings: SchemaBindings,
}

/// The definition of a type together with its extensions.
struct AllNodes {
    definition: TypeDefinitionNode,
    extensions: Vec<TypeExtensionNode>,
}

impl AllNodes {
    /// Returns the definition followed by the extensions.
    fn refs(&self) -> Vec<TypeNodeRef<'_>> {
        let mut nodes = alloc::vec![TypeNodeRef::Definition(&self.definition)];
        nodes.extend(self.extensions.iter().map(TypeNodeRef::Extension));
        nodes
    }
}

impl Scope {
    /// Prepares the scope over the standard types, the extension map and the
    /// bindings.
    ///
    /// A specified scalar the bindings name enters the standard type map as
    /// a new scalar built from the config of the specified one with the
    /// binding laid over it, which is the in place assignment of
    /// `addResolversToSchema.js:30-40` over a type this crate cannot write
    /// to, so every reference the document makes to that name reaches the
    /// bound type.
    fn new(
        type_extensions_map: ObjMap<Vec<TypeExtensionNode>>,
        bindings: SchemaBindings,
    ) -> Result<Self, DefinitionError> {
        let mut std_type_map = ObjMap::default();
        for scalar in specified_scalar_types() {
            let ty = match bindings.scalars.get(&scalar.name) {
                Some(binding) => Arc::new(ScalarType::new(binding.apply(scalar.to_config()))?),
                None => Arc::clone(scalar),
            };
            std_type_map.insert(scalar.name.clone(), NamedType::from(ty));
        }
        for ty in introspection_types()? {
            std_type_map.insert(ty.name().to_string(), ty.clone());
        }
        Ok(Self {
            type_map: OnceBox::new(),
            std_type_map,
            type_extensions_map,
            bindings,
        })
    }

    /// Publishes the complete type map to the thunks.
    fn set_type_map(&self, type_map: ObjMap<NamedType>) {
        self.type_map.get_or_init(|| Box::new(type_map));
    }

    /// Returns the type map.
    ///
    /// A thunk runs only after the map is published, so the empty cell is the
    /// reference's `invariant(false)`, whose default message this is.
    fn type_map(&self) -> Result<&ObjMap<NamedType>, DefinitionError> {
        self.type_map
            .get()
            .ok_or_else(|| DefinitionError::plain("Unexpected invariant triggered."))
    }

    /// Returns `typeExtensionsMap[name] ?? []`.
    fn extensions_of(&self, name: &str) -> Vec<TypeExtensionNode> {
        self.type_extensions_map
            .get(name)
            .cloned()
            .unwrap_or_default()
    }

    /// Derived from graphql-js 16.12.0, src/utilities/extendSchema.ts, MIT.
    fn replace_type(&self, ty: &Type) -> Result<Type, DefinitionError> {
        if let Some(list) = ty.as_list() {
            return Ok(Type::from(ListType::new(self.replace_type(&list.of_type)?)));
        }
        if let Some(non_null) = ty.as_non_null() {
            return Ok(Type::from(NonNullType::new(
                self.replace_type(non_null.of_type())?,
            )?));
        }
        Ok(Type::from(self.replace_named_type(&get_named_type(ty))?))
    }

    /// Derived from graphql-js 16.12.0, src/utilities/extendSchema.ts, MIT.
    ///
    /// Note: While this could make early assertions to get the correctly
    /// typed values, that would throw immediately while type system
    /// validation with `validateSchema()` will produce more actionable
    /// results. Every type of the schema is in the map, so a miss is the
    /// undefined slot of the reference, reported as an unknown type.
    fn replace_named_type(&self, ty: &NamedType) -> Result<NamedType, DefinitionError> {
        self.type_map()?
            .get(ty.name())
            .cloned()
            .ok_or_else(|| unknown_type(ty.name()))
    }

    /// Derived from graphql-js 16.12.0, src/utilities/extendSchema.ts, MIT.
    fn replace_directive(&self, directive: &Directive) -> Result<Arc<Directive>, DefinitionError> {
        let config = directive.to_config();
        let args = self.map_args(&config.args)?;
        Ok(Arc::new(Directive::new(DirectiveConfig {
            args,
            ..config
        })?))
    }

    /// Returns `mapValue(args, extendArg)`.
    fn map_args(
        &self,
        args: &ObjMap<ArgumentConfig>,
    ) -> Result<ObjMap<ArgumentConfig>, DefinitionError> {
        args.iter()
            .map(|(name, arg)| Ok((name.clone(), self.extend_arg(arg)?)))
            .collect()
    }

    /// The six kinds are the whole enum, so the invariant of the reference has
    /// no branch.
    ///
    /// Derived from graphql-js 16.12.0, src/utilities/extendSchema.ts, MIT.
    fn extend_named_type(self: &Arc<Self>, ty: &NamedType) -> Result<NamedType, DefinitionError> {
        if is_introspection_type(ty) || is_specified_scalar_type(ty) {
            // Builtin types are not extended.
            return Ok(ty.clone());
        }
        Ok(match ty {
            NamedType::Scalar(scalar) => {
                NamedType::from(Arc::new(self.extend_scalar_type(scalar)?))
            }
            NamedType::Object(object) => {
                NamedType::from(Arc::new(self.extend_object_type(object)?))
            }
            NamedType::Interface(interface) => {
                NamedType::from(Arc::new(self.extend_interface_type(interface)?))
            }
            NamedType::Union(union) => NamedType::from(Arc::new(self.extend_union_type(union)?)),
            NamedType::Enum(enum_type) => {
                NamedType::from(Arc::new(self.extend_enum_type(enum_type)?))
            }
            NamedType::InputObject(input_object) => {
                NamedType::from(Arc::new(self.extend_input_object_type(input_object)?))
            }
        })
    }

    /// Derived from graphql-js 16.12.0, src/utilities/extendSchema.ts, MIT.
    fn extend_input_object_type(
        self: &Arc<Self>,
        ty: &InputObjectType,
    ) -> Result<InputObjectType, DefinitionError> {
        let config = ty.to_config()?;
        let extensions = self.extensions_of(&config.name);

        let scope = Arc::clone(self);
        let config_fields = resolve_obj_map_thunk(&config.fields)?.into_owned();
        let field_extensions = extensions.clone();
        let fields = ThunkObjMap::thunk(move || {
            let mut fields: ObjMap<InputFieldConfig> = ObjMap::default();
            for (name, field) in &config_fields {
                let r#type = scope.replace_type(&field.r#type)?;
                fields.insert(
                    name.clone(),
                    InputFieldConfig {
                        r#type,
                        ..field.clone()
                    },
                );
            }
            for (name, field) in scope.build_input_field_map(&extension_refs(&field_extensions))? {
                fields.insert(name, field);
            }
            Ok(fields)
        });

        let mut extension_ast_nodes = config.extension_ast_nodes;
        extension_ast_nodes.extend(pick_extensions(&extensions, |node| match node {
            TypeExtensionNode::InputObjectTypeExtension(node) => Some(node),
            _ => None,
        }));
        InputObjectType::new(InputObjectTypeConfig {
            fields,
            extension_ast_nodes,
            ..config
        })
    }

    /// Derived from graphql-js 16.12.0, src/utilities/extendSchema.ts, MIT.
    fn extend_enum_type(&self, ty: &EnumType) -> Result<EnumType, DefinitionError> {
        let config = ty.to_config()?;
        let extensions = self.extensions_of(&ty.name);

        let mut values = resolve_obj_map_thunk(&config.values)?.into_owned();
        for (name, value) in build_enum_value_map(&extension_refs(&extensions))? {
            values.insert(name, value);
        }

        let mut extension_ast_nodes = config.extension_ast_nodes;
        extension_ast_nodes.extend(pick_extensions(&extensions, |node| match node {
            TypeExtensionNode::EnumTypeExtension(node) => Some(node),
            _ => None,
        }));
        EnumType::new(EnumTypeConfig {
            values: ThunkObjMap::Map(values),
            extension_ast_nodes,
            ..config
        })
    }

    /// Derived from graphql-js 16.12.0, src/utilities/extendSchema.ts, MIT.
    fn extend_scalar_type(&self, ty: &ScalarType) -> Result<ScalarType, DefinitionError> {
        let config = ty.to_config();
        let extensions = self.extensions_of(&config.name);

        let mut specified_by_url = config.specified_by_url.clone();
        for extension_node in &extensions {
            specified_by_url =
                get_specified_by_url(extension_directives(extension_node))?.or(specified_by_url);
        }

        let mut extension_ast_nodes = config.extension_ast_nodes;
        extension_ast_nodes.extend(pick_extensions(&extensions, |node| match node {
            TypeExtensionNode::ScalarTypeExtension(node) => Some(node),
            _ => None,
        }));
        let name = config.name.clone();
        ScalarType::new(self.bindings.bind_scalar(
            &name,
            ScalarTypeConfig {
                specified_by_url,
                extension_ast_nodes,
                ..config
            },
        ))
    }

    /// Derived from graphql-js 16.12.0, src/utilities/extendSchema.ts, MIT.
    fn extend_object_type(
        self: &Arc<Self>,
        ty: &Arc<ObjectType>,
    ) -> Result<ObjectType, DefinitionError> {
        let config = ty.to_config()?;
        let extensions = self.extensions_of(&config.name);

        let old = Arc::clone(ty);
        let interfaces = self.extended_interfaces(extensions.clone(), move || {
            Ok(old.get_interfaces()?.to_vec())
        });
        let config_fields = resolve_obj_map_thunk(&config.fields)?.into_owned();
        let fields = self.extended_fields(config.name.clone(), config_fields, extensions.clone());

        let mut extension_ast_nodes = config.extension_ast_nodes;
        extension_ast_nodes.extend(pick_extensions(&extensions, |node| match node {
            TypeExtensionNode::ObjectTypeExtension(node) => Some(node),
            _ => None,
        }));
        let is_type_of = self.bindings.is_type_of(&config.name, config.is_type_of);
        ObjectType::new(ObjectTypeConfig {
            interfaces,
            fields,
            is_type_of,
            extension_ast_nodes,
            ..config
        })
    }

    /// Derived from graphql-js 16.12.0, src/utilities/extendSchema.ts, MIT.
    fn extend_interface_type(
        self: &Arc<Self>,
        ty: &Arc<InterfaceType>,
    ) -> Result<InterfaceType, DefinitionError> {
        let config = ty.to_config()?;
        let extensions = self.extensions_of(&config.name);

        let old = Arc::clone(ty);
        let interfaces = self.extended_interfaces(extensions.clone(), move || {
            Ok(old.get_interfaces()?.to_vec())
        });
        let config_fields = resolve_obj_map_thunk(&config.fields)?.into_owned();
        let fields = self.extended_fields(config.name.clone(), config_fields, extensions.clone());

        let mut extension_ast_nodes = config.extension_ast_nodes;
        extension_ast_nodes.extend(pick_extensions(&extensions, |node| match node {
            TypeExtensionNode::InterfaceTypeExtension(node) => Some(node),
            _ => None,
        }));
        let resolve_type = self
            .bindings
            .resolve_type(&config.name, config.resolve_type);
        InterfaceType::new(InterfaceTypeConfig {
            interfaces,
            fields,
            resolve_type,
            extension_ast_nodes,
            ..config
        })
    }

    /// Derived from graphql-js 16.12.0, src/utilities/extendSchema.ts, MIT.
    fn extend_union_type(
        self: &Arc<Self>,
        ty: &Arc<UnionType>,
    ) -> Result<UnionType, DefinitionError> {
        let config = ty.to_config()?;
        let extensions = self.extensions_of(&config.name);

        let scope = Arc::clone(self);
        let old = Arc::clone(ty);
        let member_extensions = extensions.clone();
        let types = ThunkReadonlyArray::thunk(move || {
            let mut types = Vec::new();
            for member in old.get_types()? {
                types.push(scope.replace_named_type(member)?);
            }
            types.extend(scope.build_union_types(&extension_refs(&member_extensions))?);
            Ok(types)
        });

        let mut extension_ast_nodes = config.extension_ast_nodes;
        extension_ast_nodes.extend(pick_extensions(&extensions, |node| match node {
            TypeExtensionNode::UnionTypeExtension(node) => Some(node),
            _ => None,
        }));
        let resolve_type = self
            .bindings
            .resolve_type(&config.name, config.resolve_type);
        UnionType::new(UnionTypeConfig {
            types,
            resolve_type,
            extension_ast_nodes,
            ..config
        })
    }

    /// Returns the interfaces thunk of an extended object or interface type.
    fn extended_interfaces(
        self: &Arc<Self>,
        extensions: Vec<TypeExtensionNode>,
        old_interfaces: impl Fn() -> Result<Vec<NamedType>, DefinitionError> + Send + Sync + 'static,
    ) -> ThunkReadonlyArray<NamedType> {
        let scope = Arc::clone(self);
        ThunkReadonlyArray::thunk(move || {
            let mut interfaces = Vec::new();
            for interface in old_interfaces()? {
                interfaces.push(scope.replace_named_type(&interface)?);
            }
            interfaces.extend(scope.build_interfaces(&extension_refs(&extensions))?);
            Ok(interfaces)
        })
    }

    /// Returns the fields thunk of an extended object or interface type
    /// named `type_name`.
    fn extended_fields(
        self: &Arc<Self>,
        type_name: String,
        config_fields: ObjMap<FieldConfig>,
        extensions: Vec<TypeExtensionNode>,
    ) -> ThunkObjMap<FieldConfig> {
        let scope = Arc::clone(self);
        ThunkObjMap::thunk(move || {
            let mut fields: ObjMap<FieldConfig> = ObjMap::default();
            for (name, field) in &config_fields {
                fields.insert(name.clone(), scope.extend_field(field)?);
            }
            for (name, field) in scope.build_field_map(&extension_refs(&extensions))? {
                fields.insert(name, field);
            }
            scope.bindings.bind_fields(&type_name, &mut fields);
            Ok(fields)
        })
    }

    /// Derived from graphql-js 16.12.0, src/utilities/extendSchema.ts, MIT.
    fn extend_field(&self, field: &FieldConfig) -> Result<FieldConfig, DefinitionError> {
        Ok(FieldConfig {
            r#type: self.replace_type(&field.r#type)?,
            args: self.map_args(&field.args)?,
            ..field.clone()
        })
    }

    /// Derived from graphql-js 16.12.0, src/utilities/extendSchema.ts, MIT.
    fn extend_arg(&self, arg: &ArgumentConfig) -> Result<ArgumentConfig, DefinitionError> {
        Ok(ArgumentConfig {
            r#type: self.replace_type(&arg.r#type)?,
            ..arg.clone()
        })
    }

    /// Returns the root types of the extended schema.
    fn operation_types(
        &self,
        schema_config: &SchemaConfig,
        schema_def: Option<&SchemaDefinitionNode>,
        schema_extensions: &[&SchemaExtensionNode],
    ) -> Result<OperationTypes, DefinitionError> {
        // Get the extended root operation types.
        let replace = |root: &Option<NamedType>| {
            root.as_ref()
                .map(|root| self.replace_named_type(root))
                .transpose()
        };
        let mut operation_types = OperationTypes {
            query: replace(&schema_config.query)?,
            mutation: replace(&schema_config.mutation)?,
            subscription: replace(&schema_config.subscription)?,
        };
        // Then, incorporate schema definition and all schema extensions.
        if let Some(schema_def) = schema_def {
            self.get_operation_types(
                &mut operation_types,
                &[schema_def.operation_types.as_slice()],
            )?;
        }
        let extension_operation_types: Vec<&[OperationTypeDefinitionNode]> = schema_extensions
            .iter()
            .map(|node| node.operation_types.as_deref().unwrap_or(&[]))
            .collect();
        self.get_operation_types(&mut operation_types, &extension_operation_types)?;
        Ok(operation_types)
    }

    /// Derived from graphql-js 16.12.0, src/utilities/extendSchema.ts, MIT.
    ///
    /// The reference returns a fresh object the caller spreads over the
    /// earlier keys, and writing the slots in place is the same override.
    /// Note: While this could make early assertions to get the correctly
    /// typed values below, that would throw immediately while type system
    /// validation with `validateSchema()` will produce more actionable
    /// results.
    fn get_operation_types(
        &self,
        op_types: &mut OperationTypes,
        nodes: &[&[OperationTypeDefinitionNode]],
    ) -> Result<(), DefinitionError> {
        for operation_types_nodes in nodes {
            for operation_type in *operation_types_nodes {
                let root = self.get_named_type(&operation_type.r#type)?;
                match operation_type.operation {
                    OperationTypeNode::Query => op_types.query = Some(root),
                    OperationTypeNode::Mutation => op_types.mutation = Some(root),
                    OperationTypeNode::Subscription => op_types.subscription = Some(root),
                }
            }
        }
        Ok(())
    }

    /// Derived from graphql-js 16.12.0, src/utilities/extendSchema.ts, MIT.
    fn get_named_type(&self, node: &NamedTypeNode) -> Result<NamedType, DefinitionError> {
        let name = node.name.value();
        let ty = match self.std_type_map.get(name) {
            Some(std_type) => Some(std_type),
            None => self.type_map()?.get(name),
        };
        ty.cloned().ok_or_else(|| unknown_type(name))
    }

    /// Derived from graphql-js 16.12.0, src/utilities/extendSchema.ts, MIT.
    fn get_wrapped_type(&self, node: &TypeNode) -> Result<Type, DefinitionError> {
        match node {
            TypeNode::ListType(node) => Ok(Type::from(ListType::new(
                self.get_wrapped_type(&node.r#type)?,
            ))),
            TypeNode::NonNullType(node) => {
                let inner = TypeNode::from(node.r#type.clone());
                Ok(Type::from(NonNullType::new(
                    self.get_wrapped_type(&inner)?,
                )?))
            }
            TypeNode::NamedType(node) => Ok(Type::from(self.get_named_type(node)?)),
        }
    }

    /// Derived from graphql-js 16.12.0, src/utilities/extendSchema.ts, MIT.
    fn build_directive(
        &self,
        node: &DirectiveDefinitionNode,
    ) -> Result<Arc<Directive>, DefinitionError> {
        let mut locations = Vec::with_capacity(node.locations.len());
        for location in &node.locations {
            locations.push(directive_location(location.value())?);
        }
        Ok(Arc::new(Directive::new(DirectiveConfig {
            name: String::from(node.name.value()),
            description: description_of(node.description.as_ref()),
            locations,
            location_names: None,
            is_repeatable: node.repeatable,
            args: self.build_argument_map(node.arguments.as_deref())?,
            extensions: JsObject::new(),
            ast_node: Some(node.clone()),
        })?))
    }

    /// Derived from graphql-js 16.12.0, src/utilities/extendSchema.ts, MIT.
    ///
    /// Note: While this could make assertions to get the correctly typed
    /// value, that would throw immediately while type system validation
    /// with `validateSchema()` will produce more actionable results.
    fn build_field_map(
        &self,
        nodes: &[TypeNodeRef<'_>],
    ) -> Result<ObjMap<FieldConfig>, DefinitionError> {
        let mut field_config_map: ObjMap<FieldConfig> = ObjMap::default();
        for node in nodes {
            let node_fields = node.fields().unwrap_or(&[]);

            for field in node_fields {
                let config = FieldConfig {
                    r#type: self.get_wrapped_type(&field.r#type)?,
                    description: description_of(field.description.as_ref()),
                    args: self.build_argument_map(field.arguments.as_deref())?,
                    deprecation_reason: get_deprecation_reason(field.directives.as_deref())?,
                    ast_node: Some(field.clone()),
                    resolve: None,
                    subscribe: None,
                    extensions: JsObject::new(),
                };
                field_config_map.insert(String::from(field.name.value()), config);
            }
        }
        Ok(field_config_map)
    }

    /// Derived from graphql-js 16.12.0, src/utilities/extendSchema.ts, MIT.
    fn build_argument_map(
        &self,
        args: Option<&[InputValueDefinitionNode]>,
    ) -> Result<ObjMap<ArgumentConfig>, DefinitionError> {
        let args_nodes = args.unwrap_or(&[]);

        let mut arg_config_map: ObjMap<ArgumentConfig> = ObjMap::default();
        for arg in args_nodes {
            let r#type = self.get_wrapped_type(&arg.r#type)?;

            let config = ArgumentConfig {
                description: description_of(arg.description.as_ref()),
                default_value: value_from_ast(arg.default_value.as_ref(), &r#type, None)?,
                deprecation_reason: get_deprecation_reason(arg.directives.as_deref())?,
                ast_node: Some(arg.clone()),
                extensions: JsObject::new(),
                r#type,
            };
            arg_config_map.insert(String::from(arg.name.value()), config);
        }
        Ok(arg_config_map)
    }

    /// Derived from graphql-js 16.12.0, src/utilities/extendSchema.ts, MIT.
    fn build_input_field_map(
        &self,
        nodes: &[TypeNodeRef<'_>],
    ) -> Result<ObjMap<InputFieldConfig>, DefinitionError> {
        let mut input_field_map: ObjMap<InputFieldConfig> = ObjMap::default();
        for node in nodes {
            let fields_nodes = node.input_fields().unwrap_or(&[]);

            for field in fields_nodes {
                let r#type = self.get_wrapped_type(&field.r#type)?;

                let config = InputFieldConfig {
                    description: description_of(field.description.as_ref()),
                    default_value: value_from_ast(field.default_value.as_ref(), &r#type, None)?,
                    deprecation_reason: get_deprecation_reason(field.directives.as_deref())?,
                    ast_node: Some(field.clone()),
                    extensions: JsObject::new(),
                    r#type,
                };
                input_field_map.insert(String::from(field.name.value()), config);
            }
        }
        Ok(input_field_map)
    }

    /// Derived from graphql-js 16.12.0, src/utilities/extendSchema.ts, MIT.
    ///
    /// Note: While this could make assertions to get the correctly typed
    /// values below, that would throw immediately while type system
    /// validation with `validateSchema()` will produce more actionable
    /// results.
    fn build_interfaces(
        &self,
        nodes: &[TypeNodeRef<'_>],
    ) -> Result<Vec<NamedType>, DefinitionError> {
        let mut interfaces = Vec::new();
        for node in nodes {
            for interface in node.interfaces().unwrap_or(&[]) {
                interfaces.push(self.get_named_type(interface)?);
            }
        }
        Ok(interfaces)
    }

    /// Derived from graphql-js 16.12.0, src/utilities/extendSchema.ts, MIT.
    ///
    /// Note: While this could make assertions to get the correctly typed
    /// values below, that would throw immediately while type system
    /// validation with `validateSchema()` will produce more actionable
    /// results.
    fn build_union_types(
        &self,
        nodes: &[TypeNodeRef<'_>],
    ) -> Result<Vec<NamedType>, DefinitionError> {
        let mut types = Vec::new();
        for node in nodes {
            for member in node.types().unwrap_or(&[]) {
                types.push(self.get_named_type(member)?);
            }
        }
        Ok(types)
    }

    /// Each case of the switch is a function of its own below.
    ///
    /// Derived from graphql-js 16.12.0, src/utilities/extendSchema.ts, MIT.
    fn build_type(
        self: &Arc<Self>,
        ast_node: &TypeDefinitionNode,
    ) -> Result<NamedType, DefinitionError> {
        let name = definition_name(ast_node).to_string();
        let extension_ast_nodes = self.extensions_of(&name);
        let all_nodes = Arc::new(AllNodes {
            definition: ast_node.clone(),
            extensions: extension_ast_nodes,
        });

        Ok(match ast_node {
            TypeDefinitionNode::ObjectTypeDefinition(node) => {
                NamedType::from(Arc::new(self.build_object_type(name, node, all_nodes)?))
            }
            TypeDefinitionNode::InterfaceTypeDefinition(node) => {
                NamedType::from(Arc::new(self.build_interface_type(name, node, all_nodes)?))
            }
            TypeDefinitionNode::EnumTypeDefinition(node) => {
                NamedType::from(Arc::new(build_enum_type(name, node, &all_nodes)?))
            }
            TypeDefinitionNode::UnionTypeDefinition(node) => {
                NamedType::from(Arc::new(self.build_union_type(name, node, all_nodes)?))
            }
            TypeDefinitionNode::ScalarTypeDefinition(node) => {
                let config = build_scalar_type(name.clone(), node, &all_nodes)?;
                NamedType::from(Arc::new(ScalarType::new(
                    self.bindings.bind_scalar(&name, config),
                )?))
            }
            TypeDefinitionNode::InputObjectTypeDefinition(node) => NamedType::from(Arc::new(
                self.build_input_object_type(name, node, all_nodes)?,
            )),
        })
    }

    /// The `OBJECT_TYPE_DEFINITION` case of `buildType`.
    fn build_object_type(
        self: &Arc<Self>,
        name: String,
        ast_node: &ObjectTypeDefinitionNode,
        all_nodes: Arc<AllNodes>,
    ) -> Result<ObjectType, DefinitionError> {
        let extension_ast_nodes = pick_extensions(&all_nodes.extensions, |node| match node {
            TypeExtensionNode::ObjectTypeExtension(node) => Some(node),
            _ => None,
        });
        let interfaces = self.built_interfaces(Arc::clone(&all_nodes));
        let fields = self.built_fields(all_nodes);
        let is_type_of = self.bindings.is_type_of(&name, None);
        ObjectType::new(ObjectTypeConfig {
            name,
            description: description_of(ast_node.description.as_ref()),
            interfaces,
            fields,
            is_type_of,
            extensions: JsObject::new(),
            ast_node: Some(ast_node.clone()),
            extension_ast_nodes,
        })
    }

    /// The `INTERFACE_TYPE_DEFINITION` case of `buildType`.
    fn build_interface_type(
        self: &Arc<Self>,
        name: String,
        ast_node: &InterfaceTypeDefinitionNode,
        all_nodes: Arc<AllNodes>,
    ) -> Result<InterfaceType, DefinitionError> {
        let extension_ast_nodes = pick_extensions(&all_nodes.extensions, |node| match node {
            TypeExtensionNode::InterfaceTypeExtension(node) => Some(node),
            _ => None,
        });
        let interfaces = self.built_interfaces(Arc::clone(&all_nodes));
        let fields = self.built_fields(all_nodes);
        let resolve_type = self.bindings.resolve_type(&name, None);
        InterfaceType::new(InterfaceTypeConfig {
            name,
            description: description_of(ast_node.description.as_ref()),
            interfaces,
            fields,
            resolve_type,
            extensions: JsObject::new(),
            ast_node: Some(ast_node.clone()),
            extension_ast_nodes,
        })
    }

    /// The `UNION_TYPE_DEFINITION` case of `buildType`.
    fn build_union_type(
        self: &Arc<Self>,
        name: String,
        ast_node: &UnionTypeDefinitionNode,
        all_nodes: Arc<AllNodes>,
    ) -> Result<UnionType, DefinitionError> {
        let extension_ast_nodes = pick_extensions(&all_nodes.extensions, |node| match node {
            TypeExtensionNode::UnionTypeExtension(node) => Some(node),
            _ => None,
        });
        let types = self.built_union_types(all_nodes);
        let resolve_type = self.bindings.resolve_type(&name, None);
        UnionType::new(UnionTypeConfig {
            name,
            description: description_of(ast_node.description.as_ref()),
            types,
            resolve_type,
            extensions: JsObject::new(),
            ast_node: Some(ast_node.clone()),
            extension_ast_nodes,
        })
    }

    /// The `INPUT_OBJECT_TYPE_DEFINITION` case of `buildType`.
    fn build_input_object_type(
        self: &Arc<Self>,
        name: String,
        ast_node: &InputObjectTypeDefinitionNode,
        all_nodes: Arc<AllNodes>,
    ) -> Result<InputObjectType, DefinitionError> {
        let extension_ast_nodes = pick_extensions(&all_nodes.extensions, |node| match node {
            TypeExtensionNode::InputObjectTypeExtension(node) => Some(node),
            _ => None,
        });
        let fields = self.built_input_fields(all_nodes);
        InputObjectType::new(InputObjectTypeConfig {
            name,
            description: description_of(ast_node.description.as_ref()),
            fields,
            extensions: JsObject::new(),
            ast_node: Some(ast_node.clone()),
            extension_ast_nodes,
            is_one_of: is_one_of(ast_node.directives.as_deref())?,
        })
    }

    /// Returns the interfaces thunk of a built type, `() => buildInterfaces(allNodes)`.
    fn built_interfaces(
        self: &Arc<Self>,
        all_nodes: Arc<AllNodes>,
    ) -> ThunkReadonlyArray<NamedType> {
        let scope = Arc::clone(self);
        ThunkReadonlyArray::thunk(move || scope.build_interfaces(&all_nodes.refs()))
    }

    /// Returns the fields thunk of a built type, `() => buildFieldMap(allNodes)`,
    /// with the field bindings of the type laid over the result.
    fn built_fields(self: &Arc<Self>, all_nodes: Arc<AllNodes>) -> ThunkObjMap<FieldConfig> {
        let scope = Arc::clone(self);
        ThunkObjMap::thunk(move || {
            let mut fields = scope.build_field_map(&all_nodes.refs())?;
            scope
                .bindings
                .bind_fields(definition_name(&all_nodes.definition), &mut fields);
            Ok(fields)
        })
    }

    /// Returns the members thunk of a built union, `() => buildUnionTypes(allNodes)`.
    fn built_union_types(
        self: &Arc<Self>,
        all_nodes: Arc<AllNodes>,
    ) -> ThunkReadonlyArray<NamedType> {
        let scope = Arc::clone(self);
        ThunkReadonlyArray::thunk(move || scope.build_union_types(&all_nodes.refs()))
    }

    /// Returns the fields thunk of a built input object, `() => buildInputFieldMap(allNodes)`.
    fn built_input_fields(
        self: &Arc<Self>,
        all_nodes: Arc<AllNodes>,
    ) -> ThunkObjMap<InputFieldConfig> {
        let scope = Arc::clone(self);
        ThunkObjMap::thunk(move || scope.build_input_field_map(&all_nodes.refs()))
    }
}

/// A type definition or a type extension a builder reads members from.
///
/// The builders of the reference take a list that mixes definitions and
/// extensions and read `fields`, `interfaces`, `values` or `types` of each
/// node, which is undefined on a node of another kind.
#[derive(Clone, Copy)]
enum TypeNodeRef<'a> {
    Definition(&'a TypeDefinitionNode),
    Extension(&'a TypeExtensionNode),
}

impl<'a> TypeNodeRef<'a> {
    /// Returns `node.fields` of an object or interface node.
    fn fields(self) -> Option<&'a [FieldDefinitionNode]> {
        match self {
            Self::Definition(TypeDefinitionNode::ObjectTypeDefinition(node)) => {
                node.fields.as_deref()
            }
            Self::Definition(TypeDefinitionNode::InterfaceTypeDefinition(node)) => {
                node.fields.as_deref()
            }
            Self::Extension(TypeExtensionNode::ObjectTypeExtension(node)) => node.fields.as_deref(),
            Self::Extension(TypeExtensionNode::InterfaceTypeExtension(node)) => {
                node.fields.as_deref()
            }
            _ => None,
        }
    }

    /// Returns `node.fields` of an input object node.
    fn input_fields(self) -> Option<&'a [InputValueDefinitionNode]> {
        match self {
            Self::Definition(TypeDefinitionNode::InputObjectTypeDefinition(node)) => {
                node.fields.as_deref()
            }
            Self::Extension(TypeExtensionNode::InputObjectTypeExtension(node)) => {
                node.fields.as_deref()
            }
            _ => None,
        }
    }

    /// Returns `node.values` of an enum node.
    fn values(self) -> Option<&'a [EnumValueDefinitionNode]> {
        match self {
            Self::Definition(TypeDefinitionNode::EnumTypeDefinition(node)) => {
                node.values.as_deref()
            }
            Self::Extension(TypeExtensionNode::EnumTypeExtension(node)) => node.values.as_deref(),
            _ => None,
        }
    }

    /// Returns `node.interfaces` of an object or interface node.
    fn interfaces(self) -> Option<&'a [NamedTypeNode]> {
        match self {
            Self::Definition(TypeDefinitionNode::ObjectTypeDefinition(node)) => {
                node.interfaces.as_deref()
            }
            Self::Definition(TypeDefinitionNode::InterfaceTypeDefinition(node)) => {
                node.interfaces.as_deref()
            }
            Self::Extension(TypeExtensionNode::ObjectTypeExtension(node)) => {
                node.interfaces.as_deref()
            }
            Self::Extension(TypeExtensionNode::InterfaceTypeExtension(node)) => {
                node.interfaces.as_deref()
            }
            _ => None,
        }
    }

    /// Returns `node.types` of a union node.
    fn types(self) -> Option<&'a [NamedTypeNode]> {
        match self {
            Self::Definition(TypeDefinitionNode::UnionTypeDefinition(node)) => {
                node.types.as_deref()
            }
            Self::Extension(TypeExtensionNode::UnionTypeExtension(node)) => node.types.as_deref(),
            _ => None,
        }
    }
}

/// Returns the extension nodes as builder inputs.
fn extension_refs(extensions: &[TypeExtensionNode]) -> Vec<TypeNodeRef<'_>> {
    extensions.iter().map(TypeNodeRef::Extension).collect()
}

/// Returns the extensions of one kind, cloned, for a typed
/// `extensionASTNodes` list.
fn pick_extensions<T: Clone>(
    extensions: &[TypeExtensionNode],
    pick: impl Fn(&TypeExtensionNode) -> Option<&T>,
) -> Vec<T> {
    extensions.iter().filter_map(pick).cloned().collect()
}

/// The predicate narrows the node in place, and the port clones it into the
/// union the builders own.
fn type_definition(def: &DefinitionNode) -> Option<TypeDefinitionNode> {
    Some(match def {
        DefinitionNode::ScalarTypeDefinition(node) => {
            TypeDefinitionNode::ScalarTypeDefinition(node.clone())
        }
        DefinitionNode::ObjectTypeDefinition(node) => {
            TypeDefinitionNode::ObjectTypeDefinition(node.clone())
        }
        DefinitionNode::InterfaceTypeDefinition(node) => {
            TypeDefinitionNode::InterfaceTypeDefinition(node.clone())
        }
        DefinitionNode::UnionTypeDefinition(node) => {
            TypeDefinitionNode::UnionTypeDefinition(node.clone())
        }
        DefinitionNode::EnumTypeDefinition(node) => {
            TypeDefinitionNode::EnumTypeDefinition(node.clone())
        }
        DefinitionNode::InputObjectTypeDefinition(node) => {
            TypeDefinitionNode::InputObjectTypeDefinition(node.clone())
        }
        _ => return None,
    })
}

/// Translation of `isTypeExtensionNode` over a definition, as the narrower
/// union.
fn type_extension(def: &DefinitionNode) -> Option<TypeExtensionNode> {
    Some(match def {
        DefinitionNode::ScalarTypeExtension(node) => {
            TypeExtensionNode::ScalarTypeExtension(node.clone())
        }
        DefinitionNode::ObjectTypeExtension(node) => {
            TypeExtensionNode::ObjectTypeExtension(node.clone())
        }
        DefinitionNode::InterfaceTypeExtension(node) => {
            TypeExtensionNode::InterfaceTypeExtension(node.clone())
        }
        DefinitionNode::UnionTypeExtension(node) => {
            TypeExtensionNode::UnionTypeExtension(node.clone())
        }
        DefinitionNode::EnumTypeExtension(node) => {
            TypeExtensionNode::EnumTypeExtension(node.clone())
        }
        DefinitionNode::InputObjectTypeExtension(node) => {
            TypeExtensionNode::InputObjectTypeExtension(node.clone())
        }
        _ => return None,
    })
}

/// Returns `node.name.value` of a type definition.
fn definition_name(node: &TypeDefinitionNode) -> &str {
    match node {
        TypeDefinitionNode::ScalarTypeDefinition(node) => node.name.value(),
        TypeDefinitionNode::ObjectTypeDefinition(node) => node.name.value(),
        TypeDefinitionNode::InterfaceTypeDefinition(node) => node.name.value(),
        TypeDefinitionNode::UnionTypeDefinition(node) => node.name.value(),
        TypeDefinitionNode::EnumTypeDefinition(node) => node.name.value(),
        TypeDefinitionNode::InputObjectTypeDefinition(node) => node.name.value(),
    }
}

/// Returns `node.name.value` of a type extension.
fn extension_name(node: &TypeExtensionNode) -> &str {
    match node {
        TypeExtensionNode::ScalarTypeExtension(node) => node.name.value(),
        TypeExtensionNode::ObjectTypeExtension(node) => node.name.value(),
        TypeExtensionNode::InterfaceTypeExtension(node) => node.name.value(),
        TypeExtensionNode::UnionTypeExtension(node) => node.name.value(),
        TypeExtensionNode::EnumTypeExtension(node) => node.name.value(),
        TypeExtensionNode::InputObjectTypeExtension(node) => node.name.value(),
    }
}

/// Returns `node.directives` of a type extension of any kind.
fn extension_directives(node: &TypeExtensionNode) -> Option<&[DirectiveNode]> {
    match node {
        TypeExtensionNode::ScalarTypeExtension(ScalarTypeExtensionNode { directives, .. })
        | TypeExtensionNode::ObjectTypeExtension(ObjectTypeExtensionNode { directives, .. })
        | TypeExtensionNode::InterfaceTypeExtension(InterfaceTypeExtensionNode {
            directives,
            ..
        })
        | TypeExtensionNode::UnionTypeExtension(UnionTypeExtensionNode { directives, .. })
        | TypeExtensionNode::EnumTypeExtension(EnumTypeExtensionNode { directives, .. })
        | TypeExtensionNode::InputObjectTypeExtension(InputObjectTypeExtensionNode {
            directives,
            ..
        }) => directives.as_deref(),
    }
}

/// Returns `node.description?.value`.
fn description_of(description: Option<&StringValueNode>) -> Option<String> {
    description.map(|node| node.value.clone())
}

/// Returns the reference's plain `Error` for a type no definition answers to.
fn unknown_type(name: &str) -> DefinitionError {
    DefinitionError::plain(format!("Unknown type: \"{name}\"."))
}

/// Returns the location a name of a directive definition spells.
///
/// The reference keeps the name as a string. The parser accepts only a name of
/// `DirectiveLocation` there, so a document that came out of the parser never
/// misses. The enum of this crate cannot hold another name, so a document built
/// by hand with an unknown location is a plain error in the words of Aosabi.
fn directive_location(name: &str) -> Result<DirectiveLocation, DefinitionError> {
    DirectiveLocation::from_name(name)
        .ok_or_else(|| DefinitionError::plain(format!("Unknown directive location: \"{name}\".")))
}

/// The values of an enum are no thunk in the reference, so they are read at
/// once.
fn build_enum_type(
    name: String,
    ast_node: &EnumTypeDefinitionNode,
    all_nodes: &AllNodes,
) -> Result<EnumType, DefinitionError> {
    EnumType::new(EnumTypeConfig {
        name,
        description: description_of(ast_node.description.as_ref()),
        values: ThunkObjMap::Map(build_enum_value_map(&all_nodes.refs())?),
        extensions: JsObject::new(),
        ast_node: Some(ast_node.clone()),
        extension_ast_nodes: pick_extensions(&all_nodes.extensions, |node| match node {
            TypeExtensionNode::EnumTypeExtension(node) => Some(node),
            _ => None,
        }),
    })
}

/// The URL is read from the definition alone, as the reference reads it. The
/// caller lays the binding of the scalar over the config and builds the type,
/// which is the scalar mapper of `addResolversToSchema.js:216-243` over
/// `toConfig()` of the type this case builds.
fn build_scalar_type(
    name: String,
    ast_node: &ScalarTypeDefinitionNode,
    all_nodes: &AllNodes,
) -> Result<ScalarTypeConfig, DefinitionError> {
    Ok(ScalarTypeConfig {
        name,
        description: description_of(ast_node.description.as_ref()),
        specified_by_url: get_specified_by_url(ast_node.directives.as_deref())?,
        serialize: None,
        parse_value: None,
        parse_literal: None,
        extensions: JsObject::new(),
        ast_node: Some(ast_node.clone()),
        extension_ast_nodes: pick_extensions(&all_nodes.extensions, |node| match node {
            TypeExtensionNode::ScalarTypeExtension(node) => Some(node),
            _ => None,
        }),
    })
}

/// Derived from graphql-js 16.12.0, src/utilities/extendSchema.ts, MIT.
fn build_enum_value_map(
    nodes: &[TypeNodeRef<'_>],
) -> Result<ObjMap<EnumValueConfig>, DefinitionError> {
    let mut enum_value_map: ObjMap<EnumValueConfig> = ObjMap::default();
    for node in nodes {
        let values_nodes = node.values().unwrap_or(&[]);

        for value in values_nodes {
            let config = EnumValueConfig {
                description: description_of(value.description.as_ref()),
                value: JsValue::Undefined,
                deprecation_reason: get_deprecation_reason(value.directives.as_deref())?,
                extensions: JsObject::new(),
                ast_node: Some(value.clone()),
            };
            enum_value_map.insert(String::from(value.name.value()), config);
        }
    }
    Ok(enum_value_map)
}

/// Given a field or enum value node, returns the string value for the
/// deprecation reason.
///
/// A reason that is not a string, which is the null literal, reads as no
/// reason, because every consumer of the reference tests
/// `deprecationReason != null`.
///
/// Derived from graphql-js 16.12.0, src/utilities/extendSchema.ts, MIT.
fn get_deprecation_reason(
    directives: Option<&[DirectiveNode]>,
) -> Result<Option<String>, DefinitionError> {
    let deprecated = get_directive_values(&graphql_deprecated_directive(), directives, None)?;
    Ok(deprecated.and_then(|values| string_value(values.get("reason"))))
}

/// Given a scalar node, returns the string value for the specifiedByURL.
///
/// Derived from graphql-js 16.12.0, src/utilities/extendSchema.ts, MIT.
fn get_specified_by_url(
    directives: Option<&[DirectiveNode]>,
) -> Result<Option<String>, DefinitionError> {
    let specified_by = get_directive_values(&graphql_specified_by_directive(), directives, None)?;
    Ok(specified_by.and_then(|values| string_value(values.get("url"))))
}

/// Given an input object node, returns if the node should be `OneOf`.
///
/// An object is truthy, so any argument map at all is true.
///
/// Derived from graphql-js 16.12.0, src/utilities/extendSchema.ts, MIT.
fn is_one_of(directives: Option<&[DirectiveNode]>) -> Result<bool, DefinitionError> {
    Ok(get_directive_values(&graphql_one_of_directive(), directives, None)?.is_some())
}

/// Returns the text of a string value, and `None` for anything else.
fn string_value(value: Option<&JsValue>) -> Option<String> {
    match value {
        Some(JsValue::String(text)) => Some(String::from_utf8_lossy(text.as_wtf8()).into_owned()),
        _ => None,
    }
}

/// Prepares an object map of argument values given a directive definition
/// and a AST node which may contain directives.
///
/// Optionally also accepts a map of variable values. If the directive does not
/// exist on the node, returns `None`.
///
/// Derived from graphql-js 16.12.0, src/execution/values.ts, MIT.
///
/// # Errors
///
/// Returns what [`get_argument_values`] returns for the directive node.
pub fn get_directive_values(
    directive_def: &Directive,
    directives: Option<&[DirectiveNode]>,
    variable_values: Option<&JsObject>,
) -> Result<Option<JsObject>, DefinitionError> {
    let directive_node = directives.and_then(|directives| {
        directives
            .iter()
            .find(|directive| directive.name.value() == directive_def.name)
    });

    directive_node
        .map(|directive_node| get_argument_values(directive_def, directive_node, variable_values))
        .transpose()
}

/// Prepares an object map of argument values given a list of argument
/// definitions and list of argument AST nodes.
///
/// The executor, which also calls it for a field, carries the field shape.
/// `variable_values` is tested with `hasOwnProperty`, so a key that holds
/// undefined counts as provided and reads as null.
///
/// Derived from graphql-js 16.12.0, src/execution/values.ts, MIT.
///
/// # Errors
///
/// Returns the four `GraphQLError`s of the reference, for a required
/// argument that is missing, a required argument whose variable has no
/// runtime value, a non null argument given null, and an argument whose
/// literal does not coerce, and what `valueFromAST` throws.
// Out of line, so a node with no directive compiles no read of its arguments.
#[inline(never)]
pub fn get_argument_values(
    directive_def: &Directive,
    node: &DirectiveNode,
    variable_values: Option<&JsObject>,
) -> Result<JsObject, DefinitionError> {
    let mut coerced_values = JsObject::new();

    let argument_nodes = node.arguments.as_deref().unwrap_or(&[]);
    let mut arg_node_map: ObjMap<&ArgumentNode> = ObjMap::default();
    for arg in argument_nodes {
        arg_node_map.insert(String::from(arg.name.value()), arg);
    }

    for arg_def in &directive_def.args {
        let name = &arg_def.name;
        let arg_type = &arg_def.r#type;
        let Some(argument_node) = arg_node_map.get(name) else {
            if !arg_def.default_value.is_undefined() {
                coerced_values.insert(name.as_str(), arg_def.default_value.clone());
            } else if is_non_null_type(arg_type) {
                let options = GraphQLErrorOptions {
                    nodes: Some(alloc::vec![AstNode::from(node.clone())]),
                    ..GraphQLErrorOptions::default()
                };
                return Err(GraphQLError::new(
                    format!(
                        "Argument \"{name}\" of required type \"{arg_type}\" was not provided."
                    ),
                    options,
                )
                .into());
            }
            continue;
        };

        let value_node = &argument_node.value;
        let mut is_null = matches!(value_node, ValueNode::NullValue(_));

        if let ValueNode::Variable(variable) = value_node {
            let variable_name = variable.name.value();
            let Some(values) = variable_values.filter(|values| values.contains_key(variable_name))
            else {
                if !arg_def.default_value.is_undefined() {
                    coerced_values.insert(name.as_str(), arg_def.default_value.clone());
                } else if is_non_null_type(arg_type) {
                    return Err(GraphQLError::new(
                        format!(
                            "Argument \"{name}\" of required type \"{arg_type}\" was provided the variable \"${variable_name}\" which was not provided a runtime value."
                        ),
                        blame(value_node),
                    )
                    .into());
                }
                continue;
            };
            is_null = values
                .get(variable_name)
                .is_none_or(|value| matches!(value, JsValue::Null | JsValue::Undefined));
        }

        if is_null && is_non_null_type(arg_type) {
            return Err(GraphQLError::new(
                format!("Argument \"{name}\" of non-null type \"{arg_type}\" must not be null."),
                blame(value_node),
            )
            .into());
        }

        let coerced_value = value_from_ast(Some(value_node), arg_type, variable_values)?;
        if coerced_value.is_undefined() {
            // Note: ValuesOfCorrectTypeRule validation should catch this before
            // execution. This is a runtime check to ensure execution does not
            // continue with an invalid argument value.
            return Err(GraphQLError::new(
                format!(
                    "Argument \"{name}\" has invalid value {}.",
                    print(value_node)
                ),
                blame(value_node),
            )
            .into());
        }
        coerced_values.insert(name.as_str(), coerced_value);
    }
    Ok(coerced_values)
}
