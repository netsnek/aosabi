// SPDX-License-Identifier: MIT
/*
 * aosabi-core: the randomness capability, the random bytes a service asks its
 * context for.
 *
 * No analogue on the reference side, where the crypto global of the runtime is
 * the randomness. The capability is one of the five of Documentation/architecture.md section
 * 14.2.
 */

//! `Randomness`, `RandomError`, `NO_RANDOMNESS`, `install_randomness`,
//! `fill`, `seed_keys` and `DevUrandom`.
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
#[cfg(unix)]
use std::{
    fs::File,
    io::{
        self,
        Read, //
    },
};
use std::{
    rc::Rc,
    sync::OnceLock, //
};

use aosabi_http::{
    Body,
    Response,
    ResponseInit, //
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

/// The message of the one error an entry answers in place of a request on a
/// thread that [`seed_keys`] leaves without a key.
pub const NO_RANDOMNESS: &str = "no source of random bytes: the engine answers no request on this \
                                 target until the host installs a provider with install_randomness";

/// The status of [`refused`], the one the module of the TypeScript option
/// answers a request with before its boot.
const REFUSED: u16 = 503;

/// Returns the answer of the fetch of a booted app, of the middleware of
/// `app::handler` and of the fetch of `graphtp::served` on a thread that
/// [`seed_keys`] leaves without a key: the status 503, no header field and the
/// text of [`NO_RANDOMNESS`], in the manner of the answer of the module of the
/// TypeScript option before its boot.
pub(crate) fn refused() -> Response {
    Response::new(
        Body::Full(Rc::from(NO_RANDOMNESS.as_bytes())),
        Some(ResponseInit::status(REFUSED)),
    )
    .unwrap_or_else(|_| Response::empty(REFUSED))
}

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

/// Keys the keyed hashes of the calling thread, once per thread.
///
/// A map of `aosabi-json` whose builder is `aosabi_json::value::KeyState`
/// hashes its keys under the key that `aosabi_json::value::seed` stored for the
/// thread that makes it. On `wasm32-unknown-unknown` and the operating systems
/// `xous` and `vexos` that is every map, and elsewhere it is the tables that
/// name `KeyState`, the cache of parsed documents among them, since the
/// `RandomState` of the standard library keys the others. The maps a boot makes
/// before the first request keep the key the thread held at the boot, which is
/// harmless, since their keys come from the schema and not from a client. On a
/// thread that holds a key this is one read of a thread local, and on the first
/// call of any other thread it stores sixteen bytes of the installed provider.
///
/// With no provider installed, a target whose standard library keys its
/// `RandomState` from the operating system takes two words that a `RandomState`
/// hashes. The three targets named above store nothing then, since their
/// standard library keys a `RandomState` from the addresses of two values in
/// its memory, and a key once stored stays the key of the thread. A host on one
/// of them installs a provider before its first request, which on the
/// WebAssembly of a page or of a Worker reads `crypto.getRandomValues`.
///
/// The entries the doc of this module names call this before they read a
/// request, so a thread that reaches the engine through one of them holds a
/// key before it parses a body.
/// Until a provider is installed and fills, the call answers [`RandomError`],
/// the entries the doc of this module names refuse the request as it says,
/// and the next call tries again.
///
/// A build without the feature `std` has no `seed`, so its maps keep the
/// fixed key, which the doc of `seed` weighs, and the function below keys
/// nothing there.
///
/// # Errors
///
/// Returns [`RandomError`] when the thread holds no key after the call,
/// which happens on the three targets named above alone, while no provider
/// is installed or the installed one cannot fill.
#[cfg(feature = "std")]
pub fn seed_keys() -> Result<(), RandomError> {
    if aosabi_json::value::KeyState::seeded().is_some() {
        return Ok(());
    }
    let mut key = [0; 16];
    if fill(&mut key).is_err() && !system_key(&mut key) {
        return Err(RandomError);
    }
    // A key stored on this thread since the check above stays the key.
    let _ = aosabi_json::value::seed(key);
    Ok(())
}

/// Keys nothing and refuses nothing, since a build without the feature `std`
/// has no `seed`.
///
/// # Errors
///
/// None, the signature is the one of the build with the feature `std`.
#[cfg(not(feature = "std"))]
pub const fn seed_keys() -> Result<(), RandomError> {
    Ok(())
}

/// Fills `key` with two words that a `RandomState` of the standard library
/// hashes under the key it draws from the operating system.
///
/// The targets left out are those of `aosabi_json::value::KeyHasherBuilder`
/// whose standard library has no such source.
#[cfg(all(
    feature = "std",
    not(any(
        all(target_family = "wasm", target_os = "unknown"),
        target_os = "xous",
        target_os = "vexos",
    ))
))]
fn system_key(key: &mut [u8; 16]) -> bool {
    use std::hash::{
        BuildHasher as _,
        RandomState, //
    };

    let state = RandomState::new();
    let (low, high) = key.split_at_mut(8);
    low.copy_from_slice(&state.hash_one(0_u8).to_le_bytes());
    high.copy_from_slice(&state.hash_one(1_u8).to_le_bytes());
    true
}

/// Answers that the target has no source of its own to key a thread with.
#[cfg(all(
    feature = "std",
    any(
        all(target_family = "wasm", target_os = "unknown"),
        target_os = "xous",
        target_os = "vexos",
    )
))]
const fn system_key(_: &mut [u8; 16]) -> bool {
    false
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
