// SPDX-License-Identifier: MIT
/*
 * aosabi-auth-zitadel crypto: the signer and the digest.
 *
 * No analogue on the reference side, where the WebCrypto of Node signs with
 * RSASSA-PKCS1-v1_5 and SHA-256 and digests with SHA-256. The feature
 * host-crypto takes both from the host the crate runs in, the crypto of Bun or
 * of Cloudflare, through the functions the host installs, on any target.
 */

//! `sign_rs256`, `sha256`, `import_private_key`, the hidden `verify_rs256`,
//! and under the feature `host-crypto` the trait `Host` and `install_host`.
//!
//! The backend follows the build and never the target alone. The cards of pure
//! Rust take the default feature `ring`, so they sign and digest over `ring` on
//! a target of `cfg(unix)` as before. The module `core.wasm` and the native
//! library of the TypeScript option take the feature `host-crypto` with no
//! default, so they link no `ring` and no `rustls-pki-types` and hand the
//! signature, the digest, the import and the read of a key file and the clock
//! of a client assertion to the host, whose functions `install_host` keeps.
//! Where both features are on, the host wins.
//!
//! The key of the reference passes `crypto.createPrivateKey`, which reads a PEM
//! private key in the PKCS#1, PKCS#8 or SEC1 form, and
//! `crypto.subtle.importKey` for `RSASSA-PKCS1-v1_5` with SHA-256, which takes
//! an RSA key alone. Here the PEM is read by `rustls-pki-types` and the DER of
//! the key by this file: a PKCS#1 key is the `RSAPrivateKey` of RFC 8017
//! appendix A.1.2, and a PKCS#8 key carries one under the algorithm
//! `rsaEncryption`. A text that holds no private key, and an RSA key whose DER
//! does not read, answers the error the decoder of OpenSSL 3 raises in
//! `createPrivateKey`, and a key that is no RSA key answers the `DataError` of
//! the import.
//!
//! The import keeps every RSA private key whose DER it reads and reads the bits
//! of its modulus from that DER, as `crypto.subtle.importKey` keeps the
//! `modulusLength` of the key. `oauth4webapi` refuses a key under 2048 bits
//! only when it signs, with the `UnsupportedOperationError` of
//! `checkRsaKeyAlgorithm`, so such a key is kept as [`PrivateKey::Small`] and
//! [`sign_rs256`] refuses it with that error, which moves no failure.
//!
//! `ring` 0.17.14 signs with a key of two primes whose length is a multiple of
//! 512 bits, with a modulus of 2048 to 4096 bits and a public exponent from
//! 65537 to 2^33 - 1, so the keys of 2048, 3072 and 4096 bits, and refuses
//! every other key. Node 24.13.0 signs with the other keys of 2048 bits or more
//! as well: two primes of 2560, 3584, 4608 and 8192 bits and three primes of
//! 3072 bits, and two primes of 2048 bits with the public exponent 3. Over
//! `ring` such a key is kept as `PrivateKey::Unsigned`, and `sign_rs256`
//! refuses it as it refuses a small key, the entry of `NOT_PORTED` in
//! `aosabi-compat` on the RSA keys `ring` does not sign.
//!
//! The signature of PKCS#1 v1.5 is deterministic. `ring` 0.17.14 does not blind
//! the private operation: `sign` hands its random source to the padding alone,
//! whose PKCS#1 v1.5 encoder does not read it, and the exponentiation runs by
//! the Chinese remainder theorem with no blinding factor.

#[cfg(any(feature = "host-crypto", all(unix, feature = "ring")))]
use std::rc::Rc;

use aosabi_core::Thrown;

use crate::oidc::errors::OauthError;
#[cfg(any(feature = "host-crypto", all(unix, feature = "ring")))]
use crate::oidc::errors::UnsupportedOperationError;

/// The message every call answers on a target with no signer.
#[cfg(not(all(unix, feature = "ring", not(feature = "host-crypto"))))]
const NO_CRYPTO: &str = "crypto is not available on this card until M12";

