// SPDX-License-Identifier: MIT
/*
 * aosabi-federation: the delegation of @graphql-tools/delegate 12.0.9 below
 * the reference's federation.
 *
 * No range of its own.
 */

//! The delegation of a field to a schema, the port of `@graphql-tools/delegate`
//! 12.0.9 as far as the reference's federation reaches it.
//!
//! | Module | Translates | Holds |
//! | --- | --- | --- |
//! | [`subschema`] | `index.cjs:10-59`, `:2391-2424` | [`Subschema`], [`SubschemaConfig`], [`SchemaOrSubschemaConfig`], [`Executor`] and `apply_schema_transforms` |
//! | [`external`] | `index.cjs:60-103` | the side table of the external objects |
//! | [`resolve_external`] | `index.cjs:290-459` | [`resolve_external_value`] and the object and list forms |
//! | [`check_result`] | `index.cjs:460-559` | [`check_result_and_handle_errors`] and [`merge_data_and_errors`] |
//! | [`finalize`] | `index.cjs:560-1177` | [`finalize_gateway_request`] and the filter of a selection set |
//! | [`prepare`] | `index.cjs:1178-1703` | [`prepare_gateway_document`] and `wrap_concrete_types` |
//! | [`transformer`] | `index.cjs:1704-1787` | [`Transformer`] and `get_delegating_operation` |
//! | [`create_request`](mod@create_request) | `index.cjs:1788-1987` | [`create_request()`] and `project_argument_value` |
//! | [`merged_resolver`] | `index.cjs:1988-2061` | [`default_merged_resolver`] |
//! | [`delegate_to_schema`](mod@delegate_to_schema) | `index.cjs:2425-2651` | [`delegate_to_schema()`], [`delegate_request`] and `get_executor` |
//!
//! The ranges the call graph does not reach are named in the module that
//! holds their neighbours: the stitching of `mergeFields` at `:99-288` in
//! [`external`], `handleLeftOver` and the functions below it at
//! `:2062-2389` in [`merged_resolver`], `cloneSubschemaConfig` at
//! `:2394-2423` in [`subschema`], and `setObjectKeyPath` at `:2652-2670` in
//! [`delegate_to_schema`](mod@delegate_to_schema). The last three functions
//! of the file, `extractUnavailableFieldsFromSelectionSet`,
//! `extractUnavailableFields` and `subtractSelectionSets` at `:2672-2898`,
//! are exports no function of the delegation calls, and are left out.

pub mod check_result;
pub mod create_request;
pub mod delegate_to_schema;
pub mod external;
pub mod finalize;
pub mod merged_resolver;
pub mod prepare;
pub mod resolve_external;
pub mod subschema;
pub mod transformer;

pub use self::{
    check_result::{
        check_result_and_handle_errors,
        merge_data_and_errors, //
    },
    create_request::{
        CreateRequestOptions,
        create_request, //
    },
    delegate_to_schema::{
        DelegateToSchemaOptions,
        delegate_request,
        delegate_to_schema, //
    },
    external::{
        annotate_external_object,
        get_subschema,
        get_unpathed_errors,
        is_external_object, //
    },
    finalize::finalize_gateway_request,
    merged_resolver::default_merged_resolver,
    prepare::prepare_gateway_document,
    resolve_external::resolve_external_value,
    subschema::{
        Executor,
        SchemaOrSubschemaConfig,
        Subschema,
        SubschemaConfig, //
    },
    transformer::Transformer, //
};
