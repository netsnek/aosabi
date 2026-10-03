// SPDX-License-Identifier: MIT
/*
 * aosabi-carrier-usb linux ffs: the bulk function over a mounted FunctionFS,
 * its descriptors and strings, its control endpoint and its two bulk
 * endpoints.
 *
 * No analogue on the reference side. The layouts are those of the FunctionFS
 * interface of include/uapi/linux/usb/functionfs.h and of
 * drivers/usb/gadget/function/f_fs.c at the tag v6.17 of Linux, read at
 * https://github.com/torvalds/linux/tree/v6.17, the descriptors of the USB 2.0
 * specification, sections 9.6.5 and 9.6.6, and the extended compat ID of the
 * Microsoft OS 1.0 descriptors.
 */

//! The bulk function over `FunctionFS`.
//!
//! [`descriptors`] and [`strings`] write what `FunctionFS` takes on `ep0`: one
//! vendor interface of class 0xFF with a bulk IN and a bulk OUT endpoint in
//! the full speed set, and at high speed the high speed and `SuperSpeed` sets
//! beside it, and the extended compat ID `WINUSB`, which a host reads where
//! the gadget of the card turns its OS descriptors on. The vendor and the
//! product id and the serial number are the gadget's, which the card's setup
//! composes in configfs, and never this crate's.
//!
//! [`Bulk`] opens the mount: it writes the descriptors and the strings to
//! `ep0`, reads the events of the function from `ep0` without blocking, and
//! moves one Homa packet a transfer through the two endpoint files. A read of
//! an endpoint file that `FunctionFS` has enabled blocks until the host sends a
//! transfer and a write blocks until the host reads it, whatever `O_NONBLOCK`
//! says, and the files answer no `poll`, so neither a reactor nor a sweep of it
//! can wait on them. Each endpoint file is therefore moved by a relay, a mover
//! process that blocks on the file and exchanges one datagram a transfer with
//! the driver over a Unix datagram socket pair, which the reactor waits on, and
//! a watcher process that ends the mover once the driver drops the relay or is
//! gone. A transfer whose length is an exact multiple of `wMaxPacketSize` below
//! the largest packet is followed by a write of no bytes, which `FunctionFS`
//! sends as a zero length packet.
//!
//! `wMaxPacketSize` is read back where the function is enabled: the speed the
//! UDC reports in `/sys/class/udc/*/current_speed` names the set of
//! descriptors the host took, since the endpoint descriptor of the host's
//! choice reaches user space only through an ioctl, which the crate cannot
//! call without `unsafe`.

use alloc::{
    collections::VecDeque,
    vec::Vec, //
};
use std::{
    fs::{
        self,
        File,
        OpenOptions, //
    },
    io::{
        self,
        ErrorKind,
        Read as _,
        Write as _, //
    },
    os::{
        fd::{
            AsFd,
            BorrowedFd, //
        },
        unix::fs::OpenOptionsExt as _, //
    },
    path::{
        Path,
        PathBuf, //
    }, //
};

use aosabi_serve::runtime::{
    Handle,
    Interest,
    SourceId, //
};
use homa::{
    Carrier,
    Priority, //
};

use super::relay::{
    Direction,
    Relay,
    Transfer, //
};
use crate::profile::{
    Speed,
    needs_zlp, //
};

/// The magic of the descriptors of the second version.
const DESCRIPTORS_MAGIC_V2: u32 = 3;

/// The magic of the strings.
const STRINGS_MAGIC: u32 = 2;

/// The flags of the descriptors: full speed, high speed, `SuperSpeed` and the
/// Microsoft OS descriptors.
const HAS_FS_DESC: u32 = 1;
const HAS_HS_DESC: u32 = 2;
const HAS_SS_DESC: u32 = 4;
const HAS_MS_OS_DESC: u32 = 8;

/// The US English language id of the strings.
const LANG_EN_US: u16 = 0x0409;

/// The bytes of one event of `ep0`, `struct usb_functionfs_event`, a setup
/// packet of 8 bytes, the type and 3 bytes of padding.
const EVENT_LEN: usize = 12;

/// The `wMaxPacketSize` of a bulk endpoint at `SuperSpeed`.
const SUPER_SPEED_PACKET: u16 = 1024;

/// The Linux value of `O_NONBLOCK`.
const O_NONBLOCK: i32 = 1 << 11;

/// The directory where each UDC reports the speed it runs at.
const UDC_DIR: &str = "/sys/class/udc";

