// SPDX-License-Identifier: MIT
/*
 * aosabi-type: the client schema that graphql-js builds from an introspection
 * result.
 *
 * Derived from graphql-js 16.12.0, src/utilities/buildClientSchema.ts, MIT.
 * Derived from graphql-js 16.12.0, src/jsutils/keyValMap.ts, MIT.
 * Derived from graphql-js 16.12.0, src/jsutils/isObjectLike.ts, MIT.
 */

//! The client schema of an introspection result.
//!
//! The function [`build_client_schema`] turns the `data` of an answer to the
//! introspection query into a [`Schema`] with no resolver, the schema `pull`
//! walks to write a registry. The type map is built in the order of
//! `__schema.types`, and the schema constructor keeps that order, so the order
//! `pull` walks is the order graphql-js gives.
//!
//! # Reading a plain object
//!
//! The reference reads the result as a plain object of `JSON.parse`, member by
//! member, with no schema of its own. Its checks are truthiness tests and the
//! `devAssert` calls of the constructors it hands each member to, and a member
//! of the wrong shape that no check meets fails inside the engine with a
//! `TypeError`. The port reads a [`JsValue`] the same way: a member read on
//! `null` or `undefined` is the `TypeError` of that read, a member read on a
//! string, a number or a boolean is `undefined`, a list is iterated as
//! `for...of` iterates it, a string by its code points, and a key of a map is
//! the string `ToPropertyKey` makes of the name the reference reads. Every
//! message the reference throws, its own and the `TypeError` of V8, is a
//! [`DefinitionError::Plain`] with the reference's text, and a name that fails
//! `assertName` is its [`DefinitionError::GraphQL`]. The texts of the
//! `TypeError` are those node 24.13.0 prints for the pinned graphql 16.12.0.
//!
//! Five readings differ from the reference where a value carries what the
//! types of this crate cannot hold, and no server that answers the query
//! reaches the first three. The introspection schema types a description, a
//! reason and a flag, and every name of a schema has passed `assertName`, so
//! it holds no surrogate. A description or a deprecation reason that is a
//! string is read as itself and any other value as absent, where the reference
//! carries the value. A flag such as `isOneOf` or `isRepeatable` is read by
//! its truthiness, where the reference carries the value and every later
//! reader tests its truthiness. A lone surrogate in a name, in a key or in a
//! string iterated as a list is read as one replacement character, a single
//! code unit as the surrogate is, so the same check of the reference refuses
//! it (`assertName` for a name, the unknown type message for a type reference,
//! the message of `buildType` for an item) and the message differs from the
//! reference in the printed character alone.
//!
//! A server reaches the last two readings. A location of a directive that
//! names no member of [`DirectiveLocation`], which a server of a later version
//! of GraphQL may answer, stands in the names of the directive's locations and
//! not among its locations, where the reference carries the item in its list.
//! No step of a delegation reads a location of a remote directive and the
//! validation counts the names, so a delegation answers as the reference
//! answers, and a printed schema writes an item that is no string as
//! `Array.prototype.join` writes it. A description, a deprecation reason or a
//! URL may hold a lone surrogate, which `JSON.stringify` writes as an escape
//! such as `\ud800`, and the port reads each lone surrogate as one replacement
//! character where the reference carries the surrogate and prints it
//! unchanged.
//!
//! A URL that is not a string is no reading of its own. The port refuses it
//! with the `devAssert` message of the scalar constructor,
//! `<name> must provide "specifiedByURL" as a string, but got: <value>.`, as
//! the reference refuses it.
//!
//! [quote]: https://tc39.es/ecma262/#sec-quotejsonstring
//!
//! # Recursion
//!
//! The field thunk of an input object reads the default value of each field
//! against its type, and that read resolves the fields of every input object
//! the value names. A default value that names the input object whose fields
//! are being resolved enters its thunk again, which the reference repeats
//! until the stack ends with the `RangeError`
//! `Maximum call stack size exceeded`. The port answers that message as a
//! plain error when the thunk is entered the second time, because the stack of
//! a Rust process ends in an abort and never in an error.

use alloc::{
    borrow::Cow,
    boxed::Box,
    format,
    string::String,
    sync::Arc,
    vec::Vec, //
};
use core::sync::atomic::{
    AtomicBool,
    Ordering, //
};

use aosabi_json::{
    JsObject,
    JsString,
    JsValue, //
};
use aosabi_lang::{
    Nesting,
    ParseOptions,
    ParseValueError,
    Source,
    js_string::to_string_lossy,
    parse_value_within, //
};
use once_cell::race::OnceBox;

use crate::{
    assert_name::assert_name,
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
        assert_interface_type,
        assert_nullable_type,
        assert_object_type,
        define_arguments,
        is_input_type,
        is_output_type,
        number_to_string, //
    },
    directives::{
        Directive,
        DirectiveConfig,
        DirectiveLocation, //
    },
    inspect::inspect,
    introspection::{
        introspection_schema,
        introspection_types, //
    },
    scalars::specified_scalar_types,
    schema::{
        Schema,
        SchemaConfig, //
    },
    value_from_ast::value_from_ast, //
};

