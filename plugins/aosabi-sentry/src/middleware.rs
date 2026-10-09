// SPDX-License-Identifier: MIT
/*
 * aosabi-sentry: the middleware of @hono/sentry, which reports the error of a
 * route.
 *
 * Derived from hono 1.2.2, dist/index.js, MIT.
 */

//! `sentry`, `get_sentry` and the context variable `sentry`.
//!
//! The middleware builds one [`Toucan`] per request from the DSN of the
//! environment of the context, `SENTRY_DSN` or else `NEXT_PUBLIC_SENTRY_DSN`,
//! overridden by its options, with the request data allowing the header
//! `user-agent` and every search parameter, sets it as the variable `sentry`,
//! awaits the rest of the chain and captures `c.error` when the chain set one.
//! Hono sets `c.error` before it calls its error handler, and only for a
//! thrown `Error`, so the middleware reports the error of a route whose answer
//! the error handler wrote.
//!
//! On the Node adapter the environment of the context is the adapter pair,
//! which holds no variable, so an environment DSN reaches the middleware only
//! through its option `dsn`. The middleware takes no callback as its second
//! parameter.

use std::rc::Rc;

use aosabi_core::{
    Context,
    Key,
    Next,
    Thrown,
    execute::value::{
        Value,
        to_js_string, //
    }, //
};
use aosabi_http::MiddlewareFn;
use aosabi_lang::js_string::to_string_lossy;

use crate::{
    envelope::Exception,
    hub::ClientOptions,
    toucan::{
        RequestDataOptions,
        Toucan,
        ToucanOptions, //
    },
};

/// The typed key of the variable `sentry`, the client of the request.
pub const SENTRY: Key<Rc<Toucan>> = Key::new("sentry");

/// Returns the middleware that reports the error of a route, `sentry(options)`.
#[must_use]
pub fn sentry(options: ClientOptions) -> Rc<dyn MiddlewareFn<Thrown>> {
    let options = Rc::new(options);
    Rc::new(move |c: Context, next: Next| {
        let options = Rc::clone(&options);
        async move {
            let toucan = client(&c, &options);
            c.set(SENTRY, Rc::clone(&toucan));
            let response = next.run(c.clone()).await?;
            if let Some(error) = c.error()
                && let Some(thrown) = error.downcast_ref::<Thrown>()
            {
                toucan.capture_exception(exception_of(thrown));
            }
            Ok::<_, Thrown>(response)
        }
    })
}

/// Returns the client the middleware builds for the request `c` with
/// `options`, the `Toucan` of `index.js:14-30`, before it sets the variable.
///
/// The host of the TypeScript option runs the middleware in its own hono and
/// asks the engine for this client, which its facade calls.
#[must_use]
pub fn client(c: &Context, options: &ClientOptions) -> Rc<Toucan> {
    Rc::new(Toucan::new(toucan_options(c, options)))
}

/// Returns the client of the request, `getSentry(c)`, when the middleware set one.
#[must_use]
pub fn get_sentry(c: &Context) -> Option<Rc<Toucan>> {
    c.get(SENTRY)
}

/// Returns the options of the Toucan of the request `c`.
///
/// The DSN of the options wins over the environment, `{dsn: env, ...options}`,
/// and an option the application left out keeps the value of the
/// environment.
fn toucan_options(c: &Context, options: &ClientOptions) -> ToucanOptions {
    let env = c.env();
    let dsn = options
        .dsn
        .clone()
        .or_else(|| env.var("SENTRY_DSN"))
        .or_else(|| env.var("NEXT_PUBLIC_SENTRY_DSN"));
    ToucanOptions {
        client: ClientOptions {
            dsn,
            ..options.clone()
        },
        request: Some(c.req().raw()),
        request_data_options: RequestDataOptions {
            allowed_headers: Some(vec![String::from("user-agent")]),
            allow_all_search_params: true,
        },
    }
}

/// Returns the exception of a thrown value, as the event builder of Toucan
/// reads it.
///
/// An `Error` gives its name and its message, `No error message` for an empty
/// one, and any other value is wrapped as an `Error` whose message is its
/// text.
fn exception_of(thrown: &Thrown) -> Exception {
    match &thrown.0 {
        Value::Error(error) => exception(
            to_string_lossy(&error.name()).into_owned(),
            to_string_lossy(&error.message()).into_owned(),
        ),
        other => exception(
            String::from("Error"),
            to_string_lossy(&to_js_string(other)).into_owned(),
        ),
    }
}

/// Returns the exception of an error of the name `ty` and the message `value`,
/// `No error message` for an empty message, as the event builder of Toucan
/// reads an `Error`.
#[must_use]
pub fn exception(ty: String, value: String) -> Exception {
    let value = if value.is_empty() {
        String::from("No error message")
    } else {
        value
    };
    Exception { ty, value }
}
