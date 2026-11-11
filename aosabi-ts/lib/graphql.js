// SPDX-License-Identifier: MIT
/*
 * graphql.js: the entry aosabi/graphql of the package aosabi, in place of
 * graphql for an app of the TypeScript option.
 *
 * The entry holds GraphQLError, the class of aosabi-ts/lib/errors.js, which
 * stamps the brand the masking of the engine reads, so an error an app throws
 * is masked as Yoga masks it where the key maskedErrors of the configuration
 * turns the mask on. The class translates graphql-js 16.12.0,
 * src/error/GraphQLError.ts, under the licence MIT. The names an app reads off
 * the info of a resolver are types, which index.d.ts declares.
 */
export { GraphQLError } from "./errors.js";