/// The options of `buildClientSchema`.
///
/// The second argument of `buildClientSchema`, whose one member defaults to
/// false.
///
/// Derived from graphql-js 16.12.0, src/type/schema.ts, MIT.
#[derive(Clone, Copy, Debug, Default)]
pub struct BuildClientSchemaOptions {
    /// When building a schema from a GraphQL service's introspection result,
    /// it might be safe to assume the schema is valid, which skips the type
    /// system validation of the built schema.
    pub assume_valid: bool,
}

/// Builds a [`Schema`] for use by client tools.
///
/// Given the result of a client running the introspection query, creates and
/// returns a schema which can be then used with all the tools of this crate,
/// but cannot be used to execute a query, as introspection does not represent
/// the "resolver", "parse" or "serialize" functions or any other server
/// internal mechanisms.
///
/// Derived from graphql-js 16.12.0, src/utilities/buildClientSchema.ts, MIT.
///
/// The argument `introspection` is the `data` member of the answer, the
/// value that carries `__schema`, and `options` the one flag of the schema.
/// This function expects a complete introspection result. Don't forget to
/// check the `errors` member of a server response before calling this
/// function.
///
/// # Errors
///
/// Returns the plain `Error` of the reference for a result without
/// `__schema`, for a type, a field or a directive that misses a member the
/// full query asks for, for a type reference no type of the result answers
/// to, and for a type in the wrong position, the plain `Error` of V8 for a
/// member of the wrong shape that no check of the reference meets, the error
/// of `assertName` for a name that is no GraphQL name, the syntax error of a
/// default value, and what the schema constructor throws.
pub fn build_client_schema(
    introspection: &JsValue,
    options: BuildClientSchemaOptions,
) -> Result<Schema, DefinitionError> {
    let schema_introspection = schema_of(introspection).ok_or_else(|| {
        DefinitionError::plain(format!(
            "Invalid or incomplete introspection result. Ensure that you are passing \"data\" property of introspection response and no \"errors\" was returned alongside: {}.",
            inspect(introspection)
        ))
    })?;

    let scope = Arc::new(Scope::default());

    // Iterate through all types, getting the type definition for each.
    let mut type_map = key_val_map(
        member(schema_introspection, "types"),
        |type_introspection| scope.build_type(type_introspection),
    )?;

    // Include standard types only if they are used.
    for std_type in specified_scalar_types() {
        if let Some(slot) = type_map.get_mut(&std_type.name) {
            *slot = NamedType::Scalar(Arc::clone(std_type));
        }
    }
    for std_type in introspection_types()? {
        if let Some(slot) = type_map.get_mut(std_type.name()) {
            *slot = std_type.clone();
        }
    }
    scope.set_type_map(type_map);

    // Get the root Query, Mutation, and Subscription types.
    let query = scope.root_type(member(schema_introspection, "queryType"))?;
    let mutation = scope.root_type(member(schema_introspection, "mutationType"))?;
    let subscription = scope.root_type(member(schema_introspection, "subscriptionType"))?;

    // Get the directives supported by Introspection, assuming empty-set if
    // directives were not queried for.
    let directive_introspections = member(schema_introspection, "directives");
    let directives = if truthy(directive_introspections) {
        array_for(
            directive_introspections,
            "schemaIntrospection.directives.map",
        )?
        .iter()
        .map(|directive_introspection| scope.build_directive(directive_introspection))
        .collect::<Result<Vec<_>, _>>()?
    } else {
        Vec::new()
    };

    // Then produce and return a Schema with these types.
    Schema::new(
        SchemaConfig {
            description: optional_string(member(schema_introspection, "description")),
            query,
            mutation,
            subscription,
            types: scope.type_map()?.values().cloned().collect(),
            directives: Some(directives),
            extensions: JsObject::new(),
            ast_node: None,
            extension_ast_nodes: Vec::new(),
            assume_valid: options.assume_valid,
        },
        &introspection_schema()?,
    )
}

/// Returns `introspection.__schema` when both are object like.
fn schema_of(introspection: &JsValue) -> Option<&JsValue> {
    if !is_object_like(Some(introspection)) {
        return None;
    }
    member(introspection, "__schema").filter(|schema| is_object_like(Some(schema)))
}

/// The scope the builders of `buildClientSchema` close over.
///
/// The reference builds the type map in its own scope and every builder
/// reads it from there, inside the thunks of the types too, which run after
/// the function has returned. The map lands in a race cell once every type
/// of the result exists and the standard types have replaced their copies,
/// because a thunk of a type refers to types that are built after it.
#[derive(Default)]
struct Scope {
    type_map: OnceBox<ObjMap<NamedType>>,
}

impl Scope {
    /// Publishes the complete type map to the thunks and the root lookups.
    fn set_type_map(&self, type_map: ObjMap<NamedType>) {
        self.type_map.get_or_init(|| Box::new(type_map));
    }

    /// Returns the type map.
    ///
    /// A lookup runs only after the map is published, so the empty cell is the
    /// reference's `invariant(false)`, whose default message this is.
    fn type_map(&self) -> Result<&ObjMap<NamedType>, DefinitionError> {
        self.type_map
            .get()
            .ok_or_else(|| DefinitionError::plain("Unexpected invariant triggered."))
    }

