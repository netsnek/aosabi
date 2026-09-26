// SPDX-License-Identifier: MIT
/*
 * aosabi-validate: the rule lists of the specification, which are the rules
 * of an executable document and the rules of a schema document, each in the
 * order the reference runs them.
 *
 * Derived from graphql-js 16.12.0, src/validation/specifiedRules.ts, MIT.
 */

//! The specified rules of the validation layer.
//!
//! [`SPECIFIED_RULES`] is the list [`crate::validate()`] runs by default and
//! [`SPECIFIED_SDL_RULES`] the list [`crate::validate_sdl()`] runs by
//! default. The order of a list is the order the parallel visitor calls the
//! rules at every node, so it is the order of the errors of one node, and it
//! is copied from the reference entry by entry.
//!
//! An entry carries the name of the rule function beside the function,
//! because the harness runs one rule alone by that name.
//! [`specified_rule_named()`] and [`specified_sdl_rule_named()`] are that
//! lookup.
//!
//! A rule the reference types over the base context, or over the union of
//! the two derived contexts, enters a list through a closure that passes the
//! context of the list on, which is the subtyping the reference relies on
//! when it places such a function in a list of a narrower rule type.

use crate::{
    rules::{
        executable_definitions_rule::executable_definitions_rule,
        fields_on_correct_type_rule::fields_on_correct_type_rule,
        fragments_on_composite_types_rule::fragments_on_composite_types_rule,
        known_argument_names_rule::{
            known_argument_names_on_directives_rule,
            known_argument_names_rule, //
        },
        known_directives_rule::known_directives_rule,
        known_fragment_names_rule::known_fragment_names_rule,
        known_type_names_rule::known_type_names_rule,
        lone_anonymous_operation_rule::lone_anonymous_operation_rule,
        lone_schema_definition_rule::lone_schema_definition_rule,
        max_introspection_depth_rule::max_introspection_depth_rule,
        no_fragment_cycles_rule::no_fragment_cycles_rule,
        no_undefined_variables_rule::no_undefined_variables_rule,
        no_unused_fragments_rule::no_unused_fragments_rule,
        no_unused_variables_rule::no_unused_variables_rule,
        overlapping_fields_can_be_merged_rule::overlapping_fields_can_be_merged_rule,
        possible_fragment_spreads_rule::possible_fragment_spreads_rule,
        possible_type_extensions_rule::possible_type_extensions_rule,
        provided_required_arguments_rule::{
            provided_required_arguments_on_directives_rule,
            provided_required_arguments_rule, //
        },
        scalar_leafs_rule::scalar_leafs_rule,
        single_field_subscriptions_rule::single_field_subscriptions_rule,
        unique_argument_definition_names_rule::unique_argument_definition_names_rule,
        unique_argument_names_rule::unique_argument_names_rule,
        unique_directive_names_rule::unique_directive_names_rule,
        unique_directives_per_location_rule::unique_directives_per_location_rule,
        unique_enum_value_names_rule::unique_enum_value_names_rule,
        unique_field_definition_names_rule::unique_field_definition_names_rule,
        unique_fragment_names_rule::unique_fragment_names_rule,
        unique_input_field_names_rule::unique_input_field_names_rule,
        unique_operation_names_rule::unique_operation_names_rule,
        unique_operation_types_rule::unique_operation_types_rule,
        unique_type_names_rule::unique_type_names_rule,
        unique_variable_names_rule::unique_variable_names_rule,
        values_of_correct_type_rule::values_of_correct_type_rule,
        variables_are_input_types_rule::variables_are_input_types_rule,
        variables_in_allowed_position_rule::variables_in_allowed_position_rule, //
    },
    validation_context::{
        SdlValidationRule,
        ValidationRule, //
    }, //
};

/// The `MaxIntrospectionDepthRule` entry, which both lists below hold.
const MAX_INTROSPECTION_DEPTH_RULE: ValidationRule = ValidationRule {
    name: "MaxIntrospectionDepthRule",
    create: |context| max_introspection_depth_rule(context),
};

/// Technically these aren't part of the spec but they are strongly
/// encouraged validation rules.
///
/// Derived from graphql-js 16.12.0, src/validation/specifiedRules.ts, MIT.
pub const RECOMMENDED_RULES: &[ValidationRule] = &[MAX_INTROSPECTION_DEPTH_RULE];