/// An event of the function that `ep0` reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FfsEvent {
    /// The function was bound to a UDC.
    Bind,
    /// The function was unbound from its UDC.
    Unbind,
    /// The host chose a configuration that holds the function.
    Enable,
    /// The host left the configuration, reset the bus or the device went
    /// off the bus.
    Disable,
    /// A control request to the interface arrived, which the function
    /// stalls, with the byte `bmRequestType` of its setup packet.
    Setup(u8),
    /// The bus was suspended.
    Suspend,
    /// The bus resumed.
    Resume,
}

impl FfsEvent {
    /// Returns the event of a type of `struct usb_functionfs_event`, `None` for
    /// a type the interface does not define.
    fn of(kind: u8, request_type: u8) -> Option<Self> {
        Some(match kind {
            0 => Self::Bind,
            1 => Self::Unbind,
            2 => Self::Enable,
            3 => Self::Disable,
            4 => Self::Setup(request_type),
            5 => Self::Suspend,
            6 => Self::Resume,
            _ => return None,
        })
    }
}

/// Returns the interface descriptor of the function, one vendor interface
/// with two endpoints and the string 1 as its name.
fn interface_descriptor() -> [u8; 9] {
    [9, 4, 0, 0, 2, 0xFF, 0, 0, 1]
}

/// Returns the endpoint descriptor of a bulk endpoint at `address` with
/// `max_packet_size`.
fn endpoint_descriptor(address: u8, max_packet_size: u16) -> [u8; 7] {
    let [low, high] = max_packet_size.to_le_bytes();
    [7, 5, address, 2, low, high, 0]
}

/// Returns the `SuperSpeed` endpoint companion descriptor of a bulk endpoint
/// with no burst and no streams.
fn companion_descriptor() -> [u8; 6] {
    [6, 0x30, 0, 0, 0, 0]
}

/// Appends one set of descriptors, the interface and the IN and OUT
/// endpoints, the IN endpoint first, so `FunctionFS` names it `ep1` and the OUT
/// endpoint `ep2`, and returns the count of descriptors it appended.
fn push_set(out: &mut Vec<u8>, max_packet_size: u16, companion: bool) -> u32 {
    out.extend_from_slice(&interface_descriptor());
    out.extend_from_slice(&endpoint_descriptor(0x81, max_packet_size));
    if companion {
        out.extend_from_slice(&companion_descriptor());
    }
    out.extend_from_slice(&endpoint_descriptor(0x02, max_packet_size));
    if companion {
        out.extend_from_slice(&companion_descriptor());
    }
    if companion { 5 } else { 3 }
}

/// Returns the descriptors `FunctionFS` takes on `ep0` for a function that
/// runs at `speed`, with `max_packet_size` the `wMaxPacketSize` of its bulk
/// endpoints at that speed.
///
/// A function at full speed carries the full speed set alone. A function at
/// high speed carries the full speed set with packets of 64 bytes, the high
/// speed set with `max_packet_size` and the `SuperSpeed` set with 1,024 and its
/// companions, so the host takes the set of the speed the bus runs at. The
/// extended compat ID `WINUSB` of the interface follows.
#[must_use]
pub fn descriptors(speed: Speed, max_packet_size: u16) -> Vec<u8> {
    let mut sets = Vec::new();
    let (flags, counts) = match speed {
        Speed::Full => {
            let fs = push_set(&mut sets, max_packet_size, false);
            (HAS_FS_DESC | HAS_MS_OS_DESC, [fs, 0, 0])
        }
        Speed::High => {
            let fs = push_set(&mut sets, Speed::Full.max_packet_size(), false);
            let hs = push_set(&mut sets, max_packet_size, false);
            let ss = push_set(&mut sets, SUPER_SPEED_PACKET, true);
            (
                HAS_FS_DESC | HAS_HS_DESC | HAS_SS_DESC | HAS_MS_OS_DESC,
                [fs, hs, ss],
            )
        }
    };
    let os = compat_id();
    let mut head = Vec::new();
    head.extend_from_slice(&DESCRIPTORS_MAGIC_V2.to_le_bytes());
    head.extend_from_slice(&0u32.to_le_bytes());
    head.extend_from_slice(&flags.to_le_bytes());
    for count in counts.into_iter().filter(|&count| count > 0) {
        head.extend_from_slice(&count.to_le_bytes());
    }
    head.extend_from_slice(&1u32.to_le_bytes());
    head.extend_from_slice(&sets);
    head.extend_from_slice(&os);
    let length = u32::try_from(head.len()).unwrap_or(u32::MAX);
    head[4..8].copy_from_slice(&length.to_le_bytes());
    head
}

