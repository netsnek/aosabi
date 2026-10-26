# Roadmap

This folder is the public roadmap of Aosabi. It lists the work that comes after the release 0.1.0,
one file per area, and it holds the work alone: what is done stands in the
[CHANGELOG](../CHANGELOG.md) and in the [documentation](../Documentation/README.md).

An item names what it builds and why. It carries no date, because an item is done when the notebook
that judges it passes, and the order inside an area is the order the work is planned in. The five
[principles](../Documentation/principles.md) decide every item, and where an item weighs a cost on a
server against a cost on a device, the device comes first.

| Area | What comes |
| --- | --- |
| [engine.md](engine.md) | The single pass on its own, and a faster crossing from JavaScript |
| [embedded.md](embedded.md) | The engine on boards, device cards and the first runs on hardware |
| [graphtp.md](graphtp.md) | GraphTP on every carrier, in every client and in its own release |
| [http.md](http.md) | One GraphQL layer in place of two HTTP frameworks |
| [typescript.md](typescript.md) | More hosts, a published package and a core API of its own |
| [python.md](python.md) | A third language over the same engine |
| [security.md](security.md) | A FIPS build, upload bounds and signed device cards |
| [tooling.md](tooling.md) | Generated reference documentation, hosted tools and the command line |
| [compatibility.md](compatibility.md) | The parts of Pylon v3 the layer does not cover yet |

A proposal for the roadmap is an issue of the repository, and a change to it is a pull request that
edits the file of its area, as [CONTRIBUTING.md](../CONTRIBUTING.md) describes.