/// This set includes all validation rules defined by the GraphQL spec.
///
/// The order of the rules in this list has been adjusted to lead to the most
/// clear output when encountering multiple validation errors. The last entry
/// is the spread of [`RECOMMENDED_RULES`].
///
/// Derived from graphql-js 16.12.0, src/validation/specifiedRules.ts, MIT.
pub const SPECIFIED_RULES: &[ValidationRule] = &[
    ValidationRule {
        name: "ExecutableDefinitionsRule",
        create: |context| executable_definitions_rule(context),
    },
    ValidationRule {
        name: "UniqueOperationNamesRule",
        create: |context| unique_operation_names_rule(context),
    },
    ValidationRule {
        name: "LoneAnonymousOperationRule",
        create: |context| lone_anonymous_operation_rule(context),
    },
    ValidationRule {
        name: "SingleFieldSubscriptionsRule",
        create: single_field_subscriptions_rule,
    },
    ValidationRule {
        name: "KnownTypeNamesRule",
        create: |context| known_type_names_rule(context.into()),
    },
    ValidationRule {
        name: "FragmentsOnCompositeTypesRule",
        create: fragments_on_composite_types_rule,
    },
    ValidationRule {
        name: "VariablesAreInputTypesRule",
        create: variables_are_input_types_rule,
    },
    ValidationRule {
        name: "ScalarLeafsRule",
        create: scalar_leafs_rule,
    },
    ValidationRule {
        name: "FieldsOnCorrectTypeRule",
        create: fields_on_correct_type_rule,
    },
    ValidationRule {
        name: "UniqueFragmentNamesRule",
        create: |context| unique_fragment_names_rule(context),
    },
    ValidationRule {
        name: "KnownFragmentNamesRule",
        create: known_fragment_names_rule,
    },
    ValidationRule {
        name: "NoUnusedFragmentsRule",
        create: |context| no_unused_fragments_rule(context),
    },
    ValidationRule {
        name: "PossibleFragmentSpreadsRule",
        create: possible_fragment_spreads_rule,
    },
    ValidationRule {
        name: "NoFragmentCyclesRule",
        create: |context| no_fragment_cycles_rule(context),
    },
    ValidationRule {
        name: "UniqueVariableNamesRule",
        create: |context| unique_variable_names_rule(context),
    },
    ValidationRule {
        name: "NoUndefinedVariablesRule",
        create: no_undefined_variables_rule,
    },
    ValidationRule {
        name: "NoUnusedVariablesRule",
        create: no_unused_variables_rule,
    },
    ValidationRule {
        name: "KnownDirectivesRule",
        create: |context| known_directives_rule(context.into()),
    },
    ValidationRule {
        name: "UniqueDirectivesPerLocationRule",
        create: |context| unique_directives_per_location_rule(context.into()),
    },
    ValidationRule {
        name: "KnownArgumentNamesRule",
        create: known_argument_names_rule,
    },
    ValidationRule {
        name: "UniqueArgumentNamesRule",
        create: |context| unique_argument_names_rule(context),
    },
    ValidationRule {
        name: "ValuesOfCorrectTypeRule",
        create: values_of_correct_type_rule,
    },
    ValidationRule {
        name: "ProvidedRequiredArgumentsRule",
        create: provided_required_arguments_rule,
    },
    ValidationRule {
        name: "VariablesInAllowedPositionRule",
        create: variables_in_allowed_position_rule,
    },
    ValidationRule {
        name: "OverlappingFieldsCanBeMergedRule",
        create: overlapping_fields_can_be_merged_rule,
    },
    ValidationRule {
        name: "UniqueInputFieldNamesRule",
        create: |context| unique_input_field_names_rule(context),
    },
    MAX_INTROSPECTION_DEPTH_RULE,
];

/// The rules of a schema document, which the reference marks internal.
///
/// Derived from graphql-js 16.12.0, src/validation/specifiedRules.ts, MIT.
pub const SPECIFIED_SDL_RULES: &[SdlValidationRule] = &[
    SdlValidationRule {
        name: "LoneSchemaDefinitionRule",
        create: lone_schema_definition_rule,
    },
    SdlValidationRule {
        name: "UniqueOperationTypesRule",
        create: unique_operation_types_rule,
    },
    SdlValidationRule {
        name: "UniqueTypeNamesRule",
        create: unique_type_names_rule,
    },
    SdlValidationRule {
        name: "UniqueEnumValueNamesRule",
        create: unique_enum_value_names_rule,
    },
    SdlValidationRule {
        name: "UniqueFieldDefinitionNamesRule",
        create: unique_field_definition_names_rule,
    },
    SdlValidationRule {
        name: "UniqueArgumentDefinitionNamesRule",
        create: unique_argument_definition_names_rule,
    },
    SdlValidationRule {
        name: "UniqueDirectiveNamesRule",
        create: unique_directive_names_rule,
    },
    SdlValidationRule {
        name: "KnownTypeNamesRule",
        create: |context| known_type_names_rule(context.into()),
    },
    SdlValidationRule {
        name: "KnownDirectivesRule",
        create: |context| known_directives_rule(context.into()),
    },
    SdlValidationRule {
        name: "UniqueDirectivesPerLocationRule",
        create: |context| unique_directives_per_location_rule(context.into()),
    },
    SdlValidationRule {
        name: "PossibleTypeExtensionsRule",
        create: possible_type_extensions_rule,
    },
    SdlValidationRule {
        name: "KnownArgumentNamesOnDirectivesRule",
        create: |context| known_argument_names_on_directives_rule(context.into()),
    },
    SdlValidationRule {
        name: "UniqueArgumentNamesRule",
        create: |context| unique_argument_names_rule(context),
    },
    SdlValidationRule {
        name: "UniqueInputFieldNamesRule",
        create: |context| unique_input_field_names_rule(context),
    },
    SdlValidationRule {
        name: "ProvidedRequiredArgumentsOnDirectivesRule",
        create: |context| provided_required_arguments_on_directives_rule(context.into()),
    },
];

/// Returns the rule of [`SPECIFIED_RULES`] whose function is named `name`
/// in the reference, such as `ScalarLeafsRule`.
#[must_use]
pub fn specified_rule_named(name: &str) -> Option<ValidationRule> {
    SPECIFIED_RULES
        .iter()
        .find(|candidate| candidate.name == name)
        .copied()
}

/// Returns the rule of [`SPECIFIED_SDL_RULES`] whose function is named
/// `name` in the reference, such as `UniqueTypeNamesRule`.
#[must_use]
pub fn specified_sdl_rule_named(name: &str) -> Option<SdlValidationRule> {
    SPECIFIED_SDL_RULES
        .iter()
        .find(|candidate| candidate.name == name)
        .copied()
}
