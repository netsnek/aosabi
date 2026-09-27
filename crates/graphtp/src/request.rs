// SPDX-License-Identifier: MIT
/*
 * graphtp request: the owned request of an OPERATION, generic over its body,
 * with its parts and its builder.
 *
 * No analogue in the reference. The request and its mapping onto an HTTP POST
 * are those of the specification, and the shape of the type, a head of parts
 * and a body with a builder, is that of the request of the crate http 1.5.0.
 */

//! The owned request of an OPERATION, generic over its body as the request of
//! `http` is.
//!
//! A request maps one to one onto an HTTP POST: its credential is the value of
//! `authorization` and its body the body of the POST, the GraphQL request as
//! its client wrote it. A PULL and a CANCEL carry no GraphQL request and have
//! no owned form, and [`crate::wire`] writes and reads them.

use alloc::vec::Vec;

use crate::{
    credential::Credential,
    status::Code,
    version::Version, //
};

/// An OPERATION: its parts, the version and the credential, and its body.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Request<B> {
    /// The version and the credential.
    head: Parts,
    /// The GraphQL request.
    body: B,
}

/// The parts of a request besides its body.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Parts {
    /// The version the request is written in.
    pub version: Version,
    /// The value of `Authorization`, or `None` for none.
    ///
    /// The builder checks it by rule 6 of `docs/graphtp.md` section 5.3 and by
    /// the length a head carries, and a writer checks it again through
    /// [`Credential::new`], since the field is public.
    pub credential: Option<Vec<u8>>,
}

impl Parts {
    /// Returns the parts of GraphTP/1 with no credential.
    const fn new() -> Parts {
        Parts {
            version: Version::V1,
            credential: None,
        }
    }
}

impl Request<()> {
    /// Returns a builder of a request.
    ///
    /// The function stands on the request of the body `()` alone, as the
    /// builder of the request of `http` 1.5.0 does, so `Request::builder()`
    /// names its type with no annotation, and [`Builder::body`] gives the
    /// request its body type.
    #[must_use]
    pub const fn builder() -> Builder {
        Builder::new()
    }
}

impl<B> Request<B> {
    /// Returns the request of GraphTP/1 with no credential and the body `body`.
    #[must_use]
    pub const fn new(body: B) -> Request<B> {
        Request {
            head: Parts::new(),
            body,
        }
    }

    /// Returns the request of the parts `parts` and the body `body`.
    #[must_use]
    pub const fn from_parts(parts: Parts, body: B) -> Request<B> {
        Request { head: parts, body }
    }

    /// Returns the parts and the body of the request.
    #[must_use]
    pub fn into_parts(self) -> (Parts, B) {
        (self.head, self.body)
    }

    /// Returns the version the request is written in.
    #[must_use]
    pub const fn version(&self) -> Version {
        self.head.version
    }

    /// Returns the value of `Authorization`, or `None` for none.
    #[must_use]
    pub fn credential(&self) -> Option<&[u8]> {
        self.head.credential.as_deref()
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

    /// Returns the request whose body is `f` of this body, with the same parts.
    #[must_use]
    pub fn map<C>(self, f: impl FnOnce(B) -> C) -> Request<C> {
        Request {
            head: self.head,
            body: f(self.body),
        }
    }
}

/// The builder of a [`Request`], which checks the credential as it is set.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Builder {
    /// The parts so far, or the code that refuses the first bad one.
    inner: Result<Parts, Code>,
}

impl Builder {
    /// Returns the builder of a request of GraphTP/1 with no credential.
    #[must_use]
    pub const fn new() -> Builder {
        Builder {
            inner: Ok(Parts::new()),
        }
    }

    /// Sets the version the request is written in.
    #[must_use]
    pub fn version(self, version: Version) -> Builder {
        self.and_then(|mut parts| {
            parts.version = version;
            Ok(parts)
        })
    }

    /// Sets the credential, the value of `Authorization`, and an empty value
    /// sets none.
    ///
    /// The value is checked by rule 6 of `docs/graphtp.md` section 5.3, every
    /// byte `0x20` to `0x7E`, and by the 65,535 bytes a head carries, and a
    /// value that fails refuses the request [`Builder::body`] builds.
    #[must_use]
    pub fn credential(self, value: impl Into<Vec<u8>>) -> Builder {
        let value = value.into();
        self.and_then(|mut parts| {
            Credential::new(&value)?;
            parts.credential = (!value.is_empty()).then_some(value);
            Ok(parts)
        })
    }

    /// Returns the request of the parts set and the body `body`.
    ///
    /// # Errors
    ///
    /// Returns the [`Code`] a server refuses the first bad part with: 400.7 for
    /// a credential with a byte outside `0x20` to `0x7E`, and 431.0 for one
    /// longer than 65,535 bytes.
    pub fn body<B>(self, body: B) -> Result<Request<B>, Code> {
        self.inner.map(|parts| Request::from_parts(parts, body))
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
