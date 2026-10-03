// SPDX-License-Identifier: MIT
/*
 * aosabi-fetch: the root store of the client.
 *
 * No analogue on the reference side, where Node reads its bundled roots and
 * the file NODE_EXTRA_CA_CERTS names.
 */

//! The root store from the system bundle and the extra CA file.
//!
//! No root store is compiled in. The roots are the system bundle, the file
//! `SSL_CERT_FILE` names or else the first of [`BUNDLES`] that exists, which
//! the base image of a container card carries, and the certificates of the
//! extra CA file `AOSABI_EXTRA_CA_CERTS` names, with `NODE_EXTRA_CA_CERTS` as
//! its Node name. The two names follow the rule of the alias table of
//! `docs/cli-and-targets.md` section 6.2, the Aosabi name winning and a note
//! when both are set and disagree, and are read here and not through the
//! table of `aosabi-core`, so a served app that links no client keeps the
//! table it had. A file is read as PEM through the reader of
//! `rustls-pki-types`, and a certificate rustls cannot parse is skipped, as a
//! bundle of a distribution may hold one.
//!
//! An extra CA file that cannot be read or holds no certificate is ignored
//! with the warning Node writes for it,
//! `Warning: Ignoring extra certs from ...`, and the call goes on with the
//! system bundle. A store with no certificate at all ends every call to an
//! `https:` URL with a TLS error that names the files it looked for.

use std::{
    env,
    ffi::OsString,
    fs,
    path::{
        Path,
        PathBuf, //
    },
};

use rustls::RootCertStore;
use rustls_pki_types::{
    CertificateDer,
    pem::PemObject, //
};

/// The variable that names a bundle in place of the system's.
pub const BUNDLE_ENV: &str = "SSL_CERT_FILE";

/// The variable that names the extra CA file.
pub const EXTRA_ENV: &str = "AOSABI_EXTRA_CA_CERTS";

/// The Node name of [`EXTRA_ENV`], the variable Node reads.
pub const EXTRA_ENV_NODE: &str = "NODE_EXTRA_CA_CERTS";

/// The system bundles in the order they are tried: Debian and its
/// derivatives with the distroless images, Fedora and its derivatives, and
/// Alpine.
pub const BUNDLES: [&str; 3] = [
    "/etc/ssl/certs/ca-certificates.crt",
    "/etc/pki/tls/certs/ca-bundle.crt",
    "/etc/ssl/cert.pem",
];

/// The files the root store of a process is read from, as the environment
/// names them at the start of the driver.
#[derive(Clone, Debug, Default)]
pub struct RootSources {
    /// The bundle `SSL_CERT_FILE` names, `None` for the first of [`BUNDLES`].
    bundle: Option<PathBuf>,
    /// The extra CA file, `None` when no variable names one.
    extra: Option<PathBuf>,
}

impl RootSources {
    /// Returns the files the environment names.
    #[must_use]
    pub fn from_env() -> Self {
        let bundle = env::var_os(BUNDLE_ENV)
            .filter(|value: &OsString| !value.is_empty())
            .map(PathBuf::from);
        Self {
            bundle,
            extra: extra_file(),
        }
    }

    /// Reads the root store.
    ///
    /// # Errors
    ///
    /// Returns the reason when the store holds no certificate, naming the
    /// files it looked for.
    pub fn load(&self) -> Result<RootCertStore, String> {
        let mut store = RootCertStore::empty();
        let bundle = self.bundle.clone().or_else(|| {
            BUNDLES
                .iter()
                .map(PathBuf::from)
                .find(|path| path.is_file())
        });
        if let Some(path) = &bundle
            && let Ok(certs) = read_certs(path)
        {
            store.add_parsable_certificates(certs);
        }
        if let Some(path) = &self.extra {
            match read_certs(path) {
                Ok(certs) => {
                    store.add_parsable_certificates(certs);
                }
                Err(reason) => eprintln!(
                    "Warning: Ignoring extra certs from `{}`, load failed: {reason}",
                    path.display()
                ),
            }
        }
        if store.is_empty() {
            let looked: Vec<String> = match &self.bundle {
                Some(path) => vec![path.display().to_string()],
                None => BUNDLES.iter().map(|path| String::from(*path)).collect(),
            };
            return Err(format!(
                "no root certificate: none was read from {}",
                looked.join(", ")
            ));
        }
        Ok(store)
    }
}

/// Returns the extra CA file the environment names.
///
/// The Aosabi name wins when it is set, and when both names are set and
/// disagree a note goes to standard error, as the alias table asks.
fn extra_file() -> Option<PathBuf> {
    let set = |name| env::var_os(name).filter(|value: &OsString| !value.is_empty());
    match (set(EXTRA_ENV), set(EXTRA_ENV_NODE)) {
        (Some(aosabi), Some(node)) => {
            if aosabi != node {
                eprintln!(
                    "note: {EXTRA_ENV_NODE} and {EXTRA_ENV} are both set and disagree, and {EXTRA_ENV}={} wins",
                    aosabi.to_string_lossy()
                );
            }
            Some(PathBuf::from(aosabi))
        }
        (aosabi, node) => aosabi.or(node).map(PathBuf::from),
    }
}

/// Returns the certificates of the PEM file `path`.
fn read_certs(path: &Path) -> Result<Vec<CertificateDer<'static>>, String> {
    let pem = fs::read(path).map_err(|error| error.to_string())?;
    let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_slice_iter(&pem)
        .filter_map(Result::ok)
        .collect();
    if certs.is_empty() {
        return Err(String::from("no certificate"));
    }
    Ok(certs)
}
