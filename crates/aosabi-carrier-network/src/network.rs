// SPDX-License-Identifier: MIT
/*
 * No analogue on the reference side.
 */

//! What a card names for `GraphTP`(Homa) on Network.
//!
//! A card names the path, the crate's engine over raw IP 146 or the socket of
//! the crate's kernel module, the [`Protection`] of the path under Homa, which
//! has no default because Homa carries no security of its own, whether the
//! path is required or optional, the Homa processes of the host, which share
//! its link and its `max_incoming` equally, and the MTU of the path.
//!
//! The Homa port of a service is its HTTP port, 3000 by the port rule of the
//! serving plugin, so one service is one port of the host on both protocols. A
//! peer is an address with its Homa port, so the streams of two client
//! processes of one host are kept apart.

use core::{
    fmt,
    net::{
        IpAddr,
        Ipv4Addr, //
    }, //
};
use std::{
    env,
    fs::File,
    io::{
        self,
        Read as _, //
    }, //
};

use aosabi_serve::{
    NodeServerOptions,
    port_rule, //
};
use homa::{
    Config,
    ConfigError,
    Encap,
    Profile, //
};

/// The variable that names the path, `ip`, `ip:<port>` or `kernel:<port>`.
pub const HOMA_NETWORK_ENV: &str = "AOSABI_HOMA_NETWORK";

/// The variable that names the protection, `ipsec`, `trusted-fabric` or
/// `wireguard-without-priorities`.
pub const PROTECTION_ENV: &str = "AOSABI_HOMA_PROTECTION";

/// The variable that makes the path required where it holds `1`.
pub const REQUIRED_ENV: &str = "AOSABI_HOMA_REQUIRED";

/// The variable that names the Homa processes of the host, 1 by default.
pub const PROCESSES_ENV: &str = "AOSABI_HOMA_PROCESSES";

/// The variable that names the local address, every address of IPv4 by
/// default, whose family is the family of the path.
pub const BIND_ENV: &str = "AOSABI_HOMA_BIND";

/// The variable that names the MTU of the path, 1,500 by default.
pub const MTU_ENV: &str = "AOSABI_HOMA_MTU";

/// The longest Homa message, which a server card answers whole.
pub const MAX_MESSAGE: u32 = 1_000_000;

/// The bytes of one page of the endpoint, so that the 16 pages a slot takes
/// at most hold a message of [`MAX_MESSAGE`] bytes, since the full form
/// carries no LIMITS and the engine takes the longest message of a peer from
/// its own pages.
pub const PAGE_LEN: u32 = 62_500;

/// The bytes of the page pool, 16 MiB, which the card may lower.
pub const POOL: u32 = 16 << 20;

/// The pages of the pool with the two bytes a page of the page table.
pub const PAGES: u32 = POOL / (PAGE_LEN + 2);

/// The peers an endpoint keeps at once, idle ones evicted to admit more.
pub const PEERS: u32 = 4_096;

/// The RPCs an endpoint serves at once.
pub const SLOTS: u32 = 16_384;

/// The gaps a message may have while it arrives.
pub const MAX_GAPS: u8 = 16;

/// The MTU of a path that names none.
pub const DEFAULT_MTU: u16 = 1_500;

/// The smallest MTU a path may name, the datagram every IPv4 host takes.
pub const MIN_MTU: u16 = 576;

/// The rate of the host's link in bits a second, `HomaModule`'s `link_mbps` of
/// 25,000.
pub const LINK_BITS_PER_S: u64 = 25_000_000_000;

/// The backlog the rate pacer lets wait in the link, and the shortest message
/// it holds back, `HomaModule`'s defaults.
pub const PACER_BACKLOG_NS: u64 = 5_000;
/// See [`PACER_BACKLOG_NS`].
pub const PACER_DEFER_MIN: u32 = 1_000;

/// The bytes of the socket's buffers, above the 400,000 bytes of
/// `max_incoming` the profile grants at once, as the plan asks of a card.
pub const SOCKET_BUFFER: usize = 4 << 20;

/// How `GraphTP`(Homa) reaches Network on this host.
#[derive(Clone, Debug)]
pub enum Path {
    /// The crate's engine over a raw socket of IP protocol 146.
    Ip(homa::ip::Options),
}

/// The layer that protects `GraphTP`(Homa) on Network, which a card must
/// name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protection {
    /// `IPsec`, which keeps the DS field, so the priorities reach the switch.
    Ipsec,
    /// A fabric the operator trusts.
    TrustedFabric,
    /// `WireGuard` writes 0 into the outer TOS, so the priorities are declared
    /// absent.
    WireGuardWithoutPriorities,
}

