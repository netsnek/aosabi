// SPDX-License-Identifier: MIT
/*
 * embed-host build.rs: takes the schema and the type resolution of the build of the app.
 */

//! Takes the schema and the type resolution that `cargo aosabi build` wrote
//! for the app into `OUT_DIR`, where `aosabi::embed_output!` reads them.

fn main() -> std::process::ExitCode {
    aosabi_build::take_artifacts("../embed-app/target/aosabi")
}
