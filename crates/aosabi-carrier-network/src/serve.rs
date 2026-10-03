// SPDX-License-Identifier: MIT
/*
 * aosabi-carrier-network serve: the driver of Network in the shape of the
 * drivers of serve_with, which starts GraphTP(Homa) beside the HTTP(TCP)
 * listener of the same booted app.
 *
 * No analogue on the reference side.
 */

//! The driver of Network.
//!
//! [`driver`] is the driver `serve_with` starts, `Some` where
//! [`HomaNetwork::from_env`] names a path. It reads its options when the boot
//! calls it, so a card that names no protection fails the boot rather than
//! serve Homa unprotected, and its [`HomaListen`] builds a `GraphTP` listener
//! over the fetch of the booted app it is handed, with the bounds of the card
//! `server`, so one server answers both protocols through one pipeline.
//!
//! A path the card marks optional that cannot open leaves the service on
//! HTTP(TCP) alone, and a Homa port another process of the host holds prints
//! the port and ends the process with 70, `EX_SOFTWARE` of `sysexits.h`,
//! since a second service on one port would answer the requests of the first.
//! A path the card marks required that cannot open or that fails prints why
//! and ends the process with 69, `EX_UNAVAILABLE`, since the card asked for
//! no service without it.

mod app;

use alloc::boxed::Box;
use std::{
    eprintln,
    io,
    process,
    rc::Rc, //
};

use aosabi_core::{
    BootedApp,
    Listen, //
};
use aosabi_serve::{
    BuildOutput,
    runtime::LocalBoxFuture, //
};
use graphql_over_graphtp::{
    Listener,
    limits, //
};

pub use self::app::app_fetch;
use crate::{
    homa_ip::serve_ip,
    network::{
        HomaNetwork,
        NetworkError,
        Path,
        Requirement, //
    }, //
};

/// The exit of a process whose Homa port another process holds, `EX_SOFTWARE`
/// of `sysexits.h`, the exit the plan gives `PortTaken`.
pub const EXIT_PORT_TAKEN: i32 = 70;

/// The exit of a process whose required path cannot open or failed,
/// `EX_UNAVAILABLE` of `sysexits.h`.
pub const EXIT_REQUIRED: i32 = 69;

/// The driver `serve_with` starts beside the HTTP(TCP) listener, `Some` when
/// [`HomaNetwork::from_env`] names a path.
///
/// It captures nothing, reads its options when it runs, and builds its
/// `Listener` over the fetch of the booted app it is handed, so one server
/// answers both protocols.
///
/// # Errors
///
/// Returns the refusal of [`HomaNetwork::from_env`] as an error of the kind
/// `InvalidInput`, a path named without a protection among it, which fails
/// the boot.
pub fn driver(_build: &BuildOutput) -> io::Result<Option<Box<dyn Listen>>> {
    match HomaNetwork::from_env() {
        Ok(Some(options)) => Ok(Some(Box::new(HomaListen { options }))),
        Ok(None) => Ok(None),
        Err(NetworkError::Io(error)) => Err(error),
        Err(error) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            alloc::format!("aosabi-carrier-network: {error}"),
        )),
    }
}

/// The listener of Network the boot runs over the booted app.
#[derive(Debug)]
pub struct HomaListen {
    /// What the card names for `GraphTP`(Homa) on Network.
    options: HomaNetwork,
}

impl HomaListen {
    /// Returns the listener of `options`, which a host that names its path
    /// in its own code runs as the driver's would run.
    #[must_use]
    pub fn new(options: HomaNetwork) -> Self {
        Self { options }
    }
}

impl Listen for HomaListen {
    fn run(self: Box<Self>, app: Rc<BootedApp>) -> LocalBoxFuture<'static, ()> {
        Box::pin(async move {
            let listener = match Listener::new(app_fetch(app), &limits::SERVER) {
                Ok(listener) => listener,
                Err(error) => {
                    eprintln!("aosabi-carrier-network: the listener: {error}");
                    return;
                }
            };
            let options = self.options;
            let Path::Ip(path) = &options.path;
            let port = path.port;
            let required = options.requirement == Requirement::Required;
            match serve_ip(options, listener).await {
                Ok(()) => {}
                Err(NetworkError::PortTaken(taken)) => {
                    eprintln!(
                        "aosabi-carrier-network: PortTaken: another process holds the Homa port {taken}"
                    );
                    process::exit(EXIT_PORT_TAKEN);
                }
                Err(error) => {
                    eprintln!("aosabi-carrier-network: the Homa port {port}: {error}");
                    if required {
                        process::exit(EXIT_REQUIRED);
                    }
                }
            }
        })
    }
}
