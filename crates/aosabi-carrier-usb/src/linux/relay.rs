// SPDX-License-Identifier: MIT
/*
 * aosabi-carrier-usb linux relay: the relay of one endpoint file of
 * FunctionFS, a mover, a process of its own that blocks on the file and
 * exchanges one datagram a transfer with the driver, and its watcher, which
 * ends the mover once the driver is gone.
 *
 * No analogue on the reference side. The relay exists because a read or a
 * write of an endpoint file of FunctionFS that the host enabled blocks
 * whatever O_NONBLOCK says and the file answers no poll, while a driver of the
 * runtime of aosabi-serve starts no thread and blocks on no file in its turns,
 * and the workspace holds no unsafe to reach the asynchronous interface of the
 * file.
 */

//! The relay of an endpoint file.
//!
//! [`Relay::start`] creates a Unix datagram socket pair, starts the program
//! of this process again as the watcher of the relay, with the variable
//! `AOSABI_USB_RELAY` naming the role, the direction and the endpoint file,
//! a pipe as its standard input and one end of the pair as its standard
//! output, and keeps the other end of the pair, which the reactor of the
//! runtime waits on, and the other end of the pipe. The watcher starts the
//! program once more as the mover, with its own process id in the variable
//! and that end of the pair as its standard input and output, and reads the
//! pipe, to which the driver writes nothing. The read returns at the end of
//! the pipe, where the driver drops the relay or is gone in any way, a kill
//! included, and the watcher then ends the mover and itself. So, while its
//! watcher lives, a mover that blocks on its endpoint file never outlives its
//! driver and never keeps the function open against a driver that starts
//! again. The program answers the variable before anything else through
//! [`relay_from_env`], which a binary that links the bulk driver calls first
//! in its `main`, the facade's `serve` and the devtool among them, so no
//! second program ships beside a card's binary.
//!
//! Every datagram opens with a byte of its kind: 0 carries one transfer, the
//! bytes after it, and 1 carries the end of the relay, the text of the error
//! of its endpoint file after it. The mover of the OUT endpoint reads one
//! transfer a read, at most the largest packet, and sends each as a datagram
//! of kind 0. The mover of the IN endpoint writes each datagram of kind 0 as
//! one transfer, a datagram of no bytes after its kind as a zero length
//! packet. Either mover sends a datagram of kind 1 and ends where its file
//! fails, which `FunctionFS` answers with `ESHUTDOWN` once the host leaves the
//! configuration.
//!
//! At the top of each turn of its loop, before the read of its file on the
//! OUT endpoint and before the receive from its socket on the IN endpoint, a
//! mover looks whether its parent is still the watcher whose process id its
//! variable names, and ends where it is not, so a mover whose watcher is gone
//! before its first look ends at that look. No look stands between the read
//! and the send of the OUT mover or between the receive and the write of the
//! IN mover. So a mover whose watcher alone was killed ends after the next
//! transfer it moves, and the mover of the IN endpoint ends within 1 s as
//! well while it waits for a datagram. The mover of the OUT endpoint blocks
//! on its file between transfers, so it holds the file until the host sends
//! a transfer or leaves the configuration.

use alloc::{
    format,
    vec,
    vec::Vec, //
};
use std::{
    env,
    fs::{
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
            BorrowedFd,
            OwnedFd, //
        },
        unix::{
            net::UnixDatagram,
            process::parent_id, //
        }, //
    },
    path::{
        Path,
        PathBuf, //
    },
    process::{
        self,
        Child,
        ChildStdin,
        Command,
        ExitCode,
        Stdio, //
    },
    time::Duration, //
};

/// The environment variable that makes the program a relay, the watcher
/// under `watch:<out|in>:<endpoint file>` and the mover under
/// `<process id of its watcher>:<out|in>:<endpoint file>`.
pub const RELAY_ENV: &str = "AOSABI_USB_RELAY";

/// The role of the watcher in the variable of the relay.
const WATCH: &str = "watch";

/// The kind of a datagram that carries a transfer.
const KIND_DATA: u8 = 0;

/// The kind of a datagram that carries the end of a relay.
const KIND_END: u8 = 1;

/// How long the mover of the IN endpoint waits for a datagram before it
/// looks whether its watcher is gone.
const PARENT_CHECK: Duration = Duration::from_secs(1);

/// The direction of the endpoint a relay moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// The OUT endpoint, whose transfers the host sends.
    Out,
    /// The IN endpoint, whose transfers the host reads.
    In,
}

impl Direction {
    /// Returns the name of the direction in the variable of the relay.
    const fn name(self) -> &'static str {
        match self {
            Self::Out => "out",
            Self::In => "in",
        }
    }
}

