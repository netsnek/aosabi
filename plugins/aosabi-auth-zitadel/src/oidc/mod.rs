// SPDX-License-Identifier: MIT
/*
 * aosabi-auth-zitadel oidc: the OIDC client subset of the two halves.
 *
 * No file of its own on the reference side. This file holds the module list.
 */

//! The OIDC client subset that the login half and the provider half reach.
//!
//! The modules, each with what it translates and what it holds:
//!
//! - `discovery` translates `discovery` of `openid-client` and the discovery request of
//!   `oauth4webapi` and holds the discovered configuration and the issuer check.
//! - `authorization` translates `buildAuthorizationUrl` and the random and PKCE functions and
//!   holds the authorization URL, the state and the code verifier.
//! - `code_grant` translates `authorizationCodeGrant` and the token endpoint functions and holds
//!   the code grant and the token endpoint response.
//! - `client_auth` translates `PrivateKeyJwt` and `signJwt` of `oauth4webapi` and holds the client
//!   assertion.
//! - `introspection` translates `tokenIntrospection` and the introspection functions and holds the
//!   introspection request and its answer.
//! - `userinfo` translates `fetchUserInfo` and the user info and resource functions and holds the
//!   user info request and its answer.
//! - `errors` translates the error classes of both packages and holds the errors both halves
//!   throw.
//! - `jwt` translates RFC 7515, the compact serialization, and holds base64url and the compact
//!   form of a JWS.

pub mod authorization;
pub mod client_auth;
pub mod code_grant;
pub mod discovery;
pub mod errors;
pub mod introspection;
pub mod jwt;
pub mod userinfo;

pub use self::{
    authorization::{
        build_authorization_url,
        calculate_pkce_code_challenge,
        random_pkce_code_verifier,
        random_state, //
    },
    client_auth::PrivateKeyJwt,
    code_grant::{
        AuthorizationCodeGrantChecks,
        TokenEndpointResponse,
        authorization_code_grant, //
    },
    discovery::{
        Configuration,
        discovery,
        perform_discovery, //
    },
    errors::{
        AuthorizationResponseError,
        ClientError,
        OauthError,
        OperationProcessingError,
        ResponseBodyError,
        UnsupportedOperationError,
        WWWAuthenticateChallengeError, //
    },
    introspection::{
        IntrospectionResponse,
        token_introspection, //
    },
    userinfo::fetch_user_info,
};
