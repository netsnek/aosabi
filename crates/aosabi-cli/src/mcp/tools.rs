// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-cli: the four tools of the protocol server.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/cli/mcp.ts,
 * Apache-2.0. Changed by the Aosabi authors.
 */

//! The tools.
//!
//! `aosabi mcp` offers four tools whose names, descriptions and input schemas
//! are literals, `describe_app`, `get_entity` with a required `name`,
//! `get_operation` with a required `name`, and `verify`. [`list`] answers
//! them with the keys `name`, `description` and `inputSchema` in that order.
//!
//! [`call`] is the handler of `tools/call`. Every tool runs the command line
//! again as a child in the root, `inspect --json` or `verify --json` with
//! the entry forwarded as `-e`, and reads its standard output.
//! `describe_app` and `verify` answer that output as one text item,
//! `get_entity` answers `No entity "<name>". Known: ` and the entity names
//! joined by `, `, or the entity with its authz row and the operations that
//! mention it, `get_operation` answers the operation through the writer with
//! the gap of two spaces or `No operation "<name>".`, and an unknown tool
//! answers `Unknown tool "<name>".`. A child that fails answers its trimmed
//! standard error or `aosabi <command> exited <code>`, and every failure is
//! the result `{content: [{type: "text", text}], isError: true}`. The
//! standard error of a child is kept for that answer and written to the
//! standard error of the server after the child ends, so the protocol stream
//! on standard output stays pure.
//!
//! While [`VENDORED_CHILD_RUNS_NO_COMMAND`] is set, no child runs and every
//! tool reads an empty standard output. `describe_app` and `verify` therefore
//! answer an empty text, and `get_entity` and `get_operation` answer
//! `Unexpected end of JSON input`, the message of `JSON.parse` over an empty
//! text, with `isError`. Without the quirk the child is
//! [`std::env::current_exe`] and the tools answer the model and the verdict.

use std::{
    io::{
        self,
        Write, //
    },
    path::PathBuf,
    process::{
        Command,
        Stdio, //
    },
};

use aosabi_json::{
    JsObject,
    JsValue,
    number_to_string,
    to_string_with_gap, //
};

use crate::cargo;

/// Whether the child of a tool runs no command and prints nothing.
pub const VENDORED_CHILD_RUNS_NO_COMMAND: bool = true;

/// The message `JSON.parse` throws for a text with no value.
pub const UNEXPECTED_END: &str = "Unexpected end of JSON input";

/// The description of `describe_app`.
const DESCRIBE_APP: &str = "The whole-app model in one structured artifact: GraphQL schema + persistence entities (columns/relations/indexes) + operations + per-model authz + queues. Read this first to understand the app instead of opening many files.";

/// The description of `get_entity`.
const GET_ENTITY: &str = "One persisted entity \u{2014} its columns, relations and indexes \u{2014} plus its authz shape and the operations that reference it.";

/// The description of the argument of `get_entity`.
const ENTITY_NAME: &str = "Entity name, e.g. \"Post\"";

/// The description of `get_operation`.
const GET_OPERATION: &str =
    "One GraphQL operation: its root (Query/Mutation), arguments and return type.";

/// The description of the argument of `get_operation`.
const OPERATION_NAME: &str = "Operation name, e.g. \"posts\"";

/// The description of `verify`.
const VERIFY: &str = "Build + typecheck + migration check \u{2192} a stratified verdict (pass / review / fail). Call this AFTER editing source to know whether the change is safe: \"review\" flags follow-ups like an uncaptured migration; \"fail\" means it will not build or typecheck.";

/// Where the children of the tools run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Context {
    /// The project root, the working directory of every child.
    pub root: PathBuf,
    /// The entry `-e` forwards, when the server was given one.
    pub entry: Option<PathBuf>,
    /// The `--manifest-path` every child takes, when the server was given
    /// one.
    pub manifest_path: Option<PathBuf>,
}

/// Returns the result of `tools/list`, `{tools}` with the four tools.
#[must_use]
pub fn list() -> JsValue {
    let tools = vec![
        tool("describe_app", DESCRIBE_APP, None),
        tool("get_entity", GET_ENTITY, Some(ENTITY_NAME)),
        tool("get_operation", GET_OPERATION, Some(OPERATION_NAME)),
        tool("verify", VERIFY, None),
    ];
    let mut object = JsObject::with_capacity(1);
    object.insert("tools", tools);
    JsValue::Object(object)
}

/// Returns the result of `tools/call` of the tool `name` with `arguments`.
#[must_use]
pub fn call(context: &Context, name: &str, arguments: &JsValue) -> JsValue {
    let answer = match name {
        "describe_app" => run_cli(context, "inspect").map(|out| text(&out)),
        "verify" => run_cli(context, "verify").map(|out| text(&out)),
        "get_entity" => get_entity(context, &js_string(cargo::field(arguments, "name"))),
        "get_operation" => get_operation(context, &js_string(cargo::field(arguments, "name"))),
        _ => Ok(fail(&format!("Unknown tool \"{name}\"."))),
    };
    answer.unwrap_or_else(|message| fail(&message))
}

/// Returns one tool, with a required string argument `name` described by
/// `argument` when it takes one.
fn tool(name: &str, description: &str, argument: Option<&str>) -> JsValue {
    let mut schema = JsObject::with_capacity(3);
    schema.insert("type", "object");
    let mut properties = JsObject::new();
    if let Some(argument) = argument {
        let mut property = JsObject::with_capacity(2);
        property.insert("type", "string");
        property.insert("description", argument);
        properties.insert("name", JsValue::Object(property));
    }
    schema.insert("properties", JsValue::Object(properties));
    if argument.is_some() {
        schema.insert("required", vec![JsValue::from("name")]);
    }
    let mut object = JsObject::with_capacity(3);
    object.insert("name", name);
    object.insert("description", description);
    object.insert("inputSchema", JsValue::Object(schema));
    JsValue::Object(object)
}

