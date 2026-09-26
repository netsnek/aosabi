// SPDX-License-Identifier: MIT
/*
 * aosabi-validate: the rule `FieldsOnCorrectTypeRule`.
 *
 * Derived from graphql-js 16.12.0, src/validation/rules/FieldsOnCorrectTypeRule.ts, MIT.
 */

//! The `FieldsOnCorrectTypeRule` of the validation layer.

use alloc::{
    borrow::ToOwned,
    boxed::Box,
    format,
    string::String,
    vec,
    vec::Vec, //
};
use core::cmp::Ordering;

use aosabi_json::{
    map::{
        IndexMap,
        IndexSet, //
    },
    value::KeyHasherBuilder, //
};
use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions,
    ast::FieldNode,
    visitor::{
        EnterLeave,
        VisitContext,
        VisitResult,
        Visitor, //
    }, //
};
use aosabi_type::{
    DefinitionError,
    Field,
    NamedType,
    ObjMap,
    Schema,
    Type,
    did_you_mean,
    is_abstract_type,
    is_interface_type,
    natural_compare,
    suggestion_list, //
};

use crate::validation_context::{
    Rule,
    ValidationContext, //
};

/// Fields on correct type
///
/// A GraphQL document is only valid if all fields selected are defined by the
/// parent type, or are an allowed meta field such as `__typename`.
///
/// See <https://spec.graphql.org/draft/#sec-Field-Selections>
///
/// The visitor holds the `Field` hook, which reports a field the parent type
/// does not define, with the types or the field names that lie close to it.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/FieldsOnCorrectTypeRule.ts, MIT.
pub struct FieldsOnCorrectTypeRule<'c, 'a> {
    /// The context the rule reads and reports to.
    context: &'c ValidationContext<'a>,
}

impl<'c, 'a> Rule<'c, 'a> for FieldsOnCorrectTypeRule<'c, 'a> {
    type Context = &'c ValidationContext<'a>;

    fn new(context: Self::Context) -> Self {
        Self { context }
    }
}

impl<'a> FieldsOnCorrectTypeRule<'_, 'a> {
    /// Checks the field `node`, the body of the `Field` hook at lines 36-66.
    ///
    /// # Errors
    ///
    /// Returns what a thunk of the schema returns, which the reference
    /// throws out of the hook.
    fn check_field(&self, node: &'a FieldNode) -> Result<(), DefinitionError> {
        let Some(ty) = self.context.get_parent_type() else {
            return Ok(());
        };
        if self.context.get_field_def().is_some() {
            return Ok(());
        }
        // This field doesn't exist, lets look for suggestions.
        let schema = self.context.get_schema();
        let field_name = node.name.value();

        // First determine if there are any suggested types to condition on.
        let mut suggestion = did_you_mean(
            Some("to use an inline fragment on"),
            &get_suggested_type_names(schema, &ty, field_name)?,
        );

        // If there are no suggested types, then perhaps this was a typo?
        if suggestion.is_empty() {
            suggestion = did_you_mean(None, &get_suggested_field_names(&ty, field_name)?);
        }

        // Report an error, including helpful suggestions.
        self.context.report_error(GraphQLError::new(
            format!(
                "Cannot query field \"{field_name}\" on type \"{}\".",
                ty.name()
            ) + &suggestion,
            GraphQLErrorOptions {
                nodes: Some(vec![node.clone().into()]),
                ..GraphQLErrorOptions::default()
            },
        ));
        Ok(())
    }
}

impl<'a> Visitor<'a> for FieldsOnCorrectTypeRule<'_, 'a> {
    fn enter_field(&mut self, node: &'a FieldNode, _context: &VisitContext<'a, '_>) -> VisitResult {
        match self.check_field(node) {
            Ok(()) => VisitResult::Continue,
            Err(error) => self.context.throw_error(error),
        }
    }
}

