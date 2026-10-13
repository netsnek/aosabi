// SPDX-License-Identifier: MIT
/*
 * aosabi-js sentry: the Sentry of an app of the TypeScript option over
 * aosabi-sentry, the plugin useSentry, Sentry.init of aosabi/sentry/node and
 * the client of a request.
 *
 * No analogue file in the reference.
 */

//! The Sentry of an app of the TypeScript option.
//!
//! The marker `useSentry` of the entry `aosabi` becomes the plugin of
//! `aosabi-sentry` over its options, [`plugin`]: the values of its keys cross
//! as JSON data, and its option `skip` as a function of the table of the
//! boot, which the engine calls with the payload of `onExecute`, whose member
//! `args`, the view of the arguments of the execution of `crate::nodes`, the
//! wrapper hands the function of the app. Any other key whose value is a
//! function or holds one fails the boot with the waiting it names, since the
//! engine runs no such function. `Sentry.init` of `aosabi/sentry/node`
//! crosses in the boot record, [`init`], so the hub of the engine holds a
//! client of the DSN, the environment, the release and the sample rate the
//! client of the reference holds after `Sentry.init`.
//!
//! The client of a request is the `Toucan` the middleware of `@hono/sentry`
//! sets under the key `sentry`. That middleware runs in the hono of the host,
//! `aosabi/pylon`, in the place of the plugin in the plugin chain of the
//! base pipeline, and asks the engine for the client of the request it opened,
//! the act `START` of the op `SENTRY` of the export `ctx`, which builds it as
//! the middleware builds it from the options of the marker and the head of the
//! request, [`aosabi_sentry::middleware::client`], and keeps it until the
//! request ends. The middleware sets the facade of `aosabi-ts/lib/toucan.js`
//! over that client under the key `sentry` of the `c` of hono, and each member
//! of the facade calls the op `SENTRY` of the export `ctx`, [`call`]: a setter
//! of the scope, a capture of an error or of a message, or a fork of
//! `withScope`, a copy of the scope the engine keeps until the request ends,
//! as `withScope` of Toucan hands its callback a clone with a copy of the
//! scope data, which lives as long as the callback holds it, and once the rest
//! of the chain has run the act `CAPTURE` reports the error hono set on `c`,
//! as the middleware does. The base pipeline of the canary sets that
//! client for every request whose environment names a DSN, which its
//! middleware of the host asks for through the same act, the client of the
//! canary.

#![forbid(unsafe_code)]

use core::cell::RefCell;
use std::rc::Rc;

use aosabi_core::{
    Context,
    IntoPlugin as _,
    PluginSlot,
    Thrown, //
};
use aosabi_execute::value::{
    Completion,
    Value, //
};
use aosabi_json::{
    JsObject,
    JsValue,
    number_to_string, //
};
use aosabi_lang::js_string::to_string_lossy;
use aosabi_sentry::{
    ClientOptions,
    EventIdKey,
    Scope,
    SentryPluginOptions,
    envelope::{
        Breadcrumb,
        Exception, //
    },
    middleware::{
        client,
        exception, //
    },
    toucan::{
        Toucan,
        message_of, //
    },
    use_sentry, //
};

use crate::{
    host::{
        self,
        Reader, //
    },
    request::{
        self,
        BOOT, //
    },
    value,
};

/// The op of the export `ctx` a member of the client of Sentry calls, after
/// the op of the claims of `crate::zitadel`.
pub const OP: u8 = 34;

/// The calls of the op [`OP`], the byte after it.
mod act {
    /// Makes a fork of a scope and answers its number.
    pub const FORK: u8 = 0;
    /// Calls a setter of a scope, its name and its arguments as JSON data.
    pub const SET: u8 = 2;
    /// Captures a message with its level.
    pub const MESSAGE: u8 = 3;
    /// Captures an error by its name and its message.
    pub const EXCEPTION: u8 = 4;
    /// Builds the client of the request, after the number of the marker
    /// `useSentry` among those of the configuration, or `u32::MAX` for the
    /// client of the base pipeline of the canary, and answers whether the
    /// request holds a client.
    pub const START: u8 = 5;
    /// Reports the error hono set on `c`, its name and its message, as the
    /// middleware of `@hono/sentry` reports it once the rest of the chain has
    /// run.
    pub const CAPTURE: u8 = 6;
}

/// The number of the act [`act::START`] that names the client of the base
/// pipeline of the canary.
const CANARY: u32 = u32::MAX;

/// The message of an option of `useSentry` that is a function or holds one
/// the engine runs none of, after the name of the option.
const WAITS: &str = "of useSentry holds a function, which aosabi runs none of until a fixture passes \
                     one, and the options skip, dsn, environment, release, sampleRate, eventIdKey and \
                     the four flags cross";

