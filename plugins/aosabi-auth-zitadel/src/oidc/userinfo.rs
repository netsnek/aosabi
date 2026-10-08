// SPDX-License-Identifier: MIT
/*
 * aosabi-auth-zitadel oidc/userinfo: the user info of a token.
 *
 * Derived from openid-client 6.8.2, build/index.js, MIT.
 * Derived from oauth4webapi 3.8.4, build/index.js, MIT.
 */

//! `fetch_user_info`.
//!
//! The request is a `GET` of the discovered `userinfo_endpoint` with the headers
//! `accept: application/json, application/jwt`, `authorization: Bearer <token>` and
//! `user-agent: openid-client/v6.8.2` in that order, and no redirect is followed. The answer
//! must carry no `WWW-Authenticate` challenge, answer 200 and hold a string `sub` equal to the
//! expected subject.
//!
//! An answer of type `application/jwt` is read as a JWT whose claims are checked and whose
//! signature is not, which is quirk `Q10-4`. Its `alg` must be one the discovery names in
//! `userinfo_signing_alg_values_supported`, since the client names none and the check has no
//! default, so without that list the answer is refused.

use aosabi_core::{
    Thrown,
    net, //
};
use aosabi_http::{
    Body,
    Headers,
    Request,
    Response, //
};
use aosabi_json::{
    JsObject,
    JsValue, //
};

use crate::oidc::{
    authorization::{
        Endpoint,
        resolve_endpoint, //
    },
    code_grant::{
        body_cause,
        check_authentication_challenges,
        check_signing_algorithm,
        same_value_zero, //
    },
    discovery::{
        CLOCK_SKEW,
        CLOCK_TOLERANCE,
        Configuration,
        USER_AGENT,
        assert_string,
        content_type,
        response_json_body, //
    },
    errors::{
        INVALID_RESPONSE,
        JSON_ATTRIBUTE_COMPARISON,
        OauthError,
        OperationProcessingError,
        RESPONSE_IS_NOT_CONFORM,
        error_handler, //
    },
    introspection::{
        assert_argument,
        assert_text,
        body_text,
        validate_audience,
        validate_issuer, //
    },
    jwt::decode_claims,
};

/// The `accept` of a user info request of a client that names no signed answer.
const ACCEPT: &str = "application/json, application/jwt";

/// Sends the user info request for `access_token`, `userInfoRequest(as, client, accessToken,
/// options)`.
///
/// # Errors
///
/// Returns the `OperationProcessingError` of a user info endpoint that is
/// missing or not `https:`, the coded `TypeError` of an empty token and the
/// `TypeError` `fetch failed` of the call.
async fn user_info_request(
    config: &Configuration,
    access_token: &str,
) -> Result<Response, OauthError> {
    let url = resolve_endpoint(config, Endpoint::UserInfo)?;
    assert_text(access_token, "\"accessToken\"")?;
    let mut headers = Headers::new();
    // A literal name and a literal value are always valid, and a token that
    // is no valid header value answers the `TypeError` of `Headers.set`.
    let _ = headers.set("accept", ACCEPT);
    headers
        .set("authorization", &format!("Bearer {access_token}"))
        .map_err(|error| OauthError::Thrown(Thrown::type_error(&error.message)))?;
    let _ = headers.set("user-agent", USER_AGENT);
    let request = Request::new("GET", url.href.clone(), headers, Body::Empty);
    net::fetch(request).await.map_err(OauthError::Fetch)
}

/// Reads and checks the user info answer, `processUserInfoResponse(as, client, expectedSubject,
/// response)`.
///
/// # Errors
///
/// Returns the failure of each check, in the order of the reference.
async fn process_user_info_response(
    config: &Configuration,
    expected_subject: &JsValue,
    response: &Response,
) -> Result<JsObject, OauthError> {
    check_authentication_challenges(response)?;
    if response.status() != 200 {
        return Err(OperationProcessingError::new(
            "\"response\" is not a conform UserInfo Endpoint response (unexpected HTTP status \
             code)",
            RESPONSE_IS_NOT_CONFORM,
            JsValue::Object(JsObject::new()),
        )
        .into());
    }
    let json = if content_type(response).as_deref() == Some("application/jwt") {
        let text = body_text(response).await?;
        let supported = config
            .server_metadata()
            .get("userinfo_signing_alg_values_supported");
        let decoded = decode_claims(
            &text,
            |header| check_signing_algorithm(supported, None, header),
            CLOCK_SKEW,
            CLOCK_TOLERANCE,
        )?;
        if !matches!(decoded.claims.get("aud"), None | Some(JsValue::Undefined)) {
            validate_audience(config.client_id(), &decoded.claims)?;
        }
        if !matches!(decoded.claims.get("iss"), None | Some(JsValue::Undefined)) {
            validate_issuer(config, &decoded.claims)?;
        }
        decoded.claims
    } else {
        response_json_body(response).await?
    };
    assert_string(
        json.get("sub"),
        "\"response\" body \"sub\" property",
        INVALID_RESPONSE,
        || body_cause(json.clone()),
    )?;
    assert_argument(expected_subject, "\"expectedSubject\"")?;
    if !same_value_zero(
        json.get("sub").unwrap_or(&JsValue::Undefined),
        expected_subject,
    ) {
        let mut cause = JsObject::new();
        cause.insert("expected", expected_subject.clone());
        cause.insert("body", json.clone());
        cause.insert("attribute", "sub");
        return Err(OperationProcessingError::new(
            "unexpected \"response\" body \"sub\" property value",
            JSON_ATTRIBUTE_COMPARISON,
            JsValue::Object(cause),
        )
        .into());
    }
    Ok(json)
}

/// Fetches the user info of `access_token`, `fetchUserInfo(config, accessToken, expectedSubject)`.
///
/// `expected_subject` is any value, since the provider hands the `sub` of the introspection as
/// it is, which the check of the answer holds to a string after the request is sent, as
/// `assertString` of the reference does.
///
/// # Errors
///
/// Returns what `errorHandler` throws for a failed request or a failed check
/// of the answer.
pub async fn fetch_user_info(
    config: &Configuration,
    access_token: &str,
    expected_subject: &JsValue,
) -> Result<JsObject, Thrown> {
    let response = user_info_request(config, access_token)
        .await
        .map_err(error_handler)?;
    process_user_info_response(config, expected_subject, &response)
        .await
        .map_err(error_handler)
}
