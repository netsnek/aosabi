// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-core glue decorate: the run time of a decorated entry, method,
 * static or function.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/core/create-decorator.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 */

//! `decorated` and the cell of the decorators of a site.
//!
//! A decorated site of the reference is one async function per decorator, the
//! outermost first, each awaiting its callback with the positional arguments
//! before it calls the next, and the innermost calling the member itself:
//! `async function (...args) { await callback(...args); return original.apply(this, args) }`.
//! [`decorated`] is that nest on the agent of the call, so a decorated
//! synchronous member settles as the reference's asynchronous one does.
//!
//! The macro that reads a site evaluates its `#[decorate(expr)]` expressions
//! once, at the first call on a thread, and keeps them in a cell of the
//! thread, the twin of a TypeScript decorator factory evaluated once at the
//! definition of its class.

use core::cell::OnceCell;
use std::{
    rc::Rc,
    thread::LocalKey, //
};

use aosabi_execute::{
    promise::Agent,
    value::{
        Completion,
        Value, //
    }, //
};

use crate::{
    decorator::{
        Decorator,
        DecoratorArgs, //
    },
    glue::Thrown,
};

/// The cell of the decorators of one site.
pub type DecoratorCell = OnceCell<Rc<[Decorator]>>;

/// The member a decorated site calls last, over the agent and the arguments.
pub type Member = Rc<dyn Fn(&Agent, &[Value]) -> Completion>;

/// Returns the decorators of the site whose cell is `site`, made by `init` at
/// the first call on the thread.
pub fn decorators(
    site: &'static LocalKey<DecoratorCell>,
    init: fn() -> Vec<Decorator>,
) -> Rc<[Decorator]> {
    site.with(|cell| Rc::clone(cell.get_or_init(|| Rc::from(init()))))
}

/// Calls `member` with `args` behind `decorators`, the outermost first.
///
/// Each decorator is one async function: its body calls the callback, which
/// is an async function too, awaits the promise of the callback and then
/// calls the next decorator or the member, whose value the promise of the
/// wrapper resolves with. A callback that throws rejects the wrapper, and the
/// member is not called.
#[must_use]
pub fn decorated(
    agent: &Agent,
    decorators: Rc<[Decorator]>,
    args: &[Value],
    member: Member,
) -> Completion {
    wrap(agent, decorators, 0, Rc::from(args), member)
}

/// Calls the wrapper of the decorator `index`.
fn wrap(
    agent: &Agent,
    decorators: Rc<[Decorator]>,
    index: usize,
    args: Rc<[Value]>,
    member: Member,
) -> Completion {
    let Some(decorator) = decorators.get(index).cloned() else {
        return member(agent, &args);
    };
    let body = async move {
        let Some(agent) = Agent::current() else {
            return Completion::Throw(Thrown::error(OUTSIDE_AGENT).0);
        };
        let callback = decorator.call(DecoratorArgs::new(Rc::clone(&args)));
        let awaited = agent.spawn(Box::pin(async move {
            match callback.await {
                Ok(()) => Completion::Normal(Value::Undefined),
                Err(thrown) => Completion::Throw(thrown.0),
            }
        }));
        if let Err(thrown) = awaited.await {
            return Completion::Throw(thrown);
        }
        wrap(&agent, decorators, index + 1, args, member)
    };
    Completion::Normal(Value::Promise(agent.spawn(Box::pin(body))))
}

/// The throw of a decorated call polled outside an agent, which no executor
/// does.
const OUTSIDE_AGENT: &str = "a decorated call ran outside the agent of its request";
