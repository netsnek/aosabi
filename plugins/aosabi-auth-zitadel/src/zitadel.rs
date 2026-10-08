// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-auth-zitadel zitadel: the login half and the shared bootstrap.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/auth/zitadel.ts,
 * Apache-2.0. Changed by the Aosabi authors.
 *
 * The options, the reader of the auth key, the import of the private key, the
 * bootstrap cached per issuer and zitadelLogin with its routes login, logout
 * and callback.
 */

//! `zitadel_login`, its options and plugin, and the bootstrap of an issuer.
//!
//! The plugin `zitadel_login` returns is unnamed and has `setup` alone, which
//! registers `GET <endpoint>/login`, `GET <endpoint>/logout` and
//! `GET <endpoint>/callback` in that order, with the endpoint `/auth` by
//! default. The login route sets the cookie `aosabi_code_verifier` and
//! redirects to the authorization URL, the logout route deletes the cookie
//! `aosabi-auth` and redirects to `/`, and the callback route exchanges the
//! code for tokens, sets `aosabi-auth` to the access token and redirects to
//! `/`. The callback runs the bootstrap before it reads its query, so a
//! failed bootstrap answers 500 before the 400 of a missing code. Every other
//! failure is no `HTTPException`, so the root answers it with 500.
//!
//! The bootstrap is shared by the provider and the login routes. It reads the
//! auth key, imports its private key, and discovers the issuer with the key
//! as the client authentication. The key path is no part of the key, so a
//! second caller with the same issuer and another key path shares the first
//! key, which is quirk `Q10-3`. The provider and the routes each keep the
//! slot the cache gave their first call, so a later call reads its slot with
//! no lookup.

use core::{
    cell::{
        Cell,
        OnceCell,
        RefCell, //
    },
    fmt,
    future::Future,
    pin::Pin,
    task::{
        Context as TaskContext,
        Poll,
        Waker, //
    },
};
use std::rc::Rc;

use aosabi_core::{
    App, IntoPlugin, Plugin, PluginObject, Thrown,
    context::get_context,
    execute::value::{
        ErrorName,
        ErrorObject,
        Value, //
    },
    plugin::object::__wrap,
};
use aosabi_http::{
    Context, Env, HttpException, Response,
    helper::cookie::{
        delete_cookie,
        get_cookie,
        set_cookie, //
    },
    parse_url,
    utils::cookie::CookieOptions,
};
use aosabi_json::{
    JsObject,
    JsString,
    JsValue,
    map::IndexMap, //
};
use aosabi_lang::js_string::to_string_lossy;

use crate::{
    crypto::{
        self,
        PrivateKey, //
    },
    oidc::{
        Configuration,
        authorization::{
            build_authorization_url,
            calculate_pkce_code_challenge,
            random_pkce_code_verifier,
            random_state, //
        },
        client_auth::{
            PrivateKeyJwt,
            private_key_jwt, //
        },
        code_grant::{
            AuthorizationCodeGrantChecks,
            authorization_code_grant, //
        },
        discovery::{
            discovery,
            invalid_url, //
        },
        errors::{
            CodedTypeError,
            ERR_INVALID_ARG_TYPE, //
        },
    },
};

/// The key path of both halves when the options name none.
pub(crate) const KEY_PATH: &str = "key.json";

/// The options of `zitadelLogin`, `ZitadelOptions & {endpoint?: string}`.
#[derive(Clone, Debug, Default)]
pub struct ZitadelLoginOptions {
    /// OIDC issuer URL, e.g. `https://acme.zitadel.cloud`.
    pub issuer: String,
    /// Service account key file (PKCS1 PEM JSON), `key.json` by default, or the variable
    /// `AUTH_KEY`.
    pub key_path: Option<String>,
    /// The path the three routes are mounted under, `/auth` by default.
    pub endpoint: Option<String>,
}

/// Returns the plain `new Error(message)` of a thrown value.
fn error(message: &str) -> Thrown {
    Thrown::error(message)
}

