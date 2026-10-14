// SPDX-License-Identifier: MIT
/*
 * aosabi-cli: the templates of the files new writes.
 */

//! The templates.
//!
//! Every file of a scaffold is one of these texts, with the name of the
//! service and the specification of the facade filled in. A scaffold carries
//! no `///` and no repository header.
//!
//! It holds the one import of the prelude, the root `Query` with an inherent
//! block whose method `hello` answers `Hello, world!`, and the entry `app`,
//! which the builder finds by its return type and which answers
//! `App::new().query(Query)`. An empty `Mutation` would register nothing,
//! since an empty root prints no type and leaves no trace in the schema, so
//! the index names none. The scaffold needs no build script, no `aosabi.config.rs`, no
//! `src/bindings.rs`, no `[lib]` path and no allowance of the naming lints,
//! since the builder reads the plain source and names every member by the
//! rule of GraphQL.
//!
//! The configuration stands in the chain of the entry, which
//! `aosabi::serve_app` serves. The chain of a scaffold with no feature
//! registers no plugin, because the card resolves the carrier and the boot
//! appends the serving plugin when the chain names none. Under the feature
//! `auth` the chain registers the identity over `zitadel_auth` and the login
//! plugin over a placeholder issuer under example.com, and the manifest names the facade with
//! its feature [`AUTH_FEATURE`], which carries both. Under the feature
//! `pages` the chain registers the file server of `public/` as a middleware
//! on the cards that serve sockets.

use crate::generate::toml_string;

/// The version of a scaffold.
pub const PACKAGE_VERSION: &str = "0.0.1";

/// The compatibility date of the `wrangler.toml` of the `workers` runtime.
pub const COMPATIBILITY_DATE: &str = "2026-09-23";

/// The index of a service, its crate root `src/lib.rs`.
pub const LIB_RS: &str = "use aosabi::prelude::*;

pub struct Query;

