// SPDX-License-Identifier: MIT
/*
 * aosabi-type: the schema, the type map it collects and the lookups over it.
 *
 * Derived from graphql-js 16.12.0, src/type/schema.ts, MIT.
 */

//! The schema of the type layer.
//!
//! A [`Schema`] is built from its root types, an optional list of further
//! types and an optional list of directives, and its constructor collects
//! every reachable named type into a map in the order
//! `collectReferencedTypes` visits them. That order is what introspection
//! lists and what the printer walks, so it is kept exactly: the given types
//! first, each with the types it reaches, then the query, mutation and
//! subscription roots, then the argument types of every directive, and the
//! `__Schema` type last.
//!
//! The reference imports `__Schema` from its introspection module. The
//! constructor takes it as an argument here, because the introspection
//! types are a module of their own, and the caller passes the one shared
//! `__Schema` allocation of that module.
//!
//! The `isSchema` and `assertSchema` predicates of the reference narrow an
//! `unknown`, and the argument type carries that here, so neither has an
//! item.

use alloc::{
    boxed::Box,
    format,
    string::String,
    sync::Arc,
    vec::Vec, //
};
use core::fmt;

use aosabi_json::{
    JsObject,
    map::IndexMap,
    value::KeyHasherBuilder, //
};
use aosabi_lang::{
    GraphQLError,
    ast::{
        OperationTypeNode,
        SchemaDefinitionNode,
        SchemaExtensionNode, //
    }, //
};
use once_cell::race::OnceBox;

use crate::{
    definition::{
        DefinitionError,
        InterfaceType,
        NamedType,
        ObjMap,
        ObjectType,
        Type,
        get_named_type, //
    },
    directives::{
        Directive,
        specified_directives, //
    }, //
};

/// The configuration of a schema.
///
/// `directives` stays optional, because an absent list means the specified
/// directives and an empty list means none, and `types` is a list, because an
/// absent list and an empty list read the same.
///
/// Derived from graphql-js 16.12.0, src/type/schema.ts, MIT.
#[derive(Debug, Default)]
pub struct SchemaConfig {
    /// The description.
    pub description: Option<String>,
    /// The query root type.
    ///
    /// The reference types each root as an object type and stores whatever
    /// named type the config holds, and `validateSchema` reports a root that
    /// is not an object type, so each root holds any [`NamedType`].
    pub query: Option<NamedType>,
    /// The mutation root type.
    pub mutation: Option<NamedType>,
    /// The subscription root type.
    pub subscription: Option<NamedType>,
    /// Further types to include, in order.
    pub types: Vec<NamedType>,
    /// The directives, or the specified directives when absent.
    pub directives: Option<Vec<Arc<Directive>>>,
    /// The extensions.
    pub extensions: JsObject,
    /// The definition node the schema was built from.
    pub ast_node: Option<SchemaDefinitionNode>,
    /// The extension nodes the schema was built from.
    pub extension_ast_nodes: Vec<SchemaExtensionNode>,
    /// Whether the schema is known to be valid, which skips validation.
    pub assume_valid: bool,
}

/// The types that implement an interface.
#[derive(Debug, Default)]
struct Implementations {
    objects: Vec<Arc<ObjectType>>,
    interfaces: Vec<Arc<InterfaceType>>,
    /// The entries of `objects` as named types, which is the shape
    /// `getPossibleTypes` answers a union and an interface alike in.
    possible_types: Vec<NamedType>,
}

/// The types that implement an interface, borrowed from the schema.
#[derive(Debug, Clone, Copy)]
pub struct InterfaceImplementations<'a> {
    /// The object types that implement the interface.
    pub objects: &'a [Arc<ObjectType>],
    /// The interface types that implement the interface.
    pub interfaces: &'a [Arc<InterfaceType>],
}

