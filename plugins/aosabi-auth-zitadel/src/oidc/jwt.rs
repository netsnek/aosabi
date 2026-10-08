// SPDX-License-Identifier: MIT
/*
 * aosabi-auth-zitadel oidc/jwt: base64url and the compact form of a JWS.
 *
 * No analogue file on the reference side. The format is the compact
 * serialization of RFC 7515, https://www.rfc-editor.org/rfc/rfc7515, with the
 * base64url encoding of RFC 4648 section 5. The functions translate b64u,
 * encodeBase64Url and decodeBase64Url of oauth4webapi 3.8.4,
 * build/index.js:41-92, epochTime of build/index.js:441-443, the input of
 * signJwt of build/index.js:540-547, and validateJwt of
 * build/index.js:1710-1782.
 */

//! `base64url_encode`, `base64url_decode`, `encode_compact` and
//! `decode_claims`.
//!
//! Node 24.13.0 has no `Uint8Array.fromBase64` and no
//! `Uint8Array.prototype.toBase64`, so `oauth4webapi` takes its fallbacks:
//! the encoder writes `btoa` with `+`, `/` and `=` replaced, and the decoder
//! replaces `-` and `_`, drops white space and runs `atob`, the forgiving
//! base64 decode of the HTML Standard. So a decoded input may carry its
//! padding or `+` and `/`, and a length of one more than a multiple of four
//! fails.
//!
//! The encoder writes four characters for each three bytes into one buffer
//! reserved for the whole form. The decoder reads an input of the two
//! alphabets alone four characters at a time through one table, which is
//! the bytes the forgiving decode answers for such an input, and hands every
//! other input, white space, padding and a lone character among them, to the
//! forgiving decode, which answers its bytes or its failure.

use aosabi_json::{
    JsObject,
    JsValue,
    is_js_space, //
};

use crate::oidc::{
    code_grant::claims_cause,
    errors::{
        CodedTypeError,
        ERR_INVALID_ARG_VALUE,
        INVALID_RESPONSE,
        JWT_TIMESTAMP_CHECK,
        OauthError,
        OperationProcessingError,
        PARSE_ERROR,
        UnsupportedOperationError, //
    },
};

/// The alphabet of base64url, RFC 4648 section 5.
const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// The mark of a byte that is no character of either alphabet in [`SEXTETS`].
const NOT_A_CHARACTER: u8 = 0xff;

/// The value of each byte that is a character of base64url or of base64, `+` and `/`, and
/// [`NOT_A_CHARACTER`] for every other byte.
const SEXTETS: [u8; 256] = sextets();

/// The message of a base64url input that does not decode.
const NOT_ENCODED: &str = "The input to be decoded is not correctly encoded.";

/// Returns the table [`SEXTETS`].
const fn sextets() -> [u8; 256] {
    let mut table = [NOT_A_CHARACTER; 256];
    let mut value = 0;
    while value < 64 {
        table[ALPHABET[value as usize] as usize] = value;
        value += 1;
    }
    table[b'+' as usize] = 62;
    table[b'/' as usize] = 63;
    table
}

/// Returns the character of the sextet of `n` that starts `shift` bits up.
fn character(n: u32, shift: u32) -> u8 {
    ALPHABET[((n >> shift) & 0x3f) as usize]
}

/// Returns the base64url form of `bytes` without padding, `b64u(bytes)`.
#[must_use]
pub fn base64url_encode(bytes: &[u8]) -> String {
    let mut out = Vec::with_capacity(bytes.len().div_ceil(3) * 4);
    encode_into(&mut out, bytes);
    text_of(out)
}

/// Appends the base64url form of `bytes` without padding to `out`.
fn encode_into(out: &mut Vec<u8>, bytes: &[u8]) {
    let mut chunks = bytes.chunks_exact(3);
    for chunk in &mut chunks {
        let n = (u32::from(chunk[0]) << 16) | (u32::from(chunk[1]) << 8) | u32::from(chunk[2]);
        out.extend_from_slice(&[
            character(n, 18),
            character(n, 12),
            character(n, 6),
            character(n, 0),
        ]);
    }
    match *chunks.remainder() {
        [a] => {
            let n = u32::from(a) << 16;
            out.extend_from_slice(&[character(n, 18), character(n, 12)]);
        }
        [a, b] => {
            let n = (u32::from(a) << 16) | (u32::from(b) << 8);
            out.extend_from_slice(&[character(n, 18), character(n, 12), character(n, 6)]);
        }
        _ => {}
    }
}

