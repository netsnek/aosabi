// SPDX-License-Identifier: MIT
/*
 * aosabi-core boot prebuilt write: the schema record of the SDL and the IR of
 * a build, written on the machine that builds the app.
 *
 * No analogue in the reference. It runs the boot of the SDL and the IR of
 * crates/aosabi-core/src/boot/mod.rs and the schema build of
 * crates/aosabi-core/src/create_schema.rs over the two texts and writes what
 * the reader of its parent module makes the schema from.
 */

//! The writer of the schema record.
//!
//! [`write()`] reads the IR and holds it to the SDL as a boot does, builds the
//! schema `create_schema` builds from the SDL with the bindings every boot
//! lays over it, the bound scalars of the SDL, and validates it, so a build
//! whose schema a boot could not serve takes no record and boots from its SDL
//! as before. It then writes the type map of that schema in its order, with
//! the description and the URL a custom scalar takes from the SDL before its
//! binding, which a second build with no bindings answers, since the reader
//! lays the binding over them again. A default value is the value the build
//! with the bound scalars made of its literal, which the reader takes as it
//! is, since every boot binds the same scalars to the same SDL.

use std::sync::Arc;

use aosabi_json::JsValue;
use aosabi_lang::{
    ParseOptions,
    js_string::to_string_lossy, //
};
use aosabi_type::{
    Argument,
    BuildSchemaOptions,
    DefinitionError,
    Directive,
    Field,
    InputField,
    NamedType,
    ObjMap,
    Schema,
    Type,
    specified_directives,
    validate_schema, //
};
use aosabi_validate::build_schema::build_schema;

use super::{
    ARRAY, BITS, DEFINED, DEPTH, DIGITS, FALSE, Kind, LAST, LIST, MAGIC, MORE, NAMED, NON_NULL,
    NULL, NUMBER, OBJECT, SHARED, SPECIFIED, STRING, TRUE, UNDEFINED, digest,
};
use crate::{
    boot::{
        check,
        ir::Ir,
        resolve_type, //
    },
    create_schema::{
        ResolverMap,
        make_executable_schema, //
    },
    glue::resolve_type::{
        ResolveTypeCheck,
        ResolveTypeRule, //
    },
    scalars::bound_scalars,
};

/// Writes the schema record of the SDL `type_defs` and the JSON text `ir` of
/// the IR of the same build.
///
/// # Errors
///
/// Returns what the boot of the two texts returns, a message that names the
/// first error of the validation of the schema, or a message that names the
/// part of the schema a record holds no form of.
pub fn write(type_defs: &str, ir: &str) -> Result<Vec<u8>, String> {
    let model = Ir::read(ir)?;
    let rules = resolve_type::rules(&model);
    check::against_sdl(type_defs, &model, &rules)?;
    let map = ResolverMap {
        scalars: bound_scalars(type_defs),
        ..ResolverMap::default()
    };
    let schema = make_executable_schema(type_defs, &map).map_err(|error| error.to_string())?;
    if let Some(error) = validate_schema(&schema)
        .map_err(|error| message(&error))?
        .first()
    {
        return Err(format!(
            "the schema of the build is not valid, so the boot builds it from the SDL: {}",
            to_string_lossy(&error.message)
        ));
    }
    let plain = build_schema(
        type_defs,
        BuildSchemaOptions::default(),
        ParseOptions::default(),
    )
    .map_err(|error| message(&error))?;
    let mut out = Out::default();
    out.bytes.extend_from_slice(MAGIC.as_bytes());
    out.count(type_defs.len());
    out.num(digest(type_defs));
    write_rules(&mut out, &rules);
    write_members(&mut out, &model);
    Writer::new(&schema, &plain).schema(&mut out)?;
    Ok(out.bytes)
}

/// Returns the message of an error of the type layer.
fn message(error: &DefinitionError) -> String {
    to_string_lossy(error.message()).into_owned()
}

/// The bytes of a record being written.
#[derive(Default)]
struct Out {
    /// The bytes so far.
    bytes: Vec<u8>,
}

impl Out {
    /// Writes a number.
    fn num(&mut self, mut value: u64) {
        while value >= DIGITS {
            self.bytes.push(MORE + digit(value));
            value >>= BITS;
        }
        self.bytes.push(LAST + digit(value));
    }

    /// Writes a count or a length.
    fn count(&mut self, count: usize) {
        self.num(u64::try_from(count).unwrap_or(u64::MAX));
    }

    /// Writes a flag.
    fn flag(&mut self, flag: bool) {
        self.num(u64::from(flag));
    }

    /// Writes a text.
    fn text(&mut self, text: &str) {
        self.count(text.len());
        self.bytes.extend_from_slice(text.as_bytes());
    }

