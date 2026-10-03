// SPDX-License-Identifier: MIT
/*
 * aosabi-carrier-usb linux: the driver of USB in Linux user space, the
 * gadget side of a Linux card, USB bulk through FunctionFS and USB serial on
 * the tty of the ACM function, each served by the GraphTP listener as a local
 * task of the runtime of aosabi-serve.
 *
 * No analogue on the reference side.
 */

//! The Linux driver of USB.
//!
//! [`serve_bulk`] runs one endpoint of the server role over a [`Bulk`]
//! function with the listener in front of it: the host enabling the function
//! is the link going up, and a disable, an unbind or a relay whose endpoint
//! file failed is the link going down, so the listener ends the peer's
//! streams, while a halted endpoint is cleared by the host and repaired by
//! RESEND and a suspended bus is silence. [`serve_serial`] runs one endpoint
//! over a [`GadgetTty`] in delimited `AoFrame`, whose link is up from the
//! start and ends with the tty.

#[cfg(feature = "serial")]
mod acm;
mod app;
#[cfg(feature = "bulk")]
mod ffs;
#[cfg(feature = "bulk")]
mod relay;
mod task;

use alloc::{
    boxed::Box,
    rc::Rc, //
};
use core::cell::Cell;
#[cfg(any(feature = "bulk", feature = "serial"))]
use core::{
    future::poll_fn,
    task::{
        Context,
        Poll, //
    }, //
};
use std::{
    env,
    eprintln,
    io,
    path::PathBuf, //
};

#[cfg(feature = "serial")]
use alloc::vec;
use aosabi_core::{
    BootedApp,
    Listen, //
};
use aosabi_serve::{
    BuildOutput,
    runtime::{
        LocalBoxFuture,
        spawn_local, //
    }, //
};
use graphql_over_graphtp::{
    Fetch,
    limits::CardLimits, //
};

#[cfg(feature = "serial")]
use aosabi_serve::runtime::Interest;
#[cfg(any(feature = "bulk", feature = "serial"))]
use graphql_over_graphtp::{
    Listener,
    limits, //
};
#[cfg(feature = "bulk")]
use homa::Limits;
#[cfg(feature = "serial")]
use homa::stream::{
    ByteStream,
    Decoder,
    Framed,
    Mode,
    frame_len, //
};
use homa::{
    Config,
    Stats, //
};

#[cfg(feature = "serial")]
pub use self::acm::GadgetTty;
pub use self::{
    app::{
        Seed,
        app_fetch,
        seed, //
    },
    task::{
        Registered,
        Task,
        TaskError, //
    }, //
};
#[cfg(feature = "bulk")]
pub use self::{
    ffs::{
        Bulk,
        BulkCounts,
        FfsEvent,
        descriptors,
        negotiated_packet_size,
        strings, //
    },
    relay::{
        RELAY_ENV,
        relay_from_env, //
    }, //
};
use crate::profile::MAX_RPCS;

/// The environment variable that starts the bulk driver, the mount of
/// `FunctionFS`.
pub const FFS_ENV: &str = "AOSABI_USB_FFS";

/// The environment variable that starts the serial driver, the tty of the
/// ACM function.
pub const ACM_ENV: &str = "AOSABI_USB_ACM";

/// The bytes of one page of the endpoint, so a slot of 16 pages holds the
/// 131,072 bytes of the player's answer buffer.
const PAGE_LEN: u32 = 8_192;

/// The pages one slot of the endpoint takes at most, the bound of the
/// engine.
const PAGES_PER_SLOT: u32 = 16;

// The flags of the modules are those of Linux, so the feature refuses to
// build anywhere else rather than pass a flag it does not mean.
#[cfg(not(target_os = "linux"))]
compile_error!("the feature linux of aosabi-carrier-usb builds on Linux alone");

/// Returns the configuration of an endpoint that serves one function of a
/// card with the bounds `card`, and the longest message it accepts.
///
/// The slots are the RPCs a peer may hold open, and a slot takes at most 16
/// pages of 8,192 bytes, so a message holds the longest request of the card
/// and at most 131,072 bytes.
#[must_use]
pub fn link_config(card: &CardLimits, seed: Seed) -> (Config, u32) {
    let slots = u32::from(MAX_RPCS);
    let request = card
        .request_buffer
        .saturating_add(usize::from(card.max_credential))
        .saturating_add(3);
    let max_message = u32::try_from(request)
        .unwrap_or(u32::MAX)
        .min(PAGE_LEN * PAGES_PER_SLOT);
    let config = Config {
        peers: 1,
        client_slots: 0,
        server_slots: slots,
        page_len: PAGE_LEN,
        pages: slots * PAGES_PER_SLOT,
        max_gaps: 2,
        size_buckets: 0,
        first_id: seed.first_id,
        hash_key: seed.hash_key,
        port: 0,
        extra_ports: 0,
    };
    (config, max_message)
}

