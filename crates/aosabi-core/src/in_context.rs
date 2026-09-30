// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-core in_context: the `@inContext` directive as a resolver sees it.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/core/in-context.ts and
 * packages/pylon/src/plugins/use-in-context.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 */

//! The context an operation declares in its own document.
//!
//! An operation carries its locale as `@inContext(locale: $__locale)`, so the
//! locale is a variable and therefore a part of every client cache key. The
//! server reads the directive off the operation before it executes and keeps
//! what it found in the request context, where a resolver reads it through
//! [`get_locale`] and [`get_in_context`].
//!
//! The SDL of the directive is the extracted literal `in_context.graphql`
//! beside this file, which `tools/provenance` writes. The plugin
//! `use_in_context` of `plugins::in_context` reads the directive in its hook
//! `onExecute` over HTTP, and the service below HTTP calls
//! [`apply_in_context`] where that hook would run, over the same reading.

use aosabi_execute::kernel_entry::Request;
use aosabi_json::{
    JsString,
    JsValue,
    reader, //
};
use aosabi_lang::{
    ParseOptions,
    ast::{
        DefinitionNode,
        DirectiveNode,
        DocumentNode,
        NameNode,
        OperationDefinitionNode,
        ValueNode, //
    },
    js_string::to_string_lossy,
    parse, //
};

use aosabi_http::Key;
use aosabi_json::JsObject;

use crate::context::{
    Context,
    get_context, //
};

/// The SDL of the directive, `in-context.ts:37-44`.
pub const IN_CONTEXT_SDL: &str = include_str!("in_context.graphql");

/// The context variable that holds the resolved context of the operation,
/// `in-context.ts:47`.
pub const IN_CONTEXT_KEY: &str = "pylonInContext";

/// The typed key of [`IN_CONTEXT_KEY`].
pub const IN_CONTEXT: Key<InContext> = Key::new(IN_CONTEXT_KEY);

/// The name of the directive as a document writes it.
const DIRECTIVE_NAME: &str = "inContext";

/// Everything `@inContext` carried for one operation.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InContext {
    /// The locale the operation asked for, `None` when it asked for none.
    pub locale: Option<String>,
    /// The `context` argument parsed from its JSON text, an object or an
    /// array. The reference types it as the `OperationContext` bag of the
    /// application.
    pub context: Option<JsValue>,
}

/// Returns the locale the running operation asked for.
///
/// `None` is meaningful: the caller stated no locale, and a resolver serves
/// what it considers neutral.
#[must_use]
pub fn get_locale() -> Option<String> {
    get_in_context().locale
}

/// Returns everything `@inContext` carried for the running operation.
///
/// Outside a request there is no context to read, and the answer is the empty
/// value.
#[must_use]
pub fn get_in_context() -> InContext {
    let Ok(context) = get_context() else {
        return InContext::default();
    };
    context.get(IN_CONTEXT).unwrap_or_default()
}

impl InContext {
    /// Returns the operation context read as the application's own type, the
    /// twin of the `OperationContext` bag an application types by
    /// declaration merging.
    ///
    /// The conversion reads the parsed object, and `None` answers an operation
    /// that carried no context and a context the conversion refuses.
    #[must_use]
    pub fn context_as<T: TryFrom<JsObject>>(&self) -> Option<T> {
        match &self.context {
            Some(JsValue::Object(object)) => T::try_from(object.clone()).ok(),
            _ => None,
        }
    }
}

/// Reads `@inContext` off the operation of `request` into `context`.
///
/// This stands where the hook `onExecute` of the plugin runs. The hook sees a
/// parsed and validated document. The pipeline of the executor crate parses
/// inside, so the document is parsed here a second time, and only when its
/// text names the directive at all. A document that does not parse reaches
/// no resolver, so nothing is lost by leaving its context unset.
pub fn apply_in_context(context: &Context, request: &Request<'_>) {
    if !request.document.contains(DIRECTIVE_NAME) {
        return;
    }
    let options = ParseOptions {
        no_location: true,
        ..ParseOptions::default()
    };
    let Ok(document) = parse(request.document, options) else {
        return;
    };
    let in_context = in_context_of(
        &document,
        request.operation_name,
        request.variables.as_ref(),
    );
    if let Some(in_context) = in_context {
        context.set(IN_CONTEXT, in_context);
    }
}

/// Returns what the `@inContext` directive of the executing operation carries.
///
/// `variable_values` is the variable map of the request as it arrived, which
/// is what the hook reads. `None` means the context stays unset, because the
/// operation has no such directive or nothing in it resolved to a value.
#[must_use]
pub fn in_context_of(
    document: &DocumentNode,
    operation_name: Option<&str>,
    variable_values: Option<&JsValue>,
) -> Option<InContext> {
    let operation = find_operation(document, operation_name)?;
    let directive = operation
        .directives
        .as_ref()?
        .iter()
        .find(|directive| directive.name.value() == DIRECTIVE_NAME)?;

    let locale = arg_value(directive, "locale", variable_values);
    // `context` rides as a JSON string. A malformed value is ignored rather
    // than failing the operation.
    let context = arg_value(directive, "context", variable_values)
        .and_then(|json| reader::parse(json.as_wtf8()).ok())
        .filter(|parsed| matches!(parsed, JsValue::Object(_) | JsValue::Array(_)));

    // Nothing resolved (e.g. the variables were all null): leave the context unset.
    if locale.is_none() && context.is_none() {
        return None;
    }
    Some(InContext {
        locale: locale.map(|locale| to_string_lossy(&locale).into_owned()),
        context,
    })
}

/// Returns the operation the executor is about to run.
///
/// An operation name that is absent or empty is falsy and selects the first
/// operation.
fn find_operation<'a>(
    document: &'a DocumentNode,
    operation_name: Option<&str>,
) -> Option<&'a OperationDefinitionNode> {
    let operation_name = operation_name.filter(|name| !name.is_empty());
    document.definitions.iter().find_map(|definition| {
        let DefinitionNode::OperationDefinition(operation) = definition else {
            return None;
        };
        let name = operation.name.as_ref().map(NameNode::value);
        (operation_name.is_none() || name == operation_name).then_some(operation)
    })
}

/// Resolves the argument `name` of `directive` to its string value.
///
/// A variable is the compiled form, where one document serves every value, and
/// a string literal is the hand written form. Any other literal and an empty
/// string answer `None`.
fn arg_value(
    directive: &DirectiveNode,
    name: &str,
    variable_values: Option<&JsValue>,
) -> Option<JsString> {
    let arg = directive
        .arguments
        .as_ref()?
        .iter()
        .find(|arg| arg.name.value() == name)?;
    let value = match &arg.value {
        ValueNode::Variable(variable) => match variable_values? {
            JsValue::Object(variables) => match variables.get(variable.name.value())? {
                JsValue::String(text) => text.clone(),
                _ => return None,
            },
            _ => return None,
        },
        ValueNode::StringValue(literal) => JsString::from(literal.value.as_str()),
        _ => return None,
    };
    (!value.is_empty()).then_some(value)
}
