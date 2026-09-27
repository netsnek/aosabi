// SPDX-License-Identifier: MIT
/*
 * graphtp: the types of GraphTP and the codec of the GraphTP/1 wire form.
 *
 * No analogue in the reference. The specification is docs/graphtp.md, whose
 * section 10.3 names the modules below and section 10.4 their public items.
 * The crate is a protocol crate in the shape of the crate http 1.5.0, so it
 * holds no I/O, no transport, no executor and no server.
 */

//! The types of `GraphTP`, the Graph Transfer Protocol, and the codec of its
//! wire form GraphTP/1, for GraphQL requests and answers over a message
//! transport.
//!
//! A request is a head of 3 bytes, the credential and the content, and an
//! answer is a head of 2 bytes, status and substatus, and the body. The one
//! content coding is DEFLATE in the zlib format for a content or a body of
//! more than 128 bytes, >128 B, and the specification puts the encoder and the
//! decoder in every build, behind no feature.
//!
//! The modules [`version`], [`kind`], [`status`], [`head`], [`credential`],
//! [`body`] and [`sink`] hold the heads of both messages, the status registry,
//! the checks of the credential, the first byte rule and the sink every writer
//! puts its bytes into. The module [`deflate`] holds the DEFLATE core, its
//! streaming encoder and its decoder, and [`zlib`] the profile of `GraphTP`
//! over it with its header check, its bound and its codes. The module [`wire`]
//! holds the decoders of a request, its content, an answer and its body with
//! the 11 rules of refusal, and the writers of every message, which code a
//! content or a body at >128 B, and [`stream`] the handles, the batch number
//! and the writers and the reader of the stream bodies. The modules
//! [`request`] and [`response`] hold the owned request of an OPERATION and the
//! owned answer, generic over the body. The judges of the crate are the
//! notebooks `tests/notebooks/24_graphtp.ipynb` and
//! `tests/notebooks/37_content_coding.ipynb`, with the primitive `graphtp` of
//! `aosabi-devtool` as their door to the crate.
//!
//! The crate builds with no standard library and needs an allocator, takes no
//! outside crate and has no feature. There is no test in this crate, which is
//! the rule of CONTRIBUTING.md section 3.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

pub mod body;
pub mod credential;
pub mod deflate;
pub mod head;
pub mod kind;
pub mod request;
pub mod response;
pub mod sink;
pub mod status;
pub mod stream;
pub mod version;
pub mod wire;
pub mod zlib;
