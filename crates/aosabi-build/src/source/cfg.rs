// SPDX-License-Identifier: MIT
/*
 * aosabi-build: the active set of the build of a crate and the strip of its
 * source over that set, which rustc applies before it compiles a crate.
 *
 * The rules are those of the Rust Reference, chapter Conditional compilation.
 */

//! The active set of a build and the strip of a crate.
//!
//! Rustc compiles of a crate what the configuration of its build keeps. Before
//! it reads the items, it applies every `cfg_attr` whose predicate holds as the
//! attributes it names and drops every item, statement, field, variant,
//! argument, element, arm and parameter whose `cfg` holds no more. The reader
//! strips each file it parses the same way, so the module tree, the bodies and
//! the types it reads are those rustc compiles, and a form a configuration
//! leaves out is no form of the crate.
//!
//! The active set of a build, [`Cfg`], is what `rustc --print cfg` names for the
//! target of the build with the flags cargo passes rustc for the profile of the
//! build and the flags of the build after them, as cargo orders them, which
//! [`Cfg::from_rustc`] asks the rustc of the build and reads from the text it
//! prints, while [`Cfg::parse`] reads a set from a text alone. The flags of the
//! build are those of the compile that builds the binary the card ships: on the
//! host the flags of the environment and then those the build passes, and in the
//! builder stage of the image of the card `docker` the flags of that stage
//! alone, which no flag of the environment reaches, [`Cfg::with_own_flags`].
//! Cargo passes `-C opt-level=3` in the release profile, no flag that changes
//! the set in the dev profile, and `-C panic=abort` where the profile aborts on
//! a panic, and rustc derives the debug assertions from the last `-C opt-level`
//! it takes, so an optimization level among the flags of the environment decides
//! them in the set as it decides them in the build, [`Cfg::profile_flags`]. A
//! profile the configuration of cargo names, by a variable
//! `CARGO_PROFILE_<name>_<key>` or in a configuration file, may set the debug
//! assertions and the panic strategy beyond the profiles the command line
//! writes, so where the build names one the reader refuses at its line a
//! predicate with the key `debug_assertions` or `panic`, with the key named,
//! [`Cfg::with_configured_profile`]. The features cargo enables for each crate
//! join the set as the pairs `feature = "name"`, which cargo hands rustc as
//! `--cfg`. A predicate holds where its pairs stand in that set, through `all`,
//! `any`, `not`, `true` and `false`. A key the set does not name holds only
//! where a flag or a build script sets it, which the builder does not see, so
//! the reader evaluates a key the set names, the key `feature` and a key rustc
//! 1.95.0 or cargo knows on a stable toolchain, which `rustc --print cfg` prints
//! wherever the build sets it, and refuses at its line a predicate with any
//! other key, with the key named.
//!
//! The set names the target of its build as well, and the pairs
//! `rustc --print cfg` prints for that target with the flags of the build alone,
//! the target information over which cargo takes a table
//! `[target.<platform>.dependencies]` of a manifest, where no flag of a profile
//! stands, so `debug_assertions` holds there as rustc derives it from those
//! flags in the release profile as well. A table applies where cargo takes it, a
//! `cfg(..)` whose predicate holds over the pairs of the target information or
//! the triple of the target, [`Cfg::platform_holds`].
//!
//! Rustc removes an expression whose `cfg` holds no more in the positions of a
//! statement, the last statement of a block among them, of an element of an
//! array or a tuple and of an argument of a call or a method call, and in no
//! other, where a `cfg` on an expression fails on stable Rust. So the strip
//! refuses a `cfg` that stands on an expression in any other position at its
//! line. A block, an `if`, a `match` or a loop before a last statement that the
//! strip leaves out becomes the answer of its block, as rustc takes it. The
//! strip reads no tokens of a macro call, whose expansion rustc strips after it
//! expands the call, so the arguments of a macro reach the reader as written.

use std::{
    ffi::OsString,
    path::Path,
    process::Command, //
};

use proc_macro2::Ident;
use syn::{
    Attribute,
    Expr,
    Fields,
    FnArg,
    GenericParam,
    Generics,
    ImplItem,
    Lit,
    Meta,
    Pat,
    Signature,
    Stmt,
    Token,
    TraitItem,
    ext::IdentExt,
    parse::{
        ParseStream,
        Parser, //
    },
    punctuated::Punctuated,
    spanned::Spanned, //
};

use super::{
    Place,
    Refusal, //
};

/// The keys rustc 1.95.0 and cargo know whose pairs `rustc --print cfg` prints
/// wherever a build sets them on a stable toolchain, with `feature`, which cargo
/// sets for each feature it enables, and `test` and `docsrs`, which cargo
/// declares for every crate. The 20 keys of rustc are those of the 31 names
/// `rustc -Zunstable-options --print=check-cfg` of rustc 1.95.0 lists that
/// compile under a `cfg` on a stable toolchain, while the other 11 meet error
/// E0658 there.
const KNOWN: [&str; 23] = [
    "clippy",
    "debug_assertions",
    "doc",
    "docsrs",
    "doctest",
    "feature",
    "miri",
    "panic",
    "proc_macro",
    "rustfmt",
    "target_abi",
    "target_arch",
    "target_endian",
    "target_env",
    "target_family",
    "target_feature",
    "target_has_atomic",
    "target_os",
    "target_pointer_width",
    "target_vendor",
    "test",
    "unix",
    "windows",
];

/// The keys a profile of cargo sets in the active set of a build, through the
/// level of optimization, the debug assertions and the panic strategy.
const PROFILE_KEYS: [&str; 2] = ["debug_assertions", "panic"];

/// The pairs `rustc --print cfg` prints, each a key alone or a key and its
/// value.
type Pairs = Vec<(String, Option<String>)>;

