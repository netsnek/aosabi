// SPDX-License-Identifier: MIT
/*
 * aosabi-cli: the built in cards and their resolution.
 *
 * No analogue in the reference. The cards are the table of
 * docs/cli-and-targets.md section 5.0 as data. Each file of the directory says
 * what it holds.
 */

//! The built in cards.
//!
//! With no `--device` and no card in `devices/`, the card is `server`.
//!
//! [`Card`] carries what a build learns from its card: the class, the target
//! and profile, every carrier with its protocols, the encoding, what it
//! compiles, whether it serves and the set of its release build at the switch
//! `AOSABI_STD` at `source`.

use std::path::Path;

use crate::{
    codes,
    generate::ServerPackage,
    project::Project,
    report::CliError,
    std_source::SourceSet, //
};

pub mod builtin;
pub mod digests;

/// The row of the device card track that reads a card file, an installed
/// card and the folder `devices/` in a build.
pub const CARD_FILE_ROW: &str = "DC5";

/// The name of the card a build takes with no `--device`.
pub const DEFAULT_CARD: &str = "server";

/// The folder of the installed cards beside the sources.
pub const DEVICES_DIR: &str = "devices";

/// What a build learns from its card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Card {
    /// The name `--device` gives.
    pub name: &'static str,
    /// The class of the card.
    pub class: Class,
    /// The target the card compiles for, `None` for the host.
    pub triple: Option<&'static str>,
    /// The profile the card builds in.
    pub profile: Profile,
    /// Every carrier with the protocols it links.
    pub carriers: &'static [(Carrier, &'static [Protocol])],
    /// The encoding of every GraphQL result.
    pub encoding: Encoding,
    /// What the card compiles.
    pub builds: Builds,
    /// Whether the card serves a socket, which is when the build step runs.
    pub serves: bool,
    /// The set a release build of the card takes at the switch `AOSABI_STD`
    /// at `source`, `None` for a card that builds stock at every value.
    pub source: Option<SourceSet>,
}

/// The class of a card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    /// A server.
    Server,
    /// A container image of a server.
    Container,
    /// A Cloudflare Worker.
    Worker,
    /// A microcontroller.
    Mcu,
}

/// The profile of a build.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    /// The development profile, the release profile under `--release`.
    Dev,
    /// The release profile.
    Release,
}

/// A carrier of ADR-0023.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Carrier {
    /// Network.
    Network,
    /// Unix Socket.
    UnixSocket,
    /// USB.
    Usb,
    /// Bluetooth.
    Bluetooth,
    /// Serial Line.
    SerialLine,
    /// `LoRa`.
    LoRa,
}

/// A protocol of ADR-0023.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protocol {
    /// HTTP(TCP).
    HttpTcp,
    /// HTTP(Unix Socket).
    HttpUnixSocket,
    /// GraphTP(Homa).
    GraphtpHoma,
}

/// The encoding of a GraphQL result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    /// JSON.
    Json,
}

/// What a card compiles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Builds {
    /// The generated server package for the host.
    ServerPackage(ServerPackage),
    /// The library of the service for the card's target with the features of
    /// the facade the card names.
    AppForTarget {
        /// The features of the facade, each `aosabi/<feature>`.
        features: &'static [&'static str],
    },
    /// Packages of the workspace of a path facade for the card's target.
    KernelForTarget {
        /// Each package with whether it builds without its default features.
        packages: &'static [(&'static str, bool)],
    },
}

impl Class {
    /// Returns the name of the class.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Server => "server",
            Self::Container => "container",
            Self::Worker => "worker",
            Self::Mcu => "mcu",
        }
    }
}

impl Profile {
    /// Returns the name of the profile.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Dev => "dev",
            Self::Release => "release",
        }
    }
}

impl Carrier {
    /// Returns the name of the carrier as ADR-0023 spells it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Network => "Network",
            Self::UnixSocket => "Unix Socket",
            Self::Usb => "USB",
            Self::Bluetooth => "Bluetooth",
            Self::SerialLine => "Serial Line",
            Self::LoRa => "LoRa",
        }
    }
}

impl Protocol {
    /// Returns the name of the protocol as ADR-0023 spells it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::HttpTcp => "HTTP(TCP)",
            Self::HttpUnixSocket => "HTTP(Unix Socket)",
            Self::GraphtpHoma => "GraphTP(Homa)",
        }
    }
}

impl Encoding {
    /// Returns the name of the encoding.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Json => "JSON",
        }
    }
}

/// Returns the card the argument `arg` of `--device` names for `project`.
///
/// # Errors
///
/// Returns `AO-C0201` for a card file, an installed card id, or a card in
/// `devices/` with no `--device`, and `AO-C0202` for a name of no built in
/// card.
pub fn resolve(arg: Option<&str>, project: &Project) -> Result<&'static Card, CliError> {
    let Some(name) = arg else {
        if holds_a_card(project) {
            return Err(codes::devices_folder(CARD_FILE_ROW));
        }
        return by_name(DEFAULT_CARD).ok_or_else(|| codes::no_builtin_card(DEFAULT_CARD));
    };
    let picture = Path::new(name)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("png"));
    if picture || name.contains('/') {
        return Err(codes::card_file(name, CARD_FILE_ROW));
    }
    by_name(name).ok_or_else(|| codes::no_builtin_card(name))
}

/// Returns the built in card named `name`.
#[must_use]
pub fn by_name(name: &str) -> Option<&'static Card> {
    builtin::BUILTIN.iter().find(|card| card.name == name)
}

/// Returns whether `devices/` of `project` holds a file, a card the track
/// installs.
fn holds_a_card(project: &Project) -> bool {
    std::fs::read_dir(project.root.join(DEVICES_DIR)).is_ok_and(|entries| {
        entries
            .filter_map(Result::ok)
            .any(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
    })
}
