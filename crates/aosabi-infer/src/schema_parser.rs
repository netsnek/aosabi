// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-infer: the schema parser, which walks the types of an application
 * from its three roots, collects the working schema, promotes unions and
 * inheritance to interfaces and projects the result to the intermediate
 * representation.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/cli/builder/schema/schema-parser.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 *
 * The `classImplementsMap` stretch of `:400-439` is not translated, because the
 * map is created at `:988` and nothing ever writes to it, so the loop over it
 * never runs.
 */

//! The schema parser of the inference.
//!
//! [`SchemaParser::parse`] runs the passes of `parse` in the order of the
//! reference, and the walk behind it, `rec_loop`, tests its eleven guards in
//! the order of `schema-parser.ts:1001-1254`. That order is the whole contract
//! of this module, because the order in which types are registered is the order
//! of the blocks in the printed schema. The reference writes `parse` and
//! `makeReferenceSchema` as two long functions with closures inside.
//!
//! # JavaScript semantics the port reproduces
//!
//! - A `Map` and a `Set` iterate in insertion order and visit an entry that is
//!   added while the loop runs. Every map of the reference schema is an
//!   [`IndexMap`] keyed by the type handle, and the two loops that walk a map
//!   while the walk below them grows it, the heritage fixpoint of `:1322-1386`,
//!   read it by position.
//! - `for ... of` over an array reads the live length, so an element pushed
//!   during the loop is visited, and the fixpoint reads a derived list by
//!   position for that reason.
//! - `Array.prototype.forEach` over `this.schema.unions` keeps walking the
//!   array it started on when the property is assigned a filtered copy
//!   (`:249-261`), so the port walks a copy taken before the loop.
//! - `[...fields]` copies the references and not the objects. An interface
//!   made from a base type and an interface made from a union therefore
//!   share their field type objects with a type, and the rename of
//!   `:388-394` reaches both. [`Field`] keeps its type behind a shared
//!   handle for that reason.
//! - `Object.entries` reads `properties` and `args` in
//!   `OrdinaryOwnPropertyKeys` order, which is insertion order for identifier
//!   keys. Both are an [`aosabi_ir::Record`], which reads in that order for
//!   every key.
//! - `Array.prototype.sort` is stable, and with no comparator it orders strings
//!   by their UTF-16 code units.
//! - `x || y` takes `y` for the empty string and for `undefined`, so a
//!   description that is empty is absent in the projection.
//! - `JSON.stringify` writes the keys of an object in creation order and leaves
//!   out a key whose value is `undefined`, which decides the text of
//!   [`api_shape`].
//!
//! [map]: https://tc39.es/ecma262/#sec-map.prototype.foreach
//! [map-iterator]: https://tc39.es/ecma262/#sec-createmapiterator
//! [array-iterator]: https://tc39.es/ecma262/#sec-%arrayiteratorprototype%.next
//! [own-keys]: https://tc39.es/ecma262/#sec-ordinaryownpropertykeys
//! [sort]: https://tc39.es/ecma262/#sec-array.prototype.sort
//! [sort-compare]: https://tc39.es/ecma262/#sec-sortcompare
//! [to-boolean]: https://tc39.es/ecma262/#sec-toboolean
//! [stringify]: https://tc39.es/ecma262/#sec-json.stringify

use std::{
    cell::RefCell,
    cmp::Reverse,
    rc::Rc, //
};

use aosabi_ir::{
    Record,
    RootName,
    SchemaIr,
    TypeRef,
    empty_ir, //
};
use aosabi_json::{
    JsObject,
    JsValue,
    map::{
        IndexMap,
        IndexSet, //
    },
    value::KeyHasherBuilder, //
};

use crate::{
    error::InferError,
    scalars::SCALAR_NAMES,
    type_definition_builder::{
        FieldDefinition,
        TypeDefinitionBuilder,
        TypeDefinitionOptions,
        capitalize, //
    },
    type_source::{
        SymbolFlags,
        TypeFlags,
        TypeSource, //
    },
    types_helper::{
        get_promise_type,
        get_public_properties_of_type,
        is_function,
        is_list,
        is_primitive,
        is_primitive_union,
        is_promise,
        is_subscription_repeater,
        safe_type_name, //
    }, //
};

/// A GraphQL union of the working schema, `schema-parser.ts:24-27`.
#[derive(Debug, Clone)]
pub struct Union<T> {
    /// The name of the union.
    pub name: String,
    /// The union type the name stands for.
    pub raw_type: T,
    /// The names of the member types.
    pub types: Vec<String>,
    /// The documentation of the union type.
    pub description: String,
    /// The type resolver, set by the last pass of [`SchemaParser::parse`].
    pub resolve_type: Option<ResolveTypeRule>,
}

/// A GraphQL interface of the working schema, `schema-parser.ts:29-35`.
#[derive(Debug, Clone)]
pub struct Interface {
    /// The name of the interface.
    pub name: String,
    /// The documentation of the interface.
    pub description: String,
    /// The fields of the interface.
    pub fields: Vec<Field>,
    /// The interfaces this one implements, present on an interface made from
    /// inheritance.
    pub implements: Option<Vec<String>>,
    /// The type resolver, set by the last pass of [`SchemaParser::parse`].
    pub resolve_type: Option<ResolveTypeRule>,
}

/// The type resolver of a union or of an interface, the model the reference
/// writes as the text of a function, `schema-parser.ts:541-628`.
///
/// The build writes the text of the model into `resolvers.js`, which the
/// reference writes, and the table of the model into the crate of a server,
/// whose run time answers the same type the text answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolveTypeRule {
    /// The names of the member types, those with more fields first.
    pub valid_names: Vec<String>,
    /// The checks of the members, in the same order, a member with no field
    /// left out.
    pub checks: Vec<ResolveTypeCheck>,
}

/// One check of a type resolver, `schema-parser.ts:578-615`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveTypeCheck {
    /// A node that holds one of `fields` is the member `name`, the required
    /// fields no other member has.
    AnyField {
        /// The fields, in the order of the member.
        fields: Vec<String>,
        /// The member.
        name: String,
    },
    /// A node that holds every one of `fields` with a value other than
    /// `undefined` is the member `name`.
    AllFields {
        /// The fields, in the order of the member.
        fields: Vec<String>,
        /// The member.
        name: String,
    },
}

impl ResolveTypeRule {
    /// Returns the source text of the function, which the reference evaluates
    /// with `new Function`.
    #[must_use]
    pub fn to_js(&self) -> String {
        let valid_names = self
            .valid_names
            .iter()
            .map(|name| format!("'{name}'"))
            .collect::<Vec<String>>()
            .join(", ");
        let checks = self
            .checks
            .iter()
            .map(ResolveTypeCheck::to_js)
            .collect::<Vec<String>>()
            .join(" ");
        [
            "function resolveType(node) { if (!node || typeof node !== 'object') return null; if (node.__typename && [",
            &valid_names,
            "].indexOf(node.__typename) !== -1) return node.__typename; ",
            &checks,
            " return null; }",
        ]
        .concat()
    }
}

impl ResolveTypeCheck {
    /// Returns the source text of the check.
    #[must_use]
    pub fn to_js(&self) -> String {
        match self {
            Self::AnyField { fields, name } => {
                let cond = fields
                    .iter()
                    .map(|field| format!("\"{field}\" in node"))
                    .collect::<Vec<String>>()
                    .join(" || ");
                ["if (", &cond, ") { return '", name, "'; }"].concat()
            }
            Self::AllFields { fields, name } => {
                let cond = fields
                    .iter()
                    .map(|field| format!("\"{field}\" in node && node[\"{field}\"] !== undefined"))
                    .collect::<Vec<String>>()
                    .join(" && ");
                ["if (", &cond, ") { return '", name, "'; }"].concat()
            }
        }
    }
}

/// A GraphQL enum of the working schema, `schema-parser.ts:37-39`.
#[derive(Debug, Clone)]
pub struct Enum<T> {
    /// The name of the enum.
    pub name: String,
    /// The literal union the name stands for.
    pub raw_type: T,
    /// The enum values.
    pub values: Vec<String>,
    /// The documentation of the enum type.
    pub description: String,
}

/// A type reference with its description, `schema-parser.ts:41-52`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeDefinition {
    /// The GraphQL type name.
    pub name: String,
    /// Whether the type is a list.
    pub is_list: bool,
    /// Whether the named type is non null.
    pub is_required: bool,
    /// Whether the list itself is non null.
    pub is_list_required: Option<bool>,
    /// The definition of the list element.
    pub element: Option<Box<FieldDefinition>>,
    /// The documentation that goes with the reference.
    pub description: String,
}

impl TypeDefinition {
    /// Builds `{...fieldDef, description}`, the spread of
    /// `schema-parser.ts:854-857`.
    fn new(def: FieldDefinition, description: String) -> Self {
        Self {
            name: def.name,
            is_list: def.is_list,
            is_required: def.is_required,
            is_list_required: def.is_list_required,
            element: def.element,
            description,
        }
    }

