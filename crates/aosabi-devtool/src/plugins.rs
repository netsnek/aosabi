// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool plugins: the primitive through which a notebook sorts
 * plugins, gates them by role, boots them and remaps refusals in process.
 *
 * No analogue in the reference.
 */

//! The primitive `plugins` of the seam.
//!
//! The flag `--requests PATH` answers every request line of the file in the
//! batch form of [`crate::batch`], `id` first and `op` second, with no server.
//!
//! | `op` | Request fields | Payload |
//! | --- | --- | --- |
//! | `topo_sort` | `plugins`, `{name, depends_on}` each | `order`, the input indices, or `error`, the message of the cycle |
//! | `role_gate` | `plugins`, `{name, strategy, roles}` each | `ran`, the names whose setup ran |
//! | `boot` | `plugins`, `{name?, depends_on?, strategy?, roles?, setup?, middleware?}` each, `landing_page?`, `graphiql?`, `viewer?` | `role` and `passes`, or `thrown` and `cause` |
//! | `map_forbidden` | `errors`, `{message, path?, forbidden?, extensions?}` each | `errors` as `toJSON` writes them, or `unchanged` |
//!
//! `role_gate` and `boot` run [`execute_config`](fn@execute_config) over a
//! fresh `App::default()` with the built in plugins the facade hands the
//! boot, so the always on plugins take part as in a booted app. A label of
//! `boot` is the input index of a plugin of the line, or `{"builtin": <role>, "pass": <n>}`
//! for an always on plugin, since plugins are keyed by identity and names
//! collide. Both read the run role of the process environment, so a notebook
//! runs one process per role. A setup of `ok` answers at once,
//! `throw:<message>` throws `new Error(message)` and `throw-value:<text>`
//! throws the string, which the loader wraps.

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
    cell::RefCell,
    path::PathBuf,
    process::ExitCode,
    rc::Rc,
    sync::Arc, //
};

use aosabi_auth::{
    ForbiddenError,
    authz::map_forbidden_errors, //
};
use aosabi_core::{
    App,
    AppConfig,
    BootedApp,
    Builtins,
    ExecuteConfigArgs,
    Plugin,
    PluginList,
    PluginObject,
    PluginRole,
    PluginSlot,
    Role,
    Strategy,
    Thrown,
    app::execute_config::{
        PluginLabel,
        PluginRecord,
        SlotRecord, //
    },
    current_role,
    execute::errors::ThrownError,
    execute_config,
    plugin::object::__wrap,
    topo_sort_plugins, //
};
use aosabi_json::{
    JsObject,
    JsString,
    JsValue,
    Opaque, //
};
use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions,
    OriginalError,
    PathSegment, //
};

use crate::{
    batch::{
        self,
        Operations, //
    },
    cli::{
        self,
        Grammar,
        Matches, //
    },
    seam, //
};

/// The name of the primitive.
const NAME: &str = "plugins";

/// The milestone the primitive lands at.
const LANDS_AT: &str = "M7";

/// The most polls a pass may take, since every setup of a line answers at once.
const POLLS: usize = 1024;

/// The built in plugins the facade hands the boot, the viewer of the first
/// slot and the landing page.
const BUILTINS: Builtins = Builtins {
    first: None,
    viewer: aosabi_viewer::use_viewer,
    unhandled_route: aosabi_landing::use_unhandled_route,
};

/// The operations of the primitive.
const OPERATIONS: &Operations = &[
    ("topo_sort", op_topo_sort),
    ("role_gate", op_role_gate),
    ("boot", op_boot),
    ("map_forbidden", op_map_forbidden),
];

/// The command line of the `plugins` primitive.
pub static GRAMMAR: Grammar = Grammar {
    options: &[cli::SMOKE, cli::REQUESTS],
    positional: None,
    conflicts: &[],
};

/// What the `plugins` primitive accepts.
#[derive(Debug)]
pub struct PluginsArgs {
    /// Whether the call asks for the interface line.
    smoke: bool,
    /// The file of request lines of the batch form.
    requests: Option<PathBuf>,
}

impl PluginsArgs {
    /// Reads the arguments `matches` holds for [`GRAMMAR`].
    pub fn read(matches: &Matches) -> Self {
        Self {
            smoke: matches.flag("smoke"),
            requests: matches.path("requests"),
        }
    }
}

