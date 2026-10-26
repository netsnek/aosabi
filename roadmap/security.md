# Security

The defaults of this release are in the [security guide](../Documentation/guides/security.md). The
work here adds what a deployment needs beyond them, one opt in at a time.

1. A FIPS build as an opt in beside the lean default, with a FIPS validated provider under `rustls`,
   so a deployment that must use validated cryptography has a build for it. The default build keeps
   `ring`, and a TypeScript app keeps the cryptography of its host.
2. A bound of its own on uploads in a Rust service, beside the switch that turns uploads on.
3. Signed device cards with a trust store and a revocation list, so no code of a card runs on a
   build host without a trusted signature, as the [embedded roadmap](embedded.md) describes.
4. A build of the walk of the single pass alone for devices, so the code that reads untrusted bytes
   on a board is the walk and its lexer and nothing more.
