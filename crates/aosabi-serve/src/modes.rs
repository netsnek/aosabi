// SPDX-License-Identifier: MIT
/*
 * aosabi-serve: the harness modes of a served binary.
 *
 * No analogue on the reference side.
 */

//! The harness modes.
//!
//! The variable `AOSABI_BOOT_REPORT` names a file the boot writes one JSON
//! document into after its last pass: the plugins of the configuration in their
//! order, each with the text and the line of its element of `plugins!` or the
//! file and the line of the call of `.plugin(..)` of the builder of an app that
//! registered it, its name, its strategy and its roles, and every route with
//! its method, its path, its base path and the line of its registration, which
//! check C4 of `12_authoring_parity` and the family R read.
//!
//! The report also carries what the passes of the boot did, in the fields the
//! reference half `tests/sidecar/boot-report.mjs` writes and those its
//! operation `boot` answers: `entry`, `server` or `worker`, and `role`, the run
//! role of the passes, `config_plugins`, the plugins of the configuration as
//! declared, `config_plugins_after_boot`, the array the last pass wrote back,
//! which holds the always on plugins twice by quirk
//! `Q01_DUPLICATE_PLUGIN_WRITEBACK`, `passes`, one entry per pass with its
//! strategy, its role, its array with the strategy written back and its sorted,
//! loaded and middleware plugins, `middleware`, the names of the plugins whose
//! middleware the loader pushed in that order, and `gates`, the app and the
//! line of every `.gate(...)` of the tree. A plugin is labelled by its
//! identity, its index in the configuration as the boot found it or, for an
//! always on plugin, `{"builtin": <role>, "pass": <n>}`, because names collide
//! and the last one wins, and its members are those of `Plugin::members`.

use std::{
    ffi::OsStr,
    fs,
    io::{
        self,
        Write as _, //
    },
    path::Path,
    process::ExitCode,
};

use aosabi_core::{
    BootError,
    BootedApp,
    IntoApp as _,
    IntoEntry,
    PluginRole,
    PluginSlot,
    ResolverMap,
    Role,
    Service,
    Strategy,
    app::{
        env_alias,
        execute_config::{
            PassRecord,
            PluginLabel,
            PluginRecord,
            SlotRecord, //
        }, //
    },
    glue::resolve_type::type_resolver,
    plugin::PluginSource,
    request_stream::answer_requests as answer_lines, //
};
use aosabi_json::{
    JsObject,
    JsValue, //
};

use crate::{
    build_step::config_abort,
    serve::BuildOutput, //
};

/// The variable that names the file of the boot report.
pub const BOOT_REPORT_ENV: &str = "AOSABI_BOOT_REPORT";

/// The usage line of a served binary.
pub const USAGE: &str = "usage: [--print-schema | --requests PATH]";

/// The exit code of a wrong call, `EX_USAGE` of sysexits.h.
pub const EX_USAGE: u8 = 64;

/// The exit code of a request file that cannot be read, `EX_NOINPUT`.
pub const EX_NOINPUT: u8 = 66;

/// The exit code of a failure of the software, `EX_SOFTWARE`.
pub const EX_SOFTWARE: u8 = 70;

/// The exit code of an output that cannot be written, `EX_IOERR`.
pub const EX_IOERR: u8 = 74;

/// Prints the schema of `build` and answers 0.
#[must_use]
pub fn print_schema(build: &BuildOutput) -> ExitCode {
    emit(build.schema.as_bytes())
}

/// Answers the request lines of the file `path` through a service over the
/// roots of `entry`.
///
/// An entry that answers an error ends with the abort of a configuration that
/// throws at load, [`config_abort`], as the boot does.
#[must_use]
pub fn answer_requests<E: IntoEntry>(
    entry: fn() -> E,
    build: &BuildOutput,
    path: &OsStr,
) -> ExitCode {
    let app = match entry().into_entry() {
        Ok(app) => app.into_app(),
        Err(cause) => return config_abort(&cause),
    };
    let mut resolvers = ResolverMap::default();
    for rule in build.resolve_types {
        resolvers
            .resolve_types
            .insert(String::from(rule.type_name), type_resolver(rule));
    }
    let service = match Service::builder(build.schema)
        .roots(app.graphql().clone())
        .resolvers(resolvers)
        .build()
    {
        Ok(service) => service,
        Err(error) => {
            eprintln!("{}", BootError::Schema(error));
            return ExitCode::from(EX_SOFTWARE);
        }
    };
    match fs::read(path) {
        Ok(requests) => emit(&answer_lines(&service, &requests)),
        Err(error) => {
            eprintln!("{}: {error}", Path::new(path).display());
            ExitCode::from(EX_NOINPUT)
        }
    }
}

/// Tells whether the boot binds the port itself, the development variant of
/// the harness.
#[must_use]
pub fn dev_serving() -> bool {
    env_alias::var("NODE_ENV").as_deref() == Some("development")
        && env_alias::var("AOSABI_DEV").as_deref() == Some("1")
}

