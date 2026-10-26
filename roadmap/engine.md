# The engine

The engine parses, validates and executes every request, and it is the part every build links. The
work here makes the single pass stand on its own and takes cost out of the path of a request.

## 1. The single pass on its own

Today the single pass accepts a valid document in one walk and hands every other document to the 27
validation rules, which write the errors. The next steps let a device build the walk alone:

1. A crate of its own for the walk, with a lexer that needs no atomic operations and no standard
   library, so it reaches the ESP32-C3, whose core has neither.
2. The errors of each rule written by the walk itself, with their locations counted in UTF-16 code
   units as graphql-js counts them, so a build needs no rule at all.
3. Fewer reasons to defer, so the walk decides more documents alone.
4. A build setting that chooses the walk without the rules, beside the two settings a build has
   today.
5. An executor for devices that runs over the plan the walk builds, so a device needs no syntax tree
   for a document it accepts.

## 2. The crossing between JavaScript and the engine

On Cloudflare Workers and on Bun, a TypeScript app hands each request, each header and each string
to the engine across the boundary of the module. The crossing costs time that a native build does
not pay, and the work brings it down: fewer records per request, fewer buffers, and the string
builtins of the WebAssembly engine where the host offers them.

## 3. Speed, measured against graphql-js

Every change of the engine that claims speed carries its notebook, and the numbers the documentation
quotes are measured again against graphql-js 16.12.0 on the same machine whenever the engine changes
its path.
