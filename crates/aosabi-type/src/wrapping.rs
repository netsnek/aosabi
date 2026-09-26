// SPDX-License-Identifier: MIT
/*
 * aosabi-type: the wrapping schema of a remote and the type node its request
 * builder prints.
 *
 * Derived from @graphql-tools/wrap 11.1.9, dist/index.cjs, MIT.
 * Derived from @graphql-tools/utils 11.0.0, cjs/mapSchema.js, MIT.
 * Derived from @graphql-tools/utils 11.0.0, cjs/astFromType.js, MIT.
 */

//! The wrapping schema of a remote, built once from its client schema with
//! its resolvers in place, and the type node a request builder prints.
//!
//! [`create_wrapping_schema`] maps the client schema `build_client_schema`
//! makes of a remote's introspection into the schema the delegation runs
//! over, `mapSchema` of `createWrappingSchema`. A specified scalar, a
//! specified directive and an introspection type stay the instance they are,
//! as `mapSchema` keeps them. The resolvers are values of the executor,
//! which this crate carries in a [󰀋](crate::ResolverHandle) without reading,
//! so the caller hands them in as the bindings of the root fields and the
//! one merged resolver.
//!
//! A clearing binding is spelled out here, since the bindings of
//! [󰀀](crate::schema_bindings) lay a present member over the built one and
//! never clear one. A client schema carries none of the cleared members, so
//! the clearing changes no type a remote describes, and it is kept because
//! the reference clears them and a schema built another way may carry them.
//! `mapSchema` also serializes the default value of every argument and input
//! field with the old leaf types and parses it with the new ones. The leaf
//! types of a client schema and of its wrapping schema have the identity
//! hooks and the same enum names, so the round trip answers the value it was
//! given and the port keeps the value as it stands.
//!
//! [`ast_from_type`] writes a type as the type node of a variable
//! definition, which the request builder of a delegation reads off the
//! argument node `astFromArg` makes of the argument of the target's root
//! field. The builder reads the member `type` of that node and nothing else,
//! so the module holds the function that member is, and `astFromArg` with
//! the description, the default value and the directives of an argument
//! belongs to the printer of a schema with directives, which no caller of
//! the delegation reaches.
//!
//! The module uses `core` and `alloc` and no item of the standard library,
//! shares a type and its scope through [`SharedType`] and never names the
//! pointer behind it, and holds no static.

use alloc::{
    boxed::Box,
    format,
    string::{
        String,
        ToString, //
    },
    vec::Vec, //
};

use aosabi_json::JsValue;
use aosabi_lang::ast::{
    ListTypeNode,
    NameNode,
    NamedTypeNode,
    NonNullTypeNode,
    NullableTypeNode,
    TypeNode, //
};
use once_cell::race::OnceBox;

use crate::{
    definition::{
        ArgumentConfig,
        DefinitionError,
        EnumType,
        EnumTypeConfig,
        FieldConfig,
        FieldResolver,
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
        SharedType,
        ThunkObjMap,
        ThunkReadonlyArray,
        Type,
        UnionType,
        UnionTypeConfig,
        get_named_type,
        resolve_obj_map_thunk, //
    },
    directives::{
        Directive,
        DirectiveConfig,
        is_specified_directive,
        specified_directives, //
    },
    introspection::{
        introspection_schema,
        is_introspection_type, //
    },
    scalars::is_specified_scalar_type,
    schema::{
        Schema,
        SchemaConfig, //
    },
    schema_bindings::FieldBinding, //
};

