// SPDX-License-Identifier: MIT
/*
 * aosabi-http: the context of one request, the object every handler of the
 * chain receives.
 *
 * Derived from hono 4.11.9, dist/context.js, MIT.
 */

//! The context of a request.
//!
//! The reference passes one object through every handler of the chain, and a
//! handler's future outlives the call that made it, so [`Context`] is a shared
//! handle over interior state. It holds the request, the prepared status and
//! headers, the response once one is set, the variables of `c.set` and the
//! typed slots the core keeps its request state in.
//!
//! The helpers `c.text`, `c.json`, `c.html` and `c.body` take the status and the
//! headers that `c.status` and `c.header` prepared, because Rust has no optional
//! argument: `c.status(403); c.text(t)` gives the same status, headers and body as
//! `c.text(t, 403)` of the reference, since every helper takes the prepared status
//! and headers when no argument names them (rule 8 of section HONO of the old page
//! of the plan). The helper `c.text` with nothing prepared answers
//! `new Response(text)`, the fast path whose `content-type` the adapter writes
//! with the space.
//!
//! A call of `c.header` that the platform refuses throws in the reference. It
//! returns nothing here, keeps the `TypeError`, and the next helper that makes
//! a response answers it as the thrown error, which reaches the same error
//! handler at the end of the same handler.

use core::{
    any::{
        Any,
        TypeId, //
    },
    cell::{
        Cell,
        RefCell, //
    },
    marker::PhantomData, //
};
use std::{
    borrow::Cow,
    rc::Rc, //
};

use aosabi_json::JsValue;

use crate::{
    fetch::{
        body::Body,
        headers::Headers,
        request::Request,
        response::{
            FetchError,
            HeadersInit,
            Response,
            ResponseInit, //
        }, //
    },
    helper::html::HtmlEscaped,
    request::RequestView,
    router::Params,
    utils::url::encode_uri, //
    web_app::RouteError,
};

/// The default `content-type` of `c.text`, with the space.
pub const TEXT_PLAIN: &str = "text/plain; charset=UTF-8";

/// A typed key of the variables of a context, the twin of a member of `interface Variables`.
pub struct Key<T> {
    /// The name of the variable.
    pub name: &'static str,
    /// The type of the variable.
    _type: PhantomData<fn() -> T>,
}

impl<T> Key<T> {
    /// Creates the key `name`.
    #[must_use]
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            _type: PhantomData,
        }
    }
}

impl<T> Clone for Key<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Key<T> {}

/// The variables of the environment a platform binds, which a Worker's env holds.
#[derive(Clone, Debug, Default)]
pub struct Vars(pub Vec<(String, String)>);

/// The environment of a request, `c.env`.
///
/// The Node adapter hands `fetch` the pair `{incoming, outgoing}`, a Worker hands
/// its bindings, and `getContext()` sets `process.env` on the Node
/// runtime.
#[derive(Clone)]
pub enum Env {
    /// The bindings of the platform, the adapter pair or a Worker's env.
    Bindings(Rc<dyn Any>),
    /// The environment of the process.
    Process,
}

impl Env {
    /// Returns the variable `name` of the environment.
    ///
    /// The process answers its variable, bindings that are [`Vars`] answer
    /// their entry, and any other bindings, the adapter pair among them, hold
    /// no variable.
    #[must_use]
    pub fn var(&self, name: &str) -> Option<String> {
        match self {
            Self::Process => std::env::var(name).ok(),
            Self::Bindings(bindings) => bindings.downcast_ref::<Vars>().and_then(|vars| {
                vars.0
                    .iter()
                    .find(|(key, _)| key == name)
                    .map(|(_, value)| value.clone())
            }),
        }
    }
}

impl core::fmt::Debug for Env {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Bindings(_) => f.write_str("Env::Bindings"),
            Self::Process => f.write_str("Env::Process"),
        }
    }
}

/// The execution context of a platform, `c.executionCtx`, which a Worker hands to `fetch`.
#[derive(Clone)]
pub struct ExecutionCtx(pub Rc<dyn Any>);

impl core::fmt::Debug for ExecutionCtx {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("ExecutionCtx")
    }
}

/// The second argument of a helper, a status or a response whose status and headers it takes.
#[derive(Clone, Debug, Default)]
pub enum Arg {
    /// No argument, which takes the prepared status.
    #[default]
    None,
    /// A status.
    Status(u16),
    /// A response or an init whose status and headers the helper takes.
    Init(ResponseInit),
}

