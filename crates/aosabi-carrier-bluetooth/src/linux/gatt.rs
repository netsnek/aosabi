// SPDX-License-Identifier: MIT
/*
 * aosabi-carrier-bluetooth linux gatt: the GATT data path through BlueZ, the
 * service of Aosabi registered over D-Bus and one Homa packet an ATT value.
 *
 * No analogue on the reference side.
 */

//! The GATT data path.
//!
//! [`Gatt::register`] exports the service of Aosabi on the system bus and
//! registers it with the adapter's `GattManager1`, with an advertisement of
//! the service's UUID through `LEAdvertisingManager1`. The PSM
//! characteristic answers the PSM of the channel to a read with encryption,
//! RX takes one Homa packet a write without response with encryption, and TX
//! carries one Homa packet a notification, which the service sends through
//! `PropertiesChanged` of its `Value`, `BlueZ` turning each into a notification
//! to every central that subscribed.
//!
//! The data path serves one central at a time as its one peer, [`PEER`]. The
//! first central that writes once TX is subscribed brings that link up, since
//! `StartNotify` names no device, with the `ATT_MTU` the write names, so the
//! LIMITS of this end is the first notification the central receives. The
//! link goes down where the central's device reports `Connected` false, where
//! the device leaves the bus's objects, or where TX loses its subscription,
//! and the values the central wrote that the endpoint has not taken go with
//! it. While the link is up the data path refuses the write of any other
//! central with `org.bluez.Error.NotPermitted` and takes none of its values,
//! so the next central's link goes up only after the first one's went down.
//!
//! A notification of TX still reaches every central that subscribed, since
//! `bluetoothd` notifies each value of `PropertiesChanged` to every central
//! whose Client Characteristic Configuration is set. A second central that
//! pairs and subscribes therefore receives the packets the data path sends to
//! the central it serves. The notification of one central alone goes through a
//! file descriptor of `AcquireNotify`, which the bus hands over in an ancillary
//! message that no safe call of the stable standard library receives.
//!
//! `bluetoothd` notifies a central that set the bit of Multiple Handle Value
//! Notification in its Client Supported Features through that PDU, which cuts
//! each value to `ATT_MTU` less 5 bytes where a single notification holds
//! `ATT_MTU` less 3, and `BlueZ` as a central sets the bit on every server that
//! has the characteristic. The data path therefore sends no packet longer than
//! [`sent_packet`] to any central, and lowers the `max_packet` of a LIMITS a
//! central writes to it before the endpoint reads it, since the endpoint sizes
//! its packets to a peer by the peer's LIMITS.
//!
//! No signal of the bus tells the data path that a notification left the
//! controller, so the data path paces itself at the connection interval of a
//! [`Pace`]: it counts every notification it hands the bus until a slot of
//! the pace retires it and answers that it is not ready while the depth of
//! the pace is held, [`Pace::PLANNING`] from the moment it registers, until
//! [`Gatt::set_pace`] names another pace or none. The engine then keeps the
//! rest of a long answer, where SRPT puts a short answer first, and
//! [`Gatt::paced_until`] names the instant the task of the data path wakes
//! at to hand over more.
//!
//! Under the feature `trace` a harness turns on a [`GattTrace`] with
//! [`Gatt::trace_on`], which keeps the time and the compact header of every
//! packet `send` hands to the bus and of every value `recv` takes from RX, up
//! to the number of packets the harness names. The time is that of the
//! realtime clock, which a central in the same kernel reads as well, so the
//! stamps of both ends of one packet compare.

#[cfg(feature = "trace")]
use alloc::rc::Rc;
use alloc::{
    collections::VecDeque,
    format,
    string::{
        String,
        ToString, //
    },
    vec,
    vec::Vec, //
};
#[cfg(feature = "trace")]
use core::cell::{
    Cell,
    RefCell, //
};
use core::{
    fmt::Write as _,
    task::Waker,
    time::Duration, //
};
#[cfg(feature = "trace")]
use std::time::{
    SystemTime,
    UNIX_EPOCH, //
};
use std::{
    io,
    os::fd::AsFd,
    time::Instant, //
};

use aosabi_serve::runtime::{
    Handle,
    Interest,
    SourceId, //
};
#[cfg(feature = "trace")]
use homa::wire::compact::MAX_HEADER_LEN;
use homa::{
    Carrier,
    Nanos,
    Priority,
    wire::{
        Compact,
        Form,
        Packet,
        compact::MIN_PACKET, //
    }, //
};

use super::dbus::{
    Bus,
    Kind,
    Message,
    Value, //
};
use crate::profile::{
    DEFAULT_ATT_MTU,
    PSM_CHARACTERISTIC,
    Pace,
    RX,
    SERVICE,
    TX, //
};

/// The bus name of `BlueZ`.
const BLUEZ: &str = "org.bluez";

/// The object path of the application, the root of the objects the service
/// exports.
pub const APP_PATH: &str = "/org/aosabi/gatt";

/// The object path of the service.
const SERVICE_PATH: &str = "/org/aosabi/gatt/service0";

