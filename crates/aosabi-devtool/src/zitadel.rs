// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool zitadel: the primitive through which a notebook maps a
 * principal, reads a token, takes the roles of a claim and writes the
 * introspection body of the Zitadel provider in process.
 *
 * No analogue in the reference. The protocol is tests/sidecar/zitadel.md, and
 * the judging notebook is tests/notebooks/63_zitadel_auth.ipynb, step 2.
 */

//! The primitive `zitadel` of the seam.
//!
//! The flag `--requests PATH` answers every request line of the file in the
//! batch form of [`crate::batch`], `id` first and `op` second, with no server
//! and no network, so one file of request lines drives this primitive and the
//! sidecar alike.
//!
//! | `op` | Request fields | Payload |
//! | --- | --- | --- |
//! | `versions` | none | `reference`, `hono`, `openid_client` and `oauth4webapi`, the versions the crate ports |
//! | `principal` | `sub`, `roles`, `user`, `mappers`? | `principal`, what `zitadel_principal` answers |
//! | `read_token` | `authorization`?, `query`?, `cookie`? as a string or a list of strings | `token`, what `read_token` answers |
//! | `roles` | `claim` | `roles`, what `roles_of` answers, or `thrown`, its error by `name` and `message` |
//! | `introspection_body` | `issuer`, `token`, `parameters`, `auth_key` | `body` and `assertion`, the form body `token_introspection` posts with its client assertion masked |
//!
//! A value on the line takes the markers of the protocol: the object
//! `{"$undefined":true}` stands for `undefined` at any depth, and a number
//! JSON has no notation for is written `{"$number":"NaN"}` or with the name
//! of its infinity. An answer writes the keys of an object in the own key
//! order of JavaScript, the canonical integer keys ascending first, as the
//! sidecar reads them from the object `JSON.parse` makes.
//!
//! The mappers of `principal` are the ids of the protocol, each a function
//! pointer of [`ZitadelAuthOptions`] that answers what the mapper of the
//! sidecar answers, converted to the type of its option. The options are
//! typed as the TypeScript options are, so a mapper answer outside that type
//! takes the value the reference makes of it: an id mapper that answers
//! `null` or `undefined` answers the subject, the `?? sub` of the reference,
//! which the function pointer reads from a `static` of this thread, as rule
//! 8 of section AUTH of the plan asks of a mapper that needs state, a
//! permissions mapper the empty list of `?? []`, and an attributes mapper the
//! user info of `?? user`. A tenant mapper that answers `null` answers no
//! tenant, the one value of `Option<PrincipalId>` for both `null` and
//! `undefined`, so its answer writes `undefined` where the reference keeps
//! `null`, which step 2 of the notebook compares as one value. A mapper
//! answer no typed option holds, such as an object for an id, answers
//! `fatal`.
//!
//! `introspection_body` installs a recording driver of the network
//! capability on this thread, which takes the one request and answers an
//! inactive token, so nothing leaves the process. It verifies the client
//! assertion with the public half of the key of the request before it masks
//! it, as the sidecar verifies the assertion of the reference.

use core::{
    cell::{
        Cell,
        RefCell, //
    },
    future::Future,
    pin::pin,
    task::{
        Context as TaskContext,
        Poll,
        Waker, //
    },
};
use std::{
    process::ExitCode,
    rc::Rc, //
};

use aosabi_auth::{
    Principal,
    PrincipalId, //
};
use aosabi_auth_zitadel::{
    OidcUser, ZitadelAuthOptions,
    oidc::{
        discovery::configuration,
        jwt::{
            base64url_decode,
            parse_decoded, //
        },
        token_introspection,
    },
    verify_rs256,
    zitadel::client_of,
    zitadel_auth::{
        read_token,
        roles_of, //
    },
    zitadel_principal,
};
#[cfg(unix)]
use aosabi_core::random::{
    DevUrandom,
    install_randomness, //
};
use aosabi_core::{
    Thrown,
    net::{
        Fetch,
        FetchError,
        install_fetch, //
    },
};
use aosabi_http::{
    Body,
    Context,
    Env,
    Headers,
    HeadersInit,
    LocalBoxFuture,
    Request,
    Response,
    ResponseInit,
    Vars, //
};
use aosabi_json::{
    JsObject,
    JsString,
    JsValue, //
};
use aosabi_lang::js_string::to_string_lossy;
use aosabi_type::scalars::string_to_number;

