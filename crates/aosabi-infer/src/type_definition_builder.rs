// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-infer: the type mapping, which turns one type into the name, the
 * list shape and the nullability of a GraphQL type reference, and which owns
 * every naming rule of the inference.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/cli/builder/schema/type-definition-builder.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 *
 * The private `isTypeSymbol` of `:93-103` is not translated, because nothing
 * calls it.
 */

//! The type mapping of the inference.
//!
//! [`TypeDefinitionBuilder::get_type_definition`] is `getTypeDefinition` of
//! the reference, the function behind the thirty rows of the rule table. The
//! reference writes it as one arrow function of 540 lines.
//!
//! # JavaScript semantics the port reproduces
//!
//! - A type name is tested for truthiness at every use, and the empty string
//!   and `undefined` are both falsy. The port keeps a name as
//!   `Option<String>` and turns an empty string into `None` where one can
//!   arise, so the two falsy values are one.
//! - `options.isRequired !== undefined ? options.isRequired : !wasOptional`
//!   is `Option::unwrap_or`.
//! - `x.charAt(0).toUpperCase() + x.slice(1)` takes one UTF-16 code unit. A
//!   first character outside the basic plane is a lone surrogate there and
//!   has no upper case mapping, so the name stays as it is. A first character
//!   inside it takes the full Unicode mapping, which `char::to_uppercase`
//!   gives.
//! - `members.join('_').toUpperCase()` is the same mapping over a whole
//!   string, which `str::to_uppercase` gives.
//! - `new Set(defs)` over freshly built objects removes nothing, because a
//!   `Set` compares objects by identity and every call of `getTypeDefinition`
//!   returns a new object. The port therefore keeps every member definition
//!   of `:529-535`.
//! - A `Map` keeps the place of a key that is set again, which
//!   [`IndexMap::insert`] does.
//! - `startsWith`, `endsWith` and `slice(prefix.length)` work on code units.
//!   For two well formed strings a code unit prefix is a character prefix, so
//!   `str::starts_with`, `str::ends_with` and `str::strip_prefix` agree.
//!
//! [to-boolean]: https://tc39.es/ecma262/#sec-toboolean
//! [char-at]: https://tc39.es/ecma262/#sec-string.prototype.charat
//! [to-upper-case]: https://tc39.es/ecma262/#sec-string.prototype.touppercase
//! [same-value-zero]: https://tc39.es/ecma262/#sec-samevaluezero
//! [map-set]: https://tc39.es/ecma262/#sec-map.prototype.set

use aosabi_json::{
    map::IndexMap,
    value::KeyHasherBuilder, //
};

use crate::{
    error::InferError,
    type_source::{
        TypeFlags,
        TypeSource, //
    },
    types_helper::{
        exclude_null_undefined_from_type,
        get_promise_type,
        is_empty_object,
        is_function,
        is_list,
        is_literal_type,
        is_never,
        is_primitive,
        is_primitive_union,
        is_promise,
        is_subscription_repeater,
        safe_type_name, //
    }, //
};

/// A GraphQL union the mapping collected, `type-definition-builder.ts:18-22`.
#[derive(Debug, Clone)]
pub struct Union<T> {
    /// The name of the union.
    pub name: String,
    /// The union type the name stands for.
    pub raw_type: T,
    /// The names of the member types, in member order.
    pub types: Vec<String>,
}

/// A GraphQL enum the mapping collected, `type-definition-builder.ts:24-28`.
#[derive(Debug, Clone)]
pub struct Enum<T> {
    /// The name of the enum.
    pub name: String,
    /// The literal union the name stands for.
    pub raw_type: T,
    /// The enum values, in member order.
    pub values: Vec<String>,
}

/// The result of the mapping, `type-definition-builder.ts:30-37`.
///
/// For a list, `name` and `is_required` describe the innermost element,
/// `is_list_required` the outermost list, and `element` keeps the whole
/// element definition so a list of lists survives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldDefinition {
    /// The GraphQL type name.
    pub name: String,
    /// Whether the type is a list.
    pub is_list: bool,
    /// Whether the named type is non null.
    pub is_required: bool,
    /// Whether the list itself is non null, absent for a type that is no
    /// list.
    pub is_list_required: Option<bool>,
    /// The definition of the list element.
    pub element: Option<Box<FieldDefinition>>,
}

