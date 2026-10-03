// SPDX-License-Identifier: MIT
/*
 * hyper: the channels of a request body on one thread, without the feature
 * sync.
 */
//! The channels of a request body on one thread.
//!
//! Without `sync` the body of a request reaches the service through the
//! channels of this module in place of those of `futures-channel`, whose
//! channels need atomics. They share their state through the `Rc` and
//! `RefCell` of [`shared`](super::shared), so they are not `Send`, and the
//! trailers travel through the channel of one value of
//! [`oneshot`](super::oneshot).

pub(crate) mod mpsc {
    //! A channel of chunks with one slot per sender.
    //!
    //! It keeps the order and the back pressure of `futures_channel::mpsc`
    //! with a buffer of zero: a sender that sent waits until the receiver
    //! took its value, and a clone of the sender has a slot of its own, so it
    //! can always send one value, an error after the last chunk for instance.

    use alloc::collections::VecDeque;
    use core::cell::{Cell, RefCell};
    use core::pin::Pin;
    use core::task::{Context, Poll, Waker};

    use crate::common::shared::Share;

    /// Returns the two ends of a new channel.
    pub(crate) fn channel<T>() -> (Sender<T>, Receiver<T>) {
        let inner = Share::new(RefCell::new(Inner {
            queue: VecDeque::new(),
            senders: 1,
            receiving: true,
            rx_waker: None,
            tx_waker: None,
        }));
        (
            Sender {
                inner: inner.clone(),
                parked: Share::new(Cell::new(false)),
            },
            Receiver {
                inner,
                terminated: false,
            },
        )
    }

    /// The end that sends, one value at a time.
    pub(crate) struct Sender<T> {
        inner: Share<RefCell<Inner<T>>>,
        parked: Share<Cell<bool>>,
    }

    /// The end that receives the values in the order they were sent.
    pub(crate) struct Receiver<T> {
        inner: Share<RefCell<Inner<T>>>,
        terminated: bool,
    }

    struct Inner<T> {
        queue: VecDeque<(T, Share<Cell<bool>>)>,
        senders: usize,
        receiving: bool,
        rx_waker: Option<Waker>,
        tx_waker: Option<Waker>,
    }

    /// The error of a sender whose receiver is gone.
    #[derive(Debug)]
    pub(crate) struct SendError;

    /// The error of a value that could not be sent, which it gives back.
    #[derive(Debug)]
    pub(crate) struct TrySendError<T>(T);

    impl<T> TrySendError<T> {
        /// Returns the value that was not sent.
        pub(crate) fn into_inner(self) -> T {
            self.0
        }
    }

    impl<T> Sender<T> {
        /// Answers ready once the slot of this sender is free.
        pub(crate) fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), SendError>> {
            let mut inner = self.inner.borrow_mut();
            if !inner.receiving {
                return Poll::Ready(Err(SendError));
            }
            if self.parked.get() {
                inner.tx_waker = Some(cx.waker().clone());
                return Poll::Pending;
            }
            Poll::Ready(Ok(()))
        }

        /// Sends `value` when the slot of this sender is free.
        pub(crate) fn try_send(&mut self, value: T) -> Result<(), TrySendError<T>> {
            let waker = {
                let mut inner = self.inner.borrow_mut();
                if !inner.receiving || self.parked.get() {
                    return Err(TrySendError(value));
                }
                self.parked.set(true);
                inner.queue.push_back((value, self.parked.clone()));
                inner.rx_waker.take()
            };
            if let Some(waker) = waker {
                waker.wake();
            }
            Ok(())
        }
    }

    impl<T> Clone for Sender<T> {
        fn clone(&self) -> Self {
            self.inner.borrow_mut().senders += 1;
            Sender {
                inner: self.inner.clone(),
                parked: Share::new(Cell::new(false)),
            }
        }
    }

    impl<T> Drop for Sender<T> {
        fn drop(&mut self) {
            let waker = {
                let mut inner = self.inner.borrow_mut();
                inner.senders -= 1;
                if inner.senders == 0 {
                    inner.rx_waker.take()
                } else {
                    None
                }
            };
            if let Some(waker) = waker {
                waker.wake();
            }
        }
    }

    // The receiver polls as the `Stream` and `FusedStream` of `futures-core`
    // do, through methods of its own.
    impl<T> Receiver<T> {
        /// Polls for the next value, and answers `None` once every sender is
        /// gone and the queue is empty.
        pub(crate) fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<T>> {
            let (value, waker) = {
                let mut inner = self.inner.borrow_mut();
                match inner.queue.pop_front() {
                    Some((value, parked)) => {
                        parked.set(false);
                        (value, inner.tx_waker.take())
                    }
                    None if inner.senders == 0 => {
                        drop(inner);
                        self.terminated = true;
                        return Poll::Ready(None);
                    }
                    None => {
                        inner.rx_waker = Some(cx.waker().clone());
                        return Poll::Pending;
                    }
                }
            };
            if let Some(waker) = waker {
                waker.wake();
            }
            Poll::Ready(Some(value))
        }
    }

    impl<T> Receiver<T> {
        /// Tells whether `poll_next` answered `None` already.
        pub(crate) fn is_terminated(&self) -> bool {
            self.terminated
        }
    }

    impl<T> Drop for Receiver<T> {
        fn drop(&mut self) {
            let waker = {
                let mut inner = self.inner.borrow_mut();
                inner.receiving = false;
                inner.tx_waker.take()
            };
            if let Some(waker) = waker {
                waker.wake();
            }
        }
    }
}

pub(crate) use super::oneshot;
