// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool homa_sim pty: the link pty of a homa-sim scenario, both ends
 * in this process over the two ends of a pseudo terminal pair.
 *
 * No analogue in the reference. The pair is two pseudo terminals whose masters
 * tests/harness/homa/pty.py joins with a relay, so the devtool opens the path
 * of each slave through std::fs::File, calls no openpty and holds no unsafe.
 * The judging notebook is tests/notebooks/22_homa_lossy.ipynb, whose socket
 * half runs it.
 */

//! The pty link of the `homa-sim` primitive.
//!
//! The link `pty` opens the slave of each pseudo terminal the scenario names,
//! `pty_a` for end A and `pty_b` for end B, non blocking and without making
//! it the controlling terminal, and runs each in delimited `AoFrame` through
//! `homa::stream::Framed`, as Serial Line runs. `pty.py` puts both slaves in
//! raw mode and moves the bytes between the masters.
//!
//! The relay of `pty.py` is a thread of another process, so a byte takes a
//! while on the link, and the link keeps the clock of the run in step with
//! the wall clock: every call of `step` that finds the clock moved sleeps as
//! long as the clock moved, up to one tick, and names the next tick as the
//! link's due time, so the run never finds itself without a due time while
//! bytes cross the relay.

use std::{
    fs::{
        File,
        OpenOptions, //
    },
    io,
    os::unix::fs::OpenOptionsExt as _,
    path::PathBuf,
    time::Duration, //
};

use homa::{
    Nanos,
    Profile,
    sim::{
        Injected,
        Side, //
    },
    stream::{
        self,
        ByteStream,
        Decoder,
        Framed,
        Mode, //
    }, //
};

use super::{
    scenario::Net,
    socket::SideEnd, //
};

/// The Linux value of `O_NONBLOCK`.
const O_NONBLOCK: i32 = 1 << 11;

/// The Linux value of `O_NOCTTY`, an open that makes the terminal no
/// controlling terminal.
const O_NOCTTY: i32 = 1 << 8;

/// The longest the link lets the clock of the run move ahead of the wall
/// clock at once, and the due time it names after now.
const TICK: Nanos = 200_000;

/// The slave of a pseudo terminal, read and written without blocking.
pub struct Tty(File);

impl ByteStream for Tty {
    type Error = io::Error;

    fn read(&mut self, buf: &mut [u8]) -> io::Result<Option<usize>> {
        loop {
            match io::Read::read(&mut self.0, buf) {
                Ok(read) => return Ok(Some(read)),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(None),
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => return Err(error),
            }
        }
    }

    fn write(&mut self, bytes: &[u8]) -> io::Result<Option<usize>> {
        loop {
            match io::Write::write(&mut self.0, bytes) {
                Ok(written) => return Ok(Some(written)),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(None),
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => return Err(error),
            }
        }
    }
}

/// Opens the slave of a pseudo terminal at `path`, non blocking and never as
/// the controlling terminal.
fn open_tty(path: &PathBuf) -> io::Result<Tty> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(O_NONBLOCK | O_NOCTTY)
        .open(path)
        .map(Tty)
}

/// The buffers of the two ends of the link `pty`: for each end one frame
/// received and two frames queued for the terminal.
pub struct PtyBuffers {
    /// The receive and the transmit buffer of A, then of B.
    parts: [Vec<u8>; 4],
}

/// The link `pty`: the slaves of the two pseudo terminals of `pty.py`, in
/// delimited `AoFrame`.
pub struct PtyNet<'b> {
    /// End A.
    a: Framed<'b, Tty>,
    /// End B.
    b: Framed<'b, Tty>,
    /// The time of the run at the last call of `step`.
    last: Nanos,
}

impl<'b> PtyNet<'b> {
    /// Returns the buffers the link needs for the packet of `profile`.
    pub fn buffers(profile: &Profile) -> PtyBuffers {
        let max_packet = usize::from(profile.max_packet);
        let rx = Decoder::buffer_len(Mode::Delimited, max_packet);
        let tx = stream::frame_len(Mode::Delimited, max_packet).saturating_mul(2);
        PtyBuffers {
            parts: [vec![0; rx], vec![0; tx], vec![0; rx], vec![0; tx]],
        }
    }

    /// Opens the slave at `paths[0]` for A and at `paths[1]` for B, framed
    /// over `buffers`.
    pub fn open(paths: &[PathBuf; 2], buffers: &'b mut PtyBuffers) -> io::Result<Self> {
        let a = open_tty(&paths[0])?;
        let b = open_tty(&paths[1])?;
        let [rx_a, tx_a, rx_b, tx_b] = &mut buffers.parts;
        Ok(Self {
            a: Framed::new(a, Mode::Delimited, rx_a, tx_a),
            b: Framed::new(b, Mode::Delimited, rx_b, tx_b),
            last: 0,
        })
    }
}

impl<'b> Net for PtyNet<'b> {
    type End<'a>
        = SideEnd<'a, Framed<'b, Tty>>
    where
        Self: 'a;

    fn end(&mut self, side: Side) -> SideEnd<'_, Framed<'b, Tty>> {
        match side {
            Side::A => SideEnd::new(&mut self.a, Side::B),
            Side::B => SideEnd::new(&mut self.b, Side::A),
        }
    }

    fn step(&mut self, now: Nanos) -> Option<Nanos> {
        if now > self.last {
            std::thread::sleep(Duration::from_nanos((now - self.last).min(TICK)));
            self.last = now;
        }
        Some(now.saturating_add(TICK))
    }

    fn injected(&self) -> Option<Injected> {
        None
    }
}
