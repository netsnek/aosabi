// SPDX-License-Identifier: MIT
/*
 * aosabi-js plugin: the plugins of the app as plugins of the engine.
 *
 * No analogue in the reference.
 */

//! The plugins of the app.
//!
//! A plugin of the app crosses at boot as the keys the host reports of its
//! object, the record [`read`] takes: its name, its strategy, its roles and its
//! dependencies, which the sort of the engine reads, the form of its hook
//! `onExecute` and the names of the other hooks of Envelop and Yoga it holds.
//! Its setup and its middleware run in the hono of the host, `aosabi/pylon`,
//! the setup in the pass of `executeConfig` of the glue and the middleware in
//! the plugin chain of the base pipeline there, so neither crosses into the
//! engine.
//!
//! The hook `onExecute` of a plugin of the app runs in the engine as a hook of
//! `aosabi-hooks`: the engine calls the function of the host with a payload
//! whose members are functions of the engine, and keeps the hooks it answers in
//! the table of the request, and the done hook of the execution calls their
//! `onExecuteDone` with the payload `{args, result, setResult}`, as the
//! orchestrator of Envelop hands each hook its payload. The member `args` of
//! both payloads is one view of the arguments of the execution, which
//! `crate::nodes` numbers for the request. The result is the outcome of the
//! engine with its members in their order, and each of its errors carries the
//! nodes, the source and the positions of the error of the engine, so a remap
//! that hands `err.nodes` to a new `GraphQLError` keeps the locations of the
//! error it remaps. Once the done hook returns, the wrapper hands the engine
//! the result it leaves, the one `setResult` set last or the result the payload
//! handed as the hook left it, through the op `SET_RESULT` of the export `ctx`,
//! as Yoga reads the one object Envelop holds after the hooks ran. An error of
//! that result that the payload handed crosses back as the error of the engine
//! it stands for, so its bytes stay the engine's, and a result whose members
//! the three members of a result hold no form for keeps them in their order.
//! Any other hook of Envelop or Yoga a plugin holds fails the boot with its
//! name, as `aosabi migrate` names it with its file and its line.
//!
//! A hook that throws rejects the execution as a hook of Envelop rejects it, a
//! done hook through the throw of its payload under the feature `host-hooks` of
//! `aosabi-hooks`, and the engine reads the thrown value as the handler of Yoga
//! reads a value that escapes the execution, so a `GraphQLError` and a
//! `ServiceError` keep their message and their extensions. A done
//! hook that answers a promise is awaited through the payload under the same
//! feature, and a rejection rejects the execution the same way. The payload and
//! the result a done hook leaves cross at any depth. Envelop also awaits a hook
//! `onExecute` that answers a promise, which throws a `TypeError` at its call
//! here, and a result set after the done hook settled takes no effect, each
//! named in section 9 of `docs/ts1-surface.md`.

#![forbid(unsafe_code)]

use core::cell::{
    Cell,
    RefCell, //
};
use std::{
    rc::Rc,
    sync::Arc, //
};

use aosabi_auth::authz::map_forbidden_errors;
use aosabi_core::{
    IntoPlugin as _,
    plugin::{
        Plugin,
        PluginObject,
        PluginRole,
        Strategy,
        object::{
            __wrap,
            OnExecuteFn, //
        },
        on_execute_done, //
    },
};
use aosabi_execute::{
    ExecutionResult,
    errors::copy_graphql_error,
    result::HostMember,
    value::{
        Completion,
        Value, //
    },
};
use aosabi_hooks::plugin::{
    ExecutionOutcome,
    OnExecuteDone,
    OnExecuteHooks, //
};
use aosabi_json::{
    JsObject as JsonObject,
    JsString,
    JsValue, //
};
use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions,
    OriginalError,
    Source,
    SourceLocation,
    ast::AstNode, //
};

use crate::{
    host::{
        self,
        Reader,
        Tag, //
    },
    request::{
        self,
        BOOT, //
    },
    value, //
};

/// The forms of the hook `onExecute` of a plugin of the boot record.
mod form {
    /// The plugin holds no hook `onExecute`.
    pub const NONE: u8 = 0;
    /// The hook is a function of the app, after the handle of the function
    /// the wrapper calls it through and the handle of the function that calls
    /// a done hook.
    pub const HOST: u8 = 1;
    /// The hook is the map of the identity plugin, an item of the engine.
    pub const IDENTITY: u8 = 2;
}