/// Runs the `plugins` primitive.
pub fn run(args: &PluginsArgs) -> ExitCode {
    if args.smoke {
        return seam::smoke_at(NAME, LANDS_AT);
    }
    let outcome = seam::required(args.requests.as_deref(), "--requests")
        .and_then(|requests| batch::run(OPERATIONS, requests));
    seam::finish(NAME, outcome)
}

/// Answers `topo_sort`, `topoSortPlugins(plugins)` as the input indices.
fn op_topo_sort(request: &JsObject) -> Result<JsObject, JsString> {
    let rows = array(request, "plugins")?;
    let mut slots = Vec::with_capacity(rows.len());
    for row in rows {
        let row = object(row, "topo_sort: a plugin is an object")?;
        let plugin = PluginObject {
            name: optional_string(row, "name", "topo_sort")?,
            depends_on: optional_strings(row, "depends_on", "topo_sort")?,
            ..PluginObject::default()
        };
        slots.push(PluginSlot::new(Rc::new(plugin)));
    }
    let mut payload = JsObject::new();
    match topo_sort_plugins(&slots) {
        Ok(sorted) => {
            let order = sorted.0.iter().map(|plugin| {
                let at = slots
                    .iter()
                    .position(|slot| slot.identity() == identity(plugin));
                at.map_or(JsValue::Null, |at| JsValue::from(index(at)))
            });
            payload.insert("order", JsValue::Array(order.collect()));
        }
        Err(error) => {
            payload.insert("error", error.message());
        }
    }
    Ok(payload)
}

/// Answers `role_gate`, the names whose setup the first pass ran under the
/// role of the process environment.
fn op_role_gate(request: &JsObject) -> Result<JsObject, JsString> {
    let ran: Rc<RefCell<Vec<String>>> = Rc::default();
    let mut slots = Vec::new();
    for row in array(request, "plugins")? {
        let row = object(row, "role_gate: a probe is an object with a string name")?;
        let Some(JsValue::String(name)) = row.get("name") else {
            return Err(JsString::from(
                "role_gate: a probe is an object with a string name",
            ));
        };
        let name = lossless(name, "role_gate")?;
        let recorded = Rc::clone(&ran);
        let label = name.clone();
        let plugin = PluginObject {
            name: Some(name),
            strategy: optional_strategy(row, "role_gate")?,
            roles: optional_roles(row, "role_gate")?,
            setup: __wrap::setup(move |_: &mut App| recorded.borrow_mut().push(label.clone())),
            ..PluginObject::default()
        };
        slots.push(PluginSlot::new(Rc::new(plugin)));
    }
    let mut config = AppConfig {
        plugins: Some(PluginList::new(slots)),
        ..AppConfig::default()
    };
    let mut target = App::default();
    let pass = execute_config(
        &mut config,
        ExecuteConfigArgs::default(),
        &mut target,
        &BUILTINS,
    );
    settle(pass)?.map_err(|error| JsString::from(error.to_string()))?;
    let names = ran
        .borrow()
        .iter()
        .map(|name| JsValue::from(name.as_str()))
        .collect();
    let mut payload = JsObject::new();
    payload.insert("ran", JsValue::Array(names));
    Ok(payload)
}