    /// Returns the reference without its description, the `bare` arrow of
    /// `schema-parser.ts:146`.
    #[must_use]
    pub fn field_definition(&self) -> FieldDefinition {
        FieldDefinition {
            name: self.name.clone(),
            is_list: self.is_list,
            is_required: self.is_required,
            is_list_required: self.is_list_required,
            element: self.element.clone(),
        }
    }
}

/// A name with a type, which is an argument of a field and a field of an
/// input object, `schema-parser.ts:62-65` and `:73-76`.
#[derive(Debug, Clone)]
pub struct Argument {
    /// The name of the argument or of the input field.
    pub name: String,
    /// The type of the argument or of the input field.
    pub r#type: TypeDefinition,
}

/// A field of an object type or of an interface, `schema-parser.ts:54-57`
/// and `:71-78`.
///
/// The type sits behind a shared handle, because the reference copies field
/// objects by reference from a type into an interface and later renames the
/// type of a field in place. A clone of a field shares the type of the
/// original, which is what a copied reference does. No borrow of the handle
/// outlives a method of this type, so reading and renaming never meet.
#[derive(Debug, Clone)]
pub struct Field {
    /// The name of the field.
    pub name: String,
    /// The type of the field, shared with every copy of the field.
    r#type: Rc<RefCell<TypeDefinition>>,
    /// The arguments of the field, in parameter order.
    pub args: Vec<Argument>,
}

impl Field {
    /// Creates a field with a type of its own.
    #[must_use]
    pub fn new(name: String, r#type: TypeDefinition, args: Vec<Argument>) -> Self {
        Self {
            name,
            r#type: Rc::new(RefCell::new(r#type)),
            args,
        }
    }

    /// Returns a copy of the type of the field as it reads now.
    #[must_use]
    pub fn type_definition(&self) -> TypeDefinition {
        self.r#type.borrow().clone()
    }

    /// Renames the type of the field and of every copy that shares it, the
    /// assignment of `schema-parser.ts:391`.
    fn set_type_name(&self, name: &str) {
        self.r#type.borrow_mut().name = String::from(name);
    }

    /// Returns a copy of the field whose type is a new object with
    /// `description`, the spread of `schema-parser.ts:529-532`.
    fn with_description(&self, description: String) -> Self {
        let mut r#type = self.type_definition();
        r#type.description = description;

        Self::new(self.name.clone(), r#type, self.args.clone())
    }
}

/// An input object of the working schema, `schema-parser.ts:59-66`.
#[derive(Debug, Clone)]
pub struct Input {
    /// The name of the input object.
    pub name: String,
    /// The documentation of the input type.
    pub description: String,
    /// The fields of the input object.
    pub fields: Vec<Argument>,
}

/// An object type of the working schema, `schema-parser.ts:68-81`.
#[derive(Debug, Clone)]
pub struct Type<T> {
    /// The name of the object type.
    pub name: String,
    /// The documentation of the type.
    pub description: String,
    /// The fields of the type.
    pub fields: Vec<Field>,
    /// The interfaces the type implements, absent until a pass adds one.
    pub implements: Option<Vec<String>>,
    /// The type the object was made from. For a list this is its number
    /// index type. The reference falls back to the string index type, which
    /// the seam does not ask for, because the one reader of this field is the
    /// stretch of `:400-439` that never runs.
    pub raw_type: T,
}

/// The working representation of the inference, `schema-parser.ts:98-105`.
///
/// It carries type handles, which the passes of [`SchemaParser::parse`]
/// compare, and the intermediate representation that leaves the parser does
/// not.
#[derive(Debug, Clone)]
pub struct Schema<T> {
    /// The object types, in registration order.
    pub types: Vec<Type<T>>,
    /// The input objects, in registration order.
    pub inputs: Vec<Input>,
    /// The interfaces, in promotion order.
    pub interfaces: Vec<Interface>,
    /// The unions that stay unions.
    pub unions: Vec<Union<T>>,
    /// The enums.
    pub enums: Vec<Enum<T>>,
    /// The scalar names, [`SCALAR_NAMES`].
    pub scalars: Vec<String>,
}

/// The types of the three root operation objects, `schema-parser.ts:132-136`.
#[derive(Debug, Clone, Copy)]
pub struct Index<T> {
    /// The type of the `Query` object.
    pub query: Option<T>,
    /// The type of the `Mutation` object.
    pub mutation: Option<T>,
    /// The type of the `Subscription` object.
    pub subscription: Option<T>,
}

/// Renders a definition without its description as a JSON value, the `bare`
/// arrow of `schema-parser.ts:146` as `JSON.stringify` sees it.
///
/// The keys stand in the order of the object literals of
/// `type-definition-builder.ts`, and an option that is absent there is left
/// out.
fn bare(def: &FieldDefinition) -> JsValue {
    let mut object = JsObject::new();

    object.insert("name", def.name.as_str());
    object.insert("isList", def.is_list);
    object.insert("isRequired", def.is_required);

    if let Some(is_list_required) = def.is_list_required {
        object.insert("isListRequired", is_list_required);
    }

    if let Some(element) = &def.element {
        object.insert("element", bare(element));
    }

    JsValue::Object(object)
}

/// Returns the API facing shape of `field` as a comparable string,
/// `schema-parser.ts:145-156`.
///
/// The shape is the name, the type and the arguments, and never a
/// description. Two union members share a field when the texts are equal, so
/// nullability and list depth decide and documentation does not.
#[must_use]
pub fn api_shape(field: &Field) -> String {
    let mut shape = JsObject::new();

    shape.insert("name", field.name.as_str());
    shape.insert("type", bare(&field.type_definition().field_definition()));
    shape.insert(
        "args",
        field
            .args
            .iter()
            .map(|arg| {
                let mut object = JsObject::new();

                object.insert("name", arg.name.as_str());
                object.insert("type", bare(&arg.r#type.field_definition()));

                JsValue::Object(object)
            })
            .collect::<Vec<JsValue>>(),
    );

    aosabi_json::to_string(&JsValue::Object(shape)).unwrap_or_default()
}

/// The direction of a walk, the `processing` argument of the reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Processing {
    /// The output direction, `'types'`.
    Types,
    /// The input direction, `'inputs'`.
    Inputs,
}

/// One argument of a property of the reference schema,
/// `schema-parser.ts:112-116`.
struct ReferenceArgument<T> {
    /// The type of the argument.
    r#type: T,
    /// `Some(false)` for an optional parameter and `None` otherwise, so the
    /// nullability then follows from the type.
    is_required: Option<bool>,
    /// The documentation of the parameter.
    documentation: String,
}

/// One property of a type of the reference schema,
/// `schema-parser.ts:107-118`.
struct ReferenceSchemaType<T, Y> {
    /// The type of the property.
    return_type: T,
    /// The symbol of the property.
    symbol: Y,
    /// The arguments of the property, keyed by parameter name.
    args: Record<ReferenceArgument<T>>,
}

/// The properties of each walked type, a `Map` of the reference.
type ReferenceTypes<T, Y> = IndexMap<T, Record<ReferenceSchemaType<T, Y>>, KeyHasherBuilder>;

/// What the walk collects, `schema-parser.ts:120-130` without the map that
/// nothing fills.
struct ReferenceSchema<T, Y> {
    /// The types walked in the output direction.
    types: ReferenceTypes<T, Y>,
    /// The types walked in the input direction.
    inputs: ReferenceTypes<T, Y>,
    /// The derived types of each base type.
    inheritance_map: IndexMap<T, Vec<T>, KeyHasherBuilder>,
}

impl<T, Y> ReferenceSchema<T, Y> {
    /// Returns the map of `processing`, `referenceSchema[processing]`.
    fn of(&self, processing: Processing) -> &ReferenceTypes<T, Y> {
        match processing {
            Processing::Types => &self.types,
            Processing::Inputs => &self.inputs,
        }
    }

    /// Returns the map of `processing` for writing.
    fn of_mut(&mut self, processing: Processing) -> &mut ReferenceTypes<T, Y> {
        match processing {
            Processing::Types => &mut self.types,
            Processing::Inputs => &mut self.inputs,
        }
    }
}

/// Where a walk step came from, the `info` argument of `recLoop`.
struct Info<T> {
    /// The property the type was reached through.
    property_name: Option<String>,
    /// The type that owns that property.
    parent_type: Option<T>,
}

impl<T> Info<T> {
    /// Returns the `{}` a walk starts with.
    fn empty() -> Self {
        Self {
            property_name: None,
            parent_type: None,
        }
    }
}

/// The property of the reference schema that a function adds its arguments
/// to, the `schemaType` of `schema-parser.ts:1116-1118`.
struct ArgumentOwner<'a, T> {
    /// The direction whose map holds the owner.
    processing: Processing,
    /// The type that owns the property.
    parent_type: T,
    /// The name of the property.
    property_name: &'a str,
}

/// What a type resolver is made for, the `'types' in entity` test of
/// `schema-parser.ts:545-548`.
enum ResolveEntity<'a> {
    /// A union with these member names.
    Union(&'a [String]),
    /// An interface with this name.
    Interface(&'a str),
}

/// Returns `path` with `segment` appended, the `[...path, segment]` of the
/// reference.
fn extend(path: &[String], segment: &str) -> Vec<String> {
    let mut extended = path.to_vec();
    extended.push(String::from(segment));

    extended
}

/// Adds `interface_name` to an `implements` list once, creating the list when
/// it is absent, `schema-parser.ts:333-339`.
fn add_implements(implements: &mut Option<Vec<String>>, interface_name: &str) {
    let implements = implements.get_or_insert_with(Vec::new);

    if !implements.iter().any(|name| name == interface_name) {
        implements.push(String::from(interface_name));
    }
}

/// Returns whether a field name is removed from the schema, the `isReserved`
/// arrow of `schema-parser.ts:943-955`.
///
/// A name is reserved when it fails `/^[_A-Za-z][_0-9A-Za-z]*$/` or starts with
/// `__`. The `$` of a regular expression with no `m` flag matches at the end of
/// the input alone.
///
/// [assertion]: https://tc39.es/ecma262/#sec-compileassertion
fn is_reserved(name: &str) -> bool {
    let mut chars = name.chars();
    let is_valid = chars
        .next()
        .is_some_and(|first| first == '_' || first.is_ascii_alphabetic())
        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric());

    if !is_valid {
        return true;
    }

    name.starts_with("__")
}

/// The handles of one source, which name the reference schema of its walk.
type ReferenceSchemaOf<S> = ReferenceSchema<<S as TypeSource>::Type, <S as TypeSource>::Symbol>;

/// The properties of one type of the reference schema of a source.
type PropertiesOf<S> =
    Record<ReferenceSchemaType<<S as TypeSource>::Type, <S as TypeSource>::Symbol>>;

/// Walks the types of an application and builds its schema,
/// `schema-parser.ts:158-1390`.
pub struct SchemaParser<'s, S: TypeSource> {
    /// The working schema.
    schema: Schema<S::Type>,
    /// The type source, which stands in for the checker, the program and the
    /// entry file.
    source: &'s S,
    /// The type mapping, which owns the names.
    type_definition_builder: TypeDefinitionBuilder<'s, S>,
    /// The root operation type names captured by [`SchemaParser::parse`].
    root_type_names: IndexSet<String, KeyHasherBuilder>,
    /// The text of every `consola.warn` of the reference, in emission order.
    warnings: Vec<String>,
}

