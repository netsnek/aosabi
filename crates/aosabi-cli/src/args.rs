// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-cli: the command grammar of the two programs, their argument structs
 * and their usage texts.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/cli/index.ts and
 * packages/create-pylon/src/index.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 */

//! The command grammar.
//!
//! [`parse`] reads the arguments of `cargo-aosabi` and `aosabi` into the
//! [`Global`] flags and one [`Command`]. Cargo runs `cargo aosabi` as the
//! program `cargo-aosabi` with `aosabi` as its first argument, so that
//! argument is dropped and the two programs are one.
//!
//! The parse follows commander 12.1.0, in two passes. The program pass takes `-v`, `--verbose`,
//! `--manifest-path` and `-V`, `--version` wherever they stand, which is how
//! commander reads the options of the program before and after a command, and
//! keeps every other argument in order: the arguments up to the first one that
//! looks like an option are the operands, the first of which names the
//! command, and the rest goes to the command, whose own pass reads its options
//! the same way. An option takes its value from the next argument whatever it
//! looks like, or behind `=`, a short option also from the rest of its own
//! argument, and `--` ends the options. The first unknown option and every
//! argument behind it are kept aside, so `-h` or `--help` among them still
//! prints the help, and only then is the unknown option refused. Arguments
//! beyond those a command declares are accepted and left unread, because
//! commander 12.1.0 allows excess arguments unless a program asks otherwise.
//!
//! A refused command line is a [`UsageError`] whose text is commander's:
//! `error: unknown option '<flag>'` with a suggestion of the closest long
//! option, `error: unknown command '<name>'` with a suggestion of the closest
//! command, `error: missing required argument '<name>'`,
//! `error: option '<flags>' argument missing`, and
//! `error: option '<flags>' argument '<value>' is invalid. Allowed choices are <a>, <b>.`.
//! No command line at all, or one with the program flags alone, prints the
//! help of the program to standard error and exits 1, and `help <name>` with a
//! name no command carries does the same, as commander does.
//!
//! The help texts are commander's layout, wrapped at 80 columns as commander
//! wraps them when standard output is no terminal, with the program named
//! `aosabi`, as `docs/cli-and-targets.md` section 1 names it. An option that
//! names the entry reads its default as the Rust entry, the crate root, which
//! is cargo's default `src/lib.rs` in a scaffold of `aosabi new`. The
//! descriptions write U+2014 and U+2192 and the source spells them as
//! escapes.

use std::{
    ffi::OsString,
    fmt::Write,
    path::PathBuf, //
};

use crate::{
    dotenv,
    later::{
        self,
        Resolved, //
    }, //
};

/// The name the program gives itself in its help and its errors.
pub const PROGRAM: &str = "aosabi";

/// The first argument cargo passes to `cargo-aosabi` for `cargo aosabi`.
pub const CARGO_ARGUMENT: &str = "aosabi";

/// The version `--version` prints, the version of the package.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The width commander wraps a help text at when standard output is no
/// terminal.
const HELP_WIDTH: usize = 80;

/// The flags every command line may carry, wherever they stand.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Global {
    /// `-v` or `--verbose`, which prints the detail of every failure whole.
    pub verbose: bool,
    /// `--manifest-path`, the `Cargo.toml` of the service.
    pub manifest_path: Option<PathBuf>,
}

/// How a command may treat `Cargo.lock` and the network, as cargo's flags
/// of the same names.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LockMode {
    /// No flag: cargo may write the lock and fetch.
    #[default]
    Normal,
    /// `--locked`: the lock stays as it is.
    Locked,
    /// `--frozen`, or `--locked` with `--offline`: the lock stays and nothing
    /// is fetched.
    Frozen,
    /// `--offline`: nothing is fetched.
    Offline,
}

/// The command a command line names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// `build`.
    Build(BuildArgs),
    /// `dev`.
    Dev(DevArgs),
    /// `inspect`.
    Inspect(InspectArgs),
    /// `verify`.
    Verify(VerifyArgs),
    /// `mcp`.
    Mcp(McpArgs),
    /// `pull`.
    Pull(PullArgs),
    /// `new`.
    New(NewArgs),
    /// A command that lands with a later milestone or track row.
    Later {
        /// The command as typed, its words joined by one space.
        command: String,
        /// The milestone or the track row that lands it.
        lands: &'static str,
    },
    /// `-h`, `--help` or `help`, for the program or for one command.
    Help {
        /// The command whose help is asked for, `None` for the program.
        command: Option<String>,
    },
    /// `-V` or `--version`.
    Version,
}

/// The arguments of `build`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BuildArgs {
    /// `--device`, the card to build for.
    pub device: Option<String>,
    /// Every `--include`, in order.
    pub include: Vec<PathBuf>,
    /// `--release`.
    pub release: bool,
    /// The lock flags.
    pub lock: LockMode,
}

/// The arguments of `dev`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DevArgs {
    /// `--worker`.
    pub worker: bool,
    /// The lock flags.
    pub lock: LockMode,
}

/// The arguments of `inspect`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InspectArgs {
    /// `-e`, or `-m` when `-e` is absent.
    pub entry: Option<PathBuf>,
    /// `--json`.
    pub json: bool,
    /// `--sdl`.
    pub sdl: bool,
    /// `--ddl`.
    pub ddl: bool,
    /// The lock flags.
    pub lock: LockMode,
}

/// The arguments of `verify`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VerifyArgs {
    /// `-e`, or `-m` when `-e` is absent.
    pub entry: Option<PathBuf>,
    /// `--json`.
    pub json: bool,
    /// The lock flags.
    pub lock: LockMode,
}

/// The arguments of `mcp`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct McpArgs {
    /// `-e`, or `-m` when `-e` is absent.
    pub entry: Option<PathBuf>,
    /// `-c`, the project root, `.` by default.
    pub cwd: PathBuf,
}

