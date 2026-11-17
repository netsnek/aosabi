// SPDX-License-Identifier: MIT
/*
 * aosabi-cli: the refusal codes of the command line.
 *
 * No analogue in the reference.
 */

//! The refusal codes.
//!
//! Every refusal of the command line carries one code of [`CODES`], prompts
//! for nothing and exits 1, and every refusal but `AO-C0502` and `AO-C0601`
//! prints what would change the outcome as its hint: the lines of the manifest
//! for `AO-C0101` and for a manifest that names a crate of Aosabi from a
//! registry, `AO-C0108`, the change of the manifest and the command that drops
//! the pin for such a crate that the lock or the resolved graph holds, and the
//! command for every other. No command of this build renames the types of a
//! remote or brings a command of a later release, so those two print no hint.
//! The text of each refusal is built by one function of this module from one
//! string literal, so a command that refuses names the function and never
//! assembles a message at the site that fails (`Documentation/coding-style.md`
//! section 7).
//!
//! The codes `AO-C` are the command line's own. `AO-R0105` is the design code
//! for a host tool the card needs, which
//! the command line uses for a missing standard library of the card's target.
//!
//! A code that no refusal issues any more leaves [`CODES`] and stands in
//! [`RETIRED`] with its reason, and is never reused: `AO-C0403` refused
//! `new --features auth` while the facade carried no `zitadel_auth`, and it
//! carries it now under the feature `auth-zitadel`, so `new` writes that
//! scaffold.

use std::{
    fmt::Write as _,
    path::Path, //
};

use crate::{
    args::{
        NewArgs,
        Runtime, //
    },
    generate::LINUX_FEATURES,
    new::feature_name,
    project::{
        CheckedLock,
        canonical_name,
        framework_prefix, //
    },
    report::CliError, //
};

/// One refusal code and what it means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Code {
    /// The code as it is printed, `AO-C0101`.
    pub id: &'static str,
    /// What the code refuses, one sentence.
    pub meaning: &'static str,
}

/// The package is no Aosabi service.
pub const AO_C0101: &str = "AO-C0101";
/// A package of the derive path has no library target at `src/index.rs`.
pub const AO_C0102: &str = "AO-C0102";
/// The package is not one crate with one facade.
pub const AO_C0103: &str = "AO-C0103";
/// An entry that is not the crate root of the package.
pub const AO_C0104: &str = "AO-C0104";
/// A plain crate whose source the builder cannot read, or that states a
/// construct the builder cannot read.
pub const AO_C0105: &str = "AO-C0105";
/// A package of the derive path, whose entry the generated workspace names
/// as the function `default`, beside a facade that declares the feature
/// `mirror` and whose dependency names it not.
pub const AO_C0106: &str = "AO-C0106";
/// A package with `aosabi.config.rs` beside a facade that declares the
/// feature `mirror` and whose dependency names it not.
pub const AO_C0107: &str = "AO-C0107";
/// A crate of Aosabi from a registry, in a manifest, the lock or the resolved
/// graph.
pub const AO_C0108: &str = "AO-C0108";
/// A card file, an installed card or a verb that arrives with device cards.
pub const AO_C0201: &str = "AO-C0201";
/// A device that names no built in card.
pub const AO_C0202: &str = "AO-C0202";
/// A host tool the card needs is absent.
pub const AO_R0105: &str = "AO-R0105";
/// A package the lock of the project does not pin, other than the build side
/// of a plain crate from the source and at the version of the facade it pins.
pub const AO_C0301: &str = "AO-C0301";
/// Cargo.lock of the project, or of the workspace of a path facade a card
/// builds the crates of its kernel in, is missing under `--locked` or
/// `--frozen`.
pub const AO_C0302: &str = "AO-C0302";
/// The target of `new` exists and is not empty.
pub const AO_C0401: &str = "AO-C0401";
/// The target of `new` gives no valid package name.
pub const AO_C0402: &str = "AO-C0402";
/// A feature the runtime of `new` lacks.
pub const AO_C0404: &str = "AO-C0404";
/// A `new` that names no checkout of Aosabi.
pub const AO_C0405: &str = "AO-C0405";
/// A URL of `pull` whose scheme the client does not speak.
pub const AO_C0501: &str = "AO-C0501";
/// A remote type name that collides with an item the registry writes.
pub const AO_C0502: &str = "AO-C0502";
/// A command that lands with a later release.
pub const AO_C0601: &str = "AO-C0601";
/// The container engine failed.
pub const AO_C0701: &str = "AO-C0701";

