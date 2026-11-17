// SPDX-License-Identifier: MIT
/*
 * graphql.d.ts: the types of the entry aosabi/graphql of the package, the class GraphQLError of
 * graphql 16.12.0 for lib/graphql.js.
 *
 * The folder types/graphql/ holds the declaration file of GraphQLError and the files it imports as
 * the package graphql 16.12.0 of the reference's pinned package tree ships them, as vendored
 * literals of graphql-js, MIT. This file re-exports the class and the types of its shape.
 */

export { GraphQLError } from './types/graphql/error/GraphQLError.js'

export type {
  GraphQLErrorExtensions,
  GraphQLErrorOptions,
  GraphQLFormattedError,
  GraphQLFormattedErrorExtensions,
} from './types/graphql/error/GraphQLError.js'
