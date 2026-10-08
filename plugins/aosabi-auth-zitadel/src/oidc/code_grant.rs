// SPDX-License-Identifier: MIT
/*
 * aosabi-auth-zitadel oidc/code_grant: the authorization code grant.
 *
 * Derived from openid-client 6.8.2, build/index.js, MIT.
 * Derived from oauth4webapi 3.8.4, build/index.js, MIT.
 */

//! `authorization_code_grant` and `TokenEndpointResponse`.
//!
//! The grant reads the parameters of the callback URL, checks them against the expected
//! state, posts the code to the token endpoint and checks the answer.
//!
//! The body of the token request is the query of the callback in its order with `code`
//! set in place, then `redirect_uri`, `code_verifier`, `grant_type`, `client_id`,
//! `client_assertion_type` and `client_assertion`, sent as a `POST` with the headers
//! `accept`, `content-type` and `user-agent` in that order. The request names no
//! `accept-encoding` and carries no signal. So the network capability ends it at its own
//! 30 seconds with the `TypeError` `fetch failed`.
//!
//! An ID token in the answer is decoded and its claims checked: `aud`, `exp`, `iat`,
//! `iss` and `sub` present, `iss` the issuer, `aud` the client, `azp` for more than one
//! audience, `auth_time` a number and no `nonce`. Its signature is not checked, since
//! the non repudiation option is off.
//!
//! A string of the answer stays a [`JsString`] up to the place that reads it as text.
//! The access token keeps a lone surrogate the answer escapes, as the string of the
//! reference keeps it, so the callback of the login half meets it where `setCookie`
//! meets it. The token type and the ID token are read as text through `to_string_lossy`
//! of `aosabi-lang`, which takes U+FFFD for each lone surrogate, and that changes no
//! answer, since such a type names neither `dpop` nor `bearer` and such a token decodes
//! to no JSON.
//!
//! The token type is lowered by `lower_ascii_exact` of `aosabi-json`, which maps every
//! character whose lower case holds an ASCII character as `toLowerCase` maps it, U+212A
//! KELVIN SIGN to `k` among them, and keeps every other one, so the type compares with
//! `dpop` and `bearer` as the reference's compares, and the crate links none of the
//! Unicode tables of `str::to_lowercase`. The type written back into the body is the
//! reference's for every type the check takes, and a body reaches the cause of an error
//! alone, which no answer and no log record writes.

use std::borrow::Cow;

use aosabi_core::{
    Thrown,
    net, //
};
use aosabi_http::{
    Body,
    Headers,
    Request,
    Response,
    Url,
    fetch::url::UrlSearchParams, //
};
use aosabi_json::{
    JsObject,
    JsString,
    JsValue,
    case::lower_ascii_exact,
    is_js_space, //
};
use aosabi_lang::js_string::to_string_lossy;

use crate::oidc::{
    authorization::{
        Endpoint,
        query_start,
        resolve_endpoint, //
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
        AuthorizationResponseError,
        CodedTypeError,
        ERR_INVALID_ARG_VALUE,
        INVALID_RESPONSE,
        JWT_CLAIM_COMPARISON,
        OauthError,
        OperationProcessingError,
        RESPONSE_IS_NOT_CONFORM,
        RESPONSE_IS_NOT_JSON,
        ResponseBodyError,
        UnsupportedOperationError,
        WWWAuthenticateChallengeError,
        error_handler, //
    },
    introspection::{
        is_text,
        validate_audience,
        validate_issuer,
        validate_presence, //
    },
    jwt::{
        DecodedJwt,
        decode_claims, //
    },
};

/// The `content-type` of a form body the client sends.
pub(crate) const FORM: &str = "application/x-www-form-urlencoded;charset=UTF-8";

/// The checks of the authorization response and of the grant, `AuthorizationCodeGrantChecks`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AuthorizationCodeGrantChecks {
    /// The PKCE code verifier, `pkceCodeVerifier`, no PKCE when absent or empty.
    pub pkce_code_verifier: Option<String>,
    /// The `state` the response must carry, `expectedState`, none when absent.
    pub expected_state: Option<String>,
}

/// The answer of the token endpoint, `TokenEndpointResponse`.
#[derive(Clone, Debug, PartialEq)]
pub struct TokenEndpointResponse {
    /// The body as checked, `token_type` in lower case and `expires_in` a number when present.
    pub body: JsObject,
    /// `access_token`, with a lone surrogate the answer escapes kept.
    pub access_token: JsString,
    /// `expires_in`, in seconds, absent when the body names none.
    pub expires_in: Option<f64>,
    /// The checked claims of the ID token, absent when the body carries none, `claims()`.
    pub claims: Option<JsObject>,
}