/// The arguments of `pull`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PullArgs {
    /// The remote GraphQL endpoint.
    pub url: String,
    /// `-n`, `remote` by default.
    pub name: String,
    /// `-o`, `./src/generated` by default.
    pub output: PathBuf,
}

/// The arguments of `new`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewArgs {
    /// The directory of the new service.
    pub target: PathBuf,
    /// `-r`, `native` by default.
    pub runtime: Runtime,
    /// Every feature of `--features`, in order.
    pub features: Vec<Feature>,
    /// `--force`.
    pub force: bool,
    /// `--aosabi-path`, the Aosabi checkout the scaffold names by path.
    pub aosabi_path: Option<PathBuf>,
}

/// The runtime of a new service.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Runtime {
    /// A native binary.
    #[default]
    Native,
    /// A Cloudflare Worker.
    Workers,
}

/// A feature of a new service.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Feature {
    /// `auth`.
    Auth,
    /// `pages`.
    Pages,
}

/// A command line the grammar refuses, with the whole text to print on
/// standard error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsageError(pub String);

/// What an option takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Takes {
    /// Nothing, a flag.
    Nothing,
    /// One value, `<value>`.
    Value,
    /// Any number of values up to the next option, `[values...]`.
    Values,
}

/// One option of the grammar.
#[derive(Clone, Copy, Debug)]
struct Opt {
    /// The flags as commander prints them in the help and in its errors.
    flags: &'static str,
    /// The short spelling, `-e`.
    short: Option<&'static str>,
    /// The long spelling, `--entry`.
    long: &'static str,
    /// What the option takes.
    takes: Takes,
    /// The values the option accepts, every value when empty.
    choices: &'static [&'static str],
    /// Whether one value may hold a comma separated list.
    list: bool,
    /// The default the help names.
    default: Option<&'static str>,
    /// The description the help prints.
    description: &'static str,
}

impl Opt {
    /// Returns a flag with the spellings of `flags`.
    const fn flag(
        flags: &'static str,
        short: Option<&'static str>,
        long: &'static str,
        description: &'static str,
    ) -> Self {
        Self {
            flags,
            short,
            long,
            takes: Takes::Nothing,
            choices: &[],
            list: false,
            default: None,
            description,
        }
    }

    /// Returns an option that takes one value.
    const fn value(
        flags: &'static str,
        short: Option<&'static str>,
        long: &'static str,
        description: &'static str,
    ) -> Self {
        Self {
            takes: Takes::Value,
            ..Self::flag(flags, short, long, description)
        }
    }

    /// Returns the option with the default `default`, which the help names.
    const fn with_default(self, default: &'static str) -> Self {
        Self {
            default: Some(default),
            ..self
        }
    }

    /// Returns whether `arg` spells the option.
    fn is(&self, arg: &str) -> bool {
        self.long == arg || self.short == Some(arg)
    }
}

/// One positional argument of a command.
#[derive(Clone, Copy, Debug)]
struct Positional {
    /// The name, as the help and the error of a missing argument print it.
    name: &'static str,
    /// The description the help prints.
    description: &'static str,
}

/// One command of the grammar.
#[derive(Clone, Copy, Debug)]
struct Spec {
    /// The name, the first operand.
    name: &'static str,
    /// The description.
    description: &'static str,
    /// The required positional arguments, in order.
    arguments: &'static [Positional],
    /// The options, the help option left out.
    options: &'static [Opt],
}

/// `-h`, `--help`, which every command answers.
const HELP: Opt = Opt::flag(
    "-h, --help",
    Some("-h"),
    "--help",
    "display help for command",
);

/// `-V`, `--version`, of the program.
const VERSION_OPT: Opt = Opt::flag(
    "-V, --version",
    Some("-V"),
    "--version",
    "output the version number",
);

/// `-v`, `--verbose`, read wherever it stands.
const VERBOSE: Opt = Opt::flag(
    "-v, --verbose",
    Some("-v"),
    "--verbose",
    "Verbose output \u{2014} debug-level logging",
);

/// `--manifest-path`, read wherever it stands.
const MANIFEST_PATH: Opt = Opt::value(
    "--manifest-path <path>",
    None,
    "--manifest-path",
    "Path to Cargo.toml",
);

/// `--locked`, cargo's flag.
const LOCKED: Opt = Opt::flag(
    "--locked",
    None,
    "--locked",
    "Assert that Cargo.lock will remain unchanged",
);

/// `--frozen`, cargo's flag.
const FROZEN: Opt = Opt::flag(
    "--frozen",
    None,
    "--frozen",
    "Equivalent to specifying both --locked and --offline",
);

/// `--offline`, cargo's flag.
const OFFLINE: Opt = Opt::flag(
    "--offline",
    None,
    "--offline",
    "Run without accessing the network",
);

/// `-e`, `--entry`.
const ENTRY: Opt = Opt::value(
    "-e, --entry <path>",
    Some("-e"),
    "--entry",
    "Entry that constructs your app / registers models (default the crate root, ./src/lib.rs)",
);

/// `-m`, `--models`.
const MODELS: Opt = Opt::value(
    "-m, --models <path>",
    Some("-m"),
    "--models",
    "Deprecated alias for --entry",
);

/// The options of `build`.
const BUILD_OPTIONS: [Opt; 8] = [
    Opt::value(
        "--device <card>",
        None,
        "--device",
        "The built in card to build for, server, server-legacy, docker, worker or esp32c3-devkitm1 (default server)",
    ),
    Opt::value(
        "--include <path>",
        None,
        "--include",
        "Copy this project-relative file/dir next to the binary into target/aosabi/dist/, or into the image of the docker card (repeatable)",
    ),
    Opt::flag(
        "--release",
        None,
        "--release",
        "Build the release profile, which the docker card always builds",
    ),
    LOCKED,
    FROZEN,
    OFFLINE,
    VERBOSE,
    MANIFEST_PATH,
];