/// The object path of the PSM characteristic.
const PSM_PATH: &str = "/org/aosabi/gatt/service0/char0";

/// The object path of RX.
const RX_PATH: &str = "/org/aosabi/gatt/service0/char1";

/// The object path of TX.
const TX_PATH: &str = "/org/aosabi/gatt/service0/char2";

/// The object path of the advertisement.
const ADV_PATH: &str = "/org/aosabi/gatt/advertisement0";

/// The interfaces of `BlueZ` and of the bus the service speaks.
const SERVICE_IFACE: &str = "org.bluez.GattService1";
const CHARACTERISTIC_IFACE: &str = "org.bluez.GattCharacteristic1";
const ADVERTISEMENT_IFACE: &str = "org.bluez.LEAdvertisement1";
const DEVICE_IFACE: &str = "org.bluez.Device1";
const ADAPTER_IFACE: &str = "org.bluez.Adapter1";
const PROPERTIES_IFACE: &str = "org.freedesktop.DBus.Properties";
const OBJECT_MANAGER_IFACE: &str = "org.freedesktop.DBus.ObjectManager";
const INTROSPECTABLE_IFACE: &str = "org.freedesktop.DBus.Introspectable";

/// The bytes of the notifications that may wait to be written to the bus
/// before the data path answers that it is not ready, whatever its pace.
const TX_QUEUE: usize = 64 * 1024;

/// The peer of the one central the data path serves.
pub const PEER: u32 = 0;

/// The error a write of a second central is refused with, which `bluetoothd`
/// answers a write request with as Write Not Permitted.
const NOT_PERMITTED: &str = "org.bluez.Error.NotPermitted";

/// The bytes a value in a Multiple Handle Value Notification carries beside
/// those of a single notification: its length.
const MULTIPLE_LENGTH: u16 = 2;

/// The nanoseconds `bluetoothd` holds the last notification of a burst to a
/// central that set the bit of Multiple Handle Value Notification before it
/// sends it alone, `NFY_MULT_TIMEOUT`.
const BURST_HOLD: Nanos = 10_000_000;

/// Returns the bytes of the largest value `bluetoothd` notifies whole at
/// `att_mtu` to every central, `ATT_MTU` less 5.
#[must_use]
pub const fn notified_len(att_mtu: u16) -> u16 {
    att_mtu.saturating_sub(3 + MULTIPLE_LENGTH)
}

/// Returns the bytes of the largest packet the data path sends to a central
/// at `att_mtu`: a value `bluetoothd` notifies whole, and never below the 20
/// bytes of the smallest packet of Homa, which a central that set the bit of
/// multiple notifications receives cut at an `ATT_MTU` below 25.
#[must_use]
pub const fn sent_packet(att_mtu: u16) -> u16 {
    let notified = notified_len(att_mtu);
    if notified > MIN_PACKET {
        notified
    } else {
        MIN_PACKET
    }
}

/// Lowers the `max_packet` of `value` to `cap` where `value` is a LIMITS, and
/// leaves every other packet as it is.
fn cap_limits(value: &mut Vec<u8>, cap: u16) {
    let Ok(decoded) = Compact::decode(value) else {
        return;
    };
    let Packet::Limits(mut limits) = decoded.packet else {
        return;
    };
    if limits.max_packet <= cap {
        return;
    }
    limits.max_packet = cap;
    let mut out = [0u8; Compact::MAX_HEADER];
    if let Ok(len) = Compact::header(
        &Packet::Limits(limits),
        decoded.ports,
        decoded.need_limits,
        &mut out,
    ) {
        value.clear();
        value.extend_from_slice(&out[..len]);
    }
}

/// An event of a link of the data path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GattEvent {
    /// The link of a central went up with the `ATT_MTU` of its link.
    Up {
        /// The central.
        peer: u32,
        /// Its `ATT_MTU`.
        att_mtu: u16,
    },
    /// The link of a central went down.
    Down {
        /// The central.
        peer: u32,
    },
}

/// What the data path counted, which a harness reads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GattCounts {
    /// The values the centrals wrote to RX.
    pub writes: u64,
    /// The notifications of TX.
    pub notifications: u64,
    /// The notifications dropped since no central was up.
    pub dropped: u64,
    /// The reads of the PSM characteristic.
    pub psm_reads: u64,
    /// The method calls the service refused, each answered with an error
    /// where it asks for a reply.
    pub refused: u64,
    /// The links that went up.
    pub ups: u64,
    /// The links that went down.
    pub downs: u64,
}

/// One packet of the trace of the data path.
#[cfg(feature = "trace")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TracedPacket {
    /// The nanoseconds since the UNIX epoch on the realtime clock at which
    /// `send` handed the packet to the bus or `recv` took it from RX.
    pub at_ns: u64,
    /// Whether `send` handed the packet to the bus, false where `recv` took
    /// it from RX.
    pub sent: bool,
    /// The bytes of `header` that hold the compact header.
    len: u8,
    /// The compact header of the packet in its first `len` bytes.
    header: [u8; MAX_HEADER_LEN],
}

