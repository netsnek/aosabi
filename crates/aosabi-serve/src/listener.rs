// SPDX-License-Identifier: MIT
/*
 * aosabi-serve: the TCP listener of a served binary.
 *
 * Derived from hono 1.19.9, dist/server.js, MIT.
 */

//! `TcpListen`.
//!
//! The function `serve` of the adapter creates Node's server over the request
//! listener and listens on the port with no host, which Node binds on every
//! address, and with no connection cap. The listener here binds the port on
//! every address, IPv6 with IPv4 beside it and IPv4 alone where the host has
//! no IPv6, and runs every accepted socket as a local task through
//! `serve_accepted`, which selects the protocol.

use core::{
    cell::Cell,
    time::Duration, //
};
use std::{
    io,
    net::{
        Ipv4Addr,
        Ipv6Addr,
        SocketAddr, //
    },
    rc::Rc,
};

use crate::{
    protocols::{
        TlsConfig,
        serve_accepted, //
    },
    runtime::{
        LocalBoxFuture,
        hyper_rt::RuntimeTimer,
        net::TcpListener,
        spawn_local, //
    },
};
use aosabi_core::{
    BootedApp,
    Listen, //
};

/// The open connections one listener holds at most, the default.
pub const MAX_CONNECTIONS: usize = 4_096;

/// How long the loop waits after an accept that failed.
const ACCEPT_RETRY: Duration = Duration::from_millis(10);

/// The TCP listener a serving plugin binds and the boot runs.
pub struct TcpListen {
    /// The bound listener.
    listener: TcpListener,
    /// The TLS configuration of the port, `None` for a plain port.
    tls: Option<Rc<TlsConfig>>,
}

impl TcpListen {
    /// Binds `port` on every address of the current runtime, with TLS when
    /// `tls` names a configuration.
    ///
    /// # Errors
    ///
    /// Returns the error of the bind on IPv4 when IPv6 is refused too.
    pub fn bind(port: u16, tls: Option<Rc<TlsConfig>>) -> io::Result<Self> {
        let any6 = SocketAddr::from((Ipv6Addr::UNSPECIFIED, port));
        let listener = match TcpListener::bind(any6) {
            Ok(listener) => listener,
            Err(_) => TcpListener::bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, port)))?,
        };
        Ok(Self { listener, tls })
    }

    /// Returns the address the listener is bound to.
    ///
    /// # Errors
    ///
    /// Returns the error of `getsockname`.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }
}

impl Listen for TcpListen {
    fn run(self: Box<Self>, app: Rc<BootedApp>) -> LocalBoxFuture<'static, ()> {
        Box::pin(async move {
            let open = Rc::new(Cell::new(0_usize));
            loop {
                let stream = match self.listener.accept().await {
                    Ok((stream, _)) => stream,
                    Err(error) => {
                        // A full descriptor table fails every accept until a
                        // connection closes, so the loop waits a moment.
                        eprintln!("aosabi-serve: accept failed: {error}");
                        if let Ok(timer) = RuntimeTimer::current() {
                            timer.sleep_for(ACCEPT_RETRY).await;
                        }
                        continue;
                    }
                };
                if open.get() >= MAX_CONNECTIONS {
                    drop(stream);
                    continue;
                }
                open.set(open.get() + 1);
                let served = serve_accepted(stream, Rc::clone(&app), self.tls.clone());
                let open = Rc::clone(&open);
                spawn_local(Box::pin(async move {
                    served.await;
                    open.set(open.get() - 1);
                }));
            }
        })
    }
}
