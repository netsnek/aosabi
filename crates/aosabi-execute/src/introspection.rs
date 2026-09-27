// SPDX-License-Identifier: MIT
/*
 * aosabi-execute introspection: the dispatch of an introspection resolver of the type layer.
 *
 * No analogue in the reference, where an introspection field carries a
 * JavaScript function of its own.
 */

//! The introspection dispatch of the executor.
//!
//! A field of an introspection type stores one variant of
//! [`IntrospectionResolver`] in its `resolve` slot, and the type layer holds
//! one function per variant, which reads a definition and answers a
//! definition or a plain value. [`resolve_introspection_field`] is the call
//! of that function with the source value the executor holds. A schema, a
//! type and a directive travel as a [`HostValue`] that shares the definition,
//! and a field, an argument, an input field and an enum value as a
//! [`Definition`], its position in the schema of the execution, so the
//! resolvers hand out the schema's own definitions and copy none of them. A
//! member the reference answers `undefined` for is [`Value::Undefined`],
//! which completes to null.
//!
//! A resolver reads the schema of the execution, the object type its field
//! is selected on and the coerced arguments of a field whose definition holds
//! any, where they stand, the parts of the reference's `info` and `args` the
//! resolvers of `introspection.ts` read, so the executor builds no info, no
//! path and no object of the arguments for an introspection field. No code
//! outside the executor sees an introspection resolver, so no reader of an
//! info or of an arguments object can tell.
//!
//! The value of `__typename` on an object of a node of the plan is the name
//! of the type of the node, which the plan holds as its JSON literal, so the
//! executor appends that literal with no call of `TypeNameMetaField` where
//! the specified `String` answers the name as itself (see
//! [`PlanNode::typename`](crate::plan::PlanNode::typename)). The resolver
//! runs for a root field of a mutation, whose fields run one after the
//! other, and where a serializer of the request binds `String` otherwise.

use aosabi_json::{
    JsObject,
    JsValue, //
};
use aosabi_type::{
    DefinitionError,
    Directive,
    EnumValue,
    Field,
    InputField,
    InputValue,
    IntrospectionResolver,
    NamedType,
    ObjMap,
    ObjectType,
    Schema,
    SharedType,
    Type,
    introspection, //
};

use crate::{
    errors::thrown_definition_error,
    execute::to_boolean,
    value::{
        Completion,
        HostValue,
        Value, //
    }, //
};

/// A definition of the schema an introspection field resolves to, by its
/// position in the schema of the execution.
///
/// A position names a type in the type map of the schema, a directive among
/// the directives of the schema, and a field, an input field, a value or an
/// argument among those of its owner, each in the order the schema holds
/// them, which no execution changes. So the definition is read where it
/// stands, and the value holds no count of a reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Definition {
    /// The field at the second position of the object or interface type at
    /// the first.
    Field(u32, u32),
    /// The argument at the third position of the field at the second
    /// position of the object or interface type at the first.
    FieldArgument(u32, u32, u32),
    /// The argument at the second position of the directive at the first.
    DirectiveArgument(u32, u32),
    /// The input field at the second position of the input object type at
    /// the first.
    InputField(u32, u32),
    /// The value at the second position of the enum type at the first.
    EnumValue(u32, u32),
}

/// Calls the resolver `resolver` names on `source`, as the reference calls its function.
///
/// `source` is the host value the parent field resolved to, `args` the
/// coerced arguments of a field whose definition holds any, read where they
/// stand, `schema` the schema of the execution and `parent_type` the object
/// type the field is selected on. A source of a kind the resolver does not
/// read answers `undefined`. A throw of a thunk of the schema is the throw of
/// the resolver.
// Out of line, so only an execution that resolves a field of the introspection compiles it.
#[inline(never)]
pub fn resolve_introspection_field(
    resolver: IntrospectionResolver,
    source: &Value,
    args: Option<&JsObject>,
    schema: &SharedType<Schema>,
    parent_type: &ObjectType,
) -> Completion {
    let result = match resolver {
        IntrospectionResolver::SchemaMetaField => {
            Ok(Value::Host(HostValue::Schema(SharedType::clone(schema))))
        }
        IntrospectionResolver::TypeMetaField => Ok(type_meta_field(args, schema)),
        IntrospectionResolver::TypeNameMetaField => Ok(Value::from(
            introspection::type_name_meta_field(parent_type),
        )),
        _ => resolve_on_source(resolver, source, args, schema),
    };
    result.map_err(thrown_definition_error).into()
}