    /// Returns the root type a truthy reference names, and none otherwise.
    fn root_type(&self, type_ref: Option<&JsValue>) -> Result<Option<NamedType>, DefinitionError> {
        if truthy(type_ref) {
            Ok(Some(self.get_object_type(type_ref)?))
        } else {
            Ok(None)
        }
    }

    /// Given a type reference in introspection, return the type instance,
    /// preferring cached instances before building new instances.
    ///
    /// Derived from graphql-js 16.12.0, src/utilities/buildClientSchema.ts, MIT.
    fn get_type(&self, type_ref: Option<&JsValue>) -> Result<Type, DefinitionError> {
        let kind = get(type_ref, "kind")?;
        if is_string(kind, "LIST") {
            let item_ref = get(type_ref, "ofType")?;
            if !truthy(item_ref) {
                return Err(decorated_too_deep());
            }
            return Ok(Type::from(ListType::new(self.get_type(item_ref)?)));
        }
        if is_string(kind, "NON_NULL") {
            let nullable_ref = get(type_ref, "ofType")?;
            if !truthy(nullable_ref) {
                return Err(decorated_too_deep());
            }
            let nullable_type = self.get_type(nullable_ref)?;
            return Ok(Type::from(NonNullType::new(
                assert_nullable_type(&nullable_type)?.clone(),
            )?));
        }
        Ok(Type::from(self.get_named_type(type_ref)?))
    }

    /// Returns the named type a reference names.
    ///
    /// The map is read by the property key the reference reads it by, and the
    /// message names the name as a template literal prints it, which is the
    /// same string.
    ///
    /// Derived from graphql-js 16.12.0, src/utilities/buildClientSchema.ts, MIT.
    fn get_named_type(&self, type_ref: Option<&JsValue>) -> Result<NamedType, DefinitionError> {
        let type_name = get(type_ref, "name")?;
        if !truthy(type_name) {
            return Err(DefinitionError::plain(format!(
                "Unknown type reference: {}.",
                inspect_member(type_ref)
            )));
        }

        let type_name = to_property_key(type_name);
        self.type_map()?.get(&type_name).cloned().ok_or_else(|| {
            DefinitionError::plain(format!(
                "Invalid or incomplete schema, unknown type: {type_name}. Ensure that a full introspection query is used in order to build a client schema."
            ))
        })
    }

    /// Returns the object type a reference names.
    ///
    /// Derived from graphql-js 16.12.0, src/utilities/buildClientSchema.ts, MIT.
    fn get_object_type(&self, type_ref: Option<&JsValue>) -> Result<NamedType, DefinitionError> {
        let ty = Type::from(self.get_named_type(type_ref)?);
        Ok(NamedType::Object(Arc::clone(assert_object_type(&ty)?)))
    }

    /// Returns the interface type a reference names.
    ///
    /// Derived from graphql-js 16.12.0, src/utilities/buildClientSchema.ts, MIT.
    fn get_interface_type(&self, type_ref: Option<&JsValue>) -> Result<NamedType, DefinitionError> {
        let ty = Type::from(self.get_named_type(type_ref)?);
        Ok(NamedType::Interface(Arc::clone(assert_interface_type(
            &ty,
        )?)))
    }

    /// Given a type's introspection result, construct the correct type
    /// instance.
    ///
    /// The kind is compared as the `switch` compares it, by strict equality
    /// with a string.
    ///
    /// Derived from graphql-js 16.12.0, src/utilities/buildClientSchema.ts, MIT.
    fn build_type(self: &Arc<Self>, ty: &JsValue) -> Result<NamedType, DefinitionError> {
        let present = Some(ty);
        if !is_nullish(present) && !is_nullish(get(present, "name")?) {
            let kind = get(present, "kind")?;
            if !is_nullish(kind) {
                match string_value(kind) {
                    Some("SCALAR") => return Ok(NamedType::from(Arc::new(build_scalar_def(ty)?))),
                    Some("OBJECT") => {
                        return Ok(NamedType::from(Arc::new(self.build_object_def(ty)?)));
                    }
                    Some("INTERFACE") => {
                        return Ok(NamedType::from(Arc::new(self.build_interface_def(ty)?)));
                    }
                    Some("UNION") => {
                        return Ok(NamedType::from(Arc::new(self.build_union_def(ty)?)));
                    }
                    Some("ENUM") => return Ok(NamedType::from(Arc::new(build_enum_def(ty)?))),
                    Some("INPUT_OBJECT") => {
                        return Ok(NamedType::from(Arc::new(self.build_input_object_def(ty)?)));
                    }
                    _ => {}
                }
            }
        }
        Err(DefinitionError::plain(format!(
            "Invalid or incomplete introspection result. Ensure that a full introspection query is used in order to build a client schema: {}.",
            inspect(ty)
        )))
    }

    /// Returns the interfaces an object or an interface type implements.
    ///
    /// Runs inside the interfaces thunk of the type.
    ///
    /// Derived from graphql-js 16.12.0, src/utilities/buildClientSchema.ts, MIT.
    fn build_implementations_list(
        &self,
        implementing_introspection: &JsValue,
    ) -> Result<Vec<NamedType>, DefinitionError> {
        let interfaces = member(implementing_introspection, "interfaces");
        // A server that predates interfaces on interfaces answers null for
        // the interfaces of an interface.
        if matches!(interfaces, Some(JsValue::Null))
            && is_string(member(implementing_introspection, "kind"), "INTERFACE")
        {
            return Ok(Vec::new());
        }

        if !truthy(interfaces) {
            return Err(DefinitionError::plain(format!(
                "Introspection result missing interfaces: {}.",
                inspect(implementing_introspection)
            )));
        }

        array_for(interfaces, "implementingIntrospection.interfaces.map")?
            .iter()
            .map(|type_ref| self.get_interface_type(Some(type_ref)))
            .collect()
    }