/// Returns the string of a string value, `None` for any other value.
///
/// The string stays a [`JsString`], so it keeps a lone surrogate up to the
/// place that reads it as text.
fn text_of(value: Option<&JsValue>) -> Option<JsString> {
    match value {
        Some(JsValue::String(text)) => Some(text.clone()),
        _ => None,
    }
}

/// Returns whether `a` and `b` are the same value, `SameValueZero`, which `includes` compares with.
///
/// Two objects or two arrays are never the same here, since every value of
/// a parsed body is an object of its own.
#[expect(
    clippy::float_cmp,
    reason = "SameValueZero compares two doubles exactly, as JavaScript does"
)]
pub(crate) fn same_value_zero(a: &JsValue, b: &JsValue) -> bool {
    match (a, b) {
        (JsValue::Undefined, JsValue::Undefined) | (JsValue::Null, JsValue::Null) => true,
        (JsValue::Bool(x), JsValue::Bool(y)) => x == y,
        (JsValue::Number(x), JsValue::Number(y)) => x == y || (x.is_nan() && y.is_nan()),
        (JsValue::String(x), JsValue::String(y)) => x == y,
        _ => false,
    }
}

/// Returns whether `c` ends the match of `.`, a line terminator.
fn is_line_terminator(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// Returns `value` as `String(value)` writes it.
fn js_to_string(value: &JsValue) -> String {
    match value {
        JsValue::Undefined => String::from("undefined"),
        JsValue::Null => String::from("null"),
        JsValue::Bool(b) => b.to_string(),
        JsValue::Number(_) => aosabi_json::to_string(value).unwrap_or_default(),
        JsValue::String(text) => to_string_lossy(text).into_owned(),
        JsValue::Array(items) => items
            .iter()
            .map(|item| match item {
                JsValue::Undefined | JsValue::Null => String::new(),
                other => js_to_string(other),
            })
            .collect::<Vec<_>>()
            .join(","),
        JsValue::Object(_) | JsValue::Opaque(_) => String::from("[object Object]"),
    }
}

/// Returns the number at the start of `text`, `parseFloat(text)`.
///
/// White space at the start is skipped, and the longest prefix that is a
/// decimal literal with an optional sign, or `Infinity` with an optional
/// sign, is read. No such prefix answers `NaN`.
fn parse_float(text: &str) -> f64 {
    let text = text.trim_start_matches(is_js_space);
    let bytes = text.as_bytes();
    let mut end = 0;
    if matches!(bytes.first(), Some(b'+' | b'-')) {
        end = 1;
    }
    if text[end..].starts_with("Infinity") {
        let negative = bytes.first() == Some(&b'-');
        return if negative {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
    }
    let digits = |from: usize| {
        bytes[from..]
            .iter()
            .take_while(|byte| byte.is_ascii_digit())
            .count()
    };
    let int = digits(end);
    end += int;
    let mut frac = 0;
    if bytes.get(end) == Some(&b'.') {
        frac = digits(end + 1);
        if int > 0 || frac > 0 {
            end += 1 + frac;
        }
    }
    if int == 0 && frac == 0 {
        return f64::NAN;
    }
    if matches!(bytes.get(end), Some(b'e' | b'E')) {
        let mut at = end + 1;
        if matches!(bytes.get(at), Some(b'+' | b'-')) {
            at += 1;
        }
        let exp = digits(at);
        if exp > 0 {
            end = at + exp;
        }
    }
    text[..end].parse().unwrap_or(f64::NAN)
}

/// Checks that `input` is a finite number at or above zero, `assertNumber(input, true, it, code,
/// cause)`.
///
/// The function `cause` makes the cause only for a value that fails.
///
/// # Errors
///
/// Returns the `OperationProcessingError` with `code` and the cause of a
/// value that is no finite number or is negative.
fn assert_non_negative(
    input: f64,
    it: &str,
    code: &'static str,
    cause: impl FnOnce() -> JsValue,
) -> Result<(), OauthError> {
    if !input.is_finite() {
        return Err(
            OperationProcessingError::new(format!("{it} must be a number"), code, cause()).into(),
        );
    }
    if input < 0.0 {
        return Err(OperationProcessingError::new(
            format!("{it} must be a non-negative number"),
            code,
            cause(),
        )
        .into());
    }
    Ok(())
}

/// Returns `{body}`, the cause of a failed check of a body.
///
/// The body is the parsed object for a check of one of its properties, and
/// the parsed value for a body that is no object.
pub(crate) fn body_cause(body: impl Into<JsValue>) -> JsValue {
    let mut cause = JsObject::new();
    cause.insert("body", body);
    JsValue::Object(cause)
}

/// Returns `{claims}`, the cause of a failed check of a claim set.
pub(crate) fn claims_cause(claims: &JsObject) -> JsObject {
    let mut cause = JsObject::new();
    cause.insert("claims", claims.clone());
    cause
}

/// Returns the one value of `name` in `parameters`, `getURLSearchParameter(parameters, name)`.
///
/// # Errors
///
/// Returns the `OperationProcessingError` of a name given more than once.
fn get_url_search_parameter<'a>(
    parameters: &'a [(String, String)],
    name: &str,
) -> Result<Option<&'a str>, OauthError> {
    let mut values = parameters
        .iter()
        .filter(|(key, _)| key == name)
        .map(|(_, value)| value.as_str());
    let first = values.next();
    if values.next().is_some() {
        return Err(OperationProcessingError::new(
            format!("\"{name}\" parameter must be provided only once"),
            INVALID_RESPONSE,
            JsValue::Undefined,
        )
        .into());
    }
    Ok(first)
}