/// Returns the one OS descriptor of the function, the extended compat ID
/// `WINUSB` of interface 0: the header of `FunctionFS`,
/// `struct usb_os_desc_header`, with the version 0x0100, the index 4 and one
/// entry, and the entry, `struct usb_ext_compat_desc`.
fn compat_id() -> Vec<u8> {
    let mut entry = Vec::with_capacity(24);
    entry.push(0);
    entry.push(1);
    entry.extend_from_slice(b"WINUSB\0\0");
    entry.extend_from_slice(&[0u8; 8]);
    entry.extend_from_slice(&[0u8; 6]);
    let mut out = Vec::with_capacity(11 + entry.len());
    out.push(0);
    let length = u32::try_from(11 + entry.len()).unwrap_or(u32::MAX);
    out.extend_from_slice(&length.to_le_bytes());
    out.extend_from_slice(&0x0100u16.to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.push(1);
    out.push(0);
    out.extend_from_slice(&entry);
    out
}

/// Returns the strings `FunctionFS` takes on `ep0`, one string in US English,
/// the name `interface` of the interface.
#[must_use]
pub fn strings(interface: &str) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&STRINGS_MAGIC.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&LANG_EN_US.to_le_bytes());
    out.extend_from_slice(interface.as_bytes());
    out.push(0);
    let length = u32::try_from(out.len()).unwrap_or(u32::MAX);
    out[4..8].copy_from_slice(&length.to_le_bytes());
    out
}

/// Returns the `wMaxPacketSize` of the bulk endpoints at the speed the one
/// UDC that runs reports, or `None` where no UDC or more than one reports a
/// speed.
#[must_use]
pub fn negotiated_packet_size() -> Option<u16> {
    let mut found = None;
    for entry in fs::read_dir(UDC_DIR).ok()?.flatten() {
        let Ok(speed) = fs::read_to_string(entry.path().join("current_speed")) else {
            continue;
        };
        let size = match speed.trim() {
            "full-speed" => Speed::Full.max_packet_size(),
            "high-speed" => Speed::High.max_packet_size(),
            "super-speed" | "super-speed-plus" => SUPER_SPEED_PACKET,
            _ => continue,
        };
        if found.replace(size).is_some() {
            return None;
        }
    }
    found
}

/// What a bulk function counted, which a harness reads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BulkCounts {
    /// The transfers the host sent and the relay handed over.
    pub transfers_in: u64,
    /// The transfers handed to the relay of the IN endpoint.
    pub transfers_out: u64,
    /// The zero length packets handed to the relay after an exact multiple.
    pub zero_length: u64,
    /// The packets dropped while the function was not enabled.
    pub dropped_down: u64,
    /// The relays started.
    pub relays_started: u64,
    /// The relays that ended with an error of their endpoint file.
    pub relays_ended: u64,
    /// The events `ep0` reported.
    pub events: u64,
    /// The control requests stalled.
    pub stalled: u64,
}

/// The bulk function over a mounted `FunctionFS`, one Homa packet a
/// transfer.
pub struct Bulk {
    /// The mount.
    dir: PathBuf,
    /// The control endpoint, open for reading and writing, non blocking.
    ep0: File,
    /// The relay of the OUT endpoint, `ep2`, which hands over each transfer
    /// the host sent.
    out: Relay,
    /// The relay of the IN endpoint, `ep1`, which writes each transfer.
    inn: Relay,
    /// The profile of the speed the function was opened for.
    speed: Speed,
    /// The `wMaxPacketSize` of the endpoints at the speed the host took.
    max_packet_size: u16,
    /// The largest packet of the profile.
    max_packet: u16,
    /// Whether the host enabled the function.
    enabled: bool,
    /// The datagrams for the IN relay the socket did not take yet.
    pending: VecDeque<Vec<u8>>,
    /// The places of `ep0` and of the two relays in the reactor of a runtime.
    sources: Option<(Handle, [SourceId; 3])>,
    /// What the function counted.
    counts: BulkCounts,
}

impl Bulk {
    /// Writes the descriptors and the strings to `dir/ep0`, opens the
    /// control endpoint non blocking and starts the relays of the two
    /// endpoint files.
    ///
    /// # Errors
    ///
    /// Returns the error of a write to `ep0`, which `FunctionFS` answers for
    /// descriptors it refuses, and of the start of a relay.
    pub fn open(dir: &Path, speed: Speed) -> io::Result<Self> {
        let mut ep0 = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(O_NONBLOCK)
            .open(dir.join("ep0"))?;
        ep0.write_all(&descriptors(speed, speed.max_packet_size()))?;
        ep0.write_all(&strings("Aosabi"))?;
        let profile = crate::profile::bulk_profile(speed);
        let out = Relay::start(Direction::Out, &dir.join("ep2"), profile.max_packet)?;
        let inn = Relay::start(Direction::In, &dir.join("ep1"), profile.max_packet)?;
        Ok(Self {
            dir: dir.to_path_buf(),
            ep0,
            out,
            inn,
            speed,
            max_packet_size: speed.max_packet_size(),
            max_packet: profile.max_packet,
            enabled: false,
            pending: VecDeque::new(),
            sources: None,
            counts: BulkCounts::default(),
        })
    }

