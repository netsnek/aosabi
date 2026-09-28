// SPDX-License-Identifier: MIT
/*
 * graphql-over-http host_env: the variables of the process and the console a host
 * of the TypeScript option hands the engine.
 *
 * No analogue in the reference, which reads process.env of its host and
 * prints through its console. A module for wasm32-unknown-unknown reads no
 * environment and its standard streams write nowhere, so the host of the
 * crate aosabi-js hands the variables of its process in at the boot and the
 * writer of its console.
 */

//! The variables of the process and the console a host hands in.
//!
//! Yoga reads `NODE_ENV` of the process for the interval of the ping of a stream
//! and `DEBUG` for the level of its logger. The module `core.wasm` of `aosabi-js`
//! reads no environment, so its host hands the variables of its process in at the
//! boot through [`set_process_var`], and [`var`] answers a variable the host
//! handed in before one of the process, which the native library of `aosabi-js`
//! reads as well. The logger of Yoga prints through the console of its host, so a
//! host sets the writer of its console through [`set_writer`], which every line
//! of the logger goes through in place of the standard streams of the process.
//! Only `aosabi-js` turns the feature `host-env` on, so a Rust service reads the
//! variables of its process and prints on its streams alone.

use core::cell::RefCell;
use std::rc::Rc;

/// The writer of the console of a host, which takes whether a line goes to
/// standard error and the line with no newline.
pub type Writer = Rc<dyn Fn(bool, &str)>;

// An `unsafe` cell would skip each borrow flag.
thread_local! {
    /// The variables the host handed in, by name, in the order it handed them.
    static HANDED: RefCell<Vec<(String, String)>> = const { RefCell::new(Vec::new()) };
    /// The writer of the console of the host, if it set one.
    static WRITER: RefCell<Option<Writer>> = const { RefCell::new(None) };
}

/// Keeps the variable `name` of the process of the host with `value`.
pub fn set_process_var(name: &str, value: &str) {
    // A thread that tears its locals down keeps no variable any more.
    let _ = HANDED.try_with(|handed| {
        let mut handed = handed.borrow_mut();
        match handed.iter_mut().find(|(key, _)| key == name) {
            Some(entry) => entry.1 = String::from(value),
            None => handed.push((String::from(name), String::from(value))),
        }
    });
}

/// Returns the variable `name`, the one the host handed in, or else the one
/// of the process.
#[must_use]
pub fn var(name: &str) -> Option<String> {
    HANDED
        .try_with(|handed| {
            handed
                .borrow()
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.clone())
        })
        .ok()
        .flatten()
        .or_else(|| std::env::var(name).ok())
}

/// Sets the writer of the console of the host, which every line of the
/// logger of Yoga goes through from now on, or takes it away with `None`.
pub fn set_writer(writer: Option<Writer>) {
    // A thread that tears its locals down writes nothing any more.
    let _ = WRITER.try_with(|held| *held.borrow_mut() = writer);
}

/// Returns the writer of the console of the host, if it set one.
#[must_use]
pub fn writer() -> Option<Writer> {
    WRITER.try_with(|held| held.borrow().clone()).ok().flatten()
}