/// The `CryptoKey` of a private RSA key for `RSASSA-PKCS1-v1_5` with SHA-256.
#[cfg(all(unix, feature = "ring", not(feature = "host-crypto")))]
#[derive(Clone, Debug)]
pub(crate) enum PrivateKey {
    /// A key `ring` signs with, of two primes and 2048, 3072 or 4096 bits.
    Rsa(Rc<ring::rsa::KeyPair>),
    /// A key whose modulus is under the 2048 bits `oauth4webapi` signs with.
    Small,
    /// A key of 2048 bits or more that `ring` 0.17.14 does not sign and Node
    /// does, the entry of `NOT_PORTED` in `aosabi-compat`.
    Unsigned,
}

/// The bits of the modulus under which `oauth4webapi` refuses an RSA key, `checkRsaKeyAlgorithm`.
#[cfg(any(feature = "host-crypto", all(unix, feature = "ring")))]
const MODULUS_FLOOR_BITS: usize = 2048;

/// The tag of a DER `INTEGER`.
#[cfg(any(feature = "host-crypto", all(unix, feature = "ring")))]
const INTEGER: u8 = 0x02;

/// The tag of a DER `OCTET STRING`.
#[cfg(any(feature = "host-crypto", all(unix, feature = "ring")))]
const OCTET_STRING: u8 = 0x04;

/// The tag of a DER `SEQUENCE`.
#[cfg(any(feature = "host-crypto", all(unix, feature = "ring")))]
const SEQUENCE: u8 = 0x30;

/// The DER of the object identifier `rsaEncryption`, 1.2.840.113549.1.1.1 of RFC 8017 appendix A.1.
#[cfg(any(feature = "host-crypto", all(unix, feature = "ring")))]
const RSA_ENCRYPTION: &[u8] = &[
    0x06, 0x09, 0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x01,
];

/// The `CryptoKey` of a private RSA key the host signs with, for `RSASSA-PKCS1-v1_5` with SHA-256.
#[cfg(feature = "host-crypto")]
#[derive(Clone, Debug)]
pub(crate) enum PrivateKey {
    /// The DER of the `RSAPrivateKey` of RFC 8017 appendix A.1.2 of a key of 2048 bits or more,
    /// which the host signs with.
    Rsa(Rc<[u8]>),
    /// A key whose modulus is under the 2048 bits `oauth4webapi` signs with.
    Small,
}

/// The `CryptoKey` of a private key, which no call on this target makes.
#[cfg(not(any(feature = "host-crypto", all(unix, feature = "ring"))))]
#[derive(Clone, Debug)]
pub(crate) enum PrivateKey {}

/// Returns the error a PEM text that holds no private key raises in `createPrivateKey`.
///
/// Its enumerable properties are `opensslErrorStack`, `library`, `reason` and
/// `code` in this order, as Node 24.13.0 sets them for a text with no key and
/// for a PKCS#1 public key.
#[cfg(any(feature = "host-crypto", all(unix, feature = "ring")))]
fn decoder_error() -> Thrown {
    use aosabi_core::execute::value::{
        ErrorName,
        ErrorObject,
        Value, //
    };
    use aosabi_json::{
        JsObject,
        JsValue, //
    };

    let mut props = JsObject::new();
    props.insert(
        "opensslErrorStack",
        vec![JsValue::from(
            "error:1E08010C:DECODER routines::unsupported",
        )],
    );
    props.insert("library", "DECODER routines");
    props.insert("reason", "unsupported");
    props.insert("code", "ERR_OSSL_UNSUPPORTED");
    Thrown(Value::from(ErrorObject::custom(
        ErrorName::Error,
        "error:1E08010C:DECODER routines::unsupported",
        props,
        None,
    )))
}

/// Returns the `DataError` of an import of a key that is no RSA key.
#[cfg(any(feature = "host-crypto", all(unix, feature = "ring")))]
fn data_error() -> Thrown {
    use aosabi_core::execute::value::{
        ErrorName,
        ErrorObject,
        Value, //
    };
    use aosabi_json::{
        JsObject,
        JsString, //
    };

    Thrown(Value::from(ErrorObject::custom(
        ErrorName::Other(JsString::from("DataError")),
        "Invalid key type",
        JsObject::new(),
        None,
    )))
}

