// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool execute: executes an operation without HTTP, over a schema
 * whose resolvers a behaviour table describes.
 *
 * No analogue in the reference. The crates under test are aosabi-execute and
 * the resolver wrapping of aosabi-core, and the judging notebook is
 * tests/notebooks/20_execute.ipynb, which compares the bytes against the node
 * sidecar tests/sidecar/execute.mjs running the executor Yoga runs over the
 * same table.
 */

//! The `execute` primitive of the seam.
//!
//! `--table PATH` names a behaviour table, which is what [`table`] reads, and
//! `--request PATH` names one request, a JSON object with a `document` and
//! optionally `variables`, `operationName`, `validate` and `trace`. One of
//! the two paths may be `-`, which is stdin. The payload is the result text,
//! which is the execution result written by aosabi-json the way
//! `JSON.stringify` writes it on the node side: `errors` first when there is
//! one, then `data`, and each error in the key order `message`, `locations`,
//! `path`, `extensions`. With `--trace` one newline and the settle order
//! trace as one JSON array follow the result text.
//!
//! A document that does not parse, a document validation refuses and an
//! early error of the executor are results and exit 0. What the entry throws
//! exits 1 with `[{"message":..}]`, and so does a table whose schema does not
//! build. A file that is no table or no request exits 65.
//!
//! `--requests PATH` is the batch form of [`crate::batch`] with two
//! operations. `table` loads the behaviour table under `table` and answers
//! `{"name":..,"ok":true}`, or `{"errors":[{"message":..}]}` for a schema
//! that does not build. `execute` runs `document` against the table whose
//! name travels as `table` and answers `{"result":"<text>"}`, or
//! `{"thrown":{"name":..,"message":..}}` for what the entry throws, and
//! `"trace":[..]` last when the request asks for it. `result` travels as a
//! string, so the comparison sees the bytes of the result and not a second
//! reading of them.
//!
//! The result text is written from the arena of the execution, the writer a
//! served answer takes, and `--result tree` writes it from the tree its data
//! builds on its first read instead, the data a hook reads, so a notebook
//! holds both readings to the bytes of the reference.
//!
//! One request runs on one fresh agent with virtual timers, so the clock of
//! every request starts at 0. An event of the trace carries `time`, `label`
//! and `state` as the node side writes them, and `tick` last, the count of
//! jobs started, which only this side has. `--timers host` runs every agent
//! on the clock of the host instead, one unit a millisecond, which [`clock`]
//! keeps, and the order of two timers stays the order of their deadlines.
//!
//! The batch form answers a third operation, `subscribe`, which subscribes to
//! one document against a loaded table and pulls its stream, and which
//! [`subscribe`] describes. A table reaches the subscriptions through four
//! more spec kinds, `repeater`, `not_iterable`, `pubsub_topic` and
//! `pubsub_publish`, and each request has a pub sub of its own, which
//! [`pub_sub`] keeps.
//!
//! Three more operations of the batch form serve a root dropped and built
//! again in one process, which the node side builds again with no such
//! operation. `root` builds the factory of the root named `root` of the table
//! under `table`, a service table, outside every schema, reads
//! each member of it as a root field reads it and holds the factory under the
//! name of the table, and answers `{"name":..,"read":..}` with the count of
//! the members read. `drop` drops the table or the held root whose name
//! travels as `table` and answers `{"name":..,"dropped":true}`. `roots`
//! answers `{"live":..,"gone":..}`, the entries of the table of root members
//! of aosabi-core on the thread of the run whose factory lives and whose
//! factory is gone.
//!
//! The operation `coerce` coerces the variables of `document`, the operation
//! `operationName` names, against the table whose name travels as `table`,
//! through `get_variable_values` of aosabi-execute twice: from values the
//! coercion owns, as the executor holds the variables of an in process
//! request, and from values another reader shares, as the parameters of a
//! served request share them. It answers
//! `{"owned":"<text>","shared":"<text>"}`, each the text of `{"coerced":..}`
//! or of `{"errors":[..]}`, so a notebook holds the two coercions to each
//! other where the executor runs only the first.
//!
//! The operation `project` wraps the member of the root `Query` of a loaded
//! service table that the first field of a query names, as the root resolver
//! wrapping of aosabi-core wraps it, assigns the `writes` of the request
//! to the projection and answers its own keys and its `reads`, which
//! [`project`] describes, so a notebook holds the keys of the view to those
//! of the object of the reference, which no answer of the reference reads.