    /// Builds an object type whose interfaces and fields are thunks.
    ///
    /// Derived from graphql-js 16.12.0, src/utilities/buildClientSchema.ts, MIT.
    fn build_object_def(
        self: &Arc<Self>,
        object_introspection: &JsValue,
    ) -> Result<ObjectType, DefinitionError> {
        let (interfaces, fields) = self.implementing_thunks(object_introspection);
        ObjectType::new(ObjectTypeConfig {
            name: assert_name_value(member(object_introspection, "name"))?,
            description: optional_string(member(object_introspection, "description")),
            interfaces,
            fields,
            ..ObjectTypeConfig::default()
        })
    }

    /// Builds an interface type whose interfaces and fields are thunks.
    ///
    /// Derived from graphql-js 16.12.0, src/utilities/buildClientSchema.ts, MIT.
    fn build_interface_def(
        self: &Arc<Self>,
        interface_introspection: &JsValue,
    ) -> Result<InterfaceType, DefinitionError> {
        let (interfaces, fields) = self.implementing_thunks(interface_introspection);
        InterfaceType::new(InterfaceTypeConfig {
            name: assert_name_value(member(interface_introspection, "name"))?,
            description: optional_string(member(interface_introspection, "description")),
            interfaces,
            fields,
            ..InterfaceTypeConfig::default()
        })
    }

    /// Returns the two thunks of an object or an interface type, its
    /// interfaces and its fields, each over its own handle on the result.
    fn implementing_thunks(
        self: &Arc<Self>,
        type_introspection: &JsValue,
    ) -> (ThunkReadonlyArray<NamedType>, ThunkObjMap<FieldConfig>) {
        let source = Arc::new(type_introspection.clone());
        let scope = Arc::clone(self);
        let interfaces_source = Arc::clone(&source);
        let interfaces =
            ThunkReadonlyArray::thunk(move || scope.build_implementations_list(&interfaces_source));
        let scope = Arc::clone(self);
        let fields = ThunkObjMap::thunk(move || scope.build_field_def_map(&source));
        (interfaces, fields)
    }

    /// Builds a union type whose members are a thunk.
    ///
    /// Derived from graphql-js 16.12.0, src/utilities/buildClientSchema.ts, MIT.
    fn build_union_def(
        self: &Arc<Self>,
        union_introspection: &JsValue,
    ) -> Result<UnionType, DefinitionError> {
        if !truthy(member(union_introspection, "possibleTypes")) {
            return Err(DefinitionError::plain(format!(
                "Introspection result missing possibleTypes: {}.",
                inspect(union_introspection)
            )));
        }
        let source = Arc::new(union_introspection.clone());
        let scope = Arc::clone(self);
        UnionType::new(UnionTypeConfig {
            name: assert_name_value(member(union_introspection, "name"))?,
            description: optional_string(member(union_introspection, "description")),
            types: ThunkReadonlyArray::thunk(move || {
                array_for(
                    member(&source, "possibleTypes"),
                    "unionIntrospection.possibleTypes.map",
                )?
                .iter()
                .map(|type_ref| scope.get_object_type(Some(type_ref)))
                .collect()
            }),
            ..UnionTypeConfig::default()
        })
    }

    /// Builds an input object type whose fields are a thunk.
    ///
    /// The thunk refuses to be entered while it runs, which section Recursion
    /// of the module documentation explains.
    ///
    /// Derived from graphql-js 16.12.0, src/utilities/buildClientSchema.ts, MIT.
    fn build_input_object_def(
        self: &Arc<Self>,
        input_object_introspection: &JsValue,
    ) -> Result<InputObjectType, DefinitionError> {
        if !truthy(member(input_object_introspection, "inputFields")) {
            return Err(DefinitionError::plain(format!(
                "Introspection result missing inputFields: {}.",
                inspect(input_object_introspection)
            )));
        }
        let source = Arc::new(input_object_introspection.clone());
        let scope = Arc::clone(self);
        let resolving = AtomicBool::new(false);
        InputObjectType::new(InputObjectTypeConfig {
            name: assert_name_value(member(input_object_introspection, "name"))?,
            description: optional_string(member(input_object_introspection, "description")),
            fields: ThunkObjMap::thunk(move || {
                let _entered = Entered::enter(&resolving)?;
                let input_fields =
                    scope.build_input_value_def_map(member(&source, "inputFields"))?;
                Ok(input_fields
                    .into_iter()
                    .map(|(name, input_value)| (name, input_value.into_input_field()))
                    .collect())
            }),
            is_one_of: truthy(member(input_object_introspection, "isOneOf")),
            ..InputObjectTypeConfig::default()
        })
    }

