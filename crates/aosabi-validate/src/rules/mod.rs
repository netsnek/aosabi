// SPDX-License-Identifier: MIT
/*
 * aosabi-validate: the module list of the validation rules, one module for
 * each file of the rule directory of the reference.
 *
 * Derived from graphql-js 16.12.0, src/validation/rules, MIT.
 */

//! The validation rules of the validation layer.
//!
//! One module stands for each file of the rule directory of the reference
//! and carries its name in snake case. A module holds the rule as a struct
//! that implements [`Rule`](crate::validation_context::Rule) and the
//! visitor of the language layer, and the function of the reference in
//! snake case, which is what [`crate::specified_rules`] lists. The two
//! custom rules of `rules/custom` are in neither specified list and have no
//! module here.
//!
//! | Module | Translates | Context |
//! | --- | --- | --- |
//! | [`executable_definitions_rule`] | `ExecutableDefinitionsRule.ts` | any document |
//! | [`fields_on_correct_type_rule`] | `FieldsOnCorrectTypeRule.ts` | an executable document |
//! | [`fragments_on_composite_types_rule`] | `FragmentsOnCompositeTypesRule.ts` | an executable document |
//! | [`known_argument_names_rule`] | `KnownArgumentNamesRule.ts` | an executable document and either document |
//! | [`known_directives_rule`] | `KnownDirectivesRule.ts` | either document |
//! | [`known_fragment_names_rule`] | `KnownFragmentNamesRule.ts` | an executable document |
//! | [`known_type_names_rule`] | `KnownTypeNamesRule.ts` | either document |
//! | [`lone_anonymous_operation_rule`] | `LoneAnonymousOperationRule.ts` | any document |
//! | [`lone_schema_definition_rule`] | `LoneSchemaDefinitionRule.ts` | a schema document |
//! | [`max_introspection_depth_rule`] | `MaxIntrospectionDepthRule.ts` | any document |
//! | [`no_fragment_cycles_rule`] | `NoFragmentCyclesRule.ts` | any document |
//! | [`no_undefined_variables_rule`] | `NoUndefinedVariablesRule.ts` | an executable document |
//! | [`no_unused_fragments_rule`] | `NoUnusedFragmentsRule.ts` | any document |
//! | [`no_unused_variables_rule`] | `NoUnusedVariablesRule.ts` | an executable document |
//! | [`overlapping_fields_can_be_merged_rule`] | `OverlappingFieldsCanBeMergedRule.ts` | an executable document |
//! | [`possible_fragment_spreads_rule`] | `PossibleFragmentSpreadsRule.ts` | an executable document |
//! | [`possible_type_extensions_rule`] | `PossibleTypeExtensionsRule.ts` | a schema document |
//! | [`provided_required_arguments_rule`] | `ProvidedRequiredArgumentsRule.ts` | an executable document and either document |
//! | [`scalar_leafs_rule`] | `ScalarLeafsRule.ts` | an executable document |
//! | [`single_field_subscriptions_rule`] | `SingleFieldSubscriptionsRule.ts` | an executable document |
//! | [`unique_argument_definition_names_rule`] | `UniqueArgumentDefinitionNamesRule.ts` | a schema document |
//! | [`unique_argument_names_rule`] | `UniqueArgumentNamesRule.ts` | any document |
//! | [`unique_directive_names_rule`] | `UniqueDirectiveNamesRule.ts` | a schema document |
//! | [`unique_directives_per_location_rule`] | `UniqueDirectivesPerLocationRule.ts` | either document |
//! | [`unique_enum_value_names_rule`] | `UniqueEnumValueNamesRule.ts` | a schema document |
//! | [`unique_field_definition_names_rule`] | `UniqueFieldDefinitionNamesRule.ts` | a schema document |
//! | [`unique_fragment_names_rule`] | `UniqueFragmentNamesRule.ts` | any document |
//! | [`unique_input_field_names_rule`] | `UniqueInputFieldNamesRule.ts` | any document |
//! | [`unique_operation_names_rule`] | `UniqueOperationNamesRule.ts` | any document |
//! | [`unique_operation_types_rule`] | `UniqueOperationTypesRule.ts` | a schema document |
//! | [`unique_type_names_rule`] | `UniqueTypeNamesRule.ts` | a schema document |
//! | [`unique_variable_names_rule`] | `UniqueVariableNamesRule.ts` | any document |
//! | [`values_of_correct_type_rule`] | `ValuesOfCorrectTypeRule.ts` | an executable document |
//! | [`variables_are_input_types_rule`] | `VariablesAreInputTypesRule.ts` | an executable document |
//! | [`variables_in_allowed_position_rule`] | `VariablesInAllowedPositionRule.ts` | an executable document |

pub mod executable_definitions_rule;
pub mod fields_on_correct_type_rule;
pub mod fragments_on_composite_types_rule;
pub mod known_argument_names_rule;
pub mod known_directives_rule;
pub mod known_fragment_names_rule;
pub mod known_type_names_rule;
pub mod lone_anonymous_operation_rule;
pub mod lone_schema_definition_rule;
pub mod max_introspection_depth_rule;
pub mod no_fragment_cycles_rule;
pub mod no_undefined_variables_rule;
pub mod no_unused_fragments_rule;
pub mod no_unused_variables_rule;
pub mod overlapping_fields_can_be_merged_rule;
pub mod possible_fragment_spreads_rule;
pub mod possible_type_extensions_rule;
pub mod provided_required_arguments_rule;
pub mod scalar_leafs_rule;
pub mod single_field_subscriptions_rule;
pub mod unique_argument_definition_names_rule;
pub mod unique_argument_names_rule;
pub mod unique_directive_names_rule;
pub mod unique_directives_per_location_rule;
pub mod unique_enum_value_names_rule;
pub mod unique_field_definition_names_rule;
pub mod unique_fragment_names_rule;
pub mod unique_input_field_names_rule;
pub mod unique_operation_names_rule;
pub mod unique_operation_types_rule;
pub mod unique_type_names_rule;
pub mod unique_variable_names_rule;
pub mod values_of_correct_type_rule;
pub mod variables_are_input_types_rule;
pub mod variables_in_allowed_position_rule;