/// Writes the boot report of `app` into the file `AOSABI_BOOT_REPORT` names,
/// and does nothing when it names none.
///
/// # Errors
///
/// Returns the error of the write.
pub fn write_boot_report(app: &BootedApp) -> io::Result<()> {
    let Some(path) = std::env::var_os(BOOT_REPORT_ENV).filter(|path| !path.is_empty()) else {
        return Ok(());
    };
    let mut out = Vec::new();
    aosabi_json::write(&mut out, &JsValue::Object(boot_report(app)));
    out.push(b'\n');
    fs::write(path, out)
}

/// Returns the boot report of `app`.
#[must_use]
pub fn boot_report(app: &BootedApp) -> JsObject {
    let plugins = app
        .config()
        .and_then(|config| config.plugins.as_ref())
        .map(|list| list.slots().iter().map(plugin_entry).collect())
        .unwrap_or_default();
    let routes = app
        .routes()
        .iter()
        .map(|route| {
            let mut entry = JsObject::new();
            entry.insert("method", JsValue::from(route.method.as_str()));
            entry.insert("path", JsValue::from(route.path.as_str()));
            entry.insert("basePath", JsValue::from(route.base_path.as_str()));
            entry.insert("file", JsValue::from(route.registered_at.file()));
            entry.insert("line", JsValue::from(route.registered_at.line()));
            JsValue::Object(entry)
        })
        .collect();
    let record = app.boot_record();
    let role = record.passes.first().map_or(Role::Web, |pass| pass.role);
    let entry = if role == Role::Worker {
        "worker"
    } else {
        "server"
    };
    let middleware = record
        .passes
        .iter()
        .flat_map(|pass| &pass.middleware)
        .map(|plugin| plugin.name.as_deref().map_or(JsValue::Null, JsValue::from))
        .collect();
    let gates = app
        .gates()
        .iter()
        .map(|gate| {
            let mut entry = JsObject::new();
            entry.insert(
                "app",
                gate.app.as_deref().map_or(JsValue::Null, JsValue::from),
            );
            entry.insert("file", JsValue::from(gate.registered_at.file()));
            entry.insert("line", JsValue::from(gate.registered_at.line()));
            JsValue::Object(entry)
        })
        .collect();
    let mut report = JsObject::new();
    report.insert("plugins", JsValue::Array(plugins));
    report.insert("routes", JsValue::Array(routes));
    report.insert("entry", JsValue::from(entry));
    report.insert("role", JsValue::from(role_name(role)));
    report.insert("config_plugins", JsValue::Array(declared_plugins(app)));
    report.insert(
        "config_plugins_after_boot",
        JsValue::Array(record.after_boot.iter().map(slot_entry).collect()),
    );
    report.insert(
        "passes",
        JsValue::Array(record.passes.iter().map(pass_entry).collect()),
    );
    report.insert("middleware", JsValue::Array(middleware));
    report.insert("gates", JsValue::Array(gates));
    report
}

/// Returns the plugins of the configuration as declared, each labelled by its
/// index, with the strategy it states before the boot writes one back.
///
/// The array the boot writes back holds the plugins of the configuration as
/// one run in the order of their indices, between the always on plugins of
/// both passes, so the walk finds them in order.
fn declared_plugins(app: &BootedApp) -> Vec<JsValue> {
    let record = app.boot_record();
    let Some(list) = app.config().and_then(|config| config.plugins.as_ref()) else {
        return Vec::new();
    };
    list.slots()
        .iter()
        .filter_map(|slot| {
            let label @ PluginLabel::Declared(_) = record.label_of(slot)? else {
                return None;
            };
            let plugin = slot.plugin.as_ref();
            let members = plugin.members();
            let depends_on = members.contains(&"dependsOn").then(|| plugin.depends_on());
            Some(plugin_entry_of(
                label,
                plugin.name(),
                plugin.strategy(),
                plugin.roles(),
                depends_on,
                &members,
            ))
        })
        .collect()
}

/// Returns the entry of one pass of the boot.
fn pass_entry(pass: &PassRecord) -> JsValue {
    let plugins = |list: &[PluginRecord]| JsValue::Array(list.iter().map(plugin_record).collect());
    let mut entry = JsObject::new();
    entry.insert("strategy", strategy_value(Some(pass.strategy)));
    entry.insert("role", JsValue::from(role_name(pass.role)));
    entry.insert(
        "array",
        JsValue::Array(pass.array.iter().map(slot_entry).collect()),
    );
    entry.insert("sorted", plugins(&pass.sorted));
    entry.insert("loaded", plugins(&pass.loaded));
    entry.insert("middleware", plugins(&pass.middleware));
    JsValue::Object(entry)
}

/// Returns the entry of one plugin of an array, `{label, name, strategy,
/// roles, depends_on, members}`.
fn slot_entry(slot: &SlotRecord) -> JsValue {
    plugin_entry_of(
        slot.plugin.label,
        slot.plugin.name.as_deref(),
        slot.strategy,
        slot.roles.as_deref(),
        slot.depends_on.as_deref(),
        &slot.plugin.members,
    )
}