/// The forms of the result of an execution the payload of a done hook holds.
mod outcome {
    /// A single result, with its members.
    pub const SINGLE: u8 = 0;
    /// A stream of results, which a subscription answers.
    pub const STREAM: u8 = 1;
}

/// The members of a result in the payload of a done hook and in a result the
/// app sets, each a byte and what follows it.
mod member {
    /// The member `errors` that holds an array, its count and each error.
    pub const ERRORS: u8 = 0;
    /// The member `data`, the value record of its copy.
    pub const DATA: u8 = 1;
    /// The member `extensions` that holds an object, the value record of its
    /// copy.
    pub const EXTENSIONS: u8 = 2;
    /// Any other member, or `errors` or `extensions` that hold null, its key
    /// and the value record of its copy.
    pub const OTHER: u8 = 3;
}

/// The forms of an error of a result the app sets.
mod set {
    /// An error of the result the payload handed, after its index there.
    pub const ENGINE: u8 = 0;
    /// A `GraphQLError` of the app, after the message, the path, the
    /// locations and the extensions of its formatted error.
    pub const APP: u8 = 1;
}

/// A done hook that runs, the number its payload carries, copies of the
/// errors of the result its payload handed, by their index, and the result
/// the app left.
struct Running {
    /// The number of the call.
    number: u32,
    /// The errors of the result the payload handed.
    errors: Vec<GraphQLError>,
    /// The result the app left.
    set: Option<ExecutionResult>,
}

// An `unsafe` cell would skip each borrow flag.
thread_local! {
    /// The done hooks that run, each while its call of the host runs and,
    /// for one that answered a promise, until the orchestrator has awaited
    /// it, so the done hooks of requests that wait side by side keep their
    /// own.
    static RUNNING: RefCell<Vec<Running>> = const { RefCell::new(Vec::new()) };
    /// The number of the last call of a done hook.
    static CALLS: Cell<u32> = const { Cell::new(0) };
}

/// The entry of a done hook in [`RUNNING`], by its number, which leaves the
/// table when it is dropped, at the end of the call of the host or once the
/// orchestrator has awaited the promise the hook answered, or with the future
/// of a request that ends before that.
struct Entry(u32);

impl Entry {
    /// Enters the done hook `number` with the copies `errors` of the errors
    /// of the result its payload hands.
    fn open(number: u32, errors: Vec<GraphQLError>) -> Self {
        RUNNING.with_borrow_mut(|running| {
            running.push(Running {
                number,
                errors,
                set: None,
            });
        });
        Self(number)
    }

    /// Takes the result the app left for the done hook, if any.
    fn take_set(&self) -> Option<ExecutionResult> {
        RUNNING.with_borrow_mut(|running| {
            running
                .iter_mut()
                .find(|running| running.number == self.0)
                .and_then(|running| running.set.take())
        })
    }
}

impl Drop for Entry {
    fn drop(&mut self) {
        // The table may be gone or borrowed where a request ends as the
        // thread of the native library ends, and its entry goes with it.
        let _left = RUNNING.try_with(|running| {
            if let Ok(mut running) = running.try_borrow_mut() {
                running.retain(|running| running.number != self.0);
            }
        });
    }
}

/// What a plugin of the app of the boot record is, beside its plugin of the
/// engine: whether its hook `onExecute` is the map of the identity plugin.
pub struct Read {
    /// The plugin of the engine.
    pub plugin: Rc<dyn Plugin>,
    /// Whether its hook `onExecute` is the map of the identity plugin.
    pub identity: bool,
}