/// Schema Definition.
///
/// A Schema is created by supplying the root types of each type of operation,
/// query and mutation (optional). A schema definition is then supplied to the
/// validator and executor.
///
/// Derived from graphql-js 16.12.0, src/type/schema.ts, MIT.
///
/// When the schema is constructed, by default only the types that are
/// reachable by traversing the root types are included, other types must be
/// explicitly referenced. If a list of directives is provided, that is the
/// exact list of directives represented and allowed, and otherwise the
/// specified directives are.
pub struct Schema {
    /// The description.
    pub description: Option<String>,
    /// The extensions.
    pub extensions: JsObject,
    /// The definition node the schema was built from.
    pub ast_node: Option<SchemaDefinitionNode>,
    /// The extension nodes the schema was built from.
    pub extension_ast_nodes: Vec<SchemaExtensionNode>,
    validation_errors: OnceBox<Vec<GraphQLError>>,
    query_type: Option<NamedType>,
    mutation_type: Option<NamedType>,
    subscription_type: Option<NamedType>,
    directives: Vec<Arc<Directive>>,
    type_map: ObjMap<NamedType>,
    sub_type_map: ObjMap<OnceBox<ObjMap<bool>>>,
    implementations_map: ObjMap<Implementations>,
}

impl Schema {
    /// Builds a schema from `config`.
    ///
    /// `introspection_schema` is the `__Schema` object type, which the
    /// reference imports and collects last. The three `devAssert` calls over
    /// the shapes of the config, the types and the directives have no
    /// reading, because the config carries the shapes, and `isDirective` is
    /// true of every directive here.
    ///
    /// Derived from graphql-js 16.12.0, src/type/schema.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns what a thunk of a reachable type throws, the reference's
    /// plain `Error` for a type without a name, and the reference's plain
    /// `Error` for two distinct types that share a name.
    pub fn new(
        config: SchemaConfig,
        introspection_schema: &Arc<ObjectType>,
    ) -> Result<Self, DefinitionError> {
        // If this schema was built from a source known to be valid, then it
        // may be marked with assumeValid to avoid an additional type system
        // validation.
        let validation_errors = OnceBox::new();
        if config.assume_valid {
            validation_errors.get_or_init(|| Box::new(Vec::new()));
        }

        // Provide specified directives (e.g. @include and @skip) by default.
        let directives = config
            .directives
            .unwrap_or_else(|| specified_directives().to_vec());

        // To preserve order of user-provided types, we add first to add them
        // to the set of "collected" types, so `collectReferencedTypes` ignore
        // them.
        let mut all_referenced_types = TypeSet::with_capacity(config.types.len());
        for ty in &config.types {
            all_referenced_types.insert(ty.clone());
        }
        for ty in &config.types {
            // When we ready to process this type, we remove it from
            // "collected" types and then add it together with all dependent
            // types in the correct position.
            all_referenced_types.remove(ty);
            collect_referenced_types(&Type::from(ty), &mut all_referenced_types)?;
        }

        for root in [&config.query, &config.mutation, &config.subscription]
            .into_iter()
            .flatten()
        {
            collect_referenced_types(&Type::from(root), &mut all_referenced_types)?;
        }

        for directive in &directives {
            // Directives are not validated until validateSchema() is called.
            for arg in &directive.args {
                collect_referenced_types(&arg.r#type, &mut all_referenced_types)?;
            }
        }
        collect_referenced_types(
            &Type::Object(Arc::clone(introspection_schema)),
            &mut all_referenced_types,
        )?;

        // Storing the resulting map for reference by the schema.
        let mut type_map: ObjMap<NamedType> = ObjMap::default();
        // Keep track of all implementations by interface name.
        let mut implementations_map: ObjMap<Implementations> = ObjMap::default();

        for named_type in all_referenced_types.iter() {
            let type_name = named_type.name();
            if type_name.is_empty() {
                return Err(DefinitionError::plain(
                    "One of the provided types for building the Schema is missing a name.",
                ));
            }
            if type_map.contains_key(type_name) {
                return Err(DefinitionError::plain(format!(
                    "Schema must contain uniquely named types but contains multiple types named \"{type_name}\"."
                )));
            }
            type_map.insert(String::from(type_name), named_type.clone());

            match named_type {
                NamedType::Interface(interface) => {
                    // Store implementations by interface.
                    for iface in interface.get_interfaces()? {
                        if let NamedType::Interface(iface) = iface {
                            implementations_map
                                .entry(iface.name.clone())
                                .or_default()
                                .interfaces
                                .push(Arc::clone(interface));
                        }
                    }
                }
                NamedType::Object(object) => {
                    // Store implementations by objects.
                    for iface in object.get_interfaces()? {
                        if let NamedType::Interface(iface) = iface {
                            let implementations =
                                implementations_map.entry(iface.name.clone()).or_default();
                            implementations.objects.push(Arc::clone(object));
                            implementations.possible_types.push(named_type.clone());
                        }
                    }
                }
                _ => {}
            }
        }

        // The sub type map fills one entry per abstract type on its first
        // `isSubType`, and a cell per abstract type of the map is what lets
        // that happen behind a shared reference.
        let sub_type_map = type_map
            .iter()
            .filter(|(_, ty)| matches!(ty, NamedType::Interface(_) | NamedType::Union(_)))
            .map(|(name, _)| (name.clone(), OnceBox::new()))
            .collect();

        Ok(Self {
            description: config.description,
            extensions: config.extensions,
            ast_node: config.ast_node,
            extension_ast_nodes: config.extension_ast_nodes,
            validation_errors,
            query_type: config.query,
            mutation_type: config.mutation,
            subscription_type: config.subscription,
            directives,
            type_map,
            sub_type_map,
            implementations_map,
        })
    }

    /// Returns the cached result of `validateSchema`, if there is one.
    #[must_use]
    pub fn validation_errors(&self) -> Option<&[GraphQLError]> {
        self.validation_errors.get().map(Vec::as_slice)
    }

    /// Stores the result of `validateSchema`, keeping an earlier one.
    ///
    /// The reference assigns `__validationErrors` once validation has run
    /// and returns the stored list on every later call, so a second store
    /// leaves the first in place, and the returned slice is what is stored.
    pub fn set_validation_errors(&self, errors: Vec<GraphQLError>) -> &[GraphQLError] {
        self.validation_errors
            .get_or_init(|| Box::new(errors))
            .as_slice()
    }

    /// Returns the query root type.
    ///
    /// Derived from graphql-js 16.12.0, src/type/schema.ts, MIT.
    #[must_use]
    pub fn get_query_type(&self) -> Option<&NamedType> {
        self.query_type.as_ref()
    }

    /// Returns the mutation root type.
    ///
    /// Derived from graphql-js 16.12.0, src/type/schema.ts, MIT.
    #[must_use]
    pub fn get_mutation_type(&self) -> Option<&NamedType> {
        self.mutation_type.as_ref()
    }

    /// Returns the subscription root type.
    ///
    /// Derived from graphql-js 16.12.0, src/type/schema.ts, MIT.
    #[must_use]
    pub fn get_subscription_type(&self) -> Option<&NamedType> {
        self.subscription_type.as_ref()
    }

    /// Returns the root type of an operation.
    ///
    /// Derived from graphql-js 16.12.0, src/type/schema.ts, MIT.
    #[must_use]
    pub fn get_root_type(&self, operation: OperationTypeNode) -> Option<&NamedType> {
        match operation {
            OperationTypeNode::Query => self.get_query_type(),
            OperationTypeNode::Mutation => self.get_mutation_type(),
            OperationTypeNode::Subscription => self.get_subscription_type(),
        }
    }

    /// Returns every named type of the schema, keyed by name in collection
    /// order.
    ///
    /// Derived from graphql-js 16.12.0, src/type/schema.ts, MIT.
    #[must_use]
    pub fn get_type_map(&self) -> &ObjMap<NamedType> {
        &self.type_map
    }

    /// Returns the named type called `name`.
    ///
    /// Derived from graphql-js 16.12.0, src/type/schema.ts, MIT.
    #[must_use]
    pub fn get_type(&self, name: &str) -> Option<&NamedType> {
        self.get_type_map().get(name)
    }

    /// Returns the object types an abstract type can resolve to.
    ///
    /// A union answers its members, which are whatever named types its config
    /// holds, and any other named type answers the objects that implement it,
    /// which is an empty list for a type that is no interface.
    ///
    /// Derived from graphql-js 16.12.0, src/type/schema.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns what the member thunk of a union throws.
    pub fn get_possible_types<'a>(
        &'a self,
        abstract_type: &'a NamedType,
    ) -> Result<&'a [NamedType], DefinitionError> {
        match abstract_type {
            NamedType::Union(union) => union.get_types(),
            _ => Ok(self
                .implementations_map
                .get(abstract_type.name())
                .map_or(&[], |implementations| &implementations.possible_types)),
        }
    }

    /// Returns the types that implement an interface.
    ///
    /// Derived from graphql-js 16.12.0, src/type/schema.ts, MIT.
    #[must_use]
    pub fn get_implementations(
        &self,
        interface_type: &InterfaceType,
    ) -> InterfaceImplementations<'_> {
        self.implementations_of(&interface_type.name)
    }

    /// Returns the implementations recorded under `name`, or none.
    fn implementations_of(&self, name: &str) -> InterfaceImplementations<'_> {
        match self.implementations_map.get(name) {
            Some(implementations) => InterfaceImplementations {
                objects: &implementations.objects,
                interfaces: &implementations.interfaces,
            },
            None => InterfaceImplementations {
                objects: &[],
                interfaces: &[],
            },
        }
    }

    /// Returns whether `maybe_sub_type` is a member or an implementation of
    /// `abstract_type`.
    ///
    /// The map of one abstract type is computed on its first call and kept
    /// when the type is in the schema, and computed each time for one that is
    /// not.
    ///
    /// Derived from graphql-js 16.12.0, src/type/schema.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns what the member thunk of a union throws.
    pub fn is_sub_type(
        &self,
        abstract_type: &NamedType,
        maybe_sub_type: &NamedType,
    ) -> Result<bool, DefinitionError> {
        let compute = || -> Result<ObjMap<bool>, DefinitionError> {
            let mut map: ObjMap<bool> = ObjMap::default();

            if let NamedType::Union(union) = abstract_type {
                for ty in union.get_types()? {
                    map.insert(String::from(ty.name()), true);
                }
            } else {
                let implementations = self.implementations_of(abstract_type.name());
                for ty in implementations.objects {
                    map.insert(ty.name.clone(), true);
                }
                for ty in implementations.interfaces {
                    map.insert(ty.name.clone(), true);
                }
            }

            Ok(map)
        };

        let name = maybe_sub_type.name();
        match self.sub_type_map.get(abstract_type.name()) {
            Some(cell) => Ok(cell
                .get_or_try_init(|| compute().map(Box::new))?
                .contains_key(name)),
            None => Ok(compute()?.contains_key(name)),
        }
    }

    /// Returns the directives of the schema.
    ///
    /// Derived from graphql-js 16.12.0, src/type/schema.ts, MIT.
    #[must_use]
    pub fn get_directives(&self) -> &[Arc<Directive>] {
        &self.directives
    }

    /// Returns the directive called `name`.
    ///
    /// Derived from graphql-js 16.12.0, src/type/schema.ts, MIT.
    #[must_use]
    pub fn get_directive(&self, name: &str) -> Option<&Arc<Directive>> {
        self.get_directives()
            .iter()
            .find(|directive| directive.name == name)
    }

    /// Returns the config the schema was built from, normalized.
    ///
    /// `assumeValid` is true when a validation result is stored, which is
    /// what the reference reads from `__validationErrors`.
    ///
    /// Derived from graphql-js 16.12.0, src/type/schema.ts, MIT.
    #[must_use]
    pub fn to_config(&self) -> SchemaConfig {
        SchemaConfig {
            description: self.description.clone(),
            query: self.get_query_type().cloned(),
            mutation: self.get_mutation_type().cloned(),
            subscription: self.get_subscription_type().cloned(),
            types: self.get_type_map().values().cloned().collect(),
            directives: Some(self.get_directives().to_vec()),
            extensions: self.extensions.clone(),
            ast_node: self.ast_node.clone(),
            extension_ast_nodes: self.extension_ast_nodes.clone(),
            assume_valid: self.validation_errors.get().is_some(),
        }
    }
}

