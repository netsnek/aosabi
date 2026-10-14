// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-cli: the supervisor of the served child of the development loop.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/cli/dev/dev-server.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 */

//! The supervisor.
//!
//! A Rust service is a binary, so [`Supervisor`] runs it as a child and the restart is
//! the whole reload:
//!
//! 1. The first build is steps 3 to 7 of `build` for the `server` card in
//!    the dev profile, with the output of cargo on standard error as it
//!    comes. A failed first build prints that output and the supervisor keeps
//!    watching.
//! 2. The child is the built binary with `NODE_ENV=development`,
//!    `AOSABI_ENV=development`, `AOSABI_DEV=1`, `PORT` and `AOSABI_PORT` at the
//!    port of [`env::port`], and under `--worker` `AOSABI_ROLE=worker`, beside
//!    the pairs of `.env` the environment does not set. Under those variables
//!    the served binary binds the port itself and prints nothing. The child
//!    stays in the process group of the supervisor, so a Ctrl C ends both, and
//!    its standard output goes to the standard error of the command line.
//! 3. The first time it answers, the supervisor prints a box of its own, which
//!    reads `Aosabi is up and running.` and
//!    `Press Ctrl + C to stop the server.` in the frame `consola.box` draws,
//!    and then `Aosabi running at http://localhost:<port>`, the line the
//!    served binary prints outside development. The worker role probes
//!    nothing and prints `[dev] worker started, watching src for changes`
//!    once.
//! 4. Every changed path prints `[dev] <kind> reload`, the character U+2014 and
//!    the path relative to the root, and the highest kind
//!    of a burst is kept. After 200 ms with no change, 150 ms in the worker
//!    role, the supervisor prints `[dev] rebuilding` and builds. A burst of the
//!    kind `config` reads the project again first, since the manifest or the
//!    configuration changed. A build that succeeds prints
//!    `[dev] restarted`, kills the old child with [`Child::kill`], since the
//!    served binary has no graceful shutdown, starts the new one and probes it.
//!    A build that fails prints what cargo printed and
//!    `[dev] build failed, the previous server keeps running`, the rule of
//!    `docs/cli-and-targets.md` section 4, or
//!    `[dev] build failed, watching for the next change` while no child runs.
//!
//! The builds run one after the other on the thread of the supervisor, so a
//! change that arrives during a build is seen by the next poll and starts one
//! more build after it. A child that exits on its own is reported once with
//! its status and starts again with the next build. No signal handler is installed, because the standard library
//! offers none without `unsafe`, and the served binary carries no guard against
//! an orphaned child.

use std::{
    ffi::{
        OsStr,
        OsString, //
    },
    fmt::Write as _,
    io,
    path::{
        Path,
        PathBuf, //
    },
    process::{
        Child,
        Command,
        ExitCode,
        ExitStatus,
        Stdio, //
    },
    thread,
    time::{
        Duration,
        Instant, //
    },
};

use super::watch::{
    Kind,
    POLL_INTERVAL,
    Watcher, //
};
use crate::{
    args::{
        BuildArgs,
        LockMode, //
    },
    build,
    cards::Card,
    cargo::{
        Cargo,
        Output, //
    },
    env,
    generate::Flavour,
    http::{
        self,
        Limits, //
    },
    project::{
        self,
        Project, //
    },
    report::{
        self,
        CliError, //
    },
};

/// The quiet after the last change of a burst before a build in the web role.
pub const WEB_QUIET: Duration = Duration::from_millis(200);

/// The quiet in the worker role.
pub const WORKER_QUIET: Duration = Duration::from_millis(150);

/// The time between two probes of the readiness.
pub const PROBE_INTERVAL: Duration = Duration::from_millis(100);

/// The time the readiness may take before the supervisor stops probing.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(30);

/// The time one probe may take to connect and to be answered.
const PROBE_LIMIT: Duration = Duration::from_secs(1);

/// The body of a probe.
const PROBE_BODY: &[u8] = br#"{"query":"{__typename}"}"#;

/// The lines of the box the first readiness prints.
const BOX_LINES: [&str; 3] = [
    "Aosabi is up and running.",
    "",
    "Press Ctrl + C to stop the server.",
];

/// The line the worker role prints once its first child runs.
const WORKER_LINE: &str = "[dev] worker started, watching src for changes";

/// The line before every build after the first.
const REBUILDING_LINE: &str = "[dev] rebuilding";

/// The line after a rebuild that succeeded, before the restart.
const RESTARTED_LINE: &str = "[dev] restarted";