use crate::{
    batch::Operations,
    execute::error_record,
    seam::{
        self,
        BatchArgs, //
    }, //
};

/// The name of the primitive.
const NAME: &str = "zitadel";

/// The milestone the primitive lands at.
const LANDS_AT: &str = "M7b";

/// The versions of the packages the crate ports, by the keys of the sidecar.
const VERSIONS: [(&str, &str); 4] = [
    ("reference", "2.9.5"),
    ("hono", "4.11.9"),
    ("openid_client", "6.8.2"),
    ("oauth4webapi", "3.8.4"),
];

/// The marker of `undefined` on the line.
const UNDEFINED: &str = "$undefined";

/// The marker of a number JSON has no notation for.
const NUMBER: &str = "$number";

/// The claim of the organisation a tenant mapper of the variant `mapped` reads.
const RESOURCE_OWNER: &str = "urn:zitadel:iam:user:resourceowner:id";

/// The URL every `read_token` request carries, under a host of the reserved domain.
const PROBE_URL: &str = "http://app.example.invalid/probe";

/// The name of the client assertion in a form body, and the text that stands for its value.
const ASSERTION: &str = "client_assertion";
const ASSERTION_MASK: &str = "<client-assertion>";

/// The claims of a client assertion a run draws, each masked by its name.
const DRAWN_CLAIMS: [&str; 4] = ["jti", "iat", "nbf", "exp"];

/// The answer of the recording driver, an inactive token.
const INACTIVE: &str = "{\"active\":false}";

/// The most polls an introspection may take, since the recording driver answers at once.
const POLLS: usize = 1024;

/// The operations of the primitive.
const OPERATIONS: &Operations = &[
    ("versions", op_versions),
    ("principal", op_principal),
    ("read_token", op_read_token),
    ("roles", op_roles),
    ("introspection_body", op_introspection_body),
];

/// Runs the `zitadel` primitive, whose one form is the batch form.
pub fn run(args: &BatchArgs) -> ExitCode {
    seam::run_batch(NAME, LANDS_AT, OPERATIONS, args)
}

/// Answers `versions`, the versions of the packages the crate ports.
// The signature is the one every operation of the batch form has.
#[expect(clippy::unnecessary_wraps)]
fn op_versions(_request: &JsObject) -> Result<JsObject, JsString> {
    let mut payload = JsObject::new();
    for (key, version) in VERSIONS {
        payload.insert(key, version);
    }
    Ok(payload)
}

/*
 * The values on the line.
 */

/// Returns `value` with the marker of `undefined` read back, at every depth.
fn decode(value: &JsValue) -> JsValue {
    match value {
        JsValue::Object(object) => {
            if object.len() == 1 && object.get(UNDEFINED) == Some(&JsValue::Bool(true)) {
                return JsValue::Undefined;
            }
            let mut out = JsObject::with_capacity(object.len());
            for (key, member) in object.iter() {
                out.insert(key.clone(), decode(member));
            }
            JsValue::Object(out)
        }
        JsValue::Array(items) => JsValue::Array(items.iter().map(decode).collect()),
        other => other.clone(),
    }
}

/// Returns the marker of `undefined`.
fn undefined() -> JsValue {
    let mut marker = JsObject::new();
    marker.insert(UNDEFINED, true);
    JsValue::Object(marker)
}

/// Returns `number` as the line writes it, a marker for `NaN` and the infinities.
fn number(number: f64) -> JsValue {
    if number.is_finite() {
        return JsValue::Number(number);
    }
    let name = if number.is_nan() {
        "NaN"
    } else if number > 0.0 {
        "Infinity"
    } else {
        "-Infinity"
    };
    let mut marker = JsObject::new();
    marker.insert(NUMBER, name);
    JsValue::Object(marker)
}

