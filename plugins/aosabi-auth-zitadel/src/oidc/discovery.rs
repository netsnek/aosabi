// SPDX-License-Identifier: MIT
/*
 * aosabi-auth-zitadel oidc/discovery: the discovery of an issuer.
 *
 * Derived from openid-client 6.8.2, build/index.js, MIT.
 * Derived from oauth4webapi 3.8.4, build/index.js, MIT.
 */

//! `discovery`, `perform_discovery` and the issuer check.
//!
//! The issuer is discovered with no `allowInsecureRequests`, so an issuer that is not
//! `https:` is refused with `only requests to HTTPS are allowed`, and an issuer the
//! document names other than the `href` of the issuer URL is refused with
//! `discovered metadata issuer does not match the expected issuer`. The request is a
//! `GET` of `/.well-known/openid-configuration` below the issuer with
//! `accept: application/json` and `user-agent: openid-client/v6.8.2` in that order
//! and a signal that runs out after 30 seconds, whose end the network capability
//! answers as its own timeout of 30 seconds.

use core::fmt::Display;

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
    parse_url, //
};
use aosabi_json::{
    JsObject,
    JsValue, //
};
use aosabi_lang::js_string::to_string_lossy;

use crate::oidc::{
    authorization::Endpoints,
    client_auth::PrivateKeyJwt,
    errors::{
        ClientError,
        CodedTypeError,
        ERR_INVALID_ARG_TYPE,
        HTTP_REQUEST_FORBIDDEN,
        INVALID_RESPONSE,
        JSON_ATTRIBUTE_COMPARISON,
        OauthError,
        OperationProcessingError,
        PARSE_ERROR,
        REQUEST_PROTOCOL_FORBIDDEN,
        RESPONSE_IS_NOT_CONFORM,
        RESPONSE_IS_NOT_JSON,
        error_handler, //
    },
};

/// The `user-agent` of `openid-client` on Node, whose navigator names no browser.
pub const USER_AGENT: &str = "openid-client/v6.8.2";

/// The suffix `appendWellKnown` adds for the algorithm `oidc`.
const WELL_KNOWN: &str = ".well-known/openid-configuration";

/// The clock tolerance of a client that sets none, in seconds.
pub const CLOCK_TOLERANCE: f64 = 30.0;

/// The clock skew of a client that sets none, in seconds.
pub const CLOCK_SKEW: f64 = 0.0;

/// The configuration of a client of a discovered server, `openid.Configuration`.
#[derive(Clone, Debug)]
pub struct Configuration {
    /// The server metadata as discovered, `as`.
    server: JsObject,
    /// The id of the client, `client.client_id`.
    client_id: String,
    /// The client authentication, `auth`.
    auth: PrivateKeyJwt,
    /// Whether the server is Microsoft Entra ID, whose issuer names the tenant.
    entra_id: bool,
    /// The endpoints of the server metadata, each checked at its first use.
    endpoints: Endpoints,
}

impl Configuration {
    /// Returns the server metadata, `config.serverMetadata()`.
    #[must_use]
    pub fn server_metadata(&self) -> &JsObject {
        &self.server
    }

    /// Returns the endpoints of the server metadata as their checks found them.
    pub(crate) fn endpoints(&self) -> &Endpoints {
        &self.endpoints
    }

    /// Returns the id of the client, `config.clientMetadata().client_id`.
    #[must_use]
    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    /// Returns the client authentication.
    #[must_use]
    pub fn client_authentication(&self) -> &PrivateKeyJwt {
        &self.auth
    }

    /// Returns the `issuer` of the server metadata, `undefined` where it names none.
    #[must_use]
    pub fn issuer(&self) -> &JsValue {
        self.server.get("issuer").unwrap_or(&JsValue::Undefined)
    }

    /// Returns whether the server is Microsoft Entra ID, `kEntraId in as`.
    #[must_use]
    pub fn is_entra_id(&self) -> bool {
        self.entra_id
    }
}

/// Discovers `server` and answers the configuration of the client, `discovery(server, clientId,
/// undefined, auth)`.
///
/// `client_id` is the value the key file names, which must be a string
/// that is not empty.
///
/// # Errors
///
/// Returns the thrown value of [`perform_discovery`], and the coded
/// `TypeError` `"clientId" must be a non-empty string` for a `client_id`
/// that fails the check of the constructor.
pub async fn discovery(
    server: &Url,
    client_id: &JsValue,
    client_authentication: PrivateKeyJwt,
) -> Result<Configuration, Thrown> {
    let (metadata, entra_id) = perform_discovery(server).await?;
    let mut config = configuration(metadata, client_id, client_authentication)?;
    config.entra_id = entra_id;
    Ok(config)
}

