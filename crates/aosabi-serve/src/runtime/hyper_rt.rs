// SPDX-License-Identifier: MIT
/*
 * aosabi-serve: the runtime traits of hyper over the runtime.
 *
 * No analogue on the reference side.
 */

//! `HyperIo`, `RuntimeTimer`, `RuntimeSleep` and `LocalExec`.
//!
//! A read lands in a buffer on the stack and reaches hyper through
//! `ReadBufCursor::put_slice`, the one safe way in, because
//! `ReadBufCursor::advance` is `unsafe`. The buffer holds the read size of
//! Node's connection, 64 KiB, so a read of the socket hands hyper what one
//! read of Node's socket hands llhttp. The timer and its sleep are `Send` and
//! `Sync`, as hyper bounds them, over the heap of deadlines the clock of the
//! runtime shares (rule 2), and the executor spawns a stream of hyper's HTTP/2
//! server as a local task with no `Send` bound.

use core::{
    future::Future,
    pin::Pin,
    task::{
        Context,
        Poll, //
    },
};
use std::{
    io,
    sync::{
        Arc,
        Mutex, //
    },
    time::{
        Duration,
        Instant, //
    },
};

use hyper::rt::ReadBufCursor;

use super::{
    Handle,
    net::Socket,
    timers::{
        TimerHeap,
        WakerSlot,
        lock, //
    },
};

/// The bytes one read takes at most, the read size of Node's connection.
pub const READ_SIZE: usize = 65_536;

/// A stream of the runtime as hyper's IO.
pub struct HyperIo<S> {
    /// The non blocking stream.
    stream: S,
    /// The bytes a protocol probe read ahead, handed to hyper first.
    ahead: Vec<u8>,
}

impl<S> HyperIo<S> {
    /// Returns the IO of `stream`.
    pub fn new(stream: S) -> Self {
        Self::with_ahead(stream, Vec::new())
    }

    /// Returns the IO of `stream` whose first read answers `ahead`, the
    /// bytes a protocol probe read before it chose the protocol.
    pub fn with_ahead(stream: S, ahead: Vec<u8>) -> Self {
        Self { stream, ahead }
    }

    /// Returns the stream.
    pub fn get_ref(&self) -> &S {
        &self.stream
    }
}

// An `unsafe` `advance` would skip this copy.
impl<S: Socket + Unpin> hyper::rt::Read for HyperIo<S> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        mut buf: ReadBufCursor<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if !this.ahead.is_empty() {
            let n = this.ahead.len().min(buf.remaining());
            buf.put_slice(&this.ahead[..n]);
            this.ahead.drain(..n);
            return Poll::Ready(Ok(()));
        }
        // The buffer of a read lives on the stack of the one thread for the
        // length of the read, so an idle connection holds none of it, and it
        // takes a whole read of Node's size, which node1 parses before it
        // polls a service.
        #[expect(
            clippy::large_stack_arrays,
            reason = "one read of Node's size, held for the read alone"
        )]
        let mut stack = [0u8; READ_SIZE];
        let want = buf.remaining().min(READ_SIZE);
        match this.stream.poll_read(cx, &mut stack[..want]) {
            Poll::Ready(Ok(n)) => {
                buf.put_slice(&stack[..n]);
                Poll::Ready(Ok(()))
            }
            Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<S: Socket + Unpin> hyper::rt::Write for HyperIo<S> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.stream.poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(self.stream.shutdown_write())
    }

    fn is_write_vectored(&self) -> bool {
        false
    }
}

/// Hyper's timer over the heap of deadlines of the runtime.
#[derive(Clone)]
pub struct RuntimeTimer {
    /// The heap the clock of the runtime shares.
    heap: Arc<Mutex<TimerHeap>>,
}

impl RuntimeTimer {
    /// Returns the timer of the runtime whose `block_on` runs on this thread.
    ///
    /// # Errors
    ///
    /// Returns an error outside a runtime.
    pub fn current() -> io::Result<Self> {
        Ok(Self::of(&Handle::current()?))
    }

    /// Returns the timer of the runtime of `handle`.
    #[must_use]
    pub fn of(handle: &Handle) -> Self {
        Self {
            heap: handle.clock().heap(),
        }
    }

    /// Returns a sleep of `duration`, which a caller awaits without naming
    /// the trait of hyper.
    #[must_use]
    pub fn sleep_for(&self, duration: Duration) -> RuntimeSleep {
        RuntimeSleep {
            deadline: Instant::now() + duration,
            heap: Arc::clone(&self.heap),
            slot: None,
        }
    }
}

impl hyper::rt::Timer for RuntimeTimer {
    fn sleep(&self, duration: Duration) -> Pin<Box<dyn hyper::rt::Sleep>> {
        Box::pin(self.sleep_for(duration))
    }

    fn sleep_until(&self, deadline: Instant) -> Pin<Box<dyn hyper::rt::Sleep>> {
        Box::pin(RuntimeSleep {
            deadline,
            heap: Arc::clone(&self.heap),
            slot: None,
        })
    }

    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// A sleep of [`RuntimeTimer`], ready once its deadline has passed.
pub struct RuntimeSleep {
    /// When the sleep ends.
    deadline: Instant,
    /// The heap the deadline goes into.
    heap: Arc<Mutex<TimerHeap>>,
    /// The slot of the waker, shared with the heap once the sleep waits.
    slot: Option<WakerSlot>,
}

impl Future for RuntimeSleep {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let this = self.get_mut();
        if Instant::now() >= this.deadline {
            return Poll::Ready(());
        }
        if let Some(slot) = &this.slot {
            *lock(slot) = Some(cx.waker().clone());
        } else {
            let slot: WakerSlot = Arc::new(Mutex::new(Some(cx.waker().clone())));
            lock(&this.heap).push(this.deadline, Arc::clone(&slot));
            this.slot = Some(slot);
        }
        Poll::Pending
    }
}

impl hyper::rt::Sleep for RuntimeSleep {}

impl Drop for RuntimeSleep {
    fn drop(&mut self) {
        // A slot that still holds a waker has its entry in the heap, which
        // turns stale and leaves with the others, while a slot the heap has
        // already emptied has no entry left.
        if let Some(slot) = &self.slot
            && lock(slot).take().is_some()
        {
            lock(&self.heap).forget();
        }
    }
}

/// Hyper's executor over `spawn_local`, which spawns a stream of the HTTP/2
/// server as a local task.
#[cfg(feature = "http2")]
#[derive(Clone, Copy, Debug, Default)]
pub struct LocalExec;

#[cfg(feature = "http2")]
impl<F> hyper::rt::Executor<F> for LocalExec
where
    F: Future + 'static,
{
    fn execute(&self, future: F) {
        super::spawn_local(Box::pin(async move {
            future.await;
        }));
    }
}
