# Bookshelf

A small service in Rust: a shelf of books in memory with two queries, a mutation, a route beside
`/graphql` and a middleware, all in [src/lib.rs](src/lib.rs). It is a cargo workspace of its own
that names the facade crate `aosabi` by its path in this repository.

From the root of the repository, build the command line once, then the service, and start it:

```sh
cargo build --locked -p aosabi-cli --bin cargo-aosabi
target/debug/cargo-aosabi build --release --locked --manifest-path examples/bookshelf/Cargo.toml
PORT=4000 examples/bookshelf/target/aosabi/dist/bookshelf-server
```

The build infers the schema from the types of the service and writes it to
`examples/bookshelf/target/aosabi/schema.graphql`. Ask the service from a second terminal:

```sh
curl -s localhost:4000/graphql -H 'content-type: application/json' \
  -d '{"query":"{ books(genre: SCIENCE) { title author year } }"}'
curl -s localhost:4000/health
```

```json
{"data":{"books":[{"title":"Opticks","author":"Isaac Newton","year":1704}]}}
{"status":"ok","books":3}
```
