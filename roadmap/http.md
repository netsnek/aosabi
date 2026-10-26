# HTTP

A request over HTTP passes the native server, a router with its middleware and a layer of GraphQL
over HTTP before the engine sees the query. The work here makes that path shorter, with no answer
changed that a client relies on.

1. An inventory of every plugin, hook, option, header, status and error text that a request can
   reach on the path, each mapped to a place in the new path or to a named difference.
2. An oracle that sends each request of the test suite to the old path and to the new one and
   compares the answers byte for byte, so nothing is missed.
3. One function for GraphQL over HTTP in place of the layer of today: it reads the request, refuses
   what a GraphQL server refuses, answers with the status and the type the client accepts, masks
   unexpected errors and keeps the request cache.
4. GraphTP calling the hook layer and the engine directly, with no HTTP request in between, while
   the middleware of an app keeps running on a GraphTP call.
5. A small router of Aosabi's own for Rust services, with the API a Rust service writes today, and
   no router in `core.wasm`, since the host routes a TypeScript app.
6. The cost of a request measured on each layer before and after, on Workers, Bun, a native card and
   the ESP32-C3.
