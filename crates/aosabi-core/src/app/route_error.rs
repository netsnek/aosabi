// SPDX-License-Identifier: MIT
/*
 * aosabi-core: the thrown value as the error of a route, and the error of a
 * plugin that fails during its setup.
 *
 * No analogue on the reference side for the first, where a route throws a
 * JavaScript value and hono reads it.
 */

//! `impl RouteError for Thrown` and [`PluginError`].
//!
//! The web app of `aosabi-http`, the translation of the app of hono, names the
//! error of a handler as a type parameter, and the core fills it with
//! [`Thrown`], the value a Rust route throws. The readings hono and the base
//! pipeline make of a thrown value are the ones below: whether it is
//! an instance of `Error`, the response of an `HTTPException`, a numeric
//! `statusCode`, the message and the name.

use core::fmt;

use aosabi_execute::value::{
    ErrorObject,
    Value,
    to_js_string, //
};
use aosabi_http::{
    HttpException,
    Response,
    RouteError, //
};
use aosabi_json::JsValue;
use aosabi_lang::js_string::to_string_lossy;

use crate::{
    app::plugin_order::PluginOrderError,
    glue::Thrown, //
};

impl RouteError for Thrown {
    fn is_error(&self) -> bool {
        self.0.is_error()
    }

    fn http_exception_response(&self) -> Option<Response> {
        let Value::Error(error) = &self.0 else {
            return None;
        };
        let exception = error.host()?.downcast_ref::<HttpException>()?;
        Some(exception.get_response())
    }

    fn status_code(&self) -> Option<f64> {
        let Value::Error(error) = &self.0 else {
            return None;
        };
        match error.prop("statusCode")? {
            JsValue::Number(status) => Some(*status),
            _ => None,
        }
    }

    fn message(&self) -> String {
        match &self.0 {
            Value::Error(error) => to_string_lossy(&error.message()).into_owned(),
            other => to_string_lossy(&to_js_string(other)).into_owned(),
        }
    }

    fn name(&self) -> String {
        match &self.0 {
            Value::Error(error) => to_string_lossy(&error.name()).into_owned(),
            _ => String::from("Error"),
        }
    }

    fn error(message: &str) -> Self {
        Self(Value::from(ErrorObject::error(message)))
    }

    fn type_error(message: &str) -> Self {
        Self(Value::from(ErrorObject::type_error(message)))
    }

    fn range_error(message: &str) -> Self {
        Self(Value::from(ErrorObject::range_error(message)))
    }
}

/// The error a pass of the boot throws, a plugin that failed during its
/// setup or a cycle of the dependencies.
///
/// The message names the plugin by its name,
/// `Aosabi plugin "<name>" failed during setup: <message>`, or by its index in
/// the sorted array,
/// `Aosabi plugin #<index> (unnamed) failed during setup: <message>`, for no
/// name and for the empty one, which the reference tests for truth, with the
/// message of the cause, and the cause stays attached. A cause that is no
/// error is `new Error(String(e))`. The cycle of `plugin-order.ts:26-28`
/// reaches the loader unwrapped, so it is an error of its message with no
/// cause.
#[derive(Debug)]
pub struct PluginError {
    /// The message of the error.
    pub message: String,
    /// What the setup threw, `None` for a cycle.
    pub cause: Option<Thrown>,
}

impl PluginError {
    /// Returns the error of the plugin at `index` of the sorted array, named
    /// `name` when it has a name, whose setup threw `cause`.
    #[must_use]
    pub fn of(name: Option<&str>, index: usize, cause: Thrown) -> Self {
        let label = match name.filter(|name| !name.is_empty()) {
            Some(name) => format!("\"{name}\""),
            None => format!("#{index} (unnamed)"),
        };
        let cause = if cause.0.is_error() {
            cause
        } else {
            let message = to_string_lossy(&to_js_string(&cause.0)).into_owned();
            Thrown::error(&message)
        };
        let message = format!(
            "Aosabi plugin {label} failed during setup: {}",
            cause.message()
        );
        Self {
            message,
            cause: Some(cause),
        }
    }

    /// Returns the line node prints after `[cause]: ` for the cause, `None` with no cause.
    ///
    /// Node prints the first line of the stack of the cause, which starts with
    /// the `name` of the error, a colon, a space and its `message`, or with
    /// the name alone for an empty message. The loader makes every cause an
    /// error, so the line never reads a thrown value that is none.
    #[must_use]
    pub fn cause_line(&self) -> Option<String> {
        let cause = self.cause.as_ref()?;
        let name = cause.name();
        let message = cause.message();
        if message.is_empty() {
            return Some(name);
        }
        Some(format!("{name}: {message}"))
    }
}

impl From<PluginOrderError> for PluginError {
    fn from(error: PluginOrderError) -> Self {
        Self {
            message: error.message,
            cause: None,
        }
    }
}

impl fmt::Display for PluginError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl core::error::Error for PluginError {}
