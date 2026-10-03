// SPDX-License-Identifier: MIT
/*
 * aosabi-serve example far_timer: the runtime with one timer beyond the 32 bit
 * range of a wait, serving one connection.
 *
 * No analogue on the reference side. The program is the probe of the timeout
 * of the epoll reactor, which tests/notebooks/08_card_builds.ipynb builds for
 * the host with each backend of rustix and for the player's own target and
 * runs in each. It ships in no artifact.
 */

//! A runtime that accepts one connection while a far timer waits.
//!
//! The program reads a delay in milliseconds from its first argument, spawns
//! a task that awaits [`aosabi_core::clock::delay`] of it, binds a TCP
//! listener of the runtime on 127.0.0.1 at a port the kernel chooses, prints
//! the line `port <port>` and awaits one connection. When one arrives it
//! prints the line `accepted` and exits 0 while the timer still waits. It
//! exits 64 when its argument is no number of milliseconds and 1 when the
//! runtime or the listener fails.
//!
//! The least deadline of the loop is the timer's, so each turn waits in the
//! reactor up to it. A reactor that cannot wait that long writes a line to
//! standard error on every turn and reads no event, so the program accepts
//! nothing while its processor time grows, which the notebook reads.

use std::{
    env,
    process::ExitCode, //
};

use aosabi_serve::runtime::{
    Runtime,
    net::TcpListener,
    spawn_local, //
};

fn main() -> ExitCode {
    let Some(delay_ms) = env::args().nth(1).and_then(|arg| arg.parse::<u64>().ok()) else {
        eprintln!("far_timer: the first argument is no delay in milliseconds");
        return ExitCode::from(64);
    };
    let runtime = match Runtime::new() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("far_timer: the runtime: {error}");
            return ExitCode::from(1);
        }
    };
    aosabi_core::clock::install_clock(runtime.clock());
    runtime.block_on(async move {
        spawn_local(Box::pin(aosabi_core::clock::delay(delay_ms)));
        let listener = match TcpListener::bind("127.0.0.1:0") {
            Ok(listener) => listener,
            Err(error) => {
                eprintln!("far_timer: the listener: {error}");
                return ExitCode::from(1);
            }
        };
        match listener.local_addr() {
            Ok(address) => println!("port {}", address.port()),
            Err(error) => {
                eprintln!("far_timer: the address of the listener: {error}");
                return ExitCode::from(1);
            }
        }
        match listener.accept().await {
            Ok(_) => {
                println!("accepted");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("far_timer: the accept: {error}");
                ExitCode::from(1)
            }
        }
    })
}