/// Returns `{expected, parameters}`, the cause of a failed check of an authorization response.
///
/// The parameters are a host object of the reference, which no layer reads,
/// so they stand as `{}`.
fn parameters_cause(expected: Option<JsValue>) -> JsValue {
    let mut cause = JsObject::new();
    if let Some(expected) = expected {
        cause.insert("expected", expected);
    }
    cause.insert("parameters", JsObject::new());
    JsValue::Object(cause)
}

/// Checks the parameters of an authorization response, `validateAuthResponse(as, client,
/// parameters, expectedState)`.
///
/// # Errors
///
/// Returns the `OperationProcessingError` of a JARM response, of a repeated
/// parameter, of a missing or foreign `iss` and of a `state` that differs,
/// the coded `TypeError` of an empty expected state, the
/// `AuthorizationResponseError` of an `error` and the
/// `UnsupportedOperationError` of an `id_token` or a `token`.
pub fn validate_auth_response(
    config: &Configuration,
    parameters: &[(String, String)],
    expected_state: Option<&str>,
) -> Result<Vec<(String, String)>, OauthError> {
    if get_url_search_parameter(parameters, "response")?.is_some_and(|value| !value.is_empty()) {
        return Err(OperationProcessingError::new(
            "\"parameters\" contains a JARM response, use validateJwtAuthResponse() instead of \
             validateAuthResponse()",
            INVALID_RESPONSE,
            parameters_cause(None),
        )
        .into());
    }
    let iss = get_url_search_parameter(parameters, "iss")?.filter(|value| !value.is_empty());
    let state = get_url_search_parameter(parameters, "state")?;
    let metadata = config.server_metadata();
    if iss.is_none()
        && metadata
            .get("authorization_response_iss_parameter_supported")
            .is_some_and(JsValue::truthy)
    {
        return Err(OperationProcessingError::new(
            "response parameter \"iss\" (issuer) missing",
            INVALID_RESPONSE,
            parameters_cause(None),
        )
        .into());
    }
    let issuer = config.issuer();
    if let Some(iss) = iss
        && !matches!(issuer, JsValue::String(text) if *text == iss)
    {
        return Err(OperationProcessingError::new(
            "unexpected \"iss\" (issuer) response parameter value",
            INVALID_RESPONSE,
            parameters_cause(Some(issuer.clone())),
        )
        .into());
    }
    match expected_state {
        None => {
            if state.is_some() {
                return Err(OperationProcessingError::new(
                    "unexpected \"state\" response parameter encountered",
                    INVALID_RESPONSE,
                    parameters_cause(Some(JsValue::Undefined)),
                )
                .into());
            }
        }
        Some(expected) => {
            if expected.is_empty() {
                return Err(CodedTypeError::new(
                    "\"expectedState\" argument must not be empty",
                    ERR_INVALID_ARG_VALUE,
                )
                .into());
            }
            if state != Some(expected) {
                let message = if state.is_none() {
                    "response parameter \"state\" missing"
                } else {
                    "unexpected \"state\" response parameter value"
                };
                return Err(OperationProcessingError::new(
                    message,
                    INVALID_RESPONSE,
                    parameters_cause(Some(JsValue::from(expected))),
                )
                .into());
            }
        }
    }
    if get_url_search_parameter(parameters, "error")?.is_some_and(|value| !value.is_empty()) {
        return Err(AuthorizationResponseError::new(
            "authorization response from the server is an error",
            parameters.to_vec(),
        )
        .into());
    }
    let id_token = get_url_search_parameter(parameters, "id_token")?;
    let token = get_url_search_parameter(parameters, "token")?;
    if id_token.is_some() || token.is_some() {
        return Err(
            UnsupportedOperationError::new("implicit and hybrid flows are not supported").into(),
        );
    }
    Ok(parameters.to_vec())
}