/// Reads the auth key of the service account, `loadAuthKey(keyPath)`.
///
/// The variable `AUTH_KEY` of the environment `getContext()` sets is read
/// first when it is not empty, and the file at `keyPath` below the working
/// directory otherwise, each parsed as JSON. On the Node runtime that
/// environment is `process.env`, so the variable of the process is read
/// whatever its bytes, as the reference reads it. It is decoded by
/// `OsStr::to_string_lossy`, with U+FFFD in place of the bytes that are no
/// UTF-8, as node is taken to decode `process.env`.
///
/// # Errors
///
/// Returns `Context not defined` outside a request, `Error reading
/// AUTH_KEY. Make sure it is valid JSON.` for a variable that is no JSON,
/// and `Error reading key file "<keyPath>". Make sure it exists and is valid
/// JSON.` for a file that cannot be read or is no JSON.
pub(crate) fn load_auth_key(key_path: &str) -> Result<JsValue, Thrown> {
    let env_key = match get_context()?.env() {
        Env::Process => {
            std::env::var_os("AUTH_KEY").map(|value| value.to_string_lossy().into_owned())
        }
        bindings @ Env::Bindings(_) => bindings.var("AUTH_KEY"),
    };
    if let Some(env_key) = env_key.filter(|value| !value.is_empty()) {
        return aosabi_json::parse(env_key.as_bytes())
            .map_err(|_| error("Error reading AUTH_KEY. Make sure it is valid JSON."));
    }
    read_key_file(key_path).ok_or_else(|| {
        error(&format!(
            "Error reading key file \"{key_path}\". Make sure it exists and is valid JSON."
        ))
    })
}

/// Returns the JSON of the file at `key_path` below the working directory.
///
/// The bytes are decoded as `readFile(path, 'utf-8')` decodes them, which
/// keeps a byte order mark and replaces every byte that is no UTF-8.
fn read_key_file(key_path: &str) -> Option<JsValue> {
    let bytes = key_file_bytes(key_path)?;
    let text = String::from_utf8_lossy(&bytes);
    aosabi_json::parse(text.as_bytes()).ok()
}

/// Returns the bytes of the file at `key_path` below the working directory
/// of the process.
#[cfg(not(feature = "host-crypto"))]
fn key_file_bytes(key_path: &str) -> Option<Vec<u8>> {
    let cwd = std::env::current_dir().ok()?;
    let path = aosabi_http::path::join(&[&cwd.to_string_lossy(), key_path]);
    std::fs::read(path).ok()
}

/// Returns the bytes of the file at `key_path` below the working directory
/// of the host, which the host reads, since the module of a Worker reads no
/// file of its own and the host of a native library holds the working
/// directory of the app.
#[cfg(feature = "host-crypto")]
fn key_file_bytes(key_path: &str) -> Option<Vec<u8>> {
    crypto::host_read_file(key_path)
}

/// The types `createPrivateKey` takes for its key, as its messages list them.
const KEY_TYPES: &str = "string or an instance of ArrayBuffer, Buffer, TypedArray, or DataView";

/// Returns the type of `value` as a message of Node names it after `Received`.
fn received(value: &JsValue) -> String {
    match value {
        JsValue::Undefined => String::from("undefined"),
        JsValue::Null => String::from("null"),
        JsValue::Bool(b) => format!("type boolean ({b})"),
        JsValue::Number(n) if *n == 0.0 && n.is_sign_negative() => String::from("type number (-0)"),
        JsValue::Number(_) => format!(
            "type number ({})",
            aosabi_json::to_string(value).unwrap_or_default()
        ),
        JsValue::Array(_) => String::from("an instance of Array"),
        _ => String::from("an instance of Object"),
    }
}

/// Returns the coded `TypeError` of `createPrivateKey` for a key of the wrong type.
fn invalid_key(what: &str, value: &JsValue) -> Thrown {
    Thrown::from(CodedTypeError::new(
        format!(
            "The {what} must be of type {KEY_TYPES}. Received {}",
            received(value)
        ),
        ERR_INVALID_ARG_TYPE,
    ))
}