/// Every code of the command line, the codes of the project first,
/// `AO-C0105`, `AO-C0106`, `AO-C0107` and `AO-C0108` after them, and
/// `AO-C0405` after the codes of `new`.
pub const CODES: [Code; 21] = [
    Code {
        id: AO_C0101,
        meaning: "the package names no dependency aosabi, or a package of the derive path names no build dependency aosabi-build, so it is no Aosabi service",
    },
    Code {
        id: AO_C0102,
        meaning: "a package of the derive path has no library target whose source path ends in src/index.rs",
    },
    Code {
        id: AO_C0103,
        meaning: "the package names aosabi from two sources or at two requirements, or carries a [workspace] table that lists members, or is the member of a workspace above it that lists members beside it, because a service is one crate",
    },
    Code {
        id: AO_C0104,
        meaning: "an -e or -m value that is not the crate root of the package, cargo's default src/lib.rs or the path of its [lib] table",
    },
    Code {
        id: AO_C0105,
        meaning: "a package that names no build side whose source the builder cannot read, a file that does not parse as Rust among them, or a plain crate whose source states a construct the builder cannot read or an argument type no reader of the engine takes, whose glue cannot name a type or a member it reaches, or whose crate root holds no entry, each refused at its file and line",
    },
    Code {
        id: AO_C0106,
        meaning: "a package of the derive path, whose roots a fragment of graphql! holds and whose entry is the function default, whose facade, one that declares the feature mirror, it names without that feature, since that path is retired for application code",
    },
    Code {
        id: AO_C0107,
        meaning: "aosabi.config.rs in a package whose facade, one that declares the feature mirror, it names without that feature, since the file is retired for the configuration the chain of the entry carries, which serve_app serves",
    },
    Code {
        id: AO_C0108,
        meaning: "a crate of Aosabi, aosabi, a crate named aosabi-*, graphtp-* or aoviewer-*, graphql-over-http, graphql-over-graphtp, graphtp, homa or aoviewer, in any spelling of letter case and of - and _, comes from a registry, named by a package of the workspace, pinned by the lock of the project, by the lock a build derives from it or by the lock of the workspace of a path facade, or resolved into its graph, and Aosabi is released through its repository alone, so a project names them by path from a checkout or from a repository",
    },
    Code {
        id: AO_C0201,
        meaning: "a card file, an installed card id, a card in devices/, or a card or registry verb, which arrive with device cards",
    },
    Code {
        id: AO_C0202,
        meaning: "--device names no built in card",
    },
    Code {
        id: AO_R0105,
        meaning: "a host tool the card needs is absent, here the standard library of the card's target",
    },
    Code {
        id: AO_C0301,
        meaning: "the generated workspace needs a package with a source that Cargo.lock of the project does not pin, other than the build side of a plain crate from the source and at the version of the facade it pins",
    },
    Code {
        id: AO_C0302,
        meaning: "Cargo.lock of the project, or of the workspace of a path facade a card builds the crates of its kernel in, is missing under --locked or --frozen",
    },
    Code {
        id: AO_C0401,
        meaning: "the target of new exists and is not empty",
    },
    Code {
        id: AO_C0402,
        meaning: "the target of new gives no valid package name",
    },
    Code {
        id: AO_C0404,
        meaning: "a feature the runtime of new lacks",
    },
    Code {
        id: AO_C0405,
        meaning: "new without --aosabi-path, since Aosabi is released through its repository alone and the scaffold names the facade by path from a checkout",
    },
    Code {
        id: AO_C0501,
        meaning: "pull got a URL whose scheme is not http or https, or https under the http:// only client",
    },
    Code {
        id: AO_C0502,
        meaning: "a remote type name collides with an item the registry writes",
    },
    Code {
        id: AO_C0601,
        meaning: "a db sub command, docs or bridge, each of which lands with a later release",
    },
    Code {
        id: AO_C0701,
        meaning: "the container engine failed",
    },
];

