// SPDX-License-Identifier: MIT
/*
 * aosabi-core: the run time methods of the marker File.
 */

//! The methods of [`File`].
//!
//! A `File` parameter of a resolver is the file of a multipart request, which the
//! `File` scalar passes untouched, so the resolver reads it as the web `File` the
//! form parser made. `name`, `size`, `type` and `lastModified` are properties of
//! the web `File` and methods here, and `type` is the raw name `r#type`. A marker
//! that holds no upload answers an empty file.

use std::sync::Arc;

use aosabi_execute::value::{
    HostValue,
    Value, //
};
use aosabi_http::fetch::body::utf8_decode;
use graphql_over_http::FileBody;

use crate::{
    descriptor::File,
    result::Result, //
};

/// The body of a file as a stream of chunks, `file.stream()`.
///
/// The body of a multipart part is whole when the resolver runs, so the
/// stream answers it as one chunk.
#[derive(Clone, Debug)]
pub struct FileStream {
    /// The bytes not yet read.
    bytes: Option<Arc<[u8]>>,
}

impl FileStream {
    /// Returns the next chunk, `None` at the end.
    pub fn next_chunk(&mut self) -> Option<Vec<u8>> {
        self.bytes
            .take()
            .filter(|bytes| !bytes.is_empty())
            .map(|bytes| bytes.to_vec())
    }
}

impl File {
    /// Returns the upload the marker holds.
    fn body(&self) -> Option<&FileBody> {
        match self.held() {
            Value::Host(HostValue::Opaque(object)) => object.downcast_ref::<FileBody>(),
            _ => None,
        }
    }

    /// Returns whether the argument carried an upload, a web `File`.
    ///
    /// The scalar `File` of the reference takes any value, and the reference
    /// reads a property of a value that is no `File`, a string a client sends
    /// in the place of a file among them, as `undefined`, where [`File::name`],
    /// `File::r#type` and [`File::size`] answer the empty string and 0. A port
    /// that writes a property of such a value reads this first.
    #[must_use]
    pub fn is_upload(&self) -> bool {
        self.body().is_some()
    }

    /// Returns the name of the file, `file.name`.
    #[must_use]
    pub fn name(&self) -> &str {
        self.body().map_or("", |body| body.name.as_str())
    }

    /// Returns the size of the file in bytes, `file.size`, the JavaScript
    /// number of the web `File`.
    #[must_use]
    pub fn size(&self) -> f64 {
        // A body a request carries is far below 2^53 bytes.
        #[expect(
            clippy::cast_precision_loss,
            reason = "a body a request carries is far below 2^53 bytes"
        )]
        let size = self.body().map_or(0, FileBody::size) as f64;
        size
    }

    /// Returns the media type of the file, `file.type`.
    #[must_use]
    pub fn r#type(&self) -> &str {
        self.body().map_or("", |body| body.r#type.as_str())
    }

    /// Returns the time of the last change in milliseconds, `file.lastModified`.
    #[must_use]
    pub fn last_modified(&self) -> f64 {
        self.body().map_or(0.0, |body| body.last_modified)
    }

    /// Returns the bytes of the file, `await file.arrayBuffer()`.
    ///
    /// # Errors
    ///
    /// Never, as the body is whole, and the result keeps the throw of the
    /// reference's promise at its place.
    #[expect(
        clippy::unused_async,
        reason = "the method is the asynchronous arrayBuffer of the web File"
    )]
    pub async fn array_buffer(&self) -> Result<Vec<u8>> {
        Ok(self
            .body()
            .map_or_else(Vec::new, |body| body.bytes.to_vec()))
    }

    /// Returns the text of the file decoded as UTF-8, `await file.text()`.
    ///
    /// # Errors
    ///
    /// Never, as for [`File::array_buffer`].
    #[expect(
        clippy::unused_async,
        reason = "the method is the asynchronous text of the web File"
    )]
    pub async fn text(&self) -> Result<String> {
        Ok(self
            .body()
            .map_or_else(String::new, |body| utf8_decode(&body.bytes)))
    }

    /// Returns the body of the file as a stream, `file.stream()`.
    #[must_use]
    pub fn stream(&self) -> FileStream {
        FileStream {
            bytes: self.body().map(|body| Arc::clone(&body.bytes)),
        }
    }
}
