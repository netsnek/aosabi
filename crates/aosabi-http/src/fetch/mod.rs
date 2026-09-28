// SPDX-License-Identifier: MIT
/*
 * aosabi-http: the Fetch types a Node request and a Node answer carry.
 *
 * No analogue on the reference side. The types follow the Fetch Standard,
 * https://fetch.spec.whatwg.org/, as the Node adapter of hono fills them, and
 * each file of the directory names what it translates.
 */

//! The Fetch types of a served request and its answer.

pub mod body;
pub mod headers;
pub mod request;
pub mod response;
pub mod url;