    /// Builds the field map of an object or an interface type.
    ///
    /// Runs inside the fields thunk of the type.
    ///
    /// Derived from graphql-js 16.12.0, src/utilities/buildClientSchema.ts, MIT.
    fn build_field_def_map(
        &self,
        type_introspection: &JsValue,
    ) -> Result<ObjMap<FieldConfig>, DefinitionError> {
        let fields = member(type_introspection, "fields");
        if !truthy(fields) {
            return Err(DefinitionError::plain(format!(
                "Introspection result missing fields: {}.",
                inspect(type_introspection)
            )));
        }

        key_val_map(fields, |field_introspection| {
            self.build_field(field_introspection)
        })
    }

    /// Builds the config of one field.
    ///
    /// A type prints in a message the way its `toJSON` prints it, which is its
    /// `Display`.
    ///
    /// Derived from graphql-js 16.12.0, src/utilities/buildClientSchema.ts, MIT.
    fn build_field(&self, field_introspection: &JsValue) -> Result<FieldConfig, DefinitionError> {
        let ty = self.get_type(member(field_introspection, "type"))?;
        if !is_output_type(&ty) {
            return Err(DefinitionError::plain(format!(
                "Introspection must provide output type for fields, but received: {ty}."
            )));
        }

        let args = member(field_introspection, "args");
        if !truthy(args) {
            return Err(DefinitionError::plain(format!(
                "Introspection result missing field args: {}.",
                inspect(field_introspection)
            )));
        }

        Ok(FieldConfig {
            description: optional_string(member(field_introspection, "description")),
            deprecation_reason: optional_string(member(field_introspection, "deprecationReason")),
            args: self
                .build_input_value_def_map(args)?
                .into_iter()
                .map(|(name, input_value)| (name, input_value.into_argument()))
                .collect(),
            ..FieldConfig::of_type(ty)
        })
    }

    /// Builds the map of the arguments or the input fields of a list.
    ///
    /// Derived from graphql-js 16.12.0, src/utilities/buildClientSchema.ts, MIT.
    fn build_input_value_def_map(
        &self,
        input_value_introspections: Option<&JsValue>,
    ) -> Result<ObjMap<InputValue>, DefinitionError> {
        key_val_map(input_value_introspections, |input_value| {
            self.build_input_value(input_value)
        })
    }

    /// Builds one argument or input field.
    ///
    /// A default value that is not null is parsed as a value literal and read
    /// against the type, and a literal the type does not accept reads as no
    /// default. The parse runs under the bound of [`ends_the_stack`], which
    /// throws the stack error of the reference where its parser ends its
    /// stack.
    ///
    /// Derived from graphql-js 16.12.0, src/utilities/buildClientSchema.ts, MIT.
    fn build_input_value(
        &self,
        input_value_introspection: &JsValue,
    ) -> Result<InputValue, DefinitionError> {
        let ty = self.get_type(member(input_value_introspection, "type"))?;
        if !is_input_type(&ty) {
            return Err(DefinitionError::plain(format!(
                "Introspection must provide input type for arguments, but received: {ty}."
            )));
        }

        let default_value = match member(input_value_introspection, "defaultValue") {
            None | Some(JsValue::Null) => JsValue::Undefined,
            Some(default_value) => {
                let value_node = parse_value_within(
                    source_of(default_value)?,
                    ParseOptions::default(),
                    ends_the_stack,
                )
                .map_err(|error| match error {
                    ParseValueError::Syntax(error) => DefinitionError::from(*error),
                    ParseValueError::Nesting => {
                        DefinitionError::plain("Maximum call stack size exceeded")
                    }
                })?;
                value_from_ast(Some(&value_node), &ty, None)?
            }
        };
        Ok(InputValue {
            description: optional_string(member(input_value_introspection, "description")),
            ty,
            default_value,
            deprecation_reason: optional_string(member(
                input_value_introspection,
                "deprecationReason",
            )),
        })
    }

    /// Builds one directive.
    ///
    /// The members are read in the order the object literal of the reference
    /// reads them, the arguments are built before the constructor checks the
    /// name, and the locations are read as [`DirectiveLocation`] once every
    /// check of the reference has passed.
    ///
    /// Derived from graphql-js 16.12.0, src/utilities/buildClientSchema.ts, MIT.
    /// Derived from graphql-js 16.12.0, src/type/directives.ts, MIT.
    fn build_directive(
        &self,
        directive_introspection: &JsValue,
    ) -> Result<Arc<Directive>, DefinitionError> {
        let args = get(Some(directive_introspection), "args")?;
        if !truthy(args) {
            return Err(DefinitionError::plain(format!(
                "Introspection result missing directive args: {}.",
                inspect(directive_introspection)
            )));
        }
        let locations = member(directive_introspection, "locations");
        if !truthy(locations) {
            return Err(DefinitionError::plain(format!(
                "Introspection result missing directive locations: {}.",
                inspect(directive_introspection)
            )));
        }

        // A string has a `slice` as an array has, and fails the check of the
        // constructor after its name.
        let locations = match locations {
            Some(JsValue::Array(locations)) => Some(locations.as_slice()),
            Some(JsValue::String(_)) => None,
            _ => {
                return Err(DefinitionError::plain(
                    "directiveIntrospection.locations.slice is not a function",
                ));
            }
        };
        let args: ObjMap<ArgumentConfig> = self
            .build_input_value_def_map(args)?
            .into_iter()
            .map(|(name, input_value)| (name, input_value.into_argument()))
            .collect();

        let name = assert_name_value(member(directive_introspection, "name"))?;
        let Some(locations) = locations else {
            return Err(DefinitionError::plain(format!(
                "@{name} locations must be an Array."
            )));
        };
        define_arguments(&args)?;
        let (locations, location_names) = directive_locations(locations);

        Ok(Arc::new(Directive::new(DirectiveConfig {
            name,
            description: optional_string(member(directive_introspection, "description")),
            locations,
            location_names,
            args,
            is_repeatable: truthy(member(directive_introspection, "isRepeatable")),
            extensions: JsObject::new(),
            ast_node: None,
        })?))
    }
}

