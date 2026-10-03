// SPDX-License-Identifier: MIT
/*
 * aosabi-serve: the provider over the file system that serve_static of
 * aosabi-http reads a static file through.
 *
 * Derived from hono 1.19.9, dist/serve-static.js, MIT.
 */

//! The file system provider of `serve_static`.
//!
//! The Node adapter of hono serves a file through `node:fs` and joins paths
//! with `node:path`. [`FsContent`] is that platform half over `std::fs` for
//! the middleware of `aosabi-http`, which holds the steps. Its `stat` is
//! `fs.statSync`, which follows a link, with the size, the kind and the birth
//! time of [`std::fs::Metadata::created`], its `open` is
//! `fs.createReadStream` with a seek to the first offset of a range, and its
//! `join` is `path.join` of Node on a POSIX platform,
//! [`aosabi_http::path::join`]. The function [`serve_static`] is the
//! `serveStatic` of the adapter, which warns at construction where the root
//! does not exist and binds the provider.
//!
//! The stream of a file answers its first poll on a later turn, as the first
//! read of `fs.createReadStream` runs on the thread pool of libuv and is not
//! within the two ticks the adapter waits for a chunk. A read takes up to 64
//! KiB, the `highWaterMark` of a file stream. A file that cannot be opened or
//! read fails the stream with the message of the operating system, where Node
//! writes the code of libuv and the call, and the adapter ends the answer on
//! either side.
//!
//! [streams.js]: https://github.com/nodejs/node/blob/v24.13.0/lib/internal/fs/streams.js

use core::{
    pin::Pin,
    task::{
        Context as TaskContext,
        Poll, //
    }, //
};
use std::{
    fs::{
        self,
        File, //
    },
    io::{
        self,
        Read,
        Seek,
        SeekFrom, //
    },
    path::Path, //
};

use aosabi_http::{
    Body,
    BodyStream,
    Context,
    LocalBoxFuture,
    Next,
    Response,
    Stream,
    fetch::body::{
        BodyError,
        Chunk, //
    },
    middleware::serve_static::{
        self as web_static,
        Content,
        ServeStaticOptions,
        Stat, //
    },
    web_app::RouteError, //
};

/// The bytes of one read of a file stream, the `highWaterMark` of Node.
const HIGH_WATER_MARK: usize = 64 * 1024;

/// The provider of `serve_static` over the file system of the process.
///
/// A relative path resolves against the working directory, as the root of the
/// adapter does.
#[derive(Clone, Copy, Debug, Default)]
pub struct FsContent;

impl Content for FsContent {
    /// Returns `fs.statSync(path)`, `None` where it throws.
    ///
    /// A platform that knows no birth time answers none.
    fn stat(&self, path: &str) -> Option<Stat> {
        let metadata = fs::metadata(path).ok()?;
        Some(Stat {
            size: metadata.len(),
            is_dir: metadata.is_dir(),
            birth_time: metadata.created().ok(),
        })
    }

    /// Returns the stream `fs.createReadStream(path, { start, end })` reads.
    ///
    /// The file is opened at the first read, so the provider always answers
    /// a body, and a file that cannot be opened fails the stream.
    fn open(&self, path: &str, range: Option<(u64, u64)>) -> Option<Body> {
        let stream = ReadStream {
            path: String::from(path),
            range,
            state: ReadState::Unpolled,
        };
        Some(Body::Stream(BodyStream::new(stream)))
    }

    /// Returns `path.join(base, path)` of Node on a POSIX platform.
    fn join(&self, base: &str, path: &str) -> String {
        aosabi_http::path::join(&[base, path])
    }
}

/// Returns the middleware `serveStatic(options)` of the Node adapter, the
/// middleware of `aosabi-http` over [`FsContent`].
///
/// A non empty root that does not exist prints
/// `serveStatic: root path '<root>' is not found, are you sure it's correct?`
/// to stderr once, when the middleware is made.
pub fn serve_static<E: RouteError>(
    options: ServeStaticOptions,
) -> impl Fn(Context, Next<E>) -> LocalBoxFuture<'static, Result<Response, E>> + 'static {
    if !options.root.is_empty() && !Path::new(&options.root).exists() {
        eprintln!(
            "serveStatic: root path '{root}' is not found, are you sure it's correct?",
            root = options.root
        );
    }
    web_static::serve_static(options, FsContent)
}

/// The stream of one file, `fs.createReadStream`.
struct ReadStream {
    /// The path of the file.
    path: String,
    /// The first and the last offset of the read, both included, where a
    /// range bounds it.
    range: Option<(u64, u64)>,
    /// Where the read stands.
    state: ReadState,
}

/// Where the read of a [`ReadStream`] stands.
enum ReadState {
    /// The stream has not been polled.
    Unpolled,
    /// The stream has been polled and the file is not open.
    Unopened,
    /// The file is open, and `left` is the count of bytes the range still
    /// allows, `u64::MAX` where none bounds the read.
    Reading {
        /// The open file.
        file: File,
        /// The bytes the read may still take.
        left: u64,
    },
    /// The read ended or failed.
    Ended,
}

impl Stream for ReadStream {
    type Item = Chunk;

    fn poll_next(self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Option<Chunk>> {
        let stream = self.get_mut();
        if let ReadState::Unpolled = stream.state {
            stream.state = ReadState::Unopened;
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }
        Poll::Ready(stream.read_chunk())
    }
}

impl ReadStream {
    /// Returns the next chunk of the file, `None` at its end.
    // A read into a buffer left unwritten would skip the zeroing. A memory
    // map of the file would hand its pages on with no copy.
    fn read_chunk(&mut self) -> Option<Chunk> {
        if let ReadState::Unopened = self.state {
            match self.open_file() {
                Ok(reading) => self.state = reading,
                Err(error) => {
                    self.state = ReadState::Ended;
                    return Some(Err(BodyError::Failed(error.to_string())));
                }
            }
        }
        let ReadState::Reading { file, left } = &mut self.state else {
            return None;
        };
        let want = usize::try_from(*left).map_or(HIGH_WATER_MARK, |left| left.min(HIGH_WATER_MARK));
        let mut buffer = vec![0; want];
        let read = loop {
            match file.read(&mut buffer) {
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                read => break read,
            }
        };
        match read {
            Ok(0) => {
                self.state = ReadState::Ended;
                None
            }
            Ok(count) => {
                buffer.truncate(count);
                *left -= u64::try_from(count).unwrap_or(*left);
                Some(Ok(buffer))
            }
            Err(error) => {
                self.state = ReadState::Ended;
                Some(Err(BodyError::Failed(error.to_string())))
            }
        }
    }

    /// Opens the file at the first offset of the range.
    fn open_file(&self) -> io::Result<ReadState> {
        let mut file = File::open(&self.path)?;
        let left = match self.range {
            Some((start, end)) => {
                file.seek(SeekFrom::Start(start))?;
                end - start + 1
            }
            None => u64::MAX,
        };
        Ok(ReadState::Reading { file, left })
    }
}