/// What a served function counted, which a harness reads while and after
/// the task runs.
#[derive(Debug, Default)]
pub struct Counts {
    /// The counters of the endpoint after the last turn.
    pub stats: Cell<Stats>,
    /// The answers the listener handed the endpoint.
    pub answered: Cell<u64>,
    /// The links that went up.
    pub ups: Cell<u64>,
    /// The links that went down.
    pub downs: Cell<u64>,
    /// What the bulk function counted.
    #[cfg(feature = "bulk")]
    pub bulk: Cell<BulkCounts>,
    /// The frames the framing of the serial function dropped in either
    /// direction.
    pub dropped: Cell<u64>,
}

/// Serves `GraphTP`(Homa) on the bulk function `bulk` until `ep0` or a relay
/// fails, as a local task of the runtime over `Endpoint::pump` and
/// `listener`.
#[cfg(feature = "bulk")]
#[must_use]
pub fn serve_bulk(bulk: Bulk, listener: Listener<()>) -> LocalBoxFuture<'static, io::Result<()>> {
    serve_bulk_counted(bulk, listener, Rc::new(Counts::default()))
}

/// Serves as [`serve_bulk`] does and keeps what the function counted in
/// `counts`.
#[cfg(feature = "bulk")]
#[must_use]
pub fn serve_bulk_counted(
    mut bulk: Bulk,
    listener: Listener<()>,
    counts: Rc<Counts>,
) -> LocalBoxFuture<'static, io::Result<()>> {
    Box::pin(async move {
        let card = listener.limits();
        let (config, max_message) = link_config(card, seed()?);
        let profile = crate::profile::bulk_profile(bulk.speed());
        let local = crate::profile::limits(&profile, max_message);
        let mut task = Task::new(config, profile, listener)?;
        bulk.register(task.handle())?;
        let mut up = false;
        poll_fn(|cx| step_bulk(&mut task, &mut bulk, &mut up, local, &counts, cx)).await
    })
}

/// Runs the events and the turns of one wake of a bulk task.
#[cfg(feature = "bulk")]
fn step_bulk(
    task: &mut Task<()>,
    bulk: &mut Bulk,
    up: &mut bool,
    local: Limits,
    counts: &Counts,
    cx: &mut Context<'_>,
) -> Poll<io::Result<()>> {
    loop {
        match bulk.poll_event() {
            Ok(Some(FfsEvent::Enable)) if !*up => {
                if let Err(error) = task.link_up((), local, local) {
                    return Poll::Ready(Err(TaskError::Link(error).into()));
                }
                *up = true;
                counts.ups.set(counts.ups.get() + 1);
            }
            Ok(Some(_)) => {}
            Ok(None) => break,
            Err(error) => return Poll::Ready(Err(error)),
        }
    }
    let ran = task.run(bulk, cx);
    if *up && !bulk.enabled() {
        task.link_down((), cx);
        *up = false;
        counts.downs.set(counts.downs.get() + 1);
    }
    counts.stats.set(task.stats());
    counts.answered.set(task.answered());
    counts.bulk.set(bulk.counts());
    if let Err(error) = ran {
        return Poll::Ready(Err(error));
    }
    bulk.register_interest(cx.waker());
    Poll::Pending
}

/// Serves `GraphTP`(Homa) on the serial function `tty` until the tty fails
/// or ends, as a local task of the runtime over `Endpoint::pump` and
/// `listener`, the tty in the reactor and delimited `AoFrame` in front.
#[cfg(feature = "serial")]
#[must_use]
pub fn serve_serial(
    tty: GadgetTty,
    listener: Listener<()>,
) -> LocalBoxFuture<'static, io::Result<()>> {
    serve_serial_counted(tty, listener, Rc::new(Counts::default()))
}

/// Serves as [`serve_serial`] does and keeps what the function counted in
/// `counts`.
#[cfg(feature = "serial")]
#[must_use]
pub fn serve_serial_counted(
    tty: GadgetTty,
    listener: Listener<()>,
    counts: Rc<Counts>,
) -> LocalBoxFuture<'static, io::Result<()>> {
    Box::pin(async move {
        let card = listener.limits();
        let (config, max_message) = link_config(card, seed()?);
        let profile = crate::profile::serial_profile();
        let local = crate::profile::limits(&profile, max_message);
        let mut task = Task::new(config, profile, listener)?;
        task.link_up((), local, local).map_err(TaskError::Link)?;
        counts.ups.set(1);
        let largest = usize::from(profile.max_packet);
        let mut rx = vec![0u8; Decoder::buffer_len(Mode::Delimited, largest)];
        let mut tx = vec![0u8; 4 * frame_len(Mode::Delimited, largest)];
        let tty = Registered::new(tty, task.handle())?;
        let mut framed = Framed::new(tty, Mode::Delimited, &mut rx, &mut tx);
        poll_fn(|cx| step_serial(&mut task, &mut framed, &counts, cx)).await
    })
}