/// Imports the private key of the key file, `importPrivateKey(pkcs1Pem)`.
///
/// The key passes `crypto.createPrivateKey`, which takes a PEM text, and the
/// import for `RSASSA-PKCS1-v1_5` with SHA-256. An object or an array is read
/// as the options of `createPrivateKey`, whose property `key` holds the PEM
/// text. Its other options (`format`, `type`, `passphrase`, `encoding`) are
/// not read, a known limit.
///
/// # Errors
///
/// Returns the errors of `createPrivateKey` on Node 24.13.0: the coded
/// `TypeError` for a key that is no string, the uncoded `TypeError` of its
/// destructuring for `null`, the coded `TypeError` of the property `key.key`
/// for an object or an array, and the failure of the import of [`crypto`].
pub(crate) fn import_private_key(pkcs1_pem: &JsValue) -> Result<PrivateKey, Thrown> {
    let pem = match pkcs1_pem {
        JsValue::String(pem) => pem,
        JsValue::Null => {
            return Err(Thrown::type_error(
                "Cannot destructure property 'key' of 'key' as it is null.",
            ));
        }
        JsValue::Object(options) => match options.get("key") {
            Some(JsValue::String(pem)) => pem,
            inner => {
                return Err(invalid_key(
                    "\"key.key\" property",
                    inner.unwrap_or(&JsValue::Undefined),
                ));
            }
        },
        JsValue::Array(_) => {
            return Err(invalid_key("\"key.key\" property", &JsValue::Undefined));
        }
        _ => return Err(invalid_key("\"key\" argument", pkcs1_pem)),
    };
    // The decoder of node reads the UTF-8 of the text, which carries U+FFFD
    // for each lone surrogate, the text `to_string_lossy` answers.
    crypto::import_private_key(&to_string_lossy(pem))
}

/// Returns the property `name` of the parsed auth key, `authKey[name]`.
///
/// # Errors
///
/// Returns the `TypeError` of a property read on `null`.
fn property(auth_key: &JsValue, name: &str) -> Result<JsValue, Thrown> {
    match auth_key {
        JsValue::Null => Err(Thrown(Value::from(ErrorObject::type_error(format!(
            "Cannot read properties of null (reading '{name}')"
        ))))),
        JsValue::Object(object) => Ok(object.get(name).cloned().unwrap_or_default()),
        _ => Ok(JsValue::Undefined),
    }
}

/// Builds the configuration of `issuer`, the body of the bootstrap.
///
/// The steps run in the order of the reference's expression: the auth key,
/// the issuer URL, `clientId`, the import of `key`, `keyId`, the client
/// authentication and the discovery. On a build with no signer the import
/// throws, so the bootstrap ends before the discovery with that error.
#[cfg_attr(
    not(any(feature = "host-crypto", all(unix, feature = "ring"))),
    expect(unreachable_code, reason = "this target has no signer")
)]
async fn build(issuer: String, key_path: String) -> Result<Rc<Configuration>, Thrown> {
    let auth_key = load_auth_key(&key_path)?;
    let server = parse_url(&issuer, None).map_err(|_| invalid_url(&issuer))?;
    let (client_id, auth) = client_of(&auth_key)?;
    #[cfg(not(any(feature = "host-crypto", all(unix, feature = "ring"))))]
    {
        let _ = (&server, &client_id, &auth);
        return Err(crypto::no_signer());
    }
    let config = discovery(&server, &client_id, auth).await?;
    Ok(Rc::new(config))
}

/// Returns `clientId` and the client authentication of a parsed auth key.
///
/// The steps run in the order of the reference's expression: `clientId`,
/// the import of `key`, `keyId` and `PrivateKeyJwt({key, kid})`. It is
/// hidden, for the devtool, whose primitive `zitadel` signs the client
/// assertion of an introspection body with the key of a run.
///
/// # Errors
///
/// Returns the `TypeError` of a property read on `null`, the failure of the
/// import of the key and the coded `TypeError` of a `keyId` that fails the
/// check of `PrivateKeyJwt`.
#[doc(hidden)]
pub fn client_of(auth_key: &JsValue) -> Result<(JsValue, PrivateKeyJwt), Thrown> {
    let client_id = property(auth_key, "clientId")?;
    let key = import_private_key(&property(auth_key, "key")?)?;
    let kid = property(auth_key, "keyId")?;
    let auth = private_key_jwt(key, &kid).map_err(Thrown::from)?;
    Ok((client_id, auth))
}

