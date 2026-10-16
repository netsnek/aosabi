// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool federation: the seam of Federation, the transforms, the
 * documents, the patch of a result, the merge of a result, the delegation, the
 * stripping of ignored characters and the body of a request.
 *
 * No analogue in the reference. It is the Rust half of
 * tests/sidecar/federation.mjs and of the operation strip of
 * tests/sidecar/graphql-js.mjs, whose protocol tests/sidecar/federation.md
 * writes.
 */

//! The primitive `federation` of the seam.
//!
//! The flag `--requests PATH` answers every request line of the file in the
//! batch form of [`crate::batch`], `id` first and `op` second, with no
//! server, in the codec of the sidecar: a one key object whose key begins
//! with `$` stands for a value JSON cannot spell, `{"$undefined": true}`,
//! `{"$number": "NaN"}`, `{"$date": ...}`, `{"$file": {...}}`,
//! `{"$error": {...}, "ref": n}` and, in an answer, `{"$function": null}`.
//!
//! | `op` | Answers | Payload |
//! | --- | --- | --- |
//! | `transform` | the request half of the four transforms in the reversed order of the list a federation hands the delegation | `document`, `variables`, `unchanged` and `calls`, or `thrown` and `calls` |
//! | `prepare` | `prepare_gateway_document` | `document` or `thrown` |
//! | `finalize` | `finalize_gateway_request` | `document`, `variables` and `overlapping_aliases`, or `thrown` |
//! | `create_request` | `create_request` for one root field of a client document | `document` and `variables`, or `thrown` |
//! | `patch_result` | the result half of `PatchTransform` | `result` and `unchanged`, or `thrown` |
//! | `merge_data_and_errors` | `merge_data_and_errors` over data, errors and a path | `data` and `unpathed_errors`, or `thrown` |
//! | `serialize_request` | the body of a request with no file | `body` |
//! | `strip` | `strip_ignored_characters` of a source | `stripped`, or `errors` with the syntax error |
//! | `delegate` | a front execution whose root resolvers delegate through a configured federation to the remote stub, or one delegation outside a resolver | `result`, the text of the execution result, `errors` for a front schema that does not build, `value` or `thrown` |
//!
//! A patch and a forced value are code, so a request names a patch by an id
//! of the patch table of the sidecar, which [`patch`] mirrors, and a forced
//! value by `{"$fn": value}` or `{"$ctx": key}`. A
//! forced value of a policy is a function pointer of the context, so the
//! values of one request sit in a table of sixteen slots, each a function
//! that reads its slot and counts its call. A request that forces more values
//! draws a `fatal`.
//!
//! `delegate` builds the front of a request from its SDL as a served app
//! builds its schema, with root functions that call a configured federation,
//! `create_gateway` with its wrapping schema, through the untyped seam of
//! `aosabi-federation`, since a request names its key, its arguments, its
//! needs and its patches as data. A guard and a headers function are ids of
//! the tables of the sidecar. The execution runs on the runtime of
//! `aosabi-serve` with the client of `aosabi-fetch` as the network capability,
//! so the federation reaches the remote stub the request names on the
//! loopback, and the requests the stub records are the ones a served card
//! sends. The operation answers on Linux alone, where that runtime builds.

use std::{
    borrow::Cow,
    cell::{
        Cell,
        RefCell, //
    },
    path::PathBuf,
    process::ExitCode,
    rc::Rc,
    sync::Arc, //
};

use aosabi_core::{
    Thrown,
    context::service_context, //
};
use aosabi_execute::{
    execute::{FieldNodes, ResolveInfo},
    normalized_executor::get_operation_ast,
    path::add_path,
    value::{
        ErrorName,
        ErrorObject,
        Function,
        HostValue,
        ObjectLike,
        PlainObject,
        Value,
        to_js_string, //
    }, //
};
use aosabi_federation::{
    delegate::{
        check_result::merge_data_and_errors,
        create_request::{
            CreateRequestOptions,
            create_request, //
        },
        finalize::finalize_gateway_request,
        prepare::prepare_gateway_document,
        subschema::SchemaOrSubschemaConfig, //
    },
    executor_http::serialize::serialize_execution_request,
    needs::{
        Needs,
        NeedsNode, //
    },
    policy::{
        FieldPolicy,
        Force,
        ForceFn,
        PatchPolicy,
        Policies,
        pass, //
    },
    transforms::{
        DelegationContext,
        ExecutionRequest,
        Transform,
        Variables,
        force_args::ForceArgsTransform,
        inject_needs::InjectNeedsTransform,
        inline_args::InlineArgsTransform,
        patch::{
            PatchApi,
            PatchFn,
            PatchTable,
            PatchTransform, //
        }, //
    }, //
};
use aosabi_http::Context;
use aosabi_json::{
    JsObject,
    JsString,
    JsValue,
    Opaque,
    value::OpaqueHandle, //
};
use aosabi_lang::{
    GraphQLError, GraphQLErrorOptions, ParseOptions, PathSegment,
    ast::{
        DefinitionNode,
        DocumentNode,
        FragmentDefinitionNode,
        OperationDefinitionNode,
        OperationTypeNode,
        SelectionNode, //
    },
    js_string::{
        JsStringBuilder,
        to_string_lossy, //
    },
    parser::{
        parse,
        parse_type, //
    },
    printer::print,
    strip_ignored_characters::strip_ignored_characters,
};
use aosabi_type::{
    BuildSchemaOptions,
    NamedType,
    Schema,
    Type,
    type_from_ast::type_from_ast, //
};
use graphql_over_http::FileBody;

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
const NAME: &str = "federation";

/// The milestone the primitive lands at.
const LANDS_AT: &str = "M10";

/// The operations of the primitive.
const OPERATIONS: &Operations = &[
    ("transform", op_transform),
    ("prepare", op_prepare),
    ("finalize", op_finalize),
    ("create_request", op_create_request),
    ("patch_result", op_patch_result),
    ("merge_data_and_errors", op_merge_data_and_errors),
    ("serialize_request", op_serialize_request),
    ("strip", op_strip),
    ("delegate", op_delegate),
];

/// The command line of the `federation` primitive.
pub static GRAMMAR: Grammar = Grammar {
    options: &[cli::SMOKE, cli::REQUESTS],
    positional: None,
    conflicts: &[],
};

/// What the `federation` primitive accepts.
#[derive(Debug)]
pub struct FederationArgs {
    /// Whether the call asks for the interface line.
    smoke: bool,
    /// The file of request lines of the batch form.
    requests: Option<PathBuf>,
}

impl FederationArgs {
    /// Reads the arguments `matches` holds for [`GRAMMAR`].
    pub fn read(matches: &Matches) -> Self {
        Self {
            smoke: matches.flag("smoke"),
            requests: matches.path("requests"),
        }
    }
}

/// Runs the `federation` primitive.
pub fn run(args: &FederationArgs) -> ExitCode {
    if args.smoke {
        return seam::smoke_at(NAME, LANDS_AT);
    }
    let outcome = seam::required(args.requests.as_deref(), "--requests")
        .and_then(|requests| batch::run(OPERATIONS, requests));
    seam::finish(NAME, outcome)
}

/// The refusal of a request, the text of its `fatal`.
type Fatal = JsString;

/// Returns the `fatal` of `op` with the reason `reason`.
fn fatal(op: &str, reason: impl AsRef<str>) -> Fatal {
    JsString::from(format!("{op}: {}", reason.as_ref()))
}

/*
 * The forced values of a request.
 *
 * A forced function of a policy is a function pointer of the context, so the
 * values one request forces sit in a table the sixteen slot functions read.
 * Every slot counts its call, which is the member `calls` of an answer.
 */

/// What a forced function of a request answers.
enum Forced {
    /// `{"$fn": value}`, the value.
    Value(JsValue),
    /// `{"$ctx": key}`, the member `key` of the context object of the request.
    Member(String),
}

/// The context object of a request, which a forced `$ctx` reads, and `None`
/// for a context that is no object.
struct ContextObject(Option<JsObject>);

thread_local! {
    /// The forced values of the request being answered, one per slot.
    static FORCED: RefCell<Vec<Forced>> = const { RefCell::new(Vec::new()) };
    /// The calls of a forced function the request being answered made.
    static CALLS: Cell<u32> = const { Cell::new(0) };
}

/// Answers the forced value of slot `N` for the context `ctx`.
fn slot<const N: usize>(ctx: &Context) -> Option<JsValue> {
    CALLS.with(|calls| calls.set(calls.get().saturating_add(1)));
    FORCED.with(|forced| match forced.borrow().get(N)? {
        Forced::Value(value) => Some(value.clone()),
        Forced::Member(key) => ctx.extension::<ContextObject>().and_then(|object| {
            object
                .0
                .as_ref()
                .and_then(|object| object.get(key).cloned())
        }),
    })
}

/// The slot functions, the forced functions one request may hold.
const SLOTS: [ForceFn; 16] = [
    slot::<0>, slot::<1>, slot::<2>, slot::<3>, slot::<4>, slot::<5>, slot::<6>, slot::<7>,
    slot::<8>, slot::<9>, slot::<10>, slot::<11>, slot::<12>, slot::<13>, slot::<14>, slot::<15>,
];

/// Empties the table of forced values and the count of their calls.
fn reset_forced() {
    FORCED.with(|forced| forced.borrow_mut().clear());
    CALLS.with(|calls| calls.set(0));
}

/// Returns the slot function of `forced`, the next free slot.
fn push_slot(forced: Forced) -> Result<ForceFn, String> {
    FORCED.with(|slots| {
        let next = slots.borrow().len();
        let function = *SLOTS
            .get(next)
            .ok_or("a request forces more than 16 values through functions")?;
        slots.borrow_mut().push(forced);
        Ok(function)
    })
}

/// Returns the calls the forced functions of the request made, as a JSON number.
fn calls() -> JsValue {
    JsValue::from(f64::from(CALLS.with(Cell::get)))
}

/*
 * The codec.
 */

/// A `Date` of a request, its ISO text, or `None` for an invalid date.
struct RequestDate(Option<String>);