/// Returns the base64url form `out` as text.
///
/// Its bytes are characters of the alphabet and the dots of a compact form,
/// which are ASCII, so the empty text of a failed conversion never answers.
// A `from_utf8_unchecked` would skip this pass.
fn text_of(out: Vec<u8>) -> String {
    String::from_utf8(out).unwrap_or_default()
}

/// Returns the value of one character of either alphabet, `None` for any other byte.
fn sextet(c: u8) -> Option<u32> {
    let value = SEXTETS[usize::from(c)];
    (value != NOT_A_CHARACTER).then_some(u32::from(value))
}

/// Returns the values of up to four characters from the top of 24 bits down, `None` where one
/// is a byte of neither alphabet.
fn sextets_of(characters: &[u8]) -> Option<u32> {
    characters
        .iter()
        .zip([18, 12, 6, 0])
        .try_fold(0, |n, (&c, shift)| Some(n | (sextet(c)? << shift)))
}

/// Returns the bytes of an input of the two alphabets alone whose length leaves no lone character.
///
/// Every four characters are three bytes, and the two or three characters
/// at the end one or two bytes, the bits of the forgiving decode. Any other
/// input answers `None`.
fn decode_plain(input: &[u8]) -> Option<Vec<u8>> {
    let len = input.len();
    if len % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(len / 4 * 3 + (len % 4).saturating_sub(1));
    let mut chunks = input.chunks_exact(4);
    for chunk in &mut chunks {
        let [_, first, second, third] = sextets_of(chunk)?.to_be_bytes();
        out.extend_from_slice(&[first, second, third]);
    }
    let rest = chunks.remainder();
    if !rest.is_empty() {
        let [_, first, second, _] = sextets_of(rest)?.to_be_bytes();
        out.extend_from_slice(&[first, second][..rest.len() - 1]);
    }
    Some(out)
}

/// Returns the bytes of a base64url input, `b64u(input)` for a string.
///
/// An input of the two alphabets alone whose length leaves no lone
/// character decodes through `decode_plain`, and every other input
/// through the forgiving base64 decode.
///
/// # Errors
///
/// Returns the coded `TypeError` `The input to be decoded is not correctly
/// encoded.` when the forgiving base64 decode fails.
pub fn base64url_decode(input: &str) -> Result<Vec<u8>, CodedTypeError> {
    match decode_plain(input.as_bytes()) {
        Some(bytes) => Ok(bytes),
        None => decode_forgiving(input),
    }
}

/// Returns the bytes of a base64url input by the forgiving base64 decode, `atob`.
///
/// The input loses its white space, `-` and `_` read as `+` and `/`, and
/// up to two `=` end an input of a multiple of four characters.
///
/// # Errors
///
/// Returns the coded `TypeError` `The input to be decoded is not correctly
/// encoded.` when the forgiving base64 decode fails.
fn decode_forgiving(input: &str) -> Result<Vec<u8>, CodedTypeError> {
    let fail = || CodedTypeError::new(NOT_ENCODED, ERR_INVALID_ARG_VALUE);
    let mut data = Vec::with_capacity(input.len());
    for c in input.chars().filter(|c| !is_js_space(*c)) {
        match c {
            '-' => data.push(b'+'),
            '_' => data.push(b'/'),
            c if c.is_ascii() => data.push(c as u8),
            _ => return Err(fail()),
        }
    }
    if data.len() % 4 == 0 {
        for _ in 0..2 {
            if data.last() == Some(&b'=') {
                data.pop();
            }
        }
    }
    if data.len() % 4 == 1 {
        return Err(fail());
    }
    let mut out = Vec::with_capacity(data.len() * 3 / 4);
    let mut buffer = 0_u32;
    let mut bits = 0_u32;
    for c in data {
        buffer = buffer << 6 | sextet(c).ok_or_else(fail)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buffer >> bits) & 0xff) as u8);
        }
    }
    Ok(out)
}