impl<'s, S: TypeSource> SchemaParser<'s, S> {
    /// Creates a parser over `source` with an empty schema that declares the
    /// twelve scalars, `schema-parser.ts:165-188`.
    #[must_use]
    pub fn new(source: &'s S) -> Self {
        let scalars: Vec<String> = SCALAR_NAMES
            .iter()
            .map(|name| String::from(*name))
            .collect();

        Self {
            type_definition_builder: TypeDefinitionBuilder::new(source, &scalars),
            schema: Schema {
                types: Vec::new(),
                inputs: Vec::new(),
                interfaces: Vec::new(),
                unions: Vec::new(),
                enums: Vec::new(),
                scalars,
            },
            source,
            root_type_names: IndexSet::default(),
            warnings: Vec::new(),
        }
    }

    /// Walks the three roots of `index` and fills the working schema,
    /// `schema-parser.ts:193-477`.
    ///
    /// The passes run in the order of the reference: the walk, the output
    /// types, the input types, the forbidden field names, the unions and the
    /// enums, the promotion of unions to interfaces, the interfaces made from
    /// inheritance, the `Void` fields and the type resolvers.
    ///
    /// # Errors
    ///
    /// Returns [`InferError::Thrown`] where the reference throws, which is
    /// for a function with no parent type, for a union member that is no
    /// object type of the schema, and for every error of
    /// [`TypeDefinitionBuilder::get_type_definition`]. Returns
    /// [`InferError::ReadOfUndefined`] where the reference reads the owner of
    /// the arguments of a function from `undefined`, which the walk of this
    /// file never reaches.
    pub fn parse(&mut self, index: Index<S::Type>) -> Result<(), InferError> {
        if index.query.is_some() {
            self.root_type_names.insert(String::from("Query"));
        }
        if index.mutation.is_some() {
            self.root_type_names.insert(String::from("Mutation"));
        }
        if index.subscription.is_some() {
            self.root_type_names.insert(String::from("Subscription"));
        }

        let reference_schema = self.make_reference_schema(&index)?;

        for (r#type, properties) in &reference_schema.types {
            let type_name = if index.query == Some(*r#type) {
                Some("Query")
            } else if index.mutation == Some(*r#type) {
                Some("Mutation")
            } else if index.subscription == Some(*r#type) {
                Some("Subscription")
            } else {
                None
            };

            self.process_schema_reference(*r#type, properties, type_name, Processing::Types)?;
        }

        for (r#type, properties) in &reference_schema.inputs {
            self.process_schema_reference(*r#type, properties, None, Processing::Inputs)?;
        }

        self.extract_forbidden_field_names_from_schema();
        self.collect_unions_and_enums();
        self.promote_unions_to_interfaces()?;
        self.create_inheritance_interfaces(&reference_schema);
        self.remove_void_fields();
        self.generate_resolve_types();

        Ok(())
    }

    /// Copies the unions and the enums of the type mapping into the schema
    /// with their documentation, and drops every type and input that an enum
    /// represents, `schema-parser.ts:220-245`.
    fn collect_unions_and_enums(&mut self) {
        self.schema.unions = self
            .type_definition_builder
            .get_unions()
            .iter()
            .map(|union| Union {
                name: union.name.clone(),
                raw_type: union.raw_type,
                types: union.types.clone(),
                description: self.get_type_documentation(union.raw_type),
                resolve_type: None,
            })
            .collect();

        self.schema.enums = self
            .type_definition_builder
            .get_enums()
            .iter()
            .map(|enum_type| Enum {
                name: enum_type.name.clone(),
                raw_type: enum_type.raw_type,
                values: enum_type.values.clone(),
                description: self.get_type_documentation(enum_type.raw_type),
            })
            .collect();

        let Schema {
            types,
            inputs,
            enums,
            ..
        } = &mut self.schema;

        types.retain(|r#type| !enums.iter().any(|e| e.name == r#type.name));
        inputs.retain(|input| !enums.iter().any(|e| e.name == input.name));
    }

    /// Replaces every union whose members share a field by an interface of
    /// the same name, `schema-parser.ts:249-276`.
    fn promote_unions_to_interfaces(&mut self) -> Result<(), InferError> {
        let unions = self.schema.unions.clone();

        for union in &unions {
            let Some(interface_union) =
                Self::check_if_interface_is_possible_for_union(union, &self.schema.types)?
            else {
                continue;
            };

            let interface_name = interface_union.name.clone();
            self.schema.interfaces.push(interface_union);
            self.schema.unions.retain(|u| u.name != union.name);

            for r#type in &mut self.schema.types {
                if union.types.contains(&r#type.name) {
                    r#type
                        .implements
                        .get_or_insert_with(Vec::new)
                        .push(interface_name.clone());
                }
            }
        }

        Ok(())
    }

    /// Creates the interface `I` plus the base name for every base type of
    /// the inheritance map that is a type of the schema, and rewrites every
    /// reference to the base type, `schema-parser.ts:279-396`.
    fn create_inheritance_interfaces(&mut self, reference_schema: &ReferenceSchemaOf<S>) {
        for (base_type, derived_types) in &reference_schema.inheritance_map {
            let base_type_name = safe_type_name(&self.source.type_to_string(*base_type));
            let interface_name = format!("I{base_type_name}");

            let Some(base_index) = self
                .schema
                .types
                .iter()
                .position(|t| t.name == base_type_name)
            else {
                continue;
            };

            if !self
                .schema
                .interfaces
                .iter()
                .any(|i| i.name == interface_name)
            {
                let target_interface = self.make_inheritance_interface(
                    reference_schema,
                    *base_type,
                    base_index,
                    &interface_name,
                );
                self.schema.interfaces.push(target_interface);
            }

            if let Some(base_schema_type) = self.schema.types.get_mut(base_index) {
                add_implements(&mut base_schema_type.implements, &interface_name);
            }

            let mut visited = IndexSet::default();
            self.add_interface_to_derived(
                reference_schema,
                derived_types,
                &interface_name,
                &mut visited,
            );

            for r#type in &self.schema.types {
                for field in &r#type.fields {
                    if field.type_definition().name == base_type_name {
                        field.set_type_name(&interface_name);
                    }
                }
            }
        }
    }