/// The options of `dev`.
const DEV_OPTIONS: [Opt; 6] = [
    Opt::flag(
        "--worker",
        None,
        "--worker",
        "Run a background WORKER instead of the web server, no HTTP, with watch/restart. Production equivalent: the built binary with AOSABI_ROLE=worker.",
    ),
    LOCKED,
    FROZEN,
    OFFLINE,
    VERBOSE,
    MANIFEST_PATH,
];

/// The options of `inspect`.
const INSPECT_OPTIONS: [Opt; 10] = [
    ENTRY,
    MODELS,
    Opt::flag(
        "--json",
        None,
        "--json",
        "Emit the full AppModel as JSON (default)",
    ),
    Opt::flag("--sdl", None, "--sdl", "Emit the GraphQL schema (SDL)"),
    Opt::flag("--ddl", None, "--ddl", "Emit the Postgres DDL"),
    LOCKED,
    FROZEN,
    OFFLINE,
    VERBOSE,
    MANIFEST_PATH,
];

/// The options of `verify`.
const VERIFY_OPTIONS: [Opt; 8] = [
    ENTRY,
    MODELS,
    Opt::flag("--json", None, "--json", "Emit the verdict as JSON"),
    LOCKED,
    FROZEN,
    OFFLINE,
    VERBOSE,
    MANIFEST_PATH,
];

/// The options of `mcp`.
const MCP_OPTIONS: [Opt; 5] = [
    ENTRY,
    MODELS,
    Opt::value(
        "-c, --cwd <dir>",
        Some("-c"),
        "--cwd",
        "Project root to inspect (default: current directory)",
    )
    .with_default("\".\""),
    VERBOSE,
    MANIFEST_PATH,
];

/// The options of `pull`.
const PULL_OPTIONS: [Opt; 3] = [
    Opt::value(
        "-n, --name <name>",
        Some("-n"),
        "--name",
        "The name of the remote service (used for the generated filename)",
    )
    .with_default("\"remote\""),
    Opt::value(
        "-o, --output <dir>",
        Some("-o"),
        "--output",
        "The directory to output the generated types",
    )
    .with_default("\"./src/generated\""),
    VERBOSE,
];

/// The values of `--runtime`.
const RUNTIMES: [&str; 2] = ["native", "workers"];

/// The values of `--features`.
const FEATURES: [&str; 2] = ["auth", "pages"];

/// The options of `new`.
const NEW_OPTIONS: [Opt; 5] = [
    Opt {
        choices: &RUNTIMES,
        ..Opt::value(
            "-r, --runtime <runtime>",
            Some("-r"),
            "--runtime",
            "Runtime",
        )
    }
    .with_default("\"native\""),
    Opt {
        takes: Takes::Values,
        choices: &FEATURES,
        list: true,
        ..Opt::flag("--features [features...]", None, "--features", "Features")
    },
    Opt::flag(
        "--force",
        None,
        "--force",
        "Write into a target that exists and is not empty",
    ),
    Opt::value(
        "--aosabi-path <dir>",
        None,
        "--aosabi-path",
        "Name aosabi by path from the Aosabi checkout at <dir>",
    ),
    VERBOSE,
];

/// The commands of the grammar, in the order of the help.
const COMMANDS: [Spec; 7] = [
    Spec {
        name: "build",
        description: "Build the Aosabi schema",
        arguments: &[],
        options: &BUILD_OPTIONS,
    },
    Spec {
        name: "inspect",
        description: "Serialize the app model (schema + entities + queues + authz)",
        arguments: &[],
        options: &INSPECT_OPTIONS,
    },
    Spec {
        name: "verify",
        description: "Build + typecheck + migration check \u{2192} a stratified verdict (pass/review/fail)",
        arguments: &[],
        options: &VERIFY_OPTIONS,
    },
    Spec {
        name: "mcp",
        description: "Run the Aosabi MCP server (stdio): describe_app / get_entity / get_operation / verify",
        arguments: &[],
        options: &MCP_OPTIONS,
    },
    Spec {
        name: "pull",
        description: "Fetch a remote GraphQL API and generate strictly-typed gateway models.",
        arguments: &[Positional {
            name: "url",
            description: "The remote GraphQL endpoint URL",
        }],
        options: &PULL_OPTIONS,
    },
    Spec {
        name: "dev",
        description: "Start the Aosabi development server",
        arguments: &[],
        options: &DEV_OPTIONS,
    },
    Spec {
        name: "new",
        description: "Create a new Aosabi service",
        arguments: &[Positional {
            name: "target",
            description: "The directory of the new service",
        }],
        options: &NEW_OPTIONS,
    },
];

/// The later commands that have a help page, with their description, in the
/// order of the help of the program after `dev` and `new`.
const LATER_PAGES: [(&str, &str); 3] = [
    ("db", "Manage the database schema (ORM migrations)"),
    (
        "docs",
        "Write the API reference of the service beside its schema",
    ),
    (
        "bridge",
        "Serve a device over GraphTP to HTTP clients on this machine",
    ),
];