/// Imports the PEM private key `pem` for `RSASSA-PKCS1-v1_5` with SHA-256.
///
/// The PEM holds a PKCS#1 or a PKCS#8 key. An RSA key whose DER reads is
/// kept whatever `ring` makes of it: a modulus under 2048 bits as
/// [`PrivateKey::Small`], a key `ring` signs with as [`PrivateKey::Rsa`], and
/// every other key as [`PrivateKey::Unsigned`], so [`sign_rs256`] answers
/// the refusal of each.
///
/// # Errors
///
/// Returns the error of the decoder for a text that holds no private key and
/// for an RSA key whose DER does not read, and the `DataError` of the import
/// for a key that is no RSA key.
#[cfg(all(unix, feature = "ring", not(feature = "host-crypto")))]
pub(crate) fn import_private_key(pem: &str) -> Result<PrivateKey, Thrown> {
    use rustls_pki_types::{
        PrivateKeyDer,
        pem::PemObject, //
    };

    let der = PrivateKeyDer::from_pem_slice(pem.as_bytes()).map_err(|_| decoder_error())?;
    let rsa_key = match &der {
        PrivateKeyDer::Pkcs1(key) => key.secret_pkcs1_der(),
        PrivateKeyDer::Pkcs8(key) => match rsa_key_of_pkcs8(key.secret_pkcs8_der()) {
            Pkcs8Key::Rsa(rsa_key) => rsa_key,
            Pkcs8Key::Other => return Err(data_error()),
            Pkcs8Key::Unread => return Err(decoder_error()),
        },
        _ => return Err(data_error()),
    };
    let bits = modulus_bits(rsa_key).ok_or_else(decoder_error)?;
    if bits < MODULUS_FLOOR_BITS {
        return Ok(PrivateKey::Small);
    }
    Ok(ring::rsa::KeyPair::from_der(rsa_key)
        .map_or(PrivateKey::Unsigned, |pair| PrivateKey::Rsa(Rc::new(pair))))
}

/// What the `PrivateKeyInfo` of a PKCS#8 key carries, as [`rsa_key_of_pkcs8`] reads it.
#[cfg(any(feature = "host-crypto", all(unix, feature = "ring")))]
enum Pkcs8Key<'a> {
    /// The DER of the `RSAPrivateKey` of a key of the algorithm `rsaEncryption`.
    Rsa(&'a [u8]),
    /// A key of another algorithm.
    Other,
    /// Bytes that are no `PrivateKeyInfo`.
    Unread,
}

/// Returns the DER element of `tag` at the start of `input`, as its contents and the bytes after it.
///
/// The length is the definite form of X.690 section 8.1.3 in up to four
/// bytes, which every key of this file fits, and anything else reads as no
/// element.
#[cfg(any(feature = "host-crypto", all(unix, feature = "ring")))]
fn der_element(input: &[u8], tag: u8) -> Option<(&[u8], &[u8])> {
    let (&found, rest) = input.split_first()?;
    if found != tag {
        return None;
    }
    let (&first, rest) = rest.split_first()?;
    let (length, rest) = if first < 0x80 {
        (usize::from(first), rest)
    } else {
        let (digits, rest) = rest.split_at_checked(usize::from(first & 0x7F))?;
        if digits.is_empty() || digits.len() > 4 {
            return None;
        }
        let length = digits
            .iter()
            .fold(0_usize, |length, digit| (length << 8) | usize::from(*digit));
        (length, rest)
    };
    rest.split_at_checked(length)
}

