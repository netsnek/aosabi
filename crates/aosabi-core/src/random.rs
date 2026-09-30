// SPDX-License-Identifier: MIT
/*
 * aosabi-core: the randomness capability, the random bytes a service asks its
 * context for.
 *
 * No analogue on the reference side, where the crypto global of the runtime is
 * the randomness. The capability is one of the five of Documentation/architecture.md section
 * 14.2.
 */

//! `Randomness`, `RandomError`, `install_randomness`, `fill` and
//! `DevUrandom`.
//!
//! A service asks its context for a capability rather than assuming one, so the random
//! bytes of a request id or of the event id of an error come from the provider the
//! environment installed, once per process, in a [`OnceLock`]. With no provider
//! [`fill`] answers [`RandomError`] and never falls back to a weaker source. A Unix
//! host provides `DevUrandom`, which the boot installs before its first pass, and a
//! Worker build has no provider until the glue of the Worker installs the `crypto` of
//! its host. Node always has `crypto`, so the reference never takes its own fallback to
//! `Math.random`.
//!
//! `DevUrandom` opens the device at its first fill and keeps it open for the process,
//! so a fill is one read and no open and close, which every request that mints its
//! request id pays.

use core::fmt;
use std::sync::OnceLock;
#[cfg(unix)]
use std::{
    fs::File,
    io::{
        self,
        Read, //
    },
};

/// The randomness an environment provides.
///
/// The provider is held by the process and may be read from any thread of
/// it, so it is `Send` and `Sync`.
pub trait Randomness: Send + Sync + 'static {
    /// Fills `buf` with random bytes.
    ///
    /// # Errors
    ///
    /// Returns [`RandomError`] when the source cannot be read.
    fn fill(&self, buf: &mut [u8]) -> Result<(), RandomError>;
}

/// No random bytes, because no provider is installed or the source failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RandomError;

impl fmt::Display for RandomError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("no source of random bytes")
    }
}

impl core::error::Error for RandomError {}

/// The provider of the process.
static PROVIDER: OnceLock<Box<dyn Randomness>> = OnceLock::new();

/// Installs `provider` as the randomness of the process.
///
/// The first call wins and a later call is ignored, so a provider the serving
/// layer installs before the boot stays when the boot installs its default.
pub fn install_randomness(provider: Box<dyn Randomness>) {
    // A provider that is set already stays, which is the rule above.
    let _ = PROVIDER.set(provider);
}

/// Fills `buf` from the installed provider.
///
/// # Errors
///
/// Returns [`RandomError`] when no provider is installed or the provider
/// cannot read its source.
pub fn fill(buf: &mut [u8]) -> Result<(), RandomError> {
    PROVIDER.get().ok_or(RandomError)?.fill(buf)
}

/// The provider of a Unix host, `/dev/urandom` read through [`File`].
///
/// The device never blocks once the kernel has seeded it. The first fill
/// opens it and the process keeps it in `DEVICE`, which every later fill
/// reads through `&File`, the reader the standard library gives a shared
/// file, so a fill takes no lock and the process holds one descriptor of the
/// device. An open that fails is not kept, so the next fill opens the device
/// again, and an open or a read that fails answers [`RandomError`].
#[cfg(unix)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DevUrandom;

/// The device `DevUrandom` reads, opened at its first fill.
#[cfg(unix)]
static DEVICE: OnceLock<File> = OnceLock::new();

/// Returns the open device, opening it when no fill has opened it yet.
///
/// Two threads that open it at once both open it, the first to finish keeps
/// its file, and the file of the other closes.
#[cfg(unix)]
fn device() -> io::Result<&'static File> {
    if let Some(file) = DEVICE.get() {
        return Ok(file);
    }
    let file = File::open("/dev/urandom")?;
    Ok(DEVICE.get_or_init(|| file))
}

#[cfg(unix)]
impl Randomness for DevUrandom {
    // The `getrandom` system call in `unsafe` code would fill faster than the read of
    // the held device.
    fn fill(&self, buf: &mut [u8]) -> Result<(), RandomError> {
        device()
            .and_then(|mut source| source.read_exact(buf))
            .map_err(|_| RandomError)
    }
}
