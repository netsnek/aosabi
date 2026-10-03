// SPDX-License-Identifier: MIT
/*
 * aosabi-carrier-network example raw_server: GraphTP(Homa) on Network alone,
 * over a fetch that answers every OPERATION at once.
 *
 * No analogue on the reference side. The program is the server build that
 * names GraphTP(Homa) on Network with nothing of an app behind its listener,
 * which the raw server cell of the plan of the Homa track weighs by the code
 * and read only bytes of the symbols of homa with its IP driver, of this
 * crate and of socket2. It ships in no artifact.
 */

//! A server of `GraphTP`(Homa) on Network alone.
//!
//! The program reads `AOSABI_HOMA_NETWORK` and `AOSABI_HOMA_PROTECTION` as a
//! card's driver does, serves the path through the `GraphTP` listener with
//! the bounds of the card `server` over a fetch that answers every OPERATION
//! with `{"data":{"ok":true}}`, and runs until the path fails. It exits 64
//! where the environment names no path or one that
//! [`HomaNetwork::from_env`] refuses, `NoProtection` among them, 70 where
//! another process holds the Homa port, and 1 for any other failure.

use std::{
    future,
    process::ExitCode,
    rc::Rc, //
};

use aosabi_carrier_network::{
    HomaNetwork,
    NetworkError,
    serve_ip, //
};
use aosabi_serve::runtime::Runtime;
use graphql_over_graphtp::{
    Answer,
    Body,
    Fetch,
    Listener,
    Reply,
    Request,
    limits, //
};

/// The answer of every OPERATION.
const OK: &[u8] = br#"{"data":{"ok":true}}"#;

/// The media type of the answer, the answer type of `GraphTP` as Yoga writes
/// it.
const ANSWER_TYPE: &[u8] = b"application/graphql-response+json; charset=utf-8";

fn main() -> ExitCode {
    let options = match HomaNetwork::from_env() {
        Ok(Some(options)) => options,
        Ok(None) => {
            eprintln!("raw_server: AOSABI_HOMA_NETWORK names no path");
            return ExitCode::from(64);
        }
        Err(error) => {
            eprintln!("raw_server: {error}");
            return ExitCode::from(64);
        }
    };
    let fetch: Fetch = Rc::new(|_request: Request| -> Reply {
        Box::pin(future::ready(Answer {
            status: 200,
            substatus: 0,
            content_type: Some(ANSWER_TYPE.to_vec()),
            body: Body::Full(OK.to_vec()),
        }))
    });
    let listener = match Listener::new(fetch, &limits::SERVER) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("raw_server: the listener: {error}");
            return ExitCode::from(1);
        }
    };
    let runtime = match Runtime::new() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("raw_server: the runtime: {error}");
            return ExitCode::from(1);
        }
    };
    match runtime.block_on(serve_ip(options, listener)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(NetworkError::PortTaken(port)) => {
            eprintln!("raw_server: PortTaken: another process holds the Homa port {port}");
            ExitCode::from(70)
        }
        Err(error) => {
            eprintln!("raw_server: {error}");
            ExitCode::from(1)
        }
    }
}
