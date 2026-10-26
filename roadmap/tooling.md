# Tooling

The command line builds, serves, inspects and scaffolds a service today. The work here adds the
tools that leave the build host with a service and the tools a developer uses beside it.

1. Reference documentation generated at build time: `aosabi docs` writes an API reference of the
   schema in Markdown and in Typst with diagrams of the type graph, the PDF on request, with no
   browser, no Node.js and no network, deterministic enough that the test suite compares its bytes.
2. The schema viewer aoviewer and GraphiQL as static pages that run in the browser and reach a
   service by URL, so no build of a service carries a development page.
3. A runtime endpoint of the Model Context Protocol beside the one `aosabi mcp` serves over standard
   input and output today.
4. A client generated from the schema of a service for the TypeScript layer.
