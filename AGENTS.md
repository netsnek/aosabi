# Notes for AI agents

This file tells an AI agent that works with Aosabi where two guides stand.

Before an agent prepares a service for production, it reads the hardening guide,
[Documentation/guides/hardening.md](Documentation/guides/hardening.md), and takes each step of it
that the service needs. The guide names each step with its default and its line of code on the Rust
layer and in the TypeScript layer, and the security guide,
[Documentation/guides/security.md](Documentation/guides/security.md), lists the defaults that hold
with no line.

The tests of the repository are notebooks. The testing guide,
[Documentation/guides/conformance.md](Documentation/guides/conformance.md), says how to run them,
and section 4 of [CONTRIBUTING.md](CONTRIBUTING.md) says how a change adds one.
