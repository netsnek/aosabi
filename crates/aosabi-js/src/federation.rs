// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-js federation: the federations of the app over aosabi-federation.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/core/gateway.ts,
 * Apache-2.0. Changed by the Aosabi authors.
 */

//! The federations of the app.
//!
//! A federation the app makes with `createGateway().configure(config)` of the
//! package is a marker of the wrapper with a number of its own. Its first
//! delegation in an instance of the engine hands the engine its
//! configuration: the URL, `strict`, the headers function and the patches,
//! each function of the app put in the table of the boot, which lives as
//! long as the instance, with the identity the wrapper gives the headers
//! function, and the policy `pass` attached to each patch. The engine keeps
//! one [`Gateway`] of `aosabi-federation` per number, made by
//! `__configure_host`, so the cache of remote schemas, the executor of a slot
//! and the policies are the crate's and hold as on the Rust option, while a
//! patch, a guard and the headers function are calls of the host.
//!
//! A delegation of the app, `gateway.delegate(key, {args, needs, guard})`, asks
//! the engine through the member [`OP`] of the export `ctx`, with the number of
//! the info of the call that runs and the data of its arguments and its needs,
//! which the wrapper writes as the reference's builder of the syntax tree reads
//! them. The engine reads the resolve info and the context of the request as
//! the reference reads them before its first await, starts the delegation of
//! the crate as a task of the agent of the request, and answers the number of
//! its promise in the list of the request. The wrapper answers the app a marker
//! of that number, and a resolver or a member of a patch that answers the
//! marker answers the promise of the engine itself, so the value the delegation
//! settles with never crosses to the host and back. The guard runs once the
//! value is there, over the value the delegation answered, and a value that is
//! not nullish and that the guard rejects becomes `null`.

#![forbid(unsafe_code)]

use core::cell::RefCell;
use std::{
    borrow::Cow,
    rc::Rc,
    sync::Arc, //
};

use aosabi_core::{
    Thrown,
    context::run_with_context,
    remote::Registry,
    resolve_info::run_with_execution_context, //
};
use aosabi_execute::{
    promise::{
        Agent,
        Promise, //
    },
    value::{
        Completion,
        HostValue,
        Value, //
    }, //
};
use aosabi_federation::{
    gateway::{
        Gateway,
        create_gateway, //
    },
    headers::{
        Headers,
        HostHeaders, //
    },
    needs::{
        Needs,
        NeedsNode, //
    },
    policy::{
        FieldPolicy,
        Force,
        PatchPolicy, //
    },
    transforms::patch::{
        PatchApi,
        PatchFn,
        PatchTable, //
    },
    utils::is_truthy,
};
use aosabi_http::Context;
use aosabi_json::{
    JsObject,
    JsString,
    JsValue, //
};
use aosabi_lang::js_string::to_string_lossy;

use crate::{
    host::{
        self,
        Reader, //
    },
    request,
    value, //
};

/// The member of the export `ctx` that starts a delegation, after the number
/// of the info of the call, the number of the federation, its configuration
/// where the engine has not seen it, the key, the data of the arguments and
/// of the needs, and the handle of the guard.
pub const OP: u8 = 35;

/// The table of the boot, which holds the functions of a configuration.
const BOOT: u32 = 0;

/// The message of a delegation whose federation the engine does not know.
///
/// No analogue in the reference, whose federation is the object the app
/// calls. The wrapper hands the configuration with the first delegation of a
/// federation in each instance, so no delegation of the app meets the text.
const UNKNOWN: &str = "aosabi-js: a delegation names a federation the engine was not handed";

/// The message of a delegation that finds no agent of its request.
///
/// No analogue in the reference, where a job queue always exists. A
/// delegation runs inside a call of the request, whose agent the request
/// adopted at its first import, so no delegation of the app meets the text.
const NO_AGENT: &str = "aosabi-js: a delegation runs where its request has no agent";

/// The message of a delegation the list of its request does not hold.
///
/// No analogue in the reference. A marker of a delegation is answered in the
/// request that made it, so a marker an app keeps across requests meets the
/// text.
const NO_DELEGATION: &str = "aosabi-js: a delegation answered in a request that did not start it";

/// The registry of a federation of the host, which lists no key and no type.
///
/// An app of the TypeScript option names its keys and its patches as data,
/// so nothing of the typed surface of the crate reads the registry.
struct HostRegistry;

impl Registry for HostRegistry {
    const DELEGATE: &'static [&'static str] = &[];
    const TYPES: &'static [&'static str] = &[];
}

