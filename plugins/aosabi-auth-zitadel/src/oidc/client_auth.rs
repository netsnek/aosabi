// SPDX-License-Identifier: MIT
/*
 * aosabi-auth-zitadel oidc/client_auth: the client authentication
 * private_key_jwt.
 *
 * Derived from oauth4webapi 3.8.4, build/index.js, MIT.
 */

//! `private_key_jwt` and `client_assertion_payload`.
//!
//! The client authenticates at the token and the introspection endpoint with
//! a JWT it signs, whose header is `{"alg":"RS256","kid":...}` and whose
//! payload is `{jti, aud, exp, iat, nbf, iss, sub}` in that order, `exp`
//! sixty seconds after `iat`, `now` in whole seconds and `jti` 32 random
//! bytes in base64url. The clock skew of the client is 0, since the
//! reference sets none.

use std::rc::Rc;

use aosabi_core::random;
use aosabi_http::fetch::url::UrlSearchParams;
use aosabi_json::{
    JsObject,
    JsValue, //
};

use aosabi_lang::js_string::to_string_lossy;

use crate::{
    crypto::{
        PrivateKey,
        sign_rs256, //
    },
    oidc::{
        errors::{
            CodedTypeError,
            ERR_INVALID_ARG_TYPE,
            ERR_INVALID_ARG_VALUE,
            OauthError, //
        },
        jwt::{
            base64url_encode,
            encode_compact,
            epoch_time, //
        },
    },
};

/// The `client_assertion_type` of a JWT bearer assertion, RFC 7523.
pub const CLIENT_ASSERTION_TYPE: &str = "urn:ietf:params:oauth:client-assertion-type:jwt-bearer";

/// The client authentication `PrivateKeyJwt({key, kid})` answers.
#[derive(Clone, Debug)]
pub struct PrivateKeyJwt {
    /// The private key of the client.
    key: Rc<PrivateKey>,
    /// The `kid` of the header, absent when the key file names none.
    kid: Option<String>,
}

/// Returns the client authentication over `key` and `kid`, `PrivateKeyJwt({key, kid})`.
///
/// A `kid` that is present is a string that is not empty, which is the
/// check of `getKeyAndKid`.
///
/// # Errors
///
/// Returns the coded `TypeError` `"kid" must be a string` or `"kid" must not
/// be empty` for a `kid` that fails the check.
pub(crate) fn private_key_jwt(
    key: PrivateKey,
    kid: &JsValue,
) -> Result<PrivateKeyJwt, CodedTypeError> {
    let kid = match kid {
        JsValue::Undefined => None,
        JsValue::String(text) if text.is_empty() => {
            return Err(CodedTypeError::new(
                "\"kid\" must not be empty",
                ERR_INVALID_ARG_VALUE,
            ));
        }
        JsValue::String(text) => Some(to_string_lossy(text).into_owned()),
        _ => {
            return Err(CodedTypeError::new(
                "\"kid\" must be a string",
                ERR_INVALID_ARG_TYPE,
            ));
        }
    };
    Ok(PrivateKeyJwt {
        key: Rc::new(key),
        kid,
    })
}

/// Returns 32 random bytes in base64url, `randomBytes()`.
///
/// # Errors
///
/// Returns the `TypeError` of a process with no source of random bytes,
/// which Node never meets.
pub(crate) fn random_bytes() -> Result<String, OauthError> {
    let mut bytes = [0_u8; 32];
    random::fill(&mut bytes)
        .map_err(|error| OauthError::Thrown(aosabi_core::Thrown::type_error(&error.to_string())))?;
    Ok(base64url_encode(&bytes))
}

/// Returns the claims of a client assertion, `clientAssertionPayload(as, client)`.
///
/// `issuer` is the `issuer` of the server metadata and `client_id` the id
/// of the client.
///
/// # Errors
///
/// Returns the `TypeError` of a process with no source of random bytes.
pub fn client_assertion_payload(issuer: &JsValue, client_id: &str) -> Result<JsObject, OauthError> {
    let now = epoch_time();
    let mut payload = JsObject::new();
    payload.insert("jti", random_bytes()?);
    payload.insert("aud", issuer.clone());
    payload.insert("exp", now + 60.0);
    payload.insert("iat", now);
    payload.insert("nbf", now);
    payload.insert("iss", client_id);
    payload.insert("sub", client_id);
    Ok(payload)
}

impl PrivateKeyJwt {
    /// Sets the client authentication on `body`, the function `PrivateKeyJwt` answers.
    ///
    /// `body` gains `client_id`, `client_assertion_type` and
    /// `client_assertion` in that order, each set in place where it is
    /// present already. `issuer` is the `issuer` of the server metadata.
    ///
    /// # Errors
    ///
    /// Returns the failure of the signer or of the source of random bytes.
    pub fn authenticate(
        &self,
        issuer: &JsValue,
        client_id: &str,
        body: &mut UrlSearchParams,
    ) -> Result<(), OauthError> {
        let mut header = JsObject::new();
        header.insert("alg", "RS256");
        header.insert(
            "kid",
            self.kid
                .as_deref()
                .map_or(JsValue::Undefined, JsValue::from),
        );
        let header = JsValue::Object(header);
        let payload = JsValue::Object(client_assertion_payload(issuer, client_id)?);
        body.set("client_id", client_id);
        body.set("client_assertion_type", CLIENT_ASSERTION_TYPE);
        let assertion = encode_compact(&header, &payload, |input| sign_rs256(&self.key, input))?;
        body.set("client_assertion", &assertion);
        Ok(())
    }
}