/// Returns what the DER `PrivateKeyInfo` `der` of RFC 5958 section 2 carries.
///
/// The key of the algorithm `rsaEncryption`, whose parameters are the `NULL`
/// of RFC 8017 appendix A.1 or absent, is the contents of its `privateKey`.
#[cfg(any(feature = "host-crypto", all(unix, feature = "ring")))]
fn rsa_key_of_pkcs8(der: &[u8]) -> Pkcs8Key<'_> {
    let Some((info, [])) = der_element(der, SEQUENCE) else {
        return Pkcs8Key::Unread;
    };
    let Some(([0 | 1], info)) = der_element(info, INTEGER) else {
        return Pkcs8Key::Unread;
    };
    let Some((algorithm, info)) = der_element(info, SEQUENCE) else {
        return Pkcs8Key::Unread;
    };
    let Some((key, _)) = der_element(info, OCTET_STRING) else {
        return Pkcs8Key::Unread;
    };
    match algorithm.strip_prefix(RSA_ENCRYPTION) {
        Some([] | [0x05, 0x00]) => Pkcs8Key::Rsa(key),
        _ => Pkcs8Key::Other,
    }
}

/// Returns the bits of the modulus of the DER `RSAPrivateKey` `der`, or none when it does not read.
///
/// The key of RFC 8017 appendix A.1.2 is the version, 0 for two primes and
/// 1 for more, the eight integers from the modulus to the coefficient, and
/// for the version 1 the sequence of the other primes. The bits are those of
/// the modulus without the zeros that lead it.
#[cfg(any(feature = "host-crypto", all(unix, feature = "ring")))]
fn modulus_bits(der: &[u8]) -> Option<usize> {
    let (key, []) = der_element(der, SEQUENCE)? else {
        return None;
    };
    let (version, mut rest) = der_element(key, INTEGER)?;
    let (modulus, _) = der_element(rest, INTEGER)?;
    for _ in 0..8 {
        rest = der_element(rest, INTEGER)?.1;
    }
    match (version, rest) {
        ([0], []) => {}
        ([1], others) => {
            let (_, []) = der_element(others, SEQUENCE)? else {
                return None;
            };
        }
        _ => return None,
    }
    let start = modulus.iter().position(|byte| *byte != 0)?;
    let (top, rest) = modulus.get(start..)?.split_first()?;
    let top_bit = (0..8_usize).rev().find(|bit| (top >> bit) & 1 == 1)?;
    Some(rest.len() * 8 + top_bit + 1)
}

/// Answers the `TypeError` of rule 9, since this target has no signer.
///
/// # Errors
#[cfg(not(any(feature = "host-crypto", all(unix, feature = "ring"))))]
pub(crate) fn import_private_key(_pem: &str) -> Result<PrivateKey, Thrown> {
    Err(Thrown::type_error(NO_CRYPTO))
}

/// Returns the `TypeError` of rule 9 that every call of the signer answers on this target.
///
/// The bootstrap imports the key of the signer, so on this target it never
/// settles with a configuration and no path after it runs. The bootstrap
/// returns this error where its discovery would start, and the login routes
/// and the provider right after the bootstrap, each on a build with no signer,
/// so the module keeps none of the code of those paths.
#[cfg(not(any(feature = "host-crypto", all(unix, feature = "ring"))))]
pub(crate) fn no_signer() -> Thrown {
    Thrown::type_error(NO_CRYPTO)
}

/// Signs `data` with `key` by RSASSA-PKCS1-v1_5 with SHA-256, `crypto.subtle.sign`.
///
/// # Errors
///
/// Returns the `UnsupportedOperationError` of a key under 2048 bits, which
/// `oauth4webapi` answers as well, and of a key `ring` does not sign, the
/// entry of `NOT_PORTED`, and the `OperationError` of `WebCrypto` when
/// `ring` fails.
#[cfg(all(unix, feature = "ring", not(feature = "host-crypto")))]
pub(crate) fn sign_rs256(key: &PrivateKey, data: &[u8]) -> Result<Vec<u8>, OauthError> {
    let pair = match key {
        PrivateKey::Rsa(pair) => pair,
        PrivateKey::Small | PrivateKey::Unsigned => {
            return Err(UnsupportedOperationError::new(
                "unsupported RSASSA-PKCS1-v1_5 modulusLength",
            )
            .into());
        }
    };
    let rng = ring::rand::SystemRandom::new();
    let mut signature = vec![0_u8; pair.public().modulus_len()];
    pair.sign(
        &ring::signature::RSA_PKCS1_SHA256,
        &rng,
        data,
        &mut signature,
    )
    .map_err(|_| OauthError::OperationFailed)?;
    Ok(signature)
}