/// Go through all of the implementations of type, as well as the interfaces that
/// they implement. If any of those types include the provided field, suggest them,
/// sorted by how often the type is referenced.
///
/// The `Set` of the reference finds a type by identity and iterates in the order
/// of insertion, and the equality of a [`NamedType`] is identity.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/FieldsOnCorrectTypeRule.ts, MIT.
///
/// # Errors
///
/// Returns what a thunk of the schema returns.
fn get_suggested_type_names(
    schema: &Schema,
    ty: &NamedType,
    field_name: &str,
) -> Result<Vec<String>, DefinitionError> {
    if !is_abstract_type(&Type::from(ty)) {
        // Must be an Object type, which does not have possible fields.
        return Ok(Vec::new());
    }

    let mut suggested_types: IndexSet<&NamedType, KeyHasherBuilder> = IndexSet::default();
    let mut usage_count: IndexMap<&str, usize, KeyHasherBuilder> = IndexMap::default();
    for possible_type in schema.get_possible_types(ty)? {
        if !defines_field(possible_type, field_name)? {
            continue;
        }

        // This object type defines this field.
        suggested_types.insert(possible_type);
        usage_count.insert(possible_type.name(), 1);

        for possible_interface in get_interfaces(possible_type)? {
            if !defines_field(possible_interface, field_name)? {
                continue;
            }

            // This interface type defines this field.
            suggested_types.insert(possible_interface);
            *usage_count.entry(possible_interface.name()).or_insert(0) += 1;
        }
    }

    let mut sorted_types: Vec<&NamedType> = suggested_types.into_iter().collect();
    array_sort(&mut sorted_types, |type_a, type_b| {
        compare_suggested_types(schema, &usage_count, type_a, type_b)
    })?;
    Ok(sorted_types
        .into_iter()
        .map(|suggested_type| suggested_type.name().to_owned())
        .collect())
}

/// Orders two suggested types, the comparator of the sort at lines 110-126
/// of the reference.
///
/// The more common type comes first, then a super type before its sub type,
/// then the natural order of the names. `usage_count` holds a count for
/// every suggested type by name.
///
/// # Errors
///
/// Returns what [`Schema::is_sub_type()`] returns.
fn compare_suggested_types(
    schema: &Schema,
    usage_count: &IndexMap<&str, usize, KeyHasherBuilder>,
    type_a: &NamedType,
    type_b: &NamedType,
) -> Result<Ordering, DefinitionError> {
    // Suggest both interface and object types based on how common they are.
    let usage_count_a = usage_count.get(type_a.name()).copied().unwrap_or(0);
    let usage_count_b = usage_count.get(type_b.name()).copied().unwrap_or(0);
    let usage_count_diff = usage_count_b.cmp(&usage_count_a);
    if usage_count_diff != Ordering::Equal {
        return Ok(usage_count_diff);
    }

    // Suggest super types first followed by subtypes
    if is_interface_type(&Type::from(type_a)) && schema.is_sub_type(type_a, type_b)? {
        return Ok(Ordering::Less);
    }
    if is_interface_type(&Type::from(type_b)) && schema.is_sub_type(type_b, type_a)? {
        return Ok(Ordering::Greater);
    }

    Ok(natural_compare(type_a.name(), type_b.name()))
}

/// For the field name provided, determine if there are any similar field names
/// that may be the result of a typo.
///
/// `Object.keys` lists an index key before every other key and the other keys in
/// the order they were set. A field name starts with a letter or an underscore,
/// so no key is an index and the order is the order of insertion.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/FieldsOnCorrectTypeRule.ts, MIT.
///
/// # Errors
///
/// Returns what the field thunk of `ty` returns.
fn get_suggested_field_names(
    ty: &NamedType,
    field_name: &str,
) -> Result<Vec<String>, DefinitionError> {
    if let Some(fields) = get_fields(ty)? {
        let possible_field_names: Vec<String> = fields.keys().cloned().collect();
        return Ok(suggestion_list(field_name, &possible_field_names));
    }
    // Otherwise, must be a Union type, which does not define fields.
    Ok(Vec::new())
}