#[cfg(feature = "trace")]
impl TracedPacket {
    /// Returns the compact header of the packet.
    #[must_use]
    pub fn header(&self) -> &[u8] {
        &self.header[..usize::from(self.len)]
    }
}

/// The trace of the data path, which keeps the first packets up to its
/// capacity and counts those past it.
///
/// A harness turns it on with [`Gatt::trace_on`] and reads it while and
/// after the data path runs, since the data path and the harness share it.
#[cfg(feature = "trace")]
#[derive(Debug)]
pub struct GattTrace {
    /// The packets kept, in the order they crossed.
    packets: RefCell<Vec<TracedPacket>>,
    /// The packets the trace keeps at most.
    capacity: usize,
    /// The packets past the capacity, which the trace counts and does not
    /// keep.
    dropped: Cell<u64>,
}

#[cfg(feature = "trace")]
impl GattTrace {
    /// Returns an empty trace that keeps at most `capacity` packets.
    fn new(capacity: usize) -> Self {
        Self {
            packets: RefCell::new(Vec::new()),
            capacity,
            dropped: Cell::new(0),
        }
    }

    /// Returns the packets the trace kept, in the order they crossed.
    #[must_use]
    pub fn packets(&self) -> Vec<TracedPacket> {
        self.packets.borrow().clone()
    }

    /// Returns the packets the trace keeps at most.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Returns the packets past the capacity, which the trace did not keep.
    #[must_use]
    pub fn dropped(&self) -> u64 {
        self.dropped.get()
    }

    /// Keeps a packet stamped now with the compact header `header`, one
    /// `send` handed to the bus where `sent` holds.
    fn push(&self, sent: bool, header: &[u8]) {
        let mut packets = self.packets.borrow_mut();
        if packets.len() >= self.capacity {
            self.dropped.set(self.dropped.get() + 1);
            return;
        }
        let at_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| {
                u64::try_from(since.as_nanos()).unwrap_or(u64::MAX)
            });
        let len = header.len().min(MAX_HEADER_LEN);
        let mut kept = TracedPacket {
            at_ns,
            sent,
            len: u8::try_from(len).unwrap_or(u8::MAX),
            header: [0; MAX_HEADER_LEN],
        };
        kept.header[..len].copy_from_slice(&header[..len]);
        packets.push(kept);
    }
}

/// Returns the bytes of the compact header at the head of the value
/// `value`: those before the payload of a DATA and the whole of any other
/// packet, which is how the engine hands `send` a header apart from its
/// payload.
#[cfg(feature = "trace")]
fn header_len(value: &[u8]) -> usize {
    match Compact::decode(value).map(|decoded| decoded.packet) {
        Ok(Packet::Data(data)) => value.len() - data.payload.len(),
        _ => value.len(),
    }
}

/// The count of the pace of the data path: the packets it handed the bus that
/// the paced link has not carried yet, which the slots of the pace retire.
///
/// A write of the central arrives right after the connection event that carried
/// it, so a write that arrives while the pace holds no packet sets the next
/// slot between two events of the link, the write that brings the link up among
/// them: halfway from that write to the last instant at which a notification
/// `bluetoothd` holds for its timer still leaves at the next event, half of the
/// interval less [`BURST_HOLD`] after it. A request the central wrote at the
/// same event then reaches the engine before the slot, and the last packet of
/// the pair the slot hands over leaves at the next event although `bluetoothd`
/// holds it. While the pace holds packets every slot follows the one before by
/// one interval, so a turn that wakes up to a quarter interval late does not
/// lower its rate. A slot retires at most `per_event` packets, and only those
/// handed at least three quarters of an interval before it, which left at the
/// event before it. The packets a turn hands over in the quarter interval after
/// a slot leave at the next event, while a packet handed later, after a write
/// woke a data path that held less than its depth, may come after that event
/// and leave at the one after it.
#[derive(Debug)]
struct Paced {
    /// The pace, each of its values at least 1.
    pace: Pace,
    /// The instant the link went up, from which the times below count, and
    /// `None` while it is down.
    origin: Option<Instant>,
    /// The next slot in nanoseconds after the origin.
    slot: Nanos,
    /// When the data path handed the bus each packet the link has not
    /// carried yet, in nanoseconds after the origin, oldest first.
    held: VecDeque<Nanos>,
}

impl Paced {
    /// Returns the count of a link paced at `pace`, which starts once the link
    /// goes up.
    ///
    /// A pace of no time, of no packet an event or of no depth would hold
    /// every packet, so each of those values counts as 1.
    fn new(pace: Pace) -> Self {
        Self {
            pace: Pace {
                interval: pace.interval.max(1),
                per_event: pace.per_event.max(1),
                tx_depth: pace.tx_depth.max(1),
            },
            origin: None,
            slot: 0,
            held: VecDeque::new(),
        }
    }

    /// Returns the nanoseconds from a write that sets the slots to the next
    /// slot, half of the interval less [`BURST_HOLD`].
    fn phase(&self) -> Nanos {
        self.pace.interval.saturating_sub(BURST_HOLD) / 2
    }

