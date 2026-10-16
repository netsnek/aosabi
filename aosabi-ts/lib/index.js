// SPDX-License-Identifier: MIT
/*
 * index.js: the root aosabi of the package aosabi, which re-exports the entry
 * aosabi/pylon, lib/pylon.js, until the core layer comes another day.
 *
 * The names of the root stand in the entry aosabi/pylon, an app of the class
 * App over the real hono, and the root
 * re-exports them, so an import of the root reaches them as well.
 */
export * from "./pylon.js";