    /// Writes a text that may be absent.
    fn optional(&mut self, text: Option<&str>) {
        match text {
            None => self.num(0),
            Some(text) => {
                self.count(text.len() + 1);
                self.bytes.extend_from_slice(text.as_bytes());
            }
        }
    }

    /// Writes a count and the texts of `texts`.
    fn texts<'a>(&mut self, texts: impl ExactSizeIterator<Item = &'a str>) {
        self.count(texts.len());
        for text in texts {
            self.text(text);
        }
    }
}

/// Returns the lowest digit of `value`.
fn digit(value: u64) -> u8 {
    u8::try_from(value % DIGITS).unwrap_or(0)
}

/// Writes the type resolvers of the IR.
fn write_rules(out: &mut Out, rules: &[ResolveTypeRule]) {
    out.count(rules.len());
    for rule in rules {
        out.text(rule.type_name);
        out.texts(rule.valid_names.iter().copied());
        out.count(rule.checks.len());
        for check in rule.checks {
            let (any, fields, name) = match check {
                ResolveTypeCheck::AnyField { fields, name } => (true, fields, name),
                ResolveTypeCheck::AllFields { fields, name } => (false, fields, name),
            };
            out.flag(any);
            out.text(name);
            out.texts(fields.iter().copied());
        }
    }
}

/// Writes the members of the unions and the interfaces of the IR: the
/// objects with the interfaces each implements, the interfaces and the
/// unions with their members.
fn write_members(out: &mut Out, model: &Ir) {
    out.count(model.objects.len());
    for object in &model.objects {
        out.text(&object.name);
        out.texts(object.implements.iter().map(String::as_str));
    }
    out.texts(
        model
            .interfaces
            .iter()
            .map(|interface| interface.name.as_str()),
    );
    out.count(model.unions.len());
    for union in &model.unions {
        out.text(&union.name);
        out.texts(union.members.iter().map(String::as_str));
    }
}

/// The writer of the schema part: the schema, the schema of the same SDL
/// with no bindings, and the place of each named type.
struct Writer<'s> {
    /// The schema `create_schema` builds.
    schema: &'s Schema,
    /// The schema of the SDL with no bindings, whose scalars hold what the
    /// SDL gives them.
    plain: &'s Schema,
    /// The types of the record, the type map of `schema` without the types
    /// every schema shares.
    types: Vec<&'s NamedType>,
    /// The place of each named type, the shared ones first.
    places: ObjMap<usize>,
}

impl<'s> Writer<'s> {
    /// Returns the writer of `schema`, with `plain` the schema of its SDL
    /// with no bindings.
    fn new(schema: &'s Schema, plain: &'s Schema) -> Self {
        let mut places = ObjMap::default();
        for (place, name) in SHARED.iter().enumerate() {
            places.insert(String::from(*name), place);
        }
        let types: Vec<&NamedType> = schema
            .get_type_map()
            .values()
            .filter(|ty| !places.contains_key(ty.name()))
            .collect();
        for (place, ty) in types.iter().enumerate() {
            places.insert(String::from(ty.name()), SHARED.len() + place);
        }
        Self {
            schema,
            plain,
            types,
            places,
        }
    }

    /// Writes the schema part.
    fn schema(&self, out: &mut Out) -> Result<(), String> {
        out.optional(self.schema.description.as_deref());
        out.count(self.types.len());
        for ty in &self.types {
            self.named(out, ty)?;
        }
        let order = self.order()?;
        out.count(order.len());
        for place in order {
            out.count(place);
        }
        let declared: Vec<usize> = SHARED
            .iter()
            .enumerate()
            .filter(|(_, name)| {
                self.plain
                    .get_type(name)
                    .is_some_and(aosabi_type::is_specified_scalar_type)
            })
            .map(|(place, _)| place)
            .collect();
        out.count(declared.len());
        for place in declared {
            out.count(place);
        }
        let roots = [
            self.schema.get_query_type(),
            self.schema.get_mutation_type(),
            self.schema.get_subscription_type(),
        ];
        for root in roots {
            match root {
                None => out.num(0),
                Some(root) => out.count(self.place(root.name())? + 1),
            }
        }
        out.count(self.schema.get_directives().len());
        for directive in self.schema.get_directives() {
            self.directive(out, directive)?;
        }
        Ok(())
    }

