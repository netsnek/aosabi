// SPDX-License-Identifier: MIT
/*
 * aosabi-cli: the digests of the two base images of the container files of
 * the docker card.
 *
 * No analogue in the reference. Each digest is the digest of the image index
 * its tag names, read from the header Docker-Content-Digest of the answers to
 * https://registry-1.docker.io/v2/library/rust/manifests/1.95.0-slim-bookworm
 * and https://gcr.io/v2/distroless/cc-debian12/manifests/nonroot, and podman
 * 5.8.2 lists each among the RepoDigests of the image it pulled for that tag
 * the same day. An index resolves to the image of the architecture that
 * builds, which on the aarch64 host of that day is
 * sha256:d81582a30689ffafaa04e2a9e117d39cc5032cb12eee8d6a5f26b1f215c1e3e8 for
 * the builder and
 * sha256:9ae6393649d0c55122c66fe6d4803b0d85b2d7b60b2b09b690ce80c086f40a83 for
 * the runtime, so one Containerfile builds on every host.
 */

//! The base image digests of the `docker` card.

/// The image of the builder stage, the slim Rust image of the pinned toolchain.
pub const BUILDER_IMAGE: &str = "docker.io/library/rust:1.95.0-slim-bookworm";

/// The toolchain of [`BUILDER_IMAGE`], whose component `rust-src` the builder
/// stage of a build at the switch `AOSABI_STD` at `source` adds.
pub const BUILDER_TOOLCHAIN: &str = "1.95.0";

/// The digest of the image index of [`BUILDER_IMAGE`].
pub const BUILDER_DIGEST: &str =
    "sha256:d7482085ff5b415f84dba5647ae71606650bdef00db7aeb69f4b3d170c3e4082";

/// The image of the runtime stage, distroless with the C runtime and a user that is not root.
pub const RUNTIME_IMAGE: &str = "gcr.io/distroless/cc-debian12:nonroot";

/// The digest of the image index of [`RUNTIME_IMAGE`].
pub const RUNTIME_DIGEST: &str =
    "sha256:9dac0a79194e45a7da0158a9c6da57b217585af0786db3845d1f0ec1a0dd182f";