mod clock;
mod interpret;
mod project;
mod pub_sub;
mod session;
mod subscribe;
mod table;

use std::{
    borrow::Cow,
    cell::RefCell,
    path::PathBuf,
    process::ExitCode,
    rc::Rc, //
};

use aosabi_core::resolvers::{
    resolvers_to_graphql_resolvers::root_member_counts,
    wrap_resolver::set_eager_projection, //
};
use aosabi_execute::{
    errors::copy_graphql_error,
    kernel_entry::{
        Executed,
        Request,
        RunFailure, //
    },
    normalized_executor::get_operation_ast,
    promise::{
        AgentOptions,
        TraceEvent, //
    },
    result::ExecutionResult,
    value::{
        Value,
        to_js_string, //
    },
    values::{
        CoercedVariableValues,
        get_variable_values, //
    }, //
};
use aosabi_json::{
    JsObject,
    JsString,
    JsValue, //
};
use aosabi_lang::{
    ParseOptions,
    js_string::to_string_lossy, //
};

use crate::{
    batch::{
        self,
        Operations, //
    },
    cli::{
        self,
        Choice,
        Grammar,
        Matches,
        Opt,
        Refusal, //
    },
    execute::{
        interpret::{
            in_window,
            set_host_timers,
            timer_mode, //
        },
        pub_sub::begin_request,
        session::{
            HoldError,
            LoadError,
            Session, //
        },
        table::Table, //
    },
    homa_packet::count,
    seam::{
        self,
        Answer,
        Failure, //
    }, //
};

/// The name of the primitive, which opens every refusal it writes.
const NAME: &str = "execute";

/// The milestone the primitive lands at.
const LANDS_AT: &str = "M4";

/// The operations of the protocol that reach aosabi-execute.
const OPERATIONS: &Operations = &[
    ("table", op_table),
    ("execute", op_execute),
    ("subscribe", subscribe::op_subscribe),
    ("root", op_root),
    ("drop", op_drop),
    ("roots", op_roots),
    ("coerce", op_coerce),
    ("project", project::op_project),
];

/// The refusal of a run that ends with its result pending.
const STALLED: &str =
    "the request stalled: no job, no timer and no waker is left and the result is pending";

thread_local! {
    /// Whether the result texts of the run are written from the tree of their data, `--result tree`.
    static TREE: core::cell::Cell<bool> = const { core::cell::Cell::new(false) };

    /// The tables the batch form has loaded.
    ///
    /// An operation of [`crate::batch`] is a plain function over one request,
    /// and a `table` request is read by the `execute` requests behind it, so
    /// the tables live beside the operations for the length of the run. A
    /// run answers its file on one thread.
    static SESSION: RefCell<Session> = RefCell::new(Session::default());
}

/// The command line of the `execute` primitive.
pub static GRAMMAR: Grammar = Grammar {
    options: &[
        cli::SMOKE,
        Opt::path(
            "table",
            "PATH",
            "Read the behaviour table from this file, or from stdin when it is `-`",
        ),
        Opt::path(
            "request",
            "PATH",
            "Read the request from this file, or from stdin when it is `-`",
        ),
        Opt::flag(
            "trace",
            "Write the settle order trace behind the result text",
        ),
        cli::REQUESTS,
        Opt::choice(
            "timers",
            "MODE",
            "The clock the timers of every request read",
            &TIMERS,
        )
        .default("virtual"),
        Opt::choice(
            "result",
            "FORM",
            "What every result text is written from",
            &RESULT_FORMS,
        )
        .default("arena"),
        Opt::choice(
            "projection",
            "FORM",
            "What the resolver wrapping projects an object into",
            &PROJECTIONS,
        )
        .default("view"),
    ],
    positional: None,
    conflicts: &[("requests", &["table", "request", "trace"])],
};

/// The clocks of `--timers`, in the order of the help.
const TIMERS: [Choice; 2] = [
    Choice {
        name: "virtual",
        help: "A clock that starts at 0 and moves only when a timer fires",
    },
    Choice {
        name: "host",
        help: "The monotonic clock of the host, one unit a millisecond",
    },
];