/// Returns the text of `string`, a lone surrogate written as U+FFFD.
fn lossy(string: &JsString) -> String {
    to_string_lossy(string).into_owned()
}

/// Returns the one key object `{tag: value}`.
fn tagged(tag: &str, value: JsValue) -> JsValue {
    let mut out = JsObject::new();
    out.insert(tag, value);
    JsValue::Object(out)
}

/// Returns `{"$undefined": true}`.
fn undefined() -> JsValue {
    tagged("$undefined", JsValue::Bool(true))
}

/// Returns the tag of `object`, its first key when that begins with `$`.
fn tag_of(object: &JsObject) -> Option<String> {
    let (key, _) = object.own_keys().next()?;
    let key = key.as_str()?;
    key.starts_with('$').then(|| key.to_owned())
}

/// Returns the text of a string value, or `None` for any other value.
fn string_of(value: Option<&JsValue>) -> Option<String> {
    match value {
        Some(JsValue::String(string)) => Some(lossy(string)),
        _ => None,
    }
}

/// Returns the string field `name` of `request`, or the `fatal` of its absence.
fn require(request: &JsObject, op: &str, name: &str) -> Result<String, Fatal> {
    string_of(request.get(name))
        .ok_or_else(|| fatal(op, format!("the request carries no string field {name}")))
}

/// Returns the double `at` as a count of 0 to 2^32 - 1, or `None` when it is not one.
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

/// Returns the number a `$number` names, `NaN`, `Infinity`, `-Infinity` or `-0`.
fn special_number(raw: Option<&JsValue>) -> Result<f64, String> {
    match string_of(raw).as_deref() {
        Some("NaN") => Ok(f64::NAN),
        Some("Infinity") => Ok(f64::INFINITY),
        Some("-Infinity") => Ok(f64::NEG_INFINITY),
        Some("-0") => Ok(-0.0),
        other => Err(format!(
            "the number {other:?} is none of NaN, Infinity, -Infinity, -0"
        )),
    }
}

/// Returns the file a `$file` describes, `content` or else `size` bytes of `x`.
fn file_of(raw: Option<&JsValue>) -> Result<FileBody, String> {
    let Some(JsValue::Object(raw)) = raw else {
        return Err(String::from("a $file carries a string name"));
    };
    let name = string_of(raw.get("name")).ok_or("a $file carries a string name")?;
    let content: Vec<u8> = match raw.get("content") {
        Some(JsValue::String(content)) => content.as_wtf8().to_vec(),
        None | Some(JsValue::Undefined) => match raw.get("size") {
            Some(JsValue::Number(size)) => {
                vec![b'x'; whole(*size).ok_or("a $file carries a content or a size")?]
            }
            _ => return Err(String::from("a $file carries a content or a size")),
        },
        Some(_) => return Err(String::from("the content of a $file is a string")),
    };
    let r#type = string_of(raw.get("type")).unwrap_or_default();
    Ok(FileBody::new(&content, &name, &r#type))
}

/// Returns the JSON value a request writes for `raw`, its tags read.
fn decode_js(raw: &JsValue) -> Result<JsValue, String> {
    match raw {
        JsValue::Array(items) => Ok(JsValue::Array(
            items.iter().map(decode_js).collect::<Result<_, _>>()?,
        )),
        JsValue::Object(object) => {
            if let Some(tag) = tag_of(object) {
                return match tag.as_str() {
                    "$undefined" => Ok(JsValue::Undefined),
                    "$number" => special_number(object.get("$number")).map(JsValue::Number),
                    "$date" => Ok(JsValue::Opaque(Opaque(Arc::new(RequestDate(string_of(
                        object.get("$date"),
                    )))))),
                    "$file" => file_of(object.get("$file"))
                        .map(|file| JsValue::Opaque(Opaque(Arc::new(file)))),
                    other => Err(format!("unknown tag {other}")),
                };
            }
            let mut out = JsObject::new();
            for (key, member) in object.own_keys() {
                out.insert(key.clone(), decode_js(member)?);
            }
            Ok(JsValue::Object(out))
        }
        other => Ok(other.clone()),
    }
}

/// Returns the JSON an answer writes for `number`, a tag for one JSON cannot spell.
fn encode_number(number: f64) -> JsValue {
    if number.is_nan() {
        tagged("$number", JsValue::from("NaN"))
    } else if number == f64::INFINITY {
        tagged("$number", JsValue::from("Infinity"))
    } else if number == f64::NEG_INFINITY {
        tagged("$number", JsValue::from("-Infinity"))
    } else if number == 0.0 && number.is_sign_negative() {
        tagged("$number", JsValue::from("-0"))
    } else {
        JsValue::Number(number)
    }
}

/// Returns the JSON an answer writes for a host object, a file or a date.
fn encode_opaque(handle: &OpaqueHandle) -> JsValue {
    if let Some(file) = handle.downcast_ref::<FileBody>() {
        let mut out = JsObject::new();
        out.insert("name", file.name.as_str());
        // A file of a request holds far fewer than 2^32 bytes.
        let size = u32::try_from(file.bytes.len()).unwrap_or(u32::MAX);
        out.insert("size", f64::from(size));
        if !file.r#type.is_empty() {
            out.insert("type", file.r#type.as_str());
        }
        return tagged("$file", JsValue::Object(out));
    }
    if let Some(date) = handle.downcast_ref::<RequestDate>() {
        return tagged(
            "$date",
            date.0.as_deref().map_or(JsValue::Null, JsValue::from),
        );
    }
    JsValue::Object(JsObject::new())
}

/// Returns the JSON an answer writes for the JSON value `value`.
fn encode_js(value: &JsValue) -> JsValue {
    match value {
        JsValue::Undefined => undefined(),
        JsValue::Number(number) => encode_number(*number),
        JsValue::Array(items) => JsValue::Array(items.iter().map(encode_js).collect()),
        JsValue::Object(object) => {
            let mut out = JsObject::new();
            for (key, member) in object.own_keys() {
                out.insert(key.clone(), encode_js(member));
            }
            JsValue::Object(out)
        }
        JsValue::Opaque(opaque) => encode_opaque(&opaque.0),
        other => other.clone(),
    }
}

/// Returns the JSON an answer writes for `variables`, `$undefined` for none.
fn encode_variables(variables: Option<&Variables>) -> JsValue {
    variables.map_or_else(undefined, |variables| {
        encode_js(&JsValue::Object(variables.to_object()))
    })
}

/// The errors a request carried, each with its `ref`, so an answer that hands
/// one back writes the same `ref`.
#[derive(Default)]
struct Session {
    /// The errors of the request and their `ref`.
    errors: Vec<(Rc<ErrorObject>, JsValue)>,
}

impl Session {
    /// Returns the `ref` of `error`, when the request carried it.
    fn ref_of(&self, error: &Rc<ErrorObject>) -> Option<&JsValue> {
        self.errors
            .iter()
            .find(|(known, _)| Rc::ptr_eq(known, error))
            .map(|(_, reference)| reference)
    }
}

/// Returns the path a request writes as a list of keys and indices.
fn path_of(raw: &JsValue) -> Result<Vec<PathSegment>, String> {
    let JsValue::Array(items) = raw else {
        return Err(String::from("a path is a list"));
    };
    items
        .iter()
        .map(|item| match item {
            JsValue::String(key) => Ok(PathSegment::Key(lossy(key))),
            JsValue::Number(index) => whole(*index)
                .map(PathSegment::Index)
                .ok_or_else(|| String::from("a path segment is a key or an index")),
            _ => Err(String::from("a path segment is a key or an index")),
        })
        .collect()
}

/// Returns the path a request writes, `None` for an absent or a null one.
fn optional_path(raw: Option<&JsValue>) -> Result<Option<Vec<PathSegment>>, String> {
    match raw {
        None | Some(JsValue::Undefined | JsValue::Null) => Ok(None),
        Some(path) => path_of(path).map(Some),
    }
}

/// Returns the extensions a request writes, `None` for absent ones or for no object.
fn optional_extensions(raw: Option<&JsValue>) -> Result<Option<JsObject>, String> {
    match raw {
        None | Some(JsValue::Undefined) => Ok(None),
        Some(extensions) => match decode_js(extensions)? {
            JsValue::Object(extensions) => Ok(Some(extensions)),
            _ => Ok(None),
        },
    }
}

/// Returns the error an `$error` describes, a `GraphQLError` with its path and
/// extensions, or a plain error of its name.
fn make_error(raw: Option<&JsValue>) -> Result<ErrorObject, String> {
    let Some(JsValue::Object(raw)) = raw else {
        return Err(String::from("an $error carries a string message"));
    };
    let message = string_of(raw.get("message")).ok_or("an $error carries a string message")?;
    let name = string_of(raw.get("name"));
    if name.as_deref() == Some("GraphQLError") {
        return Ok(ErrorObject::GraphQL(GraphQLError::new(
            message,
            GraphQLErrorOptions {
                path: optional_path(raw.get("path"))?,
                extensions: optional_extensions(raw.get("extensions"))?,
                ..GraphQLErrorOptions::default()
            },
        )));
    }
    let name = match name {
        Some(name) if name != "Error" => ErrorName::Other(JsString::from(name)),
        _ => ErrorName::Error,
    };
    Ok(ErrorObject::Plain {
        name,
        message: JsString::from(message),
    })
}

/// Returns the value of the executor a request writes for `raw`, its errors
/// kept in `session`.
fn decode_value(raw: &JsValue, session: &mut Session) -> Result<Value, String> {
    match raw {
        JsValue::Array(items) => Ok(Value::from(
            items
                .iter()
                .map(|item| decode_value(item, session))
                .collect::<Result<Vec<_>, _>>()?,
        )),
        JsValue::Object(object) => {
            if let Some(tag) = tag_of(object) {
                if tag == "$error" {
                    let error = Rc::new(make_error(object.get("$error"))?);
                    if let Some(reference) = object.get("ref")
                        && !reference.is_undefined()
                    {
                        session.errors.push((Rc::clone(&error), reference.clone()));
                    }
                    return Ok(Value::Error(error));
                }
                return Ok(Value::from_js(&decode_js(raw)?));
            }
            let mut entries = Vec::new();
            for (key, member) in object.own_keys() {
                entries.push((lossy(key), decode_value(member, session)?));
            }
            Ok(Value::from(PlainObject::from_entries(entries)))
        }
        other => Ok(Value::from_js(other)),
    }
}

/// Returns a thrown value as JSON, `name` first and then what the error's own
/// `toJSON` writes, or `name` null and the value for a value that is no error.
fn error_json(value: &Value, session: &Session) -> JsValue {
    let Value::Error(error) = value else {
        let mut out = JsObject::new();
        out.insert("name", JsValue::Null);
        out.insert("value", encode_value(value, session));
        return JsValue::Object(out);
    };
    let mut out = JsObject::new();
    out.insert("name", error.name());
    match error.as_ref() {
        ErrorObject::GraphQL(graphql) => {
            for (key, member) in &JsObject::from(&graphql.to_json()) {
                out.insert(key.clone(), member.clone());
            }
        }
        ErrorObject::Aggregate { message, errors } => {
            out.insert("message", message.clone());
            out.insert(
                "errors",
                JsValue::Array(errors.iter().map(|one| error_json(one, session)).collect()),
            );
        }
        other => {
            out.insert("message", other.message());
        }
    }
    JsValue::Object(out)
}

/// Returns the JSON an answer writes for the value of the executor `value`.
fn encode_value(value: &Value, session: &Session) -> JsValue {
    match value {
        Value::Undefined => undefined(),
        Value::Null => JsValue::Null,
        Value::Bool(boolean) => JsValue::Bool(*boolean),
        Value::Number(number) => encode_number(*number),
        Value::String(string) => JsValue::String(string.clone()),
        Value::Date(date) => tagged(
            "$date",
            date.to_iso_string().map_or(JsValue::Null, JsValue::from),
        ),
        Value::List(items) => JsValue::Array(
            items
                .iter()
                .map(|item| encode_value(item, session))
                .collect(),
        ),
        Value::Object(object) => {
            let mut out = JsObject::new();
            for key in object.own_keys() {
                let member = Rc::clone(object).get(&key);
                out.insert(key.as_str(), encode_value(&member, session));
            }
            JsValue::Object(out)
        }
        Value::Function(_) => tagged("$function", JsValue::Null),
        Value::Error(error) => {
            let mut out = JsObject::new();
            out.insert("$error", error_json(value, session));
            if let Some(reference) = session.ref_of(error) {
                out.insert("ref", reference.clone());
            }
            JsValue::Object(out)
        }
        Value::Host(HostValue::Opaque(handle)) => encode_opaque(handle),
        Value::Promise(_) | Value::Host(_) => JsValue::Object(JsObject::new()),
    }
}

/// Reports whether `a` and `b` are the same value as `===` compares them.
// Two numbers are the same value under `===` when they are equal as doubles,
// with no margin, which is the comparison the operator makes.
#[expect(clippy::float_cmp)]
fn strict_equals(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Undefined, Value::Undefined) | (Value::Null, Value::Null) => true,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Number(a), Value::Number(b)) => a == b,
        (Value::String(a), Value::String(b)) => a == b,
        (Value::Object(a), Value::Object(b)) => std::ptr::addr_eq(Rc::as_ptr(a), Rc::as_ptr(b)),
        (Value::List(a), Value::List(b)) => Rc::ptr_eq(a, b),
        (Value::Error(a), Value::Error(b)) => Rc::ptr_eq(a, b),
        (Value::Host(HostValue::Opaque(a)), Value::Host(HostValue::Opaque(b))) => {
            std::ptr::addr_eq(Arc::as_ptr(a), Arc::as_ptr(b))
        }
        _ => false,
    }
}

