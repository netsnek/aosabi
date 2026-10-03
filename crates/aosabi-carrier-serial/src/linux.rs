// SPDX-License-Identifier: MIT
/*
 * aosabi-carrier-serial linux: the driver of Serial Line in Linux user space,
 * a tty in delimited AoFrame served by the GraphTP listener as a local task of
 * the runtime of aosabi-serve.
 *
 * No analogue on the reference side.
 */

//! The Linux driver of Serial Line.
//!
//! A [`Tty`] is the file of a terminal opened non blocking and without making
//! it the controlling terminal of the process, whose raw mode and rate the
//! card's setup sets with `stty`, since setting them in process needs
//! `tcsetattr`. [`serve`] runs one endpoint of the server role over the tty in
//! delimited `AoFrame`, with the listener in front of it, as a local task of
//! the runtime: the tty sits in the runtime's reactor, the endpoint's
//! deadlines on the runtime's timer, and the task ends when the file fails. A
//! serial line reports no event of its link, so the link is up from the start
//! and stays up, and a client that restarts sends a CANCEL of handle 0 before
//! its first OPERATION.
//!
//! It reads its variable when the boot calls it, and its [`SerialListen`]
//! builds a listener over the fetch of the booted app it is handed, with the
//! bounds of the card `player`, the Linux device card these drivers serve.

mod app;
mod task;

use alloc::{
    boxed::Box,
    rc::Rc,
    vec, //
};
use core::{
    cell::Cell,
    future::poll_fn,
    task::{
        Context,
        Poll, //
    }, //
};
use std::{
    env,
    eprintln,
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
            BorrowedFd, //
        },
        unix::fs::OpenOptionsExt as _, //
    },
    path::{
        Path,
        PathBuf, //
    }, //
};

use aosabi_core::{
    BootedApp,
    Listen, //
};
use aosabi_serve::{
    BuildOutput,
    runtime::{
        Interest,
        LocalBoxFuture, //
    }, //
};
use graphql_over_graphtp::{
    Listener,
    limits::{
        self,
        CardLimits, //
    }, //
};
use homa::{
    Config,
    Stats,
    stream::{
        ByteStream,
        Decoder,
        Framed,
        Mode,
        frame_len, //
    }, //
};

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
use crate::profile::{
    MAX_RPCS,
    limits as line_limits,
    profile, //
};

/// The environment variable that starts the driver, `<tty>:<baud>`.
pub const SERIAL_ENV: &str = "AOSABI_SERIAL";

/// The Linux value of `O_NONBLOCK`.
const O_NONBLOCK: i32 = 1 << 11;

/// The Linux value of `O_NOCTTY`, an open that makes the terminal no
/// controlling terminal.
const O_NOCTTY: i32 = 1 << 8;

/// The bytes of one page of the endpoint, so a slot of 16 pages holds the
/// 131,072 bytes of the player's answer buffer.
const PAGE_LEN: u32 = 8_192;

/// The pages one slot of the endpoint takes at most, the bound of the
/// engine.
const PAGES_PER_SLOT: u32 = 16;

// The constants above are those of Linux, and another system spells the same
// flags otherwise, so the module refuses to build anywhere else rather than
// pass a flag it does not mean.
#[cfg(not(target_os = "linux"))]
compile_error!("the feature linux of aosabi-carrier-serial builds on Linux alone");

/// A terminal opened non blocking, whose raw mode and rate the card's setup
/// sets with `stty`.
#[derive(Debug)]
pub struct Tty {
    /// The file of the terminal.
    file: File,
    /// The path it was opened at.
    path: PathBuf,
}