/// The forms of `--result`, in the order of the help.
const RESULT_FORMS: [Choice; 2] = [
    Choice {
        name: "arena",
        help: "The arena of the execution, which a served answer is written from",
    },
    Choice {
        name: "tree",
        help: "The tree the data of the result builds on its first read, which a hook reads",
    },
];

/// The forms of `--projection`, in the order of the help.
const PROJECTIONS: [Choice; 2] = [
    Choice {
        name: "view",
        help: "A view of the selection, the projection every card builds",
    },
    Choice {
        name: "object",
        help: "The plain object the reference builds, the second judge of the view",
    },
];

/// What the resolver wrapping of a run projects an object into.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProjectionForm {
    /// A view of the selection, the projection every card builds.
    View,
    /// The plain object the reference builds.
    Object,
}

/// What the result texts of a run are written from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResultForm {
    /// The arena of the execution.
    Arena,
    /// The tree of the data.
    Tree,
}

/// What the `execute` primitive accepts.
#[derive(Debug)]
pub struct ExecuteArgs {
    /// Whether the call asks for the interface line.
    smoke: bool,
    /// The file of the behaviour table, where `-` is stdin.
    table: Option<PathBuf>,
    /// The file of the request, where `-` is stdin.
    request: Option<PathBuf>,
    /// Whether the call writes the settle order trace.
    trace: bool,
    /// The file of request lines of the batch form.
    requests: Option<PathBuf>,
    /// Whether every agent reads the clock of the host.
    host_timers: bool,
    /// What every result text is written from.
    result: ResultForm,
    /// What the resolver wrapping projects an object into.
    projection: ProjectionForm,
}

impl ExecuteArgs {
    /// Reads the arguments `matches` holds for [`GRAMMAR`].
    ///
    /// # Errors
    ///
    /// Returns a [`Refusal`] when the value of `--timers` is not UTF-8, which
    /// the parser refuses before for an option of choices.
    pub fn read(matches: &Matches) -> Result<Self, Refusal> {
        Ok(Self {
            smoke: matches.flag("smoke"),
            table: matches.path("table"),
            request: matches.path("request"),
            trace: matches.flag("trace"),
            requests: matches.path("requests"),
            host_timers: matches.text("timers")?.as_deref() == Some("host"),
            result: if matches.text("result")?.as_deref() == Some("tree") {
                ResultForm::Tree
            } else {
                ResultForm::Arena
            },
            projection: if matches.text("projection")?.as_deref() == Some("object") {
                ProjectionForm::Object
            } else {
                ProjectionForm::View
            },
        })
    }
}

/// Runs the `execute` primitive.
pub fn run(args: &ExecuteArgs) -> ExitCode {
    if args.smoke {
        return seam::smoke_at(NAME, LANDS_AT);
    }
    set_host_timers(args.host_timers);
    TREE.set(args.result == ResultForm::Tree);
    set_eager_projection(args.projection == ProjectionForm::Object);
    if let Some(requests) = &args.requests {
        return seam::finish(NAME, batch::run(OPERATIONS, requests));
    }
    seam::finish(NAME, answer(args))
}

/// Returns the result text of the one call the flags name.
fn answer(args: &ExecuteArgs) -> Result<Answer, Failure> {
    let table = seam::required(args.table.as_deref(), "--table")?;
    let request = seam::required(args.request.as_deref(), "--request")?;
    seam::refuse_shared_stdin(table, request)?;
    let table = Table::from_js(&seam::read_json(table)?)
        .map_err(|reason| Failure::Data(format!("the table breaks the grammar: {reason}")))?;
    let JsValue::Object(request) = seam::read_json(request)? else {
        return Err(Failure::Data(String::from("the request is no JSON object")));
    };
    let mut call = Call::of(&request).map_err(Failure::Data)?;
    call.trace |= args.trace;

    let name = table.name.clone();
    let mut session = Session::default();
    match session.load(table) {
        Ok(()) => {}
        Err(LoadError::Schema(error)) => return Ok(refused(error.message().clone())),
        Err(LoadError::NameTaken(name)) => return Err(Failure::Data(name_taken(&name))),
    }
    begin_request();
    let executed = call
        .run(&session, &name)
        .ok_or_else(|| Failure::Data(no_table(&name)))?;
    single_answer(executed, call.trace)
}