/// The line of a failed build while a child runs.
const FAILED_LINE: &str = "[dev] build failed, the previous server keeps running";

/// The line of a failed build while no child runs.
const FAILED_IDLE_LINE: &str = "[dev] build failed, watching for the next change";

/// What a supervisor is started with.
#[derive(Debug)]
pub struct Setup {
    /// The working directory the project is read from.
    pub cwd: PathBuf,
    /// The `--manifest-path` of the command line.
    pub manifest_path: Option<PathBuf>,
    /// The lock flags of the command.
    pub lock: LockMode,
    /// The cargo every build runs.
    pub cargo: Cargo,
    /// The project, read again after a burst of the kind `config`.
    pub project: Project,
    /// The card of every build, `server`.
    pub card: &'static Card,
    /// The port the child binds and the probe reads.
    pub port: u16,
    /// Whether the child runs in the worker role.
    pub worker: bool,
    /// Whether a failure prints its detail whole.
    pub verbose: bool,
}

/// The supervisor of the child of `cargo aosabi dev`: the project, the
/// port, the role and the child.
#[derive(Debug)]
pub struct Supervisor {
    /// What the supervisor was started with, the project as last read.
    setup: Setup,
    /// The running child, `None` before the first good build and after the
    /// child exited.
    child: Option<Child>,
    /// Whether the box or the worker line was printed.
    announced: bool,
}

impl Supervisor {
    /// Returns the supervisor of `setup`, with no child yet.
    #[must_use]
    pub fn new(setup: Setup) -> Self {
        Self {
            setup,
            child: None,
            announced: false,
        }
    }

    /// Runs the first build, then polls `watcher` and rebuilds after every
    /// burst of changes until the process ends.
    ///
    /// It returns only when the root of the project can no longer be read,
    /// with [`ExitCode::FAILURE`].
    pub fn run(&mut self, watcher: &mut Watcher) -> ExitCode {
        self.cycle(None);
        let mut pending: Option<Kind> = None;
        let mut last_change = Instant::now();
        loop {
            thread::sleep(POLL_INTERVAL);
            self.reap();
            let changes = match watcher.poll() {
                Ok(changes) => changes,
                Err(error) => {
                    self.stop();
                    report::report_failure(
                        &CliError::new(format!(
                            "the project root {} can no longer be read: {error}",
                            self.setup.project.root.display()
                        )),
                        self.setup.verbose,
                    );
                    return ExitCode::FAILURE;
                }
            };
            for (path, kind) in changes {
                let shown = path.strip_prefix(&self.setup.project.root).unwrap_or(&path);
                eprintln!("[dev] {} reload \u{2014} {}", kind.name(), shown.display());
                pending = Some(pending.map_or(kind, |known| known.max(kind)));
                last_change = Instant::now();
            }
            if let Some(kind) = pending
                && last_change.elapsed() >= self.quiet()
            {
                pending = None;
                self.cycle(Some(kind));
            }
        }
    }

    /// Returns the quiet of the role.
    fn quiet(&self) -> Duration {
        if self.setup.worker {
            WORKER_QUIET
        } else {
            WEB_QUIET
        }
    }

    /// Builds and, on success, restarts the child: the first build when
    /// `burst` is `None`, else a rebuild after a burst whose highest kind is
    /// `burst`.
    fn cycle(&mut self, burst: Option<Kind>) {
        if burst.is_some() {
            eprintln!("{REBUILDING_LINE}");
        }
        let built = self.reload(burst).and_then(|()| self.build());
        let binary = match built {
            Ok(binary) => binary,
            Err(error) => {
                if !build::is_shown(&error) {
                    report::report_failure(&error, self.setup.verbose);
                }
                if self.child.is_some() {
                    eprintln!("{FAILED_LINE}");
                } else {
                    eprintln!("{FAILED_IDLE_LINE}");
                }
                return;
            }
        };
        if burst.is_some() {
            eprintln!("{RESTARTED_LINE}");
        }
        self.stop();
        match self.spawn(&binary) {
            Ok(child) => self.child = Some(child),
            Err(error) => {
                report::report_failure(
                    &CliError::new(format!(
                        "the server {} could not start: {error}",
                        binary.display()
                    )),
                    self.setup.verbose,
                );
                return;
            }
        }
        if self.setup.worker {
            if !self.announced {
                self.announced = true;
                eprintln!("{WORKER_LINE}");
            }
        } else {
            self.probe();
        }
    }

