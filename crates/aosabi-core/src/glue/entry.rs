// SPDX-License-Identifier: MIT
/*
 * aosabi-core glue entry: the run time of graphql!.
 *
 * No analogue on the reference side, where the object literal of the roots is
 * an object of JavaScript already. The object follows the evaluation of an
 * object literal with spreads, ECMA-262 section 13.2.5.5,
 * https://tc39.es/ecma262/#sec-runtime-semantics-propertydefinitionevaluation.
 */

//! The run time of `graphql!`.
//!
//! The macro `graphql!` turns each entry into a function of the executor, a
//! [`EntryFunction`] whose `run` reads the positional arguments and calls the
//! function of the entry, the static it references or the free function under
//! `#[aosabi::resolver]`. A root is an object of those functions built by
//! [`root_object`] in the order of the literal: an entry sets its key and a
//! spread copies the own keys of the other root, a key set twice keeping its
//! first position and its later value, which is what JavaScript does for
//! `{a, ...m1, b}`. That order is not the order of the schema, which the
//! checker gives the spread type in. [`factory`] makes the factory of a root,
//! which each thread of a service calls once to read the members of the root.

use std::sync::Arc;

use aosabi_execute::{
    kernel_entry::RootFactory,
    promise::Agent,
    value::{
        Completion,
        Function,
        PlainObject,
        Value, //
    }, //
};

use crate::descriptor::MethodInfo;

/// The run time of one function an entry reaches, over the agent and the
/// positional arguments of the call.
pub type Run = fn(&Agent, &[Value]) -> Completion;

/// A function an entry of `graphql!` reaches, with its descriptor.
///
/// The resolvers block emits one for every static as the associated item
/// `__AOSABI_STATIC_<name>`, and `#[aosabi::resolver]` emits one for a free
/// function as `<name>::__AOSABI_RESOLVER`.
#[derive(Clone, Copy, Debug)]
pub struct EntryFunction {
    /// The descriptor of the function.
    pub info: &'static MethodInfo,
    /// The run time of the function.
    pub run: Run,
}

/// One part of a root object in source order.
#[derive(Clone, Copy, Debug)]
pub enum Part {
    /// An entry, its key and its run time.
    Entry(&'static str, Run),
    /// A spread, the object of the other root.
    Spread(fn() -> Value),
}

/// Returns the object of a root made of `parts` in source order.
///
/// An entry sets its key to its function, and a spread sets each own key of
/// the other object to its value, a key set before keeping its position.
/// A thread calls it once per root, so the object takes room for one entry
/// per part up front.
#[must_use]
pub fn root_object(parts: &[Part]) -> Value {
    let mut entries: Vec<(String, Value)> = Vec::with_capacity(parts.len());
    let mut set =
        |key: String, value: Value| match entries.iter_mut().find(|(known, _)| *known == key) {
            Some(entry) => entry.1 = value,
            None => entries.push((key, value)),
        };
    for part in parts {
        match part {
            Part::Entry(name, run) => {
                set(
                    String::from(*name),
                    Value::Function(Function::new(name, *run)),
                );
            }
            Part::Spread(object) => {
                let Value::Object(object) = object() else {
                    continue;
                };
                for key in object.own_keys() {
                    let value = std::rc::Rc::clone(&object).get(&key);
                    set(key, value);
                }
            }
        }
    }
    Value::Object(PlainObject::from_entries(entries))
}

/// Returns the factory that answers a fresh object of a root per call.
#[must_use]
pub fn factory(object: fn() -> Value) -> RootFactory {
    Arc::new(object)
}