impl Protection {
    /// Returns the protection of its name, `ipsec`, `trusted-fabric` or
    /// `wireguard-without-priorities`.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "ipsec" => Some(Self::Ipsec),
            "trusted-fabric" => Some(Self::TrustedFabric),
            "wireguard-without-priorities" => Some(Self::WireGuardWithoutPriorities),
            _ => None,
        }
    }

    /// Returns the name [`Protection::parse`] reads.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Ipsec => "ipsec",
            Self::TrustedFabric => "trusted-fabric",
            Self::WireGuardWithoutPriorities => "wireguard-without-priorities",
        }
    }

    /// Returns whether the packets carry their priority on this path.
    #[must_use]
    pub const fn priorities(self) -> bool {
        !matches!(self, Self::WireGuardWithoutPriorities)
    }
}

/// Whether a card needs the path or serves HTTP(TCP) alone without it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Requirement {
    /// A path that cannot open or that fails ends the process.
    Required,
    /// A path that cannot open prints one line, and the service answers
    /// HTTP(TCP) alone.
    Optional,
}

/// The first identifier and the key of the table of an endpoint.
#[derive(Clone, Copy, Debug)]
pub struct Seed {
    /// The first client identifier, even and not 0.
    pub first_id: u64,
    /// The key of the table hash.
    pub hash_key: [u8; 16],
}

/// Reads a [`Seed`] from `/dev/urandom`, until `aosabi_core::random` lands.
///
/// # Errors
///
/// Returns the error of the file.
pub fn seed() -> io::Result<Seed> {
    let mut bytes = [0u8; 24];
    File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    let mut first = [0u8; 8];
    first.copy_from_slice(&bytes[..8]);
    let mut hash_key = [0u8; 16];
    hash_key.copy_from_slice(&bytes[8..]);
    // The full form carries identifiers of 64 bits, made even and never 0.
    let first_id = u64::from_le_bytes(first) & !1;
    Ok(Seed {
        first_id: if first_id == 0 { 2 } else { first_id },
        hash_key,
    })
}

/// What a card names for `GraphTP`(Homa) on Network. `protection` has no
/// default.
#[derive(Clone, Debug)]
pub struct HomaNetwork {
    /// The path.
    pub path: Path,
    /// The protection of the path under Homa.
    pub protection: Protection,
    /// Whether the path is required.
    pub requirement: Requirement,
    /// Homa processes on the host, each with an equal static share of the
    /// link and of `max_incoming`.
    pub processes: u16,
    /// The MTU of the path.
    pub mtu: u16,
    /// The configuration of the endpoint.
    pub config: Config,
}

impl HomaNetwork {
    /// Returns the raw path of `options` with `protection`, optional, one
    /// process on the host, the default MTU and the configuration of a
    /// server card for the ports of `options`.
    #[must_use]
    pub fn ip(mut options: homa::ip::Options, protection: Protection, seed: Seed) -> Self {
        options.priorities = protection.priorities();
        let config = server_config(options.port, options.extra_ports, seed);
        Self {
            path: Path::Ip(options),
            protection,
            requirement: Requirement::Optional,
            processes: 1,
            mtu: DEFAULT_MTU,
            config,
        }
    }

    /// A path of `ip` alone takes the HTTP port of the port rule. No
    /// protection named is a refusal, never a default.
    ///
    /// # Errors
    ///
    /// Returns [`NetworkError::NoProtection`] where the path is named and no
    /// protection is, [`NetworkError::NoModule`] for the kernel path, whose
    /// client lands at stage H5, and [`NetworkError::Io`] with the kind
    /// `InvalidInput` for a value that names no path, port, protection,
    /// count, address or MTU, and the error of `/dev/urandom`.
    pub fn from_env() -> Result<Option<Self>, NetworkError> {
        let Some(value) = env::var_os(HOMA_NETWORK_ENV) else {
            return Ok(None);
        };
        let value = value.to_string_lossy().into_owned();
        let (kind, port) = match value.split_once(':') {
            Some((kind, port)) => (kind, Some(port)),
            None => (value.as_str(), None),
        };
        let port = match port {
            Some(text) => text
                .parse::<u16>()
                .ok()
                .filter(|&port| port != 0)
                .ok_or_else(|| invalid(HOMA_NETWORK_ENV, &value))?,
            None => port_rule(&NodeServerOptions::default()),
        };
        let protection = match env::var(PROTECTION_ENV) {
            Ok(name) => Protection::parse(&name).ok_or_else(|| invalid(PROTECTION_ENV, &name))?,
            Err(_) => return Err(NetworkError::NoProtection),
        };
        let requirement = if env::var(REQUIRED_ENV).is_ok_and(|value| value == "1") {
            Requirement::Required
        } else {
            Requirement::Optional
        };
        let processes = read_or(PROCESSES_ENV, 1u16)?;
        if processes == 0 {
            return Err(invalid(PROCESSES_ENV, "0"));
        }
        let mtu = read_or(MTU_ENV, DEFAULT_MTU)?;
        if mtu < MIN_MTU {
            return Err(invalid(MTU_ENV, &alloc::format!("{mtu}")));
        }
        let bind = read_or(BIND_ENV, IpAddr::V4(Ipv4Addr::UNSPECIFIED))?;
        match kind {
            "ip" => {
                let options = homa::ip::Options {
                    encap: encap_of(bind),
                    bind,
                    port,
                    extra_ports: 0,
                    priorities: protection.priorities(),
                    send_buffer: SOCKET_BUFFER,
                    recv_buffer: SOCKET_BUFFER,
                };
                let seed = seed().map_err(NetworkError::Io)?;
                Ok(Some(Self {
                    path: Path::Ip(options),
                    protection,
                    requirement,
                    processes,
                    mtu,
                    config: server_config(port, 0, seed),
                }))
            }
            "kernel" => Err(NetworkError::NoModule),
            _ => Err(invalid(HOMA_NETWORK_ENV, &value)),
        }
    }