    /// Starts the slots of a link that goes up at `now`, the first of them
    /// [`Paced::phase`] later.
    fn start(&mut self, now: Instant) {
        self.origin = Some(now);
        self.slot = self.phase();
        self.held.clear();
    }

    /// Takes a write of the central read at `now` as the mark of the event
    /// that carried it, which sets the next slot [`Paced::phase`] later where
    /// the pace holds no packet.
    fn heard(&mut self, now: Instant) {
        self.settle(now);
        if self.held.is_empty() {
            self.slot = self.since(now).saturating_add(self.phase());
        }
    }

    /// Ends the slots of a link that went down, with the packets it held.
    fn stop(&mut self) {
        self.origin = None;
        self.held.clear();
    }

    /// Returns the nanoseconds from the moment the link went up to `now`, 0
    /// while it is down.
    fn since(&self, now: Instant) -> Nanos {
        self.origin.map_or(0, |origin| {
            u64::try_from(now.saturating_duration_since(origin).as_nanos()).unwrap_or(Nanos::MAX)
        })
    }

    /// Returns how many of the held packets the slots up to `at` retire, and
    /// the first slot after `at`.
    fn retired_by(&self, at: Nanos) -> (usize, Nanos) {
        let interval = self.pace.interval;
        let age = interval - interval / 4;
        let mut slot = self.slot;
        let mut retired = 0;
        while slot <= at {
            if retired == self.held.len() {
                // A slot with nothing left to retire changes nothing, so the
                // slots up to `at` pass at once.
                let passed = (at - slot) / interval + 1;
                slot = slot.saturating_add(passed.saturating_mul(interval));
                break;
            }
            let mut this = 0;
            while this < usize::from(self.pace.per_event)
                && self
                    .held
                    .get(retired)
                    .is_some_and(|&handed| handed.saturating_add(age) <= slot)
            {
                retired += 1;
                this += 1;
            }
            slot = slot.saturating_add(interval);
        }
        (retired, slot)
    }

    /// Retires the packets the slots up to `now` retire.
    fn settle(&mut self, now: Instant) {
        let (retired, slot) = self.retired_by(self.since(now));
        self.held.drain(..retired);
        self.slot = slot;
    }

    /// Answers whether the link has room for one more packet at `now`.
    fn ready(&self, now: Instant) -> bool {
        let (retired, _) = self.retired_by(self.since(now));
        self.held.len() - retired < usize::from(self.pace.tx_depth)
    }

    /// Counts one packet handed to the bus at `now`.
    fn sent(&mut self, now: Instant) {
        self.settle(now);
        self.held.push_back(self.since(now));
    }

    /// Returns the slot that retires the oldest packet held where the link
    /// holds its depth at `now`, and `None` where it has room.
    fn wake_at(&mut self, now: Instant) -> Option<Instant> {
        self.settle(now);
        if self.held.len() < usize::from(self.pace.tx_depth) {
            return None;
        }
        let interval = self.pace.interval;
        let due = self.held.front()?.saturating_add(interval - interval / 4);
        let mut slot = self.slot;
        if due > slot {
            let slots = (due - slot).div_ceil(interval);
            slot = slot.saturating_add(slots.saturating_mul(interval));
        }
        self.origin?.checked_add(Duration::from_nanos(slot))
    }
}

/// Why the service refused a method call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Refusal {
    /// The service exports no such method.
    Unknown,
    /// A central wrote while the link of another one is up.
    OtherCentral,
}

/// The service of Aosabi on `BlueZ`, the GATT data path as a carrier whose
/// one peer, [`PEER`], is the central it serves.
pub struct Gatt {
    /// The connection to the system bus.
    bus: Bus,
    /// The object path of the adapter.
    adapter: String,
    /// The PSM the characteristic answers.
    psm: u16,
    /// The object path of the device of the central whose link is up.
    central: Option<String>,
    /// Whether TX is subscribed.
    notifying: bool,
    /// The values written to RX, each with its central.
    received: VecDeque<(u32, Vec<u8>)>,
    /// The events of the links.
    events: VecDeque<GattEvent>,
    /// The place of the bus in the reactor of a runtime.
    source: Option<(Handle, SourceId)>,
    /// What the data path counted.
    counts: GattCounts,
    /// The count of the pace of the data path, `None` where it is not paced.
    paced: Option<Paced>,
    /// The trace of the data path, where a harness turned it on.
    #[cfg(feature = "trace")]
    trace: Option<Rc<GattTrace>>,
}

/// Returns the text form of a UUID of the service.
fn uuid_text(uuid: &[u8; 16]) -> String {
    let hex = uuid.iter().fold(String::new(), |mut hex, byte| {
        // A write to a string does not fail.
        let _ = write!(hex, "{byte:02x}");
        hex
    });
    format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    )
}

