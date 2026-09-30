// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-core create_schema: the resolver map of a service and the schema
 * built from it.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/app/pylon-handler.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 *
 * Derived from @graphql-tools/schema 10.0.31, cjs/addResolversToSchema.js, MIT.
 * Derived from @graphql-tools/utils 11.0.0, cjs/mapSchema.js, MIT.
 */

//! The executable schema of a service.
//!
//! A TypeScript service builds its schema with `createSchema` of Yoga, which is
//! `makeExecutableSchema` of `@graphql-tools/schema`: the SDL is built into a
//! schema, and a resolver map is laid over it. The reference lays the map
//! over a finished schema by building the schema a second time. A type of
//! Aosabi sits behind an `Arc` and takes its hooks at construction, so the
//! map enters the one build through the binding seam of the type layer.
//!
//! A resolver map of the reference is one object keyed by type name, whose
//! values mix field resolvers with the members `__resolveType` and
//! `__isTypeOf` and with scalar instances. [`ResolverMap`] holds one map per
//! kind, so an entry has the type its kind needs.

use core::fmt;
use std::sync::Arc;

use aosabi_execute::execute::{
    IsTypeOf,
    IsTypeOfFn,
    ResolveType,
    TypeResolverFn, //
};
use aosabi_json::JsString;
use aosabi_lang::{
    ParseOptions,
    js_string::to_string_lossy, //
};
use aosabi_type::{
    BuildSchemaOptions,
    DefinitionError,
    NamedType,
    ObjMap,
    ResolverHandle,
    Schema,
    SchemaConfig,
    get_named_type,
    introspection_schema,
    is_specified_directive,
    is_specified_scalar_type,
    schema_bindings::{
        FieldBinding,
        ScalarBinding,
        SchemaBindings, //
    }, //
};
use aosabi_validate::build_schema::build_schema_with_bindings;

use crate::scalars::bound_scalars;

/// The enumerable members of a scalar instance, in the order its constructor
/// assigns them.
///
/// The validation pass walks the members of whatever a resolver map holds
/// under a type name, and for a scalar instance those are the properties of
/// `GraphQLScalarType`.
const SCALAR_INSTANCE_MEMBERS: [&str; 9] = [
    "name",
    "description",
    "specifiedByURL",
    "serialize",
    "parseValue",
    "parseLiteral",
    "extensions",
    "astNode",
    "extensionASTNodes",
];

/// The resolvers, the type resolvers and the scalars laid over an SDL text.
#[derive(Clone, Default)]
pub struct ResolverMap {
    /// The field bindings, by type name and then by field name.
    pub fields: ObjMap<ObjMap<FieldBinding>>,
    /// The member `__resolveType` of an interface or a union, by type name.
    pub resolve_types: ObjMap<TypeResolverFn>,
    /// The member `__isTypeOf` of an object type, by type name.
    pub is_type_ofs: ObjMap<IsTypeOfFn>,
    /// The scalars, by type name.
    pub scalars: ObjMap<ScalarBinding>,
}

/// What [`create_schema`] throws.
#[derive(Debug)]
pub enum CreateSchemaError {
    /// A plain `Error` of the resolver conversion or of the validation pass,
    /// carried as its message.
    Resolvers(JsString),
    /// What the schema build throws, a syntax error, an SDL rule or a
    /// definition the type layer refuses.
    Schema(DefinitionError),
}

impl CreateSchemaError {
    /// Returns the message of the error.
    #[must_use]
    pub const fn message(&self) -> &JsString {
        match self {
            Self::Resolvers(message) => message,
            Self::Schema(error) => error.message(),
        }
    }
}

impl fmt::Display for CreateSchemaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&to_string_lossy(self.message()))
    }
}

impl core::error::Error for CreateSchemaError {}

impl From<DefinitionError> for CreateSchemaError {
    fn from(error: DefinitionError) -> Self {
        Self::Schema(error)
    }
}

/// Merges two resolver maps one level deep, with `b` winning a conflict.
///
/// For a type present in both maps the field bindings are combined, so a
/// build side addition lives beside the resolvers of the application for the
/// same type. A key that exists keeps its position and takes the value of
/// `b`, which is what the spread of the reference does.
#[must_use]
pub fn merge_resolver_maps(a: ResolverMap, b: ResolverMap) -> ResolverMap {
    let mut out = a;
    for (type_name, fields) in b.fields {
        out.fields.entry(type_name).or_default().extend(fields);
    }
    out.resolve_types.extend(b.resolve_types);
    out.is_type_ofs.extend(b.is_type_ofs);
    out.scalars.extend(b.scalars);
    out
}

