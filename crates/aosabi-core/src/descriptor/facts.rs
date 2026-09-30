// SPDX-License-Identifier: MIT
/*
 * aosabi-core: the facts of a decorator and of a guard call that the
 * authoring check reads.
 *
 * No analogue on the reference side. The facts stand in for the decorator
 * nodes and the leading guard calls the sidecar reads off the TypeScript
 * source for check C3 of tests/notebooks/12_authoring_parity.ipynb.
 */

//! `DecoratorFact`, `GuardFact`, `GuardCall` and `GuardKind`.
//!
//! A decorator of a TypeScript service is an expression at its place, `@audited('orders')`
//! above a method, and the authoring check compares where each decorator of a
//! pair stands and what it says. The macros record the text of each
//! `#[decorate(...)]` and the line it stands on, and `authoring.json`
//! carries them beside the node they decorate.
//!
//! A guard is a call of `require_role` or `authorize` in the leading run of a
//! body, the twin of `requireRole(...)` and `authorize(...)` at the head of a
//! resolver of a TypeScript service. The run is the statements from the start of the body
//! while each is such a call through `?`, or the tail of the body with or
//! without `?`, so a statement that drops the result of a guard ends the run,
//! as rule 1 of the module doc of `aosabi-macros/src/guards.rs` says. The
//! macros record each guard of the run with its roles, the checks of its
//! closure and its line, and `authoring.json` carries them beside the
//! decorators. The inference reads none of these facts, because neither a
//! decorator nor a guard changes a byte of a schema.

/// One decorator of an entry, a method, a static or a free function.
#[derive(Clone, Copy, Debug)]
pub struct DecoratorFact {
    /// The expression of the decorator as written, the tokens of
    /// `#[decorate(...)]` printed back.
    pub text: &'static str,
    /// The source line of the attribute, counted from 1.
    pub line: u32,
}

/// Which call a guard is.
#[derive(Clone, Copy, Debug)]
pub enum GuardKind {
    /// `require_role(&[...])?`, the twin of `requireRole(...)`.
    RequireRole,
    /// `authorize(|p| ...)?`, the twin of `authorize(p => ...)`.
    Authorize,
}

impl GuardKind {
    /// Returns the name of the call, `require_role` or `authorize`, as the
    /// facade names it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::RequireRole => "require_role",
            Self::Authorize => "authorize",
        }
    }
}

/// One check inside the closure of an `authorize` guard.
#[derive(Clone, Copy, Debug)]
pub struct GuardCall {
    /// `has_role` or `has_permission`.
    pub name: &'static str,
    /// The string literals among the arguments of the check, in source
    /// order, the elements of a literal slice among them and the principal
    /// left out.
    pub args: &'static [&'static str],
}

/// One guard of the leading run of a body.
#[derive(Clone, Copy, Debug)]
pub struct GuardFact {
    /// Which call the guard is.
    pub kind: GuardKind,
    /// The roles of a `require_role` whose argument is a literal slice of
    /// string literals, in order, and `None` for any other argument and for
    /// an `authorize`.
    pub args: Option<&'static [&'static str]>,
    /// The `has_role` and `has_permission` calls inside the closure of an
    /// `authorize`, in source order, and none for a `require_role`.
    pub calls: &'static [GuardCall],
    /// The count of every other call and every comparison inside the closure
    /// of an `authorize`, and 0 for a `require_role`.
    pub others: u32,
    /// The source line of the call, counted from 1.
    pub line: u32,
}
