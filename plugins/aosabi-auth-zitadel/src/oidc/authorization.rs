// SPDX-License-Identifier: MIT
/*
 * aosabi-auth-zitadel oidc/authorization: the authorization URL and its
 * random values.
 *
 * Derived from openid-client 6.8.2, build/index.js, MIT.
 * Derived from oauth4webapi 3.8.4, build/index.js, MIT.
 */

//! `random_pkce_code_verifier`, `calculate_pkce_code_challenge`,
//! `random_state` and `build_authorization_url`.
//!
//! The verifier and the state are 32 random bytes of the randomness capability in base64url
//! with no padding, 43 characters each, and the challenge is base64url of the SHA-256 of the
//! verifier. The authorization URL is the discovered `authorization_endpoint` whose query
//! gains the parameters of the caller in their order, then `client_id` and
//! `response_type=code` where the caller names neither. `openid-client` runs none of these
//! through `errorHandler`, so each failure reaches the caller as the reference throws it.
//!
//! An endpoint of the server metadata is parsed and checked at the first request that sends to
//! it, and the configuration keeps what the check found, the URL or the refusal, for every
//! later request. The reference parses the endpoint at every request, and the parse reads the
//! text of the endpoint alone, which a configuration never changes, so each request meets the
//! URL or the refusal the reference meets, at the same place among its checks.

use core::cell::OnceCell;

use aosabi_http::{
    Url,
    fetch::url::UrlSearchParams,
    parse_url, //
};
use aosabi_json::JsValue;
use aosabi_lang::js_string::to_string_lossy;

use crate::{
    crypto,
    oidc::{
        client_auth::random_bytes,
        discovery::{
            Configuration,
            check_protocol, //
        },
        errors::{
            CodedTypeError,
            ERR_INVALID_ARG_VALUE,
            HTTP_REQUEST_FORBIDDEN,
            INVALID_SERVER_METADATA,
            MISSING_SERVER_METADATA,
            OauthError,
            OperationProcessingError, //
        },
        jwt::base64url_encode,
    },
};

/// Returns a random PKCE code verifier, `randomPKCECodeVerifier()`.
///
/// # Errors
///
/// Returns the `TypeError` of a process with no source of random bytes.
pub fn random_pkce_code_verifier() -> Result<String, OauthError> {
    random_bytes()
}

/// Returns a random `state`, `randomState()`.
///
/// # Errors
///
/// Returns the `TypeError` of a process with no source of random bytes.
pub fn random_state() -> Result<String, OauthError> {
    random_bytes()
}

/// Returns the S256 challenge of `code_verifier`, `calculatePKCECodeChallenge(codeVerifier)`.
///
/// # Errors
///
/// Returns the coded `TypeError` `codeVerifier must not be empty` for an
/// empty verifier, and the `TypeError` of a target with no digest.
pub fn calculate_pkce_code_challenge(code_verifier: &str) -> Result<String, OauthError> {
    if code_verifier.is_empty() {
        return Err(
            CodedTypeError::new("codeVerifier must not be empty", ERR_INVALID_ARG_VALUE).into(),
        );
    }
    let digest = crypto::sha256(code_verifier.as_bytes())?;
    Ok(base64url_encode(&digest))
}

/// Returns whether `input` starts with a scheme other than `http`, `https`, `ws`, `wss` and `ftp`,
/// and its colon.
///
/// A URL of a scheme that is not special parses in `URL.parse` whatever its
/// rest, and so does a `file:` URL, while the parser of the tree refuses
/// both. A malformed URL of the other special schemes fails in `URL.parse`
/// as well, so it stays out of this branch.
fn has_unparsed_scheme(input: &str) -> bool {
    let input = input.trim_matches(|c: char| c <= ' ');
    let Some((scheme, _)) = input.split_once(':') else {
        return false;
    };
    let mut chars = scheme.chars();
    let valid = chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    valid
        && !matches!(
            scheme.to_ascii_lowercase().as_str(),
            "http" | "https" | "ws" | "wss" | "ftp"
        )
}

/// An endpoint of the server metadata that a request of this crate sends to.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Endpoint {
    /// `authorization_endpoint`, which the login redirects to.
    Authorization,
    /// `token_endpoint`, which the callback posts the code to.
    Token,
    /// `introspection_endpoint`, which the provider posts a token to.
    Introspection,
    /// `userinfo_endpoint`, which the provider asks for the user of a token.
    UserInfo,
}

impl Endpoint {
    /// Returns the name of the endpoint in the server metadata.
    const fn name(self) -> &'static str {
        match self {
            Self::Authorization => "authorization_endpoint",
            Self::Token => "token_endpoint",
            Self::Introspection => "introspection_endpoint",
            Self::UserInfo => "userinfo_endpoint",
        }
    }
}

/// What the check of one endpoint found, the URL or the refusal of [`resolve_endpoint`].
type Checked = Result<Url, OperationProcessingError>;

/// The endpoints of one configuration, each checked at its first use.
#[derive(Clone, Debug, Default)]
pub(crate) struct Endpoints {
    /// The check of [`Endpoint::Authorization`].
    authorization: OnceCell<Checked>,
    /// The check of [`Endpoint::Token`].
    token: OnceCell<Checked>,
    /// The check of [`Endpoint::Introspection`].
    introspection: OnceCell<Checked>,
    /// The check of [`Endpoint::UserInfo`].
    user_info: OnceCell<Checked>,
}