impl FieldDefinition {
    /// Builds the definition of a type that is no list, the object literal
    /// `{name, isList: false, isRequired}` of the reference.
    fn named(name: impl Into<String>, is_required: bool) -> Self {
        Self {
            name: name.into(),
            is_list: false,
            is_required,
            is_list_required: None,
            element: None,
        }
    }
}

/// The options of one mapping call, `type-definition-builder.ts:107-115`.
///
/// The default is `{isInputType: false, dryRun: false}`, the default of the
/// reference.
#[derive(Debug, Clone, Default)]
pub struct TypeDefinitionOptions {
    /// Whether the type stands in an input position.
    pub is_input_type: bool,
    /// The nullability the caller fixes, which otherwise follows from the
    /// type.
    pub is_required: Option<bool>,
    /// The property the type was reached through, which names an anonymous
    /// type.
    pub property_name: Option<String>,
    /// Whether the call leaves the name maps, the unions and the enums
    /// untouched.
    pub dry_run: bool,
}

/// The name map of one direction, a `Map<ts.Type, string>` of the reference.
type NameMap<T> = IndexMap<T, String, KeyHasherBuilder>;

/// Places a counter suffix inside a generic name, the `suffixPattern` arrow
/// of `type-definition-builder.ts:386`.
///
/// The counter goes after the argument prefix, so the second `UserConnection`
/// is `User_1Connection` and not `UserConnection_1`.
struct SuffixPattern {
    /// The name of the first type argument.
    prefix: String,
    /// The name after the prefix.
    rest: String,
}

impl SuffixPattern {
    /// Returns the name with `suffix` between the prefix and the rest.
    fn apply(&self, suffix: &str) -> String {
        format!("{}{suffix}{}", self.prefix, self.rest)
    }
}

/// What a stretch of the shape chain of `type-definition-builder.ts:411-631`
/// leaves behind.
enum Shape {
    /// The stretch returns this definition.
    Return(FieldDefinition),
    /// The stretch falls through to `:633` with this name.
    Named(String),
}

/// Turns an empty string into `None`, which makes the two falsy names of the
/// reference one.
fn non_empty(name: String) -> Option<String> {
    (!name.is_empty()).then_some(name)
}

/// Returns `name.charAt(0).toUpperCase() + name.slice(1)`.
///
/// `charAt(0)` is one UTF-16 code unit. A first character outside the basic
/// plane is a lone surrogate there, which has no upper case mapping, so such
/// a name is returned as it is.
pub(crate) fn capitalize(name: &str) -> String {
    let mut chars = name.chars();

    match chars.next() {
        Some(first) if first.len_utf16() == 1 => first.to_uppercase().chain(chars).collect(),
        _ => String::from(name),
    }
}

/// Maps types to GraphQL type references and collects the unions and the
/// enums it meets, `type-definition-builder.ts:39-676`.
pub struct TypeDefinitionBuilder<'s, S: TypeSource> {
    /// The type source, which stands in for the checker and the program.
    source: &'s S,
    /// The scalar names of the schema, `this.schema.scalars`.
    scalars: Vec<String>,
    /// The names of the output types, `typesNameMap`.
    types_name_map: NameMap<S::Type>,
    /// The names of the input types, `inputsNameMap`.
    inputs_name_map: NameMap<S::Type>,
    /// The unions in the order they were met.
    unions: Vec<Union<S::Type>>,
    /// The enums in the order they were met.
    enums: Vec<Enum<S::Type>>,
}

impl<'s, S: TypeSource> TypeDefinitionBuilder<'s, S> {
    /// Creates a builder over `source` with the scalar names `scalars`.
    ///
    /// The reference shares its schema object with the builder and reads
    /// `schema.scalars` alone, and the scalar list never changes after
    /// construction, so a copy of the names reads the same.
    #[must_use]
    pub fn new(source: &'s S, scalars: &[String]) -> Self {
        Self {
            source,
            scalars: scalars.to_vec(),
            types_name_map: NameMap::default(),
            inputs_name_map: NameMap::default(),
            unions: Vec::new(),
            enums: Vec::new(),
        }
    }