    /// Returns the speed the function was opened for.
    #[must_use]
    pub fn speed(&self) -> Speed {
        self.speed
    }

    /// Returns the `wMaxPacketSize` of the endpoints at the speed the host
    /// took.
    #[must_use]
    pub fn max_packet_size(&self) -> u16 {
        self.max_packet_size
    }

    /// Returns the largest packet of the profile of the function.
    #[must_use]
    pub fn max_packet(&self) -> u16 {
        self.max_packet
    }

    /// Returns whether the host enabled the function.
    #[must_use]
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Returns what the function counted.
    #[must_use]
    pub fn counts(&self) -> BulkCounts {
        self.counts
    }

    /// Adds `ep0` and the sockets of the two relays to the reactor of the
    /// runtime of `handle`, which removes them when the function drops.
    ///
    /// # Errors
    ///
    /// Returns the error of the reactor.
    pub fn register(&mut self, handle: &Handle) -> io::Result<()> {
        let reactor = handle.reactor();
        let ep0 = reactor.add(self.ep0.as_fd())?;
        let out = reactor.add(self.out.as_fd())?;
        let inn = reactor.add(self.inn.as_fd())?;
        self.sources = Some((handle.clone(), [ep0, out, inn]));
        Ok(())
    }

    /// Wakes the task of `waker` once `ep0` or the relay of the OUT endpoint
    /// may be read, and once the relay of the IN endpoint may take a
    /// datagram where some wait.
    pub fn register_interest(&self, waker: &core::task::Waker) {
        let Some((handle, [ep0, out, inn])) = &self.sources else {
            return;
        };
        let reactor = handle.reactor();
        reactor.register(*ep0, Interest::Read, waker);
        reactor.register(*out, Interest::Read, waker);
        reactor.register(*inn, Interest::Read, waker);
        if !self.pending.is_empty() {
            reactor.register(*inn, Interest::Write, waker);
        }
    }

    /// Reports a transfer that moved, which the poll reactor reads as heat.
    fn progressed(&self, index: usize) {
        if let Some((handle, sources)) = &self.sources
            && let Some(id) = sources.get(index)
        {
            handle.reactor().progressed(*id);
        }
    }

    /// Returns the next event of the function, or `None` where `ep0` holds
    /// none, and stalls a control request to the interface, which the
    /// function answers none of.
    ///
    /// An enable reads back the `wMaxPacketSize` of the speed the host took,
    /// and a disable and an unbind stop the relays and start new ones, which
    /// wait until the host enables the function again.
    ///
    /// # Errors
    ///
    /// Returns the error of `ep0`, and of the start of a relay.
    pub fn poll_event(&mut self) -> io::Result<Option<FfsEvent>> {
        let mut event = [0u8; EVENT_LEN];
        let read = match self.ep0.read(&mut event) {
            Ok(read) => read,
            Err(error) if error.kind() == ErrorKind::WouldBlock => return Ok(None),
            Err(error) if error.kind() == ErrorKind::Interrupted => return Ok(None),
            Err(error) => return Err(error),
        };
        if read < EVENT_LEN {
            return Ok(None);
        }
        self.counts.events += 1;
        let Some(event) = FfsEvent::of(event[8], event[0]) else {
            return Ok(None);
        };
        match event {
            FfsEvent::Enable => {
                self.enabled = true;
                self.max_packet_size =
                    negotiated_packet_size().unwrap_or(self.speed.max_packet_size());
            }
            FfsEvent::Disable | FfsEvent::Unbind => {
                if self.enabled {
                    self.enabled = false;
                    self.restart_relays()?;
                }
            }
            FfsEvent::Setup(request_type) => self.stall(request_type),
            FfsEvent::Bind | FfsEvent::Suspend | FfsEvent::Resume => {}
        }
        Ok(Some(event))
    }