/// Returns the type the `name` argument of `__type` names, and `undefined` when there is none.
fn type_meta_field(args: Option<&JsObject>, schema: &Schema) -> Value {
    let Some(JsValue::String(name)) = args.and_then(|args| args.get("name")) else {
        return Value::Undefined;
    };
    let Some(name) = name.as_str() else {
        return Value::Undefined;
    };
    introspection::type_meta_field(schema, name).map_or(Value::Undefined, named_type_value)
}

/// Calls the resolver of a field of an introspection type on the definition `source` holds.
fn resolve_on_source(
    resolver: IntrospectionResolver,
    source: &Value,
    args: Option<&JsObject>,
    schema: &Schema,
) -> Result<Value, DefinitionError> {
    let Value::Host(source) = source else {
        return Ok(Value::Undefined);
    };
    match source {
        HostValue::Schema(source) => Ok(schema_member(resolver, source)),
        HostValue::Directive(directive) => Ok(directive_member(resolver, directive, args, schema)),
        HostValue::Type(ty) => type_member(resolver, ty, args, schema),
        HostValue::Definition(definition) => definition_member(resolver, *definition, args, schema),
        HostValue::Context(_) | HostValue::Info(_) | HostValue::Opaque(_) | HostValue::Node(_) => {
            Ok(Value::Undefined)
        }
    }
}

/// Returns whether the `includeDeprecated` argument is truthy.
///
/// A field whose definition holds no argument has no `includeDeprecated`,
/// which reads `undefined`, so it is not truthy.
fn include_deprecated(args: Option<&JsObject>) -> bool {
    args.and_then(|args| args.get("includeDeprecated"))
        .is_some_and(|value| to_boolean(&Value::from_js(value)))
}

/// Returns a text member, which is `undefined` when the definition has none.
fn optional_text(text: Option<&str>) -> Value {
    text.map_or(Value::Undefined, Value::from)
}

/// Returns a type as the host value an introspection field resolves to.
fn type_value(ty: &Type) -> Value {
    Value::Host(HostValue::Type(ty.clone()))
}

/// Returns a named type as the host value an introspection field resolves to.
fn named_type_value(ty: &NamedType) -> Value {
    Value::Host(HostValue::Type(Type::from(ty)))
}

