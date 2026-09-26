// SPDX-License-Identifier: MIT
/*
 * aosabi-type: the hooks a caller binds to the types of a schema while the
 * schema is built.
 *
 * No analogue in graphql-js.
 *
 * Derived from @graphql-tools/schema 10.0.31, cjs/addResolversToSchema.js, MIT.
 */

//! The bindings a schema is built with.
//!
//! `makeExecutableSchema` builds the schema of an SDL text and lays the
//! resolver map over the finished schema afterwards, writing a specified
//! scalar in place and building every other bound type a second time from its
//! config with the hooks of the map set on it. A type of this crate sits
//! behind an `Arc` and takes its hooks at construction, so the map enters the
//! one build instead: the builders of [󰀄](mod@crate::extend_schema) read the
//! bindings of a type and of a field at the moment they build its config, and
//! [󰀅](crate::build_schema_with_bindings) is the entry that hands them in.
//!
//! A binding that names no type, or a field no type has, is ignored here. The
//! validation pass of the caller refuses such an entry before the build, so
//! the builders never see it.

use alloc::string::String;

use aosabi_json::JsObject;

use crate::definition::{
    FieldConfig,
    FieldResolver,
    IsTypeOfFn,
    ObjMap,
    ScalarLiteralParser,
    ScalarSerializer,
    ScalarTypeConfig,
    ScalarValueParser,
    TypeResolver, //
};

/// The members of a scalar instance a resolver map binds over a scalar of
/// the schema.
///
/// The reference copies every enumerable property of the instance over the
/// config the scalar was built from, so a description or a URL the instance
/// holds as `undefined` clears what the SDL declared. That is the outer
/// `Option` of [󰀃](Self::description) and of [󰀄](Self::specified_by_url):
/// `None` leaves the built value alone and `Some(None)` clears it.
/// `extensions` is merged over the built extensions, key by key.
#[derive(Clone, Debug, Default)]
pub struct ScalarBinding {
    /// The description, where `Some(None)` clears the built one.
    pub description: Option<Option<String>>,
    /// The URL of the specification, where `Some(None)` clears the built one.
    pub specified_by_url: Option<Option<String>>,
    /// The serializer.
    pub serialize: Option<ScalarSerializer>,
    /// The value parser.
    pub parse_value: Option<ScalarValueParser>,
    /// The literal parser.
    pub parse_literal: Option<ScalarLiteralParser>,
    /// The extensions, merged over the built ones.
    pub extensions: JsObject,
}

impl ScalarBinding {
    /// Returns `config` with this binding laid over it.
    ///
    /// The two differ in what happens to the extensions, which the loop
    /// replaces and the mapper merges, and in the definition nodes, which the
    /// loop replaces by the instance's and the mapper keeps. A specified
    /// scalar of this crate carries empty extensions and a binding carries no
    /// node, so one reading serves both.
    #[must_use]
    pub fn apply(&self, config: ScalarTypeConfig) -> ScalarTypeConfig {
        let mut extensions = config.extensions;
        for (key, value) in self.extensions.iter() {
            extensions.insert(key.clone(), value.clone());
        }
        ScalarTypeConfig {
            description: self.description.clone().unwrap_or(config.description),
            specified_by_url: self
                .specified_by_url
                .clone()
                .unwrap_or(config.specified_by_url),
            serialize: self.serialize.clone().or(config.serialize),
            parse_value: self.parse_value.clone().or(config.parse_value),
            parse_literal: self.parse_literal.clone().or(config.parse_literal),
            extensions,
            ..config
        }
    }
}

/// The `resolve` and `subscribe` functions a resolver map binds to a field.
///
/// The reference copies every own property of the object under a field name
/// over the field config, and a function under that name is the `resolve`
/// alone. A slot that is `None` is a property the object does not have, so
/// the built value stays.
#[derive(Clone, Debug, Default)]
pub struct FieldBinding {
    /// The resolver.
    pub resolve: Option<FieldResolver>,
    /// The subscription resolver.
    pub subscribe: Option<FieldResolver>,
}

impl FieldBinding {
    /// Returns `config` with this binding laid over it.
    #[must_use]
    pub fn apply(&self, config: FieldConfig) -> FieldConfig {
        FieldConfig {
            resolve: self.resolve.clone().or(config.resolve),
            subscribe: self.subscribe.clone().or(config.subscribe),
            ..config
        }
    }
}

/// Everything a resolver map binds, one map per kind of entry.
///
/// A resolver map of the reference is one object keyed by type name whose
/// values mix field resolvers, the members `__resolveType` and `__isTypeOf`
/// and scalar instances. The builders read each kind from the map of its
/// kind, keyed by the type name and, for a field, by the field name after
/// it.
#[derive(Clone, Debug, Default)]
pub struct SchemaBindings {
    /// The scalar instances, by type name.
    pub scalars: ObjMap<ScalarBinding>,
    /// The field bindings, by type name and then by field name.
    pub fields: ObjMap<ObjMap<FieldBinding>>,
    /// The member `__resolveType` of an interface or a union, by type name.
    pub resolve_types: ObjMap<TypeResolver>,
    /// The member `__isTypeOf` of an object type, by type name.
    pub is_type_ofs: ObjMap<IsTypeOfFn>,
}

impl SchemaBindings {
    /// Returns `config` of the scalar `name` with its binding laid over it,
    /// or `config` itself when nothing is bound to that name.
    #[must_use]
    pub fn bind_scalar(&self, name: &str, config: ScalarTypeConfig) -> ScalarTypeConfig {
        match self.scalars.get(name) {
            Some(binding) => binding.apply(config),
            None => config,
        }
    }

    /// Lays the field bindings of the type `type_name` over `fields`.
    ///
    /// Runs over every field of every object and interface type and changes
    /// the ones the map names.
    pub fn bind_fields(&self, type_name: &str, fields: &mut ObjMap<FieldConfig>) {
        let Some(bindings) = self.fields.get(type_name) else {
            return;
        };
        for (field_name, binding) in bindings {
            if let Some(config) = fields.get_mut(field_name) {
                *config = binding.apply(config.clone());
            }
        }
    }

    /// Returns the `resolveType` of the interface or union `name`, `built`
    /// when the map binds none.
    #[must_use]
    pub fn resolve_type(&self, name: &str, built: Option<TypeResolver>) -> Option<TypeResolver> {
        self.resolve_types.get(name).cloned().or(built)
    }

    /// Returns the `isTypeOf` of the object type `name`, `built` when the map
    /// binds none.
    #[must_use]
    pub fn is_type_of(&self, name: &str, built: Option<IsTypeOfFn>) -> Option<IsTypeOfFn> {
        self.is_type_ofs.get(name).cloned().or(built)
    }
}
