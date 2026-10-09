// SPDX-License-Identifier: MIT
/*
 * aosabi-sentry: the Sentry client of init, its scopes and its spans.
 *
 * Derived from @sentry/node 8.55.0, ., MIT.
 * Derived from @sentry/core 8.55.0, build/cjs, MIT.
 */

//! `init`, `Client`, `Scope`, `Span`, `capture_exception`, `with_scope` and
//! `start_span_manual`.
//!
//! The plugin reports through the hub of the SDK, which holds the client that
//! `Sentry.init` made and the current scope. The plugin calls no `init`, so it
//! sends an event only where the application initialized the SDK itself, and
//! with no client `captureException` mints its event id and returns it without
//! capturing.
//!
//! The hub lives in thread locals, since a request and its futures never
//! leave the one runtime thread. [`init`] sets the client, and the current
//! scope starts empty. A scope that [`with_scope`] forks is handed to its
//! callback, and a capture inside it goes through
//! [`Scope::capture_exception`], the method a scope of the SDK has as well,
//! so the fork never needs a borrow of the current scope while the callback
//! runs.
//!
//! A span records nothing. With no client of its own a span of the SDK changes
//! no byte, so [`start_span_manual`] runs its callback with a span whose
//! setters do nothing, inside the scope it forks.

use core::{
    cell::RefCell,
    mem, //
};
use std::rc::Rc;

use aosabi_core::random;
use aosabi_json::{
    JsObject,
    JsValue, //
};

use crate::{
    dsn::{
        Dsn,
        DsnError, //
    },
    envelope::{
        Breadcrumb,
        Envelope,
        Event,
        Exception,
        PLATFORM,
        event_id,
        now_seconds, //
    },
    integrations,
};

/// The environment of an event whose client names none, `DEFAULT_ENVIRONMENT`.
const DEFAULT_ENVIRONMENT: &str = "production";

/// The most breadcrumbs a scope keeps, `DEFAULT_MAX_BREADCRUMBS`.
const MAX_BREADCRUMBS: usize = 100;

/// The options of a client, the keys of `Sentry.init` and of the middleware
/// this port carries.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClientOptions {
    /// `dsn`, the project the events go to. A client with none sends nothing.
    pub dsn: Option<String>,
    /// `environment`, `production` when absent.
    pub environment: Option<String>,
    /// `release`.
    pub release: Option<String>,
    /// `sampleRate`, the share of the errors that are sent, all when absent.
    pub sample_rate: Option<f64>,
}

/// What a capture reports, an error or a message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Report {
    /// The error of `captureException`.
    Exception(Exception),
    /// The message of `captureMessage` with its level, `info` where the call
    /// names none, the event of `eventFromMessage`.
    Message {
        /// The message.
        message: String,
        /// The level.
        level: String,
    },
}

/// A client, the part of `BaseClient` the two halves of the plugin share.
#[derive(Debug)]
pub struct Client {
    /// The DSN, none when the options name no valid one.
    dsn: Option<Dsn>,
    /// The options.
    options: ClientOptions,
}

impl Client {
    /// Returns the client of `options`.
    ///
    /// A DSN that is empty makes no client that sends, as the constructor of
    /// the SDK skips a falsy `dsn`, and a DSN the expression of the SDK does
    /// not match prints `Invalid Sentry Dsn: <text>` to standard error, as
    /// `dsnFromString` prints it through the console.
    #[must_use]
    pub fn new(options: ClientOptions) -> Self {
        let dsn = match options.dsn.as_deref() {
            None | Some("") => None,
            Some(text) => match Dsn::parse(text) {
                Ok(dsn) => Some(dsn),
                Err(DsnError::Syntax) => {
                    eprintln!("Invalid Sentry Dsn: {text}");
                    None
                }
                Err(DsnError::Invalid) => None,
            },
        };
        Self { dsn, options }
    }