/// What a relay handed the driver.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transfer {
    /// One transfer of the given length, copied into the caller's buffer.
    Data(usize),
    /// The relay ended, since its endpoint file failed.
    Ended,
}

impl Transfer {
    /// Returns a datagram of kind 0 that carries the bytes of `parts` in
    /// order as one transfer.
    #[must_use]
    pub fn datagram(parts: &[&[u8]]) -> Vec<u8> {
        let len = parts.iter().map(|part| part.len()).sum::<usize>();
        let mut datagram = Vec::with_capacity(1 + len);
        datagram.push(KIND_DATA);
        for part in parts {
            datagram.extend_from_slice(part);
        }
        datagram
    }
}

/// A relay of one endpoint file and the ends of its socket pair and of the
/// pipe of its watcher the driver keeps.
pub struct Relay {
    /// The driver's end of the pair, non blocking.
    socket: UnixDatagram,
    /// The process of the watcher.
    watcher: Child,
    /// The end of the pipe the watcher reads, whose close ends the relay.
    pipe: Option<ChildStdin>,
    /// The buffer one datagram lands in.
    rx: Vec<u8>,
}

impl Relay {
    // An `unsafe` AIO path would need no relay.
    /// Starts the relay of the endpoint file `file` in `direction`, whose
    /// transfers are at most `max_packet` bytes.
    ///
    /// # Errors
    ///
    /// Returns the error of the socket pair, of the path of this program or
    /// of the start of the process.
    pub fn start(direction: Direction, file: &Path, max_packet: u16) -> io::Result<Self> {
        let (socket, theirs) = UnixDatagram::pair()?;
        socket.set_nonblocking(true)?;
        let program = env::current_exe()?;
        let mut watcher = Command::new(program)
            .env(
                RELAY_ENV,
                format!("{WATCH}:{}:{}", direction.name(), file.display()),
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::from(OwnedFd::from(theirs)))
            .spawn()?;
        let pipe = watcher.stdin.take();
        Ok(Self {
            socket,
            watcher,
            pipe,
            rx: vec![0; 1 + usize::from(max_packet)],
        })
    }

    /// Sends one datagram to the relay and answers whether its socket took
    /// it, false where the socket is full.
    ///
    /// # Errors
    ///
    /// Returns the error of the socket, which a relay that ended answers.
    pub fn send(&self, datagram: &[u8]) -> io::Result<bool> {
        loop {
            match self.socket.send(datagram) {
                Ok(_) => return Ok(true),
                Err(error) if error.kind() == ErrorKind::WouldBlock => return Ok(false),
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                Err(error) => return Err(error),
            }
        }
    }

    /// Takes the next datagram of the relay, a transfer copied into `buf` or
    /// the end of the relay, or `None` where none waits.
    ///
    /// # Errors
    ///
    /// Returns the error of the socket.
    pub fn receive(&mut self, buf: &mut [u8]) -> io::Result<Option<Transfer>> {
        let len = loop {
            match self.socket.recv(&mut self.rx) {
                Ok(len) => break len,
                Err(error) if error.kind() == ErrorKind::WouldBlock => return Ok(None),
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                Err(error) => return Err(error),
            }
        };
        match self.rx.get(..len) {
            Some([KIND_DATA, data @ ..]) => {
                let take = data.len().min(buf.len());
                buf[..take].copy_from_slice(&data[..take]);
                Ok(Some(Transfer::Data(take)))
            }
            _ => Ok(Some(Transfer::Ended)),
        }
    }
}

impl AsFd for Relay {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.socket.as_fd()
    }
}

impl Drop for Relay {
    fn drop(&mut self) {
        // The end of the pipe ends the watcher, which ends the mover and
        // reaps it first, and the wait reaps the watcher.
        drop(self.pipe.take());
        let _ = self.watcher.wait();
    }
}

/// Runs this process as a relay where `AOSABI_USB_RELAY` names one, and
/// returns its exit code, or `None` where the variable is unset.
///
/// A binary that links the bulk driver calls it first in its `main`, before
/// it reads its command line.
#[must_use]
pub fn relay_from_env() -> Option<ExitCode> {
    let value = env::var_os(RELAY_ENV)?;
    let value = value.to_string_lossy();
    let (watcher, endpoint) = match value.split_once(':') {
        Some((WATCH, endpoint)) => (None, endpoint),
        Some((pid, endpoint)) => match pid.parse::<u32>() {
            Ok(pid) => (Some(pid), endpoint),
            Err(_) => return Some(ExitCode::from(64)),
        },
        None => return Some(ExitCode::from(64)),
    };
    let (direction, file) = match endpoint.split_once(':') {
        Some(("out", file)) => (Direction::Out, PathBuf::from(file)),
        Some(("in", file)) => (Direction::In, PathBuf::from(file)),
        _ => return Some(ExitCode::from(64)),
    };
    let ran = match watcher {
        None => watch(direction, &file),
        Some(watcher) => relay(direction, &file, watcher),
    };
    Some(match ran {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::from(74),
    })
}

