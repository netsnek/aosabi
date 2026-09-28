// SPDX-License-Identifier: MIT
/*
 * graphql-over-http: the GraphiQL page Yoga answers.
 *
 * The template graphiql/template.html beside it is written by tools/provenance
 * and by nothing else.
 *
 * Derived from graphql-yoga 5.18.0, cjs/graphiql-html.js, MIT.
 * Derived from graphql-yoga 5.18.0, cjs/plugins/use-graphiql.js, MIT.
 */

//! `render_graphiql`, the `GraphiQL` page.
//!
//! The page is the template with the first `__TITLE__` replaced by the title,
//! `Yoga GraphiQL` when it is absent or empty, and the first `__OPTS__`
//! replaced by the JSON of the options. `String.prototype.replace` reads `$`
//! patterns in its replacement even when the pattern is a string, so `$$`,
//! `$&`, `` $` `` and `$'` in a title or in the options expand as they do
//! there.
//!
//! The file `graphiql/template.html` is the module `graphiql-html.js` of the
//! package as `tools/provenance` copies it, a string literal after `export
//! default`, so the page is that literal's value, read once.
//!
//! [󰀀](crate::answer::answer) answers a `GET` whose `Accept` contains
//! `text/html` and that passes one of the five endpoint tests with the page,
//! `Content-Type: text/html` with no charset and status 200, when the options
//! of the request are not `false`, before the hooks `onRequest` of the app.

use std::sync::OnceLock;

use aosabi_hooks::HookContext;
use aosabi_http::Request;
use aosabi_json::{
    JsObject,
    JsValue, //
};

/// The module the template file holds.
const TEMPLATE_MODULE: &str = include_str!("graphiql/template.html");

/// The title of a page whose options name none.
pub const DEFAULT_TITLE: &str = "Yoga GraphiQL";

/// The options of the page, the object `JSON.stringify` writes in the order the literal of the app names the keys.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GraphiqlOptions {
    /// `shouldPersistHeaders`.
    pub should_persist_headers: Option<bool>,
    /// `title`.
    pub title: Option<String>,
    /// `defaultQuery`.
    pub default_query: Option<String>,
}

impl GraphiqlOptions {
    /// Returns the JSON of the options, `JSON.stringify(opts ?? {})`.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut object = JsObject::new();
        if let Some(value) = self.should_persist_headers {
            object.insert("shouldPersistHeaders", JsValue::Bool(value));
        }
        if let Some(title) = &self.title {
            object.insert("title", JsValue::from(title.as_str()));
        }
        if let Some(query) = &self.default_query {
            object.insert("defaultQuery", JsValue::from(query.as_str()));
        }
        aosabi_json::to_string(&JsValue::Object(object)).unwrap_or_default()
    }
}

/// The factory of the options of a request, which answers `false` as `None`.
pub type GraphiqlOptionsFactory =
    std::rc::Rc<dyn Fn(&Request, &HookContext) -> Option<GraphiqlOptions>>;

/// The option `graphiql`.
#[derive(Clone, Default)]
pub enum Graphiql {
    /// No option, the page with the options `{}`.
    #[default]
    Default,
    /// `graphiql: false`, which leaves the page out of the function.
    Off,
    /// An options object.
    Options(GraphiqlOptions),
    /// A factory of the options of a request.
    Factory(GraphiqlOptionsFactory),
}

/// Returns the factory of the options of a request under `graphiql`, none for `graphiql: false`.
///
/// Yoga installs `useGraphiQL` unless its option is `false`.
#[must_use]
pub fn options_factory(graphiql: &Graphiql) -> Option<GraphiqlOptionsFactory> {
    match graphiql {
        Graphiql::Off => None,
        Graphiql::Default => Some(std::rc::Rc::new(|_: &Request, _: &HookContext| {
            Some(GraphiqlOptions::default())
        })),
        Graphiql::Options(options) => {
            let options = options.clone();
            Some(std::rc::Rc::new(move |_: &Request, _: &HookContext| {
                Some(options.clone())
            }))
        }
        Graphiql::Factory(factory) => Some(std::rc::Rc::clone(factory)),
    }
}

/// Returns whether `request` asks for the page, `shouldRenderGraphiQL(request)`.
#[must_use]
pub fn should_render_graphiql(request: &Request) -> bool {
    request.method == "GET"
        && request
            .header("accept")
            .is_some_and(|accept| accept.contains("text/html"))
}

/// Returns the value of the string literal the template module exports.
///
/// The literal escapes a double quote and a backslash, and any other escape of
/// a JavaScript string literal the package might write is read as that literal
/// reads it.
fn template() -> &'static str {
    static TEMPLATE: OnceLock<String> = OnceLock::new();
    TEMPLATE.get_or_init(|| {
        let module = TEMPLATE_MODULE.trim_end();
        let literal = module
            .strip_prefix("export default \"")
            .and_then(|rest| rest.strip_suffix("\";"))
            .unwrap_or(module);
        unescape_literal(literal)
    })
}

/// Returns the value of the body of a double quoted string literal.
fn unescape_literal(literal: &str) -> String {
    let mut out = String::with_capacity(literal.len());
    let mut chars = literal.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('b') => out.push('\u{8}'),
            Some('f') => out.push('\u{C}'),
            Some('v') => out.push('\u{B}'),
            Some('0') => out.push('\0'),
            Some('x') => {
                let digits: String = chars.by_ref().take(2).collect();
                if let Some(c) = u32::from_str_radix(&digits, 16)
                    .ok()
                    .and_then(char::from_u32)
                {
                    out.push(c);
                }
            }
            Some('u') => {
                let digits: String = chars.by_ref().take(4).collect();
                if let Some(c) = u32::from_str_radix(&digits, 16)
                    .ok()
                    .and_then(char::from_u32)
                {
                    out.push(c);
                }
            }
            Some('\n') | None => {}
            Some(other) => out.push(other),
        }
    }
    out
}

/// Returns `text.replace(pattern, replacement)` for a string pattern, the first occurrence replaced.
///
/// The replacement expands `$$` to `$`, `$&` to the match, `` $` `` to the
/// text before it and `$'` to the text after it, and keeps every other `$` as
/// it is, because a string pattern has no capture.
#[must_use]
pub fn replace_first(text: &str, pattern: &str, replacement: &str) -> String {
    let Some(position) = text.find(pattern) else {
        return String::from(text);
    };
    let before = &text[..position];
    let after = &text[position + pattern.len()..];
    let mut substitution = String::with_capacity(replacement.len());
    let mut chars = replacement.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '$' {
            substitution.push(c);
            continue;
        }
        match chars.peek() {
            Some('$') => {
                chars.next();
                substitution.push('$');
            }
            Some('&') => {
                chars.next();
                substitution.push_str(pattern);
            }
            Some('`') => {
                chars.next();
                substitution.push_str(before);
            }
            Some('\'') => {
                chars.next();
                substitution.push_str(after);
            }
            _ => substitution.push('$'),
        }
    }
    format!("{before}{substitution}{after}")
}

/// Returns the page of `options`, `renderGraphiQL(opts)`.
#[must_use]
pub fn render_graphiql(options: &GraphiqlOptions) -> String {
    let title = options
        .title
        .as_deref()
        .filter(|title| !title.is_empty())
        .unwrap_or(DEFAULT_TITLE);
    let page = replace_first(template(), "__TITLE__", title);
    replace_first(&page, "__OPTS__", &options.to_json())
}
