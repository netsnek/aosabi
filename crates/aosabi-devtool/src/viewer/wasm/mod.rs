// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool viewer wasm: the runner of the committed wasm module.
 *
 * The flag --wasm answers an operation through the exports of the committed
 * module in place of the linked core, so the same call judges the shipped
 * bytes.
 */

//! The runner of the committed module.

mod info;
mod runner;