/// Reads a plugin of the app at the cursor of `reader`, or answers the
/// message of the refusal of a hook the engine runs not, or `None` for a
/// broken record.
///
/// The record is the name, a byte 0 for none or 1 and the name, the strategy,
/// 0 for none, 1 for `first` and 2 for `last`, the roles, a byte 0 for none or
/// 1, a count and a byte per role, 0 for `web` and 1 for `worker`, the
/// dependencies, a byte 0 for none or 1, a count and each name, the form of
/// the hook `onExecute`, with the two handles of the form [`form::HOST`], and
/// the names of the other hooks it holds, a count and each name.
// Out of line, so a boot compiles the read of a plugin of the app where the configuration holds
// one.
#[inline(never)]
pub fn read(reader: &mut Reader<'_>) -> Option<Result<Read, String>> {
    let name = match reader.byte()? {
        0 => None,
        _ => Some(String::from(reader.text()?)),
    };
    let strategy = match reader.byte()? {
        1 => Some(Strategy::First),
        2 => Some(Strategy::Last),
        _ => None,
    };
    let roles = if reader.byte()? == 0 {
        None
    } else {
        let mut roles = Vec::new();
        for _ in 0..reader.count()? {
            roles.push(if reader.byte()? == 0 {
                PluginRole::Web
            } else {
                PluginRole::Worker
            });
        }
        Some(roles)
    };
    let depends_on = if reader.byte()? == 0 {
        None
    } else {
        let mut names = Vec::new();
        for _ in 0..reader.count()? {
            names.push(String::from(reader.text()?));
        }
        Some(names)
    };
    let (on_execute, identity): (Option<OnExecuteFn>, bool) = match reader.byte()? {
        form::NONE => (None, false),
        form::HOST => (host_execute(reader.word()?, reader.word()?), false),
        form::IDENTITY => (identity_execute(), true),
        _ => return None,
    };
    let mut hooks = Vec::new();
    for _ in 0..reader.count()? {
        hooks.push(String::from(reader.text()?));
    }
    if let Some(hook) = hooks.first() {
        let label = name.as_deref().map_or_else(
            || String::from("an unnamed plugin"),
            |name| format!("the plugin \"{name}\""),
        );
        return Some(Err(format!(
            "{label} of the app holds the hook {hook}, which has no engine item in aosabi, since the engine runs onExecute alone of the hooks of Envelop and Yoga, and aosabi migrate names the hook with its file and its line"
        )));
    }
    Some(Ok(Read {
        plugin: PluginObject {
            name,
            roles,
            strategy,
            depends_on,
            on_execute,
            ..PluginObject::default()
        }
        .into_plugin(),
        identity,
    }))
}

/// Returns the hook `onExecute` of the map of the identity plugin: the done
/// hook maps every error whose original error is a `ForbiddenError` to the
/// error `FORBIDDEN` of [`map_forbidden_errors`], over a single result, as the
/// done hook of `useIdentity` maps them.
fn identity_execute() -> Option<OnExecuteFn> {
    __wrap::on_execute(|_payload| {
        on_execute_done(|done| {
            if let ExecutionOutcome::Single(result) = &mut *done.result
                && let Some(mapped) = map_forbidden_errors(&result.errors)
            {
                result.errors = mapped;
            }
        })
    })
}

/// Returns the hook `onExecute` of a plugin of the app whose function the
/// wrapper calls through the function `execute` of the table of the boot,
/// and whose done hook it calls through the function `done` of that table.
///
/// The call of `execute` hands the host the payload of `onExecute` with the
/// number of the view of the arguments of the execution, and the host answers
/// the handle of the hooks the app answered in the table of the request, or
/// `undefined` where it answered no hooks. A throw of the call throws the hook,
/// as a hook of Envelop that throws rejects the execution, with the value the
/// handler of Yoga reads of what the app threw, a `GraphQLError` it keeps among
/// them, through [`value::call_hook`].
fn host_execute(execute: u32, done: u32) -> Option<OnExecuteFn> {
    __wrap::on_execute(move |payload| {
        let request = request::current();
        let args = crate::nodes::note_args(request, payload.args, payload.context);
        let completion = value::call_hook(BOOT, execute, request, 1, |out| {
            out.push(host::EXECUTE);
            host::push_word(out, args);
        });
        match completion {
            Completion::Throw(thrown) => Err(thrown),
            Completion::Normal(Value::Number(handle)) => {
                Ok(host_done(done, handle_of(handle)?, args))
            }
            Completion::Normal(_) => Ok(None),
        }
    })
}

/// Returns the number `number` as a handle of a table, or the thrown value of
/// a number that is no handle.
fn handle_of(number: f64) -> Result<u32, Value> {
    value::index_of(number)
        .and_then(|index| u32::try_from(index).ok())
        .ok_or_else(|| {
            Value::from(aosabi_execute::value::ErrorObject::error(
                "the host answered the hooks of onExecute with no handle",
            ))
        })
}

