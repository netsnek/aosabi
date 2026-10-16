// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool yoga: the primitive through which a notebook sends a request
 * to the Yoga of graphql-over-http in process, with no socket and no base
 * pipeline.
 *
 * No analogue in the reference.
 */

//! The primitive `yoga` of the seam.
//!
//! The flag `--requests PATH` answers every request line of the file, one
//! response line each. A request line is
//! `{"id", "instance", "method", "url", "headers", "body"}`: the name of an
//! instance of [`instances`], the method, the URL, whole or as a path that
//! the origin `http://localhost` completes, the headers as an object or as an
//! array of name and value pairs in order, and the body as text, or
//! `body_base64` in its place for bytes. A response line is
//! `{"id", "status", "status_text", "headers", "body_base64"}` with the
//! headers as pairs in the order the response carries them, or
//! `{"id", "error"}` for an instance the primitive does not know, a line it
//! cannot read, or a request Yoga rejects.
//!
//! A resolver of the capture schemas answers at once, so the fetch settles
//! within polls of the future with no waker and no clock.
//!
//! The flag `--single-pass on` or `--single-pass off` lays the field
//! `single_pass` of the options of Yoga over every instance, adds the probe
//! instances of [`pass_probe`], and names on each response line the verdicts
//! the engine counted for its request under the key `single_pass`:
//! `{"accepted", "refused", "deferred", "reasons", "gates"}`, the validations
//! the pass accepted, refused and deferred, the deferred ones by the name of
//! their reason where there are any, and the validations each gate from 1 to 5
//! sent to the rules before the pass ran. A request that met no validate
//! function, such as a hit of the request cache or a text that does not parse,
//! counts nothing. The flag `--parser-and-validation-cache off` leaves the
//! request cache of Yoga out of the nine capture instances, and `on` keeps it
//! as Yoga installs it. Without either flag every instance is the one of
//! [`instances`] and every response line is the one above.
//!
//! The flag `--graphtp DIR` serves every instance over `GraphTP(Homa)` in
//! place of a file of request lines: each instance booted as an app, its fetch
//! behind the `GraphTP` listener of `aosabi-core` on a `SOCK_SEQPACKET` socket
//! and a stream socket of prefixed `AoFrame` in `DIR`, until the file
//! `DIR/stop` stands and every connection has closed, and one line of what it
//! served. The flag needs the Unix Socket transports of `homa`, which build on
//! Linux alone, and refuses elsewhere.

pub mod base64;
pub mod capture_schema;
pub mod instances;
pub mod pass_probe;
#[cfg(target_os = "linux")]
pub mod serve;

/// The flag `--graphtp` where the Unix Socket transports of `homa` do not
/// build, which refuses.
#[cfg(not(target_os = "linux"))]
pub mod serve {
    use std::path::Path;

    use crate::seam::{
        Answer,
        Failure, //
    };

    /// Refuses, since the Unix Socket transports of `homa` build on Linux
    /// alone.
    ///
    /// # Errors
    ///
    /// Always returns [`Failure::Usage`].
    pub fn serve(_dir: &Path) -> Result<Answer, Failure> {
        Err(Failure::Usage(String::from(
            "--graphtp needs the Unix Socket transports of homa, which build on Linux alone",
        )))
    }
}

use core::{
    future::Future,
    pin::pin,
    task::{
        Context,
        Poll,
        Waker, //
    },
};
use std::{
    path::PathBuf,
    process::ExitCode,
    rc::Rc, //
};

use aosabi_hooks::plugins::use_engine::{
    PassTally,
    REASON_ORDER, //
};
use aosabi_http::{
    Body,
    Env,
    Headers,
    Request, //
};
use aosabi_json::{
    JsObject,
    JsValue, //
};
use graphql_over_http::ServerContext;

use crate::{
    cli::{
        self,
        Choice,
        Grammar,
        Matches,
        Opt, //
    },
    json::reader,
    seam::{
        self,
        Answer,
        Failure, //
    },
};

use self::instances::{
    Instances,
    Switches, //
};

/// The name of the primitive.
const NAME: &str = "yoga";

/// The milestone the primitive lands at.
const LANDS_AT: &str = "M5";

/// The origin a URL given as a path is completed with.
const ORIGIN: &str = "http://localhost";

/// The polls a fetch may take before the primitive gives it up.
const POLLS: usize = 1_000_000;

/// The option that serves every instance over `GraphTP(Homa)` on Unix
/// Sockets in a directory.
const GRAPHTP: Opt = Opt::path(
    "graphtp",
    "DIR",
    "Serve every instance over GraphTP(Homa) on Unix Sockets in this directory until DIR/stop stands",
);

/// The option that lays the single pass, on or off, over every instance and
/// names the verdicts of each request on its response line.
const SINGLE_PASS: Opt = Opt::choice(
    "single-pass",
    "SWITCH",
    "Validate every instance through the single pass or the rules alone, add the probe instances and name the verdicts of each request",
    &PASS_SWITCH,
);