impl Tty {
    /// Opens `path` for reading and writing with `O_NONBLOCK` and `O_NOCTTY`
    /// through `OpenOptionsExt::custom_flags`.
    ///
    /// # Errors
    ///
    /// Returns the error of the open.
    pub fn open(path: &Path) -> io::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(O_NONBLOCK | O_NOCTTY)
            .open(path)?;
        Ok(Self {
            file,
            path: path.to_path_buf(),
        })
    }

    /// Returns the path the terminal was opened at.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl ByteStream for Tty {
    type Error = io::Error;

    /// Reads what the terminal holds, `None` where it would block. A
    /// terminal whose other end hung up answers `EIO`, which fails the
    /// stream.
    fn read(&mut self, buf: &mut [u8]) -> io::Result<Option<usize>> {
        loop {
            match self.file.read(buf) {
                Ok(read) => return Ok(Some(read)),
                Err(error) if error.kind() == ErrorKind::WouldBlock => return Ok(None),
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                Err(error) => return Err(error),
            }
        }
    }

    fn write(&mut self, bytes: &[u8]) -> io::Result<Option<usize>> {
        loop {
            match self.file.write(bytes) {
                Ok(written) => return Ok(Some(written)),
                Err(error) if error.kind() == ErrorKind::WouldBlock => return Ok(None),
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                Err(error) => return Err(error),
            }
        }
    }
}

impl AsFd for Tty {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.file.as_fd()
    }
}

/// A stream in the reactor that reports each byte it moves and counts them.
struct Counted<'c, S: ByteStream + AsFd> {
    /// The stream and its place in the reactor.
    stream: Registered<S>,
    /// The counts the harness reads.
    counts: &'c Counts,
}

impl<S: ByteStream + AsFd> ByteStream for Counted<'_, S> {
    type Error = S::Error;

    fn read(&mut self, buf: &mut [u8]) -> Result<Option<usize>, S::Error> {
        let read = self.stream.get_mut().read(buf)?;
        if let Some(read) = read.filter(|&read| read > 0) {
            self.stream.progressed();
            self.counts
                .bytes_in
                .set(self.counts.bytes_in.get() + read as u64);
        }
        Ok(read)
    }

    fn write(&mut self, bytes: &[u8]) -> Result<Option<usize>, S::Error> {
        let written = self.stream.get_mut().write(bytes)?;
        if let Some(written) = written.filter(|&written| written > 0) {
            self.stream.progressed();
            self.counts
                .bytes_out
                .set(self.counts.bytes_out.get() + written as u64);
        }
        Ok(written)
    }
}

/// What a served line counted, which a harness reads while and after the
/// task runs.
#[derive(Debug, Default)]
pub struct Counts {
    /// The counters of the endpoint after the last turn.
    pub stats: Cell<Stats>,
    /// The frames the framing dropped in either direction, damaged, cut or
    /// too long.
    pub dropped: Cell<u64>,
    /// The bytes read from the line.
    pub bytes_in: Cell<u64>,
    /// The bytes written to the line.
    pub bytes_out: Cell<u64>,
    /// The answers the listener handed the endpoint.
    pub answered: Cell<u64>,
}

/// Returns the configuration of an endpoint that serves one line of a card
/// with the bounds `card`, and the longest message it accepts.
///
/// The slots are the RPCs a peer may hold open on the line, and a slot takes
/// at most 16 pages of 8,192 bytes, so a message holds the longest request
/// of the card and at most 131,072 bytes.
#[must_use]
pub fn line_config(card: &CardLimits, seed: Seed) -> (Config, u32) {
    let slots = u32::from(MAX_RPCS);
    let longest = PAGE_LEN * PAGES_PER_SLOT;
    let request = card
        .request_buffer
        .saturating_add(usize::from(card.max_credential))
        .saturating_add(3);
    let max_message = u32::try_from(request).unwrap_or(u32::MAX).min(longest);
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

/// Serves `GraphTP`(Homa) on one terminal at `baud` until the file fails, as
/// a local task of the runtime over `Endpoint::pump` and `listener`, the
/// terminal in the reactor and delimited `AoFrame` in front.
#[must_use]
pub fn serve(
    tty: Tty,
    baud: u32,
    listener: Listener<()>,
) -> LocalBoxFuture<'static, io::Result<()>> {
    serve_counted(tty, baud, listener, Rc::new(Counts::default()))
}