/// Returns the hooks of an execution whose done hook calls `onExecuteDone`
/// of the hooks `hooks` of the table of the request through the function
/// `done` of the table of the boot.
///
/// The call hands the host the hooks and the payload of the done hook, its
/// number, the number `args` of the view of the arguments of the execution and
/// the outcome of the execution, inside the agent of the execution, and the
/// result the app leaves, which the wrapper hands the engine through the op
/// `SET_RESULT` of the export `ctx` once the hook has returned, replaces the
/// outcome. Where the hook answered a promise, the call answers the promise the
/// wrapper chains to it, which fulfils once the wrapper has handed that result,
/// and the orchestrator awaits it before the next done hook runs and replaces
/// the outcome then, as `iterateAsync` awaits a done hook of Envelop. A call
/// that throws throws the hook, and a promise that rejects rejects it, so the
/// execution rejects with the value the handler of Yoga reads of what the app
/// threw and a result the app left takes no effect, as a done hook of Envelop
/// that throws or rejects rejects the execution.
fn host_done(done: u32, hooks: u32, args: u32) -> Option<OnExecuteHooks> {
    on_execute_done(move |payload: &mut OnExecuteDone<'_>| {
        let request = request::current();
        let number = CALLS.get().wrapping_add(1);
        CALLS.set(number);
        let errors = match &*payload.result {
            ExecutionOutcome::Single(result) => {
                result.errors.iter().map(copy_graphql_error).collect()
            }
            ExecutionOutcome::Stream(_) => Vec::new(),
        };
        let entry = Entry::open(number, errors);
        let agent = payload.agent().clone();
        let completion = agent.enter(|| {
            value::call_hook(BOOT, done, request, 2, |out| {
                host::push_tag(out, Tag::Held);
                host::push_word(out, hooks);
                out.push(host::DONE);
                host::push_word(out, number);
                host::push_word(out, args);
                write_outcome(out, &*payload.result, request);
            })
        });
        match completion {
            Completion::Throw(thrown) => payload.throw(thrown),
            Completion::Normal(Value::Promise(promise)) => payload.wait(
                promise,
                Box::new(move |outcome: &mut ExecutionOutcome| {
                    if let Some(result) = entry.take_set() {
                        *outcome = ExecutionOutcome::Single(result);
                    }
                }),
            ),
            Completion::Normal(_) => {
                if let Some(result) = entry.take_set() {
                    payload.set_result(ExecutionOutcome::Single(result));
                }
            }
        }
    })
}

/// The sources the nodes and the errors of an outcome blame, each written
/// once before the errors and named by its index after it.
#[derive(Default)]
struct Sources<'a>(Vec<&'a Arc<Source>>);

impl<'a> Sources<'a> {
    /// Returns the index of `source`, which it takes the first time.
    fn index(&mut self, source: &'a Arc<Source>) -> usize {
        if let Some(index) = self.0.iter().position(|known| Arc::ptr_eq(known, source)) {
            return index;
        }
        self.0.push(source);
        self.0.len() - 1
    }
}

/// Returns the members of `result` in the order its object holds them: those
/// a host set, the data and the extensions where the result holds them, or
/// `errors` where it holds an error, `data` where it holds the key and
/// `extensions` where it holds them, the order of
/// [`ExecutionResult::to_js_value`].
fn members_of(result: &ExecutionResult) -> Vec<HostMember> {
    if let Some(members) = &result.host_members {
        return members
            .iter()
            .filter(|member| match member {
                HostMember::Data => result.has_data(),
                HostMember::Extensions => result.extensions.is_some(),
                HostMember::Errors | HostMember::Other(..) => true,
            })
            .cloned()
            .collect();
    }
    let mut members = Vec::with_capacity(3);
    if !result.errors.is_empty() {
        members.push(HostMember::Errors);
    }
    if result.has_data() {
        members.push(HostMember::Data);
    }
    if result.extensions.is_some() {
        members.push(HostMember::Extensions);
    }
    members
}

