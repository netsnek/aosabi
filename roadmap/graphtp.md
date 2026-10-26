# GraphTP

GraphTP carries GraphQL where HTTP does not fit, and it is specified apart from Aosabi so any server
and client can speak it. The work here takes it onto every carrier and into every client.

1. The crate `graphtp` and the npm package `graphtp` released on their own registries, with the
   specification beside them, so another server or client can implement GraphTP/1 from the
   specification and its test vectors alone.
2. The links of the npm package for serial lines, USB and Bluetooth in Node.js, and for Web Serial,
   WebUSB and Web Bluetooth in a browser, so a page reaches a device with `graphtpFetch`.
3. A gateway on the desktop that serves a device over GraphTP to the HTTP clients of the same
   machine, so every GraphQL tool reaches a device with no change.
4. A client of GraphTP for Rust programs that parses no GraphQL and holds no schema.
5. GraphTP over Homa through a kernel module of Homa on a data centre network, beside the raw IP of
   today.
6. The carriers of a board served by the GraphTP server of the engine on the board, as the [embedded
   roadmap](embedded.md) builds them.
