# SPDX-License-Identifier: MIT
"""Aosabi harness: the Python references of GraphTP over Homa.

The modules of this package implement what the notebooks compare the Rust
crates against, written from the specification and apart from the crates, so no
implementation is judged against itself. The module `graphtp` holds the
reference of GraphTP/1 that 24_graphtp compares the crate graphtp against, the
status registry, the heads, the credential rules and the first byte rule with
its zlib header profile, from docs/graphtp.md sections 2.5, 3.2, 3.4, 5.1 to
5.3 and 5.5.

A notebook imports a module by its name, with the repository tests directory on
the path:

    from harness.homa import graphtp
"""