/// Appends the record of the outcome `outcome` of an execution of `request`
/// to the payload of a done hook.
///
/// The record is the byte of a stream, or the byte of a single result, the
/// sources its errors blame, each its body, its name and the line and the
/// column of its offset, and its members in their order, each a byte of
/// [`member`] and what follows it: for the errors their count and each
/// error, its index, its message, the value records of its path, its
/// locations, its extensions and its original error, its nodes, its source
/// and its positions, for the data and the extensions the value record of
/// their copy, and for another member its key and that record, each copy
/// written by [`host::push_json`], so it crosses at any depth. The path and
/// the locations are those its formatted error writes, an original error of
/// the host crosses as the value the host holds, and each node is its kind
/// and the start, the end and the source of its location, which the wrapper
/// gives a node of the error it makes.
fn write_outcome(out: &mut Vec<u8>, outcome: &ExecutionOutcome, request: u32) {
    let ExecutionOutcome::Single(result) = outcome else {
        out.push(outcome::STREAM);
        return;
    };
    out.push(outcome::SINGLE);
    let mut sources = Sources::default();
    for error in &result.errors {
        for loc in error.nodes.iter().flatten().filter_map(AstNode::loc) {
            sources.index(&loc.source);
        }
        if let Some(source) = &error.source {
            sources.index(source);
        }
    }
    host::push_count(out, sources.0.len());
    for source in &sources.0 {
        host::push_bytes(out, source.bytes());
        host::push_bytes(out, source.name().as_bytes());
        let offset = source.location_offset();
        push_size(out, offset.line);
        push_size(out, offset.column);
    }
    let members = members_of(result);
    host::push_count(out, members.len());
    for member in &members {
        match member {
            HostMember::Errors => {
                out.push(member::ERRORS);
                host::push_count(out, result.errors.len());
                for (index, error) in result.errors.iter().enumerate() {
                    write_error(out, index, error, request, &mut sources);
                }
            }
            HostMember::Data => {
                out.push(member::DATA);
                host::push_json(out, result.data().unwrap_or(&JsValue::Null));
            }
            HostMember::Extensions => {
                out.push(member::EXTENSIONS);
                let extensions = result.extensions.clone().unwrap_or_default();
                host::push_json(out, &JsValue::Object(extensions));
            }
            HostMember::Other(key, value) => {
                out.push(member::OTHER);
                host::push_string(out, key);
                host::push_json(out, value);
            }
        }
    }
}

/// Appends the record of the error `error` at `index` of an outcome of
/// `request`, whose sources `sources` names.
fn write_error<'a>(
    out: &mut Vec<u8>,
    index: usize,
    error: &'a GraphQLError,
    request: u32,
    sources: &mut Sources<'a>,
) {
    host::push_count(out, index);
    host::push_string(out, &error.message);
    let formatted = JsValue::from(error.to_json());
    for key in ["path", "locations"] {
        match &formatted {
            JsValue::Object(object) => match object.get(key) {
                Some(member) => host::push_json(out, member),
                None => host::push_tag(out, Tag::Undefined),
            },
            _ => host::push_tag(out, Tag::Undefined),
        }
    }
    host::push_json(out, &JsValue::Object(error.extensions.clone()));
    value::push_original(out, error, request);
    match &error.nodes {
        None => out.push(0),
        Some(nodes) => {
            out.push(1);
            host::push_count(out, nodes.len());
            for node in nodes {
                host::push_bytes(out, node.kind().as_str().as_bytes());
                match node.loc() {
                    None => out.push(0),
                    Some(loc) => {
                        out.push(1);
                        push_size(out, loc.start);
                        push_size(out, loc.end);
                        host::push_count(out, sources.index(&loc.source));
                    }
                }
            }
        }
    }
    match &error.source {
        None => out.push(0),
        Some(source) => {
            out.push(1);
            host::push_count(out, sources.index(source));
        }
    }
    match &error.positions {
        None => out.push(0),
        Some(positions) => {
            out.push(1);
            host::push_count(out, positions.len());
            for &position in positions {
                push_size(out, position);
            }
        }
    }
}

/// Appends `size`, an offset in code units, a line or a column of a body the
/// lexer reads, as the number the host reads.
#[expect(
    clippy::cast_precision_loss,
    reason = "a size of a body the lexer reads fits a u32, which a double holds exactly"
)]
fn push_size(out: &mut Vec<u8>, size: usize) {
    host::push_number(out, size as f64);
}