/// The codes that no refusal issues any more, each with what it refused and
/// why it went.
///
/// They are recorded so that nothing reuses them and so that a reader who
/// meets one in another document finds out where it went.
pub const RETIRED: &[Code] = &[Code {
    id: "AO-C0403",
    meaning: "new --features auth before the release that brings zitadel_auth. Retired with that release, which ends the refusal, because the facade carries zitadel_auth under the feature auth-zitadel and new writes the scaffold",
}];

/// The names of the built in cards, as a refusal lists them.
///
/// They are the five cards of `BUILTIN` in `cards/builtin.rs`, `server`,
/// `server-legacy`, `docker`, `worker` and `esp32c3-devkitm1`, with
/// `server-legacy` as a second server card.
pub const BUILTIN_CARD_NAMES: &str = "server, server-legacy, docker, worker and esp32c3-devkitm1";

/// Returns the refusal of the package `package`, which is no Aosabi service,
/// `AO-C0101`.
///
/// A `plain` crate needs the facade alone, so its message and its hint name
/// the facade alone, and a crate of the derive path needs the facade and its
/// build side, so its message names both and its hint the manifest and the
/// build script of the derive path.
///
/// Both hints name the crates by the path of a checkout of Aosabi, with the
/// placeholder `<checkout>` for its directory, and no version requirement of
/// a registry, and both name the facade in the two tables that
/// `aosabi new --aosabi-path` writes, the second for a Unix target with the
/// features [`LINUX_FEATURES`] of the cards that serve on Linux, since the
/// lock of the project pins the packages the engine takes from a registry
/// only through it and a manifest without it is refused with `AO-C0301`.
#[must_use]
pub fn not_a_service(package: &str, plain: bool) -> CliError {
    let features = LINUX_FEATURES
        .iter()
        .map(|feature| format!("\"{feature}\""))
        .collect::<Vec<_>>()
        .join(", ");
    if plain {
        return CliError::coded(
            AO_C0101,
            format!(
                "the package `{package}` names no dependency `aosabi`, so it is no Aosabi service"
            ),
        )
        .with_hint(format!("add the facade to Cargo.toml from a checkout of Aosabi in the two tables aosabi new --aosabi-path <checkout> writes\n\n    [dependencies]\n    aosabi = {{ path = \"<checkout>/crates/aosabi\" }}\n\n    [target.'cfg(unix)'.dependencies]\n    aosabi = {{ path = \"<checkout>/crates/aosabi\", features = [{features}] }}"));
    }
    CliError::coded(
        AO_C0101,
        format!(
            "the package `{package}` names no dependency `aosabi` or no build dependency `aosabi-build`, so it is no Aosabi service"
        ),
    )
    .with_hint(format!(
        "add the facade in the two tables aosabi new --aosabi-path <checkout> writes and its build side to Cargo.toml from a checkout of Aosabi\n\n    [dependencies]\n    aosabi = {{ path = \"<checkout>/crates/aosabi\" }}\n\n    [target.'cfg(unix)'.dependencies]\n    aosabi = {{ path = \"<checkout>/crates/aosabi\", features = [{features}] }}\n\n    [build-dependencies]\n    aosabi-build = {{ path = \"<checkout>/crates/aosabi-build\" }}\n\nand the one line build.rs\n\n    fn main() -> std::process::ExitCode {{\n        aosabi_build::declarations_main()\n    }}",
    ))
}

/// Returns the refusal of the package `package`, which has no library target
/// at `src/index.rs`, `AO-C0102`.
#[must_use]
pub fn no_library_root(package: &str) -> CliError {
    CliError::coded(
        AO_C0102,
        format!(
            "the package `{package}` has no library target whose source path ends in src/index.rs"
        ),
    )
    .with_hint("add to Cargo.toml\n\n    [lib]\n    path = \"src/index.rs\"")
}

/// Returns the refusal of the package `package`, which names the facade
/// twice, `AO-C0103`.
#[must_use]
pub fn facade_twice(package: &str) -> CliError {
    CliError::coded(
        AO_C0103,
        format!(
            "the package `{package}` names `aosabi` from two sources or at two requirements, and a service is one crate with one facade"
        ),
    )
    .with_hint("name `aosabi` with one source and one requirement in every table of Cargo.toml")
}