/// Starts the mover of the endpoint file `file` in `direction` on the socket
/// of the standard output, with the process id of this process in its
/// variable, and ends it once the standard input, the pipe the driver holds
/// the other end of, reaches its end.
fn watch(direction: Direction, file: &Path) -> io::Result<()> {
    let socket = io::stdout().as_fd().try_clone_to_owned()?;
    let started = env::current_exe().and_then(|program| {
        Command::new(program)
            .env(
                RELAY_ENV,
                format!("{}:{}:{}", process::id(), direction.name(), file.display()),
            )
            .stdin(Stdio::from(socket.try_clone()?))
            .stdout(Stdio::from(socket.try_clone()?))
            .spawn()
    });
    let mut mover = match started {
        Ok(mover) => mover,
        Err(error) => {
            send_end(&UnixDatagram::from(socket), &error);
            return Err(error);
        }
    };
    // The driver writes nothing to the pipe, so a read returns at its end.
    let mut byte = [0u8; 1];
    let mut input = io::stdin().lock();
    loop {
        match input.read(&mut byte) {
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
    }
    // A mover that ended already answers the kill with an error, and the
    // wait reaps it either way.
    let _ = mover.kill();
    mover.wait().map(|_| ())
}

/// Moves the transfers of the endpoint file `file` in `direction`, as the
/// mover, until the file or the socket of the standard input fails or the
/// process `watcher`, which the variable names, is no longer its parent.
fn relay(direction: Direction, file: &Path, watcher: u32) -> io::Result<()> {
    let socket = UnixDatagram::from(io::stdin().as_fd().try_clone_to_owned()?);
    match direction {
        Direction::Out => relay_out(&socket, file, watcher),
        Direction::In => relay_in(&socket, file, watcher),
    }
}

/// Sends the end of the relay with the error that ended it.
fn send_end(socket: &UnixDatagram, error: &io::Error) {
    let mut datagram = vec![KIND_END];
    datagram.extend_from_slice(format!("{error}").as_bytes());
    // A driver that is gone takes no datagram, and the relay ends either way.
    let _ = socket.send(&datagram);
}

/// Reads one transfer a read from the OUT endpoint and sends each as a
/// datagram, and ends before the next read where the process `watcher` is
/// no longer its parent.
fn relay_out(socket: &UnixDatagram, file: &Path, watcher: u32) -> io::Result<()> {
    let mut endpoint = File::open(file)?;
    let mut buf =
        vec![0u8; 1 + usize::from(crate::profile::bulk_profile(crate::Speed::High).max_packet)];
    loop {
        if parent_id() != watcher {
            return Ok(());
        }
        let read = match endpoint.read(&mut buf[1..]) {
            Ok(read) => read,
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) => {
                send_end(socket, &error);
                return Err(error);
            }
        };
        buf[0] = KIND_DATA;
        socket.send(&buf[..=read])?;
    }
}

/// Writes each datagram as one transfer to the IN endpoint, and ends before
/// the next receive, which waits at most 1 s, where the process `watcher` is
/// no longer its parent.
fn relay_in(socket: &UnixDatagram, file: &Path, watcher: u32) -> io::Result<()> {
    let mut endpoint = OpenOptions::new().write(true).open(file)?;
    socket.set_read_timeout(Some(PARENT_CHECK))?;
    let mut buf =
        vec![0u8; 1 + usize::from(crate::profile::bulk_profile(crate::Speed::High).max_packet)];
    loop {
        if parent_id() != watcher {
            return Ok(());
        }
        let len = match socket.recv(&mut buf) {
            Ok(len) => len,
            Err(error)
                if matches!(
                    error.kind(),
                    ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted
                ) =>
            {
                continue;
            }
            Err(error) => return Err(error),
        };
        let Some([KIND_DATA, transfer @ ..]) = buf.get(..len) else {
            continue;
        };
        if let Err(error) = write_transfer(&mut endpoint, transfer) {
            send_end(socket, &error);
            return Err(error);
        }
    }
}

/// Writes `transfer` to the endpoint in one write, a write of no bytes being
/// a zero length packet.
fn write_transfer(endpoint: &mut File, transfer: &[u8]) -> io::Result<()> {
    loop {
        match endpoint.write(transfer) {
            Ok(_) => return Ok(()),
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
}
