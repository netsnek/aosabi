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
//! prints the command that would change the outcome as its hint. No command of
//! this build renames the types of a remote or brings a command of a later
//! milestone, so those two print no hint. The text of each refusal is built by
//! one function of this module from one string literal, so a command that
//! refuses names the function and never assembles a message at the site that
//! fails (`Documentation/coding-style.md` section 7).
//!
//! The codes `AO-C` are the command line's own. `AO-R0105` is the design code
//! of `docs/device-cards.md` section 6 for a host tool the card needs, which
//! the command line uses for a missing standard library of the card's target.

use std::path::Path;

use crate::report::CliError;

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
/// A card file, an installed card or a verb of the device card track.
pub const AO_C0201: &str = "AO-C0201";
/// A device that names no built in card.
pub const AO_C0202: &str = "AO-C0202";
/// A host tool the card needs is absent.
pub const AO_R0105: &str = "AO-R0105";
/// A package the lock of the project does not pin, other than the build side
/// of a plain crate from the source and at the version of the facade it pins.
pub const AO_C0301: &str = "AO-C0301";
/// The lock of the project is missing under `--locked` or `--frozen`.
pub const AO_C0302: &str = "AO-C0302";
/// The target of `new` exists and is not empty.
pub const AO_C0401: &str = "AO-C0401";
/// The target of `new` gives no valid package name.
pub const AO_C0402: &str = "AO-C0402";
/// A feature the runtime of `new` lacks.
pub const AO_C0404: &str = "AO-C0404";
/// A URL of `pull` whose scheme the client does not speak.
pub const AO_C0501: &str = "AO-C0501";
/// A remote type name that collides with an item the registry writes.
pub const AO_C0502: &str = "AO-C0502";
/// A command that lands with a later milestone.
pub const AO_C0601: &str = "AO-C0601";
/// The container engine failed.
pub const AO_C0701: &str = "AO-C0701";

/// Every code of the command line in the order of the plan's table, with
/// `AO-C0105`, `AO-C0106` and `AO-C0107` after the codes of the project.
pub const CODES: [Code; 19] = [
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
        meaning: "the package names aosabi from two sources or at two requirements, or carries a [workspace] table that lists members, because a service is one crate",
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
        meaning: "a package of the derive path, whose roots a fragment of graphql! holds and whose entry is the function default, whose facade, one that declares the feature mirror, it names without that feature, since item A11 of APL1 retires that path for application code",
    },
    Code {
        id: AO_C0107,
        meaning: "aosabi.config.rs in a package whose facade, one that declares the feature mirror, it names without that feature, since item A11 of APL1 retires the file for the configuration the chain of the entry carries, which serve_app serves",
    },
    Code {
        id: AO_C0201,
        meaning: "a card file, an installed card id, a card in devices/, or a card or registry verb, which arrive with the device card track",
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
        meaning: "Cargo.lock of the project is missing under --locked or --frozen",
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
        id: AO_C0501,
        meaning: "pull got a URL whose scheme is not http or https, or https under the http:// only client",
    },
    Code {
        id: AO_C0502,
        meaning: "a remote type name collides with an item the registry writes",
    },
    Code {
        id: AO_C0601,
        meaning: "a db sub command, which lands with M9, docs, which lands with M14, or bridge, which lands with M12",
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
    meaning: "new --features auth before M7b, which brings zitadel_auth. Retired with the landing of M7b, which ends the refusal, because the facade carries zitadel_auth under the feature auth-zitadel and new writes the scaffold",
}];

/// The names of the built in cards, as a refusal lists them.
pub const BUILTIN_CARD_NAMES: &str = "server, server-legacy, docker, worker and esp32c3-devkitm1";

