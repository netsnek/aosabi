// SPDX-License-Identifier: MIT
/*
 * aosabi-validate: the validation of an executable document against a
 * schema, and the validation of a schema document on its own or against the
 * schema it extends.
 *
 * Derived from graphql-js 16.12.0, src/validation/validate.ts, MIT.
 */

//! The validation entry points of the validation layer.
//!
//! [`validate()`] walks an executable document once with every rule in
//! parallel under one type information, [`validate_sdl()`] does the same for
//! a schema document without type information, and [`assert_valid_sdl()`]
//! and [`assert_valid_sdl_extension()`] fold a non empty error list into the
//! one plain error the schema builders throw.
//!
//! # The shape of the port
//!
//! The error callback of [`validate()`] throws a frozen object once the
//! error limit is reached, and the `try` around the walk swallows exactly
//! that object. The callback answers [`ValidationAbort`] here, the context
//! stops reporting, and a guard around the walk breaks it at the next hook,
//! so the error list ends with the abort message as it does in the
//! reference.
//!
//! Any other exception leaves `validate` in the reference. The one source
//! of such an exception that has a reading here is a thunk of the schema,
//! whose failure is a [`DefinitionError`]. The type information returns it
//! from its enter, a rule hands it to
//! [`AstValidationContext::throw_error()`], and the entry points return it
//! as `Err`.

use alloc::{
    boxed::Box,
    vec::Vec, //
};
use core::cell::RefCell;

use aosabi_json::JsString;
use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions,
    ast::DocumentNode,
    js_string::join,
    visitor::{
        EnterLeave,
        NodeRef,
        VisitContext,
        VisitResult,
        Visitor,
        enter_for_kind,
        leave_for_kind,
        visit,
        visit_in_parallel, //
    }, //
};
use aosabi_type::{
    DefinitionError,
    Schema,
    TypeInfo,
    assert_valid_schema,
    visit_with_type_info, //
};

use crate::{
    specified_rules::{
        SPECIFIED_RULES,
        SPECIFIED_SDL_RULES, //
    },
    validation_context::{
        AstValidationContext,
        SdlValidationContext,
        SdlValidationRule,
        ValidationAbort,
        ValidationContext,
        ValidationRule, //
    }, //
};

/// The error limit of [`validate()`] when the options name none.
///
/// Derived from graphql-js 16.12.0, src/validation/validate.ts, MIT.
pub const DEFAULT_MAX_ERRORS: usize = 100;

/// The options of [`validate()`].
///
/// Derived from graphql-js 16.12.0, src/validation/validate.ts, MIT.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ValidateOptions {
    /// The count of errors after which the validation aborts,
    /// [`DEFAULT_MAX_ERRORS`] when absent.
    ///
    /// The reference compares the length of the error list against a number.
    /// A negative limit aborts at the first error, which `0` does here, and
    /// a `NaN` never aborts, because a comparison with `NaN` is false, which
    /// `usize::MAX` does here.
    pub max_errors: Option<usize>,
}

/// A visitor that breaks the walk once the context is stopped, and forwards
/// every hook to `visitor` until then.
///
/// The exception of the reference ends the walk inside the hook that
/// throws. A hook returns here, so the walk ends at the next hook instead,
/// and the context reports nothing in between.
struct StopGuard<'c, 'v, 'a, V: ?Sized> {
    /// The context that knows whether the validation is stopped.
    context: &'c AstValidationContext<'a>,
    /// The visitor of the walk.
    visitor: &'v mut V,
}

impl<'a, V> Visitor<'a> for StopGuard<'_, '_, 'a, V>
where
    V: Visitor<'a> + ?Sized,
{
    fn enter(&mut self, node: NodeRef<'a>, context: &VisitContext<'a, '_>) -> VisitResult {
        if self.context.is_stopped() {
            return VisitResult::Break;
        }
        enter_for_kind(&mut *self.visitor, node, context)
    }

    fn leave(&mut self, node: NodeRef<'a>, context: &VisitContext<'a, '_>) -> VisitResult {
        if self.context.is_stopped() {
            return VisitResult::Break;
        }
        leave_for_kind(&mut *self.visitor, node, context)
    }
}