/// The parts of an argument and of an input field, which the reference
/// builds with one function.
struct InputValue {
    description: Option<String>,
    ty: Type,
    default_value: JsValue,
    deprecation_reason: Option<String>,
}

impl InputValue {
    /// Returns the parts as the config of an argument.
    fn into_argument(self) -> ArgumentConfig {
        ArgumentConfig {
            description: self.description,
            default_value: self.default_value,
            deprecation_reason: self.deprecation_reason,
            ..ArgumentConfig::of_type(self.ty)
        }
    }

    /// Returns the parts as the config of an input field.
    fn into_input_field(self) -> InputFieldConfig {
        InputFieldConfig {
            description: self.description,
            default_value: self.default_value,
            deprecation_reason: self.deprecation_reason,
            ..InputFieldConfig::of_type(self.ty)
        }
    }
}

/// The mark of a field thunk of an input object that runs.
///
/// The flag is set while the thunk runs and cleared when the mark drops, on
/// the error path as on the success path, so a thunk that fails runs again
/// on the next read, as every thunk of the crate does.
struct Entered<'a>(&'a AtomicBool);

impl<'a> Entered<'a> {
    /// Marks the thunk of `resolving` as running.
    ///
    /// # Errors
    ///
    /// Returns the plain error `Maximum call stack size exceeded` when the
    /// thunk runs already, which section Recursion of the module
    /// documentation explains.
    fn enter(resolving: &'a AtomicBool) -> Result<Self, DefinitionError> {
        if resolving.load(Ordering::Relaxed) {
            return Err(DefinitionError::plain("Maximum call stack size exceeded"));
        }
        resolving.store(true, Ordering::Relaxed);
        Ok(Self(resolving))
    }
}

impl Drop for Entered<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Relaxed);
    }
}

/// Builds a scalar type with no coercion of its own.
///
/// Derived from graphql-js 16.12.0, src/utilities/buildClientSchema.ts, MIT.
/// Derived from graphql-js 16.12.0, src/type/definition.ts, MIT.
fn build_scalar_def(scalar_introspection: &JsValue) -> Result<ScalarType, DefinitionError> {
    let name = assert_name_value(member(scalar_introspection, "name"))?;
    let specified_by_url = match member(scalar_introspection, "specifiedByURL") {
        None | Some(JsValue::Null) => None,
        Some(JsValue::String(url)) => Some(text_of(url)),
        Some(other) => {
            return Err(DefinitionError::plain(format!(
                "{name} must provide \"specifiedByURL\" as a string, but got: {}.",
                inspect(other)
            )));
        }
    };
    ScalarType::new(ScalarTypeConfig {
        name,
        description: optional_string(member(scalar_introspection, "description")),
        specified_by_url,
        ..ScalarTypeConfig::default()
    })
}

/// Builds an enum type, whose values are read at once.
///
/// The value map is built before the constructor asserts the name of the type,
/// as the object literal of the reference is evaluated before the constructor
/// runs.
///
/// Derived from graphql-js 16.12.0, src/utilities/buildClientSchema.ts, MIT.
fn build_enum_def(enum_introspection: &JsValue) -> Result<EnumType, DefinitionError> {
    let enum_values = member(enum_introspection, "enumValues");
    if !truthy(enum_values) {
        return Err(DefinitionError::plain(format!(
            "Introspection result missing enumValues: {}.",
            inspect(enum_introspection)
        )));
    }
    let values = key_val_map(enum_values, |value_introspection| {
        Ok(EnumValueConfig {
            description: optional_string(member(value_introspection, "description")),
            deprecation_reason: optional_string(member(value_introspection, "deprecationReason")),
            ..EnumValueConfig::default()
        })
    })?;
    EnumType::new(EnumTypeConfig {
        name: assert_name_value(member(enum_introspection, "name"))?,
        description: optional_string(member(enum_introspection, "description")),
        values: ThunkObjMap::Map(values),
        ..EnumTypeConfig::default()
    })
}

/// Creates a keyed map from a list, keyed by the `name` member of each item.
///
/// The key is read before `val_fn` runs, so an item that is `null` fails on
/// the read of its name, and a key that repeats keeps the place of its first
/// item with the value of its last, which is the assignment of the reference.
///
/// Derived from graphql-js 16.12.0, src/jsutils/keyValMap.ts, MIT.
fn key_val_map<V>(
    list: Option<&JsValue>,
    mut val_fn: impl FnMut(&JsValue) -> Result<V, DefinitionError>,
) -> Result<ObjMap<V>, DefinitionError> {
    let mut result = ObjMap::default();
    for item in iterate(list)?.iter() {
        let key = to_property_key(get(Some(item), "name")?);
        result.insert(key, val_fn(item)?);
    }
    Ok(result)
}

