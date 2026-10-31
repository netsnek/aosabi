// SPDX-License-Identifier: MIT
/*
 * aosabi-kernel cbor: the CBOR codec, withdrawn.
 *
 * No analogue in the reference. The codec was to carry the operations and the
 * results of the framed carriers in CBOR with per operation integer key maps,
 * and it was withdrawn with the closed executor and the persisted document
 * table.
 */

//! The CBOR codec, withdrawn.
//!
//! The module holds no item. The transport is JSON, compressed where a
//! carrier compresses it, and CBOR is withdrawn and not postponed, so no
//! encoder, no decoder and no key map lands here.