    /// Reads the project again after a burst of the kind `config`.
    fn reload(&mut self, burst: Option<Kind>) -> Result<(), CliError> {
        if burst == Some(Kind::Config) {
            self.setup.project = project::load(
                &self.setup.cwd,
                self.setup.manifest_path.as_deref(),
                self.setup.lock,
                &self.setup.cargo,
            )?;
        }
        Ok(())
    }

    /// Runs steps 3 to 7 of `build` for the card and answers the binary.
    fn build(&self) -> Result<PathBuf, CliError> {
        let args = BuildArgs {
            device: None,
            include: Vec::new(),
            release: false,
            lock: self.setup.lock,
        };
        let outcome = build::run(
            &self.setup.project,
            self.setup.card,
            &args,
            Flavour::Build,
            Output::Inherit,
            &self.setup.cargo,
        )?;
        outcome.binary.ok_or_else(|| {
            CliError::new(format!(
                "the card {} built no binary to serve",
                self.setup.card.name
            ))
        })
    }

    /// Starts `binary` as the child in the project root.
    fn spawn(&self, binary: &Path) -> io::Result<Child> {
        let port = self.setup.port.to_string();
        let mut pairs: Vec<(OsString, OsString)> = self.setup.cargo.env.clone();
        pairs.extend(env::both_names("NODE_ENV", OsStr::new("development")));
        pairs.extend(env::both_names("AOSABI_DEV", OsStr::new("1")));
        pairs.extend(env::both_names("PORT", OsStr::new(&port)));
        if self.setup.worker {
            pairs.extend(env::both_names("AOSABI_ROLE", OsStr::new("worker")));
        }
        Command::new(binary)
            .current_dir(&self.setup.project.root)
            .envs(pairs)
            .stdin(Stdio::null())
            .stdout(Stdio::from(io::stderr()))
            .stderr(Stdio::inherit())
            .spawn()
    }

    /// Probes the readiness of the child, and prints the box and the running
    /// line the first time it answers.
    fn probe(&mut self) {
        let url = format!("http://127.0.0.1:{}/graphql", self.setup.port);
        let limits = Limits {
            connect: PROBE_LIMIT,
            read: PROBE_LIMIT,
            ..Limits::default()
        };
        let deadline = Instant::now() + PROBE_TIMEOUT;
        loop {
            if self.reap() {
                return;
            }
            let answered = http::post_with(&url, "application/json", PROBE_BODY, limits)
                .is_ok_and(|response| response.status == 200);
            if answered {
                break;
            }
            if Instant::now() >= deadline {
                eprintln!(
                    "[dev] the server answered no probe within {} seconds",
                    PROBE_TIMEOUT.as_secs()
                );
                return;
            }
            thread::sleep(PROBE_INTERVAL);
        }
        if !self.announced {
            self.announced = true;
            eprint!("{}", frame(&BOX_LINES));
            eprintln!("Aosabi running at http://localhost:{}", self.setup.port);
        }
    }

    /// Reports a child that exited on its own, once, and answers whether one
    /// did.
    fn reap(&mut self) -> bool {
        let Some(child) = &mut self.child else {
            return false;
        };
        let Ok(Some(status)) = child.try_wait() else {
            return false;
        };
        self.child = None;
        let role = if self.setup.worker {
            "worker"
        } else {
            "server"
        };
        eprintln!(
            "[dev] the {role} exited with {}, and the next change starts it again",
            describe(status)
        );
        true
    }

    /// Ends the running child, if any.
    fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Returns `status` as the exit line names it.
fn describe(status: ExitStatus) -> String {
    status
        .code()
        .map_or_else(|| String::from("a signal"), |code| format!("status {code}"))
}

/// Returns the frame `consola.box` of consola 3.4.2 draws around `lines`: an
/// empty line, a rounded border one space in, a row of padding above and below,
/// two spaces on each side of the widest line, and an empty line.
fn frame(lines: &[&str]) -> String {
    let width = lines
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0)
        + 4;
    let rule = "\u{2500}".repeat(width);
    let blank = format!(" \u{2502}{}\u{2502}\n", " ".repeat(width));
    let mut out = format!("\n \u{256d}{rule}\u{256e}\n{blank}");
    for line in lines {
        let pad = width - 2 - line.chars().count();
        let _ = writeln!(out, " \u{2502}  {line}{}\u{2502}", " ".repeat(pad));
    }
    out.push_str(&blank);
    let _ = write!(out, " \u{2570}{rule}\u{256f}\n\n");
    out
}