/// Returns the items `for...of` visits in a value.
///
/// An array yields its elements and a string its code points, each as a
/// string. Every other value is the `TypeError` of V8, which names the
/// parameter of `keyValMap`, the one place this file iterates.
fn iterate(list: Option<&JsValue>) -> Result<Cow<'_, [JsValue]>, DefinitionError> {
    match list {
        Some(JsValue::Array(items)) => Ok(Cow::Borrowed(items)),
        Some(JsValue::String(text)) => Ok(Cow::Owned(
            text_of(text)
                .chars()
                .map(|c| JsValue::String(JsString::from(c)))
                .collect(),
        )),
        _ => Err(DefinitionError::plain("list is not iterable")),
    }
}

/// Returns the elements of an array the reference calls a method of.
///
/// The argument `expression` is the method the reference calls, as V8
/// names it in the `TypeError` for a value that has no such method.
fn array_for<'a>(
    list: Option<&'a JsValue>,
    expression: &str,
) -> Result<&'a [JsValue], DefinitionError> {
    match list {
        Some(JsValue::Array(items)) => Ok(items),
        _ => Err(DefinitionError::plain(format!(
            "{expression} is not a function"
        ))),
    }
}

/// Reads the member `key` of a value, the way a property access does.
///
/// A `base` of `None` is `undefined`. A read on `undefined` or on `null` is
/// the `TypeError` of V8. A read on a string, a number or a boolean is
/// `undefined`, because none of them has a member of the names this file
/// reads, and so is a read on an array.
fn get<'a>(base: Option<&'a JsValue>, key: &str) -> Result<Option<&'a JsValue>, DefinitionError> {
    match base {
        None | Some(JsValue::Undefined) => Err(DefinitionError::plain(format!(
            "Cannot read properties of undefined (reading '{key}')"
        ))),
        Some(JsValue::Null) => Err(DefinitionError::plain(format!(
            "Cannot read properties of null (reading '{key}')"
        ))),
        Some(base) => Ok(member(base, key)),
    }
}

/// Reads the member `key` of a value that is neither `undefined` nor `null`.
///
/// An absent member and a member that is `undefined` are both `None`.
fn member<'a>(base: &'a JsValue, key: &str) -> Option<&'a JsValue> {
    match base {
        JsValue::Object(object) => object.get(key).filter(|value| !value.is_undefined()),
        _ => None,
    }
}

/// Returns whether a value is object like, which is an array or an object.
///
/// Derived from graphql-js 16.12.0, src/jsutils/isObjectLike.ts, MIT.
fn is_object_like(value: Option<&JsValue>) -> bool {
    matches!(
        value,
        Some(JsValue::Array(_) | JsValue::Object(_) | JsValue::Opaque(_))
    )
}

/// Returns whether a value is `null` or `undefined`, which is `value ==
/// null`.
fn is_nullish(value: Option<&JsValue>) -> bool {
    matches!(value, None | Some(JsValue::Undefined | JsValue::Null))
}

/// Returns the truthiness of a value, which is `ToBoolean`.
///
/// The values `false`, a zero, `NaN`, the empty string, `null` and `undefined`
/// are false and every other value is true.
///
/// [toboolean]: https://tc39.es/ecma262/#sec-toboolean
fn truthy(value: Option<&JsValue>) -> bool {
    match value {
        None | Some(JsValue::Undefined | JsValue::Null) => false,
        Some(JsValue::Bool(value)) => *value,
        Some(JsValue::Number(number)) => !(*number == 0.0 || number.is_nan()),
        Some(JsValue::String(text)) => !text.is_empty(),
        Some(JsValue::Array(_) | JsValue::Object(_) | JsValue::Opaque(_)) => true,
    }
}

/// Returns whether a value is the string `expected`, which is `===` with a
/// string.
fn is_string(value: Option<&JsValue>, expected: &str) -> bool {
    string_value(value) == Some(expected)
}

/// Returns the text of a value that is a string with no lone surrogate.
fn string_value(value: Option<&JsValue>) -> Option<&str> {
    match value {
        Some(JsValue::String(text)) => text.as_str(),
        _ => None,
    }
}

/// Returns the string `String(value)` makes of a value, which is also the
/// property key `ToPropertyKey` makes of it.
///
/// An array joins its elements with a comma, an element that is `null` or
/// `undefined` as the empty string, and an object is `[object Object]`.
///
/// [tostring]: https://tc39.es/ecma262/#sec-tostring
/// [join]: https://tc39.es/ecma262/#sec-array.prototype.join
fn to_property_key(value: Option<&JsValue>) -> String {
    match value {
        None | Some(JsValue::Undefined) => String::from("undefined"),
        Some(JsValue::Null) => String::from("null"),
        Some(JsValue::Bool(value)) => String::from(if *value { "true" } else { "false" }),
        Some(JsValue::Number(number)) => number_to_string(*number),
        Some(JsValue::String(text)) => text_of(text),
        Some(JsValue::Array(items)) => {
            let parts: Vec<String> = items
                .iter()
                .map(|item| match item {
                    JsValue::Undefined | JsValue::Null => String::new(),
                    item => to_property_key(Some(item)),
                })
                .collect();
            parts.join(",")
        }
        Some(JsValue::Object(_) | JsValue::Opaque(_)) => String::from("[object Object]"),
    }
}