impl Endpoints {
    /// Returns the kept check of `endpoint`, empty until its first use.
    fn cell(&self, endpoint: Endpoint) -> &OnceCell<Checked> {
        match endpoint {
            Endpoint::Authorization => &self.authorization,
            Endpoint::Token => &self.token,
            Endpoint::Introspection => &self.introspection,
            Endpoint::UserInfo => &self.user_info,
        }
    }
}

/// Checks the value `value` of the endpoint `endpoint`, the steps of `resolveEndpoint`.
///
/// The value must be a string that parses as a URL and whose scheme is
/// `https:`. A value of a scheme that is not special or of `file:`, which
/// the parser of the tree refuses and `URL.parse` takes, answers the
/// refusal of its protocol, as the reference answers it. Any other value
/// that fails to parse, a malformed `https:` URL among them, answers the
/// invalid metadata error, as `URL.parse` gives `null` for it.
fn check_endpoint(value: Option<&JsValue>, endpoint: Endpoint) -> Checked {
    let text = match value {
        Some(JsValue::String(text)) => Some(to_string_lossy(text).into_owned()),
        _ => None,
    };
    let parsed = text.as_deref().map(|text| parse_url(text, None));
    match parsed {
        Some(Ok(url)) => {
            check_protocol(&url, true)?;
            Ok(url)
        }
        Some(Err(_)) if text.as_deref().is_some_and(has_unparsed_scheme) => {
            Err(OperationProcessingError::new(
                "only requests to HTTPS are allowed",
                HTTP_REQUEST_FORBIDDEN,
                JsValue::from(text.as_deref().unwrap_or_default()),
            ))
        }
        _ => {
            let code = if matches!(value, None | Some(JsValue::Undefined)) {
                MISSING_SERVER_METADATA
            } else {
                INVALID_SERVER_METADATA
            };
            let name = endpoint.name();
            let mut cause = aosabi_json::JsObject::new();
            cause.insert("attribute", name);
            Err(OperationProcessingError::new(
                format!("authorization server metadata does not contain a valid \"as.{name}\""),
                code,
                JsValue::Object(cause),
            ))
        }
    }
}

/// Returns the endpoint `endpoint` of the server metadata as a URL, `resolveEndpoint(as, endpoint,
/// false, true)`.
///
/// The first call of a configuration checks the endpoint with
/// [`check_endpoint`] and keeps the URL or the refusal, and every call
/// answers the URL or a copy of the refusal, so a caller meets the refusal
/// at the place of its own checks where the reference meets it.
///
/// # Errors
///
/// Returns the `OperationProcessingError` of a missing or invalid endpoint
/// and of an endpoint that is not `https:`.
pub(crate) fn resolve_endpoint(
    config: &Configuration,
    endpoint: Endpoint,
) -> Result<&Url, OauthError> {
    config
        .endpoints()
        .cell(endpoint)
        .get_or_init(|| check_endpoint(config.server_metadata().get(endpoint.name()), endpoint))
        .as_ref()
        .map_err(|refusal| refusal.clone().into())
}

/// Returns where the query or the fragment of `href` starts, the length of `href` where it has
/// neither.
///
/// The scheme, the userinfo, the host and the path hold no `?` and no `#`,
/// so the first of them after the scheme starts the query or the fragment.
pub(crate) fn query_start(href: &str) -> usize {
    href.find("://").map_or(href.len(), |at| {
        let authority = at + 3;
        href[authority..]
            .find(['?', '#'])
            .map_or(href.len(), |end| authority + end)
    })
}

/// Returns `url` with `pairs` appended to its query, `url.searchParams.append(k, v)` for each pair.
///
/// The query of `url` is parsed, gains the pairs and is written back by the
/// urlencoded serializer, so a query the endpoint carries is written anew,
/// and the fragment stays.
fn append_search_params(url: &Url, pairs: &UrlSearchParams) -> Url {
    let mut params = UrlSearchParams::from_query(&url.search);
    for (name, value) in pairs.pairs() {
        params.append(name, value);
    }
    let serialized = params.to_string();
    // A query holds no `#`, so the fragment starts at the first `#` after
    // the start of the query.
    let (head, rest) = url.href.split_at(query_start(&url.href));
    let fragment = rest.find('#').map_or("", |at| &rest[at..]);
    let mut out = url.clone();
    out.href = format!("{head}?{serialized}{fragment}");
    out.search = if serialized.is_empty() {
        String::new()
    } else {
        format!("?{serialized}")
    };
    out
}

/// Returns the URL of the authorization request, `buildAuthorizationUrl(config, parameters)`.
///
/// `parameters` come in their order, `client_id` is set to the id of the
/// client unless they name one, and `response_type` to `code` unless they
/// name one or `request_uri` or `request`. Every pair is appended to the
/// query of the discovered `authorization_endpoint`.
///
/// # Errors
///
/// Returns the `OperationProcessingError` of an endpoint that is missing,
/// that is no URL or that is not `https:`.
pub fn build_authorization_url(
    config: &Configuration,
    parameters: &[(&str, &str)],
) -> Result<Url, OauthError> {
    let endpoint = resolve_endpoint(config, Endpoint::Authorization)?;
    let mut params = UrlSearchParams::new();
    for (name, value) in parameters {
        params.append(name, value);
    }
    if !params.has("client_id") {
        params.set("client_id", config.client_id());
    }
    if !params.has("request_uri") && !params.has("request") && !params.has("response_type") {
        params.set("response_type", "code");
    }
    Ok(append_search_params(endpoint, &params))
}