/// Answers `boot`, both passes of the boot over the plugins of the line.
fn op_boot(request: &JsObject) -> Result<JsObject, JsString> {
    let ran: Rc<RefCell<Vec<usize>>> = Rc::default();
    let mut slots = Vec::new();
    for (at, row) in array(request, "plugins")?.iter().enumerate() {
        let row = object(row, "boot: a plugin is an object")?;
        slots.push(PluginSlot::new(Rc::new(boot_plugin(row, at, &ran)?)));
    }
    let mut config = AppConfig {
        plugins: Some(PluginList::new(slots)),
        landing_page: optional_bool(request, "landing_page", "boot")?,
        graphiql: optional_bool(request, "graphiql", "boot")?,
        viewer: optional_bool(request, "viewer", "boot")?,
        ..AppConfig::default()
    };
    let mut target = App::default();
    let mut ran_per_pass = Vec::with_capacity(2);
    for strategy in [None, Some(Strategy::Last)] {
        ran.borrow_mut().clear();
        let args = ExecuteConfigArgs {
            plugins_strategy: strategy,
        };
        let pass = execute_config(&mut config, args, &mut target, &BUILTINS);
        if let Err(error) = settle(pass)? {
            let mut payload = JsObject::new();
            payload.insert("thrown", error.message.as_str());
            payload.insert(
                "cause",
                error.cause_line().map_or(JsValue::Null, JsValue::from),
            );
            return Ok(payload);
        }
        ran_per_pass.push(ran.borrow().clone());
    }
    let (booted, _listeners) = BootedApp::new(target);
    let mut pushed = 0;
    let mut passes = Vec::with_capacity(2);
    for (record, ran) in booted.boot_record().passes.iter().zip(&ran_per_pass) {
        let own: Vec<usize> = record
            .loaded
            .iter()
            .filter_map(|plugin| match plugin.label {
                PluginLabel::Declared(at) => Some(at),
                PluginLabel::Builtin(..) => None,
            })
            .collect();
        if own != *ran {
            return Err(JsString::from(format!(
                "boot: a pass ran the setups {ran:?}, its sort names {own:?}"
            )));
        }
        pushed += record.middleware.len();
        let mut pass = JsObject::new();
        pass.insert("strategy", strategy_name(record.strategy));
        let array = record.array.iter().map(slot_entry).collect();
        pass.insert("array", JsValue::Array(array));
        let sorted = record.sorted.iter().map(label).collect();
        pass.insert("sorted", JsValue::Array(sorted));
        let loaded = record.loaded.iter().map(label).collect();
        pass.insert("loaded", JsValue::Array(loaded));
        pass.insert("middleware", index(pushed));
        passes.push(JsValue::Object(pass));
    }
    let mut payload = JsObject::new();
    payload.insert("role", role_name(current_role()));
    payload.insert("passes", JsValue::Array(passes));
    Ok(payload)
}

/// Returns the plugin of the row `row` at input index `at` of a `boot` line.
///
/// Only the members the row names are present, because the members of a
/// plugin are what the boot report compares.
fn boot_plugin(
    row: &JsObject,
    at: usize,
    ran: &Rc<RefCell<Vec<usize>>>,
) -> Result<PluginObject, JsString> {
    let setup = match row.get("setup") {
        None | Some(JsValue::Null) => None,
        Some(JsValue::String(setup)) => {
            let setup = lossless(setup, "boot")?;
            if setup == "ok" {
                let recorded = Rc::clone(ran);
                __wrap::setup(move |_: &mut App| recorded.borrow_mut().push(at))
            } else if let Some(message) = setup.strip_prefix("throw:") {
                let message = String::from(message);
                __wrap::setup(move |_: &mut App| -> aosabi_core::Result<()> {
                    Err(Thrown::error(&message))
                })
            } else if let Some(text) = setup.strip_prefix("throw-value:") {
                let text = String::from(text);
                __wrap::setup(move |_: &mut App| -> aosabi_core::Result<()> {
                    Err(Thrown::value(text.as_str()))
                })
            } else {
                return Err(refusal_of_setup(&JsValue::String(JsString::from(setup))));
            }
        }
        Some(other) => return Err(refusal_of_setup(other)),
    };
    let middleware = match row.get("middleware") {
        Some(JsValue::Bool(true)) => {
            __wrap::middleware(|c, next: aosabi_core::Next| async move { next.run(c).await })
        }
        _ => None,
    };
    Ok(PluginObject {
        name: optional_string(row, "name", "boot")?,
        roles: optional_roles(row, "boot")?,
        strategy: optional_strategy(row, "boot")?,
        depends_on: optional_strings(row, "depends_on", "boot")?,
        setup,
        middleware,
        ..PluginObject::default()
    })
}

/// Returns the `fatal` of a setup that is none of the three forms.
fn refusal_of_setup(setup: &JsValue) -> JsString {
    let printed = aosabi_json::to_string(setup).unwrap_or_default();
    JsString::from(format!(
        "boot: a setup is ok, throw:<message> or throw-value:<text>, it is {printed}"
    ))
}

