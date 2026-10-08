// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-auth-zitadel zitadel_auth: the provider half.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/auth/zitadel.ts,
 * Apache-2.0. Changed by the Aosabi authors.
 *
 * The options with their four mappers, the reader of the token,
 * zitadelPrincipal and zitadelAuth.
 */

//! `zitadel_auth`, its options and provider, and `zitadel_principal`.
//!
//! The provider reads the token of a request, answers no principal without
//! one, runs the bootstrap the login half shares, introspects the token with
//! the extra parameter `scope=openid email profile`, answers no principal for
//! an inactive token or one with no `sub`, fetches the user info for that
//! `sub`, takes the roles from the claim `urn:zitadel:iam:org:projects:roles`
//! and maps the claims to the principal. It caches nothing per token, so
//! every request with a token asks the issuer twice. Every failure is thrown,
//! and the identity middleware answers it with 500.
//!
//! The token is the second word of an `Authorization` header split at every
//! space when the first word is `Bearer`, else the query `token`, else the
//! cookie `aosabi-auth`. On a service served on `GraphTP` the listener
//! presents the credential as `authorization` with no query and no cookie, so
//! there the provider reads the `Bearer` token alone.
//!
//! The provider implements the seam with its own marker, so `use_identity`
//! infers it, and an application that keeps the answers by token writes its
//! own provider that calls `identify` of this one.

use std::{
    borrow::Cow,
    rc::Rc, //
};

use aosabi_auth_contract::{
    Principal,
    PrincipalId,
    provider::{
        IdentityProvider,
        ProviderFuture, //
    }, //
};
use aosabi_core::Thrown;
use aosabi_http::{
    Context,
    helper::cookie::get_cookie, //
};
use aosabi_json::{
    JsObject,
    JsString,
    JsValue, //
};
use aosabi_lang::js_string::to_string_lossy;

use crate::{
    oidc::{
        introspection::token_introspection,
        userinfo::fetch_user_info, //
    },
    zitadel::{
        AUTH_COOKIE,
        IssuerSlot,
        KEY_PATH,
        bootstrap, //
    },
};

/// The extra parameter of the introspection, `{scope: 'openid email profile'}`.
const SCOPE: &str = "openid email profile";

/// The claim of the introspection whose keys are the roles.
const ROLES_CLAIM: &str = "urn:zitadel:iam:org:projects:roles";

/// The message V8 gives the `TypeError` of `valueOf` called on the claim when
/// the claim is an object whose own key `valueOf` holds no function.
const NOT_A_FUNCTION: &str =
    "introspection.urn:zitadel:iam:org:projects:roles?.valueOf is not a function";

/// The OIDC user info Zitadel returns, which already carries arbitrary claims.
pub type OidcUser = JsObject;

/// The options of `zitadelAuth`, `ZitadelOptions` with the four mapping functions.
///
/// A mapper is a function pointer, so `Some(|u| ...)` in the literal reads as
/// the TypeScript `id: u => ...`, and a mapper that needs state reads a
/// `static`, since a function pointer captures nothing.
#[derive(Clone, Debug, Default)]
pub struct ZitadelAuthOptions {
    /// OIDC issuer URL, e.g. `https://acme.zitadel.cloud`.
    pub issuer: String,
    /// Service account key file (PKCS1 PEM JSON), `key.json` by default, or the variable
    /// `AUTH_KEY`.
    pub key_path: Option<String>,
    /// Override the principal id (default: the OIDC `sub`).
    pub id: Option<fn(&OidcUser) -> PrincipalId>,
    /// Derive the tenant id (e.g. org) from the claims (default: none).
    pub tenant: Option<fn(&OidcUser) -> Option<PrincipalId>>,
    /// Fine grained permissions, if your claims carry them (default: none).
    pub permissions: Option<fn(&OidcUser) -> Vec<String>>,
    /// Attributes for ABAC rules (default: all claims).
    pub attributes: Option<fn(&OidcUser) -> JsObject>,
}

/// The marker of the Zitadel provider's form of the provider seam.
pub enum ZitadelProvider {}

