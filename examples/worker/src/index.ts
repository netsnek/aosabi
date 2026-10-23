// SPDX-License-Identifier: MIT

import {App} from 'aosabi/pylon'

// The build infers the field `hello(name: String): String!` from this signature.
export default new App({
  graphql: {
    Query: {
      hello: (name?: string): string => `Hello, ${name ?? 'world'}`,
    },
  },
})