/// The message of a call of the client of Sentry of a request that holds none.
const NO_CLIENT: &str =
    "the request holds no client of Sentry, which @hono/sentry sets where a DSN is set";

/// A fork of a scope, the scope of the clone `withScope` hands its callback.
struct Fork {
    /// The request.
    request: u32,
    /// The number of the fork, from 1, 0 being the scope of the request.
    number: u32,
    /// The scope.
    scope: Scope,
}

// An `unsafe` cell would skip each borrow flag.
thread_local! {
    /// The forks of the requests in flight.
    static FORKS: RefCell<Vec<Fork>> = const { RefCell::new(Vec::new()) };
    /// The clients of the requests in flight, each with its request.
    static CLIENTS: RefCell<Vec<(u32, Rc<Toucan>)>> = const { RefCell::new(Vec::new()) };
    /// The options of the middleware of each marker `useSentry` of the
    /// configuration, in their order.
    static MARKERS: RefCell<Vec<ClientOptions>> = const { RefCell::new(Vec::new()) };
}

/// Returns the plugin of the marker `useSentry` with the JSON data of its
/// options `options` and the functions `functions` of its options, each its
/// key and its handle in the table of the boot, 0 for a key whose value is a
/// function or holds one the engine runs none of.
///
/// Every other key of the middleware takes no effect here, and one that holds
/// a function fails the boot, section 14 of `docs/ts1-surface.md`.
///
/// # Errors
///
/// Returns the message of an option that holds a function the engine runs
/// none of.
pub fn plugin(options: &JsValue, functions: &[(String, u32)]) -> Result<PluginSlot, String> {
    let empty = JsObject::new();
    let object = match options {
        JsValue::Object(object) => object,
        _ => &empty,
    };
    let flag = |key: &str| match object.get(key) {
        Some(JsValue::Bool(value)) => Some(*value),
        _ => None,
    };
    let text = |key: &str| match object.get(key) {
        Some(JsValue::String(value)) => Some(to_string_lossy(value).into_owned()),
        _ => None,
    };
    let mut sentry = SentryPluginOptions {
        start_transaction: flag("startTransaction"),
        rename_transaction: flag("renameTransaction"),
        include_raw_result: flag("includeRawResult"),
        include_execute_variables: flag("includeExecuteVariables"),
        event_id_key: match object.get("eventIdKey") {
            None | Some(JsValue::Undefined) => None,
            Some(JsValue::Null) => Some(EventIdKey::Null),
            Some(other) => Some(EventIdKey::Key(text_of(other))),
        },
        dsn: text("dsn"),
        environment: text("environment"),
        release: text("release"),
        sample_rate: match object.get("sampleRate") {
            Some(JsValue::Number(rate)) => Some(*rate),
            _ => None,
        },
        ..SentryPluginOptions::default()
    };
    for (key, handle) in functions {
        match (key.as_str(), *handle) {
            ("skip", handle) if handle != 0 => sentry.skip = Some(skip(handle)),
            (key, _) => return Err(format!("the option {key} {WAITS}")),
        }
    }
    MARKERS.with_borrow_mut(|markers| {
        markers.push(ClientOptions {
            dsn: sentry.dsn.clone(),
            environment: sentry.environment.clone(),
            release: sentry.release.clone(),
            sample_rate: sentry.sample_rate,
        });
    });
    Ok(PluginSlot::new(use_sentry(sentry).into_plugin()))
}

/// Returns the option `skip` over the function `handle` of the table of the
/// boot, which the engine calls with the payload of `onExecute` and which
/// answers whether the function of the app answered a true value for the
/// arguments of the execution.
///
/// A throw of the function of the app skips nothing, where the hook of the
/// reference throws, since the option of `aosabi-sentry` answers a flag.
fn skip(handle: u32) -> aosabi_sentry::use_sentry::ArgsFn<bool> {
    Box::new(move |view| {
        let request = request::current();
        let args = crate::nodes::note_args(request, view.execution, view.context);
        let completion = value::call_hook(BOOT, handle, request, 1, |out| {
            out.push(host::EXECUTE);
            host::push_word(out, args);
        });
        matches!(completion, Completion::Normal(Value::Bool(true)))
    })
}

/// Initializes the hub of the engine with the JSON data `options` of the last
/// `Sentry.init` the app made, `aosabi_sentry::init`.
// Out of line, so a boot compiles the options of `Sentry.init` where the app made one.
#[inline(never)]
pub fn init(options: &JsValue) {
    let JsValue::Object(object) = options else {
        aosabi_sentry::init(ClientOptions::default());
        return;
    };
    let text = |key: &str| match object.get(key) {
        Some(JsValue::String(value)) => Some(to_string_lossy(value).into_owned()),
        _ => None,
    };
    aosabi_sentry::init(ClientOptions {
        dsn: text("dsn"),
        environment: text("environment"),
        release: text("release"),
        sample_rate: match object.get("sampleRate") {
            Some(JsValue::Number(rate)) => Some(*rate),
            _ => None,
        },
    });
}