/// The provider `zitadelAuth` returns: a token of the request to its principal.
#[derive(Clone, Debug)]
pub struct ZitadelAuth {
    /// The options.
    options: Rc<ZitadelAuthOptions>,
    /// The key path, `options.keyPath ?? 'key.json'`.
    key_path: Rc<str>,
    /// The slot of the issuer, kept at the first call of the provider or of a copy of it.
    slot: Rc<IssuerSlot>,
}

/// The claims the provider maps to a principal: the `sub` of the
/// introspection, the roles of its claim and the user info.
pub type Claims = (String, Vec<String>, OidcUser);

impl ZitadelAuth {
    /// Answers the principal of the request of `c`, the function `zitadelAuth` returns.
    ///
    /// # Errors
    ///
    /// Returns the value the bootstrap, the introspection or the user info threw.
    async fn principal(&self, c: &Context) -> Result<Option<Principal>, Thrown> {
        let Some((sub, roles, user)) = self.claims(c).await? else {
            return Ok(None);
        };
        Ok(Some(principal_of(
            Cow::Owned(sub),
            roles,
            Cow::Owned(user),
            &self.options,
        )))
    }

    /// Answers the claims of the request of `c` that the provider maps to its
    /// principal, or none for a request with no token, an inactive token or
    /// one with no `sub`, every step of `zitadel.ts:135-148` but the mapping.
    ///
    /// The host of the TypeScript option maps the claims with the mappers of
    /// its app, functions of the host that no function pointer of
    /// [`ZitadelAuthOptions`] holds, through `zitadelPrincipal` of the
    /// reference, so the provider answers them before the mapping.
    ///
    /// # Errors
    ///
    /// Returns the value the bootstrap, the introspection or the user info threw.
    #[cfg_attr(
        not(any(feature = "host-crypto", all(unix, feature = "ring"))),
        expect(unreachable_code, reason = "this build has no signer")
    )]
    pub async fn claims(&self, c: &Context) -> Result<Option<Claims>, Thrown> {
        let Some(token) = read_token(c).filter(|token| !token.is_empty()) else {
            return Ok(None);
        };
        let config = bootstrap(&self.slot, &self.options.issuer, &self.key_path).await?;
        #[cfg(not(any(feature = "host-crypto", all(unix, feature = "ring"))))]
        {
            let _ = (&token, &config);
            return Err(crate::crypto::no_signer());
        }
        let introspection = token_introspection(&config, &token, &[("scope", SCOPE)]).await?;
        let sub = introspection.get("sub").unwrap_or(&JsValue::Undefined);
        if !introspection.active() || !sub.truthy() {
            return Ok(None);
        }
        let user = fetch_user_info(&config, &token, sub).await?;
        let roles = roles_of(introspection.get(ROLES_CLAIM))?;
        // The user info check holds `sub` to a string that is not empty.
        let sub = match sub {
            JsValue::String(text) => to_string_lossy(text).into_owned(),
            _ => String::new(),
        };
        Ok(Some((sub, roles, user)))
    }
}

impl IdentityProvider<Context, Thrown, ZitadelProvider> for ZitadelAuth {
    fn identify<'a>(&'a self, context: &'a Context) -> ProviderFuture<'a, Thrown> {
        Box::pin(self.principal(context))
    }
}

/// Returns the provider that introspects the request's token against the issuer.
#[must_use]
pub fn zitadel_auth(options: ZitadelAuthOptions) -> ZitadelAuth {
    let key_path = Rc::from(options.key_path.as_deref().unwrap_or(KEY_PATH));
    ZitadelAuth {
        options: Rc::new(options),
        key_path,
        slot: Rc::new(IssuerSlot::default()),
    }
}

/// Maps the claims to the principal, exported for testing without an OIDC server.
///
/// `id` is the mapper's answer or `sub`, `tenant` the mapper's answer or
/// none, `roles` the roles, `permissions` the mapper's answer or an empty
/// list, and `attributes` the mapper's answer or the user info whole.
#[must_use]
pub fn zitadel_principal(
    sub: &str,
    roles: Vec<String>,
    user: &OidcUser,
    options: &ZitadelAuthOptions,
) -> Principal {
    principal_of(Cow::Borrowed(sub), roles, Cow::Borrowed(user), options)
}