/// Returns the refusal of the package `package`, whose `[workspace]` table
/// lists members, `AO-C0103`.
///
/// Cargo takes a member from the list `members` of the table and from every
/// path package inside the folder of the workspace whose folder the list
/// `exclude` names not, so the hint names both and keeps the exclude, which
/// the scaffold of `aosabi new` writes for a checkout of Aosabi inside it.
#[must_use]
pub fn workspace_with_members(package: &str) -> CliError {
    CliError::coded(
        AO_C0103,
        format!(
            "the package `{package}` carries a [workspace] table that lists members, and a service is one crate"
        ),
    )
    .with_hint(
        "list no members in the [workspace] table of the service and keep its exclude, and move a path package out of the folder of the service or add its folder to exclude, since cargo takes every path package inside the folder of a workspace as a member",
    )
}

/// Returns the refusal of the package `package`, the member of the
/// workspace whose root manifest `root_manifest` lies above it and lists
/// members beside it, `AO-C0103`.
///
/// The `[workspace]` table stands in that root manifest and not in the
/// manifest of the service, so the hint keeps the service the only member of
/// that workspace, or takes it out of the members of that workspace, adds its
/// folder to the exclude where a glob of the members or a path dependency of
/// a member brings it in, and gives it an empty `[workspace]` table of its
/// own.
#[must_use]
pub fn workspace_above_with_members(package: &str, root_manifest: &Path) -> CliError {
    CliError::coded(
        AO_C0103,
        format!(
            "the package `{package}` is a member of the workspace of {}, which lists members beside it, and a service is one crate",
            root_manifest.display()
        ),
    )
    .with_hint(format!(
        "keep `{package}` the only member of the workspace of {}, or take `{package}` out of its members, add its folder to the exclude where a glob of the members or a path dependency of a member brings it in, and give `{package}` an empty [workspace] table of its own",
        root_manifest.display()
    ))
}

/// Returns the refusal of the entry `value`, which is not the crate root
/// `lib_path` of the package, `AO-C0104`.
///
/// The crate root is the source path of the library target as cargo resolves
/// it, the path of the `[lib]` table or cargo's default `src/lib.rs` where the
/// manifest names none, which is the crate root of a scaffold of `aosabi new`.
#[must_use]
pub fn entry_not_library(value: &Path, lib_path: &Path) -> CliError {
    CliError::coded(
        AO_C0104,
        format!(
            "the entry {} is not the crate root {} of the package, and the entry of an Aosabi service is its crate root",
            value.display(),
            lib_path.display()
        ),
    )
    .with_hint("leave out -e and -m, which default to the crate root")
}

/// Returns the refusal of the plain crate of the package `package`, whose
/// source states the constructs `refusals` the builder cannot read, each a
/// line `<file>:<line>: <message>` with the file relative to the package,
/// `AO-C0105`.
///
/// The message names every refusal on a line of its own, so the build
/// prints each at the file and the line of the author, and the hint is the
/// fix each line names, since no flag changes the outcome.
#[must_use]
pub fn source_refused(package: &str, refusals: &[String]) -> CliError {
    let count = if refusals.len() == 1 {
        String::from("1 construct")
    } else {
        format!("{} constructs", refusals.len())
    };
    let mut message = format!(
        "the builder cannot read {count} of the plain source of the crate `{package}`, each refused at its file and line"
    );
    for refusal in refusals {
        message.push_str("\n  ");
        message.push_str(refusal);
    }
    CliError::coded(AO_C0105, message)
        .with_hint("write each construct in the form its line names, then build again")
}

/// Returns the refusal of the package `package`, which names no build side
/// and whose source the builder cannot read for the reason `error`, the error
/// of the reader at its file and line, `AO-C0105`.
///
/// Such a package is a plain crate, so the builder reads its source, and the
/// hint is the fix the error names, since no flag changes the outcome.
#[must_use]
pub fn source_unread(package: &str, error: &str) -> CliError {
    CliError::coded(
        AO_C0105,
        format!(
            "the builder cannot read the plain source of the crate `{package}`, which names no build side\n  {error}"
        ),
    )
    .with_hint("make the file the error names readable as Rust source, then build again")
}