/// Returns the configuration of a client of the server `server`, `new Configuration(server,
/// clientId, undefined, auth)`.
///
/// `server` is the server metadata. It is hidden, for the devtool, whose
/// primitive `zitadel` writes the introspection body of a configuration
/// that no discovery made.
///
/// # Errors
///
/// Returns the coded `TypeError` `"clientId" must be a non-empty string`
/// for a `client_id` that is no string or is empty.
#[doc(hidden)]
pub fn configuration(
    server: JsObject,
    client_id: &JsValue,
    client_authentication: PrivateKeyJwt,
) -> Result<Configuration, Thrown> {
    let client_id = match client_id {
        JsValue::String(text) if !text.is_empty() => to_string_lossy(text).into_owned(),
        _ => {
            return Err(Thrown::from(CodedTypeError::new(
                "\"clientId\" must be a non-empty string",
                ERR_INVALID_ARG_TYPE,
            )));
        }
    };
    Ok(Configuration {
        server,
        client_id,
        auth: client_authentication,
        entra_id: false,
        endpoints: Endpoints::default(),
    })
}

/// Fetches and checks the server metadata of `server`, `performDiscovery(server)`.
///
/// A URL whose `href` holds `/.well-known/` is fetched as it is and its
/// issuer is not compared. Any other URL gains the suffix of the algorithm
/// `oidc`, and the `issuer` of its document, read as a URL, must have the
/// `href` of `server`, unless `server` is Microsoft Entra ID or Azure AD B2C.
/// The answer holds the metadata and whether the server is Entra ID.
///
/// # Errors
///
/// Returns what `errorHandler` throws for a failed request or document, the
/// `OperationProcessingError` of a URL with `/.well-known/` that is not
/// `https:`, which the reference throws before its handler is in place, the
/// `TypeError` `Invalid URL` of an issuer that is no URL, and the
/// `ClientError` of an issuer that does not match.
pub async fn perform_discovery(server: &Url) -> Result<(JsObject, bool), Thrown> {
    let resolve = !server.href.contains("/.well-known/");
    let url = if resolve {
        discovery_url(server).map_err(error_handler)?
    } else {
        check_protocol(server, true).map_err(Thrown::from)?;
        server.href.clone()
    };
    let metadata = request(&url).await.map_err(error_handler)?;
    if !resolve {
        return Ok((metadata, false));
    }
    let issuer = match metadata.get("issuer") {
        Some(JsValue::String(text)) => to_string_lossy(text).into_owned(),
        _ => String::new(),
    };
    let discovered = parse_url(&issuer, None).map_err(|_| invalid_url(&issuer))?;
    if discovered.href == server.href {
        return Ok((metadata, false));
    }
    if server.origin() == "https://login.microsoftonline.com" {
        return Ok((metadata, true));
    }
    if server.hostname.ends_with(".b2clogin.com") {
        return Ok((metadata, false));
    }
    Err(Thrown::from(ClientError::new(
        "discovered metadata issuer does not match the expected issuer",
        Some(JSON_ATTRIBUTE_COMPARISON),
    )))
}

/// Returns the `TypeError` `Invalid URL` of `new URL(input)`.
pub(crate) fn invalid_url(input: &str) -> Thrown {
    use aosabi_core::execute::value::{
        ErrorName,
        ErrorObject,
        Value, //
    };

    let mut props = JsObject::new();
    props.insert("code", "ERR_INVALID_URL");
    props.insert("input", input);
    Thrown(Value::from(ErrorObject::custom(
        ErrorName::TypeError,
        "Invalid URL",
        props,
        None,
    )))
}

/// Refuses a URL that is not `https:` when `enforce_https` holds, `checkProtocol(url,
/// enforceHttps)`.
///
/// # Errors
///
/// Returns the `OperationProcessingError` of a URL that is not `https:` or
/// neither `http:` nor `https:`.
pub(crate) fn check_protocol(
    url: &Url,
    enforce_https: bool,
) -> Result<(), OperationProcessingError> {
    if enforce_https && url.protocol != "https:" {
        return Err(OperationProcessingError::new(
            "only requests to HTTPS are allowed",
            HTTP_REQUEST_FORBIDDEN,
            JsValue::from(url.href.as_str()),
        ));
    }
    if url.protocol != "https:" && url.protocol != "http:" {
        return Err(OperationProcessingError::new(
            "only HTTP and HTTPS requests are allowed",
            REQUEST_PROTOCOL_FORBIDDEN,
            JsValue::from(url.href.as_str()),
        ));
    }
    Ok(())
}