/// The help page of `db`: each sub command with its term and its
/// description.
const DB_PAGE: [(&str, &str); 16] = [
    (
        "status [options]",
        "Show pending schema changes and unapplied migrations",
    ),
    (
        "diff [options] [name]",
        "Generate a migration from the diff between models and the migration history",
    ),
    (
        "plan [options]",
        "Print the SQL each migration would run, without touching a database",
    ),
    (
        "check [options]",
        "CI gate: fail on uncaptured model changes or tampered migrations",
    ),
    (
        "fix-deps [options]",
        "Find (and with --write, backfill) cross-app dependency edges a migration references but does not declare",
    ),
    (
        "migrate [options]",
        "Apply unapplied migrations to the database (requires DATABASE_URL)",
    ),
    (
        "rollback [options]",
        "Reverse the most recently applied migration(s) (requires DATABASE_URL)",
    ),
    (
        "resolve [options] <name>",
        "Mark a migration applied/rolled-back in the ledger without running it",
    ),
    (
        "rename-app [options] <spec>",
        "Re-point the migration ledger after renaming an app (run once per DB before migrate)",
    ),
    (
        "seed [options]",
        "Run the seed file to populate the database (requires DATABASE_URL)",
    ),
    (
        "baseline [options] [name]",
        "Adopt an existing database: introspect it, generate model stubs + an initial migration, and mark it applied (requires DATABASE_URL)",
    ),
    (
        "merge [options] [name]",
        "Reconverge divergent migration heads (after a branch merge) into a merge migration",
    ),
    (
        "squash [options] [name]",
        "Collapse the schema migration history into a single migration (rewrites history)",
    ),
    (
        "push [options]",
        "Sync models to the database directly, without a migration (prototyping)",
    ),
    (
        "create [options]",
        "Create the target database if it does not exist (requires DATABASE_URL)",
    ),
    (
        "reset [options]",
        "DROP everything and re-apply all migrations from scratch (dev; requires DATABASE_URL)",
    ),
];

/// The description of the program in its help.
const PROGRAM_DESCRIPTION: &str = "Aosabi Development CLI";

/// One argument, its text for matching and its bytes for a path.
#[derive(Clone, Debug)]
struct Arg {
    /// The argument as text, with U+FFFD for each byte that is no UTF-8.
    text: String,
    /// The argument as the operating system passed it.
    raw: OsString,
}

impl Arg {
    /// Returns the argument `raw`.
    fn new(raw: &OsString) -> Self {
        Self {
            text: raw.to_string_lossy().into_owned(),
            raw: raw.clone(),
        }
    }

    /// Returns whether the argument looks like an option, as commander's
    /// `maybeOption` reads it.
    fn is_option_like(&self) -> bool {
        self.text.len() > 1 && self.text.starts_with('-')
    }
}

/// The options a pass matched, in order, each with its value.
#[derive(Debug, Default)]
struct Matches {
    found: Vec<(&'static str, Option<Arg>)>,
}

impl Matches {
    /// Returns whether the option `long` was given.
    fn flag(&self, long: &str) -> bool {
        self.found.iter().any(|(name, _)| *name == long)
    }

    /// Returns the last value of the option `long`.
    fn last(&self, long: &str) -> Option<&Arg> {
        self.found
            .iter()
            .rev()
            .find(|(name, value)| *name == long && value.is_some())
            .and_then(|(_, value)| value.as_ref())
    }

    /// Returns every value of the option `long`, in order.
    fn all(&self, long: &str) -> impl Iterator<Item = &Arg> {
        self.found
            .iter()
            .filter(move |(name, _)| *name == long)
            .filter_map(|(_, value)| value.as_ref())
    }
}

/// What a pass leaves: the operands, the first unknown option with every
/// argument behind it, and the options it matched.
#[derive(Debug, Default)]
struct Pass {
    operands: Vec<Arg>,
    unknown: Vec<Arg>,
    matches: Matches,
}

/// What the program pass ends in.
enum ProgramPass {
    /// `-V` or `--version` stood among the arguments.
    Version,
    /// The flags of the program and the arguments of the command.
    Rest(Global, Vec<Arg>, Vec<Arg>),
}

/// Parses `argv`, the program name first, into the global flags and one
/// command.
///
/// # Errors
///
/// Returns a [`UsageError`] with commander's text for an unknown option or
/// command, an option without its value, a value outside an option's
/// choices and a missing required argument, and with the help of the program
/// for a command line that names no command.
pub fn parse(argv: &[OsString]) -> Result<(Global, Command), UsageError> {
    let mut list: Vec<Arg> = argv.iter().skip(1).map(Arg::new).collect();
    if list.first().is_some_and(|arg| arg.text == CARGO_ARGUMENT) {
        list.remove(0);
    }
    let (global, operands, unknown) = match program_pass(list)? {
        ProgramPass::Version => return Ok((Global::default(), Command::Version)),
        ProgramPass::Rest(global, operands, unknown) => (global, operands, unknown),
    };
    let command = dispatch(&operands, &unknown)?;
    Ok((global, command))
}

/// Returns the help text of the command `command`, or of the program for
/// `None`, and `None` for a name that has no help page.
#[must_use]
pub fn help(command: Option<&str>) -> Option<String> {
    let Some(name) = command else {
        return Some(program_help());
    };
    if let Some(spec) = COMMANDS.iter().find(|spec| spec.name == name) {
        return Some(command_help(spec));
    }
    let (_, description) = LATER_PAGES.iter().find(|(word, _)| *word == name)?;
    Some(later_help(name, description))
}

/// Reads the flags of the program wherever they stand and splits the rest
/// into the operands and the arguments from the first unknown option on.
fn program_pass(args: Vec<Arg>) -> Result<ProgramPass, UsageError> {
    let mut global = Global::default();
    let mut operands = Vec::new();
    let mut unknown: Vec<Arg> = Vec::new();
    let mut queue = args.into_iter();
    let mut pending: Option<Arg> = None;
    while let Some(arg) = pending.take().or_else(|| queue.next()) {
        if arg.text == "--" {
            if unknown.is_empty() {
                operands.extend(queue);
            } else {
                unknown.push(arg);
                unknown.extend(queue);
            }
            break;
        }
        if arg.is_option_like() {
            if VERBOSE.is(&arg.text) {
                global.verbose = true;
                continue;
            }
            if VERSION_OPT.is(&arg.text) {
                return Ok(ProgramPass::Version);
            }
            if MANIFEST_PATH.is(&arg.text) {
                let value = queue.next().ok_or_else(|| missing_value(&MANIFEST_PATH))?;
                global.manifest_path = Some(PathBuf::from(value.raw));
                continue;
            }
            if let Some(value) = arg.text.strip_prefix("--manifest-path=") {
                global.manifest_path = Some(PathBuf::from(value));
                continue;
            }
            if let Some(rest) = short_combination(&arg.text, 'v') {
                global.verbose = true;
                pending = Some(rest);
                continue;
            }
            if short_combination(&arg.text, 'V').is_some() {
                return Ok(ProgramPass::Version);
            }
        }
        if arg.is_option_like() || !unknown.is_empty() {
            unknown.push(arg);
        } else {
            operands.push(arg);
        }
    }
    Ok(ProgramPass::Rest(global, operands, unknown))
}

/// Returns the rest of a combination of short flags that starts with
/// `-<letter>`, as the argument commander reads next, `-hx` for `-vhx`.
fn short_combination(text: &str, letter: char) -> Option<Arg> {
    let mut chars = text.chars();
    if chars.next() != Some('-') || chars.next() != Some(letter) {
        return None;
    }
    let rest = chars.as_str();
    if rest.is_empty() {
        return None;
    }
    let raw = OsString::from(format!("-{rest}"));
    Some(Arg::new(&raw))
}

/// Finds the command the operands name and reads its arguments.
fn dispatch(operands: &[Arg], unknown: &[Arg]) -> Result<Command, UsageError> {
    if let Some(first) = operands.first() {
        let name = first.text.as_str();
        if let Some(spec) = COMMANDS.iter().find(|spec| spec.name == name) {
            return command_pass(spec, &operands[1..], unknown);
        }
        if later::row(name).is_some() {
            return later_command(name, &operands[1..], unknown);
        }
        if name == "help" {
            let asked = operands
                .get(1)
                .map(|arg| arg.text.as_str())
                .filter(|asked| !asked.is_empty());
            return match asked {
                None => Ok(Command::Help { command: None }),
                Some(asked) if help(Some(asked)).is_some() => Ok(Command::Help {
                    command: Some(asked.to_owned()),
                }),
                Some(_) => Err(UsageError(program_help())),
            };
        }
    }
    if operands.is_empty() && unknown.is_empty() {
        return Err(UsageError(program_help()));
    }
    if asks_for_help(unknown) {
        return Ok(Command::Help { command: None });
    }
    if let Some(first) = operands.first() {
        let names: Vec<&str> = program_command_names().collect();
        return Err(unknown_command(&first.text, &names));
    }
    Err(unknown_option(&unknown[0].text, &[]))
}

/// Returns the names of the commands of the help of the program, the
/// candidates of a suggestion.
fn program_command_names() -> impl Iterator<Item = &'static str> {
    COMMANDS
        .iter()
        .map(|spec| spec.name)
        .chain(LATER_PAGES.iter().map(|(word, _)| *word))
        .chain(["help"])
}