// An `unsafe` cell would skip each borrow flag.
thread_local! {
    /// The federations of the instance, by the number the wrapper gives each.
    static GATEWAYS: RefCell<Vec<(u32, Gateway<HostRegistry>)>> = const { RefCell::new(Vec::new()) };

    /// The promises of the delegations of each request in flight, by request,
    /// each list in the order the delegations started.
    static DELEGATIONS: RefCell<Vec<(u32, Vec<Promise>)>> = const { RefCell::new(Vec::new()) };
}

/// Starts the delegation of the record at the cursor of `reader`, a call of
/// the app in `request` over its context `c`, and answers the number of its
/// promise, or `None` for a broken record.
pub fn start(request: u32, c: &Context, reader: &mut Reader<'_>) -> Option<Result<Value, Thrown>> {
    let info = reader.word()?;
    let number = reader.word()?;
    let configured = match reader.byte()? {
        0 => None,
        _ => Some(configure(reader)?),
    };
    let key = String::from(reader.text()?);
    let args = match host::read_json(reader)? {
        JsValue::Object(args) => args,
        _ => JsObject::new(),
    };
    let needs = needs_of(&host::read_json(reader)?).unwrap_or_default();
    let guard = match reader.byte()? {
        0 => None,
        _ => Some(reader.word()?),
    };
    if let Some(configured) = configured {
        match configured {
            Ok(gateway) => keep(number, gateway),
            Err(thrown) => return Some(Err(thrown)),
        }
    }
    let Some(gateway) = gateway(number) else {
        return Some(Err(Thrown::type_error(UNKNOWN)));
    };
    let Some(agent) = Agent::current().or_else(|| request::agent_of(request)) else {
        return Some(Err(Thrown::type_error(NO_AGENT)));
    };
    let execution = crate::context::info_of(request, info);
    let begin = || {
        let running = gateway.__delegate_key(&key, args, needs);
        agent.spawn(Box::pin(async move {
            match running.await {
                Ok(value) => match guard {
                    Some(guard) if !value.is_nullish() => {
                        match value::call_value(request, guard, request, &value) {
                            Completion::Normal(kept) if is_truthy(&kept) => {
                                Completion::Normal(value)
                            }
                            Completion::Normal(_) => Completion::Normal(Value::Null),
                            Completion::Throw(error) => Completion::Throw(error),
                        }
                    }
                    _ => Completion::Normal(value),
                },
                Err(Thrown(error)) => Completion::Throw(error),
            }
        }))
    };
    let promise = request::with_current(request, || {
        agent.enter(|| {
            run_with_context(c.clone(), || match execution {
                Some(execution) => run_with_execution_context(execution, begin),
                None => begin(),
            })
        })
    });
    let at = DELEGATIONS.with_borrow_mut(|delegations| {
        let at = delegations
            .iter()
            .position(|(held, _)| *held == request)
            .unwrap_or_else(|| {
                delegations.push((request, Vec::new()));
                delegations.len() - 1
            });
        let list = &mut delegations[at].1;
        list.push(promise);
        host::length(list.len() - 1)
    });
    Some(Ok(Value::Number(f64::from(at))))
}

/// Returns the promise of the delegation `number` of `request`, the value a
/// resolver or a member of a patch answers with the marker of the wrapper.
pub fn delegation(request: u32, number: u32) -> Value {
    let promise = DELEGATIONS.with_borrow(|delegations| {
        let (_, list) = delegations.iter().find(|(held, _)| *held == request)?;
        list.get(usize::try_from(number).ok()?).cloned()
    });
    promise.map_or_else(
        || {
            Value::from(aosabi_execute::value::ErrorObject::type_error(
                NO_DELEGATION,
            ))
        },
        Value::Promise,
    )
}

/// Drops the delegations of `request`, which the request drops with itself.
pub fn forget(request: u32) {
    let gone = DELEGATIONS.with_borrow_mut(|delegations| {
        delegations
            .iter()
            .position(|(held, _)| *held == request)
            .map(|at| delegations.swap_remove(at))
    });
    drop(gone);
}

/// Keeps `gateway` under the number `number` of the wrapper.
fn keep(number: u32, gateway: Gateway<HostRegistry>) {
    GATEWAYS.with_borrow_mut(|gateways| {
        match gateways.iter_mut().find(|(held, _)| *held == number) {
            Some(entry) => entry.1 = gateway,
            None => gateways.push((number, gateway)),
        }
    });
}