/// What a bootstrap answers, the configuration or the value it threw.
type Settled = Result<Rc<Configuration>, Thrown>;

/// The future of a bootstrap in flight.
type Running = Pin<Box<dyn Future<Output = Settled>>>;

/// The state of the bootstrap of one issuer.
enum State {
    /// In flight, with the future while no waiter polls it.
    Running(Option<Running>),
    /// Settled, for the life of the process.
    Settled(Settled),
}

/// The wakers of the waiters of one bootstrap, each at the id its waiter took.
///
/// The slot hands out the ids 0, 1, 2 and on in the order its waiters come,
/// so a register and a removal touch the one place of the id, and a wake
/// reads the wakers in the order of the ids, first in, first out. Each wake
/// takes the wakers out before it wakes any, so a task a wake polls at once
/// meets no borrowed list.
#[derive(Default)]
struct Waiters(RefCell<Vec<Option<Waker>>>);

impl Waiters {
    /// Records `waker` as the waker of the waiter `id`, where the waker it holds wakes another task.
    fn register(&self, id: usize, waker: &Waker) {
        let mut list = self.0.borrow_mut();
        if list.len() <= id {
            list.resize(id + 1, None);
        }
        if let Some(entry) = list.get_mut(id)
            && !entry.as_ref().is_some_and(|kept| kept.will_wake(waker))
        {
            *entry = Some(waker.clone());
        }
    }

    /// Forgets the waiter `id`.
    fn remove(&self, id: usize) {
        if let Some(entry) = self.0.borrow_mut().get_mut(id) {
            *entry = None;
        }
    }

    /// Wakes every waiter once and keeps them, which a driver that leaves asks.
    fn wake_all(&self) {
        let wakers: Vec<Waker> = self.0.borrow().iter().flatten().cloned().collect();
        for waker in wakers {
            waker.wake();
        }
    }

    /// Wakes every waiter once and forgets them all, which the settled bootstrap asks.
    fn settle(&self) {
        let wakers = self.0.take();
        for waker in wakers.into_iter().flatten() {
            waker.wake();
        }
    }
}

/// The slot of one issuer: the bootstrap and the waiters of it.
///
/// The first waiter that polls the bootstrap drives it with the waker of its
/// own task, and every other waiter waits for it to settle, so a step of the
/// bootstrap wakes the one task that drives it. A driver that leaves before
/// the bootstrap settles wakes every waiter, and the first of them that
/// polls drives it from then on, so the bootstrap goes on when the waiter
/// that started it is dropped, as a promise of the reference settles
/// whoever awaits it. With no waiter left the bootstrap rests until the next
/// call of its issuer polls it, since this crate holds no task of its own.
struct Slot {
    /// The bootstrap.
    state: RefCell<State>,
    /// The waiters.
    waiters: Waiters,
    /// The id the next waiter takes, which wraps after `usize::MAX` waiters.
    next: Cell<usize>,
    /// The id of the waiter that drives the bootstrap, none before the first poll and after a
    /// driver leaves.
    driver: Cell<Option<usize>>,
}

thread_local! {
    /// The bootstrap of each issuer, `configCache`.
    static CACHE: RefCell<IndexMap<String, Rc<Slot>>> = RefCell::new(IndexMap::default());
}

/// The slot of the issuer of one caller, the provider or the routes, empty until its first call.
#[derive(Default)]
pub(crate) struct IssuerSlot(OnceCell<Rc<Slot>>);

impl fmt::Debug for IssuerSlot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("IssuerSlot")
            .field(&self.0.get().is_some())
            .finish()
    }
}

/// A waiter of the bootstrap of one issuer.
struct Wait {
    /// The slot.
    slot: Rc<Slot>,
    /// The id of this waiter among the waiters of the slot.
    id: usize,
    /// Whether this waiter recorded its waker with the slot.
    registered: bool,
}

