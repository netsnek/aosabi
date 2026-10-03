// SPDX-License-Identifier: MIT
/*
 * aosabi-carrier-usb linux acm: the serial function of the gadget, the tty
 * of the ACM function the card's setup composes, a byte stream in delimited
 * AoFrame.
 *
 * No analogue on the reference side. The tty is /dev/ttyGS<n> of the function
 * acm of the gadget's configfs, which the Homa design note, first version,
 * homa/design-v1.md section 3.3 in the design folder outside this repository,
 * carries in delimited AoFrame.
 */

//! The serial function of the gadget.
//!
//! A [`GadgetTty`] is the tty of the ACM function opened non blocking and
//! without making it the controlling terminal of the process, whose raw mode
//! the card's setup sets with `stty`. A host that detaches or unbinds the
//! gadget hangs the tty up, which a read answers with `EIO` or the end of the
//! stream, the link going down.

use std::{
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
    path::Path, //
};

use homa::stream::ByteStream;

/// The Linux value of `O_NONBLOCK`.
const O_NONBLOCK: i32 = 1 << 11;

/// The Linux value of `O_NOCTTY`.
const O_NOCTTY: i32 = 1 << 8;

/// The tty of the ACM function of the gadget, `/dev/ttyGS<n>`, a byte
/// stream with delimited `AoFrame`.
#[derive(Debug)]
pub struct GadgetTty {
    /// The file of the tty.
    file: File,
}

impl GadgetTty {
    /// Opens `path` for reading and writing with `O_NONBLOCK` and `O_NOCTTY`.
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
        Ok(Self { file })
    }
}

impl ByteStream for GadgetTty {
    type Error = io::Error;

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

impl AsFd for GadgetTty {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.file.as_fd()
    }
}