/// Answers the call of the op [`OP`] of `request` over the context `c`, the
/// record at the cursor of `reader`, or `None` for a broken record.
///
/// A fork answers its number and a capture its event id, and a setter
/// answers `undefined`.
// Out of line, so a request compiles the calls of the client of Sentry where it makes one.
#[inline(never)]
pub fn call(request: u32, c: &Context, reader: &mut Reader<'_>) -> Option<Result<Value, Thrown>> {
    let act = reader.byte()?;
    let number = reader.word()?;
    if act == act::START {
        return Some(Ok(Value::Bool(start(request, c, number))));
    }
    let Some(toucan) = client_of(request) else {
        return Some(Err(Thrown::error(NO_CLIENT)));
    };
    if act == act::CAPTURE {
        let ty = String::from(reader.text()?);
        let value = String::from(reader.text()?);
        toucan.capture_exception(exception(ty, value));
        return Some(Ok(Value::Undefined));
    }
    Some(Ok(match act {
        act::FORK => {
            let scope =
                with_scope(request, number, |scope| scope.clone()).unwrap_or_else(|| toucan.fork());
            Value::Number(f64::from(fork(request, scope)))
        }
        act::SET => {
            let name = String::from(reader.text()?);
            let args = host::read_json(reader)?;
            let apply = |scope: &mut Scope| set(scope, &name, &args);
            if number == 0 {
                toucan.configure_scope(apply);
            } else {
                with_scope(request, number, apply)?;
            }
            Value::Undefined
        }
        act::MESSAGE | act::EXCEPTION => {
            let report = if act == act::MESSAGE {
                let message = String::from(reader.text()?);
                let level = match reader.byte()? {
                    1 => Some(String::from(reader.text()?)),
                    _ => None,
                };
                message_of(&message, level.as_deref())
            } else {
                let ty = String::from(reader.text()?);
                let value = String::from(reader.text()?);
                aosabi_sentry::Report::Exception(Exception { ty, value })
            };
            let scope = if number == 0 {
                toucan.fork()
            } else {
                with_scope(request, number, |scope| scope.clone())?
            };
            toucan
                .capture_in(&scope, report)
                .map_or(Value::Undefined, |id| Value::from_js(&JsValue::from(id)))
        }
        _ => return None,
    }))
}

/// Applies the setter `name` of a scope of Sentry with the arguments `args`,
/// an array of JSON data, to `scope`, as the setters of the scope of
/// `@sentry/core` 8.9.2 write the values of the types an app hands them: a
/// user, the tags and the extras and one of each, the level, the fingerprint,
/// a context, the name of the transaction and a breadcrumb with its category,
/// its message and its level.
///
/// Where `scope.js` keeps the value of `setLevel`, `setTransactionName` and
/// `setFingerprint` as given, the scope of the port keeps the text of the
/// level and of the name, `String(value)`, and the texts of the parts of a
/// fingerprint, so a value of another type than a string reaches the event as
/// its text. A fingerprint that is no array and tags or extras that are no
/// object leave the scope as it was, a user that is no object clears the user
/// and a context that is no object removes the context, where `scope.js`
/// keeps each as given or spreads it, and a setter the scope of the port
/// holds no field for changes nothing, each a difference named in section 14
/// of `docs/ts1-surface.md`.
fn set(scope: &mut Scope, name: &str, args: &JsValue) {
    let JsValue::Array(args) = args else {
        return;
    };
    let first = args.first().unwrap_or(&JsValue::Undefined);
    let text = text_of;
    match name {
        "setUser" => scope.set_user(match first {
            JsValue::Object(user) => Some(user.clone()),
            _ => None,
        }),
        "setTags" | "setExtras" => {
            if let JsValue::Object(object) = first {
                if name == "setTags" {
                    scope.set_tags(object);
                } else {
                    scope.set_extras(object);
                }
            }
        }
        "setTag" => scope.set_tag(
            &text(first),
            args.get(1).cloned().unwrap_or(JsValue::Undefined),
        ),
        "setExtra" => scope.set_extra(
            &text(first),
            args.get(1).cloned().unwrap_or(JsValue::Undefined),
        ),
        "setLevel" => scope.set_level(text(first)),
        "setTransactionName" => scope.set_transaction_name(text(first)),
        "setFingerprint" => {
            if let JsValue::Array(parts) = first {
                scope.set_fingerprint(parts.iter().map(text).collect());
            }
        }
        "setContext" => scope.set_context(
            &text(first),
            match args.get(1) {
                Some(JsValue::Object(context)) => Some(context.clone()),
                _ => None,
            },
        ),
        "addBreadcrumb" => {
            if let JsValue::Object(crumb) = first {
                let member = |key: &str| crumb.get(key).map(text);
                scope.add_breadcrumb(Breadcrumb {
                    category: member("category"),
                    message: member("message"),
                    level: member("level"),
                    timestamp: match crumb.get("timestamp") {
                        Some(JsValue::Number(time)) => Some(*time),
                        _ => None,
                    },
                });
            }
        }
        _ => {}
    }
}