/// Signs nothing, since this target has no signer and so no key.
///
/// The import answers the `TypeError` of rule 9 before any key exists, so no
/// call reaches this function.
#[cfg(not(any(feature = "host-crypto", all(unix, feature = "ring"))))]
pub(crate) fn sign_rs256(key: &PrivateKey, _data: &[u8]) -> Result<Vec<u8>, OauthError> {
    match *key {}
}

/// Verifies the RSASSA-PKCS1-v1_5 signature with SHA-256 `signature` of `data` with the public half
/// of the PEM private key `pem`.
///
/// It is hidden, for the devtool, whose primitive `zitadel` verifies the
/// client assertion it signed before it masks it, as
/// `tests/sidecar/zitadel.mjs` verifies the assertion of the reference.
///
/// # Errors
///
/// Returns the failure of [`import_private_key`], and the
/// `UnsupportedOperationError` of a key [`sign_rs256`] refuses as a
/// `TypeError`.
#[cfg(all(unix, feature = "ring", not(feature = "host-crypto")))]
#[doc(hidden)]
pub fn verify_rs256(pem: &str, data: &[u8], signature: &[u8]) -> Result<bool, Thrown> {
    let PrivateKey::Rsa(pair) = import_private_key(pem)? else {
        return Err(Thrown::type_error(
            "unsupported RSASSA-PKCS1-v1_5 modulusLength",
        ));
    };
    let public = ring::signature::UnparsedPublicKey::new(
        &ring::signature::RSA_PKCS1_2048_8192_SHA256,
        pair.public().as_ref(),
    );
    Ok(public.verify(data, signature).is_ok())
}

/// Answers the `TypeError` of rule 9, since this target has no verifier.
///
/// # Errors
#[cfg(not(all(unix, feature = "ring", not(feature = "host-crypto"))))]
#[doc(hidden)]
pub fn verify_rs256(_pem: &str, _data: &[u8], _signature: &[u8]) -> Result<bool, Thrown> {
    Err(Thrown::type_error(NO_CRYPTO))
}

/// Returns the SHA-256 digest of `data`, `crypto.subtle.digest('SHA-256', data)`.
///
/// # Errors
///
/// Never over `ring`. On a build with no signer always the `TypeError` of
/// rule 9.
#[cfg(all(unix, feature = "ring", not(feature = "host-crypto")))]
#[expect(
    clippy::unnecessary_wraps,
    reason = "the target with no signer answers an error through the same signature"
)]
pub(crate) fn sha256(data: &[u8]) -> Result<[u8; 32], Thrown> {
    let digest = ring::digest::digest(&ring::digest::SHA256, data);
    let mut out = [0_u8; 32];
    out.copy_from_slice(digest.as_ref());
    Ok(out)
}

/// Answers the `TypeError` of rule 9, since this target has no digest.
///
/// # Errors
#[cfg(not(any(feature = "host-crypto", all(unix, feature = "ring"))))]
pub(crate) fn sha256(_data: &[u8]) -> Result<[u8; 32], Thrown> {
    Err(Thrown::type_error(NO_CRYPTO))
}

/// The functions of the host the feature `host-crypto` signs, digests and
/// reads a key file with.
///
/// The module `core.wasm` reaches them through its import block and the native
/// library through the function pointers its boot takes, so on Bun they are the
/// `node:crypto` and the `node:fs` of Bun and on a Worker the `node:crypto` of
/// Cloudflare.
#[cfg(feature = "host-crypto")]
pub trait Host {
    /// Answers whether the host imports the private key whose `RSAPrivateKey`
    /// DER is `key`, as `crypto.createPrivateKey` and the import of the
    /// reference take a key once, when the bootstrap reads it.
    fn import_rs256(&self, key: &[u8]) -> bool;

    /// Signs `data` with the private key whose `RSAPrivateKey` DER is `key`,
    /// RSASSA-PKCS1-v1_5 with SHA-256, or answers `None` where the host does
    /// not sign with that key.
    fn sign_rs256(&self, key: &[u8], data: &[u8]) -> Option<Vec<u8>>;