/*
 * The patch table of the sidecar.
 */

/// Returns the member `key` of `data`, `undefined` for a value that is no object.
fn member(data: &Value, key: &str) -> Value {
    match data {
        Value::Object(object) => Rc::clone(object).get(key),
        _ => Value::Undefined,
    }
}

/// Returns a new object with the own members of `data`, `{...data}`.
fn spread(data: &Value) -> Rc<PlainObject> {
    let out = PlainObject::new();
    if let Value::Object(object) = data {
        for key in object.own_keys() {
            out.set(&key, Rc::clone(object).get(&key));
        }
    }
    out
}

/// Returns the patch the sidecar names `id`.
fn patch(id: &str) -> Result<PatchFn, String> {
    let patch: PatchFn = match id {
        "identity" | "passthrough" => Rc::new(|data, _api| Ok(data)),
        "spread_full_name" => Rc::new(|data, _api| {
            let mut full = JsStringBuilder::new();
            full.push(&to_js_string(&member(&data, "firstName")));
            full.push_str(" ");
            full.push(&to_js_string(&member(&data, "lastName")));
            let out = spread(&data);
            out.set("fullName", Value::String(full.finish()));
            Ok(Value::from(out))
        }),
        "stamp_kind" => Rc::new(|data, _api| {
            let doctor = matches!(member(&data, "kind"), Value::String(kind) if kind == "doctor");
            let out = PlainObject::new();
            let (typename, last) = if doctor {
                ("DoctorProfile", "specialty")
            } else {
                ("PatientProfile", "insuranceId")
            };
            out.set("__typename", Value::String(JsString::from(typename)));
            out.set("id", member(&data, "id"));
            out.set("email", member(&data, "email"));
            out.set(last, member(&data, last));
            Ok(Value::from(out))
        }),
        "nested_org" => Rc::new(|data, _api| {
            let out = spread(&data);
            let org = Function::new("org", |_agent, _args| {
                aosabi_execute::value::Completion::Normal(Value::Undefined)
            });
            out.set("org", Value::Function(org));
            Ok(Value::from(out))
        }),
        "returns_array" => Rc::new(|data, _api| Ok(Value::from(vec![data]))),
        "returns_null" => Rc::new(|_data, _api| Ok(Value::Null)),
        "returns_string" => Rc::new(|_data, _api| Ok(Value::String(JsString::from("patched")))),
        "throws_plain" => Rc::new(|_data, _api| Err(Thrown::error("patch failed"))),
        "throws_graphql" => Rc::new(|_data, _api| {
            let mut extensions = JsObject::new();
            extensions.insert("code", "PATCH_REFUSED");
            let error = GraphQLError::new(
                "patch refused",
                GraphQLErrorOptions {
                    extensions: Some(extensions),
                    ..GraphQLErrorOptions::default()
                },
            );
            Err(Thrown(Value::from(ErrorObject::GraphQL(error))))
        }),
        "project_id" => Rc::new(|data, _api| {
            let out = PlainObject::new();
            out.set("__typename", Value::String(JsString::from("Product")));
            out.set("id", member(&data, "id"));
            Ok(Value::from(out))
        }),
        "project_handle" => Rc::new(|data, _api| {
            let out = PlainObject::new();
            out.set("handle", member(&data, "handle"));
            Ok(Value::from(out))
        }),
        other => return Err(format!("no patch has the id {other:?}")),
    };
    Ok(patch)
}

/*
 * The policies, the patches and the needs of a request.
 */

/// Returns the forced value a policy of a request writes.
fn decode_forced(raw: &JsValue) -> Result<Force, String> {
    if let JsValue::Object(object) = raw
        && let Some(tag) = tag_of(object)
    {
        match tag.as_str() {
            "$fn" => {
                let value = decode_js(object.get("$fn").unwrap_or(&JsValue::Undefined))?;
                return Ok(Force::Context(push_slot(Forced::Value(value))?));
            }
            "$ctx" => {
                let key = string_of(object.get("$ctx")).ok_or("a $ctx names a string key")?;
                return Ok(Force::Context(push_slot(Forced::Member(key))?));
            }
            _ => {}
        }
    }
    Ok(Force::Value(decode_js(raw)?))
}

/// Returns the policy of one field, `{"args", "force"}`.
fn decode_policy(raw: &JsValue) -> Result<FieldPolicy, String> {
    let JsValue::Object(raw) = raw else {
        return Err(String::from("a policy is an object"));
    };
    let args = match raw.get("args") {
        None | Some(JsValue::Undefined | JsValue::Null) => None,
        Some(JsValue::Array(items)) => Some(
            items
                .iter()
                .map(|item| {
                    string_of(Some(item))
                        .map(Cow::Owned)
                        .ok_or("an allowed name is a string")
                })
                .collect::<Result<Vec<_>, _>>()?,
        ),
        Some(_) => return Err(String::from("the args of a policy is a list")),
    };
    let force = match raw.get("force") {
        None | Some(JsValue::Undefined | JsValue::Null) => None,
        Some(JsValue::Object(force)) => {
            let mut out = Vec::new();
            for (name, value) in force.own_keys() {
                out.push((Cow::Owned(lossy(name)), decode_forced(value)?));
            }
            Some(out)
        }
        Some(_) => return Err(String::from("the force of a policy is an object")),
    };
    Ok(FieldPolicy { args, force })
}

/// Returns the policies of a request, keyed `<Type>.<field>` as a federation
/// collects them.
fn decode_policies(raw: Option<&JsValue>) -> Result<Option<Rc<Policies>>, String> {
    let object = match raw {
        None | Some(JsValue::Undefined | JsValue::Null) => return Ok(None),
        Some(JsValue::Object(object)) => object,
        Some(_) => return Err(String::from("policies is an object of Type.field keys")),
    };
    let mut policies = Policies::new();
    for (key, policy) in object.own_keys() {
        policies.set(lossy(key), decode_policy(policy)?);
    }
    Ok(Some(Rc::new(policies)))
}

/// Returns the policy `pass` attaches to a patch, by field name.
fn decode_patch_policy(raw: &JsObject) -> Result<PatchPolicy, String> {
    let mut out = Vec::new();
    for (name, one) in raw.own_keys() {
        out.push((Cow::Owned(lossy(name)), decode_policy(one)?));
    }
    Ok(out)
}