/// The active set of the build of a crate for one target, the pairs `rustc
/// --print cfg` names, each a key alone or a key and its value, the pairs of
/// the target information over which cargo takes the tables of a platform,
/// and the triple of that target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cfg {
    /// The pairs of the compile of a crate, in the order rustc prints them.
    pairs: Pairs,
    /// The pairs rustc prints for the target with the flags of the build
    /// alone and no flag of its profile, the target information of cargo.
    platform: Pairs,
    /// The triple of the target, `None` for a set read from a text alone.
    target: Option<String>,
    /// Where the configuration of cargo names a profile, a variable or a
    /// file, `None` where the build takes the profiles of its workspace.
    configured: Option<String>,
    /// The compile that passes rustc flags of its own alone, which no flag
    /// of the environment of the command line reaches, named for a refusal,
    /// `None` for a compile on the host.
    own: Option<String>,
}

impl Cfg {
    /// Returns the set that the text `text` alone holds, as `rustc --print
    /// cfg` writes it, one pair per line, `unix` or `target_os="linux"`, which
    /// stands for the target information as well, with no call of rustc.
    ///
    /// The set names no target, so [`Cfg::platform_holds`] counts a table of
    /// a triple as one that applies.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let pairs = pairs_of(text);
        Self {
            platform: pairs.clone(),
            pairs,
            target: None,
            configured: None,
            own: None,
        }
    }

    /// Returns the flags cargo passes rustc for the dev profile of cargo, none
    /// that changes the set, or for its release profile, `-C opt-level=3`,
    /// with `-C panic=abort` where `abort` holds, as it passes them for the
    /// release profile of the generated workspace, which aborts on a panic.
    ///
    /// Cargo passes `-C opt-level` where the level of the profile is not 0,
    /// `-C panic` where the profile does not unwind, and `-C debug-assertions`
    /// only where the setting of the profile differs from what rustc derives
    /// from that level, the debug assertions at level 0 alone, which neither of
    /// the two profiles does. So rustc derives the debug assertions of a build
    /// from the last `-C opt-level` it takes, one of the flags of the
    /// environment after these among them.
    #[must_use]
    pub fn profile_flags(release: bool, abort: bool) -> Vec<String> {
        let mut flags = Vec::new();
        if release {
            flags.extend([String::from("-C"), String::from("opt-level=3")]);
        }
        if abort {
            flags.extend([String::from("-C"), String::from("panic=abort")]);
        }
        flags
    }

    /// Returns the arguments of the call of rustc that prints the pairs of the
    /// target `triple`, the host for `None`, with the flags `flags`.
    #[must_use]
    pub fn print_args(triple: Option<&str>, flags: &[String]) -> Vec<String> {
        let mut args = vec![String::from("--print"), String::from("cfg")];
        if let Some(triple) = triple {
            args.push(String::from("--target"));
            args.push(String::from(triple));
        }
        args.extend(flags.iter().cloned());
        args
    }

    /// Returns the active set that rustc prints, called through the factory
    /// `rustc`, for the target `triple`, the host for `None`, with the flags
    /// `profile` cargo passes for the profile of the build and the flags
    /// `flags` of the build after them, as cargo orders them, and the target
    /// information, which it prints with `flags` alone.
    ///
    /// Cargo asks rustc for the target information of a build with the flags of
    /// the build and no flag of its profile, and takes the tables of a platform
    /// over it, so `debug_assertions` holds there as rustc derives it from those
    /// flags alone, in the release profile as well. The set takes `triple` as
    /// its target, and for the host the triple that `--print host-tuple` prints
    /// after the pairs in the same call, which rustc 1.95.0 prints on a stable
    /// toolchain.
    ///
    /// # Errors
    ///
    /// Returns the text of the failure when rustc does not start or ends with
    /// a failure, its first line of standard error among it.
    pub fn from_rustc(
        rustc: &dyn Fn() -> Command,
        triple: Option<&str>,
        profile: &[String],
        flags: &[String],
    ) -> Result<Self, String> {
        let (platform, target) = printed(rustc(), triple, flags)?;
        let pairs = if profile.is_empty() {
            platform.clone()
        } else {
            let all: Vec<String> = profile.iter().chain(flags).cloned().collect();
            printed(rustc(), triple, &all)?.0
        };
        Ok(Self {
            pairs,
            platform,
            target: Some(target),
            configured: None,
            own: None,
        })
    }

    /// Returns the active set of the host in the dev profile, which the rustc
    /// of `RUSTC`, or `rustc` from the path, prints in `cwd`.
    ///
    /// # Errors
    ///
    /// Returns what [`Cfg::from_rustc`] returns.
    pub fn host(cwd: &Path) -> Result<Self, String> {
        let rustc = std::env::var_os("RUSTC")
            .filter(|rustc| !rustc.is_empty())
            .unwrap_or_else(|| OsString::from("rustc"));
        let command = || {
            let mut command = Command::new(&rustc);
            command.current_dir(cwd);
            command
        };
        Self::from_rustc(&command, None, &[], &[])
    }

    /// Returns the set with a profile of its build that the configuration of
    /// cargo names at `origin`, a variable or a file, which may set the debug
    /// assertions and the panic strategy of the build beyond the profiles of
    /// its workspace, so the strip refuses at its line a predicate with the
    /// key `debug_assertions` or `panic`, with the key and `origin` named.
    #[must_use]
    pub fn with_configured_profile(self, origin: String) -> Self {
        Self {
            configured: Some(origin),
            ..self
        }
    }

    /// Returns the set of a compile, `build`, that passes rustc flags of its
    /// own alone, which no flag of the environment of the command line
    /// reaches, as the builder stage of the image of the card `docker` passes
    /// them, so a refusal names no flag of `RUSTFLAGS` as the way to set a
    /// key for the build and names `build` instead.
    #[must_use]
    pub fn with_own_flags(self, build: String) -> Self {
        Self {
            own: Some(build),
            ..self
        }
    }

    /// Returns whether the pairs of the compile hold the pair of the key `key`
    /// and the value `value`, a key alone for `None`.
    fn holds(&self, key: &str, value: Option<&str>) -> bool {
        held(&self.pairs, key, value)
    }

    /// Returns whether the pairs of the compile name the key `key` with any
    /// value.
    fn names(&self, key: &str) -> bool {
        self.pairs.iter().any(|(known, _)| known == key)
    }

    /// Returns whether a table `[target.<platform>.dependencies]` of a
    /// manifest applies to the build of this set, as cargo takes it, with
    /// `platform` read as the key of the table, its quotes left out.
    ///
    /// Cargo takes a table of a `cfg(..)` where its predicate holds over the
    /// pairs of the target information, where the key `feature`, a key those
    /// pairs do not name and a predicate rustc evaluates on no stable toolchain
    /// hold not, and a table of a triple where it names the target of the build.
    /// So `debug_assertions` holds in such a table as rustc derives it from the
    /// flags of the build alone, in the release profile as well, and `test` and
    /// `proc_macro` hold not, the three values cargo warns it does not support
    /// for selecting dependencies. A `cfg(..)` that holds no predicate the
    /// reader reads, which cargo refuses, and a triple beside a set that names
    /// no target count as tables that apply, so the reader reads such a
    /// dependency as it read every table before.
    #[must_use]
    pub fn platform_holds(&self, platform: &str) -> bool {
        let Some(inner) = platform
            .strip_prefix("cfg(")
            .and_then(|rest| rest.strip_suffix(')'))
        else {
            return self
                .target
                .as_deref()
                .is_none_or(|target| target == platform);
        };
        predicate
            .parse_str(inner)
            .map_or(true, |read| self.platform_predicate(&read))
    }

    /// Returns whether the predicate `predicate` of a table of a platform
    /// holds over the pairs of the target information, as cargo evaluates it.
    fn platform_predicate(&self, predicate: &Predicate) -> bool {
        match predicate {
            Predicate::Key(key) => match key.unraw().to_string().as_str() {
                "true" => true,
                "false" => false,
                name => held(&self.platform, name, None),
            },
            Predicate::Pair(key, Some(value)) => {
                let key = key.unraw().to_string();
                key != "feature" && held(&self.platform, &key, Some(value))
            }
            Predicate::Pair(_, None) | Predicate::Other(_) => false,
            Predicate::List(name, inner) => match name.unraw().to_string().as_str() {
                "all" => inner.iter().all(|inner| self.platform_predicate(inner)),
                "any" => inner.iter().any(|inner| self.platform_predicate(inner)),
                _ => inner.len() == 1 && !self.platform_predicate(&inner[0]),
            },
        }
    }
}