/// The values of `--single-pass`, in the order of the help.
const PASS_SWITCH: [Choice; 2] = [
    Choice {
        name: "on",
        help: "The single pass, with the rules for every document it does not accept",
    },
    Choice {
        name: "off",
        help: "The rules alone, each validation counted at the gate of the option",
    },
];

/// The option that keeps or leaves out the request cache of Yoga on the nine
/// capture instances.
const CACHE: Opt = Opt::choice(
    "parser-and-validation-cache",
    "SWITCH",
    "Keep the request cache of Yoga on the capture instances or leave it out",
    &CACHE_SWITCH,
);

/// The values of `--parser-and-validation-cache`, in the order of the help.
const CACHE_SWITCH: [Choice; 2] = [
    Choice {
        name: "on",
        help: "The request cache as Yoga installs it",
    },
    Choice {
        name: "off",
        help: "No request cache, parserAndValidationCache: false",
    },
];

/// The command line of the `yoga` primitive.
pub static GRAMMAR: Grammar = Grammar {
    options: &[cli::SMOKE, cli::REQUESTS, GRAPHTP, SINGLE_PASS, CACHE],
    positional: None,
    conflicts: &[(
        "graphtp",
        &["requests", "single-pass", "parser-and-validation-cache"],
    )],
};

/// What the `yoga` primitive accepts.
#[derive(Debug)]
pub struct YogaArgs {
    /// Whether the call asks for the interface line.
    smoke: bool,
    /// The file of request lines of the batch form.
    requests: Option<PathBuf>,
    /// The directory of the sockets of `--graphtp`.
    graphtp: Option<PathBuf>,
    /// The choices the command line lays over every instance.
    switches: Switches,
}

impl YogaArgs {
    /// Reads the arguments `matches` holds for [`GRAMMAR`].
    pub fn read(matches: &Matches) -> Self {
        Self {
            smoke: matches.flag("smoke"),
            requests: matches.path("requests"),
            graphtp: matches.path("graphtp"),
            switches: Switches {
                single_pass: switch(matches, "single-pass"),
                cache: switch(matches, "parser-and-validation-cache"),
            },
        }
    }
}

/// Returns whether the switch `long` is on, `None` where the command line
/// names none.
///
/// The parser refuses a value of an option of choices outside its choices
/// before the primitive runs, so the value is `on` or `off`.
fn switch(matches: &Matches, long: &str) -> Option<bool> {
    matches.text(long).ok().flatten().map(|value| value == "on")
}

/// Runs the `yoga` primitive.
pub fn run(args: &YogaArgs) -> ExitCode {
    if args.smoke {
        return seam::smoke_at(NAME, LANDS_AT);
    }
    if let Some(dir) = args.graphtp.as_deref() {
        return seam::finish(NAME, serve::serve(dir));
    }
    let outcome = seam::required(args.requests.as_deref(), "--requests")
        .and_then(seam::read_bytes)
        .and_then(|bytes| answer_all(&bytes, args.switches));
    seam::finish(NAME, outcome)
}

/// Answers every request line of `bytes` through the instances of `switches`.
fn answer_all(bytes: &[u8], switches: Switches) -> Result<Answer, Failure> {
    let instances = Instances::with(switches).map_err(Failure::Data)?;
    let mut out = Vec::new();
    for line in bytes.split(|&byte| byte == b'\n') {
        if line.trim_ascii().is_empty() {
            continue;
        }
        aosabi_json::write(&mut out, &JsValue::Object(answer(&instances, line)));
        out.push(b'\n');
    }
    Ok(Answer::Payload(out))
}

/// Returns the response line of one request line.
fn answer(instances: &Instances, line: &[u8]) -> JsObject {
    let request = match reader::parse(line) {
        Ok(JsValue::Object(request)) => request,
        Ok(_) => return error(JsValue::Null, "the request line is not an object"),
        Err(failure) => {
            return error(
                JsValue::Null,
                &format!("the request line is not JSON: {failure}"),
            );
        }
    };
    let id = request.get("id").cloned().unwrap_or(JsValue::Null);
    respond(instances, &request, id.clone()).unwrap_or_else(|reason| error(id, &reason))
}

/// Returns the response line `{"id", "error"}`.
fn error(id: JsValue, reason: &str) -> JsObject {
    let mut line = JsObject::with_capacity(2);
    line.insert("id", id);
    line.insert("error", reason);
    line
}