/// Returns the patch table of a request, an id or `{"id", "policy"}` per type.
fn decode_patches(raw: Option<&JsValue>) -> Result<Option<Rc<PatchTable>>, String> {
    let object = match raw {
        None | Some(JsValue::Undefined | JsValue::Null) => return Ok(None),
        Some(JsValue::Object(object)) => object,
        Some(_) => return Err(String::from("patches is an object of type names")),
    };
    let mut table = PatchTable::new();
    for (type_name, spec) in object.own_keys() {
        let type_name = lossy(type_name);
        match spec {
            JsValue::String(id) => table.insert(type_name, patch(&lossy(id))?, None),
            JsValue::Object(spec) => {
                let id = string_of(spec.get("id")).ok_or("a patch names a string id")?;
                let one = patch(&id)?;
                match spec.get("policy") {
                    None | Some(JsValue::Undefined) => table.insert(type_name, one, None),
                    Some(JsValue::Object(policy)) => {
                        let (one, policy) = pass(one, decode_patch_policy(policy)?).into_parts();
                        table.insert(type_name, one, Some(policy));
                    }
                    Some(_) => {
                        return Err(format!(
                            "the policy of {type_name} is an object of field names"
                        ));
                    }
                }
            }
            _ => return Err(format!("the patch of {type_name} is an id or an object")),
        }
    }
    Ok(Some(Rc::new(table)))
}

/// Reports whether `value` is truthy as JavaScript reads it.
fn is_truthy(value: &JsValue) -> bool {
    match value {
        JsValue::Undefined | JsValue::Null => false,
        JsValue::Bool(boolean) => *boolean,
        JsValue::Number(number) => *number != 0.0 && !number.is_nan(),
        JsValue::String(string) => !string.is_empty(),
        _ => true,
    }
}

/// Returns the own members of `raw` as `Object.entries` reads them, the
/// indices of a list and of a string and no member of any other value.
fn entries_of(raw: &JsValue) -> JsObject {
    match raw {
        JsValue::Object(object) => object.clone(),
        JsValue::Array(items) => items
            .iter()
            .enumerate()
            .map(|(index, item)| (index.to_string(), item.clone()))
            .collect(),
        JsValue::String(string) => lossy(string)
            .encode_utf16()
            .enumerate()
            .map(|(index, unit)| {
                (
                    index.to_string(),
                    JsValue::String(JsString::from_utf16(&[unit])),
                )
            })
            .collect(),
        _ => JsObject::new(),
    }
}