/// Returns `value` as the line writes it, at every depth and in the own key order of JavaScript.
fn encode(value: &JsValue) -> JsValue {
    match value {
        JsValue::Undefined => undefined(),
        JsValue::Number(value) => number(*value),
        JsValue::Array(items) => JsValue::Array(items.iter().map(encode).collect()),
        JsValue::Object(object) => {
            let mut out = JsObject::with_capacity(object.len());
            for (key, member) in object.own_keys() {
                out.insert(key.clone(), encode(member));
            }
            JsValue::Object(out)
        }
        other => other.clone(),
    }
}

/// Returns the string field `name` of `request`, or the `fatal` naming it.
fn string<'a>(request: &'a JsObject, op: &str, name: &str) -> Result<&'a JsString, JsString> {
    match request.get(name) {
        Some(JsValue::String(text)) => Ok(text),
        _ => Err(JsString::from(format!(
            "{op}: the request carries no string field {name}"
        ))),
    }
}

/// Returns the object field `name` of `request`, or the `fatal` naming it.
fn record<'a>(request: &'a JsObject, op: &str, name: &str) -> Result<&'a JsObject, JsString> {
    match request.get(name) {
        Some(JsValue::Object(object)) => Ok(object),
        _ => Err(JsString::from(format!(
            "{op}: the request carries no object field {name}"
        ))),
    }
}

/// Returns the optional string or null field `name` of `request`, none when it is absent or null.
fn optional_string(request: &JsObject, op: &str, name: &str) -> Result<Option<String>, JsString> {
    match request.get(name) {
        None | Some(JsValue::Undefined | JsValue::Null) => Ok(None),
        Some(JsValue::String(text)) => Ok(Some(to_string_lossy(text).into_owned())),
        Some(_) => Err(JsString::from(format!(
            "{op}: the field {name} is a string or null"
        ))),
    }
}

/// Returns the `fatal` of a thrown value, its message.
fn thrown_text(thrown: &Thrown) -> JsString {
    match error_record(&thrown.0).get("message") {
        Some(JsValue::String(message)) => message.clone(),
        _ => JsString::from("the value thrown carries no message"),
    }
}

/*
 * The operation principal.
 */

/// What a mapper of this thread reads beside the user info, the `static` of rule 8.
#[derive(Default)]
struct Mapping {
    /// The introspected subject of the request, which `?? sub` falls back to.
    sub: RefCell<String>,
    /// Whether a mapper answered a value its typed option does not hold.
    unrepresentable: Cell<bool>,
}

thread_local! {
    /// The mapping of the request in hand.
    static MAPPING: Mapping = Mapping::default();
}

/// Records that a mapper answered a value its option does not hold.
fn unrepresentable() {
    let _ = MAPPING.try_with(|mapping| mapping.unrepresentable.set(true));
}

/// Returns the subject of the request in hand, the fallback of `?? sub`.
fn subject() -> PrincipalId {
    let sub = MAPPING
        .try_with(|mapping| mapping.sub.borrow().clone())
        .unwrap_or_default();
    PrincipalId::String(sub)
}

/// Returns an id for a mapper answer, `options.id?.(user) ?? sub`.
fn id_of(answer: Option<&JsValue>) -> PrincipalId {
    match answer {
        None | Some(JsValue::Undefined | JsValue::Null) => subject(),
        Some(JsValue::String(text)) => PrincipalId::String(to_string_lossy(text).into_owned()),
        Some(JsValue::Number(value)) => PrincipalId::Number(*value),
        Some(_) => {
            unrepresentable();
            subject()
        }
    }
}

/// Returns a tenant for a mapper answer, `options.tenant?.(user)`.
fn tenant_of(answer: Option<&JsValue>) -> Option<PrincipalId> {
    match answer {
        None | Some(JsValue::Undefined | JsValue::Null) => None,
        Some(JsValue::String(text)) => {
            Some(PrincipalId::String(to_string_lossy(text).into_owned()))
        }
        Some(JsValue::Number(value)) => Some(PrincipalId::Number(*value)),
        Some(_) => {
            unrepresentable();
            None
        }
    }
}