/// Returns the federation of the number `number` of the wrapper.
fn gateway(number: u32) -> Option<Gateway<HostRegistry>> {
    GATEWAYS.with_borrow(|gateways| {
        gateways
            .iter()
            .find(|(held, _)| *held == number)
            .map(|(_, gateway)| gateway.clone())
    })
}

/// Reads the configuration at the cursor of `reader` and answers its
/// federation, or the refusal of a policy, or `None` for a broken record.
///
/// The record is the URL, the byte of `strict`, the headers function as its
/// handle in the table of the boot and its identity, or a byte 0, and the
/// patches in the order of the keys of the object of the app, each its type
/// name, its kind, a function of the table of the boot or the identity of
/// `passthrough()`, and the policy `pass` attached to it, a JSON record, or a
/// byte 0.
fn configure(reader: &mut Reader<'_>) -> Option<Result<Gateway<HostRegistry>, Thrown>> {
    let url = String::from(reader.text()?);
    let strict = reader.byte()? != 0;
    let headers = if reader.byte()? == 0 {
        None
    } else {
        let handle = reader.word()?;
        let identity = reader.word()?;
        Some(host_headers(handle, identity))
    };
    let count = reader.count()?;
    let mut table = PatchTable::new();
    for _ in 0..count {
        let type_name = String::from(reader.text()?);
        let patch = match reader.byte()? {
            0 => host_patch(reader.word()?),
            _ => passthrough(),
        };
        let policy = match reader.byte()? {
            0 => None,
            _ => match patch_policy(&host::read_json(reader)?) {
                Some(policy) => Some(policy),
                None => return Some(Err(Thrown::type_error(BROKEN_POLICY))),
            },
        };
        table.insert(type_name, patch, policy);
    }
    let patches = (count > 0).then(|| Rc::new(table));
    Some(Ok(
        create_gateway::<HostRegistry>().__configure_host(url, headers, patches, strict)
    ))
}

/// The message of a policy the wrapper wrote in a form the engine does not read.
///
/// No analogue in the reference. The wrapper writes the allowed names as
/// strings and the forced values as data, and refuses a forced value that
/// is a function before it writes the record, so no policy of the app meets
/// the text.
const BROKEN_POLICY: &str = "aosabi-js: a policy of a patch is no object of fields";

/// Returns the patch of the function `handle` of the table of the boot, a
/// call of the host with the object it patches, whose answer is what the
/// patch answers and whose throw is what it throws.
///
/// The wrapper's function calls the patch of the app with the object and the
/// context of its federation, `(data, api) => value`, as the reference hands
/// every patch the object with `delegate`, so the handle `api` of the crate is
/// not read.
fn host_patch(handle: u32) -> PatchFn {
    Rc::new(move |data: Value, _api: &PatchApi| {
        match value::call_value(BOOT, handle, request::current(), &data) {
            Completion::Normal(answer) => Ok(answer),
            Completion::Throw(error) => Err(Thrown(error)),
        }
    })
}

/// Returns the patch of `passthrough()`, which answers the object it is handed.
///
/// The reference's `passthrough()` answers `data => data`, so the transform
/// merges the object over its own copy, and the engine answers the object with
/// no call of the host.
fn passthrough() -> PatchFn {
    Rc::new(|data: Value, _api: &PatchApi| Ok(data))
}

/// Returns the headers function of the host of the function `handle` of the
/// table of the boot, with the identity `identity` the wrapper gives the
/// function of the app.
///
/// The call hands the host the context of the delegating request, which the
/// wrapper reads as the facade `c` of that request, or `undefined` for the
/// introspection, and reads the pairs of names and values the wrapper's
/// function answers, the members of the object the function of the app answers
/// in the order of a spread, each value as `String` writes it. A call that
/// throws answers no header, since the executor of the crate takes the headers
/// of a function that answers them always, where the reference rejects the
/// request of the executor.
fn host_headers(handle: u32, identity: u32) -> HostHeaders {
    HostHeaders {
        identity: u64::from(identity),
        call: Rc::new(move |context: Option<&Context>| {
            let arg = context.map_or(Value::Undefined, |_| {
                // An `unsafe` `Rc` would skip the atomic count each copy takes.
                Value::Host(HostValue::Opaque(Arc::new(crate::context::ContextMark)))
            });
            match value::call_value(BOOT, handle, request::current(), &arg) {
                Completion::Normal(answer) => headers_of(&answer),
                Completion::Throw(_) => Headers::new(),
            }
        }),
    }
}

