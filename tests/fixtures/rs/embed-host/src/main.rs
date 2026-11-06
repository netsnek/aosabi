// SPDX-License-Identifier: MIT
/*
 * embed-host: a program that boots the roots of an app and answers request bodies line by line.
 */

//! A program that holds the engine and the roots of an app in itself.
//!
//! The host boots the app of `tests/fixtures/rs/embed-app` through
//! [`aosabi::embed::Embedded::boot`] and reads one GraphQL request body per
//! line from standard input, the JSON object a GraphQL request over HTTP
//! carries. It writes one line for each, the JSON text of the response body,
//! and skips a line that is empty. The argument `--schema` prints the schema
//! text the engine answers with instead.
//!
//! A boot that fails exits with 70 and its message on standard error, and a
//! line that cannot be read or written exits with 74.

use std::{
    io::{
        self,
        BufRead as _,
        Write as _, //
    },
    process::ExitCode,
};

use aosabi::embed::Embedded;

/// The exit status of a boot that fails, `EX_SOFTWARE` of `sysexits.h`.
const EX_SOFTWARE: u8 = 70;

/// The exit status of a line that cannot be read or written, `EX_IOERR`.
const EX_IOERR: u8 = 74;

fn main() -> ExitCode {
    let engine = match Embedded::boot(embed_app::__aosabi::entry, aosabi::embed_output!()) {
        Ok(engine) => engine,
        Err(error) => {
            eprintln!("embed-host: {error}");
            return ExitCode::from(EX_SOFTWARE);
        }
    };
    let mut out = io::stdout().lock();
    if std::env::args().nth(1).as_deref() == Some("--schema") {
        return match out.write_all(engine.schema().as_bytes()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(_) => ExitCode::from(EX_IOERR),
        };
    }
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else {
            return ExitCode::from(EX_IOERR);
        };
        if line.trim().is_empty() {
            continue;
        }
        let answer = engine.answer(&line);
        if writeln!(out, "{answer}")
            .and_then(|()| out.flush())
            .is_err()
        {
            return ExitCode::from(EX_IOERR);
        }
    }
    ExitCode::SUCCESS
}