/// Returns whether `args` hold `-h` or `--help` as a whole argument.
fn asks_for_help(args: &[Arg]) -> bool {
    args.iter().any(|arg| HELP.is(&arg.text))
}

/// Resolves a command of a later row and its sub command.
fn later_command(word: &str, operands: &[Arg], unknown: &[Arg]) -> Result<Command, UsageError> {
    let help_command = operands.first().is_some_and(|arg| arg.text == "help");
    if (help_command || asks_for_help(unknown)) && help(Some(word)).is_some() {
        return Ok(Command::Help {
            command: Some(word.to_owned()),
        });
    }
    let texts: Vec<String> = operands.iter().map(|arg| arg.text.clone()).collect();
    let all: Vec<String> = texts
        .iter()
        .cloned()
        .chain(unknown.iter().map(|arg| arg.text.clone()))
        .collect();
    match later::resolve(word, &texts, &all) {
        Some(Resolved::Later { command, lands }) => Ok(Command::Later { command, lands }),
        Some(Resolved::UnknownSub(name)) => {
            let subs: Vec<&str> = later::row(word)
                .map(|row| row.subs.iter().map(|(sub, _)| *sub).collect())
                .unwrap_or_default();
            Err(unknown_command(&name, &subs))
        }
        None => Err(UsageError(program_help())),
    }
}

/// Reads the arguments of the command `spec`: `operands` that the program
/// pass kept before the first option, and `unknown` from it on.
fn command_pass(spec: &Spec, operands: &[Arg], unknown: &[Arg]) -> Result<Command, UsageError> {
    let pass = option_pass(spec.options, unknown)?;
    if asks_for_help(&pass.unknown) {
        return Ok(Command::Help {
            command: Some(spec.name.to_owned()),
        });
    }
    if let Some(first) = pass.unknown.first() {
        return Err(unknown_option(&first.text, spec.options));
    }
    let positionals: Vec<Arg> = operands.iter().cloned().chain(pass.operands).collect();
    if let Some(missing) = spec.arguments.get(positionals.len()) {
        return Err(UsageError(format!(
            "error: missing required argument '{}'\n",
            missing.name
        )));
    }
    Ok(build_command(spec.name, &pass.matches, &positionals))
}