impl Future for Wait {
    type Output = Settled;

    fn poll(self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Settled> {
        let this = self.get_mut();
        let slot = &this.slot;
        let mut future = match &mut *slot.state.borrow_mut() {
            State::Settled(settled) => return Poll::Ready(settled.clone()),
            State::Running(future) => future.take(),
        };
        slot.waiters.register(this.id, cx.waker());
        this.registered = true;
        // The future is out only while its driver polls it, and the settle wakes this waiter.
        let Some(running) = future.as_mut() else {
            return Poll::Pending;
        };
        if slot.driver.get().is_some_and(|driver| driver != this.id) {
            *slot.state.borrow_mut() = State::Running(future);
            return Poll::Pending;
        }
        slot.driver.set(Some(this.id));
        match running.as_mut().poll(cx) {
            Poll::Ready(settled) => {
                *slot.state.borrow_mut() = State::Settled(settled.clone());
                slot.driver.set(None);
                slot.waiters.settle();
                Poll::Ready(settled)
            }
            Poll::Pending => {
                *slot.state.borrow_mut() = State::Running(future);
                Poll::Pending
            }
        }
    }
}

impl Drop for Wait {
    fn drop(&mut self) {
        if !self.registered {
            return;
        }
        self.slot.waiters.remove(self.id);
        if self.slot.driver.get() == Some(self.id) {
            self.slot.driver.set(None);
            self.slot.waiters.wake_all();
        }
    }
}

/// Returns the slot of `issuer`, made with the bootstrap of `key_path` when there is none.
///
/// The cache is read before an entry is made, so the slot of an issuer
/// that has one costs no copy of its name.
fn slot(issuer: &str, key_path: &str) -> Rc<Slot> {
    let made = || {
        Rc::new(Slot {
            state: RefCell::new(State::Running(Some(Box::pin(build(
                String::from(issuer),
                String::from(key_path),
            ))))),
            waiters: Waiters::default(),
            next: Cell::new(0),
            driver: Cell::new(None),
        })
    };
    CACHE
        .try_with(|cache| {
            if let Some(found) = cache.borrow().get(issuer) {
                return Rc::clone(found);
            }
            Rc::clone(
                cache
                    .borrow_mut()
                    .entry(String::from(issuer))
                    .or_insert_with(made),
            )
        })
        .unwrap_or_else(|_| made())
}

/// Runs the one bootstrap of `issuer`, shared by the provider and the login routes,
/// `bootstrap(issuer, keyPath)`.
///
/// The first call of an issuer starts the bootstrap with its key path and
/// caches it before it settles, and every later call of the same issuer
/// awaits that one, whatever its key path, and answers what it settled
/// with, a failure included, for the life of the process. The first call
/// of a caller keeps the slot of its issuer in `kept`, and every later call
/// of that caller reads it there.
///
/// # Errors
///
/// Returns the value the bootstrap threw.
pub(crate) async fn bootstrap(
    kept: &IssuerSlot,
    issuer: &str,
    key_path: &str,
) -> Result<Rc<Configuration>, Thrown> {
    let slot = Rc::clone(kept.0.get_or_init(|| slot(issuer, key_path)));
    let id = slot.next.get();
    slot.next.set(id.wrapping_add(1));
    Wait {
        slot,
        id,
        registered: false,
    }
    .await
}

/// The endpoint the three routes are mounted under when the options name none.
const ENDPOINT: &str = "/auth";

/// The scope the login route asks for.
const SCOPE: &str =
    "openid profile email urn:zitadel:iam:user:resourceowner urn:zitadel:iam:org:projects:roles";

/// The cookie that carries the PKCE code verifier from the login to the callback.
const CODE_VERIFIER_COOKIE: &str = "aosabi_code_verifier";

/// The cookie that carries the access token, which the provider half reads.
pub(crate) const AUTH_COOKIE: &str = "aosabi-auth";

/// The life of the cookie of the code verifier, in seconds.
const CODE_VERIFIER_MAX_AGE: f64 = 300.0;

/// The life of the cookie of the access token when the answer names no `expires_in`, in seconds.
const AUTH_MAX_AGE: f64 = 3600.0;

/// The issuer, the key path and the endpoint the three routes share.
#[derive(Debug)]
struct Routes {
    /// The issuer.
    issuer: String,
    /// The key path.
    key_path: String,
    /// The endpoint.
    endpoint: String,
    /// The slot of the issuer, kept at the first call of a route.
    slot: IssuerSlot,
}

/// Returns the URL of the request of `c`, `new URL(c.req.url)`.
fn request_url(c: &Context) -> Result<aosabi_http::Url, Thrown> {
    let url = c.req().url();
    parse_url(&url, None).map_err(|_| invalid_url(&url))
}

/// Answers `GET <endpoint>/login`, the redirect to the authorization URL.
///
/// The bootstrap runs first, then the verifier and its challenge are made,
/// the cookie `aosabi_code_verifier` is set with `httpOnly` and `maxAge` 300,
/// and the answer redirects to the authorization URL with the scope, the
/// challenge, `S256`, the `redirect_uri` of the origin of the request URL
/// with the endpoint and `/callback`, and a random state.
#[cfg_attr(
    not(any(feature = "host-crypto", all(unix, feature = "ring"))),
    expect(unreachable_code, reason = "this target has no signer")
)]
async fn login(c: Context, routes: Rc<Routes>) -> Result<Response, Thrown> {
    let config = bootstrap(&routes.slot, &routes.issuer, &routes.key_path).await?;
    #[cfg(not(any(feature = "host-crypto", all(unix, feature = "ring"))))]
    {
        let _ = (&c, &config);
        return Err(crypto::no_signer());
    }
    let code_verifier = random_pkce_code_verifier()?;
    let code_challenge = calculate_pkce_code_challenge(&code_verifier)?;
    set_cookie(
        &c,
        CODE_VERIFIER_COOKIE,
        &code_verifier,
        CookieOptions {
            http_only: true,
            max_age: Some(CODE_VERIFIER_MAX_AGE),
            ..CookieOptions::default()
        },
    )?;
    let redirect_uri = format!("{}{}/callback", request_url(&c)?.origin(), routes.endpoint);
    let state = random_state()?;
    let url = build_authorization_url(
        &config,
        &[
            ("scope", SCOPE),
            ("code_challenge", &code_challenge),
            ("code_challenge_method", "S256"),
            ("redirect_uri", &redirect_uri),
            ("state", &state),
        ],
    )?;
    c.redirect(&url.href)
}