/// Returns `String(value)` of the JSON data `value`, as a setter of the
/// scope takes the text of a key or of a level: an array as the texts of its
/// items joined by commas, `null` and `undefined` among them as empty texts,
/// as `Array.prototype.join` of ECMA-262 writes it, and an object as
/// `[object Object]`.
fn text_of(value: &JsValue) -> String {
    match value {
        JsValue::Undefined => String::from("undefined"),
        JsValue::Null => String::from("null"),
        JsValue::Bool(flag) => flag.to_string(),
        JsValue::Number(number) => number_to_string(*number),
        JsValue::String(text) => to_string_lossy(text).into_owned(),
        JsValue::Array(items) => items
            .iter()
            .map(|item| match item {
                JsValue::Undefined | JsValue::Null => String::new(),
                other => text_of(other),
            })
            .collect::<Vec<_>>()
            .join(","),
        JsValue::Object(_) | JsValue::Opaque(_) => String::from("[object Object]"),
    }
}

/// Keeps `scope` as a fork of `request` and answers its number.
fn fork(request: u32, scope: Scope) -> u32 {
    FORKS.with_borrow_mut(|forks| {
        let number = forks
            .iter()
            .filter(|fork| fork.request == request)
            .map(|fork| fork.number)
            .max()
            .unwrap_or(0)
            + 1;
        forks.push(Fork {
            request,
            number,
            scope,
        });
        number
    })
}

/// Runs `body` over the fork `number` of `request`, or answers `None` where
/// the request holds no such fork.
fn with_scope<R>(request: u32, number: u32, body: impl FnOnce(&mut Scope) -> R) -> Option<R> {
    FORKS.with_borrow_mut(|forks| {
        forks
            .iter_mut()
            .find(|fork| fork.request == request && fork.number == number)
            .map(|fork| body(&mut fork.scope))
    })
}

/// Builds the client of `request` over the context `c` of its head, the act
/// [`act::START`], for the marker `useSentry` of the number `marker` or for
/// the base pipeline of the canary, and answers whether the request holds
/// a client.
///
/// The client of a marker is the one the middleware of `@hono/sentry` builds
/// with the DSN, the environment, the release and the sample rate of the
/// options of the marker, and the client of the canary the one it builds with
/// no option where the environment of the request names a DSN, `SENTRY_DSN` or
/// `NEXT_PUBLIC_SENTRY_DSN`, as the canary goes on with no client otherwise.
/// The environment of a request is the variables of the process of the host
/// the boot took, the string bindings of a Worker among them. A request holds
/// one client, the first one built.
fn start(request: u32, c: &Context, marker: u32) -> bool {
    if client_of(request).is_some() {
        return true;
    }
    let options = if marker == CANARY {
        let env = c.env();
        let dsn = env
            .var("SENTRY_DSN")
            .or_else(|| env.var("NEXT_PUBLIC_SENTRY_DSN"))
            .filter(|dsn| !dsn.is_empty());
        if dsn.is_none() {
            return false;
        }
        ClientOptions::default()
    } else {
        let Some(options) = usize::try_from(marker)
            .ok()
            .and_then(|at| MARKERS.with_borrow(|markers| markers.get(at).cloned()))
        else {
            return false;
        };
        options
    };
    let toucan = client(c, &options);
    CLIENTS.with_borrow_mut(|clients| clients.push((request, toucan)));
    true
}

/// Returns the client of `request`, once one is built.
fn client_of(request: u32) -> Option<Rc<Toucan>> {
    CLIENTS.with_borrow(|clients| {
        clients
            .iter()
            .find(|(held, _)| *held == request)
            .map(|(_, toucan)| Rc::clone(toucan))
    })
}

/// Drops every fork and the client of `request`, once the request ends.
pub fn forget(request: u32) {
    // A thread that tears its locals down keeps no fork and no client any
    // more.
    let _ = FORKS.try_with(|forks| {
        forks.borrow_mut().retain(|fork| fork.request != request);
    });
    let _ = CLIENTS.try_with(|clients| {
        if let Ok(mut clients) = clients.try_borrow_mut() {
            clients.retain(|(held, _)| *held != request);
        }
    });
}