/// Returns the field map of an object or interface type, the `getFields()`
/// of the reference, and `None` for a type that defines no fields.
///
/// # Errors
///
/// Returns what the field thunk of `ty` returns.
fn get_fields(ty: &NamedType) -> Result<Option<&ObjMap<Field>>, DefinitionError> {
    match ty {
        NamedType::Object(object_type) => object_type.get_fields().map(Some),
        NamedType::Interface(interface_type) => interface_type.get_fields().map(Some),
        _ => Ok(None),
    }
}

/// Returns the interfaces of an object or interface type, the
/// `getInterfaces()` of the reference, and none for any other type.
///
/// # Errors
///
/// Returns what the interface thunk of `ty` returns.
fn get_interfaces(ty: &NamedType) -> Result<&[NamedType], DefinitionError> {
    match ty {
        NamedType::Object(object_type) => object_type.get_interfaces(),
        NamedType::Interface(interface_type) => interface_type.get_interfaces(),
        _ => Ok(&[]),
    }
}

/// Returns whether `ty` defines a field named `field_name`, the
/// `getFields()[fieldName]` test of the reference.
///
/// # Errors
///
/// Returns what the field thunk of `ty` returns.
fn defines_field(ty: &NamedType, field_name: &str) -> Result<bool, DefinitionError> {
    Ok(get_fields(ty)?.is_some_and(|fields| fields.contains_key(field_name)))
}

/// Sorts `list` in place with `compare`, the `Array.prototype.sort` of the
/// reference.
///
/// The comparator of the rule is no total order, since a super type comes before
/// its sub type whatever their names are, so the result depends on which pairs
/// the sort compares. ECMA-262 leaves that order to the implementation. The
/// reference runs on V8, whose sort finds the first run, reverses it when it is
/// strictly descending and places every further element by binary insertion, as
/// long as the array holds fewer than 64 elements. This function is those two
/// steps for every length. The sort is stable, so a consistent comparator gives
/// the one stable order at any length, and only a comparator that is not
/// consistent over 64 or more suggested types reaches the merge steps of V8 that
/// are not reproduced here. The sort of the standard library is not used,
/// because it may panic on a comparator that is no total order.
///
/// # Errors
///
/// Returns the first error `compare` returns, which leaves `list` in the
/// order the sort reached.
fn array_sort<T, E>(
    list: &mut [T],
    mut compare: impl FnMut(&T, &T) -> Result<Ordering, E>,
) -> Result<(), E> {
    let run_length = count_and_make_run(list, &mut compare)?;
    for start in run_length..list.len() {
        let mut left = 0;
        let mut right = start;
        while left < right {
            let mid = left + ((right - left) >> 1);
            if compare(&list[start], &list[mid])? == Ordering::Less {
                right = mid;
            } else {
                left = mid + 1;
            }
        }
        list[left..=start].rotate_right(1);
    }
    Ok(())
}

/// Returns the length of the run that starts `list` and makes it ascending,
/// the `CountAndMakeRun` step of [`array_sort()`].
///
/// A run is ascending, where no element is less than the one before it, or
/// strictly descending, which keeps the reversal stable.
///
/// # Errors
///
/// Returns the first error `compare` returns.
fn count_and_make_run<T, E>(
    list: &mut [T],
    compare: &mut impl FnMut(&T, &T) -> Result<Ordering, E>,
) -> Result<usize, E> {
    if list.len() < 2 {
        return Ok(list.len());
    }
    let is_descending = compare(&list[1], &list[0])? == Ordering::Less;
    let mut run_length = 2;
    while run_length < list.len() {
        let order = compare(&list[run_length], &list[run_length - 1])?;
        if is_descending != (order == Ordering::Less) {
            break;
        }
        run_length += 1;
    }
    if is_descending {
        list[..run_length].reverse();
    }
    Ok(run_length)
}

/// Creates the visitor of `FieldsOnCorrectTypeRule` over `context`.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/FieldsOnCorrectTypeRule.ts, MIT.
#[must_use]
pub fn fields_on_correct_type_rule<'c, 'a>(
    context: &'c ValidationContext<'a>,
) -> Box<dyn EnterLeave<'a> + 'c> {
    FieldsOnCorrectTypeRule::boxed(context)
}