/// Answers `GET <endpoint>/logout`, which deletes the cookie `aosabi-auth` and redirects to `/`.
fn logout(c: &Context) -> Result<Response, Thrown> {
    delete_cookie(c, AUTH_COOKIE, CookieOptions::default())?;
    c.redirect("/")
}

/// Returns the first value of `name` in `query`, the member of `c.req.query()`.
fn first<'a>(query: &'a [(String, String)], name: &str) -> Option<&'a str> {
    query
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}

/// Returns the text of `value` for `setCookie`, whose `encodeURIComponent` throws on a lone surrogate.
///
/// # Errors
///
/// Returns the `URIError` `URI malformed` for a string with a lone surrogate,
/// which `encodeURIComponent` throws inside `serialize` of hono.
fn cookie_text(value: &JsString) -> Result<&str, Thrown> {
    value.as_str().ok_or_else(|| {
        Thrown(Value::from(ErrorObject::custom(
            ErrorName::Other(JsString::from("URIError")),
            "URI malformed",
            JsObject::new(),
            None,
        )))
    })
}

/// Answers `GET <endpoint>/callback`, the exchange of the code for the session cookie.
///
/// The bootstrap runs first. A `code` or a `state` that is missing or empty
/// answers the `HTTPException` 400 `Missing authorization code or state`, and
/// a missing or empty cookie `aosabi_code_verifier` the `HTTPException` 400
/// `Missing code verifier`. The grant takes the callback URL, the verifier,
/// the callback's own `state` as the expected one and the pairs of the
/// callback URL as the parameters of the token request. The answer sets
/// `aosabi-auth` to the access token with `httpOnly` and `maxAge` of
/// `expires_in`, 3600 where that is absent or 0, and redirects to `/`. An
/// access token with a lone surrogate answers the `URIError` of
/// [`cookie_text`], as `setCookie` throws it.
#[cfg_attr(
    not(any(feature = "host-crypto", all(unix, feature = "ring"))),
    expect(unreachable_code, reason = "this target has no signer")
)]
async fn callback(c: Context, routes: Rc<Routes>) -> Result<Response, Thrown> {
    let config = bootstrap(&routes.slot, &routes.issuer, &routes.key_path).await?;
    #[cfg(not(any(feature = "host-crypto", all(unix, feature = "ring"))))]
    {
        let _ = (&c, &config);
        return Err(crypto::no_signer());
    }
    let query = c.req().query_all();
    let code = first(&query, "code").filter(|code| !code.is_empty());
    let state = first(&query, "state").filter(|state| !state.is_empty());
    let (Some(_), Some(state)) = (code, state) else {
        return Err(HttpException::new(
            400,
            Some(String::from("Missing authorization code or state")),
        )
        .into());
    };
    let Some(code_verifier) =
        get_cookie(&c, CODE_VERIFIER_COOKIE).filter(|verifier| !verifier.is_empty())
    else {
        return Err(HttpException::new(400, Some(String::from("Missing code verifier"))).into());
    };
    let cb_url = request_url(&c)?;
    let checks = AuthorizationCodeGrantChecks {
        pkce_code_verifier: Some(code_verifier),
        expected_state: Some(String::from(state)),
    };
    let tokens =
        authorization_code_grant(&config, &cb_url, &checks, &cb_url.search_params()).await?;
    let max_age = tokens
        .expires_in
        .filter(|seconds| *seconds != 0.0)
        .unwrap_or(AUTH_MAX_AGE);
    set_cookie(
        &c,
        AUTH_COOKIE,
        cookie_text(&tokens.access_token)?,
        CookieOptions {
            http_only: true,
            max_age: Some(max_age),
            ..CookieOptions::default()
        },
    )?;
    c.redirect("/")
}