impl Gatt {
    /// Exports the service on `bus`, registers it with the adapter `adapter`,
    /// `hci0` for example, and advertises it, with `psm` as the PSM its
    /// characteristic answers.
    ///
    /// The adapter is powered and made pairable first, so a central can pair
    /// before it reads or writes a characteristic that requires encryption.
    /// The data path is paced at [`Pace::PLANNING`] until
    /// [`Gatt::set_pace`] names another pace or none.
    ///
    /// # Errors
    ///
    /// Returns the error of the bus and the error `BlueZ` answers a
    /// registration with.
    pub fn register(bus: Bus, adapter: &str, psm: u16) -> io::Result<Self> {
        let mut gatt = Self {
            bus,
            adapter: format!("/org/bluez/{adapter}"),
            psm,
            central: None,
            notifying: false,
            received: VecDeque::new(),
            events: VecDeque::new(),
            source: None,
            counts: GattCounts::default(),
            paced: Some(Paced::new(Pace::PLANNING)),
            #[cfg(feature = "trace")]
            trace: None,
        };
        gatt.bus.add_match(&format!(
            "type='signal',sender='{BLUEZ}',interface='{PROPERTIES_IFACE}',member='PropertiesChanged',arg0='{DEVICE_IFACE}'"
        ))?;
        gatt.bus.add_match(&format!(
            "type='signal',sender='{BLUEZ}',interface='{OBJECT_MANAGER_IFACE}',member='InterfacesRemoved'"
        ))?;
        for (name, value) in [("Powered", true), ("Pairable", true)] {
            let set = Message::call(
                BLUEZ,
                &gatt.adapter,
                PROPERTIES_IFACE,
                "Set",
                vec![
                    Value::Str(ADAPTER_IFACE.to_string()),
                    Value::Str(name.to_string()),
                    Value::Variant(alloc::boxed::Box::new(Value::Bool(value))),
                ],
            );
            gatt.bus.call(&set)?;
        }
        let adapter_path = gatt.adapter.clone();
        gatt.register_object(
            &adapter_path,
            "org.bluez.GattManager1",
            "RegisterApplication",
            APP_PATH,
        )?;
        gatt.register_object(
            &adapter_path,
            "org.bluez.LEAdvertisingManager1",
            "RegisterAdvertisement",
            ADV_PATH,
        )?;
        Ok(gatt)
    }