/// Returns the definitions `listed` as host values, each made by `value` of
/// its position among `all`.
///
/// A resolver of the type layer lists the definitions of an owner in the
/// order they stand there, every one or the ones that are not deprecated, so
/// one walk of `all` finds the position of each by its address.
fn listed_values<'a, T: 'a>(
    all: impl IntoIterator<Item = &'a T>,
    listed: &[&'a T],
    value: impl Fn(u32) -> Definition,
) -> Vec<Value> {
    let mut all = all.into_iter().enumerate();
    let mut values = Vec::with_capacity(listed.len());
    for wanted in listed {
        let at = all
            .find(|(_, definition)| core::ptr::eq(*definition, *wanted))
            .and_then(|(at, _)| u32::try_from(at).ok());
        if let Some(at) = at {
            values.push(Value::Host(HostValue::Definition(value(at))));
        }
    }
    values
}

/// Returns the position of the named type `ty` in the type map of `schema`.
///
/// The type of that name in the map must be `ty` itself, which every type a
/// valid schema reaches is, since the schema collects each type it reaches
/// once by its name. A type of no such position answers none, and the list
/// member of it `undefined`.
fn position_of(schema: &Schema, ty: &Type) -> Option<u32> {
    let types = schema.get_type_map();
    let at = types.get_index_of(introspection::type_name(ty)?)?;
    let (_, named) = types.get_index(at)?;
    let same = match (ty, named) {
        (Type::Object(ty), NamedType::Object(named)) => SharedType::ptr_eq(ty, named),
        (Type::Interface(ty), NamedType::Interface(named)) => SharedType::ptr_eq(ty, named),
        (Type::Enum(ty), NamedType::Enum(named)) => SharedType::ptr_eq(ty, named),
        (Type::InputObject(ty), NamedType::InputObject(named)) => SharedType::ptr_eq(ty, named),
        _ => false,
    };
    if !same {
        return None;
    }
    u32::try_from(at).ok()
}

/// Returns the named type at the position `ty` of the type map of `schema`.
fn type_at(schema: &Schema, ty: u32) -> Option<&NamedType> {
    usize::try_from(ty)
        .ok()
        .and_then(|ty| schema.get_type_map().get_index(ty))
        .map(|(_, named)| named)
}

/// Returns the fields of the object or interface type `named`, none for any other type.
fn fields_of(named: &NamedType) -> Result<Option<&ObjMap<Field>>, DefinitionError> {
    match named {
        NamedType::Object(object) => object.get_fields().map(Some),
        NamedType::Interface(interface) => interface.get_fields().map(Some),
        _ => Ok(None),
    }
}

/// Returns the item at the position `at` of `items`.
fn item<T>(items: &[T], at: u32) -> Option<&T> {
    usize::try_from(at).ok().and_then(|at| items.get(at))
}

/// Returns the value at the position `at` of `map`.
fn entry<V>(map: &ObjMap<V>, at: u32) -> Option<&V> {
    usize::try_from(at)
        .ok()
        .and_then(|at| map.get_index(at))
        .map(|(_, value)| value)
}

/// Returns the field at the position `field` of the object or interface type at `ty`.
fn field_at(schema: &Schema, ty: u32, field: u32) -> Result<Option<&Field>, DefinitionError> {
    let Some(named) = type_at(schema, ty) else {
        return Ok(None);
    };
    Ok(fields_of(named)?.and_then(|fields| entry(fields, field)))
}

/// Returns the input field at the position `field` of the input object type at `ty`.
fn input_field_at(
    schema: &Schema,
    ty: u32,
    field: u32,
) -> Result<Option<&InputField>, DefinitionError> {
    let Some(NamedType::InputObject(input_object)) = type_at(schema, ty) else {
        return Ok(None);
    };
    Ok(entry(input_object.get_fields()?, field))
}

/// Returns the value at the position `value` of the enum type at `ty`.
fn enum_value_at(
    schema: &Schema,
    ty: u32,
    value: u32,
) -> Result<Option<&EnumValue>, DefinitionError> {
    let Some(NamedType::Enum(enum_type)) = type_at(schema, ty) else {
        return Ok(None);
    };
    Ok(item(enum_type.get_values()?, value))
}

/// Returns the member of `__Schema` that `resolver` names.
fn schema_member(resolver: IntrospectionResolver, schema: &Schema) -> Value {
    match resolver {
        IntrospectionResolver::SchemaDescription => {
            optional_text(introspection::schema_description(schema))
        }
        IntrospectionResolver::SchemaTypes => {
            let types: Vec<Value> = introspection::schema_types(schema)
                .into_iter()
                .map(named_type_value)
                .collect();
            Value::from(types)
        }
        IntrospectionResolver::SchemaQueryType => {
            introspection::schema_query_type(schema).map_or(Value::Undefined, named_type_value)
        }
        IntrospectionResolver::SchemaMutationType => {
            introspection::schema_mutation_type(schema).map_or(Value::Undefined, named_type_value)
        }
        IntrospectionResolver::SchemaSubscriptionType => {
            introspection::schema_subscription_type(schema)
                .map_or(Value::Undefined, named_type_value)
        }
        IntrospectionResolver::SchemaDirectives => {
            let directives: Vec<Value> = introspection::schema_directives(schema)
                .iter()
                .map(|directive| Value::Host(HostValue::Directive(SharedType::clone(directive))))
                .collect();
            Value::from(directives)
        }
        _ => Value::Undefined,
    }
}

/// Returns the member of `__Directive` that `resolver` names.
///
/// The arguments of the directive are listed by its position among the
/// directives of `schema`, and a directive of no such position lists none.
fn directive_member(
    resolver: IntrospectionResolver,
    directive: &SharedType<Directive>,
    args: Option<&JsObject>,
    schema: &Schema,
) -> Value {
    match resolver {
        IntrospectionResolver::DirectiveName => {
            Value::from(introspection::directive_name(directive))
        }
        IntrospectionResolver::DirectiveDescription => {
            optional_text(introspection::directive_description(directive))
        }
        IntrospectionResolver::DirectiveIsRepeatable => {
            Value::Bool(introspection::directive_is_repeatable(directive))
        }
        IntrospectionResolver::DirectiveLocations => {
            let locations: Vec<Value> = introspection::directive_locations(directive)
                .iter()
                .map(|location| Value::from(location.as_str()))
                .collect();
            Value::from(locations)
        }
        IntrospectionResolver::DirectiveArgs => {
            let at = schema
                .get_directives()
                .iter()
                .position(|known| SharedType::ptr_eq(known, directive))
                .and_then(|at| u32::try_from(at).ok());
            let Some(at) = at else {
                return Value::Undefined;
            };
            let listed = introspection::directive_args(directive, include_deprecated(args));
            Value::from(listed_values(&directive.args, &listed, |arg| {
                Definition::DirectiveArgument(at, arg)
            }))
        }
        _ => Value::Undefined,
    }
}

/// Returns the member of `__Type` that `resolver` names.
fn type_member(
    resolver: IntrospectionResolver,
    ty: &Type,
    args: Option<&JsObject>,
    schema: &Schema,
) -> Result<Value, DefinitionError> {
    let member = match resolver {
        IntrospectionResolver::TypeKind => Value::from(introspection::type_kind(ty).as_str()),
        IntrospectionResolver::TypeName => optional_text(introspection::type_name(ty)),
        IntrospectionResolver::TypeDescription => {
            optional_text(introspection::type_description(ty))
        }
        IntrospectionResolver::TypeSpecifiedByUrl => {
            optional_text(introspection::type_specified_by_url(ty))
        }
        IntrospectionResolver::TypeOfType => {
            introspection::type_of_type(ty).map_or(Value::Undefined, type_value)
        }
        IntrospectionResolver::TypeIsOneOf => {
            introspection::type_is_one_of(ty).map_or(Value::Undefined, Value::Bool)
        }
        _ => return type_list_member(resolver, ty, args, schema),
    };
    Ok(member)
}

/// Returns the member of `__Type` that `resolver` names, for a member that is a list.
///
/// A field, an input field and a value of the type are listed by the
/// position of the type in the type map of `schema` (see [`position_of`]).
fn type_list_member(
    resolver: IntrospectionResolver,
    ty: &Type,
    args: Option<&JsObject>,
    schema: &Schema,
) -> Result<Value, DefinitionError> {
    let members: Option<Vec<Value>> = match resolver {
        IntrospectionResolver::TypeFields => {
            match introspection::type_fields(ty, include_deprecated(args))? {
                Some(listed) => owned_list(schema, ty, |at| {
                    let all = type_at(schema, at).map(fields_of).transpose()?.flatten();
                    Ok(all.map(|all| {
                        listed_values(all.values(), &listed, |field| Definition::Field(at, field))
                    }))
                })?,
                None => None,
            }
        }
        IntrospectionResolver::TypeInterfaces => introspection::type_interfaces(ty)?
            .map(|interfaces| interfaces.iter().map(named_type_value).collect()),
        IntrospectionResolver::TypePossibleTypes => introspection::type_possible_types(ty, schema)?
            .map(|possible_types| possible_types.iter().map(named_type_value).collect()),
        IntrospectionResolver::TypeEnumValues => {
            match introspection::type_enum_values(ty, include_deprecated(args))? {
                Some(listed) => owned_list(schema, ty, |at| {
                    let Some(NamedType::Enum(enum_type)) = type_at(schema, at) else {
                        return Ok(None);
                    };
                    Ok(Some(listed_values(
                        enum_type.get_values()?,
                        &listed,
                        |value| Definition::EnumValue(at, value),
                    )))
                })?,
                None => None,
            }
        }
        IntrospectionResolver::TypeInputFields => {
            match introspection::type_input_fields(ty, include_deprecated(args))? {
                Some(listed) => owned_list(schema, ty, |at| {
                    let Some(NamedType::InputObject(input_object)) = type_at(schema, at) else {
                        return Ok(None);
                    };
                    Ok(Some(listed_values(
                        input_object.get_fields()?.values(),
                        &listed,
                        |field| Definition::InputField(at, field),
                    )))
                })?,
                None => None,
            }
        }
        _ => None,
    };
    Ok(members.map_or(Value::Undefined, Value::from))
}

/// Returns the list `list` makes of the position of `ty` in the type map of
/// `schema`, none where the type has no such position.
fn owned_list(
    schema: &Schema,
    ty: &Type,
    list: impl FnOnce(u32) -> Result<Option<Vec<Value>>, DefinitionError>,
) -> Result<Option<Vec<Value>>, DefinitionError> {
    position_of(schema, ty).map_or(Ok(None), list)
}

/// Returns the member that `resolver` names of the definition `definition`.
///
/// A position that names no definition of `schema` answers `undefined`,
/// which no definition an introspection resolver of the execution handed out
/// meets.
fn definition_member(
    resolver: IntrospectionResolver,
    definition: Definition,
    args: Option<&JsObject>,
    schema: &Schema,
) -> Result<Value, DefinitionError> {
    match definition {
        Definition::Field(ty, at) => {
            Ok(field_at(schema, ty, at)?.map_or(Value::Undefined, |field| {
                field_member(resolver, field, args, |arg| {
                    Definition::FieldArgument(ty, at, arg)
                })
            }))
        }
        Definition::FieldArgument(ty, field, at) => {
            let argument = field_at(schema, ty, field)?.and_then(|field| item(&field.args, at));
            argument.map_or(Ok(Value::Undefined), |argument| {
                input_value_member(resolver, InputValue::Argument(argument))
            })
        }
        Definition::DirectiveArgument(directive, at) => {
            let argument = item(schema.get_directives(), directive)
                .and_then(|directive| item(&directive.args, at));
            argument.map_or(Ok(Value::Undefined), |argument| {
                input_value_member(resolver, InputValue::Argument(argument))
            })
        }
        Definition::InputField(ty, at) => input_field_at(schema, ty, at)?
            .map_or(Ok(Value::Undefined), |input_field| {
                input_value_member(resolver, InputValue::InputField(input_field))
            }),
        Definition::EnumValue(ty, at) => Ok(enum_value_at(schema, ty, at)?
            .map_or(Value::Undefined, |enum_value| {
                enum_value_member(resolver, enum_value)
            })),
    }
}

/// Returns the member of `__Field` that `resolver` names.
///
/// `argument` makes the definition of the argument at a position of the
/// field.
fn field_member(
    resolver: IntrospectionResolver,
    field: &Field,
    args: Option<&JsObject>,
    argument: impl Fn(u32) -> Definition,
) -> Value {
    match resolver {
        IntrospectionResolver::FieldName => Value::from(introspection::field_name(field)),
        IntrospectionResolver::FieldDescription => {
            optional_text(introspection::field_description(field))
        }
        IntrospectionResolver::FieldArgs => {
            let listed = introspection::field_args(field, include_deprecated(args));
            Value::from(listed_values(&field.args, &listed, argument))
        }
        IntrospectionResolver::FieldType => type_value(introspection::field_type(field)),
        IntrospectionResolver::FieldIsDeprecated => {
            Value::Bool(introspection::field_is_deprecated(field))
        }
        IntrospectionResolver::FieldDeprecationReason => {
            optional_text(introspection::field_deprecation_reason(field))
        }
        _ => Value::Undefined,
    }
}

/// Returns the member of `__InputValue` that `resolver` names.
fn input_value_member(
    resolver: IntrospectionResolver,
    input_value: InputValue<'_>,
) -> Result<Value, DefinitionError> {
    let member = match resolver {
        IntrospectionResolver::InputValueName => {
            Value::from(introspection::input_value_name(input_value))
        }
        IntrospectionResolver::InputValueDescription => {
            optional_text(introspection::input_value_description(input_value))
        }
        IntrospectionResolver::InputValueType => {
            type_value(introspection::input_value_type(input_value))
        }
        IntrospectionResolver::InputValueDefaultValue => {
            introspection::input_value_default_value(input_value)?
                .map_or(Value::Undefined, Value::from)
        }
        IntrospectionResolver::InputValueIsDeprecated => {
            Value::Bool(introspection::input_value_is_deprecated(input_value))
        }
        IntrospectionResolver::InputValueDeprecationReason => {
            optional_text(introspection::input_value_deprecation_reason(input_value))
        }
        _ => Value::Undefined,
    };
    Ok(member)
}

/// Returns the member of `__EnumValue` that `resolver` names.
fn enum_value_member(resolver: IntrospectionResolver, enum_value: &EnumValue) -> Value {
    match resolver {
        IntrospectionResolver::EnumValueName => {
            Value::from(introspection::enum_value_name(enum_value))
        }
        IntrospectionResolver::EnumValueDescription => {
            optional_text(introspection::enum_value_description(enum_value))
        }
        IntrospectionResolver::EnumValueIsDeprecated => {
            Value::Bool(introspection::enum_value_is_deprecated(enum_value))
        }
        IntrospectionResolver::EnumValueDeprecationReason => {
            optional_text(introspection::enum_value_deprecation_reason(enum_value))
        }
        _ => Value::Undefined,
    }
}
