// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-infer: the predicates of the inference, the questions about one type
 * that the walk and the type mapping share.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/cli/builder/schema/types-helper.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 */

//! The predicates of the inference.
//!
//! One function stands for each function of `types-helper.ts`, in the order
//! of that file, generic over the [`TypeSource`] with the source as its first
//! argument where the reference takes the checker or reads the type object.
//!
//! # JavaScript semantics the port reproduces
//!
//! - `flags & Flag` is truthy when the bit is set, which is
//!   [`TypeFlags::contains`] of a one bit constant.
//! - `a && b && c` yields the first falsy operand, so `isPublic` of
//!   `types-helper.ts:195-201` is `undefined` for a symbol with no value
//!   declaration, and the strict comparison `isPublic === false` of `:203`
//!   keeps such a symbol.
//! - `String.prototype.replace` with a regular expression that has no `u`
//!   flag matches UTF-16 code units, so [`safe_type_name`] turns a character
//!   outside the basic plane into two underscores.
//!
//! [to-boolean]: https://tc39.es/ecma262/#sec-toboolean
//! [logical]: https://tc39.es/ecma262/#sec-binary-logical-operators
//! [strict]: https://tc39.es/ecma262/#sec-isstrictlyequal
//! [regexp]: https://tc39.es/ecma262/#sec-get-regexp.prototype.unicode

use crate::type_source::{
    ModifierFlags,
    ObjectFlags,
    TypeFlags,
    TypeSource, //
};

