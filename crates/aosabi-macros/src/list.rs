// SPDX-License-Identifier: MIT
/*
 * aosabi-macros: the declaration list of the target a macro expands in.
 *
 * No analogue on the reference side.
 */

//! The declaration list of the target a macro expands in.
//!
//! The build script of an application crate writes one list per target of the
//! package into `OUT_DIR/aosabi_declarations/`. A macro runs inside rustc, and
//! cargo gives rustc the environment of the target it compiles: `OUT_DIR` for
//! every target of a package with a build script, `CARGO_BIN_NAME` for
//! binaries and binary examples, `CARGO_TARGET_TMPDIR` for integration tests
//! and benchmarks, and `CARGO_CRATE_NAME` for every target. The name of the
//! list follows from them, `bin-<name>.rs`, `test-<crate>.rs` or
//! `lib-<crate>.rs`, as the build script names it, so the library and an
//! example of a library crate type each read their own list.
//!
//! The build script writes a list for every target it finds, so a missing list
//! in a crate whose build script writes lists is a target the script did not
//! see. Cargo runs the script again when the manifest, a file it read or a
//! directory of the target auto discovery that existed at its last run
//! changes, so a target in a directory made after that run, or one the
//! manifest states in a form the script does not read, has no list. The macro
//! refuses it, because without its list the derive would declare nothing and
//! the type would drop out of the heritage sweep silently.
//!
//! A doc test is a crate of its own that rustdoc compiles with the variables
//! of the library and with `UNSTABLE_RUSTDOC_TEST_PATH` set, so a macro there
//! reads no list.
//!
//! [the cargo book on environment variables]: https://doc.rust-lang.org/cargo/reference/environment-variables.html

use std::{
    env,
    path::PathBuf, //
};

/// The directory of the lists inside `OUT_DIR`, which the build script writes
/// as well.
const DECLARATIONS_DIR: &str = "aosabi_declarations";

/// The build script of the crate writes declaration lists and wrote none for
/// the target a macro expands in.
pub(crate) struct Missing;

/// Returns the path of the declaration list of the target this expansion
/// compiles, or `None` when the crate writes no lists.
///
/// A crate with no build script, or with one that does not write the lists,
/// has no list, and its macros emit no declaration. The build then finds no
/// declaration of a root of that crate and names the missing build script.
///
/// # Errors
///
/// Returns [`Missing`] when `OUT_DIR/aosabi_declarations/` exists and holds no
/// list of this target.
pub(crate) fn path() -> Result<Option<String>, Missing> {
    if env::var_os("UNSTABLE_RUSTDOC_TEST_PATH").is_some() {
        return Ok(None);
    }
    let Some(out_dir) = env::var_os("OUT_DIR") else {
        return Ok(None);
    };
    let name = if let Ok(bin) = env::var("CARGO_BIN_NAME") {
        format!("bin-{bin}.rs")
    } else {
        let Ok(krate) = env::var("CARGO_CRATE_NAME") else {
            return Ok(None);
        };
        if env::var_os("CARGO_TARGET_TMPDIR").is_some() {
            format!("test-{krate}.rs")
        } else {
            format!("lib-{krate}.rs")
        }
    };
    let lists = PathBuf::from(out_dir).join(DECLARATIONS_DIR);
    let list = lists.join(name);
    if list.is_file() {
        return Ok(list.into_os_string().into_string().ok());
    }
    if lists.is_dir() {
        return Err(Missing);
    }
    Ok(None)
}