/// Returns the answer of the single form for one run.
fn single_answer(executed: Executed, trace: bool) -> Result<Answer, Failure> {
    let mut out = match executed.outcome {
        Ok(result) => result_bytes(&result),
        Err(RunFailure::Thrown(thrown)) => return Ok(refused(thrown_message(&thrown))),
        Err(RunFailure::Stalled) => return Err(Failure::Data(String::from(STALLED))),
    };
    if trace {
        out.push(b'\n');
        aosabi_json::write(&mut out, &trace_json(&executed.trace));
    }
    Ok(Answer::Payload(out))
}

/// Returns the bytes of the result text of `result`, as `JSON.stringify` writes it.
///
/// The data is written from the arena of its execution, as a served answer
/// writes it, and under `--result tree` from the tree
/// [`ExecutionResult::data`] builds, which the writer of `aosabi-json` writes.
pub(crate) fn result_bytes(result: &ExecutionResult) -> Vec<u8> {
    if !TREE.get() {
        let mut out = Vec::new();
        result.write_json(&mut out);
        return out;
    }
    let tree = ExecutionResult::new(
        result.errors.iter().map(copy_graphql_error).collect(),
        result.data().cloned(),
        result.extensions.clone(),
    );
    aosabi_json::to_bytes(&tree.to_js_value()).unwrap_or_default()
}

/// Returns the result text of `result`, the string of [`result_bytes`].
///
/// Both writers escape a lone surrogate and write every other code point as
/// its UTF-8, so the bytes are UTF-8 and the string takes them with no copy.
// A `String::from_utf8_unchecked` would skip this pass.
pub(crate) fn result_text(result: &ExecutionResult) -> String {
    String::from_utf8(result_bytes(result))
        .unwrap_or_else(|error| String::from_utf8_lossy(error.as_bytes()).into_owned())
}

/// Returns the answer that refuses a call with the one error `message`.
fn refused(message: JsString) -> Answer {
    Answer::refused(vec![seam::message_json(message)])
}

/// Returns the message of a thrown value.
///
/// An error answers its `message` and any other value `String(value)`.
fn thrown_message(thrown: &Value) -> JsString {
    match thrown {
        Value::Error(error) => error.message(),
        other => to_js_string(other),
    }
}

/// Returns the trace as one JSON array.
fn trace_json(trace: &[TraceEvent]) -> JsValue {
    JsValue::Array(trace.iter().map(TraceEvent::to_js_value).collect())
}

/// Returns the refusal of a table whose name the run holds already.
fn name_taken(name: &str) -> String {
    format!("the run holds a table named {name} already")
}

/// Returns the refusal of a request that names a table the run never loaded.
fn no_table(name: &str) -> String {
    format!("the run holds no table named {name}")
}

/// One call of the executor, which is everything an `execute` request names
/// beside its table.
struct Call {
    /// The document text.
    document: String,
    /// The raw variable values, when the request carries any.
    variables: Option<JsValue>,
    /// The name of the operation to run, when the request names one.
    operation_name: Option<String>,
    /// Whether the document is validated before it runs.
    validate: bool,
    /// Whether the settle order trace is recorded.
    trace: bool,
}

impl Call {
    /// Reads the call out of `request`.
    ///
    /// # Errors
    ///
    /// Returns the refusal of a request without a `document` string or with
    /// an `operationName` that is no string.
    fn of(request: &JsObject) -> Result<Self, String> {
        let Some(JsValue::String(document)) = request.get("document") else {
            return Err(String::from("the request carries no string field document"));
        };
        let operation_name = match request.get("operationName") {
            None | Some(JsValue::Null | JsValue::Undefined) => None,
            Some(JsValue::String(name)) => Some(to_string_lossy(name).into_owned()),
            Some(_) => return Err(String::from("operationName is no string")),
        };
        Ok(Self {
            // A document text reaches the pipeline as a Rust string, so a
            // lone surrogate in it reads as the replacement character.
            document: to_string_lossy(document).into_owned(),
            variables: request.get("variables").cloned(),
            operation_name,
            validate: flag(request, "validate", true),
            trace: flag(request, "trace", false),
        })
    }

    /// Runs the call against the table `name` of `session`.
    ///
    /// Returns `None` when the session holds no such table.
    fn run(&self, session: &Session, name: &str) -> Option<Executed> {
        let request = Request {
            document: &self.document,
            variables: self.variables.clone(),
            operation_name: self.operation_name.as_deref(),
            validate: self.validate,
        };
        let options = AgentOptions {
            timers: timer_mode(),
            trace: self.trace,
        };
        in_window(|| session.execute(name, &request, options)).0
    }
}