/// Implements the "Validation" section of the spec.
///
/// Validation runs synchronously, returning an array of encountered errors,
/// or an empty array if no errors were encountered and the document is
/// valid.
///
/// Derived from graphql-js 16.12.0, src/validation/validate.ts, MIT.
///
/// A list of specific validation rules may be provided as `rules`. If not
/// provided, the default list of rules defined by the GraphQL specification
/// is used, which is [`SPECIFIED_RULES`]. Each validation rule is a function
/// which returns a visitor, and a rule reports its errors to the context.
///
/// Validate stops validation after the `max_errors` limit of `options` has
/// been reached. Attackers can send pathologically invalid queries to induce
/// a denial of service, so by default `max_errors` is set to 100 errors.
///
/// Optionally a custom `type_info` may be provided, which the reference
/// deprecates. If not provided, one is created from `schema`. The
/// `devAssert` over the document has no reading, because the argument type
/// carries it.
///
/// # Errors
///
/// Returns the plain error of [`assert_valid_schema()`] when `schema` is
/// invalid, and what a thunk of the schema throws during the walk.
pub fn validate<'a>(
    schema: &'a Schema,
    document_ast: &'a DocumentNode,
    rules: Option<&[ValidationRule]>,
    options: ValidateOptions,
    type_info: Option<TypeInfo<'a>>,
) -> Result<Vec<GraphQLError>, DefinitionError> {
    let max_errors = options.max_errors.unwrap_or(DEFAULT_MAX_ERRORS);
    let rules = rules.unwrap_or(SPECIFIED_RULES);

    // If the schema used for validation is invalid, throw an error.
    assert_valid_schema(schema)?;

    let mut errors: Vec<GraphQLError> = Vec::new();
    let type_info = RefCell::new(type_info.unwrap_or_else(|| TypeInfo::new(schema, None, None)));
    let thrown = {
        let sink = &mut errors;
        let context = ValidationContext::new(
            schema,
            document_ast,
            &type_info,
            Box::new(move |error| {
                if sink.len() >= max_errors {
                    sink.push(GraphQLError::new(
                        "Too many validation errors, error limit reached. Validation aborted.",
                        GraphQLErrorOptions::default(),
                    ));
                    return Err(ValidationAbort);
                }
                sink.push(error);
                Ok(())
            }),
        );
        visit_with_rules(document_ast, &context, &type_info, rules)
    };
    match thrown {
        Some(error) => Err(error),
        None => Ok(errors),
    }
}