/// Returns whether the pairs `pairs` hold the pair of the key `key` and the
/// value `value`, a key alone for `None`.
fn held(pairs: &[(String, Option<String>)], key: &str, value: Option<&str>) -> bool {
    pairs
        .iter()
        .any(|(known, held)| known == key && held.as_deref() == value)
}

/// Returns the pairs the rustc of `command` prints for the target `triple`,
/// the host for `None`, with the flags `flags`, and the triple of that
/// target, `triple` or the one `--print host-tuple` prints after the pairs.
fn printed(
    mut command: Command,
    triple: Option<&str>,
    flags: &[String],
) -> Result<(Pairs, String), String> {
    command.args(Cfg::print_args(triple, flags));
    if triple.is_none() {
        command.args(["--print", "host-tuple"]);
    }
    let output = command
        .output()
        .map_err(|error| format!("rustc --print cfg does not start: {error}"))?;
    if !output.status.success() {
        let text = String::from_utf8_lossy(&output.stderr);
        let first = text.lines().next().unwrap_or_default();
        return Err(format!("rustc --print cfg failed: {first}"));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let Some(triple) = triple else {
        let text = text.trim_end();
        let (pairs, host) = text.rsplit_once('\n').unwrap_or(("", text));
        return Ok((pairs_of(pairs), String::from(host.trim())));
    };
    Ok((pairs_of(&text), String::from(triple)))
}

/// Returns the pairs of the text `text` that `rustc --print cfg` writes, one
/// pair per line.
fn pairs_of(text: &str) -> Pairs {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| match line.split_once('=') {
            Some((key, value)) => (String::from(key.trim()), Some(unquote(value.trim()))),
            None => (String::from(line), None),
        })
        .collect()
}

/// Returns the value `value` of a line of `rustc --print cfg` without its
/// quotes and with its escapes read.
fn unquote(value: &str) -> String {
    let Some(inner) = value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
    else {
        return String::from(value);
    };
    let mut text = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            text.extend(chars.next());
        } else {
            text.push(c);
        }
    }
    text
}

/// The active set of the build of one crate: the set of its target and the
/// features cargo enables for it.
#[derive(Clone, Copy)]
pub(crate) struct Active<'a> {
    /// The set `rustc --print cfg` names for the target of the build.
    pub(crate) cfg: &'a Cfg,
    /// The features cargo enables for the crate.
    pub(crate) features: &'a [String],
}

impl Active<'_> {
    /// Returns whether the key `key`, with the value `value` or alone, stands
    /// in the set, or `None` for a key the reader cannot evaluate.
    fn has(&self, key: &str, value: Option<&str>) -> Option<bool> {
        if key == "feature"
            && let Some(name) = value
            && self.features.iter().any(|feature| feature == name)
        {
            return Some(true);
        }
        let held = self.cfg.holds(key, value);
        (held || self.cfg.names(key) || KNOWN.contains(&key)).then_some(held)
    }
}