    /// Builds the interface of one base type, `schema-parser.ts:293-327`.
    ///
    /// The fields are the field objects of the base type, shared and not
    /// copied, and `implements` names the interface of every base of the base
    /// type that the inheritance map knows by identity or by symbol.
    fn make_inheritance_interface(
        &self,
        reference_schema: &ReferenceSchemaOf<S>,
        base_type: S::Type,
        base_index: usize,
        interface_name: &str,
    ) -> Interface {
        let source = self.source;
        let base_schema_type = self.schema.types.get(base_index);
        let mut implements = Vec::new();

        for base_base_type in source.get_base_types(base_type).unwrap_or_default() {
            let base_base_type_name = safe_type_name(&source.type_to_string(base_base_type));
            let base_interface_name = format!("I{base_base_type_name}");

            if reference_schema
                .inheritance_map
                .contains_key(&base_base_type)
                || self.has_key_with_symbol_of(&reference_schema.inheritance_map, base_base_type)
            {
                implements.push(base_interface_name);
            }
        }

        Interface {
            name: String::from(interface_name),
            description: base_schema_type.map_or_else(String::new, |t| t.description.clone()),
            fields: base_schema_type.map_or_else(Vec::new, |t| t.fields.clone()),
            implements: Some(implements),
            resolve_type: None,
        }
    }

    /// Returns whether a key of `map` carries the symbol of `type`, the
    /// symbol fallback the reference writes beside every identity lookup,
    /// `schema-parser.ts:316-323`.
    fn has_key_with_symbol_of<V>(
        &self,
        map: &IndexMap<S::Type, V, KeyHasherBuilder>,
        r#type: S::Type,
    ) -> bool {
        let Some(symbol) = self.source.get_symbol(r#type) else {
            return false;
        };

        map.keys()
            .any(|key| self.source.get_symbol(*key) == Some(symbol))
    }

    /// Adds `interface_name` to every type derived from a base type,
    /// transitively, the `addInterfaceToDerived` arrow of
    /// `schema-parser.ts:342-383`.
    ///
    /// The checker refuses a class that extends itself, so the reference
    /// needs no guard against a cycle. Heritage attributes can write one, and
    /// `visited` ends the recursion there. For a heritage without a cycle it
    /// changes nothing, because a second visit of a type adds no name twice.
    fn add_interface_to_derived(
        &mut self,
        reference_schema: &ReferenceSchemaOf<S>,
        types: &[S::Type],
        interface_name: &str,
        visited: &mut IndexSet<S::Type, KeyHasherBuilder>,
    ) {
        for derived_type in types {
            if !visited.insert(*derived_type) {
                continue;
            }

            let derived_type_name = safe_type_name(&self.source.type_to_string(*derived_type));

            if let Some(derived_schema_type) = self
                .schema
                .types
                .iter_mut()
                .find(|t| t.name == derived_type_name)
            {
                add_implements(&mut derived_schema_type.implements, interface_name);
            }

            let sub_derived_types =
                reference_schema
                    .inheritance_map
                    .get(derived_type)
                    .or_else(|| {
                        self.get_by_symbol_of(&reference_schema.inheritance_map, *derived_type)
                    });

            if let Some(sub_derived_types) = sub_derived_types {
                self.add_interface_to_derived(
                    reference_schema,
                    sub_derived_types,
                    interface_name,
                    visited,
                );
            }
        }
    }