/// Returns the permissions for a mapper answer, `options.permissions?.(user) ?? []`.
fn permissions_of(answer: Option<&JsValue>) -> Vec<String> {
    match answer {
        None | Some(JsValue::Undefined | JsValue::Null) => Vec::new(),
        Some(JsValue::Array(items)) => items
            .iter()
            .map(|item| {
                if let JsValue::String(text) = item {
                    to_string_lossy(text).into_owned()
                } else {
                    unrepresentable();
                    String::new()
                }
            })
            .collect(),
        Some(_) => {
            unrepresentable();
            Vec::new()
        }
    }
}

/// Returns `Number(value)`, ECMA-262 `ToNumber` for the values a claim holds.
fn to_number(value: Option<&JsValue>) -> f64 {
    match value {
        None | Some(JsValue::Undefined) => f64::NAN,
        Some(JsValue::Null) => 0.0,
        Some(JsValue::Bool(value)) => f64::from(u8::from(*value)),
        Some(JsValue::Number(value)) => *value,
        Some(JsValue::String(text)) => text.as_str().map_or(f64::NAN, string_to_number),
        Some(_) => {
            unrepresentable();
            f64::NAN
        }
    }
}

/// Answers the email of `user`, the id mapper `email`, `user => user.email`.
fn id_email(user: &OidcUser) -> PrincipalId {
    id_of(user.get("email"))
}

/// Answers the number of the `sub` of `user`, the id mapper `sub_number`,
/// `user => Number(user.sub)`.
fn id_sub_number(user: &OidcUser) -> PrincipalId {
    PrincipalId::Number(to_number(user.get("sub")))
}

/// Answers `null`, which reaches `?? sub`, the id mapper `null`, `() => null`.
fn id_null(_user: &OidcUser) -> PrincipalId {
    id_of(Some(&JsValue::Null))
}

/// Answers the organisation of `user`, the tenant mapper `resourceowner`,
/// `user => user['urn:zitadel:iam:user:resourceowner:id']`.
fn tenant_resource_owner(user: &OidcUser) -> Option<PrincipalId> {
    tenant_of(user.get(RESOURCE_OWNER))
}

/// Answers the `org_id` of `user`, the tenant mapper `org_id`, `user => user.org_id`.
fn tenant_org_id(user: &OidcUser) -> Option<PrincipalId> {
    tenant_of(user.get("org_id"))
}

/// Answers no tenant, the tenant mapper `null`, `() => null`.
fn tenant_null(_user: &OidcUser) -> Option<PrincipalId> {
    tenant_of(Some(&JsValue::Null))
}

/// Answers the `perm` of `user`, the permissions mapper `perm`, `user => user.perm ?? []`.
fn permissions_perm(user: &OidcUser) -> Vec<String> {
    permissions_of(user.get("perm"))
}

/// Answers the `perms` of `user`, the permissions mapper `perms`, `user => user.perms ?? []`.
fn permissions_perms(user: &OidcUser) -> Vec<String> {
    permissions_of(user.get("perms"))
}

/// Answers `null`, which reaches `?? []`, the permissions mapper `null`, `() => null`.
fn permissions_null(_user: &OidcUser) -> Vec<String> {
    permissions_of(Some(&JsValue::Null))
}

/// Answers the email of `user` alone, the attributes mapper `email`,
/// `user => ({email: user.email})`.
fn attributes_email(user: &OidcUser) -> JsObject {
    let mut attributes = JsObject::new();
    attributes.insert("email", user.get("email").cloned().unwrap_or_default());
    attributes
}

/// Answers `null`, which reaches `?? user`, the attributes mapper `null`, `() => null`.
fn attributes_null(user: &OidcUser) -> JsObject {
    user.clone()
}

