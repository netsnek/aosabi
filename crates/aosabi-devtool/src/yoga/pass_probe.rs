// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool yoga pass_probe: the two instances that judge the gates of
 * the engine of the single pass, the gate of the document and the gate of
 * the schema.
 *
 * No analogue in the reference, which has no single pass.
 */

//! The probe instances of the single pass.
//!
//! | Name | Schema | Options beside the base options |
//! | --- | --- | --- |
//! | `passReplace` | first | the plugin [`Replace`], no request cache |
//! | `passInvalidSchema` | a schema `validateSchema` refuses | a context factory that throws, `maskedErrors: false`, no request cache |
//!
//! Neither installs the request cache, so every validation of a request meets
//! the validate function of the engine and its gates.
//!
//! The plugin of `passReplace` hands the validation another document than
//! the one the parse function of the engine returned for the request, chosen
//! by the request header `x-pass-replace`, with the text of the request and
//! the text of the header `x-pass-text`:
//!
//! | Value | Hook | The document the validation gets |
//! | --- | --- | --- |
//! | `set-tree` | `onParse`, `setParsedDocument` | the tree of `x-pass-text` under the location of the request's text |
//! | `set-loc` | `onParse`, `setParsedDocument` | the tree of the request's text under the location of `x-pass-text` |
//! | `set-copy` | `onParse`, `setParsedDocument` | the request's text parsed by the plugin |
//! | `set-earlier` | `onParse`, `setParsedDocument` | the document the parse function returned before its last one |
//! | `set-last` | `onParse`, `setParsedDocument` | the last document the parse function returned, as the request cache hands back a kept document |
//! | `done-tree` | the done hook of `onParse`, `replaceParseResult` | the tree of `x-pass-text` under the location of the parsed document |
//! | `done-loc` | the done hook of `onParse`, `replaceParseResult` | the tree of the parsed document under the location of `x-pass-text` |
//! | `done-copy` | the done hook of `onParse`, `replaceParseResult` | a copy of the parsed document at another address |
//! | `done-earlier` | the done hook of `onParse`, `replaceParseResult` | the document the parse function returned before this one |
//!
//! Every document but the one of `set-last` is no document the parse function
//! of the engine returned last, so the gate of the document closes and the
//! rules validate it afresh, as they do with the single pass off. A request
//! without the header, or one whose earlier document does not exist yet,
//! passes through untouched, and the plugin keeps the last two documents the
//! parse function returned.
//!
//! The schema of `passInvalidSchema` builds, and `validateSchema` of
//! graphql-js refuses it, since the object type `Thing` lacks the field `id`
//! of its interface `Node`. The validation throws that error before Yoga
//! builds the context, whose factory throws as an auth check would, so the
//! answer carries the schema's message and not the context's, with the single
//! pass on and off.

use core::cell::RefCell;
use std::{
    any::Any,
    rc::Rc,
    sync::Arc, //
};

use aosabi_core::{
    ResolverMap,
    make_executable_schema, //
};
use aosabi_execute::{
    ErrorObject,
    Value, //
};
use aosabi_hooks::{
    HookContext,
    HookPlugin,
    LocalBoxFuture,
    plugin::{
        OnParse,
        OnParseDone,
        OnParseDoneHook, //
    }, //
};
use aosabi_http::Request;
use aosabi_lang::{
    ParseOptions,
    ast::DocumentNode, //
};
use aosabi_type::Schema;
use graphql_over_http::{
    AgentSetup,
    HttpOptions,
    HttpPlugin,
    MaskedErrors,
    ServerAdapterPlugin, //
};

use super::capture_schema::{
    first_agent,
    first_schema, //
};

/// The names of the probe instances.
pub const NAMES: [&str; 2] = ["passReplace", "passInvalidSchema"];

/// The header that names the document the plugin of `passReplace` hands the
/// validation.
pub const MODE_HEADER: &str = "x-pass-replace";

/// The header that carries the other text of the modes `set-tree`,
/// `set-loc`, `done-tree` and `done-loc`.
pub const TEXT_HEADER: &str = "x-pass-text";

/// The type definitions of `passInvalidSchema`, which build and which
/// `validateSchema` refuses, since `Thing` does not provide the field `id` of
/// its interface `Node`.
pub const INVALID_TYPE_DEFS: &str = "
  type Query {
    hello: String!
    node: Node
  }

  interface Node {
    id: ID!
  }

  type Thing implements Node {
    name: String
  }
";

/// The message the context factory of `passInvalidSchema` throws.
pub const CONTEXT_MESSAGE: &str = "the context refuses the request";

/// Returns the options of the probe instance `name`, or `None` for a name
/// that is no probe instance.
///
/// # Errors
///
/// Returns the message of a schema that does not build.
pub fn options(name: &str) -> Result<Option<HttpOptions>, String> {
    let options = match name {
        "passReplace" => HttpOptions {
            schema: Some(first_schema()?),
            agent: first_agent(),
            plugins: vec![Rc::new(Replace::default())],
            ..base()
        },
        "passInvalidSchema" => HttpOptions {
            schema: Some(invalid_schema()?),
            agent: AgentSetup::default(),
            masked_errors: MaskedErrors::Off,
            context: Some(Rc::new(refuse_context)),
            ..base()
        },
        _ => return Ok(None),
    };
    Ok(Some(options))
}

