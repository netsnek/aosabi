// SPDX-License-Identifier: MIT
/*
 * aosabi-kernel cbor: the CBOR codec with its per operation integer key maps.
 *
 * No analogue in the reference.
 */

//! The module holds no item yet. What lands here is `encode`, `decode` and `KeyMap`: an
//! encoder beside the JSON writer, a decoder for the framed carriers, and the per
//! operation integer key map that the build step generates beside the SDL, so a field name
//! never travels over a link budgeted in kilobytes a second. The wire format is CBOR as
//! RFC 8949 defines it.
//!
//! [cbor]: https://www.rfc-editor.org/rfc/rfc8949