/// The html of `c.html`, a string or the output of the `html` template.
#[derive(Clone, Debug)]
pub enum Html {
    /// A primitive string.
    Str(String),
    /// The output of the `html` template, a `String` object of the reference.
    Escaped(HtmlEscaped),
}

impl From<&str> for Html {
    fn from(html: &str) -> Self {
        Self::Str(String::from(html))
    }
}

impl From<String> for Html {
    fn from(html: String) -> Self {
        Self::Str(html)
    }
}

impl From<HtmlEscaped> for Html {
    fn from(html: HtmlEscaped) -> Self {
        Self::Escaped(html)
    }
}

/// A variable of `c.set`, its name and its value.
type Variable = (Cow<'static, str>, Rc<dyn Any>);

/// The state every handle of a context shares.
struct ContextInner {
    /// The view of the request.
    req: RequestView,
    /// The environment.
    env: RefCell<Env>,
    /// The execution context of the platform.
    execution_ctx: Option<ExecutionCtx>,
    /// Whether a response is set.
    finalized: Cell<bool>,
    /// The error the chain handed its error handler, `c.error`.
    error: RefCell<Option<Rc<dyn Any>>>,
    /// The prepared status.
    status: Cell<Option<u16>>,
    /// The response once one is set or read.
    res: RefCell<Option<Response>>,
    /// The prepared headers.
    prepared_headers: RefCell<Option<Headers>>,
    /// The variables of `c.set`, in insertion order, each under the name of
    /// a typed key or a name a host gives at run time.
    vars: RefCell<Vec<Variable>>,
    /// The typed slots of the crates above.
    extensions: RefCell<Vec<(TypeId, Rc<dyn Any>)>>,
    /// A refusal of `c.header` that the next helper answers.
    pending: RefCell<Option<FetchError>>,
}

/// The context of one request, a shared handle.
#[derive(Clone)]
pub struct Context(Rc<ContextInner>);

impl core::fmt::Debug for Context {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Context")
            .field("path", &self.0.req.path())
            .finish_non_exhaustive()
    }
}

impl Context {
    /// Creates the context of `request` routed at `path` with the matched routes `routes`.
    #[must_use]
    pub fn new(
        request: Rc<Request>,
        path: String,
        routes: Rc<[(crate::web_app::RouteRecord, Params)]>,
        env: Env,
        execution_ctx: Option<ExecutionCtx>,
    ) -> Self {
        Self(Rc::new(ContextInner {
            req: RequestView::new(request, path, routes),
            env: RefCell::new(env),
            execution_ctx,
            finalized: Cell::new(false),
            error: RefCell::new(None),
            status: Cell::new(None),
            res: RefCell::new(None),
            prepared_headers: RefCell::new(None),
            vars: RefCell::new(Vec::new()),
            extensions: RefCell::new(Vec::new()),
            pending: RefCell::new(None),
        }))
    }

    /// Returns the view of the request, `c.req`.
    #[must_use]
    pub fn req(&self) -> RequestView {
        self.0.req.clone()
    }

    /// Returns the environment, `c.env`.
    #[must_use]
    pub fn env(&self) -> Env {
        self.0.env.borrow().clone()
    }

    /// Sets the environment, `c.env = env`.
    pub fn set_env(&self, env: Env) {
        *self.0.env.borrow_mut() = env;
    }

    /// Returns the execution context, `c.executionCtx`, which throws in the reference when there is none.
    #[must_use]
    pub fn execution_ctx(&self) -> Option<ExecutionCtx> {
        self.0.execution_ctx.clone()
    }

    /// Returns whether a response is set, `c.finalized`.
    #[must_use]
    pub fn finalized(&self) -> bool {
        self.0.finalized.get()
    }

    /// Returns the error the chain handed its error handler, `c.error`.
    #[must_use]
    pub fn error(&self) -> Option<Rc<dyn Any>> {
        self.0.error.borrow().clone()
    }

    /// Sets `c.error`.
    pub fn set_error(&self, error: Rc<dyn Any>) {
        *self.0.error.borrow_mut() = Some(error);
    }

