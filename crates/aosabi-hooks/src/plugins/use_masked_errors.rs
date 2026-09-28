// SPDX-License-Identifier: MIT
/*
 * aosabi-hooks: the plugin that masks unexpected errors.
 */

//! `use_masked_errors` and the default mask.
//!
//! The plugin masks every error of a result that is no original `GraphQLError`
//! with the message `Unexpected error.`, registers the mask as the handler of
//! a failure of the context building, and masks the error of a subscription.
//! The result of a subscription is a stream, whose results the hook `on_next`
//! of `on_subscribe_result` masks one by one, and a failure of the stream
//! reaches `on_subscribe_error`, which replaces it with its mask. Every hook
//! of one plugin shares one mask and one message, as the reference makes its
//! handler of a result once per plugin.
//!
//! An original `GraphQLError` is one whose original error is absent or an
//! original `GraphQLError` itself. The default mask of development keeps the
//! message of the original error in `extensions`, and the HTTP server passes a
//! mask of its own.

use std::rc::Rc;

use aosabi_execute::{
    ErrorObject,
    Value,
    errors::into_graphql_error,
    value::to_js_string, //
};
use aosabi_json::{
    JsObject,
    JsValue, //
};
use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions,
    OriginalError, //
};

use crate::{
    plugin::{
        HookPlugin,
        OnExecute,
        OnExecuteDoneHook,
        OnExecuteHooks,
        OnPluginInit,
        OnSubscribe,
        OnSubscribeHooks, //
    },
    utils::handle_stream_or_single_execution_result, //
};

/// The message of a masked error, `DEFAULT_ERROR_MESSAGE`.
pub const DEFAULT_ERROR_MESSAGE: &str = "Unexpected error.";

/// The mask of an error, `(error, message) => error`.
pub type MaskErrorFn = Rc<dyn Fn(Value, &str) -> Value>;

/// Returns whether `error` is an instance of `GraphQLError`, `isGraphQLError(error)`.
#[must_use]
pub fn is_graphql_error(error: &Value) -> bool {
    matches!(error, Value::Error(error) if matches!(error.as_ref(), ErrorObject::GraphQL(_)))
}

/// Returns whether `error` is an original `GraphQLError`, `isOriginalGraphQLError(error)`.
///
/// A `GraphQLError` whose original error is absent or an original
/// `GraphQLError` itself.
#[must_use]
pub fn is_original_graphql_error(error: &Value) -> bool {
    matches!(error, Value::Error(error)
        if matches!(error.as_ref(), ErrorObject::GraphQL(error) if is_original(error)))
}

/// Returns whether the `GraphQLError` `error` is an original one.
#[must_use]
pub fn is_original(error: &GraphQLError) -> bool {
    match &error.original_error {
        None => true,
        Some(OriginalError::GraphQL(original)) => is_original(original),
        Some(OriginalError::Other(_)) => false,
    }
}

/// Returns the default mask for `is_dev`, `createDefaultMaskError(isDev)`.
///
/// An original `GraphQLError` is kept, and any other error becomes a
/// `GraphQLError` of `message` whose extensions, in development, carry the
/// message of the error.
#[must_use]
pub fn create_default_mask_error(is_dev: bool) -> MaskErrorFn {
    Rc::new(move |error, message| {
        if is_original_graphql_error(&error) {
            return error;
        }
        let mut options = GraphQLErrorOptions::default();
        if is_dev {
            let mut extensions = JsObject::new();
            let text = match &error {
                Value::Error(error) => error.message(),
                value => to_js_string(value),
            };
            extensions.insert("message", JsValue::String(text));
            options.extensions = Some(extensions);
        }
        Value::from(ErrorObject::GraphQL(GraphQLError::new(message, options)))
    })
}

/// Returns the default mask, `defaultMaskError`, for the `NODE_ENV` of the process.
#[must_use]
pub fn default_mask_error() -> MaskErrorFn {
    create_default_mask_error(std::env::var("NODE_ENV").is_ok_and(|env| env == "development"))
}

/// The options of the plugin, `{ maskError, errorMessage }`.
#[derive(Clone, Default)]
pub struct MaskedErrorsOptions {
    /// The mask, the default mask when absent.
    pub mask_error: Option<MaskErrorFn>,
    /// The message, `Unexpected error.` when absent or empty.
    pub error_message: Option<String>,
}

/// The mask and the message every hook of one plugin shares.
struct Mask {
    /// The mask.
    mask_error: MaskErrorFn,
    /// The message of a masked error.
    message: String,
}

impl Mask {
    /// Returns the mask of `error`, `maskError(error, message)`.
    fn apply(&self, error: Value) -> Value {
        (self.mask_error)(error, &self.message)
    }

    /// Replaces the error in `slot` with its mask, `setError(maskError(error, message))`.
    fn replace(&self, slot: &mut Value) {
        let error = core::mem::replace(slot, Value::Undefined);
        *slot = self.apply(error);
    }

    /// Masks every error of `errors` in place, `errors.map(error => maskError(error, message))`.
    fn all(&self, errors: &mut Vec<GraphQLError>) {
        let masked = core::mem::take(errors)
            .into_iter()
            .map(|error| into_graphql_error(self.apply(Value::from(ErrorObject::GraphQL(error)))))
            .collect();
        *errors = masked;
    }

    /// Returns the done hook that masks the errors of a result, and of each
    /// result of a stream, `handleStreamOrSingleExecutionResult(payload, handleResult)`.
    fn done_hook(self: &Rc<Self>) -> OnExecuteDoneHook {
        let mask = Rc::clone(self);
        Box::new(move |payload| {
            handle_stream_or_single_execution_result(payload, move |result| {
                mask.all(&mut result.errors);
            })
        })
    }
}

/// The masked errors plugin.
struct UseMaskedErrors(Rc<Mask>);

impl HookPlugin for UseMaskedErrors {
    fn on_plugin_init(&self, p: &mut OnPluginInit<'_>) {
        let mask = Rc::clone(&self.0);
        p.register_context_error_handler(Rc::new(move |payload| mask.replace(payload.error)));
    }

    fn on_execute(&self, _p: &mut OnExecute<'_>) -> Result<Option<OnExecuteHooks>, Value> {
        Ok(Some(OnExecuteHooks {
            on_execute_done: Some(self.0.done_hook()),
        }))
    }

    fn on_subscribe(&self, _p: &mut OnSubscribe<'_>) -> Result<Option<OnSubscribeHooks>, Value> {
        let mask = Rc::clone(&self.0);
        Ok(Some(OnSubscribeHooks {
            on_subscribe_result: Some(self.0.done_hook()),
            on_subscribe_error: Some(Box::new(move |error| mask.replace(error))),
        }))
    }
}

/// Returns the plugin that masks unexpected errors, `useMaskedErrors(opts)`.
///
/// An absent mask is the default mask, and an absent or empty message is
/// `Unexpected error.`.
#[must_use]
pub fn use_masked_errors(opts: MaskedErrorsOptions) -> Rc<dyn HookPlugin> {
    let message = opts
        .error_message
        .filter(|message| !message.is_empty())
        .unwrap_or_else(|| String::from(DEFAULT_ERROR_MESSAGE));
    Rc::new(UseMaskedErrors(Rc::new(Mask {
        mask_error: opts.mask_error.unwrap_or_else(default_mask_error),
        message,
    })))
}