/// Reads `args` against `options`, as commander's `parseOptions` does.
fn option_pass(options: &[Opt], args: &[Arg]) -> Result<Pass, UsageError> {
    let mut pass = Pass::default();
    let mut queue: Vec<Arg> = args.iter().rev().cloned().collect();
    let mut variadic: Option<&Opt> = None;
    let mut to_unknown = false;
    while let Some(arg) = queue.pop() {
        if arg.text == "--" {
            let rest = queue.drain(..).rev();
            if to_unknown {
                pass.unknown.push(arg);
                pass.unknown.extend(rest);
            } else {
                pass.operands.extend(rest);
            }
            break;
        }
        if let Some(option) = variadic.filter(|_| !arg.is_option_like()) {
            set(&mut pass.matches, option, Some(arg))?;
            continue;
        }
        variadic = None;
        if arg.is_option_like()
            && let Some(option) = options.iter().find(|option| option.is(&arg.text))
        {
            let value = match option.takes {
                Takes::Nothing => None,
                Takes::Value => Some(queue.pop().ok_or_else(|| missing_value(option))?),
                Takes::Values => queue.pop_if(|next| !next.is_option_like()),
            };
            set(&mut pass.matches, option, value)?;
            if option.takes == Takes::Values {
                variadic = Some(option);
            }
            continue;
        }
        if let Some((option, value)) = attached_value(options, &arg) {
            let flag = value.is_none();
            set(&mut pass.matches, option, value)?;
            if flag {
                queue.push(short_rest(&arg));
            }
            continue;
        }
        if arg.is_option_like() {
            to_unknown = true;
        }
        if to_unknown {
            pass.unknown.push(arg);
        } else {
            pass.operands.push(arg);
        }
    }
    Ok(pass)
}

/// Returns the option of a combined short argument or of `--long=value`,
/// with the value it carries, or with `None` for a flag whose short form
/// heads a combination of flags.
fn attached_value<'a>(options: &'a [Opt], arg: &Arg) -> Option<(&'a Opt, Option<Arg>)> {
    let text = arg.text.as_str();
    if text.len() > 2 && text.starts_with('-') && !text.starts_with("--") {
        let short: String = text.chars().take(2).collect();
        let option = options
            .iter()
            .find(|option| option.short == Some(short.as_str()))?;
        if option.takes == Takes::Nothing {
            return Some((option, None));
        }
        let value: String = text.chars().skip(2).collect();
        return Some((option, Some(Arg::new(&OsString::from(value)))));
    }
    let (name, value) = text
        .strip_prefix("--")
        .and_then(|rest| rest.split_once('='))?;
    if name.is_empty() {
        return None;
    }
    let option = options
        .iter()
        .find(|option| option.long.strip_prefix("--") == Some(name))?;
    if option.takes == Takes::Nothing {
        return None;
    }
    Some((option, Some(Arg::new(&OsString::from(value)))))
}

/// Returns the rest of a combination of short flags behind its first flag.
fn short_rest(arg: &Arg) -> Arg {
    let rest: String = arg.text.chars().skip(2).collect();
    Arg::new(&OsString::from(format!("-{rest}")))
}

/// Records `option` with `value`, refusing a value outside its choices.
fn set(matches: &mut Matches, option: &Opt, value: Option<Arg>) -> Result<(), UsageError> {
    if let Some(value) = &value
        && !option.choices.is_empty()
    {
        let parts: Vec<&str> = if option.list {
            value
                .text
                .split(',')
                .filter(|part| !part.is_empty())
                .collect()
        } else {
            vec![value.text.as_str()]
        };
        if let Some(bad) = parts.iter().find(|part| !option.choices.contains(part)) {
            return Err(UsageError(format!(
                "error: option '{}' argument '{bad}' is invalid. Allowed choices are {}.\n",
                option.flags,
                option.choices.join(", ")
            )));
        }
    }
    let long = option.long;
    matches.found.push((long, value));
    Ok(())
}

/// Returns the command `name` with the arguments of `matches` and
/// `positionals`, which the passes have checked.
fn build_command(name: &str, matches: &Matches, positionals: &[Arg]) -> Command {
    let path = |long: &str| matches.last(long).map(|arg| PathBuf::from(&arg.raw));
    let entry = path("--entry").or_else(|| path("--models"));
    let lock = lock_mode(matches);
    match name {
        "build" => Command::Build(BuildArgs {
            device: matches.last("--device").map(|arg| arg.text.clone()),
            include: matches
                .all("--include")
                .map(|arg| PathBuf::from(&arg.raw))
                .collect(),
            release: matches.flag("--release"),
            lock,
        }),
        "dev" => Command::Dev(DevArgs {
            worker: matches.flag("--worker"),
            lock,
        }),
        "inspect" => Command::Inspect(InspectArgs {
            entry,
            json: matches.flag("--json"),
            sdl: matches.flag("--sdl"),
            ddl: matches.flag("--ddl"),
            lock,
        }),
        "verify" => Command::Verify(VerifyArgs {
            entry,
            json: matches.flag("--json"),
            lock,
        }),
        "mcp" => Command::Mcp(McpArgs {
            entry,
            cwd: path("--cwd").unwrap_or_else(|| PathBuf::from(".")),
        }),
        "pull" => Command::Pull(PullArgs {
            url: positionals[0].text.clone(),
            name: matches
                .last("--name")
                .map_or_else(|| "remote".to_owned(), |arg| arg.text.clone()),
            output: path("--output").unwrap_or_else(|| PathBuf::from("./src/generated")),
        }),
        _ => Command::New(NewArgs {
            target: PathBuf::from(&positionals[0].raw),
            runtime: match matches.last("--runtime").map(|arg| arg.text.as_str()) {
                Some("workers") => Runtime::Workers,
                _ => Runtime::Native,
            },
            features: features(matches),
            force: matches.flag("--force"),
            aosabi_path: path("--aosabi-path"),
        }),
    }
}

/// Returns the lock mode of the lock flags in `matches`, as cargo combines
/// them.
fn lock_mode(matches: &Matches) -> LockMode {
    let locked = matches.flag("--locked");
    let offline = matches.flag("--offline");
    if matches.flag("--frozen") || (locked && offline) {
        LockMode::Frozen
    } else if locked {
        LockMode::Locked
    } else if offline {
        LockMode::Offline
    } else {
        LockMode::Normal
    }
}

/// Returns the features of every value of `--features`, each value split at
/// its commas, in order.
fn features(matches: &Matches) -> Vec<Feature> {
    matches
        .all("--features")
        .flat_map(|arg| arg.text.split(',').map(str::to_owned).collect::<Vec<_>>())
        .filter_map(|part| match part.as_str() {
            "auth" => Some(Feature::Auth),
            "pages" => Some(Feature::Pages),
            _ => None,
        })
        .collect()
}