/// Strips the file `parsed`, which stands at `file` relative to the package,
/// over the active set `active`, as rustc strips a crate, and adds every form
/// it refuses to `refusals`.
///
/// Returns whether the inner attributes of the file keep it, which a `cfg`
/// among them that holds no more does not, so a crate root or a module whose
/// file the strip does not keep holds no item.
pub(crate) fn file(
    parsed: &mut syn::File,
    active: Active<'_>,
    file: &Path,
    refusals: &mut Vec<Refusal>,
) -> bool {
    let mut strip = Strip {
        active,
        file,
        refusals,
    };
    if !strip.attrs(&mut parsed.attrs) {
        return false;
    }
    strip.items(&mut parsed.items);
    true
}

/// The strip of one file.
struct Strip<'a> {
    /// The active set of the crate of the file.
    active: Active<'a>,
    /// The file, relative to the package.
    file: &'a Path,
    /// The forms the strip refuses, in the order it meets them.
    refusals: &'a mut Vec<Refusal>,
}

/// One predicate of a `cfg` or a `cfg_attr`, as rustc reads it.
enum Predicate {
    /// A key alone, as `unix`, or `true` or `false`.
    Key(Ident),
    /// A key and its value, as `target_os = "linux"`, `None` for a literal
    /// that is no string, which rustc refuses.
    Pair(Ident, Option<String>),
    /// `all(..)`, `any(..)` or `not(..)` of predicates.
    List(Ident, Vec<Predicate>),
    /// Any other name with arguments, as `version(..)`, which rustc evaluates
    /// on no stable toolchain.
    Other(Ident),
}

/// The predicate of a `cfg_attr` and the attributes it names after it.
type CfgAttr = (Predicate, Punctuated<Meta, Token![,]>);

