// SPDX-License-Identifier: MIT
/*
 * h2: log macros, spans and instrument that record nothing, in place of the
 * crate tracing.
 */
//! Logging that records nothing, in place of the crate `tracing`.
//!
//! The log macros take what the macros of `tracing` take and expand to `()`
//! with no argument evaluated, because some call sites use a log call as an
//! expression. A span holds nothing, and `Instrument` hands a future back as
//! it is, so the types that name `Instrumented` keep their shape. Every file
//! that logs names this module `tracing`, so its calls stay as they are.

macro_rules! trace {
    ($($arg:tt)*) => {
        ()
    };
}

macro_rules! debug {
    ($($arg:tt)*) => {
        ()
    };
}

// A macro named `warn` and exported by `use` is ambiguous with the built in
// attribute of that name, so it is written under another name.
macro_rules! warn_event {
    ($($arg:tt)*) => {
        ()
    };
}

macro_rules! trace_span {
    ($($arg:tt)*) => {
        $crate::tracing::Span::none()
    };
}

macro_rules! debug_span {
    ($($arg:tt)*) => {
        $crate::tracing::Span::none()
    };
}

pub(crate) use {debug, debug_span, trace, trace_span, warn_event as warn};

/// A span that records nothing.
#[derive(Clone, Debug)]
pub(crate) struct Span(());

impl Span {
    /// Returns a span that records nothing.
    pub(crate) fn none() -> Span {
        Span(())
    }

    /// Returns the span of the caller, which records nothing.
    pub(crate) fn current() -> Span {
        Span(())
    }

    /// Enters the span until the guard it returns is dropped.
    pub(crate) fn enter(&self) -> Entered {
        Entered(())
    }

    /// Calls `f` inside the span.
    pub(crate) fn in_scope<T>(&self, f: impl FnOnce() -> T) -> T {
        f()
    }

    /// Records that the span follows from `cause`.
    pub(crate) fn follows_from(&self, _cause: Span) -> &Span {
        self
    }
}

/// The guard of an entered span.
#[derive(Debug)]
pub(crate) struct Entered(());

pub(crate) mod instrument {
    //! Futures inside a span.

    use super::Span;

    /// A future inside a span, which is the future itself.
    pub(crate) type Instrumented<F> = F;

    /// Puts a future inside a span.
    pub(crate) trait Instrument: Sized {
        /// Returns the future inside `span`.
        fn instrument(self, span: Span) -> Instrumented<Self> {
            let _ = span;
            self
        }
    }

    impl<T> Instrument for T {}
}

pub(crate) use instrument::Instrument;