    /// Returns the places of the types the second build of the SDL path builds
    /// its schema from, in its order: the type map of the first build without
    /// the types of the introspection and the specified scalars, and then the
    /// named type of every argument of a directive that is not specified,
    /// where that list lacks it, so the schema of the record takes the order
    /// of the type map of that build.
    fn order(&self) -> Result<Vec<usize>, String> {
        let mut names: Vec<&str> = self
            .plain
            .get_type_map()
            .keys()
            .map(String::as_str)
            .filter(|name| !name.starts_with("__") && !SHARED[..5].contains(name))
            .collect();
        for directive in self.plain.get_directives() {
            if aosabi_type::is_specified_directive(directive) {
                continue;
            }
            for arg in &directive.args {
                let arg_type = aosabi_type::get_named_type(&arg.r#type);
                if !names.contains(&arg_type.name()) {
                    names.push(
                        SHARED
                            .iter()
                            .copied()
                            .find(|shared| *shared == arg_type.name())
                            .ok_or_else(|| {
                                format!(
                                    "the directive {} takes the type {}, which the record holds in no place of its own",
                                    directive.name,
                                    arg_type.name()
                                )
                            })?,
                    );
                }
            }
        }
        names.into_iter().map(|name| self.place(name)).collect()
    }

    /// Returns the place of the named type `name`.
    fn place(&self, name: &str) -> Result<usize, String> {
        self.places
            .get(name)
            .copied()
            .ok_or_else(|| format!("the schema names the type {name}, which its type map lacks"))
    }

    /// Writes one named type, its head and then its body.
    fn named(&self, out: &mut Out, ty: &NamedType) -> Result<(), String> {
        let mut body = Out::default();
        let (kind, description) = match ty {
            NamedType::Scalar(scalar) => (Kind::Scalar, scalar.description.as_deref()),
            NamedType::Object(object) => {
                held_bare(object.extensions.is_empty(), ty.name())?;
                self.named_list(
                    &mut body,
                    object.get_interfaces().map_err(|error| message(&error))?,
                )?;
                self.fields(
                    &mut body,
                    object.get_fields().map_err(|error| message(&error))?,
                )?;
                (Kind::Object, object.description.as_deref())
            }
            NamedType::Interface(interface) => {
                held_bare(interface.extensions.is_empty(), ty.name())?;
                self.named_list(
                    &mut body,
                    interface
                        .get_interfaces()
                        .map_err(|error| message(&error))?,
                )?;
                self.fields(
                    &mut body,
                    interface.get_fields().map_err(|error| message(&error))?,
                )?;
                (Kind::Interface, interface.description.as_deref())
            }
            NamedType::Union(union) => {
                held_bare(union.extensions.is_empty(), ty.name())?;
                self.named_list(
                    &mut body,
                    union.get_types().map_err(|error| message(&error))?,
                )?;
                (Kind::Union, union.description.as_deref())
            }
            NamedType::Enum(enum_type) => {
                held_bare(enum_type.extensions.is_empty(), ty.name())?;
                enum_values(
                    &mut body,
                    enum_type.get_values().map_err(|error| message(&error))?,
                )?;
                (Kind::Enum, enum_type.description.as_deref())
            }
            NamedType::InputObject(input) => {
                held_bare(input.extensions.is_empty(), ty.name())?;
                self.input_fields(
                    &mut body,
                    input.get_fields().map_err(|error| message(&error))?,
                )?;
                (Kind::InputObject, input.description.as_deref())
            }
        };
        out.num(kind.code());
        out.text(ty.name());
        match ty {
            NamedType::Scalar(_) => {
                let Some(NamedType::Scalar(plain)) = self.plain.get_type(ty.name()) else {
                    return Err(format!("the SDL holds no scalar {}", ty.name()));
                };
                out.optional(plain.description.as_deref());
                out.optional(plain.specified_by_url.as_deref());
            }
            NamedType::InputObject(input) => {
                out.optional(description);
                out.flag(input.is_one_of);
            }
            _ => out.optional(description),
        }
        out.count(body.bytes.len());
        out.bytes.append(&mut body.bytes);
        Ok(())
    }

    /// Writes a count and the places of the interfaces or the members `types`.
    fn named_list(&self, out: &mut Out, types: &[NamedType]) -> Result<(), String> {
        out.count(types.len());
        for ty in types {
            out.count(self.place(ty.name())?);
        }
        Ok(())
    }

    /// Writes a type reference.
    fn wrapped(&self, out: &mut Out, ty: &Type) -> Result<(), String> {
        if let Some(list) = ty.as_list() {
            out.num(LIST);
            return self.wrapped(out, &list.of_type);
        }
        if let Some(non_null) = ty.as_non_null() {
            out.num(NON_NULL);
            return self.wrapped(out, non_null.of_type());
        }
        let place = self.place(aosabi_type::get_named_type(ty).name())?;
        out.num(NAMED + u64::try_from(place).unwrap_or(u64::MAX));
        Ok(())
    }

    /// Writes a count and the fields of an object or an interface.
    fn fields(&self, out: &mut Out, fields: &ObjMap<Field>) -> Result<(), String> {
        out.count(fields.len());
        for field in fields.values() {
            held_bare(field.extensions.is_empty(), &field.name)?;
            out.text(&field.name);
            out.optional(field.description.as_deref());
            self.wrapped(out, &field.r#type)?;
            out.optional(field.deprecation_reason.as_deref());
            self.arguments(out, &field.args)?;
        }
        Ok(())
    }

    /// Writes a count and the arguments of a field or a directive.
    fn arguments(&self, out: &mut Out, args: &[Argument]) -> Result<(), String> {
        out.count(args.len());
        for arg in args {
            held_bare(arg.extensions.is_empty(), &arg.name)?;
            out.text(&arg.name);
            out.optional(arg.description.as_deref());
            self.wrapped(out, &arg.r#type)?;
            value(out, &arg.default_value, 0)?;
            out.optional(arg.deprecation_reason.as_deref());
        }
        Ok(())
    }

    /// Writes a count and the fields of an input object.
    fn input_fields(&self, out: &mut Out, fields: &ObjMap<InputField>) -> Result<(), String> {
        out.count(fields.len());
        for field in fields.values() {
            held_bare(field.extensions.is_empty(), &field.name)?;
            out.text(&field.name);
            out.optional(field.description.as_deref());
            self.wrapped(out, &field.r#type)?;
            value(out, &field.default_value, 0)?;
            out.optional(field.deprecation_reason.as_deref());
        }
        Ok(())
    }

    /// Writes one directive: one every schema shares by its name, and one
    /// the SDL defines whole.
    fn directive(&self, out: &mut Out, directive: &Arc<Directive>) -> Result<(), String> {
        if specified_directives()
            .iter()
            .any(|specified| Arc::ptr_eq(specified, directive))
        {
            out.num(SPECIFIED);
            out.text(&directive.name);
            return Ok(());
        }
        if directive.location_names.is_some() {
            return Err(format!(
                "the directive {} names a location the type layer holds no form of",
                directive.name
            ));
        }
        held_bare(directive.extensions.is_empty(), &directive.name)?;
        out.num(DEFINED);
        out.text(&directive.name);
        out.optional(directive.description.as_deref());
        out.texts(directive.locations.iter().map(|location| location.as_str()));
        out.flag(directive.is_repeatable);
        self.arguments(out, &directive.args)
    }
}

/// Holds a type, a field, an argument or a directive named `name` to no
/// extensions, which a builder of the SDL never sets, where `bare` is true.
fn held_bare(bare: bool, name: &str) -> Result<(), String> {
    if bare {
        return Ok(());
    }
    Err(format!(
        "{name} carries extensions, which the builders of the SDL never set and a record holds none of"
    ))
}

/// Writes a count and the values of an enum, each the value of its name.
fn enum_values(out: &mut Out, values: &[aosabi_type::EnumValue]) -> Result<(), String> {
    out.count(values.len());
    for value in values {
        held_bare(value.extensions.is_empty(), &value.name)?;
        if !matches!(
            &value.value,
            JsValue::String(text) if text.as_str() == Some(value.name.as_str())
        ) {
            return Err(format!(
                "the value {} of an enum holds another value than its name",
                value.name
            ));
        }
        out.text(&value.name);
        out.optional(value.description.as_deref());
        out.optional(value.deprecation_reason.as_deref());
    }
    Ok(())
}

/// Writes the default value `value`, nested `depth` levels deep.
fn value(out: &mut Out, value: &JsValue, depth: usize) -> Result<(), String> {
    if depth > DEPTH {
        return Err(String::from(
            "a default value nests deeper than the record holds",
        ));
    }
    match value {
        JsValue::Undefined => out.num(UNDEFINED),
        JsValue::Null => out.num(NULL),
        JsValue::Bool(false) => out.num(FALSE),
        JsValue::Bool(true) => out.num(TRUE),
        JsValue::Number(number) => {
            out.num(NUMBER);
            out.num(number.to_bits());
        }
        JsValue::String(text) => {
            out.num(STRING);
            out.text(text.as_str().ok_or_else(lone_surrogate)?);
        }
        JsValue::Array(items) => {
            out.num(ARRAY);
            out.count(items.len());
            for item in items {
                self::value(out, item, depth + 1)?;
            }
        }
        JsValue::Object(object) => {
            out.num(OBJECT);
            out.count(object.len());
            for (key, item) in object.iter() {
                out.text(key.as_str().ok_or_else(lone_surrogate)?);
                self::value(out, item, depth + 1)?;
            }
        }
        JsValue::Opaque(_) => {
            return Err(String::from(
                "a default value holds an object of the host, which a record holds no form of",
            ));
        }
    }
    Ok(())
}

/// Returns the message of a default value whose text holds a lone
/// surrogate, which the text of a record, being UTF-8, holds no form of.
fn lone_surrogate() -> String {
    String::from("a default value holds a lone surrogate, which a record holds no form of")
}