/// Returns the wrapping schema of `schema`, its root fields bound to `proxying_resolvers` and every other object field to `merged_resolver`.
///
/// `proxying_resolvers` is keyed by the name of a root type and then by the
/// name of its field, and each binding is spread over the field config,
/// `{...fieldConfig, ...proxyingResolvers[typeName]?.[fieldName]}`. The
/// types keep the order of the type map of `schema`, which is the order
/// `mapSchema` hands the schema constructor, and a type whose name starts
/// with `__` is handed over only where another type names it, as `mapTypes`
/// and `rewireTypes` of `mapSchema` leave it out and add it back. A remote
/// type named `__Mine` that no field names is therefore no type of the
/// wrapping schema, and the validation of a delegation never meets its name.
///
/// Derived from @graphql-tools/wrap 11.1.9, dist/index.cjs, MIT.
///
/// # Errors
///
/// Returns what a thunk of `schema` throws while its config is read, and
/// what the constructor of a type or of the schema throws.
pub fn create_wrapping_schema(
    schema: &Schema,
    proxying_resolvers: &ObjMap<ObjMap<FieldBinding>>,
    merged_resolver: &FieldResolver,
) -> Result<Schema, DefinitionError> {
    let config = schema.to_config();
    let root_names = [&config.query, &config.mutation, &config.subscription]
        .into_iter()
        .flatten()
        .map(|root| root.name().to_string())
        .collect();
    let scope = SharedType::new(MapScope {
        type_map: OnceBox::new(),
        root_names,
        proxying_resolvers: proxying_resolvers.clone(),
        merged_resolver: merged_resolver.clone(),
    });

    let mut type_map: ObjMap<NamedType> = ObjMap::default();
    for named_type in &config.types {
        type_map.insert(
            named_type.name().to_string(),
            scope.map_named_type(named_type)?,
        );
    }
    scope.set_type_map(type_map);

    let existing_directives = match &config.directives {
        Some(directives) => directives.as_slice(),
        None => specified_directives(),
    };
    let mut directives = Vec::with_capacity(existing_directives.len());
    for directive in existing_directives {
        directives.push(scope.rewire_directive(directive)?);
    }

    // The type map `mapSchema` hands the constructor leaves out every name
    // that starts with `__`, and `rewireTypes` adds such a type back where a
    // field, an argument, an interface or a member names it, which the
    // constructor reaches from the types it is handed. The standard
    // introspection types come back through `__Schema`, which the
    // constructor collects last.
    let types = scope
        .type_map()?
        .values()
        .filter(|named_type| !named_type.name().starts_with("__"))
        .cloned()
        .collect();
    Schema::new(
        SchemaConfig {
            query: scope.replace_root(config.query.as_ref())?,
            mutation: scope.replace_root(config.mutation.as_ref())?,
            subscription: scope.replace_root(config.subscription.as_ref())?,
            types,
            directives: Some(directives),
            ..config
        },
        &introspection_schema()?,
    )
}

/// The scope the mapped types of [`create_wrapping_schema`] close over.
///
/// The new type map is written once every named type is mapped, and the
/// thunks of the mapped types read it afterwards to reach the new instance
/// of each type they refer to, which is `rewireTypes` of `mapSchema`.
struct MapScope {
    /// The mapped types by name, published once all exist.
    type_map: OnceBox<ObjMap<NamedType>>,
    /// The names of the root types, whose fields take the proxying resolvers.
    root_names: Vec<String>,
    /// The bindings of the root fields, by type name and field name.
    proxying_resolvers: ObjMap<ObjMap<FieldBinding>>,
    /// The resolver of every object field below the roots, `defaultMergedResolver`.
    merged_resolver: FieldResolver,
}

impl MapScope {
    /// Publishes the complete type map to the thunks.
    fn set_type_map(&self, type_map: ObjMap<NamedType>) {
        self.type_map.get_or_init(|| Box::new(type_map));
    }

    /// Returns the type map, which every thunk reads after it is published.
    fn type_map(&self) -> Result<&ObjMap<NamedType>, DefinitionError> {
        self.type_map
            .get()
            .ok_or_else(|| DefinitionError::plain("Unexpected invariant triggered."))
    }

    /// Returns the mapped instance of the named type `named_type`.
    fn replace_named_type(&self, named_type: &NamedType) -> Result<NamedType, DefinitionError> {
        self.type_map()?
            .get(named_type.name())
            .cloned()
            .ok_or_else(|| {
                DefinitionError::plain(format!("Unknown type: \"{}\".", named_type.name()))
            })
    }