impl Strip<'_> {
    /// Adds the refusal `message` at the line of `span`.
    fn refuse(&mut self, span: proc_macro2::Span, message: String) {
        self.refusals.push(Refusal {
            place: Place {
                file: self.file.to_path_buf(),
                line: span.start().line,
            },
            message,
        });
    }

    /// Applies every `cfg_attr` of `attrs` whose predicate holds as the
    /// attributes it names, in their place, drops every `cfg_attr` whose
    /// predicate does not hold and every `cfg`, and returns whether every
    /// `cfg` holds, which keeps what the attributes stand on.
    fn attrs(&mut self, attrs: &mut Vec<Attribute>) -> bool {
        let mut pending: Vec<Attribute> = attrs.drain(..).rev().collect();
        let mut kept = true;
        while let Some(attr) = pending.pop() {
            if attr.path().is_ident("cfg_attr") {
                match cfg_attr(&attr) {
                    Ok((predicate, named)) => {
                        if self.holds(&predicate) {
                            for meta in named.into_iter().rev() {
                                pending.push(Attribute {
                                    meta,
                                    ..attr.clone()
                                });
                            }
                        }
                    }
                    Err(()) => self.malformed(attr.span(), "cfg_attr"),
                }
            } else if attr.path().is_ident("cfg") {
                match &attr.meta {
                    Meta::List(list) => match predicate.parse2(list.tokens.clone()) {
                        Ok(predicate) => kept &= self.holds(&predicate),
                        Err(_) => self.malformed(attr.span(), "cfg"),
                    },
                    _ => self.malformed(attr.span(), "cfg"),
                }
            } else {
                attrs.push(attr);
            }
        }
        kept
    }

    /// Refuses a `cfg` or a `cfg_attr` on an expression in a position where
    /// rustc removes none, and strips its attributes.
    fn fixed(&mut self, attrs: &mut Vec<Attribute>) {
        let configured = attrs
            .iter()
            .find(|attr| attr.path().is_ident("cfg") || attr.path().is_ident("cfg_attr"));
        if let Some(attr) = configured {
            self.refuse(
                attr.span(),
                String::from(
                    "a `cfg` stands on an expression in a position where rustc removes none, which stable Rust refuses as an attribute on an expression, so the builder cannot read what the build compiles. Put the `cfg` on the statement, on an element of an array or a tuple, on an argument of a call, on a field or on an arm",
                ),
            );
        }
        self.attrs(attrs);
    }

    /// Refuses at the line of `span` an attribute `name` whose arguments are
    /// no predicate the reader evaluates.
    fn malformed(&mut self, span: proc_macro2::Span, name: &str) {
        self.refuse(
            span,
            format!(
                "the `{name}` holds no predicate the builder evaluates as rustc does, a key, a key and a string, `true`, `false`, or `all(..)`, `any(..)` and `not(..)` of predicates, so the builder cannot tell whether rustc compiles what the attribute stands on. Write the predicate in one of these forms"
            ),
        );
    }

    /// Returns whether the predicate `predicate` holds over the active set,
    /// refusing at its line every key the reader cannot evaluate and every
    /// form rustc evaluates on no stable toolchain, which then holds as rustc
    /// holds a key the build does not set.
    fn holds(&mut self, predicate: &Predicate) -> bool {
        match predicate {
            Predicate::Key(key) => match key.unraw().to_string().as_str() {
                "true" => true,
                "false" => false,
                name => self.key(name, None, key.span()),
            },
            Predicate::Pair(key, Some(value)) => {
                self.key(&key.unraw().to_string(), Some(value), key.span())
            }
            Predicate::Pair(key, None) => {
                self.malformed(key.span(), "cfg");
                false
            }
            Predicate::List(name, inner) => {
                // Every predicate is evaluated, so every key it names is
                // refused where the reader cannot evaluate it.
                let held: Vec<bool> = inner.iter().map(|inner| self.holds(inner)).collect();
                match name.unraw().to_string().as_str() {
                    "all" => held.iter().all(|held| *held),
                    "any" => held.iter().any(|held| *held),
                    _ if held.len() == 1 => !held[0],
                    _ => {
                        self.malformed(name.span(), "cfg");
                        false
                    }
                }
            }
            Predicate::Other(name) => {
                self.refuse(
                    name.span(),
                    format!(
                        "the `cfg` holds the predicate `{name}(..)`, which rustc evaluates on no stable toolchain, so the builder cannot tell whether rustc compiles what the attribute stands on. Write the predicate with `all`, `any`, `not`, `true`, `false` and the keys of the build"
                    ),
                );
                false
            }
        }
    }

    /// Returns whether the key `key` with the value `value`, or alone,
    /// stands in the active set, refusing at the line of `span` a key the
    /// reader cannot evaluate, a key of the profile among them where the
    /// configuration of cargo names a profile of the build. A refusal names
    /// a flag of `RUSTFLAGS` as the way to set the key for a compile on the
    /// host alone, since no such flag reaches a compile with flags of its
    /// own.
    fn key(&mut self, key: &str, value: Option<&str>, span: proc_macro2::Span) -> bool {
        let own = self.active.cfg.own.as_deref();
        if let Some(origin) = &self.active.cfg.configured
            && PROFILE_KEYS.contains(&key)
        {
            let advice = match own {
                None => String::from(
                    "Leave the profiles to the command line and set the key for the build with a flag of `RUSTFLAGS`, as `-C debug-assertions=on`, which the builder reads",
                ),
                Some(build) => format!(
                    "Leave the profiles to the command line, since {build} compiles with flags of its own, which no flag of `RUSTFLAGS` reaches"
                ),
            };
            let message = format!(
                "the `cfg` names the key `{key}`, which the profile of the build sets, and {origin} names a profile of cargo, which the builder does not read, so the builder cannot tell whether rustc compiles what the attribute stands on. {advice}"
            );
            self.refuse(span, message);
            return false;
        }
        if let Some(held) = self.active.has(key, value) {
            return held;
        }
        let advice = match own {
            None => format!(", or set the key for every build with `--cfg {key}` in `RUSTFLAGS`"),
            Some(build) => format!(
                ", since {build} compiles with flags of its own, which no flag of `RUSTFLAGS` reaches"
            ),
        };
        self.refuse(
            span,
            format!(
                "the `cfg` names the key `{key}`, which neither `rustc --print cfg` names for the target of the build nor a feature of the crate sets, and which is no key rustc or cargo knows on a stable toolchain, so the builder cannot tell whether rustc compiles what the attribute stands on, since a build script or a flag may set it. Write the predicate over the keys of the build, as `cfg(unix)` or `cfg(feature = \"name\")`{advice}"
            ),
        );
        false
    }

    /// Keeps of `items` what the active set keeps, each stripped.
    fn items(&mut self, items: &mut Vec<syn::Item>) {
        items.retain_mut(|item| self.item(item));
    }

    /// Strips the item `item` and returns whether the active set keeps it.
    fn item(&mut self, item: &mut syn::Item) -> bool {
        let Some(attrs) = item_attrs(item) else {
            return true;
        };
        if !self.attrs(attrs) {
            return false;
        }
        match item {
            syn::Item::Const(item) => self.expr(&mut item.expr),
            syn::Item::Static(item) => self.expr(&mut item.expr),
            syn::Item::Enum(item) => {
                self.generics(&mut item.generics);
                retain(&mut item.variants, |variant| {
                    if !self.attrs(&mut variant.attrs) {
                        return false;
                    }
                    self.fields(&mut variant.fields);
                    if let Some((_, discriminant)) = &mut variant.discriminant {
                        self.expr(discriminant);
                    }
                    true
                });
            }
            syn::Item::Struct(item) => {
                self.generics(&mut item.generics);
                self.fields(&mut item.fields);
            }
            syn::Item::Union(item) => {
                self.generics(&mut item.generics);
                retain(&mut item.fields.named, |field| self.attrs(&mut field.attrs));
            }
            syn::Item::Fn(item) => {
                self.signature(&mut item.sig);
                self.block(&mut item.block);
            }
            syn::Item::Impl(item) => {
                self.generics(&mut item.generics);
                item.items.retain_mut(|member| self.impl_item(member));
            }
            syn::Item::Trait(item) => {
                self.generics(&mut item.generics);
                item.items.retain_mut(|member| self.trait_item(member));
            }
            syn::Item::TraitAlias(item) => self.generics(&mut item.generics),
            syn::Item::Type(item) => self.generics(&mut item.generics),
            syn::Item::ForeignMod(block) => block
                .items
                .retain_mut(|foreign| foreign_attrs(foreign).is_none_or(|attrs| self.attrs(attrs))),
            syn::Item::Mod(module) => {
                if let Some((_, items)) = &mut module.content {
                    self.items(items);
                }
            }
            _ => {}
        }
        true
    }

    /// Strips the member `member` of an impl block and returns whether the
    /// active set keeps it.
    fn impl_item(&mut self, member: &mut ImplItem) -> bool {
        match member {
            ImplItem::Const(member) => {
                self.attrs(&mut member.attrs) && {
                    self.generics(&mut member.generics);
                    self.expr(&mut member.expr);
                    true
                }
            }
            ImplItem::Fn(member) => {
                self.attrs(&mut member.attrs) && {
                    self.signature(&mut member.sig);
                    self.block(&mut member.block);
                    true
                }
            }
            ImplItem::Type(member) => {
                self.attrs(&mut member.attrs) && {
                    self.generics(&mut member.generics);
                    true
                }
            }
            ImplItem::Macro(member) => self.attrs(&mut member.attrs),
            _ => true,
        }
    }

    /// Strips the member `member` of a trait and returns whether the active
    /// set keeps it.
    fn trait_item(&mut self, member: &mut TraitItem) -> bool {
        match member {
            TraitItem::Const(member) => {
                self.attrs(&mut member.attrs) && {
                    self.generics(&mut member.generics);
                    if let Some((_, value)) = &mut member.default {
                        self.expr(value);
                    }
                    true
                }
            }
            TraitItem::Fn(member) => {
                self.attrs(&mut member.attrs) && {
                    self.signature(&mut member.sig);
                    if let Some(block) = &mut member.default {
                        self.block(block);
                    }
                    true
                }
            }
            TraitItem::Type(member) => {
                self.attrs(&mut member.attrs) && {
                    self.generics(&mut member.generics);
                    true
                }
            }
            TraitItem::Macro(member) => self.attrs(&mut member.attrs),
            _ => true,
        }
    }

    /// Keeps of the fields `fields` what the active set keeps.
    fn fields(&mut self, fields: &mut Fields) {
        match fields {
            Fields::Named(named) => retain(&mut named.named, |field| self.attrs(&mut field.attrs)),
            Fields::Unnamed(unnamed) => {
                retain(&mut unnamed.unnamed, |field| self.attrs(&mut field.attrs));
            }
            Fields::Unit => {}
        }
    }

    /// Keeps of the parameters of `generics` what the active set keeps.
    fn generics(&mut self, generics: &mut Generics) {
        retain(&mut generics.params, |param| match param {
            GenericParam::Lifetime(param) => self.attrs(&mut param.attrs),
            GenericParam::Type(param) => self.attrs(&mut param.attrs),
            GenericParam::Const(param) => self.attrs(&mut param.attrs),
        });
    }

    /// Keeps of the parameters and the generics of `sig` what the active set
    /// keeps.
    fn signature(&mut self, sig: &mut Signature) {
        self.generics(&mut sig.generics);
        retain(&mut sig.inputs, |input| match input {
            FnArg::Receiver(receiver) => self.attrs(&mut receiver.attrs),
            FnArg::Typed(typed) => {
                self.attrs(&mut typed.attrs) && {
                    self.pat(&mut typed.pat);
                    true
                }
            }
        });
    }

    /// Keeps of the statements of `block` what the active set keeps, each
    /// stripped, so a block statement before a last statement the strip
    /// leaves out answers the block, as rustc takes it.
    fn block(&mut self, block: &mut syn::Block) {
        block
            .stmts
            .retain_mut(|statement| self.statement(statement));
    }

    /// Strips the statement `statement` and returns whether the active set
    /// keeps it.
    ///
    /// The attributes of a statement that is an expression stand on that
    /// expression, and on the left operand of an assignment, a binary
    /// operation or a cast, as `syn` places them and as rustc reads them, so
    /// a `cfg` there is one on an expression rustc removes in no position.
    fn statement(&mut self, statement: &mut Stmt) -> bool {
        match statement {
            Stmt::Local(local) => {
                if !self.attrs(&mut local.attrs) {
                    return false;
                }
                self.pat(&mut local.pat);
                if let Some(init) = &mut local.init {
                    self.expr(&mut init.expr);
                    if let Some((_, diverge)) = &mut init.diverge {
                        self.expr(diverge);
                    }
                }
                true
            }
            Stmt::Item(item) => self.item(item),
            Stmt::Expr(expr, _) => self.element(expr),
            Stmt::Macro(call) => self.attrs(&mut call.attrs),
        }
    }

    /// Strips the expression `expr` in a position where rustc removes it,
    /// a statement or an element of a list, and returns whether the active
    /// set keeps it.
    fn element(&mut self, expr: &mut Expr) -> bool {
        if let Some(attrs) = expr_attrs(expr)
            && !self.attrs(attrs)
        {
            return false;
        }
        self.inner(expr);
        true
    }

    /// Strips the expression `expr` in a position where rustc removes none,
    /// refusing a `cfg` that stands on it.
    fn expr(&mut self, expr: &mut Expr) {
        if let Some(attrs) = expr_attrs(expr) {
            self.fixed(attrs);
        }
        self.inner(expr);
    }

    /// Strips what the expression `expr` holds: the elements of a list, the
    /// fields of a literal, the arms of a `match` and the parameters of a
    /// closure the active set keeps, each stripped, and every other part it
    /// holds, [`Strip::parts`].
    fn inner(&mut self, expr: &mut Expr) {
        match expr {
            Expr::Array(array) => retain(&mut array.elems, |elem| self.element(elem)),
            Expr::Tuple(tuple) => retain(&mut tuple.elems, |elem| self.element(elem)),
            Expr::Call(call) => {
                self.expr(&mut call.func);
                retain(&mut call.args, |arg| self.element(arg));
            }
            Expr::MethodCall(call) => {
                self.expr(&mut call.receiver);
                retain(&mut call.args, |arg| self.element(arg));
            }
            Expr::Struct(literal) => {
                retain(&mut literal.fields, |field| {
                    self.attrs(&mut field.attrs) && {
                        self.expr(&mut field.expr);
                        true
                    }
                });
                if let Some(rest) = &mut literal.rest {
                    self.expr(rest);
                }
            }
            Expr::Match(branch) => {
                self.expr(&mut branch.expr);
                branch.arms.retain_mut(|arm| {
                    if !self.attrs(&mut arm.attrs) {
                        return false;
                    }
                    self.pat(&mut arm.pat);
                    if let Some((_, guard)) = &mut arm.guard {
                        self.expr(guard);
                    }
                    self.expr(&mut arm.body);
                    true
                });
            }
            Expr::Closure(closure) => {
                retain(&mut closure.inputs, |input| {
                    pat_attrs(input).is_none_or(|attrs| self.attrs(attrs)) && {
                        self.pat(input);
                        true
                    }
                });
                self.expr(&mut closure.body);
            }
            _ => self.parts(expr),
        }
    }

    /// Strips every block and every operand the expression `expr` holds, in
    /// positions where rustc removes no expression.
    fn parts(&mut self, expr: &mut Expr) {
        match expr {
            Expr::Block(inner) => self.block(&mut inner.block),
            Expr::Async(inner) => self.block(&mut inner.block),
            Expr::Const(inner) => self.block(&mut inner.block),
            Expr::Unsafe(inner) => self.block(&mut inner.block),
            Expr::TryBlock(inner) => self.block(&mut inner.block),
            Expr::Loop(inner) => self.block(&mut inner.body),
            Expr::While(inner) => {
                self.expr(&mut inner.cond);
                self.block(&mut inner.body);
            }
            Expr::ForLoop(inner) => {
                self.pat(&mut inner.pat);
                self.expr(&mut inner.expr);
                self.block(&mut inner.body);
            }
            Expr::If(branch) => {
                self.expr(&mut branch.cond);
                self.block(&mut branch.then_branch);
                if let Some((_, otherwise)) = &mut branch.else_branch {
                    self.expr(otherwise);
                }
            }
            Expr::Let(binding) => {
                self.pat(&mut binding.pat);
                self.expr(&mut binding.expr);
            }
            Expr::Assign(assign) => {
                self.expr(&mut assign.left);
                self.expr(&mut assign.right);
            }
            Expr::Binary(binary) => {
                self.expr(&mut binary.left);
                self.expr(&mut binary.right);
            }
            Expr::Index(index) => {
                self.expr(&mut index.expr);
                self.expr(&mut index.index);
            }
            Expr::Repeat(repeat) => {
                self.expr(&mut repeat.expr);
                self.expr(&mut repeat.len);
            }
            Expr::Range(range) => {
                if let Some(start) = &mut range.start {
                    self.expr(start);
                }
                if let Some(end) = &mut range.end {
                    self.expr(end);
                }
            }
            Expr::Return(answer) => {
                if let Some(value) = &mut answer.expr {
                    self.expr(value);
                }
            }
            Expr::Break(jump) => {
                if let Some(value) = &mut jump.expr {
                    self.expr(value);
                }
            }
            Expr::Yield(inner) => {
                if let Some(value) = &mut inner.expr {
                    self.expr(value);
                }
            }
            Expr::Await(inner) => self.expr(&mut inner.base),
            Expr::Field(inner) => self.expr(&mut inner.base),
            Expr::Cast(inner) => self.expr(&mut inner.expr),
            Expr::Group(inner) => self.expr(&mut inner.expr),
            Expr::Paren(inner) => self.expr(&mut inner.expr),
            Expr::RawAddr(inner) => self.expr(&mut inner.expr),
            Expr::Reference(inner) => self.expr(&mut inner.expr),
            Expr::Try(inner) => self.expr(&mut inner.expr),
            Expr::Unary(inner) => self.expr(&mut inner.expr),
            _ => {}
        }
    }

    /// Keeps of the fields of the struct patterns of `pat` what the active
    /// set keeps.
    fn pat(&mut self, pat: &mut Pat) {
        match pat {
            Pat::Struct(fields) => retain(&mut fields.fields, |field| {
                self.attrs(&mut field.attrs) && {
                    self.pat(&mut field.pat);
                    true
                }
            }),
            Pat::Ident(ident) => {
                if let Some((_, inner)) = &mut ident.subpat {
                    self.pat(inner);
                }
            }
            Pat::Or(or) => or.cases.iter_mut().for_each(|case| self.pat(case)),
            Pat::Paren(inner) => self.pat(&mut inner.pat),
            Pat::Reference(inner) => self.pat(&mut inner.pat),
            Pat::Slice(slice) => slice.elems.iter_mut().for_each(|elem| self.pat(elem)),
            Pat::Tuple(tuple) => tuple.elems.iter_mut().for_each(|elem| self.pat(elem)),
            Pat::TupleStruct(tuple) => tuple.elems.iter_mut().for_each(|elem| self.pat(elem)),
            Pat::Type(typed) => self.pat(&mut typed.pat),
            _ => {}
        }
    }
}