/// Returns `url` with no query and no fragment, `stripParams(url)`.
fn strip_params(url: &Url) -> String {
    String::from(&url.href[..query_start(&url.href)])
}

/// A matched prefix of a header, the captured parts and the rest after it.
type Matched<'a> = (&'a str, &'a str, &'a str);

/// Returns whether `c` is a character of a token of RFC 9110.
fn is_tchar(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(
            c,
            '!' | '#'
                | '$'
                | '%'
                | '&'
                | '\''
                | '*'
                | '+'
                | '-'
                | '.'
                | '^'
                | '_'
                | '`'
                | '|'
                | '~'
        )
}

/// Returns whether `c` is a character of a `token68` before its padding.
fn is_t68(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_' | '~' | '+' | '/')
}

/// Returns `text` without the separators `[,\s]*` at its start.
fn skip_separators(text: &str) -> &str {
    text.trim_start_matches(|c: char| c == ',' || is_js_space(c))
}

/// Returns the token at the start of `text` and the rest, `None` for no token.
fn token(text: &str) -> Option<(&str, &str)> {
    let end = text.find(|c: char| !is_tchar(c)).unwrap_or(text.len());
    (end > 0).then(|| text.split_at(end))
}

/// Returns the rest of a match, `(.*)`, which ends at a line terminator.
fn rest_of_line(text: &str) -> &str {
    text.find(is_line_terminator)
        .map_or(text, |end| &text[..end])
}

/// Matches `^[,\s]*(token)\s*=\s*` and answers the key and the text after the `=`.
fn param_head(text: &str) -> Option<(&str, &str)> {
    let (key, after) = token(skip_separators(text))?;
    let after = after.trim_start_matches(is_js_space).strip_prefix('=')?;
    Some((key, after.trim_start_matches(is_js_space)))
}

/// Matches `quotedParamRE`, a parameter with a quoted value.
fn quoted_param(text: &str) -> Option<Matched<'_>> {
    let (key, after) = param_head(text)?;
    let body = after.strip_prefix('"')?;
    let mut chars = body.char_indices();
    let end = loop {
        match chars.next()? {
            (_, '\\') => {
                chars.next()?;
            }
            (at, '"') => break at,
            _ => {}
        }
    };
    let value = &body[..end];
    let rest = rest_of_line(skip_separators(&body[end + 1..]));
    Some((key, value, rest))
}

/// Matches `unquotedParamRE`, a parameter with a token as its value.
fn unquoted_param(text: &str) -> Option<Matched<'_>> {
    let (key, after) = param_head(text)?;
    let (value, after) = token(after)?;
    Some((key, value, rest_of_line(skip_separators(after))))
}

/// Matches `token68ParamRE` and answers the `token68` and the rest.
fn token68_param(text: &str) -> Option<(&str, &str)> {
    let mut end = text.find(|c: char| !is_t68(c)).unwrap_or(text.len());
    if end == 0 {
        return None;
    }
    for _ in 0..2 {
        if text[end..].starts_with('=') {
            end += 1;
        }
    }
    let after = &text[end..];
    match after.chars().next() {
        None => Some((text, "")),
        Some(c) if c == ',' || is_js_space(c) => {
            Some((&text[..end], rest_of_line(&after[c.len_utf8()..])))
        }
        Some(_) => None,
    }
}

/// Returns the value of a quoted parameter, its escapes read by `JSON.parse` when it holds a `\`.
fn unquote(value: &str) -> String {
    if value.contains('\\')
        && let Ok(JsValue::String(text)) = aosabi_json::parse(format!("\"{value}\"").as_bytes())
    {
        return to_string_lossy(&text).into_owned();
    }
    String::from(value)
}