/// Takes the result the app leaves for the done hook whose number the record
/// at the cursor of `reader` names, and answers `None` for a broken record.
///
/// The record is the number and the members of the result in their order,
/// each a byte of [`member`] and what follows it: for the errors their count
/// and each error, the byte [`set::ENGINE`] and its index or the byte
/// [`set::APP`], its message and the value records of its path, its
/// locations and its extensions, for the data and the extensions the value
/// record of their copy, and for another member its key and that record,
/// each the copy JSON reads, which [`host::read_json`] reads at any depth.
/// A result whose members are `errors` with an
/// error, `data` and `extensions` in that order keeps no members of its own,
/// so the engine writes it as before, and any other keeps its members in
/// their order, an empty `errors` among them. A result for a done hook that
/// no longer runs takes no effect, as the done hook has handed the outcome
/// on.
// Out of line, so a request compiles the write of a result of a plugin where a hook makes one.
#[inline(never)]
pub fn set_result(reader: &mut Reader<'_>) -> Option<()> {
    let number = reader.word()?;
    let mut errors = Vec::new();
    let mut data = None;
    let mut extensions = None;
    let mut members = Vec::new();
    for _ in 0..reader.count()? {
        match reader.byte()? {
            member::ERRORS => {
                for _ in 0..reader.count()? {
                    errors.push(set_error(reader, number)?);
                }
                members.push(HostMember::Errors);
            }
            member::DATA => {
                data = Some(host::read_json(reader)?);
                members.push(HostMember::Data);
            }
            member::EXTENSIONS => {
                let JsValue::Object(object) = host::read_json(reader)? else {
                    return None;
                };
                extensions = Some(object);
                members.push(HostMember::Extensions);
            }
            member::OTHER => {
                let key = reader.string()?;
                let copy = host::read_json(reader)?;
                members.push(HostMember::Other(key, copy));
            }
            _ => return None,
        }
    }
    let mut result = ExecutionResult::new(errors, data, extensions);
    if members != members_of(&result) {
        result.host_members = Some(members);
    }
    RUNNING.with_borrow_mut(|running| {
        if let Some(running) = running.iter_mut().find(|running| running.number == number) {
            running.set = Some(result);
        }
    });
    Some(())
}

/// Reads an error of a result the app leaves for the done hook `number` at
/// the cursor of `reader`: a copy of the error of the outcome its index
/// names, or a `GraphQLError` of the app.
// Out of line, so a hook that leaves a result with no error compiles no read of one.
#[inline(never)]
fn set_error(reader: &mut Reader<'_>, number: u32) -> Option<GraphQLError> {
    match reader.byte()? {
        set::ENGINE => {
            let index = usize::try_from(reader.word()?).ok()?;
            RUNNING.with_borrow(|running| {
                running
                    .iter()
                    .find(|running| running.number == number)
                    .and_then(|running| running.errors.get(index))
                    .map(copy_graphql_error)
            })
        }
        set::APP => app_error(reader),
        _ => None,
    }
}

/// Reads a `GraphQLError` the app set at the cursor of `reader`: its message,
/// its path, its locations and its extensions, as its formatted error writes
/// them, which the engine writes again as they are, each copy read by
/// [`host::read_json`], so it crosses at any depth.
fn app_error(reader: &mut Reader<'_>) -> Option<GraphQLError> {
    let message: JsString = reader.string()?;
    let path = host::read_json(reader)?;
    let locations = host::read_json(reader)?;
    let extensions = match host::read_json(reader)? {
        JsValue::Object(extensions) => extensions,
        _ => JsonObject::new(),
    };
    let (path, host_path) = value::paths_of_json(&path);
    let mut error = GraphQLError::new(
        message,
        GraphQLErrorOptions {
            path,
            extensions: Some(extensions),
            ..GraphQLErrorOptions::default()
        },
    );
    let (locations, host_locations) = match locations_of(&locations) {
        Some(locations) => (Some(locations), None),
        None if matches!(locations, JsValue::Array(_)) => (None, Some(locations)),
        None => (None, None),
    };
    error.locations = locations;
    error.host_places = value::host_places(host_path, host_locations);
    error.original_error = None::<OriginalError>;
    Some(error)
}

/// Returns the locations of the copy `locations`, an array of objects whose
/// members `line` and `column` are whole numbers, or `None` for any other
/// copy.
fn locations_of(locations: &JsValue) -> Option<Vec<SourceLocation>> {
    let JsValue::Array(items) = locations else {
        return None;
    };
    items
        .iter()
        .map(|item| {
            let JsValue::Object(location) = item else {
                return None;
            };
            let whole = |key: &str| match location.get(key) {
                Some(JsValue::Number(number)) => value::index_of(*number),
                _ => None,
            };
            Some(SourceLocation {
                line: whole("line")?,
                column: whole("column")?,
            })
        })
        .collect()
}