/// Returns the options a request names, the mappers its `mappers` record selects.
fn options_of(request: &JsObject) -> Result<ZitadelAuthOptions, JsString> {
    let mut options = ZitadelAuthOptions {
        issuer: String::from("https://issuer.example.invalid"),
        ..ZitadelAuthOptions::default()
    };
    let mappers = match request.get("mappers") {
        None | Some(JsValue::Undefined) => return Ok(options),
        Some(JsValue::Object(mappers)) => mappers,
        Some(_) => {
            return Err(JsString::from(
                "principal: the request carries no object field mappers",
            ));
        }
    };
    for (option, id) in mappers.iter() {
        let option = to_string_lossy(option).into_owned();
        let id = match id {
            JsValue::String(id) => to_string_lossy(id).into_owned(),
            _ => String::new(),
        };
        let unknown = || {
            JsString::from(format!(
                "principal: no mapper \"{id}\" for {option}, the ids of tests/sidecar/zitadel.md"
            ))
        };
        match option.as_str() {
            "id" => {
                options.id = Some(match id.as_str() {
                    "email" => id_email,
                    "sub_number" => id_sub_number,
                    "null" => id_null,
                    _ => return Err(unknown()),
                });
            }
            "tenant" => {
                options.tenant = Some(match id.as_str() {
                    "resourceowner" => tenant_resource_owner,
                    "org_id" => tenant_org_id,
                    "null" => tenant_null,
                    _ => return Err(unknown()),
                });
            }
            "permissions" => {
                options.permissions = Some(match id.as_str() {
                    "perm" => permissions_perm,
                    "perms" => permissions_perms,
                    "null" => permissions_null,
                    _ => return Err(unknown()),
                });
            }
            "attributes" => {
                options.attributes = Some(match id.as_str() {
                    "email" => attributes_email,
                    "null" => attributes_null,
                    _ => return Err(unknown()),
                });
            }
            _ => {
                return Err(JsString::from(format!(
                    "principal: no option \"{option}\" takes a mapper"
                )));
            }
        }
    }
    Ok(options)
}

/// Returns an id as the line writes it.
fn id_json(id: &PrincipalId) -> JsValue {
    match id {
        PrincipalId::String(text) => JsValue::from(text.as_str()),
        PrincipalId::Number(value) => number(*value),
    }
}

/// Returns a list of strings as the line writes it.
fn strings_json(list: &[String]) -> JsValue {
    JsValue::Array(
        list.iter()
            .map(|item| JsValue::from(item.as_str()))
            .collect(),
    )
}

/// Returns the principal as the line writes it, its five keys in the order of the reference.
fn principal_json(principal: &Principal) -> JsObject {
    let mut out = JsObject::with_capacity(5);
    out.insert("id", id_json(&principal.id));
    out.insert(
        "tenant",
        principal.tenant.as_ref().map_or_else(undefined, id_json),
    );
    out.insert(
        "roles",
        principal
            .roles
            .as_deref()
            .map_or_else(undefined, strings_json),
    );
    out.insert(
        "permissions",
        principal
            .permissions
            .as_deref()
            .map_or_else(undefined, strings_json),
    );
    out.insert(
        "attributes",
        principal
            .attributes
            .as_ref()
            .map_or_else(undefined, |attributes| {
                encode(&JsValue::Object(attributes.clone()))
            }),
    );
    out
}

/// Answers `principal`, `zitadel_principal(sub, roles, user, options)`.
fn op_principal(request: &JsObject) -> Result<JsObject, JsString> {
    let sub = to_string_lossy(string(request, "principal", "sub")?).into_owned();
    let roles = match request.get("roles") {
        Some(JsValue::Array(items)) => items
            .iter()
            .map(|item| match item {
                JsValue::String(text) => Ok(to_string_lossy(text).into_owned()),
                _ => Err(()),
            })
            .collect::<Result<Vec<String>, ()>>()
            .ok(),
        _ => None,
    }
    .ok_or_else(|| JsString::from("principal: roles is a list of strings"))?;
    let JsValue::Object(user) = decode(&JsValue::Object(
        record(request, "principal", "user")?.clone(),
    )) else {
        return Err(JsString::from(
            "principal: the request carries no object field user",
        ));
    };
    let options = options_of(request)?;
    let _ = MAPPING.try_with(|mapping| {
        mapping.sub.replace(sub.clone());
        mapping.unrepresentable.set(false);
    });
    let principal = zitadel_principal(&sub, roles, &user, &options);
    let held = MAPPING
        .try_with(|mapping| !mapping.unrepresentable.get())
        .unwrap_or(false);
    if !held {
        return Err(JsString::from(
            "principal: a mapper answers a value its typed option does not hold",
        ));
    }
    let mut payload = JsObject::new();
    payload.insert("principal", JsValue::Object(principal_json(&principal)));
    Ok(payload)
}