/// Returns whether `type` is an object type with no property and no call
/// signature, `types-helper.ts:3-10`.
pub fn is_empty_object<S: TypeSource>(source: &S, r#type: S::Type) -> bool {
    let flags = source.get_flags(r#type);

    (flags.contains(TypeFlags::OBJECT) || flags.contains(TypeFlags::NON_PRIMITIVE))
        && source.get_properties(r#type).is_empty()
        && source.get_call_signatures(r#type).is_empty()
}

/// Returns whether one of the base types of `type` carries the symbol name
/// `Array`, the `extendsArray` arrow of `types-helper.ts:30-31`.
fn extends_array<S: TypeSource>(source: &S, r#type: S::Type) -> bool {
    source.get_base_types(r#type).is_some_and(|bases| {
        bases.into_iter().any(|base| {
            source
                .get_symbol(base)
                .is_some_and(|symbol| source.get_name(symbol) == "Array")
        })
    })
}

/// Returns whether `type` is an array, a tuple or a type that extends the
/// array type, `types-helper.ts:12-43`.
///
/// A generic instantiation answers no base types of its own, so the
/// reference looks through its target as well.
pub fn is_list<S: TypeSource>(source: &S, r#type: S::Type) -> bool {
    let is = source.is_array_or_tuple_type_node(r#type);

    if !is {
        let target = source.get_target(r#type);
        let is_array = source
            .get_symbol(r#type)
            .is_some_and(|symbol| source.get_name(symbol) == "Array")
            || extends_array(source, r#type)
            || target.is_some_and(|target| target != r#type && extends_array(source, target));

        if is_array {
            return true;
        }
    }

    is
}

/// Returns whether `type` is a union whose members are all boolean literals,
/// `types-helper.ts:45-56`.
pub fn is_boolean<S: TypeSource>(source: &S, r#type: S::Type) -> bool {
    if source.is_union(r#type) {
        return source
            .get_types(r#type)
            .into_iter()
            .all(|t| source.get_flags(t).contains(TypeFlags::BOOLEAN_LITERAL));
    }

    false
}

/// Returns whether `type` is a string, a number, a boolean or a literal of
/// one of them, `types-helper.ts:58-68`.
pub fn is_primitive<S: TypeSource>(source: &S, r#type: S::Type) -> bool {
    let flags = source.get_flags(r#type);

    flags.contains(TypeFlags::STRING)
        || flags.contains(TypeFlags::NUMBER)
        || flags.contains(TypeFlags::BOOLEAN)
        || flags.contains(TypeFlags::STRING_LITERAL)
        || flags.contains(TypeFlags::NUMBER_LITERAL)
        || flags.contains(TypeFlags::BOOLEAN_LITERAL)
}

/// Returns whether `type` is a string, a number or a boolean literal,
/// `types-helper.ts:70-76`.
pub fn is_literal_type<S: TypeSource>(source: &S, r#type: S::Type) -> bool {
    let flags = source.get_flags(r#type);

    flags.contains(TypeFlags::STRING_LITERAL)
        || flags.contains(TypeFlags::NUMBER_LITERAL)
        || flags.contains(TypeFlags::BOOLEAN_LITERAL)
}

/// Returns whether `type` is the never type, `types-helper.ts:78-80`.
pub fn is_never<S: TypeSource>(source: &S, r#type: S::Type) -> bool {
    source.get_flags(r#type).contains(TypeFlags::NEVER)
}

/// Returns whether `type` has a call signature, `types-helper.ts:82-84`.
pub fn is_function<S: TypeSource>(source: &S, r#type: S::Type) -> bool {
    !source.get_call_signatures(r#type).is_empty()
}

/// Returns whether `flags` carry `undefined` or `null`, the filter arrow of
/// `types-helper.ts:92` and `:133`.
fn is_nullish(flags: TypeFlags) -> bool {
    flags.contains(TypeFlags::UNDEFINED) || flags.contains(TypeFlags::NULL)
}

/// Returns whether `type` is a union whose members, `null` and `undefined`
/// aside, are all literals, `types-helper.ts:86-106`.
///
/// This is the shape the type mapping turns into an enum.
pub fn is_primitive_union<S: TypeSource>(source: &S, r#type: S::Type) -> bool {
    if is_primitive(source, r#type) {
        return false;
    }

    if !source.is_union(r#type) {
        return false;
    }

    let non_nullable_types: Vec<S::Type> = source
        .get_types(r#type)
        .into_iter()
        .filter(|t| !is_nullish(source.get_flags(*t)))
        .collect();

    if non_nullable_types.is_empty() {
        return false;
    }

    non_nullable_types
        .into_iter()
        .all(|t| is_literal_type(source, t))
}

/// Returns whether the symbol of `type` is named `Promise`,
/// `types-helper.ts:108-111`.
pub fn is_promise<S: TypeSource>(source: &S, r#type: S::Type) -> bool {
    source
        .get_symbol(r#type)
        .is_some_and(|symbol| source.get_name(symbol) == "Promise")
}

/// Returns the first type argument of a generic reference, which is the
/// awaited type of a promise, `types-helper.ts:113-123`.
pub fn get_promise_type<S: TypeSource>(source: &S, r#type: S::Type) -> Option<S::Type> {
    if source
        .get_object_flags(r#type)
        .contains(ObjectFlags::REFERENCE)
    {
        return source.get_type_arguments(r#type).into_iter().next();
    }

    None
}

/// Strips `null` and `undefined` from a union and reports whether one of
/// them was there, `types-helper.ts:125-142`.
///
/// The returned pair is `type` and `wasOptional` of the reference.
pub fn exclude_null_undefined_from_type<S: TypeSource>(
    source: &S,
    r#type: S::Type,
) -> (S::Type, bool) {
    if !source.is_union(r#type) {
        return (r#type, false);
    }

    let all = source.get_types(r#type);
    let types = all
        .iter()
        .filter(|t| !is_nullish(source.get_flags(**t)))
        .count();

    let was_optional = types != all.len();

    (source.get_non_nullable_type(r#type), was_optional)
}

/// Returns whether `type` is a generic reference with a type argument, the
/// filter arrow of `types-helper.ts:152-158` and `:161-165`.
fn is_reference_with_arguments<S: TypeSource>(source: &S, r#type: S::Type) -> bool {
    source.get_flags(r#type).contains(TypeFlags::OBJECT)
        && source
            .get_object_flags(r#type)
            .contains(ObjectFlags::REFERENCE)
        && !source.get_type_arguments(r#type).is_empty()
}

/// Rebuilds a union without the array like members whose element type is a
/// member too, `types-helper.ts:150-181`.
fn union_without_array_twins<S: TypeSource>(source: &S, r#type: S::Type) -> S::Type {
    let types = source.get_types(r#type);
    let non_array_types = types
        .iter()
        .copied()
        .filter(|t| !is_reference_with_arguments(source, *t));
    let array_element_types: Vec<S::Type> = types
        .iter()
        .copied()
        .filter(|t| is_reference_with_arguments(source, *t))
        .filter_map(|t| source.get_type_arguments(t).into_iter().next())
        .collect();
    let filtered_types: Vec<S::Type> = non_array_types
        .filter(|t| !array_element_types.contains(t))
        .collect();

    source.get_union_type(&filtered_types)
}

/// Returns the members of `type` that become fields, `types-helper.ts:144-211`.
///
/// A member is dropped when it has a value declaration and that declaration
/// is private or its name starts with `#`. A member with no value declaration
/// is kept, and `protected` is not filtered.
pub fn get_public_properties_of_type<S: TypeSource>(source: &S, r#type: S::Type) -> Vec<S::Symbol> {
    let mut r#type = r#type;

    if source.is_union(r#type) {
        r#type = union_without_array_twins(source, r#type);
    }

    let mut properties = Vec::new();

    if is_list(source, r#type) || is_primitive(source, r#type) || is_primitive_union(source, r#type)
    {
        return properties;
    }

    for prop in source.get_properties(r#type) {
        let is_public = source.has_value_declaration(prop).then(|| {
            !source
                .get_combined_modifier_flags(prop)
                .contains(ModifierFlags::PRIVATE)
                && !source.get_name(prop).starts_with('#')
        });

        if is_public == Some(false) {
            continue;
        }

        properties.push(prop);
    }

    properties
}

/// Turns `name` into a GraphQL type name, `types-helper.ts:213-227`.
///
/// Every UTF-16 code unit outside `[0-9a-zA-Z_]` becomes `_`, every leading
/// underscore is dropped, and an empty result is `JSONObject`.
#[must_use]
pub fn safe_type_name(name: &str) -> String {
    let replaced: String = name
        .encode_utf16()
        .map(|unit| match char::from_u32(u32::from(unit)) {
            Some(c) if c.is_ascii_alphanumeric() || c == '_' => c,
            _ => '_',
        })
        .collect();

    let safe_name = replaced.trim_start_matches('_');

    if safe_name.is_empty() {
        return String::from("JSONObject");
    }

    String::from(safe_name)
}

/// Returns whether `type` is declared in the repeater package, which is how
/// the reference knows the return type of a subscription,
/// `types-helper.ts:229-238`.
pub fn is_subscription_repeater<S: TypeSource>(source: &S, r#type: S::Type) -> bool {
    source
        .get_symbol(r#type)
        .is_some_and(|symbol| source.is_declared_in_file(symbol, "@repeaterjs"))
}