/// Returns the predicate and the attributes of the `cfg_attr` `attr`, or
/// `Err` where its arguments are no predicate and attributes.
fn cfg_attr(attr: &Attribute) -> Result<CfgAttr, ()> {
    let Meta::List(list) = &attr.meta else {
        return Err(());
    };
    let parser = |input: ParseStream<'_>| {
        let read = predicate(input)?;
        input.parse::<Token![,]>()?;
        let named = Punctuated::<Meta, Token![,]>::parse_terminated(input)?;
        Ok((read, named))
    };
    parser.parse2(list.tokens.clone()).map_err(|_| ())
}

/// Reads one predicate of a `cfg` from `input`: a key, which may be a keyword
/// as `true` and `false` are, a key and a literal, or a name and a list in
/// parentheses, whose predicates it reads for `all`, `any` and `not` alone.
fn predicate(input: ParseStream<'_>) -> syn::Result<Predicate> {
    let key = input.call(Ident::parse_any)?;
    if input.peek(syn::token::Paren) {
        let content;
        syn::parenthesized!(content in input);
        if !matches!(key.unraw().to_string().as_str(), "all" | "any" | "not") {
            content.parse::<proc_macro2::TokenStream>()?;
            return Ok(Predicate::Other(key));
        }
        let inner = Punctuated::<Predicate, Token![,]>::parse_terminated_with(&content, predicate)?;
        return Ok(Predicate::List(key, inner.into_iter().collect()));
    }
    if input.peek(Token![=]) {
        input.parse::<Token![=]>()?;
        let value = match input.parse::<Lit>()? {
            Lit::Str(value) => Some(value.value()),
            _ => None,
        };
        return Ok(Predicate::Pair(key, value));
    }
    Ok(Predicate::Key(key))
}