/*
 * The operation read_token.
 */

/// The field `cookie` of a request shape.
enum CookieShape {
    /// No field, or null.
    Absent,
    /// A string, the value of one `Cookie` field.
    One(String),
    /// A list of strings, the value of each `Cookie` field of a request that
    /// carries its cookies in several.
    Fields(Vec<String>),
}

/// Returns the field `cookie` of `request`, or the `fatal` naming it.
fn cookie_shape(request: &JsObject) -> Result<CookieShape, JsString> {
    let refused =
        || JsString::from("read_token: the field cookie is a string, a list of strings or null");
    match request.get("cookie") {
        None | Some(JsValue::Undefined | JsValue::Null) => Ok(CookieShape::Absent),
        Some(JsValue::String(text)) => Ok(CookieShape::One(to_string_lossy(text).into_owned())),
        Some(JsValue::Array(items)) => items
            .iter()
            .map(|item| match item {
                JsValue::String(text) => Ok(to_string_lossy(text).into_owned()),
                _ => Err(refused()),
            })
            .collect::<Result<Vec<String>, JsString>>()
            .map(CookieShape::Fields),
        Some(_) => Err(refused()),
    }
}

/// Returns the `fatal` of a value the `Headers` of a request shape refuses.
fn shape_refused(error: impl core::fmt::Display) -> JsString {
    JsString::from(format!("read_token: the headers refuse the shape: {error}"))
}

/// Answers `read_token`, `read_token(c)` over the context of a request shape.
///
/// The request is `GET http://app.example.invalid/probe` with the query and
/// the headers `Authorization` and `Cookie` of the shape, each set through
/// [`Headers`], which trims a value as the `Headers` of the sidecar does. A
/// `cookie` that is a list of strings is appended as one `Cookie` field per
/// value in its order, as the sidecar appends it, so the `Headers` joins the
/// fields as it joins those of a served request.
fn op_read_token(request: &JsObject) -> Result<JsObject, JsString> {
    let authorization = optional_string(request, "read_token", "authorization")?;
    let cookie = cookie_shape(request)?;
    let query = optional_string(request, "read_token", "query")?;
    let mut headers = Headers::new();
    if let Some(value) = authorization {
        headers
            .set("Authorization", &value)
            .map_err(shape_refused)?;
    }
    match cookie {
        CookieShape::Absent => {}
        CookieShape::One(value) => headers.set("Cookie", &value).map_err(shape_refused)?,
        CookieShape::Fields(values) => {
            for value in &values {
                headers.append("Cookie", value).map_err(shape_refused)?;
            }
        }
    }
    let url = match query.as_deref() {
        None | Some("") => String::from(PROBE_URL),
        Some(query) => format!("{PROBE_URL}?{query}"),
    };
    let probe = Rc::new(Request::new("GET", url, headers, Body::Empty));
    let context = Context::new(
        probe,
        String::from("/probe"),
        Rc::from(Vec::new()),
        Env::Bindings(Rc::new(Vars::default())),
        None,
    );
    let mut payload = JsObject::new();
    payload.insert(
        "token",
        read_token(&context).map_or_else(undefined, |token| JsValue::from(token.as_str())),
    );
    Ok(payload)
}

/*
 * The operation roles.
 */