/// Returns commander's error of an option whose value is missing.
fn missing_value(option: &Opt) -> UsageError {
    UsageError(format!(
        "error: option '{}' argument missing\n",
        option.flags
    ))
}

/// Returns commander's error of the unknown option `flag`, with a suggestion
/// among the long options of `options`, of the program and `--help`.
fn unknown_option(flag: &str, options: &[Opt]) -> UsageError {
    let suggestion = if flag.starts_with("--") {
        let candidates: Vec<&str> = options
            .iter()
            .chain([&HELP])
            .chain(program_options())
            .map(|option| option.long)
            .collect();
        suggest_similar(flag, &candidates)
    } else {
        String::new()
    };
    UsageError(format!("error: unknown option '{flag}'{suggestion}\n"))
}

/// Returns commander's error of the unknown command `name`, with a
/// suggestion among `names`.
fn unknown_command(name: &str, names: &[&str]) -> UsageError {
    let suggestion = suggest_similar(name, names);
    UsageError(format!("error: unknown command '{name}'{suggestion}\n"))
}

/// Returns the options of the program in the order of its help.
fn program_options() -> [&'static Opt; 4] {
    [&VERSION_OPT, &VERBOSE, &MANIFEST_PATH, &HELP]
}

/// The largest edit distance a suggestion may have.
const MAX_DISTANCE: usize = 3;

/// Returns commander's suggestion of the candidates closest to `word`, an
/// empty string when none is close enough.
///
/// The distance is the optimal string alignment distance over UTF-16 code
/// units, a candidate of one unit is never suggested, and a candidate is close
/// enough when more than 40 percent of the longer word survives. The
/// candidates at the smallest distance are sorted, which for the ASCII names
/// of this grammar is the order `localeCompare` gives them.
fn suggest_similar(word: &str, candidates: &[&str]) -> String {
    let options = word.starts_with("--");
    let strip = |text: &str| -> Vec<u16> {
        let text = if options {
            text.get(2..).unwrap_or_default()
        } else {
            text
        };
        text.encode_utf16().collect()
    };
    let target = strip(word);
    let mut unique: Vec<&str> = Vec::new();
    for candidate in candidates {
        if !unique.contains(candidate) {
            unique.push(candidate);
        }
    }
    let mut similar: Vec<&str> = Vec::new();
    let mut best = MAX_DISTANCE;
    for candidate in unique {
        let units = strip(candidate);
        if units.len() <= 1 {
            continue;
        }
        let distance = edit_distance(&target, &units);
        let length = target.len().max(units.len());
        if (length - distance.min(length)) * 10 <= length * 4 {
            continue;
        }
        if distance < best {
            best = distance;
            similar = vec![candidate];
        } else if distance == best {
            similar.push(candidate);
        }
    }
    similar.sort_unstable();
    match similar.as_slice() {
        [] => String::new(),
        [one] => format!("\n(Did you mean {one}?)"),
        many => format!("\n(Did you mean one of {}?)", many.join(", ")),
    }
}

/// Returns the optimal string alignment distance of `a` and `b`, or the
/// longer length when the lengths differ by more than [`MAX_DISTANCE`].
fn edit_distance(a: &[u16], b: &[u16]) -> usize {
    if a.len().abs_diff(b.len()) > MAX_DISTANCE {
        return a.len().max(b.len());
    }
    let width = b.len() + 1;
    let mut d = vec![0usize; (a.len() + 1) * width];
    for i in 0..=a.len() {
        d[i * width] = i;
    }
    for (j, cell) in d.iter_mut().take(width).enumerate() {
        *cell = j;
    }
    for j in 1..=b.len() {
        for i in 1..=a.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut best = (d[(i - 1) * width + j] + 1)
                .min(d[i * width + j - 1] + 1)
                .min(d[(i - 1) * width + j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                best = best.min(d[(i - 2) * width + j - 2] + 1);
            }
            d[i * width + j] = best;
        }
    }
    d[a.len() * width + b.len()]
}

/// Returns the help of the program.
fn program_help() -> String {
    let options: Vec<(String, String)> = program_options()
        .iter()
        .map(|option| option_item(option))
        .collect();
    let mut commands: Vec<(String, String)> = COMMANDS
        .iter()
        .map(|spec| (command_term(spec), spec.description.to_owned()))
        .collect();
    let dev = commands
        .iter()
        .position(|(term, _)| term.starts_with("dev "))
        .unwrap_or(commands.len());
    commands.insert(dev, ("db".to_owned(), LATER_PAGES[0].1.to_owned()));
    commands.extend(
        LATER_PAGES[1..]
            .iter()
            .map(|(word, description)| ((*word).to_owned(), (*description).to_owned())),
    );
    commands.push((
        "help [command]".to_owned(),
        "display help for command".to_owned(),
    ));
    render_page(
        &format!("{PROGRAM} [options] [command]"),
        PROGRAM_DESCRIPTION,
        &[],
        &options,
        &commands,
    )
}

/// Returns the help of the command `spec`.
fn command_help(spec: &Spec) -> String {
    let mut usage = format!("{PROGRAM} {} [options]", spec.name);
    for argument in spec.arguments {
        let _ = write!(usage, " <{}>", argument.name);
    }
    let arguments: Vec<(String, String)> = spec
        .arguments
        .iter()
        .map(|argument| (argument.name.to_owned(), argument.description.to_owned()))
        .collect();
    let options: Vec<(String, String)> = spec
        .options
        .iter()
        .chain([&HELP])
        .map(option_item)
        .collect();
    render_page(&usage, spec.description, &arguments, &options, &[])
}