    /// Returns the SHA-256 digest of `data`, or `None` where the host has no
    /// digest.
    fn sha256(&self, data: &[u8]) -> Option<[u8; 32]>;

    /// Returns the bytes of the file at `path` below the working directory of
    /// the host, or `None` where it cannot be read.
    fn read_file(&self, path: &str) -> Option<Vec<u8>>;

    /// Returns the milliseconds since the epoch of the clock of the host,
    /// `Date.now()`, or `None` where the host reads none.
    fn now_ms(&self) -> Option<f64>;
}

#[cfg(feature = "host-crypto")]
thread_local! {
    /// The functions of the host of the runtime thread.
    static HOST: core::cell::OnceCell<Rc<dyn Host>> = const { core::cell::OnceCell::new() };
}

/// Installs `host` as the functions of the host of this thread, once, before
/// the first request. The first call wins and a later call is ignored.
#[cfg(feature = "host-crypto")]
pub fn install_host(host: Rc<dyn Host>) {
    let _ = HOST.try_with(|slot| {
        slot.get_or_init(move || host);
    });
}

/// Returns the functions of the host of this thread, `None` before
/// [`install_host`].
#[cfg(feature = "host-crypto")]
fn host() -> Option<Rc<dyn Host>> {
    HOST.try_with(|slot| slot.get().cloned()).ok().flatten()
}

/// Returns the bytes of the file at `path` below the working directory of
/// the host, through its functions, or `None` where it cannot be read.
#[cfg(feature = "host-crypto")]
pub(crate) fn host_read_file(path: &str) -> Option<Vec<u8>> {
    host()?.read_file(path)
}

/// Returns the milliseconds since the epoch of the clock of the host, through
/// its functions, or `None` where it reads none.
#[cfg(feature = "host-crypto")]
pub(crate) fn host_now_ms() -> Option<f64> {
    host()?.now_ms()
}

/// The labels of the private keys a PEM text names, as the PEM reader of the
/// feature `ring` takes them: PKCS#1, PKCS#8 and SEC1.
#[cfg(feature = "host-crypto")]
#[derive(Clone, Copy, PartialEq, Eq)]
enum PemKind {
    /// `RSA PRIVATE KEY`, PKCS#1.
    Pkcs1,
    /// `PRIVATE KEY`, PKCS#8.
    Pkcs8,
    /// `EC PRIVATE KEY`, SEC1.
    Sec1,
}

/// Returns the kind and the DER of the first private key of the PEM text
/// `pem`, or `None` where the text holds none or a section of it does not
/// decode.
///
/// A section is the lines between `-----BEGIN <label>-----` and
/// `-----END <label>-----`, and its body is base64 with white space between the
/// characters. A section of another label, a certificate or a public key among
/// them, is passed over, as `PrivateKeyDer::from_pem_slice` of
/// `rustls-pki-types` 1.15.1 passes it over, and a text the reader of that
/// version refuses beyond these rules reads here as no key.
#[cfg(feature = "host-crypto")]
fn pem_private_key(pem: &str) -> Option<(PemKind, Vec<u8>)> {
    let mut lines = pem.lines();
    while let Some(line) = lines.next() {
        let Some(label) = line
            .trim_end()
            .strip_prefix("-----BEGIN ")
            .and_then(|rest| rest.strip_suffix("-----"))
        else {
            continue;
        };
        let end = format!("-----END {label}-----");
        let mut body = String::new();
        let mut closed = false;
        for line in lines.by_ref() {
            if line.trim_end() == end {
                closed = true;
                break;
            }
            body.push_str(line.trim());
        }
        if !closed {
            return None;
        }
        let kind = match label {
            "RSA PRIVATE KEY" => PemKind::Pkcs1,
            "PRIVATE KEY" => PemKind::Pkcs8,
            "EC PRIVATE KEY" => PemKind::Sec1,
            _ => continue,
        };
        let der = crate::oidc::jwt::base64url_decode(&body).ok()?;
        return Some((kind, der));
    }
    None
}