/// Maps the claims to the principal as [`zitadel_principal`] does, over a `sub` and a user info
/// the caller owns or lends.
///
/// An owned `sub` becomes the id and an owned user info the attributes
/// with no copy where no mapper answers them, and a lent one is copied
/// there.
fn principal_of(
    sub: Cow<'_, str>,
    roles: Vec<String>,
    user: Cow<'_, OidcUser>,
    options: &ZitadelAuthOptions,
) -> Principal {
    Principal {
        id: options
            .id
            .map_or_else(|| PrincipalId::from(sub.into_owned()), |id| id(&user)),
        tenant: options.tenant.and_then(|tenant| tenant(&user)),
        roles: Some(roles),
        permissions: Some(
            options
                .permissions
                .map_or_else(Vec::new, |permissions| permissions(&user)),
        ),
        attributes: Some(match options.attributes {
            Some(attributes) => attributes(&user),
            None => user.into_owned(),
        }),
    }
}

/// Pulls the bearer, query or cookie token off the request, hidden, for the devtool.
///
/// A header that is present and not empty and whose first word is `Bearer`
/// answers its second word, none when it has one word alone and the empty
/// string after two spaces, and never falls through. Otherwise the query
/// `token` answers, the empty string included, and then the cookie
/// `aosabi-auth`.
#[doc(hidden)]
#[must_use]
pub fn read_token(c: &Context) -> Option<String> {
    if let Some(header) = c
        .req()
        .header("Authorization")
        .filter(|header| !header.is_empty())
    {
        match header.strip_prefix("Bearer") {
            Some("") => return None,
            Some(rest) if rest.starts_with(' ') => return Some(second_word(header)),
            _ => {}
        }
    }
    c.req()
        .query("token")
        .or_else(|| get_cookie(c, AUTH_COOKIE))
}

/// Returns the second word of `header`, which starts with `Bearer` and a space, cut out in place.
///
/// The word starts after the first space and runs to the next space or the
/// end, the second item of `header.split(' ')`.
///
/// # Panics
///
/// Panics where `header` is shorter than `Bearer` and a space, or where its
/// first seven bytes end inside a character, which [`read_token`] rules out
/// before it calls.
fn second_word(mut header: String) -> String {
    let start = "Bearer ".len();
    let end = header[start..]
        .find(' ')
        .map_or(header.len(), |at| start + at);
    header.truncate(end);
    header.drain(..start);
    header
}

/// Returns the length of a string in UTF-16 code units, its JavaScript length.
///
/// Every code point is one unit and one above U+FFFF two, and a lone
/// surrogate, which WTF-8 writes in three bytes, is one.
fn utf16_len(text: &JsString) -> usize {
    text.as_wtf8()
        .iter()
        .map(|byte| match byte {
            0x80..=0xBF => 0,
            0xF0..=0xFF => 2,
            _ => 1,
        })
        .sum()
}

/// Returns `Object.keys(claim?.valueOf() || {})`, hidden, for the devtool.
///
/// An object answers its keys in the V8 own key order, the canonical array
/// indices ascending first and the other keys in insertion order. A string
/// answers the indices of its UTF-16 code units and an array the indices of
/// its elements. A number, a boolean, null and an absent claim answer none.
///
/// # Errors
///
/// Returns the `TypeError` of the call of `valueOf` for an object with an own
/// key `valueOf`, such as a role of that name, since that key shadows the
/// inherited method and a JSON value is never a function.
#[doc(hidden)]
pub fn roles_of(claim: Option<&JsValue>) -> Result<Vec<String>, Thrown> {
    let indices = |count: usize| (0..count).map(|index| index.to_string()).collect();
    Ok(match claim {
        Some(JsValue::Object(object)) if object.contains_key("valueOf") => {
            return Err(Thrown::type_error(NOT_A_FUNCTION));
        }
        Some(JsValue::Object(object)) => object
            .own_keys()
            .map(|(key, _)| to_string_lossy(key).into_owned())
            .collect(),
        Some(JsValue::String(text)) => indices(utf16_len(text)),
        Some(JsValue::Array(items)) => indices(items.len()),
        _ => Vec::new(),
    })
}
