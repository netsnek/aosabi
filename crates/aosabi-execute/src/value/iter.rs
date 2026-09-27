// SPDX-License-Identifier: MIT
/*
 * aosabi-execute value iter: the async iterator protocol.
 *
 * No analogue file in the reference, where the protocol is the engine's. The port
 * follows ECMA-262 section 7.4, the operations on iterator objects,
 * https://tc39.es/ecma262/#sec-operations-on-iterator-objects, and section 27.1,
 * the iteration interfaces, https://tc39.es/ecma262/#sec-iteration.
 */

//! The async iterator protocol of ECMA-262.
//!
//! A subscription resolver answers an async iterable, and every layer between
//! it and the stream of a response reads it through the same few operations:
//! whether a value is async iterable, the call of its `[Symbol.asyncIterator]`
//! method, the read of a method such as `return`, and the iterator result
//! `{value, done}`. The key `Symbol.asyncIterator` is a symbol and no
//! string, so the method is [`ObjectLike::async_iterator`] and not a key of
//! [`ObjectLike::get`], and the value model gains no variant.
//!
//! [`IteratorObject`] is the object literal every helper of the reference
//! returns, `{next, return, throw, [Symbol.asyncIterator]() { return this }}`.
//!
//! The module names `core` and `alloc` and no item of `std`, so a build
//! without the standard library takes it as it is.

use alloc::{
    rc::Rc,
    string::String,
    vec,
    vec::Vec, //
};

use crate::{
    execute::to_boolean,
    promise::Agent,
    value::{
        Completion,
        ErrorObject,
        Function,
        ObjectLike,
        PlainObject,
        Value, //
    }, //
};

/// Returns whether `value[Symbol.asyncIterator]` is a function.
///
/// An object of the value model has the method when
/// [`ObjectLike::async_iterator`] answers one, and no other value has it.
#[must_use]
pub fn is_async_iterable(value: &Value) -> bool {
    match value {
        Value::Object(object) => Rc::clone(object).async_iterator().is_some(),
        _ => false,
    }
}

/// Calls `value[Symbol.asyncIterator]()` and returns its completion.
///
/// The call takes no argument. A value without the method throws a `TypeError`,
/// whose message is the one V8 writes for the call
/// `value[Symbol.asyncIterator]()`. Every caller asks [`is_async_iterable`]
/// first, so no answer reaches that throw.
#[must_use]
pub fn get_async_iterator(agent: &Agent, value: &Value) -> Completion {
    let method = match value {
        Value::Object(object) => Rc::clone(object).async_iterator(),
        _ => None,
    };
    match method {
        Some(method) => method.call(agent, &[]),
        None => Completion::Throw(Value::from(ErrorObject::type_error(
            "value[Symbol.asyncIterator] is not a function",
        ))),
    }
}

/// Returns `value[name]` when it is a function, and `None` otherwise.
///
/// This is the read `typeof iterator.return === 'function'` the helpers of
/// the reference make before they call an optional method. Only an object
/// has a property here, so any other value answers `None`.
#[must_use]
pub fn get_method(value: &Value, name: &str) -> Option<Function> {
    let Value::Object(object) = value else {
        return None;
    };
    match Rc::clone(object).get(name) {
        Value::Function(function) => Some(function),
        _ => None,
    }
}

/// Returns the iterator result `{value, done}`.
///
/// Translation of `CreateIterResultObject` of ECMA-262, which creates the
/// property `value` and then the property `done`, so the keys of the object are
/// in that order.
#[must_use]
pub fn iter_result(value: Value, done: bool) -> Value {
    Value::from(PlainObject::from_entries(vec![
        (String::from("value"), value),
        (String::from("done"), Value::Bool(done)),
    ]))
}

/// Reads `value` and `done` off an iterator result, `done` through `ToBoolean`.
///
/// Translation of `IteratorValue` and `IteratorComplete` of ECMA-262, which the
/// reference writes as `result.value` and `result.done`. A value that is no
/// object reads as `{value: undefined, done: false}`, which is what a property
/// read off a primitive answers. The read off `undefined` or `null` throws in the
/// reference, and no iterator of the model answers either.
#[must_use]
pub fn iter_result_parts(result: &Value) -> (Value, bool) {
    let Value::Object(object) = result else {
        return (Value::Undefined, false);
    };
    let done = to_boolean(&Rc::clone(object).get("done"));
    let value = Rc::clone(object).get("value");
    (value, done)
}

/// The object literal `{next, return, throw, [Symbol.asyncIterator]() { return this }}`.
///
/// The helpers of the reference answer an async iterator as such a literal,
/// `mapAsyncIterator` of `@whatwg-node/promise-helpers` and
/// `finalAsyncIterator` of `@envelop/core` among them. The three methods are
/// functions the helper closes over its own state, so a read of `next` off
/// the object answers a function that needs no `this`. The methods `return`
/// and `throw` are optional, because some helpers leave them out.
pub struct IteratorObject {
    /// The helper of the reference that makes the literal.
    name: &'static str,
    /// The method `next`.
    next: Function,
    /// The method `return`, when the literal has one.
    r#return: Option<Function>,
    /// The method `throw`, when the literal has one.
    throw: Option<Function>,
}

impl IteratorObject {
    /// Creates the literal that `name` returns with its three methods.
    ///
    /// The parameter `name` names the helper of the reference, `mapAsyncIterator` for
    /// example, and reaches no byte of an answer, because the constructor of
    /// a literal is `Object`.
    #[must_use]
    pub fn new(
        name: &'static str,
        next: Function,
        r#return: Option<Function>,
        throw: Option<Function>,
    ) -> Rc<Self> {
        Rc::new(Self {
            name,
            next,
            r#return,
            throw,
        })
    }

    /// Returns the helper of the reference that made the literal.
    #[must_use]
    pub fn name(&self) -> &'static str {
        self.name
    }
}

impl ObjectLike for IteratorObject {
    /// Answers `next`, `return` and `throw`, and `undefined` for any other key.
    fn get(self: Rc<Self>, key: &str) -> Value {
        let method = match key {
            "next" => Some(&self.next),
            "return" => self.r#return.as_ref(),
            "throw" => self.throw.as_ref(),
            _ => None,
        };
        method.map_or(Value::Undefined, |method| Value::Function(method.clone()))
    }

    /// Lists the present methods in the order of the literal.
    ///
    /// The method `[Symbol.asyncIterator]` is keyed by a symbol, which
    /// `Object.keys` leaves out.
    fn own_keys(&self) -> Vec<String> {
        let mut keys = vec![String::from("next")];
        if self.r#return.is_some() {
            keys.push(String::from("return"));
        }
        if self.throw.is_some() {
            keys.push(String::from("throw"));
        }
        keys
    }

    /// Answers the method `[Symbol.asyncIterator]() { return this }`.
    fn async_iterator(self: Rc<Self>) -> Option<Function> {
        Some(Function::new("[Symbol.asyncIterator]", move |_, _| {
            Completion::Normal(Value::Object(Rc::clone(&self) as Rc<dyn ObjectLike>))
        }))
    }
}
