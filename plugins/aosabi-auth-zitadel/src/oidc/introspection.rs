// SPDX-License-Identifier: MIT
/*
 * aosabi-auth-zitadel oidc/introspection: the introspection of a token.
 *
 * Derived from openid-client 6.8.2, build/index.js, MIT.
 * Derived from oauth4webapi 3.8.4, build/index.js, MIT.
 */

//! `token_introspection` and `IntrospectionResponse`.
//!
//! The request is a `POST` to the discovered `introspection_endpoint` whose form body is the
//! extra parameters, then `token`, then `client_id`, `client_assertion_type` and
//! `client_assertion` of the client authentication, with the headers
//! `accept: application/json`, `content-type: application/x-www-form-urlencoded;charset=UTF-8`
//! and `user-agent: openid-client/v6.8.2` in that order. The request carries no signal, so the
//! network capability ends it at its own 30 seconds with the `TypeError` `fetch failed`, as it
//! ends the token request.
//!
//! An answer of type `application/token-introspection+jwt` is read as a JWT whose header and
//! claims are checked and whose signature is not, which is quirk `Q10-4`. Its claim
//! `token_introspection` is the answer. Any other answer is read as JSON, and either answer
//! must carry a boolean `active`.

use aosabi_core::{
    Thrown,
    net, //
};
use aosabi_http::{
    Body,
    Headers,
    Request,
    Response,
    fetch::url::UrlSearchParams, //
};
use aosabi_json::{
    JsObject,
    JsValue,
    case::lower_ascii_exact, //
};
use aosabi_lang::js_string::to_string_lossy;

use crate::oidc::{
    authorization::{
        Endpoint,
        resolve_endpoint, //
    },
    code_grant::{
        FORM,
        body_cause,
        check_oauth_body_error,
        check_signing_algorithm,
        claims_cause,
        comparison_cause,
        expected_issuer,
        same_value_zero, //
    },
    discovery::{
        CLOCK_SKEW,
        CLOCK_TOLERANCE,
        Configuration,
        USER_AGENT,
        content_type,
        response_json_body, //
    },
    errors::{
        CodedTypeError,
        ERR_INVALID_ARG_TYPE,
        ERR_INVALID_ARG_VALUE,
        INVALID_RESPONSE,
        JWT_CLAIM_COMPARISON,
        OauthError,
        OperationProcessingError,
        error_handler, //
    },
    jwt::{
        DecodedJwt,
        decode_claims, //
    },
};

/// The content type of an introspection answer that is a JWT.
const JWT_TYPE: &str = "application/token-introspection+jwt";

/// The answer of the introspection endpoint, `IntrospectionResponse`.
///
/// It holds the JSON body, or the claim `token_introspection` of a JWT answer,
/// whose `active` is a boolean.
#[derive(Clone, Debug, PartialEq)]
pub struct IntrospectionResponse(JsObject);

impl IntrospectionResponse {
    /// Returns `active`, which the check of the answer holds to a boolean.
    #[must_use]
    pub fn active(&self) -> bool {
        matches!(self.0.get("active"), Some(JsValue::Bool(true)))
    }

    /// Returns the member `name` of the answer, `introspection[name]`.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&JsValue> {
        self.0.get(name)
    }
}

/// Checks that `input` is a string that is not empty, `assertString(input, it)` with no code.
///
/// # Errors
///
/// Returns the coded `TypeError` of a value that is no string or an empty string.
pub(crate) fn assert_argument(input: &JsValue, it: &str) -> Result<(), CodedTypeError> {
    match input {
        JsValue::String(text) if text.is_empty() => Err(CodedTypeError::new(
            format!("{it} must not be empty"),
            ERR_INVALID_ARG_VALUE,
        )),
        JsValue::String(_) => Ok(()),
        _ => Err(CodedTypeError::new(
            format!("{it} must be a string"),
            ERR_INVALID_ARG_TYPE,
        )),
    }
}

/// Checks that the text `input` is not empty, `assertString(input, it)` of a value that is a string.
///
/// The text of a caller is always a string, so the empty text alone fails,
/// with the coded `TypeError` of [`assert_argument`].
///
/// # Errors
///
/// Returns the coded `TypeError` of an empty text.
pub(crate) fn assert_text(input: &str, it: &str) -> Result<(), CodedTypeError> {
    if input.is_empty() {
        return Err(CodedTypeError::new(
            format!("{it} must not be empty"),
            ERR_INVALID_ARG_VALUE,
        ));
    }
    Ok(())
}