/// Returns the options both probe instances share: the endpoint of a served app and no
/// request cache.
fn base() -> HttpOptions {
    HttpOptions {
        graphql_endpoint: Some(String::from("/graphql")),
        parser_and_validation_cache: Some(false),
        ..HttpOptions::default()
    }
}

/// Returns the schema of `passInvalidSchema`, built with no resolver.
///
/// # Errors
///
/// Returns the message of the build, which the SDL above never meets.
fn invalid_schema() -> Result<Arc<Schema>, String> {
    make_executable_schema(INVALID_TYPE_DEFS, &ResolverMap::default())
        .map_err(|error| String::from_utf8_lossy(error.message().as_wtf8()).into_owned())
}

/// The entries the context factory of `passInvalidSchema` answers, which it
/// never answers.
type Entries = Vec<(String, Rc<dyn Any>)>;

/// Throws [`CONTEXT_MESSAGE`] as the context factory of an auth check that
/// refuses every request.
fn refuse_context(_context: &HookContext) -> LocalBoxFuture<'static, Result<Entries, Value>> {
    Box::pin(async { Err(Value::from(ErrorObject::error(CONTEXT_MESSAGE))) })
}

/// The documents the parse function of the engine returned last.
#[derive(Default)]
struct Kept {
    /// The last one.
    last: Option<Arc<DocumentNode>>,
    /// The one before it.
    earlier: Option<Arc<DocumentNode>>,
}

/// The plugin of `passReplace`, which replaces the document of a request by
/// the value of its header [`MODE_HEADER`].
#[derive(Default)]
struct Replace {
    /// The documents the parse function returned last, shared with the done
    /// hooks.
    kept: Rc<RefCell<Kept>>,
}

/// Parses `text` with the options of the engine, `None` where it does not
/// parse.
fn parsed(text: &str) -> Option<DocumentNode> {
    aosabi_lang::parse(text, ParseOptions::default()).ok()
}

/// Returns the tree of `tree` under the location of `located`.
fn mixed(tree: &DocumentNode, located: &DocumentNode) -> Arc<DocumentNode> {
    Arc::new(DocumentNode {
        loc: located.loc.clone(),
        definitions: tree.definitions.clone(),
        token_count: tree.token_count,
    })
}

/// Returns the mode and the other text a request names in its headers.
fn marker(context: &HookContext) -> Option<(String, String)> {
    let request = context.get_as::<Request>("request")?;
    let mode = request.header(MODE_HEADER)?;
    let text = request.header(TEXT_HEADER).unwrap_or_default();
    Some((mode, text))
}

impl Replace {
    /// Returns the document a mode of `onParse` hands the validation for the
    /// text `source`, or `None` where the mode names none.
    fn before(&self, mode: &str, source: &str, text: &str) -> Option<Arc<DocumentNode>> {
        let kept = self.kept.borrow();
        match mode {
            "set-tree" => Some(mixed(&parsed(text)?, &parsed(source)?)),
            "set-loc" => Some(mixed(&parsed(source)?, &parsed(text)?)),
            "set-copy" => Some(Arc::new(parsed(source)?)),
            "set-earlier" => kept.earlier.clone(),
            "set-last" => kept.last.clone(),
            _ => None,
        }
    }
}

/// Keeps `document` as the last document the parse function returned, and
/// answers the document a mode of the done hook hands the validation in its
/// place, or `None` where the mode names none.
fn after(
    kept: &RefCell<Kept>,
    mode: Option<&str>,
    text: &str,
    document: &Arc<DocumentNode>,
) -> Option<Arc<DocumentNode>> {
    let mut kept = kept.borrow_mut();
    let earlier = kept.last.replace(Arc::clone(document));
    kept.earlier.clone_from(&earlier);
    match mode? {
        "done-tree" => Some(mixed(&parsed(text)?, document)),
        "done-loc" => Some(mixed(document, &parsed(text)?)),
        "done-copy" => Some(Arc::new(DocumentNode::clone(document))),
        "done-earlier" => earlier,
        _ => None,
    }
}

impl HookPlugin for Replace {
    fn on_parse(&self, p: &mut OnParse<'_>) -> Result<Option<OnParseDoneHook>, Value> {
        let marked = marker(p.context);
        if let Some((mode, text)) = &marked
            && let Some(document) = self.before(mode, p.source, text)
        {
            p.set_parsed_document(document);
            return Ok(None);
        }
        let kept = Rc::clone(&self.kept);
        Ok(Some(Box::new(move |done: &mut OnParseDone<'_>| {
            let Ok(document) = done.result.as_ref() else {
                return;
            };
            let (mode, text) = marked.unzip();
            let text = text.unwrap_or_default();
            if let Some(replaced) = after(&kept, mode.as_deref(), &text, document) {
                done.replace_parse_result(Ok(replaced));
            }
        })))
    }
}

impl ServerAdapterPlugin for Replace {}

impl HttpPlugin for Replace {}
