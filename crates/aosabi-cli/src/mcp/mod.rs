// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-cli: the command mcp, the protocol server over standard input and
 * output.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/cli/mcp.ts,
 * Apache-2.0. Changed by the Aosabi authors.
 *
 * Each file of the directory names the part it translates.
 *
 * Derived from @modelcontextprotocol/sdk 1.29.0, dist/cjs, MIT.
 */

//! The command `mcp`.
//!
//! [`start_mcp_server`] runs a server named `aosabi` with the capability
//! `tools` over the stdio transport of the Model Context Protocol and resolves
//! `-c` against the working directory:
//!
//! - `initialize` answers
//!   `{protocolVersion, capabilities: {tools: {}}, serverInfo: {name, version}}`,
//!   the requested version when the SDK supports it and `2025-11-25`
//!   otherwise, from `2025-11-25`, `2025-06-18`, `2025-03-26`, `2024-11-05`
//!   and `2024-10-07`.
//! - `ping` answers `{}`, `tools/list` and `tools/call` answer through
//!   [`tools`], and every other method answers `-32601` with `Method not
//!   found`. A request whose parameters do not have the shape of its method
//!   answers `-32603`, the code of a handler that throws with no code of its
//!   own, with a message of Aosabi, where the SDK writes the issues of its
//!   schema check.
//! - A notification, a response and a line that does not parse are consumed
//!   and answered with nothing, through [`jsonrpc`].
//!
//! Requests are answered one after the other, the one difference from the
//! SDK, whose handlers run concurrently. The server ends with 0 when its
//! standard input ends, and standard output carries the protocol and nothing
//! else from the first byte.
//!
//! | File | Holds |
//! | --- | --- |
//! | `jsonrpc.rs` | the framing of a line, the check of a message, the two answers and the writer |
//! | `tools.rs` | the four tools, their texts and the handler of `tools/call` |

pub mod jsonrpc;
pub mod tools;

use std::{
    io::{
        self,
        Write as _, //
    },
    path::{
        Component,
        Path,
        PathBuf, //
    },
    process::ExitCode,
};

use aosabi_json::{
    JsObject,
    JsValue, //
};

use crate::{
    args::{
        Global,
        McpArgs,
        VERSION, //
    },
    cargo,
    report::{
        self,
        CliError, //
    }, //
};

/// The version the SDK answers for a version it does not support.
pub const LATEST_PROTOCOL_VERSION: &str = "2025-11-25";

/// Every version the SDK supports, the latest first.
pub const SUPPORTED_PROTOCOL_VERSIONS: [&str; 5] = [
    LATEST_PROTOCOL_VERSION,
    "2025-06-18",
    "2025-03-26",
    "2024-11-05",
    "2024-10-07",
];

/// The name the server gives in `serverInfo`.
pub const SERVER_NAME: &str = "aosabi";

/// Runs `mcp` with `args` under `global`, resolving `-c` against the working
/// directory, and answers the exit code.
#[must_use]
pub fn command(args: &McpArgs, global: &Global) -> ExitCode {
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(error) => {
            report::report_failure(
                &CliError::new(format!("the working directory is unreadable: {error}")),
                global.verbose,
            );
            return ExitCode::FAILURE;
        }
    };
    let root = resolve(&cwd, &args.cwd);
    let manifest_path = global
        .manifest_path
        .as_deref()
        .map(|path| resolve(&cwd, path));
    start_mcp_server(&root, args.entry.as_deref(), manifest_path.as_deref())
}

/// Serves the protocol on standard input and output for the project at
/// `root`, with the entry `entry` and the manifest `manifest_path` forwarded
/// to every child of a tool, until standard input ends.
#[must_use]
pub fn start_mcp_server(
    root: &Path,
    entry: Option<&Path>,
    manifest_path: Option<&Path>,
) -> ExitCode {
    let context = tools::Context {
        root: root.to_path_buf(),
        entry: entry.map(Path::to_path_buf),
        manifest_path: manifest_path.map(Path::to_path_buf),
    };
    let stdin = io::stdin();
    let mut input = stdin.lock();
    loop {
        let line = match jsonrpc::read_line(&mut input) {
            Ok(Some(line)) => line,
            Ok(None) => return ExitCode::SUCCESS,
            Err(error) => {
                let _ = writeln!(io::stderr(), "error: the standard input failed: {error}");
                return ExitCode::FAILURE;
            }
        };
        let Some(jsonrpc::Message::Request { id, method, params }) = jsonrpc::message(&line) else {
            continue;
        };
        let answer = answer(&context, &id, &method, &params);
        if jsonrpc::write(&mut io::stdout().lock(), &answer).is_err() {
            return ExitCode::FAILURE;
        }
    }
}

/// Returns the answer of the request `id` of `method` with `params`.
fn answer(context: &tools::Context, id: &JsValue, method: &str, params: &JsObject) -> JsValue {
    let params = JsValue::Object(params.clone());
    match method {
        "initialize" => match initialize(&params) {
            Some(result) => jsonrpc::result(id, result),
            None => jsonrpc::error(id, jsonrpc::INTERNAL_ERROR, "Invalid initialize parameters"),
        },
        "ping" => jsonrpc::result(id, JsValue::Object(JsObject::new())),
        "tools/list" => jsonrpc::result(id, tools::list()),
        "tools/call" => match cargo::text(&params, "name") {
            Some(name) => {
                let arguments = match cargo::field(&params, "arguments") {
                    None | Some(JsValue::Null) => JsValue::Object(JsObject::new()),
                    Some(arguments) => arguments.clone(),
                };
                jsonrpc::result(id, tools::call(context, name, &arguments))
            }
            None => jsonrpc::error(id, jsonrpc::INTERNAL_ERROR, "Invalid tools/call parameters"),
        },
        _ => jsonrpc::error(id, jsonrpc::METHOD_NOT_FOUND, "Method not found"),
    }
}

/// Returns the result of `initialize` with `params`, `None` when they carry
/// no version, no capabilities or no client.
fn initialize(params: &JsValue) -> Option<JsValue> {
    let requested = cargo::text(params, "protocolVersion")?;
    if !matches!(
        cargo::field(params, "capabilities"),
        Some(JsValue::Object(_))
    ) {
        return None;
    }
    let client = cargo::field(params, "clientInfo")?;
    cargo::text(client, "name")?;
    cargo::text(client, "version")?;
    let version = if SUPPORTED_PROTOCOL_VERSIONS.contains(&requested) {
        requested
    } else {
        LATEST_PROTOCOL_VERSION
    };
    let mut capabilities = JsObject::with_capacity(1);
    capabilities.insert("tools", JsValue::Object(JsObject::new()));
    let mut server_info = JsObject::with_capacity(2);
    server_info.insert("name", SERVER_NAME);
    server_info.insert("version", VERSION);
    let mut result = JsObject::with_capacity(3);
    result.insert("protocolVersion", version);
    result.insert("capabilities", JsValue::Object(capabilities));
    result.insert("serverInfo", JsValue::Object(server_info));
    Some(JsValue::Object(result))
}

/// Returns `path` read against `cwd` with every `.` dropped and every `..`
/// taken back, which is `path.resolve(cwd, path)`.
fn resolve(cwd: &Path, path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in cwd.join(path).components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            part => out.push(part),
        }
    }
    out
}