/// Returns the node of the key `name` of a needs object, whose value is `value`.
fn node_of(name: &JsString, value: &JsValue) -> NeedsNode {
    let selected = is_truthy(value);
    let (args, children) = match value {
        _ if !selected => (None, Vec::new()),
        JsValue::Object(object) => (
            object
                .get("__args")
                .filter(|args| is_truthy(args))
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

/// Returns the nodes of the own members of a needs object.
fn nodes_of(object: &JsObject) -> Vec<NeedsNode> {
    object
        .own_keys()
        .map(|(key, value)| node_of(key, value))
        .collect()
}

/// Returns the needs a request writes, `None` for a value that is not truthy.
fn needs_of(raw: &JsValue) -> Option<Needs> {
    is_truthy(raw).then(|| Needs::new(nodes_of(&entries_of(raw))))
}

/*
 * The schemas, the documents and the resolve information of a request.
 */

/// Returns the schema of the SDL in the field `name` of `request`.
fn schema_of(request: &JsObject, op: &str, name: &str) -> Result<Arc<Schema>, Fatal> {
    let sdl = require(request, op, name)?;
    aosabi_validate::build_schema(
        sdl.as_str(),
        BuildSchemaOptions::default(),
        ParseOptions::default(),
    )
    .map(Arc::new)
    .map_err(|error| fatal(op, format!("{name} does not build: {error}")))
}

/// Returns the document of the field `name` of `request`, parsed with locations.
fn document_of(request: &JsObject, op: &str, name: &str) -> Result<DocumentNode, Fatal> {
    let source = batch::require_source(request, name).map_err(JsString::from)?;
    parse(source, ParseOptions::default()).map_err(|error| {
        fatal(
            op,
            format!("{name} does not parse: {}", lossy(&error.message)),
        )
    })
}

/// Returns the variables of `request` decoded, `None` when it names none.
fn variables_of(request: &JsObject, op: &str, name: &str) -> Result<Option<JsObject>, Fatal> {
    match request.get(name) {
        None | Some(JsValue::Undefined) => Ok(None),
        Some(raw) => match decode_js(raw).map_err(|reason| fatal(op, reason))? {
            JsValue::Object(variables) => Ok(Some(variables)),
            JsValue::Undefined => Ok(None),
            _ => Err(fatal(op, format!("{name} is an object"))),
        },
    }
}

/// Returns the object type of the query root of `schema`, the parent type a
/// resolve information of a request carries.
fn query_root(schema: &Schema, op: &str) -> Result<Arc<aosabi_type::ObjectType>, Fatal> {
    match schema.get_root_type(OperationTypeNode::Query) {
        Some(NamedType::Object(object)) => Ok(Arc::clone(object)),
        _ => Err(fatal(op, "the target schema has no query type")),
    }
}

/// Returns the first operation of `document`, or the operation of
/// `{ __typename }` for a document that holds none.
fn first_operation(
    document: &DocumentNode,
    op: &str,
) -> Result<Rc<OperationDefinitionNode>, Fatal> {
    let first = |document: &DocumentNode| {
        document
            .definitions
            .iter()
            .find_map(|definition| match definition {
                DefinitionNode::OperationDefinition(operation) => Some(Rc::new(operation.clone())),
                _ => None,
            })
    };
    if let Some(operation) = first(document) {
        return Ok(operation);
    }
    parse("{ __typename }", ParseOptions::default())
        .ok()
        .as_ref()
        .and_then(first)
        .ok_or_else(|| fatal(op, "the document of { __typename } does not parse"))
}

/// Returns a resolve information whose members a delegation reads, the
/// variable values and the operation, beside a root field of the query root
/// of `schema`.
///
/// The sidecar hands `createRequest` and `finalizeGatewayRequest` an object of
/// the two members alone, `{variableValues, operation}`, so the other members
/// are only what the type of the executor asks for.
fn resolve_info(
    schema: &Arc<Schema>,
    op: &str,
    operation: Rc<OperationDefinitionNode>,
    field_nodes: FieldNodes,
    variable_values: JsObject,
) -> Result<Rc<ResolveInfo>, Fatal> {
    let parent_type = query_root(schema, op)?;
    let field_name = field_nodes.first().map_or_else(
        || String::from("__typename"),
        |node| String::from(node.name.value()),
    );
    Ok(Rc::new(ResolveInfo {
        path: add_path(
            None,
            PathSegment::Key(field_name.clone()),
            Some(&parent_type.name),
        ),
        return_type: Type::from(&NamedType::Object(Arc::clone(&parent_type))),
        field_name,
        field_nodes,
        parent_type,
        schema: Arc::clone(schema),
        fragments: Rc::default(),
        root_value: Value::Undefined,
        operation,
        variable_values: Rc::new(variable_values),
    }))
}

/// Returns the delegation context of a request, with the members the
/// transforms and the finalization read, the target schema, the field name
/// and the resolve information.
fn delegation_context(
    target_schema: &Arc<Schema>,
    op: &str,
    field_name: String,
    info: Option<Rc<ResolveInfo>>,
) -> Result<DelegationContext, Fatal> {
    let return_type = Type::from(&NamedType::Object(query_root(target_schema, op)?));
    Ok(DelegationContext {
        subschema: SchemaOrSubschemaConfig::Schema(Arc::clone(target_schema)),
        subschema_config: None,
        target_schema: Arc::clone(target_schema),
        operation: OperationTypeNode::Query,
        field_name,
        args: None,
        context: None,
        info,
        return_type,
        transforms: Vec::new(),
        transformed_schema: Arc::clone(target_schema),
        skip_type_merging: false,
    })
}

/// Returns the context of a forced function, carrying the context object of
/// the request in an extension slot.
fn context_of(raw: Option<&JsValue>) -> Result<Context, String> {
    let ctx = service_context();
    let object = match raw.map(decode_js).transpose()? {
        Some(JsValue::Object(object)) => Some(object),
        _ => None,
    };
    ctx.set_extension(Rc::new(ContextObject(object)));
    Ok(ctx)
}

/// Returns the payload of a refusal, `{"thrown": ...}`.
fn thrown(error: &Value, session: &Session) -> JsObject {
    let mut payload = JsObject::new();
    payload.insert("thrown", error_json(error, session));
    payload
}

/*
 * The operations.
 */

/// The names of the four transforms, in the array order a federation hands the
/// delegation.
const TRANSFORMS: [&str; 4] = ["inject_needs", "inline_args", "force_args", "patch"];

/// Answers `transform`, the request half of the transforms the request names,
/// in the reversed order of their list, as the transformer of a delegation
/// runs them.
fn op_transform(request: &JsObject) -> Result<JsObject, Fatal> {
    const OP: &str = "transform";
    reset_forced();
    let target_schema = schema_of(request, OP, "target_sdl")?;
    let field_name = require(request, OP, "field_name")?;
    let names: Vec<String> = match request.get("transforms") {
        None | Some(JsValue::Undefined) => {
            TRANSFORMS.iter().map(|name| String::from(*name)).collect()
        }
        Some(JsValue::Array(items)) => items
            .iter()
            .map(|item| {
                string_of(Some(item))
                    .filter(|name| TRANSFORMS.contains(&name.as_str()))
                    .ok_or_else(|| {
                        fatal(
                            OP,
                            "transforms is a list of inject_needs, inline_args, force_args, patch",
                        )
                    })
            })
            .collect::<Result<_, _>>()?,
        Some(_) => return Err(fatal(OP, "transforms is a list")),
    };
    let missing = JsValue::Undefined;
    let mut made: Vec<Rc<dyn Transform>> = Vec::with_capacity(names.len());
    for name in &names {
        let transform: Rc<dyn Transform> = match name.as_str() {
            "inject_needs" => {
                let needs = decode_js(request.get("needs").unwrap_or(&missing))
                    .map_err(|reason| fatal(OP, reason))?;
                Rc::new(InjectNeedsTransform::new(needs_of(&needs)))
            }
            "inline_args" => {
                let args = decode_js(request.get("args").unwrap_or(&missing))
                    .map_err(|reason| fatal(OP, reason))?;
                Rc::new(InlineArgsTransform::new(match args {
                    JsValue::Object(args) => Some(args),
                    _ => None,
                }))
            }
            "force_args" => Rc::new(ForceArgsTransform::new(
                decode_policies(request.get("policies")).map_err(|reason| fatal(OP, reason))?,
                context_of(request.get("ctx")).map_err(|reason| fatal(OP, reason))?,
            )),
            _ => Rc::new(PatchTransform::new(
                decode_patches(request.get("patches")).map_err(|reason| fatal(OP, reason))?,
                Rc::new(()) as PatchApi,
                matches!(request.get("strict"), Some(JsValue::Bool(true))),
            )),
        };
        made.push(transform);
    }
    let mut original = ExecutionRequest::new(Rc::new(document_of(request, OP, "document")?));
    original.variables = variables_of(request, OP, "variables")?.map(Variables::from);
    let original_document = Rc::clone(&original.document);
    let context = delegation_context(&target_schema, OP, field_name, None)?;
    let mut current = original;
    let mut payload = JsObject::new();
    for transform in made.iter().rev() {
        match transform.transform_request(current, &context) {
            Ok(next) => current = next,
            Err(error) => {
                payload.insert("thrown", error_json(&error.0, &Session::default()));
                payload.insert("calls", calls());
                return Ok(payload);
            }
        }
    }
    payload.insert("document", print(&*current.document));
    payload.insert("variables", encode_variables(current.variables.as_ref()));
    payload.insert(
        "unchanged",
        Rc::ptr_eq(&current.document, &original_document),
    );
    payload.insert("calls", calls());
    Ok(payload)
}

/// Answers `prepare`, `prepare_gateway_document` over a document, the target
/// schema and a return type read in the schema of `info_sdl` or the target.
fn op_prepare(request: &JsObject) -> Result<JsObject, Fatal> {
    const OP: &str = "prepare";
    let target_schema = schema_of(request, OP, "target_sdl")?;
    let info_schema = match request.get("info_sdl") {
        None | Some(JsValue::Undefined) => None,
        Some(_) => Some(schema_of(request, OP, "info_sdl")?),
    };
    let text = require(request, OP, "return_type")?;
    let type_node = parse_type(text.as_str(), ParseOptions::default()).map_err(|error| {
        fatal(
            OP,
            format!("return_type does not parse: {}", lossy(&error.message)),
        )
    })?;
    let return_type = type_from_ast(info_schema.as_ref().unwrap_or(&target_schema), &type_node)
        .ok_or_else(|| fatal(OP, format!("the schema has no type {text}")))?;
    let document = document_of(request, OP, "document")?;
    match prepare_gateway_document(
        &document,
        &[],
        &target_schema,
        &return_type,
        info_schema.as_ref(),
    ) {
        Ok(prepared) => {
            let mut payload = JsObject::new();
            payload.insert("document", print(&prepared));
            Ok(payload)
        }
        Err(error) => Ok(thrown(&error, &Session::default())),
    }
}

/// Answers `finalize`, `finalize_gateway_request` with the target schema and,
/// when the request names `outer_variables`, the variable values of the client.
fn op_finalize(request: &JsObject) -> Result<JsObject, Fatal> {
    const OP: &str = "finalize";
    let target_schema = schema_of(request, OP, "target_sdl")?;
    let mut original = ExecutionRequest::new(Rc::new(document_of(request, OP, "document")?));
    original.variables = variables_of(request, OP, "variables")?.map(Variables::from);
    let info = match variables_of(request, OP, "outer_variables")? {
        Some(outer) => Some(resolve_info(
            &target_schema,
            OP,
            first_operation(&original.document, OP)?,
            Rc::from([]),
            outer,
        )?),
        None => None,
    };
    let context = delegation_context(&target_schema, OP, String::new(), info)?;
    let mut overlapping = false;
    match finalize_gateway_request(original, &context, &mut || overlapping = true) {
        Ok(finalized) => {
            let mut payload = JsObject::new();
            payload.insert("document", print(&*finalized.document));
            payload.insert("variables", encode_variables(finalized.variables.as_ref()));
            payload.insert("overlapping_aliases", overlapping);
            Ok(payload)
        }
        Err(error) => Ok(thrown(&error, &Session::default())),
    }
}

/// Returns the operation type a request names in `target_operation`.
fn operation_type(
    raw: Option<&JsValue>,
    default: OperationTypeNode,
) -> Result<OperationTypeNode, Fatal> {
    match string_of(raw).as_deref() {
        None => Ok(default),
        Some("query") => Ok(OperationTypeNode::Query),
        Some("mutation") => Ok(OperationTypeNode::Mutation),
        Some("subscription") => Ok(OperationTypeNode::Subscription),
        Some(other) => Err(fatal(
            "create_request",
            format!("target_operation {other:?} is none of query, mutation, subscription"),
        )),
    }
}

/// Answers `create_request` for the root field of the client document whose
/// response key is `field_name`, with the client's variables as the variable
/// values of the resolve information and the fragments of the document in
/// document order, which is what a delegation hands the function.
fn op_create_request(request: &JsObject) -> Result<JsObject, Fatal> {
    const OP: &str = "create_request";
    let target_schema = schema_of(request, OP, "target_sdl")?;
    let document = document_of(request, OP, "document")?;
    let operation_name = string_of(request.get("operation_name"));
    let operation = get_operation_ast(&document, operation_name.as_deref())
        .ok_or_else(|| fatal(OP, "the document names no operation"))?;
    let response_key = require(request, OP, "field_name")?;
    let field_nodes: Vec<_> = operation
        .selection_set
        .selections
        .iter()
        .filter_map(|selection| match selection {
            SelectionNode::Field(field) => {
                let key = field.alias.as_ref().unwrap_or(&field.name).value();
                (key == response_key).then(|| Rc::new(field.clone()))
            }
            _ => None,
        })
        .collect();
    if field_nodes.is_empty() {
        return Err(fatal(OP, format!("no root field answers {response_key}")));
    }
    let field_nodes: FieldNodes = Rc::from(field_nodes);
    let fragments: Rc<[Rc<FragmentDefinitionNode>]> = document
        .definitions
        .iter()
        .filter_map(|definition| match definition {
            DefinitionNode::FragmentDefinition(fragment) => Some(Rc::new(fragment.clone())),
            _ => None,
        })
        .collect();
    let variables = variables_of(request, OP, "variables")?.unwrap_or_default();
    let target_operation = operation_type(request.get("target_operation"), operation.operation)?;
    let target_field_name = string_of(request.get("target_field_name"))
        .unwrap_or_else(|| String::from(field_nodes[0].name.value()));
    let args = match decode_js(request.get("args").unwrap_or(&JsValue::Undefined))
        .map_err(|reason| fatal(OP, reason))?
    {
        JsValue::Object(args) => Some(args),
        _ => None,
    };
    let info = resolve_info(
        &target_schema,
        OP,
        Rc::new(operation.clone()),
        Rc::clone(&field_nodes),
        variables,
    )?;
    let options = CreateRequestOptions {
        subgraph_name: None,
        fragments: Some(fragments),
        root_value: Value::Undefined,
        target_operation_name: operation
            .name
            .as_ref()
            .map(|name| String::from(name.value())),
        target_operation,
        target_schema: Some(target_schema),
        target_field_name: Some(target_field_name),
        selection_set: None,
        field_nodes: Some(field_nodes),
        context: None,
        info: Some(info),
        args,
    };
    match create_request(options) {
        Ok(created) => {
            let mut payload = JsObject::new();
            payload.insert("document", print(&created.whole_document()));
            payload.insert("variables", encode_variables(created.variables.as_ref()));
            Ok(payload)
        }
        Err(error) => Ok(thrown(&error, &Session::default())),
    }
}

/// Answers `patch_result`, the result half of `PatchTransform` over a tree.
fn op_patch_result(request: &JsObject) -> Result<JsObject, Fatal> {
    const OP: &str = "patch_result";
    reset_forced();
    let mut session = Session::default();
    let patches = decode_patches(request.get("patches")).map_err(|reason| fatal(OP, reason))?;
    let strict = matches!(request.get("strict"), Some(JsValue::Bool(true)));
    let transform = PatchTransform::new(patches, Rc::new(()) as PatchApi, strict);
    let input = decode_value(
        request.get("result").unwrap_or(&JsValue::Undefined),
        &mut session,
    )
    .map_err(|reason| fatal(OP, reason))?;
    let schema = aosabi_validate::build_schema(
        "type Query { _: Boolean }",
        BuildSchemaOptions::default(),
        ParseOptions::default(),
    )
    .map(Arc::new)
    .map_err(|error| {
        fatal(
            OP,
            format!("the schema of the context does not build: {error}"),
        )
    })?;
    let context = delegation_context(&schema, OP, String::from("_"), None)?;
    let mut payload = JsObject::new();
    match transform.transform_result(input.clone(), &context) {
        Ok(result) => {
            payload.insert("result", encode_value(&result, &session));
            payload.insert("unchanged", strict_equals(&result, &input));
        }
        Err(error) => {
            payload.insert("thrown", error_json(&error.0, &session));
        }
    }
    Ok(payload)
}

/// Answers `merge_data_and_errors` over the data of one root field and the
/// errors of a response, each built as the HTTP executor builds an error it
/// reads, `createGraphQLError(message, rest)`, which takes the path and the
/// extensions and no `locations`.
fn op_merge_data_and_errors(request: &JsObject) -> Result<JsObject, Fatal> {
    const OP: &str = "merge_data_and_errors";
    let Some(JsValue::Array(raw_errors)) = request.get("errors") else {
        return Err(fatal(OP, "errors is a list"));
    };
    let mut errors = Vec::with_capacity(raw_errors.len());
    for raw in raw_errors {
        let JsValue::Object(raw) = raw else {
            return Err(fatal(OP, "an error carries a string message"));
        };
        let message = string_of(raw.get("message"))
            .ok_or_else(|| fatal(OP, "an error carries a string message"))?;
        let options = GraphQLErrorOptions {
            path: optional_path(raw.get("path")).map_err(|reason| fatal(OP, reason))?,
            extensions: optional_extensions(raw.get("extensions"))
                .map_err(|reason| fatal(OP, reason))?,
            ..GraphQLErrorOptions::default()
        };
        errors.push(Value::from(ErrorObject::GraphQL(
            aosabi_federation::utils::create_graphql_error(message, options),
        )));
    }
    let mut session = Session::default();
    let data = decode_value(
        request.get("data").unwrap_or(&JsValue::Undefined),
        &mut session,
    )
    .map_err(|reason| fatal(OP, reason))?;
    let path = optional_path(request.get("path")).map_err(|reason| fatal(OP, reason))?;
    // The reference starts the walk at the segment 1 of an error's path, its
    // default `index = 1`, since the segment 0 is the response key of the
    // remote's root field, which the path of the front replaces.
    match merge_data_and_errors(data, &errors, path.as_deref(), 1) {
        Ok(merged) => {
            let mut payload = JsObject::new();
            payload.insert("data", encode_value(&merged.data, &session));
            payload.insert(
                "unpathed_errors",
                JsValue::Array(
                    merged
                        .unpathed_errors
                        .iter()
                        .map(|one| error_json(one, &session))
                        .collect(),
                ),
            );
            Ok(payload)
        }
        Err(error) => Ok(thrown(&error, &session)),
    }
}

/// Answers `serialize_request`, the body the HTTP executor writes for a
/// request with no file, `JSON.stringify` of `serializeExecutionRequest`.
fn op_serialize_request(request: &JsObject) -> Result<JsObject, Fatal> {
    const OP: &str = "serialize_request";
    let mut execution_request =
        ExecutionRequest::new(Rc::new(document_of(request, OP, "document")?));
    execution_request.variables = variables_of(request, OP, "variables")?
        .map(|variables| Variables::from(json_form_object(&variables)));
    execution_request.operation_name = string_of(request.get("operation_name"));
    execution_request.extensions = variables_of(request, OP, "extensions")?;
    let exclude_query = matches!(request.get("exclude_query"), Some(JsValue::Bool(true)));
    let body = serialize_execution_request(&execution_request, exclude_query, None)
        .map_err(|error| fatal(OP, lossy(&error_message(&error))))?;
    let mut payload = JsObject::new();
    payload.insert("body", body.to_json());
    Ok(payload)
}

/// Returns `value` as `JSON.stringify` reads it before it writes, a date of
/// the request as what its `toJSON` answers, its ISO text or null for an
/// invalid date.
fn json_form(value: &JsValue) -> JsValue {
    match value {
        JsValue::Array(items) => JsValue::Array(items.iter().map(json_form).collect()),
        JsValue::Object(object) => JsValue::Object(json_form_object(object)),
        JsValue::Opaque(opaque) => match opaque.0.downcast_ref::<RequestDate>() {
            Some(date) => date.0.as_deref().map_or(JsValue::Null, JsValue::from),
            None => value.clone(),
        },
        other => other.clone(),
    }
}

/// Returns the members of `object` in the form [`json_form`] gives a value.
fn json_form_object(object: &JsObject) -> JsObject {
    object
        .own_keys()
        .map(|(key, member)| (key.clone(), json_form(member)))
        .collect()
}

/// Returns the message of a thrown value.
fn error_message(error: &Value) -> JsString {
    match error {
        Value::Error(error) => error.message(),
        other => to_js_string(other),
    }
}

/// Answers `strip`, `strip_ignored_characters` of `source`, or the syntax
/// error the lexer refuses it with.
fn op_strip(request: &JsObject) -> Result<JsObject, Fatal> {
    let source = batch::require_source(request, "source").map_err(JsString::from)?;
    let mut payload = JsObject::new();
    match strip_ignored_characters(source) {
        Ok(stripped) => {
            payload.insert("stripped", stripped);
        }
        Err(error) => {
            payload.insert("errors", vec![seam::graphql_error_json(&error)]);
        }
    }
    Ok(payload)
}

/// Answers `delegate`, a front execution whose root resolvers delegate
/// through a configured federation, or a delegation outside a resolver.
///
/// The runtime of `aosabi-serve` and the client of `aosabi-fetch` build on
/// Linux alone, which the request of the remote stub needs, so the operation
/// answers there.
#[cfg(target_os = "linux")]
fn op_delegate(request: &JsObject) -> Result<JsObject, Fatal> {
    delegation::answer(request)
}

/// Answers `delegate` with the `fatal` of a target with no runtime.
///
/// The runtime of `aosabi-serve` and the client of `aosabi-fetch` build on
/// Linux alone, so a build for any other target has no network to reach the
/// remote stub through.
#[cfg(not(target_os = "linux"))]
fn op_delegate(_request: &JsObject) -> Result<JsObject, Fatal> {
    Err(fatal(
        "delegate",
        "runs on Linux alone, where the runtime of aosabi-serve and the client of aosabi-fetch build",
    ))
}

/*
 * The operation delegate.
 *
 * The front of a request is built as a served app builds its schema, its
 * root objects go through the resolver wrapping of aosabi-core, and every
 * delegate spec of a root calls the configured federation through its
 * untyped seam, which reads the key, the arguments and the needs as data.
 * The execution runs on the runtime of aosabi-serve with the client of
 * aosabi-fetch as the network capability, so the federation reaches the
 * remote stub over the loopback as a served card reaches its remote.
 */

#[cfg(target_os = "linux")]
mod delegation {
    use std::{
        any::Any,
        cell::RefCell,
        rc::Rc,
        sync::Arc, //
    };

    use aosabi_core::{
        Context,
        Resolvers,
        Thrown,
        context::{
            ambient_hooks,
            run_with_context,
            with_context, //
        },
        create_schema::{
            CreateSchemaError,
            ResolverMap,
            create_schema, //
        },
        net::install_fetch,
        remote::Registry,
        resolvers_to_graphql_resolvers,
        scalars::leaf_serializers,
        service::CoreState, //
    };
    use aosabi_execute::{
        errors::thrown_definition_error,
        kernel_entry::{
            GeneralExecutor,
            Prepared,
            Request,
            RootFactory,
            RunFailure,
            finish_request,
            prepare_request, //
        },
        promise::{
            Agent,
            AgentOptions,
            Promise,
            PromiseState, //
        },
        value::{
            Completion,
            ErrorObject,
            Function,
            PlainObject,
            Value, //
        }, //
    };
    use aosabi_federation::{
        gateway::{
            Gateway,
            create_gateway,
            reset_schema_cache, //
        },
        headers::{
            Headers,
            HeadersFn, //
        }, //
    };
    use aosabi_fetch::NativeFetch;
    use aosabi_http::{
        Body,
        Env,
        Vars, //
    };
    use aosabi_json::{
        JsObject,
        JsValue, //
    };
    use aosabi_serve::runtime::Runtime;

    use super::{
        Fatal,
        Session,
        decode_js,
        decode_patches,
        encode_value,
        error_json,
        fatal,
        is_truthy,
        lossy,
        needs_of,
        require,
        reset_forced,
        string_of,
        tag_of, //
    };

    /// The name of the operation, the prefix of its `fatal`.
    const OP: &str = "delegate";

    /// The URL of the request a front context is made of, as below HTTP.
    const FRONT_URL: &str = "http://localhost/graphql";

    /// The text of an execution whose agent ran out of work before its result.
    const STALLED: &str = "the front execution stalled before its result settled";

    /// The registry of the remote a request line describes.
    ///
    /// A request names its keys and its patches as data, so the registry
    /// lists no key and no type, and nothing of the typed surface reads it.
    struct LineRemote;

    impl Registry for LineRemote {
        const DELEGATE: &'static [&'static str] = &[];
        const TYPES: &'static [&'static str] = &[];
    }

    /// A guard of the guard table of the sidecar.
    #[derive(Clone, Copy, Debug)]
    enum Guard {
        /// `data => data.status === 'ACTIVE' && data.isPublished`.
        StatusActivePublished,
        /// `() => true`.
        AlwaysTrue,
        /// `() => false`.
        AlwaysFalse,
    }

    impl Guard {
        /// Returns the guard the id `raw` names.
        fn of(raw: &JsValue) -> Result<Self, Fatal> {
            match string_of(Some(raw)).as_deref() {
                Some("status_active_published") => Ok(Self::StatusActivePublished),
                Some("always_true") => Ok(Self::AlwaysTrue),
                Some("always_false") => Ok(Self::AlwaysFalse),
                _ => Err(fatal(OP, format!("no guard has the id {}", id_text(raw)))),
            }
        }

        /// Reports whether the guard keeps the delegated value `data`.
        fn keeps(self, data: &Value) -> bool {
            match self {
                Self::StatusActivePublished => {
                    let status = member(data, "status");
                    let active = matches!(status, Value::String(status) if status == "ACTIVE");
                    active && truthy(&member(data, "isPublished"))
                }
                Self::AlwaysTrue => true,
                Self::AlwaysFalse => false,
            }
        }
    }

    /// Returns the member `key` of `data`, `undefined` for a value that is no object.
    fn member(data: &Value, key: &str) -> Value {
        match data {
            Value::Object(object) => Rc::clone(object).get(key),
            _ => Value::Undefined,
        }
    }

    /// Reports whether `value` is truthy, ECMA-262 section 7.1.2, `ToBoolean`.
    fn truthy(value: &Value) -> bool {
        match value {
            Value::Undefined | Value::Null => false,
            Value::Bool(boolean) => *boolean,
            Value::Number(number) => *number != 0.0 && !number.is_nan(),
            Value::String(string) => !string.is_empty(),
            _ => true,
        }
    }

    /// Returns the text of an id a request names, for a `fatal`.
    fn id_text(raw: &JsValue) -> String {
        aosabi_json::to_string(raw).unwrap_or_default()
    }

    /// What a root field of the front answers.
    #[derive(Debug)]
    enum FieldSpec {
        /// `{"delegate": {"key", "args", "needs", "guard"}}`, a call of the federation.
        Delegate(Call),
        /// `{"reject": {"message", "ticks"}}`, a promise that rejects after `ticks` turns.
        Reject {
            /// The message of the error it rejects with.
            message: String,
            /// The turns of the microtask queue before it rejects.
            ticks: u32,
        },
        /// `{"value": json}`, the value.
        Value(JsValue),
    }

    /// One call of the federation's `delegate`, its key and the options the root function writes.
    #[derive(Debug)]
    struct Call {
        /// The key, `<Operation>.<field>`.
        key: String,
        /// The option `args`, with `{"$arg": i}` the i-th argument of the root function.
        args: Option<JsValue>,
        /// The option `needs`, decoded.
        needs: JsValue,
        /// The option `guard`.
        guard: Option<Guard>,
    }

    impl Call {
        /// Returns the call a spec object writes.
        fn of(raw: &JsObject) -> Result<Self, Fatal> {
            let key = string_of(raw.get("key"))
                .ok_or_else(|| fatal(OP, "a delegate call carries a string key"))?;
            let needs = match raw.get("needs") {
                None => JsValue::Undefined,
                Some(needs) => decode_js(needs).map_err(|reason| fatal(OP, reason))?,
            };
            let guard = match raw.get("guard") {
                None | Some(JsValue::Undefined) => None,
                Some(id) => Some(Guard::of(id)?),
            };
            Ok(Self {
                key,
                args: raw.get("args").cloned(),
                needs,
                guard,
            })
        }

        /// Calls the federation with the arguments `positional` of the root function.
        ///
        /// The call reads the resolve info, the context, the arguments and the
        /// key before it answers, as the async function of the reference runs
        /// up to its first await, and its promise settles with the value the
        /// delegation answers, `null` when the guard rejects a value that is
        /// not nullish.
        fn start(
            &self,
            agent: &Agent,
            gateway: &Gateway<LineRemote>,
            positional: &[Value],
        ) -> Result<Promise, Fatal> {
            let args = match &self.args {
                Some(raw) => bind_arguments(raw, positional)?,
                None => JsValue::Undefined,
            };
            // `options?.args || {}`: a falsy value is the empty object, and
            // an object is what the remote root field takes.
            let args = match args {
                JsValue::Object(args) => args,
                _ => JsObject::new(),
            };
            let needs = needs_of(&self.needs).unwrap_or_default();
            let running = gateway.__delegate_key(&self.key, args, needs);
            let guard = self.guard;
            Ok(agent.spawn(Box::pin(async move {
                match running.await {
                    Ok(value) => match guard {
                        Some(guard) if !value.is_nullish() && !guard.keeps(&value) => {
                            Completion::Normal(Value::Null)
                        }
                        _ => Completion::Normal(value),
                    },
                    Err(Thrown(error)) => Completion::Throw(error),
                }
            })))
        }
    }

    /// Returns the value of a call's `args`, with `{"$arg": i}` the i-th argument of the resolver.
    ///
    /// A one key object of another `$` tag is a value of the codec, and any
    /// other object or list is bound member by member.
    fn bind_arguments(raw: &JsValue, positional: &[Value]) -> Result<JsValue, Fatal> {
        match raw {
            JsValue::Array(items) => Ok(JsValue::Array(
                items
                    .iter()
                    .map(|item| bind_arguments(item, positional))
                    .collect::<Result<_, _>>()?,
            )),
            JsValue::Object(object) => match tag_of(object).as_deref() {
                Some("$arg") => {
                    let index = match object.get("$arg") {
                        Some(JsValue::Number(index)) => super::whole(*index),
                        _ => None,
                    };
                    Ok(index
                        .and_then(|index| positional.get(index))
                        .map_or(JsValue::Undefined, Value::to_js))
                }
                Some(_) => decode_js(raw).map_err(|reason| fatal(OP, reason)),
                None => {
                    let mut out = JsObject::new();
                    for (key, member) in object.own_keys() {
                        out.insert(key.clone(), bind_arguments(member, positional)?);
                    }
                    Ok(JsValue::Object(out))
                }
            },
            other => Ok(other.clone()),
        }
    }

    /// Returns a promise that rejects with `message` after `ticks` turns of the microtask queue.
    ///
    /// The twin of `rejectAfter` of the sidecar: a resolved promise, `ticks`
    /// less one reactions that answer `undefined`, and a reaction that throws.
    fn reject_after(agent: &Agent, ticks: u32, message: &str) -> Promise {
        let mut promise = Promise::resolve(agent, Value::Undefined);
        for _ in 1..ticks {
            promise = promise.then(
                agent,
                Some(Box::new(|_agent, _value| {
                    Completion::Normal(Value::Undefined)
                })),
                None,
            );
        }
        let message = String::from(message);
        promise.then(
            agent,
            Some(Box::new(move |_agent, _value| {
                Completion::Throw(Value::from(ErrorObject::error(message)))
            })),
            None,
        )
    }

    /// The fields of one root type, each name with its spec, in the order of the request.
    type RootFields = Vec<(String, Rc<FieldSpec>)>;

    /// The root types of a front, each with its fields, in the order of the request.
    type Roots = Vec<(String, RootFields)>;

    /// The front of the request being answered, which the root factories read.
    struct Front {
        /// The configured federation every delegate spec calls.
        gateway: Gateway<LineRemote>,
        /// The fields of each root type, in the order of the request.
        roots: Roots,
    }

    thread_local! {
        /// The front of the request being answered.
        static FRONT: RefCell<Option<Rc<Front>>> = const { RefCell::new(None) };
    }

    /// Returns the root object `root_type` of the front, one function per field.
    ///
    /// The factory of a root is called when the schema is built and once by
    /// the first call of a field of the root in the thread, whose members
    /// the thread keeps for every later call, and every call reads the front
    /// of the request being answered. Each request makes factories of its
    /// own, so a request never reads the members of another.
    fn root_object(root_type: &str) -> Value {
        let Some(front) = FRONT.with(|front| front.borrow().clone()) else {
            return Value::Undefined;
        };
        let Some((_, fields)) = front.roots.iter().find(|(name, _)| name == root_type) else {
            return Value::Undefined;
        };
        let entries = fields
            .iter()
            .map(|(name, spec)| (name.clone(), field_function(&front.gateway, spec)))
            .collect();
        Value::from(PlainObject::from_entries(entries))
    }

    /// Returns the function of one root field, as an app author writes it.
    fn field_function(gateway: &Gateway<LineRemote>, spec: &Rc<FieldSpec>) -> Value {
        let gateway = gateway.clone();
        let spec = Rc::clone(spec);
        Value::from(Function::new("", move |agent, positional| match &*spec {
            FieldSpec::Delegate(call) => match call.start(agent, &gateway, positional) {
                Ok(promise) => Completion::Normal(Value::from(promise)),
                Err(reason) => Completion::Throw(Value::from(ErrorObject::error(reason))),
            },
            FieldSpec::Reject { message, ticks } => {
                Completion::Normal(Value::from(reject_after(agent, *ticks, message)))
            }
            FieldSpec::Value(value) => Completion::Normal(Value::from_js(value)),
        }))
    }

    /// Returns the factory of the root object `root_type`.
    fn root_factory(root_type: &'static str) -> RootFactory {
        Arc::new(move || root_object(root_type))
    }

    /// Returns the roots of a request, `{"Query": {field: spec}}`.
    fn roots_of(raw: Option<&JsValue>) -> Result<Roots, Fatal> {
        let Some(JsValue::Object(raw)) = raw else {
            return Err(fatal(OP, "roots is an object"));
        };
        let mut roots = Vec::new();
        for (root_type, fields) in raw.own_keys() {
            let root_type = lossy(root_type);
            if !["Query", "Mutation", "Subscription"].contains(&root_type.as_str()) {
                return Err(fatal(OP, format!("{root_type} is no root")));
            }
            let JsValue::Object(fields) = fields else {
                return Err(fatal(
                    OP,
                    format!("the fields of {root_type} are an object"),
                ));
            };
            let mut specs = Vec::new();
            for (field_name, spec) in fields.own_keys() {
                let field_name = lossy(field_name);
                specs.push((
                    field_name.clone(),
                    Rc::new(field_spec(&root_type, &field_name, spec)?),
                ));
            }
            roots.push((root_type, specs));
        }
        Ok(roots)
    }

    /// Returns the spec of one root field.
    fn field_spec(root_type: &str, field_name: &str, raw: &JsValue) -> Result<FieldSpec, Fatal> {
        let refused = || {
            fatal(
                OP,
                format!("the spec of {root_type}.{field_name} is no delegate, reject or value"),
            )
        };
        let JsValue::Object(raw) = raw else {
            return Err(refused());
        };
        if let Some(JsValue::Object(call)) = raw.get("delegate") {
            return Ok(FieldSpec::Delegate(Call::of(call)?));
        }
        if let Some(JsValue::Object(reject)) = raw.get("reject") {
            let message = string_of(reject.get("message")).unwrap_or_default();
            let ticks = match reject.get("ticks") {
                None | Some(JsValue::Undefined | JsValue::Null) => 1,
                Some(JsValue::Number(ticks)) => super::whole(*ticks)
                    .and_then(|ticks| u32::try_from(ticks).ok())
                    .ok_or_else(|| fatal(OP, "ticks is a count"))?,
                Some(_) => return Err(fatal(OP, "ticks is a count")),
            };
            return Ok(FieldSpec::Reject { message, ticks });
        }
        if let Some(value) = raw.get("value") {
            return Ok(FieldSpec::Value(
                decode_js(value).map_err(|reason| fatal(OP, reason))?,
            ));
        }
        Err(refused())
    }

    /// The headers function `forward_authorization` of the sidecar.
    ///
    /// `ctx => ({authorization: ctx?.req?.header('authorization') ?? ''})`,
    /// which answers the empty text for the introspection, whose request
    /// carries no context.
    fn forward_authorization(ctx: Option<&Context>) -> Headers {
        let mut headers = Headers::new();
        let value = ctx
            .and_then(|ctx| ctx.req().header("authorization"))
            .unwrap_or_default();
        headers.set("authorization", value);
        headers
    }

    /// The headers function `none` of the sidecar, `() => ({})`.
    fn no_headers(_ctx: Option<&Context>) -> Headers {
        Headers::new()
    }

    /// Returns the URL of a federation, refused when it leaves the loopback.
    fn loopback_url(raw: Option<&JsValue>) -> Result<String, Fatal> {
        let url = string_of(raw).ok_or_else(|| {
            fatal(
                OP,
                format!(
                    "the federation url {} is no URL",
                    id_text(raw.unwrap_or(&JsValue::Undefined))
                ),
            )
        })?;
        let host = url
            .strip_prefix("http://")
            .map(|rest| rest.split(['/', '?', '#']).next().unwrap_or_default());
        let hostname = host.map(|host| match host.strip_prefix('[') {
            Some(inner) => inner
                .split(']')
                .next()
                .map(|name| format!("[{name}]"))
                .unwrap_or_default(),
            None => String::from(host.split(':').next().unwrap_or_default()),
        });
        match hostname.as_deref() {
            Some("127.0.0.1" | "localhost" | "[::1]") => Ok(url),
            _ => Err(fatal(
                OP,
                format!("the federation url {url} is not on the loopback"),
            )),
        }
    }

    /// Returns the configured federation of a request's `federation` object.
    fn federation_of(raw: Option<&JsValue>) -> Result<Gateway<LineRemote>, Fatal> {
        let Some(JsValue::Object(raw)) = raw else {
            return Err(fatal(OP, "federation is an object"));
        };
        let url = loopback_url(raw.get("url"))?;
        let headers: Option<HeadersFn> = match raw.get("headers") {
            None | Some(JsValue::Undefined | JsValue::Null) => None,
            Some(id) => match string_of(Some(id)).as_deref() {
                Some("forward_authorization") => Some(forward_authorization),
                Some("none") => Some(no_headers),
                _ => {
                    return Err(fatal(
                        OP,
                        format!("no headers function has the id {}", id_text(id)),
                    ));
                }
            },
        };
        let patches = decode_patches(raw.get("patches")).map_err(|reason| fatal(OP, reason))?;
        let strict = raw.get("strict").is_some_and(is_truthy);
        Ok(create_gateway::<LineRemote>().__configure_table(url, headers, patches, strict))
    }

    /// Returns the context of the front, whose request carries the head of the client request.
    ///
    /// The sidecar answers `req.header(name)` from the lower case names of
    /// `request_headers`, which the request of this context answers through
    /// the case insensitive lookup of its headers.
    fn front_context(raw: Option<&JsValue>) -> Result<Context, Fatal> {
        let mut headers = aosabi_http::Headers::new();
        if let Some(JsValue::Object(raw)) = raw {
            for (name, value) in raw.own_keys() {
                let value = match value {
                    JsValue::String(value) => lossy(value),
                    other => id_text(other),
                };
                headers
                    .set(&lossy(name), &value)
                    .map_err(|error| fatal(OP, format!("request_headers: {error}")))?;
            }
        }
        let request = Rc::new(aosabi_http::Request::new(
            "POST",
            FRONT_URL,
            headers,
            Body::Empty,
        ));
        Ok(Context::new(
            request,
            String::from("/graphql"),
            Rc::from(Vec::new()),
            Env::Bindings(Rc::new(Vars::default())),
            None,
        ))
    }

    /// Sets the ambient hooks of the context on the agent of a request, as a service does.
    fn setup_agent(agent: &Agent) {
        let (capture, restore) = ambient_hooks();
        agent.set_ambient_hooks(capture, restore);
    }

    /// Returns the runtime a request runs on, with the client of aosabi-fetch installed.
    ///
    /// The network capability keeps its first driver, so the client is
    /// installed once per process, and every request runs on a runtime of its
    /// own. The client keeps no handle of a runtime in a task of its own and
    /// closes the idle connection it kept from one request at the first call
    /// of a later one, so what it holds of an ended runtime lasts until that
    /// call.
    fn runtime() -> Result<Runtime, Fatal> {
        let fetch =
            NativeFetch::from_env().map_err(|error| fatal(OP, format!("fetch: {error}")))?;
        install_fetch(Rc::new(fetch));
        Runtime::new().map_err(|error| fatal(OP, format!("runtime: {error}")))
    }

    /// Answers `delegate`.
    ///
    /// The schema cache of the federation is reset first unless
    /// `keep_schema_cache` is true, so every case starts with an
    /// introspection, as the sidecar resets it.
    pub(super) fn answer(request: &JsObject) -> Result<JsObject, Fatal> {
        reset_forced();
        if !matches!(request.get("keep_schema_cache"), Some(JsValue::Bool(true))) {
            reset_schema_cache();
        }
        let gateway = federation_of(request.get("federation"))?;
        let context = front_context(request.get("request_headers"))?;
        let runtime = runtime()?;
        if matches!(request.get("outside_resolver"), Some(JsValue::Bool(true))) {
            return outside(request, &runtime, &gateway, &context);
        }
        let front_sdl = require(request, OP, "front_sdl")?;
        let roots = roots_of(request.get("roots"))?;
        let has = |name: &str| roots.iter().any(|(root_type, _)| root_type == name);
        let resolvers = Resolvers {
            query: has("Query").then(|| root_factory("Query")),
            mutation: has("Mutation").then(|| root_factory("Mutation")),
            subscription: has("Subscription").then(|| root_factory("Subscription")),
        };
        FRONT.with(|front| front.replace(Some(Rc::new(Front { gateway, roots }))));
        let answered = execute_front(request, &runtime, &context, &front_sdl, &resolvers);
        FRONT.with(|front| front.replace(None));
        answered
    }

    /// Answers a delegation outside a resolver, refusal R09 of `90_federation`.
    ///
    /// The federation's `delegate` is called once inside the context of the
    /// request with no resolve info in place, and its promise is driven on
    /// an agent of its own.
    fn outside(
        request: &JsObject,
        runtime: &Runtime,
        gateway: &Gateway<LineRemote>,
        context: &Context,
    ) -> Result<JsObject, Fatal> {
        let Some(JsValue::Object(call)) = request.get("call") else {
            return Err(fatal(OP, "outside_resolver needs a call"));
        };
        let call = Call::of(call)?;
        let agent = Agent::new(AgentOptions::default());
        setup_agent(&agent);
        // The call runs inside the runtime, whose handle the client of the
        // network capability reads when the delegation sends its first
        // request.
        let driven = runtime.block_on(async {
            let promise = run_with_context(context.clone(), || {
                agent.enter(|| call.start(&agent, gateway, &[]))
            })?;
            Ok::<_, Fatal>(with_context(context.clone(), agent.drive(&promise)).await)
        })?;
        let session = Session::default();
        let mut payload = JsObject::new();
        match driven {
            Ok(PromiseState::Fulfilled(value)) => {
                payload.insert("value", encode_value(&value, &session));
            }
            Ok(PromiseState::Rejected(error)) => {
                payload.insert("thrown", error_json(&error, &session));
            }
            Ok(PromiseState::Pending) | Err(_) => return Err(fatal(OP, STALLED)),
        }
        Ok(payload)
    }

    /// Returns the error value of a front schema that does not build.
    fn schema_error(error: CreateSchemaError) -> Value {
        match error {
            CreateSchemaError::Schema(error) => thrown_definition_error(error),
            CreateSchemaError::Resolvers(message) => Value::from(ErrorObject::error(message)),
        }
    }

    /// Builds the front schema and runs the client request through it.
    ///
    /// The schema is built as a served app builds it, the roots through
    /// `resolvers_to_graphql_resolvers` and the bound scalars of the SDL, and
    /// a schema the build refuses answers `errors`. The request runs through
    /// `prepare_request`, whose syntax and validation errors answer the
    /// result with no execution, and an execution that has to wait is driven
    /// on the runtime inside the context of the request.
    fn execute_front(
        request: &JsObject,
        runtime: &Runtime,
        context: &Context,
        front_sdl: &str,
        resolvers: &Resolvers,
    ) -> Result<JsObject, Fatal> {
        let session = Session::default();
        let core = CoreState::new(leaf_serializers(front_sdl));
        let built = resolvers_to_graphql_resolvers(&core, resolvers).and_then(|graphql| {
            create_schema(front_sdl, graphql, ResolverMap::default()).map_err(schema_error)
        });
        let schema = match built {
            Ok(schema) => schema,
            Err(error) => {
                let mut payload = JsObject::new();
                payload.insert("errors", JsValue::Array(vec![error_json(&error, &session)]));
                return Ok(payload);
            }
        };
        let executor = GeneralExecutor {
            schema,
            root_value: Arc::new(|| Value::Undefined),
            leaf_serializers: Some(Arc::clone(core.leaf_serializers())),
        };
        let document = require(request, OP, "document")?;
        let variables = match request.get("variables") {
            None | Some(JsValue::Undefined) => None,
            Some(raw) => Some(decode_js(raw).map_err(|reason| fatal(OP, reason))?),
        };
        let operation_name = string_of(request.get("operation_name"));
        let client = Request {
            document: &document,
            variables,
            operation_name: operation_name.as_deref(),
            validate: true,
        };
        let context_value: Rc<dyn Any> = Rc::new(context.clone());
        // The execution starts inside the runtime, whose handle the client of
        // the network capability reads when a root resolver sends the first
        // request of its delegation.
        let executed = runtime.block_on(async {
            let prepared = run_with_context(context.clone(), || {
                prepare_request(
                    &executor,
                    &client,
                    context_value,
                    AgentOptions::default(),
                    &setup_agent,
                )
            });
            match prepared {
                Prepared::Answered(executed) => executed,
                Prepared::Settled { agent, state } => finish_request(&agent, Ok(state)),
                Prepared::Driving { agent, result } => {
                    let driven = with_context(context.clone(), agent.drive(&result)).await;
                    finish_request(&agent, driven)
                }
            }
        });
        let mut payload = JsObject::new();
        match executed.outcome {
            Ok(result) => {
                let text = aosabi_json::to_string(&result.to_js_value()).unwrap_or_default();
                payload.insert("result", text);
            }
            Err(RunFailure::Thrown(error)) => {
                payload.insert("thrown", error_json(&error, &session));
            }
            Err(RunFailure::Stalled) => return Err(fatal(OP, STALLED)),
        }
        Ok(payload)
    }
}