    /// Returns `type` with its named type replaced by the mapped instance, the wrappers kept.
    fn replace_type(&self, r#type: &Type) -> Result<Type, DefinitionError> {
        if let Some(list) = r#type.as_list() {
            return Ok(Type::from(ListType::new(self.replace_type(&list.of_type)?)));
        }
        if let Some(non_null) = r#type.as_non_null() {
            return Ok(Type::from(NonNullType::new(
                self.replace_type(non_null.of_type())?,
            )?));
        }
        Ok(Type::from(
            self.replace_named_type(&get_named_type(r#type))?,
        ))
    }

    /// Returns the mapped root type, an object type of the new map.
    fn replace_root(&self, root: Option<&NamedType>) -> Result<Option<NamedType>, DefinitionError> {
        root.map(|root| self.replace_named_type(root)).transpose()
    }

    /// Returns `args` with every argument type replaced.
    fn map_args(
        &self,
        args: &ObjMap<ArgumentConfig>,
    ) -> Result<ObjMap<ArgumentConfig>, DefinitionError> {
        args.iter()
            .map(|(name, arg)| {
                Ok((
                    name.clone(),
                    ArgumentConfig {
                        r#type: self.replace_type(&arg.r#type)?,
                        ..arg.clone()
                    },
                ))
            })
            .collect()
    }

    /// Returns a directive with its argument types replaced, a specified directive as it is.
    fn rewire_directive(
        &self,
        directive: &SharedType<Directive>,
    ) -> Result<SharedType<Directive>, DefinitionError> {
        if is_specified_directive(directive) {
            return Ok(SharedType::clone(directive));
        }
        let config = directive.to_config();
        let args = self.map_args(&config.args)?;
        Ok(SharedType::new(Directive::new(DirectiveConfig {
            args,
            ..config
        })?))
    }

    /// Returns the mapped copy of a named type, the mappers of `createWrappingSchema` applied.
    ///
    /// An introspection type and a specified scalar are kept as they are,
    /// which is what `mapSchema` does with a name that starts with `__` and
    /// what the scalar mapper answers for a specified scalar.
    fn map_named_type(
        self: &SharedType<Self>,
        named_type: &NamedType,
    ) -> Result<NamedType, DefinitionError> {
        if is_introspection_type(named_type) || is_specified_scalar_type(named_type) {
            return Ok(named_type.clone());
        }
        Ok(match named_type {
            NamedType::Scalar(scalar) => NamedType::from(SharedType::new(map_scalar_type(scalar)?)),
            NamedType::Object(object) => {
                NamedType::from(SharedType::new(self.map_object_type(object)?))
            }
            NamedType::Interface(interface) => {
                NamedType::from(SharedType::new(self.map_interface_type(interface)?))
            }
            NamedType::Union(union) => {
                NamedType::from(SharedType::new(self.map_union_type(union)?))
            }
            NamedType::Enum(enum_type) => {
                NamedType::from(SharedType::new(map_enum_type(enum_type)?))
            }
            NamedType::InputObject(input_object) => {
                NamedType::from(SharedType::new(self.map_input_object_type(input_object)?))
            }
        })
    }

    /// Returns the copy of an object type with no `isTypeOf` and its fields bound.
    ///
    /// A root type binds the proxying resolver of each field, the mapper
    /// `ROOT_FIELD`, and any other object type binds the merged resolver
    /// and clears `subscribe`, the mapper `OBJECT_FIELD`.
    fn map_object_type(
        self: &SharedType<Self>,
        object: &SharedType<ObjectType>,
    ) -> Result<ObjectType, DefinitionError> {
        let config = object.to_config()?;
        let is_root = self.root_names.contains(&config.name);
        let interfaces = self.interfaces_thunk(&config.interfaces)?;
        let config_fields = resolve_obj_map_thunk(&config.fields)?.into_owned();
        let scope = SharedType::clone(self);
        let type_name = config.name.clone();
        let fields = ThunkObjMap::thunk(move || {
            let mut fields: ObjMap<FieldConfig> = ObjMap::default();
            for (field_name, field) in &config_fields {
                let mut field = scope.rewire_field(field)?;
                if is_root {
                    if let Some(binding) = scope
                        .proxying_resolvers
                        .get(&type_name)
                        .and_then(|bindings| bindings.get(field_name))
                    {
                        field = binding.apply(field);
                    }
                } else {
                    field.resolve = Some(scope.merged_resolver.clone());
                    field.subscribe = None;
                }
                fields.insert(field_name.clone(), field);
            }
            Ok(fields)
        });
        ObjectType::new(ObjectTypeConfig {
            interfaces,
            fields,
            is_type_of: None,
            ..config
        })
    }

    /// Returns the copy of an interface type with no `resolveType`.
    fn map_interface_type(
        self: &SharedType<Self>,
        interface: &SharedType<InterfaceType>,
    ) -> Result<InterfaceType, DefinitionError> {
        let config = interface.to_config()?;
        let interfaces = self.interfaces_thunk(&config.interfaces)?;
        let fields = self.fields_thunk(&config.fields)?;
        InterfaceType::new(InterfaceTypeConfig {
            interfaces,
            fields,
            resolve_type: None,
            ..config
        })
    }

    /// Returns the copy of a union type with no `resolveType`.
    fn map_union_type(
        self: &SharedType<Self>,
        union: &SharedType<UnionType>,
    ) -> Result<UnionType, DefinitionError> {
        let config = union.to_config()?;
        let members = union.get_types()?.to_vec();
        let scope = SharedType::clone(self);
        let types = ThunkReadonlyArray::thunk(move || {
            members
                .iter()
                .map(|member| scope.replace_named_type(member))
                .collect()
        });
        UnionType::new(UnionTypeConfig {
            types,
            resolve_type: None,
            ..config
        })
    }

    /// Returns the copy of an input object type with its field types replaced.
    fn map_input_object_type(
        self: &SharedType<Self>,
        input_object: &SharedType<InputObjectType>,
    ) -> Result<InputObjectType, DefinitionError> {
        let config = input_object.to_config()?;
        let config_fields = resolve_obj_map_thunk(&config.fields)?.into_owned();
        let scope = SharedType::clone(self);
        let fields = ThunkObjMap::thunk(move || {
            config_fields
                .iter()
                .map(|(name, field)| {
                    Ok((
                        name.clone(),
                        InputFieldConfig {
                            r#type: scope.replace_type(&field.r#type)?,
                            ..field.clone()
                        },
                    ))
                })
                .collect()
        });
        InputObjectType::new(InputObjectTypeConfig { fields, ..config })
    }

    /// Returns a thunk of the interfaces of an object or an interface type, each replaced.
    fn interfaces_thunk(
        self: &SharedType<Self>,
        interfaces: &ThunkReadonlyArray<NamedType>,
    ) -> Result<ThunkReadonlyArray<NamedType>, DefinitionError> {
        let old = match interfaces {
            ThunkReadonlyArray::Array(items) => items.clone(),
            ThunkReadonlyArray::Thunk(function) => function()?,
        };
        let scope = SharedType::clone(self);
        Ok(ThunkReadonlyArray::thunk(move || {
            old.iter()
                .map(|interface| scope.replace_named_type(interface))
                .collect()
        }))
    }

    /// Returns a thunk of the fields of an interface type, each rewired and left unbound.
    fn fields_thunk(
        self: &SharedType<Self>,
        fields: &ThunkObjMap<FieldConfig>,
    ) -> Result<ThunkObjMap<FieldConfig>, DefinitionError> {
        let config_fields = resolve_obj_map_thunk(fields)?.into_owned();
        let scope = SharedType::clone(self);
        Ok(ThunkObjMap::thunk(move || {
            config_fields
                .iter()
                .map(|(name, field)| Ok((name.clone(), scope.rewire_field(field)?)))
                .collect()
        }))
    }

    /// Returns a field config with its type and its argument types replaced.
    fn rewire_field(&self, field: &FieldConfig) -> Result<FieldConfig, DefinitionError> {
        Ok(FieldConfig {
            r#type: self.replace_type(&field.r#type)?,
            args: self.map_args(&field.args)?,
            ..field.clone()
        })
    }
}

