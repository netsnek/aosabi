// SPDX-License-Identifier: MIT
/*
 * aosabi-carrier-bluetooth linux channel: the L2CAP LE credit based channel
 * on Linux, one accepted socket in prefixed AoFrame served by an endpoint
 * and a listener of its own.
 *
 * No analogue on the reference side.
 */

//! The channel on Linux.
//!
//! [`serve_accepted`] serves one accepted socket of the channel, a byte
//! stream of prefixed `AoFrame`, with an endpoint and a listener of its own,
//! as the Unix Socket driver serves each accepted socket, so the channel's
//! listener needs no index of a peer. The packet of the link follows the MPS
//! of the peer by the k rule of the design note.
//!
//! Until then [`serve_channel`] answers `Unsupported` with that reason, and
//! the Linux devices serve the GATT data path alone.

use alloc::{
    boxed::Box,
    rc::Rc,
    vec, //
};
use core::{
    future::poll_fn,
    task::{
        Context,
        Poll, //
    }, //
};
use std::{
    io,
    os::fd::AsFd, //
};

use aosabi_serve::runtime::{
    Interest,
    LocalBoxFuture, //
};
use graphql_over_graphtp::Listener;
use homa::stream::{
    ByteStream,
    Decoder,
    Framed,
    Mode,
    frame_len, //
};

use super::{
    Counts,
    Registered,
    Task,
    TaskError,
    gatt::Gatt,
    link_config,
    seed, //
};
use crate::profile::{
    channel_profile,
    limits, //
};

/// Why the channel does not serve on Linux yet.
pub const CHANNEL_WAITS: &str = "the L2CAP address of the channel waits on the owner's answer to \
                                 decision P2 of the plan of the Homa track, items H3-EXC, H3-L2S \
                                 and H3-L2CAP";

/// Serves the channel on the adapter `adapter` with the PSM published in the
/// characteristic of `gatt`, one accepted socket at a time with a listener
/// that `listener` builds.
#[must_use]
pub fn serve_channel(
    adapter: &str,
    gatt: &Gatt,
    listener: impl FnMut() -> Listener<()> + 'static,
) -> LocalBoxFuture<'static, io::Result<()>> {
    let _ = (adapter, gatt.psm());
    drop(listener);
    Box::pin(async { Err(io::Error::new(io::ErrorKind::Unsupported, CHANNEL_WAITS)) })
}

/// Serves `GraphTP`(Homa) on one accepted socket of the channel to a peer
/// whose MPS is `peer_mps`, until the socket fails or ends, as a local task
/// of the runtime with prefixed `AoFrame` in front and `listener` behind.
#[must_use]
pub fn serve_accepted<S>(
    socket: S,
    peer_mps: u16,
    listener: Listener<()>,
    counts: Rc<Counts>,
) -> LocalBoxFuture<'static, io::Result<()>>
where
    S: ByteStream<Error = io::Error> + AsFd + 'static,
{
    Box::pin(async move {
        let card = listener.limits();
        let (config, max_message) = link_config(card, seed()?);
        let profile = channel_profile(peer_mps, 0);
        let local = limits(&profile, max_message);
        let mut task = Task::new(config, profile, listener)?;
        task.link_up((), local, local).map_err(TaskError::Link)?;
        counts.ups.set(counts.ups.get() + 1);
        let largest = usize::from(profile.max_packet);
        let mut rx = vec![0u8; Decoder::buffer_len(Mode::Prefixed, largest)];
        let mut tx = vec![0u8; 4 * frame_len(Mode::Prefixed, largest)];
        let socket = Registered::new(socket, task.handle())?;
        let mut framed = Framed::new(socket, Mode::Prefixed, &mut rx, &mut tx);
        poll_fn(|cx| step(&mut task, &mut framed, &counts, cx)).await
    })
}

/// Runs the turns of one wake of a channel task and registers its interests.
fn step<S>(
    task: &mut Task<()>,
    framed: &mut Framed<'_, Registered<S>>,
    counts: &Counts,
    cx: &mut Context<'_>,
) -> Poll<io::Result<()>>
where
    S: ByteStream<Error = io::Error> + AsFd,
{
    let ran = task.run(framed, cx);
    counts.stats.set(task.stats());
    counts.answered.set(task.answered());
    if let Err(error) = ran {
        task.link_down((), cx);
        counts.downs.set(counts.downs.get() + 1);
        return Poll::Ready(Err(error));
    }
    if framed.closed() {
        task.link_down((), cx);
        counts.downs.set(counts.downs.get() + 1);
        return Poll::Ready(Ok(()));
    }
    let drained = framed.flush().unwrap_or(true);
    let socket = framed.stream();
    socket.register(Interest::Read, cx);
    if !drained {
        socket.register(Interest::Write, cx);
    }
    Poll::Pending
}

impl<S: ByteStream<Error = io::Error> + AsFd> ByteStream for Registered<S> {
    type Error = io::Error;

    fn read(&mut self, buf: &mut [u8]) -> io::Result<Option<usize>> {
        let read = self.get_mut().read(buf)?;
        if read.is_some_and(|read| read > 0) {
            self.progressed();
        }
        Ok(read)
    }

    fn write(&mut self, bytes: &[u8]) -> io::Result<Option<usize>> {
        let written = self.get_mut().write(bytes)?;
        if written.is_some_and(|written| written > 0) {
            self.progressed();
        }
        Ok(written)
    }
}
