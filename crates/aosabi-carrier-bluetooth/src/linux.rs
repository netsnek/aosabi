// SPDX-License-Identifier: MIT
/*
 * aosabi-carrier-bluetooth linux: the driver of Bluetooth in Linux user
 * space, the GATT data path through BlueZ over a D-Bus client of the project
 * and the channel, each served by the GraphTP listener as a local task of the
 * runtime of aosabi-serve.
 *
 * No analogue on the reference side.
 */

//! The Linux driver of Bluetooth.
//!
//! [`serve_gatt`] runs one endpoint of the server role over the [`Gatt`] data
//! path with the listener in front of it, the one central the data path
//! serves its one peer: the first write of a central after it subscribed to
//! TX brings the link up with the `ATT_MTU` the write names, the writes of
//! another central are refused while it is up, and the device leaving the bus
//! or unsubscribing takes it down, so the listener ends the peer's streams.
//! The data path holds the packets below the engine to the depth of its
//! pace, and the task arms a timer of the runtime at the instant the pace
//! frees room, so the engine hands over the rest of a long answer one slot
//! at a time and a short answer that comes meanwhile goes first.
//!
//! The feature `trace` gives the data path a [`GattTrace`] that a harness
//! turns on before it hands the data path to [`serve_gatt_counted`] and reads
//! while and after the task runs. No card names the feature, so the driver of
//! a card keeps no trace.

mod app;
#[cfg(feature = "channel")]
mod channel;
mod dbus;
#[cfg(feature = "gatt")]
mod gatt;
mod task;

use alloc::{
    boxed::Box,
    rc::Rc, //
};
use core::cell::Cell;
#[cfg(feature = "gatt")]
use core::{
    future::{
        Future as _,
        poll_fn, //
    },
    pin::Pin,
    task::{
        Context,
        Poll, //
    }, //
};
#[cfg(feature = "gatt")]
use std::time::Instant;
use std::{
    env,
    eprintln,
    io,
    string::String, //
};