/// Keeps of `list` the elements `keep` answers true for, with the marks
/// between them, so a list that loses its last element keeps a mark after
/// the new last one, as a trailing comma.
fn retain<T, P>(list: &mut Punctuated<T, P>, mut keep: impl FnMut(&mut T) -> bool) {
    let mut kept = Punctuated::new();
    for pair in std::mem::take(list).into_pairs() {
        let (mut value, mark) = pair.into_tuple();
        if keep(&mut value) {
            kept.push_value(value);
            if let Some(mark) = mark {
                kept.push_punct(mark);
            }
        }
    }
    *list = kept;
}

/// Returns the attributes of the item `item`, `None` for an item `syn` keeps
/// as tokens.
fn item_attrs(item: &mut syn::Item) -> Option<&mut Vec<Attribute>> {
    Some(match item {
        syn::Item::Const(item) => &mut item.attrs,
        syn::Item::Enum(item) => &mut item.attrs,
        syn::Item::ExternCrate(item) => &mut item.attrs,
        syn::Item::Fn(item) => &mut item.attrs,
        syn::Item::ForeignMod(item) => &mut item.attrs,
        syn::Item::Impl(item) => &mut item.attrs,
        syn::Item::Macro(item) => &mut item.attrs,
        syn::Item::Mod(item) => &mut item.attrs,
        syn::Item::Static(item) => &mut item.attrs,
        syn::Item::Struct(item) => &mut item.attrs,
        syn::Item::Trait(item) => &mut item.attrs,
        syn::Item::TraitAlias(item) => &mut item.attrs,
        syn::Item::Type(item) => &mut item.attrs,
        syn::Item::Union(item) => &mut item.attrs,
        syn::Item::Use(item) => &mut item.attrs,
        _ => return None,
    })
}