    /// Returns the name map of the direction `options` names, the `nameMap`
    /// of `type-definition-builder.ts:166`.
    fn name_map(&self, options: &TypeDefinitionOptions) -> &NameMap<S::Type> {
        if options.is_input_type {
            &self.inputs_name_map
        } else {
            &self.types_name_map
        }
    }

    /// Returns whether `name` is a scalar of the schema, which is
    /// `this.schema.scalars.includes(name)`.
    fn is_scalar(&self, name: &str) -> bool {
        self.scalars.iter().any(|scalar| scalar == name)
    }

    /// Returns the first type registered under `name`,
    /// `type-definition-builder.ts:56-64`.
    fn get_existing_type_by_name(name: &str, name_map: &NameMap<S::Type>) -> Option<S::Type> {
        name_map.iter().find_map(|(t, n)| (n == name).then_some(*t))
    }

    /// Returns a name no other type holds, or the name of a type of the same
    /// shape, `type-definition-builder.ts:66-91`.
    ///
    /// When the type that holds `original_name` and `type` are assignable to
    /// each other the name is shared, which unifies two structurally equal
    /// types. Otherwise a counter suffix is tried until the name is free.
    fn get_unique_name(
        &self,
        r#type: S::Type,
        original_name: &str,
        name_map: &NameMap<S::Type>,
        suffix_pattern: Option<&SuffixPattern>,
    ) -> String {
        let mut name = String::from(original_name);
        let mut i: u64 = 1;

        while let Some(existing_type) = Self::get_existing_type_by_name(&name, name_map) {
            if self.source.is_type_assignable_to(r#type, existing_type)
                && self.source.is_type_assignable_to(existing_type, r#type)
            {
                return name;
            }

            let suffix = format!("_{i}");
            name = match suffix_pattern {
                Some(pattern) => pattern.apply(&suffix),
                None => format!("{original_name}{suffix}"),
            };
            i += 1;
        }

        name
    }

    /// Maps `raw_type` to a GraphQL type reference,
    /// `type-definition-builder.ts:105-646`.
    ///
    /// Unless `options` ask for a dry run, the call registers the name of
    /// every object type it meets in the name map of its direction and
    /// collects the unions and the enums.
    ///
    /// # Errors
    ///
    /// Returns [`InferError::Thrown`] where the reference throws, which is
    /// for a literal union with a member that is no string and no number
    /// literal, for an enum value that is empty, and for a union with no
    /// member.
    pub fn get_type_definition(
        &mut self,
        raw_type: S::Type,
        options: &TypeDefinitionOptions,
    ) -> Result<FieldDefinition, InferError> {
        let source = self.source;
        let (r#type, was_optional) = exclude_null_undefined_from_type(source, raw_type);
        let flags = source.get_flags(r#type);

        if flags.contains(TypeFlags::VOID)
            || flags.contains(TypeFlags::UNDEFINED)
            || flags.contains(TypeFlags::NULL)
        {
            return Ok(FieldDefinition::named("Void", false));
        }

        let is_required = options.is_required.unwrap_or(!was_optional);

        if flags.contains(TypeFlags::ANY) || is_never(source, r#type) {
            let name = if is_never(source, r#type) {
                "JSONObject"
            } else {
                "Any"
            };
            return Ok(FieldDefinition::named(name, is_required));
        }

        if let Some(def) = self.get_wrapped_type_definition(r#type, options)? {
            return Ok(def);
        }

        if is_empty_object(source, r#type) {
            return Ok(FieldDefinition::named("JSONObject", is_required));
        }

        if let Some(type_name) = self.name_map(options).get(&r#type) {
            return Ok(FieldDefinition::named(type_name.clone(), is_required));
        }

        let type_name = self.get_type_name(r#type, options)?;

        match type_name.as_deref() {
            Some("JsonValue" | "JsonObject") => {
                return Ok(FieldDefinition::named("JSON", is_required));
            }
            Some("JsonArray") => {
                return Ok(FieldDefinition {
                    name: String::from("JSON"),
                    is_list: true,
                    is_required: true,
                    is_list_required: Some(is_required),
                    element: None,
                });
            }
            _ => {}
        }

        let type_name = self.get_unique_type_name(r#type, type_name, options)?;

        if let Some(name) = &type_name
            && self.is_scalar(name)
        {
            return Ok(FieldDefinition::named(name.clone(), is_required));
        }

        // The unique name may be the name of a registered type of the same
        // shape, and the type is then registered under it already.
        if let Some(name) = self.name_map(options).get(&r#type) {
            return Ok(FieldDefinition::named(name.clone(), is_required));
        }

        self.get_type_definition_by_shape(r#type, type_name, is_required, options)
    }

    /// Maps the item type of a repeater and the awaited type of a promise,
    /// `type-definition-builder.ts:140-153`.
    ///
    /// The result is `None` for a type that is neither, and for one of them
    /// that carries no type argument.
    fn get_wrapped_type_definition(
        &mut self,
        r#type: S::Type,
        options: &TypeDefinitionOptions,
    ) -> Result<Option<FieldDefinition>, InferError> {
        let source = self.source;

        if is_subscription_repeater(source, r#type)
            && let Some(repeater_item_type) = source.get_type_arguments(r#type).into_iter().next()
        {
            return self
                .get_type_definition(repeater_item_type, options)
                .map(Some);
        }

        if is_promise(source, r#type)
            && let Some(promise_type) = get_promise_type(source, r#type)
        {
            return self.get_type_definition(promise_type, options).map(Some);
        }

        Ok(None)
    }

    /// Finds the name of `type` before deduplication,
    /// `type-definition-builder.ts:178-339`.
    fn get_type_name(
        &mut self,
        r#type: S::Type,
        options: &TypeDefinitionOptions,
    ) -> Result<Option<String>, InferError> {
        let type_name = self.get_symbol_type_name(r#type);
        let type_name = self.get_declaration_type_name(r#type, type_name);
        let type_name = self.get_property_type_name(r#type, type_name, options);

        self.get_generic_type_name(r#type, type_name, options)
    }

    /// Reads the name from the alias symbol, the symbol and the `__typename`
    /// literal, `type-definition-builder.ts:178-198`.
    ///
    /// The reference asks the checker for the type of `__typename` at a
    /// location node, which is the value declaration of the property, or the
    /// first declaration of the alias symbol, or the first declaration of the
    /// symbol. The seam has no node, so the location counts as present when
    /// one of the three symbols that would supply it is.
    fn get_symbol_type_name(&self, r#type: S::Type) -> Option<String> {
        let source = self.source;
        let alias_symbol = source.get_alias_symbol(r#type);
        let symbol = source.get_symbol(r#type);

        let mut type_name = alias_symbol
            .and_then(|alias| non_empty(source.get_name(alias)))
            .or_else(|| symbol.and_then(|symbol| non_empty(source.get_name(symbol))));

        let Some(typename_symbol) = source.get_property(r#type, "__typename") else {
            return type_name;
        };

        let has_location_node = source.has_value_declaration(typename_symbol)
            || alias_symbol.is_some()
            || symbol.is_some();

        if has_location_node {
            let typename_type = source.get_type_of_symbol_at_location(typename_symbol);

            if source
                .get_flags(typename_type)
                .contains(TypeFlags::STRING_LITERAL)
            {
                type_name = source.get_literal_value(typename_type).and_then(non_empty);
            }
        }

        type_name
    }

    /// Names an anonymous type after a declaration that has a name,
    /// `type-definition-builder.ts:201-235`.
    ///
    /// The walk over the declaration nodes and their parents is
    /// [`TypeSource::get_declaration_name`], and a walk that finds nothing
    /// leaves the name as it is.
    fn get_declaration_type_name(
        &self,
        r#type: S::Type,
        type_name: Option<String>,
    ) -> Option<String> {
        let source = self.source;

        if !matches!(type_name.as_deref(), Some("__type" | "__object") | None) {
            return type_name;
        }

        let symbol = source
            .get_alias_symbol(r#type)
            .or_else(|| source.get_symbol(r#type));

        match symbol.and_then(|symbol| source.get_declaration_name(symbol)) {
            Some(name) => non_empty(name),
            None => type_name,
        }
    }

    /// Names a type after the property it was reached through,
    /// `type-definition-builder.ts:237-254`.
    ///
    /// That holds for a type that still has no name, and in an input position
    /// for every type without an alias symbol, which is naming rule 6.
    fn get_property_type_name(
        &self,
        r#type: S::Type,
        type_name: Option<String>,
        options: &TypeDefinitionOptions,
    ) -> Option<String> {
        let source = self.source;
        let effective_property_name = options
            .property_name
            .as_deref()
            .filter(|name| !name.is_empty());

        let is_anonymous = matches!(type_name.as_deref(), Some("__type" | "__object") | None);
        let is_input_property = options.is_input_type
            && effective_property_name.is_some()
            && source.get_alias_symbol(r#type).is_none();
        let is_scalar = type_name
            .as_deref()
            .is_some_and(|name| self.is_scalar(name));

        if (is_anonymous || is_input_property)
            && !is_primitive(source, r#type)
            && !is_scalar
            && let Some(property_name) = effective_property_name
        {
            return Some(capitalize(property_name));
        }

        type_name
    }

    /// Returns the type arguments the naming reads, which are those of the
    /// reference and otherwise those of the alias,
    /// `type-definition-builder.ts:275-280` and `:371-374`.
    fn get_naming_type_arguments(&self, r#type: S::Type) -> Vec<S::Type> {
        let type_arguments = self.source.get_type_arguments(r#type);

        if type_arguments.is_empty()
            && let Some(alias_type_arguments) = self.source.get_alias_type_arguments(r#type)
        {
            return alias_type_arguments;
        }

        type_arguments
    }

    /// Prefixes the name of a generic instantiation with the names of its
    /// type arguments, sanitizes the name and capitalizes it,
    /// `type-definition-builder.ts:272-339`.
    ///
    /// For a utility alias of the default library only the prefix survives,
    /// which collides with the base type on purpose so the deduplication
    /// decides.
    fn get_generic_type_name(
        &mut self,
        r#type: S::Type,
        type_name: Option<String>,
        options: &TypeDefinitionOptions,
    ) -> Result<Option<String>, InferError> {
        let source = self.source;
        let Some(mut type_name) = type_name else {
            return Ok(None);
        };

        let type_arguments = self.get_naming_type_arguments(r#type);
        let library_type = source.is_library_type(r#type);
        let alias_symbol = source.get_alias_symbol(r#type);
        let is_type_alias = alias_symbol.is_some();
        let is_user_alias = is_type_alias && !library_type;
        let is_alias_of_other_name =
            alias_symbol.is_none_or(|alias| source.get_name(alias) != type_name);

        if !type_arguments.is_empty()
            && !is_user_alias
            && (is_alias_of_other_name || source.get_alias_type_arguments(r#type).is_some())
        {
            let is_utility = library_type && is_type_alias;
            let mut prefix = String::new();

            for (index, arg) in type_arguments.into_iter().enumerate() {
                let literal = is_literal_type(source, arg) || is_primitive_union(source, arg);

                if is_utility && index != 0 && literal {
                    continue;
                }

                prefix.push_str(&self.get_type_definition(arg, options)?.name);
            }

            if is_utility {
                type_name = prefix;
            } else if !type_name.starts_with(&prefix) {
                type_name = format!("{prefix}{type_name}");
            }
        }

        Ok(Some(capitalize(&safe_type_name(&type_name))))
    }

    /// Adds the input suffix and deduplicates the name,
    /// `type-definition-builder.ts:358-392`.
    ///
    /// The reference enters the generic stretch when `aliasTypeArguments` or
    /// `typeArguments` is truthy, and an empty array is truthy. The length
    /// test of `:376` follows at once, so an empty list and an absent one
    /// both leave the suffix pattern unset, and the port asks for a list
    /// that is not empty.
    fn get_unique_type_name(
        &mut self,
        r#type: S::Type,
        type_name: Option<String>,
        options: &TypeDefinitionOptions,
    ) -> Result<Option<String>, InferError> {
        let source = self.source;
        let Some(mut type_name) = type_name else {
            return Ok(None);
        };

        if self.is_scalar(&type_name) {
            return Ok(Some(type_name));
        }

        if options.is_input_type
            && !type_name.ends_with("Input")
            && !is_primitive_union(source, r#type)
        {
            type_name = format!("{type_name}Input");
        }

        let mut suffix_pattern = None;

        if let Some(first_arg) = self.get_naming_type_arguments(r#type).into_iter().next() {
            let prefix = self.get_type_definition(first_arg, options)?.name;

            if let Some(rest) = type_name.strip_prefix(&prefix) {
                suffix_pattern = Some(SuffixPattern {
                    rest: String::from(rest),
                    prefix,
                });
            }
        }

        Ok(Some(self.get_unique_name(
            r#type,
            &type_name,
            self.name_map(options),
            suffix_pattern.as_ref(),
        )))
    }

    /// Maps `type` by its shape and registers its name,
    /// `type-definition-builder.ts:411-645`.
    ///
    /// The shapes are tested in the order of the reference: a list, a
    /// literal union, a primitive, an intersection, a union and a function.
    /// A shape that does not return leaves a name behind, and a type that
    /// still has none is `JSONObject`.
    fn get_type_definition_by_shape(
        &mut self,
        r#type: S::Type,
        type_name: Option<String>,
        is_required: bool,
        options: &TypeDefinitionOptions,
    ) -> Result<FieldDefinition, InferError> {
        let source = self.source;
        let mut type_name = type_name;

        if is_list(source, r#type) {
            if let Some(def) = self.get_list_type_definition(r#type, is_required, options)? {
                return Ok(def);
            }
        } else if is_primitive_union(source, r#type) {
            type_name = self.get_enum_type_name(r#type, type_name, options)?;
        } else if is_primitive(source, r#type) {
            return Ok(self.get_primitive_type_definition(r#type, is_required));
        } else if source.is_intersection(r#type) {
            type_name = Some(self.get_intersection_type_name(r#type, type_name, options)?);
        } else if source.is_union(r#type) {
            match self.get_union_type_definition(r#type, type_name, is_required, options)? {
                Shape::Return(def) => return Ok(def),
                Shape::Named(name) => type_name = Some(name),
            }
        } else if is_function(source, r#type)
            && let Some(def) = self.get_function_type_definition(r#type, options)?
        {
            return Ok(def);
        }

        let type_name = type_name.unwrap_or_else(|| String::from("JSONObject"));

        if !options.dry_run {
            if options.is_input_type {
                self.inputs_name_map.insert(r#type, type_name.clone());
            } else {
                self.types_name_map.insert(r#type, type_name.clone());
            }
        }

        Ok(FieldDefinition::named(type_name, is_required))
    }

    /// Maps a list to a list of its number index type,
    /// `type-definition-builder.ts:411-429`.
    ///
    /// The whole element definition is kept, so a list of lists survives.
    fn get_list_type_definition(
        &mut self,
        r#type: S::Type,
        is_required: bool,
        options: &TypeDefinitionOptions,
    ) -> Result<Option<FieldDefinition>, InferError> {
        let Some(list_type) = self.source.get_index_type_of_type(r#type) else {
            return Ok(None);
        };

        let def = self.get_type_definition(list_type, options)?;

        Ok(Some(FieldDefinition {
            name: def.name.clone(),
            is_list: true,
            is_required: def.is_required,
            is_list_required: Some(is_required),
            element: Some(Box::new(def)),
        }))
    }

    /// Collects a literal union as an enum and returns its name,
    /// `type-definition-builder.ts:430-476`.
    ///
    /// An enum is registered once per name, and its type is written into
    /// both name maps, so one enum serves the input and the output side.
    fn get_enum_type_name(
        &mut self,
        r#type: S::Type,
        type_name: Option<String>,
        options: &TypeDefinitionOptions,
    ) -> Result<Option<String>, InferError> {
        let source = self.source;
        let mut members = Vec::new();

        for t in source.get_types(r#type) {
            let Some(name) = source.get_literal_value(t) else {
                return Err(InferError::Thrown(String::from(
                    "Invalid type for enum member",
                )));
            };

            if name.is_empty() {
                return Err(InferError::Thrown(String::from(
                    "Enum member name is undefined",
                )));
            }

            members.push(safe_type_name(&name));
        }

        if members.is_empty() {
            return Ok(type_name);
        }

        let type_name = type_name
            .or_else(|| source.get_type_node_name(r#type).and_then(non_empty))
            .unwrap_or_else(|| {
                let joined = members.join("_").to_uppercase();

                if options.is_input_type {
                    format!("{joined}Input")
                } else {
                    joined
                }
            });

        if !options.dry_run && !self.enums.iter().any(|e| e.name == type_name) {
            self.enums.push(Enum {
                name: type_name.clone(),
                values: members,
                raw_type: r#type,
            });

            self.types_name_map.insert(r#type, type_name.clone());
            self.inputs_name_map.insert(r#type, type_name.clone());
        }

        Ok(Some(type_name))
    }

    /// Maps a primitive and a literal of one to `String`, `Number` or
    /// `Boolean`, `type-definition-builder.ts:478-499`.
    fn get_primitive_type_definition(&self, r#type: S::Type, is_required: bool) -> FieldDefinition {
        let flags = self.source.get_flags(r#type);
        let mut type_name = self.source.type_to_string(r#type);

        if flags.contains(TypeFlags::STRING_LITERAL) {
            type_name = String::from("String");
        } else if flags.contains(TypeFlags::NUMBER_LITERAL) {
            type_name = String::from("Number");
        } else if flags.contains(TypeFlags::BOOLEAN_LITERAL) {
            type_name = String::from("Boolean");
        } else if flags.contains(TypeFlags::STRING) {
            type_name = String::from("String");
        } else if flags.contains(TypeFlags::NUMBER) {
            type_name = String::from("Number");
        } else if flags.contains(TypeFlags::BOOLEAN) {
            type_name = String::from("Boolean");
        }

        FieldDefinition::named(safe_type_name(&type_name), is_required)
    }

    /// Names an intersection after its alias or after its members joined
    /// with `And`, `type-definition-builder.ts:500-509`.
    fn get_intersection_type_name(
        &mut self,
        r#type: S::Type,
        type_name: Option<String>,
        options: &TypeDefinitionOptions,
    ) -> Result<String, InferError> {
        let mut type_names = Vec::new();

        for t in self.source.get_types(r#type) {
            type_names.push(self.get_type_definition(t, options)?.name);
        }

        Ok(safe_type_name(
            &type_name.unwrap_or_else(|| type_names.join("And")),
        ))
    }

    /// Returns whether a union member is a primitive or a literal union, or
    /// a list of one, the `hasPrimitivesOrEnum` arrow of
    /// `type-definition-builder.ts:514-527`.
    ///
    /// A list with no number index type counts as one, because the reference
    /// does not know what it is.
    fn is_primitive_or_enum_member(&self, t: S::Type) -> bool {
        let source = self.source;
        let element = if is_list(source, t) {
            source.get_index_type_of_type(t)
        } else {
            Some(t)
        };

        element.is_none_or(|element| {
            is_primitive(source, element) || is_primitive_union(source, element)
        })
    }

    /// Collects a union of object types, and collapses every other union to
    /// `JSON` or to its first member, `type-definition-builder.ts:510-610`.
    ///
    /// The collapse is quirk `Q09_INPUT_UNION_FIRST_MEMBER`. `required` of
    /// `:512` compares the length of a list with itself and is always true.
    fn get_union_type_definition(
        &mut self,
        r#type: S::Type,
        type_name: Option<String>,
        is_required: bool,
        options: &TypeDefinitionOptions,
    ) -> Result<Shape, InferError> {
        let union_types = self.source.get_types(r#type);
        let has_primitives_or_enum = union_types
            .iter()
            .any(|t| self.is_primitive_or_enum_member(*t));

        let member_options = TypeDefinitionOptions {
            is_required: Some(true),
            ..options.clone()
        };
        let mut union_type_defs = Vec::new();

        for t in union_types {
            union_type_defs.push(self.get_type_definition(t, &member_options)?);
        }

        // A member that appears bare and as a list keeps the list alone.
        let list_types: Vec<FieldDefinition> = union_type_defs
            .iter()
            .filter(|t| t.is_list)
            .cloned()
            .collect();

        for list_type in &list_types {
            let index = union_type_defs
                .iter()
                .position(|t| t.name == list_type.name && !t.is_list);

            if let Some(index) = index {
                union_type_defs.remove(index);
            }
        }

        let type_names: Vec<String> = union_type_defs.iter().map(|t| t.name.clone()).collect();
        let type_name = safe_type_name(&type_name.unwrap_or_else(|| type_names.join("Or")));
        let type_name = self.get_unique_name(r#type, &type_name, self.name_map(options), None);

        if union_type_defs.len() > 1 && !options.is_input_type && !has_primitives_or_enum {
            if !options.dry_run && !self.unions.iter().any(|u| u.name == type_name) {
                self.unions.push(Union {
                    name: type_name.clone(),
                    types: type_names,
                    raw_type: r#type,
                });
            }

            return Ok(Shape::Named(type_name));
        }

        if union_type_defs
            .iter()
            .any(|t| matches!(t.name.as_str(), "JSON" | "JSONObject" | "Any"))
        {
            return Ok(Shape::Return(FieldDefinition::named("JSON", is_required)));
        }

        let Some(type_def) = union_type_defs.first() else {
            return Err(InferError::Thrown(String::from(
                "Cannot get type definition",
            )));
        };

        let is_list = union_type_defs
            .iter()
            .any(|t| t.is_list && t.name == type_def.name);

        Ok(Shape::Return(FieldDefinition {
            name: type_def.name.clone(),
            is_list,
            is_required,
            is_list_required: None,
            element: None,
        }))
    }

    /// Maps a function to the awaited return type of its first call
    /// signature, `type-definition-builder.ts:611-631`.
    fn get_function_type_definition(
        &mut self,
        r#type: S::Type,
        options: &TypeDefinitionOptions,
    ) -> Result<Option<FieldDefinition>, InferError> {
        let source = self.source;
        let signature = source.get_call_signatures(r#type).into_iter().next();
        let mut return_type = signature.map(|signature| source.get_return_type(signature));

        if let Some(promise) = return_type
            && is_promise(source, promise)
            && let Some(pt) = get_promise_type(source, promise)
        {
            return_type = Some(pt);
        }

        match return_type {
            Some(return_type) => self.get_type_definition(return_type, options).map(Some),
            None => Ok(None),
        }
    }

    /// Returns the unions collected so far, `type-definition-builder.ts:648-650`.
    #[must_use]
    pub fn get_unions(&self) -> &[Union<S::Type>] {
        &self.unions
    }

    /// Returns the enums collected so far, `type-definition-builder.ts:652-654`.
    #[must_use]
    pub fn get_enums(&self) -> &[Enum<S::Type>] {
        &self.enums
    }

    /// Returns every output type named so far with its name, in the order
    /// the builder named them, the entries of `typesNameMap`.
    #[must_use]
    pub fn type_names(&self) -> Vec<(S::Type, String)> {
        self.types_name_map
            .iter()
            .map(|(r#type, name)| (*r#type, name.clone()))
            .collect()
    }

    /// Renders a definition as a GraphQL type reference,
    /// `type-definition-builder.ts:656-675`.
    #[must_use]
    pub fn type_definition_to_graphql_type(type_definition: &FieldDefinition) -> String {
        let mut r#type = type_definition.name.clone();

        if type_definition.is_required {
            r#type = format!("{type}!");
        }

        if type_definition.is_list {
            r#type = format!("[{type}]");

            if type_definition.is_list_required == Some(true) {
                r#type = format!("{type}!");
            }
        }

        r#type
    }
}