/// Returns the entry `{label, name, strategy, roles, depends_on, members}` of
/// a plugin, as the reference half of the boot report writes it.
fn plugin_entry_of(
    label: PluginLabel,
    name: Option<&str>,
    strategy: Option<Strategy>,
    roles: Option<&[PluginRole]>,
    depends_on: Option<&[String]>,
    members: &[&str],
) -> JsValue {
    let mut entry = JsObject::new();
    entry.insert("label", label_value(label));
    entry.insert("name", name.map_or(JsValue::Null, JsValue::from));
    entry.insert("strategy", strategy_value(strategy));
    entry.insert("roles", roles_value(roles));
    entry.insert("depends_on", names_value(depends_on));
    entry.insert("members", members_value(members));
    JsValue::Object(entry)
}

/// Returns the entry of one plugin of an order, `{label, name, members}`.
fn plugin_record(plugin: &PluginRecord) -> JsValue {
    let mut entry = JsObject::new();
    entry.insert("label", label_value(plugin.label));
    entry.insert(
        "name",
        plugin.name.as_deref().map_or(JsValue::Null, JsValue::from),
    );
    entry.insert("members", members_value(&plugin.members));
    JsValue::Object(entry)
}

/// Returns a label as the report writes it, the index of a declared plugin or
/// `{"builtin": <role>, "pass": <n>}`.
fn label_value(label: PluginLabel) -> JsValue {
    match label {
        PluginLabel::Declared(index) => count_value(index),
        PluginLabel::Builtin(builtin, pass) => {
            let mut entry = JsObject::new();
            entry.insert("builtin", JsValue::from(builtin.as_str()));
            entry.insert("pass", count_value(pass));
            JsValue::Object(entry)
        }
    }
}

/// Returns an index or a count as a JSON number.
#[expect(
    clippy::cast_precision_loss,
    reason = "an index of the plugin array and a count of passes stay far below 2^53"
)]
fn count_value(count: usize) -> JsValue {
    JsValue::from(count as f64)
}

/// Returns a strategy as the report writes it, `null` for none.
fn strategy_value(strategy: Option<Strategy>) -> JsValue {
    strategy.map_or(JsValue::Null, |strategy| {
        JsValue::from(match strategy {
            Strategy::First => "first",
            Strategy::Last => "last",
        })
    })
}

/// Returns the run roles of a plugin as the report writes them, `null` for none.
fn roles_value(roles: Option<&[PluginRole]>) -> JsValue {
    roles.map_or(JsValue::Null, |roles| {
        JsValue::Array(
            roles
                .iter()
                .map(|role| {
                    JsValue::from(match role {
                        PluginRole::Web => "web",
                        PluginRole::Worker => "worker",
                    })
                })
                .collect(),
        )
    })
}

/// Returns a list of names as the report writes it, `null` for none.
fn names_value(names: Option<&[String]>) -> JsValue {
    names.map_or(JsValue::Null, |names| {
        JsValue::Array(
            names
                .iter()
                .map(|name| JsValue::from(name.as_str()))
                .collect(),
        )
    })
}

/// Returns the members of a plugin as an array.
fn members_value(members: &[&str]) -> JsValue {
    JsValue::Array(
        members
            .iter()
            .map(|member| JsValue::from(*member))
            .collect(),
    )
}

/// Returns the run role of the process as the report writes it.
fn role_name(role: Role) -> &'static str {
    match role {
        Role::Web => "web",
        Role::Worker => "worker",
        Role::All => "all",
    }
}

/// Returns the entry of the report of one plugin of the configuration.
///
/// A plugin of `plugins!` names the text and the line of its element and no
/// file, and a plugin of the builder of an app names no text and the file and
/// the line of its call.
fn plugin_entry(slot: &PluginSlot) -> JsValue {
    let mut entry = JsObject::new();
    let (source, file, line) = match slot.source {
        Some(PluginSource::Element { text, line }) => {
            (JsValue::from(text), JsValue::Null, JsValue::from(line))
        }
        Some(PluginSource::Call(called)) => (
            JsValue::Null,
            JsValue::from(called.file()),
            JsValue::from(called.line()),
        ),
        None => (JsValue::Null, JsValue::Null, JsValue::Null),
    };
    entry.insert("source", source);
    entry.insert("file", file);
    entry.insert("line", line);
    entry.insert(
        "name",
        slot.plugin.name().map_or(JsValue::Null, JsValue::from),
    );
    entry.insert("strategy", strategy_value(slot.strategy.get()));
    entry.insert("roles", roles_value(slot.plugin.roles()));
    JsValue::Object(entry)
}

/// Writes `bytes` to standard output.
fn emit(bytes: &[u8]) -> ExitCode {
    let mut stdout = io::stdout().lock();
    match stdout.write_all(bytes).and_then(|()| stdout.flush()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::from(EX_IOERR),
    }
}