    /// Calls the registration `member` of `interface` on `path` for the
    /// object `object`, and answers every call `BlueZ` makes to the exported
    /// objects while the registration waits.
    fn register_object(
        &mut self,
        path: &str,
        interface: &str,
        member: &str,
        object: &str,
    ) -> io::Result<()> {
        let call = Message::call(
            BLUEZ,
            path,
            interface,
            member,
            vec![
                Value::Path(object.to_string()),
                Value::properties(Vec::new()),
            ],
        );
        let serial = self.bus.send(&call)?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(25);
        loop {
            while let Some(message) = self.bus.next_message()? {
                if message.reply_serial == Some(serial) {
                    if message.kind == Kind::Error {
                        return Err(io::Error::other(format!(
                            "BlueZ refused {member}: {}",
                            message.error_name.as_deref().unwrap_or_default()
                        )));
                    }
                    return Ok(());
                }
                self.handle(&message)?;
            }
            if std::time::Instant::now() > deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!("{member}: no reply"),
                ));
            }
            self.bus.flush()?;
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    /// Sets the PSM the characteristic answers, the number the channel is
    /// served on.
    pub fn set_psm(&mut self, psm: u16) {
        self.psm = psm;
    }

    /// Returns the PSM the characteristic answers.
    #[must_use]
    pub fn psm(&self) -> u16 {
        self.psm
    }

    /// Returns what the data path counted.
    #[must_use]
    pub fn counts(&self) -> GattCounts {
        self.counts
    }

    /// Paces the data path at `pace` from now on, or leaves it unpaced where
    /// `pace` is `None`.
    ///
    /// A data path with no pace hands the bus every packet while the bus has
    /// room, as the baseline of `AoFrame` alone does. A pace of no time, of no
    /// packet an event or of no depth takes 1 for that value, and a pace set
    /// while the link is up starts its slots now.
    pub fn set_pace(&mut self, pace: Option<Pace>) {
        self.paced = pace.map(Paced::new);
        if self.central.is_some()
            && let Some(paced) = &mut self.paced
        {
            paced.start(Instant::now());
        }
    }

    /// Returns the pace of the data path, each of its values at least 1, or
    /// `None` where the data path is not paced.
    #[must_use]
    pub fn pace(&self) -> Option<Pace> {
        self.paced.as_ref().map(|paced| paced.pace)
    }

    /// Returns the instant at which the pace frees room for the next packet,
    /// where it holds the data path at its depth now, so the task of the data
    /// path wakes then and the engine hands over what waits.
    pub fn paced_until(&mut self) -> Option<Instant> {
        self.paced.as_mut()?.wake_at(Instant::now())
    }

    /// Turns the trace of the data path on from here on, keeping at most
    /// `capacity` packets, and returns it.
    #[cfg(feature = "trace")]
    pub fn trace_on(&mut self, capacity: usize) -> Rc<GattTrace> {
        let trace = Rc::new(GattTrace::new(capacity));
        self.trace = Some(Rc::clone(&trace));
        trace
    }

    /// Keeps the packet with the compact header `header`, which `send` hands
    /// to the bus, in the trace where it is on.
    #[cfg(feature = "trace")]
    fn traced_send(&self, header: &[u8]) {
        if let Some(trace) = &self.trace {
            trace.push(true, header);
        }
    }

    /// Keeps the compact header of the value `value`, which `recv` took from
    /// RX, in the trace where it is on.
    ///
    /// The value is decoded for its header only where the trace is on, so a
    /// data path whose trace is off decodes nothing of it for the trace.
    #[cfg(feature = "trace")]
    fn traced_recv(&self, value: &[u8]) {
        if let Some(trace) = &self.trace {
            trace.push(false, &value[..header_len(value)]);
        }
    }

    /// Returns the connection to the system bus.
    #[must_use]
    pub fn bus(&self) -> &Bus {
        &self.bus
    }

    /// Adds the socket of the bus to the reactor of the runtime of `handle`,
    /// which removes it when the data path drops.
    ///
    /// # Errors
    ///
    /// Returns the error of the reactor.
    pub fn register_reactor(&mut self, handle: &Handle) -> io::Result<()> {
        let id = handle.reactor().add(self.bus.as_fd())?;
        self.source = Some((handle.clone(), id));
        Ok(())
    }

    /// Wakes the task of `waker` once the bus may be read, and once it may
    /// be written where bytes wait.
    pub fn register_interest(&self, waker: &Waker) {
        let Some((handle, id)) = &self.source else {
            return;
        };
        handle.reactor().register(*id, Interest::Read, waker);
        if self.bus.wants_write() {
            handle.reactor().register(*id, Interest::Write, waker);
        }
    }

    /// Reads every message the bus holds and answers every call to the
    /// exported objects, without blocking.
    ///
    /// # Errors
    ///
    /// Returns the error of the bus.
    pub fn poll(&mut self) -> io::Result<()> {
        self.bus.flush()?;
        while let Some(message) = self.bus.next_message()? {
            if let Some((handle, id)) = &self.source {
                handle.reactor().progressed(*id);
            }
            self.handle(&message)?;
        }
        self.bus.flush()
    }

    /// Returns the next event of a link.
    pub fn poll_event(&mut self) -> Option<GattEvent> {
        self.events.pop_front()
    }

    /// Returns whether values wait that [`Gatt::poll`] read.
    #[must_use]
    pub fn has_received(&self) -> bool {
        !self.received.is_empty()
    }

    /// Answers one message of the bus.
    fn handle(&mut self, message: &Message) -> io::Result<()> {
        match message.kind {
            Kind::MethodCall => self.answer_call(message),
            Kind::Signal => {
                self.watch(message);
                Ok(())
            }
            Kind::MethodReturn | Kind::Error => Ok(()),
        }
    }

    /// Takes the signals of the devices a link of which goes down.
    fn watch(&mut self, signal: &Message) {
        let Some(path) = signal.path.as_deref() else {
            return;
        };
        let gone = match signal.member.as_deref() {
            Some("PropertiesChanged") => {
                signal.body.first().and_then(Value::as_str) == Some(DEVICE_IFACE)
                    && signal
                        .body
                        .get(1)
                        .and_then(|changed| changed.get("Connected"))
                        == Some(&Value::Bool(false))
            }
            Some("InterfacesRemoved") => {
                let removed = signal
                    .body
                    .first()
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                return self.device_gone_at(removed);
            }
            _ => false,
        };
        if gone {
            self.device_gone_at(path);
        }
    }

    /// Takes the link down where the device `path` is the central's.
    fn device_gone_at(&mut self, path: &str) {
        if self.central.as_deref() == Some(path) {
            self.link_down();
        }
    }

    /// Takes the link of the central down where it is up, with the values
    /// it wrote that the endpoint has not taken.
    fn link_down(&mut self) {
        if self.central.take().is_some() {
            self.received.clear();
            if let Some(paced) = &mut self.paced {
                paced.stop();
            }
            self.counts.downs += 1;
            self.events.push_back(GattEvent::Down { peer: PEER });
        }
    }

    /// Answers one method call to the exported objects.
    fn answer_call(&mut self, call: &Message) -> io::Result<()> {
        let path = call.path.as_deref().unwrap_or_default();
        let reply = match (call.interface.as_deref(), call.member.as_deref()) {
            (Some(OBJECT_MANAGER_IFACE), Some("GetManagedObjects")) => {
                Ok(vec![managed_objects(path == ADV_PATH)])
            }
            (Some(PROPERTIES_IFACE), Some("GetAll")) => {
                let interface = call
                    .body
                    .first()
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                properties_of(path, interface)
                    .map(|properties| vec![properties])
                    .map_err(|()| Refusal::Unknown)
            }
            (Some(PROPERTIES_IFACE), Some("Get")) => {
                let interface = call
                    .body
                    .first()
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let name = call.body.get(1).and_then(Value::as_str).unwrap_or_default();
                properties_of(path, interface)
                    .and_then(|properties| properties.get(name).cloned().ok_or(()))
                    .map(|value| vec![Value::Variant(alloc::boxed::Box::new(value))])
                    .map_err(|()| Refusal::Unknown)
            }
            (Some(INTROSPECTABLE_IFACE), Some("Introspect")) => {
                Ok(vec![Value::Str(introspection(path))])
            }
            (Some(CHARACTERISTIC_IFACE), Some(member)) => self.characteristic(path, member, call),
            (Some(ADVERTISEMENT_IFACE), Some("Release")) => Ok(Vec::new()),
            // `bluetoothd` hands the advertisement the transmit power it chose,
            // which the service does not keep.
            (Some(PROPERTIES_IFACE), Some("Set")) if path == ADV_PATH => Ok(Vec::new()),
            _ => Err(Refusal::Unknown),
        };
        if reply.is_err() {
            self.counts.refused += 1;
        }
        if !call.wants_reply() {
            return Ok(());
        }
        let answer = match reply {
            Ok(body) => Message::reply(call, body),
            Err(Refusal::Unknown) => Message::error(
                call,
                "org.freedesktop.DBus.Error.UnknownMethod",
                "the service of Aosabi exports no such method",
            ),
            Err(Refusal::OtherCentral) => {
                Message::error(call, NOT_PERMITTED, "the data path serves another central")
            }
        };
        self.bus.send(&answer).map(|_| ())
    }

    /// Answers a call of the characteristic interface.
    fn characteristic(
        &mut self,
        path: &str,
        member: &str,
        call: &Message,
    ) -> Result<Vec<Value>, Refusal> {
        match (path, member) {
            (PSM_PATH, "ReadValue") => {
                self.counts.psm_reads += 1;
                Ok(vec![Value::bytes(&self.psm.to_le_bytes())])
            }
            (RX_PATH, "WriteValue") => {
                let value = call
                    .body
                    .first()
                    .and_then(Value::as_bytes)
                    .ok_or(Refusal::Unknown)?;
                let options = call.body.get(1);
                let device = options
                    .and_then(|options| options.get("device"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let att_mtu = match options.and_then(|options| options.get("mtu")) {
                    Some(Value::U16(mtu)) => *mtu,
                    _ => DEFAULT_ATT_MTU,
                };
                self.written(device, att_mtu, value)?;
                Ok(Vec::new())
            }
            (TX_PATH, "StartNotify") => {
                self.notifying = true;
                Ok(Vec::new())
            }
            (TX_PATH, "StopNotify") => {
                self.notifying = false;
                self.link_down();
                Ok(Vec::new())
            }
            _ => Err(Refusal::Unknown),
        }
    }

    /// Takes one value a central wrote, and brings the link up for it first
    /// where TX is subscribed and no central's link is up. A value of a
    /// central written while TX is not subscribed is dropped.
    ///
    /// # Errors
    ///
    /// Returns [`Refusal::OtherCentral`] where the link of another central
    /// is up, whose value the data path does not take.
    fn written(&mut self, device: String, att_mtu: u16, mut value: Vec<u8>) -> Result<(), Refusal> {
        self.counts.writes += 1;
        match self.central.as_deref() {
            Some(central) if central != device => return Err(Refusal::OtherCentral),
            Some(_) => {}
            None if self.notifying => {
                self.central = Some(device);
                self.counts.ups += 1;
                if let Some(paced) = &mut self.paced {
                    paced.start(Instant::now());
                }
                self.events.push_back(GattEvent::Up {
                    peer: PEER,
                    att_mtu,
                });
            }
            None => return Ok(()),
        }
        if let Some(paced) = &mut self.paced {
            paced.heard(Instant::now());
        }
        cap_limits(&mut value, sent_packet(att_mtu));
        self.received.push_back((PEER, value));
        Ok(())
    }

    /// Answers whether the link of the central is up for `peer`.
    fn up(&self, peer: u32) -> bool {
        peer == PEER && self.central.is_some()
    }
}

impl AsFd for Gatt {
    fn as_fd(&self) -> std::os::fd::BorrowedFd<'_> {
        self.bus.as_fd()
    }
}