/// Returns the refusal of the package `package` of the derive path, whose
/// entry the generated workspace would name as the function `default`, and
/// whose facade, one that declares the feature `mirror`, it names without
/// that feature, `AO-C0106`.
///
/// The derive path is retired for application code, so the message names the
/// native form of the entry, and the hint is the fix, since no flag of a
/// command changes the outcome.
#[must_use]
pub fn derive_path_retired(package: &str) -> CliError {
    CliError::coded(
        AO_C0106,
        format!(
            "the package `{package}` holds its roots in a fragment of `graphql!` with the entry `default` of the derive path, which is retired, and an application registers its roots on the chain of its entry, a public function of the crate root that answers an app"
        ),
    )
    .with_hint(
        "write the roots as plain types and the entry as

    pub fn app() -> impl IntoApp {
        App::new().query(Query)
    }

then remove build.rs and the [lib] path",
    )
}

/// Returns the refusal of the package `package`, whose `aosabi.config.rs`
/// stands beside a facade, one that declares the feature `mirror`, it names
/// without that feature, `AO-C0107`.
///
/// The file and `aosabi::serve` are retired for the configuration the chain of
/// the entry carries, which `aosabi::serve_app` serves, so the message names
/// that form, and the hint is the fix, since no flag of a command changes the
/// outcome.
#[must_use]
pub fn config_retired(package: &str) -> CliError {
    CliError::coded(
        AO_C0107,
        format!(
            "the package `{package}` holds aosabi.config.rs, which is retired, and the chain of the entry carries the plugins and the options of a configuration, which aosabi::serve_app serves"
        ),
    )
    .with_hint(
        "register each plugin with `.plugin(..)` and set each option by its call on the chain of the entry, then remove aosabi.config.rs",
    )
}

/// Returns the refusal of the package `package`, whose manifest names the
/// crate `dependency` of Aosabi from a registry under the key `key`,
/// `AO-C0108`.
///
/// Aosabi is released through its repository alone, so a project names its
/// crates by path from a checkout of Aosabi or from a repository. The hint
/// changes the source of the entry alone: the path of a checkout of Aosabi,
/// with the placeholder `<checkout>` for its directory, in place of the
/// version requirement and of any registry, with the key of the entry, a
/// renamed one among them, and every other field kept. The folder and the
/// name of the crate in the checkout are the name as [`canonical_name`]
/// spells it, which the line names in the field `package` where the key
/// differs from it. The command line takes every name with a prefix of
/// [`crate::project::FRAMEWORK_PREFIXES`] for a crate of Aosabi, so for such
/// a name the hint says so, and that a crate of another author under such a
/// name comes by path or from a repository as well.
#[must_use]
pub fn registry_source(package: &str, key: &str, dependency: &str) -> CliError {
    let name = canonical_name(dependency);
    let entry = if key == name {
        format!("{key} = {{ path = \"<checkout>/crates/{name}\" }}")
    } else {
        format!("{key} = {{ package = \"{name}\", path = \"<checkout>/crates/{name}\" }}")
    };
    let prefixed = framework_prefix(&name).map_or_else(String::new, |prefix| {
        format!(
            "\n\nthe command line takes every name with the prefix `{prefix}` for a crate of Aosabi and refuses it from a registry, so a crate of another author under such a name comes by path or from a repository as well"
        )
    });
    CliError::coded(
        AO_C0108,
        format!(
            "the package `{package}` names `{dependency}` from a registry, and Aosabi is released through its repository alone"
        ),
    )
    .with_hint(format!(
        "name `{dependency}` by path from a checkout of Aosabi in every entry of the manifest of `{package}` or of its workspace that names it, with the path in place of the version requirement and of any registry, and with the key and every other field of the entry kept\n\n    {entry}{prefixed}"
    ))
}

