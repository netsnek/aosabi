// SPDX-License-Identifier: MIT
/*
 * aosabi-build: the two directories a build writes into.
 *
 * No file of the reference is translated here.
 */

//! The two directories a build writes into.
//!
//! The artifact directory is where a person, the devtool and the harness read
//! the artifacts, and it is `target/aosabi/` of the workspace that builds.
//! The second directory is `OUT_DIR` of the build script, which is the only
//! place a crate may embed a generated file from.
//!
//! [the cargo book on build scripts]: https://doc.rust-lang.org/cargo/reference/build-scripts.html

use std::{
    env,
    ffi::OsString,
    path::{
        Path,
        PathBuf, //
    }, //
};

use crate::error::BuildError;

/// The environment variable that names the artifact directory.
///
/// It is the route for a build whose target directory carries no
/// `CACHEDIR.TAG`, and for a run outside a build script, where cargo sets no
/// `OUT_DIR`.
pub const ARTIFACT_DIR_ENV: &str = "AOSABI_ARTIFACT_DIR";

/// The environment variable cargo sets for a build script.
const OUT_DIR_ENV: &str = "OUT_DIR";

/// The file cargo writes into a target directory it creates.
const CACHEDIR_TAG: &str = "CACHEDIR.TAG";

/// The directory name of the artifacts inside a target directory.
const ARTIFACT_DIR_NAME: &str = "aosabi";

/// Names where a build writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactDirs {
    /// The directory of the three artifacts. The build creates it.
    pub artifact_dir: PathBuf,
    /// The directory of the schema copy a server binary embeds, which is
    /// `OUT_DIR`, or `None` when the run is no build script.
    pub out_dir: Option<PathBuf>,
}

impl ArtifactDirs {
    /// Reads the two directories from the environment of a build script.
    ///
    /// [`ARTIFACT_DIR_ENV`] names the artifact directory when it is set to a
    /// value that is not empty. A relative value resolves against the working
    /// directory, which for a build script is the directory of its package.
    /// Otherwise the artifact directory is `aosabi` inside the first ancestor
    /// of `OUT_DIR` that holds `CACHEDIR.TAG`, the file cargo writes into a
    /// target directory it creates.
    ///
    /// # Errors
    ///
    /// Returns [`BuildError::NoArtifactDir`] when the variable is not set and
    /// `OUT_DIR` is not set or has no such ancestor.
    ///
    /// [the cargo book on the build cache]: https://doc.rust-lang.org/cargo/reference/build-cache.html
    pub fn from_env() -> Result<Self, BuildError> {
        let out_dir = non_empty(env::var_os(OUT_DIR_ENV)).map(PathBuf::from);
        let artifact_dir = match non_empty(env::var_os(ARTIFACT_DIR_ENV)) {
            Some(named) => PathBuf::from(named),
            None => below_target_dir(out_dir.as_deref())?,
        };

        Ok(Self {
            artifact_dir,
            out_dir,
        })
    }
}

/// Returns `value` unless it is empty, because an empty path names nothing.
fn non_empty(value: Option<OsString>) -> Option<OsString> {
    value.filter(|value| !value.is_empty())
}

/// Returns the artifact directory inside the target directory of `out_dir`.
///
/// # Errors
///
/// Returns [`BuildError::NoArtifactDir`] with what the route lacks.
fn below_target_dir(out_dir: Option<&Path>) -> Result<PathBuf, BuildError> {
    let Some(out_dir) = out_dir else {
        return Err(BuildError::NoArtifactDir(format!(
            "{OUT_DIR_ENV} is not set"
        )));
    };
    let target_dir = out_dir
        .ancestors()
        .find(|ancestor| ancestor.join(CACHEDIR_TAG).is_file());

    match target_dir {
        Some(target_dir) => Ok(target_dir.join(ARTIFACT_DIR_NAME)),
        None => Err(BuildError::NoArtifactDir(format!(
            "no ancestor of {OUT_DIR_ENV} holds {CACHEDIR_TAG}"
        ))),
    }
}