/// Runs the turns of one wake of a serial task and registers its interests.
#[cfg(feature = "serial")]
fn step_serial(
    task: &mut Task<()>,
    framed: &mut Framed<'_, Registered<GadgetTty>>,
    counts: &Counts,
    cx: &mut Context<'_>,
) -> Poll<io::Result<()>> {
    let ran = task.run(framed, cx);
    counts.stats.set(task.stats());
    counts.dropped.set(framed.dropped());
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
    let tty = framed.stream();
    tty.register(Interest::Read, cx);
    if !drained {
        tty.register(Interest::Write, cx);
    }
    Poll::Pending
}

#[cfg(feature = "serial")]
impl ByteStream for Registered<GadgetTty> {
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

/// The listener of USB the boot runs over the booted app.
#[derive(Debug)]
pub struct UsbListen {
    /// The mount of `FunctionFS` of the bulk function, if one is named.
    ffs: Option<PathBuf>,
    /// The tty of the serial function, if one is named.
    acm: Option<PathBuf>,
}

/// It captures nothing and reads its variables when the boot calls it, and
/// the listener it answers builds a `GraphTP` listener for each function over
/// the fetch of the booted app it is handed.
///
/// # Errors
///
/// Returns no error, and keeps the signature of every driver of the boot.
pub fn driver(_build: &BuildOutput) -> io::Result<Option<Box<dyn Listen>>> {
    let ffs = env::var_os(FFS_ENV).map(PathBuf::from);
    let acm = env::var_os(ACM_ENV).map(PathBuf::from);
    if ffs.is_none() && acm.is_none() {
        return Ok(None);
    }
    Ok(Some(Box::new(UsbListen { ffs, acm })))
}

impl Listen for UsbListen {
    fn run(self: Box<Self>, app: Rc<BootedApp>) -> LocalBoxFuture<'static, ()> {
        let fetch = app_fetch(app);
        if let Some(mount) = self.ffs {
            spawn_local(serve_bulk_at(mount, Rc::clone(&fetch)));
        }
        if let Some(path) = self.acm {
            spawn_local(serve_serial_at(path, fetch));
        }
        Box::pin(async {})
    }
}

/// Returns the task that says a bulk function was named in a build without
/// the feature `bulk`.
#[cfg(not(feature = "bulk"))]
fn serve_bulk_at(mount: PathBuf, _fetch: Fetch) -> LocalBoxFuture<'static, ()> {
    Box::pin(async move {
        eprintln!(
            "aosabi-carrier-usb: {}: the build has no feature bulk",
            mount.display()
        );
    })
}

/// Returns the task that says a serial function was named in a build
/// without the feature `serial`.
#[cfg(not(feature = "serial"))]
fn serve_serial_at(path: PathBuf, _fetch: Fetch) -> LocalBoxFuture<'static, ()> {
    Box::pin(async move {
        eprintln!(
            "aosabi-carrier-usb: {}: the build has no feature serial",
            path.display()
        );
    })
}

/// Returns the task that serves the bulk function of the mount `mount` over
/// `fetch`, and prints why it ended.
#[cfg(feature = "bulk")]
fn serve_bulk_at(mount: PathBuf, fetch: Fetch) -> LocalBoxFuture<'static, ()> {
    Box::pin(async move {
        let served = match Listener::new(fetch, &limits::PLAYER) {
            Ok(listener) => match Bulk::open(&mount, crate::profile::Speed::High) {
                Ok(bulk) => serve_bulk(bulk, listener).await,
                Err(error) => Err(error),
            },
            Err(error) => Err(io::Error::other(error)),
        };
        if let Err(error) = served {
            eprintln!("aosabi-carrier-usb: {}: {error}", mount.display());
        }
    })
}

/// Returns the task that serves the serial function of the tty `path` over
/// `fetch`, and prints why it ended.
#[cfg(feature = "serial")]
fn serve_serial_at(path: PathBuf, fetch: Fetch) -> LocalBoxFuture<'static, ()> {
    Box::pin(async move {
        let served = match Listener::new(fetch, &limits::PLAYER) {
            Ok(listener) => match GadgetTty::open(&path) {
                Ok(tty) => serve_serial(tty, listener).await,
                Err(error) => Err(error),
            },
            Err(error) => Err(io::Error::other(error)),
        };
        if let Err(error) = served {
            eprintln!("aosabi-carrier-usb: {}: {error}", path.display());
        }
    })
}