    /// Returns whether the client sends events, which needs a DSN.
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.dsn.is_some()
    }

    /// Sends the event of `report` with the id `event_id` under `scope`.
    ///
    /// The event takes the data of `scope` and then the options of the
    /// client, as `prepareEvent` applies them, and `request` is the request
    /// data of the middleware. A client with no DSN sends nothing, and an
    /// error the sample rate leaves out is dropped, `Math.random() >
    /// sampleRate`.
    pub(crate) fn send(
        &self,
        event_id: String,
        report: Report,
        scope: &Scope,
        request: Option<JsObject>,
    ) {
        let Some(dsn) = &self.dsn else {
            return;
        };
        if let Some(rate) = self.options.sample_rate
            && sample() > rate
        {
            return;
        }
        let (exception, message, level) = match report {
            Report::Exception(exception) => (Some(exception), None, None),
            Report::Message { message, level } => (None, Some(message), Some(level)),
        };
        let mut event = Event {
            event_id,
            timestamp: now_seconds(),
            platform: PLATFORM,
            level,
            exception,
            message,
            request,
            ..Event::default()
        };
        scope.apply_to_event(&mut event);
        event.environment = Some(
            self.options
                .environment
                .clone()
                .unwrap_or_else(|| String::from(DEFAULT_ENVIRONMENT)),
        );
        event.release.clone_from(&self.options.release);
        Envelope::new(event).send(dsn);
    }
}

/// Returns a number in `[0, 1)` from the randomness capability, `Math.random()`.
///
/// With no source of random bytes the answer is 0, which keeps the error.
fn sample() -> f64 {
    let mut bytes = [0_u8; 4];
    if random::fill(&mut bytes).is_err() {
        return 0.0;
    }
    f64::from(u32::from_le_bytes(bytes)) / 4_294_967_296.0
}

/// The data of a capture that is no part of the scope, the capture context
/// of a hint, `{fingerprint, contexts}`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CaptureContext {
    /// `fingerprint`, which replaces the one of the scope.
    pub fingerprint: Option<Vec<String>>,
    /// `contexts`, which are merged over those of the scope.
    pub contexts: JsObject,
}

/// A scope, the data an event takes from where it is captured.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scope {
    /// `transactionName`.
    transaction_name: Option<String>,
    /// `tags`.
    tags: JsObject,
    /// `extra`.
    extra: JsObject,
    /// `contexts`.
    contexts: JsObject,
    /// `user`.
    user: Option<JsObject>,
    /// `level`.
    level: Option<String>,
    /// `fingerprint`.
    fingerprint: Vec<String>,
    /// `breadcrumbs`, the last [`MAX_BREADCRUMBS`].
    breadcrumbs: Vec<Breadcrumb>,
}

impl Scope {
    /// Returns an empty scope.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the name of the transaction, `scope.setTransactionName(name)`.
    pub fn set_transaction_name(&mut self, name: impl Into<String>) {
        self.transaction_name = Some(name.into());
    }

    /// Sets the tag `key`, `scope.setTag(key, value)`.
    pub fn set_tag(&mut self, key: &str, value: impl Into<JsValue>) {
        self.tags.insert(key, value.into());
    }

    /// Sets every tag of `tags`, `scope.setTags(tags)`.
    pub fn set_tags(&mut self, tags: &JsObject) {
        for (key, value) in tags.iter() {
            self.tags.insert(key.clone(), value.clone());
        }
    }

    /// Sets the extra `key`, `scope.setExtra(key, value)`.
    pub fn set_extra(&mut self, key: &str, value: impl Into<JsValue>) {
        self.extra.insert(key, value.into());
    }

    /// Sets every extra of `extras`, `scope.setExtras(extras)`.
    pub fn set_extras(&mut self, extras: &JsObject) {
        for (key, value) in extras.iter() {
            self.extra.insert(key.clone(), value.clone());
        }
    }

    /// Sets the context `key`, or removes it for `None`, `scope.setContext(key, context)`.
    pub fn set_context(&mut self, key: &str, context: Option<JsObject>) {
        match context {
            Some(context) => {
                self.contexts.insert(key, JsValue::Object(context));
            }
            None => {
                self.contexts.remove(key);
            }
        }
    }

    /// Sets the user, `scope.setUser(user)`.
    pub fn set_user(&mut self, user: Option<JsObject>) {
        self.user = user;
    }

