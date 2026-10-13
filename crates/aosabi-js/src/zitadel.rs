// SPDX-License-Identifier: MIT
/*
 * aosabi-js zitadel: the Zitadel layer of the app over aosabi-auth-zitadel.
 *
 * No analogue in the reference, whose subpath auth/zitadel runs openid-client
 * in the process of the app.
 */

//! The Zitadel layer of the app.
//!
//! A provider the app makes with `zitadelAuth(options)` of the package is a
//! function of the host that asks the engine for the claims of its request
//! through the member [`CLAIMS`] of the export `ctx`, with its issuer and its
//! key path, on the request the host opened in the engine, whose context is the
//! context of its head, since the middleware of the identity plugin runs in the
//! hono of the host before the request reaches the engine. A provider of the
//! reference holds no state beyond its options and its bootstrap is one per
//! issuer, so the engine keeps one [`ZitadelAuth`] of `aosabi-auth-zitadel` per
//! issuer and key path, made at the first call of the pair. It answers every
//! provider of the app with those options in any order the app makes and calls
//! them, a provider the app makes per request adds none, and the one bootstrap
//! per issuer of the crate holds, so the key of the first call of
//! an issuer signs for every provider of it, quirk `Q10-3`. The engine runs
//! [`ZitadelAuth::claims`] over the context of the request beside the answer of
//! the request, in the drives of its head and of its body: the token of the
//! header, the query or the cookie `aosabi-auth`, the discovery, the
//! introspection and the user info through the `fetch` of the host and the
//! client assertion signed by its crypto. The wrapper makes the promise of the
//! call before it calls, under a number of its own that the record carries, the
//! call answers at once, and the import `js_next` settles that promise with the
//! claims, `undefined` for a request the provider answers no principal for, or
//! with the thrown value. A call the app makes outside every drive of the
//! request, from a microtask of its own, drives the request at once, so its
//! claims start there. The wrapper then maps the claims with `zitadelPrincipal`
//! of the reference and the mappers of the app, functions of the host no
//! function pointer of the crate holds.
//!
//! The marker `zitadelLogin` of the configuration becomes the plugin
//! [`zitadel_login`] of the crate with the issuer, the key path and the
//! endpoint of its options, whose setup registers the three routes of the
//! login in the place the sort of the plugins gives it.

#![forbid(unsafe_code)]

use core::cell::RefCell;
use std::rc::Rc;

use aosabi_auth_zitadel::{
    ZitadelAuth,
    ZitadelAuthOptions,
    ZitadelLoginOptions,
    zitadel_auth,
    zitadel_login, //
};
use aosabi_core::{
    IntoPlugin as _,
    PluginSlot,
    Thrown,
    context::with_context, //
};
use aosabi_execute::value::Value;
use aosabi_http::Context;
use aosabi_json::{
    JsObject,
    JsValue, //
};
use aosabi_lang::js_string::to_string_lossy;

use crate::{
    context,
    host::Reader,
    request, //
};

/// The member of the export `ctx` that asks for the claims of the request,
/// after the issuer of the provider, its key path and the number of the
/// promise of the wrapper the claims settle.
pub const CLAIMS: u8 = 33;

/// One provider of the engine, made at the first call of its issuer and its
/// key path.
struct Provider {
    /// The issuer.
    issuer: String,
    /// The key path, none where the record names none.
    key_path: Option<String>,
    /// The provider of the crate.
    auth: Rc<ZitadelAuth>,
}

// An `unsafe` cell would skip each borrow flag.
thread_local! {
    /// The providers of the engine, one per issuer and key path, in the order
    /// of their first calls.
    static PROVIDERS: RefCell<Vec<Provider>> = const { RefCell::new(Vec::new()) };
}

/// Returns the provider of the engine for `issuer` and `key_path`, made at
/// the first call of the pair.
///
/// The list holds one entry per pair the app has called, so a provider the
/// app makes per request adds none, and a call finds or makes its entry
/// whatever the order of the calls, also in a module the host instantiates
/// anew after a trap, whose list starts empty while the providers of the app
/// stay.
fn provider(issuer: &str, key_path: Option<&str>) -> Rc<ZitadelAuth> {
    PROVIDERS.with_borrow_mut(|providers| {
        if let Some(found) = providers
            .iter()
            .find(|provider| provider.issuer == issuer && provider.key_path.as_deref() == key_path)
        {
            return Rc::clone(&found.auth);
        }
        let auth = Rc::new(zitadel_auth(ZitadelAuthOptions {
            issuer: String::from(issuer),
            key_path: key_path.map(String::from),
            ..ZitadelAuthOptions::default()
        }));
        providers.push(Provider {
            issuer: String::from(issuer),
            key_path: key_path.map(String::from),
            auth: Rc::clone(&auth),
        });
        auth
    })
}

/// Starts the claims of the request of `c` for the provider of the issuer
/// and the key path the record at the cursor of `reader` names, which the
/// import `js_next` hands the promise of the number the record names, and
/// answers `undefined`, or `None` for a broken record.
// Out of line, so a request compiles the read of the claims of the Zitadel layer where it makes
// one.
#[inline(never)]
pub fn start(request: u32, c: &Context, reader: &mut Reader<'_>) -> Option<Result<Value, Thrown>> {
    let issuer = reader.text()?;
    let key_path = match reader.byte()? {
        0 => None,
        _ => Some(reader.text()?),
    };
    let next = reader.word()?;
    let auth = provider(issuer, key_path);
    let context = c.clone();
    request::spawn(
        request,
        Box::pin(with_context(c.clone(), async move {
            let claims = auth.claims(&context).await;
            context::hand_value(
                request,
                next,
                &claims.map(|claims| value_of(claims.as_ref())),
            );
        })),
    );
    Some(Ok(Value::Undefined))
}

/// Returns the value the wrapper maps to a principal: `undefined` for no
/// claims, and otherwise the object `{sub, roles, user}`, the user info with
/// the keys and the order the issuer answered.
fn value_of(claims: Option<&aosabi_auth_zitadel::zitadel_auth::Claims>) -> Value {
    let Some((sub, roles, user)) = claims else {
        return Value::Undefined;
    };
    let mut object = JsObject::new();
    object.insert("sub", sub.as_str());
    object.insert(
        "roles",
        roles
            .iter()
            .map(|role| JsValue::from(role.as_str()))
            .collect::<Vec<_>>(),
    );
    object.insert("user", JsValue::Object(user.clone()));
    Value::from_js(&JsValue::Object(object))
}

/// Returns the plugin of the marker `zitadelLogin` over the options `options`,
/// a plain object of the configuration with its issuer, its key path and its
/// endpoint, each a string or absent.
pub fn login(options: &JsValue) -> PluginSlot {
    let text = |key: &str| match options {
        JsValue::Object(object) => match object.get(key) {
            Some(JsValue::String(text)) => Some(to_string_lossy(text).into_owned()),
            _ => None,
        },
        _ => None,
    };
    PluginSlot::new(
        zitadel_login(ZitadelLoginOptions {
            issuer: text("issuer").unwrap_or_default(),
            key_path: text("keyPath"),
            endpoint: text("endpoint"),
        })
        .into_plugin(),
    )
}