/// Returns the URL of the discovery document of `server`, `discoveryRequest` up to its fetch.
///
/// The path gains `/` and the suffix, and the first `//` of the result
/// becomes `/`, which is `appendWellKnown` with `replaceDoubleSlash`.
fn discovery_url(server: &Url) -> Result<String, OauthError> {
    check_protocol(server, true)?;
    let mut url = server.clone();
    let pathname = format!("{}/{WELL_KNOWN}", url.pathname).replacen("//", "/", 1);
    url.set_pathname(&pathname);
    Ok(url.href)
}

/// Sends the discovery request to `url` and reads its document,
/// `processDiscoveryResponse(_nodiscoverycheck, response)`.
async fn request(url: &str) -> Result<JsObject, OauthError> {
    let mut headers = Headers::new();
    // A literal name and a literal value are always valid.
    let _ = headers.set("accept", "application/json");
    let _ = headers.set("user-agent", USER_AGENT);
    let request = Request::new("GET", url, headers, Body::Empty);
    let response = net::fetch(request).await.map_err(|error| match error {
        net::FetchError::Timeout => OauthError::TimedOut,
        other => OauthError::Fetch(other),
    })?;
    if response.status() != 200 {
        return Err(OperationProcessingError::new(
            "\"response\" is not a conform Authorization Server Metadata response \
             (unexpected HTTP status code)",
            RESPONSE_IS_NOT_CONFORM,
            JsValue::Object(JsObject::new()),
        )
        .into());
    }
    let json = response_json_body(&response).await?;
    assert_string(
        json.get("issuer"),
        "\"response\" body \"issuer\" property",
        INVALID_RESPONSE,
        || crate::oidc::code_grant::body_cause(json.clone()),
    )?;
    Ok(json)
}

/// Returns the content type of `response` without its parameters, `getContentType(response)`.
///
/// The value of the field is cut at its first `;` in place.
#[must_use]
pub fn content_type(response: &Response) -> Option<String> {
    response.header("content-type").map(|mut value| {
        if let Some(at) = value.find(';') {
            value.truncate(at);
        }
        value
    })
}

/// Reads the body of `response` as a JSON object, `getResponseJsonBody(response)`.
///
/// A body that does not parse is refused for its content type first when
/// that is not `application/json`, and for the parse otherwise.
///
/// # Errors
///
/// Returns the `OperationProcessingError` of a content type that is not
/// `application/json` under a body that does not parse, of a body that does
/// not parse, and of a body that is no object.
pub async fn response_json_body(response: &Response) -> Result<JsObject, OauthError> {
    response_json_body_with(response, |response| {
        if content_type(response).as_deref() == Some("application/json") {
            Ok(())
        } else {
            Err(OperationProcessingError::new(
                "\"response\" content-type must be application/json",
                RESPONSE_IS_NOT_JSON,
                JsValue::Object(JsObject::new()),
            )
            .into())
        }
    })
    .await
}

/// Reads the body of `response` as a JSON object with the check `check` of its content type.
///
/// # Errors
///
/// Returns the failure of `check` under a body that does not parse, the
/// `OperationProcessingError` of a body that does not parse, and of a body
/// that is no object.
pub async fn response_json_body_with(
    response: &Response,
    check: impl FnOnce(&Response) -> Result<(), OauthError>,
) -> Result<JsObject, OauthError> {
    let parsed = match response.text().await {
        Ok(text) => aosabi_json::parse(text.as_bytes()).ok(),
        Err(_) => None,
    };
    let Some(json) = parsed else {
        check(response)?;
        return Err(OperationProcessingError::new(
            "failed to parse \"response\" body as JSON",
            PARSE_ERROR,
            JsValue::Undefined,
        )
        .into());
    };
    match json {
        JsValue::Object(object) => Ok(object),
        other => Err(OperationProcessingError::new(
            "\"response\" body must be a top level object",
            crate::oidc::errors::INVALID_RESPONSE,
            crate::oidc::code_grant::body_cause(other),
        )
        .into()),
    }
}

/// Checks that `input` is a string that is not empty, `assertString(input, it, code, cause)`.
///
/// The message names `it`, and `cause` makes the cause, each only for a
/// value that fails.
///
/// # Errors
///
/// Returns the `OperationProcessingError` with `code` and the cause of a
/// value that is no string or an empty string.
pub fn assert_string(
    input: Option<&JsValue>,
    it: impl Display,
    code: &'static str,
    cause: impl FnOnce() -> JsValue,
) -> Result<(), OauthError> {
    match input {
        Some(JsValue::String(text)) if !text.is_empty() => Ok(()),
        Some(JsValue::String(_)) => {
            Err(
                OperationProcessingError::new(format!("{it} must not be empty"), code, cause())
                    .into(),
            )
        }
        _ => Err(
            OperationProcessingError::new(format!("{it} must be a string"), code, cause()).into(),
        ),
    }
}