/// Returns the text of a string, with one replacement character for each
/// lone surrogate.
fn text_of(text: &JsString) -> String {
    to_string_lossy(text).into_owned()
}

/// Returns a member that is a string, and `None` for every other value.
///
/// The reading of a description and a deprecation reason, which section
/// Reading a plain object of the module documentation explains.
fn optional_string(value: Option<&JsValue>) -> Option<String> {
    match value {
        Some(JsValue::String(text)) => Some(text_of(text)),
        _ => None,
    }
}

/// Prints a member that may be absent the way `inspect` prints `undefined`
/// for it.
fn inspect_member(value: Option<&JsValue>) -> String {
    match value {
        Some(value) => inspect(value),
        None => inspect(&JsValue::Undefined),
    }
}

/// Asserts the name a constructor of the reference receives.
fn assert_name_value(name: Option<&JsValue>) -> Result<String, DefinitionError> {
    match name {
        None | Some(JsValue::Undefined | JsValue::Null) => {
            Err(DefinitionError::plain("Must provide name."))
        }
        Some(JsValue::String(text)) => assert_name(&text_of(text)),
        Some(_) => Err(DefinitionError::plain("Expected name to be a string.")),
    }
}

/// Returns the source of a default value the parser reads.
fn source_of(default_value: &JsValue) -> Result<Source, DefinitionError> {
    match default_value {
        JsValue::String(body) => Ok(Source::from(body.clone())),
        other => Err(DefinitionError::plain(format!(
            "Body must be a string. Received: {}.",
            inspect(other)
        ))),
    }
}

/// The stack the parser of the reference spends on one list of a value that
/// has opened no input object, in the units of [`VALUE_STACK`].
const LIST_LEVEL: u64 = 1331;

/// The stack the parser of the reference spends on one list of a value that
/// has opened an input object, in the units of [`VALUE_STACK`].
const LIST_BESIDE_OBJECT: u64 = 1389;

/// The stack the parser of the reference spends on one input object of a
/// value, in the units of [`VALUE_STACK`].
const OBJECT_LEVEL: u64 = 1747;

/// The stack the parser of the reference holds for the nesting of a default
/// value, less than the cost of 1,747 lists and of 1,331 input objects.
///
/// The parser of the reference reads a value by recursion, and its stack ends
/// with the `RangeError` `Maximum call stack size exceeded` inside
/// `buildClientSchema`. A served front of the reference builds the client
/// schema of a remote whose default value nests 1,746 lists and throws for
/// 1,747, and builds one whose default value nests 1,330 input objects and
/// throws for 1,331. A value that nests both costs the reference more than the
/// sum of those levels. Every list it holds open spends more once the value
/// has opened an input object, the lists opened before that object among them,
/// and an input object spends the same: the front serves 741 pairs of a list
/// and an input object and throws for 742, and serves 1,000 lists around 535
/// input objects and throws for 536.
const VALUE_STACK: u64 = 1747 * LIST_LEVEL - 1;

/// Returns whether the parser of the reference ends its stack with the
/// values `open` open, the bound [`parse_value_within`] reads at each list
/// and input object it opens.
///
/// The parse of the port stops at that value, so its recursion holds no more
/// levels than the reference holds, and a syntax error the text holds before
/// the value is thrown first, as the reference throws it.
fn ends_the_stack(open: &Nesting) -> bool {
    let list = if open.object_opened {
        LIST_BESIDE_OBJECT
    } else {
        LIST_LEVEL
    };
    u64::from(open.lists) * list + u64::from(open.objects) * OBJECT_LEVEL > VALUE_STACK
}

/// Returns the locations the items of a list name, and the names of all of
/// them in order when one names no [`DirectiveLocation`].
///
/// The reference keeps every item of the list as it is, and the enum of this
/// crate holds the locations of graphql-js 16.12.0 alone, so an item that
/// names another location stands among the names alone, which section
/// Reading a plain object of the module documentation explains. An item
/// that is no string is the text `Array.prototype.join` makes of it, the
/// empty string for `null`, which a printed schema writes where the
/// reference joins the item.
fn directive_locations(items: &[JsValue]) -> (Vec<DirectiveLocation>, Option<Vec<String>>) {
    let mut locations = Vec::with_capacity(items.len());
    let mut unknown = false;
    for item in items {
        match string_value(Some(item)).and_then(DirectiveLocation::from_name) {
            Some(location) => locations.push(location),
            None => unknown = true,
        }
    }
    let names = unknown.then(|| {
        items
            .iter()
            .map(|item| match item {
                JsValue::Undefined | JsValue::Null => String::new(),
                item => to_property_key(Some(item)),
            })
            .collect()
    });
    (locations, names)
}

/// Returns the plain error of a wrapping type with no inner type.
fn decorated_too_deep() -> DefinitionError {
    DefinitionError::plain("Decorated type deeper than introspection query.")
}