impl Drop for Gatt {
    fn drop(&mut self) {
        if let Some((handle, id)) = self.source.take() {
            handle.reactor().remove(self.bus.as_fd(), id);
        }
    }
}

impl Carrier for Gatt {
    /// The one central the data path serves, [`PEER`].
    type Peer = u32;
    type Error = io::Error;

    /// Answers whether the central's link is up, the pace has room for one
    /// more packet and the bus has room for a notification.
    fn ready(&self, peer: u32, _len: usize) -> bool {
        self.up(peer)
            && self.notifying
            && self
                .paced
                .as_ref()
                .is_none_or(|paced| paced.ready(Instant::now()))
            && !self.bus.wants_write_more_than(TX_QUEUE)
    }

    /// Notifies one packet on TX through `PropertiesChanged` of its `Value`,
    /// which the pace counts until a slot retires it. A packet for a central
    /// whose link is down is dropped and counted.
    fn send(
        &mut self,
        peer: u32,
        _priority: Priority,
        header: &[u8],
        payload: &[u8],
    ) -> io::Result<()> {
        if !self.up(peer) || !self.notifying {
            self.counts.dropped += 1;
            return Ok(());
        }
        let mut value = Vec::with_capacity(header.len() + payload.len());
        value.extend_from_slice(header);
        value.extend_from_slice(payload);
        let changed = Message::signal(
            TX_PATH,
            PROPERTIES_IFACE,
            "PropertiesChanged",
            vec![
                Value::Str(CHARACTERISTIC_IFACE.to_string()),
                Value::properties(vec![("Value", Value::bytes(&value))]),
                Value::strings(&[]),
            ],
        );
        self.counts.notifications += 1;
        if let Some(paced) = &mut self.paced {
            paced.sent(Instant::now());
        }
        #[cfg(feature = "trace")]
        self.traced_send(header);
        self.bus.emit(&changed)
    }

