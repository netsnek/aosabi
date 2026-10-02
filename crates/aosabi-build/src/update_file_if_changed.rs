// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-build: the write that leaves an unchanged file alone.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/cli/builder/update-file-if-changed.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 */

//! The write that leaves an unchanged file alone.
//!
//! Every artifact of the build goes through this one function, so a build
//! that infers the same schema touches no modification time, and whatever
//! watches an artifact sees a change only when the bytes changed.

use std::{
    fs,
    io,
    path::Path, //
};

/// Writes `new_content` to `path` unless the file already holds those bytes.
///
/// The answer is `true` when the file was created or rewritten and `false`
/// when it was left alone. The reference reads the file as `utf8` text and
/// compares two strings, and this function compares bytes. For well formed
/// text the two comparisons decide alike. They part on a file that holds ill
/// formed bytes, which the decoder of the reference reads as U+FFFD and
/// which counts as changed here, so the file is rewritten.
///
/// [Node.js buffers and character encodings]: https://nodejs.org/api/buffer.html#buffers-and-character-encodings
///
/// # Errors
///
/// Returns every error of the read except the one of a missing file, which
/// is the `ENOENT` test of the reference, and every error of the write.
pub fn update_file_if_changed(path: &Path, new_content: &[u8]) -> io::Result<bool> {
    match fs::read(path) {
        // No update needed.
        Ok(current_content) if current_content == new_content => return Ok(false),
        Ok(_) => {}
        // Ignore file not found error.
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }

    fs::write(path, new_content)?;
    // File created or updated.
    Ok(true)
}