use aosabi_core::{
    BootedApp,
    Listen, //
};
#[cfg(feature = "gatt")]
use aosabi_serve::runtime::{
    Handle,
    hyper_rt::{
        RuntimeSleep,
        RuntimeTimer, //
    }, //
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
#[cfg(feature = "gatt")]
use graphql_over_graphtp::{
    Listener,
    limits, //
};
use homa::{
    Config,
    Stats, //
};

#[cfg(feature = "channel")]
pub use self::channel::{
    CHANNEL_WAITS,
    serve_accepted,
    serve_channel, //
};
#[cfg(feature = "gatt")]
pub use self::gatt::{
    APP_PATH,
    Gatt,
    GattCounts,
    GattEvent,
    PEER,
    notified_len,
    sent_packet, //
};
#[cfg(feature = "trace")]
pub use self::gatt::{
    GattTrace,
    TracedPacket, //
};
pub use self::{
    app::{
        Seed,
        app_fetch,
        seed, //
    },
    dbus::{
        Bus,
        Kind,
        Message,
        Value, //
    },
    task::{
        Registered,
        Task,
        TaskError, //
    }, //
};
use crate::profile::MAX_RPCS;

/// The environment variable that starts the driver, the adapter, `hci0` for
/// example.
pub const BLUETOOTH_ENV: &str = "AOSABI_BLUETOOTH";

/// The largest `ATT_MTU` of LE, whose values hold 514 bytes, the bound of the
/// packets of the endpoint of the data path.
pub const MAX_ATT_MTU: u16 = 517;

/// The centrals the data path serves at once, one, since a notification of
/// TX reaches every subscribed central.
pub const CENTRALS: u32 = 1;

/// The passes of reading the bus and turning the endpoint one wake of the
/// data path runs at most before it yields to the other tasks.
#[cfg(feature = "gatt")]
const GATT_PASSES: usize = 8;

/// The bytes of one page of the endpoint, so a slot of 16 pages holds the
/// 131,072 bytes of the player's answer buffer.
const PAGE_LEN: u32 = 8_192;

/// The pages one slot of the endpoint takes at most, the bound of the
/// engine.
const PAGES_PER_SLOT: u32 = 16;

// The flags and the paths of the modules are those of Linux, so the feature
// refuses to build anywhere else.
#[cfg(not(target_os = "linux"))]
compile_error!("the feature linux of aosabi-carrier-bluetooth builds on Linux alone");

/// Returns the configuration of an endpoint that serves `peers` peers of a
/// card with the bounds `card`, and the longest message it accepts.
///
/// Each peer may hold its RPCs open, and a slot takes at most 16 pages of
/// 8,192 bytes, so a message holds the longest request of the card and at
/// most 131,072 bytes.
#[must_use]
pub fn link_config(card: &CardLimits, seed: Seed) -> (Config, u32) {
    peers_config(card, seed, 1)
}

/// Returns the configuration of [`link_config`] for `peers` peers.
#[must_use]
pub fn peers_config(card: &CardLimits, seed: Seed, peers: u32) -> (Config, u32) {
    let slots = u32::from(MAX_RPCS) * peers;
    let request = card
        .request_buffer
        .saturating_add(usize::from(card.max_credential))
        .saturating_add(3);
    let max_message = u32::try_from(request)
        .unwrap_or(u32::MAX)
        .min(PAGE_LEN * PAGES_PER_SLOT);
    let config = Config {
        peers,
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

/// What a served link counted, which a harness reads while and after the
/// task runs.
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
    /// What the GATT data path counted.
    #[cfg(feature = "gatt")]
    pub gatt: Cell<GattCounts>,
}

/// Serves `GraphTP`(Homa) on the GATT data path `gatt` until the bus fails,
/// as a local task of the runtime over `Endpoint::pump` and `listener`.
#[cfg(feature = "gatt")]
#[must_use]
pub fn serve_gatt(gatt: Gatt, listener: Listener<u32>) -> LocalBoxFuture<'static, io::Result<()>> {
    serve_gatt_counted(gatt, listener, Rc::new(Counts::default()))
}

/// Serves as [`serve_gatt`] does and keeps what the data path counted in
/// `counts`.
#[cfg(feature = "gatt")]
#[must_use]
pub fn serve_gatt_counted(
    mut gatt: Gatt,
    listener: Listener<u32>,
    counts: Rc<Counts>,
) -> LocalBoxFuture<'static, io::Result<()>> {
    Box::pin(async move {
        let card = listener.limits();
        let (config, max_message) = peers_config(card, seed()?, CENTRALS);
        let profile = crate::profile::gatt_profile(MAX_ATT_MTU, 0);
        let mut task = Task::new(config, profile, listener)?;
        gatt.register_reactor(task.handle())?;
        let mut slot = SlotWake::new(task.handle());
        poll_fn(|cx| step_gatt(&mut task, &mut gatt, &mut slot, max_message, &counts, cx)).await
    })
}

/// The timer that wakes the task of a paced data path at the instant its pace
/// frees room for the next packet.
#[cfg(feature = "gatt")]
struct SlotWake {
    /// The timer of the runtime.
    timer: RuntimeTimer,
    /// The instant the timer is armed at and its sleep.
    sleep: Option<(Instant, RuntimeSleep)>,
}

#[cfg(feature = "gatt")]
impl SlotWake {
    /// Returns the timer of the runtime of `handle`, not yet armed.
    fn new(handle: &Handle) -> Self {
        Self {
            timer: RuntimeTimer::of(handle),
            sleep: None,
        }
    }

    /// Arms the timer at `at` with the context `cx` and tells whether that
    /// instant has passed already, or disarms it where `at` is `None`.
    fn arm(&mut self, at: Option<Instant>, cx: &mut Context<'_>) -> bool {
        let Some(at) = at else {
            self.sleep = None;
            return false;
        };
        let now = Instant::now();
        if at <= now {
            return true;
        }
        if self.sleep.as_ref().is_none_or(|(armed, _)| *armed != at) {
            self.sleep = Some((at, self.timer.sleep_for(at - now)));
        }
        match self.sleep.as_mut() {
            Some((_, sleep)) => Pin::new(sleep).poll(cx).is_ready(),
            None => false,
        }
    }
}

/// Returns what this end accepts from a central at `att_mtu`, a packet of a
/// write of `ATT_MTU` less 3 bytes, and what it takes of the central until the
/// central's LIMITS arrives, a packet no longer than [`sent_packet`].
#[cfg(feature = "gatt")]
#[must_use]
pub fn gatt_limits(att_mtu: u16, max_message: u32) -> (homa::Limits, homa::Limits) {
    let local = crate::profile::limits(&crate::profile::gatt_profile(att_mtu, 0), max_message);
    let assumed = homa::Limits {
        max_packet: sent_packet(att_mtu),
        ..local
    };
    (local, assumed)
}