/// Returns the challenges of a `WWW-Authenticate` header,
/// `parseWwwAuthenticateChallenges(response)`.
///
/// Each challenge is `{scheme, parameters}` with `token68` where it has one,
/// the scheme and the names of the parameters in lower case. A header that
/// does not parse, and a response with no such header, answer `None`.
pub(crate) fn parse_www_authenticate_challenges(response: &Response) -> Option<Vec<JsValue>> {
    let header = response.header("www-authenticate")?;
    let mut challenges = Vec::new();
    let mut rest: Option<&str> = Some(header.as_str());
    while let Some(current) = rest.filter(|text| !text.is_empty()) {
        let (scheme, after_scheme) = token(skip_separators(current))?;
        let scheme = scheme.to_ascii_lowercase();
        if after_scheme
            .chars()
            .next()
            .is_some_and(|c| c != ',' && !is_js_space(c))
        {
            return None;
        }
        let spaced = after_scheme.trim_start_matches(is_js_space);
        let has_parameters =
            spaced.len() < after_scheme.len() && !spaced.contains(is_line_terminator);
        let mut parameters = JsObject::new();
        let mut token68 = None;
        if has_parameters {
            rest = Some(spaced);
            while let Some(text) = rest.filter(|text| !text.is_empty()) {
                if let Some((key, value, after)) = quoted_param(text) {
                    parameters.insert(key.to_ascii_lowercase(), unquote(value));
                    rest = Some(after);
                    continue;
                }
                if let Some((key, value, after)) = unquoted_param(text) {
                    parameters.insert(key.to_ascii_lowercase(), value);
                    rest = Some(after);
                    continue;
                }
                if let Some((value, after)) = token68_param(text) {
                    if !parameters.is_empty() {
                        break;
                    }
                    token68 = Some(value);
                    rest = Some(after);
                    break;
                }
                return None;
            }
        } else {
            rest = (!after_scheme.is_empty()).then_some(after_scheme);
        }
        let mut challenge = JsObject::new();
        challenge.insert("scheme", scheme);
        challenge.insert("parameters", parameters);
        if let Some(token68) = token68 {
            challenge.insert("token68", token68);
        }
        challenges.push(JsValue::Object(challenge));
    }
    (!challenges.is_empty()).then_some(challenges)
}

/// Refuses a response that carries a challenge, `checkAuthenticationChallenges(response)`.
///
/// # Errors
///
/// Returns the `WWWAuthenticateChallengeError` of a parsed challenge.
pub(crate) fn check_authentication_challenges(response: &Response) -> Result<(), OauthError> {
    match parse_www_authenticate_challenges(response) {
        Some(challenges) => Err(WWWAuthenticateChallengeError::new(
            "server responded with a challenge in the WWW-Authenticate HTTP Header",
            JsValue::Array(challenges),
            response.status(),
        )
        .into()),
        None => Ok(()),
    }
}

/// Tells whether the parsed body `json` names an `error` that is a string
/// and not empty, the test `parseOAuthResponseErrorBody` makes.
fn has_error_code(json: &JsObject) -> bool {
    matches!(json.get("error"), Some(JsValue::String(error)) if !error.is_empty())
}

/// Returns the body of an OAuth error answer, `parseOAuthResponseErrorBody(response)`.
///
/// # Errors
///
/// Returns the `OperationProcessingError` of a status from 400 to 499 whose
/// content type is not `application/json`.
async fn parse_oauth_response_error_body(
    response: &Response,
) -> Result<Option<JsObject>, OauthError> {
    if !(400..500).contains(&response.status()) {
        return Ok(None);
    }
    if content_type(response).as_deref() != Some("application/json") {
        return Err(OperationProcessingError::new(
            "\"response\" content-type must be application/json",
            RESPONSE_IS_NOT_JSON,
            JsValue::Object(JsObject::new()),
        )
        .into());
    }
    let Ok(text) = response.text().await else {
        return Ok(None);
    };
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(&text);
    match aosabi_json::parse(text.as_bytes()) {
        Ok(JsValue::Object(json)) if has_error_code(&json) => Ok(Some(json)),
        _ => Ok(None),
    }
}

/// Refuses a response whose status is not `expected`, `checkOAuthBodyError(response, expected,
/// label)`.
///
/// # Errors
///
/// Returns the `WWWAuthenticateChallengeError` of a challenge, the
/// `ResponseBodyError` of an OAuth error body, and the
/// `OperationProcessingError` of any other status.
pub(crate) async fn check_oauth_body_error(
    response: &Response,
    expected: u16,
    label: &str,
) -> Result<(), OauthError> {
    if response.status() == expected {
        return Ok(());
    }
    check_authentication_challenges(response)?;
    if let Some(body) = parse_oauth_response_error_body(response).await? {
        return Err(ResponseBodyError::new(
            "server responded with an error in the response body",
            body,
            response.status(),
        )
        .into());
    }
    Err(OperationProcessingError::new(
        format!("\"response\" is not a conform {label} response (unexpected HTTP status code)"),
        RESPONSE_IS_NOT_CONFORM,
        JsValue::Object(JsObject::new()),
    )
    .into())
}