/// Sends `request` to its instance and returns the response line with `id`.
fn respond(instances: &Instances, request: &JsObject, id: JsValue) -> Result<JsObject, String> {
    let name = text(request, "instance")?.unwrap_or_else(|| String::from("prod"));
    let yoga = instances
        .get(&name)
        .ok_or_else(|| format!("no instance is named {name}"))?;
    let method = text(request, "method")?.unwrap_or_else(|| String::from("GET"));
    let url = text(request, "url")?.unwrap_or_else(|| String::from("/graphql"));
    let url = if url.starts_with('/') {
        format!("{ORIGIN}{url}")
    } else {
        url
    };
    let counts = instances.counts(&name);
    let before = counts.map(|counts| counts.get());
    let headers = headers(request.get("headers"))?;
    let body = match (text(request, "body")?, text(request, "body_base64")?) {
        (_, Some(coded)) => {
            let bytes = base64::decode(&coded).ok_or("body_base64 is not Base64")?;
            Body::Full(Rc::from(bytes))
        }
        (Some(body), None) => Body::Full(Rc::from(body.into_bytes())),
        (None, None) => Body::Empty,
    };
    let request = Rc::new(Request::new(method, url, headers, body));
    let context = ServerContext {
        env: Env::Process,
        execution_ctx: None,
    };
    let response = settle(graphql_over_http::answer(
        yoga,
        request,
        context.into_context(),
    ))?
    .map_err(|thrown| format!("Yoga rejected the request: {thrown:?}"))?;
    let body =
        settle(response.bytes())?.map_err(|failure| format!("the body failed: {failure:?}"))?;
    let pairs = response
        .headers()
        .list()
        .iter()
        .map(|(name, value)| {
            JsValue::Array(vec![
                JsValue::from(name.as_str()),
                JsValue::from(value.as_str()),
            ])
        })
        .collect();
    let mut line = JsObject::with_capacity(5);
    line.insert("id", id);
    line.insert("status", u32::from(response.status()));
    line.insert("status_text", response.status_text());
    line.insert("headers", JsValue::Array(pairs));
    line.insert("body_base64", base64::encode(&body));
    if let (Some(counts), Some(before)) = (counts, before) {
        line.insert("single_pass", verdicts(&counts.get().since(&before)));
    }
    Ok(line)
}

/// Returns the verdicts of one request, the key `single_pass` of its
/// response line.
fn verdicts(tally: &PassTally) -> JsObject {
    let mut reasons = JsObject::new();
    for reason in REASON_ORDER {
        let count = tally.deferred_for(reason);
        if count > 0 {
            reasons.insert(reason.name(), count);
        }
    }
    let deferred = tally
        .deferred
        .iter()
        .fold(0u64, |sum, count| sum.saturating_add(*count));
    let gates = tally.closed.iter().map(|count| JsValue::from(*count));
    let mut object = JsObject::with_capacity(5);
    object.insert("accepted", tally.accepted);
    object.insert("refused", tally.refused);
    object.insert("deferred", deferred);
    object.insert("reasons", reasons);
    object.insert("gates", JsValue::Array(gates.collect()));
    object
}

/// Returns the text of the key `key`, `None` when it is absent or null.
fn text(request: &JsObject, key: &str) -> Result<Option<String>, String> {
    match request.get(key) {
        None | Some(JsValue::Null) => Ok(None),
        Some(JsValue::String(value)) => value
            .as_str()
            .map(|value| Some(value.to_owned()))
            .ok_or_else(|| format!("{key} is no well formed text")),
        Some(_) => Err(format!("{key} is not a string")),
    }
}

/// Returns the headers of a request line, an object or an array of pairs.
fn headers(value: Option<&JsValue>) -> Result<Headers, String> {
    let mut headers = Headers::new();
    let mut append = |name: &JsValue, value: &JsValue| -> Result<(), String> {
        let (JsValue::String(name), JsValue::String(value)) = (name, value) else {
            return Err(String::from("a header is not a pair of strings"));
        };
        let (Some(name), Some(value)) = (name.as_str(), value.as_str()) else {
            return Err(String::from("a header is no well formed text"));
        };
        headers
            .append(name, value)
            .map_err(|failure| format!("the header {name} is refused: {failure:?}"))
    };
    match value {
        None | Some(JsValue::Null) => {}
        Some(JsValue::Object(object)) => {
            for (name, value) in object.iter() {
                append(&JsValue::String(name.clone()), value)?;
            }
        }
        Some(JsValue::Array(pairs)) => {
            for pair in pairs {
                let JsValue::Array(pair) = pair else {
                    return Err(String::from("a header is not a pair"));
                };
                let [name, value] = pair.as_slice() else {
                    return Err(String::from("a header is not a pair"));
                };
                append(name, value)?;
            }
        }
        Some(_) => return Err(String::from("headers is neither an object nor an array")),
    }
    Ok(headers)
}

/// Polls `future` to its end with no waker, or gives it up after [`POLLS`].
fn settle<F: Future>(future: F) -> Result<F::Output, String> {
    let mut future = pin!(future);
    let mut cx = Context::from_waker(Waker::noop());
    for _ in 0..POLLS {
        if let Poll::Ready(value) = future.as_mut().poll(&mut cx) {
            return Ok(value);
        }
    }
    Err(String::from("the fetch did not settle"))
}