/// Answers `roles`, `roles_of` over a claim value, the marker of `undefined` for no claim.
fn op_roles(request: &JsObject) -> Result<JsObject, JsString> {
    let Some(claim) = request.get("claim") else {
        return Err(JsString::from("roles: the request carries no field claim"));
    };
    let claim = decode(claim);
    let claim = if claim.is_undefined() {
        None
    } else {
        Some(&claim)
    };
    let mut payload = JsObject::new();
    match roles_of(claim) {
        Ok(roles) => {
            payload.insert("roles", strings_json(&roles));
        }
        Err(thrown) => {
            payload.insert("thrown", JsValue::Object(error_record(&thrown.0)));
        }
    }
    Ok(payload)
}

/*
 * The operation introspection_body.
 */

thread_local! {
    /// The bodies the recording driver took, in order.
    static RECORDED: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// The driver of the network capability that records each request and answers an inactive token.
struct Recorder;

impl Fetch for Recorder {
    fn fetch(&self, request: Request) -> LocalBoxFuture<'static, Result<Response, FetchError>> {
        Box::pin(async move {
            let body = request
                .text()
                .await
                .map_err(|_| FetchError::Protocol(String::from("the body did not read")))?;
            let _ = RECORDED.try_with(|recorded| recorded.borrow_mut().push(body));
            let init = ResponseInit {
                status: Some(200),
                headers: Some(HeadersInit::Plain(vec![(
                    String::from("content-type"),
                    String::from("application/json"),
                )])),
                ..ResponseInit::default()
            };
            Response::new(Body::Text(Rc::from(INACTIVE)), Some(init))
                .map_err(|error| FetchError::Protocol(error.to_string()))
        })
    }
}

/// Drives `future` to its end with a waker that does nothing.
fn settle<F: Future>(future: F) -> Result<F::Output, JsString> {
    let mut future = pin!(future);
    let mut cx = TaskContext::from_waker(Waker::noop());
    for _ in 0..POLLS {
        if let Poll::Ready(value) = future.as_mut().poll(&mut cx) {
            return Ok(value);
        }
    }
    Err(JsString::from(
        "introspection_body: the introspection did not settle",
    ))
}

/// Returns the form body with the value of the client assertion replaced by its mask, and that value.
///
/// The value of an assertion is a compact JWS, whose characters the form
/// writes as they are, so the value needs no decoding.
fn mask_body(text: &str) -> (String, Option<String>) {
    let mut assertion = None;
    let masked: Vec<String> = text
        .split('&')
        .map(|pair| {
            let name = pair.split('=').next().unwrap_or_default();
            if name != ASSERTION {
                return String::from(pair);
            }
            assertion = Some(String::from(pair.get(name.len() + 1..).unwrap_or_default()));
            format!("{ASSERTION}={ASSERTION_MASK}")
        })
        .collect();
    (masked.join("&"), assertion)
}

/// Returns the decoded part `part` of a compact JWS, an object.
fn part(part: &str) -> Result<JsObject, JsString> {
    let bytes = base64url_decode(part).map_err(|_| {
        JsString::from("introspection_body: a part of the assertion is no base64url")
    })?;
    match parse_decoded(&bytes) {
        Some(JsValue::Object(object)) => Ok(object),
        _ => Err(JsString::from(
            "introspection_body: a part of the assertion is no JSON object",
        )),
    }
}

/// Returns the decoded assertion with its drawn claims masked, after its signature verified with `pem`.
fn assertion_of(compact: &str, pem: &str) -> Result<JsObject, JsString> {
    let parts: Vec<&str> = compact.split('.').collect();
    let [header, payload, signature] = parts.as_slice() else {
        return Err(JsString::from(format!(
            "introspection_body: the client assertion has {} parts",
            parts.len()
        )));
    };
    let signature = base64url_decode(signature)
        .map_err(|_| JsString::from("introspection_body: the signature is no base64url"))?;
    let signed = format!("{header}.{payload}");
    let verified =
        verify_rs256(pem, signed.as_bytes(), &signature).map_err(|thrown| thrown_text(&thrown))?;
    if !verified {
        return Err(JsString::from(
            "introspection_body: the client assertion does not verify with the key of the request",
        ));
    }
    let header = part(header)?;
    let mut payload = part(payload)?;
    let lifetime = match (payload.get("exp"), payload.get("iat")) {
        (Some(JsValue::Number(exp)), Some(JsValue::Number(iat))) => number(exp - iat),
        _ => number(f64::NAN),
    };
    for claim in DRAWN_CLAIMS {
        if payload.contains_key(claim) {
            payload.insert(claim, format!("<{claim}>").as_str());
        }
    }
    let mut out = JsObject::with_capacity(3);
    out.insert("header", JsValue::Object(header));
    out.insert("payload", JsValue::Object(payload));
    out.insert("lifetime", lifetime);
    Ok(out)
}

