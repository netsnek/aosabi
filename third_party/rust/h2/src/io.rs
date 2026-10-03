// SPDX-License-Identifier: MIT
/*
 * h2: the traits of the transport the connection reads and writes, in place
 * of those of tokio.
 */
//! The traits of the transport a connection reads and writes.
//!
//! The codec reads frames through [`AsyncRead`] and writes them through
//! [`AsyncWrite`], the methods `h2` calls of the traits of `tokio` of the same
//! names, so a runtime that is not `tokio` implements these two and nothing
//! else. A read fills a [`ReadBuf`] over a slice that is initialized already,
//! so both sides stay in safe code.

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

/// A transport the connection reads bytes from.
pub trait AsyncRead {
    /// Reads into the unfilled part of `buf`, and answers ready once it read
    /// at least one byte, or none at the end of the stream.
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>>;
}

/// A transport the connection writes bytes to.
pub trait AsyncWrite {
    /// Writes a prefix of `buf` and answers the number of bytes written.
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>>;

    /// Flushes what the transport buffered.
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>>;

    /// Shuts the write half of the transport down.
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>>;

    /// Writes a prefix of the slices of `bufs`, by default the first slice
    /// that is not empty.
    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        let buf = bufs
            .iter()
            .find(|b| !b.is_empty())
            .map_or(&[][..], |b| &**b);
        self.poll_write(cx, buf)
    }

    /// Tells whether `poll_write_vectored` writes more than one slice.
    fn is_write_vectored(&self) -> bool {
        false
    }
}

impl<T: AsyncRead + Unpin + ?Sized> AsyncRead for &mut T {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut **self).poll_read(cx, buf)
    }
}

impl<T: AsyncWrite + Unpin + ?Sized> AsyncWrite for &mut T {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut **self).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut **self).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut **self).poll_shutdown(cx)
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut **self).poll_write_vectored(cx, bufs)
    }

    fn is_write_vectored(&self) -> bool {
        (**self).is_write_vectored()
    }
}

/// A buffer a read fills from its start, over a slice that is initialized.
#[derive(Debug)]
pub struct ReadBuf<'a> {
    buf: &'a mut [u8],
    filled: usize,
}

impl<'a> ReadBuf<'a> {
    /// Returns an empty buffer over `buf`.
    pub fn new(buf: &'a mut [u8]) -> ReadBuf<'a> {
        ReadBuf { buf, filled: 0 }
    }

    /// Returns the bytes read so far.
    pub fn filled(&self) -> &[u8] {
        &self.buf[..self.filled]
    }

    /// Returns the part of the buffer a read writes into next.
    pub fn unfilled_mut(&mut self) -> &mut [u8] {
        &mut self.buf[self.filled..]
    }

    /// Returns the number of bytes the buffer still takes.
    pub fn remaining(&self) -> usize {
        self.buf.len() - self.filled
    }

    /// Counts the first `n` bytes of the unfilled part as read.
    ///
    /// # Panics
    ///
    /// Panics when `n` is larger than [`remaining`](ReadBuf::remaining).
    pub fn advance(&mut self, n: usize) {
        assert!(n <= self.remaining(), "ReadBuf::advance past the end");
        self.filled += n;
    }

    /// Appends `src` to the bytes read.
    ///
    /// # Panics
    ///
    /// Panics when `src` is longer than [`remaining`](ReadBuf::remaining).
    pub fn put_slice(&mut self, src: &[u8]) {
        self.unfilled_mut()[..src.len()].copy_from_slice(src);
        self.filled += src.len();
    }
}

/// Writes the whole of `buf` to `io`, in place of `write_all` of the
/// `AsyncWriteExt` of `tokio`.
pub(crate) async fn write_all<T>(io: &mut T, mut buf: &[u8]) -> io::Result<()>
where
    T: AsyncWrite + Unpin + ?Sized,
{
    while !buf.is_empty() {
        let n = std::future::poll_fn(|cx| Pin::new(&mut *io).poll_write(cx, buf)).await?;
        if n == 0 {
            return Err(io::ErrorKind::WriteZero.into());
        }
        buf = &buf[n..];
    }
    Ok(())
}
