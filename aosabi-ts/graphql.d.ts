// SPDX-License-Identifier: MIT
/*
 * graphql.d.ts: the types of the entry aosabi/graphql of the package, the class GraphQLError of
 * graphql 16.12.0 for lib/graphql.js.
 *
 * This file re-exports the class and the types of its shape.
 */

export { GraphQLError } from './types/graphql/error/GraphQLError.js'

export type {
  GraphQLErrorExtensions,
  GraphQLErrorOptions,
  GraphQLFormattedError,
  GraphQLFormattedErrorExtensions,
} from './types/graphql/error/GraphQLError.js'
