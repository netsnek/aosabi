# SPDX-License-Identifier: MIT
"""Aosabi harness: the Python references of GraphTP over Homa.

The modules of this package implement what the notebooks compare the Rust
crates against, written from the specification and apart from the crates, so no
implementation is judged against itself. The module `graphtp` holds the
reference of GraphTP/1 that a notebook compares the crate graphtp against, the
status registry, the heads, the credential rules and the first byte rule with
its zlib header profile.

A notebook imports a module by its name, with the repository tests directory on
the path:

    from harness.homa import graphtp
"""
