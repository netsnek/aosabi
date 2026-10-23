# Examples

Each example is small and runs as its README says.

| Example | Holds |
| --- | --- |
| [bookshelf](bookshelf) | A service in Rust with queries, a mutation, a route and a middleware |
| [worker](worker) | The smallest service in TypeScript, on Bun and as a Cloudflare Worker |

The script [check.sh](check.sh) builds and runs every example from the root of the repository and
checks each answer against the README of its example:

```sh
sh examples/check.sh
```