/// Returns the help of the later command `word`, the sub commands of `db`
/// listed with their descriptions.
fn later_help(word: &str, description: &str) -> String {
    let options = [option_item(&HELP)];
    let mut commands: Vec<(String, String)> = Vec::new();
    if word == "db" {
        commands.extend(
            DB_PAGE
                .iter()
                .map(|(term, text)| ((*term).to_owned(), (*text).to_owned())),
        );
        commands.push((
            "help [command]".to_owned(),
            "display help for command".to_owned(),
        ));
    }
    let usage = if commands.is_empty() {
        format!("{PROGRAM} {word} [options]")
    } else {
        format!("{PROGRAM} {word} [options] [command]")
    };
    render_page(&usage, description, &[], &options, &commands)
}

/// Returns the term of `spec` in the list of commands, its name, `[options]`
/// and its arguments.
fn command_term(spec: &Spec) -> String {
    let mut term = spec.name.to_owned();
    if !spec.options.is_empty() {
        term.push_str(" [options]");
    }
    for argument in spec.arguments {
        let _ = write!(term, " <{}>", argument.name);
    }
    term
}

/// Returns the term and the description of `option` in a list of options,
/// the choices and the default appended as commander appends them.
fn option_item(option: &Opt) -> (String, String) {
    let mut extra = Vec::new();
    if !option.choices.is_empty() {
        let quoted: Vec<String> = option
            .choices
            .iter()
            .map(|choice| format!("\"{choice}\""))
            .collect();
        extra.push(format!("choices: {}", quoted.join(", ")));
    }
    if let Some(default) = option.default {
        extra.push(format!("default: {default}"));
    }
    let description = if extra.is_empty() {
        option.description.to_owned()
    } else {
        format!("{} ({})", option.description, extra.join(", "))
    };
    (option.flags.to_owned(), description)
}

/// Returns a help page in the layout of commander's `formatHelp`.
fn render_page(
    usage: &str,
    description: &str,
    arguments: &[(String, String)],
    options: &[(String, String)],
    commands: &[(String, String)],
) -> String {
    let term_width = arguments
        .iter()
        .chain(options)
        .chain(commands)
        .map(|(term, _)| term.chars().count())
        .max()
        .unwrap_or(0);
    let mut out = vec![format!("Usage: {usage}"), String::new()];
    if !description.is_empty() {
        out.push(wrap(description, HELP_WIDTH, 0));
        out.push(String::new());
    }
    for (heading, items) in [
        ("Arguments:", arguments),
        ("Options:", options),
        ("Commands:", commands),
    ] {
        if items.is_empty() {
            continue;
        }
        out.push(heading.to_owned());
        let lines: Vec<String> = items
            .iter()
            .map(|(term, text)| format_item(term, text, term_width))
            .collect();
        out.push(indent_lines(&lines.join("\n")));
        out.push(String::new());
    }
    out.join("\n")
}

/// Returns one item of a list, the term padded and the description wrapped
/// under itself.
fn format_item(term: &str, description: &str, term_width: usize) -> String {
    if description.is_empty() {
        return term.to_owned();
    }
    let padding = (term_width + 2).saturating_sub(term.chars().count());
    let full = format!("{term}{}{description}", " ".repeat(padding));
    wrap(&full, HELP_WIDTH - 2, term_width + 2)
}

/// Returns `text` with two spaces before every line.
fn indent_lines(text: &str) -> String {
    text.split('\n')
        .map(|line| format!("  {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Returns `text` wrapped at `width` columns with every line after the first
/// indented by `indent`, as commander's `wrap` does.
///
/// A text with a line break followed by white space is taken as formatted by
/// hand and left as it is, and so is a text whose column would be narrower
/// than 40.
fn wrap(text: &str, width: usize, indent: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    let manual = chars
        .windows(2)
        .any(|pair| pair[0] == '\n' && is_indent_char(pair[1]));
    if manual || width < indent + 40 {
        return text.to_owned();
    }
    let column = width - indent;
    let split = indent.min(chars.len());
    let leading: String = chars[..split].iter().collect();
    let rest = &chars[split..];
    let pad = " ".repeat(indent);
    let mut lines = Vec::new();
    let mut at = 0;
    while at < rest.len() {
        if let Some(end) = wrap_match(rest, at, column - 1) {
            let line: String = rest[at..end].iter().collect();
            lines.push(line);
            at = end;
        } else {
            at += 1;
        }
    }
    let body: Vec<String> = lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            if line == "\n" {
                String::new()
            } else if i > 0 {
                format!("{pad}{}", line.trim_end_matches(dotenv::is_space))
            } else {
                line.trim_end_matches(dotenv::is_space).to_owned()
            }
        })
        .collect();
    format!("{leading}{}", body.join("\n"))
}

/// Returns the end of the match of
/// `\n|.{1,most}([\s​]|$)|[^\s​]+?([\s​]|$)` at `at`.
fn wrap_match(text: &[char], at: usize, most: usize) -> Option<usize> {
    if text[at] == '\n' {
        return Some(at + 1);
    }
    let run = text[at..]
        .iter()
        .take(most)
        .take_while(|&&c| !dotenv::is_line_terminator(c))
        .count();
    for count in (1..=run).rev() {
        let next = at + count;
        if next == text.len() {
            return Some(next);
        }
        if is_break(text[next]) {
            return Some(next + 1);
        }
    }
    if is_break(text[at]) {
        return None;
    }
    let word = text[at..].iter().take_while(|&&c| !is_break(c)).count();
    let next = at + word;
    Some(if next == text.len() { next } else { next + 1 })
}

/// Returns whether `c` is a break of `wrap`, white space or U+200B.
fn is_break(c: char) -> bool {
    dotenv::is_space(c) || c == '\u{200b}'
}

/// Returns whether `c` marks a text formatted by hand behind a line break.
fn is_indent_char(c: char) -> bool {
    matches!(
        c,
        ' ' | '\u{c}' | '\t' | '\u{b}' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}'
    )
}