/// Visits the whole document with each instance of all provided rules, and
/// returns the error that left the walk, when one did.
///
/// This uses a specialized visitor which runs multiple visitors in parallel,
/// while maintaining the visitor skip and break API.
///
/// Derived from graphql-js 16.12.0, src/validation/validate.ts, MIT.
fn visit_with_rules<'a>(
    document_ast: &'a DocumentNode,
    context: &ValidationContext<'a>,
    type_info: &RefCell<TypeInfo<'a>>,
    rules: &[ValidationRule],
) -> Option<DefinitionError> {
    let mut visitors: Vec<Box<dyn EnterLeave<'a> + '_>> =
        rules.iter().map(|rule| (rule.create)(context)).collect();
    let mut visitor = visit_in_parallel(
        visitors
            .iter_mut()
            .map(|visitor| &mut **visitor as &mut dyn EnterLeave<'a>)
            .collect(),
    );
    let mut visitor = visit_with_type_info(type_info, &mut visitor);
    visit(
        document_ast,
        &mut StopGuard {
            context,
            visitor: &mut visitor,
        },
    );
    context.take_thrown().or_else(|| visitor.into_error())
}

/// Validates a schema document, on its own or against `schema_to_extend`.
///
/// `rules` are the rules to run, [`SPECIFIED_SDL_RULES`] when absent. The
/// reference marks the function internal, and the schema builders reach it
/// through the two asserting functions below.
///
/// Derived from graphql-js 16.12.0, src/validation/validate.ts, MIT.
///
/// # Errors
///
/// Returns what a rule hands to [`AstValidationContext::throw_error()`].
pub fn validate_sdl(
    document_ast: &DocumentNode,
    schema_to_extend: Option<&Schema>,
    rules: Option<&[SdlValidationRule]>,
) -> Result<Vec<GraphQLError>, DefinitionError> {
    let rules = rules.unwrap_or(SPECIFIED_SDL_RULES);
    let mut errors: Vec<GraphQLError> = Vec::new();
    let thrown = {
        let sink = &mut errors;
        let context = SdlValidationContext::new(
            document_ast,
            schema_to_extend,
            Box::new(move |error| {
                sink.push(error);
                Ok(())
            }),
        );
        visit_with_sdl_rules(document_ast, &context, rules)
    };
    match thrown {
        Some(error) => Err(error),
        None => Ok(errors),
    }
}

/// Visits the whole schema document with every rule in parallel, and returns
/// the error that left the walk, when one did.
///
/// Derived from graphql-js 16.12.0, src/validation/validate.ts, MIT.
fn visit_with_sdl_rules<'a>(
    document_ast: &'a DocumentNode,
    context: &SdlValidationContext<'a>,
    rules: &[SdlValidationRule],
) -> Option<DefinitionError> {
    let mut visitors: Vec<Box<dyn EnterLeave<'a> + '_>> =
        rules.iter().map(|rule| (rule.create)(context)).collect();
    let mut visitor = visit_in_parallel(
        visitors
            .iter_mut()
            .map(|visitor| &mut **visitor as &mut dyn EnterLeave<'a>)
            .collect(),
    );
    visit(
        document_ast,
        &mut StopGuard {
            context,
            visitor: &mut visitor,
        },
    );
    context.take_thrown()
}

/// Joins the message of every error with a blank line, the text of the one
/// error a failed SDL validation throws.
///
/// Derived from graphql-js 16.12.0, src/validation/validate.ts, MIT.
fn join_messages(errors: &[GraphQLError]) -> JsString {
    join(errors.iter().map(|error| &error.message), "\n\n")
}

/// Utility function which asserts a SDL document is valid by throwing an
/// error if it is invalid.
///
/// The function has the type of `aosabi_type::AssertValidSdl`, the seam of
/// the schema builders, and [`crate::build_schema()`] hands it in.
///
/// Derived from graphql-js 16.12.0, src/validation/validate.ts, MIT.
///
/// # Errors
///
/// Returns a plain error whose message joins the message of every
/// validation error with a blank line, and what [`validate_sdl()`] returns.
pub fn assert_valid_sdl(document_ast: &DocumentNode) -> Result<(), DefinitionError> {
    let errors = validate_sdl(document_ast, None, None)?;
    if !errors.is_empty() {
        return Err(DefinitionError::plain(join_messages(&errors)));
    }
    Ok(())
}

/// Utility function which asserts a SDL document is valid by throwing an
/// error if it is invalid.
///
/// The function has the type of `aosabi_type::AssertValidSdlExtension`, the
/// seam of the schema extension, and [`crate::extend_schema()`] hands it in.
///
/// Derived from graphql-js 16.12.0, src/validation/validate.ts, MIT.
///
/// # Errors
///
/// Returns a plain error whose message joins the message of every
/// validation error with a blank line, and what [`validate_sdl()`] returns.
pub fn assert_valid_sdl_extension(
    document_ast: &DocumentNode,
    schema: &Schema,
) -> Result<(), DefinitionError> {
    let errors = validate_sdl(document_ast, Some(schema), None)?;
    if !errors.is_empty() {
        return Err(DefinitionError::plain(join_messages(&errors)));
    }
    Ok(())
}