/// Returns the attributes of the item `item` of an `extern` block.
fn foreign_attrs(item: &mut syn::ForeignItem) -> Option<&mut Vec<Attribute>> {
    Some(match item {
        syn::ForeignItem::Fn(item) => &mut item.attrs,
        syn::ForeignItem::Static(item) => &mut item.attrs,
        syn::ForeignItem::Type(item) => &mut item.attrs,
        syn::ForeignItem::Macro(item) => &mut item.attrs,
        _ => return None,
    })
}

/// Returns the attributes of the expression `expr`, `None` for one `syn`
/// keeps as tokens.
fn expr_attrs(expr: &mut Expr) -> Option<&mut Vec<Attribute>> {
    Some(match expr {
        Expr::Array(expr) => &mut expr.attrs,
        Expr::Assign(expr) => &mut expr.attrs,
        Expr::Async(expr) => &mut expr.attrs,
        Expr::Await(expr) => &mut expr.attrs,
        Expr::Binary(expr) => &mut expr.attrs,
        Expr::Block(expr) => &mut expr.attrs,
        Expr::Break(expr) => &mut expr.attrs,
        Expr::Call(expr) => &mut expr.attrs,
        Expr::Cast(expr) => &mut expr.attrs,
        Expr::Closure(expr) => &mut expr.attrs,
        Expr::Const(expr) => &mut expr.attrs,
        Expr::Continue(expr) => &mut expr.attrs,
        Expr::Field(expr) => &mut expr.attrs,
        Expr::ForLoop(expr) => &mut expr.attrs,
        Expr::Group(expr) => &mut expr.attrs,
        Expr::If(expr) => &mut expr.attrs,
        Expr::Index(expr) => &mut expr.attrs,
        Expr::Infer(expr) => &mut expr.attrs,
        Expr::Let(expr) => &mut expr.attrs,
        Expr::Lit(expr) => &mut expr.attrs,
        Expr::Loop(expr) => &mut expr.attrs,
        Expr::Macro(expr) => &mut expr.attrs,
        Expr::Match(expr) => &mut expr.attrs,
        Expr::MethodCall(expr) => &mut expr.attrs,
        Expr::Paren(expr) => &mut expr.attrs,
        Expr::Path(expr) => &mut expr.attrs,
        Expr::Range(expr) => &mut expr.attrs,
        Expr::RawAddr(expr) => &mut expr.attrs,
        Expr::Reference(expr) => &mut expr.attrs,
        Expr::Repeat(expr) => &mut expr.attrs,
        Expr::Return(expr) => &mut expr.attrs,
        Expr::Struct(expr) => &mut expr.attrs,
        Expr::Try(expr) => &mut expr.attrs,
        Expr::TryBlock(expr) => &mut expr.attrs,
        Expr::Tuple(expr) => &mut expr.attrs,
        Expr::Unary(expr) => &mut expr.attrs,
        Expr::Unsafe(expr) => &mut expr.attrs,
        Expr::While(expr) => &mut expr.attrs,
        Expr::Yield(expr) => &mut expr.attrs,
        _ => return None,
    })
}

/// Returns the attributes of the pattern `pat`, `None` for one `syn` keeps as
/// tokens.
fn pat_attrs(pat: &mut Pat) -> Option<&mut Vec<Attribute>> {
    Some(match pat {
        Pat::Const(pat) => &mut pat.attrs,
        Pat::Ident(pat) => &mut pat.attrs,
        Pat::Lit(pat) => &mut pat.attrs,
        Pat::Macro(pat) => &mut pat.attrs,
        Pat::Or(pat) => &mut pat.attrs,
        Pat::Paren(pat) => &mut pat.attrs,
        Pat::Path(pat) => &mut pat.attrs,
        Pat::Range(pat) => &mut pat.attrs,
        Pat::Reference(pat) => &mut pat.attrs,
        Pat::Rest(pat) => &mut pat.attrs,
        Pat::Slice(pat) => &mut pat.attrs,
        Pat::Struct(pat) => &mut pat.attrs,
        Pat::Tuple(pat) => &mut pat.attrs,
        Pat::TupleStruct(pat) => &mut pat.attrs,
        Pat::Type(pat) => &mut pat.attrs,
        Pat::Wild(pat) => &mut pat.attrs,
        _ => return None,
    })
}