/// Checks the `alg` of a JWT against `supported` or the default `fallback`,
/// `checkSigningAlgorithm(undefined, supported, fallback, header)`.
///
/// The ID token passes `id_token_signing_alg_values_supported` of the server
/// metadata and the JWT answer of an introspection
/// `introspection_signing_alg_values_supported`, each with the default
/// `RS256`, and the JWT answer of the user info
/// `userinfo_signing_alg_values_supported` with no default, since the client
/// names none.
///
/// # Errors
///
/// Returns the `OperationProcessingError` of an `alg` that the list or the
/// default does not hold, and the uncoded one of a server metadata with no
/// list where no default is given.
pub(crate) fn check_signing_algorithm(
    supported: Option<&JsValue>,
    fallback: Option<&str>,
    header: &JsObject,
) -> Result<(), OauthError> {
    let alg = header.get("alg").unwrap_or(&JsValue::Undefined);
    let (allowed, reason) = match (supported, fallback) {
        (Some(JsValue::Array(list)), _) => (
            list.iter().any(|item| same_value_zero(item, alg)),
            "authorization server metadata",
        ),
        (_, Some(fallback)) => (is_text(alg, fallback), "default value"),
        (_, None) => {
            let mut cause = JsObject::new();
            cause.insert("client", JsValue::Undefined);
            cause.insert("issuer", supported.cloned().unwrap_or_default());
            cause.insert("fallback", JsValue::Undefined);
            return Err(OperationProcessingError {
                message: String::from(
                    "missing client or server configuration to verify used JWT \"alg\" header parameter",
                ),
                code: None,
                cause: JsValue::Object(cause),
            }
            .into());
        }
    };
    if allowed {
        return Ok(());
    }
    let expected = match supported {
        Some(list @ JsValue::Array(_)) => list.clone(),
        _ => fallback.map_or(JsValue::Undefined, JsValue::from),
    };
    let mut cause = JsObject::new();
    cause.insert("header", header.clone());
    cause.insert("expected", expected);
    cause.insert("reason", reason);
    Err(OperationProcessingError::new(
        "unexpected JWT \"alg\" header parameter",
        INVALID_RESPONSE,
        JsValue::Object(cause),
    )
    .into())
}

/// Returns `replacement` with the patterns `$$`, `$&`, `` $` `` and `$'` of `GetSubstitution` read.
fn substitution(matched: &str, before: &str, after: &str, replacement: &str) -> String {
    let mut out = String::new();
    let mut chars = replacement.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '$' {
            out.push(c);
            continue;
        }
        match chars.peek() {
            Some('$') => out.push('$'),
            Some('&') => out.push_str(matched),
            Some('`') => out.push_str(before),
            Some('\'') => out.push_str(after),
            _ => {
                out.push('$');
                continue;
            }
        }
        chars.next();
    }
    out
}

/// Returns the issuer an ID token must name, `as[_expectedIssuer]?.(result) ?? as.issuer`.
///
/// For Microsoft Entra ID the first `{tenantid}` of the issuer becomes the
/// claim `tid`, as `String.prototype.replace` writes it, and every other
/// server answers its issuer as the server metadata holds it.
pub(crate) fn expected_issuer<'a>(
    config: &'a Configuration,
    claims: &JsObject,
) -> Cow<'a, JsValue> {
    let issuer = config.issuer();
    if !config.is_entra_id() {
        return Cow::Borrowed(issuer);
    }
    let text = js_to_string(issuer);
    let tid = js_to_string(claims.get("tid").unwrap_or(&JsValue::Undefined));
    let pattern = "{tenantid}";
    Cow::Owned(match text.find(pattern) {
        Some(at) => {
            let before = &text[..at];
            let after = &text[at + pattern.len()..];
            JsValue::from(format!(
                "{before}{}{after}",
                substitution(pattern, before, after, &tid)
            ))
        }
        None => JsValue::from(text),
    })
}

/// Returns `{expected, claims, claim}`, the cause of a claim that differs.
pub(crate) fn comparison_cause(expected: JsValue, claims: &JsObject, claim: &str) -> JsValue {
    let mut cause = JsObject::new();
    cause.insert("expected", expected);
    cause.insert("claims", claims.clone());
    cause.insert("claim", claim);
    JsValue::Object(cause)
}

