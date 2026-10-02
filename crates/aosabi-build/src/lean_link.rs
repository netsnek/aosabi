// SPDX-License-Identifier: MIT
/*
 * aosabi-build: the lean link of a Linux card, which leaves the unwind tables
 * out of a served binary that aborts on a panic.
 */

//! The lean link of a Linux card.
//!
//! A served binary of a Linux card aborts on a panic and never unwinds, so its
//! unwind tables `.eh_frame`, `.eh_frame_hdr` and `.gcc_except_table` serve
//! only the stack trace of a panic, which a shipped card then does not print,
//! as a Worker and a microcontroller already do not. In the lead's check of
//! that plan the lean link took 196,840 bytes of file and 198,060 loaded bytes
//! off the served twin `errors-app-server` in the size profile.
//!
//! [`lean_link`] is the call of the build script of a server package that
//! takes them out. It writes a linker script fragment of four lines,
//! [`DISCARD_UNWIND_FILE`], into `OUT_DIR`, which discards the three sections
//! and is inserted after `.text` into the default script of the linker, and it
//! asks for no `.eh_frame_hdr` and no `PT_GNU_EH_FRAME` header. The link
//! arguments reach the binaries of the package alone, so the build scripts and
//! the proc macros of the build keep their tables.
//!
//! The Linux GNU pillar ships two cards, `server` with packed relative
//! relocations, `DT_RELR`, for a loader of glibc 2.36 or newer, and
//! `server-legacy` without them for older loaders, the two otherwise the same
//! build. [`LinuxCard`] names them, and the lean link of `server` adds the
//! packed relocations as well.
//!
//! At `gnu`, its default, the compiler driver links with the linker it finds
//! and the switch adds nothing. At `lld` the build links with the LLD of the
//! toolchain that compiles it, which the sysroot of a rustup toolchain, the pin
//! and its private copy among them, carries under
//! `lib/rustlib/<host>/bin/gcc-ld`, so no tool joins the build host, and a
//! sysroot without `ld.lld` there ends the build with `cargo::error`. At `lld`
//! the link folds identical code with `--icf=all`. Identical code folding gives
//! functions of equal machine code one address. LLD keeps the address equality
//! of data unless a link asks for `--ignore-data-address-equality`, so the fold
//! removes code alone, and no crate that ships compares the address of a
//! function.
//!
//! The switch reaches `aarch64-unknown-linux-gnu` alone, the target whose folds
//! section Folds lists and whose cards `20_exec_cost` and `20_exec_asm` read.
//! The lean link of `x86_64-unknown-linux-gnu` links with the linker the
//! compiler driver finds whatever the switch says, since no listing reads the
//! folds of that target, and the cross compiler driver of the project's machine
//! runs GNU ld for `-fuse-ld=lld`, which knows no `--icf=all`. The part `x86`
//! of `tests/notebooks/17_framework_size.ipynb` prints the arguments of that
//! lean link with the switch at `lld`.
//!
//! The judging notebooks are `tests/notebooks/17_framework_size.ipynb`, which
//! builds every aarch64 row of its sections 1 and 2 in the plain and in the
//! lean link, the rows of `x86_64` in its part `x86`, and prints per card the
//! bytes the switch holds back, `tests/notebooks/18_size_oracles.ipynb`, whose
//! section Folds lists and classifies every section the fold removes, and
//! `tests/notebooks/31_http_errors.ipynb`, which answers its corpus through the
//! lean link of the twins.

use std::{
    env,
    path::{
        Path,
        PathBuf, //
    },
    process::Command,
};

use crate::{
    error::BuildError,
    update_file_if_changed::update_file_if_changed, //
};

/// The environment variable that names the Linux card of a lean link in place
/// of the card the build script names, `server` or `server-legacy`.
///
/// It is the route of a builder that links one package as both cards, as
/// `17_framework_size` does, and an empty value counts as unset.
pub const LINUX_CARD_ENV: &str = "AOSABI_LINUX_CARD";

/// The environment variable that picks the linker of the lean link of
/// `aarch64-unknown-linux-gnu`, `gnu` or `lld`.
///
/// An empty value counts as unset, and unset is `gnu`, the linker the
/// compiler driver finds. The lean link of `x86_64-unknown-linux-gnu` links
/// with that linker whatever the value, and a value that names no linker
/// fails the build for every target. The build script runs again when it
/// changes.
pub const LINKER_ENV: &str = "AOSABI_LINKER";

