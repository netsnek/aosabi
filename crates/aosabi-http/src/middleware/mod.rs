// SPDX-License-Identifier: MIT
/*
 * aosabi-http: the middleware of hono, serveStatic and
 * cors, which a service registers itself, and csrf and bodyLimit, which the
 * base pipeline of a Rust service installs.
 *
 * No file of its own on the reference side. Each file of the directory names
 * the file of hono it translates.
 */

//! The middleware of hono, `serve_static` and `cors`,
//! which a service registers itself, and `csrf` and `body_limit`, which the
//! base pipeline of a Rust service installs, the
//! divergences D70 and D76 of `docs/byte-equality.md`.

pub mod body_limit;
pub mod combine;
pub mod compress;
pub mod cors;
pub mod csrf;
pub mod serve_static;