/// Returns the body of `response` as text, the `TypeError` of its stream when it fails.
pub(crate) async fn body_text(response: &Response) -> Result<String, OauthError> {
    response
        .text()
        .await
        .map_err(|error| OauthError::Thrown(Thrown::type_error(&error.to_string())))
}

/// The names `jwtClaimNames` gives the claims a check of this crate requires.
///
/// The reference names fifteen claims, and the two callers require these five alone, the ID
/// token all five and a JWT introspection answer `aud`, `iat` and `iss`, so a name outside the
/// table reads `undefined`, as the reference reads a name outside its own.
const CLAIM_NAMES: [(&str, &str); 5] = [
    ("aud", "audience"),
    ("exp", "expiration time"),
    ("iat", "issued at"),
    ("iss", "issuer"),
    ("sub", "subject"),
];

/// Checks that the claims carry each of `required`, `validatePresence(required, result)`.
///
/// The ID token and the JWT answer of an introspection pass it, each with
/// its required claims.
///
/// # Errors
///
/// Returns the `OperationProcessingError` of the first claim that is missing.
pub(crate) fn validate_presence(claims: &JsObject, required: &[&str]) -> Result<(), OauthError> {
    for claim in required {
        if matches!(claims.get(claim), None | Some(JsValue::Undefined)) {
            let name = CLAIM_NAMES
                .iter()
                .find(|(known, _)| known == claim)
                .map_or("undefined", |(_, label)| *label);
            return Err(OperationProcessingError::new(
                format!("JWT \"{claim}\" ({name}) claim missing"),
                INVALID_RESPONSE,
                JsValue::Object(claims_cause(claims)),
            )
            .into());
        }
    }
    Ok(())
}

/// Checks that `iss` is the issuer of the server, `validateIssuer(as, result)`.
///
/// # Errors
///
/// Returns the `OperationProcessingError` of an issuer that differs.
pub(crate) fn validate_issuer(config: &Configuration, claims: &JsObject) -> Result<(), OauthError> {
    let expected = expected_issuer(config, claims);
    if same_value_zero(claims.get("iss").unwrap_or(&JsValue::Undefined), &expected) {
        return Ok(());
    }
    Err(OperationProcessingError::new(
        "unexpected JWT \"iss\" (issuer) claim value",
        JWT_CLAIM_COMPARISON,
        comparison_cause(expected.into_owned(), claims, "iss"),
    )
    .into())
}

/// Checks that `aud` is or holds the id of the client, `validateAudience(expected, result)`.
///
/// # Errors
///
/// Returns the `OperationProcessingError` of an audience that differs.
pub(crate) fn validate_audience(client_id: &str, claims: &JsObject) -> Result<(), OauthError> {
    let ok = match claims.get("aud") {
        Some(JsValue::Array(list)) => list.iter().any(|item| is_text(item, client_id)),
        other => other.is_some_and(|aud| is_text(aud, client_id)),
    };
    if ok {
        return Ok(());
    }
    Err(OperationProcessingError::new(
        "unexpected JWT \"aud\" (audience) claim value",
        JWT_CLAIM_COMPARISON,
        comparison_cause(JsValue::from(client_id), claims, "aud"),
    )
    .into())
}

/// Returns whether `value` is the string `text`, `SameValueZero(value, text)`.
pub(crate) fn is_text(value: &JsValue, text: &str) -> bool {
    matches!(value, JsValue::String(string) if *string == text)
}

/// Checks the `typ` of the header against `expected`, `checkJwtType(expected, result)`.
///
/// The value is read in lower case and without a leading `application/`, `normalizeTyp`. The
/// lower case is that of `lower_ascii_exact`, which maps every character whose lower case of
/// `toLowerCase` holds an ASCII character, U+212A KELVIN SIGN to `k` among them, so a
/// comparison with the ASCII `expected` answers as the reference's answers. The lowered text is
/// compared alone and reaches no byte.
fn check_jwt_type(decoded: &DecodedJwt, expected: &str) -> Result<(), OauthError> {
    let normalized = match decoded.header.get("typ") {
        Some(JsValue::String(text)) => {
            let typ = lower_ascii_exact(&to_string_lossy(text));
            Some(
                typ.strip_prefix("application/")
                    .map(String::from)
                    .unwrap_or(typ),
            )
        }
        _ => None,
    };
    if normalized.as_deref() == Some(expected) {
        return Ok(());
    }
    let mut cause = JsObject::new();
    cause.insert("header", decoded.header.clone());
    Err(OperationProcessingError::new(
        "unexpected JWT \"typ\" header parameter value",
        INVALID_RESPONSE,
        JsValue::Object(cause),
    )
    .into())
}