/// Returns the refusal of the crate `dependency` of Aosabi at `version`,
/// which the lock `lock` pins or the graph cargo resolved for the project
/// holds from the registry `source`, for the package `package` that depends
/// on it, `AO-C0108`.
///
/// Such a crate comes in where no manifest of the workspace names it from a
/// registry: through a path package outside the workspace, a `[patch]` table
/// or a dependency of a dependency. The hint changes where the graph takes
/// it and then says what mends the lock that was read: the command that
/// drops the pin from the lock of the project, which cargo keeps until it
/// resolves again, the next build for the lock a build derived from it,
/// which that build derives again from the lock of the project, and the same
/// command in the folder of the checkout of a path facade for the lock of
/// that checkout, [`in_checkout`].
#[must_use]
pub fn registry_resolved(
    package: &str,
    dependency: &str,
    version: &str,
    source: &str,
    lock: CheckedLock<'_>,
) -> CliError {
    let change = format!(
        "take `{dependency}` by path from a checkout of Aosabi where `{package}` names it, or through a [patch] table, or drop the dependency that brings it in"
    );
    let (place, hint) = match lock {
        CheckedLock::Project => (
            String::new(),
            format!("{change}, then drop its pin from Cargo.lock\n\n    cargo update --workspace"),
        ),
        CheckedLock::Derived => (
            " in the lock the build derived from the lock of the project".to_owned(),
            format!(
                "{change}, and the next build derives its lock again from the lock of the project"
            ),
        ),
        CheckedLock::Checkout(path) => (
            format!(" in the lock {} of the checkout", path.display()),
            format!(
                "{change}, then drop its pin from the lock of the checkout\n\n    {}",
                in_checkout(path, "cargo update --workspace")
            ),
        ),
    };
    CliError::coded(
        AO_C0108,
        format!(
            "the package `{package}` takes `{dependency}` {version} from the registry {source}{place}, and Aosabi is released through its repository alone"
        ),
    )
    .with_hint(hint)
}

/// Returns the refusal of the crate `dependency` of Aosabi at `version`,
/// which the lock `lock` pins from the registry `source` while no package of
/// the lock depends on it, `AO-C0108`.
///
/// Cargo writes no such pin, so it is left by an edit or a merge of the lock,
/// and `cargo update --workspace`, which writes the pins of the graph of the
/// workspace alone, drops it. A lock a build derived is derived again by the
/// next build.
#[must_use]
pub fn registry_unused_pin(
    dependency: &str,
    version: &str,
    source: &str,
    lock: CheckedLock<'_>,
) -> CliError {
    let (holder, hint) = match lock {
        CheckedLock::Project => (
            "the lock of the project".to_owned(),
            "drop the unused pin from Cargo.lock\n\n    cargo update --workspace".to_owned(),
        ),
        CheckedLock::Derived => (
            "the lock the build derived from the lock of the project".to_owned(),
            "the next build derives its lock again from the lock of the project".to_owned(),
        ),
        CheckedLock::Checkout(path) => (
            format!("the lock {} of the checkout", path.display()),
            format!(
                "drop the unused pin from the lock of the checkout\n\n    {}",
                in_checkout(path, "cargo update --workspace")
            ),
        ),
    };
    CliError::coded(
        AO_C0108,
        format!(
            "{holder} pins `{dependency}` {version} from the registry {source}, which no package of the lock depends on, and Aosabi is released through its repository alone"
        ),
    )
    .with_hint(hint)
}

/// Returns the refusal of the verb `command`, which arrives with device
/// cards, `AO-C0201`.
#[must_use]
pub fn card_track_verb(command: &str) -> CliError {
    CliError::coded(
        AO_C0201,
        format!(
            "`{command}` arrives with device cards, and until then a build names one of the built in cards {BUILTIN_CARD_NAMES}"
        ),
    )
    .with_hint("cargo aosabi build --device server")
}

/// Returns the refusal of the device `device`, a card file or an installed
/// card id, which a build reads once device cards land, `AO-C0201`.
#[must_use]
pub fn card_file(device: &str) -> CliError {
    CliError::coded(
        AO_C0201,
        format!(
            "the device {device} names a card file or an installed card, which a build reads once device cards land, and until then a build names one of the built in cards {BUILTIN_CARD_NAMES}"
        ),
    )
    .with_hint("cargo aosabi build --device server")
}

/// Returns the refusal of a card under `devices/`, which a build reads once
/// device cards land, `AO-C0201`.
#[must_use]
pub fn devices_folder() -> CliError {
    CliError::coded(
        AO_C0201,
        format!(
            "devices/ holds a card, which a build reads once device cards land, and until then a build names one of the built in cards {BUILTIN_CARD_NAMES}"
        ),
    )
    .with_hint("cargo aosabi build --device server")
}

/// Returns the refusal of the device `name`, which names no built in card,
/// `AO-C0202`.
#[must_use]
pub fn no_builtin_card(name: &str) -> CliError {
    CliError::coded(
        AO_C0202,
        format!(
            "the device {name} names no built in card, and the built in cards are {BUILTIN_CARD_NAMES}"
        ),
    )
    .with_hint("cargo aosabi build --device server")
}