/// Builds the executable schema of a service.
///
/// `graphql` is what [`resolvers_to_graphql_resolvers`] made of the roots,
/// and `resolvers` is the build side map, which wins a conflict. The seven
/// bound scalars go over both, `GID` only when `type_defs` declares it.
///
/// # Errors
///
/// Returns what [`make_executable_schema`] returns.
///
/// [`resolvers_to_graphql_resolvers`]: crate::resolvers::resolvers_to_graphql_resolvers()
pub fn create_schema(
    type_defs: &str,
    graphql: ResolverMap,
    resolvers: ResolverMap,
) -> Result<Arc<Schema>, CreateSchemaError> {
    let mut map = merge_resolver_maps(graphql, resolvers);
    map.scalars.extend(bound_scalars(type_defs));
    make_executable_schema(type_defs, &map)
}

/// Builds the schema of `type_defs` with `resolvers` laid over it.
///
/// The schema is built first, so an error of the SDL comes before an error of
/// the map, as in the reference. The merge of several type definition sources
/// has no reading, because a service has one SDL text. The reference lays the
/// map over the schema by building it a second time through `mapSchema`, and
/// that second build changes the type map, so `map_schema` repeats it.
///
/// # Errors
///
/// Returns [`CreateSchemaError::Schema`] for an SDL that does not build, and
/// [`CreateSchemaError::Resolvers`] with the message of
/// `addResolversToSchema` for a map that names a type or a field the schema
/// lacks.
pub fn make_executable_schema(
    type_defs: &str,
    resolvers: &ResolverMap,
) -> Result<Arc<Schema>, CreateSchemaError> {
    let bindings = schema_bindings(resolvers);
    // A boot that read the schema record of its build holds it for this text,
    // whose schema it builds in the order of the type map of the second build
    // below, known valid.
    if let Some(built) = crate::boot::prebuilt_schema(type_defs, &bindings) {
        let (schema, declared) = built?;
        let types = |name: &str| {
            schema
                .get_type(name)
                .or_else(|| declared.iter().find(|ty| ty.name() == name))
        };
        check_resolvers(&types, resolvers)?;
        return Ok(Arc::new(schema));
    }
    from_sdl(type_defs, resolvers, &bindings)
}

/// Builds the schema of `type_defs` with `bindings` laid over it, checks
/// `resolvers` against it and builds it a second time from its own type map,
/// the body of [`make_executable_schema`] where no schema record stands for
/// the text, kept out of line, so a boot from a record compiles none of it.
///
/// # Errors
///
/// Returns what [`make_executable_schema`] returns.
#[inline(never)]
fn from_sdl(
    type_defs: &str,
    resolvers: &ResolverMap,
    bindings: &SchemaBindings,
) -> Result<Arc<Schema>, CreateSchemaError> {
    let schema = build_schema_with_bindings(
        type_defs,
        BuildSchemaOptions::default(),
        ParseOptions::default(),
        bindings,
    )?;
    validate_resolvers(&schema, resolvers)?;
    Ok(Arc::new(map_schema(&schema)?))
}

/// The type names `mapSchema` leaves out of the type list it rebuilds from.
const BUILTIN_TYPES: [&str; 5] = ["String", "ID", "Int", "Float", "Boolean"];