/// Returns the answer of `get_entity` for the entity `name`.
fn get_entity(context: &Context, name: &str) -> Result<JsValue, String> {
    let model = parse_model(&run_cli(context, "inspect")?)?;
    let schema = member(&model, "schema")?;
    let entities = member(schema, "entities")?;
    let Some(entity) = cargo::field(entities, name) else {
        let known: Vec<&str> = match entities {
            JsValue::Object(object) => object.iter().filter_map(|(key, _)| key.as_str()).collect(),
            _ => Vec::new(),
        };
        return Ok(fail(&format!(
            "No entity \"{name}\". Known: {}",
            known.join(", ")
        )));
    };
    let operations: Vec<JsValue> = cargo::items(schema, "operations")
        .iter()
        .filter(|operation| {
            aosabi_json::to_string(operation).is_some_and(|written| written.contains(name))
        })
        .cloned()
        .collect();
    let authz = cargo::items(&model, "authz")
        .iter()
        .find(|row| cargo::text(row, "model") == Some(name))
        .cloned()
        .unwrap_or_default();
    let mut answer = JsObject::with_capacity(3);
    answer.insert("entity", entity.clone());
    answer.insert("authz", authz);
    answer.insert("operations", operations);
    Ok(text(
        &to_string_with_gap(&JsValue::Object(answer), "  ").unwrap_or_default(),
    ))
}

/// Returns the answer of `get_operation` for the operation `name`.
fn get_operation(context: &Context, name: &str) -> Result<JsValue, String> {
    let model = parse_model(&run_cli(context, "inspect")?)?;
    let schema = member(&model, "schema")?;
    let found = cargo::items(schema, "operations")
        .iter()
        .find(|operation| cargo::text(operation, "name") == Some(name));
    Ok(match found {
        Some(operation) => text(&to_string_with_gap(operation, "  ").unwrap_or_default()),
        None => fail(&format!("No operation \"{name}\".")),
    })
}

/// Returns the member `key` of `value`, failing as reading a property of
/// `undefined` fails.
fn member<'a>(value: &'a JsValue, key: &str) -> Result<&'a JsValue, String> {
    cargo::field(value, key)
        .ok_or_else(|| format!("Cannot read properties of undefined (reading '{key}')"))
}

/// Returns the value of the JSON text `output`, failing with the message of
/// `JSON.parse` for a text with no value.
fn parse_model(output: &str) -> Result<JsValue, String> {
    if output
        .trim_matches(|c: char| matches!(c, ' ' | '\t' | '\n' | '\r'))
        .is_empty()
    {
        return Err(String::from(UNEXPECTED_END));
    }
    aosabi_json::parse(output.as_bytes()).map_err(|error| error.to_string())
}

/// Runs the command line as a child with `command --json` in the root and
/// answers its standard output, or the failure.
fn run_cli(context: &Context, command: &str) -> Result<String, String> {
    if VENDORED_CHILD_RUNS_NO_COMMAND {
        return Ok(String::new());
    }
    let program = std::env::current_exe().map_err(|error| error.to_string())?;
    let mut child = Command::new(program);
    child.args([command, "--json"]).current_dir(&context.root);
    if let Some(entry) = &context.entry {
        child.arg("-e").arg(entry);
    }
    if let Some(manifest_path) = &context.manifest_path {
        child.arg("--manifest-path").arg(manifest_path);
    }
    let done = child
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| error.to_string())?;
    let _ = io::stderr().lock().write_all(&done.stderr);
    if done.status.success() {
        return Ok(String::from_utf8_lossy(&done.stdout).into_owned());
    }
    let stderr = String::from_utf8_lossy(&done.stderr);
    let stderr = stderr.trim();
    if stderr.is_empty() {
        let code = done
            .status
            .code()
            .map_or_else(|| String::from("null"), |code| code.to_string());
        Err(format!("aosabi {command} exited {code}"))
    } else {
        Err(stderr.to_owned())
    }
}

/// Returns `String(value ?? '')` of the argument `value`.
fn js_string(value: Option<&JsValue>) -> String {
    match value {
        None | Some(JsValue::Undefined | JsValue::Null) => String::new(),
        Some(JsValue::Bool(value)) => value.to_string(),
        Some(JsValue::Number(value)) => number_to_string(*value),
        Some(JsValue::String(value)) => String::from_utf8_lossy(value.as_wtf8()).into_owned(),
        Some(JsValue::Array(items)) => items
            .iter()
            .map(|item| match item {
                JsValue::Undefined | JsValue::Null => String::new(),
                item => js_string(Some(item)),
            })
            .collect::<Vec<_>>()
            .join(","),
        Some(JsValue::Object(_) | JsValue::Opaque(_)) => String::from("[object Object]"),
    }
}

/// Returns the result of one text item, `{content: [{type: "text", text}]}`.
fn text(text: &str) -> JsValue {
    let mut object = JsObject::with_capacity(1);
    object.insert("content", vec![text_item(text)]);
    JsValue::Object(object)
}

/// Returns the failure of one text item with `isError`.
fn fail(message: &str) -> JsValue {
    let mut object = JsObject::with_capacity(2);
    object.insert("content", vec![text_item(message)]);
    object.insert("isError", true);
    JsValue::Object(object)
}

/// Returns `{type: "text", text}`.
fn text_item(text: &str) -> JsValue {
    let mut item = JsObject::with_capacity(2);
    item.insert("type", "text");
    item.insert("text", text);
    JsValue::Object(item)
}
