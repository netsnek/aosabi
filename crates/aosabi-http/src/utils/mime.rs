// SPDX-License-Identifier: MIT
/*
 * aosabi-http: the table of the media types of hono by file extension.
 *
 * Derived from hono 4.11.9, dist/utils/mime.js, MIT.
 */

//! The media types by file extension.
//!
//! [`MIMES`] is the table `baseMimes` of hono, which the module exports as
//! `mimes`, [`get_mime_type`] reads the type of a file name from it or from a
//! table of the caller, and [`get_extension`] reads the extension of a type.
//! The file of hono 4.11.4, the second copy the lock installs, is the same
//! byte for byte, so which copy the Node adapter loads changes no byte.
//!
//! The table of the reference is an object, so a lookup also reads the
//! properties every object inherits from `Object.prototype`. An extension
//! that names one of them, such as `toString`, finds a function, and the test
//! `mimeType.startsWith("text")` throws the `TypeError` of V8.
//! [`get_mime_type`] answers that throw as [`InheritedProperty`].

use core::fmt;

/// The media type of every extension hono knows, in the order of the
/// reference.
pub const MIMES: [(&str, &str); 56] = [
    ("aac", "audio/aac"),
    ("avi", "video/x-msvideo"),
    ("avif", "image/avif"),
    ("av1", "video/av1"),
    ("bin", "application/octet-stream"),
    ("bmp", "image/bmp"),
    ("css", "text/css"),
    ("csv", "text/csv"),
    ("eot", "application/vnd.ms-fontobject"),
    ("epub", "application/epub+zip"),
    ("gif", "image/gif"),
    ("gz", "application/gzip"),
    ("htm", "text/html"),
    ("html", "text/html"),
    ("ico", "image/x-icon"),
    ("ics", "text/calendar"),
    ("jpeg", "image/jpeg"),
    ("jpg", "image/jpeg"),
    ("js", "text/javascript"),
    ("json", "application/json"),
    ("jsonld", "application/ld+json"),
    ("map", "application/json"),
    ("mid", "audio/x-midi"),
    ("midi", "audio/x-midi"),
    ("mjs", "text/javascript"),
    ("mp3", "audio/mpeg"),
    ("mp4", "video/mp4"),
    ("mpeg", "video/mpeg"),
    ("oga", "audio/ogg"),
    ("ogv", "video/ogg"),
    ("ogx", "application/ogg"),
    ("opus", "audio/opus"),
    ("otf", "font/otf"),
    ("pdf", "application/pdf"),
    ("png", "image/png"),
    ("rtf", "application/rtf"),
    ("svg", "image/svg+xml"),
    ("tif", "image/tiff"),
    ("tiff", "image/tiff"),
    ("ts", "video/mp2t"),
    ("ttf", "font/ttf"),
    ("txt", "text/plain"),
    ("wasm", "application/wasm"),
    ("webm", "video/webm"),
    ("weba", "audio/webm"),
    ("webmanifest", "application/manifest+json"),
    ("webp", "image/webp"),
    ("woff", "font/woff"),
    ("woff2", "font/woff2"),
    ("xhtml", "application/xhtml+xml"),
    ("xml", "application/xml"),
    ("zip", "application/zip"),
    ("3gp", "video/3gpp"),
    ("3g2", "video/3gpp2"),
    ("gltf", "model/gltf+json"),
    ("glb", "model/gltf-binary"),
];

/// The names of the properties of `Object.prototype` that the expression of
/// an extension can match, each a function.
///
/// The extension is one or more ASCII letters and digits, so `__proto__`,
/// `__defineGetter__`, `__defineSetter__`, `__lookupGetter__` and
/// `__lookupSetter__`, whose names hold an underscore, cannot be one.
const INHERITED: [&str; 7] = [
    "constructor",
    "hasOwnProperty",
    "isPrototypeOf",
    "propertyIsEnumerable",
    "toLocaleString",
    "toString",
    "valueOf",
];

/// The `TypeError` `getMimeType` throws where the lookup reads a property
/// every object inherits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InheritedProperty;

impl fmt::Display for InheritedProperty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("mimeType.startsWith is not a function")
    }
}

impl core::error::Error for InheritedProperty {}

/// Returns the media type of `filename` in `mimes`, with `; charset=utf-8`
/// after a type that starts with `text`.
///
/// The extension is what follows the last `.` when it is one or more ASCII
/// letters and digits, which is what `/\.([a-zA-Z0-9]+?)$/` matches, and it
/// is looked up with its case. `None` is the `undefined` of a name with no
/// such extension or of an extension the table does not hold. The reference
/// calls the function with [`MIMES`] where its caller gives no table.
///
/// # Errors
///
/// Returns [`InheritedProperty`] when the extension is not in `mimes` and
/// names a property of `Object.prototype`.
pub fn get_mime_type<K, V>(
    filename: &str,
    mimes: &[(K, V)],
) -> Result<Option<String>, InheritedProperty>
where
    K: AsRef<str>,
    V: AsRef<str>,
{
    let Some(extension) = extension(filename) else {
        return Ok(None);
    };
    let found = mimes
        .iter()
        .find(|(key, _)| key.as_ref() == extension)
        .map(|(_, mime_type)| mime_type.as_ref());
    let Some(mime_type) = found else {
        if INHERITED.contains(&extension) {
            return Err(InheritedProperty);
        }
        return Ok(None);
    };
    if mime_type.starts_with("text") {
        return Ok(Some(format!("{mime_type}; charset=utf-8")));
    }
    Ok(Some(String::from(mime_type)))
}

/// Returns the first extension of [`MIMES`] whose type is `mime_type`.
///
/// The loop `for...in` visits the keys in the order of the table, since none
/// of them is an array index.
#[must_use]
pub fn get_extension(mime_type: &str) -> Option<&'static str> {
    MIMES
        .iter()
        .find(|(_, candidate)| *candidate == mime_type)
        .map(|(extension, _)| *extension)
}

/// Returns the extension the expression `/\.([a-zA-Z0-9]+?)$/` captures in
/// `filename`.
///
/// The class holds no `.`, so a match ends at the end of the name and
/// starts after its last `.`.
fn extension(filename: &str) -> Option<&str> {
    let (_, extension) = filename.rsplit_once('.')?;
    let alphanumeric =
        !extension.is_empty() && extension.bytes().all(|byte| byte.is_ascii_alphanumeric());
    alphanumeric.then_some(extension)
}