/// Returns the boolean field `name` of `request`, `default` when it is absent.
fn flag(request: &JsObject, name: &str, default: bool) -> bool {
    match request.get(name) {
        Some(JsValue::Bool(value)) => *value,
        _ => default,
    }
}

/// Answers the operation `table`, which loads one behaviour table.
///
/// A table that breaks the grammar and a name the run holds already are
/// mistakes of the harness and answer `fatal`. A schema that does not build
/// and resolvers that name a missing type or field answer `errors` with the
/// message of the reference.
fn op_table(request: &JsObject) -> Result<JsObject, JsString> {
    let table = request
        .get("table")
        .ok_or_else(|| JsString::from("table: the request carries no field table"))?;
    let table = Table::from_js(table).map_err(|reason| format!("table: {reason}"))?;
    let name = table.name.clone();
    let mut payload = JsObject::new();
    match SESSION.with_borrow_mut(|session| session.load(table)) {
        Ok(()) => {
            payload.insert("name", name);
            payload.insert("ok", true);
        }
        Err(LoadError::Schema(error)) => {
            let errors = vec![seam::message_json(error.message().clone())];
            payload.insert("errors", JsValue::Array(errors));
        }
        Err(LoadError::NameTaken(name)) => {
            return Err(JsString::from(format!("table: {}", name_taken(&name))));
        }
    }
    Ok(payload)
}

/// Answers the operation `execute`, which runs one request against a loaded table.
fn op_execute(request: &JsObject) -> Result<JsObject, JsString> {
    let name = batch::require_string(request, "table")?;
    let call = Call::of(request).map_err(|reason| format!("{NAME}: {reason}"))?;
    begin_request();
    let executed = SESSION
        .with_borrow(|session| call.run(session, name))
        .ok_or_else(|| format!("{NAME}: {}", no_table(name)))?;
    payload(executed, call.trace)
}

/// Answers the operation `root`, which holds the factory of one root of a table with no schema.
///
/// A table that breaks the grammar or is of the mode `plain`, a root the
/// table does not name and a name the run holds already are mistakes of the
/// harness and answer `fatal`.
fn op_root(request: &JsObject) -> Result<JsObject, JsString> {
    let table = request
        .get("table")
        .ok_or_else(|| JsString::from("root: the request carries no field table"))?;
    let table = Table::from_js(table).map_err(|reason| format!("root: {reason}"))?;
    let type_name = batch::require_string(request, "root")?;
    let name = table.name.clone();
    let read = SESSION
        .with_borrow_mut(|session| session.hold(table, type_name))
        .map_err(|error| match error {
            HoldError::NameTaken(name) => format!("root: {}", name_taken(&name)),
            HoldError::NotService(name) => format!("root: the table {name} is of the mode plain"),
            HoldError::NoRoot(name) => format!("root: the table {name} names no root {type_name}"),
        })?;
    let mut payload = JsObject::new();
    payload.insert("name", name);
    payload.insert("read", count(read));
    Ok(payload)
}

/// Answers the operation `drop`, which drops a loaded table or a held root.
///
/// A name the run holds no table and no root of is a mistake of the harness
/// and answers `fatal`.
fn op_drop(request: &JsObject) -> Result<JsObject, JsString> {
    let name = batch::require_string(request, "table")?;
    if !SESSION.with_borrow_mut(|session| session.remove(name)) {
        return Err(JsString::from(format!("drop: {}", no_table(name))));
    }
    let mut payload = JsObject::new();
    payload.insert("name", name);
    payload.insert("dropped", true);
    Ok(payload)
}

/// Answers the operation `roots`, the entries of the table of root members of the thread.
// The signature is the one every operation of the batch form has.
#[expect(clippy::unnecessary_wraps)]
fn op_roots(_request: &JsObject) -> Result<JsObject, JsString> {
    let counts = root_member_counts();
    let mut payload = JsObject::new();
    payload.insert("live", count(counts.live));
    payload.insert("gone", count(counts.gone));
    Ok(payload)
}

/// The most errors the coercion of the variables reports, the `maxErrors` the
/// executor passes.
const MAX_VARIABLE_ERRORS: usize = 50;