/// Serves as [`serve`] does and keeps what the line counted in `counts`.
#[must_use]
pub fn serve_counted(
    tty: Tty,
    baud: u32,
    listener: Listener<()>,
    counts: Rc<Counts>,
) -> LocalBoxFuture<'static, io::Result<()>> {
    Box::pin(async move {
        let card = listener.limits();
        let (config, max_message) = line_config(card, seed()?);
        let line = profile(baud);
        let local = line_limits(baud, max_message);
        let mut task = Task::new(config, line, listener)?;
        task.link_up((), local, local).map_err(TaskError::Link)?;
        let largest = usize::from(line.max_packet);
        let mut rx = vec![0u8; Decoder::buffer_len(Mode::Delimited, largest)];
        let mut tx = vec![0u8; 4 * frame_len(Mode::Delimited, largest)];
        let stream = Counted {
            stream: Registered::new(tty, task.handle())?,
            counts: &counts,
        };
        let mut framed = Framed::new(stream, Mode::Delimited, &mut rx, &mut tx);
        poll_fn(|cx| step(&mut task, &mut framed, &counts, cx)).await
    })
}

/// Runs the turns of one wake of the task and registers its interests.
fn step<S: ByteStream<Error = io::Error> + AsFd>(
    task: &mut Task<()>,
    framed: &mut Framed<'_, Counted<'_, S>>,
    counts: &Counts,
    cx: &mut Context<'_>,
) -> Poll<io::Result<()>> {
    let ran = task.run(framed, cx);
    counts.stats.set(task.stats());
    counts.dropped.set(framed.dropped());
    counts.answered.set(task.answered());
    if let Err(error) = ran {
        task.link_down((), cx);
        return Poll::Ready(Err(error));
    }
    if framed.closed() {
        task.link_down((), cx);
        return Poll::Ready(Ok(()));
    }
    let drained = framed.flush().unwrap_or(true);
    let line = &framed.stream().stream;
    line.register(Interest::Read, cx);
    if !drained {
        line.register(Interest::Write, cx);
    }
    Poll::Pending
}

/// The listener of Serial Line the boot runs over the booted app.
#[derive(Debug)]
pub struct SerialListen {
    /// The path of the terminal.
    path: PathBuf,
    /// Its rate in baud.
    baud: u32,
}

/// Reads `<tty>:<baud>`, the path and the rate of a line, whose rate is the
/// text after the last colon.
fn line_of(value: &str) -> io::Result<(PathBuf, u32)> {
    let invalid = || {
        io::Error::new(
            ErrorKind::InvalidInput,
            alloc::format!("{SERIAL_ENV} names no <tty>:<baud>: {value}"),
        )
    };
    let (path, baud) = value.rsplit_once(':').ok_or_else(invalid)?;
    let baud = baud.parse::<u32>().map_err(|_| invalid())?;
    if path.is_empty() || baud == 0 {
        return Err(invalid());
    }
    Ok((PathBuf::from(path), baud))
}

/// It captures nothing and reads its variable when the boot calls it, and
/// the listener it answers builds its `GraphTP` listener over the fetch of the
/// booted app it is handed.
///
/// # Errors
///
/// Returns an error of the kind `InvalidInput` for a value that names no
/// line.
pub fn driver(_build: &BuildOutput) -> io::Result<Option<Box<dyn Listen>>> {
    let Some(value) = env::var_os(SERIAL_ENV) else {
        return Ok(None);
    };
    let value = value.to_string_lossy();
    let (path, baud) = line_of(&value)?;
    Ok(Some(Box::new(SerialListen { path, baud })))
}

impl Listen for SerialListen {
    fn run(self: Box<Self>, app: Rc<BootedApp>) -> LocalBoxFuture<'static, ()> {
        Box::pin(async move {
            let listener = match Listener::new(app_fetch(app), &limits::PLAYER) {
                Ok(listener) => listener,
                Err(error) => {
                    eprintln!("aosabi-carrier-serial: the listener: {error}");
                    return;
                }
            };
            let served = match Tty::open(&self.path) {
                Ok(tty) => serve(tty, self.baud, listener).await,
                Err(error) => Err(error),
            };
            if let Err(error) = served {
                eprintln!("aosabi-carrier-serial: {}: {error}", self.path.display());
            }
        })
    }
}

/// Returns the bytes of the frame buffers a served line holds for packets of
/// `max_packet` bytes, the receive buffer of one frame and the transmit
/// buffer of four.
#[must_use]
pub fn frame_buffers(max_packet: u16) -> usize {
    let largest = usize::from(max_packet);
    Decoder::buffer_len(Mode::Delimited, largest) + 4 * frame_len(Mode::Delimited, largest)
}