    /// Returns the value of the first key of `map` that carries the symbol
    /// of `type`, `schema-parser.ts:367-377`.
    fn get_by_symbol_of<'m, V>(
        &self,
        map: &'m IndexMap<S::Type, V, KeyHasherBuilder>,
        r#type: S::Type,
    ) -> Option<&'m V> {
        let symbol = self.source.get_symbol(r#type)?;

        map.iter().find_map(|(key, value)| {
            (self.source.get_symbol(*key) == Some(symbol)).then_some(value)
        })
    }

    /// Removes the `Void` fields of every type that implements something or
    /// is a member of a union, `schema-parser.ts:441-454`.
    ///
    /// An `implements` list that is empty counts, because an array is truthy.
    fn remove_void_fields(&mut self) {
        let Schema { types, unions, .. } = &mut self.schema;

        for r#type in types {
            if r#type.implements.is_some() || unions.iter().any(|u| u.types.contains(&r#type.name))
            {
                r#type
                    .fields
                    .retain(|field| field.type_definition().name != "Void");
            }
        }
    }

    /// Generates the type resolver text of every union and of every
    /// interface, `schema-parser.ts:456-476`.
    fn generate_resolve_types(&mut self) {
        let Schema {
            types,
            unions,
            interfaces,
            ..
        } = &mut self.schema;

        for union in unions {
            union.resolve_type = Some(Self::get_resolve_type_for_union_or_interface(
                &ResolveEntity::Union(&union.types),
                types,
                &mut self.warnings,
            ));
        }

        for intf in interfaces {
            intf.resolve_type = Some(Self::get_resolve_type_for_union_or_interface(
                &ResolveEntity::Interface(&intf.name),
                types,
                &mut self.warnings,
            ));
        }
    }

    /// Returns the interface that replaces `union` when its members share a
    /// field, `schema-parser.ts:479-539`.
    ///
    /// Fields are compared by [`api_shape`]. A shared field stays the field
    /// object of the first member unless that one carries no description and
    /// another member documents the field, in which case the interface gets a
    /// copy with that description.
    fn check_if_interface_is_possible_for_union(
        union: &Union<S::Type>,
        types: &[Type<S::Type>],
    ) -> Result<Option<Interface>, InferError> {
        let mut union_types = Vec::new();

        for t in &union.types {
            let Some(r#type) = types.iter().find(|r#type| r#type.name == *t) else {
                return Err(InferError::Thrown(format!("Type {t} not found")));
            };

            union_types.push(r#type);
        }

        // A union record holds two members or more, so a first member exists.
        let Some(base_type) = union_types.first() else {
            return Ok(None);
        };

        let shares = |f: &Field, field: &Field| api_shape(f) == api_shape(field);
        let common_fields: Vec<&Field> = base_type
            .fields
            .iter()
            .filter(|field| {
                union_types
                    .iter()
                    .all(|r#type| r#type.fields.iter().any(|f| shares(f, field)))
            })
            .collect();

        if common_fields.is_empty() {
            return Ok(None);
        }

        let fields = common_fields
            .into_iter()
            .map(|field| {
                if !field.type_definition().description.is_empty() {
                    return field.clone();
                }

                let documented = union_types
                    .iter()
                    .flat_map(|r#type| r#type.fields.iter())
                    .map(|f| (f, f.type_definition().description))
                    .find(|(f, description)| shares(f, field) && !description.is_empty());

                match documented {
                    Some((_, description)) => field.with_description(description),
                    None => field.clone(),
                }
            })
            .collect();

        Ok(Some(Interface {
            name: union.name.clone(),
            description: union.description.clone(),
            fields,
            implements: None,
            resolve_type: None,
        }))
    }

    /// Builds the type resolver of a union or of an interface,
    /// `schema-parser.ts:541-628`.
    ///
    /// The reference builds the text of a function and evaluates it with
    /// `new Function`. The port builds the model of that text, whose
    /// [`ResolveTypeRule::to_js`] is the text, and evaluates nothing. Members
    /// with more fields come first, a member is recognized by a required
    /// field no other member has, and otherwise by the presence of all of its
    /// fields.
    fn get_resolve_type_for_union_or_interface(
        entity: &ResolveEntity<'_>,
        types: &[Type<S::Type>],
        warnings: &mut Vec<String>,
    ) -> ResolveTypeRule {
        let mut entity_types: Vec<&Type<S::Type>> = match entity {
            ResolveEntity::Union(names) => {
                types.iter().filter(|t| names.contains(&t.name)).collect()
            }
            ResolveEntity::Interface(name) => types
                .iter()
                .filter(|t| {
                    t.implements
                        .as_ref()
                        .is_some_and(|implements| implements.iter().any(|i| i == name))
                })
                .collect(),
        };

        entity_types.sort_by_key(|t| Reverse(t.fields.len()));

        Self::warn_about_same_fields(&entity_types, warnings);

        let checks = entity_types
            .iter()
            .filter_map(|r#type| Self::get_resolve_type_check(r#type, &entity_types))
            .collect();

        ResolveTypeRule {
            valid_names: entity_types.iter().map(|t| t.name.clone()).collect(),
            checks,
        }
    }

    /// Warns about two members with the same field names, which a type
    /// resolver cannot tell apart, `schema-parser.ts:555-575`.
    fn warn_about_same_fields(entity_types: &[&Type<S::Type>], warnings: &mut Vec<String>) {
        let mut field_signatures: IndexMap<String, &str, KeyHasherBuilder> = IndexMap::default();

        for r#type in entity_types {
            let mut names: Vec<&str> = r#type.fields.iter().map(|f| f.name.as_str()).collect();
            names.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
            let field_names = names.join(", ");

            if let Some(existing_type) = field_signatures.get(&field_names) {
                warnings.push(format!(
                    "Warning: Union types \"{}\" and \"{}\" have the same fields: [{}]. \nConsider differentiating these types by adding unique fields or using different type names.\nThis may cause ambiguity in type resolution.",
                    r#type.name, existing_type, field_names
                ));
            } else {
                field_signatures.insert(field_names, r#type.name.as_str());
            }
        }
    }

    /// Returns the check of one member inside a type resolver, or `None` for
    /// a member with no field, `schema-parser.ts:578-615`.
    fn get_resolve_type_check(
        r#type: &Type<S::Type>,
        entity_types: &[&Type<S::Type>],
    ) -> Option<ResolveTypeCheck> {
        let other_fields: IndexSet<&str, KeyHasherBuilder> = entity_types
            .iter()
            .filter(|t| t.name != r#type.name)
            .flat_map(|t| t.fields.iter().map(|f| f.name.as_str()))
            .collect();

        let unique_fields: Vec<String> = r#type
            .fields
            .iter()
            .filter(|f| f.type_definition().is_required && !other_fields.contains(f.name.as_str()))
            .map(|f| f.name.clone())
            .collect();

        if !unique_fields.is_empty() {
            return Some(ResolveTypeCheck::AnyField {
                fields: unique_fields,
                name: r#type.name.clone(),
            });
        }

        if r#type.fields.is_empty() {
            return None;
        }

        Some(ResolveTypeCheck::AllFields {
            fields: r#type.fields.iter().map(|f| f.name.clone()).collect(),
            name: r#type.name.clone(),
        })
    }

    /// Rebuilds the type reference of the intermediate representation from a
    /// definition, the `typeRefOf` arrow of `schema-parser.ts:653-675`.
    ///
    /// The element chain tells the nesting depth and the nullability of each
    /// list level. The leaf name and its nullability come from the outer
    /// definition, whose name carries a rename to an interface.
    fn type_ref_of(scalars: &[String], td: &FieldDefinition) -> TypeRef {
        let mut list_required = Vec::new();
        let mut cur = td;

        while cur.is_list {
            list_required.push(cur.is_list_required == Some(true));

            let Some(element) = &cur.element else {
                break;
            };
            cur = element;
        }

        let mut r#ref = if scalars.contains(&td.name) {
            TypeRef::Scalar {
                name: td.name.clone(),
                nullable: !td.is_required,
            }
        } else {
            TypeRef::Ref {
                name: td.name.clone(),
                nullable: !td.is_required,
            }
        };

        for required in list_required.into_iter().rev() {
            r#ref = TypeRef::List {
                of: Box::new(r#ref),
                nullable: !required,
            };
        }

        r#ref
    }

    /// Projects an argument or an input field, the object literal of
    /// `schema-parser.ts:686-691` and `:702-707`.
    ///
    /// The reference maps an input field through `fieldOf` at `:735` and not
    /// through that literal. An input field carries a name and a type and no
    /// `args`, so `fieldOf` writes no `args` key for it and the two literals
    /// coincide.
    fn argument_of(scalars: &[String], a: &Argument) -> aosabi_ir::Field {
        aosabi_ir::Field {
            name: a.name.clone(),
            r#type: Self::type_ref_of(scalars, &a.r#type.field_definition()),
            exposed: true,
            description: Some(a.r#type.description.clone()).filter(|d| !d.is_empty()),
            args: None,
            column: None,
            relation: None,
        }
    }

    /// Projects a field of a type or of an interface, the `fieldOf` arrow of
    /// `schema-parser.ts:677-694`.
    ///
    /// `args` is present for a field that has an argument and absent
    /// otherwise.
    fn field_of(scalars: &[String], f: &Field) -> aosabi_ir::Field {
        let r#type = f.type_definition();
        let args: Vec<aosabi_ir::Field> = f
            .args
            .iter()
            .map(|a| Self::argument_of(scalars, a))
            .collect();

        aosabi_ir::Field {
            name: f.name.clone(),
            r#type: Self::type_ref_of(scalars, &r#type.field_definition()),
            exposed: true,
            description: Some(r#type.description).filter(|d| !d.is_empty()),
            args: Some(args).filter(|args| !args.is_empty()),
            column: None,
            relation: None,
        }
    }

    /// Returns the root a type name stands for when the parse captured it,
    /// the `rootTypeNames.has(type.name)` test of `schema-parser.ts:697`.
    fn root_of(&self, name: &str) -> Option<RootName> {
        if !self.root_type_names.contains(name) {
            return None;
        }

        RootName::ALL.into_iter().find(|root| root.as_str() == name)
    }

    /// Projects the working schema into the intermediate representation,
    /// `schema-parser.ts:641-756`.
    ///
    /// A type whose name is a captured root name contributes one operation
    /// per field, and every other type is an object. Every field and
    /// argument is exposed, and the entity record stays empty.
    #[must_use]
    pub fn to_ir(&self) -> SchemaIr {
        let mut ir = empty_ir();
        let scalars = &self.schema.scalars;
        ir.scalars.clone_from(scalars);

        for r#type in &self.schema.types {
            if let Some(root) = self.root_of(&r#type.name) {
                for f in &r#type.fields {
                    let returns = f.type_definition();

                    ir.operations.push(aosabi_ir::Operation {
                        root,
                        name: f.name.clone(),
                        args: f
                            .args
                            .iter()
                            .map(|a| Self::argument_of(scalars, a))
                            .collect(),
                        returns: Self::type_ref_of(scalars, &returns.field_definition()),
                        description: Some(returns.description).filter(|d| !d.is_empty()),
                    });
                }
            } else {
                ir.objects.insert(
                    r#type.name.clone(),
                    aosabi_ir::ObjectType {
                        name: r#type.name.clone(),
                        description: Some(r#type.description.clone()).filter(|d| !d.is_empty()),
                        implements: r#type.implements.clone(),
                        fields: r#type
                            .fields
                            .iter()
                            .map(|f| Self::field_of(scalars, f))
                            .collect(),
                    },
                );
            }
        }

        self.project_interfaces_and_inputs(&mut ir);
        self.project_unions_and_enums(&mut ir);

        ir
    }

    /// Copies the interfaces and the inputs into `ir`,
    /// `schema-parser.ts:722-737`.
    fn project_interfaces_and_inputs(&self, ir: &mut SchemaIr) {
        let scalars = &self.schema.scalars;

        for intf in &self.schema.interfaces {
            ir.interfaces.insert(
                intf.name.clone(),
                aosabi_ir::InterfaceType {
                    name: intf.name.clone(),
                    description: Some(intf.description.clone()).filter(|d| !d.is_empty()),
                    implements: intf.implements.clone(),
                    fields: intf
                        .fields
                        .iter()
                        .map(|f| Self::field_of(scalars, f))
                        .collect(),
                },
            );
        }

        for input in &self.schema.inputs {
            ir.inputs.insert(
                input.name.clone(),
                aosabi_ir::InputType {
                    name: input.name.clone(),
                    description: Some(input.description.clone()).filter(|d| !d.is_empty()),
                    fields: input
                        .fields
                        .iter()
                        .map(|f| Self::argument_of(scalars, f))
                        .collect(),
                },
            );
        }
    }

    /// Copies the unions and the enums into `ir`, `schema-parser.ts:739-753`.
    fn project_unions_and_enums(&self, ir: &mut SchemaIr) {
        for union in &self.schema.unions {
            ir.unions.insert(
                union.name.clone(),
                aosabi_ir::UnionType {
                    name: union.name.clone(),
                    description: Some(union.description.clone()).filter(|d| !d.is_empty()),
                    members: union.types.clone(),
                },
            );
        }

        for en in &self.schema.enums {
            ir.enums.insert(
                en.name.clone(),
                aosabi_ir::EnumType {
                    name: en.name.clone(),
                    values: en.values.clone(),
                    description: Some(en.description.clone()).filter(|d| !d.is_empty()),
                },
            );
        }
    }

    /// Renders the schema as SDL, which is `toString` of the reference,
    /// `schema-parser.ts:764-766`.
    ///
    /// The name differs from the reference, because an inherent `to_string`
    /// shadows `Display` and `Display` cannot carry the refusal of the
    /// printer. The text is `to_sdl(to_ir())`, the single rendering path.
    ///
    /// # Errors
    ///
    /// Returns [`InferError::Sdl`] when a type name of an `implements` list
    /// leaves the alphabet of the collation.
    pub fn to_sdl_string(&self) -> Result<String, InferError> {
        aosabi_ir::to_sdl(&self.to_ir()).map_err(InferError::from)
    }

    /// Returns the working schema, `schema-parser.ts:768-770`.
    #[must_use]
    pub fn get_schema(&self) -> &Schema<S::Type> {
        &self.schema
    }

    /// Returns every output type the parse named with its name, the entries
    /// of the `typesNameMap` of its type definition builder.
    #[must_use]
    pub fn type_names(&self) -> Vec<(S::Type, String)> {
        self.type_definition_builder.type_names()
    }

    /// Returns the source text of the type resolver of every union and of
    /// every interface that has a field, keyed by type name,
    /// `schema-parser.ts:772-802`.
    ///
    /// An interface with no field is skipped, because the printer drops it
    /// and a resolver for it would dangle. The reference keys a plain object
    /// by GraphQL names, which are no array indices, so insertion order is
    /// its key order.
    #[must_use]
    pub fn get_resolvers(&self) -> IndexMap<String, String, KeyHasherBuilder> {
        self.get_resolve_type_rules()
            .into_iter()
            .map(|(name, rule)| (name, rule.to_js()))
            .collect()
    }

    /// Returns the type resolver of every union and of every interface that
    /// has a field, keyed by type name, the model of
    /// [`SchemaParser::get_resolvers`].
    #[must_use]
    pub fn get_resolve_type_rules(&self) -> IndexMap<String, ResolveTypeRule, KeyHasherBuilder> {
        let empty = || ResolveTypeRule {
            valid_names: Vec::new(),
            checks: Vec::new(),
        };
        let mut resolvers = IndexMap::default();

        for union in &self.schema.unions {
            resolvers.insert(
                union.name.clone(),
                union.resolve_type.clone().unwrap_or_else(empty),
            );
        }

        for intf in &self.schema.interfaces {
            if intf.fields.is_empty() {
                continue;
            }

            resolvers.insert(
                intf.name.clone(),
                intf.resolve_type.clone().unwrap_or_else(empty),
            );
        }

        resolvers
    }

    /// Returns the text of every warning the reference prints through
    /// `consola.warn`, in emission order.
    #[must_use]
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// Finds the schema entry named `name` in the direction `processing`, or
    /// creates it, `schema-parser.ts:818-841`.
    ///
    /// The result is the position of the entry, and `None` where the
    /// reference returns, which is for a name of the scalar table and for an
    /// entry that already has fields because a type of the same shape filled
    /// it.
    fn find_or_create_root(
        &mut self,
        r#type: S::Type,
        name: &str,
        processing: Processing,
    ) -> Option<usize> {
        let found = match processing {
            Processing::Types => self.schema.types.iter().position(|t| t.name == name),
            Processing::Inputs => self.schema.inputs.iter().position(|t| t.name == name),
        };

        if let Some(index) = found {
            let is_unfilled = match processing {
                Processing::Types => self.schema.types.get(index)?.fields.is_empty(),
                Processing::Inputs => self.schema.inputs.get(index)?.fields.is_empty(),
            };

            return is_unfilled.then_some(index);
        }

        if self.schema.scalars.iter().any(|scalar| scalar == name) {
            return None;
        }

        let description = self.get_type_documentation(r#type);

        match processing {
            Processing::Types => {
                let raw_type = if is_list(self.source, r#type) {
                    self.source.get_index_type_of_type(r#type).unwrap_or(r#type)
                } else {
                    r#type
                };

                self.schema.types.push(Type {
                    name: String::from(name),
                    description,
                    fields: Vec::new(),
                    implements: None,
                    raw_type,
                });

                Some(self.schema.types.len() - 1)
            }
            Processing::Inputs => {
                self.schema.inputs.push(Input {
                    name: String::from(name),
                    description,
                    fields: Vec::new(),
                });

                Some(self.schema.inputs.len() - 1)
            }
        }
    }

    /// Turns one type of the reference schema into a type or an input of the
    /// schema, `schema-parser.ts:804-907`.
    ///
    /// `type_name` is the root name the caller supplies. It wins over the
    /// mapped name, which is why a root with no field registers under its root
    /// name although the mapping calls it `JSONObject`.
    fn process_schema_reference(
        &mut self,
        r#type: S::Type,
        properties: &PropertiesOf<S>,
        type_name: Option<&str>,
        processing: Processing,
    ) -> Result<(), InferError> {
        let is_input_type = processing == Processing::Inputs;

        let def = self.type_definition_builder.get_type_definition(
            r#type,
            &TypeDefinitionOptions {
                is_input_type,
                ..TypeDefinitionOptions::default()
            },
        )?;

        let name = type_name.map_or(def.name, String::from);

        let Some(root) = self.find_or_create_root(r#type, &name, processing) else {
            return Ok(());
        };

        for (property_name, property) in properties.entries() {
            let field_type = property.return_type;

            let field_def = self.type_definition_builder.get_type_definition(
                field_type,
                &TypeDefinitionOptions {
                    is_input_type,
                    property_name: Some(String::from(property_name)),
                    ..TypeDefinitionOptions::default()
                },
            )?;

            match processing {
                Processing::Types => {
                    let description = self.get_symbol_documentation(property.symbol);
                    let args = self.process_arguments(property_name, property)?;
                    let field = Field::new(
                        String::from(property_name),
                        TypeDefinition::new(field_def, description),
                        args,
                    );

                    if let Some(root) = self.schema.types.get_mut(root) {
                        root.fields.push(field);
                    }
                }
                Processing::Inputs => {
                    // An input field takes the documentation of its type and
                    // not of its property, quirk `Q15_INPUT_DESCRIPTION_FROM_TYPE`.
                    let description = self.get_type_documentation(field_type);
                    let field = Argument {
                        name: String::from(property_name),
                        r#type: TypeDefinition::new(field_def, description),
                    };

                    if let Some(root) = self.schema.inputs.get_mut(root)
                        && !root.fields.iter().any(|f| f.name == field.name)
                    {
                        root.fields.push(field);
                    }
                }
            }
        }

        Ok(())
    }

    /// Maps the arguments of one property, `schema-parser.ts:861-890`.
    ///
    /// An anonymous argument type is named after the field and the
    /// capitalized argument, and an argument whose printed type is a scalar
    /// name takes that name. `argName.charAt(0).toUpperCase()` takes one
    /// UTF-16 code unit, which is [`capitalize`].
    fn process_arguments(
        &mut self,
        property_name: &str,
        property: &ReferenceSchemaType<S::Type, S::Symbol>,
    ) -> Result<Vec<Argument>, InferError> {
        let mut args = Vec::new();

        for (arg_name, arg) in property.args.entries() {
            let mut field_def = self.type_definition_builder.get_type_definition(
                arg.r#type,
                &TypeDefinitionOptions {
                    is_input_type: true,
                    property_name: Some(format!("{property_name}{}", capitalize(arg_name))),
                    is_required: arg.is_required,
                    dry_run: false,
                },
            )?;

            let printed = self.source.type_to_string(arg.r#type);

            if self.schema.scalars.contains(&printed) {
                field_def.name = printed;
            }

            args.push(Argument {
                name: String::from(arg_name),
                r#type: TypeDefinition::new(field_def, arg.documentation.clone()),
            });
        }

        Ok(args)
    }

    /// Returns the documentation of `symbol`, which is its comment and, when
    /// it has tags, a newline and every tag as `@name text`,
    /// `schema-parser.ts:909-926`.
    ///
    /// A deprecation tag folds into the text like every other tag, quirk
    /// `Q07_NO_DEPRECATED`.
    fn get_symbol_documentation(&self, symbol: S::Symbol) -> String {
        let mut header = self.source.get_documentation_comment(symbol);

        let tags = self
            .source
            .get_js_doc_tags(symbol)
            .iter()
            .map(|t| format!("@{} {}", t.name, t.text))
            .collect::<Vec<String>>()
            .join("\n");

        if !tags.is_empty() {
            header.push('\n');
            header.push_str(&tags);
        }

        header
    }

    /// Returns the documentation of the symbol of `type`, or the empty string
    /// for a type with no symbol, `schema-parser.ts:928-936`.
    fn get_type_documentation(&self, r#type: S::Type) -> String {
        self.source
            .get_symbol(r#type)
            .map_or_else(String::new, |symbol| self.get_symbol_documentation(symbol))
    }

    /// Removes every field whose name is no GraphQL name or starts with `__`
    /// from the types and the inputs, `schema-parser.ts:941-982`.
    ///
    /// No warning is printed, because the reference has them commented out.
    /// This is what removes the `__typename` discriminator.
    fn extract_forbidden_field_names_from_schema(&mut self) {
        for r#type in &mut self.schema.types {
            r#type.fields.retain(|field| !is_reserved(&field.name));
        }

        for input in &mut self.schema.inputs {
            input.fields.retain(|field| !is_reserved(&field.name));
        }
    }

    /// Walks the types from the three roots, sweeps the heritage of the
    /// program and returns what the walk collected,
    /// `schema-parser.ts:984-1389`.
    ///
    /// The roots are seeded in the order Query, Mutation, Subscription, so the
    /// registration order is a depth first pre order walk from Query.
    fn make_reference_schema(
        &mut self,
        index: &Index<S::Type>,
    ) -> Result<ReferenceSchemaOf<S>, InferError> {
        let mut reference_schema = ReferenceSchema {
            types: IndexMap::default(),
            inputs: IndexMap::default(),
            inheritance_map: IndexMap::default(),
        };

        for root in [index.query, index.mutation, index.subscription]
            .into_iter()
            .flatten()
        {
            self.rec_loop_from(&mut reference_schema, root)?;
        }

        self.sweep_heritage(&mut reference_schema);
        self.walk_heritage_to_fixpoint(&mut reference_schema)?;

        Ok(reference_schema)
    }

    /// Starts a walk at `type` with the defaults of `recLoop`, which are no
    /// origin, the output direction and an empty path.
    fn rec_loop_from(
        &mut self,
        reference_schema: &mut ReferenceSchemaOf<S>,
        r#type: S::Type,
    ) -> Result<(), InferError> {
        self.rec_loop(
            reference_schema,
            r#type,
            &Info::empty(),
            Processing::Types,
            &[],
        )
    }

    /// Records every heritage clause of the program in the inheritance map,
    /// `schema-parser.ts:1269-1319`.
    ///
    /// An `extends` and an `implements` clause are treated alike. A base type
    /// is filed under the key that carries its symbol when one exists, and a
    /// derived type is appended once per symbol.
    fn sweep_heritage(&self, reference_schema: &mut ReferenceSchemaOf<S>) {
        let source = self.source;

        for declaration in source.get_heritage_declarations() {
            let derived_type = declaration.derived;

            for base_type in declaration.bases {
                let found_base_type = source
                    .get_symbol(base_type)
                    .and_then(|symbol| {
                        reference_schema
                            .inheritance_map
                            .keys()
                            .copied()
                            .find(|key| source.get_symbol(*key) == Some(symbol))
                    })
                    .unwrap_or(base_type);

                let derived_types = reference_schema
                    .inheritance_map
                    .entry(found_base_type)
                    .or_default();

                if let Some(derived_type_symbol) = source.get_symbol(derived_type)
                    && !derived_types
                        .iter()
                        .any(|t| source.get_symbol(*t) == Some(derived_type_symbol))
                {
                    derived_types.push(derived_type);
                }
            }
        }
    }

    /// Returns whether `type` is walked in the output direction, by identity
    /// or by symbol, the test the reference writes three times at
    /// `schema-parser.ts:1326-1338`, `:1342-1357` and `:1361-1373`.
    fn is_type_in_schema(&self, reference_schema: &ReferenceSchemaOf<S>, r#type: S::Type) -> bool {
        reference_schema.types.contains_key(&r#type)
            || self.has_key_with_symbol_of(&reference_schema.types, r#type)
    }

    /// Walks derived types of walked base types, and base types of walked
    /// derived types, until nothing changes, `schema-parser.ts:1321-1386`.
    ///
    /// This is why a subclass that no resolver returns still lands in the
    /// schema. A walk may add a key to the inheritance map and a type to a
    /// derived list, and the loops of the reference visit both, so the map
    /// and the list are read by position.
    fn walk_heritage_to_fixpoint(
        &mut self,
        reference_schema: &mut ReferenceSchemaOf<S>,
    ) -> Result<(), InferError> {
        let mut changed = true;

        while changed {
            changed = false;
            let mut i = 0;

            while let Some((base_type, derived_types)) = reference_schema
                .inheritance_map
                .get_index(i)
                .map(|(base_type, derived_types)| (*base_type, derived_types.clone()))
            {
                i += 1;

                let is_base_type_in_schema = self.is_type_in_schema(reference_schema, base_type);
                let is_any_derived_type_in_schema = derived_types
                    .iter()
                    .any(|derived_type| self.is_type_in_schema(reference_schema, *derived_type));

                if is_base_type_in_schema {
                    changed |= self.walk_derived_types(reference_schema, base_type)?;
                } else if is_any_derived_type_in_schema {
                    self.rec_loop_from(reference_schema, base_type)?;
                    changed = true;
                }
            }
        }

        Ok(())
    }

    /// Walks every derived type of `base_type` that is not walked yet and
    /// returns whether one was, `schema-parser.ts:1359-1379`.
    fn walk_derived_types(
        &mut self,
        reference_schema: &mut ReferenceSchemaOf<S>,
        base_type: S::Type,
    ) -> Result<bool, InferError> {
        let mut changed = false;
        let mut j = 0;

        while let Some(derived_type) = reference_schema
            .inheritance_map
            .get(&base_type)
            .and_then(|derived_types| derived_types.get(j))
            .copied()
        {
            j += 1;

            if !self.is_type_in_schema(reference_schema, derived_type) {
                self.rec_loop_from(reference_schema, derived_type)?;
                changed = true;
            }
        }

        Ok(changed)
    }

    /// Returns whether the symbol of `type` carries the class flag, the test
    /// of `schema-parser.ts:1046` and `:1050`.
    fn is_class(&self, r#type: S::Type) -> bool {
        self.source.get_symbol(r#type).is_some_and(|symbol| {
            self.source
                .get_symbol_flags(symbol)
                .contains(SymbolFlags::CLASS)
        })
    }

    /// Walks one type, the `recLoop` arrow of `schema-parser.ts:992-1255`.
    ///
    /// The guards are tested in the order of the reference, because an earlier
    /// guard shadows a later one. The first guard is the only cycle breaker,
    /// and the class guard does not return, so a class is recorded in the
    /// inheritance map and then walked as an object.
    fn rec_loop(
        &mut self,
        reference_schema: &mut ReferenceSchemaOf<S>,
        r#type: S::Type,
        info: &Info<S::Type>,
        processing: Processing,
        path: &[String],
    ) -> Result<(), InferError> {
        let source = self.source;

        if reference_schema.of(processing).contains_key(&r#type) {
            return Ok(());
        }

        if is_subscription_repeater(source, r#type) {
            let Some(repeater_item_type) = source.get_type_arguments(r#type).into_iter().next()
            else {
                return Ok(());
            };
            let path = extend(path, "REPEATER_ITEM");

            return self.rec_loop(
                reference_schema,
                repeater_item_type,
                info,
                processing,
                &path,
            );
        }

        // An empty array literal prints as `[]` and is no real type.
        let printed = source.type_to_string(r#type);

        if printed == "[]" || self.schema.scalars.contains(&printed) || is_primitive(source, r#type)
        {
            return Ok(());
        }

        if is_promise(source, r#type) {
            let promise_type = get_promise_type(source, r#type);

            return match (processing, promise_type) {
                (Processing::Types, Some(promise_type)) => {
                    let path = extend(path, "PROMISE");
                    self.rec_loop(reference_schema, promise_type, info, processing, &path)
                }
                _ => Ok(()),
            };
        }

        self.rec_loop_base_classes(reference_schema, r#type)?;

        if source.is_union(r#type) {
            self.rec_loop_union(reference_schema, r#type, info, processing, path)
        } else if is_function(source, r#type) {
            self.rec_loop_function(reference_schema, r#type, info, processing, path)
        } else if is_list(source, r#type) {
            self.rec_loop_list(reference_schema, r#type, info, processing, path)
        } else if !is_primitive(source, r#type) {
            self.rec_loop_object(reference_schema, r#type, processing, path)
        } else {
            Ok(())
        }
    }

    /// Records the base classes of a class in the inheritance map and walks
    /// each of them, `schema-parser.ts:1046-1060`.
    fn rec_loop_base_classes(
        &mut self,
        reference_schema: &mut ReferenceSchemaOf<S>,
        r#type: S::Type,
    ) -> Result<(), InferError> {
        if !self.is_class(r#type) {
            return Ok(());
        }

        for base_type in self.source.get_base_types(r#type).unwrap_or_default() {
            if !self.is_class(base_type) {
                continue;
            }

            reference_schema
                .inheritance_map
                .entry(base_type)
                .or_default()
                .push(r#type);

            self.rec_loop_from(reference_schema, base_type)?;
        }

        Ok(())
    }

    /// Walks a union, `schema-parser.ts:1062-1101`.
    ///
    /// A literal union is registered and walked again under an enum tag,
    /// which returns at the first guard. In the output direction every
    /// member that is no `null`, no `undefined` and no primitive is walked.
    /// In the input direction the non nullable type is walked, and of a union
    /// the first member alone, quirk `Q09_INPUT_UNION_FIRST_MEMBER`.
    fn rec_loop_union(
        &mut self,
        reference_schema: &mut ReferenceSchemaOf<S>,
        r#type: S::Type,
        info: &Info<S::Type>,
        processing: Processing,
        path: &[String],
    ) -> Result<(), InferError> {
        let source = self.source;

        if is_primitive_union(source, r#type) {
            reference_schema
                .of_mut(processing)
                .entry(r#type)
                .or_default();

            let path = extend(path, "ENUM");
            return self.rec_loop(reference_schema, r#type, info, processing, &path);
        }

        if processing == Processing::Types {
            for t in source.get_types(r#type) {
                let flags = source.get_flags(t);

                if flags.contains(TypeFlags::NULL)
                    || flags.contains(TypeFlags::UNDEFINED)
                    || is_primitive(source, t)
                {
                    continue;
                }

                let segment = source
                    .get_symbol(t)
                    .map(|symbol| source.get_name(symbol))
                    .filter(|name| !name.is_empty())
                    .unwrap_or_else(|| format!("N/A {}", source.type_to_string(t)));
                let path = extend(path, &segment);

                self.rec_loop(reference_schema, t, info, processing, &path)?;
            }

            return Ok(());
        }

        let mut first_type = source.get_non_nullable_type(r#type);

        if source.is_union(first_type) && !is_primitive(source, first_type) {
            self.warnings.push(format!(
                "Warning: Union types in input fields are not supported yet. Defaulting to the first type ({}) at path: {}",
                source.type_to_string(first_type),
                path.join(" > ")
            ));

            let Some(first_member) = source.get_types(first_type).into_iter().next() else {
                return Ok(());
            };
            first_type = first_member;
        }

        let path = extend(path, "NON_NULLABLE");
        self.rec_loop(reference_schema, first_type, info, processing, &path)
    }

    /// Walks a function, which is a resolver with arguments,
    /// `schema-parser.ts:1102-1203`.
    ///
    /// Only the first call signature counts. Each parameter is walked in the
    /// input direction before the return type is walked, which is why the input
    /// types of the arguments are registered before the return type. A property
    /// name that is absent reads as the key `undefined`, which is what
    /// `obj[undefined]` names.
    ///
    /// # Errors
    ///
    /// Returns [`InferError::Thrown`] for a function with no parent type,
    /// `:1108-1110`. Returns [`InferError::ReadOfUndefined`] when the map of
    /// `processing` holds no entry of the parent type. The object guard sets
    /// that entry before it walks a property, `:1220-1222`, so the walk of this
    /// file never gets there. Where it does, the reference reads the property
    /// name from `undefined` while it binds `schemaType`, `:1116-1118`, which
    /// fails with a runtime `TypeError` of the engine, and the error names that
    /// read in place of the engine's text.
    ///
    /// [key]: https://tc39.es/ecma262/#sec-topropertykey
    /// [to-object]: https://tc39.es/ecma262/#sec-toobject
    fn rec_loop_function(
        &mut self,
        reference_schema: &mut ReferenceSchemaOf<S>,
        r#type: S::Type,
        info: &Info<S::Type>,
        processing: Processing,
        path: &[String],
    ) -> Result<(), InferError> {
        let source = self.source;

        if processing == Processing::Inputs {
            return Ok(());
        }

        let Some(parent_type) = info.parent_type else {
            return Err(InferError::Thrown(String::from(
                "Cannot have a function without a parent type",
            )));
        };

        let signature = source.get_call_signatures(r#type).into_iter().next();
        let args = signature.map_or_else(Vec::new, |signature| source.get_parameters(signature));
        let return_type = signature.map(|signature| source.get_return_type(signature));

        let owner = ArgumentOwner {
            processing,
            parent_type,
            property_name: info.property_name.as_deref().unwrap_or("undefined"),
        };

        // The reference binds `schemaType` before it visits a parameter, so a
        // parent type with no entry fails a function with no parameter too.
        if !reference_schema.of(processing).contains_key(&parent_type) {
            return Err(InferError::ReadOfUndefined {
                construct: "referenceSchema[processing].get(info.parentType)",
                member: String::from(owner.property_name),
            });
        }

        for arg in args {
            self.rec_loop_parameter(reference_schema, r#type, &owner, arg, path)?;
        }

        if let Some(return_type) = return_type {
            let path = extend(path, "RETURN_TYPE");
            self.rec_loop(reference_schema, return_type, info, processing, &path)?;
        }

        Ok(())
    }

    /// Records one parameter of the function `type` as an argument of
    /// `owner` and walks its type in the input direction,
    /// `schema-parser.ts:1120-1199`.
    ///
    /// A labeled tuple rest parameter gives one argument per element. A rest
    /// parameter with no labels is skipped with a warning, quirk
    /// `Q16_UNLABELED_REST_DROPPED`.
    fn rec_loop_parameter(
        &mut self,
        reference_schema: &mut ReferenceSchemaOf<S>,
        r#type: S::Type,
        owner: &ArgumentOwner<'_, S::Type>,
        arg: S::Symbol,
        path: &[String],
    ) -> Result<(), InferError> {
        let source = self.source;
        let arg_type = source.get_type_of_symbol_at_location(arg);
        let origin = Info {
            property_name: None,
            parent_type: Some(r#type),
        };

        if let Some(elements) = source.get_labeled_tuple_elements(arg_type) {
            for element in elements {
                let argument = ReferenceArgument {
                    r#type: element.r#type,
                    is_required: element.has_initializer.then_some(false),
                    documentation: self.get_symbol_documentation(element.symbol.unwrap_or(arg)),
                };
                Self::set_argument(reference_schema, owner, &element.name, argument)?;

                let path = extend(path, &element.name);
                self.rec_loop(
                    reference_schema,
                    element.r#type,
                    &origin,
                    Processing::Inputs,
                    &path,
                )?;
            }

            return Ok(());
        }

        if source.is_rest_parameter(arg) {
            self.warnings.push(
                [
                    "Warning: Rest parameters without explicit names are not supported. Skipping rest parameter at path: ",
                    &path.join(" > "),
                    ".\n\nUnsupported: `function example(...args: any[]) { }`.\nSupported: `function example(...namedArgs: [first: string, second: number]) { }`.\nPlease provide named rest parameters to ensure proper type resolution.",
                ]
                .concat(),
            );

            return Ok(());
        }

        let name = source.get_name(arg);
        let argument = ReferenceArgument {
            r#type: arg_type,
            is_required: source.is_optional_parameter(arg).then_some(false),
            documentation: self.get_symbol_documentation(arg),
        };
        Self::set_argument(reference_schema, owner, &name, argument)?;

        let path = extend(path, &name);
        self.rec_loop(
            reference_schema,
            arg_type,
            &origin,
            Processing::Inputs,
            &path,
        )
    }

    /// Sets `schemaType.args[name]`, `schema-parser.ts:1143` and `:1183`.
    ///
    /// # Errors
    ///
    /// Returns [`InferError::ReadOfUndefined`] when the parent type holds no
    /// property of the owner's name. The object guard writes the property
    /// before it walks the property type with this origin, `:1230-1242`, so the
    /// walk of this file never gets there. Where it does, the `schemaType` of
    /// `:1116-1118` is `undefined` in the reference, and the first line that
    /// reads `args` from it, `:1143` for an element of a labeled tuple and
    /// `:1176` for an ordinary parameter, fails with a runtime `TypeError` of
    /// the engine. The text of that `TypeError` is the engine's, no vendored
    /// file holds it and it is outside the byte contract, so the error names
    /// the read and carries no message.
    ///
    /// [to-object]: https://tc39.es/ecma262/#sec-toobject
    fn set_argument(
        reference_schema: &mut ReferenceSchemaOf<S>,
        owner: &ArgumentOwner<'_, S::Type>,
        name: &str,
        argument: ReferenceArgument<S::Type>,
    ) -> Result<(), InferError> {
        let schema_type = reference_schema
            .of_mut(owner.processing)
            .get_mut(&owner.parent_type)
            .and_then(|properties| properties.get_mut(owner.property_name));

        let Some(schema_type) = schema_type else {
            return Err(InferError::ReadOfUndefined {
                construct: "schemaType",
                member: String::from("args"),
            });
        };

        schema_type.args.insert(String::from(name), argument);

        Ok(())
    }

    /// Registers a list whose item type is no primitive and walks the item
    /// type, `schema-parser.ts:1204-1216`.
    fn rec_loop_list(
        &mut self,
        reference_schema: &mut ReferenceSchemaOf<S>,
        r#type: S::Type,
        info: &Info<S::Type>,
        processing: Processing,
        path: &[String],
    ) -> Result<(), InferError> {
        let source = self.source;

        let Some(item_type) = source.get_index_type_of_type(r#type) else {
            return Ok(());
        };

        if is_primitive(source, item_type) {
            return Ok(());
        }

        reference_schema
            .of_mut(processing)
            .entry(r#type)
            .or_default();

        let path = extend(path, "ITEM_TYPE");
        self.rec_loop(reference_schema, item_type, info, processing, &path)
    }

    /// Registers an object type, records each public property and walks its
    /// type, `schema-parser.ts:1217-1254`.
    ///
    /// The type is registered before a member is walked, which is what makes
    /// a type that refers to itself and two types that refer to each other
    /// terminate.
    fn rec_loop_object(
        &mut self,
        reference_schema: &mut ReferenceSchemaOf<S>,
        r#type: S::Type,
        processing: Processing,
        path: &[String],
    ) -> Result<(), InferError> {
        let source = self.source;
        let properties = get_public_properties_of_type(source, r#type);

        reference_schema
            .of_mut(processing)
            .entry(r#type)
            .or_default();

        for property in properties {
            let property_type = source.get_type_of_symbol_at_location(property);
            let name = source.get_name(property);

            if let Some(record) = reference_schema.of_mut(processing).get_mut(&r#type)
                && !record.contains_key(&name)
            {
                record.insert(
                    name.clone(),
                    ReferenceSchemaType {
                        symbol: property,
                        return_type: property_type,
                        args: Record::new(),
                    },
                );
            }

            let origin = Info {
                property_name: Some(name.clone()),
                parent_type: Some(r#type),
            };
            let path = extend(path, &name);

            self.rec_loop(reference_schema, property_type, &origin, processing, &path)?;
        }

        Ok(())
    }
}