/// Returns the headers of the pairs `answer` the wrapper's headers function
/// answers, each a list of a name and a value.
fn headers_of(answer: &Value) -> Headers {
    let mut headers = Headers::new();
    let Value::List(pairs) = answer else {
        return headers;
    };
    for pair in pairs.iter() {
        let Value::List(pair) = pair else {
            continue;
        };
        if let (Some(Value::String(name)), Some(Value::String(value))) = (pair.first(), pair.get(1))
        {
            headers.set(&to_string_lossy(name), to_string_lossy(value).into_owned());
        }
    }
    headers
}

/// Returns the policy `pass` attaches to a patch, by field name, from its
/// JSON record, or `None` for a record of another form.
///
/// Each field is `{args?, force?}`: the allowed names a list of strings and the
/// forced values an object of data, the form `FieldPolicy` of the reference
/// holds.
fn patch_policy(raw: &JsValue) -> Option<PatchPolicy> {
    let JsValue::Object(raw) = raw else {
        return None;
    };
    let mut out = Vec::new();
    for (field, policy) in raw.own_keys() {
        out.push((Cow::Owned(lossy(field)), field_policy(policy)?));
    }
    Some(out)
}

/// Returns the policy of one field, `{args?, force?}`, or `None` for another form.
fn field_policy(raw: &JsValue) -> Option<FieldPolicy> {
    let JsValue::Object(raw) = raw else {
        return None;
    };
    let args = match raw.get("args") {
        None | Some(JsValue::Undefined | JsValue::Null) => None,
        Some(JsValue::Array(items)) => Some(
            items
                .iter()
                .map(|item| match item {
                    JsValue::String(name) => Some(Cow::Owned(lossy(name))),
                    _ => None,
                })
                .collect::<Option<Vec<_>>>()?,
        ),
        Some(_) => return None,
    };
    let force = match raw.get("force") {
        None | Some(JsValue::Undefined | JsValue::Null) => None,
        Some(JsValue::Object(force)) => Some(
            force
                .own_keys()
                .map(|(name, value)| (Cow::Owned(lossy(name)), Force::Value(value.clone())))
                .collect(),
        ),
        Some(_) => return None,
    };
    Some(FieldPolicy { args, force })
}

/// Returns the needs of the data `raw` the app hands as `needs`, `None` for
/// a value that is not truthy.
///
/// The twin of `InjectNeedsTransform` reading the object of the app: each key a
/// node, a falsy value a node that selects nothing, and an object a node with
/// the arguments of its member `__args` and a node per key.
fn needs_of(raw: &JsValue) -> Option<Needs> {
    truthy(raw).then(|| Needs::new(nodes_of(&entries_of(raw))))
}

/// Returns the nodes of the own members of a needs object.
fn nodes_of(object: &JsObject) -> Vec<NeedsNode> {
    object
        .own_keys()
        .map(|(key, value)| node_of(key, value))
        .collect()
}

/// Returns the node of the key `name` of a needs object with the value `value`.
fn node_of(name: &JsString, value: &JsValue) -> NeedsNode {
    let selected = truthy(value);
    let (args, children) = match value {
        _ if !selected => (None, Vec::new()),
        JsValue::Object(object) => (
            object
                .get("__args")
                .filter(|args| truthy(args))
                .map(entries_of),
            nodes_of(&entries_of(value)),
        ),
        JsValue::Array(_) => (None, nodes_of(&entries_of(value))),
        _ => (None, Vec::new()),
    };
    NeedsNode {
        name: Cow::Owned(lossy(name)),
        selected,
        args,
        children: Rc::from(children),
    }
}

/// Returns the own members of `raw` as `Object.entries` reads them, the
/// indices of a list and no member of any other value.
fn entries_of(raw: &JsValue) -> JsObject {
    match raw {
        JsValue::Object(object) => object.clone(),
        JsValue::Array(items) => {
            let mut object = JsObject::new();
            for (index, item) in items.iter().enumerate() {
                object.insert(JsString::from(index.to_string().as_str()), item.clone());
            }
            object
        }
        _ => JsObject::new(),
    }
}

/// Reports whether `value` is truthy as JavaScript reads it.
fn truthy(value: &JsValue) -> bool {
    match value {
        JsValue::Undefined | JsValue::Null => false,
        JsValue::Bool(boolean) => *boolean,
        JsValue::Number(number) => *number != 0.0 && !number.is_nan(),
        JsValue::String(string) => !string.is_empty(),
        _ => true,
    }
}

/// Returns `text` as a string of Rust, each lone surrogate replaced.
fn lossy(text: &JsString) -> String {
    to_string_lossy(text).into_owned()
}