/// Answers `map_forbidden`, the errors of the line through the done hook of
/// the identity plugin.
fn op_map_forbidden(request: &JsObject) -> Result<JsObject, JsString> {
    const SHAPE: &str = "map_forbidden: an error is an object with a string message";
    let mut errors = Vec::new();
    for row in array(request, "errors")? {
        let row = object(row, SHAPE)?;
        let Some(JsValue::String(message)) = row.get("message") else {
            return Err(JsString::from(SHAPE));
        };
        let original_error = match row.get("forbidden") {
            None | Some(JsValue::Null) => None,
            Some(JsValue::Object(forbidden)) => Some(forbidden_original(forbidden)?),
            Some(_) => {
                return Err(JsString::from(
                    "map_forbidden: forbidden is an object or null",
                ));
            }
        };
        let path = match row.get("path") {
            None | Some(JsValue::Null | JsValue::Undefined) => None,
            Some(JsValue::Array(path)) => Some(path_of(path)?),
            Some(_) => return Err(JsString::from("map_forbidden: a path is an array")),
        };
        let extensions = match row.get("extensions") {
            Some(JsValue::Object(extensions)) => Some(extensions.clone()),
            _ => None,
        };
        errors.push(GraphQLError::new(
            message.clone(),
            GraphQLErrorOptions {
                path,
                original_error,
                extensions,
                ..GraphQLErrorOptions::default()
            },
        ));
    }
    let mut payload = JsObject::new();
    match map_forbidden_errors(&errors) {
        Some(mapped) => {
            let errors = mapped.iter().map(|error| JsValue::from(error.to_json()));
            payload.insert("errors", JsValue::Array(errors.collect()));
        }
        None => {
            payload.insert("unchanged", true);
        }
    }
    Ok(payload)
}

/// Returns the original error of a `forbidden` object, a `ForbiddenError` in
/// the host slot of a thrown error, as a resolver that throws one leaves it.
fn forbidden_original(forbidden: &JsObject) -> Result<OriginalError, JsString> {
    let message = optional_string(forbidden, "message", "map_forbidden")?;
    let feature = optional_string(forbidden, "feature", "map_forbidden")?;
    let mut error = message.map_or_else(ForbiddenError::new, ForbiddenError::with_message);
    if let Some(feature) = feature {
        error = error.with_feature(feature);
    }
    Ok(OriginalError::Other(Box::new(ThrownError {
        name: JsString::from(ForbiddenError::NAME),
        message: error.message.clone(),
        host: Some(Opaque(Arc::new(error))),
    })))
}

/// Returns the path segments of `path`, strings and integers.
fn path_of(path: &[JsValue]) -> Result<Vec<PathSegment>, JsString> {
    path.iter()
        .map(|segment| match segment {
            JsValue::String(key) => Ok(PathSegment::Key(lossless(key, "map_forbidden")?)),
            JsValue::Number(at) => whole(*at).map(PathSegment::Index).ok_or_else(|| {
                JsString::from("map_forbidden: a path segment is a string or an index")
            }),
            _ => Err(JsString::from(
                "map_forbidden: a path segment is a string or an index",
            )),
        })
        .collect()
}

/// Returns the entry `[label, strategy]` of an array of a pass.
fn slot_entry(slot: &SlotRecord) -> JsValue {
    let strategy = slot.strategy.map_or(JsValue::Null, |strategy| {
        JsValue::from(strategy_name(strategy))
    });
    JsValue::Array(vec![label(&slot.plugin), strategy])
}

/// Returns the label of `plugin`, its input index or its built in role and pass.
fn label(plugin: &PluginRecord) -> JsValue {
    match plugin.label {
        PluginLabel::Declared(at) => JsValue::from(index(at)),
        PluginLabel::Builtin(builtin, pass) => {
            let mut label = JsObject::new();
            label.insert("builtin", builtin.as_str());
            label.insert("pass", index(pass));
            JsValue::Object(label)
        }
    }
}

/// Returns the name of `strategy`.
const fn strategy_name(strategy: Strategy) -> &'static str {
    match strategy {
        Strategy::First => "first",
        Strategy::Last => "last",
    }
}

/// Returns the name of the run role `role`, `currentRole()`.
const fn role_name(role: Role) -> &'static str {
    match role {
        Role::Web => "web",
        Role::Worker => "worker",
        Role::All => "all",
    }
}

/// Polls `future` to its end with no waker, or gives it up after [`POLLS`].
fn settle<F: Future>(future: F) -> Result<F::Output, JsString> {
    let mut future = pin!(future);
    let mut cx = Context::from_waker(Waker::noop());
    for _ in 0..POLLS {
        if let Poll::Ready(value) = future.as_mut().poll(&mut cx) {
            return Ok(value);
        }
    }
    Err(JsString::from("a pass of the boot did not settle"))
}

/// Returns the identity of `plugin`, the address of its value.
fn identity(plugin: &Rc<dyn Plugin>) -> *const () {
    Rc::as_ptr(plugin).cast::<()>()
}

/// Returns `at` as a JSON number.
fn index(at: usize) -> f64 {
    // An index of a line holds far fewer than 2^32 entries.
    f64::from(u32::try_from(at).unwrap_or(u32::MAX))
}