/// The file name of the linker script fragment in `OUT_DIR`.
pub const DISCARD_UNWIND_FILE: &str = "discard-unwind.ld";

/// The linker script fragment that discards the unwind tables.
///
/// `INSERT AFTER .text` makes the fragment augment the default script of the
/// linker instead of replacing it, so every other section keeps its place.
const DISCARD_UNWIND: &str = "SECTIONS {
  /DISCARD/ : { *(.eh_frame) *(.eh_frame_hdr) *(.gcc_except_table*) }
}
INSERT AFTER .text;
";

/// The targets whose binaries the lean link takes.
const LEAN_TARGETS: [&str; 2] = ["aarch64-unknown-linux-gnu", "x86_64-unknown-linux-gnu"];

/// The one target whose lean link the switch [`LINKER_ENV`] reaches.
const LLD_TARGET: &str = "aarch64-unknown-linux-gnu";

/// The link argument that drops the `.eh_frame_hdr` section and the
/// `PT_GNU_EH_FRAME` header the compiler driver asks the linker for.
const NO_EH_FRAME_HDR: &str = "-Wl,--no-eh-frame-hdr";

/// The link argument that packs the relative relocations as `DT_RELR`.
const PACK_RELATIVE_RELOCS: &str = "-Wl,-z,pack-relative-relocs";

/// The link argument that has the compiler driver run the linker `ld.lld` of
/// the directory its `-B` names.
const FUSE_LLD: &str = "-fuse-ld=lld";

/// The link argument that folds every section of identical contents.
const ICF_ALL: &str = "-Wl,--icf=all";

/// The directory under the sysroot of a toolchain that holds the linkers the
/// compiler driver runs, `ld.lld` among them, below `lib/rustlib/<host>/bin/`.
const GCC_LD: &str = "gcc-ld";

/// The symbols of an aarch64 link whose sections stay apart under identical
/// code folding.
///
/// They are the outline atomics `20_exec_cost` reads by name for its theorem
/// T6, global symbols of the objects of `compiler_builtins`.
const AARCH64_KEEP_UNIQUE: [&str; 4] = [
    "__aarch64_ldadd8_relax",
    "__aarch64_ldadd8_acq",
    "__aarch64_ldadd8_rel",
    "__aarch64_ldadd8_acq_rel",
];

/// Names the linker of a lean link.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Linker {
    /// The linker the compiler driver finds, GNU ld on the machines and in
    /// the builder image of the project, which folds nothing.
    Gnu,
    /// The LLD of the toolchain that compiles the build, with identical code
    /// folding.
    Lld,
}

impl Linker {
    /// Returns the name of the linker, `gnu` or `lld`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Gnu => "gnu",
            Self::Lld => "lld",
        }
    }

    /// Returns the linker whose name is `name`, or `None` for a name of no
    /// linker.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        [Self::Gnu, Self::Lld]
            .into_iter()
            .find(|linker| linker.name() == name)
    }
}

/// Names a Linux GNU card of the AI Services pillar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinuxCard {
    /// The card `server`, linked with packed relative relocations, `DT_RELR`,
    /// which a loader of glibc 2.36 or newer takes.
    Server,
    /// The card `server-legacy`, linked without packed relocations, for a
    /// loader older than glibc 2.36.
    ServerLegacy,
}

impl LinuxCard {
    /// Returns the name of the card, `server` or `server-legacy`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Server => "server",
            Self::ServerLegacy => "server-legacy",
        }
    }

    /// Returns the card whose name is `name`, or `None` for a name of no card.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        [Self::Server, Self::ServerLegacy]
            .into_iter()
            .find(|card| card.name() == name)
    }

    /// Returns the link arguments the card adds to the lean link.
    const fn link_args(self) -> &'static [&'static str] {
        match self {
            Self::Server => &[PACK_RELATIVE_RELOCS],
            Self::ServerLegacy => &[],
        }
    }
}