impl fmt::Debug for Schema {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Schema")
            .field("query", &self.query_type)
            .field("mutation", &self.mutation_type)
            .field("subscription", &self.subscription_type)
            .field("types", &self.type_map.keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

/// The types the constructor collects, in the order the `Set` of the reference keeps them.
///
/// The `Set` of the reference deletes an entry in constant time, keeps the
/// order of the others and appends an entry that is added again at its end. A
/// slot per insertion keeps that order, a deleted entry leaves its slot
/// empty, and the map holds the slot of each type by identity, so a delete
/// takes constant time and the constructor reads a schema of n types in time
/// linear in n.
struct TypeSet {
    /// The types in the order of their insertion, a deleted one as `None`.
    slots: Vec<Option<NamedType>>,
    /// The slot each type was last inserted at, by identity.
    places: IndexMap<NamedType, usize, KeyHasherBuilder>,
}

impl TypeSet {
    /// Returns a set with room for `capacity` types.
    fn with_capacity(capacity: usize) -> Self {
        Self {
            slots: Vec::with_capacity(capacity),
            places: IndexMap::with_capacity_and_hasher(capacity, KeyHasherBuilder::default()),
        }
    }

    /// Returns whether the set holds `named_type`.
    fn contains(&self, named_type: &NamedType) -> bool {
        self.places
            .get(named_type)
            .and_then(|slot| self.slots.get(*slot))
            .is_some_and(Option::is_some)
    }

    /// Adds `named_type` at the end unless the set holds it.
    fn insert(&mut self, named_type: NamedType) {
        if self.contains(&named_type) {
            return;
        }
        self.places.insert(named_type.clone(), self.slots.len());
        self.slots.push(Some(named_type));
    }

    /// Deletes `named_type`, which leaves the order of the others as it is.
    fn remove(&mut self, named_type: &NamedType) {
        if let Some(slot) = self.places.get(named_type).copied()
            && let Some(entry) = self.slots.get_mut(slot)
        {
            *entry = None;
        }
    }

    /// Iterates over the types in their order.
    fn iter(&self) -> impl Iterator<Item = &NamedType> {
        self.slots.iter().flatten()
    }
}

/// Adds the named type under `ty` and every type it reaches to `type_set`.
///
/// The set is keyed by identity, as the `Set` of the reference is, so two
/// distinct types with one name both enter it and the constructor reports
/// them. The reference returns the set, and the caller here holds it already.
///
/// Derived from graphql-js 16.12.0, src/type/schema.ts, MIT.
///
/// # Errors
///
/// Returns what a thunk of a visited type throws.
fn collect_referenced_types(ty: &Type, type_set: &mut TypeSet) -> Result<(), DefinitionError> {
    let named_type = get_named_type(ty);

    if !type_set.contains(&named_type) {
        type_set.insert(named_type.clone());
        match &named_type {
            NamedType::Union(union) => {
                for member_type in union.get_types()? {
                    collect_referenced_types(&Type::from(member_type), type_set)?;
                }
            }
            NamedType::Object(object) => {
                for interface_type in object.get_interfaces()? {
                    collect_referenced_types(&Type::from(interface_type), type_set)?;
                }

                for field in object.get_fields()?.values() {
                    collect_referenced_types(&field.r#type, type_set)?;
                    for arg in &field.args {
                        collect_referenced_types(&arg.r#type, type_set)?;
                    }
                }
            }
            NamedType::Interface(interface) => {
                for interface_type in interface.get_interfaces()? {
                    collect_referenced_types(&Type::from(interface_type), type_set)?;
                }

                for field in interface.get_fields()?.values() {
                    collect_referenced_types(&field.r#type, type_set)?;
                    for arg in &field.args {
                        collect_referenced_types(&arg.r#type, type_set)?;
                    }
                }
            }
            NamedType::InputObject(input_object) => {
                for field in input_object.get_fields()?.values() {
                    collect_referenced_types(&field.r#type, type_set)?;
                }
            }
            NamedType::Scalar(_) | NamedType::Enum(_) => {}
        }
    }

    Ok(())
}