/// Returns the double `at` as an index of 0 to 2^32 - 1, or `None` when it is
/// not one.
fn whole(at: f64) -> Option<usize> {
    if !(at.fract() == 0.0 && (0.0..=f64::from(u32::MAX)).contains(&at)) {
        return None;
    }
    // The cast is exact, because the value is a whole number of 0 to 2^32 - 1,
    // which a double carries without a gap.
    #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let at = at as u32;
    usize::try_from(at).ok()
}

/// Returns the array field `name` of `request`.
fn array<'a>(request: &'a JsObject, name: &str) -> Result<&'a [JsValue], JsString> {
    if let Some(JsValue::Array(values)) = request.get(name) {
        return Ok(values);
    }
    let op = match request.get("op") {
        Some(JsValue::String(op)) => op.as_str().unwrap_or("?"),
        _ => "?",
    };
    Err(JsString::from(format!(
        "{op}: the request carries no array field {name}"
    )))
}

/// Returns `value` as an object, or the `fatal` text `refusal`.
fn object<'a>(value: &'a JsValue, refusal: &str) -> Result<&'a JsObject, JsString> {
    match value {
        JsValue::Object(object) => Ok(object),
        _ => Err(JsString::from(refusal)),
    }
}

/// Returns `text` as a Rust string, or the `fatal` of a lone surrogate.
fn lossless(text: &JsString, op: &str) -> Result<String, JsString> {
    text.as_str().map(String::from).ok_or_else(|| {
        JsString::from(format!(
            "{op}: a string carries a lone surrogate, which a Rust string cannot hold"
        ))
    })
}

/// Returns the field `name` of `row` as a string, `None` when it is absent or null.
fn optional_string(row: &JsObject, name: &str, op: &str) -> Result<Option<String>, JsString> {
    match row.get(name) {
        None | Some(JsValue::Null | JsValue::Undefined) => Ok(None),
        Some(JsValue::String(text)) => lossless(text, op).map(Some),
        Some(other) => {
            let printed = aosabi_json::to_string(other).unwrap_or_default();
            Err(JsString::from(format!(
                "{op}: {name} is a string or null, it is {printed}"
            )))
        }
    }
}

/// Returns the field `name` of `row` as a list of strings, `None` when it is absent or null.
fn optional_strings(row: &JsObject, name: &str, op: &str) -> Result<Option<Vec<String>>, JsString> {
    let refusal = |value: &JsValue| {
        let printed = aosabi_json::to_string(value).unwrap_or_default();
        JsString::from(format!(
            "{op}: {name} is a list of strings or null, it is {printed}"
        ))
    };
    match row.get(name) {
        None | Some(JsValue::Null | JsValue::Undefined) => Ok(None),
        Some(JsValue::Array(values)) => values
            .iter()
            .map(|value| match value {
                JsValue::String(text) => lossless(text, op),
                other => Err(refusal(other)),
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Some),
        Some(other) => Err(refusal(other)),
    }
}

/// Returns the field `roles` of `row`, `None` when it is absent or null.
fn optional_roles(row: &JsObject, op: &str) -> Result<Option<Vec<PluginRole>>, JsString> {
    let Some(roles) = optional_strings(row, "roles", op)? else {
        return Ok(None);
    };
    roles
        .iter()
        .map(|role| match role.as_str() {
            "web" => Ok(PluginRole::Web),
            "worker" => Ok(PluginRole::Worker),
            other => Err(JsString::from(format!(
                "{op}: a role is web or worker, it is {other:?}"
            ))),
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

/// Returns the field `strategy` of `row`, `None` when it is absent or null.
fn optional_strategy(row: &JsObject, op: &str) -> Result<Option<Strategy>, JsString> {
    match optional_string(row, "strategy", op)?.as_deref() {
        None => Ok(None),
        Some("first") => Ok(Some(Strategy::First)),
        Some("last") => Ok(Some(Strategy::Last)),
        Some(other) => Err(JsString::from(format!(
            "{op}: a strategy is first or last, it is {other:?}"
        ))),
    }
}

/// Returns the field `name` of `row` as a boolean, `None` when it is absent or null.
fn optional_bool(row: &JsObject, name: &str, op: &str) -> Result<Option<bool>, JsString> {
    match row.get(name) {
        None | Some(JsValue::Null | JsValue::Undefined) => Ok(None),
        Some(JsValue::Bool(value)) => Ok(Some(*value)),
        Some(_) => Err(JsString::from(format!("{op}: {name} is a boolean"))),
    }
}