/// Returns the copy of a custom scalar with its three hooks cleared, which takes the identity hooks of the constructor.
fn map_scalar_type(scalar: &ScalarType) -> Result<ScalarType, DefinitionError> {
    ScalarType::new(ScalarTypeConfig {
        serialize: None,
        parse_value: None,
        parse_literal: None,
        ..scalar.to_config()
    })
}

/// Returns the copy of an enum type whose values carry no internal value, so each is its name.
fn map_enum_type(enum_type: &EnumType) -> Result<EnumType, DefinitionError> {
    let config = enum_type.to_config()?;
    let values = resolve_obj_map_thunk(&config.values)?
        .iter()
        .map(|(name, value)| {
            let mut value = value.clone();
            value.value = JsValue::Undefined;
            (name.clone(), value)
        })
        .collect();
    EnumType::new(EnumTypeConfig {
        values: ThunkObjMap::Map(values),
        ..config
    })
}

/// Returns the type node that writes `type`, a non null and a list wrapping the node of the type they wrap.
///
/// # Errors
///
/// Returns the plain error `Invalid type node <type>. Inner type of non-null
/// type cannot be a non-null type.` for a non null type around a non null
/// type, which the constructor of a non null type already refuses.
pub fn ast_from_type(r#type: &Type) -> Result<TypeNode, DefinitionError> {
    match r#type {
        Type::NonNull(non_null) => {
            let inner_type = match ast_from_type(non_null.of_type())? {
                TypeNode::NamedType(named) => NullableTypeNode::NamedType(named),
                TypeNode::ListType(list) => NullableTypeNode::ListType(list),
                TypeNode::NonNullType(_) => {
                    return Err(DefinitionError::plain(format!(
                        "Invalid type node {type}. Inner type of non-null type cannot be a non-null type."
                    )));
                }
            };
            Ok(TypeNode::NonNullType(NonNullTypeNode {
                loc: None,
                r#type: inner_type,
            }))
        }
        Type::List(list) => Ok(TypeNode::ListType(ListTypeNode {
            loc: None,
            r#type: Box::new(ast_from_type(&list.of_type)?),
        })),
        named => {
            let named_type = named.as_named();
            Ok(TypeNode::NamedType(NamedTypeNode {
                loc: None,
                name: NameNode::new(None, named_type.as_ref().map_or("", NamedType::name)),
            }))
        }
    }
}