/// Posts `token` to the introspection endpoint, `introspectionRequest(as, client, auth, token,
/// options)`.
///
/// # Errors
///
/// Returns the coded `TypeError` of an empty token, the
/// `OperationProcessingError` of an introspection endpoint that is missing or
/// not `https:`, the failure of the client authentication and the
/// `TypeError` `fetch failed` of the call.
async fn introspection_request(
    config: &Configuration,
    token: &str,
    parameters: &[(&str, &str)],
) -> Result<Response, OauthError> {
    assert_text(token, "\"token\"")?;
    let url = resolve_endpoint(config, Endpoint::Introspection)?;
    let mut body = UrlSearchParams::new();
    for (name, value) in parameters {
        body.append(name, value);
    }
    body.set("token", token);
    config
        .client_authentication()
        .authenticate(config.issuer(), config.client_id(), &mut body)?;
    let mut headers = Headers::new();
    // A literal name and a literal value are always valid.
    let _ = headers.set("accept", "application/json");
    let _ = headers.set("content-type", FORM);
    let _ = headers.set("user-agent", USER_AGENT);
    let request = Request::new(
        "POST",
        url.href.clone(),
        headers,
        Body::Text(body.to_string().into()),
    );
    net::fetch(request).await.map_err(OauthError::Fetch)
}

/// Reads and checks the answer of the introspection endpoint, `processIntrospectionResponse(as,
/// client, response)`.
///
/// # Errors
///
/// Returns the failure of each check, in the order of the reference.
async fn process_introspection_response(
    config: &Configuration,
    response: &Response,
) -> Result<JsObject, OauthError> {
    check_oauth_body_error(response, 200, "Introspection Endpoint").await?;
    let json = if content_type(response).as_deref() == Some(JWT_TYPE) {
        let text = body_text(response).await?;
        let supported = config
            .server_metadata()
            .get("introspection_signing_alg_values_supported");
        let decoded = decode_claims(
            &text,
            |header| check_signing_algorithm(supported, Some("RS256"), header),
            CLOCK_SKEW,
            CLOCK_TOLERANCE,
        )?;
        check_jwt_type(&decoded, "token-introspection+jwt")?;
        validate_presence(&decoded.claims, &["aud", "iat", "iss"])?;
        validate_issuer(config, &decoded.claims)?;
        validate_audience(config.client_id(), &decoded.claims)?;
        match decoded.claims.get("token_introspection") {
            Some(JsValue::Object(claim)) => claim.clone(),
            _ => {
                return Err(OperationProcessingError::new(
                    "JWT \"token_introspection\" claim must be a JSON object",
                    INVALID_RESPONSE,
                    JsValue::Object(claims_cause(&decoded.claims)),
                )
                .into());
            }
        }
    } else {
        response_json_body(response).await?
    };
    if !matches!(json.get("active"), Some(JsValue::Bool(_))) {
        return Err(OperationProcessingError::new(
            "\"response\" body \"active\" property must be a boolean",
            INVALID_RESPONSE,
            body_cause(json.clone()),
        )
        .into());
    }
    Ok(json)
}

/// Introspects `token` at the issuer, `tokenIntrospection(config, token, parameters)`.
///
/// `parameters` are the extra parameters of the form body in their order,
/// which come before `token`.
///
/// # Errors
///
/// Returns what `errorHandler` throws for a failed request or a failed check
/// of the answer.
pub async fn token_introspection(
    config: &Configuration,
    token: &str,
    parameters: &[(&str, &str)],
) -> Result<IntrospectionResponse, Thrown> {
    let response = introspection_request(config, token, parameters)
        .await
        .map_err(error_handler)?;
    let json = process_introspection_response(config, &response)
        .await
        .map_err(error_handler)?;
    Ok(IntrospectionResponse(json))
}