/// Returns the refusal of the target `triple`, whose standard library the
/// toolchain lacks, `AO-R0105`.
#[must_use]
pub fn target_missing(triple: &str) -> CliError {
    CliError::coded(
        AO_R0105,
        format!(
            "the toolchain has no standard library for the target {triple} the card builds for"
        ),
    )
    .with_hint(format!("rustup target add {triple}"))
}

/// Returns the refusal of the packages `packages`, each written as its name
/// and version, which the lock of the project does not pin, `AO-C0301`.
#[must_use]
pub fn unpinned_packages(packages: &[String]) -> CliError {
    CliError::coded(
        AO_C0301,
        format!(
            "the generated workspace needs packages that Cargo.lock of the project does not pin: {}",
            packages.join(", ")
        ),
    )
    .with_hint("cargo update -p aosabi")
}

/// Returns the refusal of a missing lock under the flag `flag`, `--locked`
/// or `--frozen`, `AO-C0302`: of the project, or of the workspace of a path
/// facade where `checkout` names that lock.
#[must_use]
pub fn lock_missing(flag: &str, checkout: Option<&Path>) -> CliError {
    match checkout {
        None => CliError::coded(
            AO_C0302,
            format!("Cargo.lock of the project is missing, and {flag} forbids writing it"),
        )
        .with_hint("cargo generate-lockfile"),
        Some(path) => CliError::coded(
            AO_C0302,
            format!(
                "the lock {} of the checkout is missing, and {flag} forbids writing it",
                path.display()
            ),
        )
        .with_hint(in_checkout(path, "cargo generate-lockfile")),
    }
}

/// Returns the cargo command `command` for the lock `lock` of the checkout
/// of a path facade, run in the folder of that lock, where the command line
/// runs every cargo call over it, so cargo reads the configuration of that
/// folder and rustup its toolchain file, and not those of the folder the
/// user stands in.
#[must_use]
pub fn in_checkout(lock: &Path, command: &str) -> String {
    let folder = lock.parent().unwrap_or(lock);
    format!("cd {} && {command}", shell_path(folder))
}

/// Returns the refusal of the target of `new` of `args`, which exists and is
/// not empty, `AO-C0401`.
///
/// The hint is the command of `args` with `--force`, from the checkout
/// `checkout` the command was given, which every scaffold names,
/// `AO-C0405`.
#[must_use]
pub fn target_not_empty(args: &NewArgs, checkout: &Path) -> CliError {
    CliError::coded(
        AO_C0401,
        format!(
            "the target {} exists and is not empty",
            args.target.display()
        ),
    )
    .with_hint(new_command(
        &shell_path(&args.target),
        &shell_path(checkout),
        workers(args),
        args,
        true,
    ))
}

/// Returns the refusal of the package name `name`, which breaks the rule
/// `rule` cargo applies, `AO-C0402`, with the command of `args` and the
/// checkout `checkout` the command was given in its hint, as
/// [`target_not_empty`] names it, and the placeholder `<target>` for the
/// target.
#[must_use]
pub fn bad_package_name(name: &str, rule: &str, args: &NewArgs, checkout: &Path) -> CliError {
    CliError::coded(
        AO_C0402,
        format!("the target gives the package name `{name}`, which cargo refuses: {rule}"),
    )
    .with_hint(format!(
        "{}, with a target whose last component is a valid package name",
        new_command(
            "<target>",
            &shell_path(checkout),
            workers(args),
            args,
            args.force
        )
    ))
}

/// Returns the refusal of the feature `feature`, which the runtime of `args`
/// lacks, `AO-C0404`.
///
/// The hint is the command of `args` from the checkout `checkout` under the
/// runtime `native`, which carries both features.
#[must_use]
pub fn invalid_feature(feature: &str, args: &NewArgs, checkout: &Path) -> CliError {
    CliError::coded(AO_C0404, format!("Invalid feature selected: {feature}")).with_hint(
        new_command(
            &shell_path(&args.target),
            &shell_path(checkout),
            Some("native"),
            args,
            args.force,
        ),
    )
}