/// Imports the PEM private key `pem` for `RSASSA-PKCS1-v1_5` with SHA-256,
/// with the reads and the refusals of the import over `ring`.
///
/// The PEM holds a PKCS#1 or a PKCS#8 key. An RSA key whose DER reads is kept
/// as [`PrivateKey::Small`] where its modulus is under 2048 bits and as
/// [`PrivateKey::Rsa`] otherwise, which the host signs with, so a key of two
/// primes of 2560 bits, which `ring` 0.17.14 does not sign and Node does,
/// reaches the signer of the host. A key of 2048 bits or more the host does not
/// import, a key of three primes on Bun, whose `BoringSSL` takes no key of more
/// than two primes, fails here, before the discovery.
///
/// # Errors
///
/// Returns the error of the decoder for a text that holds no private key and
/// for an RSA key whose DER does not read, the `DataError` of the import for
/// a key that is no RSA key, and the `OperationError` of a key the host does
/// not import.
#[cfg(feature = "host-crypto")]
pub(crate) fn import_private_key(pem: &str) -> Result<PrivateKey, Thrown> {
    let (kind, der) = pem_private_key(pem).ok_or_else(decoder_error)?;
    let rsa_key = match kind {
        PemKind::Pkcs1 => der.as_slice(),
        PemKind::Pkcs8 => match rsa_key_of_pkcs8(&der) {
            Pkcs8Key::Rsa(rsa_key) => rsa_key,
            Pkcs8Key::Other => return Err(data_error()),
            Pkcs8Key::Unread => return Err(decoder_error()),
        },
        PemKind::Sec1 => return Err(data_error()),
    };
    let bits = modulus_bits(rsa_key).ok_or_else(decoder_error)?;
    if bits < MODULUS_FLOOR_BITS {
        return Ok(PrivateKey::Small);
    }
    if !host().is_some_and(|host| host.import_rs256(rsa_key)) {
        return Err(not_imported());
    }
    Ok(PrivateKey::Rsa(Rc::from(rsa_key)))
}

/// Returns the `OperationError` of a key the host does not import.
#[cfg(feature = "host-crypto")]
fn not_imported() -> Thrown {
    use aosabi_core::execute::value::{
        ErrorName,
        ErrorObject,
        Value, //
    };
    use aosabi_json::{
        JsObject,
        JsString, //
    };

    Thrown(Value::from(ErrorObject::custom(
        ErrorName::Other(JsString::from("OperationError")),
        "The host does not import the key",
        JsObject::new(),
        None,
    )))
}

/// Signs `data` with `key` by RSASSA-PKCS1-v1_5 with SHA-256 through the
/// signer of the host, `sign('sha256', data, key)` of its `node:crypto`.
///
/// # Errors
///
/// Returns the `UnsupportedOperationError` of a key under 2048 bits, which
/// `oauth4webapi` answers as well, and the `OperationError` of `WebCrypto`
/// where the host does not sign.
#[cfg(feature = "host-crypto")]
pub(crate) fn sign_rs256(key: &PrivateKey, data: &[u8]) -> Result<Vec<u8>, OauthError> {
    let der = match key {
        PrivateKey::Rsa(der) => der,
        PrivateKey::Small => {
            return Err(UnsupportedOperationError::new(
                "unsupported RSASSA-PKCS1-v1_5 modulusLength",
            )
            .into());
        }
    };
    host()
        .and_then(|host| host.sign_rs256(der, data))
        .ok_or(OauthError::OperationFailed)
}

/// Returns the SHA-256 digest of `data` through the digest of the host,
/// `createHash('sha256')` of its `node:crypto`.
///
/// # Errors
///
/// Returns the `TypeError` `the host gives no SHA-256 digest` where no host
/// is installed or its digest fails.
#[cfg(feature = "host-crypto")]
pub(crate) fn sha256(data: &[u8]) -> Result<[u8; 32], Thrown> {
    host()
        .and_then(|host| host.sha256(data))
        .ok_or_else(|| Thrown::type_error("the host gives no SHA-256 digest"))
}