/// Returns the refusal of the package `package`, which is no Aosabi service,
/// `AO-C0101`.
///
/// A `plain` crate needs the facade alone, so its message and its hint name
/// the facade alone, and a crate of the derive path needs the facade and its
/// build side, so its message names both and its hint the manifest and the
/// build script of the derive path.
#[must_use]
pub fn not_a_service(package: &str, plain: bool) -> CliError {
    if plain {
        return CliError::coded(
            AO_C0101,
            format!(
                "the package `{package}` names no dependency `aosabi`, so it is no Aosabi service"
            ),
        )
        .with_hint("add the facade to Cargo.toml\n\n    [dependencies]\n    aosabi = \"0.0\"");
    }
    CliError::coded(
        AO_C0101,
        format!(
            "the package `{package}` names no dependency `aosabi` or no build dependency `aosabi-build`, so it is no Aosabi service"
        ),
    )
    .with_hint(
        "add the facade and its build side to Cargo.toml\n\n    [dependencies]\n    aosabi = \"0.0\"\n\n    [build-dependencies]\n    aosabi-build = \"0.0\"\n\nand the one line build.rs\n\n    fn main() -> std::process::ExitCode {\n        aosabi_build::declarations_main()\n    }",
    )
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
#[must_use]
pub fn workspace_with_members(package: &str) -> CliError {
    CliError::coded(
        AO_C0103,
        format!(
            "the package `{package}` carries a [workspace] table that lists members, and a service is one crate"
        ),
    )
    .with_hint("keep the [workspace] table of the service empty")
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
#[must_use]
pub fn derive_path_retired(package: &str) -> CliError {
    CliError::coded(
        AO_C0106,
        format!(
            "the package `{package}` holds its roots in a fragment of `graphql!` with the entry `default` of the derive path, which is retired, and an application registers its roots on the chain of its entry, a public function of the crate root that answers an app, as docs/authoring.md section 4 writes it"
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
#[must_use]
pub fn config_retired(package: &str) -> CliError {
    CliError::coded(
        AO_C0107,
        format!(
            "the package `{package}` holds aosabi.config.rs, which is retired, and the chain of the entry carries the plugins and the options of a configuration, which aosabi::serve_app serves, as docs/authoring.md section 6 writes it"
        ),
    )
    .with_hint(
        "register each plugin with `.plugin(..)` and set each option by its call on the chain of the entry, then remove aosabi.config.rs",
    )
}

/// Returns the refusal of the verb `command` of the device card track, which
/// lands at the track row `lands`, `AO-C0201`.
#[must_use]
pub fn card_track_verb(command: &str, lands: &str) -> CliError {
    CliError::coded(
        AO_C0201,
        format!(
            "`{command}` arrives with the device card track at {lands}, and until then a build names one of the built in cards {BUILTIN_CARD_NAMES}"
        ),
    )
    .with_hint("cargo aosabi build --device server")
}

/// Returns the refusal of the device `device`, a card file or an installed
/// card id, which the build reads from the track row `lands` on, `AO-C0201`.
#[must_use]
pub fn card_file(device: &str, lands: &str) -> CliError {
    CliError::coded(
        AO_C0201,
        format!(
            "the device {device} names a card file or an installed card, which a build reads from {lands} of the device card track on, and until then a build names one of the built in cards {BUILTIN_CARD_NAMES}"
        ),
    )
    .with_hint("cargo aosabi build --device server")
}

/// Returns the refusal of a card under `devices/`, which a build reads from
/// the track row `lands` on, `AO-C0201`.
#[must_use]
pub fn devices_folder(lands: &str) -> CliError {
    CliError::coded(
        AO_C0201,
        format!(
            "devices/ holds a card, which a build reads from {lands} of the device card track on, and until then a build names one of the built in cards {BUILTIN_CARD_NAMES}"
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

/// Returns the refusal of a missing lock of the project under the flag
/// `flag`, `--locked` or `--frozen`, `AO-C0302`.
#[must_use]
pub fn lock_missing(flag: &str) -> CliError {
    CliError::coded(
        AO_C0302,
        format!("Cargo.lock of the project is missing, and {flag} forbids writing it"),
    )
    .with_hint("cargo generate-lockfile")
}

/// Returns the refusal of the target `target` of `new`, which exists and is
/// not empty, `AO-C0401`.
#[must_use]
pub fn target_not_empty(target: &Path) -> CliError {
    CliError::coded(
        AO_C0401,
        format!("the target {} exists and is not empty", target.display()),
    )
    .with_hint(format!("aosabi new {} --force", target.display()))
}

/// Returns the refusal of the package name `name`, which breaks the rule
/// `rule` cargo applies, `AO-C0402`.
#[must_use]
pub fn bad_package_name(name: &str, rule: &str) -> CliError {
    CliError::coded(
        AO_C0402,
        format!("the target gives the package name `{name}`, which cargo refuses: {rule}"),
    )
    .with_hint("aosabi new <target>, with a target whose last component is a valid package name")
}

/// Returns the refusal of the feature `feature`, which the runtime lacks,
/// `AO-C0404`.
///
/// The hint is `new` of the target `target` with the values `features` of
/// `--features`, joined by spaces, under the runtime `native`, which carries
/// both features.
#[must_use]
pub fn invalid_feature(feature: &str, target: &Path, features: &str) -> CliError {
    CliError::coded(AO_C0404, format!("Invalid feature selected: {feature}")).with_hint(format!(
        "aosabi new {} --runtime native --features {features}",
        target.display()
    ))
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

/// Returns the refusal of the command `command`, which lands with the later
/// milestone `lands`, `AO-C0601`, with no hint, since no command of this
/// build carries it.
#[must_use]
pub fn later_milestone(command: &str, lands: &str) -> CliError {
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