/// Gives the binaries of a server package the lean link of a Linux card.
///
/// The build script of the server package calls it, beside the emitter of the
/// schema:
///
/// ```ignore
/// fn main() -> std::process::ExitCode {
///     aosabi_build::lean_link(aosabi_build::LinuxCard::Server);
///     aosabi_build::emit_from(my_app::default)
/// }
/// ```
///
/// The call acts when `TARGET` is `aarch64-unknown-linux-gnu` or
/// `x86_64-unknown-linux-gnu` and `CARGO_CFG_PANIC` names `abort` or
/// `immediate-abort`, the immediate abort of a build that measures it. It then
/// writes [`DISCARD_UNWIND_FILE`] into `OUT_DIR` when its bytes differ and
/// prints, for the binaries of the package alone,
/// `cargo::rustc-link-arg-bins=-Wl,--no-eh-frame-hdr`,
/// `cargo::rustc-link-arg-bins=-Wl,-T,<OUT_DIR>/discard-unwind.ld` and, for
/// the card [`LinuxCard::Server`], `-Wl,-z,pack-relative-relocs` in the same
/// form. [`LINUX_CARD_ENV`] names another card when it is set. [`LINKER_ENV`]
/// at `lld` adds, for `aarch64-unknown-linux-gnu` alone,
/// `-B<sysroot>/lib/rustlib/<host>/bin/gcc-ld`, `-fuse-ld=lld`,
/// `-Wl,--icf=all` and one `-Wl,--keep-unique=<symbol>` for each outline
/// atomic whose section stays apart, with the sysroot that the compiler
/// `RUSTC` of the build script prints and the `HOST` of cargo, and at `gnu`
/// or unset adds nothing. The lean link of `x86_64-unknown-linux-gnu` takes
/// none of them at any value of the switch. The build script runs again when
/// either variable changes. A build for another target, or one whose
/// `CARGO_CFG_PANIC` names neither strategy, takes no argument of the lean
/// link, and a card that has to print the stack trace of a panic does not
/// call the function.
///
/// Cargo sets `CARGO_CFG_PANIC` from the configuration the compiler prints and
/// from the flags of `RUSTFLAGS`, and a profile with `panic = "abort"` alone
/// leaves it at `unwind`. So a build that wants the lean link passes
/// `-C panic=abort` in `RUSTFLAGS` with `--target`, which keeps the flag away
/// from the build scripts and the proc macros. Cargo gives each set of
/// `RUSTFLAGS` an `OUT_DIR` of its own, so the plain and the lean link of one
/// package never share the output of a run of the script.
///
/// On an error the message goes to cargo as `cargo::error`, which fails the
/// build after the script ends, and no link argument is printed.
///
/// [the cargo book on environment variables]: https://doc.rust-lang.org/cargo/reference/environment-variables.html
pub fn lean_link(card: LinuxCard) {
    println!("cargo::rerun-if-env-changed={LINUX_CARD_ENV}");
    println!("cargo::rerun-if-env-changed={LINKER_ENV}");
    match lean_link_args(card) {
        Ok(args) => {
            for arg in args {
                println!("cargo::rustc-link-arg-bins={arg}");
            }
        }
        Err(error) => println!("cargo::error={error}"),
    }
}