/// Returns the refusal of `new` of `args`, which names no checkout of Aosabi
/// through `--aosabi-path`, `AO-C0405`.
///
/// Aosabi is released through its repository alone, so a scaffold names the
/// facade by path from a checkout, and the command writes no requirement of
/// a registry. The hint is the command of `args` with the option, with the
/// placeholder `<checkout>` for the directory of the checkout.
#[must_use]
pub fn no_checkout(args: &NewArgs) -> CliError {
    CliError::coded(
        AO_C0405,
        "Aosabi is released through its repository alone, so a new service names its crates by path from a checkout of Aosabi, which --aosabi-path names",
    )
    .with_hint(new_command(
        &shell_path(&args.target),
        "<checkout>",
        workers(args),
        args,
        args.force,
    ))
}

/// Returns the command `new` of the target `target` from the checkout
/// `checkout`, each a word of the shell or a placeholder, under the runtime
/// `runtime` where it names one, with the features of `args` in their order
/// and with `--force` where `force` holds, the hint of every refusal of
/// `new`, so a user who runs it scaffolds the service `args` asks for.
fn new_command(
    target: &str,
    checkout: &str,
    runtime: Option<&str>,
    args: &NewArgs,
    force: bool,
) -> String {
    let mut command = format!("aosabi new {target} --aosabi-path {checkout}");
    if let Some(runtime) = runtime {
        let _ = write!(command, " --runtime {runtime}");
    }
    if !args.features.is_empty() {
        command.push_str(" --features");
        for feature in &args.features {
            let _ = write!(command, " {}", feature_name(*feature));
        }
    }
    if force {
        command.push_str(" --force");
    }
    command
}

/// Returns the runtime `workers` where `args` names it, which the command of
/// a hint names, and `None` for the default runtime `native`.
fn workers(args: &NewArgs) -> Option<&'static str> {
    (args.runtime == Runtime::Workers).then_some("workers")
}

/// Returns `path` as one word of the shell: as it stands where it holds
/// nothing but ASCII letters, digits and `@%+=:,./-_`, which the shell reads
/// as they are, and in single quotes otherwise, each single quote of it
/// closed, escaped and opened again.
fn shell_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    if !text.is_empty()
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"@%+=:,./-_".contains(&byte))
    {
        text.into_owned()
    } else {
        format!("'{}'", text.replace('\'', "'\\''"))
    }
}

/// Returns the refusal of the URL `url`, whose scheme is neither `http` nor
/// `https`, `AO-C0501`.
#[must_use]
pub fn url_scheme(url: &str) -> CliError {
    CliError::coded(
        AO_C0501,
        format!("the URL {url} names no http:// or https:// scheme"),
    )
    .with_hint("aosabi pull http://<host>:<port>/graphql")
}

/// Returns the refusal of the `https://` URL `url` under the `http://` only
/// client, `AO-C0501`, with the same URL over `http://` as the hint.
#[must_use]
pub fn https_unsupported(url: &str, http_url: &str) -> CliError {
    CliError::coded(
        AO_C0501,
        format!("the URL {url} is https://, and this build of aosabi speaks http:// alone"),
    )
    .with_hint(format!("aosabi pull {http_url}"))
}

/// Returns the refusal of the remote type name `name`, which collides with an
/// item the registry writes, `AO-C0502`, with no hint, since no command of
/// this build renames a type of the remote.
#[must_use]
pub fn registry_collision(name: &str) -> CliError {
    CliError::coded(
        AO_C0502,
        format!(
            "the remote type {name} collides with an item the registry writes, Registry, args or a root name used as an object type"
        ),
    )
}

/// Returns the refusal of the command `command`, which lands with `lands`, a
/// later release, `AO-C0601`, with no hint, since no command of this build
/// carries it.
#[must_use]
pub fn later_release(command: &str, lands: &str) -> CliError {
    CliError::coded(
        AO_C0601,
        format!("`{command}` lands with {lands}, and this build of aosabi does not carry it"),
    )
}

/// Returns the refusal of the container engine `engine`, which failed with
/// the output `detail`, `AO-C0701`.
#[must_use]
pub fn engine_failed(engine: &str, detail: String) -> CliError {
    CliError::coded(AO_C0701, format!("the container engine {engine} failed"))
        .with_hint("AOSABI_CONTAINER_ENGINE=none cargo aosabi build --device docker")
        .with_detail(detail)
}