impl Query {
    pub fn hello(&self) -> String {
        String::from(\"Hello, world!\")
    }
}

pub fn app() -> impl IntoApp {
    App::new().query(Query)
}
";

/// The index of a service of the feature `auth`, whose chain registers the
/// plugins of the configuration with `auth` of the reference, with one
/// difference, the missing `use_node_server()`.
pub const LIB_RS_AUTH: &str =
    "use aosabi::auth::zitadel::{ZitadelAuthOptions, ZitadelLoginOptions, zitadel_auth};
use aosabi::prelude::*;

pub struct Query;

impl Query {
    pub fn hello(&self) -> String {
        String::from(\"Hello, world!\")
    }
}

pub fn app() -> impl IntoApp {
    App::new()
        .query(Query)
        .plugin(use_identity(zitadel_auth(ZitadelAuthOptions {
            issuer: String::from(\"https://issuer.example.com\"),
            ..ZitadelAuthOptions::default()
        })))
        .plugin(zitadel_login(ZitadelLoginOptions {
            issuer: String::from(\"https://issuer.example.com\"),
            ..ZitadelLoginOptions::default()
        }))
}
";

/// The index of a service of the feature `pages`, which serves `public/` as
/// files on the cards that serve sockets.
pub const LIB_RS_PAGES: &str = "use aosabi::prelude::*;

pub struct Query;

impl Query {
    pub fn hello(&self) -> String {
        String::from(\"Hello, world!\")
    }
}

pub fn app() -> impl IntoApp {
    let app = App::new().query(Query);
    // The file server reads the files of the process, which only the cards
    // that serve sockets have, so the build for the Worker leaves it out.
    #[cfg(unix)]
    let app = app.middleware(
        \"/*\",
        aosabi::serve_static(aosabi::ServeStaticOptions {
            root: String::from(\"./public\"),
            ..aosabi::ServeStaticOptions::default()
        }),
    );
    app
}
";

/// The index of a service of the features `auth` and `pages`, the chain of
/// [`LIB_RS_AUTH`] with the file server of [`LIB_RS_PAGES`].
pub const LIB_RS_AUTH_PAGES: &str =
    "use aosabi::auth::zitadel::{ZitadelAuthOptions, ZitadelLoginOptions, zitadel_auth};
use aosabi::prelude::*;

pub struct Query;

impl Query {
    pub fn hello(&self) -> String {
        String::from(\"Hello, world!\")
    }
}

pub fn app() -> impl IntoApp {
    let app = App::new()
        .query(Query)
        .plugin(use_identity(zitadel_auth(ZitadelAuthOptions {
            issuer: String::from(\"https://issuer.example.com\"),
            ..ZitadelAuthOptions::default()
        })))
        .plugin(zitadel_login(ZitadelLoginOptions {
            issuer: String::from(\"https://issuer.example.com\"),
            ..ZitadelLoginOptions::default()
        }));
    // The file server reads the files of the process, which only the cards
    // that serve sockets have, so the build for the Worker leaves it out.
    #[cfg(unix)]
    let app = app.middleware(
        \"/*\",
        aosabi::serve_static(aosabi::ServeStaticOptions {
            root: String::from(\"./public\"),
            ..aosabi::ServeStaticOptions::default()
        }),
    );
    app
}
";

/// Returns the index of a service with the feature `auth` when `auth` holds
/// and with the feature `pages` when `pages` holds.
#[must_use]
pub const fn lib_rs(auth: bool, pages: bool) -> &'static str {
    match (auth, pages) {
        (false, false) => LIB_RS,
        (true, false) => LIB_RS_AUTH,
        (false, true) => LIB_RS_PAGES,
        (true, true) => LIB_RS_AUTH_PAGES,
    }
}

/// The feature of the facade that the scaffold of the feature `auth` names,
/// the subpath `auth/zitadel` of the reference.
pub const AUTH_FEATURE: &str = "auth-zitadel";

/// The note `new` prints after its file list for a scaffold of the feature
/// `auth`.
///
/// The issuer of [`LIB_RS_AUTH`] is a placeholder under example.com, and the
/// plugins read the key of the service account from the variable `AUTH_KEY`
/// when it is not empty and from `key.json` below the working directory
/// otherwise. The plugins read the key once for the life of the process, so a
/// key added or changed later is read after a restart alone.
pub const AUTH_NOTE: &str = "note: the issuer in src/lib.rs is a placeholder, so set it to the URL of your Zitadel instance
note: the key of the service account comes from the variable AUTH_KEY, or else from key.json in the working directory
note: .gitignore names /key.json, so the key stays out of git
note: the service reads the key once, so a key added or changed later takes a restart
";

/// The ignore file of the `native` runtime, which keeps the build directory,
/// the variables of `.env` and the key file `key.json` the auth plugins read
/// by default out of git.
pub const GITIGNORE: &str = "/target
.env
/key.json
";

/// The ignore file of the `workers` runtime, the lines of [`GITIGNORE`] with
/// the local variables and the state of `wrangler`.
pub const GITIGNORE_WORKERS: &str = "/target
.env
/key.json
.dev.vars
.wrangler/
";

/// Returns the `Cargo.toml` of the service `name`, whose facade is `aosabi`,
/// the value of its key, and whose table of the facade for a Unix target is
/// `unix`.
///
/// The manifest names no `[lib]` table, so the crate root is cargo's default
/// `src/lib.rs`, and it names no build dependency and no lint table. The
/// second table of the facade stays, since the lock of the project pins the
/// packages the engine takes from a registry only through it: a scaffold
/// without it is refused with `AO-C0301` for `rustix`, `libc` and six more
/// packages the generated workspace needs.
#[must_use]
pub fn cargo_toml(name: &str, aosabi: &str, unix: &str) -> String {
    let name = toml_string(name);
    format!(
        "[package]
name = {name}
version = \"{PACKAGE_VERSION}\"
edition = \"2024\"
publish = false

# A service is a cargo workspace of its own, so it builds wherever it is
# created, also below another workspace.
[workspace]

[dependencies]
aosabi = {aosabi}

# The cards that serve sockets link the engine of the facade with these
# features. The table names them for a Unix target alone, so a Worker build
# links no engine. It is also how Cargo.lock pins the packages the engine
# takes from a registry, rustix and libc among them, which the lock of every
# build is checked against, so a service without it does not build.
[target.'cfg(unix)'.dependencies]
aosabi = {unix}
"
    )
}

/// Returns the page of `public/index.html` of the feature `pages`, one page
/// naming the service `name`.
#[must_use]
pub fn public_index_html(name: &str) -> String {
    format!(
        "<!doctype html>
<html lang=\"en\">
  <head>
    <meta charset=\"utf-8\" />
    <title>{name}</title>
  </head>
  <body>
    <h1>{name}</h1>
  </body>
</html>
"
    )
}

/// Returns the `wrangler.toml` of the `workers` runtime of the service
/// `name`, with no compatibility flag, and the path of `main` reserved for
/// the Workers entry.
#[must_use]
pub fn wrangler_toml(name: &str) -> String {
    let name = toml_string(name);
    format!(
        "name = {name}
main = \"target/aosabi/worker/index.js\"
compatibility_date = \"{COMPATIBILITY_DATE}\"

[build]
command = \"cargo aosabi build --device worker\"
"
    )
}

/// Returns the publish workflow of the `native` runtime for the image
/// `image` of the command line of version `version`.
///
/// It is the reference's workflow with two steps changed: the command line
/// installs before the build, and `cargo aosabi build --device docker
/// --locked` with the engine `docker` builds the image in place of `docker
/// build`, after which the push step tags `localhost/<image>:0.0.1`. The
/// licence lines the reference ends the file with are its own and left out,
/// as a scaffold carries no repository header.
#[must_use]
pub fn publish_yml(image: &str, version: &str) -> String {
    format!(
        "name: publish

on: [push]
env:
  IMAGE_NAME: {image}

jobs:
  # Push image to GitHub Packages.
  # See also https://docs.docker.com/docker-hub/builds/
  publish-container:
    runs-on: ubuntu-latest
    permissions:
      packages: write
      contents: read

    steps:
      - uses: actions/checkout@v4

      - name: Install the Aosabi command line
        run: cargo install --locked aosabi-cli --version {version}

      - name: Build image
        run: cargo aosabi build --device docker --locked
        env:
          AOSABI_CONTAINER_ENGINE: docker

      - name: Log into registry
        run: echo \"${{{{ secrets.GITHUB_TOKEN }}}}\" | docker login ghcr.io -u ${{{{ github.actor }}}} --password-stdin

      - name: Push image
        run: |
          IMAGE_ID=ghcr.io/${{{{ github.repository_owner }}}}/$IMAGE_NAME

          # Change all uppercase to lowercase
          IMAGE_ID=$(echo $IMAGE_ID | tr '[A-Z]' '[a-z]')
          # Strip git ref prefix from version
          VERSION=$(echo \"${{{{ github.ref }}}}\" | sed -e 's,.*/\\(.*\\),\\1,')
          # Strip \"v\" prefix from tag name
          [[ \"${{{{ github.ref }}}}\" == \"refs/tags/\"* ]] && VERSION=$(echo $VERSION | sed -e 's/^v//')
          # Use Docker `latest` tag convention
          [ \"$VERSION\" == \"main\" ] && VERSION=latest
          echo IMAGE_ID=$IMAGE_ID
          echo VERSION=$VERSION
          docker tag localhost/$IMAGE_NAME:{PACKAGE_VERSION} $IMAGE_ID:$VERSION
          docker push $IMAGE_ID:$VERSION
"
    )
}