    /// Sets the level of an event, `scope.setLevel(level)`.
    pub fn set_level(&mut self, level: impl Into<String>) {
        self.level = Some(level.into());
    }

    /// Sets the fingerprint, `scope.setFingerprint(fingerprint)`.
    pub fn set_fingerprint(&mut self, fingerprint: Vec<String>) {
        self.fingerprint = fingerprint;
    }

    /// Adds `breadcrumb`, `scope.addBreadcrumb(breadcrumb)`.
    ///
    /// A crumb with no timestamp takes the time it is added, and the scope
    /// keeps the last 100 crumbs.
    pub fn add_breadcrumb(&mut self, mut breadcrumb: Breadcrumb) {
        if breadcrumb.timestamp.is_none() {
            breadcrumb.timestamp = now_seconds();
        }
        self.breadcrumbs.push(breadcrumb);
        if self.breadcrumbs.len() > MAX_BREADCRUMBS {
            let excess = self.breadcrumbs.len() - MAX_BREADCRUMBS;
            self.breadcrumbs.drain(..excess);
        }
    }

    /// Captures `exception` under this scope and `hint`, `scope.captureException(exception, hint)`.
    ///
    /// The event id is minted first and answered whether or not a client
    /// exists, and only the client of [`init`] sends. The answer is `None`
    /// when the process has no source of random bytes, so no id exists to
    /// write.
    ///
    /// The event takes the isolation scope in place first and this scope over
    /// it, and the request data the isolation scope records, as `prepareEvent`
    /// merges the scopes and the integration `RequestData` writes the request.
    #[must_use]
    pub fn capture_exception(&self, exception: Exception, hint: &CaptureContext) -> Option<String> {
        let id = event_id().ok()?;
        if let Some(client) = client() {
            let mut scope = self.clone();
            scope.update(hint);
            let (mut merged, request) = integrations::isolation_scope();
            merged.merge(&scope);
            client.send(id.clone(), Report::Exception(exception), &merged, request);
        }
        Some(id)
    }

    /// Merges `over` into this scope, `mergeScopeData(data, over)`.
    ///
    /// The tags, the extras, the user and the contexts of `over` win key by
    /// key, its level and its transaction name win where it has one, and its
    /// breadcrumbs and its fingerprint follow those of this scope.
    pub(crate) fn merge(&mut self, over: &Self) {
        self.extra = merge(&self.extra, &over.extra);
        self.tags = merge(&self.tags, &over.tags);
        self.user = match (&self.user, &over.user) {
            (Some(under), Some(over)) => Some(merge(under, over)),
            (under, over) => over.clone().or_else(|| under.clone()),
        };
        self.contexts = merge(&self.contexts, &over.contexts);
        if over.level.is_some() {
            self.level.clone_from(&over.level);
        }
        if over.transaction_name.is_some() {
            self.transaction_name.clone_from(&over.transaction_name);
        }
        self.breadcrumbs.extend(over.breadcrumbs.iter().cloned());
        self.fingerprint.extend(over.fingerprint.iter().cloned());
    }

    /// Merges the capture context `hint` into the scope, `scope.update(captureContext)`.
    fn update(&mut self, hint: &CaptureContext) {
        for (key, value) in hint.contexts.iter() {
            self.contexts.insert(key.clone(), value.clone());
        }
        if let Some(fingerprint) = &hint.fingerprint {
            self.fingerprint.clone_from(fingerprint);
        }
    }

    /// Writes the data of the scope into `event`, `applyScopeDataToEvent`.
    ///
    /// The tags, the extras and the contexts of the event win over those of
    /// the scope, the level and the name of the transaction of the scope win
    /// over those of the event, the level of a message among them, the
    /// fingerprint of the scope follows that of the event, and the breadcrumbs
    /// of the scope follow those of the event.
    pub(crate) fn apply_to_event(&self, event: &mut Event) {
        event.extra = merge(&self.extra, &event.extra);
        event.tags = merge(&self.tags, &event.tags);
        event.contexts = merge(&self.contexts, &event.contexts);
        if event.user.is_none() {
            event.user.clone_from(&self.user);
        }
        if self.level.is_some() {
            event.level.clone_from(&self.level);
        }
        if self.transaction_name.is_some() {
            event.transaction.clone_from(&self.transaction_name);
        }
        event.fingerprint.extend(self.fingerprint.iter().cloned());
        event.breadcrumbs.extend(self.breadcrumbs.iter().cloned());
    }
}

