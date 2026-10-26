# Compatibility with Pylon v3

The compatibility layer `aosabi/pylon` runs Pylon v3 apps on the Aosabi engine, and the
[guide](../Documentation/guides/pylon.md) says what it covers today. The work here closes the parts
it does not cover yet, while the core of Aosabi sheds what only Pylon v3 apps need.

1. The subpaths `db` and `db/plugin` of Pylon v3, its models and their database plugin, with the
   `db` command of the command line.
2. The subpaths `queues`, `query` and `ir`.
3. Federation of `createGateway` in the TypeScript layer, beside the federation of the Rust layer of
   today.
4. The in-context directive, global ids and the process roles kept in the compatibility layer alone,
   so a service that does not ask for them carries none of them.
5. The hooks of a plugin that only a Pylon v3 app sees kept in the compatibility layer, with the
   hook layer of every carrier running all of them for the apps that use them.

Server side rendering of React pages, the `pages` subpaths of Pylon v3, stays out of scope for good,
because Aosabi serves APIs to headless frontends.
