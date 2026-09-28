// SPDX-License-Identifier: MIT
/*
 * graphql-over-http: a file of a form data body.
 *
 * Derived from node v24.13.0, deps/undici/src/lib/web/fetch/formdata-parser.js, MIT.
 */

//! `FileBody`, a file of a form data body.
//!
//! The parser makes a web `File` of every part that names a `filename`, with
//! the part's `Content-Type`, `text/plain` when it names none and the empty
//! string when it is not ASCII. The `File` constructor keeps a type of the
//! characters U+0020 to U+007E lowercased and drops any other, and its
//! `lastModified` is the time of its making in milliseconds. The file is
//! shared between threads, because it rides in the value of a variable.
//!
//! Under the feature `host-body` a file of a part the parser read in place
//! keeps no bytes of its own but the place of its bytes in the body of its
//! request, its field `part`, since the host of the TypeScript option hands
//! that body in once and reads the file at its place, so the body is the one
//! copy of the file in the engine. The bytes of such a file are none and its
//! size is the length of its part.

#[cfg(feature = "host-body")]
use core::ops::Range;
use std::sync::Arc;

/// A file of a form data body, the web `File` the parser makes.
#[derive(Clone, Debug, PartialEq)]
pub struct FileBody {
    /// `name`, the `filename` of the part.
    pub name: String,
    /// `type`, the normalized `Content-Type` of the part.
    pub r#type: String,
    /// `lastModified`, in milliseconds since the epoch.
    pub last_modified: f64,
    /// The bytes, none for a file that stands at its `part`.
    pub bytes: Arc<[u8]>,
    /// The place of the bytes of the file in the body of its request, under
    /// the feature `host-body`, or none for a file with bytes of its own.
    #[cfg(feature = "host-body")]
    pub part: Option<Range<usize>>,
}

impl FileBody {
    /// Creates `new File([bytes], name, { type })`.
    #[must_use]
    pub fn new(bytes: &[u8], name: &str, r#type: &str) -> Self {
        Self {
            name: String::from(name),
            r#type: normalized(r#type),
            last_modified: now_ms(),
            bytes: Arc::from(bytes),
            #[cfg(feature = "host-body")]
            part: None,
        }
    }

    /// Creates `new File([part], name, { type })` of the bytes at `part` of
    /// the body of its request, which the file keeps in place of the bytes.
    #[cfg(feature = "host-body")]
    #[must_use]
    pub fn in_part(part: Range<usize>, name: &str, r#type: &str) -> Self {
        Self {
            name: String::from(name),
            r#type: normalized(r#type),
            last_modified: now_ms(),
            bytes: Arc::from(&[][..]),
            part: Some(part),
        }
    }

    /// Returns `size`, the length of the bytes, or of the part of a file that
    /// stands in the body of its request.
    #[must_use]
    pub fn size(&self) -> usize {
        #[cfg(feature = "host-body")]
        if let Some(part) = &self.part {
            return part.len();
        }
        self.bytes.len()
    }
}

/// Returns `type` as the `File` constructor keeps it: lowercased where every
/// character is U+0020 to U+007E, and empty otherwise.
fn normalized(r#type: &str) -> String {
    if r#type.chars().all(|c| ('\u{20}'..='\u{7E}').contains(&c)) {
        r#type.to_ascii_lowercase()
    } else {
        String::new()
    }
}

/// Returns the milliseconds since the epoch, `Date.now()`.
///
/// A Worker reads no clock of the system from `std`, whose `SystemTime::now`
/// panics on `wasm32-unknown-unknown`, so the file of a Worker carries 0
/// until the glue of the Worker hands the host's time.
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
fn now_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |elapsed| (elapsed.as_secs_f64() * 1000.0).floor())
}

/// Returns 0, the time of a file of a Worker, see the other variant.
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
fn now_ms() -> f64 {
    0.0
}