/// Runs the events and the turns of one wake of the data path.
///
/// A wake that leaves the data path held at the depth of its pace arms
/// `slot` at the instant the pace frees room, beside the interest in the bus,
/// so the task sleeps until a write arrives or the pace lets the engine hand
/// over the next packet.
#[cfg(feature = "gatt")]
fn step_gatt(
    task: &mut Task<u32>,
    gatt: &mut Gatt,
    slot: &mut SlotWake,
    max_message: u32,
    counts: &Counts,
    cx: &mut Context<'_>,
) -> Poll<io::Result<()>> {
    for _ in 0..GATT_PASSES {
        if let Err(error) = gatt.poll() {
            return Poll::Ready(Err(error));
        }
        while let Some(event) = gatt.poll_event() {
            match event {
                GattEvent::Up { peer, att_mtu } => {
                    let (local, assumed) = gatt_limits(att_mtu, max_message);
                    if task.link_up(peer, local, assumed).is_ok() {
                        counts.ups.set(counts.ups.get() + 1);
                    }
                }
                GattEvent::Down { peer } => {
                    task.link_down(peer, cx);
                    counts.downs.set(counts.downs.get() + 1);
                }
            }
        }
        let ran = task.run(gatt, cx);
        counts.stats.set(task.stats());
        counts.answered.set(task.answered());
        counts.gatt.set(gatt.counts());
        if let Err(error) = ran {
            return Poll::Ready(Err(error));
        }
        if !gatt.has_received() {
            gatt.register_interest(cx.waker());
            if !slot.arm(gatt.paced_until(), cx) {
                return Poll::Pending;
            }
        }
    }
    cx.waker().wake_by_ref();
    Poll::Pending
}

/// The listener of Bluetooth the boot runs over the booted app.
#[derive(Debug)]
pub struct BluetoothListen {
    /// The adapter, `hci0` for example.
    adapter: String,
}

/// It captures nothing and reads its variable when the boot calls it, and
/// the listener it answers serves the GATT data path over the fetch of the
/// booted app it is handed, and the channel once its address module lands.
///
/// # Errors
///
/// Returns no error, and keeps the signature of every driver of the boot.
pub fn driver(_build: &BuildOutput) -> io::Result<Option<Box<dyn Listen>>> {
    let Some(adapter) = env::var_os(BLUETOOTH_ENV) else {
        return Ok(None);
    };
    Ok(Some(Box::new(BluetoothListen {
        adapter: adapter.to_string_lossy().into_owned(),
    })))
}

impl Listen for BluetoothListen {
    fn run(self: Box<Self>, app: Rc<BootedApp>) -> LocalBoxFuture<'static, ()> {
        let fetch = app_fetch(app);
        spawn_local(serve_gatt_on(self.adapter, fetch));
        Box::pin(async {})
    }
}

/// Returns the task that serves the GATT data path of the adapter `adapter`
/// over `fetch`, and prints why it ended.
#[cfg(feature = "gatt")]
fn serve_gatt_on(adapter: String, fetch: Fetch) -> LocalBoxFuture<'static, ()> {
    Box::pin(async move {
        let served = match Listener::new(fetch, &limits::PLAYER) {
            Ok(listener) => match Bus::system()
                .and_then(|bus| Gatt::register(bus, &adapter, crate::profile::SPSM))
            {
                Ok(gatt) => serve_gatt(gatt, listener).await,
                Err(error) => Err(error),
            },
            Err(error) => Err(io::Error::other(error)),
        };
        if let Err(error) = served {
            eprintln!("aosabi-carrier-bluetooth: {adapter}: {error}");
        }
    })
}

/// Returns the task that says the GATT data path is switched off on the
/// adapter `adapter`, where the channel alone would serve.
#[cfg(not(feature = "gatt"))]
fn serve_gatt_on(adapter: String, _fetch: Fetch) -> LocalBoxFuture<'static, ()> {
    Box::pin(async move {
        eprintln!("aosabi-carrier-bluetooth: {adapter}: the card switched the GATT data path off");
    })
}