/// Returns the link arguments of the lean link of `card` and writes its
/// fragment, or no argument when the build is not one the lean link takes.
fn lean_link_args(card: LinuxCard) -> Result<Vec<String>, BuildError> {
    let card = match env::var(LINUX_CARD_ENV) {
        Ok(name) if !name.is_empty() => LinuxCard::from_name(&name).ok_or_else(|| {
            BuildError::LeanLink(format!(
                "{LINUX_CARD_ENV} is {name:?}, and the Linux cards are server and server-legacy"
            ))
        })?,
        _ => card,
    };
    let linker = match env::var(LINKER_ENV) {
        Ok(name) if !name.is_empty() => Linker::from_name(&name).ok_or_else(|| {
            BuildError::LeanLink(format!(
                "{LINKER_ENV} is {name:?}, and the linkers of a lean link are gnu and lld"
            ))
        })?,
        _ => Linker::Gnu,
    };
    let target = env::var("TARGET").unwrap_or_default();
    let panic = env::var("CARGO_CFG_PANIC").unwrap_or_default();
    if !LEAN_TARGETS.contains(&target.as_str()) || !aborts(&panic) {
        return Ok(Vec::new());
    }

    let out_dir = env::var("OUT_DIR")
        .ok()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| BuildError::LeanLink(String::from("OUT_DIR is not set as UTF-8 text")))?;
    // The compiler driver splits the argument of `-Wl` at every comma.
    if out_dir.contains(',') {
        return Err(BuildError::LeanLink(format!(
            "OUT_DIR {out_dir} holds a comma, which ends an argument of -Wl"
        )));
    }
    let fragment = PathBuf::from(out_dir).join(DISCARD_UNWIND_FILE);
    update_file_if_changed(&fragment, DISCARD_UNWIND.as_bytes())
        .map_err(|error| BuildError::Io(fragment.clone(), error))?;

    let mut args = vec![
        String::from(NO_EH_FRAME_HDR),
        format!("-Wl,-T,{}", fragment.display()),
    ];
    args.extend(card.link_args().iter().map(|arg| String::from(*arg)));
    if linker == Linker::Lld && target == LLD_TARGET {
        args.extend(lld_args()?);
    }
    Ok(args)
}

/// Returns whether the value `panic` of `CARGO_CFG_PANIC` names a strategy
/// that never unwinds.
///
/// Cargo joins the values of a configuration that holds several with commas,
/// and the compiler sets both `panic="abort"` and `panic="immediate-abort"`
/// under `-Cpanic=immediate-abort`, so the variable reads
/// `abort,immediate-abort` there and `abort` under `-C panic=abort`. Either
/// strategy leaves the unwind tables nothing to serve. No Linux card takes the
/// immediate abort, since the abort message of a configuration needs the panic
/// hook.
fn aborts(panic: &str) -> bool {
    panic
        .split(',')
        .any(|strategy| strategy == "abort" || strategy == "immediate-abort")
}

/// Returns the link arguments that link a build for
/// `aarch64-unknown-linux-gnu` with the LLD of its toolchain and fold
/// identical code.
fn lld_args() -> Result<Vec<String>, BuildError> {
    let gcc_ld = gcc_ld_dir()?;
    let mut args = vec![
        format!("-B{}", gcc_ld.display()),
        String::from(FUSE_LLD),
        String::from(ICF_ALL),
    ];
    args.extend(
        AARCH64_KEEP_UNIQUE
            .iter()
            .map(|symbol| format!("-Wl,--keep-unique={symbol}")),
    );
    Ok(args)
}

/// Returns the directory of the linkers of the toolchain whose compiler the
/// build script names in `RUSTC`, which holds its `ld.lld`.
///
/// The sysroot is the one that compiler prints, so the link takes the LLD of
/// the toolchain that compiles the card, the pin or a copy of it, and the
/// directory is the one of the host that runs the link.
fn gcc_ld_dir() -> Result<PathBuf, BuildError> {
    let rustc = env::var_os("RUSTC")
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            BuildError::LeanLink(format!(
                "RUSTC is not set, so {LINKER_ENV} lld finds no sysroot"
            ))
        })?;
    let host = env::var("HOST")
        .ok()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| BuildError::LeanLink(String::from("HOST is not set as UTF-8 text")))?;
    let output = Command::new(&rustc)
        .args(["--print", "sysroot"])
        .output()
        .map_err(|error| {
            BuildError::LeanLink(format!(
                "{} --print sysroot does not run: {error}",
                Path::new(&rustc).display()
            ))
        })?;
    let sysroot = String::from_utf8(output.stdout)
        .ok()
        .filter(|_| output.status.success())
        .ok_or_else(|| {
            BuildError::LeanLink(format!(
                "{} --print sysroot prints no sysroot as UTF-8 text",
                Path::new(&rustc).display()
            ))
        })?;
    let dir = Path::new(sysroot.trim())
        .join("lib")
        .join("rustlib")
        .join(host)
        .join("bin")
        .join(GCC_LD);
    if dir.join("ld.lld").is_file() {
        Ok(dir)
    } else {
        Err(BuildError::LeanLink(format!(
            "{} holds no ld.lld, the linker of {LINKER_ENV} lld",
            dir.display()
        )))
    }
}
