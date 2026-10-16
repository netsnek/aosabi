// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool viewer wasm runner: the instantiation of a module and the
 * calls of its exports.
 *
 * A missing export, or an export that answers a non zero status, is refused
 * with AO-V0001.
 */

//! The runner.
