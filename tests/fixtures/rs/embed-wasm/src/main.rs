// SPDX-License-Identifier: MIT
/*
 * embed-wasm: the embedding host on a target with no source of random bytes.
 */

//! The embedding host of `80_embed` on `wasm32-unknown-unknown`, a target
//! whose standard library has no source of random bytes.
//!
//! The program boots the app of `tests/fixtures/rs/embed-app` through
//! [`Embedded::boot`] and hands it request bodies before its host installs a
//! provider of random bytes and after it installs one with
//! [`install_randomness`], the provider of the process. The module imports
//! nothing, so the provider here hands out the count of the bytes it gave,
//! which keys the thread as any provider does. The target writes no standard
//! output, so the status `main` returns is the verdict: each check that fails
//! sets its bit, a boot that fails answers [`BOOT`] alone, and 0 means every
//! check holds.

use core::sync::atomic::{
    AtomicUsize,
    Ordering, //
};
use std::process::ExitCode;

use aosabi::embed::{
    Embedded,
    RandomError,
    Randomness,
    install_randomness, //
};
use aosabi_json::value::KeyState;

/// The body of a request for the type name of the root.
const TYPENAME: &str = r#"{"query":"{ __typename }"}"#;

/// The answer to [`TYPENAME`].
const ANSWER: &str = r#"{"data":{"__typename":"Query"}}"#;

/// The answer to every body while no provider is installed.
const REFUSED: &str = concat!(
    r#"{"errors":[{"message":"no source of random bytes: the engine answers "#,
    r#"no request on this target until the host installs a provider with "#,
    r#"install_randomness"}]}"#,
);

/// A request before the install answers [`REFUSED`].
const REQUEST_REFUSED: u8 = 1;

/// A body that is no JSON answers [`REFUSED`] before the install, so the
/// entry refuses before it reads a body.
const BODY_UNREAD: u8 = 2;

/// The thread holds no key before the install.
const UNKEYED_BEFORE: u8 = 4;

/// The request answers [`ANSWER`] after the install.
const ANSWERED_AFTER: u8 = 8;

/// The thread holds a key after the install.
const KEYED_AFTER: u8 = 16;

/// The provider handed out sixteen bytes, the one key of the thread.
const SIXTEEN_BYTES: u8 = 32;

/// The status of a boot that fails.
const BOOT: u8 = 128;

/// The count of the bytes the provider handed out.
static DRAWN: AtomicUsize = AtomicUsize::new(0);

/// The provider of the program, whose bytes are the low bytes of the count
/// of the bytes it handed out before each.
struct Counting;

impl Randomness for Counting {
    fn fill(&self, buf: &mut [u8]) -> Result<(), RandomError> {
        let start = DRAWN.fetch_add(buf.len(), Ordering::Relaxed);
        for (offset, byte) in buf.iter_mut().enumerate() {
            *byte = (start + offset).to_le_bytes()[0];
        }
        Ok(())
    }
}

fn main() -> ExitCode {
    let Ok(engine) = Embedded::boot(embed_app::__aosabi::entry, aosabi::embed_output!()) else {
        return ExitCode::from(BOOT);
    };
    let before = [
        (REQUEST_REFUSED, engine.answer(TYPENAME) == REFUSED),
        (BODY_UNREAD, engine.answer("{ broken") == REFUSED),
        (UNKEYED_BEFORE, KeyState::seeded().is_none()),
    ];
    install_randomness(Box::new(Counting));
    let after = [
        (ANSWERED_AFTER, engine.answer(TYPENAME) == ANSWER),
        (KEYED_AFTER, KeyState::seeded().is_some()),
        (SIXTEEN_BYTES, DRAWN.load(Ordering::Relaxed) == 16),
    ];
    let failed = before
        .into_iter()
        .chain(after)
        .filter(|&(_, holds)| !holds)
        .fold(0, |status, (bit, _)| status | bit);
    ExitCode::from(failed)
}