/// The plugin `zitadelLogin` returns, unnamed, with `setup` alone.
#[derive(Clone, Debug)]
pub struct ZitadelLoginPlugin {
    /// The issuer, the key path and the endpoint of the routes.
    routes: Rc<Routes>,
}

impl IntoPlugin for ZitadelLoginPlugin {
    fn into_plugin(self) -> Rc<dyn Plugin> {
        let routes = self.routes;
        Rc::new(PluginObject {
            setup: __wrap::setup(move |app: &mut App| {
                let endpoint = &routes.endpoint;
                let login_routes = Rc::clone(&routes);
                app.get(&format!("{endpoint}/login"), move |c: Context| {
                    login(c, Rc::clone(&login_routes))
                });
                app.get(&format!("{endpoint}/logout"), |c: Context| async move {
                    logout(&c)
                });
                let callback_routes = Rc::clone(&routes);
                app.get(&format!("{endpoint}/callback"), move |c: Context| {
                    callback(c, Rc::clone(&callback_routes))
                });
            }),
            ..PluginObject::default()
        })
    }
}

/// Returns the plugin that registers the login, logout and callback routes,
/// `zitadelLogin(options)`.
///
/// The object of the reference carries the key `setup` alone, and the literal
/// writes the one field, so its members are `setup`.
#[must_use]
pub fn zitadel_login(options: ZitadelLoginOptions) -> ZitadelLoginPlugin {
    ZitadelLoginPlugin {
        routes: Rc::new(Routes {
            issuer: options.issuer,
            key_path: options.key_path.unwrap_or_else(|| String::from(KEY_PATH)),
            endpoint: options.endpoint.unwrap_or_else(|| String::from(ENDPOINT)),
            slot: IssuerSlot::default(),
        }),
    }
}