    /// Returns the family of the path.
    #[must_use]
    pub fn encap(&self) -> Encap {
        match &self.path {
            Path::Ip(options) => options.encap,
        }
    }
}

/// Returns the family of a local address.
#[must_use]
pub const fn encap_of(bind: IpAddr) -> Encap {
    match bind {
        IpAddr::V4(_) => Encap::Ip4,
        IpAddr::V6(_) => Encap::Ip6,
    }
}

/// Reads the variable `name` as a `T`, `default` where it is unset.
fn read_or<T: core::str::FromStr>(name: &str, default: T) -> Result<T, NetworkError> {
    match env::var(name) {
        Ok(text) => text.trim().parse().map_err(|_| invalid(name, &text)),
        Err(_) => Ok(default),
    }
}

/// Returns the refusal of a value of `name` that names nothing it takes.
fn invalid(name: &str, value: &str) -> NetworkError {
    NetworkError::Io(io::Error::new(
        io::ErrorKind::InvalidInput,
        alloc::format!("{name} names nothing this card takes: {value}"),
    ))
}

/// Why `GraphTP`(Homa) on Network did not start or stopped.
#[derive(Debug)]
pub enum NetworkError {
    /// The process lacks `CAP_NET_RAW` in its network namespace.
    NotPermitted,
    /// Another process of the host holds the Homa port.
    PortTaken(u16),
    /// `HomaModule` holds protocol 146 on this host.
    ForeignModule,
    /// No module of the crate holds protocol 146, or this build has no
    /// kernel path.
    NoModule,
    /// The card names the path and no protection.
    NoProtection,
    /// The configuration of the endpoint was refused.
    Config(ConfigError),
    /// Any other failure of a system call.
    Io(io::Error),
}

impl fmt::Display for NetworkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotPermitted => {
                f.write_str("NotPermitted: the process lacks CAP_NET_RAW in its network namespace")
            }
            Self::PortTaken(port) => {
                write!(f, "PortTaken: another process holds the Homa port {port}")
            }
            Self::ForeignModule => f.write_str("ForeignModule: HomaModule holds protocol 146"),
            Self::NoModule => f.write_str(
                "NoModule: no module of the crate holds protocol 146, the kernel path of stage H5",
            ),
            Self::NoProtection => f.write_str(
                "NoProtection: the card names GraphTP(Homa) on Network and no protection, ipsec, trusted-fabric or wireguard-without-priorities",
            ),
            Self::Config(error) => write!(f, "the configuration of the endpoint: {error}"),
            Self::Io(error) => write!(f, "{error}"),
        }
    }
}

impl core::error::Error for NetworkError {}

/// Returns the configuration of a server card's endpoint that owns `port`
/// and the `extra_ports` after it.
///
/// It keeps [`PEERS`] peers and [`SLOTS`] RPCs of the server role, none of
/// the client role, since the driver sends no request, and a pool of
/// [`PAGES`] pages of [`PAGE_LEN`] bytes, a message of [`MAX_MESSAGE`] bytes
/// spanning 16 of them.
#[must_use]
pub const fn server_config(port: u16, extra_ports: u16, seed: Seed) -> Config {
    Config {
        peers: PEERS,
        client_slots: 0,
        server_slots: SLOTS,
        page_len: PAGE_LEN,
        pages: PAGES,
        max_gaps: MAX_GAPS,
        size_buckets: 0,
        first_id: seed.first_id,
        hash_key: seed.hash_key,
        port,
        extra_ports,
    }
}

/// Returns the Network profile for `options`, `HomaModule`'s defaults with
/// the host's share of `max_incoming`.
#[must_use]
pub fn profile(options: &HomaNetwork) -> Profile {
    let mut profile = Profile::network(options.mtu, options.encap());
    profile.max_incoming /= u32::from(options.processes.max(1));
    profile
}

/// Returns the share of the host's link in bits a second that one Homa
/// process of `options` paces its packets to.
#[must_use]
pub fn link_share(options: &HomaNetwork) -> u64 {
    LINK_BITS_PER_S / u64::from(options.processes.max(1))
}