    /// Takes the next value a central wrote that [`Gatt::poll`] read, once
    /// every event of a link that came before it has been taken, so a
    /// central's first write reaches the endpoint after its link is up.
    fn recv(&mut self, buf: &mut [u8]) -> io::Result<Option<(u32, usize)>> {
        if !self.events.is_empty() {
            return Ok(None);
        }
        let Some((peer, value)) = self.received.pop_front() else {
            return Ok(None);
        };
        #[cfg(feature = "trace")]
        self.traced_recv(&value);
        let len = value.len().min(buf.len());
        buf[..len].copy_from_slice(&value[..len]);
        Ok(Some((peer, len)))
    }
}

/// Returns the objects of the application, or of the advertisement where
/// `advertisement` holds, as `GetManagedObjects` answers them.
fn managed_objects(advertisement: bool) -> Value {
    let paths: &[&str] = if advertisement {
        &[ADV_PATH]
    } else {
        &[SERVICE_PATH, PSM_PATH, RX_PATH, TX_PATH]
    };
    let entries = paths
        .iter()
        .filter_map(|path| {
            let interface = interface_of(path)?;
            let properties = properties_of(path, interface).ok()?;
            Some((
                Value::Path((*path).to_string()),
                Value::Dict(
                    "s".to_string(),
                    "a{sv}".to_string(),
                    vec![(Value::Str(interface.to_string()), properties)],
                ),
            ))
        })
        .collect();
    Value::Dict("o".to_string(), "a{sa{sv}}".to_string(), entries)
}

/// Returns the interface the exported object `path` carries.
fn interface_of(path: &str) -> Option<&'static str> {
    match path {
        SERVICE_PATH => Some(SERVICE_IFACE),
        PSM_PATH | RX_PATH | TX_PATH => Some(CHARACTERISTIC_IFACE),
        ADV_PATH => Some(ADVERTISEMENT_IFACE),
        _ => None,
    }
}

/// Returns the properties of the exported object `path` under `interface`.
///
/// The PSM characteristic is read with encryption, RX is written without
/// response with encryption, and TX notifies with encryption, so a central
/// pairs before the data path moves a byte.
fn properties_of(path: &str, interface: &str) -> Result<Value, ()> {
    if interface_of(path) != Some(interface) {
        return Err(());
    }
    let characteristic = |uuid: &[u8; 16], flags: &[&str]| {
        Value::properties(vec![
            ("UUID", Value::Str(uuid_text(uuid))),
            ("Service", Value::Path(SERVICE_PATH.to_string())),
            ("Flags", Value::strings(flags)),
        ])
    };
    Ok(match path {
        SERVICE_PATH => Value::properties(vec![
            ("UUID", Value::Str(uuid_text(&SERVICE))),
            ("Primary", Value::Bool(true)),
        ]),
        PSM_PATH => characteristic(&PSM_CHARACTERISTIC, &["encrypt-read"]),
        RX_PATH => characteristic(&RX, &["write-without-response", "encrypt-write"]),
        TX_PATH => characteristic(&TX, &["notify", "encrypt-notify"]),
        _ => Value::properties(vec![
            ("Type", Value::Str("peripheral".to_string())),
            ("ServiceUUIDs", Value::strings(&[&uuid_text(&SERVICE)])),
            ("LocalName", Value::Str("Aosabi".to_string())),
        ]),
    })
}

/// Returns the introspection data of the exported object `path`.
fn introspection(path: &str) -> String {
    let interface = interface_of(path).unwrap_or(OBJECT_MANAGER_IFACE);
    format!(
        "<!DOCTYPE node PUBLIC \"-//freedesktop//DTD D-BUS Object Introspection 1.0//EN\" \
         \"http://www.freedesktop.org/standards/dbus/1.0/introspect.dtd\">\
         <node><interface name=\"{interface}\"/><interface name=\"{PROPERTIES_IFACE}\"/></node>"
    )
}