/// Checks the claims of an ID token after `validateJwt`, the chain of
/// `processGenericAccessTokenResponse`.
///
/// The presence, the issuer and the audience are the validators the JWT answer of an
/// introspection passes too, chained in the order of the reference, and the checks of
/// `azp` and `auth_time` follow them.
///
/// # Errors
///
/// Returns the `OperationProcessingError` of a missing claim, of an issuer
/// or an audience that differs, of more than one audience with no `azp` or
/// another `azp`, and of an `auth_time` that is no number.
fn check_id_token_claims(config: &Configuration, decoded: &DecodedJwt) -> Result<(), OauthError> {
    let claims = &decoded.claims;
    validate_presence(claims, &["aud", "exp", "iat", "iss", "sub"])?;
    validate_issuer(config, claims)?;
    validate_audience(config.client_id(), claims)?;
    if let Some(JsValue::Array(list)) = claims.get("aud")
        && list.len() != 1
    {
        match claims.get("azp") {
            None | Some(JsValue::Undefined) => {
                let mut cause = claims_cause(claims);
                cause.insert("claim", "aud");
                return Err(OperationProcessingError::new(
                    "ID Token \"aud\" (audience) claim includes additional untrusted audiences",
                    JWT_CLAIM_COMPARISON,
                    JsValue::Object(cause),
                )
                .into());
            }
            Some(azp) if !is_text(azp, config.client_id()) => {
                return Err(OperationProcessingError::new(
                    "unexpected ID Token \"azp\" (authorized party) claim value",
                    JWT_CLAIM_COMPARISON,
                    comparison_cause(JsValue::from(config.client_id()), claims, "azp"),
                )
                .into());
            }
            Some(_) => {}
        }
    }
    match claims.get("auth_time") {
        None | Some(JsValue::Undefined) => Ok(()),
        Some(JsValue::Number(n)) => assert_non_negative(
            *n,
            "ID Token \"auth_time\" (authentication time)",
            INVALID_RESPONSE,
            || JsValue::Object(claims_cause(claims)),
        ),
        Some(_) => Err(OperationProcessingError::new(
            "ID Token \"auth_time\" (authentication time) must be a number",
            INVALID_RESPONSE,
            JsValue::Object(claims_cause(claims)),
        )
        .into()),
    }
}

/// Asserts that the property `name` of `body` is a string that is not empty.
fn assert_body_string(body: &JsObject, name: &str) -> Result<(), OauthError> {
    crate::oidc::discovery::assert_string(
        body.get(name),
        format_args!("\"response\" body \"{name}\" property"),
        INVALID_RESPONSE,
        || body_cause(body.clone()),
    )
}

/// Reads and checks the answer of the token endpoint, `processGenericAccessTokenResponse` and the
/// response of the OAuth 2.0 grant.
///
/// # Errors
///
/// Returns the failure of each check, in the order of the reference.
async fn process_authorization_code_response(
    config: &Configuration,
    response: &Response,
) -> Result<TokenEndpointResponse, OauthError> {
    check_oauth_body_error(response, 200, "Token Endpoint").await?;
    let mut body = response_json_body(response).await?;
    assert_body_string(&body, "access_token")?;
    assert_body_string(&body, "token_type")?;
    let token_type = match body.get("token_type") {
        Some(JsValue::String(text)) => lower_ascii_exact(&to_string_lossy(text)),
        _ => String::new(),
    };
    body.insert("token_type", token_type.as_str());
    let mut expires_in = None;
    if let Some(value) = body.get("expires_in").filter(|value| !value.is_undefined()) {
        let number = match value {
            JsValue::Number(n) => *n,
            other => parse_float(&js_to_string(other)),
        };
        assert_non_negative(
            number,
            "\"response\" body \"expires_in\" property",
            INVALID_RESPONSE,
            || body_cause(body.clone()),
        )?;
        body.insert("expires_in", number);
        expires_in = Some(number);
    }
    if body
        .get("refresh_token")
        .is_some_and(|value| !value.is_undefined())
    {
        assert_body_string(&body, "refresh_token")?;
    }
    if body
        .get("scope")
        .is_some_and(|value| !matches!(value, JsValue::Undefined | JsValue::String(_)))
    {
        return Err(OperationProcessingError::new(
            "\"response\" body \"scope\" property must be a string",
            INVALID_RESPONSE,
            body_cause(body.clone()),
        )
        .into());
    }
    let mut claims = None;
    if body
        .get("id_token")
        .is_some_and(|value| !value.is_undefined())
    {
        assert_body_string(&body, "id_token")?;
        let id_token = match body.get("id_token") {
            Some(JsValue::String(text)) => to_string_lossy(text),
            _ => Cow::Borrowed(""),
        };
        let supported = config
            .server_metadata()
            .get("id_token_signing_alg_values_supported");
        let decoded = decode_claims(
            &id_token,
            |header| check_signing_algorithm(supported, Some("RS256"), header),
            CLOCK_SKEW,
            CLOCK_TOLERANCE,
        )?;
        check_id_token_claims(config, &decoded)?;
        claims = Some(decoded.claims);
    }
    if token_type != "dpop" && token_type != "bearer" {
        return Err(UnsupportedOperationError::new("unsupported `token_type` value").into());
    }
    if let Some(claims) = &claims
        && claims
            .get("nonce")
            .is_some_and(|value| !value.is_undefined())
    {
        return Err(OperationProcessingError::new(
            "unexpected ID Token \"nonce\" claim value",
            JWT_CLAIM_COMPARISON,
            comparison_cause(JsValue::Undefined, claims, "nonce"),
        )
        .into());
    }
    let access_token = text_of(body.get("access_token")).unwrap_or_default();
    Ok(TokenEndpointResponse {
        body,
        access_token,
        expires_in,
        claims,
    })
}