    /// Sets the variable `key` to `value`, `c.set(key, value)`.
    pub fn set<T: 'static>(&self, key: Key<T>, value: T) {
        self.put_var(Cow::Borrowed(key.name), Rc::new(value));
    }

    /// Returns the variable `key`, `c.get(key)`.
    #[must_use]
    pub fn get<T: Clone + 'static>(&self, key: Key<T>) -> Option<T> {
        self.var(key.name)?.downcast_ref::<T>().cloned()
    }

    /// Sets the variable `name` to `value`, `c.set(name, value)` of a host
    /// that names its variables at run time, the wrapper of the TypeScript
    /// option, so a variable it sets and one a typed key sets live in one
    /// store.
    pub fn set_var(&self, name: &str, value: Rc<dyn Any>) {
        self.put_var(Cow::Owned(String::from(name)), value);
    }

    /// Returns the variable `name` as the store holds it, `c.get(name)`,
    /// whatever key set it.
    #[must_use]
    pub fn var(&self, name: &str) -> Option<Rc<dyn Any>> {
        let vars = self.0.vars.borrow();
        vars.iter()
            .find(|(held, _)| held == name)
            .map(|(_, value)| Rc::clone(value))
    }

    /// Returns the names of the variables in the order they were first set, the
    /// keys of the `Map` whose entries `c.var` answers as an object.
    #[must_use]
    pub fn var_names(&self) -> Vec<String> {
        self.0
            .vars
            .borrow()
            .iter()
            .map(|(name, _)| String::from(name.as_ref()))
            .collect()
    }

    /// Puts `value` under `name`, in the place of a variable of that name.
    fn put_var(&self, name: Cow<'static, str>, value: Rc<dyn Any>) {
        let mut vars = self.0.vars.borrow_mut();
        match vars.iter_mut().find(|(held, _)| *held == name) {
            Some(entry) => entry.1 = value,
            None => vars.push((name, value)),
        }
    }

    /// Returns the typed slot of `T`.
    #[must_use]
    pub fn extension<T: 'static>(&self) -> Option<Rc<T>> {
        let extensions = self.0.extensions.borrow();
        let (_, value) = extensions.iter().find(|(id, _)| *id == TypeId::of::<T>())?;
        Rc::clone(value).downcast::<T>().ok()
    }

    /// Sets the typed slot of `T`.
    pub fn set_extension<T: 'static>(&self, value: Rc<T>) {
        let mut extensions = self.0.extensions.borrow_mut();
        let value: Rc<dyn Any> = value;
        match extensions
            .iter_mut()
            .find(|(id, _)| *id == TypeId::of::<T>())
        {
            Some(entry) => entry.1 = value,
            None => extensions.push((TypeId::of::<T>(), value)),
        }
    }

    /// Returns the response, `c.res`, which makes an empty one over the prepared headers when none is set.
    ///
    /// The response it makes is kept and does not finalize the context.
    #[must_use]
    pub fn res(&self) -> Response {
        let mut res = self.0.res.borrow_mut();
        if let Some(res) = &*res {
            return res.clone();
        }
        let headers = self
            .0
            .prepared_headers
            .borrow_mut()
            .get_or_insert_with(Headers::new)
            .clone();
        let made = Response::new(
            Body::Empty,
            Some(ResponseInit {
                headers: Some(HeadersInit::Fetch(headers)),
                ..ResponseInit::default()
            }),
        )
        .unwrap_or_else(|_| Response::empty(200));
        *res = Some(made.clone());
        made
    }

    /// Sets the response, `c.res = res`, which finalizes the context.
    ///
    /// Over a response set before, the new response is remade from its body and
    /// itself, and every header of the old one but `content-type` is set on it,
    /// the `set-cookie` values appended.
    pub fn set_res(&self, res: Response) {
        let old = self.0.res.borrow().clone();
        let res = match old {
            Some(old) => carry_headers(&old, res),
            None => res,
        };
        *self.0.res.borrow_mut() = Some(res);
        self.0.finalized.set(true);
    }

    /// Puts `res` in place as the response without the carry of the setter.
    pub(crate) fn put_res(&self, res: Response) {
        *self.0.res.borrow_mut() = Some(res);
    }

    /// Sets the header `name` to `value` on the prepared headers or on the response, `c.header(name, value)`.
    ///
    /// On a finalized context the response is remade from its body and itself
    /// first.
    pub fn header(&self, name: &str, value: &str) {
        self.change_header(|headers| headers.set(name, value).map_err(FetchError::from));
    }

    /// Appends `value` to the header `name`, `c.header(name, value, { append: true })`.
    pub fn header_append(&self, name: &str, value: &str) {
        self.change_header(|headers| headers.append(name, value).map_err(FetchError::from));
    }

    /// Removes the header `name`, `c.header(name, undefined)`.
    pub fn header_delete(&self, name: &str) {
        self.change_header(|headers| {
            headers.delete(name);
            Ok(())
        });
    }

    /// Runs `change` on the headers `c.header` changes, keeping a refusal for the next helper.
    fn change_header(&self, change: impl FnOnce(&mut Headers) -> Result<(), FetchError>) {
        if self.finalized() {
            let old = self.0.res.borrow().clone();
            if let Some(old) = old
                && let Ok(remade) = Response::from_response(old.body(), &old)
            {
                self.put_res(remade);
            }
        }
        let res = self.0.res.borrow().clone();
        let result = match res {
            Some(res) => res.with_headers(change),
            None => change(
                self.0
                    .prepared_headers
                    .borrow_mut()
                    .get_or_insert_with(Headers::new),
            ),
        };
        if let Err(error) = result {
            self.0.pending.borrow_mut().get_or_insert(error);
        }
    }

    /// Sets the status the next response takes, `c.status(status)`.
    pub fn status(&self, status: u16) {
        self.0.status.set(Some(status));
    }

    /// Returns `c.text(text)`.
    ///
    /// With nothing prepared and a context not finalized this is
    /// `new Response(text)`, the fast path, and otherwise a response with
    /// `text/plain; charset=UTF-8`.
    ///
    /// # Errors
    ///
    /// Returns the `TypeError` a refused `c.header` left.
    pub fn text<E: RouteError>(&self, text: impl Into<String>) -> Result<Response, E> {
        self.text_with(text, Arg::None)
    }

    /// Returns `c.text(text, arg)`.
    ///
    /// # Errors
    ///
    /// Returns the `TypeError` a refused `c.header` or header of `arg` left.
    pub fn text_with<E: RouteError>(
        &self,
        text: impl Into<String>,
        arg: Arg,
    ) -> Result<Response, E> {
        self.take_pending()?;
        let text: String = text.into();
        let prepared = self.0.prepared_headers.borrow().is_some();
        if !prepared
            && self.0.status.get().is_none()
            && matches!(arg, Arg::None)
            && !self.finalized()
        {
            return Ok(Response::from_text(text));
        }
        self.new_response_with(
            Body::Text(Rc::from(text)),
            arg,
            &[("Content-Type", TEXT_PLAIN)],
        )
    }

    /// Returns `c.json(value)`, the value written as `JSON.stringify` writes it.
    ///
    /// # Errors
    ///
    /// Returns the `TypeError` a refused `c.header` left.
    pub fn json<E: RouteError>(&self, value: &JsValue) -> Result<Response, E> {
        self.json_with(value, Arg::None)
    }

    /// Returns `c.json(value, arg)`.
    ///
    /// A value `JSON.stringify` answers `undefined` for gives no body.
    ///
    /// # Errors
    ///
    /// Returns the `TypeError` a refused `c.header` or header of `arg` left.
    pub fn json_with<E: RouteError>(&self, value: &JsValue, arg: Arg) -> Result<Response, E> {
        self.take_pending()?;
        let body = match aosabi_json::to_string(value) {
            Some(text) => Body::Text(Rc::from(text)),
            None => Body::Empty,
        };
        self.new_response_with(body, arg, &[("Content-Type", "application/json")])
    }

    /// Returns `c.html(html)`.
    ///
    /// A string is a string body, and the output of the `html` template is the
    /// `String` object of the reference, which the adapter keeps in no cache (A5
    /// of the plan's version 2 draft).
    ///
    /// # Errors
    ///
    /// Returns the `TypeError` a refused `c.header` left.
    pub fn html<E: RouteError>(&self, html: impl Into<Html>) -> Result<Response, E> {
        self.html_with(html, Arg::None)
    }

    /// Returns `c.html(html, arg)`.
    ///
    /// # Errors
    ///
    /// Returns the `TypeError` a refused `c.header` or header of `arg` left.
    pub fn html_with<E: RouteError>(&self, html: impl Into<Html>, arg: Arg) -> Result<Response, E> {
        self.take_pending()?;
        let body = match html.into() {
            Html::Str(text) => Body::Text(Rc::from(text)),
            Html::Escaped(escaped) => Body::Html(Rc::from(escaped.as_str())),
        };
        self.new_response_with(body, arg, &[("Content-Type", "text/html; charset=UTF-8")])
    }

    /// Returns `c.body(body)`.
    ///
    /// # Errors
    ///
    /// Returns the `TypeError` a refused `c.header` left.
    pub fn body<E: RouteError>(&self, body: Body) -> Result<Response, E> {
        self.take_pending()?;
        self.new_response_with(body, Arg::None, &[])
    }

    /// Returns `c.redirect(location)`, a `302` with the header `Location` and no body.
    ///
    /// A location that holds a code point above U+00FF is written as `encodeURI`
    /// writes it, and any other location as it is. The reference answers
    /// `status ?? 302`, so a status `c.status` prepared does not reach a redirect,
    /// and no fixture and no port asks for a redirect with another status.
    ///
    /// # Errors
    ///
    /// Returns the `TypeError` a refused `c.header` left, the header
    /// `Location` among them.
    pub fn redirect<E: RouteError>(&self, location: &str) -> Result<Response, E> {
        if location.chars().any(|c| u32::from(c) > 0xFF) {
            self.header("Location", &encode_uri(location));
        } else {
            self.header("Location", location);
        }
        self.take_pending()?;
        self.new_response_with(Body::Empty, Arg::Status(302), &[])
    }

    /// Returns `c.newResponse(body, res)`, a response of `body` with the status and the headers of `res`.
    ///
    /// The response also takes the substatus of `res`, which refines the status it
    /// takes, so the substatus of Yoga's answer outlives the copy the handler
    /// makes of it.
    ///
    /// # Errors
    ///
    /// Returns the `TypeError` a refused `c.header` left.
    pub fn new_response<E: RouteError>(&self, body: Body, res: &Response) -> Result<Response, E> {
        self.take_pending()?;
        let init = ResponseInit {
            status: Some(res.status()),
            status_text: None,
            headers: Some(HeadersInit::Fetch(res.headers())),
        };
        let made = self.new_response_with(body, Arg::Init(init), &[])?;
        made.set_substatus(res.substatus());
        Ok(made)
    }

    /// Returns the response of `#newResponse(data, arg, headers)`.
    ///
    /// The headers of the response set or prepared, those of `arg` set over them
    /// with `set-cookie` appended, then `headers` set, and the status of `arg`, or
    /// of an init `arg` names, or the prepared one.
    ///
    /// # Errors
    ///
    /// Returns the `TypeError` of a header the platform refuses, and the
    /// `RangeError` of a status the platform refuses where it builds the
    /// response at once.
    pub fn new_response_with<E: RouteError>(
        &self,
        data: Body,
        arg: Arg,
        headers: &[(&str, &str)],
    ) -> Result<Response, E> {
        let mut response_headers = match self.0.res.borrow().as_ref() {
            Some(res) => res.headers(),
            None => self.0.prepared_headers.borrow().clone().unwrap_or_default(),
        };
        let type_error = |error: FetchError| E::refusal(&error);
        let (status, arg_headers) = match arg {
            Arg::Status(status) => (Some(status), None),
            Arg::Init(init) => (init.status.or(self.0.status.get()), init.headers),
            Arg::None => (self.0.status.get(), None),
        };
        if let Some(arg_headers) = arg_headers {
            let arg_headers = match arg_headers {
                HeadersInit::Fetch(headers) => headers,
                HeadersInit::Plain(entries) => {
                    Headers::from_entries(&entries).map_err(|e| type_error(e.into()))?
                }
            };
            for (key, value) in arg_headers.entries() {
                let changed = if key == "set-cookie" {
                    response_headers.append(&key, &value)
                } else {
                    response_headers.set(&key, &value)
                };
                changed.map_err(|e| type_error(e.into()))?;
            }
        }
        for (key, value) in headers {
            response_headers
                .set(key, value)
                .map_err(|e| type_error(e.into()))?;
        }
        let init = ResponseInit {
            status,
            status_text: None,
            headers: Some(HeadersInit::Fetch(response_headers)),
        };
        Response::new(data, Some(init)).map_err(type_error)
    }

    /// Answers the refusal a `c.header` left, once, which every helper that
    /// makes a response answers first, and which a host that makes a
    /// response of `#newResponse` itself answers as well.
    ///
    /// # Errors
    ///
    /// Returns the `TypeError` a refused `c.header` left.
    pub fn take_pending<E: RouteError>(&self) -> Result<(), E> {
        match self.0.pending.borrow_mut().take() {
            Some(error) => Err(E::type_error(&error.message)),
            None => Ok(()),
        }
    }
}

/// Returns `res` remade from its body and itself, with every header of `old` but `content-type` set on it.
fn carry_headers(old: &Response, res: Response) -> Response {
    let body = res.body();
    let res = Response::from_response(body, &res).unwrap_or(res);
    let old_headers = old.headers();
    for (key, value) in old_headers.entries() {
        if key == "content-type" {
            continue;
        }
        if key == "set-cookie" {
            let cookies = old_headers.get_set_cookie();
            res.delete_header("set-cookie");
            for cookie in cookies {
                // A value of a `Headers` passes a `Headers` again.
                let _ = res.append_header("set-cookie", &cookie);
            }
        } else {
            // A value of a `Headers` passes a `Headers` again.
            let _ = res.set_header(&key, &value);
        }
    }
    res
}