/// Returns the compact form of a JWS over `header` and `payload`, signed by `sign`.
///
/// The signing input is the base64url of the JSON of the header, a dot and
/// the base64url of the JSON of the payload, and the answer appends a dot
/// and the base64url of the signature, which is `signJwt`. The header and
/// the payload are objects, each written as it is borrowed.
///
/// # Errors
///
/// Returns the error of `sign`.
pub fn encode_compact<E>(
    header: &JsValue,
    payload: &JsValue,
    sign: impl FnOnce(&[u8]) -> Result<Vec<u8>, E>,
) -> Result<String, E> {
    let header = json_of(header);
    let payload = json_of(payload);
    let mut out =
        Vec::with_capacity(header.len().div_ceil(3) * 4 + 1 + payload.len().div_ceil(3) * 4);
    encode_into(&mut out, &header);
    out.push(b'.');
    encode_into(&mut out, &payload);
    let signature = sign(&out)?;
    out.reserve_exact(1 + signature.len().div_ceil(3) * 4);
    out.push(b'.');
    encode_into(&mut out, &signature);
    Ok(text_of(out))
}

/// Returns the bytes of `JSON.stringify(value)`, which are never absent for an object.
fn json_of(value: &JsValue) -> Vec<u8> {
    aosabi_json::to_bytes(value).unwrap_or_default()
}

/// Returns the seconds since the epoch, `epochTime()`, `Math.floor(Date.now() / 1000)`.
///
/// Under the feature `host-crypto` the clock is the `Date.now()` of the host
/// the crate runs in, which its functions answer. Otherwise it is
/// [`aosabi_http::date::now`], which reads 0 on a Worker until the glue of
/// the Worker hands the host's time.
#[must_use]
pub fn epoch_time() -> f64 {
    #[cfg(feature = "host-crypto")]
    if let Some(ms) = crate::crypto::host_now_ms() {
        return (ms / 1000.0).floor();
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "the seconds of a time value are far below 2^53"
    )]
    let seconds = aosabi_http::date::now().div_euclid(1000) as f64;
    seconds
}

/// Returns the JSON value of decoded bytes, `JSON.parse(buf(bytes))`.
///
/// `buf` is a `TextDecoder`, which drops a leading byte order mark and
/// replaces every byte that is no UTF-8 with U+FFFD.
#[must_use]
pub fn parse_decoded(bytes: &[u8]) -> Option<JsValue> {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let text = String::from_utf8_lossy(bytes);
    aosabi_json::parse(text.as_bytes()).ok()
}

/// A JWT whose header and claims passed `validateJwt`.
#[derive(Clone, Debug, PartialEq)]
pub struct DecodedJwt {
    /// The protected header.
    pub header: JsObject,
    /// The claims of the payload.
    pub claims: JsObject,
    /// The compact form as it arrived.
    pub jwt: String,
}

/// Returns the part of the compact form `part` as a JSON object.
fn decode_part(
    part: &str,
    jws: &str,
    parse_message: &'static str,
    object_message: &'static str,
) -> Result<JsObject, OauthError> {
    let value = base64url_decode(part)
        .ok()
        .and_then(|bytes| parse_decoded(&bytes))
        .ok_or_else(|| {
            OperationProcessingError::new(parse_message, PARSE_ERROR, JsValue::Undefined)
        })?;
    match value {
        JsValue::Object(object) => Ok(object),
        _ => {
            Err(
                OperationProcessingError::new(object_message, INVALID_RESPONSE, JsValue::from(jws))
                    .into(),
            )
        }
    }
}

/// Returns the failure of a claim whose type is not the expected one.
fn claim_type(message: &'static str, claims: &JsObject) -> OauthError {
    let cause = claims_cause(claims);
    OperationProcessingError::new(message, INVALID_RESPONSE, JsValue::Object(cause)).into()
}

/// Returns the failure of a timestamp claim.
fn claim_time(
    message: &'static str,
    claims: &JsObject,
    now: f64,
    tolerance: f64,
    claim: &str,
) -> OauthError {
    let mut cause = claims_cause(claims);
    cause.insert("now", now);
    cause.insert("tolerance", tolerance);
    cause.insert("claim", claim);
    OperationProcessingError::new(message, JWT_TIMESTAMP_CHECK, JsValue::Object(cause)).into()
}