/// Builds `schema` a second time from its own type map.
///
/// `mapTypes` walks the type map in order and drops every name that starts
/// with two underscores and every name of [`BUILTIN_TYPES`], and
/// `rewireTypes` keeps that order. The fields of a rewired type are a thunk,
/// but the arguments of a directive that is not specified are rewired at
/// once, and a named type of such an argument that the map no longer holds is
/// appended to it, which is why the type list ends with the types the custom
/// directives name. The constructor then collects every other dropped type
/// again where a kept type, a directive or the introspection root reaches it.
/// So a specified scalar the SDL declares but nothing uses leaves the schema,
/// and one a field uses moves behind the declared types. The mappers of the
/// reference rebuild each type from its config, which changes no type here,
/// because every hook entered at the first build.
///
/// Derived from @graphql-tools/utils 11.0.0, cjs/mapSchema.js, MIT.
///
/// # Errors
///
/// Returns what the constructor of the schema throws.
fn map_schema(schema: &Schema) -> Result<Schema, DefinitionError> {
    let config = schema.to_config();
    let mut types: Vec<NamedType> = config
        .types
        .into_iter()
        .filter(|ty| !ty.name().starts_with("__") && !BUILTIN_TYPES.contains(&ty.name()))
        .collect();
    for directive in schema.get_directives() {
        if is_specified_directive(directive) {
            continue;
        }
        for arg in &directive.args {
            let named_type = get_named_type(&arg.r#type);
            if !types.iter().any(|ty| ty.name() == named_type.name()) {
                types.push(named_type);
            }
        }
    }
    Schema::new(SchemaConfig { types, ..config }, &introspection_schema()?)
}

/// Returns `resolvers` as the bindings the type layer takes.
///
/// The two function kinds of the executor go behind the handle of the type
/// layer, which is how the executor finds them again on a built type.
fn schema_bindings(resolvers: &ResolverMap) -> SchemaBindings {
    let resolve_types = resolvers.resolve_types.iter().map(|(name, function)| {
        let handle = ResolverHandle::new(ResolveType(Arc::clone(function)));
        (name.clone(), handle)
    });
    let is_type_ofs = resolvers.is_type_ofs.iter().map(|(name, function)| {
        let handle = ResolverHandle::new(IsTypeOf(Arc::clone(function)));
        (name.clone(), handle)
    });
    SchemaBindings {
        scalars: resolvers.scalars.clone(),
        fields: resolvers.fields.clone(),
        resolve_types: resolve_types.collect(),
        is_type_ofs: is_type_ofs.collect(),
    }
}

/// Checks that every entry of `resolvers` names something `schema` has.
///
/// The reference walks one object in key order. The four maps are walked one
/// after the other, fields first and scalars last, so where two entries of
/// different kinds are both wrong the first error may be the other one.
fn validate_resolvers(schema: &Schema, resolvers: &ResolverMap) -> Result<(), CreateSchemaError> {
    check_resolvers(&|name| schema.get_type(name), resolvers)
}

/// Checks that every entry of `resolvers` names something `types` answers,
/// the types of the schema the pass walks by their names.
///
/// The body of [`validate_resolvers`], over the type map of the first build
/// of the SDL path, or the types of a schema record with the specified
/// scalars its SDL declares and no field names, which that first build holds
/// and the second leaves out.
fn check_resolvers<'s>(
    types: &dyn Fn(&str) -> Option<&'s NamedType>,
    resolvers: &ResolverMap,
) -> Result<(), CreateSchemaError> {
    for (type_name, fields) in &resolvers.fields {
        let members: Vec<&str> = fields.keys().map(String::as_str).collect();
        validate_entry(types(type_name), type_name, &members)?;
    }
    let type_names = resolvers
        .resolve_types
        .keys()
        .chain(resolvers.is_type_ofs.keys());
    for type_name in type_names {
        // `__resolveType` and `__isTypeOf` start with two underscores, and
        // the pass skips such a member in every branch.
        validate_entry(types(type_name), type_name, &[])?;
    }
    for type_name in resolvers.scalars.keys() {
        validate_entry(types(type_name), type_name, &SCALAR_INSTANCE_MEMBERS)?;
    }
    Ok(())
}

/// Checks one entry of a resolver map, `named_type` being the type of the
/// schema it names and `members` its member names that do not start with two
/// underscores.
///
/// A custom scalar type and an input object type pass without a check.
fn validate_entry(
    named_type: Option<&NamedType>,
    type_name: &str,
    members: &[&str],
) -> Result<(), CreateSchemaError> {
    let Some(named_type) = named_type else {
        let message = format!("\"{type_name}\" defined in resolvers, but not in schema");
        return Err(CreateSchemaError::Resolvers(JsString::from(message)));
    };
    // The reference allows a map to override a specified scalar type and
    // does not recommend it, so such an entry passes without a check.
    if is_specified_scalar_type(named_type) {
        return Ok(());
    }
    let refused = members
        .iter()
        .find_map(|member| refuse_member(named_type, type_name, member));
    match refused {
        Some(message) => Err(CreateSchemaError::Resolvers(JsString::from(message))),
        None => Ok(()),
    }
}

/// Returns the message that refuses `member` on `named_type`, `None` for a
/// member the type has.
///
/// The enum and the union branch print the name of the type, the object
/// branch prints the key of the map, and the two are the same text.
fn refuse_member(named_type: &NamedType, type_name: &str, member: &str) -> Option<String> {
    match named_type {
        NamedType::Enum(enum_type) => {
            let values = enum_type.get_values().unwrap_or_default();
            if values.iter().any(|value| value.name == member) {
                return None;
            }
            let name = &enum_type.name;
            Some(format!(
                "{name}.{member} was defined in resolvers, but not present within {name}"
            ))
        }
        NamedType::Union(union_type) => {
            let name = &union_type.name;
            Some(format!(
                "{name}.{member} was defined in resolvers, but {name} is not an object or interface type"
            ))
        }
        NamedType::Object(_) | NamedType::Interface(_) => {
            let fields = match named_type {
                NamedType::Object(object) => object.get_fields().ok(),
                NamedType::Interface(interface) => interface.get_fields().ok(),
                _ => None,
            };
            if fields.is_some_and(|fields| fields.contains_key(member)) {
                return None;
            }
            // Field present in resolver but not in schema
            Some(format!(
                "{type_name}.{member} defined in resolvers, but not in schema"
            ))
        }
        NamedType::Scalar(_) | NamedType::InputObject(_) => None,
    }
}