    /// Stalls the control request whose `bmRequestType` is `request_type`: a
    /// read of `ep0` stalls a request whose data stage goes to the host and a
    /// write one whose data stage comes from it.
    fn stall(&mut self, request_type: u8) {
        self.counts.stalled += 1;
        let mut byte = [0u8; 1];
        // The stall answers the host with an error of the endpoint, which the
        // call returns as its own error, so the result carries nothing.
        let _ = if request_type & 0x80 == 0 {
            self.ep0.write(&byte)
        } else {
            self.ep0.read(&mut byte)
        };
    }

    /// Stops both relays and starts new ones, which wait until the function
    /// is enabled again, and drops what waited for the old IN relay.
    fn restart_relays(&mut self) -> io::Result<()> {
        let reactor = self.sources.as_ref().map(|(handle, _)| handle.clone());
        if let (Some(handle), Some((_, sources))) = (&reactor, &self.sources) {
            handle.reactor().remove(self.out.as_fd(), sources[1]);
            handle.reactor().remove(self.inn.as_fd(), sources[2]);
        }
        self.pending.clear();
        self.out = Relay::start(Direction::Out, &self.dir.join("ep2"), self.max_packet)?;
        self.inn = Relay::start(Direction::In, &self.dir.join("ep1"), self.max_packet)?;
        self.counts.relays_started += 2;
        if let (Some(handle), Some((_, sources))) = (reactor, self.sources.as_mut()) {
            sources[1] = handle.reactor().add(self.out.as_fd())?;
            sources[2] = handle.reactor().add(self.inn.as_fd())?;
        }
        Ok(())
    }

    /// Hands the relay of the IN endpoint what waits, as far as its socket
    /// takes it.
    fn flush(&mut self) -> io::Result<()> {
        while let Some(front) = self.pending.front() {
            if !self.inn.send(front)? {
                return Ok(());
            }
            self.pending.pop_front();
            self.progressed(2);
        }
        Ok(())
    }

    /// Returns the relays' end where one ended, which the next enable
    /// replaces.
    fn relay_ended(&mut self) -> io::Result<()> {
        self.counts.relays_ended += 1;
        if self.enabled {
            self.enabled = false;
            self.restart_relays()?;
        }
        Ok(())
    }
}

impl AsFd for Bulk {
    /// Returns `ep0`, the descriptor whose events say whether the link is up.
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.ep0.as_fd()
    }
}

impl Drop for Bulk {
    fn drop(&mut self) {
        if let Some((handle, [ep0, out, inn])) = self.sources.take() {
            let reactor = handle.reactor();
            reactor.remove(self.ep0.as_fd(), ep0);
            reactor.remove(self.out.as_fd(), out);
            reactor.remove(self.inn.as_fd(), inn);
        }
    }
}

impl Carrier for Bulk {
    type Peer = ();
    type Error = io::Error;

    /// Answers whether the function is enabled and the relay of the IN
    /// endpoint took every packet so far.
    fn ready(&self, _peer: (), _len: usize) -> bool {
        self.enabled && self.pending.is_empty()
    }

    /// Hands one packet to the relay of the IN endpoint as one transfer,
    /// followed by a zero length packet where its length is an exact
    /// multiple of `wMaxPacketSize` below the largest packet. A packet of a
    /// function the host has not enabled is dropped and counted, as a
    /// suspended or detached bus loses it, and Homa's RESEND recovers it.
    fn send(
        &mut self,
        _peer: (),
        _priority: Priority,
        header: &[u8],
        payload: &[u8],
    ) -> io::Result<()> {
        if !self.enabled {
            self.counts.dropped_down += 1;
            return Ok(());
        }
        let len = header.len() + payload.len();
        self.pending
            .push_back(Transfer::datagram(&[header, payload]));
        self.counts.transfers_out += 1;
        if needs_zlp(len, self.max_packet_size, self.max_packet) {
            self.pending.push_back(Transfer::datagram(&[]));
            self.counts.zero_length += 1;
        }
        self.flush()
    }

    /// Takes the next transfer the host sent, skipping a transfer of no
    /// bytes, and reads a relay whose endpoint file failed as a link that
    /// went down.
    fn recv(&mut self, buf: &mut [u8]) -> io::Result<Option<((), usize)>> {
        if let Some(Transfer::Ended) = self.inn.receive(buf)? {
            self.relay_ended()?;
            return Ok(None);
        }
        self.flush()?;
        loop {
            match self.out.receive(buf)? {
                None => return Ok(None),
                Some(Transfer::Data(0)) => {}
                Some(Transfer::Data(len)) => {
                    self.counts.transfers_in += 1;
                    self.progressed(1);
                    return Ok(Some(((), len)));
                }
                Some(Transfer::Ended) => {
                    self.relay_ended()?;
                    return Ok(None);
                }
            }
        }
    }
}
