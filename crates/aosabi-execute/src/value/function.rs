// SPDX-License-Identifier: MIT
/*
 * aosabi-execute value function: a JavaScript function as the executor calls it.
 *
 * No analogue in the reference. The call is ECMA-262 Call(F, V, argumentsList),
 * https://tc39.es/ecma262/#sec-call, with no `this` value, because a member
 * function of a user object is bound before the executor sees it.
 */

//! The function value.

use std::rc::Rc;

use crate::{
    promise::Agent,
    value::{
        Completion,
        Value, //
    }, //
};

/// The body of a [`Function`], which receives the agent of the request and
/// the positional arguments.
type FunctionBody = dyn Fn(&Agent, &[Value]) -> Completion;

/// A JavaScript function, a name for `inspect` and a body over a positional argument list.
///
/// A clone is the same function, because the body sits behind an `Rc`.
#[derive(Clone)]
pub struct Function {
    /// The `name` property, which the value printers write and which is the
    /// empty string for an anonymous function.
    pub name: Rc<str>,
    /// The code of the function.
    body: Rc<FunctionBody>,
}

impl Function {
    /// Creates the function `name` with the code `body`.
    ///
    /// `body` receives the agent the call runs on and the arguments of the
    /// call, and a missing argument reads as absent from the slice rather
    /// than as `Value::Undefined`.
    pub fn new(name: &str, body: impl Fn(&Agent, &[Value]) -> Completion + 'static) -> Self {
        Self {
            name: Rc::from(name),
            body: Rc::new(body),
        }
    }

    /// Calls the function with `args` and returns its completion.
    ///
    /// This is `Call(F, undefined, args)` of ECMA-262 section 7.3.13. A throw
    /// of the body is the throw completion, and a Rust panic is no throw and is
    /// never caught.
    #[must_use]
    pub fn call(&self, agent: &Agent, args: &[Value]) -> Completion {
        (self.body)(agent, args)
    }
}
