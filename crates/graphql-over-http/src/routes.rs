// SPDX-License-Identifier: MIT
/*
 * graphql-over-http: the paths the function answers, the endpoint and the health check.
 *
 * Derived from graphql-yoga 5.18.0, cjs/plugins/use-unhandled-route.js, MIT.
 * Derived from graphql-yoga 5.18.0, cjs/plugins/use-health-check.js, MIT.
 */

//! The paths of the function.
//!
//! A request passes as one for the endpoint when it passes one of the five
//! endpoint tests, the whole URL ending in the endpoint, the whole URL ending
//! in the endpoint and a slash, the path equal to the endpoint, the path equal
//! to the endpoint and a slash, and the URL pattern of the endpoint. The
//! function answers any other request with an empty 404, the unhandled route,
//! and the app passes `landingPage: false`, so the landing page of Yoga never
//! renders and the 404 falls through to the not found handler of the app.
//!
//! The URL pattern of an endpoint that holds no pattern syntax matches the
//! path equal to it, which the third test covers already, and the endpoint of an app
//! is `/graphql`, so [`matches_endpoint`] reads the pattern as that text.
//!
//! The health check answers every request whose whole URL string ends in its
//! endpoint, `/health` by default, whatever its method.

use aosabi_execute::Value;
use aosabi_http::Request;

/// The default endpoint of the health check, `/health`.
pub const DEFAULT_HEALTH_CHECK_ENDPOINT: &str = "/health";

/// Returns whether `request` passes one of the five endpoint tests of `endpoint`.
///
/// # Errors
///
/// Returns the `TypeError` of a URL the parser refuses.
pub fn matches_endpoint(
    request: &Request,
    pathname: impl FnOnce() -> Result<String, Value>,
    endpoint: &str,
) -> Result<bool, Value> {
    let with_slash = format!("{endpoint}/");
    if request.url.ends_with(endpoint) || request.url.ends_with(&with_slash) {
        return Ok(true);
    }
    let pathname = pathname()?;
    Ok(pathname == endpoint || pathname == with_slash)
}