/// Posts the code to the token endpoint, `authorizationCodeGrantRequest` with
/// `tokenEndpointRequest`.
///
/// The body is `additional` in its order, then `redirect_uri`, `code` and
/// `code_verifier` each set in place where present, then `grant_type` and
/// the parameters of the client authentication.
///
/// # Errors
///
/// Returns the `OperationProcessingError` of a repeated or missing code and
/// of a token endpoint that is missing or not `https:`, the coded
/// `TypeError` of an empty redirect URI, the failure of the client
/// authentication and the `TypeError` `fetch failed` of the call.
async fn authorization_code_grant_request(
    config: &Configuration,
    callback_parameters: &[(String, String)],
    redirect_uri: &str,
    code_verifier: Option<&str>,
    additional: &[(String, String)],
) -> Result<Response, OauthError> {
    if redirect_uri.is_empty() {
        return Err(CodedTypeError::new(
            "\"redirectUri\" must not be empty",
            ERR_INVALID_ARG_VALUE,
        )
        .into());
    }
    let code = get_url_search_parameter(callback_parameters, "code")?
        .filter(|code| !code.is_empty())
        .ok_or_else(|| {
            OperationProcessingError::new(
                "no authorization code in \"callbackParameters\"",
                INVALID_RESPONSE,
                JsValue::Undefined,
            )
        })?;
    let mut parameters = UrlSearchParams::new();
    for (name, value) in additional {
        parameters.append(name, value);
    }
    parameters.set("redirect_uri", redirect_uri);
    parameters.set("code", code);
    if let Some(verifier) = code_verifier {
        parameters.set("code_verifier", verifier);
    }
    let url = resolve_endpoint(config, Endpoint::Token)?;
    parameters.set("grant_type", "authorization_code");
    config.client_authentication().authenticate(
        config.issuer(),
        config.client_id(),
        &mut parameters,
    )?;
    let mut headers = Headers::new();
    // A literal name and a literal value are always valid.
    let _ = headers.set("accept", "application/json");
    let _ = headers.set("content-type", FORM);
    let _ = headers.set("user-agent", USER_AGENT);
    let body = Body::Text(parameters.to_string().into());
    let request = Request::new("POST", url.href.clone(), headers, body);
    net::fetch(request).await.map_err(OauthError::Fetch)
}

/// Exchanges the code of `current_url` for tokens, `authorizationCodeGrant(config, currentUrl,
/// checks, tokenEndpointParameters)`.
///
/// `current_url` is the callback URL whose query is the authorization
/// response, and `token_endpoint_parameters` are the extra parameters of the
/// token request in their order.
///
/// # Errors
///
/// Returns what `errorHandler` throws for a failed check of the response, of
/// the request or of the answer of the token endpoint.
pub async fn authorization_code_grant(
    config: &Configuration,
    current_url: &Url,
    checks: &AuthorizationCodeGrantChecks,
    token_endpoint_parameters: &[(String, String)],
) -> Result<TokenEndpointResponse, Thrown> {
    let redirect_uri = strip_params(current_url);
    let auth_response = validate_auth_response(
        config,
        &current_url.search_params(),
        checks.expected_state.as_deref(),
    )
    .map_err(error_handler)?;
    let code_verifier = checks
        .pkce_code_verifier
        .as_deref()
        .filter(|verifier| !verifier.is_empty());
    let response = authorization_code_grant_request(
        config,
        &auth_response,
        &redirect_uri,
        code_verifier,
        token_endpoint_parameters,
    )
    .await
    .map_err(error_handler)?;
    process_authorization_code_response(config, &response)
        .await
        .map_err(error_handler)
}