/// Answers the operation `coerce`, which coerces the variables of one request both ways.
///
/// A document that does not parse, an operation the document does not hold
/// and a table the run never loaded are mistakes of the case and answer
/// `fatal`. Variables that are no object coerce as an empty object, as the
/// executor reads them.
fn op_coerce(request: &JsObject) -> Result<JsObject, JsString> {
    let name = batch::require_string(request, "table")?;
    let call = Call::of(request).map_err(|reason| format!("coerce: {reason}"))?;
    let document =
        aosabi_lang::parse(call.document.as_str(), ParseOptions::default()).map_err(|error| {
            let message = to_string_lossy(&error.message);
            format!("coerce: the document does not parse: {message}")
        })?;
    let operation = get_operation_ast(&document, call.operation_name.as_deref())
        .ok_or_else(|| JsString::from("coerce: the document holds no such operation"))?;
    let definitions = operation.variable_definitions.as_deref().unwrap_or(&[]);
    let inputs = match &call.variables {
        Some(JsValue::Object(inputs)) => inputs.clone(),
        _ => JsObject::new(),
    };
    let (owned, shared) = SESSION
        .with_borrow(|session| {
            let schema = session.schema(name)?;
            let owned = get_variable_values(
                schema,
                definitions,
                Cow::Owned(inputs.clone()),
                Some(MAX_VARIABLE_ERRORS),
            );
            let shared = get_variable_values(
                schema,
                definitions,
                Cow::Borrowed(&inputs),
                Some(MAX_VARIABLE_ERRORS),
            );
            Some((coerced_text(owned), coerced_text(shared)))
        })
        .ok_or_else(|| format!("coerce: {}", no_table(name)))?;
    let mut payload = JsObject::new();
    payload.insert("owned", owned);
    payload.insert("shared", shared);
    Ok(payload)
}

/// Returns the text of `{"coerced":..}` or of `{"errors":[..]}` of one coercion.
fn coerced_text(coerced: CoercedVariableValues) -> String {
    let value = match coerced {
        CoercedVariableValues::Coerced(coerced) => {
            let mut object = JsObject::new();
            object.insert("coerced", JsValue::Object(coerced));
            JsValue::Object(object)
        }
        CoercedVariableValues::Errors(errors) => ExecutionResult::from_errors(errors).to_js_value(),
    };
    aosabi_json::to_string(&value).unwrap_or_default()
}

/// Returns the payload of one run: `result` or `thrown`, and `trace` last.
fn payload(executed: Executed, trace: bool) -> Result<JsObject, JsString> {
    let mut payload = JsObject::new();
    match executed.outcome {
        Ok(result) => {
            payload.insert("result", result_text(&result));
        }
        Err(RunFailure::Thrown(thrown)) => {
            payload.insert("thrown", JsValue::Object(thrown_record(&thrown)));
        }
        Err(RunFailure::Stalled) => return Err(JsString::from(format!("{NAME}: {STALLED}"))),
    }
    if trace {
        payload.insert("trace", trace_json(&executed.trace));
    }
    Ok(payload)
}

/// Returns `{name, message}` of a thrown value.
///
/// The node side reads the two members off whatever was thrown, and
/// `JSON.stringify` leaves out a member that is `undefined`, so a thrown
/// value that is no error answers an empty object.
fn thrown_record(thrown: &Value) -> JsObject {
    let mut record = JsObject::new();
    if let Value::Error(error) = thrown {
        record.insert("name", error.name());
        record.insert("message", error.message());
    }
    record
}

/// Returns `{name, message}` of a thrown or rejected value, `thrownOf` of the
/// subscriptions of `execute.mjs` and `errorOf` of `pubsub.mjs`.
///
/// The name is the one of an error, or of an object whose `name` is a
/// string, and `Error` otherwise. The message is the one of an error and
/// `String(value)` of any other value. Unlike [`thrown_record`], a value that
/// is no error still answers both members.
pub(crate) fn error_record(thrown: &Value) -> JsObject {
    let (name, message) = match thrown {
        Value::Error(error) => (error.name(), error.message()),
        Value::Object(object) => {
            let name = match Rc::clone(object).get("name") {
                Value::String(name) => name,
                _ => JsString::from("Error"),
            };
            (name, to_js_string(thrown))
        }
        other => (JsString::from("Error"), to_js_string(other)),
    };
    let mut record = JsObject::new();
    record.insert("name", name);
    record.insert("message", message);
    record
}