/// Returns the number of the claim `name` when it is present.
///
/// # Errors
///
/// Returns the failure `message` when the claim is present and no number.
fn number_claim(
    claims: &JsObject,
    name: &str,
    message: &'static str,
) -> Result<Option<f64>, OauthError> {
    match claims.get(name) {
        None | Some(JsValue::Undefined) => Ok(None),
        Some(JsValue::Number(n)) => Ok(Some(*n)),
        Some(_) => Err(claim_type(message, claims)),
    }
}

/// Checks the types and the times of the registered claims of `validateJwt`.
fn check_claims(
    claims: &JsObject,
    clock_skew: f64,
    clock_tolerance: f64,
) -> Result<(), OauthError> {
    let now = epoch_time() + clock_skew;
    if let Some(exp) = number_claim(
        claims,
        "exp",
        "unexpected JWT \"exp\" (expiration time) claim type",
    )? && exp <= now - clock_tolerance
    {
        return Err(claim_time(
            "unexpected JWT \"exp\" (expiration time) claim value, expiration is past current \
             timestamp",
            claims,
            now,
            clock_tolerance,
            "exp",
        ));
    }
    number_claim(
        claims,
        "iat",
        "unexpected JWT \"iat\" (issued at) claim type",
    )?;
    if !matches!(
        claims.get("iss"),
        None | Some(JsValue::Undefined | JsValue::String(_))
    ) {
        return Err(claim_type(
            "unexpected JWT \"iss\" (issuer) claim type",
            claims,
        ));
    }
    if let Some(nbf) = number_claim(
        claims,
        "nbf",
        "unexpected JWT \"nbf\" (not before) claim type",
    )? && nbf > now + clock_tolerance
    {
        return Err(claim_time(
            "unexpected JWT \"nbf\" (not before) claim value",
            claims,
            now,
            clock_tolerance,
            "nbf",
        ));
    }
    if !matches!(
        claims.get("aud"),
        None | Some(JsValue::Undefined | JsValue::String(_) | JsValue::Array(_))
    ) {
        return Err(claim_type(
            "unexpected JWT \"aud\" (audience) claim type",
            claims,
        ));
    }
    Ok(())
}

/// Decodes a compact JWT and checks its registered claims, `validateJwt`.
///
/// The header passes through `check_alg` before the payload is read, a
/// header with `crit` is refused, and the claims `exp`, `iat`, `iss`, `nbf`
/// and `aud` are checked for their types and `exp` and `nbf` against the
/// clock. No signature is checked here, and a caller that checks one does so
/// itself. A JWE of five parts is refused, since no decryption is
/// configured.
///
/// # Errors
///
/// Returns the `OperationProcessingError` or the `UnsupportedOperationError`
/// `validateJwt` throws, or the failure of `check_alg`.
pub fn decode_claims(
    jws: &str,
    check_alg: impl FnOnce(&JsObject) -> Result<(), OauthError>,
    clock_skew: f64,
    clock_tolerance: f64,
) -> Result<DecodedJwt, OauthError> {
    let parts: Vec<&str> = jws.split('.').collect();
    if parts.len() == 5 {
        return Err(UnsupportedOperationError::new("JWE decryption is not configured").into());
    }
    if parts.len() != 3 {
        return Err(OperationProcessingError::new(
            "Invalid JWT",
            INVALID_RESPONSE,
            JsValue::from(jws),
        )
        .into());
    }
    let header = decode_part(
        parts[0],
        jws,
        "failed to parse JWT Header body as base64url encoded JSON",
        "JWT Header must be a top level object",
    )?;
    check_alg(&header)?;
    if !matches!(header.get("crit"), None | Some(JsValue::Undefined)) {
        return Err(UnsupportedOperationError::new(
            "no JWT \"crit\" header parameter extensions are supported",
        )
        .into());
    }
    let claims = decode_part(
        parts[1],
        jws,
        "failed to parse JWT Payload body as base64url encoded JSON",
        "JWT Payload must be a top level object",
    )?;
    check_claims(&claims, clock_skew, clock_tolerance)?;
    Ok(DecodedJwt {
        header,
        claims,
        jwt: String::from(jws),
    })
}