/// Answers `introspection_body`, the form body `token_introspection` posts for a token.
///
/// The configuration is the one the bootstrap makes after discovery, over the
/// server metadata `issuer` and an introspection endpoint below it, with the
/// client of `auth_key`. The extra parameters reach the body in the own key
/// order of the object, the order of the `URLSearchParams` of the reference.
fn op_introspection_body(request: &JsObject) -> Result<JsObject, JsString> {
    let op = "introspection_body";
    let issuer = to_string_lossy(string(request, op, "issuer")?).into_owned();
    let token = to_string_lossy(string(request, op, "token")?).into_owned();
    let parameters = record(request, op, "parameters")?;
    let auth_key = record(request, op, "auth_key")?;
    for field in ["keyId", "key", "clientId"] {
        if !matches!(auth_key.get(field), Some(JsValue::String(_))) {
            return Err(JsString::from(format!(
                "{op}: auth_key carries no string {field}"
            )));
        }
    }
    let pem = to_string_lossy(string(auth_key, op, "key")?).into_owned();
    let mut extra = Vec::with_capacity(parameters.len());
    for (name, value) in parameters.own_keys() {
        let JsValue::String(value) = value else {
            return Err(JsString::from(format!(
                "{op}: the parameter {} is no string",
                to_string_lossy(name).into_owned()
            )));
        };
        extra.push((
            to_string_lossy(name).into_owned(),
            to_string_lossy(value).into_owned(),
        ));
    }
    let extra: Vec<(&str, &str)> = extra
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect();

    // A target with no /dev/urandom has no signer either, so its import of
    // the key answers the TypeError of rule 9 before a random byte is asked.
    #[cfg(unix)]
    install_randomness(Box::new(DevUrandom));
    install_fetch(Rc::new(Recorder));
    let (client_id, auth) = client_of(&JsValue::Object(auth_key.clone())).map_err(|thrown| {
        JsString::from(format!(
            "{op}: the key of auth_key does not import: {}",
            to_string_lossy(&thrown_text(&thrown)).into_owned()
        ))
    })?;
    let mut server = JsObject::new();
    let base = issuer.strip_suffix('/').unwrap_or(&issuer);
    server.insert("issuer", issuer.as_str());
    server.insert(
        "introspection_endpoint",
        format!("{base}/oauth/v2/introspect").as_str(),
    );
    let config = configuration(server, &client_id, auth).map_err(|thrown| thrown_text(&thrown))?;
    let _ = RECORDED.try_with(|recorded| recorded.borrow_mut().clear());
    settle(token_introspection(&config, &token, &extra))?.map_err(|thrown| thrown_text(&thrown))?;
    let recorded = RECORDED
        .try_with(|recorded| recorded.borrow_mut().split_off(0))
        .unwrap_or_default();
    let [body] = recorded.as_slice() else {
        return Err(JsString::from(format!(
            "{op}: the introspection sent {} requests",
            recorded.len()
        )));
    };
    let (masked, assertion) = mask_body(body);
    let mut payload = JsObject::new();
    payload.insert("body", masked.as_str());
    payload.insert(
        "assertion",
        match assertion {
            Some(compact) => JsValue::Object(assertion_of(&compact, &pem)?),
            None => JsValue::Null,
        },
    );
    Ok(payload)
}