/// Returns `{...under, ...over}`.
fn merge(under: &JsObject, over: &JsObject) -> JsObject {
    let mut merged = under.clone();
    for (key, value) in over.iter() {
        merged.insert(key.clone(), value.clone());
    }
    merged
}

thread_local! {
    /// The client of [`init`].
    static CLIENT: RefCell<Option<Rc<Client>>> = const { RefCell::new(None) };
    /// The current scope.
    static CURRENT: RefCell<Scope> = RefCell::new(Scope::new());
}

/// Initializes the SDK with `options`, `Sentry.init(options)`.
///
/// An application calls it out of band, at the top of its configuration, as
/// the applications of the reference call `Sentry.init`. A second call
/// replaces the client of the first. The call installs the integrations of the
/// module `integrations` of this crate, so the lines of the console become
/// breadcrumbs and an event of a request carries its request, as the default
/// integrations of `@sentry/node` do.
pub fn init(options: ClientOptions) {
    let client = Rc::new(Client::new(options));
    // A thread that is being torn down keeps no client.
    let _ = CLIENT.try_with(|slot| *slot.borrow_mut() = Some(client));
    integrations::install();
}

/// Returns the client of [`init`], when the application made one.
pub(crate) fn client() -> Option<Rc<Client>> {
    CLIENT.try_with(|slot| slot.borrow().clone()).ok().flatten()
}

/// Runs `f` over the current scope, `f(Sentry.getCurrentScope())`.
///
/// The scope is taken out of its slot while `f` runs, so a call of the hub
/// inside `f` meets an empty scope and no borrow it would conflict with.
pub fn with_current_scope<R>(f: impl FnOnce(&mut Scope) -> R) -> R {
    let mut scope = CURRENT
        .try_with(|current| mem::take(&mut *current.borrow_mut()))
        .unwrap_or_default();
    let answer = f(&mut scope);
    let _ = CURRENT.try_with(|current| *current.borrow_mut() = scope);
    answer
}

/// Runs `f` over a fork of the current scope, `Sentry.withScope(f)`.
///
/// The fork lives as long as `f`, so what `f` sets reaches the captures it
/// makes through [`Scope::capture_exception`] and no capture after it.
pub fn with_scope<R>(f: impl FnOnce(&mut Scope) -> R) -> R {
    let mut fork = CURRENT
        .try_with(|current| current.borrow().clone())
        .unwrap_or_default();
    f(&mut fork)
}

/// Captures `exception` under the current scope, `Sentry.captureException(exception, hint)`.
#[must_use]
pub fn capture_exception(exception: Exception, hint: &CaptureContext) -> Option<String> {
    let scope = CURRENT
        .try_with(|current| current.borrow().clone())
        .unwrap_or_default();
    scope.capture_exception(exception, hint)
}

/// The options of a span, `{op, name, attributes}`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SpanOptions {
    /// `op`.
    pub op: String,
    /// `name`.
    pub name: String,
    /// `attributes`.
    pub attributes: JsObject,
}

/// A span that records nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Span;

impl Span {
    /// Renames the span, `span.updateName(name)`, which records nothing.
    pub fn update_name(&mut self, _name: &str) {}

    /// Sets an attribute, `span.setAttribute(key, value)`, which records nothing.
    pub fn set_attribute(&mut self, _key: &str, _value: &JsValue) {}

    /// Ends the span, `span.end()`, which records nothing.
    pub fn end(&mut self) {}
}

/// Runs `f` with a span of `options`, `Sentry.startSpanManual(options, f)`.
///
/// The SDK runs the callback inside `withScope`, and the span sets nothing on
/// that fork, so the callback forks the current scope again where it needs
/// one, which holds the same data.
pub fn start_span_manual<R>(_options: &SpanOptions, f: impl FnOnce(&mut Span) -> R) -> R {
    f(&mut Span)
}
