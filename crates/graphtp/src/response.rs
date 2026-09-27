// SPDX-License-Identifier: MIT
/*
 * graphtp response: the owned answer, generic over its body, with its parts
 * and its builder.
 *
 * No analogue in the reference. The answer and its mapping onto an HTTP
 * response are those of the specification, and the shape of the type, a head
 * of parts and a body with a builder, is that of the response of the crate
 * http 1.5.0.
 */

//! The owned answer, generic over its body as the response of `http` is.
//!
//! An answer maps one to one onto an HTTP response: its status is the status,
//! its substatus the field `graphtp-status`, and its body the body.

use crate::status::{
    Code,
    Status,
    Substatus, //
};

/// An answer: its parts, the status and the substatus, and its body.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Response<B> {
    /// The status and the substatus.
    head: Parts,
    /// The body.
    body: B,
}

/// The parts of an answer besides its body.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Parts {
    /// The HTTP status, one that has a status byte.
    pub status: Status,
    /// The substatus, 0 for none.
    pub substatus: Substatus,
}

impl Parts {
    /// Returns the parts of 200.0.
    const fn new() -> Parts {
        Parts {
            status: Status::OK,
            substatus: Substatus::NONE,
        }
    }

    /// Returns the code of the parts, the status with its substatus.
    #[must_use]
    pub const fn code(&self) -> Code {
        Code::new(self.status, self.substatus)
    }
}

impl Response<()> {
    /// Returns a builder of an answer.
    ///
    /// The function stands on the answer of the body `()` alone, as the
    /// builder of the response of `http` 1.5.0 does, so `Response::builder()`
    /// names its type with no annotation, and [`Builder::body`] gives the
    /// answer its body type.
    #[must_use]
    pub const fn builder() -> Builder {
        Builder::new()
    }
}

impl<B> Response<B> {
    /// Returns the answer 200.0 with the body `body`.
    #[must_use]
    pub const fn new(body: B) -> Response<B> {
        Response {
            head: Parts::new(),
            body,
        }
    }

    /// Returns the answer of the parts `parts` and the body `body`.
    #[must_use]
    pub const fn from_parts(parts: Parts, body: B) -> Response<B> {
        Response { head: parts, body }
    }

    /// Returns the parts and the body of the answer.
    #[must_use]
    pub fn into_parts(self) -> (Parts, B) {
        (self.head, self.body)
    }

    /// Returns the status.
    #[must_use]
    pub const fn status(&self) -> Status {
        self.head.status
    }

    /// Returns the substatus.
    #[must_use]
    pub const fn substatus(&self) -> Substatus {
        self.head.substatus
    }

    /// Returns the code, the status with its substatus.
    #[must_use]
    pub const fn code(&self) -> Code {
        self.head.code()
    }

    /// Returns the body.
    #[must_use]
    pub const fn body(&self) -> &B {
        &self.body
    }

    /// Returns the body for a change.
    #[must_use]
    pub const fn body_mut(&mut self) -> &mut B {
        &mut self.body
    }

    /// Returns the body and drops the parts.
    #[must_use]
    pub fn into_body(self) -> B {
        self.body
    }

    /// Returns the answer whose body is `f` of this body, with the same parts.
    #[must_use]
    pub fn map<C>(self, f: impl FnOnce(B) -> C) -> Response<C> {
        Response {
            head: self.head,
            body: f(self.body),
        }
    }
}

/// The builder of a [`Response`], which checks the status as it is set.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Builder {
    /// The parts so far, or the code that refuses the first bad one.
    inner: Result<Parts, Code>,
}

impl Builder {
    /// Returns the builder of the answer 200.0.
    #[must_use]
    pub const fn new() -> Builder {
        Builder {
            inner: Ok(Parts::new()),
        }
    }

    /// Sets the status of the number `status`.
    ///
    /// A status with no status byte refuses the answer [`Builder::body`]
    /// builds with 500.2 Status Not Representable, the code a server answers
    /// such a status of its service with. The status 294 of the GraphQL over
    /// HTTP draft is the exception: a server carries it as 200.1 or 200.2,
    /// chosen by the body, before it builds the answer, and the builder, which
    /// reads no body, refuses a 294 as it refuses every other status with no
    /// byte.
    #[must_use]
    pub fn status(self, status: u16) -> Builder {
        self.and_then(|mut parts| {
            parts.status = Status::from_u16(status).ok_or(Code::STATUS_NOT_REPRESENTABLE)?;
            Ok(parts)
        })
    }

    /// Sets the substatus.
    #[must_use]
    pub fn substatus(self, substatus: u8) -> Builder {
        self.and_then(|mut parts| {
            parts.substatus = Substatus::new(substatus);
            Ok(parts)
        })
    }

    /// Sets the status and the substatus of the code `code`.
    #[must_use]
    pub fn code(self, code: Code) -> Builder {
        self.and_then(|mut parts| {
            parts.status = code.status;
            parts.substatus = code.substatus;
            Ok(parts)
        })
    }

    /// Returns the answer of the parts set and the body `body`.
    ///
    /// # Errors
    ///
    /// Returns 500.2 when a status set has no status byte.
    pub fn body<B>(self, body: B) -> Result<Response<B>, Code> {
        self.inner.map(|parts| Response::from_parts(parts, body))
    }

    /// Applies `f` to the parts unless an earlier part was refused.
    fn and_then(self, f: impl FnOnce(Parts) -> Result<Parts, Code>) -> Builder {
        Builder {
            inner: self.inner.and_then(f),
        }
    }
}

impl Default for Builder {
    fn default() -> Builder {
        Builder::new()
    }
}
